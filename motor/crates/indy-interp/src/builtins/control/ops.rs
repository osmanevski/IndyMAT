//! Model arithmetic and interconnection: `+ - * / \ ^`, `inv`,
//! `feedback`, `series`, `parallel`, `append`.
//!
//! Result class follows MATLAB precedence ss > zpk > tf > pid; a pid stays
//! a pid only when the result is again a PID of the same filter (scalar
//! gain, scalar offset, sum of PIDs with equal `Tf`).

use indy_syntax::ast::BinOp;
use indy_value::Value;

use super::model::{Data, Kind, Ss, Sys, is_lti};
use super::num::{C, Mat, padd, pconv, pscale, ptrim};
use crate::builtins::err;
use crate::{Interp, R};

pub fn not_supported(what: &str) -> Box<indy_value::Error> {
    err(
        "Control:general:notSupported",
        format!("{} is not supported yet in the Rust engine.", what),
    )
}

/// An operand: a model, or a numeric gain.
pub enum Operand {
    Sys(Sys),
    Gain(Mat),
}

pub fn operand(v: &Value, what: &str) -> R<Operand> {
    if is_lti(v) {
        return Ok(Operand::Sys(Sys::from_value(v)?));
    }
    match v {
        Value::Num(_) | Value::Bool(_) => Ok(Operand::Gain(Mat::from_value(v, what)?)),
        _ => Err(err(
            "Control:combination:IncompatibleModels",
            format!(
                "Operations between dynamic systems and {} values are not defined.",
                v.class_name()
            ),
        )),
    }
}

/// Common sample time (`-1` matches any discrete time).
pub fn common_ts(a: f64, b: f64) -> R<f64> {
    if a == b {
        return Ok(a);
    }
    if a == -1.0 && b != 0.0 {
        return Ok(b);
    }
    if b == -1.0 && a != 0.0 {
        return Ok(a);
    }
    Err(err(
        "Control:combination:SampleTime",
        "Sampling times must agree.",
    ))
}

fn gain_sys(kind: Kind, g: &Mat, ts: f64) -> R<Sys> {
    if g.r == 1 && g.c == 1 {
        return Ok(Sys::gain(kind, g.d[0], ts));
    }
    if kind == Kind::Ss {
        let s = Ss {
            a: Mat::zeros(0, 0),
            b: Mat::zeros(0, g.c),
            c: Mat::zeros(g.r, 0),
            d: g.clone(),
        };
        return Ok(Sys::new(Kind::Ss, Data::Ss(s), ts));
    }
    Err(super::model::mimo_tf())
}

/// Bring both operands to one class and sample time.
fn unify(it: &mut Interp, a: Operand, b: Operand) -> R<(Sys, Sys, Kind)> {
    let (ka, ta) = match &a {
        Operand::Sys(s) => (Some(s.kind), Some(s.ts)),
        Operand::Gain(_) => (None, None),
    };
    let (kb, tb) = match &b {
        Operand::Sys(s) => (Some(s.kind), Some(s.ts)),
        Operand::Gain(_) => (None, None),
    };
    let kind = match (ka, kb) {
        (Some(x), Some(y)) => {
            if x.rank() >= y.rank() {
                x
            } else {
                y
            }
        }
        (Some(x), None) | (None, Some(x)) => x,
        _ => Kind::Tf,
    };
    // pid mixed with tf/zpk/ss computes in the other class.
    let kind = if kind.rank() == 0
        && (ka.is_some_and(|k| k.rank() > 0) || kb.is_some_and(|k| k.rank() > 0))
    {
        Kind::Tf
    } else {
        kind
    };
    let ts = match (ta, tb) {
        (Some(x), Some(y)) => common_ts(x, y)?,
        (Some(x), None) | (None, Some(x)) => x,
        _ => 0.0,
    };
    let conv = |it: &mut Interp, o: Operand| -> R<Sys> {
        match o {
            Operand::Sys(s) => {
                let mut s = s.convert(it, kind)?;
                s.ts = ts;
                Ok(s)
            }
            Operand::Gain(g) => gain_sys(kind, &g, ts),
        }
    };
    let sa = conv(it, a)?;
    let sb = conv(it, b)?;
    Ok((sa, sb, kind))
}

