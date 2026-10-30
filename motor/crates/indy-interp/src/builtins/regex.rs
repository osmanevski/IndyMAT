//! Regular expressions (`regexp`, `regexpi`, `regexprep`) and `sscanf`.
//!
//! The engine is an own backtracking matcher over `char` slices: the
//! pattern is parsed into a small tree, compiled into a flat instruction
//! list and run by an iterative machine with an explicit backtrack stack.
//! Recursion only happens for lookaround and atomic groups, whose nesting
//! is bounded by the parser. Every run has a step budget, so pathological
//! patterns end with an error instead of hanging or overflowing the stack.

use indy_value::{Array, Num, StructArray, Value};

use super::cells::is_cellstr;
use super::{arg_str, err, need, one, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "regexp", |_, a, n| regexp_b(a, n, false, "regexp"));
    reg(it, "regexpi", |_, a, n| regexp_b(a, n, true, "regexpi"));
    reg(it, "regexprep", |_, a, _| regexprep_b(a));
    reg(it, "sscanf", |_, a, n| sscanf_b(a, n));
}

// ===========================================================================
// Pattern tree
// ===========================================================================

const MAX_NEST: usize = 128;
const MAX_PROG: usize = 200_000;
const MAX_REPEAT: u32 = 65_535;
const STEP_BASE: u64 = 10_000_000;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Anchor {
    Bol,
    Eol,
    LineStart,
    LineEnd,
    StrStart,
    StrEnd,
    WordB,
    NotWordB,
    WordStart,
    WordEnd,
}

#[derive(Clone, Debug)]
enum Item {
    Range(char, char),
    Digit(bool),
    Word(bool),
    Space(bool),
    Posix(PosixClass, bool),
}

#[derive(Clone, Copy, Debug)]
enum PosixClass {
    Alpha,
    Digit,
    Alnum,
    Space,
    Upper,
    Lower,
    Punct,
    Xdigit,
}

#[derive(Clone, Debug)]
struct ClassSet {
    items: Vec<Item>,
    neg: bool,
}

#[derive(Clone, Debug)]
enum Node {
    Empty,
    Char(char, bool),
    Any(bool),
    Class(usize, bool),
    Anchor(Anchor),
    Group(Box<Node>, Option<usize>),
    Concat(Vec<Node>),
    Alt(Vec<Node>),
    Repeat {
        node: Box<Node>,
        min: u32,
        max: Option<u32>,
        greedy: bool,
        possessive: bool,
    },
    Backref(usize, bool),
    NamedRef(String, bool),
    Look {
        node: Box<Node>,
        ahead: bool,
        neg: bool,
    },
    Atomic(Box<Node>),
}

#[derive(Clone, Copy)]
struct Flags {
    icase: bool,
    dotall: bool,
    multiline: bool,
    extended: bool,
}

/// Compile-time options of one pattern.
#[derive(Clone, Copy)]
pub struct ReOpts {
    pub icase: bool,
    pub dotall: bool,
    pub lineanchors: bool,
    pub freespacing: bool,
}

impl Default for ReOpts {
    fn default() -> ReOpts {
        ReOpts {
            icase: false,
            dotall: true,
            lineanchors: false,
            freespacing: false,
        }
    }
}

struct Parser {
    p: Vec<char>,
    i: usize,
    flags: Flags,
    ngroups: usize,
    names: Vec<(String, usize)>,
    classes: Vec<ClassSet>,
}

