//! Sparse matrices: compressed sparse column storage (double, logical,
//! complex), always 2-D, with no explicit zeros and rows sorted in each
//! column.
//!
//! Operators follow MATLAB's result rules (see [`result_is_sparse`]). Common
//! operations run on the nonzeros; everything else, including `A\b`, computes
//! through the dense code and converts the result, refusing with an explicit
//! error when the dense form would exceed [`DENSE_LIMIT`] elements.

use crate::ast::{BinOp, UnOp};
use crate::index::{Sub, out_of_bound, read_positions};
use crate::{Error, Num, NumClass, Res, Value, ops};

/// Most elements a sparse matrix may expand to in the dense code paths.
pub const DENSE_LIMIT: usize = 1 << 28;

/// Largest row or column count an indexed assignment may grow to.
const MAX_GROWN_EXTENT: usize = 1 << 31;

/// Compressed sparse column matrix. Logical matrices store `1.0`.
#[derive(Clone, Debug, PartialEq)]
pub struct Sparse {
    pub dims: [usize; 2],
    pub logical: bool,
    pub colptr: Vec<usize>,
    pub rowidx: Vec<usize>,
    pub re: Vec<f64>,
    pub im: Option<Vec<f64>>,
}

/// One stored element: row, column, real part, imaginary part.
pub type Entry = (usize, usize, f64, f64);

fn too_big(m: usize, n: usize) -> Box<Error> {
    Error::new(
        "MATLAB:nomem",
        format!(
            "Out of memory: this operation needs the {}x{} sparse matrix in full form.",
            m, n
        ),
    )
}

impl Sparse {
    pub fn zeros(m: usize, n: usize, logical: bool) -> Sparse {
        Sparse {
            dims: [m, n],
            logical,
            colptr: vec![0; n + 1],
            rowidx: Vec::new(),
            re: Vec::new(),
            im: None,
        }
    }

    pub fn nnz(&self) -> usize {
        self.re.len()
    }

    /// Bytes MATLAB reports for this storage: an index and a value for each
    /// element (room for at least one), plus the column table.
    pub fn storage_bytes(&self) -> u64 {
        let value: u64 = match (self.logical, self.im.is_some()) {
            (true, _) => 1,
            (false, false) => 8,
            (false, true) => 16,
        };
        self.nnz().max(1) as u64 * (8 + value) + (self.dims[1] as u64 + 1) * 8
    }

    pub fn entry_im(&self, k: usize) -> f64 {
        self.im.as_ref().map_or(0.0, |v| v[k])
    }

    /// Stored elements in column-major order.
    pub fn entries(&self) -> Vec<Entry> {
        let mut out = Vec::with_capacity(self.nnz());
        for c in 0..self.dims[1] {
            for k in self.colptr[c]..self.colptr[c + 1] {
                out.push((self.rowidx[k], c, self.re[k], self.entry_im(k)));
            }
        }
        out
    }

    /// Build from entries already in column-major order without duplicates;
    /// zero entries are dropped.
    pub fn from_sorted(m: usize, n: usize, logical: bool, entries: &[Entry]) -> Sparse {
        let mut s = Sparse::zeros(m, n, logical);
        let complex = entries.iter().any(|e| e.3 != 0.0);
        let mut im = Vec::new();
        for &(r, c, x, y) in entries {
            if x == 0.0 && y == 0.0 {
                continue;
            }
            s.colptr[c + 1] += 1;
            s.rowidx.push(r);
            s.re.push(if logical { 1.0 } else { x });
            if complex {
                im.push(y);
            }
        }
        for c in 0..n {
            s.colptr[c + 1] += s.colptr[c];
        }
        if complex && !logical {
            s.im = Some(im);
        }
        s.narrow();
        s
    }

    /// Build from entries in any order: duplicates add up (logical: OR).
    pub fn from_triplets(m: usize, n: usize, logical: bool, mut entries: Vec<Entry>) -> Sparse {
        entries.sort_by_key(|e| (e.1, e.0));
        let mut merged: Vec<Entry> = Vec::with_capacity(entries.len());
        for e in entries {
            match merged.last_mut() {
                Some(l) if l.0 == e.0 && l.1 == e.1 => {
                    if logical {
                        l.2 = if l.2 != 0.0 || e.2 != 0.0 { 1.0 } else { 0.0 };
                    } else {
                        l.2 += e.2;
                        l.3 += e.3;
                    }
                }
                _ => merged.push(e),
            }
        }
        Sparse::from_sorted(m, n, logical, &merged)
    }

