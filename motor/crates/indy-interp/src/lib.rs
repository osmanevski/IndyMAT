//! Tree-walking interpreter for MATLAB/Octave code.
//!
//! Design (phase 2): the AST from `indy-syntax` is executed directly.
//! Each function call gets a [`Frame`] (variables in a hash map); scripts
//! run in the caller's frame. Files are parsed one at a time and cached;
//! a file that fails to parse only fails calls to that file (rule 1).
//! Errors carry a MATLAB identifier, a char message and a stack and are
//! always catchable by `try/catch` (rule 3).

#![allow(clippy::needless_range_loop, clippy::too_many_arguments)]

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use indy_syntax::ast::{self, Function};
use indy_syntax::{Dialect, Span, parse_file};
pub use indy_value::{Error, Res, StackFrame, Value};

mod assign;
pub mod builtins;
pub mod classes;
pub mod debug;
mod eval;

pub type R<T> = Result<T, Box<Error>>;

/// Control flow out of a statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Normal,
    Break,
    Continue,
    Return,
}

/// A parsed source file and its local functions.
pub struct FileCtx {
    pub path: PathBuf,
    /// File stem (`foo` for `foo.m`); for `eval` text a pseudo name.
    pub name: String,
    pub src: Rc<str>,
    line_starts: Vec<u32>,
    pub file: ast::File,
    /// Primary + local + nested functions by name (nested flattened).
    pub functions: HashMap<String, Rc<Function>>,
    /// First function of a function file.
    pub main: Option<Rc<Function>>,
    /// Script statements (scripts only).
    pub stmts: Rc<Vec<ast::Stmt>>,
    pub is_script: bool,
    /// Parsed as MATLAB (`"..."` literals are strings).
    pub matlab: bool,
    /// The text is the content of the file at `path` (not Command Window,
    /// section or `eval` text): its lines are file lines (debugger).
    pub file_backed: bool,
    /// Anonymous function bodies by source position. It lives here because a
    /// finished `eval` frees its context and the next one may get the same
    /// address: a table keyed by address would hand out the old body.
    pub(crate) anon_bodies: RefCell<HashMap<u32, Rc<ast::Expr>>>,
}

impl FileCtx {
    pub fn line_col(&self, offset: u32) -> (usize, usize) {
        let line = match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let col = offset - self.line_starts[line];
        (line + 1, col as usize + 1)
    }
    pub fn text(&self, span: Span) -> &str {
        self.src
            .get(span.start as usize..span.end as usize)
            .unwrap_or("")
    }
    pub fn dir(&self) -> Option<&Path> {
        self.path.parent()
    }
}

/// Octave-only block words used as code (a MATLAB parse would read them
/// as identifiers).
fn has_octave_words(src: &str) -> bool {
    const WORDS: [&str; 10] = [
        "endfunction",
        "endif",
        "endwhile",
        "endfor",
        "endswitch",
        "end_try_catch",
        "end_unwind_protect",
        "unwind_protect",
        "endparfor",
        "endclassdef",
    ];
    src.lines().any(|l| {
        let code = l.split('%').next().unwrap_or("");
        code.split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .any(|w| WORDS.contains(&w))
    })
}

fn line_starts(src: &str) -> Vec<u32> {
    let mut v = vec![0u32];
    for (i, b) in src.bytes().enumerate() {
        if b == b'\n' {
            v.push(i as u32 + 1);
        }
    }
    v
}

fn collect_functions(f: &Function, out: &mut HashMap<String, Rc<Function>>) {
    out.entry(f.name.dotted())
        .or_insert_with(|| Rc::new(f.clone()));
    for n in &f.nested {
        collect_functions(n, out);
    }
}

/// Build a context from source text.
pub fn load_source(path: PathBuf, name: String, src: &str) -> R<Rc<FileCtx>> {
    build_source(path, name, src, false)
}

/// Build a context from the full text of the file at `path` (breakpoints
/// and the debugger's call stack refer to its lines).
pub fn load_file_source(path: PathBuf, name: String, src: &str) -> R<Rc<FileCtx>> {
    build_source(path, name, src, true)
}

/// Read and parse the file at `path`.
pub(crate) fn read_source_file(path: &Path) -> R<Rc<FileCtx>> {
    match std::fs::read(path) {
        Ok(bytes) => {
            let src = match String::from_utf8(bytes) {
                Ok(s) => s,
                Err(e) => e.into_bytes().iter().map(|b| *b as char).collect(),
            };
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            load_file_source(path.to_path_buf(), name, &src)
        }
        Err(e) => Err(Error::new(
            "MATLAB:fileNotFound",
            format!("cannot read {}: {}", path.display(), e),
        )),
    }
}

fn build_source(path: PathBuf, name: String, src: &str, file_backed: bool) -> R<Rc<FileCtx>> {
    // A file that is valid MATLAB gets MATLAB semantics ("..." is a string
    // scalar, no backslash escapes); otherwise it is Octave code.
    let m = parse_file(src, Dialect::Matlab);
    let matlab = m.ok() && !has_octave_words(src);
    let res = if matlab {
        m
    } else {
        parse_file(src, Dialect::Octave)
    };
    let Some(file) = res.file else {
        let d = &res.diagnostics[0];
        return Err(Error::new(
            "Octave:parse-error",
            format!(
                "parse error in {}:{}:{}: {} [{}]",
                path.display(),
                d.line,
                d.column,
                d.message,
                d.code
            ),
        ));
    };
    let mut functions = HashMap::new();
    let mut main = None;
    let mut stmts = Vec::new();
    for item in &file.items {
        match item {
            ast::Item::Function(f) => {
                if main.is_none() && file.kind == ast::FileKind::Function {
                    main = Some(Rc::new(f.clone()));
                }
                collect_functions(f, &mut functions);
            }
            ast::Item::Stmt(s) => stmts.push(s.clone()),
            ast::Item::Class(_) => {}
        }
    }
    let is_script = file.kind == ast::FileKind::Script;
    Ok(Rc::new(FileCtx {
        path,
        name,
        line_starts: line_starts(src),
        src: Rc::from(src),
        file,
        functions,
        main,
        stmts: Rc::new(stmts),
        is_script,
        matlab,
        file_backed,
        anon_bodies: RefCell::new(HashMap::new()),
    }))
}

