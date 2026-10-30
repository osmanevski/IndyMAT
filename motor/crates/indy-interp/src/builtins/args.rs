//! MATLAB `arguments` blocks: binding (name-value options, Repeating),
//! size/class coercion, validator functions and `mustBe*` builtins.

use std::rc::Rc;

use indy_syntax::ast::{self, ArgumentsBlock, ExprKind, Function, VarDecl};
use indy_value::{Array, Error, StructArray, Value};

use super::{arg_f64, err, isequal_values, need, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "mustBePositive", |_, a, _| {
        must(&a, "mustBePositive", "Value must be positive.", |v| {
            all_num(v, |x| x > 0.0)
        })
    });
    reg(it, "mustBeNonnegative", |_, a, _| {
        must(&a, "mustBeNonnegative", "Value must be nonnegative.", |v| {
            all_num(v, |x| x >= 0.0)
        })
    });
    reg(it, "mustBeNegative", |_, a, _| {
        must(&a, "mustBeNegative", "Value must be negative.", |v| {
            all_num(v, |x| x < 0.0)
        })
    });
    reg(it, "mustBeNonpositive", |_, a, _| {
        must(&a, "mustBeNonpositive", "Value must be nonpositive.", |v| {
            all_num(v, |x| x <= 0.0)
        })
    });
    reg(it, "mustBeNonzero", |_, a, _| {
        must(&a, "mustBeNonzero", "Value must not be zero.", |v| {
            all_num(v, |x| x != 0.0)
        })
    });
    reg(it, "mustBeFinite", |_, a, _| {
        must(&a, "mustBeFinite", "Value must be finite.", |v| {
            all_num(v, f64::is_finite)
        })
    });
    reg(it, "mustBeInteger", |_, a, _| {
        must(&a, "mustBeInteger", "Value must be integer.", |v| {
            all_num(v, |x| x.is_finite() && x.fract() == 0.0)
        })
    });
    reg(it, "mustBeNumeric", |_, a, _| {
        must(&a, "mustBeNumeric", "Value must be numeric.", |v| {
            matches!(v, Value::Num(_))
        })
    });
    reg(it, "mustBeNumericOrLogical", |_, a, _| {
        must(
            &a,
            "mustBeNumericOrLogical",
            "Value must be numeric or logical.",
            |v| matches!(v, Value::Num(_) | Value::Bool(_)),
        )
    });
    reg(it, "mustBeReal", |_, a, _| {
        must(
            &a,
            "mustBeReal",
            "Value must be real.",
            |v| !matches!(v, Value::Num(n) if n.im.is_some()),
        )
    });
    reg(it, "mustBeNonempty", |_, a, _| {
        must(&a, "mustBeNonempty", "Value must not be empty.", |v| {
            !v.is_empty()
        })
    });
    reg(it, "mustBeVector", |_, a, _| {
        must(
            &a,
            "mustBeVector",
            "Value must be a 1-by-n or n-by-1 vector.",
            |v| {
                let d = v.dims_ref();
                d.len() == 2 && (d[0] == 1 || d[1] == 1)
            },
        )
    });
    reg(it, "mustBeText", |_, a, _| {
        must(
            &a,
            "mustBeText",
            "Value must be a character vector, string array, or cell array of character vectors.",
            |v| v.is_string_row() || super::cells::is_cellstr(v) || matches!(v, Value::Char(_)),
        )
    });
    reg(it, "mustBeTextScalar", |_, a, _| {
        must(
            &a,
            "mustBeTextScalar",
            "Value must be a character vector or string scalar.",
            |v| v.is_string_row(),
        )
    });
    reg(it, "mustBeNonzeroLengthText", |_, a, _| {
        must(
            &a,
            "mustBeNonzeroLengthText",
            "Value must be text with one or more characters.",
            |v| v.is_string_row() && !v.is_empty(),
        )
    });
    reg(it, "mustBeMember", |_, a, _| {
        need(&a, 2, "mustBeMember")?;
        let ok = member(&a[0], &a[1]);
        if ok {
            return Ok(vec![]);
        }
        let opts = match &a[1] {
            Value::Cell(c) => c
                .data
                .iter()
                .filter_map(|v| v.to_str())
                .map(|s| format!("'{}'", s))
                .collect::<Vec<_>>()
                .join(", "),
            v => indy_value::format::inline_text(v),
        };
        Err(err(
            "MATLAB:validators:mustBeMember",
            format!("Value must be a member of this set:\n    {}", opts),
        ))
    });
    reg(it, "mustBeGreaterThan", |_, a, _| {
        cmp_must(&a, "mustBeGreaterThan", "greater than", |x, c| x > c)
    });
    reg(it, "mustBeGreaterThanOrEqual", |_, a, _| {
        cmp_must(
            &a,
            "mustBeGreaterThanOrEqual",
            "greater than or equal to",
            |x, c| x >= c,
        )
    });
    reg(it, "mustBeLessThan", |_, a, _| {
        cmp_must(&a, "mustBeLessThan", "less than", |x, c| x < c)
    });
    reg(it, "mustBeLessThanOrEqual", |_, a, _| {
        cmp_must(
            &a,
            "mustBeLessThanOrEqual",
            "less than or equal to",
            |x, c| x <= c,
        )
    });
    reg(it, "mustBeInRange", |_, a, _| {
        need(&a, 3, "mustBeInRange")?;
        let lo = arg_f64(&a[1], "mustBeInRange")?;
        let hi = arg_f64(&a[2], "mustBeInRange")?;
        if all_num(&a[0], |x| x >= lo && x <= hi) {
            return Ok(vec![]);
        }
        Err(err(
            "MATLAB:validators:mustBeInRange",
            format!(
                "Value must be greater than or equal to {} and less than or equal to {}.",
                lo, hi
            ),
        ))
    });
    reg(it, "mustBeA", |_, a, _| {
        need(&a, 2, "mustBeA")?;
        let classes = super::cells::strings_of(&a[1], "mustBeA")?;
        if classes.iter().any(|c| super::core::isa_name(&a[0], c)) {
            return Ok(vec![]);
        }
        Err(err(
            "MATLAB:validators:mustBeA",
            format!(
                "Value must be one of the following types: {}.",
                classes.join(", ")
            ),
        ))
    });
    reg(it, "mustBeFile", |it, a, _| {
        need(&a, 1, "mustBeFile")?;
        let ok = a[0]
            .to_str()
            .is_some_and(|s| super::core::resolve_path(it, &s).is_file());
        if ok {
            Ok(vec![])
        } else {
            Err(err(
                "MATLAB:validators:mustBeFile",
                "The following files do not exist.",
            ))
        }
    });
    reg(it, "mustBeFolder", |it, a, _| {
        need(&a, 1, "mustBeFolder")?;
        let ok = a[0]
            .to_str()
            .is_some_and(|s| super::core::resolve_path(it, &s).is_dir());
        if ok {
            Ok(vec![])
        } else {
            Err(err(
                "MATLAB:validators:mustBeFolder",
                "The following folders do not exist.",
            ))
        }
    });
    reg(it, "validateattributes", validateattributes);
}

