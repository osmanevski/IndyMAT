//! Token kinds produced by the lexer.

use crate::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kw {
    If,
    Elseif,
    Else,
    End,
    For,
    Parfor,
    While,
    Switch,
    Case,
    Otherwise,
    Try,
    Catch,
    Function,
    Return,
    Break,
    Continue,
    Global,
    Persistent,
    Classdef,
    Spmd,
    // Octave only.
    Do,
    Until,
    UnwindProtect,
    UnwindProtectCleanup,
    EndIf,
    EndFor,
    EndParfor,
    EndWhile,
    EndSwitch,
    EndTryCatch,
    EndUnwindProtect,
    EndFunction,
    EndClassdef,
    EndProperties,
    EndMethods,
    EndEvents,
    EndEnumeration,
    EndSpmd,
}

impl Kw {
    pub fn lookup(word: &str, octave: bool) -> Option<Kw> {
        let kw = match word {
            "if" => Kw::If,
            "elseif" => Kw::Elseif,
            "else" => Kw::Else,
            "end" => Kw::End,
            "for" => Kw::For,
            "parfor" => Kw::Parfor,
            "while" => Kw::While,
            "switch" => Kw::Switch,
            "case" => Kw::Case,
            "otherwise" => Kw::Otherwise,
            "try" => Kw::Try,
            "catch" => Kw::Catch,
            "function" => Kw::Function,
            "return" => Kw::Return,
            "break" => Kw::Break,
            "continue" => Kw::Continue,
            "global" => Kw::Global,
            "persistent" => Kw::Persistent,
            "classdef" => Kw::Classdef,
            "spmd" => Kw::Spmd,
            _ if !octave => return None,
            "do" => Kw::Do,
            "until" => Kw::Until,
            "unwind_protect" => Kw::UnwindProtect,
            "unwind_protect_cleanup" => Kw::UnwindProtectCleanup,
            "endif" => Kw::EndIf,
            "endfor" => Kw::EndFor,
            "endparfor" => Kw::EndParfor,
            "endwhile" => Kw::EndWhile,
            "endswitch" => Kw::EndSwitch,
            "end_try_catch" => Kw::EndTryCatch,
            "end_unwind_protect" => Kw::EndUnwindProtect,
            "endfunction" => Kw::EndFunction,
            "endclassdef" => Kw::EndClassdef,
            "endproperties" => Kw::EndProperties,
            "endmethods" => Kw::EndMethods,
            "endevents" => Kw::EndEvents,
            "endenumeration" => Kw::EndEnumeration,
            "endspmd" => Kw::EndSpmd,
            _ => return None,
        };
        Some(kw)
    }

    /// Keywords that end the statement list of an enclosing block.
    pub fn closes_block(self) -> bool {
        !matches!(
            self,
            Kw::If
                | Kw::For
                | Kw::Parfor
                | Kw::While
                | Kw::Switch
                | Kw::Try
                | Kw::Return
                | Kw::Break
                | Kw::Continue
                | Kw::Global
                | Kw::Persistent
                | Kw::Classdef
                | Kw::Spmd
                | Kw::Do
                | Kw::UnwindProtect
        )
    }

    pub fn text(self) -> &'static str {
        match self {
            Kw::If => "if",
            Kw::Elseif => "elseif",
            Kw::Else => "else",
            Kw::End => "end",
            Kw::For => "for",
            Kw::Parfor => "parfor",
            Kw::While => "while",
            Kw::Switch => "switch",
            Kw::Case => "case",
            Kw::Otherwise => "otherwise",
            Kw::Try => "try",
            Kw::Catch => "catch",
            Kw::Function => "function",
            Kw::Return => "return",
            Kw::Break => "break",
            Kw::Continue => "continue",
            Kw::Global => "global",
            Kw::Persistent => "persistent",
            Kw::Classdef => "classdef",
            Kw::Spmd => "spmd",
            Kw::Do => "do",
            Kw::Until => "until",
            Kw::UnwindProtect => "unwind_protect",
            Kw::UnwindProtectCleanup => "unwind_protect_cleanup",
            Kw::EndIf => "endif",
            Kw::EndFor => "endfor",
            Kw::EndParfor => "endparfor",
            Kw::EndWhile => "endwhile",
            Kw::EndSwitch => "endswitch",
            Kw::EndTryCatch => "end_try_catch",
            Kw::EndUnwindProtect => "end_unwind_protect",
            Kw::EndFunction => "endfunction",
            Kw::EndClassdef => "endclassdef",
            Kw::EndProperties => "endproperties",
            Kw::EndMethods => "endmethods",
            Kw::EndEvents => "endevents",
            Kw::EndEnumeration => "endenumeration",
            Kw::EndSpmd => "endspmd",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    Kw(Kw),
    /// Decimal number (real part); `imag` set for `3i` / `2.5j`.
    Num {
        value: f64,
        imag: bool,
    },
    /// Hex / binary integer literal with optional `u8`..`s64` suffix.
    Int {
        value: u64,
        suffix: Option<&'static str>,
    },
    /// Single-quoted char literal, `''` already collapsed.
    Char(String),
    /// Double-quoted string literal, escapes already processed.
    Str(String),

    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Semi,
    Newline,
    /// Field-access dot (`s.a`, `s.(n)`).
    Dot,
    At,
    Question,
    Colon,
    Assign,

    Plus,
    Minus,
    Star,
    Slash,
    Backslash,
    Caret,
    DotStar,
    DotSlash,
    DotBackslash,
    DotCaret,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    AndAnd,
    OrOr,
    /// `~` or Octave `!`.
    Not,
    /// `'`
    CTranspose,
    /// `.'`
    Transpose,

    // Octave statement operators.
    PlusPlus,
    MinusMinus,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    CaretEq,
    BackslashEq,
    AndEq,
    OrEq,
    DotStarEq,
    DotSlashEq,
    DotBackslashEq,
    DotCaretEq,

    Error {
        code: &'static str,
        message: String,
    },
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
    /// Whitespace, a comment or a `...` continuation precedes the token on
    /// the same logical line. Drives the matrix-literal separation rules.
    pub space_before: bool,
}
