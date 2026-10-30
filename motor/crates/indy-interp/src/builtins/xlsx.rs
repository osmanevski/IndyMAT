//! Minimal `.xlsx` workbooks (SpreadsheetML in an OPC zip), written from
//! ECMA-376 and the PKWARE APPNOTE: read and write zip (stored and deflate),
//! workbook, worksheets, shared and inline strings, date styles.
//!
//! `tableio.rs` routes `readtable`, `readmatrix`, `readcell` and friends here
//! for `.xlsx` names; `.xls`, `.ods` and the like stay an explicit error.
//!
//! Values only: numbers, text, booleans, dates. A formula reads as its cached
//! value. Writing to an existing workbook keeps every sheet's values but
//! rewrites the file, so its formatting, formulas and charts are not kept.
//! `NaN` and missing values are written as empty cells; `Inf` has no number
//! form in SpreadsheetML and is written as the text `Inf`/`-Inf`, which the
//! readers turn back into a number.

use std::collections::BTreeMap;

use indy_value::deflate::{crc32, deflate, inflate_limited};
use indy_value::{Num, NumClass, Value};

use super::core::resolve_path;
use super::datatypes::is_native;
use super::datatypes::table::Tab;
use super::err;
use crate::{Interp, R};

const MAX_ROW: u32 = 1_048_576;
const MAX_COL: u32 = 16_384;
/// Largest zip member accepted, uncompressed.
const MAX_PART: u64 = 1 << 31;
/// Largest grid handed to the table readers, in cells.
const MAX_GRID: u64 = 1 << 28;
/// An explicit range larger than this is clipped to the used range.
const MAX_PADDED: u64 = 1_000_000;

#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    Num(f64),
    Text(String),
    Bool(bool),
    /// Excel serial date.
    Date(f64),
}

#[derive(Clone, Debug, Default)]
pub struct Sheet {
    pub name: String,
    /// Keyed by 1-based (row, column), so iteration is in row order.
    pub cells: BTreeMap<(u32, u32), Cell>,
}

#[derive(Clone, Debug, Default)]
pub struct Workbook {
    pub sheets: Vec<Sheet>,
}

fn fail(fname: &str, what: &str, msg: impl Into<String>) -> Box<indy_value::Error> {
    err(&format!("MATLAB:{}:{}", fname, what), msg)
}

fn le16(b: &[u8], p: usize) -> usize {
    u16::from_le_bytes([b[p], b[p + 1]]) as usize
}

fn le32(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes([b[p], b[p + 1], b[p + 2], b[p + 3]])
}

/// Whether a file name and optional `FileType` select this module.
pub fn is_target(name: &str, ftype: Option<&str>) -> bool {
    let base = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .to_lowercase();
    base.ends_with(".xlsx") && ftype.is_none_or(|t| t.eq_ignore_ascii_case("spreadsheet"))
}

/// `is_target` for a writer's name-value list.
pub fn wants_write(name: &str, opts: &[Value]) -> bool {
    let ftype = opts
        .chunks(2)
        .find_map(|p| match (p[0].to_str(), p.get(1)) {
            (Some(n), Some(v)) if n.eq_ignore_ascii_case("filetype") => v.to_str(),
            _ => None,
        });
    is_target(name, ftype.as_deref())
}

/// Text form of a number that parses back to the same value.
pub fn num_text(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        format!("{:?}", x)
    }
}

// ---------------------------------------------------------------------------
// Ranges
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Range {
    pub r1: Option<u32>,
    pub c1: Option<u32>,
    pub r2: Option<u32>,
    pub c2: Option<u32>,
}

/// `A1` parts: letters then digits, either of which may be absent.
fn parse_ref(s: &str) -> Option<(Option<u32>, Option<u32>)> {
    let split = s
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(s.len());
    let (letters, digits) = s.split_at(split);
    let col = if letters.is_empty() {
        None
    } else {
        let mut c = 0u32;
        for b in letters.bytes() {
            c = c
                .checked_mul(26)?
                .checked_add((b.to_ascii_uppercase() - b'A') as u32 + 1)?;
        }
        if c > MAX_COL {
            return None;
        }
        Some(c)
    };
    let row = if digits.is_empty() {
        None
    } else {
        if !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let r: u32 = digits.parse().ok()?;
        if r == 0 || r > MAX_ROW {
            return None;
        }
        Some(r)
    };
    (row.is_some() || col.is_some()).then_some((row, col))
}

fn col_name(mut c: u32) -> String {
    let mut out = Vec::new();
    while c > 0 {
        let d = (c - 1) % 26;
        out.push(b'A' + d as u8);
        c = (c - 1) / 26;
    }
    out.reverse();
    String::from_utf8_lossy(&out).into_owned()
}

/// `'B2'`, `'B2:C5'`, `'A:C'`, `'2:5'`, `[r c]` or `[r1 c1 r2 c2]`.
pub fn parse_range(v: &Value, fname: &str) -> R<Range> {
    let bad = || {
        fail(
            fname,
            "invalidRange",
            format!(
                "{}: 'Range' must look like 'B2', 'B2:C5', 'A:C', '2:5', [r c] or [r1 c1 r2 c2]; named ranges are not supported.",
                fname
            ),
        )
    };
    if v.is_text() {
        let s = v.to_str().ok_or_else(bad)?;
        let s = s.trim();
        let (a, b) = match s.split_once(':') {
            Some((a, b)) => (a, Some(b)),
            None => (s, None),
        };
        let (r1, c1) = parse_ref(a).ok_or_else(bad)?;
        let mut range = Range {
            r1,
            c1,
            r2: None,
            c2: None,
        };
        if let Some(b) = b {
            let (r2, c2) = parse_ref(b).ok_or_else(bad)?;
            // Both ends must have the same shape: 'A1:B2', 'A:B' or '1:2'.
            if r1.is_some() != r2.is_some() || c1.is_some() != c2.is_some() {
                return Err(bad());
            }
            range.r2 = r2;
            range.c2 = c2;
        } else if r1.is_none() || c1.is_none() {
            return Err(bad());
        }
        return check_order(range).ok_or_else(bad);
    }
    let x = v.to_f64_vec().filter(|_| v.is_numeric()).ok_or_else(bad)?;
    let whole =
        |t: f64, max: u32| (t.fract() == 0.0 && t >= 1.0 && t <= max as f64).then_some(t as u32);
    let range = match x.as_slice() {
        [r, c] => Range {
            r1: Some(whole(*r, MAX_ROW).ok_or_else(bad)?),
            c1: Some(whole(*c, MAX_COL).ok_or_else(bad)?),
            r2: None,
            c2: None,
        },
        [r1, c1, r2, c2] => Range {
            r1: Some(whole(*r1, MAX_ROW).ok_or_else(bad)?),
            c1: Some(whole(*c1, MAX_COL).ok_or_else(bad)?),
            r2: Some(whole(*r2, MAX_ROW).ok_or_else(bad)?),
            c2: Some(whole(*c2, MAX_COL).ok_or_else(bad)?),
        },
        _ => return Err(bad()),
    };
    check_order(range).ok_or_else(bad)
}

fn check_order(r: Range) -> Option<Range> {
    let ok = |a: Option<u32>, b: Option<u32>| match (a, b) {
        (Some(x), Some(y)) => x <= y,
        _ => true,
    };
    (ok(r.r1, r.r2) && ok(r.c1, r.c2)).then_some(r)
}

// ---------------------------------------------------------------------------
// Zip
// ---------------------------------------------------------------------------

struct ZipEntry {
    name: String,
    flags: usize,
    method: usize,
    crc: u32,
    comp: u64,
    size: u64,
    offset: u64,
}

