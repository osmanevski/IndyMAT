//! `fminunc`: quasi-Newton BFGS with a strong-Wolfe line search
//! (MATLAB's default 'quasi-newton' algorithm). The line search is a
//! bracketing/zoom search with safeguarded quadratic interpolation, so
//! iterates and counts differ from MATLAB's cubic-interpolation search.

use indy_value::Value;

use super::super::{at_most, err, need};
use super::la::inverse;
use super::options::{Disp, Opts};
use super::{call_n, cfmt, make_struct, need_function, norm_inf, norm2, real_data, take_n};
use crate::{Interp, R};

struct Obj<'a> {
    f: &'a Value,
    shape: Vec<usize>,
    user_grad: bool,
    central: bool,
    typx: Vec<f64>,
    fdstep: Option<Vec<f64>>,
    count: usize,
}

impl Obj<'_> {
    fn call(&mut self, it: &mut Interp, x: &[f64], want_grad: bool) -> R<(f64, Option<Vec<f64>>)> {
        let nout = if want_grad && self.user_grad { 2 } else { 1 };
        let xv = Value::matrix(self.shape.clone(), x.to_vec());
        let r = call_n(it, self.f, vec![xv], nout, "fminunc")?;
        self.count += 1;
        let fv = real_data(&r[0], "fminunc", "objective values")?;
        if fv.len() != 1 {
            return Err(err(
                "optimlib:fminunc:NonScalarObj",
                "fminunc: the objective function must return a scalar value.",
            ));
        }
        let g = if nout == 2 {
            let g = real_data(&r[1], "fminunc", "gradient values")?;
            if g.len() != x.len() {
                return Err(err(
                    "optimlib:fminunc:GradientSize",
                    format!(
                        "fminunc: user-defined gradient must have {} elements.",
                        x.len()
                    ),
                ));
            }
            Some(g)
        } else {
            None
        };
        Ok((fv[0], g))
    }

    fn value(&mut self, it: &mut Interp, x: &[f64]) -> R<f64> {
        Ok(self.call(it, x, false)?.0)
    }

    /// Objective and gradient at `x` (`f0` = known value at x, if any).
    fn both(&mut self, it: &mut Interp, x: &[f64], f0: Option<f64>) -> R<(f64, Vec<f64>)> {
        if self.user_grad {
            let (f, g) = self.call(it, x, true)?;
            return Ok((f, g.unwrap_or_default()));
        }
        let f = match f0 {
            Some(f) => f,
            None => self.value(it, x)?,
        };
        let n = x.len();
        let base = if self.central {
            f64::EPSILON.cbrt()
        } else {
            f64::EPSILON.sqrt()
        };
        let mut g = vec![0.0; n];
        for k in 0..n {
            let typ = self.typx.get(k).copied().unwrap_or(1.0).abs();
            let rel = match &self.fdstep {
                Some(v) if v.len() == 1 => v[0],
                Some(v) => v.get(k).copied().unwrap_or(base),
                None => base,
            };
            let sgn = if x[k] < 0.0 { -1.0 } else { 1.0 };
            let mut h = rel * sgn * x[k].abs().max(typ);
            if h == 0.0 || !h.is_finite() {
                h = base;
            }
            let mut xp = x.to_vec();
            xp[k] += h;
            let hp = xp[k] - x[k];
            let fp = self.value(it, &xp)?;
            g[k] = if self.central {
                let mut xm = x.to_vec();
                xm[k] -= h;
                let hm = x[k] - xm[k];
                (fp - self.value(it, &xm)?) / (hp + hm)
            } else {
                (fp - f) / hp
            };
        }
        Ok((f, g))
    }
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(p, q)| p * q).sum()
}

