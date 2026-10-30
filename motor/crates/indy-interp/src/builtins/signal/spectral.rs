//! Spectral estimation: `hilbert`, `periodogram`, `pwelch`,
//! `spectrogram`, `xcorr`, `envelope`.

use std::f64::consts::PI;

use indy_value::Value;

use super::windows::hamming;
use super::{
    C, CZ, cabs, cadd, cconj, cmul, columns, cvec, e, from_columns, opt_string, rcol, rvec,
};
use crate::builtins::fft::dft;
use crate::builtins::{arg_f64, need, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "hilbert", hilbert);
    reg(it, "periodogram", periodogram);
    reg(it, "pwelch", pwelch);
    reg(it, "spectrogram", spectrogram);
    reg(it, "xcorr", xcorr);
    reg(it, "envelope", envelope);
}

fn idft(x: &[C]) -> Vec<C> {
    let n = x.len() as f64;
    dft(x, true)
        .into_iter()
        .map(|c| (c.0 / n, c.1 / n))
        .collect()
}

fn analytic(x: &[C], n: usize) -> Vec<C> {
    let mut v: Vec<C> = x.iter().take(n).copied().collect();
    v.resize(n, CZ);
    let xf = dft(&v, false);
    let mut h = vec![0.0; n];
    if n > 0 {
        h[0] = 1.0;
        if n.is_multiple_of(2) {
            h[n / 2] = 1.0;
            for x in h.iter_mut().take(n / 2).skip(1) {
                *x = 2.0;
            }
        } else {
            for x in h.iter_mut().take(n.div_ceil(2)).skip(1) {
                *x = 2.0;
            }
        }
    }
    let y: Vec<C> = xf.iter().zip(&h).map(|(c, g)| (c.0 * g, c.1 * g)).collect();
    idft(&y)
}

fn hilbert(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "hilbert")?;
    let (cols, row, _) = columns(&a[0], "hilbert")?;
    let len = cols.first().map_or(0, |c| c.len());
    let n = match a.get(1) {
        Some(v) if !v.is_empty() => {
            let x = arg_f64(v, "hilbert")?;
            if x < 1.0 || x.fract() != 0.0 {
                return Err(e(
                    "signal:hilbert:n",
                    "hilbert: N must be a positive integer.",
                ));
            }
            x as usize
        }
        _ => len,
    };
    if cols.iter().flatten().any(|c| c.1 != 0.0) {
        return Err(e("signal:hilbert:complex", "hilbert: input must be real."));
    }
    let out: Vec<Vec<C>> = cols.iter().map(|c| analytic(c, n)).collect();
    Ok(vec![from_columns(&out, row)])
}

// ---------------------------------------------------------------------------
// periodogram / pwelch / spectrogram
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Sides {
    One,
    Two,
    Centered,
}

struct Opts {
    sides: Option<Sides>,
    power: bool,
}

fn split_opts(rest: &[Value], f: &str) -> R<(Vec<Value>, Opts)> {
    let mut nums = Vec::new();
    let mut o = Opts {
        sides: None,
        power: false,
    };
    for v in rest {
        match opt_string(v) {
            Some(s) => match s.to_ascii_lowercase().as_str() {
                "onesided" => o.sides = Some(Sides::One),
                "twosided" => o.sides = Some(Sides::Two),
                "centered" => o.sides = Some(Sides::Centered),
                "power" => o.power = true,
                "psd" => o.power = false,
                "reassigned" | "yaxis" | "xaxis" | "minthreshold" => {}
                _ => {
                    return Err(e(
                        "signal:spectral:option",
                        format!("{}: unknown option '{}'.", f, s),
                    ));
                }
            },
            None => nums.push(v.clone()),
        }
    }
    Ok((nums, o))
}

fn next_pow2(n: usize) -> usize {
    n.max(1).next_power_of_two()
}

struct Spec {
    /// Spectral values per frequency bin (one column).
    vals: Vec<f64>,
    freq: Vec<f64>,
}

