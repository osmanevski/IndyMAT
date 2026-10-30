//! Built-in handle classes (`containers.Map`, `inputParser`, `onCleanup`,
//! event listeners) and class introspection builtins.

use std::cell::RefCell;
use std::rc::Rc;

use indy_value::{Array, Object, StructArray, Value};

use super::cells::{cellstr_value, strings_of};
use super::{arg_str, err, isequal_values, need, one, reg};
use crate::classes::{class_of, is_class_obj, method_names, public_props};
use crate::eval::EOp;
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "containers.Map", map_new);
    reg(it, "keys", map_keys);
    reg(it, "values", map_values);
    reg(it, "isKey", map_iskey);
    reg(it, "remove", map_remove);
    reg(it, "inputParser", ip_new);
    reg(it, "addRequired", |it, a, _| ip_add(it, a, "required"));
    reg(it, "addOptional", |it, a, _| ip_add(it, a, "optional"));
    reg(it, "addParameter", |it, a, _| ip_add(it, a, "parameter"));
    reg(it, "addParamValue", |it, a, _| ip_add(it, a, "parameter"));
    reg(it, "addSwitch", |it, a, _| ip_add(it, a, "switch"));
    reg(it, "parse", ip_parse);
    reg(it, "onCleanup", |_, a, _| {
        need(&a, 1, "onCleanup")?;
        let mut o = Object::new("onCleanup");
        o.set("task", a[0].clone());
        one(Value::HObject(Rc::new(RefCell::new(o))))
    });
    reg(it, "addlistener", addlistener);
    reg(it, "listener", addlistener);
    reg(it, "notify", notify);
    reg(it, "isvalid", |_, a, _| {
        need(&a, 1, "isvalid")?;
        one(Value::bool(match &a[0] {
            Value::HObject(o) => o.borrow().get("__deleted__").is_none(),
            _ => false,
        }))
    });
    reg(it, "properties", properties);
    reg(it, "methods", methods);
    reg(it, "ismethod", |it, a, _| {
        need(&a, 2, "ismethod")?;
        let n = arg_str(&a[1], "ismethod")?;
        let _ = it;
        one(Value::bool(
            class_of(&a[0]).is_some_and(|c| c.method(&n).is_some()),
        ))
    });
    reg(it, "isprop", |_, a, _| {
        need(&a, 2, "isprop")?;
        let n = arg_str(&a[1], "isprop")?;
        let ok = match class_of(&a[0]) {
            Some(c) => c.prop(&n).is_some(),
            None => match &a[0] {
                Value::HObject(o) => o.borrow().get(&n).is_some(),
                Value::Object(o) => o.get(&n).is_some(),
                _ => false,
            },
        };
        one(Value::bool(ok))
    });
    reg(it, "metaclass", |_, a, _| {
        need(&a, 1, "metaclass")?;
        let mut s = StructArray::new_scalar();
        s.set(0, "Name", Value::str(&a[0].class_name()));
        if let Some(c) = class_of(&a[0]) {
            s.set(0, "PropertyNames", cellstr_value(public_props(&c), true));
            s.set(0, "MethodNames", cellstr_value(method_names(&c), true));
        }
        one(Value::Struct(s))
    });
    reg(it, "enumeration", enumeration);
}

fn new_handle(o: Object) -> Value {
    Value::HObject(Rc::new(RefCell::new(o)))
}

fn hget(v: &Value, name: &str) -> Option<Value> {
    match v {
        Value::HObject(o) => o.borrow().get(name).cloned(),
        _ => None,
    }
}

fn hset(v: &Value, name: &str, x: Value) {
    if let Value::HObject(o) = v {
        o.borrow_mut().set(name, x);
    }
}

fn is_class(v: &Value, name: &str) -> bool {
    matches!(v, Value::HObject(o) if o.borrow().class == name && o.borrow().meta.is_none())
}

