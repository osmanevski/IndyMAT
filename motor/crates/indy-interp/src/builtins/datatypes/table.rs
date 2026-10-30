//! `table`: heterogeneous column-oriented data (MATLAB semantics).
//!
//! Storage (`_`-props of the object): `_names` (cell row of variable
//! names), `_vars` (cell row of variable values, each with `_h` rows),
//! `_rows` (cell column of row names, or empty), `_h` (height) and
//! `_meta` (scalar struct: Description, UserData, DimensionNames,
//! VariableDescriptions, VariableUnits, VariableContinuity,
//! CustomProperties). The object's dims are `[height, width]`.

use std::cmp::Ordering;

use indy_syntax::ast::BinOp;
use indy_value::index::{Sub, index_value};
use indy_value::{Array, Error, Num, NumClass, Object, StructArray, Value, format};

use super::{
    ClassOps, bools, cat as dt_cat, cellstr_col, cellstr_row, class_is, is_native, name_values,
    natives_equal, new_obj, obj_of, text_list,
};
use crate::builtins::{err, need, reg};
use crate::eval::EOp;
use crate::{BuiltinFn, Interp, R};

pub static OPS: ClassOps = ClassOps {
    name: "table",
    get: |_, n| Err(no_var(n)),
    set: |_, n, _| Err(no_var(n)),
    convert: |_, v| match v {
        Value::Object(o) if o.class == "table" => Ok((**o).clone()),
        _ => Err(err(
            "MATLAB:table:InvalidConcatenation",
            "Tables can only be concatenated with tables or cell arrays.",
        )),
    },
    binary,
    disp: |o| disp_text(&Tab::from_obj(o)),
    display: Some(display),
    fill: |_| 0.0,
    methods: METHODS,
};

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Tab {
    pub names: Vec<String>,
    pub vars: Vec<Value>,
    pub rows: Vec<String>,
    pub h: usize,
    pub desc: Value,
    pub ud: Value,
    pub dimnames: [String; 2],
    pub units: Vec<String>,
    pub vdesc: Vec<String>,
    pub cont: Value,
    pub custom: Value,
}

impl Tab {
    pub fn new(h: usize) -> Tab {
        Tab {
            names: Vec::new(),
            vars: Vec::new(),
            rows: Vec::new(),
            h,
            desc: Value::str(""),
            ud: Value::empty(),
            dimnames: ["Row".into(), "Variables".into()],
            units: Vec::new(),
            vdesc: Vec::new(),
            cont: Value::empty(),
            custom: Value::Struct(StructArray::new_scalar()),
        }
    }

    pub fn from_obj(o: &Object) -> Tab {
        let list = |k: &str| o.get(k).and_then(text_list).unwrap_or_default();
        let vars = match o.get("_vars") {
            Some(Value::Cell(c)) => c.data.clone(),
            _ => Vec::new(),
        };
        let meta = match o.get("_meta") {
            Some(Value::Struct(s)) => s.clone(),
            _ => StructArray::new_scalar(),
        };
        let m = |k: &str| meta.get(0, k).cloned();
        let dn = m("DimensionNames")
            .and_then(|v| text_list(&v))
            .unwrap_or_default();
        Tab {
            names: list("_names"),
            vars,
            rows: list("_rows"),
            h: o.get("_h").and_then(|v| v.scalar_f64()).unwrap_or(0.0) as usize,
            desc: m("Description").unwrap_or_else(|| Value::str("")),
            ud: m("UserData").unwrap_or_default(),
            dimnames: [
                dn.first().cloned().unwrap_or_else(|| "Row".into()),
                dn.get(1).cloned().unwrap_or_else(|| "Variables".into()),
            ],
            units: m("VariableUnits")
                .and_then(|v| text_list(&v))
                .unwrap_or_default(),
            vdesc: m("VariableDescriptions")
                .and_then(|v| text_list(&v))
                .unwrap_or_default(),
            cont: m("VariableContinuity").unwrap_or_default(),
            custom: m("CustomProperties")
                .unwrap_or_else(|| Value::Struct(StructArray::new_scalar())),
        }
    }

    pub fn to_obj(&self) -> Object {
        let mut o = new_obj("table", vec![self.h, self.vars.len()]);
        o.set("_names", cellstr_row(&self.names));
        o.set(
            "_vars",
            Value::Cell(Array::new(vec![1, self.vars.len()], self.vars.clone())),
        );
        o.set(
            "_rows",
            if self.rows.is_empty() {
                Value::empty_cell()
            } else {
                cellstr_col(&self.rows)
            },
        );
        o.set("_h", Value::num(self.h as f64));
        let mut s = StructArray::new_scalar();
        s.set(0, "Description", self.desc.clone());
        s.set(0, "UserData", self.ud.clone());
        s.set(0, "DimensionNames", cellstr_row(&self.dimnames));
        s.set(0, "VariableDescriptions", cellstr_row(&self.vdesc));
        s.set(0, "VariableUnits", cellstr_row(&self.units));
        s.set(0, "VariableContinuity", self.cont.clone());
        s.set(0, "CustomProperties", self.custom.clone());
        o.set("_meta", Value::Struct(s));
        o
    }

    pub fn value(&self) -> Value {
        Value::Object(Box::new(self.to_obj()))
    }

    pub fn width(&self) -> usize {
        self.vars.len()
    }

    /// `T.Properties` as a struct (MATLAB's TableProperties fields).
    pub fn properties(&self) -> Value {
        let mut s = StructArray::new_scalar();
        s.set(0, "Description", self.desc.clone());
        s.set(0, "UserData", self.ud.clone());
        s.set(0, "DimensionNames", cellstr_row(&self.dimnames));
        s.set(0, "VariableNames", cellstr_row(&self.names));
        s.set(0, "VariableDescriptions", cell_or_empty(&self.vdesc));
        s.set(0, "VariableUnits", cell_or_empty(&self.units));
        s.set(0, "VariableContinuity", self.cont.clone());
        s.set(
            0,
            "RowNames",
            if self.rows.is_empty() {
                Value::Cell(Array::new(vec![0, 1], vec![]))
            } else {
                cellstr_col(&self.rows)
            },
        );
        s.set(0, "CustomProperties", self.custom.clone());
        Value::Struct(s)
    }

    pub fn var_index(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }

    /// Add or replace variable `name`; checks the height.
    pub fn set_var(&mut self, name: &str, v: Value) -> R<()> {
        let v = self.fit_height(v, name)?;
        match self.var_index(name) {
            Some(k) => self.vars[k] = v,
            None => {
                check_name(name)?;
                self.names.push(name.to_string());
                self.vars.push(v);
                pad_meta(&mut self.units, self.names.len() - 1);
                pad_meta(&mut self.vdesc, self.names.len() - 1);
            }
        }
        Ok(())
    }

    /// Rows of `v` must match the height (a table without variables takes
    /// the new height; scalars expand).
    fn fit_height(&mut self, v: Value, name: &str) -> R<Value> {
        let vh = height_of(&v);
        if self.vars.is_empty() && self.rows.is_empty() {
            self.h = vh;
            return Ok(v);
        }
        if vh == self.h {
            return Ok(v);
        }
        if v.numel() == 1 && !matches!(v, Value::Cell(_)) {
            let idx = Value::Num(Num::real(vec![self.h, 1], vec![1.0; self.h]));
            return rows_of(&v, &idx_list(&idx));
        }
        Err(err(
            "MATLAB:table:RowDimensionMismatch",
            format!(
                "To assign to or create a variable in a table, the number of rows must match the height of the table (variable '{}' has {} rows, the table has {}).",
                name, vh, self.h
            ),
        ))
    }

    pub fn take_rows(&self, rows: &[usize]) -> R<Tab> {
        let mut t = self.clone();
        t.vars = self
            .vars
            .iter()
            .map(|v| rows_of(v, rows))
            .collect::<R<Vec<_>>>()?;
        if !self.rows.is_empty() {
            t.rows = rows.iter().map(|&r| self.rows[r].clone()).collect();
        }
        t.h = rows.len();
        Ok(t)
    }

    pub fn take_vars(&self, vars: &[usize]) -> Tab {
        let mut t = self.clone();
        t.names = vars.iter().map(|&k| self.names[k].clone()).collect();
        t.vars = vars.iter().map(|&k| self.vars[k].clone()).collect();
        t.units = pick_meta(&self.units, vars);
        t.vdesc = pick_meta(&self.vdesc, vars);
        t
    }
}

fn pad_meta(m: &mut Vec<String>, n: usize) {
    if !m.is_empty() {
        m.resize(n + 1, String::new());
    }
}

fn pick_meta(m: &[String], vars: &[usize]) -> Vec<String> {
    if m.is_empty() {
        return Vec::new();
    }
    vars.iter()
        .map(|&k| m.get(k).cloned().unwrap_or_default())
        .collect()
}

fn cell_or_empty(m: &[String]) -> Value {
    if m.is_empty() {
        Value::Cell(Array::new(vec![1, 0], vec![]))
    } else {
        cellstr_row(m)
    }
}

pub fn tab_of(v: &Value) -> Option<Tab> {
    match v {
        Value::Object(o) if o.class == "table" && o.meta.is_none() => Some(Tab::from_obj(o)),
        _ => None,
    }
}

fn arg_tab(v: &Value, fname: &str) -> R<Tab> {
    tab_of(v).ok_or_else(|| {
        err(
            "MATLAB:invalidType",
            format!("{}: expected a table, got {}.", fname, v.class_name()),
        )
    })
}

fn no_var(n: &str) -> Box<Error> {
    err(
        "MATLAB:table:UnrecognizedVarName",
        format!("Unrecognized table variable name '{}'.", n),
    )
}

fn check_name(n: &str) -> R<()> {
    if n.is_empty() {
        return Err(err(
            "MATLAB:table:VariableNameNotValidIdentifier",
            "Table variable names must be nonempty.",
        ));
    }
    if matches!(n, "Properties" | "RowNames" | "VariableNames" | ":") {
        return Err(err(
            "MATLAB:table:ReservedVarName",
            format!("'{}' is a reserved table variable name.", n),
        ));
    }
    Ok(())
}

