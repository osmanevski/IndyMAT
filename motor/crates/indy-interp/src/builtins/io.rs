//! Minimal file I/O: fopen/fclose/fgetl/fgets/fscanf/feof/fileread, paths.
//!
//! Every fid carries a text encoding (`fopen`'s 4th argument, UTF-8 by
//! default). Text functions (`fgetl`, `fgets`, `fscanf`, `textscan`, `fprintf`,
//! `fread`/`fwrite` with `char` precision) decode and encode through it.
//! A BOM has no special meaning on an fid: it reads as `char(65279)` and is
//! never added on write. Positions (`ftell`, `fseek`) stay byte-based.

use std::borrow::Cow;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use indy_value::{Array, Num, Value};

use super::cells::cellstr_value;
use super::core::resolve_path;
use super::{arg_f64, arg_str, err, need, one, reg};
use crate::{Interp, R};

/// Text encoding of an open file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextEnc {
    Utf8,
    Latin1,
    Ascii,
    Win1252,
    Latin5,
    Win1254,
}

/// 0x80..=0x9F of windows-1252; unassigned bytes map to the same C1 control,
/// so every byte round-trips.
const WIN1252_C1: [u16; 32] = [
    0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
    0x0152, 0x008D, 0x017D, 0x008F, 0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014,
    0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178,
];

/// The six Turkish letters by which ISO-8859-9 differs from ISO-8859-1.
fn turkish_letter(b: u8) -> Option<char> {
    Some(match b {
        0xD0 => '\u{11E}',
        0xDD => '\u{130}',
        0xDE => '\u{15E}',
        0xF0 => '\u{11F}',
        0xFD => '\u{131}',
        0xFE => '\u{15F}',
        _ => return None,
    })
}

