//! Lane r/grafik-3: `isfigure`, application data, `UserData`, `ancestor`,
//! `allchild`, `gco`, figure `DefaultAxesColorOrder`/`Units`, and dot
//! access on graphics handles through the interpreter's handle-field hooks.
//! Where Octave has the function, the expected results were measured with
//! `octave-cli` 11.3 as a black box (noted per test).

use indy_graphics_builtins::register;
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

// ----- isfigure ------------------------------------------------------------------

#[test]
fn isfigure_matches_octave() {
    // Octave: isfigure(f)=1, isfigure(0)=0, isfigure([f 0 99])=[1 0 0],
    // axes 0, char 0, [] -> 0x0, f+0.5 -> 0.
    let mut it = interp();
    ok(&mut it, "f = figure(); ax = axes();");
    check(&mut it, "isfigure(f) && ~isfigure(0) && ~isfigure(ax)");
    check(&mut it, "isequal(isfigure([f 0 99]), logical([1 0 0]))");
    check(
        &mut it,
        "~isfigure('a') && isequal(size(isfigure([])), [0 0])",
    );
    check(&mut it, "~isfigure(f + 0.5) && islogical(isfigure(f))");
    ok(&mut it, "close(f);");
    check(&mut it, "~isfigure(f)");
}

// ----- application data ----------------------------------------------------------

#[test]
fn appdata_set_get_is_rm() {
    // Octave: getappdata(f) lists fields in insertion order, a missing
    // name gives [], rmappdata of a missing name errors.
    let mut it = interp();
    ok(
        &mut it,
        "f = figure(); setappdata(f, 'b', 1); setappdata(f, 'a', {2});",
    );
    check(&mut it, "isequal(fieldnames(getappdata(f)), {'b'; 'a'})");
    check(
        &mut it,
        "isequal(getappdata(f, 'a'), {2}) && isempty(getappdata(f, 'zz'))",
    );
    check(&mut it, "isappdata(f, 'a') && ~isappdata(f, 'zz')");
    ok(&mut it, "rmappdata(f, 'a');");
    check(&mut it, "~isappdata(f, 'a')");
    let (_, msg) = fails(&mut it, "rmappdata(f, 'nope')");
    assert_eq!(msg, "rmappdata: appdata 'nope' is not present");
    // Several pairs, the cell form, overwrite in place, several names.
    ok(
        &mut it,
        "setappdata(f, 'x', 1, 'y', 2); setappdata(f, {'p', 'q'}, {3, 4});",
    );
    ok(&mut it, "setappdata(f, 'b', 'new');");
    check(
        &mut it,
        "isequal(fieldnames(getappdata(f)), {'b'; 'x'; 'y'; 'p'; 'q'})",
    );
    check(
        &mut it,
        "strcmp(getappdata(f, 'b'), 'new') && getappdata(f, 'q') == 4",
    );
    ok(&mut it, "rmappdata(f, 'x', 'y');");
    check(&mut it, "~isappdata(f, 'x') && ~isappdata(f, 'y')");
    // Any value: structs, function handles.
    ok(
        &mut it,
        "s.k = [1 2]; setappdata(f, 's', s); setappdata(f, 'fn', @sin);",
    );
    check(
        &mut it,
        "isequal(getappdata(f, 's'), s) && getappdata(f, 'fn')(0) == 0",
    );
}

#[test]
fn appdata_vectors_root_and_errors() {
    let mut it = interp();
    ok(&mut it, "f = figure(); ax = axes(); h = [f ax];");
    ok(&mut it, "setappdata(h, 'm', 7);");
    // Octave: a vector gives a cell column / a logical per handle.
    check(&mut it, "isequal(getappdata(h, 'm'), {7; 7})");
    check(&mut it, "isequal(isappdata(h, 'm'), logical([1 1]))");
    // The root object has application data too.
    ok(&mut it, "setappdata(0, 'r', 5);");
    check(&mut it, "getappdata(0, 'r') == 5");
    // Octave messages for a non-handle and a bad name.
    for code in [
        "setappdata(12345, 'a', 1)",
        "getappdata(12345, 'a')",
        "isappdata(12345, 'a')",
        "getappdata(12345)",
    ] {
        let (_, msg) = fails(&mut it, code);
        assert!(
            msg.ends_with("H must be a scalar or vector of graphic handles"),
            "{code}: {msg}"
        );
    }
    let (_, msg) = fails(&mut it, "setappdata(f, 3, 1)");
    assert_eq!(msg, "setappdata: NAME must be a string or cellstr");
    let (_, msg) = fails(&mut it, "getappdata(f, {'x'})");
    assert_eq!(msg, "getappdata: NAME must be a string");
    fails(&mut it, "setappdata(f, 'b')");
}

