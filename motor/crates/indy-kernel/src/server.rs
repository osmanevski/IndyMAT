//! The JSON-lines protocol of `docs/Kernel-Protokolu.md` for any [`Engine`].

use crate::engine::{
    Engine, EngineError, EvalError, EvalMode, EvalRequest, FigureEvent, Host, Interrupted,
    PageRequest, PauseRequest, Stream, VariableSummary,
};
use crate::json::Json;
use crate::mat_ops;
use std::io::{self, BufRead, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::time::Duration;

pub const PROTOCOL: u64 = 1;
pub const MAX_LINE: usize = 4 * 1024 * 1024;
pub const MAX_FILE: u64 = 2_000_000;

/// Operations every engine answers through this server.
const OPS: &[&str] = &[
    "info",
    "eval",
    "input_reply",
    "workspace",
    "inspect",
    "rename",
    "clear",
    "cd",
    "pwd",
    "reset",
    "close",
];

/// Operations of the real engine only (a stub answers `unsupported`).
const ENGINE_OPS: &[&str] = &["save", "load"];

/// Protocol operations the UI uses that no engine implements yet (the
/// real engine implements those in [`ENGINE_OPS`]).
const UNSUPPORTED: &[(&str, &str, &str)] = &[
    (
        "debug",
        "debugger",
        "The debugger is not supported in the Rust engine yet.",
    ),
    (
        "breakpoint",
        "breakpoints",
        "Breakpoints are not supported in the Rust engine yet.",
    ),
    (
        "run_to_cursor",
        "run_to_cursor",
        "Run to Cursor is not supported in the Rust engine yet.",
    ),
    (
        "package",
        "packages",
        "Packages are not supported in the Rust engine yet.",
    ),
    (
        "profile",
        "profiler",
        "The profiler is not supported in the Rust engine yet.",
    ),
    (
        "publish",
        "publish",
        "Publishing is not supported in the Rust engine yet.",
    ),
    (
        "save",
        "mat_files",
        "Saving MAT files is not supported in the Rust engine yet.",
    ),
    (
        "load",
        "mat_files",
        "Loading MAT files is not supported in the Rust engine yet.",
    ),
    (
        "assign_scalar",
        "workspace_edit",
        "Editing Workspace values is not supported in the Rust engine yet.",
    ),
    (
        "variable_write",
        "variable_editor_write",
        "Editing variables in the Variables editor is not supported in the Rust engine yet.",
    ),
    (
        "figure_rotate",
        "figure_rotate",
        "Rotating figures is not supported in the Rust engine yet.",
    ),
];

/// The debugger's operations; [`Engine::debugger`] says whether they are real.
const DEBUG_OPS: &[&str] = &["debug", "breakpoint", "run_to_cursor"];

enum Incoming {
    Line(Result<String, String>),
    Eof,
}

/// Read bounded lines on a thread so requests are seen while an eval runs.
fn spawn_reader(mut input: impl BufRead + Send + 'static) -> Receiver<Incoming> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        loop {
            let mut buf = Vec::new();
            let read = (&mut input)
                .take(MAX_LINE as u64 + 1)
                .read_until(b'\n', &mut buf);
            match read {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            let complete = buf.last() == Some(&b'\n');
            let line = if !complete && buf.len() > MAX_LINE {
                // Discard the rest of the oversized line.
                let mut rest = Vec::new();
                if input.read_until(b'\n', &mut rest).is_err() {
                    break;
                }
                Err(format!("request line exceeds {MAX_LINE} bytes"))
            } else {
                if complete {
                    buf.pop();
                }
                if buf.last() == Some(&b'\r') {
                    buf.pop();
                }
                String::from_utf8(buf).map_err(|_| "request line is not UTF-8".to_string())
            };
            if tx.send(Incoming::Line(line)).is_err() {
                return;
            }
        }
        let _ = tx.send(Incoming::Eof);
    });
    rx
}