fn perr(msg: impl Into<String>) -> Box<indy_value::Error> {
    err("MATLAB:regexp:invalidPattern", msg.into())
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.p.get(self.i).copied()
    }
    fn peek_at(&self, k: usize) -> Option<char> {
        self.p.get(self.i + k).copied()
    }
    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn starts(&self, s: &str) -> bool {
        s.chars()
            .enumerate()
            .all(|(k, c)| self.peek_at(k) == Some(c))
    }

    fn parse_alt(&mut self, depth: usize) -> R<Node> {
        if depth > MAX_NEST {
            return Err(perr("regexp: pattern nesting is too deep."));
        }
        let mut branches = vec![self.parse_seq(depth)?];
        while self.eat('|') {
            branches.push(self.parse_seq(depth)?);
        }
        Ok(if branches.len() == 1 {
            branches.pop().unwrap_or(Node::Empty)
        } else {
            Node::Alt(branches)
        })
    }

    fn skip_space(&mut self) {
        if !self.flags.extended {
            return;
        }
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => self.i += 1,
                Some('#') => {
                    while let Some(c) = self.peek() {
                        self.i += 1;
                        if c == '\n' {
                            break;
                        }
                    }
                }
                _ => return,
            }
        }
    }

    fn parse_seq(&mut self, depth: usize) -> R<Node> {
        let mut items: Vec<Node> = Vec::new();
        loop {
            self.skip_space();
            let Some(c) = self.peek() else { break };
            if c == '|' || c == ')' {
                break;
            }
            if matches!(c, '*' | '+' | '?') {
                return Err(perr(format!(
                    "regexp: nothing to repeat before '{}' at position {} of the expression.",
                    c,
                    self.i + 1
                )));
            }
            if c == '{' && self.quant_brace().is_some() {
                return Err(perr(format!(
                    "regexp: nothing to repeat before '{{' at position {} of the expression.",
                    self.i + 1
                )));
            }
            let atom = match self.parse_atom(depth)? {
                Some(a) => a,
                None => continue,
            };
            let atom = self.parse_quant(atom)?;
            items.push(atom);
        }
        Ok(match items.len() {
            0 => Node::Empty,
            1 => items.pop().unwrap_or(Node::Empty),
            _ => Node::Concat(items),
        })
    }

    /// `{n}`, `{n,}`, `{n,m}`, `{,m}` at the cursor: (min, max, length).
    fn quant_brace(&self) -> Option<(u32, Option<u32>, usize)> {
        if self.peek() != Some('{') {
            return None;
        }
        let mut k = self.i + 1;
        let num = |k: &mut usize| -> Option<u64> {
            let s = *k;
            let mut v: u64 = 0;
            while let Some(d) = self.p.get(*k).and_then(|c| c.to_digit(10)) {
                v = v.saturating_mul(10).saturating_add(d as u64);
                *k += 1;
            }
            if *k > s { Some(v) } else { None }
        };
        let a = num(&mut k);
        let (min, max) = if self.p.get(k) == Some(&',') {
            k += 1;
            let b = num(&mut k);
            if a.is_none() && b.is_none() {
                return None;
            }
            (a.unwrap_or(0), b)
        } else {
            let a = a?;
            (a, Some(a))
        };
        if self.p.get(k) != Some(&'}') {
            return None;
        }
        let clamp = |x: u64| x.min(u32::MAX as u64) as u32;
        Some((clamp(min), max.map(clamp), k + 1 - self.i))
    }

    fn parse_quant(&mut self, atom: Node) -> R<Node> {
        self.skip_space();
        let (min, max) = match self.peek() {
            Some('*') => {
                self.i += 1;
                (0, None)
            }
            Some('+') => {
                self.i += 1;
                (1, None)
            }
            Some('?') => {
                self.i += 1;
                (0, Some(1))
            }
            Some('{') => match self.quant_brace() {
                Some((a, b, len)) => {
                    self.i += len;
                    (a, b)
                }
                None => return Ok(atom),
            },
            _ => return Ok(atom),
        };
        if min > MAX_REPEAT || max.is_some_and(|m| m > MAX_REPEAT) {
            return Err(perr("regexp: repetition count is too large."));
        }
        if max.is_some_and(|m| m < min) {
            return Err(perr("regexp: numbers out of order in {} quantifier."));
        }
        let mut greedy = true;
        let mut possessive = false;
        if self.eat('?') {
            greedy = false;
        } else if self.eat('+') {
            possessive = true;
        }
        if matches!(self.peek(), Some('*' | '+' | '?')) {
            return Err(perr(format!(
                "regexp: nothing to repeat at position {} of the expression.",
                self.i + 1
            )));
        }
        Ok(Node::Repeat {
            node: Box::new(atom),
            min,
            max,
            greedy,
            possessive,
        })
    }

    fn parse_atom(&mut self, depth: usize) -> R<Option<Node>> {
        let Some(c) = self.peek() else {
            return Ok(Some(Node::Empty));
        };
        self.i += 1;
        let f = self.flags;
        Ok(Some(match c {
            '.' => Node::Any(f.dotall),
            '^' => Node::Anchor(if f.multiline {
                Anchor::LineStart
            } else {
                Anchor::Bol
            }),
            '$' => Node::Anchor(if f.multiline {
                Anchor::LineEnd
            } else {
                Anchor::Eol
            }),
            '[' => self.parse_class()?,
            '\\' => self.parse_escape()?,
            '(' => return self.parse_group(depth),
            c => Node::Char(c, f.icase),
        }))
    }

    fn parse_group(&mut self, depth: usize) -> R<Option<Node>> {
        let open_at = self.i;
        let saved = self.flags;
        let mut cap = None;
        let mut kind = 0u8; // 0 group, 1 look, 2 atomic
        let (mut ahead, mut neg) = (true, false);
        if self.eat('?') {
            if self.eat(':') {
            } else if self.eat('=') {
                kind = 1;
            } else if self.eat('!') {
                kind = 1;
                neg = true;
            } else if self.eat('>') {
                kind = 2;
            } else if self.starts("<=") {
                self.i += 2;
                kind = 1;
                ahead = false;
            } else if self.starts("<!") {
                self.i += 2;
                kind = 1;
                ahead = false;
                neg = true;
            } else if self.eat('#') {
                while let Some(c) = self.peek() {
                    self.i += 1;
                    if c == ')' {
                        return Ok(None);
                    }
                }
                return Err(perr("regexp: missing ')' after comment."));
            } else if self.peek() == Some('<') || self.starts("P<") {
                if self.peek() == Some('P') {
                    self.i += 1;
                }
                self.i += 1;
                let name = self.read_name()?;
                if !self.eat('>') {
                    return Err(perr("regexp: syntax error in named token (missing '>')."));
                }
                if self.names.iter().any(|(n, _)| *n == name) {
                    return Err(perr(format!("regexp: duplicate token name '{}'.", name)));
                }
                self.ngroups += 1;
                cap = Some(self.ngroups);
                self.names.push((name, self.ngroups));
            } else {
                // Inline flags: (?imsx-imsx) or (?imsx-imsx: ...)
                let mut on = true;
                let mut fl = self.flags;
                loop {
                    match self.peek() {
                        Some('i') => fl.icase = on,
                        Some('s') => fl.dotall = on,
                        Some('m') => fl.multiline = on,
                        Some('x') => fl.extended = on,
                        Some('-') if on => on = false,
                        Some(')') => {
                            self.i += 1;
                            self.flags = fl;
                            return Ok(None);
                        }
                        Some(':') => {
                            self.i += 1;
                            self.flags = fl;
                            break;
                        }
                        _ => {
                            return Err(perr(format!(
                                "regexp: unrecognised group syntax at position {} of the expression.",
                                open_at
                            )));
                        }
                    }
                    self.i += 1;
                }
            }
        } else {
            self.ngroups += 1;
            cap = Some(self.ngroups);
        }
        let body = self.parse_alt(depth + 1)?;
        if !self.eat(')') {
            return Err(perr(format!(
                "regexp: the '(' at position {} of the expression has no matching ')'.",
                open_at
            )));
        }
        self.flags = saved;
        Ok(Some(match kind {
            1 => Node::Look {
                node: Box::new(body),
                ahead,
                neg,
            },
            2 => Node::Atomic(Box::new(body)),
            _ => Node::Group(Box::new(body), cap),
        }))
    }

    fn read_name(&mut self) -> R<String> {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                s.push(c);
                self.i += 1;
            } else {
                break;
            }
        }
        if s.is_empty() || s.starts_with(|c: char| c.is_ascii_digit()) {
            return Err(perr("regexp: invalid token name."));
        }
        Ok(s)
    }

    /// Escapes that denote one character: `\n`, `\x{41}`, `\.`, ...
    fn char_escape(&mut self, c: char, in_class: bool) -> R<char> {
        Ok(match c {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            'f' => '\x0c',
            'v' => '\x0b',
            'a' => '\x07',
            'e' => '\x1b',
            'b' if in_class => '\x08',
            '0' => self.read_radix(8, Some(3), 0)?,
            'x' => self.read_braced_or(16, 2)?,
            'o' => self.read_braced_or(8, 3)?,
            c => c,
        })
    }

    fn read_braced_or(&mut self, radix: u32, maxlen: usize) -> R<char> {
        if self.eat('{') {
            let c = self.read_radix(radix, None, 0)?;
            if !self.eat('}') {
                return Err(perr("regexp: missing '}' in character code escape."));
            }
            Ok(c)
        } else {
            self.read_radix(radix, Some(maxlen), 0)
        }
    }

    fn read_radix(&mut self, radix: u32, maxlen: Option<usize>, init: u32) -> R<char> {
        let mut v: u32 = init;
        let mut n = 0;
        while let Some(d) = self.peek().and_then(|c| c.to_digit(radix)) {
            if maxlen.is_some_and(|m| n >= m) {
                break;
            }
            v = v.saturating_mul(radix).saturating_add(d);
            if v > 0x10FFFF {
                return Err(perr("regexp: character code is out of range."));
            }
            self.i += 1;
            n += 1;
        }
        char::from_u32(v).ok_or_else(|| perr("regexp: invalid character code."))
    }

    fn parse_escape(&mut self) -> R<Node> {
        let Some(c) = self.peek() else {
            return Err(perr(
                "regexp: the expression ends with an unfinished escape '\\'.",
            ));
        };
        self.i += 1;
        let ic = self.flags.icase;
        let class = |me: &mut Parser, it: Item| {
            me.classes.push(ClassSet {
                items: vec![it],
                neg: false,
            });
            Node::Class(me.classes.len() - 1, ic)
        };
        Ok(match c {
            'd' | 'D' => class(self, Item::Digit(c == 'D')),
            'w' | 'W' => class(self, Item::Word(c == 'W')),
            's' | 'S' => class(self, Item::Space(c == 'S')),
            'b' => Node::Anchor(Anchor::WordB),
            'B' => Node::Anchor(Anchor::NotWordB),
            '<' => Node::Anchor(Anchor::WordStart),
            '>' => Node::Anchor(Anchor::WordEnd),
            'A' => Node::Anchor(Anchor::StrStart),
            'z' | 'Z' => Node::Anchor(Anchor::StrEnd),
            'k' => {
                let close = match self.peek() {
                    Some('<') => '>',
                    Some('{') => '}',
                    Some('\'') => '\'',
                    _ => return Err(perr("regexp: \\k must be followed by <name>.")),
                };
                self.i += 1;
                let name = self.read_name()?;
                if !self.eat(close) {
                    return Err(perr("regexp: syntax error in \\k<name>."));
                }
                Node::NamedRef(name, ic)
            }
            '1'..='9' => {
                let mut n = c.to_digit(10).unwrap_or(1) as usize;
                if let Some(d) = self.peek().and_then(|c| c.to_digit(10)) {
                    let m = n * 10 + d as usize;
                    if m <= self.ngroups {
                        n = m;
                        self.i += 1;
                    }
                }
                Node::Backref(n, ic)
            }
            c => Node::Char(self.char_escape(c, false)?, ic),
        })
    }

    fn parse_class(&mut self) -> R<Node> {
        let start = self.i;
        let mut set = ClassSet {
            items: Vec::new(),
            neg: false,
        };
        if self.eat('^') {
            set.neg = true;
        }
        let mut first = true;
        loop {
            let Some(c) = self.peek() else {
                return Err(perr(format!(
                    "regexp: the '[' at position {} of the expression has no matching ']'.",
                    start
                )));
            };
            if c == ']' && !first {
                self.i += 1;
                break;
            }
            first = false;
            if c == '['
                && self.peek_at(1) == Some(':')
                && let Some(item) = self.posix_class()
            {
                set.items.push(item);
                continue;
            }
            let lo = match self.class_atom()? {
                Ok(ch) => ch,
                Err(item) => {
                    set.items.push(item);
                    continue;
                }
            };
            if self.peek() == Some('-') && self.peek_at(1).is_some_and(|c| c != ']') {
                self.i += 1;
                match self.class_atom()? {
                    Ok(hi) => {
                        if hi < lo {
                            return Err(perr("regexp: invalid range in character class."));
                        }
                        set.items.push(Item::Range(lo, hi));
                    }
                    Err(item) => {
                        set.items.push(Item::Range(lo, lo));
                        set.items.push(Item::Range('-', '-'));
                        set.items.push(item);
                    }
                }
            } else {
                set.items.push(Item::Range(lo, lo));
            }
        }
        self.classes.push(set);
        Ok(Node::Class(self.classes.len() - 1, self.flags.icase))
    }

    fn posix_class(&mut self) -> Option<Item> {
        let names = [
            ("alpha", PosixClass::Alpha),
            ("digit", PosixClass::Digit),
            ("alnum", PosixClass::Alnum),
            ("space", PosixClass::Space),
            ("upper", PosixClass::Upper),
            ("lower", PosixClass::Lower),
            ("punct", PosixClass::Punct),
            ("xdigit", PosixClass::Xdigit),
        ];
        let neg = self.peek_at(2) == Some('^');
        let off = if neg { 3 } else { 2 };
        for (n, pc) in names {
            let len = n.chars().count();
            let ok = n
                .chars()
                .enumerate()
                .all(|(k, c)| self.peek_at(off + k) == Some(c))
                && self.peek_at(off + len) == Some(':')
                && self.peek_at(off + len + 1) == Some(']');
            if ok {
                self.i += off + len + 2;
                return Some(Item::Posix(pc, neg));
            }
        }
        None
    }

    /// One class member: a character (Ok) or a shorthand class (Err).
    fn class_atom(&mut self) -> R<Result<char, Item>> {
        let Some(c) = self.peek() else {
            return Err(perr("regexp: unterminated character class."));
        };
        self.i += 1;
        if c != '\\' {
            return Ok(Ok(c));
        }
        let Some(e) = self.peek() else {
            return Err(perr("regexp: unterminated character class."));
        };
        self.i += 1;
        Ok(match e {
            'd' | 'D' => Err(Item::Digit(e == 'D')),
            'w' | 'W' => Err(Item::Word(e == 'W')),
            's' | 'S' => Err(Item::Space(e == 'S')),
            e => Ok(self.char_escape(e, true)?),
        })
    }
}

