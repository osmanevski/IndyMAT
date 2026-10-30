//! Nonlinear equations and nonlinear least squares: `fsolve` (trust-region
//! dogleg, Levenberg–Marquardt), `lsqnonlin`, `lsqcurvefit` (LM with bound
//! projection).

use indy_value::Value;

use super::super::{at_most, err, need};
use super::la::{lstsq, matvec, tmatvec};
use super::options::{Disp, Opts};
use super::{call_n, cfmt, make_struct, need_function, norm_inf, norm2, real_data, sumsq, take_n};
use crate::{Interp, R};

// ---------------------------------------------------------------------------
// Residual function with finite-difference Jacobian
// ---------------------------------------------------------------------------

pub(super) struct Residual<'a> {
    f: &'a Value,
    shape: Vec<usize>,
    /// extra trailing arguments (`xdata` for lsqcurvefit)
    extra: Vec<Value>,
    /// subtracted from the function value (`ydata` for lsqcurvefit)
    sub: Option<Vec<f64>>,
    user_jac: bool,
    central: bool,
    typx: Vec<f64>,
    fdstep: Option<Vec<f64>>,
    lb: Option<Vec<f64>>,
    ub: Option<Vec<f64>>,
    pub(super) fcount: usize,
    /// dims of the function value (for `fval`/`residual`)
    pub(super) fdims: Option<Vec<usize>>,
    fname: &'static str,
}

pub(super) struct Pt {
    pub f: Vec<f64>,
    pub j: Option<Vec<f64>>,
}

impl Residual<'_> {
    fn m(&self) -> usize {
        self.fdims
            .as_ref()
            .map_or(0, |d| d.iter().product::<usize>())
    }

    fn point(&mut self, it: &mut Interp, x: &[f64]) -> R<Pt> {
        let xv = Value::matrix(self.shape.clone(), x.to_vec());
        let mut args = vec![xv];
        args.extend(self.extra.iter().cloned());
        let nout = if self.user_jac { 2 } else { 1 };
        let r = call_n(it, self.f, args, nout, self.fname)?;
        self.fcount += 1;
        let mut f = real_data(&r[0], self.fname, "function values")?;
        let dims = r[0].dims();
        if f.is_empty() {
            return Err(err(
                &format!("optimlib:{}:EmptyFval", self.fname),
                format!("{}: FUN must return a non-empty array.", self.fname),
            ));
        }
        if let Some(s) = &self.sub {
            if s.len() != f.len() {
                return Err(err(
                    &format!("MATLAB:{}:ydataSize", self.fname),
                    format!(
                        "{}: function value and YDATA sizes are not equal ({} vs {} elements).",
                        self.fname,
                        f.len(),
                        s.len()
                    ),
                ));
            }
            for (a, b) in f.iter_mut().zip(s) {
                *a -= b;
            }
        }
        match &self.fdims {
            None => self.fdims = Some(dims),
            Some(d) if d.iter().product::<usize>() != f.len() => {
                return Err(err(
                    &format!("MATLAB:{}:sizeChanged", self.fname),
                    format!(
                        "{}: the number of function values changed between evaluations.",
                        self.fname
                    ),
                ));
            }
            _ => {}
        }
        let j = if self.user_jac {
            let n = x.len();
            let jd = real_data(&r[1], self.fname, "Jacobian values")?;
            if jd.len() != f.len() * n {
                return Err(err(
                    &format!("MATLAB:{}:JacobianSize", self.fname),
                    format!(
                        "{}: user-defined Jacobian must be {}-by-{} (number of function values by number of variables).",
                        self.fname,
                        f.len(),
                        n
                    ),
                ));
            }
            Some(jd)
        } else {
            None
        };
        Ok(Pt { f, j })
    }

    /// Jacobian at `x` (user supplied, or finite differences around `pt`).
    fn jacobian(&mut self, it: &mut Interp, x: &[f64], pt: &Pt) -> R<Vec<f64>> {
        if let Some(j) = &pt.j {
            return Ok(j.clone());
        }
        let n = x.len();
        let m = pt.f.len();
        let mut jac = vec![0.0; m * n];
        let base = if self.central {
            f64::EPSILON.cbrt()
        } else {
            f64::EPSILON.sqrt()
        };
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
            // stay inside the bounds when possible
            if let Some(ub) = &self.ub
                && x[k] + h > ub[k]
            {
                h = -h;
            }
            if let Some(lb) = &self.lb
                && x[k] + h < lb[k]
                && x[k] - h.abs() >= lb[k]
            {
                h = -h.abs();
            }
            let mut xp = x.to_vec();
            xp[k] += h;
            let hp = xp[k] - x[k];
            let fp = self.point(it, &xp)?.f;
            if self.central {
                let mut xm = x.to_vec();
                xm[k] -= h;
                let hm = x[k] - xm[k];
                let fm = self.point(it, &xm)?.f;
                for i in 0..m {
                    jac[k * m + i] = (fp[i] - fm[i]) / (hp + hm);
                }
            } else {
                for i in 0..m {
                    jac[k * m + i] = (fp[i] - pt.f[i]) / hp;
                }
            }
        }
        Ok(jac)
    }
}

