//! Statement execution and expression evaluation.

use std::borrow::Cow;
use std::rc::Rc;

use indy_syntax::ast::{self, BinOp, Expr, ExprKind, LValue, Stmt, StmtKind};
use indy_value::index::{Sub, index_value, read_positions, to_sub};
use indy_value::{Array, Error, Num, NumClass, Value, concat, format, ops};

use crate::builtins::datatypes;

use crate::assign::assign_path;
use crate::{Flow, Interp, R, exception_value};

/// Unevaluated postfix operation.
#[derive(Clone, Copy)]
pub(crate) enum Op<'a> {
    Paren(&'a [Expr]),
    Brace(&'a [Expr]),
    Field(&'a str),
    Dyn(&'a Expr),
}

/// Postfix operation with evaluated arguments.
#[derive(Clone, Debug)]
pub enum EOp {
    Paren(Vec<Value>),
    Brace(Vec<Value>),
    Field(String),
}

/// Split `a.b(1){2}` into base `a` and its operations.
pub(crate) fn flatten(e: &Expr) -> (&Expr, Vec<Op<'_>>) {
    let mut ops = Vec::new();
    let mut cur = e;
    loop {
        match &cur.kind {
            ExprKind::Index { target, args } => {
                ops.push(Op::Paren(args));
                cur = target;
            }
            ExprKind::CellIndex { target, args } => {
                ops.push(Op::Brace(args));
                cur = target;
            }
            ExprKind::Field { target, name } => {
                ops.push(Op::Field(&name.name));
                cur = target;
            }
            ExprKind::DynField { target, name } => {
                ops.push(Op::Dyn(name));
                cur = target;
            }
            _ => break,
        }
    }
    ops.reverse();
    (cur, ops)
}

pub(crate) enum Got<'v> {
    V(Cow<'v, Value>),
    List(Vec<Value>),
    /// `ops[at]` must call the (owned) value: a handle or an object method.
    Call(Value, usize),
}

pub fn subs_of(args: &[Value]) -> R<Vec<Sub>> {
    args.iter().map(to_sub).collect()
}

fn brace_error(v: &Value) -> Box<Error> {
    Error::new(
        "MATLAB:cellRefFromNonCell",
        format!(
            "Brace indexing is not supported for variables of this type ({}).",
            v.class_name()
        ),
    )
}

fn dot_error(v: &Value) -> Box<Error> {
    Error::new(
        "MATLAB:structRefFromNonStruct",
        format!(
            "Dot indexing is not supported for variables of this type ({}).",
            v.class_name()
        ),
    )
}

pub fn no_field(name: &str) -> Box<Error> {
    Error::new(
        "MATLAB:nonExistentField",
        format!("Unrecognized field name \"{}\".", name),
    )
}

fn cs_list_error(n: usize) -> Box<Error> {
    Error::new(
        "MATLAB:index:expected_one_output_from_expression",
        format!(
            "Expected one output from a curly brace or dot indexing expression, but there were {} results.",
            n
        ),
    )
}

/// Walk evaluated operations by reference where possible.
pub(crate) fn read_path<'v>(root: &'v Value, ops: &[EOp]) -> R<Got<'v>> {
    let mut cur: Cow<'v, Value> = Cow::Borrowed(root);
    for (i, op) in ops.iter().enumerate() {
        let last = i + 1 == ops.len();
        if matches!(cur.as_ref(), Value::HObject(_))
            || crate::classes::is_class_obj(&cur)
            || crate::builtins::control::is_lti(&cur)
            || datatypes::is_native(&cur)
        {
            return Ok(Got::Call(cur.into_owned(), i));
        }
        match op {
            EOp::Paren(args) => {
                if matches!(cur.as_ref(), Value::Func(_) | Value::Object(_)) {
                    return Ok(Got::Call(cur.into_owned(), i));
                }
                let subs = subs_of(args)?;
                cur = Cow::Owned(index_value(&cur, &subs)?);
            }
            EOp::Brace(args) => {
                let subs = subs_of(args)?;
                if let Value::Str(s) = cur.as_ref() {
                    let (pos, _) = read_positions(&s.dims, &subs)?;
                    let list: Vec<Value> = pos
                        .iter()
                        .map(|&p| Value::str(s.data[p].as_deref().unwrap_or("")))
                        .collect();
                    if list.len() == 1 {
                        cur = Cow::Owned(list.into_iter().next().unwrap());
                        continue;
                    }
                    if last {
                        return Ok(Got::List(list));
                    }
                    return Err(cs_list_error(list.len()));
                }
                let Value::Cell(c) = cur.as_ref() else {
                    return Err(brace_error(&cur));
                };
                let (pos, _) = read_positions(&c.dims, &subs)?;
                if pos.len() == 1 {
                    let p = pos[0];
                    cur = match cur {
                        Cow::Borrowed(Value::Cell(c)) => Cow::Borrowed(&c.data[p]),
                        Cow::Owned(Value::Cell(mut c)) => {
                            Cow::Owned(std::mem::take(&mut c.data[p]))
                        }
                        _ => unreachable!(),
                    };
                } else {
                    let list: Vec<Value> = pos.iter().map(|&p| c.data[p].clone()).collect();
                    if last {
                        return Ok(Got::List(list));
                    }
                    return Err(cs_list_error(list.len()));
                }
            }
            EOp::Field(name) => match cur.as_ref() {
                Value::Struct(s) => {
                    let Some(f) = s.field_index(name) else {
                        return Err(no_field(name));
                    };
                    if s.numel() == 1 {
                        cur = match cur {
                            Cow::Borrowed(Value::Struct(s)) => Cow::Borrowed(&s.elems[0][f]),
                            Cow::Owned(Value::Struct(mut s)) => {
                                Cow::Owned(std::mem::take(&mut s.elems[0][f]))
                            }
                            _ => unreachable!(),
                        };
                    } else {
                        let list: Vec<Value> = s.elems.iter().map(|e| e[f].clone()).collect();
                        if last {
                            return Ok(Got::List(list));
                        }
                        return Err(cs_list_error(list.len()));
                    }
                }
                Value::Object(o) => {
                    if o.get(name).is_some() {
                        cur = match cur {
                            Cow::Borrowed(Value::Object(o)) => Cow::Borrowed(o.get(name).unwrap()),
                            Cow::Owned(Value::Object(o)) => {
                                Cow::Owned(o.get(name).unwrap().clone())
                            }
                            _ => unreachable!(),
                        };
                    } else {
                        return Ok(Got::Call(cur.into_owned(), i));
                    }
                }
                other => return Err(dot_error(other)),
            },
        }
    }
    Ok(Got::V(cur))
}

/// Ranges at least this long are stepped lazily in `for` loops.
const LAZY_RANGE: usize = 1 << 20;

/// Length of `start:step:stop` when it is long enough to step lazily; the
/// same arithmetic as `ops::range`, which handles every other case.
fn lazy_range_len(start: f64, step: f64, stop: f64) -> Option<usize> {
    if !(start.is_finite() && step.is_finite() && stop.is_finite()) || step == 0.0 {
        return None;
    }
    if (step > 0.0 && start > stop) || (step < 0.0 && start < stop) {
        return None;
    }
    let q = (stop - start) / step;
    let tol = 3.0 * f64::EPSILON * q.abs().max(1.0);
    let n = (q + tol).floor() as usize + 1;
    (LAZY_RANGE..=2_000_000_000).contains(&n).then_some(n)
}

/// Element `k` of a lazily stepped range of `n` elements (as `ops::range`).
fn lazy_range_at(start: f64, step: f64, stop: f64, k: usize, n: usize) -> f64 {
    let v = start + (k as f64) * step;
    if k + 1 == n {
        let q = (stop - start) / step;
        let tol = 3.0 * f64::EPSILON * q.abs().max(1.0);
        if (v - stop).abs() <= tol * step.abs() {
            return stop;
        }
    }
    v
}

fn no_value() -> Box<Error> {
    Error::new(
        "MATLAB:needMoreRhsOutputs",
        "Insufficient number of outputs from right hand side of equal sign to satisfy assignment (expression produced no value).",
    )
}

fn single(v: Vec<Value>) -> R<Value> {
    v.into_iter().next().ok_or_else(no_value)
}

impl Interp {
    // ===================================================================
    // Statements
    // ===================================================================

    pub fn exec_block(&mut self, b: &[Stmt]) -> R<Flow> {
        for s in b {
            match self.exec_stmt(s)? {
                Flow::Normal => {}
                f => return Ok(f),
            }
        }
        Ok(Flow::Normal)
    }

    fn show(&mut self, name: &str) {
        if let Some(v) = self.get_var(name).cloned() {
            if crate::classes::is_class_obj(&v) && matches!(self.show_object(name, &v), Ok(true)) {
                return;
            }
            if crate::builtins::control::is_lti(&v) {
                let t = crate::builtins::control::display_text(name, &v);
                self.print(&t);
                return;
            }
            if datatypes::is_native(&v) {
                let t = datatypes::display_text(name, &v);
                self.print(&t);
                return;
            }
            let t = format::display_text(name, &v);
            self.print(&t);
        }
    }

    #[inline]
    fn exec_stmt(&mut self, s: &Stmt) -> R<Flow> {
        self.frame_mut().pos = s.span.start;
        self.check_interrupt()?;
        if self.debug.active {
            self.debug_hook()?;
        }
        let r = self.exec_stmt_kind(s);
        if r.is_err() && self.debug.stop_on_error {
            return self.debug_error(r);
        }
        r
    }

    fn exec_stmt_kind(&mut self, s: &Stmt) -> R<Flow> {
        match &s.kind {
            StmtKind::Expr(e) => {
                if let ExprKind::Ident(name) = &e.kind
                    && self.has_var(name)
                {
                    if s.display {
                        self.show(name);
                    }
                    return Ok(Flow::Normal);
                }
                let vals = self.eval_multi(e, 0)?;
                if let Some(v) = vals.into_iter().next() {
                    self.set_var("ans", v);
                    if s.display {
                        self.show("ans");
                    }
                }
            }
            StmtKind::Assign { lhs, rhs, .. } => {
                if lhs.len() == 1 {
                    let LValue::Expr(t) = &lhs[0] else {
                        // `[~] = f()` evaluates and discards.
                        self.eval_multi(rhs, 1)?;
                        return Ok(Flow::Normal);
                    };
                    if let Some(n) = self.cs_targets(t)?
                        && n != 1
                    {
                        return self.multi_assign(lhs, rhs, s.display);
                    }
                    let v = self.eval(rhs)?;
                    self.assign_to(t, v)?;
                    if s.display {
                        self.show(&base_name(t));
                    }
                } else {
                    return self.multi_assign(lhs, rhs, s.display);
                }
            }
            StmtKind::OpAssign { lhs, op, rhs } => {
                let a = self.eval(lhs)?;
                let b = self.eval(rhs)?;
                let v = ops::binary(*op, &a, &b)?;
                self.assign_to(lhs, v)?;
                if s.display {
                    self.show(&base_name(lhs));
                }
            }
            StmtKind::IncDec { lhs, op } => {
                let a = self.eval(lhs)?;
                let v = ops::binary(*op, &a, &Value::num(1.0))?;
                self.assign_to(lhs, v)?;
                if s.display {
                    self.show(&base_name(lhs));
                }
            }
            StmtKind::Command { name, args } => {
                let args: Vec<Value> = args.iter().map(|a| Value::str(a)).collect();
                let vals = self.call_named(&name.name, args, 0)?;
                if let Some(v) = vals.into_iter().next() {
                    self.set_var("ans", v);
                    if s.display {
                        self.show("ans");
                    }
                }
            }
            StmtKind::If { clauses, else_body } => {
                for c in clauses {
                    if self.cond(&c.cond)? {
                        return self.exec_block(&c.body);
                    }
                }
                if let Some(b) = else_body {
                    return self.exec_block(b);
                }
            }
            StmtKind::For { var, iter, body } => return self.exec_for(var, iter, body),
            StmtKind::Parfor {
                var, iter, body, ..
            } => {
                let v = Expr {
                    kind: ExprKind::Ident(var.name.clone()),
                    span: var.span,
                };
                return self.exec_for(&v, iter, body);
            }
            StmtKind::ForKeyValue {
                value,
                key,
                iter,
                body,
            } => {
                let it = self.eval(iter)?;
                let Value::Struct(st) = it else {
                    return Err(Error::msg("for [val, key] = s requires a struct"));
                };
                for (k, f) in st.fields.iter().enumerate() {
                    let v = if st.numel() == 1 {
                        st.elems[0][k].clone()
                    } else {
                        Value::cell_row(st.elems.iter().map(|e| e[k].clone()).collect())
                    };
                    self.set_var(&value.name, v);
                    self.set_var(&key.name, Value::str(f));
                    self.check_interrupt()?;
                    match self.exec_block(body)? {
                        Flow::Break => break,
                        Flow::Return => return Ok(Flow::Return),
                        _ => {}
                    }
                }
            }
            StmtKind::While { cond, body } => {
                let head = self.frame().pos;
                let mut again = false;
                loop {
                    if again && self.debug.active {
                        self.debug_loop_head(head)?;
                    }
                    again = true;
                    if !self.cond(cond)? {
                        break;
                    }
                    self.check_interrupt()?;
                    match self.exec_block(body)? {
                        Flow::Break => break,
                        Flow::Return => return Ok(Flow::Return),
                        _ => {}
                    }
                }
            }
            StmtKind::DoUntil { body, cond } => loop {
                self.check_interrupt()?;
                match self.exec_block(body)? {
                    Flow::Break => break,
                    Flow::Return => return Ok(Flow::Return),
                    _ => {}
                }
                if self.cond(cond)? {
                    break;
                }
            },
            StmtKind::Switch {
                subject,
                cases,
                otherwise,
            } => {
                let v = self.eval(subject)?;
                for c in cases {
                    let cv = self.eval(&c.value)?;
                    if switch_match(&v, &cv) {
                        return self.exec_block(&c.body);
                    }
                }
                if let Some(b) = otherwise {
                    return self.exec_block(b);
                }
            }
            StmtKind::Try {
                body,
                catch_var,
                catch_body,
            } => {
                let nframes = self.frames.len();
                let depth = self.depth;
                let ends = self.end_stack.len();
                self.debug.try_depth += 1;
                let r = crate::catch_panic(|| self.exec_block(body));
                self.debug.try_depth -= 1;
                match r {
                    Ok(f) => return Ok(f),
                    // An interrupt from the session is not catchable.
                    Err(e) if self.interrupt => return Err(e),
                    Err(e) => {
                        self.frames.truncate(nframes);
                        self.depth = depth;
                        self.end_stack.truncate(ends);
                        self.lasterr = e.message.clone();
                        if let Some(v) = catch_var {
                            let ex = exception_value(self, &e);
                            self.set_var(&v.name, ex);
                        }
                        if let Some(b) = catch_body {
                            return self.exec_block(b);
                        }
                    }
                }
            }
            StmtKind::UnwindProtect { body, cleanup } => {
                let r = self.exec_block(body);
                let c = self.exec_block(cleanup)?;
                let f = r?;
                if c != Flow::Normal {
                    return Ok(c);
                }
                return Ok(f);
            }
            StmtKind::Spmd { body } => return self.exec_block(body),
            StmtKind::Break => return Ok(Flow::Break),
            StmtKind::Continue => return Ok(Flow::Continue),
            StmtKind::Return => return Ok(Flow::Return),
            StmtKind::Decl { kind, names } => {
                for (n, init) in names {
                    match kind {
                        ast::DeclKind::Global => {
                            if !self.globals.contains_key(&n.name) {
                                let v = match init {
                                    Some(e) => self.eval(e)?,
                                    None => Value::empty(),
                                };
                                self.globals.insert(n.name.clone(), v);
                            }
                            let f = self.frame_mut();
                            f.vars.remove(&n.name);
                            f.globals.insert(n.name.clone());
                        }
                        ast::DeclKind::Persistent => {
                            if self.frame().persist_key.is_empty() {
                                let f = self.frame();
                                let key = format!("{}>{}", f.ctx.path.display(), f.fname);
                                self.frame_mut().persist_key = key;
                            }
                            let key = self.frame().persist_key.clone();
                            let exists = self
                                .persist
                                .get(&key)
                                .is_some_and(|m| m.contains_key(&n.name));
                            if !exists {
                                let v = match init {
                                    Some(e) => self.eval(e)?,
                                    None => Value::empty(),
                                };
                                self.persist
                                    .entry(key)
                                    .or_default()
                                    .insert(n.name.clone(), v);
                            }
                            let f = self.frame_mut();
                            f.vars.remove(&n.name);
                            f.persistents.insert(n.name.clone());
                        }
                    }
                }
            }
        }
        Ok(Flow::Normal)
    }

    fn cond(&mut self, e: &Expr) -> R<bool> {
        let v = self.eval(e)?;
        v.is_true()
    }

    fn exec_for(&mut self, var: &Expr, iter: &Expr, body: &[Stmt]) -> R<Flow> {
        let simple = match &var.kind {
            ExprKind::Ident(n) => Some(n.as_str()),
            _ => None,
        };
        let head = self.frame().pos;
        // Fast path for numeric ranges: no materialised vector.
        if let ExprKind::Range { start, step, stop } = &iter.kind {
            let a = self.eval(start)?;
            let st = match step {
                Some(s) => Some(self.eval(s)?),
                None => None,
            };
            let b = self.eval(stop)?;
            if let (Value::Num(x), Value::Num(z)) = (&a, &b)
                && x.class == NumClass::Double
                && z.class == NumClass::Double
                && st
                    .as_ref()
                    .is_none_or(|s| matches!(s, Value::Num(n) if n.class == NumClass::Double))
            {
                let s0 = x.re.first().copied();
                let e0 = z.re.first().copied();
                let d0 = match &st {
                    Some(s) => s.scalar_f64(),
                    None => Some(1.0),
                };
                if let (Some(s0), Some(e0), Some(d0)) = (s0, e0, d0) {
                    // Long ranges are stepped lazily (`for k = 1:1e9` must not
                    // build an 8 GB vector before the first iteration).
                    let lazy = lazy_range_len(s0, d0, e0);
                    let vals = match lazy {
                        Some(_) => Vec::new(),
                        None => ops::range(s0, d0, e0)?,
                    };
                    let count = lazy.unwrap_or(vals.len());
                    for k in 0..count {
                        if k > 0 && self.debug.active {
                            self.debug_loop_head(head)?;
                        }
                        let v = match lazy {
                            Some(_) => lazy_range_at(s0, d0, e0, k, count),
                            None => vals[k],
                        };
                        match simple {
                            Some(n) => self.set_var(n, Value::num(v)),
                            None => self.assign_to(var, Value::num(v))?,
                        }
                        self.check_interrupt()?;
                        match self.exec_block(body)? {
                            Flow::Break => break,
                            Flow::Return => return Ok(Flow::Return),
                            _ => {}
                        }
                    }
                    return Ok(Flow::Normal);
                }
            }
            let v = self.range_value(a, st, b)?;
            return self.for_over(var, simple, v, body, head);
        }
        let v = self.eval(iter)?;
        self.for_over(var, simple, v, body, head)
    }

    fn for_over(
        &mut self,
        var: &Expr,
        simple: Option<&str>,
        v: Value,
        body: &[Stmt],
        head: u32,
    ) -> R<Flow> {
        let d = v.dims();
        let rows = d[0];
        let n = v.numel();
        if n == 0 {
            return Ok(Flow::Normal);
        }
        let cols = n / rows;
        let v = if d.len() > 2 {
            v.reshaped(vec![rows, cols])
        } else {
            v
        };
        for j in 0..cols {
            if j > 0 && self.debug.active {
                self.debug_loop_head(head)?;
            }
            let item = if rows == 1 && !matches!(v, Value::Struct(_)) {
                match &v {
                    Value::Num(x) => Value::Num(Num {
                        class: x.class,
                        dims: vec![1, 1],
                        re: vec![x.re[j]],
                        im: x.im.as_ref().map(|im| vec![im[j]]),
                    }),
                    _ => index_value(&v, &[Sub::scalar(j)])?,
                }
            } else {
                index_value(&v, &[Sub::Colon, Sub::scalar(j)])?
            };
            match simple {
                Some(name) => self.set_var(name, item),
                None => self.assign_to(var, item)?,
            }
            self.check_interrupt()?;
            match self.exec_block(body)? {
                Flow::Break => break,
                Flow::Return => return Ok(Flow::Return),
                _ => {}
            }
        }
        Ok(Flow::Normal)
    }

    /// Number of values an assignment target receives when it is a
    /// cs-list (`c{1:2}`, `s.f` of a struct array); `None` otherwise.
    fn cs_targets(&mut self, t: &Expr) -> R<Option<usize>> {
        let (base, ops) = flatten(t);
        let ExprKind::Ident(name) = &base.kind else {
            return Ok(None);
        };
        let Some(last) = ops.last() else {
            return Ok(None);
        };
        if !matches!(last, Op::Brace(_) | Op::Field(_) | Op::Dyn(_)) {
            return Ok(None);
        }
        if !self.has_var(name) {
            // `[c{1:n}] = ...` with `c` not defined yet.
            if let (1, Op::Brace(args)) = (ops.len(), last) {
                let vals = self.eval_index_args(args, &[0, 0])?;
                let subs = subs_of(&vals)?;
                return Ok(indy_value::index::write_positions(&[0, 0], &subs, &[1, 1])
                    .ok()
                    .map(|(pos, _)| pos.len()));
            }
            return Ok(None);
        }
        let eops = self.eval_ops_for(name, &ops)?;
        let (prefix, lastop) = eops.split_at(eops.len() - 1);
        let root = self.get_var(name).unwrap();
        let Ok(Got::V(v)) = read_path(root, prefix) else {
            return Ok(None);
        };
        Ok(match (&lastop[0], v.as_ref()) {
            (EOp::Brace(args), Value::Cell(c)) => {
                let subs = subs_of(args)?;
                match indy_value::index::write_positions(&c.dims, &subs, &[1, 1]) {
                    Ok((pos, _)) => Some(pos.len()),
                    Err(_) => None,
                }
            }
            (EOp::Field(_), Value::Struct(s)) => Some(s.numel()),
            _ => None,
        })
    }

    fn multi_assign(&mut self, lhs: &[LValue], rhs: &Expr, display: bool) -> R<Flow> {
        // Expand cs-list targets.
        let mut counts = Vec::with_capacity(lhs.len());
        for l in lhs {
            counts.push(match l {
                LValue::Ignore(_) => 1,
                LValue::Expr(t) => self.cs_targets(t)?.unwrap_or(1),
            });
        }
        let total: usize = counts.iter().sum();
        let vals = self.eval_multi(rhs, total)?;
        if vals.len() < total {
            let needed = lhs
                .iter()
                .zip(&counts)
                .scan(0, |acc, (l, c)| {
                    *acc += c;
                    Some((*acc, l))
                })
                .filter(|(_, l)| matches!(l, LValue::Expr(_)))
                .map(|(a, _)| a)
                .max()
                .unwrap_or(0);
            if vals.len() < needed {
                return Err(Error::new(
                    "MATLAB:TooManyOutputs",
                    "Too many output arguments.",
                ));
            }
        }
        let mut it = vals.into_iter();
        for (l, c) in lhs.iter().zip(counts) {
            match l {
                LValue::Ignore(_) => {
                    it.next();
                }
                LValue::Expr(t) => {
                    if c == 1 && self.cs_targets(t)?.is_none() {
                        let Some(v) = it.next() else { break };
                        self.assign_to(t, v)?;
                    } else {
                        let vs: Vec<Value> = it.by_ref().take(c).collect();
                        self.assign_cs(t, vs)?;
                    }
                    if display {
                        self.show(&base_name(t));
                    }
                }
            }
        }
        Ok(Flow::Normal)
    }

    /// `[c{1:2}] = deal(...)` / `[s.f] = deal(...)`.
    fn assign_cs(&mut self, t: &Expr, vals: Vec<Value>) -> R<()> {
        let (base, ops) = flatten(t);
        let ExprKind::Ident(name) = &base.kind else {
            return Err(Error::msg("invalid assignment target"));
        };
        let eops = self.eval_ops_for(name, &ops)?;
        let (prefix, last) = eops.split_at(eops.len() - 1);
        let mut slot = self.take_var(name);
        // Read the container, assign each element, write back.
        let r = (|| -> R<()> {
            let container = match &slot {
                Some(root) => match read_path(root, prefix)? {
                    Got::V(v) => v.into_owned(),
                    _ => return Err(Error::msg("invalid cs-list assignment target")),
                },
                None => Value::empty(),
            };
            let container = match (&last[0], container) {
                (EOp::Brace(_), Value::Num(n)) if n.re.is_empty() => Value::empty_cell(),
                (EOp::Field(_), Value::Num(n)) if n.re.is_empty() => {
                    Value::Struct(indy_value::StructArray::new_scalar())
                }
                (_, c) => c,
            };
            let new = match (&last[0], container) {
                (EOp::Brace(args), Value::Cell(mut c)) => {
                    let subs = subs_of(args)?;
                    let (pos, nd) = indy_value::index::write_positions(&c.dims, &subs, &[1, 1])?;
                    let old = c.dims.clone();
                    c.data = indy_value::index::grow(c.data, &old, &nd, Value::empty());
                    c.dims = nd;
                    for (p, v) in pos.into_iter().zip(vals) {
                        c.data[p] = v;
                    }
                    Value::Cell(c)
                }
                (EOp::Field(f), Value::Struct(mut s)) => {
                    let k = s.ensure_field(f);
                    for (e, v) in s.elems.iter_mut().zip(vals) {
                        e[k] = v;
                    }
                    Value::Struct(s)
                }
                _ => return Err(Error::msg("invalid cs-list assignment target")),
            };
            assign_path(&mut slot, prefix, new)
        })();
        if let Some(v) = slot {
            self.set_var(name, v);
        }
        r
    }

    /// Evaluate the arguments of an assignment chain on variable `name`.
    fn eval_ops_for(&mut self, name: &str, ops: &[Op]) -> R<Vec<EOp>> {
        let mut eops: Vec<EOp> = Vec::with_capacity(ops.len());
        for op in ops {
            match op {
                Op::Paren(args) | Op::Brace(args) => {
                    let dims = self.peek_dims(name, &eops);
                    let vals = self.eval_index_args(args, &dims)?;
                    eops.push(if matches!(op, Op::Paren(_)) {
                        EOp::Paren(vals)
                    } else {
                        EOp::Brace(vals)
                    });
                }
                Op::Field(f) => eops.push(EOp::Field(f.to_string())),
                Op::Dyn(e) => {
                    let v = self.eval(e)?;
                    eops.push(EOp::Field(field_name(&v)?));
                }
            }
        }
        Ok(eops)
    }

    fn peek_dims(&self, name: &str, eops: &[EOp]) -> Vec<usize> {
        let Some(root) = self.get_var(name) else {
            return vec![0, 0];
        };
        match read_path(root, eops) {
            Ok(Got::V(v)) => v.dims(),
            Ok(Got::List(l)) if l.len() == 1 => l[0].dims(),
            Ok(Got::Call(..)) => vec![1, 1],
            _ => vec![0, 0],
        }
    }

    pub fn assign_to(&mut self, t: &Expr, v: Value) -> R<()> {
        let (base, ops) = flatten(t);
        let name = match &base.kind {
            ExprKind::Ident(n) => n.as_str(),
            _ => {
                return Err(Error::new(
                    "MATLAB:invalidAssignment",
                    "Invalid assignment target.",
                ));
            }
        };
        if ops.is_empty() {
            self.set_var(name, v);
            return Ok(());
        }
        let eops = self.eval_ops_for(name, &ops)?;
        match self.get_var(name) {
            Some(x) if crate::classes::is_class_obj(x) => {
                return self.assign_object(name, &eops, v);
            }
            Some(x) if crate::builtins::control::is_lti(x) => {
                return crate::builtins::control::assign(self, name, &eops, v);
            }
            Some(x) if datatypes::is_native(x) => {
                let obj = self.get_var(name).cloned().unwrap();
                let new = datatypes::assign(obj, &eops, v)?;
                self.set_var(name, new);
                return Ok(());
            }
            Some(Value::HObject(_)) => {
                let h = self.get_var(name).cloned().unwrap();
                return crate::builtins::handles::handle_assign(self, h, &eops, v);
            }
            Some(Value::Num(_)) if eops.iter().any(|o| matches!(o, EOp::Field(_))) => {
                if let Some(set) = self.handle_field_set {
                    let h = self.get_var(name).cloned().unwrap();
                    if let Some(r) = set(self, &h, &eops, &v) {
                        return r;
                    }
                }
            }
            _ => {}
        }
        match self.var_slot(name) {
            Some(r) => {
                let mut slot = Some(std::mem::take(r));
                let res = assign_path(&mut slot, &eops, v);
                *r = slot.unwrap_or_default();
                res
            }
            None => {
                let mut slot = None;
                let res = assign_path(&mut slot, &eops, v);
                if let Some(mut val) = slot {
                    // `a(3) = obj` with `a` undefined: default objects fill
                    // a(1:2).
                    if matches!(val, Value::ObjArr(_)) {
                        self.fill_object_gaps(&mut val)?;
                    }
                    self.set_var(name, val);
                }
                res
            }
        }
    }

    // ===================================================================
    // Expressions
    // ===================================================================

    /// Evaluate to exactly one value.
    pub fn eval(&mut self, e: &Expr) -> R<Value> {
        match &e.kind {
            ExprKind::Number { value, imag } => Ok(if *imag {
                Value::complex(0.0, *value)
            } else {
                Value::num(*value)
            }),
            ExprKind::Integer { value, suffix } => {
                let class = match *suffix {
                    Some("u8") => NumClass::UInt8,
                    Some("u16") => NumClass::UInt16,
                    Some("u32") => NumClass::UInt32,
                    Some("u64") => NumClass::UInt64,
                    Some("s8") => NumClass::Int8,
                    Some("s16") => NumClass::Int16,
                    Some("s32") => NumClass::Int32,
                    Some("s64") => NumClass::Int64,
                    _ => {
                        if *value <= u8::MAX as u64 {
                            NumClass::UInt8
                        } else if *value <= u16::MAX as u64 {
                            NumClass::UInt16
                        } else if *value <= u32::MAX as u64 {
                            NumClass::UInt32
                        } else {
                            NumClass::UInt64
                        }
                    }
                };
                let x = if class.is_int()
                    && !matches!(
                        class,
                        NumClass::UInt8 | NumClass::UInt16 | NumClass::UInt32 | NumClass::UInt64
                    ) {
                    // Two's complement for signed suffixes.
                    let bits = match class {
                        NumClass::Int8 => 8,
                        NumClass::Int16 => 16,
                        NumClass::Int32 => 32,
                        _ => 64,
                    };
                    let v = *value as i128;
                    let v = if bits < 64 && v >= (1i128 << (bits - 1)) {
                        v - (1i128 << bits)
                    } else if bits == 64 && v >= (1i128 << 63) {
                        v - (1i128 << 64)
                    } else {
                        v
                    };
                    v as f64
                } else {
                    *value as f64
                };
                Ok(Value::Num(Num::scalar(x).with_class(class)))
            }
            ExprKind::Str(s) if self.frame().ctx.matlab => Ok(Value::string(s)),
            ExprKind::Char(s) | ExprKind::Str(s) => Ok(Value::str(s)),
            ExprKind::Ident(name) => {
                if let Some(v) = self.get_var(name) {
                    return Ok(v.clone());
                }
                single(self.call_chain(name, &[], 1)?)
            }
            ExprKind::Colon => Ok(Value::str(":")),
            ExprKind::End => {
                if let Some((_, k, n, Some(o))) = self.end_stack.last().cloned()
                    && let Some(v) = self.object_end(&o, k, n)?
                {
                    return Ok(Value::num(v));
                }
                let Some((dims, k, n, _)) = self.end_stack.last() else {
                    return Err(Error::new(
                        "MATLAB:endOutsideIndex",
                        "'end' used outside an index expression.",
                    ));
                };
                let v = if *n == 1 {
                    dims.iter().product::<usize>()
                } else if k + 1 < *n {
                    *dims.get(*k).unwrap_or(&1)
                } else {
                    dims.iter().skip(*k).product::<usize>()
                };
                Ok(Value::num(v as f64))
            }
            ExprKind::Unary { op, operand } => {
                let v = self.eval(operand)?;
                if datatypes::is_native(&v) {
                    return datatypes::unary(*op, &v);
                }
                if matches!(v, Value::Object(_) | Value::HObject(_)) {
                    return self.object_unary(*op, &v);
                }
                Ok(ops::unary(*op, &v)?)
            }
            ExprKind::Binary { op, lhs, rhs } => match op {
                BinOp::AndAnd | BinOp::OrOr => {
                    let a = self.eval(lhs)?;
                    let at = self.sc_truth(&a, *op)?;
                    if *op == BinOp::AndAnd && !at {
                        return Ok(Value::bool(false));
                    }
                    if *op == BinOp::OrOr && at {
                        return Ok(Value::bool(true));
                    }
                    let b = self.eval(rhs)?;
                    Ok(Value::bool(self.sc_truth(&b, *op)?))
                }
                _ => {
                    let a = self.eval(lhs)?;
                    let b = self.eval(rhs)?;
                    if (datatypes::is_native(&a) || datatypes::is_native(&b))
                        && !crate::classes::is_class_obj(&a)
                        && !crate::classes::is_class_obj(&b)
                    {
                        return datatypes::binary(*op, &a, &b);
                    }
                    if matches!(a, Value::Object(_) | Value::HObject(_))
                        || matches!(b, Value::Object(_) | Value::HObject(_))
                    {
                        if let Some(v) = self.object_binary(*op, &a, &b)? {
                            return Ok(v);
                        }
                        return Err(Error::msg("operator method returned no value"));
                    }
                    Ok(ops::binary(*op, &a, &b)?)
                }
            },
            ExprKind::Range { start, step, stop } => {
                let a = self.eval(start)?;
                let s = match step {
                    Some(s) => Some(self.eval(s)?),
                    None => None,
                };
                let b = self.eval(stop)?;
                if datatypes::is_native(&a) || datatypes::is_native(&b) {
                    return datatypes::time::range(&a, s.as_ref(), &b);
                }
                self.range_value(a, s, b)
            }
            ExprKind::Paren(inner) => {
                let v = self.eval_multi(inner, 1)?;
                single(v)
            }
            ExprKind::Index { .. }
            | ExprKind::CellIndex { .. }
            | ExprKind::Field { .. }
            | ExprKind::DynField { .. } => {
                let v = self.postfix(e, 1)?;
                single(v)
            }
            ExprKind::Matrix(rows) => self.build_matrix(rows),
            ExprKind::Cell(rows) => self.build_cell(rows),
            ExprKind::AnonFn { params, body } => self.make_anon(e, params, body),
            ExprKind::FuncHandle(path)
                if path.parts.len() >= 2 && self.has_var(&path.parts[0].name) =>
            {
                self.bound_method_handle(e, path)
            }
            ExprKind::FuncHandle(path) if self.nested_env_for(&path.dotted()).is_some() => {
                let env = self.nested_env_for(&path.dotted()).unwrap();
                let ctx = crate::NestedHandleCtx {
                    file: self.frame().ctx.clone(),
                    env,
                };
                Ok(Value::Func(Rc::new(indy_value::Handle {
                    name: path.dotted(),
                    anon: None,
                    ctx: Some(Rc::new(ctx) as Rc<dyn std::any::Any>),
                })))
            }
            ExprKind::FuncHandle(path) => Ok(Value::Func(Rc::new(indy_value::Handle {
                name: path.dotted(),
                anon: None,
                ctx: Some(self.frame().ctx.clone() as Rc<dyn std::any::Any>),
            }))),
            ExprKind::Assign { lhs, op, rhs } => {
                let r = self.eval(rhs)?;
                let v = match op {
                    Some(op) => {
                        let a = self.eval(lhs)?;
                        ops::binary(*op, &a, &r)?
                    }
                    None => r,
                };
                self.assign_to(lhs, v.clone())?;
                Ok(v)
            }
            ExprKind::PostIncDec { target, op } => {
                let a = self.eval(target)?;
                let v = ops::binary(*op, &a, &Value::num(1.0))?;
                self.assign_to(target, v)?;
                Ok(a)
            }
            ExprKind::NamedArg { name, .. } => Err(Error::msg(format!(
                "Name=value argument '{}' is only valid in a function call.",
                name.name
            ))),
            ExprKind::Metaclass(p) => Err(Error::new(
                "MATLAB:unsupported",
                format!(
                    "?{} (metaclass) is not supported yet (phase 2 engine).",
                    p.dotted()
                ),
            )),
            ExprKind::SuperRef { method, .. } => Err(Error::new(
                "MATLAB:unsupported",
                format!(
                    "{}@superclass calls are not supported yet (phase 2 engine).",
                    method.name
                ),
            )),
        }
    }

    fn sc_truth(&mut self, v: &Value, op: BinOp) -> R<bool> {
        if v.is_empty() {
            return Err(Error::new(
                "MATLAB:nonLogicalConditional",
                format!(
                    "Operands to the {} operator must be convertible to logical scalar values.",
                    op.text()
                ),
            ));
        }
        v.is_true()
    }

    fn range_value(&mut self, a: Value, s: Option<Value>, b: Value) -> R<Value> {
        let first = |v: &Value| -> R<f64> {
            match v.f64_at(0) {
                Some(x) => Ok(x),
                None if v.is_empty() => Ok(f64::NAN),
                None => Err(Error::msg(format!(
                    "Colon operands must be real scalars (got {}).",
                    v.class_name()
                ))),
            }
        };
        if a.is_empty() || b.is_empty() || s.as_ref().is_some_and(|s| s.is_empty()) {
            return Ok(Value::Num(Num::real(vec![1, 0], vec![])));
        }
        let x = first(&a)?;
        let st = match &s {
            Some(v) => first(v)?,
            None => 1.0,
        };
        let y = first(&b)?;
        let vals = ops::range(x, st, y)?;
        if a.is_char() && b.is_char() {
            let chars: Vec<char> = vals
                .iter()
                .map(|v| char::from_u32(*v as u32).unwrap_or('\0'))
                .collect();
            let n = chars.len();
            return Ok(Value::Char(Array::new(vec![1, n], chars)));
        }
        let class = [Some(&a), s.as_ref(), Some(&b)]
            .into_iter()
            .flatten()
            .find_map(|v| match v {
                Value::Num(n) if n.class != NumClass::Double => Some(n.class),
                _ => None,
            })
            .unwrap_or(NumClass::Double);
        let n = vals.len();
        Ok(Value::Num(Num::real(vec![1, n], vals).with_class(class)))
    }

    /// Evaluate allowing cs-lists; `nargout` is passed to calls.
    pub fn eval_multi(&mut self, e: &Expr, nargout: usize) -> R<Vec<Value>> {
        match &e.kind {
            ExprKind::Ident(name) => {
                if let Some(v) = self.get_var(name) {
                    return Ok(vec![v.clone()]);
                }
                self.call_chain(name, &[], nargout)
            }
            ExprKind::Index { .. }
            | ExprKind::CellIndex { .. }
            | ExprKind::Field { .. }
            | ExprKind::DynField { .. } => self.postfix(e, nargout),
            _ => Ok(vec![self.eval(e)?]),
        }
    }

    fn postfix(&mut self, e: &Expr, nargout: usize) -> R<Vec<Value>> {
        let (base, ops) = flatten(e);
        match &base.kind {
            ExprKind::Ident(name) => {
                if self.has_var(name) {
                    self.index_var(name, &ops, nargout)
                } else {
                    self.call_chain(name, &ops, nargout)
                }
            }
            ExprKind::SuperRef { method, superclass } => {
                let args = match ops.first() {
                    Some(Op::Paren(a)) => self.eval_args(a)?,
                    _ => vec![],
                };
                let skip = usize::from(matches!(ops.first(), Some(Op::Paren(_))));
                let rest = &ops[skip..];
                let outs = self.super_call(
                    &method.name,
                    &superclass.dotted(),
                    args,
                    if rest.is_empty() { nargout } else { 1 },
                )?;
                if rest.is_empty() {
                    return Ok(outs);
                }
                let v = single(outs)?;
                self.apply_ops(v, rest, nargout)
            }
            _ => {
                let v = self.eval(base)?;
                self.apply_ops(v, &ops, nargout)
            }
        }
    }

    /// Index into variable `name` without copying it.
    fn index_var(&mut self, name: &str, ops: &[Op], nargout: usize) -> R<Vec<Value>> {
        // Dot on a number fails in read_path; a handle hook may accept it.
        if self.handle_field_get.is_some()
            && matches!(self.get_var(name), Some(Value::Num(_)))
            && ops.iter().any(|o| matches!(o, Op::Field(_) | Op::Dyn(_)))
        {
            let v = self.get_var(name).cloned().unwrap();
            return self.apply_ops(v, ops, nargout);
        }
        let mut eops: Vec<EOp> = Vec::with_capacity(ops.len());
        let mut stopped = None;
        for (i, op) in ops.iter().enumerate() {
            match op {
                Op::Paren(args) | Op::Brace(args) => {
                    let root = self.get_var(name).unwrap();
                    let mut end_obj = None;
                    let dims = match read_path(root, &eops)? {
                        Got::V(v) => {
                            if crate::classes::is_class_obj(&v) {
                                end_obj = Some(v.clone().into_owned());
                            }
                            v.dims()
                        }
                        Got::Call(v, at) if !matches!(v, Value::Func(_)) => {
                            if datatypes::is_native(&v) {
                                datatypes::peek_dims(&v, &eops[at..])
                            } else {
                                if eops.is_empty() && crate::classes::is_class_obj(&v) {
                                    end_obj = Some(v);
                                }
                                vec![1, 1]
                            }
                        }
                        Got::List(l) => {
                            if l.len() == 1 {
                                l[0].dims()
                            } else {
                                return Err(cs_list_error(l.len()));
                            }
                        }
                        Got::Call(..) => {
                            stopped = Some(i);
                            break;
                        }
                    };
                    let vals = self.eval_index_args_obj(args, &dims, end_obj)?;
                    eops.push(if matches!(op, Op::Paren(_)) {
                        EOp::Paren(vals)
                    } else {
                        EOp::Brace(vals)
                    });
                }
                Op::Field(f) => eops.push(EOp::Field(f.to_string())),
                Op::Dyn(x) => {
                    let v = self.eval(x)?;
                    eops.push(EOp::Field(field_name(&v)?));
                }
            }
        }
        let root = self.get_var(name).unwrap();
        let got = read_path(root, &eops)?;
        let (vals, done) = match got {
            Got::V(v) => (vec![v.into_owned()], eops.len()),
            Got::List(l) => (l, eops.len()),
            Got::Call(v, at) => {
                let rest_raw = match stopped {
                    Some(i) => &ops[i..],
                    None => &[],
                };
                if let (Value::Func(_), Some(Op::Paren(a)), Some(EOp::Paren(vals))) =
                    (&v, ops.get(at), eops.get(at))
                {
                    self.pending_arg_names = Some(self.arg_names(a, vals.len()));
                }
                let out = self.apply_eops(
                    v,
                    &eops[at..],
                    if rest_raw.is_empty() { nargout } else { 1 },
                )?;
                if rest_raw.is_empty() {
                    return Ok(out);
                }
                let v = single(out)?;
                return self.apply_ops(v, rest_raw, nargout);
            }
        };
        debug_assert_eq!(done, eops.len());
        if let Some(i) = stopped {
            let v = single(vals)?;
            return self.apply_ops(v, &ops[i..], nargout);
        }
        Ok(vals)
    }

    /// Apply evaluated ops to an owned value (handles calls).
    pub(crate) fn apply_eops(
        &mut self,
        value: Value,
        eops: &[EOp],
        nargout: usize,
    ) -> R<Vec<Value>> {
        let mut vals = vec![value];
        let mut i = 0;
        while i < eops.len() {
            let last = i + 1 == eops.len();
            if vals.len() != 1 {
                return Err(cs_list_error(vals.len()));
            }
            let v = vals.pop().unwrap();
            let n = if last { nargout } else { 1 };
            if crate::classes::is_class_obj(&v) {
                return self.object_ops(v, &eops[i..], nargout);
            }
            if crate::builtins::control::is_lti(&v) {
                return crate::builtins::control::object_ops(self, v, &eops[i..], nargout);
            }
            if datatypes::is_native(&v) {
                return datatypes::index(self, v, &eops[i..], nargout);
            }
            if matches!(v, Value::HObject(_)) {
                return crate::builtins::handles::handle_ops(self, v, &eops[i..], nargout);
            }
            if let (Some(get), EOp::Field(f), Value::Num(_)) = (self.handle_field_get, &eops[i], &v)
                && let Some(r) = get(self, &v, f)
            {
                vals = r?;
                i += 1;
                continue;
            }
            match (&eops[i], &v) {
                (EOp::Paren(args), Value::Func(h)) => {
                    vals = self.call_handle(h, args.clone(), n)?;
                }
                (EOp::Paren(args), Value::Object(_)) => {
                    let subs = subs_of(args)?;
                    vals = vec![index_value(&v, &subs)?];
                }
                (EOp::Field(name), Value::Object(o)) if o.get(name).is_none() => {
                    // Method call: obj.method(args).
                    let mut args = vec![v.clone()];
                    let mut consumed = 1;
                    if let Some(EOp::Paren(a)) = eops.get(i + 1) {
                        args.extend(a.iter().cloned());
                        consumed = 2;
                    }
                    let last2 = i + consumed == eops.len();
                    vals = self.call_named(name, args, if last2 { nargout } else { 1 })?;
                    i += consumed;
                    continue;
                }
                _ => {
                    vals = match read_path(&v, &eops[i..i + 1])? {
                        Got::V(x) => vec![x.into_owned()],
                        Got::List(l) => l,
                        Got::Call(..) => unreachable!(),
                    };
                }
            }
            i += 1;
        }
        Ok(vals)
    }

    /// Apply raw ops to an owned value, evaluating args one op at a time.
    pub(crate) fn apply_ops(&mut self, value: Value, ops: &[Op], nargout: usize) -> R<Vec<Value>> {
        let mut vals = vec![value];
        for (i, op) in ops.iter().enumerate() {
            let last = i + 1 == ops.len();
            if vals.len() != 1 {
                return Err(cs_list_error(vals.len()));
            }
            let v = vals.pop().unwrap();
            let eop = match op {
                Op::Paren(args) | Op::Brace(args) => {
                    let dims = v.dims();
                    let a = if matches!(v, Value::Func(_)) && matches!(op, Op::Paren(_)) {
                        let a = self.eval_args(args)?;
                        self.pending_arg_names = Some(self.arg_names(args, a.len()));
                        a
                    } else {
                        self.eval_index_args(args, &dims)?
                    };
                    if matches!(op, Op::Paren(_)) {
                        EOp::Paren(a)
                    } else {
                        EOp::Brace(a)
                    }
                }
                Op::Field(f) => EOp::Field(f.to_string()),
                Op::Dyn(x) => {
                    let n = self.eval(x)?;
                    EOp::Field(field_name(&n)?)
                }
            };
            vals = self.apply_eops(
                v,
                std::slice::from_ref(&eop),
                if last { nargout } else { 1 },
            )?;
        }
        Ok(vals)
    }

    /// Call `name` (not a variable) with the postfix ops that follow.
    fn call_chain(&mut self, name: &str, ops: &[Op], nargout: usize) -> R<Vec<Value>> {
        let mut callee = self.resolve(name)?;
        let mut consumed = 0;
        if callee.is_none() {
            let mut dotted = name.to_string();
            for (k, op) in ops.iter().enumerate() {
                let Op::Field(f) = op else { break };
                dotted.push('.');
                dotted.push_str(f);
                if let Some(c) = self.resolve(&dotted)? {
                    callee = Some(c);
                    consumed = k + 1;
                    break;
                }
            }
        }
        // Class.member / Class.member(args): static methods, constants, enums.
        if let Some(crate::Callee::Class(cls)) = &callee
            && let Some(Op::Field(member)) = ops.get(consumed)
        {
            let cls = cls.clone();
            let member = member.to_string();
            consumed += 1;
            let args = if let Some(Op::Paren(a)) = ops.get(consumed) {
                consumed += 1;
                Some(self.eval_args(a)?)
            } else {
                None
            };
            let rest = &ops[consumed..];
            let outs = self.class_member(
                &cls,
                &member,
                args,
                if rest.is_empty() { nargout } else { 1 },
            )?;
            if rest.is_empty() {
                return Ok(outs);
            }
            let v = single(outs)?;
            return self.apply_ops(v, rest, nargout);
        }
        // `set(sys, ...)` without an output updates the variable (MATLAB).
        let set_target = match ops.get(consumed) {
            Some(Op::Paren(a)) if name == "set" && nargout == 0 && ops.len() == consumed + 1 => {
                match a.first().map(|e| &e.kind) {
                    Some(ExprKind::Ident(v)) => Some(v.to_string()),
                    _ => None,
                }
            }
            _ => None,
        };
        let args = if let Some(Op::Paren(a)) = ops.get(consumed) {
            consumed += 1;
            Some(self.eval_args(a)?)
        } else {
            None
        };
        let arg_exprs = match (consumed, ops.first()) {
            (1.., Some(Op::Paren(a))) if args.is_some() => Some(a),
            _ => None,
        };
        let rest = &ops[consumed..];
        let n = if rest.is_empty() { nargout } else { 1 };
        let args_v = args.unwrap_or_default();
        let set_target =
            set_target.filter(|_| args_v.first().is_some_and(crate::builtins::control::is_lti));
        // `inputname` for the datatypes constructors that name variables
        // after their arguments (table(x, y) has variables x and y).
        let named =
            datatypes::wants_arg_names(name) && arg_exprs.is_some_and(|a| a.len() == args_v.len());
        if named {
            let names = arg_exprs
                .unwrap()
                .iter()
                .map(|e| match &e.kind {
                    ExprKind::Ident(n) => Some(n.clone()),
                    _ => None,
                })
                .collect();
            self.ext_mut::<datatypes::table::ArgNames>().0 = names;
        }
        if args_v.iter().any(crate::classes::is_class_obj)
            && let Some((cls, m)) = self.dispatch_method(name, &args_v)
        {
            self.pending_arg_names = arg_exprs.map(|a| self.arg_names(a, args_v.len()));
            let outs = self.call_method_info(&cls, &m, args_v, n)?;
            if rest.is_empty() {
                return Ok(outs);
            }
            let v = single(outs)?;
            return self.apply_ops(v, rest, nargout);
        }
        if let Some(f) = self.mexception_method(name, &args_v) {
            let outs = f(self, args_v, n)?;
            if let Some(var) = set_target {
                if let Some(v) = outs.into_iter().next() {
                    self.set_var(&var, v);
                }
                return Ok(vec![]);
            }
            if rest.is_empty() {
                return Ok(outs);
            }
            let v = single(outs)?;
            return self.apply_ops(v, rest, nargout);
        }
        let Some(callee) = callee else {
            return Err(Interp::undefined(name));
        };
        if matches!(callee, crate::Callee::User(..)) {
            self.pending_arg_names = arg_exprs.map(|a| self.arg_names(a, args_v.len()));
        }
        let outs = self.call_callee(&callee, args_v, n);
        if named {
            self.ext_mut::<datatypes::table::ArgNames>().0.clear();
        }
        let outs = outs?;
        if rest.is_empty() {
            return Ok(outs);
        }
        let v = single(outs)?;
        self.apply_ops(v, rest, nargout)
    }

    /// Variable name behind each argument (`inputname`): a bare variable
    /// gives its name, anything else none. After an argument that expanded
    /// to a different number of values positions are unknown.
    fn arg_names(&self, exprs: &[Expr], nvals: usize) -> Vec<Option<String>> {
        let mut out = Vec::with_capacity(nvals);
        for e in exprs {
            match &e.kind {
                ExprKind::Ident(n) if self.has_var(n) => out.push(Some(n.clone())),
                // `c{:}`, `s.f`, `f(x)`: may expand to several values.
                ExprKind::Index { .. }
                | ExprKind::CellIndex { .. }
                | ExprKind::Field { .. }
                | ExprKind::DynField { .. }
                | ExprKind::NamedArg { .. } => break,
                _ => out.push(None),
            }
        }
        out.truncate(nvals);
        out
    }

    /// Function-call arguments (cs-lists expand, `Name=value` pairs).
    pub fn eval_args(&mut self, args: &[Expr]) -> R<Vec<Value>> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            match &a.kind {
                ExprKind::Colon => out.push(Value::str(":")),
                ExprKind::NamedArg { name, value } => {
                    out.push(Value::str(&name.name));
                    out.push(self.eval(value)?);
                }
                _ => out.extend(self.eval_multi(a, 1)?),
            }
        }
        Ok(out)
    }

    /// Index arguments with `end` bound to `dims`.
    pub fn eval_index_args(&mut self, args: &[Expr], dims: &[usize]) -> R<Vec<Value>> {
        self.eval_index_args_obj(args, dims, None)
    }

    /// Same, with an object whose class may overload `end`.
    pub fn eval_index_args_obj(
        &mut self,
        args: &[Expr],
        dims: &[usize],
        obj: Option<Value>,
    ) -> R<Vec<Value>> {
        let mut out = Vec::with_capacity(args.len());
        let n = args.len();
        for (k, a) in args.iter().enumerate() {
            match &a.kind {
                ExprKind::Colon => out.push(Value::str(":")),
                ExprKind::Number { value, imag: false } => out.push(Value::num(*value)),
                ExprKind::Ident(name) if self.has_var(name) => {
                    out.push(self.get_var(name).unwrap().clone())
                }
                _ => {
                    self.end_stack.push((dims.to_vec(), k, n, obj.clone()));
                    let r = self.eval_multi(a, 1);
                    self.end_stack.pop();
                    out.extend(r?);
                }
            }
        }
        Ok(out)
    }

    fn build_matrix(&mut self, rows: &[Vec<Expr>]) -> R<Value> {
        let mut row_vals = Vec::with_capacity(rows.len());
        for r in rows {
            let mut vals = Vec::with_capacity(r.len());
            for e in r {
                vals.extend(self.eval_multi(e, 1)?);
            }
            if vals.iter().all(|v| matches!(v, Value::Num(n) if n.re.len() == 1 && n.im.is_none() && n.class == NumClass::Double && n.dims.len() == 2)) && !vals.is_empty() {
                let data: Vec<f64> = vals.iter().map(|v| v.f64_at(0).unwrap()).collect();
                row_vals.push(Value::row(data));
                continue;
            }
            row_vals.push(datatypes::cat(vals, 1)?);
        }
        if row_vals.is_empty() {
            return Ok(Value::empty());
        }
        datatypes::cat(row_vals, 0)
    }

    fn build_cell(&mut self, rows: &[Vec<Expr>]) -> R<Value> {
        let mut row_vals = Vec::with_capacity(rows.len());
        for r in rows {
            let mut vals = Vec::with_capacity(r.len());
            for e in r {
                // Every element (cells included) becomes one cell element.
                for v in self.eval_multi(e, 1)? {
                    vals.push(Value::Cell(Array::scalar(v)));
                }
            }
            if vals.is_empty() {
                continue;
            }
            // Elements are cells: cells concatenate, others were wrapped.
            row_vals.push(concat::cat(vals, 1)?);
        }
        if row_vals.is_empty() {
            return Ok(Value::empty_cell());
        }
        concat::cat(row_vals, 0)
    }

    /// `@obj.method`: a handle bound to the object (as `@(varargin) obj.method(varargin{:})`).
    fn bound_method_handle(&mut self, e: &Expr, path: &ast::Path) -> R<Value> {
        let sp = e.span;
        let mk = |kind| Expr { kind, span: sp };
        let mut target = mk(ExprKind::Ident(path.parts[0].name.clone()));
        for p in &path.parts[1..] {
            target = mk(ExprKind::Field {
                target: Box::new(target),
                name: p.clone(),
            });
        }
        let va = mk(ExprKind::CellIndex {
            target: Box::new(mk(ExprKind::Ident("varargin".into()))),
            args: vec![mk(ExprKind::Colon)],
        });
        let body = mk(ExprKind::Index {
            target: Box::new(target),
            args: vec![va],
        });
        let obj = self.get_var(&path.parts[0].name).cloned().unwrap();
        Ok(Value::Func(Rc::new(indy_value::Handle {
            name: String::new(),
            anon: Some(indy_value::Anon {
                params: vec!["varargin".into()],
                body: Rc::new(body),
                captured: vec![(path.parts[0].name.clone(), obj)],
                text: format!("@{}", path.dotted()),
            }),
            ctx: Some(self.frame().ctx.clone() as Rc<dyn std::any::Any>),
        })))
    }

    fn make_anon(&mut self, e: &Expr, params: &[ast::Param], body: &Expr) -> R<Value> {
        let names: Vec<String> = params
            .iter()
            .map(|p| match p {
                ast::Param::Name(n) => n.name.clone(),
                ast::Param::WithDefault { name, .. } => name.name.clone(),
                ast::Param::Ignore(_) => "~".into(),
            })
            .collect();
        let mut ids = Vec::new();
        collect_idents(body, &mut ids);
        ids.sort();
        ids.dedup();
        let mut captured = Vec::new();
        for id in ids {
            if names.contains(&id) {
                continue;
            }
            if let Some(v) = self.get_var(&id) {
                captured.push((id, v.clone()));
            }
        }
        let ctx = self.frame().ctx.clone();
        let body_rc = ctx
            .anon_bodies
            .borrow_mut()
            .entry(e.span.start)
            .or_insert_with(|| Rc::new(body.clone()))
            .clone();
        let text = anon_text(ctx.text(e.span), &names, body);
        Ok(Value::Func(Rc::new(indy_value::Handle {
            name: String::new(),
            anon: Some(indy_value::Anon {
                params: names,
                body: body_rc,
                captured,
                text,
            }),
            ctx: Some(ctx as Rc<dyn std::any::Any>),
        })))
    }
}

