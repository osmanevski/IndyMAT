//! MATLAB `string` arrays: construction, conversion and the string
//! methods. Functions in `string_methods` win dispatch when any argument
//! is a string (class methods precede path functions, as in MATLAB); the
//! same functions are also builtins for char/cellstr input.

use indy_value::format::{escapes, printf};
use indy_value::{Array, Num, Value, to_string_array};

use super::{arg_f64, arg_str, err, need, one};
use crate::{BuiltinFn, Interp, R};

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Char,
    Cell,
    Str,
}

struct Texts {
    kind: Kind,
    dims: Vec<usize>,
    items: Vec<Option<String>>,
}

fn texts(v: &Value, fname: &str) -> R<Texts> {
    match v {
        Value::Str(a) => Ok(Texts {
            kind: Kind::Str,
            dims: a.dims.clone(),
            items: a.data.clone(),
        }),
        Value::Cell(c) => {
            let mut items = Vec::with_capacity(c.data.len());
            for x in &c.data {
                match x {
                    Value::Char(_) | Value::Str(_) => items.push(x.to_str()),
                    _ => {
                        return Err(err(
                            "MATLAB:string:MustBeCharCellOrString",
                            format!("{}: first argument must be text.", fname),
                        ));
                    }
                }
            }
            Ok(Texts {
                kind: Kind::Cell,
                dims: c.dims.clone(),
                items,
            })
        }
        Value::Char(_) => Ok(Texts {
            kind: Kind::Char,
            dims: vec![1, 1],
            items: vec![v.to_str()],
        }),
        other => Err(err(
            "MATLAB:string:MustBeCharCellOrString",
            format!(
                "{}: argument must be text, not {}.",
                fname,
                other.class_name()
            ),
        )),
    }
}

fn rebuild(kind: Kind, dims: Vec<usize>, items: Vec<Option<String>>) -> Value {
    match kind {
        Kind::Str => Value::Str(Array::new(dims, items)),
        Kind::Cell => Value::Cell(Array::new(
            dims,
            items
                .iter()
                .map(|s| Value::str(s.as_deref().unwrap_or("")))
                .collect(),
        )),
        Kind::Char => Value::str(items.first().cloned().flatten().as_deref().unwrap_or("")),
    }
}

/// Patterns argument: text or text array -> list.
fn patterns(v: &Value, fname: &str) -> R<Vec<String>> {
    Ok(texts(v, fname)?
        .items
        .into_iter()
        .map(|s| s.unwrap_or_default())
        .collect())
}

/// `'IgnoreCase', tf` (the only option); anything else is an error.
fn ignore_case(a: &[Value], fname: &str) -> R<bool> {
    let mut ic = false;
    for w in a.chunks(2) {
        let name = w[0].to_str().unwrap_or_default();
        let Some(v) = w.get(1).filter(|_| name.eq_ignore_ascii_case("IgnoreCase")) else {
            return Err(err(
                "MATLAB:InputParser:UnmatchedParameter",
                format!("{}: '{}' is not a recognized parameter.", fname, name),
            ));
        };
        ic = v.scalar_f64().is_some_and(|x| x != 0.0);
    }
    Ok(ic)
}

