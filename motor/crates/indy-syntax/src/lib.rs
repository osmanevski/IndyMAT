//! MATLAB + Octave dialect lexer and parser for the IndyMAT engine.
//!
//! FROZEN CONTRACT (29 Oct 2026). Everything in this file outside the bodies
//! of `parse_file` is shared with `indy-corpus`. Changing a public name or
//! signature here needs the orchestrator; the AST lives in `ast` and may grow
//! freely.

pub mod ast;
mod lexer;
mod parser;
mod token;

/// Byte offsets into the source, half-open `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub span: Span,
    /// 1-based line and column of `span.start`.
    pub line: u32,
    pub column: u32,
    /// Stable short code, e.g. "E0001-unterminated-string". Used for grouping
    /// in corpus reports, so keep codes stable once introduced.
    pub code: &'static str,
    pub message: String,
}

/// Which syntax to accept. `Octave` is a superset of `Matlab`: it also
/// accepts `#` comments, `!`/`!=`, `endif`/`endfor`/..., `unwind_protect`,
/// `++`/`+=`, double-quoted escapes and the other Octave extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Matlab,
    Octave,
}

#[derive(Debug, Clone)]
pub struct ParseResult {
    /// `Some` when the file parsed without errors.
    pub file: Option<ast::File>,
    /// Errors first; empty when `file` is `Some`.
    pub diagnostics: Vec<Diagnostic>,
}

impl ParseResult {
    pub fn ok(&self) -> bool {
        self.file.is_some() && self.diagnostics.is_empty()
    }
}

/// Parse one `.m` file. Never panics on any input.
///
/// Function files are parsed first assuming every function is closed by
/// `end` (so a `function` inside a body is nested) and, if that fails,
/// again assuming functions run to the next `function` or end of file.
/// When both fail, the diagnostic that got further into the file wins.
pub fn parse_file(src: &str, dialect: Dialect) -> ParseResult {
    let make_error = |span: Span, code, message: String| {
        let mut at = (span.start as usize).min(src.len());
        while !src.is_char_boundary(at) {
            at -= 1;
        }
        let line = src[..at].bytes().filter(|b| *b == b'\n').count() as u32 + 1;
        let column = src[..at].rsplit('\n').next().unwrap_or("").chars().count() as u32 + 1;
        Diagnostic {
            span,
            line,
            column,
            code,
            message,
        }
    };
    if src.len() > u32::MAX as usize {
        return ParseResult {
            file: None,
            diagnostics: vec![make_error(
                Span { start: 0, end: 0 },
                "E0111-file-too-large",
                "source files larger than 4 GiB are not supported".into(),
            )],
        };
    }
    let attempt = |nested| parser::Parser::new(src, dialect, nested).file();
    let result = match attempt(true) {
        Ok(file) => Ok(file),
        Err(first) if src.contains("function") => match attempt(false) {
            Ok(file) => Ok(file),
            Err(second) if second.span.start > first.span.start => Err(second),
            Err(_) => Err(first),
        },
        Err(first) => Err(first),
    };
    match result {
        Ok(file) => ParseResult {
            file: Some(file),
            diagnostics: vec![],
        },
        Err(e) => ParseResult {
            file: None,
            diagnostics: vec![make_error(e.span, e.code, e.message)],
        },
    }
}
