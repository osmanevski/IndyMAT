//! The debugger core against a scripted controller: where code pauses, what
//! the stack shows and what stepping does.

use indy_interp::debug::{DebugController, PauseReason, Resume};
use indy_interp::{Interp, Value};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// One step of a script: code to run in the selected frame, then how to go on.
struct Step {
    select: usize,
    eval: &'static str,
    resume: Resume,
}

fn go(resume: Resume) -> Step {
    Step {
        select: 0,
        eval: "",
        resume,
    }
}

#[derive(Default, Clone)]
struct Log {
    /// `reason name:line` of every pause, newest frame first in `name`.
    pauses: Rc<RefCell<Vec<String>>>,
    stacks: Rc<RefCell<Vec<Vec<String>>>>,
    /// Variables of the selected frame at each pause.
    vars: Rc<RefCell<Vec<Vec<String>>>>,
}

struct Scripted {
    steps: std::vec::IntoIter<Step>,
    log: Log,
}

impl DebugController for Scripted {
    fn pause(&mut self, it: &mut Interp, reason: PauseReason) -> Resume {
        let stack = it.debug_stack();
        let top = stack
            .first()
            .map(|e| format!("{}:{}", e.name, e.line))
            .unwrap_or_default();
        self.log
            .pauses
            .borrow_mut()
            .push(format!("{} {}", reason.name(), top));
        self.log.stacks.borrow_mut().push(
            stack
                .iter()
                .map(|e| format!("{}:{}", e.name, e.line))
                .collect(),
        );
        let Some(step) = self.steps.next() else {
            return Resume::Continue;
        };
        if step.select > 0 {
            assert!(it.debug_select(step.select));
        }
        self.log.vars.borrow_mut().push(
            it.debug_variables()
                .into_iter()
                .map(|(n, _, _)| n)
                .collect(),
        );
        if !step.eval.is_empty()
            && let Some(r) = it.debug_eval(step.eval)
        {
            return r;
        }
        step.resume
    }
}

struct Run {
    it: Interp,
    dir: PathBuf,
    log: Log,
}

fn setup(test: &str, files: &[(&str, &str)]) -> Run {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("debug-{test}"));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, src) in files {
        std::fs::write(dir.join(name), src).unwrap();
    }
    let mut it = Interp::new();
    it.out = Box::new(std::io::sink());
    it.set_cwd(dir.canonicalize().unwrap());
    Run {
        it,
        dir: dir.canonicalize().unwrap(),
        log: Log::default(),
    }
}

impl Run {
    fn bp(&mut self, file: &str, line: usize, cond: &str) -> usize {
        self.it
            .set_breakpoint(&self.dir.join(file), line, cond, true)
            .unwrap()
    }

    fn run(&mut self, code: &str, steps: Vec<Step>) -> Result<(), String> {
        self.it.debug_begin();
        self.it.set_debug_controller(Some(Box::new(Scripted {
            steps: steps.into_iter(),
            log: self.log.clone(),
        })));
        let r = self.it.run_text(code).map(|_| ());
        self.it.set_debug_controller(None);
        self.it.debug_end();
        self.it.unwind_to_base();
        self.it.interrupt = false;
        r.map_err(|e| e.message.clone())
    }

    fn pauses(&self) -> Vec<String> {
        self.log.pauses.borrow().clone()
    }

    fn num(&self, name: &str) -> f64 {
        self.it.get_var(name).and_then(Value::scalar_f64).unwrap()
    }
}

const NESTED: &str = "function out = nest(seed)\n  outer = seed + 100;\n  out = middle(seed);\nend\nfunction mid = middle(value)\n  mlocal = value + 10;\n  mid = inner(value);\nend\nfunction answer = inner(value)\n  ilocal = value + 1;\n  answer = ilocal * 2;\nend\n";

