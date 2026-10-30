//! Handle utilities: `isfigure`, application data (`setappdata`,
//! `getappdata`, `isappdata`, `rmappdata`), `UserData`, `ancestor`, `gco`,
//! and dot access on handle doubles (`h.Color`, `h.Color = c`,
//! `ax.XLim(2)`, `h.Parent.Type`) through the interpreter's
//! [`Interp::handle_field_get`] / [`Interp::handle_field_set`] hooks.
//!
//! Application data and `UserData` are interpreter values stored on the
//! model object ([`indy_graphics::AppData`]), so they go away with it
//! (`delete`, `close`, `clf`, a reused figure number starts empty) and
//! survive `set`. Argument rules and error texts follow Octave 11.3 measured
//! as a black box (`octave-cli`).

use std::rc::Rc;

use indy_graphics::Handle;
use indy_interp::builtins::err;
use indy_interp::{EOp, Interp, R, Value, assign_path};
use indy_value::StructArray;
use indy_value::index::{index_value, to_sub};

use crate::builtins::{bad, get_value, num, set};
use crate::convert::handles_of;
use crate::graphics;

/// The live handles in `v` (every element), or `None`.
fn live(it: &mut Interp, v: &Value) -> Option<Vec<Handle>> {
    let hs = handles_of(v)?;
    let g = graphics(it);
    (!hs.is_empty() && hs.iter().all(|h| g.is_valid(*h))).then_some(hs)
}

/// Handles for the appdata functions: a numeric scalar or vector of live
/// graphics handles (Octave's rule and message).
fn appdata_targets(it: &mut Interp, v: Option<&Value>, fname: &str) -> R<Vec<Handle>> {
    let hs = v.and_then(|v| {
        let vector = v.dims().iter().filter(|d| **d != 1).count() <= 1;
        if vector { live(it, v) } else { None }
    });
    hs.ok_or_else(|| {
        err(
            "MATLAB:hg:InvalidHandle",
            format!("{fname}: H must be a scalar or vector of graphic handles"),
        )
    })
}

fn text_arg(v: &Value) -> Option<String> {
    if v.is_string_row() || matches!(v, Value::Str(a) if a.data.len() == 1) {
        v.to_str()
    } else {
        None
    }
}

fn cellstr(v: &Value) -> Option<Vec<String>> {
    match v {
        Value::Cell(c) => c.data.iter().map(text_arg).collect(),
        _ => None,
    }
}

fn appdata_value(it: &mut Interp, h: Handle, name: &str) -> R<Option<Value>> {
    let ad = graphics(it).appdata(h).map_err(crate::convert::gerr)?;
    Ok(ad
        .get(name)
        .and_then(|v| v.downcast_ref::<Value>())
        .cloned())
}

/// `isfigure(h)`: true where `h` is a live figure handle (same size as
/// `h`; false for anything not numeric).
pub(crate) fn isfigure(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    if a.len() != 1 {
        return Err(bad("isfigure", "expected one input argument"));
    }
    let g = graphics(it);
    let ok = |x: f64| {
        Handle::from_value(x).is_some_and(|h| h.is_figure() && g.type_of(h) == Ok("figure"))
    };
    match &a[0] {
        Value::Num(n) => Ok(vec![Value::Bool(indy_value::Array::new(
            n.dims.clone(),
            n.re.iter().map(|x| ok(*x)).collect(),
        ))]),
        _ => Ok(vec![Value::bool(false)]),
    }
}

/// `setappdata(h, name, value, ...)` or `setappdata(h, {names}, {values})`.
pub(crate) fn setappdata(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let usage = || {
        err(
            "Octave:invalid-fun-call",
            "Invalid call to setappdata: expected setappdata(H, NAME, VALUE, ...) \
             or setappdata(H, {NAME1, ...}, {VALUE1, ...})",
        )
    };
    if a.len() < 3 || a.len().is_multiple_of(2) {
        return Err(usage());
    }
    let hs = appdata_targets(it, a.first(), "setappdata")?;
    let mut entries: Vec<(String, Value)> = Vec::new();
    if a.len() == 3
        && let (Some(names), Value::Cell(values)) = (cellstr(&a[1]), &a[2])
    {
        if names.len() != values.data.len() {
            return Err(bad(
                "setappdata",
                "the NAME and VALUE cell arrays must have the same number of elements",
            ));
        }
        entries.extend(names.into_iter().zip(values.data.iter().cloned()));
    } else {
        for pair in a[1..].chunks(2) {
            let name = text_arg(&pair[0]).ok_or_else(|| {
                err(
                    "MATLAB:hg:InvalidArgument",
                    "setappdata: NAME must be a string or cellstr",
                )
            })?;
            entries.push((name, pair[1].clone()));
        }
    }
    let g = graphics(it);
    for h in hs {
        let ad = g.appdata_mut(h).map_err(crate::convert::gerr)?;
        for (name, v) in &entries {
            ad.set(name, Rc::new(v.clone()));
        }
    }
    Ok(vec![])
}

