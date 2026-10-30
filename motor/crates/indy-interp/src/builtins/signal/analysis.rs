//! Analysis: `freqz`, `grpdelay`, `impz`, zero/pole/gain and
//! second-order-section conversions.

use std::f64::consts::PI;

use indy_value::Value;

use super::{
    C, C1, CZ, cabs, cadd, cconj, cdiv, cexp, cmul, cscale, csub, cvec, e, opt_string, pmul,
    poly_of_roots, pval, rcol, real_part, roots_of, rrow, rvec,
};
use crate::builtins::{arg_f64, need, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "freqz", freqz);
    reg(it, "grpdelay", grpdelay);
    reg(it, "impz", impz);
    reg(it, "tf2zp", tf2zp);
    reg(it, "zp2tf", zp2tf);
    reg(it, "tf2sos", tf2sos);
    reg(it, "zp2sos", zp2sos);
    reg(it, "sos2tf", sos2tf);
    reg(it, "sos2zp", sos2zp);
    reg(it, "zplane", zplane);
}

/// Parsed `(n | w, 'whole', Fs)` tail shared by freqz/grpdelay.
pub struct FreqSpec {
    /// Frequencies in rad/sample.
    pub w: Vec<f64>,
    /// Sampling frequency if given.
    pub fs: Option<f64>,
}

pub fn parse_freq(rest: &[Value], f: &str) -> R<FreqSpec> {
    let mut n: Option<usize> = None;
    let mut wvec: Option<Vec<f64>> = None;
    let mut whole = false;
    let mut fs = None;
    let mut nums: Vec<&Value> = Vec::new();
    for v in rest {
        match opt_string(v) {
            Some(s) => match s.to_ascii_lowercase().as_str() {
                "whole" => whole = true,
                "half" => whole = false,
                _ => {
                    return Err(e(
                        "signal:freqz:option",
                        format!("{}: unknown option '{}'.", f, s),
                    ));
                }
            },
            None => nums.push(v),
        }
    }
    if let Some(v) = nums.first()
        && !v.is_empty()
    {
        if v.numel() == 1 {
            let x = arg_f64(v, f)?;
            if x < 1.0 || x.fract() != 0.0 {
                return Err(e(
                    "signal:freqz:n",
                    format!("{}: N must be a positive integer.", f),
                ));
            }
            n = Some(x as usize);
        } else {
            wvec = Some(rvec(v, f)?);
        }
    }
    if let Some(v) = nums.get(1) {
        fs = Some(arg_f64(v, f)?);
    }
    let w = match wvec {
        Some(v) => match fs {
            Some(fsv) => v.iter().map(|x| 2.0 * PI * x / fsv).collect(),
            None => v,
        },
        None => {
            let n = n.unwrap_or(512);
            let span = if whole { 2.0 * PI } else { PI };
            (0..n).map(|i| i as f64 * span / n as f64).collect()
        }
    };
    Ok(FreqSpec { w, fs })
}

pub fn eval_tf(b: &[C], a: &[C], w: f64) -> C {
    // polynomials in z^-1 evaluated at e^{-jw}
    let zi = (w.cos(), -w.sin());
    let mut nb = CZ;
    for c in b.iter().rev() {
        nb = cadd(cmul(nb, zi), *c);
    }
    let mut na = CZ;
    for c in a.iter().rev() {
        na = cadd(cmul(na, zi), *c);
    }
    cdiv(nb, na)
}

fn ba_args(a: &[Value], f: &str) -> R<(Vec<C>, Vec<C>)> {
    need(a, 1, f)?;
    let b = cvec(&a[0], f)?;
    let den = match a.get(1) {
        Some(v) if !v.is_empty() && opt_string(v).is_none() && v.numel() >= 1 => cvec(v, f)?,
        _ => vec![C1],
    };
    if b.is_empty() || den.is_empty() {
        return Err(e(
            "signal:freqz:empty",
            format!("{}: B and A must not be empty.", f),
        ));
    }
    if den[0] == CZ {
        return Err(e(
            "signal:freqz:a0",
            format!("{}: the first element of A must be nonzero.", f),
        ));
    }
    Ok((b, den))
}

fn ba_rest(a: &[Value]) -> &[Value] {
    // freqz(b, n) is not valid MATLAB: the second argument is always A
    if a.len() > 2 { &a[2..] } else { &[] }
}

