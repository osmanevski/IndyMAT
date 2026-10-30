//! Index arithmetic shared by every array type.
//!
//! Indexing is done in two steps: the subscripts are turned into a list of
//! 0-based linear positions plus a result shape (independent of the element
//! type), then each storage gathers or scatters its elements.

use crate::{Array, Error, Num, Res, StructArray, Value, norm_dims, numel_of};

/// One evaluated subscript.
#[derive(Clone, Debug)]
pub enum Sub {
    /// Bare `:`.
    Colon,
    /// 0-based positions; `shape` is the shape of the index value
    /// (for a logical mask: the shape `find` would give).
    List { idx: Vec<usize>, shape: Vec<usize> },
}

impl Sub {
    pub fn len(&self, extent: usize) -> usize {
        match self {
            Sub::Colon => extent,
            Sub::List { idx, .. } => idx.len(),
        }
    }
    pub fn at(&self, k: usize) -> usize {
        match self {
            Sub::Colon => k,
            Sub::List { idx, .. } => idx[k],
        }
    }
    pub fn max_plus_one(&self) -> usize {
        match self {
            Sub::Colon => 0,
            Sub::List { idx, .. } => idx.iter().map(|i| i + 1).max().unwrap_or(0),
        }
    }
    pub fn scalar(i: usize) -> Sub {
        Sub::List {
            idx: vec![i],
            shape: vec![1, 1],
        }
    }
}

fn bad_index(v: f64) -> Box<Error> {
    if v == 0.0 {
        Error::new(
            "MATLAB:index:expected_integer",
            "Index in position 1 is invalid. Array indices must be positive integers or logical values.",
        )
    } else {
        Error::new(
            "MATLAB:index:expected_integer",
            format!(
                "Array indices must be positive integers or logical values (index {}).",
                crate::format::short_g(v)
            ),
        )
    }
}

/// Convert an index value (`magic colon` is handled by the caller).
pub fn to_sub(v: &Value) -> Res<Sub> {
    match v {
        Value::Bool(a) => {
            let idx: Vec<usize> = a
                .data
                .iter()
                .enumerate()
                .filter(|(_, b)| **b)
                .map(|(i, _)| i)
                .collect();
            let n = idx.len();
            let shape = if a.dims.len() == 2 && a.dims[0] == 1 {
                vec![1, n]
            } else {
                vec![n, 1]
            };
            Ok(Sub::List { idx, shape })
        }
        Value::Char(a) if a.data.len() == 1 && a.data[0] == ':' => Ok(Sub::Colon),
        Value::Sparse(s) => crate::sparse::to_sub(s),
        Value::Num(_) | Value::Char(_) => {
            let vals = v.to_f64_vec().unwrap();
            let mut idx = Vec::with_capacity(vals.len());
            for x in vals {
                if x < 1.0 || x.fract() != 0.0 || x.is_nan() {
                    return Err(bad_index(x));
                }
                idx.push(x as usize - 1);
            }
            Ok(Sub::List {
                idx,
                shape: v.dims(),
            })
        }
        other => Err(Error::new(
            "MATLAB:index:expected_integer",
            format!(
                "Array indices must be positive integers or logical values, not {}.",
                other.class_name()
            ),
        )),
    }
}

/// Dims as seen by `n` subscripts: the last subscript absorbs the rest.
pub fn collapse_dims(dims: &[usize], n: usize) -> Vec<usize> {
    let mut d: Vec<usize> = dims.to_vec();
    if n == 0 {
        return d;
    }
    if d.len() > n {
        let tail: usize = d[n - 1..].iter().product();
        d.truncate(n - 1);
        d.push(tail);
    }
    while d.len() < n {
        d.push(1);
    }
    d
}

pub fn out_of_bound(k: usize, n: usize, pos: Option<usize>) -> Box<Error> {
    match pos {
        None => Error::new(
            "MATLAB:badsubscript",
            format!(
                "Index exceeds the number of array elements. Index must not exceed {}. (index {})",
                n, k
            ),
        ),
        Some(p) => Error::new(
            "MATLAB:badsubscript",
            format!(
                "Index in position {} exceeds array bounds. Index must not exceed {}.",
                p, n
            ),
        ),
    }
}