fn all_num(v: &Value, f: impl Fn(f64) -> bool) -> bool {
    match v {
        Value::Num(_) | Value::Bool(_) => v.to_f64_vec().unwrap().into_iter().all(f),
        _ => false,
    }
}

fn must(a: &[Value], name: &str, msg: &str, ok: impl Fn(&Value) -> bool) -> R<Vec<Value>> {
    need(a, 1, name)?;
    if ok(&a[0]) {
        return Ok(vec![]);
    }
    Err(err(&format!("MATLAB:validators:{}", name), msg))
}

fn cmp_must(a: &[Value], name: &str, word: &str, f: fn(f64, f64) -> bool) -> R<Vec<Value>> {
    need(a, 2, name)?;
    let c = arg_f64(&a[1], name)?;
    if all_num(&a[0], |x| f(x, c)) {
        return Ok(vec![]);
    }
    Err(err(
        &format!("MATLAB:validators:{}", name),
        format!("Value must be {} {}.", word, indy_value::format::short_g(c)),
    ))
}

fn member(v: &Value, set: &Value) -> bool {
    match set {
        Value::Cell(c) => match v {
            Value::Cell(vc) => vc
                .data
                .iter()
                .all(|x| c.data.iter().any(|s| isequal_values(x, s, false))),
            _ => c.data.iter().any(|s| isequal_values(v, s, false)),
        },
        _ => {
            let (Some(x), Some(s)) = (v.to_f64_vec(), set.to_f64_vec()) else {
                return false;
            };
            if v.is_char() && set.is_char() {
                return isequal_values(v, set, false);
            }
            x.iter().all(|e| s.contains(e))
        }
    }
}

