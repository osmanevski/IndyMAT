//! The debugger part of `docs/Kernel-Protokolu.md` on the real `indy-kernel`.

use indy_kernel::Json;
use std::io::{BufRead, BufReader, Write};
use std::os::raw::c_int;
use std::path::{Path, PathBuf};
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
    dir: PathBuf,
    eval: u64,
}

impl Drop for Kernel {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start(test: &str, files: &[(&str, &str)]) -> Kernel {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("dbg-{test}"));
    std::fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    for (name, src) in files {
        std::fs::write(dir.join(name), src).unwrap();
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_indy-kernel"))
        .env_remove("INDYMAT_R_STUB")
        .current_dir(&dir)
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
            let Ok(line) = line else { return };
            if tx.send(Json::parse(&line).expect(&line)).is_err() {
                return;
            }
        }
    });
    let k = Kernel {
        child,
        stdin,
        lines,
        next: 0,
        dir,
        eval: 0,
    };
    assert_eq!(k.recv().get("type").unwrap().as_str(), Some("ready"));
    k
}

fn text<'a>(m: &'a Json, key: &str) -> &'a str {
    m.get(key).and_then(Json::as_str).unwrap_or("")
}

fn num(m: &Json, key: &str) -> u64 {
    m.get(key).and_then(Json::as_u64).unwrap()
}

impl Kernel {
    fn recv(&self) -> Json {
        self.lines.recv_timeout(Duration::from_secs(10)).unwrap()
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
        self.stdin.write_all(line.as_bytes()).unwrap();
        self.stdin.write_all(b"\n").unwrap();
        self.stdin.flush().unwrap();
        self.next
    }

    /// Send a request and return its response. Events seen before the
    /// response go to `seen`.
    fn call(&mut self, op: &str, params: Json, seen: &mut Vec<Json>) -> Json {
        let id = self.send(op, params);
        loop {
            let m = self.recv();
            if text(&m, "type") == "response" {
                assert_eq!(m.get("id").unwrap().as_u64(), Some(id));
                return m;
            }
            seen.push(m);
        }
    }

    fn ok(&mut self, op: &str, params: Json) -> Json {
        let mut seen = Vec::new();
        let reply = self.call(op, params, &mut seen);
        assert_eq!(reply.get("ok"), Some(&Json::Bool(true)), "{reply:?}");
        assert!(seen.is_empty(), "{seen:?}");
        reply.get("result").unwrap().clone()
    }

    fn bp(&mut self, file: &str, line: u64, condition: &str) -> Json {
        let path = self.dir.join(file).display().to_string();
        self.ok(
            "breakpoint",
            Json::object()
                .with("action", "set")
                .with("path", path)
                .with("line", line)
                .with("condition", condition),
        )
    }

    fn start_eval(&mut self, code: &str) {
        self.eval = self.next + 1;
        self.ok("eval", Json::object().with("code", code));
    }

    /// Events up to the next `paused` or terminal event.
    fn until_stop(&mut self) -> (Json, Vec<Json>) {
        let mut before = Vec::new();
        loop {
            let m = self.recv();
            assert_eq!(text(&m, "type"), "event", "{m:?}");
            assert_eq!(num(&m, "id"), self.eval);
            match text(&m, "event") {
                "paused" | "done" | "error" => return (m, before),
                _ => before.push(m),
            }
        }
    }

    fn paused(&mut self) -> Json {
        let (m, before) = self.until_stop();
        assert_eq!(text(&m, "event"), "paused", "{m:?} {before:?}");
        m
    }

    fn debug(&mut self, command: &str, extra: Json) -> Json {
        let mut params = Json::object().with("command", command);
        if let Json::Object(entries) = extra {
            for (k, v) in entries {
                params = params.with(&k, v);
            }
        }
        self.ok("debug", params)
    }

    fn sigint(&self) {
        // SAFETY: signalling our own child process.
        assert_eq!(unsafe { kill(self.child.id() as c_int, 2) }, 0);
    }
}

fn names(m: &Json) -> Vec<String> {
    m.get("variables")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|v| text(v, "name").to_string())
        .collect()
}

fn stack(m: &Json) -> Vec<String> {
    m.get("stack")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|f| format!("{}:{}", text(f, "name"), num(f, "line")))
        .collect()
}

