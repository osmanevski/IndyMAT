//! The `indy-kernel` binary against `docs/Kernel-Protokolu.md`.

use indy_kernel::Json;
use std::io::{BufRead, BufReader, Write};
use std::os::raw::c_int;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

unsafe extern "C" {
    fn kill(pid: c_int, signal: c_int) -> c_int;
}

struct Kernel {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<Json>,
    next: u64,
}

impl Kernel {
    /// The stub engine (these tests pin its exact subset and output).
    fn start() -> Kernel {
        Kernel::start_with(&["--stub"])
    }

    fn start_with(args: &[&str]) -> Kernel {
        let mut child = Command::new(env!("CARGO_BIN_EXE_indy-kernel"))
            .args(args)
            .env_remove("INDYMAT_R_STUB")
            .current_dir(std::env::temp_dir())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in stdout.lines() {
                let line = line.unwrap();
                tx.send(Json::parse(&line).expect(&line)).unwrap();
            }
        });
        Kernel {
            child,
            stdin,
            lines,
            next: 0,
        }
    }

    fn recv(&self) -> Json {
        self.lines.recv_timeout(Duration::from_secs(10)).unwrap()
    }

    fn raw(&mut self, line: &str) {
        self.stdin.write_all(line.as_bytes()).unwrap();
        self.stdin.write_all(b"\n").unwrap();
        self.stdin.flush().unwrap();
    }

    fn send(&mut self, op: &str, params: Json) -> u64 {
        self.next += 1;
        let mut request = Json::object().with("id", self.next).with("op", op);
        if let Json::Object(entries) = params {
            for (k, v) in entries {
                request = request.with(&k, v);
            }
        }
        let line = request.to_string_compact();
        self.raw(&line);
        self.next
    }

    fn call(&mut self, op: &str, params: Json) -> Json {
        let id = self.send(op, params);
        let reply = self.recv();
        assert_eq!(reply.get("type").unwrap().as_str(), Some("response"));
        assert_eq!(reply.get("id").unwrap().as_u64(), Some(id));
        reply
    }

    /// Accepted eval: returns all events up to the terminal one.
    fn eval(&mut self, code: &str) -> Vec<Json> {
        let reply = self.call("eval", Json::object().with("code", code));
        assert_eq!(reply.get("ok"), Some(&Json::Bool(true)), "{reply:?}");
        self.events()
    }

    fn events(&mut self) -> Vec<Json> {
        let mut events = Vec::new();
        loop {
            let message = self.recv();
            let kind = message.get("event").unwrap().as_str().unwrap().to_string();
            events.push(message);
            if kind == "done" || kind == "error" {
                return events;
            }
        }
    }

    fn sigint(&self) {
        // SAFETY: signalling our own child process.
        assert_eq!(unsafe { kill(self.child.id() as c_int, 2) }, 0);
    }
}

fn kind(message: &Json) -> &str {
    message.get("event").and_then(Json::as_str).unwrap_or("")
}

fn error_code(reply: &Json) -> &str {
    reply
        .get("error")
        .and_then(|e| e.get("code"))
        .and_then(Json::as_str)
        .unwrap_or("")
}

