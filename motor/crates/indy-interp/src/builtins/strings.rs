//! Text builtins (char arrays and cellstr). MATLAB `string` objects are
//! not modelled in phase 2: `"..."` is a char row (Octave semantics).

use indy_value::format::{mat2str, num2str};
use indy_value::{Array, Num, Value, concat};

use super::cells::{cellstr_value, is_cellstr, strings_of};
use super::{arg_f64, arg_str, arg_usize, err, need, one, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "num2str", num2str_b);
    reg(it, "int2str", |_, a, _| {
        need(&a, 1, "int2str")?;
        let n = super::arg_num(&a[0], "int2str")?;
        let r: Vec<f64> = n.re.iter().map(|x| x.round()).collect();
        one(Value::str(&num2str(&Num::real(n.dims, r))))
    });
    reg(it, "mat2str", |_, a, _| {
        need(&a, 1, "mat2str")?;
        let p = match a.get(1) {
            Some(v) => arg_usize(v, "mat2str")?,
            None => 15,
        };
        one(Value::str(&mat2str(&a[0], p)?))
    });
    reg(it, "str2double", str2double);
    reg(it, "str2num", str2num);
    reg(it, "strcmp", |_, a, _| cmp(&a, "strcmp", false, None));
    reg(it, "strcmpi", |_, a, _| cmp(&a, "strcmpi", true, None));
    reg(it, "strncmp", |_, a, _| {
        need(&a, 3, "strncmp")?;
        let n = arg_usize(&a[2], "strncmp")?;
        cmp(&a, "strncmp", false, Some(n))
    });
    reg(it, "strncmpi", |_, a, _| {
        need(&a, 3, "strncmpi")?;
        let n = arg_usize(&a[2], "strncmpi")?;
        cmp(&a, "strncmpi", true, Some(n))
    });
    reg(it, "strcat", strcat);
    reg(it, "strvcat", |_, a, _| char_matrix(&a, true));
    reg(it, "char", |_, a, _| char_matrix(&a, false));
    reg(it, "upper", |_, a, _| {
        map_text(&a, "upper", |s| s.to_uppercase())
    });
    reg(it, "toupper", |_, a, _| {
        map_text(&a, "toupper", |s| s.to_uppercase())
    });
    reg(it, "lower", |_, a, _| {
        map_text(&a, "lower", |s| s.to_lowercase())
    });
    reg(it, "tolower", |_, a, _| {
        map_text(&a, "tolower", |s| s.to_lowercase())
    });
    reg(it, "strtrim", |_, a, _| {
        map_text(&a, "strtrim", |s| {
            s.trim_matches(|c: char| c.is_whitespace() || c == '\0')
                .to_string()
        })
    });
    reg(it, "deblank", |_, a, _| {
        map_text(&a, "deblank", |s| {
            s.trim_end_matches(|c: char| c.is_whitespace() || c == '\0')
                .to_string()
        })
    });
    reg(it, "blanks", |_, a, _| {
        need(&a, 1, "blanks")?;
        let n = arg_usize(&a[0], "blanks")?;
        one(Value::Char(Array::new(vec![1, n], vec![' '; n])))
    });
    reg(it, "strsplit", strsplit);
    reg(it, "strjoin", strjoin);
    reg(it, "strrep", strrep);
    reg(it, "strfind", strfind);
    reg(it, "strtok", strtok);
    reg(it, "isspace", |_, a, _| {
        char_test(&a, |c| c.is_whitespace())
    });
    reg(it, "isletter", |_, a, _| {
        char_test(&a, |c| c.is_alphabetic())
    });
    reg(it, "isdigit", |_, a, _| {
        char_test(&a, |c| c.is_ascii_digit())
    });
    reg(it, "isupper", |_, a, _| char_test(&a, |c| c.is_uppercase()));
    reg(it, "islower", |_, a, _| char_test(&a, |c| c.is_lowercase()));
    reg(it, "isalpha", |_, a, _| {
        char_test(&a, |c| c.is_alphabetic())
    });
    reg(it, "isstrprop", isstrprop);
    reg(it, "startsWith", |_, a, _| affix(&a, true));
    reg(it, "endsWith", |_, a, _| affix(&a, false));
    reg(it, "contains", contains);
    reg(it, "dec2bin", |_, a, _| dec2base(&a, 2));
    reg(it, "dec2hex", |_, a, _| dec2base(&a, 16));
    reg(it, "dec2base", |_, a, _| {
        need(&a, 2, "dec2base")?;
        let b = arg_usize(&a[1], "dec2base")?;
        let mut rest = vec![a[0].clone()];
        if let Some(v) = a.get(2) {
            rest.push(v.clone());
        }
        dec2base(&rest, b as u32)
    });
    reg(it, "bin2dec", |_, a, _| base2dec(&a, 2));
    reg(it, "hex2dec", |_, a, _| base2dec(&a, 16));
    reg(it, "base2dec", |_, a, _| {
        need(&a, 2, "base2dec")?;
        let b = arg_usize(&a[1], "base2dec")?;
        base2dec(&a[..1], b as u32)
    });
    reg(it, "validatestring", validatestring);
    reg(it, "newline", |_, _, _| one(Value::str("\n")));
    reg(it, "substr", |_, a, _| {
        need(&a, 2, "substr")?;
        let s: Vec<char> = arg_str(&a[0], "substr")?.chars().collect();
        let off = arg_usize(&a[1], "substr")?.max(1) - 1;
        let len = match a.get(2) {
            Some(v) => arg_usize(v, "substr")?,
            None => s.len().saturating_sub(off),
        };
        one(Value::str(
            &s.iter().skip(off).take(len).collect::<String>(),
        ))
    });
    reg(it, "regexptranslate", |_, a, _| {
        need(&a, 2, "regexptranslate")?;
        let op = arg_str(&a[0], "regexptranslate")?;
        let s = arg_str(&a[1], "regexptranslate")?;
        let out = match op.as_str() {
            "escape" => s
                .chars()
                .flat_map(|c| {
                    if "\\^$.|?*+()[]{}".contains(c) {
                        vec!['\\', c]
                    } else {
                        vec![c]
                    }
                })
                .collect(),
            "wildcard" => s.replace('.', "\\.").replace('*', ".*").replace('?', "."),
            _ => s,
        };
        one(Value::str(&out))
    });
}

