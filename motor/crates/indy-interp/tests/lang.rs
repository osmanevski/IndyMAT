//! Language-core tests: each snippet is MATLAB code that must run without
//! error (its own `assert`s check the semantics).

use indy_interp::Interp;

fn ok(code: &str) {
    let mut it = Interp::new();
    if let Err(e) = it.run_text(code) {
        panic!("snippet failed: {}\n---\n{}", e.message, code);
    }
}

fn fails_with(code: &str, id: &str) {
    let mut it = Interp::new();
    match it.run_text(code) {
        Ok(_) => panic!("expected error {} from:\n{}", id, code),
        Err(e) => assert_eq!(e.identifier, id, "message: {}", e.message),
    }
}

#[test]
fn indexing_and_end() {
    ok("A = magic(4); assert(A(end, end) == 1); assert(isequal(A(2, :), [5 11 10 8]));
        assert(isequal(A(end), A(16))); B = A; B(:, 2) = []; assert(isequal(size(B), [4 3]));
        x = 1:5; x(end+1) = 6; x([1 end]) = []; assert(isequal(x, 2:5));
        v = []; v(3) = 7; assert(isequal(v, [0 0 7]));
        M = zeros(2); M(3, 4) = 1; assert(isequal(size(M), [3 4]) && M(3, 4) == 1);
        c = {1, 'a', [2 3]}; assert(c{end}(end) == 3);
        y = 10:-2:1; assert(isequal(y(end:-1:1), [2 4 6 8 10]));
        L = A > 10; assert(isequal(A(L)', [16 11 14 15 13 12]));
        assert(isequal(magic(3), [8 1 6; 3 5 7; 4 9 2])); assert(isequal(magic(4), [16 2 3 13; 5 11 10 8; 9 7 6 12; 4 14 15 1]));
        assert(isequal(sum(magic(6)), 111 * ones(1, 6)) && isequal(sum(magic(6), 2), 111 * ones(6, 1)));
        z = reshape(1:24, 2, 3, 4); assert(z(2, 3, 4) == 24 && isequal(size(z(:, :, 2)), [2 3]));
        w = 1:10; w(w > 5) = 0; assert(sum(w) == 15);");
}

#[test]
fn empty_rules() {
    ok(
        "assert(sum([]) == 0 && prod([]) == 1); assert(~any([]) && all([]));
        assert(isequal(size(sum(zeros(0, 3))), [1 3])); assert(isempty(max([])));
        assert(isnan(mean([]))); assert(isequal(size(zeros(0, 3)'), [3 0]));
        e = ones(2, 0); assert(isempty(e) && size(e, 2) == 0);
        assert(isequal([[] 1 2], [1 2]));",
    );
}

#[test]
fn structs_and_cs_lists() {
    ok(
        "s = struct('a', {1, 2, 3}); assert(isequal([s.a], [1 2 3]));
        [s.a] = deal(0); assert(all([s.a] == 0));
        t.x.y(2).z = 5; assert(t.x.y(2).z == 5 && isempty(t.x.y(1).z));
        c = {1, 2, 3}; m = max([c{:}]); assert(m == 3);
        [p, q] = c{1:2}; assert(p == 1 && q == 2);
        f = 'b'; u.(f) = 4; assert(u.b == 4 && isfield(u, 'b'));
        r = rmfield(u, 'b'); assert(~isfield(r, 'b'));
        k = fieldnames(struct('z', 1, 'a', 2)); assert(isequal(k, {'z'; 'a'}));
        q2 = struct('v', {}); assert(isempty(q2)); q2(2).v = 1; assert(numel(q2) == 2);",
    );
}

#[test]
fn functions_and_handles() {
    ok("f = @(x) x.^2; assert(f(3) == 9); g = @(varargin) numel(varargin); assert(g(1, 2, 3) == 3);
        a = 2; h = @(x) a * x; a = 10; assert(h(3) == 6);
        k = @plus; assert(k(1, 2) == 3); assert(strcmp(func2str(f), '@(x)x.^2'));
        [m, i] = max([4 9 1]); assert(m == 9 && i == 2);
        n = cellfun(@numel, {'ab', 'c'}); assert(isequal(n, [2 1]));
        [~, idx] = sort([3 1 2]); assert(isequal(idx, [2 3 1]));
        z = arrayfun(@(t) t * 2, [1 2 3]); assert(isequal(z, [2 4 6]));
        w = cellfun(@(s) [s 'x'], {'a', 'b'}, 'UniformOutput', false); assert(isequal(w, {'ax', 'bx'}));");
}

#[test]
fn control_flow_and_errors() {
    ok("t = 0; for k = 1:10, if k == 3, continue; end, if k > 5, break; end, t = t + k; end, assert(t == 12);
        n = 0; while n < 5, n = n + 1; end, assert(n == 5);
        switch 'b', case {'a', 'b'}, r = 1; otherwise, r = 2; end, assert(r == 1);
        switch 3, case 1, q = 0; case 3, q = 3; end, assert(q == 3);
        try, error('My:id', 'value %d', 4); catch e, assert(strcmp(e.identifier, 'My:id') && strcmp(e.message, 'value 4')); end
        try, x = [1 2 3]; x(5); catch e2, assert(strcmp(e2.identifier, 'MATLAB:badsubscript')); end
        try, undefined_thing_xyz(1); catch e3, assert(~isempty(e3.message)); end
        ME = MException('A:b', 'msg %s', 'x'); try, throw(ME); catch e4, assert(strcmp(e4.message, 'msg x')); end
        for c = {1, 'two'}, assert(iscell(c)); end
        for col = [1 2; 3 4], assert(numel(col) == 2); end");
    fails_with("error('Pkg:thing', 'boom')", "Pkg:thing");
    fails_with("x = [1 2]; y = x(3);", "MATLAB:badsubscript");
    fails_with("[1 2] + [1 2 3]", "MATLAB:sizeDimensionsMustMatch");
}

#[test]
fn operators_and_types() {
    ok("assert(isequal([1; 2; 3] + [10 20], [11 21; 12 22; 13 23]));
        assert(isequal([1 2; 3 4] * [1; 1], [3; 7])); x = [2 1; 1 3] \\ [3; 5]; assert(abs(x(1) - 0.8) < 1e-12);
        assert(isa(int8(200), 'int8') && int8(200) == 127); assert(uint8(3) / uint8(2) == 2);
        assert(class(true + true) == \"double\" | true); assert(islogical(~[1 0]));
        z = sqrt(-4); assert(imag(z) == 2); assert(abs((1 + 2i) * (3 - 1i) - (5 + 5i)) < 1e-12);
        assert(strcmp(['ab' 'cd'], 'abcd')); assert(isequal('a' + 1, 98));
        assert(xor(true, false) && ~xor(true, true)); assert(mod(-1, 3) == 2 && rem(-1, 3) == -1);
        assert(isequal(1:0, zeros(1, 0)));");
}

#[test]
fn strings_and_text() {
    ok("s = \"abc\"; assert(isstring(s) && strlength(s) == 3 && isequal(size(s), [1 1]));
        t = [\"a\" \"bc\"]; assert(isequal(size(t), [1 2]) && t(2) == \"bc\");
        u = s + \"d\"; assert(u == \"abcd\"); assert(s + 1 == \"abc1\");
        assert(strcmp(char(t(2)), 'bc')); assert(isequal(split(\"a,b\", \",\"), [\"a\"; \"b\"]));
        assert(join([\"x\" \"y\"], \"-\") == \"x-y\"); assert(sprintf('%s-%d', 'a', 3) == \"a-3\");
        assert(isequal(strsplit('a b', ' '), {'a', 'b'})); assert(strcmp(strjoin({'a', 'b'}, '+'), 'a+b'));
        assert(strcmp(num2str(pi), '3.1416') && strcmp(num2str(42), '42'));
        assert(strcmp(mat2str([1 2; 3 4]), '[1 2;3 4]')); assert(str2double('2.5') == 2.5);
        assert(strcmp(upper('ab'), 'AB') && strcmp(strtrim('  x '), 'x'));");
}

#[test]
fn classdef_like_objects() {
    ok(
        "m = containers.Map(); m('a') = 1; m('b') = 2; assert(m.Count == 2 && isKey(m, 'a'));
        assert(isequal(keys(m), {'a', 'b'}) && isequal(values(m), {1, 2}));
        p = inputParser; addRequired(p, 'x'); addParameter(p, 'Scale', 1); parse(p, 5, 'Scale', 3);
        assert(p.Results.x == 5 && p.Results.Scale == 3);",
    );
}

#[test]
fn display_does_not_fail() {
    ok(
        "x = 3\ny = [1 2; 3 4]\nz = 'txt'\nc = {1, 'a'}\ns.a = 1\nb = true\nq = \"str\"\ne = []\nf = @(t) t + 1",
    );
}

#[test]
fn never_panics_on_odd_input() {
    let snippets = [
        "x(",
        "[1 2",
        "a.b.c(3).d{2} = 1; a.b",
        "end",
        "x = 1; x{2}",
        "s = struct(); s(3).a",
        "f = @(x) x; f(1, 2, 3)",
        "[a, b] = size",
        "c = {}; c{:}",
        "x = 1:3; x(0)",
        "x = 1:3; x(1.5)",
        "zeros(-1)",
        "ones(2, 'nonsense')",
        "y = 'abc'; y.field",
        "q = @sin; q.x = 1",
        "error()",
        "repmat()",
        "reshape(1:6, 4, [])",
        "cell2mat({{1}})",
        "x = int8(1) + int16(1)",
        "m = containers.Map(); m(1)",
        "inv('a')",
        "1:1e12",
        "sprintf('%d %s')",
        "[c{1:2}] = deal(1)",
        "s.a = 1; s(2).b",
        "x = {}; x(2).y = 1",
    ];
    for code in snippets {
        let mut it = Interp::new();
        let r = indy_interp::catch_panic(|| it.run_text(code));
        if let Err(e) = &r {
            assert!(
                e.identifier != "Indy:internalError",
                "panic on {:?}: {}",
                code,
                e.message
            );
        }
    }
}
