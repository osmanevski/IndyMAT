//! Snapshot tests: each feature from the parser contract, asserted through
//! `ast::sexpr`.

use indy_syntax::{Diagnostic, Dialect, ast, parse_file};

fn parse(src: &str, dialect: Dialect) -> String {
    let r = parse_file(src, dialect);
    assert!(r.ok(), "{src:?} failed: {:?}", r.diagnostics);
    ast::sexpr(r.file.as_ref().unwrap())
}

/// Octave dialect, body of a script file.
fn s(src: &str) -> String {
    let out = parse(src, Dialect::Octave);
    let body = out
        .strip_prefix("(file script ")
        .and_then(|b| b.strip_suffix(')'))
        .unwrap_or_else(|| panic!("not a script: {out}"));
    body.to_string()
}

/// MATLAB dialect, body of a script file.
fn m(src: &str) -> String {
    let out = parse(src, Dialect::Matlab);
    out.strip_prefix("(file script ")
        .and_then(|b| b.strip_suffix(')'))
        .unwrap_or_else(|| panic!("not a script: {out}"))
        .to_string()
}

/// Single expression `x = <src>;`, returns the rhs.
fn e(src: &str) -> String {
    let out = s(&format!("x = {src};"));
    out.strip_prefix("(= x ")
        .and_then(|b| b.strip_suffix(')'))
        .unwrap_or_else(|| panic!("unexpected shape: {out}"))
        .to_string()
}

fn err(src: &str, dialect: Dialect) -> Diagnostic {
    let r = parse_file(src, dialect);
    assert!(!r.ok(), "{src:?} unexpectedly parsed");
    assert!(r.file.is_none());
    r.diagnostics[0].clone()
}

// ----- comments ------------------------------------------------------------

#[test]
fn comments() {
    assert_eq!(
        s("% c\nx = 1; % tail\n# octave\ny = 2; # tail"),
        "(= x 1.0) (= y 2.0)"
    );
    assert_eq!(s("%{\nx = 1;\n%}\ny = 2;"), "(= y 2.0)");
    assert_eq!(s("#{\nx = 1;\n#}\ny = 2;"), "(= y 2.0)");
    // Nested block comments.
    assert_eq!(s("%{\n%{\nx\n%}\nstill comment\n%}\ny;"), "y");
    // `%{` not alone on its line is a line comment.
    assert_eq!(s("x = 1; %{ not a block\ny = 2;"), "(= x 1.0) (= y 2.0)");
    assert_eq!(s("  %{  \n x\n  %}\ny;"), "y");
    // Continuation: rest of the line is a comment.
    assert_eq!(s("x = 1 + ... comment here\n 2;"), "(= x (+ 1.0 2.0))");
    assert_eq!(m("x = 1; % only percent\n"), "(= x 1.0)");
    let d = err("x = 1; # hash\n", Dialect::Matlab);
    assert_eq!(d.code, "E0105-invalid-character");
    assert_eq!((d.line, d.column), (1, 8));
}

// ----- strings and transpose ---------------------------------------------

