//! `categorical`: arrays of values from a finite set of categories.
//!
//! Storage: `#c` category codes (1-based doubles, 0 = `<undefined>`),
//! `_cats` (cell column of category names), `_ord` and `_prot` flags.

use indy_syntax::ast::BinOp;
use indy_value::{Array, Num, Object, Value, format};

use super::{ClassOps, bools, cellstr_col, class_is, data, new_obj, obj_of, set_data, text_list};
use crate::builtins::{err, need, reg};
use crate::{BuiltinFn, Interp, R};

pub static OPS: ClassOps = ClassOps {
    name: "categorical",
    get: |_, n| {
        Err(err(
            "MATLAB:categorical:UnrecognizedProperty",
            format!(
                "Unrecognized method, property, or field '{}' for class 'categorical'.",
                n
            ),
        ))
    },
    set: |_, n, _| {
        Err(err(
            "MATLAB:categorical:UnrecognizedProperty",
            format!("Unrecognized property '{}' for class 'categorical'.", n),
        ))
    },
    convert,
    binary,
    disp: |o| grid(o),
    display: None,
    fill: |_| 0.0,
    methods: METHODS,
};

pub const UNDEF: &str = "<undefined>";

pub fn cats(o: &Object) -> Vec<String> {
    o.get("_cats").and_then(text_list).unwrap_or_default()
}

pub fn is_ordinal(o: &Object) -> bool {
    o.get("_ord").and_then(|v| v.scalar_f64()).unwrap_or(0.0) != 0.0
}

fn is_protected(o: &Object) -> bool {
    o.get("_prot").and_then(|v| v.scalar_f64()).unwrap_or(0.0) != 0.0
}

pub fn make(dims: Vec<usize>, codes: Vec<f64>, cats: &[String], ord: bool, prot: bool) -> Object {
    let mut o = new_obj("categorical", dims);
    set_data(&mut o, "#c", codes);
    o.set("_cats", cellstr_col(cats));
    o.set("_ord", Value::bool(ord));
    o.set("_prot", Value::bool(prot || ord));
    o
}

pub fn undefined_array(dims: Vec<usize>) -> Value {
    let n = dims.iter().product();
    Value::Object(Box::new(make(dims, vec![0.0; n], &[], false, false)))
}

pub fn texts(o: &Object) -> Vec<String> {
    let c = cats(o);
    data(o, "#c")
        .iter()
        .map(|&k| {
            if k >= 1.0 {
                c.get(k as usize - 1)
                    .cloned()
                    .unwrap_or_else(|| UNDEF.into())
            } else {
                UNDEF.into()
            }
        })
        .collect()
}

fn grid(o: &Object) -> String {
    let t = texts(o);
    if t.is_empty() {
        return String::new();
    }
    let r = o.dims[0];
    let c: usize = o.dims[1..].iter().product();
    let w = t.iter().map(|s| s.chars().count()).max().unwrap_or(0);
    let mut s = String::new();
    for i in 0..r {
        s.push_str("    ");
        let row: Vec<String> = (0..c)
            .map(|j| {
                let x = &t[i + j * r];
                format!("{}{}", x, " ".repeat(w - x.chars().count()))
            })
            .collect();
        s.push_str(row.join("      ").trim_end());
        s.push('\n');
    }
    s
}

fn wrap(o: Object) -> Value {
    Value::Object(Box::new(o))
}

/// Text labels of plain values (numbers via num2str) and missing flags.
/// Shape, labels (`None` = missing) and numeric values of plain data.
type Labels = (Vec<usize>, Vec<Option<String>>, Option<Vec<f64>>);