impl TextEnc {
    /// Unknown names are an error: decoding with a guessed encoding would
    /// silently produce garbage.
    pub fn parse(name: &str) -> R<TextEnc> {
        let low = name.trim().to_ascii_lowercase().replace('_', "-");
        Ok(match low.as_str() {
            "" | "utf-8" | "utf8" => TextEnc::Utf8,
            "iso-8859-1" | "iso8859-1" | "latin1" | "latin-1" => TextEnc::Latin1,
            "us-ascii" | "ascii" => TextEnc::Ascii,
            "windows-1252" | "cp1252" => TextEnc::Win1252,
            "iso-8859-9" | "iso8859-9" | "latin5" | "latin-5" => TextEnc::Latin5,
            "windows-1254" | "cp1254" => TextEnc::Win1254,
            _ => {
                return Err(err(
                    "MATLAB:fopen:UnsupportedEncoding",
                    format!(
                        "fopen: unsupported character encoding '{}'. Supported: UTF-8, \
                         ISO-8859-1, US-ASCII, windows-1252, ISO-8859-9, windows-1254.",
                        name
                    ),
                ));
            }
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            TextEnc::Utf8 => "UTF-8",
            TextEnc::Latin1 => "ISO-8859-1",
            TextEnc::Ascii => "US-ASCII",
            TextEnc::Win1252 => "windows-1252",
            TextEnc::Latin5 => "ISO-8859-9",
            TextEnc::Win1254 => "windows-1254",
        }
    }

    fn single(self, b: u8) -> char {
        if b < 0x80 {
            return b as char;
        }
        let c1 = || char::from_u32(WIN1252_C1[usize::from(b) - 0x80] as u32);
        let mapped = match self {
            TextEnc::Utf8 | TextEnc::Ascii => return '\u{FFFD}',
            TextEnc::Latin1 => None,
            TextEnc::Latin5 => turkish_letter(b),
            TextEnc::Win1252 if b < 0xA0 => c1(),
            TextEnc::Win1252 => None,
            // 0x8E and 0x9E are unassigned in windows-1254.
            TextEnc::Win1254 if b == 0x8E || b == 0x9E => None,
            TextEnc::Win1254 if b < 0xA0 => c1(),
            TextEnc::Win1254 => turkish_letter(b),
        };
        mapped.unwrap_or(b as char)
    }

    /// First character of non-empty `b` and the bytes it took. An invalid or
    /// truncated UTF-8 sequence gives U+FFFD and consumes the bad bytes.
    pub fn next_char(self, b: &[u8]) -> (char, usize) {
        if self != TextEnc::Utf8 {
            return (self.single(b[0]), 1);
        }
        let head = &b[..b.len().min(4)];
        let valid = match std::str::from_utf8(head) {
            Ok(s) => s,
            Err(e) if e.valid_up_to() > 0 => {
                std::str::from_utf8(&head[..e.valid_up_to()]).unwrap_or_default()
            }
            Err(e) => return ('\u{FFFD}', e.error_len().unwrap_or(head.len())),
        };
        match valid.chars().next() {
            Some(c) => (c, c.len_utf8()),
            None => ('\u{FFFD}', 1),
        }
    }

    pub fn decode(self, b: &[u8]) -> String {
        if self == TextEnc::Utf8 {
            return String::from_utf8_lossy(b).into_owned();
        }
        b.iter().map(|x| self.single(*x)).collect()
    }

    /// Characters of `b` and the byte offset of each (plus one for the end).
    pub fn decode_offsets(self, b: &[u8]) -> (Vec<char>, Vec<usize>) {
        let mut chars = Vec::with_capacity(b.len());
        let mut offsets = Vec::with_capacity(b.len() + 1);
        let mut at = 0;
        while at < b.len() {
            let (c, n) = self.next_char(&b[at..]);
            chars.push(c);
            offsets.push(at);
            at += n;
        }
        offsets.push(b.len());
        (chars, offsets)
    }

    /// A character the encoding cannot represent becomes 0x1A (SUB), the same
    /// substitute `unicode2native` uses.
    pub fn push_char(self, c: char, out: &mut Vec<u8>) {
        if self == TextEnc::Utf8 {
            out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
        } else if c.is_ascii() {
            out.push(c as u8);
        } else if self == TextEnc::Ascii {
            out.push(0x1A);
        } else {
            out.push((0x80..=0xFF).find(|b| self.single(*b) == c).unwrap_or(0x1A));
        }
    }

    pub fn encode(self, s: &str) -> Cow<'_, [u8]> {
        if self == TextEnc::Utf8 {
            return Cow::Borrowed(s.as_bytes());
        }
        let mut out = Vec::with_capacity(s.len());
        for c in s.chars() {
            self.push_char(c, &mut out);
        }
        Cow::Owned(out)
    }
}

pub struct OpenFile {
    /// The name `fopen` was given; `fopen(fid)` returns it.
    pub name: String,
    pub path: PathBuf,
    pub mode: String,
    pub data: Vec<u8>,
    pub pos: usize,
    pub writer: Option<std::fs::File>,
    pub enc: TextEnc,
    /// Default byte order of `fread`/`fwrite` (`fopen`'s 3rd argument).
    pub big_endian: bool,
}

impl OpenFile {
    pub fn readable(&self) -> bool {
        self.writer.is_none() || self.mode.contains('+')
    }

    /// Unread bytes; the position does not move.
    pub fn rest(&mut self) -> Cow<'_, [u8]> {
        match &mut self.writer {
            Some(w) => {
                let mut buf = Vec::new();
                if let Ok(at) = w.stream_position() {
                    let _ = w.read_to_end(&mut buf);
                    let _ = w.seek(SeekFrom::Start(at));
                }
                Cow::Owned(buf)
            }
            None => Cow::Borrowed(&self.data[self.pos.min(self.data.len())..]),
        }
    }

    pub fn advance(&mut self, n: usize) {
        match &mut self.writer {
            Some(w) => {
                let _ = w.seek(SeekFrom::Current(n as i64));
            }
            None => self.pos = self.pos.saturating_add(n).min(self.data.len()),
        }
    }

    pub fn position(&mut self) -> u64 {
        match &mut self.writer {
            Some(w) => w.stream_position().unwrap_or(0),
            None => self.pos as u64,
        }
    }
}

