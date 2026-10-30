//! Builtins of the surface family: `shading imshow contour3 surfc meshc
//! bar3` over `indy_graphics::surf2`. Argument rules follow MATLAB;
//! unsupported forms raise `IndyR:graphicsUnavailable`.

use indy_graphics::linespec;
use indy_graphics::surf2::default_range;
use indy_graphics::{
    Array, Bar3Args, Bar3Style, ImshowArgs, ImshowKind, Levels, NumClass, PropValue, Shading,
};
use indy_interp::{Interp, R, Value};

use crate::builtins::{
    Opts, bad, grid_args, is_color_word, not_yet, out_handle, out_handles, pairs, positive_int,
    split_numeric, take_axes, word,
};
use crate::convert::{array, gerr, handle_prop, numbers, to_props};
use crate::graphics;

/// `shading flat|interp|faceted`, `shading(ax, mode)`.
pub(crate) fn shading(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    if nargout > 0 {
        return Err(bad("shading", "function called with too many outputs"));
    }
    let mut a = to_props(&a, "shading")?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let [mode] = a.as_slice() else {
        return Err(bad("shading", "usage: shading(MODE) or shading(AX, MODE)"));
    };
    let name = mode
        .as_str()
        .ok_or_else(|| bad("shading", "MODE must be 'flat', 'interp' or 'faceted'"))?;
    let mode =
        Shading::parse(name).ok_or_else(|| bad("shading", format!("invalid MODE \"{name}\"")))?;
    g.shading(ax, mode).map_err(gerr)?;
    Ok(vec![])
}

fn colormap_of(p: &PropValue, what: &str) -> R<Vec<[f64; 3]>> {
    let m = array(p, "imshow", what)?;
    if m.dims.len() != 2 || m.cols() != 3 || m.rows() == 0 {
        return Err(bad("imshow", format!("{what} must be an N×3 colormap")));
    }
    Ok((0..m.rows())
        .map(|r| [m.at(r, 0), m.at(r, 1), m.at(r, 2)])
        .collect())
}

fn ends(p: &PropValue, what: &str) -> R<[f64; 2]> {
    let v = numbers(p, "imshow", what)?;
    match (v.first(), v.last()) {
        (Some(a), Some(b)) if v.len() <= 2 => Ok([*a, *b]),
        _ => Err(bad("imshow", format!("{what} must be a 2-element vector"))),
    }
}

