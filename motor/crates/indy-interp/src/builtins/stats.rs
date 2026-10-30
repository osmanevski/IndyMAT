//! Descriptive statistics: `quantile`, `prctile`, `iqr`, `cov`,
//! `corrcoef`, `normalize`.
//!
//! Own code from the textbook definitions (Hyndman–Fan quantile types
//! 1–9, default type 5 as in MATLAB).

use indy_value::{Num, NumClass, Value};

use super::math::{default_dim, slices};
use super::{arg_f64, arg_num, arg_str, err, need, one, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "quantile", |_, a, _| quantile_fn(&a, "quantile", 1.0));
    reg(it, "prctile", |_, a, _| quantile_fn(&a, "prctile", 100.0));
    reg(it, "iqr", iqr);
    reg(it, "cov", cov);
    reg(it, "corrcoef", corrcoef);
    reg(it, "normalize", normalize);
    reg(it, "cummax", |_, a, n| cumext(&a, true, n));
    reg(it, "cummin", |_, a, n| cumext(&a, false, n));
    reg(it, "movmedian", |_, a, _| movgeneric(&a, "movmedian"));
    reg(it, "movmax", |_, a, _| movgeneric(&a, "movmax"));
    reg(it, "movmin", |_, a, _| movgeneric(&a, "movmin"));
    reg(it, "movvar", |_, a, _| movgeneric(&a, "movvar"));
    reg(it, "movstd", |_, a, _| movgeneric(&a, "movstd"));
    reg(it, "movprod", |_, a, _| movgeneric(&a, "movprod"));
    reg(it, "movmad", |_, a, _| movgeneric(&a, "movmad"));
    reg(it, "del2", del2);
    reg(it, "var", |_, a, n| var_std(&a, "var", n));
    reg(it, "std", |_, a, n| var_std(&a, "std", n));
}

// ---------------------------------------------------------------------------
// numeric helpers
// ---------------------------------------------------------------------------

pub(super) fn real_arg(v: &Value, fname: &str) -> R<Num> {
    let x = arg_num(v, fname)?;
    if x.im.is_some() {
        return Err(err(
            "MATLAB:unsupported",
            format!("{}: complex input is not supported.", fname),
        ));
    }
    Ok(x)
}

/// Apply `f` to every slice along `dim`; each call returns `out_len`
/// values stored along `dim` of the result.
pub(super) fn along(
    x: &Num,
    dim: usize,
    out_len: usize,
    mut f: impl FnMut(&[f64]) -> Vec<f64>,
) -> Num {
    let mut dims = x.dims.clone();
    while dims.len() <= dim {
        dims.push(1);
    }
    let len = dims[dim];
    let stride: usize = dims.iter().take(dim).product();
    let mut odims = dims.clone();
    odims[dim] = out_len;
    let total: usize = odims.iter().product();
    let mut out = vec![0.0; total];
    if len == 0 {
        // empty operating dimension: `f` sees an empty slice for every output slot
        if stride > 0 && out_len > 0 {
            for s in 0..total / out_len {
                let o_start = (s / stride) * stride * out_len + (s % stride);
                for (k, v) in f(&[]).iter().enumerate().take(out_len) {
                    out[o_start + k * stride] = *v;
                }
            }
        }
        return Num::real(odims, out);
    }
    let mut buf = vec![0.0; len];
    slices(&dims, dim, |start, st, ln| {
        for k in 0..ln {
            buf[k] = x.re[start + k * st];
        }
        let r = f(&buf);
        let o_start = (start / (stride * len)) * stride * out_len + (start % stride);
        for (k, v) in r.iter().enumerate().take(out_len) {
            out[o_start + k * stride] = *v;
        }
    });
    Num::real(odims, out)
}

fn erfc(x: f64) -> f64 {
    // erfc via series (small |x|) and continued fraction (large |x|)
    if x < 0.0 {
        return 2.0 - erfc(-x);
    }
    if x < 2.5 {
        let mut sum = x;
        let mut term = x;
        for n in 1..200 {
            term *= -x * x / n as f64;
            let add = term / (2 * n + 1) as f64;
            sum += add;
            if add.abs() < 1e-18 * sum.abs() {
                break;
            }
        }
        return 1.0 - 2.0 / std::f64::consts::PI.sqrt() * sum;
    }
    // Lentz continued fraction for erfc
    let mut f = 0.0;
    for k in (1..80).rev() {
        f = k as f64 / 2.0 / (x + f);
    }
    (-x * x).exp() / (std::f64::consts::PI.sqrt() * (x + f))
}