pub fn register(it: &mut Interp) {
    reg(it, "fopen", fopen);
    reg(it, "fclose", fclose);
    reg(it, "fgetl", |it, a, _| getline(it, a, false));
    reg(it, "fgets", |it, a, _| getline(it, a, true));
    reg(it, "fscanf", fscanf);
    reg(it, "feof", feof);
    reg(it, "frewind", |it, a, _| {
        need(&a, 1, "frewind")?;
        let id = arg_f64(&a[0], "frewind")? as i32;
        if let Some(f) = it.files_open.get_mut(&id) {
            f.pos = 0;
            if let Some(w) = &mut f.writer {
                let _ = w.rewind();
            }
        }
        Ok(vec![])
    });
    reg(it, "fflush", |it, _, _| {
        it.flush();
        Ok(vec![])
    });
    reg(it, "stdout", |_, _, _| one(Value::num(1.0)));
    reg(it, "stderr", |_, _, _| one(Value::num(2.0)));
    reg(it, "fileread", fileread);
    reg(it, "fullfile", fullfile);
    reg(it, "fileparts", fileparts);
    reg(it, "tempdir", |_, _, _| {
        let mut s = std::env::temp_dir().display().to_string();
        if !s.ends_with('/') {
            s.push('/');
        }
        one(Value::str(&s))
    });
    reg(it, "tempname", tempname);
    reg(it, "isfile", |it, a, _| {
        need(&a, 1, "isfile")?;
        let p = resolve_path(it, &arg_str(&a[0], "isfile")?);
        one(Value::bool(p.is_file()))
    });
    reg(it, "isfolder", |it, a, _| {
        need(&a, 1, "isfolder")?;
        let p = resolve_path(it, &arg_str(&a[0], "isfolder")?);
        one(Value::bool(p.is_dir()))
    });
    reg(it, "isdir", |it, a, _| {
        need(&a, 1, "isdir")?;
        let p = resolve_path(it, &arg_str(&a[0], "isdir")?);
        one(Value::bool(p.is_dir()))
    });
    reg(it, "delete", |it, a, _| {
        for v in &a {
            if let Value::HObject(h) = v {
                if let Some(cls) = crate::classes::class_of(v)
                    && let Some(m) = cls.method("delete")
                {
                    it.call_method_info(&cls, &m, vec![v.clone()], 0)?;
                }
                h.borrow_mut().set("__deleted__", Value::bool(true));
                continue;
            }
            if let Some(s) = v.to_str() {
                let _ = std::fs::remove_file(resolve_path(it, &s));
            }
        }
        Ok(vec![])
    });
    reg(it, "ferror", |it, a, nargout| {
        need(&a, 1, "ferror")?;
        let id = arg_f64(&a[0], "ferror")? as i32;
        if id > 2 && !it.files_open.contains_key(&id) {
            return Err(err("MATLAB:badfid_mx", "Invalid file identifier."));
        }
        let out = vec![Value::str(""), Value::num(0.0)];
        Ok(out.into_iter().take(nargout.max(1)).collect())
    });
    reg(it, "unlink", |it, a, nargout| {
        need(&a, 1, "unlink")?;
        let p = resolve_path(it, &arg_str(&a[0], "unlink")?);
        let (code, msg) = match std::fs::remove_file(&p) {
            Ok(()) => (0.0, String::new()),
            Err(e) => (-1.0, e.to_string()),
        };
        let out = vec![Value::num(code), Value::str(&msg)];
        Ok(out.into_iter().take(nargout.max(1)).collect())
    });
    reg(it, "canonicalize_file_name", |it, a, nargout| {
        need(&a, 1, "canonicalize_file_name")?;
        let p = resolve_path(it, &arg_str(&a[0], "canonicalize_file_name")?);
        let out = match p.canonicalize() {
            Ok(c) => vec![
                Value::str(&c.display().to_string()),
                Value::num(0.0),
                Value::str(""),
            ],
            Err(e) => vec![Value::str(""), Value::num(-1.0), Value::str(&e.to_string())],
        };
        Ok(out.into_iter().take(nargout.max(1)).collect())
    });
    reg(it, "copyfile", |it, a, nargout| {
        file_op(it, a, nargout, false)
    });
    reg(it, "movefile", |it, a, nargout| {
        file_op(it, a, nargout, true)
    });
    reg(it, "mkdir", |it, a, nargout| {
        need(&a, 1, "mkdir")?;
        let mut p = resolve_path(it, &arg_str(&a[0], "mkdir")?);
        if let Some(v) = a.get(1) {
            p = p.join(arg_str(v, "mkdir")?);
        }
        let ok = std::fs::create_dir_all(&p).is_ok();
        if nargout > 0 {
            return one(Value::bool(ok));
        }
        Ok(vec![])
    });
    reg(it, "rmdir", |it, a, nargout| {
        need(&a, 1, "rmdir")?;
        let p = resolve_path(it, &arg_str(&a[0], "rmdir")?);
        let rec = a.get(1).and_then(|v| v.to_str()).is_some_and(|s| s == "s");
        let ok = if rec {
            std::fs::remove_dir_all(&p).is_ok()
        } else {
            std::fs::remove_dir(&p).is_ok()
        };
        if nargout > 0 {
            return one(Value::bool(ok));
        }
        Ok(vec![])
    });
}

