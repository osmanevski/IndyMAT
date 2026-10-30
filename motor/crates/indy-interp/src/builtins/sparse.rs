//! Sparse matrix builtins, and the rule for every other builtin that receives
//! a sparse argument ([`call`]): each one either understands sparse values,
//! only looks at class or size, runs on the nonzeros, or computes on the full
//! form (returning a sparse first output where MATLAB does).
//!
//! `nzmax` is not stored: `nzmax(S)` reports the stored elements, and `spalloc`
//! and the `nzmax` argument of `sparse` only validate. `sprand`/`sprandn` draw
//! from the interpreter's `rng` stream, so they repeat for a seed but differ
//! from MATLAB's numbers; the `rc` form is refused.

use indy_value::sparse::{self, Entry, Sparse};
use indy_value::{Num, NumClass, Value};

use super::{arg_f64, arg_usize, err, need, one, reg};
use crate::{BuiltinFn, Interp, R};

/// Largest row or column count accepted: the column table is allocated.
const MAX_EXTENT: usize = 1 << 31;

pub fn register(it: &mut Interp) {
    reg(it, "sparse", sparse_b);
    reg(it, "issparse", |_, a, _| {
        need(&a, 1, "issparse")?;
        one(Value::bool(a[0].is_sparse()))
    });
    reg(it, "full", |_, a, _| {
        need(&a, 1, "full")?;
        one(sparse::full(&a[0])?)
    });
    reg(it, "iscomplex", |_, a, _| {
        need(&a, 1, "iscomplex")?;
        one(Value::bool(match &a[0] {
            Value::Num(n) => n.im.is_some(),
            Value::Sparse(s) => s.im.is_some(),
            _ => false,
        }))
    });
    reg(it, "nonzeros", nonzeros);
    reg(it, "nzmax", |_, a, _| {
        need(&a, 1, "nzmax")?;
        one(Value::num(match &a[0] {
            Value::Sparse(s) => s.nnz() as f64,
            v => v.numel() as f64,
        }))
    });
    reg(it, "spones", |_, a, _| {
        need(&a, 1, "spones")?;
        let s = to_sparse(&a[0])?;
        one(wrap(s.map_nz(false, |_, _| (1.0, 0.0))))
    });
    reg(it, "speye", speye);
    reg(it, "spalloc", spalloc);
    reg(it, "spdiags", spdiags);
    reg(it, "sprand", |it, a, _| random(it, a, "sprand", false));
    reg(it, "sprandn", |it, a, _| random(it, a, "sprandn", true));
    reg(it, "spfun", spfun);
}

fn wrap(s: Sparse) -> Value {
    Value::Sparse(Box::new(s))
}

fn to_sparse(v: &Value) -> R<Sparse> {
    Sparse::from_value(v)
}

fn extent(v: &Value, fname: &str) -> R<usize> {
    check_extent(arg_usize(v, fname)?)
}

fn check_extent(n: usize) -> R<usize> {
    if n > MAX_EXTENT {
        return Err(err(
            "MATLAB:array:SizeLimitExceeded",
            "Requested sparse matrix size exceeds the maximum array size.",
        ));
    }
    Ok(n)
}

// ---------------------------------------------------------------------------
// Dispatch of builtins receiving a sparse argument
// ---------------------------------------------------------------------------

