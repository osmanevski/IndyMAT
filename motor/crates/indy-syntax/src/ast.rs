//! Syntax tree. Owned by the parser lane; may grow freely.
//! `File` and `sexpr` are frozen names used by `indy-corpus`.
//!
//! Every node carries the [`Span`] of the source it came from, so the
//! interpreter can report errors at the right line. The tree keeps the
//! distinctions evaluation needs (`(x)` vs `x`, char vs string literal,
//! displayed vs suppressed statements, command syntax) and nothing else
//! (comments and whitespace are dropped).

use crate::Span;
use std::fmt::Write;

#[derive(Debug, Clone, PartialEq)]
pub struct File {
    pub kind: FileKind,
    /// Top-level items in source order. Script files may interleave
    /// statements and (Octave command-line / MATLAB local) functions.
    pub items: Vec<Item>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Script,
    Function,
    Class,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Stmt(Stmt),
    Function(Function),
    Class(ClassDef),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

/// Dotted name such as `matlab.mixin.Copyable` or `set.Value`.
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    pub parts: Vec<Ident>,
    pub span: Span,
}

impl Path {
    pub fn dotted(&self) -> String {
        let names: Vec<&str> = self.parts.iter().map(|p| p.name.as_str()).collect();
        names.join(".")
    }
}