    /// MATLAB `sparse(A)`: double or logical, 2-D.
    pub fn from_value(v: &Value) -> Res<Sparse> {
        match v {
            Value::Sparse(s) => Ok((**s).clone()),
            Value::Num(n) if n.class == NumClass::Double => {
                if n.dims.len() > 2 {
                    return Err(nd_error());
                }
                let (m, c) = (n.dims[0], n.dims[1]);
                let mut e = Vec::new();
                for j in 0..c {
                    for i in 0..m {
                        let k = i + j * m;
                        if n.re[k] != 0.0 || n.im_at(k) != 0.0 {
                            e.push((i, j, n.re[k], n.im_at(k)));
                        }
                    }
                }
                Ok(Sparse::from_sorted(m, c, false, &e))
            }
            Value::Bool(a) => {
                if a.dims.len() > 2 {
                    return Err(nd_error());
                }
                let (m, c) = (a.dims[0], a.dims[1]);
                let mut e = Vec::new();
                for j in 0..c {
                    for i in 0..m {
                        if a.data[i + j * m] {
                            e.push((i, j, 1.0, 0.0));
                        }
                    }
                }
                Ok(Sparse::from_sorted(m, c, true, &e))
            }
            other => Err(Error::new(
                "MATLAB:UndefinedFunction",
                format!(
                    "Undefined function 'sparse' for input arguments of type '{}'.",
                    other.class_name()
                ),
            )),
        }
    }

    /// Dense form, or `None` above [`DENSE_LIMIT`].
    pub fn dense(&self) -> Option<Value> {
        let [m, n] = self.dims;
        let total = m.checked_mul(n).filter(|t| *t <= DENSE_LIMIT)?;
        if self.logical {
            let mut d = vec![false; total];
            for (r, c, _, _) in self.entries() {
                d[r + c * m] = true;
            }
            return Some(crate::bool_array(vec![m, n], d));
        }
        let mut re = vec![0.0; total];
        let mut im = self.im.as_ref().map(|_| vec![0.0; total]);
        for (r, c, x, y) in self.entries() {
            re[r + c * m] = x;
            if let Some(i) = &mut im {
                i[r + c * m] = y;
            }
        }
        Some(Value::Num(Num {
            class: NumClass::Double,
            dims: vec![m, n],
            re,
            im,
        }))
    }

    pub fn to_dense(&self) -> Res<Value> {
        self.dense()
            .ok_or_else(|| too_big(self.dims[0], self.dims[1]))
    }

    /// Value at a 0-based position: position in storage when stored.
    pub fn find(&self, r: usize, c: usize) -> Option<usize> {
        let (a, b) = (self.colptr[c], self.colptr[c + 1]);
        self.rowidx[a..b].binary_search(&r).ok().map(|k| a + k)
    }

    pub fn transposed(&self, conj: bool) -> Sparse {
        let [m, n] = self.dims;
        let mut counts = vec![0usize; m + 1];
        for &r in &self.rowidx {
            counts[r + 1] += 1;
        }
        for i in 0..m {
            counts[i + 1] += counts[i];
        }
        let colptr = counts.clone();
        let nz = self.nnz();
        let mut rowidx = vec![0; nz];
        let mut re = vec![0.0; nz];
        let mut im = self.im.as_ref().map(|_| vec![0.0; nz]);
        for c in 0..n {
            for k in self.colptr[c]..self.colptr[c + 1] {
                let r = self.rowidx[k];
                let p = counts[r];
                counts[r] += 1;
                rowidx[p] = c;
                re[p] = self.re[k];
                if let Some(i) = &mut im {
                    let y = self.entry_im(k);
                    i[p] = if conj { -y } else { y };
                }
            }
        }
        Sparse {
            dims: [n, m],
            logical: self.logical,
            colptr,
            rowidx,
            re,
            im,
        }
    }

    /// Same elements in another shape (the caller checks the element count).
    pub fn reshaped(&self, m2: usize, n2: usize) -> Sparse {
        let m = self.dims[0];
        let e: Vec<Entry> = self
            .entries()
            .into_iter()
            .map(|(r, c, x, y)| {
                let p = r + c * m;
                (p % m2.max(1), p / m2.max(1), x, y)
            })
            .collect();
        Sparse::from_sorted(m2, n2, self.logical, &e)
    }

    /// Apply a zero-preserving function to the stored elements.
    pub fn map_nz(&self, logical: bool, f: impl Fn(f64, f64) -> (f64, f64)) -> Sparse {
        let e: Vec<Entry> = self
            .entries()
            .into_iter()
            .map(|(r, c, x, y)| {
                let (p, q) = f(x, y);
                (r, c, p, q)
            })
            .collect();
        Sparse::from_sorted(self.dims[0], self.dims[1], logical, &e)
    }