pub fn register(it: &mut Interp) {
    let both: &[(&'static str, BuiltinFn)] = &[
        ("string", string),
        ("strings", strings),
        ("isstring", |_, a, _| {
            need(&a, 1, "isstring")?;
            one(Value::bool(matches!(a[0], Value::Str(_))))
        }),
        ("isStringScalar", |_, a, _| {
            need(&a, 1, "isStringScalar")?;
            one(Value::bool(
                matches!(&a[0], Value::Str(s) if s.data.len() == 1),
            ))
        }),
        ("strlength", strlength),
        ("ismissing", ismissing),
        ("missing", |_, _, _| one(Value::Str(Array::scalar(None)))),
        ("split", split),
        ("join", join),
        ("splitlines", splitlines),
        ("compose", compose),
        ("replace", replace),
        ("erase", erase),
        ("extractAfter", |_, a, _| extract(&a, 1)),
        ("extractBefore", |_, a, _| extract(&a, 0)),
        ("extractBetween", |_, a, _| extract(&a, 2)),
        ("insertAfter", |_, a, _| insert(&a, true)),
        ("insertBefore", |_, a, _| insert(&a, false)),
        ("strip", strip),
        ("pad", pad),
        ("reverse", reverse),
        ("count", count),
    ];
    for (n, f) in both {
        it.builtins.insert(n, *f);
        it.string_methods.insert(n, *f);
    }
    // String-only versions of functions that also exist for char input.
    let only: &[(&'static str, BuiltinFn)] = &[
        ("upper", |_, a, _| map(&a, "upper", |s| s.to_uppercase())),
        ("lower", |_, a, _| map(&a, "lower", |s| s.to_lowercase())),
        ("strtrim", |_, a, _| {
            map(&a, "strtrim", |s| s.trim().to_string())
        }),
        ("deblank", |_, a, _| {
            map(&a, "deblank", |s| s.trim_end().to_string())
        }),
        ("strrep", |it, a, n| replace(it, a, n)),
        ("contains", |_, a, _| {
            test(&a, "contains", |s, p| s.contains(p))
        }),
        ("startsWith", |_, a, _| {
            test(&a, "startsWith", |s, p| s.starts_with(p))
        }),
        ("endsWith", |_, a, _| {
            test(&a, "endsWith", |s, p| s.ends_with(p))
        }),
        ("strcat", strcat),
        ("sprintf", |_, a, _| {
            need(&a, 1, "sprintf")?;
            let f = arg_str(&a[0], "sprintf")?;
            let s = printf(&escapes(&f), &a[1..])?;
            one(if matches!(a[0], Value::Str(_)) {
                Value::string(&s)
            } else {
                Value::str(&s)
            })
        }),
        ("char", |_, a, _| {
            need(&a, 1, "char")?;
            let t = to_string_array(&a[0]);
            let rows: Vec<String> = t
                .data
                .iter()
                .map(|s| s.clone().unwrap_or_default())
                .collect();
            if rows.len() == 1 {
                return one(Value::str(&rows[0]));
            }
            one(super::strings::char_rows_value(&rows))
        }),
        ("cellstr", |_, a, _| {
            need(&a, 1, "cellstr")?;
            let t = texts(&a[0], "cellstr")?;
            one(rebuild(Kind::Cell, t.dims, t.items))
        }),
        ("double", |_, a, _| {
            need(&a, 1, "double")?;
            let t = to_string_array(&a[0]);
            let v: Vec<f64> = t
                .data
                .iter()
                .map(|s| s.as_deref().map_or(f64::NAN, super::strings::parse_double))
                .collect();
            one(Value::Num(Num::real(t.dims, v)))
        }),
        ("str2double", |_, a, _| {
            need(&a, 1, "str2double")?;
            let t = to_string_array(&a[0]);
            let v: Vec<f64> = t
                .data
                .iter()
                .map(|s| s.as_deref().map_or(f64::NAN, super::strings::parse_double))
                .collect();
            one(Value::Num(Num::real(t.dims, v)))
        }),
        ("strjoin", |_, a, _| {
            need(&a, 1, "strjoin")?;
            let parts: Vec<String> = texts(&a[0], "strjoin")?
                .items
                .into_iter()
                .map(|s| s.unwrap_or_default())
                .collect();
            let d = match a.get(1) {
                Some(v) => escapes(&v.to_str().unwrap_or_default()),
                None => " ".into(),
            };
            one(Value::string(&parts.join(&d)))
        }),
        ("num2str", |it, a, n| {
            need(&a, 1, "num2str")?;
            // Only a string first argument is returned as is; a string
            // format (second argument) goes to the ordinary num2str.
            match (a[0].is_text(), it.builtins.get("num2str").copied()) {
                (false, Some(f)) => f(it, a, n),
                _ => one(a[0].clone()),
            }
        }),
        ("strsplit", |_, a, _| {
            need(&a, 1, "strsplit")?;
            let s = arg_str(&a[0], "strsplit")?;
            let d = match a.get(1) {
                Some(v) => patterns(v, "strsplit")?,
                None => vec![" ".into()],
            };
            let (parts, _) = split_one(&s, &d);
            let n = parts.len();
            one(Value::Str(Array::new(
                vec![1, n],
                parts.into_iter().map(Some).collect(),
            )))
        }),
    ];
    for (n, f) in only {
        it.string_methods.insert(n, *f);
    }
}

fn string(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    if a.is_empty() {
        return one(Value::string(""));
    }
    match &a[0] {
        Value::Struct(_) | Value::Func(_) => Err(err(
            "MATLAB:string:UnsupportedType",
            format!(
                "Conversion to string from {} is not possible.",
                a[0].class_name()
            ),
        )),
        Value::Cell(c) => {
            for x in &c.data {
                if !x.is_text() && !(x.is_numeric() && x.numel() == 1) {
                    return Err(err(
                        "MATLAB:string:MustBeCellstr",
                        "Conversion to string from cell is only possible for cells of text.",
                    ));
                }
            }
            one(Value::Str(c.map(|x| match x {
                Value::Num(_) | Value::Sparse(_) => {
                    to_string_array(x).data.into_iter().next().flatten()
                }
                _ => x.to_str(),
            })))
        }
        v => one(Value::Str(to_string_array(v))),
    }
}

fn strings(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let (d, _) = super::dims_and_class(&a, "strings")?;
    let n = indy_value::numel_of(&d);
    one(Value::Str(Array::new(d, vec![Some(String::new()); n])))
}

fn strlength(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "strlength")?;
    let t = texts(&a[0], "strlength")?;
    let v: Vec<f64> = t
        .items
        .iter()
        .map(|s| s.as_ref().map_or(f64::NAN, |s| s.chars().count() as f64))
        .collect();
    one(Value::Num(Num::real(t.dims, v)))
}

fn ismissing(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "ismissing")?;
    let v = &a[0];
    let r: Vec<bool> = match v {
        Value::Str(s) => s.data.iter().map(|x| x.is_none()).collect(),
        Value::Num(n) => n.re.iter().map(|x| x.is_nan()).collect(),
        Value::Cell(c) => c.data.iter().map(|x| x.is_empty() && x.is_char()).collect(),
        Value::Char(c) => c.data.iter().map(|x| *x == ' ').collect(),
        other => vec![false; other.numel()],
    };
    one(Value::Bool(Array::new(v.dims(), r)))
}