/// `getappdata(h, name)` (`[]` when absent; a cell column for several
/// handles) or `getappdata(h)`: a struct of every entry.
pub(crate) fn getappdata(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    if a.is_empty() || a.len() > 2 {
        return Err(bad(
            "getappdata",
            "expected getappdata(H) or getappdata(H, NAME)",
        ));
    }
    let hs = appdata_targets(it, a.first(), "getappdata")?;
    let Some(name) = a.get(1) else {
        if hs.len() != 1 {
            return Err(bad("getappdata", "getappdata(H) needs a single handle"));
        }
        let ad = graphics(it).appdata(hs[0]).map_err(crate::convert::gerr)?;
        let mut s = StructArray::new_scalar();
        for (k, v) in &ad.entries {
            if let Some(v) = v.downcast_ref::<Value>() {
                s.set(0, k, v.clone());
            }
        }
        return Ok(vec![Value::Struct(s)]);
    };
    let name = text_arg(name).ok_or_else(|| {
        err(
            "MATLAB:hg:InvalidArgument",
            "getappdata: NAME must be a string",
        )
    })?;
    let mut out = Vec::with_capacity(hs.len());
    for h in &hs {
        out.push(appdata_value(it, *h, &name)?.unwrap_or_default());
    }
    if out.len() == 1 {
        return Ok(out);
    }
    Ok(vec![Value::cell_col(out)])
}

/// `isappdata(h, name)`: logical, one per handle.
pub(crate) fn isappdata(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    if a.len() != 2 {
        return Err(bad("isappdata", "expected isappdata(H, NAME)"));
    }
    let hs = appdata_targets(it, a.first(), "isappdata")?;
    let name = text_arg(&a[1]).ok_or_else(|| {
        err(
            "MATLAB:hg:InvalidArgument",
            "isappdata: NAME must be a string",
        )
    })?;
    let g = graphics(it);
    let mut data = Vec::with_capacity(hs.len());
    for h in hs {
        let ad = g.appdata(h).map_err(crate::convert::gerr)?;
        data.push(ad.get(&name).is_some());
    }
    Ok(vec![Value::Bool(indy_value::Array::new(a[0].dims(), data))])
}

/// `rmappdata(h, name, ...)`: every name must be present.
pub(crate) fn rmappdata(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    if a.len() < 2 {
        return Err(bad("rmappdata", "expected rmappdata(H, NAME, ...)"));
    }
    let hs = appdata_targets(it, a.first(), "rmappdata")?;
    let names = a[1..]
        .iter()
        .map(text_arg)
        .collect::<Option<Vec<String>>>()
        .ok_or_else(|| {
            err(
                "MATLAB:hg:InvalidArgument",
                "rmappdata: NAME must be a string",
            )
        })?;
    let g = graphics(it);
    for h in hs {
        let ad = g.appdata_mut(h).map_err(crate::convert::gerr)?;
        for name in &names {
            if !ad.remove(name) {
                return Err(err(
                    "MATLAB:hg:InvalidArgument",
                    format!("rmappdata: appdata '{name}' is not present"),
                ));
            }
        }
    }
    Ok(vec![])
}

/// `get(h, 'UserData')`: `[]` until set.
pub(crate) fn user_data(it: &mut Interp, h: Handle) -> R<Value> {
    let ad = graphics(it).appdata(h).map_err(crate::convert::gerr)?;
    Ok(ad
        .user_data
        .as_ref()
        .and_then(|v| v.downcast_ref::<Value>())
        .cloned()
        .unwrap_or_default())
}

/// Splits the `UserData` pairs (the last one wins) off a `set` argument
/// list; the other arguments are returned unchanged.
pub(crate) fn split_user_data(
    it: &mut Interp,
    hs: &[Handle],
    rest: &[Value],
) -> R<(Option<Value>, Vec<Value>)> {
    if !rest.len().is_multiple_of(2) {
        return Ok((None, rest.to_vec()));
    }
    let mut user = None;
    let mut other = Vec::with_capacity(rest.len());
    for pair in rest.chunks(2) {
        if text_arg(&pair[0]).is_some_and(|n| n.eq_ignore_ascii_case("UserData")) {
            user = Some(pair[1].clone());
        } else {
            other.extend_from_slice(pair);
        }
    }
    if user.is_some() {
        let g = graphics(it);
        if hs.iter().any(|h| *h == Handle::ROOT || !g.is_valid(*h)) {
            return Err(crate::convert::gerr(
                indy_graphics::GraphicsError::invalid_handle(),
            ));
        }
    }
    Ok((user, other))
}