#[test]
fn stack_is_newest_first_and_frame_selection_changes_variables() {
    let mut r = setup("stack", &[("nest.m", NESTED)]);
    assert_eq!(r.bp("nest.m", 11, ""), 11);
    r.run(
        "x = nest(4);",
        vec![Step {
            select: 1,
            eval: "seen = mlocal; mlocal = 1000;",
            resume: Resume::Continue,
        }],
    )
    .unwrap();
    assert_eq!(r.pauses(), ["breakpoint inner:11"]);
    assert_eq!(r.log.stacks.borrow()[0], ["inner:11", "middle:7", "nest:3"]);
    let vars = &r.log.vars.borrow()[0];
    assert!(vars.contains(&"mlocal".to_string()));
    assert!(!vars.contains(&"ilocal".to_string()));
    assert_eq!(r.num("x"), 10.0);
}

#[test]
fn eval_in_a_frame_changes_that_frame() {
    let src = "function y = chg()\n  a = 1;\n  b = a;\n  y = b;\nend\n";
    let mut r = setup("chg", &[("chg.m", src)]);
    r.bp("chg.m", 3, "");
    r.run(
        "v = chg();",
        vec![Step {
            select: 0,
            eval: "a = 42;",
            resume: Resume::Continue,
        }],
    )
    .unwrap();
    assert_eq!(r.num("v"), 42.0);
}

#[test]
fn breakpoints_snap_forward_and_past_the_end_fail() {
    let src = "% comment\n\nfunction y = snapf(x)\n  % note\n\n  y = x + 1;\nend\n";
    let mut r = setup("snap", &[("snapf.m", src)]);
    assert_eq!(r.bp("snapf.m", 1, ""), 6);
    assert_eq!(r.bp("snapf.m", 4, ""), 6);
    let e =
        r.it.set_breakpoint(&r.dir.join("snapf.m"), 7, "", true)
            .unwrap_err();
    assert!(e.message.contains("No executable code"), "{}", e.message);
    r.run("snapf(1);", vec![]).unwrap();
    assert_eq!(r.pauses(), ["breakpoint snapf:6"]);
    assert!(
        r.it.set_breakpoint(&r.dir.join("absent.m"), 1, "", true)
            .is_err()
    );
}

#[test]
fn step_over_in_and_out() {
    let src = "function y = stp(x)\n  a = x;\n  b = helper(a);\n  y = b;\nend\nfunction r = helper(v)\n  r = v;\n  r = r + 1;\nend\n";
    let mut r = setup("step", &[("stp.m", src)]);
    r.bp("stp.m", 2, "");
    r.run(
        "stp(1);",
        vec![
            go(Resume::Step),
            go(Resume::Step),
            go(Resume::Step),
            go(Resume::Continue),
        ],
    )
    .unwrap();
    assert_eq!(
        r.pauses(),
        ["breakpoint stp:2", "step stp:3", "step stp:4",]
    );

    let mut r = setup("step2", &[("stp.m", src)]);
    r.bp("stp.m", 3, "");
    r.run(
        "stp(1);",
        vec![
            go(Resume::StepIn),
            go(Resume::StepOut),
            go(Resume::Continue),
        ],
    )
    .unwrap();
    assert_eq!(
        r.pauses(),
        ["breakpoint stp:3", "step helper:7", "step stp:4"]
    );
}

#[test]
fn step_goes_round_a_one_line_body_and_several_statements_pause_once() {
    let src = "function y = lp()\n  y = 0;\n  for k = 1:3\n    y = y + k; z = y;\n  end\nend\n";
    let mut r = setup("loop", &[("lp.m", src)]);
    r.bp("lp.m", 4, "");
    r.run("lp();", vec![]).unwrap();
    // No script: every pause continues; once per iteration, not per statement.
    assert_eq!(r.pauses().len(), 3);
    let mut r = setup("loop2", &[("lp.m", src)]);
    r.bp("lp.m", 4, "k == 2");
    r.run("lp();", vec![]).unwrap();
    assert_eq!(r.pauses(), ["breakpoint lp:4"]);
}

