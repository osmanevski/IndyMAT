//! Linear algebra on top of `indy_value::cmat`: complex (and exact
//! real-SVD based) `det inv lu qr svd null orth pinv norm`, plus `cond`,
//! `chol`, `sqrtm`, `logm`, `bitcmp` and `periodogram`.
//!
//! Real inputs keep going to the earlier implementations (aliased as
//! `__real_<name>` at registration time); complex inputs and the
//! matrix 2-norm use the kernel here.

use indy_value::cmat::{
    CM, chol as cm_chol, complete_basis, det as cm_det, logm as cm_logm, lu as cm_lu, qr_p,
    solve_square, sqrtm as cm_sqrtm, svd_thin,
};
use indy_value::ops::{c_abs, c_log, c_sqrt};
use indy_value::{Num, NumClass, Value};

use super::{arg_f64, arg_num, arg_str, err, need, one, reg};
use crate::{Interp, R};

const ALIASED: [(&str, &str); 9] = [
    ("det", "__real_det"),
    ("inv", "__real_inv"),
    ("lu", "__real_lu"),
    ("qr", "__real_qr"),
    ("svd", "__real_svd"),
    ("null", "__real_null"),
    ("orth", "__real_orth"),
    ("pinv", "__real_pinv"),
    ("norm", "__real_norm"),
];

pub fn register(it: &mut Interp) {
    for (name, alias) in ALIASED {
        if let Some(f) = it.builtins.get(name).copied() {
            it.builtins.insert(alias, f);
        }
    }
    reg(it, "det", |it, a, n| {
        route(it, a, n, "__real_det", is_complex, det)
    });
    reg(it, "inv", |it, a, n| {
        route(it, a, n, "__real_inv", is_complex, inv)
    });
    reg(it, "lu", |it, a, n| {
        route(it, a, n, "__real_lu", is_complex, lu)
    });
    reg(it, "qr", |it, a, n| {
        route(it, a, n, "__real_qr", is_complex, qr)
    });
    reg(it, "svd", |it, a, n| {
        route(it, a, n, "__real_svd", is_complex, svd)
    });
    reg(it, "null", |it, a, n| {
        route(it, a, n, "__real_null", is_complex, null)
    });
    reg(it, "orth", |it, a, n| {
        route(it, a, n, "__real_orth", is_complex, orth)
    });
    reg(it, "pinv", |it, a, n| {
        route(it, a, n, "__real_pinv", is_complex, pinv)
    });
    reg(it, "norm", |it, a, n| {
        route(it, a, n, "__real_norm", norm2_case, norm2)
    });
    reg(it, "cond", cond);
    reg(it, "chol", chol);
    reg(it, "sqrtm", |_, a, _| matfun(&a, "sqrtm"));
    reg(it, "logm", |_, a, _| matfun(&a, "logm"));
    reg(it, "bitcmp", bitcmp);
    reg(it, "bitshift", bitshift);
    reg(it, "trace", trace);
    reg(it, "kron", kron);
    reg(it, "periodogram", periodogram);
}

fn route(
    it: &mut Interp,
    a: Vec<Value>,
    nargout: usize,
    old: &'static str,
    pred: fn(&[Value]) -> bool,
    new: fn(&mut Interp, Vec<Value>, usize) -> R<Vec<Value>>,
) -> R<Vec<Value>> {
    if pred(&a) {
        return new(it, a, nargout);
    }
    match it.builtins.get(old).copied() {
        Some(f) => f(it, a, nargout),
        None => new(it, a, nargout),
    }
}

fn is_complex(a: &[Value]) -> bool {
    matches!(a.first(), Some(Value::Num(n)) if n.im.is_some())
}

fn matrix_of(v: &Value, fname: &str) -> R<(CM, NumClass)> {
    let x = arg_num(v, fname)?;
    if x.dims.len() != 2 {
        return Err(err(
            "MATLAB:unsupported",
            format!("{}: only 2-D matrices are supported.", fname),
        ));
    }
    Ok((CM::from_num(&x), x.class))
}

fn val(c: &CM) -> Value {
    Value::Num(c.to_num())
}

fn real_val(m: usize, n: usize, d: Vec<f64>) -> Value {
    Value::matrix(vec![m, n], d)
}