fn map(a: &[Value], fname: &str, f: impl Fn(&str) -> String) -> R<Vec<Value>> {
    need(a, 1, fname)?;
    let t = texts(&a[0], fname)?;
    let items = t.items.iter().map(|s| s.as_deref().map(&f)).collect();
    one(rebuild(t.kind, t.dims, items))
}

fn test(a: &[Value], fname: &str, f: fn(&str, &str) -> bool) -> R<Vec<Value>> {
    need(a, 2, fname)?;
    let t = texts(&a[0], fname)?;
    let pats = patterns(&a[1], fname)?;
    let ic = ignore_case(&a[2..], fname)?;
    let norm = |s: &str| if ic { s.to_lowercase() } else { s.to_string() };
    let r: Vec<bool> = t
        .items
        .iter()
        .map(|s| match s {
            Some(s) => pats.iter().any(|p| f(&norm(s), &norm(p))),
            None => false,
        })
        .collect();
    if t.kind == Kind::Char {
        return one(Value::bool(r[0]));
    }
    one(Value::Bool(Array::new(t.dims, r)))
}

/// Split `s` at any of `delims` (longest match first). Returns the parts
/// and the matched delimiters.
fn split_one(s: &str, delims: &[String]) -> (Vec<String>, Vec<String>) {
    let mut ds: Vec<&String> = delims.iter().filter(|d| !d.is_empty()).collect();
    ds.sort_by_key(|d| std::cmp::Reverse(d.len()));
    let mut parts = Vec::new();
    let mut matches = Vec::new();
    let mut cur = String::new();
    let mut rest = s;
    while !rest.is_empty() {
        if let Some(d) = ds.iter().find(|d| rest.starts_with(d.as_str())) {
            parts.push(std::mem::take(&mut cur));
            matches.push(d.to_string());
            rest = &rest[d.len()..];
            continue;
        }
        let c = rest.chars().next().unwrap();
        cur.push(c);
        rest = &rest[c.len_utf8()..];
    }
    parts.push(cur);
    (parts, matches)
}

fn split_ws(s: &str) -> (Vec<String>, Vec<String>) {
    let mut parts = Vec::new();
    let mut matches = Vec::new();
    let mut cur = String::new();
    let mut ws = String::new();
    for c in s.chars() {
        if c.is_whitespace() {
            ws.push(c);
        } else {
            if !ws.is_empty() {
                parts.push(std::mem::take(&mut cur));
                matches.push(std::mem::take(&mut ws));
            }
            cur.push(c);
        }
    }
    if !ws.is_empty() {
        parts.push(std::mem::take(&mut cur));
        matches.push(ws);
    }
    parts.push(cur);
    (parts, matches)
}