// ===========================================================================
// Program
// ===========================================================================

#[derive(Clone, Debug)]
enum Inst {
    Char(char),
    CharIc(char),
    Any,
    AnyNoNl,
    Class(usize, bool),
    Anchor(Anchor),
    Split(usize, usize),
    Jmp(usize),
    Save(usize),
    Backref(usize, bool),
    RepEnter(usize),
    RepCheck(usize),
    Look {
        ahead: bool,
        neg: bool,
        minw: usize,
        maxw: Option<usize>,
        next: usize,
    },
    Atomic {
        next: usize,
    },
    Match,
}

/// A compiled regular expression.
pub struct Regex {
    prog: Vec<Inst>,
    classes: Vec<ClassSet>,
    nregs: usize,
    pub ngroups: usize,
    /// Named groups in order of appearance: (name, group number).
    pub names: Vec<(String, usize)>,
    anchored_start: bool,
}

struct Compiler<'a> {
    prog: Vec<Inst>,
    nregs: usize,
    names: &'a [(String, usize)],
    ngroups: usize,
}

impl Compiler<'_> {
    fn push(&mut self, i: Inst) -> R<usize> {
        if self.prog.len() >= MAX_PROG {
            return Err(perr("regexp: pattern is too large."));
        }
        self.prog.push(i);
        Ok(self.prog.len() - 1)
    }

    fn emit(&mut self, n: &Node) -> R<()> {
        match n {
            Node::Empty => {}
            Node::Char(c, ic) => {
                self.push(if *ic {
                    Inst::CharIc(fold(*c))
                } else {
                    Inst::Char(*c)
                })?;
            }
            Node::Any(all) => {
                self.push(if *all { Inst::Any } else { Inst::AnyNoNl })?;
            }
            Node::Class(k, ic) => {
                self.push(Inst::Class(*k, *ic))?;
            }
            Node::Anchor(a) => {
                self.push(Inst::Anchor(*a))?;
            }
            Node::Group(b, cap) => match cap {
                Some(g) => {
                    self.push(Inst::Save(2 * g))?;
                    self.emit(b)?;
                    self.push(Inst::Save(2 * g + 1))?;
                }
                None => self.emit(b)?,
            },
            Node::Concat(v) => {
                for x in v {
                    self.emit(x)?;
                }
            }
            Node::Alt(v) => {
                let mut jumps = Vec::new();
                for (k, x) in v.iter().enumerate() {
                    if k + 1 < v.len() {
                        let sp = self.push(Inst::Split(0, 0))?;
                        self.emit(x)?;
                        jumps.push(self.push(Inst::Jmp(0))?);
                        let next = self.prog.len();
                        self.prog[sp] = Inst::Split(sp + 1, next);
                    } else {
                        self.emit(x)?;
                    }
                }
                let end = self.prog.len();
                for j in jumps {
                    self.prog[j] = Inst::Jmp(end);
                }
            }
            Node::Repeat {
                node,
                min,
                max,
                greedy,
                possessive,
            } => {
                if *possessive {
                    let at = self.push(Inst::Atomic { next: 0 })?;
                    self.emit_repeat(node, *min, *max, true)?;
                    self.push(Inst::Match)?;
                    let next = self.prog.len();
                    self.prog[at] = Inst::Atomic { next };
                } else {
                    self.emit_repeat(node, *min, *max, *greedy)?;
                }
            }
            Node::Backref(g, ic) => {
                if *g > self.ngroups {
                    return Err(perr(format!(
                        "regexp: reference to non-existent token \\{}.",
                        g
                    )));
                }
                self.push(Inst::Backref(*g, *ic))?;
            }
            Node::NamedRef(name, ic) => {
                let Some((_, g)) = self.names.iter().find(|(n, _)| n == name) else {
                    return Err(perr(format!(
                        "regexp: reference to non-existent token '{}'.",
                        name
                    )));
                };
                self.push(Inst::Backref(*g, *ic))?;
            }
            Node::Look { node, ahead, neg } => {
                let (minw, maxw) = width(node);
                let at = self.push(Inst::Match)?;
                self.emit(node)?;
                self.push(Inst::Match)?;
                let next = self.prog.len();
                self.prog[at] = Inst::Look {
                    ahead: *ahead,
                    neg: *neg,
                    minw,
                    maxw,
                    next,
                };
            }
            Node::Atomic(node) => {
                let at = self.push(Inst::Match)?;
                self.emit(node)?;
                self.push(Inst::Match)?;
                let next = self.prog.len();
                self.prog[at] = Inst::Atomic { next };
            }
        }
        Ok(())
    }

    fn split(&mut self, at: usize, body: usize, out: usize, greedy: bool) {
        self.prog[at] = if greedy {
            Inst::Split(body, out)
        } else {
            Inst::Split(out, body)
        };
    }

    fn emit_repeat(&mut self, node: &Node, min: u32, max: Option<u32>, greedy: bool) -> R<()> {
        for _ in 0..min {
            self.emit(node)?;
        }
        match max {
            None => {
                let r = self.nregs;
                self.nregs += 1;
                let top = self.push(Inst::Split(0, 0))?;
                self.push(Inst::RepEnter(r))?;
                self.emit(node)?;
                self.push(Inst::RepCheck(r))?;
                self.push(Inst::Jmp(top))?;
                let out = self.prog.len();
                self.split(top, top + 1, out, greedy);
            }
            Some(m) => {
                let mut splits = Vec::new();
                for _ in min..m {
                    splits.push(self.push(Inst::Split(0, 0))?);
                    self.emit(node)?;
                }
                let out = self.prog.len();
                for s in splits {
                    self.split(s, s + 1, out, greedy);
                }
            }
        }
        Ok(())
    }
}

