//! `upsample`, `downsample`, `decimate`, `resample`, `findpeaks`.

use indy_value::Value;

use super::windows::kaiser;
use super::{C, CZ, columns, e, from_columns, opt_string, rrow, rvec};
use crate::builtins::{arg_f64, need, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "upsample", upsample);
    reg(it, "downsample", downsample);
    reg(it, "decimate", decimate);
    reg(it, "resample", resample);
    reg(it, "findpeaks", findpeaks);
    reg(it, "interp", interp);
    reg(it, "square", square);
    reg(it, "sawtooth", sawtooth);
    reg(it, "chirp", chirp);
    reg(it, "rms", rms);
    reg(it, "db2mag", |_, a, _| {
        db_map(a, "db2mag", |x| 10f64.powf(x / 20.0))
    });
    reg(it, "mag2db", |_, a, _| {
        db_map(a, "mag2db", |x| 20.0 * x.log10())
    });
    reg(it, "db2pow", |_, a, _| {
        db_map(a, "db2pow", |x| 10f64.powf(x / 10.0))
    });
    reg(it, "pow2db", |_, a, _| {
        db_map(a, "pow2db", |x| 10.0 * x.log10())
    });
}

fn db_map(a: Vec<Value>, f: &str, g: fn(f64) -> f64) -> R<Vec<Value>> {
    need(&a, 1, f)?;
    let n = crate::builtins::arg_num(&a[0], f)?;
    if n.is_complex() {
        return Err(e(
            "signal:db:complex",
            format!("{}: input must be real.", f),
        ));
    }
    let v: Vec<f64> = n.re.iter().map(|x| g(*x)).collect();
    Ok(vec![Value::Num(indy_value::Num::real(n.dims, v))])
}

fn real_map(a: &Value, f: &str, g: impl Fn(f64) -> f64) -> R<Value> {
    let n = crate::builtins::arg_num(a, f)?;
    let v: Vec<f64> = n.re.iter().map(|x| g(*x)).collect();
    Ok(Value::Num(indy_value::Num::real(n.dims, v)))
}

fn square(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "square")?;
    let duty = match a.get(1) {
        Some(v) => arg_f64(v, "square")? / 100.0,
        None => 0.5,
    };
    let tau = 2.0 * std::f64::consts::PI;
    Ok(vec![real_map(&a[0], "square", |t| {
        if t.rem_euclid(tau) < duty * tau {
            1.0
        } else {
            -1.0
        }
    })?])
}

fn sawtooth(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "sawtooth")?;
    let w = match a.get(1) {
        Some(v) => arg_f64(v, "sawtooth")?,
        None => 1.0,
    };
    if !(0.0..=1.0).contains(&w) {
        return Err(e(
            "signal:sawtooth:width",
            "sawtooth: WIDTH must be between 0 and 1.",
        ));
    }
    let pi = std::f64::consts::PI;
    Ok(vec![real_map(&a[0], "sawtooth", |t| {
        let tn = t.rem_euclid(2.0 * pi);
        if tn < w * 2.0 * pi {
            tn / (pi * w) - 1.0
        } else {
            (pi * (w + 1.0) - tn) / (pi * (1.0 - w))
        }
    })?])
}

fn chirp(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 4, "chirp")?;
    let f0 = arg_f64(&a[1], "chirp")?;
    let t1 = arg_f64(&a[2], "chirp")?;
    let f1 = arg_f64(&a[3], "chirp")?;
    let method = match a.get(4).and_then(opt_string) {
        Some(s) => s.to_ascii_lowercase(),
        None => "linear".to_string(),
    };
    let phi = match a.get(5) {
        Some(v) => arg_f64(v, "chirp")?,
        None => 0.0,
    };
    let tau = 2.0 * std::f64::consts::PI;
    let ph = phi.to_radians();
    let g: Box<dyn Fn(f64) -> f64> = match method.as_str() {
        "linear" | "li" => {
            let beta = (f1 - f0) / t1;
            Box::new(move |t| f0 * t + beta / 2.0 * t * t)
        }
        "quadratic" | "q" => {
            let beta = (f1 - f0) / (t1 * t1);
            Box::new(move |t| f0 * t + beta / 3.0 * t * t * t)
        }
        "logarithmic" | "log" | "lo" => {
            if f0 <= 0.0 || f1 <= 0.0 {
                return Err(e(
                    "signal:chirp:freq",
                    "chirp: logarithmic sweeps need positive frequencies.",
                ));
            }
            if f0 == f1 {
                Box::new(move |t| f0 * t)
            } else {
                let b = (f1 / f0).powf(1.0 / t1);
                Box::new(move |t| f0 * (b.powf(t) - 1.0) / b.ln())
            }
        }
        _ => {
            return Err(e(
                "signal:chirp:method",
                format!(
                    "chirp: method '{}' is not supported (linear, quadratic, logarithmic).",
                    method
                ),
            ));
        }
    };
    Ok(vec![real_map(&a[0], "chirp", |t| (tau * g(t) + ph).cos())?])
}

