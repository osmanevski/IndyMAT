//! Cells and structs: struct, cell, fieldnames, cellfun, arrayfun, …

use indy_value::index::{Sub, index_value};
use indy_value::{Array, Error, Num, StructArray, Value, concat, norm_dims, numel_of};

use super::{arg_bool, arg_str, dims_and_class, err, need, one, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "struct", struct_b);
    reg(it, "cell", cell);
    reg(it, "fieldnames", fieldnames);
    reg(it, "isfield", isfield);
    reg(it, "rmfield", rmfield);
    reg(it, "setfield", setfield);
    reg(it, "getfield", getfield);
    reg(it, "orderfields", orderfields);
    reg(it, "struct2cell", struct2cell);
    reg(it, "cell2struct", cell2struct);
    reg(it, "num2cell", num2cell);
    reg(it, "cell2mat", cell2mat);
    reg(it, "cellfun", cellfun);
    reg(it, "arrayfun", arrayfun);
    reg(it, "structfun", structfun);
    reg(it, "iscellstr", |_, a, _| {
        need(&a, 1, "iscellstr")?;
        one(Value::bool(is_cellstr(&a[0])))
    });
    reg(it, "cellstr", cellstr);
    reg(it, "numfields", |_, a, _| {
        need(&a, 1, "numfields")?;
        one(Value::num(match &a[0] {
            Value::Struct(s) => s.fields.len() as f64,
            _ => 0.0,
        }))
    });
}

pub fn is_cellstr(v: &Value) -> bool {
    match v {
        Value::Cell(c) => c
            .data
            .iter()
            .all(|x| matches!(x, Value::Char(a) if a.dims.len() == 2 && (a.dims[0] <= 1))),
        _ => false,
    }
}

/// Strings of a cellstr or a single char row.
pub fn strings_of(v: &Value, fname: &str) -> R<Vec<String>> {
    match v {
        Value::Cell(c) => c
            .data
            .iter()
            .map(|x| {
                x.to_str().ok_or_else(|| {
                    err(
                        "MATLAB:invalidType",
                        format!("{}: cell elements must be character vectors.", fname),
                    )
                })
            })
            .collect(),
        Value::Char(_) => Ok(v.char_rows().unwrap()),
        other => Err(err(
            "MATLAB:invalidType",
            format!("{}: expected text, got {}.", fname, other.class_name()),
        )),
    }
}

pub fn cellstr_value(strs: Vec<String>, col: bool) -> Value {
    let vals: Vec<Value> = strs.iter().map(|s| Value::str(s)).collect();
    if col {
        Value::cell_col(vals)
    } else {
        Value::cell_row(vals)
    }
}

fn struct_b(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    if a.is_empty() {
        return one(Value::Struct(StructArray::new_scalar()));
    }
    if a.len() == 1 {
        return match &a[0] {
            Value::Struct(_) => one(a[0].clone()),
            Value::Num(n) if n.re.is_empty() => {
                one(Value::Struct(StructArray::empty_with(vec![], vec![0, 0])))
            }
            Value::Object(o) => {
                let mut s = StructArray::new_scalar();
                for (k, v) in &o.props {
                    s.set(0, k, v.clone());
                }
                one(Value::Struct(s))
            }
            _ => Err(err(
                "MATLAB:struct:invalidArgs",
                "struct: arguments must be name/value pairs.",
            )),
        };
    }
    if !a.len().is_multiple_of(2) {
        return Err(err(
            "MATLAB:struct:FieldValuePairsRequired",
            "Field and value input arguments must come in pairs.",
        ));
    }
    let mut dims: Option<Vec<usize>> = None;
    for p in a.chunks(2) {
        if let Value::Cell(c) = &p[1]
            && c.data.len() != 1
        {
            match &dims {
                None => dims = Some(c.dims.clone()),
                Some(d) if d != &c.dims => {
                    return Err(err(
                        "MATLAB:struct:ArrayDimensionMismatch",
                        "Array dimensions of cell inputs must match.",
                    ));
                }
                _ => {}
            }
        }
    }
    let dims = dims.unwrap_or_else(|| vec![1, 1]);
    let n = numel_of(&dims);
    let mut fields = Vec::new();
    for p in a.chunks(2) {
        let name = arg_str(&p[0], "struct")?;
        if !fields.contains(&name) {
            fields.push(name);
        }
    }
    let mut s = StructArray::empty_with(fields, dims);
    for p in a.chunks(2) {
        let name = p[0].to_str().unwrap();
        let f = s.field_index(&name).unwrap();
        for i in 0..n {
            s.elems[i][f] = match &p[1] {
                Value::Cell(c) if c.data.len() == 1 => c.data[0].clone(),
                Value::Cell(c) => c.data[i].clone(),
                other => other.clone(),
            };
        }
    }
    one(Value::Struct(s))
}