fn freq_axis(nfft: usize, sides: Sides, fs: Option<f64>) -> Vec<f64> {
    let step = match fs {
        Some(f) => f / nfft as f64,
        None => 2.0 * PI / nfft as f64,
    };
    match sides {
        Sides::One => {
            let m = if nfft.is_multiple_of(2) {
                nfft / 2 + 1
            } else {
                nfft.div_ceil(2)
            };
            (0..m).map(|i| i as f64 * step).collect()
        }
        Sides::Two => (0..nfft).map(|i| i as f64 * step).collect(),
        Sides::Centered => {
            let lo = -((nfft / 2) as isize);
            let lo = if nfft.is_multiple_of(2) {
                lo
            } else {
                -((nfft as isize - 1) / 2)
            };
            (0..nfft as isize).map(|i| (lo + i) as f64 * step).collect()
        }
    }
}

/// Scale a raw `|X|^2` periodogram bin vector into PSD/power, applying the
/// one-sided doubling and bin selection. `sumw2`/`sumw` come from the window.
fn finish(
    p2: Vec<f64>,
    nfft: usize,
    sides: Sides,
    fs: Option<f64>,
    power: bool,
    sumw: f64,
    sumw2: f64,
) -> Spec {
    let scale = if power {
        1.0 / (sumw * sumw)
    } else {
        match fs {
            Some(f) => 1.0 / (f * sumw2),
            None => 1.0 / (2.0 * PI * sumw2),
        }
    };
    let freq = freq_axis(nfft, sides, fs);
    let vals: Vec<f64> = match sides {
        Sides::One => {
            let m = freq.len();
            (0..m)
                .map(|i| {
                    let mut v = p2[i] * scale;
                    let nyq = nfft.is_multiple_of(2) && i == nfft / 2;
                    if i != 0 && !nyq {
                        v *= 2.0;
                    }
                    v
                })
                .collect()
        }
        Sides::Two => p2.iter().map(|v| v * scale).collect(),
        Sides::Centered => {
            let half = nfft / 2;
            let mut out = Vec::with_capacity(nfft);
            // order: -nfft/2 .. nfft/2-1 (even) or -(n-1)/2 .. (n-1)/2 (odd)
            let neg = if nfft.is_multiple_of(2) {
                half
            } else {
                (nfft - 1) / 2
            };
            for i in (nfft - neg)..nfft {
                out.push(p2[i] * scale);
            }
            for v in p2.iter().take(nfft - neg) {
                out.push(v * scale);
            }
            out
        }
    };
    Spec { vals, freq }
}

fn segment_pxx(seg: &[C], win: &[f64], nfft: usize) -> Vec<f64> {
    let mut v: Vec<C> = seg
        .iter()
        .zip(win)
        .map(|(c, w)| (c.0 * w, c.1 * w))
        .collect();
    v.resize(nfft, CZ);
    // nfft shorter than the segment: wrap-around sum (MATLAB aliases the data)
    if seg.len() > nfft {
        v = vec![CZ; nfft];
        for (i, (c, w)) in seg.iter().zip(win).enumerate() {
            v[i % nfft] = cadd(v[i % nfft], (c.0 * w, c.1 * w));
        }
    }
    dft(&v, false)
        .iter()
        .map(|c| c.0 * c.0 + c.1 * c.1)
        .collect()
}

fn pick_sides(o: &Opts, complex: bool) -> Sides {
    o.sides
        .unwrap_or(if complex { Sides::Two } else { Sides::One })
}

fn spec_outputs(specs: Vec<Spec>, row_in: bool) -> Vec<Value> {
    let freq = specs[0].freq.clone();
    let m = freq.len();
    let mut flat = Vec::with_capacity(m * specs.len());
    for s in &specs {
        flat.extend_from_slice(&s.vals);
    }
    let _ = row_in;
    vec![Value::matrix(vec![m, specs.len()], flat), rcol(freq)]
}

