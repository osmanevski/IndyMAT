//! `shading imshow contour3 surfc meshc bar3` as MATLAB code sees them.
//!
//! Numbers marked "measured" come from Octave 11.3 (`octave-cli`, fltk,
//! invisible figure) by reading object properties with `get` after the same
//! call (black box; no Octave source read). `tests/lane_b/<case>.json` is
//! Octave's own Figure-Data for `tests/lane_b/cases.txt`, produced through
//! IndyMAT's extractor with `motor/scripts/grafik_oracle.m`. `bar3` does not
//! exist in Octave 11.3: its checks follow the MATLAB documentation.

use std::path::PathBuf;
use std::process::Command;

use indy_graphics::json::{self, Json};
use indy_graphics_builtins::{UNAVAILABLE, open_figures, register};
use indy_interp::Interp;

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

fn check(it: &mut Interp, code: &str) {
    ok(it, &format!("assert({code});"));
}

fn fails(it: &mut Interp, code: &str) -> (String, String) {
    match it.run_text(code) {
        Ok(_) => panic!("expected an error from: {code}"),
        Err(e) => (e.identifier.clone(), e.message.clone()),
    }
}

fn unavailable(it: &mut Interp, code: &str) {
    let (id, msg) = fails(it, code);
    assert_eq!(id, "IndyR:graphicsUnavailable", "{code}: {msg}");
    assert!(
        msg.contains("not available in the Rust engine yet"),
        "{msg}"
    );
}

/// The figure JSON of figure `n` of a session.
fn figure_json(it: &mut Interp, n: u64) -> Json {
    let f = open_figures(it)
        .into_iter()
        .find(|f| f.number == n)
        .expect("figure is open");
    json::parse(&f.json).expect("valid JSON")
}

#[test]
fn names_are_no_longer_stubs() {
    for name in ["shading", "imshow", "contour3", "surfc", "meshc", "bar3"] {
        assert!(!UNAVAILABLE.contains(&name), "{name}");
    }
    let mut it = interp();
    check(&mut it, "exist('surfc') == 5 && exist('imshow') == 5");
}

#[test]
fn shading_follows_the_measured_rule() {
    let mut it = interp();
    // Measured: flat -> FaceColor flat, EdgeColor none; interp -> interp,
    // none; faceted -> flat, [0 0 0].
    ok(&mut it, "h = surf(peaks(5)); shading flat;");
    check(
        &mut it,
        "strcmp(get(h,'FaceColor'),'flat') && strcmp(get(h,'EdgeColor'),'none')",
    );
    ok(&mut it, "shading interp;");
    check(
        &mut it,
        "strcmp(get(h,'FaceColor'),'interp') && strcmp(get(h,'EdgeColor'),'none')",
    );
    ok(&mut it, "shading faceted;");
    check(
        &mut it,
        "strcmp(get(h,'FaceColor'),'flat') && isequal(get(h,'EdgeColor'),[0 0 0])",
    );
    // Measured: a white (mesh) or 'none' face keeps its face; edges become
    // flat/interp/flat.
    ok(&mut it, "m = mesh(peaks(5)); shading interp;");
    check(
        &mut it,
        "isequal(get(m,'FaceColor'),[1 1 1]) && strcmp(get(m,'EdgeColor'),'interp')",
    );
    ok(&mut it, "shading faceted;");
    check(&mut it, "strcmp(get(m,'EdgeColor'),'flat')");
    ok(
        &mut it,
        "s = surf(peaks(4),'FaceColor','none'); shading flat;",
    );
    check(
        &mut it,
        "strcmp(get(s,'FaceColor'),'none') && strcmp(get(s,'EdgeColor'),'flat')",
    );
    // Measured: patches change too; contour groups do not.
    ok(
        &mut it,
        "clf; p = patch([0;1;1],[0;0;1],[1;2;3]); shading flat;",
    );
    check(
        &mut it,
        "strcmp(get(p,'FaceColor'),'flat') && strcmp(get(p,'EdgeColor'),'none')",
    );
    ok(
        &mut it,
        "clf; contourf(peaks(5)); c = get(gca,'Children'); shading interp;",
    );
    check(&mut it, "isequal(get(c,'LineColor'),[0 0 0])");
    // shading(ax, ...), case-insensitive modes; usage errors.
    ok(
        &mut it,
        "clf; ax = axes; h = surf(peaks(4)); shading(ax, 'INTERP');",
    );
    check(&mut it, "strcmp(get(h,'FaceColor'),'interp')");
    fails(&mut it, "shading foo");
    fails(&mut it, "shading");
    fails(&mut it, "x = shading('flat');");
}