    /// As a double matrix.
    pub fn as_double(&self) -> Sparse {
        let mut s = self.clone();
        s.logical = false;
        s
    }

    fn narrow(&mut self) {
        if let Some(im) = &self.im
            && im.iter().all(|v| *v == 0.0)
        {
            self.im = None;
        }
    }

    fn all_finite(&self) -> bool {
        self.re.iter().all(|v| v.is_finite())
            && self
                .im
                .as_ref()
                .is_none_or(|v| v.iter().all(|x| x.is_finite()))
    }
}

fn nd_error() -> Box<Error> {
    Error::new(
        "MATLAB:sparse:NDInput",
        "N-dimensional indexing or storage is not supported for sparse matrices.",
    )
}

/// A dense copy of a sparse value; other values are cloned.
pub fn full(v: &Value) -> Res<Value> {
    match v {
        Value::Sparse(s) => s.to_dense(),
        other => Ok(other.clone()),
    }
}

/// Convert a dense double or logical matrix; other values stay as they are.
pub fn sparsify(v: Value) -> Value {
    let convertible = match &v {
        Value::Num(n) => {
            n.class == NumClass::Double && n.dims.len() == 2 && n.re.len() == n.dims[0] * n.dims[1]
        }
        Value::Bool(a) => a.dims.len() == 2,
        _ => false,
    };
    if convertible {
        match Sparse::from_value(&v) {
            Ok(s) => Value::Sparse(Box::new(s)),
            Err(_) => v,
        }
    } else {
        v
    }
}

// ---------------------------------------------------------------------------
// Operators
// ---------------------------------------------------------------------------

/// MATLAB's rule for the storage of `a op b` when an operand is sparse: two
/// sparse operands stay sparse; mixed operands give a full result unless it
/// is structurally sparse (products, quotients by a full operand, positive
/// powers, `&`), and comparisons stay sparse. Where the documentation leaves
/// a case open, Octave decided (`c./S`, `c.^S`, `S.^0`, `S|F`).
pub fn result_is_sparse(op: BinOp, a: &Value, b: &Value) -> bool {
    let (sa, sb) = (a.is_sparse(), b.is_sparse());
    if sa && sb {
        return true;
    }
    match op {
        BinOp::Add | BinOp::Sub | BinOp::Or | BinOp::AndAnd | BinOp::OrOr => false,
        BinOp::Mul => (sa && b.is_scalar()) || (sb && a.is_scalar()),
        BinOp::Div => sa && b.is_scalar(),
        BinOp::LeftDiv => sb && a.is_scalar(),
        BinOp::Pow => sa,
        BinOp::EDiv => sa,
        BinOp::ELeftDiv => sb,
        BinOp::EPow => sa && real_scalar(b).is_none_or(|p| p > 0.0),
        BinOp::EMul
        | BinOp::Eq
        | BinOp::Ne
        | BinOp::Lt
        | BinOp::Le
        | BinOp::Gt
        | BinOp::Ge
        | BinOp::And => true,
    }
}

fn compare_real(op: BinOp, p: f64, q: f64) -> bool {
    match op {
        BinOp::Eq => p == q,
        BinOp::Ne => p != q,
        BinOp::Lt => p < q,
        BinOp::Le => p <= q,
        BinOp::Gt => p > q,
        _ => p >= q,
    }
}

fn is_compare(op: BinOp) -> bool {
    matches!(
        op,
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
    )
}

/// A real double or logical scalar operand.
fn real_scalar(v: &Value) -> Option<f64> {
    match v {
        Value::Num(n) if n.class == NumClass::Double && n.im.is_none() && n.re.len() == 1 => {
            Some(n.re[0])
        }
        Value::Bool(a) if a.data.len() == 1 => Some(a.data[0] as u8 as f64),
        _ => None,
    }
}

/// `S op c` on the nonzeros, when zeros stay zeros.
fn with_scalar(op: BinOp, s: &Sparse, c: f64) -> Option<Sparse> {
    match op {
        BinOp::Mul | BinOp::EMul if c.is_finite() && s.all_finite() => {
            Some(s.map_nz(false, |x, y| (x * c, y * c)))
        }
        BinOp::Div | BinOp::EDiv if c.is_finite() && c != 0.0 && s.all_finite() => {
            Some(s.map_nz(false, |x, y| (x / c, y / c)))
        }
        BinOp::EPow if c > 0.0 && c.fract() == 0.0 && s.im.is_none() => {
            Some(s.map_nz(false, |x, _| (x.powf(c), 0.0)))
        }
        BinOp::And if c != 0.0 && !c.is_nan() => Some(s.map_nz(true, |_, _| (1.0, 0.0))),
        _ if is_compare(op) && s.im.is_none() && !compare_real(op, 0.0, c) => {
            let v = |x: f64| (compare_real(op, x, c) as u8 as f64, 0.0);
            Some(s.map_nz(true, |x, _| v(x)))
        }
        _ => None,
    }
}

