//! `isoutlier`, `rmoutliers`, `filloutliers` (MathWorks documented
//! definitions; NaN entries are ignored and never outliers).
//!
//! - median: outside median ± k·c·MAD, c = -1/(sqrt(2)·erfcinv(3/2)), k = 3
//! - mean: outside mean ± k·std, k = 3
//! - quartiles: outside [Q1 - k·IQR, Q3 + k·IQR], k = 1.5; quartiles use the
//!   MATLAB `prctile` definition (midpoint interpolation, clamped ends)
//! - grubbs: iterative two-sided Grubbs test at significance alpha (0.05)
//! - gesd: generalized extreme Studentized deviate test (Rosner), at most
//!   `MaxNumOutliers` outliers (default ceil(10% of the non-NaN count))
//! - movmedian / movmean: local median ± k·scaled MAD / mean ± k·std over
//!   a moving window that shrinks at the ends

use indy_value::{Array, Num, NumClass, Value};

use super::super::math::reduced_dims;
use super::super::{arg_f64, err, need};
use super::fill::{Ends, FMethod, Spec, fill_value};
use super::{
    dim_arg, first_dim, is_text_arg, mean_omit, median_omit, opt_name, real_data, remove_flagged,
    slice_positions, std_omit, take_n, window_arg,
};
use crate::{Interp, R};

/// -1/(sqrt(2)*erfcinv(3/2))
const MAD_SCALE: f64 = 1.482_602_218_505_602;

#[derive(Clone, Copy, PartialEq)]
enum OMethod {
    Median,
    Mean,
    Quartiles,
    Grubbs,
    Gesd,
    MovMedian(usize, usize),
    MovMean(usize, usize),
}

impl OMethod {
    fn moving(self) -> bool {
        matches!(self, OMethod::MovMedian(..) | OMethod::MovMean(..))
    }
}

struct Detect {
    method: OMethod,
    factor: f64,
    maxn: Option<usize>,
    dim: Option<usize>,
}

struct Found {
    tf: Vec<bool>,
    /// Thresholds and centre: one per slice, or one per element (moving).
    l: Vec<f64>,
    u: Vec<f64>,
    c: Vec<f64>,
    stat_dims: Vec<usize>,
    dim: usize,
}

// ---------------------------------------------------------------------------
// Student t quantile (for Grubbs / GESD)
// ---------------------------------------------------------------------------

fn ln_gamma(x: f64) -> f64 {
    super::super::math::lgamma(x)
}

/// Regularised incomplete beta I_x(a, b) (continued fraction, Lentz).
fn betainc(x: f64, a: f64, b: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let front =
        (ln_gamma(a + b) - ln_gamma(a) - ln_gamma(b) + a * x.ln() + b * (1.0 - x).ln()).exp();
    let cf = |x: f64, a: f64, b: f64| -> f64 {
        let tiny = 1e-300;
        let mut c = 1.0;
        let mut d = 1.0 - (a + b) * x / (a + 1.0);
        if d.abs() < tiny {
            d = tiny;
        }
        d = 1.0 / d;
        let mut h = d;
        for m in 1..1000 {
            let m = m as f64;
            for step in 0..2 {
                let aa = if step == 0 {
                    m * (b - m) * x / ((a + 2.0 * m - 1.0) * (a + 2.0 * m))
                } else {
                    -(a + m) * (a + b + m) * x / ((a + 2.0 * m) * (a + 2.0 * m + 1.0))
                };
                d = 1.0 + aa * d;
                if d.abs() < tiny {
                    d = tiny;
                }
                c = 1.0 + aa / c;
                if c.abs() < tiny {
                    c = tiny;
                }
                d = 1.0 / d;
                h *= d * c;
                if step == 1 && (d * c - 1.0).abs() < 1e-15 {
                    return h;
                }
            }
        }
        h
    };
    if x < (a + 1.0) / (a + b + 2.0) {
        front * cf(x, a, b) / a
    } else {
        1.0 - front * cf(1.0 - x, b, a) / b
    }
}

fn tcdf(x: f64, v: f64) -> f64 {
    let tail = 0.5 * betainc(v / (v + x * x), v / 2.0, 0.5);
    if x >= 0.0 { 1.0 - tail } else { tail }
}

