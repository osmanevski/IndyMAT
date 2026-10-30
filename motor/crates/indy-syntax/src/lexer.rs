//! On-demand lexer.
//!
//! The parser pulls tokens one at a time and may [`Lexer::reset`] to a byte
//! offset (used after a command-syntax line, whose words are read raw from
//! the source). The lexer itself never fails: problems become
//! [`Tok::Error`] tokens and lexing resumes after them.
//!
//! Context the lexer tracks itself:
//! - a bracket stack, so that `'` after whitespace inside `[]`/`{}` starts a
//!   char literal and, in the Octave dialect, newlines inside `()` are plain
//!   whitespace;
//! - whether the previous token ends a value (`'` is then a transpose);
//! - whether the previous token was a field dot (keywords become field
//!   names: `s.end`, `obj.properties`).

use crate::Dialect;
use crate::Span;
use crate::token::{Kw, Tok, Token};

pub struct Lexer<'a> {
    src: &'a str,
    b: &'a [u8],
    pos: usize,
    octave: bool,
    stack: Vec<u8>,
    prev_value: bool,
    after_dot: bool,
    line_start: usize,
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_ident_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str, dialect: Dialect) -> Self {
        Lexer {
            src,
            b: src.as_bytes(),
            pos: 0,
            octave: dialect == Dialect::Octave,
            stack: Vec::new(),
            prev_value: false,
            after_dot: false,
            line_start: 0,
        }
    }

    /// Restart lexing at `pos` (a statement boundary: empty bracket stack).
    pub fn reset(&mut self, pos: usize) {
        let pos = pos.min(self.b.len());
        self.pos = pos;
        self.stack.clear();
        self.prev_value = false;
        self.after_dot = false;
        self.line_start = self.b[..pos]
            .iter()
            .rposition(|&c| c == b'\n')
            .map_or(0, |i| i + 1);
    }

    fn peek_at(&self, off: usize) -> u8 {
        self.b.get(self.pos + off).copied().unwrap_or(0)
    }

    fn rest_of_line_blank(&self, from: usize) -> bool {
        self.b[from.min(self.b.len())..]
            .iter()
            .take_while(|&&c| c != b'\n')
            .all(|c| c.is_ascii_whitespace())
    }

    fn is_comment_char(&self, c: u8) -> bool {
        c == b'%' || (self.octave && c == b'#')
    }

    /// `%{` / `#{` alone on its line.
    fn at_block_comment_open(&self) -> bool {
        self.peek_at(1) == b'{'
            && self.b[self.line_start..self.pos]
                .iter()
                .all(|c| c.is_ascii_whitespace())
            && self.rest_of_line_blank(self.pos + 2)
    }

    /// Skips a (nestable) block comment; `pos` ends at the newline that
    /// follows the closing `%}` line.
    fn skip_block_comment(&mut self) -> Result<(), Token> {
        let start = self.pos;
        let mut depth = 0usize;
        let mut line = self.line_start;
        while line < self.b.len() {
            let eol = self.b[line..]
                .iter()
                .position(|&c| c == b'\n')
                .map_or(self.b.len(), |i| line + i);
            let text = self.src[line..eol].trim();
            let t = text.as_bytes();
            if t.len() == 2 && self.is_comment_char(t[0]) && t[1] == b'{' {
                depth += 1;
            } else if t.len() == 2 && self.is_comment_char(t[0]) && t[1] == b'}' {
                depth -= 1;
                if depth == 0 {
                    self.pos = eol;
                    return Ok(());
                }
            }
            line = eol + 1;
        }
        self.pos = self.b.len();
        Err(self.make(
            Tok::Error {
                code: "E0106-unterminated-block-comment",
                message: "block comment `%{` is never closed by `%}`".into(),
            },
            start,
            false,
        ))
    }

    fn make(&self, tok: Tok, start: usize, space_before: bool) -> Token {
        Token {
            tok,
            span: Span {
                start: start as u32,
                end: self.pos as u32,
            },
            space_before,
        }
    }

    pub fn next_token(&mut self) -> Token {
        let mut space = false;
        loop {
            let Some(&c) = self.b.get(self.pos) else {
                return self.make(Tok::Eof, self.pos, space);
            };
            match c {
                b' ' | b'\t' | b'\r' | 0x0c => {
                    self.pos += 1;
                    space = true;
                }
                b'\n' => {
                    if self.octave && self.stack.last() == Some(&b'(') {
                        self.pos += 1;
                        self.line_start = self.pos;
                        space = true;
                        continue;
                    }
                    let start = self.pos;
                    self.pos += 1;
                    self.line_start = self.pos;
                    self.prev_value = false;
                    self.after_dot = false;
                    return self.make(Tok::Newline, start, space);
                }
                _ if self.is_comment_char(c) => {
                    if self.at_block_comment_open() {
                        if let Err(t) = self.skip_block_comment() {
                            return t;
                        }
                    } else {
                        while self.pos < self.b.len() && self.b[self.pos] != b'\n' {
                            self.pos += 1;
                        }
                    }
                    space = true;
                }
                b'.' if self.peek_at(1) == b'.' && self.peek_at(2) == b'.' => {
                    // Continuation: the rest of the line is a comment.
                    while self.pos < self.b.len() && self.b[self.pos] != b'\n' {
                        self.pos += 1;
                    }
                    if self.pos < self.b.len() {
                        self.pos += 1;
                        self.line_start = self.pos;
                    }
                    space = true;
                }
                _ => break,
            }
        }
        let start = self.pos;
        let after_dot = std::mem::take(&mut self.after_dot);
        let c = self.b[self.pos];
        let tok = if is_ident_start(c) {
            while self.pos < self.b.len() && is_ident_char(self.b[self.pos]) {
                self.pos += 1;
            }
            let word = &self.src[start..self.pos];
            match Kw::lookup(word, self.octave) {
                Some(kw) if !after_dot => {
                    self.prev_value = kw == Kw::End && !self.stack.is_empty();
                    return self.make(Tok::Kw(kw), start, space);
                }
                _ => Tok::Ident(word.to_string()),
            }
        } else if c.is_ascii_digit() || (c == b'.' && self.peek_at(1).is_ascii_digit()) {
            self.number()
        } else if c == b'\'' {
            let in_matrix = matches!(self.stack.last(), Some(b'[' | b'{'));
            if self.prev_value && !(space && in_matrix) {
                self.pos += 1;
                Tok::CTranspose
            } else {
                self.char_literal()
            }
        } else if c == b'"' {
            self.string_literal()
        } else {
            self.operator()
        };
        self.prev_value = matches!(
            tok,
            Tok::Ident(_)
                | Tok::Num { .. }
                | Tok::Int { .. }
                | Tok::Char(_)
                | Tok::Str(_)
                | Tok::RParen
                | Tok::RBracket
                | Tok::RBrace
                | Tok::CTranspose
                | Tok::Transpose
        );
        self.after_dot = tok == Tok::Dot;
        self.make(tok, start, space)
    }

    fn number(&mut self) -> Tok {
        let start = self.pos;
        let c1 = self.peek_at(1);
        if self.b[start] == b'0' && matches!(c1, b'x' | b'X' | b'b' | b'B') {
            let radix = if matches!(c1, b'x' | b'X') { 16 } else { 2 };
            let digits_start = start + 2;
            let mut end = digits_start;
            while end < self.b.len() && (self.b[end] as char).is_digit(radix) {
                end += 1;
            }
            if end > digits_start {
                self.pos = end;
                let digits = &self.src[digits_start..end];
                let suffix = self.int_suffix();
                return match u64::from_str_radix(digits, radix) {
                    Ok(value) => Tok::Int { value, suffix },
                    Err(_) => Tok::Error {
                        code: "E0107-invalid-number",
                        message: format!("integer literal `{}` is too large", digits),
                    },
                };
            }
        }
        let digits = |lx: &mut Self| {
            while lx.pos < lx.b.len() && lx.b[lx.pos].is_ascii_digit() {
                lx.pos += 1;
            }
        };
        digits(self);
        if self.peek_at(0) == b'.'
            && !matches!(self.peek_at(1), b'*' | b'/' | b'\\' | b'^' | b'\'')
            && !(self.peek_at(1) == b'.' && self.peek_at(2) == b'.')
        {
            self.pos += 1;
            digits(self);
        }
        if matches!(self.peek_at(0), b'e' | b'E' | b'd' | b'D') {
            let sign = usize::from(matches!(self.peek_at(1), b'+' | b'-'));
            if self.peek_at(1 + sign).is_ascii_digit() {
                self.pos += 1 + sign;
                digits(self);
            }
        }
        let text: String = self.src[start..self.pos]
            .chars()
            .map(|c| if c == 'd' || c == 'D' { 'e' } else { c })
            .collect();
        let mut imag = false;
        if matches!(self.peek_at(0), b'i' | b'j' | b'I' | b'J') && !is_ident_char(self.peek_at(1)) {
            self.pos += 1;
            imag = true;
        }
        match text.parse::<f64>() {
            Ok(value) => Tok::Num { value, imag },
            Err(_) => Tok::Error {
                code: "E0107-invalid-number",
                message: format!("malformed number `{}`", text),
            },
        }
    }

    fn int_suffix(&mut self) -> Option<&'static str> {
        const SUFFIXES: [&str; 8] = ["u8", "u16", "u32", "u64", "s8", "s16", "s32", "s64"];
        for s in SUFFIXES {
            let end = self.pos + s.len();
            if self.b.get(self.pos..end) == Some(s.as_bytes())
                && !is_ident_char(self.b.get(end).copied().unwrap_or(0))
            {
                self.pos = end;
                return Some(s);
            }
        }
        None
    }

    fn unterminated(&self, what: &str) -> Tok {
        Tok::Error {
            code: "E0102-unterminated-string",
            message: format!("unterminated {what} literal"),
        }
    }

    fn char_literal(&mut self) -> Tok {
        self.pos += 1;
        let mut value = String::new();
        loop {
            let Some(ch) = self.src[self.pos..].chars().next() else {
                return self.unterminated("char");
            };
            match ch {
                '\n' => return self.unterminated("char"),
                '\'' if self.peek_at(1) == b'\'' => {
                    value.push('\'');
                    self.pos += 2;
                }
                '\'' => {
                    self.pos += 1;
                    return Tok::Char(value);
                }
                _ => {
                    value.push(ch);
                    self.pos += ch.len_utf8();
                }
            }
        }
    }

    fn string_literal(&mut self) -> Tok {
        self.pos += 1;
        let mut value = String::new();
        loop {
            let Some(ch) = self.src[self.pos..].chars().next() else {
                return self.unterminated("string");
            };
            match ch {
                '\n' => return self.unterminated("string"),
                '"' if self.peek_at(1) == b'"' => {
                    value.push('"');
                    self.pos += 2;
                }
                '"' => {
                    self.pos += 1;
                    return Tok::Str(value);
                }
                '\\' if self.octave => {
                    self.pos += 1;
                    self.escape(&mut value);
                }
                _ => {
                    value.push(ch);
                    self.pos += ch.len_utf8();
                }
            }
        }
    }

    /// Octave double-quote escape; `pos` is just past the backslash.
    fn escape(&mut self, out: &mut String) {
        let Some(ch) = self.src[self.pos..].chars().next() else {
            return;
        };
        if ch == '\n' || (ch == '\r' && self.peek_at(1) == b'\n') {
            // Backslash-newline continues the string on the next line.
            self.pos += if ch == '\r' { 2 } else { 1 };
            self.line_start = self.pos;
            return;
        }
        self.pos += ch.len_utf8();
        let simple = match ch {
            'n' => Some('\n'),
            't' => Some('\t'),
            'r' => Some('\r'),
            'a' => Some('\x07'),
            'b' => Some('\x08'),
            'f' => Some('\x0c'),
            'v' => Some('\x0b'),
            '\\' => Some('\\'),
            '"' => Some('"'),
            '\'' => Some('\''),
            _ => None,
        };
        if let Some(c) = simple {
            out.push(c);
            return;
        }
        let (radix, max, first) = match ch {
            '0'..='7' => (8, 3, Some(ch)),
            'x' => (16, 2, None),
            _ => {
                // Unknown escape: Octave keeps the character.
                out.push(ch);
                return;
            }
        };
        let mut digits = String::new();
        if let Some(f) = first {
            digits.push(f);
        }
        while digits.len() < max && (self.peek_at(0) as char).is_digit(radix) {
            digits.push(self.peek_at(0) as char);
            self.pos += 1;
        }
        match u32::from_str_radix(&digits, radix)
            .ok()
            .and_then(char::from_u32)
        {
            Some(c) => out.push(c),
            None => {
                out.push('\\');
                out.push(ch);
            }
        }
    }

    /// `++` / `--` only as a suffix (`x++;`, `a(k++)`).
    fn incdec_follows(&self) -> bool {
        let mut i = self.pos + 2;
        while i < self.b.len() && matches!(self.b[i], b' ' | b'\t') {
            i += 1;
        }
        match self.b.get(i) {
            None => true,
            Some(&c) => {
                matches!(c, b';' | b',' | b'\n' | b'\r' | b')' | b']' | b'}')
                    || self.is_comment_char(c)
            }
        }
    }

    fn operator(&mut self) -> Tok {
        let c = self.b[self.pos];
        let n = self.peek_at(1);
        let (tok, len) = match c {
            b'(' | b'[' | b'{' => {
                self.stack.push(c);
                let t = match c {
                    b'(' => Tok::LParen,
                    b'[' => Tok::LBracket,
                    _ => Tok::LBrace,
                };
                (t, 1)
            }
            b')' | b']' | b'}' => {
                self.stack.pop();
                let t = match c {
                    b')' => Tok::RParen,
                    b']' => Tok::RBracket,
                    _ => Tok::RBrace,
                };
                (t, 1)
            }
            b',' => (Tok::Comma, 1),
            b';' => (Tok::Semi, 1),
            b':' => (Tok::Colon, 1),
            b'@' => (Tok::At, 1),
            b'?' => (Tok::Question, 1),
            b'=' if n == b'=' => (Tok::Eq, 2),
            b'=' => (Tok::Assign, 1),
            b'~' if n == b'=' => (Tok::Ne, 2),
            b'~' => (Tok::Not, 1),
            b'!' if self.octave && n == b'=' => (Tok::Ne, 2),
            b'!' if self.octave => (Tok::Not, 1),
            b'<' if n == b'=' => (Tok::Le, 2),
            b'<' => (Tok::Lt, 1),
            b'>' if n == b'=' => (Tok::Ge, 2),
            b'>' => (Tok::Gt, 1),
            b'&' if n == b'&' => (Tok::AndAnd, 2),
            b'&' if self.octave && n == b'=' => (Tok::AndEq, 2),
            b'&' => (Tok::And, 1),
            b'|' if n == b'|' => (Tok::OrOr, 2),
            b'|' if self.octave && n == b'=' => (Tok::OrEq, 2),
            b'|' => (Tok::Or, 1),
            b'+' if self.octave && n == b'+' && self.incdec_follows() => (Tok::PlusPlus, 2),
            b'+' if self.octave && n == b'=' => (Tok::PlusEq, 2),
            b'+' => (Tok::Plus, 1),
            b'-' if self.octave && n == b'-' && self.incdec_follows() => (Tok::MinusMinus, 2),
            b'-' if self.octave && n == b'=' => (Tok::MinusEq, 2),
            b'-' => (Tok::Minus, 1),
            b'*' if self.octave && n == b'*' => (Tok::Caret, 2),
            b'*' if self.octave && n == b'=' => (Tok::StarEq, 2),
            b'*' => (Tok::Star, 1),
            b'/' if self.octave && n == b'=' => (Tok::SlashEq, 2),
            b'/' => (Tok::Slash, 1),
            b'\\' if self.octave && n == b'=' => (Tok::BackslashEq, 2),
            b'\\' => (Tok::Backslash, 1),
            b'^' if self.octave && n == b'=' => (Tok::CaretEq, 2),
            b'^' => (Tok::Caret, 1),
            b'.' if self.octave
                && matches!(n, b'*' | b'/' | b'\\' | b'^')
                && self.peek_at(2) == b'=' =>
            {
                let t = match n {
                    b'*' => Tok::DotStarEq,
                    b'/' => Tok::DotSlashEq,
                    b'\\' => Tok::DotBackslashEq,
                    _ => Tok::DotCaretEq,
                };
                (t, 3)
            }
            b'.' => match n {
                b'*' if self.octave && self.peek_at(2) == b'*' => (Tok::DotCaret, 3),
                b'*' => (Tok::DotStar, 2),
                b'/' => (Tok::DotSlash, 2),
                b'\\' => (Tok::DotBackslash, 2),
                b'^' => (Tok::DotCaret, 2),
                b'\'' => (Tok::Transpose, 2),
                _ => (Tok::Dot, 1),
            },
            _ => {
                let ch = self.src[self.pos..].chars().next().unwrap_or('?');
                self.pos += ch.len_utf8();
                return Tok::Error {
                    code: "E0105-invalid-character",
                    message: format!("invalid character {:?}", ch),
                };
            }
        };
        self.pos += len;
        tok
    }
}