fn norminv(p: f64) -> f64 {
    if !(0.0..=1.0).contains(&p) {
        return f64::NAN;
    }
    if p == 0.0 {
        return f64::NEG_INFINITY;
    }
    if p == 1.0 {
        return f64::INFINITY;
    }
    // bisection-refined Newton on the normal CDF
    let cdf = |x: f64| 0.5 * erfc(-x / std::f64::consts::SQRT_2);
    let (mut lo, mut hi) = (-40.0, 40.0);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if cdf(mid) < p {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

fn ln_gamma(x: f64) -> f64 {
    // Lanczos approximation (g = 7, n = 9)
    const C: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if x < 0.5 {
        return (std::f64::consts::PI / (std::f64::consts::PI * x).sin()).ln() - ln_gamma(1.0 - x);
    }
    let x = x - 1.0;
    let mut a = C[0];
    let t = x + 7.5;
    for (i, c) in C.iter().enumerate().skip(1) {
        a += c / (x + i as f64);
    }
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
}

/// Regularised incomplete beta function I_x(a, b).
fn betainc(x: f64, a: f64, b: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let bt = (ln_gamma(a + b) - ln_gamma(a) - ln_gamma(b) + a * x.ln() + b * (1.0 - x).ln()).exp();
    let cf = |x: f64, a: f64, b: f64| {
        let tiny = 1e-300;
        let (qab, qap, qam) = (a + b, a + 1.0, a - 1.0);
        let mut c = 1.0;
        let mut d = 1.0 - qab * x / qap;
        if d.abs() < tiny {
            d = tiny;
        }
        d = 1.0 / d;
        let mut h = d;
        for m in 1..500 {
            let m = m as f64;
            let m2 = 2.0 * m;
            let aa = m * (b - m) * x / ((qam + m2) * (a + m2));
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
            let aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
            d = 1.0 + aa * d;
            if d.abs() < tiny {
                d = tiny;
            }
            c = 1.0 + aa / c;
            if c.abs() < tiny {
                c = tiny;
            }
            d = 1.0 / d;
            let del = d * c;
            h *= del;
            if (del - 1.0).abs() < 1e-16 {
                break;
            }
        }
        h
    };
    if x < (a + 1.0) / (a + b + 2.0) {
        bt * cf(x, a, b) / a
    } else {
        1.0 - bt * cf(1.0 - x, b, a) / b
    }
}

// ---------------------------------------------------------------------------
// quantile / prctile / iqr
// ---------------------------------------------------------------------------

/// Quantile of sorted non-NaN data `s` at probability `p` (type 1–9).
fn quantile_sorted(s: &[f64], p: f64, method: u32) -> f64 {
    let n = s.len();
    if n == 0 || p.is_nan() || !(0.0..=1.0).contains(&p) {
        return f64::NAN;
    }
    let nf = n as f64;
    let at = |k: f64| s[(k as usize).clamp(1, n) - 1];
    match method {
        1 => at((nf * p).ceil().max(1.0)),
        2 => {
            let h = nf * p;
            if h.fract() == 0.0 && h >= 1.0 && h < nf {
                0.5 * (at(h) + at(h + 1.0))
            } else {
                at(h.ceil().max(1.0))
            }
        }
        3 => {
            let h = nf * p;
            let r = h.round();
            // round half to even
            let k = if (h - h.floor() - 0.5).abs() < 1e-15 && r % 2.0 != 0.0 {
                r - 1.0
            } else {
                r
            };
            at(k.max(1.0))
        }
        m => {
            let h = match m {
                4 => nf * p,
                5 => nf * p + 0.5,
                6 => (nf + 1.0) * p,
                7 => (nf - 1.0) * p + 1.0,
                8 => (nf + 1.0 / 3.0) * p + 1.0 / 3.0,
                _ => (nf + 0.25) * p + 0.375,
            };
            if h <= 1.0 {
                return s[0];
            }
            if h >= nf {
                return s[n - 1];
            }
            let j = h.floor();
            let g = h - j;
            let (lo, hi) = (at(j), at(j + 1.0));
            if g == 0.0 { lo } else { lo + g * (hi - lo) }
        }
    }
}

fn sorted_valid(v: &[f64]) -> Vec<f64> {
    let mut s: Vec<f64> = v.iter().copied().filter(|e| !e.is_nan()).collect();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    s
}

fn quantile_fn(a: &[Value], fname: &str, scale: f64) -> R<Vec<Value>> {
    need(a, 1, fname)?;
    let x = real_arg(&a[0], fname)?;
    let default_p: Vec<f64> = [0.0, 0.25, 0.5, 0.75, 1.0]
        .iter()
        .map(|v| v * scale)
        .collect();
    let p: Vec<f64> = match a.get(1) {
        Some(v) if !v.is_empty() => real_arg(v, fname)?.re,
        _ => default_p,
    };
    let dim = match a.get(2) {
        Some(v) if !v.is_empty() => {
            let d = arg_f64(v, fname)?;
            if d < 1.0 || d.fract() != 0.0 {
                return Err(err(
                    "MATLAB:getdimarg:dimensionMustBePositiveInteger",
                    "Dimension argument must be a positive integer scalar within indexing range.",
                ));
            }
            d as usize - 1
        }
        _ => default_dim(&x.dims),
    };
    let method = match a.get(3) {
        Some(v) => {
            let m = arg_f64(v, fname)?;
            if !(1.0..=9.0).contains(&m) || m.fract() != 0.0 {
                return Err(err(
                    "MATLAB:quantile:invalidMethod",
                    format!("{}: METHOD must be an integer from 1 to 9.", fname),
                ));
            }
            m as u32
        }
        None => 5,
    };
    if p.iter().any(|v| v.is_nan() || *v < 0.0 || *v > scale) {
        return Err(err(
            "MATLAB:quantile:invalidP",
            if scale == 1.0 {
                "quantile: P must contain probabilities between 0 and 1."
            } else {
                "prctile: P must contain percentages between 0 and 100."
            },
        ));
    }
    let out = along(&x, dim, p.len(), |slice| {
        let s = sorted_valid(slice);
        p.iter()
            .map(|pv| quantile_sorted(&s, pv / scale, method))
            .collect()
    });
    let keep = if x.class == NumClass::Single {
        NumClass::Single
    } else {
        NumClass::Double
    };
    one(Value::Num(out.with_class(keep)))
}

fn iqr(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "iqr")?;
    let mut x = real_arg(&a[0], "iqr")?;
    let mut dim: Option<usize> = None;
    for v in &a[1..] {
        if v.is_text() {
            let s = arg_str(v, "iqr")?.to_ascii_lowercase();
            if s == "all" {
                x = Num::real(vec![x.re.len(), 1], x.re.clone());
                dim = Some(0);
            } else {
                return Err(err(
                    "MATLAB:iqr:invalidOption",
                    format!("iqr: unknown option '{}'.", s),
                ));
            }
        } else {
            let d = arg_f64(v, "iqr")?;
            if d < 1.0 || d.fract() != 0.0 {
                return Err(err(
                    "MATLAB:getdimarg:dimensionMustBePositiveInteger",
                    "Dimension argument must be a positive integer scalar within indexing range.",
                ));
            }
            dim = Some(d as usize - 1);
        }
    }
    let dim = dim.unwrap_or_else(|| default_dim(&x.dims));
    let out = along(&x, dim, 1, |slice| {
        let s = sorted_valid(slice);
        vec![quantile_sorted(&s, 0.75, 5) - quantile_sorted(&s, 0.25, 5)]
    });
    one(Value::Num(out))
}

// ---------------------------------------------------------------------------
// cov / corrcoef
// ---------------------------------------------------------------------------

/// Observations x variables matrix from vector or matrix input.
fn obs_matrix(x: &Num, fname: &str) -> R<(usize, usize, Vec<f64>)> {
    if x.dims.len() > 2 {
        return Err(err(
            "MATLAB:cov:InputDim",
            format!("{}: input must be a vector or 2-D matrix.", fname),
        ));
    }
    if x.dims.len() == 2 && x.dims[0] == 1 && x.dims[1] > 1 {
        return Ok((x.dims[1], 1, x.re.clone()));
    }
    Ok((x.dims[0], x.dims[1], x.re.clone()))
}

/// Covariance of two columns over the rows where `rows` is true.
fn pair_cov(u: &[f64], v: &[f64], rows: &[bool], opt: f64) -> f64 {
    let idx: Vec<usize> = (0..rows.len()).filter(|i| rows[*i]).collect();
    let n = idx.len();
    if n == 0 {
        return f64::NAN;
    }
    let mu = idx.iter().map(|&i| u[i]).sum::<f64>() / n as f64;
    let mv = idx.iter().map(|&i| v[i]).sum::<f64>() / n as f64;
    let s: f64 = idx.iter().map(|&i| (u[i] - mu) * (v[i] - mv)).sum();
    let den = if opt == 1.0 || n == 1 {
        n as f64
    } else {
        (n - 1) as f64
    };
    if n == 1 && opt != 1.0 {
        return 0.0;
    }
    s / den
}

#[derive(Clone, Copy, PartialEq)]
enum NanMode {
    Include,
    OmitRows,
    Partial,
}

fn cov_matrix(n: usize, m: usize, d: &[f64], opt: f64, mode: NanMode) -> Vec<f64> {
    let col = |j: usize| &d[j * n..(j + 1) * n];
    let all_rows: Vec<bool> = match mode {
        NanMode::OmitRows => (0..n)
            .map(|i| (0..m).all(|j| !d[i + j * n].is_nan()))
            .collect(),
        _ => vec![true; n],
    };
    let mut out = vec![0.0; m * m];
    for i in 0..m {
        for j in i..m {
            let rows: Vec<bool> = match mode {
                NanMode::Partial => (0..n)
                    .map(|r| !col(i)[r].is_nan() && !col(j)[r].is_nan())
                    .collect(),
                _ => all_rows.clone(),
            };
            let c = pair_cov(col(i), col(j), &rows, opt);
            out[i + j * m] = c;
            out[j + i * m] = c;
        }
    }
    out
}

fn parse_nan_mode(v: &Value, fname: &str) -> R<NanMode> {
    let s = arg_str(v, fname)?.to_ascii_lowercase();
    Ok(match s.as_str() {
        "includenan" => NanMode::Include,
        "omitrows" => NanMode::OmitRows,
        "partialrows" => NanMode::Partial,
        _ => {
            return Err(err(
                "MATLAB:cov:invalidOption",
                format!("{}: unknown option '{}'.", fname, s),
            ));
        }
    })
}

fn cov(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "cov")?;
    let mut x = real_arg(&a[0], "cov")?;
    let mut opt = 0.0;
    let mut mode = NanMode::Include;
    let mut rest = &a[1..];
    // second data argument: a non-scalar numeric (or a scalar when the first is a scalar)
    if let Some(b) = rest.first()
        && !b.is_text()
        && (b.numel() != 1 || x.re.len() == 1 && rest.len() > 1 && !rest[1].is_text())
        && !b.is_empty()
    {
        let y = real_arg(b, "cov")?;
        if y.re.len() != x.re.len() {
            return Err(err(
                "MATLAB:cov:XYlengthMismatch",
                "cov: X and Y must have the same number of elements.",
            ));
        }
        let mut both = x.re.clone();
        both.extend_from_slice(&y.re);
        x = Num::real(vec![x.re.len(), 2], both);
        rest = &rest[1..];
    }
    for v in rest {
        if v.is_text() {
            mode = parse_nan_mode(v, "cov")?;
        } else {
            opt = arg_f64(v, "cov")?;
            if opt != 0.0 && opt != 1.0 {
                return Err(err(
                    "MATLAB:cov:invalidOpt",
                    "cov: normalization option must be 0 or 1.",
                ));
            }
        }
    }
    if x.re.is_empty() {
        return one(Value::num(f64::NAN));
    }
    let (n, m, d) = obs_matrix(&x, "cov")?;
    let c = cov_matrix(n, m, &d, opt, mode);
    one(Value::matrix(vec![m, m], c))
}