/// Linear positions and result shape for reading `A(subs...)`.
pub fn read_positions(dims: &[usize], subs: &[Sub]) -> Res<(Vec<usize>, Vec<usize>)> {
    let total = numel_of(dims);
    if subs.is_empty() {
        return Ok(((0..total).collect(), dims.to_vec()));
    }
    if subs.len() == 1 {
        let s = &subs[0];
        return match s {
            Sub::Colon => Ok(((0..total).collect(), vec![total, 1])),
            Sub::List { idx, shape } => {
                for &i in idx {
                    if i >= total {
                        return Err(out_of_bound(i + 1, total, None));
                    }
                }
                let n = idx.len();
                let index_is_vector = shape.len() == 2 && (shape[0] == 1 || shape[1] == 1);
                let src_is_vector = dims.len() == 2 && (dims[0] == 1 || dims[1] == 1);
                let out = if index_is_vector && src_is_vector && !(dims[0] == 1 && dims[1] == 1) {
                    if dims[0] == 1 { vec![1, n] } else { vec![n, 1] }
                } else {
                    shape.clone()
                };
                Ok((idx.clone(), out))
            }
        };
    }
    let d = collapse_dims(dims, subs.len());
    let mut out_dims = Vec::with_capacity(subs.len());
    for (k, s) in subs.iter().enumerate() {
        if let Sub::List { idx, .. } = s {
            for &i in idx {
                if i >= d[k] {
                    return Err(out_of_bound(i + 1, d[k], Some(k + 1)));
                }
            }
        }
        out_dims.push(s.len(d[k]));
    }
    let count: usize = out_dims.iter().product();
    let mut pos = Vec::with_capacity(count);
    if count > 0 {
        let mut strides = vec![1usize; subs.len()];
        for k in 1..subs.len() {
            strides[k] = strides[k - 1] * d[k - 1];
        }
        let mut ctr = vec![0usize; subs.len()];
        loop {
            let mut p = 0;
            for k in 0..subs.len() {
                p += subs[k].at(ctr[k]) * strides[k];
            }
            pos.push(p);
            let mut k = 0;
            loop {
                ctr[k] += 1;
                if ctr[k] < out_dims[k] {
                    break;
                }
                ctr[k] = 0;
                k += 1;
                if k == subs.len() {
                    break;
                }
            }
            if k == subs.len() {
                break;
            }
        }
    }
    Ok((pos, norm_dims(out_dims)))
}

/// Target positions for `A(subs...) = rhs` and the (grown) new dims.
/// `rhs_dims` is used to size colons over empty dimensions.
pub fn write_positions(
    dims: &[usize],
    subs: &[Sub],
    rhs_dims: &[usize],
) -> Res<(Vec<usize>, Vec<usize>)> {
    let total = numel_of(dims);
    if subs.len() == 1 {
        return match &subs[0] {
            Sub::Colon => {
                if total == 0 && numel_of(rhs_dims) > 0 {
                    // x = []; x(:) = v keeps the empty shape in MATLAB; we
                    // follow it only when sizes agree.
                    return Ok(((0..0).collect(), dims.to_vec()));
                }
                Ok(((0..total).collect(), dims.to_vec()))
            }
            Sub::List { idx, .. } => {
                let need = idx.iter().map(|i| i + 1).max().unwrap_or(0);
                if need <= total {
                    return Ok((idx.clone(), dims.to_vec()));
                }
                // Linear growth: only vectors (or empties) may grow.
                let new_dims = if total == 0 {
                    if dims.len() == 2 && dims[0] == 0 && dims[1] == 1 {
                        vec![need, 1]
                    } else {
                        vec![1, need]
                    }
                } else if dims.len() == 2 && dims[0] == 1 {
                    vec![1, need]
                } else if dims.len() == 2 && dims[1] == 1 {
                    vec![need, 1]
                } else {
                    return Err(Error::new(
                        "MATLAB:subsassigndimmismatch",
                        "Attempt to grow array along ambiguous dimension.",
                    ));
                };
                Ok((idx.clone(), new_dims))
            }
        };
    }
    let n = subs.len();
    let d = collapse_dims(dims, n);
    let collapsed = dims.len() > n;
    // Rhs non-singleton dims, consumed by colons over empty dims.
    let rhs_scalar = numel_of(rhs_dims) == 1;
    let mut rhs_iter = rhs_dims.iter().copied().filter(|&x| x != 1);
    let mut nd = Vec::with_capacity(n);
    for k in 0..n {
        let ext = match &subs[k] {
            Sub::Colon => {
                if d[k] == 0 && total == 0 {
                    if rhs_scalar {
                        1
                    } else {
                        rhs_iter.next().unwrap_or(1)
                    }
                } else {
                    d[k]
                }
            }
            Sub::List { idx, .. } => {
                if idx.len() != 1 && !rhs_scalar {
                    rhs_iter.next();
                }
                d[k].max(subs[k].max_plus_one())
            }
        };
        nd.push(ext);
    }
    if collapsed && nd != d {
        return Err(Error::new(
            "MATLAB:indexed:resize",
            "A(I,J,...) = X: dimensions mismatch (cannot resize a collapsed array)",
        ));
    }
    let mut strides = vec![1usize; n];
    for k in 1..n {
        strides[k] = strides[k - 1] * nd[k - 1];
    }
    let counts: Vec<usize> = (0..n).map(|k| subs[k].len(nd[k])).collect();
    let count: usize = counts.iter().product();
    let mut pos = Vec::with_capacity(count);
    if count > 0 {
        let mut ctr = vec![0usize; n];
        'outer: loop {
            let mut p = 0;
            for k in 0..n {
                p += subs[k].at(ctr[k]) * strides[k];
            }
            pos.push(p);
            for k in 0..n {
                ctr[k] += 1;
                if ctr[k] < counts[k] {
                    continue 'outer;
                }
                ctr[k] = 0;
            }
            break;
        }
    }
    let new_dims = if collapsed {
        dims.to_vec()
    } else {
        norm_dims(nd)
    };
    Ok((pos, new_dims))
}

