//! Time responses (`step`, `impulse`, `initial`, `lsim`), frequency
//! responses (`bode`, `freqresp`, `evalfr`, `nyquist`) and
//! discretisation (`c2d`, `d2c`).
//!
//! Continuous simulation is exact for piecewise-constant (ZOH) or
//! piecewise-linear (FOH) inputs: the model is discretised once with the
//! matrix exponential of an augmented matrix and then stepped.

use super::model::{Data, Kind, Ss, Sys};
use super::num::{C, Mat, c, expm, logm, pscale, psubst_bilinear, ptrim};
use super::ops::not_supported;
use crate::builtins::err;
use crate::{Interp, R};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hold {
    Zoh,
    Foh,
}

/// Simulation result: `y[k]` (ny values), `x[k]` (nx values).
pub struct Sim {
    pub y: Vec<Vec<f64>>,
    pub x: Vec<Vec<f64>>,
}

/// Exact ZOH (and FOH) discretisation of `(A, B)` over `dt`:
/// `x1 = Phi x0 + G1 u0 + G2 (u1 - u0)`.
pub fn discretize(it: &mut Interp, m: &Ss, dt: f64, hold: Hold) -> R<(Mat, Mat, Mat)> {
    let (nx, nu) = (m.nx(), m.nu());
    let size = nx + nu + if hold == Hold::Foh { nu } else { 0 };
    let mut f = Mat::zeros(size, size);
    for j in 0..nx {
        for i in 0..nx {
            f.set(i, j, m.a.at(i, j) * dt);
        }
    }
    for j in 0..nu {
        for i in 0..nx {
            f.set(i, nx + j, m.b.at(i, j) * dt);
        }
    }
    if hold == Hold::Foh {
        for j in 0..nu {
            f.set(nx + j, nx + nu + j, 1.0);
        }
    }
    let e = expm(it, &f)?;
    let phi = e.block(0, nx, 0, nx);
    let g1 = e.block(0, nx, nx, nx + nu);
    let g2 = if hold == Hold::Foh {
        e.block(0, nx, nx + nu, nx + 2 * nu)
    } else {
        Mat::zeros(nx, nu)
    };
    Ok((phi, g1, g2))
}

fn mv(m: &Mat, x: &[f64]) -> Vec<f64> {
    let mut r = vec![0.0; m.r];
    for j in 0..m.c {
        let v = x[j];
        if v == 0.0 {
            continue;
        }
        for i in 0..m.r {
            r[i] += m.at(i, j) * v;
        }
    }
    r
}

fn vadd(a: &mut [f64], b: &[f64]) {
    for (x, y) in a.iter_mut().zip(b) {
        *x += y;
    }
}

/// Step `n` samples of a discrete recursion (`phi, g1, g2` from
/// [`discretize`] or the discrete model itself with `g2 = 0`).
fn run(
    m: &Ss,
    phi: &Mat,
    g1: &Mat,
    g2: &Mat,
    u: &dyn Fn(usize) -> Vec<f64>,
    x0: &[f64],
    n: usize,
) -> Sim {
    let mut x = x0.to_vec();
    let mut ys = Vec::with_capacity(n);
    let mut xs = Vec::with_capacity(n);
    for k in 0..n {
        let uk = u(k);
        let mut y = mv(&m.c, &x);
        vadd(&mut y, &mv(&m.d, &uk));
        ys.push(y);
        xs.push(x.clone());
        if k + 1 < n {
            let mut nx = mv(phi, &x);
            vadd(&mut nx, &mv(g1, &uk));
            if g2.c > 0 && g2.r > 0 {
                let u1 = u(k + 1);
                let du: Vec<f64> = u1.iter().zip(&uk).map(|(a, b)| a - b).collect();
                vadd(&mut nx, &mv(g2, &du));
            }
            x = nx;
        }
    }
    Sim { y: ys, x: xs }
}