fn periodogram(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "periodogram")?;
    let (cols, row, _) = columns(&a[0], "periodogram")?;
    let (nums, o) = split_opts(&a[1..], "periodogram")?;
    let len = cols.first().map_or(0, |c| c.len());
    let complex = cols.iter().flatten().any(|c| c.1 != 0.0);
    let win: Vec<f64> = match nums.first() {
        Some(v) if !v.is_empty() => {
            let w = rvec(v, "periodogram")?;
            if w.len() != len {
                return Err(e(
                    "signal:periodogram:window",
                    "periodogram: the window must have the same length as the signal.",
                ));
            }
            w
        }
        _ => vec![1.0; len],
    };
    let nfft = match nums.get(1) {
        Some(v) if !v.is_empty() => arg_f64(v, "periodogram")? as usize,
        _ => 256.max(next_pow2(len)),
    };
    let fs = match nums.get(2) {
        Some(v) if !v.is_empty() => Some(arg_f64(v, "periodogram")?),
        _ => None,
    };
    if nfft == 0 {
        return Err(e(
            "signal:periodogram:nfft",
            "periodogram: NFFT must be positive.",
        ));
    }
    let sides = pick_sides(&o, complex);
    let sumw: f64 = win.iter().sum();
    let sumw2: f64 = win.iter().map(|w| w * w).sum();
    let specs: Vec<Spec> = cols
        .iter()
        .map(|c| {
            finish(
                segment_pxx(c, &win, nfft),
                nfft,
                sides,
                fs,
                o.power,
                sumw,
                sumw2,
            )
        })
        .collect();
    if nargout == 0 {
        return Err(e(
            "signal:periodogram:plot",
            "periodogram: the plotting form is not supported; request output arguments.",
        ));
    }
    Ok(spec_outputs(specs, row))
}

struct Welch {
    win: Vec<f64>,
    noverlap: usize,
    nfft: usize,
    fs: Option<f64>,
}

fn welch_params(nums: &[Value], len: usize, f: &str) -> R<Welch> {
    let default_l = ((len as f64 / 4.5).floor() as usize).max(1);
    let win: Vec<f64> = match nums.first() {
        Some(v) if !v.is_empty() => {
            if v.numel() == 1 {
                let l = arg_f64(v, f)?;
                if l < 1.0 || l.fract() != 0.0 {
                    return Err(e(
                        "signal:spectral:window",
                        format!("{}: window length must be a positive integer.", f),
                    ));
                }
                hamming(l as usize)
            } else {
                rvec(v, f)?
            }
        }
        _ => hamming(default_l),
    };
    let l = win.len();
    if l > len {
        return Err(e(
            "signal:spectral:window",
            format!("{}: the window is longer than the signal.", f),
        ));
    }
    let noverlap = match nums.get(1) {
        Some(v) if !v.is_empty() => {
            let x = arg_f64(v, f)?;
            if x < 0.0 || x >= l as f64 || x.fract() != 0.0 {
                return Err(e(
                    "signal:spectral:noverlap",
                    format!(
                        "{}: NOVERLAP must be a nonnegative integer smaller than the window length.",
                        f
                    ),
                ));
            }
            x as usize
        }
        _ => l / 2,
    };
    let nfft = match nums.get(2) {
        Some(v) if !v.is_empty() => {
            if v.numel() > 1 {
                return Err(e(
                    "signal:spectral:freqvec",
                    format!("{}: a frequency vector is not supported.", f),
                ));
            }
            arg_f64(v, f)? as usize
        }
        _ => 256.max(next_pow2(l)),
    };
    let fs = match nums.get(3) {
        Some(v) if !v.is_empty() => Some(arg_f64(v, f)?),
        _ => None,
    };
    if nfft == 0 {
        return Err(e(
            "signal:spectral:nfft",
            format!("{}: NFFT must be positive.", f),
        ));
    }
    Ok(Welch {
        win,
        noverlap,
        nfft,
        fs,
    })
}

fn pwelch(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "pwelch")?;
    let (cols, row, _) = columns(&a[0], "pwelch")?;
    let (nums, o) = split_opts(&a[1..], "pwelch")?;
    let len = cols.first().map_or(0, |c| c.len());
    let p = welch_params(&nums, len, "pwelch")?;
    let complex = cols.iter().flatten().any(|c| c.1 != 0.0);
    let sides = pick_sides(&o, complex);
    let l = p.win.len();
    let step = l - p.noverlap;
    let k = (len - p.noverlap) / step;
    let sumw: f64 = p.win.iter().sum();
    let sumw2: f64 = p.win.iter().map(|w| w * w).sum();
    let mut specs = Vec::new();
    for c in &cols {
        let mut acc = vec![0.0; p.nfft];
        for s in 0..k {
            let seg = &c[s * step..s * step + l];
            for (t, v) in acc.iter_mut().zip(segment_pxx(seg, &p.win, p.nfft)) {
                *t += v;
            }
        }
        for t in acc.iter_mut() {
            *t /= k as f64;
        }
        specs.push(finish(acc, p.nfft, sides, p.fs, o.power, sumw, sumw2));
    }
    if nargout == 0 {
        return Err(e(
            "signal:pwelch:plot",
            "pwelch: the plotting form is not supported; request output arguments.",
        ));
    }
    Ok(spec_outputs(specs, row))
}