fn zip_entries(b: &[u8]) -> Result<Vec<ZipEntry>, String> {
    const NOT_ZIP: &str = "the file is not a valid .xlsx workbook (not a zip archive)";
    if b.len() < 22 {
        return Err(NOT_ZIP.into());
    }
    let lowest = b.len().saturating_sub(22 + 65535);
    let eocd = (lowest..=b.len() - 22)
        .rev()
        .find(|&p| b[p..].starts_with(b"PK\x05\x06"))
        .ok_or(NOT_ZIP)?;
    let count = le16(b, eocd + 10);
    let start = le32(b, eocd + 16);
    if count == 0xFFFF || start == u32::MAX {
        return Err("zip64 workbooks are not supported yet in the Rust engine".into());
    }
    let mut p = start as usize;
    let mut out = Vec::new();
    for _ in 0..count {
        if p > b.len() || b.len() - p < 46 || !b[p..].starts_with(b"PK\x01\x02") {
            return Err("the zip directory of the workbook is corrupt".into());
        }
        let (n, e, c) = (le16(b, p + 28), le16(b, p + 30), le16(b, p + 32));
        if b.len() - p - 46 < n + e + c {
            return Err("the zip directory of the workbook is corrupt".into());
        }
        out.push(ZipEntry {
            name: String::from_utf8_lossy(&b[p + 46..p + 46 + n]).replace('\\', "/"),
            flags: le16(b, p + 8),
            method: le16(b, p + 10),
            crc: le32(b, p + 16),
            comp: le32(b, p + 20) as u64,
            size: le32(b, p + 24) as u64,
            offset: le32(b, p + 42) as u64,
        });
        p += 46 + n + e + c;
    }
    Ok(out)
}

fn zip_read(b: &[u8], e: &ZipEntry) -> Result<Vec<u8>, String> {
    let bad = || format!("the workbook part {} is corrupt", e.name);
    if e.flags & 1 != 0 {
        return Err("encrypted workbooks are not supported".into());
    }
    if e.size > MAX_PART {
        return Err(format!("the workbook part {} is too large", e.name));
    }
    let off = usize::try_from(e.offset).map_err(|_| bad())?;
    if off > b.len() || b.len() - off < 30 || !b[off..].starts_with(b"PK\x03\x04") {
        return Err(bad());
    }
    let start = off + 30 + le16(b, off + 26) + le16(b, off + 28);
    let comp = usize::try_from(e.comp).map_err(|_| bad())?;
    let data = b
        .get(start..start.checked_add(comp).ok_or_else(bad)?)
        .ok_or_else(bad)?;
    let out = match e.method {
        0 => data.to_vec(),
        8 => {
            inflate_limited(data, e.size as usize)
                .map_err(|m| format!("the workbook part {}: {}", e.name, m))?
                .0
        }
        m => return Err(format!("zip compression method {} is not supported", m)),
    };
    if out.len() as u64 != e.size || crc32(&out) != e.crc {
        return Err(bad());
    }
    Ok(out)
}

fn zip_write(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut dir = Vec::new();
    // MS-DOS date of 1980-01-01: the output depends only on the content.
    const DOS_DATE: u16 = 0x0021;
    for (name, data) in files {
        let packed = deflate(data);
        let (method, body): (u16, &[u8]) = if packed.len() < data.len() {
            (8, &packed)
        } else {
            (0, data)
        };
        let offset = out.len();
        if offset > u32::MAX as usize || data.len() > u32::MAX as usize {
            return Err("the workbook does not fit a zip file without zip64".into());
        }
        let crc = crc32(data);
        // Fields shared by the local header and the central directory.
        let mut common = Vec::new();
        common.extend_from_slice(&20u16.to_le_bytes());
        common.extend_from_slice(&0u16.to_le_bytes());
        common.extend_from_slice(&method.to_le_bytes());
        common.extend_from_slice(&0u16.to_le_bytes());
        common.extend_from_slice(&DOS_DATE.to_le_bytes());
        common.extend_from_slice(&crc.to_le_bytes());
        common.extend_from_slice(&(body.len() as u32).to_le_bytes());
        common.extend_from_slice(&(data.len() as u32).to_le_bytes());
        common.extend_from_slice(&(name.len() as u16).to_le_bytes());
        common.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(b"PK\x03\x04");
        out.extend_from_slice(&common);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(body);
        dir.extend_from_slice(b"PK\x01\x02");
        dir.extend_from_slice(&20u16.to_le_bytes());
        dir.extend_from_slice(&common);
        dir.extend_from_slice(&[0u8; 10]);
        dir.extend_from_slice(&(offset as u32).to_le_bytes());
        dir.extend_from_slice(name.as_bytes());
    }
    let start = out.len();
    if start > u32::MAX as usize || start + dir.len() > u32::MAX as usize {
        return Err("the workbook does not fit a zip file without zip64".into());
    }
    out.extend_from_slice(&dir);
    out.extend_from_slice(b"PK\x05\x06");
    out.extend_from_slice(&[0u8; 4]);
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(dir.len() as u32).to_le_bytes());
    out.extend_from_slice(&(start as u32).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    Ok(out)
}

// ---------------------------------------------------------------------------
// XML
// ---------------------------------------------------------------------------

enum Ev<'a> {
    Open {
        name: &'a str,
        attrs: &'a str,
        empty: bool,
    },
    Close(&'a str),
    /// Character data with entities still encoded.
    Text(&'a str),
    /// CDATA content, taken literally.
    Raw(&'a str),
}

/// Non-validating pull parser. Document type declarations are skipped, so no
/// custom entity is ever expanded.
struct Xml<'a> {
    s: &'a str,
    pos: usize,
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

impl<'a> Xml<'a> {
    fn new(s: &'a str) -> Self {
        Xml {
            s: s.strip_prefix('\u{FEFF}').unwrap_or(s),
            pos: 0,
        }
    }
    fn after(&mut self, from: usize, mark: &str) -> Result<usize, String> {
        match self.s[from..].find(mark) {
            Some(k) => Ok(from + k + mark.len()),
            None => Err("malformed XML in the workbook".into()),
        }
    }
    fn next_event(&mut self) -> Result<Option<Ev<'a>>, String> {
        loop {
            let rest = &self.s[self.pos..];
            if rest.is_empty() {
                return Ok(None);
            }
            if !rest.starts_with('<') {
                let end = rest.find('<').unwrap_or(rest.len());
                self.pos += end;
                return Ok(Some(Ev::Text(&rest[..end])));
            }
            if rest.starts_with("<!--") {
                self.pos = self.after(self.pos + 4, "-->")?;
            } else if rest.starts_with("<![CDATA[") {
                let begin = self.pos + 9;
                let end = self.after(begin, "]]>")?;
                self.pos = end;
                return Ok(Some(Ev::Raw(&self.s[begin..end - 3])));
            } else if rest.starts_with("<?") {
                self.pos = self.after(self.pos + 2, "?>")?;
            } else if rest.starts_with("<!") {
                self.pos = self.after(self.pos + 2, ">")?;
            } else if rest.starts_with("</") {
                let end = self.after(self.pos + 2, ">")?;
                let name = self.s[self.pos + 2..end - 1].trim();
                self.pos = end;
                return Ok(Some(Ev::Close(local(name))));
            } else {
                // Scan for '>' outside attribute quotes.
                let mut quote = None;
                let mut end = None;
                for (k, b) in rest.bytes().enumerate().skip(1) {
                    match (quote, b) {
                        (Some(q), _) if q == b => quote = None,
                        (Some(_), _) => {}
                        (None, b'"' | b'\'') => quote = Some(b),
                        (None, b'>') => {
                            end = Some(k);
                            break;
                        }
                        _ => {}
                    }
                }
                let Some(end) = end else {
                    return Err("malformed XML in the workbook".into());
                };
                let inner = &rest[1..end];
                self.pos += end + 1;
                let (inner, empty) = match inner.strip_suffix('/') {
                    Some(x) => (x, true),
                    None => (inner, false),
                };
                let cut = inner
                    .find(|c: char| c.is_ascii_whitespace())
                    .unwrap_or(inner.len());
                return Ok(Some(Ev::Open {
                    name: local(&inner[..cut]),
                    attrs: &inner[cut..],
                    empty,
                }));
            }
        }
    }
}

/// Value of the attribute with this local name.
fn attr(attrs: &str, key: &str) -> Option<String> {
    let mut rest = attrs;
    loop {
        rest = rest.trim_start();
        let eq = rest.find('=')?;
        let name = rest[..eq].trim();
        let after = rest[eq + 1..].trim_start();
        let q = after.chars().next()?;
        if q != '"' && q != '\'' {
            return None;
        }
        let end = after[1..].find(q)?;
        if local(name) == key {
            return Some(decode_entities(&after[1..1 + end]));
        }
        rest = &after[end + 2..];
    }
}

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(k) = rest.find('&') {
        out.push_str(&rest[..k]);
        rest = &rest[k..];
        let Some(end) = rest.find(';') else {
            break;
        };
        let body = &rest[1..end];
        let ch = match body {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => {
                let code = match body.strip_prefix("#x").or_else(|| body.strip_prefix("#X")) {
                    Some(h) => u32::from_str_radix(h, 16).ok(),
                    None => body.strip_prefix('#').and_then(|d| d.parse().ok()),
                };
                code.and_then(char::from_u32)
            }
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                // Not an entity we know: keep it as literal text.
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Text that would read as an ST_Xstring escape has to be escaped.
            '_' if is_x_escape(&chars[i..]) => out.push_str("_x005F_"),
            '\t' | '\n' | '\r' => out.push(c),
            // Other control characters cannot appear in XML 1.0.
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {
                out.push_str(&format!("_x{:04X}_", c as u32))
            }
            c => out.push(c),
        }
    }
    out
}

