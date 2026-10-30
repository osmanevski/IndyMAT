//! Small dense complex matrix kernel (column-major): LU, Householder QR,
//! one-sided Jacobi SVD, Cholesky, plus `sqrtm`/`logm` helpers. Real
//! matrices are the special case with zero imaginary parts. Own code from
//! the textbook algorithms.

use crate::Num;
use crate::ops::{C, c_abs, c_div, c_mul};

#[derive(Clone, Debug)]
pub struct CM {
    pub m: usize,
    pub n: usize,
    pub d: Vec<C>,
}

fn add(a: C, b: C) -> C {
    (a.0 + b.0, a.1 + b.1)
}
fn sub(a: C, b: C) -> C {
    (a.0 - b.0, a.1 - b.1)
}
fn conj(a: C) -> C {
    (a.0, -a.1)
}
fn scale(a: C, s: f64) -> C {
    (a.0 * s, a.1 * s)
}

impl CM {
    pub fn zeros(m: usize, n: usize) -> CM {
        CM {
            m,
            n,
            d: vec![(0.0, 0.0); m * n],
        }
    }
    pub fn eye(n: usize) -> CM {
        let mut r = CM::zeros(n, n);
        for i in 0..n {
            r.d[i + i * n] = (1.0, 0.0);
        }
        r
    }
    pub fn from_num(x: &Num) -> CM {
        CM {
            m: x.dims[0],
            n: x.dims[1],
            d: (0..x.re.len()).map(|i| (x.re[i], x.im_at(i))).collect(),
        }
    }
    pub fn to_num(&self) -> Num {
        let re = self.d.iter().map(|c| c.0).collect();
        let im = self.d.iter().map(|c| c.1).collect();
        Num::complex(vec![self.m, self.n], re, im)
    }
    pub fn at(&self, i: usize, j: usize) -> C {
        self.d[i + j * self.m]
    }
    pub fn set(&mut self, i: usize, j: usize, v: C) {
        self.d[i + j * self.m] = v;
    }
    pub fn ct(&self) -> CM {
        let mut r = CM::zeros(self.n, self.m);
        for i in 0..self.m {
            for j in 0..self.n {
                r.set(j, i, conj(self.at(i, j)));
            }
        }
        r
    }
    pub fn mul(&self, o: &CM) -> CM {
        let mut r = CM::zeros(self.m, o.n);
        for j in 0..o.n {
            for k in 0..self.n {
                let b = o.at(k, j);
                if b == (0.0, 0.0) {
                    continue;
                }
                for i in 0..self.m {
                    let v = add(r.at(i, j), c_mul(self.at(i, k), b));
                    r.set(i, j, v);
                }
            }
        }
        r
    }
    pub fn addm(&self, o: &CM) -> CM {
        CM {
            m: self.m,
            n: self.n,
            d: self.d.iter().zip(&o.d).map(|(a, b)| add(*a, *b)).collect(),
        }
    }
    pub fn subm(&self, o: &CM) -> CM {
        CM {
            m: self.m,
            n: self.n,
            d: self.d.iter().zip(&o.d).map(|(a, b)| sub(*a, *b)).collect(),
        }
    }
    pub fn scaled(&self, s: f64) -> CM {
        CM {
            m: self.m,
            n: self.n,
            d: self.d.iter().map(|a| scale(*a, s)).collect(),
        }
    }
    pub fn norm1(&self) -> f64 {
        (0..self.n)
            .map(|j| (0..self.m).map(|i| c_abs(self.at(i, j))).sum::<f64>())
            .fold(0.0, f64::max)
    }
    pub fn fro(&self) -> f64 {
        self.d
            .iter()
            .map(|c| c.0 * c.0 + c.1 * c.1)
            .sum::<f64>()
            .sqrt()
    }
    pub fn is_real(&self) -> bool {
        self.d.iter().all(|c| c.1 == 0.0)
    }
}

/// LU with partial pivoting: returns (LU packed, row permutation, sign,
/// singular flag).
pub fn lu(a: &CM) -> (CM, Vec<usize>, f64, bool) {
    let n = a.n.min(a.m);
    let (m, nc) = (a.m, a.n);
    let mut w = a.clone();
    let mut piv: Vec<usize> = (0..m).collect();
    let mut sign = 1.0;
    let mut singular = false;
    for col in 0..n {
        let mut best = col;
        for r in col + 1..m {
            if c_abs(w.at(r, col)) > c_abs(w.at(best, col)) {
                best = r;
            }
        }
        if c_abs(w.at(best, col)) == 0.0 {
            singular = true;
            continue;
        }
        if best != col {
            for c in 0..nc {
                let t = w.at(col, c);
                w.set(col, c, w.at(best, c));
                w.set(best, c, t);
            }
            piv.swap(col, best);
            sign = -sign;
        }
        let p = w.at(col, col);
        for r in col + 1..m {
            let f = c_div(w.at(r, col), p);
            w.set(r, col, f);
            if f != (0.0, 0.0) {
                for c in col + 1..nc {
                    let v = sub(w.at(r, c), c_mul(f, w.at(col, c)));
                    w.set(r, c, v);
                }
            }
        }
    }
    (w, piv, sign, singular)
}

