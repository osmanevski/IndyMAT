//! Line-based charts: `errorbar`, `quiver`, `fplot` (the function line and
//! its adaptive sampler) and `polarplot`/`polar` (an approximation on a
//! cartesian axes, since the Figure-Data contract has no polar axes).
//!
//! Every object here draws as `line` records (plus `text` labels for the
//! polar grid). Octave 11.3 (octave-cli, fltk) was used as a black-box
//! oracle by reading the objects it creates; the tests in
//! `indy-graphics-builtins/tests/lane_a.rs` hold the measured numbers.
//! Where Octave and MATLAB differ, MATLAB's documentation is followed.
//!
//! * **errorbar**: the data line and one NaN-separated line of bars and
//!   caps, both with role `errorbar` (the extractor's layout for Octave's
//!   `__errplot__`). MATLAB caps are `CapSize` points wide whatever the
//!   zoom; the contract has no point-sized caps, so they are converted to
//!   data units at export with the axes' pixel box (figure `Position` ×
//!   axes `Position`, 1 pt = 4/3 px). Caps never count for the limits.
//! * **quiver**: shafts, heads and (with a LineSpec marker) base markers as
//!   role-`quiver` lines. Autoscale: spacing `range(x)/n`, `range(y)/m`
//!   (m×n the size of U; `sqrt(numel)` for vectors), arrows scaled by
//!   `AutoScaleFactor / max(|(u,v)| / |spacing|)`. Heads: length
//!   `L/3 · MaxHeadSize/0.2`, half-width a third of that (measured).
//! * **fplot**: see [`fplot_sample`]; a fresh axes takes the interval as
//!   manual x-limits (measured in Octave, documented for MATLAB).
//! * **polar**: an invisible axes with `axis equal` and tight limits; the
//!   grid (circles at the r ticks, spokes every 30°), angle labels and r
//!   labels are emitted after the data (`role:"polar_grid"`, text), so the
//!   viewer's legend, which indexes records, still names the data lines.
//!   Not a MATLAB `polaraxes`: `gca` is the cartesian axes, negative rho
//!   is reflected through the origin, there is no `thetalim`/`rlim`.

use crate::color::Rgb;
use crate::error::{GraphicsError, Result};
use crate::graphics::{Graphics, Opts};
use crate::layout;
use crate::linespec::{self, LineSpec, canonical_line_style, canonical_marker};
use crate::model::{
    ErrorBar, FunctionLine, Group, GroupKind, Handle, Kind, LabelRole, Line, Mode, Paint, Quiver,
    Scale, Text,
};
use crate::plots::PlotSeries;
use crate::props::parse_paint;
use crate::scene::{AxesState, Leaf, LeafObj};
use crate::value::{Array, PropValue, finite_range};

/// Kind tag of the errorbar bars line before export adds the caps.
const BARS: &str = "errorbar_bars";

/// Points to pixels (96 px per inch).
const PX_PER_POINT: f64 = 4.0 / 3.0;

fn unavailable(func: &str, what: &str) -> GraphicsError {
    GraphicsError::new(
        "IndyR:graphicsUnavailable",
        format!("{func}: {what} is not available in the Rust engine yet"),
    )
}

fn parse_spec(spec: &Option<String>, func: &str) -> Result<LineSpec> {
    match spec {
        None => Ok(LineSpec::default()),
        Some(s) => linespec::parse(s)?
            .ok_or_else(|| GraphicsError::args(func, format!("invalid LineSpec '{s}'"))),
    }
}

// ----- property helpers ------------------------------------------------------

fn vector(v: &PropValue, name: &str) -> Result<Vec<f64>> {
    v.numbers().ok_or_else(|| GraphicsError::bad_value(name))
}

fn scalar(v: &PropValue, name: &str) -> Result<f64> {
    match v.numbers() {
        Some(n) if n.len() == 1 && n[0].is_finite() => Ok(n[0]),
        _ => Err(GraphicsError::bad_value(name)),
    }
}

fn positive(v: &PropValue, name: &str) -> Result<f64> {
    match scalar(v, name)? {
        x if x > 0.0 => Ok(x),
        _ => Err(GraphicsError::bad_value(name)),
    }
}

fn nonnegative(v: &PropValue, name: &str) -> Result<f64> {
    match scalar(v, name)? {
        x if x >= 0.0 => Ok(x),
        _ => Err(GraphicsError::bad_value(name)),
    }
}

fn on_off(v: &PropValue, name: &str) -> Result<bool> {
    match v {
        PropValue::Bool(b) => Ok(*b),
        PropValue::Num(n) if *n == 0.0 || *n == 1.0 => Ok(*n == 1.0),
        PropValue::Str(s) => match s.to_ascii_lowercase().as_str() {
            "on" => Ok(true),
            "off" => Ok(false),
            _ => Err(GraphicsError::bad_value(name)),
        },
        _ => Err(GraphicsError::bad_value(name)),
    }
}

fn paint_value(p: Paint) -> PropValue {
    match p {
        Paint::Rgb(c) => PropValue::row(&c),
        Paint::None => PropValue::str("none"),
        Paint::Auto => PropValue::str("auto"),
        Paint::Flat => PropValue::str("flat"),
        Paint::Interp => PropValue::str("interp"),
    }
}

