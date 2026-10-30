//! Number formatting: a C-compatible printf engine, `num2str`, `mat2str`
//! and MATLAB-like display (`x = 3`). Display layout is approximate.

use crate::{Error, Num, Res, Value};

#[derive(Default, Clone, Debug)]
struct Spec {
    minus: bool,
    plus: bool,
    space: bool,
    zero: bool,
    hash: bool,
    width: Option<usize>,
    prec: Option<usize>,
    conv: char,
}

fn nonfinite(x: f64) -> Option<String> {
    if x.is_nan() {
        Some("NaN".into())
    } else if x.is_infinite() {
        Some(if x > 0.0 { "Inf".into() } else { "-Inf".into() })
    } else {
        None
    }
}

/// `%.{p}e` with a C exponent (`e+05`).
pub fn fmt_e(x: f64, p: usize, upper: bool) -> String {
    let s = format!("{:.*e}", p, x);
    let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let e: i32 = exp.parse().unwrap_or(0);
    let sign = if e < 0 { '-' } else { '+' };
    let r = format!("{}e{}{:02}", mant, sign, e.abs());
    if upper { r.to_uppercase() } else { r }
}

pub fn fmt_f(x: f64, p: usize) -> String {
    format!("{:.*}", p, x)
}

fn strip_zeros(s: &str) -> String {
    if let Some((m, e)) = s.split_once(['e', 'E']) {
        let sep = if s.contains('E') { 'E' } else { 'e' };
        let m = if m.contains('.') {
            m.trim_end_matches('0').trim_end_matches('.')
        } else {
            m
        };
        format!("{}{}{}", m, sep, e)
    } else if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s.to_string()
    }
}

pub fn fmt_g(x: f64, prec: usize, alt: bool, upper: bool) -> String {
    let p = if prec == 0 { 1 } else { prec };
    if x == 0.0 {
        let s = if alt {
            format!("{:.*}", p - 1, 0.0)
        } else {
            "0".to_string()
        };
        return if x.is_sign_negative() {
            format!("-{}", s)
        } else {
            s
        };
    }
    let e = fmt_e(x, p - 1, false);
    let exp: i32 = e
        .split_once('e')
        .map(|(_, x)| x.parse().unwrap_or(0))
        .unwrap_or(0);
    let s = if exp < -4 || exp >= p as i32 {
        fmt_e(x, p - 1, upper)
    } else {
        fmt_f(x, (p as i32 - 1 - exp).max(0) as usize)
    };
    if alt { s } else { strip_zeros(&s) }
}

/// Shortest readable form used in messages (`%g` with up to 15 digits).
pub fn short_g(x: f64) -> String {
    if let Some(s) = nonfinite(x) {
        return s;
    }
    if x.fract() == 0.0 && x.abs() < 1e15 {
        return format!("{}", x as i64);
    }
    fmt_g(x, 15, false, false)
}

fn pad(s: String, spec: &Spec, numeric: bool) -> String {
    let w = spec.width.unwrap_or(0);
    let len = s.chars().count();
    if len >= w {
        return s;
    }
    let fill = w - len;
    if spec.minus {
        format!("{}{}", s, " ".repeat(fill))
    } else if spec.zero && numeric && !s.contains("Inf") && !s.contains("NaN") {
        let (sign, rest) = if s.starts_with(['-', '+', ' ']) {
            s.split_at(1)
        } else {
            ("", s.as_str())
        };
        format!("{}{}{}", sign, "0".repeat(fill), rest)
    } else {
        format!("{}{}", " ".repeat(fill), s)
    }
}

fn sign_prefix(s: String, x: f64, spec: &Spec) -> String {
    if x >= 0.0 && !s.starts_with('-') {
        if spec.plus {
            return format!("+{}", s);
        }
        if spec.space {
            return format!(" {}", s);
        }
    }
    s
}