struct Request {
    id: u64,
    op: String,
    body: Json,
}

/// `Err` carries the response to send (with the id when it was readable).
fn parse_request(line: &str) -> Result<Request, Json> {
    let body = Json::parse(line).map_err(|e| failure(Json::Null, "bad_request", &e, None))?;
    let id = match body.get("id").and_then(Json::as_u64) {
        Some(id) if id >= 1 => id,
        _ => {
            return Err(failure(
                body.get("id").cloned().unwrap_or(Json::Null),
                "bad_request",
                "The request needs an integer id of at least 1.",
                None,
            ));
        }
    };
    let op = match body.get("op").and_then(Json::as_str) {
        Some(op) => op.to_string(),
        None => {
            return Err(failure(
                id.into(),
                "bad_request",
                "The request needs an op.",
                None,
            ));
        }
    };
    Ok(Request { id, op, body })
}

fn success(id: u64, result: Json) -> Json {
    Json::object()
        .with("type", "response")
        .with("id", id)
        .with("ok", true)
        .with("result", result)
}

fn failure(id: Json, code: &str, message: &str, feature: Option<&str>) -> Json {
    let mut error = Json::object().with("code", code).with("message", message);
    if let Some(feature) = feature {
        error = error.with("feature", feature);
    }
    Json::object()
        .with("type", "response")
        .with("id", id)
        .with("ok", false)
        .with("error", error)
}

fn engine_failure(id: u64, error: &EngineError) -> Json {
    failure(
        id.into(),
        error.code.name(),
        &error.message,
        error.feature.as_deref(),
    )
}

fn bad(id: u64, message: &str) -> Json {
    failure(id.into(), "bad_request", message, None)
}

fn event(id: u64, kind: &str) -> Json {
    Json::object()
        .with("type", "event")
        .with("id", id)
        .with("event", kind)
}

fn send(out: &mut dyn Write, message: &Json) -> io::Result<()> {
    let mut line = message.to_string_compact();
    line.push('\n');
    out.write_all(line.as_bytes())?;
    out.flush()
}

fn path_text(path: &std::path::Path) -> Json {
    Json::from(path.display().to_string())
}

fn info_json(engine: &dyn Engine) -> Json {
    let info = engine.info();
    Json::object()
        .with("protocol", PROTOCOL)
        .with(
            "engine",
            Json::object()
                .with("name", info.name)
                .with("version", info.version)
                .with("stub", info.stub)
                .with("description", info.description),
        )
        .with(
            "ops",
            Json::Array(
                OPS.iter()
                    .chain(ENGINE_OPS.iter().filter(|_| !info.stub))
                    .chain(if engine.debugger() { DEBUG_OPS } else { &[] })
                    .map(|&op| Json::from(op))
                    .collect(),
            ),
        )
        .with(
            "unsupported",
            unsupported_features(info.stub, engine.debugger()),
        )
}

/// True when this engine answers `op` as `unsupported`.
fn refused(op: &str, stub: bool) -> bool {
    stub || !ENGINE_OPS.contains(&op)
}

/// UI feature names of the operations this engine does not implement
/// (optional `info` field, so a client can hide them instead of offering
/// them).
fn unsupported_features(stub: bool, debugger: bool) -> Json {
    let mut features: Vec<&str> = Vec::new();
    for (op, feature, _) in UNSUPPORTED {
        if debugger && DEBUG_OPS.contains(op) {
            continue;
        }
        if refused(op, stub) && !features.contains(feature) {
            features.push(feature);
        }
    }
    Json::Array(features.into_iter().map(Json::from).collect())
}

fn page_param(body: &Json, key: &str, default: u64, max: u64) -> Result<u64, String> {
    match body.get(key) {
        None => Ok(default),
        Some(v) => match v.as_u64() {
            Some(n) if (1..=max).contains(&n) => Ok(n),
            _ => Err(format!("{key} must be an integer from 1 to {max}.")),
        },
    }
}