pub(crate) fn store_user_data(
    it: &mut Interp,
    hs: &[Handle],
    user: Option<Value>,
) -> R<Vec<Value>> {
    if let Some(v) = user {
        let v: Rc<dyn std::any::Any> = Rc::new(v);
        let g = graphics(it);
        for h in hs {
            g.appdata_mut(*h).map_err(crate::convert::gerr)?.user_data = Some(v.clone());
        }
    }
    Ok(vec![])
}

/// `ancestor(h, type)` / `ancestor(h, type, 'toplevel')`: the nearest (or
/// outermost) object of one of the types among `h` itself and its parents;
/// `[]` when there is none or `h` is not a live handle. Several handles
/// give a cell column.
pub(crate) fn ancestor(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    if a.len() < 2 || a.len() > 3 {
        return Err(bad(
            "ancestor",
            "expected ancestor(H, TYPE) or ancestor(H, TYPE, 'toplevel')",
        ));
    }
    let types: Vec<String> = match text_arg(&a[1]) {
        Some(t) => vec![t],
        None => cellstr(&a[1])
            .ok_or_else(|| bad("ancestor", "TYPE must be a string or cell array of strings"))?,
    };
    let toplevel = match a.get(2) {
        None => false,
        Some(v) if text_arg(v).is_some_and(|s| s.eq_ignore_ascii_case("toplevel")) => true,
        Some(_) => return Err(bad("ancestor", "third argument must be \"toplevel\"")),
    };
    let hs = match &a[0] {
        Value::Num(n) => n.re.clone(),
        _ => return Err(bad("ancestor", "H must be graphics handles")),
    };
    if hs.is_empty() {
        return Ok(vec![Value::empty()]);
    }
    let g = graphics(it);
    let find = |x: f64| -> Value {
        let Some(mut h) = Handle::from_value(x).filter(|h| g.is_valid(*h)) else {
            return Value::empty();
        };
        let mut found = None;
        loop {
            let t = g.type_of(h).unwrap_or("");
            if types.iter().any(|w| w.eq_ignore_ascii_case(t)) {
                found = Some(h);
                if !toplevel {
                    break;
                }
            }
            if h == Handle::ROOT {
                break;
            }
            match g.parent_of(h) {
                Ok(p) => h = p,
                Err(_) => break,
            }
        }
        found.map_or_else(Value::empty, |h| num(h.value()))
    };
    let out: Vec<Value> = hs.into_iter().map(find).collect();
    if out.len() == 1 {
        return Ok(out);
    }
    Ok(vec![Value::cell_col(out)])
}

/// `gco` / `gco(fig)`: the figure's current object, which MATLAB sets
/// only when the user clicks an object. Rust engine figures take no clicks,
/// so it is always `[]` (as in Octave without interaction); `gco` never
/// creates a figure.
pub(crate) fn gco(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    match a.as_slice() {
        [] => {}
        [f] => {
            let fig = handles_of(f).filter(|h| h.len() == 1).map(|h| h[0]);
            let g = graphics(it);
            if !fig.is_some_and(|h| h.is_figure() && g.type_of(h) == Ok("figure")) {
                return Err(bad(
                    "gco",
                    "HFIG must be a graphics handle to a figure object",
                ));
            }
        }
        _ => return Err(bad("gco", "too many input arguments")),
    }
    Ok(vec![Value::empty()])
}

// ----- dot access --------------------------------------------------------------

/// [`Interp::handle_field_get`]: `h.Name` on live handles (one value per
/// handle, a comma-separated list for several). A deleted object handle
/// errors as such; any other double keeps the interpreter's own error.
pub(crate) fn field_get(it: &mut Interp, v: &Value, name: &str) -> Option<R<Vec<Value>>> {
    let Some(hs) = dot_handles(it, v)? else {
        return Some(Err(deleted()));
    };
    let mut out = Vec::with_capacity(hs.len());
    for h in hs {
        match get_value(it, h, name) {
            Ok(x) => out.push(x),
            Err(e) => return Some(Err(e)),
        }
    }
    Some(Ok(out))
}