#[test]
fn loop_headers_pause_on_every_iteration() {
    let src = "function y = hd()\n  y = 0;\n  for k = 1:3\n    y = y + k;\n  end\n  w = 0;\n  while w < 2\n    w = w + 1;\n  end\nend\n";
    let mut r = setup("head", &[("hd.m", src)]);
    r.bp("hd.m", 3, "");
    r.bp("hd.m", 7, "");
    r.run("v = hd();", vec![]).unwrap();
    assert_eq!(
        r.pauses(),
        [
            "breakpoint hd:3",
            "breakpoint hd:3",
            "breakpoint hd:3",
            "breakpoint hd:7",
            "breakpoint hd:7",
            "breakpoint hd:7",
        ]
    );
    assert_eq!(r.num("v"), 6.0);
    let mut r = setup("head2", &[("hd.m", src)]);
    r.bp("hd.m", 4, "k == 1");
    r.run(
        "hd();",
        vec![go(Resume::Step), go(Resume::Step), go(Resume::Continue)],
    )
    .unwrap();
    assert_eq!(r.pauses(), ["breakpoint hd:4", "step hd:3", "step hd:4"]);
}

#[test]
fn nested_parent_frame_sees_and_changes_shared_variables() {
    let src = "function y = nsh()\n  shared = 1;\n  child();\n  y = shared;\n  function child()\n    shared = shared + 1;\n    shared = shared + 1;\n  end\nend\n";
    let mut r = setup("nsh", &[("nsh.m", src)]);
    r.bp("nsh.m", 7, "");
    r.run(
        "v = nsh();",
        vec![Step {
            select: 1,
            eval: "shared = 10;",
            resume: Resume::Continue,
        }],
    )
    .unwrap();
    assert!(r.log.vars.borrow()[0].contains(&"shared".to_string()));
    assert_eq!(r.num("v"), 11.0);
}

#[test]
fn dbstack_shows_the_paused_stack_from_the_selected_frame_too() {
    let mut r = setup("dbs", &[("nest.m", NESTED)]);
    r.bp("nest.m", 11, "");
    r.run(
        "nest(1);",
        vec![Step {
            select: 1,
            eval: "[s, i] = dbstack(); assignin('base', 'n', numel(s)); assignin('base', 'i', i);",
            resume: Resume::Continue,
        }],
    )
    .unwrap();
    assert_eq!(r.num("n"), 3.0);
    assert_eq!(r.num("i"), 2.0);
}

#[test]
fn condition_error_pauses_and_reports() {
    let src = "function y = cnd(x)\n  y = x;\nend\n";
    let mut r = setup("cond", &[("cnd.m", src)]);
    r.bp("cnd.m", 2, "missing_name > 0");
    r.run("cnd(1);", vec![]).unwrap();
    assert_eq!(r.pauses(), ["breakpoint cnd:2"]);
}

#[test]
fn disabled_breakpoint_does_not_pause_and_clear_removes() {
    let src = "function y = dis(x)\n  y = x;\nend\n";
    let mut r = setup("dis", &[("dis.m", src)]);
    let path = r.dir.join("dis.m");
    r.it.set_breakpoint(&path, 2, "", false).unwrap();
    r.run("dis(1);", vec![]).unwrap();
    assert!(r.pauses().is_empty());
    r.it.set_breakpoint(&path, 2, "", true).unwrap();
    assert!(r.it.clear_breakpoint(&path, 2));
    assert!(!r.it.clear_breakpoint(&path, 2));
    r.run("dis(1);", vec![]).unwrap();
    assert!(r.pauses().is_empty());
}

