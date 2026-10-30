//! Model analysis and design: `dcgain`, `damp`, `stepinfo`, `margin`,
//! `bandwidth`, `isstable`, `ctrb`, `obsv`, `place`/`acker`, `lqr`,
//! `care`, `lyap`, `dlyap`, `gram`, `minreal`, `rlocus`.

use super::model::{Data, Kind, Sys};
use super::num::{
    C, Mat, c, padd, pconv, peval, poly_from_roots, positive_real_roots, proots, pscale,
    psubst_bilinear, ptrim,
};
use super::ops::not_supported;
use super::resp;
use crate::builtins::err;
use crate::{Interp, R};

pub fn dcgain(it: &mut Interp, sys: &Sys) -> R<Mat> {
    let (ny, nu) = sys.size();
    let s0 = if sys.is_discrete() {
        C::real(1.0)
    } else {
        C::default()
    };
    let mut nodelay = sys.clone();
    nodelay.extra.retain(|(n, _)| !n.ends_with("Delay"));
    let base = if sys.kind.rank() == 0 {
        nodelay.convert(it, Kind::Tf)?
    } else {
        nodelay
    };
    let h = match &base.data {
        Data::Tf { num, den } => {
            let (n0, d0) = (peval(num, s0).re, peval(den, s0).re);
            vec![if d0 == 0.0 {
                if n0 == 0.0 {
                    f64::NAN
                } else {
                    f64::INFINITY.copysign(n0)
                }
            } else {
                n0 / d0
            }]
        }
        Data::Ss(m) => {
            // D - C (A - s0 I)^-1 B
            let n = m.nx();
            let a = m.a.sub(&Mat::eye(n).scale(s0.re));
            match a.solve(&m.b) {
                Some(x) => m.d.sub(&m.c.mul(&x)).d,
                None if n > 0 => {
                    let h = super::model::ss_eval(m, s0);
                    h.iter()
                        .map(|z| if z.is_finite() { z.re } else { f64::INFINITY })
                        .collect()
                }
                None => m.d.d.clone(),
            }
        }
        _ => base
            .eval_at(it, s0)?
            .iter()
            .map(|z| if z.is_finite() { z.re } else { f64::INFINITY })
            .collect(),
    };
    Ok(Mat { r: ny, c: nu, d: h })
}

pub fn isstable(it: &mut Interp, sys: &Sys) -> R<bool> {
    let p = sys.poles(it)?;
    Ok(if sys.is_discrete() {
        p.iter().all(|z| z.abs() < 1.0)
    } else {
        p.iter().all(|z| z.re < 0.0)
    })
}

/// Natural frequencies, damping ratios and poles, sorted by frequency.
pub fn damp(it: &mut Interp, sys: &Sys) -> R<(Vec<f64>, Vec<f64>, Vec<C>)> {
    let mut p = sys.poles(it)?;
    let ts = if sys.ts > 0.0 { sys.ts } else { 1.0 };
    let cont = |z: C| {
        if sys.is_discrete() {
            z.ln().scale(1.0 / ts)
        } else {
            z
        }
    };
    p.sort_by(|a, b| {
        let (x, y) = (cont(*a).abs(), cont(*b).abs());
        x.partial_cmp(&y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.im.partial_cmp(&b.im).unwrap_or(std::cmp::Ordering::Equal))
    });
    let mut wn = Vec::new();
    let mut zeta = Vec::new();
    for z in &p {
        let s = cont(*z);
        let w = s.abs();
        wn.push(w);
        zeta.push(if w == 0.0 {
            -1.0f64.copysign(s.re).max(-1.0)
        } else {
            -s.re / w
        });
    }
    Ok((wn, zeta, p))
}

// ---------------------------------------------------------------------------
// stepinfo
// ---------------------------------------------------------------------------

pub struct StepInfo {
    pub rise: f64,
    pub transient: f64,
    pub settling: f64,
    pub smin: f64,
    pub smax: f64,
    pub overshoot: f64,
    pub undershoot: f64,
    pub peak: f64,
    pub peak_time: f64,
}