fn has_attr(b: &ArgumentsBlock, name: &str) -> bool {
    b.attributes
        .iter()
        .any(|a| a.name.name.eq_ignore_ascii_case(name) && !a.negated)
}

/// Bind call arguments into the (already pushed) frame, then validate.
pub fn bind(it: &mut Interp, f: &Rc<Function>, args: Vec<Value>) -> R<()> {
    let inputs: Vec<&ArgumentsBlock> = f
        .arguments
        .iter()
        .filter(|b| !has_attr(b, "Output"))
        .collect();
    let repeating: Vec<String> = inputs
        .iter()
        .filter(|b| has_attr(b, "Repeating"))
        .flat_map(|b| b.decls.iter().map(|d| d.name.dotted()))
        .collect();
    let nv_decls: Vec<&VarDecl> = inputs
        .iter()
        .flat_map(|b| b.decls.iter())
        .filter(|d| d.name.parts.len() == 2)
        .collect();
    let nv_param: Option<String> = nv_decls.first().map(|d| d.name.parts[0].name.clone());
    let param_names: Vec<String> = f
        .params
        .iter()
        .map(|p| match p {
            ast::Param::Name(n) => n.name.clone(),
            ast::Param::WithDefault { name, .. } => name.name.clone(),
            ast::Param::Ignore(_) => "~".into(),
        })
        .collect();
    let positional: Vec<&String> = param_names
        .iter()
        .filter(|n| {
            !repeating.contains(n) && Some(*n) != nv_param.as_ref() && n.as_str() != "varargin"
        })
        .collect();
    let n = args.len();
    let mut it_args = args.into_iter().peekable();
    let mut bound = 0;
    for p in &positional {
        // Stop at the first name-value pair.
        if nv_param.is_some() && bound >= positional.len().min(n) {
            break;
        }
        if let Some(v) = it_args.next() {
            if p.as_str() != "~" {
                it.set_var(p, v);
            }
            bound += 1;
        }
    }
    let rest: Vec<Value> = it_args.collect();
    let mut rest_iter = rest.into_iter();
    if !repeating.is_empty() {
        let group = repeating.len();
        let items: Vec<Value> = rest_iter.by_ref().collect();
        if !items.len().is_multiple_of(group) {
            return Err(err(
                "MATLAB:TooManyInputs",
                "Repeating arguments must come in complete groups.",
            ));
        }
        for (k, name) in repeating.iter().enumerate() {
            let vals: Vec<Value> = items.iter().skip(k).step_by(group).cloned().collect();
            it.set_var(name, Value::cell_row(vals));
        }
    }
    if let Some(np) = &nv_param {
        let mut s = StructArray::new_scalar();
        let pairs: Vec<Value> = rest_iter.collect();
        if !pairs.len().is_multiple_of(2) {
            return Err(err(
                "MATLAB:InputParser:ParamMissingValue",
                "Name-value arguments must come in pairs.",
            ));
        }
        for pair in pairs.chunks(2) {
            let name = pair[0]
                .to_str()
                .ok_or_else(|| err("MATLAB:InputParser:ParamMustBeChar", "Name must be text."))?;
            let decl = nv_decls
                .iter()
                .find(|d| d.name.parts[1].name.eq_ignore_ascii_case(&name))
                .ok_or_else(|| {
                    err(
                        "MATLAB:TooManyInputs",
                        format!("Invalid name-value argument '{}'.", name),
                    )
                })?;
            s.set(0, &decl.name.parts[1].name, pair[1].clone());
        }
        it.set_var(np, Value::Struct(s));
    } else {
        let extra: Vec<Value> = rest_iter.collect();
        if !extra.is_empty() {
            if param_names.last().is_some_and(|p| p == "varargin") {
                it.set_var("varargin", Value::cell_row(extra));
            } else {
                return Err(err("MATLAB:TooManyInputs", "Too many input arguments."));
            }
        }
    }
    it.frame_mut().nargin = n;
    // Defaults and validation, declaration order.
    for (pos, d) in inputs.iter().flat_map(|b| b.decls.iter()).enumerate() {
        let is_rep = repeating.contains(&d.name.dotted());
        if d.name.parts.len() == 2 {
            let (p, field) = (&d.name.parts[0].name, &d.name.parts[1].name);
            let present =
                matches!(it.get_var(p), Some(Value::Struct(s)) if s.field_index(field).is_some());
            if !present {
                if let Some(def) = &d.default {
                    let v = it.eval(def)?;
                    let mut s = match it.take_var(p) {
                        Some(Value::Struct(s)) => s,
                        _ => StructArray::new_scalar(),
                    };
                    s.set(0, field, v);
                    it.set_var(p, Value::Struct(s));
                }
                continue;
            }
            let Some(Value::Struct(s)) = it.get_var(p) else {
                continue;
            };
            let v = s.get(0, field).unwrap().clone();
            let v = check_decl(it, d, v, pos + 1)?;
            let Some(Value::Struct(mut s)) = it.take_var(p) else {
                continue;
            };
            s.set(0, field, v);
            it.set_var(p, Value::Struct(s));
            continue;
        }
        let name = d.name.dotted();
        if is_rep {
            let Some(Value::Cell(c)) = it.get_var(&name).cloned() else {
                continue;
            };
            let mut out = Vec::new();
            for v in c.data {
                out.push(check_decl(it, d, v, pos + 1)?);
            }
            it.set_var(&name, Value::cell_row(out));
            continue;
        }
        match it.get_var(&name).cloned() {
            Some(v) => {
                let v = check_decl(it, d, v, pos + 1)?;
                it.set_var(&name, v);
            }
            None => {
                if let Some(def) = &d.default {
                    let v = it.eval(def)?;
                    it.set_var(&name, v);
                }
            }
        }
    }
    Ok(())
}