fn freqz(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let (b, den) = ba_args(&a, "freqz")?;
    let spec = parse_freq(ba_rest(&a), "freqz")?;
    let h: Vec<C> = spec.w.iter().map(|w| eval_tf(&b, &den, *w)).collect();
    let freq: Vec<f64> = match spec.fs {
        Some(fs) => spec.w.iter().map(|w| w * fs / (2.0 * PI)).collect(),
        None => spec.w.clone(),
    };
    if nargout == 0 {
        super::plots::freqz_plot(it, &h, &freq, spec.fs.is_some())?;
        return Ok(vec![]);
    }
    Ok(vec![super::ccol(&h), rcol(freq)])
}

fn grpdelay(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let (b, den) = ba_args(&a, "grpdelay")?;
    let spec = parse_freq(ba_rest(&a), "grpdelay")?;
    let rev: Vec<C> = den.iter().rev().map(|c| cconj(*c)).collect();
    let c = pmul(&b, &rev);
    let r = den.len() as f64 - 1.0;
    let gd: Vec<f64> = spec
        .w
        .iter()
        .map(|w| {
            let (mut num, mut dd) = (CZ, CZ);
            for (k, ck) in c.iter().enumerate() {
                let ph = cexp((0.0, -w * k as f64));
                dd = cadd(dd, cmul(*ck, ph));
                num = cadd(num, cscale(cmul(*ck, ph), k as f64));
            }
            if cabs(dd) < 1e-10 * cabs(num).max(1.0) {
                0.0
            } else {
                cdiv(num, dd).0 - r
            }
        })
        .collect();
    let freq: Vec<f64> = match spec.fs {
        Some(fs) => spec.w.iter().map(|w| w * fs / (2.0 * PI)).collect(),
        None => spec.w.clone(),
    };
    Ok(vec![rcol(gd), rcol(freq)])
}

fn impulse(b: &[C], a: &[C], n: usize) -> Vec<C> {
    let a0 = a[0];
    let mut y = vec![CZ; n];
    for i in 0..n {
        let mut acc = if i < b.len() { b[i] } else { CZ };
        for k in 1..a.len().min(i + 1) {
            acc = csub(acc, cmul(a[k], y[i - k]));
        }
        y[i] = cdiv(acc, a0);
    }
    y
}

fn impz(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let (b, den) = ba_args(&a, "impz")?;
    let mut n: Option<usize> = None;
    let mut fs = 1.0;
    let mut have_fs = false;
    if let Some(v) = a.get(2)
        && !v.is_empty()
    {
        let x = arg_f64(v, "impz")?;
        if x < 1.0 || x.fract() != 0.0 {
            return Err(e("signal:impz:n", "impz: N must be a positive integer."));
        }
        n = Some(x as usize);
    }
    if let Some(v) = a.get(3) {
        fs = arg_f64(v, "impz")?;
        have_fs = true;
    }
    let n = match n {
        Some(n) => n,
        None => default_impz_len(it, &b, &den)?,
    };
    let h = impulse(&b, &den, n);
    let t: Vec<f64> = (0..n).map(|i| i as f64 / fs).collect();
    if nargout == 0 {
        super::plots::impz_plot(it, &h, &t, have_fs)?;
        return Ok(vec![]);
    }
    Ok(vec![super::ccol(&h), rcol(t)])
}

/// FIR: the number of taps. IIR: samples until the slowest pole decays
/// to 5e-5 (stable) or a fixed span for marginal/unstable filters; never
/// shorter than the numerator.
fn default_impz_len(it: &mut Interp, b: &[C], a: &[C]) -> R<usize> {
    let a_trim: Vec<C> = {
        let last = a.iter().rposition(|c| *c != CZ).unwrap_or(0);
        a[..=last].to_vec()
    };
    if a_trim.len() <= 1 {
        return Ok(b.len().max(1));
    }
    let p = roots_of(it, a)?;
    let rmax = p.iter().map(|c| cabs(*c)).fold(0.0_f64, f64::max);
    let len = if rmax < 1.0 - 1e-9 {
        ((5e-5_f64).ln() / rmax.ln()).ceil() as usize
    } else {
        100
    };
    Ok(len.clamp(b.len().max(a.len()), 100_000).max(10))
}

// ---------------------------------------------------------------------------
// zero / pole / gain
// ---------------------------------------------------------------------------

fn first_nonzero(v: &[C]) -> C {
    v.iter().find(|c| **c != CZ).copied().unwrap_or(C1)
}