fn check_unique(names: &[String]) -> R<()> {
    for (i, n) in names.iter().enumerate() {
        check_name(n)?;
        if names[..i].contains(n) {
            return Err(err(
                "MATLAB:table:DuplicateVarNames",
                format!("Duplicate table variable name: '{}'.", n),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Row helpers for any variable value
// ---------------------------------------------------------------------------

pub fn height_of(v: &Value) -> usize {
    v.dims()[0]
}

fn idx_list(v: &Value) -> Vec<usize> {
    v.to_f64_vec()
        .unwrap_or_default()
        .iter()
        .map(|x| *x as usize - 1)
        .collect()
}

fn idx_value(rows: &[usize]) -> Value {
    Value::Num(Num::real(
        vec![rows.len(), 1],
        rows.iter().map(|r| (*r + 1) as f64).collect(),
    ))
}

/// `v(rows, :, ...)` for plain values and native objects.
pub fn rows_of(v: &Value, rows: &[usize]) -> R<Value> {
    if let Some(o) = obj_of(v) {
        if o.class == "table" {
            return Ok(Tab::from_obj(o).take_rows(rows)?.value());
        }
        let d = o.dims.clone();
        let sub = Sub::List {
            idx: rows.to_vec(),
            shape: vec![rows.len(), 1],
        };
        let r = super::ew_index(o, &[sub, Sub::Colon])?;
        return Ok(Value::Object(Box::new(reshape_rows(r, &d, rows.len()))));
    }
    let d = v.dims();
    let sub = Sub::List {
        idx: rows.to_vec(),
        shape: vec![rows.len(), 1],
    };
    let r = index_value(v, &[sub, Sub::Colon])?;
    if d.len() > 2 {
        let mut nd = d.clone();
        nd[0] = rows.len();
        return Ok(r.reshaped(nd));
    }
    Ok(r)
}

fn reshape_rows(mut o: Object, d: &[usize], n: usize) -> Object {
    if d.len() > 2 {
        let mut nd = d.to_vec();
        nd[0] = n;
        for (k, v) in o.props.iter_mut() {
            if k.starts_with('#') {
                *v = std::mem::take(v).reshaped(nd.clone());
            }
        }
        o.dims = nd;
    }
    o
}

/// `v(rows, :) = rhs` (grows with default fill).
fn set_rows(v: Value, rows: &[usize], rhs: Value) -> R<Value> {
    if let Some(t) = tab_of(&v) {
        let r = tab_of(&rhs).ok_or_else(|| {
            err(
                "MATLAB:table:InvalidRHS",
                "A table variable that is a table needs a table on the right.",
            )
        })?;
        let mut t = t;
        let all: Vec<usize> = (0..t.width()).collect();
        assign_rows_from(&mut t, rows, &all, &r)?;
        return Ok(t.value());
    }
    let max = rows.iter().map(|r| r + 1).max().unwrap_or(0);
    let v = if max > height_of(&v) {
        pad_rows(v, max)?
    } else {
        v
    };
    let mut slot = Some(v);
    let ops = [EOp::Paren(vec![idx_value(rows), Value::str(":")])];
    crate::assign::assign_path(&mut slot, &ops, rhs)?;
    Ok(slot.unwrap_or_default())
}

/// Grow `v` to `h` rows with MATLAB's default fill values.
pub fn pad_rows(v: Value, h: usize) -> R<Value> {
    let d = v.dims();
    let old = d[0];
    if h <= old {
        return Ok(v);
    }
    let mut nd = d.clone();
    nd[0] = h;
    use indy_value::index::grow as g0;
    Ok(match v {
        Value::Num(mut n) => {
            n.re = g0(n.re, &d, &nd, 0.0);
            if let Some(im) = n.im.take() {
                n.im = Some(g0(im, &d, &nd, 0.0));
            }
            n.dims = nd.clone();
            Value::Num(n)
        }
        Value::Bool(a) => Value::Bool(Array::new(nd.clone(), g0(a.data, &d, &nd, false))),
        Value::Char(a) => Value::Char(Array::new(nd.clone(), g0(a.data, &d, &nd, ' '))),
        Value::Str(a) => Value::Str(Array::new(nd.clone(), g0(a.data, &d, &nd, None))),
        Value::Cell(c) => {
            let cellstr = c.data.iter().all(|x| x.is_char());
            let fill = if cellstr {
                Value::str("")
            } else {
                Value::empty()
            };
            Value::Cell(Array::new(nd.clone(), g0(c.data, &d, &nd, fill)))
        }
        Value::Object(o) if is_native(&Value::Object(o.clone())) && o.class != "table" => {
            let ops = super::ops_of(&o.class).unwrap();
            let mut o = *o;
            for (k, x) in o.props.iter_mut() {
                if let (true, Value::Num(n)) = (k.starts_with('#'), x) {
                    n.re =
                        indy_value::index::grow(std::mem::take(&mut n.re), &d, &nd, (ops.fill)(k));
                    n.dims = nd.clone();
                }
            }
            o.dims = nd.clone();
            Value::Object(Box::new(o))
        }
        other => {
            let idx: Vec<usize> = (0..h).map(|r| r.min(old.saturating_sub(1))).collect();
            if old == 0 {
                return Err(err(
                    "MATLAB:table:CannotGrow",
                    format!(
                        "Cannot grow a table variable of class {}.",
                        other.class_name()
                    ),
                ));
            }
            rows_of(&other, &idx)?
        }
    })
}

// ---------------------------------------------------------------------------
// Subscripts
// ---------------------------------------------------------------------------

fn is_colon(v: &Value) -> bool {
    matches!(v, Value::Char(a) if a.data.len() == 1 && a.data[0] == ':')
}

/// Row positions from a subscript (numbers, logical mask, `:`, row names).
fn row_positions(t: &Tab, v: &Value, grow: bool) -> R<Vec<usize>> {
    if is_colon(v) {
        return Ok((0..t.h).collect());
    }
    if v.is_text() || matches!(v, Value::Cell(_)) {
        let names = text_list(v).ok_or_else(|| {
            err(
                "MATLAB:table:InvalidRowSubscript",
                "Row subscripts must be numbers, logicals or row names.",
            )
        })?;
        return names
            .iter()
            .map(|n| {
                t.rows.iter().position(|r| r == n).ok_or_else(|| {
                    err(
                        "MATLAB:table:UnrecognizedRowName",
                        format!("Unrecognized row name '{}'.", n),
                    )
                })
            })
            .collect();
    }
    let s = indy_value::index::to_sub(v)?;
    let Sub::List { idx, .. } = s else {
        return Ok((0..t.h).collect());
    };
    if let Value::Bool(b) = v
        && b.data.len() > t.h
    {
        return Err(err(
            "MATLAB:table:RowIndexOutOfRange",
            "The logical row subscript is longer than the table height.",
        ));
    }
    if !grow && let Some(m) = idx.iter().find(|&&i| i >= t.h) {
        return Err(err(
            "MATLAB:table:RowIndexOutOfRange",
            format!(
                "Row index exceeds table dimensions. Index must not exceed {} (got {}).",
                t.h,
                m + 1
            ),
        ));
    }
    Ok(idx)
}

/// Variable positions (numbers, logical mask, `:`, names). With `grow`,
/// unknown names are appended to `new`.
fn var_positions(t: &Tab, v: &Value, new: Option<&mut Vec<String>>) -> R<Vec<usize>> {
    if is_colon(v) {
        return Ok((0..t.width()).collect());
    }
    if v.is_text() || matches!(v, Value::Cell(_)) {
        let names = text_list(v).ok_or_else(|| {
            err(
                "MATLAB:table:InvalidVarSubscript",
                "Variable subscripts must be numbers, logicals or variable names.",
            )
        })?;
        let mut out = Vec::new();
        let mut new = new;
        for n in names {
            match t.var_index(&n) {
                Some(k) => out.push(k),
                None => match new.as_deref_mut() {
                    Some(list) => {
                        check_name(&n)?;
                        let k = t.width() + list.len();
                        list.push(n);
                        out.push(k);
                    }
                    None => return Err(no_var(&n)),
                },
            }
        }
        return Ok(out);
    }
    if let Some(o) = obj_of(v)
        && o.class == "vartype"
    {
        return Ok(Vec::new());
    }
    let s = indy_value::index::to_sub(v)?;
    let Sub::List { idx, .. } = s else {
        return Ok((0..t.width()).collect());
    };
    if new.is_none()
        && let Some(m) = idx.iter().find(|&&i| i >= t.width())
    {
        return Err(err(
            "MATLAB:table:VarIndexOutOfRange",
            format!(
                "Variable index exceeds table dimensions. Index must not exceed {} (got {}).",
                t.width(),
                m + 1
            ),
        ));
    }
    Ok(idx)
}

fn two_subs<'a>(args: &'a [Value], what: &str) -> R<(&'a Value, &'a Value)> {
    match args {
        [r, c] => Ok((r, c)),
        _ => Err(err(
            "MATLAB:table:LinearSubscript",
            format!(
                "{} a table using linear indexing (one subscript) or multidimensional indexing (three or more subscripts) is not supported. Use a row subscript and a variable subscript.",
                what
            ),
        )),
    }
}

// ---------------------------------------------------------------------------
// Indexing
// ---------------------------------------------------------------------------

pub fn step(o: &Object, eops: &[EOp]) -> R<(Vec<Value>, usize)> {
    let t = Tab::from_obj(o);
    match &eops[0] {
        EOp::Field(name) => {
            if name == "Properties" {
                return Ok((vec![t.properties()], 1));
            }
            if let Some(k) = t.var_index(name) {
                return Ok((vec![t.vars[k].clone()], 1));
            }
            if *name == t.dimnames[0] {
                return Ok((vec![rownames_value(&t)], 1));
            }
            if *name == t.dimnames[1] {
                return Ok((vec![table2array_of(&t)?], 1));
            }
            Err(no_var(name))
        }
        EOp::Paren(args) => {
            let (r, c) = two_subs(args, "Subscripting into")?;
            let rows = row_positions(&t, r, false)?;
            let vars = var_positions(&t, c, None)?;
            let sub = t.take_vars(&vars).take_rows(&rows)?;
            Ok((vec![sub.value()], 1))
        }
        EOp::Brace(args) => {
            let (r, c) = two_subs(args, "Subscripting into")?;
            let rows = row_positions(&t, r, false)?;
            let vars = var_positions(&t, c, None)?;
            let parts = vars
                .iter()
                .map(|&k| rows_of(&t.vars[k], &rows))
                .collect::<R<Vec<_>>>()?;
            if parts.is_empty() {
                return Ok((vec![Value::Num(Num::real(vec![rows.len(), 0], vec![]))], 1));
            }
            Ok((vec![hcat_vars(parts)?], 1))
        }
    }
}

fn rownames_value(t: &Tab) -> Value {
    if t.rows.is_empty() {
        Value::Cell(Array::new(vec![0, 1], vec![]))
    } else {
        cellstr_col(&t.rows)
    }
}

fn hcat_vars(parts: Vec<Value>) -> R<Value> {
    if parts.len() == 1 {
        return Ok(parts.into_iter().next().unwrap());
    }
    // Cell variables combine with others as cells.
    if parts.iter().any(|p| matches!(p, Value::Cell(_))) {
        let wrapped: Vec<Value> = parts
            .into_iter()
            .map(|p| match p {
                Value::Cell(_) => p,
                other => num_to_cells(&other),
            })
            .collect();
        return indy_value::concat::cat(wrapped, 1);
    }
    dt_cat(parts, 1)
}

fn num_to_cells(v: &Value) -> Value {
    let d = v.dims();
    let h = d[0];
    let w: usize = d[1..].iter().product();
    let mut out = Vec::with_capacity(h * w);
    for k in 0..h * w {
        out.push(index_value(v, &[Sub::scalar(k)]).unwrap_or_default());
    }
    Value::Cell(Array::new(vec![h, w], out))
}

pub fn table2array_of(t: &Tab) -> R<Value> {
    if t.vars.is_empty() {
        return Ok(Value::Num(Num::real(vec![t.h, 0], vec![])));
    }
    hcat_vars(t.vars.clone())
}

// ---------------------------------------------------------------------------
// Assignment
// ---------------------------------------------------------------------------

fn is_deletion(v: &Value) -> bool {
    matches!(v, Value::Num(n) if n.re.is_empty() && n.dims == [0, 0])
}

pub fn assign(o: Object, eops: &[EOp], rhs: Value) -> R<Object> {
    let mut t = Tab::from_obj(&o);
    match &eops[0] {
        EOp::Field(name) if name == "Properties" => {
            let EOp::Field(p) = eops.get(1).ok_or_else(|| {
                err(
                    "MATLAB:table:InvalidPropertyAssignment",
                    "Assign to a specific table property (T.Properties.Name = value).",
                )
            })?
            else {
                return Err(err(
                    "MATLAB:table:InvalidPropertyAssignment",
                    "Assign to a specific table property (T.Properties.Name = value).",
                ));
            };
            let props = t.properties();
            let Value::Struct(s) = &props else {
                unreachable!()
            };
            let cur = s.get(0, p).cloned().ok_or_else(|| {
                err(
                    "MATLAB:table:UnknownProperty",
                    format!("Unrecognized table property '{}'.", p),
                )
            })?;
            let mut slot = Some(cur);
            crate::assign::assign_path(&mut slot, &eops[2..], rhs)?;
            set_property(&mut t, p, slot.unwrap_or_default())?;
        }
        EOp::Field(name) => {
            if *name == t.dimnames[0] {
                let mut slot = Some(rownames_value(&t));
                crate::assign::assign_path(&mut slot, &eops[1..], rhs)?;
                set_property(&mut t, "RowNames", slot.unwrap_or_default())?;
                return Ok(t.to_obj());
            }
            if eops.len() == 1 && is_deletion(&rhs) && t.var_index(name).is_some() {
                let k = t.var_index(name).unwrap();
                let keep: Vec<usize> = (0..t.width()).filter(|&j| j != k).collect();
                return Ok(t.take_vars(&keep).to_obj());
            }
            let newval = if eops.len() == 1 {
                rhs
            } else {
                let cur = t.var_index(name).map(|k| t.vars[k].clone());
                let mut slot = cur;
                crate::assign::assign_path(&mut slot, &eops[1..], rhs)?;
                slot.unwrap_or_default()
            };
            if *name == t.dimnames[1] && t.var_index(name).is_none() {
                return Err(err(
                    "MATLAB:table:UnsupportedAssignment",
                    format!("Assigning to T.{} is not supported yet.", name),
                ));
            }
            t.set_var(name, newval)?;
        }
        EOp::Paren(args) => {
            if eops.len() > 1 {
                // T(r, c).x = v : assign into the subtable, then write back.
                let (rv, cv) = two_subs(args, "Assigning into")?;
                let rows = row_positions(&t, rv, false)?;
                let vars = var_positions(&t, cv, None)?;
                let sub = t.take_vars(&vars).take_rows(&rows)?;
                let newsub = assign(sub.to_obj(), &eops[1..], rhs)?;
                let ns = Tab::from_obj(&newsub);
                assign_rows_from(&mut t, &rows, &vars, &ns)?;
                return Ok(t.to_obj());
            }
            let (rv, cv) = two_subs(args, "Assigning into")?;
            if is_deletion(&rhs) {
                if is_colon(rv) {
                    let vars = var_positions(&t, cv, None)?;
                    let keep: Vec<usize> = (0..t.width()).filter(|k| !vars.contains(k)).collect();
                    return Ok(t.take_vars(&keep).to_obj());
                }
                if is_colon(cv) || var_positions(&t, cv, None)?.len() == t.width() {
                    let rows = row_positions(&t, rv, false)?;
                    let keep: Vec<usize> = (0..t.h).filter(|r| !rows.contains(r)).collect();
                    return Ok(t.take_rows(&keep)?.to_obj());
                }
                return Err(err(
                    "MATLAB:table:InvalidEmptyAssignment",
                    "Deletion requires a colon (:) for either the rows or the variables.",
                ));
            }
            let mut new_names = Vec::new();
            let rows = if is_colon(rv) && t.width() == 0 {
                (0..height_of_rhs(&rhs)).collect()
            } else {
                row_positions(&t, rv, true)?
            };
            let vars = var_positions(&t, cv, Some(&mut new_names))?;
            let r = match tab_of(&rhs) {
                Some(r) => r,
                None => rhs_table(&rhs, rows.len(), vars.len())?,
            };
            add_new_vars(&mut t, &new_names, &r)?;
            assign_rows_from(&mut t, &rows, &vars, &r)?;
        }
        EOp::Brace(args) => {
            let (rv, cv) = two_subs(args, "Assigning into")?;
            let mut new_names = Vec::new();
            let rows = row_positions(&t, rv, true)?;
            let vars = var_positions(&t, cv, Some(&mut new_names))?;
            if eops.len() > 1 {
                if rows.len() != 1 || vars.len() != 1 {
                    return Err(err(
                        "MATLAB:index:multipleTargets",
                        "Nested assignment through T{r,c} needs one row and one variable.",
                    ));
                }
                let k = vars[0];
                let cur = rows_of(&t.vars[k], &rows)?;
                let mut slot = Some(cur);
                crate::assign::assign_path(&mut slot, &eops[1..], rhs)?;
                let v = std::mem::take(&mut t.vars[k]);
                t.vars[k] = set_rows(v, &rows, slot.unwrap_or_default())?;
                return Ok(t.to_obj());
            }
            if !new_names.is_empty() {
                for n in &new_names {
                    let fill = Value::Num(Num::real(vec![t.h, 1], vec![0.0; t.h]));
                    t.set_var(n, fill)?;
                }
            }
            grow_height(&mut t, &rows)?;
            if vars.len() == 1 {
                let k = vars[0];
                let v = std::mem::take(&mut t.vars[k]);
                t.vars[k] = set_rows(v, &rows, rhs)?;
            } else {
                // Split the right side by columns over the variables.
                let mut col = 0usize;
                for &k in &vars {
                    let w: usize = t.vars[k].dims()[1..].iter().product();
                    let cols: Vec<f64> = (col..col + w).map(|c| (c + 1) as f64).collect();
                    let part = index_value(
                        &rhs,
                        &[Sub::Colon, indy_value::index::to_sub(&Value::row(cols))?],
                    )?;
                    col += w;
                    let v = std::mem::take(&mut t.vars[k]);
                    t.vars[k] = set_rows(v, &rows, part)?;
                }
            }
        }
    }
    Ok(t.to_obj())
}

fn height_of_rhs(v: &Value) -> usize {
    match tab_of(v) {
        Some(t) => t.h,
        None => height_of(v),
    }
}

fn grow_height(t: &mut Tab, rows: &[usize]) -> R<()> {
    let max = rows.iter().map(|r| r + 1).max().unwrap_or(0);
    if max > t.h {
        for v in t.vars.iter_mut() {
            *v = pad_rows(std::mem::take(v), max)?;
        }
        if !t.rows.is_empty() {
            for r in t.rows.len()..max {
                t.rows.push(format!("Row{}", r + 1));
            }
        }
        t.h = max;
    }
    Ok(())
}

fn add_new_vars(t: &mut Tab, names: &[String], r: &Tab) -> R<()> {
    for (j, n) in names.iter().enumerate() {
        let src = r.vars.get(r.width() - names.len() + j).cloned();
        let fill = match src {
            Some(v) => {
                let idx: Vec<usize> = vec![0; t.h];
                if t.h == 0 {
                    rows_of(&v, &[])?
                } else {
                    let mut f = rows_of(&v, &idx)?;
                    f = default_like(f);
                    f
                }
            }
            None => Value::Num(Num::real(vec![t.h, 1], vec![0.0; t.h])),
        };
        if t.vars.is_empty() {
            t.h = height_of(&fill);
        }
        t.set_var(n, fill)?;
    }
    Ok(())
}

/// Same shape and class as `v`, filled with default values.
fn default_like(v: Value) -> Value {
    match v {
        Value::Num(mut n) => {
            n.re.iter_mut().for_each(|x| *x = 0.0);
            n.im = None;
            Value::Num(n)
        }
        Value::Bool(a) => Value::Bool(Array::new(a.dims.clone(), vec![false; a.data.len()])),
        Value::Str(a) => Value::Str(Array::new(a.dims.clone(), vec![None; a.data.len()])),
        Value::Cell(c) => Value::Cell(Array::new(
            c.dims.clone(),
            vec![Value::str(""); c.data.len()],
        )),
        other => other,
    }
}

/// Right side of `T(rows, vars) = rhs` as a table: a cell array (one cell
/// per variable) or a scalar for every element.
fn rhs_table(rhs: &Value, nrows: usize, nvars: usize) -> R<Tab> {
    let mut r = Tab::new(nrows);
    match rhs {
        Value::Cell(c) => {
            let d = c.dims.clone();
            if (d[0] != nrows || d.get(1).copied().unwrap_or(1) != nvars)
                && !(c.data.len() == 1 && nrows * nvars >= 1)
            {
                return Err(err(
                    "MATLAB:table:RHSSizeMismatch",
                    format!(
                        "The right side is a {}×{} cell array; it must be {}×{} to assign into the table.",
                        d[0],
                        d.get(1).copied().unwrap_or(1),
                        nrows,
                        nvars
                    ),
                ));
            }
            for k in 0..nvars {
                let col: Vec<Value> = (0..nrows)
                    .map(|i| {
                        if c.data.len() == 1 {
                            c.data[0].clone()
                        } else {
                            c.data[i + k * nrows].clone()
                        }
                    })
                    .collect();
                r.names.push(format!("Var{}", k + 1));
                r.vars.push(column_from_cells(col));
            }
        }
        _ => {
            if rhs.numel() != 1 && rhs.dims() != [nrows, nvars] {
                return Err(err(
                    "MATLAB:table:RHSSizeMismatch",
                    "The right side must be a table, a cell array, or a scalar.",
                ));
            }
            for k in 0..nvars {
                let col = if rhs.numel() == 1 {
                    rows_of(rhs, &vec![0; nrows])?
                } else {
                    index_value(rhs, &[Sub::Colon, Sub::scalar(k)])?
                };
                r.names.push(format!("Var{}", k + 1));
                r.vars.push(col);
            }
        }
    }
    Ok(r)
}

/// Combine cell elements of one column into a variable value: numeric /
/// logical scalars into a column, text into a cellstr, other into cells.
pub fn column_from_cells(col: Vec<Value>) -> Value {
    let n = col.len();
    if n > 0
        && col
            .iter()
            .all(|v| matches!(v, Value::Num(x) if x.re.len() == 1))
    {
        let rows: Vec<Value> = col;
        if let Ok(v) = indy_value::concat::cat(rows.clone(), 0) {
            return v;
        }
        return Value::Cell(Array::new(vec![n, 1], rows));
    }
    if n > 0
        && col
            .iter()
            .all(|v| matches!(v, Value::Bool(x) if x.data.len() == 1))
    {
        let b: Vec<bool> = col
            .iter()
            .map(|v| v.scalar_f64().unwrap_or(0.0) != 0.0)
            .collect();
        return bools(vec![n, 1], b);
    }
    if n > 0
        && col
            .iter()
            .all(|v| matches!(v, Value::Str(x) if x.data.len() == 1))
    {
        let s: Vec<Option<String>> = col
            .iter()
            .map(|v| match v {
                Value::Str(x) => x.data[0].clone(),
                _ => None,
            })
            .collect();
        return Value::Str(Array::new(vec![n, 1], s));
    }
    if n > 0
        && col.iter().all(|v| is_native(v) && v.numel() == 1)
        && col
            .windows(2)
            .all(|w| w[0].class_name() == w[1].class_name())
        && let Ok(v) = dt_cat(col.clone(), 0)
    {
        return v;
    }
    Value::Cell(Array::new(vec![n, 1], col))
}

/// Write the rows `rows` of variables `vars` of `t` from table `r`
/// (by position); grows the height.
fn assign_rows_from(t: &mut Tab, rows: &[usize], vars: &[usize], r: &Tab) -> R<()> {
    if r.width() != vars.len() {
        return Err(err(
            "MATLAB:table:RHSWidthMismatch",
            format!(
                "The right side has {} variables; the assignment needs {}.",
                r.width(),
                vars.len()
            ),
        ));
    }
    if r.h != rows.len() && r.h != 1 {
        return Err(err(
            "MATLAB:table:RHSHeightMismatch",
            format!(
                "The right side has {} rows; the assignment needs {}.",
                r.h,
                rows.len()
            ),
        ));
    }
    grow_height(t, rows)?;
    for (j, &k) in vars.iter().enumerate() {
        let mut src = r.vars[j].clone();
        if r.h == 1 && rows.len() != 1 {
            src = rows_of(&src, &vec![0; rows.len()])?;
        }
        let dst = std::mem::take(&mut t.vars[k]);
        let src = match (&dst, &src) {
            // A cellstr variable takes text from a cell right side.
            (Value::Cell(_), Value::Cell(_)) => src,
            (Value::Cell(_), _) => Value::Cell(Array::new(src.dims(), vec![src.clone()])),
            (Value::Num(_) | Value::Bool(_), Value::Cell(c)) if c.data.len() == 1 => {
                c.data[0].clone()
            }
            _ => src,
        };
        t.vars[k] = set_rows(dst, rows, src)?;
    }
    if !r.rows.is_empty() && t.rows.is_empty() && t.h == rows.len() {
        t.rows = r.rows.clone();
    }
    Ok(())
}

fn set_property(t: &mut Tab, p: &str, v: Value) -> R<()> {
    let names_of = |v: &Value, what: &str| -> R<Vec<String>> {
        text_list(v).ok_or_else(|| {
            err(
                "MATLAB:table:InvalidPropertyValue",
                format!(
                    "{} must be a cell array of character vectors or a string array.",
                    what
                ),
            )
        })
    };
    match p {
        "VariableNames" => {
            let n = names_of(&v, "VariableNames")?;
            if n.len() != t.width() {
                return Err(err(
                    "MATLAB:table:IncorrectNumberOfVarNames",
                    format!(
                        "The VariableNames property must contain one name for each variable in the table ({} names for {} variables).",
                        n.len(),
                        t.width()
                    ),
                ));
            }
            check_unique(&n)?;
            t.names = n;
        }
        "RowNames" => {
            let n = if v.is_empty() {
                Vec::new()
            } else {
                names_of(&v, "RowNames")?
            };
            if !n.is_empty() && n.len() != t.h {
                return Err(err(
                    "MATLAB:table:IncorrectNumberOfRowNames",
                    "The RowNames property must contain one name for each row in the table.",
                ));
            }
            t.rows = n;
        }
        "Description" => t.desc = v,
        "UserData" => t.ud = v,
        "DimensionNames" => {
            let n = names_of(&v, "DimensionNames")?;
            if n.len() != 2 {
                return Err(err(
                    "MATLAB:table:IncorrectNumberOfDimNames",
                    "The DimensionNames property must contain two names.",
                ));
            }
            t.dimnames = [n[0].clone(), n[1].clone()];
        }
        "VariableUnits" | "VariableDescriptions" => {
            let n = if v.is_empty() {
                Vec::new()
            } else {
                names_of(&v, p)?
            };
            if !n.is_empty() && n.len() != t.width() {
                return Err(err(
                    "MATLAB:table:IncorrectNumberOfVarProps",
                    format!(
                        "The {} property must contain one entry for each variable.",
                        p
                    ),
                ));
            }
            if p == "VariableUnits" {
                t.units = n;
            } else {
                t.vdesc = n;
            }
        }
        "VariableContinuity" => t.cont = v,
        "CustomProperties" => t.custom = v,
        _ => {
            return Err(err(
                "MATLAB:table:UnknownProperty",
                format!("Unrecognized table property '{}'.", p),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Concatenation, comparison, operators
// ---------------------------------------------------------------------------

pub fn cat(vals: Vec<Value>, d: usize) -> R<Value> {
    let mut tabs: Vec<Tab> = Vec::new();
    let template = vals.iter().find_map(tab_of).unwrap();
    for v in vals {
        if matches!(&v, Value::Num(n) if n.re.is_empty() && n.dims == [0, 0]) {
            continue;
        }
        match tab_of(&v) {
            Some(t) => tabs.push(t),
            None if matches!(v, Value::Cell(_)) && d == 0 => {
                let mut r = rhs_table(&v, v.dims()[0], template.width())?;
                r.names = template.names.clone();
                tabs.push(r);
            }
            None => {
                return Err(err(
                    "MATLAB:table:InvalidConcatenation",
                    format!(
                        "Tables can only be concatenated with tables (got {}).",
                        v.class_name()
                    ),
                ));
            }
        }
    }
    let mut it = tabs.into_iter();
    let mut out = it.next().unwrap_or(template);
    for t in it {
        if d == 1 {
            if t.h != out.h {
                return Err(err(
                    "MATLAB:table:horzcat:SizeMismatch",
                    "All tables being horizontally concatenated must have the same number of rows.",
                ));
            }
            let w = out.width();
            for (n, v) in t.names.iter().zip(t.vars) {
                if out.var_index(n).is_some() {
                    return Err(err(
                        "MATLAB:table:DuplicateVarNames",
                        format!("Duplicate table variable name: '{}'.", n),
                    ));
                }
                out.names.push(n.clone());
                out.vars.push(v);
            }
            if !out.units.is_empty() || !t.units.is_empty() {
                out.units.resize(w, String::new());
                let mut u = t.units.clone();
                u.resize(t.names.len(), String::new());
                out.units.extend(u);
            }
            if out.rows.is_empty() {
                out.rows = t.rows;
            }
        } else {
            if t.width() != out.width() {
                return Err(err(
                    "MATLAB:table:vertcat:SizeMismatch",
                    "All tables being vertically concatenated must have the same number of variables.",
                ));
            }
            let mut order = Vec::with_capacity(t.width());
            for n in &out.names {
                order.push(match t.var_index(n) {
                    Some(k) => k,
                    None if t.names.iter().all(|m| m.starts_with("Var")) => order.len(),
                    None => {
                        return Err(err(
                            "MATLAB:table:vertcat:UnequalVarNames",
                            "All tables being vertically concatenated must have the same variable names.",
                        ));
                    }
                });
            }
            for (j, k) in order.into_iter().enumerate() {
                let a = std::mem::take(&mut out.vars[j]);
                let b = t.vars[k].clone();
                out.vars[j] = vcat_var(a, b)?;
            }
            if !out.rows.is_empty() || !t.rows.is_empty() {
                let mut a = out.rows.clone();
                if a.is_empty() {
                    a = (0..out.h).map(|r| format!("Row{}", r + 1)).collect();
                }
                let mut b = t.rows.clone();
                if b.is_empty() {
                    b = (0..t.h).map(|r| format!("Row{}", out.h + r + 1)).collect();
                }
                a.extend(b);
                out.rows = a;
            }
            out.h += t.h;
        }
    }
    Ok(out.value())
}

fn vcat_var(a: Value, b: Value) -> R<Value> {
    match (&a, &b) {
        (Value::Cell(_), Value::Cell(_)) => Ok(indy_value::concat::cat(vec![a, b], 0)?),
        (Value::Cell(_), _) => {
            let w = Value::Cell(Array::new(vec![1, 1], vec![b]));
            Ok(indy_value::concat::cat(vec![a, w], 0)?)
        }
        _ => dt_cat(vec![a, b], 0),
    }
}

pub fn equal(x: &Object, y: &Object, nan_eq: bool) -> bool {
    let (a, b) = (Tab::from_obj(x), Tab::from_obj(y));
    a.h == b.h
        && a.names == b.names
        && a.rows == b.rows
        && a.vars
            .iter()
            .zip(&b.vars)
            .all(|(p, q)| natives_equal(p, q, nan_eq))
}

fn binary(op: BinOp, a: &Value, b: &Value) -> R<Value> {
    // Element-wise arithmetic and comparisons with a table and a scalar or
    // another table of the same size, variable by variable.
    let (ta, tb) = (tab_of(a), tab_of(b));
    let base = ta.clone().or_else(|| tb.clone()).unwrap();
    let mut out = base.clone();
    for k in 0..base.width() {
        let x = match &ta {
            Some(t) => t.vars[k].clone(),
            None => a.clone(),
        };
        let y = match &tb {
            Some(t) => match t.vars.get(k) {
                Some(v) => v.clone(),
                None => return Err(super::undefined_op(op, a, b)),
            },
            None => b.clone(),
        };
        if !(x.is_numeric() || matches!(x, Value::Bool(_)))
            || !(y.is_numeric() || matches!(y, Value::Bool(_)))
        {
            return Err(err(
                "MATLAB:table:math:VarTypes",
                format!(
                    "Operator '{}' on tables needs numeric or logical variables; variable '{}' is {}.",
                    op.text(),
                    base.names[k],
                    if x.is_numeric() || matches!(x, Value::Bool(_)) {
                        y.class_name()
                    } else {
                        x.class_name()
                    }
                ),
            ));
        }
        out.vars[k] = indy_value::ops::binary(op, &x, &y)?;
    }
    Ok(out.value())
}

pub fn not(o: &Object) -> R<Value> {
    let mut t = Tab::from_obj(o);
    for v in t.vars.iter_mut() {
        *v = indy_value::ops::unary(indy_syntax::ast::UnOp::Not, v)?;
    }
    Ok(t.value())
}

// ---------------------------------------------------------------------------
// Display
// ---------------------------------------------------------------------------

fn display(name: &str, o: &Object) -> String {
    let t = Tab::from_obj(o);
    if t.h == 0 || t.width() == 0 {
        let mut s = format!("{} =\n\n  {}×{} empty table\n\n", name, t.h, t.width());
        if t.width() > 0 {
            s.push_str(&disp_text(&t));
        }
        return s;
    }
    format!(
        "{} =\n\n  {}×{} table\n\n{}",
        name,
        t.h,
        t.width(),
        disp_text(&t)
    )
}

/// Text of one element of a variable for display.
fn cell_text(v: &Value) -> String {
    if let Value::Sparse(sp) = v
        && let Some(d) = sp.dense().filter(|d| d.numel() == 1)
    {
        return cell_text(&d);
    }
    if let Some(o) = obj_of(v) {
        if o.class == "table" {
            return format!("{}×{} table", o.dims[0], o.dims[1]);
        }
        let t = (super::ops_of(&o.class).unwrap().disp)(o);
        return t.trim().to_string();
    }
    match v {
        Value::Char(_) if v.is_string_row() || v.is_empty() => {
            format!("'{}'", v.to_str().unwrap_or_default())
        }
        Value::Num(n) if n.re.len() == 1 => num_text(n, 0),
        Value::Bool(a) if a.data.len() == 1 => if a.data[0] { "true" } else { "false" }.into(),
        Value::Str(a) if a.data.len() == 1 => match &a.data[0] {
            Some(s) => format!("\"{}\"", s),
            None => "<missing>".into(),
        },
        _ => format!(
            "{} {}",
            v.dims()
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("×"),
            v.class_name()
        ),
    }
}

fn num_text(n: &Num, i: usize) -> String {
    let x = n.re[i];
    let re = if n.class.is_int() || (x.fract() == 0.0 && x.abs() < 1e10) {
        format::short_g(x)
    } else if x.is_nan() {
        "NaN".into()
    } else if x.is_infinite() {
        if x > 0.0 { "Inf".into() } else { "-Inf".into() }
    } else {
        format::fmt_g(x, 5, false, false)
    };
    match &n.im {
        Some(im) if im[i] != 0.0 => {
            let y = im[i];
            let s = format::fmt_g(y.abs(), 5, false, false);
            format!("{}{}{}i", re, if y < 0.0 { "-" } else { "+" }, s)
        }
        _ => re,
    }
}

/// Display columns of one variable: (texts per row, right-aligned?).
fn var_columns(v: &Value, h: usize) -> Vec<(Vec<String>, bool)> {
    if let Value::Sparse(sp) = v
        && let Some(d) = sp.dense()
    {
        return var_columns(&d, h);
    }
    let d = v.dims();
    let w: usize = d[1..].iter().product();
    let mut cols = Vec::new();
    match v {
        Value::Char(_) => {
            let rows = v.char_rows().unwrap_or_default();
            cols.push((rows.into_iter().map(|r| r.to_string()).collect(), false));
            return cols;
        }
        Value::Cell(c) => {
            for j in 0..w {
                let col: Vec<String> = (0..h)
                    .map(|i| {
                        let x = &c.data[i + j * h];
                        if x.is_char() && (x.is_string_row() || x.is_empty()) {
                            format!("{{'{}'}}", x.to_str().unwrap_or_default())
                        } else if matches!(x, Value::Num(n) if n.re.len() == 1) {
                            format!("{{[{}]}}", cell_text(x))
                        } else {
                            format!("{{{}}}", cell_text(x).trim_matches('\''))
                        }
                    })
                    .collect();
                cols.push((col, false));
            }
            return cols;
        }
        _ => {}
    }
    if let Some(o) = obj_of(v) {
        if o.class == "table" {
            let t = Tab::from_obj(o);
            for sub in &t.vars {
                cols.extend(var_columns(sub, h));
            }
            return cols;
        }
        let texts = super::elem_texts(o);
        for j in 0..w {
            cols.push(((0..h).map(|i| texts[i + j * h].clone()).collect(), false));
        }
        return cols;
    }
    for j in 0..w {
        let col: Vec<String> = (0..h)
            .map(|i| {
                let k = i + j * h;
                match v {
                    Value::Num(n) => num_text(n, k),
                    _ => index_value(v, &[Sub::scalar(k)])
                        .map(|e| cell_text(&e))
                        .unwrap_or_default(),
                }
            })
            .collect();
        let right = matches!(v, Value::Num(_));
        cols.push((col, right));
    }
    cols
}

fn center(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n >= w {
        return s.to_string();
    }
    let left = (w - n) / 2;
    format!("{}{}{}", " ".repeat(left), s, " ".repeat(w - n - left))
}

fn pad_right(s: &str, w: usize) -> String {
    let n = s.chars().count();
    format!("{}{}", s, " ".repeat(w.saturating_sub(n)))
}

fn pad_left(s: &str, w: usize) -> String {
    let n = s.chars().count();
    format!("{}{}", " ".repeat(w.saturating_sub(n)), s)
}

pub fn disp_text(t: &Tab) -> String {
    let h = t.h;
    // Blocks: (header, width, row texts).
    let mut blocks: Vec<(String, usize, Vec<String>)> = Vec::new();
    for (name, v) in t.names.iter().zip(&t.vars) {
        let cols = var_columns(v, h);
        let mut pieces: Vec<Vec<String>> = Vec::new();
        for (col, right) in &cols {
            let w = col.iter().map(|s| s.chars().count()).max().unwrap_or(0);
            pieces.push(
                col.iter()
                    .map(|s| {
                        if *right {
                            pad_left(s, w)
                        } else {
                            pad_right(s, w)
                        }
                    })
                    .collect(),
            );
        }
        let rows: Vec<String> = (0..h)
            .map(|i| {
                pieces
                    .iter()
                    .map(|p| p[i].clone())
                    .collect::<Vec<_>>()
                    .join("    ")
            })
            .collect();
        let dw = rows.iter().map(|s| s.chars().count()).max().unwrap_or(0);
        let w = dw.max(name.chars().count());
        let rows = rows.iter().map(|s| center(&pad_right(s, dw), w)).collect();
        blocks.push((name.clone(), w, rows));
    }
    let rn_w = t.rows.iter().map(|s| s.chars().count()).max().unwrap_or(0);
    let mut out = String::new();
    let lead = |out: &mut String, s: &str| {
        out.push_str("    ");
        if rn_w > 0 {
            out.push_str(&pad_right(s, rn_w));
            out.push_str("    ");
        }
    };
    lead(&mut out, "");
    out.push_str(
        &blocks
            .iter()
            .map(|(n, w, _)| center(n, *w))
            .collect::<Vec<_>>()
            .join("    "),
    );
    out = out.trim_end().to_string();
    out.push('\n');
    lead(&mut out, "");
    out.push_str(
        &blocks
            .iter()
            .map(|(_, w, _)| "_".repeat(*w))
            .collect::<Vec<_>>()
            .join("    "),
    );
    out = out.trim_end().to_string();
    out.push_str("\n\n");
    for i in 0..h {
        let mut line = String::new();
        lead(&mut line, t.rows.get(i).map(|s| s.as_str()).unwrap_or(""));
        line.push_str(
            &blocks
                .iter()
                .map(|(_, w, r)| pad_right(&r[i], *w))
                .collect::<Vec<_>>()
                .join("    "),
        );
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out.push('\n');
    out
}

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

const TABLE_OPTS: &[&str] = &[
    "VariableNames",
    "RowNames",
    "Size",
    "VariableTypes",
    "DimensionNames",
];

/// Split `args` into data and trailing name-value options.
fn split_opts(a: &[Value], known: &[&str]) -> (usize, Vec<(String, Value)>) {
    let mut start = a.len();
    let mut i = 0;
    while i < a.len() {
        if let Some(s) = a[i].to_str()
            && a[i].is_text()
            && known.iter().any(|k| k.eq_ignore_ascii_case(&s))
            && (a.len() - i).is_multiple_of(2)
        {
            start = i;
            break;
        }
        i += 1;
    }
    let opts = a[start..]
        .chunks(2)
        .map(|p| (p[0].to_str().unwrap_or_default(), p[1].clone()))
        .collect();
    (start, opts)
}

fn opt<'a>(opts: &'a [(String, Value)], name: &str) -> Option<&'a Value> {
    opts.iter()
        .rev()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v)
}

fn names_opt(v: &Value, what: &str) -> R<Vec<String>> {
    text_list(v).ok_or_else(|| {
        err(
            "MATLAB:table:InvalidVarNames",
            format!(
                "{} must be a cell array of character vectors or a string array.",
                what
            ),
        )
    })
}

/// Apply 'VariableNames', 'RowNames', 'DimensionNames' options.
fn apply_opts(t: &mut Tab, opts: &[(String, Value)]) -> R<()> {
    if let Some(v) = opt(opts, "VariableNames") {
        let n = names_opt(v, "VariableNames")?;
        if n.len() != t.width() {
            return Err(err(
                "MATLAB:table:IncorrectNumberOfVarNames",
                format!(
                    "The VariableNames property must contain one name for each variable in the table ({} names for {} variables).",
                    n.len(),
                    t.width()
                ),
            ));
        }
        t.names = n;
    }
    check_unique(&t.names)?;
    if let Some(v) = opt(opts, "RowNames") {
        set_property(t, "RowNames", v.clone())?;
    }
    if let Some(v) = opt(opts, "DimensionNames") {
        set_property(t, "DimensionNames", v.clone())?;
    }
    Ok(())
}

fn table_fn(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let (start, opts) = split_opts(&a, TABLE_OPTS);
    let arg_names = it
        .ext::<ArgNames>()
        .map(|n| n.0.clone())
        .unwrap_or_default();
    if let Some(sz) = opt(&opts, "Size") {
        let sz = sz.to_f64_vec().unwrap_or_default();
        let types = opt(&opts, "VariableTypes")
            .and_then(text_list)
            .ok_or_else(|| {
                err(
                    "MATLAB:table:MissingVariableTypes",
                    "table: 'Size' needs 'VariableTypes'.",
                )
            })?;
        let (h, w) = (
            sz.first().copied().unwrap_or(0.0) as usize,
            sz.get(1).copied().unwrap_or(0.0) as usize,
        );
        if types.len() != w {
            return Err(err(
                "MATLAB:table:VariableTypesAndSizeMismatch",
                "table: the number of VariableTypes must match the number of variables in 'Size'.",
            ));
        }
        let mut t = Tab::new(h);
        for (k, ty) in types.iter().enumerate() {
            t.names.push(format!("Var{}", k + 1));
            t.vars.push(typed_column(ty, h)?);
        }
        apply_opts(&mut t, &opts)?;
        return Ok(vec![t.value()]);
    }
    let data = &a[..start];
    let h = data.first().map(height_of).unwrap_or(0);
    let h = match opt(&opts, "RowNames").and_then(text_list) {
        Some(r) if data.is_empty() => r.len(),
        _ => h,
    };
    let mut t = Tab::new(h);
    for (k, v) in data.iter().enumerate() {
        if height_of(v) != h {
            return Err(err(
                "MATLAB:table:UnequalVarLengths",
                "All table variables must have the same number of rows.",
            ));
        }
        let name = arg_names
            .get(k)
            .cloned()
            .flatten()
            .filter(|n| !t.names.contains(n))
            .unwrap_or_else(|| format!("Var{}", k + 1));
        t.names.push(name);
        t.vars.push(v.clone());
    }
    apply_opts(&mut t, &opts)?;
    Ok(vec![t.value()])
}

/// Names of the identifier arguments of the current builtin call (set by
/// the interpreter for `table`; MATLAB uses `inputname`).
#[derive(Default)]
pub struct ArgNames(pub Vec<Option<String>>);

fn typed_column(ty: &str, h: usize) -> R<Value> {
    Ok(match ty {
        "double" | "single" | "int8" | "int16" | "int32" | "int64" | "uint8" | "uint16"
        | "uint32" | "uint64" => {
            let n = Num::real(vec![h, 1], vec![0.0; h]);
            Value::Num(match NumClass::from_name(ty) {
                Some(c) => n.with_class(c),
                None => n,
            })
        }
        "logical" => bools(vec![h, 1], vec![false; h]),
        "string" => Value::Str(Array::new(vec![h, 1], vec![None; h])),
        "cellstr" => Value::Cell(Array::new(vec![h, 1], vec![Value::str(""); h])),
        "cell" => Value::Cell(Array::new(vec![h, 1], vec![Value::empty(); h])),
        "datetime" => super::time::nat_array(vec![h, 1]),
        "duration" => super::time::duration_value(vec![h, 1], vec![0.0; h]),
        "categorical" => super::categorical::undefined_array(vec![h, 1]),
        _ => {
            return Err(err(
                "MATLAB:table:InvalidVariableType",
                format!("table: unsupported variable type '{}'.", ty),
            ));
        }
    })
}

fn istable(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "istable")?;
    Ok(vec![Value::bool(class_is(&a[0], "table"))])
}

fn height(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "height")?;
    Ok(vec![Value::num(height_of(&a[0]) as f64)])
}

fn width(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "width")?;
    let d = a[0].dims();
    Ok(vec![Value::num(d[1..].iter().product::<usize>() as f64)])
}

fn array2table(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "array2table")?;
    let (start, opts) = split_opts(&a[1..], TABLE_OPTS);
    let _ = start;
    let x = &a[0];
    if !(x.is_numeric()
        || matches!(x, Value::Bool(_) | Value::Str(_) | Value::Cell(_))
        || is_native(x))
    {
        return Err(err(
            "MATLAB:array2table:NDArray",
            "array2table: input must be a 2-D array.",
        ));
    }
    let d = x.dims();
    if d.len() > 2 {
        return Err(err(
            "MATLAB:array2table:NDArray",
            "array2table: input must be a 2-D array.",
        ));
    }
    let base = it
        .ext::<ArgNames>()
        .and_then(|n| n.0.first().cloned().flatten());
    let mut t = Tab::new(d[0]);
    for j in 0..d[1] {
        let col = col_of(x, j)?;
        t.names.push(match &base {
            Some(b) => format!("{}{}", b, j + 1),
            None => format!("Var{}", j + 1),
        });
        t.vars.push(col);
    }
    apply_opts(&mut t, &opts)?;
    Ok(vec![t.value()])
}

fn col_of(x: &Value, j: usize) -> R<Value> {
    if let Some(o) = obj_of(x) {
        let sub = super::ew_index(o, &[Sub::Colon, Sub::scalar(j)])?;
        return Ok(Value::Object(Box::new(sub)));
    }
    index_value(x, &[Sub::Colon, Sub::scalar(j)])
}

fn cell2table(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "cell2table")?;
    let Value::Cell(c) = &a[0] else {
        return Err(err(
            "MATLAB:cell2table:NotCell",
            "cell2table: input must be a cell array.",
        ));
    };
    let (_, opts) = split_opts(&a[1..], TABLE_OPTS);
    let (h, w) = (c.dims[0], c.dims.get(1).copied().unwrap_or(1));
    let mut t = Tab::new(h);
    for j in 0..w {
        let col: Vec<Value> = (0..h).map(|i| c.data[i + j * h].clone()).collect();
        t.names.push(format!("Var{}", j + 1));
        t.vars.push(cell_column(col));
    }
    apply_opts(&mut t, &opts)?;
    Ok(vec![t.value()])
}

/// cell2table rule: a column of same-class numeric/logical scalars becomes
/// an array; text stays a cellstr; anything else stays a cell column.
fn cell_column(col: Vec<Value>) -> Value {
    let n = col.len();
    let same_num = n > 0
        && col
            .iter()
            .all(|v| matches!(v, Value::Num(x) if x.re.len() == 1))
        && col
            .windows(2)
            .all(|w| w[0].class_name() == w[1].class_name());
    if same_num
        || (n > 0
            && col
                .iter()
                .all(|v| matches!(v, Value::Bool(x) if x.data.len() == 1)))
        || (n > 0
            && col.iter().all(|v| is_native(v) && v.numel() == 1)
            && col
                .windows(2)
                .all(|w| w[0].class_name() == w[1].class_name()))
        || (n > 0
            && col
                .iter()
                .all(|v| matches!(v, Value::Str(x) if x.data.len() == 1)))
    {
        return column_from_cells(col);
    }
    Value::Cell(Array::new(vec![n, 1], col))
}

fn struct2table(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "struct2table")?;
    let Value::Struct(s) = &a[0] else {
        return Err(err(
            "MATLAB:struct2table:NotStruct",
            "struct2table: input must be a structure.",
        ));
    };
    let mut opts_v = a[1..].to_vec();
    let mut as_array = s.numel() != 1;
    if let Some(p) = opts_v.iter().position(|v| {
        v.to_str()
            .is_some_and(|s| s.eq_ignore_ascii_case("AsArray"))
    }) {
        as_array = opts_v
            .get(p + 1)
            .and_then(|v| v.scalar_f64())
            .unwrap_or(0.0)
            != 0.0;
        opts_v.drain(p..(p + 2).min(opts_v.len()));
    }
    let (_, opts) = split_opts(&opts_v, TABLE_OPTS);
    let t = if as_array {
        let n = s.numel();
        let mut t = Tab::new(n);
        for (fi, f) in s.fields.iter().enumerate() {
            let col: Vec<Value> = s.elems.iter().map(|e| e[fi].clone()).collect();
            t.names.push(f.clone());
            t.vars.push(cell_column_struct(col));
        }
        t
    } else {
        let h = s
            .fields
            .first()
            .map(|_| height_of(&s.elems[0][0]))
            .unwrap_or(0);
        let mut t = Tab::new(h);
        for (fi, f) in s.fields.iter().enumerate() {
            let v = s.elems[0][fi].clone();
            if height_of(&v) != h {
                return Err(err(
                    "MATLAB:struct2table:UnequalFieldLengths",
                    "struct2table: all fields of a scalar structure must have the same number of rows.",
                ));
            }
            t.names.push(f.clone());
            t.vars.push(v);
        }
        t
    };
    let mut t = t;
    apply_opts(&mut t, &opts)?;
    Ok(vec![t.value()])
}

/// struct2table on a struct array: scalar numerics/logicals stack, char
/// rows become a cellstr, others a cell column.
fn cell_column_struct(col: Vec<Value>) -> Value {
    let n = col.len();
    if n > 0
        && col
            .iter()
            .all(|v| v.is_char() && (v.is_string_row() || v.is_empty()))
    {
        return Value::Cell(Array::new(vec![n, 1], col));
    }
    if n > 0
        && col
            .iter()
            .all(|v| matches!(v, Value::Num(_) | Value::Bool(_)) && v.dims()[0] == 1)
        && col
            .windows(2)
            .all(|w| w[0].dims() == w[1].dims() && w[0].class_name() == w[1].class_name())
        && let Ok(v) = indy_value::concat::cat(col.clone(), 0)
    {
        return v;
    }
    cell_column(col)
}

fn table2array(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "table2array")?;
    let t = arg_tab(&a[0], "table2array")?;
    Ok(vec![table2array_of(&t)?])
}