#[test]
fn imshow_sets_up_an_image_axes() {
    let mut it = interp();
    // Measured (Octave imshow): scaled image, invisible axes, DAR [1 1 1]
    // manual, YDir reverse, limits on the pixel edges, CLim = display range
    // (manual), axes colormap gray(256).
    ok(&mut it, "h = imshow(magic(4)/16); ax = gca;");
    check(&mut it, "strcmp(get(h,'Type'),'image')");
    check(&mut it, "strcmp(get(h,'CDataMapping'),'scaled')");
    check(&mut it, "strcmp(get(ax,'Visible'),'off')");
    check(
        &mut it,
        "isequal(get(ax,'DataAspectRatio'),[1 1 1]) && strcmp(get(ax,'DataAspectRatioMode'),'manual')",
    );
    check(&mut it, "strcmp(get(ax,'YDir'),'reverse')");
    check(
        &mut it,
        "isequal(get(ax,'XLim'),[0.5 4.5]) && isequal(get(ax,'YLim'),[0.5 4.5])",
    );
    check(
        &mut it,
        "isequal(get(ax,'CLim'),[0 1]) && strcmp(get(ax,'CLimMode'),'manual')",
    );
    ok(&mut it, "m = get(ax,'Colormap');");
    check(
        &mut it,
        "isequal(size(m),[256 3]) && isequal(m(1,:),[0 0 0]) && isequal(m(end,:),[1 1 1])",
    );
    // Measured CLim per class and display-range form.
    for (code, clim) in [
        ("imshow(uint8(magic(4)*10))", "[0 255]"),
        ("imshow(uint16(magic(4)*1000))", "[0 65535]"),
        ("imshow(int16(magic(4)*100))", "[-32768 32767]"),
        ("imshow(single(magic(4)/16))", "[0 1]"),
        ("imshow(logical(eye(4)))", "[0 1]"),
        ("imshow(magic(4)/16, [0 0.5])", "[0 0.5]"),
        ("imshow(magic(4)/16, [])", "[0.0625 1]"),
        ("imshow(uint8(magic(4)), [])", "[1 16]"),
        ("imshow(magic(4)/16, 'DisplayRange', [0 2])", "[0 2]"),
    ] {
        ok(&mut it, &format!("{code};"));
        check(&mut it, &format!("isequal(get(gca,'CLim'), {clim})"));
    }
    ok(&mut it, "h = imshow(uint8(magic(4)));");
    check(&mut it, "strcmp(class(get(h,'CData')),'uint8')");
    // Measured: RGB and indexed images are direct; the map goes on the axes.
    ok(&mut it, "h = imshow(cat(3,[0 1;1 0],[0 0;1 1],[1 0;0 1]));");
    check(&mut it, "strcmp(get(h,'CDataMapping'),'direct')");
    ok(&mut it, "h = imshow([1 2;3 4], jet(4));");
    check(&mut it, "strcmp(get(h,'CDataMapping'),'direct')");
    check(&mut it, "isequal(get(gca,'Colormap'), jet(4))");
    ok(&mut it, "imshow(magic(4)/16, 'Colormap', hot(8));");
    check(&mut it, "isequal(size(get(gca,'Colormap')),[8 3])");
    // XData/YData (measured: kept as given); 'Parent'; 'fit' is the browser
    // view's own behaviour.
    ok(
        &mut it,
        "h = imshow(magic(4)/16, 'XData', [10 20], 'YData', [1 2], 'InitialMagnification', 'fit');",
    );
    check(
        &mut it,
        "isequal(get(h,'XData'),[10 20]) && isequal(get(h,'YData'),[1 2])",
    );
    ok(
        &mut it,
        "figure(2); a1 = subplot(1,2,1); a2 = subplot(1,2,2); h = imshow(magic(3)/9, 'Parent', a1);",
    );
    check(
        &mut it,
        "get(h,'Parent') == a1 && isempty(get(a2,'Children'))",
    );
    // Measured: under hold the image is added.
    ok(
        &mut it,
        "figure(3); plot(1:3); hold on; imshow(magic(4)/16);",
    );
    check(&mut it, "numel(get(gca,'Children')) == 2");
    // Unsupported forms are explicit.
    unavailable(&mut it, "imshow('peppers.png')");
    unavailable(&mut it, "imshow(magic(4)/16, 'InitialMagnification', 200)");
    unavailable(&mut it, "imshow(magic(4)/16, 'Border', 'tight')");
    unavailable(&mut it, "imshow(magic(4)/16, 'Reduce', true)");
    fails(&mut it, "imshow(int32(magic(4)))");
    fails(&mut it, "imshow(magic(4), [1 1])");
    fails(&mut it, "imshow(rand(2,2,2))");
}

