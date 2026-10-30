//! `fillmissing` and the shared gap-filling engine used by `filloutliers`.

use indy_value::{Array, Value};

use super::super::{err, need};
use super::{
    dim_arg, first_dim, is_text_arg, median_omit, no_table, opt_name, slice_positions, std_missing,
    take_n, window_arg,
};
use crate::{Interp, R};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum FMethod {
    Constant,
    Previous,
    Next,
    Nearest,
    Interp(&'static str),
    MovMean(usize, usize),
    MovMedian(usize, usize),
}

impl FMethod {
    fn numeric_only(self) -> bool {
        matches!(
            self,
            FMethod::Interp(_) | FMethod::MovMean(..) | FMethod::MovMedian(..)
        )
    }
    fn name(self) -> &'static str {
        match self {
            FMethod::Constant => "constant",
            FMethod::Previous => "previous",
            FMethod::Next => "next",
            FMethod::Nearest => "nearest",
            FMethod::Interp(m) => m,
            FMethod::MovMean(..) => "movmean",
            FMethod::MovMedian(..) => "movmedian",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Ends {
    Extrap,
    Previous,
    Next,
    Nearest,
    None,
    Scalar(f64),
}

/// What to put at one slice position.
#[derive(Clone, Copy, PartialEq)]
enum Plan {
    Keep,
    /// Copy the slice element at this local index.
    Copy(usize),
    Val(f64, f64),
    /// The fill constant of this slice.
    Const,
}

pub(super) struct Spec {
    pub method: FMethod,
    pub ends: Ends,
    /// Sample points along the operating dimension (1..n when `None`).
    pub t: Option<Vec<f64>>,
    pub maxgap: f64,
}

/// Per-slice fill constants (one entry, or one per slice).
pub(super) enum Consts {
    Num(Vec<(f64, f64)>),
    Char(Vec<char>),
    Cell(Vec<Value>),
    Str(Vec<Option<String>>),
}

impl Consts {
    fn len(&self) -> usize {
        match self {
            Consts::Num(v) => v.len(),
            Consts::Char(v) => v.len(),
            Consts::Cell(v) => v.len(),
            Consts::Str(v) => v.len(),
        }
    }
}

fn interp_call(it: &mut Interp, xs: &[f64], ys: &[f64], xq: &[f64], m: &str) -> R<Vec<f64>> {
    let r = it.call_named(
        "interp1",
        vec![
            Value::col(xs.to_vec()),
            Value::col(ys.to_vec()),
            Value::col(xq.to_vec()),
            Value::str(m),
            Value::str("extrap"),
        ],
        1,
    )?;
    match r.first().and_then(|v| v.to_f64_vec()) {
        Some(v) if v.len() == xq.len() => Ok(v),
        _ => Err(err(
            "MATLAB:fillmissing:interpFailed",
            "fillmissing: interpolation returned an unexpected result.",
        )),
    }
}

/// Plan one slice. `vals` are the slice values (numeric methods only),
/// `bad` marks entries that cannot be used as data, `target` the entries
/// to fill.
fn plan_slice(
    it: &mut Interp,
    spec: &Spec,
    vals: &[(f64, f64)],
    bad: &[bool],
    target: &[bool],
) -> R<Vec<Plan>> {
    let n = bad.len();
    let own_t: Vec<f64>;
    let t: &[f64] = match &spec.t {
        Some(t) => t,
        None => {
            own_t = (1..=n).map(|k| k as f64).collect();
            &own_t
        }
    };
    let good: Vec<usize> = (0..n).filter(|k| !bad[*k]).collect();
    // previous / next good index of every position
    let mut prev = vec![None; n];
    let mut next = vec![None; n];
    let mut last = None;
    for k in 0..n {
        prev[k] = last;
        if !bad[k] {
            last = Some(k);
        }
    }
    last = None;
    for k in (0..n).rev() {
        next[k] = last;
        if !bad[k] {
            last = Some(k);
        }
    }
    let nearest = |k: usize| -> Option<usize> {
        match (prev[k], next[k]) {
            (Some(p), Some(q)) => Some(if t[k] - t[p] < t[q] - t[k] { p } else { q }),
            (p, q) => p.or(q),
        }
    };
    let mut plan = vec![Plan::Keep; n];
    let mut queries: Vec<usize> = Vec::new();
    for k in 0..n {
        if !target[k] {
            continue;
        }
        if spec.maxgap.is_finite() && n > 0 {
            let gap = match (prev[k], next[k]) {
                (Some(p), Some(q)) => t[q] - t[p],
                (None, Some(q)) => t[q] - t[0],
                (Some(p), None) => t[n - 1] - t[p],
                (None, None) => t[n - 1] - t[0],
            };
            if gap > spec.maxgap {
                continue;
            }
        }
        let is_end = prev[k].is_none() || next[k].is_none();
        if is_end && spec.ends != Ends::Extrap {
            plan[k] = match spec.ends {
                Ends::Previous => prev[k].map_or(Plan::Keep, Plan::Copy),
                Ends::Next => next[k].map_or(Plan::Keep, Plan::Copy),
                Ends::Nearest => nearest(k).map_or(Plan::Keep, Plan::Copy),
                Ends::Scalar(x) => Plan::Val(x, 0.0),
                Ends::None | Ends::Extrap => Plan::Keep,
            };
            continue;
        }
        plan[k] = match spec.method {
            FMethod::Constant => Plan::Const,
            FMethod::Previous => prev[k].map_or(Plan::Keep, Plan::Copy),
            FMethod::Next => next[k].map_or(Plan::Keep, Plan::Copy),
            FMethod::Nearest => nearest(k).map_or(Plan::Keep, Plan::Copy),
            FMethod::Interp(_) => {
                queries.push(k);
                Plan::Keep
            }
            FMethod::MovMean(b, f) | FMethod::MovMedian(b, f) => {
                let lo = k.saturating_sub(b);
                let hi = k.saturating_add(f).min(n - 1);
                let w: Vec<(f64, f64)> = (lo..=hi).filter(|j| !bad[*j]).map(|j| vals[j]).collect();
                if w.is_empty() {
                    Plan::Keep
                } else if matches!(spec.method, FMethod::MovMean(..)) {
                    let m = w.len() as f64;
                    Plan::Val(
                        w.iter().map(|z| z.0).sum::<f64>() / m,
                        w.iter().map(|z| z.1).sum::<f64>() / m,
                    )
                } else {
                    let re: Vec<f64> = w.iter().map(|z| z.0).collect();
                    Plan::Val(median_omit(&re), 0.0)
                }
            }
        };
    }
    if let FMethod::Interp(m) = spec.method
        && !queries.is_empty()
        && good.len() >= 2
    {
        let xs: Vec<f64> = good.iter().map(|k| t[*k]).collect();
        let xq: Vec<f64> = queries.iter().map(|k| t[*k]).collect();
        let yr: Vec<f64> = good.iter().map(|k| vals[*k].0).collect();
        let re = interp_call(it, &xs, &yr, &xq, m)?;
        let im = if good.iter().any(|k| vals[*k].1 != 0.0) {
            let yi: Vec<f64> = good.iter().map(|k| vals[*k].1).collect();
            interp_call(it, &xs, &yi, &xq, m)?
        } else {
            vec![0.0; xq.len()]
        };
        for (j, k) in queries.iter().enumerate() {
            plan[*k] = Plan::Val(re[j], im[j]);
        }
    }
    Ok(plan)
}

/// Fill `target` entries of `v` along `dim`. Returns the filled value and
/// the filled-entry mask.
pub(super) fn fill_value(
    it: &mut Interp,
    v: &Value,
    dim: usize,
    spec: &Spec,
    bad: &[bool],
    target: &[bool],
    consts: Option<&Consts>,
) -> R<(Value, Vec<bool>)> {
    let dims = v.dims();
    let slices = slice_positions(&dims, dim);
    let mut out = v.clone();
    let mut filled = vec![false; v.numel()];
    for (s, pos) in slices.iter().enumerate() {
        if !pos.iter().any(|p| target[*p]) {
            continue;
        }
        let vals: Vec<(f64, f64)> = match v {
            Value::Num(n) if spec.method.numeric_only() => {
                pos.iter().map(|p| (n.re[*p], n.im_at(*p))).collect()
            }
            _ => vec![(0.0, 0.0); pos.len()],
        };
        let b: Vec<bool> = pos.iter().map(|p| bad[*p]).collect();
        let tg: Vec<bool> = pos.iter().map(|p| target[*p]).collect();
        let plan = plan_slice(it, spec, &vals, &b, &tg)?;
        let ci = match consts {
            Some(c) if c.len() > 1 => s,
            _ => 0,
        };
        for (k, pl) in plan.iter().enumerate() {
            if *pl == Plan::Keep {
                continue;
            }
            let p = pos[k];
            filled[p] = true;
            match (&mut out, v) {
                (Value::Num(o), Value::Num(src)) => {
                    let (re, im) = match pl {
                        Plan::Copy(j) => (src.re[pos[*j]], src.im_at(pos[*j])),
                        Plan::Val(r, i) => (*r, *i),
                        Plan::Const => match consts {
                            Some(Consts::Num(c)) => c[ci],
                            _ => (f64::NAN, 0.0),
                        },
                        Plan::Keep => continue,
                    };
                    o.re[p] = o.class.cast(re);
                    if im != 0.0 || o.im.is_some() {
                        let total = o.re.len();
                        let class = o.class;
                        o.im.get_or_insert_with(|| vec![0.0; total])[p] = class.cast(im);
                    }
                }
                (Value::Bool(o), Value::Bool(src)) => {
                    o.data[p] = match pl {
                        Plan::Copy(j) => src.data[pos[*j]],
                        Plan::Const => match consts {
                            Some(Consts::Num(c)) => c[ci].0 != 0.0,
                            _ => false,
                        },
                        _ => continue,
                    };
                }
                (Value::Char(o), Value::Char(src)) => {
                    o.data[p] = match pl {
                        Plan::Copy(j) => src.data[pos[*j]],
                        Plan::Const => match consts {
                            Some(Consts::Char(c)) => c[ci],
                            _ => ' ',
                        },
                        _ => continue,
                    };
                }
                (Value::Cell(o), Value::Cell(src)) => {
                    o.data[p] = match pl {
                        Plan::Copy(j) => src.data[pos[*j]].clone(),
                        Plan::Const => match consts {
                            Some(Consts::Cell(c)) => c[ci].clone(),
                            _ => Value::str(""),
                        },
                        _ => continue,
                    };
                }
                (Value::Str(o), Value::Str(src)) => {
                    o.data[p] = match pl {
                        Plan::Copy(j) => src.data[pos[*j]].clone(),
                        Plan::Const => match consts {
                            Some(Consts::Str(c)) => c[ci].clone(),
                            _ => None,
                        },
                        _ => continue,
                    };
                }
                _ => {}
            }
        }
    }
    if let Value::Num(o) = &mut out {
        o.narrow();
    }
    Ok((out, filled))
}

fn bad_const(msg: impl Into<String>) -> Box<indy_value::Error> {
    err("MATLAB:fillmissing:InvalidFillConstant", msg)
}

/// Fill constants compatible with the class of `a`.
fn parse_consts(a: &Value, c: &Value) -> R<Consts> {
    let text_items = |c: &Value| -> R<Vec<Option<String>>> {
        match c {
            Value::Char(_) => {
                if c.is_empty() {
                    return Ok(vec![Some(String::new())]);
                }
                match c.char_rows() {
                    Some(rows) if rows.len() == 1 => Ok(vec![Some(rows[0].clone())]),
                    _ => Err(bad_const(
                        "fillmissing: a char fill constant must be a character vector.",
                    )),
                }
            }
            Value::Str(s) => Ok(s.data.clone()),
            Value::Cell(cc) => cc
                .data
                .iter()
                .map(|e| match e {
                    Value::Char(_) if e.is_empty() => Ok(Some(String::new())),
                    Value::Char(_) if e.is_string_row() => Ok(e.to_str()),
                    _ => Err(bad_const(
                        "fillmissing: a cell fill constant must be a cell array of character vectors.",
                    )),
                })
                .collect(),
            other => Err(bad_const(format!(
                "fillmissing: fill constant of class {} is not compatible with {} input.",
                other.class_name(),
                a.class_name()
            ))),
        }
    };
    Ok(match a {
        Value::Num(_) | Value::Bool(_) => match c {
            Value::Num(n) => Consts::Num((0..n.re.len()).map(|k| (n.re[k], n.im_at(k))).collect()),
            Value::Bool(b) => Consts::Num(b.data.iter().map(|x| (*x as u8 as f64, 0.0)).collect()),
            other => {
                return Err(bad_const(format!(
                    "fillmissing: fill constant of class {} is not compatible with {} input.",
                    other.class_name(),
                    a.class_name()
                )));
            }
        },
        Value::Char(_) => match c {
            Value::Char(cc) => Consts::Char(cc.data.clone()),
            Value::Str(s) if s.data.len() == 1 => {
                let cs: Vec<char> = s.data[0].clone().unwrap_or_default().chars().collect();
                if cs.len() != 1 {
                    return Err(bad_const(
                        "fillmissing: the fill constant for char input must be a single character.",
                    ));
                }
                Consts::Char(cs)
            }
            other => {
                return Err(bad_const(format!(
                    "fillmissing: fill constant of class {} is not compatible with char input.",
                    other.class_name()
                )));
            }
        },
        Value::Cell(_) => {
            let items = text_items(c)?;
            if items.iter().any(|x| x.is_none()) {
                return Err(bad_const(
                    "fillmissing: a missing string cannot fill a cell array.",
                ));
            }
            Consts::Cell(
                items
                    .into_iter()
                    .map(|s| Value::str(&s.unwrap_or_default()))
                    .collect(),
            )
        }
        Value::Str(_) => Consts::Str(text_items(c)?),
        _ => {
            return Err(bad_const("fillmissing: unsupported input class."));
        }
    })
}

pub(super) fn parse_ends(v: &Value, fname: &str) -> R<Ends> {
    if let Some(s) = opt_name(v) {
        return Ok(match s.as_str() {
            "extrap" => Ends::Extrap,
            "previous" => Ends::Previous,
            "next" => Ends::Next,
            "nearest" => Ends::Nearest,
            "none" => Ends::None,
            other => {
                return Err(err(
                    &format!("MATLAB:{}:InvalidEndValues", fname),
                    format!(
                        "{}: EndValues must be 'extrap', 'previous', 'next', 'nearest', 'none' or a scalar (got '{}').",
                        fname, other
                    ),
                ));
            }
        });
    }
    match v {
        Value::Num(n) if n.re.len() == 1 && n.im.is_none() => Ok(Ends::Scalar(n.re[0])),
        Value::Bool(b) if b.data.len() == 1 => Ok(Ends::Scalar(b.data[0] as u8 as f64)),
        _ => Err(err(
            &format!("MATLAB:{}:InvalidEndValues", fname),
            format!(
                "{}: EndValues must be 'extrap', 'previous', 'next', 'nearest', 'none' or a real scalar.",
                fname
            ),
        )),
    }
}

pub(super) fn parse_sample_points(v: &Value, n: usize, fname: &str) -> R<Vec<f64>> {
    let bad = |m: &str| {
        err(
            &format!("MATLAB:{}:SamplePointsInvalid", fname),
            format!("{}: {}", fname, m),
        )
    };
    let t = match v {
        Value::Num(x) if x.im.is_none() => x.re.clone(),
        _ => return Err(bad("SamplePoints must be a real numeric vector.")),
    };
    let d = v.dims();
    if d.len() > 2 || (d[0] != 1 && d[1] != 1 && !t.is_empty()) {
        return Err(bad("SamplePoints must be a vector."));
    }
    if t.len() != n {
        return Err(bad(
            "SamplePoints must have one entry per element along the operating dimension.",
        ));
    }
    if t.iter().any(|x| !x.is_finite()) || t.windows(2).any(|w| w[1] <= w[0]) {
        return Err(bad(
            "SamplePoints must be finite, unique and sorted in ascending order.",
        ));
    }
    Ok(t)
}

pub fn fillmissing(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let f = "fillmissing";
    need(&a, 2, f)?;
    let v = &a[0];
    no_table(v, f)?;
    if matches!(a[1], Value::Func(_)) {
        return Err(err(
            "MATLAB:fillmissing:unsupportedMethod",
            "fillmissing: a function handle fill method is not supported in the Rust engine yet.",
        ));
    }
    let Some(mname) = opt_name(&a[1]) else {
        return Err(err(
            "MATLAB:fillmissing:InvalidMethod",
            "fillmissing: the fill method must be a character vector or string scalar.",
        ));
    };
    let mut i = 2;
    let mut constv: Option<&Value> = None;
    let method = match mname.as_str() {
        "constant" => {
            let Some(c) = a.get(2) else {
                return Err(err(
                    "MATLAB:fillmissing:ConstantRequired",
                    "fillmissing: the 'constant' method requires a fill value.",
                ));
            };
            constv = Some(c);
            i = 3;
            FMethod::Constant
        }
        "movmean" | "movmedian" => {
            let Some(w) = a.get(2) else {
                return Err(err(
                    "MATLAB:fillmissing:WindowRequired",
                    format!(
                        "fillmissing: the '{}' method requires a window length.",
                        mname
                    ),
                ));
            };
            let (b, fw) = window_arg(w, f)?;
            i = 3;
            if mname == "movmean" {
                FMethod::MovMean(b, fw)
            } else {
                FMethod::MovMedian(b, fw)
            }
        }
        "previous" => FMethod::Previous,
        "next" => FMethod::Next,
        "nearest" => FMethod::Nearest,
        "linear" => FMethod::Interp("linear"),
        "spline" => FMethod::Interp("spline"),
        "pchip" => FMethod::Interp("pchip"),
        "makima" => FMethod::Interp("makima"),
        "knn" => {
            return Err(err(
                "MATLAB:fillmissing:unsupportedMethod",
                "fillmissing: method 'knn' is not supported in the Rust engine yet.",
            ));
        }
        _ => {
            return Err(err(
                "MATLAB:fillmissing:InvalidMethod",
                format!(
                    "fillmissing: unknown fill method '{}'.",
                    a[1].to_str().unwrap_or_default()
                ),
            ));
        }
    };
    let mut dim = None;
    if i < a.len() && !is_text_arg(&a[i]) {
        dim = Some(dim_arg(&a[i], f)?);
        i += 1;
    }
    let mut ends = Ends::Extrap;
    let mut sp: Option<&Value> = None;
    let mut maxgap = f64::INFINITY;
    let mut locs: Option<&Value> = None;
    while i < a.len() {
        let Some(name) = opt_name(&a[i]) else {
            return Err(err(
                "MATLAB:fillmissing:InvalidOption",
                "fillmissing: expected a name-value option name.",
            ));
        };
        let Some(val) = a.get(i + 1) else {
            return Err(err(
                "MATLAB:fillmissing:missingValue",
                format!("fillmissing: option '{}' requires a value.", name),
            ));
        };
        match name.as_str() {
            "endvalues" => ends = parse_ends(val, f)?,
            "samplepoints" => sp = Some(val),
            "maxgap" => {
                let g = match val {
                    Value::Num(n) if n.re.len() == 1 && n.im.is_none() => n.re[0],
                    _ => f64::NAN,
                };
                if g.is_nan() || g <= 0.0 {
                    return Err(err(
                        "MATLAB:fillmissing:MaxGapInvalid",
                        "fillmissing: MaxGap must be a positive real scalar.",
                    ));
                }
                maxgap = g;
            }
            "missinglocations" => locs = Some(val),
            "datavariables" | "replacevalues" => {
                return Err(err(
                    "MATLAB:fillmissing:tableUnsupported",
                    format!(
                        "fillmissing: option '{}' applies to table input, which is not supported in the Rust engine yet.",
                        a[i].to_str().unwrap_or_default()
                    ),
                ));
            }
            _ => {
                return Err(err(
                    "MATLAB:fillmissing:InvalidOption",
                    format!(
                        "fillmissing: unknown option '{}'.",
                        a[i].to_str().unwrap_or_default()
                    ),
                ));
            }
        }
        i += 2;
    }
    let dims = v.dims();
    let dim = dim.unwrap_or_else(|| first_dim(&dims));
    let n = dims.get(dim).copied().unwrap_or(1);
    let mask: Vec<bool> = match locs {
        Some(Value::Bool(b)) if b.dims == dims => b.data.clone(),
        Some(_) => {
            return Err(err(
                "MATLAB:fillmissing:MissingLocationsInvalid",
                "fillmissing: MissingLocations must be a logical array of the same size as the input.",
            ));
        }
        None => match std_missing(v) {
            Some(m) => m,
            None => {
                return Err(err(
                    "MATLAB:fillmissing:FirstInputInvalid",
                    format!(
                        "fillmissing: input of class {} is not supported.",
                        v.class_name()
                    ),
                ));
            }
        },
    };
    if !matches!(
        v,
        Value::Num(_) | Value::Bool(_) | Value::Char(_) | Value::Cell(_) | Value::Str(_)
    ) {
        return Err(err(
            "MATLAB:fillmissing:FirstInputInvalid",
            format!(
                "fillmissing: input of class {} is not supported.",
                v.class_name()
            ),
        ));
    }
    if let Value::Cell(c) = v
        && c.data
            .iter()
            .any(|e| !(e.is_string_row() || (e.is_char() && e.is_empty())))
    {
        return Err(err(
            "MATLAB:fillmissing:FirstInputInvalid",
            "fillmissing: a cell array input must be a cell array of character vectors.",
        ));
    }
    let numeric_needed = method.numeric_only() || matches!(ends, Ends::Scalar(_));
    let nothing = !mask.iter().any(|m| *m);
    match v {
        Value::Num(x) if x.im.is_some() => {
            if matches!(
                method,
                FMethod::Interp("pchip") | FMethod::Interp("makima") | FMethod::MovMedian(..)
            ) {
                return Err(err(
                    "MATLAB:fillmissing:ComplexUnsupported",
                    format!(
                        "fillmissing: method '{}' does not support complex input.",
                        method.name()
                    ),
                ));
            }
        }
        Value::Num(_) => {}
        Value::Bool(_) if nothing => {}
        _ if numeric_needed => {
            return Err(err(
                "MATLAB:fillmissing:MethodInvalidForType",
                format!(
                    "fillmissing: {} requires numeric input, not {}.",
                    if method.numeric_only() {
                        format!("method '{}'", method.name())
                    } else {
                        "a scalar EndValues".to_string()
                    },
                    v.class_name()
                ),
            ));
        }
        _ => {}
    }
    let t = match sp {
        Some(s) => {
            if matches!(method, FMethod::MovMean(..) | FMethod::MovMedian(..)) {
                return Err(err(
                    "MATLAB:fillmissing:unsupportedOption",
                    "fillmissing: 'SamplePoints' with movmean/movmedian is not supported in the Rust engine yet.",
                ));
            }
            Some(parse_sample_points(s, n, f)?)
        }
        None => None,
    };
    let consts = match constv {
        Some(c) => {
            let cs = parse_consts(v, c)?;
            let nslices = v.numel().checked_div(n).unwrap_or(0);
            if cs.len() == 0 || (cs.len() != 1 && cs.len() != nslices) {
                return Err(err(
                    "MATLAB:fillmissing:SizeConstant",
                    "fillmissing: the fill constant must be a scalar or have one value per vector along the operating dimension.",
                ));
            }
            Some(cs)
        }
        None => None,
    };
    let spec = Spec {
        method,
        ends,
        t,
        maxgap,
    };
    let (out, filled) = if nothing {
        (v.clone(), vec![false; v.numel()])
    } else {
        fill_value(it, v, dim, &spec, &mask, &mask, consts.as_ref())?
    };
    let tf = Value::Bool(Array::new(dims, filled));
    take_n(vec![out, tf], nargout)
}
