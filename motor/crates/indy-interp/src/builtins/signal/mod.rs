//! Signal Processing Toolbox functions (MATLAB semantics), native Rust.
//!
//! Filter design (`fir1`, `fir2`, `firls`, `butter`, `cheby1`, `cheby2`,
//! `ellip`, order estimation), analysis (`freqz`, `grpdelay`, `impz`,
//! zero/pole/gain and second-order-section conversions), filtering
//! (`filtfilt`, `sosfilt`, `fftfilt`, `medfilt1`) and spectral/resampling
//! helpers. Everything here is original code written from the MATLAB
//! documentation; Octave's `signal` package was used only as a behaviour
//! oracle for tests.

use indy_value::{Error, Num, Value};

use super::{arg_num, arg_str, err};
use crate::{Interp, R};

mod analysis;
mod design;
mod elliptic;
mod filt;
mod misc;
mod plots;
mod spectral;
mod windows;

pub type C = (f64, f64);

pub fn register(it: &mut Interp) {
    design::register(it);
    analysis::register(it);
    filt::register(it);
    spectral::register(it);
    misc::register(it);
}

// ---------------------------------------------------------------------------
// Complex scalar helpers
// ---------------------------------------------------------------------------

pub const CZ: C = (0.0, 0.0);
pub const C1: C = (1.0, 0.0);

pub fn cadd(a: C, b: C) -> C {
    (a.0 + b.0, a.1 + b.1)
}
pub fn csub(a: C, b: C) -> C {
    (a.0 - b.0, a.1 - b.1)
}
pub fn cmul(a: C, b: C) -> C {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}
pub fn cdiv(a: C, b: C) -> C {
    indy_value::ops::c_div(a, b)
}
pub fn cscale(a: C, s: f64) -> C {
    (a.0 * s, a.1 * s)
}
pub fn cconj(a: C) -> C {
    (a.0, -a.1)
}
pub fn cabs(a: C) -> f64 {
    a.0.hypot(a.1)
}
pub fn csqrt(a: C) -> C {
    indy_value::ops::c_sqrt(a)
}
pub fn cexp(a: C) -> C {
    let e = a.0.exp();
    (e * a.1.cos(), e * a.1.sin())
}
pub fn carg(a: C) -> f64 {
    a.1.atan2(a.0)
}

// ---------------------------------------------------------------------------
// Polynomials (coefficients highest power first)
// ---------------------------------------------------------------------------

pub fn pmul(a: &[C], b: &[C]) -> Vec<C> {
    if a.is_empty() || b.is_empty() {
        return vec![];
    }
    let mut w = vec![CZ; a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        for (j, y) in b.iter().enumerate() {
            w[i + j] = cadd(w[i + j], cmul(*x, *y));
        }
    }
    w
}

/// Coefficients of `prod(x - r)`.
pub fn poly_of_roots(r: &[C]) -> Vec<C> {
    let mut p = vec![C1];
    for &z in r {
        let mut q = vec![CZ; p.len() + 1];
        for (i, c) in p.iter().enumerate() {
            q[i] = cadd(q[i], *c);
            q[i + 1] = csub(q[i + 1], cmul(*c, z));
        }
        p = q;
    }
    p
}

pub fn pval(p: &[C], x: C) -> C {
    let mut acc = CZ;
    for &c in p {
        acc = cadd(cmul(acc, x), c);
    }
    acc
}

/// Real parts of a coefficient vector whose roots are conjugate-closed
/// (drops the rounding-level imaginary residue).
pub fn real_part(p: &[C]) -> Vec<f64> {
    p.iter().map(|c| c.0).collect()
}

pub fn prod(v: &[C]) -> C {
    v.iter().fold(C1, |a, b| cmul(a, *b))
}

// ---------------------------------------------------------------------------
// Value conversion
// ---------------------------------------------------------------------------