fn num2str_b(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "num2str")?;
    let v = &a[0];
    if v.is_char() {
        return one(v.clone());
    }
    if let Some(f) = a.get(1) {
        if let Some(fmt) = f.to_str() {
            let n = super::arg_num(v, "num2str")?;
            // Apply the format row by row.
            let rows = n.dims[0].max(1);
            let cols = n.re.len() / rows;
            let mut lines = Vec::new();
            for i in 0..rows {
                let vals: Vec<f64> = (0..cols).map(|j| n.re[i + j * rows]).collect();
                lines.push(indy_value::format::printf(
                    &indy_value::format::escapes(&fmt),
                    &[Value::row(vals)],
                )?);
            }
            return one(char_rows_value(&lines));
        }
        let p = arg_f64(f, "num2str")? as usize;
        let n = super::arg_num(v, "num2str")?;
        let parts: Vec<String> =
            n.re.iter()
                .map(|x| {
                    if x.is_nan() {
                        "NaN".into()
                    } else if x.is_infinite() {
                        if *x > 0.0 {
                            "Inf".into()
                        } else {
                            "-Inf".into()
                        }
                    } else {
                        indy_value::format::fmt_g(*x, p.max(1), false, false)
                    }
                })
                .collect();
        return one(Value::str(&parts.join("  ")));
    }
    let n = super::arg_num(v, "num2str")?;
    let s = num2str(&n);
    let lines: Vec<String> = s.split('\n').map(|l| l.to_string()).collect();
    one(char_rows_value(&lines))
}

/// Char matrix from rows (padded with spaces).
pub fn char_rows_value(rows: &[String]) -> Value {
    if rows.len() == 1 {
        return Value::str(&rows[0]);
    }
    let w = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0);
    let r = rows.len();
    let mut data = vec![' '; r * w];
    for (i, row) in rows.iter().enumerate() {
        for (j, c) in row.chars().enumerate() {
            data[i + j * r] = c;
        }
    }
    Value::Char(Array::new(vec![r, w], data))
}

