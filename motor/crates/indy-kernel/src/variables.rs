//! Workspace rows and `inspect` pages for interpreter values.
//!
//! The shapes follow `docs/Kernel-Protokolu.md` and what IndyMAT's Octave
//! bridge produced (`octave/__mf_execute__.m`, `octave/__mf_variable__.m`):
//! previews are `mat2str(v, 5)` for small numeric/logical/char values and
//! "RxC class" otherwise; matrix pages use `%.17g` (`%.9g` for single).

use crate::engine::{EngineError, ErrorCode, PageRequest, VariableSummary};
use crate::json::Json;
use indy_value::format::{fmt_g, handle_text, mat2str};
use indy_value::ops::dims_text;
use indy_value::{NumClass, Value};

/// Longest text preview, in characters.
const PREVIEW_CHARS: usize = 100;
/// Character pages: at most this many rows and columns are shown.
const TEXT_ROWS: usize = 100;
const TEXT_COLUMNS: usize = 10_000;

pub fn summary(name: &str, value: &Value, global: bool) -> VariableSummary {
    VariableSummary {
        name: name.to_string(),
        class: value.class_name(),
        size: value.dims_ref().iter().map(|&n| n as u64).collect(),
        bytes: bytes(value),
        complex: is_complex(value),
        global,
        preview: preview(value),
    }
}

fn is_complex(value: &Value) -> bool {
    match value {
        Value::Num(n) => n.is_complex(),
        Value::Sparse(s) => s.im.is_some(),
        _ => false,
    }
}

/// Storage estimate in bytes (MATLAB element sizes; char as 1 byte, as
/// Octave reports it).
fn bytes(value: &Value) -> u64 {
    let n = value.numel() as u64;
    match value {
        Value::Num(x) => {
            let width = match x.class {
                NumClass::Double | NumClass::Int64 | NumClass::UInt64 => 8,
                NumClass::Single | NumClass::Int32 | NumClass::UInt32 => 4,
                NumClass::Int16 | NumClass::UInt16 => 2,
                NumClass::Int8 | NumClass::UInt8 => 1,
            };
            n * width * if x.is_complex() { 2 } else { 1 }
        }
        Value::Bool(_) | Value::Char(_) => n,
        Value::Cell(c) => c.data.iter().map(bytes).sum(),
        Value::Struct(s) => s.elems.iter().flatten().map(bytes).sum(),
        Value::Str(s) => s
            .data
            .iter()
            .map(|e| e.as_ref().map_or(0, |t| t.len() as u64))
            .sum(),
        Value::Func(_) | Value::Object(_) | Value::HObject(_) | Value::ObjArr(_) => 0,
        Value::Sparse(s) => s.storage_bytes(),
    }
}

fn shape_class(value: &Value) -> String {
    let sparse = if value.is_sparse() { "sparse " } else { "" };
    format!(
        "{} {sparse}{}",
        dims_text(value.dims_ref()),
        value.class_name()
    )
}

fn clip(text: &str) -> String {
    text.chars().take(PREVIEW_CHARS).collect()
}

fn preview(value: &Value) -> String {
    let dims = value.dims_ref();
    let small = dims.len() <= 2 && value.numel() <= 12;
    match value {
        Value::Char(_) if dims[0] <= 1 => clip(&value.to_str().unwrap_or_default()),
        Value::Char(_) => shape_class(value),
        Value::Num(_) | Value::Bool(_) if small => {
            mat2str(value, 5).unwrap_or_else(|_| shape_class(value))
        }
        // Small enough for the full form; the text says it is not a plain matrix.
        Value::Sparse(s) if small => match s.dense().map(|d| mat2str(&d, 5)) {
            Some(Ok(text)) => format!("sparse {text}"),
            _ => shape_class(value),
        },
        Value::Func(h) => clip(&handle_text(h)),
        Value::Str(s) if s.data.len() == 1 => match &s.data[0] {
            Some(text) => clip(&format!("\"{text}\"")),
            None => "<missing>".into(),
        },
        _ => shape_class(value),
    }
}

fn number_text(x: f64, precision: usize) -> String {
    if x.is_nan() {
        "NaN".into()
    } else if x.is_infinite() {
        if x < 0.0 { "-Inf" } else { "Inf" }.into()
    } else {
        fmt_g(x, precision, false, false)
    }
}

fn complex_text(re: f64, im: f64, precision: usize) -> String {
    let sign = if im.is_sign_negative() { '-' } else { '+' };
    format!(
        "{}{sign}{}i",
        number_text(re, precision),
        number_text(im.abs(), precision)
    )
}

