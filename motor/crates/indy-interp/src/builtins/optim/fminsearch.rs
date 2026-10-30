//! `fminsearch`: Nelder–Mead simplex search (Lagarias, Reeds, Wright and
//! Wright 1998) with MATLAB's initial simplex, coefficients and stopping
//! test.

use indy_value::Value;

use super::super::{err, need};
use super::options::{Disp, Opts};
use super::{call_n, cfmt, eps_of, make_struct, need_function, take_n};
use crate::{Interp, R};

struct Obj<'a> {
    f: &'a Value,
    shape: Vec<usize>,
    extra: &'a [Value],
    check: bool,
    count: usize,
}

impl Obj<'_> {
    fn eval(&mut self, it: &mut Interp, x: &[f64]) -> R<f64> {
        let mut args = vec![Value::matrix(self.shape.clone(), x.to_vec())];
        args.extend(self.extra.iter().cloned());
        let r = call_n(it, self.f, args, 1, "fminsearch")?;
        self.count += 1;
        let v = &r[0];
        let y = match v {
            Value::Num(n) if n.im.is_none() && n.re.len() == 1 => n.re[0],
            Value::Bool(_) if v.numel() == 1 => v.scalar_f64().unwrap_or(0.0),
            Value::Sparse(s) if v.numel() == 1 && s.im.is_none() => v.scalar_f64().unwrap_or(0.0),
            Value::Num(n) if n.im.is_some() && n.re.len() == 1 && self.check => {
                return Err(err(
                    "MATLAB:fminsearch:checkfun:ComplexFval",
                    "User function returned a complex value when evaluated;\n FMINSEARCH cannot continue.",
                ));
            }
            _ => {
                return Err(err(
                    "MATLAB:fminsearch:checkfun:NonScalarOrComplexFval",
                    "fminsearch: the objective function must return a real scalar value.",
                ));
            }
        };
        if self.check && y.is_nan() {
            return Err(err(
                "MATLAB:fminsearch:checkfun:NaNFval",
                "User function returned NaN when evaluated;\n FMINSEARCH cannot continue.",
            ));
        }
        if self.check && y.is_infinite() {
            return Err(err(
                "MATLAB:fminsearch:checkfun:InfFval",
                "User function returned Inf or -Inf when evaluated;\n FMINSEARCH cannot continue.",
            ));
        }
        Ok(y)
    }
}

/// Stable ascending sort of the vertices by value (NaN last, like `sort`).
fn order(v: &mut [Vec<f64>], fv: &mut [f64]) {
    let mut idx: Vec<usize> = (0..fv.len()).collect();
    idx.sort_by(|&a, &b| match (fv[a].is_nan(), fv[b].is_nan()) {
        (true, true) => std::cmp::Ordering::Equal,
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        _ => fv[a]
            .partial_cmp(&fv[b])
            .unwrap_or(std::cmp::Ordering::Equal),
    });
    let nv: Vec<Vec<f64>> = idx.iter().map(|&i| v[i].clone()).collect();
    let nf: Vec<f64> = idx.iter().map(|&i| fv[i]).collect();
    v.clone_from_slice(&nv);
    fv.copy_from_slice(&nf);
}

fn row(it: &mut Interp, iter: usize, count: usize, f: f64, how: &str) {
    it.print(&format!(
        " {}        {}     {}         {}\n",
        cfmt("%5.0f", iter as f64),
        cfmt("%5.0f", count as f64),
        cfmt("%12.6g", f),
        how
    ));
}

