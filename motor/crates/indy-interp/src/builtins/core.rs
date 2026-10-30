//! Interpreter-facing builtins: errors, warnings, output, calls,
//! introspection, workspace and path commands.

use std::path::PathBuf;
use std::rc::Rc;

use indy_value::format::{disp_text, display_text, escapes, printf};
use indy_value::{Array, Error, Num, NumClass, Object, StructArray, Value};

use super::{arg_f64, arg_str, err, isequal_values, need, one, reg};
use crate::{Interp, R, error_from_exception, exception_value};

pub fn register(it: &mut Interp) {
    reg(it, "disp", disp);
    reg(it, "display", display);
    reg(it, "fdisp", fdisp);
    reg(it, "sprintf", sprintf);
    reg(it, "fprintf", fprintf);
    reg(it, "printf", printf_b);
    reg(it, "puts", puts);
    reg(it, "fputs", fputs);
    reg(it, "error", error);
    reg(it, "warning", warning);
    reg(it, "lasterr", lasterr);
    reg(it, "lastwarn", lastwarn);
    reg(it, "assert", assert);
    reg(it, "MException", mexception);
    reg(it, "throw", throw);
    reg(it, "rethrow", throw);
    reg(it, "throwAsCaller", |_, a, _| {
        need(&a, 1, "throwAsCaller")?;
        let mut e = error_from_exception(&a[0]);
        e.stack.push(indy_value::StackFrame {
            file: String::new(),
            name: "\0as-caller".into(),
            line: 0,
            column: 0,
        });
        Err(e)
    });
    reg(it, "getReport", get_report);
    reg(it, "addCause", add_cause);
    reg(it, "print_usage", print_usage);
    reg(it, "feval", feval);
    reg(it, "eval", eval);
    reg(it, "evalc", evalc);
    reg(it, "evalin", evalin);
    reg(it, "assignin", assignin);
    reg(it, "exist", exist);
    reg(it, "isvarname", isvarname);
    reg(it, "func2str", func2str);
    reg(it, "str2func", str2func);
    reg(it, "nargin", nargin);
    reg(it, "nargout", nargout_b);
    reg(it, "narginchk", narginchk);
    reg(it, "nargoutchk", nargoutchk);
    reg(it, "builtin", builtin);
    reg(it, "deal", deal);
    reg(it, "class", class);
    reg(it, "isa", isa);
    reg(it, "isnumeric", |_, a, _| {
        pred(a, |v| matches!(v, Value::Num(_)))
    });
    reg(it, "ischar", |_, a, _| {
        pred(a, |v| matches!(v, Value::Char(_)))
    });
    reg(it, "iscell", |_, a, _| {
        pred(a, |v| matches!(v, Value::Cell(_)))
    });
    reg(it, "isstruct", |_, a, _| {
        pred(a, |v| matches!(v, Value::Struct(_)))
    });
    reg(it, "islogical", |_, a, _| {
        pred(a, |v| matches!(v, Value::Bool(_)))
    });
    reg(it, "isbool", |_, a, _| {
        pred(a, |v| matches!(v, Value::Bool(_)))
    });
    reg(it, "isfloat", |_, a, _| {
        pred(
            a,
            |v| matches!(v, Value::Num(n) if matches!(n.class, NumClass::Double | NumClass::Single)),
        )
    });
    reg(it, "isinteger", |_, a, _| {
        pred(a, |v| matches!(v, Value::Num(n) if n.class.is_int()))
    });
    reg(it, "isreal", |_, a, _| {
        pred(a, |v| match v {
            Value::Num(n) => n.im.is_none(),
            Value::Bool(_) | Value::Char(_) => true,
            _ => false,
        })
    });
    reg(it, "is_function_handle", |_, a, _| {
        pred(a, |v| matches!(v, Value::Func(_)))
    });
    reg(it, "isobject", |_, a, _| {
        pred(a, |v| matches!(v, Value::Object(_) | Value::HObject(_)))
    });
    reg(it, "isempty", |_, a, _| pred(a, |v| v.is_empty()));
    reg(it, "isscalar", |_, a, _| pred(a, |v| v.numel() == 1));
    reg(it, "isvector", |_, a, _| {
        pred(a, |v| {
            let d = v.dims_ref();
            d.len() == 2 && (d[0] == 1 || d[1] == 1) && v.numel() >= 1
        })
    });
    reg(it, "ismatrix", |_, a, _| {
        pred(a, |v| v.dims_ref().len() == 2)
    });
    reg(it, "isrow", |_, a, _| {
        pred(a, |v| v.dims_ref().len() == 2 && v.dims_ref()[0] == 1)
    });
    reg(it, "iscolumn", |_, a, _| {
        pred(a, |v| v.dims_ref().len() == 2 && v.dims_ref()[1] == 1)
    });
    reg(it, "issquare", |_, a, _| {
        pred(a, |v| {
            v.dims_ref().len() == 2 && v.dims_ref()[0] == v.dims_ref()[1]
        })
    });
    reg(it, "size", size);
    reg(it, "numel", |_, a, _| {
        need(&a, 1, "numel")?;
        one(Value::num(a[0].numel() as f64))
    });
    reg(it, "ndims", |_, a, _| {
        need(&a, 1, "ndims")?;
        one(Value::num(a[0].dims_ref().len() as f64))
    });
    reg(it, "length", |_, a, _| {
        need(&a, 1, "length")?;
        let n = if a[0].is_empty() {
            0
        } else {
            *a[0].dims_ref().iter().max().unwrap()
        };
        one(Value::num(n as f64))
    });
    reg(it, "rows", |_, a, _| {
        need(&a, 1, "rows")?;
        one(Value::num(a[0].dims_ref()[0] as f64))
    });
    reg(it, "columns", |_, a, _| {
        need(&a, 1, "columns")?;
        one(Value::num(a[0].dims_ref()[1] as f64))
    });
    reg(it, "isequal", |_, a, _| isequal_b(a, false));
    reg(it, "isequaln", |_, a, _| isequal_b(a, true));
    reg(it, "tic", tic);
    reg(it, "toc", toc);
    reg(it, "clear", clear);
    reg(it, "clearvars", clear);
    reg(it, "clc", |_, _, _| Ok(vec![]));
    reg(it, "more", |_, _, _| Ok(vec![]));
    reg(it, "beep", |_, _, _| Ok(vec![]));
    reg(it, "format", format_cmd);
    reg(it, "addpath", addpath);
    reg(it, "rmpath", rmpath);
    reg(it, "path", path_b);
    reg(it, "cd", cd);
    reg(it, "pwd", |it, _, _| {
        one(Value::str(&it.cwd.display().to_string()))
    });
    reg(it, "mfilename", mfilename);
    reg(it, "which", which);
    reg(it, "isdeployed", |_, _, _| one(Value::bool(false)));
    reg(it, "ispc", |_, _, _| one(Value::bool(cfg!(windows))));
    reg(it, "isunix", |_, _, _| one(Value::bool(cfg!(unix))));
    reg(it, "ismac", |_, _, _| {
        one(Value::bool(cfg!(target_os = "macos")))
    });
    reg(it, "filesep", |_, _, _| {
        one(Value::str(std::path::MAIN_SEPARATOR_STR))
    });
    reg(it, "pathsep", |_, _, _| {
        one(Value::str(if cfg!(windows) { ";" } else { ":" }))
    });
    reg(it, "inputname", inputname);
    reg(it, "run", run);
    reg(it, "import", |it, a, nargout| {
        for v in &a {
            let s = arg_str(v, "import")?;
            let f = it.frame_mut();
            if !f.imports.contains(&s) {
                f.imports.push(s);
            }
        }
        if nargout > 0 || a.is_empty() {
            let l: Vec<Value> = it.frame().imports.iter().map(|s| Value::str(s)).collect();
            return one(Value::cell_col(l));
        }
        Ok(vec![])
    });
    reg(it, "source", run);
    reg(it, "isargout", |it, a, _| {
        need(&a, 1, "isargout")?;
        let k = arg_f64(&a[0], "isargout")?;
        let n = it.frame().nargout;
        one(Value::bool(k >= 1.0 && (k as usize <= n.max(1))))
    });
}

