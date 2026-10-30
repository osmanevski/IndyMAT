//! `errorbar`, `quiver`, `fplot`, `polarplot`, `polar` as MATLAB code sees
//! them, the exported records, and the UI admission gate on every figure.
//!
//! Numbers marked "Octave" were measured with octave-cli 11.3 (fltk,
//! invisible figure) by reading the created objects with `get` (children
//! xdata/ydata, xlim/ylim) — black box, no Octave source.

use std::path::PathBuf;
use std::process::Command;

use indy_graphics::json::{self, Json};
use indy_graphics_builtins::{open_figures, register};
use indy_interp::{Interp, Value};

fn interp() -> Interp {
    let mut it = Interp::new();
    it.out = Box::new(std::io::sink());
    register(&mut it);
    it
}

fn ok(it: &mut Interp, code: &str) {
    if let Err(e) = it.run_text(code) {
        panic!("{code}\n  -> {} ({})", e.message, e.identifier);
    }
}

fn fails(it: &mut Interp, code: &str) -> (String, String) {
    match it.run_text(code) {
        Ok(_) => panic!("expected an error from: {code}"),
        Err(e) => (e.identifier.clone(), e.message.clone()),
    }
}

fn check(it: &mut Interp, code: &str) {
    ok(it, &format!("assert({code});"));
}

fn var(it: &Interp, name: &str) -> Value {
    it.frames[0].vars.get(name).cloned().unwrap_or_default()
}

/// The exported record of figure `n`.
fn figure(it: &mut Interp, n: u64) -> Json {
    let f = open_figures(it)
        .into_iter()
        .find(|f| f.number == n)
        .expect("figure exported");
    json::parse(&f.json).expect("valid JSON")
}

fn series(fig: &Json) -> Vec<Json> {
    fig.at("axes.0.series")
        .and_then(Json::as_array)
        .expect("series")
        .to_vec()
}

fn nums(s: &Json, key: &str) -> Vec<f64> {
    s.get(key).and_then(Json::as_f64_vec).expect(key)
}

fn close(a: &[f64], b: &[f64], tol: f64) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| (x.is_nan() && y.is_nan()) || (x - y).abs() <= tol)
}

const NAN: f64 = f64::NAN;