/// Indexing on built-in handle objects.
pub fn handle_ops(it: &mut Interp, v: Value, eops: &[EOp], nargout: usize) -> R<Vec<Value>> {
    let (vals, used) = match &eops[0] {
        EOp::Paren(args) if is_class(&v, "containers.Map") => {
            need(args, 1, "containers.Map")?;
            (vec![map_get(&v, &args[0])?], 1)
        }
        EOp::Paren(args) => {
            if args.iter().all(|a| a.scalar_f64() == Some(1.0)) {
                (vec![v.clone()], 1)
            } else {
                return Err(err(
                    "MATLAB:badsubscript",
                    "Index exceeds the number of array elements.",
                ));
            }
        }
        EOp::Field(name) => {
            if is_class(&v, "containers.Map") && name == "Count" {
                (vec![Value::num(map_len(&v) as f64)], 1)
            } else if let Some(p) = hget(&v, name).filter(|_| !name.starts_with("__")) {
                (vec![p], 1)
            } else {
                let mut args = vec![v.clone()];
                let mut used = 1;
                if let Some(EOp::Paren(a)) = eops.get(1) {
                    args.extend(a.iter().cloned());
                    used = 2;
                }
                let n = if used == eops.len() { nargout } else { 1 };
                (it.call_named(name, args, n)?, used)
            }
        }
        EOp::Brace(_) => {
            return Err(err(
                "MATLAB:cellRefFromNonCell",
                "Brace indexing is not supported for variables of this type.",
            ));
        }
    };
    if used >= eops.len() {
        return Ok(vals);
    }
    let x = vals
        .into_iter()
        .next()
        .ok_or_else(|| err("MATLAB:needMoreRhsOutputs", "Expression produced no value."))?;
    it.apply_eops(x, &eops[used..], nargout)
}

/// `h(...)... = rhs` for built-in handle objects.
pub fn handle_assign(_: &mut Interp, h: Value, eops: &[EOp], rhs: Value) -> R<()> {
    match &eops[0] {
        EOp::Paren(args) if is_class(&h, "containers.Map") => {
            need(args, 1, "containers.Map")?;
            let rhs = if eops.len() > 1 {
                let mut slot = map_get(&h, &args[0]).ok();
                crate::assign::assign_path(&mut slot, &eops[1..], rhs)?;
                slot.unwrap_or_default()
            } else {
                rhs
            };
            map_put(&h, &args[0], rhs)
        }
        EOp::Field(name) => {
            let mut slot = hget(&h, name);
            crate::assign::assign_path(&mut slot, &eops[1..], rhs)?;
            hset(&h, name, slot.unwrap_or_default());
            Ok(())
        }
        _ => Err(err(
            "MATLAB:unsupported",
            "Unsupported indexed assignment into a handle object.",
        )),
    }
}

// ---------------------------------------------------------------------------
// containers.Map
// ---------------------------------------------------------------------------

fn map_parts(m: &Value) -> (Vec<Value>, Vec<Value>, String) {
    let k = match hget(m, "__keys__") {
        Some(Value::Cell(c)) => c.data,
        _ => vec![],
    };
    let v = match hget(m, "__vals__") {
        Some(Value::Cell(c)) => c.data,
        _ => vec![],
    };
    let kt = hget(m, "KeyType")
        .and_then(|x| x.to_str())
        .unwrap_or_else(|| "char".into());
    (k, v, kt)
}

fn map_len(m: &Value) -> usize {
    map_parts(m).0.len()
}

fn key_norm(kt: &str, k: &Value) -> R<Value> {
    if kt == "char" {
        match k.to_str() {
            // Char rows and string scalars are both char keys.
            Some(s) if k.is_char() || k.is_text_scalar() => Ok(Value::str(&s)),
            _ => Err(err(
                "MATLAB:Containers:TypeMismatch",
                "Specified key type does not match the type expected for this container.",
            )),
        }
    } else {
        match k.scalar_f64() {
            Some(x) if !k.is_char() => Ok(Value::num(x)),
            _ => Err(err(
                "MATLAB:Containers:TypeMismatch",
                "Specified key type does not match the type expected for this container.",
            )),
        }
    }
}

fn key_cmp(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (a.to_str(), b.to_str(), a.is_char()) {
        (Some(x), Some(y), true) => x.cmp(&y),
        _ => a
            .scalar_f64()
            .unwrap_or(0.0)
            .partial_cmp(&b.scalar_f64().unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal),
    }
}

