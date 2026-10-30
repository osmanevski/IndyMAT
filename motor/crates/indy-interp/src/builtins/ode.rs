//! ODE solvers: adaptive explicit Runge–Kutta (Dormand–Prince 5(4) for
//! `ode45`, Bogacki–Shampine 3(2) for `ode23`), dense output by cubic
//! Hermite interpolation (`deval`, requested output times), `odeset`.
//!
//! `ode113`, `ode15s`, `ode23s`, `ode23t`, `ode23tb` use the same explicit
//! integrators (Dormand–Prince or Bogacki–Shampine): results meet the
//! requested tolerances, but stiff problems are slow — a documented
//! phase-2 limitation, not MATLAB's method.

use indy_value::{Array, Num, StructArray, Value};

use super::{arg_f64, arg_num, err, need, one, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "ode45", |it, a, n| {
        solve(it, a, n, Method::Dp45, "ode45")
    });
    reg(it, "ode23", |it, a, n| {
        solve(it, a, n, Method::Bs23, "ode23")
    });
    reg(it, "ode113", |it, a, n| {
        solve(it, a, n, Method::Dp45, "ode113")
    });
    reg(it, "ode78", |it, a, n| {
        solve(it, a, n, Method::Dp45, "ode78")
    });
    reg(it, "ode89", |it, a, n| {
        solve(it, a, n, Method::Dp45, "ode89")
    });
    reg(it, "ode15s", |it, a, n| {
        solve(it, a, n, Method::Dp45, "ode15s")
    });
    reg(it, "ode23s", |it, a, n| {
        solve(it, a, n, Method::Bs23, "ode23s")
    });
    reg(it, "ode23t", |it, a, n| {
        solve(it, a, n, Method::Bs23, "ode23t")
    });
    reg(it, "ode23tb", |it, a, n| {
        solve(it, a, n, Method::Bs23, "ode23tb")
    });
    reg(it, "odeset", odeset);
    reg(it, "odeget", |_, a, _| {
        need(&a, 2, "odeget")?;
        let name = a[1].to_str().unwrap_or_default();
        let v = match &a[0] {
            Value::Struct(s) => s
                .fields
                .iter()
                .position(|f| f.eq_ignore_ascii_case(&name))
                .map(|i| s.elems[0][i].clone())
                .filter(|v| !v.is_empty()),
            _ => None,
        };
        one(v.or_else(|| a.get(2).cloned()).unwrap_or_default())
    });
    reg(it, "deval", deval);
}

#[derive(Clone, Copy)]
enum Method {
    Dp45,
    Bs23,
}

const ODE_FIELDS: [&str; 22] = [
    "AbsTol",
    "BDF",
    "Events",
    "InitialStep",
    "Jacobian",
    "JConstant",
    "JPattern",
    "Mass",
    "MassSingular",
    "MaxOrder",
    "MaxStep",
    "MinStep",
    "MStateDependence",
    "MvPattern",
    "NonNegative",
    "NormControl",
    "OutputFcn",
    "OutputSel",
    "Refine",
    "RelTol",
    "Stats",
    "Vectorized",
];

fn odeset(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let mut s = StructArray::empty_with(
        ODE_FIELDS.iter().map(|f| f.to_string()).collect(),
        vec![1, 1],
    );
    let mut i = 0;
    while i < a.len() {
        if let Value::Struct(old) = &a[i] {
            for (k, f) in old.fields.iter().enumerate() {
                if !old.elems[0][k].is_empty() {
                    s.set(0, f, old.elems[0][k].clone());
                }
            }
            i += 1;
            continue;
        }
        let name = a[i].to_str().ok_or_else(|| {
            err(
                "MATLAB:odeset:InvalidPropName",
                "odeset: property names must be text.",
            )
        })?;
        let v = a.get(i + 1).cloned().ok_or_else(|| {
            err(
                "MATLAB:odeset:NoValue",
                format!("odeset: no value for '{}'.", name),
            )
        })?;
        let real = ODE_FIELDS
            .iter()
            .find(|f| f.eq_ignore_ascii_case(&name))
            .map(|f| f.to_string())
            .unwrap_or(name);
        s.set(0, &real, v);
        i += 2;
    }
    one(Value::Struct(s))
}