/// Minimum and maximum length of what a node can match.
fn width(n: &Node) -> (usize, Option<usize>) {
    match n {
        Node::Empty | Node::Anchor(_) | Node::Look { .. } => (0, Some(0)),
        Node::Char(..) | Node::Any(_) | Node::Class(..) => (1, Some(1)),
        Node::Group(b, _) | Node::Atomic(b) => width(b),
        Node::Concat(v) => v.iter().fold((0, Some(0)), |(a, b), x| {
            let (c, d) = width(x);
            (
                a.saturating_add(c),
                b.zip(d).map(|(b, d)| b.saturating_add(d)),
            )
        }),
        Node::Alt(v) => {
            let mut lo = usize::MAX;
            let mut hi = Some(0usize);
            for x in v {
                let (c, d) = width(x);
                lo = lo.min(c);
                hi = hi.zip(d).map(|(h, d)| h.max(d));
            }
            (if v.is_empty() { 0 } else { lo }, hi)
        }
        Node::Repeat { node, min, max, .. } => {
            let (c, d) = width(node);
            let lo = c.saturating_mul(*min as usize);
            let hi = match (d, max) {
                (Some(0), _) => Some(0),
                (Some(d), Some(m)) => Some(d.saturating_mul(*m as usize)),
                _ => None,
            };
            (lo, hi)
        }
        Node::Backref(..) | Node::NamedRef(..) => (0, None),
    }
}

impl Regex {
    pub fn new(pattern: &str, o: ReOpts) -> R<Regex> {
        let mut p = Parser {
            p: pattern.chars().collect(),
            i: 0,
            flags: Flags {
                icase: o.icase,
                dotall: o.dotall,
                multiline: o.lineanchors,
                extended: o.freespacing,
            },
            ngroups: 0,
            names: Vec::new(),
            classes: Vec::new(),
        };
        let tree = p.parse_alt(0)?;
        if p.i < p.p.len() {
            return Err(perr(format!(
                "regexp: the ')' at position {} of the expression has no matching '('.",
                p.i + 1
            )));
        }
        let anchored_start = starts_anchored(&tree);
        let mut c = Compiler {
            prog: Vec::new(),
            nregs: 0,
            names: &p.names,
            ngroups: p.ngroups,
        };
        c.emit(&tree)?;
        c.push(Inst::Match)?;
        let (prog, nregs) = (c.prog, c.nregs);
        Ok(Regex {
            prog,
            classes: p.classes,
            nregs,
            ngroups: p.ngroups,
            names: p.names,
            anchored_start,
        })
    }
}

fn starts_anchored(n: &Node) -> bool {
    match n {
        Node::Anchor(Anchor::Bol | Anchor::StrStart) => true,
        Node::Concat(v) => v.first().is_some_and(starts_anchored),
        Node::Group(b, _) | Node::Atomic(b) => starts_anchored(b),
        Node::Alt(v) => !v.is_empty() && v.iter().all(starts_anchored),
        _ => false,
    }
}

// ===========================================================================
// Matcher
// ===========================================================================

fn fold(c: char) -> char {
    if c.is_ascii() {
        c.to_ascii_lowercase()
    } else {
        c.to_lowercase().next().unwrap_or(c)
    }
}

fn upper(c: char) -> char {
    if c.is_ascii() {
        c.to_ascii_uppercase()
    } else {
        c.to_uppercase().next().unwrap_or(c)
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c')
}

impl Item {
    fn hit(&self, c: char) -> bool {
        match self {
            Item::Range(a, b) => *a <= c && c <= *b,
            Item::Digit(n) => c.is_ascii_digit() != *n,
            Item::Word(n) => is_word(c) != *n,
            Item::Space(n) => is_space(c) != *n,
            Item::Posix(p, n) => {
                let h = match p {
                    PosixClass::Alpha => c.is_alphabetic(),
                    PosixClass::Digit => c.is_ascii_digit(),
                    PosixClass::Alnum => c.is_alphanumeric(),
                    PosixClass::Space => is_space(c),
                    PosixClass::Upper => c.is_uppercase(),
                    PosixClass::Lower => c.is_lowercase(),
                    PosixClass::Punct => c.is_ascii_punctuation(),
                    PosixClass::Xdigit => c.is_ascii_hexdigit(),
                };
                h != *n
            }
        }
    }
}

impl ClassSet {
    fn matches(&self, c: char, icase: bool) -> bool {
        let test = |c: char| self.items.iter().any(|it| it.hit(c));
        let hit = test(c) || (icase && (test(fold(c)) || test(upper(c))));
        hit != self.neg
    }
}

const UNSET: usize = usize::MAX;

enum Bt {
    Alt(usize, usize),
    Cap(usize, usize),
    Reg(usize, usize),
}

struct Exec<'a> {
    re: &'a Regex,
    text: &'a [char],
    steps: u64,
    limit: u64,
}

impl Exec<'_> {
    fn anchor(&self, a: Anchor, pos: usize) -> bool {
        let t = self.text;
        let prev = if pos > 0 {
            t.get(pos - 1).copied()
        } else {
            None
        };
        let next = t.get(pos).copied();
        let pw = prev.is_some_and(is_word);
        let nw = next.is_some_and(is_word);
        match a {
            Anchor::Bol | Anchor::StrStart => pos == 0,
            Anchor::Eol | Anchor::StrEnd => pos == t.len(),
            Anchor::LineStart => pos == 0 || prev == Some('\n'),
            Anchor::LineEnd => pos == t.len() || next == Some('\n'),
            Anchor::WordB => pw != nw,
            Anchor::NotWordB => pw == nw,
            Anchor::WordStart => !pw && nw,
            Anchor::WordEnd => pw && !nw,
        }
    }

    /// Run the program from `pc` at `pos`; returns the end position.
    fn run(
        &mut self,
        pc0: usize,
        pos0: usize,
        caps: &mut [usize],
        regs: &mut [usize],
        must_end: Option<usize>,
    ) -> R<Option<usize>> {
        let mut stack: Vec<Bt> = Vec::new();
        let (mut pc, mut pos) = (pc0, pos0);
        let text = self.text;
        let re = self.re;
        loop {
            self.steps += 1;
            if self.steps > self.limit {
                return Err(err(
                    "MATLAB:regexp:complexity",
                    "regexp: pattern too complex (step limit exceeded).",
                ));
            }
            let ok = match &re.prog[pc] {
                Inst::Char(c) => {
                    if text.get(pos) == Some(c) {
                        pos += 1;
                        pc += 1;
                        true
                    } else {
                        false
                    }
                }
                Inst::CharIc(c) => {
                    if text.get(pos).is_some_and(|t| fold(*t) == *c) {
                        pos += 1;
                        pc += 1;
                        true
                    } else {
                        false
                    }
                }
                Inst::Any => {
                    if pos < text.len() {
                        pos += 1;
                        pc += 1;
                        true
                    } else {
                        false
                    }
                }
                Inst::AnyNoNl => {
                    if text.get(pos).is_some_and(|c| *c != '\n') {
                        pos += 1;
                        pc += 1;
                        true
                    } else {
                        false
                    }
                }
                Inst::Class(k, ic) => {
                    if text
                        .get(pos)
                        .is_some_and(|c| re.classes[*k].matches(*c, *ic))
                    {
                        pos += 1;
                        pc += 1;
                        true
                    } else {
                        false
                    }
                }
                Inst::Anchor(a) => {
                    pc += 1;
                    self.anchor(*a, pos)
                }
                Inst::Split(a, b) => {
                    stack.push(Bt::Alt(*b, pos));
                    pc = *a;
                    true
                }
                Inst::Jmp(a) => {
                    pc = *a;
                    true
                }
                Inst::Save(s) => {
                    stack.push(Bt::Cap(*s, caps[*s]));
                    caps[*s] = pos;
                    pc += 1;
                    true
                }
                Inst::RepEnter(r) => {
                    stack.push(Bt::Reg(*r, regs[*r]));
                    regs[*r] = pos;
                    pc += 1;
                    true
                }
                Inst::RepCheck(r) => {
                    pc += 1;
                    regs[*r] != pos
                }
                Inst::Backref(g, ic) => {
                    let (s, e) = (caps[2 * g], caps[2 * g + 1]);
                    if s == UNSET || e == UNSET || e < s {
                        false
                    } else {
                        let len = e - s;
                        let fits = pos + len <= text.len()
                            && (0..len).all(|k| {
                                let (a, b) = (text[s + k], text[pos + k]);
                                a == b || (*ic && fold(a) == fold(b))
                            });
                        if fits {
                            pos += len;
                            pc += 1;
                        }
                        fits
                    }
                }
                Inst::Look {
                    ahead,
                    neg,
                    minw,
                    maxw,
                    next,
                } => {
                    let (ahead, neg, minw, maxw, next) = (*ahead, *neg, *minw, *maxw, *next);
                    let snap = caps.to_vec();
                    let mut found = false;
                    if ahead {
                        found = self.run(pc + 1, pos, caps, regs, None)?.is_some();
                    } else if pos >= minw {
                        let lo = maxw.map_or(0, |m| pos.saturating_sub(m));
                        let mut k = pos - minw;
                        loop {
                            if self.run(pc + 1, k, caps, regs, Some(pos))?.is_some() {
                                found = true;
                                break;
                            }
                            if k == lo {
                                break;
                            }
                            k -= 1;
                        }
                    }
                    if found && !neg {
                        for (s, old) in snap.iter().enumerate() {
                            if caps[s] != *old {
                                stack.push(Bt::Cap(s, *old));
                            }
                        }
                    } else if found {
                        caps.copy_from_slice(&snap);
                    }
                    pc = next;
                    found != neg
                }
                Inst::Atomic { next } => {
                    let next = *next;
                    let snap = caps.to_vec();
                    match self.run(pc + 1, pos, caps, regs, None)? {
                        Some(e) => {
                            for (s, old) in snap.iter().enumerate() {
                                if caps[s] != *old {
                                    stack.push(Bt::Cap(s, *old));
                                }
                            }
                            pos = e;
                            pc = next;
                            true
                        }
                        None => false,
                    }
                }
                Inst::Match => {
                    if must_end.is_none_or(|e| e == pos) {
                        return Ok(Some(pos));
                    }
                    false
                }
            };
            if ok {
                continue;
            }
            // Backtrack.
            loop {
                match stack.pop() {
                    None => return Ok(None),
                    Some(Bt::Cap(s, v)) => caps[s] = v,
                    Some(Bt::Reg(r, v)) => regs[r] = v,
                    Some(Bt::Alt(p, q)) => {
                        pc = p;
                        pos = q;
                        break;
                    }
                }
            }
        }
    }
}