fn det(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "det")?;
    let (m, _) = matrix_of(&a[0], "det")?;
    if m.m != m.n {
        return Err(err("MATLAB:square", "det: matrix must be square."));
    }
    let d = cm_det(&m);
    one(Value::Num(Num::complex(vec![1, 1], vec![d.0], vec![d.1])))
}

fn inv(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "inv")?;
    let (m, _) = matrix_of(&a[0], "inv")?;
    if m.m != m.n {
        return Err(err("MATLAB:square", "inv: matrix must be square."));
    }
    let (x, sing) = solve_square(&m, &CM::eye(m.n));
    if sing {
        it.warning(
            "MATLAB:singularMatrix",
            "Matrix is singular to working precision.",
        );
        return one(Value::Num(Num::real(
            vec![m.n, m.n],
            vec![f64::INFINITY; m.n * m.n],
        )));
    }
    one(val(&x))
}

fn lu(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "lu")?;
    let (m, _) = matrix_of(&a[0], "lu")?;
    let vector = a
        .iter()
        .skip(1)
        .any(|v| v.to_str().is_some_and(|s| s.eq_ignore_ascii_case("vector")));
    let (w, piv, _, _) = cm_lu(&m);
    let (r, c) = (m.m, m.n);
    let k = r.min(c);
    if nargout <= 1 {
        return one(val(&w));
    }
    let mut l = CM::zeros(r, k);
    let mut u = CM::zeros(k, c);
    for j in 0..k {
        l.set(j, j, (1.0, 0.0));
        for i in j + 1..r {
            l.set(i, j, w.at(i, j));
        }
    }
    for i in 0..k {
        for j in i..c {
            u.set(i, j, w.at(i, j));
        }
    }
    if nargout == 2 {
        // L := P' * L, i.e. row piv[i] of the result is row i of L
        let mut pl = CM::zeros(r, k);
        for i in 0..r {
            for j in 0..k {
                pl.set(piv[i], j, l.at(i, j));
            }
        }
        return Ok(vec![val(&pl), val(&u)]);
    }
    let p = if vector {
        Value::row(piv.iter().map(|x| *x as f64 + 1.0).collect())
    } else {
        let mut pm = vec![0.0; r * r];
        for (i, &s) in piv.iter().enumerate() {
            pm[i + s * r] = 1.0;
        }
        real_val(r, r, pm)
    };
    Ok(vec![val(&l), val(&u), p])
}

fn qr(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "qr")?;
    let (m, _) = matrix_of(&a[0], "qr")?;
    let (r, c) = (m.m, m.n);
    let econ = a.len() > 1
        && (a[1].scalar_f64() == Some(0.0)
            || a[1].to_str().is_some_and(|s| s == "econ" || s == "vector"));
    let vector = a
        .get(1)
        .and_then(|v| v.to_str())
        .is_some_and(|s| s == "vector")
        || (econ && a.get(1).and_then(|v| v.scalar_f64()) == Some(0.0));
    let (q, rr, perm) = qr_p(&m, nargout >= 3);
    let k = r.min(c);
    let (q, rr) = if econ && r > c {
        let mut qe = CM::zeros(r, k);
        let mut re = CM::zeros(k, c);
        for j in 0..k {
            for i in 0..r {
                qe.set(i, j, q.at(i, j));
            }
        }
        for j in 0..c {
            for i in 0..k {
                re.set(i, j, rr.at(i, j));
            }
        }
        (qe, re)
    } else {
        (q, rr)
    };
    if nargout <= 1 {
        return one(val(&rr));
    }
    let mut out = vec![val(&q), val(&rr)];
    if nargout >= 3 {
        out.push(if econ || vector {
            Value::row(perm.iter().map(|x| *x as f64 + 1.0).collect())
        } else {
            let mut pm = vec![0.0; c * c];
            for (j, &s) in perm.iter().enumerate() {
                pm[s + j * c] = 1.0;
            }
            real_val(c, c, pm)
        });
    }
    Ok(out)
}

fn svd(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "svd")?;
    let (m, _) = matrix_of(&a[0], "svd")?;
    let econ = a
        .get(1)
        .is_some_and(|v| v.scalar_f64() == Some(0.0) || v.to_str().is_some_and(|s| s == "econ"));
    let (u, s, v) = svd_thin(&m);
    if nargout <= 1 {
        return one(Value::col(s));
    }
    let (r, c) = (m.m, m.n);
    let k = r.min(c);
    let (uu, vv, sr, sc) = if econ {
        (u, v, k, k)
    } else {
        (complete_basis(&u), complete_basis(&v), r, c)
    };
    let mut sm = vec![0.0; sr * sc];
    for (i, sv) in s.iter().enumerate() {
        sm[i + i * sr] = *sv;
    }
    Ok(vec![val(&uu), real_val(sr, sc, sm), val(&vv)])
}