pub type Block = Vec<Stmt>;

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
    /// The statement was not terminated by `;`, so its result is shown.
    /// Only meaningful for expression, assignment and command statements.
    pub display: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LValue {
    /// `~` placeholder in a multi-assignment.
    Ignore(Span),
    /// Identifier, possibly followed by `()`, `{}`, `.name`, `.(expr)`.
    Expr(Expr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclKind {
    Global,
    Persistent,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IfClause {
    pub cond: Expr,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SwitchCase {
    pub value: Expr,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    Expr(Expr),
    /// `lhs = rhs`; more than one target (or `[x] = ...`) is a
    /// multi-assignment. `bracketed` records the `[...]` form.
    Assign {
        lhs: Vec<LValue>,
        bracketed: bool,
        rhs: Expr,
    },
    /// Octave `x += e`, `x -= e`, `x *= e`, `x /= e`, `x ^= e`.
    OpAssign {
        lhs: Expr,
        op: BinOp,
        rhs: Expr,
    },
    /// Octave `x++` (`op` = `Add`) / `x--` (`op` = `Sub`).
    IncDec {
        lhs: Expr,
        op: BinOp,
    },
    /// Command syntax: `hold on`, `format long g`.
    Command {
        name: Ident,
        args: Vec<String>,
    },
    If {
        clauses: Vec<IfClause>,
        else_body: Option<Block>,
    },
    For {
        var: Expr,
        iter: Expr,
        body: Block,
    },
    /// Octave `for [value, key] = struct`.
    ForKeyValue {
        value: Ident,
        key: Ident,
        iter: Expr,
        body: Block,
    },
    Parfor {
        var: Ident,
        iter: Expr,
        max_workers: Option<Expr>,
        body: Block,
    },
    While {
        cond: Expr,
        body: Block,
    },
    /// Octave `do ... until cond`.
    DoUntil {
        body: Block,
        cond: Expr,
    },
    Switch {
        subject: Expr,
        cases: Vec<SwitchCase>,
        otherwise: Option<Block>,
    },
    Try {
        body: Block,
        catch_var: Option<Ident>,
        catch_body: Option<Block>,
    },
    UnwindProtect {
        body: Block,
        cleanup: Block,
    },
    Spmd {
        body: Block,
    },
    Break,
    Continue,
    Return,
    /// `global a b` / `persistent n`; Octave allows `global a = 1`.
    Decl {
        kind: DeclKind,
        names: Vec<(Ident, Option<Expr>)>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Param {
    Name(Ident),
    /// `~` unused input.
    Ignore(Span),
    /// Octave default value: `function f (x, tol = 1e-6)`.
    WithDefault {
        name: Ident,
        default: Expr,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    /// Plain name, or `get.Prop` / `set.Prop` inside classdef.
    pub name: Path,
    pub outputs: Vec<Ident>,
    pub params: Vec<Param>,
    pub arguments: Vec<ArgumentsBlock>,
    pub body: Block,
    /// Nested functions (only in files whose functions end with `end`).
    pub nested: Vec<Function>,
    /// Closed by `end`/`endfunction` rather than by the next function / EOF.
    pub has_end: bool,
    pub span: Span,
}

/// Signature-only method declaration inside a classdef `methods` block.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl {
    pub name: Path,
    pub outputs: Vec<Ident>,
    pub params: Vec<Param>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    pub name: Ident,
    /// `~Name` (false).
    pub negated: bool,
    pub value: Option<Expr>,
    pub span: Span,
}

/// `arguments (Input, Repeating) ... end`.
#[derive(Debug, Clone, PartialEq)]
pub struct ArgumentsBlock {
    pub attributes: Vec<Attribute>,
    pub decls: Vec<VarDecl>,
    pub span: Span,
}

/// Validated declaration, shared by `arguments` entries and classdef
/// properties: `name (dims) Class {validators} = default`.
#[derive(Debug, Clone, PartialEq)]
pub struct VarDecl {
    /// `x`, or `opts.Name` for name-value arguments.
    pub name: Path,
    pub size: Option<Vec<Expr>>,
    pub class: Option<Path>,
    pub validators: Vec<Expr>,
    pub default: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassDef {
    pub name: Ident,
    pub attributes: Vec<Attribute>,
    pub superclasses: Vec<Path>,
    pub sections: Vec<ClassSection>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClassSection {
    Properties {
        attributes: Vec<Attribute>,
        props: Vec<VarDecl>,
        span: Span,
    },
    Methods {
        attributes: Vec<Attribute>,
        functions: Vec<Function>,
        decls: Vec<FunctionDecl>,
        span: Span,
    },
    Events {
        attributes: Vec<Attribute>,
        names: Vec<Ident>,
        span: Span,
    },
    Enumeration {
        members: Vec<EnumMember>,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumMember {
    pub name: Ident,
    pub args: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Plus,
    Not,
    /// `'`
    CTranspose,
    /// `.'`
    Transpose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    LeftDiv,
    Pow,
    EMul,
    EDiv,
    ELeftDiv,
    EPow,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    /// Element-wise `&`.
    And,
    /// Element-wise `|`.
    Or,
    AndAnd,
    OrOr,
}

impl BinOp {
    pub fn text(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::LeftDiv => "\\",
            BinOp::Pow => "^",
            BinOp::EMul => ".*",
            BinOp::EDiv => "./",
            BinOp::ELeftDiv => ".\\",
            BinOp::EPow => ".^",
            BinOp::Eq => "==",
            BinOp::Ne => "~=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&",
            BinOp::Or => "|",
            BinOp::AndAnd => "&&",
            BinOp::OrOr => "||",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// Decimal literal; `imag` for `3i`.
    Number {
        value: f64,
        imag: bool,
    },
    /// Hex/binary literal (`0x1F`, `0b101u16`); the type comes from the
    /// suffix or, without one, the smallest unsigned type that fits.
    Integer {
        value: u64,
        suffix: Option<&'static str>,
    },
    /// `'...'` char row vector.
    Char(String),
    /// `"..."`: a MATLAB string scalar, an Octave double-quoted char array.
    Str(String),
    Ident(String),
    /// Bare `:` index (`x(:)`, `x(:, 1)`, size `(1,:)`).
    Colon,
    /// `end` inside an index: the last index of the enclosing object.
    End,
    Unary {
        op: UnOp,
        operand: Box<Expr>,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Range {
        start: Box<Expr>,
        step: Option<Box<Expr>>,
        stop: Box<Expr>,
    },
    /// `(e)`. Kept because it changes cs-list and display semantics.
    Paren(Box<Expr>),
    /// `target(args)`: indexing or call, decided at run time.
    Index {
        target: Box<Expr>,
        args: Vec<Expr>,
    },
    /// `target{args}`
    CellIndex {
        target: Box<Expr>,
        args: Vec<Expr>,
    },
    /// `target.name`
    Field {
        target: Box<Expr>,
        name: Ident,
    },
    /// `target.(expr)`
    DynField {
        target: Box<Expr>,
        name: Box<Expr>,
    },
    /// `[...]`, rows of elements.
    Matrix(Vec<Vec<Expr>>),
    /// `{...}`, rows of elements.
    Cell(Vec<Vec<Expr>>),
    /// `@(x, y) body`
    AnonFn {
        params: Vec<Param>,
        body: Box<Expr>,
    },
    /// `@sin`, `@pkg.fn`, `@obj.method`
    FuncHandle(Path),
    /// `?ClassName`
    Metaclass(Path),
    /// `method@Superclass` (followed by `(args)` as an `Index`).
    SuperRef {
        method: Ident,
        superclass: Path,
    },
    /// Octave assignment used as an expression: `a = b = 0`,
    /// `if ((n = numel (x)))`, `f (w ./= 2)`; `op` for `+=` and friends.
    Assign {
        lhs: Box<Expr>,
        op: Option<BinOp>,
        rhs: Box<Expr>,
    },
    /// Octave `k++` / `k--` inside an expression (value before update).
    PostIncDec {
        target: Box<Expr>,
        op: BinOp,
    },
    /// `Name=value` argument (MATLAB R2021a call syntax).
    NamedArg {
        name: Ident,
        value: Box<Expr>,
    },
}

// ---------------------------------------------------------------------------
// S-expression rendering
// ---------------------------------------------------------------------------

/// Deterministic one-line S-expression of the tree, for snapshot tests and
/// corpus diffs. Spans are not printed.
pub fn sexpr(file: &File) -> String {
    let mut out = String::from("(file ");
    out.push_str(match file.kind {
        FileKind::Script => "script",
        FileKind::Function => "function",
        FileKind::Class => "classdef",
    });
    for item in &file.items {
        out.push(' ');
        match item {
            Item::Stmt(s) => stmt(&mut out, s),
            Item::Function(f) => function(&mut out, f),
            Item::Class(c) => class(&mut out, c),
        }
    }
    out.push(')');
    out
}

/// S-expression of a single expression (handy in tests).
pub fn sexpr_expr(e: &Expr) -> String {
    let mut out = String::new();
    expr(&mut out, e);
    out
}

fn block(out: &mut String, b: &[Stmt]) {
    out.push_str("(block");
    for s in b {
        out.push(' ');
        stmt(out, s);
    }
    out.push(')');
}

fn exprs(out: &mut String, es: &[Expr]) {
    for e in es {
        out.push(' ');
        expr(out, e);
    }
}

fn stmt(out: &mut String, s: &Stmt) {
    let shows = matches!(
        s.kind,
        StmtKind::Expr(_)
            | StmtKind::Assign { .. }
            | StmtKind::OpAssign { .. }
            | StmtKind::IncDec { .. }
            | StmtKind::Command { .. }
    );
    if shows && s.display {
        out.push_str("(show ");
    }
    match &s.kind {
        StmtKind::Expr(e) => expr(out, e),
        StmtKind::Assign {
            lhs,
            bracketed,
            rhs,
        } => {
            out.push_str("(= ");
            if *bracketed {
                out.push('[');
            }
            for (i, l) in lhs.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                match l {
                    LValue::Ignore(_) => out.push('~'),
                    LValue::Expr(e) => expr(out, e),
                }
            }
            if *bracketed {
                out.push(']');
            }
            out.push(' ');
            expr(out, rhs);
            out.push(')');
        }
        StmtKind::OpAssign { lhs, op, rhs } => {
            let _ = write!(out, "({}= ", op.text());
            expr(out, lhs);
            out.push(' ');
            expr(out, rhs);
            out.push(')');
        }
        StmtKind::IncDec { lhs, op } => {
            out.push_str(if *op == BinOp::Add { "(++ " } else { "(-- " });
            expr(out, lhs);
            out.push(')');
        }
        StmtKind::Command { name, args } => {
            let _ = write!(out, "(command {}", name.name);
            for a in args {
                let _ = write!(out, " {:?}", a);
            }
            out.push(')');
        }
        StmtKind::If { clauses, else_body } => {
            out.push_str("(if");
            for c in clauses {
                out.push_str(" (");
                expr(out, &c.cond);
                out.push(' ');
                block(out, &c.body);
                out.push(')');
            }
            if let Some(b) = else_body {
                out.push_str(" (else ");
                block(out, b);
                out.push(')');
            }
            out.push(')');
        }
        StmtKind::For { var, iter, body } => {
            out.push_str("(for ");
            expr(out, var);
            out.push(' ');
            expr(out, iter);
            out.push(' ');
            block(out, body);
            out.push(')');
        }
        StmtKind::ForKeyValue {
            value,
            key,
            iter,
            body,
        } => {
            let _ = write!(out, "(for [{} {}] ", value.name, key.name);
            expr(out, iter);
            out.push(' ');
            block(out, body);
            out.push(')');
        }
        StmtKind::Parfor {
            var,
            iter,
            max_workers,
            body,
        } => {
            let _ = write!(out, "(parfor {} ", var.name);
            expr(out, iter);
            if let Some(m) = max_workers {
                out.push_str(" (workers ");
                expr(out, m);
                out.push(')');
            }
            out.push(' ');
            block(out, body);
            out.push(')');
        }
        StmtKind::While { cond, body } => {
            out.push_str("(while ");
            expr(out, cond);
            out.push(' ');
            block(out, body);
            out.push(')');
        }
        StmtKind::DoUntil { body, cond } => {
            out.push_str("(do ");
            block(out, body);
            out.push(' ');
            expr(out, cond);
            out.push(')');
        }
        StmtKind::Switch {
            subject,
            cases,
            otherwise,
        } => {
            out.push_str("(switch ");
            expr(out, subject);
            for c in cases {
                out.push_str(" (case ");
                expr(out, &c.value);
                out.push(' ');
                block(out, &c.body);
                out.push(')');
            }
            if let Some(b) = otherwise {
                out.push_str(" (otherwise ");
                block(out, b);
                out.push(')');
            }
            out.push(')');
        }
        StmtKind::Try {
            body,
            catch_var,
            catch_body,
        } => {
            out.push_str("(try ");
            block(out, body);
            if let Some(cb) = catch_body {
                out.push_str(" (catch");
                if let Some(v) = catch_var {
                    let _ = write!(out, " {}", v.name);
                }
                out.push(' ');
                block(out, cb);
                out.push(')');
            }
            out.push(')');
        }
        StmtKind::UnwindProtect { body, cleanup } => {
            out.push_str("(unwind_protect ");
            block(out, body);
            out.push_str(" (cleanup ");
            block(out, cleanup);
            out.push_str("))");
        }
        StmtKind::Spmd { body } => {
            out.push_str("(spmd ");
            block(out, body);
            out.push(')');
        }
        StmtKind::Break => out.push_str("(break)"),
        StmtKind::Continue => out.push_str("(continue)"),
        StmtKind::Return => out.push_str("(return)"),
        StmtKind::Decl { kind, names } => {
            out.push_str(match kind {
                DeclKind::Global => "(global",
                DeclKind::Persistent => "(persistent",
            });
            for (n, init) in names {
                match init {
                    None => {
                        let _ = write!(out, " {}", n.name);
                    }
                    Some(e) => {
                        let _ = write!(out, " (= {} ", n.name);
                        expr(out, e);
                        out.push(')');
                    }
                }
            }
            out.push(')');
        }
    }
    if shows && s.display {
        out.push(')');
    }
}

fn params(out: &mut String, ps: &[Param]) {
    out.push('(');
    for (i, p) in ps.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        match p {
            Param::Name(n) => out.push_str(&n.name),
            Param::Ignore(_) => out.push('~'),
            Param::WithDefault { name, default } => {
                let _ = write!(out, "(= {} ", name.name);
                expr(out, default);
                out.push(')');
            }
        }
    }
    out.push(')');
}

fn idents(out: &mut String, ids: &[Ident]) {
    out.push('(');
    for (i, id) in ids.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&id.name);
    }
    out.push(')');
}

fn signature(out: &mut String, name: &Path, outputs: &[Ident], ps: &[Param]) {
    let _ = write!(out, "{} ", name.dotted());
    idents(out, outputs);
    out.push(' ');
    params(out, ps);
}

fn function(out: &mut String, f: &Function) {
    out.push_str("(function ");
    signature(out, &f.name, &f.outputs, &f.params);
    for a in &f.arguments {
        out.push_str(" (arguments");
        attributes(out, &a.attributes);
        for d in &a.decls {
            out.push(' ');
            var_decl(out, d);
        }
        out.push(')');
    }
    out.push(' ');
    block(out, &f.body);
    for n in &f.nested {
        out.push(' ');
        function(out, n);
    }
    if !f.has_end {
        out.push_str(" noend");
    }
    out.push(')');
}

fn attributes(out: &mut String, attrs: &[Attribute]) {
    if attrs.is_empty() {
        return;
    }
    out.push_str(" (attrs");
    for a in attrs {
        out.push(' ');
        if a.negated {
            out.push('~');
        }
        match &a.value {
            None => out.push_str(&a.name.name),
            Some(v) => {
                let _ = write!(out, "(= {} ", a.name.name);
                expr(out, v);
                out.push(')');
            }
        }
    }
    out.push(')');
}

fn var_decl(out: &mut String, d: &VarDecl) {
    let _ = write!(out, "({}", d.name.dotted());
    if let Some(size) = &d.size {
        out.push_str(" (size");
        exprs(out, size);
        out.push(')');
    }
    if let Some(c) = &d.class {
        let _ = write!(out, " (class {})", c.dotted());
    }
    if !d.validators.is_empty() {
        out.push_str(" (validators");
        exprs(out, &d.validators);
        out.push(')');
    }
    if let Some(v) = &d.default {
        out.push_str(" (default ");
        expr(out, v);
        out.push(')');
    }
    out.push(')');
}

fn class(out: &mut String, c: &ClassDef) {
    let _ = write!(out, "(classdef {}", c.name.name);
    attributes(out, &c.attributes);
    if !c.superclasses.is_empty() {
        out.push_str(" (<");
        for s in &c.superclasses {
            let _ = write!(out, " {}", s.dotted());
        }
        out.push(')');
    }
    for s in &c.sections {
        out.push(' ');
        match s {
            ClassSection::Properties {
                attributes: a,
                props,
                ..
            } => {
                out.push_str("(properties");
                attributes(out, a);
                for p in props {
                    out.push(' ');
                    var_decl(out, p);
                }
            }
            ClassSection::Methods {
                attributes: a,
                functions,
                decls,
                ..
            } => {
                out.push_str("(methods");
                attributes(out, a);
                for f in functions {
                    out.push(' ');
                    function(out, f);
                }
                for d in decls {
                    out.push_str(" (declare ");
                    signature(out, &d.name, &d.outputs, &d.params);
                    out.push(')');
                }
            }
            ClassSection::Events {
                attributes: a,
                names,
                ..
            } => {
                out.push_str("(events");
                attributes(out, a);
                for n in names {
                    let _ = write!(out, " {}", n.name);
                }
            }
            ClassSection::Enumeration { members, .. } => {
                out.push_str("(enumeration");
                for m in members {
                    if m.args.is_empty() {
                        let _ = write!(out, " {}", m.name.name);
                    } else {
                        let _ = write!(out, " ({}", m.name.name);
                        exprs(out, &m.args);
                        out.push(')');
                    }
                }
            }
        }
        out.push(')');
    }
    out.push(')');
}

fn unop_text(op: UnOp) -> &'static str {
    match op {
        UnOp::Neg => "neg",
        UnOp::Plus => "uplus",
        UnOp::Not => "not",
        UnOp::CTranspose => "'",
        UnOp::Transpose => ".'",
    }
}

fn rows(out: &mut String, head: &str, rs: &[Vec<Expr>]) {
    let _ = write!(out, "({head}");
    for r in rs {
        out.push_str(" (row");
        exprs(out, r);
        out.push(')');
    }
    out.push(')');
}

fn expr(out: &mut String, e: &Expr) {
    match &e.kind {
        ExprKind::Number { value, imag } => {
            if *imag {
                let _ = write!(out, "(imag {:?})", value);
            } else {
                let _ = write!(out, "{:?}", value);
            }
        }
        ExprKind::Integer { value, suffix } => match suffix {
            Some(s) => {
                let _ = write!(out, "(int {value} {s})");
            }
            None => {
                let _ = write!(out, "(int {value})");
            }
        },
        ExprKind::Char(s) => {
            let _ = write!(out, "(char {:?})", s);
        }
        ExprKind::Str(s) => {
            let _ = write!(out, "(str {:?})", s);
        }
        ExprKind::Ident(n) => out.push_str(n),
        ExprKind::Colon => out.push(':'),
        ExprKind::End => out.push_str("end"),
        ExprKind::Unary { op, operand } => {
            let _ = write!(out, "({} ", unop_text(*op));
            expr(out, operand);
            out.push(')');
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let _ = write!(out, "({} ", op.text());
            expr(out, lhs);
            out.push(' ');
            expr(out, rhs);
            out.push(')');
        }
        ExprKind::Range { start, step, stop } => {
            out.push_str("(range ");
            expr(out, start);
            if let Some(s) = step {
                out.push(' ');
                expr(out, s);
            }
            out.push(' ');
            expr(out, stop);
            out.push(')');
        }
        ExprKind::Paren(inner) => {
            out.push_str("(paren ");
            expr(out, inner);
            out.push(')');
        }
        ExprKind::Index { target, args } => {
            out.push_str("(index ");
            expr(out, target);
            exprs(out, args);
            out.push(')');
        }
        ExprKind::CellIndex { target, args } => {
            out.push_str("(cellindex ");
            expr(out, target);
            exprs(out, args);
            out.push(')');
        }
        ExprKind::Field { target, name } => {
            out.push_str("(. ");
            expr(out, target);
            let _ = write!(out, " {})", name.name);
        }
        ExprKind::DynField { target, name } => {
            out.push_str("(.() ");
            expr(out, target);
            out.push(' ');
            expr(out, name);
            out.push(')');
        }
        ExprKind::Matrix(rs) => rows(out, "matrix", rs),
        ExprKind::Cell(rs) => rows(out, "cell", rs),
        ExprKind::AnonFn { params: ps, body } => {
            out.push_str("(lambda ");
            params(out, ps);
            out.push(' ');
            expr(out, body);
            out.push(')');
        }
        ExprKind::FuncHandle(p) => {
            let _ = write!(out, "(handle {})", p.dotted());
        }
        ExprKind::Metaclass(p) => {
            let _ = write!(out, "(metaclass {})", p.dotted());
        }
        ExprKind::SuperRef { method, superclass } => {
            let _ = write!(out, "(super {} {})", method.name, superclass.dotted());
        }
        ExprKind::Assign { lhs, op, rhs } => {
            match op {
                None => out.push_str("(= "),
                Some(op) => {
                    let _ = write!(out, "({}= ", op.text());
                }
            }
            expr(out, lhs);
            out.push(' ');
            expr(out, rhs);
            out.push(')');
        }
        ExprKind::PostIncDec { target, op } => {
            out.push_str(if *op == BinOp::Add {
                "(post++ "
            } else {
                "(post-- "
            });
            expr(out, target);
            out.push(')');
        }
        ExprKind::NamedArg { name, value } => {
            let _ = write!(out, "(named {} ", name.name);
            expr(out, value);
            out.push(')');
        }
    }
}
