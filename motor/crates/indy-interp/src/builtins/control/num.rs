//! Small numeric kernel for the control toolbox: complex scalars, real
//! polynomials (descending powers, MATLAB order) and dense real/complex
//! matrices (column-major). Eigenvalues, `expm` and polynomial roots are
//! delegated to the engine's own builtins so there is one implementation.

use indy_value::{Num, Value};

use crate::builtins::err;
use crate::{Interp, R};

// ---------------------------------------------------------------------------
// Complex scalars
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct C {
    pub re: f64,
    pub im: f64,
}

pub const fn c(re: f64, im: f64) -> C {
    C { re, im }
}

impl C {
    pub fn real(x: f64) -> C {
        c(x, 0.0)
    }
    pub fn abs(self) -> f64 {
        self.re.hypot(self.im)
    }
    pub fn arg(self) -> f64 {
        self.im.atan2(self.re)
    }
    pub fn conj(self) -> C {
        c(self.re, -self.im)
    }
    pub fn add(self, o: C) -> C {
        c(self.re + o.re, self.im + o.im)
    }
    pub fn sub(self, o: C) -> C {
        c(self.re - o.re, self.im - o.im)
    }
    pub fn mul(self, o: C) -> C {
        c(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }
    pub fn scale(self, k: f64) -> C {
        c(self.re * k, self.im * k)
    }
    pub fn div(self, o: C) -> C {
        // Smith's algorithm.
        if o.im == 0.0 {
            return c(self.re / o.re, self.im / o.re);
        }
        if o.re.abs() >= o.im.abs() {
            let r = o.im / o.re;
            let d = o.re + o.im * r;
            c((self.re + self.im * r) / d, (self.im - self.re * r) / d)
        } else {
            let r = o.re / o.im;
            let d = o.re * r + o.im;
            c((self.re * r + self.im) / d, (self.im * r - self.re) / d)
        }
    }
    pub fn inv(self) -> C {
        C::real(1.0).div(self)
    }
    pub fn exp(self) -> C {
        let m = self.re.exp();
        c(m * self.im.cos(), m * self.im.sin())
    }
    pub fn ln(self) -> C {
        c(self.abs().ln(), self.arg())
    }
    pub fn is_finite(self) -> bool {
        self.re.is_finite() && self.im.is_finite()
    }
}

/// Complex column/row values as an engine value (real when possible).
pub fn cvalue(dims: Vec<usize>, v: &[C]) -> Value {
    let re: Vec<f64> = v.iter().map(|x| x.re).collect();
    if v.iter().all(|x| x.im == 0.0) {
        return Value::Num(Num::real(dims, re));
    }
    let im: Vec<f64> = v.iter().map(|x| x.im).collect();
    Value::Num(Num::complex(dims, re, im))
}

pub fn ccol(v: &[C]) -> Value {
    cvalue(vec![v.len(), 1], v)
}

/// Complex data of a numeric value.
pub fn cdata(v: &Value) -> Option<Vec<C>> {
    let n = v.to_num()?;
    Some((0..n.re.len()).map(|i| c(n.re[i], n.im_at(i))).collect())
}

// ---------------------------------------------------------------------------
// Real polynomials (descending powers)
// ---------------------------------------------------------------------------

/// Drop leading zeros (keeps at least one coefficient).
pub fn ptrim(p: &[f64]) -> Vec<f64> {
    let first = p.iter().position(|x| *x != 0.0);
    match first {
        Some(i) => p[i..].to_vec(),
        None => vec![0.0],
    }
}

pub fn pconv(a: &[f64], b: &[f64]) -> Vec<f64> {
    if a.is_empty() || b.is_empty() {
        return vec![0.0];
    }
    let mut r = vec![0.0; a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        for (j, y) in b.iter().enumerate() {
            r[i + j] += x * y;
        }
    }
    r
}

pub fn padd(a: &[f64], b: &[f64]) -> Vec<f64> {
    let n = a.len().max(b.len());
    let mut r = vec![0.0; n];
    for (i, x) in a.iter().enumerate() {
        r[n - a.len() + i] += x;
    }
    for (i, x) in b.iter().enumerate() {
        r[n - b.len() + i] += x;
    }
    r
}

pub fn pscale(a: &[f64], k: f64) -> Vec<f64> {
    a.iter().map(|x| x * k).collect()
}

pub fn ppow(p: &[f64], n: usize) -> Vec<f64> {
    let mut r = vec![1.0];
    for _ in 0..n {
        r = pconv(&r, p);
    }
    r
}

pub fn peval(p: &[f64], x: C) -> C {
    let mut r = C::default();
    for k in p {
        r = r.mul(x).add(C::real(*k));
    }
    r
}

/// Monic polynomial with the given roots (real when the roots come in
/// conjugate pairs).
pub fn poly_from_roots(r: &[C]) -> Vec<f64> {
    let mut re = vec![1.0];
    let mut im = vec![0.0];
    for z in r {
        let n = re.len();
        let mut nre = vec![0.0; n + 1];
        let mut nim = vec![0.0; n + 1];
        for i in 0..n {
            nre[i] += re[i];
            nim[i] += im[i];
            // -z * coefficient
            nre[i + 1] -= z.re * re[i] - z.im * im[i];
            nim[i + 1] -= z.re * im[i] + z.im * re[i];
        }
        re = nre;
        im = nim;
    }
    re
}

/// Roots through the engine's `roots` (companion eigenvalues).
pub fn proots(it: &mut Interp, p: &[f64]) -> R<Vec<C>> {
    let p = ptrim(p);
    if p.len() <= 1 {
        return Ok(vec![]);
    }
    let out = it.call_named("roots", vec![Value::row(p)], 1)?;
    Ok(out.first().and_then(cdata).unwrap_or_default())
}

/// `p(s)` with `s` replaced by `(a s + b) / (c s + d)`, multiplied by
/// `(c s + d)^n` with `n = len(p) - 1`: used for bilinear maps.
pub fn psubst_bilinear(p: &[f64], a: f64, b: f64, cc: f64, d: f64, n: usize) -> Vec<f64> {
    let num = [a, b];
    let den = [cc, d];
    let m = p.len() - 1;
    let mut r = vec![0.0];
    for (i, k) in p.iter().enumerate() {
        let pw = m - i;
        let term = pconv(&ppow(&num, pw), &ppow(&den, n - pw));
        r = padd(&r, &pscale(&term, *k));
    }
    r
}

/// Real positive roots of a real polynomial (imaginary part below `tol`
/// relative to the magnitude).
pub fn positive_real_roots(it: &mut Interp, p: &[f64]) -> R<Vec<f64>> {
    let r = proots(it, p)?;
    let mut out: Vec<f64> = r
        .iter()
        .filter(|z| z.im.abs() <= 1e-7 * (1.0 + z.abs()) && z.re > 0.0)
        .map(|z| z.re)
        .collect();
    out.sort_by(|a, b| a.partial_cmp(b).unwrap());
    Ok(out)
}

// ---------------------------------------------------------------------------
// Real matrices (column-major)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Mat {
    pub r: usize,
    pub c: usize,
    pub d: Vec<f64>,
}

