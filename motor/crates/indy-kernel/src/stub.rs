//! `StubEngine`: a deliberately tiny, honest engine for wiring the app to the
//! kernel protocol before the phase 2 interpreter exists.
//!
//! Supported, nothing more: numeric scalar literals (`3`, `-2.5`, `1e3`,
//! `Inf`, `NaN`, `pi`), single-quoted char rows, variable names, `x = expr`,
//! bare expressions (`ans`), `disp(expr)`, `input('prompt')` and
//! `input('prompt', 's')`, `pause(seconds)`, `cd('folder')` / `cd folder`,
//! `pwd`, `clear` / `clear name ...`, `clc`, comments and `...` continuation.
//! Everything else is an error naming the stub. Display text approximates
//! Octave's `format short`; it is not the real interpreter's formatting.
//! Every eval that prints anything first prints [`STUB_NOTICE`].

use crate::engine::{
    Engine, EngineError, EngineInfo, ErrorCode, EvalError, EvalMode, EvalRequest, Host,
    PageRequest, SourceLocation, Stream, VariableSummary,
};
use crate::json::Json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const STUB_NOTICE: &str = "[IndyMAT Rust stub engine: not the real interpreter]\n";

const SUBSET: &str =
    "numeric and character literals, assignment, disp, input, pause, cd, pwd, clear, clc";

#[derive(Clone, Debug, PartialEq)]
enum Value {
    Double(f64),
    /// 0x0 double, `[]` (only produced by an empty `input` reply).
    Empty,
    Char(String),
}

pub struct StubEngine {
    vars: BTreeMap<String, Value>,
    start: PathBuf,
    cwd: PathBuf,
}

impl StubEngine {
    pub fn new(start: PathBuf) -> StubEngine {
        StubEngine {
            vars: BTreeMap::new(),
            cwd: start.clone(),
            start,
        }
    }
}

struct Statement {
    text: String,
    line: u64,
    display: bool,
}

/// Split into statements at `;`, `,` and newlines outside strings and
/// brackets; drop comments; join `...` continuations.
fn statements(code: &str) -> Vec<Statement> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut start_line = 1u64;
    let mut line = 1u64;
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut prev_significant: Option<char> = None;
    let chars: Vec<char> = code.chars().collect();
    let mut i = 0;
    let mut push = |current: &mut String, start_line: &mut u64, line: u64, display: bool| {
        let text = current.trim().to_string();
        if !text.is_empty() {
            out.push(Statement {
                text,
                line: *start_line,
                display,
            });
        }
        current.clear();
        *start_line = line;
    };
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = quote {
            current.push(c);
            if c == q {
                if chars.get(i + 1) == Some(&q) {
                    current.push(q);
                    i += 1;
                } else {
                    quote = None;
                }
            } else if c == '\n' {
                // Unterminated string: keep it for the error, end the line.
                quote = None;
                line += 1;
            }
            i += 1;
            continue;
        }
        match c {
            '\'' | '"' => {
                let transpose = matches!(prev_significant, Some(p) if p.is_alphanumeric() || matches!(p, '_' | ')' | ']' | '}' | '\'' | '.'))
                    && chars
                        .get(i.wrapping_sub(1))
                        .is_some_and(|p| !p.is_whitespace());
                if !transpose || c == '"' {
                    quote = Some(c);
                }
                current.push(c);
            }
            '%' | '#' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            '.' if chars.get(i + 1) == Some(&'.') && chars.get(i + 2) == Some(&'.') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                i += 1;
                line += 1;
                current.push(' ');
                continue;
            }
            '(' | '[' | '{' => {
                depth += 1;
                current.push(c);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                current.push(c);
            }
            ';' | ',' if depth <= 0 => push(&mut current, &mut start_line, line, c == ','),
            '\n' => {
                line += 1;
                if depth <= 0 {
                    push(&mut current, &mut start_line, line, true);
                    depth = 0;
                } else {
                    current.push(' ');
                }
            }
            _ => current.push(c),
        }
        if !c.is_whitespace() {
            prev_significant = Some(c);
        }
        i += 1;
    }
    push(&mut current, &mut start_line, line, true);
    out
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && text.len() <= 63
}