/// Strong-Wolfe line search. Returns `(alpha, f, g)` or `None` on failure.
fn line_search(
    it: &mut Interp,
    obj: &mut Obj,
    x: &[f64],
    f0: f64,
    g0: &[f64],
    d: &[f64],
    alpha0: f64,
) -> R<Option<(f64, f64, Vec<f64>)>> {
    const C1: f64 = 1e-4;
    const C2: f64 = 0.9;
    let dphi0 = dot(g0, d);
    let at = |a: f64| -> Vec<f64> { x.iter().zip(d).map(|(p, q)| p + a * q).collect() };
    let mut a_prev = 0.0;
    let mut f_prev = f0;
    let mut dphi_prev = dphi0;
    let mut a = alpha0;
    // bracketing phase
    let mut bracket: Option<(f64, f64, f64, f64, f64)> = None; // lo, flo, dlo, hi, fhi
    for i in 0..20 {
        let xa = at(a);
        let fa = obj.value(it, &xa)?;
        if !fa.is_finite() || fa > f0 + C1 * a * dphi0 || (i > 0 && fa >= f_prev) {
            bracket = Some((
                a_prev,
                f_prev,
                dphi_prev,
                a,
                if fa.is_finite() { fa } else { f64::INFINITY },
            ));
            break;
        }
        let (_, ga) = obj.both(it, &xa, Some(fa))?;
        let dpa = dot(&ga, d);
        if dpa.abs() <= -C2 * dphi0 {
            return Ok(Some((a, fa, ga)));
        }
        if dpa >= 0.0 {
            bracket = Some((a, fa, dpa, a_prev, f_prev));
            break;
        }
        a_prev = a;
        f_prev = fa;
        dphi_prev = dpa;
        a *= 2.0;
    }
    let Some((mut lo, mut flo, mut dlo, mut hi, mut fhi)) = bracket else {
        // no bracket after expansion: accept the last sufficient-decrease point
        if a_prev > 0.0 {
            let xa = at(a_prev);
            let (fa, ga) = obj.both(it, &xa, Some(f_prev))?;
            return Ok(Some((a_prev, fa, ga)));
        }
        return Ok(None);
    };
    // zoom phase
    for _ in 0..30 {
        // quadratic interpolation through (lo, flo, dlo) and (hi, fhi)
        let width = hi - lo;
        let mut aj = f64::NAN;
        let denom = 2.0 * (fhi - flo - dlo * width);
        if fhi.is_finite() && denom != 0.0 {
            aj = lo - dlo * width * width / denom;
        }
        let (amin, amax) = if lo < hi { (lo, hi) } else { (hi, lo) };
        let margin = 0.1 * (amax - amin);
        if !aj.is_finite() || aj < amin + margin || aj > amax - margin {
            aj = 0.5 * (lo + hi);
        }
        if (amax - amin) <= f64::EPSILON * amax.abs().max(1e-300) {
            break;
        }
        let xa = at(aj);
        let fa = obj.value(it, &xa)?;
        if !fa.is_finite() || fa > f0 + C1 * aj * dphi0 || fa >= flo {
            hi = aj;
            fhi = if fa.is_finite() { fa } else { f64::INFINITY };
        } else {
            let (_, ga) = obj.both(it, &xa, Some(fa))?;
            let dpa = dot(&ga, d);
            if dpa.abs() <= -C2 * dphi0 {
                return Ok(Some((aj, fa, ga)));
            }
            if dpa * (hi - lo) >= 0.0 {
                hi = lo;
                fhi = flo;
            }
            lo = aj;
            flo = fa;
            dlo = dpa;
        }
    }
    // best point with sufficient decrease, if any
    if lo > 0.0 && flo < f0 {
        let xa = at(lo);
        let (fa, ga) = obj.both(it, &xa, Some(flo))?;
        return Ok(Some((lo, fa, ga)));
    }
    Ok(None)
}

