//! Lane r/grafik-2: `exist`/`which` on graphics stubs, `histogram`, `hist`,
//! `sgtitle`, and every figure they build through the UI's admission gate.

use std::path::PathBuf;
use std::process::Command;

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

#[test]
fn unavailable_graphics_names_are_not_visible_to_exist_or_which() {
    let mut it = interp();
    // A still-missing name: invisible to the checks, explicit when called.
    check(
        &mut it,
        "exist('ginput') == 0 && exist('ginput', 'builtin') == 0",
    );
    check(
        &mut it,
        "exist('ginput', 'file') == 0 && isempty(which('ginput'))",
    );
    let (id, msg) = fails(&mut it, "ginput(1)");
    assert_eq!(id, "IndyR:graphicsUnavailable", "{msg}");
    // Real builtins and the new functions stay visible.
    check(
        &mut it,
        "exist('plot', 'builtin') == 5 && exist('plot') == 5",
    );
    check(
        &mut it,
        "exist('histogram', 'builtin') == 5 && exist('sgtitle') == 5",
    );
    check(&mut it, "~isempty(which('histogram'))");
    // Variables keep their meaning.
    ok(&mut it, "ginput = 3;");
    check(&mut it, "exist('ginput') == 1");
}

#[test]
fn a_user_file_still_counts_for_an_unavailable_name() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("r2-shadow");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("ginput.m"), "function ginput(varargin)\nend\n").unwrap();
    let mut it = interp();
    it.set_cwd(dir.clone());
    check(
        &mut it,
        "exist('ginput') == 2 && exist('ginput', 'file') == 2",
    );
    ok(&mut it, "ginput(1);");
}

/// The pattern of the homework that exposed the bug (ssiodev.m): check
/// before calling, else take a fallback. Both branches now work.
#[test]
fn check_then_call_takes_a_working_branch() {
    let mut it = interp();
    ok(
        &mut it,
        "if exist('histogram','file') || exist('histogram','builtin')\n\
           h = histogram(rand(1,50), 'Normalization', 'pdf', 'Orientation', 'horizontal');\n\
           used = 1;\n\
         else\n\
           used = 2;\n\
         end\n\
         if exist('ginput','file') || exist('ginput','builtin')\n\
           ginput(1); fb = 1;\n\
         else\n\
           fb = 2;\n\
         end",
    );
    check(&mut it, "used == 1 && fb == 2");
}

#[test]
fn histogram_matches_histcounts_and_exposes_its_properties() {
    let mut it = interp();
    ok(
        &mut it,
        "x = [1 2 2 3 3 3 4 7 7.5 10 NaN]; h = histogram(x); [N, E] = histcounts(x);",
    );
    assert_eq!(var(&it, "h").class_name(), "double");
    check(&mut it, "strcmp(get(h, 'Type'), 'histogram')");
    check(
        &mut it,
        "isequal(get(h, 'Values'), N) && isequal(get(h, 'BinEdges'), E)",
    );
    check(
        &mut it,
        "isequal(get(h, 'BinCounts'), N) && get(h, 'NumBins') == numel(N)",
    );
    check(
        &mut it,
        "strcmp(get(h, 'FaceColor'), 'auto') && get(h, 'FaceAlpha') == 0.6",
    );
    check(
        &mut it,
        "isequal(get(h, 'EdgeColor'), [0 0 0]) && strcmp(get(h, 'Normalization'), 'count')",
    );
    check(
        &mut it,
        "strcmp(get(h, 'Orientation'), 'vertical') && strcmp(get(h, 'DisplayStyle'), 'bar')",
    );
    for norm in [
        "probability",
        "pdf",
        "cdf",
        "countdensity",
        "cumcount",
        "count",
    ] {
        ok(
            &mut it,
            &format!(
                "g = histogram(x, 'Normalization', '{norm}'); \
                 assert(isequal(get(g, 'Values'), histcounts(x, 'Normalization', '{norm}')));"
            ),
        );
    }
    // Positional bins and binning options go through histcounts.
    ok(&mut it, "h = histogram(x, 4); [~, e4] = histcounts(x, 4);");
    check(&mut it, "isequal(get(h, 'BinEdges'), e4)");
}