fn rank_tol(m: &CM, s: &[f64]) -> f64 {
    m.m.max(m.n) as f64 * s.first().copied().unwrap_or(0.0) * f64::EPSILON
}

fn null(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "null")?;
    let (m, _) = matrix_of(&a[0], "null")?;
    let (_, s, v) = svd_thin(&m);
    let r = s.iter().filter(|x| **x > rank_tol(&m, &s)).count();
    let vf = complete_basis(&v);
    let mut out = CM::zeros(m.n, m.n - r);
    for j in r..m.n {
        for i in 0..m.n {
            out.set(i, j - r, vf.at(i, j));
        }
    }
    one(val(&out))
}

fn orth(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "orth")?;
    let (m, _) = matrix_of(&a[0], "orth")?;
    let (u, s, _) = svd_thin(&m);
    let r = s.iter().filter(|x| **x > rank_tol(&m, &s)).count();
    let mut out = CM::zeros(m.m, r);
    for j in 0..r {
        for i in 0..m.m {
            out.set(i, j, u.at(i, j));
        }
    }
    one(val(&out))
}

fn pinv(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "pinv")?;
    let (m, _) = matrix_of(&a[0], "pinv")?;
    let tol = match a.get(1) {
        Some(t) => Some(arg_f64(t, "pinv")?),
        None => None,
    };
    let x = indy_value::cmat::solve_minnorm(&m, &CM::eye(m.m), tol);
    one(val(&x))
}

fn norm2_case(a: &[Value]) -> bool {
    let Some(Value::Num(x)) = a.first() else {
        return false;
    };
    let matrix = x.dims.len() == 2 && x.dims[0] > 1 && x.dims[1] > 1;
    let p2 = match a.get(1) {
        None => true,
        Some(v) => v.scalar_f64() == Some(2.0),
    };
    matrix && p2 && x.class == NumClass::Double
}

fn norm2(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let (m, _) = matrix_of(&a[0], "norm")?;
    if m.d.iter().any(|c| !c.0.is_finite() || !c.1.is_finite()) {
        let nan = m.d.iter().any(|c| c.0.is_nan() || c.1.is_nan());
        return one(Value::num(if nan { f64::NAN } else { f64::INFINITY }));
    }
    let (_, s, _) = svd_thin(&m);
    one(Value::num(s.first().copied().unwrap_or(0.0)))
}

fn mat_norm(m: &CM, p: &str) -> f64 {
    match p {
        "1" => m.norm1(),
        "inf" => (0..m.m)
            .map(|i| (0..m.n).map(|j| c_abs(m.at(i, j))).sum::<f64>())
            .fold(0.0, f64::max),
        "fro" => m.fro(),
        _ => svd_thin(m).1.first().copied().unwrap_or(0.0),
    }
}

fn cond(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "cond")?;
    let (m, _) = matrix_of(&a[0], "cond")?;
    let p = match a.get(1) {
        None => "2".to_string(),
        Some(v) => match v.to_str() {
            Some(s) => s.to_ascii_lowercase(),
            None => {
                let x = arg_f64(v, "cond")?;
                if x == 2.0 {
                    "2".into()
                } else if x == 1.0 {
                    "1".into()
                } else if x == f64::INFINITY {
                    "inf".into()
                } else {
                    return Err(err(
                        "MATLAB:cond:pNorm",
                        "cond: P must be 1, 2, Inf or 'fro'.",
                    ));
                }
            }
        },
    };
    if m.d.iter().any(|c| !c.0.is_finite() || !c.1.is_finite()) {
        return Err(err(
            "MATLAB:cond:infnan",
            "cond: input matrix must not contain NaN or Inf.",
        ));
    }
    if m.m == 0 || m.n == 0 {
        return one(Value::num(0.0));
    }
    if p == "2" {
        let (_, s, _) = svd_thin(&m);
        let (hi, lo) = (s[0], s[s.len() - 1]);
        return one(Value::num(if lo == 0.0 { f64::INFINITY } else { hi / lo }));
    }
    if m.m != m.n {
        return Err(err(
            "MATLAB:cond:square",
            "cond: A must be a square matrix for this norm.",
        ));
    }
    let (inv, sing) = solve_square(&m, &CM::eye(m.n));
    if sing && cm_det(&m) == (0.0, 0.0) {
        return one(Value::num(f64::INFINITY));
    }
    one(Value::num(mat_norm(&m, &p) * mat_norm(&inv, &p)))
}