/// Workspace of a function that has nested functions.
///
/// Every shared variable has exactly one live copy: while a frame that
/// binds it runs (or is suspended), the value sits in that frame's `vars`;
/// it moves into the callee when a nested function of this workspace is
/// called and back when the callee returns. `vars` holds the values that
/// are not in any frame (between calls, and after the owner returned while
/// a handle to a nested function keeps the workspace alive).
pub struct NestedEnv {
    pub vars: RefCell<HashMap<String, Value>>,
    /// Variable names of the owner function (shared with nested ones).
    pub shared: HashSet<String>,
    /// Names of the owner's direct nested functions.
    pub nested: HashSet<String>,
    /// Workspace of the function enclosing the owner (owner is nested).
    pub parent: Option<Rc<NestedEnv>>,
}

/// Context of a handle to a nested function (keeps the workspace alive).
pub struct NestedHandleCtx {
    pub file: Rc<FileCtx>,
    pub env: Rc<NestedEnv>,
}

/// One function activation.
pub struct Frame {
    pub vars: HashMap<String, Value>,
    pub globals: HashSet<String>,
    /// Names bound to persistent storage `persist[persist_key]`.
    pub persistents: HashSet<String>,
    /// Set when the first `persistent` declaration runs.
    pub persist_key: String,
    pub ctx: Rc<FileCtx>,
    pub fname: String,
    pub nargin: usize,
    pub nargout: usize,
    /// Byte offset of the statement being executed (for error lines).
    pub pos: u32,
    /// Class whose method/constructor this frame runs (access rules).
    pub class_ctx: Option<Rc<classes::ClassInfo>>,
    /// `import pkg.*` / `import pkg.fn` entries of this frame.
    pub imports: Vec<String>,
    /// Set when this function has nested functions.
    pub env: Option<Rc<NestedEnv>>,
    /// Set when this frame runs a nested function.
    pub link: Option<Rc<NestedEnv>>,
    /// Names of this frame that live in a nested workspace, by workspace.
    pub binds: Vec<(Rc<NestedEnv>, HashSet<String>)>,
    /// Caller's variable name of each argument (`inputname`); `None` for
    /// expressions or when the call site is not known.
    pub arg_names: Vec<Option<String>>,
}

impl Frame {
    pub fn new(ctx: Rc<FileCtx>, fname: &str) -> Frame {
        Frame {
            vars: HashMap::new(),
            globals: HashSet::new(),
            persistents: HashSet::new(),
            persist_key: String::new(),
            ctx,
            fname: fname.to_string(),
            nargin: 0,
            nargout: 0,
            pos: 0,
            class_ctx: None,
            imports: Vec::new(),
            env: None,
            link: None,
            binds: Vec::new(),
            arg_names: Vec::new(),
        }
    }
}

pub type BuiltinFn = fn(&mut Interp, Vec<Value>, usize) -> R<Vec<Value>>;

pub use assign::assign_path;
pub use eval::EOp;

/// `h.Name` on a numeric value that an embedder (the graphics session)
/// may know as a handle. `None` = not a handle: the interpreter raises its
/// own dot-indexing error. Consulted only where that error would occur.
pub type HandleFieldGet = fn(&mut Interp, &Value, &str) -> Option<R<Vec<Value>>>;

/// `h.Name... = rhs` on a numeric variable `h` (the ops after the variable,
/// at least one of them a field). `None` = not a handle, as above.
pub type HandleFieldSet = fn(&mut Interp, &Value, &[EOp], &Value) -> Option<R<()>>;