/// Re-layout `data` (dims `old`) into `new` dims, filling with `fill`.
pub fn grow<T: Clone>(data: Vec<T>, old: &[usize], new: &[usize], fill: T) -> Vec<T> {
    if old == new {
        return data;
    }
    let total = numel_of(new);
    let old_n = numel_of(old);
    // Same leading layout (e.g. column growth or row vector) keeps order.
    let mut out = vec![fill; total];
    if old_n == 0 {
        return out;
    }
    let nd = new.len().max(old.len());
    let od: Vec<usize> = (0..nd).map(|k| *old.get(k).unwrap_or(&1)).collect();
    let nn: Vec<usize> = (0..nd).map(|k| *new.get(k).unwrap_or(&1)).collect();
    let mut nstr = vec![1usize; nd];
    for k in 1..nd {
        nstr[k] = nstr[k - 1] * nn[k - 1];
    }
    let mut ctr = vec![0usize; nd];
    for v in data.into_iter() {
        let mut p = 0;
        for k in 0..nd {
            p += ctr[k] * nstr[k];
        }
        if p < total {
            out[p] = v;
        }
        for k in 0..nd {
            ctr[k] += 1;
            if ctr[k] < od[k] {
                break;
            }
            ctr[k] = 0;
        }
    }
    out
}

/// Delete elements: `A(subs) = []`.
pub fn delete_positions(dims: &[usize], subs: &[Sub]) -> Res<(Vec<bool>, Vec<usize>)> {
    let total = numel_of(dims);
    let mut keep = vec![true; total];
    if subs.len() == 1 {
        match &subs[0] {
            Sub::Colon => {
                return Ok((vec![false; total], vec![0, 0]));
            }
            Sub::List { idx, .. } => {
                for &i in idx {
                    if i >= total {
                        return Err(out_of_bound(i + 1, total, None));
                    }
                    keep[i] = false;
                }
                let left = keep.iter().filter(|k| **k).count();
                let nd = if dims.len() == 2 && dims[1] == 1 && dims[0] != 1 {
                    vec![left, 1]
                } else if idx.is_empty() {
                    dims.to_vec()
                } else {
                    vec![1, left]
                };
                return Ok((keep, nd));
            }
        }
    }
    let n = subs.len();
    let d = collapse_dims(dims, n);
    // Find the single dimension that is not fully selected.
    let mut target = None;
    for k in 0..n {
        let full = match &subs[k] {
            Sub::Colon => true,
            Sub::List { idx, .. } => {
                let mut seen = vec![false; d[k]];
                for &i in idx {
                    if i < d[k] {
                        seen[i] = true;
                    }
                }
                seen.iter().all(|s| *s) && d[k] > 0 && idx.iter().all(|&i| i < d[k])
            }
        };
        if !full {
            if target.is_some() {
                return Err(Error::new(
                    "MATLAB:nullAssignmentWithMultipleNonColonIndices",
                    "A null assignment can have only one non-colon index.",
                ));
            }
            target = Some(k);
        }
    }
    let Some(t) = target else {
        // Everything selected: result is empty along the first listed dim.
        let mut nd = d.clone();
        nd[0] = 0;
        return Ok((vec![false; total], norm_dims(nd)));
    };
    let mut del = vec![false; d[t]];
    if let Sub::List { idx, .. } = &subs[t] {
        for &i in idx {
            if i >= d[t] {
                return Err(out_of_bound(i + 1, d[t], Some(t + 1)));
            }
            del[i] = true;
        }
    }
    let stride: usize = d[..t].iter().product();
    for (p, kp) in keep.iter_mut().enumerate() {
        let c = (p / stride) % d[t];
        if del[c] {
            *kp = false;
        }
    }
    let mut nd = d.clone();
    nd[t] = d[t] - del.iter().filter(|x| **x).count();
    Ok((keep, norm_dims(nd)))
}