fn tf2zp(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "tf2zp")?;
    let b = cvec(&a[0], "tf2zp")?;
    let den = cvec(&a[1], "tf2zp")?;
    let z = roots_of(it, &b)?;
    let p = roots_of(it, &den)?;
    let k = cdiv(first_nonzero(&b), first_nonzero(&den));
    Ok(vec![
        super::ccol(&z),
        super::ccol(&p),
        super::from_c(vec![1, 1], &[k]),
    ])
}

/// Polynomial from conjugate-closed roots, real if the roots allow it.
pub fn poly_real_if_closed(r: &[C]) -> Value {
    let p = poly_of_roots(r);
    if conj_closed(r) {
        rrow(real_part(&p))
    } else {
        super::crow(&p)
    }
}

pub fn conj_closed(r: &[C]) -> bool {
    let mut used = vec![false; r.len()];
    for i in 0..r.len() {
        if used[i] {
            continue;
        }
        let z = r[i];
        let tol = 1e-8 * cabs(z).max(1.0);
        if z.1.abs() <= tol {
            used[i] = true;
            continue;
        }
        let mut found = false;
        for j in 0..r.len() {
            if !used[j] && j != i && cabs(csub(r[j], cconj(z))) <= tol {
                used[i] = true;
                used[j] = true;
                found = true;
                break;
            }
        }
        if !found {
            return false;
        }
    }
    true
}

fn zp2tf(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 3, "zp2tf")?;
    let z = cvec(&a[0], "zp2tf")?;
    let p = cvec(&a[1], "zp2tf")?;
    let k = cvec(&a[2], "zp2tf")?;
    let k = k.first().copied().unwrap_or(C1);
    let bp = poly_of_roots(&z);
    let b: Vec<C> = bp.iter().map(|c| cmul(*c, k)).collect();
    let bv = if conj_closed(&z) && k.1 == 0.0 {
        rrow(real_part(&b))
    } else {
        super::crow(&b)
    };
    Ok(vec![bv, poly_real_if_closed(&p)])
}

// ---------------------------------------------------------------------------
// second-order sections
// ---------------------------------------------------------------------------

/// Pair the roots into sections (each section holds 0..=2 roots), complex
/// conjugates together, remaining real roots two by two.
fn conj_pairs(r: &[C]) -> (Vec<[C; 2]>, Vec<C>) {
    let mut pairs = Vec::new();
    let mut reals = Vec::new();
    let mut used = vec![false; r.len()];
    for i in 0..r.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let z = r[i];
        let tol = 1e-8 * cabs(z).max(1.0);
        if z.1.abs() <= tol {
            reals.push((z.0, 0.0));
            continue;
        }
        let mut best: Option<usize> = None;
        for j in 0..r.len() {
            if !used[j] && cabs(csub(r[j], cconj(z))) <= tol * 10.0 {
                best = Some(j);
                break;
            }
        }
        match best {
            Some(j) => {
                used[j] = true;
                let (hi, lo) = if z.1 >= 0.0 {
                    (z, cconj(z))
                } else {
                    (cconj(z), z)
                };
                let _ = lo;
                pairs.push([hi, cconj(hi)]);
            }
            None => reals.push(z),
        }
    }
    (pairs, reals)
}

