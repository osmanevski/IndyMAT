//! Control System Toolbox, native: LTI objects (`tf`, `zpk`, `ss`, `pid`,
//! `pidstd`), model arithmetic, analysis, time and frequency responses,
//! discretisation and plotting forms. MATLAB documentation is the
//! specification; Octave's control package served only as a black-box
//! oracle for test values.
//!
//! LTI models are built-in objects (see [`model`]); the interpreter routes
//! operators, property access/assignment, display and the overloaded
//! builtins listed in [`method`] here when an argument is a model.

mod analysis;
mod model;
mod num;
mod ops;
mod plots;
mod resp;
mod show;

use indy_syntax::ast::{BinOp, UnOp};
use indy_value::{Array, Object, StructArray, Value};

pub use model::is_lti;
use model::{Data, Kind, Ss, Sys, canon_prop, no_prop, prop_names};
use num::{C, Mat, ccol, cdata, cvalue};
use ops::{Operand, not_supported, operand};

use super::{arg_f64, arg_str, err, need, one, reg};
use crate::eval::EOp;
use crate::{BuiltinFn, Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "tf", tf_ctor);
    reg(it, "zpk", zpk_ctor);
    reg(it, "ss", ss_ctor);
    reg(it, "pid", pid_ctor);
    reg(it, "pidstd", pidstd_ctor);
    reg(it, "tfdata", tfdata);
    reg(it, "zpkdata", zpkdata);
    reg(it, "ssdata", ssdata);
    reg(it, "piddata", piddata);
    reg(it, "ss2tf", ss2tf);
    reg(it, "tf2ss", tf2ss);
    reg(it, "isct", |_, a, _| {
        need(&a, 1, "isct")?;
        one(Value::bool(!sys_of(&a[0], "isct")?.is_discrete()))
    });
    reg(it, "isdt", |_, a, _| {
        need(&a, 1, "isdt")?;
        one(Value::bool(sys_of(&a[0], "isdt")?.is_discrete()))
    });
    reg(it, "issiso", |_, a, _| {
        need(&a, 1, "issiso")?;
        one(Value::bool(sys_of(&a[0], "issiso")?.size() == (1, 1)))
    });
    reg(it, "isstatic", isstatic);
    reg(it, "hasdelay", |_, a, _| {
        need(&a, 1, "hasdelay")?;
        one(Value::bool(sys_of(&a[0], "hasdelay")?.input_delay() != 0.0))
    });
    reg(it, "order", order);
    reg(it, "pole", pole);
    reg(it, "zero", zero);
    reg(it, "tzero", zero);
    reg(it, "dcgain", dcgain);
    reg(it, "damp", damp);
    reg(it, "stepinfo", stepinfo);
    reg(it, "margin", margin);
    reg(it, "bandwidth", bandwidth);
    reg(it, "isstable", |it, a, _| {
        need(&a, 1, "isstable")?;
        let s = sys_of(&a[0], "isstable")?;
        one(Value::bool(analysis::isstable(it, &s)?))
    });
    reg(it, "ctrb", ctrb);
    reg(it, "obsv", obsv);
    reg(it, "place", place);
    reg(it, "acker", place);
    reg(it, "lqr", lqr);
    reg(it, "care", care);
    reg(it, "lyap", lyap);
    reg(it, "dlyap", dlyap);
    reg(it, "gram", gram);
    reg(it, "minreal", minreal);
    reg(it, "pade", pade);
    reg(it, "rlocus", rlocus);
    reg(it, "pzmap", pzmap);
    reg(it, "evalfr", evalfr);
    reg(it, "freqresp", freqresp);
    reg(it, "bode", bode);
    reg(it, "bodemag", bodemag);
    reg(it, "nyquist", nyquist);
    reg(it, "step", step);
    reg(it, "impulse", impulse);
    reg(it, "initial", initial);
    reg(it, "lsim", lsim);
    reg(it, "c2d", c2d);
    reg(it, "d2c", d2c);
    reg(it, "c2dOptions", c2d_options);
    reg(it, "feedback", feedback);
    reg(it, "series", series);
    reg(it, "parallel", parallel);
    reg(it, "append", append);
}

/// Builtins overloaded for model arguments (looked up when any argument
/// is an LTI object; MATLAB method dispatch).
pub fn method(name: &str) -> Option<BuiltinFn> {
    Some(match name {
        "size" => m_size,
        "isa" => m_isa,
        "disp" => m_disp,
        "display" => m_display,
        "set" => m_set,
        "get" => m_get,
        "properties" | "fieldnames" => m_properties,
        "isprop" => m_isprop,
        "eig" => pole,
        "inv" => m_inv,
        "isempty" => |_, a, _| {
            need(&a, 1, "isempty")?;
            let s = sys_of(&a[0], "isempty")?;
            let (y, u) = s.size();
            one(Value::bool(y == 0 || u == 0))
        },
        "ndims" => |_, _, _| one(Value::num(2.0)),
        "isreal" => |_, _, _| one(Value::bool(true)),
        "plus" => |it, a, _| binop(it, BinOp::Add, a),
        "minus" => |it, a, _| binop(it, BinOp::Sub, a),
        "mtimes" => |it, a, _| binop(it, BinOp::Mul, a),
        "mrdivide" => |it, a, _| binop(it, BinOp::Div, a),
        "mldivide" => |it, a, _| binop(it, BinOp::LeftDiv, a),
        "mpower" => |it, a, _| binop(it, BinOp::Pow, a),
        "uminus" => |it, a, _| {
            need(&a, 1, "uminus")?;
            one(unary(it, UnOp::Neg, &a[0])?)
        },
        _ => return None,
    })
}

fn binop(it: &mut Interp, op: BinOp, a: Vec<Value>) -> R<Vec<Value>> {
    need(&a, 2, op.text())?;
    one(ops::binary(it, op, &a[0], &a[1])?)
}

// ---------------------------------------------------------------------------
// Interpreter hooks
// ---------------------------------------------------------------------------

pub fn binary(it: &mut Interp, op: BinOp, a: &Value, b: &Value) -> R<Value> {
    ops::binary(it, op, a, b)
}

pub fn unary(it: &mut Interp, op: UnOp, a: &Value) -> R<Value> {
    let s = Sys::from_value(a)?;
    match op {
        UnOp::Plus => Ok(a.clone()),
        UnOp::Neg => {
            Ok(ops::mul(it, Operand::Gain(Mat::scalar(-1.0)), Operand::Sys(s))?.to_value())
        }
        UnOp::Transpose if s.size() == (1, 1) && s.kind != Kind::Ss => Ok(a.clone()),
        UnOp::Transpose => {
            let m = s.ss_data(it)?;
            let t = Ss {
                a: m.a.t(),
                b: m.c.t(),
                c: m.b.t(),
                d: m.d.t(),
            };
            Ok(Sys::new(Kind::Ss, Data::Ss(t), s.ts).to_value())
        }
        _ => Err(not_supported(&format!(
            "Operator '{}' on dynamic systems",
            match op {
                UnOp::Not => "~",
                _ => "'",
            }
        ))),
    }
}

/// Display text for `name = sys`.
pub fn display_text(name: &str, v: &Value) -> String {
    match Sys::from_value(v) {
        Ok(s) => show::display(name, &s),
        Err(e) => format!(
            "{} =\n\n  <invalid {} object: {}>\n\n",
            name,
            v.class_name(),
            e.message
        ),
    }
}

fn get_prop(v: &Value, name: &str) -> R<Value> {
    let Value::Object(o) = v else { unreachable!() };
    let kind = Kind::from_name(&o.class).unwrap();
    let canon = canon_prop(kind, name).ok_or_else(|| no_prop(kind, name))?;
    Ok(o.get(canon).cloned().unwrap_or_default())
}

/// `sys.Prop...`, `sys(i,j)` reads.
pub fn object_ops(it: &mut Interp, v: Value, eops: &[EOp], nargout: usize) -> R<Vec<Value>> {
    match &eops[0] {
        EOp::Field(name) => {
            let p = get_prop(&v, name)?;
            if eops.len() == 1 {
                return Ok(vec![p]);
            }
            it.apply_eops(p, &eops[1..], nargout)
        }
        EOp::Paren(args) => {
            let s = Sys::from_value(&v)?;
            let sub = subsystem(&s, args)?;
            if eops.len() == 1 {
                return Ok(vec![sub.to_value()]);
            }
            it.apply_eops(sub.to_value(), &eops[1..], nargout)
        }
        EOp::Brace(_) => Err(err(
            "MATLAB:cellRefFromNonCell",
            "Brace indexing is not supported for variables of this type.",
        )),
    }
}