pub fn gather<T: Clone>(data: &[T], pos: &[usize]) -> Vec<T> {
    pos.iter().map(|&p| data[p].clone()).collect()
}

fn keep_only<T>(data: Vec<T>, keep: &[bool]) -> Vec<T> {
    data.into_iter()
        .zip(keep.iter())
        .filter(|(_, k)| **k)
        .map(|(v, _)| v)
        .collect()
}

// ---------------------------------------------------------------------------
// Value-level operations
// ---------------------------------------------------------------------------

/// `v(subs...)` for any value.
pub fn index_value(v: &Value, subs: &[Sub]) -> Res<Value> {
    if let Value::Sparse(s) = v {
        return crate::sparse::index(s, subs);
    }
    // Fast path: scalar subscripts.
    let (pos, dims) = read_positions(v.dims_ref(), subs)?;
    Ok(match v {
        Value::Num(n) => Value::Num(Num {
            class: n.class,
            dims,
            re: gather(&n.re, &pos),
            im: n.im.as_ref().map(|im| gather(im, &pos)),
        })
        .narrowed(),
        Value::Bool(a) => Value::Bool(Array {
            dims,
            data: gather(&a.data, &pos),
        }),
        Value::Char(a) => Value::Char(Array {
            dims,
            data: gather(&a.data, &pos),
        }),
        Value::Cell(a) => Value::Cell(Array {
            dims,
            data: gather(&a.data, &pos),
        }),
        Value::Struct(s) => Value::Struct(StructArray {
            dims,
            fields: s.fields.clone(),
            elems: gather(&s.elems, &pos),
        }),
        Value::Str(a) => Value::Str(Array {
            dims,
            data: gather(&a.data, &pos),
        }),
        Value::Object(_) | Value::HObject(_) | Value::ObjArr(_) => {
            let o = crate::ObjArray::parts(v).unwrap();
            o.with(dims, gather(&o.arr.data, &pos))
        }
        Value::Sparse(_) => unreachable!("handled above"),
        Value::Func(_) => {
            if pos.iter().all(|p| *p == 0) && pos.len() == 1 {
                v.clone()
            } else {
                return Err(Error::new(
                    "MATLAB:badsubscript",
                    "Index exceeds the number of array elements. Index must not exceed 1.",
                ));
            }
        }
    })
}

impl Value {
    pub fn narrowed(self) -> Value {
        match self {
            Value::Num(mut n) => {
                n.narrow();
                Value::Num(n)
            }
            v => v,
        }
    }
}

/// Type a mixed assignment target/rhs pair resolves to.
fn assign_kind(target: &Value, rhs: &Value) -> Res<u8> {
    // 0 num, 1 bool, 2 char, 3 cell, 4 struct
    Ok(match (target, rhs) {
        (Value::Str(_), Value::Str(_) | Value::Char(_) | Value::Num(_) | Value::Bool(_)) => 5,
        (Value::Num(n), Value::Str(_)) if n.re.is_empty() => 5,
        (Value::Cell(_), Value::Cell(_)) => 3,
        (Value::Cell(_), _) => {
            return Err(Error::new(
                "MATLAB:invalidConversion",
                format!(
                    "Conversion to cell from {} is not possible.",
                    rhs.class_name()
                ),
            ));
        }
        (Value::Struct(_), Value::Struct(_)) => 4,
        (Value::Struct(_), _) => {
            return Err(Error::new(
                "MATLAB:invalidConversion",
                format!(
                    "Conversion to struct from {} is not possible.",
                    rhs.class_name()
                ),
            ));
        }
        (Value::Bool(_), Value::Bool(_)) => 1,
        (Value::Char(_), Value::Char(_)) => 2,
        (
            Value::Num(_) | Value::Bool(_) | Value::Char(_),
            Value::Num(_) | Value::Bool(_) | Value::Char(_),
        ) => 0,
        (_, _) => {
            return Err(Error::new(
                "MATLAB:invalidConversion",
                format!(
                    "Conversion to {} from {} is not possible.",
                    target.class_name(),
                    rhs.class_name()
                ),
            ));
        }
    })
}