#[test]
fn full_session() {
    let mut k = Kernel::start();
    let ready = k.recv();
    assert_eq!(ready.get("type").unwrap().as_str(), Some("ready"));
    assert_eq!(ready.get("protocol").unwrap().as_u64(), Some(1));
    assert_eq!(
        ready.get("engine").unwrap().get("stub"),
        Some(&Json::Bool(true))
    );

    // x = 3 round trip.
    let events = k.eval("x = 3");
    let text: String = events
        .iter()
        .filter(|e| kind(e) == "output")
        .map(|e| e.get("text").unwrap().as_str().unwrap())
        .collect();
    assert!(text.ends_with("x = 3\n"), "{text:?}");
    assert!(text.contains("stub"));
    assert!(
        events
            .iter()
            .filter(|e| kind(e) == "output")
            .all(|e| e.get("stub") == Some(&Json::Bool(true)))
    );
    assert_eq!(kind(events.last().unwrap()), "done");
    let ws = k.call("workspace", Json::object());
    let vars = ws
        .get("result")
        .unwrap()
        .get("variables")
        .unwrap()
        .as_array()
        .unwrap()
        .to_vec();
    assert_eq!(vars.len(), 1);
    assert_eq!(vars[0].get("name").unwrap().as_str(), Some("x"));
    assert_eq!(vars[0].get("preview").unwrap().as_str(), Some("3"));
    let page = k.call("inspect", Json::object().with("name", "x"));
    assert_eq!(
        page.get("result")
            .unwrap()
            .get("rows")
            .unwrap()
            .to_string_compact(),
        "[[\"3\"]]"
    );

    // Errors are terminal events, earlier assignments stay.
    let events = k.eval("y = 4; z = nope");
    let last = events.last().unwrap();
    assert_eq!(kind(last), "error");
    assert_eq!(
        last.get("message").unwrap().as_str(),
        Some("'nope' undefined")
    );
    assert_eq!(last.get("interrupted"), Some(&Json::Bool(false)));

    // Input request and reply; a reply with nothing pending is invalid.
    let reply = k.call("input_reply", Json::object().with("text", "1"));
    assert_eq!(error_code(&reply), "invalid");
    let reply = k.call("eval", Json::object().with("code", "a = input('A? ');"));
    assert_eq!(reply.get("ok"), Some(&Json::Bool(true)));
    let request = k.recv();
    assert_eq!(kind(&request), "input_request");
    assert_eq!(request.get("prompt").unwrap().as_str(), Some("A? "));
    let busy = k.call("workspace", Json::object());
    assert_eq!(error_code(&busy), "busy");
    let reply = k.call("input_reply", Json::object().with("text", "7"));
    assert_eq!(reply.get("ok"), Some(&Json::Bool(true)));
    assert_eq!(kind(k.events().last().unwrap()), "done");

    // Interrupt twice during pause: one interrupted error, session lives on.
    let reply = k.call(
        "eval",
        Json::object().with("code", "b = 1; pause(30); b = 2"),
    );
    assert_eq!(reply.get("ok"), Some(&Json::Bool(true)));
    std::thread::sleep(Duration::from_millis(200));
    k.sigint();
    k.sigint();
    let events = k.events();
    let last = events.last().unwrap();
    assert_eq!(kind(last), "error");
    assert_eq!(last.get("interrupted"), Some(&Json::Bool(true)));
    // Idle SIGINT has no effect, and does not leak into the next eval.
    k.sigint();
    std::thread::sleep(Duration::from_millis(100));
    let events = k.eval("disp(b)");
    assert_eq!(kind(events.last().unwrap()), "done");
    assert!(
        events
            .iter()
            .any(|e| e.get("text").and_then(Json::as_str) == Some("1\n"))
    );

    // Interrupt cancels a pending input request.
    k.call("eval", Json::object().with("code", "c = input('C? ')"));
    assert_eq!(kind(&k.recv()), "input_request");
    k.sigint();
    assert_eq!(
        k.events().last().unwrap().get("interrupted"),
        Some(&Json::Bool(true))
    );

    // Workspace actions.
    let reply = k.call(
        "rename",
        Json::object().with("old_name", "x").with("new_name", "w"),
    );
    assert_eq!(reply.get("ok"), Some(&Json::Bool(true)));
    let reply = k.call(
        "clear",
        Json::object().with("names", Json::Array(vec!["w".into()])),
    );
    assert_eq!(
        reply
            .get("result")
            .unwrap()
            .get("names")
            .unwrap()
            .to_string_compact(),
        "[\"w\"]"
    );
    let reply = k.call("cd", Json::object().with("path", "/"));
    assert_eq!(
        reply.get("result").unwrap().get("cwd").unwrap().as_str(),
        Some("/")
    );
    let reply = k.call("cd", Json::object().with("path", "/no/such/folder"));
    assert_eq!(error_code(&reply), "not_found");

    // Unsupported features say so explicitly.
    for (op, feature) in [
        ("debug", "debugger"),
        ("breakpoint", "breakpoints"),
        ("run_to_cursor", "run_to_cursor"),
        ("package", "packages"),
        ("save", "mat_files"),
    ] {
        let reply = k.call(op, Json::object());
        assert_eq!(error_code(&reply), "unsupported");
        let error = reply.get("error").unwrap();
        assert_eq!(error.get("feature").unwrap().as_str(), Some(feature));
        assert!(
            error
                .get("message")
                .unwrap()
                .as_str()
                .unwrap()
                .contains("the Rust engine")
        );
    }
    assert_eq!(
        error_code(&k.call("frobnicate", Json::object())),
        "unknown_op"
    );
    k.raw("{not json");
    assert_eq!(error_code(&k.recv()), "bad_request");
    k.raw("{\"op\":\"info\"}");
    assert_eq!(error_code(&k.recv()), "bad_request");
    let reply = k.call("eval", Json::object().with("mode", "file"));
    assert_eq!(error_code(&reply), "bad_request");

    // Reset empties the workspace.
    let reply = k.call("reset", Json::object());
    assert_eq!(reply.get("ok"), Some(&Json::Bool(true)));
    let ws = k.call("workspace", Json::object());
    assert_eq!(
        ws.get("result")
            .unwrap()
            .get("variables")
            .unwrap()
            .to_string_compact(),
        "[]"
    );

    // Close answers, then the process exits 0.
    let reply = k.call("close", Json::object());
    assert_eq!(reply.get("ok"), Some(&Json::Bool(true)));
    let status = k.child.wait().unwrap();
    assert!(status.success());
}