#[test]
fn appdata_survives_set_and_goes_with_the_object() {
    let mut it = interp();
    ok(
        &mut it,
        "f = figure(1); setappdata(f, 'b', 1); set(f, 'Name', 'zz');",
    );
    check(&mut it, "getappdata(f, 'b') == 1");
    ok(
        &mut it,
        "h = plot(1:3); setappdata(h, 'k', 2); set(h, 'Color', [1 0 0]);",
    );
    ok(&mut it, "h.LineWidth = 2;");
    check(&mut it, "getappdata(h, 'k') == 2");
    // Deleted with the object; a reused figure number starts empty (Octave).
    ok(&mut it, "delete(h);");
    fails(&mut it, "getappdata(h, 'k')");
    ok(&mut it, "close(f); f = figure(1);");
    check(
        &mut it,
        "~isappdata(f, 'b') && isempty(fieldnames(getappdata(f)))",
    );
    // clf deletes the axes and their data, not the figure's.
    ok(
        &mut it,
        "setappdata(f, 'c', 3); ax = gca(); setappdata(ax, 'd', 4); clf;",
    );
    check(&mut it, "getappdata(f, 'c') == 3 && ~isgraphics(ax)");
}

#[test]
fn user_data_holds_any_value() {
    let mut it = interp();
    ok(&mut it, "h = plot(1:3);");
    check(&mut it, "isempty(get(h, 'UserData'))");
    ok(
        &mut it,
        "s.a = {1, 'x'}; set(h, 'UserData', s, 'LineWidth', 4);",
    );
    check(
        &mut it,
        "isequal(get(h, 'UserData'), s) && get(h, 'LineWidth') == 4",
    );
    ok(&mut it, "h.UserData = 'z';");
    check(
        &mut it,
        "strcmp(h.UserData, 'z') && strcmp(get(h, 'userdata'), 'z')",
    );
    ok(&mut it, "f = gcf(); set(f, 'UserData', 7);");
    check(&mut it, "get(f, 'UserData') == 7");
}

// ----- ancestor, allchild, gco ------------------------------------------------------

#[test]
fn ancestor_matches_octave() {
    let mut it = interp();
    ok(&mut it, "f = figure(); ax = axes(); l = line(1:2, 1:2);");
    check(
        &mut it,
        "ancestor(l, 'figure') == f && ancestor(l, 'axes') == ax",
    );
    check(
        &mut it,
        "ancestor(l, {'hggroup', 'axes'}) == ax && ancestor(l, 'line') == l",
    );
    // Case-insensitive type; no match -> [] (0x0); 'toplevel'.
    check(&mut it, "ancestor(l, 'Figure') == f");
    check(&mut it, "isequal(size(ancestor(l, 'text')), [0 0])");
    check(&mut it, "ancestor(l, 'figure', 'toplevel') == f");
    check(&mut it, "ancestor(l, 'axes', 'toplevel') == ax");
    // The root is an ancestor of everything (Octave: 0).
    check(
        &mut it,
        "ancestor(f, 'root') == 0 && ancestor(0, 'root') == 0",
    );
    // Several handles give a cell column.
    ok(&mut it, "c = ancestor([l; ax], 'line');");
    check(
        &mut it,
        "iscell(c) && isequal(size(c), [2 1]) && c{1} == l && isempty(c{2})",
    );
    // Not a live handle: [] (Octave does not error).
    check(
        &mut it,
        "isempty(ancestor(99999, 'figure')) && isempty(ancestor([], 'figure'))",
    );
    check(&mut it, "isempty(ancestor(l, {}))");
    let (_, msg) = fails(&mut it, "ancestor(l, 3)");
    assert!(
        msg.contains("TYPE must be a string or cell array of strings"),
        "{msg}"
    );
    let (_, msg) = fails(&mut it, "ancestor(l, 'figure', 'x')");
    assert!(msg.contains("third argument must be \"toplevel\""), "{msg}");
    fails(&mut it, "ancestor(l)");
}

#[test]
fn ancestor_toplevel_through_groups() {
    // A bar group inside an axes: nearest and outermost 'hggroup'/'axes'.
    let mut it = interp();
    ok(&mut it, "b = bar([1 2 3]); ax = gca();");
    check(
        &mut it,
        "ancestor(b, 'hggroup') == b && ancestor(b, 'axes', 'toplevel') == ax",
    );
    check(
        &mut it,
        "ancestor(b, {'axes', 'figure'}, 'toplevel') == gcf()",
    );
}