fn scatter<T: Clone>(
    data: Vec<T>,
    old: &[usize],
    new: &[usize],
    fill: T,
    pos: &[usize],
    src: &[T],
) -> Vec<T> {
    let mut d = grow(data, old, new, fill);
    if src.len() == 1 {
        for &p in pos {
            d[p] = src[0].clone();
        }
    } else {
        for (k, &p) in pos.iter().enumerate() {
            d[p] = src[k].clone();
        }
    }
    d
}

/// `target(subs...) = rhs` in place. `target` of `None` means undefined.
pub fn assign_index(target: &mut Option<Value>, subs: &[Sub], rhs: Value) -> Res<()> {
    if rhs.is_sparse() || matches!(target, Some(Value::Sparse(_))) {
        return crate::sparse::assign(target, subs, rhs);
    }
    let tgt_obj = target.as_ref().is_some_and(Value::is_object_like);
    let blank = match target {
        None => true,
        Some(Value::Num(n)) => n.re.is_empty() && n.dims == [0, 0],
        _ => false,
    };
    if tgt_obj || (rhs.is_object_like() && blank) {
        return assign_objects(target, subs, rhs);
    }
    let base = match target.take() {
        Some(v) => v,
        None => match &rhs {
            Value::Cell(_) => Value::empty_cell(),
            Value::Struct(s) => {
                Value::Struct(StructArray::empty_with(s.fields.clone(), vec![0, 0]))
            }
            Value::Str(_) => Value::Str(Array::new(vec![0, 0], vec![])),
            Value::Bool(_) => Value::Bool(Array::new(vec![0, 0], vec![])),
            Value::Char(_) => Value::Char(Array::new(vec![0, 0], vec![])),
            Value::Num(n) => Value::Num(Num {
                class: n.class,
                ..Num::empty()
            }),
            Value::Sparse(_) => unreachable!("handled above"),
            Value::Func(_) | Value::Object(_) | Value::HObject(_) | Value::ObjArr(_) => {
                if subs
                    .iter()
                    .all(|s| matches!(s, Sub::List{idx, ..} if idx == &vec![0]))
                {
                    *target = Some(rhs);
                    return Ok(());
                }
                return Err(Error::msg(
                    "Cannot create an array of this class by indexing.",
                ));
            }
        },
    };
    // Retype an empty double target to the rhs type (x = []; x(2) = 'a').
    let base = match (base, &rhs) {
        (Value::Num(n), Value::Cell(_)) if n.re.is_empty() => {
            Value::Cell(Array::new(n.dims.clone(), vec![]))
        }
        (Value::Num(n), Value::Struct(s)) if n.re.is_empty() => {
            Value::Struct(StructArray::empty_with(s.fields.clone(), n.dims.clone()))
        }
        (Value::Num(n), Value::Char(_)) if n.re.is_empty() && n.dims == [0, 0] => {
            Value::Char(Array::new(vec![0, 0], vec![]))
        }
        (Value::Num(n), Value::Bool(_)) if n.re.is_empty() && n.dims == [0, 0] => {
            Value::Bool(Array::new(vec![0, 0], vec![]))
        }
        (b, _) => b,
    };
    let rn = rhs.numel();
    let old = base.dims();
    let checked = write_positions(&old, subs, rhs.dims_ref()).and_then(|(pos, new)| {
        if rn != 1 && rn != pos.len() {
            return Err(Error::new(
                "MATLAB:subsassignnumelmismatch",
                format!(
                    "Unable to perform assignment because the left and right sides have a different number of elements ({} and {}).",
                    pos.len(),
                    rn
                ),
            ));
        }
        let kind = assign_kind(&base, &rhs)?;
        Ok((pos, new, kind))
    });
    match checked {
        Ok((pos, new, kind)) => {
            *target = Some(if pos.is_empty() && new == old {
                base
            } else {
                assign_checked(base, rhs, old, new, pos, kind)
            });
            Ok(())
        }
        Err(e) => {
            *target = Some(base);
            Err(e)
        }
    }
}

