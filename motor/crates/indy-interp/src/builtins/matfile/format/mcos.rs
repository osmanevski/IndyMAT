//! MATLAB's object data in level 5 files: function handles and string arrays.
//!
//! MathWorks does not document this part. The layout here was measured on
//! MATLAB R2025b files (anonymous handles with and without a captured
//! variable, a scalar string), and what those files do not show is assumed:
//! see the notes on each item.
//!
//! - A handle is `mxFUNCTION` holding a struct with `matlabroot`, `separator`,
//!   `sentinel` and `function_handle` (`function`, `type`, `file`, ...).
//! - An object in a variable is `mxOPAQUE` (`MCOS`, class name) holding a
//!   reference `[0xDD000000 ndims dims... object_id class_id]`. A captured
//!   workspace is such an object, class `function_handle_workspace`.
//! - Object contents live in one unnamed `uint8` element, the last of the file,
//!   whose offset is in the file header. It is itself a small MAT stream: a
//!   struct `MCOS` holding `FileWrapper__`, a cell of the metadata, `[]`, the
//!   property values, and per-class tables.

use std::path::PathBuf;
use std::rc::Rc;

use indy_syntax::ast;
use indy_value::{Anon, Array, Handle, StructArray, Value};

use super::*;

const REF_MAGIC: u32 = 0xDD00_0000;
const WORKSPACE_CLASS: &str = "function_handle_workspace";
const STRING_CLASS: &str = "string";
/// `FileWrapper__` metadata version of the measured files.
const WRAPPER_VERSION: u32 = 4;
/// Element count that marks a `<missing>` string (assumed: the measured file
/// holds one non-missing scalar).
const MISSING_COUNT: u64 = u64::MAX;
/// Every measured anonymous function text starts with `sf%0`. What the number
/// counts is unknown (no measured file has two handles), so it is always 0.
const ANON_PREFIX: &str = "sf%0";
/// Bytes of object data one file may make the reader decode: a reference can
/// be repeated, so nested objects could otherwise multiply without limit.
const MAX_OBJECT_BYTES: usize = 1 << 31;

// ---------------------------------------------------------------------------
// Elements
// ---------------------------------------------------------------------------

fn array(class: u8, dims: &[usize], name: &[u8], data: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(data.len() + 48);
    let mut flags = Vec::with_capacity(8);
    flags.extend_from_slice(&(class as u32).to_le_bytes());
    flags.extend_from_slice(&0u32.to_le_bytes());
    put_elem(&mut body, MI_UINT32, &flags);
    let d: Vec<u8> = dims
        .iter()
        .flat_map(|&n| (n as i32).to_le_bytes())
        .collect();
    put_elem(&mut body, MI_INT32, &d);
    put_elem(&mut body, MI_INT8, name);
    body.extend_from_slice(data);
    matrix_element(&body)
}

fn matrix_element(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 8);
    out.extend_from_slice(&MI_MATRIX.to_le_bytes());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
    out
}

fn text_array(s: &str) -> Vec<u8> {
    let n = s.encode_utf16().count();
    let dims = [if n == 0 { 0 } else { 1 }, n];
    let mut data = Vec::new();
    put_elem(&mut data, MI_UTF8, s.as_bytes());
    array(MX_CHAR, &dims, b"", &data)
}

fn struct_array(dims: &[usize], fields: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut data = Vec::new();
    let longest = fields.iter().map(|(f, _)| f.len()).max().unwrap_or(0);
    // As measured: `MCOS` and `a` both get 5, no fields 1.
    let len = if fields.is_empty() {
        1
    } else {
        (longest + 1).max(5)
    };
    put_elem(&mut data, MI_INT32, &(len as i32).to_le_bytes());
    let mut names = Vec::with_capacity(len * fields.len());
    for (f, _) in fields {
        let mut b = f.as_bytes().to_vec();
        b.resize(len, 0);
        names.extend_from_slice(&b);
    }
    put_elem(&mut data, MI_INT8, &names);
    for (_, v) in fields {
        data.extend_from_slice(v);
    }
    array(MX_STRUCT, dims, b"", &data)
}

fn cell_column(items: &[Vec<u8>]) -> Vec<u8> {
    array(MX_CELL, &[items.len(), 1], b"", &items.concat())
}