#[test]
fn run_to_cursor_is_temporary_and_stays_pending_past_another_stop() {
    let src = "function y = rtc()\n  a = 1;\n  b = 2;\n  c = 3;\n  y = a + b + c;\nend\n";
    struct Rtc {
        path: PathBuf,
        target: usize,
        log: Log,
    }
    impl DebugController for Rtc {
        fn pause(&mut self, it: &mut Interp, reason: PauseReason) -> Resume {
            let e = &it.debug_stack()[0];
            self.log
                .pauses
                .borrow_mut()
                .push(format!("{} {}:{}", reason.name(), e.name, e.line));
            if reason == PauseReason::Breakpoint && e.line == 2 {
                assert_eq!(
                    it.debug_run_to(&self.path, self.target).unwrap(),
                    self.target
                );
                assert!(it.debug_run_to(&self.path, self.target).is_err());
            }
            if reason == PauseReason::Breakpoint && e.line == 3 {
                assert_eq!(it.debug_run_to_target().unwrap().1, self.target);
            }
            Resume::Continue
        }
    }
    let mut r = setup("rtc", &[("rtc.m", src)]);
    let path = r.dir.join("rtc.m");
    r.bp("rtc.m", 2, "");
    r.bp("rtc.m", 3, "");
    r.it.debug_begin();
    r.it.set_debug_controller(Some(Box::new(Rtc {
        path,
        target: 4,
        log: r.log.clone(),
    })));
    r.it.run_text("rtc(); rtc();").unwrap();
    assert_eq!(
        r.pauses(),
        [
            "breakpoint rtc:2",
            "breakpoint rtc:3",
            "run_to_cursor rtc:4",
            "breakpoint rtc:2",
            "breakpoint rtc:3",
            "run_to_cursor rtc:4",
        ]
    );
}

#[test]
fn dbstop_if_error_pauses_at_the_error_outside_try_only() {
    let src = "function y = boom(x)\n  y = x;\n  error('kaboom');\nend\n";
    let wrapped =
        "function y = guard()\n  try\n    y = boom(1);\n  catch\n    y = -1;\n  end\nend\n";
    let mut r = setup("err", &[("boom.m", src), ("guard.m", wrapped)]);
    r.run("dbstop if error", vec![]).unwrap();
    assert!(r.it.stop_on_error());
    assert_eq!(
        r.run("boom(1);", vec![go(Resume::Continue)]).unwrap_err(),
        "kaboom"
    );
    assert_eq!(r.pauses(), ["error boom:3"]);
    r.run("v = guard();", vec![]).unwrap();
    assert_eq!(r.num("v"), -1.0);
    assert_eq!(r.pauses().len(), 1);
    r.run("dbclear if error", vec![]).unwrap();
    assert!(!r.it.stop_on_error());
}

#[test]
fn quit_and_stop_are_not_catchable() {
    let src = "function y = tr()\n  y = 0;\n  try\n    y = 1;\n  catch\n    y = 2;\n  end\n  y = 3;\nend\n";
    let mut r = setup("quit", &[("tr.m", src)]);
    r.bp("tr.m", 4, "");
    let e = r.run("tr();", vec![go(Resume::Quit)]).unwrap_err();
    assert_eq!(e, "Operation interrupted.");
    assert_eq!(r.pauses().len(), 1);
}

#[test]
fn keyboard_pauses_with_a_controller_and_does_nothing_without() {
    let src = "function y = kb()\n  y = 1;\n  keyboard;\n  y = 2;\nend\n";
    let mut r = setup("kb", &[("kb.m", src)]);
    r.run("v = kb();", vec![go(Resume::Step), go(Resume::Continue)])
        .unwrap();
    assert_eq!(r.pauses(), ["keyboard kb:3", "step kb:4"]);
    assert_eq!(r.num("v"), 2.0);
    r.it.run_text("w = kb();").unwrap();
    assert_eq!(r.num("w"), 2.0);
}

