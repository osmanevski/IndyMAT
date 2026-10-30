//! Operators: element-wise with implicit expansion, matrix products,
//! division via own small LU/QR (no LAPACK), transposes, ranges.

use crate::ast::{BinOp, UnOp};
use crate::{Array, Error, Num, NumClass, Res, Value, norm_dims, numel_of};

pub type C = (f64, f64);

pub fn c_mul(a: C, b: C) -> C {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}
pub fn c_div(a: C, b: C) -> C {
    if b.1 == 0.0 {
        return (a.0 / b.0, a.1 / b.0);
    }
    // Smith's algorithm.
    if b.0.abs() >= b.1.abs() {
        let r = b.1 / b.0;
        let d = b.0 + b.1 * r;
        ((a.0 + a.1 * r) / d, (a.1 - a.0 * r) / d)
    } else {
        let r = b.0 / b.1;
        let d = b.0 * r + b.1;
        ((a.0 * r + a.1) / d, (a.1 * r - a.0) / d)
    }
}
pub fn c_abs(a: C) -> f64 {
    a.0.hypot(a.1)
}
pub fn c_exp(a: C) -> C {
    let e = a.0.exp();
    if a.1 == 0.0 {
        return (e, 0.0);
    }
    (e * a.1.cos(), e * a.1.sin())
}
pub fn c_log(a: C) -> C {
    (c_abs(a).ln(), a.1.atan2(a.0))
}
pub fn c_sqrt(a: C) -> C {
    if a.1 == 0.0 {
        return if a.0 >= 0.0 {
            (a.0.sqrt(), 0.0)
        } else {
            (0.0, (-a.0).sqrt())
        };
    }
    let r = c_abs(a);
    let re = ((r + a.0) / 2.0).sqrt();
    let im = ((r - a.0) / 2.0).sqrt().copysign(a.1);
    (re, im)
}
pub fn c_pow(a: C, b: C) -> C {
    if b.1 == 0.0 && a.1 == 0.0 && (a.0 >= 0.0 || b.0.fract() == 0.0) {
        return (a.0.powf(b.0), 0.0);
    }
    if a.0 == 0.0 && a.1 == 0.0 {
        return if b.0 > 0.0 {
            (0.0, 0.0)
        } else {
            (f64::INFINITY, 0.0)
        };
    }
    if b.1 == 0.0 && b.0.fract() == 0.0 && b.0.abs() <= 64.0 {
        // Exact repeated multiplication for small integer powers.
        let mut r = (1.0, 0.0);
        let mut base = a;
        let mut n = b.0.abs() as u64;
        while n > 0 {
            if n & 1 == 1 {
                r = c_mul(r, base);
            }
            base = c_mul(base, base);
            n >>= 1;
        }
        return if b.0 < 0.0 { c_div((1.0, 0.0), r) } else { r };
    }
    c_exp(c_mul(b, c_log(a)))
}

fn type_error(op: &str, a: &Value, b: &Value) -> Box<Error> {
    Error::new(
        "MATLAB:UndefinedFunction",
        format!(
            "Operator '{}' is not supported for operands of type '{}' and '{}'.",
            op,
            a.class_name(),
            b.class_name()
        ),
    )
}

/// Result dims of implicit expansion.
pub fn bcast_dims(a: &[usize], b: &[usize]) -> Option<Vec<usize>> {
    let n = a.len().max(b.len());
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let x = *a.get(k).unwrap_or(&1);
        let y = *b.get(k).unwrap_or(&1);
        if x == y {
            out.push(x);
        } else if x == 1 {
            out.push(y);
        } else if y == 1 {
            out.push(x);
        } else {
            return None;
        }
    }
    Some(norm_dims(out))
}

fn dims_error(a: &[usize], b: &[usize]) -> Box<Error> {
    Error::new(
        "MATLAB:sizeDimensionsMustMatch",
        format!(
            "Arrays have incompatible sizes for this operation ({} vs {}).",
            dims_text(a),
            dims_text(b)
        ),
    )
}

pub fn dims_text(d: &[usize]) -> String {
    d.iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join("x")
}