// ---------------------------------------------------------------------------
// Engines
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Stop {
    Grad,
    Step,
    FunChange,
    SmallDir,
    MaxIter,
    MaxFev,
    Radius,
}

pub(super) struct Cfg {
    pub tolfun: f64,
    pub tolopt: f64,
    pub tolx: f64,
    pub maxiter: f64,
    pub maxfev: f64,
    pub disp: Disp,
    pub lambda0: f64,
    pub lb: Option<Vec<f64>>,
    pub ub: Option<Vec<f64>>,
    /// label of the residual column in the LM iteration display
    pub label: &'static str,
}

pub(super) struct Sol {
    pub x: Vec<f64>,
    pub f: Vec<f64>,
    pub j: Vec<f64>,
    pub iters: usize,
    pub stop: Stop,
    pub opt: f64,
    pub step: f64,
}

fn projected_grad(g: &[f64], x: &[f64], cfg: &Cfg) -> Vec<f64> {
    g.iter()
        .enumerate()
        .map(|(i, gi)| {
            let at_lb = cfg.lb.as_ref().is_some_and(|l| x[i] <= l[i]);
            let at_ub = cfg.ub.as_ref().is_some_and(|u| x[i] >= u[i]);
            if (at_lb && *gi > 0.0) || (at_ub && *gi < 0.0) {
                0.0
            } else {
                *gi
            }
        })
        .collect()
}

fn clip(x: &mut [f64], cfg: &Cfg) {
    for i in 0..x.len() {
        if let Some(l) = &cfg.lb
            && x[i] < l[i]
        {
            x[i] = l[i];
        }
        if let Some(u) = &cfg.ub
            && x[i] > u[i]
        {
            x[i] = u[i];
        }
    }
}

fn start(it: &mut Interp, r: &mut Residual, x0: &[f64], init: Option<Pt>) -> R<(Pt, Vec<f64>)> {
    let pt = match init {
        Some(p) => p,
        None => r.point(it, x0)?,
    };
    if pt.f.iter().any(|v| !v.is_finite()) {
        return Err(err(
            &format!("optimlib:{}:UsrObjUndefAtX0", r.fname),
            format!(
                "Objective function is returning undefined values at initial point. {} cannot continue.",
                r.fname
            ),
        ));
    }
    let j = r.jacobian(it, x0, &pt)?;
    if j.iter().any(|v| !v.is_finite()) {
        return Err(err(
            &format!("optimlib:{}:JacobianUndefAtX0", r.fname),
            format!(
                "Jacobian is returning undefined values at initial point. {} cannot continue.",
                r.fname
            ),
        ));
    }
    Ok((pt, j))
}

fn g(v: f64) -> String {
    cfmt("%g", v)
}