fn format_number(x: f64, spec: &Spec) -> String {
    if let Some(s) = nonfinite(x) {
        let s = sign_prefix(s, x, spec);
        return pad(s, spec, false);
    }
    let mut conv = spec.conv;
    if matches!(conv, 'd' | 'i' | 'u' | 'x' | 'X' | 'o' | 'c') && x.fract() != 0.0 {
        // MATLAB switches to %e for non-integers under integer conversions.
        let mut s2 = spec.clone();
        s2.conv = 'e';
        if s2.prec.is_none() {
            s2.prec = Some(6);
        }
        conv = 'e';
        let _ = conv;
        return format_number(x, &s2);
    }
    let s = match conv {
        'd' | 'i' | 'u' => {
            let mut s = format!("{}", x.abs() as u128);
            if let Some(p) = spec.prec
                && s.len() < p
            {
                s = format!("{}{}", "0".repeat(p - s.len()), s);
            }
            if x < 0.0 { format!("-{}", s) } else { s }
        }
        'x' => format!("{:x}", x as i128),
        'X' => format!("{:X}", x as i128),
        'o' => format!("{:o}", x as i128),
        'f' | 'F' => fmt_f(x, spec.prec.unwrap_or(6)),
        'e' | 'E' => fmt_e(x, spec.prec.unwrap_or(6), conv == 'E'),
        'g' | 'G' => fmt_g(x, spec.prec.unwrap_or(6), spec.hash, conv == 'G'),
        _ => short_g(x),
    };
    let s = sign_prefix(s, x, spec);
    pad(s, spec, true)
}

/// Process backslash escapes of a printf format (`\n`, `\t`, `\\`, ...).
pub fn escapes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('a') => out.push('\x07'),
            Some('b') => out.push('\x08'),
            Some('f') => out.push('\x0c'),
            Some('v') => out.push('\x0b'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('\'') => out.push('\''),
            Some('x') => {
                let mut h = String::new();
                while let Some(d) = it.peek() {
                    if d.is_ascii_hexdigit() && h.len() < 4 {
                        h.push(*d);
                        it.next();
                    } else {
                        break;
                    }
                }
                if let Some(ch) = u32::from_str_radix(&h, 16).ok().and_then(char::from_u32) {
                    out.push(ch);
                }
            }
            Some(d) if d.is_digit(8) => {
                let mut o = String::from(d);
                while let Some(d) = it.peek() {
                    if d.is_digit(8) && o.len() < 3 {
                        o.push(*d);
                        it.next();
                    } else {
                        break;
                    }
                }
                if let Some(ch) = u32::from_str_radix(&o, 8).ok().and_then(char::from_u32) {
                    out.push(ch);
                }
            }
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[derive(Clone, Debug)]
enum Item {
    N(f64),
    S(String),
}

enum Piece {
    Lit(String),
    Conv(Spec, bool, bool),
}

fn parse_format(f: &str) -> Vec<Piece> {
    let chars: Vec<char> = f.chars().collect();
    let mut out = Vec::new();
    let mut lit = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != '%' {
            lit.push(c);
            i += 1;
            continue;
        }
        if i + 1 < chars.len() && chars[i + 1] == '%' {
            lit.push('%');
            i += 2;
            continue;
        }
        let start = i;
        i += 1;
        let mut spec = Spec::default();
        while i < chars.len() {
            match chars[i] {
                '-' => spec.minus = true,
                '+' => spec.plus = true,
                ' ' => spec.space = true,
                '0' => spec.zero = true,
                '#' => spec.hash = true,
                _ => break,
            }
            i += 1;
        }
        let mut star_w = false;
        let mut star_p = false;
        if i < chars.len() && chars[i] == '*' {
            star_w = true;
            i += 1;
        } else {
            let mut w = String::new();
            while i < chars.len() && chars[i].is_ascii_digit() {
                w.push(chars[i]);
                i += 1;
            }
            if !w.is_empty() {
                spec.width = w.parse().ok();
            }
        }
        if i < chars.len() && chars[i] == '.' {
            i += 1;
            if i < chars.len() && chars[i] == '*' {
                star_p = true;
                i += 1;
            } else {
                let mut p = String::new();
                while i < chars.len() && chars[i].is_ascii_digit() {
                    p.push(chars[i]);
                    i += 1;
                }
                spec.prec = Some(p.parse().unwrap_or(0));
            }
        }
        // Length modifiers are accepted and ignored.
        while i < chars.len() && matches!(chars[i], 'l' | 'h' | 'L' | 'q' | 'j' | 'z' | 't') {
            i += 1;
        }
        if i < chars.len() && "diouxXfFeEgGcsaA".contains(chars[i]) {
            spec.conv = chars[i];
            i += 1;
            if !lit.is_empty() {
                out.push(Piece::Lit(std::mem::take(&mut lit)));
            }
            out.push(Piece::Conv(spec, star_w, star_p));
        } else {
            lit.extend(&chars[start..i]);
        }
    }
    if !lit.is_empty() {
        out.push(Piece::Lit(lit));
    }
    out
}

