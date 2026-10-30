//! Least-squares polynomial fit (`polyfit`, `polyval` with error
//! estimate) and the window functions of the signal toolbox
//! (`hamming hann hanning blackman bartlett rectwin kaiser triang`).

use indy_value::{Num, StructArray, Value};

use super::{arg_f64, arg_num, arg_str, err, need, one, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "polyfit", polyfit);
    reg(it, "polyval", polyval);
    reg(it, "vander", vander);
    reg(it, "polyder", polyder);
    reg(it, "polyint", polyint);
    reg(it, "fftshift", |_, a, _| shift(&a, "fftshift", false));
    reg(it, "ifftshift", |_, a, _| shift(&a, "ifftshift", true));
    reg(it, "hamming", |_, a, _| window(&a, "hamming"));
    reg(it, "hann", |_, a, _| window(&a, "hann"));
    reg(it, "hanning", |_, a, _| window(&a, "hanning"));
    reg(it, "blackman", |_, a, _| window(&a, "blackman"));
    reg(it, "bartlett", |_, a, _| window(&a, "bartlett"));
    reg(it, "rectwin", |_, a, _| window(&a, "rectwin"));
    reg(it, "triang", |_, a, _| window(&a, "triang"));
    reg(it, "kaiser", |_, a, _| window(&a, "kaiser"));
}

// ---------------------------------------------------------------------------
// polyfit / polyval
// ---------------------------------------------------------------------------

/// Householder least squares for a column-major `m x n` matrix with
/// `m >= n`: returns `(coefficients, R (n x n, column-major))`.
fn lstsq(m: usize, n: usize, a: &[f64], b: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let mut r = a.to_vec();
    let mut q = b.to_vec();
    for k in 0..n.min(m) {
        let norm: f64 = (k..m).map(|i| r[i + k * m].powi(2)).sum::<f64>().sqrt();
        if norm == 0.0 {
            continue;
        }
        let alpha = if r[k + k * m] > 0.0 { -norm } else { norm };
        let mut v: Vec<f64> = (k..m).map(|i| r[i + k * m]).collect();
        v[0] -= alpha;
        let vn: f64 = v.iter().map(|x| x * x).sum();
        if vn == 0.0 {
            continue;
        }
        for j in k..n {
            let dot: f64 = (k..m).map(|i| v[i - k] * r[i + j * m]).sum();
            let f = 2.0 * dot / vn;
            for i in k..m {
                r[i + j * m] -= f * v[i - k];
            }
        }
        let dot: f64 = (k..m).map(|i| v[i - k] * q[i]).sum();
        let f = 2.0 * dot / vn;
        for i in k..m {
            q[i] -= f * v[i - k];
        }
    }
    let mut rn = vec![0.0; n * n];
    for j in 0..n {
        for i in 0..=j.min(m.saturating_sub(1)) {
            rn[i + j * n] = r[i + j * m];
        }
    }
    let mut p = vec![0.0; n];
    for i in (0..n).rev() {
        let mut s = q[i];
        for j in i + 1..n {
            s -= rn[i + j * n] * p[j];
        }
        p[i] = s / rn[i + i * n];
    }
    (p, rn)
}

