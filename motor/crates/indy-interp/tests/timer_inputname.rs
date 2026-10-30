//! `inputname` and `timer`.

use indy_interp::Interp;
use std::path::PathBuf;

fn run_with(files: &[(&str, &str)], code: &str) -> Interp {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("timer-inputname");
    std::fs::create_dir_all(&dir).unwrap();
    for (n, s) in files {
        std::fs::write(dir.join(format!("{n}.m")), s).unwrap();
    }
    let mut it = Interp::new();
    it.add_path(dir, true);
    if let Err(e) = it.run_text(code) {
        panic!("snippet failed: {}\n---\n{}", e.message, code);
    }
    it
}

const NAMES: (&str, &str) = (
    "tin_names",
    "function s = tin_names(varargin)
  s = '';
  for k = 1:nargin
    s = [s '[' inputname(k) ']'];
  end
end
",
);

#[test]
fn inputname_variables_and_expressions() {
    run_with(
        &[NAMES],
        "x = 4; y = 5; c = {1, 2};
         assert(strcmp(tin_names(x), '[x]'));
         assert(strcmp(tin_names(x + 1), '[]'));
         assert(strcmp(tin_names(x, 3, y), '[x][][y]'));
         % after a cs-list the positions are unknown
         assert(strcmp(tin_names(y, c{:}), '[y][][]'));
         f = @tin_names; assert(strcmp(f(y), '[y]'));
         try
           inputname(1); ok = false;
         catch
           ok = true;
         end
         assert(ok);",
    );
}

#[test]
fn timer_single_shot_and_fixed_rate() {
    run_with(
        &[],
        "t = timer('TimerFcn', @(varargin) 0, 'StartDelay', 0.01);
         start(t); wait(t); assert(strcmp(t.Running, 'off') && t.TasksExecuted == 1); delete(t);
         assert(~isvalid(t));
         global tn; tn = 0;
         t = timer('TimerFcn', 'global tn; tn = tn + 1;', 'ExecutionMode', 'fixedRate', ...
                   'Period', 0.01, 'TasksToExecute', 3);
         start(t); assert(strcmp(t.Running, 'on')); wait(t);
         assert(tn == 3 && t.TasksExecuted == 3 && strcmp(t.Running, 'off'));",
    );
}

#[test]
fn timer_stop_pause_and_errors() {
    run_with(
        &[],
        "global seen; seen = {};
         t = timer('ExecutionMode', 'fixedSpacing', 'Period', 0.01, ...
                   'TimerFcn', @(tt, ev) 0, ...
                   'StopFcn', 'global seen; seen{end+1} = ''stop'';');
         start(t); pause(0.05); assert(t.TasksExecuted >= 1); stop(t);
         assert(strcmp(t.Running, 'off') && isequal(seen, {'stop'}));
         t = timer('TimerFcn', @(tt, ev) stop(tt), 'ExecutionMode', 'fixedRate', 'Period', 0.01);
         start(t); wait(t); assert(t.TasksExecuted == 1);
         t = timer('TimerFcn', @(tt, ev) error('x:y', 'boom'), ...
                   'ErrorFcn', 'global seen; seen{end+1} = ''err'';');
         start(t); wait(t);
         assert(strcmp(t.Running, 'off') && isequal(seen, {'stop', 'err'}));
         t = timer('TimerFcn', {@(tt, ev, a) assert(strcmp(ev.Type, 'TimerFcn') && a == 7), 7});
         start(t); wait(t); assert(t.TasksExecuted == 1);",
    );
}
