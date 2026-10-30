//! `errorbar`, `quiver`, `fplot`, `polarplot`, `polar`: MATLAB argument
//! parsing over `indy_graphics::lines2`.
//!
//! `fplot` evaluates the function handle here (vectorised first; if that
//! errors or returns the wrong number of values, element by element) and
//! hands the adaptive samples to the model. The handles themselves stay in
//! this crate so `get(h,'Function')` returns the function handle.

use std::collections::HashMap;

use indy_graphics::lines2::{ErrorBarArgs, FunctionInfo, QuiverArgs, fplot_sample};
use indy_graphics::{Array, Handle, PropValue, callargs, linespec};
use indy_interp::{Interp, R, Value};

use crate::builtins::{
    Opts, bad, not_yet, out_handle, out_handles, pairs, split_numeric, take_axes, take_opt, word,
};
use crate::convert::{array, gerr, to_prop, to_props};
use crate::graphics;

/// Leading LineSpec / keyword strings of a call: consumed while they are a
/// keyword in `keywords` or (once) a LineSpec.
fn specs_and_keywords(
    rest: &[PropValue],
    keywords: &[&str],
) -> (Option<String>, Vec<String>, usize) {
    let mut spec = None;
    let mut words = Vec::new();
    let mut i = 0;
    while let Some(PropValue::Str(s)) = rest.get(i) {
        let low = s.to_ascii_lowercase();
        if keywords.contains(&low.as_str()) {
            words.push(low);
        } else if spec.is_none() && matches!(linespec::parse(s), Ok(Some(_))) {
            spec = Some(s.clone());
        } else {
            break;
        }
        i += 1;
    }
    (spec, words, i)
}

fn arrays(nums: &[PropValue], fname: &str) -> R<Vec<Array>> {
    nums.iter().map(|p| array(p, fname, "data")).collect()
}

/// `errorbar(y,err)`, `errorbar(x,y,err)`, `errorbar(x,y,neg,pos)`,
/// `errorbar(x,y,yneg,ypos,xneg,xpos)`, `errorbar(...,ornt)` with
/// `'vertical'|'horizontal'|'both'`, a LineSpec, name/value, `ax` first.
pub(crate) fn errorbar(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "errorbar")?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let (nums, rest) = split_numeric(&a);
    let (spec, words, used) = specs_and_keywords(rest, &["vertical", "horizontal", "both"]);
    if words.len() > 1 {
        return Err(bad("errorbar", "give at most one orientation"));
    }
    let orient = words.first().map(String::as_str).unwrap_or("vertical");
    let opts = pairs("errorbar", &rest[used..])?;
    let n = arrays(&nums, "errorbar")?;
    let mut args = ErrorBarArgs {
        spec,
        ..ErrorBarArgs::default()
    };
    let (neg, pos) = match n.as_slice() {
        [y, e] => {
            args.y = y.clone();
            (e.clone(), e.clone())
        }
        [x, y, e] => {
            args.x = Some(x.clone());
            args.y = y.clone();
            (e.clone(), e.clone())
        }
        [x, y, lo, hi] => {
            args.x = Some(x.clone());
            args.y = y.clone();
            (lo.clone(), hi.clone())
        }
        [x, y, yn, yp, xn, xp] => {
            if !words.is_empty() {
                return Err(bad(
                    "errorbar",
                    "an orientation cannot be combined with yneg, ypos, xneg, xpos",
                ));
            }
            args.x = Some(x.clone());
            args.y = y.clone();
            args.x_neg = Some(xn.clone());
            args.x_pos = Some(xp.clone());
            (yn.clone(), yp.clone())
        }
        [] | [_] => return Err(bad("errorbar", "not enough input arguments")),
        _ => {
            return Err(bad(
                "errorbar",
                "expected errorbar(y,err), (x,y,err), (x,y,neg,pos) or (x,y,yneg,ypos,xneg,xpos)",
            ));
        }
    };
    if n.len() < 6 {
        if orient != "horizontal" {
            args.y_neg = Some(neg.clone());
            args.y_pos = Some(pos.clone());
        }
        if orient != "vertical" {
            args.x_neg = Some(neg);
            args.x_pos = Some(pos);
        }
    } else {
        args.y_neg = Some(neg);
        args.y_pos = Some(pos);
    }
    let hs = graphics(it).errorbar(ax, &args, &opts).map_err(gerr)?;
    out_handles(&hs, nargout)
}