/// Whether the text starts with `_xHHHH_`.
fn is_x_escape(c: &[char]) -> bool {
    c.len() >= 7
        && c[0] == '_'
        && c[1] == 'x'
        && c[2..6].iter().all(|d| d.is_ascii_hexdigit())
        && c[6] == '_'
}

/// Undo the `_xHHHH_` escapes of ECMA-376 ST_Xstring.
fn decode_xstring(s: &str) -> String {
    if !s.contains("_x") {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if is_x_escape(&chars[i..]) {
            let hex: String = chars[i + 2..i + 6].iter().collect();
            let ch = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32);
            if let Some(c) = ch {
                out.push(c);
                i += 7;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// Workbook reading
// ---------------------------------------------------------------------------

fn part_text(bytes: Vec<u8>, name: &str) -> Result<String, String> {
    String::from_utf8(bytes).map_err(|_| format!("the workbook part {} is not UTF-8", name))
}

struct Parts<'a> {
    bytes: &'a [u8],
    entries: Vec<ZipEntry>,
}

impl Parts<'_> {
    fn text(&self, name: &str) -> Result<Option<String>, String> {
        match self.entries.iter().find(|e| e.name == name) {
            Some(e) => part_text(zip_read(self.bytes, e)?, name).map(Some),
            None => Ok(None),
        }
    }
}

/// Join a relationship target to the folder of its source part.
fn join_target(base: &str, target: &str) -> String {
    let full = match target.strip_prefix('/') {
        Some(t) => t.to_string(),
        None if base.is_empty() => target.to_string(),
        None => format!("{}/{}", base, target),
    };
    let mut parts: Vec<&str> = Vec::new();
    for p in full.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            x => parts.push(x),
        }
    }
    parts.join("/")
}

fn shared_strings(xml: &str) -> Result<Vec<String>, String> {
    let mut x = Xml::new(xml);
    let mut out = Vec::new();
    let mut cur = String::new();
    let (mut in_t, mut phonetic) = (false, 0usize);
    while let Some(ev) = x.next_event()? {
        match ev {
            Ev::Open { name, empty, .. } => match name {
                "si" if empty => out.push(String::new()),
                "si" => cur.clear(),
                "rPh" if !empty => phonetic += 1,
                "t" if !empty && phonetic == 0 => in_t = true,
                _ => {}
            },
            Ev::Close(name) => match name {
                "si" => out.push(decode_xstring(&cur)),
                "rPh" => phonetic = phonetic.saturating_sub(1),
                "t" => in_t = false,
                _ => {}
            },
            Ev::Text(t) if in_t => cur.push_str(&decode_entities(t)),
            Ev::Raw(t) if in_t => cur.push_str(t),
            _ => {}
        }
    }
    Ok(out)
}

fn is_date_format(code: &str) -> bool {
    let mut plain = String::new();
    let (mut quoted, mut bracket, mut skip) = (false, false, false);
    for c in code.chars() {
        if skip {
            skip = false;
        } else if quoted {
            quoted = c != '"';
        } else if bracket {
            bracket = c != ']';
        } else {
            match c {
                '"' => quoted = true,
                '[' => bracket = true,
                '\\' | '_' | '*' => skip = true,
                c => plain.push(c.to_ascii_lowercase()),
            }
        }
    }
    plain != "general" && plain.chars().any(|c| matches!(c, 'y' | 'd' | 'h' | 's'))
}

/// For each `cellXfs` entry, whether it formats a date or time.
fn date_styles(xml: &str) -> Result<Vec<bool>, String> {
    let mut x = Xml::new(xml);
    let mut custom: BTreeMap<u32, bool> = BTreeMap::new();
    let mut out = Vec::new();
    let mut in_xfs = false;
    while let Some(ev) = x.next_event()? {
        match ev {
            Ev::Open { name, attrs, empty } => match name {
                "numFmt" => {
                    if let (Some(id), Some(code)) =
                        (attr(attrs, "numFmtId"), attr(attrs, "formatCode"))
                        && let Ok(id) = id.parse()
                    {
                        custom.insert(id, is_date_format(&code));
                    }
                }
                "cellXfs" if !empty => in_xfs = true,
                "xf" if in_xfs => {
                    let id: u32 = attr(attrs, "numFmtId")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                    let builtin = matches!(id, 14..=22 | 45..=47);
                    out.push(custom.get(&id).copied().unwrap_or(builtin));
                }
                _ => {}
            },
            Ev::Close("cellXfs") => in_xfs = false,
            _ => {}
        }
    }
    Ok(out)
}

struct SheetContext<'a> {
    strings: &'a [String],
    dates: &'a [bool],
    /// Days to add to a serial date of a workbook on the 1904 system.
    date_offset: f64,
}

