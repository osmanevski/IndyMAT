//! MATLAB datatypes built into the engine as native objects: `table`,
//! `datetime`, `duration`, `calendarDuration` and `categorical`.
//!
//! A native object is a `Value::Object` without classdef metadata whose
//! class is one of [`CLASSES`]. Its storage lives in the object's props:
//!
//! - element-wise classes (everything but `table`) keep one array per
//!   `#`-prop, each with the object's `dims` (`#v` serial values,
//!   `#c` category codes, ...); `_`-props hold hidden metadata and plain
//!   props (`Format`, `TimeZone`) are public properties;
//! - `table` keeps its variables and metadata in `_`-props (see `table.rs`).
//!
//! The interpreter calls into this module from its indexing, assignment,
//! operator, concatenation, display and method-dispatch paths; methods of
//! these classes win over same-named path functions, as for `string`.

pub mod categorical;
pub mod table;
pub mod tableio;
pub mod time;

use std::collections::HashMap;
use std::sync::OnceLock;

use indy_syntax::ast::{BinOp, UnOp};
use indy_value::index::{Sub, assign_index, delete_index, grow, index_value, write_positions};
use indy_value::{Array, Error, Num, Object, Value, concat, ops};

use super::{err, reg};
use crate::eval::{EOp, subs_of};
use crate::{BuiltinFn, Interp, R};

pub const CLASSES: &[&str] = &[
    "table",
    "datetime",
    "duration",
    "calendarDuration",
    "categorical",
];

/// Per-class behaviour.
pub struct ClassOps {
    pub name: &'static str,
    /// Read a public property (`x.Format`).
    pub get: fn(&Object, &str) -> R<Value>,
    /// Write a public property; validates the value.
    pub set: fn(&mut Object, &str, Value) -> R<()>,
    /// Convert `v` to this class, taking metadata from `like` (used by
    /// indexed assignment and concatenation).
    pub convert: fn(&Object, &Value) -> R<Object>,
    pub binary: fn(BinOp, &Value, &Value) -> R<Value>,
    /// Text of `disp(x)`.
    pub disp: fn(&Object) -> String,
    /// Text of `x` (statement without semicolon); `None` uses the default
    /// `name =\n\n  <dims> <class>\n\n<disp>\n` layout.
    pub display: Option<fn(&str, &Object) -> String>,
    /// Value used when an element-wise array grows (`#`-prop name given).
    pub fill: fn(&str) -> f64,
    pub methods: &'static [(&'static str, BuiltinFn)],
}

fn ops_of(class: &str) -> Option<&'static ClassOps> {
    Some(match class {
        "table" => &table::OPS,
        "datetime" => &time::DATETIME,
        "duration" => &time::DURATION,
        "calendarDuration" => &time::CALDUR,
        "categorical" => &categorical::OPS,
        _ => return None,
    })
}

pub fn register(it: &mut Interp) {
    table::register(it);
    time::register(it);
    categorical::register(it);
    tableio::register(it);
    reg(it, "istabular", |_, a, _| {
        Ok(vec![Value::bool(
            a.first().is_some_and(|v| class_is(v, "table")),
        )])
    });
}

/// Builtins whose argument names the interpreter passes in
/// [`table::ArgNames`] (MATLAB's `inputname`).
pub fn wants_arg_names(name: &str) -> bool {
    matches!(name, "table" | "array2table" | "addvars")
}

/// Whether `v` is one of the engine's native datatypes objects.
pub fn is_native(v: &Value) -> bool {
    match v {
        Value::Object(o) => o.meta.is_none() && CLASSES.contains(&o.class.as_str()),
        _ => false,
    }
}

pub fn class_is(v: &Value, class: &str) -> bool {
    matches!(v, Value::Object(o) if o.meta.is_none() && o.class == class)
}

/// Names `exist` reports as class files (Octave keeps these classes in the
/// `datatypes` package as classdef files).
pub fn is_class_file(name: &str) -> bool {
    CLASSES.contains(&name) || name == "string"
}

fn obj(v: &Value) -> &Object {
    match v {
        Value::Object(o) => o,
        _ => unreachable!("native datatypes value expected"),
    }
}

pub fn obj_of(v: &Value) -> Option<&Object> {
    match v {
        Value::Object(o) if is_native(v) => Some(o),
        _ => None,
    }
}