/// The mirrored operator of `c op S` as `S op' c`, when one exists.
fn mirrored(op: BinOp) -> Option<BinOp> {
    Some(match op {
        BinOp::Mul | BinOp::EMul | BinOp::Eq | BinOp::Ne | BinOp::And => op,
        BinOp::ELeftDiv => BinOp::EDiv,
        BinOp::LeftDiv => BinOp::Div,
        BinOp::Lt => BinOp::Gt,
        BinOp::Le => BinOp::Ge,
        BinOp::Gt => BinOp::Lt,
        BinOp::Ge => BinOp::Le,
        _ => return None,
    })
}

/// Merge two same-size matrices; `f` gets `(x, y)` with zeros for gaps.
fn merge(a: &Sparse, b: &Sparse, logical: bool, f: impl Fn(Entry, Entry) -> (f64, f64)) -> Sparse {
    let [m, n] = a.dims;
    let mut out = Vec::with_capacity(a.nnz() + b.nnz());
    for c in 0..n {
        let (mut p, pe) = (a.colptr[c], a.colptr[c + 1]);
        let (mut q, qe) = (b.colptr[c], b.colptr[c + 1]);
        while p < pe || q < qe {
            let ra = if p < pe { a.rowidx[p] } else { usize::MAX };
            let rb = if q < qe { b.rowidx[q] } else { usize::MAX };
            let r = ra.min(rb);
            let x = if ra == r {
                p += 1;
                (r, c, a.re[p - 1], a.entry_im(p - 1))
            } else {
                (r, c, 0.0, 0.0)
            };
            let y = if rb == r {
                q += 1;
                (r, c, b.re[q - 1], b.entry_im(q - 1))
            } else {
                (r, c, 0.0, 0.0)
            };
            let (u, v) = f(x, y);
            out.push((r, c, u, v));
        }
    }
    Sparse::from_sorted(m, n, logical, &out)
}

/// Sparse product, one accumulator column at a time.
fn matmul(a: &Sparse, b: &Sparse) -> Sparse {
    let (m, n) = (a.dims[0], b.dims[1]);
    let mut wr = vec![0.0; m];
    let mut wi = vec![0.0; m];
    let mut stamp = vec![usize::MAX; m];
    let mut out = Vec::new();
    for j in 0..n {
        let mut rows = Vec::new();
        for kb in b.colptr[j]..b.colptr[j + 1] {
            let (k, y, yi) = (b.rowidx[kb], b.re[kb], b.entry_im(kb));
            for ka in a.colptr[k]..a.colptr[k + 1] {
                let i = a.rowidx[ka];
                if stamp[i] != j {
                    stamp[i] = j;
                    wr[i] = 0.0;
                    wi[i] = 0.0;
                    rows.push(i);
                }
                let (x, xi) = (a.re[ka], a.entry_im(ka));
                wr[i] += x * y - xi * yi;
                wi[i] += x * yi + xi * y;
            }
        }
        rows.sort_unstable();
        for i in rows {
            out.push((i, j, wr[i], wi[i]));
        }
    }
    Sparse::from_sorted(m, n, false, &out)
}

/// A real, finite, double, 2-D dense matrix.
fn plain_matrix(v: &Value) -> Option<&Num> {
    match v {
        Value::Num(n)
            if n.class == NumClass::Double
                && n.im.is_none()
                && n.dims.len() == 2
                && n.re.iter().all(|x| x.is_finite()) =>
        {
            Some(n)
        }
        _ => None,
    }
}

