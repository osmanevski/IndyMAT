//! One thin builtin per graphics name: split the MATLAB argument list, call
//! the model, convert the result. Argument rules follow MATLAB; a first
//! argument that is an axes handle is the target axes (`plot(ax, ...)`).
//! Outputs (handles) are only returned when the caller asks for them, as in
//! MATLAB, except for queries (`gcf`, `xlim()`, ...).

use std::sync::OnceLock;

use indy_graphics::color::{colormap_by_name, default_colormap};
use indy_graphics::{
    AxisCmd, BarArgs, CloseTarget, ColormapArg, Graphics, GridArgs, Handle, LabelRole, Levels,
    PropValue, ScatterArgs, callargs,
};
use indy_interp::builtins::err;
use indy_interp::{BuiltinFn, Interp, R, Value};
use indy_value::Num;

use crate::convert::{
    array, from_prop, gerr, handle_prop, handles_of, handles_value, lines, numbers, to_props,
};
use crate::graphics;

pub(crate) type Opts = Vec<(String, PropValue)>;

/// The interpreter's own `delete` (files, handle-class objects).
static PLAIN_DELETE: OnceLock<BuiltinFn> = OnceLock::new();

pub fn register(it: &mut Interp) {
    if let Some(&plain) = it.builtins.get("delete") {
        let _ = PLAIN_DELETE.set(plain);
    }
    let table: &[(&'static str, BuiltinFn)] = &[
        ("figure", figure),
        ("gcf", gcf),
        ("gca", gca),
        ("axes", axes),
        ("subplot", subplot),
        ("hold", hold),
        ("ishold", ishold),
        ("close", close),
        ("clf", clf),
        ("cla", cla),
        ("delete", delete),
        ("drawnow", drawnow),
        ("plot", plot),
        ("plot3", plot3),
        ("semilogx", semilogx),
        ("semilogy", semilogy),
        ("loglog", loglog),
        ("line", line),
        ("scatter", scatter),
        ("scatter3", scatter3),
        ("bar", bar),
        ("barh", barh),
        ("stairs", stairs),
        ("stem", stem),
        ("area", area),
        ("fill", fill),
        ("patch", patch),
        ("surf", surf),
        ("mesh", mesh),
        ("contour", contour),
        ("contourf", contourf),
        ("image", image),
        ("imagesc", imagesc),
        ("text", text),
        ("xlabel", xlabel),
        ("ylabel", ylabel),
        ("zlabel", zlabel),
        ("title", title),
        ("legend", legend),
        ("grid", grid),
        ("box", box_),
        ("axis", axis),
        ("xlim", xlim),
        ("ylim", ylim),
        ("zlim", zlim),
        ("caxis", caxis),
        ("clim", caxis),
        ("colormap", colormap),
        ("colorbar", colorbar),
        ("view", view),
        ("daspect", daspect),
        ("pbaspect", pbaspect),
        ("xticks", xticks),
        ("yticks", yticks),
        ("zticks", zticks),
        ("xticklabels", xticklabels),
        ("yticklabels", yticklabels),
        ("get", get),
        ("set", set),
        ("ishandle", ishandle),
        ("isgraphics", isgraphics),
        ("findobj", findobj),
        ("findall", findobj),
        ("peaks", peaks),
        ("print", crate::export::print),
        ("saveas", crate::export::saveas),
        ("exportgraphics", crate::export::exportgraphics),
        ("histogram", crate::charts::histogram),
        ("hist", crate::charts::hist),
        ("sgtitle", crate::charts::sgtitle),
        ("pie", crate::charts::pie),
        ("yyaxis", crate::charts::yyaxis),
        ("shading", crate::lane_b::shading),
        ("imshow", crate::lane_b::imshow),
        ("contour3", crate::lane_b::contour3),
        ("surfc", crate::lane_b::surfc),
        ("meshc", crate::lane_b::meshc),
        ("bar3", crate::lane_b::bar3),
        ("errorbar", crate::lane_a::errorbar),
        ("quiver", crate::lane_a::quiver),
        ("fplot", crate::lane_a::fplot),
        ("polarplot", crate::lane_a::polarplot),
        ("polar", crate::lane_a::polar),
        ("isfigure", crate::handles::isfigure),
        ("setappdata", crate::handles::setappdata),
        ("getappdata", crate::handles::getappdata),
        ("isappdata", crate::handles::isappdata),
        ("rmappdata", crate::handles::rmappdata),
        ("ancestor", crate::handles::ancestor),
        ("gco", crate::handles::gco),
        ("allchild", crate::handles::allchild),
    ];
    for &(name, f) in table {
        if name == "delete" || !it.builtins.contains_key(name) {
            it.builtins.insert(name, f);
        }
    }
    for &(name, f) in COLORMAP_FNS {
        it.builtins.entry(name).or_insert(f);
    }
    it.handle_field_get = Some(crate::handles::field_get);
    it.handle_field_set = Some(crate::handles::field_set);
}

// ----- helpers ---------------------------------------------------------------

pub(crate) fn bad(fname: &str, detail: impl std::fmt::Display) -> Box<indy_interp::Error> {
    err("MATLAB:hg:InvalidArgument", format!("{fname}: {detail}"))
}

pub(crate) fn not_yet(fname: &str, what: impl std::fmt::Display) -> Box<indy_interp::Error> {
    err(
        "IndyR:graphicsUnavailable",
        format!("{fname}: {what} is not available in the Rust engine yet"),
    )
}

pub(crate) fn num(v: f64) -> Value {
    Value::num(v)
}

/// Handles as the call's outputs: only when asked for.
pub(crate) fn out_handles(hs: &[Handle], nargout: usize) -> R<Vec<Value>> {
    if nargout == 0 {
        return Ok(vec![]);
    }
    Ok(vec![handles_value(hs)])
}

pub(crate) fn out_handle(h: Handle, nargout: usize) -> R<Vec<Value>> {
    out_handles(&[h], nargout)
}

pub(crate) fn row(v: &[f64]) -> Value {
    Value::row(v.to_vec())
}

/// Removes a leading axes handle from the arguments.
pub(crate) fn take_axes(g: &Graphics, a: &mut Vec<PropValue>) -> Option<Handle> {
    let h = match a.first() {
        Some(PropValue::Num(n)) => Handle::from_value(*n)?,
        _ => return None,
    };
    if g.type_of(h).ok()? == "axes" {
        a.remove(0);
        Some(h)
    } else {
        None
    }
}

/// Removes a leading figure handle from the arguments.
pub(crate) fn take_figure(g: &Graphics, a: &mut Vec<PropValue>) -> Option<Handle> {
    let h = match a.first() {
        Some(PropValue::Num(n)) => Handle::from_value(*n)?,
        _ => return None,
    };
    if g.type_of(h).ok()? == "figure" {
        a.remove(0);
        Some(h)
    } else {
        None
    }
}

pub(crate) fn pairs(fname: &str, a: &[PropValue]) -> R<Opts> {
    let opts = callargs::name_value(fname, a).map_err(gerr)?;
    Ok(opts
        .into_iter()
        .map(|(k, v)| {
            let v = handle_prop(&k, v);
            (k, v)
        })
        .collect())
}

/// Splits off the leading numeric arguments.
pub(crate) fn split_numeric(a: &[PropValue]) -> (Vec<PropValue>, &[PropValue]) {
    let n = a
        .iter()
        .position(|v| !matches!(v, PropValue::Num(_) | PropValue::Array(_)))
        .unwrap_or(a.len());
    (a[..n].to_vec(), &a[n..])
}

pub(crate) fn word(p: &PropValue) -> Option<String> {
    p.as_str().map(str::to_ascii_lowercase)
}

pub(crate) fn set_all(g: &mut Graphics, h: Handle, opts: &Opts) -> R<()> {
    for (k, v) in opts {
        g.set(h, k, v.clone()).map_err(gerr)?;
    }
    Ok(())
}

/// The option named `name` (case-insensitive), removed from `opts`.
pub(crate) fn take_opt(opts: &mut Opts, name: &str) -> Option<PropValue> {
    let i = opts
        .iter()
        .position(|(k, _)| k.eq_ignore_ascii_case(name))?;
    Some(opts.remove(i).1)
}

pub(crate) fn scalar(p: &PropValue, fname: &str, what: &str) -> R<f64> {
    match numbers(p, fname, what)?.as_slice() {
        [x] => Ok(*x),
        _ => Err(bad(fname, format!("{what} must be a scalar"))),
    }
}

pub(crate) fn pair(p: &PropValue, fname: &str, what: &str) -> R<[f64; 2]> {
    match numbers(p, fname, what)?.as_slice() {
        [a, b] => Ok([*a, *b]),
        _ => Err(bad(fname, format!("{what} must have two elements"))),
    }
}

pub(crate) fn on_off(p: &PropValue, fname: &str) -> R<Option<bool>> {
    match word(p).as_deref() {
        Some("on") => Ok(Some(true)),
        Some("off") => Ok(Some(false)),
        _ => Err(bad(fname, "the argument must be 'on' or 'off'")),
    }
}

// ----- figures and axes --------------------------------------------------------

/// `figure`, `figure(n)`, `figure(h)`, `figure('Name', v, ...)`.
fn figure(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let a = to_props(&a, "figure")?;
    let g = graphics(it);
    let (number, rest) = match a.first() {
        Some(PropValue::Num(n)) => (Some(*n), &a[1..]),
        Some(PropValue::Str(_)) | None => (None, &a[..]),
        Some(_) => return Err(bad("figure", "N must be a figure number")),
    };
    let opts = pairs("figure", rest)?;
    let h = g.figure(number).map_err(gerr)?;
    set_all(g, h, &opts)?;
    out_handle(h, nargout)
}

fn gcf(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    if !a.is_empty() {
        return Err(bad("gcf", "too many input arguments"));
    }
    Ok(vec![num(graphics(it).gcf().value())])
}

fn gca(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "gca")?;
    let g = graphics(it);
    if let Some(f) = take_figure(g, &mut a) {
        g.make_current(f).map_err(gerr)?;
    }
    if !a.is_empty() {
        return Err(bad("gca", "the argument must be a figure handle"));
    }
    Ok(vec![num(g.gca().value())])
}