fn labels(v: &Value) -> R<Labels> {
    match v {
        Value::Num(n) => {
            let labs =
                n.re.iter()
                    .map(|x| {
                        if x.is_nan() {
                            None
                        } else {
                            Some(num_label(*x))
                        }
                    })
                    .collect();
            Ok((n.dims.clone(), labs, Some(n.re.clone())))
        }
        Value::Bool(b) => Ok((
            b.dims.clone(),
            b.data
                .iter()
                .map(|x| Some(if *x { "true" } else { "false" }.to_string()))
                .collect(),
            Some(b.data.iter().map(|x| if *x { 1.0 } else { 0.0 }).collect()),
        )),
        Value::Str(s) => Ok((s.dims.clone(), s.data.clone(), None)),
        Value::Char(c) => {
            if c.dims[0] > 1 {
                let rows = v.char_rows().unwrap_or_default();
                Ok((
                    vec![rows.len(), 1],
                    rows.into_iter()
                        .map(|r| Some(r.trim_end().to_string()))
                        .collect(),
                    None,
                ))
            } else {
                let t = v.to_str().unwrap_or_default();
                Ok((
                    vec![1, 1],
                    vec![if t.is_empty() { None } else { Some(t) }],
                    None,
                ))
            }
        }
        Value::Cell(c) => {
            let mut out = Vec::with_capacity(c.data.len());
            for x in &c.data {
                let t = x.to_str().ok_or_else(|| {
                    err(
                        "MATLAB:categorical:CellNotText",
                        "categorical: a cell array input must contain only character vectors.",
                    )
                })?;
                out.push(if t.is_empty() { None } else { Some(t) });
            }
            Ok((c.dims.clone(), out, None))
        }
        _ => Err(err(
            "MATLAB:categorical:InvalidData",
            format!(
                "categorical: cannot create a categorical array from a {}.",
                v.class_name()
            ),
        )),
    }
}

fn num_label(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        format::fmt_g(x, 5, false, false)
    }
}

/// Codes of `labs` in `cats`, appending unknown labels when allowed.
fn encode(labs: &[Option<String>], cats: &mut Vec<String>, grow: bool) -> R<Vec<f64>> {
    labs.iter()
        .map(|l| match l {
            None => Ok(0.0),
            Some(s) if s == UNDEF => Ok(0.0),
            Some(s) => match cats.iter().position(|c| c == s) {
                Some(k) => Ok(k as f64 + 1.0),
                None if grow => {
                    cats.push(s.clone());
                    Ok(cats.len() as f64)
                }
                None => Err(err(
                    "MATLAB:categorical:ProtectedForAssign",
                    format!(
                        "Cannot add the new category '{}' to a protected categorical array.",
                        s
                    ),
                )),
            },
        })
        .collect()
}

fn convert(like: &Object, v: &Value) -> R<Object> {
    let mut cs = cats(like);
    let grow = !is_protected(like);
    if let Some(o) = obj_of(v) {
        if o.class != "categorical" {
            return Err(err(
                "MATLAB:categorical:InvalidConversion",
                format!("Cannot combine a categorical array with a {}.", o.class),
            ));
        }
        let labs: Vec<Option<String>> = texts(o)
            .into_iter()
            .map(|t| if t == UNDEF { None } else { Some(t) })
            .collect();
        // Keep unused categories of the other operand too.
        for c in cats(o) {
            if !cs.contains(&c) {
                if !grow {
                    return Err(err(
                        "MATLAB:categorical:ProtectedForCombination",
                        format!(
                            "Cannot combine categorical arrays: category '{}' is not in the protected categories.",
                            c
                        ),
                    ));
                }
                cs.push(c);
            }
        }
        let codes = encode(&labs, &mut cs, grow)?;
        return Ok(make(
            o.dims.clone(),
            codes,
            &cs,
            is_ordinal(like),
            is_protected(like),
        ));
    }
    let (dims, labs, _) = labels(v)?;
    let codes = encode(&labs, &mut cs, grow)?;
    Ok(make(dims, codes, &cs, is_ordinal(like), is_protected(like)))
}

/// Concatenate categoricals (categories are merged in order).
pub fn cat_objs(objs: Vec<Object>, d: usize) -> R<Object> {
    let mut like = objs[0].clone();
    for o in &objs[1..] {
        like = convert(o, &Value::Object(Box::new(like.clone())))
            .map(|_| like.clone())
            .unwrap_or(like);
        for c in cats(o) {
            let mut cs = cats(&like);
            if !cs.contains(&c) {
                if is_protected(&like) {
                    return Err(err(
                        "MATLAB:categorical:ProtectedForCombination",
                        format!(
                            "Cannot combine categorical arrays: category '{}' is not in the protected categories.",
                            c
                        ),
                    ));
                }
                cs.push(c);
                like.set("_cats", cellstr_col(&cs));
            }
        }
    }
    let conv = objs
        .iter()
        .map(|o| convert(&like, &Value::Object(Box::new(o.clone()))))
        .collect::<R<Vec<_>>>()?;
    super::ew_cat(&like, conv, d)
}

pub fn equal(x: &Object, y: &Object) -> bool {
    x.dims == y.dims && texts(x) == texts(y)
}