const KEYWORDS: &[&str] = &[
    "break",
    "case",
    "catch",
    "classdef",
    "continue",
    "do",
    "else",
    "elseif",
    "end",
    "for",
    "function",
    "global",
    "if",
    "otherwise",
    "parfor",
    "persistent",
    "return",
    "switch",
    "try",
    "until",
    "unwind_protect",
    "while",
];

fn valid_variable_name(text: &str) -> bool {
    is_identifier(text) && !KEYWORDS.contains(&text)
}

/// `name(args)` with the whole statement being one call.
fn call<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(name)?.trim_start();
    let inner = rest.strip_prefix('(')?.strip_suffix(')')?;
    Some(inner)
}

/// Split call arguments at top-level commas.
fn arguments(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote = false;
    let mut depth = 0;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if quote {
            current.push(c);
            if c == '\'' {
                if chars.get(i + 1) == Some(&'\'') {
                    current.push('\'');
                    i += 1;
                } else {
                    quote = false;
                }
            }
        } else {
            match c {
                '\'' => {
                    quote = true;
                    current.push(c);
                }
                '(' | '[' => {
                    depth += 1;
                    current.push(c);
                }
                ')' | ']' => {
                    depth -= 1;
                    current.push(c);
                }
                ',' if depth == 0 => args.push(std::mem::take(&mut current).trim().to_string()),
                _ => current.push(c),
            }
        }
        i += 1;
    }
    if !current.trim().is_empty() || !args.is_empty() {
        args.push(current.trim().to_string());
    }
    args
}

fn char_literal(text: &str) -> Option<String> {
    let inner = text.strip_prefix('\'')?.strip_suffix('\'')?;
    let mut out = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\'' {
            if chars.next() != Some('\'') {
                return None;
            }
            out.push('\'');
        } else {
            out.push(c);
        }
    }
    Some(out)
}

fn number_literal(text: &str) -> Option<f64> {
    let (sign, body) = match text.as_bytes().first() {
        Some(b'-') => (-1.0, text[1..].trim_start()),
        Some(b'+') => (1.0, text[1..].trim_start()),
        _ => (1.0, text),
    };
    let magnitude = match body {
        "Inf" | "inf" => f64::INFINITY,
        "NaN" | "nan" => f64::NAN,
        "pi" => std::f64::consts::PI,
        _ => {
            let bytes = body.as_bytes();
            let mut i = 0;
            let mut digits = 0;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
                digits += 1;
            }
            if i < bytes.len() && bytes[i] == b'.' {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                    digits += 1;
                }
            }
            if digits == 0 {
                return None;
            }
            if i < bytes.len() && matches!(bytes[i], b'e' | b'E') {
                i += 1;
                if i < bytes.len() && matches!(bytes[i], b'+' | b'-') {
                    i += 1;
                }
                let exp_start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if i == exp_start {
                    return None;
                }
            }
            if i != bytes.len() {
                return None;
            }
            body.parse::<f64>().ok()?
        }
    };
    Some(sign * magnitude)
}

