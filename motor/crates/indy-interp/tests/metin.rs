//! String function positions and argument checks (MATLAB rules).

use indy_interp::Interp;

fn ok(code: &str) {
    let mut it = Interp::new();
    if let Err(e) = it.run_text(code) {
        panic!("snippet failed: {}\n---\n{}", e.message, code);
    }
}

fn fails(code: &str) {
    let mut it = Interp::new();
    assert!(it.run_text(code).is_err(), "expected an error from: {code}");
}

#[test]
fn extract_between_positions() {
    ok("assert(isequal(extractBetween('abcdef', 2, 4), {'bcd'}));
        assert(isequal(extractBetween('abcdef', 2, 4, 'Boundaries', 'exclusive'), {'c'}));
        assert(isequal(extractBetween('abcdef', 2, 4, 'Boundaries', 'inclusive'), {'bcd'}));
        assert(isequal(extractBetween('çığ', 2, 3), {'ığ'}));
        assert(isequal(extractBetween({'çığ', 'şöü'}, 2, 3), {'ığ', 'öü'}));
        assert(isequal(extractBetween({'abc', 'defg'}, [1 2], [2 4]), {'ab', 'efg'}));
        assert(extractBetween(\"abcdef\", 3, 3) == \"c\");
        assert(isequal(extractBetween('[a][b]', '[', ']'), {'a'; 'b'}));");
    fails("extractBetween('abc', 2, 5);");
    fails("extractBetween('abc', 1, 2, 'Bogus', true);");
    fails("extractBetween('[a]', '[', ']', 'Bogus', true);");
}

#[test]
fn pad_and_strip_sides() {
    ok("assert(strcmp(pad('a', 3), 'a  ') && strcmp(pad('a', 3, 'left'), '  a'));
        assert(strcmp(pad('a', 4, 'both', '*'), '*a**') && strcmp(pad('ab', 4, 'right', '.'), 'ab..'));
        assert(strcmp(strip('  a  '), 'a') && strcmp(strip('xxaxx', 'x'), 'a'));
        assert(strcmp(strip('xxaxx', 'left', 'x'), 'axx') && strcmp(strip('  a ', 'right'), '  a'));");
    fails("pad('a', 3, 'middle');");
    fails("strip('a', 'middle');");
    fails("strip('a', 'left', 'xy');");
}

#[test]
fn unknown_options_are_errors() {
    ok(
        "assert(contains('Abc', 'a', 'IgnoreCase', true) && ~contains('Abc', 'a'));
        assert(startsWith('Abc', 'a', 'IgnoreCase', 1) && endsWith('abC', 'c', 'IgnoreCase', 1));
        assert(count('aAa', 'a', 'IgnoreCase', true) == 3);
        assert(isequal(split('a,b', ','), {'a'; 'b'}));",
    );
    for c in [
        "contains('a', 'a', 'Bogus', true);",
        "startsWith('a', 'a', 'Bogus', true);",
        "endsWith('a', 'a', 'Bogus', true);",
        "count('a', 'a', 'Bogus', true);",
        "split('a', ',', 1, 'Bogus');",
        "contains('a', 'a', 'IgnoreCase');",
    ] {
        fails(c);
    }
}