fn delays_for_sum(a: &Sys, b: &Sys) -> R<f64> {
    let (da, db) = (a.input_delay(), b.input_delay());
    if (da - db).abs() > 0.0 {
        return Err(not_supported("Adding models with different time delays"));
    }
    Ok(da)
}

fn finish(mut s: Sys, delay: f64) -> Sys {
    s.extra.clear();
    if delay != 0.0 {
        s.set_extra("InputDelay", Value::num(delay));
    }
    s
}

pub fn binary(it: &mut Interp, op: BinOp, a: &Value, b: &Value) -> R<Value> {
    match op {
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::LeftDiv => {}
        BinOp::Pow => return Ok(power(it, a, b)?.to_value()),
        BinOp::EMul | BinOp::EDiv | BinOp::ELeftDiv | BinOp::EPow => {
            return Err(not_supported(&format!(
                "Element-wise operator '{}' on dynamic systems",
                op.text()
            )));
        }
        _ => {
            return Err(err(
                "Control:general:notSupported",
                format!(
                    "Operator '{}' is not defined for dynamic system models.",
                    op.text()
                ),
            ));
        }
    }
    let oa = operand(a, "operand")?;
    let ob = operand(b, "operand")?;
    let r = match op {
        BinOp::Add => add(it, oa, ob, 1.0)?,
        BinOp::Sub => add(it, oa, ob, -1.0)?,
        BinOp::Mul => mul(it, oa, ob)?,
        BinOp::Div => {
            // a / b = a * inv(b)
            let ib = inv_operand(it, ob)?;
            mul(it, oa, ib)?
        }
        _ => {
            let ia = inv_operand(it, oa)?;
            mul(it, ia, ob)?
        }
    };
    Ok(r.to_value())
}

fn inv_operand(it: &mut Interp, o: Operand) -> R<Operand> {
    Ok(match o {
        Operand::Sys(s) => Operand::Sys(inv(it, &s)?),
        Operand::Gain(g) => {
            if g.r == 1 && g.c == 1 {
                Operand::Gain(Mat::scalar(1.0 / g.d[0]))
            } else {
                Operand::Gain(g.inv().ok_or_else(|| {
                    err(
                        "Control:general:singular",
                        "Matrix is singular to working precision.",
                    )
                })?)
            }
        }
    })
}

type PidGains = (f64, f64, f64, f64);

fn pid_like(o: &Operand) -> Option<(Kind, PidGains, f64)> {
    match o {
        Operand::Sys(s) if s.kind.rank() == 0 => Some((s.kind, s.pid_gains()?, s.ts)),
        _ => None,
    }
}

fn scalar_gain(o: &Operand) -> Option<f64> {
    match o {
        Operand::Gain(g) if g.r == 1 && g.c == 1 => Some(g.d[0]),
        _ => None,
    }
}

fn pid_result(it: &mut Interp, kind: Kind, g: (f64, f64, f64, f64), proto: &Sys) -> R<Sys> {
    let (kp, ki, kd, tf) = g;
    let mut s = Sys::new(Kind::Pid, Data::Pid { kp, ki, kd, tf }, proto.ts);
    for f in ["IFormula", "DFormula"] {
        if let Some(v) = proto.get_extra(f) {
            s.set_extra(f, v.clone());
        }
    }
    if kind == Kind::PidStd {
        s = s.convert(it, Kind::PidStd)?;
    }
    Ok(s)
}

fn proto(o: &Operand) -> Option<&Sys> {
    match o {
        Operand::Sys(s) => Some(s),
        _ => None,
    }
}