fn table2cell(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "table2cell")?;
    let t = arg_tab(&a[0], "table2cell")?;
    let (h, w) = (t.h, t.width());
    let mut out = vec![Value::empty(); h * w];
    for (j, v) in t.vars.iter().enumerate() {
        for i in 0..h {
            out[i + j * h] = elem_of_row(v, i)?;
        }
    }
    Ok(vec![Value::Cell(Array::new(vec![h, w], out))])
}

/// Row `i` of a variable as one value (cell contents for cell variables).
fn elem_of_row(v: &Value, i: usize) -> R<Value> {
    let r = rows_of(v, &[i])?;
    match r {
        Value::Cell(c) if c.data.len() == 1 => Ok(c.data.into_iter().next().unwrap()),
        other => Ok(other),
    }
}

fn table2struct(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "table2struct")?;
    let t = arg_tab(&a[0], "table2struct")?;
    let scalar = a.len() >= 3
        && a[1]
            .to_str()
            .is_some_and(|s| s.eq_ignore_ascii_case("ToScalar"))
        && a[2].scalar_f64().unwrap_or(0.0) != 0.0;
    if scalar {
        let mut s = StructArray::new_scalar();
        for (n, v) in t.names.iter().zip(&t.vars) {
            s.set(0, n, v.clone());
        }
        return Ok(vec![Value::Struct(s)]);
    }
    let mut s = StructArray::empty_with(t.names.clone(), vec![t.h, 1]);
    for i in 0..t.h {
        for (fi, v) in t.vars.iter().enumerate() {
            s.elems[i][fi] = elem_of_row(v, i)?;
        }
    }
    Ok(vec![Value::Struct(s)])
}