/// MATLAB-style text of an anonymous function: `@(x,y)x+2*y`.
fn anon_text(_full: &str, params: &[String], body: &Expr) -> String {
    format!("@({}){}", params.join(","), unparse(body))
}

fn prec(e: &Expr) -> u8 {
    match &e.kind {
        ExprKind::Binary { op, .. } => match op {
            BinOp::OrOr => 1,
            BinOp::AndAnd => 2,
            BinOp::Or => 3,
            BinOp::And => 4,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 5,
            BinOp::Add | BinOp::Sub => 7,
            BinOp::Mul
            | BinOp::Div
            | BinOp::LeftDiv
            | BinOp::EMul
            | BinOp::EDiv
            | BinOp::ELeftDiv => 8,
            BinOp::Pow | BinOp::EPow => 10,
        },
        ExprKind::Range { .. } => 6,
        ExprKind::Unary {
            op: ast::UnOp::Neg | ast::UnOp::Plus | ast::UnOp::Not,
            ..
        } => 9,
        _ => 11,
    }
}

/// Render an expression the way MATLAB's func2str does (no blanks
/// around operators, `,` between elements and arguments).
pub fn unparse(e: &Expr) -> String {
    let sub = |x: &Expr, min: u8| {
        let t = unparse(x);
        if prec(x) < min { format!("({})", t) } else { t }
    };
    let list = |xs: &[Expr]| xs.iter().map(unparse).collect::<Vec<_>>().join(",");
    match &e.kind {
        ExprKind::Number { value, imag } => {
            let s = format::short_g(*value);
            if *imag { format!("{}i", s) } else { s }
        }
        ExprKind::Integer { value, .. } => value.to_string(),
        ExprKind::Char(s) => format!("'{}'", s.replace('\'', "''")),
        ExprKind::Str(s) => format!("\"{}\"", s.replace('"', "\"\"")),
        ExprKind::Ident(n) => n.clone(),
        ExprKind::Colon => ":".into(),
        ExprKind::End => "end".into(),
        ExprKind::Unary { op, operand } => match op {
            ast::UnOp::Neg => format!("-{}", sub(operand, 9)),
            ast::UnOp::Plus => format!("+{}", sub(operand, 9)),
            ast::UnOp::Not => format!("~{}", sub(operand, 9)),
            ast::UnOp::Transpose => format!("{}.'", sub(operand, 11)),
            ast::UnOp::CTranspose => format!("{}'", sub(operand, 11)),
        },
        ExprKind::Binary { op, lhs, rhs } => {
            let p = prec(e);
            format!("{}{}{}", sub(lhs, p), op.text(), sub(rhs, p + 1))
        }
        ExprKind::Range { start, step, stop } => match step {
            Some(st) => format!("{}:{}:{}", sub(start, 7), sub(st, 7), sub(stop, 7)),
            None => format!("{}:{}", sub(start, 7), sub(stop, 7)),
        },
        ExprKind::Paren(x) => format!("({})", unparse(x)),
        ExprKind::Index { target, args } => format!("{}({})", unparse(target), list(args)),
        ExprKind::CellIndex { target, args } => format!("{}{{{}}}", unparse(target), list(args)),
        ExprKind::Field { target, name } => format!("{}.{}", unparse(target), name.name),
        ExprKind::DynField { target, name } => format!("{}.({})", unparse(target), unparse(name)),
        ExprKind::Matrix(rows) | ExprKind::Cell(rows) => {
            let body = rows.iter().map(|r| list(r)).collect::<Vec<_>>().join(";");
            if matches!(e.kind, ExprKind::Matrix(_)) {
                format!("[{}]", body)
            } else {
                format!("{{{}}}", body)
            }
        }
        ExprKind::AnonFn { params, body } => {
            let names: Vec<String> = params
                .iter()
                .map(|p| match p {
                    ast::Param::Name(n) => n.name.clone(),
                    ast::Param::WithDefault { name, .. } => name.name.clone(),
                    ast::Param::Ignore(_) => "~".into(),
                })
                .collect();
            anon_text("", &names, body)
        }
        ExprKind::FuncHandle(p) => format!("@{}", p.dotted()),
        ExprKind::Metaclass(p) => format!("?{}", p.dotted()),
        ExprKind::SuperRef { method, superclass } => {
            format!("{}@{}", method.name, superclass.dotted())
        }
        ExprKind::Assign { lhs, rhs, .. } => format!("{}={}", unparse(lhs), unparse(rhs)),
        ExprKind::PostIncDec { target, op } => format!(
            "{}{}",
            unparse(target),
            if *op == BinOp::Add { "++" } else { "--" }
        ),
        ExprKind::NamedArg { name, value } => format!("{}={}", name.name, unparse(value)),
    }
}