/// Validate outputs (`arguments (Output)`), in the callee frame.
pub fn check_outputs(it: &mut Interp, f: &Rc<Function>) -> R<()> {
    for b in f.arguments.iter().filter(|b| has_attr(b, "Output")) {
        let rep = has_attr(b, "Repeating");
        for (pos, d) in b.decls.iter().enumerate() {
            let name = d.name.dotted();
            let Some(v) = it.get_var(&name).cloned() else {
                continue;
            };
            if rep {
                if let Value::Cell(c) = v {
                    let mut out = Vec::new();
                    for x in c.data {
                        out.push(check_decl(it, d, x, pos + 1)?);
                    }
                    let n = out.len();
                    it.set_var(&name, Value::Cell(Array::new(vec![1, n], out)));
                }
                continue;
            }
            let v = check_decl(it, d, v, pos + 1)?;
            it.set_var(&name, v);
        }
    }
    Ok(())
}

fn wrap(e: Box<Error>, pos: usize) -> Box<Error> {
    let mut e = e;
    if !e.message.starts_with("Invalid argument") {
        e.message = format!("Invalid argument at position {}. {}", pos, e.message);
    }
    e
}

pub(crate) fn check_decl(it: &mut Interp, d: &VarDecl, v: Value, pos: usize) -> R<Value> {
    let mut v = v;
    if let Some(cls) = &d.class {
        let c = cls.dotted();
        v = coerce_class(v, &c).map_err(|e| wrap(e, pos))?;
    }
    if let Some(size) = &d.size {
        let mut want: Vec<Option<usize>> = Vec::new();
        for s in size {
            match &s.kind {
                ExprKind::Colon => want.push(None),
                _ => {
                    let x = it.eval(s)?;
                    want.push(Some(arg_f64(&x, "arguments")? as usize));
                }
            }
        }
        v = coerce_size(v, &want).map_err(|e| wrap(e, pos))?;
    }
    for val in &d.validators {
        let r = match &val.kind {
            ExprKind::Ident(name) => it.call_named(name, vec![v.clone()], 0),
            _ => {
                // Validator call with explicit arguments, evaluated with the
                // argument bound under its declared name.
                let name = d.name.parts.last().unwrap().name.clone();
                let saved = it.get_var(&name).cloned();
                it.set_var(&name, v.clone());
                let r = it.eval_multi(val, 0);
                match saved {
                    Some(s) => it.set_var(&name, s),
                    None => {
                        it.clear_var(&name);
                    }
                }
                r
            }
        };
        r.map_err(|e| wrap(e, pos))?;
    }
    Ok(v)
}