fn file_op(it: &mut Interp, a: Vec<Value>, nargout: usize, mv: bool) -> R<Vec<Value>> {
    need(&a, 2, "copyfile")?;
    let src = resolve_path(it, &arg_str(&a[0], "copyfile")?);
    let mut dst = resolve_path(it, &arg_str(&a[1], "copyfile")?);
    if dst.is_dir()
        && let Some(name) = src.file_name()
    {
        dst = dst.join(name);
    }
    let r = if mv {
        std::fs::rename(&src, &dst)
    } else {
        std::fs::copy(&src, &dst).map(|_| ())
    };
    match r {
        Ok(()) => {
            if nargout > 0 {
                return Ok(vec![Value::bool(true), Value::str(""), Value::str("")]
                    .into_iter()
                    .take(nargout)
                    .collect());
            }
            Ok(vec![])
        }
        Err(e) => {
            if nargout > 0 {
                return Ok(vec![
                    Value::bool(false),
                    Value::str(&e.to_string()),
                    Value::str(""),
                ]
                .into_iter()
                .take(nargout)
                .collect());
            }
            Err(err("MATLAB:COPYFILE:OSError", format!("{}", e)))
        }
    }
}

/// `[filename, permission, machinefmt, encodingOut] = fopen(fid)`; all empty
/// for a fid that is not open.
fn fopen_query(it: &Interp, id: i32, nargout: usize) -> R<Vec<Value>> {
    let (name, mode, machine, enc) = match id {
        0 => ("stdin", "r", "ieee-le", "UTF-8"),
        1 => ("stdout", "w", "ieee-le", "UTF-8"),
        2 => ("stderr", "w", "ieee-le", "UTF-8"),
        _ => match it.files_open.get(&id) {
            Some(f) => (
                f.name.as_str(),
                f.mode.as_str(),
                if f.big_endian { "ieee-be" } else { "ieee-le" },
                f.enc.name(),
            ),
            None => ("", "", "", ""),
        },
    };
    let out = [name, mode, machine, enc].map(Value::str);
    Ok(out.into_iter().take(nargout.max(1)).collect())
}

/// The permission `fopen(fid)` reports: binary unless text mode was requested.
fn reported_mode(mode: &str) -> String {
    if mode.contains(['t', 'b']) {
        mode.to_string()
    } else {
        format!("{}b", mode)
    }
}