/// One match: 0-based half-open extents; `groups[k]` is group k+1.
#[derive(Clone, Debug)]
pub struct Found {
    pub start: usize,
    pub end: usize,
    pub groups: Vec<Option<(usize, usize)>>,
}

impl Regex {
    /// All matches, scanning left to right. Empty matches are kept only
    /// with `emptymatch`; `limit` stops after that many matches.
    pub fn find_all(&self, text: &[char], emptymatch: bool, limit: Option<usize>) -> R<Vec<Found>> {
        let mut ex = Exec {
            re: self,
            text,
            steps: 0,
            limit: STEP_BASE.saturating_add((text.len() as u64).saturating_mul(200)),
        };
        let nslots = 2 * (self.ngroups + 1);
        let mut caps = vec![UNSET; nslots];
        let mut regs = vec![UNSET; self.nregs];
        let mut out = Vec::new();
        let mut from = 0usize;
        while from <= text.len() {
            if limit.is_some_and(|l| out.len() >= l) {
                break;
            }
            let mut hit = None;
            for s in from..=text.len() {
                if self.anchored_start && s > 0 {
                    break;
                }
                caps.fill(UNSET);
                if let Some(e) = ex.run(0, s, &mut caps, &mut regs, None)? {
                    hit = Some((s, e));
                    break;
                }
            }
            let Some((s, e)) = hit else { break };
            if e == s && !emptymatch {
                from = s + 1;
                continue;
            }
            let groups = (1..=self.ngroups)
                .map(|g| {
                    let (a, b) = (caps[2 * g], caps[2 * g + 1]);
                    if a == UNSET || b == UNSET || b < a {
                        None
                    } else {
                        Some((a, b))
                    }
                })
                .collect();
            out.push(Found {
                start: s,
                end: e,
                groups,
            });
            from = if e == s { e + 1 } else { e };
        }
        Ok(out)
    }
}

// ===========================================================================
// regexp / regexpi
// ===========================================================================

#[derive(Clone, Copy, PartialEq)]
enum Out {
    Start,
    End,
    TokExt,
    Match,
    Tokens,
    Names,
    Split,
}

const DEFAULT_ORDER: [Out; 7] = [
    Out::Start,
    Out::End,
    Out::TokExt,
    Out::Match,
    Out::Tokens,
    Out::Names,
    Out::Split,
];

struct MatchOpts {
    re: ReOpts,
    once: bool,
    emptymatch: bool,
}

/// Parse a shared option word; true when it was recognised.
fn common_option(o: &mut MatchOpts, s: &str) -> bool {
    match s {
        "once" => o.once = true,
        "ignorecase" => o.re.icase = true,
        "matchcase" => o.re.icase = false,
        "lineanchors" => o.re.lineanchors = true,
        "stringanchors" => o.re.lineanchors = false,
        "dotall" => o.re.dotall = true,
        "dotexceptnewline" => o.re.dotall = false,
        "freespacing" => o.re.freespacing = true,
        "literalspacing" => o.re.freespacing = false,
        "emptymatch" => o.emptymatch = true,
        "noemptymatch" => o.emptymatch = false,
        "warnings" => {}
        _ => return false,
    }
    true
}

fn chars_of(v: &Value, fname: &str) -> R<Vec<char>> {
    match v {
        Value::Char(_) => Ok(v.to_str().unwrap_or_default().chars().collect()),
        other => Err(err(
            "MATLAB:regexp:invalidInput",
            format!(
                "{}: input must be a character vector or a cell array of character vectors, got {}.",
                fname,
                other.class_name()
            ),
        )),
    }
}

fn substr(t: &[char], a: usize, b: usize) -> Value {
    let a = a.min(t.len());
    let b = b.clamp(a, t.len());
    Value::str(&t[a..b].iter().collect::<String>())
}

fn empty_row() -> Value {
    Value::Num(Num::real(vec![1, 0], vec![]))
}

fn empty_cell_row() -> Value {
    Value::Cell(Array::new(vec![1, 0], vec![]))
}

fn token_list(re: &Regex, f: &Found) -> Vec<Option<(usize, usize)>> {
    if re.ngroups == 0 {
        vec![Some((f.start, f.end))]
    } else {
        f.groups.clone()
    }
}

fn tok_cells(re: &Regex, t: &[char], f: &Found) -> Value {
    Value::cell_row(
        token_list(re, f)
            .iter()
            .map(|g| match g {
                Some((a, b)) => substr(t, *a, *b),
                None => Value::str(""),
            })
            .collect(),
    )
}

fn tok_extents(re: &Regex, f: &Found) -> Value {
    let toks = token_list(re, f);
    let k = toks.len();
    let mut data = vec![0.0; 2 * k];
    for (i, g) in toks.iter().enumerate() {
        let (a, b) = match g {
            Some((a, b)) => (*a as f64 + 1.0, *b as f64),
            None => (f.start as f64 + 1.0, f.start as f64),
        };
        data[i] = a;
        data[i + k] = b;
    }
    Value::matrix(vec![k, 2], data)
}

fn names_struct(re: &Regex, t: &[char], found: &[Found]) -> Value {
    if re.names.is_empty() {
        return Value::Struct(StructArray::new_scalar());
    }
    let fields: Vec<String> = re.names.iter().map(|(n, _)| n.clone()).collect();
    let dims = match found.len() {
        0 => vec![0, 0],
        n => vec![1, n],
    };
    let mut s = StructArray::empty_with(fields, dims);
    for (i, f) in found.iter().enumerate() {
        for (k, (_, g)) in re.names.iter().enumerate() {
            s.elems[i][k] = match f.groups.get(g - 1).copied().flatten() {
                Some((a, b)) => substr(t, a, b),
                None => Value::str(""),
            };
        }
    }
    Value::Struct(s)
}