fn rms(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "rms")?;
    let (cols, row, _) = columns(&a[0], "rms")?;
    let vals: Vec<f64> = if row {
        let c = &cols[0];
        vec![(c.iter().map(|z| z.0 * z.0 + z.1 * z.1).sum::<f64>() / c.len() as f64).sqrt()]
    } else {
        cols.iter()
            .map(|c| (c.iter().map(|z| z.0 * z.0 + z.1 * z.1).sum::<f64>() / c.len() as f64).sqrt())
            .collect()
    };
    Ok(vec![Value::row(vals)])
}

fn interp(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "interp")?;
    let r = pos_int(&a[1], "interp", "R")?;
    let n = match a.get(2) {
        Some(v) => pos_int(v, "interp", "N")?,
        None => 4,
    };
    let alpha = match a.get(3) {
        Some(v) => arg_f64(v, "interp")?,
        None => 0.5,
    };
    if !(alpha > 0.0 && alpha <= 1.0) {
        return Err(e(
            "signal:interp:cutoff",
            "interp: CUTOFF must be in (0, 1].",
        ));
    }
    let single = a[0].class_name() == "single";
    let (cols, row, _) = columns(&a[0], "interp")?;
    if cols.iter().flatten().any(|c| c.1 != 0.0) {
        return Err(e(
            "signal:interp:complex",
            "interp: complex input is not supported.",
        ));
    }
    let len = cols.first().map_or(0, |c| c.len());
    if len <= 2 * n {
        return Err(e(
            "signal:interp:InvalidDimensions",
            "interp: the input must have more than 2*N samples.",
        ));
    }
    let out: Vec<Vec<C>> = if r == 1 {
        cols.clone()
    } else {
        // Band-limited interpolation: least-squares lowpass of length 2*N*R-1
        // (passband to ALPHA/R, stop from 1/R), gain R, applied to the
        // zero-stuffed signal; the group delay is removed.
        let taps = 2 * n * r - 1;
        let fv = Value::row(vec![0.0, alpha / r as f64, 1.0 / r as f64, 1.0]);
        let av = Value::row(vec![1.0, 1.0, 0.0, 0.0]);
        let h0 = it.call_named("firls", vec![Value::num((taps - 1) as f64), fv, av], 1)?;
        let h: Vec<f64> = rvec(&h0[0], "interp")?
            .iter()
            .map(|x| x * r as f64)
            .collect();
        let delay = n * r - 1;
        cols.iter()
            .map(|c| {
                let x: Vec<f64> = c.iter().map(|z| z.0).collect();
                let m = x.len() * r;
                (0..m)
                    .map(|k| {
                        // y[k] = sum_j h[k + delay - j*r] x[j]
                        let center = k + delay;
                        let jmax = (center / r).min(x.len() - 1);
                        let jmin = if center >= h.len() {
                            (center - h.len() + 1).div_ceil(r)
                        } else {
                            0
                        };
                        let mut s = 0.0;
                        for j in jmin..=jmax {
                            s += x[j] * h[center - j * r];
                        }
                        (s, 0.0)
                    })
                    .collect()
            })
            .collect()
    };
    let v = from_columns(&out, row);
    if single {
        let num = v
            .to_num()
            .ok_or_else(|| e("signal:interp:internal", "interp: internal type error."))?;
        return Ok(vec![Value::Num(
            num.with_class(indy_value::NumClass::Single),
        )]);
    }
    Ok(vec![v])
}