fn items_of(args: &[Value]) -> Res<Vec<Item>> {
    let mut items = Vec::new();
    for a in args {
        match a {
            Value::Char(c) => items.push(Item::S(
                // Column-major order, like MATLAB's flattening.
                c.data.iter().collect(),
            )),
            Value::Num(_) | Value::Bool(_) => {
                for x in a.to_f64_vec().unwrap() {
                    items.push(Item::N(x));
                }
            }
            Value::Str(s) => {
                for e in &s.data {
                    items.push(Item::S(str_elem(e, false)));
                }
            }
            Value::Cell(c) => {
                if c.data.is_empty() {
                    continue;
                }
                return Err(Error::new(
                    "MATLAB:printf:invalidInputType",
                    "Only numeric, char or logical arguments are supported (got a cell).",
                ));
            }
            other => {
                return Err(Error::new(
                    "MATLAB:printf:invalidInputType",
                    format!(
                        "Only numeric, char or logical arguments are supported (got {}).",
                        other.class_name()
                    ),
                ));
            }
        }
    }
    Ok(items)
}

/// MATLAB `sprintf(fmt, args...)`; `fmt` has escapes already processed.
pub fn printf(fmt: &str, args: &[Value]) -> Res<String> {
    let pieces = parse_format(fmt);
    let mut items = items_of(args)?;
    items.reverse(); // pop from the back
    let nconv = pieces
        .iter()
        .filter(|p| matches!(p, Piece::Conv(..)))
        .count();
    let mut out = String::new();
    if nconv == 0 {
        for p in &pieces {
            if let Piece::Lit(s) = p {
                out.push_str(s);
            }
        }
        return Ok(out);
    }
    let had_args = !items.is_empty();
    let mut first = true;
    loop {
        for p in &pieces {
            match p {
                Piece::Lit(s) => out.push_str(s),
                Piece::Conv(spec, sw, sp) => {
                    if items.is_empty() && (had_args || !first) {
                        return Ok(out);
                    }
                    let mut spec = spec.clone();
                    if *sw && let Some(Item::N(w)) = items.pop() {
                        if w < 0.0 {
                            spec.minus = true;
                        }
                        spec.width = Some(w.abs() as usize);
                    }
                    if *sp && let Some(Item::N(p)) = items.pop() {
                        spec.prec = Some(p.max(0.0) as usize);
                    }
                    let Some(item) = items.pop() else {
                        // No data at all: conversions print nothing.
                        if matches!(spec.conv, 's' | 'c') || true {
                            out.push_str(&pad(String::new(), &spec, false));
                        }
                        continue;
                    };
                    match (spec.conv, item) {
                        ('s', Item::S(s)) => {
                            let s = match spec.prec {
                                Some(p) => s.chars().take(p).collect(),
                                None => s,
                            };
                            out.push_str(&pad(s, &spec, false));
                        }
                        ('c', Item::S(s)) => out.push_str(&pad(s, &spec, false)),
                        ('s' | 'c', Item::N(x)) => {
                            let s = if x.fract() == 0.0 && (0.0..1114112.0).contains(&x) {
                                char::from_u32(x as u32)
                                    .map(String::from)
                                    .unwrap_or_default()
                            } else if spec.conv == 's' {
                                short_g(x)
                            } else {
                                format_number(
                                    x,
                                    &Spec {
                                        conv: 'f',
                                        ..spec.clone()
                                    },
                                )
                            };
                            out.push_str(&pad(s, &spec, false));
                        }
                        (_, Item::S(s)) => {
                            // Numeric conversion meets text: MATLAB prints the
                            // text as is when it cannot be read as numbers.
                            let mut codes: Vec<Item> =
                                s.chars().map(|c| Item::N(c as u32 as f64)).collect();
                            if codes.is_empty() {
                                continue;
                            }
                            let firstc = codes.remove(0);
                            for c in codes.into_iter().rev() {
                                items.push(c);
                            }
                            if let Item::N(x) = firstc {
                                out.push_str(&format_number(x, &spec));
                            }
                        }
                        (_, Item::N(x)) => out.push_str(&format_number(x, &spec)),
                    }
                }
            }
        }
        first = false;
        if items.is_empty() {
            break;
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// num2str / mat2str / display
// ---------------------------------------------------------------------------

fn is_intlike(v: &[f64]) -> bool {
    v.iter().all(|x| x.fract() == 0.0 || !x.is_finite())
}

fn complex_str(re: f64, im: f64, f: &dyn Fn(f64) -> String) -> String {
    let sign = if im < 0.0 || (im == 0.0 && im.is_sign_negative()) {
        '-'
    } else {
        '+'
    };
    format!("{}{}{}i", f(re), sign, f(im.abs()))
}

/// `num2str(x)` for numeric/logical input (2-D).
pub fn num2str(n: &Num) -> String {
    let d = &n.dims;
    if n.re.is_empty() {
        return String::new();
    }
    let rows = d[0];
    let cols = n.re.len() / rows.max(1);
    let all_int = is_intlike(&n.re) && n.im.as_ref().is_none_or(|im| is_intlike(im));
    let cell = |i: usize| -> String {
        if all_int {
            let f = |x: f64| nonfinite(x).unwrap_or_else(|| format!("{}", x as i128));
            match &n.im {
                Some(im) => complex_str(n.re[i], im[i], &f),
                None => f(n.re[i]),
            }
        } else {
            let maxabs =
                n.re.iter()
                    .filter(|x| x.is_finite())
                    .fold(0.0f64, |a, b| a.max(b.abs()));
            let digits = if maxabs > 0.0 {
                maxabs.log10().floor() as i64
            } else {
                0
            };
            let prec = (digits + 5).clamp(5, 16) as usize;
            let prec = if n.re.len() == 1 { prec.max(5) } else { prec };
            let f = |x: f64| nonfinite(x).unwrap_or_else(|| fmt_g(x, prec, false, false));
            match &n.im {
                Some(im) => complex_str(n.re[i], im[i], &f),
                None => f(n.re[i]),
            }
        }
    };
    if n.re.len() == 1 {
        return cell(0);
    }
    let strs: Vec<String> = (0..n.re.len()).map(cell).collect();
    let w = strs.iter().map(|s| s.len()).max().unwrap_or(0);
    let sep = 2;
    let mut lines = Vec::with_capacity(rows);
    for i in 0..rows {
        let mut line = String::new();
        for j in 0..cols {
            let s = &strs[i + j * rows];
            let width = if j == 0 { w } else { w + sep };
            line.push_str(&" ".repeat(width - s.len()));
            line.push_str(s);
        }
        lines.push(line);
    }
    // Remove common leading blanks (MATLAB trims the char matrix).
    let lead = lines
        .iter()
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|l| l[lead..].to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// `mat2str(x)` / `mat2str(x, n)`.
pub fn mat2str(v: &Value, prec: usize) -> Res<String> {
    let one = |x: f64| -> String {
        if let Some(s) = nonfinite(x) {
            return s;
        }
        fmt_g(x, prec, false, false)
    };
    let (dims, cellf): (Vec<usize>, Box<dyn Fn(usize) -> String>) = match v {
        Value::Num(n) => {
            let n = n.clone();
            (
                n.dims.clone(),
                Box::new(move |i| match &n.im {
                    Some(im) => complex_str(n.re[i], im[i], &one),
                    None => one(n.re[i]),
                }),
            )
        }
        Value::Bool(a) => {
            let a = a.clone();
            (
                a.dims.clone(),
                Box::new(move |i| {
                    if a.data[i] {
                        "true".into()
                    } else {
                        "false".into()
                    }
                }),
            )
        }
        Value::Str(a) => {
            if a.data.len() == 1 {
                return Ok(str_elem(&a.data[0], true));
            }
            let (r, c) = (a.dims[0], a.data.len() / a.dims[0].max(1));
            let mut s = String::from("[");
            for i in 0..r {
                if i > 0 {
                    s.push(';');
                }
                for j in 0..c {
                    if j > 0 {
                        s.push(' ');
                    }
                    s.push_str(&str_elem(&a.data[i + j * r], true));
                }
            }
            s.push(']');
            return Ok(s);
        }
        Value::Char(a) => {
            let rows = v.char_rows().unwrap();
            if a.dims[0] <= 1 {
                return Ok(format!(
                    "'{}'",
                    rows.first()
                        .cloned()
                        .unwrap_or_default()
                        .replace('\'', "''")
                ));
            }
            let parts: Vec<String> = rows
                .iter()
                .map(|r| format!("'{}'", r.replace('\'', "''")))
                .collect();
            return Ok(format!("[{}]", parts.join(";")));
        }
        _ => {
            return Err(Error::new(
                "MATLAB:mat2str:NonNumericInput",
                "Input must be a 2-D numeric, logical or char array.",
            ));
        }
    };
    if dims.len() > 2 {
        return Err(Error::new(
            "MATLAB:mat2str:TwoDInput",
            "Input matrix must be 2-D.",
        ));
    }
    let (r, c) = (dims[0], dims[1]);
    if r * c == 1 {
        return Ok(cellf(0));
    }
    let mut s = String::from("[");
    for i in 0..r {
        if i > 0 {
            s.push(';');
        }
        for j in 0..c {
            if j > 0 {
                s.push(' ');
            }
            s.push_str(&cellf(i + j * r));
        }
    }
    s.push(']');
    if r * c == 0 {
        return Ok(format!("zeros({},{})", r, c));
    }
    Ok(s)
}

/// One element in "format short".
fn short_elem(x: f64) -> String {
    if let Some(s) = nonfinite(x) {
        return s;
    }
    if x.fract() == 0.0 && x.abs() < 1e10 {
        return format!("{}", x as i64);
    }
    let a = x.abs();
    if (1e-3..1e5).contains(&a) {
        fmt_f(x, 4)
    } else {
        fmt_e(x, 4, false)
    }
}

fn num_matrix_text(n: &Num) -> String {
    let rows = n.dims[0];
    let cols = n.re.len().checked_div(rows).unwrap_or(0);
    let all_int = is_intlike(&n.re) && n.im.is_none();
    let strs: Vec<String> = (0..n.re.len())
        .map(|i| {
            let f = |x: f64| {
                if all_int {
                    nonfinite(x).unwrap_or_else(|| format!("{}", x as i64))
                } else {
                    short_elem(x)
                }
            };
            match &n.im {
                Some(im) => complex_str(n.re[i], im[i], &f),
                None => f(n.re[i]),
            }
        })
        .collect();
    let w = strs.iter().map(|s| s.len()).max().unwrap_or(1) + 3;
    let mut out = String::new();
    for i in 0..rows {
        for j in 0..cols {
            let s = &strs[i + j * rows];
            out.push_str(&" ".repeat(w.saturating_sub(s.len())));
            out.push_str(s);
        }
        out.push('\n');
    }
    out
}

/// Text `disp(v)` prints (with trailing newline).
fn str_elem(s: &Option<String>, quoted: bool) -> String {
    match s {
        Some(t) if quoted => format!("\"{}\"", t),
        Some(t) => t.clone(),
        None => "<missing>".into(),
    }
}

fn str_matrix(a: &crate::Array<Option<String>>) -> String {
    let rows = a.dims[0];
    let cols = a.data.len().checked_div(rows).unwrap_or(0);
    let mut out = String::new();
    for i in 0..rows {
        out.push_str("   ");
        for j in 0..cols {
            out.push_str(&format!(" {}", str_elem(&a.data[i + j * rows], true)));
            if j + 1 < cols {
                out.push_str("   ");
            }
        }
        out.push('\n');
    }
    out
}

/// Rows of a cell array shown element by element; the body shared by
/// `disp(c)` and `c = ...` display.
fn cell_body(c: &crate::Array<Value>) -> String {
    let mut s = String::new();
    let rows = c.dims[0];
    let cols = c.data.len() / rows.max(1);
    for i in 0..rows {
        s.push_str("  ");
        for j in 0..cols {
            s.push_str(&format!("{{{}}}    ", inline_text(&c.data[i + j * rows])));
        }
        s.push('\n');
    }
    s
}

pub fn disp_text(v: &Value) -> String {
    match v {
        Value::Sparse(s) => crate::sparse::text(s),
        Value::Str(a) => {
            if a.data.len() == 1 {
                return format!("{}\n", str_elem(&a.data[0], false));
            }
            str_matrix(a)
        }
        Value::Num(n) => {
            if n.re.is_empty() {
                return String::new();
            }
            if n.re.len() == 1 {
                let s = match &n.im {
                    Some(im) => complex_str(n.re[0], im[0], &short_elem),
                    None => short_elem(n.re[0]),
                };
                return format!("{}\n", s);
            }
            if n.dims.len() > 2 {
                return format!("[{} {}]\n", crate::ops::dims_text(&n.dims), n.class.name());
            }
            num_matrix_text(n)
        }
        Value::Bool(a) => disp_text(&Value::Num(Value::Bool(a.clone()).to_num().unwrap())),
        Value::Char(_) => {
            let rows = v.char_rows().unwrap();
            let mut s = rows.join("\n");
            if !rows.is_empty() {
                s.push('\n');
            }
            s
        }
        Value::Cell(c) => {
            if c.data.is_empty() {
                return "{}\n".into();
            }
            cell_body(c)
        }
        Value::Struct(s) => {
            if s.elems.len() == 1 {
                let mut out = String::new();
                for (k, f) in s.fields.iter().enumerate() {
                    out.push_str(&format!("    {}: {}\n", f, inline_text(&s.elems[0][k])));
                }
                out
            } else {
                format!(
                    "  {} struct array with fields:\n{}",
                    crate::ops::dims_text(&s.dims),
                    s.fields
                        .iter()
                        .map(|f| format!("    {}\n", f))
                        .collect::<String>()
                )
            }
        }
        Value::Func(h) => format!("{}\n", handle_text(h)),
        Value::Object(o) => object_text(&o.class, &o.props),
        Value::HObject(o) => {
            let o = o.borrow();
            object_text(&o.class, &o.props)
        }
        Value::ObjArr(o) => obj_array_text(o),
    }
}

/// `  1x2 Point array with properties:` and the property names.
fn obj_array_text(o: &crate::ObjArray) -> String {
    let mut s = format!(
        "  {} {} array with properties:\n\n",
        crate::ops::dims_text(&o.arr.dims),
        o.class
    );
    let names: Vec<String> = match o.arr.data.first() {
        Some(Value::Object(e)) => e.props.iter().map(|p| p.0.clone()).collect(),
        Some(Value::HObject(e)) => e.borrow().props.iter().map(|p| p.0.clone()).collect(),
        _ => Vec::new(),
    };
    for n in names {
        s.push_str(&format!("    {}\n", n));
    }
    s
}

fn object_text(class: &str, props: &[(String, Value)]) -> String {
    let mut s = format!("  {} with properties:\n\n", class);
    for (k, v) in props {
        s.push_str(&format!("    {}: {}\n", k, inline_text(v)));
    }
    s
}

pub fn handle_text(h: &crate::Handle) -> String {
    match &h.anon {
        Some(a) => a.text.clone(),
        None => format!("@{}", h.name),
    }
}

/// Short one-line description used inside struct/cell display.
pub fn inline_text(v: &Value) -> String {
    match v {
        Value::Char(_) if v.is_string_row() => format!("'{}'", v.to_str().unwrap()),
        Value::Num(n) if n.re.len() == 1 => match &n.im {
            Some(im) => complex_str(n.re[0], im[0], &short_elem),
            None => short_elem(n.re[0]),
        },
        Value::Bool(a) if a.data.len() == 1 => (a.data[0] as u8).to_string(),
        Value::Num(n) if n.re.len() <= 10 && n.dims.len() == 2 && n.dims[0] == 1 => {
            let parts: Vec<String> = n.re.iter().map(|x| short_elem(*x)).collect();
            format!("[{}]", parts.join(" "))
        }
        Value::Func(h) => handle_text(h),
        Value::Str(a) if a.data.len() == 1 => str_elem(&a.data[0], true),
        _ => format!(
            "[{} {}]",
            crate::ops::dims_text(v.dims_ref()),
            v.class_name()
        ),
    }
}

/// Text for `name = value` display (statement without `;`).
pub fn display_text(name: &str, v: &Value) -> String {
    match v {
        Value::Str(a) if a.data.len() == 1 => {
            format!("{} = {}\n", name, str_elem(&a.data[0], true))
        }
        Value::Str(a) => format!(
            "{} =\n\n  {} string array\n\n{}\n",
            name,
            crate::ops::dims_text(&a.dims),
            str_matrix(a)
        ),
        Value::Num(n) if n.re.len() == 1 => format!("{} = {}", name, disp_text(v)),
        Value::Bool(a) if a.data.len() == 1 => format!("{} = {}", name, disp_text(v)),
        Value::Char(_) if v.is_string_row() => format!("{} = '{}'\n", name, v.to_str().unwrap()),
        Value::Func(h) => format!("{} =\n\n    {}\n\n", name, handle_text(h)),
        Value::Sparse(s) => format!("{} =\n\n{}\n", name, crate::sparse::text(s)),
        _ if v.is_empty() && !matches!(v, Value::Struct(_)) => {
            let d = v.dims();
            match v {
                Value::Cell(_) => format!("{} = {{}}({})\n", name, crate::ops::dims_text(&d)),
                Value::Char(_) => format!("{} = ''\n", name),
                _ => format!("{} = [](${})\n", name, crate::ops::dims_text(&d)).replace('$', ""),
            }
        }
        Value::Cell(c) => {
            format!("{} =\n\n{}\n", name, cell_body(c))
        }
        _ => format!("{} =\n\n{}\n", name, disp_text(v)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn c_formats() {
        assert_eq!(fmt_e(12345.678, 3, false), "1.235e+04");
        assert_eq!(fmt_g(0.0001, 6, false, false), "0.0001");
        assert_eq!(fmt_g(1e-5, 6, false, false), "1e-05");
        assert_eq!(fmt_g(123456789.0, 6, false, false), "1.23457e+08");
        assert_eq!(fmt_g(1.234567, 5, false, false), "1.2346");
        assert_eq!(fmt_g(100.0, 6, false, false), "100");
    }
    #[test]
    fn printf_cycles() {
        let s = printf("%d,", &[Value::row(vec![1.0, 2.0, 3.0])]).unwrap();
        assert_eq!(s, "1,2,3,");
        let s = printf("%s=%5.2f\n", &[Value::str("a"), Value::num(3.1426)]).unwrap();
        assert_eq!(s, "a= 3.14\n");
        assert_eq!(
            printf("%03d|%-4d|", &[Value::num(7.0), Value::num(5.0)]).unwrap(),
            "007|5   |"
        );
        assert_eq!(printf("no args\n", &[]).unwrap(), "no args\n");
    }
    #[test]
    fn num2str_basic() {
        assert_eq!(num2str(&Num::scalar(std::f64::consts::PI)), "3.1416");
        assert_eq!(num2str(&Num::scalar(42.0)), "42");
        assert_eq!(
            num2str(&Num::real(vec![1, 3], vec![1.0, 2.0, 3.0])),
            "1  2  3"
        );
    }
}