fn index_list(v: &Value, n: usize) -> R<Vec<usize>> {
    if v.to_str().as_deref() == Some(":") {
        return Ok((0..n).collect());
    }
    if let Value::Bool(b) = v {
        return Ok(b
            .data
            .iter()
            .enumerate()
            .filter(|(_, x)| **x)
            .map(|(i, _)| i)
            .collect());
    }
    let x = v.to_f64_vec().unwrap_or_default();
    x.iter()
        .map(|k| {
            if *k >= 1.0 && k.fract() == 0.0 && (*k as usize) <= n {
                Ok(*k as usize - 1)
            } else {
                Err(err(
                    "MATLAB:badsubscript",
                    "Index exceeds the number of model inputs or outputs.",
                ))
            }
        })
        .collect()
}

fn subsystem(s: &Sys, args: &[Value]) -> R<Sys> {
    let (ny, nu) = s.size();
    let (rows, cols) = match args.len() {
        0 => return Ok(s.clone()),
        2 => (index_list(&args[0], ny)?, index_list(&args[1], nu)?),
        _ => {
            return Err(err(
                "Control:ltiobject:subsref3",
                "Use sys(i,j) to select outputs i and inputs j.",
            ));
        }
    };
    if rows == (0..ny).collect::<Vec<_>>() && cols == (0..nu).collect::<Vec<_>>() {
        return Ok(s.clone());
    }
    let Data::Ss(m) = &s.data else {
        return Err(err(
            "MATLAB:badsubscript",
            "Index exceeds the number of model inputs or outputs.",
        ));
    };
    let pick_rows = |x: &Mat, r: &[usize]| {
        let mut o = Mat::zeros(r.len(), x.c);
        for (i, &ri) in r.iter().enumerate() {
            for j in 0..x.c {
                o.set(i, j, x.at(ri, j));
            }
        }
        o
    };
    let pick_cols = |x: &Mat, c: &[usize]| pick_rows(&x.t(), c).t();
    let sub = Ss {
        a: m.a.clone(),
        b: pick_cols(&m.b, &cols),
        c: pick_rows(&m.c, &rows),
        d: pick_cols(&pick_rows(&m.d, &rows), &cols),
    };
    Ok(Sys::new(Kind::Ss, Data::Ss(sub), s.ts))
}

/// Validate a raw object and return its normalised value.
fn normalised(o: Object) -> R<Value> {
    Ok(Sys::from_object(&o)?.to_value())
}

/// `var.Prop... = rhs` for a model held in variable `name`.
pub fn assign(it: &mut Interp, name: &str, eops: &[EOp], rhs: Value) -> R<()> {
    let Some(Value::Object(o)) = it.get_var(name).cloned() else {
        unreachable!()
    };
    let mut o = *o;
    let kind = Kind::from_name(&o.class).unwrap();
    let EOp::Field(pname) = &eops[0] else {
        return Err(not_supported(
            "Indexed assignment into dynamic system models",
        ));
    };
    let canon = canon_prop(kind, pname).ok_or_else(|| no_prop(kind, pname))?;
    let newval = if eops.len() == 1 {
        rhs
    } else {
        let mut slot = Some(o.get(canon).cloned().unwrap_or_default());
        crate::assign::assign_path(&mut slot, &eops[1..], rhs)?;
        slot.unwrap_or_default()
    };
    o.set(canon, newval);
    let v = normalised(o)?;
    it.set_var(name, v);
    Ok(())
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

/// A model argument (numeric values become static tf gains).
fn sys_of(v: &Value, fname: &str) -> R<Sys> {
    if is_lti(v) {
        return Sys::from_value(v);
    }
    match v {
        Value::Num(_) | Value::Bool(_) => {
            let m = Mat::from_value(v, fname)?;
            if m.r == 1 && m.c == 1 {
                Ok(Sys::gain(Kind::Tf, m.d[0], 0.0))
            } else {
                Ok(Sys::new(
                    Kind::Ss,
                    Data::Ss(Ss {
                        a: Mat::zeros(0, 0),
                        b: Mat::zeros(0, m.c),
                        c: Mat::zeros(m.r, 0),
                        d: m,
                    }),
                    0.0,
                ))
            }
        }
        _ => Err(err(
            "Control:general:invalidType",
            format!(
                "{}: expected a dynamic system model, got {}.",
                fname,
                v.class_name()
            ),
        )),
    }
}

fn is_text(v: &Value) -> bool {
    matches!(v, Value::Char(_) | Value::Str(_))
}

fn mat_arg(v: &Value, what: &str) -> R<Mat> {
    Mat::from_value(v, what)
}

fn vec_arg(v: &Value, what: &str) -> R<Vec<f64>> {
    v.to_f64_vec().ok_or_else(|| {
        err(
            "Control:general:invalidType",
            format!("{} must be a numeric vector.", what),
        )
    })
}

fn row(v: Vec<f64>) -> Value {
    Value::row(v)
}

fn col(v: Vec<f64>) -> Value {
    Value::col(v)
}

fn cell1(v: Value) -> Value {
    Value::Cell(Array::scalar(v))
}

/// Apply `Name, Value` pairs to a model object.
fn apply_pairs(mut o: Object, pairs: &[Value]) -> R<Object> {
    let kind = Kind::from_name(&o.class).unwrap();
    if !pairs.len().is_multiple_of(2) {
        return Err(err(
            "Control:ltiobject:setLTI4",
            "Property names and values must come in pairs.",
        ));
    }
    for p in pairs.chunks(2) {
        let name = p[0].to_str().ok_or_else(|| {
            err(
                "Control:ltiobject:setLTI4",
                "Property names must be character vectors or strings.",
            )
        })?;
        let canon = canon_prop(kind, &name).ok_or_else(|| no_prop(kind, &name))?;
        o.set(canon, p[1].clone());
    }
    Ok(o)
}

fn finish_ctor(s: Sys, pairs: &[Value]) -> R<Vec<Value>> {
    let o = apply_pairs(s.to_object(), pairs)?;
    one(normalised(o)?)
}

/// Split constructor arguments into data and trailing name/value pairs.
fn split_pairs(a: &[Value]) -> (&[Value], &[Value]) {
    let pos = a.iter().position(is_text).unwrap_or(a.len());
    (&a[..pos], &a[pos..])
}

fn ts_arg(v: Option<&Value>) -> R<f64> {
    match v {
        None => Ok(0.0),
        Some(x) => {
            let t = arg_f64(x, "Ts")?;
            if !(t == 0.0 || t == -1.0 || (t > 0.0 && t.is_finite())) {
                return Err(err(
                    "Control:ltiobject:setLTI1",
                    "The \"Ts\" property must be a positive scalar, 0, or -1.",
                ));
            }
            Ok(t)
        }
    }
}

/// `tf(sys)`-style conversion when the first argument is a model.
fn convert_ctor(it: &mut Interp, kind: Kind, a: &[Value]) -> R<Option<Vec<Value>>> {
    if let Some(v) = a.first()
        && is_lti(v)
    {
        let s = Sys::from_value(v)?.convert(it, kind)?;
        let rest = &a[1..];
        // Optional conversion method strings ('min', 'measured') are ignored.
        let pairs = if rest.len() % 2 == 1 {
            &rest[1..]
        } else {
            rest
        };
        return Ok(Some(finish_ctor(s, pairs)?));
    }
    Ok(None)
}

// ---------------------------------------------------------------------------
// Constructors
// ---------------------------------------------------------------------------

fn tf_ctor(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    if let Some(r) = convert_ctor(it, Kind::Tf, &a)? {
        return Ok(r);
    }
    if a.is_empty() {
        return one(Sys::new(Kind::Tf, Data::Empty, 0.0).to_value());
    }
    if let Some(v) = a[0].to_str().filter(|_| is_text(&a[0])) {
        // tf('s'), tf('z', Ts)
        let ts = if a.len() >= 2 && !is_text(&a[1]) {
            ts_arg(a.get(1))?
        } else {
            -1.0
        };
        let (num, den, ts) = match v.as_str() {
            "s" | "p" => (vec![1.0, 0.0], vec![0.0, 1.0], 0.0),
            "z" | "q" => (vec![1.0, 0.0], vec![0.0, 1.0], ts),
            "z^-1" => (vec![1.0], vec![1.0, 0.0], ts),
            _ => {
                return Err(err(
                    "Control:ltiobject:tf1",
                    format!("Invalid variable name '{}' (use 's' or 'z').", v),
                ));
            }
        };
        let mut s = Sys::tf(&num, &den, ts);
        s.set_extra("Variable", Value::str(if v == "z^-1" { "z" } else { &v }));
        let skip = if ts != 0.0 && a.len() >= 2 && !is_text(&a[1]) {
            2
        } else {
            1
        };
        return finish_ctor(s, &a[skip..]);
    }
    let (data, pairs) = split_pairs(&a);
    let s = match data.len() {
        1 => {
            let m = mat_arg(&data[0], "tf")?;
            if m.r * m.c != 1 {
                return Err(model::mimo_tf());
            }
            Sys::gain(Kind::Tf, m.d[0], 0.0)
        }
        2 | 3 => {
            let num = vec_cell(&data[0])?;
            let den = vec_cell(&data[1])?;
            let ts = ts_arg(data.get(2))?;
            let mut s = Sys::new(Kind::Tf, Data::Empty, ts);
            // z^-1 variable: ascending powers of z^-1, pad to equal length.
            let zinv = pairs.chunks(2).any(|p| {
                p[0].to_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case("variable"))
                    && p.get(1)
                        .and_then(|v| v.to_str())
                        .is_some_and(|v| v == "z^-1" || v == "q")
            });
            let (num, den) = if zinv {
                let l = num.len().max(den.len());
                let pad = |mut v: Vec<f64>| {
                    v.resize(l, 0.0);
                    v
                };
                (pad(num), pad(den))
            } else {
                (num, den)
            };
            let (n, d) = model::tf_norm(&num, &den);
            s.data = Data::Tf { num: n, den: d };
            if num::ptrim(&den) == [0.0] {
                return Err(err(
                    "Control:ltiobject:tfProperties2",
                    "The denominator of a transfer function cannot be zero.",
                ));
            }
            s
        }
        _ => {
            return Err(err(
                "Control:general:InvalidSyntax",
                "Invalid syntax for tf: use tf(num,den), tf(num,den,Ts) or tf('s').",
            ));
        }
    };
    finish_ctor(s, pairs)
}