/// `quiver(U,V)`, `quiver(X,Y,U,V)`, `quiver(...,scale)`, `quiver(...,'off')`,
/// LineSpec, `'filled'`, name/value, `ax` first.
pub(crate) fn quiver(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "quiver")?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let (nums, rest) = split_numeric(&a);
    let (spec, words, used) = specs_and_keywords(rest, &["filled", "off"]);
    let opts = pairs("quiver", &rest[used..])?;
    let n = arrays(&nums, "quiver")?;
    let scalar_of = |s: &Array| -> R<f64> {
        if s.numel() == 1 {
            Ok(s.data[0])
        } else {
            Err(bad("quiver", "the scale must be a scalar"))
        }
    };
    let mut args = QuiverArgs {
        spec,
        filled: words.iter().any(|w| w == "filled"),
        ..QuiverArgs::default()
    };
    match n.as_slice() {
        [u, v] => (args.u, args.v) = (u.clone(), v.clone()),
        [u, v, s] => {
            (args.u, args.v) = (u.clone(), v.clone());
            args.scale = Some(scalar_of(s)?);
        }
        [x, y, u, v] => {
            (args.x, args.y) = (Some(x.clone()), Some(y.clone()));
            (args.u, args.v) = (u.clone(), v.clone());
        }
        [x, y, u, v, s] => {
            (args.x, args.y) = (Some(x.clone()), Some(y.clone()));
            (args.u, args.v) = (u.clone(), v.clone());
            args.scale = Some(scalar_of(s)?);
        }
        [x, y, z, ..] if n.len() >= 6 => {
            let _ = (x, y, z);
            return Err(not_yet("quiver", "3-D arrows (use quiver3)"));
        }
        _ => {
            return Err(bad(
                "quiver",
                "expected quiver(U,V), quiver(X,Y,U,V) or a trailing scale",
            ));
        }
    }
    if words.iter().any(|w| w == "off") {
        args.scale = Some(0.0);
    }
    let h = graphics(it).quiver(ax, &args, &opts).map_err(gerr)?;
    out_handle(h, nargout)
}

// ----- fplot -------------------------------------------------------------------------

/// The function handles of fplot lines (`get(h,'Function')`).
#[derive(Default)]
struct Functions(HashMap<Handle, (Value, Option<Value>)>);

/// `get(h, 'Function' | 'XFunction' | 'YFunction')` of an fplot line, as
/// the function handle it was made from. `None` for anything else.
pub(crate) fn function_of(it: &mut Interp, h: Handle, name: &str) -> Option<Value> {
    let key = name.to_ascii_lowercase();
    let kind = graphics(it).type_of(h).ok()?;
    let (x, y) = it.ext::<Functions>()?.0.get(&h)?.clone();
    match (kind, key.as_str()) {
        ("functionline", "function") => Some(x),
        ("parameterizedfunctionline", "xfunction") => Some(x),
        ("parameterizedfunctionline", "yfunction") => y,
        _ => None,
    }
}

fn values_of(v: &Value, n: usize) -> Option<Vec<f64>> {
    let out = match v {
        Value::Num(num) => {
            let mut re = num.re.clone();
            if let Some(im) = &num.im {
                // Complex values are not real points of the curve.
                for (r, i) in re.iter_mut().zip(im) {
                    if *i != 0.0 {
                        *r = f64::NAN;
                    }
                }
            }
            re
        }
        Value::Bool(_) => v.to_f64_vec()?,
        _ => return None,
    };
    (out.len() == n).then_some(out)
}

/// `f(t)` for every t: one vectorised call, else element by element.
fn evaluate(it: &mut Interp, f: &Value, t: &[f64]) -> R<Vec<f64>> {
    if let Ok(out) = it.call_value(f, vec![Value::row(t.to_vec())], 1)
        && let Some(v) = out.first().and_then(|v| values_of(v, t.len()))
    {
        return Ok(v);
    }
    let mut ys = Vec::with_capacity(t.len());
    for &ti in t {
        let out = it.call_value(f, vec![Value::num(ti)], 1)?;
        let y = out.first().and_then(|v| values_of(v, 1)).ok_or_else(|| {
            bad(
                "fplot",
                "the function must return one real number per input",
            )
        })?;
        ys.push(y[0]);
    }
    Ok(ys)
}

fn function_text(f: &Value) -> String {
    match f {
        Value::Func(h) => indy_value::format::handle_text(h),
        _ => String::new(),
    }
}