fn binary(op: BinOp, a: &Value, b: &Value) -> R<Value> {
    let like = obj_of(if class_is(a, "categorical") { a } else { b }).unwrap();
    let mut probe = like.clone();
    probe.set("_prot", Value::bool(false));
    let x = convert(&probe, a)?;
    let y = convert(&x, b)?;
    let x = convert(&y, a)?;
    let (xc, yc) = (data(&x, "#c"), data(&y, "#c"));
    let ordinal = is_ordinal(like);
    let (dims, pairs) = {
        let ia: Vec<usize> = (0..xc.len()).collect();
        let ib: Vec<usize> = (0..yc.len()).collect();
        indy_value::ops::zip_bcast(&x.dims, &ia, &y.dims, &ib, |i, j| (i, j))?
    };
    let f: fn(f64, f64) -> bool = match op {
        BinOp::Eq => |p, q| p != 0.0 && p == q,
        BinOp::Ne => |p, q| p == 0.0 || q == 0.0 || p != q,
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge if !ordinal => {
            return Err(err(
                "MATLAB:categorical:NotOrdinal",
                "Relational comparisons are not allowed for categorical arrays that are not ordinal.",
            ));
        }
        BinOp::Lt => |p, q| p != 0.0 && q != 0.0 && p < q,
        BinOp::Le => |p, q| p != 0.0 && q != 0.0 && p <= q,
        BinOp::Gt => |p, q| p != 0.0 && q != 0.0 && p > q,
        BinOp::Ge => |p, q| p != 0.0 && q != 0.0 && p >= q,
        _ => return Err(super::undefined_op(op, a, b)),
    };
    Ok(bools(
        dims,
        pairs.iter().map(|&(i, j)| f(xc[i], yc[j])).collect(),
    ))
}

// ---------------------------------------------------------------------------
// Construction and functions
// ---------------------------------------------------------------------------

fn categorical_fn(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "categorical")?;
    let mut pos: Vec<Value> = Vec::new();
    let mut ord = false;
    let mut prot = false;
    let mut i = 0;
    while i < a.len() {
        let v = &a[i];
        if i >= 1
            && let Some(s) = v.to_str()
            && v.is_text()
            && matches!(s.to_ascii_lowercase().as_str(), "ordinal" | "protected")
            && i + 1 < a.len()
        {
            let on = a[i + 1].scalar_f64().unwrap_or(0.0) != 0.0;
            if s.eq_ignore_ascii_case("ordinal") {
                ord = on;
            } else {
                prot = on;
            }
            i += 2;
            continue;
        }
        pos.push(v.clone());
        i += 1;
    }
    let x = &pos[0];
    if let Some(o) = obj_of(x)
        && o.class == "categorical"
        && pos.len() == 1
    {
        let mut o = o.clone();
        if ord {
            o.set("_ord", Value::bool(true));
            o.set("_prot", Value::bool(true));
        }
        return Ok(vec![wrap(o)]);
    }
    let (dims, labs, nums) = labels(x)?;
    let (cats_v, codes) = match pos.get(1) {
        Some(vs) => {
            // Explicit value set: codes by matching values.
            let (_, vlabs, vnums) = labels(vs)?;
            let names: Vec<String> = match pos.get(2) {
                Some(n) => text_list(n).ok_or_else(|| {
                    err(
                        "MATLAB:categorical:InvalidNames",
                        "categorical: category names must be text.",
                    )
                })?,
                None => vlabs
                    .iter()
                    .map(|l| l.clone().unwrap_or_default())
                    .collect(),
            };
            if names.len() != vlabs.len() {
                return Err(err(
                    "MATLAB:categorical:NumNamesMismatch",
                    "categorical: the number of category names must match the number of values in the value set.",
                ));
            }
            let codes = match (&nums, &vnums) {
                (Some(xn), Some(vn)) => xn
                    .iter()
                    .map(|x| {
                        vn.iter()
                            .position(|v| v == x)
                            .map(|k| k as f64 + 1.0)
                            .unwrap_or(0.0)
                    })
                    .collect(),
                _ => labs
                    .iter()
                    .map(|l| match l {
                        None => 0.0,
                        Some(s) => vlabs
                            .iter()
                            .position(|v| v.as_deref() == Some(s.as_str()))
                            .map(|k| k as f64 + 1.0)
                            .unwrap_or(0.0),
                    })
                    .collect(),
            };
            let mut uniq = names.clone();
            uniq.dedup();
            (names, codes)
        }
        None => {
            // Categories: sorted unique values.
            let mut cs: Vec<String> = Vec::new();
            match &nums {
                Some(n) => {
                    let mut vals: Vec<f64> = n.iter().copied().filter(|x| !x.is_nan()).collect();
                    vals.sort_by(|p, q| p.partial_cmp(q).unwrap());
                    vals.dedup();
                    if matches!(x, Value::Bool(_)) {
                        cs = vals
                            .iter()
                            .map(|v| if *v != 0.0 { "true" } else { "false" }.to_string())
                            .collect();
                    } else {
                        cs = vals.iter().map(|v| num_label(*v)).collect();
                    }
                }
                None => {
                    for l in labs.iter().flatten() {
                        if !cs.contains(l) {
                            cs.push(l.clone());
                        }
                    }
                    cs.sort();
                }
            }
            let codes = encode(&labs, &mut cs, false)?;
            (cs, codes)
        }
    };
    Ok(vec![wrap(make(dims, codes, &cats_v, ord, prot))])
}

