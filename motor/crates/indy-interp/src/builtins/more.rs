//! More builtins the compatibility layer relies on: file renaming and
//! listing, handle introspection, piecewise polynomials, delimited text,
//! JSON, and a few numeric helpers.

use indy_value::{Array, Num, StructArray, Value};

use super::core::resolve_path;
use super::{arg_f64, arg_num, arg_str, err, need, one, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "rename", |it, a, nargout| {
        need(&a, 2, "rename")?;
        let from = resolve_path(it, &arg_str(&a[0], "rename")?);
        let to = resolve_path(it, &arg_str(&a[1], "rename")?);
        let (code, msg) = match std::fs::rename(&from, &to) {
            Ok(()) => (0.0, String::new()),
            Err(e) => (-1.0, e.to_string()),
        };
        Ok(vec![Value::num(code), Value::str(&msg)]
            .into_iter()
            .take(nargout.max(1))
            .collect())
    });
    reg(it, "dir", dir);
    reg(it, "ls", |it, a, nargout| {
        let r = dir(it, a, 1)?;
        let Some(Value::Struct(s)) = r.into_iter().next() else {
            return Ok(vec![]);
        };
        let names: Vec<String> = (0..s.numel())
            .filter_map(|i| s.get(i, "name").and_then(|v| v.to_str()))
            .filter(|n| n != "." && n != "..")
            .collect();
        if nargout > 0 {
            return one(super::strings::char_rows_value(&names));
        }
        it.print(&format!("{}\n", names.join("  ")));
        Ok(vec![])
    });
    reg(it, "functions", |_, a, _| {
        need(&a, 1, "functions")?;
        let Value::Func(h) = &a[0] else {
            return Err(err(
                "MATLAB:functions:InvalidInput",
                "functions: input must be a function handle.",
            ));
        };
        let mut s = StructArray::new_scalar();
        match &h.anon {
            Some(an) => {
                s.set(0, "function", Value::str(&an.text));
                s.set(0, "type", Value::str("anonymous"));
                s.set(0, "file", Value::str(""));
                let mut ws = StructArray::new_scalar();
                for (k, v) in &an.captured {
                    ws.set(0, k, v.clone());
                }
                s.set(0, "workspace", Value::cell_row(vec![Value::Struct(ws)]));
            }
            None => {
                let file = h
                    .ctx
                    .clone()
                    .and_then(|c| c.downcast::<crate::FileCtx>().ok())
                    .map(|c| {
                        if c.functions.contains_key(&h.name) && c.name != h.name {
                            c.path.display().to_string()
                        } else {
                            String::new()
                        }
                    })
                    .unwrap_or_default();
                s.set(0, "function", Value::str(&h.name));
                s.set(
                    0,
                    "type",
                    Value::str(if file.is_empty() {
                        "simple"
                    } else {
                        "scopedfunction"
                    }),
                );
                s.set(0, "file", Value::str(&file));
            }
        }
        one(Value::Struct(s))
    });
    reg(it, "mkpp", mkpp);
    reg(it, "unmkpp", unmkpp);
    reg(it, "ppval", ppval);
    reg(it, "rcond", |_, a, _| {
        need(&a, 1, "rcond")?;
        let x = arg_num(&a[0], "rcond")?;
        let n = x.dims[0];
        if x.dims.len() != 2 || x.dims[1] != n {
            return Err(err("MATLAB:square", "rcond: matrix must be square."));
        }
        if n == 0 {
            return one(Value::num(f64::INFINITY));
        }
        let norm1 = |m: &[f64]| {
            (0..n)
                .map(|j| (0..n).map(|i| m[i + j * n].abs()).sum::<f64>())
                .fold(0.0, f64::max)
        };
        let (inv, sing) = indy_value::ops::inverse(&x)?;
        if sing {
            return one(Value::num(0.0));
        }
        one(Value::num(1.0 / (norm1(&x.re) * norm1(&inv.re))))
    });
    reg(it, "sortrows", sortrows);
    reg(it, "speye", |it, a, n| it.call_named("eye", a, n));
    reg(it, "sinc", |_, a, _| {
        need(&a, 1, "sinc")?;
        let x = arg_num(&a[0], "sinc")?;
        let v =
            x.re.iter()
                .map(|t| {
                    if *t == 0.0 {
                        1.0
                    } else {
                        (std::f64::consts::PI * t).sin() / (std::f64::consts::PI * t)
                    }
                })
                .collect();
        one(Value::Num(Num::real(x.dims, v)))
    });
    reg(it, "unwrap", |_, a, _| {
        need(&a, 1, "unwrap")?;
        let x = arg_num(&a[0], "unwrap")?;
        let tol = match a.get(1) {
            Some(v) if !v.is_empty() => arg_f64(v, "unwrap")?,
            _ => std::f64::consts::PI,
        };
        let mut out = x.re.clone();
        let dim = super::math::default_dim(&x.dims);
        super::math::slices(&x.dims, dim, |start, stride, len| {
            let mut off = 0.0;
            for k in 1..len {
                let p = start + k * stride;
                let d = x.re[p] - x.re[p - stride];
                if d > tol {
                    off -= 2.0
                        * std::f64::consts::PI
                        * ((d + std::f64::consts::PI) / (2.0 * std::f64::consts::PI)).floor();
                } else if d < -tol {
                    off += 2.0
                        * std::f64::consts::PI
                        * ((-d + std::f64::consts::PI) / (2.0 * std::f64::consts::PI)).floor();
                }
                out[p] = x.re[p] + off;
            }
        });
        one(Value::Num(Num::real(x.dims, out)))
    });
    reg(it, "diary", |_, _, _| Ok(vec![]));
    reg(it, "csvwrite", |it, a, _| dlmwrite(it, a, ","));
    reg(it, "dlmwrite", |it, a, _| {
        let delim = a
            .get(2)
            .and_then(|v| v.to_str())
            .map(|s| indy_value::format::escapes(&s))
            .unwrap_or_else(|| ",".into());
        let delim = if delim.is_empty() {
            ",".to_string()
        } else {
            delim
        };
        dlmwrite(it, a, &delim)
    });
    reg(it, "csvread", |it, a, _| dlmread(it, a, Some(',')));
    reg(it, "dlmread", |it, a, _| {
        let d = a
            .get(1)
            .and_then(|v| v.to_str())
            .and_then(|s| indy_value::format::escapes(&s).chars().next());
        dlmread(it, a, d)
    });
    reg(it, "jsonencode", |_, a, _| {
        need(&a, 1, "jsonencode")?;
        let pretty = a.windows(2).any(|w| {
            w[0].to_str().is_some_and(|s| s == "PrettyPrint") && w[1].scalar_f64() == Some(1.0)
        });
        let mut s = String::new();
        json_encode(&a[0], &mut s)?;
        if pretty {
            s = pretty_json(&s);
        }
        one(Value::str(&s))
    });
    reg(it, "jsondecode", |_, a, _| {
        need(&a, 1, "jsondecode")?;
        let s: Vec<char> = arg_str(&a[0], "jsondecode")?.chars().collect();
        let mut p = 0;
        let j = parse_json(&s, &mut p)?;
        skip_ws(&s, &mut p);
        if p != s.len() {
            return Err(err(
                "MATLAB:json:ExpectedEOF",
                "jsondecode: unexpected text after JSON value.",
            ));
        }
        one(json_to_value(j))
    });
}