fn finish_cell(
    kind: &str,
    style: usize,
    v: &str,
    inline: &str,
    ctx: &SheetContext<'_>,
) -> Result<Option<Cell>, String> {
    let v = v.trim();
    if kind == "inlineStr" {
        return Ok((!inline.is_empty()).then(|| Cell::Text(decode_xstring(inline))));
    }
    if v.is_empty() {
        return Ok(None);
    }
    Ok(Some(match kind {
        "s" => {
            let k: usize = v
                .parse()
                .map_err(|_| "a shared string index in the workbook is invalid".to_string())?;
            let s = ctx
                .strings
                .get(k)
                .ok_or("a shared string index in the workbook is out of range")?;
            if s.is_empty() {
                return Ok(None);
            }
            Cell::Text(s.clone())
        }
        "str" | "e" | "d" => Cell::Text(decode_xstring(v)),
        "b" => Cell::Bool(v == "1" || v.eq_ignore_ascii_case("true")),
        "" | "n" => {
            let x: f64 = v
                .parse()
                .ok()
                .filter(|x: &f64| x.is_finite())
                .ok_or_else(|| format!("the workbook has an invalid number '{}'", v))?;
            if ctx.dates.get(style).copied().unwrap_or(false) {
                Cell::Date(x + ctx.date_offset)
            } else {
                Cell::Num(x)
            }
        }
        other => return Err(format!("unknown cell type '{}' in the workbook", other)),
    }))
}

fn parse_sheet(xml: &str, sheet: &mut Sheet, ctx: &SheetContext<'_>) -> Result<(), String> {
    let mut x = Xml::new(xml);
    let (mut row, mut col) = (0u32, 0u32);
    let mut cur: Option<(u32, u32, String, usize)> = None;
    let (mut v, mut inline) = (String::new(), String::new());
    let (mut in_v, mut in_t, mut phonetic) = (false, false, 0usize);
    while let Some(ev) = x.next_event()? {
        match ev {
            Ev::Open { name, attrs, empty } => match name {
                "row" => {
                    row = match attr(attrs, "r") {
                        Some(r) => r
                            .parse()
                            .map_err(|_| "the workbook has an invalid row number".to_string())?,
                        None => row + 1,
                    };
                    if row == 0 || row > MAX_ROW {
                        return Err("the workbook has an invalid row number".into());
                    }
                    col = 0;
                }
                "c" => {
                    match attr(attrs, "r") {
                        Some(r) => match parse_ref(&r) {
                            Some((Some(rr), Some(cc))) => {
                                row = rr;
                                col = cc;
                            }
                            _ => {
                                return Err(format!(
                                    "invalid cell reference '{}' in the workbook",
                                    r
                                ));
                            }
                        },
                        None => col += 1,
                    }
                    if row == 0 || col == 0 || col > MAX_COL {
                        return Err("the workbook has a cell outside the sheet".into());
                    }
                    if !empty {
                        v.clear();
                        inline.clear();
                        let style = attr(attrs, "s").and_then(|s| s.parse().ok()).unwrap_or(0);
                        cur = Some((row, col, attr(attrs, "t").unwrap_or_default(), style));
                    }
                }
                "v" if !empty && cur.is_some() => in_v = true,
                "rPh" if !empty => phonetic += 1,
                "t" if !empty && cur.is_some() && phonetic == 0 => in_t = true,
                _ => {}
            },
            Ev::Close(name) => match name {
                "v" => in_v = false,
                "t" => in_t = false,
                "rPh" => phonetic = phonetic.saturating_sub(1),
                "c" => {
                    if let Some((r, c, kind, style)) = cur.take()
                        && let Some(cell) = finish_cell(&kind, style, &v, &inline, ctx)?
                    {
                        sheet.cells.insert((r, c), cell);
                    }
                }
                _ => {}
            },
            Ev::Text(t) => {
                if in_v {
                    v.push_str(&decode_entities(t));
                } else if in_t {
                    inline.push_str(&decode_entities(t));
                }
            }
            Ev::Raw(t) => {
                if in_v {
                    v.push_str(t);
                } else if in_t {
                    inline.push_str(t);
                }
            }
        }
    }
    Ok(())
}

fn relationships(xml: &str) -> Result<Vec<(String, String, String)>, String> {
    let mut x = Xml::new(xml);
    let mut out = Vec::new();
    while let Some(ev) = x.next_event()? {
        if let Ev::Open {
            name: "Relationship",
            attrs,
            ..
        } = ev
        {
            out.push((
                attr(attrs, "Id").unwrap_or_default(),
                attr(attrs, "Type").unwrap_or_default(),
                attr(attrs, "Target").unwrap_or_default(),
            ));
        }
    }
    Ok(out)
}

pub fn read_workbook(bytes: &[u8]) -> Result<Workbook, String> {
    let parts = Parts {
        bytes,
        entries: zip_entries(bytes)?,
    };
    let mut book_path = "xl/workbook.xml".to_string();
    if let Some(rels) = parts.text("_rels/.rels")? {
        for (_, kind, target) in relationships(&rels)? {
            if kind.ends_with("/officeDocument") {
                book_path = join_target("", &target);
            }
        }
    }
    let Some(book) = parts.text(&book_path)? else {
        return Err("the file is not a valid .xlsx workbook (no workbook part)".into());
    };
    let (base, file) = match book_path.rsplit_once('/') {
        Some((b, f)) => (b.to_string(), f.to_string()),
        None => (String::new(), book_path.clone()),
    };
    let rels_path = join_target(&base, &format!("_rels/{}.rels", file));
    let rels = match parts.text(&rels_path)? {
        Some(r) => relationships(&r)?,
        None => Vec::new(),
    };
    let target_of = |suffix: &str, fallback: &str| {
        rels.iter()
            .find(|r| r.1.ends_with(suffix))
            .map_or(join_target(&base, fallback), |r| join_target(&base, &r.2))
    };
    let strings = match parts.text(&target_of("/sharedStrings", "sharedStrings.xml"))? {
        Some(t) => shared_strings(&t)?,
        None => Vec::new(),
    };
    let dates = match parts.text(&target_of("/styles", "styles.xml"))? {
        Some(t) => date_styles(&t)?,
        None => Vec::new(),
    };
    let mut listed = Vec::new();
    let mut date_offset = 0.0;
    let mut x = Xml::new(&book);
    while let Some(ev) = x.next_event()? {
        match ev {
            Ev::Open {
                name: "sheet",
                attrs,
                ..
            } => listed.push((
                attr(attrs, "name").unwrap_or_default(),
                attr(attrs, "id").unwrap_or_default(),
            )),
            Ev::Open {
                name: "workbookPr",
                attrs,
                ..
            } if attr(attrs, "date1904").is_some_and(|v| v == "1" || v == "true") => {
                date_offset = 1462.0;
            }
            _ => {}
        }
    }
    let ctx = SheetContext {
        strings: &strings,
        dates: &dates,
        date_offset,
    };
    let mut wb = Workbook::default();
    for (name, id) in listed {
        let Some(rel) = rels.iter().find(|r| r.0 == id) else {
            return Err(format!("the sheet '{}' has no part in the workbook", name));
        };
        let path = join_target(&base, &rel.2);
        let Some(xml) = parts.text(&path)? else {
            return Err(format!("the sheet '{}' has no part in the workbook", name));
        };
        let mut sheet = Sheet {
            name,
            cells: BTreeMap::new(),
        };
        parse_sheet(&xml, &mut sheet, &ctx)?;
        wb.sheets.push(sheet);
    }
    Ok(wb)
}

// ---------------------------------------------------------------------------
// Workbook writing
// ---------------------------------------------------------------------------

const XML_HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";
const MAIN_NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG_REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml";
/// Index of the date style in `cellXfs`.
const DATE_STYLE: u32 = 1;

