//! `smoothdata`: moving-window smoothing along one dimension.
//!
//! Windows follow the `movmean` rules and shrink at the ends. NaN values
//! are left out of each window by default ('omitnan'); with 'includenan'
//! a window holding a NaN gives NaN.
//!
//! - movmean / movmedian: window mean / median
//! - gaussian: weights exp(-(d/sigma)^2/2), sigma = window/5, renormalised
//!   over the points present (scalar window only)
//! - lowess / loess: local weighted linear / quadratic regression with
//!   tricube weights (1-(|d|/dmax)^3)^3, dmax the largest distance in the
//!   window (the definition documented for Curve Fitting Toolbox `smooth`)
//! - rlowess / rloess: as above, then four robust re-fits with bisquare
//!   weights on residuals scaled by 6·median(|residual|)
//! - sgolay: unweighted least-squares polynomial of 'Degree' (default 2)
//!   per window; the degree is lowered where a shrunken window has fewer
//!   points
//!
//! The automatic window heuristic of MATLAB is not implemented: a window
//! must be given.

use indy_value::{Num, NumClass, Value};

use super::super::{arg_f64, err, need};
use super::{
    dim_arg, first_dim, is_text_arg, mean_omit, median_omit, opt_name, real_data, slice_positions,
    take_n, window_arg,
};
use crate::{Interp, R};

#[derive(Clone, Copy, PartialEq)]
enum SMethod {
    MovMean,
    MovMedian,
    Gaussian,
    Lowess,
    Loess,
    RLowess,
    RLoess,
    Sgolay,
}

fn method_of(s: &str) -> Option<SMethod> {
    Some(match s {
        "movmean" => SMethod::MovMean,
        "movmedian" => SMethod::MovMedian,
        "gaussian" => SMethod::Gaussian,
        "lowess" => SMethod::Lowess,
        "loess" => SMethod::Loess,
        "rlowess" => SMethod::RLowess,
        "rloess" => SMethod::RLoess,
        "sgolay" => SMethod::Sgolay,
        _ => return None,
    })
}