/// Handle sparse values themselves, or only forward or inspect them.
const AWARE: &[&str] = &[
    "sparse",
    "issparse",
    "iscomplex",
    "full",
    "nonzeros",
    "nzmax",
    "spones",
    "spfun",
    "spdiags",
    "sprand",
    "sprandn",
    // size and class
    "size",
    "numel",
    "ndims",
    "length",
    "rows",
    "columns",
    "isempty",
    "isscalar",
    "isvector",
    "ismatrix",
    "isrow",
    "iscolumn",
    "issquare",
    "class",
    // containers and calls
    "disp",
    "display",
    "feval",
    "builtin",
    "deal",
    "assignin",
    "parse",
    "containers.Map",
    "struct",
    "cellfun",
    "arrayfun",
    "setfield",
    "getfield",
    "isfield",
    "isequal",
    "isequaln",
    "horzcat",
    "vertcat",
    "cat",
    // operators in function form
    "plus",
    "minus",
    "times",
    "mtimes",
    "rdivide",
    "ldivide",
    "mrdivide",
    "mldivide",
    "power",
    "mpower",
    "eq",
    "ne",
    "lt",
    "le",
    "gt",
    "ge",
    "and",
    "or",
    "not",
    "uminus",
    "uplus",
    "transpose",
    "ctranspose",
];

/// Decide only from the class: a 1x1 stand-in saves the full form.
const BY_CLASS: &[&str] = &[
    "isa",
    "isnumeric",
    "isfloat",
    "islogical",
    "isinteger",
    "isreal",
    "ischar",
    "iscell",
    "isstruct",
    "iscellstr",
    "isstring",
    "isobject",
    "is_function_handle",
];

/// `f(0) = 0`: run on the nonzeros.
const ZERO_PRESERVING: &[&str] = &[
    "abs", "real", "imag", "conj", "sign", "floor", "ceil", "round", "fix", "sqrt", "sin", "tan",
    "asin", "atan", "sinh", "tanh", "asinh", "atanh", "expm1", "log1p",
];

/// Computed on the full form; MATLAB's first output is sparse.
const SPARSE_RESULT: &[&str] = &[
    "xor",
    "sum",
    "prod",
    "cumsum",
    "cumprod",
    "max",
    "min",
    "any",
    "all",
    "mean",
    "kron",
    "inv",
    "repmat",
    "circshift",
    "flip",
    "fliplr",
    "flipud",
    "rot90",
    "sort",
    "isnan",
    "isinf",
    "isfinite",
    "cast",
    "exp",
    "cos",
    "cosh",
    "log",
    "log2",
    "log10",
    "mod",
    "rem",
    "trace",
    "abs",
    "real",
    "imag",
    "conj",
    "sign",
    "floor",
    "ceil",
    "round",
    "fix",
    "sqrt",
    "sin",
    "tan",
    "asin",
    "atan",
    "sinh",
    "tanh",
    "asinh",
    "atanh",
    "expm1",
    "log1p",
];

/// A sparse value in any position makes these sparse; the others look at their
/// first argument only (`repmat(A, S, 1)` gets a size from `S`, nothing more).
const SPARSE_FROM_ANY: &[&str] = &["xor", "max", "min", "kron", "mod", "rem"];

/// Run builtin `f` called `name` with at least one sparse argument.
pub fn call(
    it: &mut Interp,
    name: &str,
    f: BuiltinFn,
    mut args: Vec<Value>,
    nargout: usize,
) -> R<Vec<Value>> {
    if AWARE.contains(&name) {
        return f(it, args, nargout);
    }
    if BY_CLASS.contains(&name) {
        for a in args.iter_mut() {
            if let Value::Sparse(s) = a {
                *a = if s.logical {
                    Value::bool(false)
                } else if s.im.is_some() {
                    Value::complex(0.0, 1.0)
                } else {
                    Value::num(0.0)
                };
            }
        }
        return f(it, args, nargout);
    }
    if let Some(r) = native(it, name, f, &args, nargout)? {
        return Ok(r);
    }
    let sparse_out =
        SPARSE_RESULT.contains(&name) && (args[0].is_sparse() || SPARSE_FROM_ANY.contains(&name));
    let dense = args
        .iter()
        .map(sparse::full)
        .collect::<R<Vec<Value>>>()
        .map_err(|e| {
            err(
                &e.identifier,
                format!("{}: {}", name, e.message.trim_end_matches('.')),
            )
        })?;
    let mut r = f(it, dense, nargout)?;
    if sparse_out && let Some(first) = r.first_mut() {
        *first = sparse::sparsify(std::mem::take(first));
    }
    Ok(r)
}

