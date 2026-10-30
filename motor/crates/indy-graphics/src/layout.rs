//! Automatic layout rules: limits, ticks, tick labels, camera, subplot and
//! colorbar geometry.
//!
//! Every rule here was fitted black-box to values measured from Octave 11.3
//! (fltk) in `tests/oracle/measure.json`; see `motor/docs/Grafik.md` for the
//! evidence and the known mismatches. No Octave source was consulted.

use crate::model::{GridSlot, LabelRole, Text};

/// Tick spacing for a range: Lewart's SCALE1 rule with five intervals
/// (published algorithm, CACM 1973): the step is 1, 2, 5 or 10 times a power
/// of ten, chosen with geometric thresholds √2, √10, √50.
pub fn tick_step(range: f64) -> f64 {
    let dx = range / 5.0;
    if !(dx.is_finite() && dx > 0.0) {
        return 1.0;
    }
    let scale = 10f64.powi(dx.log10().floor() as i32);
    let x = dx / scale;
    let factor = if x < 2f64.sqrt() {
        1.0
    } else if x < 10f64.sqrt() {
        2.0
    } else if x < 50f64.sqrt() {
        5.0
    } else {
        10.0
    };
    factor * scale
}

fn round_out(lo: f64, hi: f64, step: f64) -> [f64; 2] {
    [(lo / step).floor() * step, (hi / step).ceil() * step]
}

/// Expands an empty range the way Octave does for a constant data set.
fn expand_degenerate(v: f64) -> [f64; 2] {
    if v == 0.0 {
        [-1.0, 1.0]
    } else {
        [v - 0.1 * v.abs(), v + 0.1 * v.abs()]
    }
}

/// Automatic limits and ticks for a linear axis whose data spans `range`
/// (`None` when the axis holds no finite data: `[0, 1]`).
///
/// Two passes: round the data outward to the step of its range, then round
/// again to the step of the rounded range; ticks use the second step.
pub fn auto_limits(range: Option<[f64; 2]>) -> ([f64; 2], Vec<f64>) {
    let [lo, hi] = match range {
        None => [0.0, 1.0],
        Some([a, b]) if a == b => expand_degenerate(a),
        Some(r) => r,
    };
    let first = round_out(lo, hi, tick_step(hi - lo));
    let step = tick_step(first[1] - first[0]);
    let lim = round_out(first[0], first[1], step);
    (lim, ticks_with_step(lim, step))
}

/// Ticks for fixed (manual, tight or stretched) limits: one pass.
pub fn ticks_for(lim: [f64; 2]) -> Vec<f64> {
    ticks_with_step(lim, tick_step(lim[1] - lim[0]))
}

fn ticks_with_step(lim: [f64; 2], step: f64) -> Vec<f64> {
    let a = (lim[0] / step).ceil();
    let b = (lim[1] / step).floor();
    if !(a.is_finite() && b.is_finite()) || b - a > 4096.0 {
        return Vec::new();
    }
    (a as i64..=b as i64).map(|k| k as f64 * step).collect()
}

/// Ticks of a logarithmic axis: integer powers of ten inside the limits.
pub fn log_ticks(lim: [f64; 2]) -> Vec<f64> {
    if !(lim[0] > 0.0 && lim[1] > lim[0]) {
        return Vec::new();
    }
    let a = lim[0].log10().ceil() as i32;
    let b = lim[1].log10().floor() as i32;
    (a..=b.min(a + 4096)).map(|k| 10f64.powi(k)).collect()
}

/// Automatic log-axis limits: decades enclosing the positive data.
pub fn auto_log_limits(range: Option<[f64; 2]>) -> [f64; 2] {
    match range {
        Some([lo, hi]) if lo > 0.0 && hi > lo => [
            10f64.powi(lo.log10().floor() as i32),
            10f64.powi(hi.log10().ceil() as i32),
        ],
        Some([v, _]) if v > 0.0 => [
            10f64.powi(v.log10().floor() as i32),
            10f64.powi(v.log10().floor() as i32 + 1),
        ],
        _ => [1.0, 10.0],
    }
}