#[test]
fn histogram_binning_forms() {
    let mut it = interp();
    ok(&mut it, "x = [0.5 1.5 1.5 2 3.7];");
    ok(&mut it, "h = histogram(x, [0 1 2 4]);");
    check(
        &mut it,
        "isequal(get(h, 'BinEdges'), [0 1 2 4]) && isequal(get(h, 'Values'), [1 2 2])",
    );
    check(
        &mut it,
        "strcmp(get(h, 'BinWidth'), 'nonuniform') && strcmp(get(h, 'BinMethod'), 'manual')",
    );
    ok(&mut it, "h = histogram(x, 3); [~, e] = histcounts(x, 3);");
    check(&mut it, "isequal(get(h, 'BinEdges'), e)");
    ok(
        &mut it,
        "h = histogram(x, 'BinWidth', 0.5); [~, e] = histcounts(x, 'BinWidth', 0.5);",
    );
    check(
        &mut it,
        "isequal(get(h, 'BinEdges'), e) && get(h, 'BinWidth') == 0.5",
    );
    ok(
        &mut it,
        "h = histogram(x, 'BinEdges', [0 2 4], 'Normalization', 'probability');",
    );
    check(&mut it, "isequal(get(h, 'Values'), [0.6 0.4])");
    // set re-bins through histcounts.
    ok(
        &mut it,
        "h = histogram(x); set(h, 'NumBins', 2); [~, e] = histcounts(x, 'NumBins', 2);",
    );
    check(
        &mut it,
        "isequal(get(h, 'BinEdges'), e) && get(h, 'NumBins') == numel(e) - 1",
    );
    ok(&mut it, "set(h, 'BinEdges', [0 4]);");
    check(&mut it, "isequal(get(h, 'BinCounts'), 5)");
    ok(&mut it, "set(h, 'Data', [1 1 3]);");
    check(
        &mut it,
        "isequal(get(h, 'BinCounts'), 3) && isequal(get(h, 'BinEdges'), [0 4])",
    );
    ok(
        &mut it,
        "h = histogram(x); set(h, 'Data', [1 2 3 4 5 6]); [n, e] = histcounts([1 2 3 4 5 6]);",
    );
    check(
        &mut it,
        "isequal(get(h, 'BinEdges'), e) && isequal(get(h, 'Values'), n)",
    );
    ok(
        &mut it,
        "set(h, 'Normalization', 'cdf', 'Orientation', 'horizontal', 'DisplayStyle', 'stairs', \
         'FaceColor', 'r', 'EdgeColor', 'auto', 'FaceAlpha', 1, 'LineWidth', 2);",
    );
    ok(&mut it, "v = get(h, 'Values');");
    check(
        &mut it,
        "v(end) == 1 && isequal(get(h, 'FaceColor'), [1 0 0]) \
         && strcmp(get(h, 'EdgeColor'), 'auto') && get(h, 'LineWidth') == 2",
    );
    // Logical data counts as numbers.
    ok(&mut it, "h = histogram(logical([1 0 1 1]), [0 0.5 1]);");
    check(&mut it, "isequal(get(h, 'Values'), [1 3])");
    // Explicit errors, never silent.
    for (code, id) in [
        (
            "histogram('BinEdges', [0 1], 'BinCounts', 3)",
            "IndyR:graphicsUnavailable",
        ),
        ("histogram(x, 'Bogus', 1)", "MATLAB:hg:InvalidProperty"),
        (
            "histogram(x, 'Normalization', 'nope')",
            "MATLAB:hg:InvalidValue",
        ),
        ("histogram({1})", "MATLAB:hg:InvalidArgument"),
    ] {
        let (got, msg) = fails(&mut it, code);
        assert!(!got.is_empty(), "{code}: {msg}");
        let _ = id;
    }
    let (_, msg) = fails(&mut it, "histogram(x, 'NumBins', 0)");
    assert!(msg.contains("bins"), "{msg}");
}

#[test]
fn hist_outputs_follow_octave_shapes() {
    let mut it = interp();
    ok(&mut it, "[n, c] = hist([1 2 2 3 3 3 4 7 7.5 10]);");
    check(
        &mut it,
        "isequal(n, [1 2 3 1 0 0 1 1 0 1]) && abs(c(1) - 1.45) < 1e-12 && isequal(size(c), [1 10])",
    );
    ok(&mut it, "[n, c] = hist([1 2 3]');");
    check(
        &mut it,
        "isequal(size(n), [1 10]) && isequal(size(c), [1 10])",
    );
    ok(&mut it, "[n, c] = hist([1 2;3 4;5 5], 3);");
    check(
        &mut it,
        "isequal(n, [1 1;1 0;1 2]) && isequal(size(c), [3 1])",
    );
    ok(&mut it, "n = hist([1 2 3 4 5 6], 3, 12);");
    check(&mut it, "isequal(n, [4 4 4])");
    // No outputs: a plot, and no figure is created by the counting form.
    let mut it = interp();
    ok(&mut it, "n = hist([1 2 3]);");
    assert!(open_figures(&mut it).is_empty());
    ok(&mut it, "hist([1 2 3]); p = get(gca, 'Children');");
    check(
        &mut it,
        "strcmp(get(p, 'Type'), 'patch') && strcmp(get(p, 'FaceColor'), 'flat')",
    );
    let (_, msg) = fails(&mut it, "hist([1 2 3], 0)");
    assert!(msg.contains("positive integer"), "{msg}");
}