// ---------------------------------------------------------------------------
// Variables
// ---------------------------------------------------------------------------

fn var_list(t: &Tab, v: &Value) -> R<Vec<usize>> {
    var_positions(t, v, None)
}

fn addvars(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "addvars")?;
    let mut t = arg_tab(&a[0], "addvars")?;
    let (start, opts) = split_opts(&a[1..], &["Before", "After", "NewVariableNames"]);
    let data = &a[1..1 + start];
    let arg_names = it
        .ext::<ArgNames>()
        .map(|n| n.0.clone())
        .unwrap_or_default();
    let names: Vec<String> = match opt(&opts, "NewVariableNames") {
        Some(v) => names_opt(v, "NewVariableNames")?,
        None => (0..data.len())
            .map(|k| {
                arg_names
                    .get(k + 1)
                    .cloned()
                    .flatten()
                    .unwrap_or_else(|| format!("Var{}", t.width() + k + 1))
            })
            .collect(),
    };
    if names.len() != data.len() {
        return Err(err(
            "MATLAB:table:addvars:NewVarNamesMismatch",
            "addvars: the number of new variable names must match the number of new variables.",
        ));
    }
    let w0 = t.width();
    let pos = if let Some(b) = opt(&opts, "Before") {
        var_list(&t, b)?.first().copied().unwrap_or(0)
    } else if let Some(af) = opt(&opts, "After") {
        var_list(&t, af)?.first().map(|k| k + 1).unwrap_or(w0)
    } else {
        w0
    };
    for (n, v) in names.iter().zip(data) {
        t.set_var(n, v.clone())?;
    }
    check_unique(&t.names)?;
    let mut order: Vec<usize> = (0..w0).collect();
    let added: Vec<usize> = (w0..t.width()).collect();
    order.splice(pos..pos, added);
    Ok(vec![t.take_vars(&order).value()])
}

