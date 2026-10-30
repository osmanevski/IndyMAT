//! End-to-end parity: every MATLAB case of `indy-graphics/tests/oracle/
//! cases.txt` runs as MATLAB code through the interpreter and these
//! builtins, and the figure must equal the Octave measurement
//! (`<case>.json`, IndyMAT's own extractor in Octave 11.3) field by field,
//! with the tolerances of `indy-graphics/tests/oracle.rs`.

use indy_graphics::json::{self, Json};
use indy_interp::Interp;

const ORACLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../indy-graphics/tests/oracle");

fn cases() -> Vec<(String, String)> {
    let text = std::fs::read_to_string(format!("{ORACLE}/cases.txt")).expect("cases.txt");
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

#[test]
fn matlab_cases_match_octave() {
    let mut compared = 0;
    let mut failures = Vec::new();
    for (name, code) in cases() {
        let Ok(want) = std::fs::read_to_string(format!("{ORACLE}/{name}.json")) else {
            continue;
        };
        let want = json::parse(want.trim()).expect("oracle JSON parses");
        compared += 1;
        match run(&code) {
            Err(e) => failures.push(format!("== {name}: error: {e}")),
            Ok(text) => {
                let got = json::parse(&text).expect("emitted JSON parses");
                let mut d = Vec::new();
                diff("", &want, &got, &mut d);
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
    eprintln!("matlab oracle: compared {compared} cases");
    assert_eq!(compared, 58, "every oracle case has a measurement");
    assert!(
        failures.is_empty(),
        "{} of {compared} case(s) differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