#[test]
fn contour3_draws_levels_in_3d() {
    let mut it = interp();
    let z = "Z = [1 2 3;2 4 2;3 2 1];";
    ok(&mut it, z);
    // Measured: contour3(Z,3) levels [1 1.75 2.5 3.25], zlim [1.5 3.5],
    // clim [1.75 3.25]; view [-37.5 30], grid on.
    ok(&mut it, "contour3(Z, 3); h = get(gca,'Children');");
    check(&mut it, "isequal(get(h,'LevelList'), [1 1.75 2.5 3.25])");
    check(&mut it, "isequal(get(gca,'ZLim'), [1.5 3.5])");
    check(&mut it, "isequal(get(gca,'CLim'), [1.75 3.25])");
    check(&mut it, "isequal(get(gca,'View'), [-37.5 30])");
    check(
        &mut it,
        "strcmp(get(gca,'XGrid'),'on') && strcmp(get(gca,'ZGrid'),'on')",
    );
    // Measured: contour3(Z,[2 3]) zlim [2 3]; contour3(Z) zlim [1 4].
    ok(&mut it, "contour3(Z, [2 3]);");
    check(&mut it, "isequal(get(gca,'ZLim'), [2 3])");
    ok(&mut it, "contour3(Z);");
    check(&mut it, "isequal(get(gca,'ZLim'), [1 4])");
    // LineSpec and name/value pairs.
    ok(
        &mut it,
        "contour3(1:3, [10 20 30], Z, 'r--', 'LineWidth', 2); h = get(gca,'Children');",
    );
    check(&mut it, "isequal(get(h,'LineColor'), [1 0 0])");
    check(
        &mut it,
        "strcmp(get(h,'LineStyle'),'--') && get(h,'LineWidth') == 2",
    );
    check(&mut it, "isequal(get(gca,'YLim'), [10 30])");
    // Each line lies at its level.
    let f = figure_json(&mut it, 1);
    let series = f.at("axes.0.series").and_then(Json::as_array).unwrap();
    assert!(!series.is_empty());
    for s in series {
        let level = s.get("level").and_then(Json::as_f64).unwrap();
        let zs = s.get("z").and_then(Json::as_f64_vec).unwrap();
        assert!(zs.iter().all(|v| *v == level));
        assert_eq!(s.get("role").and_then(Json::as_str), Some("contour"));
    }
    // The contour matrix output and fills are explicit gaps.
    unavailable(&mut it, "[C, h] = contour3(Z);");
    unavailable(&mut it, "h = contour3(Z);");
    unavailable(
        &mut it,
        "contour3(Z); set(get(gca,'Children'), 'Fill', 'on');",
    );
    unavailable(&mut it, "contour3(Z, 'o');");
}

