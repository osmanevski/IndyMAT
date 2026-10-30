//! Statistics and specialised charts over the model: `histogram`, `hist`.
//!
//! `histogram` takes its bin edges from the interpreter's own `histcounts`
//! with the same binning arguments, so `get(h,'Values')` and `BinEdges`
//! always equal `histcounts` for the same call, as MATLAB guarantees.

use indy_graphics::{Handle, HistBins, PropValue, hist_counts};
use indy_interp::{Interp, R, Value};

use crate::builtins::{Opts, bad, not_yet, out_handle, out_handles, pairs, take_axes};
use crate::convert::{array, from_prop, gerr, numbers, to_prop, to_props};
use crate::graphics;

/// Binning options of `histogram`: passed to `histcounts`, not the model.
const BINNING: &[&str] = &["numbins", "binedges", "binwidth", "binlimits", "binmethod"];

fn is_text(v: &Value) -> bool {
    v.is_char() || v.is_string_row()
}

/// Bin edges of `data` from `histcounts(data, args...)`.
fn edges_of(it: &mut Interp, data: &Value, args: Vec<Value>) -> R<Vec<f64>> {
    let mut call = vec![data.clone()];
    call.extend(args);
    let out = it.call_named("histcounts", call, 2)?;
    out.get(1)
        .and_then(Value::to_f64_vec)
        .ok_or_else(|| bad("histogram", "histcounts returned no bin edges"))
}

/// The binning method recorded for a histogram created or re-binned with
/// these `histcounts` options.
fn method_of(args: &[Value]) -> String {
    let mut method = "auto".to_string();
    for pair in args.chunks(2) {
        let key = pair[0].to_str().unwrap_or_default().to_ascii_lowercase();
        match key.as_str() {
            "binmethod" => {
                method = pair
                    .get(1)
                    .and_then(Value::to_str)
                    .unwrap_or_default()
                    .to_ascii_lowercase()
            }
            "binlimits" => {}
            _ => method = "manual".into(),
        }
    }
    method
}

/// `histogram(X)`, `histogram(X, nbins)`, `histogram(X, edges)`,
/// `histogram(ax, ...)`, then name/value pairs.
pub(crate) fn histogram(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let mut a = a;
    let ax = {
        let g = graphics(it);
        let mut head: Vec<PropValue> = a
            .first()
            .map(|v| to_prop(v, "histogram"))
            .transpose()?
            .into_iter()
            .collect();
        let ax = take_axes(g, &mut head);
        if ax.is_some() {
            a.remove(0);
        }
        ax
    };
    let Some(data) = a.first().cloned() else {
        return Err(bad("histogram", "not enough input arguments"));
    };
    if is_text(&data) {
        return Err(not_yet(
            "histogram",
            "the 'BinEdges'/'BinCounts' form and categorical data",
        ));
    }
    if !matches!(data, Value::Num(_) | Value::Bool(_)) {
        return Err(bad("histogram", "X must be numeric or logical"));
    }
    let values = numbers(&to_prop(&data, "histogram")?, "histogram", "X")?;
    let mut i = 1;
    let mut hc: Vec<Value> = Vec::new();
    if let Some(v) = a.get(1)
        && !is_text(v)
    {
        let n = v.to_f64_vec().unwrap_or_default();
        let key = if n.len() == 1 { "NumBins" } else { "BinEdges" };
        hc.push(Value::str(key));
        hc.push(v.clone());
        i = 2;
    }
    let mut rest: Vec<Value> = Vec::new();
    while i < a.len() {
        let key = a[i]
            .to_str()
            .filter(|_| is_text(&a[i]))
            .ok_or_else(|| bad("histogram", "property names must be text"))?;
        let Some(v) = a.get(i + 1) else {
            return Err(bad("histogram", "name/value arguments must come in pairs"));
        };
        let low = key.to_ascii_lowercase();
        if BINNING.contains(&low.as_str()) {
            hc.push(a[i].clone());
            hc.push(v.clone());
        } else if low == "bincounts" {
            return Err(not_yet("histogram", "the 'BinCounts' form"));
        } else {
            rest.push(a[i].clone());
            rest.push(v.clone());
        }
        i += 2;
    }
    let method = method_of(&hc);
    let edges = edges_of(it, &data, hc)?;
    let opts: Opts = pairs("histogram", &to_props(&rest, "histogram")?)?;
    let g = graphics(it);
    let h = g
        .histogram(ax, values, edges, &method, &opts)
        .map_err(gerr)?;
    out_handle(h, nargout)
}

