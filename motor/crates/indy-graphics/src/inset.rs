//! Subplot alignment around labels and titles, and the drawn plot box.
//!
//! Measured black-box in Octave 11.3 as IndyMAT runs it (`octave
//! --no-gui`, Qt on the native macOS platform; `TightInset`/`Position` of
//! subplot axes; no Octave source read):
//!
//! * Each subplot cell has an outer box: the figure edge on the grid's
//!   border, otherwise half the 21.25 px cell gap beyond the neighbouring
//!   axes (`1 - 0.925`, `0.4422 + 0.0253` at 560×420).
//! * An axes needs `10 px + title` above, `16.51 px + xlabel` below and
//!   `17.5 px + ylabel` on the left. Those two tick-label bases stay fixed
//!   while the axes font size is the default 10 (Octave does not re-measure
//!   the tick labels then); at other sizes they follow the size and the
//!   widest y tick label.
//! * When one of them does not fit between the axes and its outer box,
//!   every subplot axes of the figure gives up the largest overflow on that
//!   side, so the grid stays aligned (`subplot(2,1,2); title('T')` shrinks
//!   both rows; a title on the top row has room and changes nothing).
//!
//! Text heights use a fitted glyph model of the ink box (half-pixel
//! steps): `(top + bottom)·size + 0.5` px rounded to 0.5, with `top` the
//! tallest character class and `bottom` the deepest, and 1.17·size per
//! extra line. It reproduces the measured extents of 11 strings at 5 sizes
//! to ±0.5 px. (fltk and offscreen Qt measure about 1 px taller; the oracle
//! cases of this module are measured with `GRAFIK_TOOLKIT=qtnative`.)

use std::collections::HashMap;

use crate::graphics::Graphics;
use crate::layout;
use crate::model::{Handle, Kind, LabelRole, Mode, Text};

/// Half the subplot cell gap in pixels (the gap is 21.25 px + 0.2/n).
const HALF_GAP_PX: f64 = 10.625;
const TOP_BASE_PX: f64 = 10.0;
/// Tick-label insets at the default font size (not re-measured by Octave).
const BOTTOM_DEFAULT_PX: f64 = 16.51;
const LEFT_DEFAULT_PX: f64 = 17.5;

fn half_px(v: f64) -> f64 {
    (2.0 * v).round() / 2.0
}

/// Ink height in pixels of one line of text at `size` points.
fn line_ink(line: &str, size: f64) -> f64 {
    let mut top: f64 = 0.0;
    let mut bottom: f64 = 0.0;
    for ch in line.chars() {
        if ch.is_whitespace() {
            continue;
        }
        let t = if "İÖÜĞŞÇÂÎÛÄÅÉÈÊ".contains(ch) {
            0.88
        } else if ch.is_uppercase() || ch.is_ascii_digit() {
            0.73
        } else if "acegmnopqrsuvwxyzıçş·+-=:<>~*".contains(ch) {
            0.535
        } else {
            0.76
        };
        top = top.max(t);
        let b = if "gjpqyçşŞÇ(),;[]_|{}".contains(ch) {
            0.215
        } else if "ceosaugdbpöCGOQS".contains(ch) {
            0.03
        } else {
            0.0
        };
        bottom = bottom.max(b);
    }
    if top == 0.0 {
        return 0.0;
    }
    half_px((top + bottom) * size + 0.5)
}

/// Line pitch of multi-line labels.
fn line_pitch(size: f64) -> f64 {
    half_px(1.17 * size)
}

/// Ink height of a (multi-line) label, 0 when it shows nothing.
pub(crate) fn text_height(lines: &[String], size: f64) -> f64 {
    let shown: Vec<&String> = lines.iter().filter(|l| !l.trim().is_empty()).collect();
    if shown.is_empty() {
        return 0.0;
    }
    let ink = shown.iter().map(|l| line_ink(l, size)).fold(0.0, f64::max);
    ink + (lines.len() as f64 - 1.0) * line_pitch(size)
}

/// Width in pixels of a y tick label (measured advances at 8–16 pt).
fn label_width(s: &str, size: f64) -> f64 {
    half_px(
        s.chars()
            .map(|c| match c {
                '0'..='9' | 'e' => 0.555,
                '.' | ',' => 0.25,
                '-' | '+' | '−' => 0.33,
                _ => 0.5,
            })
            .sum::<f64>()
            * size,
    )
}