fn pred(a: Vec<Value>, f: impl Fn(&Value) -> bool) -> R<Vec<Value>> {
    need(&a, 1, "is*")?;
    one(Value::bool(f(&a[0])))
}

// ----- output ------------------------------------------------------------

fn disp(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "disp")?;
    let t = disp_text(&a[0]);
    it.print(&t);
    Ok(vec![])
}

fn display(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "display")?;
    let t = display_text("ans", &a[0]);
    it.print(&t);
    Ok(vec![])
}

fn fdisp(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "fdisp")?;
    let t = disp_text(&a[1]);
    write_fid(it, &a[0], &t)?;
    Ok(vec![])
}

fn format_args(a: &[Value], fname: &str) -> R<String> {
    need(a, 1, fname)?;
    let f = match &a[0] {
        Value::Char(_) => a[0].to_str().unwrap(),
        // A string scalar is a valid format, like a char row.
        Value::Str(_) if a[0].is_text_scalar() => match a[0].to_str() {
            Some(f) => f,
            None => {
                return Err(err(
                    "MATLAB:sprintf:invalidFormat",
                    format!("{}: format must not be a missing string.", fname),
                ));
            }
        },
        Value::Num(_) | Value::Bool(_) => {
            // sprintf(5) prints the number's char; MATLAB requires text.
            return Err(err(
                "MATLAB:sprintf:invalidFormat",
                format!("{}: format must be a character vector.", fname),
            ));
        }
        other => {
            return Err(err(
                "MATLAB:sprintf:invalidFormat",
                format!(
                    "{}: format must be text, not {}.",
                    fname,
                    other.class_name()
                ),
            ));
        }
    };
    printf(&escapes(&f), &a[1..])
}

fn sprintf(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let s = format_args(&a, "sprintf")?;
    one(Value::str(&s))
}

