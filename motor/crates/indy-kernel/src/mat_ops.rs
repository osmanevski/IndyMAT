//! The protocol's `save` and `load` operations (MAT files), for engines
//! that are not stubs. Parameters and results: `docs/Kernel-Protokolu.md`
//! ("MAT files"); the shape is the one IndyMAT's Octave bridge gives the
//! Workspace panel (`octave/__mf_workspace__.m`).
//!
//! - Writing and assigning run the engine's own `save`/`load` builtins
//!   through [`Engine::eval`] with a silent host, so the bytes on disk and
//!   the values in the workspace are exactly what the language gives.
//! - Inspection and the checks before a load read the file here with the
//!   same reader (`indy_interp::builtins::matfile`), before any variable is
//!   assigned.

use crate::engine::{
    Engine, EngineError, ErrorCode, EvalMode, EvalRequest, FigureEvent, Host, Interrupted, Stream,
};
use crate::json::Json;
use indy_interp::builtins::matfile;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Largest MAT file `load` accepts (the app's own limit is 200 MB).
pub const MAX_MAT_FILE: u64 = 256 * 1024 * 1024;
/// Most variables a file may list for inspection.
const MAX_VARIABLES: usize = 5000;

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::new(ErrorCode::Invalid, message)
}

/// Collects warnings; everything else an eval could send is dropped.
struct QuietHost<'a> {
    stderr: String,
    interrupt: &'a AtomicBool,
}

impl Host for QuietHost<'_> {
    fn output(&mut self, stream: Stream, text: &str) {
        if stream == Stream::Stderr {
            self.stderr.push_str(text);
        }
    }
    fn clc(&mut self) {}
    fn figure(&mut self, _: FigureEvent) {}
    fn input(&mut self, _: &str) -> Result<String, Interrupted> {
        Err(Interrupted)
    }
    fn interrupted(&mut self) -> bool {
        self.interrupt.load(Ordering::SeqCst)
    }
}

/// Run engine code; its warnings (`warning: ...` lines) on success.
fn run(
    engine: &mut dyn Engine,
    code: String,
    interrupt: &AtomicBool,
) -> Result<Vec<String>, EngineError> {
    interrupt.store(false, Ordering::SeqCst);
    let mut host = QuietHost {
        stderr: String::new(),
        interrupt,
    };
    let request = EvalRequest {
        code,
        mode: EvalMode::Code,
        path: None,
    };
    let result = engine.eval(&request, &mut host);
    interrupt.store(false, Ordering::SeqCst);
    result.map_err(|e| {
        if e.interrupted {
            EngineError::new(ErrorCode::Engine, "Operation interrupted.")
        } else {
            EngineError::new(ErrorCode::Engine, e.message)
        }
    })?;
    Ok(host
        .stderr
        .lines()
        .filter_map(|l| l.strip_prefix("warning: "))
        .map(str::to_string)
        .collect())
}

fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

fn valid_name(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && text.len() <= 63
}

/// The `path` parameter, absolute (relative paths follow the engine folder).
fn path_param(engine: &dyn Engine, body: &Json) -> Result<PathBuf, EngineError> {
    let text = body
        .get("path")
        .and_then(Json::as_str)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| invalid("path must be a non-empty string."))?;
    if text.contains(['\n', '\r', '\0']) {
        return Err(invalid("Invalid MAT path."));
    }
    let p = PathBuf::from(text);
    Ok(if p.is_absolute() {
        p
    } else {
        engine.cwd().join(p)
    })
}

fn strings_param(body: &Json, key: &str) -> Result<Option<Vec<String>>, EngineError> {
    match body.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Array(items)) if items.len() <= MAX_VARIABLES => items
            .iter()
            .map(|v| v.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()
            .map(Some)
            .ok_or_else(|| invalid(format!("{key} must be an array of strings."))),
        Some(_) => Err(invalid(format!("{key} must be an array of strings."))),
    }
}

fn names_json(names: &[String]) -> Json {
    Json::Array(names.iter().map(|n| Json::from(n.as_str())).collect())
}

fn base_names(engine: &dyn Engine) -> Vec<String> {
    engine.workspace().into_iter().map(|v| v.name).collect()
}

fn read_mat(path: &Path) -> Result<matfile::MatData, EngineError> {
    let shown = path.display().to_string();
    let file = std::fs::File::open(path).map_err(|_| {
        EngineError::new(ErrorCode::NotFound, format!("MAT file not found: {shown}"))
    })?;
    let mut bytes = Vec::new();
    file.take(MAX_MAT_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| EngineError::new(ErrorCode::Engine, format!("Cannot read {shown}: {e}")))?;
    if bytes.len() as u64 > MAX_MAT_FILE {
        return Err(invalid(format!(
            "The MAT file exceeds {} MB: {shown}",
            MAX_MAT_FILE >> 20
        )));
    }
    matfile::parse(&bytes, &shown).map_err(|e| invalid(e.message))
}

fn size_text(dims: &[usize]) -> String {
    dims.iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join("x")
}