fn vec_cell(v: &Value) -> R<Vec<f64>> {
    let x = match v {
        Value::Cell(c) if c.data.len() == 1 => c.data[0].clone(),
        Value::Cell(_) => return Err(model::mimo_tf()),
        _ => v.clone(),
    };
    let n = x.to_num().ok_or_else(|| {
        err(
            "Control:ltiobject:tfProperties1",
            "Numerator and denominator must be vectors of real numbers.",
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
            "Numerator and denominator must contain finite values.",
        ));
    }
    if n.re.is_empty() {
        return Ok(vec![0.0]);
    }
    Ok(n.re)
}

fn roots_arg(v: &Value) -> R<Vec<C>> {
    let x = match v {
        Value::Cell(c) if c.data.len() == 1 => c.data[0].clone(),
        Value::Cell(c) if c.data.is_empty() => Value::empty(),
        Value::Cell(_) => return Err(model::mimo_tf()),
        _ => v.clone(),
    };
    cdata(&x).ok_or_else(|| {
        err(
            "Control:ltiobject:zpkProperties1",
            "Zeros and poles must be numeric vectors.",
        )
    })
}

fn zpk_ctor(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    if let Some(r) = convert_ctor(it, Kind::Zpk, &a)? {
        return Ok(r);
    }
    if a.is_empty() {
        return one(Sys::new(Kind::Zpk, Data::Empty, 0.0).to_value());
    }
    if let Some(v) = a[0].to_str().filter(|_| is_text(&a[0])) {
        let ts = if a.len() >= 2 && !is_text(&a[1]) {
            ts_arg(a.get(1))?
        } else {
            -1.0
        };
        let (z, ts) = match v.as_str() {
            "s" => (vec![C::default()], 0.0),
            "z" => (vec![C::default()], ts),
            _ => {
                return Err(err(
                    "Control:ltiobject:zpk1",
                    format!("Invalid variable name '{}' (use 's' or 'z').", v),
                ));
            }
        };
        let s = Sys::new(
            Kind::Zpk,
            Data::Zpk {
                z,
                p: vec![],
                k: C::real(1.0),
            },
            ts,
        );
        let skip = if ts != 0.0 && a.len() >= 2 && !is_text(&a[1]) {
            2
        } else {
            1
        };
        return finish_ctor(s, &a[skip..]);
    }
    let (data, pairs) = split_pairs(&a);
    let s = match data.len() {
        1 => {
            let k = cdata(&data[0]).unwrap_or_default();
            if k.len() != 1 {
                return Err(model::mimo_tf());
            }
            Sys::new(
                Kind::Zpk,
                Data::Zpk {
                    z: vec![],
                    p: vec![],
                    k: k[0],
                },
                0.0,
            )
        }
        3 | 4 => {
            let z = roots_arg(&data[0])?;
            let p = roots_arg(&data[1])?;
            let k = cdata(&data[2]).unwrap_or_default();
            if k.len() != 1 {
                return Err(model::mimo_tf());
            }
            let ts = ts_arg(data.get(3))?;
            Sys::new(Kind::Zpk, Data::Zpk { z, p, k: k[0] }, ts)
        }
        _ => {
            return Err(err(
                "Control:general:InvalidSyntax",
                "Invalid syntax for zpk: use zpk(z,p,k), zpk(z,p,k,Ts) or zpk('s').",
            ));
        }
    };
    finish_ctor(s, pairs)
}

fn ss_ctor(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    if let Some(r) = convert_ctor(it, Kind::Ss, &a)? {
        return Ok(r);
    }
    if a.is_empty() {
        return one(Sys::new(
            Kind::Ss,
            Data::Ss(Ss {
                a: Mat::zeros(0, 0),
                b: Mat::zeros(0, 0),
                c: Mat::zeros(0, 0),
                d: Mat::zeros(0, 0),
            }),
            0.0,
        )
        .to_value());
    }
    let (data, pairs) = split_pairs(&a);
    let s = match data.len() {
        1 => {
            let d = mat_arg(&data[0], "D")?;
            Sys::new(
                Kind::Ss,
                Data::Ss(Ss {
                    a: Mat::zeros(0, 0),
                    b: Mat::zeros(0, d.c),
                    c: Mat::zeros(d.r, 0),
                    d,
                }),
                0.0,
            )
        }
        3..=5 => {
            let am = mat_arg(&data[0], "A")?;
            let bm = mat_arg(&data[1], "B")?;
            let cm = mat_arg(&data[2], "C")?;
            let dm = match data.get(3) {
                Some(v) => mat_arg(v, "D")?,
                None => Mat::zeros(0, 0),
            };
            let ts = ts_arg(data.get(4))?;
            Sys::new(Kind::Ss, Data::Ss(model::check_ss(am, bm, cm, dm)?), ts)
        }
        _ => {
            return Err(err(
                "Control:general:InvalidSyntax",
                "Invalid syntax for ss: use ss(A,B,C,D) or ss(A,B,C,D,Ts).",
            ));
        }
    };
    finish_ctor(s, pairs)
}

fn pid_like_ctor(it: &mut Interp, a: Vec<Value>, kind: Kind) -> R<Vec<Value>> {
    if let Some(r) = convert_ctor(it, kind, &a)? {
        return Ok(r);
    }
    let (data, pairs) = split_pairs(&a);
    if data.len() > 5 {
        return Err(err(
            "Control:general:InvalidSyntax",
            format!("Too many input arguments for {}.", kind.name()),
        ));
    }
    let names: [&str; 4] = if kind == Kind::Pid {
        ["Kp", "Ki", "Kd", "Tf"]
    } else {
        ["Kp", "Ti", "Td", "N"]
    };
    let mut o = Sys::gain(kind, 1.0, 0.0).to_object();
    if data.is_empty() {
        o.set("Kp", Value::num(1.0));
    }
    for (i, v) in data.iter().take(4).enumerate() {
        o.set(names[i], v.clone());
    }
    if let Some(t) = data.get(4) {
        o.set("Ts", Value::num(ts_arg(Some(t))?));
    }
    let o = apply_pairs(o, pairs)?;
    one(normalised(o)?)
}