const NESTED: &str = "function out = nest(seed)\n  outer_local = seed + 100;\n  out = middle(seed);\nend\nfunction mid = middle(value)\n  middle_local = [value, value + 10];\n  mid = inner(value);\nend\nfunction answer = inner(value)\n  inner_local = value + 1;\n  answer = inner_local * 2;\nend\n";

#[test]
fn info_lists_the_debugger_as_supported() {
    let mut k = start("info", &[]);
    let info = k.ok("info", Json::object());
    let ops = info.get("ops").unwrap().to_string_compact();
    for op in ["\"debug\"", "\"breakpoint\"", "\"run_to_cursor\""] {
        assert!(ops.contains(op), "{ops}");
    }
    let unsupported = info.get("unsupported").unwrap().to_string_compact();
    for feature in ["\"debugger\"", "\"breakpoints\"", "\"run_to_cursor\""] {
        assert!(!unsupported.contains(feature), "{unsupported}");
    }
}

#[test]
fn pause_stack_frames_variables_and_continue() {
    let mut k = start("pause", &[("nest.m", NESTED)]);
    let set = k.bp("nest.m", 11, "");
    assert_eq!(num(&set, "actual_line"), 11);
    k.start_eval("r = nest(4);");
    let p = k.paused();
    assert_eq!(text(&p, "reason"), "breakpoint");
    assert_eq!(text(&p, "file"), k.dir.join("nest.m").display().to_string());
    assert_eq!(num(&p, "line"), 11);
    assert_eq!(num(&p, "frame"), 1);
    assert_eq!(stack(&p), ["inner:11", "middle:7", "nest:3"]);
    assert!(names(&p).contains(&"inner_local".to_string()));
    assert!(p.get("run_to_cursor").unwrap().is_null());

    k.debug("frame", Json::object().with("frame", 2u64));
    let p = k.paused();
    assert_eq!(num(&p, "frame"), 2);
    assert_eq!(num(&p, "line"), 7);
    assert!(names(&p).contains(&"middle_local".to_string()));
    assert!(!names(&p).contains(&"inner_local".to_string()));

    // Workspace and inspect read the selected frame.
    let ws = k.ok("workspace", Json::object());
    assert!(names(&ws).contains(&"middle_local".to_string()));
    let page = k.debug("inspect", Json::object().with("name", "middle_local"));
    assert_eq!(
        page.get("rows").unwrap().to_string_compact(),
        "[[\"4\",\"14\"]]"
    );
    let page = k.ok("inspect", Json::object().with("name", "middle_local"));
    assert_eq!(text(&page, "name"), "middle_local");

    // Invalid frame; other ops are busy; input is refused.
    let mut seen = Vec::new();
    let bad = k.call(
        "debug",
        Json::object().with("command", "frame").with("frame", 99u64),
        &mut seen,
    );
    assert_eq!(text(bad.get("error").unwrap(), "code"), "invalid");
    let busy = k.call("eval", Json::object().with("code", "1"), &mut seen);
    assert_eq!(text(busy.get("error").unwrap(), "code"), "busy");
    let no = k.call("input_reply", Json::object().with("text", "1"), &mut seen);
    assert_eq!(text(no.get("error").unwrap(), "code"), "invalid");
    assert!(seen.is_empty());

    k.debug("continue", Json::object());
    let (end, _) = k.until_stop();
    assert_eq!(text(&end, "event"), "done");
    let ws = k.ok("workspace", Json::object());
    assert!(names(&ws).contains(&"r".to_string()));
}