fn pos_int(v: &Value, f: &str, what: &str) -> R<usize> {
    let x = arg_f64(v, f)?;
    if x < 1.0 || x.fract() != 0.0 {
        return Err(e(
            "signal:resample:arg",
            format!("{}: {} must be a positive integer.", f, what),
        ));
    }
    Ok(x as usize)
}

fn phase_arg(a: &[Value], n: usize, f: &str) -> R<usize> {
    match a.get(2) {
        Some(v) => {
            let p = arg_f64(v, f)?;
            if p < 0.0 || p.fract() != 0.0 || p >= n as f64 {
                return Err(e(
                    "signal:resample:phase",
                    format!("{}: PHASE must be an integer between 0 and N-1.", f),
                ));
            }
            Ok(p as usize)
        }
        None => Ok(0),
    }
}

fn upsample(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "upsample")?;
    let n = pos_int(&a[1], "upsample", "N")?;
    let ph = phase_arg(&a, n, "upsample")?;
    let (cols, row, _) = columns(&a[0], "upsample")?;
    let out: Vec<Vec<C>> = cols
        .iter()
        .map(|c| {
            let mut y = vec![CZ; c.len() * n];
            for (i, v) in c.iter().enumerate() {
                y[i * n + ph] = *v;
            }
            y
        })
        .collect();
    Ok(vec![from_columns(&out, row)])
}

fn downsample(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "downsample")?;
    let n = pos_int(&a[1], "downsample", "N")?;
    let ph = phase_arg(&a, n, "downsample")?;
    let (cols, row, _) = columns(&a[0], "downsample")?;
    let out: Vec<Vec<C>> = cols
        .iter()
        .map(|c| c.iter().skip(ph).step_by(n).copied().collect())
        .collect();
    Ok(vec![from_columns(&out, row)])
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn decimate(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "decimate")?;
    let r = pos_int(&a[1], "decimate", "R")?;
    let mut fir = false;
    let mut order: Option<usize> = None;
    for v in &a[2..] {
        match opt_string(v) {
            Some(s) if s.eq_ignore_ascii_case("fir") => fir = true,
            Some(s) if s.eq_ignore_ascii_case("iir") => fir = false,
            Some(s) => {
                return Err(e(
                    "signal:decimate:option",
                    format!("decimate: unknown option '{}'.", s),
                ));
            }
            None => order = Some(pos_int(v, "decimate", "N")?),
        }
    }
    if r == 1 {
        return Ok(vec![a[0].clone()]);
    }
    let (cols, row, _) = columns(&a[0], "decimate")?;
    if cols.iter().flatten().any(|c| c.1 != 0.0) {
        return Err(e(
            "signal:decimate:complex",
            "decimate: complex input is not supported.",
        ));
    }
    let mut out_cols = Vec::new();
    for c in &cols {
        let x: Vec<f64> = c.iter().map(|z| z.0).collect();
        let y = if fir {
            let n = order.unwrap_or(30);
            let b = it.call_named(
                "fir1",
                vec![Value::num(n as f64), Value::num(1.0 / r as f64)],
                1,
            )?;
            let bv = rvec(&b[0], "decimate")?;
            let delay = n / 2;
            let mut padded = x.clone();
            padded.extend(std::iter::repeat_n(0.0, delay));
            let f = super::filt::df2t(&bv, &[1.0], &padded, &[]).0;
            f[delay..].to_vec()
        } else {
            let n = order.unwrap_or(8);
            let mut cur = x.clone();
            // factor large ratios into stages of at most 13
            let mut left = r;
            let mut stages = Vec::new();
            while left > 13 {
                let f = (2..=13).rev().find(|f| left % f == 0).unwrap_or(13);
                stages.push(f);
                left /= f;
                if f == 13 && left % 13 != 0 && left > 13 {
                    // fall through; remaining handled by loop bound
                }
            }
            stages.push(left);
            let total: usize = stages.iter().product();
            if total != r {
                return Err(e(
                    "signal:decimate:ratio",
                    "decimate: cannot factor R into stages of at most 13.",
                ));
            }
            for st in stages {
                if st == 1 {
                    continue;
                }
                let ba = it.call_named(
                    "cheby1",
                    vec![
                        Value::num(n as f64),
                        Value::num(0.05),
                        Value::num(0.8 / st as f64),
                    ],
                    2,
                )?;
                let f = it.call_named(
                    "filtfilt",
                    vec![ba[0].clone(), ba[1].clone(), Value::col(cur)],
                    1,
                )?;
                let fv = rvec(&f[0], "decimate")?;
                cur = fv.into_iter().step_by(st).collect();
            }
            out_cols.push(cur.iter().map(|v| (*v, 0.0)).collect::<Vec<C>>());
            continue;
        };
        out_cols.push(y.into_iter().step_by(r).map(|v| (v, 0.0)).collect());
    }
    Ok(vec![from_columns(&out_cols, row)])
}