/// `[x, fval, exitflag, output] = fminsearch(fun, x0[, options, p1, ...])`
pub(super) fn fminsearch(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "fminsearch")?;
    if matches!(a[0], Value::Struct(_)) {
        return Err(err(
            "MATLAB:fminsearch:ProblemStruct",
            "fminsearch: the problem-structure syntax is not supported by this engine.",
        ));
    }
    need_function(&a[0], "fminsearch")?;
    let x0 = match &a[1] {
        Value::Num(n) if n.im.is_some() => {
            return Err(err(
                "MATLAB:fminsearch:ComplexX0",
                "fminsearch: complex X0 is not supported.",
            ));
        }
        v @ (Value::Num(_) | Value::Bool(_)) => v.to_f64_vec().unwrap_or_default(),
        v => {
            return Err(err(
                "MATLAB:fminsearch:NonDoubleInput",
                format!(
                    "FMINSEARCH only accepts inputs of data type double, got {}.",
                    v.class_name()
                ),
            ));
        }
    };
    let shape = a[1].dims();
    let n = x0.len();
    let o = Opts::new(a.get(2), "fminsearch")?;
    o.reject(&["OutputFcn"])?;
    o.reject(&["PlotFcns", "PlotFcn"])?;
    let disp = o.display(Disp::Notify)?;
    let tolx = o.f64(&["TolX", "StepTolerance"], 1e-4)?;
    let tolf = o.f64(&["TolFun", "FunctionTolerance"], 1e-4)?;
    let maxfun = o.count(&["MaxFunEvals", "MaxFunctionEvaluations"], 200.0 * n as f64)?;
    let maxiter = o.count(&["MaxIter", "MaxIterations"], 200.0 * n as f64)?;
    let check = o.flag(&["FunValCheck"])?;
    let extra: &[Value] = if a.len() > 3 { &a[3..] } else { &[] };
    let mut obj = Obj {
        f: &a[0],
        shape: shape.clone(),
        extra,
        check,
        count: 0,
    };

    let header = |it: &mut Interp| {
        it.print("\n Iteration   Func-count     min f(x)         Procedure\n");
    };
    let mut v: Vec<Vec<f64>> = vec![x0.clone()];
    let mut fv: Vec<f64> = vec![obj.eval(it, &x0)?];
    if disp == Disp::Iter {
        header(it);
        row(it, 0, obj.count, fv[0], "");
    }
    let mut itercount = 0usize;
    let mut flag = 1;
    let mut how;
    if n > 0 {
        for j in 0..n {
            let mut y = x0.clone();
            y[j] = if y[j] != 0.0 { 1.05 * y[j] } else { 0.00025 };
            fv.push(obj.eval(it, &y)?);
            v.push(y);
        }
        order(&mut v, &mut fv);
        itercount = 1;
        how = "initial simplex";
        if disp == Disp::Iter {
            row(it, itercount, obj.count, fv[0], how);
        }
        loop {
            if !((obj.count as f64) < maxfun && (itercount as f64) < maxiter) {
                break;
            }
            let fspread = fv[1..]
                .iter()
                .map(|f| (fv[0] - f).abs())
                .fold(0.0f64, |m, d| if d.is_nan() { f64::NAN } else { m.max(d) });
            let xspread = v[1..]
                .iter()
                .flat_map(|vi| vi.iter().zip(&v[0]).map(|(p, q)| (p - q).abs()))
                .fold(0.0f64, |m, d| if d.is_nan() { f64::NAN } else { m.max(d) });
            let vmax = v[0].iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            if fspread <= tolf.max(10.0 * eps_of(fv[0])) && xspread <= tolx.max(10.0 * eps_of(vmax))
            {
                break;
            }
            // centroid of the n best points
            let mut xbar = vec![0.0; n];
            for vi in &v[..n] {
                for k in 0..n {
                    xbar[k] += vi[k];
                }
            }
            for c in xbar.iter_mut() {
                *c /= n as f64;
            }
            let worst = v[n].clone();
            // MATLAB's forms: (1 + rho) xbar - rho w, (1 + rho chi) xbar - rho chi w, ...
            let lin = |p: f64, q: f64| -> Vec<f64> {
                xbar.iter()
                    .zip(&worst)
                    .map(|(c, w)| p * c + q * w)
                    .collect()
            };
            let xr = lin(2.0, -1.0);
            let fxr = obj.eval(it, &xr)?;
            if fxr < fv[0] {
                let xe = lin(3.0, -2.0);
                let fxe = obj.eval(it, &xe)?;
                if fxe < fxr {
                    v[n] = xe;
                    fv[n] = fxe;
                    how = "expand";
                } else {
                    v[n] = xr;
                    fv[n] = fxr;
                    how = "reflect";
                }
            } else if fxr < fv[n - 1] {
                v[n] = xr;
                fv[n] = fxr;
                how = "reflect";
            } else {
                let mut shrink = false;
                if fxr < fv[n] {
                    let xc = lin(1.5, -0.5);
                    let fxc = obj.eval(it, &xc)?;
                    if fxc <= fxr {
                        v[n] = xc;
                        fv[n] = fxc;
                        how = "contract outside";
                    } else {
                        shrink = true;
                    }
                } else {
                    let xcc = lin(0.5, 0.5);
                    let fxcc = obj.eval(it, &xcc)?;
                    if fxcc < fv[n] {
                        v[n] = xcc;
                        fv[n] = fxcc;
                        how = "contract inside";
                    } else {
                        shrink = true;
                    }
                }
                if shrink {
                    for j in 1..=n {
                        let nvj: Vec<f64> = v[j]
                            .iter()
                            .zip(&v[0])
                            .map(|(p, q)| q + 0.5 * (p - q))
                            .collect();
                        fv[j] = obj.eval(it, &nvj)?;
                        v[j] = nvj;
                    }
                    how = "shrink";
                }
            }
            order(&mut v, &mut fv);
            itercount += 1;
            if disp == Disp::Iter {
                row(it, itercount, obj.count, fv[0], how);
            }
        }
        if obj.count as f64 >= maxfun {
            flag = 0;
        } else if itercount as f64 >= maxiter {
            flag = -1;
        }
    }
    let msg = match flag {
        1 => format!(
            "Optimization terminated:\n the current x satisfies the termination criteria using OPTIONS.TolX of {} \n and F(X) satisfies the convergence criteria using OPTIONS.TolFun of {} \n",
            cfmt("%e", tolx),
            cfmt("%e", tolf)
        ),
        0 => format!(
            "Exiting: Maximum number of function evaluations has been exceeded\n         - increase MaxFunEvals option.\n         Current function value: {} \n",
            cfmt("%f", fv[0])
        ),
        _ => format!(
            "Exiting: Maximum number of iterations has been exceeded\n         - increase MaxIter option.\n         Current function value: {} \n",
            cfmt("%f", fv[0])
        ),
    };
    let flag = flag.max(0);
    let show = match disp {
        Disp::Off => false,
        Disp::Notify => flag == 0,
        Disp::Final | Disp::Iter => true,
    };
    if show {
        it.print(&format!(" \n{}", msg));
    }
    let out = make_struct(vec![
        ("iterations", Value::num(itercount as f64)),
        ("funcCount", Value::num(obj.count as f64)),
        ("algorithm", Value::str("Nelder-Mead simplex direct search")),
        ("message", Value::str(&msg)),
    ]);
    take_n(
        vec![
            Value::matrix(shape, v[0].clone()),
            Value::num(fv[0]),
            Value::num(flag as f64),
            out,
        ],
        nargout,
    )
}