pub fn parse_double(s: &str) -> f64 {
    let t = s.trim();
    if t.is_empty() {
        return f64::NAN;
    }
    let lower = t.to_ascii_lowercase();
    match lower.as_str() {
        "inf" | "+inf" => return f64::INFINITY,
        "-inf" => return f64::NEG_INFINITY,
        "nan" | "+nan" | "-nan" => return f64::NAN,
        _ => {}
    }
    let cleaned: String = t.chars().filter(|c| *c != ',').collect();
    let cleaned = cleaned.replace(['d', 'D'], "e");
    if cleaned.contains("..") {
        return f64::NAN;
    }
    cleaned.parse::<f64>().unwrap_or(f64::NAN)
}

fn str2double(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "str2double")?;
    match &a[0] {
        Value::Cell(c) => {
            let v: Vec<f64> = c
                .data
                .iter()
                .map(|x| x.to_str().map_or(f64::NAN, |s| parse_double(&s)))
                .collect();
            one(Value::Num(Num::real(c.dims.clone(), v)))
        }
        Value::Char(_) => {
            let rows = a[0].char_rows().unwrap();
            if rows.len() <= 1 {
                return one(Value::num(parse_double(&a[0].to_str().unwrap())));
            }
            one(Value::col(rows.iter().map(|r| parse_double(r)).collect()))
        }
        _ => one(Value::num(f64::NAN)),
    }
}

fn str2num(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "str2num")?;
    let s = arg_str(&a[0], "str2num")?;
    let text = format!("__str2num_tmp__ = [{}];", s);
    let ok = it.run_text(&text).is_ok();
    let v = if ok {
        it.take_var("__str2num_tmp__").unwrap_or_default()
    } else {
        Value::empty()
    };
    let mut out = vec![v];
    if nargout > 1 {
        out.push(Value::bool(ok));
    }
    Ok(out)
}

/// strcmp family over char/cellstr combinations.
fn cmp(a: &[Value], fname: &str, nocase: bool, n: Option<usize>) -> R<Vec<Value>> {
    need(a, 2, fname)?;
    let eq = |x: &Value, y: &Value| -> bool {
        let (Some(s), Some(t)) = (x.to_str(), y.to_str()) else {
            return false;
        };
        if !x.is_text() || !y.is_text() {
            return false;
        }
        let strish = matches!(x, Value::Str(_)) || matches!(y, Value::Str(_));
        if !strish && n.is_none() && x.dims_ref() != y.dims_ref() && !(x.is_empty() && y.is_empty())
        {
            return false;
        }
        let (s, t) = if nocase {
            (s.to_lowercase(), t.to_lowercase())
        } else {
            (s, t)
        };
        match n {
            Some(k) => {
                let sc: Vec<char> = s.chars().collect();
                let tc: Vec<char> = t.chars().collect();
                sc.len() >= k && tc.len() >= k && sc[..k] == tc[..k]
            }
            None => s == t,
        }
    };
    match (&a[0], &a[1]) {
        (Value::Cell(x), Value::Cell(y)) => {
            if x.data.len() == 1 {
                let r = y.data.iter().map(|v| eq(&x.data[0], v)).collect();
                return one(Value::Bool(Array::new(y.dims.clone(), r)));
            }
            if y.data.len() == 1 {
                let r = x.data.iter().map(|v| eq(v, &y.data[0])).collect();
                return one(Value::Bool(Array::new(x.dims.clone(), r)));
            }
            if x.dims != y.dims {
                return Err(err(
                    "MATLAB:strcmp:InputsSizeMismatch",
                    "Inputs must be the same size or either one can be a scalar.",
                ));
            }
            let r = x.data.iter().zip(&y.data).map(|(p, q)| eq(p, q)).collect();
            one(Value::Bool(Array::new(x.dims.clone(), r)))
        }
        (Value::Cell(x), other) | (other, Value::Cell(x)) => {
            let r = x.data.iter().map(|v| eq(v, other)).collect();
            one(Value::Bool(Array::new(x.dims.clone(), r)))
        }
        (x, y) => one(Value::bool(eq(x, y))),
    }
}