/// What a name resolves to when called.
#[derive(Clone)]
pub enum Callee {
    User(Rc<FileCtx>, Rc<Function>),
    Script(Rc<FileCtx>),
    Builtin(&'static str, BuiltinFn),
    Class(Rc<classes::ClassInfo>),
}

/// Interactive front end of an embedding session (the IndyMAT Rust kernel).
/// The `indy` CLI has none: output then goes to stdout/stderr and nothing
/// interrupts.
pub trait Session {
    /// Text for the Command Window (`stderr` for warnings and fid 2).
    fn output(&mut self, stderr: bool, text: &str);
    /// True once the running code must stop.
    fn interrupted(&mut self) -> bool;
    /// One line of user input; `None` when interrupted.
    fn input(&mut self, prompt: &str) -> Option<String>;
    /// Clear the Command Window.
    fn clc(&mut self);
    /// Deliver buffered output now (before a debugger pause).
    fn flush(&mut self) {}
}

/// Identifier of the error that ends code interrupted by the session.
pub const INTERRUPTED_ID: &str = "Indy:interrupted";

/// Statements and loop iterations between two `Session::interrupted` polls.
const INTERRUPT_POLL: u32 = 256;

#[derive(Default)]
pub struct WarnState {
    pub disabled: HashSet<String>,
    pub all_off: bool,
    pub last_msg: String,
    pub last_id: String,
}

/// Interpreter state.
pub struct Interp {
    pub path: Vec<PathBuf>,
    pub cwd: PathBuf,
    files: HashMap<PathBuf, Result<Rc<FileCtx>, Box<Error>>>,
    lookup: HashMap<String, Option<PathBuf>>,
    pub builtins: HashMap<&'static str, BuiltinFn>,
    pub globals: HashMap<String, Value>,
    pub persist: HashMap<String, HashMap<String, Value>>,
    pub frames: Vec<Frame>,
    /// `end` context: dims of the indexed value, position, count.
    pub(crate) end_stack: Vec<(Vec<usize>, usize, usize, Option<Value>)>,
    pub warn: WarnState,
    pub lasterr: String,
    pub rng: builtins::Rng,
    pub out: Box<dyn Write>,
    /// Capture buffer for `evalc`.
    pub capture: Option<String>,
    pub depth: usize,
    pub tic: Option<std::time::Instant>,
    pub format_long: bool,
    /// Open files for fopen/fprintf: id -> (path, content buffer, mode).
    pub files_open: HashMap<i32, builtins::OpenFile>,
    pub classes: HashMap<String, Rc<classes::ClassInfo>>,
    /// Methods of the `string` class (win dispatch for string arguments).
    pub string_methods: HashMap<&'static str, BuiltinFn>,
    pub(crate) pending_link: Option<Rc<NestedEnv>>,
    /// Argument names of the call being made (see [`Frame::arg_names`]).
    pub(crate) pending_arg_names: Option<Vec<Option<String>>>,
    /// Embedding session; `None` for the CLI.
    pub session: Option<Box<dyn Session>>,
    /// Set once the session reported an interrupt; sticky until cleared.
    pub interrupt: bool,
    poll_count: u32,
    /// State owned by builtins registered from outside this crate (the
    /// graphics session), one value per type; see [`Interp::ext_mut`]. The
    /// interpreter itself never reads it.
    extensions: HashMap<std::any::TypeId, Box<dyn std::any::Any>>,
    /// Dot access on handle doubles; see [`HandleFieldGet`].
    pub handle_field_get: Option<HandleFieldGet>,
    pub handle_field_set: Option<HandleFieldSet>,
    /// Breakpoints, stepping and pauses (see [`debug`]).
    pub debug: debug::DebugState,
}

pub const MAX_DEPTH: usize = 256;

impl Interp {
    pub fn new() -> Interp {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut it = Interp {
            path: Vec::new(),
            cwd,
            files: HashMap::new(),
            lookup: HashMap::new(),
            builtins: HashMap::new(),
            globals: HashMap::new(),
            persist: HashMap::new(),
            frames: Vec::new(),
            end_stack: Vec::new(),
            warn: WarnState::default(),
            lasterr: String::new(),
            rng: builtins::Rng::new(42),
            out: Box::new(std::io::BufWriter::new(std::io::stdout())),
            capture: None,
            depth: 0,
            tic: None,
            format_long: false,
            files_open: HashMap::new(),
            classes: HashMap::new(),
            string_methods: HashMap::new(),
            pending_link: None,
            pending_arg_names: None,
            session: None,
            interrupt: false,
            poll_count: 0,
            extensions: HashMap::new(),
            handle_field_get: None,
            handle_field_set: None,
            debug: debug::DebugState::default(),
        };
        builtins::register(&mut it);
        debug::register(&mut it);
        let base = load_source(PathBuf::from("<base>"), "base".into(), "").unwrap();
        it.frames.push(Frame::new(base, "base"));
        it
    }

    /// The extension state of type `T`, created with `T::default()` on
    /// first use. It lives as long as this interpreter.
    pub fn ext_mut<T: Default + 'static>(&mut self) -> &mut T {
        self.extensions
            .entry(std::any::TypeId::of::<T>())
            .or_insert_with(|| Box::new(T::default()))
            .downcast_mut::<T>()
            .expect("extension slot holds its own type")
    }