/// Spacing of a uniform time vector.
pub fn uniform_dt(t: &[f64]) -> R<f64> {
    if t.len() < 2 {
        return Ok(0.0);
    }
    let dt = (t[t.len() - 1] - t[0]) / (t.len() - 1) as f64;
    if dt <= 0.0 || !dt.is_finite() {
        return Err(err(
            "Control:analysis:lsim4",
            "The time vector must be increasing.",
        ));
    }
    for w in t.windows(2) {
        if ((w[1] - w[0]) - dt).abs() > 1e-6 * dt.max(1e-300) * 1e3 {
            return Err(err(
                "Control:analysis:lsim4",
                "The time vector must be evenly spaced.",
            ));
        }
    }
    Ok(dt)
}

/// General linear simulation: input samples `u[k]` (nu each) at times
/// `t`, initial state `x0`.
pub fn lsim(
    it: &mut Interp,
    sys: &Sys,
    t: &[f64],
    u: &[Vec<f64>],
    x0: &[f64],
    hold: Hold,
) -> R<Sim> {
    let m = sys.ss_data(it)?;
    let n = t.len();
    let delay = sys.input_delay();
    let dt = uniform_dt(t)?;
    let dt = if n < 2 {
        if sys.is_discrete() { sys.ts.abs() } else { 1.0 }
    } else {
        dt
    };
    let (phi, g1, g2) = if sys.is_discrete() {
        if sys.ts > 0.0 && n >= 2 && (dt - sys.ts).abs() > 1e-9 * sys.ts.max(1.0) {
            return Err(err(
                "Control:analysis:lsim2",
                format!(
                    "For discrete-time models, the time vector must be spaced by the sample time Ts = {}.",
                    sys.ts
                ),
            ));
        }
        (m.a.clone(), m.b.clone(), Mat::zeros(0, 0))
    } else {
        discretize(it, &m, dt, hold)?
    };
    // Input delay as a whole number of samples.
    let shift = if delay == 0.0 {
        0
    } else {
        let s = if sys.is_discrete() { delay } else { delay / dt };
        if (s - s.round()).abs() > 1e-9 * s.abs().max(1.0) {
            return Err(not_supported(
                "Simulating a time delay that is not a multiple of the time step",
            ));
        }
        s.round() as usize
    };
    let nu = m.nu();
    let uf = |k: usize| -> Vec<f64> {
        if k < shift {
            vec![0.0; nu]
        } else {
            u.get(k - shift).cloned().unwrap_or_else(|| vec![0.0; nu])
        }
    };
    Ok(run(&m, &phi, &g1, &g2, &uf, x0, n))
}