fn pid_ctor(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    pid_like_ctor(it, a, Kind::Pid)
}

fn pidstd_ctor(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    pid_like_ctor(it, a, Kind::PidStd)
}

// ---------------------------------------------------------------------------
// Data access
// ---------------------------------------------------------------------------

fn want_v(a: &[Value]) -> bool {
    a.iter()
        .skip(1)
        .any(|v| v.to_str().is_some_and(|s| s.eq_ignore_ascii_case("v")))
}

fn tfdata(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "tfdata")?;
    let s = sys_of(&a[0], "tfdata")?.convert(it, Kind::Tf)?;
    let (num, den) = match &s.data {
        Data::Tf { num, den } => (num.clone(), den.clone()),
        _ => (vec![], vec![]),
    };
    let ts = Value::num(s.ts);
    if want_v(&a) {
        return Ok(vec![row(num), row(den), ts]);
    }
    Ok(vec![cell1(row(num)), cell1(row(den)), ts])
}

fn zpkdata(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "zpkdata")?;
    let s = sys_of(&a[0], "zpkdata")?.convert(it, Kind::Zpk)?;
    let ts = Value::num(s.ts);
    let (z, p, k) = match &s.data {
        Data::Zpk { z, p, k } => (ccol(z), ccol(p), cvalue(vec![1, 1], &[*k])),
        _ => {
            return Ok(vec![
                Value::Cell(Array::new(vec![0, 0], vec![])),
                Value::Cell(Array::new(vec![0, 0], vec![])),
                Value::empty(),
                ts,
            ]);
        }
    };
    if want_v(&a) {
        return Ok(vec![z, p, k, ts]);
    }
    Ok(vec![cell1(z), cell1(p), k, ts])
}

fn ssdata(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "ssdata")?;
    let s = sys_of(&a[0], "ssdata")?;
    let m = s.ss_data(it)?;
    Ok(vec![
        m.a.to_value(),
        m.b.to_value(),
        m.c.to_value(),
        m.d.to_value(),
        Value::num(s.ts),
    ])
}

fn piddata(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "piddata")?;
    let s = sys_of(&a[0], "piddata")?.convert(it, Kind::Pid)?;
    let (kp, ki, kd, tf) = s.pid_gains().unwrap();
    Ok(vec![
        Value::num(kp),
        Value::num(ki),
        Value::num(kd),
        Value::num(tf),
        Value::num(s.ts),
    ])
}

fn ss2tf(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 4, "ss2tf")?;
    let m = model::check_ss(
        mat_arg(&a[0], "A")?,
        mat_arg(&a[1], "B")?,
        mat_arg(&a[2], "C")?,
        mat_arg(&a[3], "D")?,
    )?;
    let iu = match a.get(4) {
        Some(v) => arg_f64(v, "ss2tf")? as usize,
        None => 1,
    };
    if iu == 0 || iu > m.nu() {
        return Err(err(
            "Control:ss2tf:badInput",
            "ss2tf: input index out of range.",
        ));
    }
    let den = model::charpoly(it, &m.a)?;
    let ny = m.ny();
    let mut nums: Vec<Vec<f64>> = Vec::new();
    for i in 0..ny {
        let (n, _) = model::ss_tf(it, &m, i, iu - 1)?;
        let mut n = num::ptrim(&n);
        while n.len() < den.len() {
            n.insert(0, 0.0);
        }
        nums.push(n);
    }
    let w = den.len();
    let mut d = vec![0.0; ny * w];
    for (i, n) in nums.iter().enumerate() {
        for j in 0..w {
            d[i + j * ny] = n[j];
        }
    }
    Ok(vec![Value::matrix(vec![ny, w], d), row(den)])
}

fn tf2ss(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "tf2ss")?;
    let m = model::tf_ss(&vec_arg(&a[0], "num")?, &vec_arg(&a[1], "den")?)?;
    Ok(vec![
        m.a.to_value(),
        m.b.to_value(),
        m.c.to_value(),
        m.d.to_value(),
    ])
}

fn isstatic(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "isstatic")?;
    let s = sys_of(&a[0], "isstatic")?;
    let p = s.poles(it)?;
    let z = if s.size() == (1, 1) {
        s.zeros(it)?
    } else {
        vec![]
    };
    one(Value::bool(
        p.is_empty() && z.is_empty() && s.input_delay() == 0.0,
    ))
}

fn order(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "order")?;
    let s = sys_of(&a[0], "order")?;
    let n = match &s.data {
        Data::Ss(m) => m.nx(),
        _ => s.poles(it)?.len(),
    };
    one(Value::num(n as f64))
}

// ---------------------------------------------------------------------------
// Overloaded methods
// ---------------------------------------------------------------------------

fn m_size(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "size")?;
    let s = sys_of(&a[0], "size")?;
    let (ny, nu) = s.size();
    if let Some(d) = a.get(1) {
        let k = arg_f64(d, "size")?;
        return one(Value::num(match k as usize {
            1 => ny as f64,
            2 => nu as f64,
            _ => 1.0,
        }));
    }
    if nargout >= 2 {
        let mut out = vec![Value::num(ny as f64), Value::num(nu as f64)];
        for _ in 2..nargout {
            out.push(Value::num(1.0));
        }
        return Ok(out);
    }
    one(row(vec![ny as f64, nu as f64]))
}

fn m_isa(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "isa")?;
    let c = arg_str(&a[1], "isa")?;
    if !is_lti(&a[0]) {
        return one(Value::bool(super::core::isa_name(&a[0], &c)));
    }
    let cls = a[0].class_name();
    let generic = [
        "lti",
        "DynamicSystem",
        "InputOutputModel",
        "numlti",
        "ltipack.SingleRateSystem",
    ];
    let yes = cls == c
        || generic.contains(&c.as_str())
        || (c == "pidstd" && false)
        || (c == "StaticModel" && false);
    one(Value::bool(yes))
}

fn m_disp(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "disp")?;
    let s = sys_of(&a[0], "disp")?;
    let t = show::disp(&s);
    it.print(&t);
    Ok(vec![])
}

fn m_display(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "display")?;
    let name = a
        .get(1)
        .and_then(|v| v.to_str())
        .unwrap_or_else(|| "ans".into());
    let t = display_text(&name, &a[0]);
    it.print(&t);
    Ok(vec![])
}

fn m_set(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "set")?;
    let Value::Object(o) = &a[0] else {
        return Err(err(
            "Control:general:invalidType",
            "set: the first argument must be a model.",
        ));
    };
    if a.len() == 1 {
        // List settable properties.
        let kind = Kind::from_name(&o.class).unwrap();
        let mut t = String::new();
        for n in prop_names(kind) {
            t.push_str(&format!("    {}\n", n));
        }
        it.print(&t);
        return Ok(vec![]);
    }
    let o = apply_pairs((**o).clone(), &a[1..])?;
    let v = normalised(o)?;
    let _ = nargout;
    one(v)
}

fn m_get(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "get")?;
    let Value::Object(o) = &a[0] else {
        return Err(err(
            "Control:general:invalidType",
            "get: the first argument must be a model.",
        ));
    };
    match a.get(1) {
        Some(Value::Cell(c)) => {
            let vals = c
                .data
                .iter()
                .map(|n| get_prop(&a[0], &n.to_str().unwrap_or_default()))
                .collect::<R<Vec<_>>>()?;
            one(Value::Cell(Array::new(vec![1, vals.len()], vals)))
        }
        Some(n) => one(get_prop(&a[0], &arg_str(n, "get")?)?),
        None => {
            let mut s = StructArray::new_scalar();
            for (n, v) in &o.props {
                s.set(0, n, v.clone());
            }
            if nargout == 0 {
                let t = indy_value::format::display_text("ans", &Value::Struct(s.clone()));
                it.print(&t);
                return Ok(vec![]);
            }
            one(Value::Struct(s))
        }
    }
}