fn dir(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let pat = match a.first() {
        Some(v) => arg_str(v, "dir")?,
        None => ".".into(),
    };
    let p = resolve_path(it, &pat);
    let (folder, filter) = if p.is_dir() {
        (p.clone(), None)
    } else {
        (
            p.parent()
                .map(|x| x.to_path_buf())
                .unwrap_or_else(|| it.cwd.clone()),
            p.file_name().map(|s| s.to_string_lossy().to_string()),
        )
    };
    let glob = |name: &str, pat: &str| -> bool {
        fn m(n: &[char], p: &[char]) -> bool {
            match (p.first(), n.first()) {
                (None, None) => true,
                (Some('*'), _) => m(n, &p[1..]) || (!n.is_empty() && m(&n[1..], p)),
                (Some('?'), Some(_)) => m(&n[1..], &p[1..]),
                (Some(a), Some(b)) if a == b => m(&n[1..], &p[1..]),
                _ => false,
            }
        }
        m(
            &name.chars().collect::<Vec<_>>(),
            &pat.chars().collect::<Vec<_>>(),
        )
    };
    let mut entries: Vec<(String, bool, u64)> = Vec::new();
    if filter.is_none() {
        entries.push((".".into(), true, 0));
        entries.push(("..".into(), true, 0));
    }
    if let Ok(rd) = std::fs::read_dir(&folder) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if let Some(f) = &filter
                && !glob(&name, f)
            {
                continue;
            }
            let md = e.metadata().ok();
            entries.push((
                name,
                md.as_ref().is_some_and(|m| m.is_dir()),
                md.map(|m| m.len()).unwrap_or(0),
            ));
        }
    }
    entries.sort_by(|x, y| x.0.cmp(&y.0));
    let fields = ["name", "folder", "date", "bytes", "isdir", "datenum"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut s = StructArray::empty_with(fields, vec![entries.len(), 1]);
    for (i, (n, d, b)) in entries.iter().enumerate() {
        s.elems[i] = vec![
            Value::str(n),
            Value::str(&folder.display().to_string()),
            Value::str(""),
            Value::num(*b as f64),
            Value::bool(*d),
            Value::num(0.0),
        ];
    }
    if nargout == 0 {
        let names: Vec<String> = entries
            .iter()
            .map(|e| e.0.clone())
            .filter(|n| n != "." && n != "..")
            .collect();
        it.print(&format!("{}\n", names.join("  ")));
        return Ok(vec![]);
    }
    one(Value::Struct(s))
}