/// `save`: `path`, `names` (absent or null for all variables). The file is
/// MAT version 7. Result `{path, names, warnings}`; `names` are the
/// variables the file holds afterwards (skipped values are in `warnings`).
pub fn save(
    engine: &mut dyn Engine,
    body: &Json,
    interrupt: &AtomicBool,
) -> Result<Json, EngineError> {
    let path = path_param(engine, body)?;
    let names = strings_param(body, "names")?;
    let available = base_names(engine);
    let mut code = format!(
        "builtin('save', {}, '-v7'",
        quote(&path.display().to_string())
    );
    match &names {
        None => {
            if available.is_empty() {
                return Err(invalid("No variables to save."));
            }
        }
        Some(names) => {
            if names.is_empty() {
                return Err(invalid("No variables selected for saving."));
            }
            if let Some(bad) = names.iter().find(|n| !valid_name(n)) {
                return Err(invalid(format!("Invalid variable name: {bad}")));
            }
            // The workspace list is capped at 500 rows; beyond that the
            // builtin's own check reports a missing name.
            if available.len() < 500 && names.iter().any(|n| !available.contains(n)) {
                return Err(invalid("One of the selected variables no longer exists."));
            }
            for n in names {
                code.push_str(", ");
                code.push_str(&quote(n));
            }
        }
    }
    code.push_str(");");
    let warnings = run(engine, code, interrupt)?;
    let saved: Vec<String> = read_mat(&path)?.vars.into_iter().map(|v| v.name).collect();
    Ok(Json::object()
        .with("path", path.display().to_string())
        .with("names", names_json(&saved))
        .with("warnings", names_json(&warnings)))
}

/// `load` with `inspect: true`: `{path, variables:[{name,size,class,bytes}],
/// replacements, warnings}` without changing the workspace. Otherwise the
/// variables are assigned: optional `inventory` (the inspected
/// `variables`) and `replacements` must still match, else nothing is
/// assigned. Result `{path, names, replacements, warnings}`.
pub fn load(
    engine: &mut dyn Engine,
    body: &Json,
    interrupt: &AtomicBool,
) -> Result<Json, EngineError> {
    let path = path_param(engine, body)?;
    let inspect = match body.get("inspect") {
        None | Some(Json::Null) => false,
        Some(Json::Bool(b)) => *b,
        Some(_) => return Err(invalid("inspect must be true or false.")),
    };
    let data = read_mat(&path)?;
    if data.vars.len() > MAX_VARIABLES {
        return Err(invalid(format!(
            "The MAT-file contains more than {MAX_VARIABLES} variables."
        )));
    }
    let infos = matfile::file_infos(&data);
    let base = base_names(engine);
    let loadable: Vec<&String> = data
        .vars
        .iter()
        .filter(|v| v.value.is_some() && valid_name(&v.name))
        .map(|v| &v.name)
        .collect();
    let replacements: Vec<String> = base
        .iter()
        .filter(|n| loadable.contains(n))
        .cloned()
        .collect();
    let shown = path.display().to_string();
    if inspect {
        let variables = infos
            .iter()
            .map(|i| {
                Json::object()
                    .with("name", i.name.as_str())
                    .with("size", size_text(&i.dims))
                    .with("class", i.class.as_str())
                    .with("bytes", i.bytes)
            })
            .collect();
        return Ok(Json::object()
            .with("path", shown)
            .with("variables", Json::Array(variables))
            .with("replacements", names_json(&replacements))
            .with("warnings", names_json(&data.warnings)));
    }
    if let Some(items) = body.get("inventory").filter(|v| !v.is_null()) {
        let items = items
            .as_array()
            .ok_or_else(|| invalid("inventory must be an array."))?;
        let changed = || invalid("The MAT-file variable list has changed since inspection.");
        let mut expected: Vec<&str> = Vec::with_capacity(items.len());
        for item in items {
            let name = item
                .get("name")
                .and_then(Json::as_str)
                .ok_or_else(changed)?;
            let info = infos.iter().find(|i| i.name == name).ok_or_else(changed)?;
            if item.get("class").and_then(Json::as_str) != Some(info.class.as_str())
                || item.get("size").and_then(Json::as_str) != Some(size_text(&info.dims).as_str())
            {
                return Err(invalid(
                    "A MAT-file variable's class or size has changed since inspection.",
                ));
            }
            expected.push(name);
        }
        let mut have: Vec<&str> = infos.iter().map(|i| i.name.as_str()).collect();
        expected.sort_unstable();
        have.sort_unstable();
        if expected != have {
            return Err(changed());
        }
    }
    if let Some(mut confirmed) = strings_param(body, "replacements")? {
        let mut now = replacements.clone();
        confirmed.sort();
        now.sort();
        if confirmed != now {
            return Err(invalid(
                "The variables to replace have changed since confirmation; inspect the file again.",
            ));
        }
    }
    let warnings = if loadable.is_empty() {
        data.warnings.clone()
    } else {
        run(
            engine,
            format!("builtin('load', {}, '-mat');", quote(&shown)),
            interrupt,
        )?
    };
    let names: Vec<String> = loadable.into_iter().cloned().collect();
    Ok(Json::object()
        .with("path", shown)
        .with("names", names_json(&names))
        .with("replacements", names_json(&replacements))
        .with("warnings", names_json(&warnings)))
}