/// Zip two arrays under implicit expansion.
pub fn zip_bcast<A: Copy, B: Copy, R>(
    ad: &[usize],
    a: &[A],
    bd: &[usize],
    b: &[B],
    mut f: impl FnMut(A, B) -> R,
) -> Res<(Vec<usize>, Vec<R>)> {
    if ad == bd {
        return Ok((
            ad.to_vec(),
            a.iter().zip(b).map(|(x, y)| f(*x, *y)).collect(),
        ));
    }
    if b.len() == 1 && numel_of(bd) == 1 {
        let y = b[0];
        return Ok((ad.to_vec(), a.iter().map(|x| f(*x, y)).collect()));
    }
    if a.len() == 1 && numel_of(ad) == 1 {
        let x = a[0];
        return Ok((bd.to_vec(), b.iter().map(|y| f(x, *y)).collect()));
    }
    let od = bcast_dims(ad, bd).ok_or_else(|| dims_error(ad, bd))?;
    let n = od.len();
    let total = numel_of(&od);
    let mut out = Vec::with_capacity(total);
    if total == 0 {
        return Ok((od, out));
    }
    let dim = |d: &[usize], k: usize| *d.get(k).unwrap_or(&1);
    let mut sa = vec![0usize; n];
    let mut sb = vec![0usize; n];
    let (mut pa, mut pb) = (1usize, 1usize);
    for k in 0..n {
        sa[k] = if dim(ad, k) == 1 { 0 } else { pa };
        sb[k] = if dim(bd, k) == 1 { 0 } else { pb };
        pa *= dim(ad, k);
        pb *= dim(bd, k);
    }
    let mut ctr = vec![0usize; n];
    let (mut ia, mut ib) = (0usize, 0usize);
    for _ in 0..total {
        out.push(f(a[ia], b[ib]));
        for k in 0..n {
            ctr[k] += 1;
            ia += sa[k];
            ib += sb[k];
            if ctr[k] < od[k] {
                break;
            }
            ia -= sa[k] * od[k];
            ib -= sb[k] * od[k];
            ctr[k] = 0;
        }
    }
    Ok((od, out))
}

/// Numeric operands (logical and char convert to double).
fn numeric_pair(op: &str, a: &Value, b: &Value) -> Res<(Num, Num, NumClass)> {
    let (Some(x), Some(y)) = (a.to_num(), b.to_num()) else {
        return Err(type_error(op, a, b));
    };
    let class = result_class(x.class, y.class)?;
    Ok((x, y, class))
}

pub fn result_class(a: NumClass, b: NumClass) -> Res<NumClass> {
    Ok(match (a, b) {
        (NumClass::Double, c) | (c, NumClass::Double) => c,
        (x, y) if x == y => x,
        (NumClass::Single, c) | (c, NumClass::Single) if c.is_int() => c,
        (x, y) => {
            return Err(Error::new(
                "MATLAB:mixedClasses",
                format!(
                    "Integers can only be combined with integers of the same class, or scalar doubles ({} and {}).",
                    x.name(),
                    y.name()
                ),
            ));
        }
    })
}

fn complex_parts(n: &Num) -> Vec<C> {
    match &n.im {
        Some(im) => n.re.iter().zip(im).map(|(r, i)| (*r, *i)).collect(),
        None => n.re.iter().map(|r| (*r, 0.0)).collect(),
    }
}

fn from_complex(dims: Vec<usize>, v: Vec<C>, class: NumClass) -> Value {
    let re = v.iter().map(|c| c.0).collect();
    let im = v.iter().map(|c| c.1).collect();
    Value::Num(Num::complex(dims, re, im).with_class(class))
}

fn real_result(dims: Vec<usize>, re: Vec<f64>, class: NumClass) -> Value {
    Value::Num(Num::real(dims, re).with_class(class))
}

