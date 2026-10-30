//! Filter design: `fir1`, `fir2`, `firls`, `butter`, `cheby1`, `cheby2`,
//! `ellip`, order estimation and `bilinear`.

use std::f64::consts::PI;

use indy_value::Value;

use super::elliptic::ellipap;
use super::windows::{hamming, sinc};
use super::{
    C, C1, CZ, cadd, cdiv, cmul, cscale, csqrt, csub, cvec, e, from_c, opt_string, poly_of_roots,
    prod, real_part, rrow, rvec,
};
use crate::builtins::{arg_f64, need, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "fir1", fir1);
    reg(it, "fir2", fir2);
    reg(it, "firls", firls);
    reg(it, "butter", |_, a, n| iir_design(Kind::Butter, a, n));
    reg(it, "cheby1", |_, a, n| iir_design(Kind::Cheby1, a, n));
    reg(it, "cheby2", |_, a, n| iir_design(Kind::Cheby2, a, n));
    reg(it, "ellip", |_, a, n| iir_design(Kind::Ellip, a, n));
    reg(it, "buttord", |_, a, n| ord_design(Kind::Butter, a, n));
    reg(it, "cheb1ord", |_, a, n| ord_design(Kind::Cheby1, a, n));
    reg(it, "cheb2ord", |_, a, n| ord_design(Kind::Cheby2, a, n));
    reg(it, "bilinear", bilinear);
}

fn order_arg(v: &Value, f: &str) -> R<usize> {
    let x = arg_f64(v, f)?;
    if x < 1.0 || x.fract() != 0.0 || !x.is_finite() {
        return Err(e(
            "MATLAB:signal:invalidOrder",
            format!("{}: the order must be a positive integer.", f),
        ));
    }
    Ok(x as usize)
}

// ---------------------------------------------------------------------------
// fir1
// ---------------------------------------------------------------------------

/// Passband edges `[lo, hi]` (normalised to Nyquist = 1) for the band
/// layout `edges` whose first band is a passband iff `first_pass`.
fn pass_bands(edges: &[f64], first_pass: bool) -> Vec<(f64, f64)> {
    let mut bands = Vec::new();
    let mut e0 = vec![0.0];
    e0.extend_from_slice(edges);
    e0.push(1.0);
    for (i, w) in e0.windows(2).enumerate() {
        if (i.is_multiple_of(2)) == first_pass {
            bands.push((w[0], w[1]));
        }
    }
    bands
}

pub fn fir_ideal(n: usize, bands: &[(f64, f64)]) -> Vec<f64> {
    (0..=n)
        .map(|k| {
            let m = k as f64 - n as f64 / 2.0;
            bands
                .iter()
                .map(|(lo, hi)| hi * sinc(hi * m) - lo * sinc(lo * m))
                .sum()
        })
        .collect()
}

