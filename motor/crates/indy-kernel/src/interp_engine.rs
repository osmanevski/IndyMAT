//! The real engine: the `indy-interp` tree-walking interpreter behind the
//! kernel protocol.
//!
//! - `file` mode code arrives already read by the server (`server::eval`).
//! - Output, input, `clc` and interrupt polls go through [`Host`] via a
//!   [`Session`] bridge installed for the duration of each eval; nothing is
//!   written to stdout.
//! - The base workspace persists across evals. Before each eval the parsed
//!   file cache is dropped so edited and new `.m` files are seen.
//! - Search path: the current folder (the start folder is the user's
//!   workspace folder), then the folders in `INDYMAT_R_PATH` (the app sets
//!   IndyMAT's own `octave/` helpers, e.g. the netlist layer `im_*`).
//!   IndyMAT's `octave/compat` is not added: its wrappers
//!   look up Octave's own library, which the Rust engine does not have. User code
//!   can still call `addpath`.
//! - Graphics: `indy-graphics-builtins` keeps the session's figures in the
//!   interpreter. After every eval (also a failed or interrupted one) the
//!   engine reports the complete list of open figures as `figure` events,
//!   in figure-number order (at most 12), each with its Figure-Data JSON;
//!   only changed figures are re-serialised and re-rendered (`png_base64`).

use crate::debugger::{self, DebugBridge};
use crate::engine::{
    Engine, EngineError, EngineInfo, ErrorCode, EvalError, EvalMode, EvalRequest, FigureEvent,
    Host, PageRequest, SourceLocation, Stream, VariableSummary,
};
use crate::json::Json;
use crate::session_builtins;
use crate::variables;
use indy_interp::{
    Callee, INTERRUPTED_ID, Interp, Session, catch_panic, load_file_source, load_source,
};
use indy_syntax::ast::FileKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Output is batched into events of at most this many bytes...
const OUTPUT_BATCH: usize = 8192;
/// ...or this old.
const OUTPUT_DELAY: Duration = Duration::from_millis(50);

pub struct InterpEngine {
    it: Interp,
    start: PathBuf,
}

fn fresh_interp(cwd: &Path) -> Interp {
    let mut it = Interp::new();
    // Never write to the protocol's stdout, even by accident.
    it.out = Box::new(std::io::sink());
    it.set_cwd(cwd.to_path_buf());
    // Extra folders from the app (IndyMAT's own `octave/` helpers such as the
    // netlist layer), after the current folder so user files still win.
    if let Some(dirs) = std::env::var_os("INDYMAT_R_PATH") {
        for dir in std::env::split_paths(&dirs).filter(|d| d.is_dir()) {
            it.add_path(dir, false);
        }
    }
    session_builtins::register(&mut it);
    it
}

impl InterpEngine {
    pub fn new(start: PathBuf) -> InterpEngine {
        InterpEngine {
            it: fresh_interp(&start),
            start,
        }
    }

    /// Run the request's code; on error also the source location.
    fn run(
        &mut self,
        request: &EvalRequest,
        source: &str,
    ) -> Result<(), (Box<indy_interp::Error>, Option<SourceLocation>)> {
        let (path, name) = match &request.path {
            Some(p) => (
                p.clone(),
                p.file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "base".into()),
            ),
            None => (PathBuf::from("<base>"), "base".to_string()),
        };
        // Only `file` text is the file's own text: its lines can pause.
        let load = if request.mode == EvalMode::File {
            load_file_source
        } else {
            load_source
        };
        let ctx = load(path, name.clone(), source).map_err(|e| (e, None))?;
        if request.mode == EvalMode::File && !ctx.is_script {
            if ctx.file.kind == FileKind::Class {
                let e = indy_interp::Error::new(
                    "IndyR:runClassdef",
                    format!("{name} is a classdef file; it cannot be run as a script."),
                );
                return Err((e, None));
            }
            let main = ctx.main.clone().ok_or_else(|| {
                let e = indy_interp::Error::new(
                    "MATLAB:unsupported",
                    format!("{name}: file defines no function"),
                );
                (e, None)
            })?;
            let callee = Callee::User(ctx, main);
            return catch_panic(|| self.it.call_callee(&callee, vec![], 0))
                .map(|_| ())
                .map_err(|e| {
                    let location = self.location(request, &e);
                    (e, location)
                });
        }
        let base = &mut self.it.frames[0];
        let saved_ctx = std::mem::replace(&mut base.ctx, ctx.clone());
        let saved_name = std::mem::replace(&mut base.fname, name);
        let saved_pos = base.pos;
        let stmts = ctx.stmts.clone();
        let result = catch_panic(|| self.it.exec_block(&stmts)).map(|_| ());
        let result = result.map_err(|e| {
            let location = self.location(request, &e);
            (e, location)
        });
        self.it.unwind_to_base();
        let base = &mut self.it.frames[0];
        base.ctx = saved_ctx;
        base.fname = saved_name;
        base.pos = saved_pos;
        result
    }

    /// Where an error happened: the innermost frame in a real file, else
    /// the failing statement of the run code. Only for `file`/`section`
    /// requests with a source path (the command window has no file).
    fn location(&self, request: &EvalRequest, e: &indy_interp::Error) -> Option<SourceLocation> {
        if request.mode == EvalMode::Code || request.path.is_none() {
            return None;
        }
        let here = self.it.loc(self.it.frame());
        e.stack
            .iter()
            .chain(std::iter::once(&here))
            .find(|f| Path::new(&f.file).is_absolute())
            .map(|f| SourceLocation {
                file: PathBuf::from(&f.file),
                line: f.line as u64,
            })
    }

    /// The complete list of open figures, one `figure` event each.
    fn report_figures(&mut self, host: &mut dyn Host) {
        let figures = catch_panic(|| Ok(indy_graphics_builtins::open_figures(&mut self.it)));
        for f in figures.unwrap_or_default() {
            let (figure, reason) = match Json::parse(&f.json) {
                Ok(data) => {
                    let interactive = matches!(data.get("supported"), Some(Json::Bool(true)));
                    let reason = match data.get("reason").and_then(Json::as_str) {
                        Some(r) if !interactive => r.to_string(),
                        _ => String::new(),
                    };
                    (Some(data), reason)
                }
                Err(_) => (
                    None,
                    "Could not generate interactive plot data.".to_string(),
                ),
            };
            host.figure(FigureEvent {
                number: f.number,
                name: f.name,
                figure,
                png_base64: f.png.as_deref().map(|png| base64(png)),
                reason,
            });
        }
    }

    fn variable(&self, name: &str) -> Option<(&indy_interp::Value, bool)> {
        let base = &self.it.frames[0];
        if base.globals.contains(name) {
            return self.it.globals.get(name).map(|v| (v, true));
        }
        base.vars.get(name).map(|v| (v, false))
    }
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len() / 3 * 4 + 4);
    for chunk in data.chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// [`Session`] over the eval's [`Host`], with batched output.