fn one_output(which: Out, re: &Regex, t: &[char], found: &[Found], once: bool) -> Value {
    match which {
        Out::Start | Out::End => {
            let v: Vec<f64> = found
                .iter()
                .map(|f| {
                    if which == Out::Start {
                        f.start as f64 + 1.0
                    } else {
                        f.end as f64
                    }
                })
                .collect();
            if once {
                v.first().map_or_else(Value::empty, |x| Value::num(*x))
            } else if v.is_empty() {
                empty_row()
            } else {
                Value::row(v)
            }
        }
        Out::TokExt => {
            if once {
                found
                    .first()
                    .map_or_else(Value::empty, |f| tok_extents(re, f))
            } else if found.is_empty() {
                empty_cell_row()
            } else {
                Value::cell_row(found.iter().map(|f| tok_extents(re, f)).collect())
            }
        }
        Out::Match => {
            if once {
                found
                    .first()
                    .map_or_else(|| Value::str(""), |f| substr(t, f.start, f.end))
            } else if found.is_empty() {
                empty_cell_row()
            } else {
                Value::cell_row(found.iter().map(|f| substr(t, f.start, f.end)).collect())
            }
        }
        Out::Tokens => {
            if once {
                found
                    .first()
                    .map_or_else(Value::empty_cell, |f| tok_cells(re, t, f))
            } else if found.is_empty() {
                empty_cell_row()
            } else {
                Value::cell_row(found.iter().map(|f| tok_cells(re, t, f)).collect())
            }
        }
        Out::Names => names_struct(
            re,
            t,
            if once {
                &found[..found.len().min(1)]
            } else {
                found
            },
        ),
        Out::Split => {
            let mut parts = Vec::new();
            let mut prev = 0;
            for f in found {
                parts.push(substr(t, prev, f.start));
                prev = f.end;
            }
            parts.push(substr(t, prev, t.len()));
            Value::cell_row(parts)
        }
    }
}

/// Texts of an argument and, for a cell input, its dims.
type TextList = (Vec<Vec<char>>, Option<Vec<usize>>);

/// Element list and shape of a text argument (char row or cellstr).
fn text_list(v: &Value, fname: &str) -> R<TextList> {
    match v {
        Value::Cell(c) => {
            if !is_cellstr(v) {
                return Err(err(
                    "MATLAB:regexp:invalidInput",
                    format!("{}: cell array elements must be character vectors.", fname),
                ));
            }
            let l = c
                .data
                .iter()
                .map(|x| chars_of(x, fname))
                .collect::<R<Vec<_>>>()?;
            Ok((l, Some(c.dims.clone())))
        }
        other => Ok((vec![chars_of(other, fname)?], None)),
    }
}

fn regexp_b(a: Vec<Value>, nargout: usize, icase: bool, fname: &str) -> R<Vec<Value>> {
    need(&a, 2, fname)?;
    let mut o = MatchOpts {
        re: ReOpts {
            icase,
            ..ReOpts::default()
        },
        once: false,
        emptymatch: false,
    };
    let mut order: Vec<Out> = Vec::new();
    let mut force_cell = false;
    for v in &a[2..] {
        let s = arg_str(v, fname)?.to_lowercase();
        let sel = match s.as_str() {
            "start" => Some(Out::Start),
            "end" => Some(Out::End),
            "tokenextents" => Some(Out::TokExt),
            "match" => Some(Out::Match),
            "tokens" => Some(Out::Tokens),
            "names" => Some(Out::Names),
            "split" => Some(Out::Split),
            _ => None,
        };
        if let Some(x) = sel {
            order.push(x);
        } else if s == "forcecelloutput" {
            force_cell = true;
        } else if !common_option(&mut o, &s) {
            return Err(err(
                "MATLAB:regexp:unknownOption",
                format!("{}: unknown option '{}'.", fname, s),
            ));
        }
    }
    for d in DEFAULT_ORDER {
        if !order.contains(&d) {
            order.push(d);
        }
    }
    let nout = nargout.clamp(1, order.len());
    let order = &order[..nout];
    let (strs, sdims) = text_list(&a[0], fname)?;
    let (pats, pdims) = match &a[1] {
        Value::Cell(_) => {
            let (l, d) = text_list(&a[1], fname)?;
            (
                l.into_iter()
                    .map(|p| p.into_iter().collect::<String>())
                    .collect::<Vec<_>>(),
                d,
            )
        }
        v => (vec![arg_str(v, fname)?], None),
    };
    let outdims = match (&sdims, &pdims) {
        (None, None) => None,
        (Some(d), None) | (None, Some(d)) => Some(d.clone()),
        (Some(d), Some(e)) => {
            if strs.len() == 1 {
                Some(e.clone())
            } else if pats.len() == 1 || d == e {
                Some(d.clone())
            } else {
                return Err(err(
                    "MATLAB:regexp:cellMismatch",
                    format!("{}: cell array inputs must have the same size.", fname),
                ));
            }
        }
    };
    let n = strs.len().max(pats.len());
    let mut compiled: Vec<Regex> = Vec::with_capacity(pats.len());
    for p in &pats {
        compiled.push(Regex::new(p, o.re)?);
    }
    let mut cols: Vec<Vec<Value>> = vec![Vec::with_capacity(n); order.len()];
    for i in 0..n {
        let t = &strs[if strs.len() == 1 { 0 } else { i }];
        let re = &compiled[if compiled.len() == 1 { 0 } else { i }];
        let found = re.find_all(t, o.emptymatch, if o.once { Some(1) } else { None })?;
        for (k, w) in order.iter().enumerate() {
            cols[k].push(one_output(*w, re, t, &found, o.once));
        }
    }
    Ok(cols
        .into_iter()
        .map(|mut c| match &outdims {
            None if !force_cell => c.pop().unwrap_or_default(),
            None => Value::Cell(Array::scalar(c.pop().unwrap_or_default())),
            Some(d) => Value::Cell(Array::new(d.clone(), c)),
        })
        .collect())
}

// ===========================================================================
// regexprep
// ===========================================================================

enum RepPart {
    Lit(String),
    Group(usize),
    Named(String),
}

fn parse_rep(r: &[char]) -> Vec<RepPart> {
    let mut parts = Vec::new();
    let mut lit = String::new();
    let mut i = 0;
    while i < r.len() {
        let c = r[i];
        if c == '\\' && i + 1 < r.len() {
            let e = r[i + 1];
            lit.push(match e {
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                'f' => '\x0c',
                'v' => '\x0b',
                'a' => '\x07',
                e => e,
            });
            i += 2;
            continue;
        }
        if c == '$' && i + 1 < r.len() {
            if let Some(d) = r[i + 1].to_digit(10) {
                parts.push(RepPart::Lit(std::mem::take(&mut lit)));
                parts.push(RepPart::Group(d as usize));
                i += 2;
                continue;
            }
            if r[i + 1] == '<'
                && let Some(off) = r[i + 2..].iter().position(|c| *c == '>')
            {
                parts.push(RepPart::Lit(std::mem::take(&mut lit)));
                parts.push(RepPart::Named(r[i + 2..i + 2 + off].iter().collect()));
                i += off + 3;
                continue;
            }
        }
        lit.push(c);
        i += 1;
    }
    parts.push(RepPart::Lit(lit));
    parts
}

fn replace_one(
    re: &Regex,
    t: &[char],
    rep: &[RepPart],
    o: &MatchOpts,
    nth: Option<usize>,
) -> R<Vec<char>> {
    let limit = nth.or(if o.once { Some(1) } else { None });
    let found = re.find_all(t, o.emptymatch, limit)?;
    let mut out: Vec<char> = Vec::with_capacity(t.len());
    let mut prev = 0;
    for (k, f) in found.iter().enumerate() {
        if let Some(n) = nth
            && k + 1 != n
        {
            continue;
        }
        out.extend_from_slice(&t[prev..f.start]);
        for p in rep {
            let span = match p {
                RepPart::Lit(s) => {
                    out.extend(s.chars());
                    None
                }
                RepPart::Group(0) => Some((f.start, f.end)),
                RepPart::Group(g) => f.groups.get(g - 1).copied().flatten(),
                RepPart::Named(name) => re
                    .names
                    .iter()
                    .find(|(n, _)| n == name)
                    .and_then(|(_, g)| f.groups.get(g - 1).copied().flatten()),
            };
            if let Some((a, b)) = span {
                out.extend_from_slice(&t[a..b]);
            }
        }
        prev = f.end;
    }
    out.extend_from_slice(&t[prev..]);
    Ok(out)
}