fn fir1(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "fir1")?;
    let mut n = a[0].scalar_f64().filter(|x| *x >= 1.0 && x.fract() == 0.0);
    if n.is_none() {
        return Err(e(
            "signal:fir1:order",
            "fir1: the order must be a positive integer.",
        ));
    }
    let wn = rvec(&a[1], "fir1")?;
    if wn.is_empty() || wn.iter().any(|w| !(*w > 0.0 && *w < 1.0)) {
        return Err(e(
            "signal:fir1:cutoff",
            "fir1: the cutoff frequencies must be between 0 and 1 (exclusive).",
        ));
    }
    if wn.windows(2).any(|w| w[1] <= w[0]) {
        return Err(e(
            "signal:fir1:cutoff",
            "fir1: the cutoff frequencies must be strictly increasing.",
        ));
    }
    let mut ftype: Option<String> = None;
    let mut window: Option<Vec<f64>> = None;
    let mut scale = true;
    for v in &a[2..] {
        if let Some(s) = opt_string(v) {
            match s.to_ascii_lowercase().as_str() {
                "scale" => scale = true,
                "noscale" => scale = false,
                "low" | "high" | "bandpass" | "stop" | "dc-0" | "dc-1" => {
                    ftype = Some(s.to_ascii_lowercase())
                }
                _ => {
                    return Err(e(
                        "signal:fir1:badType",
                        format!("fir1: unknown option '{}'.", s),
                    ));
                }
            }
        } else {
            window = Some(rvec(v, "fir1")?);
        }
    }
    let m = wn.len();
    let first_pass = match (m, ftype.as_deref()) {
        (1, None) | (1, Some("low")) => true,
        (1, Some("high")) => false,
        (2, None) | (2, Some("bandpass")) => false,
        (2, Some("stop")) => true,
        (_, None) | (_, Some("dc-0")) => false,
        (_, Some("dc-1")) => true,
        (_, Some(t)) => {
            return Err(e(
                "signal:fir1:badType",
                format!("fir1: filter type '{}' does not fit this cutoff vector.", t),
            ));
        }
    };
    let bands = pass_bands(&wn, first_pass);
    let ends_at_nyquist = bands.last().is_some_and(|b| b.1 == 1.0);
    let mut nn = n.take().unwrap_or(1.0) as usize;
    if ends_at_nyquist && !nn.is_multiple_of(2) {
        // MATLAB raises the order: odd symmetric filters vanish at Nyquist.
        nn += 1;
    }
    let win = match window {
        Some(w) => {
            if w.len() != nn + 1 {
                return Err(e(
                    "signal:fir1:window",
                    "fir1: the window length must equal the filter order plus one.",
                ));
            }
            w
        }
        None => hamming(nn + 1),
    };
    let mut b = fir_ideal(nn, &bands);
    for (x, w) in b.iter_mut().zip(&win) {
        *x *= w;
    }
    if scale {
        let f0 = if bands[0].0 == 0.0 {
            0.0
        } else if bands[0].1 == 1.0 {
            1.0
        } else {
            (bands[0].0 + bands[0].1) / 2.0
        };
        let mut s = CZ;
        for (k, x) in b.iter().enumerate() {
            let ph = -PI * f0 * k as f64;
            s = cadd(s, (x * ph.cos(), x * ph.sin()));
        }
        let g = s.0.hypot(s.1);
        for x in b.iter_mut() {
            *x /= g;
        }
    }
    Ok(vec![rrow(b)])
}

// ---------------------------------------------------------------------------
// fir2
// ---------------------------------------------------------------------------

