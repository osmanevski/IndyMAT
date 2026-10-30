//! Behaviour of the graphics builtins as MATLAB code sees them.

use indy_graphics_builtins::{UNAVAILABLE, open_figures, register};
use indy_interp::{Interp, Value, catch_panic};

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

fn var(it: &Interp, name: &str) -> Value {
    it.frames[0].vars.get(name).cloned().unwrap_or_default()
}

fn check(it: &mut Interp, code: &str) {
    ok(it, &format!("assert({code});"));
}

#[test]
fn handles_are_doubles_and_properties_go_through_the_model() {
    let mut it = interp();
    ok(
        &mut it,
        "figure(); h = plot(1:3, [2 4 6], 'r--', 'LineWidth', 2);",
    );
    assert_eq!(var(&it, "h").class_name(), "double");
    check(
        &mut it,
        "isequal(get(h, 'YData'), [2 4 6]) && strcmp(get(h, 'Type'), 'line')",
    );
    check(
        &mut it,
        "strcmp(get(h, 'LineStyle'), '--') && isequal(get(h, 'Color'), [1 0 0])",
    );
    ok(&mut it, "set(h, 'Marker', 'o');");
    check(&mut it, "strcmp(get(h, 'Marker'), 'o')");
    check(&mut it, "strcmp(get(gca, 'Type'), 'axes') && gcf == 1");
    check(
        &mut it,
        "ishandle(h) && isgraphics(h, 'line') && ~isgraphics(h, 'axes')",
    );
    check(
        &mut it,
        "isequal(ishandle([h 12345]), [true false]) && ~ishandle('x')",
    );
    // A vector of handles gives a cell column; set reaches each handle.
    ok(
        &mut it,
        "hs = plot(1:3, [1 2 3; 4 5 6]'); set(hs, 'LineWidth', 4); w = get(hs, 'LineWidth');",
    );
    check(&mut it, "iscell(w) && isequal(size(w), [2 1]) && w{2} == 4");
    check(&mut it, "isequal(size(hs), [2 1])");
    // The parent of a line is the axes; handle properties come back as doubles.
    check(
        &mut it,
        "get(hs(1), 'Parent') == gca && get(gca, 'Parent') == gcf",
    );
    ok(&mut it, "delete(hs(1));");
    check(&mut it, "~ishandle(hs(1)) && ishandle(hs(2))");
}

#[test]
fn values_convert_both_ways() {
    let mut it = interp();
    // Class is kept for colour data; char matrices are multi-line text.
    ok(&mut it, "im = image(uint8([1 2; 3 4]));");
    check(&mut it, "isa(get(im, 'CData'), 'uint8')");
    ok(&mut it, "t = title(['ab'; 'cd']);");
    check(&mut it, "isequal(get(t, 'String'), {'ab', 'cd'})");
    ok(&mut it, "plot(1:3); l = legend({'veri'});");
    check(&mut it, "isequal(get(l, 'String'), {'veri'})");
    ok(&mut it, "xlabel(\"string scalar\");");
    check(
        &mut it,
        "strcmp(get(get(gca, 'XLabel'), 'String'), 'string scalar')",
    );
    ok(&mut it, "plot(logical([1 0 1]));");
    // Graphics errors keep their MATLAB identifiers.
    let (id, _) = fails(&mut it, "figure(0)");
    assert_eq!(id, "MATLAB:figure:InvalidNumber");
    let (id, msg) = fails(&mut it, "get(gca, 'NoSuchProperty')");
    assert!(id.starts_with("MATLAB:"), "{id}: {msg}");
    let (_, msg) = fails(&mut it, "plot(1:3, struct('a', 1))");
    assert!(msg.starts_with("plot: "), "{msg}");
    let (_, msg) = fails(&mut it, "plot([1 2] + 1i)");
    assert!(msg.contains("complex"), "{msg}");
}

#[test]
fn queries_and_command_forms() {
    let mut it = interp();
    ok(&mut it, "plot(1:10); xlim([2 5]); ylim([0 20]);");
    check(&mut it, "isequal(xlim(), [2 5]) && isequal(ylim(), [0 20])");
    check(&mut it, "strcmp(xlim('mode'), 'manual')");
    ok(&mut it, "xlim auto; axis([0 1 0 2]);");
    check(&mut it, "isequal(axis(), [0 1 0 2])");
    ok(
        &mut it,
        "hold on; plot(1:3); hold off; grid on; box on; axis tight;",
    );
    check(&mut it, "strcmp(get(gca, 'XGrid'), 'on')");
    check(&mut it, "numel(findobj(gca, 'Type', 'line')) == 2");
    ok(&mut it, "surf(peaks(5)); view(30, 40); [az, el] = view();");
    check(&mut it, "az == 30 && el == 40 && isequal(view(), [30 40])");
    ok(&mut it, "view(2);");
    check(&mut it, "isequal(view(), [0 90])");
    ok(&mut it, "caxis([0 5]); c = caxis(); clim auto;");
    check(&mut it, "isequal(c, [0 5])");
    ok(&mut it, "m = colormap(jet(8)); m2 = colormap();");
    check(
        &mut it,
        "isequal(size(m), [8 3]) && isequal(m, m2) && isequal(size(gray(3)), [3 3])",
    );
    check(&mut it, "size(gray(), 1) == 8");
    ok(&mut it, "xticks([0 2 4]); xticklabels({'a', 'b', 'c'});");
    check(
        &mut it,
        "isequal(xticks(), [0 2 4]) && isequal(xticklabels(), {'a', 'b', 'c'})",
    );
    ok(&mut it, "a = subplot(2, 1, 1); b = subplot(212);");
    check(
        &mut it,
        "a ~= b && numel(findobj(gcf, 'Type', 'axes')) == 2",
    );
    ok(
        &mut it,
        "f = figure('visible', 'off'); ax = axes(f); hold(ax, 'on');",
    );
    check(
        &mut it,
        "strcmp(get(ax, 'nextplot'), 'add') && get(ax, 'Parent') == f && gcf == f",
    );
    ok(&mut it, "drawnow; drawnow limitrate;");
    // Plots return handles only when asked: no `ans`.
    ok(&mut it, "clear ans; plot(1:3);");
    check(&mut it, "~exist('ans', 'var')");
    ok(&mut it, "close all;");
    check(&mut it, "isempty(findobj(0, 'Type', 'figure'))");
}

