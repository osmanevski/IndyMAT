//! MATLAB-style display of LTI models (`sys` without `;`, `disp`).

use indy_value::format::fmt_g;

use super::model::{Data, Kind, Ss, Sys};
use super::num::{C, ptrim};

fn g4(x: f64) -> String {
    let s = fmt_g(x, 4, false, false);
    if s == "-0" { "0".into() } else { s }
}

/// `s^2 + 2 s + 10` for descending coefficients.
pub fn poly_text(p: &[f64], var: &str) -> String {
    let p = ptrim(p);
    let n = p.len() - 1;
    let ascending = var == "z^-1" || var == "q";
    let mut out = String::new();
    let mut first = true;
    let terms: Vec<(usize, f64)> = if ascending {
        // Coefficients stored as for z: b0 z^n + ... ; show b0 + b1 z^-1 ...
        p.iter().enumerate().map(|(i, a)| (i, *a)).collect()
    } else {
        p.iter().enumerate().map(|(i, a)| (n - i, *a)).collect()
    };
    for (pw, a) in terms {
        if a == 0.0 {
            continue;
        }
        let mono = match (pw, ascending) {
            (0, _) => String::new(),
            (1, false) => var.to_string(),
            (k, false) => format!("{}^{}", var, k),
            (1, true) => var.to_string(),
            (k, true) => format!("{}^-{}", var.trim_end_matches("^-1"), k),
        };
        let mag = a.abs();
        let coef = if mag == 1.0 && !mono.is_empty() {
            String::new()
        } else {
            g4(mag)
        };
        let body = match (coef.is_empty(), mono.is_empty()) {
            (true, _) => mono,
            (false, true) => coef,
            (false, false) => format!("{} {}", coef, mono),
        };
        if first {
            if a < 0.0 {
                out.push('-');
            }
            out.push_str(&body);
            first = false;
        } else {
            out.push_str(if a < 0.0 { " - " } else { " + " });
            out.push_str(&body);
        }
    }
    if first {
        out.push('0');
    }
    out
}

fn center(s: &str, w: usize) -> String {
    let l = s.chars().count();
    let pad = (w.saturating_sub(l)) / 2;
    format!("{}{}", " ".repeat(pad), s)
}

/// Two-line fraction block (or one line when the denominator is 1).
fn fraction(num: &str, den: Option<&str>, prefix: &str) -> Vec<String> {
    let pre = prefix.chars().count();
    match den {
        None => vec![format!("  {}{}", prefix, num)],
        Some(d) => {
            let w = num.chars().count().max(d.chars().count());
            vec![
                format!("  {}{}", " ".repeat(pre), center(num, w)),
                format!("  {}{}", prefix, "-".repeat(w)),
                format!("  {}{}", " ".repeat(pre), center(d, w)),
            ]
        }
    }
}

fn delay_prefix(sys: &Sys, var: &str) -> String {
    let d = sys.input_delay();
    if d == 0.0 {
        return String::new();
    }
    if sys.is_discrete() {
        format!("{}^(-{}) * ", var.trim_end_matches("^-1"), g4(d))
    } else {
        format!("exp(-{}*s) * ", g4(d))
    }
}

fn footer(sys: &Sys, what: &str) -> Vec<String> {
    let mut v = Vec::new();
    if sys.is_discrete() {
        if sys.ts < 0.0 {
            v.push("Sample time: unspecified".to_string());
        } else {
            v.push(format!("Sample time: {} seconds", g4(sys.ts)));
        }
        v.push(format!("Discrete-time {}.", what));
    } else {
        v.push(format!("Continuous-time {}.", what));
    }
    v
}

fn var_of(sys: &Sys) -> String {
    sys.get_extra("Variable")
        .and_then(|v| v.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if sys.is_discrete() {
                "z".into()
            } else {
                "s".into()
            }
        })
}