fn mkpp(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "mkpp")?;
    let breaks = arg_num(&a[0], "mkpp")?.re;
    let coefs = arg_num(&a[1], "mkpp")?;
    let pieces = breaks.len().saturating_sub(1);
    if pieces == 0 {
        return Err(err(
            "MATLAB:mkpp:InvalidBreaks",
            "mkpp: at least two breaks are required.",
        ));
    }
    let d = match a.get(2) {
        Some(v) => arg_f64(v, "mkpp")? as usize,
        None => 1,
    };
    let total = coefs.re.len();
    let order = total / (pieces * d).max(1);
    if order * pieces * d != total {
        return Err(err(
            "MATLAB:mkpp:CoefsBreaksMismatch",
            "mkpp: coefficients do not match the breaks.",
        ));
    }
    // Coefficients as (pieces*d) x order, row-major by piece.
    let rows = pieces * d;
    let mut c = vec![0.0; rows * order];
    if coefs.dims.len() == 2 && coefs.dims[0] == rows && coefs.dims[1] == order {
        c = coefs.re.clone();
    } else {
        // Treat input as a flat list in column-major (rows x order).
        c[..total].copy_from_slice(&coefs.re[..total]);
    }
    let mut s = StructArray::new_scalar();
    s.set(0, "form", Value::str("pp"));
    s.set(0, "breaks", Value::row(breaks));
    s.set(0, "coefs", Value::matrix(vec![rows, order], c));
    s.set(0, "pieces", Value::num(pieces as f64));
    s.set(0, "order", Value::num(order as f64));
    s.set(0, "dim", Value::num(d as f64));
    s.set(0, "orient", Value::str("first"));
    one(Value::Struct(s))
}

fn pp_parts(v: &Value) -> R<(Vec<f64>, Num, usize, usize, usize)> {
    let Value::Struct(s) = v else {
        return Err(err(
            "MATLAB:ppval:InvalidPP",
            "ppval: first argument must be a piecewise polynomial.",
        ));
    };
    let g = |f: &str| {
        s.get(0, f).cloned().ok_or_else(|| {
            err(
                "MATLAB:ppval:InvalidPP",
                format!("ppval: missing field {}.", f),
            )
        })
    };
    let breaks = g("breaks")?.to_f64_vec().unwrap_or_default();
    let coefs = g("coefs")?.to_num().unwrap_or_else(Num::empty);
    let pieces = g("pieces")?.scalar_f64().unwrap_or(0.0) as usize;
    let order = g("order")?.scalar_f64().unwrap_or(0.0) as usize;
    let dim = g("dim")?.scalar_f64().unwrap_or(1.0) as usize;
    Ok((breaks, coefs, pieces, order, dim))
}