/// Trust-region dogleg on the sum of squares (square systems).
pub(super) fn dogleg(
    it: &mut Interp,
    r: &mut Residual,
    x0: &[f64],
    cfg: &Cfg,
    init: Option<Pt>,
) -> R<Sol> {
    let n = x0.len();
    let mut x = x0.to_vec();
    let (pt, mut jac) = start(it, r, &x, init)?;
    let mut fv = pt.f;
    let m = fv.len();
    let mut f = sumsq(&fv);
    let mut grad = tmatvec(&jac, m, n, &fv);
    let mut opt = norm_inf(&grad);
    let mut delta = 1.0f64;
    let mut iters = 0usize;
    let mut last_step = 0.0;
    if cfg.disp == Disp::Iter {
        it.print(&format!(
            "\n                                         Norm of      First-order   Trust-region\n Iteration  Func-count     f(x)          step         optimality    radius\n{:>6}{:>12}{:>16}{:>30}{:>16}\n",
            0,
            r.fcount,
            g(f),
            cfmt("%.3g", opt),
            g(delta)
        ));
    }
    let stop = loop {
        if opt <= cfg.tolopt {
            break Stop::Grad;
        }
        if iters as f64 >= cfg.maxiter {
            break Stop::MaxIter;
        }
        if r.fcount as f64 >= cfg.maxfev {
            break Stop::MaxFev;
        }
        iters += 1;
        // Gauss-Newton step and Cauchy point
        let neg_f: Vec<f64> = fv.iter().map(|v| -v).collect();
        let sgn = lstsq(&jac, m, n, &neg_f);
        let ngn = norm2(&sgn);
        let s: Vec<f64> = if ngn <= delta && ngn.is_finite() {
            sgn
        } else {
            let jg = matvec(&jac, m, n, &grad);
            let gg = sumsq(&grad);
            let jgjg = sumsq(&jg);
            let ng = gg.sqrt();
            let sc: Vec<f64> = if jgjg > 0.0 {
                grad.iter().map(|v| -v * gg / jgjg).collect()
            } else {
                grad.iter().map(|v| -v * delta / ng).collect()
            };
            let nsc = norm2(&sc);
            if nsc >= delta || !ngn.is_finite() {
                grad.iter().map(|v| -v * delta / ng).collect()
            } else {
                // s = sc + tau (sgn - sc), ||s|| = delta
                let dvec: Vec<f64> = sgn.iter().zip(&sc).map(|(a, b)| a - b).collect();
                let a = sumsq(&dvec);
                let b = 2.0 * sc.iter().zip(&dvec).map(|(p, q)| p * q).sum::<f64>();
                let c = nsc * nsc - delta * delta;
                let disc = (b * b - 4.0 * a * c).max(0.0);
                let tau = if a > 0.0 {
                    ((-b + disc.sqrt()) / (2.0 * a)).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                sc.iter().zip(&dvec).map(|(p, q)| p + tau * q).collect()
            }
        };
        let ns = norm2(&s);
        let xt: Vec<f64> = x.iter().zip(&s).map(|(a, b)| a + b).collect();
        let pt_t = r.point(it, &xt)?;
        let ft = sumsq(&pt_t.f);
        let js = matvec(&jac, m, n, &s);
        let pred = f - fv
            .iter()
            .zip(&js)
            .map(|(a, b)| (a + b) * (a + b))
            .sum::<f64>();
        let ared = f - ft;
        let rho = if !ft.is_finite() {
            -1.0
        } else if pred > 0.0 {
            ared / pred
        } else if ared > 0.0 {
            1.0
        } else {
            -1.0
        };
        // Radius update. The constants reproduce the radius column of the
        // iteration display in MATLAB's fsolve documentation example (no
        // growth for ratios around 0.8-0.9 on the boundary, growth by 2.5
        // after a very good, long step); MATLAB's exact rule is not
        // documented.
        if rho < 0.25 {
            delta = 0.25 * ns;
        } else if rho > 0.95 && ns >= 0.5 * delta {
            delta *= 2.5;
        }
        let accepted = rho > 1e-4;
        if accepted {
            x = xt;
            jac = r.jacobian(it, &x, &pt_t)?;
            fv = pt_t.f;
            f = ft;
            grad = tmatvec(&jac, m, n, &fv);
            opt = norm_inf(&grad);
            last_step = ns;
        }
        if cfg.disp == Disp::Iter {
            it.print(&format!(
                "{:>6}{:>12}{:>16}{:>15}{:>15}{:>16}\n",
                iters,
                r.fcount,
                g(f),
                g(ns),
                cfmt("%.3g", opt),
                g(delta)
            ));
        }
        let xtol = cfg.tolx * (f64::EPSILON.sqrt() + norm2(&x));
        if accepted && ns < xtol {
            break Stop::Step;
        }
        if !accepted && delta < xtol {
            break Stop::Radius;
        }
    };
    Ok(Sol {
        x,
        f: fv,
        j: jac,
        iters,
        stop,
        opt,
        step: last_step,
    })
}

/// Levenberg–Marquardt with optional bound projection.
pub(super) fn levmar(
    it: &mut Interp,
    r: &mut Residual,
    x0: &[f64],
    cfg: &Cfg,
    init: Option<Pt>,
) -> R<Sol> {
    let n = x0.len();
    let mut x = x0.to_vec();
    clip(&mut x, cfg);
    let (pt, mut jac) = start(it, r, &x, init)?;
    let mut fv = pt.f;
    let m = fv.len();
    let mut f = sumsq(&fv);
    let mut grad = tmatvec(&jac, m, n, &fv);
    let mut opt = norm_inf(&projected_grad(&grad, &x, cfg));
    let mut lambda = cfg.lambda0;
    let mut iters = 0usize;
    let mut last_step = 0.0;
    if cfg.disp == Disp::Iter {
        it.print(&format!(
            "\n                                        First-order                     Norm of\n Iteration  Func-count    {:<8}       optimality       Lambda           step\n{:>6}{:>12}{:>16}{:>16}{:>13}\n",
            cfg.label,
            0,
            r.fcount,
            g(f),
            cfmt("%.3g", opt),
            g(lambda)
        ));
    }
    let stop = loop {
        if opt <= cfg.tolopt {
            break Stop::Grad;
        }
        if iters as f64 >= cfg.maxiter {
            break Stop::MaxIter;
        }
        if r.fcount as f64 >= cfg.maxfev {
            break Stop::MaxFev;
        }
        iters += 1;
        // [J; sqrt(lambda) I] d = [-F; 0]
        let mm = m + n;
        let sl = lambda.sqrt();
        let mut aug = vec![0.0; mm * n];
        for j in 0..n {
            aug[j * mm..j * mm + m].copy_from_slice(&jac[j * m..(j + 1) * m]);
            aug[j * mm + m + j] = sl;
        }
        let mut rhs: Vec<f64> = fv.iter().map(|v| -v).collect();
        rhs.extend(std::iter::repeat_n(0.0, n));
        let d = lstsq(&aug, mm, n, &rhs);
        let mut xt: Vec<f64> = x.iter().zip(&d).map(|(a, b)| a + b).collect();
        clip(&mut xt, cfg);
        let s: Vec<f64> = xt.iter().zip(&x).map(|(a, b)| a - b).collect();
        let ns = norm2(&s);
        let xtol = cfg.tolx * (f64::EPSILON.sqrt() + norm2(&x));
        if !(ns >= xtol) {
            break Stop::SmallDir;
        }
        let pt_t = r.point(it, &xt)?;
        let ft = sumsq(&pt_t.f);
        let accepted = ft.is_finite() && ft < f;
        let fold = f;
        if accepted {
            x = xt;
            jac = r.jacobian(it, &x, &pt_t)?;
            fv = pt_t.f;
            f = ft;
            grad = tmatvec(&jac, m, n, &fv);
            opt = norm_inf(&projected_grad(&grad, &x, cfg));
            lambda = (lambda * 0.1).max(1e-30);
            last_step = ns;
        } else {
            lambda *= 10.0;
        }
        if cfg.disp == Disp::Iter {
            it.print(&format!(
                "{:>6}{:>12}{:>16}{:>16}{:>13}{:>15}\n",
                iters,
                r.fcount,
                g(f),
                cfmt("%.3g", opt),
                g(lambda),
                g(ns)
            ));
        }
        if accepted {
            let xtol = cfg.tolx * (f64::EPSILON.sqrt() + norm2(&x));
            if ns < xtol {
                break Stop::Step;
            }
            if f > 0.0 && fold - f <= cfg.tolfun * fold && !(opt <= cfg.tolopt) {
                break Stop::FunChange;
            }
        } else if !lambda.is_finite() || lambda > 1e30 {
            break Stop::SmallDir;
        }
    };
    Ok(Sol {
        x,
        f: fv,
        j: jac,
        iters,
        stop,
        opt,
        step: last_step,
    })
}

// ---------------------------------------------------------------------------
// Front ends
// ---------------------------------------------------------------------------

struct Common {
    user_jac: bool,
    central: bool,
    typx: Vec<f64>,
    fdstep: Option<Vec<f64>>,
    tolfun: f64,
    tolopt: f64,
    tolx: f64,
    maxiter: f64,
    maxfev: f64,
    disp: Disp,
    lambda0: f64,
    alg: Option<String>,
    defaults: Defaults,
}

/// Which tolerances/limits were left at their default (for messages).
struct Defaults {
    tolfun: bool,
    tolopt: bool,
    tolx: bool,
    maxiter: bool,
    maxfev: bool,
}

fn read_common(o: &Opts, n: usize) -> R<Common> {
    o.reject(&["OutputFcn"])?;
    o.reject(&["PlotFcn", "PlotFcns"])?;
    o.reject(&["CheckGradients", "DerivativeCheck"])?;
    o.reject(&["UseParallel"])?;
    o.reject(&["JacobianMultiplyFcn", "JacobMult"])?;
    let central = match o.text(&["FiniteDifferenceType", "FinDiffType"])?.as_deref() {
        None | Some("forward") => false,
        Some("central") => true,
        Some(other) => {
            return Err(err(
                "optimlib:options:FiniteDifferenceType",
                format!(
                    "Invalid value '{}' for option 'FiniteDifferenceType' (expected 'forward' or 'central').",
                    other
                ),
            ));
        }
    };
    let typx = o.vector(&["TypicalX"])?.unwrap_or_else(|| vec![1.0; n]);
    if typx.len() != n && typx.len() != 1 {
        return Err(err(
            "optimlib:options:TypicalX",
            "Option 'TypicalX' must have the same number of elements as X0.",
        ));
    }
    let typx = if typx.len() == 1 {
        vec![typx[0]; n]
    } else {
        typx
    };
    let tf_names = ["FunctionTolerance", "TolFun"];
    let to_names = ["OptimalityTolerance", "TolFun"];
    let tx_names = ["StepTolerance", "TolX"];
    let mi_names = ["MaxIterations", "MaxIter"];
    let mf_names = ["MaxFunctionEvaluations", "MaxFunEvals"];
    Ok(Common {
        user_jac: o.flag(&["SpecifyObjectiveGradient", "Jacobian"])?,
        central,
        typx,
        fdstep: o.vector(&["FiniteDifferenceStepSize", "FinDiffRelStep"])?,
        tolfun: o.f64(&tf_names, 1e-6)?,
        tolopt: o.f64(&to_names, 1e-6)?,
        tolx: o.f64(&tx_names, 1e-6)?,
        maxiter: o.count(&mi_names, 400.0)?,
        maxfev: o.count(&mf_names, 100.0 * n as f64)?,
        disp: o.display(Disp::Final)?,
        lambda0: o.f64(&["InitDamping"], 0.01)?,
        alg: o.text(&["Algorithm"])?,
        defaults: Defaults {
            tolfun: o.is_default(&tf_names, 1e-6),
            tolopt: o.is_default(&to_names, 1e-6),
            tolx: o.is_default(&tx_names, 1e-6),
            maxiter: o.is_default(&mi_names, 400.0),
            maxfev: o.is_default(&mf_names, 100.0 * n as f64),
        },
    })
}

fn x0_data(v: &Value, fname: &str) -> R<(Vec<f64>, Vec<usize>)> {
    let x = real_data(v, fname, "initial points X0")?;
    if x.is_empty() {
        return Err(err(
            &format!("optimlib:{}:EmptyX0", fname),
            format!("{}: X0 must not be empty.", fname),
        ));
    }
    Ok((x, v.dims()))
}

fn dv(default: bool) -> &'static str {
    if default {
        "default value of the "
    } else {
        "value of the "
    }
}