pub fn write_fid(it: &mut Interp, fid: &Value, s: &str) -> R<()> {
    let id = arg_f64(fid, "fprintf")? as i32;
    match id {
        1 => it.print(s),
        2 => it.eprint(s),
        _ => super::io::write_to(it, id, s)?,
    }
    Ok(())
}

fn fprintf(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "fprintf")?;
    let (fid, rest) = if matches!(a[0], Value::Num(_)) && a[0].is_scalar() {
        (a[0].clone(), &a[1..])
    } else {
        (Value::num(1.0), &a[..])
    };
    let s = if rest.is_empty() {
        String::new()
    } else {
        format_args(rest, "fprintf")?
    };
    write_fid(it, &fid, &s)?;
    if nargout > 0 {
        return one(Value::num(s.len() as f64));
    }
    Ok(vec![])
}

fn printf_b(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let s = format_args(&a, "printf")?;
    it.print(&s);
    Ok(vec![])
}

fn puts(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "puts")?;
    let s = arg_str(&a[0], "puts")?;
    it.print(&s);
    Ok(vec![])
}

fn fputs(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "fputs")?;
    let s = arg_str(&a[1], "fputs")?;
    write_fid(it, &a[0], &s)?;
    Ok(vec![])
}

// ----- errors ------------------------------------------------------------

/// Split `(id, fmt, args)` / `(msg)` like MATLAB's error/warning.
pub fn id_and_message(a: &[Value], fname: &str) -> R<(String, String)> {
    need(a, 1, fname)?;
    let first = arg_str(&a[0], fname)?;
    let looks_like_id = |s: &str| {
        s.contains(':')
            && !s.contains(char::is_whitespace)
            && !s.ends_with(':')
            && !s.starts_with(':')
            && s.split(':').all(|p| !p.is_empty())
    };
    if a.len() >= 2 && looks_like_id(&first) {
        let fmt = arg_str(&a[1], fname)?;
        let msg = printf(&escapes(&fmt), &a[2..])?;
        return Ok((first, msg));
    }
    if a.len() == 1 {
        return Ok((String::new(), first));
    }
    let msg = printf(&escapes(&first), &a[1..])?;
    Ok((String::new(), msg))
}

fn error(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "error")?;
    match &a[0] {
        Value::Struct(_) | Value::Object(_) => {
            if let Value::Struct(s) = &a[0]
                && s.numel() == 0
            {
                return Ok(vec![]);
            }
            return Err(error_from_exception(&a[0]));
        }
        _ => {}
    }
    let (id, msg) = id_and_message(&a, "error")?;
    Err(Error::new(&id, msg))
}

fn warn_state(it: &Interp, id: &str) -> Value {
    let on = if id == "all" {
        !it.warn.all_off
    } else {
        !(it.warn.disabled.contains(id)
            || (it.warn.all_off && !it.warn.disabled.contains(&format!("+{}", id))))
    };
    let mut s = StructArray::new_scalar();
    s.set(0, "identifier", Value::str(id));
    s.set(0, "state", Value::str(if on { "on" } else { "off" }));
    Value::Struct(s)
}

fn warning(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    if a.is_empty() {
        return Ok(vec![]);
    }
    if let Value::Struct(s) = &a[0] {
        // Restore a saved state array.
        for i in 0..s.numel() {
            let id = s
                .get(i, "identifier")
                .and_then(|v| v.to_str())
                .unwrap_or_default();
            let st = s
                .get(i, "state")
                .and_then(|v| v.to_str())
                .unwrap_or_default();
            set_warn(it, &id, st == "on");
        }
        return Ok(vec![]);
    }
    let first = arg_str(&a[0], "warning")?;
    let lower = first.to_ascii_lowercase();
    if matches!(lower.as_str(), "on" | "off" | "query" | "error") && (a.len() <= 2) {
        let id = if a.len() == 2 {
            arg_str(&a[1], "warning")?
        } else {
            "all".into()
        };
        let prev = warn_state(it, &id);
        match lower.as_str() {
            "on" => set_warn(it, &id, true),
            "off" => set_warn(it, &id, false),
            _ => {}
        }
        if nargout > 0 || lower == "query" {
            return one(prev);
        }
        return Ok(vec![]);
    }
    let (id, msg) = id_and_message(&a, "warning")?;
    it.warning(&id, &msg);
    Ok(vec![])
}

fn set_warn(it: &mut Interp, id: &str, on: bool) {
    if id == "all" {
        it.warn.all_off = !on;
        it.warn.disabled.clear();
        return;
    }
    if on {
        it.warn.disabled.remove(id);
        if it.warn.all_off {
            it.warn.disabled.insert(format!("+{}", id));
        }
    } else {
        it.warn.disabled.insert(id.to_string());
        it.warn.disabled.remove(&format!("+{}", id));
    }
}

fn lasterr(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let prev = Value::str(&it.lasterr);
    if let Some(v) = a.first() {
        it.lasterr = arg_str(v, "lasterr")?;
    }
    one(prev)
}