fn map_find(keys: &[Value], k: &Value) -> Result<usize, usize> {
    keys.binary_search_by(|x| key_cmp(x, k))
}

fn map_get(m: &Value, k: &Value) -> R<Value> {
    let (keys, vals, kt) = map_parts(m);
    let k = key_norm(&kt, k)?;
    match map_find(&keys, &k) {
        Ok(i) => Ok(vals[i].clone()),
        Err(_) => Err(err(
            "MATLAB:Containers:Map:NoKey",
            match k.to_str() {
                Some(s) if k.is_char() => format!(
                    "The specified key '{}' is not present in this container.",
                    s
                ),
                _ => "The specified key is not present in this container.".to_string(),
            },
        )),
    }
}

fn map_put(m: &Value, k: &Value, v: Value) -> R<()> {
    let (mut keys, mut vals, kt) = map_parts(m);
    let k = key_norm(&kt, k)?;
    match map_find(&keys, &k) {
        Ok(i) => vals[i] = v,
        Err(i) => {
            keys.insert(i, k);
            vals.insert(i, v);
        }
    }
    hset(m, "__keys__", Value::cell_row(keys));
    hset(m, "__vals__", Value::cell_row(vals));
    Ok(())
}

fn map_new(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let mut o = Object::new("containers.Map");
    o.set("KeyType", Value::str("char"));
    o.set("ValueType", Value::str("any"));
    o.set("__keys__", Value::cell_row(vec![]));
    o.set("__vals__", Value::cell_row(vec![]));
    let m = new_handle(o);
    let mut i = 0;
    let mut init: Option<(Value, Value)> = None;
    if !a.is_empty()
        && !a[0]
            .to_str()
            .is_some_and(|s| matches!(s.as_str(), "KeyType" | "ValueType" | "UniformValues"))
    {
        need(&a, 2, "containers.Map")?;
        init = Some((a[0].clone(), a[1].clone()));
        i = 2;
    }
    while i + 1 < a.len() {
        let name = arg_str(&a[i], "containers.Map")?;
        match name.as_str() {
            "KeyType" => hset(&m, "KeyType", a[i + 1].clone()),
            "ValueType" => hset(&m, "ValueType", a[i + 1].clone()),
            "UniformValues" => {}
            _ => {
                return Err(err(
                    "MATLAB:Containers:Map:InvalidParam",
                    format!("Unknown parameter '{}'.", name),
                ));
            }
        }
        i += 2;
    }
    if let Some((k, v)) = init {
        let ks: Vec<Value> = match &k {
            Value::Cell(c) => c.data.clone(),
            Value::Num(n) if n.re.len() > 1 => n.re.iter().map(|x| Value::num(*x)).collect(),
            other => vec![other.clone()],
        };
        if ks
            .first()
            .is_some_and(|x| !x.is_char() && !x.is_text_scalar())
        {
            hset(&m, "KeyType", Value::str("double"));
        }
        let vs: Vec<Value> = match &v {
            Value::Cell(c) if ks.len() != 1 || c.data.len() == 1 => c.data.clone(),
            // One numeric value per key: Map({'a','b'}, [1 2]).
            Value::Num(n) if ks.len() > 1 && n.re.len() == ks.len() && n.im.is_none() => {
                n.re.iter().map(|x| Value::num(*x)).collect()
            }
            other => vec![other.clone()],
        };
        if ks.len() != vs.len() {
            return Err(err(
                "MATLAB:Containers:Map:NumKeysNotEqualNumValues",
                "The number of keys and values must be the same.",
            ));
        }
        for (kk, vv) in ks.into_iter().zip(vs) {
            map_put(&m, &kk, vv)?;
        }
    }
    one(m)
}

fn map_keys(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    need(&a, 1, "keys")?;
    if !is_class(&a[0], "containers.Map") {
        return Err(Interp::undefined("keys")).or_else(|e| fallback(it, "keys", a, n, e));
    }
    one(Value::cell_row(map_parts(&a[0]).0))
}