/// `imshow(I)`, `imshow(I, [low high])`, `imshow(I, [])`, `imshow(RGB)`,
/// `imshow(BW)`, `imshow(X, map)`, `imshow(ax, ...)` and the options
/// `DisplayRange Colormap XData YData Parent InitialMagnification Border`.
pub(crate) fn imshow(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let logical = |v: &Value| matches!(v, Value::Bool(_));
    let mut vals = a;
    let mut a = to_props(&vals, "imshow")?;
    let g = graphics(it);
    let mut ax = take_axes(g, &mut a);
    if a.len() < vals.len() {
        vals.remove(0);
    }
    let Some(first) = a.first() else {
        return Err(bad("imshow", "not enough input arguments"));
    };
    if matches!(first, PropValue::Str(_)) {
        return Err(not_yet("imshow", "reading an image file"));
    }
    let is_bool = vals.first().is_some_and(logical);
    let cdata = array(first, "imshow", "the image")?;
    let class_ok = is_bool || default_range(cdata.class).is_some();
    if !class_ok {
        return Err(bad(
            "imshow",
            format!("images of class {} are not supported", cdata.class.name()),
        ));
    }
    let rgb = cdata.dims.len() == 3 && cdata.dims[2] == 3;
    if cdata.dims.len() > 2 && !rgb {
        return Err(bad("imshow", "the image must be M×N or M×N×3"));
    }
    let mut range: Option<Option<[f64; 2]>> = None; // Some(None) = `[]`
    let mut map: Option<Vec<[f64; 3]>> = None;
    let mut i = 1;
    if let Some(p @ (PropValue::Num(_) | PropValue::Array(_))) = a.get(1) {
        i = 2;
        let arr = array(p, "imshow", "the second argument")?;
        if arr.is_empty() {
            range = Some(None);
        } else if arr.numel() == 2 && arr.dims.len() == 2 && arr.rows() == 1 {
            range = Some(Some([arr.data[0], arr.data[1]]));
        } else if arr.dims.len() == 2 && arr.cols() == 3 {
            map = Some(colormap_of(p, "MAP")?);
        } else {
            return Err(bad(
                "imshow",
                "the second argument must be [LOW HIGH], [] or a colormap",
            ));
        }
    }
    let rest = &a[i..];
    if rest.len() % 2 != 0 {
        return Err(bad("imshow", "name/value arguments must come in pairs"));
    }
    let (mut x, mut y, mut cmap) = (None, None, None);
    for pair in rest.chunks(2) {
        let key = word(&pair[0]).ok_or_else(|| bad("imshow", "option names must be text"))?;
        let v = &pair[1];
        match key.as_str() {
            "displayrange" => {
                let arr = array(v, "imshow", "DisplayRange")?;
                range = Some(match arr.data.as_slice() {
                    [] => None,
                    [lo, hi] => Some([*lo, *hi]),
                    _ => return Err(bad("imshow", "DisplayRange must be [LOW HIGH] or []")),
                });
            }
            "colormap" => cmap = Some(colormap_of(v, "Colormap")?),
            "xdata" => x = Some(ends(v, "XData")?),
            "ydata" => y = Some(ends(v, "YData")?),
            "parent" => match handle_prop("Parent", v.clone()) {
                PropValue::Handle(h) if g.type_of(h).ok() == Some("axes") => ax = Some(h),
                _ => return Err(bad("imshow", "Parent must be an axes handle")),
            },
            // The browser view always fits the image to the figure panel,
            // which is what 'fit' asks for; a fixed magnification is not.
            "initialmagnification" => {
                if word(v).as_deref() != Some("fit") {
                    return Err(not_yet("imshow", "a numeric 'InitialMagnification'"));
                }
            }
            "border" => match word(v).as_deref() {
                Some("loose") => {}
                Some("tight") => return Err(not_yet("imshow", "'Border','tight'")),
                _ => return Err(bad("imshow", "Border must be 'loose' or 'tight'")),
            },
            other => return Err(not_yet("imshow", format!("the option '{other}'"))),
        }
    }
    let kind = if let Some(map) = map {
        if rgb {
            return Err(bad("imshow", "an RGB image takes no colormap"));
        }
        ImshowKind::Indexed { map }
    } else if rgb {
        if range.is_some() {
            return Err(not_yet("imshow", "a display range for an RGB image"));
        }
        ImshowKind::Truecolor
    } else {
        let class = if is_bool {
            NumClass::Double
        } else {
            cdata.class
        };
        let default = default_range(class).unwrap_or([0.0, 1.0]);
        let range = match range {
            None => default,
            Some(Some(r)) => r,
            Some(None) => match cdata.finite_range() {
                Some([lo, hi]) if lo < hi => [lo, hi],
                _ => default,
            },
        };
        ImshowKind::Gray { range }
    };
    let args = ImshowArgs {
        cdata,
        kind,
        x,
        y,
        colormap: cmap,
    };
    let h = g.imshow(ax, args, &Opts::new()).map_err(gerr)?;
    out_handle(h, nargout)
}