fn fir2(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 3, "fir2")?;
    let nn = order_arg(&a[0], "fir2")?;
    let f = rvec(&a[1], "fir2")?;
    let m = rvec(&a[2], "fir2")?;
    if f.len() != m.len() || f.len() < 2 {
        return Err(e(
            "signal:fir2:lengths",
            "fir2: F and M must be vectors of the same length (at least 2).",
        ));
    }
    if f[0] != 0.0 || f[f.len() - 1] != 1.0 {
        return Err(e(
            "signal:fir2:range",
            "fir2: the frequency vector must start at 0 and end at 1.",
        ));
    }
    if f.windows(2).any(|w| w[1] < w[0]) {
        return Err(e(
            "signal:fir2:order",
            "fir2: the frequency vector must be nondecreasing.",
        ));
    }
    // optional npt, lap, window
    let mut nums: Vec<&Value> = Vec::new();
    for v in &a[3..] {
        nums.push(v);
    }
    let mut npt = if nn < 1024 {
        512
    } else {
        (nn as f64).log2().ceil().exp2() as usize
    };
    let mut lap = 25usize;
    let mut window: Option<Vec<f64>> = None;
    match nums.len() {
        0 => {}
        1 => {
            if nums[0].numel() == 1 {
                npt = arg_f64(nums[0], "fir2")? as usize;
            } else {
                window = Some(rvec(nums[0], "fir2")?);
            }
        }
        2 => {
            npt = arg_f64(nums[0], "fir2")? as usize;
            if nums[1].numel() == 1 {
                lap = arg_f64(nums[1], "fir2")? as usize;
            } else {
                window = Some(rvec(nums[1], "fir2")?);
            }
        }
        _ => {
            npt = arg_f64(nums[0], "fir2")? as usize;
            lap = arg_f64(nums[1], "fir2")? as usize;
            window = Some(rvec(nums[2], "fir2")?);
        }
    }
    if npt == 0 || (npt as f64) <= nn as f64 / 2.0 {
        return Err(e(
            "signal:fir2:npt",
            "fir2: npt must be greater than half the filter order.",
        ));
    }
    let win = match window {
        Some(w) if w.len() == nn + 1 => w,
        Some(_) => {
            return Err(e(
                "signal:fir2:window",
                "fir2: the window length must equal the filter order plus one.",
            ));
        }
        None => hamming(nn + 1),
    };
    let df = 1.0 / npt as f64;
    // widen repeated frequency points by lap/2 grid steps on both sides
    let mut ff = f.clone();
    let mut mm = m.clone();
    let half = lap as f64 / 2.0 * df;
    let mut i = 0;
    let mut nf = Vec::new();
    let mut nm = Vec::new();
    while i < ff.len() {
        if i + 1 < ff.len() && ff[i] == ff[i + 1] {
            let c = ff[i];
            nf.push((c - half).max(0.0));
            nm.push(mm[i]);
            nf.push((c + half).min(1.0));
            nm.push(mm[i + 1]);
            i += 2;
        } else {
            nf.push(ff[i]);
            nm.push(mm[i]);
            i += 1;
        }
    }
    ff = nf;
    mm = nm;
    let interp = |x: f64| -> f64 {
        let mut j = 0;
        while j + 1 < ff.len() - 1 && ff[j + 1] <= x {
            j += 1;
        }
        let (x0, x1) = (ff[j], ff[j + 1]);
        if x1 == x0 {
            mm[j + 1]
        } else {
            mm[j] + (mm[j + 1] - mm[j]) * (x - x0) / (x1 - x0)
        }
    };
    let grid: Vec<f64> = (0..=npt).map(|i| i as f64 * df).collect();
    let mag: Vec<f64> = grid.iter().map(|g| interp(*g)).collect();
    // Hermitian spectrum with linear phase of n/2 samples, length 2*npt.
    let nfft = 2 * npt;
    let delay = nn as f64 / 2.0;
    let mut spec = vec![CZ; nfft];
    for i in 0..=npt {
        let ph = -delay * PI * grid[i];
        let h = (mag[i] * ph.cos(), mag[i] * ph.sin());
        spec[i] = h;
        if i > 0 && i < npt {
            spec[nfft - i] = (h.0, -h.1);
        }
    }
    // the Nyquist bin must be real
    spec[npt].1 = 0.0;
    let mut b = Vec::with_capacity(nn + 1);
    for k in 0..=nn {
        let mut s = 0.0;
        for (i, h) in spec.iter().enumerate() {
            let ang = 2.0 * PI * ((i * k) % nfft) as f64 / nfft as f64;
            s += h.0 * ang.cos() - h.1 * ang.sin();
        }
        b.push(s / nfft as f64);
    }
    for (x, w) in b.iter_mut().zip(&win) {
        *x *= w;
    }
    Ok(vec![rrow(b)])
}

// ---------------------------------------------------------------------------
// firls (least-squares multiband, default type)
// ---------------------------------------------------------------------------

pub fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for c in 0..n {
        let p = (c..n).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))?;
        if a[p][c].abs() < 1e-300 {
            return None;
        }
        a.swap(c, p);
        b.swap(c, p);
        for r in c + 1..n {
            let f = a[r][c] / a[c][c];
            for k in c..n {
                a[r][k] -= f * a[c][k];
            }
            b[r] -= f * b[c];
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let mut s = b[r];
        for k in r + 1..n {
            s -= a[r][k] * x[k];
        }
        x[r] = s / a[r][r];
    }
    Some(x)
}

fn int_cos(x: f64, w1: f64, w2: f64) -> f64 {
    if x == 0.0 {
        w2 - w1
    } else {
        ((x * w2).sin() - (x * w1).sin()) / x
    }
}

fn int_w_cos(x: f64, w: f64) -> f64 {
    if x == 0.0 {
        w * w / 2.0
    } else {
        w * (x * w).sin() / x + (x * w).cos() / (x * x)
    }
}