fn strcat(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    if a.iter().any(|v| matches!(v, Value::Cell(_))) {
        let mut n = 1;
        let mut dims = vec![1, 1];
        for v in &a {
            if let Value::Cell(c) = v
                && c.data.len() != 1
            {
                n = c.data.len();
                dims = c.dims.clone();
            }
        }
        let mut out = vec![String::new(); n];
        for v in &a {
            match v {
                Value::Cell(c) => {
                    for (i, o) in out.iter_mut().enumerate() {
                        let x = if c.data.len() == 1 {
                            &c.data[0]
                        } else {
                            &c.data[i]
                        };
                        o.push_str(&x.to_str().unwrap_or_default());
                    }
                }
                other => {
                    let s = other.to_str().unwrap_or_default();
                    for o in out.iter_mut() {
                        o.push_str(&s);
                    }
                }
            }
        }
        let vals: Vec<Value> = out.iter().map(|s| Value::str(s)).collect();
        return one(Value::Cell(Array::new(dims, vals)));
    }
    // Char inputs: trailing whitespace of each char argument is removed.
    let mut rows: Option<Vec<String>> = None;
    for v in &a {
        let r: Vec<String> = match v {
            Value::Char(_) => v
                .char_rows()
                .unwrap()
                .into_iter()
                .map(|s| s.trim_end_matches([' ', '\t', '\n', '\0']).to_string())
                .collect(),
            Value::Num(_) | Value::Bool(_) => {
                let n = v.to_num().unwrap();
                vec![
                    n.re.iter()
                        .map(|x| char::from_u32(*x as u32).unwrap_or('\0'))
                        .collect(),
                ]
            }
            _ => {
                return Err(err(
                    "MATLAB:strcat:InvalidInputType",
                    "strcat: inputs must be char or cell arrays.",
                ));
            }
        };
        if v.is_empty() {
            continue;
        }
        rows = Some(match rows {
            None => r,
            Some(prev) => {
                if prev.len() == r.len() {
                    prev.into_iter().zip(r).map(|(p, q)| p + &q).collect()
                } else if r.len() == 1 {
                    prev.into_iter().map(|p| p + &r[0]).collect()
                } else if prev.len() == 1 {
                    r.into_iter().map(|q| prev[0].clone() + &q).collect()
                } else {
                    return Err(err(
                        "MATLAB:strcat:NumberOfInputRows",
                        "strcat: inputs must have the same number of rows.",
                    ));
                }
            }
        });
    }
    let rows = rows.unwrap_or_else(|| vec![String::new()]);
    one(char_rows_value(&rows))
}

fn char_matrix(a: &[Value], skip_empty: bool) -> R<Vec<Value>> {
    if a.len() == 1
        && let Some(m) = crate::classes::get_prop(&a[0], "__member__")
            .filter(|_| crate::classes::is_class_obj(&a[0]))
    {
        return one(m);
    }
    if a.len() == 1 {
        match &a[0] {
            Value::Char(_) => return one(a[0].clone()),
            Value::Num(_) | Value::Bool(_) => return one(super::math::to_class(&a[0], "char")?),
            _ => {}
        }
    }
    let mut rows = Vec::new();
    for v in a {
        match v {
            Value::Cell(_) => rows.extend(strings_of(v, "char")?),
            Value::Char(_) => {
                let r = v.char_rows().unwrap();
                if r.is_empty() && !skip_empty {
                    rows.push(String::new());
                }
                rows.extend(r);
            }
            Value::Num(_) | Value::Bool(_) => {
                let c = super::math::to_class(v, "char")?;
                rows.extend(c.char_rows().unwrap());
            }
            other => {
                return Err(err(
                    "MATLAB:char:invalidInput",
                    format!("char: cannot convert {} to char.", other.class_name()),
                ));
            }
        }
    }
    if skip_empty {
        rows.retain(|r| !r.is_empty());
    }
    if rows.is_empty() {
        return one(Value::str(""));
    }
    if rows.len() == 1 {
        return one(Value::str(&rows[0]));
    }
    one(char_rows_value(&rows))
}