fn resample(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 3, "resample")?;
    let p0 = pos_int(&a[1], "resample", "P")?;
    let q0 = pos_int(&a[2], "resample", "Q")?;
    let g = gcd(p0, q0);
    let (p, q) = (p0 / g, q0 / g);
    let nn = match a.get(3) {
        Some(v) if !v.is_empty() && v.numel() == 1 => arg_f64(v, "resample")? as usize,
        _ => 10,
    };
    let beta = match a.get(4) {
        Some(v) => arg_f64(v, "resample")?,
        None => 5.0,
    };
    let (cols, row, _) = columns(&a[0], "resample")?;
    if cols.iter().flatten().any(|c| c.1 != 0.0) {
        return Err(e(
            "signal:resample:complex",
            "resample: complex input is not supported.",
        ));
    }
    let h: Vec<f64> = if p == 1 && q == 1 {
        vec![1.0]
    } else if let Some(v) = a.get(3).filter(|v| v.numel() > 1) {
        rvec(v, "resample")?
    } else {
        let pq = p.max(q);
        let fc = 0.5 / pq as f64;
        let l = 2 * nn * pq + 1;
        let fv = Value::row(vec![0.0, 2.0 * fc, 2.0 * fc, 1.0]);
        let av = Value::row(vec![1.0, 1.0, 0.0, 0.0]);
        let h0 = it.call_named("firls", vec![Value::num((l - 1) as f64), fv, av], 1)?;
        let hv = rvec(&h0[0], "resample")?;
        let w = kaiser(l, beta);
        let mut h: Vec<f64> = hv.iter().zip(&w).map(|(x, y)| x * y).collect();
        let s: f64 = h.iter().sum();
        for x in h.iter_mut() {
            *x *= p as f64 / s;
        }
        h
    };
    let lh = h.len();
    // delay compensation (half the filter length, rounded so the polyphase grid lines up)
    let lhalf0 = (lh - 1) / 2;
    let nz = q - (lhalf0 % q);
    let lhalf = lhalf0 + nz;
    let delay = lhalf.div_ceil(q);
    let mut out_cols = Vec::new();
    for c in &cols {
        let x: Vec<f64> = c.iter().map(|z| z.0).collect();
        let lx = x.len();
        let target = (lx * p).div_ceil(q);
        let mut nz1 = 0;
        while ((lx.saturating_sub(1)) * p + lh + nz + nz1).div_ceil(q) < target + delay {
            nz1 += 1;
            if nz1 > 8 * q {
                break;
            }
        }
        let mut hh = vec![0.0; nz];
        hh.extend_from_slice(&h);
        hh.extend(std::iter::repeat_n(0.0, nz1));
        let full_len = ((lx.saturating_sub(1)) * p + hh.len()).div_ceil(q);
        let mut y = vec![0.0; full_len];
        for (m, ym) in y.iter_mut().enumerate() {
            let idx = m * q;
            // sum over x[j] * hh[idx - j*p]
            let jmax = (idx / p).min(lx.saturating_sub(1));
            let jmin = if idx >= hh.len() {
                (idx - hh.len() + 1).div_ceil(p)
            } else {
                0
            };
            let mut s = 0.0;
            for j in jmin..=jmax {
                let k = idx - j * p;
                if k < hh.len() {
                    s += x[j] * hh[k];
                }
            }
            *ym = s;
        }
        let end = (delay + target).min(y.len());
        out_cols.push(
            y[delay.min(end)..end]
                .iter()
                .map(|v| (*v, 0.0))
                .collect::<Vec<C>>(),
        );
    }
    Ok(vec![from_columns(&out_cols, row)])
}

