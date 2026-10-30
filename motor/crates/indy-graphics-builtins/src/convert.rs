//! The `Value ↔ PropValue` adapter between the interpreter and the
//! graphics model.
//!
//! - real numeric → `Num` (double scalar) or `Array` (class kept);
//!   logical → the same with class double (the model has no logical class);
//! - char row / string scalar → `Str`; char matrix rows, cellstr and string
//!   arrays → `Cell`;
//! - handles cross as doubles ([`Handle::value`] / [`Handle::from_value`]);
//! - [`GraphicsError`] `{id, message}` → an interpreter error with that id.

use indy_graphics::{Array, GraphicsError, Handle, NumClass, PropValue};
use indy_interp::builtins::err;
use indy_interp::{R, Value};
use indy_value::{Num, NumClass as VClass};

pub fn gerr(e: GraphicsError) -> Box<indy_interp::Error> {
    err(&e.id, e.message)
}

fn class_in(c: VClass) -> NumClass {
    match c {
        VClass::Double => NumClass::Double,
        VClass::Single => NumClass::Single,
        VClass::Int8 => NumClass::Int8,
        VClass::Int16 => NumClass::Int16,
        VClass::Int32 => NumClass::Int32,
        VClass::Int64 => NumClass::Int64,
        VClass::UInt8 => NumClass::UInt8,
        VClass::UInt16 => NumClass::UInt16,
        VClass::UInt32 => NumClass::UInt32,
        VClass::UInt64 => NumClass::UInt64,
    }
}

fn class_out(c: NumClass) -> VClass {
    match c {
        NumClass::Double => VClass::Double,
        NumClass::Single => VClass::Single,
        NumClass::Int8 => VClass::Int8,
        NumClass::Int16 => VClass::Int16,
        NumClass::Int32 => VClass::Int32,
        NumClass::Int64 => VClass::Int64,
        NumClass::UInt8 => VClass::UInt8,
        NumClass::UInt16 => VClass::UInt16,
        NumClass::UInt32 => VClass::UInt32,
        NumClass::UInt64 => VClass::UInt64,
    }
}

fn numeric(dims: &[usize], data: Vec<f64>, class: NumClass) -> PropValue {
    if class == NumClass::Double && data.len() == 1 {
        return PropValue::Num(data[0]);
    }
    let mut dims = dims.to_vec();
    while dims.len() < 2 {
        dims.push(1);
    }
    PropValue::Array(Array { dims, data, class })
}

fn text_of(v: &Value) -> Option<String> {
    match v {
        Value::Char(a) if a.dims.first().copied().unwrap_or(0) <= 1 => {
            Some(a.data.iter().collect())
        }
        Value::Str(a) if a.data.len() == 1 => Some(a.data[0].clone().unwrap_or_default()),
        _ => None,
    }
}

/// One interpreter value as a graphics property value.
pub fn to_prop(v: &Value, fname: &str) -> R<PropValue> {
    let bad = |what: &str| err("MATLAB:hg:InvalidArgument", format!("{fname}: {what}"));
    match v {
        Value::Num(n) => {
            if n.im.as_ref().is_some_and(|im| im.iter().any(|x| *x != 0.0)) {
                return Err(bad("complex values are not supported"));
            }
            Ok(numeric(&n.dims, n.re.clone(), class_in(n.class)))
        }
        Value::Bool(a) => Ok(numeric(
            &a.dims,
            a.data.iter().map(|b| f64::from(u8::from(*b))).collect(),
            NumClass::Double,
        )),
        Value::Char(a) => {
            if a.dims.len() == 2 && a.dims[0] > 1 {
                Ok(PropValue::Cell(v.char_rows().unwrap_or_default()))
            } else {
                Ok(PropValue::Str(a.data.iter().collect()))
            }
        }
        Value::Str(a) if a.data.len() == 1 => {
            Ok(PropValue::Str(a.data[0].clone().unwrap_or_default()))
        }
        Value::Str(a) => Ok(PropValue::Cell(
            a.data
                .iter()
                .map(|s| s.clone().unwrap_or_default())
                .collect(),
        )),
        Value::Cell(c) => c
            .data
            .iter()
            .map(text_of)
            .collect::<Option<Vec<String>>>()
            .map(PropValue::Cell)
            .ok_or_else(|| bad("cell arguments must contain only text")),
        _ => Err(bad(&format!(
            "arguments of class {} are not supported",
            v.class_name()
        ))),
    }
}

pub fn to_props(a: &[Value], fname: &str) -> R<Vec<PropValue>> {
    a.iter().map(|v| to_prop(v, fname)).collect()
}

/// A graphics property value back as an interpreter value.
pub fn from_prop(p: PropValue) -> Value {
    match p {
        PropValue::Num(n) => Value::num(n),
        PropValue::Array(a) => {
            let class = class_out(a.class);
            Value::Num(Num::real(a.dims, a.data).with_class(class))
        }
        PropValue::Str(s) => Value::str(&s),
        PropValue::Cell(c) => Value::cell_row(c.iter().map(|s| Value::str(s)).collect()),
        PropValue::Handle(h) => Value::num(h.value()),
        PropValue::Handles(hs) => handles_value(&hs),
        PropValue::Bool(b) => Value::bool(b),
    }
}

/// A list of handles as a column of doubles (`[]` when empty).
pub fn handles_value(hs: &[Handle]) -> Value {
    if hs.is_empty() {
        return Value::empty();
    }
    Value::col(hs.iter().map(|h| h.value()).collect())
}

/// The handles in a numeric value (`None` if it is not numeric or holds a
/// non-integer).
pub fn handles_of(v: &Value) -> Option<Vec<Handle>> {
    if !matches!(v, Value::Num(_)) {
        return None;
    }
    v.to_f64_vec()?
        .into_iter()
        .map(Handle::from_value)
        .collect()
}

/// Property names whose value is a handle: a double crosses as `Handle`.
pub fn handle_prop(name: &str, v: PropValue) -> PropValue {
    let key = name.to_ascii_lowercase();
    match v {
        PropValue::Num(n) if matches!(key.as_str(), "parent" | "currentaxes") => {
            Handle::from_value(n).map_or(PropValue::Num(n), PropValue::Handle)
        }
        v => v,
    }
}

/// Numeric content of a property value, flattened.
pub fn numbers(p: &PropValue, fname: &str, what: &str) -> R<Vec<f64>> {
    p.numbers().ok_or_else(|| {
        err(
            "MATLAB:hg:InvalidArgument",
            format!("{fname}: {what} must be numeric"),
        )
    })
}

pub fn array(p: &PropValue, fname: &str, what: &str) -> R<Array> {
    p.as_array().ok_or_else(|| {
        err(
            "MATLAB:hg:InvalidArgument",
            format!("{fname}: {what} must be numeric"),
        )
    })
}

/// Text lines of a label/text argument (char, cellstr or a number).
pub fn lines(p: &PropValue, fname: &str) -> R<Vec<String>> {
    match p {
        PropValue::Str(s) => Ok(vec![s.clone()]),
        PropValue::Cell(c) => Ok(c.clone()),
        PropValue::Num(n) => Ok(vec![format_number(*n)]),
        _ => Err(err(
            "MATLAB:hg:InvalidArgument",
            format!("{fname}: the text must be a character vector or a cell array of them"),
        )),
    }
}

fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}