/// Quantile of Student's t with `v` degrees of freedom (bisection).
fn tinv(p: f64, v: f64) -> f64 {
    if !(p > 0.0 && p < 1.0) || v <= 0.0 {
        return f64::NAN;
    }
    let mut hi = 1.0;
    while tcdf(hi, v) < p && hi < 1e300 {
        hi *= 2.0;
    }
    let mut lo = -1.0;
    while tcdf(lo, v) > p && lo > -1e300 {
        lo *= 2.0;
    }
    for _ in 0..300 {
        let mid = 0.5 * (lo + hi);
        if mid == lo || mid == hi {
            break;
        }
        if tcdf(mid, v) < p {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Grubbs / GESD critical value for `n` points at significance `alpha`.
fn critical(n: usize, alpha: f64) -> f64 {
    if n < 3 {
        return f64::NAN;
    }
    let nf = n as f64;
    let t = tinv(1.0 - alpha / (2.0 * nf), nf - 2.0);
    (nf - 1.0) * t / ((nf - 2.0 + t * t) * nf).sqrt()
}

// ---------------------------------------------------------------------------
// detection
// ---------------------------------------------------------------------------

fn quantile_mid(s: &[f64], p: f64) -> f64 {
    let n = s.len();
    if n == 0 {
        return f64::NAN;
    }
    let h = n as f64 * p + 0.5;
    if h <= 1.0 {
        return s[0];
    }
    if h >= n as f64 {
        return s[n - 1];
    }
    let j = h.floor();
    let g = h - j;
    let lo = s[j as usize - 1];
    let hi = s[j as usize];
    lo + g * (hi - lo)
}

/// Remove-one-at-a-time tests. Returns (outlier flags, L, U, C).
fn iterative(v: &[f64], alpha: f64, gesd: Option<usize>) -> (Vec<bool>, f64, f64, f64) {
    let mut idx: Vec<usize> = (0..v.len()).filter(|k| !v[*k].is_nan()).collect();
    let mut flags = vec![false; v.len()];
    let stats = |idx: &[usize]| {
        let w: Vec<f64> = idx.iter().map(|k| v[*k]).collect();
        (mean_omit(&w), std_omit(&w))
    };
    let extreme = |idx: &[usize], m: f64| {
        let mut best = 0;
        let mut bd = -1.0;
        for (j, k) in idx.iter().enumerate() {
            let d = (v[*k] - m).abs();
            if d > bd {
                bd = d;
                best = j;
            }
        }
        (best, bd)
    };
    match gesd {
        None => loop {
            let n = idx.len();
            let (m, s) = stats(&idx);
            let g = critical(n, alpha);
            if n < 3 || s == 0.0 || s.is_nan() {
                return (flags, m - g * s, m + g * s, m);
            }
            let (j, d) = extreme(&idx, m);
            if d / s > g {
                flags[idx[j]] = true;
                idx.remove(j);
            } else {
                return (flags, m - g * s, m + g * s, m);
            }
        },
        Some(r) => {
            let n0 = idx.len();
            let r = r.min(n0.saturating_sub(2));
            let mut order = Vec::new();
            let mut work = idx.clone();
            let mut k_out = 0;
            for step in 1..=r {
                let n = work.len();
                let (m, s) = stats(&work);
                if s == 0.0 || s.is_nan() {
                    break;
                }
                let (j, d) = extreme(&work, m);
                if d / s > critical(n, alpha) {
                    k_out = step;
                }
                order.push(work[j]);
                work.remove(j);
            }
            for k in order.iter().take(k_out) {
                flags[*k] = true;
            }
            idx.retain(|k| !flags[*k]);
            let (m, s) = stats(&idx);
            let g = critical(idx.len(), alpha);
            (flags, m - g * s, m + g * s, m)
        }
    }
}

fn detect(x: &Num, d: &Detect) -> Found {
    let dims = x.dims.clone();
    let dim = d.dim.unwrap_or_else(|| first_dim(&dims));
    let slices = slice_positions(&dims, dim);
    let n_el = x.re.len();
    let mut tf = vec![false; n_el];
    let moving = d.method.moving();
    let cap = if moving { n_el } else { slices.len() };
    let (mut l, mut u, mut c) = (
        vec![f64::NAN; cap],
        vec![f64::NAN; cap],
        vec![f64::NAN; cap],
    );
    let k = d.factor;
    for (s, pos) in slices.iter().enumerate() {
        let v: Vec<f64> = pos.iter().map(|p| x.re[*p]).collect();
        let n = v.len();
        match d.method {
            OMethod::MovMedian(b, f) | OMethod::MovMean(b, f) => {
                for i in 0..n {
                    let w = &v[i.saturating_sub(b)..=i.saturating_add(f).min(n - 1)];
                    let (cc, spread) = if matches!(d.method, OMethod::MovMedian(..)) {
                        let m = median_omit(w);
                        let dev: Vec<f64> = w.iter().map(|e| (e - m).abs()).collect();
                        (m, MAD_SCALE * median_omit(&dev))
                    } else {
                        (mean_omit(w), std_omit(w))
                    };
                    let p = pos[i];
                    l[p] = cc - k * spread;
                    u[p] = cc + k * spread;
                    c[p] = cc;
                    tf[p] = v[i] < l[p] || v[i] > u[p];
                }
            }
            _ => {
                let (flags, lo, hi, cc) = match d.method {
                    OMethod::Median => {
                        let m = median_omit(&v);
                        let dev: Vec<f64> = v.iter().map(|e| (e - m).abs()).collect();
                        let sm = MAD_SCALE * median_omit(&dev);
                        (None, m - k * sm, m + k * sm, m)
                    }
                    OMethod::Mean => {
                        let m = mean_omit(&v);
                        let sd = std_omit(&v);
                        (None, m - k * sd, m + k * sd, m)
                    }
                    OMethod::Quartiles => {
                        let mut srt: Vec<f64> = v.iter().copied().filter(|e| !e.is_nan()).collect();
                        srt.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                        let q1 = quantile_mid(&srt, 0.25);
                        let q3 = quantile_mid(&srt, 0.75);
                        let iqr = q3 - q1;
                        let (lo, hi) = (q1 - k * iqr, q3 + k * iqr);
                        (None, lo, hi, (lo + hi) / 2.0)
                    }
                    OMethod::Grubbs => {
                        let (fl, lo, hi, cc) = iterative(&v, k, None);
                        (Some(fl), lo, hi, cc)
                    }
                    OMethod::Gesd => {
                        let valid = v.iter().filter(|e| !e.is_nan()).count();
                        let r = d
                            .maxn
                            .unwrap_or(((valid as f64) * 0.1).ceil() as usize)
                            .max(1);
                        let (fl, lo, hi, cc) = iterative(&v, k, Some(r));
                        (Some(fl), lo, hi, cc)
                    }
                    _ => (None, f64::NAN, f64::NAN, f64::NAN),
                };
                for (i, p) in pos.iter().enumerate() {
                    tf[*p] = match &flags {
                        Some(fl) => fl[i],
                        None => v[i] < lo || v[i] > hi,
                    };
                }
                l[s] = lo;
                u[s] = hi;
                c[s] = cc;
            }
        }
    }
    let stat_dims = if moving {
        dims.clone()
    } else {
        reduced_dims(&dims, dim)
    };
    Found {
        tf,
        l,
        u,
        c,
        stat_dims,
        dim,
    }
}

/// Parse `[method [, window]] [, dim], Name, Value` from `a[start..]`.
/// `extra` handles caller-specific options (returns false when unknown).
fn parse_detect(
    a: &[Value],
    start: usize,
    fname: &str,
    extra: &mut dyn FnMut(&str, &Value) -> R<bool>,
) -> R<Detect> {
    let mut i = start;
    let mut method = OMethod::Median;
    if let Some(s) = a.get(i).and_then(opt_name) {
        let m = match s.as_str() {
            "median" => Some(OMethod::Median),
            "mean" => Some(OMethod::Mean),
            "quartiles" => Some(OMethod::Quartiles),
            "grubbs" => Some(OMethod::Grubbs),
            "gesd" => Some(OMethod::Gesd),
            "movmedian" | "movmean" => {
                let Some(w) = a.get(i + 1) else {
                    return Err(err(
                        &format!("MATLAB:{}:WindowRequired", fname),
                        format!("{}: the '{}' method requires a window length.", fname, s),
                    ));
                };
                let (b, f) = window_arg(w, fname)?;
                i += 1;
                Some(if s == "movmedian" {
                    OMethod::MovMedian(b, f)
                } else {
                    OMethod::MovMean(b, f)
                })
            }
            "percentiles" => {
                return Err(err(
                    &format!("MATLAB:{}:unsupportedMethod", fname),
                    format!(
                        "{}: method 'percentiles' is not supported in the Rust engine yet.",
                        fname
                    ),
                ));
            }
            _ => None,
        };
        if let Some(m) = m {
            method = m;
            i += 1;
        }
    }
    let mut dim = None;
    if i < a.len() && !is_text_arg(&a[i]) {
        dim = Some(dim_arg(&a[i], fname)?);
        i += 1;
    }
    let mut factor = None;
    let mut maxn = None;
    while i < a.len() {
        let Some(name) = opt_name(&a[i]) else {
            return Err(err(
                &format!("MATLAB:{}:InvalidOption", fname),
                format!("{}: expected a method or a name-value option name.", fname),
            ));
        };
        let Some(val) = a.get(i + 1) else {
            return Err(err(
                &format!("MATLAB:{}:missingValue", fname),
                format!("{}: option '{}' requires a value.", fname, name),
            ));
        };
        match name.as_str() {
            "thresholdfactor" => {
                let x = arg_f64(val, fname)?;
                if x.is_nan() || x < 0.0 {
                    return Err(err(
                        &format!("MATLAB:{}:ThresholdFactorInvalid", fname),
                        format!(
                            "{}: ThresholdFactor must be a nonnegative real scalar.",
                            fname
                        ),
                    ));
                }
                factor = Some(x);
            }
            "maxnumoutliers" => {
                let x = arg_f64(val, fname)?;
                if !(x >= 1.0 && x.fract() == 0.0 && x.is_finite()) {
                    return Err(err(
                        &format!("MATLAB:{}:MaxNumOutliersInvalid", fname),
                        format!(
                            "{}: MaxNumOutliers must be a positive integer scalar.",
                            fname
                        ),
                    ));
                }
                maxn = Some(x as usize);
            }
            "samplepoints" => {
                return Err(err(
                    &format!("MATLAB:{}:unsupportedOption", fname),
                    format!(
                        "{}: 'SamplePoints' is not supported in the Rust engine yet.",
                        fname
                    ),
                ));
            }
            "datavariables" | "outputformat" => {
                return Err(err(
                    &format!("MATLAB:{}:tableUnsupported", fname),
                    format!(
                        "{}: option '{}' applies to table input, which is not supported in the Rust engine yet.",
                        fname,
                        a[i].to_str().unwrap_or_default()
                    ),
                ));
            }
            other => {
                if !extra(other, val)? {
                    return Err(err(
                        &format!("MATLAB:{}:InvalidOption", fname),
                        format!(
                            "{}: unknown option '{}'.",
                            fname,
                            a[i].to_str().unwrap_or_default()
                        ),
                    ));
                }
            }
        }
        i += 2;
    }
    let factor = match (factor, method) {
        (Some(x), OMethod::Grubbs | OMethod::Gesd) => {
            if x > 1.0 {
                return Err(err(
                    &format!("MATLAB:{}:ThresholdFactorInvalid", fname),
                    format!(
                        "{}: ThresholdFactor must be between 0 and 1 for 'grubbs' and 'gesd'.",
                        fname
                    ),
                ));
            }
            x
        }
        (Some(x), _) => x,
        (None, OMethod::Quartiles) => 1.5,
        (None, OMethod::Grubbs | OMethod::Gesd) => 0.05,
        (None, _) => 3.0,
    };
    Ok(Detect {
        method,
        factor,
        maxn,
        dim,
    })
}

fn stat_value(v: &[f64], dims: &[usize], class: NumClass) -> Value {
    let n = Num::real(dims.to_vec(), v.to_vec());
    Value::Num(if class == NumClass::Single {
        n.with_class(NumClass::Single)
    } else {
        n
    })
}

pub fn isoutlier(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let f = "isoutlier";
    need(&a, 1, f)?;
    let x = real_data(&a[0], f)?;
    let d = parse_detect(&a, 1, f, &mut |_, _| Ok(false))?;
    let fo = detect(&x, &d);
    let cl = x.class;
    take_n(
        vec![
            Value::Bool(Array::new(x.dims.clone(), fo.tf)),
            stat_value(&fo.l, &fo.stat_dims, cl),
            stat_value(&fo.u, &fo.stat_dims, cl),
            stat_value(&fo.c, &fo.stat_dims, cl),
        ],
        nargout,
    )
}

pub fn rmoutliers(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let f = "rmoutliers";
    need(&a, 1, f)?;
    let v = &a[0];
    let x = real_data(v, f)?;
    let mut minnum = 1usize;
    let mut d = parse_detect(&a, 1, f, &mut |name, val| {
        if name == "minnumoutliers" {
            let k = arg_f64(val, f)?;
            if !(k >= 1.0 && k.fract() == 0.0 && k.is_finite()) {
                return Err(err(
                    "MATLAB:rmoutliers:MinNumOutliersInvalid",
                    "rmoutliers: MinNumOutliers must be a positive integer scalar.",
                ));
            }
            minnum = k as usize;
            return Ok(true);
        }
        Ok(false)
    })?;
    let dims = x.dims.clone();
    if dims.len() > 2 {
        return Err(err(
            "MATLAB:rmoutliers:NDArray",
            "rmoutliers: input must be a vector or a matrix.",
        ));
    }
    let rd = d
        .dim
        .unwrap_or(if dims[0] == 1 && dims[1] != 1 { 1 } else { 0 });
    if rd > 1 {
        return Err(err(
            "MATLAB:rmoutliers:InvalidDim",
            "rmoutliers: dimension must be 1 or 2.",
        ));
    }
    d.dim = Some(rd);
    let fo = detect(&x, &d);
    let (b, tf) = remove_flagged(v, &fo.tf, Some(fo.dim), minnum, f)?;
    take_n(vec![b, tf], nargout)
}

enum OFill {
    Const(f64),
    Center,
    Clip,
    Method(FMethod),
}

pub fn filloutliers(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let f = "filloutliers";
    need(&a, 2, f)?;
    let x = real_data(&a[0], f)?;
    let fill = match opt_name(&a[1]) {
        Some(s) => match s.as_str() {
            "center" => OFill::Center,
            "clip" => OFill::Clip,
            "previous" => OFill::Method(FMethod::Previous),
            "next" => OFill::Method(FMethod::Next),
            "nearest" => OFill::Method(FMethod::Nearest),
            "linear" => OFill::Method(FMethod::Interp("linear")),
            "spline" => OFill::Method(FMethod::Interp("spline")),
            "pchip" => OFill::Method(FMethod::Interp("pchip")),
            "makima" => OFill::Method(FMethod::Interp("makima")),
            other => {
                return Err(err(
                    "MATLAB:filloutliers:InvalidFillMethod",
                    format!("filloutliers: unknown fill method '{}'.", other),
                ));
            }
        },
        None => match &a[1] {
            Value::Num(n) if n.re.len() == 1 && n.im.is_none() => OFill::Const(n.re[0]),
            Value::Bool(b) if b.data.len() == 1 => OFill::Const(b.data[0] as u8 as f64),
            _ => {
                return Err(err(
                    "MATLAB:filloutliers:InvalidFillMethod",
                    "filloutliers: the fill method must be a method name or a real scalar.",
                ));
            }
        },
    };
    let d = parse_detect(&a, 2, f, &mut |name, _| {
        if name == "outlierlocations" {
            return Err(err(
                "MATLAB:filloutliers:unsupportedOption",
                "filloutliers: 'OutlierLocations' is not supported in the Rust engine yet.",
            ));
        }
        Ok(false)
    })?;
    let fo = detect(&x, &d);
    let moving = d.method.moving();
    let slices = slice_positions(&x.dims, fo.dim);
    let mut out = x.clone();
    match fill {
        OFill::Method(m) => {
            let bad: Vec<bool> = (0..x.re.len())
                .map(|k| fo.tf[k] || x.re[k].is_nan())
                .collect();
            let spec = Spec {
                method: m,
                ends: Ends::Extrap,
                t: None,
                maxgap: f64::INFINITY,
            };
            let (v, _) = fill_value(
                it,
                &Value::Num(x.clone()),
                fo.dim,
                &spec,
                &bad,
                &fo.tf,
                None,
            )?;
            if let Value::Num(n) = v {
                out = n;
            }
        }
        _ => {
            for (s, pos) in slices.iter().enumerate() {
                for p in pos {
                    if !fo.tf[*p] {
                        continue;
                    }
                    let si = if moving { *p } else { s };
                    let val = match fill {
                        OFill::Const(c) => c,
                        OFill::Center => fo.c[si],
                        OFill::Clip => {
                            if x.re[*p] < fo.l[si] {
                                fo.l[si]
                            } else {
                                fo.u[si]
                            }
                        }
                        OFill::Method(_) => continue,
                    };
                    out.re[*p] = out.class.cast(val);
                }
            }
        }
    }
    let cl = x.class;
    take_n(
        vec![
            Value::Num(out),
            Value::Bool(Array::new(x.dims.clone(), fo.tf)),
            stat_value(&fo.l, &fo.stat_dims, cl),
            stat_value(&fo.u, &fo.stat_dims, cl),
            stat_value(&fo.c, &fo.stat_dims, cl),
        ],
        nargout,
    )
}