impl Mat {
    pub fn zeros(r: usize, c: usize) -> Mat {
        Mat {
            r,
            c,
            d: vec![0.0; r * c],
        }
    }
    pub fn eye(n: usize) -> Mat {
        let mut m = Mat::zeros(n, n);
        for i in 0..n {
            m.d[i + i * n] = 1.0;
        }
        m
    }
    pub fn scalar(x: f64) -> Mat {
        Mat {
            r: 1,
            c: 1,
            d: vec![x],
        }
    }
    pub fn col(v: &[f64]) -> Mat {
        Mat {
            r: v.len(),
            c: 1,
            d: v.to_vec(),
        }
    }
    pub fn at(&self, i: usize, j: usize) -> f64 {
        self.d[i + j * self.r]
    }
    pub fn set(&mut self, i: usize, j: usize, x: f64) {
        self.d[i + j * self.r] = x;
    }
    pub fn is_empty(&self) -> bool {
        self.d.is_empty()
    }
    pub fn from_value(v: &Value, what: &str) -> R<Mat> {
        let n = v.to_num().ok_or_else(|| {
            err(
                "Control:general:invalidType",
                format!("{} must be a numeric matrix.", what),
            )
        })?;
        if n.im.is_some() {
            return Err(err(
                "Control:general:complexData",
                format!("{}: complex data is not supported yet.", what),
            ));
        }
        if n.dims.len() > 2 {
            return Err(err(
                "Control:general:invalidType",
                format!("{} must be a 2-D matrix.", what),
            ));
        }
        Ok(Mat {
            r: n.dims[0],
            c: n.dims[1],
            d: n.re,
        })
    }
    pub fn to_value(&self) -> Value {
        Value::matrix(vec![self.r, self.c], self.d.clone())
    }
    pub fn t(&self) -> Mat {
        let mut m = Mat::zeros(self.c, self.r);
        for i in 0..self.r {
            for j in 0..self.c {
                m.set(j, i, self.at(i, j));
            }
        }
        m
    }
    pub fn mul(&self, o: &Mat) -> Mat {
        let mut m = Mat::zeros(self.r, o.c);
        for j in 0..o.c {
            for k in 0..self.c {
                let b = o.at(k, j);
                if b == 0.0 {
                    continue;
                }
                for i in 0..self.r {
                    m.d[i + j * self.r] += self.at(i, k) * b;
                }
            }
        }
        m
    }
    pub fn add(&self, o: &Mat) -> Mat {
        self.lin(1.0, o, 1.0)
    }
    pub fn sub(&self, o: &Mat) -> Mat {
        self.lin(1.0, o, -1.0)
    }
    /// `a*self + b*o`.
    pub fn lin(&self, a: f64, o: &Mat, b: f64) -> Mat {
        Mat {
            r: self.r,
            c: self.c,
            d: self
                .d
                .iter()
                .zip(&o.d)
                .map(|(x, y)| a * x + b * y)
                .collect(),
        }
    }
    pub fn scale(&self, k: f64) -> Mat {
        Mat {
            r: self.r,
            c: self.c,
            d: self.d.iter().map(|x| x * k).collect(),
        }
    }
    pub fn norm1(&self) -> f64 {
        (0..self.c)
            .map(|j| (0..self.r).map(|i| self.at(i, j).abs()).sum::<f64>())
            .fold(0.0, f64::max)
    }
    /// `[self o]`.
    pub fn hcat(&self, o: &Mat) -> Mat {
        let r = if self.c == 0 { o.r } else { self.r };
        let mut d = self.d.clone();
        d.extend_from_slice(&o.d);
        Mat {
            r,
            c: self.c + o.c,
            d,
        }
    }
    /// `[self; o]`.
    pub fn vcat(&self, o: &Mat) -> Mat {
        let c = if self.r == 0 { o.c } else { self.c };
        let mut m = Mat::zeros(self.r + o.r, c);
        for j in 0..c {
            for i in 0..self.r {
                m.set(i, j, self.at(i, j));
            }
            for i in 0..o.r {
                m.set(self.r + i, j, o.at(i, j));
            }
        }
        m
    }
    pub fn blkdiag(&self, o: &Mat) -> Mat {
        let mut m = Mat::zeros(self.r + o.r, self.c + o.c);
        for j in 0..self.c {
            for i in 0..self.r {
                m.set(i, j, self.at(i, j));
            }
        }
        for j in 0..o.c {
            for i in 0..o.r {
                m.set(self.r + i, self.c + j, o.at(i, j));
            }
        }
        m
    }
    pub fn block(&self, r0: usize, r1: usize, c0: usize, c1: usize) -> Mat {
        let mut m = Mat::zeros(r1 - r0, c1 - c0);
        for j in c0..c1 {
            for i in r0..r1 {
                m.set(i - r0, j - c0, self.at(i, j));
            }
        }
        m
    }
    pub fn col_of(&self, j: usize) -> Mat {
        self.block(0, self.r, j, j + 1)
    }
    pub fn row_of(&self, i: usize) -> Mat {
        self.block(i, i + 1, 0, self.c)
    }
    /// Solve `self * X = b` (partial pivoting). `None` when singular.
    pub fn solve(&self, b: &Mat) -> Option<Mat> {
        let n = self.r;
        if n != self.c || b.r != n {
            return None;
        }
        let mut a = self.clone();
        let mut x = b.clone();
        let scale = a.norm1().max(f64::MIN_POSITIVE);
        for k in 0..n {
            let mut p = k;
            for i in k + 1..n {
                if a.at(i, k).abs() > a.at(p, k).abs() {
                    p = i;
                }
            }
            if a.at(p, k).abs() <= 1e-14 * scale {
                return None;
            }
            if p != k {
                for j in 0..n {
                    let t = a.at(k, j);
                    a.set(k, j, a.at(p, j));
                    a.set(p, j, t);
                }
                for j in 0..x.c {
                    let t = x.at(k, j);
                    x.set(k, j, x.at(p, j));
                    x.set(p, j, t);
                }
            }
            let piv = a.at(k, k);
            for i in k + 1..n {
                let f = a.at(i, k) / piv;
                if f == 0.0 {
                    continue;
                }
                for j in k..n {
                    let v = a.at(i, j) - f * a.at(k, j);
                    a.set(i, j, v);
                }
                for j in 0..x.c {
                    let v = x.at(i, j) - f * x.at(k, j);
                    x.set(i, j, v);
                }
            }
        }
        for j in 0..x.c {
            for k in (0..n).rev() {
                let mut s = x.at(k, j);
                for m in k + 1..n {
                    s -= a.at(k, m) * x.at(m, j);
                }
                x.set(k, j, s / a.at(k, k));
            }
        }
        Some(x)
    }
    pub fn inv(&self) -> Option<Mat> {
        self.solve(&Mat::eye(self.r))
    }
    /// Least-squares solution of `self * X = b` (normal equations; the
    /// systems here are small and well conditioned).
    pub fn lstsq(&self, b: &Mat) -> Option<Mat> {
        let at = self.t();
        at.mul(self).solve(&at.mul(b))
    }
    pub fn symmetrize(&self) -> Mat {
        self.lin(0.5, &self.t(), 0.5)
    }
}

