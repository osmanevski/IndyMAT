//! Value model of the IndyMAT engine.
//!
//! Every value is an N-d array stored column-major with MATLAB shape rules:
//! `dims` always has at least two entries and trailing singleton dimensions
//! beyond the second are dropped. Numbers are always `f64`; integer and
//! `single` classes keep their class tag and are rounded/saturated after
//! each operation (so `int64` beyond 2^53 loses precision — a documented
//! limit of phase 2).
//!
//! Char arrays hold Unicode scalar values (`char`), not UTF-16 code units:
//! a character outside the BMP is one element here and two in MATLAB.

#![allow(clippy::needless_range_loop, clippy::too_many_arguments)]

use std::fmt;
use std::rc::Rc;

pub mod cmat;
pub mod concat;
pub mod deflate;
pub mod format;
pub mod index;
pub mod ops;
pub mod sparse;

pub use indy_syntax::ast;

/// One entry of an error's call stack (innermost first).
#[derive(Debug, Clone, PartialEq)]
pub struct StackFrame {
    pub file: String,
    pub name: String,
    pub line: usize,
    pub column: usize,
}

/// Error raised by value operations; the interpreter adds the stack.
#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    pub identifier: String,
    pub message: String,
    pub stack: Vec<StackFrame>,
    /// The MException-like object when one was thrown explicitly
    /// (keeps `cause`, custom classes and extra properties).
    pub object: Option<Value>,
}