#[test]
fn allchild_lists_hidden_labels() {
    // Octave: plotted children newest first, then title zlabel ylabel xlabel.
    let mut it = interp();
    ok(
        &mut it,
        "f = figure(); ax = axes(); l = line(1:2, 1:2); l2 = line(1:2, 1:2);",
    );
    ok(&mut it, "c = allchild(ax);");
    check(
        &mut it,
        "isequal(c, [l2; l; get(ax, 'Title'); get(ax, 'ZLabel'); get(ax, 'YLabel'); get(ax, 'XLabel')])",
    );
    check(
        &mut it,
        "any(allchild(f) == ax) && isequal(size(allchild(l)), [0 0])",
    );
    check(
        &mut it,
        "iscell(allchild([ax; f])) && isempty(allchild([]))",
    );
    fails(&mut it, "allchild(99999)");
}

#[test]
fn gco_is_empty_without_clicks() {
    // MATLAB: the object last clicked, [] if none; the Rust engine figures take
    // no clicks. Octave without interaction: [] and the same HFIG error.
    let mut it = interp();
    check(&mut it, "isempty(gco()) && isequal(size(gco()), [0 0])");
    check(&mut it, "isempty(get(0, 'CurrentFigure'))"); // gco made no figure
    ok(&mut it, "f = figure(); ax = axes(); plot(1:3);");
    check(
        &mut it,
        "isempty(gco()) && isempty(gco(f)) && isempty(get(f, 'CurrentObject'))",
    );
    let (_, msg) = fails(&mut it, "gco(ax)");
    assert!(
        msg.contains("HFIG must be a graphics handle to a figure object"),
        "{msg}"
    );
    fails(&mut it, "set(f, 'CurrentObject', ax)");
}

// ----- figure properties used by colororder/tiledlayout ----------------------------------

#[test]
fn figure_default_axes_color_order() {
    let mut it = interp();
    ok(&mut it, "f = figure(); a = axes();");
    check(
        &mut it,
        "isequal(get(f, 'DefaultAxesColorOrder'), get(0, 'DefaultAxesColorOrder'))",
    );
    check(
        &mut it,
        "isequal(get(a, 'ColorOrder'), get(0, 'DefaultAxesColorOrder'))",
    );
    ok(
        &mut it,
        "c = [1 0 0; 0 0 1]; set(f, 'DefaultAxesColorOrder', c); b = axes();",
    );
    check(
        &mut it,
        "isequal(get(f, 'DefaultAxesColorOrder'), c) && isequal(get(b, 'ColorOrder'), c)",
    );
    // An axes reset by a new plot takes the figure default again.
    ok(
        &mut it,
        "set(b, 'ColorOrder', [0 1 0]); axes(b); h = plot(1:3);",
    );
    check(
        &mut it,
        "isequal(get(b, 'ColorOrder'), c) && isequal(get(h, 'Color'), [1 0 0])",
    );
}

#[test]
fn units_are_reported_and_other_units_refused() {
    let mut it = interp();
    ok(&mut it, "f = figure(); ax = axes();");
    check(
        &mut it,
        "strcmp(get(f, 'Units'), 'pixels') && strcmp(get(ax, 'Units'), 'normalized')",
    );
    ok(
        &mut it,
        "set(ax, 'Units', 'normalized'); set(f, 'Units', 'pixels');",
    );
    ok(
        &mut it,
        "b = axes('Parent', f, 'Units', 'normalized', 'Position', [0.1 0.1 0.5 0.5]);",
    );
    let (id, _) = fails(&mut it, "set(ax, 'Units', 'pixels')");
    assert_eq!(id, "IndyMAT:graphics:unsupported");
    fails(&mut it, "set(ax, 'Units', 'furlongs')");
}

// ----- dot access ------------------------------------------------------------------