fn limit_text(fname: &str, stop: Stop, c: &Common) -> String {
    if stop == Stop::MaxIter {
        format!(
            "Solver stopped prematurely.\n\n{} stopped because it exceeded the iteration limit,\noptions.MaxIterations = {}{}.",
            fname,
            cfmt("%g", c.maxiter),
            if c.defaults.maxiter {
                " (the default value)"
            } else {
                ""
            }
        )
    } else {
        format!(
            "Solver stopped prematurely.\n\n{} stopped because it exceeded the function evaluation limit,\noptions.MaxFunctionEvaluations = {}{}.",
            fname,
            cfmt("%g", c.maxfev),
            if c.defaults.maxfev {
                " (the default value)"
            } else {
                ""
            }
        )
    }
}

fn show(it: &mut Interp, disp: Disp, flag: i32, msg: &str) {
    let show = match disp {
        Disp::Off => false,
        Disp::Notify => flag <= 0,
        Disp::Final | Disp::Iter => true,
    };
    if show {
        let details = if flag != 0 {
            "\n\n<stopping criteria details>\n"
        } else {
            "\n\n"
        };
        it.print(&format!("\n{}{}", msg, details));
    }
}

fn fval_value(r: &Residual, f: Vec<f64>) -> Value {
    let dims = r.fdims.clone().unwrap_or_else(|| vec![f.len(), 1]);
    Value::matrix(dims, f)
}