fn fallback(
    _: &mut Interp,
    name: &str,
    _: Vec<Value>,
    _: usize,
    _: Box<indy_value::Error>,
) -> R<Vec<Value>> {
    Err(err(
        "MATLAB:UndefinedFunction",
        format!(
            "Undefined function '{}' for input arguments of this type.",
            name
        ),
    ))
}

fn map_values(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    need(&a, 1, "values")?;
    if !is_class(&a[0], "containers.Map") {
        return fallback(it, "values", a, n, err("", ""));
    }
    match a.get(1) {
        None => one(Value::cell_row(map_parts(&a[0]).1)),
        Some(Value::Cell(c)) => {
            let mut out = Vec::new();
            for k in &c.data {
                out.push(map_get(&a[0], k)?);
            }
            one(Value::Cell(Array::new(c.dims.clone(), out)))
        }
        Some(_) => Err(err(
            "MATLAB:Containers:Map:InvalidKeys",
            "Keys must be given in a cell array.",
        )),
    }
}

fn map_iskey(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    need(&a, 2, "isKey")?;
    if !is_class(&a[0], "containers.Map") {
        return fallback(it, "isKey", a, n, err("", ""));
    }
    let (keys, _, kt) = map_parts(&a[0]);
    let has = |k: &Value| {
        key_norm(&kt, k)
            .ok()
            .is_some_and(|k| map_find(&keys, &k).is_ok())
    };
    match &a[1] {
        Value::Cell(c) => one(Value::Bool(Array::new(
            c.dims.clone(),
            c.data.iter().map(has).collect(),
        ))),
        k => one(Value::bool(has(k))),
    }
}

fn map_remove(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    need(&a, 2, "remove")?;
    if !is_class(&a[0], "containers.Map") {
        return fallback(it, "remove", a, n, err("", ""));
    }
    let (mut keys, mut vals, kt) = map_parts(&a[0]);
    let ks: Vec<Value> = match &a[1] {
        Value::Cell(c) => c.data.clone(),
        k => vec![k.clone()],
    };
    for k in ks {
        let k = key_norm(&kt, &k)?;
        match map_find(&keys, &k) {
            Ok(i) => {
                keys.remove(i);
                vals.remove(i);
            }
            Err(_) => {
                return Err(err(
                    "MATLAB:Containers:Map:NoKey",
                    "The specified key is not present in this container.",
                ));
            }
        }
    }
    hset(&a[0], "__keys__", Value::cell_row(keys));
    hset(&a[0], "__vals__", Value::cell_row(vals));
    one(a[0].clone())
}

// ---------------------------------------------------------------------------
// inputParser
// ---------------------------------------------------------------------------

fn ip_new(_: &mut Interp, _: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let mut o = Object::new("inputParser");
    o.set("FunctionName", Value::str(""));
    o.set("CaseSensitive", Value::bool(false));
    o.set("KeepUnmatched", Value::bool(false));
    o.set("PartialMatching", Value::bool(true));
    o.set("StructExpand", Value::bool(true));
    o.set("Parameters", Value::cell_row(vec![]));
    o.set("Results", Value::Struct(StructArray::new_scalar()));
    o.set("Unmatched", Value::Struct(StructArray::new_scalar()));
    o.set("UsingDefaults", Value::cell_row(vec![]));
    o.set("__spec__", Value::cell_row(vec![]));
    one(new_handle(o))
}