/// Element-wise arithmetic.
fn arith(op: BinOp, a: &Value, b: &Value) -> Res<Value> {
    let (x, y, class) = numeric_pair(op.text(), a, b)?;
    let complex = x.im.is_some() || y.im.is_some();
    if !complex {
        let f: fn(f64, f64) -> f64 = match op {
            BinOp::Add => |p, q| p + q,
            BinOp::Sub => |p, q| p - q,
            BinOp::Mul | BinOp::EMul => |p, q| p * q,
            BinOp::Div | BinOp::EDiv => |p, q| p / q,
            BinOp::LeftDiv | BinOp::ELeftDiv => |p, q| q / p,
            BinOp::Pow | BinOp::EPow => {
                // Negative base with fractional exponent goes complex.
                let needs_c = {
                    let ax = x.re.iter().any(|v| *v < 0.0);
                    let fy = y.re.iter().any(|v| v.fract() != 0.0);
                    ax && fy
                };
                if needs_c && !class.is_int() {
                    let (d, v) = zip_bcast(
                        &x.dims,
                        &complex_parts(&x),
                        &y.dims,
                        &complex_parts(&y),
                        c_pow,
                    )?;
                    return Ok(from_complex(d, v, class));
                }
                |p: f64, q: f64| p.powf(q)
            }
            _ => unreachable!(),
        };
        let (d, v) = zip_bcast(&x.dims, &x.re, &y.dims, &y.re, f)?;
        return Ok(real_result(d, v, class));
    }
    let f: fn(C, C) -> C = match op {
        BinOp::Add => |p, q| (p.0 + q.0, p.1 + q.1),
        BinOp::Sub => |p, q| (p.0 - q.0, p.1 - q.1),
        BinOp::Mul | BinOp::EMul => c_mul,
        BinOp::Div | BinOp::EDiv => c_div,
        BinOp::LeftDiv | BinOp::ELeftDiv => |p, q| c_div(q, p),
        BinOp::Pow | BinOp::EPow => c_pow,
        _ => unreachable!(),
    };
    let (d, v) = zip_bcast(&x.dims, &complex_parts(&x), &y.dims, &complex_parts(&y), f)?;
    Ok(from_complex(d, v, class))
}

fn compare(op: BinOp, a: &Value, b: &Value) -> Res<Value> {
    let (Some(x), Some(y)) = (a.to_num(), b.to_num()) else {
        return Err(type_error(op.text(), a, b));
    };
    if matches!(op, BinOp::Eq | BinOp::Ne) && (x.im.is_some() || y.im.is_some()) {
        let eq = op == BinOp::Eq;
        let (d, v) = zip_bcast(
            &x.dims,
            &complex_parts(&x),
            &y.dims,
            &complex_parts(&y),
            |p, q| (p == q) == eq,
        )?;
        return Ok(Value::Bool(Array::new(d, v)));
    }
    let f: fn(f64, f64) -> bool = match op {
        BinOp::Eq => |p, q| p == q,
        BinOp::Ne => |p, q| p != q,
        BinOp::Lt => |p, q| p < q,
        BinOp::Le => |p, q| p <= q,
        BinOp::Gt => |p, q| p > q,
        BinOp::Ge => |p, q| p >= q,
        _ => unreachable!(),
    };
    let (d, v) = zip_bcast(&x.dims, &x.re, &y.dims, &y.re, f)?;
    Ok(Value::Bool(Array::new(d, v)))
}

/// Logical value of each element (NaN is an error).
pub fn to_logical_data(v: &Value) -> Res<Vec<bool>> {
    match v {
        Value::Bool(a) => Ok(a.data.clone()),
        Value::Num(n) => {
            let mut out = Vec::with_capacity(n.re.len());
            for i in 0..n.re.len() {
                if n.re[i].is_nan() {
                    return Err(Error::new(
                        "MATLAB:nologicalnan",
                        "NaN's cannot be converted to logicals.",
                    ));
                }
                out.push(n.re[i] != 0.0 || n.im_at(i) != 0.0);
            }
            Ok(out)
        }
        Value::Char(a) => Ok(a.data.iter().map(|c| *c != '\0').collect()),
        other => Err(Error::new(
            "MATLAB:invalidConversion",
            format!(
                "Conversion to logical from {} is not possible.",
                other.class_name()
            ),
        )),
    }
}