fn polyfit(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 3, "polyfit")?;
    let x = arg_num(&a[0], "polyfit")?;
    let y = arg_num(&a[1], "polyfit")?;
    if x.re.len() != y.re.len() {
        return Err(err(
            "MATLAB:polyfit:XYSizeMismatch",
            "X and Y vectors must be the same size.",
        ));
    }
    if x.im.is_some() || y.im.is_some() {
        return Err(err(
            "MATLAB:unsupported",
            "polyfit: complex data is not supported.",
        ));
    }
    let n = arg_f64(&a[2], "polyfit")?;
    if n < 0.0 || n.fract() != 0.0 || !n.is_finite() {
        return Err(err(
            "MATLAB:polyfit:nonNegativeInteger",
            "N must be a nonnegative integer.",
        ));
    }
    let n = n as usize;
    let m = x.re.len();
    let mut xs = x.re.clone();
    let mut mu = (0.0, 1.0);
    if nargout > 2 && m > 0 {
        let mean = xs.iter().sum::<f64>() / m as f64;
        let sd = if m > 1 {
            (xs.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (m - 1) as f64).sqrt()
        } else {
            0.0
        };
        mu = (mean, sd);
        xs.iter_mut().for_each(|v| *v = (*v - mean) / sd);
    }
    let cols = n + 1;
    let mut v = vec![0.0; m * cols];
    for i in 0..m {
        let mut pw = 1.0;
        for j in (0..cols).rev() {
            v[i + j * m] = pw;
            pw *= xs[i];
        }
    }
    let (p, r, normr, df) = if m >= cols {
        let (p, r) = lstsq(m, cols, &v, &y.re);
        let res: f64 = (0..m)
            .map(|i| {
                let fit: f64 = (0..cols).map(|j| v[i + j * m] * p[j]).sum();
                (y.re[i] - fit).powi(2)
            })
            .sum();
        (p, r, res.sqrt(), (m - cols) as f64)
    } else {
        // underdetermined: minimum-norm solution via the normal equations
        // of the transposed system (pad with zero rows).
        let mut vp = vec![0.0; cols * cols];
        let mut yp = y.re.clone();
        yp.resize(cols, 0.0);
        for j in 0..cols {
            for i in 0..m {
                vp[i + j * cols] = v[i + j * m];
            }
        }
        let (p, r) = lstsq(cols, cols, &vp, &yp);
        (p, r, 0.0, 0.0)
    };
    let mut out = vec![Value::row(p)];
    if nargout > 1 {
        let mut s = StructArray::new_scalar();
        s.set(0, "R", Value::matrix(vec![cols, cols], r));
        s.set(0, "df", Value::num(df));
        s.set(0, "normr", Value::num(normr));
        out.push(Value::Struct(s));
    }
    if nargout > 2 {
        out.push(Value::col(vec![mu.0, mu.1]));
    }
    Ok(out)
}

fn polyval(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "polyval")?;
    let want_delta = nargout > 1;
    let s = match a.get(2) {
        Some(Value::Struct(s)) => Some(s),
        _ => None,
    };
    if want_delta && s.is_none() {
        return Err(err(
            "MATLAB:polyval:RequiresS",
            "polyval: S is required for the error estimate output.",
        ));
    }
    // y itself: the plain evaluation (S slot, if any, is dropped)
    let mut plain = vec![a[0].clone(), a[1].clone()];
    if a.len() > 3 {
        plain.push(Value::empty());
        plain.push(a[3].clone());
    }
    let y = super::extra::polyval(it, plain, 1)?;
    if !want_delta {
        return Ok(y);
    }
    let s = s.unwrap();
    let get = |f: &str| {
        s.get(0, f).and_then(|v| v.to_num()).ok_or_else(|| {
            err(
                "MATLAB:polyval:InvalidS",
                format!("polyval: S is missing field {}.", f),
            )
        })
    };
    let r = get("R")?;
    let df = get("df")?.re.first().copied().unwrap_or(0.0);
    let normr = get("normr")?.re.first().copied().unwrap_or(0.0);
    let p = arg_num(&a[0], "polyval")?;
    let x = arg_num(&a[1], "polyval")?;
    let cols = p.re.len();
    let (mu0, mu1) = match a.get(3) {
        Some(m) => {
            let m = arg_num(m, "polyval")?.re;
            (
                m.first().copied().unwrap_or(0.0),
                m.get(1).copied().unwrap_or(1.0),
            )
        }
        None => (0.0, 1.0),
    };
    let delta: Vec<f64> =
        x.re.iter()
            .map(|&xv| {
                let xv = (xv - mu0) / mu1;
                // row of the Vandermonde matrix, then solve v = w * R for w
                let mut v = vec![0.0; cols];
                let mut pw = 1.0;
                for j in (0..cols).rev() {
                    v[j] = pw;
                    pw *= xv;
                }
                let mut w = vec![0.0; cols];
                for j in 0..cols {
                    let mut t = v[j];
                    for i in 0..j {
                        t -= w[i] * r.re[i + j * cols];
                    }
                    w[j] = t / r.re[j + j * cols];
                }
                let s2: f64 = w.iter().map(|e| e * e).sum();
                (1.0 + s2).sqrt() * normr / df.sqrt()
            })
            .collect();
    Ok(vec![
        y.into_iter().next().unwrap_or_default(),
        Value::Num(Num::real(x.dims.clone(), delta)),
    ])
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