/// Root factor text for zpk display: `(s+1)`, `s`, `(s^2 + 2s + 10)`.
fn zpk_factors(r: &[C], var: &str) -> String {
    let mut parts: Vec<(String, usize)> = Vec::new();
    let mut used = vec![false; r.len()];
    let push = |t: String, parts: &mut Vec<(String, usize)>| {
        if let Some(p) = parts.iter_mut().find(|(s, _)| *s == t) {
            p.1 += 1;
        } else {
            parts.push((t, 1));
        }
    };
    for i in 0..r.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let z = r[i];
        if z.im == 0.0 {
            let t = if z.re == 0.0 {
                var.to_string()
            } else if z.re < 0.0 {
                format!("({}+{})", var, g4(-z.re))
            } else {
                format!("({}-{})", var, g4(z.re))
            };
            push(t, &mut parts);
        } else {
            if let Some(j) = (i + 1..r.len()).find(|&j| !used[j] && r[j] == z.conj()) {
                used[j] = true;
            }
            let b = -2.0 * z.re;
            let cc = z.re * z.re + z.im * z.im;
            let mid = if b == 0.0 {
                String::new()
            } else if b > 0.0 {
                format!(" + {}{}", g4(b), var)
            } else {
                format!(" - {}{}", g4(-b), var)
            };
            push(format!("({}^2{} + {})", var, mid, g4(cc)), &mut parts);
        }
    }
    parts
        .into_iter()
        .map(|(t, k)| if k > 1 { format!("{}^{}", t, k) } else { t })
        .collect::<Vec<_>>()
        .join(" ")
}

fn matrix_block(
    label: &str,
    m: &[f64],
    r: usize,
    cols: usize,
    rn: &[String],
    cn: &[String],
) -> Vec<String> {
    let mut out = vec![format!("  {} = ", label)];
    if r == 0 || cols == 0 {
        out.push("    []".into());
        return out;
    }
    let rw = rn.iter().map(|s| s.chars().count()).max().unwrap_or(0);
    let mut widths = Vec::new();
    for j in 0..cols {
        let mut w = cn[j].chars().count();
        for i in 0..r {
            w = w.max(g4(m[i + j * r]).chars().count());
        }
        widths.push(w + 2);
    }
    let mut head = format!("   {}", " ".repeat(rw));
    for j in 0..cols {
        head.push_str(&format!("{:>w$}", cn[j], w = widths[j]));
    }
    out.push(head);
    for i in 0..r {
        let mut line = format!("   {:<w$}", rn[i], w = rw);
        for j in 0..cols {
            line.push_str(&format!("{:>w$}", g4(m[i + j * r]), w = widths[j]));
        }
        out.push(line);
    }
    out
}

fn names(sys: &Sys, prop: &str, n: usize, base: &str) -> Vec<String> {
    let given: Vec<String> = match sys.get_extra(prop) {
        Some(indy_value::Value::Cell(c)) => c
            .data
            .iter()
            .map(|v| v.to_str().unwrap_or_default())
            .collect(),
        _ => vec![],
    };
    (0..n)
        .map(|i| match given.get(i) {
            Some(s) if !s.is_empty() => s.clone(),
            _ => {
                if n == 1 && base != "x" {
                    format!("{}1", base)
                } else {
                    format!("{}{}", base, i + 1)
                }
            }
        })
        .collect()
}

fn ss_lines(sys: &Sys, m: &Ss) -> Vec<String> {
    let (nx, ny, nu) = (m.nx(), m.ny(), m.nu());
    let xn = names(sys, "StateName", nx, "x");
    let un = names(sys, "InputName", nu, "u");
    let yn = names(sys, "OutputName", ny, "y");
    let mut out = Vec::new();
    if nx > 0 {
        out.extend(matrix_block("A", &m.a.d, nx, nx, &xn, &xn));
        out.push(" ".into());
        out.extend(matrix_block("B", &m.b.d, nx, nu, &xn, &un));
        out.push(" ".into());
        out.extend(matrix_block("C", &m.c.d, ny, nx, &yn, &xn));
        out.push(" ".into());
    }
    out.extend(matrix_block("D", &m.d.d, ny, nu, &yn, &un));
    out.push(" ".into());
    let d = sys.input_delay();
    if d != 0.0 {
        out.push(format!(
            "Input delays ({}): {}",
            if sys.is_discrete() {
                "sampling periods"
            } else {
                "seconds"
            },
            g4(d)
        ));
        out.push(" ".into());
    }
    if nx == 0 {
        out.push("Static gain.".into());
    } else {
        out.extend(footer(sys, "state-space model"));
    }
    out
}