fn opt(o: Option<&Value>, name: &str) -> Option<Value> {
    match o {
        Some(Value::Struct(s)) => s
            .fields
            .iter()
            .position(|f| f.eq_ignore_ascii_case(name))
            .map(|i| s.elems[0][i].clone())
            .filter(|v| !v.is_empty()),
        _ => None,
    }
}

fn call_f(it: &mut Interp, f: &Value, t: f64, y: &[f64]) -> R<Vec<f64>> {
    let r = it.call_value(f, vec![Value::num(t), Value::col(y.to_vec())], 1)?;
    let v = r.first().ok_or_else(|| {
        err(
            "MATLAB:odearguments:NoOutput",
            "ODE function returned no value.",
        )
    })?;
    let d = v.to_f64_vec().ok_or_else(|| {
        err(
            "MATLAB:odearguments:NotNumeric",
            "ODE function must return a numeric column.",
        )
    })?;
    if d.len() != y.len() {
        return Err(err(
            "MATLAB:odearguments:SizeIC",
            format!(
                "ODE function returns a vector of length {}, but the initial condition has length {}.",
                d.len(),
                y.len()
            ),
        ));
    }
    Ok(d)
}

/// One accepted step and what its continuous extension needs.
struct Dense {
    t: f64,
    h: f64,
    y: Vec<f64>,
    /// Dormand-Prince stages; `None` for ode23 (cubic Hermite then).
    k: Option<Vec<Vec<f64>>>,
    y1: Vec<f64>,
    f0: Vec<f64>,
    f1: Vec<f64>,
}

impl Dense {
    /// Solution at `x` inside the step: Shampine's fourth-order
    /// continuous extension of the Dormand-Prince pair (L. F. Shampine,
    /// "Some practical Runge-Kutta formulas", Math. Comp. 46, 1986), the
    /// interpolant MATLAB's ode45 documents for Refine and tspan output.
    fn eval(&self, x: f64) -> Vec<f64> {
        let s = (x - self.t) / self.h;
        let Some(k) = &self.k else {
            let m = Mesh {
                t: vec![self.t, self.t + self.h],
                y: vec![self.y.clone(), self.y1.clone()],
                f: vec![self.f0.clone(), self.f1.clone()],
            };
            return hermite(&m, x, false);
        };
        const BI: [[f64; 4]; 7] = [
            [1.0, -183.0 / 64.0, 37.0 / 12.0, -145.0 / 128.0],
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 1500.0 / 371.0, -1000.0 / 159.0, 1000.0 / 371.0],
            [0.0, -125.0 / 32.0, 125.0 / 12.0, -375.0 / 64.0],
            [0.0, 9477.0 / 3392.0, -729.0 / 106.0, 25515.0 / 6784.0],
            [0.0, -11.0 / 7.0, 11.0 / 3.0, -55.0 / 28.0],
            [0.0, 1.5, -4.0, 2.5],
        ];
        let sp = [s, s * s, s * s * s, s * s * s * s];
        let w: Vec<f64> = BI
            .iter()
            .map(|r| (0..4).map(|j| r[j] * sp[j]).sum())
            .collect();
        (0..self.y.len())
            .map(|i| self.y[i] + self.h * (0..7).map(|j| w[j] * k[j][i]).sum::<f64>())
            .collect()
    }
}

struct Mesh {
    t: Vec<f64>,
    y: Vec<Vec<f64>>,
    f: Vec<Vec<f64>>,
}

