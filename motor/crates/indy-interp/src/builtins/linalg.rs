//! Small dense linear algebra without LAPACK: Householder QR and
//! one-sided Jacobi SVD (real matrices), plus null/orth/pinv/ipermute.

use indy_value::{Num, Value};

use super::{arg_num, err, need, one, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "qr", qr);
    reg(it, "lu", lu);
    reg(it, "svd", svd);
    reg(it, "null", |_, a, _| {
        need(&a, 1, "null")?;
        let (m, n, x) = real_matrix(&a[0], "null")?;
        let (_, s, v) = jacobi_svd(m, n, &x);
        let tol = m.max(n) as f64 * s.first().copied().unwrap_or(0.0) * f64::EPSILON;
        let r = s.iter().filter(|x| **x > tol).count();
        let cols: Vec<usize> = (r..n).collect();
        let mut out = Vec::with_capacity(n * cols.len());
        for &c in &cols {
            out.extend_from_slice(&v[c * n..c * n + n]);
        }
        one(Value::matrix(vec![n, cols.len()], out))
    });
    reg(it, "orth", |_, a, _| {
        need(&a, 1, "orth")?;
        let (m, n, x) = real_matrix(&a[0], "orth")?;
        let (u, s, _) = jacobi_svd(m, n, &x);
        let tol = m.max(n) as f64 * s.first().copied().unwrap_or(0.0) * f64::EPSILON;
        let r = s.iter().filter(|x| **x > tol).count();
        one(Value::matrix(vec![m, r], u[..m * r].to_vec()))
    });
    reg(it, "pinv", |_, a, _| {
        need(&a, 1, "pinv")?;
        let (m, n, x) = real_matrix(&a[0], "pinv")?;
        let (u, s, v) = jacobi_svd(m, n, &x);
        let tol = m.max(n) as f64 * s.first().copied().unwrap_or(0.0) * f64::EPSILON;
        let mut out = vec![0.0; n * m];
        for (k, sk) in s.iter().enumerate() {
            if *sk <= tol {
                continue;
            }
            for i in 0..n {
                for j in 0..m {
                    out[i + j * n] += v[i + k * n] * u[j + k * m] / sk;
                }
            }
        }
        one(Value::matrix(vec![n, m], out))
    });
    reg(it, "ipermute", |it, a, n| {
        need(&a, 2, "ipermute")?;
        let order: Vec<f64> = a[1].to_f64_vec().unwrap_or_default();
        let mut inv = vec![0.0; order.len()];
        for (i, o) in order.iter().enumerate() {
            let k = *o as usize;
            if k == 0 || k > order.len() {
                return Err(err(
                    "MATLAB:ipermute:invalidOrder",
                    "ipermute: invalid permutation.",
                ));
            }
            inv[k - 1] = i as f64 + 1.0;
        }
        it.call_named("permute", vec![a[0].clone(), Value::row(inv)], n)
    });
}

fn real_matrix(v: &Value, fname: &str) -> R<(usize, usize, Vec<f64>)> {
    let x = arg_num(v, fname)?;
    if x.im.is_some() || x.dims.len() != 2 {
        return Err(err(
            "MATLAB:unsupported",
            format!(
                "{}: only real 2-D matrices are supported (phase 2 engine).",
                fname
            ),
        ));
    }
    Ok((x.dims[0], x.dims[1], x.re))
}

/// Householder QR: returns (Q m x m, R m x n).
pub fn householder_qr(m: usize, n: usize, a: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let mut r = a.to_vec();
    let mut q = vec![0.0; m * m];
    for i in 0..m {
        q[i + i * m] = 1.0;
    }
    for k in 0..m.min(n) {
        let norm: f64 = (k..m)
            .map(|i| r[i + k * m] * r[i + k * m])
            .sum::<f64>()
            .sqrt();
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
        for c in 0..n {
            let d: f64 = (k..m).map(|i| v[i - k] * r[i + c * m]).sum();
            let f = 2.0 * d / vn;
            for i in k..m {
                r[i + c * m] -= f * v[i - k];
            }
        }
        // Q = Q * H
        for row in 0..m {
            let d: f64 = (k..m).map(|i| q[row + i * m] * v[i - k]).sum();
            let f = 2.0 * d / vn;
            for i in k..m {
                q[row + i * m] -= f * v[i - k];
            }
        }
        for i in k + 1..m {
            r[i + k * m] = 0.0;
        }
    }
    (q, r)
}