fn native(op: BinOp, a: &Value, b: &Value) -> Option<Value> {
    let wrap = |s: Sparse| Some(Value::Sparse(Box::new(s)));
    match (a, b) {
        (Value::Sparse(x), Value::Sparse(y)) => {
            if x.dims == y.dims && x.nnz() + y.nnz() > 0 && x.dims != [1, 1] {
                match op {
                    BinOp::Add => {
                        return wrap(merge(x, y, false, |p, q| (p.2 + q.2, p.3 + q.3)));
                    }
                    BinOp::Sub => {
                        return wrap(merge(x, y, false, |p, q| (p.2 - q.2, p.3 - q.3)));
                    }
                    BinOp::EMul if x.all_finite() && y.all_finite() => {
                        return wrap(merge(x, y, false, |p, q| {
                            (p.2 * q.2 - p.3 * q.3, p.2 * q.3 + p.3 * q.2)
                        }));
                    }
                    _ => {}
                }
            }
            if x.dims == y.dims && matches!(op, BinOp::And | BinOp::Or) {
                let and = op == BinOp::And;
                return wrap(merge(x, y, true, |p, q| {
                    let (l, r) = (p.2 != 0.0 || p.3 != 0.0, q.2 != 0.0 || q.3 != 0.0);
                    ((if and { l && r } else { l || r }) as u8 as f64, 0.0)
                }));
            }
            if op == BinOp::Mul
                && x.dims != [1, 1]
                && y.dims != [1, 1]
                && x.dims[1] == y.dims[0]
                && x.all_finite()
                && y.all_finite()
            {
                return wrap(matmul(x, y));
            }
            None
        }
        (Value::Sparse(x), other) => {
            if let Some(c) = real_scalar(other) {
                return with_scalar(op, x, c).map(|s| Value::Sparse(Box::new(s)));
            }
            let f = plain_matrix(other)?;
            if op != BinOp::Mul || x.dims[1] != f.dims[0] || x.im.is_some() || !x.all_finite() {
                return None;
            }
            let (m, k, n) = (x.dims[0], f.dims[0], f.dims[1]);
            let mut out = vec![0.0; m.checked_mul(n).filter(|t| *t <= DENSE_LIMIT)?];
            for j in 0..n {
                for c in 0..k {
                    let y = f.re[c + j * k];
                    if y != 0.0 {
                        for p in x.colptr[c]..x.colptr[c + 1] {
                            out[x.rowidx[p] + j * m] += x.re[p] * y;
                        }
                    }
                }
            }
            Some(Value::matrix(vec![m, n], out))
        }
        (other, Value::Sparse(y)) => {
            if let Some(c) = real_scalar(other) {
                return with_scalar(mirrored(op)?, y, c).map(|s| Value::Sparse(Box::new(s)));
            }
            let f = plain_matrix(other)?;
            if op != BinOp::Mul || f.dims[1] != y.dims[0] || y.im.is_some() || !y.all_finite() {
                return None;
            }
            let (m, n) = (f.dims[0], y.dims[1]);
            let mut out = vec![0.0; m.checked_mul(n).filter(|t| *t <= DENSE_LIMIT)?];
            for j in 0..n {
                for p in y.colptr[j]..y.colptr[j + 1] {
                    let k = y.rowidx[p];
                    for i in 0..m {
                        out[i + j * m] += f.re[i + k * m] * y.re[p];
                    }
                }
            }
            Some(Value::matrix(vec![m, n], out))
        }
        _ => None,
    }
}

/// `a op b` when an operand is sparse. Zero-preserving cases run on the
/// nonzeros; the others go through the dense operators.
pub fn binary(op: BinOp, a: &Value, b: &Value) -> Res<Value> {
    if let Some(v) = native(op, a, b) {
        return Ok(v);
    }
    let r = ops::binary(op, &full(a)?, &full(b)?)?;
    Ok(if result_is_sparse(op, a, b) {
        sparsify(r)
    } else {
        r
    })
}

pub fn unary(op: UnOp, s: &Sparse) -> Res<Value> {
    Ok(match op {
        UnOp::Plus => Value::Sparse(Box::new(s.as_double())),
        UnOp::Neg => Value::Sparse(Box::new(s.map_nz(false, |x, y| (-x, -y)))),
        UnOp::Not => sparsify(ops::unary(UnOp::Not, &s.to_dense()?)?),
        UnOp::Transpose => Value::Sparse(Box::new(s.transposed(false))),
        UnOp::CTranspose => Value::Sparse(Box::new(s.transposed(true))),
    })
}

// ---------------------------------------------------------------------------
// Indexing
// ---------------------------------------------------------------------------