    /// The extension state of type `T`, if it was ever created.
    pub fn ext<T: 'static>(&self) -> Option<&T> {
        self.extensions
            .get(&std::any::TypeId::of::<T>())
            .and_then(|b| b.downcast_ref::<T>())
    }

    pub fn add_path(&mut self, dir: PathBuf, front: bool) {
        self.path.retain(|p| p != &dir);
        if front {
            self.path.insert(0, dir);
        } else {
            self.path.push(dir);
        }
        self.lookup.clear();
    }
    pub fn remove_path(&mut self, dir: &Path) {
        self.path.retain(|p| p != dir);
        self.lookup.clear();
    }
    pub fn set_cwd(&mut self, dir: PathBuf) {
        self.cwd = dir;
        self.lookup.clear();
    }

    pub fn frame(&self) -> &Frame {
        self.frames.last().unwrap()
    }
    pub fn frame_mut(&mut self) -> &mut Frame {
        self.frames.last_mut().unwrap()
    }

    // ----- output -------------------------------------------------------

    pub fn print(&mut self, s: &str) {
        if let Some(c) = &mut self.capture {
            c.push_str(s);
            return;
        }
        if let Some(session) = &mut self.session {
            session.output(false, s);
            return;
        }
        let _ = self.out.write_all(s.as_bytes());
    }
    pub fn flush(&mut self) {
        let _ = self.out.flush();
    }
    pub fn eprint(&mut self, s: &str) {
        if let Some(session) = &mut self.session {
            session.output(true, s);
            return;
        }
        self.flush();
        let mut e = std::io::stderr();
        let _ = e.write_all(s.as_bytes());
        let _ = e.flush();
    }

    // ----- session ------------------------------------------------------

    /// The error that ends interrupted code.
    pub fn interrupted_error() -> Box<Error> {
        Error::new(INTERRUPTED_ID, "Operation interrupted.")
    }

    /// Poll the session (every few calls); fail once it reports an
    /// interrupt. Without a session this never fails.
    pub fn check_interrupt(&mut self) -> R<()> {
        if self.interrupt {
            return Err(Interp::interrupted_error());
        }
        if self.session.is_none() {
            return Ok(());
        }
        self.poll_count += 1;
        if self.poll_count < INTERRUPT_POLL {
            return Ok(());
        }
        self.poll_count = 0;
        self.poll_interrupt()
    }

    /// Poll the session now (long builtins such as `pause`).
    pub fn poll_interrupt(&mut self) -> R<()> {
        if !self.interrupt
            && let Some(session) = &mut self.session
        {
            self.interrupt = session.interrupted();
        }
        if self.interrupt {
            return Err(Interp::interrupted_error());
        }
        Ok(())
    }

    /// Forget parsed files and name lookups, so edited or new files are
    /// seen (an interactive session calls this before each command).
    pub fn clear_file_cache(&mut self) {
        self.files.clear();
        self.lookup.clear();
    }

    /// Drop every frame but the base one after an aborted run.
    pub fn unwind_to_base(&mut self) {
        self.frames.truncate(1);
        self.depth = 0;
        self.end_stack.clear();
        self.pending_link = None;
        self.pending_arg_names = None;
    }

    // ----- variables ----------------------------------------------------

    pub fn get_var(&self, name: &str) -> Option<&Value> {
        let f = self.frame();
        if !f.globals.is_empty() && f.globals.contains(name) {
            return self.globals.get(name);
        }
        if !f.persistents.is_empty() && f.persistents.contains(name) {
            return self.persist.get(&f.persist_key).and_then(|m| m.get(name));
        }
        f.vars.get(name)
    }
    /// Mutable slot of an existing variable.
    pub fn var_slot(&mut self, name: &str) -> Option<&mut Value> {
        let f = self.frames.last_mut().unwrap();
        if !f.globals.is_empty() && f.globals.contains(name) {
            return self.globals.get_mut(name);
        }
        if !f.persistents.is_empty() && f.persistents.contains(name) {
            return self
                .persist
                .get_mut(&f.persist_key)
                .and_then(|m| m.get_mut(name));
        }
        f.vars.get_mut(name)
    }
    pub fn has_var(&self, name: &str) -> bool {
        self.get_var(name).is_some()
    }
    pub fn set_var(&mut self, name: &str, v: Value) {
        let f = self.frames.last_mut().unwrap();
        if !f.globals.is_empty() && f.globals.contains(name) {
            self.globals.insert(name.to_string(), v);
            return;
        }
        if !f.persistents.is_empty() && f.persistents.contains(name) {
            self.persist
                .entry(f.persist_key.clone())
                .or_default()
                .insert(name.to_string(), v);
            return;
        }
        if let Some(slot) = f.vars.get_mut(name) {
            *slot = v;
        } else {
            f.vars.insert(name.to_string(), v);
        }
    }
    pub fn take_var(&mut self, name: &str) -> Option<Value> {
        let f = self.frames.last_mut().unwrap();
        if !f.globals.is_empty() && f.globals.contains(name) {
            return self.globals.remove(name);
        }
        if !f.persistents.is_empty() && f.persistents.contains(name) {
            return self
                .persist
                .get_mut(&f.persist_key)
                .and_then(|m| m.remove(name));
        }
        f.vars.remove(name)
    }
    pub fn clear_var(&mut self, name: &str) -> Option<Value> {
        let f = self.frames.last_mut().unwrap();
        f.globals.remove(name);
        f.vars.remove(name)
    }

    // ----- errors -------------------------------------------------------

    pub fn loc(&self, frame: &Frame) -> StackFrame {
        let (line, column) = frame.ctx.line_col(frame.pos);
        StackFrame {
            file: frame.ctx.path.display().to_string(),
            name: frame.fname.clone(),
            line,
            column,
        }
    }

    /// Full stack of an error as seen from the current frame (innermost first).
    pub fn full_stack(&self, e: &Error) -> Vec<StackFrame> {
        let mut st = e.stack.clone();
        for f in self.frames.iter().rev() {
            if f.fname == "base" && f.ctx.name == "base" {
                continue;
            }
            st.push(self.loc(f));
        }
        st
    }

    // ----- warnings -----------------------------------------------------

    pub fn warning(&mut self, id: &str, msg: &str) {
        self.warn.last_msg = msg.to_string();
        self.warn.last_id = id.to_string();
        if self.warn.all_off && !id.is_empty() && !self.warn.disabled.contains(&format!("+{}", id))
        {
            return;
        }
        if self.warn.all_off && id.is_empty() {
            return;
        }
        if !id.is_empty() && self.warn.disabled.contains(id) {
            return;
        }
        if let Some(c) = &mut self.capture {
            c.push_str(&format!("warning: {}\n", msg));
            return;
        }
        self.eprint(&format!("warning: {}\n", msg));
    }

    // ----- files and lookup ---------------------------------------------

    pub fn load_file(&mut self, path: &Path) -> R<Rc<FileCtx>> {
        if let Some(r) = self.files.get(path) {
            return r.clone();
        }
        let r = read_source_file(path);
        self.files.insert(path.to_path_buf(), r.clone());
        r
    }

    fn search_dirs(&self) -> Vec<PathBuf> {
        let mut v = vec![self.cwd.clone()];
        v.extend(self.path.iter().cloned());
        v
    }

    /// Find `name.m` on cwd + path (packages via `+pkg` folders).
    pub fn find_file(&mut self, name: &str) -> Option<PathBuf> {
        if let Some(r) = self.lookup.get(name) {
            return r.clone();
        }
        let rel: PathBuf = if name.contains('.') {
            let parts: Vec<&str> = name.split('.').collect();
            let mut p = PathBuf::new();
            for d in &parts[..parts.len() - 1] {
                p.push(format!("+{}", d));
            }
            p.push(format!("{}.m", parts[parts.len() - 1]));
            p
        } else {
            PathBuf::from(format!("{}.m", name))
        };
        let mut found = None;
        if !name.is_empty() && !name.contains(['/', '\\']) {
            for d in self.search_dirs() {
                let p = d.join(&rel);
                if p.is_file() {
                    found = Some(p);
                    break;
                }
            }
        }
        self.lookup.insert(name.to_string(), found.clone());
        found
    }

    /// Resolve a function name from the current context.
    pub fn resolve(&mut self, name: &str) -> R<Option<Callee>> {
        let ctx = self.frame().ctx.clone();
        self.resolve_in(name, &ctx)
    }

    pub fn resolve_in(&mut self, name: &str, ctx: &Rc<FileCtx>) -> R<Option<Callee>> {
        if let Some(f) = ctx.functions.get(name) {
            return Ok(Some(Callee::User(ctx.clone(), f.clone())));
        }
        // private/ next to the calling file (or the file itself in private/).
        if let Some(dir) = ctx.dir() {
            let key = format!("{}\0{}", dir.display(), name);
            let found = match self.lookup.get(&key) {
                Some(r) => r.clone(),
                None => {
                    let mut cands = vec![dir.join("private").join(format!("{}.m", name))];
                    if dir.file_name().is_some_and(|n| n == "private") {
                        cands.push(dir.join(format!("{}.m", name)));
                    }
                    let f = cands.into_iter().find(|p| p.is_file());
                    self.lookup.insert(key, f.clone());
                    f
                }
            };
            if let Some(p) = found {
                return self.callee_for_file(&p).map(Some);
            }
        }
        if let Some(p) = self.find_file(name) {
            return self.callee_for_file(&p).map(Some);
        }
        if let Some((k, f)) = self.builtins.get_key_value(name) {
            return Ok(Some(Callee::Builtin(k, *f)));
        }
        let imports = self.frame().imports.clone();
        for imp in imports.iter().rev() {
            let full = if let Some(pkg) = imp.strip_suffix(".*") {
                format!("{}.{}", pkg, name)
            } else if imp.rsplit('.').next() == Some(name) {
                imp.clone()
            } else {
                continue;
            };
            if let Some(p) = self.find_file(&full) {
                return self.callee_for_file(&p).map(Some);
            }
        }
        Ok(None)
    }

    fn callee_for_file(&mut self, p: &Path) -> R<Callee> {
        let ctx = self.load_file(p)?;
        if ctx.is_script {
            return Ok(Callee::Script(ctx));
        }
        if ctx.file.kind == ast::FileKind::Class {
            return Ok(Callee::Class(self.class_from_ctx(&ctx)?));
        }
        match ctx.main.clone() {
            Some(f) => Ok(Callee::User(ctx, f)),
            None => Err(Error::new(
                "MATLAB:unsupported",
                format!("{}: file defines no function", p.display()),
            )),
        }
    }

    pub fn function_exists(&mut self, name: &str) -> bool {
        matches!(self.resolve(name), Ok(Some(_)))
    }

    // ----- calls --------------------------------------------------------

    pub fn undefined(name: &str) -> Box<Error> {
        Error::new(
            "MATLAB:UndefinedFunction",
            format!("Undefined function or variable '{}'.", name),
        )
    }

    /// Call a function by name (object methods take precedence).
    pub fn call_named(&mut self, name: &str, args: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
        if args.iter().any(classes::is_class_obj)
            && let Some((cls, m)) = self.dispatch_method(name, &args)
        {
            return self.call_method_info(&cls, &m, args, nargout);
        }
        if let Some(f) = self.mexception_method(name, &args) {
            return f(self, args, nargout);
        }
        match self.resolve(name)? {
            Some(c) => self.call_callee(&c, args, nargout),
            None => Err(Interp::undefined(name)),
        }
    }

    pub fn call_callee(&mut self, c: &Callee, args: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
        match c {
            Callee::Builtin(name, f) => {
                let mut r = if args.iter().any(Value::is_sparse) {
                    builtins::sparse::call(self, name, *f, args, nargout)?
                } else {
                    f(self, args, nargout)?
                };
                r.truncate(nargout.max(1));
                Ok(r)
            }
            Callee::User(ctx, f) => self.call_user(ctx.clone(), f.clone(), args, nargout),
            Callee::Class(c) => self.construct(c, args, nargout),
            Callee::Script(ctx) => {
                if !args.is_empty() {
                    return Err(Error::new(
                        "MATLAB:scriptNoArgs",
                        format!("Attempt to execute SCRIPT {} as a function.", ctx.name),
                    ));
                }
                self.run_script_ctx(ctx.clone())?;
                Ok(vec![])
            }
        }
    }

    /// Run a script's statements in the current frame.
    pub fn run_script_ctx(&mut self, ctx: Rc<FileCtx>) -> R<()> {
        let saved_ctx = std::mem::replace(&mut self.frame_mut().ctx, ctx.clone());
        let saved_pos = self.frame().pos;
        let saved_name = std::mem::replace(&mut self.frame_mut().fname, ctx.name.clone());
        self.debug_enter_script(saved_ctx.clone(), saved_pos, saved_name.clone());
        let stmts = ctx.stmts.clone();
        let mut r = self.exec_block(&stmts);
        self.debug_leave_script();
        if let Err(e) = &mut r {
            e.stack.push(self.loc(self.frame()));
        }
        let f = self.frame_mut();
        f.ctx = saved_ctx;
        f.pos = saved_pos;
        f.fname = saved_name;
        r.map(|_| ())
    }

    pub fn call_user(
        &mut self,
        ctx: Rc<FileCtx>,
        f: Rc<Function>,
        args: Vec<Value>,
        nargout: usize,
    ) -> R<Vec<Value>> {
        self.call_user_ex(ctx, f, args, nargout, vec![], None)
    }

    /// Call a user function with pre-set variables (constructor output)
    /// and an optional class context (methods).
    pub fn call_user_ex(
        &mut self,
        ctx: Rc<FileCtx>,
        f: Rc<Function>,
        args: Vec<Value>,
        nargout: usize,
        preset: Vec<(String, Value)>,
        class_ctx: Option<Rc<classes::ClassInfo>>,
    ) -> R<Vec<Value>> {
        if self.depth >= MAX_DEPTH {
            return Err(Error::new(
                "MATLAB:recursionLimit",
                format!("Maximum recursion limit of {} reached.", MAX_DEPTH),
            ));
        }
        let arg_names = self.pending_arg_names.take().unwrap_or_default();
        let fname = f.name.dotted();
        let has_varargin =
            matches!(f.params.last(), Some(ast::Param::Name(n)) if n.name == "varargin");
        let fixed = if has_varargin {
            f.params.len() - 1
        } else {
            f.params.len()
        };
        let validated = !f.arguments.is_empty();
        if args.len() > fixed && !has_varargin && !validated {
            return Err(Error::new(
                "MATLAB:TooManyInputs",
                format!("Too many input arguments (function '{}').", fname),
            ));
        }
        let has_varargout = f.outputs.last().is_some_and(|o| o.name == "varargout");
        let fixed_out = if has_varargout {
            f.outputs.len() - 1
        } else {
            f.outputs.len()
        };
        if nargout > fixed_out && !has_varargout {
            return Err(Error::new(
                "MATLAB:TooManyOutputs",
                format!("Too many output arguments (function '{}').", fname),
            ));
        }
        let mut frame = Frame::new(ctx.clone(), &fname);
        frame.arg_names = arg_names;
        frame.nargin = args.len();
        frame.nargout = nargout;
        frame.class_ctx = class_ctx;
        let own: HashSet<String> = f
            .params
            .iter()
            .filter_map(|p| match p {
                ast::Param::Name(n) => Some(n.name.clone()),
                ast::Param::WithDefault { name, .. } => Some(name.name.clone()),
                ast::Param::Ignore(_) => None,
            })
            .chain(f.outputs.iter().map(|o| o.name.clone()))
            .collect();
        let link = self
            .pending_link
            .take()
            .or_else(|| self.nested_env_for(&fname));
        // Names of enclosing workspaces this function sees (innermost
        // first); its own parameters and outputs are local.
        let mut outer: HashSet<String> = HashSet::new();
        if let Some(env) = &link {
            let mut e = Some(env.clone());
            while let Some(env) = e {
                let names: HashSet<String> = env
                    .shared
                    .iter()
                    .filter(|n| !own.contains(*n) && !outer.contains(*n))
                    .cloned()
                    .collect();
                outer.extend(names.iter().cloned());
                self.gather_shared(&env, &names, &mut frame.vars);
                frame.binds.push((env.clone(), names));
                e = env.parent.clone();
            }
            frame.link = Some(env.clone());
        }
        if !f.nested.is_empty() {
            let mut ids = Vec::new();
            eval::collect_stmt_idents(&f.body, &mut ids);
            let shared: HashSet<String> = ids
                .into_iter()
                .chain(own.iter().cloned())
                .filter(|n| !outer.contains(n))
                .collect();
            let nested = f.nested.iter().map(|n| n.name.dotted()).collect();
            let env = Rc::new(NestedEnv {
                vars: RefCell::new(HashMap::new()),
                shared: shared.clone(),
                nested,
                parent: link.clone(),
            });
            frame.binds.push((env.clone(), shared));
            frame.env = Some(env);
        }
        for (k, v) in preset {
            frame.vars.insert(k, v);
        }
        let nargs = args.len();
        let mut defaults = Vec::new();
        let mut pending_args = None;
        if validated {
            pending_args = Some(args);
        } else {
            let mut it = args.into_iter();
            for (i, p) in f.params.iter().enumerate() {
                if has_varargin && i == fixed {
                    let rest: Vec<Value> = it.by_ref().collect();
                    frame.vars.insert("varargin".into(), Value::cell_row(rest));
                    break;
                }
                let v = it.next();
                match p {
                    ast::Param::Name(n) => {
                        if let Some(v) = v {
                            frame.vars.insert(n.name.clone(), v);
                        }
                    }
                    ast::Param::WithDefault { name, default } => match v {
                        Some(v) => {
                            frame.vars.insert(name.name.clone(), v);
                        }
                        None => defaults.push((name.name.clone(), default)),
                    },
                    ast::Param::Ignore(_) => {}
                }
            }
        }
        let _ = nargs;
        self.frames.push(frame);
        self.depth += 1;
        let mut result = (|| -> R<()> {
            for (name, d) in defaults {
                let v = self.eval(d)?;
                self.set_var(&name, v);
            }
            if let Some(a) = pending_args {
                builtins::args::bind(self, &f, a)?;
            }
            self.exec_block(&f.body)?;
            if validated {
                builtins::args::check_outputs(self, &f)?;
            }
            Ok(())
        })();
        // Outputs of a workspace owner stay in the workspace (a handle to
        // a nested function may still read them).
        let keep_outs = self.frame().env.is_some();
        // Collect outputs before popping.
        let mut outs = Vec::new();
        if result.is_ok() {
            let want = nargout.max(1);
            'collect: for (i, o) in f.outputs.iter().enumerate() {
                if i >= want {
                    break;
                }
                if has_varargout && i == fixed_out {
                    match self.get_var("varargout") {
                        Some(Value::Cell(c)) => {
                            for v in c.data.iter() {
                                if outs.len() >= want {
                                    break 'collect;
                                }
                                outs.push(v.clone());
                            }
                        }
                        Some(_) => {
                            result = Err(Error::new(
                                "MATLAB:varargoutNotCell",
                                "varargout must be a cell array.",
                            ));
                        }
                        None => {}
                    }
                    if outs.len() < nargout {
                        result = Err(Error::new(
                            "MATLAB:unassignedOutputs",
                            format!(
                                "Output argument \"varargout{{{}}}\" not assigned a value in the execution with \"{}\" function.",
                                outs.len() - fixed_out + 1,
                                fname
                            ),
                        ));
                    }
                    break;
                }
                let v = if keep_outs {
                    self.get_var(&o.name).cloned()
                } else {
                    self.take_var(&o.name)
                };
                match v {
                    Some(v) => outs.push(v),
                    None => {
                        if i < nargout {
                            result = Err(Error::new(
                                "MATLAB:unassignedOutputs",
                                format!(
                                    "Output argument \"{}\" (and possibly others) not assigned a value in the execution with \"{}\" function.",
                                    o.name, fname
                                ),
                            ));
                        }
                        break;
                    }
                }
            }
        }
        if let Err(e) = &mut result {
            let loc = self.loc(self.frame());
            if e.stack.last().is_some_and(|s| s.name == "\0as-caller") {
                // throwAsCaller: this frame is hidden from the stack.
                e.stack.pop();
            } else if e.stack.last() != Some(&loc) {
                e.stack.push(loc);
            }
        }
        self.depth -= 1;
        let mut frame = self.frames.pop().unwrap();
        Interp::release_shared(&mut frame);
        self.run_destructors(frame.vars.into_values().collect());
        self.reload_shared();
        result.map(|_| outs)
    }

    /// Call a function value (handle, or a name given as char).
    pub fn call_value(&mut self, f: &Value, args: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
        match f {
            Value::Func(h) => self.call_handle(h, args, nargout),
            Value::Char(_) => {
                let name = f.to_str().unwrap();
                self.call_named(&name, args, nargout)
            }
            other => Err(Error::new(
                "MATLAB:feval:invalidFunction",
                format!(
                    "Expected a function handle or function name, got a value of class {}.",
                    other.class_name()
                ),
            )),
        }
    }

    pub fn call_handle(
        &mut self,
        h: &Rc<indy_value::Handle>,
        args: Vec<Value>,
        nargout: usize,
    ) -> R<Vec<Value>> {
        // Argument names reach only user functions called by name.
        let arg_names = self.pending_arg_names.take();
        if let Some(n) = h
            .ctx
            .clone()
            .and_then(|c| c.downcast::<NestedHandleCtx>().ok())
        {
            if let Some(Callee::User(c, f)) = self.resolve_in(&h.name, &n.file)? {
                self.pending_link = Some(n.env.clone());
                self.pending_arg_names = arg_names;
                return self.call_user(c, f, args, nargout);
            }
            return Err(Interp::undefined(&h.name));
        }
        let ctx = h
            .ctx
            .clone()
            .and_then(|c| c.downcast::<FileCtx>().ok())
            .unwrap_or_else(|| self.frame().ctx.clone());
        match &h.anon {
            None => {
                if args.iter().any(classes::is_class_obj)
                    && let Some((cls, m)) = self.dispatch_method(&h.name, &args)
                {
                    return self.call_method_info(&cls, &m, args, nargout);
                }
                if let Some(f) = self.mexception_method(&h.name, &args) {
                    return f(self, args, nargout);
                }
                let c = self.resolve_in(&h.name, &ctx)?;
                if matches!(c, Some(Callee::User(..))) {
                    self.pending_arg_names = arg_names;
                }
                match c {
                    Some(c) => self.call_callee(&c, args, nargout),
                    None => Err(Interp::undefined(&h.name)),
                }
            }
            Some(a) => {
                if self.depth >= MAX_DEPTH {
                    return Err(Error::new(
                        "MATLAB:recursionLimit",
                        format!("Maximum recursion limit of {} reached.", MAX_DEPTH),
                    ));
                }
                let mut frame = Frame::new(ctx, "@<anonymous>");
                for (n, v) in &a.captured {
                    frame.vars.insert(n.clone(), v.clone());
                }
                let has_varargin = a.params.last().is_some_and(|p| p == "varargin");
                let fixed = if has_varargin {
                    a.params.len() - 1
                } else {
                    a.params.len()
                };
                if args.len() > fixed && !has_varargin {
                    return Err(Error::new(
                        "MATLAB:TooManyInputs",
                        "Too many input arguments.",
                    ));
                }
                frame.nargin = args.len();
                frame.nargout = nargout;
                let mut it = args.into_iter();
                for (i, p) in a.params.iter().enumerate() {
                    if has_varargin && i == fixed {
                        frame
                            .vars
                            .insert(p.clone(), Value::cell_row(it.by_ref().collect()));
                        break;
                    }
                    match it.next() {
                        Some(v) => {
                            if p != "~" {
                                frame.vars.insert(p.clone(), v);
                            }
                        }
                        None => break,
                    }
                }
                self.frames.push(frame);
                self.depth += 1;
                let body = a.body.clone();
                let r = self.eval_multi(&body, nargout);
                self.depth -= 1;
                self.frames.pop();
                self.reload_shared();
                r
            }
        }
    }

    // ----- entry points -------------------------------------------------

    /// Run a script file in the base workspace.
    pub fn run_file(&mut self, path: &Path) -> R<()> {
        let ctx = self.load_file(path)?;
        if !ctx.is_script {
            let name = ctx.name.clone();
            let c = self.callee_for_file(path)?;
            let _ = name;
            self.call_callee(&c, vec![], 0)?;
            return Ok(());
        }
        let stmts = ctx.stmts.clone();
        let base = self.frames.first_mut().unwrap();
        base.ctx = ctx.clone();
        base.fname = ctx.name.clone();
        let r = self.exec_block(&stmts);
        r.map(|_| ())
    }

    /// Run source text in the current workspace (used by `eval`).
    pub fn run_text(&mut self, text: &str) -> R<Vec<Value>> {
        let ctx = load_source(
            self.frame().ctx.path.clone(),
            self.frame().ctx.name.clone(),
            text,
        )?;
        let saved = std::mem::replace(&mut self.frame_mut().ctx, ctx.clone());
        let saved_pos = self.frame().pos;
        let fname = self.frame().fname.clone();
        self.debug_enter_script(saved.clone(), saved_pos, fname);
        let r = self.exec_block(&ctx.stmts);
        self.debug_leave_script();
        let f = self.frame_mut();
        f.ctx = saved;
        f.pos = saved_pos;
        r.map(|_| vec![])
    }
}