fn split(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "split")?;
    let t = texts(&a[0], "split")?;
    let delims = match a.get(1) {
        Some(v) => Some(patterns(v, "split")?),
        None => None,
    };
    if a.len() > 3 || a.get(2).is_some_and(|d| !d.is_numeric()) {
        return Err(err(
            "MATLAB:TooManyInputs",
            "split: expected split(str, delimiter, dim).",
        ));
    }
    let mut all = Vec::new();
    for s in &t.items {
        let s = s.clone().ok_or_else(|| {
            err(
                "MATLAB:string:split:Missing",
                "split: missing strings cannot be split.",
            )
        })?;
        all.push(match &delims {
            Some(d) => split_one(&s, d),
            None => split_ws(&s),
        });
    }
    let m = all.first().map_or(0, |p| p.0.len());
    if all.iter().any(|p| p.0.len() != m) {
        return Err(err(
            "MATLAB:string:split:SplitDimensionMismatch",
            "Unable to split: each element must split into the same number of parts.",
        ));
    }
    let kind = if t.kind == Kind::Str {
        Kind::Str
    } else {
        Kind::Cell
    };
    let n = all.len();
    let layout = |count: usize, pick: &dyn Fn(usize, usize) -> String| -> Value {
        // Parts go along the first trailing singleton dimension.
        let (dims, items): (Vec<usize>, Vec<Option<String>>) = if n == 1 {
            (
                vec![count, 1],
                (0..count).map(|k| Some(pick(0, k))).collect(),
            )
        } else if t.dims.len() == 2 && t.dims[1] == 1 {
            let mut v = vec![None; n * count];
            for i in 0..n {
                for k in 0..count {
                    v[i + k * n] = Some(pick(i, k));
                }
            }
            (vec![n, count], v)
        } else {
            let mut d = t.dims.clone();
            d.push(count);
            let mut v = vec![None; n * count];
            for i in 0..n {
                for k in 0..count {
                    v[i + k * n] = Some(pick(i, k));
                }
            }
            (d, v)
        };
        rebuild(kind, dims, items)
    };
    let mut out = vec![layout(m, &|i, k| all[i].0[k].clone())];
    if nargout > 1 {
        out.push(layout(m.saturating_sub(1), &|i, k| all[i].1[k].clone()));
    }
    Ok(out)
}

fn join(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "join")?;
    let t = texts(&a[0], "join")?;
    if t.kind == Kind::Char {
        return one(a[0].clone());
    }
    let delims = match a.get(1) {
        Some(v) if !v.is_numeric() => patterns(v, "join")?,
        _ => vec![" ".into()],
    };
    let dim = match a.iter().skip(1).find(|v| v.is_numeric()) {
        Some(v) => arg_f64(v, "join")? as usize - 1,
        None => t.dims.iter().rposition(|d| *d != 1).unwrap_or(1),
    };
    let d = &t.dims;
    let len = *d.get(dim).unwrap_or(&1);
    let stride: usize = d.iter().take(dim).product();
    let outer: usize = d.iter().skip(dim + 1).product();
    if delims.len() != 1 && delims.len() + 1 != len {
        return Err(err(
            "MATLAB:string:join:WrongNumberOfDelimiters",
            "join: the number of delimiters must be one less than the number of elements to join.",
        ));
    }
    let mut items = Vec::new();
    for o in 0..outer {
        for i in 0..stride {
            let mut s = String::new();
            let mut missing = false;
            for k in 0..len {
                if k > 0 {
                    s.push_str(&delims[if delims.len() == 1 { 0 } else { k - 1 }]);
                }
                match &t.items[o * stride * len + i + k * stride] {
                    Some(x) => s.push_str(x),
                    None => missing = true,
                }
            }
            items.push(if missing { None } else { Some(s) });
        }
    }
    let mut nd = d.clone();
    while nd.len() <= dim {
        nd.push(1);
    }
    nd[dim] = 1;
    one(rebuild(t.kind, indy_value::norm_dims(nd), items))
}