pub fn add(it: &mut Interp, a: Operand, b: Operand, sign: f64) -> R<Sys> {
    // PID-preserving cases.
    match (pid_like(&a), pid_like(&b), scalar_gain(&a), scalar_gain(&b)) {
        (Some((k, (kp, ki, kd, tf), _)), None, None, Some(x)) => {
            let p = proto(&a).unwrap().clone();
            return pid_result(it, k, (kp + sign * x, ki, kd, tf), &p);
        }
        (None, Some((k, (kp, ki, kd, tf), _)), Some(x), None) => {
            let p = proto(&b).unwrap().clone();
            return pid_result(it, k, (x + sign * kp, sign * ki, sign * kd, tf), &p);
        }
        (Some((k1, g1, t1)), Some((k2, g2, t2)), _, _) if g1.3 == g2.3 && t1 == t2 => {
            let p = proto(&a).unwrap().clone();
            let kind = if k1 == k2 { k1 } else { Kind::Pid };
            return pid_result(
                it,
                kind,
                (
                    g1.0 + sign * g2.0,
                    g1.1 + sign * g2.1,
                    g1.2 + sign * g2.2,
                    g1.3,
                ),
                &p,
            );
        }
        _ => {}
    }
    let (sa, sb, kind) = unify(it, a, b)?;
    let delay = delays_for_sum(&sa, &sb)?;
    let ts = sa.ts;
    let data = match kind {
        Kind::Ss => {
            let (x, y) = (ss_of(&sa), ss_of(&sb));
            if x.ny() != y.ny() || x.nu() != y.nu() {
                return Err(err(
                    "Control:combination:IncompatibleIODims",
                    "In \"sys1+sys2\", the models must have the same number of inputs and outputs.",
                ));
            }
            Data::Ss(Ss {
                a: x.a.blkdiag(&y.a),
                b: x.b.vcat(&y.b),
                c: x.c.hcat(&y.c.scale(sign)),
                d: x.d.lin(1.0, &y.d, sign),
            })
        }
        _ => {
            let (n1, d1) = sa.tf_data(it)?;
            let (n2, d2) = sb.tf_data(it)?;
            let (num, den) = rat_sum(&n1, &d1, &n2, &d2, sign);
            let tf = Sys::tf(&num, &den, ts);
            if kind == Kind::Zpk {
                return Ok(finish(tf.convert(it, Kind::Zpk)?, delay));
            }
            tf.data
        }
    };
    Ok(finish(
        Sys::new(if kind.rank() == 0 { Kind::Tf } else { kind }, data, ts),
        delay,
    ))
}

fn rat_sum(n1: &[f64], d1: &[f64], n2: &[f64], d2: &[f64], sign: f64) -> (Vec<f64>, Vec<f64>) {
    let (t1, t2) = (ptrim(d1), ptrim(d2));
    if t2 == [1.0] {
        return (padd(n1, &pscale(&pconv(n2, &t1), sign)), t1);
    }
    if t1 == [1.0] {
        return (padd(&pconv(n1, &t2), &pscale(n2, sign)), t2);
    }
    (
        padd(&pconv(n1, d2), &pscale(&pconv(n2, d1), sign)),
        pconv(d1, d2),
    )
}

fn ss_of(s: &Sys) -> Ss {
    match &s.data {
        Data::Ss(x) => x.clone(),
        _ => unreachable!(),
    }
}

