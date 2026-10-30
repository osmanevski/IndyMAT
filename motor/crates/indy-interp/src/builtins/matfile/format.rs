//! MAT-file formats, written from MathWorks' "MAT-File Format" document.
//!
//! - Level 5 (MATLAB 5 / `-v6`, and `-v7` with `miCOMPRESSED` elements):
//!   read in either byte order; written little-endian. Double, single,
//!   integers, logical, char, complex, cell, struct (arrays), sparse (real,
//!   complex, logical), function handles and string arrays are read and
//!   written (the last two: [`mcos`]); `mxOBJECT` (old-style class objects) is
//!   read as a struct. Other `mxOPAQUE` objects (`table`, classdef objects) are
//!   skipped with a warning.
//!   Written sparse arrays use `mxSPARSE`; Octave's logical sparse layout
//!   (`mxUINT8` with the logical flag) is read as well.
//! - Level 4 (`-v4`): 2-D numeric, char and sparse matrices, both directions.
//! - Version 7.3 (HDF5) is refused with an explicit error.
//!
//! The reader never trusts a size: every count is checked against the bytes
//! present, so a damaged file gives an error, never a panic or a huge
//! allocation.

use indy_value::sparse::{Entry, Sparse};
use indy_value::{Array, Num, NumClass, StructArray, Value};

use super::zlib;

mod mcos;

// Data types (miXXX).
const MI_INT8: u32 = 1;
const MI_UINT8: u32 = 2;
const MI_INT16: u32 = 3;
const MI_UINT16: u32 = 4;
const MI_INT32: u32 = 5;
const MI_UINT32: u32 = 6;
const MI_SINGLE: u32 = 7;
const MI_DOUBLE: u32 = 9;
const MI_INT64: u32 = 12;
const MI_UINT64: u32 = 13;
const MI_MATRIX: u32 = 14;
const MI_COMPRESSED: u32 = 15;
const MI_UTF8: u32 = 16;
const MI_UTF16: u32 = 17;
const MI_UTF32: u32 = 18;

// Array classes (mxXXX_CLASS).
const MX_CELL: u8 = 1;
const MX_STRUCT: u8 = 2;
const MX_OBJECT: u8 = 3;
const MX_CHAR: u8 = 4;
const MX_SPARSE: u8 = 5;
const MX_DOUBLE: u8 = 6;
const MX_SINGLE: u8 = 7;
const MX_INT8: u8 = 8;
const MX_UINT8: u8 = 9;
const MX_INT16: u8 = 10;
const MX_UINT16: u8 = 11;
const MX_INT32: u8 = 12;
const MX_UINT32: u8 = 13;
const MX_INT64: u8 = 14;
const MX_UINT64: u8 = 15;
const MX_FUNCTION: u8 = 16;
const MX_OPAQUE: u8 = 17;

const FLAG_COMPLEX: u32 = 0x0800;
const FLAG_GLOBAL: u32 = 0x0400;
const FLAG_LOGICAL: u32 = 0x0200;

/// Most elements a struct array without fields may claim (it has no bytes
/// to check the count against).
const MAX_EMPTY_ELEMS: usize = 10_000_000;

/// One variable of a MAT file. `value` is `None` when it was skipped
/// (the reason is in [`MatData::warnings`]).
#[derive(Clone, Debug)]
pub struct MatVar {
    pub name: String,
    pub value: Option<Value>,
    pub global: bool,
    /// MATLAB class name as stored (also for skipped variables).
    pub class: String,
    pub dims: Vec<usize>,
    /// The complete element as stored in the file (tag included, compressed
    /// elements still compressed), for `save -append`. Empty for level 4.
    pub raw: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct MatData {
    pub vars: Vec<MatVar>,
    pub warnings: Vec<String>,
    /// Level 5 file with subsystem data (MATLAB objects): `-append` must not
    /// rewrite it.
    pub subsystem: bool,
    /// Byte range of the element holding object data.
    pub subsystem_range: Option<(usize, usize)>,
    pub level: u8,
}

/// Which file a byte sequence holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Level5,
    Hdf5,
    Level4,
    Unknown,
}

pub fn sniff(data: &[u8]) -> Kind {
    if data.len() >= 128 && (&data[126..128] == b"IM" || &data[126..128] == b"MI") {
        if data.starts_with(b"MATLAB 7.3") {
            return Kind::Hdf5;
        }
        return Kind::Level5;
    }
    if data.len() >= 520 && &data[512..520] == b"\x89HDF\r\n\x1a\n" {
        return Kind::Hdf5;
    }
    if level4_header(data, 0).is_some() {
        return Kind::Level4;
    }
    Kind::Unknown
}

// ---------------------------------------------------------------------------
// Level 5 reader
// ---------------------------------------------------------------------------

struct Rd<'a> {
    buf: &'a [u8],
    pos: usize,
    be: bool,
}

struct Elem<'a> {
    ty: u32,
    data: &'a [u8],
}

fn u32_at(b: &[u8], be: bool) -> u32 {
    let a = [b[0], b[1], b[2], b[3]];
    if be {
        u32::from_be_bytes(a)
    } else {
        u32::from_le_bytes(a)
    }
}

impl<'a> Rd<'a> {
    fn done(&self) -> bool {
        self.pos + 8 > self.buf.len()
    }
    fn elem(&mut self) -> Result<Elem<'a>, String> {
        let tag = self
            .buf
            .get(self.pos..self.pos + 8)
            .ok_or("truncated data element")?;
        let first = u32_at(&tag[0..4], self.be);
        if first >> 16 != 0 {
            // Small data element: size and type share the first word.
            let size = (first >> 16) as usize;
            if size > 4 {
                return Err("invalid small data element".into());
            }
            let data = &tag[4..4 + size];
            self.pos += 8;
            return Ok(Elem {
                ty: first & 0xffff,
                data,
            });
        }
        let mut size = u32_at(&tag[4..8], self.be) as usize;
        let start = self.pos + 8;
        if first == MI_MATRIX && start + size > self.buf.len() {
            // Octave 11 counts a trailing small element as 8 bytes plus its
            // data; the contents are checked element by element anyway.
            size = self.buf.len() - start;
        }
        let data = self
            .buf
            .get(start..start.checked_add(size).ok_or("invalid element size")?)
            .ok_or("truncated data element")?;
        // Compressed elements are not padded; all others end on 8 bytes.
        let padded = if first == MI_COMPRESSED {
            size
        } else {
            size.div_ceil(8) * 8
        };
        self.pos = (start + padded).min(self.buf.len());
        Ok(Elem { ty: first, data })
    }
}