fn removevars(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "removevars")?;
    let t = arg_tab(&a[0], "removevars")?;
    let rm = var_list(&t, &a[1])?;
    let keep: Vec<usize> = (0..t.width()).filter(|k| !rm.contains(k)).collect();
    Ok(vec![t.take_vars(&keep).value()])
}

fn renamevars(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 3, "renamevars")?;
    let mut t = arg_tab(&a[0], "renamevars")?;
    let which = var_list(&t, &a[1])?;
    let new = names_opt(&a[2], "renamevars: new names")?;
    if new.len() != which.len() {
        return Err(err(
            "MATLAB:table:renamevars:SizeMismatch",
            "renamevars: the number of new names must match the number of variables.",
        ));
    }
    for (k, n) in which.into_iter().zip(new) {
        t.names[k] = n;
    }
    check_unique(&t.names)?;
    Ok(vec![t.value()])
}

fn movevars(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "movevars")?;
    let t = arg_tab(&a[0], "movevars")?;
    let mv = var_list(&t, &a[1])?;
    let (_, opts) = split_opts(&a[2..], &["Before", "After"]);
    let rest: Vec<usize> = (0..t.width()).filter(|k| !mv.contains(k)).collect();
    let pos = if let Some(b) = opt(&opts, "Before") {
        let k = var_list(&t, b)?[0];
        rest.iter().position(|&x| x == k).unwrap_or(0)
    } else if let Some(af) = opt(&opts, "After") {
        let k = var_list(&t, af)?[0];
        rest.iter()
            .position(|&x| x == k)
            .map(|p| p + 1)
            .unwrap_or(rest.len())
    } else {
        rest.len()
    };
    let mut order = rest;
    order.splice(pos..pos, mv);
    Ok(vec![t.take_vars(&order).value()])
}

// ---------------------------------------------------------------------------
// Sorting, head/tail
// ---------------------------------------------------------------------------

/// Sort key of one row of a variable (one entry per column).
#[derive(Clone, Debug)]
enum Key {
    N(f64),
    S(Option<String>),
}

fn cmp_key(a: &Key, b: &Key) -> Ordering {
    match (a, b) {
        (Key::N(x), Key::N(y)) => match (x.is_nan(), y.is_nan()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            _ => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        },
        (Key::S(x), Key::S(y)) => match (x, y) {
            (None, None) => Ordering::Equal,
            (None, _) => Ordering::Greater,
            (_, None) => Ordering::Less,
            (Some(p), Some(q)) => p.cmp(q),
        },
        (Key::N(_), Key::S(_)) => Ordering::Less,
        (Key::S(_), Key::N(_)) => Ordering::Greater,
    }
}

