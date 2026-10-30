//! regexp / regexpi / regexprep / sscanf through the interpreter.

use indy_interp::Interp;

fn check(it: &mut Interp, cond: &str) {
    let code = format!("t__ = isequal({}, true);", cond);
    if let Err(e) = it.run_text(&code) {
        panic!("{} -> error: {}", cond, e.message);
    }
    let ok = it.get_var("t__").and_then(|v| v.scalar_f64()) == Some(1.0);
    assert!(ok, "failed: {}", cond);
}

fn run(it: &mut Interp, code: &str) {
    if let Err(e) = it.run_text(code) {
        panic!("{} -> error: {}", code, e.message);
    }
}

fn fails(it: &mut Interp, code: &str) {
    assert!(it.run_text(code).is_err(), "expected an error: {}", code);
}

#[test]
fn regexp_default_outputs() {
    let mut it = Interp::new();
    run(&mut it, "[s,e,te,m,t,nm,sp] = regexp('ab12cd3', '\\d+');");
    check(&mut it, "isequal(s, [3 7]) && isequal(e, [4 7])");
    check(&mut it, "isequal(m, {'12', '3'})");
    check(
        &mut it,
        "numel(t) == 2 && isequal(t{1}, {'12'}) && isequal(t{2}, {'3'})",
    );
    check(&mut it, "isequal(te{1}, [3 4])");
    check(&mut it, "isequal(sp, {'ab', 'cd', ''})");
    check(&mut it, "isstruct(nm) && isequal(size(nm), [1 1])");
}

#[test]
fn regexp_no_match_shapes() {
    let mut it = Interp::new();
    run(&mut it, "[s,e,te,m,t,nm,sp] = regexp('abc', 'x');");
    check(
        &mut it,
        "isequal(size(s), [1 0]) && isequal(size(e), [1 0])",
    );
    check(&mut it, "iscell(m) && isequal(size(m), [1 0])");
    check(&mut it, "iscell(t) && isequal(size(t), [1 0])");
    check(&mut it, "isequal(size(te), [1 0])");
    check(&mut it, "isequal(sp, {'abc'})");
    run(
        &mut it,
        "[m, t, s] = regexp('abc', 'x', 'match', 'tokens', 'start', 'once');",
    );
    check(
        &mut it,
        "ischar(m) && isempty(m) && iscell(t) && isempty(t) && isempty(s)",
    );
}

#[test]
fn regexp_selectors_tokens_names() {
    let mut it = Interp::new();
    run(
        &mut it,
        "[t, nm] = regexp('k=v; a=b', '(?<key>\\w)=(?<val>\\w)', 'tokens', 'names');",
    );
    check(
        &mut it,
        "numel(t) == 2 && isequal(t{1}, {'k', 'v'}) && isequal(t{2}, {'a', 'b'})",
    );
    check(
        &mut it,
        "isequal(size(nm), [1 2]) && strcmp(nm(2).key, 'a') && strcmp(nm(1).val, 'v')",
    );
    run(
        &mut it,
        "nm = regexp('k=v', '(?<key>\\w)=(?<val>\\w)', 'names');",
    );
    check(&mut it, "isequal(size(nm), [1 1]) && strcmp(nm.key, 'k')");
    // Pattern without groups: the whole match is the token (MATLAB).
    run(&mut it, "t = regexp('a1b2', '\\d', 'tokens');");
    check(&mut it, "numel(t) == 2 && isequal(t{2}, {'2'})");
    // Unmatched optional group gives an empty token.
    run(&mut it, "t = regexp('ab', '(a)(x)?b', 'tokens', 'once');");
    check(
        &mut it,
        "numel(t) == 2 && strcmp(t{1}, 'a') && isempty(t{2})",
    );
    check(
        &mut it,
        "strcmp(regexp('xaby', 'a(b)', 'match', 'once'), 'ab')",
    );
    check(
        &mut it,
        "isequal(regexp('xaby', 'a(b)', 'tokens', 'once'), {'b'})",
    );
    check(
        &mut it,
        "isequal(regexp('a,b,,c', ',', 'split'), {'a', 'b', '', 'c'})",
    );
    check(
        &mut it,
        "isequal(regexp('a1b2c', '\\d', 'split', 'once'), {'a', 'b2c'})",
    );
    check(
        &mut it,
        "isequal(regexp('abab', '(a)(b)', 'tokenExtents', 'once'), [1 1; 2 2])",
    );
}