fn lu(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "lu")?;
    let (m, n, x) = real_matrix(&a[0], "lu")?;
    let k = m.min(n);
    let mut u = x.clone();
    let mut l = vec![0.0; m * k];
    let mut perm: Vec<usize> = (0..m).collect();
    for c in 0..k {
        let p = (c..m)
            .max_by(|&i, &j| {
                u[i + c * m]
                    .abs()
                    .partial_cmp(&u[j + c * m].abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(c);
        if p != c {
            for j in 0..n {
                u.swap(c + j * m, p + j * m);
            }
            for j in 0..c {
                l.swap(c + j * m, p + j * m);
            }
            perm.swap(c, p);
        }
        l[c + c * m] = 1.0;
        let piv = u[c + c * m];
        for i in c + 1..m {
            let f = if piv != 0.0 { u[i + c * m] / piv } else { 0.0 };
            l[i + c * m] = f;
            for j in c..n {
                u[i + j * m] -= f * u[c + j * m];
            }
        }
    }
    let mut uu = vec![0.0; k * n];
    for j in 0..n {
        for i in 0..k.min(j + 1) {
            uu[i + j * k] = u[i + j * m];
        }
    }
    let mut pm = vec![0.0; m * m];
    for (i, &p) in perm.iter().enumerate() {
        pm[i + p * m] = 1.0;
    }
    if nargout >= 3 {
        return Ok(vec![
            Value::matrix(vec![m, k], l),
            Value::matrix(vec![k, n], uu),
            Value::matrix(vec![m, m], pm),
        ]);
    }
    // Two outputs: L is permuted so that A = L*U.
    let mut lp = vec![0.0; m * k];
    for (i, &p) in perm.iter().enumerate() {
        for j in 0..k {
            lp[p + j * m] = l[i + j * m];
        }
    }
    if nargout == 2 {
        return Ok(vec![
            Value::matrix(vec![m, k], lp),
            Value::matrix(vec![k, n], uu),
        ]);
    }
    let mut y = uu.clone();
    for j in 0..k.min(n) {
        for i in j + 1..k {
            y[i + j * k] = l[i + j * m];
        }
    }
    one(Value::matrix(vec![k, n], y))
}

/// QR with column pivoting: A(:, p) = Q*R.
fn pivoted_qr(m: usize, n: usize, a: &[f64]) -> (Vec<f64>, Vec<f64>, Vec<usize>) {
    let mut cols: Vec<usize> = (0..n).collect();
    let mut work = a.to_vec();
    for k in 0..m.min(n) {
        let norm = |c: usize, w: &[f64]| (k..m).map(|i| w[i + c * m] * w[i + c * m]).sum::<f64>();
        let best = (k..n)
            .max_by(|&i, &j| {
                norm(i, &work)
                    .partial_cmp(&norm(j, &work))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(k);
        if best != k {
            for i in 0..m {
                work.swap(i + k * m, i + best * m);
            }
            cols.swap(k, best);
        }
        // Reduce column k below the diagonal (Householder on the sub-block).
        let nrm: f64 = (k..m)
            .map(|i| work[i + k * m] * work[i + k * m])
            .sum::<f64>()
            .sqrt();
        if nrm == 0.0 {
            continue;
        }
        let alpha = if work[k + k * m] > 0.0 { -nrm } else { nrm };
        let mut v: Vec<f64> = (k..m).map(|i| work[i + k * m]).collect();
        v[0] -= alpha;
        let vn: f64 = v.iter().map(|x| x * x).sum();
        if vn == 0.0 {
            continue;
        }
        for c in k..n {
            let d: f64 = (k..m).map(|i| v[i - k] * work[i + c * m]).sum();
            let f = 2.0 * d / vn;
            for i in k..m {
                work[i + c * m] -= f * v[i - k];
            }
        }
    }
    let mut ap = vec![0.0; m * n];
    for (j, &c) in cols.iter().enumerate() {
        ap[j * m..j * m + m].copy_from_slice(&a[c * m..c * m + m]);
    }
    let (q, r) = householder_qr(m, n, &ap);
    (q, r, cols)
}

fn qr(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "qr")?;
    let (m, n, x) = real_matrix(&a[0], "qr")?;
    let econ = a.len() > 1
        && (a[1].scalar_f64() == Some(0.0)
            || a[1].to_str().is_some_and(|s| s == "econ" || s == "vector"));
    if nargout >= 3 {
        let (q, r, cols) = pivoted_qr(m, n, &x);
        let p = if econ {
            Value::row(cols.iter().map(|c| *c as f64 + 1.0).collect())
        } else {
            let mut pm = vec![0.0; n * n];
            for (j, &c) in cols.iter().enumerate() {
                pm[c + j * n] = 1.0;
            }
            Value::matrix(vec![n, n], pm)
        };
        if econ && m > n {
            let mut re = vec![0.0; n * n];
            for j in 0..n {
                for i in 0..n {
                    re[i + j * n] = r[i + j * m];
                }
            }
            return Ok(vec![
                Value::matrix(vec![m, n], q[..m * n].to_vec()),
                Value::matrix(vec![n, n], re),
                p,
            ]);
        }
        return Ok(vec![
            Value::matrix(vec![m, m], q),
            Value::matrix(vec![m, n], r),
            p,
        ]);
    }
    let (q, r) = householder_qr(m, n, &x);
    if nargout <= 1 {
        return one(Value::matrix(vec![m, n], r));
    }
    if econ && m > n {
        let qe = q[..m * n].to_vec();
        let mut re = vec![0.0; n * n];
        for j in 0..n {
            for i in 0..n {
                re[i + j * n] = r[i + j * m];
            }
        }
        return Ok(vec![
            Value::matrix(vec![m, n], qe),
            Value::matrix(vec![n, n], re),
        ]);
    }
    Ok(vec![
        Value::matrix(vec![m, m], q),
        Value::matrix(vec![m, n], r),
    ])
}

/// One-sided Jacobi SVD: returns (U m x n, s descending, V n x n) for
/// m >= n; transposes internally otherwise and returns U m x min.
pub fn jacobi_svd(m: usize, n: usize, a: &[f64]) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    if m < n {
        let mut t = vec![0.0; n * m];
        for i in 0..m {
            for j in 0..n {
                t[j + i * n] = a[i + j * m];
            }
        }
        let (u, s, v) = jacobi_svd(n, m, &t);
        // A' = U S V'  =>  A = V S U'
        let k = s.len();
        let mut uu = vec![0.0; m * k];
        uu[..m * k].copy_from_slice(&v[..m * k]);
        let mut vv = vec![0.0; n * n];
        vv[..n * k].copy_from_slice(&u[..n * k]);
        return (uu, s, vv);
    }
    let mut u = a.to_vec();
    let mut v = vec![0.0; n * n];
    for i in 0..n {
        v[i + i * n] = 1.0;
    }
    for _sweep in 0..60 {
        let mut off = 0.0f64;
        for p in 0..n {
            for q in p + 1..n {
                let (mut alpha, mut beta, mut gamma) = (0.0, 0.0, 0.0);
                for i in 0..m {
                    let (x, y) = (u[i + p * m], u[i + q * m]);
                    alpha += x * x;
                    beta += y * y;
                    gamma += x * y;
                }
                if gamma == 0.0 {
                    continue;
                }
                off = off.max(gamma.abs() / (alpha * beta).sqrt().max(f64::MIN_POSITIVE));
                let zeta = (beta - alpha) / (2.0 * gamma);
                let t = zeta.signum() / (zeta.abs() + (1.0 + zeta * zeta).sqrt());
                let t = if zeta == 0.0 { 1.0 } else { t };
                let c = 1.0 / (1.0 + t * t).sqrt();
                let s = c * t;
                for i in 0..m {
                    let (x, y) = (u[i + p * m], u[i + q * m]);
                    u[i + p * m] = c * x - s * y;
                    u[i + q * m] = s * x + c * y;
                }
                for i in 0..n {
                    let (x, y) = (v[i + p * n], v[i + q * n]);
                    v[i + p * n] = c * x - s * y;
                    v[i + q * n] = s * x + c * y;
                }
            }
        }
        if off < 1e-15 {
            break;
        }
    }
    let mut sv: Vec<(f64, usize)> = (0..n)
        .map(|j| {
            (
                (0..m)
                    .map(|i| u[i + j * m] * u[i + j * m])
                    .sum::<f64>()
                    .sqrt(),
                j,
            )
        })
        .collect();
    sv.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut uo = vec![0.0; m * n];
    let mut vo = vec![0.0; n * n];
    let mut s = Vec::with_capacity(n);
    for (k, (sk, j)) in sv.iter().enumerate() {
        s.push(*sk);
        for i in 0..m {
            uo[i + k * m] = if *sk > 0.0 { u[i + j * m] / sk } else { 0.0 };
        }
        for i in 0..n {
            vo[i + k * n] = v[i + j * n];
        }
    }
    (uo, s, vo)
}

fn svd(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "svd")?;
    let (m, n, x) = real_matrix(&a[0], "svd")?;
    let (u, s, v) = jacobi_svd(m, n, &x);
    let k = s.len();
    if nargout <= 1 {
        return one(Value::col(s));
    }
    let econ = a.len() > 1;
    if econ {
        let mut sd = vec![0.0; k * k];
        for i in 0..k {
            sd[i + i * k] = s[i];
        }
        return Ok(vec![
            Value::matrix(vec![m, k], u[..m * k].to_vec()),
            Value::matrix(vec![k, k], sd),
            Value::matrix(vec![n, k], v[..n * k].to_vec()),
        ]);
    }
    // Full U: complete the basis with QR of [U_k].
    let (q, _) = householder_qr(m, k, &u[..m * k]);
    let mut uf = q;
    for j in 0..k {
        // Keep the computed singular vectors (sign as computed).
        uf[j * m..j * m + m].copy_from_slice(&u[j * m..j * m + m]);
    }
    let mut sd = vec![0.0; m * n];
    for i in 0..k.min(m).min(n) {
        sd[i + i * m] = s[i];
    }
    Ok(vec![
        Value::matrix(vec![m, m], uf),
        Value::matrix(vec![m, n], sd),
        Value::matrix(vec![n, n], v),
    ])
}

#[allow(dead_code)]
fn unused(_: Num) {}