/// Numbers of a numeric data element, any storage type, as f64.
fn numbers(e: &Elem, be: bool) -> Result<Vec<f64>, String> {
    let d = e.data;
    macro_rules! conv {
        ($w:expr, $f:expr) => {{
            if !d.len().is_multiple_of($w) {
                return Err("numeric data has a partial element".into());
            }
            Ok(d.chunks_exact($w).map($f).collect())
        }};
    }
    let a2 = |c: &[u8]| [c[0], c[1]];
    let a4 = |c: &[u8]| [c[0], c[1], c[2], c[3]];
    let a8 = |c: &[u8]| [c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]];
    match e.ty {
        MI_INT8 => conv!(1, |c: &[u8]| c[0] as i8 as f64),
        MI_UINT8 | MI_UTF8 => conv!(1, |c: &[u8]| c[0] as f64),
        MI_INT16 => conv!(2, |c: &[u8]| if be {
            i16::from_be_bytes(a2(c))
        } else {
            i16::from_le_bytes(a2(c))
        } as f64),
        MI_UINT16 | MI_UTF16 => conv!(2, |c: &[u8]| if be {
            u16::from_be_bytes(a2(c))
        } else {
            u16::from_le_bytes(a2(c))
        } as f64),
        MI_INT32 => conv!(4, |c: &[u8]| if be {
            i32::from_be_bytes(a4(c))
        } else {
            i32::from_le_bytes(a4(c))
        } as f64),
        MI_UINT32 | MI_UTF32 => conv!(4, |c: &[u8]| if be {
            u32::from_be_bytes(a4(c))
        } else {
            u32::from_le_bytes(a4(c))
        } as f64),
        MI_SINGLE => conv!(4, |c: &[u8]| if be {
            f32::from_be_bytes(a4(c))
        } else {
            f32::from_le_bytes(a4(c))
        } as f64),
        MI_DOUBLE => conv!(8, |c: &[u8]| if be {
            f64::from_be_bytes(a8(c))
        } else {
            f64::from_le_bytes(a8(c))
        }),
        MI_INT64 => conv!(8, |c: &[u8]| if be {
            i64::from_be_bytes(a8(c))
        } else {
            i64::from_le_bytes(a8(c))
        } as f64),
        MI_UINT64 => conv!(8, |c: &[u8]| if be {
            u64::from_be_bytes(a8(c))
        } else {
            u64::from_le_bytes(a8(c))
        } as f64),
        t => Err(format!("unexpected data type {t} in numeric data")),
    }
}

fn text(e: &Elem) -> String {
    let end = e.data.iter().position(|&b| b == 0).unwrap_or(e.data.len());
    String::from_utf8_lossy(&e.data[..end]).into_owned()
}

pub fn class_of_mx(c: u8) -> &'static str {
    match c {
        MX_CELL => "cell",
        MX_STRUCT => "struct",
        MX_OBJECT => "object",
        MX_CHAR => "char",
        MX_SPARSE => "double",
        MX_DOUBLE => "double",
        MX_SINGLE => "single",
        MX_INT8 => "int8",
        MX_UINT8 => "uint8",
        MX_INT16 => "int16",
        MX_UINT16 => "uint16",
        MX_INT32 => "int32",
        MX_UINT32 => "uint32",
        MX_INT64 => "int64",
        MX_UINT64 => "uint64",
        MX_FUNCTION => "function_handle",
        MX_OPAQUE => "opaque",
        _ => "unknown",
    }
}

fn num_class(c: u8) -> Option<NumClass> {
    Some(match c {
        MX_DOUBLE => NumClass::Double,
        MX_SINGLE => NumClass::Single,
        MX_INT8 => NumClass::Int8,
        MX_UINT8 => NumClass::UInt8,
        MX_INT16 => NumClass::Int16,
        MX_UINT16 => NumClass::UInt16,
        MX_INT32 => NumClass::Int32,
        MX_UINT32 => NumClass::UInt32,
        MX_INT64 => NumClass::Int64,
        MX_UINT64 => NumClass::UInt64,
        _ => return None,
    })
}

/// A decoded array, or a skipped one with the reason.
enum Decoded {
    Value(Value),
    Skipped(String),
}

struct Header {
    flags: u32,
    class: u8,
    dims: Vec<usize>,
    name: String,
    /// Class name of `mxOBJECT` / `mxOPAQUE` arrays.
    class_name: String,
    global: bool,
    nzmax: usize,
}

struct Reader<'w> {
    be: bool,
    warnings: &'w mut Vec<String>,
    /// An array read so far declared more bytes than it used (see `child`).
    oversized: bool,
    subsystem: Option<&'w mcos::Subsystem>,
    /// Object data decoded so far (see `mcos::Reader::object`).
    object_bytes: usize,
}