fn fopen(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "fopen")?;
    if matches!(a[0], Value::Num(_)) && a[0].is_scalar() {
        let id = arg_f64(&a[0], "fopen")?;
        return fopen_query(it, id as i32, nargout);
    }
    let name = arg_str(&a[0], "fopen")?;
    if name == "all" {
        let ids: Vec<f64> = it.files_open.keys().map(|k| *k as f64).collect();
        return one(Value::row(ids));
    }
    let mode = match a.get(1) {
        Some(v) => arg_str(v, "fopen")?,
        None => "r".into(),
    };
    // Checked first: a rejected call must not truncate a file opened with 'w'.
    let big_endian = match a.get(2).map(|v| arg_str(v, "fopen")).transpose()? {
        Some(s) if !s.is_empty() => super::extra::machine_be(&s, "fopen")?,
        _ => false,
    };
    let enc = match a.get(3) {
        Some(v) => TextEnc::parse(&arg_str(v, "fopen")?)?,
        None => TextEnc::Utf8,
    };
    let path = resolve_path(it, &name);
    let m = mode.replace(['t', 'b'], "");
    let res: Result<(Vec<u8>, Option<std::fs::File>), String> =
        if m.starts_with('r') && !m.contains('+') {
            std::fs::read(&path)
                .map(|data| (data, None))
                .map_err(|e| e.to_string())
        } else {
            let mut o = std::fs::OpenOptions::new();
            if m.starts_with('a') {
                o.append(true).create(true);
            } else if m.starts_with('w') {
                o.write(true).create(true).truncate(true);
            } else {
                o.read(true).write(true);
            }
            if m.contains('+') {
                o.read(true);
            }
            o.open(&path)
                .map(|f| (vec![], Some(f)))
                .map_err(|e| e.to_string())
        };
    match res {
        Ok((data, writer)) => {
            let id = (3..).find(|k| !it.files_open.contains_key(k)).unwrap();
            let f = OpenFile {
                name,
                path,
                mode: reported_mode(&mode),
                data,
                pos: 0,
                writer,
                enc,
                big_endian,
            };
            it.files_open.insert(id, f);
            let mut out = vec![Value::num(id as f64)];
            if nargout > 1 {
                out.push(Value::str(""));
            }
            Ok(out)
        }
        Err(e) => {
            let mut out = vec![Value::num(-1.0)];
            if nargout > 1 {
                out.push(Value::str(&e));
            }
            Ok(out)
        }
    }
}

fn fclose(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "fclose")?;
    if a[0].to_str().is_some_and(|s| s == "all") {
        it.files_open.clear();
        return one(Value::num(0.0));
    }
    let id = arg_f64(&a[0], "fclose")? as i32;
    match it.files_open.remove(&id) {
        Some(mut f) => {
            if let Some(w) = &mut f.writer {
                let _ = w.flush();
            }
            one(Value::num(0.0))
        }
        None => Err(err("MATLAB:fclose:invalidFid", "Invalid file identifier.")),
    }
}

pub fn write_to(it: &mut Interp, id: i32, s: &str) -> R<()> {
    match it.files_open.get_mut(&id) {
        Some(f) => match &mut f.writer {
            Some(w) => {
                w.write_all(&f.enc.encode(s))
                    .map_err(|e| err("MATLAB:fprintf:writeFailed", format!("fprintf: {}", e)))?;
                Ok(())
            }
            None => Err(err(
                "MATLAB:fprintf:readOnly",
                "fprintf: file is open for reading only.",
            )),
        },
        None => Err(err(
            "MATLAB:badfid_mx",
            "Invalid file identifier. Use fopen to generate a valid file identifier.",
        )),
    }
}