/// The forms that work on the storage.
fn native(
    it: &mut Interp,
    name: &str,
    f: BuiltinFn,
    a: &[Value],
    nargout: usize,
) -> R<Option<Vec<Value>>> {
    let Value::Sparse(s) = &a[0] else {
        return Ok(None);
    };
    let done = |v: Value| Ok(Some(vec![v]));
    if a.len() == 1 && ZERO_PRESERVING.contains(&name) {
        return on_nonzeros(it, s, f).map(Some);
    }
    match (name, a.len()) {
        ("nnz", 1) => done(Value::num(s.nnz() as f64)),
        ("double", 1) => done(wrap(s.as_double())),
        ("logical", 1) => {
            if s.im.is_some() {
                return Err(err(
                    "MATLAB:logical:complex",
                    "Complex values cannot be converted to logicals.",
                ));
            }
            if s.re.iter().any(|v| v.is_nan()) {
                return Err(err(
                    "MATLAB:nologicalnan",
                    "NaN's cannot be converted to logicals.",
                ));
            }
            done(wrap(s.map_nz(true, |_, _| (1.0, 0.0))))
        }
        ("find", 1) => Ok(Some(find(s, nargout))),
        ("squeeze", 1) => done(a[0].clone()),
        ("reshape", _) => done(wrap(reshape(s, &a[1..])?)),
        ("permute", 2) => done(wrap(permute(s, &a[1])?)),
        ("sum", 1 | 2) => {
            let dim = match a.get(1) {
                None => {
                    if s.dims[0] == 1 && s.dims[1] != 1 {
                        2
                    } else {
                        1
                    }
                }
                Some(d) if d.is_text() => return Ok(None),
                Some(d) => arg_usize(d, "sum")?,
            };
            done(wrap(sum(s, dim)?))
        }
        ("tril" | "triu", 1 | 2) => {
            let k = match a.get(1) {
                Some(v) => arg_f64(v, name)? as i64,
                None => 0,
            };
            let lower = name == "tril";
            let e: Vec<Entry> = s
                .entries()
                .into_iter()
                .filter(|e| {
                    let d = e.1 as i64 - e.0 as i64;
                    if lower { d <= k } else { d >= k }
                })
                .collect();
            done(wrap(Sparse::from_sorted(
                s.dims[0], s.dims[1], s.logical, &e,
            )))
        }
        ("diag", 1 | 2) => {
            let k = match a.get(1) {
                Some(v) => arg_f64(v, "diag")? as i64,
                None => 0,
            };
            done(wrap(diag(s, k)?))
        }
        _ => Ok(None),
    }
}

fn on_nonzeros(it: &mut Interp, s: &Sparse, f: BuiltinFn) -> R<Vec<Value>> {
    if s.nnz() == 0 {
        return Ok(vec![wrap(s.as_double())]);
    }
    let input = Value::Num(Num {
        class: NumClass::Double,
        dims: vec![s.nnz(), 1],
        re: s.re.clone(),
        im: s.im.clone(),
    });
    let r = f(it, vec![input], 1)?;
    Ok(vec![wrap(with_values(s, r.first())?)])
}

/// `s` with its stored elements replaced by a column of new values.
fn with_values(s: &Sparse, values: Option<&Value>) -> R<Sparse> {
    let n = values.and_then(Value::to_num).ok_or_else(|| {
        err(
            "MATLAB:sparse:invalidResult",
            "The function must return numeric values.",
        )
    })?;
    if n.re.len() != s.nnz() {
        return Err(err(
            "MATLAB:sparse:invalidResult",
            "The function must return one value per nonzero.",
        ));
    }
    let e: Vec<Entry> = s
        .entries()
        .iter()
        .enumerate()
        .map(|(k, x)| (x.0, x.1, n.re[k], n.im_at(k)))
        .collect();
    Ok(Sparse::from_sorted(s.dims[0], s.dims[1], false, &e))
}

