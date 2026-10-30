//! The debugger behind the kernel protocol (`docs/Kernel-Protokolu.md`,
//! section Debugger): the `breakpoint` op and the request loop of a pause.
//!
//! A pause runs inside the interpreter's statement hook (see
//! `indy_interp::debug`): [`DebugBridge::pause`] writes the `paused` event and
//! serves requests through [`Host`] until one of them resumes the code.

use crate::engine::{EngineError, ErrorCode, Host, PauseRequest};
use crate::json::Json;
use crate::server;
use crate::variables;
use indy_interp::debug::{DebugController, PauseReason, Resume};
use indy_interp::{Error, Interp};
use std::path::PathBuf;

pub(crate) struct DebugBridge {
    /// The `&mut dyn Host` of the running `eval`, lifetime erased; see
    /// `InterpEngine::eval`. `HostBridge` holds the same pointer; neither
    /// uses it while the other is running.
    pub host: *mut (dyn Host + 'static),
}

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::new(ErrorCode::Invalid, message)
}

fn engine_error(e: &Error) -> EngineError {
    let code = if e.identifier == indy_interp::debug::NOT_FOUND_ID {
        ErrorCode::NotFound
    } else {
        ErrorCode::Invalid
    };
    EngineError::new(code, e.message.clone())
}

fn path_param(body: &Json) -> Result<PathBuf, EngineError> {
    match body.get("path").and_then(Json::as_str) {
        Some(p) if !p.is_empty() => Ok(PathBuf::from(p)),
        _ => Err(invalid("path must be a non-empty string.")),
    }
}

fn line_param(body: &Json) -> Result<usize, EngineError> {
    match body.get("line").and_then(Json::as_u64) {
        Some(n) if n >= 1 => Ok(n as usize),
        _ => Err(invalid("line must be an integer of at least 1.")),
    }
}

/// The `breakpoint` op, idle or paused.
pub(crate) fn breakpoint_op(it: &mut Interp, body: &Json) -> Result<Json, EngineError> {
    match body.get("action").and_then(Json::as_str) {
        Some("set") => {
            let (path, line) = (path_param(body)?, line_param(body)?);
            let condition = match body.get("condition") {
                None | Some(Json::Null) => "",
                Some(Json::String(c)) => c.as_str(),
                Some(_) => return Err(invalid("condition must be a string.")),
            };
            let enabled = match body.get("enabled") {
                None | Some(Json::Null) => true,
                Some(Json::Bool(b)) => *b,
                Some(_) => return Err(invalid("enabled must be true or false.")),
            };
            let actual = it
                .set_breakpoint(&path, line, condition, enabled)
                .map_err(|e| engine_error(&e))?;
            Ok(Json::object()
                .with("path", path.display().to_string())
                .with("line", line)
                .with("actual_line", actual))
        }
        Some("clear") => {
            let (path, line) = (path_param(body)?, line_param(body)?);
            let cleared = it.clear_breakpoint(&path, line);
            Ok(Json::object()
                .with("path", path.display().to_string())
                .with("line", line)
                .with("cleared", cleared))
        }
        Some("clear_all") => {
            let path = match body.get("path") {
                None | Some(Json::Null) => None,
                Some(_) => Some(path_param(body)?),
            };
            Ok(Json::object().with("cleared", it.clear_breakpoints(path.as_deref())))
        }
        Some("list") => {
            let rows = it
                .breakpoints()
                .into_iter()
                .map(|(b, actual)| {
                    Json::object()
                        .with("path", b.path.display().to_string())
                        .with("line", b.line)
                        .with("actual_line", actual.map_or(Json::Null, Json::from))
                        .with("enabled", b.enabled)
                        .with("condition", b.condition)
                })
                .collect::<Vec<_>>();
            Ok(Json::object()
                .with("breakpoints", rows)
                .with("stop_on_error", it.stop_on_error()))
        }
        _ => Err(invalid("action must be set, clear, clear_all or list.")),
    }
}

fn workspace_rows(it: &Interp) -> Json {
    server::variable_rows(
        it.debug_variables()
            .into_iter()
            .map(|(name, value, global)| variables::summary(&name, value, global))
            .take(500)
            .collect(),
    )
}

fn inspect(it: &Interp, body: &Json) -> Result<Json, EngineError> {
    let page = server::page_request(body).map_err(invalid)?;
    let Some(name) = body.get("name").and_then(Json::as_str) else {
        return Err(invalid("name must be a string."));
    };
    variables::page(name, it.debug_variable(name).map(|(v, _)| v), page)
}

