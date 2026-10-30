//! `lsqnonneg`: Lawson–Hanson active-set non-negative least squares.

use indy_value::Value;

use super::super::{at_most, err, need};
use super::la::{lstsq, matvec, tmatvec};
use super::options::{Disp, Opts};
use super::{cfmt, make_struct, real_data, sumsq, take_n};
use crate::{Interp, R};

/// `[x, resnorm, residual, exitflag, output, lambda] = lsqnonneg(C, d[, options])`
pub(super) fn lsqnonneg(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "lsqnonneg")?;
    at_most(&a, 3, "lsqnonneg")?;
    let cd = a[0].dims();
    if cd.len() > 2 {
        return Err(err(
            "MATLAB:lsqnonneg:NotMatrix",
            "lsqnonneg: C must be a 2-D matrix.",
        ));
    }
    let (m, n) = (cd[0], cd[1]);
    let c = real_data(&a[0], "lsqnonneg", "C")
        .map_err(|_| err("MATLAB:lsqnonneg:ComplexCorD", "C and d must be real."))?;
    let d = real_data(&a[1], "lsqnonneg", "d")
        .map_err(|_| err("MATLAB:lsqnonneg:ComplexCorD", "C and d must be real."))?;
    if d.len() != m {
        return Err(err(
            "MATLAB:lsqnonneg:InvalidDimensions",
            "lsqnonneg: the number of rows of C must equal the length of d.",
        ));
    }
    if c.iter().chain(&d).any(|v| !v.is_finite()) {
        return Err(err(
            "MATLAB:lsqnonneg:NonFiniteInput",
            "lsqnonneg: C and d must not contain Inf or NaN values.",
        ));
    }
    let o = Opts::new(a.get(2), "lsqnonneg")?;
    let disp = o.display(Disp::Notify)?;
    // tol = 10*eps*norm(C,1)*length(C)
    let norm1 = (0..n)
        .map(|j| c[j * m..(j + 1) * m].iter().map(|v| v.abs()).sum::<f64>())
        .fold(0.0, f64::max);
    let len = if m == 0 || n == 0 { 0 } else { m.max(n) };
    let tol = o.f64(
        &["StepTolerance", "TolX"],
        10.0 * f64::EPSILON * norm1 * len as f64,
    )?;
    let itmax = 3 * n;

    let mut x = vec![0.0; n];
    let mut p = vec![false; n]; // passive set (free variables)
    let solve_p = |p: &[bool]| -> Vec<f64> {
        let cols: Vec<usize> = (0..n).filter(|&j| p[j]).collect();
        let mut sub = Vec::with_capacity(m * cols.len());
        for &j in &cols {
            sub.extend_from_slice(&c[j * m..(j + 1) * m]);
        }
        let zs = lstsq(&sub, m, cols.len(), &d);
        let mut z = vec![0.0; n];
        for (k, &j) in cols.iter().enumerate() {
            z[j] = zs[k];
        }
        z
    };
    let resid = |x: &[f64]| -> Vec<f64> {
        let cx = matvec(&c, m, n, x);
        d.iter().zip(&cx).map(|(a, b)| a - b).collect()
    };
    let mut w = tmatvec(&c, m, n, &resid(&x));
    let mut outer = 0usize;
    let mut iter = 0usize;
    let mut flag = 1;
    'outer: while (0..n).any(|j| !p[j] && w[j] > tol) {
        outer += 1;
        // most positive multiplier among the active (zero) variables
        let mut t = usize::MAX;
        for j in 0..n {
            if !p[j] && (t == usize::MAX || w[j] > w[t]) {
                t = j;
            }
        }
        if t == usize::MAX {
            break;
        }
        p[t] = true;
        let mut z = solve_p(&p);
        while (0..n).any(|j| p[j] && z[j] <= 0.0) {
            iter += 1;
            if iter > itmax {
                flag = 0;
                x = z;
                break 'outer;
            }
            let mut alpha = f64::INFINITY;
            for j in 0..n {
                if p[j] && z[j] <= 0.0 {
                    let den = x[j] - z[j];
                    if den != 0.0 {
                        alpha = alpha.min(x[j] / den);
                    }
                }
            }
            if !alpha.is_finite() {
                alpha = 0.0;
            }
            for j in 0..n {
                x[j] += alpha * (z[j] - x[j]);
                if p[j] && x[j].abs() < tol {
                    p[j] = false;
                }
            }
            z = solve_p(&p);
        }
        x = z;
        w = tmatvec(&c, m, n, &resid(&x));
    }
    let residual = resid(&x);
    let lambda = tmatvec(&c, m, n, &residual);
    let msg = if flag == 1 {
        "Optimization terminated.".to_string()
    } else {
        format!(
            "Exiting: Iteration count is exceeded, exiting LSQNONNEG.\nTry raising the tolerance (OPTIONS.TolX), currently {}.",
            cfmt("%g", tol)
        )
    };
    let show = match disp {
        Disp::Off => false,
        Disp::Notify => flag == 0,
        Disp::Final | Disp::Iter => true,
    };
    if show {
        it.print(&format!("{}\n", msg));
    }
    let out = make_struct(vec![
        ("iterations", Value::num(outer as f64)),
        ("algorithm", Value::str("active-set")),
        ("message", Value::str(&msg)),
    ]);
    take_n(
        vec![
            Value::col(x),
            Value::num(sumsq(&residual)),
            Value::col(residual),
            Value::num(flag as f64),
            out,
            Value::col(lambda),
        ],
        nargout,
    )
}
