//! MAT files and workspace listings: `save`, `load`, `who`, `whos`.
//!
//! Semantics follow MATLAB's documentation:
//! - `save` writes MAT version 7 (level 5 with compressed elements) by
//!   default; `-v6` uncompressed, `-v4` level 4, `-mat` = default. A file
//!   name without an extension gets `.mat` (not with `-ascii`). Variables:
//!   names, `*` wildcards, `-regexp` patterns, `-struct s [fields]`;
//!   `-append` replaces or adds variables. `-ascii` writes numbers as text
//!   (8 digits, `-double` 16, `-tabs` tab-separated). Octave's spellings
//!   `-mat7-binary`, `-mat-binary`, `-v7`/`-7`, `-v6`/`-6` are accepted.
//! - `load` reads level 5 (both byte orders, compressed or not) and level
//!   4 files; a file with an extension other than `.mat` is numeric text
//!   unless `-mat` is given (a MAT header is still recognised). With an
//!   output it returns a struct (a matrix for text files), otherwise it
//!   assigns into the calling workspace.
//! - Function handles and string arrays are stored as MATLAB stores them.
//!   Sparse matrices stay sparse in both directions; `-ascii` writes the full
//!   form.
//! - Values the format cannot hold here (tables, datetimes, classdef objects,
//!   nested function handles) never pass silently. As a variable of their own
//!   they are skipped with a warning: nothing stands in for them. Inside a cell
//!   or struct the element is saved as `[]` and the warning names its path
//!   (`s.f{2}`), so the rest of the variable is kept.
//! - `matfile` is not provided (explicit error).

pub mod format;
pub mod zlib;

use std::path::{Path, PathBuf};

use indy_value::{Array, Num, NumClass, StructArray, Value};

use super::core::{is_varname, resolve_path};
use super::regex::{ReOpts, Regex};
use super::{err, one, reg};
use crate::{Interp, R};

pub use format::{MatData, MatVar};