#[test]
fn surfc_and_meshc_put_contours_under_the_surface() {
    let mut it = interp();
    ok(&mut it, "Z = [1 2 3;2 4 2;3 2 1];");
    // Measured: h = [surface; hggroup] (2x1), contours at the lower z limit.
    ok(&mut it, "h = surfc(Z);");
    check(&mut it, "isequal(size(h), [2 1])");
    check(
        &mut it,
        "strcmp(get(h(1),'Type'),'surface') && strcmp(get(h(2),'Type'),'hggroup')",
    );
    check(
        &mut it,
        "isequal(get(gca,'ZLim'), [1 4]) && isequal(get(gca,'CLim'), [1 4])",
    );
    let plane = |it: &mut Interp| -> Vec<f64> {
        let f = figure_json(it, 1);
        let series = f.at("axes.0.series").and_then(Json::as_array).unwrap();
        series
            .iter()
            .filter(|s| s.get("role").and_then(Json::as_str) == Some("contour"))
            .flat_map(|s| s.get("z").and_then(Json::as_f64_vec).unwrap())
            .collect()
    };
    assert!(plane(&mut it).iter().all(|z| *z == 1.0));
    // Measured: Z+10 -> plane 11; a held line down to -3 -> plane -3.
    ok(&mut it, "surfc(1:3, [10 20 30], Z + 10);");
    assert!(plane(&mut it).iter().all(|z| *z == 11.0));
    ok(
        &mut it,
        "cla; hold on; plot3([1 1],[1 1],[-3 3]); surfc(Z); hold off;",
    );
    assert!(plane(&mut it).iter().all(|z| *z == -3.0));
    // Options go to the surface (MATLAB documentation); meshc is a mesh.
    ok(&mut it, "h = surfc(Z, 'EdgeColor', 'r');");
    check(
        &mut it,
        "isequal(get(h(1),'EdgeColor'),[1 0 0]) && strcmp(get(h(2),'LineColor'),'flat')",
    );
    ok(&mut it, "h = meshc(Z);");
    check(
        &mut it,
        "isequal(get(h(1),'FaceColor'),[1 1 1]) && strcmp(get(h(1),'EdgeColor'),'flat')",
    );
    // Measured: the contour levels are those of Z.
    ok(&mut it, "h = surfc(magic(4));");
    check(
        &mut it,
        "abs(max(get(h(2),'LevelList') - [1 2.36363636363636 3.72727272727273 5.09090909090909 6.45454545454545 7.81818181818182 9.18181818181818 10.5454545454545 11.9090909090909 13.2727272727273 14.6363636363636])) < 1e-12",
    );
}