fn bessel_i0(x: f64) -> f64 {
    let (mut sum, mut term) = (1.0, 1.0);
    let q = x * x / 4.0;
    for k in 1..500 {
        term *= q / (k * k) as f64;
        sum += term;
        if term < sum * 1e-17 {
            break;
        }
    }
    sum
}

fn window(a: &[Value], name: &str) -> R<Vec<Value>> {
    need(a, 1, name)?;
    let l = arg_f64(&a[0], name)?;
    if l < 0.0 || l.fract() != 0.0 || !l.is_finite() {
        return Err(err(
            "MATLAB:window:invalidLength",
            format!("{}: L must be a nonnegative integer.", name),
        ));
    }
    let l = l as usize;
    let mut beta = 0.5;
    let mut periodic = false;
    let mut rest = &a[1..];
    if name == "kaiser"
        && let Some(b) = rest.first().filter(|v| !v.is_text())
    {
        beta = arg_f64(b, name)?;
        rest = &rest[1..];
    }
    if let Some(o) = rest.first() {
        if rest.len() > 1 {
            return Err(err(
                "MATLAB:TooManyInputs",
                format!("Too many input arguments (function '{}').", name),
            ));
        }
        let s = arg_str(o, name)?.to_ascii_lowercase();
        match s.as_str() {
            "periodic" if name != "kaiser" => periodic = true,
            "symmetric" => {}
            _ => {
                return Err(err(
                    "MATLAB:window:invalidOption",
                    format!("{}: option must be 'symmetric' or 'periodic'.", name),
                ));
            }
        }
    }
    if l == 0 {
        return one(Value::matrix(vec![0, 1], vec![]));
    }
    if l == 1 {
        return one(Value::num(1.0));
    }
    // symmetric window of length `n`
    let sym = |n: usize| -> Vec<f64> {
        let pi = std::f64::consts::PI;
        let nm = (n - 1) as f64;
        (0..n)
            .map(|i| {
                let k = i as f64;
                match name {
                    "hamming" => 0.54 - 0.46 * (2.0 * pi * k / nm).cos(),
                    "hann" => 0.5 * (1.0 - (2.0 * pi * k / nm).cos()),
                    "hanning" => 0.5 - 0.5 * (2.0 * pi * (k + 1.0) / (n as f64 + 1.0)).cos(),
                    "blackman" => {
                        0.42 - 0.5 * (2.0 * pi * k / nm).cos() + 0.08 * (4.0 * pi * k / nm).cos()
                    }
                    "bartlett" => 1.0 - (2.0 * k / nm - 1.0).abs(),
                    "triang" => {
                        // nonzero end points
                        let nn = n as f64;
                        if n % 2 == 1 {
                            1.0 - ((k - (nn - 1.0) / 2.0).abs() * 2.0) / (nn + 1.0)
                        } else {
                            1.0 - ((k - (nn - 1.0) / 2.0).abs() * 2.0) / nn
                        }
                    }
                    "kaiser" => {
                        let t = 2.0 * k / nm - 1.0;
                        bessel_i0(beta * (1.0 - t * t).max(0.0).sqrt()) / bessel_i0(beta)
                    }
                    _ => 1.0,
                }
            })
            .collect()
    };
    let w = if periodic {
        // periodic form: drop the last point of the (L+1)-point window
        let mut w = sym(l + 1);
        w.truncate(l);
        w
    } else {
        sym(l)
    };
    one(Value::col(w))
}

fn vander(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "vander")?;
    let c = arg_num(&a[0], "vander")?;
    let m = c.re.len();
    let n = match a.get(1) {
        Some(v) => super::arg_usize(v, "vander")?,
        None => m,
    };
    let mut out = vec![0.0; m * n];
    for i in 0..m {
        for j in 0..n {
            out[i + j * m] = c.re[i].powi((n - 1 - j) as i32);
        }
    }
    one(Value::matrix(vec![m, n], out))
}

