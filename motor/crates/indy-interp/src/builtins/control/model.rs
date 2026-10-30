//! LTI model objects: `tf`, `zpk`, `ss`, `pid`, `pidstd`.
//!
//! An LTI model reaches MATLAB code as a built-in `Value::Object` (no
//! classdef metadata) whose properties are the MATLAB properties in
//! MATLAB order. The object is the single source of truth: every
//! operation parses it into a [`Sys`], computes, and builds a fresh
//! object with [`Sys::to_value`], which also validates and normalises
//! (so `set` and `obj.Prop = v` simply write the raw property and
//! re-parse). Transfer functions and zero/pole/gain models are SISO
//! (or empty); state-space models may be MIMO.

use indy_value::{Array, Object, StructArray, Value};

use super::num::{C, Mat, c, ccol, cdata, pconv, peval, poly_from_roots, proots, ptrim};
use crate::builtins::err;
use crate::{Interp, R};

pub const LTI_CLASSES: [&str; 5] = ["tf", "zpk", "ss", "pid", "pidstd"];

pub fn is_lti(v: &Value) -> bool {
    matches!(v, Value::Object(o) if o.meta.is_none() && LTI_CLASSES.contains(&o.class.as_str()))
}

/// Class precedence for mixed operations (higher wins).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Pid,
    PidStd,
    Tf,
    Zpk,
    Ss,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Tf => "tf",
            Kind::Zpk => "zpk",
            Kind::Ss => "ss",
            Kind::Pid => "pid",
            Kind::PidStd => "pidstd",
        }
    }
    pub fn from_name(s: &str) -> Option<Kind> {
        Some(match s {
            "tf" => Kind::Tf,
            "zpk" => Kind::Zpk,
            "ss" => Kind::Ss,
            "pid" => Kind::Pid,
            "pidstd" => Kind::PidStd,
            _ => return None,
        })
    }
    /// Rank for result-class decisions: pid and pidstd rank alike.
    pub fn rank(self) -> u8 {
        match self {
            Kind::Pid | Kind::PidStd => 0,
            Kind::Tf => 1,
            Kind::Zpk => 2,
            Kind::Ss => 3,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ss {
    pub a: Mat,
    pub b: Mat,
    pub c: Mat,
    pub d: Mat,
}

impl Ss {
    pub fn nx(&self) -> usize {
        self.a.r
    }
    pub fn ny(&self) -> usize {
        self.d.r
    }
    pub fn nu(&self) -> usize {
        self.d.c
    }
    pub fn gain(d: f64) -> Ss {
        Ss {
            a: Mat::zeros(0, 0),
            b: Mat::zeros(0, 1),
            c: Mat::zeros(1, 0),
            d: Mat::scalar(d),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Data {
    /// SISO transfer function, descending powers, equal lengths.
    Tf {
        num: Vec<f64>,
        den: Vec<f64>,
    },
    Zpk {
        z: Vec<C>,
        p: Vec<C>,
        k: C,
    },
    Ss(Ss),
    /// Parallel PID: `Kp + Ki/s + Kd*s/(Tf*s+1)`.
    Pid {
        kp: f64,
        ki: f64,
        kd: f64,
        tf: f64,
    },
    /// Standard PID: `Kp*(1 + 1/(Ti*s) + Td*s/((Td/N)*s+1))`.
    PidStd {
        kp: f64,
        ti: f64,
        td: f64,
        n: f64,
    },
    /// `tf()`, `zpk()`, `ss()`: 0×0 model.
    Empty,
}

/// A parsed LTI model.
#[derive(Clone, Debug, PartialEq)]
pub struct Sys {
    pub kind: Kind,
    pub data: Data,
    pub ts: f64,
    /// Every other property (names, delays, notes...), by MATLAB name.
    pub extra: Vec<(String, Value)>,
}

const TAIL: [&str; 10] = [
    "InputName",
    "InputUnit",
    "InputGroup",
    "OutputName",
    "OutputUnit",
    "OutputGroup",
    "Notes",
    "UserData",
    "Name",
    "SamplingGrid",
];

/// MATLAB property names of a class, in display order.
pub fn prop_names(kind: Kind) -> Vec<&'static str> {
    let head: &[&str] = match kind {
        Kind::Tf => &[
            "Numerator",
            "Denominator",
            "Variable",
            "IODelay",
            "InputDelay",
            "OutputDelay",
            "Ts",
            "TimeUnit",
        ],
        Kind::Zpk => &[
            "Z",
            "P",
            "K",
            "DisplayFormat",
            "Variable",
            "IODelay",
            "InputDelay",
            "OutputDelay",
            "Ts",
            "TimeUnit",
        ],
        Kind::Ss => &[
            "A",
            "B",
            "C",
            "D",
            "E",
            "Scaled",
            "StateName",
            "StatePath",
            "StateUnit",
            "InternalDelay",
            "InputDelay",
            "OutputDelay",
            "Ts",
            "TimeUnit",
        ],
        Kind::Pid => &[
            "Kp",
            "Ki",
            "Kd",
            "Tf",
            "IFormula",
            "DFormula",
            "InputDelay",
            "OutputDelay",
            "Ts",
            "TimeUnit",
        ],
        Kind::PidStd => &[
            "Kp",
            "Ti",
            "Td",
            "N",
            "IFormula",
            "DFormula",
            "InputDelay",
            "OutputDelay",
            "Ts",
            "TimeUnit",
        ],
    };
    let mut v = head.to_vec();
    v.extend_from_slice(&TAIL);
    v
}

fn data_props(kind: Kind) -> &'static [&'static str] {
    match kind {
        Kind::Tf => &["Numerator", "Denominator"],
        Kind::Zpk => &["Z", "P", "K"],
        Kind::Ss => &["A", "B", "C", "D", "E"],
        Kind::Pid => &["Kp", "Ki", "Kd", "Tf"],
        Kind::PidStd => &["Kp", "Ti", "Td", "N"],
    }
}

/// Canonical spelling of a property name (case-insensitive, unique
/// prefixes and the classic aliases `num`/`den`).
pub fn canon_prop(kind: Kind, name: &str) -> Option<&'static str> {
    let names = prop_names(kind);
    let low = name.to_ascii_lowercase();
    match (kind, low.as_str()) {
        (Kind::Tf, "num") => return Some("Numerator"),
        (Kind::Tf, "den") => return Some("Denominator"),
        (Kind::Zpk, "z") => return Some("Z"),
        (Kind::Zpk, "p") => return Some("P"),
        (Kind::Zpk, "k") => return Some("K"),
        (Kind::Ss, "a") => return Some("A"),
        (Kind::Ss, "b") => return Some("B"),
        (Kind::Ss, "c") => return Some("C"),
        (Kind::Ss, "d") => return Some("D"),
        (Kind::Ss, "e") => return Some("E"),
        (Kind::PidStd, "n") => return Some("N"),
        _ => {}
    }
    if let Some(n) = names.iter().find(|n| n.eq_ignore_ascii_case(name)) {
        return Some(n);
    }
    let hits: Vec<&&str> = names
        .iter()
        .filter(|n| !low.is_empty() && n.to_ascii_lowercase().starts_with(&low))
        .collect();
    if hits.len() == 1 {
        return Some(hits[0]);
    }
    None
}

pub fn no_prop(kind: Kind, name: &str) -> Box<indy_value::Error> {
    err(
        "Control:ltiobject:setLTI2",
        format!(
            "The \"{}\" property of the \"{}\" class is not a public property or does not exist.",
            name,
            kind.name()
        ),
    )
}

fn empty_cell(r: usize, c: usize) -> Value {
    Value::Cell(Array::new(vec![r, c], vec![]))
}

fn cell_of(r: usize, c: usize, v: Value) -> Value {
    Value::Cell(Array::new(vec![r, c], vec![v; r * c]))
}

fn name_cell(n: usize) -> Value {
    cell_of(n, 1, Value::str(""))
}

fn empty_struct() -> Value {
    Value::Struct(StructArray::new_scalar())
}

/// Finite real double scalar (integer and single classes become double).
fn real_scalar(v: &Value, prop: &str, id: &str) -> R<f64> {
    let bad = || {
        err(
            id,
            format!(
                "The value of the \"{}\" property must be a finite and real number.",
                prop
            ),
        )
    };
    match v {
        Value::Num(n) if n.re.len() == 1 && n.im.is_none() => {
            let x = n.re[0];
            if x.is_nan() || (x.is_infinite() && !matches!(prop, "Ti" | "N")) {
                return Err(bad());
            }
            Ok(x)
        }
        _ => Err(bad()),
    }
}

fn row_of(v: &Value, what: &str) -> R<Vec<f64>> {
    let v = match v {
        Value::Cell(c) if c.data.len() == 1 => c.data[0].clone(),
        Value::Cell(c) if c.data.len() > 1 => {
            return Err(mimo_tf());
        }
        other => other.clone(),
    };
    let n = v.to_num().ok_or_else(|| {
        err(
            "Control:ltiobject:tfProperties1",
            format!(
                "The \"{}\" property must be a row vector of real numbers.",
                what
            ),
        )
    })?;
    if n.im.is_some() {
        return Err(err(
            "Control:general:complexData",
            "Complex transfer function coefficients are not supported yet in the Rust engine.",
        ));
    }
    if n.re.iter().any(|x| !x.is_finite()) {
        return Err(err(
            "Control:ltiobject:tfProperties1",
            format!("The \"{}\" property must contain finite values.", what),
        ));
    }
    Ok(n.re)
}

pub fn mimo_tf() -> Box<indy_value::Error> {
    err(
        "Control:general:notSupported",
        "MIMO transfer function and zero/pole/gain models are not supported yet in the Rust engine (use ss).",
    )
}

fn col_c(v: &Value, what: &str) -> R<Vec<C>> {
    let v = match v {
        Value::Cell(c) if c.data.len() == 1 => c.data[0].clone(),
        Value::Cell(c) if c.data.len() > 1 => return Err(mimo_tf()),
        other => other.clone(),
    };
    cdata(&v).ok_or_else(|| {
        err(
            "Control:ltiobject:zpkProperties1",
            format!("The \"{}\" property must be a vector of numbers.", what),
        )
    })
}

/// Trim leading zeros of both and pad to equal length (MATLAB storage).
pub fn tf_norm(num: &[f64], den: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let n = ptrim(num);
    let d = ptrim(den);
    let len = n.len().max(d.len());
    let pad = |p: Vec<f64>| {
        let mut r = vec![0.0; len - p.len()];
        r.extend(p);
        r
    };
    (pad(n), pad(d))
}

impl Sys {
    pub fn new(kind: Kind, data: Data, ts: f64) -> Sys {
        Sys {
            kind,
            data,
            ts,
            extra: Vec::new(),
        }
    }
    pub fn tf(num: &[f64], den: &[f64], ts: f64) -> Sys {
        let (num, den) = tf_norm(num, den);
        Sys::new(Kind::Tf, Data::Tf { num, den }, ts)
    }
    pub fn gain(kind: Kind, k: f64, ts: f64) -> Sys {
        let data = match kind {
            Kind::Tf => Data::Tf {
                num: vec![k],
                den: vec![1.0],
            },
            Kind::Zpk => Data::Zpk {
                z: vec![],
                p: vec![],
                k: C::real(k),
            },
            Kind::Ss => Data::Ss(Ss::gain(k)),
            Kind::Pid => Data::Pid {
                kp: k,
                ki: 0.0,
                kd: 0.0,
                tf: 0.0,
            },
            Kind::PidStd => Data::PidStd {
                kp: k,
                ti: f64::INFINITY,
                td: 0.0,
                n: f64::INFINITY,
            },
        };
        Sys::new(kind, data, ts)
    }
    pub fn is_discrete(&self) -> bool {
        self.ts != 0.0
    }
    pub fn get_extra(&self, name: &str) -> Option<&Value> {
        self.extra.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }
    pub fn set_extra(&mut self, name: &str, v: Value) {
        match self.extra.iter_mut().find(|(n, _)| n == name) {
            Some(s) => s.1 = v,
            None => self.extra.push((name.to_string(), v)),
        }
    }
    /// `[ny, nu]`.
    pub fn size(&self) -> (usize, usize) {
        match &self.data {
            Data::Empty => (0, 0),
            Data::Ss(s) => (s.ny(), s.nu()),
            _ => (1, 1),
        }
    }
    pub fn nx(&self) -> usize {
        match &self.data {
            Data::Ss(s) => s.nx(),
            _ => 0,
        }
    }
    pub fn input_delay(&self) -> f64 {
        let d = |n: &str| {
            self.get_extra(n)
                .and_then(|v| v.to_f64_vec())
                .and_then(|v| v.first().copied())
                .unwrap_or(0.0)
        };
        d("InputDelay") + d("OutputDelay") + d("IODelay")
    }
    /// Copy the metadata (names, delays...) of `o` but keep own data.
    pub fn with_meta_of(mut self, o: &Sys) -> Sys {
        self.extra = o.extra.clone();
        self
    }

    // ----- parsing ------------------------------------------------------

    pub fn from_value(v: &Value) -> R<Sys> {
        match v {
            Value::Object(o) if is_lti(v) => Sys::from_object(o),
            _ => Err(err(
                "Control:general:invalidType",
                format!("Expected a dynamic system model, got {}.", v.class_name()),
            )),
        }
    }

    pub fn from_object(o: &Object) -> R<Sys> {
        let kind = Kind::from_name(&o.class).unwrap();
        let get = |n: &str| o.get(n).cloned().unwrap_or_default();
        let ts = match o.get("Ts") {
            Some(v) => {
                let t = v.scalar_f64().ok_or_else(|| {
                    err(
                        "Control:ltiobject:setLTI1",
                        "The \"Ts\" property must be a positive scalar, 0, or -1.",
                    )
                })?;
                if !(t == 0.0 || t == -1.0 || (t > 0.0 && t.is_finite())) {
                    return Err(err(
                        "Control:ltiobject:setLTI1",
                        "The \"Ts\" property must be a positive scalar, 0, or -1.",
                    ));
                }
                t
            }
            None => 0.0,
        };
        let data = match kind {
            Kind::Tf => {
                let (nv, dv) = (get("Numerator"), get("Denominator"));
                if nv.is_empty() && dv.is_empty() {
                    Data::Empty
                } else {
                    let num = row_of(&nv, "Numerator")?;
                    let den = row_of(&dv, "Denominator")?;
                    if ptrim(&den) == [0.0] {
                        return Err(err(
                            "Control:ltiobject:tfProperties2",
                            "The denominator of a transfer function cannot be zero.",
                        ));
                    }
                    let (num, den) = tf_norm(&num, &den);
                    Data::Tf { num, den }
                }
            }
            Kind::Zpk => {
                let (zv, pv, kv) = (get("Z"), get("P"), get("K"));
                if zv.is_empty() && pv.is_empty() && kv.is_empty() {
                    Data::Empty
                } else {
                    let incompatible = || {
                        err(
                            "Control:ltiobject:zpkProperties4",
                            "The values of the \"Z\", \"P\" and \"K\" properties must have compatible sizes.",
                        )
                    };
                    let cells_ok = |v: &Value| match v {
                        Value::Cell(c) => c.data.len() == 1,
                        _ => true,
                    };
                    if kv.numel() != 1 || !cells_ok(&zv) || !cells_ok(&pv) {
                        if kv.numel() > 1 || matches!(&zv, Value::Cell(c) if c.data.len() > 1) {
                            return Err(mimo_tf());
                        }
                        return Err(incompatible());
                    }
                    let z = col_c(&zv, "Z")?;
                    let p = col_c(&pv, "P")?;
                    let k = cdata(&kv).and_then(|v| v.first().copied()).ok_or_else(|| {
                        err(
                            "Control:ltiobject:zpkProperties1",
                            "The \"K\" property must be a numeric array.",
                        )
                    })?;
                    Data::Zpk { z, p, k }
                }
            }
            Kind::Ss => {
                let m = |n: &str| Mat::from_value(&get(n), n);
                let (a, b, cm, d) = (m("A")?, m("B")?, m("C")?, m("D")?);
                if let Some(e) = o.get("E")
                    && !e.is_empty()
                {
                    return Err(err(
                        "Control:general:notSupported",
                        "Descriptor state-space models (E matrix) are not supported yet in the Rust engine.",
                    ));
                }
                Data::Ss(check_ss(a, b, cm, d)?)
            }
            Kind::Pid => {
                let id = "Control:ltiobject:pidSet1";
                let kp = real_scalar(&get("Kp"), "Kp", id)?;
                let ki = real_scalar(&get("Ki"), "Ki", id)?;
                let kd = real_scalar(&get("Kd"), "Kd", id)?;
                let tf = real_scalar(&get("Tf"), "Tf", id)?;
                if tf < 0.0 {
                    return Err(err(
                        id,
                        "The value of the \"Tf\" property must be a nonnegative real number.",
                    ));
                }
                Data::Pid { kp, ki, kd, tf }
            }
            Kind::PidStd => {
                let id = "Control:ltiobject:pidstdSet1";
                let kp = real_scalar(&get("Kp"), "Kp", id)?;
                let ti = real_scalar(&get("Ti"), "Ti", id)?;
                let td = real_scalar(&get("Td"), "Td", id)?;
                let n = real_scalar(&get("N"), "N", id)?;
                if ti <= 0.0 || td < 0.0 || n <= 0.0 {
                    return Err(err(
                        id,
                        "Ti and N must be positive and Td nonnegative in a standard-form PID.",
                    ));
                }
                Data::PidStd { kp, ti, td, n }
            }
        };
        let skip = data_props(kind);
        let extra = o
            .props
            .iter()
            .filter(|(n, _)| n != "Ts" && !skip.contains(&n.as_str()))
            .cloned()
            .collect();
        Ok(Sys {
            kind,
            data,
            ts,
            extra,
        })
    }

    // ----- building -----------------------------------------------------

    pub fn to_value(&self) -> Value {
        Value::Object(Box::new(self.to_object()))
    }

    pub fn to_object(&self) -> Object {
        let mut o = Object::new(self.kind.name());
        let (ny, nu) = self.size();
        let discrete = self.is_discrete();
        for name in prop_names(self.kind) {
            let user = self.get_extra(name).cloned();
            let v = match name {
                "Numerator" | "Denominator" => match &self.data {
                    Data::Tf { num, den } => Value::Cell(Array::scalar(Value::row(
                        if name == "Numerator" { num } else { den }.clone(),
                    ))),
                    _ => empty_cell(0, 0),
                },
                "Z" | "P" => match &self.data {
                    Data::Zpk { z, p, .. } => {
                        Value::Cell(Array::scalar(ccol(if name == "Z" { z } else { p })))
                    }
                    _ => empty_cell(0, 0),
                },
                "K" => match &self.data {
                    Data::Zpk { k, .. } => super::num::cvalue(vec![1, 1], &[*k]),
                    _ => Value::empty(),
                },
                "A" | "B" | "C" | "D" => match &self.data {
                    Data::Ss(s) => match name {
                        "A" => s.a.to_value(),
                        "B" => s.b.to_value(),
                        "C" => s.c.to_value(),
                        _ => s.d.to_value(),
                    }
                    .clone(),
                    _ => Value::empty(),
                },
                "E" => Value::empty(),
                "Scaled" => user.unwrap_or(Value::bool(false)),
                "StateName" | "StatePath" | "StateUnit" => resize_names(user, self.nx()),
                "InternalDelay" => Value::matrix(vec![0, 1], vec![]),
                "Kp" | "Ki" | "Kd" | "Tf" | "Ti" | "Td" | "N" => {
                    let x = match (&self.data, name) {
                        (Data::Pid { kp, .. }, "Kp") => *kp,
                        (Data::Pid { ki, .. }, "Ki") => *ki,
                        (Data::Pid { kd, .. }, "Kd") => *kd,
                        (Data::Pid { tf, .. }, "Tf") => *tf,
                        (Data::PidStd { kp, .. }, "Kp") => *kp,
                        (Data::PidStd { ti, .. }, "Ti") => *ti,
                        (Data::PidStd { td, .. }, "Td") => *td,
                        (Data::PidStd { n, .. }, "N") => *n,
                        _ => 0.0,
                    };
                    Value::num(x)
                }
                "IFormula" | "DFormula" => {
                    if !discrete {
                        Value::str("")
                    } else {
                        match user.and_then(|v| v.to_str()) {
                            Some(s) if !s.is_empty() => Value::str(&s),
                            _ => Value::str("ForwardEuler"),
                        }
                    }
                }
                "DisplayFormat" => user.unwrap_or_else(|| Value::str("roots")),
                "Variable" => {
                    let cur = user.and_then(|v| v.to_str()).unwrap_or_default();
                    let ok_c = ["s", "p"];
                    let ok_d = ["z", "q", "z^-1"];
                    let v = if discrete {
                        if ok_d.contains(&cur.as_str()) {
                            cur
                        } else {
                            "z".to_string()
                        }
                    } else if ok_c.contains(&cur.as_str()) {
                        cur
                    } else {
                        "s".to_string()
                    };
                    Value::str(&v)
                }
                "IODelay" => user
                    .filter(|v| v.numel() == ny * nu)
                    .unwrap_or_else(|| Value::matrix(vec![ny, nu], vec![0.0; ny * nu])),
                "InputDelay" => user
                    .filter(|v| v.numel() == nu)
                    .unwrap_or_else(|| Value::matrix(vec![nu, 1], vec![0.0; nu])),
                "OutputDelay" => user
                    .filter(|v| v.numel() == ny)
                    .unwrap_or_else(|| Value::matrix(vec![ny, 1], vec![0.0; ny])),
                "Ts" => Value::num(self.ts),
                "TimeUnit" => user.unwrap_or_else(|| Value::str("seconds")),
                "InputName" | "InputUnit" => resize_names(user, nu),
                "OutputName" | "OutputUnit" => resize_names(user, ny),
                "InputGroup" | "OutputGroup" | "SamplingGrid" => user.unwrap_or_else(empty_struct),
                "Notes" => user.unwrap_or_else(|| Value::Str(Array::new(vec![0, 1], vec![]))),
                "UserData" => user.unwrap_or_default(),
                "Name" => user.unwrap_or_else(|| Value::str("")),
                _ => user.unwrap_or_default(),
            };
            o.props.push((name.to_string(), v));
        }
        o
    }
}

/// Channel/state names: a column cell of `n` char vectors. Wrong-size
/// user values fall back to blanks (a 0×0 model keeps what it was given).
fn resize_names(user: Option<Value>, n: usize) -> Value {
    let to_cell = |v: Value| -> Option<Value> {
        match v {
            Value::Cell(c) => Some(Value::Cell(Array::new(vec![c.data.len(), 1], c.data))),
            Value::Char(_) => Some(Value::Cell(Array::scalar(v))),
            Value::Str(s) => Some(Value::Cell(Array::new(
                vec![s.data.len(), 1],
                s.data
                    .iter()
                    .map(|x| Value::str(x.as_deref().unwrap_or("")))
                    .collect(),
            ))),
            _ => None,
        }
    };
    match user.and_then(to_cell) {
        Some(Value::Cell(c)) if c.data.len() == n || n == 0 => {
            if n == 0 && c.data.is_empty() {
                return name_cell(0);
            }
            Value::Cell(c)
        }
        _ => name_cell(n),
    }
}

pub fn check_ss(a: Mat, b: Mat, cm: Mat, d: Mat) -> R<Ss> {
    let nx = a.r;
    let bad = |m: &str| err("Control:ltiobject:ssProperties1", m.to_string());
    if a.r != a.c {
        return Err(bad("The \"A\" matrix must be square."));
    }
    // Infer sizes from whichever matrices are given.
    let nu = if b.c > 0 || b.r > 0 { b.c } else { d.c };
    let ny = if cm.r > 0 || cm.c > 0 { cm.r } else { d.r };
    let b = if b.is_empty() && nx == 0 {
        Mat::zeros(0, nu)
    } else {
        b
    };
    let cm = if cm.is_empty() && nx == 0 {
        Mat::zeros(ny, 0)
    } else {
        cm
    };
    let zero_scalar = d.r * d.c == 1 && d.d[0] == 0.0 && (ny != 1 || nu != 1);
    let d = if zero_scalar || (d.is_empty() && (ny > 0 && nu > 0)) {
        Mat::zeros(ny, nu)
    } else {
        d
    };
    if b.r != nx {
        return Err(bad(
            "The \"A\" and \"B\" matrices must have the same number of rows.",
        ));
    }
    if cm.c != nx {
        return Err(bad(
            "The \"A\" and \"C\" matrices must have the same number of columns.",
        ));
    }
    if d.r != cm.r || d.c != b.c {
        return Err(bad(
            "The \"D\" matrix must have as many rows as \"C\" and as many columns as \"B\".",
        ));
    }
    if a.d
        .iter()
        .chain(&b.d)
        .chain(&cm.d)
        .chain(&d.d)
        .any(|x| !x.is_finite())
    {
        return Err(bad("State-space matrices must contain finite values."));
    }
    Ok(Ss { a, b, c: cm, d })
}

// ---------------------------------------------------------------------------
// Conversions
// ---------------------------------------------------------------------------

fn complex_gain() -> Box<indy_value::Error> {
    err(
        "Control:general:complexData",
        "Models with a complex gain are not supported yet in the Rust engine.",
    )
}

fn siso_only(s: &Sys) -> R<()> {
    if s.size() != (1, 1) {
        return Err(err(
            "Control:general:notSupported",
            format!(
                "This operation supports SISO models only in the Rust engine ({}x{} model given).",
                s.size().0,
                s.size().1
            ),
        ));
    }
    Ok(())
}

/// Integrator `1/s` (continuous) or its discrete formula, as num/den.
fn integ_formula(form: &str, ts: f64) -> R<(Vec<f64>, Vec<f64>)> {
    if ts == 0.0 {
        return Ok((vec![1.0], vec![1.0, 0.0]));
    }
    let t = if ts < 0.0 { 1.0 } else { ts };
    Ok(match form {
        "" | "ForwardEuler" => (vec![t], vec![1.0, -1.0]),
        "BackwardEuler" => (vec![t, 0.0], vec![1.0, -1.0]),
        "Trapezoidal" => (vec![t / 2.0, t / 2.0], vec![1.0, -1.0]),
        other => {
            return Err(err(
                "Control:ltiobject:pidSet2",
                format!(
                    "Unknown discrete integrator formula '{}' (ForwardEuler, BackwardEuler or Trapezoidal).",
                    other
                ),
            ));
        }
    })
}

/// Sum of rational terms.
fn rat_add(a: (Vec<f64>, Vec<f64>), b: (Vec<f64>, Vec<f64>)) -> (Vec<f64>, Vec<f64>) {
    if a.1 == b.1 {
        return (super::num::padd(&a.0, &b.0), a.1);
    }
    (
        super::num::padd(&pconv(&a.0, &b.1), &pconv(&b.0, &a.1)),
        pconv(&a.1, &b.1),
    )
}

impl Sys {
    fn formula(&self, name: &str) -> String {
        self.get_extra(name)
            .and_then(|v| v.to_str())
            .unwrap_or_default()
    }

    /// Parallel-form gains of a PID object.
    pub fn pid_gains(&self) -> Option<(f64, f64, f64, f64)> {
        match self.data {
            Data::Pid { kp, ki, kd, tf } => Some((kp, ki, kd, tf)),
            Data::PidStd { kp, ti, td, n } => {
                let ki = if ti.is_infinite() { 0.0 } else { kp / ti };
                let tf = if n.is_infinite() { 0.0 } else { td / n };
                Some((kp, ki, kp * td, tf))
            }
            _ => None,
        }
    }

    /// SISO transfer function (num, den), unnormalised.
    pub fn tf_data(&self, it: &mut Interp) -> R<(Vec<f64>, Vec<f64>)> {
        match &self.data {
            Data::Empty => Err(err(
                "Control:general:emptyModel",
                "This operation is not defined for empty models.",
            )),
            Data::Tf { num, den } => Ok((num.clone(), den.clone())),
            Data::Zpk { z, p, k } => {
                if k.im != 0.0 {
                    return Err(complex_gain());
                }
                let mut num = poly_from_roots(z);
                for x in num.iter_mut() {
                    *x *= k.re;
                }
                Ok(tf_norm(&num, &poly_from_roots(p)))
            }
            Data::Ss(s) => {
                siso_only(self)?;
                ss_tf(it, s, 0, 0)
            }
            Data::Pid { .. } | Data::PidStd { .. } => {
                let (kp, ki, kd, tf) = self.pid_gains().unwrap();
                let mut acc = (vec![kp], vec![1.0]);
                if ki != 0.0 {
                    let (n, d) = integ_formula(&self.formula("IFormula"), self.ts)?;
                    acc = rat_add(acc, (super::num::pscale(&n, ki), d));
                }
                if kd != 0.0 {
                    // Kd / (Tf + DF(s)) with DF = 1/s continuous.
                    let (n, d) = integ_formula(&self.formula("DFormula"), self.ts)?;
                    // Kd / (Tf + n/d) = Kd d / (Tf d + n)
                    let den = super::num::padd(&super::num::pscale(&d, tf), &n);
                    acc = rat_add(acc, (super::num::pscale(&d, kd), den));
                }
                let (n, d) = tf_norm(&acc.0, &acc.1);
                // Normalise to a monic denominator like MATLAB's tf(pid).
                let lead = d[d.iter().position(|x| *x != 0.0).unwrap_or(0)];
                Ok((
                    n.iter().map(|x| x / lead).collect(),
                    d.iter().map(|x| x / lead).collect(),
                ))
            }
        }
    }

    /// Zeros, poles and gain (SISO).
    pub fn zpk_data(&self, it: &mut Interp) -> R<(Vec<C>, Vec<C>, C)> {
        match &self.data {
            Data::Zpk { z, p, k } => Ok((z.clone(), p.clone(), *k)),
            Data::Ss(_) => {
                let (num, den) = self.tf_data(it)?;
                let (mut z, mut p, k) = tf_zpk(it, &num, &den)?;
                sort_desc(&mut z);
                sort_desc(&mut p);
                Ok((z, p, k))
            }
            _ => {
                let (num, den) = self.tf_data(it)?;
                tf_zpk(it, &num, &den)
            }
        }
    }

    /// State-space realisation.
    pub fn ss_data(&self, it: &mut Interp) -> R<Ss> {
        match &self.data {
            Data::Ss(s) => Ok(s.clone()),
            Data::Empty => Ok(Ss {
                a: Mat::zeros(0, 0),
                b: Mat::zeros(0, 0),
                c: Mat::zeros(0, 0),
                d: Mat::zeros(0, 0),
            }),
            _ => {
                let (num, den) = self.tf_data(it)?;
                tf_ss(&num, &den)
            }
        }
    }

    /// Convert to another class (keeping metadata).
    pub fn convert(&self, it: &mut Interp, kind: Kind) -> R<Sys> {
        if kind == self.kind {
            return Ok(self.clone());
        }
        let data = match kind {
            Kind::Tf => match &self.data {
                Data::Empty => Data::Empty,
                _ => {
                    let (num, den) = self.tf_data(it)?;
                    let (num, den) = tf_norm(&num, &den);
                    Data::Tf { num, den }
                }
            },
            Kind::Zpk => match &self.data {
                Data::Empty => Data::Empty,
                _ => {
                    let (z, p, k) = self.zpk_data(it)?;
                    Data::Zpk { z, p, k }
                }
            },
            Kind::Ss => Data::Ss(self.ss_data(it)?),
            Kind::Pid | Kind::PidStd => {
                let (num, den) = self.tf_data(it)?;
                let (kp, ki, kd, tf) = tf_pid(&num, &den, self.ts)?;
                if kind == Kind::Pid {
                    Data::Pid { kp, ki, kd, tf }
                } else {
                    if kp == 0.0 && (ki != 0.0 || kd != 0.0) {
                        return Err(err(
                            "Control:transformation:pidstd1",
                            "This model cannot be represented as a standard-form PID (Kp = 0).",
                        ));
                    }
                    let ti = if ki == 0.0 { f64::INFINITY } else { kp / ki };
                    let td = kd / kp.max(f64::MIN_POSITIVE);
                    let n = if tf == 0.0 { f64::INFINITY } else { td / tf };
                    Data::PidStd { kp, ti, td, n }
                }
            }
        };
        let mut out = Sys::new(kind, data, self.ts);
        out.extra = self
            .extra
            .iter()
            .filter(|(n, _)| prop_names(kind).contains(&n.as_str()))
            .cloned()
            .collect();
        Ok(out)
    }

    /// Poles in model order.
    pub fn poles(&self, it: &mut Interp) -> R<Vec<C>> {
        match &self.data {
            Data::Zpk { p, .. } => Ok(p.clone()),
            Data::Ss(s) => super::num::eigvals(it, &s.a),
            Data::Empty => Ok(vec![]),
            _ => {
                let (_, den) = self.tf_data(it)?;
                proots(it, &den)
            }
        }
    }

    pub fn zeros(&self, it: &mut Interp) -> R<Vec<C>> {
        match &self.data {
            Data::Zpk { z, .. } => Ok(z.clone()),
            Data::Empty => Ok(vec![]),
            _ => {
                siso_only(self)?;
                let (num, _) = self.tf_data(it)?;
                proots(it, &num)
            }
        }
    }

    /// Frequency response at one complex point (`s`, or `z` when
    /// discrete), as an ny×nu column-major matrix. Delays included.
    pub fn eval_at(&self, it: &mut Interp, s: C) -> R<Vec<C>> {
        let mut h = match &self.data {
            Data::Empty => vec![],
            Data::Tf { num, den } => vec![peval(num, s).div(peval(den, s))],
            Data::Zpk { z, p, k } => {
                let mut v = *k;
                for x in z {
                    v = v.mul(s.sub(*x));
                }
                for x in p {
                    v = v.div(s.sub(*x));
                }
                vec![v]
            }
            Data::Ss(m) => ss_eval(m, s),
            _ => {
                let (num, den) = self.tf_data(it)?;
                vec![peval(&num, s).div(peval(&den, s))]
            }
        };
        let delay = self.input_delay();
        if delay != 0.0 {
            let f = if self.is_discrete() {
                s.ln().scale(-delay).exp()
            } else {
                s.scale(-delay).exp()
            };
            for x in h.iter_mut() {
                *x = x.mul(f);
            }
        }
        Ok(h)
    }
}

/// `C (sI-A)^-1 B + D`.
pub fn ss_eval(m: &Ss, s: C) -> Vec<C> {
    let (nx, ny, nu) = (m.nx(), m.ny(), m.nu());
    let mut out: Vec<C> = m.d.d.iter().map(|x| C::real(*x)).collect();
    if nx == 0 {
        return out;
    }
    let mut mm = vec![C::default(); nx * nx];
    for j in 0..nx {
        for i in 0..nx {
            mm[i + j * nx] = C::real(-m.a.at(i, j));
        }
        mm[j + j * nx] = mm[j + j * nx].add(s);
    }
    let b: Vec<C> = m.b.d.iter().map(|x| C::real(*x)).collect();
    let Some(x) = super::num::csolve(nx, &mm, &b, nu) else {
        return vec![c(f64::INFINITY, 0.0); ny * nu];
    };
    for j in 0..nu {
        for i in 0..ny {
            let mut acc = C::default();
            for k in 0..nx {
                acc = acc.add(x[k + j * nx].scale(m.c.at(i, k)));
            }
            out[i + j * ny] = out[i + j * ny].add(acc);
        }
    }
    out
}

fn sort_desc(v: &mut [C]) {
    v.sort_by(|a, b| {
        b.re.partial_cmp(&a.re)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.im.partial_cmp(&a.im).unwrap_or(std::cmp::Ordering::Equal))
    });
}

pub fn tf_zpk(it: &mut Interp, num: &[f64], den: &[f64]) -> R<(Vec<C>, Vec<C>, C)> {
    let n = ptrim(num);
    let d = ptrim(den);
    if n == [0.0] {
        return Ok((vec![], proots(it, &d)?, C::default()));
    }
    let k = n[0] / d[0];
    Ok((proots(it, &n)?, proots(it, &d)?, C::real(k)))
}

/// Characteristic polynomial through the eigenvalues.
pub fn charpoly(it: &mut Interp, a: &Mat) -> R<Vec<f64>> {
    let ev = super::num::eigvals(it, a)?;
    Ok(poly_from_roots(&ev))
}

/// Transfer function of channel (i, j) of a state-space model:
/// `num = det(sI-A+b c) - det(sI-A) + d det(sI-A)`.
pub fn ss_tf(it: &mut Interp, s: &Ss, i: usize, j: usize) -> R<(Vec<f64>, Vec<f64>)> {
    let d = s.d.at(i, j);
    if s.nx() == 0 {
        return Ok((vec![d], vec![1.0]));
    }
    let b = s.b.col_of(j);
    let cr = s.c.row_of(i);
    let pa = charpoly(it, &s.a)?;
    let pabc = charpoly(it, &s.a.sub(&b.mul(&cr)))?;
    let mut num = vec![0.0; pa.len()];
    for k in 0..pa.len() {
        let v = pabc[k] - pa[k] + d * pa[k];
        let scale = pa[k].abs().max(pabc[k].abs());
        num[k] = if v.abs() <= 1e-12 * scale { 0.0 } else { v };
    }
    Ok(tf_norm(&num, &pa))
}

/// Controllable canonical realisation of a proper SISO transfer function.
pub fn tf_ss(num: &[f64], den: &[f64]) -> R<Ss> {
    let d = ptrim(den);
    let n = ptrim(num);
    if n.len() > d.len() {
        return Err(err(
            "Control:transformation:ss2",
            "Improper transfer functions cannot be converted to state space (the Rust engine has no descriptor models).",
        ));
    }
    let lead = d[0];
    let d: Vec<f64> = d.iter().map(|x| x / lead).collect();
    let mut b = vec![0.0; d.len() - n.len()];
    b.extend(n.iter().map(|x| x / lead));
    let nx = d.len() - 1;
    if n == [0.0] {
        // Zero system: keep the states (MATLAB keeps the denominator).
    }
    let mut a = Mat::zeros(nx, nx);
    for j in 0..nx {
        a.set(0, j, -d[j + 1]);
    }
    for i in 1..nx {
        a.set(i, i - 1, 1.0);
    }
    let mut bm = Mat::zeros(nx, 1);
    if nx > 0 {
        bm.set(0, 0, 1.0);
    }
    let mut cm = Mat::zeros(1, nx);
    for j in 0..nx {
        cm.set(0, j, b[j + 1] - b[0] * d[j + 1]);
    }
    Ok(Ss {
        a,
        b: bm,
        c: cm,
        d: Mat::scalar(b[0]),
    })
}

/// PID gains of a SISO transfer function (continuous parallel form).
pub fn tf_pid(num: &[f64], den: &[f64], ts: f64) -> R<(f64, f64, f64, f64)> {
    let fail = || {
        err(
            "Control:transformation:pid1",
            "The model cannot be represented as a PID controller.",
        )
    };
    if ts != 0.0 {
        return Err(err(
            "Control:general:notSupported",
            "Conversion of discrete-time models to pid is not supported yet in the Rust engine.",
        ));
    }
    let d = ptrim(den);
    let lead = d[0];
    let d: Vec<f64> = d.iter().map(|x| x / lead).collect();
    let n: Vec<f64> = ptrim(num).iter().map(|x| x / lead).collect();
    let nc = |k: usize| -> f64 {
        // coefficient of s^k
        if k < n.len() { n[n.len() - 1 - k] } else { 0.0 }
    };
    let tol = 1e-12;
    match d.len() {
        1 => {
            if n.len() > 1 {
                return Err(fail());
            }
            Ok((nc(0), 0.0, 0.0, 0.0))
        }
        2 => {
            let a = d[1];
            if a.abs() <= tol {
                // (n2 s^2 + n1 s + n0)/s
                if n.len() > 3 {
                    return Err(fail());
                }
                Ok((nc(1), nc(0), nc(2), 0.0))
            } else {
                // Kp + Kd s/(Tf s + 1), a = 1/Tf
                if n.len() > 2 {
                    return Err(fail());
                }
                let kp = nc(0) / a;
                let kd = (nc(1) - kp) / a;
                Ok((kp, 0.0, kd, 1.0 / a))
            }
        }
        3 => {
            if d[2].abs() > tol * (1.0 + d[1].abs()) || d[1] <= 0.0 {
                return Err(fail());
            }
            let a = d[1];
            let ki = nc(0) / a;
            let kp = (nc(1) - ki) / a;
            let kd = (nc(2) - kp) / a;
            Ok((kp, ki, kd, 1.0 / a))
        }
        _ => Err(fail()),
    }
}