/// Assignment into an object array. Elements created by growing the array
/// are left as `Value::default()`; the interpreter replaces them with
/// default-constructed objects.
fn assign_objects(target: &mut Option<Value>, subs: &[Sub], rhs: Value) -> Res<()> {
    let Some(r) = crate::ObjArray::parts(&rhs) else {
        return Err(Error::new(
            "MATLAB:invalidConversion",
            format!(
                "Conversion to {} from {} is not possible.",
                target.as_ref().map(|t| t.class_name()).unwrap_or_default(),
                rhs.class_name()
            ),
        ));
    };
    let mut t = match target.as_ref().and_then(crate::ObjArray::parts) {
        Some(p) => p,
        None => r.with_empty(),
    };
    if t.class != r.class {
        return Err(Error::new(
            "MATLAB:invalidConversion",
            format!(
                "Conversion to {} from {} is not possible.",
                t.class, r.class
            ),
        ));
    }
    let r = r.arr;
    let old = t.arr.dims.clone();
    let (pos, new) = write_positions(&old, subs, &r.dims)?;
    if r.data.len() != 1 && r.data.len() != pos.len() {
        return Err(Error::new(
            "MATLAB:subsassignnumelmismatch",
            format!(
                "Unable to perform assignment because the left and right sides have a different number of elements ({} and {}).",
                pos.len(),
                r.data.len()
            ),
        ));
    }
    let data = std::mem::take(&mut t.arr.data);
    let data = scatter(data, &old, &new, Value::default(), &pos, &r.data);
    *target = Some(t.with(new, data));
    Ok(())
}

fn assign_checked(
    base: Value,
    rhs: Value,
    old: Vec<usize>,
    new: Vec<usize>,
    pos: Vec<usize>,
    kind: u8,
) -> Value {
    match kind {
        5 => {
            let a = match base {
                Value::Str(a) => a,
                other => Array::new(other.dims(), vec![None; other.numel()]),
            };
            let r = crate::to_string_array(&rhs);
            Value::Str(Array {
                data: scatter(a.data, &old, &new, None, &pos, &r.data),
                dims: new,
            })
        }
        3 => {
            let (Value::Cell(a), Value::Cell(r)) = (base, rhs) else {
                unreachable!()
            };
            Value::Cell(Array {
                data: scatter(a.data, &old, &new, Value::default(), &pos, &r.data),
                dims: new,
            })
        }
        4 => {
            let (Value::Struct(mut s), Value::Struct(r)) = (base, rhs) else {
                unreachable!()
            };
            // Union of fields; rhs elements re-ordered to target fields.
            for f in &r.fields {
                s.ensure_field(f);
            }
            let nf = s.fields.len();
            let src: Vec<Vec<Value>> = r
                .elems
                .iter()
                .map(|e| {
                    s.fields
                        .iter()
                        .map(|f| match r.field_index(f) {
                            Some(i) => e[i].clone(),
                            None => Value::default(),
                        })
                        .collect()
                })
                .collect();
            let elems = scatter(s.elems, &old, &new, vec![Value::default(); nf], &pos, &src);
            Value::Struct(StructArray {
                dims: new,
                fields: s.fields,
                elems,
            })
        }
        1 => {
            let (Value::Bool(a), Value::Bool(r)) = (base, rhs) else {
                unreachable!()
            };
            Value::Bool(Array {
                data: scatter(a.data, &old, &new, false, &pos, &r.data),
                dims: new,
            })
        }
        2 => {
            let (Value::Char(a), Value::Char(r)) = (base, rhs) else {
                unreachable!()
            };
            Value::Char(Array {
                data: scatter(a.data, &old, &new, '\0', &pos, &r.data),
                dims: new,
            })
        }
        _ => {
            let class = match (&base, &rhs) {
                (Value::Num(a), _) if a.class != crate::NumClass::Double => a.class,
                (_, Value::Num(r)) if r.class != crate::NumClass::Double => r.class,
                (Value::Num(a), _) => a.class,
                _ => crate::NumClass::Double,
            };
            let a = match base {
                Value::Num(n) => n,
                other => other.to_num().unwrap(),
            };
            let r = match rhs {
                Value::Num(n) => n,
                other => other.to_num().unwrap(),
            }
            .with_class(class);
            let re = scatter(a.re, &old, &new, 0.0, &pos, &r.re);
            let im = if a.im.is_some() || r.im.is_some() {
                let ai = a.im.unwrap_or_else(|| vec![0.0; old.iter().product()]);
                let ri = r.im.unwrap_or_else(|| vec![0.0; r.re.len()]);
                Some(scatter(ai, &old, &new, 0.0, &pos, &ri))
            } else {
                None
            };
            let mut n = Num {
                class,
                dims: new,
                re,
                im,
            };
            n.narrow();
            Value::Num(n)
        }
    }
}