fn m_properties(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "properties")?;
    let kind = Kind::from_name(&a[0].class_name()).unwrap();
    let names = prop_names(kind);
    if nargout == 0 {
        let mut t = format!("Properties for class {}:\n\n", kind.name());
        for n in &names {
            t.push_str(&format!("    {}\n", n));
        }
        t.push('\n');
        it.print(&t);
        return Ok(vec![]);
    }
    one(Value::Cell(Array::new(
        vec![names.len(), 1],
        names.iter().map(|n| Value::str(n)).collect(),
    )))
}

fn m_isprop(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "isprop")?;
    let kind = Kind::from_name(&a[0].class_name()).unwrap();
    let n = arg_str(&a[1], "isprop")?;
    one(Value::bool(
        prop_names(kind).iter().any(|p| p.eq_ignore_ascii_case(&n)),
    ))
}

fn m_inv(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "inv")?;
    let s = sys_of(&a[0], "inv")?;
    one(ops::inv(it, &s)?.to_value())
}

// ---------------------------------------------------------------------------
// Analysis builtins
// ---------------------------------------------------------------------------

fn pole(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "pole")?;
    let s = sys_of(&a[0], "pole")?;
    one(ccol(&s.poles(it)?))
}

fn zero(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "zero")?;
    let s = sys_of(&a[0], "zero")?;
    let z = s.zeros(it)?;
    let k = if s.size() == (1, 1) {
        s.zpk_data(it)?.2
    } else {
        C::default()
    };
    Ok(vec![ccol(&z), cvalue(vec![1, 1], &[k])])
}

fn dcgain(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "dcgain")?;
    let s = sys_of(&a[0], "dcgain")?;
    one(analysis::dcgain(it, &s)?.to_value())
}

fn damp(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "damp")?;
    let s = sys_of(&a[0], "damp")?;
    let (wn, zeta, p) = analysis::damp(it, &s)?;
    if nargout == 0 {
        let mut t = String::from(
            "                                                                       \n         Pole              Damping       Frequency      Time Constant  \n",
        );
        let unit = "(rad/seconds)";
        t.push_str(&format!(
            "                                       {}      (seconds)    \n                                                                       \n",
            unit
        ));
        for i in 0..p.len() {
            let e = |x: f64| indy_value::format::fmt_e(x, 2, false);
            let pz = if p[i].im == 0.0 {
                format!("{:>9}         ", e(p[i].re))
            } else {
                format!(
                    "{:>9} {} {}i",
                    e(p[i].re),
                    if p[i].im < 0.0 { "-" } else { "+" },
                    e(p[i].im.abs())
                )
            };
            let tau = if zeta[i] * wn[i] != 0.0 {
                1.0 / (zeta[i] * wn[i])
            } else {
                f64::INFINITY
            };
            t.push_str(&format!(
                " {}     {:>9}      {:>9}         {:>9}     \n",
                pz,
                e(zeta[i]),
                e(wn[i]),
                e(tau)
            ));
        }
        t.push('\n');
        it.print(&t);
        return Ok(vec![]);
    }
    Ok(vec![col(wn), col(zeta), ccol(&p)])
}

fn stepinfo(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "stepinfo")?;
    let mut st = 0.02;
    let mut rt = (0.1, 0.9);
    let first_text = a.iter().position(is_text).unwrap_or(a.len());
    for p in a[first_text..].chunks(2) {
        let n = p[0].to_str().unwrap_or_default().to_ascii_lowercase();
        let v = p.get(1).cloned().unwrap_or_default();
        match n.as_str() {
            "settlingtimethreshold" => st = arg_f64(&v, "stepinfo")?,
            "risetimelimits" => {
                let x = vec_arg(&v, "RiseTimeLimits")?;
                if x.len() == 2 {
                    rt = (x[0], x[1]);
                }
            }
            _ => {}
        }
    }
    let pos = &a[..first_text];
    let info = if is_lti(&pos[0]) {
        let s = Sys::from_value(&pos[0])?;
        analysis::stepinfo_sys(it, &s, st, rt)?
    } else {
        let y = vec_arg(&pos[0], "y")?;
        let t = match pos.get(1) {
            Some(v) if !v.is_empty() => vec_arg(v, "t")?,
            _ => (0..y.len()).map(|k| k as f64).collect(),
        };
        if t.len() != y.len() {
            return Err(err(
                "Control:analysis:stepinfo1",
                "The vectors y and t must have the same length.",
            ));
        }
        let yf = match pos.get(2) {
            Some(v) => arg_f64(v, "stepinfo")?,
            None => *y.last().unwrap_or(&f64::NAN),
        };
        let yi = match pos.get(3) {
            Some(v) => arg_f64(v, "stepinfo")?,
            None => 0.0,
        };
        analysis::stepinfo_data(&t, &y, yf, yi, st, rt)
    };
    let mut s = StructArray::new_scalar();
    for (n, v) in info.fields() {
        s.set(0, n, Value::num(v));
    }
    one(Value::Struct(s))
}

fn margin(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "margin")?;
    let s = if a.len() >= 3 && !is_lti(&a[0]) {
        return Err(not_supported(
            "margin(mag, phase, w) on frequency-response data",
        ));
    } else {
        sys_of(&a[0], "margin")?
    };
    let m = analysis::margin(it, &s)?;
    if nargout == 0 {
        plots::margin_plot(it, &s, &m)?;
        return Ok(vec![]);
    }
    Ok(vec![
        Value::num(m.gm),
        Value::num(m.pm),
        Value::num(m.wcg),
        Value::num(m.wcp),
    ])
}

fn bandwidth(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "bandwidth")?;
    if !is_lti(&a[0]) {
        return matrix_bandwidth(&a, nargout);
    }
    let s = sys_of(&a[0], "bandwidth")?;
    let drop = match a.get(1) {
        Some(v) => arg_f64(v, "bandwidth")?,
        None => -3.0,
    };
    one(Value::num(analysis::bandwidth(it, &s, drop)?))
}

/// `bandwidth(A)`: lower and upper bandwidth of a numeric matrix.
fn matrix_bandwidth(a: &[Value], nargout: usize) -> R<Vec<Value>> {
    let m = mat_arg(&a[0], "bandwidth")?;
    let (mut lo, mut up) = (0usize, 0usize);
    for j in 0..m.c {
        for i in 0..m.r {
            if m.at(i, j) != 0.0 {
                lo = lo.max(i.saturating_sub(j));
                up = up.max(j.saturating_sub(i));
            }
        }
    }
    match a.get(1).and_then(|v| v.to_str()).as_deref() {
        Some("lower") => one(Value::num(lo as f64)),
        Some("upper") => one(Value::num(up as f64)),
        Some(other) => Err(err(
            "MATLAB:bandwidth:invalidType",
            format!(
                "bandwidth: type must be 'lower' or 'upper', got '{}'.",
                other
            ),
        )),
        None if nargout >= 2 => Ok(vec![Value::num(lo as f64), Value::num(up as f64)]),
        None => Err(err(
            "MATLAB:bandwidth:invalidType",
            "bandwidth(A) needs two outputs, or the type 'lower' or 'upper'.",
        )),
    }
}

fn ab_of(it: &mut Interp, a: &[Value], fname: &str, second: &str) -> R<(Mat, Mat)> {
    if is_lti(&a[0]) {
        let s = Sys::from_value(&a[0])?;
        let m = s.ss_data(it)?;
        return Ok(if second == "B" {
            (m.a, m.b)
        } else {
            (m.a, m.c)
        });
    }
    need(a, 2, fname)?;
    Ok((mat_arg(&a[0], "A")?, mat_arg(&a[1], second)?))
}

fn ctrb(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "ctrb")?;
    let (am, bm) = ab_of(it, &a, "ctrb", "B")?;
    if am.r != am.c || bm.r != am.r {
        return Err(err(
            "Control:general:dimension",
            "ctrb: A must be square and B must have as many rows as A.",
        ));
    }
    one(analysis::ctrb(&am, &bm).to_value())
}

fn obsv(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "obsv")?;
    let (am, cm) = ab_of(it, &a, "obsv", "C")?;
    if am.r != am.c || cm.c != am.r {
        return Err(err(
            "Control:general:dimension",
            "obsv: A must be square and C must have as many columns as A.",
        ));
    }
    one(analysis::obsv(&am, &cm).to_value())
}

