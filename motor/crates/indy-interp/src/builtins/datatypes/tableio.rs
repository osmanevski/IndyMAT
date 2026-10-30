//! Delimited text files and tables: readtable, writetable, readmatrix,
//! writematrix, readcell, writecell, readlines, writelines, readvars,
//! textscan, detectImportOptions, setvartype.
//!
//! Text files (CSV, TXT, DAT, ...) are handled here and `.xlsx` workbooks
//! through `builtins::xlsx`; other spreadsheet formats give an explicit "not
//! supported yet" error. Files are decoded as UTF-8
//! (a leading BOM is dropped; invalid UTF-8 falls back to Latin-1).
//!
//! Parsing model shared by the readers: the text is split into records
//! (one per line; quoted fields may span lines, `""` inside quotes is a
//! literal quote), blank lines are skipped, unquoted fields are trimmed of
//! spaces and tabs. A quote only opens a quoted field when it is the first
//! character of the field; after leading whitespace it is dropped and the
//! field stays unquoted (MATLAB R2025b behaviour).

use std::io::Write;

use indy_value::{Array, Num, NumClass, StructArray, Value, concat, format};

use super::table::{Tab, tab_of};
use super::{cellstr_col, cellstr_row, is_native, num_col, text_list};
use crate::builtins::core::resolve_path;
use crate::builtins::xlsx::{self, Cell};
use crate::builtins::{err, need, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "readtable", readtable);
    reg(it, "writetable", writetable);
    reg(it, "readmatrix", readmatrix);
    reg(it, "writematrix", writematrix);
    reg(it, "readcell", readcell);
    reg(it, "writecell", writecell);
    reg(it, "readlines", readlines);
    reg(it, "writelines", writelines);
    reg(it, "readvars", readvars);
    reg(it, "textscan", textscan);
    reg(it, "detectImportOptions", detect_import_options);
    reg(it, "setvartype", setvartype);
}

// ---------------------------------------------------------------------------
// Option helpers
// ---------------------------------------------------------------------------

type Opts = Vec<(String, Value)>;

fn bad(fname: &str, what: &str, msg: impl Into<String>) -> Box<indy_value::Error> {
    err(&format!("MATLAB:{}:{}", fname, what), msg)
}

/// Name/value pairs (names lower-cased).
fn name_value(a: &[Value], fname: &str) -> R<Opts> {
    if !a.len().is_multiple_of(2) {
        return Err(bad(
            fname,
            "invalidNameValue",
            format!("{}: name-value arguments must come in pairs.", fname),
        ));
    }
    a.chunks(2)
        .map(|p| match (&p[0], p[0].to_str()) {
            (v, Some(n)) if v.is_text() => Ok((n.to_lowercase(), p[1].clone())),
            _ => Err(bad(
                fname,
                "invalidNameValue",
                format!("{}: expected an option name (text).", fname),
            )),
        })
        .collect()
}

fn text_of(v: &Value, fname: &str, name: &str) -> R<String> {
    match v.to_str() {
        Some(s) if v.is_text() => Ok(s),
        _ => Err(bad(
            fname,
            "invalidOption",
            format!("{}: '{}' must be text.", fname, name),
        )),
    }
}

fn bool_of(v: &Value, fname: &str, name: &str) -> R<bool> {
    if v.is_text() {
        return match v.to_str().unwrap_or_default().to_lowercase().as_str() {
            "true" | "on" => Ok(true),
            "false" | "off" => Ok(false),
            _ => Err(bad(
                fname,
                "invalidOption",
                format!("{}: '{}' must be true or false.", fname, name),
            )),
        };
    }
    match (v, v.scalar_f64()) {
        (Value::Num(_) | Value::Bool(_), Some(x)) if !x.is_nan() => Ok(x != 0.0),
        _ => Err(bad(
            fname,
            "invalidOption",
            format!("{}: '{}' must be true or false.", fname, name),
        )),
    }
}

fn count_of(v: &Value, fname: &str, name: &str) -> R<usize> {
    match (v, v.scalar_f64()) {
        (Value::Num(_), Some(x)) if x >= 0.0 && x.fract() == 0.0 && x.is_finite() => Ok(x as usize),
        _ => Err(bad(
            fname,
            "invalidOption",
            format!("{}: '{}' must be a nonnegative integer.", fname, name),
        )),
    }
}

fn unknown(fname: &str, name: &str) -> Box<indy_value::Error> {
    bad(
        fname,
        "unsupportedOption",
        format!("{}: unsupported option '{}'.", fname, name),
    )
}

/// Delimiters of a reader: only spaces/tabs means runs of whitespace.
fn delims_value(v: &Value, fname: &str) -> R<Delims> {
    let d = delim_list(v, fname)?;
    let white = d.iter().all(|x| x == &vec![' '] || x == &vec!['\t']) && d.contains(&vec![' ']);
    Ok(if white { Delims(None) } else { Delims(Some(d)) })
}

/// Map delimiter names and escapes to the delimiter strings.
fn delim_list(v: &Value, fname: &str) -> R<Vec<Vec<char>>> {
    let items = match v {
        Value::Cell(_) | Value::Str(_) => text_list(v),
        Value::Char(_) => v.to_str().map(|s| vec![s]),
        _ => None,
    }
    .ok_or_else(|| {
        bad(
            fname,
            "invalidDelimiter",
            format!("{}: 'Delimiter' must be text.", fname),
        )
    })?;
    let single_char_vec = matches!(v, Value::Char(_));
    let mut out: Vec<Vec<char>> = Vec::new();
    for s in items {
        let mapped = match s.to_lowercase().as_str() {
            "comma" => ",".to_string(),
            "space" => " ".to_string(),
            "tab" => "\t".to_string(),
            "semi" | "semicolon" => ";".to_string(),
            "bar" | "pipe" => "|".to_string(),
            _ => format::escapes(&s),
        };
        if mapped.is_empty() {
            return Err(bad(
                fname,
                "invalidDelimiter",
                format!("{}: 'Delimiter' must not be empty.", fname),
            ));
        }
        if single_char_vec {
            // A char vector lists single-character delimiters.
            for ch in mapped.chars() {
                out.push(vec![ch]);
            }
        } else {
            out.push(mapped.chars().collect());
        }
    }
    out.sort_by_key(|d| std::cmp::Reverse(d.len()));
    out.dedup();
    Ok(out)
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

const SHEETS: &[&str] = &[".xlsx", ".xls", ".xlsm", ".xlsb", ".xltx", ".xltm", ".ods"];
const OTHER_FORMATS: &[&str] = &[".xml", ".docx", ".html", ".htm", ".mat"];

fn ext_of(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    match base.rfind('.') {
        Some(k) => base[k..].to_lowercase(),
        None => String::new(),
    }
}

fn sheet_error(fname: &str, what: &str) -> Box<indy_value::Error> {
    bad(
        fname,
        "spreadsheetUnsupported",
        format!(
            "{}: spreadsheet files ({}) are not supported yet; use .csv or .txt",
            fname, what
        ),
    )
}

/// Reject non-text file types explicitly.
fn check_file_type(fname: &str, name: &str, ftype: Option<&str>) -> R<()> {
    let ext = ext_of(name);
    if let Some(t) = ftype {
        return match t.to_lowercase().as_str() {
            "text" | "delimitedtext" => Ok(()),
            "spreadsheet" => Err(sheet_error(
                fname,
                if ext.is_empty() { "spreadsheet" } else { &ext },
            )),
            _ => Err(bad(
                fname,
                "unsupportedFileType",
                format!(
                    "{}: FileType '{}' is not supported yet; use 'text'",
                    fname, t
                ),
            )),
        };
    }
    if SHEETS.contains(&ext.as_str()) {
        return Err(sheet_error(fname, &ext));
    }
    if OTHER_FORMATS.contains(&ext.as_str()) {
        return Err(bad(
            fname,
            "unsupportedFileType",
            format!(
                "{}: {} files are not supported yet; use .csv or .txt",
                fname, ext
            ),
        ));
    }
    Ok(())
}

fn file_name(v: &Value, fname: &str) -> R<String> {
    match v.to_str() {
        Some(s) if v.is_text() && !s.is_empty() => Ok(s),
        _ => Err(bad(
            fname,
            "invalidFileName",
            format!(
                "{}: the file name must be a nonempty character vector or string scalar.",
                fname
            ),
        )),
    }
}

/// Decode file bytes: BOMs dropped, UTF-8 with a Latin-1 fallback.
pub fn decode_bytes(b: &[u8]) -> String {
    if let Some(r) = b.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return decode_utf8(r);
    }
    if b.len() >= 2 && (b[..2] == [0xFF, 0xFE] || b[..2] == [0xFE, 0xFF]) {
        let le = b[0] == 0xFF;
        let units: Vec<u16> = b[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| {
                if le {
                    u16::from_le_bytes(*c)
                } else {
                    u16::from_be_bytes(*c)
                }
            })
            .collect();
        return String::from_utf16_lossy(&units);
    }
    decode_utf8(b)
}

fn decode_utf8(b: &[u8]) -> String {
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => b.iter().map(|&x| x as char).collect(),
    }
}

fn read_file(it: &Interp, fname: &str, name: &str) -> R<String> {
    let p = resolve_path(it, name);
    match std::fs::read(&p) {
        Ok(b) => Ok(decode_bytes(&b)),
        Err(_) => Err(bad(
            fname,
            "FileNotFound",
            format!(
                "{}: Unable to find or open '{}'. Check the path and filename or file permissions.",
                fname, name
            ),
        )),
    }
}

fn write_file(it: &Interp, fname: &str, name: &str, text: &str, append: bool) -> R<()> {
    let p = resolve_path(it, name);
    let mut o = std::fs::OpenOptions::new();
    if append {
        o.append(true).create(true);
    } else {
        o.write(true).create(true).truncate(true);
    }
    let res = o.open(&p).and_then(|mut f| f.write_all(text.as_bytes()));
    res.map_err(|e| {
        bad(
            fname,
            "FileOpenError",
            format!("{}: unable to write file '{}': {}", fname, name, e),
        )
    })
}

fn write_mode(v: &Value, fname: &str) -> R<bool> {
    match text_of(v, fname, "WriteMode")?.to_lowercase().as_str() {
        "overwrite" => Ok(false),
        "append" => Ok(true),
        m => Err(bad(
            fname,
            "invalidWriteMode",
            format!(
                "{}: WriteMode '{}' is not supported for text files; use 'overwrite' or 'append'.",
                fname, m
            ),
        )),
    }
}

/// Whether a file exists and is non-empty (append without header).
fn has_content(it: &Interp, name: &str) -> bool {
    std::fs::metadata(resolve_path(it, name)).is_ok_and(|m| m.len() > 0)
}

// ---------------------------------------------------------------------------
// Record splitting
// ---------------------------------------------------------------------------

/// What a field held in a spreadsheet; `NotSheet` for text files and blanks.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum SheetKind {
    #[default]
    NotSheet,
    Text,
    Num,
    Bool,
    Date,
}

#[derive(Clone, Debug, Default)]
struct Field {
    s: String,
    quoted: bool,
    sheet: SheetKind,
}

#[derive(Clone, Debug)]
struct Record {
    /// 1-based physical line where the record starts.
    line: usize,
    fields: Vec<Field>,
}

/// Delimiters: `None` means runs of whitespace.
#[derive(Clone, Debug)]
struct Delims(Option<Vec<Vec<char>>>);

impl Delims {
    fn white(&self) -> bool {
        self.0.is_none()
    }
    fn len_at(&self, c: &[char], i: usize) -> usize {
        if i >= c.len() {
            return 0;
        }
        match &self.0 {
            None => usize::from(c[i] == ' ' || c[i] == '\t'),
            Some(ds) => ds
                .iter()
                .find(|d| c[i..].starts_with(d))
                .map_or(0, |d| d.len()),
        }
    }
    fn is_delim_char(&self, ch: char) -> bool {
        match &self.0 {
            None => ch == ' ' || ch == '\t',
            Some(ds) => ds.iter().any(|d| d.len() == 1 && d[0] == ch),
        }
    }
}

fn is_eol(ch: char) -> bool {
    ch == '\n' || ch == '\r'
}

fn skip_eol(c: &[char], i: usize) -> usize {
    if i < c.len() && c[i] == '\r' && i + 1 < c.len() && c[i + 1] == '\n' {
        i + 2
    } else {
        i + 1
    }
}