fn find(s: &Sparse, nargout: usize) -> Vec<Value> {
    let m = s.dims[0];
    let e = s.entries();
    let row = m == 1 && s.dims[1] != 1;
    let vec = |v: Vec<f64>| {
        if row { Value::row(v) } else { Value::col(v) }
    };
    if nargout <= 1 {
        return vec![vec(e.iter().map(|x| (x.0 + x.1 * m + 1) as f64).collect())];
    }
    let mut out = vec![
        vec(e.iter().map(|x| (x.0 + 1) as f64).collect()),
        vec(e.iter().map(|x| (x.1 + 1) as f64).collect()),
    ];
    if nargout >= 3 {
        let dims = if row {
            vec![1, e.len()]
        } else {
            vec![e.len(), 1]
        };
        out.push(if s.logical {
            indy_value::bool_array(dims, vec![true; e.len()])
        } else {
            let mut n = Num::real(dims, s.re.clone());
            n.im = s.im.clone();
            Value::Num(n)
        });
    }
    out
}

fn reshape(s: &Sparse, sizes: &[Value]) -> R<Sparse> {
    let total = s.dims[0].saturating_mul(s.dims[1]);
    let bad = || {
        err(
            "MATLAB:getReshapeDims:notSameNumel",
            "Number of elements must not change. Use [] as one of the size inputs to automatically calculate the appropriate size for that dimension.",
        )
    };
    let mut dims: Vec<Option<usize>> = Vec::new();
    if let [v] = sizes {
        for x in v.to_f64_vec().unwrap_or_default() {
            dims.push(Some(x as usize));
        }
    } else {
        for v in sizes {
            dims.push(if v.is_empty() {
                None
            } else {
                Some(arg_usize(v, "reshape")?)
            });
        }
    }
    let known: usize = dims.iter().flatten().product();
    if dims.iter().filter(|d| d.is_none()).count() > 1 {
        return Err(err(
            "MATLAB:getReshapeDims:unknownDim",
            "Size can only have one unknown dimension.",
        ));
    }
    let mut full: Vec<usize> = Vec::with_capacity(dims.len());
    for d in &dims {
        full.push(match d {
            Some(x) => *x,
            None => {
                if known == 0 || !total.is_multiple_of(known) {
                    return Err(bad());
                }
                total / known
            }
        });
    }
    while full.len() > 2 && full.last() == Some(&1) {
        full.pop();
    }
    if full.len() != 2 {
        if full.len() > 2 {
            return Err(err(
                "MATLAB:sparse:NDInput",
                "Sparse matrices must be 2-D: reshape cannot make an N-D sparse array.",
            ));
        }
        return Err(err(
            "MATLAB:getReshapeDims:sizeVector",
            "Size vector must have at least two elements.",
        ));
    }
    if full[0].saturating_mul(full[1]) != total {
        return Err(bad());
    }
    check_extent(full[1])?;
    Ok(s.reshaped(full[0], full[1]))
}

fn permute(s: &Sparse, order: &Value) -> R<Sparse> {
    let p = order.to_f64_vec().unwrap_or_default();
    let mut seen = vec![false; p.len()];
    for x in &p {
        let k = *x as usize;
        if *x < 1.0 || x.fract() != 0.0 || k > p.len() || seen[k - 1] {
            return Err(err(
                "MATLAB:permute:invalidPermutation",
                "Invalid permutation vector: each dimension must appear exactly once.",
            ));
        }
        seen[k - 1] = true;
    }
    if p.len() < 2 {
        return Err(err(
            "MATLAB:permute:tooFewDimensions",
            "Permutation vector must have at least 2 elements.",
        ));
    }
    if p[2..].iter().enumerate().any(|(k, x)| *x as usize != k + 3) {
        return Err(err(
            "MATLAB:sparse:NDInput",
            "Sparse matrices must be 2-D: permute cannot move a dimension past the second.",
        ));
    }
    Ok(if p[0] == 2.0 {
        s.transposed(false)
    } else {
        s.clone()
    })
}