impl Reader<'_> {
    /// The `miMATRIX` element at `r`, leaving `r` after it.
    fn child(&mut self, r: &mut Rd, depth: usize) -> Result<(Header, Decoded), String> {
        let start = r.pos;
        let e = r.elem()?;
        if e.ty != MI_MATRIX {
            return Err("an array element was expected".into());
        }
        let mut sub = Rd {
            buf: e.data,
            pos: 0,
            be: self.be,
        };
        let outer = std::mem::replace(&mut self.oversized, false);
        let out = self.matrix(&mut sub, depth)?;
        // A correct size is a multiple of 8. Octave 11 writes the size of a
        // multi-row char array holding a small element 4 bytes too large,
        // and its ancestors add that up (their total may be a multiple of 8
        // again); continue after the bytes the array really used.
        let oversized = self.oversized || !e.data.len().is_multiple_of(8);
        if oversized && sub.pos < e.data.len() {
            r.pos = start + 8 + sub.pos.div_ceil(8) * 8;
        }
        self.oversized = outer || oversized;
        Ok(out)
    }

    /// Contents of one `miMATRIX` element (subelements, no tag).
    fn matrix(&mut self, r: &mut Rd, depth: usize) -> Result<(Header, Decoded), String> {
        let data = r.buf;
        if depth > 200 {
            return Err("arrays are nested too deeply".into());
        }
        let mut h = Header {
            flags: 0,
            class: MX_DOUBLE,
            dims: vec![0, 0],
            name: String::new(),
            class_name: String::new(),
            global: false,
            nzmax: 0,
        };
        if data.is_empty() {
            // An empty miMATRIX is the empty double matrix.
            return Ok((h, Decoded::Value(Value::empty())));
        }
        let flags = r.elem()?;
        if flags.data.len() < 8 {
            return Err("invalid array flags".into());
        }
        let w = u32_at(&flags.data[0..4], self.be);
        h.flags = w;
        h.class = (w & 0xff) as u8;
        h.global = w & FLAG_GLOBAL != 0;
        h.nzmax = u32_at(&flags.data[4..8], self.be) as usize;
        if h.class == MX_OPAQUE {
            // Opaque arrays: name, type system, class name, then the data.
            h.name = text(&r.elem()?);
            let _system = text(&r.elem()?);
            h.class_name = text(&r.elem()?);
            h.dims = vec![1, 1];
            if matches!(
                h.class_name.as_str(),
                "string" | "function_handle_workspace"
            ) {
                let class = h.class_name.clone();
                let d = self.object(r, &class, depth)?;
                if let Decoded::Value(v) = &d {
                    h.dims = v.dims();
                }
                return Ok((h, d));
            }
            let what = match h.class_name.as_str() {
                "table" | "timetable" => format!(
                    "{} objects cannot be loaded from MAT files in the Rust engine yet",
                    h.class_name
                ),
                c => format!(
                    "objects of class '{c}' cannot be loaded from MAT files in the Rust engine yet"
                ),
            };
            return Ok((h, Decoded::Skipped(what)));
        }
        let dims_e = r.elem()?;
        let dims = numbers(&dims_e, self.be)?;
        if dims.len() < 2 || dims.iter().any(|&d| d < 0.0) {
            return Err("invalid dimensions".into());
        }
        h.dims = indy_value::norm_dims(dims.iter().map(|&d| d as usize).collect());
        let numel = h
            .dims
            .iter()
            .try_fold(1usize, |a, &d| a.checked_mul(d))
            .ok_or("dimensions too large")?;
        h.name = text(&r.elem()?);
        let decoded = match h.class {
            MX_DOUBLE..=MX_UINT64 => {
                let cls = num_class(h.class).ok_or("bad class")?;
                let re = numbers(&r.elem()?, self.be)?;
                if w & FLAG_LOGICAL != 0 && h.nzmax > 0 && !r.done() {
                    // Octave writes logical sparse matrices as uint8 arrays
                    // with the sparse layout (MATLAB uses mxSPARSE): a plain
                    // logical array ends after its data, and the row index
                    // count says nothing (it can equal the element count).
                    let v = self.sparse(r, &h, re)?;
                    return Ok((h, Decoded::Value(v)));
                }
                if re.len() != numel {
                    return Err("numeric data does not match the dimensions".into());
                }
                let im = if w & FLAG_COMPLEX != 0 {
                    let im = numbers(&r.elem()?, self.be)?;
                    if im.len() != numel {
                        return Err("imaginary data does not match the dimensions".into());
                    }
                    Some(im)
                } else {
                    None
                };
                if w & FLAG_LOGICAL != 0 && im.is_none() {
                    Value::Bool(Array::new(
                        h.dims.clone(),
                        re.iter().map(|&x| x != 0.0).collect(),
                    ))
                } else {
                    let n = Num {
                        class: cls,
                        dims: h.dims.clone(),
                        re,
                        im,
                    };
                    Value::Num(n)
                }
            }
            MX_CHAR => {
                let e = if r.done() {
                    Elem {
                        ty: MI_UINT16,
                        data: &[],
                    }
                } else {
                    r.elem()?
                };
                let chars: Vec<char> = if e.ty == MI_UTF8 {
                    String::from_utf8_lossy(e.data).chars().collect()
                } else {
                    let units: Vec<u16> = numbers(&e, self.be)?
                        .into_iter()
                        .map(|x| x as u16)
                        .collect();
                    if e.ty == MI_UTF32 {
                        units
                            .iter()
                            .map(|&u| char::from_u32(u as u32).unwrap_or('\u{fffd}'))
                            .collect()
                    } else {
                        // One char per UTF-16 unit (MATLAB's char is a
                        // code unit); lone surrogates become U+FFFD.
                        units
                            .iter()
                            .map(|&u| char::from_u32(u as u32).unwrap_or('\u{fffd}'))
                            .collect()
                    }
                };
                if chars.len() != numel {
                    return Err("character data does not match the dimensions".into());
                }
                Value::Char(Array::new(h.dims.clone(), chars))
            }
            MX_CELL => {
                if numel > data.len() / 8 {
                    return Err("cell data does not match the dimensions".into());
                }
                let mut items = Vec::with_capacity(numel);
                for _ in 0..numel {
                    let (_, d) = self.child(r, depth + 1)?;
                    items.push(self.nested(d, "cell element"));
                }
                Value::Cell(Array::new(h.dims.clone(), items))
            }
            MX_STRUCT | MX_OBJECT => {
                if h.class == MX_OBJECT {
                    h.class_name = text(&r.elem()?);
                }
                let len_e = r.elem()?;
                let len = *numbers(&len_e, self.be)?
                    .first()
                    .ok_or("invalid field names")? as usize;
                let names_e = r.elem()?;
                let fields: Vec<String> = if len == 0 {
                    vec![]
                } else {
                    names_e
                        .data
                        .chunks(len)
                        .map(|c| {
                            let end = c.iter().position(|&b| b == 0).unwrap_or(c.len());
                            String::from_utf8_lossy(&c[..end]).into_owned()
                        })
                        .filter(|s| !s.is_empty())
                        .collect()
                };
                let nf = fields.len();
                let enough = if nf == 0 {
                    numel <= MAX_EMPTY_ELEMS
                } else {
                    numel.checked_mul(nf).is_some_and(|k| k <= data.len() / 8)
                };
                if !enough {
                    return Err("struct data does not match the dimensions".into());
                }
                let mut elems = Vec::with_capacity(numel);
                for _ in 0..numel {
                    let mut e = Vec::with_capacity(nf);
                    for f in &fields {
                        let (_, d) = self.child(r, depth + 1)?;
                        e.push(self.nested(d, &format!("field '{f}'")));
                    }
                    elems.push(e);
                }
                Value::Struct(StructArray {
                    dims: h.dims.clone(),
                    fields,
                    elems,
                })
            }
            MX_SPARSE => {
                let ir = numbers(&r.elem()?, self.be)?;
                self.sparse(r, &h, ir)?
            }
            MX_FUNCTION => {
                let (_, inner) = self.child(r, depth + 1)?;
                let d = match inner {
                    Decoded::Value(v) => match mcos::handle_from(&v) {
                        Ok(f) => Decoded::Value(f),
                        Err(why) => Decoded::Skipped(why),
                    },
                    skipped => skipped,
                };
                return Ok((h, d));
            }
            c => return Err(format!("unknown array class {c}")),
        };
        if h.class == MX_OBJECT {
            self.warnings.push(format!(
                "Variable '{}' is an object of class '{}'; it was loaded as a struct.",
                h.name, h.class_name
            ));
        }
        Ok((h, Decoded::Value(decoded)))
    }

    /// The rest of a sparse array (after its row indices).
    fn sparse(&mut self, r: &mut Rd, h: &Header, ir: Vec<f64>) -> Result<Value, String> {
        if h.dims.len() != 2 {
            return Err("sparse arrays must be 2-D".into());
        }
        let (m, n) = (h.dims[0], h.dims[1]);
        let jc = numbers(&r.elem()?, self.be)?;
        let pr = if r.done() {
            vec![]
        } else {
            numbers(&r.elem()?, self.be)?
        };
        let pi = if h.flags & FLAG_COMPLEX != 0 && !r.done() {
            Some(numbers(&r.elem()?, self.be)?)
        } else {
            None
        };
        if jc.len() != n + 1 {
            return Err("invalid sparse column index".into());
        }
        let nnz = *jc.last().unwrap_or(&0.0) as usize;
        let logical = h.flags & FLAG_LOGICAL != 0;
        if nnz > ir.len() || (!logical && nnz > pr.len()) {
            return Err("invalid sparse data".into());
        }
        let mut entries: Vec<Entry> = Vec::with_capacity(nnz);
        for j in 0..n {
            let (a, b) = (jc[j] as usize, jc[j + 1] as usize);
            if a > b || b > nnz {
                return Err("invalid sparse column index".into());
            }
            for k in a..b {
                let i = ir[k] as usize;
                if ir[k] < 0.0 || i >= m {
                    return Err("invalid sparse row index".into());
                }
                let re = pr.get(k).copied().unwrap_or(1.0);
                let im = pi.as_ref().and_then(|p| p.get(k)).copied().unwrap_or(0.0);
                entries.push((i, j, re, im));
            }
        }
        // Sorts rows and drops explicit zeros: storage here is canonical.
        let s = Sparse::from_triplets(m, n, logical, entries);
        Ok(Value::Sparse(Box::new(s)))
    }

    /// Nested arrays that cannot be decoded become `[]` with a warning.
    fn nested(&mut self, d: Decoded, place: &str) -> Value {
        match d {
            Decoded::Value(v) => v,
            Decoded::Skipped(why) => {
                self.warnings
                    .push(format!("A {place} was replaced by []: {why}."));
                Value::empty()
            }
        }
    }
}

/// Where the header says the object data element starts, if anywhere usable.
fn subsystem_offset(word: &[u8], be: bool, len: usize) -> Option<usize> {
    let b: [u8; 8] = word.try_into().ok()?;
    let at = if be {
        u64::from_be_bytes(b)
    } else {
        u64::from_le_bytes(b)
    };
    let at = usize::try_from(at).ok()?;
    (at >= 128 && at + 8 <= len).then_some(at)
}