/// `[x, fval, exitflag, output, jacobian] = fsolve(fun, x0[, options])`
pub(super) fn fsolve(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "fsolve")?;
    at_most(&a, 3, "fsolve")?;
    if matches!(a[0], Value::Struct(_)) {
        return Err(err(
            "optimlib:fsolve:ProblemStruct",
            "fsolve: the problem-structure syntax is not supported by this engine.",
        ));
    }
    need_function(&a[0], "fsolve")?;
    let (x0, shape) = x0_data(&a[1], "fsolve")?;
    let n = x0.len();
    let o = Opts::new(a.get(2), "fsolve")?;
    let c = read_common(&o, n)?;
    o.reject(&["SubproblemAlgorithm"])?;
    let mut r = Residual {
        f: &a[0],
        shape,
        extra: vec![],
        sub: None,
        user_jac: c.user_jac,
        central: c.central,
        typx: c.typx.clone(),
        fdstep: c.fdstep.clone(),
        lb: None,
        ub: None,
        fcount: 0,
        fdims: None,
        fname: "fsolve",
    };
    let mut cfg = Cfg {
        tolfun: c.tolfun,
        tolopt: c.tolopt,
        tolx: c.tolx,
        maxiter: c.maxiter,
        maxfev: c.maxfev,
        disp: c.disp,
        lambda0: c.lambda0,
        lb: None,
        ub: None,
        label: "Residual",
    };
    let alg = c
        .alg
        .clone()
        .unwrap_or_else(|| "trust-region-dogleg".into());
    let use_lm = match alg.as_str() {
        "trust-region-dogleg" | "trust-region" => false,
        "levenberg-marquardt" | "lm" => true,
        other => {
            return Err(err(
                "optimlib:fsolve:InvalidAlgorithm",
                format!(
                    "fsolve: invalid value '{}' for option 'Algorithm' (expected 'trust-region-dogleg', 'trust-region' or 'levenberg-marquardt').",
                    other
                ),
            ));
        }
    };
    // the number of equations is known after the first evaluation
    let sol = if use_lm {
        levmar(it, &mut r, &x0, &cfg, None)?
    } else {
        // the dogleg needs a square system; otherwise fall back to LM
        let probe = r.point(it, &x0)?;
        if probe.f.len() != n {
            if alg == "trust-region-dogleg" {
                it.warning(
                    "optim:fsolve:NonSquareSystem",
                    "Trust-region-dogleg algorithm of FSOLVE cannot handle non-square systems; using Levenberg-Marquardt algorithm instead.",
                );
            }
            cfg.label = "Residual";
            levmar(it, &mut r, &x0, &cfg, Some(probe))?
        } else {
            dogleg(it, &mut r, &x0, &cfg, Some(probe))?
        }
    };
    let used_lm = use_lm || r.m() != n;
    let fsq = sumsq(&sol.f);
    let mut flag: i32 = match sol.stop {
        Stop::Grad => 1,
        Stop::Step => 2,
        Stop::FunChange => 3,
        Stop::SmallDir => 4,
        Stop::MaxIter | Stop::MaxFev => 0,
        Stop::Radius => -3,
    };
    if flag > 0 && !(fsq <= c.tolfun.sqrt()) {
        flag = -2;
    }
    let d = &c.defaults;
    let msg = match (flag, sol.stop) {
        (1, _) => format!(
            "Equation solved.\n\nfsolve completed because the vector of function values is near zero\nas measured by the {}function tolerance, and\nthe problem appears regular as measured by the gradient.",
            dv(d.tolfun)
        ),
        (2, _) => format!(
            "Equation solved, solver stalled.\n\nfsolve stopped because the relative size of the current step is less than the\n{}step size tolerance and the vector of function values\nis near zero as measured by the {}function tolerance.",
            dv(d.tolx),
            dv(d.tolfun)
        ),
        (3, _) => format!(
            "Equation solved, solver stalled.\n\nfsolve stopped because the relative change in the sum of squares of the\nfunction values is less than the {}function tolerance and the vector of\nfunction values is near zero as measured by the {}function tolerance.",
            dv(d.tolfun),
            dv(d.tolfun)
        ),
        (4, _) => format!(
            "Equation solved, solver stalled.\n\nfsolve stopped because the norm of the current step is less than the\n{}step size tolerance and the vector of function values\nis near zero as measured by the {}function tolerance.",
            dv(d.tolx),
            dv(d.tolfun)
        ),
        (0, s) => limit_text("fsolve", s, &c),
        (-2, Stop::Grad) => format!(
            "No solution found.\n\nfsolve stopped because the problem appears regular as measured by the gradient,\nbut the vector of function values is not near zero as measured by the\n{}function tolerance.",
            dv(d.tolfun)
        ),
        (-2, _) => format!(
            "No solution found.\n\nfsolve stopped because the relative size of the current step is less than the\n{}step size tolerance, but the vector of function values\nis not near zero as measured by the {}function tolerance.",
            dv(d.tolx),
            dv(d.tolfun)
        ),
        _ => format!(
            "No solution found.\n\nfsolve stopped because the trust-region radius became smaller than the\n{}step size tolerance, and the vector of function values\nis not near zero as measured by the {}function tolerance.",
            dv(d.tolx),
            dv(d.tolfun)
        ),
    };
    show(it, c.disp, flag, &msg);
    let m = sol.f.len();
    let out = make_struct(vec![
        ("iterations", Value::num(sol.iters as f64)),
        ("funcCount", Value::num(r.fcount as f64)),
        (
            "algorithm",
            Value::str(if used_lm {
                "levenberg-marquardt"
            } else {
                "trust-region-dogleg"
            }),
        ),
        ("firstorderopt", Value::num(sol.opt)),
        ("message", Value::str(&msg)),
    ]);
    let jac = Value::matrix(vec![m, n], sol.j);
    let xv = Value::matrix(r.shape.clone(), sol.x);
    let fv = fval_value(&r, sol.f);
    take_n(vec![xv, fv, Value::num(flag as f64), out, jac], nargout)
}