impl Graphics {
    /// Font size Octave uses for an axes label: an explicit `FontSize` is
    /// absolute, the default follows 1.1 × the axes font size.
    fn label_size(&self, t: &Text, axes_font: f64) -> f64 {
        let default = if t.role == LabelRole::Title {
            11.0
        } else {
            10.0
        };
        if t.font_size == default {
            1.1 * axes_font
        } else {
            t.font_size
        }
    }

    fn label_ink(&self, h: Handle, axes_font: f64) -> f64 {
        match self.objects.get(&h).map(|o| (&o.kind, o.visible)) {
            Some((Kind::Text(t), true)) => text_height(&t.lines, self.label_size(t, axes_font)),
            _ => 0.0,
        }
    }

    /// The figure-wide subplot shrink `[left, bottom, right, top]`
    /// (normalized) for the axes `ax`, zero when it is not a subplot.
    pub(crate) fn subplot_inset(&self, ax: Handle) -> [f64; 4] {
        let Some(fig) = self.objects.get(&ax).map(|o| o.parent) else {
            return [0.0; 4];
        };
        if !self.is_aligned_subplot(fig, ax) {
            return [0.0; 4];
        }
        self.figure_insets(fig)
            .get(&ax)
            .copied()
            .unwrap_or([0.0; 4])
    }

    fn figure_size(&self, fig: Handle) -> [f64; 2] {
        self.figure_ref(fig)
            .map_or(layout::FIGURE_SIZE, |f| [f.position[2], f.position[3]])
    }

    /// A subplot axes still at its grid cell (not moved by the user or by
    /// `sgtitle`).
    fn is_aligned_subplot(&self, fig: Handle, ax: Handle) -> bool {
        let Ok(a) = self.axes_ref(ax) else {
            return false;
        };
        let Some(slot) = &a.slot else { return false };
        if a.sg_base.is_some() {
            return false;
        }
        let cell = layout::subplot_position_sized(slot, self.figure_size(fig));
        a.base_position
            .iter()
            .zip(&cell)
            .all(|(p, c)| (p - c).abs() < 1e-12)
    }

    fn figure_insets(&self, fig: Handle) -> HashMap<Handle, [f64; 4]> {
        let size = self.figure_size(fig);
        let Some(children) = self.objects.get(&fig).map(|o| o.children.clone()) else {
            return HashMap::new();
        };
        let members: Vec<Handle> = children
            .into_iter()
            .filter(|&h| self.is_aligned_subplot(fig, h))
            .collect();
        let mut over = [0.0f64; 4];
        for &h in &members {
            let Ok(a) = self.axes_ref(h) else { continue };
            let Some(slot) = &a.slot else { continue };
            if !self.objects.get(&h).is_some_and(|o| o.visible) {
                continue;
            }
            let p = a.base_position;
            let gap = |n: usize, px: f64| 21.25 / px + 0.2 / n as f64;
            let cols: Vec<usize> = slot.cells.iter().map(|c| (c - 1) % slot.cols).collect();
            let rows: Vec<usize> = slot.cells.iter().map(|c| (c - 1) / slot.cols).collect();
            let left_neighbour = cols.iter().all(|&c| c > 0);
            let right_neighbour = cols.iter().all(|&c| c + 1 < slot.cols);
            let row_above = rows.iter().all(|&r| r > 0);
            let row_below = rows.iter().all(|&r| r + 1 < slot.rows);
            let half = [HALF_GAP_PX / size[0], HALF_GAP_PX / size[1]];
            let outer_left = if left_neighbour {
                p[0] - gap(slot.cols, size[0]) + half[0]
            } else {
                0.0
            };
            let outer_right = if right_neighbour {
                p[0] + p[2] + half[0]
            } else {
                1.0
            };
            let outer_bottom = if row_below {
                p[1] - gap(slot.rows, size[1]) + half[1]
            } else {
                0.0
            };
            let outer_top = if row_above {
                p[1] + p[3] + half[1]
            } else {
                1.0
            };
            let fs = a.font_size;
            let mut need = [0.0, 0.0, 0.0, TOP_BASE_PX + self.label_ink(a.title, fs)];
            if a.is_2d_view() {
                let (bottom, left) = if fs == 10.0 {
                    (BOTTOM_DEFAULT_PX, LEFT_DEFAULT_PX)
                } else {
                    let widest = self
                        .axes_state(h)
                        .map(|st| {
                            st.ticklabel[1]
                                .iter()
                                .map(|s| label_width(s, fs))
                                .fold(0.0, f64::max)
                        })
                        .unwrap_or(0.0);
                    (1.4375 * fs + 5.98, 5.0 + 0.5 * fs + widest)
                };
                need[1] = bottom + self.label_ink(a.x.label, fs);
                need[0] = left + self.label_ink(a.y.label, fs);
            }
            let room = [
                (p[0] - outer_left) * size[0],
                (p[1] - outer_bottom) * size[1],
                (outer_right - p[0] - p[2]) * size[0],
                (outer_top - p[1] - p[3]) * size[1],
            ];
            for k in 0..4 {
                over[k] = over[k].max(need[k] - room[k]);
            }
        }
        let shrink = [
            over[0].max(0.0) / size[0],
            over[1].max(0.0) / size[1],
            over[2].max(0.0) / size[0],
            over[3].max(0.0) / size[1],
        ];
        members.into_iter().map(|h| (h, shrink)).collect()
    }