/// `contour3(Z)`, `contour3(X, Y, Z)`, `(..., n|levels)`, LineSpec,
/// name/value pairs.
pub(crate) fn contour3(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    if nargout > 0 {
        return Err(not_yet("contour3", "the contour matrix output"));
    }
    let mut a = to_props(&a, "contour3")?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let (mut data, mut rest) = split_numeric(&a);
    let levels = match data.len() {
        2 | 4 => {
            let v = numbers(
                &data.pop().unwrap_or(PropValue::Num(0.0)),
                "contour3",
                "levels",
            )?;
            match v.as_slice() {
                [n] => Levels::Count(positive_int(*n, "contour3")?),
                [a, b] if a == b => Levels::Values(vec![*a]),
                _ => Levels::Values(v),
            }
        }
        _ => Levels::Auto,
    };
    let args = grid_args(&data, "contour3", false)?;
    let mut spec = None;
    if rest.len() % 2 == 1
        && let Some(PropValue::Str(s)) = rest.first()
    {
        spec = Some(
            linespec::parse(s)
                .map_err(gerr)?
                .ok_or_else(|| bad("contour3", format!("invalid LineSpec '{s}'")))?,
        );
        rest = &rest[1..];
    }
    let opts = pairs("contour3", rest)?;
    g.contour3(ax, &args, &levels, spec.as_ref(), &opts)
        .map_err(gerr)?;
    Ok(vec![])
}

fn surfc_impl(it: &mut Interp, a: Vec<Value>, nargout: usize, fname: &str) -> R<Vec<Value>> {
    let mut a = to_props(&a, fname)?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let (data, rest) = split_numeric(&a);
    let args = grid_args(&data, fname, true)?;
    let opts = pairs(fname, rest)?;
    let hs = g.surfc(ax, &args, fname == "meshc", &opts).map_err(gerr)?;
    out_handles(&hs, nargout)
}

/// `surfc(Z)`, `surfc(Z, C)`, `surfc(X, Y, Z[, C])`, then surface options.
pub(crate) fn surfc(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    surfc_impl(it, a, n, "surfc")
}

/// `meshc`, as `surfc` with a mesh.
pub(crate) fn meshc(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    surfc_impl(it, a, n, "meshc")
}

/// `bar3(Z)`, `bar3(y, Z)`, `(..., width)`, `(..., 'detached'|'stacked')`,
/// `(..., color)`, then surface options.
pub(crate) fn bar3(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "bar3")?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let (data, mut rest) = split_numeric(&a);
    let arr = |p: &PropValue, what: &str| array(p, "bar3", what);
    let scalar = |p: &PropValue| arr(p, "WIDTH").map(|x| (x.numel() == 1).then(|| x.data[0]));
    let (y, z, width): (Option<Vec<f64>>, Array, Option<f64>) = match data.as_slice() {
        [z] => (None, arr(z, "Z")?, None),
        [first, second] => {
            let s = arr(second, "Z")?;
            match scalar(second)? {
                Some(w) if arr(first, "Z")?.numel() > 1 || s.numel() == 1 => {
                    (None, arr(first, "Z")?, Some(w))
                }
                _ => (Some(arr(first, "Y")?.data), s, None),
            }
        }
        [y, z, w] => (
            Some(arr(y, "Y")?.data),
            arr(z, "Z")?,
            Some(scalar(w)?.ok_or_else(|| bad("bar3", "WIDTH must be a scalar"))?),
        ),
        _ => return Err(bad("bar3", "expected Z, (y, Z) or (y, Z, width)")),
    };
    let mut style = Bar3Style::Detached;
    let mut color = None;
    while let Some(w) = rest.first().and_then(word) {
        match w.as_str() {
            "detached" => style = Bar3Style::Detached,
            "stacked" => style = Bar3Style::Stacked,
            "grouped" => return Err(not_yet("bar3", "the 'grouped' layout")),
            c if is_color_word(c) && rest.len() % 2 == 1 => {
                color = indy_graphics::color::named(c);
            }
            _ => break,
        }
        rest = &rest[1..];
    }
    let opts = pairs("bar3", rest)?;
    let args = Bar3Args {
        y,
        z,
        width: width.unwrap_or(0.8),
        style,
        color,
    };
    let hs = g.bar3(ax, &args, &opts).map_err(gerr)?;
    out_handles(&hs, nargout)
}