fn spectrogram(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "spectrogram")?;
    let (cols, _, _) = columns(&a[0], "spectrogram")?;
    if cols.len() != 1 {
        return Err(e(
            "signal:spectrogram:input",
            "spectrogram: the input must be a vector.",
        ));
    }
    let x = &cols[0];
    let (nums, o) = split_opts(&a[1..], "spectrogram")?;
    let p = welch_params(&nums, x.len(), "spectrogram")?;
    let complex = x.iter().any(|c| c.1 != 0.0);
    let sides = pick_sides(&o, complex);
    let l = p.win.len();
    let step = l - p.noverlap;
    let k = (x.len() - p.noverlap) / step;
    if k == 0 {
        return Err(e(
            "signal:spectrogram:short",
            "spectrogram: the signal is shorter than one window.",
        ));
    }
    let sumw: f64 = p.win.iter().sum();
    let sumw2: f64 = p.win.iter().map(|w| w * w).sum();
    let freq = freq_axis(p.nfft, sides, p.fs);
    let m = freq.len();
    let mut s_re = Vec::with_capacity(m * k);
    let mut s_im = Vec::with_capacity(m * k);
    let mut pp = Vec::with_capacity(m * k);
    for s in 0..k {
        let seg = &x[s * step..s * step + l];
        let mut v: Vec<C> = seg
            .iter()
            .zip(&p.win)
            .map(|(c, w)| (c.0 * w, c.1 * w))
            .collect();
        v.resize(p.nfft, CZ);
        let xf = dft(&v, false);
        let p2: Vec<f64> = xf.iter().map(|c| c.0 * c.0 + c.1 * c.1).collect();
        let spec = finish(p2, p.nfft, sides, p.fs, o.power, sumw, sumw2);
        let sel: Vec<C> = match sides {
            Sides::One => xf[..m].to_vec(),
            Sides::Two => xf.clone(),
            Sides::Centered => {
                let neg = if p.nfft.is_multiple_of(2) {
                    p.nfft / 2
                } else {
                    (p.nfft - 1) / 2
                };
                let mut r = xf[p.nfft - neg..].to_vec();
                r.extend_from_slice(&xf[..p.nfft - neg]);
                r
            }
        };
        for c in sel {
            s_re.push(c.0);
            s_im.push(c.1);
        }
        pp.extend(spec.vals);
    }
    let fs = p.fs.unwrap_or(1.0);
    let t: Vec<f64> = (0..k)
        .map(|s| (l as f64 / 2.0 + (s * step) as f64) / fs)
        .collect();
    if nargout == 0 {
        return Err(e(
            "signal:spectrogram:plot",
            "spectrogram: the plotting form is not supported; request output arguments.",
        ));
    }
    let sv = Value::Num(indy_value::Num::complex(vec![m, k], s_re, s_im));
    Ok(vec![
        sv,
        rcol(freq),
        Value::row(t),
        Value::matrix(vec![m, k], pp),
    ])
}

// ---------------------------------------------------------------------------
// xcorr
// ---------------------------------------------------------------------------