fn chol(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "chol")?;
    let (m, class) = matrix_of(&a[0], "chol")?;
    if m.m != m.n {
        return Err(err("MATLAB:square", "chol: matrix must be square."));
    }
    let mut lower = false;
    for v in &a[1..] {
        match arg_str(v, "chol")?.to_ascii_lowercase().as_str() {
            "lower" => lower = true,
            "upper" => lower = false,
            "vector" | "matrix" => {}
            o => {
                return Err(err(
                    "MATLAB:chol:unknownOption",
                    format!("chol: unknown option '{}'.", o),
                ));
            }
        }
    }
    let src = if lower { m.ct() } else { m };
    let (r, p) = cm_chol(&src);
    if p != 0 && nargout < 2 {
        return Err(err("MATLAB:posdef", "Matrix must be positive definite."));
    }
    let _ = it;
    let out = if lower { r.ct() } else { r };
    let mut num = out.to_num();
    if class == NumClass::Single {
        num = num.with_class(NumClass::Single);
    }
    let mut res = vec![Value::Num(num)];
    if nargout > 1 {
        res.push(Value::num(p as f64));
    }
    Ok(res)
}

fn matfun(a: &[Value], fname: &str) -> R<Vec<Value>> {
    need(a, 1, fname)?;
    let (m, _) = matrix_of(&a[0], fname)?;
    if m.m != m.n {
        return Err(err(
            "MATLAB:square",
            format!("{}: matrix must be square.", fname),
        ));
    }
    let n = m.n;
    let is_diag = (0..n).all(|j| (0..n).all(|i| i == j || m.at(i, j) == (0.0, 0.0)));
    let f = |z: (f64, f64)| {
        if fname == "sqrtm" {
            c_sqrt(z)
        } else {
            c_log(z)
        }
    };
    if is_diag {
        let mut r = CM::zeros(n, n);
        for i in 0..n {
            r.set(i, i, f(m.at(i, i)));
        }
        return one(val(&r));
    }
    let res = if fname == "sqrtm" {
        cm_sqrtm(&m)
    } else {
        cm_logm(&m)
    };
    match res {
        Some(r) => one(val(&r)),
        None => Err(err(
            "MATLAB:unsupported",
            format!(
                "{}: matrix is singular or has eigenvalues on the negative real axis, which this implementation does not handle.",
                fname
            ),
        )),
    }
}

// ---------------------------------------------------------------------------
// bitcmp
// ---------------------------------------------------------------------------

fn bitcmp(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "bitcmp")?;
    let x = arg_num(&a[0], "bitcmp")?;
    let mut class = x.class;
    if let Some(t) = a.get(1) {
        let name = match t.to_str() {
            Some(s) => s,
            None => {
                return Err(err(
                    "MATLAB:bitcmp:unknownTypeString",
                    "ASSUMEDTYPE must be an integer type name.",
                ));
            }
        };
        match NumClass::from_name(&name) {
            Some(c) if c.is_int() => class = c,
            _ => {
                return Err(err(
                    "MATLAB:bitcmp:unknownTypeString",
                    "ASSUMEDTYPE must be an integer type name.",
                ));
            }
        }
    }
    let out: Vec<f64> =
        x.re.iter()
            .map(|&v| {
                if class.is_int() {
                    let (lo, hi) = class.int_range();
                    if lo < 0.0 { -v - 1.0 } else { hi - v }
                } else if class == NumClass::Single {
                    16_777_215.0 - v
                } else {
                    9_007_199_254_740_991.0 - v
                }
            })
            .collect();
    if !class.is_int() && x.re.iter().any(|v| *v < 0.0 || v.fract() != 0.0) {
        return Err(err(
            "MATLAB:bitcmp:nonIntegerInput",
            "bitcmp: inputs must be nonnegative integers.",
        ));
    }
    let keep = if class.is_int() { class } else { x.class };
    one(Value::Num(Num::real(x.dims, out).with_class(keep)))
}