/// `[x, fval, exitflag, output, grad, hessian] = fminunc(fun, x0[, options])`
pub(super) fn fminunc(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "fminunc")?;
    at_most(&a, 3, "fminunc")?;
    if matches!(a[0], Value::Struct(_)) {
        return Err(err(
            "optimlib:fminunc:ProblemStruct",
            "fminunc: the problem-structure syntax is not supported by this engine.",
        ));
    }
    need_function(&a[0], "fminunc")?;
    let x0 = real_data(&a[1], "fminunc", "initial points X0")?;
    if x0.is_empty() {
        return Err(err(
            "optimlib:fminunc:EmptyX0",
            "fminunc: X0 must not be empty.",
        ));
    }
    let n = x0.len();
    let o = Opts::new(a.get(2), "fminunc")?;
    o.reject(&["OutputFcn"])?;
    o.reject(&["PlotFcn", "PlotFcns"])?;
    o.reject(&["CheckGradients", "DerivativeCheck"])?;
    o.reject(&["UseParallel"])?;
    o.reject(&["HessianFcn", "Hessian", "HessFcn"])?;
    match o.text(&["Algorithm"])?.as_deref() {
        None | Some("quasi-newton") => {}
        Some(other) => {
            return Err(err(
                "optimlib:fminunc:UnsupportedAlgorithm",
                format!(
                    "fminunc: Algorithm '{}' is not supported by this engine (only 'quasi-newton').",
                    other
                ),
            ));
        }
    }
    match o.text(&["HessUpdate"])?.as_deref() {
        None | Some("bfgs") => {}
        Some(other) => {
            return Err(err(
                "optimlib:fminunc:UnsupportedHessUpdate",
                format!(
                    "fminunc: HessUpdate '{}' is not supported by this engine (only 'bfgs').",
                    other
                ),
            ));
        }
    }
    let central = match o.text(&["FiniteDifferenceType", "FinDiffType"])?.as_deref() {
        None | Some("forward") => false,
        Some("central") => true,
        Some(other) => {
            return Err(err(
                "optimlib:options:FiniteDifferenceType",
                format!(
                    "Invalid value '{}' for option 'FiniteDifferenceType'.",
                    other
                ),
            ));
        }
    };
    let to_names = ["OptimalityTolerance", "TolFun"];
    let tx_names = ["StepTolerance", "TolX"];
    let mi_names = ["MaxIterations", "MaxIter"];
    let mf_names = ["MaxFunctionEvaluations", "MaxFunEvals"];
    let tolopt = o.f64(&to_names, 1e-6)?;
    let tolx = o.f64(&tx_names, 1e-6)?;
    let maxiter = o.count(&mi_names, 400.0)?;
    let maxfev = o.count(&mf_names, 100.0 * n as f64)?;
    let objlimit = o.f64(&["ObjectiveLimit"], -1e20)?;
    let disp = o.display(Disp::Final)?;
    let typx = o.vector(&["TypicalX"])?.unwrap_or_else(|| vec![1.0; n]);
    let typx = if typx.len() == 1 {
        vec![typx[0]; n]
    } else {
        typx
    };
    let mut obj = Obj {
        f: &a[0],
        shape: a[1].dims(),
        user_grad: o.flag(&["SpecifyObjectiveGradient", "GradObj"])?,
        central,
        typx,
        fdstep: o.vector(&["FiniteDifferenceStepSize", "FinDiffRelStep"])?,
        count: 0,
    };

    let mut x = x0;
    let (mut f, mut g) = obj.both(it, &x, None)?;
    if !f.is_finite() || g.iter().any(|v| !v.is_finite()) {
        return Err(err(
            "optimlib:fminunc:UsrObjUndefAtX0",
            "Objective function is undefined at initial point. Fminunc cannot continue.",
        ));
    }
    let g0n = norm_inf(&g);
    let gtol = tolopt * g0n.max(1.0);
    let mut h = identity(n);
    let mut first_update = true;
    let mut iters = 0usize;
    let mut last_step = 0.0;
    let mut last_alpha = 0.0;
    if disp == Disp::Iter {
        it.print(&format!(
            "                                                        First-order \n Iteration  Func-count       f(x)        Step-size       optimality\n{}\n",
            cfmt("%6d", 0.0)
                + &cfmt(" %11d", obj.count as f64)
                + &cfmt(" %16.6g", f)
                + &" ".repeat(14)
                + &cfmt(" %12.3g", g0n)
        ));
    }
    let flag: i32 = loop {
        if norm_inf(&g) <= gtol {
            break 1;
        }
        if f < objlimit {
            break -3;
        }
        if iters as f64 >= maxiter || obj.count as f64 >= maxfev {
            break 0;
        }
        let mut d: Vec<f64> = (0..n)
            .map(|i| -(0..n).map(|j| h[j * n + i] * g[j]).sum::<f64>())
            .collect();
        if !(dot(&g, &d) < 0.0) {
            h = identity(n);
            first_update = true;
            d = g.iter().map(|v| -v).collect();
        }
        let alpha0 = if iters == 0 {
            (1.0 / norm_inf(&g)).min(1.0)
        } else {
            1.0
        };
        let ls = line_search(it, &mut obj, &x, f, &g, &d, alpha0)?;
        iters += 1;
        let Some((alpha, fnew, gnew)) = ls else {
            break 5;
        };
        let s: Vec<f64> = d.iter().map(|v| alpha * v).collect();
        let y: Vec<f64> = gnew.iter().zip(&g).map(|(p, q)| p - q).collect();
        for i in 0..n {
            x[i] += s[i];
        }
        f = fnew;
        g = gnew;
        last_step = norm2(&s);
        last_alpha = alpha;
        if disp == Disp::Iter {
            it.print(&format!(
                "{}\n",
                cfmt("%6d", iters as f64)
                    + &cfmt(" %11d", obj.count as f64)
                    + &cfmt(" %16.6g", f)
                    + &cfmt(" %13.6g", alpha)
                    + &cfmt(" %12.3g", norm_inf(&g))
            ));
        }
        if norm_inf(&g) <= gtol {
            break 1;
        }
        if last_step < tolx * (1.0 + norm2(&x)) {
            break 2;
        }
        let sy = dot(&s, &y);
        if sy > f64::EPSILON.sqrt() * norm2(&s) * norm2(&y) {
            if first_update {
                let yy = dot(&y, &y);
                h = identity(n);
                for i in 0..n {
                    h[i * n + i] = sy / yy;
                }
                first_update = false;
            }
            // H+ = H + (sy + y'Hy) ss'/sy^2 - (Hy s' + s y'H)/sy
            let hy: Vec<f64> = (0..n)
                .map(|i| (0..n).map(|j| h[j * n + i] * y[j]).sum())
                .collect();
            let yhy = dot(&y, &hy);
            let c1 = (sy + yhy) / (sy * sy);
            for j in 0..n {
                for i in 0..n {
                    h[j * n + i] += c1 * s[i] * s[j] - (hy[i] * s[j] + s[i] * hy[j]) / sy;
                }
            }
        }
    };
    let dflt = |names: &[&str], v: f64| {
        if o.is_default(names, v) {
            "default value of the "
        } else {
            "value of the "
        }
    };
    let msg = match flag {
        1 => format!(
            "Local minimum found.\n\nOptimization completed because the size of the gradient is less than\nthe {}optimality tolerance.",
            dflt(&to_names, 1e-6)
        ),
        2 => format!(
            "Local minimum possible.\n\nfminunc stopped because the size of the current step is less than\nthe {}step size tolerance.",
            dflt(&tx_names, 1e-6)
        ),
        5 => "Local minimum possible.\n\nfminunc stopped because it cannot decrease the objective function\nalong the current search direction.".to_string(),
        -3 => format!(
            "Problem appears unbounded.\n\nfminunc stopped because the objective function value is less than\nthe {}objective function limit.",
            dflt(&["ObjectiveLimit"], -1e20)
        ),
        _ if iters as f64 >= maxiter => format!(
            "Solver stopped prematurely.\n\nfminunc stopped because it exceeded the iteration limit,\noptions.MaxIterations = {}{}.",
            cfmt("%g", maxiter),
            if o.is_default(&mi_names, 400.0) { " (the default value)" } else { "" }
        ),
        _ => format!(
            "Solver stopped prematurely.\n\nfminunc stopped because it exceeded the function evaluation limit,\noptions.MaxFunctionEvaluations = {}{}.",
            cfmt("%g", maxfev),
            if o.is_default(&mf_names, 100.0 * n as f64) { " (the default value)" } else { "" }
        ),
    };
    let show = match disp {
        Disp::Off => false,
        Disp::Notify => flag <= 0,
        Disp::Final | Disp::Iter => true,
    };
    if show {
        let tail = if flag == 0 {
            "\n\n"
        } else {
            "\n\n<stopping criteria details>\n"
        };
        it.print(&format!("\n{}{}", msg, tail));
    }
    let out = make_struct(vec![
        ("iterations", Value::num(iters as f64)),
        ("funcCount", Value::num(obj.count as f64)),
        ("stepsize", Value::num(last_step)),
        ("lssteplength", Value::num(last_alpha)),
        ("firstorderopt", Value::num(norm_inf(&g))),
        ("algorithm", Value::str("quasi-newton")),
        ("message", Value::str(&msg)),
    ]);
    let hess = if nargout >= 6 {
        let b = inverse(&h, n).unwrap_or_else(|| vec![f64::NAN; n * n]);
        // symmetrise the round-off
        let mut s = vec![0.0; n * n];
        for i in 0..n {
            for j in 0..n {
                s[j * n + i] = 0.5 * (b[j * n + i] + b[i * n + j]);
            }
        }
        Value::matrix(vec![n, n], s)
    } else {
        Value::empty()
    };
    take_n(
        vec![
            Value::matrix(obj.shape.clone(), x),
            Value::num(f),
            Value::num(flag as f64),
            out,
            Value::col(g),
            hess,
        ],
        nargout,
    )
}

fn identity(n: usize) -> Vec<f64> {
    let mut h = vec![0.0; n * n];
    for i in 0..n {
        h[i * n + i] = 1.0;
    }
    h
}