impl StepInfo {
    pub fn nan() -> StepInfo {
        StepInfo {
            rise: f64::NAN,
            transient: f64::NAN,
            settling: f64::NAN,
            smin: f64::NAN,
            smax: f64::NAN,
            overshoot: f64::NAN,
            undershoot: f64::NAN,
            peak: f64::INFINITY,
            peak_time: f64::INFINITY,
        }
    }
    pub fn fields(&self) -> [(&'static str, f64); 9] {
        [
            ("RiseTime", self.rise),
            ("TransientTime", self.transient),
            ("SettlingTime", self.settling),
            ("SettlingMin", self.smin),
            ("SettlingMax", self.smax),
            ("Overshoot", self.overshoot),
            ("Undershoot", self.undershoot),
            ("Peak", self.peak),
            ("PeakTime", self.peak_time),
        ]
    }
}

/// Step metrics of sampled data (linear interpolation of crossings).
pub fn stepinfo_data(
    t: &[f64],
    y: &[f64],
    yfinal: f64,
    yinit: f64,
    st: f64,
    rt: (f64, f64),
) -> StepInfo {
    let n = y.len();
    if n == 0 || !yfinal.is_finite() {
        return StepInfo::nan();
    }
    let dy = yfinal - yinit;
    let cross = |level: f64| -> f64 {
        // First time the response reaches `level` (in the direction of dy).
        let reach = |v: f64| if dy >= 0.0 { v >= level } else { v <= level };
        for k in 0..n {
            if reach(y[k]) {
                if k == 0 {
                    return t[0];
                }
                let (y0, y1) = (y[k - 1], y[k]);
                let f = if y1 != y0 {
                    (level - y0) / (y1 - y0)
                } else {
                    1.0
                };
                return t[k - 1] + f * (t[k] - t[k - 1]);
            }
        }
        f64::NAN
    };
    let rise = cross(yinit + rt.1 * dy) - cross(yinit + rt.0 * dy);
    let last_out = |thr: f64| -> f64 {
        // Last time |y - yfinal| exceeds thr (interpolated exit time).
        let mut k = n;
        while k > 0 && (y[k - 1] - yfinal).abs() <= thr {
            k -= 1;
        }
        if k == 0 {
            return t[0];
        }
        if k == n {
            return f64::NAN;
        }
        let (e0, e1) = ((y[k - 1] - yfinal).abs(), (y[k] - yfinal).abs());
        let f = if e0 != e1 {
            (e0 - thr) / (e0 - e1)
        } else {
            0.0
        };
        t[k - 1] + f * (t[k] - t[k - 1])
    };
    let settling = last_out(st * dy.abs());
    let emax = y.iter().map(|v| (v - yfinal).abs()).fold(0.0, f64::max);
    let transient = last_out(st * emax);
    let t90 = cross(yinit + rt.1 * dy);
    let after: Vec<f64> = t
        .iter()
        .zip(y)
        .filter(|(tt, _)| t90.is_nan() || **tt >= t90 - 1e-12)
        .map(|(_, v)| *v)
        .collect();
    let smin = after.iter().cloned().fold(f64::INFINITY, f64::min);
    let smax = after.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let (mut pk, mut pt) = (0.0f64, t[0]);
    for k in 0..n {
        if (y[k] - yinit).abs() > pk {
            pk = (y[k] - yinit).abs();
            pt = t[k];
        }
    }
    let ext = if dy >= 0.0 {
        y.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
    } else {
        y.iter().cloned().fold(f64::INFINITY, f64::min)
    };
    let low = if dy >= 0.0 {
        y.iter().cloned().fold(f64::INFINITY, f64::min)
    } else {
        y.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
    };
    let overshoot = if dy == 0.0 {
        0.0
    } else {
        (100.0 * (ext - yfinal) / dy).max(0.0)
    };
    let undershoot = if dy == 0.0 {
        0.0
    } else {
        (100.0 * (yinit - low) / dy).max(0.0)
    };
    StepInfo {
        rise,
        transient,
        settling,
        smin,
        smax,
        overshoot,
        undershoot,
        peak: pk,
        peak_time: pt,
    }
}

pub fn stepinfo_sys(it: &mut Interp, sys: &Sys, st: f64, rt: (f64, f64)) -> R<StepInfo> {
    if sys.size() != (1, 1) {
        return Err(not_supported("stepinfo of MIMO models"));
    }
    if !isstable(it, sys)? {
        return Ok(StepInfo::nan());
    }
    let yf = dcgain(it, sys)?.d[0];
    let mut t = resp::default_time(it, sys, false)?;
    if !sys.is_discrete() && t.len() < 20000 {
        // Finer grid for accurate crossings.
        let tf = *t.last().unwrap();
        let n = 20000;
        t = (0..=n).map(|k| tf * k as f64 / n as f64).collect();
    }
    let sim = resp::step(it, sys, &t, 0)?;
    let y: Vec<f64> = sim.y.iter().map(|v| v[0]).collect();
    Ok(stepinfo_data(&t, &y, yf, 0.0, st, rt))
}

// ---------------------------------------------------------------------------
// Frequency-domain margins
// ---------------------------------------------------------------------------

/// `p(j w)` split into real polynomials in w: (real part, imag part),
/// both descending.
fn jw_split(p: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let n = p.len();
    let mut re = vec![0.0; n];
    let mut im = vec![0.0; n];
    for (i, a) in p.iter().enumerate() {
        let k = n - 1 - i; // power
        // j^k: 1, j, -1, -j
        match k % 4 {
            0 => re[i] = *a,
            1 => im[i] = *a,
            2 => re[i] = -a,
            _ => im[i] = -a,
        }
    }
    (re, im)
}

/// Continuous-equivalent polynomials in v (for discrete models, z is
/// mapped by z = (1 + p)/(1 - p) and p = j v).
struct Freq {
    num: Vec<f64>,
    den: Vec<f64>,
    discrete: Option<f64>,
}

impl Freq {
    fn new(it: &mut Interp, sys: &Sys) -> R<Freq> {
        if sys.size() != (1, 1) {
            return Err(not_supported("Margins of MIMO models"));
        }
        if sys.input_delay() != 0.0 {
            return Err(not_supported("Margins of models with time delays"));
        }
        let (num, den) = sys.tf_data(it)?;
        if sys.is_discrete() {
            let ts = if sys.ts > 0.0 { sys.ts } else { 1.0 };
            let n = num.len().max(den.len()) - 1;
            Ok(Freq {
                num: psubst_bilinear(&num, 1.0, 1.0, -1.0, 1.0, n),
                den: psubst_bilinear(&den, 1.0, 1.0, -1.0, 1.0, n),
                discrete: Some(ts),
            })
        } else {
            Ok(Freq {
                num,
                den,
                discrete: None,
            })
        }
    }
    fn to_w(&self, v: f64) -> f64 {
        match self.discrete {
            Some(ts) => 2.0 * v.atan() / ts,
            None => v,
        }
    }
    fn at(&self, v: f64) -> C {
        let s = c(0.0, v);
        peval(&self.num, s).div(peval(&self.den, s))
    }
    /// |N|^2 - g^2 |D|^2 as a polynomial in v.
    fn gain_poly(&self, g: f64) -> Vec<f64> {
        let (nr, ni) = jw_split(&self.num);
        let (dr, di) = jw_split(&self.den);
        let n2 = padd(&pconv(&nr, &nr), &pconv(&ni, &ni));
        let d2 = padd(&pconv(&dr, &dr), &pconv(&di, &di));
        padd(&n2, &pscale(&d2, -g * g))
    }
    /// Im(N conj D) as a polynomial in v.
    fn phase_poly(&self) -> Vec<f64> {
        let (nr, ni) = jw_split(&self.num);
        let (dr, di) = jw_split(&self.den);
        padd(&pconv(&ni, &dr), &pscale(&pconv(&nr, &di), -1.0))
    }
}

fn clean_roots(it: &mut Interp, p: &[f64]) -> R<Vec<f64>> {
    let p = ptrim(p);
    let scale = p.iter().map(|x| x.abs()).fold(0.0, f64::max);
    if scale == 0.0 {
        return Ok(vec![]);
    }
    let q: Vec<f64> = p
        .iter()
        .map(|x| if x.abs() <= 1e-14 * scale { 0.0 } else { *x })
        .collect();
    positive_real_roots(it, &q)
}

pub struct Margins {
    pub gm: f64,
    pub pm: f64,
    pub wcg: f64,
    pub wcp: f64,
}

pub fn margin(it: &mut Interp, sys: &Sys) -> R<Margins> {
    let f = Freq::new(it, sys)?;
    // Phase crossovers: G real and negative.
    let mut cands: Vec<(f64, f64)> = Vec::new(); // (v, |G|)
    for v in clean_roots(it, &f.phase_poly())? {
        let g = f.at(v);
        if g.is_finite() && g.re < 0.0 && g.im.abs() <= 1e-6 * g.abs().max(1e-300) {
            cands.push((v, g.abs()));
        }
    }
    let g0 = f.at(0.0);
    if g0.is_finite() && g0.re < 0.0 {
        cands.push((0.0, g0.abs()));
    }
    if f.discrete.is_some() {
        // Nyquist frequency (z = -1, v = infinity).
        let (nn, dd) = (ptrim(&f.num), ptrim(&f.den));
        if nn.len() == dd.len() && nn != [0.0] {
            let g = nn[0] / dd[0];
            if g < 0.0 {
                cands.push((f64::INFINITY, g.abs()));
            }
        }
    }
    let (mut gm, mut wcg) = (f64::INFINITY, f64::NAN);
    let mut best = f64::INFINITY;
    for (v, mag) in cands {
        if mag == 0.0 {
            continue;
        }
        let m = 1.0 / mag;
        if m.ln().abs() < best {
            best = m.ln().abs();
            gm = m;
            wcg = if v.is_infinite() {
                std::f64::consts::PI / f.discrete.unwrap()
            } else {
                f.to_w(v)
            };
        }
    }
    // Gain crossovers: |G| = 1.
    let (mut pm, mut wcp) = (f64::INFINITY, f64::NAN);
    let mut best = f64::INFINITY;
    for v in clean_roots(it, &f.gain_poly(1.0))? {
        let g = f.at(v);
        let mut ph = g.arg().to_degrees() + 180.0;
        while ph > 180.0 {
            ph -= 360.0;
        }
        while ph <= -180.0 {
            ph += 360.0;
        }
        if ph.abs() < best {
            best = ph.abs();
            pm = ph;
            wcp = f.to_w(v);
        }
    }
    Ok(Margins { gm, pm, wcg, wcp })
}

pub fn bandwidth(it: &mut Interp, sys: &Sys, drop_db: f64) -> R<f64> {
    let dc = dcgain(it, sys)?;
    let g0 = dc.d.first().copied().unwrap_or(f64::NAN);
    if !g0.is_finite() {
        return Ok(f64::NAN);
    }
    let f = Freq::new(it, sys)?;
    let g = g0.abs() * 10f64.powf(drop_db / 20.0);
    let roots = clean_roots(it, &f.gain_poly(g))?;
    Ok(match roots.first() {
        Some(v) => f.to_w(*v),
        None => f64::INFINITY,
    })
}

// ---------------------------------------------------------------------------
// State-space design
// ---------------------------------------------------------------------------

pub fn ctrb(a: &Mat, b: &Mat) -> Mat {
    let mut out = b.clone();
    let mut blk = b.clone();
    for _ in 1..a.r {
        blk = a.mul(&blk);
        out = out.hcat(&blk);
    }
    out
}

pub fn obsv(a: &Mat, cm: &Mat) -> Mat {
    ctrb(&a.t(), &cm.t()).t()
}

/// Ackermann's formula (single input).
pub fn acker(a: &Mat, b: &Mat, p: &[C]) -> R<Mat> {
    let n = a.r;
    if b.c != 1 {
        return Err(not_supported(
            "Pole placement with several inputs (place for MIMO)",
        ));
    }
    if p.len() != n {
        return Err(err(
            "Control:design:place1",
            format!(
                "The number of poles must equal the number of states ({}).",
                n
            ),
        ));
    }
    let phi_c = poly_from_roots(p);
    // phi(A) = A^n + c1 A^(n-1) + ... + cn I
    let mut phi = Mat::zeros(n, n);
    for coef in &phi_c {
        phi = phi.mul(a).add(&Mat::eye(n).scale(*coef));
    }
    let cm = ctrb(a, b);
    let mut en = Mat::zeros(1, n);
    en.set(0, n - 1, 1.0);
    // K = en * Cm^-1 * phi(A)  =>  solve Cm' y = en'
    let y = cm.t().solve(&en.t()).ok_or_else(|| {
        err(
            "Control:design:place2",
            "The pair (A,B) is not controllable: the poles cannot be placed.",
        )
    })?;
    Ok(y.t().mul(&phi))
}

/// Continuous algebraic Riccati equation
/// `A'X + XA - (XB + N) R^-1 (B'X + N') + Q = 0` by the matrix sign
/// function of the Hamiltonian.
pub fn care(a: &Mat, b: &Mat, q: &Mat, r: &Mat, nmat: Option<&Mat>) -> R<Mat> {
    let n = a.r;
    let fail = |m: &str| err("Control:design:care1", m.to_string());
    let ri = r.inv().ok_or_else(|| fail("R must be invertible."))?;
    let (abar, qbar) = match nmat {
        Some(nm) => (
            a.sub(&b.mul(&ri).mul(&nm.t())),
            q.sub(&nm.mul(&ri).mul(&nm.t())),
        ),
        None => (a.clone(), q.clone()),
    };
    let g = b.mul(&ri).mul(&b.t());
    let top = abar.hcat(&g.scale(-1.0));
    let bot = qbar.scale(-1.0).hcat(&abar.t().scale(-1.0));
    let h = top.vcat(&bot);
    let w = matrix_sign(&h).ok_or_else(|| {
        fail("The Hamiltonian has eigenvalues on the imaginary axis: no stabilizing solution.")
    })?;
    let w11 = w.block(0, n, 0, n);
    let w12 = w.block(0, n, n, 2 * n);
    let w21 = w.block(n, 2 * n, 0, n);
    let w22 = w.block(n, 2 * n, n, 2 * n);
    let lhs = w12.vcat(&w22.add(&Mat::eye(n)));
    let rhs = w11.add(&Mat::eye(n)).vcat(&w21).scale(-1.0);
    let x = lhs
        .lstsq(&rhs)
        .ok_or_else(|| fail("No stabilizing solution exists (the pair is not stabilizable)."))?;
    if x.d.iter().any(|v| !v.is_finite()) {
        return Err(fail("No finite stabilizing solution exists."));
    }
    Ok(x.symmetrize())
}

fn matrix_sign(h: &Mat) -> Option<Mat> {
    let m = h.r;
    let mut z = h.clone();
    for _ in 0..200 {
        let zi = z.inv()?;
        // Determinant scaling via the norms (cheap, robust).
        let cfac = (zi.norm1() / z.norm1()).sqrt();
        let nz = z.lin(0.5 * cfac, &zi, 0.5 / cfac);
        let diff = nz.sub(&z).norm1();
        z = nz;
        if diff <= 1e-13 * z.norm1() {
            break;
        }
    }
    let _ = m;
    if z.d.iter().any(|v| !v.is_finite()) {
        return None;
    }
    Some(z)
}

/// `A X + X B + C = 0` (Kronecker form; small systems).
pub fn sylvester(a: &Mat, b: &Mat, cm: &Mat) -> R<Mat> {
    let (n, m) = (a.r, b.r);
    let nm = n * m;
    let mut k = Mat::zeros(nm, nm);
    // vec(AX) = (I_m ⊗ A) vec X ; vec(XB) = (B' ⊗ I_n) vec X
    for j in 0..m {
        for i in 0..n {
            let row = i + j * n;
            for p in 0..n {
                let col = p + j * n;
                k.set(row, col, k.at(row, col) + a.at(i, p));
            }
            for q in 0..m {
                let col = i + q * n;
                k.set(row, col, k.at(row, col) + b.at(q, j));
            }
        }
    }
    let x = k.solve(&Mat::col(&cm.d).scale(-1.0)).ok_or_else(|| {
        err(
            "Control:design:lyap1",
            "The Lyapunov/Sylvester equation has no unique solution.",
        )
    })?;
    Ok(Mat { r: n, c: m, d: x.d })
}

/// `A X A' - X + Q = 0`.
pub fn dlyap(a: &Mat, q: &Mat) -> R<Mat> {
    let n = a.r;
    let nn = n * n;
    let mut k = Mat::zeros(nn, nn);
    // vec(A X A') = (A ⊗ A) vec X
    for j in 0..n {
        for i in 0..n {
            let row = i + j * n;
            for q2 in 0..n {
                for p in 0..n {
                    let col = p + q2 * n;
                    k.set(row, col, a.at(i, p) * a.at(j, q2));
                }
            }
            k.set(row, row, k.at(row, row) - 1.0);
        }
    }
    let x = k.solve(&Mat::col(&q.d).scale(-1.0)).ok_or_else(|| {
        err(
            "Control:design:lyap1",
            "The discrete Lyapunov equation has no unique solution.",
        )
    })?;
    Ok(Mat { r: n, c: n, d: x.d })
}

// ---------------------------------------------------------------------------
// minreal, rlocus
// ---------------------------------------------------------------------------

pub fn minreal(it: &mut Interp, sys: &Sys, tol: f64) -> R<Sys> {
    if let Data::Ss(m) = &sys.data
        && sys.size() != (1, 1)
    {
        let r1 = super::num::rank(it, &ctrb(&m.a, &m.b))?;
        let r2 = super::num::rank(it, &obsv(&m.a, &m.c))?;
        if r1 == m.nx() && r2 == m.nx() {
            return Ok(sys.clone());
        }
        return Err(not_supported(
            "minreal of non-minimal MIMO state-space models",
        ));
    }
    let (z, p, k) = sys.zpk_data(it)?;
    let mut z = z;
    let mut keep_p = Vec::new();
    for pp in p {
        let tol_abs = tol * (1.0 + pp.abs()) * 10.0;
        let hit = z
            .iter()
            .enumerate()
            .filter(|(_, zz)| zz.sub(pp).abs() <= tol_abs)
            .min_by(|a, b| {
                a.1.sub(pp)
                    .abs()
                    .partial_cmp(&b.1.sub(pp).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i);
        match hit {
            Some(i) => {
                z.remove(i);
            }
            None => keep_p.push(pp),
        }
    }
    let red = Sys::new(Kind::Zpk, Data::Zpk { z, p: keep_p, k }, sys.ts).with_meta_of(sys);
    let kind = if sys.kind.rank() == 0 {
        Kind::Tf
    } else {
        sys.kind
    };
    let out = red.convert(it, kind)?;
    if let Data::Tf { num, den } = &out.data {
        let d = ptrim(den);
        let lead = d[0];
        let mut s = Sys::tf(&pscale(num, 1.0 / lead), &pscale(den, 1.0 / lead), sys.ts);
        s.extra = out.extra.clone();
        return Ok(s);
    }
    Ok(out)
}

/// Closed-loop poles of `den + k num` for each gain; columns follow
/// continuous branches.
pub fn rlocus(it: &mut Interp, sys: &Sys, gains: Option<Vec<f64>>) -> R<(Vec<Vec<C>>, Vec<f64>)> {
    if sys.size() != (1, 1) {
        return Err(not_supported("rlocus of MIMO models"));
    }
    let (num, den) = sys.tf_data(it)?;
    let (n, d) = (ptrim(&num), ptrim(&den));
    let ks = match gains {
        Some(k) => k,
        None => {
            let roots: Vec<C> = proots(it, &d)?.into_iter().chain(proots(it, &n)?).collect();
            let rho = roots.iter().map(|z| z.abs()).fold(1.0, f64::max);
            let excess = d.len().saturating_sub(n.len()) as i32;
            let scale = (d[0] / n[0]).abs() * rho.powi(excess.max(1));
            let mut k = vec![0.0];
            let m = 300;
            for i in 0..=m {
                k.push(scale * 10f64.powf(-3.0 + 6.0 * i as f64 / m as f64));
            }
            k
        }
    };
    let order = d.len().max(n.len()) - 1;
    let mut cols: Vec<Vec<C>> = Vec::with_capacity(ks.len());
    for &k in &ks {
        let ch = padd(&d, &pscale(&n, k));
        let mut r = proots(it, &ch)?;
        while r.len() < order {
            r.push(c(f64::INFINITY, 0.0));
        }
        if let Some(prev) = cols.last() {
            // Greedy matching to the previous column.
            let mut used = vec![false; r.len()];
            let mut ordered = Vec::with_capacity(r.len());
            for pz in prev {
                let mut best = None;
                let mut bd = f64::INFINITY;
                for (i, z) in r.iter().enumerate() {
                    if used[i] {
                        continue;
                    }
                    let dd = z.sub(*pz).abs();
                    let dd = if dd.is_nan() { f64::INFINITY } else { dd };
                    if best.is_none() || dd < bd {
                        bd = dd;
                        best = Some(i);
                    }
                }
                if let Some(i) = best {
                    used[i] = true;
                    ordered.push(r[i]);
                }
            }
            r = ordered;
        }
        cols.push(r);
    }
    Ok((cols, ks))
}