/// Text of the element at 0-based row `i`, column `j` of a 2-D value.
fn element_text(value: &Value, i: usize, j: usize) -> String {
    let index = i + j * value.dims_ref()[0];
    match value {
        Value::Bool(b) => (b.data[index] as u8).to_string(),
        Value::Num(x) => {
            let precision = if x.class == NumClass::Single { 9 } else { 17 };
            match &x.im {
                Some(im) => complex_text(x.re[index], im[index], precision),
                None => number_text(x.re[index], precision),
            }
        }
        Value::Sparse(s) => {
            let k = s.find(i, j);
            let re = k.map_or(0.0, |k| s.re[k]);
            match (&s.im, s.logical) {
                (_, true) => (re as u8).to_string(),
                (Some(_), _) => complex_text(re, k.map_or(0.0, |k| s.entry_im(k)), 17),
                (None, _) => number_text(re, 17),
            }
        }
        _ => String::new(),
    }
}

fn not_found(name: &str) -> EngineError {
    EngineError::new(ErrorCode::NotFound, format!("Variable not found: {name}"))
}

/// The protocol's variable page for `value`.
pub fn page(name: &str, value: Option<&Value>, page: PageRequest) -> Result<Json, EngineError> {
    let value = value.ok_or_else(|| not_found(name))?;
    let dims = value.dims_ref();
    let base = Json::object()
        .with("name", name)
        .with("class", value.class_name())
        .with(
            "size",
            Json::Array(dims.iter().map(|&n| Json::from(n as u64)).collect()),
        )
        .with("real", !is_complex(value))
        .with("editable", false);
    let flat = dims.len() <= 2;
    Ok(match value {
        Value::Num(_) | Value::Bool(_) | Value::Sparse(_) if flat => {
            let (rows, columns) = (dims[0] as u64, dims[1] as u64);
            let first_row = page.row.min(rows + 1);
            let first_column = page.column.min(columns + 1);
            let row_count = page.rows.min(rows + 1 - first_row);
            let column_count = page.columns.min(columns + 1 - first_column);
            let cells: Vec<Json> = (0..row_count)
                .map(|r| {
                    Json::Array(
                        (0..column_count)
                            .map(|c| {
                                let (i, j) = (first_row - 1 + r, first_column - 1 + c);
                                Json::from(element_text(value, i as usize, j as usize))
                            })
                            .collect(),
                    )
                })
                .collect();
            let truncated = page.row > 1
                || page.column > 1
                || first_row - 1 + row_count < rows
                || first_column - 1 + column_count < columns;
            let base = match value {
                Value::Sparse(s) => base.with("sparse", true).with("nnz", s.nnz() as u64),
                _ => base,
            };
            base.with("kind", "matrix")
                .with("row", page.row)
                .with("column", page.column)
                .with("row_count", row_count)
                .with("column_count", column_count)
                .with("row_total", rows)
                .with("column_total", columns)
                .with("truncated", truncated)
                .with("rows", Json::Array(cells))
        }
        Value::Char(_) if flat => {
            let lines = value.char_rows().unwrap_or_default();
            let truncated = lines.len() > TEXT_ROWS || dims[1] > TEXT_COLUMNS;
            let text: Vec<String> = lines
                .iter()
                .take(TEXT_ROWS)
                .map(|l| l.chars().take(TEXT_COLUMNS).collect())
                .collect();
            base.with("kind", "char-text")
                .with("text", text.join("\n"))
                .with("truncated", truncated)
        }
        _ => base
            .with("kind", "detail")
            .with("text", detail_text(value))
            .with("truncated", false),
    })
}

/// Read-only summary for values without a matrix or text page.
fn detail_text(value: &Value) -> String {
    let shape = dims_text(value.dims_ref());
    match value {
        Value::Num(_) | Value::Bool(_) | Value::Char(_) | Value::Sparse(_) => format!(
            "{shape} {}: multidimensional array; use indexing to inspect it.",
            value.class_name()
        ),
        Value::Cell(c) => {
            let mut text = format!("Cell array {shape}\n");
            for (k, item) in c.data.iter().take(30).enumerate() {
                text.push_str(&format!("{{{}}}: {}\n", k + 1, shape_class(item)));
            }
            if c.data.len() > 30 {
                text.push_str("...\n");
            }
            text
        }
        Value::Struct(s) => format!("Struct {shape}\nFields:\n{}", s.fields.join("\n")),
        Value::Func(h) => handle_text(h),
        _ => format!(
            "{}: {} ({shape}). Contents are not expanded; the class and size summary is read-only.",
            match value {
                Value::Str(_) => "String array",
                _ => "Object",
            },
            value.class_name()
        ),
    }
}