fn splitlines(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "splitlines")?;
    let t = texts(&a[0], "splitlines")?;
    let s = t.items.first().cloned().flatten().unwrap_or_default();
    let norm = s.replace("\r\n", "\n").replace('\r', "\n");
    let parts: Vec<Option<String>> = norm.split('\n').map(|p| Some(p.to_string())).collect();
    let n = parts.len();
    let kind = if t.kind == Kind::Str {
        Kind::Str
    } else {
        Kind::Cell
    };
    one(rebuild(kind, vec![n, 1], parts))
}

fn compose(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "compose")?;
    let ft = texts(&a[0], "compose")?;
    let fmt = escapes(&ft.items.first().cloned().flatten().unwrap_or_default());
    let args = &a[1..];
    let rows = args
        .iter()
        .map(|v| {
            if v.is_text() && v.numel() <= 1 || v.is_char() {
                1
            } else {
                v.dims_ref()[0]
            }
        })
        .max()
        .unwrap_or(1);
    let mut items = Vec::with_capacity(rows);
    for r in 0..rows {
        let mut row_args = Vec::new();
        for v in args {
            match v {
                Value::Char(_) => row_args.push(v.clone()),
                Value::Str(s) => {
                    let k = if s.data.len() == 1 {
                        0
                    } else {
                        r.min(s.data.len() - 1)
                    };
                    row_args.push(Value::str(s.data[k].as_deref().unwrap_or("<missing>")));
                }
                Value::Cell(c) => {
                    let k = if c.data.len() == 1 {
                        0
                    } else {
                        r.min(c.data.len() - 1)
                    };
                    row_args.push(c.data[k].clone());
                }
                _ => {
                    let n = v.to_num().ok_or_else(|| {
                        err(
                            "MATLAB:compose:InvalidInput",
                            "compose: unsupported argument type.",
                        )
                    })?;
                    let nr = n.dims[0].max(1);
                    let nc = n.re.len() / nr;
                    let rr = if nr == 1 { 0 } else { r.min(nr - 1) };
                    row_args.push(Value::row((0..nc).map(|j| n.re[rr + j * nr]).collect()));
                }
            }
        }
        items.push(Some(printf(&fmt, &row_args)?));
    }
    let kind = if ft.kind == Kind::Str {
        Kind::Str
    } else {
        Kind::Cell
    };
    one(rebuild(kind, vec![rows, 1], items))
}

fn pairwise(a: &[Value], fname: &str, f: &dyn Fn(&str) -> String) -> R<Vec<Value>> {
    let t = texts(&a[0], fname)?;
    let items = t.items.iter().map(|s| s.as_deref().map(f)).collect();
    one(rebuild(t.kind, t.dims, items))
}

fn replace(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 3, "replace")?;
    let old = patterns(&a[1], "replace")?;
    let new = patterns(&a[2], "replace")?;
    if new.len() != 1 && new.len() != old.len() {
        return Err(err(
            "MATLAB:string:replace:SizeMismatch",
            "replace: NEW must be a scalar or the same size as OLD.",
        ));
    }
    pairwise(&a, "replace", &|s: &str| {
        // Non-overlapping, left to right, longest pattern at each position.
        let mut out = String::new();
        let mut rest = s;
        'outer: while !rest.is_empty() {
            let mut best: Option<usize> = None;
            for (k, o) in old.iter().enumerate() {
                if !o.is_empty()
                    && rest.starts_with(o.as_str())
                    && best.is_none_or(|b| o.len() > old[b].len())
                {
                    best = Some(k);
                }
            }
            if let Some(k) = best {
                out.push_str(&new[if new.len() == 1 { 0 } else { k }]);
                rest = &rest[old[k].len()..];
                continue 'outer;
            }
            let c = rest.chars().next().unwrap();
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
        out
    })
}

fn erase(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    need(&a, 2, "erase")?;
    let mut b = a.clone();
    b.push(Value::str(""));
    b.truncate(3);
    b[2] = Value::str("");
    replace(it, b, n)
}

fn position_at(v: &Value, k: usize, s: &str, after: bool) -> R<Option<usize>> {
    if v.is_numeric() && v.numel() > 1 {
        let x = v.f64_at(k.min(v.numel() - 1)).unwrap_or(1.0);
        return position(&Value::num(x), s, after);
    }
    if let Value::Cell(c) = v
        && !c.data.is_empty()
    {
        return position(&c.data[k.min(c.data.len() - 1)], s, after);
    }
    position(v, s, after)
}