pub fn register(it: &mut Interp) {
    reg(it, "save", save);
    reg(it, "load", load);
    reg(it, "who", who);
    reg(it, "whos", whos);
    reg(it, "matfile", |_, _, _| {
        Err(err(
            "IndyR:matfileUnsupported",
            "matfile objects are not supported in the Rust engine yet; use load and save instead.",
        ))
    });
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Text arguments of save/load/who/whos (command and function forms).
fn texts(args: &[Value], fname: &str) -> R<Vec<String>> {
    args.iter()
        .map(|a| {
            a.to_str().ok_or_else(|| {
                err(
                    &format!("MATLAB:{fname}:invalidInput"),
                    format!("{fname}: arguments must be character vectors or strings."),
                )
            })
        })
        .collect()
}

/// `*` wildcard match (MATLAB variable selection).
fn glob(pat: &str, s: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    let t: Vec<char> = s.chars().collect();
    let (mut i, mut j) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while j < t.len() {
        if i < p.len() && p[i] == '*' {
            star = Some(i);
            i += 1;
            mark = j;
        } else if i < p.len() && p[i] == t[j] {
            i += 1;
            j += 1;
        } else if let Some(s) = star {
            i = s + 1;
            mark += 1;
            j = mark;
        } else {
            return false;
        }
    }
    p[i..].iter().all(|&c| c == '*')
}

/// How variable names are selected.
struct Selector {
    patterns: Vec<String>,
    regex: Vec<Regex>,
}

impl Selector {
    fn new(patterns: Vec<String>, regexp: Vec<String>) -> R<Selector> {
        let regex = regexp
            .iter()
            .map(|p| Regex::new(p, ReOpts::default()))
            .collect::<R<Vec<_>>>()?;
        Ok(Selector { patterns, regex })
    }
    fn all(&self) -> bool {
        self.patterns.is_empty() && self.regex.is_empty()
    }
    fn matches(&self, name: &str) -> bool {
        if self.all() {
            return true;
        }
        if self.patterns.iter().any(|p| glob(p, name)) {
            return true;
        }
        let chars: Vec<char> = name.chars().collect();
        self.regex.iter().any(|r| {
            r.find_all(&chars, false, Some(1))
                .is_ok_and(|f| !f.is_empty())
        })
    }
    /// Names in `available` order for wildcards and regexps; plain names in
    /// the given order. Unmatched plain names are returned separately.
    fn select(&self, available: &[String]) -> (Vec<String>, Vec<String>) {
        if self.all() {
            return (available.to_vec(), vec![]);
        }
        let mut out: Vec<String> = Vec::new();
        let mut missing = Vec::new();
        for p in &self.patterns {
            if p.contains('*') {
                let hits: Vec<&String> = available.iter().filter(|n| glob(p, n)).collect();
                if hits.is_empty() {
                    missing.push(p.clone());
                }
                for h in hits {
                    if !out.contains(h) {
                        out.push(h.clone());
                    }
                }
            } else if available.contains(p) {
                if !out.contains(p) {
                    out.push(p.clone());
                }
            } else {
                missing.push(p.clone());
            }
        }
        if !self.regex.is_empty() {
            for n in available {
                let chars: Vec<char> = n.chars().collect();
                let hit = self.regex.iter().any(|r| {
                    r.find_all(&chars, false, Some(1))
                        .is_ok_and(|f| !f.is_empty())
                });
                if hit && !out.contains(n) {
                    out.push(n.clone());
                }
            }
        }
        (out, missing)
    }
}

/// Variable names of the current workspace, sorted (MATLAB `who` order).
pub fn workspace_names(it: &Interp) -> Vec<String> {
    let f = it.frame();
    let mut names: Vec<String> = f
        .vars
        .keys()
        .chain(f.globals.iter())
        .chain(f.persistents.iter())
        .filter(|n| it.get_var(n).is_some())
        .cloned()
        .collect();
    names.sort();
    names.dedup();
    names
}

fn is_global(it: &Interp, name: &str) -> bool {
    it.frame().globals.contains(name)
}

/// MATLAB's storage size in bytes (`whos`): element data only, no array
/// header overhead (char is 2 bytes, as in MATLAB).
pub fn bytes_of(v: &Value) -> u64 {
    let n = v.numel() as u64;
    match v {
        Value::Num(x) => {
            let w = match x.class {
                NumClass::Double | NumClass::Int64 | NumClass::UInt64 => 8,
                NumClass::Single | NumClass::Int32 | NumClass::UInt32 => 4,
                NumClass::Int16 | NumClass::UInt16 => 2,
                NumClass::Int8 | NumClass::UInt8 => 1,
            };
            n * w * if x.im.is_some() { 2 } else { 1 }
        }
        Value::Bool(_) => n,
        Value::Char(_) => 2 * n,
        Value::Cell(c) => c.data.iter().map(bytes_of).sum(),
        Value::Struct(s) => s.elems.iter().flatten().map(bytes_of).sum(),
        Value::Str(s) => s
            .data
            .iter()
            .map(|e| e.as_ref().map_or(0, |t| 2 * t.chars().count() as u64))
            .sum(),
        Value::Sparse(s) => s.storage_bytes(),
        Value::Func(_) | Value::Object(_) | Value::HObject(_) | Value::ObjArr(_) => 0,
    }
}

fn read_bytes(path: &Path) -> R<Vec<u8>> {
    std::fs::read(path).map_err(|e| {
        err(
            "MATLAB:load:couldNotReadFile",
            format!(
                "Unable to read file '{}': {}.",
                path.display(),
                io_reason(&e)
            ),
        )
    })
}

fn io_reason(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::NotFound => "No such file or directory".into(),
        std::io::ErrorKind::PermissionDenied => "Permission denied".into(),
        _ => e.to_string(),
    }
}

fn has_ext(p: &Path) -> bool {
    p.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.rfind('.').is_some_and(|i| i > 0 && i + 1 < n.len()))
}

fn is_mat_ext(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("mat"))
}

/// Parse a MAT file's bytes (any supported level).
pub fn parse(bytes: &[u8], shown: &str) -> R<MatData> {
    format::read(bytes).map_err(|m| {
        err(
            "MATLAB:load:unableToReadMatFile",
            format!("Unable to read MAT-file {shown}: {m}."),
        )
    })
}

// ---------------------------------------------------------------------------
// save
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fmt {
    V7,
    V6,
    V4,
    Ascii,
}

struct SaveArgs {
    file: Option<String>,
    fmt: Fmt,
    explicit_fmt: bool,
    append: bool,
    double: bool,
    tabs: bool,
    names: Vec<String>,
    regexp: Vec<String>,
    structs: Option<(String, Vec<String>)>,
}