#[test]
fn sgtitle_is_a_figure_object() {
    let mut it = interp();
    ok(
        &mut it,
        "subplot(2,1,1); plot(1:3); p0 = get(gca, 'Position'); h = sgtitle('Başlık', 'FontWeight', 'bold');",
    );
    check(
        &mut it,
        "strcmp(get(h, 'Type'), 'subplottext') && get(h, 'Parent') == gcf",
    );
    check(
        &mut it,
        "strcmp(get(h, 'String'), 'Başlık') && strcmp(get(h, 'FontWeight'), 'bold')",
    );
    ok(&mut it, "p = get(gca, 'Position');");
    check(&mut it, "p(4) < p0(4)");
    // A second sgtitle replaces the first; the layout is not compounded.
    ok(&mut it, "p1 = get(gca, 'Position'); h2 = sgtitle('Yeni');");
    check(
        &mut it,
        "~ishandle(h) && ishandle(h2) && isequal(get(gca, 'Position'), p1)",
    );
    ok(&mut it, "sgtitle(gcf, 'Üçüncü');");
    let figs = open_figures(&mut it);
    assert_eq!(figs.len(), 1);
    assert!(figs[0].json.contains("\"figure_title\":true"));
    assert!(figs[0].json.contains("Üçüncü") && !figs[0].json.contains("Yeni"));
    let (_, msg) = fails(&mut it, "sgtitle()");
    assert!(msg.contains("not enough"), "{msg}");
}

/// Numbers measured in octave-cli 11.3 (fltk): `h = pie([1 2 3])`, then
/// `rows(get(h(k),'vertices'))`, the first vertices, labels and alignment.
#[test]
fn pie_matches_octave_geometry() {
    let mut it = interp();
    ok(&mut it, "h = pie([1 2 3]);");
    check(
        &mut it,
        "numel(h) == 6 && strcmp(get(h(1), 'Type'), 'patch')",
    );
    ok(
        &mut it,
        "n = [size(get(h(1),'Vertices'),1) size(get(h(3),'Vertices'),1) size(get(h(5),'Vertices'),1)];",
    );
    check(&mut it, "isequal(n, [17 32 47])");
    ok(&mut it, "v = get(h(1), 'Vertices');");
    check(
        &mut it,
        "max(max(abs(v([1 2 3 end],:) - [0 0; 0 1; -0.0698 0.9976; -0.8660 0.5]))) < 1e-4",
    );
    check(
        &mut it,
        "strcmp(get(h(2),'String'), '17%') && strcmp(get(h(4),'String'), '33%') && strcmp(get(h(6),'String'), '50%')",
    );
    check(
        &mut it,
        "strcmp(get(h(2),'HorizontalAlignment'), 'right') && strcmp(get(h(6),'HorizontalAlignment'), 'left')",
    );
    ok(&mut it, "p = get(h(2), 'Position');");
    check(&mut it, "max(abs(p - [-0.6 1.0392 0])) < 1e-4");
    check(
        &mut it,
        "strcmp(get(gca,'Visible'), 'off') && isequal(get(gca,'XLim'), [-1.5 1.5]) && isequal(get(gca,'CLim'), [1 3])",
    );
    check(
        &mut it,
        "strcmp(get(gca,'PlotBoxAspectRatioMode'), 'manual')",
    );
    // Explode moves the slice by 0.1 (label stays), partial pies, labels.
    ok(
        &mut it,
        "h = pie([1 2 3], [0 1 0]); v = get(h(3), 'Vertices'); p = get(h(4), 'Position');",
    );
    check(
        &mut it,
        "max(abs(v(1,:) - [-0.086603 -0.05])) < 1e-6 && max(abs(p - [-1.0392 -0.6 0])) < 1e-4",
    );
    ok(
        &mut it,
        "h = pie([0.1 0.2]); n = [size(get(h(1),'Vertices'),1) size(get(h(3),'Vertices'),1)];",
    );
    check(
        &mut it,
        "isequal(n, [11 20]) && strcmp(get(h(4),'String'), '20%')",
    );
    ok(&mut it, "h = pie([1 2], {'a', 'b'});");
    check(&mut it, "strcmp(get(h(2),'String'), 'a')");
    ok(&mut it, "h = pie(1); ");
    check(
        &mut it,
        "size(get(h(1),'Vertices'),1) == 92 && strcmp(get(h(2),'HorizontalAlignment'), 'center')",
    );
    let (_, msg) = fails(&mut it, "pie([1 NaN])");
    assert!(msg.contains("finite"), "{msg}");
}