/// The page parameters of `inspect`.
pub(crate) fn page_request(body: &Json) -> Result<PageRequest, String> {
    Ok(PageRequest {
        row: page_param(body, "row", 1, 9_007_199_254_740_991)?,
        column: page_param(body, "column", 1, 9_007_199_254_740_991)?,
        rows: page_param(body, "rows", 100, 100)?,
        columns: page_param(body, "columns", 30, 30)?,
    })
}

/// The rows of a `workspace` result.
pub(crate) fn variable_rows(variables: Vec<VariableSummary>) -> Json {
    Json::Array(
        variables
            .into_iter()
            .map(|v| {
                Json::object()
                    .with("name", v.name)
                    .with("class", v.class)
                    .with(
                        "size",
                        Json::Array(v.size.into_iter().map(Json::from).collect()),
                    )
                    .with("bytes", v.bytes)
                    .with("complex", v.complex)
                    .with("global", v.global)
                    .with("preview", v.preview)
            })
            .collect(),
    )
}

fn names_param(body: &Json) -> Result<Option<Vec<String>>, String> {
    match body.get("names") {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Array(items)) if items.len() <= 500 => items
            .iter()
            .map(|v| v.as_str().map(str::to_string).ok_or_else(String::new))
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
            .map_err(|_| "names must be strings.".to_string()),
        Some(_) => Err("names must be an array of at most 500 strings.".into()),
    }
}

/// [`Host`] for one running eval: forwards events and answers requests that
/// arrive meanwhile.
struct EvalHost<'a> {
    id: u64,
    out: &'a mut dyn Write,
    rx: &'a Receiver<Incoming>,
    interrupt: &'a AtomicBool,
    info: Json,
    stub: bool,
    /// The client asked to close or went away: stop and send nothing more.
    gone: bool,
}

impl EvalHost<'_> {
    fn emit(&mut self, message: &Json) {
        if !self.gone && send(self.out, message).is_err() {
            self.gone = true;
        }
    }

    /// Handle one request during the eval. Returns the input reply text when
    /// `awaiting_input` and the request is a valid `input_reply`.
    fn side_request(&mut self, incoming: Incoming, awaiting_input: bool) -> Option<String> {
        let line = match incoming {
            Incoming::Eof => {
                self.gone = true;
                return None;
            }
            Incoming::Line(Err(message)) => {
                self.emit(&failure(Json::Null, "bad_request", &message, None));
                return None;
            }
            Incoming::Line(Ok(line)) if line.trim().is_empty() => return None,
            Incoming::Line(Ok(line)) => line,
        };
        let request = match parse_request(&line) {
            Ok(request) => request,
            Err(response) => {
                self.emit(&response);
                return None;
            }
        };
        let id = request.id;
        match request.op.as_str() {
            "input_reply" if awaiting_input => {
                match request.body.get("text").and_then(Json::as_str) {
                    Some(text) if !text.contains(['\n', '\r']) => {
                        let text = text.to_string();
                        self.emit(&success(id, Json::object()));
                        return Some(text);
                    }
                    _ => self.emit(&bad(id, "text must be one line of text.")),
                }
            }
            "input_reply" => {
                self.emit(&failure(id.into(), "invalid", "No input is pending.", None))
            }
            "info" => {
                let info = self.info.clone();
                self.emit(&success(id, info));
            }
            "close" => {
                self.emit(&success(id, Json::object()));
                self.gone = true;
            }
            _ => self.emit(&failure(
                id.into(),
                "busy",
                "Code is running; wait for it to finish or interrupt it.",
                None,
            )),
        }
        None
    }
}