fn place(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 3, "place")?;
    let am = mat_arg(&a[0], "A")?;
    let bm = mat_arg(&a[1], "B")?;
    let p = cdata(&a[2]).unwrap_or_default();
    one(analysis::acker(&am, &bm, &p)?.to_value())
}

fn lqr(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let (am, bm, rest) = if !a.is_empty() && is_lti(&a[0]) {
        let s = Sys::from_value(&a[0])?;
        if s.is_discrete() {
            return Err(not_supported("lqr for discrete-time models (dlqr)"));
        }
        let m = s.ss_data(it)?;
        (m.a, m.b, &a[1..])
    } else {
        need(&a, 4, "lqr")?;
        (mat_arg(&a[0], "A")?, mat_arg(&a[1], "B")?, &a[2..])
    };
    need(rest, 2, "lqr")?;
    let q = mat_arg(&rest[0], "Q")?;
    let r = mat_arg(&rest[1], "R")?;
    let nm = match rest.get(2) {
        Some(v) => Some(mat_arg(v, "N")?),
        None => None,
    };
    let x = analysis::care(&am, &bm, &q, &r, nm.as_ref())?;
    let mut rhs = bm.t().mul(&x);
    if let Some(n) = &nm {
        rhs = rhs.add(&n.t());
    }
    let k = r
        .solve(&rhs)
        .ok_or_else(|| err("Control:design:lqr1", "R must be invertible."))?;
    let cl = num::eigvals(it, &am.sub(&bm.mul(&k)))?;
    Ok(vec![k.to_value(), x.to_value(), ccol(&cl)])
}

fn care(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 3, "care")?;
    let am = mat_arg(&a[0], "A")?;
    let bm = mat_arg(&a[1], "B")?;
    let q = mat_arg(&a[2], "Q")?;
    let r = match a.get(3) {
        Some(v) => mat_arg(v, "R")?,
        None => Mat::eye(bm.c),
    };
    let nm = match a.get(4) {
        Some(v) if !v.is_empty() => Some(mat_arg(v, "S")?),
        _ => None,
    };
    if a.get(5).is_some_and(|v| !v.is_empty()) {
        return Err(not_supported("care with a descriptor matrix E"));
    }
    let x = analysis::care(&am, &bm, &q, &r, nm.as_ref())?;
    let mut rhs = bm.t().mul(&x);
    if let Some(n) = &nm {
        rhs = rhs.add(&n.t());
    }
    let g = r
        .solve(&rhs)
        .ok_or_else(|| err("Control:design:care1", "R must be invertible."))?;
    let l = num::eigvals(it, &am.sub(&bm.mul(&g)))?;
    Ok(vec![x.to_value(), ccol(&l), g.to_value()])
}

fn lyap(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "lyap")?;
    let am = mat_arg(&a[0], "A")?;
    if a.len() >= 4 {
        return Err(not_supported("lyap with a descriptor matrix E"));
    }
    if a.len() == 3 {
        let bm = mat_arg(&a[1], "B")?;
        let cm = mat_arg(&a[2], "C")?;
        return one(analysis::sylvester(&am, &bm, &cm)?.to_value());
    }
    let q = mat_arg(&a[1], "Q")?;
    if am.r != am.c || q.r != am.r || q.c != am.r {
        return Err(err(
            "Control:design:lyap2",
            "lyap: A and Q must be square matrices of the same size.",
        ));
    }
    one(analysis::sylvester(&am, &am.t(), &q)?.to_value())
}

fn dlyap(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "dlyap")?;
    if a.len() > 2 {
        return Err(not_supported("dlyap with more than two arguments"));
    }
    let am = mat_arg(&a[0], "A")?;
    let q = mat_arg(&a[1], "Q")?;
    if am.r != am.c || q.r != am.r || q.c != am.r {
        return Err(err(
            "Control:design:lyap2",
            "dlyap: A and Q must be square matrices of the same size.",
        ));
    }
    one(analysis::dlyap(&am, &q)?.to_value())
}

fn gram(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "gram")?;
    let s = sys_of(&a[0], "gram")?;
    let m = s.ss_data(it)?;
    let kind = arg_str(&a[1], "gram")?.to_ascii_lowercase();
    let (am, q) = match kind.as_str() {
        "c" => (m.a.clone(), m.b.mul(&m.b.t())),
        "o" => (m.a.t(), m.c.t().mul(&m.c)),
        _ => return Err(not_supported("This gram type")),
    };
    let x = if s.is_discrete() {
        analysis::dlyap(&am, &q)?
    } else {
        analysis::sylvester(&am, &am.t(), &q)?
    };
    one(x.to_value())
}

fn minreal(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "minreal")?;
    let s = sys_of(&a[0], "minreal")?;
    let tol = match a.get(1) {
        Some(v) => arg_f64(v, "minreal")?,
        None => f64::EPSILON.sqrt(),
    };
    one(analysis::minreal(it, &s, tol)?.to_value())
}

fn pade(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "pade")?;
    let n = arg_f64(&a[1], "pade")? as usize;
    if is_lti(&a[0]) {
        let s = Sys::from_value(&a[0])?;
        let tau = s.input_delay();
        let mut base = s.clone();
        base.extra.retain(|(n, _)| !n.ends_with("Delay"));
        if tau == 0.0 {
            return one(base.to_value());
        }
        let (pn, pd) = resp::pade(tau, n);
        let d = Sys::tf(&pn, &pd, 0.0);
        let r = ops::mul(it, Operand::Sys(base), Operand::Sys(d))?;
        return one(r.to_value());
    }
    let t = arg_f64(&a[0], "pade")?;
    let (pn, pd) = resp::pade(t, n);
    if nargout == 0 {
        return Err(not_supported("pade plot form"));
    }
    Ok(vec![row(pn), row(pd)])
}

fn rlocus(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "rlocus")?;
    let s = sys_of(&a[0], "rlocus")?;
    let k = match a.get(1) {
        Some(v) if !is_text(v) => Some(vec_arg(v, "k")?),
        _ => None,
    };
    let (cols, ks) = analysis::rlocus(it, &s, k)?;
    let n = cols.first().map(|c| c.len()).unwrap_or(0);
    let mut data = Vec::with_capacity(n * cols.len());
    for c in &cols {
        data.extend_from_slice(c);
    }
    if nargout == 0 {
        plots::rlocus_plot(it, &s, &cols)?;
        return Ok(vec![]);
    }
    Ok(vec![cvalue(vec![n, cols.len()], &data), row(ks)])
}

fn pzmap(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "pzmap")?;
    let s = sys_of(&a[0], "pzmap")?;
    let p = s.poles(it)?;
    let z = if s.size() == (1, 1) {
        s.zeros(it)?
    } else {
        vec![]
    };
    if nargout == 0 {
        plots::pzmap_plot(it, &s, &p, &z)?;
        return Ok(vec![]);
    }
    Ok(vec![ccol(&p), ccol(&z)])
}

// ---------------------------------------------------------------------------
// Frequency responses
// ---------------------------------------------------------------------------

fn evalfr(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "evalfr")?;
    let s = sys_of(&a[0], "evalfr")?;
    let x = cdata(&a[1]).unwrap_or_default();
    let x = *x.first().ok_or_else(|| {
        err(
            "Control:analysis:evalfr1",
            "evalfr needs a scalar frequency point.",
        )
    })?;
    let (ny, nu) = s.size();
    let h = s.eval_at(it, x)?;
    one(cvalue(vec![ny, nu], &h))
}

/// Frequency vector argument: vector, `{wmin, wmax}` or default.
fn freq_arg(it: &mut Interp, s: &Sys, v: Option<&Value>) -> R<Vec<f64>> {
    match v {
        Some(Value::Cell(c)) if c.data.len() == 2 => {
            let lo = arg_f64(&c.data[0], "w")?.log10();
            let hi = arg_f64(&c.data[1], "w")?.log10();
            let n = 400;
            Ok((0..n)
                .map(|k| 10f64.powf(lo + (hi - lo) * k as f64 / (n - 1) as f64))
                .collect())
        }
        Some(v) if !v.is_empty() && !is_text(v) => vec_arg(v, "w"),
        _ => resp::default_freqs(it, s),
    }
}