fn into_obj(v: Value) -> Object {
    match v {
        Value::Object(o) => *o,
        _ => unreachable!("native datatypes value expected"),
    }
}

// ---------------------------------------------------------------------------
// Method table
// ---------------------------------------------------------------------------

type Table = HashMap<(&'static str, &'static str), BuiltinFn>;

fn method_table() -> &'static Table {
    static T: OnceLock<Table> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = Table::new();
        for c in CLASSES {
            let ops = ops_of(c).unwrap();
            if *c != "table" {
                for (n, f) in EW_METHODS {
                    t.insert((*c, *n), *f);
                }
            }
            for (n, f) in ops.methods {
                t.insert((*c, *n), *f);
            }
        }
        t
    })
}

/// The native method `name` for the first datatypes object among `args`.
pub fn method(name: &str, args: &[Value]) -> Option<BuiltinFn> {
    let t = method_table();
    for a in args {
        if let Value::Object(o) = a
            && is_native(a)
        {
            let key = CLASSES.iter().find(|c| **c == o.class).unwrap();
            if let Some((_, f)) = t.get_key_value(&(*key, name)).map(|(k, f)| (k, *f)) {
                return Some(f);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Interpreter entry points
// ---------------------------------------------------------------------------

/// `x(...)`, `x{...}`, `x.name` on a native object, then the remaining ops.
pub fn index(it: &mut Interp, v: Value, eops: &[EOp], nargout: usize) -> R<Vec<Value>> {
    let (vals, used) = step(&v, eops)?;
    if used >= eops.len() {
        return Ok(vals);
    }
    if vals.len() != 1 {
        return Err(Error::new(
            "MATLAB:index:expected_one_output_from_expression",
            format!(
                "Expected one output from a curly brace or dot indexing expression, but there were {} results.",
                vals.len()
            ),
        ));
    }
    let next = vals.into_iter().next().unwrap();
    it.apply_eops(next, &eops[used..], nargout)
}

/// Dims of the value selected by `eops` (for `end`); best effort.
pub fn peek_dims(v: &Value, eops: &[EOp]) -> Vec<usize> {
    let mut cur = v.clone();
    let mut rest = eops;
    while !rest.is_empty() {
        if is_native(&cur) {
            match step(&cur, rest) {
                Ok((vals, used)) if vals.len() == 1 => {
                    cur = vals.into_iter().next().unwrap();
                    rest = &rest[used..];
                }
                _ => return cur.dims(),
            }
        } else {
            return match crate::eval::read_path(&cur, rest) {
                Ok(crate::eval::Got::V(x)) => x.dims(),
                _ => cur.dims(),
            };
        }
    }
    cur.dims()
}

/// One indexing step without the interpreter: values and ops consumed.
fn step(v: &Value, eops: &[EOp]) -> R<(Vec<Value>, usize)> {
    let o = obj(v);
    if o.class == "table" {
        return table::step(o, eops);
    }
    let ops = ops_of(&o.class).unwrap();
    match &eops[0] {
        EOp::Paren(args) => {
            let subs = subs_of(args)?;
            Ok((vec![Value::Object(Box::new(ew_index(o, &subs)?))], 1))
        }
        EOp::Field(name) => Ok((vec![(ops.get)(o, name)?], 1)),
        EOp::Brace(_) => Err(Error::new(
            "MATLAB:cellRefFromNonCell",
            format!(
                "Brace indexing is not supported for variables of this type ({}).",
                o.class
            ),
        )),
    }
}

/// `x(...)... = rhs` / `x.name... = rhs` on a native object.
pub fn assign(v: Value, eops: &[EOp], rhs: Value) -> R<Value> {
    let o = into_obj(v);
    if o.class == "table" {
        return table::assign(o, eops, rhs).map(|t| Value::Object(Box::new(t)));
    }
    let ops = ops_of(&o.class).unwrap();
    match &eops[0] {
        EOp::Field(name) => {
            let mut o = o;
            let newval = if eops.len() == 1 {
                rhs
            } else {
                let mut slot = Some((ops.get)(&o, name)?);
                crate::assign::assign_path(&mut slot, &eops[1..], rhs)?;
                slot.unwrap_or_default()
            };
            (ops.set)(&mut o, name, newval)?;
            Ok(Value::Object(Box::new(o)))
        }
        EOp::Paren(args) => {
            if eops.len() > 1 {
                return Err(err(
                    "MATLAB:index:multipleTargets",
                    format!(
                        "Nested indexed assignment into a {} array is not supported.",
                        o.class
                    ),
                ));
            }
            let subs = subs_of(args)?;
            if matches!(&rhs, Value::Num(n) if n.re.is_empty() && n.dims == [0, 0]) {
                return Ok(Value::Object(Box::new(ew_delete(o, &subs)?)));
            }
            let r = (ops.convert)(&o, &rhs)?;
            let mut n = ew_assign(o, &subs, &r, ops)?;
            if let Some(c) = r.get("_cats") {
                n.set("_cats", c.clone());
            }
            Ok(Value::Object(Box::new(n)))
        }
        EOp::Brace(_) => Err(Error::new(
            "MATLAB:cellAssToNonCell",
            format!(
                "Unable to perform assignment because brace indexing is not supported for variables of this type ({}).",
                o.class
            ),
        )),
    }
}

pub fn binary(op: BinOp, a: &Value, b: &Value) -> R<Value> {
    // string + datetime/duration/categorical concatenates their text.
    if op == BinOp::Add && (matches!(a, Value::Str(_)) || matches!(b, Value::Str(_))) {
        let as_str = |v: &Value| -> Value {
            match obj_of(v) {
                Some(o) if o.class != "table" => Value::Str(Array::new(
                    o.dims.clone(),
                    elem_texts(o)
                        .into_iter()
                        .map(|t| {
                            if t == "NaT" || t == "<undefined>" {
                                None
                            } else {
                                Some(t)
                            }
                        })
                        .collect(),
                )),
                _ => v.clone(),
            }
        };
        if !(class_is(a, "table") || class_is(b, "table")) {
            return ops::binary(op, &as_str(a), &as_str(b));
        }
    }
    // MATLAB precedence: table, then datetime, then (calendar) durations.
    let order = [
        "table",
        "datetime",
        "calendarDuration",
        "duration",
        "categorical",
    ];
    for c in order {
        if class_is(a, c) || class_is(b, c) {
            return (ops_of(c).unwrap().binary)(op, a, b);
        }
    }
    Err(undefined_op(op, a, b))
}

pub fn undefined_op(op: BinOp, a: &Value, b: &Value) -> Box<Error> {
    Error::new(
        "MATLAB:UndefinedFunction",
        format!(
            "Operator '{}' is not supported for operands of type '{}' and '{}'.",
            op.text(),
            a.class_name(),
            b.class_name()
        ),
    )
}

pub fn unary(op: UnOp, a: &Value) -> R<Value> {
    let o = obj(a);
    match op {
        UnOp::Transpose | UnOp::CTranspose => {
            if o.class == "table" {
                return Err(err(
                    "MATLAB:table:UndefinedTransposeFunction",
                    "Transpose is not defined for tables; use rows2vars or table2array.",
                ));
            }
            let conj = op == UnOp::CTranspose;
            Ok(Value::Object(Box::new(ew_map(o, |d| {
                ops::transpose(&d, conj)
            })?)))
        }
        UnOp::Neg | UnOp::Plus if matches!(o.class.as_str(), "duration" | "calendarDuration") => {
            let neg = op == UnOp::Neg;
            Ok(Value::Object(Box::new(ew_map(o, |d| {
                if neg {
                    Ok(ops::unary(UnOp::Neg, &d)?)
                } else {
                    Ok(d)
                }
            })?)))
        }
        UnOp::Not if o.class == "table" => table::not(o),
        _ => Err(Error::new(
            "MATLAB:UndefinedFunction",
            format!(
                "Unary operator '{}' is not supported for type '{}'.",
                crate::classes::unop_method(op),
                o.class
            ),
        )),
    }
}

/// Concatenation `[a, b]` (d = 1) / `[a; b]` (d = 0) when any operand may
/// be a native object; plain values go to `concat::cat`.
pub fn cat(vals: Vec<Value>, d: usize) -> R<Value> {
    let Some(first) = vals.iter().find(|v| is_native(v)) else {
        return concat::cat(vals, d);
    };
    let class = obj(first).class.clone();
    if class == "table" {
        return table::cat(vals, d);
    }
    // datetime wins over duration and text; categorical over text.
    let lead = ["datetime", "calendarDuration", "duration", "categorical"]
        .into_iter()
        .find(|c| vals.iter().any(|v| class_is(v, c)))
        .unwrap_or(class.as_str())
        .to_string();
    let like = obj(vals.iter().find(|v| class_is(v, &lead)).unwrap()).clone();
    let ops = ops_of(&lead).unwrap();
    let mut objs = Vec::with_capacity(vals.len());
    for v in &vals {
        if matches!(v, Value::Num(n) if n.re.is_empty() && n.dims == [0, 0]) {
            continue;
        }
        objs.push((ops.convert)(&like, v)?);
    }
    if lead == "categorical" {
        return categorical::cat_objs(objs, d).map(|o| Value::Object(Box::new(o)));
    }
    Ok(Value::Object(Box::new(ew_cat(&like, objs, d)?)))
}

/// Full display text of `name = x`.
pub fn display_text(name: &str, v: &Value) -> String {
    let o = obj(v);
    let ops = ops_of(&o.class).unwrap();
    if let Some(f) = ops.display {
        return f(name, o);
    }
    let body = (ops.disp)(o);
    if o.dims.iter().product::<usize>() == 0 {
        return format!(
            "{} = \n\n  {} empty {} array\n\n",
            name,
            times_text(&o.dims),
            o.class
        );
    }
    if o.dims == [1, 1] {
        return format!("{} = \n\n  {}\n\n{}\n", name, o.class, body);
    }
    format!(
        "{} = \n\n  {} {} array\n\n{}\n",
        name,
        times_text(&o.dims),
        o.class,
        body
    )
}

/// `3×2` (MATLAB's size text in object displays).
pub fn times_text(d: &[usize]) -> String {
    d.iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join("×")
}

/// Display text of each element of an element-wise object.
pub fn elem_texts(o: &Object) -> Vec<String> {
    match o.class.as_str() {
        "categorical" => categorical::texts(o),
        _ => time::texts_of(o),
    }
}

// ---------------------------------------------------------------------------
// Element-wise arrays
// ---------------------------------------------------------------------------

pub fn new_obj(class: &str, dims: Vec<usize>) -> Object {
    let mut o = Object::new(class);
    o.dims = indy_value::norm_dims(dims);
    o
}

/// The `#`-prop `key` as an f64 vector (empty if absent).
pub fn data(o: &Object, key: &str) -> Vec<f64> {
    match o.get(key) {
        Some(Value::Num(n)) => n.re.clone(),
        Some(v) => v.to_f64_vec().unwrap_or_default(),
        None => Vec::new(),
    }
}

pub fn set_data(o: &mut Object, key: &str, v: Vec<f64>) {
    let d = o.dims.clone();
    o.set(key, Value::Num(Num::real(d, v)));
}

/// Apply `f` to every `#`-prop; dims follow the result.
pub fn ew_map(o: &Object, mut f: impl FnMut(Value) -> R<Value>) -> R<Object> {
    let mut n = o.clone();
    let mut dims = None;
    for (k, v) in n.props.iter_mut() {
        if k.starts_with('#') {
            *v = f(std::mem::take(v))?;
            dims = Some(v.dims());
        }
    }
    if let Some(d) = dims {
        n.dims = d;
    }
    Ok(n)
}

pub fn ew_index(o: &Object, subs: &[Sub]) -> R<Object> {
    ew_map(o, |d| index_value(&d, subs))
}

fn ew_delete(o: Object, subs: &[Sub]) -> R<Object> {
    ew_map(&o, |d| delete_index(d, subs))
}

fn ew_assign(o: Object, subs: &[Sub], r: &Object, ops: &ClassOps) -> R<Object> {
    let (_, nd) = write_positions(&o.dims, subs, &r.dims)?;
    let mut n = o;
    let old = n.dims.clone();
    for (k, v) in n.props.iter_mut() {
        if !k.starts_with('#') {
            continue;
        }
        let mut cur = std::mem::take(v);
        if nd != old
            && let Value::Num(x) = &mut cur
        {
            x.re = grow(std::mem::take(&mut x.re), &old, &nd, (ops.fill)(k));
            x.dims = nd.clone();
        }
        let rv = r.get(k).cloned().unwrap_or_default();
        let mut slot = Some(cur);
        assign_index(&mut slot, subs, rv)?;
        *v = slot.unwrap_or_default();
    }
    n.dims = nd;
    if let Some(Value::Num(x)) = n
        .props
        .iter()
        .find(|(k, _)| k.starts_with('#'))
        .map(|p| &p.1)
    {
        n.dims = x.dims.clone();
    }
    Ok(n)
}

pub fn ew_cat(like: &Object, objs: Vec<Object>, d: usize) -> R<Object> {
    let mut n = like.clone();
    if objs.is_empty() {
        return Ok(n);
    }
    let keys: Vec<String> = like
        .props
        .iter()
        .filter(|(k, _)| k.starts_with('#'))
        .map(|(k, _)| k.clone())
        .collect();
    let mut dims = None;
    for k in &keys {
        let parts: Vec<Value> = objs
            .iter()
            .map(|o| o.get(k).cloned().unwrap_or_default())
            .collect();
        let c = concat::cat(parts, d)?;
        dims = Some(c.dims());
        n.set(k, c);
    }
    if let Some(dm) = dims {
        n.dims = dm;
    }
    Ok(n)
}

/// Call the plain builtin `name` on each `#`-prop of the first native
/// argument (shape functions: reshape, repmat, flip, ...).
fn ew_call(it: &mut Interp, name: &str, a: Vec<Value>) -> R<Vec<Value>> {
    let Some(p) = a.iter().position(is_native) else {
        return Err(err(
            "MATLAB:invalidType",
            format!("{}: no object argument", name),
        ));
    };
    let f = *it
        .builtins
        .get(name)
        .ok_or_else(|| Interp::undefined(name))?;
    let o = obj(&a[p]).clone();
    let r = ew_map(&o, |d| {
        let mut args = a.clone();
        args[p] = d;
        f(it, args, 1)?
            .into_iter()
            .next()
            .ok_or_else(|| Error::msg(format!("{}: no result", name)))
    })?;
    Ok(vec![Value::Object(Box::new(r))])
}

macro_rules! ew_fns {
    ($($f:ident => $name:literal),* $(,)?) => {
        $(fn $f(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
            ew_call(it, $name, a)
        })*
    };
}

ew_fns!(
    ew_reshape => "reshape",
    ew_repmat => "repmat",
    ew_flip => "flip",
    ew_fliplr => "fliplr",
    ew_flipud => "flipud",
    ew_circshift => "circshift",
    ew_squeeze => "squeeze",
    ew_permute => "permute",
    ew_transpose_f => "transpose",
    ew_ctranspose_f => "ctranspose",
);

fn ew_cat_fn(d: Option<usize>) -> impl Fn(Vec<Value>) -> R<Vec<Value>> {
    move |mut a: Vec<Value>| {
        let d = match d {
            Some(d) => d,
            None => {
                let k = a.remove(0).scalar_f64().unwrap_or(1.0);
                (k.max(1.0) as usize) - 1
            }
        };
        Ok(vec![cat(a, d)?])
    }
}

fn ew_horzcat(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    ew_cat_fn(Some(1))(a)
}
fn ew_vertcat(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    ew_cat_fn(Some(0))(a)
}
fn ew_catn(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    ew_cat_fn(None)(a)
}

fn ew_disp(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let o = obj(&a[0]);
    let t = (ops_of(&o.class).unwrap().disp)(o);
    it.print(&t);
    Ok(vec![])
}

fn ew_display(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let t = display_text("ans", &a[0]);
    it.print(&t);
    Ok(vec![])
}

fn ew_properties(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let o = obj(&a[0]);
    let names: Vec<Value> = public_props(o).iter().map(|s| Value::str(s)).collect();
    if nargout == 0 {
        let mut t = format!("Properties for class {}:\n\n", o.class);
        for n in public_props(o) {
            t.push_str(&format!("    {}\n", n));
        }
        t.push('\n');
        it.print(&t);
        return Ok(vec![]);
    }
    Ok(vec![Value::cell_col(names)])
}

pub fn public_props(o: &Object) -> Vec<String> {
    if o.class == "table" {
        return vec!["Properties".into()];
    }
    o.props
        .iter()
        .filter(|(k, _)| !k.starts_with('#') && !k.starts_with('_'))
        .map(|(k, _)| k.clone())
        .collect()
}

fn ew_isobject(_: &mut Interp, _: Vec<Value>, _: usize) -> R<Vec<Value>> {
    Ok(vec![Value::bool(true)])
}

fn ew_isequal(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let ok = a.windows(2).all(|w| natives_equal(&w[0], &w[1], false));
    Ok(vec![Value::bool(ok)])
}

fn ew_isequaln(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let ok = a.windows(2).all(|w| natives_equal(&w[0], &w[1], true));
    Ok(vec![Value::bool(ok)])
}

/// `isequal` for datatypes values (also used for table variables).
pub fn natives_equal(a: &Value, b: &Value, nan_eq: bool) -> bool {
    let (Some(x), Some(y)) = (obj_of(a), obj_of(b)) else {
        if is_native(a) || is_native(b) {
            // datetime vs text and categorical vs text compare by value.
            let (n, other) = if is_native(a) { (a, b) } else { (b, a) };
            let o = obj(n);
            if let Some(ops) = ops_of(&o.class)
                && o.class != "table"
                && let Ok(c) = (ops.convert)(o, other)
            {
                return natives_equal(n, &Value::Object(Box::new(c)), nan_eq);
            }
            return false;
        }
        return super::isequal_values(a, b, nan_eq);
    };
    if x.class != y.class || x.dims != y.dims {
        return false;
    }
    match x.class.as_str() {
        "table" => table::equal(x, y, nan_eq),
        "categorical" => categorical::equal(x, y),
        _ => x
            .props
            .iter()
            .filter(|(k, _)| k.starts_with('#'))
            .all(|(k, v)| {
                y.get(k)
                    .is_some_and(|w| super::isequal_values(v, w, nan_eq))
            }),
    }
}

fn ew_size_like(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    // `length`, `numel` and `size` read dims; kept for completeness.
    Ok(vec![Value::num(a[0].numel() as f64)])
}

const EW_METHODS: &[(&str, BuiltinFn)] = &[
    ("reshape", ew_reshape),
    ("repmat", ew_repmat),
    ("flip", ew_flip),
    ("fliplr", ew_fliplr),
    ("flipud", ew_flipud),
    ("circshift", ew_circshift),
    ("squeeze", ew_squeeze),
    ("permute", ew_permute),
    ("transpose", ew_transpose_f),
    ("ctranspose", ew_ctranspose_f),
    ("horzcat", ew_horzcat),
    ("vertcat", ew_vertcat),
    ("cat", ew_catn),
    ("disp", ew_disp),
    ("display", ew_display),
    ("properties", ew_properties),
    ("isobject", ew_isobject),
    ("isequal", ew_isequal),
    ("isequaln", ew_isequaln),
    ("numel", ew_size_like),
];

// ---------------------------------------------------------------------------
// Small helpers shared by the class modules
// ---------------------------------------------------------------------------

/// Text items of a char row, char matrix (rows), cellstr or string array.
pub fn text_list(v: &Value) -> Option<Vec<String>> {
    match v {
        Value::Char(a) => {
            if a.dims.len() == 2 && a.dims[0] > 1 {
                return v.char_rows();
            }
            Some(vec![v.to_str().unwrap_or_default()])
        }
        Value::Str(a) => Some(
            a.data
                .iter()
                .map(|s| s.clone().unwrap_or_default())
                .collect(),
        ),
        Value::Cell(c) => c.data.iter().map(|x| x.to_str()).collect(),
        _ => None,
    }
}

pub fn cellstr_row(items: &[String]) -> Value {
    Value::cell_row(items.iter().map(|s| Value::str(s)).collect())
}

pub fn cellstr_col(items: &[String]) -> Value {
    Value::cell_col(items.iter().map(|s| Value::str(s)).collect())
}

/// Name/value options after `start`: lower-cased names.
pub fn name_values(a: &[Value], fname: &str) -> R<Vec<(String, Value)>> {
    if !a.len().is_multiple_of(2) {
        return Err(err(
            "MATLAB:invalidNameValue",
            format!("{}: name-value arguments must come in pairs.", fname),
        ));
    }
    a.chunks(2)
        .map(|p| {
            let n = p[0].to_str().ok_or_else(|| {
                err(
                    "MATLAB:invalidNameValue",
                    format!("{}: expected a parameter name.", fname),
                )
            })?;
            Ok((n, p[1].clone()))
        })
        .collect()
}

pub fn num_col(v: Vec<f64>) -> Value {
    let n = v.len();
    Value::Num(Num::real(vec![n, 1], v))
}

pub fn bools(dims: Vec<usize>, b: Vec<bool>) -> Value {
    Value::Bool(Array::new(dims, b))
}