#[test]
fn errorbar_objects_properties_and_records() {
    let mut it = interp();
    ok(
        &mut it,
        "figure(1); h = errorbar([1 2 3], [2 4 3], [0.5 0.2 0.3]);",
    );
    check(&mut it, "strcmp(get(h, 'Type'), 'errorbar')");
    check(
        &mut it,
        "isequal(get(h, 'YNegativeDelta'), [0.5 0.2 0.3]) && isequal(get(h, 'UData'), [0.5 0.2 0.3])",
    );
    check(
        &mut it,
        "isempty(get(h, 'XNegativeDelta')) && get(h, 'CapSize') == 6 && get(h, 'LineWidth') == 0.5",
    );
    // Octave: colour order first colour, '-' data line, no marker.
    check(
        &mut it,
        "isequal(get(h, 'Color'), [0 0.447 0.741]) && strcmp(get(h, 'LineStyle'), '-') && strcmp(get(h, 'Marker'), 'none')",
    );
    let fig = figure(&mut it, 1);
    let s = series(&fig);
    assert_eq!(s.len(), 2);
    assert_eq!(s[0].get("role").and_then(Json::as_str), Some("errorbar"));
    assert_eq!(s[1].get("role").and_then(Json::as_str), Some("errorbar"));
    assert_eq!(nums(&s[0], "y"), [2.0, 4.0, 3.0]);
    // Octave __errplot__ bars line: per point the bar, top cap, bottom cap.
    let octave_y = [
        1.5, 2.5, NAN, 2.5, 2.5, NAN, 1.5, 1.5, NAN, 3.8, 4.2, NAN, 4.2, 4.2, NAN, 3.8, 3.8, NAN,
        2.7, 3.3, NAN, 3.3, 3.3, NAN, 2.7, 2.7, NAN,
    ];
    assert!(close(&nums(&s[1], "y"), &octave_y, 1e-12));
    // Caps: CapSize 6 pt = 8 px over 560*0.775 px for the x range 2
    // (Octave instead uses 1% of the x data range: ±0.02 here).
    let x = nums(&s[1], "x");
    let half = 4.0 * 2.0 / (560.0 * 0.775);
    assert!(close(&x[3..5], &[1.0 - half, 1.0 + half], 1e-12), "{x:?}");
    // MATLAB caps are point-sized and never move the limits (Octave's
    // data-unit caps widen them to [0.5 3.5]).
    assert_eq!(
        fig.at("axes.0.xlim").and_then(Json::as_f64_vec),
        Some(vec![1.0, 3.0])
    );
    assert_eq!(
        fig.at("axes.0.ylim").and_then(Json::as_f64_vec),
        Some(vec![1.5, 4.5])
    );

    // The group's name labels its first line only.
    ok(&mut it, "legend('meas');");
    let s = series(&figure(&mut it, 1));
    assert_eq!(
        s[0].get("display_name").and_then(Json::as_str),
        Some("meas")
    );
    assert_eq!(s[1].get("display_name").and_then(Json::as_str), Some(""));

    // Asymmetric, both directions, LineSpec and options.
    ok(
        &mut it,
        "figure(2); h = errorbar(1:3, [2 4 3], [.1 .1 .1], [.2 .2 .2], [.3 .3 .3], [.4 .4 .4], 'r--o', 'CapSize', 0, 'LineWidth', 2);",
    );
    check(
        &mut it,
        "isequal(get(h, 'XPositiveDelta'), [.4 .4 .4]) && isequal(get(h, 'LData'), [.1 .1 .1])",
    );
    check(
        &mut it,
        "isequal(get(h, 'Color'), [1 0 0]) && strcmp(get(h, 'LineStyle'), '--') && strcmp(get(h, 'Marker'), 'o') && get(h, 'LineWidth') == 2",
    );
    let s = series(&figure(&mut it, 2));
    // CapSize 0: only the bars, 3 vertical then 3 horizontal.
    assert!(close(
        &nums(&s[1], "x"),
        &[
            1., 1., NAN, 2., 2., NAN, 3., 3., NAN, 0.7, 1.4, NAN, 1.7, 2.4, NAN, 2.7, 3.4, NAN
        ],
        1e-12
    ));
    assert_eq!(s[1].get("line_style").and_then(Json::as_str), Some("-"));
    assert_eq!(s[1].get("line_width").and_then(Json::as_f64), Some(2.0));

    // Matrix input: one object per column; horizontal orientation.
    ok(
        &mut it,
        "figure(3); h = errorbar((1:4)', [1 2; 3 4; 5 6; 7 8], 0.3*ones(4, 2), 'horizontal');",
    );
    check(&mut it, "isequal(size(h), [2 1])");
    check(
        &mut it,
        "isempty(get(h(1), 'YNegativeDelta')) && isequal(get(h(2), 'XNegativeDelta'), 0.3*ones(1, 4))",
    );
    // Octave: the second object takes the second colour.
    check(&mut it, "isequal(get(h(2), 'Color'), [0.85 0.325 0.098])");
    ok(&mut it, "figure(4); h = errorbar([2 4 3], [1 1 1], 'o');");
    check(
        &mut it,
        "isequal(get(h, 'XData'), [1 2 3]) && strcmp(get(h, 'LineStyle'), 'none')",
    );
    ok(&mut it, "set(h, 'CapSize', 12, 'YPositiveDelta', [3 3 3]);");
    check(
        &mut it,
        "get(h, 'CapSize') == 12 && isequal(get(h, 'UData'), [3 3 3])",
    );
    // Errors are explicit.
    let (id, _) = fails(&mut it, "errorbar(1:3, [1 2 3], [1 2]);");
    assert_eq!(id, "MATLAB:narginchk:invalidInput");
    let (id, _) = fails(&mut it, "errorbar(1:3);");
    assert_eq!(id, "MATLAB:hg:InvalidArgument");
    let (id, _) = fails(&mut it, "errorbar(1:3, 1:3, 1:3, 'Bogus', 1);");
    assert_eq!(id, "MATLAB:hg:InvalidProperty");
    check(&mut it, "exist('errorbar') == 5");
}