impl Interp {
    /// The workspace whose nested function `name` is, seen from the
    /// running frame (its own nested functions, then those of the
    /// enclosing workspaces: siblings, the parent's siblings, …).
    pub(crate) fn nested_env_for(&self, name: &str) -> Option<Rc<NestedEnv>> {
        let cur = self.frames.last()?;
        if let Some(env) = &cur.env
            && env.nested.contains(name)
        {
            return Some(env.clone());
        }
        let mut e = cur.link.clone();
        while let Some(env) = e {
            if env.nested.contains(name) {
                return Some(env);
            }
            e = env.parent.clone();
        }
        None
    }

    /// Move the live copies of `names` of workspace `env` into `vars`:
    /// from the workspace store, else from the innermost frame binding
    /// them (a caller still on the stack).
    fn gather_shared(
        &mut self,
        env: &Rc<NestedEnv>,
        names: &HashSet<String>,
        vars: &mut HashMap<String, Value>,
    ) {
        let mut store = env.vars.borrow_mut();
        for n in names {
            if let Some(v) = store.remove(n) {
                vars.insert(n.clone(), v);
                continue;
            }
            for f in self.frames.iter_mut().rev() {
                if f.binds
                    .iter()
                    .any(|(e, ns)| Rc::ptr_eq(e, env) && ns.contains(n))
                {
                    if let Some(v) = f.vars.remove(n) {
                        vars.insert(n.clone(), v);
                    }
                    break;
                }
            }
        }
    }