pub fn expm(it: &mut Interp, a: &Mat) -> R<Mat> {
    if a.r == 0 {
        return Ok(Mat::zeros(0, 0));
    }
    let out = it.call_named("expm", vec![a.to_value()], 1)?;
    Mat::from_value(&out[0], "expm")
}

pub fn eigvals(it: &mut Interp, a: &Mat) -> R<Vec<C>> {
    if a.r == 0 {
        return Ok(vec![]);
    }
    let out = it.call_named("eig", vec![a.to_value()], 1)?;
    Ok(cdata(&out[0]).unwrap_or_default())
}

pub fn rank(it: &mut Interp, a: &Mat) -> R<usize> {
    if a.is_empty() {
        return Ok(0);
    }
    let out = it.call_named("rank", vec![a.to_value()], 1)?;
    Ok(out[0].scalar_f64().unwrap_or(0.0) as usize)
}

/// Matrix logarithm by inverse scaling and squaring (Denman–Beavers
/// square roots, then the Gregory series of `log(I+X)`), real input.
pub fn logm(a: &Mat) -> Option<Mat> {
    let n = a.r;
    let id = Mat::eye(n);
    let mut y = a.clone();
    let mut k = 0;
    while y.sub(&id).norm1() > 0.25 {
        // Denman–Beavers iteration for sqrt(y).
        let mut s = y.clone();
        let mut z = Mat::eye(n);
        for _ in 0..100 {
            let si = s.inv()?;
            let zi = z.inv()?;
            let ns = s.lin(0.5, &zi, 0.5);
            let nz = z.lin(0.5, &si, 0.5);
            let done = ns.sub(&s).norm1() <= 1e-15 * ns.norm1();
            s = ns;
            z = nz;
            if done {
                break;
            }
        }
        y = s;
        k += 1;
        if k > 60 {
            return None;
        }
    }
    // log(Y) = 2 atanh((Y-I)(Y+I)^-1) = 2 sum Z^(2j+1)/(2j+1)
    let zm = y.add(&id).t().solve(&y.sub(&id).t())?.t();
    let z2 = zm.mul(&zm);
    let mut term = zm.clone();
    let mut sum = zm.clone();
    for j in 1..200 {
        term = term.mul(&z2);
        let add = term.scale(1.0 / (2 * j + 1) as f64);
        sum = sum.add(&add);
        if add.norm1() <= 1e-17 * sum.norm1().max(1e-300) {
            break;
        }
    }
    Some(sum.scale(2.0 * (1u64 << k) as f64))
}