/// `axes`, `axes(h)`, `axes('Position', p, 'Parent', f, ...)`.
fn axes(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "axes")?;
    let g = graphics(it);
    if let Some(h) = take_axes(g, &mut a) {
        if !a.is_empty() {
            return Err(bad("axes", "too many input arguments"));
        }
        g.make_current(h).map_err(gerr)?;
        return out_handle(h, nargout);
    }
    // `axes(fig, ...)`: a new axes in that figure.
    if let Some(f) = take_figure(g, &mut a) {
        g.make_current(f).map_err(gerr)?;
    }
    let mut opts = pairs("axes", &a)?;
    if let Some(p) = take_opt(&mut opts, "Parent") {
        match p {
            PropValue::Handle(f) if g.type_of(f).ok() == Some("figure") => {
                g.make_current(f).map_err(gerr)?
            }
            _ => return Err(bad("axes", "Parent must be a figure handle")),
        }
    }
    let position = match take_opt(&mut opts, "Position") {
        Some(p) => {
            let v = numbers(&p, "axes", "Position")?;
            let p: [f64; 4] = v
                .try_into()
                .map_err(|_| bad("axes", "Position must have four elements"))?;
            Some(p)
        }
        None => None,
    };
    let h = g.axes(position).map_err(gerr)?;
    set_all(g, h, &opts)?;
    out_handle(h, nargout)
}

pub(crate) fn positive_int(x: f64, fname: &str) -> R<usize> {
    if x.is_finite() && x >= 1.0 && x.fract() == 0.0 && x < 1e9 {
        Ok(x as usize)
    } else {
        Err(bad(fname, "arguments must be positive integers"))
    }
}

/// `subplot(m, n, p)` (p may list several tiles) or `subplot(mnp)`.
fn subplot(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let a = to_props(&a, "subplot")?;
    let (m, n, p) = match a.as_slice() {
        [PropValue::Num(mnp)] => {
            let k = positive_int(*mnp, "subplot")?;
            if !(111..=999).contains(&k) {
                return Err(bad("subplot", "a single argument must have three digits"));
            }
            (k / 100, (k / 10) % 10, vec![k % 10])
        }
        [m, n, p] => {
            let m = positive_int(scalar(m, "subplot", "M")?, "subplot")?;
            let n = positive_int(scalar(n, "subplot", "N")?, "subplot")?;
            let p = numbers(p, "subplot", "P")?
                .into_iter()
                .map(|x| positive_int(x, "subplot"))
                .collect::<R<Vec<usize>>>()?;
            (m, n, p)
        }
        [_, _, _, extra, ..] => {
            let what = match extra.as_str() {
                Some(s) => format!("the '{s}' option"),
                None => "this form".to_string(),
            };
            return Err(not_yet("subplot", what));
        }
        _ => return Err(bad("subplot", "expected subplot(m, n, p)")),
    };
    let h = graphics(it).subplot(m, n, &p).map_err(gerr)?;
    out_handle(h, nargout)
}

/// `hold`, `hold on/off/all`, `hold(ax, ...)`.
fn hold(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "hold")?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let state = match a.as_slice() {
        [] => None,
        [p] => match word(p).as_deref() {
            Some("on") | Some("all") => Some(true),
            Some("off") => Some(false),
            _ => return Err(bad("hold", "the argument must be 'on' or 'off'")),
        },
        _ => return Err(bad("hold", "too many input arguments")),
    };
    g.hold(ax, state).map_err(gerr)?;
    Ok(vec![])
}

fn ishold(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "ishold")?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    Ok(vec![Value::bool(g.ishold(ax))])
}

/// `close`, `close all`, `close(h)`, `close(name)`.
fn close(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let a = to_props(&a, "close")?;
    let g = graphics(it);
    match a.first() {
        None => g.close(CloseTarget::Current).map_err(gerr)?,
        Some(PropValue::Str(s)) if s.eq_ignore_ascii_case("all") => {
            g.close(CloseTarget::All).map_err(gerr)?
        }
        Some(PropValue::Str(name)) => {
            let found = g
                .figure_handles()
                .into_iter()
                .find(|f| g.get(*f, "Name").ok() == Some(PropValue::Str(name.clone())));
            match found {
                Some(f) => g.close(CloseTarget::Handle(f)).map_err(gerr)?,
                None => return Err(bad("close", format!("no figure named '{name}'"))),
            }
        }
        Some(p) => {
            for x in numbers(p, "close", "H")? {
                let h =
                    Handle::from_value(x).ok_or_else(|| bad("close", "invalid figure handle"))?;
                g.close(CloseTarget::Handle(h)).map_err(gerr)?;
            }
        }
    }
    if nargout > 0 {
        return Ok(vec![num(1.0)]);
    }
    Ok(vec![])
}