fn position(v: &Value, s: &str, after: bool) -> R<Option<usize>> {
    if v.is_numeric() {
        let p = arg_f64(v, "extract")? as usize;
        let n = s.chars().count();
        if p < 1 || p > n + 1 {
            return Err(err(
                "MATLAB:string:extract:OutOfRange",
                "Position is out of range.",
            ));
        }
        return Ok(Some(if after { p } else { p - 1 }));
    }
    let pat = v.to_str().unwrap_or_default();
    Ok(s.find(&pat).map(|b| {
        let start = s[..b].chars().count();
        if after {
            start + pat.chars().count()
        } else {
            start
        }
    }))
}

fn extract(a: &[Value], mode: u8) -> R<Vec<Value>> {
    need(a, 2, "extract")?;
    let t = texts(&a[0], "extract")?;
    if mode == 2 && a.len() >= 3 && !a[1].is_numeric() && !a[2].is_numeric() {
        return extract_between_text(a, &t);
    }
    if mode == 2 && a.len() >= 3 && a[1].is_numeric() && a[2].is_numeric() {
        return extract_between_positions(a, t);
    }
    let mut items = Vec::new();
    for (k, s) in t.items.iter().enumerate() {
        let Some(s) = s else {
            items.push(None);
            continue;
        };
        let ch: Vec<char> = s.chars().collect();
        let r = match mode {
            0 => position_at(&a[1], k, s, false)?.map(|p| ch[..p].iter().collect::<String>()),
            1 => position_at(&a[1], k, s, true)?.map(|p| ch[p..].iter().collect::<String>()),
            _ => {
                need(a, 3, "extractBetween")?;
                let st = position(&a[1], s, true)?;
                match st {
                    Some(p0) => {
                        let tail: String = ch[p0..].iter().collect();
                        let end = if a[2].is_numeric() {
                            Some(arg_f64(&a[2], "extractBetween")? as usize - p0)
                        } else {
                            position(&a[2], &tail, false)?
                        };
                        end.map(|e| tail.chars().take(e).collect::<String>())
                    }
                    None => None,
                }
            }
        };
        items.push(Some(r.unwrap_or_default()));
    }
    one(rebuild(t.kind, t.dims, items))
}

/// `extractBetween(str, startPos, endPos)`: characters startPos..endPos
/// (inclusive by default; `'Boundaries','exclusive'` drops both ends).
/// Positions may be per element. Char input gives a cell, as in MATLAB.
fn extract_between_positions(a: &[Value], t: Texts) -> R<Vec<Value>> {
    let mut inclusive = true;
    let opts = &a[3..];
    for w in opts.chunks(2) {
        let name = w[0].to_str().unwrap_or_default();
        let val = w.get(1).and_then(|v| v.to_str()).unwrap_or_default();
        if !name.eq_ignore_ascii_case("Boundaries") {
            return Err(err(
                "MATLAB:string:extractBetween:UnknownOption",
                format!("extractBetween: unknown option '{}'.", name),
            ));
        }
        inclusive = match val.to_ascii_lowercase().as_str() {
            "inclusive" => true,
            "exclusive" => false,
            _ => {
                return Err(err(
                    "MATLAB:string:extractBetween:BadBoundaries",
                    "extractBetween: Boundaries must be 'inclusive' or 'exclusive'.",
                ));
            }
        };
    }
    let at = |v: &Value, k: usize| -> R<f64> {
        let n = v.numel();
        if n == 0 {
            return Err(err(
                "MATLAB:string:extract:OutOfRange",
                "Position is out of range.",
            ));
        }
        Ok(v.f64_at(if n == 1 { 0 } else { k.min(n - 1) })
            .unwrap_or(f64::NAN))
    };
    let mut items = Vec::new();
    for (k, s) in t.items.iter().enumerate() {
        let Some(s) = s else {
            items.push(None);
            continue;
        };
        let ch: Vec<char> = s.chars().collect();
        let (p, q) = (at(&a[1], k)?, at(&a[2], k)?);
        let n = ch.len() as f64;
        if p.fract() != 0.0 || q.fract() != 0.0 || p < 1.0 || q > n || p > q + 1.0 {
            return Err(err(
                "MATLAB:string:extract:OutOfRange",
                "Position is out of range.",
            ));
        }
        let (lo, hi) = if inclusive {
            (p as usize - 1, q as usize)
        } else {
            (p as usize, (q as usize).saturating_sub(1).max(p as usize))
        };
        items.push(Some(ch[lo..hi].iter().collect::<String>()));
    }
    let kind = if t.kind == Kind::Char {
        Kind::Cell
    } else {
        t.kind
    };
    one(rebuild(kind, t.dims, items))
}