fn words32(xs: &[u32]) -> Vec<u8> {
    xs.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// `[0xDD000000 2 1 1 object class]`: a scalar object.
fn reference(object: usize, class: usize) -> Vec<u8> {
    let mut data = Vec::new();
    put_elem(
        &mut data,
        MI_UINT32,
        &words32(&[REF_MAGIC, 2, 1, 1, object as u32, class as u32]),
    );
    array(MX_UINT32, &[6, 1], b"", &data)
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

/// Why a handle cannot be stored in a way that works after `load`.
pub(super) fn handle_problem(h: &Handle) -> Option<String> {
    if h.ctx
        .as_ref()
        .is_some_and(|c| c.is::<crate::NestedHandleCtx>())
    {
        return Some(format!(
            "the handle to nested function '{}' cannot be saved: the Rust engine cannot restore the running workspace it shares",
            h.name
        ));
    }
    if let Some(a) = &h.anon {
        if !a.text.starts_with("@(") {
            return Some(format!(
                "the object method handle {} cannot be saved to MAT files in the Rust engine",
                a.text
            ));
        }
        return a.captured.iter().find_map(|(n, v)| {
            let why = captured_problem(v)?;
            Some(format!(
                "the anonymous function {} captures '{n}', which cannot be saved: {why}",
                a.text
            ))
        });
    }
    let ctx = handle_file(h)?;
    let session_only = ctx.functions.contains_key(&h.name) && !ctx.file_backed;
    session_only.then(|| {
        format!(
            "the handle to '{}' cannot be saved: that function exists only in this session, not in a file",
            h.name
        )
    })
}

fn handle_file(h: &Handle) -> Option<Rc<crate::FileCtx>> {
    h.ctx.clone()?.downcast::<crate::FileCtx>().ok()
}

fn is_main(ctx: &crate::FileCtx, name: &str) -> bool {
    ctx.file_backed && ctx.main.as_ref().is_some_and(|m| m.name.dotted() == name)
}

/// A captured value has no path a load could leave empty and still work, so
/// anything inside it that cannot be stored makes the whole handle unstorable.
fn captured_problem(v: &Value) -> Option<String> {
    match v {
        Value::Cell(c) => c.data.iter().find_map(captured_problem),
        Value::Struct(s) => s.elems.iter().flatten().find_map(captured_problem),
        other => unsupported(other),
    }
}

impl Writer<'_> {
    /// Register an object; contents follow in `objects[id - 1].value`.
    fn new_object(&mut self, class: &'static str) -> (usize, usize) {
        let class_id = match self.classes.iter().position(|c| *c == class) {
            Some(k) => k + 1,
            None => {
                self.classes.push(class);
                self.classes.len()
            }
        };
        self.objects.push(Object {
            class_id,
            value: Vec::new(),
        });
        (self.objects.len(), class_id)
    }

    /// Contents of an `mxOPAQUE` element.
    fn opaque_body(name: &str, global: bool, class: &str, refer: &[u8]) -> Vec<u8> {
        let mut body = Vec::with_capacity(refer.len() + 64);
        let g = if global { FLAG_GLOBAL } else { 0 };
        let mut f = Vec::with_capacity(8);
        f.extend_from_slice(&(MX_OPAQUE as u32 | g).to_le_bytes());
        f.extend_from_slice(&0u32.to_le_bytes());
        put_elem(&mut body, MI_UINT32, &f);
        put_elem(&mut body, MI_INT8, name.as_bytes());
        put_elem(&mut body, MI_INT8, b"MCOS");
        put_elem(&mut body, MI_INT8, class.as_bytes());
        body.extend_from_slice(refer);
        body
    }

    /// A string array as `mxOPAQUE`, its data in property `any`:
    /// `uint64 [1 ndims dims... counts... utf16]`. Measured on a scalar;
    /// arrays and missing elements follow the same reading and are assumed.
    pub(super) fn string_body(
        &mut self,
        name: &str,
        global: bool,
        s: &Array<Option<String>>,
    ) -> Result<Vec<u8>, String> {
        let mut words: Vec<u64> = vec![1, s.dims.len() as u64];
        words.extend(s.dims.iter().map(|&d| d as u64));
        let mut units: Vec<u16> = Vec::new();
        for e in &s.data {
            match e {
                Some(t) => {
                    let before = units.len();
                    units.extend(t.encode_utf16());
                    words.push((units.len() - before) as u64);
                }
                None => words.push(MISSING_COUNT),
            }
        }
        let mut bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        bytes.extend(units.iter().flat_map(|u| u.to_le_bytes()));
        while !bytes.len().is_multiple_of(8) {
            bytes.push(0);
        }
        size_word(bytes.len())?;
        let mut data = Vec::new();
        put_elem(&mut data, MI_UINT64, &bytes);
        let value = array(MX_UINT64, &[1, bytes.len() / 8], b"", &data);
        let (id, class_id) = self.new_object(STRING_CLASS);
        self.objects[id - 1].value = value;
        Ok(Self::opaque_body(
            name,
            global,
            STRING_CLASS,
            &reference(id, class_id),
        ))
    }

    /// The struct inside an `mxFUNCTION` element.
    pub(super) fn handle_struct(&mut self, h: &Handle, path: &str) -> Result<Vec<u8>, String> {
        let fh = match &h.anon {
            Some(a) => {
                let (id, class_id) = self.new_object(WORKSPACE_CLASS);
                let ws = self.workspace_value(a, class_id, path)?;
                self.objects[id - 1].value = ws;
                self.warn_local_calls(h, a, path);
                let body = Self::opaque_body("", false, WORKSPACE_CLASS, &reference(id, class_id));
                struct_array(
                    &[1, 1],
                    &[
                        ("function", text_array(&format!("{ANON_PREFIX}{}", a.text))),
                        ("type", text_array("anonymous")),
                        ("file", text_array("")),
                        ("within_file_path", text_array("")),
                        ("workspace", matrix_element(&body)),
                    ],
                )
            }
            None => named_handle(h),
        };
        // No MATLAB here to name: the field locates files of the saving
        // installation, and an empty one claims none.
        Ok(struct_array(
            &[1, 1],
            &[
                ("matlabroot", text_array("")),
                ("separator", text_array(std::path::MAIN_SEPARATOR_STR)),
                ("sentinel", text_array("@")),
                ("function_handle", fh),
            ],
        ))
    }

    /// `{reference-to-itself, struct of captured variables}`.
    fn workspace_value(
        &mut self,
        a: &Anon,
        class_id: usize,
        path: &str,
    ) -> Result<Vec<u8>, String> {
        let mut fields = Vec::with_capacity(a.captured.len());
        for (n, v) in &a.captured {
            let at = format!("{path} (captured '{n}')");
            fields.push((n.as_str(), self.encode_at("", v, false, &at)?));
        }
        Ok(cell_column(&[
            reference(0, class_id),
            struct_array(&[1, 1], &fields),
        ]))
    }

    /// A local function is not reachable from a handle restored by `load`:
    /// the text is all that is stored.
    fn warn_local_calls(&mut self, h: &Handle, a: &Anon, path: &str) {
        let Some(ctx) = handle_file(h) else { return };
        let mut ids = Vec::new();
        crate::eval::collect_idents(&a.body, &mut ids);
        ids.sort();
        ids.dedup();
        for id in ids {
            let is_param = a.params.contains(&id);
            let is_captured = a.captured.iter().any(|(n, _)| *n == id);
            if !is_param && !is_captured && ctx.functions.contains_key(&id) && !is_main(&ctx, &id) {
                self.warnings.push(format!(
                    "'{path}' calls local function '{id}', which will not be found after load."
                ));
            }
        }
    }

    pub fn has_objects(&self) -> bool {
        !self.objects.is_empty()
    }

    /// The unnamed `uint8` element holding all objects, or `None` without any.
    pub fn subsystem(&self) -> Option<Vec<u8>> {
        if self.objects.is_empty() {
            return None;
        }
        let n = self.classes.len() + 1;
        let none = || struct_array(&[1, 0], &[]);
        let per_class = || cell_column(&vec![none(); n]);
        let mut zeros = Vec::new();
        put_elem(&mut zeros, MI_INT32, &vec![0u8; n * 4]);
        let mut meta = Vec::new();
        put_elem(&mut meta, MI_UINT8, &self.metadata());
        let mut cells = vec![
            array(MX_UINT8, &[self.metadata().len(), 1], b"", &meta),
            matrix_element(&[]),
        ];
        cells.extend(self.objects.iter().map(|o| o.value.clone()));
        cells.push(per_class());
        cells.push(array(MX_INT32, &[n, 1], b"", &zeros));
        cells.push(per_class());
        let wrapper = Self::opaque_body("", false, "FileWrapper__", &cell_column(&cells));
        let main = struct_array(&[1, 1], &[("MCOS", matrix_element(&wrapper))]);
        // A second, empty stream of the same shape ends every measured file.
        let empty_stream = {
            let inner = struct_array(&[1, 1], &[("MCOS", matrix_element(&[]))]);
            let mut b = STREAM_HEAD.to_vec();
            b.extend_from_slice(&inner);
            let mut d = Vec::new();
            put_elem(&mut d, MI_UINT8, &b);
            array(MX_UINT8, &[1, b.len()], b"", &d)
        };
        let mut stream = STREAM_HEAD.to_vec();
        stream.extend_from_slice(&main);
        stream.extend_from_slice(&empty_stream);
        let mut d = Vec::new();
        put_elem(&mut d, MI_UINT8, &stream);
        Some(array(MX_UINT8, &[1, stream.len()], b"", &d))
    }

    /// `FileWrapper__` metadata: version, string count, 8 offsets, strings,
    /// then classes, per-object properties, objects, and empty tables. Every
    /// object here has one property, `any`, its value being the object's
    /// position; more than one object is assumed from the layout of the
    /// measured files (one handle; a string next to `containers.Map`s).
    fn metadata(&self) -> Vec<u8> {
        let mut strings = b"any\0".to_vec();
        for c in &self.classes {
            strings.extend_from_slice(c.as_bytes());
            strings.push(0);
        }
        while !(strings.len() + 40).is_multiple_of(8) {
            strings.push(0);
        }
        let mut classes = vec![0u32; 4];
        for k in 0..self.classes.len() {
            classes.extend_from_slice(&[0, 2 + k as u32, 0, 0]);
        }
        let mut props = vec![0u32; 2];
        let mut objects = vec![0u32; 6];
        for (k, o) in self.objects.iter().enumerate() {
            props.extend_from_slice(&[1, 1, 1, k as u32]);
            objects.extend_from_slice(&[o.class_id as u32, 0, 0, k as u32 + 1, 0, 0]);
        }
        let at0 = 40 + strings.len() as u32;
        let at1 = at0 + 4 * classes.len() as u32;
        let at2 = at1 + 4 * props.len() as u32;
        let at3 = at2 + 4 * objects.len() as u32;
        let at4 = at3 + 8;
        let at7 = at4 + 8;
        let total = at7 + 16;
        let mut out = words32(&[
            WRAPPER_VERSION,
            1 + self.classes.len() as u32,
            at0,
            at1,
            at2,
            at3,
            at4,
            at4,
            at4,
            at7,
        ]);
        out.extend_from_slice(&strings);
        out.extend(words32(&classes));
        out.extend(words32(&props));
        out.extend(words32(&objects));
        out.extend(words32(&[0, 0, 0, 0]));
        out.extend(words32(&[1, total - 8, 0, 0]));
        out
    }
}

/// `function_handle` of a named handle: `simple`, or `scopedfunction` for a
/// local function of a file. No R2025b sample holds these: the fields are the
/// ones Octave 11 accepts from MATLAB's files, and are assumed for MATLAB.
fn named_handle(h: &Handle) -> Vec<u8> {
    if let Some(ctx) = handle_file(h)
        && ctx.file_backed
        && ctx.functions.contains_key(&h.name)
        && !is_main(&ctx, &h.name)
    {
        return struct_array(
            &[1, 1],
            &[
                ("function", text_array(&h.name)),
                ("type", text_array("scopedfunction")),
                ("file", text_array(&ctx.path.to_string_lossy())),
                ("parentage", cell_column(&[text_array(&h.name)])),
            ],
        );
    }
    struct_array(
        &[1, 1],
        &[
            ("function", text_array(&h.name)),
            ("type", text_array("simple")),
            ("file", text_array("")),
        ],
    )
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

/// The contents of a file's object element.
pub(super) struct Subsystem {
    data: Vec<u8>,
    be: bool,
    /// Byte ranges of the `FileWrapper__` cell's elements.
    slots: Vec<(usize, usize)>,
    /// Class names; `classes[class_id]`, the first entry is unused.
    classes: Vec<String>,
    /// `objects[id] = (class_id, property table index)`.
    objects: Vec<(usize, usize)>,
    /// Value position of property `any`, per property table index.
    any: Vec<Option<usize>>,
}

/// Move past the flags, sizes and names of an array, up to its data.
fn skip_header(r: &mut Rd, opaque: bool) -> Result<(), String> {
    r.elem()?;
    if opaque {
        r.elem()?;
        r.elem()?;
        r.elem()?;
    } else {
        r.elem()?;
        r.elem()?;
    }
    Ok(())
}

impl Subsystem {
    /// Parse the element at `start` of the file. `Err` says why not.
    pub(super) fn parse(file: &[u8], start: usize, be: bool) -> Result<Subsystem, String> {
        let mut top = Rd {
            buf: file,
            pos: start,
            be,
        };
        let e = top.elem()?;
        let inflated;
        let matrix: &[u8] = match e.ty {
            MI_COMPRESSED => {
                inflated = zlib::inflate(e.data, zlib::MAX_OUTPUT)?;
                let mut r = Rd {
                    buf: &inflated,
                    pos: 0,
                    be,
                };
                let inner = r.elem()?;
                if inner.ty != MI_MATRIX {
                    return Err("an array was expected".into());
                }
                inner.data
            }
            MI_MATRIX => e.data,
            _ => return Err("an array was expected".into()),
        };
        let mut r = Rd {
            buf: matrix,
            pos: 0,
            be,
        };
        skip_header(&mut r, false)?;
        let stream = r.elem()?.data.to_vec();
        Self::from_stream(stream, be)
    }

    /// `stream`: 8 bytes of head, then the struct holding `MCOS`.
    fn from_stream(data: Vec<u8>, be: bool) -> Result<Subsystem, String> {
        let mut r = Rd {
            buf: &data,
            pos: 8,
            be,
        };
        if data.len() < 8 {
            return Err("it is too short".into());
        }
        let main = r.elem()?;
        let mut s = Rd {
            buf: main.data,
            pos: 0,
            be,
        };
        skip_header(&mut s, false)?;
        let _ = s.elem()?;
        let names = s.elem()?;
        if text(&names) != "MCOS" {
            return Err("it holds no MCOS data".into());
        }
        let wrapper = s.elem()?;
        let mut w = Rd {
            buf: wrapper.data,
            pos: 0,
            be,
        };
        skip_header(&mut w, true)?;
        let cell = w.elem()?;
        let mut c = Rd {
            buf: cell.data,
            pos: 0,
            be,
        };
        skip_header(&mut c, false)?;
        let base = cell.data.as_ptr() as usize - data.as_ptr() as usize;
        let mut slots = Vec::new();
        while !c.done() {
            let at = c.pos;
            c.elem()?;
            slots.push((base + at, base + c.pos.min(cell.data.len())));
        }
        let &(a, b) = slots.first().ok_or("it holds no metadata")?;
        let mut m = Rd {
            buf: &data[a..b],
            pos: 0,
            be,
        };
        let meta_elem = m.elem()?;
        let mut mm = Rd {
            buf: meta_elem.data,
            pos: 0,
            be,
        };
        skip_header(&mut mm, false)?;
        let meta = mm.elem()?.data.to_vec();
        let (classes, objects, any) = Self::metadata(&meta, be)?;
        Ok(Subsystem {
            be,
            slots,
            classes,
            objects,
            any,
            data,
        })
    }

    #[allow(clippy::type_complexity)]
    fn metadata(
        m: &[u8],
        be: bool,
    ) -> Result<(Vec<String>, Vec<(usize, usize)>, Vec<Option<usize>>), String> {
        let word = |at: usize| -> Result<usize, String> {
            m.get(at..at + 4)
                .map(|b| u32_at(b, be) as usize)
                .ok_or_else(|| "its metadata is truncated".to_string())
        };
        let version = word(0)? as u32;
        if version != WRAPPER_VERSION {
            return Err(format!("its metadata version {version} is not supported"));
        }
        let count = word(4)?;
        let mut offs = [0usize; 8];
        for (k, o) in offs.iter_mut().enumerate() {
            *o = word(8 + 4 * k)?;
        }
        if offs[0] < 40 || offs.windows(2).any(|w| w[0] > w[1]) || offs[7] > m.len() {
            return Err("its metadata offsets are invalid".into());
        }
        let strings: Vec<String> = m[40..offs[0]]
            .split(|&b| b == 0)
            .take(count)
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .collect();
        let string = |i: usize| i.checked_sub(1).and_then(|k| strings.get(k));
        let mut classes = Vec::new();
        for at in (offs[0]..offs[1]).step_by(16) {
            let (pkg, name) = (word(at)?, word(at + 4)?);
            let n = string(name).cloned().unwrap_or_default();
            classes.push(match string(pkg) {
                Some(p) if !p.is_empty() => format!("{p}.{n}"),
                _ => n,
            });
        }
        // Property tables: `count (name kind value)*`, ending on 8 bytes.
        let mut any = vec![None];
        let mut at = offs[1] + 8;
        while at < offs[2] {
            let n = word(at)?;
            if n > (offs[2] - at).saturating_sub(4) / 12 {
                return Err("its property table is invalid".into());
            }
            let mut value = None;
            for k in 0..n {
                let e = at + 4 + 12 * k;
                if string(word(e)?).is_some_and(|s| s == "any") && word(e + 4)? == 1 {
                    value = Some(word(e + 8)?);
                }
            }
            any.push(value);
            at += 4 + 12 * n;
            at = at.div_ceil(8) * 8;
        }
        let mut objects = Vec::new();
        for at in (offs[2]..offs[3]).step_by(24) {
            objects.push((word(at)?, word(at + 12)?));
        }
        Ok((classes, objects, any))
    }

    /// The element holding property `any` of an object, if it has one.
    fn any_slot(&self, object: usize, class: &str) -> Result<&[u8], String> {
        let &(class_id, table) = self
            .objects
            .get(object)
            .ok_or_else(|| format!("object {object} is not in the file"))?;
        if self.classes.get(class_id).is_none_or(|c| c != class) {
            return Err(format!("object {object} is not of class '{class}'"));
        }
        let value = self
            .any
            .get(table)
            .copied()
            .flatten()
            .ok_or_else(|| format!("object {object} has no data"))?;
        let &(a, b) = self
            .slots
            .get(value + 2)
            .ok_or_else(|| format!("object {object} has no data"))?;
        Ok(&self.data[a..b])
    }
}

/// `[0xDD000000 ndims dims... object class]` of a scalar object.
fn scalar_object(e: &Elem, be: bool) -> Result<usize, String> {
    let w = numbers(e, be)?;
    if w.len() < 5 || w[0] as u32 != REF_MAGIC {
        return Err("the object reference is invalid".into());
    }
    let nd = w[1] as usize;
    if nd.checked_add(4) != Some(w.len()) {
        return Err("arrays of objects cannot be loaded in the Rust engine yet".into());
    }
    if w[2..2 + nd].iter().any(|&d| d != 1.0) {
        return Err("arrays of objects cannot be loaded in the Rust engine yet".into());
    }
    Ok(w[2 + nd] as usize)
}

impl Reader<'_> {
    /// An `mxOPAQUE` value of class `class`; `r` is at the reference.
    pub(super) fn object(
        &mut self,
        r: &mut Rd,
        class: &str,
        depth: usize,
    ) -> Result<Decoded, String> {
        let Some(sub) = self.subsystem else {
            return Ok(Decoded::Skipped(format!(
                "the file holds no data for objects of class '{class}'"
            )));
        };
        let refer = r.elem()?;
        if refer.ty != MI_MATRIX {
            return Err("an object reference was expected".into());
        }
        let mut rr = Rd {
            buf: refer.data,
            pos: 0,
            be: self.be,
        };
        skip_header(&mut rr, false)?;
        let object = match scalar_object(&rr.elem()?, self.be) {
            Ok(k) => k,
            Err(why) => return Ok(Decoded::Skipped(why)),
        };
        let slot = sub.any_slot(object, class)?;
        self.object_bytes = self.object_bytes.saturating_add(slot.len());
        if self.object_bytes > MAX_OBJECT_BYTES {
            return Err("the objects in this file repeat each other without limit".into());
        }
        let mut s = Rd {
            buf: slot,
            pos: 0,
            be: sub.be,
        };
        if class == STRING_CLASS {
            let e = s.elem()?;
            let mut body = Rd {
                buf: e.data,
                pos: 0,
                be: sub.be,
            };
            skip_header(&mut body, false)?;
            return Ok(Decoded::Value(decode_string(&body.elem()?, sub.be)?));
        }
        let (_, d) = self.child(&mut s, depth + 1)?;
        let Decoded::Value(Value::Cell(c)) = d else {
            return Err("a captured workspace was expected".into());
        };
        match c.data.get(1) {
            Some(ws @ Value::Struct(_)) => Ok(Decoded::Value(ws.clone())),
            _ => Err("a captured workspace was expected".into()),
        }
    }
}

fn decode_string(e: &Elem, be: bool) -> Result<Value, String> {
    if e.ty != MI_UINT64 || !e.data.len().is_multiple_of(8) {
        return Err("the data of a string array is invalid".into());
    }
    let word = |k: usize| -> Option<u64> {
        let b: [u8; 8] = e.data.get(k * 8..k * 8 + 8)?.try_into().ok()?;
        Some(if be {
            u64::from_be_bytes(b)
        } else {
            u64::from_le_bytes(b)
        })
    };
    let bad = || "the data of a string array is invalid".to_string();
    let total = e.data.len() / 8;
    if word(0) != Some(1) {
        return Err(bad());
    }
    let nd = word(1).ok_or_else(bad)? as usize;
    if !(2..=total).contains(&nd) || nd + 2 > total {
        return Err(bad());
    }
    let mut dims = Vec::with_capacity(nd);
    let mut numel = 1usize;
    for k in 0..nd {
        let d = usize::try_from(word(2 + k).ok_or_else(bad)?).map_err(|_| bad())?;
        numel = numel.checked_mul(d).ok_or_else(bad)?;
        dims.push(d);
    }
    let text_at = (2 + nd).checked_add(numel).ok_or_else(bad)?;
    if text_at > total {
        return Err(bad());
    }
    let (pairs, _) = e.data[text_at * 8..].as_chunks::<2>();
    let units: Vec<u16> = pairs
        .iter()
        .map(|&b| {
            if be {
                u16::from_be_bytes(b)
            } else {
                u16::from_le_bytes(b)
            }
        })
        .collect();
    let mut at = 0usize;
    let mut out = Vec::with_capacity(numel);
    for k in 0..numel {
        let n = word(2 + nd + k).ok_or_else(bad)?;
        if n == MISSING_COUNT {
            out.push(None);
            continue;
        }
        let n = usize::try_from(n).map_err(|_| bad())?;
        let piece = at
            .checked_add(n)
            .and_then(|end| units.get(at..end))
            .ok_or_else(bad)?;
        out.push(Some(String::from_utf16_lossy(piece)));
        at += n;
    }
    Ok(Value::Str(Array::new(indy_value::norm_dims(dims), out)))
}

/// Text of a field, `""` when absent or not text.
fn field_text(s: &StructArray, name: &str) -> String {
    s.get(0, name).and_then(|v| v.to_str()).unwrap_or_default()
}

/// The handle described by the struct inside an `mxFUNCTION` element.
pub(super) fn handle_from(outer: &Value) -> Result<Value, String> {
    let bad = || "the stored function handle is invalid".to_string();
    let Value::Struct(o) = outer else {
        return Err(bad());
    };
    if o.elems.len() != 1 {
        return Err(bad());
    }
    let Some(Value::Struct(f)) = o.get(0, "function_handle") else {
        return Err(bad());
    };
    if f.elems.len() != 1 {
        return Err(bad());
    }
    let name = field_text(f, "function");
    let handle = match field_text(f, "type").as_str() {
        "simple" => Handle {
            name,
            anon: None,
            ctx: None,
        },
        "scopedfunction" => scoped_handle(name, &field_text(f, "file"))?,
        "anonymous" => {
            let captured = match f.get(0, "workspace") {
                Some(Value::Struct(w)) if w.elems.len() == 1 => w
                    .fields
                    .iter()
                    .zip(&w.elems[0])
                    .map(|(n, v)| (n.clone(), v.clone()))
                    .collect(),
                None => vec![],
                Some(_) => {
                    return Err(format!(
                        "the variables captured by anonymous function {name} could not be read"
                    ));
                }
            };
            anon_handle(&name, captured)?
        }
        "nested" => {
            return Err(format!(
                "the handle to nested function '{name}' needs a running workspace that the Rust engine cannot restore"
            ));
        }
        other => return Err(format!("handles of type '{other}' are not supported")),
    };
    Ok(Value::Func(Rc::new(handle)))
}

/// The text after MATLAB's `sf%N` tag.
fn anon_text_of(stored: &str) -> &str {
    let Some(rest) = stored.strip_prefix("sf%") else {
        return stored;
    };
    let body = rest.trim_start_matches(|c: char| c.is_ascii_digit());
    if body.starts_with('@') { body } else { stored }
}

/// Parsed only; nothing in the text runs until the handle is called.
fn anon_handle(stored: &str, captured: Vec<(String, Value)>) -> Result<Handle, String> {
    let text = anon_text_of(stored);
    let invalid = || format!("the anonymous function text '{text}' could not be read");
    let ctx = crate::load_source(PathBuf::new(), "load".into(), text).map_err(|_| invalid())?;
    let [st] = ctx.stmts.as_slice() else {
        return Err(invalid());
    };
    let ast::StmtKind::Expr(e) = &st.kind else {
        return Err(invalid());
    };
    let ast::ExprKind::AnonFn { params, body } = &e.kind else {
        return Err(invalid());
    };
    let names: Vec<String> = params
        .iter()
        .map(|p| match p {
            ast::Param::Name(n) => n.name.clone(),
            ast::Param::WithDefault { name, .. } => name.name.clone(),
            ast::Param::Ignore(_) => "~".into(),
        })
        .collect();
    let anon = Anon {
        text: format!("@({}){}", names.join(","), crate::eval::unparse(body)),
        params: names,
        body: Rc::new((**body).clone()),
        captured,
    };
    Ok(Handle {
        name: String::new(),
        anon: Some(anon),
        ctx: Some(ctx.clone() as Rc<dyn std::any::Any>),
    })
}

/// A local function of `file`: parsed from the file if it is still there.
fn scoped_handle(name: String, file: &str) -> Result<Handle, String> {
    let gone =
        || format!("the handle to local function '{name}' needs '{file}', which could not be read");
    let path = PathBuf::from(file);
    let is_m = path.extension().is_some_and(|e| e == "m");
    if !is_m || !path.is_file() {
        return Err(gone());
    }
    let ctx = crate::read_source_file(&path).map_err(|_| gone())?;
    if !ctx.functions.contains_key(&name) {
        return Err(gone());
    }
    Ok(Handle {
        name,
        anon: None,
        ctx: Some(ctx as Rc<dyn std::any::Any>),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_huge_dimension_count_in_a_reference_is_refused() {
        let words: Vec<f64> = vec![REF_MAGIC as f64, 1e20, 1.0, 1.0, 0.0, 0.0];
        let data: Vec<u8> = words.iter().flat_map(|x| x.to_le_bytes()).collect();
        let e = Elem {
            ty: MI_DOUBLE,
            data: &data,
        };
        assert!(scalar_object(&e, false).is_err());
    }

    #[test]
    fn a_property_table_cannot_reach_past_its_section() {
        // Version, one string, 8 offsets; the string "any"; no classes; the
        // property section holds 8 bytes, then one table of count 1 in the 12
        // bytes left, which needs 16.
        let at0 = 44u32;
        let at1 = at0;
        let at2 = at1 + 8 + 12;
        let mut words = vec![
            WRAPPER_VERSION,
            1,
            at0,
            at1,
            at2,
            at2,
            at2,
            at2,
            at2,
            at2 + 8,
        ];
        words.push(u32::from_le_bytes(*b"any\0"));
        words.extend([0, 0, 1, 1, 1]);
        words.extend([0, 0]);
        let meta = words32(&words);
        let why = Subsystem::metadata(&meta, false).err();
        assert_eq!(why.as_deref(), Some("its property table is invalid"));
    }
}