fn clf(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "clf")?;
    let g = graphics(it);
    let f = take_figure(g, &mut a);
    match a.as_slice() {
        [] => {}
        [p] if word(p).as_deref() == Some("reset") => {}
        _ => return Err(bad("clf", "expected clf, clf(fig) or clf reset")),
    }
    g.clf(f).map_err(gerr)?;
    let f = f.unwrap_or_else(|| g.gcf());
    out_handle(f, nargout)
}

fn cla(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "cla")?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    match a.as_slice() {
        [] => {}
        [p] if word(p).as_deref() == Some("reset") => {}
        _ => return Err(bad("cla", "expected cla, cla(ax) or cla reset")),
    }
    g.cla(ax).map_err(gerr)?;
    Ok(vec![])
}

/// `delete(h)` for graphics handles; anything else is the interpreter's
/// own `delete` (files, handle objects).
fn delete(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let handles: Option<Vec<Handle>> =
        if !a.is_empty() && a.iter().all(|v| matches!(v, Value::Num(_))) {
            a.iter()
                .map(handles_of)
                .collect::<Option<Vec<_>>>()
                .map(|v| v.concat())
        } else {
            None
        };
    match handles {
        Some(hs) => {
            let g = graphics(it);
            if hs.iter().any(|h| *h == Handle::ROOT || !g.is_valid(*h)) {
                return Err(err(
                    "MATLAB:hg:udd_interface:CannotDelete",
                    "delete: invalid or deleted graphics object",
                ));
            }
            for h in hs {
                // A parent deleted earlier in the list takes its children.
                if g.is_valid(h) {
                    g.delete(h).map_err(gerr)?;
                }
            }
            Ok(vec![])
        }
        None => match PLAIN_DELETE.get() {
            Some(plain) => plain(it, a, nargout),
            None => Ok(vec![]),
        },
    }
}

fn drawnow(_: &mut Interp, _: Vec<Value>, _: usize) -> R<Vec<Value>> {
    Ok(vec![])
}

// ----- line plots ---------------------------------------------------------------

#[derive(Clone, Copy)]
enum LineKind {
    Plot,
    Plot3,
    Log(bool, bool),
}

fn line_plot(
    it: &mut Interp,
    a: Vec<Value>,
    nargout: usize,
    fname: &str,
    kind: LineKind,
) -> R<Vec<Value>> {
    let mut a = to_props(&a, fname)?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let dims = if matches!(kind, LineKind::Plot3) {
        3
    } else {
        2
    };
    let (series, opts) = callargs::split_series(fname, &a, dims).map_err(gerr)?;
    let opts: Opts = opts
        .into_iter()
        .map(|(k, v)| {
            let v = handle_prop(&k, v);
            (k, v)
        })
        .collect();
    let hs = match kind {
        LineKind::Plot => g.plot(ax, &series, &opts),
        LineKind::Plot3 => g.plot3(ax, &series, &opts),
        LineKind::Log(x, y) => g.plot_log(ax, &series, x, y, &opts),
    }
    .map_err(gerr)?;
    out_handles(&hs, nargout)
}

fn plot(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    line_plot(it, a, n, "plot", LineKind::Plot)
}
fn plot3(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    line_plot(it, a, n, "plot3", LineKind::Plot3)
}
fn semilogx(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    line_plot(it, a, n, "semilogx", LineKind::Log(true, false))
}
fn semilogy(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    line_plot(it, a, n, "semilogy", LineKind::Log(false, true))
}
fn loglog(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    line_plot(it, a, n, "loglog", LineKind::Log(true, true))
}

/// `line(x, y[, z], ...)` or `line('XData', x, 'YData', y, ...)`.
fn line(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "line")?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let (data, rest) = split_numeric(&a);
    let mut opts = pairs("line", rest)?;
    let mut data: Vec<Vec<f64>> = data
        .iter()
        .map(|p| numbers(p, "line", "data"))
        .collect::<R<_>>()?;
    if data.is_empty() {
        for name in ["XData", "YData", "ZData"] {
            if let Some(p) = take_opt(&mut opts, name) {
                data.push(numbers(&p, "line", name)?);
            }
        }
    }
    let h = match data.as_slice() {
        [x, y] => g.line(ax, x, y, None, &opts),
        [x, y, z] => g.line(ax, x, y, Some(z), &opts),
        _ => return Err(bad("line", "expected line(x, y) or line(x, y, z)")),
    }
    .map_err(gerr)?;
    out_handle(h, nargout)
}

pub(crate) fn is_marker(s: &str) -> bool {
    matches!(
        s,
        "o" | "+"
            | "*"
            | "."
            | "x"
            | "s"
            | "d"
            | "^"
            | "v"
            | "<"
            | ">"
            | "p"
            | "h"
            | "square"
            | "diamond"
            | "pentagram"
            | "hexagram"
    )
}

pub(crate) fn is_color_word(s: &str) -> bool {
    indy_graphics::color::named(s).is_some()
}

/// `scatter(x, y[, s[, c]][, marker][, 'filled'], ...)`, `scatter3(x, y, z, ...)`.
fn scatter_impl(
    it: &mut Interp,
    a: Vec<Value>,
    nargout: usize,
    fname: &str,
    three: bool,
) -> R<Vec<Value>> {
    let mut a = to_props(&a, fname)?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let need = if three { 3 } else { 2 };
    let mut i = 0;
    let mut coords = Vec::new();
    while i < a.len()
        && coords.len() < need
        && matches!(a[i], PropValue::Num(_) | PropValue::Array(_))
    {
        coords.push(numbers(&a[i], fname, "data")?);
        i += 1;
    }
    if coords.len() < need {
        return Err(bad(fname, "not enough input arguments"));
    }
    let mut args = ScatterArgs {
        x: coords[0].clone(),
        y: coords[1].clone(),
        z: three.then(|| coords[2].clone()),
        ..Default::default()
    };
    if let Some(p @ (PropValue::Num(_) | PropValue::Array(_))) = a.get(i) {
        args.sizes = numbers(p, fname, "S")?;
        i += 1;
        if let Some(p) = a.get(i) {
            let colour = match p {
                PropValue::Num(_) | PropValue::Array(_) => true,
                PropValue::Str(s) => is_color_word(s) && !is_marker(s),
                _ => false,
            };
            if colour {
                args.color = Some(p.clone());
                i += 1;
            }
        }
    }
    while let Some(PropValue::Str(s)) = a.get(i) {
        let w = s.to_ascii_lowercase();
        if w == "filled" {
            args.filled = true;
        } else if is_marker(&w) && args.marker.is_none() {
            args.marker = Some(s.clone());
        } else if args.color.is_none() && is_color_word(&w) {
            args.color = Some(PropValue::Str(s.clone()));
        } else {
            break;
        }
        i += 1;
    }
    let opts = pairs(fname, &a[i..])?;
    let h = g.scatter(ax, &args, &opts).map_err(gerr)?;
    out_handle(h, nargout)
}

fn scatter(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    scatter_impl(it, a, n, "scatter", false)
}
fn scatter3(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    scatter_impl(it, a, n, "scatter3", true)
}