impl Host for EvalHost<'_> {
    fn output(&mut self, stream: Stream, text: &str) {
        let mut message = event(self.id, "output")
            .with("stream", stream.name())
            .with("text", text);
        if self.stub {
            message = message.with("stub", true);
        }
        self.emit(&message);
    }

    fn clc(&mut self) {
        let message = event(self.id, "clc");
        self.emit(&message);
    }

    fn figure(&mut self, figure: FigureEvent) {
        let message = event(self.id, "figure")
            .with("number", figure.number)
            .with("name", figure.name)
            .with("figure", figure.figure.unwrap_or(Json::Null))
            .with("png_base64", figure.png_base64)
            .with("reason", figure.reason);
        self.emit(&message);
    }

    fn input(&mut self, prompt: &str) -> Result<String, Interrupted> {
        let message = event(self.id, "input_request").with("prompt", prompt);
        self.emit(&message);
        loop {
            if self.gone || self.interrupt.load(Ordering::SeqCst) {
                return Err(Interrupted);
            }
            match self.rx.recv_timeout(Duration::from_millis(20)) {
                Ok(incoming) => {
                    if let Some(text) = self.side_request(incoming, true) {
                        return Ok(text);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => self.gone = true,
            }
        }
    }

    fn paused(&mut self, fields: Vec<(&'static str, Json)>) {
        let mut message = event(self.id, "paused");
        for (key, value) in fields {
            message = message.with(key, value);
        }
        self.emit(&message);
    }

    fn pause_wait(&mut self) -> PauseRequest {
        loop {
            if self.gone || self.interrupt.load(Ordering::SeqCst) {
                return PauseRequest::Interrupted;
            }
            match self.rx.recv_timeout(Duration::from_millis(20)) {
                Ok(Incoming::Line(Ok(line))) => {
                    if let Ok(request) = parse_request(&line)
                        && matches!(
                            request.op.as_str(),
                            "debug" | "breakpoint" | "run_to_cursor" | "workspace" | "inspect"
                        )
                    {
                        return PauseRequest::Request {
                            id: request.id,
                            op: request.op,
                            body: request.body,
                        };
                    }
                    self.side_request(Incoming::Line(Ok(line)), false);
                }
                Ok(incoming) => {
                    self.side_request(incoming, false);
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => self.gone = true,
            }
        }
    }

    fn pause_reply(&mut self, id: u64, result: Result<Json, EngineError>) {
        let message = match result {
            Ok(result) => success(id, result),
            Err(error) => engine_failure(id, &error),
        };
        self.emit(&message);
    }

    fn interrupted(&mut self) -> bool {
        loop {
            match self.rx.try_recv() {
                Ok(incoming) => {
                    self.side_request(incoming, false);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.gone = true;
                    break;
                }
            }
        }
        self.gone || self.interrupt.load(Ordering::SeqCst)
    }
}

enum Flow {
    Continue,
    Exit,
}

/// Serve the protocol until `close` or EOF. `interrupt` is set by SIGINT.
pub fn serve(
    engine: &mut dyn Engine,
    input: impl BufRead + Send + 'static,
    out: &mut dyn Write,
    interrupt: &AtomicBool,
) -> io::Result<()> {
    let info = engine.info();
    let ready = Json::object()
        .with("type", "ready")
        .with("protocol", PROTOCOL)
        .with(
            "engine",
            Json::object()
                .with("name", info.name)
                .with("version", info.version)
                .with("stub", info.stub)
                .with("description", info.description),
        )
        .with("cwd", path_text(&engine.cwd()));
    send(out, &ready)?;
    let rx = spawn_reader(input);
    loop {
        let line = match rx.recv() {
            Ok(Incoming::Line(Ok(line))) => line,
            Ok(Incoming::Line(Err(message))) => {
                send(out, &failure(Json::Null, "bad_request", &message, None))?;
                continue;
            }
            Ok(Incoming::Eof) | Err(_) => return Ok(()),
        };
        if line.trim().is_empty() {
            continue;
        }
        let request = match parse_request(&line) {
            Ok(request) => request,
            Err(response) => {
                send(out, &response)?;
                continue;
            }
        };
        if let Flow::Exit = handle(engine, request, out, &rx, interrupt)? {
            return Ok(());
        }
    }
}

fn handle(
    engine: &mut dyn Engine,
    request: Request,
    out: &mut dyn Write,
    rx: &Receiver<Incoming>,
    interrupt: &AtomicBool,
) -> io::Result<Flow> {
    let Request { id, op, body } = request;
    let str_param = |key: &str| body.get(key).and_then(Json::as_str).map(str::to_string);
    let response = match op.as_str() {
        "info" => success(id, info_json(engine)),
        "eval" => return eval(engine, id, &body, out, rx, interrupt),
        "input_reply" => failure(id.into(), "invalid", "No input is pending.", None),
        "workspace" => {
            let variables = engine.workspace().into_iter().take(500).collect();
            success(
                id,
                Json::object().with("variables", variable_rows(variables)),
            )
        }
        "inspect" => {
            let page = page_request(&body);
            match (str_param("name"), page) {
                (Some(name), Ok(page)) => match engine.inspect(&name, page) {
                    Ok(result) => success(id, result),
                    Err(error) => engine_failure(id, &error),
                },
                (None, _) => bad(id, "name must be a string."),
                (_, Err(message)) => bad(id, &message),
            }
        }
        "rename" => match (str_param("old_name"), str_param("new_name")) {
            (Some(old), Some(new)) => match engine.rename(&old, &new) {
                Ok(()) => success(
                    id,
                    Json::object().with("old_name", old).with("new_name", new),
                ),
                Err(error) => engine_failure(id, &error),
            },
            _ => bad(id, "old_name and new_name must be strings."),
        },
        "clear" => match names_param(&body) {
            Ok(names) => match engine.clear(names.as_deref()) {
                Ok(cleared) => success(
                    id,
                    Json::object().with(
                        "names",
                        Json::Array(cleared.into_iter().map(Json::from).collect()),
                    ),
                ),
                Err(error) => engine_failure(id, &error),
            },
            Err(message) => bad(id, &message),
        },
        "cd" => match str_param("path") {
            Some(path) => match engine.cd(&PathBuf::from(path)) {
                Ok(cwd) => success(id, Json::object().with("cwd", path_text(&cwd))),
                Err(error) => engine_failure(id, &error),
            },
            None => bad(id, "path must be a string."),
        },
        "pwd" => success(id, Json::object().with("cwd", path_text(&engine.cwd()))),
        "reset" => {
            engine.reset();
            success(id, Json::object().with("cwd", path_text(&engine.cwd())))
        }
        "close" => {
            send(out, &success(id, Json::object()))?;
            return Ok(Flow::Exit);
        }
        "save" | "load" if !engine.info().stub => {
            let result = if op == "save" {
                mat_ops::save(engine, &body, interrupt)
            } else {
                mat_ops::load(engine, &body, interrupt)
            };
            match result {
                Ok(result) => success(id, result),
                Err(error) => engine_failure(id, &error),
            }
        }
        "breakpoint" if engine.debugger() => match engine.breakpoint(&body) {
            Ok(result) => success(id, result),
            Err(error) => engine_failure(id, &error),
        },
        "debug" | "run_to_cursor" if engine.debugger() => {
            failure(id.into(), "invalid", "The debugger is not paused.", None)
        }
        other => match UNSUPPORTED.iter().find(|(name, _, _)| *name == other) {
            Some((_, feature, message)) => {
                failure(id.into(), "unsupported", message, Some(feature))
            }
            None => failure(
                id.into(),
                "unknown_op",
                &format!("Unknown operation: {other}"),
                None,
            ),
        },
    };
    send(out, &response)?;
    Ok(Flow::Continue)
}

fn eval(
    engine: &mut dyn Engine,
    id: u64,
    body: &Json,
    out: &mut dyn Write,
    rx: &Receiver<Incoming>,
    interrupt: &AtomicBool,
) -> io::Result<Flow> {
    let mode = match body.get("mode").map(Json::as_str) {
        None => Some(EvalMode::Code),
        Some(Some(text)) => EvalMode::parse(text),
        Some(None) => None,
    };
    let Some(mode) = mode else {
        send(out, &bad(id, "mode must be code, section or file."))?;
        return Ok(Flow::Continue);
    };
    let path = match body.get("path") {
        None | Some(Json::Null) => None,
        Some(Json::String(p)) if !p.is_empty() => Some(PathBuf::from(p)),
        Some(_) => {
            send(out, &bad(id, "path must be a non-empty string."))?;
            return Ok(Flow::Continue);
        }
    };
    let mut code = match body.get("code") {
        None | Some(Json::Null) => String::new(),
        Some(Json::String(code)) => code.clone(),
        Some(_) => {
            send(out, &bad(id, "code must be a string."))?;
            return Ok(Flow::Continue);
        }
    };
    if mode == EvalMode::File {
        let Some(file) = &path else {
            send(out, &bad(id, "file mode needs a path."))?;
            return Ok(Flow::Continue);
        };
        if code.is_empty() {
            match read_source(file) {
                Ok(text) => code = text,
                Err(response) => {
                    send(out, &response(id))?;
                    return Ok(Flow::Continue);
                }
            }
        }
    }
    // Clear before the acknowledgement: a SIGINT sent after the client has
    // read the ack always reaches this eval.
    interrupt.store(false, Ordering::SeqCst);
    send(out, &success(id, Json::object()))?;
    let info = engine.info();
    let mut host = EvalHost {
        id,
        out,
        rx,
        interrupt,
        info: info_json(engine),
        stub: info.stub,
        gone: false,
    };
    let request = EvalRequest { code, mode, path };
    let result = engine.eval(&request, &mut host);
    if host.gone {
        return Ok(Flow::Exit);
    }
    // A signal that arrived after the engine's last check belongs to this
    // eval only; it must never stop the next one.
    interrupt.store(false, Ordering::SeqCst);
    let cwd = path_text(&engine.cwd());
    let terminal = match result {
        Ok(()) => event(id, "done").with("cwd", cwd),
        Err(EvalError {
            message,
            interrupted,
            quit,
            location,
        }) => {
            let mut message = event(id, "error")
                .with("message", message)
                .with("interrupted", interrupted)
                .with("cwd", cwd);
            if quit {
                message = message.with("quit", true);
            }
            if let Some(location) = location {
                message = message.with(
                    "location",
                    Json::object()
                        .with("file", path_text(&location.file))
                        .with("line", location.line),
                );
            }
            message
        }
    };
    send(host.out, &terminal)?;
    Ok(Flow::Continue)
}

type Refusal = Box<dyn Fn(u64) -> Json>;

fn read_source(file: &std::path::Path) -> Result<String, Refusal> {
    let shown = file.display().to_string();
    let mut handle = std::fs::File::open(file).map_err(|_| -> Refusal {
        let message = format!("File not found: {shown}");
        Box::new(move |id| failure(id.into(), "not_found", &message, None))
    })?;
    let mut bytes = Vec::new();
    let read = (&mut handle).take(MAX_FILE + 1).read_to_end(&mut bytes);
    if read.is_err() || bytes.len() as u64 > MAX_FILE {
        let message = format!("The file cannot be read or exceeds {MAX_FILE} bytes: {shown}");
        return Err(Box::new(move |id| {
            failure(id.into(), "invalid", &message, None)
        }));
    }
    let text = String::from_utf8(bytes).map_err(|_| -> Refusal {
        let message = format!("The file is not UTF-8 text: {shown}");
        Box::new(move |id| failure(id.into(), "invalid", &message, None))
    })?;
    Ok(text
        .strip_prefix('\u{feff}')
        .map(str::to_string)
        .unwrap_or(text))
}