#[test]
fn quiver_autoscale_and_heads_match_octave() {
    let mut it = interp();
    ok(
        &mut it,
        "figure(1); [X, Y] = meshgrid(0:1, 0:1); q = quiver(X, Y, [1 0; 0.5 1], [0 1; 0.5 -1]);",
    );
    check(&mut it, "strcmp(get(q, 'Type'), 'quiver')");
    check(
        &mut it,
        "strcmp(get(q, 'AutoScale'), 'on') && get(q, 'AutoScaleFactor') == 0.9 && get(q, 'MaxHeadSize') == 0.2",
    );
    check(
        &mut it,
        "isequal(get(q, 'UData'), [1 0.5 0 1]) && isequal(get(q, 'XData'), [0 0 1 1])",
    );
    let fig = figure(&mut it, 1);
    let s = series(&fig);
    assert_eq!(s.len(), 2);
    assert!(
        s.iter()
            .all(|r| r.get("role").and_then(Json::as_str) == Some("quiver"))
    );
    // Octave, same call: shafts and heads.
    let shafts_x = [0., 0.45, NAN, 0., 0.225, NAN, 1., 1., NAN, 1., 1.45, NAN];
    let shafts_y = [0., 0., NAN, 1., 1.225, NAN, 0., 0.45, NAN, 1., 0.55, NAN];
    let heads_x = [
        0.3, 0.45, 0.3, NAN, 0.175, 0.225, 0.125, NAN, 1.05, 1., 0.95, NAN, 1.25, 1.45, 1.35, NAN,
    ];
    let heads_y = [
        -0.05, 0., 0.05, NAN, 1.125, 1.225, 1.175, NAN, 0.3, 0.45, 0.3, NAN, 0.65, 0.55, 0.75, NAN,
    ];
    assert!(close(&nums(&s[0], "x"), &shafts_x, 1e-12));
    assert!(close(&nums(&s[0], "y"), &shafts_y, 1e-12));
    assert!(close(&nums(&s[1], "x"), &heads_x, 1e-12));
    assert!(close(&nums(&s[1], "y"), &heads_y, 1e-12));
    // Octave limits: x [0 2], y [-0.5 1.5].
    assert_eq!(
        fig.at("axes.0.xlim").and_then(Json::as_f64_vec),
        Some(vec![0.0, 2.0])
    );
    assert_eq!(
        fig.at("axes.0.ylim").and_then(Json::as_f64_vec),
        Some(vec![-0.5, 1.5])
    );

    // Vectors use sqrt(N) cells; MaxHeadSize scales the heads (Octave).
    ok(
        &mut it,
        "figure(2); q = quiver([0 1 2], [0 0 0], [1 1 2], [0 0 0], 'MaxHeadSize', 0.1);",
    );
    let s = series(&figure(&mut it, 2));
    let tol = 5e-6;
    assert!(close(
        &nums(&s[0], "x"),
        &[0., 0.519615, NAN, 1., 1.51962, NAN, 2., 3.03923, NAN],
        tol
    ));
    assert!(close(
        &nums(&s[1], "x")[..4],
        &[0.433013, 0.519615, 0.433013, NAN],
        tol
    ));
    assert!(close(
        &nums(&s[1], "y")[..4],
        &[-0.0288675, 0., 0.0288675, NAN],
        tol
    ));
    // Matrix U: rows x cols cells (Octave: 2x3 grid of unit x vectors -> 0.75).
    ok(
        &mut it,
        "figure(3); [X, Y] = meshgrid(0:2, 0:1); quiver(X, Y, ones(2, 3), zeros(2, 3));",
    );
    let s = series(&figure(&mut it, 3));
    assert!(close(&nums(&s[0], "x")[..3], &[0., 0.75, NAN], 1e-12));
    // U, V only: the grid 1:n x 1:m; Octave scale 0.15435.
    ok(&mut it, "figure(4); quiver([1 2; 3 4], [1 0; 0 1]);");
    let s = series(&figure(&mut it, 4));
    assert!(close(
        &nums(&s[0], "x"),
        &[
            1., 1.15435, NAN, 1., 1.46305, NAN, 2., 2.3087, NAN, 2., 2.61739, NAN
        ],
        tol
    ));
    // 'off' and scale 0: no autoscale (Octave: shafts of the raw length).
    ok(
        &mut it,
        "figure(5); q = quiver([0 1], [0 0], [1 1], [1 0], 'off');",
    );
    check(&mut it, "strcmp(get(q, 'AutoScale'), 'off')");
    let s = series(&figure(&mut it, 5));
    assert!(close(&nums(&s[0], "x"), &[0., 1., NAN, 1., 2., NAN], 1e-12));
    assert!(close(
        &nums(&s[1], "x"),
        &[7.0 / 9.0, 1., 5.0 / 9.0, NAN, 5.0 / 3.0, 2., 5.0 / 3.0, NAN],
        1e-12
    ));
    ok(&mut it, "q = quiver([0 1], [0 0], [1 1], [1 0], 2);");
    check(
        &mut it,
        "strcmp(get(q, 'AutoScale'), 'on') && get(q, 'AutoScaleFactor') == 2",
    );
    // LineSpec, marker at the bases, 'filled', ShowArrowHead.
    ok(
        &mut it,
        "figure(6); q = quiver([0 1], [0 0], [1 1], [1 0], 'r--o', 'filled', 'LineWidth', 2);",
    );
    check(
        &mut it,
        "isequal(get(q, 'Color'), [1 0 0]) && strcmp(get(q, 'LineStyle'), '--') && get(q, 'LineWidth') == 2",
    );
    let s = series(&figure(&mut it, 6));
    assert_eq!(s.len(), 3);
    assert_eq!(s[2].get("marker").and_then(Json::as_str), Some("o"));
    assert_eq!(s[2].get("line_style").and_then(Json::as_str), Some("none"));
    assert_eq!(
        s[2].get("marker_face_color").and_then(Json::as_f64_vec),
        Some(vec![1.0, 0.0, 0.0])
    );
    assert_eq!(s[1].get("line_style").and_then(Json::as_str), Some("--"));
    ok(&mut it, "set(q, 'ShowArrowHead', 'off');");
    assert_eq!(series(&figure(&mut it, 6)).len(), 2);
    let (id, _) = fails(&mut it, "quiver([1 2], [1 2 3]);");
    assert_eq!(id, "MATLAB:narginchk:invalidInput");
    let (id, _) = fails(&mut it, "quiver(1, 2, 3, 4, 5, 6);");
    assert_eq!(id, "IndyR:graphicsUnavailable");
    let (id, _) = fails(&mut it, "set(q, 'UData', [1 2 3]);");
    assert_eq!(id, "MATLAB:hg:InvalidPropertyValue");
}