fn unmkpp(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "unmkpp")?;
    let (b, c, p, o, d) = pp_parts(&a[0])?;
    let out = vec![
        Value::row(b),
        Value::Num(c),
        Value::num(p as f64),
        Value::num(o as f64),
        Value::num(d as f64),
    ];
    Ok(out.into_iter().take(nargout.max(1)).collect())
}

fn ppval(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "ppval")?;
    let (b, c, pieces, order, d) = pp_parts(&a[0])?;
    let x = arg_num(&a[1], "ppval")?;
    let rows = pieces * d;
    let mut out = Vec::with_capacity(x.re.len() * d);
    for &xv in &x.re {
        let mut k = 0;
        while k + 1 < pieces && xv >= b[k + 1] {
            k += 1;
        }
        let t = xv - b[k];
        for comp in 0..d {
            let r = k * d + comp;
            let mut acc = 0.0;
            for j in 0..order {
                acc = acc * t + c.re[r + j * rows];
            }
            out.push(if xv.is_nan() { f64::NAN } else { acc });
        }
    }
    if d == 1 {
        return one(Value::Num(Num::real(x.dims, out)));
    }
    let n = x.re.len();
    one(Value::matrix(vec![d, n], out))
}

fn sortrows(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "sortrows")?;
    let cols_spec: Vec<f64> = match a.get(1) {
        Some(v) if v.is_numeric() => v.to_f64_vec().unwrap_or_default(),
        _ => vec![],
    };
    let desc_all = a
        .iter()
        .skip(1)
        .any(|v| v.to_str().is_some_and(|s| s == "descend"));
    match &a[0] {
        Value::Cell(c) => {
            let strs = super::cells::strings_of(&a[0], "sortrows")?;
            let r = c.dims[0];
            let mut idx: Vec<usize> = (0..r).collect();
            idx.sort_by(|&x, &y| {
                if desc_all {
                    strs[y].cmp(&strs[x])
                } else {
                    strs[x].cmp(&strs[y])
                }
            });
            let cols = c.data.len() / r.max(1);
            let mut data = Vec::with_capacity(c.data.len());
            for j in 0..cols {
                for &i in &idx {
                    data.push(c.data[i + j * r].clone());
                }
            }
            let mut out = vec![Value::Cell(Array::new(c.dims.clone(), data))];
            if nargout > 1 {
                out.push(Value::col(idx.iter().map(|i| *i as f64 + 1.0).collect()));
            }
            Ok(out)
        }
        v => {
            let x = arg_num(v, "sortrows")?;
            let (r, cn) = (x.dims[0], x.dims.get(1).copied().unwrap_or(1));
            let spec: Vec<(usize, bool)> = if cols_spec.is_empty() {
                (0..cn).map(|j| (j, desc_all)).collect()
            } else {
                cols_spec
                    .iter()
                    .map(|k| ((k.abs() as usize).max(1) - 1, *k < 0.0 || desc_all))
                    .collect()
            };
            let mut idx: Vec<usize> = (0..r).collect();
            idx.sort_by(|&p, &q| {
                for &(j, desc) in &spec {
                    let (u, w) = (x.re[p + j * r], x.re[q + j * r]);
                    let o = u.partial_cmp(&w).unwrap_or(std::cmp::Ordering::Equal);
                    let o = if desc { o.reverse() } else { o };
                    if o != std::cmp::Ordering::Equal {
                        return o;
                    }
                }
                std::cmp::Ordering::Equal
            });
            let mut data = Vec::with_capacity(x.re.len());
            for j in 0..cn {
                for &i in &idx {
                    data.push(x.re[i + j * r]);
                }
            }
            let mut out = vec![Value::Num(
                Num::real(x.dims.clone(), data).with_class(x.class),
            )];
            if nargout > 1 {
                out.push(Value::col(idx.iter().map(|i| *i as f64 + 1.0).collect()));
            }
            Ok(out)
        }
    }
}