fn ip_add(_: &mut Interp, a: Vec<Value>, kind: &str) -> R<Vec<Value>> {
    need(&a, 2, "inputParser")?;
    if !is_class(&a[0], "inputParser") {
        return Err(err(
            "MATLAB:UndefinedFunction",
            "addRequired/addOptional/addParameter need an inputParser.",
        ));
    }
    let name = arg_str(&a[1], "inputParser")?;
    let (default, validator) = match kind {
        "required" => (Value::empty(), a.get(2).cloned()),
        "switch" => (Value::bool(false), None),
        _ => {
            need(&a, 3, "inputParser")?;
            (
                a[2].clone(),
                a.get(3).filter(|v| matches!(v, Value::Func(_))).cloned(),
            )
        }
    };
    let mut s = StructArray::new_scalar();
    s.set(0, "kind", Value::str(kind));
    s.set(0, "name", Value::str(&name));
    s.set(0, "default", default);
    s.set(0, "validator", validator.unwrap_or_default());
    let mut spec = match hget(&a[0], "__spec__") {
        Some(Value::Cell(c)) => c.data,
        _ => vec![],
    };
    spec.push(Value::Struct(s));
    let names: Vec<Value> = spec
        .iter()
        .filter_map(|v| {
            if let Value::Struct(s) = v {
                s.get(0, "name").cloned()
            } else {
                None
            }
        })
        .collect();
    hset(&a[0], "__spec__", Value::cell_row(spec));
    hset(&a[0], "Parameters", Value::cell_row(names));
    Ok(vec![])
}

fn validate(it: &mut Interp, f: &Value, v: &Value, name: &str) -> R<bool> {
    if !matches!(f, Value::Func(_)) {
        return Ok(true);
    }
    match it.call_value(f, vec![v.clone()], 1) {
        Ok(r) => match r.first() {
            Some(x) => x.is_true().map_err(|_| {
                err(
                    "MATLAB:InputParser:ArgumentFailedValidation",
                    format!("The value of '{}' is invalid.", name),
                )
            }),
            None => Ok(true),
        },
        Err(e) => Err(err(
            "MATLAB:InputParser:ArgumentFailedValidation",
            format!("The value of '{}' is invalid. {}", name, e.message),
        )),
    }
}