fn map_text(a: &[Value], fname: &str, f: impl Fn(&str) -> String) -> R<Vec<Value>> {
    need(a, 1, fname)?;
    match &a[0] {
        Value::Cell(c) => {
            let mut out = Vec::with_capacity(c.data.len());
            for v in &c.data {
                out.push(match v.to_str() {
                    Some(s) => Value::str(&f(&s)),
                    None => {
                        return Err(err(
                            "MATLAB:invalidType",
                            format!("{}: cell elements must be text.", fname),
                        ));
                    }
                });
            }
            one(Value::Cell(Array::new(c.dims.clone(), out)))
        }
        Value::Char(_) => {
            let rows = a[0].char_rows().unwrap();
            if rows.len() <= 1 {
                let s = a[0].to_str().unwrap();
                let r = f(&s);
                if r.is_empty() && a[0].dims_ref() == [1, 0] {
                    return one(Value::Char(Array::new(vec![1, 0], vec![])));
                }
                return one(Value::str(&r));
            }
            let mapped: Vec<String> = rows.iter().map(|r| f(r)).collect();
            one(char_rows_value(&mapped))
        }
        other => one(other.clone()),
    }
}

fn char_test(a: &[Value], f: fn(char) -> bool) -> R<Vec<Value>> {
    need(a, 1, "is*")?;
    match &a[0] {
        Value::Char(c) => one(Value::Bool(Array::new(
            c.dims.clone(),
            c.data.iter().map(|x| f(*x)).collect(),
        ))),
        v => one(Value::Bool(Array::new(v.dims(), vec![false; v.numel()]))),
    }
}

fn isstrprop(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "isstrprop")?;
    let p = arg_str(&a[1], "isstrprop")?;
    let f: fn(char) -> bool = match p.as_str() {
        "alpha" => |c| c.is_alphabetic(),
        "digit" => |c| c.is_ascii_digit(),
        "alphanum" => |c| c.is_alphanumeric(),
        "lower" => |c| c.is_lowercase(),
        "upper" => |c| c.is_uppercase(),
        "wspace" => |c| c.is_whitespace(),
        "punct" => |c| c.is_ascii_punctuation(),
        "cntrl" => |c| c.is_control(),
        "xdigit" => |c| c.is_ascii_hexdigit(),
        "print" => |c| !c.is_control(),
        "graphic" | "graph" => |c| !c.is_control() && !c.is_whitespace(),
        _ => {
            return Err(err(
                "MATLAB:isstrprop:invalidCategory",
                format!("isstrprop: unknown category '{}'.", p),
            ));
        }
    };
    char_test(&a[..1], f)
}

fn texts(v: &Value, fname: &str) -> R<(Vec<String>, Option<Vec<usize>>)> {
    match v {
        Value::Cell(c) => Ok((strings_of(v, fname)?, Some(c.dims.clone()))),
        _ => Ok((vec![arg_str(v, fname)?], None)),
    }
}

fn bool_result(vals: Vec<bool>, dims: Option<Vec<usize>>) -> Value {
    match dims {
        Some(d) => Value::Bool(Array::new(d, vals)),
        None => Value::bool(vals[0]),
    }
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

fn affix(a: &[Value], start: bool) -> R<Vec<Value>> {
    need(a, 2, "startsWith")?;
    let (strs, dims) = texts(&a[0], "startsWith")?;
    let (pats, _) = texts(&a[1], "startsWith")?;
    let ic = ignore_case(&a[2..], if start { "startsWith" } else { "endsWith" })?;
    let norm = |s: &str| if ic { s.to_lowercase() } else { s.to_string() };
    let vals = strs
        .iter()
        .map(|s| {
            let s = norm(s);
            pats.iter().any(|p| {
                let p = norm(p);
                if start {
                    s.starts_with(&p)
                } else {
                    s.ends_with(&p)
                }
            })
        })
        .collect();
    one(bool_result(vals, dims))
}

fn contains(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "contains")?;
    let (strs, dims) = texts(&a[0], "contains")?;
    let (pats, _) = texts(&a[1], "contains")?;
    let ic = ignore_case(&a[2..], "contains")?;
    let norm = |s: &str| if ic { s.to_lowercase() } else { s.to_string() };
    let vals = strs
        .iter()
        .map(|s| pats.iter().any(|p| norm(s).contains(&norm(p))))
        .collect();
    one(bool_result(vals, dims))
}