fn firls(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 3, "firls")?;
    let nn = order_arg(&a[0], "firls")?;
    let f = rvec(&a[1], "firls")?;
    let amp = rvec(&a[2], "firls")?;
    if f.len() != amp.len() || !f.len().is_multiple_of(2) || f.is_empty() {
        return Err(e(
            "signal:firls:lengths",
            "firls: F and A must have the same, even length.",
        ));
    }
    if f.windows(2).any(|w| w[1] < w[0]) || f[0] < 0.0 || f[f.len() - 1] > 1.0 {
        return Err(e(
            "signal:firls:freq",
            "firls: frequencies must be nondecreasing within [0, 1].",
        ));
    }
    let nb = f.len() / 2;
    let mut w = vec![1.0; nb];
    let mut rest = &a[3..];
    if let Some(v) = rest.first()
        && opt_string(v).is_none()
    {
        w = rvec(v, "firls")?;
        if w.len() != nb {
            return Err(e(
                "signal:firls:weights",
                "firls: one weight per band is required.",
            ));
        }
        rest = &rest[1..];
    }
    if let Some(v) = rest.first()
        && let Some(s) = opt_string(v)
        && !s.is_empty()
    {
        return Err(e(
            "signal:firls:type",
            format!("firls: filter type '{}' is not supported yet.", s),
        ));
    }
    let even = nn.is_multiple_of(2);
    let m = if even { nn / 2 } else { nn.div_ceil(2) };
    let nunk = if even { m + 1 } else { m };
    // basis k: cos(k w) for even order (k = 0..=m); cos((k-1/2) w) for odd (k = 1..=m)
    let freq = |k: usize| -> f64 { if even { k as f64 } else { k as f64 + 0.5 } };
    let mut q = vec![vec![0.0; nunk]; nunk];
    let mut p = vec![0.0; nunk];
    for bi in 0..nb {
        let (w1, w2) = (f[2 * bi] * PI, f[2 * bi + 1] * PI);
        let (a1, a2) = (amp[2 * bi], amp[2 * bi + 1]);
        let slope = if w2 > w1 { (a2 - a1) / (w2 - w1) } else { 0.0 };
        let c0 = a1 - slope * w1;
        for i in 0..nunk {
            let xi = freq(i);
            for j in 0..nunk {
                let xj = freq(j);
                q[i][j] += w[bi] * 0.5 * (int_cos(xi - xj, w1, w2) + int_cos(xi + xj, w1, w2));
            }
            let ip = slope * (int_w_cos(xi, w2) - int_w_cos(xi, w1)) + c0 * int_cos(xi, w1, w2);
            p[i] += w[bi] * ip;
        }
    }
    let coef = solve(q, p).ok_or_else(|| e("signal:firls:singular", "firls: singular system."))?;
    let mut h = vec![0.0; nn + 1];
    if even {
        h[m] = coef[0];
        for k in 1..=m {
            h[m - k] = coef[k] / 2.0;
            h[m + k] = coef[k] / 2.0;
        }
    } else {
        for k in 1..=m {
            h[m - k] = coef[k - 1] / 2.0;
            h[m + k - 1] = coef[k - 1] / 2.0;
        }
    }
    Ok(vec![rrow(h)])
}

// ---------------------------------------------------------------------------
// IIR design
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Butter,
    Cheby1,
    Cheby2,
    Ellip,
}

pub type Zpk = (Vec<C>, Vec<C>, f64);

fn buttap(n: usize) -> Zpk {
    let p = (0..n)
        .map(|k| {
            let th = PI * (2 * k + n + 1) as f64 / (2 * n) as f64;
            (th.cos(), th.sin())
        })
        .collect();
    (vec![], p, 1.0)
}

fn cheb1ap(n: usize, rp: f64) -> Zpk {
    let eps = (10f64.powf(rp / 10.0) - 1.0).sqrt();
    let mu = (1.0 / eps).asinh() / n as f64;
    let p: Vec<C> = (1..=n)
        .map(|k| {
            let th = PI * (2 * k - 1) as f64 / (2 * n) as f64;
            (-mu.sinh() * th.sin(), -mu.cosh() * th.cos())
        })
        .collect();
    let mut kk = prod(&p.iter().map(|c| (-c.0, -c.1)).collect::<Vec<_>>()).0;
    if n.is_multiple_of(2) {
        kk /= (1.0 + eps * eps).sqrt();
    }
    (vec![], p, kk)
}

fn cheb2ap(n: usize, rs: f64) -> Zpk {
    let delta = 1.0 / (10f64.powf(rs / 10.0) - 1.0).sqrt();
    let mu = (1.0 / delta).asinh() / n as f64;
    let mut z = Vec::new();
    let mut p = Vec::new();
    for k in 1..=n {
        let th = PI * (2 * k - 1) as f64 / (2 * n) as f64;
        if 2 * k - 1 != n {
            z.push((0.0, 1.0 / th.cos()));
        }
        let q = (-mu.sinh() * th.sin(), mu.cosh() * th.cos());
        p.push(cdiv(C1, q));
    }
    let neg = |v: &[C]| v.iter().map(|c| (-c.0, -c.1)).collect::<Vec<_>>();
    let k = cdiv(prod(&neg(&p)), prod(&neg(&z))).0;
    (z, p, k)
}