fn hermite(m: &Mesh, x: f64, deriv: bool) -> Vec<f64> {
    let n = m.t.len();
    let fwd = m.t[n - 1] >= m.t[0];
    // Find interval [k, k+1] containing x.
    let mut k = 0;
    while k + 2 < n && (if fwd { x > m.t[k + 1] } else { x < m.t[k + 1] }) {
        k += 1;
    }
    if n == 1 {
        return m.y[0].clone();
    }
    let (t0, t1) = (m.t[k], m.t[k + 1]);
    let h = t1 - t0;
    let s = (x - t0) / h;
    let (y0, y1, f0, f1) = (&m.y[k], &m.y[k + 1], &m.f[k], &m.f[k + 1]);
    (0..y0.len())
        .map(|i| {
            if deriv {
                let h00 = 6.0 * s * s - 6.0 * s;
                let h10 = 3.0 * s * s - 4.0 * s + 1.0;
                let h01 = -6.0 * s * s + 6.0 * s;
                let h11 = 3.0 * s * s - 2.0 * s;
                (h00 * y0[i] + h01 * y1[i]) / h + h10 * f0[i] + h11 * f1[i]
            } else {
                let h00 = 2.0 * s.powi(3) - 3.0 * s * s + 1.0;
                let h10 = s.powi(3) - 2.0 * s * s + s;
                let h01 = -2.0 * s.powi(3) + 3.0 * s * s;
                let h11 = s.powi(3) - s * s;
                h00 * y0[i] + h10 * h * f0[i] + h01 * y1[i] + h11 * h * f1[i]
            }
        })
        .collect()
}

