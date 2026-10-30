//! Builtins that only make sense in an interactive session: `input`,
//! `clc`/`home` and an interruptible `pause` (`keyboard` is a debugger pause,
//! see `indy_interp::debug`). They talk to the
//! kernel through `Interp::session`; the `indy` CLI never registers them.
//! The graphics builtins come from `indy-graphics-builtins`, which the CLI
//! registers as well.

use indy_interp::builtins::{arg_str, err, one};
use indy_interp::{BuiltinFn, Interp, R, Value};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// The interpreter's own `pause`, used for the forms the session does not
/// change (`pause on`, `pause('query')`, invalid arguments).
static PLAIN_PAUSE: OnceLock<BuiltinFn> = OnceLock::new();

pub fn register(it: &mut Interp) {
    if let Some(&plain) = it.builtins.get("pause") {
        let _ = PLAIN_PAUSE.set(plain);
    }
    it.builtins.insert("input", input);
    it.builtins.insert("clc", clc);
    it.builtins.insert("home", clc);
    it.builtins.insert("pause", pause);
    // Graphics (shared with the `indy` CLI): the covered names over the
    // `indy-graphics` model, the others as explicit errors. The engine
    // exports the open figures after each eval.
    indy_graphics_builtins::register(it);
}

/// One line from the session; an interrupt ends the running code.
fn read_line(it: &mut Interp, prompt: &str, fname: &str) -> R<String> {
    let Some(session) = it.session.as_mut() else {
        return Err(err(
            "IndyR:noSession",
            format!("{fname}: no interactive session to read from."),
        ));
    };
    match session.input(prompt) {
        Some(line) => Ok(line),
        None => {
            it.interrupt = true;
            Err(Interp::interrupted_error())
        }
    }
}

/// `input(prompt)` evaluates the reply in the caller's workspace (an empty
/// reply gives `[]`, an invalid one asks again); `input(prompt, 's')`
/// returns the text. Like IndyMAT's Octave bridge, the prompt is printed
/// and the reply is not echoed.
fn input(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    if a.len() > 2 {
        return Err(err(
            "MATLAB:TooManyInputs",
            "input: too many input arguments.",
        ));
    }
    let prompt = match a.first() {
        Some(v) => arg_str(v, "input")?,
        None => String::new(),
    };
    let text_mode = match a.get(1) {
        None => false,
        Some(v) if v.to_str().as_deref() == Some("s") => true,
        Some(_) => {
            return Err(err(
                "MATLAB:input:invalidOption",
                "input: the second argument must be 's'.",
            ));
        }
    };
    loop {
        it.print(&prompt);
        let line = read_line(it, &prompt, "input")?;
        if text_mode {
            return one(Value::str(&line));
        }
        if line.trim().is_empty() {
            return one(Value::empty());
        }
        match it.call_named("eval", vec![Value::str(&line)], 1) {
            Ok(mut values) if !values.is_empty() => return Ok(vec![values.swap_remove(0)]),
            Ok(_) => it.eprint("input: the reply produced no value.\n"),
            Err(e) if it.interrupt => return Err(e),
            Err(e) => it.eprint(&format!("{}\n", e.message)),
        }
    }
}

fn clc(it: &mut Interp, _: Vec<Value>, _: usize) -> R<Vec<Value>> {
    if let Some(session) = it.session.as_mut() {
        session.clc();
    }
    Ok(vec![])
}

/// `pause(t)` waits the full time (the CLI caps it at 1 s) and stops at an
/// interrupt; `pause(Inf)` waits for one.
fn pause(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let seconds = match a.as_slice() {
        [v] if v.is_numeric() && v.is_scalar() => v.scalar_f64().filter(|t| *t >= 0.0),
        _ => None,
    };
    let Some(seconds) = seconds else {
        return match PLAIN_PAUSE.get() {
            Some(plain) => plain(it, a, nargout),
            None => Ok(vec![]),
        };
    };
    let end =
        (seconds.is_finite()).then(|| Instant::now() + Duration::from_secs_f64(seconds.min(1e9)));
    loop {
        it.poll_interrupt()?;
        let slice = match end {
            Some(end) => {
                let now = Instant::now();
                if now >= end {
                    return Ok(vec![]);
                }
                (end - now).min(Duration::from_millis(20))
            }
            None => Duration::from_millis(20),
        };
        std::thread::sleep(slice);
    }
}