fn arg_cat<'a>(v: &'a Value, fname: &str) -> R<&'a Object> {
    match obj_of(v) {
        Some(o) if o.class == "categorical" => Ok(o),
        _ => Err(err(
            "MATLAB:categorical:InvalidInput",
            format!("{}: input must be a categorical array.", fname),
        )),
    }
}

fn categories_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "categories")?;
    let o = arg_cat(&a[0], "categories")?;
    let c = cats(o);
    if c.is_empty() {
        return Ok(vec![Value::Cell(Array::new(vec![0, 1], vec![]))]);
    }
    Ok(vec![cellstr_col(&c)])
}

fn countcats_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "countcats")?;
    let o = arg_cat(&a[0], "countcats")?;
    let k = cats(o).len();
    let codes = data(o, "#c");
    let d = o.dims.clone();
    let dim = match a.get(1).and_then(|v| v.scalar_f64()) {
        Some(x) => x as usize - 1,
        None => d.iter().position(|&n| n != 1).unwrap_or(0),
    };
    if d.len() == 2
        && (d[0] == 1 || d[1] == 1)
        && dim == d.iter().position(|&n| n != 1).unwrap_or(0)
    {
        let mut cnt = vec![0.0; k];
        for c in &codes {
            if *c >= 1.0 {
                cnt[*c as usize - 1] += 1.0;
            }
        }
        let dims = if d[0] == 1 { vec![1, k] } else { vec![k, 1] };
        return Ok(vec![Value::Num(Num::real(dims, cnt))]);
    }
    // Matrix: counts per column (dim 1) or per row (dim 2).
    let (r, c) = (d[0], d[1..].iter().product::<usize>());
    if dim == 0 {
        let mut out = vec![0.0; k * c];
        for j in 0..c {
            for i in 0..r {
                let code = codes[i + j * r];
                if code >= 1.0 {
                    out[(code as usize - 1) + j * k] += 1.0;
                }
            }
        }
        Ok(vec![Value::Num(Num::real(vec![k, c], out))])
    } else {
        let mut out = vec![0.0; r * k];
        for j in 0..c {
            for i in 0..r {
                let code = codes[i + j * r];
                if code >= 1.0 {
                    out[i + (code as usize - 1) * r] += 1.0;
                }
            }
        }
        Ok(vec![Value::Num(Num::real(vec![r, k], out))])
    }
}

fn iscategorical_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    Ok(vec![Value::bool(
        a.first().is_some_and(|v| class_is(v, "categorical")),
    )])
}

fn isundefined_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "isundefined")?;
    let o = arg_cat(&a[0], "isundefined")?;
    Ok(vec![bools(
        o.dims.clone(),
        data(o, "#c").iter().map(|c| *c == 0.0).collect(),
    )])
}

fn iscategory_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "iscategory")?;
    let o = arg_cat(&a[0], "iscategory")?;
    let c = cats(o);
    let names = text_list(&a[1]).unwrap_or_default();
    let n = names.len();
    Ok(vec![
        bools(
            if matches!(a[1], Value::Char(_)) {
                vec![1, 1]
            } else {
                a[1].dims()
            },
            names.iter().map(|s| c.contains(s)).collect::<Vec<_>>(),
        )
        .reshaped(if n == 1 { vec![1, 1] } else { a[1].dims() }),
    ])
}