/// The fields of a `paused` event.
fn paused_fields(it: &mut Interp, reason: PauseReason) -> Vec<(&'static str, Json)> {
    let stack = it.debug_stack();
    let selected = it.debug_selected();
    let rows = stack
        .iter()
        .enumerate()
        .map(|(i, e)| {
            Json::object()
                .with("index", i + 1)
                .with("name", e.name.as_str())
                .with("file", e.file.as_str())
                .with("line", e.line)
                .with("column", e.column)
                .with("current", i == selected)
        })
        .collect::<Vec<_>>();
    let (file, line, frame) = match stack.get(selected) {
        Some(e) => (e.file.clone(), e.line, selected + 1),
        None => (String::new(), 0, 0),
    };
    let target = match it.debug_run_to_target() {
        Some((path, line)) => Json::object()
            .with("path", path.display().to_string())
            .with("line", line),
        None => Json::Null,
    };
    vec![
        ("reason", reason.name().into()),
        ("file", file.into()),
        ("line", line.into()),
        ("frame", frame.into()),
        ("stack", rows.into()),
        ("variables", workspace_rows(it)),
        ("run_to_cursor", target),
    ]
}

/// What a request during a pause leads to.
enum Next {
    /// Answered; the pause goes on unchanged.
    Stay,
    /// Answered; report the pause again (selection or variables changed).
    Refresh,
    /// Answered; the code goes on.
    Resume(Resume),
}

fn debug_command(it: &mut Interp, body: &Json) -> Result<(Json, Next), EngineError> {
    let empty = Json::object;
    let resume = |r| Ok((empty(), Next::Resume(r)));
    match body.get("command").and_then(Json::as_str) {
        Some("continue") => resume(Resume::Continue),
        Some("step") => resume(Resume::Step),
        Some("in") => resume(Resume::StepIn),
        Some("out") => resume(Resume::StepOut),
        Some("quit") => resume(Resume::Quit),
        Some("eval") => {
            let code = match body.get("code").and_then(Json::as_str) {
                Some(code) if !code.trim().is_empty() && !code.contains(['\n', '\r']) => code,
                _ => return Err(invalid("code must be one non-empty line.")),
            };
            let requested = it.debug_eval(code);
            let next = match requested {
                _ if it.interrupt => Next::Resume(Resume::Stop),
                Some(r) => Next::Resume(r),
                None => Next::Refresh,
            };
            Ok((empty(), next))
        }
        Some("frame") => {
            let index = body.get("frame").and_then(Json::as_u64).unwrap_or(0) as usize;
            if index == 0 || !it.debug_select(index - 1) {
                return Err(invalid("The call stack frame is no longer valid."));
            }
            Ok((empty(), Next::Refresh))
        }
        Some("inspect") => Ok((inspect(it, body)?, Next::Stay)),
        _ => Err(invalid(
            "command must be continue, step, in, out, quit, eval, frame or inspect.",
        )),
    }
}

fn paused_request(it: &mut Interp, op: &str, body: &Json) -> Result<(Json, Next), EngineError> {
    match op {
        "debug" => debug_command(it, body),
        "breakpoint" => Ok((breakpoint_op(it, body)?, Next::Stay)),
        "workspace" => Ok((
            Json::object().with("variables", workspace_rows(it)),
            Next::Stay,
        )),
        "inspect" => Ok((inspect(it, body)?, Next::Stay)),
        "run_to_cursor" => {
            let (path, line) = (path_param(body)?, line_param(body)?);
            let actual = it.debug_run_to(&path, line).map_err(|e| engine_error(&e))?;
            let result = Json::object()
                .with("path", path.display().to_string())
                .with("line", line)
                .with("actual_line", actual);
            Ok((result, Next::Resume(Resume::Continue)))
        }
        other => Err(invalid(format!("{other} is not available while paused."))),
    }
}

impl DebugController for DebugBridge {
    fn pause(&mut self, it: &mut Interp, reason: PauseReason) -> Resume {
        // SAFETY: see the field; the eval that owns the host is on the stack.
        let host = unsafe { &mut *self.host };
        loop {
            host.paused(paused_fields(it, reason));
            loop {
                match host.pause_wait() {
                    PauseRequest::Unsupported => return Resume::Continue,
                    PauseRequest::Interrupted => return Resume::Stop,
                    PauseRequest::Request { id, op, body } => {
                        match paused_request(it, &op, &body) {
                            Err(error) => host.pause_reply(id, Err(error)),
                            Ok((result, next)) => {
                                host.pause_reply(id, Ok(result));
                                match next {
                                    Next::Stay => {}
                                    Next::Refresh => break,
                                    Next::Resume(r) => return r,
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
