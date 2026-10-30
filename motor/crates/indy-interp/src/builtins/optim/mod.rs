//! Optimisation and multi-dimensional integration (native builtins).
//!
//! * `fsolve` — trust-region dogleg (MATLAB default) and
//!   Levenberg–Marquardt; `'trust-region'` is accepted as an alias of the
//!   dogleg method (MATLAB's subspace trust-region method for large sparse
//!   systems is not implemented separately).
//! * `lsqnonlin`, `lsqcurvefit` — Levenberg–Marquardt for every
//!   `Algorithm` value; bounds are enforced by projecting each trial point
//!   onto the box (MATLAB's default trust-region-reflective method is not
//!   implemented, so iterates and iteration counts differ).
//! * `lsqnonneg` — Lawson–Hanson active-set NNLS.
//! * `fminsearch` — Nelder–Mead as described by Lagarias et al. (1998),
//!   with MATLAB's initial simplex and stopping test.
//! * `fminunc` — quasi-Newton BFGS with a Wolfe line search.
//! * `optimoptions` — returns a plain struct (MATLAB returns an object)
//!   with a `SolverName` field and the solver's options under their
//!   current MATLAB names.
//! * `integral2`, `integral3`, `quad2d` — adaptive Gauss–Kronrod (7, 15)
//!   tensor rules (see `cubature.rs`).
//!
//! All solvers read both the `optimset` names (`TolFun`, `MaxIter`, ...)
//! and the `optimoptions` names (`FunctionTolerance`, `MaxIterations`,
//! ...), case-insensitively; an empty field means "use the default".
//! Options that are recognised but not implemented raise an error naming
//! the option instead of being silently ignored.

// Negated comparisons are deliberate: they treat NaN as "test failed".
#![allow(clippy::neg_cmp_op_on_partial_ord, clippy::too_many_arguments)]

use indy_value::{StructArray, Value};

use super::{err, reg};
use crate::{Interp, R};

mod cubature;
mod fminsearch;
mod fminunc;
mod fsolve;
mod la;
mod nnls;
mod options;

pub fn register(it: &mut Interp) {
    reg(it, "fsolve", fsolve::fsolve);
    reg(it, "lsqnonlin", fsolve::lsqnonlin);
    reg(it, "lsqcurvefit", fsolve::lsqcurvefit);

    reg(it, "lsqnonneg", nnls::lsqnonneg);
    reg(it, "optimoptions", options::optimoptions);
    reg(it, "fminsearch", fminsearch::fminsearch);
    reg(it, "fminunc", fminunc::fminunc);
    reg(it, "integral2", cubature::integral2);
    reg(it, "integral3", cubature::integral3);
    reg(it, "quad2d", cubature::quad2d);
}

// ---------------------------------------------------------------------------
// Small shared helpers
// ---------------------------------------------------------------------------

pub(super) fn make_struct(fields: Vec<(&str, Value)>) -> Value {
    let mut s = StructArray::new_scalar();
    for (k, v) in fields {
        s.set(0, k, v);
    }
    Value::Struct(s)
}

pub(super) fn take_n(v: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    Ok(v.into_iter().take(nargout.max(1)).collect())
}

/// C-style formatting of one number (`%e`, `%g`, `%f`, ...).
pub(super) fn cfmt(fmt: &str, x: f64) -> String {
    indy_value::format::printf(fmt, &[Value::num(x)]).unwrap_or_else(|_| format!("{}", x))
}

pub(super) fn norm2(v: &[f64]) -> f64 {
    // scaled to avoid overflow for large entries
    let s = v.iter().fold(0.0f64, |m, x| m.max(x.abs()));
    if s == 0.0 || !s.is_finite() {
        return s;
    }
    s * v.iter().map(|x| (x / s) * (x / s)).sum::<f64>().sqrt()
}

pub(super) fn norm_inf(v: &[f64]) -> f64 {
    v.iter().fold(
        0.0f64,
        |m, x| {
            if x.is_nan() { f64::NAN } else { m.max(x.abs()) }
        },
    )
}

pub(super) fn sumsq(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum()
}

/// Real f64 data of a numeric/logical value; complex values are rejected.
pub(super) fn real_data(v: &Value, fname: &str, what: &str) -> R<Vec<f64>> {
    match v {
        Value::Sparse(s) => real_data(&s.to_dense()?, fname, what),
        Value::Num(n) if n.im.is_some() => Err(err(
            &format!("MATLAB:{}:complexValues", fname),
            format!("{}: complex {} are not supported.", fname, what),
        )),
        Value::Num(_) | Value::Bool(_) => Ok(v.to_f64_vec().unwrap_or_default()),
        other => Err(err(
            &format!("MATLAB:{}:invalidValue", fname),
            format!(
                "{}: {} must be numeric, got a value of class {}.",
                fname,
                what,
                other.class_name()
            ),
        )),
    }
}

/// Call `f` and return its first `nout` outputs (error if it returned fewer).
pub(super) fn call_n(
    it: &mut Interp,
    f: &Value,
    args: Vec<Value>,
    nout: usize,
    fname: &str,
) -> R<Vec<Value>> {
    let r = it.call_value(f, args, nout)?;
    if r.len() < nout.max(1) {
        return Err(err(
            &format!("MATLAB:{}:noOutput", fname),
            format!(
                "{}: the function did not return {} output value(s).",
                fname,
                nout.max(1)
            ),
        ));
    }
    Ok(r)
}

/// `eps(x)` as in MATLAB: the spacing of doubles at |x|.
pub(super) fn eps_of(x: f64) -> f64 {
    let a = x.abs();
    if !a.is_finite() {
        return f64::NAN;
    }
    if a < f64::MIN_POSITIVE {
        return f64::from_bits(1); // 2^-1074
    }
    let bits = a.to_bits();
    let exp = ((bits >> 52) & 0x7ff) as i32 - 1023;
    2f64.powi(exp - 52)
}

pub(super) fn is_function(v: &Value) -> bool {
    matches!(v, Value::Func(_)) || v.is_char()
}

pub(super) fn need_function(v: &Value, fname: &str) -> R<()> {
    if is_function(v) {
        Ok(())
    } else {
        Err(err(
            &format!("MATLAB:{}:invalidFunction", fname),
            format!(
                "{}: FUN must be a function handle or function name, got a value of class {}.",
                fname,
                v.class_name()
            ),
        ))
    }
}
