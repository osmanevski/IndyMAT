//! Parity for the functions added by lane r/grafik-2: each MATLAB case of
//! `tests/oracle_r2/cases.txt` (functions Octave 11.3 has: `hist`) and
//! `tests/oracle_compat/cases.txt` (functions only IndyMAT's own MIT
//! `octave/compat` layer provides in Octave: `sgtitle`) runs through the
//! interpreter and these builtins; the figure must equal the measurement
//! field by field (tolerances as in `oracle_matlab.rs`). Fixtures come from
//! `motor/scripts/grafik_oracle.sh` (header of each cases file).

use indy_graphics::json::{self, Json};
use indy_interp::Interp;

const R2: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/oracle_r2");
const COMPAT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/oracle_compat");

fn cases(dir: &str) -> Vec<(String, String)> {
    let text = std::fs::read_to_string(format!("{dir}/cases.txt")).expect("cases.txt");
    let mut out: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("%% ") {
            out.push((name.trim().to_string(), String::new()));
        } else if let Some((_, code)) = out.last_mut() {
            code.push_str(line);
            code.push('\n');
        }
    }
    out
}

fn close(a: f64, b: f64, path: &str) -> bool {
    let tol = if path.contains("position") {
        1e-8
    } else {
        1e-9
    };
    a == b || (a - b).abs() <= tol * a.abs().max(b.abs()).max(1.0)
}

fn diff(path: &str, want: &Json, got: &Json, out: &mut Vec<String>) {
    match (want, got) {
        (Json::Num(a), Json::Num(b)) => {
            if !close(*a, *b, path) {
                out.push(format!("{path}: octave {a} vs indy {b}"));
            }
        }
        (Json::Arr(a), Json::Arr(b)) => {
            if a.len() != b.len() {
                out.push(format!("{path}: length {} vs {}", a.len(), b.len()));
                return;
            }
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                diff(&format!("{path}.{i}"), x, y, out);
            }
        }
        (Json::Obj(a), Json::Obj(b)) => {
            for (k, v) in a {
                match got.get(k) {
                    Some(w) => diff(&format!("{path}.{k}"), v, w, out),
                    None => out.push(format!("{path}.{k}: missing")),
                }
            }
            for (k, _) in b {
                if want.get(k).is_none() {
                    out.push(format!("{path}.{k}: extra"));
                }
            }
        }
        (a, b) if a == b => {}
        (a, b) => out.push(format!(
            "{path}: octave {} vs indy {}",
            a.to_json_string(),
            b.to_json_string()
        )),
    }
}

/// Runs `code` in a fresh interpreter whose session starts with one figure
/// (as the oracle script does) and returns the current figure's JSON.
fn run(code: &str) -> Result<String, String> {
    let mut it = Interp::new();
    it.out = Box::new(std::io::sink());
    indy_graphics_builtins::register(&mut it);
    it.run_text("figure;").map_err(|e| e.message.clone())?;
    it.run_text(code).map_err(|e| e.message.clone())?;
    let g = indy_graphics_builtins::graphics(&mut it);
    let fig = g.gcf();
    Ok(g.figure_json(fig))
}

/// Accepted differences: (case, path prefixes that may differ, reason).
const KNOWN: &[(&str, &[&str], &str)] = &[(
    "hist_norm",
    &[
        ".axes.0.xlim.0",
        ".axes.0.xtick",
        ".axes.0.camera.",
        ".axes.0.data_aspect_ratio.0",
    ],
    "Octave computes the first bar edge of hist([1:6],3) as 0.9999999999999999 \
     (float noise), which pulls the automatic x limit to 0; the engine gets 1 \
     exactly and keeps [1 6]",
)];

fn compare(dir: &str) -> (usize, usize, Vec<String>) {
    let mut compared = 0;
    let all = cases(dir);
    let mut failures = Vec::new();
    for (name, code) in &all {
        let Ok(want) = std::fs::read_to_string(format!("{dir}/{name}.json")) else {
            failures.push(format!("== {name}: no measurement"));
            continue;
        };
        let want = json::parse(want.trim()).expect("oracle JSON parses");
        compared += 1;
        match run(code) {
            Err(e) => failures.push(format!("== {name}: error: {e}")),
            Ok(text) => {
                let got = json::parse(&text).expect("emitted JSON parses");
                let mut d = Vec::new();
                diff("", &want, &got, &mut d);
                if let Some((_, paths, _)) = KNOWN.iter().find(|(n, ..)| n == name) {
                    assert!(!d.is_empty(), "{name}: known difference no longer occurs");
                    d.retain(|p| !paths.iter().any(|k| p.starts_with(k)));
                }
                if !d.is_empty() {
                    failures.push(format!(
                        "== {name} ({} differences)\n  {}",
                        d.len(),
                        d.iter().take(12).cloned().collect::<Vec<_>>().join("\n  ")
                    ));
                }
            }
        }
    }
    (all.len(), compared, failures)
}

#[test]
fn hist_cases_match_octave() {
    let (n, compared, failures) = compare(R2);
    eprintln!("oracle_r2: compared {compared} of {n} cases");
    assert!(n >= 8 && compared == n);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn sgtitle_cases_match_indymat_compat() {
    let (n, compared, failures) = compare(COMPAT);
    eprintln!("oracle_compat: compared {compared} of {n} cases");
    assert!(n >= 5 && compared == n);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