#[test]
fn bar3_builds_one_surface_per_column() {
    let mut it = interp();
    // MATLAB documentation (not measurable in Octave): one surface per column
    // of Z, width 0.8, view 3; bars from 0 to Z(i,j) at x = j, y = i.
    ok(&mut it, "h = bar3(magic(3));");
    check(&mut it, "isequal(size(h), [3 1])");
    check(&mut it, "all(strcmp(get(h,'Type'), 'surface'))");
    check(
        &mut it,
        "isequal(get(gca,'View'), [-37.5 30]) && strcmp(get(gca,'YDir'),'reverse')",
    );
    ok(
        &mut it,
        "x = get(h(2),'XData'); y = get(h(2),'YData'); z = get(h(2),'ZData');",
    );
    check(
        &mut it,
        "abs(min(x(:)) - 1.6) < 1e-12 && abs(max(x(:)) - 2.4) < 1e-12",
    );
    check(
        &mut it,
        "abs(min(y(:)) - 0.6) < 1e-12 && abs(max(y(:)) - 3.4) < 1e-12",
    );
    check(&mut it, "max(z(:)) == 9 && min(z(:)) == 0");
    check(&mut it, "isequal(size(z), [14 4]) && all(isnan(z(5,:)))");
    // A vector is one column; bar3(y, Z, width); colour; stacked.
    ok(&mut it, "h = bar3([10 20 30], [1 2 3], 0.5, 'r');");
    check(
        &mut it,
        "numel(h) == 1 && isequal(get(h,'FaceColor'), [1 0 0])",
    );
    ok(&mut it, "y = get(h,'YData');");
    check(
        &mut it,
        "abs(min(y(:)) - 7.5) < 1e-12 && abs(max(y(:)) - 32.5) < 1e-12",
    );
    ok(
        &mut it,
        "h = bar3([1 2; 3 4], 'stacked'); z = get(h(2),'ZData'); x = get(h(2),'XData');",
    );
    check(
        &mut it,
        "max(z(:)) == 7 && min(z(:)) == 1 && abs(max(x(:)) - 1.4) < 1e-12",
    );
    ok(&mut it, "h = bar3(magic(3), 'EdgeColor', 'b');");
    check(&mut it, "isequal(get(h(1),'EdgeColor'), [0 0 1])");
    unavailable(&mut it, "bar3(magic(3), 'grouped')");
    fails(&mut it, "bar3([1 2 3], [1 2])");
}

// ----- Octave parity --------------------------------------------------------

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/lane_b")
}

fn cases() -> Vec<(String, String)> {
    let text = std::fs::read_to_string(dir().join("cases.txt")).unwrap();
    let mut out: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("%%") {
            out.push((name.trim().to_string(), String::new()));
        } else if let Some(last) = out.last_mut() {
            last.1.push_str(line);
            last.1.push('\n');
        }
    }
    out
}

/// Fields that differ from Octave on purpose, per case prefix.
fn ignored(case: &str, path: &str) -> bool {
    // Octave stretches the z limits of an `axis image` axes with its plot
    // box (zlim [-2 2] for a 4x4 image; z ticks/camera follow); the engine
    // keeps the 2-D default [-1 1]. z is not drawn in a 2-D image axes.
    case.starts_with("im")
        && [
            "zlim",
            "ztick",
            "zticklabel",
            "camera",
            "plot_box_aspect_ratio",
        ]
        .iter()
        .any(|k| path.starts_with(&format!("axes.0.{k}")))
}

fn diff(case: &str, path: &str, want: &Json, got: &Json, out: &mut Vec<String>) {
    if ignored(case, path) {
        return;
    }
    match (want, got) {
        (Json::Num(a), Json::Num(b)) => {
            if !(a == b || (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)) {
                out.push(format!("{path}: octave {a} vs engine {b}"));
            }
        }
        (Json::Arr(a), Json::Arr(b)) => {
            if a.len() != b.len() {
                out.push(format!("{path}: length {} vs {}", a.len(), b.len()));
                return;
            }
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                diff(case, &format!("{path}.{i}"), x, y, out);
            }
        }
        (Json::Obj(a), Json::Obj(b)) => {
            for (k, v) in a {
                let p = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                match got.get(k) {
                    Some(w) => diff(case, &p, v, w, out),
                    None => out.push(format!("{p}: missing in engine")),
                }
            }
            for (k, _) in b {
                if want.get(k).is_none() {
                    out.push(format!("{path}.{k}: only in engine"));
                }
            }
        }
        (a, b) if a == b => {}
        (a, b) => out.push(format!(
            "{path}: octave {} vs engine {}",
            a.to_json_string(),
            b.to_json_string()
        )),
    }
}

