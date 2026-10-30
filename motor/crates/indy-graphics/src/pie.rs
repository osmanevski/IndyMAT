//! `pie`: one patch and one text label per slice, measured black-box in
//! Octave 11.3: slices start at 90° and run counter-clockwise; a slice of
//! fraction `f` is the centre plus `ceil(90·f)+1` evenly spaced arc points;
//! values summing to less than 1 draw a partial pie; exploded slices move
//! 0.1 along their mid angle (their labels do not); labels are the rounded
//! percentage at 1.2 × the radius, right-aligned left of the centre,
//! left-aligned right of it, centred at the vertical. Colours: `FaceColor
//! 'flat'` with colour data k (the colormap through CLim `[1 n]`). The axes
//! is invisible with limits `[-1.5 1.5]` and a square plot box.
//!
//! Contract: the 2-D viewer stretches an axes to its box, which is why
//! fixed-aspect patches fall back (`unsupported_patch`). For a pie the
//! record's `position` is the square plot box Octave and MATLAB actually
//! draw (centred in `Position`), so the stretch is exact and the slices are
//! round: a documented approximation (`get(ax,'Position')` is unchanged).

use crate::error::{GraphicsError, Result};
use crate::graphics::{Graphics, Opts};
use crate::model::{Handle, Kind, LabelRole, Mode, Paint, Patch, Text};
use crate::value::Array;

/// Percentage label of a fraction (Octave: rounded, `%d%%`).
pub fn pie_label(f: f64) -> String {
    format!("{}%", (100.0 * f).round() as i64)
}

impl Graphics {
    /// `pie(x, explode, labels)`; returns `[patch, text]` per slice.
    pub fn pie(
        &mut self,
        ax: Option<Handle>,
        x: &[f64],
        explode: Option<&[f64]>,
        labels: Option<&[String]>,
        opts: &Opts,
    ) -> Result<Vec<Handle>> {
        if x.is_empty() || x.iter().any(|v| !v.is_finite() || *v < 0.0) {
            return Err(GraphicsError::args(
                "pie",
                "all data in X must be finite and nonnegative",
            ));
        }
        if explode.is_some_and(|e| e.len() != x.len()) {
            return Err(GraphicsError::args(
                "pie",
                "EXPLODE must have one element per slice",
            ));
        }
        if labels.is_some_and(|l| l.len() != x.len()) {
            return Err(GraphicsError::args(
                "pie",
                "LABELS must have one entry per slice",
            ));
        }
        let total: f64 = x.iter().sum();
        if total == 0.0 {
            return Err(GraphicsError::args("pie", "the data must not all be zero"));
        }
        let scale = if total < 1.0 { 1.0 } else { total };
        let ax = self.new_plot(ax)?;
        {
            let a = self.axes_mut(ax)?;
            a.pie = true;
            for i in 0..2 {
                let p = a.axis_mut(i);
                p.lim = [-1.5, 1.5];
                p.lim_mode = Mode::Manual;
            }
            a.pbar = [1.0, 1.0, 1.0];
            a.pbar_mode = Mode::Manual;
        }
        self.obj_mut(ax)?.visible = false;
        let mut out = Vec::with_capacity(2 * x.len());
        let mut start = std::f64::consts::FRAC_PI_2;
        for (k, v) in x.iter().enumerate() {
            let f = v / scale;
            let end = start + 2.0 * std::f64::consts::PI * f;
            let mid = (start + end) / 2.0;
            let off = if explode.is_some_and(|e| e[k] != 0.0) {
                [0.1 * mid.cos(), 0.1 * mid.sin()]
            } else {
                [0.0, 0.0]
            };
            let n = (90.0 * f).ceil() as usize + 1;
            let mut px = vec![off[0]];
            let mut py = vec![off[1]];
            for i in 0..n {
                let t = if n == 1 {
                    start
                } else {
                    start + (end - start) * i as f64 / (n - 1) as f64
                };
                px.push(off[0] + t.cos());
                py.push(off[1] + t.sin());
            }
            let count = px.len();
            let mut v = px;
            v.extend(py);
            let patch = Patch {
                vertices: Array::matrix(count, 2, v),
                faces: Array::matrix(1, count, (1..=count).map(|i| i as f64).collect()),
                fvcdata: Array::scalar((k + 1) as f64),
                face_color: Paint::Flat,
                edge_color: Paint::Rgb([0.0, 0.0, 0.0]),
                cdata_direct: false,
                line_style: "-".into(),
                line_width: 0.5,
                marker: "none".into(),
                face_alpha: 1.0,
                edge_alpha: 1.0,
                display_name: String::new(),
            };
            out.push(self.add(ax, Kind::Patch(patch)));
            let mut t = Text::new(LabelRole::Free);
            t.lines = vec![match labels {
                Some(l) => l[k].clone(),
                None => pie_label(f),
            }];
            t.position = [1.2 * mid.cos(), 1.2 * mid.sin(), 0.0];
            let c = mid.cos();
            t.horizontal_alignment = if c.abs() < 1e-10 {
                "center"
            } else if c < 0.0 {
                "right"
            } else {
                "left"
            }
            .into();
            out.push(self.add(ax, Kind::Text(t)));
            start = end;
        }
        self.apply_opts(&out, opts)?;
        Ok(out)
    }
}

/// The plot box a pie axes is drawn in: the largest square (in pixels)
/// centred in `position` of a figure of `size` pixels.
pub(crate) fn square_box(position: [f64; 4], size: [f64; 2]) -> [f64; 4] {
    let (w, h) = (position[2] * size[0], position[3] * size[1]);
    let s = w.min(h);
    let (nw, nh) = (s / size[0], s / size[1]);
    [
        position[0] + (position[2] - nw) / 2.0,
        position[1] + (position[3] - nh) / 2.0,
        nw,
        nh,
    ]
}