fn lp2lp(z: &[C], p: &[C], k: f64, wo: f64) -> Zpk {
    let deg = p.len() as i32 - z.len() as i32;
    (
        z.iter().map(|c| cscale(*c, wo)).collect(),
        p.iter().map(|c| cscale(*c, wo)).collect(),
        k * wo.powi(deg),
    )
}

fn lp2hp(z: &[C], p: &[C], k: f64, wo: f64) -> Zpk {
    let deg = p.len() - z.len();
    let mut zz: Vec<C> = z.iter().map(|c| cdiv((wo, 0.0), *c)).collect();
    zz.extend(std::iter::repeat_n(CZ, deg));
    let pp: Vec<C> = p.iter().map(|c| cdiv((wo, 0.0), *c)).collect();
    let g = cdiv(
        prod(&z.iter().map(|c| (-c.0, -c.1)).collect::<Vec<_>>()),
        prod(&p.iter().map(|c| (-c.0, -c.1)).collect::<Vec<_>>()),
    )
    .0;
    (zz, pp, k * g)
}

fn split_all(v: &[C], wo: f64, bw: f64, inv: bool) -> Vec<C> {
    let pairs: Vec<[C; 2]> = v.iter().map(|c| split_pair(*c, wo, bw, inv)).collect();
    let mut out: Vec<C> = pairs.iter().map(|p| p[0]).collect();
    out.extend(pairs.iter().map(|p| p[1]));
    out
}

fn split_pair(s: C, wo: f64, bw: f64, inv: bool) -> [C; 2] {
    // roots of s^2 - a s + wo^2 with a = s*bw (bandpass) or bw/s (bandstop)
    let a = if inv {
        cdiv((bw, 0.0), s)
    } else {
        cscale(s, bw)
    };
    let h = cscale(a, 0.5);
    let d = csqrt(csub(cmul(h, h), (wo * wo, 0.0)));
    let (r1, r2) = (cadd(h, d), csub(h, d));
    // Root order follows the reference implementation: bandpass lists the
    // root on the opposite side of the real axis first, bandstop the same
    // side (real prototype poles: negative / positive imaginary part).
    let want_pos = if inv { s.1 >= 0.0 } else { s.1 < 0.0 };
    if (r1.1 >= 0.0) == want_pos {
        [r1, r2]
    } else {
        [r2, r1]
    }
}

fn lp2bp(z: &[C], p: &[C], k: f64, wo: f64, bw: f64) -> Zpk {
    let deg = p.len() - z.len();
    let mut zz: Vec<C> = split_all(z, wo, bw, false);
    zz.extend(std::iter::repeat_n(CZ, deg));
    let pp: Vec<C> = split_all(p, wo, bw, false);
    (zz, pp, k * bw.powi(deg as i32))
}

fn lp2bs(z: &[C], p: &[C], k: f64, wo: f64, bw: f64) -> Zpk {
    let deg = p.len() - z.len();
    let mut zz: Vec<C> = split_all(z, wo, bw, true);
    for _ in 0..deg {
        zz.push((0.0, -wo));
        zz.push((0.0, wo));
    }
    let pp: Vec<C> = split_all(p, wo, bw, true);
    let g = cdiv(
        prod(&z.iter().map(|c| (-c.0, -c.1)).collect::<Vec<_>>()),
        prod(&p.iter().map(|c| (-c.0, -c.1)).collect::<Vec<_>>()),
    )
    .0;
    (zz, pp, k * g)
}

/// Bilinear transform of an analog zpk model, `s = 2 fs (z-1)/(z+1)`.
pub fn bilinear_zpk(z: &[C], p: &[C], k: f64, fs: f64) -> Zpk {
    let fs2 = 2.0 * fs;
    let map = |c: &C| cdiv(cadd((fs2, 0.0), *c), csub((fs2, 0.0), *c));
    let mut zz: Vec<C> = z.iter().map(map).collect();
    let pp: Vec<C> = p.iter().map(map).collect();
    zz.extend(std::iter::repeat_n(
        (-1.0, 0.0),
        p.len().saturating_sub(z.len()),
    ));
    let num = prod(&z.iter().map(|c| csub((fs2, 0.0), *c)).collect::<Vec<_>>());
    let den = prod(&p.iter().map(|c| csub((fs2, 0.0), *c)).collect::<Vec<_>>());
    (zz, pp, k * cdiv(num, den).0)
}