fn lastwarn(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let msg = Value::str(&it.warn.last_msg);
    let id = Value::str(&it.warn.last_id);
    if let Some(v) = a.first() {
        it.warn.last_msg = arg_str(v, "lastwarn")?;
        it.warn.last_id = match a.get(1) {
            Some(v) => arg_str(v, "lastwarn")?,
            None => String::new(),
        };
    }
    if nargout >= 2 {
        return Ok(vec![msg, id]);
    }
    one(msg)
}

fn assert(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "assert")?;
    let message_mode =
        a.len() == 1 || ((a[1].is_char() || a[1].is_text_scalar()) && !a[0].is_text());
    if message_mode {
        let ok = match &a[0] {
            Value::Num(_) | Value::Bool(_) | Value::Char(_) => {
                let d = indy_value::ops::to_logical_data(&a[0])?;
                !d.is_empty() && d.iter().all(|b| *b) || (d.is_empty() && false)
            }
            other => {
                return Err(err(
                    "MATLAB:assertion:invalidCondition",
                    format!(
                        "assert: condition must be logical, not {}.",
                        other.class_name()
                    ),
                ));
            }
        };
        if ok {
            return Ok(vec![]);
        }
        if a.len() == 1 {
            return Err(err("MATLAB:assertion:failed", "Assertion failed."));
        }
        let (id, msg) = id_and_message(&a[1..], "assert")?;
        return Err(Error::new(&id, msg));
    }
    // Octave form: assert(observed, expected [, tol]).
    let tol = match a.get(2) {
        Some(t) => arg_f64(t, "assert")?,
        None => 0.0,
    };
    let (obs, exp) = (&a[0], &a[1]);
    let ok = if tol == 0.0 {
        isequal_values(obs, exp, true)
            || (obs.is_numeric() && exp.is_numeric() && same_numbers(obs, exp, 0.0))
    } else {
        same_numbers(obs, exp, tol)
    };
    if ok {
        return Ok(vec![]);
    }
    Err(err(
        "Octave:assert-failed",
        format!(
            "ASSERT errors for:  assert (cond,expected,tol)\n  Location  |  Observed  |  Expected\n  {} | {}",
            indy_value::format::inline_text(obs),
            indy_value::format::inline_text(exp)
        ),
    ))
}

fn same_numbers(a: &Value, b: &Value, tol: f64) -> bool {
    let (Some(x), Some(y)) = (a.to_num(), b.to_num()) else {
        return false;
    };
    let scalar_b = y.re.len() == 1;
    if x.dims != y.dims && !scalar_b {
        return false;
    }
    for i in 0..x.re.len() {
        let j = if scalar_b { 0 } else { i };
        let (p, q) = (x.re[i], y.re[j]);
        if p.is_nan() && q.is_nan() {
            continue;
        }
        if p == q {
            continue;
        }
        let d = (p - q).abs() + (x.im_at(i) - y.im_at(j)).abs();
        let lim = if tol < 0.0 { tol.abs() * q.abs() } else { tol };
        if d > lim || d.is_nan() {
            return false;
        }
    }
    true
}

fn mexception(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "MException")?;
    let id = arg_str(&a[0], "MException")?;
    let fmt = arg_str(&a[1], "MException")?;
    let msg = printf(&escapes(&fmt), &a[2..])?;
    one(make_exception(&id, &msg))
}

pub fn make_exception(id: &str, msg: &str) -> Value {
    let mut o = Object::new("MException");
    o.set("identifier", Value::str(id));
    o.set("message", Value::str(msg));
    o.set(
        "stack",
        Value::Struct(StructArray::empty_with(
            vec!["file".into(), "name".into(), "line".into(), "column".into()],
            vec![0, 1],
        )),
    );
    o.set("cause", Value::Cell(Array::new(vec![0, 1], vec![])));
    Value::Object(Box::new(o))
}

fn throw(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "throw")?;
    Err(error_from_exception(&a[0]))
}

fn get_report(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "getReport")?;
    let basic = a
        .get(1)
        .and_then(|v| v.to_str())
        .is_some_and(|s| s == "basic");
    fn report(v: &Value, basic: bool, depth: usize) -> String {
        let Value::Object(o) = v else {
            return String::new();
        };
        let msg = o
            .get("message")
            .and_then(|v| v.to_str())
            .unwrap_or_default();
        let mut s = msg;
        if !basic {
            if let Some(Value::Struct(st)) = o.get("stack") {
                for i in 0..st.numel() {
                    let name = st
                        .get(i, "name")
                        .and_then(|v| v.to_str())
                        .unwrap_or_default();
                    let line = st
                        .get(i, "line")
                        .and_then(|v| v.scalar_f64())
                        .unwrap_or(0.0);
                    s.push_str(&format!("\nError in {} (line {})", name, line));
                }
            }
            if let Some(Value::Cell(c)) = o.get("cause")
                && !c.data.is_empty()
            {
                s.push_str("\n\nCaused by:");
                for cause in &c.data {
                    let inner = report(cause, basic, depth + 1);
                    for line in inner.lines() {
                        s.push_str(&format!("\n{}{}", "    ".repeat(depth + 1), line));
                    }
                }
            }
        }
        s
    }
    one(Value::str(&report(&a[0], basic, 0)))
}