fn solve(
    it: &mut Interp,
    a: Vec<Value>,
    nargout: usize,
    method: Method,
    name: &str,
) -> R<Vec<Value>> {
    need(&a, 3, name)?;
    let f = a[0].clone();
    let tspan = arg_num(&a[1], name)?.re;
    if tspan.len() < 2 {
        return Err(err(
            "MATLAB:odearguments:TspanLen",
            "The tspan vector must have at least two elements.",
        ));
    }
    let y0 = arg_num(&a[2], name)?.re;
    let o = a.get(3);
    let rtol = opt(o, "RelTol")
        .and_then(|v| v.scalar_f64())
        .unwrap_or(1e-3);
    let atol: Vec<f64> = match opt(o, "AbsTol").and_then(|v| v.to_f64_vec()) {
        Some(v) if v.len() == y0.len() => v,
        Some(v) if !v.is_empty() => vec![v[0]; y0.len()],
        _ => vec![1e-6; y0.len()],
    };
    let (t0, tf) = (tspan[0], tspan[tspan.len() - 1]);
    let dir = if tf >= t0 { 1.0 } else { -1.0 };
    let span = (tf - t0).abs();
    let maxstep = opt(o, "MaxStep")
        .and_then(|v| v.scalar_f64())
        .unwrap_or(span / 10.0)
        .abs();
    let refine = opt(o, "Refine")
        .and_then(|v| v.scalar_f64())
        .unwrap_or(match method {
            Method::Dp45 if name == "ode45" => 4.0,
            _ => 1.0,
        }) as usize;
    let order = match method {
        Method::Dp45 => 5.0,
        Method::Bs23 => 3.0,
    };
    let mut t = t0;
    let mut y = y0.clone();
    let mut fy = call_f(it, &f, t, &y)?;
    // Step control after Shampine & Reichelt, "The MATLAB ODE Suite"
    // (SIAM J. Sci. Comput. 18, 1997): errors are measured against
    // max(|y|, AbsTol/RelTol) and compared with RelTol.
    let pow = 1.0 / order;
    let hmin_at = |t: f64| 16.0 * f64::EPSILON * t.abs().max(f64::MIN_POSITIVE);
    let mut h = match opt(o, "InitialStep").and_then(|v| v.scalar_f64()) {
        Some(h) => h.abs().min(maxstep).min(span),
        None => {
            // Initial step from the slope at t0.
            let rh = (0..y.len())
                .map(|i| (fy[i] / y[i].abs().max(atol[i] / rtol)).abs())
                .fold(0.0, f64::max)
                / (0.8 * rtol.powf(pow));
            let mut h = maxstep.min(span);
            if h * rh > 1.0 {
                h = 1.0 / rh;
            }
            h.max(hmin_at(t0))
        }
    };
    let mut mesh = Mesh {
        t: vec![t],
        y: vec![y.clone()],
        f: vec![fy.clone()],
    };
    let mut steps = 0usize;
    let mut fails = 0usize;
    let mut nofailed = true;
    // Output points (tspan of two elements): steps plus Refine points.
    let mut out_t = vec![t];
    let mut out_y = vec![y.clone()];
    // Accepted Dormand-Prince steps for the continuous extension.
    let mut dense: Vec<Dense> = Vec::new();
    while (tf - t) * dir > 0.0 {
        if steps > 1_000_000 {
            return Err(err(
                "MATLAB:ode45:TooManySteps",
                format!("{}: too many steps.", name),
            ));
        }
        h = h.min(maxstep).max(hmin_at(t));
        // Stretch the step onto tfinal when it is within 10%.
        let last = 1.1 * h >= (tf - t).abs();
        if last {
            h = (tf - t).abs();
        }
        let hs = dir * h;
        let n = y.len();
        let (ynew, errv, fnew, kd) = match method {
            Method::Dp45 => {
                const C: [f64; 7] = [0.0, 0.2, 0.3, 0.8, 8.0 / 9.0, 1.0, 1.0];
                const A: [[f64; 6]; 7] = [
                    [0.0; 6],
                    [0.2, 0.0, 0.0, 0.0, 0.0, 0.0],
                    [3.0 / 40.0, 9.0 / 40.0, 0.0, 0.0, 0.0, 0.0],
                    [44.0 / 45.0, -56.0 / 15.0, 32.0 / 9.0, 0.0, 0.0, 0.0],
                    [
                        19372.0 / 6561.0,
                        -25360.0 / 2187.0,
                        64448.0 / 6561.0,
                        -212.0 / 729.0,
                        0.0,
                        0.0,
                    ],
                    [
                        9017.0 / 3168.0,
                        -355.0 / 33.0,
                        46732.0 / 5247.0,
                        49.0 / 176.0,
                        -5103.0 / 18656.0,
                        0.0,
                    ],
                    [
                        35.0 / 384.0,
                        0.0,
                        500.0 / 1113.0,
                        125.0 / 192.0,
                        -2187.0 / 6784.0,
                        11.0 / 84.0,
                    ],
                ];
                const E: [f64; 7] = [
                    71.0 / 57600.0,
                    0.0,
                    -71.0 / 16695.0,
                    71.0 / 1920.0,
                    -17253.0 / 339200.0,
                    22.0 / 525.0,
                    -1.0 / 40.0,
                ];
                let mut k: Vec<Vec<f64>> = vec![fy.clone()];
                for s in 1..7 {
                    let ys: Vec<f64> = (0..n)
                        .map(|i| y[i] + hs * (0..s).map(|j| A[s][j] * k[j][i]).sum::<f64>())
                        .collect();
                    k.push(call_f(it, &f, t + C[s] * hs, &ys)?);
                }
                let ynew: Vec<f64> = (0..n)
                    .map(|i| y[i] + hs * (0..6).map(|j| A[6][j] * k[j][i]).sum::<f64>())
                    .collect();
                let e: Vec<f64> = (0..n)
                    .map(|i| hs * (0..7).map(|j| E[j] * k[j][i]).sum::<f64>())
                    .collect();
                (ynew, e, k[6].clone(), Some(k))
            }
            Method::Bs23 => {
                let k1 = fy.clone();
                let y2: Vec<f64> = (0..n).map(|i| y[i] + hs * 0.5 * k1[i]).collect();
                let k2 = call_f(it, &f, t + 0.5 * hs, &y2)?;
                let y3: Vec<f64> = (0..n).map(|i| y[i] + hs * 0.75 * k2[i]).collect();
                let k3 = call_f(it, &f, t + 0.75 * hs, &y3)?;
                let ynew: Vec<f64> = (0..n)
                    .map(|i| {
                        y[i] + hs * (2.0 / 9.0 * k1[i] + 1.0 / 3.0 * k2[i] + 4.0 / 9.0 * k3[i])
                    })
                    .collect();
                let k4 = call_f(it, &f, t + hs, &ynew)?;
                let e: Vec<f64> = (0..n)
                    .map(|i| {
                        hs * (-5.0 / 72.0 * k1[i] + 1.0 / 12.0 * k2[i] + 1.0 / 9.0 * k3[i]
                            - 1.0 / 8.0 * k4[i])
                    })
                    .collect();
                (ynew, e, k4, None)
            }
        };
        let errn = (0..n)
            .map(|i| (errv[i] / (rtol * y[i].abs().max(ynew[i].abs())).max(atol[i])).abs())
            .fold(0.0, f64::max);
        if !errn.is_finite() {
            return Err(err(
                "MATLAB:ode45:IntegrationTolNotMet",
                format!("{}: failure at t={}; non-finite values.", name, t),
            ));
        }
        if errn <= 1.0 {
            let tnew = if last { tf } else { t + hs };
            let hs = tnew - t;
            let step = Dense {
                t,
                h: hs,
                y: y.clone(),
                k: kd,
                y1: ynew.clone(),
                f0: fy.clone(),
                f1: fnew.clone(),
            };
            // Refine points inside the step (continuous extension).
            for r in 1..refine {
                let tr = t + hs * r as f64 / refine as f64;
                out_t.push(tr);
                out_y.push(step.eval(tr));
            }
            t = tnew;
            y = ynew;
            fy = fnew;
            mesh.t.push(t);
            mesh.y.push(y.clone());
            mesh.f.push(fy.clone());
            out_t.push(t);
            out_y.push(y.clone());
            dense.push(step);
            steps += 1;
            // No increase right after a failed attempt.
            if nofailed {
                let temp = 1.25 * errn.powf(pow);
                h = if temp > 0.2 { h / temp } else { 5.0 * h };
            }
            nofailed = true;
        } else {
            fails += 1;
            if h <= hmin_at(t) {
                return Err(err(
                    "MATLAB:ode45:IntegrationTolNotMet",
                    format!(
                        "{}: unable to meet integration tolerances at t={}.",
                        name, t
                    ),
                ));
            }
            let floor = if order > 4.0 { 0.1 } else { 0.5 };
            h = if nofailed {
                h * (0.8 * errn.powf(-pow)).max(floor)
            } else {
                0.5 * h
            };
            h = h.max(hmin_at(t));
            nofailed = false;
        }
    }
    let n = y0.len();
    if nargout <= 1 {
        let m = mesh.t.len();
        let mut s = StructArray::new_scalar();
        s.set(0, "solver", Value::str(name));
        s.set(0, "x", Value::row(mesh.t.clone()));
        let mut ym = vec![0.0; n * m];
        let mut fm = vec![0.0; n * m];
        for j in 0..m {
            for i in 0..n {
                ym[i + j * n] = mesh.y[j][i];
                fm[i + j * n] = mesh.f[j][i];
            }
        }
        s.set(0, "y", Value::matrix(vec![n, m], ym));
        let mut idata = StructArray::new_scalar();
        idata.set(0, "f", Value::matrix(vec![n, m], fm));
        s.set(0, "idata", Value::Struct(idata));
        let mut stats = StructArray::new_scalar();
        stats.set(0, "nsteps", Value::num(steps as f64));
        stats.set(0, "nfailed", Value::num(fails as f64));
        s.set(0, "stats", Value::Struct(stats));
        return one(Value::Struct(s));
    }
    let (tout, yout): (Vec<f64>, Vec<Vec<f64>>) = if tspan.len() > 2 {
        let yv = tspan
            .iter()
            .map(|&x| {
                // Step containing x (the last one for x == tfinal).
                let k = dense
                    .iter()
                    .position(|d| (x - (d.t + d.h)) * dir <= 0.0)
                    .unwrap_or(dense.len().saturating_sub(1));
                match dense.get(k) {
                    Some(d) if x == d.t + d.h => d.y1.clone(),
                    Some(d) if x != t0 => d.eval(x),
                    _ => y0.clone(),
                }
            })
            .collect();
        (tspan.clone(), yv)
    } else {
        (out_t, out_y)
    };
    let m = tout.len();
    let mut ym = vec![0.0; m * n];
    for (j, row) in yout.iter().enumerate() {
        for i in 0..n {
            ym[j + i * m] = row[i];
        }
    }
    Ok(vec![Value::col(tout), Value::matrix(vec![m, n], ym)])
}