#[derive(Clone, Copy, PartialEq)]
enum Band {
    Low,
    High,
    Pass,
    Stop,
}

pub fn zpk_to_tf(z: &[C], p: &[C], k: f64) -> (Vec<f64>, Vec<f64>) {
    let b: Vec<f64> = real_part(&poly_of_roots(z)).iter().map(|x| x * k).collect();
    let a = real_part(&poly_of_roots(p));
    (b, a)
}

fn tf_to_ss(b: &[f64], a: &[f64]) -> (Vec<f64>, Vec<f64>, Vec<f64>, f64, usize) {
    // controller canonical form (MATLAB tf2ss); b, a same length n+1, a[0] = 1
    let n = a.len() - 1;
    let a0 = a[0];
    let bb: Vec<f64> = b.iter().map(|x| x / a0).collect();
    let aa: Vec<f64> = a.iter().map(|x| x / a0).collect();
    let mut am = vec![0.0; n * n];
    for j in 0..n {
        am[j * n] = -aa[j + 1]; // first row, column-major
    }
    for i in 1..n {
        am[(i - 1) * n + i] = 1.0;
    }
    let mut bm = vec![0.0; n];
    if n > 0 {
        bm[0] = 1.0;
    }
    let d = bb[0];
    let cm: Vec<f64> = (0..n).map(|j| bb[j + 1] - d * aa[j + 1]).collect();
    (am, bm, cm, d, n)
}

fn iir_design(kind: Kind, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let name = match kind {
        Kind::Butter => "butter",
        Kind::Cheby1 => "cheby1",
        Kind::Cheby2 => "cheby2",
        Kind::Ellip => "ellip",
    };
    let nripple = match kind {
        Kind::Butter => 0,
        Kind::Cheby1 | Kind::Cheby2 => 1,
        Kind::Ellip => 2,
    };
    need(&a, 2 + nripple, name)?;
    let n = order_arg(&a[0], name)?;
    let mut rp = 0.0;
    let mut rs = 0.0;
    match kind {
        Kind::Butter => {}
        Kind::Cheby1 => rp = arg_f64(&a[1], name)?,
        Kind::Cheby2 => rs = arg_f64(&a[1], name)?,
        Kind::Ellip => {
            rp = arg_f64(&a[1], name)?;
            rs = arg_f64(&a[2], name)?;
        }
    }
    if matches!(kind, Kind::Cheby1 | Kind::Ellip) && rp < 0.0 {
        return Err(e(
            "signal:design:rp",
            format!("{}: passband ripple must be nonnegative.", name),
        ));
    }
    if matches!(kind, Kind::Cheby2 | Kind::Ellip) && rs < 0.0 {
        return Err(e(
            "signal:design:rs",
            format!("{}: stopband attenuation must be nonnegative.", name),
        ));
    }
    if kind == Kind::Ellip && rs <= rp {
        return Err(e(
            "signal:design:rsrp",
            "ellip: stopband attenuation must be larger than the passband ripple.",
        ));
    }
    let wn = rvec(&a[1 + nripple], name)?;
    let mut analog = false;
    let mut band: Option<Band> = None;
    for v in &a[2 + nripple..] {
        let s = opt_string(v).ok_or_else(|| {
            e(
                "signal:design:option",
                format!("{}: unexpected numeric option.", name),
            )
        })?;
        match s.to_ascii_lowercase().as_str() {
            "s" => analog = true,
            "z" => analog = false,
            "low" => band = Some(Band::Low),
            "high" => band = Some(Band::High),
            "bandpass" => band = Some(Band::Pass),
            "stop" => band = Some(Band::Stop),
            _ => {
                return Err(e(
                    "signal:design:option",
                    format!("{}: unknown option '{}'.", name, s),
                ));
            }
        }
    }
    let band = match (wn.len(), band) {
        (1, None) | (1, Some(Band::Low)) => Band::Low,
        (1, Some(Band::High)) => Band::High,
        (2, None) | (2, Some(Band::Pass)) => Band::Pass,
        (2, Some(Band::Stop)) => Band::Stop,
        _ => {
            return Err(e(
                "signal:design:wn",
                format!(
                    "{}: Wn must be a scalar or a two-element vector matching the filter type.",
                    name
                ),
            ));
        }
    };
    if wn.iter().any(|w| *w <= 0.0 || w.is_nan() || !w.is_finite()) {
        return Err(e(
            "signal:design:wn",
            format!("{}: Wn must be positive.", name),
        ));
    }
    if !analog && wn.iter().any(|w| *w >= 1.0) {
        return Err(e(
            "signal:design:wn",
            format!("{}: Wn must be within 0 and 1.", name),
        ));
    }
    if wn.len() == 2 && wn[0] >= wn[1] {
        return Err(e(
            "signal:design:wn",
            format!("{}: Wn must be increasing.", name),
        ));
    }
    let (z0, p0, k0) = match kind {
        Kind::Butter => buttap(n),
        Kind::Cheby1 => cheb1ap(n, rp),
        Kind::Cheby2 => cheb2ap(n, rs),
        Kind::Ellip => ellipap(n, rp, rs),
    };
    let fs = 2.0;
    let u: Vec<f64> = if analog {
        wn.clone()
    } else {
        wn.iter().map(|w| 2.0 * fs * (PI * w / fs).tan()).collect()
    };
    let (z1, p1, k1) = match band {
        Band::Low => lp2lp(&z0, &p0, k0, u[0]),
        Band::High => lp2hp(&z0, &p0, k0, u[0]),
        Band::Pass => lp2bp(&z0, &p0, k0, (u[0] * u[1]).sqrt(), u[1] - u[0]),
        Band::Stop => lp2bs(&z0, &p0, k0, (u[0] * u[1]).sqrt(), u[1] - u[0]),
    };
    let (zf, pf, kf) = if analog {
        (z1, p1, k1)
    } else {
        bilinear_zpk(&z1, &p1, k1, fs)
    };
    if nargout == 3 {
        return Ok(vec![super::ccol(&zf), super::ccol(&pf), Value::num(kf)]);
    }
    let (b, aa) = zpk_to_tf(&zf, &pf, kf);
    if nargout >= 4 {
        let (am, bm, cm, d, nst) = tf_to_ss(&b, &aa);
        return Ok(vec![
            Value::matrix(vec![nst, nst], am),
            Value::col(bm),
            Value::row(cm),
            Value::num(d),
        ]);
    }
    Ok(vec![rrow(b), rrow(aa)])
}