/// extractBetween with text boundaries: every occurrence, cell output
/// for char input; `'Boundaries','inclusive'` keeps the delimiters.
fn extract_between_text(a: &[Value], t: &Texts) -> R<Vec<Value>> {
    let start = a[1].to_str().unwrap_or_default();
    let end = a[2].to_str().unwrap_or_default();
    for w in a[3..].chunks(2) {
        let ok = w.len() == 2
            && w[0]
                .to_str()
                .is_some_and(|s| s.eq_ignore_ascii_case("Boundaries"))
            && w[1].to_str().is_some_and(|s| {
                s.eq_ignore_ascii_case("inclusive") || s.eq_ignore_ascii_case("exclusive")
            });
        if !ok {
            return Err(err(
                "MATLAB:string:extractBetween:UnknownOption",
                "extractBetween: the only option is 'Boundaries', 'inclusive' or 'exclusive'.",
            ));
        }
    }
    let inclusive = a[3..].windows(2).any(|w| {
        w[0].to_str()
            .is_some_and(|s| s.eq_ignore_ascii_case("Boundaries"))
            && w[1]
                .to_str()
                .is_some_and(|s| s.eq_ignore_ascii_case("inclusive"))
    });
    let mut all = Vec::new();
    for s in t.items.iter().flatten() {
        let mut rest = s.as_str();
        let mut found = Vec::new();
        while let Some(i) = rest.find(&start) {
            let after = &rest[i + start.len()..];
            let Some(j) = after.find(&end) else { break };
            let inner = &after[..j];
            found.push(if inclusive {
                format!("{}{}{}", start, inner, end)
            } else {
                inner.to_string()
            });
            rest = &after[j + end.len()..];
        }
        all.push(found);
    }
    if t.items.len() == 1 {
        let found = all.pop().unwrap_or_default();
        let n = found.len();
        let kind = if t.kind == Kind::Str {
            Kind::Str
        } else {
            Kind::Cell
        };
        return one(rebuild(
            kind,
            vec![n, 1],
            found.into_iter().map(Some).collect(),
        ));
    }
    let items = all.into_iter().map(|f| f.into_iter().next()).collect();
    one(rebuild(t.kind, t.dims.clone(), items))
}

fn insert(a: &[Value], after: bool) -> R<Vec<Value>> {
    need(a, 3, "insertAfter")?;
    let t = texts(&a[0], "insertAfter")?;
    let ins = a[2].to_str().unwrap_or_default();
    let mut items = Vec::new();
    for s in &t.items {
        let Some(s) = s else {
            items.push(None);
            continue;
        };
        let out = if a[1].is_numeric() {
            let p = position(&a[1], s, true)?.unwrap_or(0);
            let p = if after { p } else { p.saturating_sub(1) };
            let ch: Vec<char> = s.chars().collect();
            format!(
                "{}{}{}",
                ch[..p].iter().collect::<String>(),
                ins,
                ch[p..].iter().collect::<String>()
            )
        } else {
            let pat = a[1].to_str().unwrap_or_default();
            if pat.is_empty() {
                s.clone()
            } else if after {
                s.replace(&pat, &format!("{}{}", pat, ins))
            } else {
                s.replace(&pat, &format!("{}{}", ins, pat))
            }
        };
        items.push(Some(out));
    }
    one(rebuild(t.kind, t.dims, items))
}