fn sheet_xml(
    sheet: &Sheet,
    strings: &mut Vec<String>,
    index: &mut BTreeMap<String, usize>,
) -> String {
    let mut out = format!("{}<worksheet xmlns=\"{}\">", XML_HEAD, MAIN_NS);
    if !sheet.cells.is_empty() {
        let r1 = sheet.cells.keys().map(|k| k.0).min().unwrap_or(1);
        let r2 = sheet.cells.keys().map(|k| k.0).max().unwrap_or(1);
        let c1 = sheet.cells.keys().map(|k| k.1).min().unwrap_or(1);
        let c2 = sheet.cells.keys().map(|k| k.1).max().unwrap_or(1);
        out.push_str(&format!(
            "<dimension ref=\"{}{}:{}{}\"/>",
            col_name(c1),
            r1,
            col_name(c2),
            r2
        ));
    }
    out.push_str("<sheetData>");
    let mut open = None;
    for (&(r, c), cell) in &sheet.cells {
        if open != Some(r) {
            if open.is_some() {
                out.push_str("</row>");
            }
            out.push_str(&format!("<row r=\"{}\">", r));
            open = Some(r);
        }
        let at = format!("{}{}", col_name(c), r);
        match cell {
            Cell::Num(x) => out.push_str(&format!("<c r=\"{}\"><v>{}</v></c>", at, num_text(*x))),
            Cell::Date(x) => out.push_str(&format!(
                "<c r=\"{}\" s=\"{}\"><v>{}</v></c>",
                at,
                DATE_STYLE,
                num_text(*x)
            )),
            Cell::Bool(b) => {
                out.push_str(&format!("<c r=\"{}\" t=\"b\"><v>{}</v></c>", at, *b as u8))
            }
            Cell::Text(s) => {
                let k = *index.entry(s.clone()).or_insert_with(|| {
                    strings.push(s.clone());
                    strings.len() - 1
                });
                out.push_str(&format!("<c r=\"{}\" t=\"s\"><v>{}</v></c>", at, k));
            }
        }
    }
    if open.is_some() {
        out.push_str("</row>");
    }
    out.push_str("</sheetData></worksheet>");
    out
}

pub fn write_workbook(wb: &Workbook) -> Result<Vec<u8>, String> {
    let n = wb.sheets.len();
    let mut strings = Vec::new();
    let mut index = BTreeMap::new();
    let mut sheets = Vec::with_capacity(n);
    for s in &wb.sheets {
        sheets.push(sheet_xml(s, &mut strings, &mut index));
    }
    let mut types = format!(
        "{}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
         <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
         <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
         <Override PartName=\"/xl/workbook.xml\" ContentType=\"{ct}.sheet.main+xml\"/>\
         <Override PartName=\"/xl/styles.xml\" ContentType=\"{ct}.styles+xml\"/>\
         <Override PartName=\"/xl/sharedStrings.xml\" ContentType=\"{ct}.sharedStrings+xml\"/>",
        XML_HEAD,
        ct = CT
    );
    let mut book = format!(
        "{}<workbook xmlns=\"{}\" xmlns:r=\"{}\"><sheets>",
        XML_HEAD, MAIN_NS, REL_NS
    );
    let mut rels = format!("{}<Relationships xmlns=\"{}\">", XML_HEAD, PKG_REL_NS);
    for (k, s) in wb.sheets.iter().enumerate() {
        let id = k + 1;
        types.push_str(&format!(
            "<Override PartName=\"/xl/worksheets/sheet{}.xml\" ContentType=\"{}.worksheet+xml\"/>",
            id, CT
        ));
        book.push_str(&format!(
            "<sheet name=\"{}\" sheetId=\"{}\" r:id=\"rId{}\"/>",
            xml_escape(&s.name),
            id,
            id
        ));
        rels.push_str(&format!(
            "<Relationship Id=\"rId{}\" Type=\"{}/worksheet\" Target=\"worksheets/sheet{}.xml\"/>",
            id, REL_NS, id
        ));
    }
    types.push_str("</Types>");
    book.push_str("</sheets></workbook>");
    rels.push_str(&format!(
        "<Relationship Id=\"rId{}\" Type=\"{ns}/styles\" Target=\"styles.xml\"/>\
         <Relationship Id=\"rId{}\" Type=\"{ns}/sharedStrings\" Target=\"sharedStrings.xml\"/>\
         </Relationships>",
        n + 1,
        n + 2,
        ns = REL_NS
    ));
    let root = format!(
        "{}<Relationships xmlns=\"{}\"><Relationship Id=\"rId1\" Type=\"{}/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>",
        XML_HEAD, PKG_REL_NS, REL_NS
    );
    // Style 1 is the built-in format 22, a date with a time.
    let styles = format!(
        "{}<styleSheet xmlns=\"{}\">\
         <fonts count=\"1\"><font><sz val=\"11\"/><name val=\"Calibri\"/></font></fonts>\
         <fills count=\"2\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill></fills>\
         <borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders>\
         <cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>\
         <cellXfs count=\"2\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/>\
         <xf numFmtId=\"22\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyNumberFormat=\"1\"/></cellXfs>\
         <cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles>\
         </styleSheet>",
        XML_HEAD, MAIN_NS
    );
    let mut sst = format!(
        "{}<sst xmlns=\"{}\" uniqueCount=\"{}\">",
        XML_HEAD,
        MAIN_NS,
        strings.len()
    );
    for s in &strings {
        let keep = s.starts_with(char::is_whitespace) || s.ends_with(char::is_whitespace);
        let space = if keep { " xml:space=\"preserve\"" } else { "" };
        sst.push_str(&format!("<si><t{}>{}</t></si>", space, xml_escape(s)));
    }
    sst.push_str("</sst>");
    let mut files = vec![
        ("[Content_Types].xml".to_string(), types.into_bytes()),
        ("_rels/.rels".to_string(), root.into_bytes()),
        ("xl/workbook.xml".to_string(), book.into_bytes()),
        ("xl/_rels/workbook.xml.rels".to_string(), rels.into_bytes()),
        ("xl/styles.xml".to_string(), styles.into_bytes()),
        ("xl/sharedStrings.xml".to_string(), sst.into_bytes()),
    ];
    for (k, s) in sheets.into_iter().enumerate() {
        files.push((format!("xl/worksheets/sheet{}.xml", k + 1), s.into_bytes()));
    }
    zip_write(&files)
}

// ---------------------------------------------------------------------------
// Reading a grid for tableio
// ---------------------------------------------------------------------------

fn sheet_index(wb: &Workbook, sel: Option<&Value>, fname: &str) -> R<usize> {
    let Some(v) = sel else {
        return Ok(0);
    };
    if v.is_text() {
        let want = v.to_str().unwrap_or_default();
        return wb
            .sheets
            .iter()
            .position(|s| s.name == want)
            .or_else(|| {
                wb.sheets
                    .iter()
                    .position(|s| s.name.eq_ignore_ascii_case(&want))
            })
            .ok_or_else(|| {
                fail(
                    fname,
                    "sheetNotFound",
                    format!(
                        "{}: the sheet '{}' does not exist in the workbook.",
                        fname, want
                    ),
                )
            });
    }
    match v.scalar_f64().filter(|_| v.is_numeric()) {
        Some(x) if x >= 1.0 && x.fract() == 0.0 && x <= wb.sheets.len() as f64 => {
            Ok(x as usize - 1)
        }
        Some(x) => Err(fail(
            fname,
            "sheetNotFound",
            format!(
                "{}: the sheet number {} does not exist; the workbook has {} sheets.",
                fname,
                x,
                wb.sheets.len()
            ),
        )),
        None => Err(fail(
            fname,
            "invalidSheet",
            format!(
                "{}: 'Sheet' must be a sheet name or a positive integer.",
                fname
            ),
        )),
    }
}

/// One row of a grid: its 1-based line relative to the range, and its cells.
pub type GridRow = (usize, Vec<Option<Cell>>);

