//! Plotting forms (`freqz`/`impz`/`zplane` without outputs): the data is
//! computed here and drawn by calling the engine's own graphics builtins.

use std::f64::consts::PI;

use indy_value::Value;

use super::{C, cabs, rcol};
use crate::{Interp, R};

fn call(it: &mut Interp, name: &str, args: Vec<Value>) -> R<()> {
    it.call_named(name, args, 0)?;
    Ok(())
}

fn s(x: &str) -> Value {
    Value::str(x)
}

pub fn unwrap(p: &[f64]) -> Vec<f64> {
    let mut out = Vec::with_capacity(p.len());
    let mut off = 0.0;
    for (i, &v) in p.iter().enumerate() {
        if i > 0 {
            let d = v - p[i - 1];
            if d > PI {
                off -= 2.0 * PI * ((d + PI) / (2.0 * PI)).floor();
            } else if d < -PI {
                off += 2.0 * PI * ((-d + PI) / (2.0 * PI)).floor();
            }
        }
        out.push(v + off);
    }
    out
}

pub fn freqz_plot(it: &mut Interp, h: &[C], freq: &[f64], hz: bool) -> R<()> {
    let x: Vec<f64> = if hz {
        freq.to_vec()
    } else {
        freq.iter().map(|w| w / PI).collect()
    };
    let mag: Vec<f64> = h
        .iter()
        .map(|c| 20.0 * cabs(*c).max(1e-300).log10())
        .collect();
    let ph: Vec<f64> = unwrap(&h.iter().map(|c| c.1.atan2(c.0)).collect::<Vec<_>>())
        .iter()
        .map(|p| p * 180.0 / PI)
        .collect();
    let xl = if hz {
        "Frequency (Hz)"
    } else {
        "Normalized Frequency (\\times\\pi rad/sample)"
    };
    call(
        it,
        "subplot",
        vec![Value::num(2.0), Value::num(1.0), Value::num(1.0)],
    )?;
    call(it, "plot", vec![rcol(x.clone()), rcol(mag)])?;
    call(it, "grid", vec![s("on")])?;
    call(it, "xlabel", vec![s(xl)])?;
    call(it, "ylabel", vec![s("Magnitude (dB)")])?;
    call(
        it,
        "subplot",
        vec![Value::num(2.0), Value::num(1.0), Value::num(2.0)],
    )?;
    call(it, "plot", vec![rcol(x), rcol(ph)])?;
    call(it, "grid", vec![s("on")])?;
    call(it, "xlabel", vec![s(xl)])?;
    call(it, "ylabel", vec![s("Phase (degrees)")])?;
    Ok(())
}

pub fn impz_plot(it: &mut Interp, h: &[C], t: &[f64], hz: bool) -> R<()> {
    let re: Vec<f64> = h.iter().map(|c| c.0).collect();
    call(it, "stem", vec![rcol(t.to_vec()), rcol(re)])?;
    call(
        it,
        "xlabel",
        vec![s(if hz { "Time (s)" } else { "n (samples)" })],
    )?;
    call(it, "ylabel", vec![s("Amplitude")])?;
    call(it, "title", vec![s("Impulse Response")])?;
    call(it, "grid", vec![s("on")])?;
    Ok(())
}

pub fn zplane_plot(it: &mut Interp, z: &[C], p: &[C]) -> R<()> {
    let th: Vec<f64> = (0..=200).map(|i| 2.0 * PI * i as f64 / 200.0).collect();
    let cx: Vec<f64> = th.iter().map(|t| t.cos()).collect();
    let cy: Vec<f64> = th.iter().map(|t| t.sin()).collect();
    call(it, "plot", vec![rcol(cx), rcol(cy), s("k:")])?;
    call(it, "hold", vec![s("on")])?;
    if !z.is_empty() {
        call(
            it,
            "plot",
            vec![
                rcol(z.iter().map(|c| c.0).collect()),
                rcol(z.iter().map(|c| c.1).collect()),
                s("o"),
            ],
        )?;
    }
    if !p.is_empty() {
        call(
            it,
            "plot",
            vec![
                rcol(p.iter().map(|c| c.0).collect()),
                rcol(p.iter().map(|c| c.1).collect()),
                s("x"),
            ],
        )?;
    }
    call(it, "axis", vec![s("equal")])?;
    call(it, "grid", vec![s("on")])?;
    call(it, "xlabel", vec![s("Real Part")])?;
    call(it, "ylabel", vec![s("Imaginary Part")])?;
    Ok(())
}