fn pid_lines(sys: &Sys) -> Vec<String> {
    let (kp, ki, kd, tf) = sys.pid_gains().unwrap();
    let var = if sys.is_discrete() { "z" } else { "s" };
    let mut top = String::from("  ");
    let mut mid = String::from("  ");
    let mut bot = String::from("  ");
    let add = |t: &str, m: &str, b: &str, top: &mut String, mid: &mut String, bot: &mut String| {
        let w = t
            .chars()
            .count()
            .max(m.chars().count())
            .max(b.chars().count());
        top.push_str(&format!("{:<w$}", t, w = w));
        mid.push_str(&format!("{:<w$}", m, w = w));
        bot.push_str(&format!("{:<w$}", b, w = w));
    };
    let std = sys.kind == Kind::PidStd;
    let mut with = Vec::new();
    if std {
        let (p, ti, td, n) = match sys.data {
            Data::PidStd { kp, ti, td, n } => (kp, ti, td, n),
            _ => unreachable!(),
        };
        add("", "Kp * (1 + ", "", &mut top, &mut mid, &mut bot);
        with.push(format!("Kp = {}", g4(p)));
        if ti.is_finite() {
            add(
                &center("1", 4),
                "----",
                &center("Ti", 4),
                &mut top,
                &mut mid,
                &mut bot,
            );
            add("   1 ", " * ---", "   s ", &mut top, &mut mid, &mut bot);
            with.push(format!("Ti = {}", g4(ti)));
        }
        if td != 0.0 {
            if n.is_finite() {
                add(
                    "      Td*s    ",
                    " + ------------",
                    "    (Td/N)*s+1",
                    &mut top,
                    &mut mid,
                    &mut bot,
                );
                with.push(format!("Td = {}", g4(td)));
                with.push(format!("N = {}", g4(n)));
            } else {
                add(
                    "",
                    &format!(" + Td * {}", var),
                    "",
                    &mut top,
                    &mut mid,
                    &mut bot,
                );
                with.push(format!("Td = {}", g4(td)));
            }
        }
        add("", ")", "", &mut top, &mut mid, &mut bot);
    } else {
        let mut first = true;
        if kp != 0.0 || (ki == 0.0 && kd == 0.0) {
            add("", "Kp", "", &mut top, &mut mid, &mut bot);
            with.push(format!("Kp = {}", g4(kp)));
            first = false;
        }
        let plus = |first: bool| if first { "" } else { " + " };
        if ki != 0.0 {
            add(
                "",
                &format!("{}Ki * ", plus(first)),
                "",
                &mut top,
                &mut mid,
                &mut bot,
            );
            let den = if sys.is_discrete() { "z-1" } else { "s" };
            let numt = if sys.is_discrete() { "Ts" } else { "1" };
            let w = den.len().max(numt.len()) + 2;
            add(
                &center(numt, w),
                &"-".repeat(w),
                &center(den, w),
                &mut top,
                &mut mid,
                &mut bot,
            );
            with.push(format!("Ki = {}", g4(ki)));
            first = false;
        }
        if kd != 0.0 {
            if tf == 0.0 {
                add(
                    "",
                    &format!("{}Kd * {}", plus(first), var),
                    "",
                    &mut top,
                    &mut mid,
                    &mut bot,
                );
            } else {
                add(
                    "",
                    &format!("{}Kd * ", plus(first)),
                    "",
                    &mut top,
                    &mut mid,
                    &mut bot,
                );
                let den = format!("Tf*{}+1", var);
                let w = den.len() + 2;
                add(
                    &center(var, w),
                    &"-".repeat(w),
                    &center(&den, w),
                    &mut top,
                    &mut mid,
                    &mut bot,
                );
            }
            with.push(format!("Kd = {}", g4(kd)));
            if tf != 0.0 {
                with.push(format!("Tf = {}", g4(tf)));
            }
        }
    }
    let mut out = Vec::new();
    if top.trim().is_empty() {
        out.push(mid.trim_end().to_string());
    } else {
        out.push(top.trim_end().to_string());
        out.push(mid.trim_end().to_string());
        out.push(bot.trim_end().to_string());
    }
    out.push(" ".into());
    out.push(format!("  with {}", with.join(", ")));
    out.push(" ".into());
    let form = if std { "standard" } else { "parallel" };
    if sys.is_discrete() {
        out.push(if sys.ts < 0.0 {
            "Sample time: unspecified".into()
        } else {
            format!("Sample time: {} seconds", g4(sys.ts))
        });
        out.push(format!("Discrete-time PID controller in {} form.", form));
    } else {
        out.push(format!("Continuous-time PID controller in {} form.", form));
    }
    out
}