#[test]
fn eval_in_frame_output_error_and_control_commands() {
    let mut k = start("eval", &[("nest.m", NESTED)]);
    k.bp("nest.m", 10, "");
    k.start_eval("r = nest(1);");
    k.paused();
    let mut seen = Vec::new();
    let reply = k.call(
        "debug",
        Json::object()
            .with("command", "eval")
            .with("code", "disp(value + 41)"),
        &mut seen,
    );
    assert_eq!(reply.get("ok"), Some(&Json::Bool(true)));
    assert!(seen.iter().any(|m| text(m, "text") == "42\n"), "{seen:?}");
    k.paused();
    let mut seen = Vec::new();
    k.call(
        "debug",
        Json::object()
            .with("command", "eval")
            .with("code", "no_such_thing_here"),
        &mut seen,
    );
    assert!(
        seen.iter()
            .any(|m| text(m, "stream") == "stderr" && text(m, "text").starts_with("error: ")),
        "{seen:?}"
    );
    k.paused();
    // A breakpoint does not fire inside debug eval.
    k.debug("eval", Json::object().with("code", "q = nest(2);"));
    let p = k.paused();
    assert_eq!(num(&p, "line"), 10);
    assert!(names(&p).contains(&"q".to_string()));
    // dbstep typed at the prompt is a step.
    k.debug("eval", Json::object().with("code", "dbstep"));
    let p = k.paused();
    assert_eq!(text(&p, "reason"), "step");
    assert_eq!(num(&p, "line"), 11);
    k.debug("eval", Json::object().with("code", "dbcont"));
    let (end, _) = k.until_stop();
    assert_eq!(text(&end, "event"), "done");
}

#[test]
fn stop_while_paused_is_idempotent_and_the_next_eval_is_clean() {
    let mut k = start("stop", &[("nest.m", NESTED)]);
    k.bp("nest.m", 10, "");
    k.start_eval("before = 1; r = nest(1); after = 2;");
    k.paused();
    k.sigint();
    k.sigint();
    let (end, _) = k.until_stop();
    assert_eq!(text(&end, "event"), "error");
    assert_eq!(end.get("interrupted"), Some(&Json::Bool(true)));
    assert_eq!(end.get("quit"), None);
    assert_eq!(text(&end, "message"), "Operation interrupted.");
    let ws = names(&k.ok("workspace", Json::object()));
    assert!(ws.contains(&"before".to_string()));
    assert!(!ws.contains(&"after".to_string()));
    k.ok("breakpoint", Json::object().with("action", "clear_all"));
    k.start_eval("after = 3;");
    let (end, _) = k.until_stop();
    assert_eq!(text(&end, "event"), "done");
}

#[test]
fn quit_ends_with_the_quit_flag() {
    let mut k = start("quit", &[("nest.m", NESTED)]);
    k.bp("nest.m", 10, "");
    k.start_eval("r = nest(1);");
    k.paused();
    k.debug("quit", Json::object());
    let (end, _) = k.until_stop();
    assert_eq!(text(&end, "event"), "error");
    assert_eq!(end.get("quit"), Some(&Json::Bool(true)));
    assert_eq!(end.get("interrupted"), Some(&Json::Bool(true)));
}

#[test]
fn run_to_cursor_reports_pending_and_removes_the_target() {
    let other = "function y = other(x)\n\n  y = x + 1;\nend\n";
    let mut k = start("rtc", &[("nest.m", NESTED), ("other.m", other)]);
    k.bp("nest.m", 10, "");
    k.bp("nest.m", 11, "");
    k.start_eval("r = nest(1);");
    k.paused();
    let path = k.dir.join("other.m").display().to_string();
    let target = k.ok(
        "run_to_cursor",
        Json::object()
            .with("path", path.as_str())
            .with("line", 2u64),
    );
    assert_eq!(num(&target, "actual_line"), 3);
    let p = k.paused();
    assert_eq!(num(&p, "line"), 11);
    let pending = p.get("run_to_cursor").unwrap();
    assert_eq!(text(pending, "path"), path);
    assert_eq!(num(pending, "line"), 3);
    let mut seen = Vec::new();
    let again = k.call(
        "run_to_cursor",
        Json::object()
            .with("path", path.as_str())
            .with("line", 2u64),
        &mut seen,
    );
    assert_eq!(text(again.get("error").unwrap(), "code"), "invalid");
    k.debug("continue", Json::object());
    assert_eq!(text(&k.until_stop().0, "event"), "done");
    // The target is gone after the eval.
    k.start_eval("other(1);");
    assert_eq!(text(&k.until_stop().0, "event"), "done");
}

