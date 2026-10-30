//! Debugger core: breakpoints, stepping and pauses at statement boundaries.
//!
//! The interpreter is a tree walker, so a pause is a call into the
//! embedder's [`DebugController`] from inside the statement hook: the Rust
//! call stack, and with it every MATLAB frame, stays alive until the
//! controller says how to resume. While paused the controller may evaluate
//! code in a selected frame ([`Interp::debug_eval`]), read that frame's
//! variables and change breakpoints.
//!
//! Cost when nothing is set: the statement hook is one `bool` test
//! (`DebugState::active`, true only with a controller installed and an
//! enabled breakpoint, a pending step or a run-to-cursor target), plus one
//! test on the error path for `dbstop if error`.
//!
//! Lines: a statement's line is the line of its first character. A
//! breakpoint at a line where no statement starts snaps to the next line
//! that starts one (MATLAB rule), computed against each loaded text of the
//! file. Several statements on one line pause once ("new line" = another
//! file or line than the frame's previous statement, or a statement at or
//! before it, i.e. a loop going round). Only code from real files pauses:
//! Command Window text, `section` text and `eval` strings never do, but
//! the functions and scripts they call do.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::OnceLock;

use indy_syntax::ast::{self, Stmt, StmtKind};

use crate::builtins::{arg_str, err, one};
use crate::{BuiltinFn, FileCtx, Frame, Interp, R, Value};
use indy_value::{Error, StructArray};

/// How paused code goes on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resume {
    Continue,
    /// Step over: the next line of this frame or a caller.
    Step,
    /// Step in: the next line anywhere.
    StepIn,
    /// Step out: the next line of a caller.
    StepOut,
    /// `dbquit`: end the running code.
    Quit,
    /// Interrupt (Stop): end the running code.
    Stop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PauseReason {
    Breakpoint,
    Step,
    Keyboard,
    Error,
    RunToCursor,
}

impl PauseReason {
    pub fn name(self) -> &'static str {
        match self {
            PauseReason::Breakpoint => "breakpoint",
            PauseReason::Step => "step",
            PauseReason::Keyboard => "keyboard",
            PauseReason::Error => "error",
            PauseReason::RunToCursor => "run_to_cursor",
        }
    }
}

/// The embedder's side of a pause (the kernel). `pause` returns when the
/// code must go on; meanwhile it may call the `debug_*` methods of `it`.
pub trait DebugController {
    fn pause(&mut self, it: &mut Interp, reason: PauseReason) -> Resume;
}

#[derive(Clone, Debug, PartialEq)]
pub struct Breakpoint {
    /// Canonical path of the `.m` file.
    pub path: PathBuf,
    /// Requested line (the line it stops at may be later, see module doc).
    pub line: usize,
    pub condition: String,
    pub enabled: bool,
}

/// One entry of the paused call stack (newest first).
#[derive(Clone, Debug, PartialEq)]
pub struct StackEntry {
    pub name: String,
    /// Absolute path of the file.
    pub file: String,
    pub line: usize,
    pub column: usize,
    /// Index into `Interp::frames` of the workspace of this entry.
    pub frame: usize,
}

