//! Recursive-descent statement parser with precedence climbing for
//! expressions.
//!
//! # Operator precedence (low to high)
//!
//! `||`, `&&`, `|`, `&`, comparisons, `:` (range), binary `+ -`,
//! `* / \ .* ./ .\`, prefix `- + ~ !`, then `^ .^ ' .'` (left to right).
//! The operand of `^`/`.^` may carry its own prefix operators, so `2^-1`
//! is `2^(-1)` while `-2^2` is `-(2^2)`.
//!
//! # Matrix and cell literals
//!
//! Inside `[]`/`{}` whitespace separates elements. An element ends before
//! a token that follows whitespace and cannot continue the expression, and
//! before a `+`/`-` that has whitespace before it but not after it:
//! `[1 -2]` and `[a -b]` have two elements, `[1 - 2]`, `[a - b]` and
//! `[a-b]` have one. `(`, `{` and `.` after whitespace start a new element
//! (`[a (1)]` is `[a, 1]`). A `'` after whitespace starts a char literal
//! (decided by the lexer). Inside parentheses nested in a literal the
//! rules are off again.
//!
//! # `end`
//!
//! `end` is the last-index expression while any enclosing `()`/`{}` index
//! or call argument list is open (also through nested `[]`, `()` and
//! anonymous functions, e.g. `a(b(end))`, `x{end}(end)`, `a([1 end])`);
//! everywhere else it closes a block.
//!
//! # Command syntax
//!
//! A statement that starts with an identifier `name` (not a keyword) is
//! parsed as command syntax (`name word1 word2 ...`) when the next token is
//! on the same line, separated from `name` by whitespace, and none of these
//! hold:
//!
//! 1. the next token is a statement terminator (`,` `;` newline, end of
//!    file) or a block keyword (`end`, `else`, ...);
//! 2. the next token is `(` (`disp (x)` is a call);
//! 3. the next token is `=` or an Octave assignment operator (`+=`, `++`);
//! 4. the next token is a binary operator followed by whitespace
//!    (`a - b`, `a == b`);
//! 5. `name` was already assigned earlier in the current scope (function
//!    parameters/outputs, assignment targets, `for` variables,
//!    `global`/`persistent` names, `catch` identifiers): then
//!    `a -b` and `a 'x'` are expressions, as in MATLAB where variables
//!    win over commands.
//!
//! So `hold on`, `format long g`, `pkg load signal`, `warning off all`,
//! `ls -l` and `disp 'hi'` are commands; `a - b`, `a -b` (with `a` a known
//! variable) and `disp (x)` are not. Arguments are read raw from the source
//! up to `,`, `;`, newline or a comment start; `'...'`/`"..."` quote a
//! word (with `''`/`""` escapes).
//!
//! # Function files
//!
//! Whether functions end with `end` cannot be decided locally (in a file
//! without `end`s a `function` inside a body starts the next function; in
//! one with `end`s it starts a nested function). The file is parsed first
//! assuming terminated functions (nested functions allowed) and, if that
//! fails, again assuming unterminated functions; see `parse_file`.

use std::collections::HashSet;

use crate::ast::*;
use crate::lexer::Lexer;
use crate::token::{Kw, Tok, Token};
use crate::{Dialect, Span};

/// Nesting limit for statements and expressions. Deeper input yields
/// `E0104-nesting-too-deep` instead of overflowing the stack.
const MAX_DEPTH: u32 = 128;

#[derive(Debug)]
pub struct PErr {
    pub span: Span,
    pub code: &'static str,
    pub message: String,
}

type R<T> = Result<T, PErr>;

pub struct Parser<'a> {
    src: &'a str,
    lexer: Lexer<'a>,
    buf: Vec<Token>,
    pos: usize,
    last_end: u32,
    octave: bool,
    /// Functions are terminated by `end`; a `function` inside a body is a
    /// nested function.
    nested_functions: bool,
    in_matrix: bool,
    index_depth: u32,
    depth: u32,
    scopes: Vec<HashSet<String>>,
}

fn sp(start: u32, end: u32) -> Span {
    Span { start, end }
}

fn binop(t: &Tok) -> Option<(u8, BinOp)> {
    Some(match t {
        Tok::OrOr => (1, BinOp::OrOr),
        Tok::AndAnd => (2, BinOp::AndAnd),
        Tok::Or => (3, BinOp::Or),
        Tok::And => (4, BinOp::And),
        Tok::Eq => (5, BinOp::Eq),
        Tok::Ne => (5, BinOp::Ne),
        Tok::Lt => (5, BinOp::Lt),
        Tok::Le => (5, BinOp::Le),
        Tok::Gt => (5, BinOp::Gt),
        Tok::Ge => (5, BinOp::Ge),
        Tok::Plus => (7, BinOp::Add),
        Tok::Minus => (7, BinOp::Sub),
        Tok::Star => (8, BinOp::Mul),
        Tok::Slash => (8, BinOp::Div),
        Tok::Backslash => (8, BinOp::LeftDiv),
        Tok::DotStar => (8, BinOp::EMul),
        Tok::DotSlash => (8, BinOp::EDiv),
        Tok::DotBackslash => (8, BinOp::ELeftDiv),
        _ => return None,
    })
}

const RANGE_PREC: u8 = 6;

fn op_assign(t: &Tok) -> Option<BinOp> {
    Some(match t {
        Tok::PlusEq => BinOp::Add,
        Tok::MinusEq => BinOp::Sub,
        Tok::StarEq => BinOp::Mul,
        Tok::SlashEq => BinOp::Div,
        Tok::CaretEq => BinOp::Pow,
        Tok::BackslashEq => BinOp::LeftDiv,
        Tok::AndEq => BinOp::And,
        Tok::OrEq => BinOp::Or,
        Tok::DotStarEq => BinOp::EMul,
        Tok::DotSlashEq => BinOp::EDiv,
        Tok::DotBackslashEq => BinOp::ELeftDiv,
        Tok::DotCaretEq => BinOp::EPow,
        _ => return None,
    })
}

fn is_lvalue(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Ident(_) => true,
        ExprKind::Index { target, .. }
        | ExprKind::CellIndex { target, .. }
        | ExprKind::Field { target, .. }
        | ExprKind::DynField { target, .. } => is_lvalue(target),
        _ => false,
    }
}

fn lvalue_root(e: &Expr) -> Option<&str> {
    match &e.kind {
        ExprKind::Ident(n) => Some(n),
        ExprKind::Index { target, .. }
        | ExprKind::CellIndex { target, .. }
        | ExprKind::Field { target, .. }
        | ExprKind::DynField { target, .. } => lvalue_root(target),
        _ => None,
    }
}

fn closer_for(kw: Kw) -> Kw {
    match kw {
        Kw::If => Kw::EndIf,
        Kw::For => Kw::EndFor,
        Kw::Parfor => Kw::EndParfor,
        Kw::While => Kw::EndWhile,
        Kw::Switch => Kw::EndSwitch,
        Kw::Try => Kw::EndTryCatch,
        Kw::UnwindProtect => Kw::EndUnwindProtect,
        Kw::Function => Kw::EndFunction,
        Kw::Classdef => Kw::EndClassdef,
        Kw::Spmd => Kw::EndSpmd,
        other => other,
    }
}