fn sum(s: &Sparse, dim: usize) -> R<Sparse> {
    let [m, n] = s.dims;
    let along = |rows: usize, cols: usize, key: &dyn Fn(&Entry) -> (usize, usize)| {
        let e: Vec<Entry> = s
            .entries()
            .iter()
            .map(|x| {
                let (r, c) = key(x);
                (r, c, x.2, x.3)
            })
            .collect();
        Sparse::from_triplets(rows, cols, false, e)
    };
    match dim {
        0 => Err(err(
            "MATLAB:getdimarg:dimensionMustBePositiveInteger",
            "Dimension argument must be a positive integer scalar.",
        )),
        1 => Ok(along(1, n, &|x| (0, x.1))),
        2 => Ok(along(m, 1, &|x| (x.0, 0))),
        _ => Ok(s.as_double()),
    }
}

fn diag(s: &Sparse, k: i64) -> R<Sparse> {
    let [m, n] = s.dims;
    let up = k.unsigned_abs() as usize;
    let (dr, dc) = if k >= 0 { (0, up) } else { (up, 0) };
    if m == 1 || n == 1 {
        let size = check_extent(m.max(n) + up)?;
        let e: Vec<Entry> = s
            .entries()
            .iter()
            .map(|x| {
                let t = x.0 + x.1;
                (t + dr, t + dc, x.2, x.3)
            })
            .collect();
        return Ok(Sparse::from_sorted(size, size, s.logical, &e));
    }
    let len = if dr >= m || dc >= n {
        0
    } else {
        (m - dr).min(n - dc)
    };
    let e: Vec<Entry> = s
        .entries()
        .iter()
        .filter(|x| x.0 >= dr && x.1 >= dc && x.0 - dr == x.1 - dc)
        .map(|x| (x.0 - dr, 0, x.2, x.3))
        .collect();
    Ok(Sparse::from_sorted(len, 1, s.logical, &e))
}

// ---------------------------------------------------------------------------
// Constructors
// ---------------------------------------------------------------------------

/// `sparse(A)`, `sparse(m, n)`, `sparse(i, j, v)`, `sparse(i, j, v, m, n)`,
/// `sparse(i, j, v, m, n, nzmax)`.
fn sparse_b(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "sparse")?;
    match a.len() {
        1 => one(wrap(to_sparse(&a[0])?)),
        2 => one(wrap(Sparse::zeros(
            extent(&a[0], "sparse")?,
            extent(&a[1], "sparse")?,
            false,
        ))),
        3 | 5 | 6 => from_triplets(&a),
        _ => Err(err(
            "MATLAB:narginchk:tooManyInputs",
            "sparse: expected 1, 2, 3, 5 or 6 input arguments.",
        )),
    }
}