/// A script (or `eval` text) running in the frame of its caller: the
/// caller's position, saved while the frame shows the script.
pub(crate) struct ScriptCall {
    frame: usize,
    ctx: Rc<FileCtx>,
    pos: u32,
    fname: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StepMode {
    Over(usize),
    In,
    Out(usize),
}

/// Lines of a loaded file that hold breakpoints: actual line -> indices
/// into `DebugState::breakpoints` (`TEMP` = the run-to-cursor target).
type LineMap = HashMap<usize, Vec<usize>>;

const TEMP: usize = usize::MAX;

#[derive(Default)]
pub struct DebugState {
    controller: Option<Box<dyn DebugController>>,
    breakpoints: Vec<Breakpoint>,
    /// `dbstop if error`.
    pub(crate) stop_on_error: bool,
    /// Run-to-cursor target: canonical path, requested line.
    temp: Option<(PathBuf, usize)>,
    step: Option<StepMode>,
    /// The statement hook must run (see module doc).
    pub(crate) active: bool,
    /// Per activation depth: (file, line, position) of the last statement.
    last: Vec<Option<(usize, usize, u32)>>,
    lines: HashMap<usize, (Weak<FileCtx>, Rc<LineMap>)>,
    paused: bool,
    /// The call stack of the current pause (frames are split while
    /// `debug_eval` runs in a lower frame).
    stack: Vec<StackEntry>,
    /// Selected entry of the paused stack (0 = newest).
    selected: usize,
    /// Resume requested by `dbcont`/`dbstep`/`dbquit` during `debug_eval`.
    pending: Option<Resume>,
    /// The hook is off (condition evaluation).
    quiet: bool,
    /// Address of the last error a `dbstop if error` pause was made for.
    error_seen: usize,
    /// Set when the code was ended by `dbquit` (cleared by `debug_begin`).
    pub quit: bool,
    /// Nesting of `try` blocks (`dbstop if error` stops only outside).
    pub(crate) try_depth: usize,
    pub(crate) scripts: Vec<ScriptCall>,
}

/// Start lines of every statement in `ctx`, sorted.
fn stmt_lines(ctx: &FileCtx) -> Vec<usize> {
    fn block(ctx: &FileCtx, b: &[Stmt], out: &mut Vec<usize>) {
        for s in b {
            stmt(ctx, s, out);
        }
    }
    fn stmt(ctx: &FileCtx, s: &Stmt, out: &mut Vec<usize>) {
        out.push(ctx.line_col(s.span.start).0);
        match &s.kind {
            StmtKind::If { clauses, else_body } => {
                for c in clauses {
                    block(ctx, &c.body, out);
                }
                if let Some(b) = else_body {
                    block(ctx, b, out);
                }
            }
            StmtKind::For { body, .. }
            | StmtKind::Parfor { body, .. }
            | StmtKind::ForKeyValue { body, .. }
            | StmtKind::While { body, .. }
            | StmtKind::DoUntil { body, .. }
            | StmtKind::Spmd { body } => block(ctx, body, out),
            StmtKind::Switch {
                cases, otherwise, ..
            } => {
                for c in cases {
                    block(ctx, &c.body, out);
                }
                if let Some(b) = otherwise {
                    block(ctx, b, out);
                }
            }
            StmtKind::Try {
                body, catch_body, ..
            } => {
                block(ctx, body, out);
                if let Some(b) = catch_body {
                    block(ctx, b, out);
                }
            }
            StmtKind::UnwindProtect { body, cleanup } => {
                block(ctx, body, out);
                block(ctx, cleanup, out);
            }
            _ => {}
        }
    }
    fn function(ctx: &FileCtx, f: &ast::Function, out: &mut Vec<usize>) {
        block(ctx, &f.body, out);
        for n in &f.nested {
            function(ctx, n, out);
        }
    }
    let mut out = Vec::new();
    for item in &ctx.file.items {
        match item {
            ast::Item::Stmt(s) => stmt(ctx, s, &mut out),
            ast::Item::Function(f) => function(ctx, f, &mut out),
            ast::Item::Class(c) => {
                for section in &c.sections {
                    if let ast::ClassSection::Methods { functions, .. } = section {
                        for f in functions {
                            function(ctx, f, &mut out);
                        }
                    }
                }
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn snap(lines: &[usize], line: usize) -> Option<usize> {
    let i = lines.partition_point(|&l| l < line);
    lines.get(i).copied()
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub const NOT_FOUND_ID: &str = "Indy:debug:fileNotFound";

impl DebugState {
    fn refresh(&mut self) {
        self.lines.clear();
        self.active = self.controller.is_some()
            && (self.step.is_some()
                || self.temp.is_some()
                || self.breakpoints.iter().any(|b| b.enabled));
    }
}

impl Interp {
    // ----- embedder interface --------------------------------------------

    /// Install (or remove) the embedder's pause handler. Without one
    /// nothing ever pauses.
    pub fn set_debug_controller(&mut self, controller: Option<Box<dyn DebugController>>) {
        self.debug.controller = controller;
        self.debug.refresh();
    }

    /// Reset the per-run state before the embedder runs new code.
    pub fn debug_begin(&mut self) {
        let d = &mut self.debug;
        d.step = None;
        d.temp = None;
        d.last.clear();
        d.paused = false;
        d.pending = None;
        d.quiet = false;
        d.quit = false;
        d.error_seen = 0;
        d.try_depth = 0;
        d.scripts.clear();
        d.refresh();
    }

    /// Forget the per-run state after the code ended (breakpoints stay).
    pub fn debug_end(&mut self) {
        self.debug_begin();
    }

    /// True while a controller handles a pause.
    pub fn debug_paused(&self) -> bool {
        self.debug.paused
    }

    /// Set (or replace) the breakpoint at `path`:`line`; returns the line
    /// it stops at.
    pub fn set_breakpoint(
        &mut self,
        path: &Path,
        line: usize,
        condition: &str,
        enabled: bool,
    ) -> R<usize> {
        let path = self.breakpoint_file(path)?;
        let actual = self.actual_line(&path, line)?;
        let bp = Breakpoint {
            path: path.clone(),
            line,
            condition: condition.trim().to_string(),
            enabled,
        };
        match self
            .debug
            .breakpoints
            .iter_mut()
            .find(|b| b.path == path && b.line == line)
        {
            Some(b) => *b = bp,
            None => self.debug.breakpoints.push(bp),
        }
        self.debug.refresh();
        Ok(actual)
    }

    /// Remove the breakpoint requested at `path`:`line`.
    pub fn clear_breakpoint(&mut self, path: &Path, line: usize) -> bool {
        let path = canonical(path);
        let before = self.debug.breakpoints.len();
        self.debug
            .breakpoints
            .retain(|b| !(b.path == path && b.line == line));
        self.debug.refresh();
        self.debug.breakpoints.len() != before
    }

    /// Remove all breakpoints, or those of one file; returns the count.
    pub fn clear_breakpoints(&mut self, path: Option<&Path>) -> usize {
        let before = self.debug.breakpoints.len();
        match path.map(canonical) {
            Some(p) => self.debug.breakpoints.retain(|b| b.path != p),
            None => self.debug.breakpoints.clear(),
        }
        self.debug.refresh();
        before - self.debug.breakpoints.len()
    }

    /// Every breakpoint with the line it stops at in the file's current
    /// text (`None` when the file cannot be read or has no code there).
    pub fn breakpoints(&mut self) -> Vec<(Breakpoint, Option<usize>)> {
        let list = self.debug.breakpoints.clone();
        list.into_iter()
            .map(|b| {
                let actual = self.actual_line(&b.path, b.line).ok();
                (b, actual)
            })
            .collect()
    }

    pub fn stop_on_error(&self) -> bool {
        self.debug.stop_on_error
    }

    pub fn set_stop_on_error(&mut self, on: bool) {
        self.debug.stop_on_error = on;
    }

    /// Breakpoints without the per-run state (to carry them over a reset).
    pub fn take_breakpoints(&mut self) -> (Vec<Breakpoint>, bool) {
        let list = std::mem::take(&mut self.debug.breakpoints);
        self.debug.refresh();
        (list, self.debug.stop_on_error)
    }

    pub fn restore_breakpoints(&mut self, list: Vec<Breakpoint>, stop_on_error: bool) {
        self.debug.breakpoints = list;
        self.debug.stop_on_error = stop_on_error;
        self.debug.refresh();
    }

    /// While paused: stop at `path`:`line` (temporary breakpoint) and
    /// continue. Returns the line it stops at.
    pub fn debug_run_to(&mut self, path: &Path, line: usize) -> R<usize> {
        if self.debug.temp.is_some() {
            return Err(err(
                "Indy:debug:runToCursorPending",
                "Run to Cursor is already pending.",
            ));
        }
        let path = self.breakpoint_file(path)?;
        let actual = self.actual_line(&path, line)?;
        self.debug.temp = Some((path, line));
        self.debug.refresh();
        Ok(actual)
    }

    /// The pending run-to-cursor target: path and the line it stops at.
    pub fn debug_run_to_target(&mut self) -> Option<(PathBuf, usize)> {
        let (path, line) = self.debug.temp.clone()?;
        let actual = self.actual_line(&path, line).unwrap_or(line);
        Some((path, actual))
    }

    /// The paused call stack, newest first. Code that is not a file (the
    /// Command Window, `eval` text) and anonymous functions are not entries.
    pub fn debug_stack(&self) -> Vec<StackEntry> {
        if self.debug.paused {
            return self.debug.stack.clone();
        }
        self.live_stack()
    }

    fn live_stack(&self) -> Vec<StackEntry> {
        let mut out = Vec::new();
        let mut entry = |ctx: &Rc<FileCtx>, pos: u32, fname: &str, frame: usize| {
            if !ctx.file_backed {
                return;
            }
            let (line, column) = ctx.line_col(pos);
            out.push(StackEntry {
                name: fname.to_string(),
                file: canonical(&ctx.path).display().to_string(),
                line,
                column,
                frame,
            });
        };
        for (i, f) in self.frames.iter().enumerate().rev() {
            if f.fname == "@<anonymous>" {
                continue;
            }
            entry(&f.ctx, f.pos, &f.fname, i);
            for s in self.debug.scripts.iter().rev().filter(|s| s.frame == i) {
                entry(&s.ctx, s.pos, &s.fname, i);
            }
        }
        out
    }

    /// Index of the selected stack entry (0 = newest).
    pub fn debug_selected(&self) -> usize {
        self.debug.selected
    }

    /// Select stack entry `index` (0 = newest); false when out of range.
    pub fn debug_select(&mut self, index: usize) -> bool {
        if index >= self.debug.stack.len() {
            return false;
        }
        self.debug.selected = index;
        true
    }

    /// Index into `frames` of the selected entry's workspace.
    pub fn debug_frame_index(&self) -> usize {
        self.debug
            .stack
            .get(self.debug.selected)
            .map(|e| e.frame)
            .unwrap_or(self.frames.len() - 1)
    }

    /// The selected entry's workspace.
    pub fn debug_frame(&self) -> &Frame {
        &self.frames[self.debug_frame_index()]
    }

    /// Variables of the selected workspace: name, value, global.
    pub fn debug_variables(&self) -> Vec<(String, &Value, bool)> {
        let f = self.debug_frame();
        let mut names: Vec<&String> = f
            .vars
            .keys()
            .chain(f.globals.iter())
            .chain(f.persistents.iter())
            .chain(f.binds.iter().flat_map(|(_, names)| names.iter()))
            .collect();
        names.sort();
        names.dedup();
        names
            .into_iter()
            .filter_map(|n| self.debug_variable(n).map(|(v, g)| (n.clone(), v, g)))
            .collect()
    }

    /// One variable of the selected workspace, and whether it is global.
    pub fn debug_variable(&self, name: &str) -> Option<(&Value, bool)> {
        let f = self.debug_frame();
        if f.globals.contains(name) {
            return self.globals.get(name).map(|v| (v, true));
        }
        if f.persistents.contains(name) {
            return self
                .persist
                .get(&f.persist_key)
                .and_then(|m| m.get(name))
                .map(|v| (v, false));
        }
        if let Some(v) = f.vars.get(name) {
            return Some((v, false));
        }
        // A variable shared with a nested function lives in the callee
        // while that runs.
        let (env, _) = f.binds.iter().find(|(_, names)| names.contains(name))?;
        self.frames[self.debug_frame_index() + 1..]
            .iter()
            .rev()
            .find(|u| {
                u.binds
                    .iter()
                    .any(|(e, names)| Rc::ptr_eq(e, env) && names.contains(name))
            })
            .and_then(|u| u.vars.get(name))
            .map(|v| (v, false))
    }

    /// Move the live copies of the selected frame's shared variables from
    /// the callees into it; returns where each came from.
    fn gather_for_eval(&mut self, fi: usize) -> Vec<(usize, String)> {
        let mut moved = Vec::new();
        let binds: Vec<(Rc<crate::NestedEnv>, Vec<String>)> = self.frames[fi]
            .binds
            .iter()
            .map(|(e, names)| (e.clone(), names.iter().cloned().collect()))
            .collect();
        for (env, names) in binds {
            for name in names {
                if self.frames[fi].vars.contains_key(&name) {
                    continue;
                }
                let holder = (fi + 1..self.frames.len()).rev().find(|&u| {
                    let f = &self.frames[u];
                    f.vars.contains_key(&name)
                        && f.binds
                            .iter()
                            .any(|(e, ns)| Rc::ptr_eq(e, &env) && ns.contains(&name))
                });
                if let Some(u) = holder
                    && let Some(v) = self.frames[u].vars.remove(&name)
                {
                    self.frames[fi].vars.insert(name.clone(), v);
                    moved.push((u, name));
                }
            }
        }
        moved
    }

    /// While paused: run `code` in the selected workspace. Output goes to
    /// the session, an error is printed as `error: <msg>`; breakpoints do
    /// not fire inside. Returns the resume requested by `dbcont`, `dbstep`
    /// or `dbquit` in the code.
    pub fn debug_eval(&mut self, code: &str) -> Option<Resume> {
        let fi = self.debug_frame_index();
        let shared = self.gather_for_eval(fi);
        let upper = self.frames.split_off(fi + 1);
        let quiet = std::mem::replace(&mut self.debug.quiet, true);
        // A pause inside `evalc`: the prompt's output is not the capture's.
        let capture = self.capture.take();
        let r = crate::catch_panic(|| self.run_text(code));
        self.capture = capture;
        self.debug.quiet = quiet;
        self.frames.truncate(fi + 1);
        self.frames.extend(upper);
        for (u, name) in shared {
            if let Some(v) = self.frames[fi].vars.remove(&name) {
                self.frames[u].vars.insert(name, v);
            }
        }
        if let Err(e) = r
            && !self.interrupt
        {
            self.eprint(&format!("error: {}\n", e.message));
        }
        self.flush_session();
        self.debug.pending.take()
    }

    // ----- interpreter side ----------------------------------------------

    fn flush_session(&mut self) {
        if let Some(s) = &mut self.session {
            s.flush();
        }
    }

    fn breakpoint_file(&mut self, path: &Path) -> R<PathBuf> {
        let is_m = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("m"));
        match path.canonicalize() {
            Ok(p) if p.is_file() && is_m => Ok(p),
            _ => Err(err(
                NOT_FOUND_ID,
                format!("No .m file {} for the breakpoint.", path.display()),
            )),
        }
    }

    /// The line a breakpoint requested at `line` stops at in the current
    /// text of `path`.
    fn actual_line(&mut self, path: &Path, line: usize) -> R<usize> {
        // The file as it is now, not the copy cached for running code.
        let ctx = crate::read_source_file(path)?;
        snap(&stmt_lines(&ctx), line.max(1)).ok_or_else(|| {
            err(
                "Indy:debug:noCode",
                format!(
                    "No executable code at or after line {} of {}.",
                    line,
                    path.display()
                ),
            )
        })
    }

    /// Breakpoint lines of a loaded file (cached per loaded text).
    fn line_map(&mut self, ctx: &Rc<FileCtx>) -> Rc<LineMap> {
        let key = Rc::as_ptr(ctx) as usize;
        if let Some((w, m)) = self.debug.lines.get(&key)
            && w.upgrade().is_some_and(|c| Rc::ptr_eq(&c, ctx))
        {
            return m.clone();
        }
        let path = canonical(&ctx.path);
        let mut map = LineMap::new();
        let wanted: Vec<(usize, usize)> = self
            .debug
            .breakpoints
            .iter()
            .enumerate()
            .filter(|(_, b)| b.enabled && b.path == path)
            .map(|(i, b)| (i, b.line))
            .chain(
                self.debug
                    .temp
                    .iter()
                    .filter(|(p, _)| *p == path)
                    .map(|(_, l)| (TEMP, *l)),
            )
            .collect();
        if !wanted.is_empty() {
            let lines = stmt_lines(ctx);
            for (i, l) in wanted {
                if let Some(a) = snap(&lines, l) {
                    map.entry(a).or_default().push(i);
                }
            }
        }
        let map = Rc::new(map);
        self.debug
            .lines
            .insert(key, (Rc::downgrade(ctx), map.clone()));
        map
    }

    fn debug_depth(&self) -> usize {
        self.frames.len() + self.debug.scripts.len()
    }

    /// Record the running statement as its activation's last one; true
    /// when it starts a new line (module doc).
    fn debug_mark(&mut self, ctx: &Rc<FileCtx>, line: usize, pos: u32) -> bool {
        let depth = self.debug_depth();
        let key = (Rc::as_ptr(ctx) as usize, line, pos);
        let last = &mut self.debug.last;
        last.resize(depth + 1, None);
        let new = match last[depth] {
            Some((p, l, q)) => p != key.0 || l != line || pos <= q,
            None => true,
        };
        last[depth] = Some(key);
        new
    }

    /// The statement hook (only while `debug.active`).
    pub(crate) fn debug_hook(&mut self) -> R<()> {
        if self.debug.quiet || self.debug.paused {
            return Ok(());
        }
        let (ctx, pos) = {
            let f = self.frame();
            (f.ctx.clone(), f.pos)
        };
        if !ctx.file_backed {
            return Ok(());
        }
        let line = ctx.line_col(pos).0;
        if !self.debug_mark(&ctx, line, pos) {
            return Ok(());
        }
        let depth = self.debug_depth();
        let mut reason = match self.debug.step {
            Some(StepMode::In) => Some(PauseReason::Step),
            Some(StepMode::Over(d)) if depth <= d => Some(PauseReason::Step),
            Some(StepMode::Out(d)) if depth < d => Some(PauseReason::Step),
            _ => None,
        };
        let hits = self.line_map(&ctx).get(&line).cloned();
        for i in hits.unwrap_or_default() {
            if i == TEMP {
                self.debug.temp = None;
                self.debug.refresh();
                reason = Some(PauseReason::RunToCursor);
                continue;
            }
            if reason.is_some() {
                continue;
            }
            let Some(b) = self.debug.breakpoints.get(i) else {
                continue;
            };
            if b.condition.is_empty() || self.breakpoint_condition(&b.condition.clone())? {
                reason = Some(PauseReason::Breakpoint);
            }
        }
        match reason {
            Some(r) => self.debug_pause(r),
            None => Ok(()),
        }
    }

    /// A loop goes round: its header line runs again (MATLAB stops there
    /// on every iteration).
    pub(crate) fn debug_loop_head(&mut self, head: u32) -> R<()> {
        self.frame_mut().pos = head;
        self.debug_hook()
    }

    /// Evaluate a breakpoint condition in the running frame. An error is
    /// reported and counts as true (the code pauses there, as in Octave).
    fn breakpoint_condition(&mut self, cond: &str) -> R<bool> {
        let quiet = std::mem::replace(&mut self.debug.quiet, true);
        let r = crate::catch_panic(|| {
            let eval = *self
                .builtins
                .get("eval")
                .ok_or_else(|| Error::msg("eval is not available"))?;
            let v = eval(self, vec![Value::str(cond)], 1)?;
            match v.first() {
                Some(v) => v.is_true(),
                None => Err(Error::msg("the condition has no value")),
            }
        });
        self.debug.quiet = quiet;
        match r {
            Ok(b) => Ok(b),
            Err(e) if self.interrupt => Err(e),
            Err(e) => {
                self.eprint(&format!(
                    "Error evaluating breakpoint condition: {}\n",
                    e.message
                ));
                Ok(true)
            }
        }
    }

    /// Pause here: hand control to the controller and apply its resume.
    pub fn debug_pause(&mut self, reason: PauseReason) -> R<()> {
        if self.debug.paused || self.debug.quiet {
            return Ok(());
        }
        let Some(mut ctl) = self.debug.controller.take() else {
            return Ok(());
        };
        self.flush_session();
        // The current statement counts as seen: stepping goes to the next line.
        let (ctx, pos) = {
            let f = self.frame();
            (f.ctx.clone(), f.pos)
        };
        if ctx.file_backed {
            let line = ctx.line_col(pos).0;
            self.debug_mark(&ctx, line, pos);
        }
        let depth = self.debug_depth();
        self.debug.step = None;
        self.debug.stack = self.live_stack();
        self.debug.paused = true;
        self.debug.selected = 0;
        self.debug.pending = None;
        let resume = ctl.pause(self, reason);
        self.debug.paused = false;
        self.debug.stack.clear();
        self.debug.selected = 0;
        self.debug.controller = Some(ctl);
        self.debug.step = match resume {
            Resume::Step => Some(StepMode::Over(depth)),
            Resume::StepIn => Some(StepMode::In),
            Resume::StepOut => Some(StepMode::Out(depth)),
            _ => None,
        };
        self.debug.refresh();
        match resume {
            Resume::Quit | Resume::Stop => {
                self.debug.quit = resume == Resume::Quit;
                self.debug.temp = None;
                self.debug.refresh();
                self.interrupt = true;
                Err(Interp::interrupted_error())
            }
            _ => Ok(()),
        }
    }

    /// `dbstop if error`: pause where an error not caught by `try` arises.
    pub(crate) fn debug_error<T>(&mut self, r: R<T>) -> R<T> {
        let Err(e) = r else { return r };
        let addr = &*e as *const Error as usize;
        if self.interrupt
            || self.debug.try_depth > 0
            || self.debug.quiet
            || self.debug.paused
            || self.debug.controller.is_none()
            || self.debug.error_seen == addr
        {
            return Err(e);
        }
        self.debug.error_seen = addr;
        self.eprint(&format!("Paused on error: {}\n", e.message));
        self.debug_pause(PauseReason::Error)?;
        Err(e)
    }

    /// A script (or `eval` text) starts running in the current frame.
    pub(crate) fn debug_enter_script(&mut self, ctx: Rc<FileCtx>, pos: u32, fname: String) {
        let frame = self.frames.len() - 1;
        self.debug.scripts.push(ScriptCall {
            frame,
            ctx,
            pos,
            fname,
        });
    }

    pub(crate) fn debug_leave_script(&mut self) {
        self.debug.scripts.pop();
    }
}

// ----- builtins -----------------------------------------------------------

/// The interpreter's `dbstack`, used outside a pause.
static PLAIN_DBSTACK: OnceLock<BuiltinFn> = OnceLock::new();

pub(crate) fn register(it: &mut Interp) {
    if let Some(&plain) = it.builtins.get("dbstack") {
        let _ = PLAIN_DBSTACK.set(plain);
    }
    it.builtins.insert("dbstack", dbstack);
    it.builtins.insert("keyboard", keyboard);
    it.builtins.insert("dbstop", dbstop);
    it.builtins.insert("dbclear", dbclear);
    it.builtins.insert("dbstatus", dbstatus);
    it.builtins.insert("dbcont", dbcont);
    it.builtins.insert("dbstep", dbstep);
    it.builtins.insert("dbquit", dbquit);
    it.builtins.insert("dbup", dbup);
    it.builtins.insert("dbdown", dbdown);
}

/// `dbstack`: while paused the stack of the pause, with the selected entry
/// as second output.
fn dbstack(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    if !it.debug.paused {
        return match PLAIN_DBSTACK.get() {
            Some(plain) => plain(it, a, nargout),
            None => Ok(vec![]),
        };
    }
    let mut omit = 0;
    let mut complete = false;
    for v in &a {
        match v.to_str() {
            Some(s) => complete |= s.eq_ignore_ascii_case("-completenames"),
            None => omit = v.scalar_f64().unwrap_or(0.0).max(0.0) as usize,
        }
    }
    let selected = it.debug.selected;
    let stack: Vec<StackEntry> = it.debug.stack.iter().skip(omit).cloned().collect();
    if nargout == 0 {
        let mut text = String::new();
        for (i, e) in stack.iter().enumerate() {
            let mark = if i + omit == selected { "-->" } else { "   " };
            text.push_str(&format!("{mark} In {} (line {})\n", e.name, e.line));
        }
        it.print(&text);
        return Ok(vec![]);
    }
    let elems: Vec<Vec<Value>> = stack
        .iter()
        .map(|e| {
            let file = if complete {
                e.file.clone()
            } else {
                Path::new(&e.file)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default()
            };
            vec![
                Value::str(&file),
                Value::str(&e.name),
                Value::num(e.line as f64),
                Value::num(e.column as f64),
            ]
        })
        .collect();
    let st = Value::Struct(StructArray {
        dims: vec![elems.len(), 1],
        fields: vec!["file".into(), "name".into(), "line".into(), "column".into()],
        elems,
    });
    let index = Value::num((selected + 1).saturating_sub(omit) as f64);
    let mut out = vec![st, index];
    out.truncate(nargout.max(1));
    Ok(out)
}

/// `keyboard`: a real debugger pause; nothing without a controller (CLI)
/// or inside a pause.
fn keyboard(it: &mut Interp, _: Vec<Value>, _: usize) -> R<Vec<Value>> {
    it.debug_pause(PauseReason::Keyboard)?;
    Ok(vec![])
}

fn words(a: &[Value], fname: &str) -> R<Vec<String>> {
    a.iter()
        .map(|v| {
            if v.is_numeric() && v.is_scalar() {
                Ok(format!("{}", v.scalar_f64().unwrap_or(0.0)))
            } else {
                arg_str(v, fname)
            }
        })
        .collect()
}

fn parse_line(s: &str, fname: &str) -> R<usize> {
    match s.parse::<f64>() {
        Ok(v) if v >= 1.0 && v.fract() == 0.0 => Ok(v as usize),
        _ => Err(err(
            "Indy:debug:badLine",
            format!("{fname}: invalid line number '{s}'."),
        )),
    }
}

/// File and (for `NAME`, `file>sub`) the first statement line of a
/// function named in `dbstop`/`dbclear`.
fn locate(it: &mut Interp, name: &str, fname: &str) -> R<(PathBuf, Option<usize>)> {
    let (main, sub) = match name.split_once('>') {
        Some((m, s)) => (m, Some(s)),
        None => (name, None),
    };
    let main = main.strip_suffix(".m").unwrap_or(main);
    let missing = || {
        err(
            "Indy:debug:noFunction",
            format!("{fname}: no file or function '{name}' found."),
        )
    };
    let as_path = PathBuf::from(format!("{main}.m"));
    let ctx = if main.contains('/') && as_path.is_file() {
        it.load_file(&as_path)?
    } else {
        match it.resolve(main)? {
            Some(crate::Callee::User(ctx, _)) | Some(crate::Callee::Script(ctx)) => ctx,
            _ => return Err(missing()),
        }
    };
    let first = |f: &ast::Function| f.body.first().map(|s| ctx.line_col(s.span.start).0);
    let line = match sub {
        Some(s) => Some(
            ctx.functions
                .get(s)
                .map(|f| first(f))
                .ok_or_else(missing)?
                .ok_or_else(missing)?,
        ),
        None => ctx.main.as_ref().and_then(|f| first(f)),
    };
    Ok((ctx.path.clone(), line))
}

/// `dbstop in NAME [at LINE] [if COND]`, `dbstop NAME LINE`, `dbstop if error`.
fn dbstop(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let w = words(&a, "dbstop")?;
    if w.first().is_some_and(|s| s == "if") {
        if w.get(1).is_some_and(|s| s == "error") {
            it.debug.stop_on_error = true;
            return Ok(vec![]);
        }
        return Err(err(
            "Indy:debug:unsupported",
            "dbstop: only 'dbstop if error' is supported among the 'if' forms.",
        ));
    }
    let mut rest = w.as_slice();
    if rest.first().is_some_and(|s| s == "in") {
        rest = &rest[1..];
    }
    let Some(name) = rest.first() else {
        return Err(err(
            "Indy:debug:usage",
            "dbstop: a function name is needed.",
        ));
    };
    rest = &rest[1..];
    let mut line = None;
    if rest.first().is_some_and(|s| s == "at") {
        rest = &rest[1..];
    }
    if let Some(s) = rest.first()
        && s != "if"
    {
        line = Some(parse_line(s, "dbstop")?);
        rest = &rest[1..];
    }
    let cond = match rest.first() {
        Some(s) if s == "if" => rest[1..].join(" "),
        Some(s) => {
            return Err(err(
                "Indy:debug:usage",
                format!("dbstop: unexpected '{s}'."),
            ));
        }
        None => String::new(),
    };
    let (path, first) = locate(it, name, "dbstop")?;
    let line = line.or(first).ok_or_else(|| {
        err(
            "Indy:debug:noCode",
            format!("dbstop: '{name}' has no executable code."),
        )
    })?;
    let actual = it.set_breakpoint(&path, line, &cond, true)?;
    if nargout > 0 {
        return one(Value::num(actual as f64));
    }
    Ok(vec![])
}

/// `dbclear all`, `dbclear in NAME [at LINE]`, `dbclear NAME [LINE]`,
/// `dbclear if error`.
fn dbclear(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let w = words(&a, "dbclear")?;
    match w.first().map(String::as_str) {
        None => {
            return Err(err(
                "Indy:debug:usage",
                "dbclear: say what to clear (all, a function name, or 'if error').",
            ));
        }
        Some("all") => {
            it.clear_breakpoints(None);
            it.debug.stop_on_error = false;
            return Ok(vec![]);
        }
        Some("if") => {
            it.debug.stop_on_error = false;
            return Ok(vec![]);
        }
        _ => {}
    }
    let mut rest = w.as_slice();
    if rest.first().is_some_and(|s| s == "in") {
        rest = &rest[1..];
    }
    let Some(name) = rest.first() else {
        return Err(err(
            "Indy:debug:usage",
            "dbclear: a function name is needed.",
        ));
    };
    rest = &rest[1..];
    if rest.first().is_some_and(|s| s == "at") {
        rest = &rest[1..];
    }
    let (path, _) = locate(it, name, "dbclear")?;
    match rest.first() {
        None => {
            it.clear_breakpoints(Some(&path));
        }
        Some(s) => {
            let line = parse_line(s, "dbclear")?;
            // The requested line, or a breakpoint that stops at that line.
            let path = canonical(&path);
            let targets: Vec<usize> = it
                .breakpoints()
                .into_iter()
                .filter(|(b, actual)| b.path == path && (b.line == line || *actual == Some(line)))
                .map(|(b, _)| b.line)
                .collect();
            for l in targets {
                it.clear_breakpoint(&path, l);
            }
        }
    }
    Ok(vec![])
}

/// `dbstatus`: prints the breakpoints, or returns an N×1 struct (one
/// element per file) with `name`, `file`, `line` and `cond`.
fn dbstatus(it: &mut Interp, _: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let mut files: Vec<(PathBuf, Vec<(usize, String)>)> = Vec::new();
    for (b, actual) in it.breakpoints() {
        if !b.enabled {
            continue;
        }
        let Some(line) = actual else { continue };
        match files.iter_mut().find(|(p, _)| *p == b.path) {
            Some((_, v)) => v.push((line, b.condition.clone())),
            None => files.push((b.path.clone(), vec![(line, b.condition.clone())])),
        }
    }
    for (_, v) in &mut files {
        v.sort();
        v.dedup_by(|x, y| x.0 == y.0);
    }
    let name_of = |p: &Path| {
        p.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    if nargout == 0 {
        let mut text = String::new();
        for (p, v) in &files {
            let lines: Vec<String> = v.iter().map(|(l, _)| l.to_string()).collect();
            text.push_str(&format!(
                "Breakpoint in {} at line {}\n",
                name_of(p),
                lines.join(", ")
            ));
        }
        if it.debug.stop_on_error {
            text.push_str("Stop if error\n");
        }
        it.print(&text);
        return Ok(vec![]);
    }
    let elems = files
        .iter()
        .map(|(p, v)| {
            vec![
                Value::str(&name_of(p)),
                Value::str(&p.display().to_string()),
                Value::row(v.iter().map(|(l, _)| *l as f64).collect()),
                Value::cell_row(v.iter().map(|(_, c)| Value::str(c)).collect()),
            ]
        })
        .collect::<Vec<_>>();
    one(Value::Struct(StructArray {
        dims: vec![elems.len(), 1],
        fields: vec!["name".into(), "file".into(), "line".into(), "cond".into()],
        elems,
    }))
}

fn need_pause(it: &Interp, fname: &str) -> R<()> {
    if it.debug.paused {
        return Ok(());
    }
    Err(err(
        "Indy:debug:notPaused",
        format!("{fname}: only valid while the debugger is paused."),
    ))
}

fn dbcont(it: &mut Interp, _: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need_pause(it, "dbcont")?;
    it.debug.pending = Some(Resume::Continue);
    Ok(vec![])
}

fn dbquit(it: &mut Interp, _: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need_pause(it, "dbquit")?;
    it.debug.pending = Some(Resume::Quit);
    Ok(vec![])
}

fn dbstep(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need_pause(it, "dbstep")?;
    let w = words(&a, "dbstep")?;
    it.debug.pending = Some(match w.first().map(String::as_str) {
        Some("in") => Resume::StepIn,
        Some("out") => Resume::StepOut,
        _ => Resume::Step,
    });
    Ok(vec![])
}

fn move_frame(it: &mut Interp, a: Vec<Value>, up: bool, fname: &str) -> R<Vec<Value>> {
    need_pause(it, fname)?;
    let w = words(&a, fname)?;
    let n = match w.first() {
        Some(s) => parse_line(s, fname)?,
        None => 1,
    };
    let len = it.debug.stack.len();
    let cur = it.debug.selected;
    let target = if up {
        (cur + n).min(len.saturating_sub(1))
    } else {
        cur.saturating_sub(n)
    };
    it.debug.selected = target;
    if let Some(e) = it.debug.stack.get(target) {
        let text = format!("{}: {} at line {}\n", fname, e.name, e.line);
        it.print(&text);
    }
    Ok(vec![])
}

fn dbup(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    move_frame(it, a, true, "dbup")
}

fn dbdown(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    move_frame(it, a, false, "dbdown")
}