/// `bar(y)`, `bar(x, y)`, `bar(..., width)`, `bar(..., 'stacked')`,
/// `bar(..., color)`, then name/value pairs.
fn bar_impl(
    it: &mut Interp,
    a: Vec<Value>,
    nargout: usize,
    fname: &str,
    horizontal: bool,
) -> R<Vec<Value>> {
    let mut a = to_props(&a, fname)?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let (data, rest) = split_numeric(&a);
    let mut args = BarArgs::default();
    match data.as_slice() {
        [y] => args.y = array(y, fname, "Y")?,
        [y, PropValue::Num(w)] if array(y, fname, "Y")?.numel() > 1 => {
            args.y = array(y, fname, "Y")?;
            args.width = Some(*w);
        }
        [x, y] => {
            args.x = Some(numbers(x, fname, "X")?);
            args.y = array(y, fname, "Y")?;
        }
        [x, y, w] => {
            args.x = Some(numbers(x, fname, "X")?);
            args.y = array(y, fname, "Y")?;
            args.width = Some(scalar(w, fname, "WIDTH")?);
        }
        _ => return Err(bad(fname, "expected bar(y), bar(x, y) or bar(x, y, width)")),
    }
    let mut i = 0;
    while let Some(PropValue::Str(s)) = rest.get(i) {
        match s.to_ascii_lowercase().as_str() {
            "stacked" => args.stacked = true,
            "grouped" => args.stacked = false,
            "hist" | "histc" => return Err(not_yet(fname, format!("the '{s}' style"))),
            w if args.color.is_none() && is_color_word(w) => {
                args.color = Some(PropValue::Str(s.clone()))
            }
            _ => break,
        }
        i += 1;
    }
    let opts = pairs(fname, &rest[i..])?;
    let hs = g.bar(ax, &args, horizontal, &opts).map_err(gerr)?;
    out_handles(&hs, nargout)
}

fn bar(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    bar_impl(it, a, n, "bar", false)
}
fn barh(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    bar_impl(it, a, n, "barh", true)
}

fn series_plot(it: &mut Interp, a: Vec<Value>, nargout: usize, fname: &str) -> R<Vec<Value>> {
    let mut a = to_props(&a, fname)?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let filled = fname == "stem" && {
        let before = a.len();
        a.retain(|p| word(p).as_deref() != Some("filled"));
        a.len() != before
    };
    let (series, opts) = callargs::split_series(fname, &a, 2).map_err(gerr)?;
    let opts: Opts = opts
        .into_iter()
        .map(|(k, v)| {
            let v = handle_prop(&k, v);
            (k, v)
        })
        .collect();
    let [s] = series.as_slice() else {
        return Err(bad(fname, "expected one Y or X, Y data group"));
    };
    let hs = if fname == "stem" {
        g.stem(ax, s, filled, &opts)
    } else {
        g.stairs(ax, s, &opts)
    }
    .map_err(gerr)?;
    out_handles(&hs, nargout)
}

fn stairs(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    series_plot(it, a, n, "stairs")
}
fn stem(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    series_plot(it, a, n, "stem")
}

/// `area(y)`, `area(x, y)`, then name/value pairs.
fn area(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "area")?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let (data, rest) = split_numeric(&a);
    let opts = pairs("area", rest)?;
    let hs = match data.as_slice() {
        [y] => g.area(ax, None, &array(y, "area", "Y")?, &opts),
        [x, y] => {
            let x = numbers(x, "area", "X")?;
            g.area(ax, Some(&x), &array(y, "area", "Y")?, &opts)
        }
        [_, _, _] => return Err(not_yet("area", "the BASEVALUE argument")),
        _ => return Err(bad("area", "expected area(y) or area(x, y)")),
    }
    .map_err(gerr)?;
    out_handles(&hs, nargout)
}

/// `fill(x, y, c, ...)` with one or more triples, then name/value pairs.
fn fill(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "fill")?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let mut i = 0;
    let mut triples = Vec::new();
    while i + 3 <= a.len() && matches!(a[i], PropValue::Num(_) | PropValue::Array(_)) {
        let x = numbers(&a[i], "fill", "X")?;
        let y = numbers(&a[i + 1], "fill", "Y")?;
        triples.push((x, y, a[i + 2].clone()));
        i += 3;
    }
    if triples.is_empty() {
        return Err(bad("fill", "expected fill(x, y, c)"));
    }
    let opts = pairs("fill", &a[i..])?;
    // Later triples of one call add to the axes of the first, as in MATLAB.
    let mut hs = Vec::new();
    let held = g.ishold(ax);
    for (k, (x, y, c)) in triples.iter().enumerate() {
        let r = g.fill(ax, x, y, c, &opts);
        if k == 0 && triples.len() > 1 && r.is_ok() {
            g.hold(ax, Some(true)).map_err(gerr)?;
        }
        match r {
            Ok(h) => hs.push(h),
            Err(e) => {
                if triples.len() > 1 && k > 0 {
                    g.hold(ax, Some(held)).map_err(gerr)?;
                }
                return Err(gerr(e));
            }
        }
    }
    if triples.len() > 1 {
        g.hold(ax, Some(held)).map_err(gerr)?;
    }
    out_handles(&hs, nargout)
}

/// `patch(x, y, c, ...)` or `patch('Vertices', v, 'Faces', f, ...)`.
fn patch(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "patch")?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let (data, rest) = split_numeric(&a);
    let h = match data.as_slice() {
        [] => {
            let mut opts = pairs("patch", rest)?;
            let v = take_opt(&mut opts, "Vertices");
            let f = take_opt(&mut opts, "Faces");
            match (v, f) {
                (Some(v), Some(f)) => g.patch_faces(
                    ax,
                    array(&v, "patch", "Vertices")?,
                    array(&f, "patch", "Faces")?,
                    &opts,
                ),
                _ => {
                    return Err(not_yet(
                        "patch",
                        "this form (use x, y, c or Vertices/Faces)",
                    ));
                }
            }
        }
        [x, y] => {
            let (c, rest) = rest
                .split_first()
                .ok_or_else(|| bad("patch", "expected patch(x, y, c)"))?;
            let opts = pairs("patch", rest)?;
            g.patch(
                ax,
                &numbers(x, "patch", "X")?,
                &numbers(y, "patch", "Y")?,
                c,
                &opts,
            )
        }
        [x, y, c] => {
            let opts = pairs("patch", rest)?;
            g.patch(
                ax,
                &numbers(x, "patch", "X")?,
                &numbers(y, "patch", "Y")?,
                c,
                &opts,
            )
        }
        _ => return Err(not_yet("patch", "patch(x, y, z, c)")),
    }
    .map_err(gerr)?;
    out_handle(h, nargout)
}

// ----- surfaces, contours, images -------------------------------------------------

pub(crate) fn grid_args(data: &[PropValue], fname: &str, with_c: bool) -> R<GridArgs> {
    let arr = |p: &PropValue, what: &str| array(p, fname, what);
    Ok(match (data, with_c) {
        ([z], _) => GridArgs::z(arr(z, "Z")?),
        ([z, c], true) => GridArgs {
            c: Some(arr(c, "C")?),
            ..GridArgs::z(arr(z, "Z")?)
        },
        ([x, y, z], _) => GridArgs::xyz(arr(x, "X")?, arr(y, "Y")?, arr(z, "Z")?),
        ([x, y, z, c], true) => GridArgs {
            c: Some(arr(c, "C")?),
            ..GridArgs::xyz(arr(x, "X")?, arr(y, "Y")?, arr(z, "Z")?)
        },
        _ => return Err(bad(fname, "expected Z or X, Y, Z data")),
    })
}