fn deval(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "deval")?;
    let (sol, x) = if matches!(a[0], Value::Struct(_)) {
        (&a[0], &a[1])
    } else {
        (&a[1], &a[0])
    };
    let Value::Struct(s) = sol else {
        return Err(err(
            "MATLAB:deval:SolNotFromSolver",
            "deval: first argument must be a solution structure.",
        ));
    };
    let mt = s
        .get(0, "x")
        .and_then(|v| v.to_f64_vec())
        .unwrap_or_default();
    let ym = s
        .get(0, "y")
        .and_then(|v| v.to_num())
        .ok_or_else(|| err("MATLAB:deval:SolNotFromSolver", "deval: solution has no y."))?;
    let fm = match s.get(0, "idata") {
        Some(Value::Struct(i)) => i.get(0, "f").and_then(|v| v.to_num()),
        _ => None,
    }
    .ok_or_else(|| {
        err(
            "MATLAB:deval:SolNotFromSolver",
            "deval: solution has no interpolation data.",
        )
    })?;
    let n = ym.dims[0];
    let m = mt.len();
    if m == 0 {
        return Err(err(
            "MATLAB:deval:SolNotFromSolver",
            "deval: empty solution.",
        ));
    }
    let col = |d: &Num, j: usize| (0..n).map(|i| d.re[i + j * n]).collect::<Vec<f64>>();
    let mesh = Mesh {
        t: mt.clone(),
        y: (0..m).map(|j| col(&ym, j)).collect(),
        f: (0..m).map(|j| col(&fm, j)).collect(),
    };
    let xs = arg_num(x, "deval")?.re;
    let idx: Vec<usize> = match a.get(2) {
        Some(v) => v
            .to_f64_vec()
            .unwrap_or_default()
            .iter()
            .map(|k| *k as usize - 1)
            .collect(),
        None => (0..n).collect(),
    };
    let (lo, hi) = (mt[0].min(mt[m - 1]), mt[0].max(mt[m - 1]));
    for xv in &xs {
        if *xv < lo - 1e-12 * (hi - lo).abs().max(1.0)
            || *xv > hi + 1e-12 * (hi - lo).abs().max(1.0)
        {
            return Err(err(
                "MATLAB:deval:SolOutsideInterval",
                format!(
                    "Attempting to evaluate the solution outside the interval [{}, {}] where it is defined.",
                    lo, hi
                ),
            ));
        }
    }
    let k = idx.len();
    let mut v = vec![0.0; k * xs.len()];
    let mut d = vec![0.0; k * xs.len()];
    for (j, xv) in xs.iter().enumerate() {
        let yv = hermite(&mesh, *xv, false);
        let dv = hermite(&mesh, *xv, true);
        for (r, &i) in idx.iter().enumerate() {
            if i >= n {
                return Err(err(
                    "MATLAB:deval:InvalidIDX",
                    "deval: component index out of range.",
                ));
            }
            v[r + j * k] = yv[i];
            d[r + j * k] = dv[i];
        }
    }
    let out = vec![
        Value::matrix(vec![k, xs.len()], v),
        Value::matrix(vec![k, xs.len()], d),
    ];
    let _ = arg_f64;
    let _ = Array::<f64>::scalar;
    Ok(out.into_iter().take(nargout.max(1)).collect())
}
