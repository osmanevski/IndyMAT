//! Parity against Octave: every case in `tests/oracle/cases.txt` was run in
//! Octave 11.3 (fltk) through IndyMAT's extractor by
//! `motor/scripts/grafik_oracle.sh`; the same calls through this crate must
//! give the same Figure-Data JSON, field by field.
//!
//! Numbers compare with a 1e-9 relative tolerance (positions 1e-8: the
//! subplot gap constant is fitted). Each remaining, understood difference is
//! listed in `KNOWN` with its reason; anything else fails.

mod cases;

use indy_graphics::json::{self, Json};

/// (case, path prefix, reason). A difference whose path starts with the
/// prefix is accepted and reported, never silently ignored.
const KNOWN: &[(&str, &str, &str)] = &[];

fn oracle(name: &str) -> Option<Json> {
    let path = format!("{}/tests/oracle/{name}.json", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(path).ok()?;
    json::parse(text.trim())
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
                out.push(format!("{path}: octave {a} vs rust {b}"));
            }
        }
        (Json::Arr(a), Json::Arr(b)) => {
            if a.len() != b.len() {
                let show = |v: &[Json]| {
                    let s = Json::Arr(v.to_vec()).to_json_string();
                    if s.len() > 160 {
                        format!("{}…", &s[..160])
                    } else {
                        s
                    }
                };
                out.push(format!(
                    "{path}: length {} vs {} ({} vs {})",
                    a.len(),
                    b.len(),
                    show(a),
                    show(b)
                ));
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
                    None => out.push(format!("{path}.{k}: missing in rust")),
                }
            }
            for (k, _) in b {
                if want.get(k).is_none() {
                    out.push(format!("{path}.{k}: extra in rust"));
                }
            }
        }
        (a, b) if a == b => {}
        (a, b) => out.push(format!(
            "{path}: octave {} vs rust {}",
            a.to_json_string(),
            b.to_json_string()
        )),
    }
}

#[test]
fn oracle_parity() {
    let mut failures = Vec::new();
    let mut report = Vec::new();
    let mut compared = 0;
    for (name, build) in cases::cases() {
        let Some(want) = oracle(name) else {
            report.push(format!(
                "{name}: no Octave measurement (see {name}.error.txt)"
            ));
            continue;
        };
        compared += 1;
        let got = json::parse(&cases::run(build)).expect("emitted JSON must parse");
        let mut diffs = Vec::new();
        diff("", &want, &got, &mut diffs);
        let (known, unknown): (Vec<_>, Vec<_>) = diffs.into_iter().partition(|d| {
            KNOWN
                .iter()
                .any(|(case, prefix, _)| *case == name && d.starts_with(prefix))
        });
        if !known.is_empty() {
            report.push(format!("{name}: {} known difference(s)", known.len()));
        }
        if !unknown.is_empty() {
            failures.push(format!(
                "== {name} ({} differences)\n  {}",
                unknown.len(),
                unknown
                    .iter()
                    .take(25)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n  ")
            ));
        }
    }
    eprintln!("oracle: compared {compared} cases\n{}", report.join("\n"));
    assert!(
        failures.is_empty(),
        "{} case(s) differ from Octave:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