fn logical_op(op: BinOp, a: &Value, b: &Value) -> Res<Value> {
    let x = to_logical_data(a)?;
    let y = to_logical_data(b)?;
    let f: fn(bool, bool) -> bool = if op == BinOp::And {
        |p, q| p && q
    } else {
        |p, q| p || q
    };
    let (d, v) = zip_bcast(a.dims_ref(), &x, b.dims_ref(), &y, f)?;
    Ok(Value::Bool(Array::new(d, v)))
}

fn string_binary(op: BinOp, a: &Value, b: &Value) -> Res<Value> {
    let x = crate::to_string_array(a);
    let y = crate::to_string_array(b);
    match op {
        BinOp::Add => {
            let (d, v) = zip_bcast(
                &x.dims,
                &idx(x.data.len()),
                &y.dims,
                &idx(y.data.len()),
                |i, j| match (&x.data[i], &y.data[j]) {
                    (Some(p), Some(q)) => Some(format!("{}{}", p, q)),
                    _ => None,
                },
            )?;
            Ok(Value::Str(Array::new(d, v)))
        }
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            let (d, v) = zip_bcast(
                &x.dims,
                &idx(x.data.len()),
                &y.dims,
                &idx(y.data.len()),
                |i, j| match (&x.data[i], &y.data[j]) {
                    (Some(p), Some(q)) => match op {
                        BinOp::Eq => p == q,
                        BinOp::Ne => p != q,
                        BinOp::Lt => p < q,
                        BinOp::Le => p <= q,
                        BinOp::Gt => p > q,
                        _ => p >= q,
                    },
                    _ => op == BinOp::Ne,
                },
            )?;
            Ok(Value::Bool(Array::new(d, v)))
        }
        _ => Err(type_error(op.text(), a, b)),
    }
}

fn idx(n: usize) -> Vec<usize> {
    (0..n).collect()
}

/// Any binary operator except `&&`/`||` (handled by the interpreter).
pub fn binary(op: BinOp, a: &Value, b: &Value) -> Res<Value> {
    if matches!(a, Value::Str(_)) || matches!(b, Value::Str(_)) {
        return string_binary(op, a, b);
    }
    // Scalar double fast path.
    if let (Value::Num(x), Value::Num(y)) = (a, b)
        && x.re.len() == 1
        && y.re.len() == 1
        && x.im.is_none()
        && y.im.is_none()
        && x.class == NumClass::Double
        && y.class == NumClass::Double
        && x.dims.len() == 2
        && y.dims.len() == 2
    {
        let (p, q) = (x.re[0], y.re[0]);
        let r = match op {
            BinOp::Add => Some(p + q),
            BinOp::Sub => Some(p - q),
            BinOp::Mul | BinOp::EMul => Some(p * q),
            BinOp::Div | BinOp::EDiv => Some(p / q),
            BinOp::LeftDiv | BinOp::ELeftDiv => Some(q / p),
            BinOp::Pow | BinOp::EPow if p >= 0.0 || q.fract() == 0.0 => Some(p.powf(q)),
            _ => None,
        };
        if let Some(r) = r {
            return Ok(Value::num(r));
        }
        let c = match op {
            BinOp::Eq => Some(p == q),
            BinOp::Ne => Some(p != q),
            BinOp::Lt => Some(p < q),
            BinOp::Le => Some(p <= q),
            BinOp::Gt => Some(p > q),
            BinOp::Ge => Some(p >= q),
            _ => None,
        };
        if let Some(c) = c {
            return Ok(Value::bool(c));
        }
    }
    if a.is_sparse() || b.is_sparse() {
        return crate::sparse::binary(op, a, b);
    }
    match op {
        BinOp::Add | BinOp::Sub | BinOp::EMul | BinOp::EDiv | BinOp::ELeftDiv | BinOp::EPow => {
            arith(op, a, b)
        }
        BinOp::Mul => {
            if a.is_scalar() || b.is_scalar() {
                arith(op, a, b)
            } else {
                matmul(a, b)
            }
        }
        BinOp::Div => {
            if b.is_scalar() {
                arith(op, a, b)
            } else {
                mrdivide(a, b)
            }
        }
        BinOp::LeftDiv => {
            if a.is_scalar() {
                arith(op, a, b)
            } else {
                mldivide(a, b)
            }
        }
        BinOp::Pow => mpower(a, b),
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => compare(op, a, b),
        BinOp::And | BinOp::Or => logical_op(op, a, b),
        BinOp::AndAnd | BinOp::OrOr => Err(Error::msg("internal: short-circuit op in binary()")),
    }
}