/// Cells of a sheet selection. Without a two-corner range the grid is the used
/// range and blank rows are dropped, as the text readers drop blank lines.
pub fn read_grid(
    it: &Interp,
    fname: &str,
    name: &str,
    sheet: Option<&Value>,
    range: Option<&Value>,
) -> R<Vec<GridRow>> {
    let path = resolve_path(it, name);
    let bytes = std::fs::read(&path).map_err(|_| {
        fail(
            fname,
            "FileNotFound",
            format!(
                "{}: Unable to find or open '{}'. Check the path and filename or file permissions.",
                fname, name
            ),
        )
    })?;
    let wb = read_workbook(&bytes).map_err(|m| {
        fail(
            fname,
            "invalidWorkbook",
            format!("{}: {} ('{}').", fname, m, name),
        )
    })?;
    let range = range
        .map(|r| parse_range(r, fname))
        .transpose()?
        .unwrap_or_default();
    if wb.sheets.is_empty() {
        return Ok(Vec::new());
    }
    let cells = &wb.sheets[sheet_index(&wb, sheet, fname)?].cells;
    if cells.is_empty() {
        return Ok(Vec::new());
    }
    let used_r1 = cells.keys().map(|k| k.0).min().unwrap_or(1);
    let used_r2 = cells.keys().map(|k| k.0).max().unwrap_or(1);
    let used_c1 = cells.keys().map(|k| k.1).min().unwrap_or(1);
    let used_c2 = cells.keys().map(|k| k.1).max().unwrap_or(1);
    let keep_blank = range.r2.is_some() && range.c2.is_some();
    let r1 = range.r1.unwrap_or(used_r1);
    let c1 = range.c1.unwrap_or(used_c1);
    let mut r2 = range.r2.unwrap_or(used_r2);
    let mut c2 = range.c2.unwrap_or(used_c2);
    if keep_blank && (r2 - r1 + 1) as u64 * (c2 - c1 + 1) as u64 > MAX_PADDED {
        r2 = r2.min(used_r2);
        c2 = c2.min(used_c2);
    }
    if r1 > r2 || c1 > c2 {
        return Ok(Vec::new());
    }
    let width = (c2 - c1 + 1) as usize;
    let mut rows: BTreeMap<u32, Vec<Option<Cell>>> = BTreeMap::new();
    if keep_blank {
        for r in r1..=r2 {
            rows.insert(r, vec![None; width]);
        }
    }
    for (&(r, c), cell) in cells.range((r1, 1)..=(r2, MAX_COL)) {
        if c < c1 || c > c2 {
            continue;
        }
        if !rows.contains_key(&r) && (rows.len() as u64 + 1) * width as u64 > MAX_GRID {
            return Err(fail(
                fname,
                "tooLarge",
                format!("{}: the selected part of '{}' is too large.", fname, name),
            ));
        }
        rows.entry(r).or_insert_with(|| vec![None; width])[(c - c1) as usize] = Some(cell.clone());
    }
    Ok(rows
        .into_iter()
        .map(|(r, cells)| ((r - r1 + 1) as usize, cells))
        .collect())
}

// ---------------------------------------------------------------------------
// Writing from tableio
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    InPlace,
    OverwriteSheet,
    Append,
    ReplaceFile,
}

struct WriteOpts {
    sheet: Option<Value>,
    range: Option<Range>,
    mode: Mode,
    names: bool,
    row_names: bool,
}

fn bool_opt(v: &Value, fname: &str, name: &str) -> R<bool> {
    if v.is_text() {
        return match v.to_str().unwrap_or_default().to_lowercase().as_str() {
            "true" | "on" => Ok(true),
            "false" | "off" => Ok(false),
            _ => Err(fail(
                fname,
                "invalidOption",
                format!("{}: '{}' must be true or false.", fname, name),
            )),
        };
    }
    match (v, v.scalar_f64()) {
        (Value::Num(_) | Value::Bool(_), Some(x)) if !x.is_nan() => Ok(x != 0.0),
        _ => Err(fail(
            fname,
            "invalidOption",
            format!("{}: '{}' must be true or false.", fname, name),
        )),
    }
}

fn write_opts(a: &[Value], fname: &str, table: bool) -> R<WriteOpts> {
    if !a.len().is_multiple_of(2) {
        return Err(fail(
            fname,
            "invalidNameValue",
            format!("{}: name-value arguments must come in pairs.", fname),
        ));
    }
    let mut o = WriteOpts {
        sheet: None,
        range: None,
        mode: Mode::InPlace,
        names: table,
        row_names: false,
    };
    for p in a.chunks(2) {
        let name = match p[0].to_str() {
            Some(s) if p[0].is_text() => s,
            _ => {
                return Err(fail(
                    fname,
                    "invalidNameValue",
                    format!("{}: expected an option name (text).", fname),
                ));
            }
        };
        match name.to_lowercase().as_str() {
            "sheet" => o.sheet = Some(p[1].clone()),
            "range" => o.range = Some(parse_range(&p[1], fname)?),
            "writemode" => {
                let m = p[1].to_str().filter(|_| p[1].is_text()).unwrap_or_default();
                o.mode = match m.to_lowercase().as_str() {
                    "inplace" => Mode::InPlace,
                    "overwritesheet" => Mode::OverwriteSheet,
                    "append" => Mode::Append,
                    "replacefile" => Mode::ReplaceFile,
                    _ => {
                        return Err(fail(
                            fname,
                            "invalidWriteMode",
                            format!(
                                "{}: WriteMode '{}' must be 'inplace', 'overwritesheet', 'append' or 'replacefile' for spreadsheet files.",
                                fname, m
                            ),
                        ));
                    }
                };
            }
            "writevariablenames" if table => {
                o.names = bool_opt(&p[1], fname, "WriteVariableNames")?
            }
            "writerownames" if table => o.row_names = bool_opt(&p[1], fname, "WriteRowNames")?,
            "filetype" => {}
            // These only change how a file looks in Excel, not its values.
            "autofitwidth" | "preserveformat" => {
                bool_opt(&p[1], fname, &name)?;
            }
            "useexcel" => {
                if bool_opt(&p[1], fname, "UseExcel")? {
                    return Err(fail(
                        fname,
                        "unsupportedOption",
                        format!(
                            "{}: 'UseExcel' true is not available in the Rust engine.",
                            fname
                        ),
                    ));
                }
            }
            _ => {
                return Err(fail(
                    fname,
                    "unsupportedOption",
                    format!(
                        "{}: option '{}' does not apply to spreadsheet files or is not supported yet.",
                        fname, name
                    ),
                ));
            }
        }
    }
    if o.mode == Mode::Append && o.range.is_some() {
        return Err(fail(
            fname,
            "invalidOption",
            format!(
                "{}: 'Range' cannot be combined with WriteMode 'append'.",
                fname
            ),
        ));
    }
    Ok(o)
}

fn valid_sheet_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars().count() <= 31
        && !s.contains(['[', ']', ':', '*', '?', '/', '\\'])
        && !s.starts_with('\'')
        && !s.ends_with('\'')
}