/// C `printf("%.{precision}g")`.
pub fn format_g(v: f64, precision: usize) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Inf".into() } else { "-Inf".into() };
    }
    if v == 0.0 {
        return "0".into();
    }
    let p = precision.max(1);
    // Rust's `{:e}` rounds the exact binary value, like glibc.
    let sci = format!("{:.*e}", p - 1, v);
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let strip = |s: &str| -> String {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s.to_string()
        }
    };
    if exp < -4 || exp >= p as i32 {
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{}e{sign}{:02}", strip(mantissa), exp.abs())
    } else {
        let decimals = (p as i32 - 1 - exp).max(0) as usize;
        strip(&format!("{v:.decimals$}"))
    }
}

/// Automatic tick label: `%.5g` for |v| >= 1, `%.4g` below (2194 of 2195
/// measured labels; the miss is `100005` → Octave `1.0000e+05`).
pub fn tick_label(v: f64) -> String {
    format_g(v, if v.abs() >= 1.0 { 5 } else { 4 })
}

/// Log-axis tick label: TeX `10^{k}` (measured with semilogx/loglog).
pub fn log_tick_label(v: f64) -> String {
    format!("10^{{{}}}", v.log10().round() as i64)
}

/// Default text object for an axes label or title.
pub fn label_text(role: LabelRole) -> Text {
    let mut t = Text::new(role);
    t.horizontal_alignment = "center".into();
    match role {
        LabelRole::XLabel => t.vertical_alignment = "top".into(),
        LabelRole::YLabel => {
            t.vertical_alignment = "bottom".into();
            t.rotation = 90.0;
        }
        LabelRole::ZLabel => t.vertical_alignment = "bottom".into(),
        LabelRole::Title => {
            t.vertical_alignment = "bottom".into();
            t.font_size = 11.0;
            t.font_weight = "bold".into();
        }
        // IndyMAT's own sgtitle (octave/compat/sgtitle.m, measured in
        // Octave): a full-width top-aligned box, 15 pt, normal weight.
        LabelRole::SuperTitle => {
            t.vertical_alignment = "top".into();
            t.units = "normalized".into();
            t.font_size = 15.0;
            t.margin = 5.0;
        }
        LabelRole::Free => {}
    }
    t
}

/// Measured: Octave keeps the zlabel text `Visible=off` even in 3-D views
/// (`plot3(...); zlabel('Z')`), so the extractor exports it as `""`.
pub fn label_visible(role: LabelRole) -> bool {
    role != LabelRole::ZLabel
}

/// Default figure size in pixels; subplot gaps depend on it.
pub const FIGURE_SIZE: [f64; 2] = [560.0, 420.0];

/// Subplot plot box. Fitted to the measured grids (1..4 rows/columns, two
/// figure sizes): the gap between cells is `21.25 px / figure size + 0.2/n`
/// and the cells fill the default box `[0.13 0.11 0.775 0.815]`.
pub fn subplot_position_sized(slot: &GridSlot, figure: [f64; 2]) -> [f64; 4] {
    let axis = |n: usize, start: f64, extent: f64, pixels: f64| {
        let gap = 21.25 / pixels + 0.2 / n as f64;
        let pitch = (extent + gap) / n as f64;
        (pitch, pitch - gap, start)
    };
    let (px, w, x0) = axis(slot.cols, 0.13, 0.775, figure[0]);
    let (py, h, y0) = axis(slot.rows, 0.11, 0.815, figure[1]);
    let mut left = f64::INFINITY;
    let mut right = f64::NEG_INFINITY;
    let mut bottom = f64::INFINITY;
    let mut top = f64::NEG_INFINITY;
    for &cell in &slot.cells {
        let col = (cell - 1) % slot.cols;
        let row_from_top = (cell - 1) / slot.cols;
        let row = slot.rows - 1 - row_from_top;
        let l = x0 + col as f64 * px;
        let b = y0 + row as f64 * py;
        left = left.min(l);
        right = right.max(l + w);
        bottom = bottom.min(b);
        top = top.max(b + h);
    }
    [left, bottom, right - left, top - bottom]
}

pub fn subplot_position(slot: &GridSlot) -> [f64; 4] {
    subplot_position_sized(slot, FIGURE_SIZE)
}