fn ip_parse(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "parse")?;
    let p = a[0].clone();
    if !is_class(&p, "inputParser") {
        return Err(err(
            "MATLAB:UndefinedFunction",
            "parse requires an inputParser object.",
        ));
    }
    let spec: Vec<StructArray> = match hget(&p, "__spec__") {
        Some(Value::Cell(c)) => c
            .data
            .into_iter()
            .filter_map(|v| {
                if let Value::Struct(s) = v {
                    Some(s)
                } else {
                    None
                }
            })
            .collect(),
        _ => vec![],
    };
    let flag = |n: &str| {
        hget(&p, n)
            .and_then(|v| v.scalar_f64())
            .is_some_and(|x| x != 0.0)
    };
    let (case_sens, keep, partial, expand) = (
        flag("CaseSensitive"),
        flag("KeepUnmatched"),
        flag("PartialMatching"),
        flag("StructExpand"),
    );
    let field = |s: &StructArray, f: &str| s.get(0, f).cloned().unwrap_or_default();
    let sname = |s: &StructArray| field(s, "name").to_str().unwrap_or_default();
    let kind = |s: &StructArray| field(s, "kind").to_str().unwrap_or_default();
    let mut results: Vec<(String, Value)> = Vec::new();
    let mut defaults: Vec<String> = Vec::new();
    let args: Vec<Value> = a[1..].to_vec();
    let mut i = 0;
    let params: Vec<&StructArray> = spec
        .iter()
        .filter(|s| matches!(kind(s).as_str(), "parameter" | "switch"))
        .collect();
    let match_param = |n: &str| -> Option<String> {
        let eq = |x: &str, y: &str| {
            if case_sens {
                x == y
            } else {
                x.eq_ignore_ascii_case(y)
            }
        };
        if let Some(s) = params.iter().find(|s| eq(&sname(s), n)) {
            return Some(sname(s));
        }
        if partial {
            let hits: Vec<String> = params
                .iter()
                .map(|s| sname(s))
                .filter(|pn| {
                    if case_sens {
                        pn.starts_with(n)
                    } else {
                        pn.to_lowercase().starts_with(&n.to_lowercase())
                    }
                })
                .collect();
            if hits.len() == 1 {
                return Some(hits[0].clone());
            }
        }
        None
    };
    for s in spec.iter().filter(|s| kind(s) == "required") {
        let n = sname(s);
        let Some(v) = args.get(i) else {
            return Err(err(
                "MATLAB:InputParser:notEnoughInputs",
                "Not enough input arguments.",
            ));
        };
        if !validate(it, &field(s, "validator"), v, &n)? {
            return Err(err(
                "MATLAB:InputParser:ArgumentFailedValidation",
                format!("The value of '{}' is invalid.", n),
            ));
        }
        results.push((n, v.clone()));
        i += 1;
    }
    for s in spec.iter().filter(|s| kind(s) == "optional") {
        let n = sname(s);
        let take = match args.get(i) {
            Some(v) => {
                let is_name = v
                    .to_str()
                    .is_some_and(|t| v.is_char() && match_param(&t).is_some());
                !is_name && validate(it, &field(s, "validator"), v, &n).unwrap_or(false)
            }
            None => false,
        };
        if take {
            results.push((n, args[i].clone()));
            i += 1;
        } else {
            results.push((n.clone(), field(s, "default")));
            defaults.push(n);
        }
    }
    let mut pairs: Vec<(String, Value)> = Vec::new();
    while i < args.len() {
        if expand && let Value::Struct(st) = &args[i] {
            for (k, f) in st.fields.iter().enumerate() {
                pairs.push((f.clone(), st.elems[0][k].clone()));
            }
            i += 1;
            continue;
        }
        let Some(n) = args[i].to_str() else {
            return Err(err(
                "MATLAB:InputParser:ParamMustBeChar",
                "Parameter names must be character vectors.",
            ));
        };
        let v = args.get(i + 1).cloned().ok_or_else(|| {
            err(
                "MATLAB:InputParser:ParamMissingValue",
                format!("No value was given for '{}'.", n),
            )
        })?;
        pairs.push((n, v));
        i += 2;
    }
    let mut unmatched = StructArray::new_scalar();
    let mut given: Vec<(String, Value)> = Vec::new();
    for (n, v) in pairs {
        match match_param(&n) {
            Some(real) => given.push((real, v)),
            None if keep => unmatched.set(0, &n, v),
            None => {
                return Err(err(
                    "MATLAB:InputParser:UnmatchedParameter",
                    format!(
                        "'{}' is not a recognized parameter. For a list of valid name-value pair arguments, see the documentation for this function.",
                        n
                    ),
                ));
            }
        }
    }
    for s in &params {
        let n = sname(s);
        match given.iter().rev().find(|(g, _)| *g == n) {
            Some((_, v)) => {
                if !validate(it, &field(s, "validator"), v, &n)? {
                    return Err(err(
                        "MATLAB:InputParser:ArgumentFailedValidation",
                        format!("The value of '{}' is invalid.", n),
                    ));
                }
                results.push((n, v.clone()));
            }
            None => {
                results.push((n.clone(), field(s, "default")));
                defaults.push(n);
            }
        }
    }
    results.sort_by(|x, y| x.0.cmp(&y.0));
    let mut r = StructArray::new_scalar();
    for (k, v) in results {
        r.set(0, &k, v);
    }
    hset(&p, "Results", Value::Struct(r));
    hset(&p, "Unmatched", Value::Struct(unmatched));
    hset(&p, "UsingDefaults", cellstr_value(defaults, false));
    Ok(vec![])
}

// ---------------------------------------------------------------------------
// events and introspection
// ---------------------------------------------------------------------------

fn addlistener(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 3, "addlistener")?;
    let ev = arg_str(&a[1], "addlistener")?;
    let mut o = Object::new("event.listener");
    o.set("Source", a[0].clone());
    o.set("EventName", Value::str(&ev));
    o.set("Callback", a[2].clone());
    o.set("Enabled", Value::bool(true));
    let l = new_handle(o);
    let mut ls = get_listeners(&a[0]).unwrap_or_default();
    ls.push(l.clone());
    match &a[0] {
        Value::HObject(h) => h.borrow_mut().set("__listeners__", Value::cell_row(ls)),
        _ => {
            return Err(err(
                "MATLAB:class:listenerNeedsHandle",
                "addlistener requires a handle object.",
            ));
        }
    }
    one(l)
}