/// Response to a constant input `u` (or zero input with state `x0`) at
/// uniform times `t` (any start >= 0); the input starts at time `delay`.
fn const_response(
    it: &mut Interp,
    sys: &Sys,
    m: &Ss,
    t: &[f64],
    u: &[f64],
    x0: &[f64],
    impulse_ts: bool,
) -> R<Sim> {
    let n = t.len();
    let delay = sys.input_delay();
    let (nx, ny) = (m.nx(), m.ny());
    if sys.is_discrete() {
        let ts = if sys.ts > 0.0 { sys.ts } else { 1.0 };
        let idx: Vec<usize> = t
            .iter()
            .map(|x| {
                let k = x / ts;
                if (k - k.round()).abs() > 1e-6 || k < -1e-9 {
                    None
                } else {
                    Some(k.round() as usize)
                }
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                err(
                    "Control:analysis:step2",
                    "For discrete-time models, the time vector must contain multiples of the sample time.",
                )
            })?;
        let last = idx.iter().copied().max().unwrap_or(0) + 1;
        let shift = delay.round() as usize;
        let uu = u.to_vec();
        let nu = m.nu();
        let uf = move |k: usize| -> Vec<f64> {
            if k < shift {
                vec![0.0; nu]
            } else if impulse_ts {
                if k == shift {
                    uu.clone()
                } else {
                    vec![0.0; nu]
                }
            } else {
                uu.clone()
            }
        };
        let sim = run(m, &m.a, &m.b, &Mat::zeros(0, 0), &uf, x0, last);
        return Ok(Sim {
            y: idx.iter().map(|k| sim.y[*k].clone()).collect(),
            x: idx.iter().map(|k| sim.x[*k].clone()).collect(),
        });
    }
    let dt = uniform_dt(t)?;
    let mut ys = vec![vec![0.0; ny]; n];
    let mut xs = vec![vec![0.0; nx]; n];
    // First sample at or after the delay.
    let first = t.iter().position(|x| *x - delay >= -1e-12);
    let Some(first) = first else {
        return Ok(Sim { y: ys, x: xs });
    };
    for k in 0..first {
        // Before the input acts: free response of x0 (zero for step).
        let s0 = t[k];
        let (phi, _, _) = discretize(it, m, s0, Hold::Zoh)?;
        let x = mv(&phi, x0);
        ys[k] = mv(&m.c, &x);
        xs[k] = x;
    }
    let s0 = (t[first] - delay).max(0.0);
    let (phi0, g0, _) = discretize(it, m, s0, Hold::Zoh)?;
    let mut x = mv(&phi0, x0);
    vadd(&mut x, &mv(&g0, u));
    let (phi, g1, _) = if n - first >= 2 {
        discretize(it, m, dt, Hold::Zoh)?
    } else {
        (Mat::eye(nx), Mat::zeros(nx, m.nu()), Mat::zeros(0, 0))
    };
    for k in first..n {
        let mut y = mv(&m.c, &x);
        vadd(&mut y, &mv(&m.d, u));
        ys[k] = y;
        xs[k] = x.clone();
        let mut nxv = mv(&phi, &x);
        vadd(&mut nxv, &mv(&g1, u));
        x = nxv;
    }
    Ok(Sim { y: ys, x: xs })
}

/// Step response of input channel `j`.
pub fn step(it: &mut Interp, sys: &Sys, t: &[f64], j: usize) -> R<Sim> {
    let m = sys.ss_data(it)?;
    let mut u = vec![0.0; m.nu()];
    u[j] = 1.0;
    let x0 = vec![0.0; m.nx()];
    const_response(it, sys, &m, t, &u, &x0, false)
}

/// Impulse response of input channel `j` (continuous: `C e^{At} B`,
/// the direct feedthrough impulse is not shown, as in MATLAB; discrete:
/// response to a pulse of area 1, height `1/Ts`).
pub fn impulse(it: &mut Interp, sys: &Sys, t: &[f64], j: usize) -> R<Sim> {
    let m = sys.ss_data(it)?;
    if sys.is_discrete() {
        let ts = if sys.ts > 0.0 { sys.ts } else { 1.0 };
        let mut u = vec![0.0; m.nu()];
        u[j] = 1.0 / ts;
        let x0 = vec![0.0; m.nx()];
        return const_response(it, sys, &m, t, &u, &x0, true);
    }
    let x0: Vec<f64> = (0..m.nx()).map(|i| m.b.at(i, j)).collect();
    let free = Ss {
        d: Mat::zeros(m.ny(), m.nu()),
        ..m.clone()
    };
    let delay = sys.input_delay();
    if delay == 0.0 {
        let u = vec![0.0; m.nu()];
        return const_response(it, sys, &free, t, &u, &x0, false);
    }
    // Shift: y(t) = h(t - delay).
    let shifted: Vec<f64> = t.iter().map(|x| x - delay).collect();
    let pos = shifted.iter().position(|x| *x >= -1e-12).unwrap_or(t.len());
    let mut nodelay = sys.clone();
    nodelay.extra.retain(|(n, _)| !n.ends_with("Delay"));
    let u = vec![0.0; m.nu()];
    let tail: Vec<f64> = shifted[pos..].iter().map(|x| x.max(0.0)).collect();
    let mut sim = Sim {
        y: vec![vec![0.0; m.ny()]; pos],
        x: vec![vec![0.0; m.nx()]; pos],
    };
    if !tail.is_empty() {
        let r = const_response(it, &nodelay, &free, &tail, &u, &x0, false)?;
        sim.y.extend(r.y);
        sim.x.extend(r.x);
    }
    Ok(sim)
}