impl Error {
    pub fn new(identifier: &str, message: impl Into<String>) -> Box<Error> {
        Box::new(Error {
            identifier: identifier.to_string(),
            message: message.into(),
            stack: Vec::new(),
            object: None,
        })
    }
    pub fn msg(message: impl Into<String>) -> Box<Error> {
        Error::new("", message)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

pub type Res<T> = Result<T, Box<Error>>;

// ---------------------------------------------------------------------------
// Dimensions
// ---------------------------------------------------------------------------

/// Normalise a dims vector: at least 2 entries, no trailing 1s past the 2nd.
pub fn norm_dims(mut d: Vec<usize>) -> Vec<usize> {
    while d.len() < 2 {
        d.push(if d.is_empty() { 0 } else { 1 });
    }
    while d.len() > 2 && *d.last().unwrap() == 1 {
        d.pop();
    }
    d
}

pub fn numel_of(d: &[usize]) -> usize {
    d.iter().product()
}

/// Generic dense column-major array.
#[derive(Clone, Debug, PartialEq)]
pub struct Array<T> {
    pub dims: Vec<usize>,
    pub data: Vec<T>,
}

impl<T: Clone> Array<T> {
    pub fn new(dims: Vec<usize>, data: Vec<T>) -> Self {
        let dims = norm_dims(dims);
        debug_assert_eq!(numel_of(&dims), data.len());
        Array { dims, data }
    }
    pub fn scalar(v: T) -> Self {
        Array {
            dims: vec![1, 1],
            data: vec![v],
        }
    }
    pub fn filled(dims: Vec<usize>, v: T) -> Self {
        let dims = norm_dims(dims);
        let n = numel_of(&dims);
        Array {
            dims,
            data: vec![v; n],
        }
    }
    pub fn row(data: Vec<T>) -> Self {
        Array {
            dims: vec![1, data.len()],
            data,
        }
    }
    pub fn col(data: Vec<T>) -> Self {
        Array {
            dims: vec![data.len(), 1],
            data,
        }
    }
    pub fn numel(&self) -> usize {
        self.data.len()
    }
    pub fn map<U: Clone>(&self, f: impl Fn(&T) -> U) -> Array<U> {
        Array {
            dims: self.dims.clone(),
            data: self.data.iter().map(f).collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Numeric arrays
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumClass {
    Double,
    Single,
    Int8,
    Int16,
    Int32,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
}

impl NumClass {
    pub fn name(self) -> &'static str {
        match self {
            NumClass::Double => "double",
            NumClass::Single => "single",
            NumClass::Int8 => "int8",
            NumClass::Int16 => "int16",
            NumClass::Int32 => "int32",
            NumClass::Int64 => "int64",
            NumClass::UInt8 => "uint8",
            NumClass::UInt16 => "uint16",
            NumClass::UInt32 => "uint32",
            NumClass::UInt64 => "uint64",
        }
    }
    pub fn from_name(s: &str) -> Option<NumClass> {
        Some(match s {
            "double" => NumClass::Double,
            "single" => NumClass::Single,
            "int8" => NumClass::Int8,
            "int16" => NumClass::Int16,
            "int32" => NumClass::Int32,
            "int64" => NumClass::Int64,
            "uint8" => NumClass::UInt8,
            "uint16" => NumClass::UInt16,
            "uint32" => NumClass::UInt32,
            "uint64" => NumClass::UInt64,
            _ => return None,
        })
    }
    pub fn is_int(self) -> bool {
        !matches!(self, NumClass::Double | NumClass::Single)
    }
    pub fn int_range(self) -> (f64, f64) {
        match self {
            NumClass::Int8 => (-128.0, 127.0),
            NumClass::Int16 => (-32768.0, 32767.0),
            NumClass::Int32 => (-2147483648.0, 2147483647.0),
            NumClass::Int64 => (-9223372036854775808.0, 9223372036854775807.0),
            NumClass::UInt8 => (0.0, 255.0),
            NumClass::UInt16 => (0.0, 65535.0),
            NumClass::UInt32 => (0.0, 4294967295.0),
            NumClass::UInt64 => (0.0, 18446744073709551615.0),
            _ => (f64::NEG_INFINITY, f64::INFINITY),
        }
    }
    /// Convert one value into this class's value set.
    pub fn cast(self, x: f64) -> f64 {
        match self {
            NumClass::Double => x,
            NumClass::Single => x as f32 as f64,
            _ => {
                if x.is_nan() {
                    return 0.0;
                }
                let (lo, hi) = self.int_range();
                x.round().clamp(lo, hi)
            }
        }
    }
}

/// Real or complex numeric array. `im` is `None` for real arrays.
#[derive(Clone, Debug, PartialEq)]
pub struct Num {
    pub class: NumClass,
    pub dims: Vec<usize>,
    pub re: Vec<f64>,
    pub im: Option<Vec<f64>>,
}

impl Num {
    pub fn real(dims: Vec<usize>, re: Vec<f64>) -> Num {
        Num {
            class: NumClass::Double,
            dims: norm_dims(dims),
            re,
            im: None,
        }
    }
    pub fn complex(dims: Vec<usize>, re: Vec<f64>, im: Vec<f64>) -> Num {
        let mut n = Num {
            class: NumClass::Double,
            dims: norm_dims(dims),
            re,
            im: Some(im),
        };
        n.narrow();
        n
    }
    pub fn scalar(x: f64) -> Num {
        Num {
            class: NumClass::Double,
            dims: vec![1, 1],
            re: vec![x],
            im: None,
        }
    }
    pub fn empty() -> Num {
        Num::real(vec![0, 0], vec![])
    }
    pub fn numel(&self) -> usize {
        self.re.len()
    }
    pub fn is_complex(&self) -> bool {
        self.im.is_some()
    }
    /// Drop an all-zero imaginary part (MATLAB narrows results to real).
    pub fn narrow(&mut self) {
        if let Some(im) = &self.im
            && im.iter().all(|v| *v == 0.0)
        {
            self.im = None;
        }
    }
    pub fn with_class(mut self, class: NumClass) -> Num {
        if class != NumClass::Double {
            for v in self.re.iter_mut() {
                *v = class.cast(*v);
            }
            if let Some(im) = &mut self.im {
                for v in im.iter_mut() {
                    *v = class.cast(*v);
                }
            }
        }
        self.class = class;
        self
    }
    pub fn im_at(&self, i: usize) -> f64 {
        self.im.as_ref().map_or(0.0, |v| v[i])
    }
}

// ---------------------------------------------------------------------------
// Structs, objects, handles
// ---------------------------------------------------------------------------

/// Struct array: shared ordered field list, one value vector per element.
#[derive(Clone, Debug, PartialEq)]
pub struct StructArray {
    pub dims: Vec<usize>,
    pub fields: Vec<String>,
    /// `elems[i][f]` is field `fields[f]` of element `i` (column-major).
    pub elems: Vec<Vec<Value>>,
}

impl StructArray {
    pub fn new_scalar() -> StructArray {
        StructArray {
            dims: vec![1, 1],
            fields: vec![],
            elems: vec![vec![]],
        }
    }
    pub fn empty_with(fields: Vec<String>, dims: Vec<usize>) -> StructArray {
        let dims = norm_dims(dims);
        let n = numel_of(&dims);
        let k = fields.len();
        StructArray {
            dims,
            fields,
            elems: vec![vec![Value::default(); k]; n],
        }
    }
    pub fn numel(&self) -> usize {
        self.elems.len()
    }
    pub fn field_index(&self, name: &str) -> Option<usize> {
        self.fields.iter().position(|f| f == name)
    }
    /// Index of `name`, adding the field (filled with `[]`) if missing.
    pub fn ensure_field(&mut self, name: &str) -> usize {
        if let Some(i) = self.field_index(name) {
            return i;
        }
        self.fields.push(name.to_string());
        for e in self.elems.iter_mut() {
            e.push(Value::default());
        }
        self.fields.len() - 1
    }
    pub fn get(&self, elem: usize, name: &str) -> Option<&Value> {
        self.field_index(name).map(|f| &self.elems[elem][f])
    }
    pub fn set(&mut self, elem: usize, name: &str, v: Value) {
        let f = self.ensure_field(name);
        self.elems[elem][f] = v;
    }
    pub fn remove_field(&mut self, name: &str) -> bool {
        match self.field_index(name) {
            Some(f) => {
                self.fields.remove(f);
                for e in self.elems.iter_mut() {
                    e.remove(f);
                }
                true
            }
            None => false,
        }
    }
}

/// Object with a class name and ordered properties (MException,
/// classdef instances). `meta` is the interpreter's class description
/// (opaque here); `None` for built-in objects.
#[derive(Clone, Debug)]
pub struct Object {
    pub class: String,
    pub props: Vec<(String, Value)>,
    pub meta: Option<Rc<dyn std::any::Any>>,
    /// Array size reported by `size`/`numel`: `[1, 1]` except for the
    /// interpreter's built-in array-like classes (table, datetime, ...).
    pub dims: Vec<usize>,
}

impl PartialEq for Object {
    fn eq(&self, other: &Object) -> bool {
        self.class == other.class && self.dims == other.dims && self.props == other.props
    }
}

impl Object {
    pub fn new(class: &str) -> Object {
        Object {
            class: class.to_string(),
            props: Vec::new(),
            meta: None,
            dims: vec![1, 1],
        }
    }
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.props.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }
    pub fn set(&mut self, name: &str, v: Value) {
        match self.props.iter_mut().find(|(n, _)| n == name) {
            Some(slot) => slot.1 = v,
            None => self.props.push((name.to_string(), v)),
        }
    }
}

/// Anonymous function: parameters, body and captured workspace.
#[derive(Debug)]
pub struct Anon {
    pub params: Vec<String>,
    pub body: Rc<ast::Expr>,
    pub captured: Vec<(String, Value)>,
    /// Source text (`@(x) x + 1`) for display and `func2str`.
    pub text: String,
}

/// Function handle. `ctx` is opaque interpreter context (the defining file)
/// so that handles to local functions resolve wherever they are called.
#[derive(Debug)]
pub struct Handle {
    pub name: String,
    pub anon: Option<Anon>,
    pub ctx: Option<Rc<dyn std::any::Any>>,
}

impl PartialEq for Handle {
    fn eq(&self, other: &Handle) -> bool {
        std::ptr::eq(self, other)
            || (self.anon.is_none() && other.anon.is_none() && self.name == other.name)
    }
}

// ---------------------------------------------------------------------------
// Value
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(Num),
    Bool(Array<bool>),
    Char(Array<char>),
    Cell(Array<Value>),
    Struct(StructArray),
    Func(Rc<Handle>),
    Object(Box<Object>),
    /// MATLAB string array; `None` is `<missing>`.
    Str(Array<Option<String>>),
    /// Instance of a handle class: copies share the object.
    HObject(Rc<std::cell::RefCell<Object>>),
    /// Array of class instances (numel != 1; a single instance is always
    /// `Object`/`HObject`). Elements are `Object` or `HObject` values.
    ObjArr(Box<ObjArray>),
    /// Sparse matrix (double, logical or complex), compressed by columns.
    Sparse(Box<sparse::Sparse>),
}

/// Object array: class name plus the elements, column-major.
#[derive(Clone, Debug)]
pub struct ObjArray {
    pub class: String,
    pub arr: Array<Value>,
    /// Class metadata (as in [`Object::meta`]); kept for empty arrays.
    pub meta: Option<Rc<dyn std::any::Any>>,
}

impl PartialEq for ObjArray {
    fn eq(&self, other: &ObjArray) -> bool {
        self.class == other.class && self.arr == other.arr
    }
}

impl ObjArray {
    /// An object value as an array (a scalar is 1x1).
    pub fn parts(v: &Value) -> Option<ObjArray> {
        match v {
            Value::Object(o) => Some(ObjArray {
                class: o.class.clone(),
                arr: Array::scalar(v.clone()),
                meta: o.meta.clone(),
            }),
            Value::HObject(o) => Some(ObjArray {
                class: o.borrow().class.clone(),
                arr: Array::scalar(v.clone()),
                meta: o.borrow().meta.clone(),
            }),
            Value::ObjArr(o) => Some((**o).clone()),
            _ => None,
        }
    }
    /// 0x0 array of the same class.
    pub fn with_empty(&self) -> ObjArray {
        ObjArray {
            class: self.class.clone(),
            arr: Array::new(vec![0, 0], vec![]),
            meta: self.meta.clone(),
        }
    }
    /// Same class and metadata, other elements.
    pub fn with(&self, dims: Vec<usize>, data: Vec<Value>) -> Value {
        Value::obj_array(ObjArray {
            class: self.class.clone(),
            arr: Array::new(dims, data),
            meta: self.meta.clone(),
        })
    }
}

impl Default for Value {
    fn default() -> Value {
        Value::Num(Num::empty())
    }
}

impl Value {
    pub fn num(x: f64) -> Value {
        Value::Num(Num::scalar(x))
    }
    pub fn complex(re: f64, im: f64) -> Value {
        Value::Num(Num::complex(vec![1, 1], vec![re], vec![im]))
    }
    pub fn bool(b: bool) -> Value {
        Value::Bool(Array::scalar(b))
    }
    pub fn empty() -> Value {
        Value::default()
    }
    pub fn str(s: &str) -> Value {
        let chars: Vec<char> = s.chars().collect();
        if chars.is_empty() {
            return Value::Char(Array::new(vec![0, 0], vec![]));
        }
        Value::Char(Array::row(chars))
    }
    pub fn row(v: Vec<f64>) -> Value {
        Value::Num(Num::real(vec![1, v.len()], v))
    }
    pub fn col(v: Vec<f64>) -> Value {
        Value::Num(Num::real(vec![v.len(), 1], v))
    }
    pub fn matrix(dims: Vec<usize>, v: Vec<f64>) -> Value {
        Value::Num(Num::real(dims, v))
    }
    pub fn cell_row(v: Vec<Value>) -> Value {
        Value::Cell(Array::row(v))
    }
    pub fn cell_col(v: Vec<Value>) -> Value {
        Value::Cell(Array::col(v))
    }
    /// Object array value; a single element is the element itself.
    pub fn obj_array(mut o: ObjArray) -> Value {
        if o.arr.data.len() == 1 && o.arr.data[0].is_object_like() {
            return o.arr.data.pop().unwrap();
        }
        Value::ObjArr(Box::new(o))
    }
    /// Class instance or object array.
    pub fn is_object_like(&self) -> bool {
        matches!(
            self,
            Value::Object(_) | Value::HObject(_) | Value::ObjArr(_)
        )
    }
    pub fn empty_cell() -> Value {
        Value::Cell(Array::new(vec![0, 0], vec![]))
    }

    pub fn dims(&self) -> Vec<usize> {
        match self {
            Value::Num(n) => n.dims.clone(),
            Value::Bool(a) => a.dims.clone(),
            Value::Char(a) => a.dims.clone(),
            Value::Cell(a) => a.dims.clone(),
            Value::Struct(s) => s.dims.clone(),
            Value::Str(a) => a.dims.clone(),
            Value::ObjArr(o) => o.arr.dims.clone(),
            Value::Object(o) => o.dims.clone(),
            Value::Func(_) | Value::HObject(_) => vec![1, 1],
            Value::Sparse(s) => s.dims.to_vec(),
        }
    }
    pub fn dims_ref(&self) -> &[usize] {
        const ONE: [usize; 2] = [1, 1];
        match self {
            Value::Num(n) => &n.dims,
            Value::Bool(a) => &a.dims,
            Value::Char(a) => &a.dims,
            Value::Cell(a) => &a.dims,
            Value::Struct(s) => &s.dims,
            Value::Str(a) => &a.dims,
            Value::ObjArr(o) => &o.arr.dims,
            Value::Object(o) => &o.dims,
            Value::Func(_) | Value::HObject(_) => &ONE,
            Value::Sparse(s) => &s.dims,
        }
    }
    pub fn numel(&self) -> usize {
        numel_of(self.dims_ref())
    }
    pub fn is_empty(&self) -> bool {
        self.numel() == 0
    }
    pub fn is_scalar(&self) -> bool {
        self.numel() == 1
    }
    pub fn class_name(&self) -> String {
        match self {
            Value::Num(n) => n.class.name().into(),
            Value::Bool(_) => "logical".into(),
            Value::Char(_) => "char".into(),
            Value::Cell(_) => "cell".into(),
            Value::Struct(_) => "struct".into(),
            Value::Func(_) => "function_handle".into(),
            Value::Object(o) => o.class.clone(),
            Value::Str(_) => "string".into(),
            Value::HObject(o) => o.borrow().class.clone(),
            Value::ObjArr(o) => o.class.clone(),
            Value::Sparse(s) if s.logical => "logical".into(),
            Value::Sparse(_) => "double".into(),
        }
    }
    pub fn is_sparse(&self) -> bool {
        matches!(self, Value::Sparse(_))
    }
    pub fn is_numeric(&self) -> bool {
        match self {
            Value::Num(_) => true,
            Value::Sparse(s) => !s.logical,
            _ => false,
        }
    }
    pub fn is_char(&self) -> bool {
        matches!(self, Value::Char(_))
    }
    pub fn is_string_row(&self) -> bool {
        match self {
            Value::Char(a) => a.dims.len() == 2 && (a.dims[0] == 1 || a.numel() == 0),
            _ => false,
        }
    }

    /// Text of a char array (rows concatenated column-major order is not
    /// what users want: rows are read left to right, top to bottom).
    pub fn to_str(&self) -> Option<String> {
        match self {
            Value::Str(a) if a.data.len() == 1 => a.data[0].clone(),
            Value::Char(a) => {
                if a.dims.len() == 2 && a.dims[0] > 1 {
                    let (r, c) = (a.dims[0], a.dims[1]);
                    let mut s = String::new();
                    for i in 0..r {
                        for j in 0..c {
                            s.push(a.data[i + j * r]);
                        }
                    }
                    Some(s)
                } else {
                    Some(a.data.iter().collect())
                }
            }
            _ => None,
        }
    }
    /// Rows of a char matrix as strings.
    pub fn char_rows(&self) -> Option<Vec<String>> {
        match self {
            Value::Str(a) if a.data.len() == 1 => a.data[0].clone().map(|s| vec![s]),
            Value::Char(a) => {
                let r = a.dims[0];
                let c = a.data.len().checked_div(r).unwrap_or(0);
                Some(
                    (0..r)
                        .map(|i| (0..c).map(|j| a.data[i + j * r]).collect())
                        .collect(),
                )
            }
            _ => None,
        }
    }

    /// Numeric view (double, logical and char convert). `None` for others.
    pub fn to_num(&self) -> Option<Num> {
        match self {
            Value::Num(n) => Some(n.clone()),
            Value::Bool(a) => Some(Num::real(
                a.dims.clone(),
                a.data.iter().map(|b| *b as u8 as f64).collect(),
            )),
            Value::Char(a) => Some(Num::real(
                a.dims.clone(),
                a.data.iter().map(|c| *c as u32 as f64).collect(),
            )),
            Value::Sparse(s) => s.dense().and_then(|d| d.to_num()),
            _ => None,
        }
    }
    /// Real parts as f64 (numeric, logical, char).
    pub fn to_f64_vec(&self) -> Option<Vec<f64>> {
        match self {
            Value::Num(n) => Some(n.re.clone()),
            Value::Bool(a) => Some(a.data.iter().map(|b| *b as u8 as f64).collect()),
            Value::Char(a) => Some(a.data.iter().map(|c| *c as u32 as f64).collect()),
            Value::Sparse(s) => s.dense().and_then(|d| d.to_f64_vec()),
            _ => None,
        }
    }
    pub fn scalar_f64(&self) -> Option<f64> {
        if self.numel() != 1 {
            return None;
        }
        match self {
            Value::Num(n) => Some(n.re[0]),
            Value::Bool(a) => Some(a.data[0] as u8 as f64),
            Value::Char(a) => Some(a.data[0] as u32 as f64),
            Value::Sparse(s) => Some(s.re.first().copied().unwrap_or(0.0)),
            _ => None,
        }
    }
    /// Element `i` as f64 (real part).
    pub fn f64_at(&self, i: usize) -> Option<f64> {
        match self {
            Value::Num(n) => n.re.get(i).copied(),
            Value::Bool(a) => a.data.get(i).map(|b| *b as u8 as f64),
            Value::Char(a) => a.data.get(i).map(|c| *c as u32 as f64),
            Value::Sparse(s) => {
                let m = s.dims[0].max(1);
                if i >= m.saturating_mul(s.dims[1]) {
                    return None;
                }
                Some(s.find(i % m, i / m).map_or(0.0, |k| s.re[k]))
            }
            _ => None,
        }
    }

    /// MATLAB truth: non-empty and all elements non-zero (real part, NaN is an error upstream).
    pub fn is_true(&self) -> Res<bool> {
        match self {
            Value::Num(n) => {
                if n.re.iter().any(|v| v.is_nan()) {
                    return Err(Error::new(
                        "MATLAB:nologicalnan",
                        "NaN's cannot be converted to logicals.",
                    ));
                }
                Ok(
                    !n.re.is_empty()
                        && (0..n.re.len()).all(|i| n.re[i] != 0.0 || n.im_at(i) != 0.0),
                )
            }
            Value::Bool(a) => Ok(!a.data.is_empty() && a.data.iter().all(|b| *b)),
            Value::Char(a) => Ok(!a.data.is_empty() && a.data.iter().all(|c| *c != '\0')),
            Value::Sparse(s) => {
                if s.re.iter().any(|v| v.is_nan()) {
                    return Err(Error::new(
                        "MATLAB:nologicalnan",
                        "NaN's cannot be converted to logicals.",
                    ));
                }
                Ok(s.dims[0].checked_mul(s.dims[1]) == Some(s.nnz()) && s.nnz() > 0)
            }
            Value::Cell(_) => Err(Error::msg(
                "Conversion to logical from cell is not possible.",
            )),
            Value::Struct(_) => Err(Error::msg(
                "Conversion to logical from struct is not possible.",
            )),
            other => Err(Error::msg(format!(
                "Conversion to logical from {} is not possible.",
                other.class_name()
            ))),
        }
    }

    /// Re-shape keeping the data (caller checks numel).
    pub fn reshaped(self, dims: Vec<usize>) -> Value {
        let dims = norm_dims(dims);
        match self {
            Value::Num(mut n) => {
                n.dims = dims;
                Value::Num(n)
            }
            Value::Bool(mut a) => {
                a.dims = dims;
                Value::Bool(a)
            }
            Value::Char(mut a) => {
                a.dims = dims;
                Value::Char(a)
            }
            Value::Cell(mut a) => {
                a.dims = dims;
                Value::Cell(a)
            }
            Value::Struct(mut s) => {
                s.dims = dims;
                Value::Struct(s)
            }
            Value::Str(mut a) => {
                a.dims = dims;
                Value::Str(a)
            }
            Value::Sparse(s) if dims.len() == 2 => {
                Value::Sparse(Box::new(s.reshaped(dims[0], dims[1])))
            }
            other => other,
        }
    }
}

impl Value {
    pub fn string(s: &str) -> Value {
        Value::Str(Array::scalar(Some(s.to_string())))
    }
    pub fn is_text(&self) -> bool {
        matches!(self, Value::Char(_) | Value::Str(_))
    }
    /// Text scalar: char row or 1x1 string.
    pub fn is_text_scalar(&self) -> bool {
        self.is_string_row() || matches!(self, Value::Str(a) if a.data.len() == 1)
    }
}

/// Convert to a string array (MATLAB `string(x)` for text and numbers).
pub fn to_string_array(v: &Value) -> Array<Option<String>> {
    match v {
        Value::Str(a) => a.clone(),
        Value::Char(a) => {
            if a.dims.len() == 2 && a.dims[0] > 1 {
                let rows = v.char_rows().unwrap();
                let n = rows.len();
                Array::new(vec![n, 1], rows.into_iter().map(Some).collect())
            } else {
                Array::scalar(Some(a.data.iter().collect()))
            }
        }
        Value::Cell(c) => c.map(|x| x.to_str()),
        Value::Sparse(s) => match s.dense() {
            Some(d) => to_string_array(&d),
            None => Array::scalar(None),
        },
        Value::Bool(b) => b.map(|x| {
            Some(if *x {
                "true".to_string()
            } else {
                "false".to_string()
            })
        }),
        Value::Num(n) => {
            let mut out = Vec::with_capacity(n.re.len());
            for i in 0..n.re.len() {
                let r = n.re[i];
                if r.is_nan() && n.im.is_none() {
                    out.push(None);
                    continue;
                }
                let one = Num {
                    class: n.class,
                    dims: vec![1, 1],
                    re: vec![r],
                    im: n.im.as_ref().map(|im| vec![im[i]]),
                };
                out.push(Some(crate::format::num2str(&one)));
            }
            Array::new(n.dims.clone(), out)
        }
        _ => Array::scalar(None),
    }
}

/// Logical array from a predicate result.
pub fn bool_array(dims: Vec<usize>, data: Vec<bool>) -> Value {
    Value::Bool(Array::new(dims, data))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dims_normalise() {
        assert_eq!(norm_dims(vec![3]), vec![3, 1]);
        assert_eq!(norm_dims(vec![2, 3, 1, 1]), vec![2, 3]);
        assert_eq!(norm_dims(vec![0, 3]), vec![0, 3]);
    }
    #[test]
    fn int_cast_saturates() {
        assert_eq!(NumClass::UInt8.cast(300.0), 255.0);
        assert_eq!(NumClass::Int8.cast(-2.5), -3.0);
        assert_eq!(NumClass::Int32.cast(f64::NAN), 0.0);
    }
}