fn getline(it: &mut Interp, a: Vec<Value>, keep: bool) -> R<Vec<Value>> {
    need(&a, 1, "fgetl")?;
    let id = arg_f64(&a[0], "fgetl")? as i32;
    let Some(f) = it.files_open.get_mut(&id) else {
        return Err(err("MATLAB:badfid_mx", "Invalid file identifier."));
    };
    let enc = f.enc;
    let rest = f.rest();
    if rest.is_empty() {
        return one(Value::num(-1.0));
    }
    let end = rest
        .iter()
        .position(|b| *b == b'\n')
        .map_or(rest.len(), |p| p + 1);
    let mut s = enc.decode(&rest[..end]);
    f.advance(end);
    if !keep {
        while s.ends_with('\n') || s.ends_with('\r') {
            s.pop();
        }
    }
    one(Value::str(&s))
}

/// `[A, count, errmsg] = fscanf(fid, format, size)`: `sscanf` over the decoded
/// rest of the file; the position moves past what was actually consumed.
fn fscanf(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "fscanf")?;
    let id = arg_f64(&a[0], "fscanf")? as i32;
    let Some(f) = it.files_open.get_mut(&id) else {
        return Err(err("MATLAB:badfid_mx", "Invalid file identifier."));
    };
    let enc = f.enc;
    let (chars, offsets) = enc.decode_offsets(&f.rest());
    let mut args = vec![Value::Char(Array::row(chars)), a[1].clone()];
    args.extend(a.get(2).cloned());
    let mut out = super::regex::sscanf_b(args, 4)?;
    let next = out.get(3).and_then(|v| v.scalar_f64()).unwrap_or(1.0) as usize;
    f.advance(offsets[next.saturating_sub(1).min(offsets.len() - 1)]);
    out.truncate(nargout.clamp(1, 3));
    Ok(out)
}

fn feof(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "feof")?;
    let id = arg_f64(&a[0], "feof")? as i32;
    let Some(f) = it.files_open.get_mut(&id) else {
        return Err(err("MATLAB:badfid_mx", "Invalid file identifier."));
    };
    one(Value::bool(f.readable() && f.rest().is_empty()))
}

fn fileread(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "fileread")?;
    let p = resolve_path(it, &arg_str(&a[0], "fileread")?);
    match std::fs::read(&p) {
        Ok(b) => one(Value::str(&String::from_utf8_lossy(&b))),
        Err(e) => Err(err(
            "MATLAB:fileread:cannotOpenFile",
            format!("Could not open file {}. {}", p.display(), e),
        )),
    }
}

fn fullfile(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let parts: Vec<String> = a
        .iter()
        .map(|v| arg_str(v, "fullfile"))
        .collect::<R<Vec<_>>>()?;
    let parts: Vec<&String> = parts.iter().filter(|p| !p.is_empty()).collect();
    let mut s = String::new();
    for (i, p) in parts.iter().enumerate() {
        if i > 0 && !s.ends_with('/') {
            s.push('/');
        }
        if i > 0 {
            s.push_str(p.trim_start_matches('/'));
        } else {
            s.push_str(p);
        }
    }
    while s.contains("//") {
        s = s.replace("//", "/");
    }
    one(Value::str(&s))
}

fn fileparts(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "fileparts")?;
    let s = arg_str(&a[0], "fileparts")?;
    let (dir, file) = match s.rfind('/') {
        Some(i) => (s[..i].to_string(), s[i + 1..].to_string()),
        None => (String::new(), s.clone()),
    };
    let dir = if dir.is_empty() && s.starts_with('/') {
        "/".to_string()
    } else {
        dir
    };
    let (name, ext) = match file.rfind('.') {
        Some(i) => (file[..i].to_string(), file[i..].to_string()),
        None => (file.clone(), String::new()),
    };
    let out = vec![Value::str(&dir), Value::str(&name), Value::str(&ext)];
    Ok(out.into_iter().take(nargout.max(1)).collect())
}

fn tempname(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let dir = match a.first() {
        Some(v) => PathBuf::from(arg_str(v, "tempname")?),
        None => std::env::temp_dir(),
    };
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let p = dir.join(format!("tp{:x}{:x}", t, std::process::id()));
    one(Value::str(&p.display().to_string()))
}

#[allow(dead_code)]
fn unused(_: &Path, _: Num) -> Value {
    cellstr_value(vec![], true)
}