fn isordinal_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "isordinal")?;
    Ok(vec![Value::bool(is_ordinal(arg_cat(&a[0], "isordinal")?))])
}

fn isprotected_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "isprotected")?;
    Ok(vec![Value::bool(is_protected(arg_cat(
        &a[0],
        "isprotected",
    )?))])
}

fn with_cats(o: &Object, cs: &[String], codes: Vec<f64>) -> Value {
    let mut n = o.clone();
    n.set("_cats", cellstr_col(cs));
    set_data(&mut n, "#c", codes);
    wrap(n)
}

fn addcats_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "addcats")?;
    let o = arg_cat(&a[0], "addcats")?;
    let mut cs = cats(o);
    let new = text_list(&a[1]).unwrap_or_default();
    for n in new {
        if cs.contains(&n) {
            return Err(err(
                "MATLAB:categorical:addcats:DuplicateCategory",
                format!("addcats: category '{}' already exists.", n),
            ));
        }
        cs.push(n);
    }
    Ok(vec![with_cats(o, &cs, data(o, "#c"))])
}

fn removecats_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "removecats")?;
    let o = arg_cat(&a[0], "removecats")?;
    let cs = cats(o);
    let codes = data(o, "#c");
    let drop: Vec<String> = match a.get(1) {
        Some(v) => text_list(v).unwrap_or_default(),
        None => cs
            .iter()
            .enumerate()
            .filter(|(k, _)| !codes.contains(&(*k as f64 + 1.0)))
            .map(|(_, c)| c.clone())
            .collect(),
    };
    let keep: Vec<String> = cs.iter().filter(|c| !drop.contains(c)).cloned().collect();
    let new_codes = codes
        .iter()
        .map(|&c| {
            if c == 0.0 {
                return 0.0;
            }
            let name = &cs[c as usize - 1];
            keep.iter()
                .position(|k| k == name)
                .map(|k| k as f64 + 1.0)
                .unwrap_or(0.0)
        })
        .collect();
    Ok(vec![with_cats(o, &keep, new_codes)])
}

fn renamecats_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "renamecats")?;
    let o = arg_cat(&a[0], "renamecats")?;
    let mut cs = cats(o);
    let (old, new) = if a.len() >= 3 {
        (
            text_list(&a[1]).unwrap_or_default(),
            text_list(&a[2]).unwrap_or_default(),
        )
    } else {
        (cs.clone(), text_list(&a[1]).unwrap_or_default())
    };
    if old.len() != new.len() {
        return Err(err(
            "MATLAB:categorical:renamecats:SizeMismatch",
            "renamecats: old and new names must have the same number of elements.",
        ));
    }
    for (o_, n) in old.iter().zip(&new) {
        let k = cs.iter().position(|c| c == o_).ok_or_else(|| {
            err(
                "MATLAB:categorical:renamecats:InvalidOldNames",
                format!("renamecats: '{}' is not a category.", o_),
            )
        })?;
        cs[k] = n.clone();
    }
    Ok(vec![with_cats(o, &cs, data(o, "#c"))])
}

fn reordercats_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "reordercats")?;
    let o = arg_cat(&a[0], "reordercats")?;
    let cs = cats(o);
    let order: Vec<String> = match a.get(1) {
        None => {
            let mut s = cs.clone();
            s.sort();
            s
        }
        Some(v) if v.is_numeric() => v
            .to_f64_vec()
            .unwrap_or_default()
            .iter()
            .map(|k| cs.get(*k as usize - 1).cloned().unwrap_or_default())
            .collect(),
        Some(v) => text_list(v).unwrap_or_default(),
    };
    let mut sorted_a = order.clone();
    sorted_a.sort();
    let mut sorted_b = cs.clone();
    sorted_b.sort();
    if sorted_a != sorted_b {
        return Err(err(
            "MATLAB:categorical:reordercats:InvalidNewOrder",
            "reordercats: the new order must contain exactly the existing categories.",
        ));
    }
    let codes = data(o, "#c")
        .iter()
        .map(|&c| {
            if c == 0.0 {
                0.0
            } else {
                order.iter().position(|n| *n == cs[c as usize - 1]).unwrap() as f64 + 1.0
            }
        })
        .collect();
    Ok(vec![with_cats(o, &order, codes)])
}