fn add_cause(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "addCause")?;
    let Value::Object(o) = &a[0] else {
        return Err(err(
            "MATLAB:addCause:invalid",
            "addCause requires an MException.",
        ));
    };
    let mut o = o.clone();
    let mut causes = match o.get("cause") {
        Some(Value::Cell(c)) => c.data.clone(),
        _ => vec![],
    };
    causes.push(a[1].clone());
    let n = causes.len();
    o.set("cause", Value::Cell(Array::new(vec![n, 1], causes)));
    one(Value::Object(o))
}

fn print_usage(it: &mut Interp, _: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let name = it.frame().fname.clone();
    Err(err(
        "Octave:invalid-fun-call",
        format!("Invalid call to {}", name),
    ))
}

// ----- calls -------------------------------------------------------------

fn feval(it: &mut Interp, mut a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "feval")?;
    let f = a.remove(0);
    it.call_value(&f, a, nargout)
}

fn eval_text(it: &mut Interp, text: &str, nargout: usize) -> R<Vec<Value>> {
    if nargout > 0 {
        // Expression form: x = eval('1 + 2').
        let ctx = crate::load_source(
            it.frame().ctx.path.clone(),
            it.frame().ctx.name.clone(),
            text,
        )?;
        if let [s] = ctx.stmts.as_slice()
            && let indy_syntax::ast::StmtKind::Expr(e) = &s.kind
        {
            let saved = std::mem::replace(&mut it.frame_mut().ctx, ctx.clone());
            let r = it.eval_multi(e, nargout);
            it.frame_mut().ctx = saved;
            return r;
        }
    }
    it.run_text(text)
}

fn eval(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "eval")?;
    let text = arg_str(&a[0], "eval")?;
    if a.len() >= 2 {
        let catch_text = arg_str(&a[1], "eval")?;
        let nframes = it.frames.len();
        let depth = it.depth;
        it.debug.try_depth += 1;
        let tried = eval_text(it, &text, nargout);
        it.debug.try_depth -= 1;
        return match tried {
            Ok(v) => Ok(v),
            Err(e) => {
                it.frames.truncate(nframes);
                it.depth = depth;
                it.lasterr = e.message.clone();
                eval_text(it, &catch_text, nargout)
            }
        };
    }
    eval_text(it, &text, nargout)
}

fn evalc(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "evalc")?;
    let text = arg_str(&a[0], "evalc")?;
    let prev = it.capture.replace(String::new());
    let r = if nargout > 1 {
        eval_text(it, &text, nargout - 1)
    } else {
        it.run_text(&text)
    };
    let out = std::mem::replace(&mut it.capture, prev).unwrap_or_default();
    let mut vals = r?;
    vals.insert(0, Value::str(&out));
    vals.truncate(nargout.max(1));
    Ok(vals)
}

fn evalin(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "evalin")?;
    let ws = arg_str(&a[0], "evalin")?;
    let text = arg_str(&a[1], "evalin")?;
    let n = it.frames.len();
    let target = match ws.as_str() {
        "base" => 0,
        "caller" => n.saturating_sub(2),
        _ => {
            return Err(err(
                "MATLAB:evalin:badWorkspace",
                "evalin: workspace must be 'base' or 'caller'.",
            ));
        }
    };
    // Temporarily run on the target frame by moving it to the top.
    if target + 1 == n {
        return eval_text(it, &text, nargout);
    }
    let moved = it.frames.split_off(target + 1);
    let r = eval_text(it, &text, nargout);
    it.frames.extend(moved);
    r
}

fn assignin(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 3, "assignin")?;
    let ws = arg_str(&a[0], "assignin")?;
    let name = arg_str(&a[1], "assignin")?;
    let n = it.frames.len();
    let target = match ws.as_str() {
        "base" => 0,
        "caller" => n.saturating_sub(2),
        _ => {
            return Err(err(
                "MATLAB:assignin:badWorkspace",
                "assignin: workspace must be 'base' or 'caller'.",
            ));
        }
    };
    it.frames[target].vars.insert(name, a[2].clone());
    Ok(vec![])
}

fn exist(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "exist")?;
    let name = arg_str(&a[0], "exist")?;
    let kind = match a.get(1) {
        Some(v) => arg_str(v, "exist")?,
        None => String::new(),
    };
    let want = |k: &str| kind.is_empty() || kind == k;
    if want("var") && it.has_var(&name) {
        return one(Value::num(1.0));
    }
    if want("file") || kind.is_empty() {
        if it.frame().ctx.functions.contains_key(&name) && kind.is_empty() {
            return one(Value::num(2.0));
        }
        if it.find_file(&name).is_some() {
            return one(Value::num(2.0));
        }
        // The engine's built-in datatypes classes stand where Octave's
        // datatypes package keeps its classdef files.
        if super::datatypes::is_class_file(&name) {
            return one(Value::num(2.0));
        }
        let p = resolve_path(it, &name);
        if p.is_file() {
            return one(Value::num(2.0));
        }
        if p.is_dir() && (kind.is_empty() || kind == "dir" || kind == "file") {
            return one(Value::num(7.0));
        }
    }
    if kind == "dir" && resolve_path(it, &name).is_dir() {
        return one(Value::num(7.0));
    }
    if (want("builtin")) && it.builtins.contains_key(name.as_str()) {
        return one(Value::num(5.0));
    }
    one(Value::num(0.0))
}