/// `set` on a histogram: the binning properties re-bin through
/// `histcounts`; everything else goes to the model. Returns `false` when
/// `h` is not a histogram (nothing done).
pub(crate) fn set_histogram(it: &mut Interp, h: Handle, opts: &[(String, Value)]) -> R<bool> {
    if graphics(it).type_of(h).ok() != Some("histogram") {
        return Ok(false);
    }
    for (name, v) in opts {
        let low = name.to_ascii_lowercase();
        let g = graphics(it);
        let get = |g: &indy_graphics::Graphics, p: &str| g.get(h, p).map(from_prop).map_err(gerr);
        match low.as_str() {
            "numbins" | "binwidth" | "binlimits" | "binmethod" | "binedges" => {
                let data = get(g, "Data")?;
                let args = vec![Value::str(name), v.clone()];
                let method = method_of(&args);
                let edges = edges_of(it, &data, args)?;
                let g = graphics(it);
                g.set(h, "BinEdges", PropValue::row(&edges)).map_err(gerr)?;
                g.set(h, "BinMethod", PropValue::str(&method))
                    .map_err(gerr)?;
            }
            "data" => {
                let method = match get(g, "BinMethod")? {
                    m if m.to_str().as_deref() == Some("manual") => None,
                    m => Some(m),
                };
                if !matches!(v, Value::Num(_) | Value::Bool(_)) {
                    return Err(bad("set", "Data must be numeric or logical"));
                }
                let values = numbers(&to_prop(v, "set")?, "set", "Data")?;
                let g = graphics(it);
                g.set(h, "Data", PropValue::row(&values)).map_err(gerr)?;
                if let Some(m) = method {
                    let args = match m.to_str().as_deref() {
                        Some("auto") | None => vec![],
                        Some(_) => vec![Value::str("BinMethod"), m.clone()],
                    };
                    let edges = edges_of(it, v, args)?;
                    let g = graphics(it);
                    let keep = g.get(h, "BinMethod").map_err(gerr)?;
                    g.set(h, "BinEdges", PropValue::row(&edges)).map_err(gerr)?;
                    g.set(h, "BinMethod", keep).map_err(gerr)?;
                }
            }
            _ => {
                let p = to_prop(v, "set")?;
                g.set(h, name, p).map_err(gerr)?;
            }
        }
    }
    Ok(true)
}

/// `hist(y)`, `hist(y, nbins)`, `hist(y, centers)`, `hist(y, x, norm)`,
/// `hist(ax, ...)`; with outputs `[counts, centers]` and no plot.
pub(crate) fn hist(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "hist")?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let Some(first) = a.first() else {
        return Err(bad("hist", "not enough input arguments"));
    };
    let y = array(first, "hist", "Y")?;
    let mut i = 1;
    let bins = match a.get(1) {
        Some(p @ (PropValue::Num(_) | PropValue::Array(_))) => {
            i = 2;
            let v = numbers(p, "hist", "X")?;
            if v.len() == 1 {
                let n = v[0];
                if !(n.is_finite() && n >= 1.0 && n.fract() == 0.0 && n < 1e7) {
                    return Err(bad("hist", "the number of bins must be a positive integer"));
                }
                HistBins::Count(n as usize)
            } else {
                HistBins::Centers(v)
            }
        }
        _ => HistBins::Count(10),
    };
    let norm = match a.get(i) {
        Some(p @ (PropValue::Num(_) | PropValue::Array(_))) => {
            i += 1;
            Some(crate::builtins::scalar(p, "hist", "NORM")?)
        }
        _ => None,
    };
    let result = hist_counts(&y, &bins, norm).map_err(gerr)?;
    if nargout > 0 {
        let nb = result.centers.len();
        let vector = y.is_vector() || y.is_empty();
        let counts = if vector {
            Value::row(result.counts.data.clone())
        } else {
            Value::matrix(vec![nb, result.counts.cols()], result.counts.data.clone())
        };
        let centers = if vector {
            Value::row(result.centers.clone())
        } else {
            Value::col(result.centers.clone())
        };
        return Ok(vec![counts, centers]
            .into_iter()
            .take(nargout.max(1))
            .collect());
    }
    let opts = pairs("hist", &a[i..])?;
    let hs = g.hist_plot(ax, &result, &opts).map_err(gerr)?;
    out_handles(&hs, nargout)
}

/// `sgtitle(txt)`, `sgtitle(fig, txt)`, then name/value pairs.
pub(crate) fn sgtitle(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "sgtitle")?;
    let g = graphics(it);
    let fig = crate::builtins::take_figure(g, &mut a);
    let Some(first) = a.first() else {
        return Err(bad("sgtitle", "not enough input arguments"));
    };
    let lines = crate::convert::lines(first, "sgtitle")?;
    let opts = pairs("sgtitle", &a[1..])?;
    let h = g.sgtitle(fig, &lines, &opts).map_err(gerr)?;
    out_handle(h, nargout)
}

/// `pie(X)`, `pie(X, explode)`, `pie(X, labels)`, `pie(X, explode,
/// labels)`, `pie(ax, ...)`.
pub(crate) fn pie(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let mut a = to_props(&a, "pie")?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let Some(first) = a.first() else {
        return Err(bad("pie", "not enough input arguments"));
    };
    let x = numbers(first, "pie", "X")?;
    let mut explode = None;
    let mut labels = None;
    for p in &a[1..] {
        match p {
            PropValue::Num(_) | PropValue::Array(_) if explode.is_none() && labels.is_none() => {
                explode = Some(numbers(p, "pie", "EXPLODE")?)
            }
            PropValue::Cell(c) if labels.is_none() => labels = Some(c.clone()),
            PropValue::Str(s) if labels.is_none() && x.len() == 1 => labels = Some(vec![s.clone()]),
            _ => return Err(not_yet("pie", "this argument form")),
        }
    }
    let hs = g
        .pie(ax, &x, explode.as_deref(), labels.as_deref(), &[])
        .map_err(gerr)?;
    out_handles(&hs, nargout)
}

/// `yyaxis left`, `yyaxis right`, `yyaxis(ax, side)`.
pub(crate) fn yyaxis(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    if nargout > 0 {
        return Err(bad("yyaxis", "too many output arguments"));
    }
    let mut a = to_props(&a, "yyaxis")?;
    let g = graphics(it);
    let ax = take_axes(g, &mut a);
    let right = match a.as_slice() {
        [PropValue::Str(s)] if s.eq_ignore_ascii_case("left") => false,
        [PropValue::Str(s)] if s.eq_ignore_ascii_case("right") => true,
        _ => return Err(bad("yyaxis", "expected yyaxis left or yyaxis right")),
    };
    g.yyaxis(ax, right).map_err(gerr)?;
    Ok(vec![])
}