fn surface(it: &mut Interp, a: Vec<Value>, nargout: usize, fname: &str) -> R<Vec<Value>> {
    let mut a = to_props(&a, fname)?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let (data, rest) = split_numeric(&a);
    let args = grid_args(&data, fname, true)?;
    let opts = pairs(fname, rest)?;
    let h = if fname == "mesh" {
        g.mesh(ax, &args, &opts)
    } else {
        g.surf(ax, &args, &opts)
    }
    .map_err(gerr)?;
    out_handle(h, nargout)
}

fn surf(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    surface(it, a, n, "surf")
}
fn mesh(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    surface(it, a, n, "mesh")
}

/// `contour(Z)`, `contour(Z, n|v)`, `contour(X, Y, Z[, n|v])`, then pairs.
fn contour_impl(
    it: &mut Interp,
    a: Vec<Value>,
    nargout: usize,
    fname: &str,
    filled: bool,
) -> R<Vec<Value>> {
    if nargout > 0 {
        return Err(not_yet(fname, "the contour matrix output"));
    }
    let mut a = to_props(&a, fname)?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let (mut data, rest) = split_numeric(&a);
    let levels = match data.len() {
        2 | 4 => {
            let v = numbers(&data.pop().unwrap_or(PropValue::Num(0.0)), fname, "levels")?;
            match v.as_slice() {
                [n] => Levels::Count(positive_int(*n, fname)?),
                [a, b] if a == b => Levels::Values(vec![*a]),
                _ => Levels::Values(v),
            }
        }
        _ => Levels::Auto,
    };
    let args = grid_args(&data, fname, false)?;
    let opts = pairs(fname, rest)?;
    g.contour(ax, &args, &levels, filled, &opts).map_err(gerr)?;
    Ok(vec![])
}

fn contour(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    contour_impl(it, a, n, "contour", false)
}
fn contourf(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    contour_impl(it, a, n, "contourf", true)
}

fn ends(p: &PropValue, fname: &str, what: &str) -> R<[f64; 2]> {
    let v = numbers(p, fname, what)?;
    match (v.first(), v.last()) {
        (Some(a), Some(b)) => Ok([*a, *b]),
        _ => Err(bad(fname, format!("{what} must not be empty"))),
    }
}

/// `image(C)`, `image(x, y, C)`; `imagesc` also takes colour limits last.
fn image_impl(
    it: &mut Interp,
    a: Vec<Value>,
    nargout: usize,
    fname: &str,
    scaled: bool,
) -> R<Vec<Value>> {
    let mut a = to_props(&a, fname)?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let (data, rest) = split_numeric(&a);
    let (x, y, c, clim) = match (data.as_slice(), scaled) {
        ([c], _) => (None, None, c, None),
        ([c, l], true) => (None, None, c, Some(pair(l, fname, "CLIM")?)),
        ([x, y, c], _) => (
            Some(ends(x, fname, "X")?),
            Some(ends(y, fname, "Y")?),
            c,
            None,
        ),
        ([x, y, c, l], true) => (
            Some(ends(x, fname, "X")?),
            Some(ends(y, fname, "Y")?),
            c,
            Some(pair(l, fname, "CLIM")?),
        ),
        _ => return Err(bad(fname, "expected C or x, y, C")),
    };
    let opts = pairs(fname, rest)?;
    let h = g
        .image(ax, x, y, array(c, fname, "C")?, scaled, &opts)
        .map_err(gerr)?;
    if let Some(l) = clim {
        let axh = g.ancestor_axes(h);
        g.caxis(axh, Some(l)).map_err(gerr)?;
    }
    out_handle(h, nargout)
}

fn image(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    image_impl(it, a, n, "image", false)
}
fn imagesc(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    image_impl(it, a, n, "imagesc", true)
}

// ----- text and annotation ------------------------------------------------------

/// `text(x, y[, z], str, ...)`.
fn text(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "text")?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let (data, rest) = split_numeric(&a);
    let pos: Vec<f64> = data
        .iter()
        .map(|p| scalar(p, "text", "position"))
        .collect::<R<_>>()?;
    let pos = match pos.as_slice() {
        [x, y] => [*x, *y, 0.0],
        [x, y, z] => [*x, *y, *z],
        _ => return Err(bad("text", "expected text(x, y, str) with scalar x and y")),
    };
    let (s, rest) = rest
        .split_first()
        .ok_or_else(|| bad("text", "the text argument is missing"))?;
    let opts = pairs("text", rest)?;
    let h = g.text(ax, pos, &lines(s, "text")?, &opts).map_err(gerr)?;
    out_handle(h, nargout)
}

fn label(
    it: &mut Interp,
    a: Vec<Value>,
    nargout: usize,
    fname: &str,
    role: LabelRole,
) -> R<Vec<Value>> {
    let mut a = to_props(&a, fname)?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let (s, rest) = a
        .split_first()
        .ok_or_else(|| bad(fname, "the text argument is missing"))?;
    let opts = pairs(fname, rest)?;
    let h = g.label(ax, role, &lines(s, fname)?, &opts).map_err(gerr)?;
    out_handle(h, nargout)
}

fn xlabel(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    label(it, a, n, "xlabel", LabelRole::XLabel)
}
fn ylabel(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    label(it, a, n, "ylabel", LabelRole::YLabel)
}
fn zlabel(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    label(it, a, n, "zlabel", LabelRole::ZLabel)
}
fn title(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    label(it, a, n, "title", LabelRole::Title)
}

/// `legend`, `legend(labels...)`, `legend({labels})`, `legend show/off`,
/// with an optional `'Location', loc`.
fn legend(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "legend")?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    if matches!(a.first(), Some(PropValue::Num(_) | PropValue::Array(_))) {
        return Err(not_yet("legend", "choosing the objects of a legend"));
    }
    if let [p] = a.as_slice() {
        match word(p).as_deref() {
            Some("off") | Some("hide") => {
                g.legend_visible(ax, false).map_err(gerr)?;
                return Ok(vec![]);
            }
            Some("show") => {
                g.legend_visible(ax, true).map_err(gerr)?;
                return Ok(vec![]);
            }
            Some("boxoff") | Some("boxon") | Some("toggle") => {
                return Err(not_yet(
                    "legend",
                    format!("legend {}", word(p).unwrap_or_default()),
                ));
            }
            _ => {}
        }
    }
    let mut labels: Vec<String> = Vec::new();
    let mut location = None;
    let mut i = 0;
    while i < a.len() {
        match &a[i] {
            PropValue::Str(s) if s.eq_ignore_ascii_case("location") && i + 1 < a.len() => {
                location = Some(
                    a[i + 1]
                        .as_str()
                        .ok_or_else(|| bad("legend", "Location must be text"))?
                        .to_string(),
                );
                i += 2;
            }
            PropValue::Str(s)
                if s.eq_ignore_ascii_case("orientation")
                    || s.eq_ignore_ascii_case("autoupdate") =>
            {
                return Err(not_yet("legend", format!("the '{s}' option")));
            }
            PropValue::Str(s) => {
                labels.push(s.clone());
                i += 1;
            }
            PropValue::Cell(c) => {
                labels.extend(c.iter().cloned());
                i += 1;
            }
            _ => return Err(bad("legend", "labels must be text")),
        }
    }
    let labels = (!a.is_empty() && !(labels.is_empty() && location.is_some())).then_some(labels);
    let h = g
        .legend(ax, labels.as_deref(), location.as_deref())
        .map_err(gerr)?;
    out_handle(h, nargout)
}

