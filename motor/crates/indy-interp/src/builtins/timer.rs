//! `timer` objects.
//!
//! Execution model: the engine has one thread and MATLAB runs timer
//! callbacks only when it is idle or inside `pause`, `drawnow`, `waitfor`
//! and `wait`. Here `start(t)` runs `StartFcn` and schedules the timer;
//! due callbacks then run on the engine thread while `wait(t)` or `pause(n)`
//! blocks (both poll the session interrupt every 50 ms at most). `wait`
//! returns once the timer stopped (its tasks are done, a callback called
//! `stop`, or a callback failed). A timer that is still scheduled when the
//! running code ends does not fire later: there is no idle loop.
//! `TimerFcn`, `StartFcn`, `StopFcn` and `ErrorFcn` take a function handle
//! (called with the timer and an event struct), a cell `{f, extra...}` or a
//! char command (run in the current workspace).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use indy_value::{Object, StructArray, Value};

use super::{arg_str, err, need, one, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "timer", timer_new);
    reg(it, "start", |it, a, _| {
        for t in timers(&a, "start")? {
            start(it, &t)?;
        }
        Ok(vec![])
    });
    reg(it, "stop", |it, a, _| {
        for t in timers(&a, "stop")? {
            stop(it, &t)?;
        }
        Ok(vec![])
    });
    reg(it, "wait", |it, a, _| {
        for t in timers(&a, "wait")? {
            service(it, Some(&t), None)?;
        }
        Ok(vec![])
    });
}

type Obj = Rc<RefCell<Object>>;

/// Scheduled timers: object and the time its next callback is due.
#[derive(Default)]
struct Schedule {
    epoch: Option<Instant>,
    due: Vec<(Obj, f64)>,
    count: usize,
}

impl Schedule {
    fn now(&mut self) -> f64 {
        self.epoch
            .get_or_insert_with(Instant::now)
            .elapsed()
            .as_secs_f64()
    }
}

/// True while `pause` must wait for scheduled timers (used by `pause`).
pub fn any_scheduled(it: &Interp) -> bool {
    it.ext::<Schedule>().is_some_and(|s| !s.due.is_empty())
}

const PROPS: [&str; 16] = [
    "AveragePeriod",
    "BusyMode",
    "ErrorFcn",
    "ExecutionMode",
    "InstantPeriod",
    "Name",
    "ObjectVisibility",
    "Period",
    "Running",
    "StartDelay",
    "StartFcn",
    "StopFcn",
    "Tag",
    "TasksExecuted",
    "TasksToExecute",
    "TimerFcn",
];

fn timer_new(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let n = {
        let s = it.ext_mut::<Schedule>();
        s.count += 1;
        s.count
    };
    let mut o = Object::new("timer");
    for p in PROPS {
        let v = match p {
            "AveragePeriod" | "InstantPeriod" => Value::num(f64::NAN),
            "BusyMode" => Value::str("drop"),
            "ExecutionMode" => Value::str("singleShot"),
            "Name" => Value::str(&format!("timer-{n}")),
            "ObjectVisibility" => Value::str("on"),
            "Period" => Value::num(1.0),
            "Running" => Value::str("off"),
            "StartDelay" | "TasksExecuted" => Value::num(0.0),
            "TasksToExecute" => Value::num(f64::INFINITY),
            "Tag" => Value::str(""),
            _ => Value::empty(),
        };
        o.props.push((p.to_string(), v));
    }
    o.props.push(("Type".into(), Value::str("timer")));
    o.props.push(("UserData".into(), Value::empty()));
    if !a.len().is_multiple_of(2) {
        return Err(err(
            "MATLAB:timer:invalidInput",
            "timer: properties must come in Name, Value pairs.",
        ));
    }
    for pair in a.chunks(2) {
        let name = arg_str(&pair[0], "timer")?;
        let Some(key) = o
            .props
            .iter()
            .map(|p| p.0.clone())
            .find(|k| k.eq_ignore_ascii_case(&name))
        else {
            return Err(err(
                "MATLAB:timer:invalidProperty",
                format!("timer: there is no '{name}' property."),
            ));
        };
        check_prop(&key, &pair[1])?;
        o.set(&key, pair[1].clone());
    }
    one(Value::HObject(Rc::new(RefCell::new(o))))
}