#[test]
fn dot_get_and_set_on_handles() {
    let mut it = interp();
    ok(&mut it, "h = plot(1:3); h.LineWidth = 3;");
    check(&mut it, "h.LineWidth == 3 && get(h, 'LineWidth') == 3");
    ok(&mut it, "ax = gca(); ax.XLim = [0 5];");
    check(&mut it, "isequal(xlim(), [0 5]) && ax.XLim(2) == 5");
    // Indexed assignment into a property: read, modify, write back.
    ok(&mut it, "ax.XLim(2) = 7;");
    check(&mut it, "isequal(ax.XLim, [0 7])");
    // Chains through handle-valued properties, on both sides.
    check(
        &mut it,
        "strcmp(h.Parent.Type, 'axes') && h.Parent.Parent == gcf()",
    );
    ok(&mut it, "h.Parent.YLim = [1 2];");
    check(&mut it, "isequal(ylim(), [1 2])");
    // Handles keep their value: the variable is not replaced.
    check(
        &mut it,
        "isequal(class(h), 'double') && h == findobj(gca(), 'Type', 'line')",
    );
    // Names are case-insensitive, as in get/set; dynamic field names work.
    check(&mut it, "isequal(h.color, get(h, 'Color'))");
    ok(&mut it, "p = 'Marker'; h.(p) = 'o';");
    check(&mut it, "strcmp(h.(p), 'o')");
    // A function call result: gca().XLim and gcf().Name.
    ok(&mut it, "f = gcf(); f.Name = 'abc';");
    check(
        &mut it,
        "strcmp(gcf().Name, 'abc') && isequal(gca().XLim, [0 7])",
    );
    // The histogram object's own properties.
    ok(
        &mut it,
        "figure(); hh = histogram([1 2 2 3 3 3], [0.5 1.5 2.5 3.5]);",
    );
    check(
        &mut it,
        "isequal(hh.Values, [1 2 3]) && isequal(hh.BinEdges, [0.5 1.5 2.5 3.5])",
    );
}

#[test]
fn dot_on_handle_arrays() {
    let mut it = interp();
    ok(&mut it, "h = plot([1 2; 3 4]);");
    ok(&mut it, "h(2).LineWidth = 5;");
    check(&mut it, "h(2).LineWidth == 5 && h(1).LineWidth == 0.5");
    // Several handles: a comma-separated list.
    ok(&mut it, "w = [h.LineWidth];");
    assert_eq!(var(&it, "w").to_f64_vec(), Some(vec![0.5, 5.0]));
    let (_, msg) = fails(&mut it, "h.LineWidth = 2");
    assert!(msg.contains("single graphics object"), "{msg}");
}

#[test]
fn dot_errors_keep_their_meaning() {
    let mut it = interp();
    // No graphics at all, and with figures open: a plain double keeps the
    // interpreter's own messages.
    let plain_get = "Dot indexing is not supported for variables of this type (double).";
    let plain_set = "Unable to perform assignment because dot indexing is not supported for variables of this type (double).";
    ok(&mut it, "x = 5.5; y = [1 2 3];");
    assert_eq!(fails(&mut it, "x.foo").1, plain_get);
    assert_eq!(fails(&mut it, "x.foo = 1").1, plain_set);
    ok(&mut it, "h = plot(1:3);");
    assert_eq!(fails(&mut it, "x.foo").1, plain_get);
    assert_eq!(fails(&mut it, "y(2).foo").1, plain_get);
    assert_eq!(fails(&mut it, "x.foo = 1").1, plain_set);
    // The root (0) is never read as a handle: 0 is too common a number.
    ok(&mut it, "z = 0;");
    assert_eq!(fails(&mut it, "z.Children").1, plain_get);
    // An unknown property of a live handle is the graphics error.
    let (id, _) = fails(&mut it, "h.NoSuchProperty");
    assert_eq!(id, "MATLAB:hg:InvalidProperty");
    let (id, _) = fails(&mut it, "h.NoSuchProperty = 1");
    assert_eq!(id, "MATLAB:hg:InvalidProperty");
    // Read-only properties stay read-only.
    fails(&mut it, "h.Type = 'x'");
    // A deleted object: MATLAB's message, for get and set.
    ok(&mut it, "delete(h);");
    assert_eq!(fails(&mut it, "h.Color").1, "Invalid or deleted object.");
    assert_eq!(
        fails(&mut it, "h.Color = [1 0 0]").1,
        "Invalid or deleted object."
    );
    // Structs are unaffected.
    ok(&mut it, "s.a = 1; s.a = 2;");
    check(&mut it, "s.a == 2");
}

#[test]
fn dot_without_graphics_registered_is_unchanged() {
    let mut it = Interp::new();
    it.out = Box::new(std::io::sink());
    assert!(it.handle_field_get.is_none() && it.handle_field_set.is_none());
    ok(&mut it, "x = 1;");
    assert_eq!(
        fails(&mut it, "x.a").1,
        "Dot indexing is not supported for variables of this type (double)."
    );
}