/// Header text and the byte order of a level 5 file.
fn level5_order(data: &[u8]) -> Result<bool, String> {
    match data.get(126..128) {
        Some(b"IM") => Ok(false),
        Some(b"MI") => Ok(true),
        _ => Err("not a MAT-file".into()),
    }
}

pub fn read(data: &[u8]) -> Result<MatData, String> {
    match sniff(data) {
        Kind::Level5 => read5(data),
        Kind::Level4 => read4(data),
        Kind::Hdf5 => Err(
            "MAT-file version 7.3 (HDF5) is not supported in the Rust engine yet; save it with -v7 instead"
                .into(),
        ),
        Kind::Unknown => Err("the file is not a MAT-file".into()),
    }
}

fn read5(data: &[u8]) -> Result<MatData, String> {
    let be = level5_order(data)?;
    let sub = &data[116..124];
    let mut out = MatData {
        subsystem: sub.iter().any(|&b| b != 0 && b != b' '),
        level: 5,
        ..MatData::default()
    };
    let mut warnings = Vec::new();
    let sub_at = subsystem_offset(sub, be, data.len());
    let objects = sub_at.and_then(|at| match mcos::Subsystem::parse(data, at, be) {
        Ok(s) => Some(s),
        Err(why) => {
            warnings.push(format!("The file's object data could not be read: {why}."));
            None
        }
    });
    let mut r = Rd {
        buf: data,
        pos: 128,
        be,
    };
    let mut object_bytes = 0;
    while !r.done() {
        let start = r.pos;
        if objects.is_some() && sub_at == Some(start) {
            r.elem()?;
            out.subsystem_range = Some((start, r.pos));
            continue;
        }
        let first = u32_at(&data[start..start + 4], be);
        let mut rd = Reader {
            be,
            warnings: &mut warnings,
            oversized: false,
            subsystem: objects.as_ref(),
            object_bytes,
        };
        let (h, d) = match first {
            MI_MATRIX => {
                let got = rd.child(&mut r, 0)?;
                object_bytes = rd.object_bytes;
                got
            }
            MI_COMPRESSED => {
                let e = r.elem()?;
                let inflated = zlib::inflate(e.data, zlib::MAX_OUTPUT)
                    .map_err(|m| format!("a compressed variable is damaged ({m})"))?;
                let mut inner = Rd {
                    buf: &inflated,
                    pos: 0,
                    be,
                };
                if inner.done() || u32_at(&inflated[0..4], be) != MI_MATRIX {
                    continue;
                }
                let got = rd.child(&mut inner, 0)?;
                object_bytes = rd.object_bytes;
                got
            }
            ty => {
                r.elem()?;
                warnings.push(format!(
                    "An element of unknown type {ty} at byte {start} was skipped; variables may be missing."
                ));
                continue;
            }
        };
        let raw = data[start..r.pos.max(start)].to_vec();
        if h.name.is_empty() {
            // MATLAB's subsystem data (no name): not a variable.
            continue;
        }
        let class = match h.class {
            MX_OBJECT | MX_OPAQUE => h.class_name.clone(),
            c if h.flags & FLAG_LOGICAL != 0 && c != MX_CHAR => "logical".into(),
            c => class_of_mx(c).into(),
        };
        let value = match d {
            Decoded::Value(v) => Some(v),
            Decoded::Skipped(why) => {
                warnings.push(format!("Variable '{}' was not loaded: {why}.", h.name));
                None
            }
        };
        out.vars.push(MatVar {
            name: h.name,
            value,
            global: h.global,
            class,
            dims: h.dims,
            raw,
        });
    }
    out.warnings = warnings;
    Ok(out)
}

// ---------------------------------------------------------------------------
// Level 5 writer
// ---------------------------------------------------------------------------

/// Collects what [`Writer::encode`] has to report about stored values, and
/// the objects ([`mcos`]) the file's last element must hold.
pub struct Writer<'w> {
    pub warnings: &'w mut Vec<String>,
    objects: Vec<Object>,
    classes: Vec<&'static str>,
}

/// One stored object: its class and encoded property value.
struct Object {
    class_id: usize,
    value: Vec<u8>,
}

/// Start of the small MAT stream inside the object data element.
const STREAM_HEAD: [u8; 8] = [0, 1, b'I', b'M', 0, 0, 0, 0];

fn pad8(v: &mut Vec<u8>) {
    while !v.len().is_multiple_of(8) {
        v.push(0);
    }
}