/// `a * b` (signal flows through b first).
pub fn mul(it: &mut Interp, a: Operand, b: Operand) -> R<Sys> {
    match (pid_like(&a), pid_like(&b), scalar_gain(&a), scalar_gain(&b)) {
        (Some((k, (kp, ki, kd, tf), _)), None, None, Some(x))
        | (None, Some((k, (kp, ki, kd, tf), _)), Some(x), None) => {
            let p = proto(&a).or(proto(&b)).unwrap().clone();
            return pid_result(it, k, (kp * x, ki * x, kd * x, tf), &p);
        }
        _ => {}
    }
    let (sa, sb, kind) = unify(it, a, b)?;
    let delay = sa.input_delay() + sb.input_delay();
    let ts = sa.ts;
    let data = match kind {
        Kind::Ss => {
            let (x, y) = (ss_of(&sa), ss_of(&sb));
            if x.nu() != y.ny() {
                return Err(err(
                    "Control:combination:IncompatibleIODims",
                    "In \"sys1*sys2\", the number of inputs of sys1 must match the number of outputs of sys2.",
                ));
            }
            // x1' = A1 x1 + B1 (C2 x2 + D2 u), x2' = A2 x2 + B2 u
            let top = x.a.hcat(&x.b.mul(&y.c));
            let bot = Mat::zeros(y.nx(), x.nx()).hcat(&y.a);
            Data::Ss(Ss {
                a: top.vcat(&bot),
                b: x.b.mul(&y.d).vcat(&y.b),
                c: x.c.hcat(&x.d.mul(&y.c)),
                d: x.d.mul(&y.d),
            })
        }
        Kind::Zpk => {
            let (z1, p1, k1) = sa.zpk_data(it)?;
            let (z2, p2, k2) = sb.zpk_data(it)?;
            let mut z = z1;
            z.extend(z2);
            let mut p = p1;
            p.extend(p2);
            let k = k1.mul(k2);
            if k == C::default() {
                z.clear();
            }
            Data::Zpk { z, p, k }
        }
        _ => {
            let (n1, d1) = sa.tf_data(it)?;
            let (n2, d2) = sb.tf_data(it)?;
            Sys::tf(&pconv(&n1, &n2), &pconv(&d1, &d2), ts).data
        }
    };
    Ok(finish(
        Sys::new(if kind.rank() == 0 { Kind::Tf } else { kind }, data, ts),
        delay,
    ))
}

pub fn inv(it: &mut Interp, s: &Sys) -> R<Sys> {
    if s.input_delay() != 0.0 {
        return Err(not_supported("Inverting a model with time delays"));
    }
    let data = match &s.data {
        Data::Ss(m) => {
            if m.ny() != m.nu() {
                return Err(err(
                    "Control:transformation:inv1",
                    "Only square models can be inverted.",
                ));
            }
            let di = m.d.inv().ok_or_else(|| {
                not_supported(
                    "Inverting a state-space model with a singular D matrix (descriptor result)",
                )
            })?;
            Data::Ss(Ss {
                a: m.a.sub(&m.b.mul(&di).mul(&m.c)),
                b: m.b.mul(&di),
                c: di.mul(&m.c).scale(-1.0),
                d: di,
            })
        }
        Data::Zpk { z, p, k } => {
            if *k == C::default() {
                return Err(err(
                    "Control:transformation:inv1",
                    "The model is not invertible (zero gain).",
                ));
            }
            Data::Zpk {
                z: p.clone(),
                p: z.clone(),
                k: k.inv(),
            }
        }
        Data::Empty => Data::Empty,
        _ => {
            let (n, d) = s.tf_data(it)?;
            if ptrim(&n) == [0.0] {
                return Err(err(
                    "Control:transformation:inv1",
                    "The model is not invertible (zero gain).",
                ));
            }
            Sys::tf(&d, &n, s.ts).data
        }
    };
    let kind = if s.kind.rank() == 0 { Kind::Tf } else { s.kind };
    Ok(Sys::new(kind, data, s.ts))
}

pub fn power(it: &mut Interp, a: &Value, b: &Value) -> R<Sys> {
    if !is_lti(a) || is_lti(b) {
        return Err(not_supported("This form of '^' on dynamic systems"));
    }
    let n = b.scalar_f64().filter(|x| x.fract() == 0.0).ok_or_else(|| {
        err(
            "Control:combination:mpower",
            "In \"sys^k\", the exponent k must be an integer.",
        )
    })?;
    let s = Sys::from_value(a)?;
    let base = if n < 0.0 { inv(it, &s)? } else { s.clone() };
    let kind = if s.kind.rank() == 0 { Kind::Tf } else { s.kind };
    let mut acc = Sys::gain(kind, 1.0, s.ts);
    if let Data::Ss(m) = &base.data
        && m.ny() != 1
    {
        acc = gain_sys(Kind::Ss, &Mat::eye(m.ny()), s.ts)?;
    }
    for _ in 0..(n.abs() as usize) {
        acc = mul(it, Operand::Sys(acc), Operand::Sys(base.clone()))?;
    }
    Ok(acc)
}