// ---------------------------------------------------------------------------
// Complex matrices (for frequency responses)
// ---------------------------------------------------------------------------

/// Solve `M x = b` for complex `M` (n×n) and several right-hand sides.
pub fn csolve(n: usize, m: &[C], b: &[C], nrhs: usize) -> Option<Vec<C>> {
    let mut a = m.to_vec();
    let mut x = b.to_vec();
    let at = |a: &[C], i: usize, j: usize| a[i + j * n];
    let scale = a.iter().map(|z| z.abs()).fold(0.0, f64::max).max(1e-300);
    for k in 0..n {
        let mut p = k;
        for i in k + 1..n {
            if at(&a, i, k).abs() > at(&a, p, k).abs() {
                p = i;
            }
        }
        if at(&a, p, k).abs() <= 1e-15 * scale {
            return None;
        }
        if p != k {
            for j in 0..n {
                a.swap(k + j * n, p + j * n);
            }
            for j in 0..nrhs {
                x.swap(k + j * n, p + j * n);
            }
        }
        let piv = at(&a, k, k);
        for i in k + 1..n {
            let f = at(&a, i, k).div(piv);
            for j in k..n {
                a[i + j * n] = a[i + j * n].sub(f.mul(a[k + j * n]));
            }
            for j in 0..nrhs {
                x[i + j * n] = x[i + j * n].sub(f.mul(x[k + j * n]));
            }
        }
    }
    for j in 0..nrhs {
        for k in (0..n).rev() {
            let mut s = x[k + j * n];
            for q in k + 1..n {
                s = s.sub(a[k + q * n].mul(x[q + j * n]));
            }
            x[k + j * n] = s.div(a[k + k * n]);
        }
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poly_roots_roundtrip() {
        let p = poly_from_roots(&[c(-1.0, 2.0), c(-1.0, -2.0), c(-3.0, 0.0)]);
        // (s^2 + 2s + 5)(s + 3) = s^3 + 5 s^2 + 11 s + 15
        assert_eq!(p, vec![1.0, 5.0, 11.0, 15.0]);
    }

    #[test]
    fn solve_and_logm() {
        let a = Mat {
            r: 2,
            c: 2,
            d: vec![0.0, -2.0, 1.0, -3.0],
        };
        let x = a.solve(&Mat::col(&[1.0, 0.0])).unwrap();
        assert!((x.d[0] + 1.5).abs() < 1e-14 && (x.d[1] - 1.0).abs() < 1e-14);
        // logm(diag(e, e^2)) = diag(1, 2)
        let e = Mat {
            r: 2,
            c: 2,
            d: vec![1f64.exp(), 0.0, 0.0, 2f64.exp()],
        };
        let l = logm(&e).unwrap();
        assert!((l.d[0] - 1.0).abs() < 1e-12 && (l.d[3] - 2.0).abs() < 1e-12);
        assert!(l.d[1].abs() < 1e-12 && l.d[2].abs() < 1e-12);
    }
}