#[test]
fn fplot_samples_adaptively_and_breaks_at_discontinuities() {
    let mut it = interp();
    ok(&mut it, "figure(1); h = fplot(@sin);");
    check(&mut it, "strcmp(get(h, 'Type'), 'functionline')");
    check(
        &mut it,
        "isa(get(h, 'Function'), 'function_handle') && strcmp(func2str(get(h, 'Function')), 'sin')",
    );
    check(
        &mut it,
        "isequal(get(h, 'XRange'), [-5 5]) && get(h, 'MeshDensity') == 23",
    );
    // Octave and MATLAB: the x-limits are the interval, in manual mode.
    check(
        &mut it,
        "isequal(xlim, [-5 5]) && strcmp(get(gca, 'XLimMode'), 'manual') && isequal(ylim, [-1 1])",
    );
    ok(&mut it, "x = get(h, 'XData'); y = get(h, 'YData');");
    check(
        &mut it,
        "x(1) == -5 && x(end) == 5 && numel(x) > 23 && max(abs(y - sin(x))) == 0",
    );
    check(&mut it, "all(diff(get(h, 'XData')) > 0)");
    // Poles of tan in [-5, 5]: one NaN break each.
    ok(&mut it, "h = fplot(@tan);");
    check(&mut it, "sum(isnan(get(h, 'YData'))) == 4");
    // A function that only works on scalars is evaluated element-wise.
    ok(
        &mut it,
        "h = fplot(@(x) x^2, [0 1], 'r--', 'MeshDensity', 5, 'LineWidth', 2);",
    );
    check(
        &mut it,
        "isequal(xlim, [0 1]) && get(h, 'MeshDensity') == 5 && isequal(get(h, 'Color'), [1 0 0])",
    );
    check(
        &mut it,
        "strcmp(get(h, 'LineStyle'), '--') && get(h, 'LineWidth') == 2 && get(h, 'YData')(end) == 1",
    );
    // Complex values are not drawn.
    ok(&mut it, "h = fplot(@sqrt);");
    ok(&mut it, "y = get(h, 'YData'); x = get(h, 'XData');");
    check(&mut it, "all(isnan(y(x < 0))) && all(~isnan(y(x >= 0)))");
    // Parametric.
    ok(&mut it, "figure(2); h = fplot(@cos, @sin, [0 2*pi]);");
    check(
        &mut it,
        "strcmp(get(h, 'Type'), 'parameterizedfunctionline') && isequal(get(h, 'TRange'), [0 2*pi])",
    );
    ok(&mut it, "x = get(h, 'XData'); y = get(h, 'YData');");
    check(&mut it, "max(abs(x.^2 + y.^2 - 1)) < 1e-12");
    check(&mut it, "strcmp(func2str(get(h, 'YFunction')), 'sin')");
    // Under hold on the limits are not forced.
    ok(
        &mut it,
        "figure(3); plot([-10 10], [0 0]); hold on; fplot(@sin, [0 1]); hold off;",
    );
    check(&mut it, "isequal(xlim, [-10 10])");
    let (id, _) = fails(&mut it, "fplot('sin(x)');");
    assert_eq!(id, "IndyR:graphicsUnavailable");
    let (id, _) = fails(&mut it, "fplot(@sin, [1 0]);");
    assert_eq!(id, "MATLAB:hg:InvalidArgument");
    let (id, _) = fails(&mut it, "fplot(@(x) [x x]);");
    assert_eq!(id, "MATLAB:hg:InvalidArgument");
    let (id, _) = fails(&mut it, "h = fplot(@sin); set(h, 'XRange', [0 1]);");
    assert_eq!(id, "IndyR:graphicsUnavailable");
    let (id, _) = fails(&mut it, "fplot(@sin, 'ShowPoles', 'off');");
    assert_eq!(id, "IndyR:graphicsUnavailable");
    let _ = var(&it, "h");
}

