//! Regressions found over the real app (29 Oct 2026): string scalars as
//! formats and map keys, and `disp` of a cell.

use indy_interp::Interp;

/// Run `src`, then return the text variable `var`.
fn text(src: &str, var: &str) -> String {
    let mut it = Interp::new();
    if let Err(e) = it.run_text(src) {
        panic!("{}\n  -> {} ({})", src, e.message, e.identifier);
    }
    it.get_var(var)
        .and_then(|v| v.to_str())
        .unwrap_or_else(|| panic!("no text variable {}", var))
}

#[test]
fn string_scalar_format() {
    let src = r#"a = 1.5; b = 2;
        s = evalc('fprintf("%.4f vs %.4f\n", a, b)');
        t = evalc('fprintf(1, "%s=%d\n", "x", 3)');
        u = evalc('printf("%d|", [1 2])');
        try, error("my:id", "val %d", 3); catch e, m = e.message; end
        try, assert(false, "bad %d", 4); catch e, n = e.message; end
        w = evalc('warning("w %s", "str")');
        z = evalc('disp(num2str(pi, "%8.3f"))');"#;
    assert_eq!(text(src, "s"), "1.5000 vs 2.0000\n");
    assert_eq!(text(src, "t"), "x=3\n");
    assert_eq!(text(src, "u"), "1|2|");
    assert_eq!(text(src, "m"), "val 3");
    assert_eq!(text(src, "n"), "bad 4");
    assert!(text(src, "w").contains("w str"));
    assert_eq!(text(src, "z").trim(), "3.142");
}

#[test]
fn string_scalar_map_keys() {
    let src = r#"m = containers.Map(); m("a") = 1; m("b") = 2;
        k = strjoin(m.keys(), ",");
        has = [isKey(m, "a"), isKey(m, "z")];
        both = isKey(m, {"a", "z"});
        v = m("b");
        vs = values(m, {"a", "b"});
        remove(m, "a");
        left = m.Count;
        n = containers.Map({"x", "y"}, [3 4]);
        r = [n("x"), n("y")];
        out = sprintf("%s|%d%d|%d%d|%d|%d|%d%d|%d", k, has, both, v, left, r, vs{2});"#;
    assert_eq!(text(src, "out"), "a,b|10|10|2|1|34|2");
}

#[test]
fn disp_of_a_cell_shows_contents() {
    let src = r#"a = evalc("disp({'a'})");
        b = evalc("disp({1, 'x'; [1 2 3], {2}})");
        m = containers.Map({'k'}, {5});
        c = evalc("disp(m.keys())");
        d = evalc("x = {'a'}");"#;
    assert_eq!(text(src, "a"), "  {'a'}    \n");
    assert_eq!(
        text(src, "b"),
        "  {1}    {'x'}    \n  {[1 2 3]}    {[1x1 cell]}    \n"
    );
    assert_eq!(text(src, "c"), "  {'k'}    \n");
    // The variable display wraps the same body.
    assert_eq!(text(src, "d"), "x =\n\n  {'a'}    \n\n");
}

#[test]
fn anonymous_functions_of_successive_evals_stay_apart() {
    // Each `eval` text is freed before the next one, whose context may get
    // the same address, with `@` at the same position.
    let src = r#"out = '';
        for k = 1:50
            a = eval('arrayfun(@(x) x*2, [1 2])');
            b = eval('arrayfun(@(x) x, [1 2])');
            c = eval('cellfun(@(x) x+1, {1})');
            out = [out sprintf('%d', [a b c])];
        end"#;
    assert_eq!(text(src, "out"), "24122".repeat(50));
}