fn regexprep_b(a: Vec<Value>) -> R<Vec<Value>> {
    let fname = "regexprep";
    need(&a, 3, fname)?;
    let mut o = MatchOpts {
        re: ReOpts::default(),
        once: false,
        emptymatch: false,
    };
    let mut nth = None;
    for v in &a[3..] {
        if let Value::Num(_) = v {
            let x = v.scalar_f64().unwrap_or(0.0);
            if !(x >= 1.0 && x.fract() == 0.0 && x.is_finite()) {
                return Err(err(
                    "MATLAB:regexprep:invalidN",
                    "regexprep: N must be a positive integer.",
                ));
            }
            nth = Some(x as usize);
            continue;
        }
        let s = arg_str(v, fname)?.to_lowercase();
        if s != "preservecase" && !common_option(&mut o, &s) {
            return Err(err(
                "MATLAB:regexprep:unknownOption",
                format!("regexprep: unknown option '{}'.", s),
            ));
        }
    }
    let (strs, sdims) = text_list(&a[0], fname)?;
    let (pats, _) = text_list(&a[1], fname)?;
    let (reps, _) = text_list(&a[2], fname)?;
    if reps.len() != 1 && reps.len() != pats.len() {
        return Err(err(
            "MATLAB:regexprep:repMismatch",
            "regexprep: the replacement cell array must match the expression cell array in size.",
        ));
    }
    let mut compiled = Vec::with_capacity(pats.len());
    for (k, p) in pats.iter().enumerate() {
        let r = &reps[if reps.len() == 1 { 0 } else { k }];
        compiled.push((
            Regex::new(&p.iter().collect::<String>(), o.re)?,
            parse_rep(r),
        ));
    }
    let mut outs = Vec::with_capacity(strs.len());
    for s in strs {
        let mut cur = s;
        for (re, rep) in &compiled {
            cur = replace_one(re, &cur, rep, &o, nth)?;
        }
        outs.push(Value::str(&cur.into_iter().collect::<String>()));
    }
    match sdims {
        None => one(outs.pop().unwrap_or_default()),
        Some(d) => one(Value::Cell(Array::new(d, outs))),
    }
}

// ===========================================================================
// sscanf
// ===========================================================================

#[derive(Debug)]
enum Dir {
    Ws,
    Lit(char),
    Conv {
        kind: char,
        width: Option<usize>,
        skip: bool,
        set: Option<ClassSet>,
    },
}

fn parse_format(f: &[char]) -> R<Vec<Dir>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < f.len() {
        let c = f[i];
        if c.is_whitespace() {
            while i < f.len() && f[i].is_whitespace() {
                i += 1;
            }
            out.push(Dir::Ws);
            continue;
        }
        if c != '%' {
            out.push(Dir::Lit(c));
            i += 1;
            continue;
        }
        i += 1;
        if f.get(i) == Some(&'%') {
            out.push(Dir::Lit('%'));
            i += 1;
            continue;
        }
        let skip = f.get(i) == Some(&'*');
        if skip {
            i += 1;
        }
        let mut width = None;
        while let Some(d) = f.get(i).and_then(|c| c.to_digit(10)) {
            width = Some(
                width
                    .unwrap_or(0usize)
                    .saturating_mul(10)
                    .saturating_add(d as usize),
            );
            i += 1;
        }
        while matches!(f.get(i), Some('l' | 'h' | 'L')) {
            i += 1;
        }
        let Some(&k) = f.get(i) else {
            return Err(err(
                "MATLAB:sscanf:invalidFormat",
                "sscanf: invalid format specifier.",
            ));
        };
        i += 1;
        let set = if k == '[' {
            let mut s = ClassSet {
                items: Vec::new(),
                neg: false,
            };
            if f.get(i) == Some(&'^') {
                s.neg = true;
                i += 1;
            }
            let mut first = true;
            loop {
                let Some(&c) = f.get(i) else {
                    return Err(err(
                        "MATLAB:sscanf:invalidFormat",
                        "sscanf: unterminated %[ in format.",
                    ));
                };
                i += 1;
                if c == ']' && !first {
                    break;
                }
                first = false;
                if f.get(i) == Some(&'-') && f.get(i + 1).is_some_and(|c| *c != ']') {
                    let hi = f[i + 1];
                    s.items.push(Item::Range(c.min(hi), c.max(hi)));
                    i += 2;
                } else {
                    s.items.push(Item::Range(c, c));
                }
            }
            Some(s)
        } else {
            if !"diufegEGxXosc".contains(k) {
                return Err(err(
                    "MATLAB:sscanf:invalidFormat",
                    format!("sscanf: unsupported conversion '%{}'.", k),
                ));
            }
            None
        };
        out.push(Dir::Conv {
            kind: k,
            width,
            skip,
            set,
        });
    }
    Ok(out)
}

/// Parse a number at the start of `t` (already width-limited).
fn scan_number(t: &[char], kind: char) -> Option<(f64, usize)> {
    let mut i = 0;
    let mut sign = 1.0;
    if let Some(c) = t.first()
        && (*c == '+' || *c == '-')
    {
        if *c == '-' {
            sign = -1.0;
        }
        i = 1;
    }
    let digits = |i: &mut usize, radix: u32| -> (f64, usize) {
        let s = *i;
        let mut v = 0.0;
        while let Some(d) = t.get(*i).and_then(|c| c.to_digit(radix)) {
            v = v * radix as f64 + d as f64;
            *i += 1;
        }
        (v, *i - s)
    };
    match kind {
        'd' | 'u' => {
            let (v, n) = digits(&mut i, 10);
            (n > 0).then_some((sign * v, i))
        }
        'x' | 'X' => {
            if t.get(i) == Some(&'0')
                && matches!(t.get(i + 1), Some('x' | 'X'))
                && t.get(i + 2).is_some_and(|c| c.is_ascii_hexdigit())
            {
                i += 2;
            }
            let (v, n) = digits(&mut i, 16);
            (n > 0).then_some((sign * v, i))
        }
        'o' => {
            let (v, n) = digits(&mut i, 8);
            (n > 0).then_some((sign * v, i))
        }
        'i' => {
            if t.get(i) == Some(&'0')
                && matches!(t.get(i + 1), Some('x' | 'X'))
                && t.get(i + 2).is_some_and(|c| c.is_ascii_hexdigit())
            {
                i += 2;
                let (v, _) = digits(&mut i, 16);
                return Some((sign * v, i));
            }
            let radix = if t.get(i) == Some(&'0') { 8 } else { 10 };
            let (v, n) = digits(&mut i, radix);
            (n > 0).then_some((sign * v, i))
        }
        _ => {
            let rest: String = t[i..].iter().take(8).collect::<String>().to_lowercase();
            for (word, val) in [
                ("infinity", f64::INFINITY),
                ("inf", f64::INFINITY),
                ("nan", f64::NAN),
            ] {
                if rest.starts_with(word) {
                    return Some((sign * val, i + word.len()));
                }
            }
            let s = i;
            let mut nd = 0;
            while t.get(i).is_some_and(|c| c.is_ascii_digit()) {
                i += 1;
                nd += 1;
            }
            if t.get(i) == Some(&'.') {
                i += 1;
                while t.get(i).is_some_and(|c| c.is_ascii_digit()) {
                    i += 1;
                    nd += 1;
                }
            }
            if nd == 0 {
                return None;
            }
            if matches!(t.get(i), Some('e' | 'E' | 'd' | 'D')) {
                let mut j = i + 1;
                if matches!(t.get(j), Some('+' | '-')) {
                    j += 1;
                }
                if t.get(j).is_some_and(|c| c.is_ascii_digit()) {
                    while t.get(j).is_some_and(|c| c.is_ascii_digit()) {
                        j += 1;
                    }
                    i = j;
                }
            }
            let txt: String = t[s..i]
                .iter()
                .map(|c| if matches!(c, 'd' | 'D') { 'e' } else { *c })
                .collect();
            txt.parse::<f64>().ok().map(|v| (sign * v, i))
        }
    }
}