#[test]
fn polarplot_is_a_cartesian_approximation() {
    let mut it = interp();
    ok(
        &mut it,
        "figure(1); t = linspace(0, 2*pi, 50); h = polarplot(t, 2*abs(sin(t)) + 0.5, 'r');",
    );
    check(&mut it, "strcmp(get(h, 'Type'), 'line')");
    check(
        &mut it,
        "max(abs(get(h, 'XData') - (2*abs(sin(t)) + 0.5).*cos(t))) < 1e-15",
    );
    // Octave's polar axes is invisible too (and uses theta ticks 0:30:330).
    check(&mut it, "strcmp(get(gca, 'Visible'), 'off')");
    check(&mut it, "strcmp(get(gca, 'DataAspectRatioMode'), 'manual')");
    check(&mut it, "numel(get(gca, 'Children')) == 2");
    ok(&mut it, "legend('rho');");
    let fig = figure(&mut it, 1);
    assert_eq!(
        fig.at("axes.0.visible").and_then(Json::as_bool),
        Some(false)
    );
    let s = series(&fig);
    // The data first (legend entries index the records), then the grid.
    assert_eq!(s[0].get("display_name").and_then(Json::as_str), Some("rho"));
    assert!(s[0].get("role").is_none());
    let roles: Vec<&str> = s[1..]
        .iter()
        .map(|r| r.get("role").and_then(Json::as_str).unwrap_or(""))
        .collect();
    assert_eq!(&roles[..3], ["polar_grid"; 3]);
    let labels: Vec<String> = s
        .iter()
        .filter(|r| r.get("kind").and_then(Json::as_str) == Some("text"))
        .map(|r| {
            r.at("lines.0")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string()
        })
        .collect();
    let mut want: Vec<String> = (0..12).map(|k| format!("{}°", 30 * k)).collect();
    want.extend(["0.5", "1", "1.5", "2", "2.5"].map(String::from));
    assert_eq!(labels, want);
    // Limits: the labels' room around r = 2.5, round in the pixel box.
    let ylim = fig.at("axes.0.ylim").and_then(Json::as_f64_vec).unwrap();
    assert!(close(&ylim, &[-2.875, 2.875], 1e-12));
    // More data under hold: the grid follows.
    ok(
        &mut it,
        "hold on; polarplot(t, 4*ones(size(t)), '--'); hold off;",
    );
    let fig = figure(&mut it, 1);
    let ylim = fig.at("axes.0.ylim").and_then(Json::as_f64_vec).unwrap();
    assert!(close(&ylim, &[-4.6, 4.6], 1e-12));
    // polarplot(rho), several pairs, old polar.
    ok(&mut it, "figure(2); h = polarplot([1 2 3 2 1]);");
    check(&mut it, "abs(get(h, 'XData')(end) - 1) < 1e-15");
    ok(
        &mut it,
        "figure(3); h = polarplot(t, ones(size(t)), t, 2*ones(size(t)), 'LineWidth', 2);",
    );
    check(&mut it, "numel(h) == 2 && get(h(2), 'LineWidth') == 2");
    ok(&mut it, "figure(4); h = polar(t, t, 'g');");
    check(&mut it, "isequal(get(h, 'Color'), [0 1 0])");
    // A new plot without hold clears the polar grid and shows the axes.
    ok(&mut it, "plot(1:3);");
    check(
        &mut it,
        "strcmp(get(gca, 'Visible'), 'on') && numel(get(gca, 'Children')) == 1",
    );
    let (id, _) = fails(&mut it, "hold on; polarplot(t, t);");
    assert_eq!(id, "IndyR:graphicsUnavailable");
    let (id, _) = fails(&mut it, "polar(t, t, 'LineWidth', 2);");
    assert_eq!(id, "MATLAB:hg:InvalidArgument");
}