// ---------------------------------------------------------------------------
// findpeaks
// ---------------------------------------------------------------------------

struct Peak {
    idx: usize,
    height: f64,
    prom: f64,
    width: f64,
}

fn all_peaks(y: &[f64]) -> Vec<usize> {
    // collapse runs of equal values; a peak is a run higher than both neighbours
    let n = y.len();
    let mut starts = vec![0usize];
    for i in 1..n {
        if y[i] != y[i - 1] {
            starts.push(i);
        }
    }
    let mut out = Vec::new();
    for k in 1..starts.len().saturating_sub(1) {
        let v = y[starts[k]];
        if v > y[starts[k - 1]] && v > y[starts[k + 1]] {
            out.push(starts[k]);
        }
    }
    out
}

fn prominence(y: &[f64], i: usize) -> (f64, usize, usize) {
    let n = y.len();
    let v = y[i];
    let mut lmin = v;
    let mut lb = i;
    let mut j = i;
    while j > 0 {
        j -= 1;
        if y[j] > v {
            break;
        }
        if y[j] < lmin {
            lmin = y[j];
            lb = j;
        }
    }
    let mut rmin = v;
    let mut rb = i;
    let mut j = i;
    while j + 1 < n {
        j += 1;
        if y[j] > v {
            break;
        }
        if y[j] < rmin {
            rmin = y[j];
            rb = j;
        }
    }
    (v - lmin.max(rmin), lb, rb)
}

fn half_width(y: &[f64], x: &[f64], i: usize, prom: f64, lb: usize, rb: usize) -> f64 {
    let level = y[i] - prom / 2.0;
    let mut l = i;
    while l > lb && y[l] > level {
        l -= 1;
    }
    let xl = if y[l] < level && l < i {
        x[l] + (x[l + 1] - x[l]) * (level - y[l]) / (y[l + 1] - y[l])
    } else {
        x[l]
    };
    let mut r = i;
    while r < rb && y[r] > level {
        r += 1;
    }
    let xr = if y[r] < level && r > i {
        x[r] - (x[r] - x[r - 1]) * (level - y[r]) / (y[r - 1] - y[r])
    } else {
        x[r]
    };
    xr - xl
}