// ---------------------------------------------------------------------------
// periodogram (basic): one-sided / two-sided PSD or power
// ---------------------------------------------------------------------------

fn periodogram(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "periodogram")?;
    let x = arg_num(&a[0], "periodogram")?;
    if x.dims.len() != 2 {
        return Err(err(
            "MATLAB:periodogram:invalidInput",
            "periodogram: X must be a vector or matrix.",
        ));
    }
    let vector = x.dims[0] == 1 || x.dims[1] == 1;
    let (n, cols) = if vector {
        (x.re.len(), 1)
    } else {
        (x.dims[0], x.dims[1])
    };
    let complex = x.im.is_some();
    let mut pos: Vec<&Value> = Vec::new();
    let mut sides: Option<String> = None;
    let mut power = false;
    for v in &a[1..] {
        if v.is_text() {
            match arg_str(v, "periodogram")?.to_ascii_lowercase().as_str() {
                "onesided" => sides = Some("one".into()),
                "twosided" => sides = Some("two".into()),
                "centered" => sides = Some("centered".into()),
                "power" => power = true,
                "psd" => power = false,
                o => {
                    return Err(err(
                        "MATLAB:periodogram:unknownOption",
                        format!("periodogram: unknown option '{}'.", o),
                    ));
                }
            }
        } else {
            pos.push(v);
        }
    }
    if pos.len() > 3 {
        return Err(err(
            "MATLAB:TooManyInputs",
            "Too many input arguments (function 'periodogram').",
        ));
    }
    let win: Vec<f64> = match pos.first() {
        Some(w) if !w.is_empty() => {
            let w = arg_num(w, "periodogram")?.re;
            if w.len() != n {
                return Err(err(
                    "MATLAB:periodogram:windowLength",
                    "periodogram: window length must match the signal length.",
                ));
            }
            w
        }
        _ => vec![1.0; n],
    };
    let nfft = match pos.get(1) {
        Some(v) if !v.is_empty() => arg_f64(v, "periodogram")? as usize,
        _ => {
            let mut p = 1usize;
            while p < n {
                p <<= 1;
            }
            p.max(256)
        }
    };
    let fs = match pos.get(2) {
        Some(v) if !v.is_empty() => Some(arg_f64(v, "periodogram")?),
        _ => None,
    };
    let sides = sides.unwrap_or_else(|| if complex { "two".into() } else { "one".into() });
    if sides == "one" && complex {
        return Err(err(
            "MATLAB:periodogram:complexOneSided",
            "periodogram: a complex signal cannot have a one-sided spectrum.",
        ));
    }
    let w2: f64 = win.iter().map(|w| w * w).sum();
    let wsum: f64 = win.iter().sum();
    let scale_f = fs.unwrap_or(2.0 * std::f64::consts::PI);
    let norm = if power { wsum * wsum } else { w2 * scale_f };
    let nrows = match sides.as_str() {
        "one" => nfft / 2 + 1,
        _ => nfft,
    };
    let mut pxx = vec![0.0; nrows * cols];
    for c in 0..cols {
        let sig: Vec<(f64, f64)> = (0..nfft)
            .map(|i| {
                if i < n {
                    let k = c * n + i;
                    (x.re[k] * win[i], x.im_at(k) * win[i])
                } else {
                    (0.0, 0.0)
                }
            })
            .collect();
        let spec = super::fft::dft(&sig, false);
        let mut full: Vec<f64> = spec
            .iter()
            .map(|z| (z.0 * z.0 + z.1 * z.1) / norm)
            .collect();
        if sides == "one" {
            let nyq = if nfft % 2 == 0 { nfft / 2 } else { usize::MAX };
            for (k, v) in full.iter_mut().enumerate().take(nrows) {
                if k != 0 && k != nyq {
                    *v *= 2.0;
                }
            }
            full.truncate(nrows);
        } else if sides == "centered" {
            let h = nfft.div_ceil(2);
            full.rotate_left(h);
        }
        pxx[c * nrows..(c + 1) * nrows].copy_from_slice(&full);
    }
    let df = scale_f / nfft as f64;
    let freqs: Vec<f64> = match sides.as_str() {
        "one" => (0..nrows).map(|k| k as f64 * df).collect(),
        "centered" => {
            let h = nfft / 2;
            let start = if nfft % 2 == 0 {
                -(h as isize)
            } else {
                -(((nfft - 1) / 2) as isize)
            };
            (0..nfft)
                .map(|k| (start + k as isize) as f64 * df)
                .collect()
        }
        _ => (0..nfft).map(|k| k as f64 * df).collect(),
    };
    if nargout == 0 {
        return Err(err(
            "MATLAB:unsupported",
            "periodogram: plotting without output arguments is not supported; request outputs.",
        ));
    }
    let mut out = vec![Value::matrix(vec![nrows, cols], pxx)];
    if nargout > 1 {
        out.push(Value::col(freqs));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// trace, kron, bitshift
// ---------------------------------------------------------------------------

fn trace(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "trace")?;
    let x = arg_num(&a[0], "trace")?;
    if x.dims.len() != 2 {
        return Err(err("MATLAB:trace:square", "Matrix must be 2-D."));
    }
    let r = x.dims[0];
    let k = r.min(x.dims[1]);
    let re: f64 = (0..k).map(|i| x.re[i + i * r]).sum();
    let im: f64 = (0..k).map(|i| x.im_at(i + i * r)).sum();
    one(Value::Num(
        Num::complex(vec![1, 1], vec![re], vec![im]).with_class(x.class),
    ))
}