fn check_prop(key: &str, v: &Value) -> R<()> {
    let ok = match key {
        "Period" => v.scalar_f64().is_some_and(|x| x >= 0.001),
        "StartDelay" => v.scalar_f64().is_some_and(|x| x >= 0.0 && x.is_finite()),
        "TasksToExecute" => v.scalar_f64().is_some_and(|x| x >= 1.0),
        "ExecutionMode" => v.to_str().is_some_and(|s| {
            ["singleShot", "fixedRate", "fixedSpacing", "fixedDelay"]
                .iter()
                .any(|m| m.eq_ignore_ascii_case(&s))
        }),
        _ => true,
    };
    if ok {
        Ok(())
    } else {
        Err(err(
            "MATLAB:timer:invalidValue",
            format!("timer: invalid value for property '{key}'."),
        ))
    }
}

fn timers(a: &[Value], fname: &str) -> R<Vec<Obj>> {
    need(a, 1, fname)?;
    let mut out = Vec::new();
    for v in a {
        match v {
            Value::HObject(o) if o.borrow().class == "timer" && o.borrow().meta.is_none() => {
                if o.borrow().get("__deleted__").is_some() {
                    return Err(err(
                        "MATLAB:timer:invalid",
                        format!("{fname}: the timer object has been deleted."),
                    ));
                }
                out.push(o.clone());
            }
            _ => {
                return Err(err(
                    "MATLAB:timer:invalid",
                    format!("{fname}: argument must be a timer object."),
                ));
            }
        }
    }
    Ok(out)
}

fn prop(t: &Obj, name: &str) -> Value {
    t.borrow().get(name).cloned().unwrap_or_default()
}

fn num(t: &Obj, name: &str) -> f64 {
    prop(t, name).scalar_f64().unwrap_or(0.0)
}

fn mode(t: &Obj) -> String {
    prop(t, "ExecutionMode")
        .to_str()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn running(t: &Obj) -> bool {
    prop(t, "Running").to_str().as_deref() == Some("on") && t.borrow().get("__deleted__").is_none()
}

fn start(it: &mut Interp, t: &Obj) -> R<()> {
    if running(t) {
        return Err(err(
            "MATLAB:timer:alreadystarted",
            format!(
                "Timer '{}' is already running.",
                prop(t, "Name").to_str().unwrap_or_default()
            ),
        ));
    }
    {
        let mut o = t.borrow_mut();
        o.set("Running", Value::str("on"));
        o.set("TasksExecuted", Value::num(0.0));
    }
    let s = it.ext_mut::<Schedule>();
    let due = s.now() + num(t, "StartDelay");
    s.due.push((t.clone(), due));
    callback(it, t, "StartFcn")?;
    Ok(())
}

fn stop(it: &mut Interp, t: &Obj) -> R<()> {
    if !running(t) {
        return Ok(());
    }
    t.borrow_mut().set("Running", Value::str("off"));
    it.ext_mut::<Schedule>()
        .due
        .retain(|(o, _)| !Rc::ptr_eq(o, t));
    callback(it, t, "StopFcn")
}

/// `pause(secs)` while timers are scheduled.
pub fn pause_for(it: &mut Interp, secs: f64) -> R<()> {
    let until = it.ext_mut::<Schedule>().now() + secs;
    service(it, None, Some(until))
}

/// Run due callbacks until timer `wait_for` stopped, or until `until`
/// passed (`pause`).
pub fn service(it: &mut Interp, wait_for: Option<&Obj>, until: Option<f64>) -> R<()> {
    loop {
        it.poll_interrupt()?;
        // Drop timers deleted or stopped behind our back.
        it.ext_mut::<Schedule>().due.retain(|(o, _)| running(o));
        if let Some(t) = wait_for
            && !running(t)
        {
            return Ok(());
        }
        let s = it.ext_mut::<Schedule>();
        let now = s.now();
        if until.is_some_and(|u| now >= u) {
            return Ok(());
        }
        let next = s
            .due
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.1.total_cmp(&b.1.1))
            .map(|(i, (_, d))| (i, *d));
        let Some((i, due)) = next else {
            match until {
                Some(u) => std::thread::sleep(Duration::from_secs_f64((u - now).min(0.05))),
                None => return Ok(()),
            }
            continue;
        };
        if due > now {
            let mut wait = (due - now).min(0.05);
            if let Some(u) = until {
                wait = wait.min(u - now);
            }
            std::thread::sleep(Duration::from_secs_f64(wait.max(0.0)));
            continue;
        }
        let t = s.due[i].0.clone();
        fire(it, &t, due)?;
    }
}

