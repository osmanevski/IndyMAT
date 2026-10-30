//! Shared helpers for the statistics builtins: argument parsing,
//! name/value options, ranks, small dense linear algebra and value
//! constructors (struct, cell).

use indy_value::{Array, Num, StructArray, Value};

use super::math::default_dim;
pub(super) use super::stats::{along, real_arg};
use super::{arg_f64, arg_str, err};
use crate::R;

pub fn strv(s: &str) -> Value {
    Value::str(s)
}

/// Scalar struct from `(name, value)` pairs.
pub fn st(fields: Vec<(&str, Value)>) -> Value {
    let mut s = StructArray::new_scalar();
    for (k, v) in fields {
        s.set(0, k, v);
    }
    Value::Struct(s)
}

/// Cell array of `dims` from column-major values.
pub fn cell(dims: Vec<usize>, v: Vec<Value>) -> Value {
    Value::Cell(Array::new(dims, v))
}

pub fn col(v: Vec<f64>) -> Value {
    Value::col(v)
}

pub fn mat(rows: usize, cols: usize, v: Vec<f64>) -> Value {
    Value::Num(Num::real(vec![rows, cols], v))
}

/// Operating dimension: explicit 1-based `dim` argument or first non-singleton.
pub fn dim_arg(x: &Num, v: Option<&Value>, fname: &str) -> R<usize> {
    match v {
        Some(v) if !v.is_empty() => {
            let d = arg_f64(v, fname)?;
            if d < 1.0 || d.fract() != 0.0 {
                return Err(err(
                    "MATLAB:getdimarg:dimensionMustBePositiveInteger",
                    "Dimension argument must be a positive integer scalar within indexing range.",
                ));
            }
            Ok(d as usize - 1)
        }
        _ => Ok(default_dim(&x.dims)),
    }
}

/// Split trailing `'Name', value` pairs (case-insensitive names) from `a`
/// starting at `from`.
pub fn name_values(a: &[Value], from: usize, fname: &str) -> R<Vec<(String, Value)>> {
    let mut out = Vec::new();
    let mut i = from;
    while i < a.len() {
        let Some(k) = a[i].to_str() else {
            return Err(err(
                "stats:InvalidOption",
                format!("{fname}: expected an option name at argument {}.", i + 1),
            ));
        };
        if i + 1 >= a.len() {
            return Err(err(
                "stats:MissingValue",
                format!("{fname}: option '{k}' needs a value."),
            ));
        }
        out.push((k.to_lowercase(), a[i + 1].clone()));
        i += 2;
    }
    Ok(out)
}

pub fn opt<'a>(o: &'a [(String, Value)], name: &str) -> Option<&'a Value> {
    o.iter()
        .rev()
        .find(|(k, _)| k == &name.to_lowercase())
        .map(|(_, v)| v)
}

pub fn opt_str(o: &[(String, Value)], name: &str, fname: &str) -> R<Option<String>> {
    match opt(o, name) {
        Some(v) => Ok(Some(arg_str(v, fname)?.to_lowercase())),
        None => Ok(None),
    }
}

/// Integer count argument within `[min, max]` (finite, integral).
pub fn count_arg(v: &Value, fname: &str, min: f64, max: f64) -> R<usize> {
    let x = arg_f64(v, fname)?;
    if !(x.is_finite() && x.fract() == 0.0 && x >= min && x <= max) {
        return Err(err(
            "stats:InvalidCount",
            format!("{fname}: expected an integer between {min} and {max}, got {x}."),
        ));
    }
    Ok(x as usize)
}

/// Result class: single if any input is single, else double.
pub fn cls(out: Num, ins: &[&Num]) -> Num {
    if ins.iter().any(|n| n.class == indy_value::NumClass::Single) {
        out.with_class(indy_value::NumClass::Single)
    } else {
        out
    }
}

pub fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

/// Sample variance (n − 1).
pub fn var(v: &[f64]) -> f64 {
    let m = mean(v);
    v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (v.len() as f64 - 1.0)
}

pub fn median(v: &[f64]) -> f64 {
    if v.is_empty() || v.iter().any(|x| x.is_nan()) {
        return f64::NAN;
    }
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let n = s.len();
    if n % 2 == 1 {
        s[n / 2]
    } else {
        (s[n / 2 - 1] + s[n / 2]) / 2.0
    }
}

/// Average ranks (ties share the mean rank) and Σ(t³ − t) over tie groups.
pub fn ranks(v: &[f64]) -> (Vec<f64>, f64) {
    let n = v.len();
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&a, &b| v[a].total_cmp(&v[b]));
    let mut r = vec![0.0; n];
    let mut tie = 0.0;
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j + 1 < n && v[idx[j + 1]] == v[idx[i]] {
            j += 1;
        }
        let avg = (i + j) as f64 / 2.0 + 1.0;
        for k in i..=j {
            r[idx[k]] = avg;
        }
        let t = (j - i + 1) as f64;
        tie += t * t * t - t;
        i = j + 1;
    }
    (r, tie)
}

/// Column-major dense matrix view of a real `Num` (2-D).
#[derive(Clone, Debug)]
pub struct M {
    pub r: usize,
    pub c: usize,
    pub d: Vec<f64>,
}