pub fn unary(op: UnOp, a: &Value) -> Res<Value> {
    if let Value::Sparse(s) = a {
        return crate::sparse::unary(op, s);
    }
    match op {
        UnOp::Plus => match a {
            Value::Num(_) => Ok(a.clone()),
            Value::Bool(_) | Value::Char(_) => Ok(Value::Num(a.to_num().unwrap())),
            _ => Err(Error::msg(format!(
                "Unary operator '+' is not supported for operand of type '{}'.",
                a.class_name()
            ))),
        },
        UnOp::Neg => match a {
            Value::Num(n) => {
                let mut n = n.clone();
                for v in n.re.iter_mut() {
                    *v = -*v;
                }
                if let Some(im) = &mut n.im {
                    for v in im.iter_mut() {
                        *v = -*v;
                    }
                }
                Ok(Value::Num(n.with_class_keep()))
            }
            Value::Bool(_) | Value::Char(_) => unary(UnOp::Neg, &Value::Num(a.to_num().unwrap())),
            _ => Err(Error::msg(format!(
                "Unary operator '-' is not supported for operand of type '{}'.",
                a.class_name()
            ))),
        },
        UnOp::Not => {
            let d = to_logical_data(a)?;
            Ok(Value::Bool(Array::new(
                a.dims(),
                d.into_iter().map(|b| !b).collect(),
            )))
        }
        UnOp::Transpose => transpose(a, false),
        UnOp::CTranspose => transpose(a, true),
    }
}

impl Num {
    fn with_class_keep(self) -> Num {
        let c = self.class;
        self.with_class(c)
    }
}

fn transpose_data<T: Clone>(d: &[usize], data: &[T]) -> Vec<T> {
    let (r, c) = (d[0], d[1]);
    let mut out = Vec::with_capacity(data.len());
    for i in 0..r {
        for j in 0..c {
            out.push(data[i + j * r].clone());
        }
    }
    out
}

pub fn transpose(a: &Value, conj: bool) -> Res<Value> {
    if let Value::Sparse(s) = a {
        return Ok(Value::Sparse(Box::new(s.transposed(conj))));
    }
    let d = a.dims();
    if d.len() > 2 {
        return Err(Error::new(
            "MATLAB:transpose:ndArray",
            "Transpose on ND array is not defined. Use PERMUTE instead.",
        ));
    }
    let nd = vec![d[1], d[0]];
    Ok(match a {
        Value::Num(n) => Value::Num(Num {
            class: n.class,
            dims: nd,
            re: transpose_data(&d, &n.re),
            im: n.im.as_ref().map(|im| {
                let mut t = transpose_data(&d, im);
                if conj {
                    for v in t.iter_mut() {
                        *v = -*v;
                    }
                }
                t
            }),
        }),
        Value::Bool(x) => Value::Bool(Array::new(nd, transpose_data(&d, &x.data))),
        Value::Char(x) => Value::Char(Array::new(nd, transpose_data(&d, &x.data))),
        Value::Cell(x) => Value::Cell(Array::new(nd, transpose_data(&d, &x.data))),
        Value::Str(x) => Value::Str(Array::new(nd, transpose_data(&d, &x.data))),
        Value::Struct(s) => {
            let mut s = s.clone();
            s.elems = transpose_data(&d, &s.elems);
            s.dims = nd;
            Value::Struct(s)
        }
        other => other.clone(),
    })
}

// ---------------------------------------------------------------------------
// Matrix algebra
// ---------------------------------------------------------------------------

fn as_matrix(v: &Value, what: &str) -> Res<Num> {
    let n = v.to_num().ok_or_else(|| {
        Error::msg(format!(
            "Operator '{}' is not supported for operands of type '{}'.",
            what,
            v.class_name()
        ))
    })?;
    if n.dims.len() > 2 {
        return Err(Error::msg(format!(
            "Operator '{}' is only defined for 2-D arrays.",
            what
        )));
    }
    Ok(n)
}