    /// A returning frame hands its shared variables back to their
    /// workspaces. Its own workspace is kept only when something (a
    /// handle to a nested function) still refers to it.
    fn release_shared(frame: &mut Frame) {
        for (env, names) in std::mem::take(&mut frame.binds) {
            let own = frame.env.as_ref().is_some_and(|e| Rc::ptr_eq(e, &env));
            // References: `frame.env` and this bind entry.
            if own && Rc::strong_count(&env) <= 2 {
                continue;
            }
            let mut store = env.vars.borrow_mut();
            for n in names {
                match frame.vars.remove(&n) {
                    Some(v) => {
                        store.insert(n, v);
                    }
                    None => {
                        store.remove(&n);
                    }
                }
            }
        }
    }

    /// The frame resuming after a call takes back shared variables that
    /// the callee chain left in the workspace stores.
    fn reload_shared(&mut self) {
        let Some(f) = self.frames.last_mut() else {
            return;
        };
        for (env, names) in &f.binds {
            let mut store = env.vars.borrow_mut();
            if store.is_empty() {
                continue;
            }
            for n in names {
                if let Some(v) = store.remove(n) {
                    f.vars.insert(n.clone(), v);
                }
            }
        }
    }

    /// Built-in MException and string methods win over same-named path
    /// functions.
    pub fn mexception_method(&self, name: &str, args: &[Value]) -> Option<BuiltinFn> {
        if args.iter().any(builtins::control::is_lti)
            && let Some(f) = builtins::control::method(name)
        {
            return Some(f);
        }
        if let Some(f) = builtins::datatypes::method(name, args) {
            return Some(f);
        }
        if args.iter().any(|a| matches!(a, Value::Str(_)))
            && let Some(f) = self.string_methods.get(name)
        {
            return Some(*f);
        }
        if !matches!(
            name,
            "getReport" | "throw" | "rethrow" | "throwAsCaller" | "addCause"
        ) {
            return None;
        }
        match args.first() {
            Some(Value::Object(o)) if o.class == "MException" && o.meta.is_none() => {
                self.builtins.get(name).copied()
            }
            _ => None,
        }
    }