fn base_name(t: &Expr) -> String {
    let (b, _) = flatten(t);
    match &b.kind {
        ExprKind::Ident(n) => n.clone(),
        _ => "ans".into(),
    }
}

pub fn field_name(v: &Value) -> R<String> {
    v.to_str().ok_or_else(|| {
        Error::new(
            "MATLAB:dynamicFieldName",
            "Dynamic structure field names must be character vectors.",
        )
    })
}

pub fn switch_match(v: &Value, cv: &Value) -> bool {
    if matches!(v, Value::Str(_)) || matches!(cv, Value::Str(_)) {
        if let Value::Cell(c) = cv {
            return c.data.iter().any(|x| switch_match(v, x));
        }
        return v.is_text() && cv.is_text() && v.to_str().is_some() && v.to_str() == cv.to_str();
    }
    match cv {
        Value::Cell(c) => c.data.iter().any(|x| switch_match(v, x)),
        Value::Char(_) => match v {
            Value::Char(_) => v.to_str() == cv.to_str() && v.dims_ref().len() == 2,
            _ => false,
        },
        _ => {
            if v.is_char() {
                return false;
            }
            if v.is_empty() && cv.is_empty() {
                return true;
            }
            match (v.scalar_f64(), cv.scalar_f64()) {
                (Some(a), Some(b)) => a == b,
                _ => crate::builtins::isequal_values(v, cv, false),
            }
        }
    }
}