fn strsplit(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "strsplit")?;
    let s = arg_str(&a[0], "strsplit")?;
    let mut delims: Vec<String> = match a.get(1) {
        Some(v) if !matches!(v.to_str().as_deref(), Some("CollapseDelimiters")) => {
            let (d, _) = texts(v, "strsplit")?;
            d.into_iter()
                .map(|x| indy_value::format::escapes(&x))
                .collect()
        }
        _ => vec![" ".into(), "\t".into(), "\n".into()],
    };
    let mut collapse = true;
    let mut i = if a.len() >= 2
        && !delims.is_empty()
        && a[1].to_str().as_deref() != Some("CollapseDelimiters")
    {
        2
    } else {
        1
    };
    while i + 1 < a.len() {
        if a[i]
            .to_str()
            .is_some_and(|s| s.eq_ignore_ascii_case("CollapseDelimiters"))
        {
            collapse = a[i + 1].scalar_f64().is_some_and(|x| x != 0.0);
        }
        i += 2;
    }
    delims.sort_by_key(|d| std::cmp::Reverse(d.len()));
    let chars: Vec<char> = s.chars().collect();
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut k = 0;
    let mut last_was_delim = false;
    while k < chars.len() {
        let rest: String = chars[k..].iter().collect();
        if let Some(d) = delims
            .iter()
            .find(|d| !d.is_empty() && rest.starts_with(d.as_str()))
        {
            if !(collapse && last_was_delim) {
                parts.push(std::mem::take(&mut cur));
            }
            k += d.chars().count();
            last_was_delim = true;
            continue;
        }
        last_was_delim = false;
        cur.push(chars[k]);
        k += 1;
    }
    parts.push(cur);
    one(cellstr_value(parts, false))
}

fn strjoin(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "strjoin")?;
    if !is_cellstr(&a[0]) {
        return Err(err(
            "MATLAB:strjoin:InvalidCellType",
            "strjoin: first input must be a cell array of character vectors.",
        ));
    }
    let strs = strings_of(&a[0], "strjoin")?;
    let delim = match a.get(1) {
        Some(v) => indy_value::format::escapes(&arg_str(v, "strjoin")?),
        None => " ".into(),
    };
    one(Value::str(&strs.join(&delim)))
}

fn strrep(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 3, "strrep")?;
    let from = arg_str(&a[1], "strrep")?;
    let to = arg_str(&a[2], "strrep")?;
    let f = |s: &str| {
        if from.is_empty() {
            s.to_string()
        } else {
            s.replace(&from, &to)
        }
    };
    match &a[0] {
        Value::Cell(c) => {
            let out: Vec<Value> = c
                .data
                .iter()
                .map(|v| Value::str(&f(&v.to_str().unwrap_or_default())))
                .collect();
            one(Value::Cell(Array::new(c.dims.clone(), out)))
        }
        v => one(Value::str(&f(&arg_str(v, "strrep")?))),
    }
}

fn find_all(s: &[char], p: &[char]) -> Vec<f64> {
    if p.is_empty() || p.len() > s.len() {
        return vec![];
    }
    (0..=s.len() - p.len())
        .filter(|&i| &s[i..i + p.len()] == p)
        .map(|i| i as f64 + 1.0)
        .collect()
}

fn strfind(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "strfind")?;
    let p: Vec<char> = arg_str(&a[1], "strfind")?.chars().collect();
    let res = |s: &str| {
        let s: Vec<char> = s.chars().collect();
        let v = find_all(&s, &p);
        if v.is_empty() {
            Value::Num(Num::real(vec![1, 0], vec![]))
        } else {
            Value::row(v)
        }
    };
    match &a[0] {
        Value::Cell(c) => {
            let out: Vec<Value> = c
                .data
                .iter()
                .map(|v| res(&v.to_str().unwrap_or_default()))
                .collect();
            one(Value::Cell(Array::new(c.dims.clone(), out)))
        }
        v => one(res(&arg_str(v, "strfind")?)),
    }
}