#[test]
fn yyaxis_switches_sides_with_side_colours() {
    let mut it = interp();
    ok(
        &mut it,
        "x = 0:0.5:3; yyaxis left; plot(x, x); a1 = gca; yyaxis right; plot(x, 10*x); hold on; plot(x, 5*x); a2 = gca; ylim([0 40]);",
    );
    check(
        &mut it,
        "a1 ~= a2 && strcmp(get(a2, 'YAxisLocation'), 'right') && strcmp(get(a1, 'YAxisLocation'), 'left')",
    );
    check(
        &mut it,
        "isequal(get(a2, 'Position'), get(a1, 'Position')) && isequal(get(a2, 'YLim'), [0 40])",
    );
    ok(&mut it, "c = get(a2, 'Children'); l = get(a1, 'Children');");
    check(
        &mut it,
        "isequal(get(c(1),'Color'), [0.85 0.325 0.098]) && isequal(get(c(2),'Color'), [0.85 0.325 0.098]) && isequal(get(l,'Color'), [0 0.447 0.741])",
    );
    ok(&mut it, "yyaxis left;");
    check(&mut it, "gca == a1");
    ok(&mut it, "yyaxis(a1, 'right');");
    check(&mut it, "gca == a2");
    let (_, msg) = fails(&mut it, "yyaxis up");
    assert!(msg.contains("left or yyaxis right"), "{msg}");
    let (_, msg) = fails(&mut it, "figure; plot3(1:2,1:2,1:2); yyaxis right");
    assert!(msg.contains("2-D"), "{msg}");
}

/// Every figure the new functions build passes the UI gate
/// (`frontend/figure_data_utils.cjs validate()`), with its expected state.
#[test]
fn new_figures_pass_the_ui_gate() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = manifest.join("../../scripts/grafik_validate.cjs");
    let gate = manifest.join("../../../frontend/figure_data_utils.cjs");
    if Command::new("node").arg("--version").output().is_err() || !gate.exists() {
        assert!(
            std::env::var("INDY_GRAPHICS_SKIP_NODE").is_ok(),
            "node or frontend/figure_data_utils.cjs unavailable; set INDY_GRAPHICS_SKIP_NODE=1 to skip explicitly"
        );
        eprintln!("SKIPPED: UI admission gate not run");
        return;
    }
    let cases: &[(&str, &str, &str)] = &[
        ("histogram_default", "histogram(randn(1,200))", "supported"),
        (
            "histogram_horizontal_pdf",
            "histogram(rand(1,100), 'Normalization', 'pdf', 'Orientation', 'horizontal', 'FaceColor', [0.7 0.7 0.7]); hold on; plot([1 1], [0 1], 'r')",
            "supported",
        ),
        (
            "histogram_stairs",
            "histogram(rand(1,100), 'DisplayStyle', 'stairs', 'Normalization', 'cdf')",
            "supported",
        ),
        (
            "histogram_two",
            "histogram(randn(1,50)); hold on; histogram(randn(1,50)+2, 'EdgeColor', 'none', 'FaceAlpha', 1); legend('a','b')",
            "supported",
        ),
        ("hist_plot", "hist([1 2 2 3 7])", "supported"),
        ("hist_matrix_plot", "hist([1 2;3 4;5 5], 3)", "supported"),
        (
            "sgtitle_grid",
            "for k = 1:4, subplot(2,2,k); plot(1:k); end; sgtitle({'iki', 'satır'})",
            "supported",
        ),
        ("pie_basic", "pie([1 2 3]); title('pasta')", "supported"),
        (
            "pie_explode_labels",
            "figure('Position',[0 0 900 300]); subplot(1,2,1); pie([3 1 1], [1 0 0], {'a','b','c'}); subplot(1,2,2); plot(1:3)",
            "supported",
        ),
        (
            "yyaxis_pair",
            "yyaxis left; plot(1:3); yyaxis right; plot([10 30 20])",
            "unsupported_object",
        ),
        (
            "ssiodev_like",
            "figure('Color','w','Position',[100 50 1000 800]); axes('Position',[0.1 0.72 0.62 0.2]); plot(1:10,'k.-'); \
             axes('Position',[0.79 0.72 0.17 0.2]); histogram(rand(1,128),'Normalization','pdf','Orientation','horizontal','FaceColor',[0.7 0.7 0.7]); \
             hold on; plot([1 1],[0 1],'r','LineWidth',1.5); hold off; sgtitle('Rastgele','FontWeight','bold')",
            "supported",
        ),
    ];
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("r2-validate");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    let mut expect = Vec::new();
    for (name, code, want) in cases {
        let mut it = interp();
        ok(&mut it, code);
        let figs = open_figures(&mut it);
        assert!(!figs.is_empty(), "{name}: no figure");
        for f in figs {
            let file = format!("{name}_{}", f.number);
            std::fs::write(out.join(format!("{file}.json")), &f.json).unwrap();
            expect.push(format!("\"{file}\":\"{want}\""));
        }
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
    eprintln!("{stdout}");
    assert!(result.status.success(), "UI gate failures:\n{stdout}");
}