/// Index of the sheet to write, created when it does not exist yet.
fn target_sheet(wb: &mut Workbook, sel: Option<&Value>, fname: &str) -> R<usize> {
    let fresh = |wb: &Workbook| {
        let mut k = wb.sheets.len() + 1;
        while wb.sheets.iter().any(|s| s.name == format!("Sheet{}", k)) {
            k += 1;
        }
        format!("Sheet{}", k)
    };
    let Some(v) = sel else {
        if wb.sheets.is_empty() {
            wb.sheets.push(Sheet {
                name: "Sheet1".into(),
                cells: BTreeMap::new(),
            });
        }
        return Ok(0);
    };
    if v.is_text() {
        let want = v.to_str().unwrap_or_default();
        if let Some(k) = wb
            .sheets
            .iter()
            .position(|s| s.name.eq_ignore_ascii_case(&want))
        {
            return Ok(k);
        }
        if !valid_sheet_name(&want) {
            return Err(fail(
                fname,
                "invalidSheet",
                format!(
                    "{}: '{}' is not a valid sheet name (1 to 31 characters, none of []:*?/\\).",
                    fname, want
                ),
            ));
        }
        wb.sheets.push(Sheet {
            name: want,
            cells: BTreeMap::new(),
        });
        return Ok(wb.sheets.len() - 1);
    }
    match v.scalar_f64().filter(|_| v.is_numeric()) {
        Some(x) if x >= 1.0 && x.fract() == 0.0 && x <= 4096.0 => {
            let n = x as usize;
            while wb.sheets.len() < n {
                let name = fresh(wb);
                wb.sheets.push(Sheet {
                    name,
                    cells: BTreeMap::new(),
                });
            }
            Ok(n - 1)
        }
        _ => Err(fail(
            fname,
            "invalidSheet",
            format!(
                "{}: 'Sheet' must be a sheet name or a positive integer.",
                fname
            ),
        )),
    }
}

/// Merge rows into the file. `header` leading rows are left out when appending
/// to a sheet that already has data.
fn store(
    it: &Interp,
    fname: &str,
    name: &str,
    rows: &[Vec<Option<Cell>>],
    header: usize,
    o: &WriteOpts,
) -> R<()> {
    let path = resolve_path(it, name);
    let existing = std::fs::metadata(&path).is_ok_and(|m| m.len() > 0);
    let mut wb = if o.mode == Mode::ReplaceFile || !existing {
        Workbook::default()
    } else {
        let bytes = std::fs::read(&path).map_err(|e| {
            fail(
                fname,
                "FileOpenError",
                format!("{}: unable to read '{}': {}", fname, name, e),
            )
        })?;
        read_workbook(&bytes).map_err(|m| {
            fail(
                fname,
                "invalidWorkbook",
                format!(
                    "{}: {} ('{}'); use WriteMode 'replacefile' to overwrite it.",
                    fname, m, name
                ),
            )
        })?
    };
    let k = target_sheet(&mut wb, o.sheet.as_ref(), fname)?;
    let sheet = &mut wb.sheets[k];
    if o.mode == Mode::OverwriteSheet {
        sheet.cells.clear();
    }
    let r = o.range.unwrap_or_default();
    let (start_r, start_c, skip) = if o.mode == Mode::Append {
        match sheet.cells.keys().map(|k| k.0).max() {
            Some(last) => (last + 1, 1, header),
            None => (1, 1, 0),
        }
    } else {
        (r.r1.unwrap_or(1), r.c1.unwrap_or(1), 0)
    };
    let (end_r, end_c) = (r.r2.unwrap_or(MAX_ROW), r.c2.unwrap_or(MAX_COL));
    for (i, row) in rows.iter().skip(skip).enumerate() {
        let rr = start_r as u64 + i as u64;
        if rr > end_r as u64 {
            break;
        }
        for (j, cell) in row.iter().enumerate() {
            let cc = start_c as u64 + j as u64;
            if cc > end_c as u64 {
                break;
            }
            if rr > MAX_ROW as u64 || cc > MAX_COL as u64 {
                return Err(fail(
                    fname,
                    "tooLarge",
                    format!("{}: the data does not fit in a worksheet.", fname),
                ));
            }
            let key = (rr as u32, cc as u32);
            match cell {
                Some(c) => {
                    sheet.cells.insert(key, c.clone());
                }
                None => {
                    sheet.cells.remove(&key);
                }
            }
        }
    }
    let bytes = write_workbook(&wb)
        .map_err(|m| fail(fname, "FileOpenError", format!("{}: {}", fname, m)))?;
    std::fs::write(&path, bytes).map_err(|e| {
        fail(
            fname,
            "FileOpenError",
            format!("{}: unable to write file '{}': {}", fname, name, e),
        )
    })
}

fn num_cell(n: &Num, i: usize) -> Option<Cell> {
    let x = n.re[i];
    if x.is_nan() {
        None
    } else if x.is_infinite() {
        Some(Cell::Text(if x > 0.0 { "Inf" } else { "-Inf" }.to_string()))
    } else if n.class == NumClass::Single {
        // The shortest text of the f32, so 0.1 does not become 0.10000000149.
        let s = format!("{}", x as f32);
        Some(Cell::Num(s.parse().unwrap_or(x)))
    } else {
        Some(Cell::Num(x))
    }
}

fn text_cell(s: &str) -> Option<Cell> {
    (!s.is_empty()).then(|| Cell::Text(s.to_string()))
}

fn scalar_cell(it: &mut Interp, e: &Value, fname: &str) -> R<Option<Cell>> {
    let bad = || {
        fail(
            fname,
            "unsupportedType",
            format!(
                "{}: cells must contain scalar numbers, logicals, text or missing values (found {}).",
                fname,
                e.class_name()
            ),
        )
    };
    match e {
        Value::Num(n) if n.is_complex() => Err(complex_error(fname)),
        Value::Num(n) if n.re.len() == 1 => Ok(num_cell(n, 0)),
        Value::Num(n) if n.re.is_empty() => Ok(None),
        Value::Bool(b) if b.data.len() == 1 => Ok(Some(Cell::Bool(b.data[0]))),
        Value::Char(_) if e.is_string_row() || e.is_empty() => {
            Ok(text_cell(&e.to_str().unwrap_or_default()))
        }
        Value::Str(s) if s.data.len() == 1 => Ok(s.data[0].as_deref().and_then(text_cell)),
        _ if is_native(e) && e.numel() == 1 => {
            let mut col = native_cells(it, e, 1, fname)?;
            Ok(col.pop().and_then(|mut c| c.pop()).flatten())
        }
        _ => Err(bad()),
    }
}

fn complex_error(fname: &str) -> Box<indy_value::Error> {
    fail(
        fname,
        "unsupportedType",
        format!(
            "{}: complex values cannot be written to a spreadsheet.",
            fname
        ),
    )
}

/// Columns of a datetime (real dates) or other native class (their text).
fn native_cells(it: &mut Interp, v: &Value, h: usize, fname: &str) -> R<Vec<Vec<Option<Cell>>>> {
    let class = v.class_name();
    if class == "table" {
        return Err(fail(
            fname,
            "unsupportedType",
            format!("{}: variables of type table are not supported.", fname),
        ));
    }
    if class == "datetime" {
        let r = it.call_named("exceltime", vec![v.clone()], 1)?;
        let serial = r.into_iter().next().unwrap_or_default();
        let x = serial.to_f64_vec().unwrap_or_default();
        return Ok(x
            .chunks(h.max(1))
            .map(|c| {
                c.iter()
                    .map(|&d| (!d.is_nan()).then_some(Cell::Date(d)))
                    .collect()
            })
            .collect());
    }
    let r = it.call_named("cellstr", vec![v.clone()], 1)?;
    let cs = r.into_iter().next().unwrap_or_default();
    value_cells(it, &cs, h, fname)
}

