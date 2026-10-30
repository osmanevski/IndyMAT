//! Small dense linear algebra on column-major `m x n` f64 matrices.

/// `A * x` for an `m x n` matrix.
pub(super) fn matvec(a: &[f64], m: usize, n: usize, x: &[f64]) -> Vec<f64> {
    let mut y = vec![0.0; m];
    for j in 0..n {
        let xj = x[j];
        if xj == 0.0 {
            continue;
        }
        let col = &a[j * m..(j + 1) * m];
        for i in 0..m {
            y[i] += col[i] * xj;
        }
    }
    y
}

/// `A' * y` for an `m x n` matrix.
pub(super) fn tmatvec(a: &[f64], m: usize, n: usize, y: &[f64]) -> Vec<f64> {
    (0..n)
        .map(|j| {
            a[j * m..(j + 1) * m]
                .iter()
                .zip(y)
                .map(|(p, q)| p * q)
                .sum()
        })
        .collect()
}

/// Least-squares solution of `A x = b` (`m x n`, any shape) by Householder
/// QR with column pivoting. Rank-deficient problems get the basic
/// solution (zeros in the dependent columns), like MATLAB's backslash.
pub(super) fn lstsq(a: &[f64], m: usize, n: usize, b: &[f64]) -> Vec<f64> {
    let mut r = a.to_vec();
    let mut qtb = b.to_vec();
    let mut perm: Vec<usize> = (0..n).collect();
    let mut cn: Vec<f64> = (0..n)
        .map(|j| r[j * m..(j + 1) * m].iter().map(|v| v * v).sum())
        .collect();
    let k = m.min(n);
    let mut diag = vec![0.0; k];
    for p in 0..k {
        // pivot: the remaining column of largest norm
        let mut best = p;
        for j in p + 1..n {
            if cn[j] > cn[best] {
                best = j;
            }
        }
        if best != p {
            for i in 0..m {
                r.swap(p * m + i, best * m + i);
            }
            cn.swap(p, best);
            perm.swap(p, best);
        }
        let norm: f64 = r[p * m + p..(p + 1) * m]
            .iter()
            .map(|v| v * v)
            .sum::<f64>()
            .sqrt();
        if norm == 0.0 || !norm.is_finite() {
            diag[p] = 0.0;
            continue;
        }
        let alpha = if r[p * m + p] > 0.0 { -norm } else { norm };
        // v = x - alpha e1, stored in place
        r[p * m + p] -= alpha;
        let vnorm2: f64 = r[p * m + p..(p + 1) * m].iter().map(|v| v * v).sum();
        if vnorm2 > 0.0 {
            for j in p + 1..n {
                let mut s = 0.0;
                for i in p..m {
                    s += r[p * m + i] * r[j * m + i];
                }
                let f = 2.0 * s / vnorm2;
                for i in p..m {
                    r[j * m + i] -= f * r[p * m + i];
                }
            }
            let mut s = 0.0;
            for i in p..m {
                s += r[p * m + i] * qtb[i];
            }
            let f = 2.0 * s / vnorm2;
            for i in p..m {
                qtb[i] -= f * r[p * m + i];
            }
        }
        diag[p] = alpha;
        for j in p + 1..n {
            cn[j] = r[j * m + p + 1..(j + 1) * m].iter().map(|v| v * v).sum();
        }
    }
    let d0 = diag.first().map_or(0.0, |d| d.abs());
    let tol = (m.max(n) as f64) * f64::EPSILON * d0;
    let rank = diag
        .iter()
        .take_while(|d| d.abs() > tol && d0 > 0.0)
        .count();
    let mut z = vec![0.0; n];
    for p in (0..rank).rev() {
        let mut s = qtb[p];
        for j in p + 1..rank {
            s -= r[j * m + p] * z[j];
        }
        z[p] = s / diag[p];
    }
    let mut x = vec![0.0; n];
    for p in 0..n {
        x[perm[p]] = z[p];
    }
    x
}

/// Inverse of a square matrix by Gauss–Jordan elimination with partial
/// pivoting; `None` when singular.
pub(super) fn inverse(a: &[f64], n: usize) -> Option<Vec<f64>> {
    let mut m = a.to_vec();
    let mut inv = vec![0.0; n * n];
    for i in 0..n {
        inv[i * n + i] = 1.0;
    }
    for c in 0..n {
        let mut p = c;
        for r in c + 1..n {
            if m[c * n + r].abs() > m[c * n + p].abs() {
                p = r;
            }
        }
        let piv = m[c * n + p];
        if piv == 0.0 || !piv.is_finite() {
            return None;
        }
        if p != c {
            for j in 0..n {
                m.swap(j * n + p, j * n + c);
                inv.swap(j * n + p, j * n + c);
            }
        }
        for j in 0..n {
            m[j * n + c] /= piv;
            inv[j * n + c] /= piv;
        }
        for r in 0..n {
            if r == c {
                continue;
            }
            let f = m[c * n + r];
            if f == 0.0 {
                continue;
            }
            for j in 0..n {
                m[j * n + r] -= f * m[j * n + c];
                inv[j * n + r] -= f * inv[j * n + c];
            }
        }
    }
    Some(inv)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lstsq_square_and_overdetermined() {
        // [2 1; 1 3] x = [3; 5] -> x = [0.8; 1.4]
        let x = lstsq(&[2.0, 1.0, 1.0, 3.0], 2, 2, &[3.0, 5.0]);
        assert!((x[0] - 0.8).abs() < 1e-14 && (x[1] - 1.4).abs() < 1e-14);
        // fit y = a + b t to (0,1),(1,3),(2,5): a = 1, b = 2
        let x = lstsq(&[1.0, 1.0, 1.0, 0.0, 1.0, 2.0], 3, 2, &[1.0, 3.0, 5.0]);
        assert!((x[0] - 1.0).abs() < 1e-13 && (x[1] - 2.0).abs() < 1e-13);
        // rank deficient: no panic, finite answer
        let x = lstsq(&[1.0, 1.0, 1.0, 1.0], 2, 2, &[2.0, 2.0]);
        assert!(x.iter().all(|v| v.is_finite()));
        assert!(lstsq(&[], 0, 0, &[]).is_empty());
    }

    #[test]
    fn inverse_works() {
        let inv = inverse(&[4.0, 2.0, 7.0, 6.0], 2).unwrap();
        // [4 7; 2 6]^-1 = [0.6 -0.7; -0.2 0.4]
        let want = [0.6, -0.2, -0.7, 0.4];
        for (a, b) in inv.iter().zip(want) {
            assert!((a - b).abs() < 1e-14);
        }
        assert!(inverse(&[1.0, 2.0, 2.0, 4.0], 2).is_none());
    }
}