#[test]
fn file_mode_scripts_pause_and_section_text_does_not() {
    let script = "total = 0;\nfor v = 1:3\n  total = total + v;\nend\n";
    let mut k = start("file", &[("scr.m", script)]);
    k.bp("scr.m", 3, "v == 2");
    let path = k.dir.join("scr.m").display().to_string();
    k.eval = k.next + 1;
    k.ok(
        "eval",
        Json::object()
            .with("mode", "file")
            .with("path", path.as_str()),
    );
    let p = k.paused();
    assert_eq!(text(&p, "file"), path);
    assert_eq!(stack(&p), ["scr:3"]);
    assert!(names(&p).contains(&"v".to_string()));
    k.debug("continue", Json::object());
    assert_eq!(text(&k.until_stop().0, "event"), "done");
    // The same file's text as a section has other line numbers: no pause.
    k.eval = k.next + 1;
    k.ok(
        "eval",
        Json::object()
            .with("mode", "section")
            .with("path", path.as_str())
            .with("code", "a = 1;\nb = 2;\nv = 2; c = 3;\n"),
    );
    assert_eq!(text(&k.until_stop().0, "event"), "done");
}

#[test]
fn keyboard_in_the_command_window_has_no_location_and_keeps_input_closed() {
    let mut k = start("kb", &[]);
    k.start_eval("kept = 1; keyboard; after = 2;");
    let p = k.paused();
    assert_eq!(text(&p, "reason"), "keyboard");
    assert_eq!(text(&p, "file"), "");
    assert_eq!(num(&p, "line"), 0);
    assert_eq!(num(&p, "frame"), 0);
    assert!(names(&p).contains(&"kept".to_string()));
    k.debug("continue", Json::object());
    assert_eq!(text(&k.until_stop().0, "event"), "done");
    assert!(names(&k.ok("workspace", Json::object())).contains(&"after".to_string()));
}

#[test]
fn input_during_a_pause_eval_and_during_running_code() {
    let src = "function y = asker()\n  y = input('now? ');\nend\n";
    let mut k = start("input", &[("asker.m", src)]);
    k.bp("asker.m", 2, "");
    k.start_eval("v = asker();");
    k.paused();
    k.debug("continue", Json::object());
    while text(&k.recv(), "event") != "input_request" {}
    k.ok("input_reply", Json::object().with("text", "7"));
    assert_eq!(text(&k.until_stop().0, "event"), "done");
}

#[test]
fn idle_and_running_refusals_and_breakpoint_errors() {
    let mut k = start("idle", &[("nest.m", NESTED)]);
    let mut seen = Vec::new();
    let r = k.call(
        "debug",
        Json::object().with("command", "continue"),
        &mut seen,
    );
    assert_eq!(text(r.get("error").unwrap(), "code"), "invalid");
    let r = k.call(
        "run_to_cursor",
        Json::object().with("path", "/x.m").with("line", 1u64),
        &mut seen,
    );
    assert_eq!(text(r.get("error").unwrap(), "code"), "invalid");
    let set = |p: &Path, line: u64| {
        Json::object()
            .with("action", "set")
            .with("path", p.display().to_string())
            .with("line", line)
    };
    let dir = k.dir.clone();
    let r = k.call("breakpoint", set(&dir.join("absent.m"), 1), &mut seen);
    assert_eq!(text(r.get("error").unwrap(), "code"), "not_found");
    let r = k.call("breakpoint", set(&dir.join("nest.m"), 500), &mut seen);
    assert_eq!(text(r.get("error").unwrap(), "code"), "invalid");
    k.bp("nest.m", 1, "");
    let list = k.ok("breakpoint", Json::object().with("action", "list"));
    let rows = list.get("breakpoints").unwrap().as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(num(&rows[0], "line"), 1);
    assert_eq!(num(&rows[0], "actual_line"), 2);
    let cleared = k.ok(
        "breakpoint",
        Json::object()
            .with("action", "clear")
            .with("path", dir.join("nest.m").display().to_string())
            .with("line", 1u64),
    );
    assert_eq!(cleared.get("cleared"), Some(&Json::Bool(true)));
    // While code runs everything of the debugger is busy.
    k.start_eval("pause(0.3);");
    let r = k.call(
        "debug",
        Json::object().with("command", "continue"),
        &mut seen,
    );
    assert_eq!(text(r.get("error").unwrap(), "code"), "busy");
    assert_eq!(text(&k.until_stop().0, "event"), "done");
    // Breakpoints survive reset.
    k.bp("nest.m", 10, "");
    k.ok("reset", Json::object());
    let list = k.ok("breakpoint", Json::object().with("action", "list"));
    assert_eq!(
        list.get("breakpoints").unwrap().as_array().unwrap().len(),
        1
    );
}