pub fn resolve_path(it: &Interp, name: &str) -> PathBuf {
    let p = PathBuf::from(name);
    if p.is_absolute() { p } else { it.cwd.join(p) }
}

fn isvarname(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "isvarname")?;
    let ok = a[0].to_str().is_some_and(|s| is_varname(&s));
    one(Value::bool(ok))
}

pub fn is_varname(s: &str) -> bool {
    let mut c = s.chars();
    matches!(c.next(), Some(ch) if ch.is_ascii_alphabetic())
        && c.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        && s.len() <= 63
        && !KEYWORDS.contains(&s)
}

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

fn func2str(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "func2str")?;
    match &a[0] {
        Value::Func(h) => one(Value::str(&match &h.anon {
            Some(an) => an.text.clone(),
            None => h.name.clone(),
        })),
        _ => Err(err(
            "MATLAB:func2str:InputArg",
            "func2str: input must be a function handle.",
        )),
    }
}

fn str2func(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "str2func")?;
    let s = arg_str(&a[0], "str2func")?;
    let s = s.trim();
    if s.starts_with('@') {
        let ctx = crate::load_source(it.frame().ctx.path.clone(), "str2func".into(), s)?;
        if let [st] = ctx.stmts.as_slice()
            && let indy_syntax::ast::StmtKind::Expr(e) = &st.kind
        {
            // Evaluate in an empty workspace (no captured variables).
            let mut f = crate::Frame::new(ctx.clone(), "str2func");
            f.nargin = 0;
            it.frames.push(f);
            let r = it.eval(e);
            it.frames.pop();
            return one(r?);
        }
        return Err(err(
            "MATLAB:str2func:invalid",
            "str2func: invalid function text.",
        ));
    }
    one(Value::Func(Rc::new(indy_value::Handle {
        name: s.to_string(),
        anon: None,
        ctx: Some(it.frame().ctx.clone() as Rc<dyn std::any::Any>),
    })))
}

fn param_counts(it: &mut Interp, f: &Value) -> R<(f64, f64)> {
    let count =
        |params: usize, var: bool| -> f64 { if var { -(params as f64) } else { params as f64 } };
    match f {
        Value::Func(h) if h.anon.is_some() => {
            let an = h.anon.as_ref().unwrap();
            let var = an.params.last().is_some_and(|p| p == "varargin");
            Ok((count(an.params.len(), var), 1.0))
        }
        _ => {
            let name = match f {
                Value::Func(h) => h.name.clone(),
                _ => arg_str(f, "nargin")?,
            };
            match it.resolve(&name)? {
                Some(crate::Callee::User(_, func)) => {
                    let var = matches!(func.params.last(), Some(indy_syntax::ast::Param::Name(n)) if n.name == "varargin");
                    let vout = func.outputs.last().is_some_and(|o| o.name == "varargout");
                    Ok((
                        count(func.params.len(), var),
                        count(func.outputs.len(), vout),
                    ))
                }
                Some(_) => Ok((-1.0, -1.0)),
                None => Err(Interp::undefined(&name)),
            }
        }
    }
}

fn nargin(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    if let Some(f) = a.first() {
        let (i, _) = param_counts(it, f)?;
        return one(Value::num(i));
    }
    one(Value::num(it.frame().nargin as f64))
}

fn nargout_b(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    if let Some(f) = a.first() {
        let (_, o) = param_counts(it, f)?;
        return one(Value::num(o));
    }
    one(Value::num(it.frame().nargout as f64))
}

fn narginchk(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "narginchk")?;
    let lo = arg_f64(&a[0], "narginchk")?;
    let hi = arg_f64(&a[1], "narginchk")?;
    let n = it.frame().nargin as f64;
    if n < lo {
        return Err(err(
            "MATLAB:narginchk:notEnoughInputs",
            "Not enough input arguments.",
        ));
    }
    if n > hi {
        return Err(err(
            "MATLAB:narginchk:tooManyInputs",
            "Too many input arguments.",
        ));
    }
    Ok(vec![])
}

fn nargoutchk(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "nargoutchk")?;
    let lo = arg_f64(&a[0], "nargoutchk")?;
    let hi = arg_f64(&a[1], "nargoutchk")?;
    let n = it.frame().nargout as f64;
    if n < lo {
        return Err(err(
            "MATLAB:nargoutchk:notEnoughOutputs",
            "Not enough output arguments.",
        ));
    }
    if n > hi {
        return Err(err(
            "MATLAB:nargoutchk:tooManyOutputs",
            "Too many output arguments.",
        ));
    }
    Ok(vec![])
}