pub fn initial(it: &mut Interp, sys: &Sys, t: &[f64], x0: &[f64]) -> R<Sim> {
    let m = sys.ss_data(it)?;
    if x0.len() != m.nx() {
        return Err(err(
            "Control:analysis:initial1",
            format!(
                "The initial condition must be a vector with {} entries.",
                m.nx()
            ),
        ));
    }
    let u = vec![0.0; m.nu()];
    let mut nodelay = sys.clone();
    nodelay.extra.retain(|(n, _)| !n.ends_with("Delay"));
    const_response(it, &nodelay, &m, t, &u, x0, false)
}

// ---------------------------------------------------------------------------
// Default time and frequency grids
// ---------------------------------------------------------------------------

fn nice_ceil(x: f64) -> f64 {
    if x <= 0.0 || !x.is_finite() {
        return 1.0;
    }
    let p = 10f64.powf(x.log10().floor());
    for m in [
        1.0, 1.2, 1.4, 1.5, 1.6, 1.8, 2.0, 2.5, 3.0, 3.5, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0,
    ] {
        if m * p >= x * (1.0 - 1e-12) {
            return m * p;
        }
    }
    10.0 * p
}

/// Simulation horizon and time vector chosen from the poles.
pub fn default_time(it: &mut Interp, sys: &Sys, impulse_like: bool) -> R<Vec<f64>> {
    let poles = sys.poles(it)?;
    let delay = sys.input_delay();
    if sys.is_discrete() {
        let ts = if sys.ts > 0.0 { sys.ts } else { 1.0 };
        let mut nmax: f64 = 10.0;
        for p in &poles {
            let r = p.abs();
            if r == 0.0 {
                continue;
            }
            let sigma = -r.ln();
            if sigma > 1e-9 {
                nmax = nmax.max(7.0 / sigma);
            } else if sigma < -1e-9 {
                nmax = nmax.max(5.0 / -sigma);
            } else {
                nmax = nmax.max(50.0);
            }
        }
        let n = (nmax.ceil() + delay).min(10000.0) as usize;
        return Ok((0..=n).map(|k| k as f64 * ts).collect());
    }
    let mut tf: f64 = 0.0;
    let mut wmax: f64 = 0.0;
    let mut marginal = false;
    for p in &poles {
        let s = -p.re;
        wmax = wmax.max(p.abs());
        if s > 1e-9 * (1.0 + p.abs()) {
            tf = tf.max(7.0 / s);
        } else if s < -1e-9 * (1.0 + p.abs()) {
            tf = tf.max(5.0 / -s);
        } else {
            marginal = true;
            if p.im.abs() > 0.0 {
                tf = tf.max(10.0 * 2.0 * std::f64::consts::PI / p.im.abs());
            }
        }
        if p.im.abs() > 1e-9 {
            tf = tf.max(3.0 * 2.0 * std::f64::consts::PI / p.im.abs());
        }
    }
    if tf == 0.0 {
        tf = if marginal { 10.0 } else { 1.0 };
    }
    let _ = impulse_like;
    let tf = nice_ceil(tf + delay);
    let mut n = 1000usize;
    if wmax > 0.0 {
        // At least ~15 samples per fastest time constant.
        let need = (tf * wmax * 15.0 / (2.0 * std::f64::consts::PI)).ceil() as usize;
        n = n.max(need).min(20000);
    }
    let dt = tf / n as f64;
    Ok((0..=n).map(|k| k as f64 * dt).collect())
}