/// ny×nu×N array from per-frequency blocks.
fn freq_cube(ny: usize, nu: usize, h: &[Vec<C>], f: impl Fn(C) -> f64) -> Value {
    let mut d = Vec::with_capacity(ny * nu * h.len());
    for blk in h {
        d.extend(blk.iter().map(|z| f(*z)));
    }
    Value::matrix(vec![ny, nu, h.len()], d)
}

fn freqresp(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "freqresp")?;
    let s = sys_of(&a[0], "freqresp")?;
    let w = freq_arg(it, &s, a.get(1))?;
    let (ny, nu) = s.size();
    let h = resp::freqresp(it, &s, &w)?;
    let flat: Vec<C> = h.iter().flatten().copied().collect();
    Ok(vec![cvalue(vec![ny, nu, w.len()], &flat), col(w)])
}

/// Split plotting arguments: systems with optional line specs, then
/// trailing numeric arguments.
type Systems = Vec<(Sys, Option<String>)>;

fn plot_args(a: &[Value]) -> R<(Systems, Vec<Value>)> {
    let mut systems: Vec<(Sys, Option<String>)> = Vec::new();
    let mut rest = Vec::new();
    for v in a {
        if is_lti(v) && rest.is_empty() {
            systems.push((Sys::from_value(v)?, None));
        } else if is_text(v) && !systems.is_empty() && rest.is_empty() {
            systems.last_mut().unwrap().1 = v.to_str();
        } else {
            rest.push(v.clone());
        }
    }
    if systems.is_empty() {
        if let Some(v) = a.first() {
            systems.push((sys_of(v, "response")?, None));
            rest.remove(0);
        } else {
            return Err(err("MATLAB:minrhs", "Not enough input arguments."));
        }
    }
    Ok((systems, rest))
}

fn bode(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "bode")?;
    let (systems, rest) = plot_args(&a)?;
    if nargout == 0 {
        plots::bode_plot(it, &systems, rest.first(), true)?;
        return Ok(vec![]);
    }
    let s = &systems[0].0;
    let w = freq_arg(it, s, rest.first())?;
    let (ny, nu) = s.size();
    let h = resp::freqresp(it, s, &w)?;
    let mag = freq_cube(ny, nu, &h, |z| z.abs());
    // Phase unwrapped per channel.
    let mut ph = vec![0.0; ny * nu * w.len()];
    for ch in 0..ny * nu {
        let series: Vec<C> = h.iter().map(|b| b[ch]).collect();
        for (k, p) in resp::phase_deg(&series).into_iter().enumerate() {
            ph[ch + k * ny * nu] = p;
        }
    }
    Ok(vec![mag, Value::matrix(vec![ny, nu, w.len()], ph), col(w)])
}

fn bodemag(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "bodemag")?;
    let (systems, rest) = plot_args(&a)?;
    if nargout == 0 {
        plots::bode_plot(it, &systems, rest.first(), false)?;
        return Ok(vec![]);
    }
    let s = &systems[0].0;
    let w = freq_arg(it, s, rest.first())?;
    let (ny, nu) = s.size();
    let h = resp::freqresp(it, s, &w)?;
    Ok(vec![freq_cube(ny, nu, &h, |z| z.abs()), col(w)])
}

fn nyquist(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "nyquist")?;
    let (systems, rest) = plot_args(&a)?;
    if nargout == 0 {
        plots::nyquist_plot(it, &systems, rest.first())?;
        return Ok(vec![]);
    }
    let s = &systems[0].0;
    let w = freq_arg(it, s, rest.first())?;
    let (ny, nu) = s.size();
    let h = resp::freqresp(it, s, &w)?;
    Ok(vec![
        freq_cube(ny, nu, &h, |z| z.re),
        freq_cube(ny, nu, &h, |z| z.im),
        col(w),
    ])
}

// ---------------------------------------------------------------------------
// Time responses
// ---------------------------------------------------------------------------

/// Time argument: vector, final time, or default.
fn time_arg(it: &mut Interp, s: &Sys, v: Option<&Value>, impulse_like: bool) -> R<Vec<f64>> {
    match v {
        Some(v) if !v.is_empty() && !is_text(v) => {
            let t = vec_arg(v, "t")?;
            if t.len() == 1 {
                let tf = t[0];
                if tf.is_nan() || tf <= 0.0 {
                    return Err(err(
                        "Control:analysis:step1",
                        "The final time must be positive.",
                    ));
                }
                if s.is_discrete() {
                    let ts = if s.ts > 0.0 { s.ts } else { 1.0 };
                    let n = (tf / ts + 1e-9).floor() as usize;
                    return Ok((0..=n).map(|k| k as f64 * ts).collect());
                }
                let def = resp::default_time(it, s, impulse_like)?;
                let dt0 = def.get(1).copied().unwrap_or(tf / 1000.0);
                let n = ((tf / dt0).ceil() as usize).clamp(100, 100000);
                return Ok((0..=n).map(|k| tf * k as f64 / n as f64).collect());
            }
            Ok(t)
        }
        _ => resp::default_time(it, s, impulse_like),
    }
}

/// Pack per-channel simulations: y is N×ny (one input) or N×ny×nu.
fn pack(sims: &[resp::Sim], ny: usize, nx: usize, n: usize) -> (Value, Value) {
    let nu = sims.len();
    let mut y = vec![0.0; n * ny * nu];
    let mut x = vec![0.0; n * nx * nu];
    for (j, s) in sims.iter().enumerate() {
        for k in 0..n {
            for i in 0..ny {
                y[k + i * n + j * n * ny] = s.y[k][i];
            }
            for i in 0..nx {
                x[k + i * n + j * n * nx] = s.x[k][i];
            }
        }
    }
    (
        Value::matrix(vec![n, ny, nu], y),
        Value::matrix(vec![n, nx, nu], x),
    )
}

fn step_like(it: &mut Interp, a: Vec<Value>, nargout: usize, impulse: bool) -> R<Vec<Value>> {
    let fname = if impulse { "impulse" } else { "step" };
    need(&a, 1, fname)?;
    let (systems, rest) = plot_args(&a)?;
    if nargout == 0 {
        let mut curves = Vec::new();
        for (s, spec) in &systems {
            let t = time_arg(it, s, rest.first(), impulse)?;
            let (ny, nu) = s.size();
            for j in 0..nu {
                let sim = if impulse {
                    resp::impulse(it, s, &t, j)?
                } else {
                    resp::step(it, s, &t, j)?
                };
                for i in 0..ny {
                    let y: Vec<f64> = sim.y.iter().map(|v| v[i]).collect();
                    curves.push((t.clone(), y, spec.clone(), s.is_discrete()));
                }
            }
        }
        plots::time_plot(
            it,
            &curves,
            if impulse {
                "Impulse Response"
            } else {
                "Step Response"
            },
        )?;
        return Ok(vec![]);
    }
    let s = &systems[0].0;
    let t = time_arg(it, s, rest.first(), impulse)?;
    let (ny, nu) = s.size();
    let mut sims = Vec::new();
    for j in 0..nu {
        sims.push(if impulse {
            resp::impulse(it, s, &t, j)?
        } else {
            resp::step(it, s, &t, j)?
        });
    }
    let nx = sims
        .first()
        .and_then(|s| s.x.first())
        .map_or(0, |x| x.len());
    let (y, x) = pack(&sims, ny, nx, t.len());
    Ok(vec![y, col(t), x])
}

fn step(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    step_like(it, a, nargout, false)
}

fn impulse(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    step_like(it, a, nargout, true)
}

fn initial(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "initial")?;
    let s = sys_of(&a[0], "initial")?;
    if !matches!(s.data, Data::Ss(_)) {
        return Err(err(
            "Control:analysis:initial2",
            "initial requires a state-space model (use ss).",
        ));
    }
    let x0 = vec_arg(&a[1], "x0")?;
    let t = time_arg(it, &s, a.get(2), false)?;
    let sim = resp::initial(it, &s, &t, &x0)?;
    let ny = s.size().0;
    if nargout == 0 {
        let mut curves = Vec::new();
        for i in 0..ny {
            curves.push((
                t.clone(),
                sim.y.iter().map(|v| v[i]).collect(),
                None,
                s.is_discrete(),
            ));
        }
        plots::time_plot(it, &curves, "Response to Initial Conditions")?;
        return Ok(vec![]);
    }
    let (y, x) = pack(std::slice::from_ref(&sim), ny, x0.len(), t.len());
    Ok(vec![y, col(t), x])
}