fn from_triplets(a: &[Value]) -> R<Vec<Value>> {
    let index = |v: &Value| -> R<Vec<f64>> {
        sparse::full(v)?.to_f64_vec().ok_or_else(|| {
            err(
                "MATLAB:sparse:invalidIndex",
                "sparse: subscripts must be numeric.",
            )
        })
    };
    let (iv, jv) = (index(&a[0])?, index(&a[1])?);
    let vals = sparse::full(&a[2])?;
    let logical = matches!(vals, Value::Bool(_));
    let vv = match &vals {
        Value::Num(n) if n.class == NumClass::Double => n.clone(),
        Value::Bool(_) => vals.to_num().unwrap_or_else(Num::empty),
        other => {
            return Err(err(
                "MATLAB:UndefinedFunction",
                format!(
                    "Undefined function 'sparse' for input arguments of type '{}'.",
                    other.class_name()
                ),
            ));
        }
    };
    let lens = [iv.len(), jv.len(), vv.re.len()];
    let count = lens.iter().copied().max().unwrap_or(0);
    let count = if lens.contains(&0) { 0 } else { count };
    if lens.iter().any(|l| *l != 1 && *l != count) {
        return Err(err(
            "MATLAB:sparse:ThisVectorSizesMismatch",
            "Vectors must be the same lengths.",
        ));
    }
    let pick = |len: usize, t: usize| if len == 1 { 0 } else { t };
    let mut e = Vec::with_capacity(count);
    let (mut max_i, mut max_j) = (0, 0);
    for t in 0..count {
        let (x, y) = (iv[pick(iv.len(), t)], jv[pick(jv.len(), t)]);
        for w in [x, y] {
            if w < 1.0 || w.fract() != 0.0 || !w.is_finite() {
                return Err(err(
                    "MATLAB:sparse:NonPosIndex",
                    "Index into matrix must be positive.",
                ));
            }
        }
        let (r, c) = (x as usize - 1, y as usize - 1);
        max_i = max_i.max(r + 1);
        max_j = max_j.max(c + 1);
        let k = pick(vv.re.len(), t);
        e.push((r, c, vv.re[k], vv.im_at(k)));
    }
    let (m, n) = if a.len() >= 5 {
        (extent(&a[3], "sparse")?, extent(&a[4], "sparse")?)
    } else {
        (max_i, check_extent(max_j)?)
    };
    if max_i > m || max_j > n {
        return Err(err(
            "MATLAB:sparse:indexExceedsDims",
            "Index exceeds array dimensions.",
        ));
    }
    let s = Sparse::from_triplets(m, n, logical, e);
    if let Some(v) = a.get(5)
        && arg_usize(v, "sparse")? < s.nnz()
    {
        return Err(err(
            "MATLAB:sparse:nzmaxTooSmall",
            "sparse: nzmax must not be less than the number of nonzeros.",
        ));
    }
    one(wrap(s))
}

/// Dimensions from `f(n)`, `f([m n])` or `f(m, n)`.
fn size_args(a: &[Value], fname: &str) -> R<(usize, usize)> {
    let d = match a {
        [] => return Ok((1, 1)),
        [v] => {
            let x = sparse::full(v)?.to_f64_vec().unwrap_or_default();
            match x.as_slice() {
                [n] => vec![*n, *n],
                [m, n] => vec![*m, *n],
                _ => {
                    return Err(err(
                        "MATLAB:speye:invalidSize",
                        format!("{}: size must be a scalar or a two-element vector.", fname),
                    ));
                }
            }
        }
        [m, n, ..] => vec![arg_f64(m, fname)?, arg_f64(n, fname)?],
    };
    let one_dim = |x: f64| -> R<usize> {
        if x.is_nan() || x.fract() != 0.0 {
            return Err(err(
                "MATLAB:NonIntegerInput",
                format!("{}: size inputs must be integers.", fname),
            ));
        }
        check_extent(x.max(0.0) as usize)
    };
    Ok((one_dim(d[0])?, one_dim(d[1])?))
}

fn speye(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let (m, n) = size_args(&a, "speye")?;
    let e: Vec<Entry> = (0..m.min(n)).map(|k| (k, k, 1.0, 0.0)).collect();
    one(wrap(Sparse::from_sorted(m, n, false, &e)))
}

fn spalloc(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "spalloc")?;
    if let Some(v) = a.get(2) {
        arg_usize(v, "spalloc")?;
    }
    let m = extent(&a[0], "spalloc")?;
    let n = match a.get(1) {
        Some(v) => extent(v, "spalloc")?,
        None => m,
    };
    one(wrap(Sparse::zeros(m, n, false)))
}

/// Row of the `B` matrix holding the element of a diagonal: the column
/// number for tall matrices, the row number for wide ones (MATLAB's rule).
fn diag_slot(m: usize, n: usize, r: usize, c: usize) -> usize {
    if m >= n { c } else { r }
}