fn trim_leading(mut p: Vec<f64>) -> Vec<f64> {
    let first = p
        .iter()
        .position(|v| *v != 0.0)
        .unwrap_or(p.len().saturating_sub(1));
    p.drain(..first);
    if p.is_empty() {
        p.push(0.0);
    }
    p
}

fn convolve(a: &[f64], b: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        for (j, y) in b.iter().enumerate() {
            out[i + j] += x * y;
        }
    }
    out
}

fn deriv(p: &[f64]) -> Vec<f64> {
    let n = p.len();
    if n <= 1 {
        return vec![0.0];
    }
    trim_leading((0..n - 1).map(|i| p[i] * (n - 1 - i) as f64).collect())
}

fn poly_vec(v: &Value, fname: &str) -> R<Vec<f64>> {
    let x = arg_num(v, fname)?;
    if x.im.is_some() {
        return Err(err(
            "MATLAB:unsupported",
            format!("{}: complex polynomials are not supported.", fname),
        ));
    }
    if x.re.is_empty() {
        return Ok(vec![0.0]);
    }
    if x.dims.len() != 2 || (x.dims[0] != 1 && x.dims[1] != 1) {
        return Err(err(
            "MATLAB:polyder:notVector",
            format!("{}: argument must be a vector.", fname),
        ));
    }
    Ok(x.re)
}

fn polyder(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "polyder")?;
    let u = poly_vec(&a[0], "polyder")?;
    if a.len() == 1 {
        return one(Value::row(deriv(&u)));
    }
    let v = poly_vec(&a[1], "polyder")?;
    if nargout <= 1 {
        // derivative of the product
        return one(Value::row(deriv(&convolve(&u, &v))));
    }
    // quotient u / v : numerator v*u' - u*v', denominator v^2
    let (du, dv) = (deriv(&u), deriv(&v));
    let (a1, a2) = (convolve(&v, &du), convolve(&u, &dv));
    let n = a1.len().max(a2.len());
    let pad = |x: &[f64]| {
        let mut r = vec![0.0; n - x.len()];
        r.extend_from_slice(x);
        r
    };
    let (p1, p2) = (pad(&a1), pad(&a2));
    let num: Vec<f64> = p1.iter().zip(&p2).map(|(x, y)| x - y).collect();
    Ok(vec![
        Value::row(trim_leading(num)),
        Value::row(trim_leading(convolve(&v, &v))),
    ])
}

fn polyint(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "polyint")?;
    let p = poly_vec(&a[0], "polyint")?;
    let k = match a.get(1) {
        Some(v) => arg_f64(v, "polyint")?,
        None => 0.0,
    };
    let n = p.len();
    let mut out: Vec<f64> = (0..n).map(|i| p[i] / (n - i) as f64).collect();
    out.push(k);
    one(Value::row(out))
}

fn shift(a: &[Value], fname: &str, inverse: bool) -> R<Vec<Value>> {
    need(a, 1, fname)?;
    let x = arg_num(&a[0], fname)?;
    let dims = x.dims.clone();
    let which: Vec<usize> = match a.get(1) {
        Some(d) => vec![super::arg_usize(d, fname)?.saturating_sub(1)],
        None => {
            if dims.iter().filter(|d| **d > 1).count() <= 1 {
                vec![dims.iter().position(|d| *d > 1).unwrap_or(0)]
            } else {
                (0..dims.len()).collect()
            }
        }
    };
    let mut cur = x;
    for dim in which {
        let len = cur.dims.get(dim).copied().unwrap_or(1);
        if len <= 1 {
            continue;
        }
        let sh = if inverse { len.div_ceil(2) } else { len / 2 };
        let stride: usize = cur.dims.iter().take(dim).product();
        let mut re = cur.re.clone();
        let mut im = cur.im.clone();
        super::math::slices(&cur.dims, dim, |start, st, ln| {
            for k in 0..ln {
                let dst = start + ((k + sh) % ln) * st;
                let src = start + k * st;
                re[dst] = cur.re[src];
                if let (Some(o), Some(i)) = (im.as_mut(), cur.im.as_ref()) {
                    o[dst] = i[src];
                }
            }
        });
        let _ = stride;
        cur = Num {
            re,
            im,
            ..cur.clone()
        };
    }
    one(Value::Num(cur))
}