// ---------------------------------------------------------------------------
// bilinear
// ---------------------------------------------------------------------------

fn bilinear(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "bilinear")?;
    // bilinear(z,p,k,fs) / bilinear(b,a,fs) / [..]
    if a.len() == 4 {
        let z = cvec(&a[0], "bilinear")?;
        let p = cvec(&a[1], "bilinear")?;
        let k = arg_f64(&a[2], "bilinear")?;
        let fs = arg_f64(&a[3], "bilinear")?;
        let (zz, pp, kk) = bilinear_zpk(&z, &p, k, fs);
        if nargout >= 3 {
            return Ok(vec![super::ccol(&zz), super::ccol(&pp), Value::num(kk)]);
        }
        let (b, aa) = zpk_to_tf(&zz, &pp, kk);
        return Ok(vec![rrow(b), rrow(aa)]);
    }
    // transfer function form: roots through the engine
    let b = cvec(&a[0], "bilinear")?;
    let aa = cvec(&a[1], "bilinear")?;
    let fs = if a.len() >= 3 {
        arg_f64(&a[2], "bilinear")?
    } else {
        1.0
    };
    let bz = super::roots_of(it, &b)?;
    let az = super::roots_of(it, &aa)?;
    let first = |v: &[C]| v.iter().find(|c| **c != CZ).copied().unwrap_or(C1);
    let kk = cdiv(first(&b), first(&aa)).0;
    let (zz, pp, k2) = bilinear_zpk(&bz, &az, kk, fs);
    let (bo, ao) = zpk_to_tf(&zz, &pp, k2);
    let _ = from_c;
    Ok(vec![rrow(bo), rrow(ao)])
}

// ---------------------------------------------------------------------------
// Order estimation (lowpass/highpass/bandpass/bandstop)
// ---------------------------------------------------------------------------