fn builtin(it: &mut Interp, mut a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "builtin")?;
    let name = arg_str(&a.remove(0), "builtin")?;
    match it.builtins.get(name.as_str()).copied() {
        Some(f) if a.iter().any(Value::is_sparse) => super::sparse::call(it, &name, f, a, nargout),
        Some(f) => f(it, a, nargout),
        None => Err(err(
            "MATLAB:builtin:notFound",
            format!("builtin: '{}' is not a built-in function.", name),
        )),
    }
}

fn deal(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let n = nargout.max(1);
    if a.len() == 1 {
        return Ok(vec![a[0].clone(); n]);
    }
    if a.len() != n {
        return Err(err(
            "MATLAB:deal:narginNargoutMismatch",
            "The number of outputs should match the number of inputs.",
        ));
    }
    Ok(a)
}

fn class(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "class")?;
    one(Value::str(&a[0].class_name()))
}

pub fn isa_name(v: &Value, c: &str) -> bool {
    let cls = v.class_name();
    if cls == c {
        return true;
    }
    if let Some(info) = crate::classes::class_of(v) {
        return info.is_a(c);
    }
    if c == "handle" && matches!(v, Value::HObject(_)) {
        return true;
    }
    match c {
        "numeric" => matches!(v, Value::Num(_)),
        "float" => {
            matches!(v, Value::Num(n) if matches!(n.class, NumClass::Double | NumClass::Single))
        }
        "integer" => matches!(v, Value::Num(n) if n.class.is_int()),
        "handle" => false,
        _ => false,
    }
}

fn isa(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "isa")?;
    let c = arg_str(&a[1], "isa")?;
    one(Value::bool(isa_name(&a[0], &c)))
}

fn size(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "size")?;
    let d = a[0].dims();
    let at = |k: usize| *d.get(k).unwrap_or(&1) as f64;
    if a.len() >= 2 {
        let mut out = Vec::new();
        for v in &a[1..] {
            for k in v.to_f64_vec().unwrap_or_default() {
                if k < 1.0 || k.fract() != 0.0 {
                    return Err(err(
                        "MATLAB:getdimarg:dimensionMustBePositiveInteger",
                        "Dimension argument must be a positive integer scalar.",
                    ));
                }
                out.push(at(k as usize - 1));
            }
        }
        if nargout <= 1 {
            return one(Value::row(out));
        }
        return Ok(out.into_iter().map(Value::num).collect());
    }
    if nargout <= 1 {
        return one(Value::row(d.iter().map(|x| *x as f64).collect()));
    }
    let mut out = Vec::with_capacity(nargout);
    for k in 0..nargout {
        if k + 1 == nargout {
            let rest: usize = d.iter().skip(k).product();
            out.push(Value::num(if k >= d.len() { 1.0 } else { rest as f64 }));
        } else {
            out.push(Value::num(at(k)));
        }
    }
    Ok(out)
}

fn isequal_b(a: Vec<Value>, nan: bool) -> R<Vec<Value>> {
    need(&a, 2, "isequal")?;
    let ok = a.windows(2).all(|w| isequal_values(&w[0], &w[1], nan));
    one(Value::bool(ok))
}

fn tic(it: &mut Interp, _: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let now = std::time::Instant::now();
    it.tic = Some(now);
    if nargout > 0 {
        let us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_micros() as f64)
            .unwrap_or(0.0);
        return one(Value::Num(Num::scalar(us).with_class(NumClass::UInt64)));
    }
    Ok(vec![])
}

fn toc(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let secs = if let Some(t0) = a.first() {
        let us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_micros() as f64)
            .unwrap_or(0.0);
        (us - arg_f64(t0, "toc")?) / 1e6
    } else {
        match it.tic {
            Some(t) => t.elapsed().as_secs_f64(),
            None => {
                return Err(err(
                    "MATLAB:toc:callTicFirst",
                    "You must call TIC without an output argument before calling TOC without an input argument.",
                ));
            }
        }
    };
    if nargout > 0 {
        return one(Value::num(secs));
    }
    it.print(&format!("Elapsed time is {:.6} seconds.\n", secs));
    Ok(vec![])
}

fn clear(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let names: Vec<String> = a.iter().filter_map(|v| v.to_str()).collect();
    if names.is_empty()
        || names
            .iter()
            .any(|n| n == "all" || n == "-all" || n == "variables" || n == "-v")
    {
        let f = it.frame_mut();
        let vals: Vec<Value> = f.vars.drain().map(|(_, v)| v).collect();
        f.globals.clear();
        it.run_destructors(vals);
        return Ok(vec![]);
    }
    for n in names {
        if n == "functions" || n == "-f" || n == "global" || n == "-g" || n == "classes" {
            continue;
        }
        if n.contains('*') {
            let pre = n.trim_end_matches('*').to_string();
            let f = it.frame_mut();
            f.vars.retain(|k, _| !k.starts_with(&pre));
            continue;
        }
        if let Some(v) = it.clear_var(&n) {
            it.run_destructors(vec![v]);
        }
    }
    Ok(vec![])
}