fn mergecats_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "mergecats")?;
    let o = arg_cat(&a[0], "mergecats")?;
    let cs = cats(o);
    let old = text_list(&a[1]).unwrap_or_default();
    let target = match a.get(2) {
        Some(v) => v.to_str().unwrap_or_default(),
        None => old.first().cloned().unwrap_or_default(),
    };
    let mut new_cats: Vec<String> = Vec::new();
    for c in &cs {
        let name = if old.contains(c) {
            target.clone()
        } else {
            c.clone()
        };
        if !new_cats.contains(&name) {
            new_cats.push(name);
        }
    }
    let codes = data(o, "#c")
        .iter()
        .map(|&c| {
            if c == 0.0 {
                return 0.0;
            }
            let n = &cs[c as usize - 1];
            let name = if old.contains(n) { &target } else { n };
            new_cats.iter().position(|x| x == name).unwrap() as f64 + 1.0
        })
        .collect();
    Ok(vec![with_cats(o, &new_cats, codes)])
}

fn setcats_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "setcats")?;
    let o = arg_cat(&a[0], "setcats")?;
    let cs = cats(o);
    let new = text_list(&a[1]).unwrap_or_default();
    let codes = data(o, "#c")
        .iter()
        .map(|&c| {
            if c == 0.0 {
                return 0.0;
            }
            new.iter()
                .position(|x| *x == cs[c as usize - 1])
                .map(|k| k as f64 + 1.0)
                .unwrap_or(0.0)
        })
        .collect();
    Ok(vec![with_cats(o, &new, codes)])
}

fn double_m(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let o = obj_of(&a[0]).unwrap();
    let v = data(o, "#c")
        .into_iter()
        .map(|c| if c == 0.0 { f64::NAN } else { c })
        .collect();
    Ok(vec![Value::Num(Num::real(o.dims.clone(), v))])
}

fn char_m(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let o = obj_of(&a[0]).unwrap();
    let t = texts(o);
    if t.len() == 1 {
        return Ok(vec![Value::str(&t[0])]);
    }
    let w = t.iter().map(|s| s.chars().count()).max().unwrap_or(0);
    let n = t.len();
    let mut d = vec![' '; n * w];
    for (i, s) in t.iter().enumerate() {
        for (j, c) in s.chars().enumerate() {
            d[i + j * n] = c;
        }
    }
    Ok(vec![Value::Char(Array::new(vec![n, w], d))])
}

fn string_m(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let o = obj_of(&a[0]).unwrap();
    Ok(vec![Value::Str(Array::new(
        o.dims.clone(),
        texts(o)
            .into_iter()
            .map(|t| if t == UNDEF { None } else { Some(t) })
            .collect(),
    ))])
}

fn cellstr_m(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let o = obj_of(&a[0]).unwrap();
    Ok(vec![Value::Cell(Array::new(
        o.dims.clone(),
        texts(o).iter().map(|s| Value::str(s)).collect(),
    ))])
}

fn ismember_m(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "ismember")?;
    let x: Vec<String> = match obj_of(&a[0]) {
        Some(o) if o.class == "categorical" => texts(o),
        _ => text_list(&a[0]).unwrap_or_default(),
    };
    let set: Vec<String> = match obj_of(&a[1]) {
        Some(o) if o.class == "categorical" => texts(o),
        _ => text_list(&a[1]).unwrap_or_default(),
    };
    let dims = a[0].dims();
    let dims = if matches!(a[0], Value::Char(_)) {
        vec![1, 1]
    } else {
        dims
    };
    let tf: Vec<bool> = x.iter().map(|s| s != UNDEF && set.contains(s)).collect();
    let loc: Vec<f64> = x
        .iter()
        .map(|s| {
            set.iter()
                .position(|t| t == s)
                .map(|k| k as f64 + 1.0)
                .unwrap_or(0.0)
        })
        .collect();
    Ok(vec![
        bools(dims.clone(), tf),
        Value::Num(Num::real(dims, loc)),
    ])
}