pub(super) fn sscanf_b(a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "sscanf")?;
    let text: Vec<char> = match &a[0] {
        Value::Char(c) => c.data.clone(),
        other => {
            return Err(err(
                "MATLAB:sscanf:invalidInput",
                format!(
                    "sscanf: first argument must be a character vector, got {}.",
                    other.class_name()
                ),
            ));
        }
    };
    let fmt: Vec<char> = arg_str(&a[1], "sscanf")?.chars().collect();
    let dirs = parse_format(&fmt)?;
    // Size argument: n, Inf, [m n], [m Inf].
    let mut shape: Option<(usize, Option<usize>)> = None;
    let mut max = usize::MAX;
    if let Some(sz) = a.get(2) {
        let v = sz
            .to_f64_vec()
            .ok_or_else(|| err("MATLAB:sscanf:invalidSize", "sscanf: size must be numeric."))?;
        let dim = |x: f64| -> R<Option<usize>> {
            if x == f64::INFINITY {
                Ok(None)
            } else if x >= 0.0 && x.fract() == 0.0 {
                Ok(Some(x as usize))
            } else {
                Err(err(
                    "MATLAB:sscanf:invalidSize",
                    "sscanf: size must be non-negative integers or Inf.",
                ))
            }
        };
        match v.len() {
            1 => {
                if let Some(n) = dim(v[0])? {
                    max = n;
                }
            }
            2 => {
                let m = dim(v[0])?.ok_or_else(|| {
                    err(
                        "MATLAB:sscanf:invalidSize",
                        "sscanf: the row count cannot be Inf.",
                    )
                })?;
                let n = dim(v[1])?;
                if let Some(n) = n {
                    max = m.saturating_mul(n);
                }
                shape = Some((m, n));
            }
            _ => {
                return Err(err(
                    "MATLAB:sscanf:invalidSize",
                    "sscanf: size must be a scalar or a 2-element vector.",
                ));
            }
        }
    }
    let nconv = dirs
        .iter()
        .filter(|d| matches!(d, Dir::Conv { skip: false, .. }))
        .count();
    let all_text = nconv > 0
        && dirs.iter().all(|d| match d {
            Dir::Conv { kind, skip, .. } => *skip || matches!(kind, 's' | 'c' | '['),
            _ => true,
        });
    let mut out: Vec<f64> = Vec::new();
    let mut count = 0usize;
    let mut errmsg = String::new();
    let mut pos = 0usize;
    let n = text.len();
    let skip_ws = |pos: &mut usize| {
        while *pos < n && text[*pos].is_whitespace() {
            *pos += 1;
        }
    };
    'outer: loop {
        let pass_start = pos;
        let mut any_conv = false;
        for d in &dirs {
            if out.len() >= max {
                break 'outer;
            }
            match d {
                Dir::Ws => skip_ws(&mut pos),
                Dir::Lit(c) => {
                    if pos < n && text[pos] == *c {
                        pos += 1;
                    } else {
                        break 'outer;
                    }
                }
                Dir::Conv {
                    kind,
                    width,
                    skip,
                    set,
                } => {
                    any_conv = true;
                    if !matches!(kind, 'c' | '[') {
                        skip_ws(&mut pos);
                    }
                    if pos >= n {
                        break 'outer;
                    }
                    let lim = width.map_or(n, |w| pos.saturating_add(w).min(n));
                    let mut vals: Vec<f64> = Vec::new();
                    match kind {
                        's' => {
                            while pos < lim && !text[pos].is_whitespace() {
                                vals.push(text[pos] as u32 as f64);
                                pos += 1;
                            }
                        }
                        'c' => {
                            let w = width.unwrap_or(1);
                            let end = pos.saturating_add(w).min(n);
                            vals.extend(text[pos..end].iter().map(|c| *c as u32 as f64));
                            pos = end;
                        }
                        '[' => {
                            let st = set.as_ref();
                            while pos < lim && st.is_some_and(|s| s.matches(text[pos], false)) {
                                vals.push(text[pos] as u32 as f64);
                                pos += 1;
                            }
                            if vals.is_empty() {
                                errmsg = "sscanf: format failed to match".into();
                                break 'outer;
                            }
                        }
                        k => match scan_number(&text[pos..lim], *k) {
                            Some((v, used)) => {
                                vals.push(v);
                                pos += used;
                            }
                            None => {
                                errmsg = "sscanf: format failed to match".into();
                                break 'outer;
                            }
                        },
                    }
                    if !*skip {
                        let room = max - out.len();
                        out.extend(vals.into_iter().take(room));
                        count += 1;
                    }
                }
            }
        }
        if !any_conv || pos >= n || pos == pass_start {
            break;
        }
    }
    let k = out.len();
    let result = if all_text {
        Value::Char(Array::row(
            out.iter()
                .map(|x| char::from_u32(*x as u32).unwrap_or('\0'))
                .collect(),
        ))
    } else {
        match shape {
            None => Value::col(out),
            Some((m, ncols)) => {
                let cols = match ncols {
                    Some(c) if m.saturating_mul(c) <= k => c,
                    _ => {
                        if m == 0 {
                            0
                        } else {
                            k.div_ceil(m)
                        }
                    }
                };
                out.resize(m * cols, 0.0);
                Value::matrix(vec![m, cols], out)
            }
        }
    };
    let mut outs = vec![
        result,
        Value::num(count as f64),
        Value::str(&errmsg),
        Value::num(pos as f64 + 1.0),
    ];
    outs.truncate(nargout.clamp(1, 4));
    Ok(outs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(p: &str, t: &str) -> Vec<(usize, usize)> {
        let re = Regex::new(p, ReOpts::default()).unwrap();
        let t: Vec<char> = t.chars().collect();
        re.find_all(&t, false, None)
            .unwrap()
            .iter()
            .map(|f| (f.start, f.end))
            .collect()
    }

    #[test]
    fn engine_basics() {
        assert_eq!(find(r"\d+", "ab12cd3"), vec![(2, 4), (6, 7)]);
        assert_eq!(find("a|bc", "xbca"), vec![(1, 3), (3, 4)]);
        assert_eq!(find("a{2,3}", "aaaaaaa"), vec![(0, 3), (3, 6)]);
        assert_eq!(find("a+?", "aaa"), vec![(0, 1), (1, 2), (2, 3)]);
        assert_eq!(find(r"(a)\1", "aaxaa"), vec![(0, 2), (3, 5)]);
        assert_eq!(find(r"(?<=\$)\d+", "a$12 7"), vec![(2, 4)]);
        assert_eq!(find(r"\w+(?=!)", "hi! yo"), vec![(0, 2)]);
        assert_eq!(find(r"(?i)abc", "xABC"), vec![(1, 4)]);
        assert_eq!(find(r"a*+a", "aaa"), vec![]);
        assert_eq!(find(r"\bfoo\b", "foo food"), vec![(0, 3)]);
    }

    #[test]
    fn engine_robust() {
        for p in [
            "(",
            ")",
            "[a",
            "a**",
            "*",
            r"\",
            "(?<n>a)(?<n>b)",
            "a{3,1}",
            r"\k<x>",
            "(?z)",
            r"\2(a)",
        ] {
            assert!(Regex::new(p, ReOpts::default()).is_err(), "{}", p);
        }
        let re = Regex::new("(a*)*b", ReOpts::default()).unwrap();
        let t: Vec<char> = "a".repeat(40).chars().collect();
        let _ = re.find_all(&t, false, None);
        let re = Regex::new("(a|aa)*c", ReOpts::default()).unwrap();
        let t: Vec<char> = "a".repeat(60).chars().collect();
        assert!(re.find_all(&t, false, None).is_err());
        let deep = "(".repeat(5000) + &")".repeat(5000);
        assert!(Regex::new(&deep, ReOpts::default()).is_err());
        assert!(Regex::new("a{65535}{65535}", ReOpts::default()).is_err());
    }

    #[test]
    fn fuzz_no_panic() {
        let alphabet: Vec<char> = r"ab.*+?|()[]^-${}\\dwsk<>=!:,1209iP#x".chars().collect();
        let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let text: Vec<char> = "ab1 ba\nAB_x-[]".chars().collect();
        for _ in 0..3000 {
            let len = (next() % 14) as usize;
            let p: String = (0..len)
                .map(|_| alphabet[(next() % alphabet.len() as u64) as usize])
                .collect();
            if let Ok(re) = Regex::new(&p, ReOpts::default()) {
                let _ = re.find_all(&text, true, None);
                let _ = re.find_all(&text, false, Some(2));
            }
        }
        for f in ["%", "%[", "%5", "%*", "%[^", "%q", "%d%", "%%%d"] {
            let _ = sscanf_b(vec![Value::str("12 ab"), Value::str(f)], 3);
        }
    }
}