fn get_listeners(v: &Value) -> Option<Vec<Value>> {
    match v {
        Value::HObject(h) => match h.borrow().get("__listeners__") {
            Some(Value::Cell(c)) => Some(c.data.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn notify(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "notify")?;
    let ev = arg_str(&a[1], "notify")?;
    let mut data = StructArray::new_scalar();
    data.set(0, "Source", a[0].clone());
    data.set(0, "EventName", Value::str(&ev));
    let data = a.get(2).cloned().unwrap_or(Value::Struct(data));
    for l in get_listeners(&a[0]).unwrap_or_default() {
        let (name, cb, deleted, enabled) = match &l {
            Value::HObject(h) => {
                let h = h.borrow();
                (
                    h.get("EventName")
                        .and_then(|v| v.to_str())
                        .unwrap_or_default(),
                    h.get("Callback").cloned().unwrap_or_default(),
                    h.get("__deleted__").is_some(),
                    h.get("Enabled").and_then(|v| v.scalar_f64()).unwrap_or(1.0) != 0.0,
                )
            }
            _ => continue,
        };
        if name == ev && !deleted && enabled {
            it.call_value(&cb, vec![a[0].clone(), data.clone()], 0)?;
        }
    }
    Ok(vec![])
}

fn properties(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "properties")?;
    let names = match class_of(&a[0]) {
        Some(c) => public_props(&c),
        None => {
            if let Some(n) = a[0].to_str()
                && let Some(c) = it.resolve_class(&n)?
            {
                public_props(&c)
            } else {
                match &a[0] {
                    Value::HObject(o) => o
                        .borrow()
                        .props
                        .iter()
                        .filter(|p| !p.0.starts_with("__"))
                        .map(|p| p.0.clone())
                        .collect(),
                    Value::Object(o) => o.props.iter().map(|p| p.0.clone()).collect(),
                    _ => vec![],
                }
            }
        }
    };
    if nargout == 0 {
        let mut s = String::from("Properties:\n\n");
        for n in &names {
            s.push_str(&format!("    {}\n", n));
        }
        it.print(&s);
        return Ok(vec![]);
    }
    one(cellstr_value(names, true))
}

fn methods(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "methods")?;
    let cls = match class_of(&a[0]) {
        Some(c) => Some(c),
        None => match a[0].to_str() {
            Some(n) => it.resolve_class(&n)?,
            None => None,
        },
    };
    let names = cls.map(|c| method_names(&c)).unwrap_or_default();
    if nargout == 0 {
        let s = format!("Methods:\n\n    {}\n", names.join("  "));
        it.print(&s);
        return Ok(vec![]);
    }
    one(cellstr_value(names, true))
}

fn enumeration(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "enumeration")?;
    let cls = match class_of(&a[0]) {
        Some(c) => c,
        None => {
            let n = arg_str(&a[0], "enumeration")?;
            it.resolve_class(&n)?
                .ok_or_else(|| err("MATLAB:class:InvalidEnum", format!("No class '{}'.", n)))?
        }
    };
    let mut members = Vec::new();
    for i in 0..cls.enums.len() {
        members.push(it.enum_member(&cls, i)?);
    }
    let names: Vec<String> = cls.enums.iter().map(|e| e.0.clone()).collect();
    // Object arrays are not modelled: members come back as a column cell.
    let n = members.len();
    let mut out = vec![Value::Cell(Array::new(vec![n, 1], members))];
    if nargout > 1 {
        out.push(cellstr_value(names, true));
    }
    Ok(out)
}

/// `isequal` support for objects.
pub fn objects_equal(a: &Value, b: &Value) -> Option<bool> {
    match (a, b) {
        (Value::HObject(x), Value::HObject(y)) => Some(
            Rc::ptr_eq(x, y)
                || (x.borrow().class == y.borrow().class && x.borrow().props == y.borrow().props),
        ),
        (Value::Object(x), Value::Object(y)) => Some(
            x.class == y.class
                && x.props.len() == y.props.len()
                && x.props
                    .iter()
                    .all(|(n, v)| y.get(n).is_some_and(|w| isequal_values(v, w, false))),
        ),
        _ => None,
    }
}

#[allow(dead_code)]
fn unused(v: &Value) -> bool {
    is_class_obj(v) && strings_of(v, "").is_ok()
}