/// Build second-order sections (rows `[b0 b1 b2 1 a1 a2]`) and gain.
pub fn zpk_to_sos(z: &[C], p: &[C], k: f64, up: bool) -> (Vec<[f64; 6]>, f64) {
    let (mut zp, mut zr) = conj_pairs(z);
    let (mut pp, mut pr) = conj_pairs(p);
    // process poles closest to the unit circle first
    pp.sort_by(|x, y| cabs(y[0]).total_cmp(&cabs(x[0])));
    pr.sort_by(|x, y| cabs(*y).total_cmp(&cabs(*x)));
    let mut sections: Vec<([C; 2], [C; 2], usize, usize)> = Vec::new();
    // pole groups: complex pairs, then real pairs
    let mut groups: Vec<Vec<C>> = pp.iter().map(|x| x.to_vec()).collect();
    let mut rr = pr.clone();
    while !rr.is_empty() {
        let first = rr.remove(0);
        if rr.is_empty() {
            groups.push(vec![first]);
        } else {
            // pair with the real pole closest to it
            let j = (0..rr.len())
                .min_by(|&a, &b| {
                    (rr[a].0 - first.0)
                        .abs()
                        .total_cmp(&(rr[b].0 - first.0).abs())
                })
                .unwrap_or(0);
            let second = rr.remove(j);
            groups.push(vec![first, second]);
        }
    }
    // 'up': complex sections from the farthest pole to the closest to the
    // unit circle, real-pole sections last
    groups.sort_by(|x, y| {
        let cx = x[0].1 == 0.0;
        let cy = y[0].1 == 0.0;
        let rx = x.iter().map(|c| cabs(*c)).fold(0.0_f64, f64::max);
        let ry = y.iter().map(|c| cabs(*c)).fold(0.0_f64, f64::max);
        cx.cmp(&cy).then(rx.total_cmp(&ry))
    });
    let nsec = groups.len();
    for g in &groups {
        let pole_ref = g[0];
        let mut zs: Vec<C> = Vec::new();
        if g.len() == 2 && g[0].1 != 0.0 {
            // complex pole pair: take the nearest complex zero pair, else two real zeros
            if let Some((i, _)) = zp.iter().enumerate().min_by(|a, b| {
                cabs(csub(a.1[0], pole_ref)).total_cmp(&cabs(csub(b.1[0], pole_ref)))
            }) {
                let cp = zp.remove(i);
                zs = cp.to_vec();
            } else {
                take_real_zeros(&mut zr, pole_ref, &mut zs, 2);
            }
        } else {
            take_real_zeros(&mut zr, pole_ref, &mut zs, 2);
        }
        let mut sec = ([C1, CZ], [C1, CZ], zs.len(), g.len());
        let _ = &mut sec;
        sections.push((
            [
                g.first().copied().unwrap_or(CZ),
                g.get(1).copied().unwrap_or(CZ),
            ],
            [
                zs.first().copied().unwrap_or(CZ),
                zs.get(1).copied().unwrap_or(CZ),
            ],
            zs.len(),
            g.len(),
        ));
    }
    // leftover zero pairs (more zeros than poles are unusual): ignore
    let _ = (&zp, &zr);
    let mut rows: Vec<[f64; 6]> = Vec::new();
    for (pol, zer, nz, np) in &sections {
        let zroots: Vec<C> = zer[..*nz].to_vec();
        let proots: Vec<C> = pol[..*np].to_vec();
        let mut num = real_part(&poly_of_roots(&zroots));
        let mut den = real_part(&poly_of_roots(&proots));
        while num.len() < 3 {
            num.push(0.0);
        }
        while den.len() < 3 {
            den.push(0.0);
        }
        rows.push([num[0], num[1], num[2], den[0], den[1], den[2]]);
    }
    if rows.is_empty() {
        rows.push([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    }
    let _ = nsec;
    if !up {
        rows.reverse();
    }
    (rows, k)
}

fn take_real_zeros(zr: &mut Vec<C>, near: C, out: &mut Vec<C>, want: usize) {
    for _ in 0..want {
        if zr.is_empty() {
            break;
        }
        let i = (0..zr.len())
            .min_by(|&a, &b| cabs(csub(zr[a], near)).total_cmp(&cabs(csub(zr[b], near))))
            .unwrap_or(0);
        out.push(zr.remove(i));
    }
}

fn pad_zero_roots(z: &[C], p: &[C]) -> Vec<C> {
    // MATLAB pads with zeros at the origin so that #zeros == #poles
    let mut zz = z.to_vec();
    while zz.len() < p.len() {
        zz.push(CZ);
    }
    zz
}

fn sos_output(rows: Vec<[f64; 6]>, g: f64, nargout: usize) -> Vec<Value> {
    let mut rows = rows;
    if nargout < 2 {
        for x in rows[0].iter_mut().take(3) {
            *x *= g;
        }
    }
    let m = rows.len();
    let mut flat = vec![0.0; m * 6];
    for (i, r) in rows.iter().enumerate() {
        for (j, v) in r.iter().enumerate() {
            flat[j * m + i] = *v;
        }
    }
    let mut out = vec![Value::matrix(vec![m, 6], flat)];
    if nargout >= 2 {
        out.push(Value::num(g));
    }
    out
}

fn up_flag(a: &[Value], f: &str) -> R<bool> {
    let mut up = true;
    for v in a {
        if let Some(s) = opt_string(v) {
            match s.to_ascii_lowercase().as_str() {
                "up" => up = true,
                "down" => up = false,
                "none" | "inf" | "two" => {}
                _ => {
                    return Err(e(
                        "signal:sos:option",
                        format!("{}: unknown option '{}'.", f, s),
                    ));
                }
            }
        }
    }
    Ok(up)
}

fn tf2sos(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "tf2sos")?;
    let b = cvec(&a[0], "tf2sos")?;
    let den = cvec(&a[1], "tf2sos")?;
    let up = up_flag(&a[2..], "tf2sos")?;
    let z = roots_of(it, &b)?;
    let p = roots_of(it, &den)?;
    let k = cdiv(first_nonzero(&b), first_nonzero(&den)).0;
    let zz = pad_zero_roots(&z, &p);
    let (rows, g) = zpk_to_sos(&zz, &p, k, up);
    Ok(sos_output(rows, g, nargout))
}

fn zp2sos(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 3, "zp2sos")?;
    let z = cvec(&a[0], "zp2sos")?;
    let p = cvec(&a[1], "zp2sos")?;
    let k = arg_f64(&a[2], "zp2sos")?;
    let up = up_flag(&a[3..], "zp2sos")?;
    let zz = pad_zero_roots(&z, &p);
    let (rows, g) = zpk_to_sos(&zz, &p, k, up);
    Ok(sos_output(rows, g, nargout))
}