/// Identifiers used anywhere in statements (variables of a function body).
pub(crate) fn collect_stmt_idents(b: &[Stmt], out: &mut Vec<String>) {
    for s in b {
        match &s.kind {
            StmtKind::Expr(e) => collect_idents(e, out),
            StmtKind::Assign { lhs, rhs, .. } => {
                for l in lhs {
                    if let LValue::Expr(e) = l {
                        collect_idents(e, out);
                    }
                }
                collect_idents(rhs, out);
            }
            StmtKind::OpAssign { lhs, rhs, .. } => {
                collect_idents(lhs, out);
                collect_idents(rhs, out);
            }
            StmtKind::IncDec { lhs, .. } => collect_idents(lhs, out),
            StmtKind::Command { .. } | StmtKind::Break | StmtKind::Continue | StmtKind::Return => {}
            StmtKind::If { clauses, else_body } => {
                for c in clauses {
                    collect_idents(&c.cond, out);
                    collect_stmt_idents(&c.body, out);
                }
                if let Some(b) = else_body {
                    collect_stmt_idents(b, out);
                }
            }
            StmtKind::For { var, iter, body } => {
                collect_idents(var, out);
                collect_idents(iter, out);
                collect_stmt_idents(body, out);
            }
            StmtKind::ForKeyValue {
                value,
                key,
                iter,
                body,
            } => {
                out.push(value.name.clone());
                out.push(key.name.clone());
                collect_idents(iter, out);
                collect_stmt_idents(body, out);
            }
            StmtKind::Parfor {
                var, iter, body, ..
            } => {
                out.push(var.name.clone());
                collect_idents(iter, out);
                collect_stmt_idents(body, out);
            }
            StmtKind::While { cond, body } | StmtKind::DoUntil { body, cond } => {
                collect_idents(cond, out);
                collect_stmt_idents(body, out);
            }
            StmtKind::Switch {
                subject,
                cases,
                otherwise,
            } => {
                collect_idents(subject, out);
                for c in cases {
                    collect_idents(&c.value, out);
                    collect_stmt_idents(&c.body, out);
                }
                if let Some(b) = otherwise {
                    collect_stmt_idents(b, out);
                }
            }
            StmtKind::Try {
                body,
                catch_var,
                catch_body,
            } => {
                collect_stmt_idents(body, out);
                if let Some(v) = catch_var {
                    out.push(v.name.clone());
                }
                if let Some(b) = catch_body {
                    collect_stmt_idents(b, out);
                }
            }
            StmtKind::UnwindProtect { body, cleanup } => {
                collect_stmt_idents(body, out);
                collect_stmt_idents(cleanup, out);
            }
            StmtKind::Spmd { body } => collect_stmt_idents(body, out),
            StmtKind::Decl { names, .. } => {
                for (n, e) in names {
                    out.push(n.name.clone());
                    if let Some(e) = e {
                        collect_idents(e, out);
                    }
                }
            }
        }
    }
}