fn strip(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "strip")?;
    // strip(str), strip(str, side), strip(str, char), strip(str, side, char)
    let mut side = "both".to_string();
    let mut chars: Option<Vec<char>> = None;
    if a.len() > 3 {
        return Err(err(
            "MATLAB:TooManyInputs",
            "strip: too many input arguments.",
        ));
    }
    for (i, v) in a[1..].iter().enumerate() {
        let s = v.to_str().ok_or_else(|| {
            err(
                "MATLAB:strip:InvalidArgument",
                "strip: side and strip character must be text.",
            )
        })?;
        let low = s.to_ascii_lowercase();
        if i == 0 && matches!(low.as_str(), "both" | "left" | "right") {
            side = low;
        } else if s.chars().count() == 1 {
            chars = Some(s.chars().collect());
        } else {
            return Err(err(
                "MATLAB:strip:InvalidArgument",
                format!(
                    "strip: '{}' is neither 'left', 'right' or 'both' nor a single character.",
                    s
                ),
            ));
        }
    }
    let is_strip = |c: char| match &chars {
        Some(cs) => cs.contains(&c),
        None => c.is_whitespace(),
    };
    pairwise(&a, "strip", &|s: &str| match side.as_str() {
        "left" => s.trim_start_matches(is_strip).to_string(),
        "right" => s.trim_end_matches(is_strip).to_string(),
        _ => s.trim_matches(is_strip).to_string(),
    })
}

fn pad(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "pad")?;
    let t = texts(&a[0], "pad")?;
    let mut width = None;
    let mut side = "right".to_string();
    let mut fill = ' ';
    // pad(str, [n], [side], [padChar])
    let mut seen_side = false;
    for (i, v) in a[1..].iter().enumerate() {
        if v.is_numeric() && i == 0 {
            width = Some(arg_f64(v, "pad")? as usize);
            continue;
        }
        let s = v.to_str().ok_or_else(|| {
            err(
                "MATLAB:pad:InvalidArgument",
                "pad: side and pad character must be text.",
            )
        })?;
        let low = s.to_ascii_lowercase();
        if !seen_side && matches!(low.as_str(), "left" | "right" | "both") {
            side = low;
            seen_side = true;
        } else if s.chars().count() == 1 && i + 2 == a.len() {
            fill = s.chars().next().unwrap();
        } else {
            return Err(err(
                "MATLAB:pad:InvalidArgument",
                format!(
                    "pad: '{}' is neither 'left', 'right' or 'both' nor a single pad character.",
                    s
                ),
            ));
        }
    }
    let w = width.unwrap_or_else(|| {
        t.items
            .iter()
            .flatten()
            .map(|s| s.chars().count())
            .max()
            .unwrap_or(0)
    });
    let items = t
        .items
        .iter()
        .map(|s| {
            s.as_ref().map(|s| {
                let n = s.chars().count();
                if n >= w {
                    return s.clone();
                }
                let k = w - n;
                let f = |m: usize| fill.to_string().repeat(m);
                match side.as_str() {
                    "left" => format!("{}{}", f(k), s),
                    "both" => format!("{}{}{}", f(k / 2), s, f(k - k / 2)),
                    _ => format!("{}{}", s, f(k)),
                }
            })
        })
        .collect();
    one(rebuild(t.kind, t.dims, items))
}

fn reverse(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "reverse")?;
    pairwise(&a, "reverse", &|s: &str| s.chars().rev().collect())
}

fn count(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "count")?;
    let t = texts(&a[0], "count")?;
    let pats = patterns(&a[1], "count")?;
    let ic = ignore_case(&a[2..], "count")?;
    let v: Vec<f64> = t
        .items
        .iter()
        .map(|s| {
            let Some(s) = s else { return f64::NAN };
            let s = if ic { s.to_lowercase() } else { s.clone() };
            let mut n = 0;
            let mut rest = s.as_str();
            'o: while !rest.is_empty() {
                for p in &pats {
                    let p = if ic { p.to_lowercase() } else { p.clone() };
                    if !p.is_empty() && rest.starts_with(&p) {
                        n += 1;
                        rest = &rest[p.len()..];
                        continue 'o;
                    }
                }
                let c = rest.chars().next().unwrap();
                rest = &rest[c.len_utf8()..];
            }
            n as f64
        })
        .collect();
    if t.kind == Kind::Char {
        return one(Value::num(v[0]));
    }
    one(Value::Num(Num::real(t.dims, v)))
}

fn strcat(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let mut acc: Option<Value> = None;
    for v in &a {
        let x = Value::Str(to_string_array(v));
        acc = Some(match acc {
            None => x,
            Some(p) => indy_value::ops::binary(indy_syntax::ast::BinOp::Add, &p, &x)?,
        });
    }
    one(acc.unwrap_or_else(|| Value::string("")))
}