/// `tiledlayout` does not exist in Octave 11.3, so nothing could be
/// measured; tiles reuse the subplot grid (an explicit assumption).
pub fn tile_position(rows: usize, cols: usize, tile: usize) -> [f64; 4] {
    subplot_position(&GridSlot {
        rows,
        cols,
        cells: vec![tile],
    })
}

pub fn overlaps(a: &[f64; 4], b: &[f64; 4]) -> bool {
    let eps = 1e-9;
    a[0] < b[0] + b[2] - eps
        && b[0] < a[0] + a[2] - eps
        && a[1] < b[1] + b[3] - eps
        && b[1] < a[1] + a[3] - eps
}

/// Colorbar placement for a peer axes at `pos`: returns (new peer position,
/// colorbar position, vertical). Fractions measured for all eight locations.
pub fn colorbar_geometry(location: &str, pos: [f64; 4]) -> Option<([f64; 4], [f64; 4], bool)> {
    let [x, y, w, h] = pos;
    Some(match location {
        "eastoutside" => (
            [x, y, 0.8 * w, h],
            [x + 0.8 * w + 0.1 * w, y, 0.06 * w, h],
            true,
        ),
        "westoutside" => ([x + 0.2 * w, y, 0.8 * w, h], [x, y, 0.06 * w, h], true),
        "northoutside" => (
            [x, y, w, 0.8 * h],
            [x, y + 0.8 * h + 0.1 * h, w, 0.06 * h],
            false,
        ),
        "southoutside" => ([x, y + 0.2 * h, w, 0.8 * h], [x, y, w, 0.06 * h], false),
        "east" => (
            pos,
            [x + w - 0.1 * w, y + 0.05 * h, 0.054 * w, 0.9 * h],
            true,
        ),
        "west" => (pos, [x + 0.05 * w, y + 0.05 * h, 0.054 * w, 0.9 * h], true),
        "north" => (pos, [x + 0.05 * w, y + 0.9 * h, 0.9 * w, 0.054 * h], false),
        "south" => (pos, [x + 0.05 * w, y + 0.05 * h, 0.9 * w, 0.054 * h], false),
        _ => return None,
    })
}

/// Camera of an axes with orthographic automatic camera.
pub struct Camera {
    pub position: [f64; 3],
    pub target: [f64; 3],
    pub up: [f64; 3],
    pub view_angle: f64,
}

