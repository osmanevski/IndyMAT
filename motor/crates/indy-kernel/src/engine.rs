//! The engine interface behind the kernel protocol (`docs/Kernel-Protokolu.md`).
//!
//! An engine never writes to stdout. Everything user-visible goes through
//! [`Host`], which the protocol server implements.

use crate::json::Json;
use std::path::{Path, PathBuf};

/// What the engine is. `stub` is true for anything but the real interpreter.
#[derive(Clone, Debug)]
pub struct EngineInfo {
    pub name: String,
    pub version: String,
    pub stub: bool,
    /// User-facing one-line description, shown as the engine version text.
    pub description: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvalMode {
    Code,
    Section,
    File,
}

impl EvalMode {
    pub fn parse(text: &str) -> Option<EvalMode> {
        match text {
            "code" => Some(EvalMode::Code),
            "section" => Some(EvalMode::Section),
            "file" => Some(EvalMode::File),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EvalRequest {
    pub code: String,
    pub mode: EvalMode,
    /// Source file of the code, if any (always set for `File`).
    pub path: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

impl Stream {
    pub fn name(self) -> &'static str {
        match self {
            Stream::Stdout => "stdout",
            Stream::Stderr => "stderr",
        }
    }
}

/// One open figure, reported after each eval (complete list, in order).
#[derive(Clone, Debug)]
pub struct FigureEvent {
    pub number: u64,
    pub name: String,
    /// Figure-Data-v3 object, or `None` when only a PNG (or nothing) exists.
    pub figure: Option<Json>,
    pub png_base64: Option<String>,
    /// Why the figure is not interactive; empty when it is.
    pub reason: String,
}

/// The eval was interrupted (SIGINT) or the client went away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interrupted;

#[derive(Clone, Debug, PartialEq)]
pub struct SourceLocation {
    pub file: PathBuf,
    pub line: u64,
}

/// How a failed eval ended.
#[derive(Clone, Debug, PartialEq)]
pub struct EvalError {
    pub message: String,
    pub interrupted: bool,
    /// The debugger's `quit` ended the code (`interrupted` is true as well).
    pub quit: bool,
    pub location: Option<SourceLocation>,
}

impl EvalError {
    pub fn new(message: impl Into<String>) -> EvalError {
        EvalError {
            message: message.into(),
            interrupted: false,
            quit: false,
            location: None,
        }
    }

    pub fn interrupted() -> EvalError {
        EvalError {
            message: "Operation interrupted.".into(),
            interrupted: true,
            quit: false,
            location: None,
        }
    }
}

impl From<Interrupted> for EvalError {
    fn from(_: Interrupted) -> EvalError {
        EvalError::interrupted()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    NotFound,
    Invalid,
    Unsupported,
    Engine,
}

impl ErrorCode {
    pub fn name(self) -> &'static str {
        match self {
            ErrorCode::NotFound => "not_found",
            ErrorCode::Invalid => "invalid",
            ErrorCode::Unsupported => "unsupported",
            ErrorCode::Engine => "engine",
        }
    }
}

/// Failure of a non-eval operation; maps to a protocol error response.
#[derive(Clone, Debug, PartialEq)]
pub struct EngineError {
    pub code: ErrorCode,
    pub message: String,
    /// UI feature name for `Unsupported`.
    pub feature: Option<String>,
}

impl EngineError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> EngineError {
        EngineError {
            code,
            message: message.into(),
            feature: None,
        }
    }

    pub fn unsupported(feature: &str, message: impl Into<String>) -> EngineError {
        EngineError {
            code: ErrorCode::Unsupported,
            message: message.into(),
            feature: Some(feature.to_string()),
        }
    }
}

/// One row of the Workspace panel.
#[derive(Clone, Debug, PartialEq)]
pub struct VariableSummary {
    pub name: String,
    pub class: String,
    pub size: Vec<u64>,
    pub bytes: u64,
    pub complex: bool,
    pub global: bool,
    /// Short display text (at most a few dozen characters).
    pub preview: String,
}

/// A rectangular page of a variable for `inspect` (1-based).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageRequest {
    pub row: u64,
    pub column: u64,
    pub rows: u64,
    pub columns: u64,
}

impl Default for PageRequest {
    fn default() -> PageRequest {
        PageRequest {
            row: 1,
            column: 1,
            rows: 100,
            columns: 30,
        }
    }
}

/// The next thing that happens during a debugger pause.
#[derive(Clone, Debug)]
pub enum PauseRequest {
    /// A request the engine must answer with [`Host::pause_reply`].
    Request { id: u64, op: String, body: Json },
    /// Interrupt, or the client went away: end the code.
    Interrupted,
    /// This host cannot pause: go on.
    Unsupported,
}

/// The services an engine may use while evaluating code.
pub trait Host {
    fn output(&mut self, stream: Stream, text: &str);
    /// The user cleared the Command Window (`clc`).
    fn clc(&mut self);
    fn figure(&mut self, figure: FigureEvent);
    /// Ask the user for one line of input; blocks until reply or interrupt.
    fn input(&mut self, prompt: &str) -> Result<String, Interrupted>;
    /// True once the eval must stop. Call between statements and in long loops.
    fn interrupted(&mut self) -> bool;
    /// Report a debugger pause (the fields of the `paused` event).
    fn paused(&mut self, _fields: Vec<(&'static str, Json)>) {}
    /// Wait during a pause for the next request.
    fn pause_wait(&mut self) -> PauseRequest {
        PauseRequest::Unsupported
    }
    /// Answer a request from [`Host::pause_wait`].
    fn pause_reply(&mut self, _id: u64, _result: Result<Json, EngineError>) {}
}

/// A computation engine. The phase 2 interpreter implements this trait.
pub trait Engine {
    fn info(&self) -> EngineInfo;
    /// Run code to completion, error or interrupt. Figures are reported
    /// through `host.figure` after the code ends (complete list).
    fn eval(&mut self, request: &EvalRequest, host: &mut dyn Host) -> Result<(), EvalError>;
    /// Variables of the base workspace, sorted by name.
    fn workspace(&self) -> Vec<VariableSummary>;
    /// A page of a variable in the protocol's variable-page shape.
    fn inspect(&self, name: &str, page: PageRequest) -> Result<Json, EngineError>;
    fn cwd(&self) -> PathBuf;
    fn cd(&mut self, path: &Path) -> Result<PathBuf, EngineError>;
    /// Empty workspace, close figures, return to the start folder.
    fn reset(&mut self);

    /// The engine implements `debug`, `breakpoint` and `run_to_cursor`.
    fn debugger(&self) -> bool {
        false
    }

    /// The `breakpoint` op while no code runs (only with [`Engine::debugger`]).
    fn breakpoint(&mut self, _body: &Json) -> Result<Json, EngineError> {
        Err(EngineError::unsupported(
            "breakpoints",
            "Breakpoints are not supported by this engine yet.",
        ))
    }

    fn rename(&mut self, _old: &str, _new: &str) -> Result<(), EngineError> {
        Err(EngineError::unsupported(
            "workspace_edit",
            "Renaming variables is not supported by this engine yet.",
        ))
    }

    /// Clear the named variables, or all when `None`. Returns cleared names.
    fn clear(&mut self, _names: Option<&[String]>) -> Result<Vec<String>, EngineError> {
        Err(EngineError::unsupported(
            "workspace_edit",
            "Clearing variables is not supported by this engine yet.",
        ))
    }
}