fn corrcoef(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "corrcoef")?;
    let mut x = real_arg(&a[0], "corrcoef")?;
    let mut rest = &a[1..];
    if let Some(b) = rest.first()
        && !b.is_text()
    {
        let y = real_arg(b, "corrcoef")?;
        if y.re.len() != x.re.len() {
            return Err(err(
                "MATLAB:corrcoef:XYlengthMismatch",
                "corrcoef: X and Y must have the same number of elements.",
            ));
        }
        let mut both = x.re.clone();
        both.extend_from_slice(&y.re);
        x = Num::real(vec![x.re.len(), 2], both);
        rest = &rest[1..];
    }
    let mut alpha = 0.05;
    let mut rows = "all".to_string();
    if !rest.len().is_multiple_of(2) {
        return Err(err(
            "MATLAB:corrcoef:nameValue",
            "corrcoef: name-value arguments must come in pairs.",
        ));
    }
    for kv in rest.chunks(2) {
        let name = arg_str(&kv[0], "corrcoef")?.to_ascii_lowercase();
        match name.as_str() {
            "alpha" => alpha = arg_f64(&kv[1], "corrcoef")?,
            "rows" => {
                rows = arg_str(&kv[1], "corrcoef")?.to_ascii_lowercase();
                if !matches!(rows.as_str(), "all" | "complete" | "pairwise") {
                    return Err(err(
                        "MATLAB:corrcoef:invalidRows",
                        "corrcoef: Rows must be 'all', 'complete' or 'pairwise'.",
                    ));
                }
            }
            _ => {
                return Err(err(
                    "MATLAB:corrcoef:unknownOption",
                    format!("corrcoef: unknown option '{}'.", name),
                ));
            }
        }
    }
    let (n, m, d) = obs_matrix(&x, "corrcoef")?;
    let col = |j: usize| &d[j * n..(j + 1) * n];
    let complete: Vec<bool> = (0..n)
        .map(|i| (0..m).all(|j| !d[i + j * n].is_nan()))
        .collect();
    let mut r = vec![0.0; m * m];
    let mut cnt = vec![0usize; m * m];
    for i in 0..m {
        for j in i..m {
            let sel: Vec<bool> = match rows.as_str() {
                "complete" => complete.clone(),
                "pairwise" => (0..n)
                    .map(|k| !col(i)[k].is_nan() && !col(j)[k].is_nan())
                    .collect(),
                _ => vec![true; n],
            };
            let ct = sel.iter().filter(|b| **b).count();
            let cij = pair_cov(col(i), col(j), &sel, 0.0);
            let cii = pair_cov(col(i), col(i), &sel, 0.0);
            let cjj = pair_cov(col(j), col(j), &sel, 0.0);
            let v = if i == j && ct > 1 && !cii.is_nan() {
                1.0
            } else {
                cij / (cii.sqrt() * cjj.sqrt())
            };
            let v = if ct < 2 { f64::NAN } else { v };
            r[i + j * m] = v;
            r[j + i * m] = v;
            cnt[i + j * m] = ct;
            cnt[j + i * m] = ct;
        }
    }
    let mut out = vec![Value::matrix(vec![m, m], r.clone())];
    if nargout > 1 {
        let mut p = vec![0.0; m * m];
        let mut lo = vec![0.0; m * m];
        let mut up = vec![0.0; m * m];
        let z = norminv(1.0 - alpha / 2.0);
        for k in 0..m * m {
            let (rv, nn) = (r[k], cnt[k] as f64);
            if k % m == k / m {
                p[k] = 1.0;
                lo[k] = 1.0;
                up[k] = 1.0;
                continue;
            }
            if nn > 2.0 && rv.abs() < 1.0 {
                let t = rv * ((nn - 2.0) / (1.0 - rv * rv)).sqrt();
                p[k] = betainc((nn - 2.0) / (nn - 2.0 + t * t), (nn - 2.0) / 2.0, 0.5);
            } else if nn > 2.0 && rv.abs() == 1.0 {
                p[k] = 0.0;
            } else {
                p[k] = f64::NAN;
            }
            if nn > 3.0 {
                let zr = rv.atanh();
                let se = 1.0 / (nn - 3.0).sqrt();
                lo[k] = (zr - z * se).tanh();
                up[k] = (zr + z * se).tanh();
            } else {
                lo[k] = f64::NAN;
                up[k] = f64::NAN;
            }
        }
        out.push(Value::matrix(vec![m, m], p));
        if nargout > 2 {
            out.push(Value::matrix(vec![m, m], lo));
        }
        if nargout > 3 {
            out.push(Value::matrix(vec![m, m], up));
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// normalize
// ---------------------------------------------------------------------------

fn mean_omit(v: &[f64]) -> f64 {
    let s: Vec<f64> = v.iter().copied().filter(|e| !e.is_nan()).collect();
    if s.is_empty() {
        f64::NAN
    } else {
        s.iter().sum::<f64>() / s.len() as f64
    }
}

fn std_omit(v: &[f64]) -> f64 {
    let s: Vec<f64> = v.iter().copied().filter(|e| !e.is_nan()).collect();
    let n = s.len();
    if n == 0 {
        return f64::NAN;
    }
    if n == 1 {
        return 0.0;
    }
    let m = s.iter().sum::<f64>() / n as f64;
    (s.iter().map(|e| (e - m).powi(2)).sum::<f64>() / (n - 1) as f64).sqrt()
}

fn median_omit(v: &[f64]) -> f64 {
    quantile_sorted(&sorted_valid(v), 0.5, 5)
}

fn mad_omit(v: &[f64]) -> f64 {
    let m = median_omit(v);
    let dev: Vec<f64> = v.iter().map(|e| (e - m).abs()).collect();
    median_omit(&dev)
}

fn normalize(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "normalize")?;
    let x = real_arg(&a[0], "normalize")?;
    let mut dim: Option<usize> = None;
    let mut rest = &a[1..];
    if let Some(v) = rest.first()
        && !v.is_text()
    {
        let d = arg_f64(v, "normalize")?;
        if d < 1.0 || d.fract() != 0.0 {
            return Err(err(
                "MATLAB:normalize:invalidDim",
                "normalize: DIM must be a positive integer.",
            ));
        }
        dim = Some(d as usize - 1);
        rest = &rest[1..];
    }
    let dim = dim.unwrap_or_else(|| default_dim(&x.dims));
    let mut method = "zscore".to_string();
    let mut opt: Option<&Value> = None;
    if let Some(v) = rest.first() {
        method = arg_str(v, "normalize")?.to_ascii_lowercase();
        rest = &rest[1..];
        if let Some(o) = rest.first() {
            opt = Some(o);
            rest = &rest[1..];
        }
    }
    if !rest.is_empty() {
        return Err(err(
            "MATLAB:normalize:unsupported",
            "normalize: only the method and one method option are supported.",
        ));
    }
    let opt_str = opt.and_then(|o| o.to_str()).map(|s| s.to_ascii_lowercase());
    let opt_num: Option<Vec<f64>> = match opt {
        Some(o) if !o.is_text() => Some(real_arg(o, "normalize")?.re),
        _ => None,
    };
    let bad = |m: &str| {
        err(
            "MATLAB:normalize:invalidOption",
            format!("normalize: {}", m),
        )
    };
    // (center, scale) for a slice; result = (v - center) / scale
    let cs = |v: &[f64]| -> R<(f64, f64)> {
        Ok(match method.as_str() {
            "zscore" => match opt_str.as_deref() {
                None | Some("std") => (mean_omit(v), std_omit(v)),
                Some("robust") => (median_omit(v), mad_omit(v)),
                Some(_) => return Err(bad("zscore option must be 'std' or 'robust'.")),
            },
            "norm" => {
                let p = match (&opt_num, &opt_str) {
                    (Some(n), _) if n.len() == 1 => n[0],
                    (_, Some(s)) if s == "inf" => f64::INFINITY,
                    (None, None) => 2.0,
                    _ => return Err(bad("norm option must be a positive scalar or Inf.")),
                };
                let f = v.iter().copied().filter(|e| !e.is_nan());
                let nrm = if p.is_infinite() {
                    f.map(f64::abs).fold(0.0, f64::max)
                } else {
                    f.map(|e| e.abs().powf(p)).sum::<f64>().powf(1.0 / p)
                };
                (0.0, nrm)
            }
            "scale" => {
                let s = match (&opt_num, opt_str.as_deref()) {
                    (Some(n), _) if n.len() == 1 => n[0],
                    (None, None) | (None, Some("mad")) => mad_omit(v),
                    (None, Some("std")) => std_omit(v),
                    (None, Some("first")) => {
                        v.iter().copied().find(|e| !e.is_nan()).unwrap_or(f64::NAN)
                    }
                    (None, Some("iqr")) => {
                        let s = sorted_valid(v);
                        quantile_sorted(&s, 0.75, 5) - quantile_sorted(&s, 0.25, 5)
                    }
                    _ => return Err(bad("invalid scale option.")),
                };
                (0.0, s)
            }
            "range" => {
                let (lo, hi) = match &opt_num {
                    Some(n) if n.len() == 2 => (n[0], n[1]),
                    None => (0.0, 1.0),
                    _ => return Err(bad("range option must be a two-element vector [a b].")),
                };
                let valid: Vec<f64> = v.iter().copied().filter(|e| !e.is_nan()).collect();
                let mn = valid.iter().copied().fold(f64::INFINITY, f64::min);
                let mx = valid.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                // (v - mn) / (mx - mn) * (hi - lo) + lo  ==  (v - c) / s
                let s = (mx - mn) / (hi - lo);
                (mn - lo * s, s)
            }
            "center" => {
                let c = match (&opt_num, opt_str.as_deref()) {
                    (Some(n), _) if n.len() == 1 => n[0],
                    (None, None) | (None, Some("mean")) => mean_omit(v),
                    (None, Some("median")) => median_omit(v),
                    _ => return Err(bad("invalid center option.")),
                };
                (c, 1.0)
            }
            "medianiqr" => {
                let s = sorted_valid(v);
                (
                    median_omit(v),
                    quantile_sorted(&s, 0.75, 5) - quantile_sorted(&s, 0.25, 5),
                )
            }
            other => return Err(bad(&format!("unknown method '{}'.", other))),
        })
    };
    // Collect (center, scale) per slice, then apply.
    let mut dims = x.dims.clone();
    while dims.len() <= dim {
        dims.push(1);
    }
    let mut out = x.re.clone();
    let mut cs_all: Vec<(usize, f64, f64)> = Vec::new();
    let mut failure = None;
    slices(&dims, dim, |start, st, ln| {
        let v: Vec<f64> = (0..ln).map(|k| x.re[start + k * st]).collect();
        match cs(&v) {
            Ok((c, s)) => {
                for k in 0..ln {
                    out[start + k * st] = (x.re[start + k * st] - c) / s;
                }
                cs_all.push((start, c, s));
            }
            Err(e) => failure = Some(e),
        }
    });
    if let Some(e) = failure {
        return Err(e);
    }
    let res = Num::real(x.dims.clone(), out);
    let mut vals = vec![Value::Num(res)];
    if nargout > 1 {
        let mut cd = dims.clone();
        cd[dim] = 1;
        let total: usize = cd.iter().product();
        let stride: usize = dims.iter().take(dim).product();
        let len = dims[dim];
        let mut cv = vec![0.0; total];
        let mut sv = vec![0.0; total];
        for (start, c, s) in &cs_all {
            let o = (start / (stride * len)) * stride + (start % stride);
            cv[o] = *c;
            sv[o] = *s;
        }
        vals.push(Value::Num(Num::real(cd.clone(), cv)));
        vals.push(Value::Num(Num::real(cd, sv)));
    }
    Ok(vals)
}

// ---------------------------------------------------------------------------
// var / std with weights, dim, 'all', nan flags
// ---------------------------------------------------------------------------

fn var_std(a: &[Value], fname: &str, nargout: usize) -> R<Vec<Value>> {
    need(a, 1, fname)?;
    let mut x = real_arg(&a[0], fname)?;
    let mut rest = a[1..].to_vec();
    let mut omit = false;
    rest.retain(|v| {
        if let Some(s) = v.to_str() {
            if s.eq_ignore_ascii_case("omitnan") {
                omit = true;
                return false;
            }
            if s.eq_ignore_ascii_case("includenan") {
                return false;
            }
        }
        true
    });
    let mut w: Option<Vec<f64>> = None;
    let mut wopt = 0.0;
    if let Some(v) = rest.first()
        && !v.is_empty()
    {
        let wv = real_arg(v, fname)?.re;
        if wv.len() == 1 {
            wopt = wv[0];
            if wopt != 0.0 && wopt != 1.0 {
                return Err(err(
                    "MATLAB:var:invalidWeight",
                    format!(
                        "{}: normalization weight must be 0, 1 or a vector of weights.",
                        fname
                    ),
                ));
            }
        } else {
            if wv.iter().any(|e| *e < 0.0 || e.is_nan()) {
                return Err(err(
                    "MATLAB:var:invalidWeight",
                    format!("{}: weights must be nonnegative.", fname),
                ));
            }
            w = Some(wv);
        }
    }
    let mut dim = None;
    if let Some(v) = rest.get(1) {
        if v.is_text() {
            if arg_str(v, fname)?.eq_ignore_ascii_case("all") {
                x = Num::real(vec![x.re.len(), 1], x.re.clone());
                dim = Some(0);
            }
        } else if !v.is_empty() {
            let d = arg_f64(v, fname)?;
            if d < 1.0 || d.fract() != 0.0 {
                return Err(err(
                    "MATLAB:getdimarg:dimensionMustBePositiveInteger",
                    "Dimension argument must be a positive integer scalar within indexing range.",
                ));
            }
            dim = Some(d as usize - 1);
        }
    }
    if x.dims == [0, 0] && dim.is_none() {
        return Ok(vec![Value::num(f64::NAN), Value::num(f64::NAN)]);
    }
    let dim = dim.unwrap_or_else(|| default_dim(&x.dims));
    let len = x.dims.get(dim).copied().unwrap_or(1);
    if let Some(wv) = &w
        && wv.len() != len
    {
        return Err(err(
            "MATLAB:var:invalidWeightLength",
            format!(
                "{}: weight vector length must match the operating dimension.",
                fname
            ),
        ));
    }
    let sd = fname == "std";
    let mut means = Vec::new();
    let out = along(&x, dim, 1, |slice| {
        let idx: Vec<usize> = (0..slice.len())
            .filter(|&i| !(omit && slice[i].is_nan()))
            .collect();
        let wt = |i: usize| w.as_ref().map_or(1.0, |wv| wv[i]);
        let sw: f64 = idx.iter().map(|&i| wt(i)).sum();
        let mu = idx.iter().map(|&i| wt(i) * slice[i]).sum::<f64>() / sw;
        let ss: f64 = idx.iter().map(|&i| wt(i) * (slice[i] - mu).powi(2)).sum();
        let n = idx.len() as f64;
        let denom = if w.is_some() {
            sw
        } else if wopt == 1.0 || n <= 1.0 {
            n
        } else {
            n - 1.0
        };
        means.push(mu);
        let r = if n == 0.0 { f64::NAN } else { ss / denom };
        vec![if sd { r.sqrt() } else { r }]
    });
    let mut res = vec![Value::Num(out.clone())];
    if nargout > 1 {
        res.push(Value::Num(Num::real(out.dims.clone(), means)));
    }
    Ok(res)
}

// ---------------------------------------------------------------------------
// cummax / cummin with index output, 'reverse'
// ---------------------------------------------------------------------------

fn cumext(a: &[Value], max: bool, nargout: usize) -> R<Vec<Value>> {
    let fname = if max { "cummax" } else { "cummin" };
    need(a, 1, fname)?;
    let x = real_arg(&a[0], fname)?;
    let mut reverse = false;
    let mut dim = None;
    for v in &a[1..] {
        if v.is_text() {
            match arg_str(v, fname)?.to_ascii_lowercase().as_str() {
                "reverse" => reverse = true,
                "forward" => {}
                o => {
                    return Err(err(
                        "MATLAB:invalidOption",
                        format!("{}: unknown option '{}'.", fname, o),
                    ));
                }
            }
        } else {
            let d = arg_f64(v, fname)?;
            if d < 1.0 || d.fract() != 0.0 {
                return Err(err(
                    "MATLAB:getdimarg:dimensionMustBePositiveInteger",
                    "Dimension argument must be a positive integer scalar within indexing range.",
                ));
            }
            dim = Some(d as usize - 1);
        }
    }
    let dim = dim.unwrap_or_else(|| default_dim(&x.dims));
    let mut dims = x.dims.clone();
    while dims.len() <= dim {
        dims.push(1);
    }
    let mut re = x.re.clone();
    let mut idx = vec![0.0; x.re.len()];
    slices(&dims, dim, |start, st, len| {
        let mut best = f64::NAN;
        let mut bi = 0usize;
        for kk in 0..len {
            let k = if reverse { len - 1 - kk } else { kk };
            let v = x.re[start + k * st];
            if best.is_nan() && !v.is_nan() || (max && v > best) || (!max && v < best) {
                best = v;
                bi = k;
            }
            re[start + k * st] = best;
            idx[start + k * st] = (bi + 1) as f64;
        }
    });
    let mut out = vec![Value::Num(
        Num::real(x.dims.clone(), re).with_class(x.class),
    )];
    if nargout > 1 {
        out.push(Value::Num(Num::real(x.dims.clone(), idx)));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// moving-window statistics (median, max, min, var, std, prod, mad)
// ---------------------------------------------------------------------------

fn movgeneric(a: &[Value], fname: &str) -> R<Vec<Value>> {
    need(a, 2, fname)?;
    let x = real_arg(&a[0], fname)?;
    let w = real_arg(&a[1], fname)?.re;
    let (nb, na) = match w.len() {
        1 => {
            let k = w[0];
            if k < 1.0 || k.fract() != 0.0 {
                return Err(err(
                    "MATLAB:movfun:wlenInvalid",
                    format!("{}: window length must be a positive integer.", fname),
                ));
            }
            let k = k as usize;
            if k % 2 == 1 {
                ((k - 1) / 2, (k - 1) / 2)
            } else {
                (k / 2, k / 2 - 1)
            }
        }
        2 => (w[0].max(0.0) as usize, w[1].max(0.0) as usize),
        _ => {
            return Err(err(
                "MATLAB:movfun:wlenInvalid",
                format!(
                    "{}: window must be a scalar or a two-element vector.",
                    fname
                ),
            ));
        }
    };
    let mut dim = None;
    let mut omit = false;
    let mut endpoints = "shrink".to_string();
    let mut pad = 0.0;
    let mut i = 2;
    while i < a.len() {
        if a[i].is_text() {
            let s = arg_str(&a[i], fname)?.to_ascii_lowercase();
            match s.as_str() {
                "omitnan" => omit = true,
                "includenan" => omit = false,
                "endpoints" => {
                    let v = a.get(i + 1).ok_or_else(|| {
                        err(
                            "MATLAB:movfun:nameValue",
                            format!("{}: missing Endpoints value.", fname),
                        )
                    })?;
                    if v.is_text() {
                        endpoints = arg_str(v, fname)?.to_ascii_lowercase();
                    } else {
                        endpoints = "value".into();
                        pad = arg_f64(v, fname)?;
                    }
                    i += 1;
                }
                other => {
                    return Err(err(
                        "MATLAB:movfun:invalidOption",
                        format!("{}: unknown option '{}'.", fname, other),
                    ));
                }
            }
        } else {
            let d = arg_f64(&a[i], fname)?;
            if d < 1.0 || d.fract() != 0.0 {
                return Err(err(
                    "MATLAB:getdimarg:dimensionMustBePositiveInteger",
                    "Dimension argument must be a positive integer scalar within indexing range.",
                ));
            }
            dim = Some(d as usize - 1);
        }
        i += 1;
    }
    let dim = dim.unwrap_or_else(|| default_dim(&x.dims));
    let len = x.dims.get(dim).copied().unwrap_or(1);
    let discard = endpoints == "discard";
    let first = if discard { nb.min(len) } else { 0 };
    let last = if discard {
        len.saturating_sub(na).max(first)
    } else {
        len
    };
    let out_len = last - first;
    let stat = |v: &[f64]| -> f64 {
        let vals: Vec<f64> = if omit {
            v.iter().copied().filter(|e| !e.is_nan()).collect()
        } else {
            v.to_vec()
        };
        if vals.is_empty() {
            return f64::NAN;
        }
        if vals.iter().any(|e| e.is_nan()) && fname != "movmax" && fname != "movmin" {
            return f64::NAN;
        }
        match fname {
            "movmedian" => median_omit(&vals),
            "movmax" => vals
                .iter()
                .copied()
                .filter(|e| !e.is_nan())
                .fold(f64::NAN, |m, e| if m.is_nan() || e > m { e } else { m }),
            "movmin" => vals
                .iter()
                .copied()
                .filter(|e| !e.is_nan())
                .fold(f64::NAN, |m, e| if m.is_nan() || e < m { e } else { m }),
            "movvar" => std_omit(&vals).powi(2),
            "movstd" => std_omit(&vals),
            "movprod" => vals.iter().product(),
            _ => mad_omit(&vals),
        }
    };
    let out = along(&x, dim, out_len, |slice| {
        (first..last)
            .map(|c| {
                let lo = c as isize - nb as isize;
                let hi = c as isize + na as isize;
                let mut win = Vec::new();
                for k in lo..=hi {
                    if k >= 0 && (k as usize) < len {
                        win.push(slice[k as usize]);
                    } else if endpoints == "fill" {
                        win.push(f64::NAN);
                    } else if endpoints == "value" {
                        win.push(pad);
                    }
                }
                stat(&win)
            })
            .collect()
    });
    one(Value::Num(out))
}

// ---------------------------------------------------------------------------
// del2 (discrete Laplacian, uniform spacing)
// ---------------------------------------------------------------------------

fn del2(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "del2")?;
    let x = real_arg(&a[0], "del2")?;
    let hs: Vec<f64> = match a.len() {
        1 => vec![1.0; x.dims.len()],
        2 => {
            let h = arg_f64(&a[1], "del2")?;
            vec![h; x.dims.len()]
        }
        _ => {
            let mut v = Vec::new();
            for h in &a[1..] {
                v.push(arg_f64(h, "del2")?);
            }
            v
        }
    };
    let vec_input = x.dims.len() == 2 && (x.dims[0] == 1 || x.dims[1] == 1);
    let dims_used: Vec<usize> = if vec_input {
        vec![if x.dims[0] == 1 { 1 } else { 0 }]
    } else {
        (0..x.dims.len()).collect()
    };
    if hs.len() < dims_used.len() && a.len() > 2 {
        return Err(err(
            "MATLAB:del2:spacing",
            "del2: a spacing value is required for every dimension.",
        ));
    }
    let mut total = vec![0.0; x.re.len()];
    let mut count = 0.0;
    for (k, &dim) in dims_used.iter().enumerate() {
        // the first spacing belongs to x, i.e. to dimension 2
        let slot = if vec_input {
            0
        } else {
            [1, 0].get(dim).copied().unwrap_or(dim)
        };
        let h = *hs.get(slot).unwrap_or(&hs[0]);
        let _ = k;
        let len = x.dims[dim];
        if len < 3 {
            continue;
        }
        count += 1.0;
        let d2 = along(&x, dim, len, |s| {
            let mut r = vec![0.0; len];
            for i in 1..len - 1 {
                r[i] = (s[i - 1] - 2.0 * s[i] + s[i + 1]) / (h * h);
            }
            r[0] = 2.0 * r[1] - if len > 3 { r[2] } else { r[1] };
            r[len - 1] = 2.0 * r[len - 2] - if len > 3 { r[len - 3] } else { r[len - 2] };
            r
        });
        for (t, v) in total.iter_mut().zip(&d2.re) {
            *t += v;
        }
    }
    let _ = count;
    let nd = dims_used.len() as f64;
    let scale = if nd <= 1.0 { 0.25 } else { 1.0 / (2.0 * nd) };
    let out: Vec<f64> = total.iter().map(|v| v * scale).collect();
    one(Value::Num(Num::real(x.dims.clone(), out)))
}