/// Automatic camera (fitted to 26 measured views incl. manual aspect, and
/// the view angle to 180 more):
/// with `dims = range ./ dar`, the camera sits `5·|dims|` from the box
/// centre along the view direction (scaled back by DAR), and the view
/// angle just fits the box's projected half extent.
/// `reverse` flags reversed axis directions: the camera term of a reversed
/// axis is negated (measured for x; applied to y/z by symmetry).
pub fn camera(
    lims: [[f64; 2]; 3],
    dar: [f64; 3],
    pbar: [f64; 3],
    view: [f64; 2],
    reverse: [bool; 3],
    viewport: f64,
) -> Camera {
    let target = [
        (lims[0][0] + lims[0][1]) / 2.0,
        (lims[1][0] + lims[1][1]) / 2.0,
        (lims[2][0] + lims[2][1]) / 2.0,
    ];
    let range = [
        lims[0][1] - lims[0][0],
        lims[1][1] - lims[1][0],
        lims[2][1] - lims[2][0],
    ];
    let dims = [range[0] / dar[0], range[1] / dar[1], range[2] / dar[2]];
    // Scaled norm: squaring ranges near 1e300 must not overflow.
    let m = dims.iter().fold(0.0f64, |a, d| a.max(d.abs()));
    let dist = if m > 0.0 {
        5.0 * m * dims.iter().map(|d| (d / m) * (d / m)).sum::<f64>().sqrt()
    } else {
        0.0
    };
    let (az, el) = (view[0].to_radians(), view[1].to_radians());
    let dir = [az.sin() * el.cos(), -az.cos() * el.cos(), el.sin()];
    let flip = |i: usize| if reverse[i] { -1.0 } else { 1.0 };
    let position = [
        target[0] + flip(0) * dist * dir[0] * dar[0],
        target[1] + flip(1) * dist * dir[1] * dar[1],
        target[2] + flip(2) * dist * dir[2] * dar[2],
    ];
    let top = view[1].abs() == 90.0;
    let up = if top {
        [
            -az.sin() * range[0] / pbar[0],
            az.cos() * range[1] / pbar[1],
            0.0,
        ]
    } else {
        [0.0, 0.0, 1.0]
    };
    // Screen axes in DAR-scaled space.
    let right = [az.cos(), az.sin(), 0.0];
    let screen_up = if top {
        [-az.sin(), az.cos(), 0.0]
    } else {
        let z_dot = dir[2];
        let v = [-z_dot * dir[0], -z_dot * dir[1], 1.0 - z_dot * dir[2]];
        let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [v[0] / n, v[1] / n, v[2] / n]
    };
    let half = |axis: [f64; 3]| {
        0.5 * (dims[0] * axis[0].abs() + dims[1] * axis[1].abs() + dims[2] * axis[2].abs())
    };
    // Measured (180 views × viewports × plot boxes, all exact): the angle
    // spans the horizontal half extent when the box is relatively wider
    // than the axes' pixel viewport (`viewport` = width/height), else the
    // vertical one; ties take the horizontal.
    let (h_half, v_half) = (half(right), half(screen_up));
    let extent = if !(viewport.is_finite() && viewport > 0.0) {
        h_half.max(v_half)
    } else if h_half >= v_half * viewport * (1.0 - 1e-9) {
        h_half
    } else {
        v_half
    };
    let view_angle = 2.0 * (extent / dist).atan().to_degrees();
    Camera {
        position,
        target,
        up,
        view_angle,
    }
}

/// Automatic data aspect ratio: `range ./ pbar`, normalised by its minimum.
pub fn auto_dar(lims: [[f64; 2]; 3], pbar: [f64; 3]) -> [f64; 3] {
    let r = [
        (lims[0][1] - lims[0][0]) / pbar[0],
        (lims[1][1] - lims[1][0]) / pbar[1],
        (lims[2][1] - lims[2][0]) / pbar[2],
    ];
    let m = r[0].min(r[1]).min(r[2]);
    if m > 0.0 && m.is_finite() {
        [r[0] / m, r[1] / m, r[2] / m]
    } else {
        [1.0, 1.0, 1.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printf_g() {
        assert_eq!(format_g(1.2000000000000002, 5), "1.2");
        assert_eq!(format_g(200000.0, 5), "2e+05");
        assert_eq!(format_g(99995.0, 5), "99995");
        assert_eq!(format_g(123400.0, 5), "1.234e+05");
        assert_eq!(format_g(0.0001, 5), "0.0001");
        assert_eq!(format_g(2e-6, 5), "2e-06");
        assert_eq!(format_g(-0.5, 5), "-0.5");
        assert_eq!(format_g(9.999999999999999e-06, 5), "1e-05");
        assert_eq!(format_g(0.00015, 5), "0.00015");
    }

    #[test]
    fn limits_two_pass() {
        assert_eq!(auto_limits(Some([0.0, 0.3])).0, [0.0, 0.35000000000000003]);
        assert_eq!(auto_limits(Some([0.0, 150.0])).0, [0.0, 200.0]);
        assert_eq!(
            auto_limits(Some([3.0, 5.0])).1,
            vec![3.0, 3.5, 4.0, 4.5, 5.0]
        );
        assert_eq!(auto_limits(Some([0.0, 0.0])).0, [-1.0, 1.0]);
        assert_eq!(auto_limits(None).0, [0.0, 1.0]);
    }

    #[test]
    fn subplot_grid() {
        let p = subplot_position(&GridSlot {
            rows: 2,
            cols: 2,
            cells: vec![2],
        });
        let want = [
            0.5864732145706635,
            0.5927976194275512,
            0.31852678542933666,
            0.3322023805724488,
        ];
        for (a, b) in p.iter().zip(want) {
            assert!((a - b).abs() < 1e-8, "{p:?}");
        }
    }
}