#[test]
fn scripts_are_stack_entries_and_step_over_skips_them() {
    let inner = "ia = 1;\nib = 2;\n";
    let outer = "oa = 1;\ninnerscr;\noc = 3;\n";
    let mut r = setup("script", &[("innerscr.m", inner), ("outerscr.m", outer)]);
    r.bp("outerscr.m", 2, "");
    r.run("outerscr", vec![go(Resume::Step), go(Resume::Continue)])
        .unwrap();
    assert_eq!(r.pauses(), ["breakpoint outerscr:2", "step outerscr:3"]);
    let mut r = setup("script2", &[("innerscr.m", inner), ("outerscr.m", outer)]);
    r.bp("innerscr.m", 2, "");
    r.run("outerscr", vec![]).unwrap();
    assert_eq!(r.log.stacks.borrow()[0], ["innerscr:2", "outerscr:2"]);
}

#[test]
fn command_text_and_eval_never_pause_on_their_own_lines() {
    let mut r = setup("text", &[]);
    r.run("eval('x = 1;'); y = 2;", vec![]).unwrap();
    assert!(r.pauses().is_empty());
    // A keyboard in text has no source location.
    r.run("keyboard; z = 3;", vec![go(Resume::Step)]).unwrap();
    assert_eq!(r.pauses(), ["keyboard "]);
    assert_eq!(r.num("z"), 3.0);
}

#[test]
fn dbstop_builtin_forms_and_dbstatus() {
    let src =
        "function y = bif(x)\n  y = x;\n  y = y + 1;\nend\nfunction z = sub1()\n  z = 1;\nend\n";
    let mut r = setup("bif", &[("bif.m", src)]);
    r.run("dbstop in bif", vec![]).unwrap();
    r.run("s = dbstatus(); n = numel(s); l = s.line;", vec![])
        .unwrap();
    assert_eq!(r.num("n"), 1.0);
    assert_eq!(r.num("l"), 2.0);
    r.run("dbclear all; dbstop('bif', 3); dbstop bif 2 if x>5", vec![])
        .unwrap();
    r.run("bif(1);", vec![go(Resume::Continue)]).unwrap();
    assert_eq!(r.pauses(), ["breakpoint bif:3"]);
    r.run("dbclear in bif at 3; bif(1);", vec![]).unwrap();
    assert_eq!(r.pauses().len(), 1);
    r.run("bif(9);", vec![]).unwrap();
    assert_eq!(r.pauses().len(), 2);
    r.run("dbclear bif; bif(9); dbstop in bif>sub1", vec![])
        .unwrap();
    assert_eq!(r.pauses().len(), 2);
    r.run("s = dbstatus(); l = s.line;", vec![]).unwrap();
    assert_eq!(r.num("l"), 6.0);
    for bad in ["dbcont", "dbquit", "dbstep", "dbup", "dbdown"] {
        let e = r.run(bad, vec![]).unwrap_err();
        assert!(e.contains("only valid while"), "{e}");
    }
    assert!(r.run("dbstop in no_such_function_here", vec![]).is_err());
}

#[test]
fn control_builtins_in_debug_eval_resume() {
    let src = "function y = ctl(x)\n  y = x;\n  y = y + 1;\n  y = y + 2;\nend\n";
    let mut r = setup("ctl", &[("ctl.m", src)]);
    r.bp("ctl.m", 2, "");
    r.run(
        "v = ctl(1);",
        vec![
            Step {
                select: 0,
                eval: "dbstep",
                resume: Resume::Quit,
            },
            Step {
                select: 0,
                eval: "dbcont",
                resume: Resume::Quit,
            },
        ],
    )
    .unwrap();
    assert_eq!(r.pauses(), ["breakpoint ctl:2", "step ctl:3"]);
    assert_eq!(r.num("v"), 4.0);
    let e = r
        .run(
            "ctl(1);",
            vec![Step {
                select: 0,
                eval: "dbquit",
                resume: Resume::Continue,
            }],
        )
        .unwrap_err();
    assert_eq!(e, "Operation interrupted.");
}

#[test]
fn an_edited_file_is_snapped_against_its_new_text() {
    let mut r = setup("edit", &[("edt.m", "function edt()\n  a = 1;\nend\n")]);
    r.bp("edt.m", 2, "");
    std::fs::write(
        r.dir.join("edt.m"),
        "function edt()\n  % new comment\n  % another\n  a = 1;\nend\n",
    )
    .unwrap();
    r.it.clear_file_cache();
    r.run("edt();", vec![]).unwrap();
    assert_eq!(r.pauses(), ["breakpoint edt:4"]);
}