/// `S(subs...)`: always a sparse result.
pub fn index(s: &Sparse, subs: &[Sub]) -> Res<Value> {
    let [m, n] = s.dims;
    let wrap = |x: Sparse| Ok(Value::Sparse(Box::new(x)));
    match subs {
        [] => wrap(s.clone()),
        [Sub::Colon] => {
            let total = m
                .checked_mul(n)
                .ok_or_else(|| Error::new("MATLAB:nomem", "Out of memory."))?;
            wrap(s.reshaped(total, 1))
        }
        [one] => {
            let (pos, dims) = read_positions(&[m, n], std::slice::from_ref(one))?;
            if dims.len() > 2 {
                return Err(nd_error());
            }
            let om = dims[0];
            let mut e = Vec::new();
            for (t, &p) in pos.iter().enumerate() {
                if let Some(k) = s.find(p % m, p / m) {
                    e.push((t % om, t / om, s.re[k], s.entry_im(k)));
                }
            }
            wrap(Sparse::from_sorted(om, dims[1], s.logical, &e))
        }
        [rows, cols, rest @ ..] => {
            for (k, x) in rest.iter().enumerate() {
                if let Sub::List { idx, .. } = x
                    && let Some(i) = idx.iter().find(|i| **i != 0)
                {
                    return Err(out_of_bound(i + 1, 1, Some(k + 3)));
                }
            }
            if rest.iter().any(|x| x.len(1) != 1) {
                return Err(nd_error());
            }
            for (k, (x, ext)) in [(rows, m), (cols, n)].iter().enumerate() {
                if let Sub::List { idx, .. } = x
                    && let Some(i) = idx.iter().find(|i| **i >= *ext)
                {
                    return Err(out_of_bound(i + 1, *ext, Some(k + 1)));
                }
            }
            let (om, on) = (rows.len(m), cols.len(n));
            let mut e = Vec::new();
            for t in 0..on {
                let c = cols.at(t);
                match rows {
                    Sub::Colon => {
                        for k in s.colptr[c]..s.colptr[c + 1] {
                            e.push((s.rowidx[k], t, s.re[k], s.entry_im(k)));
                        }
                    }
                    Sub::List { idx, .. } => {
                        for (o, &r) in idx.iter().enumerate() {
                            if let Some(k) = s.find(r, c) {
                                e.push((o, t, s.re[k], s.entry_im(k)));
                            }
                        }
                    }
                }
            }
            wrap(Sparse::from_sorted(om, on, s.logical, &e))
        }
    }
}

/// A sparse index value as a subscript.
pub fn to_sub(s: &Sparse) -> Res<Sub> {
    if s.logical {
        let m = s.dims[0];
        let idx: Vec<usize> = s.entries().iter().map(|e| e.0 + e.1 * m).collect();
        let k = idx.len();
        let shape = if m == 1 { vec![1, k] } else { vec![k, 1] };
        return Ok(Sub::List { idx, shape });
    }
    crate::index::to_sub(&s.to_dense()?)
}

fn mismatch(have: usize, got: usize) -> Box<Error> {
    Error::new(
        "MATLAB:subsassignnumelmismatch",
        format!(
            "Unable to perform assignment because the left and right sides have a different number of elements ({} and {}).",
            have, got
        ),
    )
}

fn distinct(idx: &[usize]) -> bool {
    let mut v = idx.to_vec();
    v.sort_unstable();
    v.windows(2).all(|w| w[0] != w[1])
}

/// Keep the stored elements selected by `keep` and add the new ones.
fn replace(
    s: &Sparse,
    dims: [usize; 2],
    logical: bool,
    keep: impl Fn(usize, usize) -> bool,
    new: Vec<Entry>,
) -> Sparse {
    let mut e: Vec<Entry> = s.entries().into_iter().filter(|x| keep(x.0, x.1)).collect();
    e.extend(new);
    Sparse::from_triplets(dims[0], dims[1], logical, e)
}

/// Direct forms of `S(subs...) = rhs` on the stored elements; `None` when
/// the dense path must decide (growth by a linear index, repeated subscripts,
/// N-d subscripts).
fn assign_native(s: &Sparse, subs: &[Sub], rhs: &Num) -> Res<Option<Sparse>> {
    let [m, n] = s.dims;
    let rn = rhs.re.len();
    let value = |t: usize| {
        let k = if rn == 1 { 0 } else { t };
        (rhs.re[k], rhs.im_at(k))
    };
    match subs {
        [Sub::List { idx, .. }] => {
            let total = m.saturating_mul(n);
            if idx.iter().any(|i| *i >= total) || !distinct(idx) {
                return Ok(None);
            }
            if rn != 1 && rn != idx.len() {
                return Err(mismatch(idx.len(), rn));
            }
            let set: std::collections::HashSet<usize> = idx.iter().copied().collect();
            let new = idx
                .iter()
                .enumerate()
                .map(|(t, &p)| {
                    let (x, y) = value(t);
                    (p % m, p / m, x, y)
                })
                .collect();
            Ok(Some(replace(
                s,
                s.dims,
                s.logical,
                |r, c| !set.contains(&(r + c * m)),
                new,
            )))
        }
        [rows, cols] => {
            if (matches!(rows, Sub::Colon) && m == 0) || (matches!(cols, Sub::Colon) && n == 0) {
                return Ok(None);
            }
            for x in [rows, cols] {
                if let Sub::List { idx, .. } = x
                    && !distinct(idx)
                {
                    return Ok(None);
                }
            }
            let (om, on) = (rows.len(m), cols.len(n));
            let count = om
                .checked_mul(on)
                .ok_or_else(|| Error::new("MATLAB:nomem", "Out of memory."))?;
            if rn != 1 && rn != count {
                return Err(mismatch(count, rn));
            }
            let dims = [m.max(rows.max_plus_one()), n.max(cols.max_plus_one())];
            if dims[0] > MAX_GROWN_EXTENT || dims[1] > MAX_GROWN_EXTENT {
                return Err(too_big(dims[0], dims[1]));
            }
            let mut in_rows = vec![false; dims[0]];
            let mut in_cols = vec![false; dims[1]];
            for t in 0..om {
                in_rows[rows.at(t)] = true;
            }
            for t in 0..on {
                in_cols[cols.at(t)] = true;
            }
            let zero_fill = rn == 1 && rhs.re[0] == 0.0 && rhs.im_at(0) == 0.0;
            if !zero_fill && count > DENSE_LIMIT {
                return Err(too_big(om, on));
            }
            let mut new = Vec::new();
            if !zero_fill {
                new.reserve(count);
                for j in 0..on {
                    for i in 0..om {
                        let (x, y) = value(i + j * om);
                        new.push((rows.at(i), cols.at(j), x, y));
                    }
                }
            }
            Ok(Some(replace(
                s,
                dims,
                s.logical,
                |r, c| !(in_rows[r] && in_cols[c]),
                new,
            )))
        }
        _ => Ok(None),
    }
}