fn diagonal_list(v: &Value) -> R<Vec<i64>> {
    let d = sparse::full(v)?.to_f64_vec().unwrap_or_default();
    if d.iter().any(|x| x.fract() != 0.0 || !x.is_finite()) {
        return Err(err(
            "MATLAB:spdiags:invalidDiagonal",
            "spdiags: diagonal numbers must be integers.",
        ));
    }
    Ok(d.iter().map(|x| *x as i64).collect())
}

fn spdiags(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "spdiags")?;
    match a.len() {
        1 | 2 => extract_diagonals(&a, nargout),
        3 | 4 => build_diagonals(&a),
        _ => Err(err(
            "MATLAB:narginchk:tooManyInputs",
            "spdiags: expected 1 to 4 input arguments.",
        )),
    }
}

/// `[B, d] = spdiags(A)` and `B = spdiags(A, d)`.
fn extract_diagonals(a: &[Value], nargout: usize) -> R<Vec<Value>> {
    let s = to_sparse(&sparse::full(&a[0])?)?;
    let [m, n] = s.dims;
    let e = s.entries();
    let list = match a.get(1) {
        Some(v) => diagonal_list(v)?,
        None => {
            let mut d: Vec<i64> = e.iter().map(|x| x.1 as i64 - x.0 as i64).collect();
            d.sort_unstable();
            d.dedup();
            d
        }
    };
    let rows = m.min(n);
    let total = rows.saturating_mul(list.len());
    if total > sparse::DENSE_LIMIT {
        return Err(err("MATLAB:nomem", "Out of memory."));
    }
    let mut re = vec![0.0; total];
    let mut im = vec![0.0; total];
    for (r, c, x, y) in e {
        let d = c as i64 - r as i64;
        for (k, want) in list.iter().enumerate() {
            if *want == d {
                let p = diag_slot(m, n, r, c) + k * rows;
                re[p] = x;
                im[p] = y;
            }
        }
    }
    let b = Value::Num(Num::complex(vec![rows, list.len()], re, im));
    let mut out = vec![b];
    if nargout > 1 {
        out.push(Value::col(list.iter().map(|x| *x as f64).collect()));
    }
    Ok(out)
}

/// `S = spdiags(B, d, A)` and `S = spdiags(B, d, m, n)`.
fn build_diagonals(a: &[Value]) -> R<Vec<Value>> {
    let list = diagonal_list(&a[1])?;
    let (m, n, base) = if a.len() == 4 {
        (extent(&a[2], "spdiags")?, extent(&a[3], "spdiags")?, None)
    } else {
        let s = to_sparse(&sparse::full(&a[2])?)?;
        (s.dims[0], s.dims[1], Some(s))
    };
    let b = match sparse::full(&a[0])?.to_num() {
        Some(x) if x.dims.len() == 2 => x,
        _ => {
            return Err(err(
                "MATLAB:spdiags:invalidInput",
                "spdiags: B must be a numeric matrix.",
            ));
        }
    };
    let mut e: Vec<Entry> = match &base {
        // Diagonals named in `d` are replaced, the others stay.
        Some(s) => s
            .entries()
            .into_iter()
            .filter(|x| !list.contains(&(x.1 as i64 - x.0 as i64)))
            .collect(),
        None => Vec::new(),
    };
    let (brows, bcols) = (b.dims[0], b.dims[1]);
    if bcols != list.len() {
        return Err(err(
            "MATLAB:spdiags:InvalidSizeBD",
            "spdiags: B must have one column per diagonal in d.",
        ));
    }
    for (k, d) in list.iter().enumerate() {
        let (r0, c0) = if *d >= 0 {
            (0, *d as usize)
        } else {
            (d.unsigned_abs() as usize, 0)
        };
        if r0 >= m || c0 >= n {
            continue;
        }
        for t in 0..(m - r0).min(n - c0) {
            let (r, c) = (r0 + t, c0 + t);
            let row = diag_slot(m, n, r, c);
            if row >= brows {
                return Err(err(
                    "MATLAB:spdiags:InvalidSizeBD",
                    "spdiags: B has too few rows for this matrix size.",
                ));
            }
            let p = row + k * brows;
            e.push((r, c, b.re[p], b.im_at(p)));
        }
    }
    one(wrap(Sparse::from_triplets(m, n, false, e)))
}