pub fn det(a: &CM) -> C {
    let (w, _, sign, _) = lu(a);
    let mut d = (sign, 0.0);
    for i in 0..a.n {
        d = c_mul(d, w.at(i, i));
    }
    d
}

/// Solve a square system; returns (X, singular-or-ill-conditioned).
pub fn solve_square(a: &CM, b: &CM) -> (CM, bool) {
    let n = a.n;
    let (w, piv, _, mut singular) = lu(a);
    let diag: Vec<f64> = (0..n).map(|i| c_abs(w.at(i, i))).collect();
    let maxd = diag.iter().cloned().fold(0.0, f64::max);
    let mind = diag.iter().cloned().fold(f64::INFINITY, f64::min);
    if n > 0 && (maxd == 0.0 || mind / maxd < f64::EPSILON) {
        singular = true;
    }
    let mut x = CM::zeros(n, b.n);
    for j in 0..b.n {
        let mut y: Vec<C> = (0..n).map(|i| b.at(piv[i], j)).collect();
        for i in 0..n {
            for k in 0..i {
                y[i] = sub(y[i], c_mul(w.at(i, k), y[k]));
            }
        }
        for i in (0..n).rev() {
            for k in i + 1..n {
                y[i] = sub(y[i], c_mul(w.at(i, k), y[k]));
            }
            y[i] = c_div(y[i], w.at(i, i));
        }
        for i in 0..n {
            x.set(i, j, y[i]);
        }
    }
    (x, singular)
}

/// Householder QR (LAPACK sign convention: real diagonal in R).
/// Returns (Q m x m, R m x n).
pub fn qr(a: &CM) -> (CM, CM) {
    let (q, r, _) = qr_p(a, false);
    (q, r)
}

/// Householder QR, optionally with column pivoting (largest remaining
/// column norm first). Returns (Q, R, column permutation).
pub fn qr_p(a: &CM, pivot: bool) -> (CM, CM, Vec<usize>) {
    let (m, n) = (a.m, a.n);
    let mut r = a.clone();
    let mut q = CM::eye(m);
    let mut perm: Vec<usize> = (0..n).collect();
    for k in 0..m.min(n) {
        if pivot {
            let cn = |j: usize| -> f64 {
                (k..m)
                    .map(|i| {
                        let c = r.at(i, j);
                        c.0 * c.0 + c.1 * c.1
                    })
                    .sum()
            };
            let mut best = k;
            for j in k + 1..n {
                if cn(j) > cn(best) * (1.0 + 1e-14) {
                    best = j;
                }
            }
            if best != k {
                for i in 0..m {
                    let t = r.at(i, k);
                    r.set(i, k, r.at(i, best));
                    r.set(i, best, t);
                }
                perm.swap(k, best);
            }
        }
        let alpha = r.at(k, k);
        let xnorm = (k + 1..m)
            .map(|i| {
                let c = r.at(i, k);
                c.0 * c.0 + c.1 * c.1
            })
            .sum::<f64>()
            .sqrt();
        if xnorm == 0.0 && alpha.1 == 0.0 {
            continue;
        }
        let beta = -(alpha.0.hypot(alpha.1).hypot(xnorm)).copysign(alpha.0);
        let tau = ((beta - alpha.0) / beta, -alpha.1 / beta);
        let denom = sub(alpha, (beta, 0.0));
        let mut v: Vec<C> = vec![(1.0, 0.0)];
        for i in k + 1..m {
            v.push(c_div(r.at(i, k), denom));
        }
        // apply H^H = I - conj(tau) v v^H to R from the left
        let tc = conj(tau);
        for j in k..n {
            let mut dot = (0.0, 0.0);
            for i in k..m {
                dot = add(dot, c_mul(conj(v[i - k]), r.at(i, j)));
            }
            let f = c_mul(tc, dot);
            for i in k..m {
                let nv = sub(r.at(i, j), c_mul(f, v[i - k]));
                r.set(i, j, nv);
            }
        }
        // accumulate Q = Q H, H = I - tau v v^H
        for i in 0..m {
            let mut dot = (0.0, 0.0);
            for j in k..m {
                dot = add(dot, c_mul(q.at(i, j), v[j - k]));
            }
            let f = c_mul(dot, tau);
            for j in k..m {
                let nv = sub(q.at(i, j), c_mul(f, conj(v[j - k])));
                q.set(i, j, nv);
            }
        }
        r.set(k, k, (beta, 0.0));
        for i in k + 1..m {
            r.set(i, k, (0.0, 0.0));
        }
    }
    (q, r, perm)
}