/// Per-row keys of a variable.
fn row_keys(v: &Value, h: usize) -> R<Vec<Vec<Key>>> {
    let mut out = vec![Vec::new(); h];
    if let Some(o) = obj_of(v) {
        match o.class.as_str() {
            "categorical" => {
                let c = super::data(o, "#c");
                let ordinal = super::categorical::is_ordinal(o);
                let cats = super::categorical::cats(o);
                for (i, row) in out.iter_mut().enumerate() {
                    let code = c.get(i).copied().unwrap_or(0.0);
                    row.push(if ordinal || code == 0.0 {
                        Key::N(if code == 0.0 { f64::NAN } else { code })
                    } else {
                        Key::N(code)
                    });
                    let _ = &cats;
                }
            }
            "table" => {
                let t = Tab::from_obj(o);
                for sub in &t.vars {
                    let k = row_keys(sub, h)?;
                    for (r, kk) in out.iter_mut().zip(k) {
                        r.extend(kk);
                    }
                }
            }
            "calendarDuration" => {
                return Err(err(
                    "MATLAB:table:sortrows:UnsortableVariable",
                    "sortrows: calendarDuration variables cannot be sorted.",
                ));
            }
            _ => {
                let d = super::data(o, "#v");
                let w = d.len() / h.max(1);
                for j in 0..w {
                    for (i, row) in out.iter_mut().enumerate() {
                        row.push(Key::N(d[i + j * h]));
                    }
                }
            }
        }
        return Ok(out);
    }
    match v {
        Value::Cell(c) => {
            for (i, row) in out.iter_mut().enumerate() {
                let x = &c.data[i];
                if !(x.is_char() && (x.is_string_row() || x.is_empty())) {
                    return Err(err(
                        "MATLAB:table:sortrows:CellNotCellstr",
                        "sortrows: a cell variable must be a cell array of character vectors to be sorted.",
                    ));
                }
                row.push(Key::S(x.to_str()));
            }
        }
        Value::Str(a) => {
            for (i, row) in out.iter_mut().enumerate() {
                row.push(Key::S(a.data[i].clone()));
            }
        }
        Value::Char(_) => {
            let rows = v.char_rows().unwrap_or_default();
            for (i, row) in out.iter_mut().enumerate() {
                row.push(Key::S(rows.get(i).cloned()));
            }
        }
        _ => {
            let d = v.to_f64_vec().ok_or_else(|| {
                err(
                    "MATLAB:table:sortrows:UnsortableVariable",
                    format!(
                        "sortrows: cannot sort a variable of class {}.",
                        v.class_name()
                    ),
                )
            })?;
            let w = d.len() / h.max(1);
            for j in 0..w {
                for (i, row) in out.iter_mut().enumerate() {
                    row.push(Key::N(d[i + j * h]));
                }
            }
        }
    }
    Ok(out)
}

/// Stable row order by `vars` (with per-variable descending flags).
fn sort_order(t: &Tab, vars: &[(usize, bool)], use_rownames: bool) -> R<Vec<usize>> {
    let mut keys: Vec<Vec<(Key, bool)>> = vec![Vec::new(); t.h];
    if use_rownames {
        for (i, k) in keys.iter_mut().enumerate() {
            k.push((
                Key::S(t.rows.get(i).cloned()),
                vars.first().is_some_and(|v| v.1),
            ));
        }
    }
    for &(k, desc) in vars {
        let rk = row_keys(&t.vars[k], t.h)?;
        for (row, r) in keys.iter_mut().zip(rk) {
            row.extend(r.into_iter().map(|x| (x, desc)));
        }
    }
    let mut order: Vec<usize> = (0..t.h).collect();
    order.sort_by(|&a, &b| {
        for ((x, desc), (y, _)) in keys[a].iter().zip(&keys[b]) {
            let mut o = cmp_key(x, y);
            if *desc {
                o = o.reverse();
            }
            if o != Ordering::Equal {
                return o;
            }
        }
        Ordering::Equal
    });
    Ok(order)
}

fn sortrows(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "sortrows")?;
    let t = arg_tab(&a[0], "sortrows")?;
    let mut rest: Vec<Value> = a[1..].to_vec();
    // Trailing 'ascend'/'descend' (one per variable, or one for all).
    let mut dirs: Vec<bool> = Vec::new();
    let mut opts_start = rest.len();
    for (i, v) in rest.iter().enumerate() {
        if let Some(s) = v.to_str()
            && v.is_text()
            && matches!(
                s.to_ascii_lowercase().as_str(),
                "missingplacement" | "comparisonmethod"
            )
        {
            opts_start = i;
            break;
        }
    }
    rest.truncate(opts_start);
    if let Some(last) = rest.last()
        && let Some(list) = text_list(last)
        && !list.is_empty()
        && list
            .iter()
            .all(|s| matches!(s.as_str(), "ascend" | "descend"))
        && (rest.len() >= 2 || matches!(list[0].as_str(), "ascend" | "descend"))
    {
        dirs = list.iter().map(|s| s == "descend").collect();
        rest.pop();
    }
    let mut use_rownames = false;
    let mut vars: Vec<(usize, bool)> = match rest.first() {
        None => (0..t.width()).map(|k| (k, false)).collect(),
        Some(v) if v.is_text() || matches!(v, Value::Cell(_)) => {
            let names = text_list(v).unwrap_or_default();
            let mut out = Vec::new();
            for n in names {
                if n == t.dimnames[0] || n == "RowNames" {
                    use_rownames = true;
                    continue;
                }
                out.push((t.var_index(&n).ok_or_else(|| no_var(&n))?, false));
            }
            out
        }
        Some(v) => {
            let ks = v.to_f64_vec().unwrap_or_default();
            ks.iter()
                .map(|&k| {
                    let i = k.abs() as usize;
                    if i == 0 || i > t.width() {
                        return Err(err(
                            "MATLAB:table:sortrows:BadSubscript",
                            "sortrows: variable index out of range.",
                        ));
                    }
                    Ok((i - 1, k < 0.0))
                })
                .collect::<R<Vec<_>>>()?
        }
    };
    if !dirs.is_empty() {
        for (j, v) in vars.iter_mut().enumerate() {
            v.1 = dirs.get(j).copied().unwrap_or(dirs[0]);
        }
    }
    let order = if use_rownames && vars.is_empty() {
        let mut o: Vec<usize> = (0..t.h).collect();
        let desc = dirs.first().copied().unwrap_or(false);
        o.sort_by(|&x, &y| {
            let c = t.rows[x].cmp(&t.rows[y]);
            if desc { c.reverse() } else { c }
        });
        o
    } else {
        sort_order(&t, &vars, use_rownames)?
    };
    let s = t.take_rows(&order)?;
    let mut out = vec![s.value()];
    if nargout > 1 {
        out.push(super::num_col(
            order.iter().map(|&i| (i + 1) as f64).collect(),
        ));
    }
    Ok(out)
}

fn head(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    head_tail(a, true)
}

fn tail(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    head_tail(a, false)
}

fn head_tail(a: Vec<Value>, head: bool) -> R<Vec<Value>> {
    let fname = if head { "head" } else { "tail" };
    need(&a, 1, fname)?;
    let k = match a.get(1) {
        Some(v) => super::super::arg_usize(v, fname)?,
        None => 8,
    };
    let h = height_of(&a[0]);
    let k = k.min(h);
    let rows: Vec<usize> = if head {
        (0..k).collect()
    } else {
        (h - k..h).collect()
    };
    Ok(vec![rows_of(&a[0], &rows)?])
}

// ---------------------------------------------------------------------------
// varfun, rowfun, groupsummary, joins, summary
// ---------------------------------------------------------------------------

fn func_label(f: &Value) -> String {
    match f {
        Value::Func(h) if h.anon.is_none() => {
            h.name.rsplit('.').next().unwrap_or(&h.name).to_string()
        }
        _ => "Fun".into(),
    }
}

fn varfun(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "varfun")?;
    let f = a[0].clone();
    let t = arg_tab(&a[1], "varfun")?;
    let nv = name_values(&a[2..], "varfun")?;
    let mut inputs: Vec<usize> = (0..t.width()).collect();
    let mut groups: Vec<usize> = Vec::new();
    let mut format_ = "table".to_string();
    for (n, v) in &nv {
        match n.to_ascii_lowercase().as_str() {
            "inputvariables" => inputs = var_list(&t, v)?,
            "groupingvariables" => groups = var_list(&t, v)?,
            "outputformat" => format_ = v.to_str().unwrap_or_default().to_ascii_lowercase(),
            "errorhandler" => {}
            _ => {
                return Err(err(
                    "MATLAB:table:varfun:InvalidParameter",
                    format!("varfun: unknown parameter '{}'.", n),
                ));
            }
        }
    }
    inputs.retain(|k| !groups.contains(k));
    let label = func_label(&f);
    if groups.is_empty() {
        let mut out = Tab::new(1);
        let mut cells = Vec::new();
        for &k in &inputs {
            let r = it
                .call_value(&f, vec![t.vars[k].clone()], 1)?
                .into_iter()
                .next()
                .unwrap_or_default();
            cells.push(r.clone());
            if format_ == "table" {
                if out.vars.is_empty() {
                    out.h = height_of(&r);
                }
                out.names.push(format!("{}_{}", label, t.names[k]));
                out.vars.push(r);
            }
        }
        return Ok(vec![match format_.as_str() {
            "uniform" => dt_cat(cells, 1)?,
            "cell" => Value::cell_row(cells),
            _ => {
                if out.vars.iter().any(|v| height_of(v) != out.h) {
                    return Err(err(
                        "MATLAB:table:varfun:OutputRowsMismatch",
                        "varfun: all function results must have the same number of rows.",
                    ));
                }
                out.value()
            }
        }]);
    }
    let (gidx, gfirst) = group_rows(&t, &groups)?;
    let ng = gfirst.len();
    let mut out = t.take_vars(&groups).take_rows(&gfirst)?;
    out.rows.clear();
    let counts: Vec<f64> = (0..ng)
        .map(|g| gidx.iter().filter(|&&x| x == g).count() as f64)
        .collect();
    out.set_var("GroupCount", super::num_col(counts))?;
    for &k in &inputs {
        let mut vals = Vec::with_capacity(ng);
        for g in 0..ng {
            let rows: Vec<usize> = (0..t.h).filter(|&i| gidx[i] == g).collect();
            let sub = rows_of(&t.vars[k], &rows)?;
            vals.push(
                it.call_value(&f, vec![sub], 1)?
                    .into_iter()
                    .next()
                    .unwrap_or_default(),
            );
        }
        out.set_var(&format!("{}_{}", label, t.names[k]), dt_cat(vals, 0)?)?;
    }
    Ok(vec![out.value()])
}

/// Group number of each row (groups in sorted key order) and the first
/// row of each group.
fn group_rows(t: &Tab, groups: &[usize]) -> R<(Vec<usize>, Vec<usize>)> {
    let vars: Vec<(usize, bool)> = groups.iter().map(|&k| (k, false)).collect();
    let order = sort_order(t, &vars, false)?;
    let mut keys: Vec<Vec<Key>> = vec![Vec::new(); t.h];
    for &k in groups {
        for (row, r) in keys.iter_mut().zip(row_keys(&t.vars[k], t.h)?) {
            row.extend(r);
        }
    }
    let same = |a: &[Key], b: &[Key]| {
        a.iter()
            .zip(b)
            .all(|(x, y)| cmp_key(x, y) == Ordering::Equal && !matches!(x, Key::N(v) if v.is_nan()))
    };
    let mut gidx = vec![0usize; t.h];
    let mut first = Vec::new();
    let mut prev: Option<usize> = None;
    for &r in &order {
        match prev {
            Some(p) if same(&keys[p], &keys[r]) => {}
            _ => first.push(r),
        }
        gidx[r] = first.len() - 1;
        prev = Some(r);
    }
    Ok((gidx, first))
}

fn rowfun(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "rowfun")?;
    let _ = nargout;
    let f = a[0].clone();
    let t = arg_tab(&a[1], "rowfun")?;
    let nv = name_values(&a[2..], "rowfun")?;
    let mut inputs: Vec<usize> = (0..t.width()).collect();
    let mut nout = 1usize;
    let mut out_names: Option<Vec<String>> = None;
    let mut format_ = "table".to_string();
    let mut separate = true;
    for (n, v) in &nv {
        match n.to_ascii_lowercase().as_str() {
            "inputvariables" => inputs = var_list(&t, v)?,
            "numoutputs" => nout = v.scalar_f64().unwrap_or(1.0) as usize,
            "outputvariablenames" => {
                let names = names_opt(v, "OutputVariableNames")?;
                nout = names.len();
                out_names = Some(names);
            }
            "outputformat" => format_ = v.to_str().unwrap_or_default().to_ascii_lowercase(),
            "separateinputs" => separate = v.scalar_f64().unwrap_or(1.0) != 0.0,
            "groupingvariables" => {
                return Err(err(
                    "MATLAB:table:rowfun:Unsupported",
                    "rowfun: GroupingVariables is not supported yet.",
                ));
            }
            "errorhandler" => {}
            _ => {
                return Err(err(
                    "MATLAB:table:rowfun:InvalidParameter",
                    format!("rowfun: unknown parameter '{}'.", n),
                ));
            }
        }
    }
    let mut cols: Vec<Vec<Value>> = vec![Vec::new(); nout];
    for i in 0..t.h {
        let args: Vec<Value> = if separate {
            inputs
                .iter()
                .map(|&k| elem_of_row_keep(&t.vars[k], i))
                .collect::<R<Vec<_>>>()?
        } else {
            let parts = inputs
                .iter()
                .map(|&k| rows_of(&t.vars[k], &[i]))
                .collect::<R<Vec<_>>>()?;
            vec![hcat_vars(parts)?]
        };
        let r = it.call_value(&f, args, nout)?;
        if r.len() < nout {
            return Err(err(
                "MATLAB:table:rowfun:TooFewOutputs",
                "rowfun: the function returned fewer outputs than requested.",
            ));
        }
        for (c, v) in cols.iter_mut().zip(r) {
            c.push(v);
        }
    }
    let names = out_names.unwrap_or_else(|| {
        if nout == 1 {
            vec![format!("{}_output", func_label(&f)).replace("Fun_output", "Var1")]
        } else {
            (1..=nout).map(|k| format!("Var{}", k)).collect()
        }
    });
    match format_.as_str() {
        "uniform" => {
            let vals = cols
                .into_iter()
                .map(|c| {
                    if c.is_empty() {
                        Ok(Value::Num(Num::real(vec![0, 1], vec![])))
                    } else {
                        dt_cat(c, 0)
                    }
                })
                .collect::<R<Vec<_>>>()?;
            Ok(vals)
        }
        "cell" => {
            let h = t.h;
            let mut data = Vec::with_capacity(h * nout);
            for c in cols {
                data.extend(c);
            }
            Ok(vec![Value::Cell(Array::new(vec![h, nout], data))])
        }
        _ => {
            let mut out = Tab::new(t.h);
            out.rows = t.rows.clone();
            for (n, c) in names.iter().zip(cols) {
                let v = if c.is_empty() {
                    Value::Num(Num::real(vec![0, 1], vec![]))
                } else {
                    column_from_cells(c)
                };
                out.set_var(n, v)?;
            }
            Ok(vec![out.value()])
        }
    }
}