fn dlmwrite(it: &mut Interp, a: Vec<Value>, delim: &str) -> R<Vec<Value>> {
    need(&a, 2, "dlmwrite")?;
    let p = resolve_path(it, &arg_str(&a[0], "dlmwrite")?);
    let x = arg_num(&a[1], "dlmwrite")?;
    let (r, c) = (x.dims[0], x.re.len().checked_div(x.dims[0]).unwrap_or(0));
    let mut s = String::new();
    for i in 0..r {
        let row: Vec<String> = (0..c)
            .map(|j| indy_value::format::fmt_g(x.re[i + j * r], 5, false, false))
            .collect();
        s.push_str(&row.join(delim));
        s.push('\n');
    }
    std::fs::write(&p, s)
        .map_err(|e| err("MATLAB:dlmwrite:FileOpenError", format!("dlmwrite: {}", e)))?;
    Ok(vec![])
}

fn dlmread(it: &mut Interp, a: Vec<Value>, delim: Option<char>) -> R<Vec<Value>> {
    need(&a, 1, "dlmread")?;
    let p = resolve_path(it, &arg_str(&a[0], "dlmread")?);
    let text = std::fs::read_to_string(&p)
        .map_err(|e| err("MATLAB:dlmread:FileNotOpened", format!("dlmread: {}", e)))?;
    let rows: Vec<Vec<f64>> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let parts: Vec<&str> = match delim {
                Some(d) => l.split(d).collect(),
                None => l
                    .split(|ch: char| ch == ',' || ch.is_whitespace())
                    .filter(|s| !s.is_empty())
                    .collect(),
            };
            parts
                .iter()
                .map(|s| s.trim().parse::<f64>().unwrap_or(0.0))
                .collect()
        })
        .collect();
    let r = rows.len();
    let c = rows.iter().map(|x| x.len()).max().unwrap_or(0);
    let mut out = vec![0.0; r * c];
    for (i, row) in rows.iter().enumerate() {
        for (j, v) in row.iter().enumerate() {
            out[i + j * r] = *v;
        }
    }
    one(Value::matrix(vec![r, c], out))
}

// ----- JSON -----------------------------------------------------------------

fn json_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn json_num(x: f64, out: &mut String) {
    if !x.is_finite() {
        out.push_str("null");
    } else if x.fract() == 0.0 && x.abs() < 1e15 {
        out.push_str(&format!("{}", x as i64));
    } else {
        out.push_str(&indy_value::format::fmt_g(x, 15, false, false));
    }
}

fn json_encode(v: &Value, out: &mut String) -> R<()> {
    match v {
        Value::Sparse(s) => json_encode(&s.to_dense()?, out)?,
        Value::Num(n) if n.im.is_some() => {
            return Err(err(
                "MATLAB:jsonencode:unsupportedType",
                "jsonencode: complex values are not supported.",
            ));
        }
        Value::Char(_) => json_str(&v.to_str().unwrap_or_default(), out),
        Value::Str(s) if s.data.len() == 1 => match &s.data[0] {
            Some(t) => json_str(t, out),
            None => out.push_str("null"),
        },
        Value::Bool(b) if b.data.len() == 1 => {
            out.push_str(if b.data[0] { "true" } else { "false" })
        }
        Value::Num(n) if n.re.len() == 1 && n.dims == [1, 1] => json_num(n.re[0], out),
        Value::Num(_) | Value::Bool(_) | Value::Str(_) => {
            let d = v.dims();
            let elems: Vec<Value> = (0..v.numel())
                .map(|i| indy_value::index::index_value(v, &[indy_value::index::Sub::scalar(i)]))
                .collect::<Result<_, _>>()?;
            if d.len() == 2 && (d[0] == 1 || d[1] == 1) {
                out.push('[');
                for (i, e) in elems.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    json_encode(e, out)?;
                }
                out.push(']');
            } else {
                // Matrix: array of rows.
                let (r, c) = (d[0], v.numel() / d[0].max(1));
                out.push('[');
                for i in 0..r {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push('[');
                    for j in 0..c {
                        if j > 0 {
                            out.push(',');
                        }
                        json_encode(&elems[i + j * r], out)?;
                    }
                    out.push(']');
                }
                out.push(']');
            }
        }
        Value::Cell(c) => {
            out.push('[');
            for (i, e) in c.data.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                json_encode(e, out)?;
            }
            out.push(']');
        }
        Value::Struct(s) => {
            let one_obj = |e: &Vec<Value>, out: &mut String| -> R<()> {
                out.push('{');
                for (k, f) in s.fields.iter().enumerate() {
                    if k > 0 {
                        out.push(',');
                    }
                    json_str(f, out);
                    out.push(':');
                    json_encode(&e[k], out)?;
                }
                out.push('}');
                Ok(())
            };
            if s.numel() == 1 {
                one_obj(&s.elems[0], out)?;
            } else {
                out.push('[');
                for (i, e) in s.elems.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    one_obj(e, out)?;
                }
                out.push(']');
            }
        }
        other => {
            return Err(err(
                "MATLAB:jsonencode:unsupportedType",
                format!("jsonencode: unsupported type {}.", other.class_name()),
            ));
        }
    }
    Ok(())
}