/// `feedback(sys1, sys2, sign)`; `sign = -1` is negative feedback.
pub fn feedback(it: &mut Interp, a: Operand, b: Operand, sign: f64) -> R<Sys> {
    let (sa, sb, kind) = unify(it, a, b)?;
    if sa.input_delay() != 0.0 || sb.input_delay() != 0.0 {
        return Err(not_supported("feedback with time delays"));
    }
    let ts = sa.ts;
    let data = match kind {
        Kind::Ss => {
            let (x, y) = (ss_of(&sa), ss_of(&sb));
            if y.nu() != x.ny() || y.ny() != x.nu() {
                return Err(err(
                    "Control:combination:IncompatibleIODims",
                    "In \"feedback(sys1,sys2)\", the sizes of sys1 and sys2 are incompatible.",
                ));
            }
            let (n1, n2, ny, nu) = (x.nx(), y.nx(), x.ny(), x.nu());
            // y1 = Cy [x1;x2] + Dy r,  u1 = Cu [x1;x2] + Du r.
            let e = Mat::eye(ny)
                .sub(&x.d.mul(&y.d).scale(sign))
                .inv()
                .ok_or_else(|| {
                    err(
                        "Control:combination:feedback1",
                        "The feedback loop is algebraically singular.",
                    )
                })?;
            let cy = e.mul(&x.c.hcat(&x.d.mul(&y.c).scale(sign)));
            let dy = e.mul(&x.d);
            let cu =
                y.d.mul(&cy)
                    .scale(sign)
                    .add(&Mat::zeros(nu, n1).hcat(&y.c.scale(sign)));
            let du = y.d.mul(&dy).scale(sign).add(&Mat::eye(nu));
            let a1 = x.a.hcat(&Mat::zeros(n1, n2)).add(&x.b.mul(&cu));
            let a2 = Mat::zeros(n2, n1).hcat(&y.a).add(&y.b.mul(&cy));
            Data::Ss(Ss {
                a: a1.vcat(&a2),
                b: x.b.mul(&du).vcat(&y.b.mul(&dy)),
                c: cy,
                d: dy,
            })
        }
        _ => {
            let (n1, d1) = sa.tf_data(it)?;
            let (n2, d2) = sb.tf_data(it)?;
            let num = pconv(&n1, &d2);
            let den = padd(&pconv(&d1, &d2), &pscale(&pconv(&n1, &n2), -sign));
            if ptrim(&den) == [0.0] {
                return Err(err(
                    "Control:combination:feedback1",
                    "The feedback loop is algebraically singular.",
                ));
            }
            let tf = Sys::tf(&num, &den, ts);
            if kind == Kind::Zpk {
                return tf.convert(it, Kind::Zpk);
            }
            tf.data
        }
    };
    Ok(Sys::new(
        if kind.rank() == 0 { Kind::Tf } else { kind },
        data,
        ts,
    ))
}

/// Block-diagonal `append(sys1, sys2, ...)` (state space for MIMO).
pub fn append(it: &mut Interp, parts: Vec<Operand>) -> R<Sys> {
    let mut acc: Option<Ss> = None;
    let mut ts: Option<f64> = None;
    for p in parts {
        let (m, t) = match p {
            Operand::Sys(s) => (s.ss_data(it)?, Some(s.ts)),
            Operand::Gain(g) => (
                Ss {
                    a: Mat::zeros(0, 0),
                    b: Mat::zeros(0, g.c),
                    c: Mat::zeros(g.r, 0),
                    d: g,
                },
                None,
            ),
        };
        if let Some(t) = t {
            ts = Some(match ts {
                Some(x) => common_ts(x, t)?,
                None => t,
            });
        }
        acc = Some(match acc {
            None => m,
            Some(x) => Ss {
                a: x.a.blkdiag(&m.a),
                b: x.b.blkdiag(&m.b),
                c: x.c.blkdiag(&m.c),
                d: x.d.blkdiag(&m.d),
            },
        });
    }
    let m = acc.ok_or_else(|| err("Control:general:minrhs", "append needs at least one model."))?;
    Ok(Sys::new(Kind::Ss, Data::Ss(m), ts.unwrap_or(0.0)))
}