fn grid(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "grid")?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    match a.as_slice() {
        [] => g.grid(ax, None, false),
        [p] if word(p).as_deref() == Some("minor") => g.grid(ax, None, true),
        [p] => g.grid(ax, on_off(p, "grid")?, false),
        _ => return Err(bad("grid", "too many input arguments")),
    }
    .map_err(gerr)?;
    Ok(vec![])
}

fn box_(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "box")?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    match a.as_slice() {
        [] => g.box_(ax, None),
        [p] => g.box_(ax, on_off(p, "box")?),
        _ => return Err(bad("box", "too many input arguments")),
    }
    .map_err(gerr)?;
    Ok(vec![])
}

fn is_3d(g: &Graphics, ax: Handle) -> bool {
    match g.get(ax, "View").ok().and_then(|v| v.numbers()) {
        Some(v) => v.get(1).is_some_and(|el| *el != 90.0),
        None => false,
    }
}

/// `axis(limits)`, `axis tight/equal/...`, `v = axis`.
fn axis(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "axis")?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    if a.is_empty() {
        let ax = match ax {
            Some(h) => h,
            None => g.gca(),
        };
        let n = if is_3d(g, ax) { 3 } else { 2 };
        let mut v = Vec::new();
        for i in 0..n {
            v.extend(g.limits(Some(ax), i).map_err(gerr)?);
        }
        return Ok(vec![row(&v)]);
    }
    for p in &a {
        let cmd = match p {
            PropValue::Num(_) | PropValue::Array(_) => {
                AxisCmd::Limits(numbers(p, "axis", "LIMITS")?)
            }
            PropValue::Str(s) => match s.to_ascii_lowercase().as_str() {
                "tight" => AxisCmd::Tight,
                "equal" => AxisCmd::Equal,
                "normal" => AxisCmd::Normal,
                "auto" => AxisCmd::Auto,
                "off" => AxisCmd::Off,
                "on" => AxisCmd::On,
                "square" => AxisCmd::Square,
                "ij" => AxisCmd::Ij,
                "xy" => AxisCmd::Xy,
                "image" => AxisCmd::Image,
                "manual" => {
                    let axh = match ax {
                        Some(h) => h,
                        None => g.gca(),
                    };
                    for i in 0..3 {
                        let lim = g.limits(Some(axh), i).map_err(gerr)?;
                        g.set_limits(Some(axh), i, Some(lim)).map_err(gerr)?;
                    }
                    continue;
                }
                other => return Err(not_yet("axis", format!("axis {other}"))),
            },
            _ => return Err(bad("axis", "invalid argument")),
        };
        g.axis(ax, &cmd).map_err(gerr)?;
    }
    Ok(vec![])
}

fn limit_fn(it: &mut Interp, a: Vec<Value>, fname: &str, i: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, fname)?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let letter = fname[..1].to_ascii_uppercase();
    match a.as_slice() {
        [] => Ok(vec![row(&g.limits(ax, i).map_err(gerr)?)]),
        [p @ (PropValue::Num(_) | PropValue::Array(_))] => {
            g.set_limits(ax, i, Some(pair(p, fname, "limits")?))
                .map_err(gerr)?;
            Ok(vec![])
        }
        [p] => match word(p).as_deref() {
            Some("auto") => {
                g.set_limits(ax, i, None).map_err(gerr)?;
                Ok(vec![])
            }
            Some("manual") => {
                let lim = g.limits(ax, i).map_err(gerr)?;
                g.set_limits(ax, i, Some(lim)).map_err(gerr)?;
                Ok(vec![])
            }
            Some("mode") => {
                let axh = match ax {
                    Some(h) => h,
                    None => g.gca(),
                };
                let v = g.get(axh, &format!("{letter}LimMode")).map_err(gerr)?;
                Ok(vec![from_prop(v)])
            }
            _ => Err(bad(fname, "expected limits, 'auto', 'manual' or 'mode'")),
        },
        _ => Err(bad(fname, "too many input arguments")),
    }
}

fn xlim(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    limit_fn(it, a, "xlim", 0)
}
fn ylim(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    limit_fn(it, a, "ylim", 1)
}
fn zlim(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    limit_fn(it, a, "zlim", 2)
}

/// `caxis`/`clim`: query, `[lo hi]`, `'auto'`, `'manual'`.
fn caxis(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "caxis")?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let axh = match ax {
        Some(h) => h,
        None => g.gca(),
    };
    match a.as_slice() {
        [] => Ok(vec![from_prop(g.get(axh, "CLim").map_err(gerr)?)]),
        [p @ (PropValue::Num(_) | PropValue::Array(_))] => {
            g.caxis(Some(axh), Some(pair(p, "caxis", "limits")?))
                .map_err(gerr)?;
            Ok(vec![])
        }
        [p] => match word(p).as_deref() {
            Some("auto") => {
                g.caxis(Some(axh), None).map_err(gerr)?;
                Ok(vec![])
            }
            Some("manual") => {
                let l = numbers(&g.get(axh, "CLim").map_err(gerr)?, "caxis", "CLim")?;
                g.caxis(Some(axh), Some([l[0], l[1]])).map_err(gerr)?;
                Ok(vec![])
            }
            _ => Err(bad("caxis", "expected limits, 'auto' or 'manual'")),
        },
        _ => Err(bad("caxis", "too many input arguments")),
    }
}

fn map_value(map: &[[f64; 3]]) -> Value {
    let n = map.len();
    let mut data = vec![0.0; n * 3];
    for (r, rgb) in map.iter().enumerate() {
        for c in 0..3 {
            data[r + c * n] = rgb[c];
        }
    }
    Value::Num(Num::real(vec![n, 3], data))
}

fn map_rows(p: &PropValue) -> R<Vec<[f64; 3]>> {
    let a = array(p, "colormap", "MAP")?;
    if a.dims.len() != 2 || a.cols() != 3 {
        return Err(bad("colormap", "MAP must be an N×3 matrix"));
    }
    Ok((0..a.rows())
        .map(|r| [a.at(r, 0), a.at(r, 1), a.at(r, 2)])
        .collect())
}

/// `colormap`, `colormap(name)`, `colormap(map)`, `colormap(ax, ...)`.
fn colormap(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "colormap")?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let arg = match a.as_slice() {
        [] => None,
        [PropValue::Str(s)] if s.eq_ignore_ascii_case("default") => {
            Some(ColormapArg::Rows(default_colormap()))
        }
        [PropValue::Str(s)] => Some(ColormapArg::Name(s.to_ascii_lowercase(), None)),
        [p] => Some(ColormapArg::Rows(map_rows(p)?)),
        _ => return Err(bad("colormap", "too many input arguments")),
    };
    let query = arg.is_none();
    let map = g.colormap(ax, arg.as_ref()).map_err(gerr)?;
    if query || nargout > 0 {
        return Ok(vec![map_value(&map)]);
    }
    Ok(vec![])
}