/// Row `i` of a variable for rowfun: cell contents of cellstr variables.
fn elem_of_row_keep(v: &Value, i: usize) -> R<Value> {
    elem_of_row(v, i)
}

fn groupsummary(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "groupsummary")?;
    let t = arg_tab(&a[0], "groupsummary")?;
    let groups = var_list(&t, &a[1])?;
    let mut methods: Vec<Value> = Vec::new();
    let mut datavars: Option<Vec<usize>> = None;
    let mut i = 2;
    if let Some(m) = a.get(i) {
        match m {
            Value::Cell(c) => methods = c.data.clone(),
            Value::Func(_) => methods = vec![m.clone()],
            _ if m.is_text() => {
                let s = m.to_str().unwrap_or_default();
                if s.eq_ignore_ascii_case("IncludeEmptyGroups")
                    || s.eq_ignore_ascii_case("IncludeMissingGroups")
                {
                    i = a.len();
                } else {
                    methods = text_list(m)
                        .unwrap_or_default()
                        .iter()
                        .map(|s| Value::str(s))
                        .collect();
                }
            }
            _ => {}
        }
        i += 1;
    }
    if let Some(d) = a.get(i)
        && !d.to_str().is_some_and(|s| s.starts_with("Include"))
    {
        datavars = Some(var_list(&t, d)?);
    }
    let data: Vec<usize> = match datavars {
        Some(d) => d,
        None => (0..t.width()).filter(|k| !groups.contains(k)).collect(),
    };
    let (gidx, gfirst) = group_rows(&t, &groups)?;
    let ng = gfirst.len();
    let mut out = t.take_vars(&groups).take_rows(&gfirst)?;
    out.rows.clear();
    let counts: Vec<f64> = (0..ng)
        .map(|g| gidx.iter().filter(|&&x| x == g).count() as f64)
        .collect();
    out.set_var("GroupCount", super::num_col(counts))?;
    let mut fun_n = 0;
    for m in &methods {
        let (label, func) = match m {
            Value::Func(_) => {
                fun_n += 1;
                (format!("fun{}", fun_n), m.clone())
            }
            _ => {
                let s = m.to_str().unwrap_or_default().to_ascii_lowercase();
                let fname = match s.as_str() {
                    "mean" | "sum" | "min" | "max" | "median" | "mode" | "var" | "std"
                    | "numel" | "nnz" | "range" => s.clone(),
                    "nummissing" => "nummissing".into(),
                    "all" => {
                        return Err(err(
                            "MATLAB:groupsummary:Unsupported",
                            "groupsummary: method 'all' is not supported yet; list the methods.",
                        ));
                    }
                    _ => {
                        return Err(err(
                            "MATLAB:groupsummary:InvalidMethod",
                            format!("groupsummary: unknown method '{}'.", s),
                        ));
                    }
                };
                (s, Value::str(&fname))
            }
        };
        for &k in &data {
            let mut vals = Vec::with_capacity(ng);
            for g in 0..ng {
                let rows: Vec<usize> = (0..t.h).filter(|&i| gidx[i] == g).collect();
                let sub = rows_of(&t.vars[k], &rows)?;
                let r = match &func {
                    Value::Func(_) => it.call_value(&func, vec![sub], 1)?,
                    _ => {
                        let name = func.to_str().unwrap_or_default();
                        match name.as_str() {
                            "range" => {
                                let mx = it.call_named("max", vec![sub.clone()], 1)?;
                                let mn = it.call_named("min", vec![sub], 1)?;
                                vec![indy_value::ops::binary(BinOp::Sub, &mx[0], &mn[0])?]
                            }
                            "nummissing" => {
                                let m = it.call_named("ismissing", vec![sub], 1)?;
                                it.call_named("nnz", m, 1)?
                            }
                            "mean" | "sum" | "median" | "var" | "std" | "min" | "max" => {
                                let omit = it.call_named("rmmissing", vec![sub.clone()], 1);
                                let x = omit.ok().and_then(|v| v.into_iter().next()).unwrap_or(sub);
                                it.call_named(&name, vec![x], 1)?
                            }
                            _ => it.call_named(&name, vec![sub], 1)?,
                        }
                    }
                };
                vals.push(r.into_iter().next().unwrap_or_default());
            }
            out.set_var(&format!("{}_{}", label, t.names[k]), dt_cat(vals, 0)?)?;
        }
    }
    Ok(vec![out.value()])
}

/// Key columns of the rows of a table as comparable keys.
fn keys_of(t: &Tab, vars: &[usize]) -> R<Vec<Vec<Key>>> {
    let mut keys: Vec<Vec<Key>> = vec![Vec::new(); t.h];
    for &k in vars {
        for (row, r) in keys.iter_mut().zip(row_keys(&t.vars[k], t.h)?) {
            row.extend(r);
        }
    }
    Ok(keys)
}

fn same_keys(a: &[Key], b: &[Key]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| cmp_key(x, y) == Ordering::Equal)
}

struct JoinSpec {
    lk: Vec<usize>,
    rk: Vec<usize>,
    lv: Vec<usize>,
    rv: Vec<usize>,
    merge: bool,
    kind: String,
}

fn join_spec(l: &Tab, r: &Tab, opts: &[(String, Value)], default_merge: bool) -> R<JoinSpec> {
    let mut lk: Option<Vec<usize>> = None;
    let mut rk: Option<Vec<usize>> = None;
    let mut lv: Option<Vec<usize>> = None;
    let mut rv: Option<Vec<usize>> = None;
    let mut merge = default_merge;
    let mut kind = "full".to_string();
    for (n, v) in opts {
        match n.to_ascii_lowercase().as_str() {
            "keys" => {
                let names = text_list(v);
                match names {
                    Some(ns) => {
                        lk = Some(
                            ns.iter()
                                .map(|n| l.var_index(n).ok_or_else(|| no_var(n)))
                                .collect::<R<_>>()?,
                        );
                        rk = Some(
                            ns.iter()
                                .map(|n| r.var_index(n).ok_or_else(|| no_var(n)))
                                .collect::<R<_>>()?,
                        );
                    }
                    None => {
                        lk = Some(var_list(l, v)?);
                        rk = Some(var_list(r, v)?);
                    }
                }
            }
            "leftkeys" => lk = Some(var_list(l, v)?),
            "rightkeys" => rk = Some(var_list(r, v)?),
            "leftvariables" => lv = Some(var_list(l, v)?),
            "rightvariables" => rv = Some(var_list(r, v)?),
            "mergekeys" => merge = v.scalar_f64().unwrap_or(0.0) != 0.0,
            "type" => kind = v.to_str().unwrap_or_default().to_ascii_lowercase(),
            _ => {
                return Err(err(
                    "MATLAB:table:join:InvalidParameter",
                    format!("join: unknown parameter '{}'.", n),
                ));
            }
        }
    }
    let (lk, rk) = match (lk, rk) {
        (Some(a), Some(b)) => (a, b),
        (None, None) => {
            let common: Vec<String> = l
                .names
                .iter()
                .filter(|n| r.var_index(n).is_some())
                .cloned()
                .collect();
            if common.is_empty() {
                return Err(err(
                    "MATLAB:table:join:CantInferKey",
                    "Cannot find a common table variable to use as a key variable.",
                ));
            }
            (
                common.iter().map(|n| l.var_index(n).unwrap()).collect(),
                common.iter().map(|n| r.var_index(n).unwrap()).collect(),
            )
        }
        _ => {
            return Err(err(
                "MATLAB:table:join:KeysMismatch",
                "join: specify both LeftKeys and RightKeys.",
            ));
        }
    };
    if lk.len() != rk.len() {
        return Err(err(
            "MATLAB:table:join:KeysMismatch",
            "join: LeftKeys and RightKeys must have the same number of variables.",
        ));
    }
    Ok(JoinSpec {
        lv: lv.unwrap_or_else(|| (0..l.width()).collect()),
        rv: rv.unwrap_or_else(|| (0..r.width()).filter(|k| !rk.contains(k)).collect()),
        lk,
        rk,
        merge,
        kind,
    })
}

/// Output names for left and right variables; clashing names get the
/// table names (`_left`, `_right`) appended as MATLAB does.
fn join_names(
    l: &Tab,
    r: &Tab,
    lv: &[usize],
    rv: &[usize],
    ln: &str,
    rn: &str,
) -> (Vec<String>, Vec<String>) {
    let a: Vec<String> = lv.iter().map(|&k| l.names[k].clone()).collect();
    let b: Vec<String> = rv.iter().map(|&k| r.names[k].clone()).collect();
    let la: Vec<String> = a
        .iter()
        .map(|n| {
            if b.contains(n) {
                format!("{}_{}", n, ln)
            } else {
                n.clone()
            }
        })
        .collect();
    let lb: Vec<String> = b
        .iter()
        .map(|n| {
            if a.contains(n) {
                format!("{}_{}", n, rn)
            } else {
                n.clone()
            }
        })
        .collect();
    (la, lb)
}

fn build_join(
    l: &Tab,
    r: &Tab,
    pairs: &[(Option<usize>, Option<usize>)],
    s: &JoinSpec,
    ln: &str,
    rn: &str,
) -> R<Tab> {
    let mut out = Tab::new(pairs.len());
    let lrows: Vec<usize> = pairs.iter().map(|p| p.0.unwrap_or(0)).collect();
    let rrows: Vec<usize> = pairs.iter().map(|p| p.1.unwrap_or(0)).collect();
    let mut lv = s.lv.clone();
    let mut rv = s.rv.clone();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    if s.merge {
        for (a, b) in s.lk.iter().zip(&s.rk) {
            merged.push((*a, *b));
        }
        lv.retain(|k| !s.lk.contains(k) || true);
        rv.retain(|k| !s.rk.contains(k));
    }
    let (la, lb) = join_names(l, r, &lv, &rv, ln, rn);
    let take = |v: &Value, rows: &[usize], valid: &[bool]| -> R<Value> {
        if v.dims()[0] == 0 {
            return rows_of(v, &[]);
        }
        let x = rows_of(v, rows)?;
        if valid.iter().all(|b| *b) {
            return Ok(x);
        }
        blank_rows(x, valid)
    };
    let lvalid: Vec<bool> = pairs.iter().map(|p| p.0.is_some()).collect();
    let rvalid: Vec<bool> = pairs.iter().map(|p| p.1.is_some()).collect();
    for (j, &k) in lv.iter().enumerate() {
        let mut v = take(&l.vars[k], &lrows, &lvalid)?;
        if let Some(&(_, rk)) = merged.iter().find(|(a, _)| *a == k) {
            // Merged key: take the right value where the left is missing.
            let rvv = take(&r.vars[rk], &rrows, &rvalid)?;
            let fill: Vec<usize> = (0..pairs.len()).filter(|&i| !lvalid[i]).collect();
            if !fill.is_empty() {
                let part = rows_of(&rvv, &fill)?;
                v = set_rows(v, &fill, part)?;
            }
        }
        out.names.push(la[j].clone());
        out.vars.push(v);
    }
    for (j, &k) in rv.iter().enumerate() {
        out.names.push(lb[j].clone());
        out.vars.push(take(&r.vars[k], &rrows, &rvalid)?);
    }
    check_unique(&out.names)?;
    Ok(out)
}

/// Replace rows where `valid` is false by missing values.
fn blank_rows(v: Value, valid: &[bool]) -> R<Value> {
    let bad: Vec<usize> = (0..valid.len()).filter(|&i| !valid[i]).collect();
    let w: usize = v.dims()[1..].iter().product();
    let fill = match &v {
        Value::Num(_) => Value::Num(Num::real(vec![bad.len(), w], vec![f64::NAN; bad.len() * w])),
        Value::Bool(_) => bools(vec![bad.len(), w], vec![false; bad.len() * w]),
        Value::Str(_) => Value::Str(Array::new(vec![bad.len(), w], vec![None; bad.len() * w])),
        Value::Cell(_) => Value::Cell(Array::new(
            vec![bad.len(), w],
            vec![Value::str(""); bad.len() * w],
        )),
        _ if class_is(&v, "datetime") => super::time::nat_array(vec![bad.len(), w]),
        _ if class_is(&v, "categorical") => {
            let mut o = super::obj_of(&v).unwrap().clone();
            o = super::ew_index(
                &o,
                &[
                    Sub::List {
                        idx: vec![],
                        shape: vec![0, 1],
                    },
                    Sub::Colon,
                ],
            )?;
            let _ = o;
            return Ok(v);
        }
        _ => return Ok(v),
    };
    set_rows(v, &bad, fill)
}