fn ord_design(kind: Kind, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let name = match kind {
        Kind::Butter => "buttord",
        Kind::Cheby1 => "cheb1ord",
        _ => "cheb2ord",
    };
    need(&a, 4, name)?;
    let wp = rvec(&a[0], name)?;
    let ws = rvec(&a[1], name)?;
    let rp = arg_f64(&a[2], name)?;
    let rs = arg_f64(&a[3], name)?;
    let analog = a.get(4).and_then(opt_string).is_some_and(|s| s == "s");
    if wp.len() != ws.len() || wp.is_empty() || wp.len() > 2 {
        return Err(e(
            "signal:ord:edges",
            format!(
                "{}: Wp and Ws must be scalars or two-element vectors of equal size.",
                name
            ),
        ));
    }
    if !analog && wp.iter().chain(&ws).any(|w| !(*w > 0.0 && *w < 1.0)) {
        return Err(e(
            "signal:ord:edges",
            format!("{}: the edge frequencies must be between 0 and 1.", name),
        ));
    }
    let warp = |w: f64| if analog { w } else { (PI * w / 2.0).tan() };
    let wpw: Vec<f64> = wp.iter().map(|w| warp(*w)).collect();
    let wsw: Vec<f64> = ws.iter().map(|w| warp(*w)).collect();
    // Band type and analog lowpass stopband edge (passband edge = 1).
    let (kind_band, wa): (Band, f64);
    if wp.len() == 1 {
        if wp[0] < ws[0] {
            kind_band = Band::Low;
            wa = wsw[0] / wpw[0];
        } else {
            kind_band = Band::High;
            wa = wpw[0] / wsw[0];
        }
    } else {
        let w0 = wpw[0] * wpw[1];
        let bw = wpw[1] - wpw[0];
        if ws[0] < wp[0] {
            kind_band = Band::Pass;
            let t = |w: f64| ((w * w - w0) / (w * bw)).abs();
            wa = t(wsw[0]).min(t(wsw[1]));
        } else {
            kind_band = Band::Stop;
            let t = |w: f64| ((w * bw) / (w * w - w0)).abs();
            wa = t(wsw[0]).min(t(wsw[1]));
        }
    }
    let gp = 10f64.powf(rp / 10.0) - 1.0;
    let gs = 10f64.powf(rs / 10.0) - 1.0;
    let nreal = match kind {
        Kind::Butter => (gs / gp).log10() / (2.0 * wa.log10()),
        _ => ((gs / gp).sqrt() + ((gs / gp) - 1.0).max(0.0).sqrt()).acosh() / wa.acosh(),
    };
    let n = nreal.ceil().max(1.0);
    // natural frequency
    let wn_lp = match kind {
        Kind::Butter => 1.0 / gp.powf(1.0 / (2.0 * n)),
        Kind::Cheby1 => 1.0,
        _ => {
            // Chebyshev II: stopband edge fixed
            let t = ((gs).sqrt() + (gs - 1.0).max(0.0).sqrt()).acosh() / n;
            let _ = t;
            wa / (((gs.sqrt()).acosh() / n).cosh())
        }
    };
    let to_digital = |w: f64| if analog { w } else { 2.0 / PI * w.atan() };
    let wn: Vec<f64> = match (kind_band, kind) {
        (Band::Low, Kind::Cheby2) => vec![to_digital(wpw[0] * wn_lp)],
        (Band::Low, _) => vec![to_digital(wpw[0] * wn_lp)],
        (Band::High, _) => vec![to_digital(wpw[0] / wn_lp)],
        _ => {
            let w0 = (wpw[0] * wpw[1]).sqrt();
            let bw = (wpw[1] - wpw[0]) * wn_lp;
            let lo = (-bw / 2.0 + (bw * bw / 4.0 + w0 * w0).sqrt()).max(1e-300);
            let hi = bw / 2.0 + (bw * bw / 4.0 + w0 * w0).sqrt();
            if kind_band == Band::Pass {
                vec![to_digital(lo), to_digital(hi)]
            } else {
                let bw = (wpw[1] - wpw[0]) / wn_lp;
                let lo = -bw / 2.0 + (bw * bw / 4.0 + w0 * w0).sqrt();
                let hi = bw / 2.0 + (bw * bw / 4.0 + w0 * w0).sqrt();
                vec![to_digital(lo), to_digital(hi)]
            }
        }
    };
    Ok(vec![Value::num(n), rrow(wn)])
}