#[test]
fn file_mode_reads_the_file_and_names_it_in_errors() {
    let dir = std::env::temp_dir().join(format!("indy-kernel-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("script.m");
    std::fs::write(&file, "q = 5\nr = missing\n").unwrap();
    let mut k = Kernel::start();
    k.recv();
    let path = file.display().to_string();
    let reply = k.call(
        "eval",
        Json::object()
            .with("mode", "file")
            .with("path", path.as_str()),
    );
    assert_eq!(reply.get("ok"), Some(&Json::Bool(true)));
    let events = k.events();
    let last = events.last().unwrap();
    assert_eq!(kind(last), "error");
    let location = last.get("location").unwrap();
    assert_eq!(location.get("line").unwrap().as_u64(), Some(2));
    assert_eq!(location.get("file").unwrap().as_str(), Some(path.as_str()));
    let reply = k.call(
        "eval",
        Json::object()
            .with("mode", "file")
            .with("path", dir.join("absent.m").display().to_string()),
    );
    assert_eq!(error_code(&reply), "not_found");
    // EOF on stdin ends the child.
    drop(k.stdin);
    assert!(k.child.wait().unwrap().success());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn close_during_eval_abandons_it() {
    let mut k = Kernel::start();
    k.recv();
    let reply = k.call("eval", Json::object().with("code", "pause(30)"));
    assert_eq!(reply.get("ok"), Some(&Json::Bool(true)));
    let reply = k.call("close", Json::object());
    assert_eq!(reply.get("ok"), Some(&Json::Bool(true)));
    assert!(k.child.wait().unwrap().success());
}

// ----- the real engine (default) -------------------------------------------

fn real() -> Kernel {
    let k = Kernel::start_with(&[]);
    let ready = k.recv();
    let engine = ready.get("engine").unwrap();
    assert_eq!(engine.get("name").unwrap().as_str(), Some("indy"));
    assert_eq!(engine.get("stub"), Some(&Json::Bool(false)));
    assert_eq!(
        engine.get("description").unwrap().as_str(),
        Some("IndyMAT Rust engine (experimental)")
    );
    k
}

fn output(events: &[Json], stream: &str) -> String {
    events
        .iter()
        .filter(|e| kind(e) == "output" && e.get("stream").unwrap().as_str() == Some(stream))
        .map(|e| e.get("text").unwrap().as_str().unwrap())
        .collect()
}

fn variables(k: &mut Kernel) -> Vec<Json> {
    let ws = k.call("workspace", Json::object());
    ws.get("result")
        .unwrap()
        .get("variables")
        .unwrap()
        .as_array()
        .unwrap()
        .to_vec()
}

fn message(event: &Json) -> &str {
    event.get("message").unwrap().as_str().unwrap()
}

#[test]
fn real_engine_computes_and_keeps_the_workspace() {
    let mut k = real();
    let events = k.eval("A = magic(3); B = A*A'");
    assert_eq!(kind(events.last().unwrap()), "done");
    let text = output(&events, "stdout");
    assert!(text.starts_with("B =\n"), "{text:?}");
    assert!(text.contains("101    71    53"), "{text:?}");
    assert!(events.iter().all(|e| e.get("stub").is_none()));
    // The workspace persists across evals.
    let events = k.eval("C = B(1,1) + A(2,2)");
    assert!(output(&events, "stdout").contains("C = 106"));
    let vars = variables(&mut k);
    let names: Vec<&str> = vars
        .iter()
        .map(|v| v.get("name").unwrap().as_str().unwrap())
        .collect();
    assert_eq!(names, ["A", "B", "C"]);
    let b = &vars[1];
    assert_eq!(b.get("class").unwrap().as_str(), Some("double"));
    assert_eq!(b.get("size").unwrap().to_string_compact(), "[3,3]");
    assert_eq!(b.get("bytes").unwrap().as_u64(), Some(72));
    assert_eq!(
        b.get("preview").unwrap().as_str(),
        Some("[101 71 53;71 83 71;53 71 101]")
    );
    let page = k.call(
        "inspect",
        Json::object()
            .with("name", "B")
            .with("row", 2u64)
            .with("rows", 1u64),
    );
    let page = page.get("result").unwrap();
    assert_eq!(page.get("kind").unwrap().as_str(), Some("matrix"));
    assert_eq!(
        page.get("rows").unwrap().to_string_compact(),
        r#"[["71","83","71"]]"#
    );
    assert_eq!(page.get("truncated"), Some(&Json::Bool(true)));
    let events = k.eval("c = {1, 'x'}; s.a = 1;");
    assert_eq!(kind(events.last().unwrap()), "done");
    let page = k.call("inspect", Json::object().with("name", "c"));
    let page = page.get("result").unwrap();
    assert_eq!(page.get("kind").unwrap().as_str(), Some("detail"));
    assert!(
        page.get("text")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("{2}: 1x1 char")
    );
    let reply = k.call("inspect", Json::object().with("name", "nope"));
    assert_eq!(error_code(&reply), "not_found");
    // rename / clear / reset.
    let reply = k.call(
        "rename",
        Json::object().with("old_name", "C").with("new_name", "D"),
    );
    assert_eq!(reply.get("ok"), Some(&Json::Bool(true)));
    let events = k.eval("disp(D)");
    assert_eq!(output(&events, "stdout"), "106\n");
    let reply = k.call(
        "clear",
        Json::object().with("names", Json::Array(vec!["D".into(), "nope".into()])),
    );
    assert_eq!(
        reply
            .get("result")
            .unwrap()
            .get("names")
            .unwrap()
            .to_string_compact(),
        r#"["D"]"#
    );
    k.call("reset", Json::object());
    assert!(variables(&mut k).is_empty());
    // info lists the features no engine implements, for the UI.
    let info = k.call("info", Json::object());
    let unsupported = info.get("result").unwrap().get("unsupported").unwrap();
    assert!(unsupported.to_string_compact().contains("\"profiler\""));
    assert!(!unsupported.to_string_compact().contains("\"debugger\""));
}

#[test]
fn real_engine_routes_output_errors_and_graphics() {
    let mut k = real();
    let events =
        k.eval("disp(pi); fprintf('%d\\n', 1:3); fprintf(2, 'oops\\n'); warning('careful')");
    assert_eq!(output(&events, "stdout"), "3.1416\n1\n2\n3\n");
    assert_eq!(output(&events, "stderr"), "oops\nwarning: careful\n");
    let events = k.eval("x = 1; y = undefined_thing + 1");
    let last = events.last().unwrap();
    assert_eq!(kind(last), "error");
    assert_eq!(
        message(last),
        "Undefined function or variable 'undefined_thing'."
    );
    assert_eq!(last.get("interrupted"), Some(&Json::Bool(false)));
    assert!(last.get("location").is_none());
    let events = k.eval("disp(x)");
    assert_eq!(output(&events, "stdout"), "1\n");
    // Graphics the model does not cover keep an explicit error.
    let events = k.eval("ginput(1)");
    assert_eq!(
        message(events.last().unwrap()),
        "ginput: graphics not yet available in the Rust engine"
    );
    assert!(events.iter().all(|e| kind(e) != "figure"));
    // Output still batched at `clc` is dropped (the client discards it).
    let events = k.eval("disp(1); clc; disp(2)");
    let kinds: Vec<&str> = events.iter().map(kind).collect();
    assert_eq!(kinds, ["clc", "output", "done"]);
    assert_eq!(output(&events, "stdout"), "2\n");
}

fn figures(events: &[Json]) -> Vec<&Json> {
    events.iter().filter(|e| kind(e) == "figure").collect()
}

fn numbers(events: &[Json]) -> Vec<u64> {
    figures(events)
        .iter()
        .map(|e| e.get("number").and_then(Json::as_u64).unwrap())
        .collect()
}

#[test]
fn real_engine_reports_the_open_figures_after_every_eval() {
    let mut k = real();
    let events = k.eval("plot(1:3)");
    assert_eq!(kind(events.last().unwrap()), "done");
    let figs = figures(&events);
    assert_eq!(figs.len(), 1);
    let f = figs[0];
    assert_eq!(f.get("number").and_then(Json::as_u64), Some(1));
    assert_eq!(f.get("name").and_then(Json::as_str), Some("Figure 1"));
    assert_eq!(f.get("reason").and_then(Json::as_str), Some(""));
    let data = f.get("figure").unwrap();
    assert_eq!(data.get("supported"), Some(&Json::Bool(true)));
    let source = data.get("source").unwrap();
    assert_eq!(source.get("job").and_then(Json::as_str), Some(""));
    // Figure events come before the terminal event.
    let at = events.iter().position(|e| kind(e) == "figure").unwrap();
    assert!(at < events.len() - 1);
    // Unchanged figures are still reported (the list is complete).
    assert_eq!(numbers(&k.eval("x = 1;")), [1]);
    // Also after an error; an empty figure is an explicit fallback.
    let events = k.eval("figure('Name', 'Empty'); error('boom')");
    assert_eq!(kind(events.last().unwrap()), "error");
    assert_eq!(numbers(&events), [1, 2]);
    let empty = figures(&events)[1];
    assert_eq!(empty.get("name").and_then(Json::as_str), Some("Empty"));
    let data = empty.get("figure").unwrap();
    assert_eq!(data.get("supported"), Some(&Json::Bool(false)));
    assert_eq!(
        data.get("reason_code").and_then(Json::as_str),
        Some("no_axes")
    );
    assert!(
        !empty
            .get("reason")
            .and_then(Json::as_str)
            .unwrap()
            .is_empty()
    );
    // Handles are doubles; get/set go through the model.
    let events = k.eval("figure(1); h = plot(1:3); set(h, 'LineWidth', 3); disp(get(h, 'LineWidth')); disp(class(h)); disp(ishandle(h)); disp(isgraphics(gca, 'axes'))");
    assert_eq!(output(&events, "stdout"), "3\ndouble\n1\n1\n");
    assert_eq!(numbers(&k.eval("close(1)")), [2]);
    assert!(figures(&k.eval("close all")).is_empty());
    // At most 12 figures, in number order.
    let events = k.eval("for n = 13:-1:1, figure(n); end");
    assert_eq!(numbers(&events), (1..=12).collect::<Vec<u64>>());
    // `reset` closes every figure.
    k.call("reset", Json::object());
    assert!(figures(&k.eval("y = 2;")).is_empty());
}

#[test]
fn real_engine_interrupts_loops_twice_and_stays_usable() {
    let mut k = real();
    for code in [
        "for k = 1:1e9, end",
        "while true, try, pause(0.05); catch, end, end",
        "pause(Inf)",
    ] {
        let reply = k.call("eval", Json::object().with("code", code));
        assert_eq!(reply.get("ok"), Some(&Json::Bool(true)));
        std::thread::sleep(Duration::from_millis(300));
        k.sigint();
        k.sigint();
        let events = k.events();
        let last = events.last().unwrap();
        assert_eq!(kind(last), "error", "{code}");
        assert_eq!(last.get("interrupted"), Some(&Json::Bool(true)), "{code}");
        assert_eq!(message(last), "Operation interrupted.");
    }
    // Variables assigned before the interrupt stay; the next eval is clean.
    let events = k.eval("disp(k > 1)");
    assert_eq!(output(&events, "stdout"), "1\n");
    k.sigint();
    std::thread::sleep(Duration::from_millis(100));
    let events = k.eval("z = 2;");
    assert_eq!(kind(events.last().unwrap()), "done");
}

/// Events up to the terminal one; responses to side requests are skipped.
fn events_skipping_responses(k: &mut Kernel) -> Vec<Json> {
    let mut events = Vec::new();
    loop {
        let message = k.recv();
        if message.get("type").unwrap().as_str() == Some("response") {
            assert_eq!(message.get("ok"), Some(&Json::Bool(true)));
            continue;
        }
        let terminal = matches!(kind(&message), "done" | "error");
        events.push(message);
        if terminal {
            return events;
        }
    }
}

#[test]
fn real_engine_input_round_trips() {
    let mut k = real();
    let reply = k.call("eval", Json::object().with("code", "x = input('n? ')"));
    assert_eq!(reply.get("ok"), Some(&Json::Bool(true)));
    let prompt = k.recv();
    assert_eq!(prompt.get("text").unwrap().as_str(), Some("n? "));
    let request = k.recv();
    assert_eq!(kind(&request), "input_request");
    assert_eq!(request.get("prompt").unwrap().as_str(), Some("n? "));
    k.send("input_reply", Json::object().with("text", "6*7"));
    let events = events_skipping_responses(&mut k);
    assert_eq!(output(&events, "stdout"), "x = 42\n");
    // Text mode, then an interrupt cancels a pending input.
    let reply = k.call(
        "eval",
        Json::object().with("code", "s = input('s? ', 's');"),
    );
    assert_eq!(reply.get("ok"), Some(&Json::Bool(true)));
    k.recv();
    k.recv();
    k.send("input_reply", Json::object().with("text", "hello"));
    let events = events_skipping_responses(&mut k);
    assert_eq!(kind(events.last().unwrap()), "done");
    let reply = k.call("eval", Json::object().with("code", "t = input('t? ');"));
    assert_eq!(reply.get("ok"), Some(&Json::Bool(true)));
    k.recv();
    k.recv();
    k.sigint();
    let last = k.events().pop().unwrap();
    assert_eq!(last.get("interrupted"), Some(&Json::Bool(true)));
    let events = k.eval("disp(s)");
    assert_eq!(output(&events, "stdout"), "hello\n");
}

#[test]
fn real_engine_files_cd_and_locations() {
    let dir = std::env::temp_dir().join(format!("indy-kernel-real-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    let dir = dir.canonicalize().unwrap();
    let script = dir.join("script.m");
    std::fs::write(&script, "q = 5;\nr = helper(q)\nfail_here\n").unwrap();
    std::fs::write(
        dir.join("helper.m"),
        "function y = helper(x)\n  y = 2*x;\nend\n",
    )
    .unwrap();
    let mut k = real();
    let reply = k.call("cd", Json::object().with("path", dir.display().to_string()));
    assert_eq!(reply.get("ok"), Some(&Json::Bool(true)));
    let path = script.display().to_string();
    let reply = k.call(
        "eval",
        Json::object()
            .with("mode", "file")
            .with("path", path.as_str()),
    );
    assert_eq!(reply.get("ok"), Some(&Json::Bool(true)));
    let events = k.events();
    assert_eq!(output(&events, "stdout"), "r = 10\n");
    let last = events.last().unwrap();
    assert_eq!(kind(last), "error");
    let location = last.get("location").unwrap();
    assert_eq!(location.get("file").unwrap().as_str(), Some(path.as_str()));
    assert_eq!(location.get("line").unwrap().as_u64(), Some(3));
    // An edited function file is seen by the next eval.
    std::fs::write(
        dir.join("helper.m"),
        "function y = helper(x)\n  y = 3*x;\nend\n",
    )
    .unwrap();
    let events = k.eval("disp(helper(q))");
    assert_eq!(output(&events, "stdout"), "15\n");
    // cd inside code moves the engine folder.
    let events = k.eval("cd sub");
    let done = events.last().unwrap();
    assert_eq!(
        done.get("cwd").unwrap().as_str(),
        Some(dir.join("sub").display().to_string().as_str())
    );
    let reply = k.call("pwd", Json::object());
    assert!(
        reply
            .get("result")
            .unwrap()
            .get("cwd")
            .unwrap()
            .as_str()
            .unwrap()
            .ends_with("sub")
    );
    drop(k.stdin);
    assert!(k.child.wait().unwrap().success());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn stub_stays_available_by_flag_and_environment() {
    for (args, env) in [(&["--stub"][..], None), (&[][..], Some("1"))] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_indy-kernel"));
        command
            .args(args)
            .env_remove("INDYMAT_R_STUB")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(value) = env {
            command.env("INDYMAT_R_STUB", value);
        }
        let mut child = command.spawn().unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let ready = Json::parse(line.trim()).unwrap();
        assert_eq!(
            ready.get("engine").unwrap().get("stub"),
            Some(&Json::Bool(true))
        );
        drop(child.stdin.take());
        assert!(child.wait().unwrap().success());
    }
}

fn result(reply: &Json) -> &Json {
    assert_eq!(reply.get("ok"), Some(&Json::Bool(true)), "{reply:?}");
    reply.get("result").unwrap()
}

fn strings(v: &Json) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn real_engine_saves_and_loads_mat_files() {
    let dir = std::env::temp_dir().join(format!("indy_kernel_mat_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("ws.mat");
    let path = file.display().to_string();
    let mut k = real();
    // The real engine implements save/load; the MAT feature is not refused.
    let info = k.call("info", Json::object());
    let info = result(&info);
    assert!(strings(info.get("ops").unwrap()).contains(&"save".to_string()));
    assert!(
        !info
            .get("unsupported")
            .unwrap()
            .to_string_compact()
            .contains("mat_files")
    );

    // Nothing to save yet.
    let reply = k.call("save", Json::object().with("path", path.as_str()));
    assert_eq!(error_code(&reply), "invalid");
    k.eval("x = magic(3); s.a = 'metin'; c = {1, 2}; f = table([1;2]);");
    // Save a selection.
    let reply = k.call(
        "save",
        Json::object()
            .with("path", path.as_str())
            .with("names", Json::Array(vec!["x".into(), "s".into()])),
    );
    let r = result(&reply);
    assert_eq!(strings(r.get("names").unwrap()), ["x", "s"]);
    let reply = k.call(
        "save",
        Json::object()
            .with("path", path.as_str())
            .with("names", Json::Array(vec!["nope".into()])),
    );
    assert_eq!(error_code(&reply), "invalid");
    // Save everything: the table is skipped with a warning.
    let reply = k.call("save", Json::object().with("path", path.as_str()));
    let r = result(&reply);
    assert_eq!(strings(r.get("names").unwrap()), ["c", "s", "x"]);
    let warnings = strings(r.get("warnings").unwrap());
    assert!(
        warnings.len() == 1 && warnings[0].contains("'f'"),
        "{warnings:?}"
    );
    // Handles are saved; an element that cannot be is named by its path.
    let other = dir.join("tutamac.mat").display().to_string();
    k.eval("h = @(x) x + 1; w = {1, table([1;2])};");
    let reply = k.call(
        "save",
        Json::object()
            .with("path", other.as_str())
            .with("names", Json::Array(vec!["h".into(), "w".into()])),
    );
    let r = result(&reply);
    assert_eq!(strings(r.get("names").unwrap()), ["h", "w"]);
    let warnings = strings(r.get("warnings").unwrap());
    assert!(
        warnings.len() == 1 && warnings[0].contains("'w{2}' was saved as []"),
        "{warnings:?}"
    );
    k.eval("clear h w");

    // Inspect lists the file without touching the workspace.
    k.eval("x = 0; clear s");
    let reply = k.call(
        "load",
        Json::object()
            .with("path", path.as_str())
            .with("inspect", true),
    );
    let r = result(&reply);
    let variables = r.get("variables").unwrap().as_array().unwrap().to_vec();
    assert_eq!(
        variables[2].to_string_compact(),
        r#"{"name":"x","size":"3x3","class":"double","bytes":72}"#
    );
    assert_eq!(strings(r.get("replacements").unwrap()), ["c", "x"]);
    let events = k.eval("disp(x)");
    assert_eq!(output(&events, "stdout"), "0\n");

    // Load checks the inventory and the confirmed replacements first.
    let inventory = Json::Array(variables.clone());
    let reply = k.call(
        "load",
        Json::object()
            .with("path", path.as_str())
            .with("inventory", inventory.clone())
            .with("replacements", Json::Array(vec!["x".into()])),
    );
    assert_eq!(error_code(&reply), "invalid");
    let events = k.eval("disp(x)");
    assert_eq!(output(&events, "stdout"), "0\n");
    let reply = k.call(
        "load",
        Json::object()
            .with("path", path.as_str())
            .with("inventory", Json::Array(variables[..2].to_vec()))
            .with("replacements", Json::Array(vec!["c".into(), "x".into()])),
    );
    assert_eq!(error_code(&reply), "invalid");
    let reply = k.call(
        "load",
        Json::object()
            .with("path", path.as_str())
            .with("inventory", inventory)
            .with("replacements", Json::Array(vec!["c".into(), "x".into()])),
    );
    let r = result(&reply);
    assert_eq!(strings(r.get("names").unwrap()), ["c", "s", "x"]);
    let events = k.eval("disp(isequal(x, magic(3)) && strcmp(s.a, 'metin') && isequal(c, {1, 2}))");
    assert_eq!(output(&events, "stdout"), "1\n");

    // Missing and damaged files.
    let reply = k.call(
        "load",
        Json::object()
            .with("path", dir.join("nope.mat").display().to_string())
            .with("inspect", true),
    );
    assert_eq!(error_code(&reply), "not_found");
    std::fs::write(dir.join("bad.mat"), b"not a mat file").unwrap();
    let reply = k.call(
        "load",
        Json::object()
            .with("path", dir.join("bad.mat").display().to_string())
            .with("inspect", true),
    );
    assert_eq!(error_code(&reply), "invalid");

    // The stub still refuses MAT files.
    let mut stub = Kernel::start();
    let ready = stub.recv();
    assert_eq!(ready.get("type").unwrap().as_str(), Some("ready"));
    let info = stub.call("info", Json::object());
    assert!(
        result(&info)
            .get("unsupported")
            .unwrap()
            .to_string_compact()
            .contains("mat_files")
    );
    let reply = stub.call("load", Json::object().with("path", path.as_str()));
    assert_eq!(error_code(&reply), "unsupported");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn real_engine_shows_sparse_variables() {
    let mut k = real();
    k.eval(
        "S = sparse([1 2],[1 2],[3 4.5]); C = sparse(2,1,1-2i,2,2); L = sparse(logical([0 1]));
         B = sparse(100000,100000); B(99999,100000) = 7; E = sparse(2,3); c = {S};",
    );
    let vars = variables(&mut k);
    let row = |name: &str| {
        vars.iter()
            .find(|v| v.get("name").unwrap().as_str() == Some(name))
            .unwrap()
            .clone()
    };
    let text = |v: &Json, f: &str| v.get(f).unwrap().as_str().unwrap().to_string();
    let s = row("S");
    assert_eq!(text(&s, "class"), "double");
    assert_eq!(text(&s, "preview"), "sparse [3 0;0 4.5]");
    assert_eq!(s.get("complex"), Some(&Json::Bool(false)));
    assert_eq!(s.get("bytes").unwrap().to_string_compact(), "56");
    let c = row("C");
    assert_eq!(c.get("complex"), Some(&Json::Bool(true)));
    assert_eq!(text(&c, "preview"), "sparse [0+0i 0+0i;1-2i 0+0i]");
    assert_eq!(text(&row("L"), "class"), "logical");
    assert_eq!(text(&row("E"), "preview"), "sparse [0 0 0;0 0 0]");
    let b = row("B");
    assert_eq!(text(&b, "preview"), "100000x100000 sparse double");
    assert_eq!(
        b.get("size").unwrap().to_string_compact(),
        "[100000,100000]"
    );

    // Pages come from the storage: no full form, whatever the size.
    let page = k.call("inspect", Json::object().with("name", "S"));
    let page = result(&page);
    assert_eq!(page.get("kind").unwrap().as_str(), Some("matrix"));
    assert_eq!(page.get("sparse"), Some(&Json::Bool(true)));
    assert_eq!(page.get("nnz").unwrap().to_string_compact(), "2");
    assert_eq!(
        page.get("rows").unwrap().to_string_compact(),
        r#"[["3","0"],["0","4.5"]]"#
    );
    let page = k.call("inspect", Json::object().with("name", "C"));
    let page = result(&page);
    assert_eq!(page.get("real"), Some(&Json::Bool(false)));
    assert_eq!(
        page.get("rows").unwrap().to_string_compact(),
        r#"[["0+0i","0+0i"],["1-2i","0+0i"]]"#
    );
    let page = k.call("inspect", Json::object().with("name", "L"));
    assert_eq!(
        result(&page).get("rows").unwrap().to_string_compact(),
        r#"[["0","1"]]"#
    );
    let page = k.call(
        "inspect",
        Json::object()
            .with("name", "B")
            .with("row", 99999u64)
            .with("column", 99999u64)
            .with("rows", 2u64)
            .with("columns", 2u64),
    );
    let page = result(&page);
    assert_eq!(
        page.get("rows").unwrap().to_string_compact(),
        r#"[["0","7"],["0","0"]]"#
    );
    assert_eq!(page.get("truncated"), Some(&Json::Bool(true)));
    let page = k.call("inspect", Json::object().with("name", "c"));
    assert!(
        result(&page)
            .get("text")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("{1}: 2x2 sparse double")
    );
}