impl M {
    pub fn from_num(x: &Num, fname: &str) -> R<M> {
        if x.dims.len() > 2 {
            return Err(err(
                "MATLAB:expectedMatrix",
                format!("{fname}: input must be a 2-D matrix."),
            ));
        }
        Ok(M {
            r: x.dims[0],
            c: x.dims[1],
            d: x.re.clone(),
        })
    }
    pub fn zeros(r: usize, c: usize) -> M {
        M {
            r,
            c,
            d: vec![0.0; r * c],
        }
    }
    pub fn at(&self, i: usize, j: usize) -> f64 {
        self.d[i + j * self.r]
    }
    pub fn set(&mut self, i: usize, j: usize, v: f64) {
        self.d[i + j * self.r] = v;
    }
    pub fn col(&self, j: usize) -> &[f64] {
        &self.d[j * self.r..(j + 1) * self.r]
    }
    pub fn row(&self, i: usize) -> Vec<f64> {
        (0..self.c).map(|j| self.at(i, j)).collect()
    }
    pub fn value(self) -> Value {
        Value::Num(Num::real(vec![self.r, self.c], self.d))
    }
}

/// Solve the symmetric positive definite system `A x = b` (n×n, Cholesky).
pub fn chol_solve(a: &M, b: &[f64]) -> Option<Vec<f64>> {
    let n = a.r;
    let mut l = M::zeros(n, n);
    for j in 0..n {
        for i in j..n {
            let mut s = a.at(i, j);
            for k in 0..j {
                s -= l.at(i, k) * l.at(j, k);
            }
            if i == j {
                if s <= 0.0 {
                    return None;
                }
                l.set(i, j, s.sqrt());
            } else {
                l.set(i, j, s / l.at(j, j));
            }
        }
    }
    let mut y = vec![0.0; n];
    for i in 0..n {
        let mut s = b[i];
        for k in 0..i {
            s -= l.at(i, k) * y[k];
        }
        y[i] = s / l.at(i, i);
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut s = y[i];
        for k in i + 1..n {
            s -= l.at(k, i) * x[k];
        }
        x[i] = s / l.at(i, i);
    }
    Some(x)
}

/// Householder QR least squares with column pivoting. Returns
/// (coefficients, rank, inverse of RᵀR on the used columns embedded in a
/// p×p matrix, pivot order). Dependent columns get coefficient 0.
pub fn lstsq(x: &M, y: &[f64]) -> (Vec<f64>, usize, M) {
    let (n, p) = (x.r, x.c);
    let mut a = x.clone();
    let mut b = y.to_vec();
    let mut perm: Vec<usize> = (0..p).collect();
    let mut rank = 0;
    let scale = a.d.iter().fold(0.0f64, |m, v| m.max(v.abs())).max(1e-300);
    let tol = (n.max(p) as f64) * f64::EPSILON * scale;
    for k in 0..p.min(n) {
        // pivot: remaining column with the largest norm
        let (mut best, mut bj) = (-1.0, k);
        for j in k..p {
            let nn: f64 = (k..n).map(|i| a.at(i, j).powi(2)).sum();
            if nn > best {
                best = nn;
                bj = j;
            }
        }
        if best.sqrt() <= tol {
            break;
        }
        if bj != k {
            for i in 0..n {
                let t = a.at(i, k);
                a.set(i, k, a.at(i, bj));
                a.set(i, bj, t);
            }
            perm.swap(k, bj);
        }
        let nrm = (k..n).map(|i| a.at(i, k).powi(2)).sum::<f64>().sqrt();
        let alpha = if a.at(k, k) > 0.0 { -nrm } else { nrm };
        let mut v: Vec<f64> = (k..n).map(|i| a.at(i, k)).collect();
        v[0] -= alpha;
        let vn: f64 = v.iter().map(|t| t * t).sum();
        if vn > 0.0 {
            for j in k..p {
                let dot: f64 = (k..n).map(|i| v[i - k] * a.at(i, j)).sum();
                for i in k..n {
                    let nv = a.at(i, j) - 2.0 * dot * v[i - k] / vn;
                    a.set(i, j, nv);
                }
            }
            let dot: f64 = (k..n).map(|i| v[i - k] * b[i]).sum();
            for i in k..n {
                b[i] -= 2.0 * dot * v[i - k] / vn;
            }
        }
        rank += 1;
    }
    // back substitution on the leading rank×rank triangle
    let mut z = vec![0.0; rank];
    for i in (0..rank).rev() {
        let mut s = b[i];
        for j in i + 1..rank {
            s -= a.at(i, j) * z[j];
        }
        z[i] = s / a.at(i, i);
    }
    let mut beta = vec![0.0; p];
    for (k, zk) in z.iter().enumerate() {
        beta[perm[k]] = *zk;
    }
    // (RᵀR)⁻¹ = R⁻¹ R⁻ᵀ
    let mut rinv = M::zeros(rank, rank);
    for j in 0..rank {
        rinv.set(j, j, 1.0 / a.at(j, j));
        for i in (0..j).rev() {
            let mut s = 0.0;
            for k in i + 1..=j {
                s += a.at(i, k) * rinv.at(k, j);
            }
            rinv.set(i, j, -s / a.at(i, i));
        }
    }
    let mut cov = M::zeros(p, p);
    for i in 0..rank {
        for j in 0..rank {
            let s: f64 = (0..rank).map(|k| rinv.at(i, k) * rinv.at(j, k)).sum();
            cov.set(perm[i], perm[j], s);
        }
    }
    (beta, rank, cov)
}
