//! Filtering: `filtfilt`, `sosfilt`, `fftfilt`, `medfilt1`.

use indy_value::Value;

use super::design::solve;
use super::{C, CZ, cadd, cmul, columns, cvec, e, from_columns, opt_string, rvec};
use crate::builtins::{arg_f64, need, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "filtfilt", filtfilt);
    reg(it, "sosfilt", sosfilt);
    reg(it, "fftfilt", fftfilt);
    reg(it, "medfilt1", medfilt1);
}

/// Direct form II transposed with initial state `zi`.
pub fn df2t(b: &[f64], a: &[f64], x: &[f64], zi: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let n = b.len().max(a.len());
    let a0 = a[0];
    let mut bb: Vec<f64> = b.iter().map(|v| v / a0).collect();
    let mut aa: Vec<f64> = a.iter().map(|v| v / a0).collect();
    bb.resize(n, 0.0);
    aa.resize(n, 0.0);
    let mut z = zi.to_vec();
    z.resize(n.saturating_sub(1), 0.0);
    let mut y = Vec::with_capacity(x.len());
    for &xk in x {
        let yk = bb[0] * xk + z.first().copied().unwrap_or(0.0);
        for i in 0..n.saturating_sub(1) {
            let next = if i + 1 < n - 1 { z[i + 1] } else { 0.0 };
            z[i] = bb[i + 1] * xk + next - aa[i + 1] * yk;
        }
        y.push(yk);
    }
    (y, z)
}

/// Steady-state initial conditions for a unit step input.
fn steady_state(b: &[f64], a: &[f64]) -> Option<Vec<f64>> {
    let n = b.len();
    if n < 2 {
        return Some(vec![]);
    }
    let m = n - 1;
    let mut mat = vec![vec![0.0; m]; m];
    for (i, row) in mat.iter_mut().enumerate() {
        row[i] += 1.0;
        row[0] += a[i + 1];
        if i + 1 < m {
            row[i + 1] -= 1.0;
        }
    }
    let rhs: Vec<f64> = (0..m).map(|i| b[i + 1] - a[i + 1] * b[0]).collect();
    solve(mat, rhs)
}

fn real_cols(v: &Value, f: &str) -> R<(Vec<Vec<f64>>, bool)> {
    let (cols, row, _) = columns(v, f)?;
    if cols.iter().flatten().any(|c| c.1 != 0.0) {
        return Err(e(
            "signal:filter:complex",
            format!("{}: complex input is not supported.", f),
        ));
    }
    Ok((
        cols.iter()
            .map(|c| c.iter().map(|z| z.0).collect())
            .collect(),
        row,
    ))
}

fn to_value(cols: Vec<Vec<f64>>, row: bool) -> Value {
    let cc: Vec<Vec<C>> = cols
        .iter()
        .map(|c| c.iter().map(|x| (*x, 0.0)).collect())
        .collect();
    from_columns(&cc, row)
}

fn sos_to_tf(sos: &Value, g: &[f64], f: &str) -> R<(Vec<f64>, Vec<f64>)> {
    let n = crate::builtins::arg_num(sos, f)?;
    let m = n.dims[0];
    let mut b = vec![(1.0, 0.0)];
    let mut a = vec![(1.0, 0.0)];
    for i in 0..m {
        let nb: Vec<C> = (0..3).map(|j| (n.re[j * m + i], 0.0)).collect();
        let na: Vec<C> = (3..6).map(|j| (n.re[j * m + i], 0.0)).collect();
        b = super::pmul(&b, &nb);
        a = super::pmul(&a, &na);
    }
    let gain: f64 = g.iter().product();
    Ok((
        b.iter().map(|c| c.0 * gain).collect(),
        a.iter().map(|c| c.0).collect(),
    ))
}

fn filtfilt(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 3, "filtfilt")?;
    let sos_form = a[0].dims_ref().len() == 2 && a[0].dims_ref()[1] == 6 && a[0].dims_ref()[0] > 1;
    let (mut b, mut den) = if sos_form {
        sos_to_tf(&a[0], &rvec(&a[1], "filtfilt")?, "filtfilt")?
    } else {
        (rvec(&a[0], "filtfilt")?, rvec(&a[1], "filtfilt")?)
    };
    if b.is_empty() || den.is_empty() || den[0] == 0.0 {
        return Err(e(
            "signal:filtfilt:coeff",
            "filtfilt: invalid filter coefficients.",
        ));
    }
    let a0 = den[0];
    b.iter_mut().for_each(|v| *v /= a0);
    den.iter_mut().for_each(|v| *v /= a0);
    let nfilt = b.len().max(den.len());
    b.resize(nfilt, 0.0);
    den.resize(nfilt, 0.0);
    let nfact = 3 * (nfilt - 1);
    let (cols, row) = real_cols(&a[2], "filtfilt")?;
    let zi = steady_state(&b, &den).ok_or_else(|| {
        e(
            "signal:filtfilt:ic",
            "filtfilt: cannot compute steady-state conditions.",
        )
    })?;
    let mut out = Vec::new();
    for x in &cols {
        let n = x.len();
        if n <= nfact {
            return Err(e(
                "signal:filtfilt:minlength",
                "Data must have length more than 3 times filter order.",
            ));
        }
        let mut ext = Vec::with_capacity(n + 2 * nfact);
        for i in (1..=nfact).rev() {
            ext.push(2.0 * x[0] - x[i]);
        }
        ext.extend_from_slice(x);
        for i in 1..=nfact {
            ext.push(2.0 * x[n - 1] - x[n - 1 - i]);
        }
        let z0: Vec<f64> = zi.iter().map(|v| v * ext[0]).collect();
        let (mut y, _) = df2t(&b, &den, &ext, &z0);
        y.reverse();
        let z1: Vec<f64> = zi.iter().map(|v| v * y[0]).collect();
        let (mut y2, _) = df2t(&b, &den, &y, &z1);
        y2.reverse();
        out.push(y2[nfact..nfact + n].to_vec());
    }
    Ok(vec![to_value(out, row)])
}