#[test]
fn figures_match_octave() {
    let mut failures = Vec::new();
    let all = cases();
    assert_eq!(all.len(), 18);
    for (name, code) in &all {
        let want = std::fs::read_to_string(dir().join(format!("{name}.json"))).unwrap();
        let want = json::parse(&want).unwrap();
        let mut it = interp();
        ok(&mut it, code);
        let got = figure_json(&mut it, 1);
        let mut out = Vec::new();
        diff(name, "", &want, &got, &mut out);
        for d in out.into_iter().take(8) {
            failures.push(format!("{name}: {d}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// ----- UI admission gate ----------------------------------------------------

/// Every figure these commands produce is admissible, drawable where the
/// contract allows it, and an explicit fallback otherwise.
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
    let figures: &[(&str, &str, &str)] = &[
        (
            "contour3_peaks",
            "contour3(peaks(20), 8, 'r--');",
            "supported",
        ),
        (
            "contour3_xy",
            "[X,Y] = meshgrid(-2:0.25:2); contour3(X, Y, X.*exp(-X.^2-Y.^2), 15);",
            "supported",
        ),
        ("surfc_peaks", "surfc(peaks(25));", "supported"),
        ("meshc_peaks", "meshc(peaks(15)); colorbar;", "supported"),
        (
            "surfc_interp",
            "surfc(peaks(20)); shading interp;",
            "supported",
        ),
        ("surf_flat", "surf(peaks(12)); shading flat;", "supported"),
        (
            "mesh_interp",
            "mesh(peaks(10)); shading interp;",
            "supported",
        ),
        ("bar3_magic", "bar3(magic(4));", "supported"),
        (
            "bar3_stacked",
            "bar3([1 2 3], [3 1 2; 2 2 2; 1 3 1], 0.5, 'stacked');",
            "supported",
        ),
        (
            "bar3_nan",
            "bar3([1 NaN 3; 2 2 2]); shading interp;",
            "supported",
        ),
        (
            "imshow_gray",
            "imshow(magic(8)/64); title('gray');",
            "supported",
        ),
        ("imshow_uint8", "imshow(uint8(magic(6)*7));", "supported"),
        (
            "imshow_uint16",
            "imshow(uint16(magic(6)*700));",
            "supported",
        ),
        ("imshow_bw", "imshow(logical(eye(5)));", "supported"),
        (
            "imshow_rgb",
            "imshow(cat(3, eye(3), ones(3)/2, zeros(3)));",
            "supported",
        ),
        (
            "imshow_rgb8",
            "imshow(uint8(cat(3, eye(3)*255, ones(3)*9, zeros(3))));",
            "supported",
        ),
        (
            "imshow_indexed",
            "imshow([1 2 3; 3 2 1], jet(3));",
            "supported",
        ),
        (
            "imshow_xdata",
            "imshow(reshape(1:32,4,8)/32, 'XData', [10 20]);",
            "supported",
        ),
        (
            "imshow_subplot",
            "subplot(1,2,1); imshow(magic(4)/16); subplot(1,2,2); plot(1:3);",
            "supported",
        ),
        // The contract's image record has no int16 class: explicit fallback.
        ("imshow_int16", "imshow(int16(magic(4)));", "invalid_data"),
        // A patch under `shading interp` is the contract's interpolated-colour fallback.
        (
            "patch_interp",
            "patch([0;1;1],[0;0;1],[1;2;3]); shading interp;",
            "interpolated_color",
        ),
    ];
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("lane-b-validate");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    let mut expect = Vec::new();
    for (name, code, want) in figures {
        let mut it = interp();
        ok(&mut it, code);
        let figs = open_figures(&mut it);
        assert_eq!(figs.len(), 1, "{name}");
        std::fs::write(out.join(format!("{name}.json")), &figs[0].json).unwrap();
        expect.push(format!("\"{name}\":\"{want}\""));
    }
    let expect_file = out.join("expectations.json.txt");
    std::fs::write(&expect_file, format!("{{{}}}", expect.join(","))).unwrap();
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