macro_rules! colormap_fns {
    ($($name:literal),* $(,)?) => {
        &[$(($name, {
            fn f(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
                colormap_named(it, a, $name)
            }
            f as BuiltinFn
        })),*]
    };
}

/// `jet(n)`, `gray(n)`, ... (the names of `COLORMAP_NAMES`).
const COLORMAP_FNS: &[(&str, BuiltinFn)] = colormap_fns!(
    "viridis",
    "jet",
    "gray",
    "hot",
    "cool",
    "copper",
    "bone",
    "spring",
    "summer",
    "autumn",
    "winter",
    "turbo",
    "hsv",
    "pink",
    "lines",
    "colorcube",
    "prism",
    "flag",
    "white",
    "cubehelix",
    "ocean",
    "rainbow",
);

/// The named map with `n` rows; without `n`, the length of the current
/// figure's map (no figure is created).
fn colormap_named(it: &mut Interp, a: Vec<Value>, name: &str) -> R<Vec<Value>> {
    let a = to_props(&a, name)?;
    let g = graphics(it);
    let n = match a.as_slice() {
        [] => match g.current_figure() {
            Some(f) => match g.get(f, "Colormap") {
                Ok(PropValue::Array(m)) => m.rows(),
                _ => default_colormap().len(),
            },
            None => default_colormap().len(),
        },
        [p] => {
            let n = scalar(p, name, "N")?;
            if !(n.is_finite() && n >= 0.0 && n.fract() == 0.0 && n < 1e7) {
                return Err(bad(name, "N must be a non-negative integer"));
            }
            n as usize
        }
        _ => return Err(bad(name, "too many input arguments")),
    };
    let map = colormap_by_name(name, n).ok_or_else(|| bad(name, "unknown colormap"))?;
    Ok(vec![map_value(&map)])
}

/// `colorbar`, `colorbar(loc)`, `colorbar('Location', loc)`, `colorbar off`.
fn colorbar(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "colorbar")?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let location = match a.as_slice() {
        [] => None,
        [p] if matches!(
            word(p).as_deref(),
            Some("off") | Some("delete") | Some("hide")
        ) =>
        {
            g.colorbar_off(ax).map_err(gerr)?;
            return Ok(vec![]);
        }
        [PropValue::Str(s)] => Some(s.clone()),
        [PropValue::Str(k), PropValue::Str(loc)] if k.eq_ignore_ascii_case("location") => {
            Some(loc.clone())
        }
        _ => return Err(not_yet("colorbar", "these colorbar options")),
    };
    let h = g.colorbar(ax, location.as_deref()).map_err(gerr)?;
    out_handle(h, nargout)
}

/// `daspect(ratio)`, `daspect('auto'|'manual')`, `r = daspect`.
fn daspect(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    aspect(it, a, "daspect", "DataAspectRatio")
}

/// `pbaspect(ratio)`, `pbaspect('auto'|'manual')`, `r = pbaspect`.
fn pbaspect(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    aspect(it, a, "pbaspect", "PlotBoxAspectRatio")
}

fn aspect(it: &mut Interp, a: Vec<Value>, fname: &str, prop: &str) -> R<Vec<Value>> {
    let mut a = to_props(&a, fname)?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let ax = match ax {
        Some(h) => h,
        None => g.gca(),
    };
    match a.as_slice() {
        [] => {
            let v = numbers(&g.get(ax, prop).map_err(gerr)?, fname, prop)?;
            Ok(vec![row(&v)])
        }
        [PropValue::Str(s)] if s.eq_ignore_ascii_case("mode") => Ok(vec![from_prop(
            g.get(ax, &format!("{prop}Mode")).map_err(gerr)?,
        )]),
        [PropValue::Str(s)] => {
            g.set(ax, &format!("{prop}Mode"), PropValue::Str(s.clone()))
                .map_err(gerr)?;
            Ok(vec![])
        }
        [p] => {
            let v = numbers(p, fname, "the ratio")?;
            if v.len() != 3 {
                return Err(bad(fname, "the ratio must have 3 elements"));
            }
            g.set(ax, prop, PropValue::from([v[0], v[1], v[2]]))
                .map_err(gerr)?;
            Ok(vec![])
        }
        _ => Err(bad(fname, "too many input arguments")),
    }
}

/// `view(az, el)`, `view([az el])`, `view(2)`, `view(3)`, `[az, el] = view`.
fn view(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "view")?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let az_el = match a.as_slice() {
        [] => {
            let axh = match ax {
                Some(h) => h,
                None => g.gca(),
            };
            let v = numbers(&g.get(axh, "View").map_err(gerr)?, "view", "View")?;
            if nargout >= 2 {
                return Ok(vec![num(v[0]), num(v[1])]);
            }
            return Ok(vec![row(&v)]);
        }
        [p] => match numbers(p, "view", "the view")?.as_slice() {
            [d] if *d == 2.0 => [0.0, 90.0],
            [d] if *d == 3.0 => [-37.5, 30.0],
            [az, el] => [*az, *el],
            _ => return Err(bad("view", "expected view(2), view(3) or view([az el])")),
        },
        [az, el] => [scalar(az, "view", "AZ")?, scalar(el, "view", "EL")?],
        _ => return Err(bad("view", "too many input arguments")),
    };
    g.view(ax, az_el).map_err(gerr)?;
    Ok(vec![])
}

fn tick_fn(it: &mut Interp, a: Vec<Value>, fname: &str, prop: &str) -> R<Vec<Value>> {
    let mut a = to_props(&a, fname)?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let axh = match ax {
        Some(h) => h,
        None => g.gca(),
    };
    match a.as_slice() {
        [] => Ok(vec![from_prop(g.get(axh, prop).map_err(gerr)?)]),
        [p] => {
            match word(p).as_deref() {
                Some("auto") | Some("manual") => g
                    .set(axh, &format!("{prop}Mode"), p.clone())
                    .map_err(gerr)?,
                Some("mode") => {
                    return Ok(vec![from_prop(
                        g.get(axh, &format!("{prop}Mode")).map_err(gerr)?,
                    )]);
                }
                _ => g.set(axh, prop, p.clone()).map_err(gerr)?,
            }
            Ok(vec![])
        }
        _ => Err(bad(fname, "too many input arguments")),
    }
}

fn xticks(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    tick_fn(it, a, "xticks", "XTick")
}
fn yticks(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    tick_fn(it, a, "yticks", "YTick")
}
fn zticks(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    tick_fn(it, a, "zticks", "ZTick")
}
fn xticklabels(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    tick_fn(it, a, "xticklabels", "XTickLabel")
}
fn yticklabels(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    tick_fn(it, a, "yticklabels", "YTickLabel")
}

// ----- handles and properties -----------------------------------------------------

pub(crate) fn handle_list(v: &Value, fname: &str) -> R<Vec<Handle>> {
    handles_of(v).ok_or_else(|| {
        err(
            "MATLAB:hg:InvalidHandle",
            format!("{fname}: the first argument must be a graphics handle"),
        )
    })
}

/// `get(h, name)`; a vector of handles gives a cell column.
fn get(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let Some(first) = a.first() else {
        return Err(bad("get", "not enough input arguments"));
    };
    let hs = handle_list(first, "get")?;
    let name = match a.get(1) {
        Some(v) => v
            .to_str()
            .filter(|_| v.is_char() || v.is_string_row())
            .ok_or_else(|| not_yet("get", "a list of property names"))?,
        None => return Err(not_yet("get", "listing all properties (get(h))")),
    };
    if a.len() > 2 {
        return Err(bad("get", "too many input arguments"));
    }
    let mut values = Vec::with_capacity(hs.len());
    for h in &hs {
        values.push(get_value(it, *h, &name)?);
    }
    match values.len() {
        1 => Ok(values),
        _ => Ok(vec![Value::cell_col(values)]),
    }
}