fn put_elem(out: &mut Vec<u8>, ty: u32, data: &[u8]) {
    if data.len() <= 4 && ty != MI_MATRIX {
        // Small data element format.
        out.extend_from_slice(&((data.len() as u32) << 16 | ty).to_le_bytes());
        let mut b = [0u8; 4];
        b[..data.len()].copy_from_slice(data);
        out.extend_from_slice(&b);
        return;
    }
    out.extend_from_slice(&ty.to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    pad8(out);
}

fn mi_of(class: NumClass) -> u32 {
    match class {
        NumClass::Double => MI_DOUBLE,
        NumClass::Single => MI_SINGLE,
        NumClass::Int8 => MI_INT8,
        NumClass::UInt8 => MI_UINT8,
        NumClass::Int16 => MI_INT16,
        NumClass::UInt16 => MI_UINT16,
        NumClass::Int32 => MI_INT32,
        NumClass::UInt32 => MI_UINT32,
        NumClass::Int64 => MI_INT64,
        NumClass::UInt64 => MI_UINT64,
    }
}

fn mx_of(class: NumClass) -> u8 {
    match class {
        NumClass::Double => MX_DOUBLE,
        NumClass::Single => MX_SINGLE,
        NumClass::Int8 => MX_INT8,
        NumClass::UInt8 => MX_UINT8,
        NumClass::Int16 => MX_INT16,
        NumClass::UInt16 => MX_UINT16,
        NumClass::Int32 => MX_INT32,
        NumClass::UInt32 => MX_UINT32,
        NumClass::Int64 => MX_INT64,
        NumClass::UInt64 => MX_UINT64,
    }
}

fn number_bytes(class: NumClass, xs: &[f64]) -> Vec<u8> {
    let mut b = Vec::with_capacity(xs.len() * 8);
    for &x in xs {
        match class {
            NumClass::Double => b.extend_from_slice(&x.to_le_bytes()),
            NumClass::Single => b.extend_from_slice(&(x as f32).to_le_bytes()),
            NumClass::Int8 => b.extend_from_slice(&(x as i8).to_le_bytes()),
            NumClass::UInt8 => b.extend_from_slice(&(x as u8).to_le_bytes()),
            NumClass::Int16 => b.extend_from_slice(&(x as i16).to_le_bytes()),
            NumClass::UInt16 => b.extend_from_slice(&(x as u16).to_le_bytes()),
            NumClass::Int32 => b.extend_from_slice(&(x as i32).to_le_bytes()),
            NumClass::UInt32 => b.extend_from_slice(&(x as u32).to_le_bytes()),
            NumClass::Int64 => b.extend_from_slice(&(x as i64).to_le_bytes()),
            NumClass::UInt64 => b.extend_from_slice(&(x as u64).to_le_bytes()),
        }
    }
    b
}

/// Largest row or column count level 5 stores (dimensions are `miINT32`).
const MAX_DIM: usize = i32::MAX as usize;

/// Size word of an element; level 5 sizes are 32 bits.
fn size_word(n: usize) -> Result<u32, String> {
    u32::try_from(n).map_err(|_| "it is larger than the 4 GiB a MAT-file element can hold".into())
}

/// Why this value cannot be stored, or `None` when it can. Cells and structs
/// can always be stored: an element that cannot be is left empty, and reported
/// by its path ([`Writer::encode`]).
pub fn unsupported(v: &Value) -> Option<String> {
    match v {
        Value::Cell(_) | Value::Struct(_) | Value::Str(_) => None,
        Value::Func(h) => mcos::handle_problem(h),
        Value::Object(_) | Value::HObject(_) | Value::ObjArr(_) => {
            let c = v.class_name();
            Some(match c.as_str() {
                "table" | "timetable" => {
                    format!("{c} objects cannot be saved to MAT files in the Rust engine yet")
                }
                _ => {
                    format!(
                        "objects of class '{c}' cannot be saved to MAT files in the Rust engine yet"
                    )
                }
            })
        }
        Value::Num(_) | Value::Bool(_) | Value::Char(_) | Value::Sparse(_) => {
            let d = v.dims_ref();
            d.iter().any(|&n| n > MAX_DIM).then(|| {
                let shape: Vec<String> = d.iter().map(usize::to_string).collect();
                format!(
                    "its size {} exceeds what MAT-files can store",
                    shape.join("x")
                )
            })
        }
    }
}

impl<'w> Writer<'w> {
    pub fn new(warnings: &'w mut Vec<String>) -> Writer<'w> {
        Writer {
            warnings,
            objects: Vec::new(),
            classes: Vec::new(),
        }
    }

    /// One complete `miMATRIX` element (tag included). `Err` says why the
    /// value cannot be stored; callers skip the variable and report it.
    /// An element of a cell or struct that cannot be stored is left empty, and
    /// `warnings` names its path: the rest of the value is still worth having.
    pub fn encode(&mut self, name: &str, v: &Value, global: bool) -> Result<Vec<u8>, String> {
        let mark = (self.objects.len(), self.classes.len());
        let out = match unsupported(v) {
            Some(why) => Err(why),
            None => self.encode_at(name, v, global, name),
        };
        if out.is_err() {
            self.objects.truncate(mark.0);
            self.classes.truncate(mark.1);
        }
        out
    }

    /// An element of a cell or struct, `path` being where it is.
    fn encode_child(&mut self, v: &Value, path: &str) -> Result<Vec<u8>, String> {
        let mark = (self.objects.len(), self.classes.len());
        let out = match unsupported(v) {
            Some(why) => Err(why),
            None => self.encode_at("", v, false, path),
        };
        match out {
            Ok(b) => Ok(b),
            Err(why) => {
                self.objects.truncate(mark.0);
                self.classes.truncate(mark.1);
                self.warnings
                    .push(format!("'{path}' was saved as []: {why}."));
                self.encode_at("", &Value::empty(), false, path)
            }
        }
    }

    fn encode_at(
        &mut self,
        name: &str,
        v: &Value,
        global: bool,
        path: &str,
    ) -> Result<Vec<u8>, String> {
        let mut body = Vec::new();
        let dims = v.dims();
        let nzmax = match v {
            // Octave and MATLAB both reserve room for one element when empty.
            Value::Sparse(s) => s.nnz().max(1) as u32,
            _ => 0,
        };
        let head = |body: &mut Vec<u8>, class: u8, flags: u32| {
            let mut f = Vec::with_capacity(8);
            let g = if global { FLAG_GLOBAL } else { 0 };
            f.extend_from_slice(&(class as u32 | flags | g).to_le_bytes());
            f.extend_from_slice(&nzmax.to_le_bytes());
            put_elem(body, MI_UINT32, &f);
            let d: Vec<u8> = dims
                .iter()
                .flat_map(|&n| (n as i32).to_le_bytes())
                .collect();
            put_elem(body, MI_INT32, &d);
            put_elem(body, MI_INT8, name.as_bytes());
        };
        match v {
            Value::Num(n) => {
                let flags = if n.im.is_some() { FLAG_COMPLEX } else { 0 };
                head(&mut body, mx_of(n.class), flags);
                put_elem(&mut body, mi_of(n.class), &number_bytes(n.class, &n.re));
                if let Some(im) = &n.im {
                    put_elem(&mut body, mi_of(n.class), &number_bytes(n.class, im));
                }
            }
            Value::Bool(b) => {
                head(&mut body, MX_UINT8, FLAG_LOGICAL);
                let d: Vec<u8> = b.data.iter().map(|&x| x as u8).collect();
                put_elem(&mut body, MI_UINT8, &d);
            }
            Value::Char(c) => {
                head(&mut body, MX_CHAR, 0);
                let mut wide = false;
                let d: Vec<u8> = c
                    .data
                    .iter()
                    .flat_map(|&ch| {
                        let u = ch as u32;
                        let u = if u > 0xffff {
                            wide = true;
                            0xfffd
                        } else {
                            u
                        };
                        (u as u16).to_le_bytes()
                    })
                    .collect();
                if wide {
                    self.warnings.push(format!(
                        "Characters outside the Basic Multilingual Plane in '{}' were saved as U+FFFD.",
                        if name.is_empty() { "a char array" } else { name }
                    ));
                }
                put_elem(&mut body, MI_UINT16, &d);
            }
            Value::Cell(c) => {
                head(&mut body, MX_CELL, 0);
                for (k, e) in c.data.iter().enumerate() {
                    let at = format!("{path}{{{}}}", k + 1);
                    body.extend_from_slice(&self.encode_child(e, &at)?);
                }
            }
            Value::Struct(s) => {
                head(&mut body, MX_STRUCT, 0);
                let longest = s.fields.iter().map(|f| f.len()).max().unwrap_or(0);
                let len = if longest < 32 { 32 } else { 64 };
                put_elem(&mut body, MI_INT32, &(len as i32).to_le_bytes());
                let mut names = Vec::with_capacity(len * s.fields.len());
                for f in &s.fields {
                    let mut b = f.as_bytes().to_vec();
                    b.truncate(len - 1);
                    b.resize(len, 0);
                    names.extend_from_slice(&b);
                }
                put_elem(&mut body, MI_INT8, &names);
                let scalar = s.elems.len() == 1;
                for (k, e) in s.elems.iter().enumerate() {
                    for (f, x) in s.fields.iter().zip(e) {
                        let at = if scalar {
                            format!("{path}.{f}")
                        } else {
                            format!("{path}({}).{f}", k + 1)
                        };
                        body.extend_from_slice(&self.encode_child(x, &at)?);
                    }
                }
            }
            Value::Sparse(s) => {
                let mut flags = 0;
                if s.logical {
                    flags |= FLAG_LOGICAL;
                }
                if s.im.is_some() {
                    flags |= FLAG_COMPLEX;
                }
                head(&mut body, MX_SPARSE, flags);
                let indices = |xs: &[usize]| -> Vec<u8> {
                    xs.iter().flat_map(|&i| (i as i32).to_le_bytes()).collect()
                };
                put_elem(&mut body, MI_INT32, &indices(&s.rowidx));
                put_elem(&mut body, MI_INT32, &indices(&s.colptr));
                put_elem(&mut body, MI_DOUBLE, &number_bytes(NumClass::Double, &s.re));
                if let Some(im) = &s.im {
                    put_elem(&mut body, MI_DOUBLE, &number_bytes(NumClass::Double, im));
                }
            }
            Value::Func(h) => {
                head(&mut body, MX_FUNCTION, 0);
                body.extend_from_slice(&self.handle_struct(h, path)?);
            }
            Value::Str(s) => {
                body = self.string_body(name, global, s)?;
            }
            Value::Object(_) | Value::HObject(_) | Value::ObjArr(_) => {
                return Err(format!("{} values cannot be stored", v.class_name()));
            }
        }
        let size = size_word(body.len())?;
        let mut out = Vec::with_capacity(body.len() + 8);
        out.extend_from_slice(&MI_MATRIX.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&body);
        Ok(out)
    }
}

/// The 128-byte level 5 header.
pub fn header5() -> Vec<u8> {
    let mut h = format!(
        "MATLAB 5.0 MAT-file, Platform: {}, Created on: {} by IndyMAT",
        platform(),
        now_text()
    )
    .into_bytes();
    h.truncate(116);
    h.resize(116, b' ');
    h.extend_from_slice(&[0u8; 8]);
    h.extend_from_slice(&0x0100u16.to_le_bytes());
    h.extend_from_slice(b"IM");
    h
}

fn platform() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "MACA64",
        ("macos", _) => "MACI64",
        ("windows", _) => "PCWIN64",
        ("linux", "aarch64") => "GLNXA64",
        _ => "GLNXA64",
    }
}