fn cell(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let (d, _) = dims_and_class(&a, "cell")?;
    let n = numel_of(&d);
    one(Value::Cell(Array::new(d, vec![Value::empty(); n])))
}

fn fieldnames(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "fieldnames")?;
    match &a[0] {
        Value::Struct(s) => one(cellstr_value(s.fields.clone(), true)),
        Value::Object(_) | Value::HObject(_) if crate::classes::class_of(&a[0]).is_some() => {
            one(cellstr_value(
                crate::classes::public_props(&crate::classes::class_of(&a[0]).unwrap()),
                true,
            ))
        }
        Value::Object(o) => one(cellstr_value(
            o.props.iter().map(|p| p.0.clone()).collect(),
            true,
        )),
        Value::HObject(o) => one(cellstr_value(
            o.borrow()
                .props
                .iter()
                .filter(|p| !p.0.starts_with("__"))
                .map(|p| p.0.clone())
                .collect(),
            true,
        )),
        other => Err(err(
            "MATLAB:fieldnames:InvalidInput",
            format!(
                "Invalid input argument of type '{}'. Input must be a structure or a Java or COM object.",
                other.class_name()
            ),
        )),
    }
}

fn isfield(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "isfield")?;
    let has = |n: &str| match &a[0] {
        Value::Struct(s) => s.field_index(n).is_some(),
        _ => false,
    };
    match &a[1] {
        Value::Cell(c) => {
            let r: Vec<bool> = c
                .data
                .iter()
                .map(|v| v.to_str().is_some_and(|s| has(&s)))
                .collect();
            one(Value::Bool(Array::new(c.dims.clone(), r)))
        }
        v => one(Value::bool(v.to_str().is_some_and(|s| has(&s)))),
    }
}

fn rmfield(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "rmfield")?;
    let Value::Struct(s) = &a[0] else {
        return Err(err(
            "MATLAB:rmfield:invalidInput",
            "rmfield: first argument must be a struct.",
        ));
    };
    let mut s = s.clone();
    for name in strings_of(&a[1], "rmfield")? {
        if !s.remove_field(&name) {
            return Err(err(
                "MATLAB:rmfield:InvalidFieldname",
                format!("A field named '{}' doesn't exist.", name),
            ));
        }
    }
    one(Value::Struct(s))
}

fn setfield(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 3, "setfield")?;
    let mut slot = Some(a[0].clone());
    let mut ops = Vec::new();
    for v in &a[1..a.len() - 1] {
        match v {
            Value::Cell(c) => ops.push(crate::eval::EOp::Paren(c.data.clone())),
            other => ops.push(crate::eval::EOp::Field(arg_str(other, "setfield")?)),
        }
    }
    crate::assign::assign_path(&mut slot, &ops, a[a.len() - 1].clone())?;
    one(slot.unwrap_or_default())
}

fn getfield(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "getfield")?;
    let mut ops = Vec::new();
    for v in &a[1..] {
        match v {
            Value::Cell(c) => ops.push(crate::eval::EOp::Paren(c.data.clone())),
            other => ops.push(crate::eval::EOp::Field(arg_str(other, "getfield")?)),
        }
    }
    match crate::eval::read_path(&a[0], &ops)? {
        crate::eval::Got::V(v) => one(v.into_owned()),
        crate::eval::Got::List(l) => one(l.into_iter().next().unwrap_or_default()),
        crate::eval::Got::Call(..) => {
            Err(err("MATLAB:getfield:invalid", "getfield: invalid path."))
        }
    }
}