/// Logarithmic frequency grid covering the dynamics.
pub fn default_freqs(it: &mut Interp, sys: &Sys) -> R<Vec<f64>> {
    let mut mags: Vec<f64> = Vec::new();
    let mut roots = sys.poles(it)?;
    if sys.size() == (1, 1) {
        roots.extend(sys.zeros(it)?);
    }
    for r in roots {
        let w = if sys.is_discrete() {
            let ts = if sys.ts > 0.0 { sys.ts } else { 1.0 };
            if r.abs() == 0.0 {
                continue;
            }
            r.ln().abs() / ts
        } else {
            r.abs()
        };
        if w > 0.0 && w.is_finite() {
            mags.push(w);
        }
    }
    let (lo, mut hi) = if mags.is_empty() {
        (-1.0, 1.0)
    } else {
        let mn = mags.iter().cloned().fold(f64::INFINITY, f64::min);
        let mx = mags.iter().cloned().fold(0.0, f64::max);
        (mn.log10().floor() - 1.0, mx.log10().ceil() + 1.0)
    };
    if sys.is_discrete() {
        let ts = if sys.ts > 0.0 { sys.ts } else { 1.0 };
        hi = (std::f64::consts::PI / ts).log10();
    }
    let lo = lo.min(hi - 1.0);
    let n = (((hi - lo) * 100.0).ceil() as usize).clamp(100, 2000);
    Ok((0..n)
        .map(|k| 10f64.powf(lo + (hi - lo) * k as f64 / (n - 1) as f64))
        .collect())
}

/// Frequency response at real frequencies `w` (rad/time unit): one
/// ny×nu block per frequency.
pub fn freqresp(it: &mut Interp, sys: &Sys, w: &[f64]) -> R<Vec<Vec<C>>> {
    let mut out = Vec::with_capacity(w.len());
    // Evaluate via tf for pid objects only once.
    let base = if sys.kind.rank() == 0 {
        sys.convert(it, Kind::Tf)?
    } else {
        sys.clone()
    };
    for &wk in w {
        let s = if base.is_discrete() {
            let ts = if base.ts > 0.0 { base.ts } else { 1.0 };
            c(0.0, wk * ts).exp()
        } else {
            c(0.0, wk)
        };
        out.push(base.eval_at(it, s)?);
    }
    Ok(out)
}

/// Unwrapped phase in degrees.
pub fn phase_deg(h: &[C]) -> Vec<f64> {
    let mut out = Vec::with_capacity(h.len());
    let mut prev: Option<f64> = None;
    for z in h {
        let mut a = z.arg();
        if let Some(p) = prev {
            while a - p > std::f64::consts::PI {
                a -= 2.0 * std::f64::consts::PI;
            }
            while a - p < -std::f64::consts::PI {
                a += 2.0 * std::f64::consts::PI;
            }
        }
        prev = Some(a);
        out.push(a.to_degrees());
    }
    out
}

// ---------------------------------------------------------------------------
// Discretisation
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Method {
    Zoh,
    Foh,
    Tustin(Option<f64>),
    Impulse,
    Matched,
}

pub fn parse_method(s: &str, prewarp: Option<f64>) -> R<Method> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "zoh" => Method::Zoh,
        "foh" => Method::Foh,
        "tustin" | "bilinear" => Method::Tustin(prewarp),
        "prewarp" => Method::Tustin(Some(prewarp.ok_or_else(|| {
            err(
                "Control:transformation:c2d3",
                "The 'prewarp' method needs a prewarp frequency.",
            )
        })?)),
        "impulse" | "imp" => Method::Impulse,
        "matched" => Method::Matched,
        "least-squares" | "least" => {
            return Err(not_supported("The 'least-squares' discretisation method"));
        }
        other => {
            return Err(err(
                "Control:transformation:c2d3",
                format!("Unknown discretization method '{}'.", other),
            ));
        }
    })
}