/// `Thu Oct 30 08:00:00 2026` in UTC.
fn now_text() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    // Civil date from days since 1970-01-01 (proleptic Gregorian).
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    const WD: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MO: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!(
        "{} {} {:2} {:02}:{:02}:{:02} {}",
        WD[days.rem_euclid(7) as usize],
        MO[(m - 1) as usize],
        d,
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        y
    )
}

/// Record in a file's header where its object data element starts.
pub fn set_subsystem_offset(file: &mut [u8], at: usize) {
    file[116..124].copy_from_slice(&(at as u64).to_le_bytes());
}

/// A top-level element, compressed (`-v7`) or not (`-v6`).
pub fn wrap(element: Vec<u8>, compress: bool) -> Result<Vec<u8>, String> {
    if !compress {
        return Ok(element);
    }
    let z = zlib::deflate(&element);
    let size = size_word(z.len())?;
    let mut out = Vec::with_capacity(z.len() + 8);
    out.extend_from_slice(&MI_COMPRESSED.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&z);
    Ok(out)
}

// ---------------------------------------------------------------------------
// Level 4
// ---------------------------------------------------------------------------

struct Head4 {
    be: bool,
    prec: u32,
    text: bool,
    sparse: bool,
    rows: usize,
    cols: usize,
    imag: bool,
    namelen: usize,
}

fn level4_header(data: &[u8], pos: usize) -> Option<Head4> {
    let h = data.get(pos..pos + 20)?;
    for be in [false, true] {
        let w = |i: usize| u32_at(&h[i * 4..i * 4 + 4], be);
        let ty = w(0);
        if ty > 4052 {
            continue;
        }
        let (m, o, p, t) = (ty / 1000, ty / 100 % 10, ty / 10 % 10, ty % 10);
        // M: 0 little-endian, 1 big-endian; O must be 0; P 0..5; T 0..2.
        if (m != 0 && m != 1) || (m == 1) != be || o != 0 || p > 5 || t > 2 {
            continue;
        }
        let (rows, cols, imag, namelen) = (w(1) as usize, w(2) as usize, w(3), w(4) as usize);
        if imag > 1 || namelen == 0 || namelen > 4096 {
            continue;
        }
        return Some(Head4 {
            be,
            prec: p,
            text: t == 1,
            sparse: t == 2,
            rows,
            cols,
            imag: imag == 1,
            namelen,
        });
    }
    None
}

fn read4(data: &[u8]) -> Result<MatData, String> {
    let mut out = MatData {
        level: 4,
        ..MatData::default()
    };
    let mut pos = 0;
    while pos < data.len() {
        let h = level4_header(data, pos).ok_or("invalid level 4 MAT-file header")?;
        pos += 20;
        let name_b = data
            .get(pos..pos + h.namelen)
            .ok_or("truncated level 4 file")?;
        let end = name_b.iter().position(|&b| b == 0).unwrap_or(name_b.len());
        let name = String::from_utf8_lossy(&name_b[..end]).into_owned();
        pos += h.namelen;
        let width = match h.prec {
            0 => 8,
            1 | 2 => 4,
            3 | 4 => 2,
            _ => 1,
        };
        let ty = match h.prec {
            0 => MI_DOUBLE,
            1 => MI_SINGLE,
            2 => MI_INT32,
            3 => MI_INT16,
            4 => MI_UINT16,
            _ => MI_UINT8,
        };
        let n = h.rows.checked_mul(h.cols).ok_or("dimensions too large")?;
        let bytes = n.checked_mul(width).ok_or("dimensions too large")?;
        let mut part = || -> Result<Vec<f64>, String> {
            let d = data.get(pos..pos + bytes).ok_or("truncated level 4 file")?;
            pos += bytes;
            numbers(&Elem { ty, data: d }, h.be)
        };
        let re = part()?;
        let im = if h.imag { Some(part()?) } else { None };
        let mut dims = vec![h.rows, h.cols];
        let value = if h.sparse {
            let s = sparse4(&re, h.rows, h.cols)?;
            dims = s.dims.to_vec();
            Some(Value::Sparse(Box::new(s)))
        } else if h.text {
            Some(Value::Char(Array::new(
                dims.clone(),
                re.iter()
                    .map(|&x| char::from_u32(x as u32).unwrap_or('\u{fffd}'))
                    .collect(),
            )))
        } else {
            let mut x = Num::real(dims.clone(), re);
            x.im = im;
            x.narrow();
            Some(Value::Num(x))
        };
        out.vars.push(MatVar {
            name,
            class: if h.text { "char" } else { "double" }.into(),
            dims: indy_value::norm_dims(dims),
            value,
            global: false,
            raw: vec![],
        });
    }
    Ok(out)
}

/// Most columns a level 4 sparse matrix may declare (2 GiB of column table).
const MAX_LEVEL4_COLUMNS: usize = 1 << 28;

/// A level 4 sparse matrix: rows `i j re [im]` (1-based), the last row holding
/// the size and no element.
fn sparse4(data: &[f64], rows: usize, cols: usize) -> Result<Sparse, String> {
    if rows == 0 || !(3..=4).contains(&cols) {
        return Err("invalid level 4 sparse matrix".into());
    }
    let nnz = rows - 1;
    let size = |x: f64| {
        (x >= 0.0 && x.fract() == 0.0 && x <= MAX_DIM as f64)
            .then_some(x as usize)
            .ok_or("invalid level 4 sparse matrix")
    };
    let (m, n) = (size(data[nnz])?, size(data[rows + nnz])?);
    let mut entries: Vec<Entry> = Vec::with_capacity(nnz);
    for k in 0..nnz {
        let (i, j) = (size(data[k])?, size(data[rows + k])?);
        if i == 0 || j == 0 || i > m || j > n {
            return Err("invalid level 4 sparse matrix".into());
        }
        let im = if cols == 4 { data[3 * rows + k] } else { 0.0 };
        entries.push((i - 1, j - 1, data[2 * rows + k], im));
    }
    // The column table is allocated and, unlike level 5, nothing in the file
    // backs this count.
    if n > MAX_LEVEL4_COLUMNS {
        return Err("the level 4 sparse matrix has too many columns".into());
    }
    Ok(Sparse::from_triplets(m, n, false, entries))
}