/// Every figure of these functions passes the UI's admission gate and is
/// drawable (none is an explicit fallback).
#[test]
fn every_lane_a_figure_passes_the_ui_gate() {
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
    let cases: &[(&str, &str)] = &[
        (
            "errorbar_basic",
            "errorbar([1 2 3], [2 4 3], [0.5 0.2 0.3]); legend('a');",
        ),
        (
            "errorbar_both",
            "errorbar(1:3, [2 4 3], [.1 .1 .1], [.2 .2 .2], [.3 .3 .3], [.4 .4 .4], 'r--o');",
        ),
        (
            "errorbar_matrix",
            "errorbar((1:4)', magic(4)(:, 1:2), ones(4, 2), 'both');",
        ),
        (
            "errorbar_log",
            "errorbar(1:5, 10.^(1:5), 2.^(1:5)); set(gca, 'YScale', 'log');",
        ),
        ("errorbar_equal", "errorbar(1:3, 1:3, [1 1 1]); axis equal;"),
        (
            "quiver_grid",
            "[X, Y] = meshgrid(-2:0.5:2); quiver(X, Y, -Y, X);",
        ),
        (
            "quiver_markers",
            "quiver([0 1], [0 0], [1 1], [1 0], 'r--o', 'filled');",
        ),
        (
            "quiver_nan",
            "quiver([0 1 NaN], [0 0 0], [1 NaN 1], [0 0 0]);",
        ),
        ("fplot_sin", "fplot(@sin); title('sin');"),
        ("fplot_tan", "fplot(@tan, [-2 2]);"),
        (
            "fplot_param",
            "fplot(@cos, @sin, [0 2*pi]); legend('circle');",
        ),
        ("fplot_inv", "fplot(@(x) 1./x, [-1 1]);"),
        (
            "polar_basic",
            "t = linspace(0, 2*pi, 50); polarplot(t, 2*abs(sin(t)) + 0.5); legend('r');",
        ),
        (
            "polar_old",
            "t = linspace(0, 2*pi, 200); polar(t, sin(3*t), 'r'); title('rose');",
        ),
        (
            "polar_subplot",
            "subplot(1, 2, 1); polarplot([1 2 3]); subplot(1, 2, 2); plot(1:3);",
        ),
    ];
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("lane-a-validate");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    let mut expect = String::from("{");
    for (i, (name, code)) in cases.iter().enumerate() {
        let mut it = interp();
        ok(&mut it, &format!("figure(1); {code}"));
        let figs = open_figures(&mut it);
        assert_eq!(figs.len(), 1, "{name}");
        std::fs::write(out.join(format!("{name}.json")), &figs[0].json).unwrap();
        if i > 0 {
            expect.push(',');
        }
        expect.push_str(&format!("\"{name}\":\"supported\""));
    }
    expect.push('}');
    let expect_file = out.join("expectations.json.txt");
    std::fs::write(&expect_file, expect).unwrap();
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