/// [`Interp::handle_field_set`]: `h.Name = v`, `h(k).Name = v`,
/// `h.Name(i) = v` (read, modify, write back) and `h.Parent.Name = v`.
pub(crate) fn field_set(it: &mut Interp, v: &Value, eops: &[EOp], rhs: &Value) -> Option<R<()>> {
    // Whether the variable holds handles is decided on the whole variable;
    // a leading index then picks among them.
    handles_of(v)?;
    let first_field = eops.iter().position(|o| matches!(o, EOp::Field(_)))?;
    let mut target = v.clone();
    for op in &eops[..first_field] {
        match op {
            EOp::Paren(args) => {
                let subs = match args.iter().map(to_sub).collect::<Result<Vec<_>, _>>() {
                    Ok(s) => s,
                    Err(e) => return Some(Err(e)),
                };
                target = match index_value(&target, &subs) {
                    Ok(t) => t,
                    Err(e) => return Some(Err(e)),
                };
            }
            _ => return None,
        }
    }
    match dot_handles(it, &target)? {
        Some(_) => Some(assign_field(it, &target, &eops[first_field..], rhs)),
        None => Some(Err(deleted())),
    }
}

fn assign_field(it: &mut Interp, target: &Value, eops: &[EOp], rhs: &Value) -> R<()> {
    let Some(Some(hs)) = dot_handles(it, target) else {
        return Err(deleted());
    };
    let [h] = hs[..] else {
        return Err(err(
            "MATLAB:hg:InvalidArgument",
            "dot assignment needs a single graphics object; use set(h, name, value) \
             for several",
        ));
    };
    let Some(EOp::Field(name)) = eops.first() else {
        return Err(err(
            "MATLAB:invalidAssignment",
            "Invalid assignment target.",
        ));
    };
    let rest = &eops[1..];
    if rest.is_empty() {
        set(it, vec![num(h.value()), Value::str(name), rhs.clone()], 0)?;
        return Ok(());
    }
    let cur = get_value(it, h, name)?;
    if matches!(rest.first(), Some(EOp::Field(_)))
        && let Some(Some(_)) = dot_handles(it, &cur)
    {
        // `h.Parent.Color = c`: the inner object is changed, not `h`.
        return assign_field(it, &cur, rest, rhs);
    }
    let mut slot = Some(cur);
    assign_path(&mut slot, rest, rhs.clone())?;
    let new = slot.unwrap_or_default();
    set(it, vec![num(h.value()), Value::str(name), new], 0)?;
    Ok(())
}

/// `None`: not a handle value (the interpreter's own error applies);
/// `Some(None)`: a deleted object handle; `Some(Some(hs))`: live handles.
/// Only doubles count, never the root (`0` is far too common a number to
/// read as `groot`), and figure numbers only while that figure is open.
fn dot_handles(it: &mut Interp, v: &Value) -> Option<Option<Vec<Handle>>> {
    let Value::Num(n) = v else { return None };
    if n.class != indy_value::NumClass::Double || n.im.is_some() || n.re.is_empty() {
        return None;
    }
    let hs = handles_of(v)?;
    if hs.contains(&Handle::ROOT) {
        return None;
    }
    let g = graphics(it);
    if hs.iter().all(|h| g.is_valid(*h)) {
        return Some(Some(hs));
    }
    if hs.iter().all(|h| g.is_valid(*h) || g.was_object(*h)) {
        return Some(None);
    }
    None
}

fn deleted() -> Box<indy_interp::Error> {
    err("MATLAB:class:InvalidHandle", "Invalid or deleted object.")
}

/// `allchild(h)`: the children including hidden ones (an axes' title and
/// labels, after its plotted children: title, zlabel, ylabel, xlabel, as
/// Octave lists them). Several handles give a cell column.
pub(crate) fn allchild(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    if a.len() != 1 {
        return Err(bad("allchild", "expected allchild(H)"));
    }
    let hs = match &a[0] {
        Value::Num(n) if n.re.is_empty() => return Ok(vec![Value::empty()]),
        v => handles_of(v).ok_or_else(|| bad("allchild", "H must be graphics handles"))?,
    };
    let g = graphics(it);
    let mut out = Vec::with_capacity(hs.len());
    for h in &hs {
        if !g.is_valid(*h) {
            return Err(crate::convert::gerr(
                indy_graphics::GraphicsError::invalid_handle(),
            ));
        }
        let mut list = match g.get(*h, "Children").map_err(crate::convert::gerr)? {
            indy_graphics::PropValue::Handles(c) => c,
            _ => Vec::new(),
        };
        if g.type_of(*h) == Ok("axes") {
            for name in ["Title", "ZLabel", "YLabel", "XLabel"] {
                if let Ok(indy_graphics::PropValue::Handle(l)) = g.get(*h, name) {
                    list.push(l);
                }
            }
        }
        out.push(if list.is_empty() {
            Value::empty()
        } else {
            crate::convert::handles_value(&list)
        });
    }
    if out.len() == 1 {
        return Ok(out);
    }
    Ok(vec![Value::cell_col(out)])
}