/// One level 4 matrix; `Err` names why the value cannot be stored.
pub fn encode4(name: &str, v: &Value) -> Result<Vec<u8>, String> {
    if let Value::Sparse(s) = v {
        return encode4_sparse(name, s);
    }
    let dims = v.dims();
    if dims.len() > 2 {
        return Err("level 4 MAT-files hold only 2-D arrays".into());
    }
    let (re, im, text) = match v {
        Value::Num(n) => (n.re.clone(), n.im.clone(), false),
        Value::Bool(b) => (
            b.data.iter().map(|&x| x as u8 as f64).collect(),
            None,
            false,
        ),
        Value::Char(c) => (
            c.data.iter().map(|&ch| ch as u32 as f64).collect(),
            None,
            true,
        ),
        _ => {
            return Err(format!(
                "level 4 MAT-files cannot hold {} values",
                v.class_name()
            ));
        }
    };
    let mut out = Vec::new();
    let ty: u32 = if text { 1 } else { 0 };
    for w in [
        ty,
        dims[0] as u32,
        dims[1] as u32,
        im.is_some() as u32,
        name.len() as u32 + 1,
    ] {
        out.extend_from_slice(&w.to_le_bytes());
    }
    out.extend_from_slice(name.as_bytes());
    out.push(0);
    for x in &re {
        out.extend_from_slice(&x.to_le_bytes());
    }
    if let Some(im) = &im {
        for x in im {
            out.extend_from_slice(&x.to_le_bytes());
        }
    }
    Ok(out)
}