fn split_records(text: &str, d: &Delims, multi: bool) -> Vec<Record> {
    let c: Vec<char> = text.chars().collect();
    let n = c.len();
    let mut line = 1;
    let mut i = 0;
    let mut out = Vec::new();
    while i < n {
        let mut j = i;
        while j < n && (c[j] == ' ' || c[j] == '\t') {
            j += 1;
        }
        if j >= n {
            break;
        }
        if is_eol(c[j]) {
            i = skip_eol(&c, j);
            line += 1;
            continue;
        }
        if d.white() {
            i = j;
        }
        let start = line;
        let mut fields = Vec::new();
        loop {
            let (f, ni, nl) = parse_field(&c, i, d);
            fields.push(f);
            line += nl;
            i = ni;
            if i >= n {
                break;
            }
            if is_eol(c[i]) {
                i = skip_eol(&c, i);
                line += 1;
                break;
            }
            let k = d.len_at(&c, i);
            if k == 0 {
                // parse_field stops only at a delimiter, EOL or EOF
                i += 1;
                continue;
            }
            i += k;
            if multi || d.white() {
                loop {
                    let mut j = i;
                    if !d.white() {
                        while j < n && (c[j] == ' ' || c[j] == '\t') && !d.is_delim_char(c[j]) {
                            j += 1;
                        }
                    }
                    let k = d.len_at(&c, j);
                    if k == 0 {
                        break;
                    }
                    i = j + k;
                }
            }
            if d.white() && (i >= n || is_eol(c[i])) {
                if i < n {
                    i = skip_eol(&c, i);
                    line += 1;
                }
                break;
            }
        }
        out.push(Record {
            line: start,
            fields,
        });
    }
    out
}

/// One field starting at `i`: (field, next index at delimiter/EOL/EOF,
/// newlines consumed inside quotes).
fn parse_field(c: &[char], mut i: usize, d: &Delims) -> (Field, usize, usize) {
    let n = c.len();
    let mut lead = false;
    while i < n && (c[i] == ' ' || c[i] == '\t') && !d.is_delim_char(c[i]) {
        i += 1;
        lead = true;
    }
    let mut s = String::new();
    let mut nl = 0;
    let mut quoted = false;
    if i < n && c[i] == '"' {
        i += 1;
        if !lead {
            quoted = true;
            while i < n {
                if c[i] == '"' {
                    if i + 1 < n && c[i + 1] == '"' {
                        s.push('"');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                if c[i] == '\n' || (c[i] == '\r' && !(i + 1 < n && c[i + 1] == '\n')) {
                    nl += 1;
                }
                s.push(c[i]);
                i += 1;
            }
        }
    }
    let mut tail = String::new();
    while i < n && !is_eol(c[i]) && d.len_at(c, i) == 0 {
        tail.push(c[i]);
        i += 1;
    }
    if quoted {
        s.push_str(tail.trim_end_matches([' ', '\t']));
    } else {
        s = tail.trim_end_matches([' ', '\t']).to_string();
    }
    (
        Field {
            s,
            quoted,
            sheet: SheetKind::NotSheet,
        },
        i,
        nl,
    )
}

/// Pick a delimiter from up to 32 records after `skip` lines: the
/// candidate whose most common nonzero per-line count is the most
/// consistent (then more columns, then candidate order). No candidate
/// found: whitespace when lines have several tokens, else comma.
fn detect_delims(text: &str, skip: usize) -> Delims {
    const CANDS: [char; 4] = [',', '\t', ';', '|'];
    let c: Vec<char> = text.chars().collect();
    let n = c.len();
    let mut counts: Vec<[usize; 4]> = Vec::new();
    let mut row = [0usize; 4];
    let (mut quoted, mut nonempty, mut line) = (false, false, 1usize);
    let mut white_multi = false;
    let mut tokens = 0usize;
    let mut in_tok = false;
    let mut i = 0;
    while i < n && counts.len() < 32 {
        let ch = c[i];
        if ch == '"' {
            nonempty = true;
            if quoted && i + 1 < n && c[i + 1] == '"' {
                i += 1;
            } else {
                quoted = !quoted;
            }
        } else if !quoted && is_eol(ch) {
            if line > skip && nonempty {
                counts.push(row);
                if tokens + usize::from(in_tok) > 1 {
                    white_multi = true;
                }
            }
            row = [0; 4];
            nonempty = false;
            tokens = 0;
            in_tok = false;
            line += 1;
            if ch == '\r' && i + 1 < n && c[i + 1] == '\n' {
                i += 1;
            }
        } else if !quoted {
            if let Some(k) = CANDS.iter().position(|&x| x == ch) {
                row[k] += 1;
            }
            if ch == ' ' || ch == '\t' {
                if in_tok {
                    tokens += 1;
                }
                in_tok = false;
            } else {
                nonempty = true;
                in_tok = true;
            }
        }
        i += 1;
    }
    if counts.len() < 32 && line > skip && nonempty {
        counts.push(row);
        if tokens + usize::from(in_tok) > 1 {
            white_multi = true;
        }
    }
    let mut best: Option<(usize, usize, usize)> = None; // (support, width, k)
    for k in 0..CANDS.len() {
        let pos: Vec<usize> = counts.iter().map(|r| r[k]).filter(|&x| x > 0).collect();
        if pos.is_empty() {
            continue;
        }
        let mut freq: Vec<(usize, usize)> = Vec::new();
        for &x in &pos {
            match freq.iter_mut().find(|(v, _)| *v == x) {
                Some(e) => e.1 += 1,
                None => freq.push((x, 1)),
            }
        }
        // mode, smallest value on ties
        let (width, _) = freq
            .iter()
            .copied()
            .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
            .unwrap_or((0, 0));
        let support = counts.iter().filter(|r| r[k] == width).count();
        let better = match best {
            None => true,
            Some((s, w, _)) => support > s || (support == s && width > w),
        };
        if better {
            best = Some((support, width, k));
        }
    }
    match best {
        Some((_, _, k)) => Delims(Some(vec![vec![CANDS[k]]])),
        None if white_multi => Delims(None),
        None => Delims(Some(vec![vec![',']])),
    }
}

// ---------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------

/// MATLAB-style number text: [+-] digits [. digits] [e[+-]digits], Inf, NaN.
fn parse_number(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let (neg, body) = match t.as_bytes()[0] {
        b'-' => (true, &t[1..]),
        b'+' => (false, &t[1..]),
        _ => (false, t),
    };
    let lower = body.to_ascii_lowercase();
    let v = match lower.as_str() {
        "inf" | "infinity" => f64::INFINITY,
        "nan" => f64::NAN,
        _ => {
            let b = lower.as_bytes();
            let mut i = 0;
            let mut digits = 0;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
                digits += 1;
            }
            if i < b.len() && b[i] == b'.' {
                i += 1;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                    digits += 1;
                }
            }
            if digits == 0 {
                return None;
            }
            if i < b.len() && (b[i] == b'e' || b[i] == b'd') {
                i += 1;
                if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
                    i += 1;
                }
                let st = i;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                if i == st {
                    return None;
                }
            }
            if i != b.len() {
                return None;
            }
            lower.replace('d', "e").parse::<f64>().ok()?
        }
    };
    Some(if neg { -v } else { v })
}

/// `%.15g` (double), `%.7g` (single), integers as integers.
fn fmt_real(x: f64, class: NumClass) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Inf".into() } else { "-Inf".into() };
    }
    match class {
        NumClass::Double => format::fmt_g(x, 15, false, false),
        NumClass::Single => format::fmt_g(x, 7, false, false),
        NumClass::UInt64 => format!("{}", x as u64),
        _ => format!("{}", x as i64),
    }
}

fn fmt_elem(n: &Num, i: usize) -> String {
    let re = fmt_real(n.re[i], n.class);
    match &n.im {
        Some(im) if im[i] != 0.0 || im[i].is_nan() => {
            let y = im[i];
            let sign = if y < 0.0 || (y == 0.0 && y.is_sign_negative()) {
                "-"
            } else {
                "+"
            };
            format!("{}{}{}i", re, sign, fmt_real(y.abs(), n.class))
        }
        _ => re,
    }
}