fn save_args(words: Vec<String>) -> R<SaveArgs> {
    let mut a = SaveArgs {
        file: None,
        fmt: Fmt::V7,
        explicit_fmt: false,
        append: false,
        double: false,
        tabs: false,
        names: vec![],
        regexp: vec![],
        structs: None,
    };
    let mut regexp_mode = false;
    let mut in_struct = false;
    fn set(a: &mut SaveArgs, f: Fmt) {
        a.fmt = f;
        a.explicit_fmt = true;
    }
    let mut i = 0;
    while i < words.len() {
        let w = &words[i];
        if let Some(opt) = w.strip_prefix('-')
            && !opt.is_empty()
        {
            match opt.to_ascii_lowercase().as_str() {
                "append" => a.append = true,
                "ascii" => set(&mut a, Fmt::Ascii),
                "double" => a.double = true,
                "tabs" => a.tabs = true,
                "mat" | "v7" | "7" | "mat7-binary" | "zip" | "z" => set(&mut a, Fmt::V7),
                "v6" | "6" | "mat-binary" => set(&mut a, Fmt::V6),
                "v4" | "4" | "mat4-binary" => set(&mut a, Fmt::V4),
                "nocompression" => set(&mut a, Fmt::V6),
                "v7.3" | "hdf5" => {
                    return Err(err(
                        "IndyR:save:hdf5Unsupported",
                        "save: MAT-file version 7.3 (HDF5) is not supported in the Rust engine yet; use -v7.",
                    ));
                }
                "text" | "binary" | "float-binary" | "float-hdf5" => {
                    return Err(err(
                        "IndyR:save:formatUnsupported",
                        format!(
                            "save: Octave's {w} format is not supported in the Rust engine; use -v7, -v6 or -ascii."
                        ),
                    ));
                }
                "regexp" => {
                    regexp_mode = true;
                    in_struct = false;
                }
                "struct" => {
                    let Some(s) = words.get(i + 1) else {
                        return Err(err(
                            "MATLAB:save:missingStructName",
                            "save: -struct requires the name of a scalar structure variable.",
                        ));
                    };
                    a.structs = Some((s.clone(), vec![]));
                    in_struct = true;
                    i += 1;
                }
                _ => {
                    return Err(err(
                        "MATLAB:save:unknownOption",
                        format!("save: unknown option '{w}'."),
                    ));
                }
            }
        } else if a.file.is_none() {
            a.file = Some(w.clone());
        } else if in_struct {
            if let Some((_, f)) = &mut a.structs {
                f.push(w.clone());
            }
        } else if regexp_mode {
            a.regexp.push(w.clone());
        } else {
            a.names.push(w.clone());
        }
        i += 1;
    }
    if (a.double || a.tabs) && a.fmt != Fmt::Ascii {
        return Err(err(
            "MATLAB:save:invalidOption",
            "save: -double and -tabs are valid only with -ascii.",
        ));
    }
    Ok(a)
}

/// Variables chosen for `save`: (name, value, global).
type Chosen = Vec<(String, Value, bool)>;

fn chosen(it: &mut Interp, a: &SaveArgs) -> R<Chosen> {
    if let Some((sname, fields)) = &a.structs {
        let s = match it.get_var(sname) {
            Some(Value::Struct(s)) if s.numel() == 1 => s.clone(),
            _ => {
                return Err(err(
                    "MATLAB:save:notScalarStruct",
                    format!(
                        "The argument to -STRUCT must be the name of a scalar structure variable ('{sname}')."
                    ),
                ));
            }
        };
        let sel = Selector::new(fields.clone(), a.regexp.clone())?;
        let (names, missing) = sel.select(&s.fields);
        if let Some(m) = missing.iter().find(|m| !m.contains('*')) {
            return Err(err(
                "MATLAB:save:fieldNotFound",
                format!("Field '{m}' not found."),
            ));
        }
        for m in missing {
            it.warning(
                "MATLAB:save:fieldNotFound",
                &format!("Field '{m}' not found."),
            );
        }
        let mut out = Vec::new();
        for n in names {
            let v = s.get(0, &n).cloned().unwrap_or_default();
            out.push((n, v, false));
        }
        return Ok(out);
    }
    let available = workspace_names(it);
    let sel = Selector::new(a.names.clone(), a.regexp.clone())?;
    let (names, missing) = sel.select(&available);
    if let Some(m) = missing.iter().find(|m| !m.contains('*')) {
        return Err(err(
            "MATLAB:save:variableNotFound",
            format!("Variable '{m}' not found."),
        ));
    }
    for m in missing {
        it.warning(
            "MATLAB:save:variableNotFound",
            &format!("Variable '{m}' not found."),
        );
    }
    let mut out = Vec::new();
    for n in names {
        if let Some(v) = it.get_var(&n) {
            let v = v.clone();
            out.push((n.clone(), v, is_global(it, &n)));
        }
    }
    Ok(out)
}