fn kron(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "kron")?;
    let mut acc = arg_num(&a[0], "kron")?;
    for v in &a[1..] {
        let y = arg_num(v, "kron")?;
        if acc.dims.len() != 2 || y.dims.len() != 2 {
            return Err(err(
                "MATLAB:unsupported",
                "kron: only 2-D inputs are supported.",
            ));
        }
        let (m, n) = (acc.dims[0], acc.dims[1]);
        let (p, q) = (y.dims[0], y.dims[1]);
        let (r, c) = (m * p, n * q);
        let cplx = acc.im.is_some() || y.im.is_some();
        let mut re = vec![0.0; r * c];
        let mut im = vec![0.0; r * c];
        for i in 0..m {
            for j in 0..n {
                let (xr, xi) = (acc.re[i + j * m], acc.im_at(i + j * m));
                for k in 0..p {
                    for l in 0..q {
                        let (yr, yi) = (y.re[k + l * p], y.im_at(k + l * p));
                        let o = (i * p + k) + (j * q + l) * r;
                        re[o] = xr * yr - xi * yi;
                        im[o] = xr * yi + xi * yr;
                    }
                }
            }
        }
        let class = indy_value::ops::result_class(acc.class, y.class)?;
        acc = if cplx {
            Num::complex(vec![r, c], re, im)
        } else {
            Num::real(vec![r, c], re)
        }
        .with_class(class);
    }
    one(Value::Num(acc))
}

fn bitshift(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "bitshift")?;
    let x = arg_num(&a[0], "bitshift")?;
    let k = arg_num(&a[1], "bitshift")?;
    let width: u32 = match x.class {
        NumClass::Int8 | NumClass::UInt8 => 8,
        NumClass::Int16 | NumClass::UInt16 => 16,
        NumClass::Int32 | NumClass::UInt32 => 32,
        NumClass::Int64 | NumClass::UInt64 => 64,
        NumClass::Single => 24,
        _ => 53,
    };
    let signed = matches!(
        x.class,
        NumClass::Int8 | NumClass::Int16 | NumClass::Int32 | NumClass::Int64
    );
    let nbits = match a.get(2) {
        Some(v) => (arg_f64(v, "bitshift")? as u32).min(width),
        None => width,
    };
    let (dims, out) = indy_value::ops::zip_bcast(&x.dims, &x.re, &k.dims, &k.re, |v, s| {
        if v.is_nan() || s.is_nan() {
            return 0.0;
        }
        let bits = if signed {
            // two's complement bit pattern within `width`
            (v as i128) & ((1i128 << width) - 1)
        } else {
            v as i128
        };
        let sh = s as i32;
        let shifted = if sh >= 0 {
            bits.checked_shl(sh as u32).unwrap_or(0)
        } else {
            bits >> ((-sh) as u32).min(127)
        };
        let masked = shifted & ((1i128 << nbits) - 1);
        let r = if signed && masked >= (1i128 << (width - 1)) && nbits == width {
            masked - (1i128 << width)
        } else {
            masked
        };
        r as f64
    })?;
    one(Value::Num(Num::real(dims, out).with_class(x.class)))
}