    /// The grid position (`base_position`) after the subplot shrink.
    pub(crate) fn inset_base(&self, ax: Handle) -> [f64; 4] {
        let Ok(a) = self.axes_ref(ax) else {
            return [0.0, 0.0, 1.0, 1.0];
        };
        shrink(a.base_position, self.subplot_inset(ax))
    }

    /// `position` after the subplot shrink of its axes.
    pub(crate) fn inset_position(&self, ax: Handle, position: [f64; 4]) -> [f64; 4] {
        let s = self.subplot_inset(ax);
        if s == [0.0; 4] {
            return position;
        }
        let Ok(a) = self.axes_ref(ax) else {
            return position;
        };
        let shrunk = shrink(a.base_position, s);
        if let Some(cb) = a.colorbar
            && let Some(Kind::Colorbar(c)) = self.objects.get(&cb).map(|o| &o.kind)
            && let Some((peer, _, _)) = layout::colorbar_geometry(&c.location, shrunk)
        {
            return peer;
        }
        if position == a.base_position {
            shrunk
        } else {
            shrink(position, s)
        }
    }

    /// The drawn 2-D plot box of an axes inside `position`: with a fixed
    /// aspect (`axis equal`, `daspect`, `pbaspect`, `axis image`) the box
    /// keeps the PBAR x:y ratio, centred; otherwise the whole position.
    pub(crate) fn plot_box(&self, ax: Handle, position: [f64; 4]) -> [f64; 4] {
        let (Ok(a), Some(st)) = (self.axes_ref(ax), self.axes_state(ax)) else {
            return position;
        };
        if st.dimension != 2 || (a.dar_mode == Mode::Auto && a.pbar_mode == Mode::Auto) {
            return position;
        }
        let size = st.figure_size;
        let (w, h) = (position[2] * size[0], position[3] * size[1]);
        let ratio = st.pbar[0] / st.pbar[1];
        if !(ratio.is_finite() && ratio > 0.0 && w > 0.0 && h > 0.0) {
            return position;
        }
        let (bw, bh) = if w / h > ratio {
            (h * ratio, h)
        } else {
            (w, w / ratio)
        };
        let (nw, nh) = (bw / size[0], bh / size[1]);
        [
            position[0] + (position[2] - nw) / 2.0,
            position[1] + (position[3] - nh) / 2.0,
            nw,
            nh,
        ]
    }
}

fn shrink(p: [f64; 4], s: [f64; 4]) -> [f64; 4] {
    [
        p[0] + s[0],
        p[1] + s[1],
        p[2] - s[0] - s[2],
        p[3] - s[1] - s[3],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_title_heights() {
        // Octave 11.3 (native Qt) title extents (px) at 11 pt.
        for (s, want) in [
            ("T", 8.5),
            ("x", 6.5),
            ("g", 9.0),
            ("Tg", 11.0),
            ("İ", 10.0),
            ("İkinci dereceden sistem", 10.5),
            ("Faz portresi", 11.0),
            ("FFT · 5 Hz + 20 Hz", 8.5),
            ("A matrisi", 9.0),
            ("(a) ölçüm", 11.0),
        ] {
            assert_eq!(line_ink(s, 11.0), want, "{s}");
        }
        assert_eq!(text_height(&["T".into(), "U".into()], 11.0), 21.5);
        assert_eq!(text_height(&[String::new()], 11.0), 0.0);
    }
}