/// `target(subs...) = []`.
pub fn delete_index(v: Value, subs: &[Sub]) -> Res<Value> {
    if let Value::Sparse(s) = &v {
        return crate::sparse::delete(s, subs);
    }
    let (keep, nd) = delete_positions(v.dims_ref(), subs)?;
    Ok(match v {
        Value::Num(n) => Value::Num(Num {
            class: n.class,
            dims: nd,
            re: keep_only(n.re, &keep),
            im: n.im.map(|im| keep_only(im, &keep)),
        }),
        Value::Bool(a) => Value::Bool(Array {
            dims: nd,
            data: keep_only(a.data, &keep),
        }),
        Value::Char(a) => Value::Char(Array {
            dims: nd,
            data: keep_only(a.data, &keep),
        }),
        Value::Cell(a) => Value::Cell(Array {
            dims: nd,
            data: keep_only(a.data, &keep),
        }),
        Value::Struct(s) => Value::Struct(StructArray {
            dims: nd,
            fields: s.fields,
            elems: keep_only(s.elems, &keep),
        }),
        Value::Str(a) => Value::Str(Array {
            dims: nd,
            data: keep_only(a.data, &keep),
        }),
        v @ (Value::Object(_) | Value::HObject(_) | Value::ObjArr(_)) => {
            let o = crate::ObjArray::parts(&v).unwrap();
            o.with(nd, keep_only(o.arr.data.clone(), &keep))
        }
        other => {
            return Err(Error::msg(format!(
                "Deleting elements of a {} is not supported.",
                other.class_name()
            )));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn l(v: &[usize]) -> Sub {
        Sub::List {
            idx: v.iter().map(|x| x - 1).collect(),
            shape: vec![1, v.len()],
        }
    }
    #[test]
    fn read_matrix() {
        let (p, d) = read_positions(&[3, 4], &[Sub::Colon, l(&[2])]).unwrap();
        assert_eq!(p, vec![3, 4, 5]);
        assert_eq!(d, vec![3, 1]);
    }
    #[test]
    fn grow_row() {
        let mut t = Some(Value::row(vec![1.0, 2.0]));
        assign_index(&mut t, &[l(&[4])], Value::num(9.0)).unwrap();
        assert_eq!(t.unwrap(), Value::row(vec![1.0, 2.0, 0.0, 9.0]));
    }
    #[test]
    fn grow_matrix() {
        let mut t = Some(Value::matrix(vec![2, 2], vec![1.0, 2.0, 3.0, 4.0]));
        assign_index(&mut t, &[l(&[3]), l(&[3])], Value::num(9.0)).unwrap();
        let v = t.unwrap();
        assert_eq!(v.dims(), vec![3, 3]);
        assert_eq!(
            v.to_f64_vec().unwrap(),
            vec![1.0, 2.0, 0.0, 3.0, 4.0, 0.0, 0.0, 0.0, 9.0]
        );
    }
    #[test]
    fn delete_cols() {
        let v = Value::matrix(vec![2, 3], vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        let r = delete_index(v, &[Sub::Colon, l(&[2])]).unwrap();
        assert_eq!(r.dims(), vec![2, 2]);
        assert_eq!(r.to_f64_vec().unwrap(), vec![1.0, 2.0, 5.0, 6.0]);
    }
    #[test]
    fn colon_on_empty() {
        let mut t = Some(Value::empty());
        assign_index(
            &mut t,
            &[Sub::Colon, l(&[1])],
            Value::col(vec![1.0, 2.0, 3.0]),
        )
        .unwrap();
        assert_eq!(t.unwrap().dims(), vec![3, 1]);
    }
}
