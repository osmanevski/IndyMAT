//! `fft` / `ifft`: iterative radix-2 for powers of two, Bluestein's
//! chirp-z for other lengths. Works along the first non-singleton
//! dimension (or `dim`), with zero-padding/truncation to `n`.

use indy_value::ops::C;
use indy_value::{Num, Value};

use super::math::{default_dim, slices};
use super::{arg_num, arg_usize, err, need, one, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "fft", |_, a, _| transform(&a, false));
    reg(it, "ifft", |_, a, _| transform(&a, true));
}

fn radix2(x: &mut [C], inverse: bool) {
    let n = x.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            x.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = 2.0 * std::f64::consts::PI / len as f64 * if inverse { 1.0 } else { -1.0 };
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0, 0.0);
            for k in 0..len / 2 {
                let a = x[start + k];
                let b = x[start + k + len / 2];
                let t = (b.0 * cr - b.1 * ci, b.0 * ci + b.1 * cr);
                x[start + k] = (a.0 + t.0, a.1 + t.1);
                x[start + k + len / 2] = (a.0 - t.0, a.1 - t.1);
                let nr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = nr;
            }
        }
        len <<= 1;
    }
}

/// Unnormalised DFT of any length.
pub fn dft(x: &[C], inverse: bool) -> Vec<C> {
    let n = x.len();
    if n <= 1 {
        return x.to_vec();
    }
    if n.is_power_of_two() {
        let mut v = x.to_vec();
        radix2(&mut v, inverse);
        return v;
    }
    // Bluestein: x_k * w_k convolved with conj chirp.
    let sign = if inverse { 1.0 } else { -1.0 };
    let m = (2 * n - 1).next_power_of_two();
    let chirp: Vec<C> = (0..n)
        .map(|k| {
            let kk = (k as u128 * k as u128) % (2 * n as u128);
            let ang = sign * std::f64::consts::PI * kk as f64 / n as f64;
            (ang.cos(), ang.sin())
        })
        .collect();
    let mut a = vec![(0.0, 0.0); m];
    for k in 0..n {
        let (xr, xi) = x[k];
        let (cr, ci) = chirp[k];
        a[k] = (xr * cr - xi * ci, xr * ci + xi * cr);
    }
    let mut b = vec![(0.0, 0.0); m];
    b[0] = (chirp[0].0, -chirp[0].1);
    for k in 1..n {
        let c = (chirp[k].0, -chirp[k].1);
        b[k] = c;
        b[m - k] = c;
    }
    radix2(&mut a, false);
    radix2(&mut b, false);
    for k in 0..m {
        let (p, q) = (a[k], b[k]);
        a[k] = (p.0 * q.0 - p.1 * q.1, p.0 * q.1 + p.1 * q.0);
    }
    radix2(&mut a, true);
    (0..n)
        .map(|k| {
            let (yr, yi) = (a[k].0 / m as f64, a[k].1 / m as f64);
            let (cr, ci) = chirp[k];
            (yr * cr - yi * ci, yr * ci + yi * cr)
        })
        .collect()
}

fn transform(a: &[Value], inverse: bool) -> R<Vec<Value>> {
    let fname = if inverse { "ifft" } else { "fft" };
    need(a, 1, fname)?;
    let x = arg_num(&a[0], fname)?;
    let symmetric = a
        .iter()
        .skip(1)
        .any(|v| v.to_str().is_some_and(|s| s == "symmetric"));
    let nums: Vec<&Value> = a.iter().skip(1).filter(|v| !v.is_char()).collect();
    let dim = match nums.get(1) {
        Some(v) => arg_usize(v, fname)?.max(1) - 1,
        None => default_dim(&x.dims),
    };
    let len = *x.dims.get(dim).unwrap_or(&1);
    let n = match nums.first() {
        Some(v) if !v.is_empty() => arg_usize(v, fname)?,
        _ => len,
    };
    if x.re.is_empty() && x.dims == [0, 0] {
        return one(Value::empty());
    }
    let mut od = x.dims.clone();
    while od.len() <= dim {
        od.push(1);
    }
    od[dim] = n;
    let out_n: usize = od.iter().product();
    let mut re = vec![0.0; out_n];
    let mut im = vec![0.0; out_n];
    let ostride: usize = od.iter().take(dim).product();
    let mut slot = 0usize;
    let mut err_out = None;
    slices(&x.dims, dim, |start, stride, l| {
        let mut buf: Vec<C> = (0..n)
            .map(|k| {
                if k < l {
                    (x.re[start + k * stride], x.im_at(start + k * stride))
                } else {
                    (0.0, 0.0)
                }
            })
            .collect();
        if inverse && symmetric {
            for k in 1..n {
                if n - k > k {
                    let c = buf[k];
                    buf[n - k] = (c.0, -c.1);
                }
            }
        }
        let mut y = dft(&buf, inverse);
        if inverse {
            for v in y.iter_mut() {
                *v = (v.0 / n as f64, v.1 / n as f64);
            }
        }
        // Output slice start in the output layout.
        let inner = slot % ostride.max(1);
        let outer = slot / ostride.max(1);
        let ostart = outer * ostride * n + inner;
        for (k, v) in y.iter().enumerate() {
            let p = ostart + k * ostride;
            if p < out_n {
                re[p] = v.0;
                im[p] = if inverse && symmetric { 0.0 } else { v.1 };
            } else {
                err_out = Some(());
            }
        }
        slot += 1;
    });
    if err_out.is_some() {
        return Err(err("Indy:internalError", "fft: layout error"));
    }
    one(Value::Num(Num::complex(od, re, im)))
}
