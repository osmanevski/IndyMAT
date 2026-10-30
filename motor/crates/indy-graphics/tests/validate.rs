//! Every emitted figure must pass the UI's admission gate
//! (`frontend/figure_data_utils.cjs` `validate()`, run with node), and each
//! case must end in the expected state: drawable, or one explicit,
//! contract-defined fallback reason. Without node the test fails loudly
//! unless `INDY_GRAPHICS_SKIP_NODE=1` is set.

mod cases;

use std::path::PathBuf;
use std::process::Command;

/// Expected outcome per case: "supported" or the producer reason code.
const EXPECT: &[(&str, &str)] = &[
    ("empty_figure", "no_axes"),
    ("x_scatter_per_point", "scatter_colors"),
    ("x_marker_diamond", "unsupported_marker"),
    ("x_surface_budget", "budget_exceeded"),
    ("x_patch_interp", "interpolated_color"),
    ("x_patch_axis_equal", "unsupported_patch"),
    ("x_text_3d", "unsupported_object"),
];

fn expected(name: &str) -> &'static str {
    EXPECT
        .iter()
        .find(|(n, _)| *n == name)
        .map_or("supported", |(_, e)| e)
}

#[test]
fn every_figure_passes_the_ui_gate() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = manifest.join("../../scripts/grafik_validate.cjs");
    let gate = manifest.join("../../../frontend/figure_data_utils.cjs");
    let node_ok = Command::new("node").arg("--version").output().is_ok();
    if !node_ok || !gate.exists() {
        assert!(
            std::env::var("INDY_GRAPHICS_SKIP_NODE").is_ok(),
            "node or frontend/figure_data_utils.cjs unavailable; set INDY_GRAPHICS_SKIP_NODE=1 to skip explicitly"
        );
        eprintln!("SKIPPED: UI admission gate not run (no node or no frontend)");
        return;
    }
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("grafik-validate");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    let mut expectations = String::from("{");
    for (i, (name, build)) in cases::cases()
        .into_iter()
        .chain(cases::extra_cases())
        .enumerate()
    {
        std::fs::write(out.join(format!("{name}.json")), cases::run(build)).unwrap();
        if i > 0 {
            expectations.push(',');
        }
        expectations.push_str(&format!("\"{name}\":\"{}\"", expected(name)));
    }
    expectations.push('}');
    // Random sessions: any outcome, but always admissible (ok:true).
    for seed in 0..150u64 {
        let g = cases::random_session(seed);
        for (k, f) in g.figure_handles().into_iter().enumerate() {
            std::fs::write(out.join(format!("fuzz_{seed}_{k}.json")), g.figure_json(f)).unwrap();
        }
    }
    let expect_file = out.join("expectations.json.txt");
    std::fs::write(&expect_file, expectations).unwrap();
    let result = Command::new("node")
        .arg(&script)
        .arg(&out)
        .arg(&expect_file)
        .output()
        .expect("run node");
    let stdout = String::from_utf8_lossy(&result.stdout);
    eprintln!("{stdout}{}", String::from_utf8_lossy(&result.stderr));
    assert!(
        result.status.success(),
        "admission gate failures:\n{stdout}"
    );
}