#[test]
fn regexp_features() {
    let mut it = Interp::new();
    check(
        &mut it,
        "isequal(regexp('aaa', 'a+?', 'match'), {'a', 'a', 'a'})",
    );
    check(
        &mut it,
        "isequal(regexp('<b><i>', '<.*?>', 'match'), {'<b>', '<i>'})",
    );
    check(
        &mut it,
        "isequal(regexp('<b><i>', '<.*>', 'match'), {'<b><i>'})",
    );
    check(
        &mut it,
        "isequal(regexp('price $12, 7', '(?<=\\$)\\d+', 'match'), {'12'})",
    );
    check(
        &mut it,
        "isequal(regexp('a1 b2 c3', '[a-z](?!2)', 'match'), {'a', 'c'})",
    );
    check(
        &mut it,
        "isequal(regexp('foo.m bar.c', '\\w+(?=\\.m)', 'match'), {'foo'})",
    );
    check(
        &mut it,
        "isequal(regexp('ab xx yy', '(\\w)\\1', 'match'), {'xx', 'yy'})",
    );
    check(
        &mut it,
        "isequal(regexp('ab xx', '(?<c>\\w)\\k<c>', 'match'), {'xx'})",
    );
    check(
        &mut it,
        "isequal(regexp('ABc', '[a-z]+', 'match', 'ignorecase'), {'ABc'})",
    );
    check(&mut it, "isequal(regexpi('ABc', 'b', 'start'), 2)");
    check(&mut it, "isequal(regexp('ABc', '(?i)b'), 2)");
    check(
        &mut it,
        "isequal(regexp(sprintf('a\\nb'), '^\\w$', 'match', 'lineanchors'), {'a', 'b'})",
    );
    check(
        &mut it,
        "isempty(regexp(sprintf('a\\nb'), '^\\w$', 'match'))",
    );
    check(
        &mut it,
        "isequal(regexp(sprintf('a\\nb'), 'a.b', 'match', 'dotexceptnewline'), cell(1, 0))",
    );
    check(
        &mut it,
        "isequal(regexp('the cat', '\\<c\\w*', 'match'), {'cat'})",
    );
    check(
        &mut it,
        "isequal(regexp('x9y', '[^\\d]', 'match'), {'x', 'y'})",
    );
    check(&mut it, "isequal(regexp('aaaa', 'a{2}'), [1 3])");
    check(
        &mut it,
        "isequal(regexp('abc', 'x*', 'start', 'emptymatch'), [1 2 3 4])",
    );
    check(&mut it, "isempty(regexp('abc', 'x*', 'match'))");
    check(
        &mut it,
        "isequal(regexp('a+b', 'a\\x{2B}b', 'match'), {'a+b'})",
    );
}

#[test]
fn regexp_cell_inputs() {
    let mut it = Interp::new();
    run(&mut it, "x = regexp({'a1', 'b22'}, '\\d', 'match');");
    check(
        &mut it,
        "iscell(x) && isequal(size(x), [1 2]) && isequal(x{2}, {'2', '2'})",
    );
    run(&mut it, "x = regexp({'a1'; 'b2'}, '\\d', 'match', 'once');");
    check(&mut it, "isequal(x, {'1'; '2'})");
    run(&mut it, "x = regexp('a1b2', {'\\d', '[a-z]'}, 'match');");
    check(
        &mut it,
        "numel(x) == 2 && isequal(x{1}, {'1', '2'}) && isequal(x{2}, {'a', 'b'})",
    );
}

#[test]
fn regexp_errors_and_limits() {
    let mut it = Interp::new();
    fails(&mut it, "regexp('abc', '(a');");
    fails(&mut it, "regexp('abc', '[a');");
    fails(&mut it, "regexp('abc', '*a');");
    fails(&mut it, "regexp('abc', 'a', 'bogus');");
    fails(
        &mut it,
        "regexp('aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab', '(a|aa)*c');",
    );
}

#[test]
fn regexprep_cases() {
    let mut it = Interp::new();
    check(
        &mut it,
        "strcmp(regexprep('hello world', '(\\w+) (\\w+)', '$2 $1'), 'world hello')",
    );
    check(&mut it, "strcmp(regexprep('aaa', 'a', 'b', 'once'), 'baa')");
    check(&mut it, "strcmp(regexprep('aaa', 'a', 'b', 2), 'aba')");
    check(&mut it, "strcmp(regexprep('abc', 'x*', '-'), 'abc')");
    check(
        &mut it,
        "strcmp(regexprep('abc', '(?<n>b)', '[$0|$<n>]'), 'a[b|b]c')",
    );
    check(
        &mut it,
        "isequal(double(regexprep('a.b', '\\.', '\\n')), [97 10 98])",
    );
    check(
        &mut it,
        "strcmp(regexprep('ABC', 'b', 'x', 'ignorecase'), 'AxC')",
    );
    check(
        &mut it,
        "isequal(regexprep({'ab', 'cb'}, {'a', 'b'}, {'1', '2'}), {'12', 'c2'})",
    );
    check(&mut it, "strcmp(regexprep('a$b', '\\$', '\\$\\$'), 'a$$b')");
}

#[test]
fn sscanf_cases() {
    let mut it = Interp::new();
    run(&mut it, "[a, c, m] = sscanf('3 4 abc', '%d');");
    check(&mut it, "isequal(a, [3; 4]) && c == 2 && ~isempty(m)");
    run(&mut it, "[a, c, m] = sscanf('1,2,3', '%d,');");
    check(&mut it, "isequal(a, [1; 2; 3]) && c == 3 && isempty(m)");
    check(&mut it, "strcmp(sscanf('hello world', '%s'), 'helloworld')");
    check(
        &mut it,
        "isequal(sscanf('x=1.5 y=2', 'x=%f y=%d'), [1.5; 2])",
    );
    check(
        &mut it,
        "isequal(sscanf('1 2 3 4 5', '%d', [2 Inf]), [1 3 5; 2 4 0])",
    );
    check(&mut it, "isequal(sscanf('1 2 3 4 5', '%d', 3), [1; 2; 3])");
    check(&mut it, "isequal(sscanf('ab 12', '%s %d'), [97; 98; 12])");
    check(&mut it, "isequal(sscanf('0x1f 010', '%i'), [31; 8])");
    check(&mut it, "isequal(sscanf('123456', '%2d'), [12; 34; 56])");
    check(&mut it, "isequal(sscanf('1 2', '%*d %d'), 2)");
    check(&mut it, "isequal(size(sscanf('', '%d')), [0 1])");
    check(
        &mut it,
        "isequal(sscanf('12abc', '%d%[a-z]'), [12; 97; 98; 99])",
    );
    run(&mut it, "a = sscanf('Inf -nan 1e3', '%f');");
    check(&mut it, "isinf(a(1)) && isnan(a(2)) && a(3) == 1000");
}