fn sosfilt(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "sosfilt")?;
    let s = crate::builtins::arg_num(&a[0], "sosfilt")?;
    if s.dims.len() != 2 || s.dims[1] != 6 || s.dims[0] == 0 {
        return Err(e(
            "signal:sos:size",
            "sosfilt: SOS must be an L-by-6 matrix.",
        ));
    }
    let m = s.dims[0];
    let (cols, row) = real_cols(&a[1], "sosfilt")?;
    let mut out = Vec::new();
    for x in cols {
        let mut y = x;
        for i in 0..m {
            let r: Vec<f64> = (0..6).map(|j| s.re[j * m + i]).collect();
            if r[3] == 0.0 {
                return Err(e(
                    "signal:sos:a0",
                    "sosfilt: a0 of each section must be nonzero.",
                ));
            }
            y = df2t(&r[..3], &r[3..], &y, &[0.0, 0.0]).0;
        }
        out.push(y);
    }
    Ok(vec![to_value(out, row)])
}

fn fftfilt(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "fftfilt")?;
    let b = cvec(&a[0], "fftfilt")?;
    if b.is_empty() {
        return Err(e("signal:fftfilt:b", "fftfilt: B must not be empty."));
    }
    let (cols, row, _) = columns(&a[1], "fftfilt")?;
    let out: Vec<Vec<C>> = cols
        .iter()
        .map(|x| {
            (0..x.len())
                .map(|n| {
                    let mut acc = CZ;
                    for (k, bk) in b.iter().enumerate().take(n + 1) {
                        acc = cadd(acc, cmul(*bk, x[n - k]));
                    }
                    acc
                })
                .collect()
        })
        .collect();
    Ok(vec![from_columns(&out, row)])
}

fn medfilt1(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "medfilt1")?;
    let mut n = 3usize;
    let mut omit_nan = false;
    let mut truncate = false;
    let mut pos = 0;
    for v in &a[1..] {
        if let Some(s) = opt_string(v) {
            match s.to_ascii_lowercase().as_str() {
                "omitnan" => omit_nan = true,
                "includenan" => omit_nan = false,
                "zeropad" => truncate = false,
                "truncate" => truncate = true,
                _ => {
                    return Err(e(
                        "signal:medfilt1:option",
                        format!("medfilt1: unknown option '{}'.", s),
                    ));
                }
            }
        } else {
            if pos == 0 && !v.is_empty() {
                let x = arg_f64(v, "medfilt1")?;
                if x < 1.0 || x.fract() != 0.0 {
                    return Err(e(
                        "signal:medfilt1:n",
                        "medfilt1: N must be a positive integer.",
                    ));
                }
                n = x as usize;
            }
            pos += 1;
        }
    }
    let (cols, row) = real_cols(&a[0], "medfilt1")?;
    let lo = n / 2;
    let hi = n - 1 - lo;
    let out: Vec<Vec<f64>> = cols
        .iter()
        .map(|x| {
            let len = x.len() as isize;
            (0..len)
                .map(|k| {
                    let mut w: Vec<f64> = Vec::with_capacity(n);
                    for j in (k - lo as isize)..=(k + hi as isize) {
                        if j < 0 || j >= len {
                            if !truncate {
                                w.push(0.0);
                            }
                        } else {
                            w.push(x[j as usize]);
                        }
                    }
                    if omit_nan {
                        w.retain(|v| !v.is_nan());
                    } else if w.iter().any(|v| v.is_nan()) {
                        return f64::NAN;
                    }
                    if w.is_empty() {
                        return f64::NAN;
                    }
                    w.sort_by(|p, q| p.total_cmp(q));
                    let m = w.len();
                    if !m.is_multiple_of(2) {
                        w[m / 2]
                    } else {
                        (w[m / 2 - 1] + w[m / 2]) / 2.0
                    }
                })
                .collect()
        })
        .collect();
    Ok(vec![to_value(out, row)])
}
