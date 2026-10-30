//! Plotting forms (`step(sys)`, `bode(sys)`, ...): the data is computed
//! here and drawn by calling the engine's graphics builtins (`plot`,
//! `stairs`, `semilogx`, `subplot`, `xlabel`, `ylabel`, `title`, `hold`)
//! through the interpreter, so the figures are ordinary figures.

use indy_value::Value;

use super::analysis::Margins;
use super::model::Sys;
use super::num::C;
use super::resp;
use crate::{Interp, R};

fn call(it: &mut Interp, name: &str, args: Vec<Value>) -> R<()> {
    it.call_named(name, args, 0)?;
    Ok(())
}

fn s(x: &str) -> Value {
    Value::str(x)
}

fn ishold(it: &mut Interp) -> bool {
    it.call_named("ishold", vec![], 1)
        .ok()
        .and_then(|v| v.first().and_then(|x| x.scalar_f64()))
        .is_some_and(|x| x != 0.0)
}

fn labels(it: &mut Interp, title: &str, x: &str, y: &str) -> R<()> {
    call(it, "title", vec![s(title)])?;
    call(it, "xlabel", vec![s(x)])?;
    call(it, "ylabel", vec![s(y)])
}

/// Draw several series with one plotting function, keeping the user's
/// hold state.
fn draw(it: &mut Interp, func: &str, series: Vec<(Vec<f64>, Vec<f64>, Option<String>)>) -> R<()> {
    let held = ishold(it);
    for (k, (x, y, spec)) in series.into_iter().enumerate() {
        let mut args = vec![Value::col(x), Value::col(y)];
        if let Some(sp) = spec {
            args.push(s(&sp));
        }
        call(it, func, args)?;
        if k == 0 && !held {
            call(it, "hold", vec![s("on")])?;
        }
    }
    if !held {
        call(it, "hold", vec![s("off")])?;
    }
    Ok(())
}

pub type Curve = (Vec<f64>, Vec<f64>, Option<String>, bool);

pub fn time_plot(it: &mut Interp, curves: &[Curve], title: &str) -> R<()> {
    let discrete = curves.iter().any(|c| c.3);
    let series = curves
        .iter()
        .map(|(t, y, sp, _)| (t.clone(), y.clone(), sp.clone()))
        .collect();
    draw(it, if discrete { "stairs" } else { "plot" }, series)?;
    labels(it, title, "Time (seconds)", "Amplitude")
}

pub fn lsim_plot(it: &mut Interp, t: &[f64], inputs: &[Vec<f64>], curves: &[Curve]) -> R<()> {
    let mut series: Vec<(Vec<f64>, Vec<f64>, Option<String>)> = Vec::new();
    for u in inputs {
        series.push((t.to_vec(), u.clone(), Some(":".into())));
    }
    let discrete = curves.iter().any(|c| c.3);
    for (tt, y, sp, _) in curves {
        series.push((tt.clone(), y.clone(), sp.clone()));
    }
    draw(it, if discrete { "stairs" } else { "plot" }, series)?;
    labels(
        it,
        "Linear Simulation Results",
        "Time (seconds)",
        "Amplitude",
    )
}

fn freq_for(it: &mut Interp, sys: &Sys, w: Option<&Value>) -> R<Vec<f64>> {
    match w {
        Some(Value::Cell(c)) if c.data.len() == 2 => {
            let lo = c.data[0].scalar_f64().unwrap_or(0.1).log10();
            let hi = c.data[1].scalar_f64().unwrap_or(10.0).log10();
            Ok((0..400)
                .map(|k| 10f64.powf(lo + (hi - lo) * k as f64 / 399.0))
                .collect())
        }
        Some(v) if !v.is_empty() => Ok(v.to_f64_vec().unwrap_or_default()),
        _ => resp::default_freqs(it, sys),
    }
}