/// `sprand(m, n, density)`, `sprand(S)`, and `sprandn` alike.
fn random(it: &mut Interp, a: Vec<Value>, fname: &str, normal: bool) -> R<Vec<Value>> {
    need(&a, 1, fname)?;
    let draw = |it: &mut Interp| {
        if normal {
            it.rng.normal()
        } else {
            it.rng.uniform()
        }
    };
    if a.len() == 1 {
        let s = to_sparse(&sparse::full(&a[0])?)?;
        let e: Vec<Entry> = s
            .entries()
            .into_iter()
            .map(|x| (x.0, x.1, draw(it), 0.0))
            .collect();
        return one(wrap(Sparse::from_sorted(s.dims[0], s.dims[1], false, &e)));
    }
    if a.len() != 3 {
        return Err(err(
            "MATLAB:sprand:unsupported",
            format!(
                "{}: only {}(S) and {}(m, n, density) are supported in the Rust engine.",
                fname, fname, fname
            ),
        ));
    }
    let m = extent(&a[0], fname)?;
    let n = extent(&a[1], fname)?;
    let density = arg_f64(&a[2], fname)?;
    if !(0.0..=1.0).contains(&density) {
        return Err(err(
            "MATLAB:sprand:invalidDensity",
            format!("{}: density must be between 0 and 1.", fname),
        ));
    }
    let total = (m as f64) * (n as f64);
    let target = (total * density).round() as usize;
    let mut seen = std::collections::HashSet::new();
    let mut e = Vec::new();
    // Positions repeat rarely at low density; a repeat is dropped, so the
    // count can fall short of the target, as MATLAB documents ("approximately").
    for _ in 0..target {
        let r = ((it.rng.uniform() * m as f64) as usize).min(m.saturating_sub(1));
        let c = ((it.rng.uniform() * n as f64) as usize).min(n.saturating_sub(1));
        if seen.insert((r, c)) {
            let v = draw(it);
            e.push((r, c, v, 0.0));
        }
    }
    one(wrap(Sparse::from_triplets(m, n, false, e)))
}

fn nonzeros(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "nonzeros")?;
    let s = to_sparse(&a[0]).map_err(|_| {
        err(
            "MATLAB:nonzeros:invalidInput",
            "nonzeros: numeric or logical input required.",
        )
    })?;
    let dims = vec![s.nnz(), 1];
    if s.logical {
        return one(indy_value::bool_array(dims, vec![true; s.nnz()]));
    }
    let mut n = Num::real(dims, s.re);
    n.im = s.im;
    one(Value::Num(n))
}

/// `spfun(f, S)`: `f` on the stored elements, as a column.
fn spfun(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "spfun")?;
    let s = to_sparse(&sparse::full(&a[1])?)?;
    if s.nnz() == 0 {
        return one(wrap(s.as_double()));
    }
    let f = match &a[0] {
        Value::Char(_) | Value::Str(_) => None,
        h => Some(h.clone()),
    };
    let input = Value::Num(Num {
        class: NumClass::Double,
        dims: vec![s.nnz(), 1],
        re: s.re.clone(),
        im: s.im.clone(),
    });
    let r = match f {
        Some(h) => it.call_value(&h, vec![input], 1)?,
        None => {
            let name = a[0].to_str().unwrap_or_default();
            it.call_named(&name, vec![input], 1)?
        }
    };
    one(wrap(with_values(&s, r.first())?))
}