/// Line-style properties shared by the objects of this module.
fn set_style(l: &mut Line, key: &str, name: &str, v: &PropValue) -> Result<bool> {
    match key {
        "color" => match parse_paint(v, name, &[])? {
            Paint::Rgb(c) => l.color = c,
            _ => return Err(GraphicsError::bad_value(name)),
        },
        "linestyle" => {
            l.line_style = v
                .as_str()
                .and_then(canonical_line_style)
                .ok_or_else(|| GraphicsError::bad_value(name))?
                .to_string()
        }
        "linewidth" => l.line_width = positive(v, name)?,
        "marker" => {
            l.marker = v
                .as_str()
                .and_then(canonical_marker)
                .ok_or_else(|| GraphicsError::bad_value(name))?
                .to_string()
        }
        "markersize" => l.marker_size = positive(v, name)?,
        "markeredgecolor" => l.marker_edge = parse_paint(v, name, &["none", "auto"])?,
        "markerfacecolor" => l.marker_face = parse_paint(v, name, &["none", "auto"])?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn get_style(l: &Line, key: &str) -> Option<PropValue> {
    Some(match key {
        "color" => PropValue::row(&l.color),
        "linestyle" => PropValue::str(&l.line_style),
        "linewidth" => PropValue::Num(l.line_width),
        "marker" => PropValue::str(&l.marker),
        "markersize" => PropValue::Num(l.marker_size),
        "markeredgecolor" => paint_value(l.marker_edge),
        "markerfacecolor" => paint_value(l.marker_face),
        _ => return None,
    })
}

/// A delta vector: empty, or one value per point.
fn delta(v: &PropValue, name: &str, n: usize) -> Result<Vec<f64>> {
    let d = vector(v, name)?;
    if d.is_empty() || d.len() == n {
        Ok(d)
    } else if d.len() == 1 {
        Ok(vec![d[0]; n])
    } else {
        Err(GraphicsError::bad_value(name))
    }
}

pub(crate) fn set_errorbar(e: &mut ErrorBar, key: &str, name: &str, v: &PropValue) -> Result<()> {
    let n = e.line.x.len();
    match key {
        "xdata" => e.line.x = vector(v, name)?,
        "ydata" => e.line.y = vector(v, name)?,
        "ynegativedelta" | "ldata" => e.y_neg = delta(v, name, n)?,
        "ypositivedelta" | "udata" => e.y_pos = delta(v, name, n)?,
        "xnegativedelta" => e.x_neg = delta(v, name, n)?,
        "xpositivedelta" => e.x_pos = delta(v, name, n)?,
        "capsize" => e.cap_size = nonnegative(v, name)?,
        _ => {
            if !set_style(&mut e.line, key, name, v)? {
                return Err(GraphicsError::bad_property("errorbar", name));
            }
        }
    }
    Ok(())
}

pub(crate) fn get_errorbar(e: &ErrorBar, key: &str) -> Option<PropValue> {
    Some(match key {
        "xdata" => PropValue::row(&e.line.x),
        "ydata" => PropValue::row(&e.line.y),
        "ynegativedelta" | "ldata" => PropValue::row(&e.y_neg),
        "ypositivedelta" | "udata" => PropValue::row(&e.y_pos),
        "xnegativedelta" => PropValue::row(&e.x_neg),
        "xpositivedelta" => PropValue::row(&e.x_pos),
        "capsize" => PropValue::Num(e.cap_size),
        _ => get_style(&e.line, key)?,
    })
}

pub(crate) fn set_quiver(q: &mut Quiver, key: &str, name: &str, v: &PropValue) -> Result<()> {
    let n = q.line.x.len();
    let same = |d: Vec<f64>| {
        if d.len() == n {
            Ok(d)
        } else {
            Err(GraphicsError::bad_value(name))
        }
    };
    match key {
        "xdata" => q.line.x = same(vector(v, name)?)?,
        "ydata" => q.line.y = same(vector(v, name)?)?,
        "udata" => q.u = same(vector(v, name)?)?,
        "vdata" => q.v = same(vector(v, name)?)?,
        "autoscale" => q.auto_scale = on_off(v, name)?,
        "autoscalefactor" => q.auto_scale_factor = nonnegative(v, name)?,
        "maxheadsize" => q.max_head_size = nonnegative(v, name)?,
        "showarrowhead" => q.show_arrow_head = on_off(v, name)?,
        _ => {
            if !set_style(&mut q.line, key, name, v)? {
                return Err(GraphicsError::bad_property("quiver", name));
            }
        }
    }
    Ok(())
}

pub(crate) fn get_quiver(q: &Quiver, key: &str) -> Option<PropValue> {
    Some(match key {
        "xdata" => PropValue::row(&q.line.x),
        "ydata" => PropValue::row(&q.line.y),
        "udata" => PropValue::row(&q.u),
        "vdata" => PropValue::row(&q.v),
        "autoscale" => PropValue::on_off(q.auto_scale),
        "autoscalefactor" => PropValue::Num(q.auto_scale_factor),
        "maxheadsize" => PropValue::Num(q.max_head_size),
        "showarrowhead" => PropValue::on_off(q.show_arrow_head),
        _ => get_style(&q.line, key)?,
    })
}

pub(crate) fn set_function_line(
    f: &mut FunctionLine,
    key: &str,
    name: &str,
    v: &PropValue,
) -> Result<()> {
    let kind = if f.y_function.is_some() {
        "parameterizedfunctionline"
    } else {
        "functionline"
    };
    match key {
        "function" | "xfunction" | "yfunction" | "xrange" | "trange" | "meshdensity" | "xdata"
        | "ydata" => Err(unavailable(
            "set",
            &format!("changing '{name}' of an fplot line (it would need re-evaluation)"),
        )),
        _ => {
            if set_style(&mut f.line, key, name, v)? {
                Ok(())
            } else {
                Err(GraphicsError::bad_property(kind, name))
            }
        }
    }
}

pub(crate) fn get_function_line(f: &FunctionLine, key: &str) -> Option<PropValue> {
    let param = f.y_function.is_some();
    Some(match key {
        "xdata" => PropValue::row(&f.line.x),
        "ydata" => PropValue::row(&f.line.y),
        "function" if !param => PropValue::str(&f.function),
        "xfunction" if param => PropValue::str(&f.function),
        "yfunction" if param => PropValue::str(f.y_function.as_deref().unwrap_or("")),
        "xrange" if !param => PropValue::row(&f.range),
        "trange" if param => PropValue::row(&f.range),
        "meshdensity" => PropValue::Num(f.mesh_density),
        _ => get_style(&f.line, key)?,
    })
}

pub(crate) fn is_polar_grid(g: &Graphics, h: Handle) -> bool {
    matches!(
        g.objects.get(&h).map(|o| &o.kind),
        Some(Kind::Group(gr)) if matches!(gr.kind, GroupKind::PolarGrid)
    )
}

// ----- errorbar ----------------------------------------------------------------

/// Arguments of `errorbar`: Y (vector or matrix, one object per column),
/// optional X and the four deltas (absent = no bars in that direction).
#[derive(Debug, Clone, Default)]
pub struct ErrorBarArgs {
    pub x: Option<Array>,
    pub y: Array,
    pub y_neg: Option<Array>,
    pub y_pos: Option<Array>,
    pub x_neg: Option<Array>,
    pub x_pos: Option<Array>,
    pub spec: Option<String>,
}

/// Splits `a` like `y`: one vector per series (`y` vector: one series; `y`
/// matrix: its columns). A vector of the row count is shared by every
/// column.
fn split_like(a: &Array, y: &Array, what: &str) -> Result<Vec<Vec<f64>>> {
    let bad = || GraphicsError::args("errorbar", format!("{what} must be the same size as Y"));
    if y.is_vector() || y.is_empty() {
        if a.numel() == y.numel() {
            return Ok(vec![a.data.clone()]);
        }
        if a.numel() == 1 {
            return Ok(vec![vec![a.data[0]; y.numel()]]);
        }
        return Err(bad());
    }
    let (rows, cols) = (y.rows(), y.cols());
    if a.dims == y.dims {
        return Ok((0..cols).map(|c| a.column(c).to_vec()).collect());
    }
    if a.is_vector() && a.numel() == rows {
        return Ok(vec![a.data.clone(); cols]);
    }
    if a.numel() == 1 {
        return Ok(vec![vec![a.data[0]; rows]; cols]);
    }
    Err(bad())
}

/// The bars of one errorbar (without caps): NaN-separated segments, the
/// vertical ones first.
fn errorbar_bars(e: &ErrorBar) -> (Vec<f64>, Vec<f64>) {
    let (x, y) = (&e.line.x, &e.line.y);
    let n = x.len().min(y.len());
    let at = |d: &[f64], i: usize| d.get(i).copied().filter(|v| v.is_finite()).unwrap_or(0.0);
    let (mut bx, mut by) = (Vec::new(), Vec::new());
    if !(e.y_neg.is_empty() && e.y_pos.is_empty()) {
        for i in 0..n {
            let (lo, hi) = (at(&e.y_neg, i), at(&e.y_pos, i));
            if !(x[i].is_finite() && y[i].is_finite()) || (lo == 0.0 && hi == 0.0) {
                continue;
            }
            bx.extend([x[i], x[i], f64::NAN]);
            by.extend([y[i] - lo, y[i] + hi, f64::NAN]);
        }
    }
    if !(e.x_neg.is_empty() && e.x_pos.is_empty()) {
        for i in 0..n {
            let (lo, hi) = (at(&e.x_neg, i), at(&e.x_pos, i));
            if !(x[i].is_finite() && y[i].is_finite()) || (lo == 0.0 && hi == 0.0) {
                continue;
            }
            bx.extend([x[i] - lo, x[i] + hi, f64::NAN]);
            by.extend([y[i], y[i], f64::NAN]);
        }
    }
    (bx, by)
}

fn group_leaf(role: &'static str, name: &str, obj: LeafObj) -> Leaf {
    Leaf {
        role: Some(role),
        group_name: Some(name.to_string()),
        group: None,
        unit_box: [false; 2],
        obj,
    }
}

pub(crate) fn errorbar_leaves(e: &ErrorBar, name: &str) -> Vec<Leaf> {
    let mut data = e.line.clone();
    data.display_name = name.to_string();
    let (bx, by) = errorbar_bars(e);
    let mut bars = Line::new(bx, by, Vec::new(), e.line.color);
    bars.line_width = e.line.line_width;
    vec![
        group_leaf("errorbar", name, LeafObj::Line(data, "line", 0.0)),
        group_leaf("errorbar", name, LeafObj::Line(bars, BARS, e.cap_size)),
    ]
}

impl Graphics {
    /// `errorbar(...)`: one errorbar object per column of Y.
    pub fn errorbar(
        &mut self,
        ax: Option<Handle>,
        args: &ErrorBarArgs,
        opts: &Opts,
    ) -> Result<Vec<Handle>> {
        let spec = parse_spec(&args.spec, "errorbar")?;
        let y = &args.y;
        let ys = split_like(y, y, "Y")?;
        let xs = match &args.x {
            Some(x) => split_like(x, y, "X")?,
            None => ys
                .iter()
                .map(|c| (1..=c.len()).map(|k| k as f64).collect())
                .collect(),
        };
        let deltas = |d: &Option<Array>, what: &str| -> Result<Vec<Vec<f64>>> {
            match d {
                Some(a) if !a.is_empty() => split_like(a, y, what),
                _ => Ok(vec![Vec::new(); ys.len()]),
            }
        };
        let yn = deltas(&args.y_neg, "YNEG")?;
        let yp = deltas(&args.y_pos, "YPOS")?;
        let xn = deltas(&args.x_neg, "XNEG")?;
        let xp = deltas(&args.x_pos, "XPOS")?;
        let ax = self.new_plot(ax)?;
        let mut out = Vec::with_capacity(ys.len());
        for (k, (x, yv)) in xs.into_iter().zip(ys.iter()).enumerate() {
            let color = self.spec_color(ax, &spec)?;
            let mut line = Line::new(x, yv.clone(), Vec::new(), color);
            if let Some(s) = &spec.line_style {
                line.line_style = s.clone();
            }
            if let Some(m) = &spec.marker {
                line.marker = m.clone();
            }
            let e = ErrorBar {
                line,
                y_neg: yn[k].clone(),
                y_pos: yp[k].clone(),
                x_neg: xn[k].clone(),
                x_pos: xp[k].clone(),
                cap_size: 6.0,
            };
            out.push(self.add(
                ax,
                Kind::Group(Box::new(Group {
                    kind: GroupKind::ErrorBar(Box::new(e)),
                    display_name: String::new(),
                })),
            ));
        }
        self.apply_opts(&out, opts)?;
        Ok(out)
    }

    /// At export: the errorbar bars line with its caps, `CapSize` points
    /// wide, converted to data units with the axes pixel box and the
    /// resolved limits. `None` for every other leaf.
    pub(crate) fn errorbar_caps(
        &self,
        a: &crate::model::Axes,
        leaf: &Leaf,
        st: &AxesState,
    ) -> Option<Leaf> {
        let LeafObj::Line(l, BARS, cap) = &leaf.obj else {
            return None;
        };
        let (mut w, mut h);
        let logs;
        {
            w = st.figure_size[0] * a.position[2];
            h = st.figure_size[1] * a.position[3];
            if a.dar_mode == Mode::Manual || a.pbar_mode == Mode::Manual {
                // The plot box keeps its aspect inside the position.
                let (pw, ph) = (st.pbar[0], st.pbar[1]);
                if pw > 0.0 && ph > 0.0 {
                    let s = (w / pw).min(h / ph);
                    w = pw * s;
                    h = ph * s;
                }
            }
            logs = [a.x.scale == Scale::Log, a.y.scale == Scale::Log];
        }
        let half_px = cap * PX_PER_POINT / 2.0;
        // Half cap width in data units along axis i around value v.
        let span = |i: usize, v: f64, px: f64| -> (f64, f64) {
            let lim = st.lim[i];
            if px <= 0.0 || half_px <= 0.0 {
                return (v, v);
            }
            if logs[i] && lim[0] > 0.0 && v > 0.0 {
                let d = half_px * (lim[1] / lim[0]).log10() / px;
                (v / 10f64.powf(d), v * 10f64.powf(d))
            } else {
                let d = half_px * (lim[1] - lim[0]) / px;
                (v - d, v + d)
            }
        };
        let (mut x, mut y) = (Vec::new(), Vec::new());
        for seg in l.x.chunks(3).zip(l.y.chunks(3)) {
            let (sx, sy) = seg;
            if sx.len() < 2 {
                continue;
            }
            x.extend([sx[0], sx[1], f64::NAN]);
            y.extend([sy[0], sy[1], f64::NAN]);
            if half_px <= 0.0 {
                continue;
            }
            let vertical = sx[0] == sx[1];
            for end in [1, 0] {
                if vertical {
                    let (a, b) = span(0, sx[end], w);
                    x.extend([a, b, f64::NAN]);
                    y.extend([sy[end], sy[end], f64::NAN]);
                } else {
                    let (a, b) = span(1, sy[end], h);
                    x.extend([sx[end], sx[end], f64::NAN]);
                    y.extend([a, b, f64::NAN]);
                }
            }
        }
        let mut line = l.clone();
        line.x = x;
        line.y = y;
        let mut out = leaf.clone();
        out.obj = LeafObj::Line(line, "line", 0.0);
        Some(out)
    }
}

// ----- quiver --------------------------------------------------------------------

/// Arguments of `quiver`: bases (absent = the grid `1:n` × `1:m` of U),
/// U, V of equal size; `scale` as given (`Some(0)` = no autoscale).
#[derive(Debug, Clone, Default)]
pub struct QuiverArgs {
    pub x: Option<Array>,
    pub y: Option<Array>,
    pub u: Array,
    pub v: Array,
    pub scale: Option<f64>,
    pub spec: Option<String>,
    pub filled: bool,
}

/// The scale applied to every (u, v) (MATLAB's documented autoscale: the
/// longest arrow fits its grid cell, times `AutoScaleFactor`).
fn quiver_scale(q: &Quiver) -> f64 {
    if !q.auto_scale {
        return 1.0;
    }
    let span = |v: &[f64], n: f64| match finite_range(v) {
        Some([lo, hi]) if n > 0.0 => (hi - lo) / n,
        _ => 0.0,
    };
    let dx = span(&q.line.x, q.grid[1]);
    let dy = span(&q.line.y, q.grid[0]);
    let cell = dx * dx + dy * dy;
    let longest =
        q.u.iter()
            .zip(&q.v)
            .map(|(u, v)| u * u + v * v)
            .filter(|m| m.is_finite())
            .fold(0.0, f64::max);
    if cell > 0.0 && longest > 0.0 {
        q.auto_scale_factor / (longest / cell).sqrt()
    } else {
        // One arrow or zero vectors: measured, the factor alone.
        q.auto_scale_factor
    }
}

/// Shafts and heads of a quiver, NaN-separated.
pub(crate) fn quiver_geometry(q: &Quiver) -> [(Vec<f64>, Vec<f64>); 2] {
    let s = quiver_scale(q);
    let n = q
        .line
        .x
        .len()
        .min(q.line.y.len())
        .min(q.u.len())
        .min(q.v.len());
    let (mut sx, mut sy) = (Vec::with_capacity(3 * n), Vec::with_capacity(3 * n));
    let (mut hx, mut hy) = (Vec::new(), Vec::new());
    for i in 0..n {
        let (x, y) = (q.line.x[i], q.line.y[i]);
        let (du, dv) = (s * q.u[i], s * q.v[i]);
        let (tx, ty) = (x + du, y + dv);
        sx.extend([x, tx, f64::NAN]);
        sy.extend([y, ty, f64::NAN]);
        let len = du.hypot(dv);
        if !q.show_arrow_head || !(len.is_finite() && len > 0.0 && tx.is_finite() && ty.is_finite())
        {
            continue;
        }
        let head = len / 3.0 * q.max_head_size / 0.2;
        let half = head / 3.0;
        let (ux, uy) = (du / len, dv / len);
        let (bx, by) = (tx - head * ux, ty - head * uy);
        // Normal (-uy, ux): first point on the right of the shaft.
        hx.extend([bx + half * uy, tx, bx - half * uy, f64::NAN]);
        hy.extend([by - half * ux, ty, by + half * ux, f64::NAN]);
    }
    [(sx, sy), (hx, hy)]
}

pub(crate) fn quiver_leaves(q: &Quiver, name: &str) -> Vec<Leaf> {
    let [(sx, sy), (hx, hy)] = quiver_geometry(q);
    let style = |x: Vec<f64>, y: Vec<f64>| {
        let mut l = Line::new(x, y, Vec::new(), q.line.color);
        l.line_style = q.line.line_style.clone();
        l.line_width = q.line.line_width;
        l
    };
    let mut shafts = style(sx, sy);
    shafts.display_name = name.to_string();
    let mut out = vec![group_leaf(
        "quiver",
        name,
        LeafObj::Line(shafts, "line", 0.0),
    )];
    if !hx.is_empty() {
        out.push(group_leaf(
            "quiver",
            name,
            LeafObj::Line(style(hx, hy), "line", 0.0),
        ));
    }
    if q.line.marker != "none" {
        let mut m = q.line.clone();
        m.line_style = "none".into();
        m.display_name.clear();
        out.push(group_leaf("quiver", name, LeafObj::Line(m, "line", 0.0)));
    }
    out
}

impl Graphics {
    /// `quiver(...)`.
    pub fn quiver(&mut self, ax: Option<Handle>, args: &QuiverArgs, opts: &Opts) -> Result<Handle> {
        let spec = parse_spec(&args.spec, "quiver")?;
        let (u, v) = (&args.u, &args.v);
        if u.numel() != v.numel() || (!u.is_vector() && u.dims != v.dims) {
            return Err(GraphicsError::args(
                "quiver",
                "U and V must be the same size",
            ));
        }
        let n = u.numel();
        let (rows, cols) = if u.is_vector() {
            (1, n)
        } else {
            (u.rows(), u.cols())
        };
        // Column-major grid positions of the elements of U.
        let mesh = |a: &Array, along_cols: bool, what: &str| -> Result<Vec<f64>> {
            if a.numel() == n {
                return Ok(a.data.clone());
            }
            let len = if along_cols { cols } else { rows };
            if a.is_vector() && a.numel() == len && !u.is_vector() {
                return Ok((0..n)
                    .map(|k| {
                        let (r, c) = (k % rows, k / rows);
                        a.data[if along_cols { c } else { r }]
                    })
                    .collect());
            }
            Err(GraphicsError::args(
                "quiver",
                format!("{what} must match the size of U"),
            ))
        };
        let (x, y) = match (&args.x, &args.y) {
            (Some(x), Some(y)) => (mesh(x, true, "X")?, mesh(y, false, "Y")?),
            _ => {
                let k = |i: usize| (i + 1) as f64;
                if u.is_vector() {
                    ((0..n).map(k).collect(), vec![1.0; n])
                } else {
                    (
                        (0..n).map(|i| k(i / rows)).collect(),
                        (0..n).map(|i| k(i % rows)).collect(),
                    )
                }
            }
        };
        // Spacing counts: rows×cols of a matrix U, sqrt(N) for vectors.
        let grid = if u.is_vector() {
            let s = (n as f64).sqrt();
            [s, s]
        } else {
            [rows as f64, cols as f64]
        };
        let ax = self.new_plot(ax)?;
        let cycled = self.axes_mut(ax)?.next_color();
        let mut line = Line::new(x, y, Vec::new(), spec.color.unwrap_or(cycled));
        if let Some(s) = &spec.line_style
            && s != "none"
        {
            line.line_style = s.clone();
        }
        if let Some(m) = &spec.marker {
            line.marker = m.clone();
        }
        if args.filled {
            line.marker_face = Paint::Rgb(line.color);
        }
        let mut q = Quiver {
            line,
            u: u.data.clone(),
            v: v.data.clone(),
            grid,
            auto_scale: true,
            auto_scale_factor: 0.9,
            max_head_size: 0.2,
            show_arrow_head: true,
        };
        match args.scale {
            Some(0.0) => q.auto_scale = false,
            Some(s) if s.is_finite() && s > 0.0 => q.auto_scale_factor = s,
            Some(_) => {
                return Err(GraphicsError::args(
                    "quiver",
                    "the scale must be a nonnegative scalar",
                ));
            }
            None => {}
        }
        let h = self.add(
            ax,
            Kind::Group(Box::new(Group {
                kind: GroupKind::Quiver(Box::new(q)),
                display_name: String::new(),
            })),
        );
        self.apply_opts(&[h], opts)?;
        Ok(h)
    }
}

// ----- fplot ---------------------------------------------------------------------

/// Deepest bisection of one initial interval.
pub const FPLOT_MAX_DEPTH: usize = 10;
/// Point budget of one fplot line (below the contract's 2000 samples, so
/// the line is never decimated).
pub const FPLOT_MAX_POINTS: usize = 1500;
/// Midpoint deviation from the chord (in units of the initial x/y range)
/// below which an interval is flat enough.
pub const FPLOT_TOLERANCE: f64 = 1e-3;

/// Evaluates a batch of parameters into curve points for [`fplot_sample`].
pub type CurveEval<'a, E> = dyn FnMut(&[f64]) -> std::result::Result<Vec<[f64; 2]>, E> + 'a;

#[derive(Clone, Copy)]
struct Seg {
    ta: f64,
    tb: f64,
    pa: [f64; 2],
    pb: [f64; 2],
    active: bool,
    jump: bool,
}

fn finite2(p: [f64; 2]) -> bool {
    p[0].is_finite() && p[1].is_finite()
}

/// Samples a curve `t → (x, y)` on `range` for `fplot`: `density` evenly
/// spaced points, then bisection where the midpoint leaves the chord by
/// more than [`FPLOT_TOLERANCE`] of the initial range, where the curve
/// meets a NaN/Inf boundary, up to [`FPLOT_MAX_DEPTH`] levels and
/// [`FPLOT_MAX_POINTS`] points. Values more than one range away from the
/// initial centre are off-scale and not refined further. A NaN point is
/// inserted at a detected discontinuity: an interval still not flat at the
/// deepest level whose ends differ by over 5% of the range, or an
/// off-scale interval whose ends lie on opposite sides (a pole).
/// `eval` gets every batch of parameters at once (one vectorised call).
pub fn fplot_sample<E>(
    range: [f64; 2],
    density: usize,
    eval: &mut CurveEval<'_, E>,
) -> std::result::Result<Vec<[f64; 2]>, E> {
    let [a, b] = range;
    let n = density.max(2);
    let t0: Vec<f64> = (0..n)
        .map(|k| {
            if k + 1 == n {
                b
            } else {
                a + (b - a) * k as f64 / (n - 1) as f64
            }
        })
        .collect();
    let p0 = eval(&t0)?;
    let ext = |i: usize| {
        let v: Vec<f64> = p0.iter().map(|p| p[i]).collect();
        match finite_range(&v) {
            Some([lo, hi]) if hi > lo => ((hi - lo), (lo + hi) / 2.0),
            Some([lo, _]) => (1.0, lo),
            None => (1.0, 0.0),
        }
    };
    let (sx, cx) = ext(0);
    let (sy, cy) = ext(1);
    let off = |p: [f64; 2]| ((p[0] - cx) / sx).abs() > 1.0 || ((p[1] - cy) / sy).abs() > 1.0;
    let side = |p: [f64; 2]| ((p[1] - cy).signum(), (p[0] - cx).signum());
    let mut segs: Vec<Seg> = t0
        .windows(2)
        .zip(p0.windows(2))
        .map(|(t, p)| Seg {
            ta: t[0],
            tb: t[1],
            pa: p[0],
            pb: p[1],
            active: true,
            jump: false,
        })
        .collect();
    let mut count = n;
    for _ in 0..FPLOT_MAX_DEPTH {
        for s in segs.iter_mut().filter(|s| s.active) {
            let (fa, fb) = (finite2(s.pa), finite2(s.pb));
            // Off-scale: beyond one range, or ±Inf (a pole hit exactly).
            let far = |p: [f64; 2]| {
                if finite2(p) {
                    off(p)
                } else {
                    !p[0].is_nan() && !p[1].is_nan()
                }
            };
            if !fa && !fb {
                s.active = false;
            } else if far(s.pa) && far(s.pb) {
                s.active = false;
                s.jump = fa && fb && side(s.pa) != side(s.pb);
            }
        }
        let todo: Vec<usize> = (0..segs.len()).filter(|&i| segs[i].active).collect();
        if todo.is_empty() || count + todo.len() > FPLOT_MAX_POINTS {
            break;
        }
        let mids: Vec<f64> = todo
            .iter()
            .map(|&i| (segs[i].ta + segs[i].tb) / 2.0)
            .collect();
        let pm = eval(&mids)?;
        count += todo.len();
        let mut next = Vec::with_capacity(segs.len() + todo.len());
        let mut k = 0;
        for (i, s) in segs.iter().enumerate() {
            if k < todo.len() && todo[k] == i {
                let (m, p) = (mids[k], pm[k]);
                k += 1;
                let refine = if finite2(s.pa) && finite2(s.pb) && finite2(p) {
                    let dx = (p[0] - (s.pa[0] + s.pb[0]) / 2.0) / sx;
                    let dy = (p[1] - (s.pa[1] + s.pb[1]) / 2.0) / sy;
                    dx.hypot(dy) > FPLOT_TOLERANCE
                } else {
                    true
                };
                let mut left = Seg {
                    tb: m,
                    pb: p,
                    active: refine,
                    ..*s
                };
                let mut right = Seg {
                    ta: m,
                    pa: p,
                    active: refine,
                    ..*s
                };
                if refine && !(finite2(s.pa) && finite2(s.pb) && finite2(p)) {
                    // Only the half that crosses a finite/non-finite edge.
                    left.active = finite2(left.pa) != finite2(left.pb);
                    right.active = finite2(right.pa) != finite2(right.pb);
                }
                next.push(left);
                next.push(right);
            } else {
                next.push(*s);
            }
        }
        segs = next;
    }
    for s in segs.iter_mut().filter(|s| s.active) {
        if finite2(s.pa) && finite2(s.pb) {
            let dx = (s.pb[0] - s.pa[0]) / sx;
            let dy = (s.pb[1] - s.pa[1]) / sy;
            s.jump = dx.hypot(dy) > 0.05;
        }
    }
    let mut out = Vec::with_capacity(segs.len() + 2);
    if let Some(first) = segs.first() {
        out.push(first.pa);
    }
    for s in &segs {
        if s.jump {
            out.push([f64::NAN, f64::NAN]);
        }
        out.push(s.pb);
    }
    Ok(out)
}

/// What an `fplot` line was made from.
#[derive(Debug, Clone, Default)]
pub struct FunctionInfo {
    pub function: String,
    pub y_function: Option<String>,
    pub range: [f64; 2],
    pub mesh_density: f64,
    pub spec: Option<String>,
}

impl Graphics {
    /// `fplot`: a function line through the sampled points. A fresh
    /// (not held) axes takes the interval as its x-limits (MATLAB).
    pub fn function_line(
        &mut self,
        ax: Option<Handle>,
        x: Vec<f64>,
        y: Vec<f64>,
        info: &FunctionInfo,
        opts: &Opts,
    ) -> Result<Handle> {
        let spec = parse_spec(&info.spec, "fplot")?;
        let ax = self.new_plot(ax)?;
        let fresh = !self.axes_ref(ax)?.hold_add;
        let color = self.spec_color(ax, &spec)?;
        let mut line = Line::new(x, y, Vec::new(), color);
        if let Some(s) = &spec.line_style {
            line.line_style = s.clone();
        }
        if let Some(m) = &spec.marker {
            line.marker = m.clone();
        }
        let f = FunctionLine {
            line,
            function: info.function.clone(),
            y_function: info.y_function.clone(),
            range: info.range,
            mesh_density: info.mesh_density,
        };
        let h = self.add(
            ax,
            Kind::Group(Box::new(Group {
                kind: GroupKind::FunctionLine(Box::new(f)),
                display_name: String::new(),
            })),
        );
        if fresh && info.y_function.is_none() && info.range[0] < info.range[1] {
            let a = self.axes_mut(ax)?;
            a.x.lim = info.range;
            a.x.lim_mode = Mode::Manual;
        }
        self.apply_opts(&[h], opts)?;
        Ok(h)
    }
}

// ----- polar -----------------------------------------------------------------------

const GRID_COLOR: Rgb = [0.8725, 0.8725, 0.8725];
const AXIS_COLOR: Rgb = [0.15, 0.15, 0.15];
const CIRCLE_SEGMENTS: usize = 96;

fn polar_text(text: String, x: f64, y: f64, ha: &str, va: &str) -> LeafObj {
    let mut t = Text::new(LabelRole::Free);
    t.lines = vec![text];
    t.position = [x, y, 0.0];
    t.horizontal_alignment = ha.into();
    t.vertical_alignment = va.into();
    t.color = AXIS_COLOR;
    t.interpreter = "none".into();
    LeafObj::Text(t)
}

/// The outer radius and r ticks of a polar grid holding data up to `rmax`
/// (automatic r limits from 0, like MATLAB's `RLim`).
pub fn polar_ticks(rmax: f64) -> (f64, Vec<f64>) {
    let rmax = if rmax.is_finite() && rmax > 0.0 {
        rmax
    } else {
        1.0
    };
    let (lim, ticks) = layout::auto_limits(Some([0.0, rmax]));
    let r = lim[1];
    (
        r,
        ticks.into_iter().filter(|t| *t > 0.0 && *t <= r).collect(),
    )
}

/// Grid, frame circle, labels and the invisible extent anchor of a polar
/// axes, from the radius of the data lines already in `leaves`.
pub(crate) fn polar_leaves(leaves: &[Leaf]) -> Vec<Leaf> {
    let mut rmax: f64 = 0.0;
    for leaf in leaves {
        if let LeafObj::Line(l, ..) = &leaf.obj {
            for (x, y) in l.x.iter().zip(&l.y) {
                let r = x.hypot(*y);
                if r.is_finite() {
                    rmax = rmax.max(r);
                }
            }
        }
    }
    let (r, ticks) = polar_ticks(rmax);
    let circle = |rad: f64, x: &mut Vec<f64>, y: &mut Vec<f64>| {
        for k in 0..=CIRCLE_SEGMENTS {
            let a = std::f64::consts::TAU * k as f64 / CIRCLE_SEGMENTS as f64;
            x.push(rad * a.cos());
            y.push(rad * a.sin());
        }
        x.push(f64::NAN);
        y.push(f64::NAN);
    };
    let (mut gx, mut gy) = (Vec::new(), Vec::new());
    for &t in ticks.iter().filter(|t| **t < r) {
        circle(t, &mut gx, &mut gy);
    }
    for k in 0..12 {
        let a = (30.0 * k as f64).to_radians();
        gx.extend([0.0, r * a.cos(), f64::NAN]);
        gy.extend([0.0, r * a.sin(), f64::NAN]);
    }
    let mut grid = Line::new(gx, gy, Vec::new(), GRID_COLOR);
    grid.line_width = 0.5;
    let (mut ox, mut oy) = (Vec::new(), Vec::new());
    circle(r, &mut ox, &mut oy);
    let frame = Line::new(ox, oy, Vec::new(), AXIS_COLOR);
    // Invisible: only makes the tight limits include the labels.
    let e = 1.15 * r;
    let mut anchor = Line::new(vec![-e, e], vec![-e, e], Vec::new(), AXIS_COLOR);
    anchor.line_style = "none".into();
    let leaf = |obj| Leaf {
        role: Some("polar_grid"),
        group_name: None,
        group: None,
        unit_box: [false; 2],
        obj,
    };
    let mut out = vec![
        leaf(LeafObj::Line(grid, "line", 0.0)),
        leaf(LeafObj::Line(frame, "line", 0.0)),
        leaf(LeafObj::Line(anchor, "line", 0.0)),
    ];
    for k in 0..12 {
        let deg = 30 * k;
        let a = (deg as f64).to_radians();
        let (c, s) = (a.cos(), a.sin());
        let ha = if c > 0.1 {
            "left"
        } else if c < -0.1 {
            "right"
        } else {
            "center"
        };
        let va = if s > 0.1 {
            "bottom"
        } else if s < -0.1 {
            "top"
        } else {
            "middle"
        };
        out.push(leaf(polar_text(
            format!("{deg}°"),
            1.04 * r * c,
            1.04 * r * s,
            ha,
            va,
        )));
    }
    let a = 80f64.to_radians();
    for &t in &ticks {
        out.push(leaf(polar_text(
            layout::tick_label(t),
            t * a.cos(),
            t * a.sin(),
            "left",
            "bottom",
        )));
    }
    out
}

fn linspace(a: f64, b: f64, n: usize) -> Vec<f64> {
    match n {
        0 => Vec::new(),
        1 => vec![a],
        _ => (0..n)
            .map(|k| a + (b - a) * k as f64 / (n - 1) as f64)
            .collect(),
    }
}

/// θ/ρ pairs of one series, MATLAB pairing (vector with matrix columns).
fn polar_pairs(
    theta: Option<&Array>,
    rho: &Array,
    func: &str,
) -> Result<Vec<(Vec<f64>, Vec<f64>)>> {
    let columns = |a: &Array| -> Vec<Vec<f64>> {
        if a.is_vector() || a.is_empty() {
            vec![a.data.clone()]
        } else {
            (0..a.cols()).map(|c| a.column(c).to_vec()).collect()
        }
    };
    let rhos = columns(rho);
    let Some(theta) = theta else {
        return Ok(rhos
            .into_iter()
            .map(|r| (linspace(0.0, std::f64::consts::TAU, r.len()), r))
            .collect());
    };
    let thetas = columns(theta);
    let bad = || GraphicsError::args(func, "THETA and RHO must be the same size");
    match (thetas.len(), rhos.len()) {
        (1, _) if rhos.iter().all(|r| r.len() == thetas[0].len()) => {
            Ok(rhos.into_iter().map(|r| (thetas[0].clone(), r)).collect())
        }
        (_, 1) if thetas.iter().all(|t| t.len() == rhos[0].len()) => {
            Ok(thetas.into_iter().map(|t| (t, rhos[0].clone())).collect())
        }
        (a, b) if a == b && theta.dims == rho.dims => Ok(thetas.into_iter().zip(rhos).collect()),
        _ => Err(bad()),
    }
}

impl Graphics {
    /// `polarplot(theta, rho, ...)` / `polar(theta, rho)`: the data as x/y
    /// lines (ρ·cos θ, ρ·sin θ) on a cartesian axes that draws a polar grid.
    /// Approximation: `gca` stays a cartesian axes (hidden, `axis equal`).
    pub fn polar_plot(
        &mut self,
        ax: Option<Handle>,
        func: &str,
        series: &[PlotSeries],
        opts: &Opts,
    ) -> Result<Vec<Handle>> {
        if series.is_empty() {
            return Err(GraphicsError::args(func, "not enough input arguments"));
        }
        let mut lines: Vec<(Vec<f64>, Vec<f64>, Option<String>)> = Vec::new();
        for s in series {
            parse_spec(&s.spec, func)?;
            for (t, r) in polar_pairs(s.x.as_ref(), &s.y, func)? {
                let x = t.iter().zip(&r).map(|(t, r)| r * t.cos()).collect();
                let y = t.iter().zip(&r).map(|(t, r)| r * t.sin()).collect();
                lines.push((x, y, s.spec.clone()));
            }
        }
        let ax = self.new_plot(ax)?;
        let children = self.obj(ax)?.children.clone();
        let has_grid = children.iter().any(|h| is_polar_grid(self, *h));
        if !has_grid {
            if !children.is_empty() {
                return Err(unavailable(
                    func,
                    "plotting into a held cartesian axes (polar axes are approximated)",
                ));
            }
            self.add(
                ax,
                Kind::Group(Box::new(Group {
                    kind: GroupKind::PolarGrid,
                    display_name: String::new(),
                })),
            );
            self.axis(Some(ax), &crate::annot::AxisCmd::Equal)?;
            self.axis(Some(ax), &crate::annot::AxisCmd::Tight)?;
            self.obj_mut(ax)?.visible = false;
        }
        let held = self.axes_ref(ax)?.hold_add;
        self.axes_mut(ax)?.hold_add = true;
        let mut out = Vec::new();
        let mut result = Ok(());
        for (x, y, spec) in lines {
            let mut s = PlotSeries::xy(&x, &y);
            s.spec = spec;
            match self.plot(Some(ax), &[s], &[]) {
                Ok(h) => out.extend(h),
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
        }
        self.axes_mut(ax)?.hold_add = held;
        result?;
        self.apply_opts(&out, opts)?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(f: fn(f64) -> f64, range: [f64; 2]) -> Vec<[f64; 2]> {
        let mut eval = |t: &[f64]| -> std::result::Result<Vec<[f64; 2]>, ()> {
            Ok(t.iter().map(|&t| [t, f(t)]).collect())
        };
        fplot_sample(range, 23, &mut eval).unwrap()
    }

    #[test]
    fn fplot_refines_curves_and_breaks_at_poles() {
        let line = sample(|x| 2.0 * x + 1.0, [-5.0, 5.0]);
        // One round of midpoints, then every interval is flat.
        assert_eq!(
            line.len(),
            45,
            "a straight line needs no further refinement"
        );
        let sin = sample(f64::sin, [-5.0, 5.0]);
        assert!(sin.len() > 23 && sin.len() < 400, "{}", sin.len());
        assert!(sin.iter().all(|p| p[1].is_finite()));
        assert_eq!(sin[0][0], -5.0);
        assert_eq!(sin[sin.len() - 1][0], 5.0);
        // Sorted in t, every point on the curve.
        assert!(sin.windows(2).all(|w| w[0][0] < w[1][0]));
        assert!(sin.iter().all(|p| (p[1] - p[0].sin()).abs() < 1e-15));
        let tan = sample(f64::tan, [-2.0, 2.0]);
        let gaps = tan.iter().filter(|p| p[1].is_nan()).count();
        assert_eq!(gaps, 2, "tan has poles at ±pi/2");
        let step = sample(|x| if x < 0.3 { 0.0 } else { 1.0 }, [-1.0, 1.0]);
        assert_eq!(step.iter().filter(|p| p[1].is_nan()).count(), 1);
        assert!(sample(f64::tan, [-5.0, 5.0]).len() <= FPLOT_MAX_POINTS + 10);
    }

    #[test]
    fn polar_ticks_start_at_zero() {
        // The cartesian automatic-limit rule on [0, max rho]. Octave's polar
        // (measured, same data) uses r ticks 0:3 instead.
        assert_eq!(polar_ticks(2.5), (2.5, vec![0.5, 1.0, 1.5, 2.0, 2.5]));
        assert_eq!(polar_ticks(1.0).0, 1.0);
        assert_eq!(polar_ticks(0.0).0, 1.0);
    }
}