pub(crate) fn collect_idents(e: &Expr, out: &mut Vec<String>) {
    match &e.kind {
        ExprKind::Ident(n) => out.push(n.clone()),
        ExprKind::Unary { operand, .. } => collect_idents(operand, out),
        ExprKind::Binary { lhs, rhs, .. } => {
            collect_idents(lhs, out);
            collect_idents(rhs, out);
        }
        ExprKind::Range { start, step, stop } => {
            collect_idents(start, out);
            if let Some(s) = step {
                collect_idents(s, out);
            }
            collect_idents(stop, out);
        }
        ExprKind::Paren(x) => collect_idents(x, out),
        ExprKind::Index { target, args } | ExprKind::CellIndex { target, args } => {
            collect_idents(target, out);
            for a in args {
                collect_idents(a, out);
            }
        }
        ExprKind::Field { target, .. } => collect_idents(target, out),
        ExprKind::DynField { target, name } => {
            collect_idents(target, out);
            collect_idents(name, out);
        }
        ExprKind::Matrix(rows) | ExprKind::Cell(rows) => {
            for r in rows {
                for x in r {
                    collect_idents(x, out);
                }
            }
        }
        ExprKind::AnonFn { body, .. } => collect_idents(body, out),
        ExprKind::Assign { lhs, rhs, .. } => {
            collect_idents(lhs, out);
            collect_idents(rhs, out);
        }
        ExprKind::PostIncDec { target, .. } => collect_idents(target, out),
        ExprKind::NamedArg { value, .. } => collect_idents(value, out),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lazy_ranges_match_materialised_ones() {
        for (a, d, b) in [
            (1.0, 1.0, 2_000_000.0),
            (0.0, 0.1, 2e5),
            (5e5, -0.25, -1.0),
            (1.0, 1.0 / 3.0, 400_000.0),
        ] {
            let n = lazy_range_len(a, d, b).unwrap();
            let vals = ops::range(a, d, b).unwrap();
            assert_eq!(n, vals.len());
            for (k, v) in vals.iter().enumerate() {
                assert_eq!(lazy_range_at(a, d, b, k, n).to_bits(), v.to_bits());
            }
        }
        assert_eq!(lazy_range_len(1.0, 1.0, 10.0), None);
        assert_eq!(lazy_range_len(1.0, 0.0, 1e9), None);
        assert_eq!(lazy_range_len(1.0, 1.0, 1e12), None);
        assert_eq!(lazy_range_len(1.0, 1.0, 1e9), Some(1_000_000_000));
    }
}