pub fn matmul(a: &Value, b: &Value) -> Res<Value> {
    let x = as_matrix(a, "*")?;
    let y = as_matrix(b, "*")?;
    let class = result_class(x.class, y.class)?;
    let (m, k) = (x.dims[0], x.dims[1]);
    let (k2, n) = (y.dims[0], y.dims[1]);
    if k != k2 {
        return Err(Error::new(
            "MATLAB:innerdim",
            "Incorrect dimensions for matrix multiplication. Check that the number of columns in the first matrix matches the number of rows in the second matrix.",
        ));
    }
    let mm = |xr: &[f64], yr: &[f64]| {
        let mut out = vec![0.0; m * n];
        for j in 0..n {
            for p in 0..k {
                let yv = yr[p + j * k];
                if yv == 0.0 {
                    continue;
                }
                let col = &xr[p * m..p * m + m];
                let o = &mut out[j * m..j * m + m];
                for i in 0..m {
                    o[i] += col[i] * yv;
                }
            }
        }
        out
    };
    // NaN/Inf in a skipped zero column must still propagate: redo naively.
    let has_nonfinite = x.re.iter().any(|v| !v.is_finite());
    let mm_safe = |xr: &[f64], yr: &[f64]| {
        if !has_nonfinite {
            return mm(xr, yr);
        }
        let mut out = vec![0.0; m * n];
        for j in 0..n {
            for i in 0..m {
                let mut s = 0.0;
                for p in 0..k {
                    s += xr[i + p * m] * yr[p + j * k];
                }
                out[i + j * m] = s;
            }
        }
        out
    };
    if x.im.is_none() && y.im.is_none() {
        return Ok(real_result(vec![m, n], mm_safe(&x.re, &y.re), class));
    }
    let zx = vec![0.0; x.re.len()];
    let zy = vec![0.0; y.re.len()];
    let xi = x.im.as_deref().unwrap_or(&zx);
    let yi = y.im.as_deref().unwrap_or(&zy);
    let rr = mm_safe(&x.re, &y.re);
    let ii = mm_safe(xi, yi);
    let ri = mm_safe(&x.re, yi);
    let ir = mm_safe(xi, &y.re);
    let re: Vec<f64> = rr.iter().zip(&ii).map(|(a, b)| a - b).collect();
    let im: Vec<f64> = ri.iter().zip(&ir).map(|(a, b)| a + b).collect();
    Ok(Value::Num(
        Num::complex(vec![m, n], re, im).with_class(class),
    ))
}