#[test]
fn strings_and_transpose() {
    assert_eq!(e("'it''s'"), r#"(char "it's")"#);
    assert_eq!(e("a'"), "(' a)");
    assert_eq!(e("a.'"), "(.' a)");
    assert_eq!(e("a''"), "(' (' a))");
    assert_eq!(e("a(1)'"), "(' (index a 1.0))");
    assert_eq!(e("[1 2]'"), "(' (matrix (row 1.0 2.0)))");
    assert_eq!(e("{1}'"), "(' (cell (row 1.0)))");
    assert_eq!(e("2'"), "(' 2.0)");
    assert_eq!(e("a.'' "), "(' (.' a))");
    assert_eq!(e("x(end)'"), "(' (index x end))");
    // After an operator or `(` it is a char literal.
    assert_eq!(e("f('a')"), r#"(index f (char "a"))"#);
    assert_eq!(e("1 + 'a'"), r#"(+ 1.0 (char "a"))"#);
    // In a matrix, `'` after whitespace starts a literal.
    assert_eq!(e("[a 'b']"), r#"(matrix (row a (char "b")))"#);
    assert_eq!(e("[a' b']"), "(matrix (row (' a) (' b)))");
    assert_eq!(e("[a', 'x']"), r#"(matrix (row (' a) (char "x")))"#);
    // Double quotes.
    assert_eq!(e(r#""a""b""#), r#"(str "a\"b")"#);
    assert_eq!(e(r#""a\tb\n""#), r#"(str "a\tb\n")"#);
    assert_eq!(e(r#""q\"x""#), r#"(str "q\"x")"#);
    assert_eq!(m(r#"x = "a\nb";"#), r#"(= x (str "a\\nb"))"#);
    assert_eq!(e(r#""\101\x42""#), r#"(str "AB")"#);
    assert_eq!(e("'a\\nb'"), r#"(char "a\\nb")"#);
    assert_eq!(e("\"ab\\\ncd\""), r#"(str "abcd")"#);
}

// ----- numbers -------------------------------------------------------------

#[test]
fn numbers() {
    assert_eq!(e("42"), "42.0");
    assert_eq!(e("3.25"), "3.25");
    assert_eq!(e(".5"), "0.5");
    assert_eq!(e("1."), "1.0");
    assert_eq!(e("1e3"), "1000.0");
    assert_eq!(e("2.5E-2"), "0.025");
    assert_eq!(e("1d2"), "100.0");
    assert_eq!(e("1i"), "(imag 1.0)");
    assert_eq!(e("2.5j"), "(imag 2.5)");
    assert_eq!(e("1e2i"), "(imag 100.0)");
    assert_eq!(e("3 + 4i"), "(+ 3.0 (imag 4.0))");
    assert_eq!(e("0x1F"), "(int 31)");
    assert_eq!(e("0b101"), "(int 5)");
    assert_eq!(e("0x2Au16"), "(int 42 u16)");
    assert_eq!(e("0b101010s16"), "(int 42 s16)");
    // `1./x` is element-wise division, not `1.` then `/`.
    assert_eq!(e("1./x"), "(./ 1.0 x)");
    assert_eq!(e("2.^x"), "(.^ 2.0 x)");
    assert_eq!(e("1.*x"), "(.* 1.0 x)");
    assert_eq!(e("1:3"), "(range 1.0 3.0)");
    assert_eq!(
        err("x = 0x1FFFFFFFFFFFFFFFFF;", Dialect::Octave).code,
        "E0107-invalid-number"
    );
}

// ----- matrices and cells --------------------------------------------------

#[test]
fn matrix_separation_rules() {
    assert_eq!(e("[1 2 3]"), "(matrix (row 1.0 2.0 3.0))");
    assert_eq!(e("[1, 2; 3, 4]"), "(matrix (row 1.0 2.0) (row 3.0 4.0))");
    assert_eq!(e("[1 2\n 3 4]"), "(matrix (row 1.0 2.0) (row 3.0 4.0))");
    assert_eq!(e("[1 -2]"), "(matrix (row 1.0 (neg 2.0)))");
    assert_eq!(e("[1 - 2]"), "(matrix (row (- 1.0 2.0)))");
    assert_eq!(e("[1-2]"), "(matrix (row (- 1.0 2.0)))");
    assert_eq!(e("[1 -2 + 3]"), "(matrix (row 1.0 (+ (neg 2.0) 3.0)))");
    assert_eq!(e("[a -b]"), "(matrix (row a (neg b)))");
    assert_eq!(e("[a - b]"), "(matrix (row (- a b)))");
    assert_eq!(e("[a- b]"), "(matrix (row (- a b)))");
    assert_eq!(e("[a +b]"), "(matrix (row a (uplus b)))");
    assert_eq!(e("[a ~b]"), "(matrix (row a (not b)))");
    assert_eq!(e("[a ~= b]"), "(matrix (row (~= a b)))");
    assert_eq!(e("[a (1)]"), "(matrix (row a (paren 1.0)))");
    assert_eq!(
        e("[a(1) b{2}]"),
        "(matrix (row (index a 1.0) (cellindex b 2.0)))"
    );
    assert_eq!(
        e("[f(1, -2) 3]"),
        "(matrix (row (index f 1.0 (neg 2.0)) 3.0))"
    );
    assert_eq!(e("[(1 -2) 3]"), "(matrix (row (paren (- 1.0 2.0)) 3.0))");
    assert_eq!(e("[a.b c]"), "(matrix (row (. a b) c))");
    assert_eq!(e("[1 : 3]"), "(matrix (row (range 1.0 3.0)))");
    assert_eq!(e("[]"), "(matrix)");
    assert_eq!(e("[ ]"), "(matrix)");
    assert_eq!(e("[1, 2, ]"), "(matrix (row 1.0 2.0))");
    assert_eq!(e("[1;2;]"), "(matrix (row 1.0) (row 2.0))");
    assert_eq!(
        e("[[1 2]; [3 4]]"),
        "(matrix (row (matrix (row 1.0 2.0))) (row (matrix (row 3.0 4.0))))"
    );
    assert_eq!(e("[1, 2, ... comment\n 3]"), "(matrix (row 1.0 2.0 3.0))");
    assert_eq!(e("[1 ...\n 2]"), "(matrix (row 1.0 2.0))");
    assert_eq!(
        e("[1 2 % c\n 3 4 # d\n]"),
        "(matrix (row 1.0 2.0) (row 3.0 4.0))"
    );
    assert_eq!(
        e("[\n 1 2\n\n 3 4\n]"),
        "(matrix (row 1.0 2.0) (row 3.0 4.0))"
    );
    assert_eq!(e("[x{:}]"), "(matrix (row (cellindex x :)))");
    assert_eq!(e("[@sin @cos]"), "(matrix (row (handle sin) (handle cos)))");
    assert_eq!(
        e("['ab' 'cd']"),
        r#"(matrix (row (char "ab") (char "cd")))"#
    );
}

#[test]
fn cells() {
    assert_eq!(e("{}"), "(cell)");
    assert_eq!(
        e("{1, 'a'; [2 3], {}}"),
        r#"(cell (row 1.0 (char "a")) (row (matrix (row 2.0 3.0)) (cell)))"#
    );
    assert_eq!(e("{a -b}"), "(cell (row a (neg b)))");
    assert_eq!(e("{a - b}"), "(cell (row (- a b)))");
    assert_eq!(
        e("{@(x) x + 1, 2}"),
        "(cell (row (lambda (x) (+ x 1.0)) 2.0))"
    );
    assert_eq!(
        e("{'a' 'b'\n 'c' 'd'}"),
        r#"(cell (row (char "a") (char "b")) (row (char "c") (char "d")))"#
    );
}

// ----- operators -----------------------------------------------------------

#[test]
fn precedence() {
    assert_eq!(e("a || b && c"), "(|| a (&& b c))");
    assert_eq!(e("a | b & c"), "(| a (& b c))");
    assert_eq!(e("a && b | c"), "(&& a (| b c))");
    assert_eq!(e("a == b < c"), "(< (== a b) c)");
    assert_eq!(e("a < b + c"), "(< a (+ b c))");
    assert_eq!(e("a + b * c"), "(+ a (* b c))");
    assert_eq!(e("a - b - c"), "(- (- a b) c)");
    assert_eq!(e("a / b \\ c"), "(\\ (/ a b) c)");
    assert_eq!(e("a .* b ./ c .\\ d"), "(.\\ (./ (.* a b) c) d)");
    assert_eq!(e("a ~= b"), "(~= a b)");
    assert_eq!(e("a != b"), "(~= a b)");
    assert_eq!(e("~a == b"), "(== (not a) b)");
    assert_eq!(e("!a"), "(not a)");
    assert_eq!(e("-a * b"), "(* (neg a) b)");
    assert_eq!(e("-2^2"), "(neg (^ 2.0 2.0))");
    assert_eq!(e("2^-1"), "(^ 2.0 (neg 1.0))");
    assert_eq!(e("2^-x'"), "(' (^ 2.0 (neg x)))");
    assert_eq!(e("a^b^c"), "(^ (^ a b) c)");
    assert_eq!(e("a.^2'"), "(' (.^ a 2.0))");
    assert_eq!(e("-a'"), "(neg (' a))");
    assert_eq!(e("~a'"), "(not (' a))");
    assert_eq!(e("a**2"), "(^ a 2.0)");
    assert_eq!(e("+a"), "(uplus a)");
    assert_eq!(e("--a"), "(neg (neg a))");
    assert_eq!(e("a * -b"), "(* a (neg b))");
}

#[test]
fn ranges() {
    assert_eq!(e("a:b"), "(range a b)");
    assert_eq!(e("a:s:b"), "(range a s b)");
    assert_eq!(e("1:n+1"), "(range 1.0 (+ n 1.0))");
    assert_eq!(e("a+1:2*b"), "(range (+ a 1.0) (* 2.0 b))");
    assert_eq!(e("x == 1:3"), "(== x (range 1.0 3.0))");
    assert_eq!(e("1:3 == x"), "(== (range 1.0 3.0) x)");
    assert_eq!(e("-1:-1:-5"), "(range (neg 1.0) (neg 1.0) (neg 5.0))");
    assert_eq!(e("x(:)"), "(index x :)");
    assert_eq!(e("x(:, 1)"), "(index x : 1.0)");
    assert_eq!(e("x(2:end)"), "(index x (range 2.0 end))");
}

// ----- end -----------------------------------------------------------------

#[test]
fn end_in_index() {
    assert_eq!(e("a(end)"), "(index a end)");
    assert_eq!(e("a(end-1, end)"), "(index a (- end 1.0) end)");
    assert_eq!(e("a(b(end))"), "(index a (index b end))");
    assert_eq!(e("x{end}(end)"), "(index (cellindex x end) end)");
    assert_eq!(e("a([1 end])"), "(index a (matrix (row 1.0 end)))");
    assert_eq!(e("a((end))"), "(index a (paren end))");
    assert_eq!(s("x(end+1) = 5;"), "(= (index x (+ end 1.0)) 5.0)");
    assert_eq!(s("if a(end), b; end"), "(if ((index a end) (block b)))");
    // `end` outside any index closes blocks, and is an error at top level.
    assert_eq!(
        err("x = end;", Dialect::Octave).code,
        "E0101-unexpected-token"
    );
    assert_eq!(err("end", Dialect::Octave).code, "E0103-unbalanced-block");
}

#[test]
fn octave_block_closers() {
    let src = "if a\n b;\nendif\nfor k = 1:3\nendfor\nwhile c\nendwhile\nswitch x\n case 1\nendswitch\n\
               try\n y;\nend_try_catch\nunwind_protect\n z;\nunwind_protect_cleanup\n w;\nend_unwind_protect\n\
               parfor i = 1:2\nendparfor";
    assert_eq!(
        s(src),
        "(if (a (block b))) (for k (range 1.0 3.0) (block)) (while c (block)) \
         (switch x (case 1.0 (block))) (try (block y)) \
         (unwind_protect (block z) (cleanup (block w))) (parfor i (range 1.0 2.0) (block))"
    );
    let d = err("if a\n b;\nendwhile", Dialect::Octave);
    assert_eq!(d.code, "E0103-unbalanced-block");
    assert_eq!((d.line, d.column), (3, 1));
    // MATLAB has no `endif`: it is a plain identifier there.
    assert_eq!(
        err("if a\n b;\nendif", Dialect::Matlab).code,
        "E0103-unbalanced-block"
    );
}

// ----- statements ----------------------------------------------------------

#[test]
fn assignments() {
    assert_eq!(s("x = 1"), "(show (= x 1.0))");
    assert_eq!(s("x = 1;"), "(= x 1.0)");
    assert_eq!(s("x = 1, y = 2;"), "(show (= x 1.0)) (= y 2.0)");
    assert_eq!(s("x"), "(show x)");
    assert_eq!(s("[a, b] = f(1);"), "(= [a b] (index f 1.0))");
    assert_eq!(s("[a, ~, c] = f(x);"), "(= [a ~ c] (index f x))");
    assert_eq!(s("[a b] = deal(1, 2);"), "(= [a b] (index deal 1.0 2.0))");
    assert_eq!(s("[~] = f();"), "(= [~] (index f))");
    assert_eq!(s("[s.a, c{2}] = f;"), "(= [(. s a) (cellindex c 2.0)] f)");
    assert_eq!(s("s.a.b = 1;"), "(= (. (. s a) b) 1.0)");
    assert_eq!(s("s.(name) = 1;"), "(= (.() s name) 1.0)");
    assert_eq!(
        s("s.(['a' 'b'])(2) = 1;"),
        r#"(= (index (.() s (matrix (row (char "a") (char "b")))) 2.0) 1.0)"#
    );
    assert_eq!(s("x(:) = [];"), "(= (index x :) (matrix))");
    assert_eq!(
        s("c{3}.f(2) = [];"),
        "(= (index (. (cellindex c 3.0) f) 2.0) (matrix))"
    );
    assert_eq!(s("[1 2]"), "(show (matrix (row 1.0 2.0)))");
    assert_eq!(
        err("f(1) + 1 = 2;", Dialect::Octave).code,
        "E0110-invalid-assignment-target"
    );
    assert_eq!(
        err("[a, 1] = f;", Dialect::Octave).code,
        "E0110-invalid-assignment-target"
    );
}

#[test]
fn octave_assignment_operators() {
    assert_eq!(s("x += 1;"), "(+= x 1.0)");
    assert_eq!(s("x -= 1;"), "(-= x 1.0)");
    assert_eq!(s("x *= 2;"), "(*= x 2.0)");
    assert_eq!(s("x /= 2;"), "(/= x 2.0)");
    assert_eq!(s("x ^= 2;"), "(^= x 2.0)");
    assert_eq!(s("x .*= w;"), "(.*= x w)");
    assert_eq!(s("k &= y > 0;"), "(&= k (> y 0.0))");
    assert_eq!(s("k |= z;"), "(|= k z)");
    assert_eq!(s("a(2) += 3"), "(show (+= (index a 2.0) 3.0))");
    assert_eq!(s("x++;"), "(++ x)");
    assert_eq!(s("x--;"), "(-- x)");
    assert_eq!(s("a(k++) = 1;"), "(= (index a (post++ k)) 1.0)");
    assert_eq!(s("a = b = 0;"), "(= a (= b 0.0))");
    assert_eq!(s("a = b.c = d = [];"), "(= a (= (. b c) (= d (matrix))))");
    assert_eq!(
        s("(n = numel (x)) || (n = 1);"),
        "(|| (paren (= n (index numel x))) (paren (= n 1.0)))"
    );
    assert_eq!(
        s("if (L = numel (v)) end"),
        "(if ((paren (= L (index numel v))) (block)))"
    );
    assert_eq!(e("f (w ./= 2)"), "(index f (./= w 2.0))");
    // Not MATLAB.
    // (Without a prior assignment MATLAB reads `x += 1` as command syntax.)
    assert!(!parse_file("x = 0; x += 1;", Dialect::Matlab).ok());
    assert_eq!(m("x += 1"), r#"(show (command x "+=" "1"))"#);
    assert!(!parse_file("a = b = 0;", Dialect::Matlab).ok());
    // `a--b` stays a subtraction.
    assert_eq!(e("a--b"), "(- a (neg b))");
}

#[test]
fn control_flow() {
    assert_eq!(
        s("if a\n x;\nelseif b\n y;\nelse\n z;\nend"),
        "(if (a (block x)) (b (block y)) (else (block z)))"
    );
    assert_eq!(s("if a, x; end"), "(if (a (block x)))");
    assert_eq!(s("if (a) x = 1; end"), "(if ((paren a) (block (= x 1.0))))");
    assert_eq!(
        s("if a\nelse if b\n end\nend"),
        "(if (a (block)) (else (block (if (b (block))))))"
    );
    assert_eq!(
        s("for k = 1:n\n disp(k);\nend"),
        "(for k (range 1.0 n) (block (index disp k)))"
    );
    assert_eq!(
        s("for (k = 1:n)\n x;\nend"),
        "(for k (range 1.0 n) (block x))"
    );
    assert_eq!(
        s("for k = [1 2 3], x; end"),
        "(for k (matrix (row 1.0 2.0 3.0)) (block x))"
    );
    assert_eq!(s("for [v, k] = st\n x;\nend"), "(for [v k] st (block x))");
    assert_eq!(
        s("parfor i = 1:4\n x;\nend"),
        "(parfor i (range 1.0 4.0) (block x))"
    );
    assert_eq!(
        s("parfor (i = 1:4, 2)\n x;\nend"),
        "(parfor i (range 1.0 4.0) (workers 2.0) (block x))"
    );
    assert_eq!(
        s("while x > 0\n x = x - 1;\nend"),
        "(while (> x 0.0) (block (= x (- x 1.0))))"
    );
    assert_eq!(s("do\n x++;\nuntil x > 3"), "(do (block (++ x)) (> x 3.0))");
    assert_eq!(
        s("switch v\n case 1\n  a;\n case {2, 3}\n  b;\n otherwise\n  c;\nend"),
        "(switch v (case 1.0 (block a)) (case (cell (row 2.0 3.0)) (block b)) (otherwise (block c)))"
    );
    assert_eq!(
        s("switch s\n case 'x', a;\nend"),
        r#"(switch s (case (char "x") (block a)))"#
    );
    assert_eq!(
        s("try\n a;\ncatch err\n b;\nend"),
        "(try (block a) (catch err (block b)))"
    );
    assert_eq!(
        s("try\n a;\ncatch\n b;\nend"),
        "(try (block a) (catch (block b)))"
    );
    assert_eq!(
        s("try, a; catch, b; end"),
        "(try (block a) (catch (block b)))"
    );
    assert_eq!(
        s("try\n a;\ncatch disp(1)\nend"),
        "(try (block a) (catch (block (show (index disp 1.0)))))"
    );
    assert_eq!(s("try\n a;\nend"), "(try (block a))");
    assert_eq!(
        s("unwind_protect\n a;\nunwind_protect_cleanup\n b;\nend"),
        "(unwind_protect (block a) (cleanup (block b)))"
    );
    assert_eq!(
        s("for k=1:3, if k, break; else, continue; end, end"),
        "(for k (range 1.0 3.0) (block (if (k (block (break))) (else (block (continue))))))"
    );
    assert_eq!(s("return;"), "(return)");
    assert_eq!(s("global a b\npersistent n"), "(global a b) (persistent n)");
    assert_eq!(s("global g = 3"), "(global (= g 3.0))");
    assert_eq!(s("spmd\n x;\nend"), "(spmd (block x))");
}

// ----- command syntax ------------------------------------------------------

#[test]
fn command_syntax() {
    assert_eq!(s("hold on"), r#"(show (command hold "on"))"#);
    assert_eq!(s("hold on;"), r#"(command hold "on")"#);
    assert_eq!(s("format long"), r#"(show (command format "long"))"#);
    assert_eq!(s("warning off all;"), r#"(command warning "off" "all")"#);
    assert_eq!(
        s("pkg load signal"),
        r#"(show (command pkg "load" "signal"))"#
    );
    assert_eq!(
        s("clear all; close all"),
        r#"(command clear "all") (show (command close "all"))"#
    );
    assert_eq!(s("disp hello"), r#"(show (command disp "hello"))"#);
    assert_eq!(
        s("disp 'hello world'"),
        r#"(show (command disp "hello world"))"#
    );
    assert_eq!(s("disp it''s"), r#"(show (command disp "its"))"#);
    assert_eq!(s("disp a'b c'd"), r#"(show (command disp "ab cd"))"#);
    assert_eq!(
        err("disp it's", Dialect::Octave).code,
        "E0102-unterminated-string"
    );
    assert_eq!(s("ls -l"), r#"(show (command ls "-l"))"#);
    assert_eq!(
        s("hold on % comment\nx;"),
        r#"(show (command hold "on")) x"#
    );
    assert_eq!(
        s("axis equal tight, grid on"),
        r#"(show (command axis "equal" "tight")) (show (command grid "on"))"#
    );
    assert_eq!(s("import pkg.*"), r#"(show (command import "pkg.*"))"#);
    // Expressions, not commands.
    assert_eq!(s("a - b"), "(show (- a b))");
    assert_eq!(s("a == b"), "(show (== a b))");
    assert_eq!(s("disp (x)"), "(show (index disp x))");
    assert_eq!(s("x = 3;\nx -1"), "(= x 3.0) (show (- x 1.0))");
    assert_eq!(s("x = 3;\nx '"), "(= x 3.0) (show (' x))");
    assert_eq!(s("x += 1;"), "(+= x 1.0)");
    assert_eq!(s("a"), "(show a)");
    assert_eq!(s("if a, b end"), "(if (a (block (show b))))");
    // Without a prior assignment `a -b` is a command, as in MATLAB.
    assert_eq!(s("a -b"), r#"(show (command a "-b"))"#);
    // Function parameters count as variables.
    assert_eq!(
        parse("function f(a)\n a -1\nend", Dialect::Octave),
        "(file function (function f () (a) (block (show (- a 1.0)))))"
    );
    // Commands do not disturb the lexer state of the next line.
    assert_eq!(
        s("disp [\nx = 1;"),
        r#"(show (command disp "[")) (= x 1.0)"#
    );
}

// ----- functions -----------------------------------------------------------

#[test]
fn function_files() {
    assert_eq!(
        parse("function y = f(x)\n y = x;\nend", Dialect::Octave),
        "(file function (function f (y) (x) (block (= y x))))"
    );
    assert_eq!(
        parse(
            "function [a, b] = f(x, varargin)\n a = 1; b = 2;\nendfunction",
            Dialect::Octave
        ),
        "(file function (function f (a b) (x varargin) (block (= a 1.0) (= b 2.0))))"
    );
    assert_eq!(
        parse("function f\n x;\n", Dialect::Matlab),
        "(file function (function f () () (block x) noend))"
    );
    assert_eq!(
        parse("function varargout = f(~, b)\nend", Dialect::Matlab),
        "(file function (function f (varargout) (~ b) (block)))"
    );
    // Several functions without `end`.
    assert_eq!(
        parse("function a\n x;\nfunction b\n y;\n", Dialect::Matlab),
        "(file function (function a () () (block x) noend) (function b () () (block y) noend))"
    );
    // Several functions with `end`, one nested.
    assert_eq!(
        parse(
            "function a\n x;\n function b\n  y;\n end\nend\nfunction c\nend",
            Dialect::Matlab
        ),
        "(file function (function a () () (block x) (function b () () (block y))) (function c () () (block)))"
    );
    // Octave default parameter values.
    assert_eq!(
        parse("function f (x, tol = 1e-6)\nendfunction", Dialect::Octave),
        "(file function (function f () (x (= tol 1e-6)) (block)))"
    );
    // Overloaded `end` method.
    assert_eq!(
        parse("function n = end (obj, k, n)\nend", Dialect::Octave),
        "(file function (function end (n) (obj k n) (block)))"
    );
    let d = err("function f\n if x\n", Dialect::Octave);
    assert_eq!(d.code, "E0103-unbalanced-block");
}

#[test]
fn script_with_local_functions() {
    assert_eq!(
        s("x = g(2);\nfunction y = g(a)\n y = a;\nend"),
        "(= x (index g 2.0)) (function g (y) (a) (block (= y a)))"
    );
    assert_eq!(
        s("1;\nfunction h\nendfunction\nh();"),
        "1.0 (function h () () (block)) (index h)"
    );
}

#[test]
fn anonymous_functions_and_handles() {
    assert_eq!(e("@(x) x.^2"), "(lambda (x) (.^ x 2.0))");
    assert_eq!(e("@() disp(1)"), "(lambda () (index disp 1.0))");
    assert_eq!(e("@(x, y) x + y"), "(lambda (x y) (+ x y))");
    assert_eq!(
        e("@(varargin) f(varargin{:})"),
        "(lambda (varargin) (index f (cellindex varargin :)))"
    );
    assert_eq!(e("@(~) 1"), "(lambda (~) 1.0)");
    assert_eq!(e("@sin"), "(handle sin)");
    assert_eq!(e("@obj.method"), "(handle obj.method)");
    assert_eq!(e("@pkg.sub.fn"), "(handle pkg.sub.fn)");
    assert_eq!(
        e("cellfun(@(c) c * 2, x, 'UniformOutput', false)"),
        r#"(index cellfun (lambda (c) (* c 2.0)) x (char "UniformOutput") false)"#
    );
    assert_eq!(e("@(x) @(y) x + y"), "(lambda (x) (lambda (y) (+ x y)))");
}

#[test]
fn calls_fields_and_metaclass() {
    assert_eq!(
        e("s.a(1).b{2}"),
        "(cellindex (. (index (. s a) 1.0) b) 2.0)"
    );
    assert_eq!(e("s.end"), "(. s end)");
    assert_eq!(e("obj.properties"), "(. obj properties)");
    assert_eq!(e("f()"), "(index f)");
    assert_eq!(
        e("plot(x, y, LineWidth=2)"),
        "(index plot x y (named LineWidth 2.0))"
    );
    assert_eq!(e("?MyClass"), "(metaclass MyClass)");
    assert_eq!(e("?pkg.MyClass"), "(metaclass pkg.MyClass)");
    assert_eq!(
        s("obj = obj@Super(a, b);"),
        "(= obj (index (super obj Super) a b))"
    );
    assert_eq!(
        s("disp@pkg.Base(obj);"),
        "(index (super disp pkg.Base) obj)"
    );
    assert_eq!(e("f(x)(2)"), "(index (index f x) 2.0)");
    assert_eq!(e("a'(:, 1)"), "(index (' a) : 1.0)");
    assert_eq!(e("f(1,\n 2)"), "(index f 1.0 2.0)");
}

// ----- arguments blocks ----------------------------------------------------

#[test]
fn arguments_blocks() {
    let src = "function y = f(x, opts)\n\
               arguments\n\
                 x (1,:) double {mustBePositive, mustBeFinite} = 1\n\
                 opts.Scale (1,1) double = 2\n\
                 opts.Label\n\
               end\n\
               y = x;\n\
               end";
    assert_eq!(
        parse(src, Dialect::Matlab),
        "(file function (function f (y) (x opts) (arguments \
         (x (size 1.0 :) (class double) (validators mustBePositive mustBeFinite) (default 1.0)) \
         (opts.Scale (size 1.0 1.0) (class double) (default 2.0)) (opts.Label)) (block (= y x))))"
    );
    let src = "function varargout = g(x)\narguments (Input)\n x\nend\narguments (Output, Repeating)\n varargout (1,1) double\nend\nend";
    assert_eq!(
        parse(src, Dialect::Matlab),
        "(file function (function g (varargout) (x) (arguments (attrs Input) (x)) \
         (arguments (attrs Output Repeating) (varargout (size 1.0 1.0) (class double))) (block)))"
    );
    let src = "function f(x)\narguments\n x {mustBeMember(x, [\"a\" \"b\"])}\nend\nend";
    assert_eq!(
        parse(src, Dialect::Matlab),
        r#"(file function (function f () (x) (arguments (x (validators (index mustBeMember x (matrix (row (str "a") (str "b"))))))) (block)))"#
    );
    // `arguments` is an ordinary name elsewhere.
    assert_eq!(s("arguments = 3;"), "(= arguments 3.0)");
}

// ----- classdef ------------------------------------------------------------

#[test]
fn classdef_files() {
    let src = "classdef (Sealed) Foo < handle & matlab.mixin.Copyable\n\
               properties (Access = private, Constant)\n\
                 A = 1;\n\
                 B (1,1) double {mustBePositive} = 2\n\
                 C\n\
               end\n\
               properties\n\
                 D matlab.lang.OnOffSwitchState = 'on'\n\
               end\n\
               methods\n\
                 function obj = Foo(x)\n\
                   obj = obj@handle();\n\
                 end\n\
                 function v = get.A(obj)\n\
                   v = obj.A;\n\
                 end\n\
               end\n\
               methods (Abstract)\n\
                 y = measure(obj)\n\
                 [a, b] = pair(obj, k)\n\
                 reset(obj)\n\
               end\n\
               methods (Static, Access = {?Bar, ?pkg.Baz})\n\
                 r = make()\n\
               end\n\
               events (ListenAccess = protected)\n\
                 Changed\n\
                 Deleted\n\
               end\n\
               end";
    assert_eq!(
        parse(src, Dialect::Matlab),
        "(file classdef (classdef Foo (attrs Sealed) (< handle matlab.mixin.Copyable) \
         (properties (attrs (= Access private) Constant) (A (default 1.0)) \
         (B (size 1.0 1.0) (class double) (validators mustBePositive) (default 2.0)) (C)) \
         (properties (D (class matlab.lang.OnOffSwitchState) (default (char \"on\")))) \
         (methods (function Foo (obj) (x) (block (= obj (index (super obj handle))))) \
         (function get.A (v) (obj) (block (= v (. obj A))))) \
         (methods (attrs Abstract) (declare measure (y) (obj)) (declare pair (a b) (obj k)) (declare reset () (obj))) \
         (methods (attrs Static (= Access (cell (row (metaclass Bar) (metaclass pkg.Baz))))) (declare make (r) ())) \
         (events (attrs (= ListenAccess protected)) Changed Deleted)))"
    );
    assert_eq!(
        parse(
            "classdef Color\n enumeration\n  Red, Blue\n  Green (0, 1, 0)\n end\nend",
            Dialect::Matlab
        ),
        "(file classdef (classdef Color (enumeration Red Blue (Green 0.0 1.0 0.0))))"
    );
    assert_eq!(
        parse(
            "classdef A < handle\n properties\n  x = 1;\n endproperties\n methods\n  function f(o)\n  endfunction\n endmethods\nendclassdef",
            Dialect::Octave
        ),
        "(file classdef (classdef A (< handle) (properties (x (default 1.0))) (methods (function f () (o) (block)))))"
    );
    // A local function after the classdef.
    assert_eq!(
        parse("classdef A\nend\nfunction helper\nend", Dialect::Matlab),
        "(file classdef (classdef A) (function helper () () (block)))"
    );
    // A method may call a function named `methods` / `properties`.
    assert_eq!(
        parse(
            "classdef A\n methods\n  function f(o)\n   methods(o);\n  end\n end\nend",
            Dialect::Matlab
        ),
        "(file classdef (classdef A (methods (function f () (o) (block (index methods o))))))"
    );
    assert_eq!(
        err("classdef A\n properties\n  x\n", Dialect::Matlab).code,
        "E0103-unbalanced-block"
    );
}

// ----- dialect -------------------------------------------------------------

#[test]
fn matlab_dialect_rejects_octave_only_syntax() {
    for src in [
        "x = !a;",
        "x = a != b;",
        "x = 0; x++;",
        "unwind_protect\n x;\nunwind_protect_cleanup\nend",
        "# c\n",
    ] {
        assert!(
            !parse_file(src, Dialect::Matlab).ok(),
            "{src:?} parsed in MATLAB mode"
        );
        assert!(
            parse_file(src, Dialect::Octave).ok(),
            "{src:?} failed in Octave mode"
        );
    }
    // Octave-only keywords are identifiers in MATLAB.
    assert_eq!(m("do = 1;"), "(= do 1.0)");
    assert_eq!(
        m("do\n x;\nuntil y"),
        r#"(show do) x (show (command until "y"))"#
    );
}
