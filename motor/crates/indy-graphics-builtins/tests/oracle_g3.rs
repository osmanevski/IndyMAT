//! Parity for lane r/grafik-3: colour order with LineSpec colours, stem
//! marker colours, subplot alignment around titles/labels, colorbars next
//! to fixed-aspect boxes and the added colormaps. Each case of
//! `tests/oracle_g3/cases.txt` runs through the interpreter and these
//! builtins; the figure must equal the Octave 11.3 measurement field by
//! field (fixtures from `motor/scripts/grafik_oracle.sh`, see the header of
//! the cases file).

use indy_graphics::json::{self, Json};
use indy_interp::Interp;

const G3: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/oracle_g3");

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

/// Cases whose positions come from the fitted text-extent model (see
/// `indy-graphics/src/inset.rs`): Octave's ink boxes carry sub-pixel
/// offsets the model does not reproduce (multi-line labels; a rotated
/// ylabel's line pitch is a little smaller). Allowed: this many pixels of
/// the 420 px figure height on position fields.
const TEXT_MODEL_PX: &[(&str, f64)] = &[
    ("sp_xlabel_5lines", 0.01),
    ("sp_4x1_xlabel_3lines", 0.05),
    ("sp_ylabel_6lines", 0.4),
];

thread_local! {
    static CASE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

fn close(a: f64, b: f64, path: &str) -> bool {
    if path.contains("position")
        && let Some((_, px)) = CASE.with(|c| {
            TEXT_MODEL_PX
                .iter()
                .find(|(n, _)| *n == c.borrow().as_str())
                .copied()
        })
        && (a - b).abs() <= px / 420.0
    {
        return true;
    }
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
    "cb_equal_tight_subplot",
    &[".axes.0.ylim.0"],
    "Octave's `axis equal` takes the pixel box 178.37499984042853 px wide \
     (float noise; the subplot width is exactly 178.375 px), which moves the \
     stretched y limit by 3e-9 relative; the engine uses the exact width",
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
        CASE.with(|c| *c.borrow_mut() = name.clone());
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
fn grafik3_cases_match_octave() {
    let (n, compared, failures) = compare(G3);
    eprintln!("oracle_g3: compared {compared} of {n} cases");
    assert!(n >= 40 && compared == n);
    assert!(
        failures.is_empty(),
        "{} failing:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