fn format_cmd(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let prev = Value::str(if it.format_long { "long" } else { "short" });
    if a.is_empty() && nargout > 0 {
        return one(prev);
    }
    let mode = a
        .first()
        .and_then(|v| v.to_str())
        .unwrap_or_else(|| "short".into());
    it.format_long = mode.to_ascii_lowercase().starts_with("long");
    if nargout > 0 {
        return one(prev);
    }
    Ok(vec![])
}

fn addpath(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let mut front = true;
    let mut dirs = Vec::new();
    for v in &a {
        let s = arg_str(v, "addpath")?;
        match s.as_str() {
            "-end" | "-END" => front = false,
            "-begin" | "-BEGIN" | "-frozen" => {}
            _ => {
                for part in s.split(if cfg!(windows) { ';' } else { ':' }) {
                    if !part.is_empty() {
                        dirs.push(resolve_path(it, part));
                    }
                }
            }
        }
    }
    if front {
        dirs.reverse();
    }
    for d in dirs {
        it.add_path(d, front);
    }
    if nargout > 0 {
        return path_b(it, vec![], 1);
    }
    Ok(vec![])
}

fn rmpath(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    for v in &a {
        let s = arg_str(v, "rmpath")?;
        let p = resolve_path(it, &s);
        it.remove_path(&p);
    }
    Ok(vec![])
}

fn path_b(it: &mut Interp, _: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let s: Vec<String> = it.path.iter().map(|p| p.display().to_string()).collect();
    one(Value::str(&s.join(if cfg!(windows) { ";" } else { ":" })))
}

fn cd(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let prev = Value::str(&it.cwd.display().to_string());
    if let Some(v) = a.first() {
        let s = arg_str(v, "cd")?;
        let p = resolve_path(it, &s);
        if !p.is_dir() {
            return Err(err(
                "MATLAB:cd:NonExistentFolder",
                format!(
                    "cd: Cannot CD to {} (Name is nonexistent or not a folder).",
                    s
                ),
            ));
        }
        let p = p.canonicalize().unwrap_or(p);
        it.set_cwd(p);
    }
    one(prev)
}

fn mfilename(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let ctx = it.frame().ctx.clone();
    let opt = a.first().and_then(|v| v.to_str()).unwrap_or_default();
    if opt == "fullpath" || opt == "fullpathext" {
        let mut p = ctx.path.clone();
        if opt == "fullpath" {
            p.set_extension("");
        }
        return one(Value::str(&p.display().to_string()));
    }
    if ctx.name == "base" {
        return one(Value::str(""));
    }
    one(Value::str(&ctx.name))
}

fn which(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "which")?;
    let name = arg_str(&a[0], "which")?;
    if it.has_var(&name) {
        return one(Value::str("variable"));
    }
    if let Some(p) = it.find_file(&name) {
        return one(Value::str(&p.display().to_string()));
    }
    if it.builtins.contains_key(name.as_str()) {
        return one(Value::str(&format!("built-in ({})", name)));
    }
    one(Value::str(""))
}

/// `inputname(k)`: the caller's variable name of argument `k`, or `''`
/// when the argument was not a plain variable.
fn inputname(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let k = match a.first().and_then(Value::scalar_f64) {
        Some(k) if k >= 1.0 && k.fract() == 0.0 => k as usize,
        _ => {
            return Err(err(
                "MATLAB:inputname:invalidArgNumber",
                "inputname: argument number must be a positive integer.",
            ));
        }
    };
    let f = it.frame();
    if it.frames.len() < 2 {
        return Err(err(
            "MATLAB:inputname:notInFunction",
            "inputname: must be called from within a function.",
        ));
    }
    if k > f.nargin {
        return Err(err(
            "MATLAB:inputname:argNumberTooLarge",
            "inputname: argument number exceeds the number of inputs.",
        ));
    }
    let name = f.arg_names.get(k - 1).cloned().flatten();
    one(Value::str(&name.unwrap_or_default()))
}

fn run(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "run")?;
    let s = arg_str(&a[0], "run")?;
    let mut p = resolve_path(it, &s);
    if p.extension().is_none() {
        p.set_extension("m");
    }
    if !p.is_file() {
        if let Some(f) = it.find_file(&s) {
            p = f;
        } else {
            return Err(err(
                "MATLAB:run:FileNotFound",
                format!("run: file '{}' not found.", s),
            ));
        }
    }
    let ctx = it.load_file(&p)?;
    let dir = p.parent().map(|d| d.to_path_buf());
    let saved = it.cwd.clone();
    if let Some(d) = dir {
        it.set_cwd(d);
    }
    let r = it.run_script_ctx(ctx);
    it.set_cwd(saved);
    r?;
    Ok(vec![])
}

#[allow(dead_code)]
fn exception_of(it: &Interp, e: &Error) -> Value {
    exception_value(it, e)
}