fn xcorr(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "xcorr")?;
    let x = cvec(&a[0], "xcorr")?;
    let row = a[0].dims_ref()[0] == 1 && a[0].numel() > 1;
    if a[0].dims_ref().len() != 2 || (a[0].dims_ref()[0] != 1 && a[0].dims_ref()[1] != 1) {
        return Err(e(
            "signal:xcorr:matrix",
            "xcorr: matrix input is not supported; pass a vector.",
        ));
    }
    let mut y: Option<Vec<C>> = None;
    let mut maxlag: Option<usize> = None;
    let mut scale = "none".to_string();
    let mut idx = 1;
    // optional y: a vector with more than one element (or same-shaped second arg)
    if let Some(v) = a.get(1)
        && opt_string(v).is_none()
    {
        if v.numel() > 1 || (a.len() > 2 && opt_string(&a[2]).is_none()) {
            y = Some(cvec(v, "xcorr")?);
            idx = 2;
        } else if v.numel() == 1 {
            // xcorr(x, maxlag)
            idx = 1;
        }
    }
    if let Some(v) = a.get(idx)
        && opt_string(v).is_none()
    {
        if !v.is_empty() {
            let m = arg_f64(v, "xcorr")?;
            if m < 0.0 || m.fract() != 0.0 {
                return Err(e(
                    "signal:xcorr:maxlag",
                    "xcorr: MAXLAG must be a nonnegative integer.",
                ));
            }
            maxlag = Some(m as usize);
        }
        idx += 1;
    }
    if let Some(v) = a.get(idx) {
        scale = super::str_arg(v, "xcorr")?.to_ascii_lowercase();
    }
    let auto = y.is_none();
    let yv = y.clone().unwrap_or_else(|| x.clone());
    let m = x.len().max(yv.len());
    let ml = maxlag.unwrap_or(m - 1);
    let mut xs = x.clone();
    xs.resize(m, CZ);
    let mut ys = yv.clone();
    ys.resize(m, CZ);
    let lags: Vec<isize> = (-(ml as isize)..=ml as isize).collect();
    let nx: f64 = xs.iter().map(|c| c.0 * c.0 + c.1 * c.1).sum();
    let ny: f64 = ys.iter().map(|c| c.0 * c.0 + c.1 * c.1).sum();
    let mut out = Vec::with_capacity(lags.len());
    for &lag in &lags {
        let mut acc = CZ;
        // c(lag) = sum_n x(n+lag) conj(y(n))
        for n in 0..m as isize {
            let j = n + lag;
            if j >= 0 && j < m as isize {
                acc = cadd(acc, cmul(xs[j as usize], cconj(ys[n as usize])));
            }
        }
        let s = match scale.as_str() {
            "none" => 1.0,
            "biased" => 1.0 / m as f64,
            "unbiased" => 1.0 / (m as isize - lag.abs()).max(1) as f64,
            "normalized" | "coeff" => 1.0 / (nx * ny).sqrt(),
            _ => {
                return Err(e(
                    "signal:xcorr:scale",
                    format!("xcorr: unknown scale option '{}'.", scale),
                ));
            }
        };
        let _ = auto;
        out.push((acc.0 * s, acc.1 * s));
    }
    let cv = if row {
        super::crow(&out)
    } else {
        super::ccol(&out)
    };
    let mut res = vec![cv];
    if nargout >= 2 {
        res.push(Value::row(lags.iter().map(|l| *l as f64).collect()));
    }
    Ok(res)
}

// ---------------------------------------------------------------------------
// envelope (analytic method)
// ---------------------------------------------------------------------------

fn envelope(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "envelope")?;
    for v in &a[1..] {
        if let Some(s) = opt_string(v)
            && !s.eq_ignore_ascii_case("analytic")
        {
            return Err(e(
                "signal:envelope:method",
                format!(
                    "envelope: method or option '{}' is not supported yet (only 'analytic').",
                    s
                ),
            ));
        }
    }
    let (cols, row, _) = columns(&a[0], "envelope")?;
    let mut up = Vec::new();
    let mut lo = Vec::new();
    for c in &cols {
        let n = c.len() as f64;
        let mean = c.iter().map(|z| z.0).sum::<f64>() / n;
        let xm: Vec<C> = c.iter().map(|z| (z.0 - mean, 0.0)).collect();
        let h = analytic(&xm, xm.len());
        up.push(h.iter().map(|z| (cabs(*z) + mean, 0.0)).collect::<Vec<C>>());
        lo.push(
            h.iter()
                .map(|z| (-cabs(*z) + mean, 0.0))
                .collect::<Vec<C>>(),
        );
    }
    let mut out = vec![from_columns(&up, row)];
    if nargout >= 2 {
        out.push(from_columns(&lo, row));
    }
    let _ = PI;
    Ok(out)
}