fn sos_rows(v: &Value, f: &str) -> R<Vec<[f64; 6]>> {
    let n = crate::builtins::arg_num(v, f)?;
    if n.dims.len() != 2 || n.dims[1] != 6 || n.dims[0] == 0 {
        return Err(e(
            "signal:sos:size",
            format!("{}: SOS must be an L-by-6 matrix.", f),
        ));
    }
    let m = n.dims[0];
    Ok((0..m)
        .map(|i| {
            let mut r = [0.0; 6];
            for (j, x) in r.iter_mut().enumerate() {
                *x = n.re[j * m + i];
            }
            r
        })
        .collect())
}

fn sos2tf(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "sos2tf")?;
    let rows = sos_rows(&a[0], "sos2tf")?;
    let g: f64 = match a.get(1) {
        Some(v) => rvec(v, "sos2tf")?.iter().product(),
        None => 1.0,
    };
    let mut b = vec![(g, 0.0)];
    let mut den = vec![C1];
    for r in &rows {
        let nb: Vec<C> = r[..3].iter().map(|x| (*x, 0.0)).collect();
        let na: Vec<C> = r[3..].iter().map(|x| (*x, 0.0)).collect();
        b = pmul(&b, &nb);
        den = pmul(&den, &na);
    }
    // a vanishing highest-order term (z^-2L) is dropped once, as MATLAB does
    if b.len() > 1 && b.last().is_some_and(|c| *c == CZ) {
        b.pop();
    }
    if den.len() > 1 && den.last().is_some_and(|c| *c == CZ) {
        den.pop();
    }
    Ok(vec![rrow(real_part(&b)), rrow(real_part(&den))])
}

fn sos2zp(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "sos2zp")?;
    let tf = sos2tf(it, a, 2)?;
    let b = cvec(&tf[0], "sos2zp")?;
    let den = cvec(&tf[1], "sos2zp")?;
    let z = roots_of(it, &b)?;
    let p = roots_of(it, &den)?;
    let k = cdiv(first_nonzero(&b), first_nonzero(&den));
    Ok(vec![
        super::ccol(&z),
        super::ccol(&p),
        super::from_c(vec![1, 1], &[k]),
    ])
}

fn zplane(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "zplane")?;
    let (z, p) = if a.len() >= 2
        && a[1].numel() >= 1
        && (a[0].dims_ref()[1] > 1 || a[0].dims_ref()[0] == 1)
        && !(a[0].dims_ref()[0] > 1 && a[1].dims_ref()[0] > 1 && false)
    {
        // transfer function form: zplane(b, a); column vectors are zeros/poles
        let col0 = a[0].dims_ref()[1] == 1 && a[0].dims_ref()[0] > 1;
        if col0 {
            (cvec(&a[0], "zplane")?, cvec(&a[1], "zplane")?)
        } else {
            let b = cvec(&a[0], "zplane")?;
            let den = cvec(&a[1], "zplane")?;
            (roots_of(it, &b)?, roots_of(it, &den)?)
        }
    } else if a.len() >= 2 {
        (cvec(&a[0], "zplane")?, cvec(&a[1], "zplane")?)
    } else {
        let b = cvec(&a[0], "zplane")?;
        (roots_of(it, &b)?, vec![])
    };
    // MATLAB's polynomial form pads with origin roots; `zplane(b)` with a = 1 has none.
    if nargout == 0 {
        super::plots::zplane_plot(it, &z, &p)?;
        return Ok(vec![]);
    }
    let _ = (pval(&[C1], C1), cexp((0.0, 0.0)));
    Ok(vec![super::ccol(&z), super::ccol(&p)])
}