/// Thin SVD by one-sided Jacobi: A (m x n) = U diag(s) V^H with
/// k = min(m, n); singular values descending.
pub fn svd_thin(a: &CM) -> (CM, Vec<f64>, CM) {
    if a.m < a.n {
        let (u, s, v) = svd_thin(&a.ct());
        return (v, s, u);
    }
    let (m, n) = (a.m, a.n);
    let mut u = a.clone();
    let mut v = CM::eye(n);
    for _sweep in 0..60 {
        let mut rotated = false;
        for p in 0..n {
            for q in p + 1..n {
                let (mut alpha, mut beta, mut gamma) = (0.0, 0.0, (0.0, 0.0));
                for i in 0..m {
                    let (x, y) = (u.at(i, p), u.at(i, q));
                    alpha += x.0 * x.0 + x.1 * x.1;
                    beta += y.0 * y.0 + y.1 * y.1;
                    gamma = add(gamma, c_mul(conj(x), y));
                }
                let g = c_abs(gamma);
                if g == 0.0 || g <= 1e-15 * (alpha * beta).sqrt() {
                    continue;
                }
                rotated = true;
                let ph = (gamma.0 / g, gamma.1 / g);
                let zeta = (beta - alpha) / (2.0 * g);
                let t = zeta.signum() / (zeta.abs() + (1.0 + zeta * zeta).sqrt());
                let t = if zeta == 0.0 { 1.0 } else { t };
                let c = 1.0 / (1.0 + t * t).sqrt();
                let s = c * t;
                for (mat, rows) in [(&mut u, m), (&mut v, n)] {
                    for i in 0..rows {
                        let x = mat.at(i, p);
                        let y = c_mul(mat.at(i, q), conj(ph));
                        mat.set(i, p, sub(scale(x, c), scale(y, s)));
                        mat.set(i, q, add(scale(x, s), scale(y, c)));
                    }
                }
            }
        }
        if !rotated {
            break;
        }
    }
    let norms: Vec<f64> = (0..n)
        .map(|j| {
            (0..m)
                .map(|i| {
                    let c = u.at(i, j);
                    c.0 * c.0 + c.1 * c.1
                })
                .sum::<f64>()
                .sqrt()
        })
        .collect();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        norms[b]
            .partial_cmp(&norms[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut uo = CM::zeros(m, n);
    let mut vo = CM::zeros(n, n);
    let mut s = Vec::with_capacity(n);
    for (k, &j) in order.iter().enumerate() {
        s.push(norms[j]);
        for i in 0..m {
            let c = u.at(i, j);
            uo.set(
                i,
                k,
                if norms[j] > 0.0 {
                    scale(c, 1.0 / norms[j])
                } else {
                    c
                },
            );
        }
        for i in 0..n {
            vo.set(i, k, v.at(i, j));
        }
    }
    (uo, s, vo)
}

/// Complete the orthonormal columns of `u` (m x k) to a full m x m unitary
/// matrix. Zero columns (rank deficiency) are replaced too.
pub fn complete_basis(u: &CM) -> CM {
    let (m, k) = (u.m, u.n);
    let mut full = CM::zeros(m, m);
    let mut have = 0;
    let norm2 = |col: &[C]| {
        col.iter()
            .map(|c| c.0 * c.0 + c.1 * c.1)
            .sum::<f64>()
            .sqrt()
    };
    let push = |full: &mut CM, mut col: Vec<C>, have: &mut usize| -> bool {
        for _ in 0..2 {
            for j in 0..*have {
                let mut dot = (0.0, 0.0);
                for i in 0..m {
                    dot = add(dot, c_mul(conj(full.at(i, j)), col[i]));
                }
                for i in 0..m {
                    col[i] = sub(col[i], c_mul(dot, full.at(i, j)));
                }
            }
        }
        let nr = norm2(&col);
        if nr < 1e-8 {
            return false;
        }
        for i in 0..m {
            full.set(i, *have, scale(col[i], 1.0 / nr));
        }
        *have += 1;
        true
    };
    for j in 0..k {
        let col: Vec<C> = (0..m).map(|i| u.at(i, j)).collect();
        push(&mut full, col, &mut have);
    }
    for e in 0..m {
        if have == m {
            break;
        }
        let mut col = vec![(0.0, 0.0); m];
        col[e] = (1.0, 0.0);
        push(&mut full, col, &mut have);
    }
    full
}

/// Cholesky `A = R^H R`: returns (R, p) with p = 0 on success, otherwise
/// the failing pivot index (1-based) and the partial `(p-1) x (p-1)` factor.
pub fn chol(a: &CM) -> (CM, usize) {
    let n = a.n;
    let mut r = CM::zeros(n, n);
    for j in 0..n {
        let mut s = a.at(j, j).0;
        for k in 0..j {
            let c = r.at(k, j);
            s -= c.0 * c.0 + c.1 * c.1;
        }
        if s <= 0.0 || s.is_nan() {
            let mut part = CM::zeros(j, j);
            for c in 0..j {
                for i in 0..=c {
                    part.set(i, c, r.at(i, c));
                }
            }
            return (part, j + 1);
        }
        let d = s.sqrt();
        r.set(j, j, (d, 0.0));
        for i in j + 1..n {
            let mut t = a.at(j, i);
            for k in 0..j {
                t = sub(t, c_mul(conj(r.at(k, j)), r.at(k, i)));
            }
            r.set(j, i, scale(t, 1.0 / d));
        }
    }
    (r, 0)
}

/// Principal square root by Denman–Beavers iteration. `None` when the
/// iteration does not converge (eigenvalues on the closed negative real
/// axis, singular matrix).
pub fn sqrtm(a: &CM) -> Option<CM> {
    let n = a.n;
    let mut y = a.clone();
    let mut z = CM::eye(n);
    for _ in 0..100 {
        let (yi, s1) = solve_square(&y, &CM::eye(n));
        let (zi, s2) = solve_square(&z, &CM::eye(n));
        if s1 || s2 {
            return None;
        }
        let yn = y.addm(&zi).scaled(0.5);
        let zn = z.addm(&yi).scaled(0.5);
        let diff = yn.subm(&y).fro();
        y = yn;
        z = zn;
        if !y.d.iter().all(|c| c.0.is_finite() && c.1.is_finite()) {
            return None;
        }
        if diff <= 1e-15 * y.fro().max(1e-300) * 4.0 {
            return Some(y);
        }
    }
    None
}

/// Principal matrix logarithm (inverse scaling and squaring).
pub fn logm(a: &CM) -> Option<CM> {
    let n = a.n;
    let id = CM::eye(n);
    let mut x = a.clone();
    let mut k = 0;
    while x.subm(&id).norm1() > 0.2 {
        x = sqrtm(&x)?;
        k += 1;
        if k > 60 {
            return None;
        }
    }
    // log X = 2 * sum S^(2j+1) / (2j+1), S = (X - I)(X + I)^-1
    let (xp, sing) = solve_square(&x.addm(&id), &id);
    if sing {
        return None;
    }
    let s = x.subm(&id).mul(&xp);
    let s2 = s.mul(&s);
    let mut term = s.clone();
    let mut sum = s.clone();
    for j in 1..60 {
        term = term.mul(&s2);
        let add_t = term.scaled(1.0 / (2 * j + 1) as f64);
        sum = sum.addm(&add_t);
        if add_t.fro() <= 1e-18 * sum.fro().max(1e-300) {
            break;
        }
    }
    Some(sum.scaled(2.0 * (1u64 << k) as f64))
}

/// Minimum-norm least-squares solution through the SVD.
pub fn solve_minnorm(a: &CM, b: &CM, tol: Option<f64>) -> CM {
    let (u, s, v) = svd_thin(a);
    let smax = s.first().copied().unwrap_or(0.0);
    let tol = tol.unwrap_or(a.m.max(a.n) as f64 * smax * f64::EPSILON);
    let mut x = CM::zeros(a.n, b.n);
    let uh = u.ct();
    for (k, sk) in s.iter().enumerate() {
        if *sk <= tol {
            continue;
        }
        for j in 0..b.n {
            let mut c = (0.0, 0.0);
            for i in 0..a.m {
                c = add(c, c_mul(uh.at(k, i), b.at(i, j)));
            }
            let c = scale(c, 1.0 / sk);
            for i in 0..a.n {
                let nv = add(x.at(i, j), c_mul(v.at(i, k), c));
                x.set(i, j, nv);
            }
        }
    }
    x
}