fn sort_m(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let o = obj_of(&a[0]).unwrap();
    let desc = a.iter().skip(1).any(|v| {
        v.to_str()
            .is_some_and(|s| s.eq_ignore_ascii_case("descend"))
    });
    let codes = data(o, "#c");
    let mut idx: Vec<usize> = (0..codes.len()).collect();
    let key = |c: f64| if c == 0.0 { f64::INFINITY } else { c };
    idx.sort_by(|&i, &j| {
        let o = key(codes[i]).partial_cmp(&key(codes[j])).unwrap();
        if desc { o.reverse() } else { o }
    });
    let mut n = o.clone();
    set_data(&mut n, "#c", idx.iter().map(|&i| codes[i]).collect());
    let mut out = vec![wrap(n)];
    if nargout > 1 {
        out.push(Value::Num(Num::real(
            o.dims.clone(),
            idx.iter().map(|&i| (i + 1) as f64).collect(),
        )));
    }
    Ok(out)
}

fn unique_m(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let o = obj_of(&a[0]).unwrap();
    let codes = data(o, "#c");
    let mut u: Vec<f64> = codes.iter().copied().filter(|c| *c != 0.0).collect();
    u.sort_by(|p, q| p.partial_cmp(q).unwrap());
    u.dedup();
    if codes.contains(&0.0) {
        // Each undefined element is its own value, listed last.
        let n_undef = codes.iter().filter(|c| **c == 0.0).count();
        u.extend(std::iter::repeat_n(0.0, n_undef));
    }
    let n = u.len();
    let dims = if o.dims[0] == 1 && o.dims.len() == 2 {
        vec![1, n]
    } else {
        vec![n, 1]
    };
    let mut r = o.clone();
    r.dims = dims.clone();
    set_data(&mut r, "#c", u.clone());
    let mut out = vec![wrap(r)];
    if nargout > 1 {
        let first: Vec<f64> = u
            .iter()
            .map(|c| {
                codes
                    .iter()
                    .position(|x| x == c)
                    .map(|k| k as f64 + 1.0)
                    .unwrap_or(0.0)
            })
            .collect();
        out.push(super::num_col(first));
        let ic: Vec<f64> = codes
            .iter()
            .map(|c| {
                u.iter()
                    .position(|x| x == c)
                    .map(|k| k as f64 + 1.0)
                    .unwrap_or(0.0)
            })
            .collect();
        out.push(super::num_col(ic));
    }
    Ok(out)
}

fn summary_m(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let o = obj_of(&a[0]).unwrap();
    let cs = cats(o);
    let codes = data(o, "#c");
    let w = cs
        .iter()
        .map(|s| s.chars().count())
        .max()
        .unwrap_or(0)
        .max(11);
    let mut s = String::new();
    for (k, c) in cs.iter().enumerate() {
        let n = codes.iter().filter(|x| **x as usize == k + 1).count();
        s.push_str(&format!("     {:<w$}  {}\n", c, n, w = w));
    }
    let undef = codes.iter().filter(|x| **x == 0.0).count();
    if undef > 0 {
        s.push_str(&format!("     {:<w$}  {}\n", UNDEF, undef, w = w));
    }
    it.print(&s);
    Ok(vec![])
}

fn histcounts_m(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    let o = obj_of(&a[0]).unwrap();
    let k = cats(o).len();
    let mut cnt = vec![0.0; k];
    for c in data(o, "#c") {
        if c >= 1.0 {
            cnt[c as usize - 1] += 1.0;
        }
    }
    let _ = (it, n);
    Ok(vec![
        Value::Num(Num::real(vec![1, k], cnt)),
        Value::cell_row(cats(o).iter().map(|s| Value::str(s)).collect()),
    ])
}

const METHODS: &[(&str, BuiltinFn)] = &[
    ("double", double_m),
    ("char", char_m),
    ("string", string_m),
    ("cellstr", cellstr_m),
    ("ismember", ismember_m),
    ("sort", sort_m),
    ("unique", unique_m),
    ("summary", summary_m),
    ("histcounts", histcounts_m),
];

pub fn register(it: &mut Interp) {
    reg(it, "categorical", categorical_fn);
    reg(it, "categories", categories_f);
    reg(it, "countcats", countcats_f);
    reg(it, "iscategorical", iscategorical_f);
    reg(it, "isundefined", isundefined_f);
    reg(it, "iscategory", iscategory_f);
    reg(it, "isordinal", isordinal_f);
    reg(it, "isprotected", isprotected_f);
    reg(it, "addcats", addcats_f);
    reg(it, "removecats", removecats_f);
    reg(it, "renamecats", renamecats_f);
    reg(it, "reordercats", reordercats_f);
    reg(it, "mergecats", mergecats_f);
    reg(it, "setcats", setcats_f);
}