/// Columns of one value laid out as `h` rows.
fn value_cells(it: &mut Interp, v: &Value, h: usize, fname: &str) -> R<Vec<Vec<Option<Cell>>>> {
    let d = v.dims();
    let w: usize = d[1..].iter().product();
    let columns = |f: &mut dyn FnMut(usize) -> Option<Cell>| -> Vec<Vec<Option<Cell>>> {
        (0..w)
            .map(|j| (0..h).map(|i| f(i + j * h)).collect())
            .collect()
    };
    match v {
        Value::Num(n) => {
            if n.is_complex() {
                return Err(complex_error(fname));
            }
            Ok(columns(&mut |k| num_cell(n, k)))
        }
        Value::Bool(b) => Ok(columns(&mut |k| Some(Cell::Bool(b.data[k])))),
        Value::Char(_) => {
            let rows = if h == 0 {
                Vec::new()
            } else {
                v.char_rows().unwrap_or_default()
            };
            Ok(vec![rows.iter().map(|s| text_cell(s)).collect()])
        }
        Value::Str(s) => Ok(columns(&mut |k| s.data[k].as_deref().and_then(text_cell))),
        Value::Cell(c) => {
            let mut out = Vec::with_capacity(w);
            for j in 0..w {
                let mut col = Vec::with_capacity(h);
                for i in 0..h {
                    col.push(scalar_cell(it, &c.data[i + j * h], fname)?);
                }
                out.push(col);
            }
            Ok(out)
        }
        _ if is_native(v) => native_cells(it, v, h, fname),
        _ => Err(fail(
            fname,
            "unsupportedType",
            format!(
                "{}: variables of type {} are not supported.",
                fname,
                v.class_name()
            ),
        )),
    }
}

fn to_rows(cols: Vec<Vec<Option<Cell>>>, h: usize) -> Vec<Vec<Option<Cell>>> {
    let mut rows: Vec<Vec<Option<Cell>>> = (0..h).map(|_| Vec::with_capacity(cols.len())).collect();
    for col in cols {
        for (i, cell) in col.into_iter().enumerate().take(h) {
            rows[i].push(cell);
        }
    }
    rows
}

pub fn writetable(it: &mut Interp, t: &Tab, name: &str, a: &[Value]) -> R<Vec<Value>> {
    let fname = "writetable";
    let o = write_opts(a, fname, true)?;
    let h = t.h;
    let mut header = Vec::new();
    let mut cols: Vec<Vec<Option<Cell>>> = Vec::new();
    if o.row_names && !t.rows.is_empty() {
        header.push(text_cell(&t.dimnames[0]));
        cols.push(t.rows.iter().map(|s| text_cell(s)).collect());
    }
    for (k, v) in t.vars.iter().enumerate() {
        let vc = value_cells(it, v, h, fname)?;
        if vc.len() == 1 {
            header.push(text_cell(&t.names[k]));
        } else {
            for j in 0..vc.len() {
                header.push(text_cell(&format!("{}_{}", t.names[k], j + 1)));
            }
        }
        cols.extend(vc);
    }
    let mut rows = Vec::with_capacity(h + 1);
    let top = if o.names { 1 } else { 0 };
    if o.names {
        rows.push(header);
    }
    rows.extend(to_rows(cols, h));
    store(it, fname, name, &rows, top, &o)?;
    Ok(vec![])
}

pub fn writematrix(it: &mut Interp, m: &Value, name: &str, a: &[Value]) -> R<Vec<Value>> {
    let fname = "writematrix";
    let o = write_opts(a, fname, false)?;
    let d = m.dims();
    if d.len() > 2 {
        return Err(fail(
            fname,
            "invalidInput",
            "writematrix: the input must be a two-dimensional array.",
        ));
    }
    if !matches!(
        m,
        Value::Num(_) | Value::Bool(_) | Value::Str(_) | Value::Char(_)
    ) {
        return Err(fail(
            fname,
            "invalidInput",
            format!(
                "writematrix: unsupported input of class {}.",
                m.class_name()
            ),
        ));
    }
    let cols = value_cells(it, m, d[0], fname)?;
    store(it, fname, name, &to_rows(cols, d[0]), 0, &o)?;
    Ok(vec![])
}

pub fn writecell(it: &mut Interp, c: &Value, name: &str, a: &[Value]) -> R<Vec<Value>> {
    let fname = "writecell";
    let o = write_opts(a, fname, false)?;
    let h = c.dims()[0];
    let cols = value_cells(it, c, h, fname)?;
    store(it, fname, name, &to_rows(cols, h), 0, &o)?;
    Ok(vec![])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Workbook {
        let mut a = Sheet {
            name: "Veri & <x>".into(),
            cells: BTreeMap::new(),
        };
        a.cells
            .insert((1, 1), Cell::Text("başlık _x0041_ \u{1}".into()));
        a.cells.insert((1, 3), Cell::Num(0.1));
        a.cells.insert((2, 2), Cell::Bool(true));
        a.cells.insert((3, 28), Cell::Date(45000.5));
        a.cells.insert((4, 1), Cell::Text("  boşluk  ".into()));
        let b = Sheet {
            name: "Bos".into(),
            cells: BTreeMap::new(),
        };
        Workbook { sheets: vec![a, b] }
    }

    #[test]
    fn workbooks_round_trip() {
        let wb = sample();
        let bytes = write_workbook(&wb).unwrap();
        let back = read_workbook(&bytes).unwrap();
        assert_eq!(back.sheets.len(), 2);
        assert_eq!(back.sheets[0].name, wb.sheets[0].name);
        assert_eq!(back.sheets[0].cells, wb.sheets[0].cells);
        assert!(back.sheets[1].cells.is_empty());
    }

    #[test]
    fn corrupt_workbooks_are_errors_and_never_panic() {
        let bytes = write_workbook(&sample()).unwrap();
        for n in 0..bytes.len() {
            assert!(read_workbook(&bytes[..n]).is_err());
            for v in [0x00, 0xFF, b'<', b'"'] {
                let mut c = bytes.clone();
                c[n] = v;
                let _ = read_workbook(&c);
            }
        }
    }

    #[test]
    fn hostile_xml_does_not_panic() {
        for s in [
            "<",
            "<a",
            "<a b=\"",
            "</",
            "<!--",
            "<![CDATA[",
            "<?",
            "<c r=\"ZZZZZZZZZZ9\">",
            "<row r=\"99999999999\">",
            "é<é é=\"é\"/>é&é;&#xFFFFFFFF;&#1",
        ] {
            let mut sheet = Sheet::default();
            let ctx = SheetContext {
                strings: &[],
                dates: &[],
                date_offset: 0.0,
            };
            let _ = parse_sheet(s, &mut sheet, &ctx);
            let _ = shared_strings(s);
            let _ = date_styles(s);
            let _ = relationships(s);
        }
        assert_eq!(decode_entities("a&amp;b&#65;&#x42;&zz;&"), "a&bAB&zz;&");
    }

    #[test]
    fn references_parse_and_print() {
        assert_eq!(parse_ref("B2"), Some((Some(2), Some(2))));
        assert_eq!(parse_ref("XFD1048576"), Some((Some(1048576), Some(16384))));
        assert_eq!(parse_ref("XFE1"), None);
        assert_eq!(parse_ref("A0"), None);
        assert_eq!(parse_ref("1A"), None);
        assert_eq!(col_name(1), "A");
        assert_eq!(col_name(27), "AA");
        assert_eq!(col_name(16384), "XFD");
    }

    #[test]
    fn date_formats_are_told_from_number_formats() {
        for code in ["yyyy-mm-dd", "h:mm", "dd/mm/yy", "[$-409]d-mmm"] {
            assert!(is_date_format(code), "{}", code);
        }
        for code in [
            "General",
            "0.00",
            "#,##0",
            "0.00E+00",
            "\"days\" 0",
            "[Red]0",
        ] {
            assert!(!is_date_format(code), "{}", code);
        }
    }
}