fn orderfields(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "orderfields")?;
    let Value::Struct(s) = &a[0] else {
        return Err(err(
            "MATLAB:orderfields:invalid",
            "orderfields: argument must be a struct.",
        ));
    };
    let mut order: Vec<usize> = (0..s.fields.len()).collect();
    match a.get(1) {
        None => order.sort_by(|&x, &y| s.fields[x].cmp(&s.fields[y])),
        Some(Value::Struct(t)) => {
            order = t.fields.iter().filter_map(|f| s.field_index(f)).collect();
        }
        Some(Value::Cell(_)) => {
            let names = strings_of(&a[1], "orderfields")?;
            order = names.iter().filter_map(|f| s.field_index(f)).collect();
        }
        Some(v) => {
            let p = v.to_f64_vec().unwrap_or_default();
            order = p.iter().map(|x| *x as usize - 1).collect();
        }
    }
    if order.len() != s.fields.len() {
        return Err(err(
            "MATLAB:orderfields:invalid",
            "orderfields: field lists do not match.",
        ));
    }
    let ns = StructArray {
        dims: s.dims.clone(),
        fields: order.iter().map(|&i| s.fields[i].clone()).collect(),
        elems: s
            .elems
            .iter()
            .map(|e| order.iter().map(|&i| e[i].clone()).collect())
            .collect(),
    };
    let mut out = vec![Value::Struct(ns)];
    if nargout > 1 {
        out.push(Value::col(order.iter().map(|&i| i as f64 + 1.0).collect()));
    }
    Ok(out)
}

fn struct2cell(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "struct2cell")?;
    let Value::Struct(s) = &a[0] else {
        return Err(err(
            "MATLAB:struct2cell:invalid",
            "struct2cell: argument must be a struct.",
        ));
    };
    let nf = s.fields.len();
    let mut data = Vec::with_capacity(nf * s.numel());
    for e in &s.elems {
        data.extend(e.iter().cloned());
    }
    let mut dims = vec![nf];
    dims.extend(s.dims.iter().copied());
    // nf x dims(1) x dims(2): squeeze trailing singleton (1x1 struct -> nf x 1).
    one(Value::Cell(Array::new(norm_dims(dims), data)))
}

fn cell2struct(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "cell2struct")?;
    let Value::Cell(c) = &a[0] else {
        return Err(err(
            "MATLAB:cell2struct:invalid",
            "cell2struct: first argument must be a cell.",
        ));
    };
    let fields = strings_of(&a[1], "cell2struct")?;
    let dim = match a.get(2) {
        Some(v) => super::arg_usize(v, "cell2struct")?,
        None => 1,
    };
    let (r, k) = (c.dims[0], c.data.len() / c.dims[0].max(1));
    let (nf, n) = if dim == 1 { (r, k) } else { (k, r) };
    if nf != fields.len() {
        return Err(err(
            "MATLAB:cell2struct:InvalidNumberOfFields",
            "Number of field names must match the number of fields in the new structure.",
        ));
    }
    let mut s = StructArray::empty_with(fields, vec![n, 1]);
    for i in 0..n {
        for f in 0..nf {
            let (row, col) = if dim == 1 { (f, i) } else { (i, f) };
            s.elems[i][f] = c.data[row + col * r].clone();
        }
    }
    one(Value::Struct(s))
}

fn num2cell(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "num2cell")?;
    let v = &a[0];
    let n = v.numel();
    let mut data = Vec::with_capacity(n);
    for i in 0..n {
        data.push(match v {
            Value::Cell(c) => Value::Cell(Array::scalar(c.data[i].clone())),
            _ => index_value(v, &[Sub::scalar(i)])?,
        });
    }
    one(Value::Cell(Array::new(v.dims(), data)))
}