pub fn bode_plot(
    it: &mut Interp,
    systems: &[(Sys, Option<String>)],
    w: Option<&Value>,
    phase: bool,
) -> R<()> {
    let mut mags = Vec::new();
    let mut phs = Vec::new();
    for (sys, spec) in systems {
        let wv = freq_for(it, sys, w)?;
        let h = resp::freqresp(it, sys, &wv)?;
        let (ny, nu) = sys.size();
        for ch in 0..ny * nu {
            let series: Vec<C> = h.iter().map(|b| b[ch]).collect();
            let db: Vec<f64> = series.iter().map(|z| 20.0 * z.abs().log10()).collect();
            mags.push((wv.clone(), db, spec.clone()));
            phs.push((wv.clone(), resp::phase_deg(&series), spec.clone()));
        }
    }
    if phase {
        call(
            it,
            "subplot",
            vec![Value::num(2.0), Value::num(1.0), Value::num(1.0)],
        )?;
    }
    draw(it, "semilogx", mags)?;
    call(it, "title", vec![s("Bode Diagram")])?;
    call(it, "ylabel", vec![s("Magnitude (dB)")])?;
    if !phase {
        call(it, "xlabel", vec![s("Frequency (rad/s)")])?;
        return Ok(());
    }
    call(
        it,
        "subplot",
        vec![Value::num(2.0), Value::num(1.0), Value::num(2.0)],
    )?;
    draw(it, "semilogx", phs)?;
    call(it, "xlabel", vec![s("Frequency (rad/s)")])?;
    call(it, "ylabel", vec![s("Phase (deg)")])
}

pub fn margin_plot(it: &mut Interp, sys: &Sys, m: &Margins) -> R<()> {
    bode_plot(it, &[(sys.clone(), None)], None, true)?;
    let db = 20.0 * m.gm.log10();
    let t = format!(
        "Bode Diagram\nGm = {} dB (at {} rad/s), Pm = {} deg (at {} rad/s)",
        indy_value::format::fmt_g(db, 3, false, false),
        indy_value::format::fmt_g(m.wcg, 3, false, false),
        indy_value::format::fmt_g(m.pm, 3, false, false),
        indy_value::format::fmt_g(m.wcp, 3, false, false)
    );
    call(
        it,
        "subplot",
        vec![Value::num(2.0), Value::num(1.0), Value::num(1.0)],
    )?;
    call(it, "title", vec![s(&t)])
}

pub fn nyquist_plot(
    it: &mut Interp,
    systems: &[(Sys, Option<String>)],
    w: Option<&Value>,
) -> R<()> {
    let mut series = Vec::new();
    for (sys, spec) in systems {
        let wv = freq_for(it, sys, w)?;
        let h = resp::freqresp(it, sys, &wv)?;
        let re: Vec<f64> = h.iter().map(|b| b[0].re).collect();
        let im: Vec<f64> = h.iter().map(|b| b[0].im).collect();
        series.push((re.clone(), im.clone(), spec.clone()));
        // Negative frequencies: the mirror image.
        series.push((
            re,
            im.iter().map(|x| -x).collect(),
            Some(spec.clone().unwrap_or_else(|| "--".into())),
        ));
    }
    series.push((vec![-1.0], vec![0.0], Some("r+".into())));
    draw(it, "plot", series)?;
    labels(it, "Nyquist Diagram", "Real Axis", "Imaginary Axis")
}

pub fn pzmap_plot(it: &mut Interp, sys: &Sys, p: &[C], z: &[C]) -> R<()> {
    let _ = sys;
    let mut series = Vec::new();
    if !p.is_empty() {
        series.push((
            p.iter().map(|x| x.re).collect(),
            p.iter().map(|x| x.im).collect(),
            Some("x".into()),
        ));
    }
    if !z.is_empty() {
        series.push((
            z.iter().map(|x| x.re).collect(),
            z.iter().map(|x| x.im).collect(),
            Some("o".into()),
        ));
    }
    if series.is_empty() {
        series.push((vec![], vec![], None));
    }
    draw(it, "plot", series)?;
    labels(
        it,
        "Pole-Zero Map",
        "Real Axis (seconds^-1)",
        "Imaginary Axis (seconds^-1)",
    )
}

pub fn rlocus_plot(it: &mut Interp, sys: &Sys, cols: &[Vec<C>]) -> R<()> {
    let n = cols.first().map(|c| c.len()).unwrap_or(0);
    let mut series = Vec::new();
    for b in 0..n {
        let pts: Vec<C> = cols
            .iter()
            .map(|c| c[b])
            .filter(|z| z.is_finite())
            .collect();
        series.push((
            pts.iter().map(|x| x.re).collect(),
            pts.iter().map(|x| x.im).collect(),
            None,
        ));
    }
    let p = cols.first().cloned().unwrap_or_default();
    if !p.is_empty() {
        series.push((
            p.iter().map(|x| x.re).collect(),
            p.iter().map(|x| x.im).collect(),
            Some("x".into()),
        ));
    }
    let _ = sys;
    draw(it, "plot", series)?;
    labels(
        it,
        "Root Locus",
        "Real Axis (seconds^-1)",
        "Imaginary Axis (seconds^-1)",
    )
}