    /// Run `onCleanup` tasks (and handle-class `delete` methods) of values
    /// whose last reference is going away.
    pub fn run_destructors(&mut self, vals: Vec<Value>) {
        for v in vals {
            let Value::HObject(rc) = v else { continue };
            if Rc::strong_count(&rc) != 1 {
                continue;
            }
            let (class, task) = {
                let o = rc.borrow();
                (o.class.clone(), o.get("task").cloned())
            };
            if class == "onCleanup" {
                if let Some(t) = task {
                    let _ = self.call_value(&t, vec![], 0);
                }
                continue;
            }
            let obj = Value::HObject(rc);
            if let Some(cls) = classes::class_of(&obj)
                && let Some(m) = cls.method("delete")
            {
                let _ = self.call_method_info(&cls, &m, vec![obj], 0);
            }
        }
    }
}

impl Default for Interp {
    fn default() -> Interp {
        Interp::new()
    }
}

/// Make an MException object from an error.
pub fn exception_value(it: &Interp, e: &Error) -> Value {
    if let Some(Value::Object(o)) = &e.object {
        let mut o = o.clone();
        if o.get("stack").is_none_or(|s| s.is_empty()) {
            o.set("stack", stack_value(&it.full_stack(e)));
        }
        return Value::Object(o);
    }
    let mut o = indy_value::Object::new("MException");
    o.set("identifier", Value::str(&e.identifier));
    o.set("message", Value::str(&e.message));
    o.set("stack", stack_value(&it.full_stack(e)));
    o.set(
        "cause",
        Value::Cell(indy_value::Array::new(vec![0, 1], vec![])),
    );
    Value::Object(Box::new(o))
}