fn coerce_class(v: Value, c: &str) -> R<Value> {
    if v.class_name() == c {
        return Ok(v);
    }
    match c {
        "double" | "single" | "int8" | "int16" | "int32" | "int64" | "uint8" | "uint16"
        | "uint32" | "uint64" => match &v {
            Value::Num(_) | Value::Bool(_) => super::math::to_class(&v, c),
            Value::Char(_) => {
                // MATLAB converts char by code point; '3' becomes 51.
                super::math::to_class(&v, c)
            }
            other => Err(err(
                "MATLAB:validation:UnableToConvert",
                format!(
                    "Value must be convertible to {} (got {}).",
                    c,
                    other.class_name()
                ),
            )),
        },
        "logical" => super::math::to_class(&v, "logical").map_err(|_| {
            err(
                "MATLAB:validation:UnableToConvert",
                "Value must be convertible to logical.",
            )
        }),
        "char" => match &v {
            Value::Char(_) => Ok(v),
            _ => Err(err(
                "MATLAB:validation:UnableToConvert",
                "Value must be convertible to char.",
            )),
        },
        "cell" | "struct" | "function_handle" => {
            if v.class_name() == c {
                Ok(v)
            } else {
                Err(err(
                    "MATLAB:validation:UnableToConvert",
                    format!("Value must be of class {}.", c),
                ))
            }
        }
        _ => Ok(v),
    }
}

fn coerce_size(v: Value, want: &[Option<usize>]) -> R<Value> {
    let d = v.dims();
    let fits = |d: &[usize]| {
        want.len() >= d.len().min(want.len())
            && (0..want.len().max(d.len())).all(|k| {
                let have = *d.get(k).unwrap_or(&1);
                match want.get(k) {
                    Some(Some(w)) => *w == have,
                    Some(None) => true,
                    None => have == 1,
                }
            })
    };
    if fits(&d) {
        return Ok(v);
    }
    let fixed_total: Option<usize> = want
        .iter()
        .copied()
        .collect::<Option<Vec<_>>>()
        .map(|w| w.iter().product());
    // Scalar expansion to a fully fixed size.
    if v.numel() == 1
        && let Some(_) = fixed_total
    {
        let dims: Vec<usize> = want.iter().map(|w| w.unwrap()).collect();
        let n: usize = dims.iter().product();
        return super::math::take(&v, vec![0; n], dims);
    }
    // Vector re-orientation: (1,:) from a column and (:,1) from a row.
    let is_vec = d.len() == 2 && (d[0] == 1 || d[1] == 1);
    if is_vec && want.len() == 2 {
        let n = v.numel();
        let target = match (want[0], want[1]) {
            (Some(1), None) => Some(vec![1, n]),
            (None, Some(1)) => Some(vec![n, 1]),
            (Some(1), Some(m)) if m == n => Some(vec![1, n]),
            (Some(m), Some(1)) if m == n => Some(vec![n, 1]),
            _ => None,
        };
        if let Some(t) = target {
            return Ok(v.reshaped(t));
        }
    }
    let wtxt: Vec<String> = want
        .iter()
        .map(|w| w.map_or(":".into(), |x| x.to_string()))
        .collect();
    Err(err(
        "MATLAB:validation:IncompatibleSize",
        format!("Value must be of size {}.", wtxt.join("x")),
    ))
}