/// `target(subs...) = rhs` when the target or the right side is sparse. A
/// sparse target stays sparse (and logical stays logical), a full target
/// stays full, an undefined target becomes sparse.
pub fn assign(target: &mut Option<Value>, subs: &[Sub], rhs: Value) -> Res<()> {
    let plain_rhs = full(&rhs)?;
    match target.take() {
        Some(Value::Sparse(s)) => {
            let result = match plain_rhs.to_num() {
                Some(mut r) if !matches!(plain_rhs, Value::Cell(_) | Value::Struct(_)) => {
                    if s.logical {
                        r.re.iter_mut().for_each(|v| *v = (*v != 0.0) as u8 as f64);
                        r.im = None;
                    }
                    match assign_native(&s, subs, &r) {
                        Ok(Some(x)) => Ok(x),
                        Ok(None) => assign_dense(&s, subs, plain_rhs),
                        Err(e) => Err(e),
                    }
                }
                _ => Err(Error::new(
                    "MATLAB:invalidConversion",
                    format!(
                        "Conversion to {} from {} is not possible.",
                        if s.logical { "logical" } else { "double" },
                        rhs.class_name()
                    ),
                )),
            };
            match result {
                Ok(x) => {
                    *target = Some(Value::Sparse(Box::new(x)));
                    Ok(())
                }
                Err(e) => {
                    *target = Some(Value::Sparse(s));
                    Err(e)
                }
            }
        }
        None => {
            let mut t = None;
            crate::index::assign_index(&mut t, subs, plain_rhs)?;
            *target = t.map(sparsify);
            Ok(())
        }
        dense => {
            *target = dense;
            crate::index::assign_index(target, subs, plain_rhs)
        }
    }
}

fn assign_dense(s: &Sparse, subs: &[Sub], rhs: Value) -> Res<Sparse> {
    let rhs = if s.logical {
        crate::bool_array(rhs.dims(), ops::to_logical_data(&rhs)?)
    } else {
        rhs
    };
    let mut t = Some(s.to_dense()?);
    crate::index::assign_index(&mut t, subs, rhs)?;
    Sparse::from_value(&t.unwrap_or_default())
}

/// `S(subs...) = []`.
pub fn delete(s: &Sparse, subs: &[Sub]) -> Res<Value> {
    Ok(sparsify(crate::index::delete_index(s.to_dense()?, subs)?))
}

// ---------------------------------------------------------------------------
// Concatenation
// ---------------------------------------------------------------------------

fn hcat(parts: &[Sparse]) -> Sparse {
    let m = parts[0].dims[0];
    let n = parts.iter().map(|p| p.dims[1]).sum();
    let logical = parts.iter().all(|p| p.logical);
    let mut e = Vec::new();
    let mut base = 0;
    for p in parts {
        e.extend(
            p.entries()
                .into_iter()
                .map(|(r, c, x, y)| (r, c + base, x, y)),
        );
        base += p.dims[1];
    }
    Sparse::from_sorted(m, n, logical, &e)
}