fn tustin_alpha(ts: f64, prewarp: Option<f64>) -> f64 {
    match prewarp {
        Some(w) if w > 0.0 => w / (w * ts / 2.0).tan(),
        _ => 2.0 / ts,
    }
}

fn tf_result(it: &mut Interp, kind: Kind, num: &[f64], den: &[f64], ts: f64) -> R<Sys> {
    let d = ptrim(den);
    let lead = d[0];
    let n: Vec<f64> = num.iter().map(|x| x / lead).collect();
    let d: Vec<f64> = den.iter().map(|x| x / lead).collect();
    let s = Sys::tf(&n, &d, ts);
    s.convert(it, kind)
}

pub fn c2d(it: &mut Interp, sys: &Sys, ts: f64, method: Method) -> R<Sys> {
    if sys.is_discrete() {
        return Err(err(
            "Control:transformation:c2d1",
            "The model is already discrete (use d2d to resample).",
        ));
    }
    if !(ts > 0.0 && ts.is_finite()) {
        return Err(err(
            "Control:transformation:c2d2",
            "The sample time must be a positive real number.",
        ));
    }
    let kind = if sys.kind.rank() == 0 {
        Kind::Tf
    } else {
        sys.kind
    };
    let delay = sys.input_delay();
    let samples = delay / ts;
    if (samples - samples.round()).abs() > 1e-9 * samples.max(1.0) {
        return Err(not_supported(
            "Discretising a time delay that is not a multiple of the sample time",
        ));
    }
    let mut out = match method {
        Method::Tustin(pw) if kind != Kind::Ss => {
            let (num, den) = sys.tf_data(it)?;
            let a = tustin_alpha(ts, pw);
            let n = num.len().max(den.len()) - 1;
            let num = psubst_bilinear(&num, a, -a, 1.0, 1.0, n);
            let den = psubst_bilinear(&den, a, -a, 1.0, 1.0, n);
            tf_result(it, kind, &num, &den, ts)?
        }
        Method::Matched => {
            let (z, p, k) = sys.zpk_data(it)?;
            if sys.size() != (1, 1) {
                return Err(not_supported("Matched discretisation of MIMO models"));
            }
            let mut zd: Vec<C> = z.iter().map(|x| x.scale(ts).exp()).collect();
            // Zeros at infinity map to z = -1, all but one (one-sample delay kept).
            for _ in (z.len() + 1)..p.len() {
                zd.push(C::real(-1.0));
            }
            let pd: Vec<C> = p.iter().map(|x| x.scale(ts).exp()).collect();
            // Match the gain at s = 0 <-> z = 1 (or at a nearby point
            // when there is a pole or zero at the origin).
            let mut s0 = C::default();
            let at = |zz: &[C], pp: &[C], kk: C, x: C| {
                let mut v = kk;
                for r in zz {
                    v = v.mul(x.sub(*r));
                }
                for r in pp {
                    v = v.div(x.sub(*r));
                }
                v
            };
            let near0 = z.iter().chain(&p).any(|r| r.abs() < 1e-12);
            if near0 {
                s0 = c(0.0, 1e-3 / ts);
            }
            let hc = at(&z, &p, k, s0);
            let hd = at(&zd, &pd, C::real(1.0), s0.scale(ts).exp());
            let kd = hc.div(hd);
            let zs = Sys::new(
                Kind::Zpk,
                Data::Zpk {
                    z: zd,
                    p: pd,
                    k: C::real(kd.re),
                },
                ts,
            );
            zs.convert(it, kind)?
        }
        _ => {
            let m = sys.ss_data(it)?;
            let md = match method {
                Method::Zoh => {
                    let (phi, g1, _) = discretize(it, &m, ts, Hold::Zoh)?;
                    Ss {
                        a: phi,
                        b: g1,
                        ..m.clone()
                    }
                }
                Method::Foh => {
                    let (phi, g1, g2) = discretize(it, &m, ts, Hold::Foh)?;
                    // xi = x - G2 u: xi1 = Phi xi + (Phi G2 + G1 - G2) u.
                    Ss {
                        b: phi.mul(&g2).add(&g1).sub(&g2),
                        c: m.c.clone(),
                        d: m.d.add(&m.c.mul(&g2)),
                        a: phi,
                    }
                }
                Method::Impulse => {
                    if m.d.d.iter().any(|x| *x != 0.0) {
                        return Err(not_supported(
                            "Impulse-invariant discretisation of models with direct feedthrough",
                        ));
                    }
                    let (phi, _, _) = discretize(it, &m, ts, Hold::Zoh)?;
                    Ss {
                        b: phi.mul(&m.b).scale(ts),
                        d: m.c.mul(&m.b).scale(ts),
                        c: m.c.clone(),
                        a: phi,
                    }
                }
                Method::Tustin(pw) => {
                    let a = tustin_alpha(ts, pw);
                    let t2 = 2.0 / a;
                    let n = m.nx();
                    let im = Mat::eye(n).sub(&m.a.scale(t2 / 2.0));
                    let ip = Mat::eye(n).add(&m.a.scale(t2 / 2.0));
                    let inv = im.inv().ok_or_else(|| {
                        err(
                            "Control:transformation:c2d4",
                            "Tustin discretisation is singular (pole at s = 2/Ts).",
                        )
                    })?;
                    let st = t2.sqrt();
                    Ss {
                        a: inv.mul(&ip),
                        b: inv.mul(&m.b).scale(st),
                        c: m.c.mul(&inv).scale(st),
                        d: m.d.add(&m.c.mul(&inv).mul(&m.b).scale(t2 / 2.0)),
                    }
                }
                Method::Matched => unreachable!(),
            };
            let s = Sys::new(Kind::Ss, Data::Ss(md), ts);
            let s = s.convert(it, kind)?;
            if kind == Kind::Tf
                && let Data::Tf { num, den } = &s.data
            {
                // Monic denominator as MATLAB displays it.
                tf_result(it, Kind::Tf, num, den, ts)?
            } else {
                s
            }
        }
    };
    out.extra = sys
        .extra
        .iter()
        .filter(|(n, _)| !n.ends_with("Delay") && n != "Variable")
        .cloned()
        .collect();
    if delay != 0.0 {
        out.set_extra("InputDelay", indy_value::Value::num(samples.round()));
    }
    Ok(out)
}