/// One property of one object as an interpreter value (`get(h, name)`,
/// `h.Name`).
pub(crate) fn get_value(it: &mut Interp, h: Handle, name: &str) -> R<Value> {
    if let Some(f) = crate::lane_a::function_of(it, h, name) {
        return Ok(f);
    }
    if name.eq_ignore_ascii_case("UserData") {
        return crate::handles::user_data(it, h);
    }
    Ok(from_prop(graphics(it).get(h, name).map_err(gerr)?))
}

/// `set(h, name, value, ...)` on one or more handles.
pub(crate) fn set(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let Some((first, rest)) = a.split_first() else {
        return Err(bad("set", "not enough input arguments"));
    };
    let hs = handle_list(first, "set")?;
    if rest.is_empty() {
        return Err(not_yet("set", "listing settable values (set(h))"));
    }
    // `UserData` holds any value: it bypasses the property conversion.
    let (user, rest) = crate::handles::split_user_data(it, &hs, rest)?;
    let rest = rest.as_slice();
    if rest.is_empty() {
        return crate::handles::store_user_data(it, &hs, user);
    }
    let raw: Vec<(String, Value)> = rest
        .chunks(2)
        .filter_map(|p| Some((p[0].to_str()?, p.get(1)?.clone())))
        .collect();
    let rest = to_props(rest, "set")?;
    let opts = pairs("set", &rest)?;
    for &h in &hs {
        if !crate::charts::set_histogram(it, h, &raw)? {
            set_all(graphics(it), h, &opts)?;
        }
    }
    crate::handles::store_user_data(it, &hs, user)
}

fn validity(it: &mut Interp, a: &[Value], fname: &str, kind: Option<&str>) -> R<Vec<Value>> {
    let Some(v) = a.first() else {
        return Err(bad(fname, "not enough input arguments"));
    };
    let g = graphics(it);
    let ok = |x: f64| {
        Handle::from_value(x).is_some_and(|h| {
            g.is_valid(h)
                && kind.is_none_or(|k| g.type_of(h).is_ok_and(|t| t.eq_ignore_ascii_case(k)))
        })
    };
    match (v, v.to_f64_vec()) {
        (Value::Num(n), Some(xs)) => {
            let data = xs.into_iter().map(ok).collect();
            Ok(vec![Value::Bool(indy_value::Array::new(
                n.dims.clone(),
                data,
            ))])
        }
        _ => Ok(vec![Value::bool(false)]),
    }
}

fn ishandle(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    validity(it, &a, "ishandle", None)
}

fn isgraphics(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let kind = match a.get(1) {
        Some(v) => Some(
            v.to_str()
                .ok_or_else(|| bad("isgraphics", "TYPE must be text"))?,
        ),
        None => None,
    };
    validity(it, &a, "isgraphics", kind.as_deref())
}

fn matches_prop(g: &Graphics, h: Handle, name: &str, want: &PropValue) -> bool {
    let Ok(got) = g.get(h, name) else {
        return false;
    };
    match (&got, want) {
        (PropValue::Str(a), PropValue::Str(b)) => a.eq_ignore_ascii_case(b),
        _ => match (got.numbers(), want.numbers()) {
            (Some(a), Some(b)) => a == b,
            _ => from_prop(got.clone()) == from_prop(want.clone()),
        },
    }
}

/// `findobj([h,] 'Name', value, ...)`: the objects (the start objects and
/// their descendants, depth first) whose properties all match.
fn findobj(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let g = graphics(it);
    let (starts, rest) = match a.first() {
        Some(v @ Value::Num(_)) => (handle_list(v, "findobj")?, &a[1..]),
        _ => (vec![Handle::ROOT], &a[..]),
    };
    let rest = to_props(rest, "findobj")?;
    if let Some(PropValue::Str(s)) = rest.first()
        && s.starts_with('-')
    {
        return Err(not_yet("findobj", format!("the '{s}' option")));
    }
    let criteria = pairs("findobj", &rest)?;
    let mut found = Vec::new();
    let mut stack: Vec<Handle> = starts.into_iter().rev().collect();
    while let Some(h) = stack.pop() {
        if !g.is_valid(h) {
            continue;
        }
        if criteria.iter().all(|(k, v)| matches_prop(g, h, k, v)) {
            found.push(h);
        }
        if let Ok(PropValue::Handles(children)) = g.get(h, "Children") {
            stack.extend(children.into_iter().rev());
        }
    }
    if found.is_empty() {
        return Ok(vec![Value::Num(Num::real(vec![0, 1], vec![]))]);
    }
    Ok(vec![handles_value(&found)])
}

/// `peaks`, `peaks(n)`, `peaks(v)`, `peaks(X, Y)`; `[X, Y, Z] = peaks(...)`.
fn peaks(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let a = to_props(&a, "peaks")?;
    let (x, y, dims) = match a.as_slice() {
        [] | [PropValue::Num(_)] => {
            let n = match a.first() {
                Some(p) => positive_int(scalar(p, "peaks", "N")?, "peaks")?,
                None => 49,
            };
            let v: Vec<f64> = (0..n)
                .map(|k| {
                    if n == 1 {
                        -3.0
                    } else {
                        -3.0 + 6.0 * k as f64 / (n - 1) as f64
                    }
                })
                .collect();
            mesh_xy(&v, &v)
        }
        [p] => {
            let v = numbers(p, "peaks", "V")?;
            mesh_xy(&v, &v)
        }
        [px, py] => {
            let xa = array(px, "peaks", "X")?;
            let ya = array(py, "peaks", "Y")?;
            if xa.dims != ya.dims {
                return Err(bad("peaks", "X and Y must have the same size"));
            }
            (xa.data, ya.data, xa.dims)
        }
        _ => return Err(bad("peaks", "too many input arguments")),
    };
    let z: Vec<f64> = x
        .iter()
        .zip(&y)
        .map(|(&x, &y)| {
            3.0 * (1.0 - x).powi(2) * (-(x * x) - (y + 1.0).powi(2)).exp()
                - 10.0 * (x / 5.0 - x.powi(3) - y.powi(5)) * (-x * x - y * y).exp()
                - 1.0 / 3.0 * (-(x + 1.0).powi(2) - y * y).exp()
        })
        .collect();
    let m = |d: Vec<f64>| Value::Num(Num::real(dims.clone(), d));
    if nargout <= 1 {
        return Ok(vec![m(z)]);
    }
    Ok(vec![m(x), m(y), m(z)])
}

/// `meshgrid(xv, yv)` as column-major data with its dims.
pub(crate) fn mesh_xy(xv: &[f64], yv: &[f64]) -> (Vec<f64>, Vec<f64>, Vec<usize>) {
    let (r, c) = (yv.len(), xv.len());
    let mut x = Vec::with_capacity(r * c);
    let mut y = Vec::with_capacity(r * c);
    for &xj in xv {
        for &yi in yv {
            x.push(xj);
            y.push(yi);
        }
    }
    (x, y, vec![r, c])
}