fn innerjoin(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "innerjoin")?;
    let l = arg_tab(&a[0], "innerjoin")?;
    let r = arg_tab(&a[1], "innerjoin")?;
    let opts = name_values(&a[2..], "innerjoin")?;
    let s = join_spec(&l, &r, &opts, false)?;
    let lkeys = keys_of(&l, &s.lk)?;
    let rkeys = keys_of(&r, &s.rk)?;
    let mut pairs = Vec::new();
    for (i, lk) in lkeys.iter().enumerate() {
        for (j, rk) in rkeys.iter().enumerate() {
            if same_keys(lk, rk) {
                pairs.push((i, j));
            }
        }
    }
    // MATLAB sorts the result by the key values.
    pairs.sort_by(|p, q| {
        for (x, y) in lkeys[p.0].iter().zip(&lkeys[q.0]) {
            let o = cmp_key(x, y);
            if o != Ordering::Equal {
                return o;
            }
        }
        (p.0, p.1).cmp(&(q.0, q.1))
    });
    let mut spec = s;
    spec.merge = false;
    let pp: Vec<(Option<usize>, Option<usize>)> =
        pairs.iter().map(|p| (Some(p.0), Some(p.1))).collect();
    let out = build_join(&l, &r, &pp, &spec, "left", "right")?;
    let mut res = vec![out.value()];
    if nargout > 1 {
        res.push(super::num_col(
            pairs.iter().map(|p| (p.0 + 1) as f64).collect(),
        ));
        res.push(super::num_col(
            pairs.iter().map(|p| (p.1 + 1) as f64).collect(),
        ));
    }
    Ok(res)
}

fn outerjoin(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "outerjoin")?;
    let l = arg_tab(&a[0], "outerjoin")?;
    let r = arg_tab(&a[1], "outerjoin")?;
    let opts = name_values(&a[2..], "outerjoin")?;
    let mut s = join_spec(&l, &r, &opts, false)?;
    if !opts
        .iter()
        .any(|(n, _)| n.eq_ignore_ascii_case("RightVariables"))
    {
        s.rv = (0..r.width()).collect();
        if s.merge {
            s.rv.retain(|k| !s.rk.contains(k));
        }
    }
    let lkeys = keys_of(&l, &s.lk)?;
    let rkeys = keys_of(&r, &s.rk)?;
    let mut pairs: Vec<(Option<usize>, Option<usize>)> = Vec::new();
    let mut rused = vec![false; r.h];
    for (i, lk) in lkeys.iter().enumerate() {
        let mut hit = false;
        for (j, rk) in rkeys.iter().enumerate() {
            if same_keys(lk, rk) {
                pairs.push((Some(i), Some(j)));
                rused[j] = true;
                hit = true;
            }
        }
        if !hit && matches!(s.kind.as_str(), "full" | "left") {
            pairs.push((Some(i), None));
        }
    }
    if matches!(s.kind.as_str(), "full" | "right") {
        for (j, used) in rused.iter().enumerate() {
            if !used {
                pairs.push((None, Some(j)));
            }
        }
    }
    if s.kind == "right" {
        pairs.retain(|p| p.1.is_some());
    }
    let key_of = |p: &(Option<usize>, Option<usize>)| -> Vec<Key> {
        match p {
            (Some(i), _) => lkeys[*i].clone(),
            (None, Some(j)) => rkeys[*j].clone(),
            _ => Vec::new(),
        }
    };
    pairs.sort_by(|p, q| {
        let (a, b) = (key_of(p), key_of(q));
        for (x, y) in a.iter().zip(&b) {
            let o = cmp_key(x, y);
            if o != Ordering::Equal {
                return o;
            }
        }
        Ordering::Equal
    });
    let out = build_join(&l, &r, &pairs, &s, "left", "right")?;
    let mut res = vec![out.value()];
    if nargout > 1 {
        res.push(super::num_col(
            pairs
                .iter()
                .map(|p| p.0.map(|i| (i + 1) as f64).unwrap_or(0.0))
                .collect(),
        ));
        res.push(super::num_col(
            pairs
                .iter()
                .map(|p| p.1.map(|i| (i + 1) as f64).unwrap_or(0.0))
                .collect(),
        ));
    }
    Ok(res)
}

fn join(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "join")?;
    let l = arg_tab(&a[0], "join")?;
    let r = arg_tab(&a[1], "join")?;
    let opts = name_values(&a[2..], "join")?;
    let s = join_spec(&l, &r, &opts, false)?;
    let lkeys = keys_of(&l, &s.lk)?;
    let rkeys = keys_of(&r, &s.rk)?;
    let mut pairs = Vec::new();
    for (i, lk) in lkeys.iter().enumerate() {
        let hits: Vec<usize> = (0..r.h).filter(|&j| same_keys(lk, &rkeys[j])).collect();
        if hits.len() != 1 {
            return Err(err(
                "MATLAB:table:join:LeftKeyValueNotFound",
                format!(
                    "join: the key of left row {} matches {} rows of the right table; it must match exactly one.",
                    i + 1,
                    hits.len()
                ),
            ));
        }
        pairs.push((Some(i), Some(hits[0])));
    }
    let out = build_join(&l, &r, &pairs, &s, "left", "right")?;
    Ok(vec![out.value()])
}

fn summary(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "summary")?;
    let t = arg_tab(&a[0], "summary")?;
    let mut s = String::new();
    if !t.desc.is_empty() {
        s.push_str(&format!(
            "Description:  {}\n\n",
            t.desc.to_str().unwrap_or_default()
        ));
    }
    s.push_str("Variables:\n\n");
    for (n, v) in t.names.iter().zip(&t.vars) {
        let d = v.dims();
        s.push_str(&format!(
            "    {}: {} {}\n",
            n,
            d.iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join("×"),
            v.class_name()
        ));
        if v.is_numeric() && !v.is_empty() {
            let x = v.to_f64_vec().unwrap_or_default();
            let mut sorted: Vec<f64> = x.iter().copied().filter(|v| !v.is_nan()).collect();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
            let nmiss = x.len() - sorted.len();
            if !sorted.is_empty() {
                let med = {
                    let m = sorted.len();
                    if m % 2 == 1 {
                        sorted[m / 2]
                    } else {
                        (sorted[m / 2 - 1] + sorted[m / 2]) / 2.0
                    }
                };
                s.push_str("\n        Values:\n\n");
                s.push_str(&format!(
                    "            Min          {}\n",
                    format::fmt_g(sorted[0], 5, false, false)
                ));
                s.push_str(&format!(
                    "            Median       {}\n",
                    format::fmt_g(med, 5, false, false)
                ));
                s.push_str(&format!(
                    "            Max          {}\n",
                    format::fmt_g(*sorted.last().unwrap(), 5, false, false)
                ));
                if nmiss > 0 {
                    s.push_str(&format!("            NumMissing   {}\n", nmiss));
                }
            }
            s.push('\n');
        } else if matches!(v, Value::Bool(_)) {
            let x = v.to_f64_vec().unwrap_or_default();
            let tr = x.iter().filter(|v| **v != 0.0).count();
            s.push_str(&format!(
                "\n        Values:\n\n            True        {}\n            False       {}\n\n",
                tr,
                x.len() - tr
            ));
        } else if class_is(v, "categorical") {
            let o = obj_of(v).unwrap();
            let cats = super::categorical::cats(o);
            let codes = super::data(o, "#c");
            s.push('\n');
            for (k, c) in cats.iter().enumerate() {
                let n = codes.iter().filter(|x| **x as usize == k + 1).count();
                s.push_str(&format!("        {}      {}\n", c, n));
            }
            s.push('\n');
        } else {
            s.push('\n');
        }
    }
    if nargout > 0 {
        return Err(err(
            "MATLAB:table:summary:Unsupported",
            "summary: the structure output is not supported yet; call summary(T) to print.",
        ));
    }
    it.print(&s);
    Ok(vec![])
}

fn table_disp(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let t = arg_tab(&a[0], "disp")?;
    if t.h == 0 || t.width() == 0 {
        it.print(&format!("  {}×{} empty table\n\n", t.h, t.width()));
        return Ok(vec![]);
    }
    let s = disp_text(&t);
    it.print(&s);
    Ok(vec![])
}

fn table_display(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let o = super::obj_of(&a[0]).unwrap();
    let s = display("ans", o);
    it.print(&s);
    Ok(vec![])
}

fn table_isequal(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    Ok(vec![Value::bool(
        a.windows(2).all(|w| natives_equal(&w[0], &w[1], false)),
    )])
}

fn table_isequaln(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    Ok(vec![Value::bool(
        a.windows(2).all(|w| natives_equal(&w[0], &w[1], true)),
    )])
}

fn table_properties(_: &mut Interp, _: Vec<Value>, _: usize) -> R<Vec<Value>> {
    Ok(vec![Value::cell_col(vec![Value::str("Properties")])])
}

fn table_fieldnames(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let t = arg_tab(&a[0], "fieldnames")?;
    let mut n = t.names.clone();
    n.push("Properties".into());
    n.push(t.dimnames[0].clone());
    n.push(t.dimnames[1].clone());
    Ok(vec![cellstr_col(&n)])
}

fn table_cat(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let mut a = a;
    let d = a.remove(0).scalar_f64().unwrap_or(1.0) as usize;
    Ok(vec![cat(a, d.saturating_sub(1))?])
}

fn table_horzcat(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    Ok(vec![cat(a, 1)?])
}

fn table_vertcat(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    Ok(vec![cat(a, 0)?])
}

fn table_isobject(_: &mut Interp, _: Vec<Value>, _: usize) -> R<Vec<Value>> {
    Ok(vec![Value::bool(true)])
}

fn table_unique(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let t = arg_tab(&a[0], "unique")?;
    let all: Vec<usize> = (0..t.width()).collect();
    let (gidx, first) = group_rows(&t, &all)?;
    let mut out = vec![t.take_rows(&first)?.value()];
    if nargout > 1 {
        out.push(super::num_col(
            first.iter().map(|&i| (i + 1) as f64).collect(),
        ));
        out.push(super::num_col(
            gidx.iter().map(|&g| (g + 1) as f64).collect(),
        ));
    }
    Ok(out)
}

fn table_ismissing(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let t = arg_tab(&a[0], "ismissing")?;
    let mut cols: Vec<Value> = Vec::new();
    for v in &t.vars {
        let w: usize = v.dims()[1..].iter().product();
        let mut b = vec![false; t.h * w];
        match v {
            Value::Num(n) => {
                for (i, x) in n.re.iter().enumerate() {
                    b[i] = x.is_nan();
                }
            }
            Value::Str(s) => {
                for (i, x) in s.data.iter().enumerate() {
                    b[i] = x.is_none();
                }
            }
            Value::Cell(c) => {
                for (i, x) in c.data.iter().enumerate() {
                    b[i] = x.is_char() && x.is_empty();
                }
            }
            _ => {
                if let Some(o) = obj_of(v) {
                    let key = if o.class == "categorical" { "#c" } else { "#v" };
                    for (i, x) in super::data(o, key).iter().enumerate() {
                        b[i] = if key == "#c" { *x == 0.0 } else { x.is_nan() };
                    }
                }
            }
        }
        cols.push(bools(vec![t.h, w], b));
    }
    if cols.is_empty() {
        return Ok(vec![bools(vec![t.h, 0], vec![])]);
    }
    Ok(vec![indy_value::concat::cat(cols, 1)?])
}

fn rmmissing_t(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let t = arg_tab(&a[0], "rmmissing")?;
    let m = table_ismissing(it, vec![a[0].clone()], 1)?.remove(0);
    let w = m.dims()[1];
    let mv = m.to_f64_vec().unwrap_or_default();
    let keep: Vec<usize> = (0..t.h)
        .filter(|&i| (0..w).all(|j| mv[i + j * t.h] == 0.0))
        .collect();
    Ok(vec![t.take_rows(&keep)?.value()])
}

fn table_length(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let d = a[0].dims();
    Ok(vec![Value::num(if d.contains(&0) {
        0.0
    } else {
        d[0].max(d[1]) as f64
    })])
}

fn table_numel(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    Ok(vec![Value::num(a[0].numel() as f64)])
}

const METHODS: &[(&str, BuiltinFn)] = &[
    ("height", height),
    ("width", width),
    ("table2array", table2array),
    ("table2cell", table2cell),
    ("table2struct", table2struct),
    ("addvars", addvars),
    ("removevars", removevars),
    ("renamevars", renamevars),
    ("movevars", movevars),
    ("sortrows", sortrows),
    ("head", head),
    ("tail", tail),
    ("varfun", varfun),
    ("rowfun", rowfun),
    ("groupsummary", groupsummary),
    ("innerjoin", innerjoin),
    ("outerjoin", outerjoin),
    ("join", join),
    ("summary", summary),
    ("disp", table_disp),
    ("display", table_display),
    ("isequal", table_isequal),
    ("isequaln", table_isequaln),
    ("properties", table_properties),
    ("fieldnames", table_fieldnames),
    ("cat", table_cat),
    ("horzcat", table_horzcat),
    ("vertcat", table_vertcat),
    ("isobject", table_isobject),
    ("unique", table_unique),
    ("ismissing", table_ismissing),
    ("rmmissing", rmmissing_t),
    ("length", table_length),
    ("numel", table_numel),
];

pub fn register(it: &mut Interp) {
    reg(it, "table", table_fn);
    reg(it, "istable", istable);
    reg(it, "height", height);
    reg(it, "width", width);
    reg(it, "array2table", array2table);
    reg(it, "cell2table", cell2table);
    reg(it, "struct2table", struct2table);
    for (n, f) in METHODS {
        if !it.builtins.contains_key(n) {
            reg(it, n, *f);
        }
    }
}