fn pretty_json(s: &str) -> String {
    let mut out = String::new();
    let mut ind = 0usize;
    let mut in_str = false;
    let mut esc = false;
    for c in s.chars() {
        if in_str {
            out.push(c);
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                out.push(c);
            }
            '{' | '[' => {
                ind += 1;
                out.push(c);
                out.push('\n');
                out.push_str(&"  ".repeat(ind));
            }
            '}' | ']' => {
                ind = ind.saturating_sub(1);
                out.push('\n');
                out.push_str(&"  ".repeat(ind));
                out.push(c);
            }
            ',' => {
                out.push(c);
                out.push('\n');
                out.push_str(&"  ".repeat(ind));
            }
            ':' => out.push_str(": "),
            c => out.push(c),
        }
    }
    out
}

enum J {
    Null,
    B(bool),
    N(f64),
    S(String),
    A(Vec<J>),
    O(Vec<(String, J)>),
}

fn skip_ws(s: &[char], p: &mut usize) {
    while *p < s.len() && s[*p].is_whitespace() {
        *p += 1;
    }
}

fn parse_json(s: &[char], p: &mut usize) -> R<J> {
    let bad = || err("MATLAB:json:InvalidJSON", "jsondecode: invalid JSON text.");
    skip_ws(s, p);
    let Some(&c) = s.get(*p) else {
        return Err(bad());
    };
    let lit = |p: &mut usize, w: &str, v: J| -> R<J> {
        let wc: Vec<char> = w.chars().collect();
        if s.get(*p..*p + wc.len()) == Some(&wc[..]) {
            *p += wc.len();
            Ok(v)
        } else {
            Err(bad())
        }
    };
    match c {
        'n' => lit(p, "null", J::Null),
        't' => lit(p, "true", J::B(true)),
        'f' => lit(p, "false", J::B(false)),
        '"' => {
            *p += 1;
            let mut out = String::new();
            while *p < s.len() && s[*p] != '"' {
                if s[*p] == '\\' && *p + 1 < s.len() {
                    *p += 1;
                    match s[*p] {
                        'n' => out.push('\n'),
                        't' => out.push('\t'),
                        'r' => out.push('\r'),
                        'b' => out.push('\x08'),
                        'f' => out.push('\x0c'),
                        'u' => {
                            let h: String = s
                                .get(*p + 1..*p + 5)
                                .map(|x| x.iter().collect())
                                .unwrap_or_default();
                            out.push(
                                u32::from_str_radix(&h, 16)
                                    .ok()
                                    .and_then(char::from_u32)
                                    .unwrap_or('\u{FFFD}'),
                            );
                            *p += 4;
                        }
                        o => out.push(o),
                    }
                } else {
                    out.push(s[*p]);
                }
                *p += 1;
            }
            if *p >= s.len() {
                return Err(bad());
            }
            *p += 1;
            Ok(J::S(out))
        }
        '[' => {
            *p += 1;
            let mut v = Vec::new();
            skip_ws(s, p);
            if s.get(*p) == Some(&']') {
                *p += 1;
                return Ok(J::A(v));
            }
            loop {
                v.push(parse_json(s, p)?);
                skip_ws(s, p);
                match s.get(*p) {
                    Some(',') => *p += 1,
                    Some(']') => {
                        *p += 1;
                        return Ok(J::A(v));
                    }
                    _ => return Err(bad()),
                }
            }
        }
        '{' => {
            *p += 1;
            let mut v = Vec::new();
            skip_ws(s, p);
            if s.get(*p) == Some(&'}') {
                *p += 1;
                return Ok(J::O(v));
            }
            loop {
                skip_ws(s, p);
                let J::S(k) = parse_json(s, p)? else {
                    return Err(bad());
                };
                skip_ws(s, p);
                if s.get(*p) != Some(&':') {
                    return Err(bad());
                }
                *p += 1;
                v.push((k, parse_json(s, p)?));
                skip_ws(s, p);
                match s.get(*p) {
                    Some(',') => *p += 1,
                    Some('}') => {
                        *p += 1;
                        return Ok(J::O(v));
                    }
                    _ => return Err(bad()),
                }
            }
        }
        _ => {
            let start = *p;
            while *p < s.len() && (s[*p].is_ascii_digit() || "+-.eE".contains(s[*p])) {
                *p += 1;
            }
            let t: String = s[start..*p].iter().collect();
            t.parse::<f64>().map(J::N).map_err(|_| bad())
        }
    }
}