fn strtok(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "strtok")?;
    let s: Vec<char> = arg_str(&a[0], "strtok")?.chars().collect();
    let delims: Vec<char> = match a.get(1) {
        Some(v) => arg_str(v, "strtok")?.chars().collect(),
        None => vec![' ', '\t', '\n', '\x0b', '\x0c', '\r', '\0'],
    };
    let mut i = 0;
    while i < s.len() && delims.contains(&s[i]) {
        i += 1;
    }
    let mut j = i;
    while j < s.len() && !delims.contains(&s[j]) {
        j += 1;
    }
    let tok: String = s[i..j].iter().collect();
    let rem: String = s[j..].iter().collect();
    let mut out = vec![Value::str(&tok)];
    if nargout > 1 {
        out.push(Value::str(&rem));
    }
    Ok(out)
}

fn dec2base(a: &[Value], base: u32) -> R<Vec<Value>> {
    need(a, 1, "dec2bin")?;
    let n = super::arg_num(&a[0], "dec2bin")?;
    let width = match a.get(1) {
        Some(v) => arg_usize(v, "dec2bin")?,
        None => 0,
    };
    let digits = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    // Negative inputs use two's complement in the smallest signed width
    // that holds all values (MATLAB R2020a+).
    let minv = n.re.iter().cloned().fold(0.0, f64::min);
    let bits: u32 = if minv >= -128.0 {
        8
    } else if minv >= -32768.0 {
        16
    } else if minv >= -2147483648.0 {
        32
    } else {
        64
    };
    let conv = |x: f64| -> String {
        let x = if x < 0.0 {
            x.round() + 2f64.powi(bits as i32)
        } else {
            x
        };
        let mut v = x.max(0.0).round() as u128;
        if v == 0 {
            return "0".into();
        }
        let mut s = Vec::new();
        while v > 0 {
            s.push(digits.as_bytes()[(v % base as u128) as usize] as char);
            v /= base as u128;
        }
        s.iter().rev().collect()
    };
    let strs: Vec<String> = n.re.iter().map(|x| conv(*x)).collect();
    let w = strs.iter().map(|s| s.len()).max().unwrap_or(0).max(width);
    let rows: Vec<String> = strs.iter().map(|s| format!("{:0>w$}", s, w = w)).collect();
    if rows.is_empty() {
        return one(Value::str(""));
    }
    one(char_rows_value(&rows))
}

fn base2dec(a: &[Value], base: u32) -> R<Vec<Value>> {
    need(a, 1, "bin2dec")?;
    let rows = match &a[0] {
        Value::Cell(_) => strings_of(&a[0], "bin2dec")?,
        v => v.char_rows().unwrap_or_default(),
    };
    let vals: Vec<f64> = rows
        .iter()
        .map(|r| {
            let t: String = r.chars().filter(|c| !c.is_whitespace()).collect();
            let mut acc = 0f64;
            for c in t.chars() {
                match c.to_digit(base) {
                    Some(d) => acc = acc * base as f64 + d as f64,
                    None => return f64::NAN,
                }
            }
            acc
        })
        .collect();
    if vals.len() == 1 {
        return one(Value::num(vals[0]));
    }
    one(Value::col(vals))
}

fn validatestring(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "validatestring")?;
    let s = arg_str(&a[0], "validatestring")?;
    let opts = strings_of(&a[1], "validatestring")?;
    let ls = s.to_lowercase();
    let hits: Vec<&String> = opts
        .iter()
        .filter(|o| o.to_lowercase().starts_with(&ls))
        .collect();
    if let Some(exact) = opts.iter().find(|o| o.to_lowercase() == ls) {
        return one(Value::str(exact));
    }
    match hits.len() {
        1 => one(Value::str(hits[0])),
        0 => Err(err(
            "MATLAB:unrecognizedStringChoice",
            format!(
                "Expected input to match one of these values:\n\n'{}'\n\nThe input, '{}', did not match any of the valid values.",
                opts.join("', '"),
                s
            ),
        )),
        _ => Err(err(
            "MATLAB:ambiguousStringChoice",
            format!(
                "Expected input to match one of these values:\n\n'{}'\n\nThe input, '{}', matched more than one valid value.",
                opts.join("', '"),
                s
            ),
        )),
    }
}

#[allow(dead_code)]
fn cat_unused(v: Vec<Value>) -> R<Value> {
    concat::cat(v, 1)
}