/// Body lines (without the `name =` header).
pub fn body(sys: &Sys) -> Vec<String> {
    let var = var_of(sys);
    match &sys.data {
        Data::Empty => vec![format!(
            "  Empty {}: 0 outputs, 0 inputs",
            match sys.kind {
                Kind::Tf => "transfer function",
                Kind::Zpk => "zero/pole/gain model",
                _ => "state-space model",
            }
        )],
        Data::Tf { num, den } => {
            let nt = poly_text(num, &var);
            let dd = ptrim(den);
            let pre = delay_prefix(sys, &var);
            let mut out = if dd.len() == 1 && ptrim(num).len() == 1 {
                vec![format!(
                    "  {}{}",
                    pre,
                    g4(num.last().copied().unwrap_or(0.0) / dd[0])
                )]
            } else {
                fraction(&nt, Some(&poly_text(den, &var)), &pre)
            };
            out.push(" ".into());
            if dd.len() == 1 && ptrim(num).len() == 1 && sys.input_delay() == 0.0 {
                out.push("Static gain.".into());
            } else {
                out.extend(footer(sys, "transfer function"));
            }
            out
        }
        Data::Zpk { z, p, k } => {
            let kt = if k.im != 0.0 {
                format!(
                    "({}{}{}i)",
                    g4(k.re),
                    if k.im < 0.0 { "-" } else { "+" },
                    g4(k.im.abs())
                )
            } else {
                g4(k.re)
            };
            let zf = zpk_factors(z, &var);
            let num = if *k == C::default() {
                "0".to_string()
            } else if zf.is_empty() {
                kt
            } else if *k == C::real(1.0) {
                zf
            } else if *k == C::real(-1.0) {
                format!("-{}", zf)
            } else {
                format!("{} {}", kt, zf)
            };
            let pf = zpk_factors(p, &var);
            let pre = delay_prefix(sys, &var);
            let mut out = if pf.is_empty() {
                fraction(&num, None, &pre)
            } else {
                fraction(&num, Some(&pf), &pre)
            };
            out.push(" ".into());
            if z.is_empty() && p.is_empty() && sys.input_delay() == 0.0 {
                out.push("Static gain.".into());
            } else {
                out.extend(footer(sys, "zero/pole/gain model"));
            }
            out
        }
        Data::Ss(m) => ss_lines(sys, m),
        Data::Pid { .. } | Data::PidStd { .. } => pid_lines(sys),
    }
}

/// Full display of `name = sys`.
pub fn display(name: &str, sys: &Sys) -> String {
    let mut s = format!("{} =\n \n", name);
    for l in body(sys) {
        s.push_str(&l);
        s.push('\n');
    }
    s.push('\n');
    s
}

/// `disp(sys)`.
pub fn disp(sys: &Sys) -> String {
    let mut s = String::from(" \n");
    for l in body(sys) {
        s.push_str(&l);
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polynomial_text() {
        assert_eq!(poly_text(&[1.0, 2.0, 10.0], "s"), "s^2 + 2 s + 10");
        assert_eq!(poly_text(&[0.0, 1.0, 1.0], "s"), "s + 1");
        assert_eq!(poly_text(&[-1.0, 2.0], "s"), "-s + 2");
        assert_eq!(poly_text(&[1.0, -0.904837418], "z"), "z - 0.9048");
        assert_eq!(poly_text(&[0.0, 0.0951625819], "z"), "0.09516");
    }

    #[test]
    fn tf_display_matches_matlab_layout() {
        let s = Sys::tf(&[1.0], &[1.0, 1.0], 0.0);
        assert_eq!(
            display("sys", &s),
            "sys =\n \n    1\n  -----\n  s + 1\n \nContinuous-time transfer function.\n\n"
        );
    }
}