#[test]
fn uncovered_names_fail_explicitly() {
    let mut it = interp();
    for name in UNAVAILABLE {
        let (id, msg) = fails(&mut it, &format!("{name}(1)"));
        assert_eq!(id, "IndyR:graphicsUnavailable", "{name}");
        assert_eq!(
            msg,
            format!("{name}: graphics not yet available in the Rust engine")
        );
    }
    let (id, msg) = fails(&mut it, "[c, h] = contour(peaks(5));");
    assert_eq!(id, "IndyR:graphicsUnavailable");
    assert!(msg.contains("contour matrix"), "{msg}");
}

#[test]
fn delete_still_removes_files_and_objects() {
    let dir = std::env::temp_dir().join(format!("indy-gfx-delete-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("x.txt");
    std::fs::write(&file, "x").unwrap();
    let mut it = interp();
    ok(&mut it, &format!("delete('{}');", file.display()));
    assert!(!file.exists());
    let _ = std::fs::remove_dir(&dir);
    ok(&mut it, "h = plot(1:3); delete(h);");
    check(&mut it, "~ishandle(h)");
    let (_, msg) = fails(&mut it, "delete(h)");
    assert!(msg.contains("invalid or deleted"), "{msg}");
}

#[test]
fn open_figures_tracks_changes_and_closes() {
    let mut it = interp();
    assert!(open_figures(&mut it).is_empty());
    ok(&mut it, "plot(1:3); figure(3); set(gcf, 'Name', 'Drei');");
    let figs = open_figures(&mut it);
    let names: Vec<(u64, &str)> = figs.iter().map(|f| (f.number, f.name.as_str())).collect();
    assert_eq!(names, [(1, "Figure 1"), (3, "Drei")]);
    assert!(figs[0].json.starts_with('{'));
    // Unchanged figures are still listed.
    assert_eq!(open_figures(&mut it).len(), 2);
    ok(&mut it, "close(1);");
    let figs = open_figures(&mut it);
    assert_eq!(figs.len(), 1);
    assert_eq!(figs[0].number, 3);
}

#[test]
fn user_functions_still_shadow_the_builtins() {
    let dir = std::env::temp_dir().join(format!("indy-gfx-shadow-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("plot.m"), "function r = plot(x)\n  r = 42;\nend\n").unwrap();
    let mut it = interp();
    it.add_path(dir.clone(), true);
    ok(&mut it, "r = plot(1);");
    check(&mut it, "r == 42");
    let _ = std::fs::remove_dir_all(&dir);
}

/// No builtin may panic, whatever the arguments.
#[test]
fn graphics_builtins_never_panic() {
    // `print`, `saveas` and `exportgraphics` really write what the junk asks for.
    let scratch = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("never-panic");
    std::fs::create_dir_all(&scratch).unwrap();
    let plain = Interp::new();
    let probe = interp();
    let mut names: Vec<&'static str> = probe
        .builtins
        .keys()
        .copied()
        .filter(|n| !plain.builtins.contains_key(n) || *n == "delete")
        .collect();
    names.sort();
    let args = [
        "",
        "1",
        "-1",
        "0",
        "NaN",
        "Inf",
        "[]",
        "{}",
        "'on'",
        "'off'",
        "'r--o'",
        "'all'",
        "{'a', 'b'}",
        "struct('a', 1)",
        "1:3",
        "[1 2; 3 4]",
        "1e300",
        "-1e300",
        "magic(4)",
        "1, 2",
        "1:3, [3 1 2]",
        "1:3, [3 1 2], 'LineWidth'",
        "1:3, [3 1 2], 'LineWidth', NaN",
        "'Color', 'r'",
        "gca",
        "gcf",
        "gca, 'XLim'",
        "gca, 'XLim', [1 1]",
        "zeros(0, 3)",
        "ones(3, 3, 3)",
        "int8([1 2 3])",
        "true(2)",
        "@sin",
        "\"text\"",
        "1, 2, 3, 4, 5, 6, 7",
        "[1 2; 3 4], [5 6; 7 8], [1 1; 1 1]",
        "[], [], []",
    ];
    for name in &names {
        for a in &args {
            let mut it = interp();
            it.set_cwd(scratch.clone());
            let code = format!("{name}({a});");
            let r = catch_panic(|| it.run_text(&code).map(|_| ()));
            if let Err(e) = r {
                assert!(
                    !e.message.contains("internal") && !e.message.contains("panic"),
                    "{code}: {}",
                    e.message
                );
            }
            // Whatever happened, the figures still export as JSON.
            for f in open_figures(&mut it) {
                assert!(f.json.starts_with('{'), "{code}");
                assert!(f.png.is_some(), "the renderer failed after {code}");
            }
        }
    }
    assert!(names.len() > 60, "{names:?}");
}