fn findpeaks(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "findpeaks")?;
    let y = rvec(&a[0], "findpeaks")?;
    let row = a[0].dims_ref()[0] == 1;
    if a[0].dims_ref().len() != 2 || (a[0].dims_ref()[0] != 1 && a[0].dims_ref()[1] != 1) {
        return Err(e(
            "signal:findpeaks:input",
            "findpeaks: the data must be a vector.",
        ));
    }
    if y.len() < 3 {
        return Err(e(
            "signal:findpeaks:short",
            "findpeaks: the data must have at least three samples.",
        ));
    }
    let mut x: Vec<f64> = (1..=y.len()).map(|i| i as f64).collect();
    let mut rest = &a[1..];
    if let Some(v) = rest.first()
        && opt_string(v).is_none()
    {
        if v.numel() == 1 {
            let fs = arg_f64(v, "findpeaks")?;
            x = (0..y.len()).map(|i| i as f64 / fs).collect();
        } else {
            let xv = rvec(v, "findpeaks")?;
            if xv.len() != y.len() {
                return Err(e(
                    "signal:findpeaks:x",
                    "findpeaks: X must have the same length as the data.",
                ));
            }
            x = xv;
        }
        rest = &rest[1..];
    }
    let mut min_h = f64::NEG_INFINITY;
    let mut min_d = 0.0;
    let mut min_p = 0.0;
    let mut thr = 0.0;
    let mut npk = usize::MAX;
    let mut sort = "none".to_string();
    let mut min_w = 0.0;
    let mut max_w = f64::INFINITY;
    if !rest.len().is_multiple_of(2) {
        return Err(e(
            "signal:findpeaks:pairs",
            "findpeaks: options must come in name/value pairs.",
        ));
    }
    for kv in rest.chunks(2) {
        let name = super::str_arg(&kv[0], "findpeaks")?.to_ascii_lowercase();
        match name.as_str() {
            "minpeakheight" => min_h = arg_f64(&kv[1], "findpeaks")?,
            "minpeakdistance" => min_d = arg_f64(&kv[1], "findpeaks")?,
            "minpeakprominence" => min_p = arg_f64(&kv[1], "findpeaks")?,
            "threshold" => thr = arg_f64(&kv[1], "findpeaks")?,
            "npeaks" => npk = arg_f64(&kv[1], "findpeaks")? as usize,
            "sortstr" => sort = super::str_arg(&kv[1], "findpeaks")?.to_ascii_lowercase(),
            "minpeakwidth" => min_w = arg_f64(&kv[1], "findpeaks")?,
            "maxpeakwidth" => max_w = arg_f64(&kv[1], "findpeaks")?,
            "widthreference" | "annotate" => {}
            _ => {
                return Err(e(
                    "signal:findpeaks:option",
                    format!("findpeaks: unknown option '{}'.", name),
                ));
            }
        }
    }
    let n = y.len();
    let mut peaks: Vec<Peak> = Vec::new();
    for i in all_peaks(&y) {
        let h = y[i];
        if h < min_h {
            continue;
        }
        if thr > 0.0 {
            let lo = h - y[i - 1];
            let hi = h - y[(i + 1).min(n - 1)];
            if lo.min(hi) < thr {
                continue;
            }
        }
        let (prom, lb, rb) = prominence(&y, i);
        if prom < min_p {
            continue;
        }
        let width = half_width(&y, &x, i, prom, lb, rb);
        if width < min_w || width > max_w {
            continue;
        }
        peaks.push(Peak {
            idx: i,
            height: h,
            prom,
            width,
        });
    }
    if min_d > 0.0 {
        let mut order: Vec<usize> = (0..peaks.len()).collect();
        order.sort_by(|&p, &q| peaks[q].height.total_cmp(&peaks[p].height).then(p.cmp(&q)));
        let mut keep = vec![true; peaks.len()];
        for &oi in &order {
            if !keep[oi] {
                continue;
            }
            for (j, kj) in keep.iter_mut().enumerate() {
                if j != oi && *kj && (x[peaks[j].idx] - x[peaks[oi].idx]).abs() < min_d {
                    *kj = false;
                }
            }
        }
        let mut k = 0;
        peaks.retain(|_| {
            k += 1;
            keep[k - 1]
        });
    }
    match sort.as_str() {
        "descend" => peaks.sort_by(|p, q| q.height.total_cmp(&p.height)),
        "ascend" => peaks.sort_by(|p, q| p.height.total_cmp(&q.height)),
        "none" => {}
        _ => {
            return Err(e(
                "signal:findpeaks:sort",
                "findpeaks: SortStr must be 'ascend', 'descend' or 'none'.",
            ));
        }
    }
    peaks.truncate(npk);
    let mk = |v: Vec<f64>| if row { rrow(v) } else { Value::col(v) };
    if nargout == 0 {
        let locs: Vec<f64> = peaks.iter().map(|p| x[p.idx]).collect();
        let hs: Vec<f64> = peaks.iter().map(|p| p.height).collect();
        it.call_named("plot", vec![mk(x.clone()), mk(y.clone())], 0)?;
        it.call_named("hold", vec![Value::str("on")], 0)?;
        it.call_named("plot", vec![mk(locs), mk(hs), Value::str("v")], 0)?;
        return Ok(vec![]);
    }
    Ok(vec![
        mk(peaks.iter().map(|p| p.height).collect()),
        mk(peaks.iter().map(|p| x[p.idx]).collect()),
        mk(peaks.iter().map(|p| p.width).collect()),
        mk(peaks.iter().map(|p| p.prom).collect()),
    ])
}