fn cell2mat(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "cell2mat")?;
    let Value::Cell(c) = &a[0] else {
        return Err(err(
            "MATLAB:cell2mat:WrongInputType",
            "cell2mat: input must be a cell array.",
        ));
    };
    if c.data.is_empty() {
        return one(Value::empty());
    }
    if c.data
        .iter()
        .any(|v| matches!(v, Value::Cell(_) | Value::Struct(_)))
        && !c.data.iter().all(|v| matches!(v, Value::Struct(_)))
    {
        return Err(err(
            "MATLAB:cell2mat:UnsupportedCellContent",
            "Cannot support cell arrays containing cell arrays or objects.",
        ));
    }
    let rows = c.dims[0];
    let cols = c.data.len() / rows;
    let mut row_vals = Vec::with_capacity(rows);
    for i in 0..rows {
        let parts: Vec<Value> = (0..cols).map(|j| c.data[i + j * rows].clone()).collect();
        row_vals.push(concat::cat(parts, 1)?);
    }
    one(concat::cat(row_vals, 0)?)
}

/// Split trailing `'UniformOutput', tf, 'ErrorHandler', h` options.
fn fun_options(it: &mut Interp, a: &mut Vec<Value>) -> R<(bool, Option<Value>)> {
    let mut uniform = true;
    let mut handler = None;
    loop {
        let n = a.len();
        if n < 3 {
            break;
        }
        let Some(name) = a[n - 2].to_str() else { break };
        let lname = name.to_ascii_lowercase();
        if lname == "uniformoutput" {
            uniform = arg_bool(&a[n - 1], "cellfun")?;
        } else if lname == "errorhandler" {
            handler = Some(a[n - 1].clone());
        } else {
            break;
        }
        a.truncate(n - 2);
    }
    let _ = it;
    Ok((uniform, handler))
}

fn collect_uniform(
    results: Vec<Vec<Value>>,
    nout: usize,
    dims: &[usize],
    fname: &str,
) -> R<Vec<Value>> {
    let n = numel_of(dims);
    let mut outs = Vec::with_capacity(nout);
    for k in 0..nout {
        let mut all_bool = n > 0;
        let mut all_char = n > 0;
        let mut data = Vec::with_capacity(n);
        let mut im: Option<Vec<f64>> = None;
        let mut class = None;
        for (i, r) in results.iter().enumerate() {
            let v = &r[k];
            if v.numel() != 1 {
                return Err(err(
                    "MATLAB:cellfun:NotAScalarOutput",
                    format!(
                        "{}: non-scalar in Uniform output, at index {}, output {}. Set 'UniformOutput' to false.",
                        fname,
                        i + 1,
                        k + 1
                    ),
                ));
            }
            all_bool &= match v {
                Value::Bool(_) => true,
                Value::Sparse(s) => s.logical,
                _ => false,
            };
            all_char &= matches!(v, Value::Char(_));
            let num = v.to_num().ok_or_else(|| {
                err(
                    "MATLAB:cellfun:NotAScalarOutput",
                    format!("{}: all outputs must be scalar numeric, logical or char for uniform output.", fname),
                )
            })?;
            if class.is_none() {
                class = Some(num.class);
            }
            data.push(num.re[0]);
            if let Some(ii) = &num.im {
                im.get_or_insert_with(|| vec![0.0; n])[i] = ii[0];
            }
        }
        let v = if all_bool {
            Value::Bool(Array::new(
                dims.to_vec(),
                data.iter().map(|x| *x != 0.0).collect(),
            ))
        } else if all_char {
            Value::Char(Array::new(
                dims.to_vec(),
                data.iter()
                    .map(|x| char::from_u32(*x as u32).unwrap_or('\0'))
                    .collect(),
            ))
        } else {
            let mut nm = match im {
                Some(im) => Num::complex(dims.to_vec(), data, im),
                None => Num::real(dims.to_vec(), data),
            };
            if let Some(c) = class {
                nm = nm.with_class(c);
            }
            Value::Num(nm)
        };
        outs.push(v);
    }
    Ok(outs)
}