struct HostBridge {
    /// The `&mut dyn Host` of the running `eval`, lifetime erased; see
    /// `InterpEngine::eval` for why this is sound.
    host: *mut (dyn Host + 'static),
    pending: String,
    stream: Stream,
    since: Instant,
}

impl HostBridge {
    fn host(&mut self) -> &mut dyn Host {
        // SAFETY: `host` points to the `&mut dyn Host` passed to `eval`; the
        // bridge is created and dropped inside that call, and nothing else
        // uses the host while the interpreter runs.
        unsafe { &mut *self.host }
    }

    fn flush(&mut self) {
        if !self.pending.is_empty() {
            let text = std::mem::take(&mut self.pending);
            let stream = self.stream;
            self.host().output(stream, &text);
        }
        self.since = Instant::now();
    }
}

impl Session for HostBridge {
    fn flush(&mut self) {
        HostBridge::flush(self);
    }

    fn output(&mut self, stderr: bool, text: &str) {
        let stream = if stderr {
            Stream::Stderr
        } else {
            Stream::Stdout
        };
        if stream != self.stream {
            self.flush();
            self.stream = stream;
        }
        self.pending.push_str(text);
        if self.pending.len() >= OUTPUT_BATCH || self.since.elapsed() >= OUTPUT_DELAY {
            self.flush();
        }
    }

    fn interrupted(&mut self) -> bool {
        if self.since.elapsed() >= OUTPUT_DELAY {
            self.flush();
        }
        self.host().interrupted()
    }

    fn input(&mut self, prompt: &str) -> Option<String> {
        self.flush();
        self.host().input(prompt).ok()
    }

    fn clc(&mut self) {
        // Output before `clc` is discarded by the client anyway.
        self.pending.clear();
        self.host().clc();
    }
}

impl Drop for HostBridge {
    fn drop(&mut self) {
        self.flush();
    }
}

impl Engine for InterpEngine {
    fn info(&self) -> EngineInfo {
        EngineInfo {
            name: "indy".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            stub: false,
            description: "IndyMAT Rust engine (experimental)".into(),
        }
    }