/// Concatenation with a sparse operand gives a sparse matrix.
pub fn cat(values: Vec<Value>, d: usize) -> Res<Value> {
    if d >= 2 {
        return Err(nd_error());
    }
    let parts: Option<Vec<Sparse>> = values
        .iter()
        .filter(|v| !(v.is_empty() && v.dims_ref() == [0, 0]))
        .map(|v| Sparse::from_value(v).ok())
        .collect();
    if let Some(parts) = parts
        && !parts.is_empty()
    {
        let key = 1 - d;
        if parts.iter().all(|p| p.dims[key] == parts[0].dims[key]) {
            let out = if d == 1 {
                hcat(&parts)
            } else {
                let t: Vec<Sparse> = parts.iter().map(|p| p.transposed(false)).collect();
                hcat(&t).transposed(false)
            };
            return Ok(Value::Sparse(Box::new(out)));
        }
    }
    let dense = values.iter().map(full).collect::<Res<Vec<Value>>>()?;
    Ok(sparsify(crate::concat::cat(dense, d)?))
}

// ---------------------------------------------------------------------------
// Display
// ---------------------------------------------------------------------------

/// `(i,j) value` lines of a sparse matrix, in MATLAB's layout as far as it
/// is known here: no reference output of MATLAB exists in this repository.
pub fn text(s: &Sparse) -> String {
    let nz = s.nnz();
    if nz == 0 {
        return format!("   All zero sparse: {}\u{d7}{}\n", s.dims[0], s.dims[1]);
    }
    // Two copies of the first element keep the column layout for one nonzero.
    let num = Num {
        class: NumClass::Double,
        dims: vec![nz + 1, 1],
        re: s.re.iter().copied().chain([s.re[0]]).collect(),
        im: s.im.as_ref().map(|v| {
            let mut i = v.clone();
            i.push(v[0]);
            i
        }),
    };
    let body = crate::format::disp_text(&Value::Num(num));
    let lines: Vec<&str> = body.lines().filter(|l| !l.trim().is_empty()).collect();
    let values: Vec<String> = if lines.len() == nz + 1 {
        lines[..nz].iter().map(|l| l.trim().to_string()).collect()
    } else {
        // A common scale factor heads the dense text: print plain values.
        s.re.iter().map(|x| crate::format::short_g(*x)).collect()
    };
    let w = values.iter().map(|x| x.chars().count()).max().unwrap_or(0);
    let whole = s.im.is_none() && s.re.iter().all(|x| x.fract() == 0.0);
    let field = if whole { (w + 3).max(9) } else { w + 7 };
    let at: Vec<String> = s
        .entries()
        .iter()
        .map(|e| format!("({},{})", e.0 + 1, e.1 + 1))
        .collect();
    let aw = at.iter().map(|x| x.len()).max().unwrap_or(0);
    let mut out = String::new();
    for (a, v) in at.iter().zip(&values) {
        out.push_str(&format!("   {:>aw$}{:>field$}\n", a, v));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn m() -> Sparse {
        Sparse::from_triplets(
            3,
            2,
            false,
            vec![
                (2, 1, 4.0, 0.0),
                (0, 0, 1.0, 0.0),
                (0, 0, 2.0, 0.0),
                (1, 1, 0.0, 0.0),
            ],
        )
    }
    #[test]
    fn triplets_are_canonical() {
        let s = m();
        assert_eq!(s.colptr, vec![0, 1, 2]);
        assert_eq!(s.rowidx, vec![0, 2]);
        assert_eq!(s.re, vec![3.0, 4.0]);
    }
    #[test]
    fn duplicates_that_cancel_are_dropped() {
        let s = Sparse::from_triplets(1, 1, false, vec![(0, 0, 1.0, 0.0), (0, 0, -1.0, 0.0)]);
        assert_eq!(s.nnz(), 0);
    }
    #[test]
    fn dense_round_trip() {
        let s = m();
        let d = s.to_dense().unwrap();
        assert_eq!(d.to_f64_vec().unwrap(), vec![3.0, 0.0, 0.0, 0.0, 0.0, 4.0]);
        assert_eq!(Sparse::from_value(&d).unwrap(), s);
    }
    #[test]
    fn transpose_twice_is_identity() {
        let s = m();
        assert_eq!(s.transposed(false).transposed(false), s);
    }
    #[test]
    fn dense_form_is_refused_when_huge() {
        assert!(Sparse::zeros(1 << 20, 1 << 20, false).to_dense().is_err());
    }
    #[test]
    fn text_layout() {
        assert_eq!(text(&m()), "   (1,1)        3\n   (3,2)        4\n");
        assert_eq!(
            text(&Sparse::zeros(2, 3, false)),
            "   All zero sparse: 2\u{d7}3\n"
        );
    }
}