/// `fplot(f)`, `fplot(f,[xmin xmax])`, `fplot(xt,yt[,[tmin tmax]])`, a
/// LineSpec, name/value (`MeshDensity`, line properties), `ax` first.
pub(crate) fn fplot(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let mut a = a;
    let ax = match a.first() {
        Some(v @ Value::Num(_)) => {
            let mut head = vec![to_prop(v, "fplot")?];
            let ax = take_axes(graphics(it), &mut head);
            if ax.is_some() {
                a.remove(0);
            }
            ax
        }
        _ => None,
    };
    let f = match a.first() {
        Some(f @ Value::Func(_)) => f.clone(),
        Some(v) if v.is_char() || v.is_string_row() => {
            return Err(not_yet(
                "fplot",
                "a function given as text (use a function handle)",
            ));
        }
        Some(Value::Cell(_)) | Some(Value::Object(_)) => {
            return Err(not_yet(
                "fplot",
                "several functions or symbolic expressions",
            ));
        }
        Some(_) => return Err(bad("fplot", "the first argument must be a function handle")),
        None => return Err(bad("fplot", "not enough input arguments")),
    };
    let mut i = 1;
    let yf = match a.get(1) {
        Some(g @ Value::Func(_)) => {
            i = 2;
            Some(g.clone())
        }
        _ => None,
    };
    let mut range = [-5.0, 5.0];
    if let Some(v @ (Value::Num(_) | Value::Bool(_))) = a.get(i) {
        match v.to_f64_vec().as_deref() {
            Some([lo, hi]) if lo.is_finite() && hi.is_finite() && lo < hi => range = [*lo, *hi],
            Some([_, _]) => return Err(bad("fplot", "the interval must be finite and increasing")),
            _ => return Err(bad("fplot", "the interval must have two elements")),
        }
        i += 1;
    }
    let rest = to_props(&a[i..], "fplot")?;
    let (spec, _, used) = specs_and_keywords(&rest, &[]);
    let mut opts: Opts = pairs("fplot", &rest[used..])?;
    let mut density = 23.0;
    if let Some(p) = take_opt(&mut opts, "MeshDensity") {
        match p.numbers().as_deref() {
            Some([d]) if d.is_finite() && *d >= 2.0 && d.fract() == 0.0 && *d <= 1500.0 => {
                density = *d
            }
            _ => {
                return Err(bad(
                    "fplot",
                    "MeshDensity must be an integer from 2 to 1500",
                ));
            }
        }
    }
    for name in [
        "ShowPoles",
        "XRange",
        "TRange",
        "Function",
        "XFunction",
        "YFunction",
    ] {
        if take_opt(&mut opts, name).is_some() {
            return Err(not_yet("fplot", format!("the '{name}' option")));
        }
    }
    let points = {
        let mut eval = |t: &[f64]| -> R<Vec<[f64; 2]>> {
            match &yf {
                None => {
                    let y = evaluate(it, &f, t)?;
                    Ok(t.iter().zip(y).map(|(t, y)| [*t, y]).collect())
                }
                Some(g) => {
                    let x = evaluate(it, &f, t)?;
                    let y = evaluate(it, g, t)?;
                    Ok(x.into_iter().zip(y).map(|(x, y)| [x, y]).collect())
                }
            }
        };
        fplot_sample(range, density as usize, &mut eval)?
    };
    let info = FunctionInfo {
        function: function_text(&f),
        y_function: yf.as_ref().map(function_text),
        range,
        mesh_density: density,
        spec,
    };
    let x = points.iter().map(|p| p[0]).collect();
    let y = points.iter().map(|p| p[1]).collect();
    let h = graphics(it)
        .function_line(ax, x, y, &info, &opts)
        .map_err(gerr)?;
    it.ext_mut::<Functions>().0.insert(h, (f, yf));
    out_handle(h, nargout)
}

// ----- polar -------------------------------------------------------------------------

fn polar_common(it: &mut Interp, a: Vec<Value>, nargout: usize, fname: &str) -> R<Vec<Value>> {
    let mut a = to_props(&a, fname)?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    if a.first().and_then(word).is_some() {
        return Err(bad(fname, "the data must come first"));
    }
    let (series, opts) = callargs::split_series(fname, &a, 2).map_err(gerr)?;
    if fname == "polar" && (!opts.is_empty() || series.len() > 1) {
        return Err(bad(
            "polar",
            "expected polar(theta, rho[, LineSpec]); use polarplot for more",
        ));
    }
    let hs = g.polar_plot(ax, fname, &series, &opts).map_err(gerr)?;
    out_handles(&hs, nargout)
}

/// `polarplot(theta,rho[,LineSpec],...)`, `polarplot(rho)`, name/value.
pub(crate) fn polarplot(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    polar_common(it, a, nargout, "polarplot")
}

/// `polar(theta, rho[, LineSpec])` (the older function).
pub(crate) fn polar(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    polar_common(it, a, nargout, "polar")
}