    fn eval(&mut self, request: &EvalRequest, host: &mut dyn Host) -> Result<(), EvalError> {
        self.it.interrupt = false;
        self.it.clear_file_cache();
        let raw: *mut (dyn Host + '_) = &mut *host;
        // SAFETY: only the trait object's lifetime bound is erased. The
        // bridge holding the pointer is removed from the interpreter (and
        // dropped, flushing output) before this function returns, also when
        // the interpreter panics (`catch_panic` in `run`), so the pointer
        // never outlives the borrow it came from.
        let raw: *mut (dyn Host + 'static) = unsafe { std::mem::transmute(raw) };
        self.it.session = Some(Box::new(HostBridge {
            host: raw,
            pending: String::new(),
            stream: Stream::Stdout,
            since: Instant::now(),
        }));
        self.it.debug_begin();
        self.it
            .set_debug_controller(Some(Box::new(DebugBridge { host: raw })));
        let result = self.run(request, &request.code);
        let interrupted = self.it.interrupt;
        let quit = self.it.debug.quit;
        self.it.set_debug_controller(None);
        self.it.debug_end();
        self.it.session = None;
        self.it.interrupt = false;
        self.it.unwind_to_base();
        // Relative paths in the OS (and in child tools) follow `cd`.
        let _ = std::env::set_current_dir(&self.it.cwd);
        self.report_figures(host);
        match result {
            Ok(()) => Ok(()),
            Err((e, _)) if quit && e.identifier == INTERRUPTED_ID => Err(EvalError {
                message: "Debugging ended.".into(),
                interrupted: true,
                quit: true,
                location: None,
            }),
            Err((e, _)) if interrupted && e.identifier == INTERRUPTED_ID => {
                Err(EvalError::interrupted())
            }
            Err((e, location)) => Err(EvalError {
                message: e.message.clone(),
                interrupted: false,
                quit: false,
                location,
            }),
        }
    }

    fn workspace(&self) -> Vec<VariableSummary> {
        let base = &self.it.frames[0];
        let mut names: Vec<&String> = base.vars.keys().chain(base.globals.iter()).collect();
        names.sort();
        names.dedup();
        names
            .into_iter()
            .filter_map(|name| {
                let (value, global) = self.variable(name)?;
                Some(variables::summary(name, value, global))
            })
            .take(500)
            .collect()
    }

    fn inspect(&self, name: &str, page: PageRequest) -> Result<Json, EngineError> {
        variables::page(name, self.variable(name).map(|(v, _)| v), page)
    }

    fn cwd(&self) -> PathBuf {
        self.it.cwd.clone()
    }

    fn cd(&mut self, path: &Path) -> Result<PathBuf, EngineError> {
        let target = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.it.cwd.join(path)
        };
        let resolved = target
            .canonicalize()
            .ok()
            .filter(|p| p.is_dir())
            .ok_or_else(|| {
                EngineError::new(
                    ErrorCode::NotFound,
                    format!("cd: {}: No such folder", path.display()),
                )
            })?;
        self.it.set_cwd(resolved.clone());
        let _ = std::env::set_current_dir(&resolved);
        Ok(resolved)
    }

    fn debugger(&self) -> bool {
        true
    }

    fn breakpoint(&mut self, body: &Json) -> Result<Json, EngineError> {
        debugger::breakpoint_op(&mut self.it, body)
    }

    fn reset(&mut self) {
        // Breakpoints belong to the session, not to the workspace.
        let (breakpoints, stop_on_error) = self.it.take_breakpoints();
        self.it = fresh_interp(&self.start);
        self.it.restore_breakpoints(breakpoints, stop_on_error);
        let _ = std::env::set_current_dir(&self.start);
    }

    fn rename(&mut self, old: &str, new: &str) -> Result<(), EngineError> {
        if !valid_name(new) {
            return Err(EngineError::new(
                ErrorCode::Invalid,
                format!("Invalid variable name: {new}"),
            ));
        }
        if self.variable(old).is_none() {
            return Err(EngineError::new(
                ErrorCode::NotFound,
                format!("Variable not found: {old}"),
            ));
        }
        if old == new {
            return Ok(());
        }
        if self.variable(new).is_some() {
            return Err(EngineError::new(
                ErrorCode::Invalid,
                format!("A variable named {new} already exists."),
            ));
        }
        let base = &mut self.it.frames[0];
        if base.globals.contains(old) {
            return Err(EngineError::new(
                ErrorCode::Invalid,
                format!("{old} is a global variable; rename it in code."),
            ));
        }
        if let Some(value) = base.vars.remove(old) {
            base.vars.insert(new.to_string(), value);
        }
        Ok(())
    }

    fn clear(&mut self, names: Option<&[String]>) -> Result<Vec<String>, EngineError> {
        let base = &self.it.frames[0];
        let targets: Vec<String> = match names {
            None => {
                let mut all: Vec<String> = base
                    .vars
                    .keys()
                    .chain(base.globals.iter())
                    .cloned()
                    .collect();
                all.sort();
                all.dedup();
                all
            }
            Some(names) => {
                if let Some(bad) = names.iter().find(|n| !valid_name(n)) {
                    return Err(EngineError::new(
                        ErrorCode::Invalid,
                        format!("Invalid variable name: {bad}"),
                    ));
                }
                names
                    .iter()
                    .filter(|n| self.variable(n).is_some())
                    .cloned()
                    .collect()
            }
        };
        let mut removed = Vec::new();
        for name in &targets {
            // Unlinks a global from the base workspace; its value stays
            // global, as with `clear name`.
            if let Some(value) = self.it.frames[0].vars.remove(name) {
                removed.push(value);
            }
            self.it.frames[0].globals.remove(name);
        }
        self.it.run_destructors(removed);
        Ok(targets)
    }
}

fn valid_name(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && text.len() <= 63
}