#[test]
fn classdef_methods_pause_with_their_file_lines() {
    let src = "classdef dbgcls\n  methods\n    function r = twice(o, v)\n      r = v * 2;\n    end\n  end\nend\n";
    let mut r = setup("cls", &[("dbgcls.m", src)]);
    assert_eq!(r.bp("dbgcls.m", 4, ""), 4);
    r.run("o = dbgcls(); v = o.twice(4);", vec![]).unwrap();
    assert_eq!(r.pauses().len(), 1);
    assert!(r.pauses()[0].ends_with(":4"), "{:?}", r.pauses());
    assert_eq!(r.num("v"), 8.0);
}

#[test]
fn step_over_a_recursive_call_stays_at_the_same_depth() {
    let src = "function y = rec(n)\n  if n > 0\n    y = rec(n - 1) + 1;\n    y = y + 0;\n  else\n    y = 0;\n  end\nend\n";
    let mut r = setup("rec", &[("rec.m", src)]);
    r.bp("rec.m", 3, "n == 2");
    r.run("v = rec(2);", vec![go(Resume::Step), go(Resume::Continue)])
        .unwrap();
    assert_eq!(r.pauses(), ["breakpoint rec:3", "step rec:4"]);
    assert_eq!(r.log.stacks.borrow()[1].len(), 1);
    assert_eq!(r.num("v"), 2.0);
}

#[test]
fn callbacks_and_anonymous_functions_keep_a_clean_stack() {
    let src = "function y = cbk(x)\n  y = x + 1;\nend\n";
    let top = "function v = cbtop()\n  f = @(q) cbk(q);\n  v = cellfun(f, {1, 2});\nend\n";
    let mut r = setup("cb", &[("cbk.m", src), ("cbtop.m", top)]);
    r.bp("cbk.m", 2, "");
    r.run("w = cbtop();", vec![]).unwrap();
    assert_eq!(r.pauses().len(), 2);
    assert_eq!(r.log.stacks.borrow()[0], ["cbk:2", "cbtop:3"]);
}

#[test]
fn pause_inside_evalc_keeps_the_capture_and_prompt_output_apart() {
    let src = "function eco()\n  disp('kept');\n  disp('more');\nend\n";
    let mut r = setup("evc", &[("eco.m", src)]);
    r.bp("eco.m", 3, "");
    r.run(
        "t = evalc('eco()');",
        vec![Step {
            select: 0,
            eval: "disp('prompt');",
            resume: Resume::Continue,
        }],
    )
    .unwrap();
    let t = r.it.get_var("t").and_then(Value::to_str).unwrap();
    assert!(t.contains("kept") && t.contains("more"), "{t}");
    assert!(!t.contains("prompt"), "{t}");
}

#[test]
fn eval_with_a_catch_text_counts_as_try_for_dbstop_if_error() {
    let src = "function y = ec()\n  eval('error(\"inner\")', 'y = 5;');\nend\n";
    let mut r = setup("evcatch", &[("ec.m", src)]);
    r.run("dbstop if error", vec![]).unwrap();
    r.run("v = ec();", vec![]).unwrap();
    assert!(r.pauses().is_empty(), "{:?}", r.pauses());
    assert_eq!(r.num("v"), 5.0);
}

#[test]
fn nothing_pauses_without_a_controller() {
    let src = "function y = nc()\n  keyboard;\n  y = 1;\nend\n";
    let mut r = setup("nc", &[("nc.m", src)]);
    r.bp("nc.m", 3, "");
    r.it.run_text("dbstop if error; v = nc();").unwrap();
    assert_eq!(r.num("v"), 1.0);
    let e = r.it.run_text("error('plain');").unwrap_err();
    assert_eq!(e.message, "plain");
}