fn exponent_form(x: f64, decimals: usize) -> String {
    let text = format!("{x:.decimals$e}");
    let (mantissa, exp) = text.split_once('e').unwrap_or((&text, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    format!(
        "{mantissa}e{}{:02}",
        if exp < 0 { '-' } else { '+' },
        exp.abs()
    )
}

/// Approximation of Octave `format short` for a real scalar.
fn display_number(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Inf" } else { "-Inf" }.into();
    }
    if x.fract() == 0.0 && x.abs() < 1e15 {
        return format!("{}", x as i64);
    }
    let a = x.abs();
    if (1.0..1e4).contains(&a) {
        let digits = a.log10().floor() as usize + 1;
        format!("{x:.prec$}", prec = 5 - digits)
    } else if (1e-3..1.0).contains(&a) {
        format!("{x:.4}")
    } else {
        exponent_form(x, 4)
    }
}

/// `mat2str(x, 5)`-like short preview (5 significant digits).
fn preview_number(x: f64) -> String {
    if !x.is_finite() || (x.fract() == 0.0 && x.abs() < 1e15) {
        return display_number(x);
    }
    let text = format!("{x:.4e}");
    let (_, exp) = text.split_once('e').unwrap_or(("", "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    if (-5..5).contains(&exp) {
        let fixed = format!("{x:.prec$}", prec = (4 - exp) as usize);
        let trimmed = fixed.trim_end_matches('0').trim_end_matches('.');
        trimmed.to_string()
    } else {
        let mantissa = format!("{:.4}", x / 10f64.powi(exp));
        let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
        format!(
            "{mantissa}e{}{:02}",
            if exp < 0 { '-' } else { '+' },
            exp.abs()
        )
    }
}

fn shape(value: &Value) -> Vec<u64> {
    match value {
        Value::Double(_) => vec![1, 1],
        Value::Empty => vec![0, 0],
        Value::Char(s) if s.is_empty() => vec![0, 0],
        Value::Char(s) => vec![1, s.chars().count() as u64],
    }
}

fn display(name: &str, value: &Value) -> String {
    match value {
        Value::Double(x) => format!("{name} = {}\n", display_number(*x)),
        Value::Empty => format!("{name} = [](0x0)\n"),
        Value::Char(s) => format!("{name} = {s}\n"),
    }
}

struct Run<'h> {
    host: &'h mut dyn Host,
    noticed: bool,
}

impl Run<'_> {
    fn print(&mut self, text: &str) {
        if !self.noticed {
            self.noticed = true;
            self.host.output(Stream::Stdout, STUB_NOTICE);
        }
        self.host.output(Stream::Stdout, text);
    }
}

impl StubEngine {
    fn outside(text: &str) -> EvalError {
        EvalError::new(format!(
            "IndyMAT Rust stub engine: '{text}' is outside the stub subset ({SUBSET})."
        ))
    }

    fn expression(&self, text: &str, run: &mut Run) -> Result<Value, EvalError> {
        let text = text.trim();
        if let Some(s) = char_literal(text) {
            return Ok(Value::Char(s));
        }
        if let Some(value) = self.vars.get(text) {
            return Ok(value.clone());
        }
        if let Some(x) = number_literal(text) {
            return Ok(Value::Double(x));
        }
        if text == "pwd" || call(text, "pwd").is_some_and(|inner| inner.trim().is_empty()) {
            return Ok(Value::Char(self.cwd.display().to_string()));
        }
        if let Some(inner) = call(text, "input") {
            let args = arguments(inner);
            let prompt = match args.first().map(|a| char_literal(a)) {
                Some(Some(p)) => p,
                _ => return Err(Self::outside(text)),
            };
            let as_text = match args.get(1).map(|a| char_literal(a)) {
                None => false,
                Some(Some(flag)) if flag == "s" && args.len() == 2 => true,
                _ => return Err(Self::outside(text)),
            };
            let reply = run.host.input(&prompt)?;
            if as_text {
                return Ok(Value::Char(reply));
            }
            let reply = reply.trim();
            if reply.is_empty() {
                return Ok(Value::Empty);
            }
            return match self.expression(reply, run) {
                Ok(Value::Char(_)) | Err(_) => Err(EvalError::new(format!(
                    "IndyMAT Rust stub engine: input reply '{reply}' is not a number."
                ))),
                Ok(value) => Ok(value),
            };
        }
        if is_identifier(text) {
            return Err(EvalError::new(format!("'{text}' undefined")));
        }
        Err(Self::outside(text))
    }

    fn change_folder(&mut self, target: &str) -> Result<(), EvalError> {
        self.cd(Path::new(target))
            .map(|_| ())
            .map_err(|e| EvalError::new(e.message))
    }

