//! Indexed assignment: `x(i).f{k}(j) = v`, growth, deletion.
//!
//! Works in place on a slot (`None` = undefined variable) so that a loop
//! of `x(k) = v` or `s.data(k) = v` does not copy the array.

use indy_value::index::{assign_index, delete_index, delete_positions, write_positions};
use indy_value::{Error, StructArray, Value};

use crate::R;
use crate::eval::{EOp, subs_of};

fn is_deletion(v: &Value) -> bool {
    matches!(v, Value::Num(n) if n.re.is_empty() && n.dims == [0, 0] && n.class == indy_value::NumClass::Double)
}

/// Position of a single element selected by `subs` (growing allowed),
/// or `None` when the subscripts select several elements.
fn single_pos(dims: &[usize], subs: &[indy_value::index::Sub]) -> R<Option<(usize, Vec<usize>)>> {
    let (pos, nd) = write_positions(dims, subs, &[1, 1])?;
    if pos.len() == 1 {
        Ok(Some((pos[0], nd)))
    } else {
        Ok(None)
    }
}

pub fn assign_path(slot: &mut Option<Value>, ops: &[EOp], rhs: Value) -> R<()> {
    let Some(op) = ops.first() else {
        *slot = Some(rhs);
        return Ok(());
    };
    let rest = &ops[1..];
    if let Some(v) = slot.as_ref()
        && crate::builtins::datatypes::is_native(v)
    {
        let v = slot.take().unwrap();
        *slot = Some(crate::builtins::datatypes::assign(v, ops, rhs)?);
        return Ok(());
    }
    match op {
        EOp::Paren(args) => {
            let subs = subs_of(args)?;
            if rest.is_empty() {
                if is_deletion(&rhs)
                    && let Some(v) = slot.as_ref()
                    && !matches!(v, Value::Func(_) | Value::Object(_))
                {
                    delete_positions(v.dims_ref(), &subs)?;
                    let v = slot.take().unwrap();
                    *slot = Some(delete_index(v, &subs)?);
                    return Ok(());
                }
                if let Some(Value::Func(_)) = slot {
                    return Err(Error::new(
                        "MATLAB:index:assignToHandle",
                        "Indexed assignment into a function handle is not allowed.",
                    ));
                }
                return assign_index(slot, &subs, rhs);
            }
            // s(k).f... = v : work on one element.
            let dims = slot
                .as_ref()
                .map(|v| v.dims())
                .unwrap_or_else(|| vec![0, 0]);
            let Some((p, nd)) = single_pos(&dims, &subs)? else {
                return Err(Error::new(
                    "MATLAB:index:multipleTargets",
                    "Assignment to multiple elements through a nested index is not supported (use a loop or deal).",
                ));
            };
            // In-place fast path: existing struct element and a field next.
            if let (Some(Value::Struct(s)), EOp::Field(fname)) = (slot.as_mut(), &rest[0])
                && p < s.elems.len()
                && nd == s.dims
            {
                let f = s.ensure_field(fname);
                let mut sub = Some(std::mem::take(&mut s.elems[p][f]));
                let r = assign_path(&mut sub, &rest[1..], rhs);
                s.elems[p][f] = sub.unwrap_or_default();
                return r;
            }
            // General: read the element (or a fresh one), recurse, write back.
            let elem = match slot.as_ref() {
                Some(v) if p < v.numel() && nd == v.dims() => {
                    Some(indy_value::index::index_value(v, &subs)?)
                }
                Some(Value::Struct(s)) => Some(Value::Struct(StructArray::empty_with(
                    s.fields.clone(),
                    vec![1, 1],
                ))),
                _ => None,
            };
            let mut sub = elem;
            assign_path(&mut sub, rest, rhs)?;
            let sub = sub.unwrap_or_default();
            assign_index(slot, &subs, sub)
        }
        EOp::Brace(args) => {
            let subs = subs_of(args)?;
            if let Some(Value::Str(_)) = slot
                && rest.is_empty()
            {
                let text = rhs.to_str().ok_or_else(|| {
                    Error::new(
                        "MATLAB:string:braceAssign",
                        "Brace assignment into a string array needs text.",
                    )
                })?;
                return assign_index(slot, &subs, Value::string(&text));
            }
            match slot {
                None => *slot = Some(Value::empty_cell()),
                Some(Value::Num(n)) if n.re.is_empty() => *slot = Some(Value::empty_cell()),
                Some(Value::Cell(_)) => {}
                Some(other) => {
                    return Err(Error::new(
                        "MATLAB:cellAssToNonCell",
                        format!(
                            "Unable to perform assignment because brace indexing is not supported for variables of this type ({}).",
                            other.class_name()
                        ),
                    ));
                }
            }
            let Some(Value::Cell(c)) = slot.as_mut() else {
                unreachable!()
            };
            let (pos, nd) = write_positions(&c.dims, &subs, &[1, 1])?;
            if pos.len() != 1 {
                if rest.is_empty() {
                    // c{1:2} = v: MATLAB requires deal; assign to each.
                    let old = c.dims.clone();
                    c.data = indy_value::index::grow(
                        std::mem::take(&mut c.data),
                        &old,
                        &nd,
                        Value::default(),
                    );
                    c.dims = nd;
                    for p in pos {
                        c.data[p] = rhs.clone();
                    }
                    return Ok(());
                }
                return Err(Error::new(
                    "MATLAB:index:multipleTargets",
                    "Brace assignment with several targets needs a cs-list on the right (deal).",
                ));
            }
            let p = pos[0];
            if nd != c.dims {
                let old = c.dims.clone();
                c.data = indy_value::index::grow(
                    std::mem::take(&mut c.data),
                    &old,
                    &nd,
                    Value::default(),
                );
                c.dims = nd;
            }
            if rest.is_empty() {
                c.data[p] = rhs;
                return Ok(());
            }
            let mut sub = Some(std::mem::take(&mut c.data[p]));
            let r = assign_path(&mut sub, rest, rhs);
            c.data[p] = sub.unwrap_or_default();
            r
        }
        EOp::Field(name) => {
            match slot {
                None => *slot = Some(Value::Struct(StructArray::new_scalar())),
                Some(Value::Num(n)) if n.re.is_empty() => {
                    *slot = Some(Value::Struct(StructArray::new_scalar()))
                }
                _ => {}
            }
            match slot.as_mut().unwrap() {
                Value::Struct(s) => {
                    if s.numel() != 1 {
                        if s.numel() == 0 && rest.is_empty() {
                            // s = struct([]) ; s.f = v  -> 1x1
                            let mut ns = StructArray::empty_with(s.fields.clone(), vec![1, 1]);
                            ns.set(0, name, rhs);
                            *s = ns;
                            return Ok(());
                        }
                        return Err(Error::new(
                            "MATLAB:index:scalarStructRequired",
                            "Scalar structure required for this assignment.",
                        ));
                    }
                    let f = s.ensure_field(name);
                    if rest.is_empty() {
                        s.elems[0][f] = rhs;
                        return Ok(());
                    }
                    let mut sub = Some(std::mem::take(&mut s.elems[0][f]));
                    let r = assign_path(&mut sub, rest, rhs);
                    s.elems[0][f] = sub.unwrap_or_default();
                    r
                }
                Value::Object(o) => {
                    if rest.is_empty() {
                        o.set(name, rhs);
                        return Ok(());
                    }
                    let mut sub = o.get(name).cloned();
                    assign_path(&mut sub, rest, rhs)?;
                    o.set(name, sub.unwrap_or_default());
                    Ok(())
                }
                other => Err(Error::new(
                    "MATLAB:structAssToNonStruct",
                    format!(
                        "Unable to perform assignment because dot indexing is not supported for variables of this type ({}).",
                        other.class_name()
                    ),
                )),
            }
        }
    }
}
