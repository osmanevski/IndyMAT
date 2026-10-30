//! `sgtitle`: the figure-wide title.
//!
//! MATLAB's `sgtitle` makes a `subplottext` object above the subplot grid.
//! The Figure-Data contract carries it the way IndyMAT's own Octave
//! `sgtitle` (`octave/compat/sgtitle.m`) is exported: an invisible overlay
//! axes `[0 0 1 1]` drawn last, holding an empty `textbox` patch and the
//! `textbox` text with `figure_title: true`. Geometry measured in Octave
//! 11.3 with that compat function: the box is `1.2·FontSize·lines` pixels
//! high, 6 px below the figure top; subplot axes shrink their bottom and
//! height by `1 − (height + 2·gap)` (at most half the figure). Axes placed
//! with an explicit `Position` keep it. Not MATLAB-measured (Octave has no
//! `sgtitle`); the glyph extent is not measured here, the line-height
//! formula is the compat's lower bound.

use crate::emit::R;
use crate::error::{GraphicsError, Result};
use crate::graphics::{Graphics, Opts};
use crate::json::Json;
use crate::layout::label_text;
use crate::model::{Axes, Handle, Kind, LabelRole, Paint, Patch, Text};
use crate::scene::{Leaf, LeafObj, resolve};
use crate::value::Array;

const GAP_PX: f64 = 6.0;

fn line_count(t: &Text) -> usize {
    t.lines
        .iter()
        .map(|l| 1 + l.matches('\n').count())
        .sum::<usize>()
        .max(1)
}

/// `(bottom, height)` of the title box in normalized figure units.
fn title_box(t: &Text, figure_height: f64) -> (f64, f64) {
    let fh = figure_height.max(1.0);
    let height = t.font_size * 1.2 * line_count(t) as f64 / fh;
    ((1.0 - GAP_PX / fh - height).max(0.0), height)
}

impl Graphics {
    /// `sgtitle(fig, txt, opts)`: replaces the figure's previous title.
    pub fn sgtitle(
        &mut self,
        fig: Option<Handle>,
        lines: &[String],
        opts: &Opts,
    ) -> Result<Handle> {
        let fig = match fig {
            Some(f) => {
                self.figure_ref(f)?;
                f
            }
            None => self.gcf(),
        };
        let old: Vec<Handle> = self.sgtitle_handles(fig);
        let mut t = label_text(LabelRole::SuperTitle);
        t.lines = lines.to_vec();
        let fh = self.figure_ref(fig)?.position[3];
        t.position = [0.5, 1.0 - GAP_PX / fh.max(1.0), 0.0];
        let h = self.add(fig, Kind::Text(t));
        if let Err(e) = self.apply_opts(&[h], opts) {
            self.delete(h)?;
            return Err(e);
        }
        for o in old {
            self.delete(o)?;
        }
        let explicit = opts
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("position") || k.eq_ignore_ascii_case("units"));
        if !explicit {
            self.sgtitle_layout(fig, h)?;
        }
        Ok(h)
    }

    fn sgtitle_handles(&self, fig: Handle) -> Vec<Handle> {
        let Ok(f) = self.obj(fig) else {
            return Vec::new();
        };
        f.children
            .iter()
            .copied()
            .filter(|h| {
                matches!(self.objects.get(h).map(|o| &o.kind),
                    Some(Kind::Text(t)) if t.role == LabelRole::SuperTitle)
            })
            .collect()
    }

    /// Reserves room for the title in the automatic subplot/tile axes.
    fn sgtitle_layout(&mut self, fig: Handle, h: Handle) -> Result<()> {
        let fh = self.figure_ref(fig)?.position[3];
        let t = match &self.obj(h)?.kind {
            Kind::Text(t) => t.clone(),
            _ => return Err(GraphicsError::invalid_handle()),
        };
        let (_, height) = title_box(&t, fh);
        let reserve = (height + 2.0 * GAP_PX / fh.max(1.0)).min(0.5);
        let axes: Vec<Handle> = self.obj(fig)?.children.clone();
        for ax in axes {
            let Ok(a) = self.axes_mut(ax) else { continue };
            if a.slot.is_none() {
                continue;
            }
            let base = *a.sg_base.get_or_insert(a.position);
            let p = [
                base[0],
                base[1] * (1.0 - reserve),
                base[2],
                base[3] * (1.0 - reserve),
            ];
            a.position = p;
            if a.colorbar.is_none() {
                a.base_position = p;
            }
        }
        Ok(())
    }

    /// The visible title text of a figure.
    pub(crate) fn sgtitle_of(&self, fig: Handle) -> Option<&Text> {
        let f = self.obj(fig).ok()?;
        f.children
            .iter()
            .rev()
            .find_map(|h| match self.objects.get(h) {
                Some(o) if o.visible => match &o.kind {
                    Kind::Text(t) if t.role == LabelRole::SuperTitle => Some(t),
                    _ => None,
                },
                _ => None,
            })
    }

    /// The overlay axes record carrying the figure title (record `id`).
    pub(crate) fn sgtitle_overlay(&self, fig: Handle, id: usize) -> R<Option<Json>> {
        let Some(t) = self.sgtitle_of(fig) else {
            return Ok(None);
        };
        let Ok(figure) = self.figure_ref(fig) else {
            return Ok(None);
        };
        let (bottom, height) = title_box(t, figure.position[3]);
        let top = bottom + height;
        let patch = Patch {
            vertices: Array::matrix(4, 2, vec![0.0, 1.0, 1.0, 0.0, bottom, bottom, top, top]),
            faces: Array::matrix(1, 4, vec![1.0, 2.0, 3.0, 4.0]),
            fvcdata: Array::empty(),
            face_color: Paint::None,
            edge_color: Paint::None,
            cdata_direct: false,
            line_style: "-".into(),
            line_width: 0.5,
            marker: "none".into(),
            face_alpha: 1.0,
            edge_alpha: 1.0,
            display_name: String::new(),
        };
        let mut text = t.clone();
        // The overlay's data units are normalized figure units.
        text.units = "data".into();
        let leaf = |obj| Leaf {
            role: Some("textbox"),
            group_name: None,
            group: None,
            unit_box: [false; 2],
            obj,
        };
        let leaves = vec![leaf(LeafObj::Patch(patch, None)), leaf(LeafObj::Text(text))];
        let mut a = Axes::new([Handle::ROOT; 4]);
        a.position = [0.0, 0.0, 1.0, 1.0];
        a.base_position = a.position;
        for i in 0..2 {
            a.axis_mut(i).lim_mode = crate::model::Mode::Manual;
        }
        let st = resolve(&a, &leaves, figure.colormap.clone(), figure.position);
        let (mut rec, _) = self.axes_record_of(&a, st, false, id, &leaves)?;
        if let Some(Json::Arr(series)) = rec.get_mut("series")
            && let Some(s) = series.get_mut(1)
        {
            s.set("role", "textbox");
            s.set("figure_title", true);
        }
        Ok(Some(rec))
    }
}