fn lsim(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "lsim")?;
    let s = sys_of(&a[0], "lsim")?;
    let (_, nu) = s.size();
    let um = Mat::from_value(&a[1], "u")?;
    // u: N×nu (a vector counts as one input).
    let (n_u, u_rows): (usize, Vec<Vec<f64>>) = if nu == 1 && (um.r == 1 || um.c == 1) {
        (um.d.len(), um.d.iter().map(|x| vec![*x]).collect())
    } else if um.c == nu {
        (
            um.r,
            (0..um.r)
                .map(|k| (0..nu).map(|j| um.at(k, j)).collect())
                .collect(),
        )
    } else if um.r == nu {
        (
            um.c,
            (0..um.c)
                .map(|k| (0..nu).map(|j| um.at(j, k)).collect())
                .collect(),
        )
    } else {
        return Err(err(
            "Control:analysis:lsim1",
            format!(
                "The input signal u must have {} columns (one per input).",
                nu
            ),
        ));
    };
    // MATLAB picks the hold from the signal: piecewise-constant inputs
    // (most steps without change, some jumps) use ZOH, smooth ones FOH.
    let mut method = {
        let mut flat = 0usize;
        let mut moving = 0usize;
        for w in u_rows.windows(2) {
            if w[0] == w[1] {
                flat += 1;
            } else {
                moving += 1;
            }
        }
        if moving > 0 && flat >= moving {
            resp::Hold::Zoh
        } else {
            resp::Hold::Foh
        }
    };
    let mut pos: Vec<&Value> = Vec::new();
    for v in &a[2..] {
        if is_text(v) {
            match v.to_str().unwrap_or_default().to_ascii_lowercase().as_str() {
                "zoh" => method = resp::Hold::Zoh,
                "foh" => method = resp::Hold::Foh,
                other => {
                    return Err(err(
                        "Control:analysis:lsim5",
                        format!("Unknown interpolation method '{}' (zoh or foh).", other),
                    ));
                }
            }
        } else {
            pos.push(v);
        }
    }
    let t = match pos.first() {
        Some(v) if !v.is_empty() => vec_arg(v, "t")?,
        _ => {
            if s.is_discrete() {
                let ts = if s.ts > 0.0 { s.ts } else { 1.0 };
                (0..n_u).map(|k| k as f64 * ts).collect()
            } else {
                return Err(err(
                    "Control:analysis:lsim3",
                    "lsim needs a time vector t for continuous-time models.",
                ));
            }
        }
    };
    if t.len() != n_u {
        return Err(err(
            "Control:analysis:lsim1",
            "The input signal u and the time vector t must have the same number of samples.",
        ));
    }
    let nx = s.ss_data(it)?.nx();
    let x0 = match pos.get(1) {
        Some(v) if !v.is_empty() => {
            let x = vec_arg(v, "x0")?;
            if x.len() != nx {
                return Err(err(
                    "Control:analysis:lsim6",
                    format!("The initial state x0 must have {} entries.", nx),
                ));
            }
            x
        }
        _ => vec![0.0; nx],
    };
    let sim = resp::lsim(it, &s, &t, &u_rows, &x0, method)?;
    let ny = s.size().0;
    if nargout == 0 {
        let mut curves = Vec::new();
        for i in 0..ny {
            curves.push((
                t.clone(),
                sim.y.iter().map(|v| v[i]).collect(),
                None,
                s.is_discrete(),
            ));
        }
        let inputs: Vec<Vec<f64>> = (0..nu)
            .map(|j| u_rows.iter().map(|r| r[j]).collect())
            .collect();
        plots::lsim_plot(it, &t, &inputs, &curves)?;
        return Ok(vec![]);
    }
    let n = t.len();
    let mut y = vec![0.0; n * ny];
    let mut x = vec![0.0; n * nx];
    for k in 0..n {
        for i in 0..ny {
            y[k + i * n] = sim.y[k][i];
        }
        for i in 0..nx {
            x[k + i * n] = sim.x[k][i];
        }
    }
    Ok(vec![
        Value::matrix(vec![n, ny], y),
        col(t),
        Value::matrix(vec![n, nx], x),
    ])
}

// ---------------------------------------------------------------------------
// Discretisation
// ---------------------------------------------------------------------------

fn method_args(rest: &[Value]) -> R<resp::Method> {
    let mut name = "zoh".to_string();
    let mut prewarp: Option<f64> = None;
    let mut i = 0;
    while i < rest.len() {
        match &rest[i] {
            Value::Struct(st) => {
                if let Some(m) = st.get(0, "Method").and_then(|v| v.to_str()) {
                    name = m;
                }
                if let Some(w) = st.get(0, "PrewarpFrequency").and_then(|v| v.scalar_f64())
                    && w > 0.0
                {
                    prewarp = Some(w);
                }
            }
            v if is_text(v) => {
                name = v.to_str().unwrap_or_default();
                if name.eq_ignore_ascii_case("prewarp")
                    && let Some(w) = rest.get(i + 1).and_then(|v| v.scalar_f64())
                {
                    prewarp = Some(w);
                    i += 1;
                }
            }
            v => {
                if let Some(w) = v.scalar_f64() {
                    prewarp = Some(w);
                }
            }
        }
        i += 1;
    }
    resp::parse_method(&name, prewarp)
}

fn c2d(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "c2d")?;
    let s = sys_of(&a[0], "c2d")?;
    let ts = arg_f64(&a[1], "c2d")?;
    let m = method_args(&a[2..])?;
    one(resp::c2d(it, &s, ts, m)?.to_value())
}

fn d2c(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "d2c")?;
    let s = sys_of(&a[0], "d2c")?;
    let m = method_args(&a[1..])?;
    one(resp::d2c(it, &s, m)?.to_value())
}

fn c2d_options(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let mut st = StructArray::new_scalar();
    st.set(0, "Method", Value::str("zoh"));
    st.set(0, "PrewarpFrequency", Value::num(0.0));
    st.set(0, "FractDelayApproxOrder", Value::num(0.0));
    for p in a.chunks(2) {
        let n = arg_str(&p[0], "c2dOptions")?;
        let key = ["Method", "PrewarpFrequency", "FractDelayApproxOrder"]
            .iter()
            .find(|k| k.eq_ignore_ascii_case(&n))
            .ok_or_else(|| {
                err(
                    "Control:general:InvalidSyntax",
                    format!("Unknown c2dOptions option '{}'.", n),
                )
            })?;
        st.set(0, key, p.get(1).cloned().unwrap_or_default());
    }
    one(Value::Struct(st))
}

// ---------------------------------------------------------------------------
// Interconnection
// ---------------------------------------------------------------------------

fn feedback(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "feedback")?;
    let sign = match a.get(2) {
        Some(v) if !is_text(v) && v.numel() == 1 => {
            let x = arg_f64(v, "feedback")?;
            if x >= 0.0 { 1.0 } else { -1.0 }
        }
        Some(_) => {
            return Err(not_supported(
                "feedback with input/output selection vectors",
            ));
        }
        None => -1.0,
    };
    let x = operand(&a[0], "sys1")?;
    let y = operand(&a[1], "sys2")?;
    one(ops::feedback(it, x, y, sign)?.to_value())
}

fn series(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "series")?;
    if a.len() > 2 {
        return Err(not_supported("series with input/output selection vectors"));
    }
    let x = operand(&a[0], "sys1")?;
    let y = operand(&a[1], "sys2")?;
    one(ops::mul(it, y, x)?.to_value())
}

fn parallel(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "parallel")?;
    if a.len() > 2 {
        return Err(not_supported(
            "parallel with input/output selection vectors",
        ));
    }
    let x = operand(&a[0], "sys1")?;
    let y = operand(&a[1], "sys2")?;
    one(ops::add(it, x, y, 1.0)?.to_value())
}

fn append(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "append")?;
    let parts = a
        .iter()
        .map(|v| operand(v, "append"))
        .collect::<R<Vec<_>>>()?;
    one(ops::append(it, parts)?.to_value())
}