fn save(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let words = texts(&a, "save")?;
    let args = save_args(words)?;
    let mut file = args.file.clone().unwrap_or_else(|| "matlab.mat".into());
    if args.fmt != Fmt::Ascii && !has_ext(Path::new(&file)) {
        file.push_str(".mat");
    }
    let path = resolve_path(it, &file);
    let vars = chosen(it, &args)?;
    let mut warnings = Vec::new();
    let bytes = encode_file(&path, &vars, &args, &mut warnings)?;
    for w in warnings {
        it.warning("MATLAB:save:skipped", &w);
    }
    write_file(&path, &bytes, args.append && args.fmt == Fmt::Ascii)?;
    Ok(vec![])
}

fn write_file(path: &Path, bytes: &[u8], append: bool) -> R<()> {
    use std::io::Write;
    let res = if append {
        std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .and_then(|mut f| f.write_all(bytes))
    } else {
        std::fs::write(path, bytes)
    };
    res.map_err(|e| {
        err(
            "MATLAB:save:couldNotWriteFile",
            format!(
                "Unable to write file {}: {}.",
                path.display(),
                io_reason(&e)
            ),
        )
    })
}

fn encode_file(
    path: &Path,
    vars: &Chosen,
    args: &SaveArgs,
    warnings: &mut Vec<String>,
) -> R<Vec<u8>> {
    match args.fmt {
        Fmt::Ascii => Ok(ascii_text(vars, args.double, args.tabs, warnings).into_bytes()),
        Fmt::V4 => {
            let mut out = if args.append && path.exists() {
                let old = read_bytes(path)?;
                let d = parse(&old, &path.display().to_string())?;
                if d.level != 4 {
                    return Err(err(
                        "MATLAB:save:appendVersion",
                        "save: -append -v4 needs a level 4 MAT-file.",
                    ));
                }
                let mut keep = Vec::new();
                for v in d.vars {
                    if vars.iter().all(|(n, _, _)| *n != v.name)
                        && let Some(x) = &v.value
                    {
                        keep.extend(format::encode4(&v.name, x).unwrap_or_default());
                    }
                }
                keep
            } else {
                vec![]
            };
            for (n, v, _) in vars {
                match format::encode4(n, v) {
                    Ok(b) => out.extend(b),
                    Err(why) => {
                        warnings.push(format!("Variable '{n}' was not saved: {why}."));
                    }
                }
            }
            Ok(out)
        }
        Fmt::V7 | Fmt::V6 => {
            let compress = args.fmt == Fmt::V7;
            let warned_before = warnings.len();
            let mut w = format::Writer::new(warnings);
            let mut encoded = Vec::new();
            for (n, v, g) in vars {
                match w.encode(n, v, *g).and_then(|e| format::wrap(e, compress)) {
                    Ok(e) => encoded.push((n.clone(), e)),
                    Err(why) => w
                        .warnings
                        .push(format!("Variable '{n}' was not saved: {why}.")),
                }
            }
            if args.append && path.exists() {
                let old = read_bytes(path)?;
                let d = parse(&old, &path.display().to_string())?;
                if d.level != 5 {
                    return Err(err(
                        "MATLAB:save:appendVersion",
                        "save: -append to a level 4 MAT-file needs -v4.",
                    ));
                }
                let replaced: Vec<&String> = d
                    .vars
                    .iter()
                    .filter(|v| encoded.iter().any(|(n, _)| *n == v.name))
                    .map(|v| &v.name)
                    .collect();
                let new_objects = w.has_objects();
                let refused = || {
                    err(
                        "IndyR:save:appendObjects",
                        "save: this MAT-file contains MATLAB objects that the Rust engine cannot read, so it cannot change them; save to a new file.",
                    )
                };
                if new_objects && d.subsystem_range.is_some() {
                    // Object numbers would collide: store the whole file anew.
                    if d.vars.iter().any(|v| v.value.is_none()) {
                        return Err(refused());
                    }
                    w.warnings.truncate(warned_before);
                    let mut w = format::Writer::new(w.warnings);
                    let mut out = old[..128].to_vec();
                    let mut put = |w: &mut format::Writer, n: &str, v: &Value, g: bool| match w
                        .encode(n, v, g)
                        .and_then(|e| format::wrap(e, compress))
                    {
                        Ok(e) => out.extend(e),
                        Err(why) => w
                            .warnings
                            .push(format!("Variable '{n}' was not saved: {why}.")),
                    };
                    for v in &d.vars {
                        match vars.iter().find(|(n, _, _)| *n == v.name) {
                            Some((n, x, g)) if encoded.iter().any(|(e, _)| e == n) => {
                                put(&mut w, n, x, *g);
                            }
                            _ => {
                                if let Some(x) = &v.value {
                                    put(&mut w, &v.name, x, v.global);
                                }
                            }
                        }
                    }
                    for (n, x, g) in vars {
                        if !replaced.contains(&n) && encoded.iter().any(|(e, _)| e == n) {
                            put(&mut w, n, x, *g);
                        }
                    }
                    return finish_level5(out, &w, compress);
                }
                let old_objects = d.subsystem_range.map(|(a, b)| old[a..b].to_vec());
                if replaced.is_empty() && old_objects.is_none() && !new_objects {
                    // Pure addition: keep the file byte for byte, add at the end.
                    let mut out = old;
                    for (_, e) in encoded {
                        out.extend(e);
                    }
                    return Ok(out);
                }
                // New objects would need a new object element, and the old objects
                // stored in this file point into the one that could not be read.
                if d.subsystem && old_objects.is_none() && (!replaced.is_empty() || new_objects) {
                    return Err(refused());
                }
                let mut out = old[..128].to_vec();
                for v in &d.vars {
                    match encoded.iter().position(|(n, _)| *n == v.name) {
                        Some(k) => out.extend(std::mem::take(&mut encoded[k].1)),
                        None => out.extend_from_slice(&v.raw),
                    }
                }
                for (n, e) in encoded {
                    if !replaced.contains(&&n) {
                        out.extend(e);
                    }
                }
                if let Some(objects) = old_objects {
                    let at = out.len();
                    format::set_subsystem_offset(&mut out, at);
                    out.extend(objects);
                    return Ok(out);
                }
                return finish_level5(out, &w, compress);
            }
            let mut out = format::header5();
            for (_, e) in encoded {
                out.extend(e);
            }
            finish_level5(out, &w, compress)
        }
    }
}