fn valid_field(k: &str) -> String {
    let mut out: String = k
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if !out.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
        out = format!("x{}", out);
    }
    out
}

fn json_to_value(j: J) -> Value {
    match j {
        J::Null => Value::empty(),
        J::B(b) => Value::bool(b),
        J::N(x) => Value::num(x),
        J::S(s) => Value::str(&s),
        J::O(kv) => {
            let mut s = StructArray::new_scalar();
            for (k, v) in kv {
                s.set(0, &valid_field(&k), json_to_value(v));
            }
            Value::Struct(s)
        }
        J::A(items) => {
            if items.is_empty() {
                return Value::empty();
            }
            if items.iter().all(|x| matches!(x, J::N(_) | J::Null)) {
                let v: Vec<f64> = items
                    .iter()
                    .map(|x| if let J::N(n) = x { *n } else { f64::NAN })
                    .collect();
                return Value::col(v);
            }
            if items.iter().all(|x| matches!(x, J::B(_))) {
                let v: Vec<bool> = items.iter().map(|x| matches!(x, J::B(true))).collect();
                let n = v.len();
                return Value::Bool(Array::new(vec![n, 1], v));
            }
            let vals: Vec<Value> = items.into_iter().map(json_to_value).collect();
            // Array of same-shape numeric rows -> matrix.
            if vals
                .iter()
                .all(|v| matches!(v, Value::Num(n) if n.dims[1] == 1))
                && vals.windows(2).all(|w| w[0].numel() == w[1].numel())
            {
                let r = vals.len();
                let c = vals[0].numel();
                let mut out = vec![0.0; r * c];
                for (i, v) in vals.iter().enumerate() {
                    for (j, x) in v.to_f64_vec().unwrap().iter().enumerate() {
                        out[i + j * r] = *x;
                    }
                }
                return Value::matrix(vec![r, c], out);
            }
            // Array of structs with identical fields -> struct array.
            if let Some(Value::Struct(first)) = vals.first()
                && vals
                    .iter()
                    .all(|v| matches!(v, Value::Struct(s) if s.fields == first.fields))
            {
                let fields = first.fields.clone();
                let n = vals.len();
                let mut sa = StructArray::empty_with(fields, vec![n, 1]);
                for (i, v) in vals.into_iter().enumerate() {
                    if let Value::Struct(s) = v {
                        sa.elems[i] = s.elems.into_iter().next().unwrap_or_default();
                    }
                }
                return Value::Struct(sa);
            }
            Value::cell_col(vals)
        }
    }
}