/// Solve `A X = B` for real A (square: LU with partial pivoting;
/// otherwise least squares by Householder QR). Returns (X, singular).
pub fn solve(a: &Num, b: &Num) -> Res<(Num, bool)> {
    let (m, n) = (a.dims[0], a.dims[1]);
    if b.dims[0] != m {
        return Err(Error::new(
            "MATLAB:dimagree",
            "Matrix dimensions must agree.",
        ));
    }
    if a.im.is_some() || b.im.is_some() {
        use crate::cmat::{CM, solve_minnorm, solve_square};
        let (ca, cb) = (CM::from_num(a), CM::from_num(b));
        if m == n {
            let (x, sing) = solve_square(&ca, &cb);
            return Ok((x.to_num(), sing));
        }
        return Ok((solve_minnorm(&ca, &cb, None).to_num(), false));
    }
    let nrhs = b.dims[1];
    if m == n {
        let mut lu = a.re.clone();
        let mut piv: Vec<usize> = (0..n).collect();
        let mut singular = false;
        for col in 0..n {
            let mut best = col;
            for r in col + 1..n {
                if lu[r + col * n].abs() > lu[best + col * n].abs() {
                    best = r;
                }
            }
            if lu[best + col * n] == 0.0 {
                singular = true;
                continue;
            }
            if best != col {
                for c in 0..n {
                    lu.swap(col + c * n, best + c * n);
                }
                piv.swap(col, best);
            }
            let p = lu[col + col * n];
            for r in col + 1..n {
                let f = lu[r + col * n] / p;
                lu[r + col * n] = f;
                if f != 0.0 {
                    for c in col + 1..n {
                        lu[r + c * n] -= f * lu[col + c * n];
                    }
                }
            }
        }
        // Reciprocal condition estimate (cheap): ratio of pivots.
        let diag: Vec<f64> = (0..n).map(|i| lu[i + i * n].abs()).collect();
        let maxd = diag.iter().cloned().fold(0.0, f64::max);
        let mind = diag.iter().cloned().fold(f64::INFINITY, f64::min);
        if n > 0 && (maxd == 0.0 || mind / maxd < f64::EPSILON) {
            singular = true;
        }
        let mut x = vec![0.0; n * nrhs];
        for j in 0..nrhs {
            let mut y: Vec<f64> = (0..n).map(|i| b.re[piv[i] + j * m]).collect();
            for i in 0..n {
                for k in 0..i {
                    y[i] -= lu[i + k * n] * y[k];
                }
            }
            for i in (0..n).rev() {
                for k in i + 1..n {
                    y[i] -= lu[i + k * n] * y[k];
                }
                y[i] /= lu[i + i * n];
            }
            x[j * n..j * n + n].copy_from_slice(&y);
        }
        return Ok((Num::real(vec![n, nrhs], x), singular));
    }
    // Least squares / minimum-effort solution via Householder QR on A.
    let mut q = a.re.clone();
    let mut rhs = b.re.clone();
    let steps = m.min(n);
    let mut rank_def = false;
    for k in 0..steps {
        let mut norm = 0.0;
        for i in k..m {
            norm += q[i + k * m] * q[i + k * m];
        }
        let norm = norm.sqrt();
        if norm == 0.0 {
            rank_def = true;
            continue;
        }
        let alpha = if q[k + k * m] > 0.0 { -norm } else { norm };
        let mut v: Vec<f64> = (k..m).map(|i| q[i + k * m]).collect();
        v[0] -= alpha;
        let vn: f64 = v.iter().map(|x| x * x).sum();
        if vn == 0.0 {
            continue;
        }
        for c in k..n {
            let dot: f64 = (k..m).map(|i| v[i - k] * q[i + c * m]).sum();
            let f = 2.0 * dot / vn;
            for i in k..m {
                q[i + c * m] -= f * v[i - k];
            }
        }
        for c in 0..nrhs {
            let dot: f64 = (k..m).map(|i| v[i - k] * rhs[i + c * m]).sum();
            let f = 2.0 * dot / vn;
            for i in k..m {
                rhs[i + c * m] -= f * v[i - k];
            }
        }
    }
    let mut x = vec![0.0; n * nrhs];
    for c in 0..nrhs {
        for i in (0..steps).rev() {
            let mut s = rhs[i + c * m];
            for k in i + 1..steps {
                s -= q[i + k * m] * x[k + c * n];
            }
            let d = q[i + i * m];
            x[i + c * n] = if d.abs() < 1e-14 { 0.0 } else { s / d };
        }
    }
    Ok((Num::real(vec![n, nrhs], x), rank_def))
}

pub fn mldivide(a: &Value, b: &Value) -> Res<Value> {
    let x = as_matrix(a, "\\")?;
    let y = as_matrix(b, "\\")?;
    let (r, _) = solve(&x, &y)?;
    Ok(Value::Num(r))
}

pub fn mrdivide(a: &Value, b: &Value) -> Res<Value> {
    // a / b = (b' \ a')'
    let bt = transpose(b, false)?;
    let at = transpose(a, false)?;
    let r = mldivide(&bt, &at)?;
    transpose(&r, false)
}

pub fn identity(n: usize) -> Num {
    let mut re = vec![0.0; n * n];
    for i in 0..n {
        re[i + i * n] = 1.0;
    }
    Num::real(vec![n, n], re)
}