/// Add the object data element, last in the file, and point the header at it.
fn finish_level5(mut out: Vec<u8>, w: &format::Writer, compress: bool) -> R<Vec<u8>> {
    if let Some(objects) = w.subsystem() {
        let wrapped = format::wrap(objects, compress).map_err(|why| {
            err(
                "MATLAB:save:couldNotWriteFile",
                format!("save: the file's object data could not be stored: {why}."),
            )
        })?;
        let at = out.len();
        format::set_subsystem_offset(&mut out, at);
        out.extend(wrapped);
    }
    Ok(out)
}

/// `save -ascii` text: each variable's rows (2-D numeric, logical or char).
fn ascii_text(vars: &Chosen, double: bool, tabs: bool, warnings: &mut Vec<String>) -> String {
    let mut out = String::new();
    for (n, v, _) in vars {
        let full;
        let v = match v {
            Value::Sparse(s) => match s.dense() {
                Some(d) => {
                    full = d;
                    &full
                }
                None => {
                    warnings.push(format!(
                        "Variable '{n}' not written to file: its {}x{} sparse matrix is too large to write as text.",
                        s.dims[0], s.dims[1]
                    ));
                    continue;
                }
            },
            other => other,
        };
        let (dims, re, complex) = match v {
            Value::Num(x) => (x.dims.clone(), x.re.clone(), x.im.is_some()),
            Value::Bool(b) => (
                b.dims.clone(),
                b.data.iter().map(|&t| t as u8 as f64).collect(),
                false,
            ),
            Value::Char(c) => (
                c.dims.clone(),
                c.data.iter().map(|&ch| ch as u32 as f64).collect(),
                false,
            ),
            _ => {
                warnings.push(format!(
                    "Attempt to write an unsupported data type to an ASCII file. Variable '{n}' not written to file."
                ));
                continue;
            }
        };
        if complex {
            warnings.push(format!(
                "Attempt to write complex data to an ASCII file. Only the real part of '{n}' is written."
            ));
        }
        if dims.len() > 2 {
            warnings.push(format!(
                "Attempt to write an N-D array to an ASCII file. Variable '{n}' not written to file."
            ));
            continue;
        }
        let (r, c) = (dims[0], dims[1]);
        for i in 0..r {
            for j in 0..c {
                let x = re[i + j * r];
                let t = if x.is_nan() {
                    "NaN".to_string()
                } else if x.is_infinite() {
                    if x > 0.0 { "Inf" } else { "-Inf" }.to_string()
                } else {
                    indy_value::format::fmt_e(x, if double { 15 } else { 7 }, false)
                };
                if tabs {
                    if j > 0 {
                        out.push('\t');
                    }
                    out.push_str(&t);
                } else {
                    let w = if double { 24 } else { 16 };
                    out.push_str(&format!("{t:>w$}"));
                }
            }
            out.push('\n');
        }
    }
    out
}