fn apply_fun(
    it: &mut Interp,
    f: &Value,
    inputs: &[Vec<Value>],
    dims: &[usize],
    nargout: usize,
    uniform: bool,
    handler: Option<Value>,
    fname: &str,
) -> R<Vec<Value>> {
    let n = numel_of(dims);
    let nout = nargout.max(1);
    let mut results: Vec<Vec<Value>> = Vec::with_capacity(n);
    for i in 0..n {
        let args: Vec<Value> = inputs.iter().map(|c| c[i].clone()).collect();
        let r = match it.call_value(f, args.clone(), nargout) {
            Ok(r) => r,
            Err(e) => match &handler {
                Some(h) => {
                    let mut s = StructArray::new_scalar();
                    s.set(0, "message", Value::str(&e.message));
                    s.set(0, "identifier", Value::str(&e.identifier));
                    s.set(0, "index", Value::num(i as f64 + 1.0));
                    let mut hargs = vec![Value::Struct(s)];
                    hargs.extend(args);
                    it.call_value(h, hargs, nargout)?
                }
                None => return Err(e),
            },
        };
        if nargout == 0 && r.is_empty() && uniform {
            results.push(vec![]);
            continue;
        }
        if r.len() < nout {
            if nargout == 0 && uniform {
                results.push(vec![]);
                continue;
            }
            return Err(err(
                "MATLAB:cellfun:TooFewOutputs",
                format!("{}: function returned fewer outputs than requested.", fname),
            ));
        }
        results.push(r);
    }
    if nargout == 0 && results.iter().all(|r| r.is_empty()) {
        return Ok(vec![]);
    }
    if uniform {
        if results.iter().any(|r| r.is_empty()) {
            return Err(err(
                "MATLAB:cellfun:NotAScalarOutput",
                format!("{}: function returned no value for uniform output.", fname),
            ));
        }
        return collect_uniform(results, nout, dims, fname);
    }
    let mut outs = Vec::with_capacity(nout);
    for k in 0..nout {
        let data: Vec<Value> = results.iter().map(|r| r[k].clone()).collect();
        outs.push(Value::Cell(Array::new(dims.to_vec(), data)));
    }
    Ok(outs)
}

fn legacy_cellfun(name: &str, c: &Array<Value>, extra: Option<&Value>) -> Option<R<Value>> {
    let map_bool = |f: &dyn Fn(&Value) -> bool| {
        Value::Bool(Array::new(c.dims.clone(), c.data.iter().map(f).collect()))
    };
    let map_num = |f: &dyn Fn(&Value) -> f64| {
        Value::Num(Num::real(c.dims.clone(), c.data.iter().map(f).collect()))
    };
    Some(Ok(match name {
        "isempty" => map_bool(&|v| v.is_empty()),
        "islogical" => map_bool(&|v| v.class_name() == "logical"),
        "isnumeric" => map_bool(&Value::is_numeric),
        "isreal" => map_bool(&|v| match v {
            Value::Num(n) => n.im.is_none(),
            Value::Sparse(s) => s.im.is_none(),
            _ => true,
        }),
        "length" => map_num(&|v| {
            if v.is_empty() {
                0.0
            } else {
                *v.dims_ref().iter().max().unwrap() as f64
            }
        }),
        "ndims" => map_num(&|v| v.dims_ref().len() as f64),
        "numel" | "prodofsize" => map_num(&|v| v.numel() as f64),
        "size" => {
            let k = extra.and_then(|v| v.scalar_f64()).unwrap_or(1.0) as usize;
            map_num(&|v| *v.dims_ref().get(k.saturating_sub(1)).unwrap_or(&1) as f64)
        }
        "isclass" => {
            let cls = extra.and_then(|v| v.to_str()).unwrap_or_default();
            map_bool(&|v| v.class_name() == cls)
        }
        _ => return None,
    }))
}

fn cellfun(it: &mut Interp, mut a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "cellfun")?;
    let (uniform, handler) = fun_options(it, &mut a)?;
    let f = a.remove(0);
    if let Value::Char(_) = &f {
        let name = f.to_str().unwrap();
        if let Value::Cell(c) = &a[0]
            && let Some(r) = legacy_cellfun(&name, c, a.get(1))
        {
            return one(r?);
        }
    }
    let mut inputs = Vec::new();
    let mut dims = None;
    for v in &a {
        let Value::Cell(c) = v else {
            return Err(err(
                "MATLAB:cellfun:NotACell",
                "cellfun: all the input arguments must be cell arrays.",
            ));
        };
        match &dims {
            None => dims = Some(c.dims.clone()),
            Some(d) if d != &c.dims => {
                return Err(err(
                    "MATLAB:cellfun:ArgumentDimensionsMismatch",
                    "cellfun: all the input arguments must have the same size.",
                ));
            }
            _ => {}
        }
        inputs.push(c.data.clone());
    }
    let dims = dims.unwrap_or_else(|| vec![0, 0]);
    apply_fun(it, &f, &inputs, &dims, nargout, uniform, handler, "cellfun")
}