impl<'a> Parser<'a> {
    pub fn new(src: &'a str, dialect: Dialect, nested_functions: bool) -> Self {
        Parser {
            src,
            lexer: Lexer::new(src, dialect),
            buf: Vec::new(),
            pos: 0,
            last_end: 0,
            octave: dialect == Dialect::Octave,
            nested_functions,
            in_matrix: false,
            index_depth: 0,
            depth: 0,
            scopes: vec![HashSet::new()],
        }
    }

    // ----- token access -------------------------------------------------

    fn tok_at(&mut self, n: usize) -> &Token {
        while self.buf.len() <= self.pos + n {
            if matches!(self.buf.last(), Some(t) if t.tok == Tok::Eof) {
                break;
            }
            let t = self.lexer.next_token();
            self.buf.push(t);
        }
        let i = (self.pos + n).min(self.buf.len() - 1);
        &self.buf[i]
    }

    fn peek(&mut self) -> Tok {
        self.tok_at(0).tok.clone()
    }

    fn peek_n(&mut self, n: usize) -> Tok {
        self.tok_at(n).tok.clone()
    }

    fn is(&mut self, t: &Tok) -> bool {
        self.tok_at(0).tok == *t
    }

    fn is_kw(&mut self, k: Kw) -> bool {
        self.tok_at(0).tok == Tok::Kw(k)
    }

    fn start(&mut self) -> u32 {
        self.tok_at(0).span.start
    }

    fn bump(&mut self) -> Token {
        let t = self.tok_at(0).clone();
        if t.tok != Tok::Eof {
            self.pos += 1;
            self.last_end = t.span.end;
        }
        t
    }

    fn span_from(&self, start: u32) -> Span {
        sp(start, self.last_end.max(start))
    }

    /// Discard lookahead and lex again from byte `offset`.
    fn relex_from(&mut self, offset: usize) {
        self.buf.truncate(self.pos);
        self.lexer.reset(offset);
    }

    fn skip_separators(&mut self) {
        while matches!(self.peek(), Tok::Newline | Tok::Semi | Tok::Comma) {
            self.bump();
        }
    }

    // ----- errors --------------------------------------------------------

    fn line_of(&self, offset: u32) -> usize {
        let at = (offset as usize).min(self.src.len());
        self.src.as_bytes()[..at]
            .iter()
            .filter(|&&c| c == b'\n')
            .count()
            + 1
    }

    fn describe(&self, t: &Token) -> String {
        match &t.tok {
            Tok::Newline => "newline".into(),
            Tok::Eof => "end of file".into(),
            _ => {
                let s = t.span.start as usize;
                let e = (t.span.end as usize).min(self.src.len());
                let text = self.src.get(s..e).unwrap_or("?");
                let short: String = text.chars().take(24).collect();
                format!("`{short}`")
            }
        }
    }

    fn unexpected(&mut self, context: &str) -> PErr {
        let t = self.tok_at(0).clone();
        match t.tok {
            Tok::Error { code, message } => PErr {
                span: t.span,
                code,
                message,
            },
            Tok::Eof => PErr {
                span: t.span,
                code: "E0108-unexpected-eof",
                message: format!("unexpected end of file {context}"),
            },
            _ => PErr {
                span: t.span,
                code: "E0101-unexpected-token",
                message: format!("unexpected {} {context}", self.describe(&t)),
            },
        }
    }

    fn expect(&mut self, t: Tok, context: &str) -> R<Token> {
        if self.is(&t) {
            Ok(self.bump())
        } else {
            Err(self.unexpected(context))
        }
    }

    fn ident(&mut self, context: &str) -> R<Ident> {
        let t = self.tok_at(0).clone();
        if let Tok::Ident(name) = t.tok {
            self.bump();
            Ok(Ident { name, span: t.span })
        } else {
            Err(self.unexpected(context))
        }
    }

    /// Accept `end` or the Octave-specific closer of `opener`.
    fn expect_end(&mut self, opener: Kw, open_span: Span) -> R<()> {
        let t = self.tok_at(0).clone();
        match t.tok {
            Tok::Kw(Kw::End) => {
                self.bump();
                Ok(())
            }
            Tok::Kw(k) if k == closer_for(opener) && k != opener => {
                self.bump();
                Ok(())
            }
            Tok::Error { .. } => Err(self.unexpected("")),
            _ => Err(PErr {
                span: t.span,
                code: "E0103-unbalanced-block",
                message: format!(
                    "expected `end` to close `{}` opened at line {}, found {}",
                    opener.text(),
                    self.line_of(open_span.start),
                    self.describe(&t)
                ),
            }),
        }
    }

    fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> R<T>) -> R<T> {
        if self.depth >= MAX_DEPTH {
            let span = self.tok_at(0).span;
            return Err(PErr {
                span,
                code: "E0104-nesting-too-deep",
                message: format!("nesting deeper than {MAX_DEPTH} levels"),
            });
        }
        self.depth += 1;
        let r = f(self);
        self.depth -= 1;
        r
    }

    // ----- scopes --------------------------------------------------------

    fn declare(&mut self, name: &str) {
        if let Some(s) = self.scopes.last_mut() {
            s.insert(name.to_string());
        }
    }

    fn is_var(&self, name: &str) -> bool {
        self.scopes.iter().any(|s| s.contains(name))
    }

    // ----- file ----------------------------------------------------------

    pub fn file(&mut self) -> R<File> {
        let mut items = Vec::new();
        let mut kind = None;
        loop {
            self.skip_separators();
            let t = self.tok_at(0).clone();
            match t.tok {
                Tok::Eof => break,
                Tok::Kw(Kw::Function) => {
                    kind.get_or_insert(FileKind::Function);
                    items.push(Item::Function(self.function()?));
                }
                Tok::Kw(Kw::Classdef) if kind.is_none() => {
                    kind = Some(FileKind::Class);
                    items.push(Item::Class(self.classdef()?));
                }
                Tok::Kw(Kw::Classdef) => {
                    return Err(PErr {
                        span: t.span,
                        code: "E0101-unexpected-token",
                        message: "`classdef` must be the first statement of its file".into(),
                    });
                }
                Tok::Kw(k) if k.closes_block() => {
                    return Err(PErr {
                        span: t.span,
                        code: "E0103-unbalanced-block",
                        message: format!("`{}` without an open block", k.text()),
                    });
                }
                _ if kind == Some(FileKind::Class) => {
                    return Err(self.unexpected("after classdef (only functions may follow)"));
                }
                _ => {
                    kind.get_or_insert(FileKind::Script);
                    items.push(Item::Stmt(self.statement()?));
                }
            }
        }
        Ok(File {
            kind: kind.unwrap_or(FileKind::Script),
            items,
            span: sp(0, self.src.len() as u32),
        })
    }

    // ----- statements ----------------------------------------------------

    /// Statements up to (not including) a block-closing keyword or EOF.
    fn block(&mut self) -> R<Block> {
        let mut out = Vec::new();
        loop {
            self.skip_separators();
            match self.peek() {
                Tok::Eof => break,
                Tok::Kw(k) if k.closes_block() => break,
                _ => out.push(self.statement()?),
            }
        }
        Ok(out)
    }

    fn statement(&mut self) -> R<Stmt> {
        self.nested(|p| p.statement_inner())
    }

    fn statement_inner(&mut self) -> R<Stmt> {
        let t = self.tok_at(0).clone();
        let start = t.span.start;
        let kind = match t.tok {
            Tok::Kw(Kw::If) => self.if_stmt()?,
            Tok::Kw(Kw::For) => self.for_stmt()?,
            Tok::Kw(Kw::Parfor) => self.parfor_stmt()?,
            Tok::Kw(Kw::While) => {
                self.bump();
                let cond = self.expr()?;
                let body = self.block()?;
                self.expect_end(Kw::While, t.span)?;
                StmtKind::While { cond, body }
            }
            Tok::Kw(Kw::Do) => {
                self.bump();
                let body = self.block()?;
                if !self.is_kw(Kw::Until) {
                    let found = self.tok_at(0).clone();
                    return Err(PErr {
                        span: found.span,
                        code: "E0103-unbalanced-block",
                        message: format!(
                            "expected `until` to close `do` opened at line {}, found {}",
                            self.line_of(start),
                            self.describe(&found)
                        ),
                    });
                }
                self.bump();
                let cond = self.expr()?;
                StmtKind::DoUntil { body, cond }
            }
            Tok::Kw(Kw::Switch) => self.switch_stmt()?,
            Tok::Kw(Kw::Try) => self.try_stmt()?,
            Tok::Kw(Kw::UnwindProtect) => {
                self.bump();
                let body = self.block()?;
                self.expect(
                    Tok::Kw(Kw::UnwindProtectCleanup),
                    "(expected `unwind_protect_cleanup`)",
                )?;
                let cleanup = self.block()?;
                self.expect_end(Kw::UnwindProtect, t.span)?;
                StmtKind::UnwindProtect { body, cleanup }
            }
            Tok::Kw(Kw::Spmd) => {
                self.bump();
                let body = self.block()?;
                self.expect_end(Kw::Spmd, t.span)?;
                StmtKind::Spmd { body }
            }
            Tok::Kw(k @ (Kw::Break | Kw::Continue | Kw::Return)) => {
                self.bump();
                self.end_simple()?;
                let kind = match k {
                    Kw::Break => StmtKind::Break,
                    Kw::Continue => StmtKind::Continue,
                    _ => StmtKind::Return,
                };
                return Ok(Stmt {
                    kind,
                    span: self.span_from(start),
                    display: false,
                });
            }
            Tok::Kw(k @ (Kw::Global | Kw::Persistent)) => return self.decl(k),
            Tok::Kw(_) => return Err(self.unexpected("at start of statement")),
            Tok::Ident(ref name) if self.is_command(name) => return self.command(),
            _ => return self.simple(),
        };
        Ok(Stmt {
            kind,
            span: self.span_from(start),
            display: false,
        })
    }

    /// Consume a simple statement's terminator; returns whether the result
    /// is displayed (no `;`).
    fn end_simple(&mut self) -> R<bool> {
        match self.peek() {
            Tok::Semi => {
                self.bump();
                Ok(false)
            }
            Tok::Comma | Tok::Newline => {
                self.bump();
                Ok(true)
            }
            Tok::Eof => Ok(true),
            Tok::Kw(k) if k.closes_block() => Ok(true),
            _ => Err(self.unexpected("after statement")),
        }
    }

    fn is_command(&mut self, name: &str) -> bool {
        let next = self.tok_at(1).clone();
        if !next.space_before {
            return false;
        }
        match next.tok {
            Tok::Newline | Tok::Eof | Tok::Semi | Tok::Comma | Tok::LParen => false,
            Tok::Assign | Tok::PlusPlus | Tok::MinusMinus => false,
            ref t if op_assign(t).is_some() => false,
            Tok::Kw(k) if k.closes_block() => false,
            ref t
                if binop(t).is_some()
                    || *t == Tok::Colon
                    || *t == Tok::Caret
                    || *t == Tok::DotCaret =>
            {
                let after = self.src.as_bytes().get(next.span.end as usize).copied();
                let spaced = after.is_none_or(|c| c.is_ascii_whitespace());
                !spaced && !self.is_var(name)
            }
            _ => !self.is_var(name),
        }
    }

    fn command(&mut self) -> R<Stmt> {
        let name_tok = self.bump();
        let Tok::Ident(name) = name_tok.tok else {
            return Err(self.unexpected(""));
        };
        let from = self.tok_at(0).span.start as usize;
        let (args, end) = self.scan_command_args(from)?;
        self.relex_from(end);
        let display = self.end_simple()?;
        let start = name_tok.span.start;
        Ok(Stmt {
            kind: StmtKind::Command {
                name: Ident {
                    name,
                    span: name_tok.span,
                },
                args,
            },
            span: sp(start, end as u32),
            display,
        })
    }

    /// Raw command-syntax words starting at byte `i`; returns them and the
    /// byte offset of the terminator.
    fn scan_command_args(&self, mut i: usize) -> R<(Vec<String>, usize)> {
        let b = self.src.as_bytes();
        let mut args = Vec::new();
        loop {
            while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\r') {
                i += 1;
            }
            if i >= b.len() || matches!(b[i], b'\n' | b';' | b',') {
                break;
            }
            if b[i] == b'%' || (self.octave && b[i] == b'#') {
                break;
            }
            let mut word = String::new();
            while i < b.len() {
                let c = b[i];
                if matches!(c, b' ' | b'\t' | b'\r' | b'\n' | b';' | b',') {
                    break;
                }
                if c == b'\'' || c == b'"' {
                    let q_start = i;
                    i += 1;
                    loop {
                        match b.get(i) {
                            None | Some(b'\n') => {
                                return Err(PErr {
                                    span: sp(q_start as u32, i as u32),
                                    code: "E0102-unterminated-string",
                                    message: "unterminated quoted command argument".into(),
                                });
                            }
                            Some(&q) if q == c => {
                                if b.get(i + 1) == Some(&c) {
                                    word.push(c as char);
                                    i += 2;
                                } else {
                                    i += 1;
                                    break;
                                }
                            }
                            Some(_) => {
                                let ch = self.src[i..].chars().next().unwrap_or(' ');
                                word.push(ch);
                                i += ch.len_utf8();
                            }
                        }
                    }
                    continue;
                }
                let ch = self.src[i..].chars().next().unwrap_or(' ');
                word.push(ch);
                i += ch.len_utf8();
            }
            args.push(word);
        }
        Ok((args, i))
    }

    fn simple(&mut self) -> R<Stmt> {
        let start = self.start();
        if self.is(&Tok::LBracket) && self.is_multi_assign() {
            let lhs = self.lhs_list()?;
            self.expect(Tok::Assign, "(expected `=` after assignment targets)")?;
            let rhs = self.assign_rhs()?;
            let display = self.end_simple()?;
            return Ok(Stmt {
                kind: StmtKind::Assign {
                    lhs,
                    bracketed: true,
                    rhs,
                },
                span: self.span_from(start),
                display,
            });
        }
        let e = self.expr()?;
        let t = self.tok_at(0).clone();
        let kind = match t.tok {
            Tok::Assign => {
                self.check_lvalue(&e)?;
                self.bump();
                let rhs = self.assign_rhs()?;
                StmtKind::Assign {
                    lhs: vec![LValue::Expr(e)],
                    bracketed: false,
                    rhs,
                }
            }
            ref op if op_assign(op).is_some() => {
                self.check_lvalue(&e)?;
                self.bump();
                let rhs = self.expr()?;
                StmtKind::OpAssign {
                    lhs: e,
                    op: op_assign(op).unwrap_or(BinOp::Add),
                    rhs,
                }
            }
            _ => match e.kind {
                ExprKind::PostIncDec { target, op } => StmtKind::IncDec { lhs: *target, op },
                kind => StmtKind::Expr(Expr { kind, span: e.span }),
            },
        };
        let display = self.end_simple()?;
        Ok(Stmt {
            kind,
            span: self.span_from(start),
            display,
        })
    }

    /// An expression that, in Octave, may itself be an assignment
    /// (`a = b = 0`, `(n = numel (x))`, `f (w ./= 2)`).
    fn assign_rhs(&mut self) -> R<Expr> {
        let e = self.expr()?;
        let t = self.peek();
        let op = match t {
            Tok::Assign => None,
            ref t if op_assign(t).is_some() => op_assign(t),
            _ => return Ok(e),
        };
        if !(self.octave && is_lvalue(&e)) {
            return Ok(e);
        }
        self.check_lvalue(&e)?;
        self.bump();
        let rhs = self.nested(|p| p.assign_rhs())?;
        Ok(Expr {
            span: sp(e.span.start, rhs.span.end),
            kind: ExprKind::Assign {
                lhs: Box::new(e),
                op,
                rhs: Box::new(rhs),
            },
        })
    }

    fn check_lvalue(&mut self, e: &Expr) -> R<()> {
        if !is_lvalue(e) {
            return Err(PErr {
                span: e.span,
                code: "E0110-invalid-assignment-target",
                message: "left side of `=` is not assignable".into(),
            });
        }
        if let Some(root) = lvalue_root(e) {
            let root = root.to_string();
            self.declare(&root);
        }
        Ok(())
    }

    /// `[` ... matching `]` followed by `=`.
    fn is_multi_assign(&mut self) -> bool {
        let mut depth = 0usize;
        let mut i = 0;
        loop {
            match self.peek_n(i) {
                Tok::LBracket | Tok::LParen | Tok::LBrace => depth += 1,
                Tok::RBracket | Tok::RParen | Tok::RBrace => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return self.peek_n(i + 1) == Tok::Assign;
                    }
                }
                Tok::Eof => return false,
                Tok::Newline if depth == 1 => return false,
                _ => {}
            }
            i += 1;
        }
    }

    fn lhs_list(&mut self) -> R<Vec<LValue>> {
        self.bump(); // [
        let saved = std::mem::replace(&mut self.in_matrix, true);
        let r = self.lhs_list_inner();
        self.in_matrix = saved;
        r
    }

    fn lhs_list_inner(&mut self) -> R<Vec<LValue>> {
        let mut out = Vec::new();
        loop {
            let t = self.tok_at(0).clone();
            match t.tok {
                Tok::RBracket => {
                    self.bump();
                    return Ok(out);
                }
                Tok::Comma => {
                    self.bump();
                }
                Tok::Not
                    if matches!(self.peek_n(1), Tok::Comma | Tok::RBracket)
                        || self.tok_at(1).space_before =>
                {
                    self.bump();
                    out.push(LValue::Ignore(t.span));
                }
                _ => {
                    let e = self.postfix()?;
                    self.check_lvalue(&e)?;
                    out.push(LValue::Expr(e));
                }
            }
        }
    }

    fn if_stmt(&mut self) -> R<StmtKind> {
        let kw = self.bump();
        let mut clauses = Vec::new();
        let cond = self.expr()?;
        let body = self.block()?;
        clauses.push(IfClause { cond, body });
        let mut else_body = None;
        loop {
            match self.peek() {
                Tok::Kw(Kw::Elseif) => {
                    self.bump();
                    let cond = self.expr()?;
                    let body = self.block()?;
                    clauses.push(IfClause { cond, body });
                }
                Tok::Kw(Kw::Else) => {
                    self.bump();
                    else_body = Some(self.block()?);
                    break;
                }
                _ => break,
            }
        }
        self.expect_end(Kw::If, kw.span)?;
        Ok(StmtKind::If { clauses, else_body })
    }

    /// `( ident =` after `for` / `parfor`.
    fn paren_loop_header(&mut self) -> bool {
        self.is(&Tok::LParen)
            && matches!(self.peek_n(1), Tok::Ident(_))
            && self.peek_n(2) == Tok::Assign
    }

    fn for_stmt(&mut self) -> R<StmtKind> {
        let kw = self.bump();
        if self.octave && self.is(&Tok::LBracket) {
            self.bump();
            let value = self.ident("(expected value variable in `for [value, key]`)")?;
            if self.is(&Tok::Comma) {
                self.bump();
            }
            let key = self.ident("(expected key variable in `for [value, key]`)")?;
            self.expect(Tok::RBracket, "(expected `]` in `for [value, key]`)")?;
            self.expect(Tok::Assign, "(expected `=` in for loop)")?;
            self.declare(&value.name);
            self.declare(&key.name);
            let iter = self.expr()?;
            let body = self.block()?;
            self.expect_end(Kw::For, kw.span)?;
            return Ok(StmtKind::ForKeyValue {
                value,
                key,
                iter,
                body,
            });
        }
        let paren = self.paren_loop_header();
        if paren {
            self.bump();
        }
        let var = self.postfix()?;
        self.check_lvalue(&var)?;
        self.expect(Tok::Assign, "(expected `=` in for loop)")?;
        let iter = self.expr()?;
        if paren {
            self.expect(Tok::RParen, "(expected `)` closing the for header)")?;
        }
        let body = self.block()?;
        self.expect_end(Kw::For, kw.span)?;
        Ok(StmtKind::For { var, iter, body })
    }

    fn parfor_stmt(&mut self) -> R<StmtKind> {
        let kw = self.bump();
        let paren = self.paren_loop_header();
        if paren {
            self.bump();
        }
        let var = self.ident("(expected parfor variable)")?;
        self.declare(&var.name);
        self.expect(Tok::Assign, "(expected `=` in parfor loop)")?;
        let iter = self.expr()?;
        let mut max_workers = None;
        if paren {
            if self.is(&Tok::Comma) {
                self.bump();
                max_workers = Some(self.expr()?);
            }
            self.expect(Tok::RParen, "(expected `)` closing the parfor header)")?;
        }
        let body = self.block()?;
        self.expect_end(Kw::Parfor, kw.span)?;
        Ok(StmtKind::Parfor {
            var,
            iter,
            max_workers,
            body,
        })
    }

    fn switch_stmt(&mut self) -> R<StmtKind> {
        let kw = self.bump();
        let subject = self.expr()?;
        let mut cases = Vec::new();
        let mut otherwise = None;
        loop {
            self.skip_separators();
            match self.peek() {
                Tok::Kw(Kw::Case) => {
                    self.bump();
                    let value = self.expr()?;
                    let body = self.block()?;
                    cases.push(SwitchCase { value, body });
                }
                Tok::Kw(Kw::Otherwise) => {
                    self.bump();
                    otherwise = Some(self.block()?);
                }
                _ => break,
            }
        }
        self.expect_end(Kw::Switch, kw.span)?;
        Ok(StmtKind::Switch {
            subject,
            cases,
            otherwise,
        })
    }

    fn try_stmt(&mut self) -> R<StmtKind> {
        let kw = self.bump();
        let body = self.block()?;
        let mut catch_var = None;
        let mut catch_body = None;
        if self.is_kw(Kw::Catch) {
            self.bump();
            let t = self.tok_at(0).clone();
            if let Tok::Ident(name) = t.tok
                && matches!(
                    self.peek_n(1),
                    Tok::Newline | Tok::Semi | Tok::Comma | Tok::Eof
                )
            {
                self.bump();
                self.declare(&name);
                catch_var = Some(Ident { name, span: t.span });
            }
            catch_body = Some(self.block()?);
        }
        self.expect_end(Kw::Try, kw.span)?;
        Ok(StmtKind::Try {
            body,
            catch_var,
            catch_body,
        })
    }

    fn decl(&mut self, k: Kw) -> R<Stmt> {
        let start = self.bump().span.start;
        let mut names = Vec::new();
        while let Tok::Ident(_) = self.peek() {
            let id = self.ident("")?;
            self.declare(&id.name);
            let init = if self.is(&Tok::Assign) {
                self.bump();
                Some(self.expr()?)
            } else {
                None
            };
            names.push((id, init));
        }
        self.end_simple()?;
        let kind = if k == Kw::Global {
            DeclKind::Global
        } else {
            DeclKind::Persistent
        };
        Ok(Stmt {
            kind: StmtKind::Decl { kind, names },
            span: self.span_from(start),
            display: false,
        })
    }

    // ----- functions -----------------------------------------------------

    fn function(&mut self) -> R<Function> {
        self.nested(|p| p.function_inner())
    }

    fn function_inner(&mut self) -> R<Function> {
        let kw = self.bump();
        let (name, outputs, params) = self.signature()?;
        let mut scope = HashSet::new();
        scope.extend(outputs.iter().map(|o| o.name.clone()));
        scope.extend(params.iter().filter_map(|p| match p {
            Param::Name(n) | Param::WithDefault { name: n, .. } => Some(n.name.clone()),
            Param::Ignore(_) => None,
        }));
        self.scopes.push(scope);
        let r = self.function_body(kw.span, &name);
        self.scopes.pop();
        let (arguments, body, nested, has_end) = r?;
        Ok(Function {
            name,
            outputs,
            params,
            arguments,
            body,
            nested,
            has_end,
            span: self.span_from(kw.span.start),
        })
    }

    #[allow(clippy::type_complexity)]
    fn function_body(
        &mut self,
        open: Span,
        name: &Path,
    ) -> R<(Vec<ArgumentsBlock>, Block, Vec<Function>, bool)> {
        let arguments = self.arguments_blocks()?;
        let mut body = Vec::new();
        let mut nested = Vec::new();
        loop {
            body.extend(self.block()?);
            let t = self.tok_at(0).clone();
            match t.tok {
                Tok::Kw(Kw::End | Kw::EndFunction) => {
                    self.bump();
                    return Ok((arguments, body, nested, true));
                }
                Tok::Kw(Kw::Function) if self.nested_functions => {
                    nested.push(self.function()?);
                }
                Tok::Kw(Kw::Function) => return Ok((arguments, body, nested, false)),
                Tok::Eof if !self.nested_functions => {
                    return Ok((arguments, body, nested, false));
                }
                Tok::Eof => {
                    return Err(PErr {
                        span: t.span,
                        code: "E0103-unbalanced-block",
                        message: format!(
                            "function `{}` opened at line {} is not closed by `end`",
                            name.dotted(),
                            self.line_of(open.start)
                        ),
                    });
                }
                Tok::Kw(k) => {
                    return Err(PErr {
                        span: t.span,
                        code: "E0103-unbalanced-block",
                        message: format!(
                            "`{}` without an open block in function `{}`",
                            k.text(),
                            name.dotted()
                        ),
                    });
                }
                _ => return Err(self.unexpected("in function body")),
            }
        }
    }

    /// `[a, b] = name(x, y)`, `a = name`, `name(x)`, `name`.
    fn signature(&mut self) -> R<(Path, Vec<Ident>, Vec<Param>)> {
        let mut outputs = Vec::new();
        if self.is(&Tok::LBracket) {
            self.bump();
            loop {
                match self.peek() {
                    Tok::Ident(_) => outputs.push(self.ident("")?),
                    Tok::Comma => {
                        self.bump();
                    }
                    Tok::RBracket => {
                        self.bump();
                        break;
                    }
                    _ => return Err(self.unexpected("in function output list")),
                }
            }
            self.expect(Tok::Assign, "(expected `=` after function outputs)")?;
        } else if matches!(self.peek(), Tok::Ident(_)) && self.peek_n(1) == Tok::Assign {
            outputs.push(self.ident("")?);
            self.bump();
        }
        let name = if let Tok::Kw(Kw::End) = self.peek() {
            // Overloaded `end` method: `function n = end (obj, k, n)`.
            let t = self.bump();
            Path {
                parts: vec![Ident {
                    name: "end".into(),
                    span: t.span,
                }],
                span: t.span,
            }
        } else {
            self.path("(expected function name)")?
        };
        let mut params = Vec::new();
        if self.is(&Tok::LParen) {
            self.bump();
            loop {
                let t = self.tok_at(0).clone();
                match t.tok {
                    Tok::RParen => {
                        self.bump();
                        break;
                    }
                    Tok::Comma => {
                        self.bump();
                    }
                    Tok::Ident(_) => {
                        let name = self.ident("")?;
                        if self.octave && self.is(&Tok::Assign) {
                            self.bump();
                            let saved = std::mem::replace(&mut self.in_matrix, false);
                            let default = self.expr();
                            self.in_matrix = saved;
                            params.push(Param::WithDefault {
                                name,
                                default: default?,
                            });
                        } else {
                            params.push(Param::Name(name));
                        }
                    }
                    Tok::Not => {
                        self.bump();
                        params.push(Param::Ignore(t.span));
                    }
                    _ => return Err(self.unexpected("in function parameter list")),
                }
            }
        }
        Ok((name, outputs, params))
    }

    fn at_arguments_block(&mut self) -> bool {
        if !matches!(self.peek(), Tok::Ident(ref n) if n == "arguments") {
            return false;
        }
        match self.peek_n(1) {
            Tok::Newline | Tok::Semi | Tok::Comma | Tok::Eof => true,
            Tok::LParen => {
                matches!(self.peek_n(2), Tok::Ident(_))
                    && matches!(self.peek_n(3), Tok::RParen | Tok::Comma)
            }
            _ => false,
        }
    }

    fn arguments_blocks(&mut self) -> R<Vec<ArgumentsBlock>> {
        let mut out = Vec::new();
        loop {
            self.skip_separators();
            if !self.at_arguments_block() {
                return Ok(out);
            }
            let kw = self.bump();
            let attributes = if self.is(&Tok::LParen) {
                self.attributes()?
            } else {
                Vec::new()
            };
            let mut decls = Vec::new();
            loop {
                self.skip_separators();
                match self.peek() {
                    Tok::Kw(Kw::End) => {
                        self.bump();
                        break;
                    }
                    Tok::Ident(_) => decls.push(self.var_decl(true)?),
                    Tok::Eof => {
                        return Err(PErr {
                            span: kw.span,
                            code: "E0103-unbalanced-block",
                            message: "`arguments` block is not closed by `end`".into(),
                        });
                    }
                    _ => return Err(self.unexpected("in arguments block")),
                }
            }
            out.push(ArgumentsBlock {
                attributes,
                decls,
                span: self.span_from(kw.span.start),
            });
        }
    }

    /// `name (dims) Class {validators} = default`
    fn var_decl(&mut self, dotted: bool) -> R<VarDecl> {
        let start = self.start();
        let name = if dotted {
            self.path("(expected a name)")?
        } else {
            let id = self.ident("(expected a property name)")?;
            Path {
                span: id.span,
                parts: vec![id],
            }
        };
        let size = if self.is(&Tok::LParen) {
            self.bump();
            Some(self.args(Tok::RParen, false)?)
        } else {
            None
        };
        let class = if matches!(self.peek(), Tok::Ident(_)) {
            Some(self.path("")?)
        } else {
            None
        };
        let mut validators = Vec::new();
        if self.is(&Tok::LBrace) {
            let cell = self.nested(|p| p.matrix(Tok::RBrace))?;
            if let ExprKind::Cell(rows) = cell.kind {
                validators = rows.into_iter().flatten().collect();
            }
        }
        let default = if self.is(&Tok::Assign) {
            self.bump();
            Some(self.expr()?)
        } else {
            None
        };
        match self.peek() {
            Tok::Newline | Tok::Semi | Tok::Comma => {
                self.bump();
            }
            Tok::Eof | Tok::Kw(_) => {}
            _ => return Err(self.unexpected("in declaration")),
        }
        Ok(VarDecl {
            name,
            size,
            class,
            validators,
            default,
            span: self.span_from(start),
        })
    }

    fn path(&mut self, context: &str) -> R<Path> {
        let first = self.ident(context)?;
        let start = first.span.start;
        let mut parts = vec![first];
        while self.is(&Tok::Dot) && matches!(self.peek_n(1), Tok::Ident(_)) {
            self.bump();
            parts.push(self.ident("")?);
        }
        Ok(Path {
            parts,
            span: self.span_from(start),
        })
    }

    fn attributes(&mut self) -> R<Vec<Attribute>> {
        self.bump(); // (
        let saved = std::mem::replace(&mut self.in_matrix, false);
        let r = self.attributes_inner();
        self.in_matrix = saved;
        r
    }

    fn attributes_inner(&mut self) -> R<Vec<Attribute>> {
        let mut out = Vec::new();
        loop {
            let t = self.tok_at(0).clone();
            match t.tok {
                Tok::RParen => {
                    self.bump();
                    return Ok(out);
                }
                Tok::Comma | Tok::Newline => {
                    self.bump();
                }
                Tok::Not | Tok::Ident(_) => {
                    let negated = t.tok == Tok::Not;
                    if negated {
                        self.bump();
                    }
                    let name = self.ident("(expected attribute name)")?;
                    let value = if self.is(&Tok::Assign) {
                        self.bump();
                        Some(self.expr()?)
                    } else {
                        None
                    };
                    out.push(Attribute {
                        name,
                        negated,
                        value,
                        span: self.span_from(t.span.start),
                    });
                }
                _ => return Err(self.unexpected("in attribute list")),
            }
        }
    }

    // ----- classdef ------------------------------------------------------

    fn classdef(&mut self) -> R<ClassDef> {
        let saved = std::mem::replace(&mut self.nested_functions, true);
        let r = self.classdef_inner();
        self.nested_functions = saved;
        r
    }

    fn classdef_inner(&mut self) -> R<ClassDef> {
        let kw = self.bump();
        let attributes = if self.is(&Tok::LParen) {
            self.attributes()?
        } else {
            Vec::new()
        };
        let name = self.ident("(expected class name)")?;
        let mut superclasses = Vec::new();
        if self.is(&Tok::Lt) {
            self.bump();
            loop {
                superclasses.push(self.path("(expected superclass name)")?);
                if self.is(&Tok::And) {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        let mut sections = Vec::new();
        loop {
            self.skip_separators();
            let t = self.tok_at(0).clone();
            match t.tok {
                Tok::Kw(Kw::End | Kw::EndClassdef) => {
                    self.bump();
                    break;
                }
                Tok::Ident(ref s) if s == "properties" => sections.push(self.properties()?),
                Tok::Ident(ref s) if s == "methods" => sections.push(self.methods()?),
                Tok::Ident(ref s) if s == "events" => sections.push(self.events()?),
                Tok::Ident(ref s) if s == "enumeration" => sections.push(self.enumeration()?),
                Tok::Eof => {
                    return Err(PErr {
                        span: t.span,
                        code: "E0103-unbalanced-block",
                        message: format!(
                            "`classdef` opened at line {} is not closed by `end`",
                            self.line_of(kw.span.start)
                        ),
                    });
                }
                _ => return Err(self.unexpected("in classdef body")),
            }
        }
        Ok(ClassDef {
            name,
            attributes,
            superclasses,
            sections,
            span: self.span_from(kw.span.start),
        })
    }

    fn section_header(&mut self) -> R<(Token, Vec<Attribute>)> {
        let kw = self.bump();
        let attributes = if self.is(&Tok::LParen) {
            self.attributes()?
        } else {
            Vec::new()
        };
        Ok((kw, attributes))
    }

    /// Skips separators; true (and consumes it) at the section's `end`.
    fn section_end(&mut self, specific: Kw, kw: &Token) -> R<bool> {
        self.skip_separators();
        let t = self.tok_at(0).clone();
        match t.tok {
            Tok::Kw(Kw::End) => {
                self.bump();
                Ok(true)
            }
            Tok::Kw(k) if k == specific => {
                self.bump();
                Ok(true)
            }
            Tok::Eof => Err(PErr {
                span: t.span,
                code: "E0103-unbalanced-block",
                message: format!(
                    "`{}` block opened at line {} is not closed by `end`",
                    self.describe(kw),
                    self.line_of(kw.span.start)
                ),
            }),
            _ => Ok(false),
        }
    }

    fn properties(&mut self) -> R<ClassSection> {
        let (kw, attributes) = self.section_header()?;
        let mut props = Vec::new();
        while !self.section_end(Kw::EndProperties, &kw)? {
            props.push(self.var_decl(false)?);
        }
        Ok(ClassSection::Properties {
            attributes,
            props,
            span: self.span_from(kw.span.start),
        })
    }

    fn methods(&mut self) -> R<ClassSection> {
        let (kw, attributes) = self.section_header()?;
        let mut functions = Vec::new();
        let mut decls = Vec::new();
        while !self.section_end(Kw::EndMethods, &kw)? {
            match self.peek() {
                Tok::Kw(Kw::Function) => functions.push(self.function()?),
                Tok::Ident(_) | Tok::LBracket => {
                    let start = self.start();
                    let (name, outputs, params) = self.signature()?;
                    decls.push(FunctionDecl {
                        name,
                        outputs,
                        params,
                        span: self.span_from(start),
                    });
                    match self.peek() {
                        Tok::Newline | Tok::Semi | Tok::Comma => {
                            self.bump();
                        }
                        Tok::Eof | Tok::Kw(_) => {}
                        _ => return Err(self.unexpected("after method declaration")),
                    }
                }
                _ => return Err(self.unexpected("in methods block")),
            }
        }
        Ok(ClassSection::Methods {
            attributes,
            functions,
            decls,
            span: self.span_from(kw.span.start),
        })
    }

    fn events(&mut self) -> R<ClassSection> {
        let (kw, attributes) = self.section_header()?;
        let mut names = Vec::new();
        while !self.section_end(Kw::EndEvents, &kw)? {
            names.push(self.ident("in events block")?);
        }
        Ok(ClassSection::Events {
            attributes,
            names,
            span: self.span_from(kw.span.start),
        })
    }

    fn enumeration(&mut self) -> R<ClassSection> {
        let kw = self.bump();
        let mut members = Vec::new();
        while !self.section_end(Kw::EndEnumeration, &kw)? {
            let name = self.ident("in enumeration block")?;
            let args = if self.is(&Tok::LParen) {
                self.bump();
                self.args(Tok::RParen, false)?
            } else {
                Vec::new()
            };
            members.push(EnumMember {
                span: self.span_from(name.span.start),
                name,
                args,
            });
        }
        Ok(ClassSection::Enumeration {
            members,
            span: self.span_from(kw.span.start),
        })
    }

    // ----- expressions ---------------------------------------------------

    pub fn expr(&mut self) -> R<Expr> {
        self.nested(|p| p.binary(0))
    }

    fn binary(&mut self, min: u8) -> R<Expr> {
        let mut lhs = self.unary()?;
        loop {
            let t = self.tok_at(0).clone();
            if t.tok == Tok::Colon {
                if min > RANGE_PREC {
                    break;
                }
                self.bump();
                let mid = self.binary(RANGE_PREC + 1)?;
                let (step, stop) = if self.is(&Tok::Colon) {
                    self.bump();
                    (Some(Box::new(mid)), self.binary(RANGE_PREC + 1)?)
                } else {
                    (None, mid)
                };
                let span = sp(lhs.span.start, stop.span.end);
                lhs = Expr {
                    kind: ExprKind::Range {
                        start: Box::new(lhs),
                        step,
                        stop: Box::new(stop),
                    },
                    span,
                };
                continue;
            }
            let Some((prec, op)) = binop(&t.tok) else {
                break;
            };
            if prec < min {
                break;
            }
            if self.in_matrix
                && matches!(t.tok, Tok::Plus | Tok::Minus)
                && t.space_before
                && !self.tok_at(1).space_before
            {
                break;
            }
            self.bump();
            let rhs = self.binary(prec + 1)?;
            let span = sp(lhs.span.start, rhs.span.end);
            lhs = Expr {
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                span,
            };
        }
        Ok(lhs)
    }

    fn prefix_op(t: &Tok) -> Option<UnOp> {
        match t {
            Tok::Minus => Some(UnOp::Neg),
            Tok::Plus => Some(UnOp::Plus),
            Tok::Not => Some(UnOp::Not),
            _ => None,
        }
    }

    fn unary(&mut self) -> R<Expr> {
        let t = self.tok_at(0).clone();
        let Some(op) = Self::prefix_op(&t.tok) else {
            return self.power();
        };
        self.bump();
        let operand = self.nested(|p| p.unary())?;
        Ok(Expr {
            span: sp(t.span.start, operand.span.end),
            kind: ExprKind::Unary {
                op,
                operand: Box::new(operand),
            },
        })
    }

    fn power(&mut self) -> R<Expr> {
        let mut base = self.postfix()?;
        loop {
            let t = self.tok_at(0).clone();
            match t.tok {
                Tok::Caret | Tok::DotCaret => {
                    self.bump();
                    let rhs = self.power_operand()?;
                    let op = if t.tok == Tok::Caret {
                        BinOp::Pow
                    } else {
                        BinOp::EPow
                    };
                    base = Expr {
                        span: sp(base.span.start, rhs.span.end),
                        kind: ExprKind::Binary {
                            op,
                            lhs: Box::new(base),
                            rhs: Box::new(rhs),
                        },
                    };
                }
                Tok::CTranspose | Tok::Transpose => {
                    self.bump();
                    let op = if t.tok == Tok::CTranspose {
                        UnOp::CTranspose
                    } else {
                        UnOp::Transpose
                    };
                    base = Expr {
                        span: sp(base.span.start, t.span.end),
                        kind: ExprKind::Unary {
                            op,
                            operand: Box::new(base),
                        },
                    };
                    if self.octave {
                        // Octave indexes transposed values: `a'(:, j)`.
                        base = self.postfix_ops(base)?;
                    }
                }
                Tok::PlusPlus | Tok::MinusMinus if is_lvalue(&base) => {
                    self.bump();
                    let op = if t.tok == Tok::PlusPlus {
                        BinOp::Add
                    } else {
                        BinOp::Sub
                    };
                    base = Expr {
                        span: sp(base.span.start, t.span.end),
                        kind: ExprKind::PostIncDec {
                            target: Box::new(base),
                            op,
                        },
                    };
                }
                _ => return Ok(base),
            }
        }
    }

    fn power_operand(&mut self) -> R<Expr> {
        let t = self.tok_at(0).clone();
        let Some(op) = Self::prefix_op(&t.tok) else {
            return self.postfix();
        };
        self.bump();
        let operand = self.nested(|p| p.power_operand())?;
        Ok(Expr {
            span: sp(t.span.start, operand.span.end),
            kind: ExprKind::Unary {
                op,
                operand: Box::new(operand),
            },
        })
    }

    fn postfix(&mut self) -> R<Expr> {
        let e = self.primary()?;
        self.postfix_ops(e)
    }

    /// `()`, `{}`, `.name`, `.(e)` and `@Super` suffixes applied to `e`.
    fn postfix_ops(&mut self, mut e: Expr) -> R<Expr> {
        loop {
            let t = self.tok_at(0).clone();
            let start = e.span.start;
            match t.tok {
                Tok::LParen | Tok::LBrace => {
                    if self.in_matrix && t.space_before {
                        break;
                    }
                    self.bump();
                    let paren = t.tok == Tok::LParen;
                    let close = if paren { Tok::RParen } else { Tok::RBrace };
                    let args = self.args(close, paren)?;
                    let target = Box::new(e);
                    let kind = if paren {
                        ExprKind::Index { target, args }
                    } else {
                        ExprKind::CellIndex { target, args }
                    };
                    e = Expr {
                        kind,
                        span: self.span_from(start),
                    };
                }
                Tok::Dot => {
                    if self.in_matrix && t.space_before {
                        break;
                    }
                    self.bump();
                    let n = self.tok_at(0).clone();
                    match n.tok {
                        Tok::Ident(name) => {
                            self.bump();
                            e = Expr {
                                kind: ExprKind::Field {
                                    target: Box::new(e),
                                    name: Ident { name, span: n.span },
                                },
                                span: self.span_from(start),
                            };
                        }
                        Tok::LParen => {
                            self.bump();
                            let saved = std::mem::replace(&mut self.in_matrix, false);
                            let name = self.expr();
                            self.in_matrix = saved;
                            let name = name?;
                            self.expect(Tok::RParen, "(expected `)` closing dynamic field)")?;
                            e = Expr {
                                kind: ExprKind::DynField {
                                    target: Box::new(e),
                                    name: Box::new(name),
                                },
                                span: self.span_from(start),
                            };
                        }
                        _ => return Err(self.unexpected("after `.`")),
                    }
                }
                Tok::At if !t.space_before && matches!(e.kind, ExprKind::Ident(_)) => {
                    self.bump();
                    let superclass = self.path("(expected superclass name after `@`)")?;
                    let ExprKind::Ident(name) = e.kind else {
                        unreachable!()
                    };
                    e = Expr {
                        kind: ExprKind::SuperRef {
                            method: Ident { name, span: e.span },
                            superclass,
                        },
                        span: self.span_from(start),
                    };
                }
                _ => break,
            }
        }
        Ok(e)
    }

    /// Index / call arguments after the opening bracket, through `close`.
    fn args(&mut self, close: Tok, allow_named: bool) -> R<Vec<Expr>> {
        let saved = std::mem::replace(&mut self.in_matrix, false);
        self.index_depth += 1;
        let r = self.args_inner(close, allow_named);
        self.index_depth -= 1;
        self.in_matrix = saved;
        r
    }

    fn args_inner(&mut self, close: Tok, allow_named: bool) -> R<Vec<Expr>> {
        let mut args = Vec::new();
        if self.is(&close) {
            self.bump();
            return Ok(args);
        }
        loop {
            let t = self.tok_at(0).clone();
            let next = self.peek_n(1);
            let arg = if t.tok == Tok::Colon && (next == Tok::Comma || next == close) {
                self.bump();
                Expr {
                    kind: ExprKind::Colon,
                    span: t.span,
                }
            } else if let (true, Tok::Ident(name), Tok::Assign) = (allow_named, &t.tok, &next) {
                let name = Ident {
                    name: name.clone(),
                    span: t.span,
                };
                self.bump();
                self.bump();
                let value = self.expr()?;
                Expr {
                    span: sp(t.span.start, value.span.end),
                    kind: ExprKind::NamedArg {
                        name,
                        value: Box::new(value),
                    },
                }
            } else {
                self.assign_rhs()?
            };
            args.push(arg);
            let t = self.tok_at(0).clone();
            if t.tok == Tok::Comma {
                self.bump();
            } else if t.tok == close {
                self.bump();
                return Ok(args);
            } else {
                let what = if close == Tok::RParen { "`)`" } else { "`}`" };
                return Err(self.unexpected(&format!("(expected `,` or {what})")));
            }
        }
    }

    fn primary(&mut self) -> R<Expr> {
        let t = self.tok_at(0).clone();
        let kind = match t.tok {
            Tok::Ident(name) => ExprKind::Ident(name),
            Tok::Num { value, imag } => ExprKind::Number { value, imag },
            Tok::Int { value, suffix } => ExprKind::Integer { value, suffix },
            Tok::Char(s) => ExprKind::Char(s),
            Tok::Str(s) => ExprKind::Str(s),
            Tok::Kw(Kw::End) if self.index_depth > 0 => ExprKind::End,
            Tok::LParen => {
                self.bump();
                let saved = std::mem::replace(&mut self.in_matrix, false);
                let inner = self.assign_rhs();
                self.in_matrix = saved;
                let inner = inner?;
                self.expect(Tok::RParen, "(expected `)`)")?;
                return Ok(Expr {
                    kind: ExprKind::Paren(Box::new(inner)),
                    span: self.span_from(t.span.start),
                });
            }
            Tok::LBracket => return self.nested(|p| p.matrix(Tok::RBracket)),
            Tok::LBrace => return self.nested(|p| p.matrix(Tok::RBrace)),
            Tok::At => return self.handle(),
            Tok::Question => {
                self.bump();
                let path = self.path("(expected class name after `?`)")?;
                return Ok(Expr {
                    kind: ExprKind::Metaclass(path),
                    span: self.span_from(t.span.start),
                });
            }
            _ => return Err(self.unexpected("in expression")),
        };
        self.bump();
        Ok(Expr { kind, span: t.span })
    }

    fn matrix(&mut self, close: Tok) -> R<Expr> {
        let open = self.bump();
        let saved = std::mem::replace(&mut self.in_matrix, true);
        let r = self.matrix_rows(&close, open.span);
        self.in_matrix = saved;
        let rows = r?;
        let kind = if close == Tok::RBracket {
            ExprKind::Matrix(rows)
        } else {
            ExprKind::Cell(rows)
        };
        Ok(Expr {
            kind,
            span: self.span_from(open.span.start),
        })
    }

    fn matrix_rows(&mut self, close: &Tok, open: Span) -> R<Vec<Vec<Expr>>> {
        let mut rows = Vec::new();
        let mut row: Vec<Expr> = Vec::new();
        let mut after_comma = false;
        loop {
            let t = self.tok_at(0).clone();
            match t.tok {
                ref c if c == close => {
                    self.bump();
                    break;
                }
                Tok::Semi | Tok::Newline => {
                    self.bump();
                    after_comma = false;
                    if !row.is_empty() {
                        rows.push(std::mem::take(&mut row));
                    }
                }
                Tok::Comma => {
                    if row.is_empty() || after_comma {
                        return Err(self.unexpected("in matrix (missing element)"));
                    }
                    self.bump();
                    after_comma = true;
                }
                Tok::Eof => {
                    let what = if *close == Tok::RBracket { "[" } else { "{" };
                    return Err(PErr {
                        span: open,
                        code: "E0109-unclosed-bracket",
                        message: format!(
                            "`{what}` opened at line {} is never closed",
                            self.line_of(open.start)
                        ),
                    });
                }
                _ => {
                    if !row.is_empty() && !after_comma && !t.space_before {
                        return Err(self.unexpected("in matrix (expected separator)"));
                    }
                    row.push(self.assign_rhs()?);
                    after_comma = false;
                }
            }
        }
        if !row.is_empty() {
            rows.push(row);
        }
        Ok(rows)
    }

    fn handle(&mut self) -> R<Expr> {
        let at = self.bump();
        if !self.is(&Tok::LParen) {
            let path = self.path("(expected function name or `(` after `@`)")?;
            return Ok(Expr {
                kind: ExprKind::FuncHandle(path),
                span: self.span_from(at.span.start),
            });
        }
        self.bump();
        let mut params = Vec::new();
        loop {
            let t = self.tok_at(0).clone();
            match t.tok {
                Tok::RParen => {
                    self.bump();
                    break;
                }
                Tok::Comma => {
                    self.bump();
                }
                Tok::Ident(_) => params.push(Param::Name(self.ident("")?)),
                Tok::Not => {
                    self.bump();
                    params.push(Param::Ignore(t.span));
                }
                _ => return Err(self.unexpected("in anonymous function parameters")),
            }
        }
        let body = self.expr()?;
        Ok(Expr {
            span: sp(at.span.start, body.span.end),
            kind: ExprKind::AnonFn {
                params,
                body: Box::new(body),
            },
        })
    }
}