fn validateattributes(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 3, "validateattributes")?;
    let v = &a[0];
    let classes = super::cells::strings_of(&a[1], "validateattributes").unwrap_or_default();
    if !classes.is_empty() && !classes.iter().any(|c| super::core::isa_name(v, c)) {
        return Err(err(
            "MATLAB:invalidType",
            format!(
                "Expected input to be one of these types:\n\n{}\n\nInstead its type was {}.",
                classes.join(", "),
                v.class_name()
            ),
        ));
    }
    let Value::Cell(attrs) = &a[2] else {
        return Ok(vec![]);
    };
    let mut i = 0;
    let nums = v.to_f64_vec().unwrap_or_default();
    while i < attrs.data.len() {
        let name = attrs.data[i].to_str().unwrap_or_default();
        let next = attrs.data.get(i + 1);
        let fail = |what: &str| -> R<Vec<Value>> {
            Err(err(
                &format!("MATLAB:expected{}", capitalise(what)),
                format!("Expected input to be {}.", what),
            ))
        };
        match name.as_str() {
            "positive" if !nums.iter().all(|x| *x > 0.0) => return fail("positive"),
            "nonnegative" if !nums.iter().all(|x| *x >= 0.0) => return fail("nonnegative"),
            "integer" if !nums.iter().all(|x| x.fract() == 0.0 && x.is_finite()) => {
                return fail("integer");
            }
            "finite" if !nums.iter().all(|x| x.is_finite()) => return fail("finite"),
            "nonnan" if nums.iter().any(|x| x.is_nan()) => return fail("nonNaN"),
            "nonempty" if v.is_empty() => return fail("nonempty"),
            "scalar" if v.numel() != 1 => return fail("scalar"),
            "vector"
                if !(v.dims_ref().len() == 2 && (v.dims_ref()[0] == 1 || v.dims_ref()[1] == 1)) =>
            {
                return fail("vector");
            }
            "row" if v.dims_ref()[0] != 1 => return fail("row"),
            "column" if v.dims_ref().get(1) != Some(&1) => return fail("column"),
            "2d" if v.dims_ref().len() != 2 => return fail("2d"),
            "real" if matches!(v, Value::Num(n) if n.im.is_some()) => return fail("real"),
            "nonzero" if nums.contains(&0.0) => return fail("nonzero"),
            ">" | ">=" | "<" | "<=" | "numel" | "size" | "ncols" | "nrows" => {
                let lim = next.and_then(|x| x.scalar_f64()).unwrap_or(0.0);
                let ok = match name.as_str() {
                    ">" => nums.iter().all(|x| *x > lim),
                    ">=" => nums.iter().all(|x| *x >= lim),
                    "<" => nums.iter().all(|x| *x < lim),
                    "<=" => nums.iter().all(|x| *x <= lim),
                    "numel" => v.numel() as f64 == lim,
                    "ncols" => *v.dims_ref().get(1).unwrap_or(&1) as f64 == lim,
                    "nrows" => v.dims_ref()[0] as f64 == lim,
                    _ => true,
                };
                if !ok {
                    return Err(err(
                        "MATLAB:validateattributes:failed",
                        format!("Expected input to satisfy '{}'.", name),
                    ));
                }
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    Ok(vec![])
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