/// Quote text when it contains the delimiter, quotes or line breaks (or
/// always when `force`).
fn quote(s: &str, delim: &str, force: bool) -> String {
    let needs = force
        || s.contains(delim)
        || s.contains('"')
        || s.contains('\n')
        || s.contains('\r')
        || (delim == " " && s.contains('\t'));
    if needs {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

// ---------------------------------------------------------------------------
// Variable names
// ---------------------------------------------------------------------------

const KEYWORDS: &[&str] = &[
    "break",
    "case",
    "catch",
    "classdef",
    "continue",
    "else",
    "elseif",
    "end",
    "for",
    "function",
    "global",
    "if",
    "otherwise",
    "parfor",
    "persistent",
    "return",
    "spmd",
    "switch",
    "try",
    "while",
];

/// `matlab.lang.makeValidName`: whitespace removed (a following lowercase
/// letter is capitalized), invalid characters become `_`, a leading
/// non-letter gets the prefix `x`, keywords become `xKeyword`.
pub fn make_valid_name(s: &str) -> String {
    let mut out = String::new();
    let mut cap = false;
    for ch in s.trim().chars() {
        if ch.is_whitespace() {
            cap = !out.is_empty();
            continue;
        }
        let ch = if cap && ch.is_ascii_lowercase() {
            ch.to_ascii_uppercase()
        } else {
            ch
        };
        cap = false;
        out.push(if ch.is_ascii_alphanumeric() || ch == '_' {
            ch
        } else {
            '_'
        });
    }
    if out.is_empty() {
        return "x".into();
    }
    if KEYWORDS.contains(&out.as_str()) {
        let mut c = out.chars();
        let first = c.next().map(|x| x.to_ascii_uppercase()).unwrap_or('x');
        out = format!("x{}{}", first, c.as_str());
    } else if !out.starts_with(|c: char| c.is_ascii_alphabetic()) {
        out = format!("x{}", out);
    }
    out.chars().take(63).collect()
}

/// Make names unique (later duplicates get `_1`, `_2`, ...); reserved
/// table names are suffixed too.
fn make_unique(names: &mut [String]) {
    for k in 0..names.len() {
        let reserved = |n: &str| matches!(n, "Properties" | "RowNames" | "VariableNames");
        if !names[..k].contains(&names[k]) && !reserved(&names[k]) {
            continue;
        }
        let base = names[k].clone();
        let mut j = 1;
        loop {
            let cand = format!("{}_{}", base, j);
            if !names.contains(&cand) {
                names[k] = cand;
                break;
            }
            j += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Table layout (shared by readtable / detectImportOptions)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum ColType {
    Double,
    Num(NumClass),
    Logical,
    Char,
    Str,
    Other(String),
    /// Excel serial dates of a spreadsheet column.
    ExcelDate,
}

impl ColType {
    fn name(&self) -> String {
        match self {
            ColType::Double => "double".into(),
            ColType::Num(c) => c.name().into(),
            ColType::Logical => "logical".into(),
            ColType::Char => "char".into(),
            ColType::Str => "string".into(),
            ColType::Other(s) => s.clone(),
            ColType::ExcelDate => "datetime".into(),
        }
    }
    fn parse(s: &str, fname: &str) -> R<ColType> {
        let l = s.to_lowercase();
        Ok(match l.as_str() {
            "double" => ColType::Double,
            "logical" => ColType::Logical,
            "char" | "cellstr" => ColType::Char,
            "string" => ColType::Str,
            "categorical" | "datetime" | "duration" => ColType::Other(l),
            _ => match NumClass::from_name(&l) {
                Some(c) => ColType::Num(c),
                None => {
                    return Err(bad(
                        fname,
                        "invalidVariableType",
                        format!("{}: unsupported variable type '{}'.", fname, s),
                    ));
                }
            },
        })
    }
}

#[derive(Clone, Debug, Default)]
struct ReadSpec {
    delims: Option<Delims>,
    multi: bool,
    header_lines: usize,
    read_names: Option<bool>,
    names_line: Option<usize>,
    data_start: Option<usize>,
    data_end: Option<usize>,
    var_names: Option<Vec<String>>,
    var_types: Option<Vec<String>>,
    selected: Option<Vec<String>>,
    text_string: bool,
    preserve: bool,
    row_names: bool,
    treat_missing: Vec<String>,
    file_type: Option<String>,
    sheet: Option<Value>,
    range: Option<Value>,
}

struct Layout {
    delims: Delims,
    /// 0 = no header line.
    names_line: usize,
    data_start: usize,
    raw_names: Vec<String>,
    names: Vec<String>,
    types: Vec<ColType>,
    data: Vec<Record>,
}

fn is_numeric_field(f: &Field, missing: &[String]) -> bool {
    parse_number(&f.s).is_some() || missing.contains(&f.s)
}

fn layout(text: &str, spec: &ReadSpec) -> Layout {
    let skip = spec
        .names_line
        .map(|l| l.saturating_sub(1))
        .unwrap_or(spec.header_lines);
    let delims = spec
        .delims
        .clone()
        .unwrap_or_else(|| detect_delims(text, skip));
    let multi = spec.multi || delims.white();
    let recs = split_records(text, &delims, multi);
    layout_records(recs, delims, spec)
}

fn layout_records(recs: Vec<Record>, delims: Delims, spec: &ReadSpec) -> Layout {
    let (names_line, mut header, data): (usize, Option<Record>, Vec<Record>) =
        if let Some(l) = spec.names_line {
            let start = spec.data_start.unwrap_or(l + 1);
            let h = if l == 0 {
                None
            } else {
                recs.iter().find(|r| r.line == l).cloned()
            };
            let d = recs.into_iter().filter(|r| r.line >= start).collect();
            (l, h, d)
        } else {
            let mut rest: Vec<Record> = recs
                .into_iter()
                .filter(|r| r.line > spec.header_lines)
                .collect();
            if let Some(s) = spec.data_start {
                // explicit DataLines without a names line: no header
                rest.retain(|r| r.line >= s);
                (0, None, rest)
            } else {
                let want = match spec.read_names {
                    Some(b) => b,
                    None => rest.first().is_some_and(|r| {
                        r.fields.iter().any(|f| !f.s.is_empty())
                            && r.fields.iter().all(|f| {
                                f.s.is_empty()
                                    || f.sheet == SheetKind::Text
                                    || parse_number(&f.s).is_none()
                            })
                    }),
                };
                if want && !rest.is_empty() {
                    let h = rest.remove(0);
                    (h.line, Some(h), rest)
                } else {
                    (0, None, rest)
                }
            }
        };
    let mut data = data;
    if let Some(e) = spec.data_end {
        data.retain(|r| r.line <= e);
    }
    let data_start = data.first().map_or(names_line + 1, |r| r.line);
    if spec.read_names == Some(false) {
        header = None;
    }
    let hlen = header.as_ref().map_or(0, |h| h.fields.len());
    let dlen = data.iter().map(|r| r.fields.len()).max().unwrap_or(0);
    let ncols = hlen.max(dlen);
    let mut raw = Vec::new();
    let mut names = Vec::new();
    for k in 0..ncols {
        let r = header
            .as_ref()
            .and_then(|h| h.fields.get(k))
            .map(|f| f.s.clone())
            .unwrap_or_default();
        let n = if header.is_none() {
            format!("Var{}", k + 1)
        } else if k >= hlen {
            format!("ExtraVar{}", k + 1 - hlen)
        } else if r.trim().is_empty() {
            format!("Var{}", k + 1)
        } else if spec.preserve {
            r.clone()
        } else {
            make_valid_name(&r)
        };
        raw.push(r);
        names.push(n);
    }
    make_unique(&mut names);
    let types = (0..ncols)
        .map(|k| {
            let mut any = false;
            let all_num = data.iter().all(|r| match r.fields.get(k) {
                Some(f) if !f.s.is_empty() => {
                    any = true;
                    is_numeric_field(f, &spec.treat_missing)
                }
                _ => true,
            });
            let all_dates = any
                && data.iter().all(|r| match r.fields.get(k) {
                    Some(f) if !f.s.is_empty() => super::time::looks_like_date(&f.s),
                    _ => true,
                });
            if all_num || !any {
                ColType::Double
            } else if all_dates {
                ColType::Other("datetime".into())
            } else if spec.text_string {
                ColType::Str
            } else {
                ColType::Char
            }
        })
        .collect();
    Layout {
        delims,
        names_line,
        data_start,
        raw_names: raw,
        names,
        types,
        data,
    }
}

fn field_at(r: &Record, k: usize) -> &str {
    r.fields.get(k).map_or("", |f| f.s.as_str())
}

fn build_column(
    it: &mut Interp,
    data: &[Record],
    k: usize,
    ty: &ColType,
    missing: &[String],
    fname: &str,
) -> R<Value> {
    let h = data.len();
    let num = |missing: &[String]| -> Vec<f64> {
        data.iter()
            .map(|r| {
                let s = field_at(r, k);
                if missing.iter().any(|m| m == s) {
                    f64::NAN
                } else {
                    parse_number(s).unwrap_or(f64::NAN)
                }
            })
            .collect()
    };
    let texts = || -> Vec<String> { data.iter().map(|r| field_at(r, k).to_string()).collect() };
    Ok(match ty {
        ColType::Double => num_col(num(missing)),
        ColType::Num(c) => Value::Num(Num::real(vec![h, 1], num(missing)).with_class(*c)),
        ColType::Logical => {
            let mut b = Vec::with_capacity(h);
            for r in data {
                let s = field_at(r, k).trim().to_lowercase();
                b.push(match s.as_str() {
                    "true" | "t" => true,
                    "false" | "f" | "" => false,
                    _ => parse_number(&s).is_some_and(|x| x != 0.0 && !x.is_nan()),
                });
            }
            Value::Bool(Array::new(vec![h, 1], b))
        }
        ColType::Char => cellstr_col(&texts()),
        ColType::Str => Value::Str(Array::new(
            vec![h, 1],
            texts().into_iter().map(Some).collect(),
        )),
        ColType::ExcelDate => excel_dates(it, num_col(num(missing)), fname)?,
        ColType::Other(cls)
            if cls == "datetime"
                && data
                    .iter()
                    .any(|r| r.fields.get(k).is_some_and(|f| f.sheet == SheetKind::Date)) =>
        {
            excel_dates(it, num_col(num(missing)), fname)?
        }
        ColType::Other(cls) => {
            let r = it.call_named(cls, vec![cellstr_col(&texts())], 1)?;
            r.into_iter().next().ok_or_else(|| {
                bad(
                    fname,
                    "conversionFailed",
                    format!("{}: could not convert a column to {}.", fname, cls),
                )
            })?
        }
    })
}

fn read_spec(it: &mut Interp, a: &[Value], fname: &str) -> R<ReadSpec> {
    let mut spec = ReadSpec::default();
    let mut rest = a;
    if let Some(Value::Struct(s)) = a.first() {
        apply_import_struct(&mut spec, s, fname)?;
        rest = &a[1..];
    }
    let _ = it;
    for (n, v) in name_value(rest, fname)? {
        match n.as_str() {
            "delimiter" => spec.delims = Some(delims_value(&v, fname)?),
            "readvariablenames" => spec.read_names = Some(bool_of(&v, fname, "ReadVariableNames")?),
            "numheaderlines" | "headerlines" => {
                spec.header_lines = count_of(&v, fname, "NumHeaderLines")?
            }
            "variablenames" => {
                spec.var_names = Some(text_list(&v).ok_or_else(|| {
                    bad(
                        fname,
                        "invalidVariableNames",
                        format!("{}: 'VariableNames' must be a cell array of character vectors or a string array.", fname),
                    )
                })?)
            }
            "selectedvariablenames" => spec.selected = text_list(&v),
            "variabletypes" => spec.var_types = text_list(&v),
            "variablenameslinenumber" | "variablenamesline" => {
                spec.names_line = Some(count_of(&v, fname, "VariableNamesLine")?)
            }
            "datalines" | "datarange" => {
                let (s, e) = data_lines(&v, fname)?;
                spec.data_start = Some(s);
                spec.data_end = e;
            }
            "filetype" => spec.file_type = Some(text_of(&v, fname, "FileType")?),
            "texttype" => {
                spec.text_string = match text_of(&v, fname, "TextType")?.to_lowercase().as_str() {
                    "char" => false,
                    "string" => true,
                    _ => {
                        return Err(bad(
                            fname,
                            "invalidTextType",
                            format!("{}: TextType must be 'char' or 'string'.", fname),
                        ));
                    }
                }
            }
            "variablenamingrule" => {
                spec.preserve = match text_of(&v, fname, "VariableNamingRule")?.to_lowercase().as_str() {
                    "modify" => false,
                    "preserve" => true,
                    _ => {
                        return Err(bad(
                            fname,
                            "invalidNamingRule",
                            format!("{}: VariableNamingRule must be 'modify' or 'preserve'.", fname),
                        ));
                    }
                }
            }
            "readrownames" => spec.row_names = bool_of(&v, fname, "ReadRowNames")?,
            "multipledelimsasone" => spec.multi = bool_of(&v, fname, "MultipleDelimsAsOne")?,
            "consecutivedelimitersrule" => {
                spec.multi = text_of(&v, fname, "ConsecutiveDelimitersRule")?
                    .eq_ignore_ascii_case("join")
            }
            "treatasmissing" | "treatasempty" => {
                spec.treat_missing = text_list(&v).unwrap_or_default()
            }
            "encoding" => {
                let e = text_of(&v, fname, "Encoding")?.to_lowercase();
                if !matches!(e.as_str(), "utf-8" | "utf8" | "system" | "us-ascii") {
                    return Err(bad(
                        fname,
                        "unsupportedEncoding",
                        format!("{}: only UTF-8 text is supported (Encoding '{}').", fname, e),
                    ));
                }
            }
            "emptylinerule" | "whitespace" | "endofline" | "lineending" => {}
            "sheet" => spec.sheet = Some(v.clone()),
            "range" => spec.range = Some(v.clone()),
            "useexcel" => use_excel_off(&v, fname)?,
            "format" => {
                return Err(bad(
                    fname,
                    "unsupportedOption",
                    format!("{}: option '{}' is not supported yet.", fname, n),
                ));
            }
            _ => return Err(unknown(fname, &n)),
        }
    }
    Ok(spec)
}

fn data_lines(v: &Value, fname: &str) -> R<(usize, Option<usize>)> {
    if let Some(s) = v.to_str().filter(|_| v.is_text()) {
        // 'A2' style range: the row number
        let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
        return digits
            .parse::<usize>()
            .map(|r| (r.max(1), None))
            .map_err(|_| {
                bad(
                    fname,
                    "invalidDataLines",
                    format!("{}: invalid DataLines.", fname),
                )
            });
    }
    let x = v.to_f64_vec().unwrap_or_default();
    match x.as_slice() {
        [s] if *s >= 1.0 && s.is_finite() => Ok((*s as usize, None)),
        [s, e] if *s >= 1.0 && s.is_finite() => Ok((
            *s as usize,
            if e.is_finite() {
                Some(*e as usize)
            } else {
                None
            },
        )),
        _ => Err(bad(
            fname,
            "invalidDataLines",
            format!(
                "{}: DataLines must be a positive scalar or [start end].",
                fname
            ),
        )),
    }
}

/// Import options produced by detectImportOptions (a struct).
fn apply_import_struct(spec: &mut ReadSpec, s: &StructArray, fname: &str) -> R<()> {
    let get = |k: &str| s.get(0, k).cloned();
    if let Some(v) = get("Delimiter") {
        spec.delims = Some(delims_value(&v, fname)?);
    }
    if let Some(v) = get("VariableNamesLine") {
        spec.names_line = Some(count_of(&v, fname, "VariableNamesLine")?);
    }
    if let Some(v) = get("DataLines") {
        let (s, e) = data_lines(&v, fname)?;
        spec.data_start = Some(s);
        spec.data_end = e;
    }
    if let Some(v) = get("VariableNames") {
        spec.var_names = text_list(&v);
    }
    if let Some(v) = get("VariableTypes") {
        spec.var_types = text_list(&v);
    }
    if let Some(v) = get("SelectedVariableNames") {
        spec.selected = text_list(&v);
    }
    if let Some(v) = get("ConsecutiveDelimitersRule") {
        spec.multi = v.to_str().is_some_and(|x| x.eq_ignore_ascii_case("join"));
    }
    if let Some(v) = get("VariableNamingRule") {
        spec.preserve = v
            .to_str()
            .is_some_and(|x| x.eq_ignore_ascii_case("preserve"));
    }
    if let Some(v) = get("TreatAsMissing") {
        spec.treat_missing = text_list(&v).unwrap_or_default();
    }
    Ok(())
}

fn read_table_value(it: &mut Interp, a: Vec<Value>, fname: &str) -> R<Tab> {
    need(&a, 1, fname)?;
    let name = file_name(&a[0], fname)?;
    let spec = read_spec(it, &a[1..], fname)?;
    let mut lay = if xlsx::is_target(&name, spec.file_type.as_deref()) {
        reject_text_options(&a[1..], fname)?;
        let recs = sheet_records(it, fname, &name, spec.sheet.as_ref(), spec.range.as_ref())?;
        let mut l = layout_records(recs, Delims(Some(Vec::new())), &spec);
        l.types = sheet_types(&l.data, l.names.len(), &spec);
        l
    } else {
        text_file_only(spec.sheet.as_ref(), spec.range.as_ref(), fname)?;
        check_file_type(fname, &name, spec.file_type.as_deref())?;
        let text = read_file(it, fname, &name)?;
        layout(&text, &spec)
    };
    let ncols = lay.names.len();
    if let Some(vn) = &spec.var_names {
        if vn.len() != ncols && ncols > 0 {
            return Err(bad(
                fname,
                "VariableNamesMismatch",
                format!(
                    "{}: the number of VariableNames ({}) must match the number of columns in the file ({}).",
                    fname,
                    vn.len(),
                    ncols
                ),
            ));
        }
        if ncols > 0 {
            lay.raw_names = vn.clone();
            lay.names = vn.clone();
        }
    }
    if let Some(vt) = &spec.var_types {
        if vt.len() != ncols {
            return Err(bad(
                fname,
                "VariableTypesMismatch",
                format!(
                    "{}: the number of VariableTypes ({}) must match the number of columns ({}).",
                    fname,
                    vt.len(),
                    ncols
                ),
            ));
        }
        lay.types = vt
            .iter()
            .map(|t| ColType::parse(t, fname))
            .collect::<R<Vec<_>>>()?;
    }
    let mut cols: Vec<usize> = (0..ncols).collect();
    let mut rows: Vec<String> = Vec::new();
    let mut row_dim: Option<String> = None;
    if spec.row_names && ncols > 0 {
        cols.remove(0);
        rows = lay
            .data
            .iter()
            .map(|r| field_at(r, 0).to_string())
            .collect();
        make_unique_rows(&mut rows);
        if lay.names_line > 0 && !lay.raw_names[0].trim().is_empty() {
            row_dim = Some(lay.names[0].clone());
        }
    }
    if let Some(sel) = &spec.selected {
        let mut picked = Vec::new();
        for s in sel {
            match cols.iter().find(|&&k| &lay.names[k] == s) {
                Some(&k) => picked.push(k),
                None => {
                    return Err(bad(
                        fname,
                        "UnknownVariable",
                        format!(
                            "{}: unknown variable name '{}' in SelectedVariableNames.",
                            fname, s
                        ),
                    ));
                }
            }
        }
        cols = picked;
    }
    let mut t = Tab::new(lay.data.len());
    let mut modified = false;
    for &k in &cols {
        let v = build_column(it, &lay.data, k, &lay.types[k], &spec.treat_missing, fname)?;
        t.names.push(lay.names[k].clone());
        t.vars.push(v);
        if lay.names_line > 0 && lay.raw_names[k] != lay.names[k] && !lay.raw_names[k].is_empty() {
            modified = true;
        }
    }
    if modified {
        t.vdesc = cols
            .iter()
            .map(|&k| {
                if lay.raw_names[k] != lay.names[k] && !lay.raw_names[k].is_empty() {
                    format!("Original column heading: '{}'", lay.raw_names[k])
                } else {
                    String::new()
                }
            })
            .collect();
    }
    t.rows = rows;
    if let Some(d) = row_dim
        && d != t.dimnames[1]
    {
        t.dimnames[0] = d;
    }
    Ok(t)
}

fn make_unique_rows(rows: &mut [String]) {
    for k in 0..rows.len() {
        if rows[k].is_empty() {
            rows[k] = format!("Row{}", k + 1);
        }
        if rows[..k].contains(&rows[k]) {
            let base = rows[k].clone();
            let mut j = 1;
            while rows.contains(&format!("{}_{}", base, j)) {
                j += 1;
            }
            rows[k] = format!("{}_{}", base, j);
        }
    }
}

fn readtable(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let t = read_table_value(it, a, "readtable")?;
    Ok(vec![t.value()])
}

fn readvars(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let t = read_table_value(it, a, "readvars")?;
    if nargout > t.vars.len() {
        return Err(bad(
            "readvars",
            "TooManyOutputs",
            format!(
                "readvars: {} outputs requested but the file has {} variables.",
                nargout,
                t.vars.len()
            ),
        ));
    }
    Ok(t.vars.into_iter().take(nargout.max(1)).collect())
}

fn detect_import_options(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let fname = "detectImportOptions";
    need(&a, 1, fname)?;
    let name = file_name(&a[0], fname)?;
    let spec = read_spec(it, &a[1..], fname)?;
    check_file_type(fname, &name, spec.file_type.as_deref())?;
    let text = read_file(it, fname, &name)?;
    let lay = layout(&text, &spec);
    let mut s = StructArray::new_scalar();
    let delim = match &lay.delims.0 {
        None => vec![" ".to_string(), "\t".to_string()],
        Some(ds) => ds.iter().map(|d| d.iter().collect()).collect(),
    };
    s.set(0, "Delimiter", cellstr_row(&delim));
    s.set(0, "Whitespace", Value::str("\\b\\t "));
    s.set(
        0,
        "LineEnding",
        cellstr_row(&["\\n".into(), "\\r".into(), "\\r\\n".into()]),
    );
    s.set(0, "Encoding", Value::str("UTF-8"));
    s.set(0, "VariableNames", cellstr_row(&lay.names));
    s.set(
        0,
        "VariableTypes",
        cellstr_row(&lay.types.iter().map(|t| t.name()).collect::<Vec<_>>()),
    );
    s.set(0, "SelectedVariableNames", cellstr_row(&lay.names));
    s.set(0, "VariableNamesLine", Value::num(lay.names_line as f64));
    s.set(
        0,
        "DataLines",
        Value::row(vec![lay.data_start as f64, f64::INFINITY]),
    );
    s.set(
        0,
        "ConsecutiveDelimitersRule",
        Value::str(if spec.multi || lay.delims.white() {
            "join"
        } else {
            "split"
        }),
    );
    s.set(
        0,
        "VariableNamingRule",
        Value::str(if spec.preserve { "preserve" } else { "modify" }),
    );
    s.set(0, "ExtraColumnsRule", Value::str("addvars"));
    s.set(0, "EmptyLineRule", Value::str("skip"));
    s.set(0, "MissingRule", Value::str("fill"));
    s.set(0, "ImportErrorRule", Value::str("fill"));
    Ok(vec![Value::Struct(s)])
}

/// `opts = setvartype(opts, type)` / `setvartype(opts, names, type)`.
fn setvartype(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let fname = "setvartype";
    need(&a, 2, fname)?;
    let Value::Struct(mut s) = a[0].clone() else {
        return Err(bad(
            fname,
            "invalidOptions",
            "setvartype: the first argument must be import options from detectImportOptions.",
        ));
    };
    let names = s
        .get(0, "VariableNames")
        .and_then(text_list)
        .unwrap_or_default();
    let mut types = s
        .get(0, "VariableTypes")
        .and_then(text_list)
        .unwrap_or_else(|| vec!["double".into(); names.len()]);
    types.resize(names.len(), "double".into());
    let (sel, ty): (Vec<usize>, Vec<String>) = if a.len() == 2 {
        let t = text_list(&a[1]).unwrap_or_default();
        ((0..names.len()).collect(), t)
    } else {
        let wanted: Vec<usize> = match &a[1] {
            Value::Num(n) => {
                n.re.iter()
                    .map(|x| (*x as usize).saturating_sub(1))
                    .collect()
            }
            v => {
                let w = text_list(v).unwrap_or_default();
                let mut idx = Vec::new();
                for n in w {
                    match names.iter().position(|x| *x == n) {
                        Some(k) => idx.push(k),
                        None => {
                            return Err(bad(
                                fname,
                                "UnknownVariable",
                                format!("setvartype: unknown variable name '{}'.", n),
                            ));
                        }
                    }
                }
                idx
            }
        };
        (wanted, text_list(&a[2]).unwrap_or_default())
    };
    for (j, &k) in sel.iter().enumerate() {
        let t = if ty.len() == 1 {
            ty[0].clone()
        } else {
            ty.get(j).cloned().unwrap_or_default()
        };
        ColType::parse(&t, fname)?;
        if k >= types.len() {
            return Err(bad(
                fname,
                "badIndex",
                "setvartype: variable index out of range.",
            ));
        }
        types[k] = t;
    }
    s.set(0, "VariableTypes", cellstr_row(&types));
    Ok(vec![Value::Struct(s)])
}

// ---------------------------------------------------------------------------
// writetable
// ---------------------------------------------------------------------------

struct WriteSpec {
    delim: String,
    names: bool,
    row_names: bool,
    append: bool,
    quote_all: Option<bool>,
}

fn write_spec(a: &[Value], fname: &str, default_names: bool) -> R<WriteSpec> {
    let mut w = WriteSpec {
        delim: ",".into(),
        names: default_names,
        row_names: false,
        append: false,
        quote_all: None,
    };
    let mut ftype: Option<String> = None;
    for (n, v) in name_value(a, fname)? {
        match n.as_str() {
            "delimiter" => {
                let d = delim_list(&v, fname)?;
                if d.len() != 1 {
                    return Err(bad(
                        fname,
                        "invalidDelimiter",
                        format!("{}: specify a single delimiter.", fname),
                    ));
                }
                w.delim = d[0].iter().collect();
            }
            "writevariablenames" => w.names = bool_of(&v, fname, "WriteVariableNames")?,
            "writerownames" => w.row_names = bool_of(&v, fname, "WriteRowNames")?,
            "writemode" => w.append = write_mode(&v, fname)?,
            "quotestrings" => {
                w.quote_all = Some(if v.is_text() {
                    match v.to_str().unwrap_or_default().to_lowercase().as_str() {
                        "all" | "true" => true,
                        "minimal" | "none" | "false" => false,
                        q => {
                            return Err(bad(
                                fname,
                                "invalidQuoteStrings",
                                format!(
                                    "{}: QuoteStrings '{}' must be 'minimal', 'all' or 'none'.",
                                    fname, q
                                ),
                            ));
                        }
                    }
                } else {
                    bool_of(&v, fname, "QuoteStrings")?
                })
            }
            "filetype" => ftype = Some(text_of(&v, fname, "FileType")?),
            "encoding" => {
                let e = text_of(&v, fname, "Encoding")?.to_lowercase();
                if !matches!(e.as_str(), "utf-8" | "utf8" | "system" | "us-ascii") {
                    return Err(bad(
                        fname,
                        "unsupportedEncoding",
                        format!(
                            "{}: only UTF-8 output is supported (Encoding '{}').",
                            fname, e
                        ),
                    ));
                }
            }
            "sheet" | "range" | "usexcel" | "useexcel" | "autofitwidth" | "preservedformat" => {
                return Err(bad(
                    fname,
                    "unsupportedOption",
                    format!("{}: option '{}' is not supported yet.", fname, n),
                ));
            }
            _ => return Err(unknown(fname, &n)),
        }
    }
    if let Some(t) = ftype {
        check_file_type(fname, "", Some(&t))?;
    }
    Ok(w)
}

/// Text cells of one value laid out as `h` rows: (columns of texts,
/// whether each column is text).
fn value_columns(it: &mut Interp, v: &Value, h: usize, fname: &str) -> R<Vec<(Vec<String>, bool)>> {
    if let Value::Sparse(sp) = v {
        return value_columns(it, &sp.to_dense()?, h, fname);
    }
    let d = v.dims();
    let w: usize = if d.is_empty() {
        0
    } else {
        d[1..].iter().product()
    };
    let unsupported = |what: &str| {
        bad(
            fname,
            "unsupportedType",
            format!("{}: variables of type {} are not supported.", fname, what),
        )
    };
    Ok(match v {
        Value::Num(n) => (0..w)
            .map(|j| ((0..h).map(|i| fmt_elem(n, i + j * h)).collect(), false))
            .collect(),
        Value::Bool(b) => (0..w)
            .map(|j| {
                (
                    (0..h)
                        .map(|i| if b.data[i + j * h] { "1" } else { "0" }.to_string())
                        .collect(),
                    false,
                )
            })
            .collect(),
        Value::Char(_) => {
            let rows = if h == 0 {
                Vec::new()
            } else {
                v.char_rows().unwrap_or_default()
            };
            vec![(rows, true)]
        }
        Value::Str(s) => (0..w)
            .map(|j| {
                (
                    (0..h)
                        .map(|i| s.data[i + j * h].clone().unwrap_or_default())
                        .collect(),
                    true,
                )
            })
            .collect(),
        Value::Cell(c) => {
            let mut cols = Vec::new();
            for j in 0..w {
                let mut col = Vec::with_capacity(h);
                let mut text = false;
                for i in 0..h {
                    let e = &c.data[i + j * h];
                    let (s, t) = scalar_text(e).ok_or_else(|| {
                        unsupported(&format!("cell containing {}", e.class_name()))
                    })?;
                    text |= t;
                    col.push(s);
                }
                cols.push((col, text));
            }
            cols
        }
        _ if is_native(v) => {
            if v.class_name() == "table" {
                return Err(unsupported("table"));
            }
            let r = it.call_named("cellstr", vec![v.clone()], 1)?;
            let cs = r.into_iter().next().unwrap_or_default();
            return value_columns(it, &cs, h, fname);
        }
        _ => return Err(unsupported(&v.class_name())),
    })
}

/// Text of a scalar cell element (number, logical, text, missing).
fn scalar_text(e: &Value) -> Option<(String, bool)> {
    if let Value::Sparse(sp) = e {
        return sp
            .dense()
            .filter(|d| d.numel() == 1)
            .and_then(|d| scalar_text(&d));
    }
    match e {
        Value::Num(n) if n.re.len() == 1 => Some((fmt_elem(n, 0), false)),
        Value::Num(n) if n.re.is_empty() => Some((String::new(), false)),
        Value::Bool(b) if b.data.len() == 1 => {
            Some((if b.data[0] { "1" } else { "0" }.to_string(), false))
        }
        Value::Char(_) if e.is_string_row() || e.is_empty() => {
            Some((e.to_str().unwrap_or_default(), true))
        }
        Value::Str(s) if s.data.len() == 1 => match &s.data[0] {
            Some(x) => Some((x.clone(), true)),
            None => Some((String::new(), false)),
        },
        _ => None,
    }
}

fn writetable(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let fname = "writetable";
    need(&a, 1, fname)?;
    let t = tab_of(&a[0]).ok_or_else(|| {
        bad(
            fname,
            "notATable",
            "writetable: the first input must be a table.",
        )
    })?;
    let (name, opts) = match a.get(1) {
        Some(v) if v.is_text() && !is_option_name(v, WRITETABLE_OPTS) => {
            (file_name(v, fname)?, &a[2..])
        }
        _ => ("table.txt".to_string(), &a[1..]),
    };
    if xlsx::wants_write(&name, opts) {
        return xlsx::writetable(it, &t, &name, opts);
    }
    check_file_type(fname, &name, None)?;
    let w = write_spec(opts, fname, true)?;
    if ext_of(&name) == ".txt"
        && !opts.iter().any(|v| {
            v.to_str()
                .is_some_and(|s| s.eq_ignore_ascii_case("delimiter"))
        })
    {
        // MATLAB writes comma-delimited text for .txt as well.
    }
    let h = t.h;
    let mut header: Vec<String> = Vec::new();
    let mut cols: Vec<(Vec<String>, bool)> = Vec::new();
    if w.row_names && !t.rows.is_empty() {
        header.push(t.dimnames[0].clone());
        cols.push((t.rows.clone(), true));
    }
    for (k, v) in t.vars.iter().enumerate() {
        let vc = value_columns(it, v, h, fname)?;
        let nm = &t.names[k];
        if vc.len() == 1 {
            header.push(nm.clone());
        } else {
            for j in 0..vc.len() {
                header.push(format!("{}_{}", nm, j + 1));
            }
        }
        cols.extend(vc);
    }
    let force = w.quote_all.unwrap_or(false);
    let mut out = String::new();
    let skip_header = w.append && has_content(it, &name);
    if w.names && !skip_header && !header.is_empty() {
        let line: Vec<String> = header.iter().map(|s| quote(s, &w.delim, false)).collect();
        out.push_str(&line.join(&w.delim));
        out.push('\n');
    }
    for i in 0..h {
        let line: Vec<String> = cols
            .iter()
            .map(|(c, text)| {
                let s = c.get(i).map(|s| s.as_str()).unwrap_or("");
                if *text {
                    quote(s, &w.delim, force)
                } else {
                    s.to_string()
                }
            })
            .collect();
        out.push_str(&line.join(&w.delim));
        out.push('\n');
    }
    write_file(it, fname, &name, &out, w.append)?;
    Ok(vec![])
}

const WRITETABLE_OPTS: &[&str] = &[
    "Delimiter",
    "WriteVariableNames",
    "WriteRowNames",
    "WriteMode",
    "QuoteStrings",
    "FileType",
    "Encoding",
];

fn is_option_name(v: &Value, names: &[&str]) -> bool {
    v.to_str()
        .is_some_and(|s| names.iter().any(|n| n.eq_ignore_ascii_case(&s)))
}

// ---------------------------------------------------------------------------
// readmatrix / writematrix / readcell / writecell
// ---------------------------------------------------------------------------

struct CellSpec {
    delims: Option<Delims>,
    header: Option<usize>,
    multi: bool,
    out_type: Option<String>,
    text_string: bool,
    treat_missing: Vec<String>,
    sheet: Option<Value>,
    range: Option<Value>,
    is_sheet: bool,
}

fn cell_spec(a: &[Value], fname: &str, name: &str) -> R<CellSpec> {
    let mut s = CellSpec {
        delims: None,
        header: None,
        multi: false,
        out_type: None,
        text_string: false,
        treat_missing: Vec::new(),
        sheet: None,
        range: None,
        is_sheet: false,
    };
    let mut ftype = None;
    for (n, v) in name_value(a, fname)? {
        match n.as_str() {
            "delimiter" => {
                s.delims = Some(delims_value(&v, fname)?);
            }
            "numheaderlines" | "headerlines" => {
                s.header = Some(count_of(&v, fname, "NumHeaderLines")?)
            }
            "filetype" => ftype = Some(text_of(&v, fname, "FileType")?),
            "consecutivedelimitersrule" => {
                s.multi =
                    text_of(&v, fname, "ConsecutiveDelimitersRule")?.eq_ignore_ascii_case("join")
            }
            "multipledelimsasone" => s.multi = bool_of(&v, fname, "MultipleDelimsAsOne")?,
            "outputtype" if fname == "readmatrix" => {
                let t = text_of(&v, fname, "OutputType")?.to_lowercase();
                if t != "string" && t != "char" && NumClass::from_name(&t).is_none() {
                    return Err(bad(
                        fname,
                        "invalidOutputType",
                        format!("readmatrix: unsupported OutputType '{}'.", t),
                    ));
                }
                s.out_type = Some(t);
            }
            "texttype" if fname == "readcell" => {
                s.text_string = text_of(&v, fname, "TextType")?.eq_ignore_ascii_case("string")
            }
            "treatasmissing" => s.treat_missing = text_list(&v).unwrap_or_default(),
            "encoding" | "emptylinerule" | "whitespace" | "lineending" => {}
            "sheet" => s.sheet = Some(v.clone()),
            "range" => s.range = Some(v.clone()),
            "useexcel" => use_excel_off(&v, fname)?,
            _ => return Err(unknown(fname, &n)),
        }
    }
    s.is_sheet = xlsx::is_target(name, ftype.as_deref());
    if s.is_sheet {
        reject_text_options(a, fname)?;
    } else {
        text_file_only(s.sheet.as_ref(), s.range.as_ref(), fname)?;
        check_file_type(fname, name, ftype.as_deref())?;
    }
    Ok(s)
}

fn records_for(text: &str, s: &CellSpec) -> Vec<Record> {
    let skip = s.header.unwrap_or(0);
    let d = s
        .delims
        .clone()
        .unwrap_or_else(|| detect_delims(text, skip));
    let multi = s.multi || d.white();
    split_records(text, &d, multi)
        .into_iter()
        .filter(|r| r.line > skip)
        .collect()
}

fn readmatrix(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let fname = "readmatrix";
    need(&a, 1, fname)?;
    let name = file_name(&a[0], fname)?;
    let s = cell_spec(&a[1..], fname, &name)?;
    let mut recs = cell_records(it, fname, &name, &s)?;
    if s.header.is_none() {
        // Leading all-text rows are header lines when numeric rows follow.
        let numeric = |r: &Record| r.fields.iter().any(|f| parse_number(&f.s).is_some());
        if let Some(first_num) = recs.iter().position(numeric) {
            recs.drain(..first_num);
        }
    }
    let h = recs.len();
    let w = recs.iter().map(|r| r.fields.len()).max().unwrap_or(0);
    if h == 0 || w == 0 {
        return Ok(vec![Value::empty()]);
    }
    if let Some(t) = &s.out_type
        && (t == "string" || t == "char")
    {
        let mut data = vec![None; h * w];
        for (i, r) in recs.iter().enumerate() {
            for (j, f) in r.fields.iter().enumerate() {
                data[i + j * h] = Some(f.s.clone());
            }
        }
        if t == "char" {
            let cells = data
                .into_iter()
                .map(|x| Value::str(&x.unwrap_or_default()))
                .collect();
            return Ok(vec![Value::Cell(Array::new(vec![h, w], cells))]);
        }
        return Ok(vec![Value::Str(Array::new(vec![h, w], data))]);
    }
    let mut data = vec![f64::NAN; h * w];
    for (i, r) in recs.iter().enumerate() {
        for (j, f) in r.fields.iter().enumerate() {
            if !s.treat_missing.contains(&f.s) {
                data[i + j * h] = parse_number(&f.s).unwrap_or(f64::NAN);
            }
        }
    }
    let mut n = Num::real(vec![h, w], data);
    if let Some(c) = s.out_type.as_deref().and_then(NumClass::from_name) {
        n = n.with_class(c);
    }
    Ok(vec![Value::Num(n)])
}

fn readcell(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let fname = "readcell";
    need(&a, 1, fname)?;
    let name = file_name(&a[0], fname)?;
    let s = cell_spec(&a[1..], fname, &name)?;
    let recs = cell_records(it, fname, &name, &s)?;
    let h = recs.len();
    let w = recs.iter().map(|r| r.fields.len()).max().unwrap_or(0);
    if h == 0 || w == 0 {
        return Ok(vec![Value::Cell(Array::new(vec![0, 0], vec![]))]);
    }
    let missing = Value::Str(Array::scalar(None));
    let mut data = vec![missing.clone(); h * w];
    for (i, r) in recs.iter().enumerate() {
        for (j, f) in r.fields.iter().enumerate() {
            data[i + j * h] = if f.s.is_empty() || s.treat_missing.contains(&f.s) {
                missing.clone()
            } else if f.sheet == SheetKind::Bool {
                Value::bool(f.s == "1")
            } else if f.sheet == SheetKind::Date {
                let x = parse_number(&f.s).unwrap_or(f64::NAN);
                excel_dates(it, Value::num(x), fname)?
            } else if let Some(x) = parse_number(&f.s).filter(|_| !f.quoted) {
                Value::num(x)
            } else if s.text_string {
                Value::string(&f.s)
            } else {
                Value::str(&f.s)
            };
        }
    }
    Ok(vec![Value::Cell(Array::new(vec![h, w], data))])
}

fn writer_args<'a>(
    a: &'a [Value],
    fname: &str,
    default: &str,
    known: &[&str],
) -> R<(String, &'a [Value])> {
    match a.get(1) {
        Some(v) if v.is_text() && !is_option_name(v, known) => Ok((file_name(v, fname)?, &a[2..])),
        _ => Ok((default.to_string(), &a[1.min(a.len())..])),
    }
}

const WRITE_OPTS: &[&str] = &[
    "Delimiter",
    "WriteMode",
    "QuoteStrings",
    "FileType",
    "Encoding",
];

fn writematrix(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let fname = "writematrix";
    need(&a, 1, fname)?;
    let (name, opts) = writer_args(&a, fname, "matrix.txt", WRITE_OPTS)?;
    if xlsx::wants_write(&name, opts) {
        return xlsx::writematrix(it, &a[0], &name, opts);
    }
    check_file_type(fname, &name, None)?;
    let w = write_spec(opts, fname, false)?;
    let m = &a[0];
    let d = m.dims();
    if d.len() > 2 {
        return Err(bad(
            fname,
            "invalidInput",
            "writematrix: the input must be a two-dimensional array.",
        ));
    }
    if matches!(m, Value::Cell(_)) {
        return Err(bad(
            fname,
            "invalidInput",
            "writematrix: cell arrays are not supported; use writecell.",
        ));
    }
    let (r, c) = (d[0], d[1]);
    let cols: Vec<(Vec<String>, bool)> = match m {
        Value::Char(_) => {
            let rows = if r == 0 {
                Vec::new()
            } else {
                m.char_rows().unwrap_or_default()
            };
            vec![(rows, true)]
        }
        Value::Num(_) | Value::Bool(_) | Value::Str(_) => value_columns(it, m, r, fname)?,
        _ => {
            return Err(bad(
                fname,
                "invalidInput",
                format!(
                    "writematrix: unsupported input of class {}.",
                    m.class_name()
                ),
            ));
        }
    };
    let _ = c;
    let force = w.quote_all.unwrap_or(false);
    let nrows = cols.first().map_or(0, |c| c.0.len());
    let mut out = String::new();
    for i in 0..nrows {
        let line: Vec<String> = cols
            .iter()
            .map(|(c, text)| {
                if *text {
                    quote(&c[i], &w.delim, force)
                } else {
                    c[i].clone()
                }
            })
            .collect();
        out.push_str(&line.join(&w.delim));
        out.push('\n');
    }
    write_file(it, fname, &name, &out, w.append)?;
    Ok(vec![])
}

fn writecell(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let fname = "writecell";
    need(&a, 1, fname)?;
    let Value::Cell(c) = &a[0] else {
        return Err(bad(
            fname,
            "invalidInput",
            "writecell: the input must be a cell array.",
        ));
    };
    if c.dims.len() > 2 {
        return Err(bad(
            fname,
            "invalidInput",
            "writecell: the input must be a two-dimensional cell array.",
        ));
    }
    let (name, opts) = writer_args(&a, fname, "cell.txt", WRITE_OPTS)?;
    if xlsx::wants_write(&name, opts) {
        return xlsx::writecell(it, &a[0], &name, opts);
    }
    check_file_type(fname, &name, None)?;
    let w = write_spec(opts, fname, false)?;
    let force = w.quote_all.unwrap_or(false);
    let (r, k) = (c.dims[0], c.dims.get(1).copied().unwrap_or(1));
    let mut out = String::new();
    for i in 0..r {
        let mut line = Vec::with_capacity(k);
        for j in 0..k {
            let e = &c.data[i + j * r];
            let (s, text) = scalar_text(e).ok_or_else(|| {
                bad(
                    fname,
                    "unsupportedType",
                    format!(
                        "writecell: cells must contain scalar numbers, logicals, text or missing values (found {}).",
                        e.class_name()
                    ),
                )
            })?;
            line.push(if text { quote(&s, &w.delim, force) } else { s });
        }
        out.push_str(&line.join(&w.delim));
        out.push('\n');
    }
    write_file(it, fname, &name, &out, w.append)?;
    Ok(vec![])
}

// ---------------------------------------------------------------------------
// Spreadsheets: cells as records, so the text readers' logic applies to them
// ---------------------------------------------------------------------------

/// Sheet cells as records. Text is marked quoted so it stays text, and each
/// field remembers its cell type for `sheet_types` and `readcell`.
fn sheet_records(
    it: &Interp,
    fname: &str,
    name: &str,
    sheet: Option<&Value>,
    range: Option<&Value>,
) -> R<Vec<Record>> {
    let rows = xlsx::read_grid(it, fname, name, sheet, range)?;
    Ok(rows
        .into_iter()
        .map(|(line, cells)| Record {
            line,
            fields: cells
                .into_iter()
                .map(|c| match c {
                    None => Field::default(),
                    Some(Cell::Text(s)) => Field {
                        s,
                        quoted: true,
                        sheet: SheetKind::Text,
                    },
                    Some(Cell::Num(x)) => Field {
                        s: xlsx::num_text(x),
                        quoted: false,
                        sheet: SheetKind::Num,
                    },
                    Some(Cell::Bool(b)) => Field {
                        s: if b { "1" } else { "0" }.to_string(),
                        quoted: false,
                        sheet: SheetKind::Bool,
                    },
                    Some(Cell::Date(x)) => Field {
                        s: xlsx::num_text(x),
                        quoted: false,
                        sheet: SheetKind::Date,
                    },
                })
                .collect(),
        })
        .collect())
}

/// Column types from the cell types: text that reads as a number (such as a
/// written `Inf`) does not make a column textual.
fn sheet_types(data: &[Record], ncols: usize, spec: &ReadSpec) -> Vec<ColType> {
    (0..ncols)
        .map(|k| {
            let fields = || {
                data.iter()
                    .filter_map(move |r| r.fields.get(k))
                    .filter(|f| !f.s.is_empty())
            };
            let all =
                |kind: SheetKind| fields().next().is_some() && fields().all(|f| f.sheet == kind);
            if all(SheetKind::Bool) {
                ColType::Logical
            } else if all(SheetKind::Date) {
                ColType::ExcelDate
            } else if fields().all(|f| is_numeric_field(f, &spec.treat_missing)) {
                ColType::Double
            } else if spec.text_string {
                ColType::Str
            } else {
                ColType::Char
            }
        })
        .collect()
}

fn excel_dates(it: &mut Interp, serial: Value, fname: &str) -> R<Value> {
    let args = vec![serial, Value::str("ConvertFrom"), Value::str("excel")];
    let r = it.call_named("datetime", args, 1)?;
    r.into_iter().next().ok_or_else(|| {
        bad(
            fname,
            "conversionFailed",
            format!("{}: could not convert a column to datetime.", fname),
        )
    })
}

fn cell_records(it: &Interp, fname: &str, name: &str, s: &CellSpec) -> R<Vec<Record>> {
    if s.is_sheet {
        let mut recs = sheet_records(it, fname, name, s.sheet.as_ref(), s.range.as_ref())?;
        let skip = s.header.unwrap_or(0);
        recs.retain(|r| r.line > skip);
        return Ok(recs);
    }
    let text = read_file(it, fname, name)?;
    Ok(records_for(&text, s))
}

/// Options that only make sense for text files are errors on a spreadsheet.
fn reject_text_options(a: &[Value], fname: &str) -> R<()> {
    const TEXT_ONLY: &[&str] = &[
        "delimiter",
        "multipledelimsasone",
        "consecutivedelimitersrule",
        "datalines",
        "datarange",
        "whitespace",
        "endofline",
        "lineending",
        "encoding",
    ];
    for p in a.chunks(2) {
        if let Some(n) = p[0].to_str().filter(|_| p[0].is_text())
            && TEXT_ONLY.contains(&n.to_lowercase().as_str())
        {
            return Err(bad(
                fname,
                "unsupportedOption",
                format!(
                    "{}: option '{}' does not apply to spreadsheet files; use 'Sheet' and 'Range'.",
                    fname, n
                ),
            ));
        }
    }
    Ok(())
}

/// `Sheet` and `Range` on a file that is not a workbook.
fn text_file_only(sheet: Option<&Value>, range: Option<&Value>, fname: &str) -> R<()> {
    let name = match (sheet, range) {
        (Some(_), _) => "sheet",
        (_, Some(_)) => "range",
        _ => return Ok(()),
    };
    Err(bad(
        fname,
        "unsupportedOption",
        format!("{}: option '{}' is not supported yet.", fname, name),
    ))
}

fn use_excel_off(v: &Value, fname: &str) -> R<()> {
    if bool_of(v, fname, "UseExcel")? {
        return Err(bad(
            fname,
            "unsupportedOption",
            format!(
                "{}: 'UseExcel' true is not available in the Rust engine.",
                fname
            ),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// readlines / writelines
// ---------------------------------------------------------------------------

fn readlines(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let fname = "readlines";
    need(&a, 1, fname)?;
    let name = file_name(&a[0], fname)?;
    let mut skip_empty = false;
    let mut error_empty = false;
    let mut ws = "preserve".to_string();
    let mut ending: Option<Vec<String>> = None;
    for (n, v) in name_value(&a[1..], fname)? {
        match n.as_str() {
            "emptylinerule" => match text_of(&v, fname, "EmptyLineRule")?.to_lowercase().as_str() {
                "read" => {}
                "skip" => skip_empty = true,
                "error" => error_empty = true,
                r => {
                    return Err(bad(
                        fname,
                        "invalidEmptyLineRule",
                        format!(
                            "readlines: EmptyLineRule '{}' must be 'read', 'skip' or 'error'.",
                            r
                        ),
                    ));
                }
            },
            "whitespacerule" => {
                ws = text_of(&v, fname, "WhitespaceRule")?.to_lowercase();
                if !matches!(
                    ws.as_str(),
                    "preserve" | "trim" | "trimleading" | "trimtrailing"
                ) {
                    return Err(bad(
                        fname,
                        "invalidWhitespaceRule",
                        "readlines: WhitespaceRule must be 'preserve', 'trim', 'trimleading' or 'trimtrailing'.",
                    ));
                }
            }
            "lineending" => {
                ending = Some(
                    text_list(&v)
                        .unwrap_or_default()
                        .iter()
                        .map(|s| format::escapes(s))
                        .filter(|s| !s.is_empty())
                        .collect(),
                )
            }
            "encoding" => {}
            _ => return Err(unknown(fname, &n)),
        }
    }
    check_file_type(fname, &name, None)?;
    let text = read_file(it, fname, &name)?;
    let mut lines: Vec<String> = match &ending {
        Some(e) if !e.is_empty() => split_by(&text, e),
        _ => {
            let t = text.replace("\r\n", "\n").replace('\r', "\n");
            t.split('\n').map(|s| s.to_string()).collect()
        }
    };
    for l in lines.iter_mut() {
        *l = match ws.as_str() {
            "trim" => l.trim().to_string(),
            "trimleading" => l.trim_start().to_string(),
            "trimtrailing" => l.trim_end().to_string(),
            _ => std::mem::take(l),
        };
    }
    if error_empty && lines.iter().any(|l| l.is_empty()) && !(lines.len() == 1 && text.is_empty()) {
        let last_ok = lines.iter().rev().skip(1).all(|l| !l.is_empty())
            && lines.last().is_some_and(|l| l.is_empty());
        if !last_ok {
            return Err(bad(
                fname,
                "EmptyLine",
                "readlines: the file contains an empty line.",
            ));
        }
    }
    if skip_empty {
        lines.retain(|l| !l.is_empty());
    }
    let n = lines.len();
    Ok(vec![Value::Str(Array::new(
        vec![n, 1],
        lines.into_iter().map(Some).collect(),
    ))])
}

fn split_by(text: &str, ends: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut rest = text;
    'outer: while !rest.is_empty() {
        for e in ends {
            if let Some(r) = rest.strip_prefix(e.as_str()) {
                out.push(std::mem::take(&mut cur));
                rest = r;
                continue 'outer;
            }
        }
        let mut chars = rest.chars();
        if let Some(ch) = chars.next() {
            cur.push(ch);
        }
        rest = chars.as_str();
    }
    out.push(cur);
    out
}

fn writelines(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let fname = "writelines";
    need(&a, 2, fname)?;
    let data = &a[0];
    let lines: Vec<String> = match data {
        Value::Char(c) => {
            let d = &c.dims;
            if d.len() > 2 || (d[0] > 1 && !(d[0] == 0 || d[1] == 0)) {
                return Err(bad(
                    fname,
                    "invalidData",
                    "writelines: character data must be a character vector (not a matrix).",
                ));
            }
            vec![data.to_str().unwrap_or_default()]
        }
        Value::Str(s) => s
            .data
            .iter()
            .map(|x| x.clone().unwrap_or_default())
            .collect(),
        Value::Cell(c) => c
            .data
            .iter()
            .map(|x| match x {
                Value::Char(_) if x.is_string_row() || x.is_empty() => {
                    Some(x.to_str().unwrap_or_default())
                }
                Value::Str(s) if s.data.len() == 1 => Some(s.data[0].clone().unwrap_or_default()),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                bad(
                    fname,
                    "invalidData",
                    "writelines: cell data must contain character vectors.",
                )
            })?,
        _ => {
            return Err(bad(
                fname,
                "invalidData",
                "writelines: data must be a character vector, cell array of character vectors or string array.",
            ));
        }
    };
    let name = file_name(&a[1], fname)?;
    check_file_type(fname, &name, None)?;
    let mut append = false;
    let mut ending = "\n".to_string();
    for (n, v) in name_value(&a[2..], fname)? {
        match n.as_str() {
            "writemode" => append = write_mode(&v, fname)?,
            "lineending" => ending = format::escapes(&text_of(&v, fname, "LineEnding")?),
            "encoding" => {}
            _ => return Err(unknown(fname, &n)),
        }
    }
    let mut out = String::new();
    for l in lines {
        out.push_str(&l);
        out.push_str(&ending);
    }
    write_file(it, fname, &name, &out, append)?;
    Ok(vec![])
}

// ---------------------------------------------------------------------------
// textscan
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Kind {
    F64,
    F32,
    Int(NumClass),
    Str,
    Quoted,
    Chars,
    Set(Vec<char>, bool),
    Literal(Vec<char>),
}

#[derive(Clone, Debug)]
struct Item {
    kind: Kind,
    skip: bool,
    width: Option<usize>,
}

fn ts_err(msg: impl Into<String>) -> Box<indy_value::Error> {
    err("MATLAB:textscan:badFormat", msg)
}

fn parse_format(f: &str) -> R<Vec<Item>> {
    let c: Vec<char> = f.chars().collect();
    let n = c.len();
    let mut out = Vec::new();
    let mut i = 0;
    let mut lit = String::new();
    let flush = |lit: &mut String, out: &mut Vec<Item>| {
        if !lit.is_empty() {
            let s = format::escapes(lit);
            for part in s.split(char::is_whitespace).filter(|p| !p.is_empty()) {
                out.push(Item {
                    kind: Kind::Literal(part.chars().collect()),
                    skip: true,
                    width: None,
                });
            }
            lit.clear();
        }
    };
    while i < n {
        if c[i] != '%' {
            lit.push(c[i]);
            i += 1;
            continue;
        }
        if i + 1 < n && c[i + 1] == '%' {
            lit.push('%');
            i += 2;
            continue;
        }
        flush(&mut lit, &mut out);
        i += 1;
        let skip = i < n && c[i] == '*';
        if skip {
            i += 1;
        }
        let mut wd = String::new();
        while i < n && c[i].is_ascii_digit() {
            wd.push(c[i]);
            i += 1;
        }
        if i < n && c[i] == '.' {
            i += 1;
            while i < n && c[i].is_ascii_digit() {
                i += 1;
            }
        }
        let width = wd.parse::<usize>().ok().filter(|w| *w > 0);
        if i >= n {
            return Err(ts_err(
                "textscan: incomplete conversion specifier in the format.",
            ));
        }
        let conv = c[i];
        i += 1;
        let bits = |i: &mut usize| -> Option<u32> {
            for b in ["64", "32", "16", "8"] {
                let bc: Vec<char> = b.chars().collect();
                if c[*i..].starts_with(&bc) {
                    *i += bc.len();
                    return b.parse().ok();
                }
            }
            None
        };
        let kind = match conv {
            'f' => match bits(&mut i) {
                Some(32) => Kind::F32,
                Some(64) | None => Kind::F64,
                Some(b) => return Err(ts_err(format!("textscan: invalid format %f{}.", b))),
            },
            'n' => Kind::F64,
            'd' => Kind::Int(match bits(&mut i) {
                Some(8) => NumClass::Int8,
                Some(16) => NumClass::Int16,
                Some(64) => NumClass::Int64,
                _ => NumClass::Int32,
            }),
            'u' => Kind::Int(match bits(&mut i) {
                Some(8) => NumClass::UInt8,
                Some(16) => NumClass::UInt16,
                Some(64) => NumClass::UInt64,
                _ => NumClass::UInt32,
            }),
            's' => Kind::Str,
            'q' => Kind::Quoted,
            'c' => Kind::Chars,
            '[' => {
                let neg = i < n && c[i] == '^';
                if neg {
                    i += 1;
                }
                let mut set = Vec::new();
                if i < n && c[i] == ']' {
                    set.push(']');
                    i += 1;
                }
                while i < n && c[i] != ']' {
                    set.push(c[i]);
                    i += 1;
                }
                if i >= n {
                    return Err(ts_err("textscan: unterminated %[ in the format."));
                }
                i += 1;
                Kind::Set(set, neg)
            }
            'D' | 'T' => {
                return Err(ts_err(format!(
                    "textscan: %{} (datetime/duration) conversions are not supported yet.",
                    conv
                )));
            }
            _ => {
                return Err(ts_err(format!(
                    "textscan: unsupported conversion specifier '%{}' in the format.",
                    conv
                )));
            }
        };
        out.push(Item { kind, skip, width });
    }
    flush(&mut lit, &mut out);
    Ok(out)
}

struct Scanner {
    c: Vec<char>,
    i: usize,
    delims: Vec<Vec<char>>,
    white: Vec<char>,
    eols: Option<Vec<Vec<char>>>,
    multi: bool,
    comment: Option<(Vec<char>, Option<Vec<char>>)>,
    treat_empty: Vec<String>,
}

impl Scanner {
    fn at_end(&self) -> bool {
        self.i >= self.c.len()
    }
    fn eol_len(&self, i: usize) -> usize {
        if i >= self.c.len() {
            return 0;
        }
        match &self.eols {
            None => {
                if self.c[i] == '\r' && i + 1 < self.c.len() && self.c[i + 1] == '\n' {
                    2
                } else {
                    usize::from(is_eol(self.c[i]))
                }
            }
            Some(es) => es
                .iter()
                .find(|e| !e.is_empty() && self.c[i..].starts_with(e))
                .map_or(0, |e| e.len()),
        }
    }
    fn delim_len(&self, i: usize) -> usize {
        if i >= self.c.len() {
            return 0;
        }
        self.delims
            .iter()
            .find(|d| self.c[i..].starts_with(d))
            .map_or(0, |d| d.len())
    }
    fn is_ws(&self, ch: char) -> bool {
        self.white.contains(&ch) && !self.delims.iter().any(|d| d.len() == 1 && d[0] == ch)
    }
    fn comment_len(&self, i: usize) -> usize {
        let Some((start, end)) = &self.comment else {
            return 0;
        };
        if start.is_empty() || !self.c[i..].starts_with(start) {
            return 0;
        }
        let mut j = i + start.len();
        match end {
            Some(e) => {
                while j < self.c.len() && !self.c[j..].starts_with(e) {
                    j += 1;
                }
                (j + e.len()).min(self.c.len()) - i
            }
            None => {
                while j < self.c.len() && self.eol_len(j) == 0 {
                    j += 1;
                }
                j - i
            }
        }
    }
    fn skip_ws(&mut self) {
        while self.i < self.c.len() {
            if self.is_ws(self.c[self.i]) {
                self.i += 1;
            } else {
                let k = self.comment_len(self.i);
                if k == 0 {
                    break;
                }
                self.i += k;
            }
        }
    }
    fn skip_ws_eol(&mut self) {
        loop {
            self.skip_ws();
            let k = self.eol_len(self.i);
            if k == 0 {
                break;
            }
            self.i += k;
        }
    }
    /// End of a token: whitespace, delimiter, EOL or EOF.
    fn stop_at(&self, i: usize) -> bool {
        i >= self.c.len()
            || (self.delims.is_empty() && (self.is_ws(self.c[i]) || self.c[i] == ' '))
            || self.delim_len(i) > 0
            || self.eol_len(i) > 0
    }
    fn at_empty_field(&self) -> bool {
        !self.delims.is_empty()
            && (self.delim_len(self.i) > 0 || self.eol_len(self.i) > 0 || self.at_end())
    }
    fn after_field(&mut self) {
        self.skip_ws();
        let k = self.delim_len(self.i);
        if k > 0 {
            self.i += k;
            if self.multi {
                loop {
                    let save = self.i;
                    self.skip_ws();
                    let k = self.delim_len(self.i);
                    if k == 0 {
                        self.i = save;
                        break;
                    }
                    self.i += k;
                }
            }
        }
    }
    fn number(&mut self, width: Option<usize>) -> Option<f64> {
        let lim = width.map_or(self.c.len(), |w| (self.i + w).min(self.c.len()));
        let c = &self.c;
        let mut j = self.i;
        if j < lim && (c[j] == '+' || c[j] == '-') {
            j += 1;
        }
        let word = |j: usize, w: &str| -> bool {
            let wc: Vec<char> = w.chars().collect();
            j + wc.len() <= lim
                && c[j..j + wc.len()]
                    .iter()
                    .zip(&wc)
                    .all(|(a, b)| a.eq_ignore_ascii_case(b))
        };
        let end = if word(j, "inf") || word(j, "nan") {
            j + 3
        } else {
            let mut digits = 0;
            while j < lim && c[j].is_ascii_digit() {
                j += 1;
                digits += 1;
            }
            if j < lim && c[j] == '.' {
                j += 1;
                while j < lim && c[j].is_ascii_digit() {
                    j += 1;
                    digits += 1;
                }
            }
            if digits == 0 {
                return None;
            }
            if j < lim && matches!(c[j], 'e' | 'E' | 'd' | 'D') {
                let mut k = j + 1;
                if k < lim && (c[k] == '+' || c[k] == '-') {
                    k += 1;
                }
                let st = k;
                while k < lim && c[k].is_ascii_digit() {
                    k += 1;
                }
                if k > st {
                    j = k;
                }
            }
            j
        };
        let s: String = c[self.i..end].iter().collect();
        let v = parse_number(&s)?;
        self.i = end;
        Some(v)
    }
    fn token(&mut self, width: Option<usize>) -> String {
        let lim = width.map_or(usize::MAX, |w| self.i.saturating_add(w));
        let st = self.i;
        while self.i < lim && !self.stop_at(self.i) {
            self.i += 1;
        }
        let s: String = self.c[st..self.i].iter().collect();
        if self.delims.is_empty() {
            s
        } else {
            let white = &self.white;
            s.trim_end_matches(|ch: char| white.contains(&ch))
                .to_string()
        }
    }
    fn treat_empty_here(&mut self) -> bool {
        for t in &self.treat_empty {
            let tc: Vec<char> = t.chars().collect();
            if !tc.is_empty()
                && self.c[self.i..].starts_with(&tc)
                && self.stop_at(self.i + tc.len())
            {
                self.i += tc.len();
                return true;
            }
        }
        false
    }
}

enum Col {
    Num(Vec<f64>, NumClass),
    Text(Vec<String>),
    Chars(Vec<String>, usize),
}

struct TsOpts {
    n: Option<usize>,
    header: usize,
    collect: bool,
    return_on_error: bool,
    empty_value: f64,
    text_string: bool,
}

fn textscan(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "textscan")?;
    // Source
    let mut fid: Option<i32> = None;
    let (chars, offsets): (Vec<char>, Option<Vec<usize>>) = match &a[0] {
        v if v.is_text() => (v.to_str().unwrap_or_default().chars().collect(), None),
        Value::Num(_) => {
            let id =
                v_f64(&a[0]).ok_or_else(|| ts_err("textscan: invalid file identifier."))? as i32;
            let f = it.files_open.get_mut(&id).ok_or_else(|| {
                err(
                    "MATLAB:textscan:invalidFileID",
                    "textscan: Invalid file identifier. Use fopen to generate a valid file identifier.",
                )
            })?;
            if !f.readable() {
                return Err(err(
                    "MATLAB:textscan:writeOnly",
                    "textscan: the file is not open for reading.",
                ));
            }
            fid = Some(id);
            // Decoded in the fid's encoding. As with fgetl and fread, a BOM is
            // not special here: only the path-based readers drop it.
            let enc = f.enc;
            let (cs, offs) = enc.decode_offsets(&f.rest());
            (cs, Some(offs))
        }
        v => {
            return Err(ts_err(format!(
                "textscan: the first input must be text or a file identifier (got {}).",
                v.class_name()
            )));
        }
    };
    let mut k = 1;
    let mut format = String::new();
    if let Some(v) = a.get(1)
        && v.is_text()
        && !is_option_name(v, TS_OPTS)
    {
        format = v.to_str().unwrap_or_default();
        k = 2;
    }
    let mut opts = TsOpts {
        n: None,
        header: 0,
        collect: false,
        return_on_error: true,
        empty_value: f64::NAN,
        text_string: false,
    };
    if let Some(v) = a.get(k)
        && matches!(v, Value::Num(_))
    {
        let x = v_f64(v).ok_or_else(|| ts_err("textscan: the repeat count must be a scalar."))?;
        if x.is_finite() && x >= 0.0 {
            opts.n = Some(x as usize);
        } else if !(x.is_infinite() && x > 0.0) && x != -1.0 {
            return Err(ts_err(
                "textscan: the repeat count must be a nonnegative integer, -1 or Inf.",
            ));
        }
        k += 1;
    }
    let mut sc = Scanner {
        c: chars,
        i: 0,
        delims: Vec::new(),
        white: vec![' ', '\x08', '\t'],
        eols: None,
        multi: false,
        comment: None,
        treat_empty: Vec::new(),
    };
    for (n, v) in name_value(&a[k..], "textscan")? {
        match n.as_str() {
            "delimiter" => sc.delims = delim_list(&v, "textscan")?,
            "whitespace" => {
                sc.white = format::escapes(&text_of(&v, "textscan", "Whitespace")?)
                    .chars()
                    .collect()
            }
            "endofline" => {
                let e = format::escapes(&text_of(&v, "textscan", "EndOfLine")?);
                sc.eols = Some(vec![e.chars().collect()]);
            }
            "headerlines" => opts.header = count_of(&v, "textscan", "HeaderLines")?,
            "multipledelimsasone" => sc.multi = bool_of(&v, "textscan", "MultipleDelimsAsOne")?,
            "collectoutput" => opts.collect = bool_of(&v, "textscan", "CollectOutput")?,
            "returnonerror" => opts.return_on_error = bool_of(&v, "textscan", "ReturnOnError")?,
            "emptyvalue" => {
                opts.empty_value = v_f64(&v)
                    .ok_or_else(|| ts_err("textscan: EmptyValue must be a numeric scalar."))?
            }
            "treatasempty" => sc.treat_empty = text_list(&v).unwrap_or_default(),
            "commentstyle" => {
                let l = text_list(&v).unwrap_or_default();
                sc.comment = match l.as_slice() {
                    [s] => Some((s.chars().collect(), None)),
                    [s, e] => Some((s.chars().collect(), Some(e.chars().collect()))),
                    _ => {
                        return Err(ts_err(
                            "textscan: CommentStyle must be text or a pair {start, end}.",
                        ));
                    }
                };
            }
            "texttype" => {
                opts.text_string =
                    text_of(&v, "textscan", "TextType")?.eq_ignore_ascii_case("string")
            }
            "bufsize" | "expchars" => {}
            _ => return Err(unknown("textscan", &n)),
        }
    }
    // Header lines
    for _ in 0..opts.header {
        while !sc.at_end() && sc.eol_len(sc.i) == 0 {
            sc.i += 1;
        }
        sc.i += sc.eol_len(sc.i);
    }
    let mut items = parse_format(&format)?;
    if items.is_empty() {
        items = auto_format(&mut sc);
    }
    let (cols, _) = run_scan(&mut sc, &items, &opts)?;
    let mut outs: Vec<Value> = cols
        .into_iter()
        .map(|c| col_value(c, opts.text_string))
        .collect();
    if opts.collect {
        outs = collect_outputs(outs);
    }
    let pos = match (&fid, &offsets) {
        (Some(id), Some(offs)) => {
            let consumed = offs.get(sc.i).copied().unwrap_or(0);
            match it.files_open.get_mut(id) {
                Some(f) => {
                    f.advance(consumed);
                    f.position() as f64
                }
                None => 0.0,
            }
        }
        _ => sc.i as f64,
    };
    let cell = Value::Cell(Array::new(vec![1, outs.len()], outs));
    let mut r = vec![cell];
    if nargout > 1 {
        r.push(Value::num(pos));
    }
    Ok(r)
}

const TS_OPTS: &[&str] = &[
    "Delimiter",
    "Whitespace",
    "EndOfLine",
    "HeaderLines",
    "MultipleDelimsAsOne",
    "CollectOutput",
    "ReturnOnError",
    "EmptyValue",
    "TreatAsEmpty",
    "CommentStyle",
    "TextType",
];

fn v_f64(v: &Value) -> Option<f64> {
    v.scalar_f64()
}

/// Empty format: one `%f` per field of the first non-empty line.
fn auto_format(sc: &mut Scanner) -> Vec<Item> {
    let save = sc.i;
    sc.skip_ws_eol();
    let mut count = 0;
    while !sc.at_end() && sc.eol_len(sc.i) == 0 {
        if sc.at_empty_field() {
            count += 1;
        } else {
            let st = sc.i;
            sc.token(None);
            if sc.i == st {
                sc.i += 1;
            }
            count += 1;
        }
        let st = sc.i;
        sc.after_field();
        if sc.i == st && !sc.at_end() && sc.eol_len(sc.i) == 0 {
            sc.i += 1;
        }
    }
    sc.i = save;
    (0..count.max(1))
        .map(|_| Item {
            kind: Kind::F64,
            skip: false,
            width: None,
        })
        .collect()
}

fn run_scan(sc: &mut Scanner, items: &[Item], o: &TsOpts) -> R<(Vec<Col>, usize)> {
    let mut cols: Vec<Col> = items
        .iter()
        .filter(|it| !it.skip)
        .map(|it| match &it.kind {
            Kind::F64 => Col::Num(Vec::new(), NumClass::Double),
            Kind::F32 => Col::Num(Vec::new(), NumClass::Single),
            Kind::Int(c) => Col::Num(Vec::new(), *c),
            Kind::Chars => Col::Chars(Vec::new(), it.width.unwrap_or(1)),
            _ => Col::Text(Vec::new()),
        })
        .collect();
    let mut rows = 0;
    'rows: while o.n.is_none_or(|n| rows < n) {
        let row_start = sc.i;
        let mut ci = 0;
        for (k, item) in items.iter().enumerate() {
            let field_start = sc.i;
            if k == 0 || sc.delims.is_empty() {
                sc.skip_ws_eol();
            } else {
                sc.skip_ws();
            }
            if sc.at_end()
                && !(k > 0 && sc.at_empty_field() && sc.i > row_start && last_was_delim(sc))
            {
                break 'rows;
            }
            let ok = match &item.kind {
                Kind::Literal(l) => {
                    if sc.c[sc.i..].starts_with(l) {
                        sc.i += l.len();
                        true
                    } else {
                        false
                    }
                }
                Kind::F64 | Kind::F32 | Kind::Int(_) => {
                    let v = if sc.at_empty_field() || sc.treat_empty_here() {
                        Some(match item.kind {
                            Kind::Int(_) => 0.0,
                            _ => o.empty_value,
                        })
                    } else {
                        sc.number(item.width).map(|x| match item.kind {
                            Kind::Int(_) => x.trunc(),
                            _ => x,
                        })
                    };
                    match v {
                        Some(x) => {
                            if !item.skip
                                && let Col::Num(d, _) = &mut cols[ci]
                            {
                                d.push(x);
                            }
                            true
                        }
                        None => false,
                    }
                }
                Kind::Str | Kind::Quoted => {
                    let s = if matches!(item.kind, Kind::Quoted) && sc.c.get(sc.i) == Some(&'"') {
                        sc.i += 1;
                        let mut s = String::new();
                        while sc.i < sc.c.len() {
                            if sc.c[sc.i] == '"' {
                                if sc.c.get(sc.i + 1) == Some(&'"') {
                                    s.push('"');
                                    sc.i += 2;
                                    continue;
                                }
                                sc.i += 1;
                                break;
                            }
                            s.push(sc.c[sc.i]);
                            sc.i += 1;
                        }
                        s.push_str(&sc.token(None));
                        s
                    } else {
                        sc.token(item.width)
                    };
                    if !item.skip
                        && let Col::Text(d) = &mut cols[ci]
                    {
                        d.push(s);
                    }
                    true
                }
                Kind::Chars => {
                    let w = item.width.unwrap_or(1);
                    if sc.i + w > sc.c.len() {
                        false
                    } else {
                        let s: String = sc.c[sc.i..sc.i + w].iter().collect();
                        sc.i += w;
                        if !item.skip
                            && let Col::Chars(d, _) = &mut cols[ci]
                        {
                            d.push(s);
                        }
                        true
                    }
                }
                Kind::Set(set, neg) => {
                    let lim = item.width.map_or(usize::MAX, |w| sc.i.saturating_add(w));
                    let st = sc.i;
                    while sc.i < sc.c.len() && sc.i < lim && (set.contains(&sc.c[sc.i]) != *neg) {
                        sc.i += 1;
                    }
                    let s: String = sc.c[st..sc.i].iter().collect();
                    if !item.skip
                        && let Col::Text(d) = &mut cols[ci]
                    {
                        d.push(s);
                    }
                    true
                }
            };
            if !ok {
                if o.return_on_error {
                    sc.i = field_start;
                    break 'rows;
                }
                let rest: String = sc.c[sc.i..]
                    .iter()
                    .take_while(|c| !is_eol(**c))
                    .take(60)
                    .collect();
                return Err(err(
                    "MATLAB:textscan:handleErrorAndShowInfo",
                    format!(
                        "textscan: Mismatch between file and format character vector.\nTrouble reading '{}' field from file (row number {}, field number {}) ==> {}",
                        kind_label(&item.kind),
                        rows + 1,
                        k + 1,
                        rest
                    ),
                ));
            }
            if !matches!(item.kind, Kind::Literal(_)) {
                sc.after_field();
            }
            if !item.skip {
                ci += 1;
            }
        }
        rows += 1;
        if sc.i == row_start {
            break;
        }
    }
    Ok((cols, rows))
}

/// Whether the previous character closed a field with a delimiter (so a
/// final empty field exists at EOF).
fn last_was_delim(sc: &Scanner) -> bool {
    sc.delims
        .iter()
        .any(|d| sc.i >= d.len() && sc.c[sc.i - d.len()..sc.i] == d[..])
}

fn kind_label(k: &Kind) -> &'static str {
    match k {
        Kind::F64 | Kind::F32 | Kind::Int(_) => "Numeric",
        Kind::Literal(_) => "Literal",
        _ => "String",
    }
}

fn col_value(c: Col, text_string: bool) -> Value {
    match c {
        Col::Num(d, cls) => {
            let n = Num::real(vec![d.len(), 1], d);
            Value::Num(if cls == NumClass::Double {
                n
            } else {
                n.with_class(cls)
            })
        }
        Col::Text(d) => {
            if text_string {
                let n = d.len();
                Value::Str(Array::new(vec![n, 1], d.into_iter().map(Some).collect()))
            } else {
                cellstr_col(&d)
            }
        }
        Col::Chars(d, w) => {
            let h = d.len();
            let mut data = vec![' '; h * w];
            for (i, s) in d.iter().enumerate() {
                for (j, ch) in s.chars().take(w).enumerate() {
                    data[i + j * h] = ch;
                }
            }
            Value::Char(Array::new(vec![h, w], data))
        }
    }
}

/// CollectOutput: consecutive outputs of the same class are concatenated
/// horizontally (shorter columns padded with NaN, 0 or '').
fn collect_outputs(outs: Vec<Value>) -> Vec<Value> {
    let mut groups: Vec<Vec<Value>> = Vec::new();
    for v in outs {
        match groups.last_mut() {
            Some(g) if g[0].class_name() == v.class_name() => g.push(v),
            _ => groups.push(vec![v]),
        }
    }
    groups
        .into_iter()
        .map(|g| {
            if g.len() == 1 {
                return g.into_iter().next().unwrap_or_default();
            }
            let h = g.iter().map(|v| v.dims()[0]).max().unwrap_or(0);
            let padded: Vec<Value> = g.iter().map(|v| pad_col(v, h)).collect();
            concat::cat(padded.clone(), 1).unwrap_or_else(|_| Value::cell_row(padded))
        })
        .collect()
}

fn pad_col(v: &Value, h: usize) -> Value {
    let d = v.dims();
    if d[0] == h {
        return v.clone();
    }
    match v {
        Value::Num(n) => {
            let fill = if n.class.is_int() { 0.0 } else { f64::NAN };
            let mut re = n.re.clone();
            re.resize(h, fill);
            let m = Num::real(vec![h, 1], re);
            Value::Num(if n.class == NumClass::Double {
                m
            } else {
                m.with_class(n.class)
            })
        }
        Value::Cell(c) => {
            let mut data = c.data.clone();
            data.resize(h, Value::str(""));
            Value::Cell(Array::new(vec![h, 1], data))
        }
        Value::Str(s) => {
            let mut data = s.data.clone();
            data.resize(h, Some(String::new()));
            Value::Str(Array::new(vec![h, 1], data))
        }
        _ => v.clone(),
    }
}