fn bounds(v: Option<&Value>, n: usize, fname: &str, which: &str) -> R<Option<Vec<f64>>> {
    let Some(v) = v else { return Ok(None) };
    if v.is_empty() {
        return Ok(None);
    }
    let b = real_data(v, fname, which)?;
    if b.len() != n {
        return Err(err(
            &format!("optimlib:{}:InvalidBounds", fname),
            format!(
                "{}: {} must have the same number of elements as X0 ({}).",
                fname, which, n
            ),
        ));
    }
    Ok(Some(b))
}

/// Shared driver of lsqnonlin / lsqcurvefit.
fn lsq_driver(
    it: &mut Interp,
    fname: &'static str,
    f: &Value,
    x0v: &Value,
    extra: Vec<Value>,
    sub: Option<Vec<f64>>,
    lbv: Option<&Value>,
    ubv: Option<&Value>,
    optv: Option<&Value>,
    nargout: usize,
) -> R<Vec<Value>> {
    need_function(f, fname)?;
    let (x0, shape) = x0_data(x0v, fname)?;
    let n = x0.len();
    let lb = bounds(lbv, n, fname, "lower bounds LB")?;
    let ub = bounds(ubv, n, fname, "upper bounds UB")?;
    let o = Opts::new(optv, fname)?;
    let c = read_common(&o, n)?;
    match c.alg.as_deref() {
        None | Some("trust-region-reflective") | Some("levenberg-marquardt") => {}
        Some(other) => {
            return Err(err(
                &format!("optimlib:{}:InvalidAlgorithm", fname),
                format!(
                    "{}: Algorithm '{}' is not supported by this engine (expected 'trust-region-reflective' or 'levenberg-marquardt'; both run Levenberg-Marquardt here).",
                    fname, other
                ),
            ));
        }
    }
    if let (Some(l), Some(u)) = (&lb, &ub)
        && l.iter().zip(u).any(|(p, q)| p > q)
    {
        let msg = "Exiting due to infeasibility: at least one lower bound exceeds the corresponding upper bound.".to_string();
        show(it, c.disp, -2, &msg);
        let out = make_struct(vec![
            ("iterations", Value::num(0.0)),
            ("funcCount", Value::num(0.0)),
            ("algorithm", Value::str("levenberg-marquardt")),
            ("message", Value::str(&msg)),
        ]);
        return take_n(
            vec![
                Value::matrix(shape, x0),
                Value::empty(),
                Value::empty(),
                Value::num(-2.0),
                out,
            ],
            nargout,
        );
    }
    let mut r = Residual {
        f,
        shape,
        extra,
        sub,
        user_jac: c.user_jac,
        central: c.central,
        typx: c.typx.clone(),
        fdstep: c.fdstep.clone(),
        lb: lb.clone(),
        ub: ub.clone(),
        fcount: 0,
        fdims: None,
        fname,
    };
    let cfg = Cfg {
        tolfun: c.tolfun,
        tolopt: c.tolopt,
        tolx: c.tolx,
        maxiter: c.maxiter,
        maxfev: c.maxfev,
        disp: c.disp,
        lambda0: c.lambda0,
        lb,
        ub,
        label: "Resnorm",
    };
    let mut x0 = x0;
    clip(&mut x0, &cfg);
    let sol = levmar(it, &mut r, &x0, &cfg, None)?;
    let flag: i32 = match sol.stop {
        Stop::Grad => 1,
        Stop::Step => 2,
        Stop::FunChange => 3,
        Stop::SmallDir | Stop::Radius => 4,
        Stop::MaxIter | Stop::MaxFev => 0,
    };
    let d = &c.defaults;
    let msg = match flag {
        1 => format!(
            "Local minimum found.\n\nOptimization completed because the size of the gradient is less than\nthe {}optimality tolerance.",
            dv(d.tolopt)
        ),
        2 => format!(
            "Local minimum possible.\n\n{} stopped because the size of the current step is less than\nthe {}step size tolerance.",
            fname,
            dv(d.tolx)
        ),
        3 => format!(
            "Local minimum possible.\n\n{} stopped because the final change in the sum of squares relative to\nits initial value is less than the {}function tolerance.",
            fname,
            dv(d.tolfun)
        ),
        4 => format!(
            "Local minimum possible.\n\n{} stopped because the norm of the current search direction is less than\nthe {}step size tolerance.",
            fname,
            dv(d.tolx)
        ),
        _ => limit_text(fname, sol.stop, &c),
    };
    show(it, c.disp, flag, &msg);
    let m = sol.f.len();
    let resnorm = sumsq(&sol.f);
    // Lagrange multiplier estimates of the active bounds from the gradient
    // of the sum of squares, 2 J' F.
    let grad = tmatvec(&sol.j, m, n, &sol.f);
    let mut lower = vec![0.0; n];
    let mut upper = vec![0.0; n];
    for i in 0..n {
        if cfg.lb.as_ref().is_some_and(|l| sol.x[i] <= l[i]) && grad[i] > 0.0 {
            lower[i] = 2.0 * grad[i];
        }
        if cfg.ub.as_ref().is_some_and(|u| sol.x[i] >= u[i]) && grad[i] < 0.0 {
            upper[i] = -2.0 * grad[i];
        }
    }
    let lambda = make_struct(vec![
        ("lower", Value::col(lower)),
        ("upper", Value::col(upper)),
    ]);
    let out = make_struct(vec![
        ("firstorderopt", Value::num(sol.opt)),
        ("iterations", Value::num(sol.iters as f64)),
        ("funcCount", Value::num(r.fcount as f64)),
        ("stepsize", Value::num(sol.step)),
        ("algorithm", Value::str("levenberg-marquardt")),
        ("message", Value::str(&msg)),
    ]);
    let jac = Value::matrix(vec![m, n], sol.j);
    let xv = Value::matrix(r.shape.clone(), sol.x);
    let res = fval_value(&r, sol.f);
    take_n(
        vec![
            xv,
            Value::num(resnorm),
            res,
            Value::num(flag as f64),
            out,
            lambda,
            jac,
        ],
        nargout,
    )
}