fn elements(v: &Value) -> R<Vec<Value>> {
    let n = v.numel();
    if v.is_sparse() && n > indy_value::sparse::DENSE_LIMIT {
        // The list below would need every element, zeros included.
        return Err(err(
            "MATLAB:nomem",
            "Out of memory: arrayfun needs every element of this sparse matrix.",
        ));
    }
    let mut out = Vec::with_capacity(n);
    match v {
        Value::Func(_) | Value::Object(_) => out.push(v.clone()),
        Value::Num(x) if x.im.is_none() => {
            for i in 0..n {
                out.push(Value::Num(Num {
                    class: x.class,
                    dims: vec![1, 1],
                    re: vec![x.re[i]],
                    im: None,
                }));
            }
        }
        _ => {
            for i in 0..n {
                out.push(index_value(v, &[Sub::scalar(i)])?);
            }
        }
    }
    Ok(out)
}

fn arrayfun(it: &mut Interp, mut a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "arrayfun")?;
    let (uniform, handler) = fun_options(it, &mut a)?;
    let f = a.remove(0);
    let mut inputs = Vec::new();
    let mut dims: Option<Vec<usize>> = None;
    for v in &a {
        let d = v.dims();
        match &dims {
            None => dims = Some(d),
            Some(x) if x != &d => {
                return Err(err(
                    "MATLAB:arrayfun:ArgumentDimensionsMismatch",
                    "arrayfun: all the input arguments must have the same size.",
                ));
            }
            _ => {}
        }
        inputs.push(elements(v)?);
    }
    let dims = dims.unwrap_or_else(|| vec![0, 0]);
    apply_fun(
        it, &f, &inputs, &dims, nargout, uniform, handler, "arrayfun",
    )
}

fn structfun(it: &mut Interp, mut a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "structfun")?;
    let (uniform, handler) = fun_options(it, &mut a)?;
    let f = a[0].clone();
    let Value::Struct(s) = &a[1] else {
        return Err(err(
            "MATLAB:structfun:NotAScalarStructure",
            "structfun: second argument must be a scalar struct.",
        ));
    };
    if s.numel() != 1 {
        return Err(err(
            "MATLAB:structfun:NotAScalarStructure",
            "structfun: second argument must be a scalar struct.",
        ));
    }
    let vals = s.elems[0].clone();
    let n = vals.len();
    let out = apply_fun(
        it,
        &f,
        &[vals],
        &[n, 1],
        nargout,
        uniform,
        handler,
        "structfun",
    )?;
    if !uniform {
        // Non-uniform structfun returns a struct with the same fields.
        let mut res = Vec::new();
        for o in out {
            let Value::Cell(c) = o else { continue };
            let mut ns = StructArray::empty_with(s.fields.clone(), vec![1, 1]);
            ns.elems[0] = c.data;
            res.push(Value::Struct(ns));
        }
        return Ok(res);
    }
    Ok(out)
}

fn cellstr(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "cellstr")?;
    match &a[0] {
        Value::Cell(_) => {
            if is_cellstr(&a[0]) {
                one(a[0].clone())
            } else {
                Err(err(
                    "MATLAB:cellstr:InputClass",
                    "Input must be a string, a character array, or a cell array of character vectors.",
                ))
            }
        }
        Value::Char(_) => {
            let rows = a[0].char_rows().unwrap();
            let rows: Vec<String> = rows.into_iter().map(|r| r.trim_end().to_string()).collect();
            if rows.is_empty() {
                return one(Value::cell_col(vec![Value::str("")]));
            }
            one(cellstr_value(rows, true))
        }
        _ => Err(Error::new(
            "MATLAB:cellstr:InputClass",
            "Input must be a string, a character array, or a cell array of character vectors.",
        )),
    }
}