fn fire(it: &mut Interp, t: &Obj, due: f64) -> R<()> {
    callback(it, t, "TimerFcn")?;
    let done = num(t, "TasksExecuted") + 1.0;
    t.borrow_mut().set("TasksExecuted", Value::num(done));
    if !running(t) {
        return Ok(());
    }
    let m = mode(t);
    if m == "singleshot" || done >= num(t, "TasksToExecute") {
        return stop(it, t);
    }
    let period = num(t, "Period");
    let s = it.ext_mut::<Schedule>();
    // fixedRate counts from the previous due time, fixedSpacing and
    // fixedDelay from the end of the callback.
    let next = if m == "fixedrate" {
        due + period
    } else {
        s.now() + period
    };
    if let Some(slot) = s.due.iter_mut().find(|(o, _)| Rc::ptr_eq(o, t)) {
        slot.1 = next;
    }
    Ok(())
}

/// Run callback property `which`; a failing `TimerFcn` runs `ErrorFcn`
/// and stops the timer, as MATLAB does.
fn callback(it: &mut Interp, t: &Obj, which: &str) -> R<()> {
    let cb = prop(t, which);
    if cb.is_empty() {
        return Ok(());
    }
    let r = invoke(it, t, &cb, which);
    let Err(e) = r else { return Ok(()) };
    if e.identifier == crate::INTERRUPTED_ID {
        return Err(e);
    }
    let name = prop(t, "Name").to_str().unwrap_or_default();
    it.eprint(&format!(
        "Error while evaluating {which} for timer '{name}'\n\n{}\n",
        e.message
    ));
    if which == "TimerFcn" || which == "StartFcn" {
        t.borrow_mut().set("Running", Value::str("off"));
        it.ext_mut::<Schedule>()
            .due
            .retain(|(o, _)| !Rc::ptr_eq(o, t));
        let ecb = prop(t, "ErrorFcn");
        if !ecb.is_empty() {
            let _ = invoke(it, t, &ecb, "ErrorFcn");
        }
        let _ = invoke_if_set(it, t, "StopFcn");
    }
    Ok(())
}

fn invoke_if_set(it: &mut Interp, t: &Obj, which: &str) -> R<()> {
    let cb = prop(t, which);
    if cb.is_empty() {
        return Ok(());
    }
    invoke(it, t, &cb, which)
}

fn invoke(it: &mut Interp, t: &Obj, cb: &Value, which: &str) -> R<()> {
    let mut data = StructArray::new_scalar();
    let time = it.call_named("clock", vec![], 1)?.into_iter().next();
    data.set(0, "time", time.unwrap_or_default());
    let mut ev = StructArray::new_scalar();
    ev.set(0, "Type", Value::str(which));
    ev.set(0, "Data", Value::Struct(data));
    let args = vec![Value::HObject(t.clone()), Value::Struct(ev)];
    match cb {
        Value::Func(_) => it.call_value(cb, args, 0).map(|_| ()),
        Value::Cell(c) if !c.data.is_empty() => {
            let mut args = args;
            args.extend(c.data[1..].iter().cloned());
            it.call_value(&c.data[0], args, 0).map(|_| ())
        }
        Value::Char(_) => it.run_text(&cb.to_str().unwrap_or_default()).map(|_| ()),
        _ => Err(err(
            "MATLAB:timer:invalidCallback",
            format!("{which} must be a function handle, a cell array or a command."),
        )),
    }
}