pub fn stack_value(st: &[StackFrame]) -> Value {
    let mut s = indy_value::StructArray::empty_with(
        vec!["file".into(), "name".into(), "line".into(), "column".into()],
        vec![st.len(), 1],
    );
    for (i, f) in st.iter().enumerate() {
        s.elems[i] = vec![
            Value::str(&f.file),
            Value::str(&f.name),
            Value::num(f.line as f64),
            Value::num(f.column as f64),
        ];
    }
    Value::Struct(s)
}

/// Error from an MException-like value (for `throw`, `rethrow`, `error(ME)`).
pub fn error_from_exception(v: &Value) -> Box<Error> {
    match v {
        Value::Object(o) => {
            let id = o
                .get("identifier")
                .and_then(|v| v.to_str())
                .unwrap_or_default();
            let msg = o
                .get("message")
                .and_then(|v| v.to_str())
                .unwrap_or_default();
            let mut e = Error::new(&id, msg);
            e.object = Some(v.clone());
            e
        }
        Value::Struct(s) if s.numel() == 1 => {
            let id = s
                .get(0, "identifier")
                .and_then(|v| v.to_str())
                .unwrap_or_default();
            let msg = s
                .get(0, "message")
                .and_then(|v| v.to_str())
                .unwrap_or_default();
            Error::new(&id, msg)
        }
        _ => Error::msg("Invalid exception value."),
    }
}

/// Run `f` and turn a panic into an error (an engine bug, never a crash).
pub fn catch_panic<T>(f: impl FnOnce() -> R<T>) -> R<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(p) => {
            let msg = p
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| p.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".into());
            Err(Error::new(
                "Indy:internalError",
                format!("internal engine error (please report): {}", msg),
            ))
        }
    }
}
