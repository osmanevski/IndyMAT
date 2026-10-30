//! Plain value types of the API: numeric arrays and property values.
//!
//! These are deliberately independent of the interpreter's value type (which
//! is not frozen yet); the interpreter converts at the call boundary.

use crate::model::Handle;

/// Numeric class of an array. Graphics keeps it for CData, where the
/// contract records `cdata_class`; values are always carried as f64.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NumClass {
    #[default]
    Double,
    Single,
    Int8,
    UInt8,
    Int16,
    UInt16,
    Int32,
    UInt32,
    Int64,
    UInt64,
}

impl NumClass {
    pub fn name(self) -> &'static str {
        match self {
            NumClass::Double => "double",
            NumClass::Single => "single",
            NumClass::Int8 => "int8",
            NumClass::UInt8 => "uint8",
            NumClass::Int16 => "int16",
            NumClass::UInt16 => "uint16",
            NumClass::Int32 => "int32",
            NumClass::UInt32 => "uint32",
            NumClass::Int64 => "int64",
            NumClass::UInt64 => "uint64",
        }
    }

    pub fn is_float(self) -> bool {
        matches!(self, NumClass::Double | NumClass::Single)
    }
}

/// A real N-d array (N = 2 or 3 in practice), column-major.
#[derive(Debug, Clone, PartialEq)]
pub struct Array {
    pub dims: Vec<usize>,
    pub data: Vec<f64>,
    pub class: NumClass,
}

impl Default for Array {
    fn default() -> Self {
        Array::empty()
    }
}

impl Array {
    pub fn empty() -> Array {
        Array {
            dims: vec![0, 0],
            data: Vec::new(),
            class: NumClass::Double,
        }
    }

    pub fn scalar(v: f64) -> Array {
        Array::matrix(1, 1, vec![v])
    }

    /// A 1×N row vector.
    pub fn row(v: &[f64]) -> Array {
        Array::matrix(1, v.len(), v.to_vec())
    }

    /// An N×1 column vector.
    pub fn col(v: &[f64]) -> Array {
        Array::matrix(v.len(), 1, v.to_vec())
    }

    /// A rows×cols matrix from column-major data. Panics if sizes disagree,
    /// so callers building arrays from user data should use [`Array::new`].
    pub fn matrix(rows: usize, cols: usize, data: Vec<f64>) -> Array {
        assert_eq!(rows * cols, data.len(), "Array::matrix size mismatch");
        Array {
            dims: vec![rows, cols],
            data,
            class: NumClass::Double,
        }
    }

    /// Checked constructor for any dimensions (trailing singleton dims are
    /// dropped down to two).
    pub fn new(dims: &[usize], data: Vec<f64>) -> Option<Array> {
        let mut dims = dims.to_vec();
        while dims.len() < 2 {
            dims.push(1);
        }
        while dims.len() > 2 && dims.last() == Some(&1) {
            dims.pop();
        }
        (dims.iter().product::<usize>() == data.len()).then_some(Array {
            dims,
            data,
            class: NumClass::Double,
        })
    }

    /// Builds a matrix from row slices (convenient in tests).
    pub fn from_rows(rows: &[&[f64]]) -> Array {
        let r = rows.len();
        let c = rows.first().map_or(0, |row| row.len());
        let mut data = Vec::with_capacity(r * c);
        for j in 0..c {
            for row in rows {
                data.push(row.get(j).copied().unwrap_or(f64::NAN));
            }
        }
        Array::matrix(r, c, data)
    }

    pub fn with_class(mut self, class: NumClass) -> Array {
        self.class = class;
        self
    }

    pub fn rows(&self) -> usize {
        self.dims[0]
    }

    /// Product of all dimensions after the first two collapses into pages.
    pub fn cols(&self) -> usize {
        self.dims.get(1).copied().unwrap_or(1)
    }

    pub fn pages(&self) -> usize {
        self.dims[2..].iter().product()
    }

    pub fn numel(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn is_vector(&self) -> bool {
        self.dims.len() == 2 && (self.dims[0] == 1 || self.dims[1] == 1)
    }

    pub fn is_scalar(&self) -> bool {
        self.data.len() == 1
    }

    pub fn at(&self, r: usize, c: usize) -> f64 {
        self.data[r + c * self.rows()]
    }

    /// Column `c` of a 2-D matrix.
    pub fn column(&self, c: usize) -> &[f64] {
        let r = self.rows();
        &self.data[c * r..(c + 1) * r]
    }

    /// Finite min/max, ignoring NaN; `None` when nothing is finite.
    pub fn finite_range(&self) -> Option<[f64; 2]> {
        finite_range(&self.data)
    }
}

pub fn finite_range(values: &[f64]) -> Option<[f64; 2]> {
    let mut out: Option<[f64; 2]> = None;
    for &v in values.iter().filter(|v| v.is_finite()) {
        out = Some(match out {
            None => [v, v],
            Some([lo, hi]) => [lo.min(v), hi.max(v)],
        });
    }
    out
}

/// A property value as seen by `set`/`get`.
#[derive(Debug, Clone, PartialEq)]
pub enum PropValue {
    Num(f64),
    Array(Array),
    Str(String),
    /// Cell array of strings (legend labels, tick labels, multi-line text).
    Cell(Vec<String>),
    Handle(Handle),
    Handles(Vec<Handle>),
    Bool(bool),
}

impl PropValue {
    pub fn str(s: &str) -> PropValue {
        PropValue::Str(s.to_string())
    }

    pub fn on_off(b: bool) -> PropValue {
        PropValue::str(if b { "on" } else { "off" })
    }

    pub fn row(v: &[f64]) -> PropValue {
        PropValue::Array(Array::row(v))
    }

    /// Numeric content (scalar, vector or matrix) flattened column-major.
    pub fn numbers(&self) -> Option<Vec<f64>> {
        match self {
            PropValue::Num(n) => Some(vec![*n]),
            PropValue::Array(a) => Some(a.data.clone()),
            PropValue::Bool(b) => Some(vec![f64::from(u8::from(*b))]),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            PropValue::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<Array> {
        match self {
            PropValue::Num(n) => Some(Array::scalar(*n)),
            PropValue::Array(a) => Some(a.clone()),
            _ => None,
        }
    }
}

impl From<f64> for PropValue {
    fn from(n: f64) -> Self {
        PropValue::Num(n)
    }
}
impl From<&str> for PropValue {
    fn from(s: &str) -> Self {
        PropValue::str(s)
    }
}
impl From<Array> for PropValue {
    fn from(a: Array) -> Self {
        PropValue::Array(a)
    }
}
impl<const N: usize> From<[f64; N]> for PropValue {
    fn from(v: [f64; N]) -> Self {
        PropValue::row(&v)
    }
}
