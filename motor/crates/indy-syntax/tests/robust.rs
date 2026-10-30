//! `parse_file` must never panic and must report hostile input as a
//! diagnostic with a stable code and a correct position.

use indy_syntax::{Dialect, parse_file};

fn code(src: &str) -> &'static str {
    let r = parse_file(src, Dialect::Octave);
    assert!(!r.ok(), "{src:?} unexpectedly parsed");
    r.diagnostics[0].code
}

#[test]
fn empty_and_trivial_files() {
    for src in [
        "",
        "\n",
        "   ",
        "%",
        "% only a comment",
        ";",
        ",,;\n;",
        "%{\n%}",
    ] {
        let r = parse_file(src, Dialect::Octave);
        assert!(r.ok(), "{src:?}: {:?}", r.diagnostics);
        assert!(r.diagnostics.is_empty());
    }
}

#[test]
fn error_codes_and_positions() {
    assert_eq!(code("x = 'abc"), "E0102-unterminated-string");
    assert_eq!(code("x = \"abc"), "E0102-unterminated-string");
    assert_eq!(code("'"), "E0102-unterminated-string");
    assert_eq!(code("x = [1 2"), "E0109-unclosed-bracket");
    assert_eq!(code("x = {1"), "E0109-unclosed-bracket");
    assert_eq!(code("x = (1"), "E0108-unexpected-eof");
    assert_eq!(code("x = 1)"), "E0101-unexpected-token");
    assert_eq!(code("x = [1 2)"), "E0101-unexpected-token");
    assert_eq!(code("if x\n y;\n"), "E0103-unbalanced-block");
    assert_eq!(code("end"), "E0103-unbalanced-block");
    assert_eq!(code("x = 1 +"), "E0108-unexpected-eof");
    assert_eq!(code("x = $"), "E0105-invalid-character");
    assert_eq!(code("x = ü;"), "E0105-invalid-character");
    assert_eq!(code("%{\nnever closed"), "E0106-unterminated-block-comment");
    assert_eq!(code("x = [1,,2];"), "E0101-unexpected-token");
    assert_eq!(code("x = [2pi];"), "E0101-unexpected-token");
    assert_eq!(code("x = 1 2;"), "E0101-unexpected-token");
    assert_eq!(code("1 = x;"), "E0110-invalid-assignment-target");

    let r = parse_file("a = 1;\nb = 'oops\nc = 3;", Dialect::Octave);
    let d = &r.diagnostics[0];
    assert_eq!(
        (d.line, d.column, d.code),
        (2, 5, "E0102-unterminated-string")
    );
    assert_eq!(d.span.start, 11);

    // Columns count characters, not bytes.
    let r = parse_file("s = 'çğ'; x = )", Dialect::Octave);
    let d = &r.diagnostics[0];
    assert_eq!((d.line, d.column), (1, 15));

    let r = parse_file("function f\n if x\n  y;\n", Dialect::Octave);
    let d = &r.diagnostics[0];
    assert_eq!(d.code, "E0103-unbalanced-block");
    assert!(d.message.contains("line 2"), "{}", d.message);
}

#[test]
fn deep_nesting_is_a_diagnostic_not_a_stack_overflow() {
    for (open, close) in [("(", ")"), ("[", "]"), ("{", "}"), ("-", ""), ("@() ", "")] {
        let src = format!("x = {}1{};", open.repeat(10_000), close.repeat(10_000));
        assert_eq!(code(&src), "E0104-nesting-too-deep", "{open}");
    }
    let src = "if x\n".repeat(10_000);
    assert_eq!(code(&src), "E0104-nesting-too-deep");
    let src = "a(".repeat(10_000);
    assert_eq!(code(&src), "E0104-nesting-too-deep");
    // Many unterminated functions are siblings, not nesting.
    let src = "function f\n".repeat(10_000);
    assert!(parse_file(&src, Dialect::Octave).ok());
    // Moderate nesting is fine.
    let src = format!("x = {}1{};", "(".repeat(60), ")".repeat(60));
    assert!(parse_file(&src, Dialect::Octave).ok());
}

/// Deterministic pseudo-random fragments: every prefix and many
/// recombinations of tricky snippets must parse or fail without panicking.
#[test]
fn fuzz_ish_no_panic() {
    let pieces = [
        "x",
        " ",
        "=",
        "'",
        "\"",
        "[",
        "]",
        "{",
        "}",
        "(",
        ")",
        ";",
        ",",
        "\n",
        "%",
        "#",
        "%{\n",
        "\n%}\n",
        "...",
        "end",
        "if",
        "function",
        "classdef",
        "@",
        "?",
        ".",
        ".'",
        "1e",
        "0x",
        "0b2",
        "~",
        "!",
        "+",
        "++",
        "-=",
        ":",
        "\\",
        "\\\n",
        "ü",
        "\r\n",
        "\t",
        "arguments",
        "properties",
        "methods",
        "hold on",
        "1i",
        "&&",
        "a(end",
        "catch e",
        "for [a,b] =",
    ];
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for _ in 0..4000 {
        let len = (next() % 24) as usize;
        let src: String = (0..len)
            .map(|_| pieces[(next() % pieces.len() as u64) as usize])
            .collect();
        for dialect in [Dialect::Octave, Dialect::Matlab] {
            let r = parse_file(&src, dialect);
            assert_eq!(r.ok(), r.diagnostics.is_empty());
            if let Some(d) = r.diagnostics.first() {
                assert!(d.line >= 1 && d.column >= 1);
                assert!(d.span.start as usize <= src.len());
            }
        }
    }
    let sample = "classdef (Sealed) A < handle\n properties\n  x (1,:) double = [1 -2]';\n end\n \
                  methods\n  function o = A(v)\n   o.x(end+1) = v{end}; % c\n  end\n end\nend\n";
    for i in 0..=sample.len() {
        if sample.is_char_boundary(i) {
            let _ = parse_file(&sample[..i], Dialect::Octave);
            let _ = parse_file(&sample[i..], Dialect::Matlab);
        }
    }
}