/// `[x, resnorm, residual, exitflag, output, lambda, jacobian] =
/// lsqnonlin(fun, x0[, lb, ub, options])`
pub(super) fn lsqnonlin(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "lsqnonlin")?;
    if a.len() > 5 {
        return Err(err(
            "optimlib:lsqnonlin:UnsupportedConstraints",
            "lsqnonlin: linear and nonlinear constraints (A, b, Aeq, beq, nonlcon) are not supported by this engine.",
        ));
    }
    lsq_driver(
        it,
        "lsqnonlin",
        &a[0],
        &a[1],
        vec![],
        None,
        a.get(2),
        a.get(3),
        a.get(4),
        nargout,
    )
}

/// `[x, resnorm, residual, exitflag, output, lambda, jacobian] =
/// lsqcurvefit(fun, x0, xdata, ydata[, lb, ub, options])`
pub(super) fn lsqcurvefit(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 4, "lsqcurvefit")?;
    if a.len() > 7 {
        return Err(err(
            "optimlib:lsqcurvefit:UnsupportedConstraints",
            "lsqcurvefit: linear and nonlinear constraints (A, b, Aeq, beq, nonlcon) are not supported by this engine.",
        ));
    }
    let ydata = real_data(&a[3], "lsqcurvefit", "YDATA")?;
    lsq_driver(
        it,
        "lsqcurvefit",
        &a[0],
        &a[1],
        vec![a[2].clone()],
        Some(ydata),
        a.get(4),
        a.get(5),
        a.get(6),
        nargout,
    )
}