/// Level 4 has no logical class: a logical sparse matrix is stored as double,
/// as a dense logical array is.
fn encode4_sparse(name: &str, s: &Sparse) -> Result<Vec<u8>, String> {
    let (rows, cols) = (s.nnz() + 1, if s.im.is_some() { 4u32 } else { 3 });
    let entries = s.entries();
    let mut out = Vec::with_capacity(21 + name.len() + rows * cols as usize * 8);
    for w in [2u32, rows as u32, cols, 0, name.len() as u32 + 1] {
        out.extend_from_slice(&w.to_le_bytes());
    }
    out.extend_from_slice(name.as_bytes());
    out.push(0);
    let mut column = |at: &dyn Fn(&Entry) -> f64, last: f64| {
        for e in &entries {
            out.extend_from_slice(&at(e).to_le_bytes());
        }
        out.extend_from_slice(&last.to_le_bytes());
    };
    column(&|e| (e.0 + 1) as f64, s.dims[0] as f64);
    column(&|e| (e.1 + 1) as f64, s.dims[1] as f64);
    column(&|e| e.2, 0.0);
    if s.im.is_some() {
        column(&|e| e.3, 0.0);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round(v: &Value, compress: bool) -> Value {
        let mut warnings = Vec::new();
        let mut w = Writer::new(&mut warnings);
        let mut file = header5();
        file.extend(wrap(w.encode("v", v, false).unwrap(), compress).unwrap());
        let d = read(&file).unwrap();
        assert_eq!(d.vars.len(), 1);
        d.vars[0].value.clone().unwrap()
    }

    #[test]
    fn values_round_trip() {
        let mut s = StructArray::empty_with(vec!["a".into(), "bb".into()], vec![1, 2]);
        s.elems[0][0] = Value::num(1.0);
        s.elems[1][1] = Value::str("x");
        let vals = vec![
            Value::matrix(vec![2, 3], vec![1.0, 2.0, 3.0, 4.0, 5.5, -6.0]),
            Value::empty(),
            Value::complex(1.0, -2.0),
            Value::Num(Num::real(vec![1, 2], vec![-3.0, 100.0]).with_class(NumClass::Int8)),
            Value::Bool(Array::row(vec![true, false])),
            Value::str("İzmir"),
            Value::str(""),
            Value::cell_row(vec![Value::num(1.0), Value::str("iki")]),
            Value::Struct(s),
            Value::Struct(StructArray::empty_with(vec!["p".into()], vec![0, 0])),
        ];
        for v in &vals {
            for c in [false, true] {
                assert_eq!(&round(v, c), v);
            }
        }
    }

    #[test]
    fn damaged_files_are_errors() {
        let mut warnings = Vec::new();
        let mut w = Writer::new(&mut warnings);
        let mut file = header5();
        file.extend(
            wrap(
                w.encode("v", &Value::matrix(vec![3, 3], vec![1.0; 9]), false)
                    .unwrap(),
                true,
            )
            .unwrap(),
        );
        for cut in 0..file.len() {
            let _ = read(&file[..cut]);
        }
        for i in 128..file.len() {
            let mut f = file.clone();
            f[i] ^= 0x5a;
            let _ = read(&f);
        }
        assert!(read(b"hello").is_err());
    }

    #[test]
    fn unknown_elements_are_reported() {
        let mut warnings = Vec::new();
        let mut w = Writer::new(&mut warnings);
        let mut file = header5();
        file.extend_from_slice(&40u32.to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend(wrap(w.encode("v", &Value::num(1.0), false).unwrap(), false).unwrap());
        let d = read(&file).unwrap();
        assert_eq!(d.vars.len(), 1);
        assert_eq!(d.warnings.len(), 1);
        assert!(d.warnings[0].contains("unknown type 40"));
    }

    #[test]
    fn level4_round_trip() {
        let v = Value::matrix(vec![2, 2], vec![1.0, 2.0, 3.0, 4.0]);
        let mut f = encode4("a", &v).unwrap();
        f.extend(encode4("t", &Value::str("hey")).unwrap());
        assert_eq!(sniff(&f), Kind::Level4);
        let d = read(&f).unwrap();
        assert_eq!(d.vars[0].value.as_ref(), Some(&v));
        assert_eq!(d.vars[1].value.as_ref(), Some(&Value::str("hey")));
    }

    fn sparse_of(m: usize, n: usize, logical: bool, e: &[Entry]) -> Value {
        Value::Sparse(Box::new(Sparse::from_sorted(m, n, logical, e)))
    }

    fn sparse_values() -> Vec<Value> {
        vec![
            sparse_of(3, 2, false, &[(0, 0, 1.5, 0.0), (2, 1, -2.0, 0.0)]),
            sparse_of(2, 2, false, &[(0, 0, 1.0, 2.0), (1, 1, 0.0, -3.0)]),
            sparse_of(2, 3, true, &[(1, 0, 1.0, 0.0), (0, 2, 1.0, 0.0)]),
            sparse_of(2, 3, false, &[]),
            sparse_of(0, 0, false, &[]),
            sparse_of(0, 4, true, &[]),
            sparse_of(1, 5, false, &[(0, 1, 7.0, 0.0), (0, 4, 8.0, 0.0)]),
            sparse_of(1 << 30, 1 << 20, false, &[(1 << 29, 7, 3.0, 0.0)]),
        ]
    }

    #[test]
    fn sparse_round_trips() {
        for v in &sparse_values() {
            for c in [false, true] {
                assert_eq!(&round(v, c), v);
            }
        }
    }

    #[test]
    fn sparse_inside_containers_round_trips() {
        let s = sparse_of(2, 2, false, &[(0, 1, 4.0, 0.0)]);
        let v = Value::cell_row(vec![s.clone(), Value::cell_row(vec![s.clone()])]);
        assert_eq!(round(&v, true), v);
        let mut st = StructArray::new_scalar();
        st.set(0, "f", s);
        let v = Value::Struct(st);
        assert_eq!(round(&v, false), v);
    }

    #[test]
    fn sparse_level4_round_trips() {
        for v in sparse_values()
            .iter()
            .filter(|v| v.class_name() == "double")
        {
            let d = read(&encode4("a", v).unwrap()).unwrap();
            assert_eq!(d.vars[0].value.as_ref(), Some(v));
            assert_eq!(d.vars[0].dims, v.dims());
        }
        let d = read(&encode4("a", &sparse_values()[2]).unwrap()).unwrap();
        assert_eq!(d.vars[0].class, "double");
        assert!(d.vars[0].value.as_ref().unwrap().is_sparse());
    }

    #[test]
    fn octave_logical_sparse_layout_is_recognised() {
        // mxUINT8 with the logical flag, ir holding as many rows as the array
        // has elements: 1x2 with both set, so ir = [0 0].
        let mut body = Vec::new();
        let mut flags = Vec::new();
        flags.extend_from_slice(&(MX_UINT8 as u32 | FLAG_LOGICAL).to_le_bytes());
        flags.extend_from_slice(&2u32.to_le_bytes());
        put_elem(&mut body, MI_UINT32, &flags);
        let ints = |xs: &[i32]| -> Vec<u8> { xs.iter().flat_map(|x| x.to_le_bytes()).collect() };
        put_elem(&mut body, MI_INT32, &ints(&[1, 2]));
        put_elem(&mut body, MI_INT8, b"v");
        put_elem(&mut body, MI_INT32, &ints(&[0, 0]));
        put_elem(&mut body, MI_INT32, &ints(&[0, 1, 2]));
        put_elem(
            &mut body,
            MI_DOUBLE,
            &number_bytes(NumClass::Double, &[1.0, 1.0]),
        );
        let mut file = header5();
        file.extend_from_slice(&MI_MATRIX.to_le_bytes());
        file.extend_from_slice(&(body.len() as u32).to_le_bytes());
        file.extend_from_slice(&body);
        let d = read(&file).unwrap();
        assert_eq!(
            d.vars[0].value.as_ref(),
            Some(&sparse_of(
                1,
                2,
                true,
                &[(0, 0, 1.0, 0.0), (0, 1, 1.0, 0.0)]
            ))
        );
    }

    #[test]
    fn damaged_sparse_never_panics() {
        for v in &sparse_values()[..4] {
            let mut warnings = Vec::new();
            let mut w = Writer::new(&mut warnings);
            let mut file = header5();
            file.extend(w.encode("v", v, false).unwrap());
            for cut in 0..file.len() {
                let _ = read(&file[..cut]);
            }
            for i in 128..file.len() {
                for x in [0x5a, 0xff, 0x80] {
                    let mut f = file.clone();
                    f[i] ^= x;
                    let _ = read(&f);
                }
            }
            let four = encode4("v", v).unwrap();
            for i in 0..four.len() {
                let mut f = four.clone();
                f[i] ^= 0x41;
                let _ = read(&f);
            }
        }
    }

    fn file_of(w: &Writer, element: Vec<u8>, compress: bool) -> Vec<u8> {
        let mut file = header5();
        file.extend(wrap(element, compress).unwrap());
        if let Some(sub) = w.subsystem() {
            let at = file.len();
            set_subsystem_offset(&mut file, at);
            file.extend(wrap(sub, compress).unwrap());
        }
        file
    }

    fn named(name: &str) -> Value {
        Value::Func(std::rc::Rc::new(indy_value::Handle {
            name: name.into(),
            anon: None,
            ctx: None,
        }))
    }

    fn stored(v: &Value, compress: bool) -> (Value, Vec<String>) {
        let mut warnings = Vec::new();
        let mut w = Writer::new(&mut warnings);
        let e = w.encode("v", v, false).unwrap();
        let file = file_of(&w, e, compress);
        let d = read(&file).unwrap();
        assert_eq!(d.warnings, Vec::<String>::new());
        assert_eq!(d.vars.len(), 1);
        (d.vars[0].value.clone().unwrap(), warnings)
    }

    #[test]
    fn strings_round_trip() {
        let vals = vec![
            Value::Str(Array::new(vec![1, 1], vec![Some("metin".to_string())])),
            Value::Str(Array::new(vec![1, 1], vec![Some(String::new())])),
            Value::Str(Array::new(vec![1, 1], vec![None])),
            Value::Str(Array::new(
                vec![2, 2],
                vec![
                    Some("İzmir".to_string()),
                    None,
                    Some("𝄞a".to_string()),
                    Some(String::new()),
                ],
            )),
            Value::Str(Array::new(vec![0, 0], vec![])),
            Value::Str(Array::new(
                vec![1, 2, 2],
                vec![
                    Some("a".to_string()),
                    Some("b".to_string()),
                    Some("c".to_string()),
                    Some("d".to_string()),
                ],
            )),
        ];
        for v in &vals {
            for c in [false, true] {
                assert_eq!(&stored(v, c).0, v);
            }
        }
    }

    #[test]
    fn named_handles_round_trip_in_containers() {
        let v = Value::cell_row(vec![named("sin"), Value::num(2.0), named("pkg.fn")]);
        for c in [false, true] {
            assert_eq!(stored(&v, c).0, v);
        }
    }

    #[test]
    fn unstorable_elements_stay_empty_and_named() {
        let bound = Value::Func(std::rc::Rc::new(indy_value::Handle {
            name: String::new(),
            anon: Some(indy_value::Anon {
                params: vec![],
                body: std::rc::Rc::new(indy_syntax::ast::Expr {
                    kind: indy_syntax::ast::ExprKind::Ident("o".into()),
                    span: indy_syntax::Span { start: 0, end: 0 },
                }),
                captured: vec![],
                text: "@o.m".into(),
            }),
            ctx: None,
        }));
        let mut st = StructArray::new_scalar();
        st.set(0, "ok", Value::num(1.0));
        st.set(
            0,
            "bad",
            Value::cell_row(vec![Value::num(2.0), bound.clone()]),
        );
        let (back, warnings) = stored(&Value::Struct(st), true);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].starts_with("'v.bad{2}' was saved as []"),
            "{warnings:?}"
        );
        let Value::Struct(b) = back else {
            panic!("a struct was expected")
        };
        assert_eq!(b.get(0, "ok"), Some(&Value::num(1.0)));
        assert_eq!(
            b.get(0, "bad"),
            Some(&Value::cell_row(vec![Value::num(2.0), Value::empty()]))
        );
        assert!(unsupported(&bound).is_some());
        assert!(unsupported(&sparse_of(1 << 31, 1, false, &[])).is_some());
        assert!(unsupported(&sparse_of(MAX_DIM, 1, false, &[])).is_none());
    }

    #[test]
    fn damaged_object_data_never_panics() {
        let mut warnings = Vec::new();
        let mut w = Writer::new(&mut warnings);
        let v = Value::cell_row(vec![
            named("sin"),
            Value::Str(Array::new(vec![1, 2], vec![Some("ab".to_string()), None])),
        ]);
        let e = w.encode("v", &v, false).unwrap();
        let file = file_of(&w, e, false);
        for cut in 0..file.len() {
            let _ = read(&file[..cut]);
        }
        for i in 116..file.len() {
            for x in [0x5a, 0xff, 0x80, 0x01] {
                let mut f = file.clone();
                f[i] ^= x;
                let _ = read(&f);
            }
        }
    }

    #[test]
    fn element_sizes_stop_at_32_bits() {
        assert_eq!(size_word(u32::MAX as usize), Ok(u32::MAX));
        assert!(size_word(u32::MAX as usize + 1).is_err());
    }
}