    fn statement(&mut self, stmt: &Statement, run: &mut Run) -> Result<(), EvalError> {
        let text = stmt.text.as_str();
        let (word, rest) = match text.find(|c: char| c.is_whitespace()) {
            Some(i) => (&text[..i], text[i..].trim()),
            None => (text, ""),
        };
        // Command syntax: `clear a b`, `cd folder`, `clc`, `pwd`.
        let command = !rest.starts_with('=') && !rest.starts_with('(');
        match word {
            "clc" if rest.is_empty() => {
                run.host.clc();
                return Ok(());
            }
            "clear" if command => {
                if rest.is_empty() || rest == "all" || rest == "-all" {
                    self.vars.clear();
                } else {
                    for name in rest.split_whitespace() {
                        if !valid_variable_name(name) {
                            return Err(Self::outside(text));
                        }
                        self.vars.remove(name);
                    }
                }
                return Ok(());
            }
            "cd" if command && !rest.is_empty() => {
                let target = char_literal(rest).unwrap_or_else(|| rest.to_string());
                return self.change_folder(&target);
            }
            _ => {}
        }
        if let Some(inner) = call(text, "cd") {
            return match self.expression(inner, run)? {
                Value::Char(target) => self.change_folder(&target),
                _ => Err(EvalError::new(
                    "cd: the folder name must be a character row.",
                )),
            };
        }
        if let Some(inner) = call(text, "disp") {
            let value = self.expression(inner, run)?;
            match value {
                Value::Double(x) => run.print(&format!("{}\n", display_number(x))),
                Value::Char(s) => run.print(&format!("{s}\n")),
                Value::Empty => {}
            }
            return Ok(());
        }
        if let Some(inner) = call(text, "pause") {
            let seconds = match self.expression(inner, run)? {
                Value::Double(x) if x >= 0.0 => x,
                _ => {
                    return Err(EvalError::new(
                        "pause: the time must be a non-negative number.",
                    ));
                }
            };
            let end = Instant::now() + Duration::from_secs_f64(seconds.min(86_400.0));
            while Instant::now() < end {
                if run.host.interrupted() {
                    return Err(EvalError::interrupted());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            return Ok(());
        }
        if let Some((left, right)) = text.split_once('=') {
            let left = left.trim();
            let quoted = left.contains(['\'', '"']);
            if !quoted && !right.starts_with('=') && !left.ends_with(['~', '<', '>', '=']) {
                if !valid_variable_name(left) {
                    return Err(Self::outside(text));
                }
                let value = self.expression(right, run)?;
                if stmt.display {
                    run.print(&display(left, &value));
                }
                self.vars.insert(left.to_string(), value);
                return Ok(());
            }
        }
        let value = self.expression(text, run)?;
        let name = if self.vars.contains_key(text) {
            text
        } else {
            "ans"
        };
        if stmt.display {
            run.print(&display(name, &value));
        }
        if name == "ans" {
            self.vars.insert("ans".into(), value);
        }
        Ok(())
    }
}

impl Engine for StubEngine {
    fn info(&self) -> EngineInfo {
        EngineInfo {
            name: "stub".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            stub: true,
            description: format!(
                "IndyMAT Rust stub engine {} (not the real interpreter)",
                env!("CARGO_PKG_VERSION")
            ),
        }
    }

    fn eval(&mut self, request: &EvalRequest, host: &mut dyn Host) -> Result<(), EvalError> {
        let mut run = Run {
            host,
            noticed: false,
        };
        for stmt in statements(&request.code) {
            if run.host.interrupted() {
                return Err(EvalError::interrupted());
            }
            if let Err(mut error) = self.statement(&stmt, &mut run) {
                if !error.interrupted && request.mode != EvalMode::Code {
                    error.location = request.path.clone().map(|file| SourceLocation {
                        file,
                        line: stmt.line,
                    });
                }
                return Err(error);
            }
        }
        // The stub has no graphics: the complete list of open figures is empty.
        Ok(())
    }

    fn workspace(&self) -> Vec<VariableSummary> {
        self.vars
            .iter()
            .take(500)
            .map(|(name, value)| {
                let size = shape(value);
                let (class, bytes, preview) = match value {
                    Value::Double(x) => ("double", 8, preview_number(*x)),
                    Value::Empty => ("double", 0, "[](0x0)".to_string()),
                    Value::Char(s) => {
                        let n = s.chars().count();
                        let preview = if n <= 12 {
                            s.clone()
                        } else {
                            format!("{}x{} char", size[0], size[1])
                        };
                        ("char", n as u64, preview)
                    }
                };
                VariableSummary {
                    name: name.clone(),
                    class: class.into(),
                    size,
                    bytes,
                    complex: false,
                    global: false,
                    preview,
                }
            })
            .collect()
    }

    fn inspect(&self, name: &str, page: PageRequest) -> Result<Json, EngineError> {
        let value = self.vars.get(name).ok_or_else(|| {
            EngineError::new(ErrorCode::NotFound, format!("Variable not found: {name}"))
        })?;
        let size = shape(value);
        let base = Json::object()
            .with("name", name)
            .with(
                "class",
                if matches!(value, Value::Char(_)) {
                    "char"
                } else {
                    "double"
                },
            )
            .with(
                "size",
                Json::Array(size.iter().map(|&n| Json::from(n)).collect()),
            )
            .with("real", true)
            .with("editable", false);
        Ok(match value {
            Value::Char(s) => base.with("kind", "char-text").with("text", s.as_str()),
            Value::Double(_) | Value::Empty => {
                let (rows, columns) = (size[0], size[1]);
                let visible = page.row <= rows && page.column <= columns;
                let row_count: u64 = if visible { 1 } else { 0 };
                let cells = match value {
                    Value::Double(x) if visible => {
                        let text = if x.is_finite() {
                            format!("{x}")
                        } else {
                            display_number(*x)
                        };
                        vec![Json::Array(vec![Json::from(text)])]
                    }
                    _ => Vec::new(),
                };
                base.with("kind", "matrix")
                    .with("row", page.row)
                    .with("column", page.column)
                    .with("row_count", row_count)
                    .with("column_count", row_count)
                    .with("row_total", rows)
                    .with("column_total", columns)
                    .with("truncated", page.row > 1 || page.column > 1)
                    .with("rows", Json::Array(cells))
            }
        })
    }

    fn cwd(&self) -> PathBuf {
        self.cwd.clone()
    }

    fn cd(&mut self, path: &Path) -> Result<PathBuf, EngineError> {
        let target = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.cwd.join(path)
        };
        let resolved = target.canonicalize().map_err(|_| {
            EngineError::new(
                ErrorCode::NotFound,
                format!("cd: {}: No such file or directory", path.display()),
            )
        })?;
        if !resolved.is_dir() {
            return Err(EngineError::new(
                ErrorCode::NotFound,
                format!("cd: {}: Not a directory", path.display()),
            ));
        }
        self.cwd = resolved.clone();
        Ok(resolved)
    }

    fn reset(&mut self) {
        self.vars.clear();
        self.cwd = self.start.clone();
    }

    fn rename(&mut self, old: &str, new: &str) -> Result<(), EngineError> {
        if !valid_variable_name(new) {
            return Err(EngineError::new(
                ErrorCode::Invalid,
                format!("Invalid variable name: {new}"),
            ));
        }
        if old == new {
            return Ok(());
        }
        if self.vars.contains_key(new) {
            return Err(EngineError::new(
                ErrorCode::Invalid,
                format!("A variable named {new} already exists."),
            ));
        }
        let value = self.vars.remove(old).ok_or_else(|| {
            EngineError::new(ErrorCode::NotFound, format!("Variable not found: {old}"))
        })?;
        self.vars.insert(new.to_string(), value);
        Ok(())
    }

    fn clear(&mut self, names: Option<&[String]>) -> Result<Vec<String>, EngineError> {
        match names {
            None => Ok(std::mem::take(&mut self.vars).into_keys().collect()),
            Some(names) => {
                if let Some(bad) = names.iter().find(|n| !valid_variable_name(n)) {
                    return Err(EngineError::new(
                        ErrorCode::Invalid,
                        format!("Invalid variable name: {bad}"),
                    ));
                }
                Ok(names
                    .iter()
                    .filter(|n| self.vars.remove(n.as_str()).is_some())
                    .cloned()
                    .collect())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{FigureEvent, Interrupted};

    #[derive(Default)]
    struct Recorder {
        out: String,
        inputs: Vec<String>,
        prompts: Vec<String>,
        clc: usize,
        stop_after: Option<usize>,
        checks: usize,
    }

    impl Host for Recorder {
        fn output(&mut self, _stream: Stream, text: &str) {
            self.out.push_str(text);
        }
        fn clc(&mut self) {
            self.clc += 1;
        }
        fn figure(&mut self, _figure: FigureEvent) {}
        fn input(&mut self, prompt: &str) -> Result<String, Interrupted> {
            self.prompts.push(prompt.into());
            if self.inputs.is_empty() {
                Err(Interrupted)
            } else {
                Ok(self.inputs.remove(0))
            }
        }
        fn interrupted(&mut self) -> bool {
            self.checks += 1;
            self.stop_after.is_some_and(|n| self.checks > n)
        }
    }

    fn run(engine: &mut StubEngine, code: &str, host: &mut Recorder) -> Result<(), EvalError> {
        engine.eval(
            &EvalRequest {
                code: code.into(),
                mode: EvalMode::Code,
                path: None,
            },
            host,
        )
    }

    fn engine() -> StubEngine {
        StubEngine::new(std::env::temp_dir().canonicalize().unwrap())
    }

    #[test]
    fn assignment_display_and_workspace() {
        let mut e = engine();
        let mut h = Recorder::default();
        run(
            &mut e,
            "x = 3\ny = 2.5; s = 'it''s', -4\n% comment\nz = pi",
            &mut h,
        )
        .unwrap();
        assert_eq!(
            h.out,
            format!("{STUB_NOTICE}x = 3\ns = it's\nans = -4\nz = 3.1416\n")
        );
        let names: Vec<_> = e.workspace().into_iter().map(|v| v.name).collect();
        assert_eq!(names, ["ans", "s", "x", "y", "z"]);
        let x = &e.workspace()[2];
        assert_eq!(
            (x.class.as_str(), x.size.clone(), x.preview.as_str()),
            ("double", vec![1, 1], "3")
        );
        assert_eq!(e.workspace()[4].preview, "3.1416");
    }

    #[test]
    fn no_output_means_no_notice() {
        let mut e = engine();
        let mut h = Recorder::default();
        run(&mut e, "x = 3;", &mut h).unwrap();
        assert_eq!(h.out, "");
    }

    #[test]
    fn errors_are_honest_and_keep_earlier_assignments() {
        let mut e = engine();
        let mut h = Recorder::default();
        let err = run(&mut e, "a = 1; b = y", &mut h).unwrap_err();
        assert_eq!(err.message, "'y' undefined");
        assert!(e.vars.contains_key("a"));
        let err = run(&mut e, "c = [1 2 3]", &mut h).unwrap_err();
        assert!(err.message.contains("stub"), "{}", err.message);
        let err = run(&mut e, "for k=1:3, end", &mut h).unwrap_err();
        assert!(err.message.contains("outside the stub subset"));
    }

    #[test]
    fn number_formats() {
        assert_eq!(display_number(2.5), "2.5000");
        assert_eq!(display_number(123.456), "123.46");
        assert_eq!(display_number(-0.5), "-0.5000");
        assert_eq!(display_number(1e-7), "1.0000e-07");
        assert_eq!(display_number(123456.7), "1.2346e+05");
        assert_eq!(display_number(f64::NEG_INFINITY), "-Inf");
        assert_eq!(preview_number(2.5), "2.5");
        assert_eq!(preview_number(1e-7), "1e-07");
        assert_eq!(number_literal("1e3"), Some(1000.0));
        assert_eq!(number_literal(".5"), Some(0.5));
        assert_eq!(number_literal("1e"), None);
        assert_eq!(number_literal("3x"), None);
    }

    #[test]
    fn input_and_interrupt() {
        let mut e = engine();
        let mut h = Recorder {
            inputs: vec!["42".into(), "hello".into()],
            ..Default::default()
        };
        run(&mut e, "a = input('A? ')\nb = input('B? ', 's');", &mut h).unwrap();
        assert_eq!(h.prompts, ["A? ", "B? "]);
        assert_eq!(e.vars["a"], Value::Double(42.0));
        assert_eq!(e.vars["b"], Value::Char("hello".into()));
        let mut h = Recorder::default();
        assert!(
            run(&mut e, "c = input('C? ')", &mut h)
                .unwrap_err()
                .interrupted
        );
        let mut h = Recorder {
            stop_after: Some(3),
            ..Default::default()
        };
        let err = run(&mut e, "d = 1; pause(30); d = 2", &mut h).unwrap_err();
        assert!(err.interrupted);
        assert_eq!(e.vars["d"], Value::Double(1.0));
    }

    #[test]
    fn folders_clear_rename_reset() {
        let mut e = engine();
        let start = e.cwd();
        let mut h = Recorder::default();
        run(&mut e, "cd('/')\np = pwd;", &mut h).unwrap();
        assert_eq!(e.cwd(), PathBuf::from("/"));
        assert_eq!(e.vars["p"], Value::Char("/".into()));
        assert!(run(&mut e, "cd /definitely/not/here", &mut h).is_err());
        run(&mut e, "x = 1; y = 2; clc", &mut h).unwrap();
        assert_eq!(h.clc, 1);
        e.rename("x", "w").unwrap();
        assert_eq!(e.rename("w", "y").unwrap_err().code, ErrorCode::Invalid);
        assert_eq!(e.rename("nope", "q").unwrap_err().code, ErrorCode::NotFound);
        assert_eq!(e.clear(Some(&["w".into(), "zz".into()])).unwrap(), ["w"]);
        run(&mut e, "clear y", &mut h).unwrap();
        assert!(!e.vars.contains_key("y"));
        e.reset();
        assert!(e.workspace().is_empty());
        assert_eq!(e.cwd(), start);
    }

    #[test]
    fn inspect_pages() {
        let mut e = engine();
        let mut h = Recorder::default();
        run(&mut e, "x = 0.1; s = 'abc';", &mut h).unwrap();
        let page = e.inspect("x", PageRequest::default()).unwrap();
        assert_eq!(page.get("rows").unwrap().to_string_compact(), "[[\"0.1\"]]");
        assert_eq!(page.get("kind").unwrap().as_str(), Some("matrix"));
        let page = e.inspect("s", PageRequest::default()).unwrap();
        assert_eq!(page.get("text").unwrap().as_str(), Some("abc"));
        assert_eq!(
            e.inspect("nope", PageRequest::default()).unwrap_err().code,
            ErrorCode::NotFound
        );
    }

    #[test]
    fn never_panics_on_odd_input() {
        let samples = [
            "'",
            "''",
            "x = '",
            "((((",
            "))))",
            "x = = 3",
            "= 3",
            "...",
            "x = 3 ...\n+ 4",
            "\u{0}",
            "é = 1",
            "disp(",
            "input()",
            "pause(-1)",
            "cd(3)",
            "clear 1x",
            "%{",
            "x(1) = 2",
            "'a'''",
            "a.b = 1",
            "x = 1e999",
            "x=-",
            "x = --3",
        ];
        for code in samples {
            let mut e = engine();
            let mut h = Recorder::default();
            let _ = run(&mut e, code, &mut h);
        }
    }
}