pub fn inverse(a: &Num) -> Res<(Num, bool)> {
    let n = a.dims[0];
    if a.dims.len() != 2 || a.dims[1] != n {
        return Err(Error::new("MATLAB:square", "Matrix must be square."));
    }
    let (x, sing) = solve(a, &identity(n))?;
    if sing {
        return Ok((Num::real(vec![n, n], vec![f64::INFINITY; n * n]), true));
    }
    Ok((x, false))
}

fn mpower(a: &Value, b: &Value) -> Res<Value> {
    if a.is_scalar() && b.is_scalar() {
        return arith(BinOp::EPow, a, b);
    }
    if b.is_scalar() {
        let x = as_matrix(a, "^")?;
        if x.dims[0] != x.dims[1] {
            return Err(Error::new(
                "MATLAB:square",
                "Incorrect dimensions for raising a matrix to a power. Check that the matrix is square and the power is a scalar.",
            ));
        }
        let p = b.scalar_f64().unwrap_or(f64::NAN);
        if p.fract() == 0.0 {
            let mut base = Value::Num(x.clone());
            if p < 0.0 {
                base = Value::Num(inverse(&x)?.0);
            }
            let mut e = p.abs() as u64;
            let mut result = Value::Num(identity(x.dims[0]));
            while e > 0 {
                if e & 1 == 1 {
                    result = matmul(&result, &base)?;
                }
                e >>= 1;
                if e > 0 {
                    base = matmul(&base, &base)?;
                }
            }
            return Ok(result);
        }
        return Err(Error::msg(
            "Non-integer matrix powers are not supported yet (phase 2 engine).",
        ));
    }
    Err(Error::msg(
        "Scalar or matrix raised to a matrix power is not supported yet (phase 2 engine).",
    ))
}

/// `start:step:stop` as a row of doubles.
pub fn range(start: f64, step: f64, stop: f64) -> Res<Vec<f64>> {
    if start.is_nan() || step.is_nan() || stop.is_nan() {
        return Ok(vec![f64::NAN]);
    }
    if step == 0.0 || (step > 0.0 && start > stop) || (step < 0.0 && start < stop) {
        return Ok(vec![]);
    }
    let q = (stop - start) / step;
    if !q.is_finite() {
        return Err(Error::new(
            "MATLAB:pmaxsize",
            "Maximum variable size allowed by the program is exceeded.",
        ));
    }
    let tol = 3.0 * f64::EPSILON * q.abs().max(1.0);
    let n = (q + tol).floor() as usize + 1;
    if n > 2_000_000_000 {
        return Err(Error::new(
            "MATLAB:pmaxsize",
            "Maximum variable size allowed by the program is exceeded.",
        ));
    }
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        out.push(start + (k as f64) * step);
    }
    // Snap the last element onto `stop` when within rounding.
    if let Some(last) = out.last_mut()
        && ((*last - stop).abs() <= tol * step.abs())
    {
        *last = stop;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn broadcast_add() {
        let a = Value::col(vec![1.0, 2.0, 3.0]);
        let b = Value::row(vec![10.0, 20.0]);
        let r = binary(BinOp::Add, &a, &b).unwrap();
        assert_eq!(r.dims(), vec![3, 2]);
        assert_eq!(
            r.to_f64_vec().unwrap(),
            vec![11.0, 12.0, 13.0, 21.0, 22.0, 23.0]
        );
    }
    #[test]
    fn solve_square() {
        let a = Value::matrix(vec![2, 2], vec![2.0, 1.0, 1.0, 3.0]);
        let b = Value::col(vec![3.0, 5.0]);
        let x = mldivide(&a, &b).unwrap().to_f64_vec().unwrap();
        assert!((x[0] - 0.8).abs() < 1e-12 && (x[1] - 1.4).abs() < 1e-12);
    }
    #[test]
    fn range_count() {
        assert_eq!(range(0.0, 0.1, 1.0).unwrap().len(), 11);
        assert_eq!(range(1.0, 1.0, 0.0).unwrap().len(), 0);
    }
    #[test]
    fn negative_fractional_power_is_complex() {
        let r = binary(BinOp::Pow, &Value::num(-8.0), &Value::num(1.0 / 3.0)).unwrap();
        assert!(matches!(r, Value::Num(ref n) if n.im.is_some()));
    }
}