pub fn cvec(v: &Value, f: &str) -> R<Vec<C>> {
    let n = arg_num(v, f)?;
    Ok((0..n.re.len()).map(|i| (n.re[i], n.im_at(i))).collect())
}

pub fn rvec(v: &Value, f: &str) -> R<Vec<f64>> {
    let n = arg_num(v, f)?;
    if n.is_complex() {
        return Err(err(
            "MATLAB:invalidType",
            format!("{}: input must be real.", f),
        ));
    }
    Ok(n.re)
}

pub fn from_c(dims: Vec<usize>, v: &[C]) -> Value {
    let re = v.iter().map(|c| c.0).collect();
    let im = v.iter().map(|c| c.1).collect();
    Value::Num(Num::complex(dims, re, im))
}
pub fn crow(v: &[C]) -> Value {
    from_c(vec![1, v.len()], v)
}
pub fn ccol(v: &[C]) -> Value {
    from_c(vec![v.len(), 1], v)
}
pub fn rrow(v: Vec<f64>) -> Value {
    Value::row(v)
}
pub fn rcol(v: Vec<f64>) -> Value {
    Value::col(v)
}
pub fn rmat(rows: usize, cols: usize, v: Vec<f64>) -> Value {
    Value::matrix(vec![rows, cols], v)
}

/// A real polynomial argument: vector, any orientation.
pub fn is_vec(v: &Value) -> bool {
    let d = v.dims_ref();
    d.len() == 2 && (d[0] == 1 || d[1] == 1)
}

/// Matrix columns (`dims = [r, c]`); a row vector counts as one column.
/// Returns `(columns, was_row)`.
pub fn columns(v: &Value, f: &str) -> R<(Vec<Vec<C>>, bool, usize)> {
    let n = arg_num(v, f)?;
    if n.dims.len() > 2 {
        return Err(err(
            "MATLAB:invalidType",
            format!("{}: N-d arrays are not supported.", f),
        ));
    }
    let (r, c) = (n.dims[0], n.dims[1]);
    if r == 1 {
        let col: Vec<C> = (0..c).map(|i| (n.re[i], n.im_at(i))).collect();
        return Ok((vec![col], true, 1));
    }
    let mut out = Vec::with_capacity(c);
    for j in 0..c {
        out.push(
            (0..r)
                .map(|i| (n.re[j * r + i], n.im_at(j * r + i)))
                .collect(),
        );
    }
    Ok((out, false, r))
}

/// Inverse of [`columns`]: rebuild a value from columns.
pub fn from_columns(cols: &[Vec<C>], was_row: bool) -> Value {
    if was_row {
        return crow(cols.first().map_or(&[][..], |c| &c[..]));
    }
    let r = cols.first().map_or(0, |c| c.len());
    let flat: Vec<C> = cols.iter().flatten().copied().collect();
    from_c(vec![r, cols.len()], &flat)
}

pub fn opt_string(v: &Value) -> Option<String> {
    if v.is_text_scalar() { v.to_str() } else { None }
}

pub fn str_arg(v: &Value, f: &str) -> R<String> {
    arg_str(v, f)
}

pub fn e(id: &str, msg: impl Into<String>) -> Box<Error> {
    Error::new(id, msg)
}

pub fn nonempty(v: &Value) -> bool {
    !v.is_empty()
}

// ---------------------------------------------------------------------------
// Roots through the engine's `roots` builtin (keeps one eigen-solver)
// ---------------------------------------------------------------------------

pub fn roots_of(it: &mut Interp, p: &[C]) -> R<Vec<C>> {
    // strip leading zeros
    let first = p.iter().position(|c| *c != CZ);
    let Some(first) = first else {
        return Ok(vec![]);
    };
    let p = &p[first..];
    if p.len() <= 1 {
        return Ok(vec![]);
    }
    let r = it.call_named("roots", vec![ccol(p)], 1)?;
    cvec(&r[0], "roots")
}