/// Weighted least-squares polynomial of degree `deg` through (x, y) with
/// weights w, evaluated at x = 0. The degree drops when the system is
/// singular. NaN when no point has positive weight.
fn local_fit(x: &[f64], y: &[f64], w: &[f64], deg: usize) -> f64 {
    let scale = x.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let npos = w.iter().filter(|v| **v > 0.0).count();
    if npos == 0 {
        return f64::NAN;
    }
    let mut d = deg.min(npos - 1);
    loop {
        let m = d + 1;
        // normal equations
        let mut a = vec![vec![0.0; m + 1]; m];
        for k in 0..x.len() {
            if w[k] <= 0.0 {
                continue;
            }
            let xs = x[k] / scale;
            let mut pw = vec![1.0; 2 * m];
            for p in 1..2 * m {
                pw[p] = pw[p - 1] * xs;
            }
            for r in 0..m {
                for c in 0..m {
                    a[r][c] += w[k] * pw[r + c];
                }
                a[r][m] += w[k] * pw[r] * y[k];
            }
        }
        // Gaussian elimination with partial pivoting
        let mut ok = true;
        let norm = a
            .iter()
            .flat_map(|r| r.iter())
            .fold(0.0f64, |s, v| s.max(v.abs()));
        for col in 0..m {
            let piv = (col..m)
                .max_by(|i, j| {
                    a[*i][col]
                        .abs()
                        .partial_cmp(&a[*j][col].abs())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap_or(col);
            if a[piv][col].abs() <= 1e-12 * norm.max(f64::MIN_POSITIVE) {
                ok = false;
                break;
            }
            a.swap(col, piv);
            for r in 0..m {
                if r != col {
                    let fct = a[r][col] / a[col][col];
                    for c in col..=m {
                        a[r][c] -= fct * a[col][c];
                    }
                }
            }
        }
        if ok {
            return a[0][m] / a[0][0];
        }
        if d == 0 {
            return f64::NAN;
        }
        d -= 1;
    }
}

struct Opts {
    method: SMethod,
    b: usize,
    f: usize,
    width: f64,
    omit: bool,
    degree: usize,
}

fn smooth_slice(v: &[f64], o: &Opts) -> Vec<f64> {
    let n = v.len();
    let mut out = vec![f64::NAN; n];
    let robust = matches!(o.method, SMethod::RLowess | SMethod::RLoess);
    let deg = match o.method {
        SMethod::Lowess | SMethod::RLowess => 1,
        SMethod::Loess | SMethod::RLoess => 2,
        _ => o.degree,
    };
    let mut rw = vec![1.0; n];
    let scale = v
        .iter()
        .filter(|x| x.is_finite())
        .fold(0.0f64, |m, x| m.max(x.abs()));
    let passes = if robust { 5 } else { 1 };
    for pass in 0..passes {
        let before = out.clone();
        for i in 0..n {
            let lo = i.saturating_sub(o.b);
            let hi = i.saturating_add(o.f).min(n - 1);
            let mut xs = Vec::new();
            let mut ys = Vec::new();
            let mut ws = Vec::new();
            let mut has_nan = false;
            for j in lo..=hi {
                if v[j].is_nan() {
                    has_nan = true;
                    continue;
                }
                xs.push(j as f64 - i as f64);
                ys.push(v[j]);
                ws.push(rw[j]);
            }
            if has_nan && !o.omit {
                out[i] = f64::NAN;
                continue;
            }
            if ys.is_empty() {
                out[i] = f64::NAN;
                continue;
            }
            out[i] = match o.method {
                SMethod::MovMean => mean_omit(&ys),
                SMethod::MovMedian => median_omit(&ys),
                SMethod::Gaussian => {
                    let sigma = o.width / 5.0;
                    let (mut s, mut sw) = (0.0, 0.0);
                    for (x, y) in xs.iter().zip(&ys) {
                        let w = (-0.5 * (x / sigma).powi(2)).exp();
                        s += w * y;
                        sw += w;
                    }
                    s / sw
                }
                SMethod::Sgolay => {
                    let w = vec![1.0; xs.len()];
                    local_fit(&xs, &ys, &w, deg)
                }
                _ => {
                    let dmax = xs.iter().fold(0.0f64, |m, x| m.max(x.abs()));
                    let w: Vec<f64> = xs
                        .iter()
                        .zip(&ws)
                        .map(|(x, r)| {
                            let t = if dmax > 0.0 {
                                (1.0 - (x.abs() / dmax).powi(3)).powi(3)
                            } else {
                                1.0
                            };
                            t * r
                        })
                        .collect();
                    local_fit(&xs, &ys, &w, deg)
                }
            };
            // A robust re-fit whose window lost every weight keeps the
            // previous pass value.
            if pass > 0 && out[i].is_nan() && !before[i].is_nan() {
                out[i] = before[i];
            }
        }
        if pass + 1 < passes {
            let res: Vec<f64> = (0..n).map(|k| (v[k] - out[k]).abs()).collect();
            let mad = median_omit(&res);
            // Stop when the fit is already exact up to rounding.
            if mad.is_nan() || mad <= 1e-10 * scale {
                break;
            }
            for k in 0..n {
                let u = res[k] / (6.0 * mad);
                rw[k] = if u.is_nan() || u >= 1.0 {
                    0.0
                } else {
                    (1.0 - u * u).powi(2)
                };
            }
        }
    }
    out
}

pub fn smoothdata(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let fname = "smoothdata";
    need(&a, 1, fname)?;
    let x = real_data(&a[0], fname)?;
    let mut i = 1;
    let mut dim = None;
    if i < a.len() && !is_text_arg(&a[i]) {
        dim = Some(dim_arg(&a[i], fname)?);
        i += 1;
    }
    let mut method = SMethod::MovMean;
    if let Some(m) = a.get(i).and_then(opt_name).and_then(|s| method_of(&s)) {
        method = m;
        i += 1;
    }
    let mut window: Option<Value> = None;
    if i < a.len() && !is_text_arg(&a[i]) {
        window = Some(a[i].clone());
        i += 1;
    }
    let mut omit = true;
    let mut degree: Option<usize> = None;
    while i < a.len() {
        let Some(name) = opt_name(&a[i]) else {
            return Err(err(
                "MATLAB:smoothdata:InvalidOption",
                "smoothdata: expected a method, a NaN flag or a name-value option name.",
            ));
        };
        match name.as_str() {
            "omitnan" | "omitmissing" => {
                omit = true;
                i += 1;
                continue;
            }
            "includenan" | "includemissing" => {
                omit = false;
                i += 1;
                continue;
            }
            _ => {}
        }
        let Some(val) = a.get(i + 1) else {
            return Err(err(
                "MATLAB:smoothdata:missingValue",
                format!("smoothdata: option '{}' requires a value.", name),
            ));
        };
        match name.as_str() {
            "degree" => {
                let d = arg_f64(val, fname)?;
                if !(d >= 0.0 && d.fract() == 0.0 && d.is_finite()) {
                    return Err(err(
                        "MATLAB:smoothdata:DegreeInvalid",
                        "smoothdata: Degree must be a nonnegative integer scalar.",
                    ));
                }
                degree = Some(d as usize);
            }
            "samplepoints" | "smoothingfactor" => {
                return Err(err(
                    "MATLAB:smoothdata:unsupportedOption",
                    format!(
                        "smoothdata: option '{}' is not supported in the Rust engine yet.",
                        a[i].to_str().unwrap_or_default()
                    ),
                ));
            }
            "datavariables" | "replacevalues" => {
                return Err(err(
                    "MATLAB:smoothdata:tableUnsupported",
                    format!(
                        "smoothdata: option '{}' applies to table input, which is not supported in the Rust engine yet.",
                        a[i].to_str().unwrap_or_default()
                    ),
                ));
            }
            _ => {
                return Err(err(
                    "MATLAB:smoothdata:InvalidOption",
                    format!(
                        "smoothdata: unknown method or option '{}'.",
                        a[i].to_str().unwrap_or_default()
                    ),
                ));
            }
        }
        i += 2;
    }
    if degree.is_some() && method != SMethod::Sgolay {
        return Err(err(
            "MATLAB:smoothdata:DegreeInvalid",
            "smoothdata: 'Degree' applies only to the 'sgolay' method.",
        ));
    }
    let Some(window) = window else {
        return Err(err(
            "MATLAB:smoothdata:unsupportedWindow",
            "smoothdata: automatic window selection is not supported in the Rust engine yet; give an explicit window length.",
        ));
    };
    let dims = x.dims.clone();
    let dim = dim.unwrap_or_else(|| first_dim(&dims));
    let n = dims.get(dim).copied().unwrap_or(1);
    let (b, f, width) = if method == SMethod::Gaussian {
        let w = match &window {
            Value::Num(wn) if wn.re.len() == 1 && wn.im.is_none() => wn.re[0],
            _ => {
                return Err(err(
                    "MATLAB:smoothdata:wrongWindowLength",
                    "smoothdata: the 'gaussian' method requires a positive scalar window length.",
                ));
            }
        };
        window_arg(&window, fname)?;
        let capped = w.min(2.0 * n as f64);
        let b = (capped / 2.0).floor().max(0.0) as usize;
        let f = ((capped - 1.0) / 2.0).floor().max(0.0) as usize;
        (b, f, w)
    } else {
        let (b, f) = window_arg(&window, fname)?;
        (b, f, b as f64 + f as f64 + 1.0)
    };
    let degree = degree.unwrap_or(2);
    if method == SMethod::Sgolay && (degree as f64) >= (b as f64 + f as f64 + 1.0) {
        return Err(err(
            "MATLAB:smoothdata:DegreeInvalid",
            "smoothdata: 'Degree' must be less than the window length.",
        ));
    }
    let o = Opts {
        method,
        b,
        f,
        width,
        omit,
        degree,
    };
    let mut out = Num::real(dims.clone(), x.re.clone());
    for pos in slice_positions(&dims, dim) {
        let v: Vec<f64> = pos.iter().map(|p| x.re[*p]).collect();
        let r = smooth_slice(&v, &o);
        for (k, p) in pos.iter().enumerate() {
            out.re[*p] = r[k];
        }
    }
    if x.class == NumClass::Single {
        out = out.with_class(NumClass::Single);
    }
    take_n(vec![Value::Num(out), window], nargout)
}