pub fn d2c(it: &mut Interp, sys: &Sys, method: Method) -> R<Sys> {
    if !sys.is_discrete() {
        return Err(err(
            "Control:transformation:d2c1",
            "The model is already continuous.",
        ));
    }
    let ts = if sys.ts > 0.0 { sys.ts } else { 1.0 };
    let kind = if sys.kind.rank() == 0 {
        Kind::Tf
    } else {
        sys.kind
    };
    let mut out = match method {
        Method::Tustin(pw) if kind != Kind::Ss => {
            let (num, den) = sys.tf_data(it)?;
            let a = tustin_alpha(ts, pw);
            let n = num.len().max(den.len()) - 1;
            // z = (a + s)/(a - s)
            let num = psubst_bilinear(&num, 1.0, a, -1.0, a, n);
            let den = psubst_bilinear(&den, 1.0, a, -1.0, a, n);
            tf_result(it, kind, &num, &den, 0.0)?
        }
        Method::Zoh => {
            let m = sys.ss_data(it)?;
            let (nx, nu) = (m.nx(), m.nu());
            let big = m.a.hcat(&m.b).vcat(&Mat::zeros(nu, nx).hcat(&Mat::eye(nu)));
            let l = logm(&big).ok_or_else(|| {
                err(
                    "Control:transformation:d2c2",
                    "The ZOH inverse does not exist (the model has a pole at z = 0 or on the negative real axis).",
                )
            })?;
            if super::num::eigvals(it, &m.a)?
                .iter()
                .any(|z| z.abs() < 1e-12 || (z.im.abs() < 1e-12 && z.re < 0.0))
            {
                return Err(err(
                    "Control:transformation:d2c2",
                    "The ZOH inverse does not exist (the model has a pole at z = 0 or on the negative real axis).",
                ));
            }
            let l = l.scale(1.0 / ts);
            let mc = Ss {
                a: l.block(0, nx, 0, nx),
                b: l.block(0, nx, nx, nx + nu),
                c: m.c.clone(),
                d: m.d.clone(),
            };
            let s = Sys::new(Kind::Ss, Data::Ss(mc), 0.0).convert(it, kind)?;
            if let Data::Tf { num, den } = &s.data {
                tf_result(it, Kind::Tf, num, den, 0.0)?
            } else {
                s
            }
        }
        Method::Tustin(pw) => {
            let a = tustin_alpha(ts, pw);
            let m = sys.ss_data(it)?;
            // Inverse of the c2d formula with T = 2/a.
            let t2 = 2.0 / a;
            let n = m.nx();
            let ip = Mat::eye(n).add(&m.a);
            let inv = ip.inv().ok_or_else(|| {
                err(
                    "Control:transformation:d2c3",
                    "Tustin inverse is singular (pole at z = -1).",
                )
            })?;
            let st = t2.sqrt();
            let ac = m.a.sub(&Mat::eye(n)).mul(&inv).scale(2.0 / t2);
            let bc = inv.mul(&m.b).scale(2.0 / st);
            let cc = m.c.mul(&inv).scale(2.0 / st);
            let dc = m.d.sub(&m.c.mul(&inv).mul(&m.b));
            Sys::new(
                Kind::Ss,
                Data::Ss(Ss {
                    a: ac,
                    b: bc,
                    c: cc,
                    d: dc,
                }),
                0.0,
            )
        }
        Method::Matched => {
            let (z, p, k) = sys.zpk_data(it)?;
            let zc: Vec<C> = z.iter().map(|x| x.ln().scale(1.0 / ts)).collect();
            let pc: Vec<C> = p.iter().map(|x| x.ln().scale(1.0 / ts)).collect();
            let at = |zz: &[C], pp: &[C], kk: C, x: C| {
                let mut v = kk;
                for r in zz {
                    v = v.mul(x.sub(*r));
                }
                for r in pp {
                    v = v.div(x.sub(*r));
                }
                v
            };
            let hd = at(&z, &p, k, C::real(1.0));
            let hc = at(&zc, &pc, C::real(1.0), C::default());
            let s = Sys::new(
                Kind::Zpk,
                Data::Zpk {
                    z: zc,
                    p: pc,
                    k: C::real(hd.div(hc).re),
                },
                0.0,
            );
            s.convert(it, kind)?
        }
        _ => return Err(not_supported("This d2c method")),
    };
    out.ts = 0.0;
    Ok(out)
}

/// Padé approximation of `exp(-T s)` of order `n`: (num, den).
pub fn pade(t: f64, n: usize) -> (Vec<f64>, Vec<f64>) {
    // Coefficients of s^k: c_k = (2n-k)! n! / ((2n)! k! (n-k)!) T^k
    let fact = |m: usize| (1..=m).fold(1.0f64, |a, b| a * b as f64);
    let mut num = vec![0.0; n + 1];
    let mut den = vec![0.0; n + 1];
    for k in 0..=n {
        let ck =
            fact(2 * n - k) * fact(n) / (fact(2 * n) * fact(k) * fact(n - k)) * t.powi(k as i32);
        // descending order: index n-k
        den[n - k] = ck;
        num[n - k] = if k % 2 == 1 { -ck } else { ck };
    }
    // MATLAB normalises the denominator to be monic.
    let lead = den[0];
    (pscale(&num, 1.0 / lead), pscale(&den, 1.0 / lead))
}