// ---------------------------------------------------------------------------
// load
// ---------------------------------------------------------------------------

/// The file `load` reads: as given, else with `.mat`, else on the path.
fn find_load_file(it: &Interp, name: &str) -> Option<PathBuf> {
    let mut tries = vec![name.to_string()];
    if !has_ext(Path::new(name)) {
        tries.insert(0, format!("{name}.mat"));
    }
    for t in &tries {
        let p = resolve_path(it, t);
        if p.is_file() {
            return Some(p);
        }
    }
    if !Path::new(name).is_absolute() {
        for dir in &it.path {
            for t in &tries {
                let p = dir.join(t);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    None
}

/// Variable name MATLAB gives a text file's data: the file name stem with
/// non-word characters replaced, prefixed with `X` when it starts badly.
fn ascii_var_name(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("data")
        .to_string();
    let mut s: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if !s.starts_with(|c: char| c.is_ascii_alphabetic()) {
        s.insert(0, 'X');
    }
    s.truncate(63);
    s
}

/// Numeric text: rows on lines, numbers separated by spaces, tabs or commas;
/// `%` starts a comment.
pub fn parse_ascii(text: &str, shown: &str) -> R<Value> {
    let mut rows: Vec<Vec<f64>> = Vec::new();
    for (k, line) in text.lines().enumerate() {
        let line = line.split('%').next().unwrap_or("");
        let toks: Vec<&str> = line
            .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
            .filter(|t| !t.is_empty())
            .collect();
        if toks.is_empty() {
            continue;
        }
        let mut row = Vec::with_capacity(toks.len());
        for t in toks {
            let x = match t.to_ascii_lowercase().as_str() {
                "nan" | "+nan" | "-nan" => f64::NAN,
                "inf" | "+inf" => f64::INFINITY,
                "-inf" => f64::NEG_INFINITY,
                s => s.replace(['d', 'D'], "e").parse::<f64>().map_err(|_| {
                    err(
                        "MATLAB:load:unableToReadAsciiFile",
                        format!(
                            "Unable to read file '{shown}'. Input must be a MAT-file or an ASCII file containing numeric data with same number of columns in each row."
                        ),
                    )
                })?,
            };
            row.push(x);
        }
        if let Some(first) = rows.first()
            && first.len() != row.len()
        {
            return Err(err(
                "MATLAB:load:numColumnsNotSame",
                format!(
                    "Number of columns on line {} of ASCII file {shown} must be the same as previous lines.",
                    k + 1
                ),
            ));
        }
        rows.push(row);
    }
    let r = rows.len();
    let c = rows.first().map_or(0, |x| x.len());
    let mut data = vec![0.0; r * c];
    for (i, row) in rows.iter().enumerate() {
        for (j, &x) in row.iter().enumerate() {
            data[i + j * r] = x;
        }
    }
    Ok(Value::Num(Num::real(vec![r, c], data)))
}

fn load(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let words = texts(&a, "load")?;
    let (mut file, mut force_mat, mut force_ascii, mut regexp_mode) = (None, false, false, false);
    let (mut names, mut regexp) = (Vec::new(), Vec::new());
    for w in words {
        match w.to_ascii_lowercase().as_str() {
            "-mat" => force_mat = true,
            "-ascii" => force_ascii = true,
            "-regexp" => regexp_mode = true,
            o if o.starts_with('-') && o.len() > 1 => {
                return Err(err(
                    "MATLAB:load:unknownOption",
                    format!("load: unknown option '{w}'."),
                ));
            }
            _ if file.is_none() => file = Some(w),
            _ if regexp_mode => regexp.push(w),
            _ => names.push(w),
        }
    }
    if force_mat && force_ascii {
        return Err(err(
            "MATLAB:load:conflictingOptions",
            "load: -mat and -ascii cannot be used together.",
        ));
    }
    let name = file.unwrap_or_else(|| "matlab.mat".into());
    let Some(path) = find_load_file(it, &name) else {
        return Err(err(
            "MATLAB:load:couldNotReadFile",
            format!("Unable to find file or directory '{name}'."),
        ));
    };
    let shown = path.display().to_string();
    let bytes = read_bytes(&path)?;
    let kind = format::sniff(&bytes);
    let mat = !force_ascii
        && (force_mat
            || is_mat_ext(&path)
            || kind == format::Kind::Level5
            || kind == format::Kind::Hdf5
            || (!has_ext(&path) && kind != format::Kind::Unknown));
    if !mat {
        if bytes.starts_with(b"# Created by Octave") {
            return Err(err(
                "IndyR:load:octaveText",
                format!(
                    "{shown} is in Octave's text format, which the Rust engine cannot read; save it from Octave with save -v7."
                ),
            ));
        }
        let text = String::from_utf8_lossy(&bytes);
        let v = parse_ascii(&text, &shown)?;
        if nargout > 0 {
            return one(v);
        }
        it.set_var(&ascii_var_name(&path), v);
        return Ok(vec![]);
    }
    let data = parse(&bytes, &shown)?;
    for w in &data.warnings {
        it.warning("MATLAB:load:skipped", w);
    }
    for v in &data.vars {
        if !is_varname(&v.name) {
            it.warning(
                "MATLAB:load:invalidName",
                &format!(
                    "Variable '{}' was not loaded: it is not a valid variable name.",
                    v.name
                ),
            );
        }
    }
    let sel = Selector::new(names, regexp)?;
    let available: Vec<String> = data
        .vars
        .iter()
        .filter(|v| is_varname(&v.name))
        .map(|v| v.name.clone())
        .collect();
    let (wanted, missing) = sel.select(&available);
    for m in missing {
        it.warning(
            "MATLAB:load:variableNotFound",
            &format!("Variable '{m}' not found."),
        );
    }
    let picked: Vec<MatVar> = data
        .vars
        .into_iter()
        .filter(|v| v.value.is_some() && wanted.contains(&v.name))
        .collect();
    if nargout > 0 {
        let mut s = StructArray::new_scalar();
        for v in picked {
            if let Some(x) = v.value {
                s.set(0, &v.name, x);
            }
        }
        return one(Value::Struct(s));
    }
    for v in picked {
        if let Some(x) = v.value {
            it.set_var(&v.name, x);
        }
    }
    Ok(vec![])
}

// ---------------------------------------------------------------------------
// who / whos
// ---------------------------------------------------------------------------

/// One row of `whos`.
pub struct Info {
    pub name: String,
    pub dims: Vec<usize>,
    pub bytes: u64,
    pub class: String,
    pub global: bool,
    pub sparse: bool,
    pub complex: bool,
}

pub fn info_of(name: &str, v: &Value, global: bool) -> Info {
    Info {
        name: name.to_string(),
        dims: v.dims(),
        bytes: bytes_of(v),
        class: v.class_name(),
        global,
        sparse: v.is_sparse(),
        complex: match v {
            Value::Num(n) => n.im.is_some(),
            Value::Sparse(s) => s.im.is_some(),
            _ => false,
        },
    }
}

/// Rows of a MAT file for `whos -file` (skipped variables included).
pub fn file_infos(data: &MatData) -> Vec<Info> {
    data.vars
        .iter()
        .map(|v| match &v.value {
            Some(x) => info_of(&v.name, x, v.global),
            None => Info {
                name: v.name.clone(),
                dims: v.dims.clone(),
                bytes: 0,
                class: v.class.clone(),
                global: v.global,
                sparse: false,
                complex: false,
            },
        })
        .collect()
}

fn listing(it: &mut Interp, a: &[Value], fname: &str) -> R<Vec<Info>> {
    let words = texts(a, fname)?;
    let (mut file, mut regexp_mode) = (None, false);
    let (mut names, mut regexp) = (Vec::new(), Vec::new());
    let mut i = 0;
    while i < words.len() {
        let w = &words[i];
        match w.to_ascii_lowercase().as_str() {
            "-file" => {
                let Some(f) = words.get(i + 1) else {
                    return Err(err(
                        &format!("MATLAB:{fname}:missingFile"),
                        format!("{fname}: -file requires a file name."),
                    ));
                };
                file = Some(f.clone());
                i += 1;
            }
            "-regexp" => regexp_mode = true,
            "global" | "-global" => {}
            _ if regexp_mode => regexp.push(w.clone()),
            _ => names.push(w.clone()),
        }
        i += 1;
    }
    let global_only = words
        .iter()
        .any(|w| w.eq_ignore_ascii_case("global") || w.eq_ignore_ascii_case("-global"));
    let sel = Selector::new(names, regexp)?;
    let rows = match file {
        Some(f) => {
            let Some(path) = find_load_file(it, &f) else {
                return Err(err(
                    "MATLAB:whos:fileNotFound",
                    format!("Unable to find file or directory '{f}'."),
                ));
            };
            let bytes = read_bytes(&path)?;
            let data = parse(&bytes, &path.display().to_string())?;
            file_infos(&data)
        }
        None => {
            let mut rows = Vec::new();
            for n in workspace_names(it) {
                if let Some(v) = it.get_var(&n) {
                    rows.push(info_of(&n, v, is_global(it, &n)));
                }
            }
            rows
        }
    };
    Ok(rows
        .into_iter()
        .filter(|r| sel.matches(&r.name) && (!global_only || r.global))
        .collect())
}

fn size_text(d: &[usize]) -> String {
    d.iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join("x")
}

fn who(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let rows = listing(it, &a, "who")?;
    if nargout > 0 {
        let names = rows.into_iter().map(|r| Value::str(&r.name)).collect();
        return one(Value::Cell(Array::col(names)));
    }
    if rows.is_empty() {
        return Ok(vec![]);
    }
    let width = rows
        .iter()
        .map(|r| r.name.chars().count())
        .max()
        .unwrap_or(0)
        + 2;
    let per = (80 / width).max(1);
    let mut t = String::from("\nYour variables are:\n\n");
    for chunk in rows.chunks(per) {
        for r in chunk {
            t.push_str(&format!("{:<width$}", r.name));
        }
        t = t.trim_end().to_string();
        t.push('\n');
    }
    t.push('\n');
    it.print(&t);
    Ok(vec![])
}

fn whos(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let rows = listing(it, &a, "whos")?;
    if nargout > 0 {
        let fields: Vec<String> = [
            "name",
            "size",
            "bytes",
            "class",
            "global",
            "sparse",
            "complex",
            "nesting",
            "persistent",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let mut s = StructArray::empty_with(fields, vec![rows.len(), 1]);
        for (k, r) in rows.iter().enumerate() {
            let mut nest = StructArray::new_scalar();
            nest.set(0, "function", Value::str(""));
            nest.set(0, "level", Value::num(1.0));
            let dims: Vec<f64> = r.dims.iter().map(|&n| n as f64).collect();
            s.elems[k] = vec![
                Value::str(&r.name),
                Value::row(dims),
                Value::num(r.bytes as f64),
                Value::str(&r.class),
                Value::bool(r.global),
                Value::bool(r.sparse),
                Value::bool(r.complex),
                Value::Struct(nest),
                Value::bool(false),
            ];
        }
        return one(Value::Struct(s));
    }
    if rows.is_empty() {
        return Ok(vec![]);
    }
    let nw = rows
        .iter()
        .map(|r| r.name.chars().count())
        .max()
        .unwrap_or(0)
        .max(4);
    let sizes: Vec<String> = rows.iter().map(|r| size_text(&r.dims)).collect();
    let sw = sizes.iter().map(|s| s.len()).max().unwrap_or(0).max(4);
    let bw = rows
        .iter()
        .map(|r| r.bytes.to_string().len())
        .max()
        .unwrap_or(0)
        .max(5);
    let cw = rows.iter().map(|r| r.class.len()).max().unwrap_or(0).max(5);
    let mut t = format!(
        "  {:<nw$}    {:<sw$}    {:>bw$}  {:<cw$}    Attributes\n\n",
        "Name", "Size", "Bytes", "Class"
    );
    for (r, sz) in rows.iter().zip(&sizes) {
        let mut attr = Vec::new();
        if r.global {
            attr.push("global");
        }
        if r.sparse {
            attr.push("sparse");
        }
        if r.complex {
            attr.push("complex");
        }
        let line = format!(
            "  {:<nw$}    {:<sw$}    {:>bw$}  {:<cw$}    {}",
            r.name,
            sz,
            r.bytes,
            r.class,
            attr.join(", ")
        );
        t.push_str(line.trim_end());
        t.push('\n');
    }
    t.push('\n');
    it.print(&t);
    Ok(vec![])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards() {
        assert!(glob("a*", "abc"));
        assert!(glob("*c", "abc"));
        assert!(glob("a*c", "abbbc"));
        assert!(!glob("a*d", "abc"));
        assert!(glob("*", ""));
        assert!(!glob("x", "xy"));
    }

    #[test]
    fn ascii_parse() {
        let v = parse_ascii("1 2\n% c\n3,4\n", "f").unwrap();
        assert_eq!(v, Value::matrix(vec![2, 2], vec![1.0, 3.0, 2.0, 4.0]));
        assert!(parse_ascii("1 2\n3\n", "f").is_err());
        assert!(parse_ascii("a b\n", "f").is_err());
    }
}
