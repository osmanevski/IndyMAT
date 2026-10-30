//! classdef support: value and handle classes, properties (defaults,
//! Constant, Dependent, access, validation, get/set methods), methods
//! (instance, Static, overloaded operators, subsref/subsasgn/end/numel),
//! single inheritance chains, enumerations and simple events.
//!
//! Not supported in phase 2: object arrays, methods in `@Class` folders,
//! multiple-dispatch precedence rules, metaclass objects beyond `Name`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use indy_syntax::ast::{self, BinOp, ClassSection, Expr, Function, UnOp, VarDecl};
use indy_value::{Error, Object, StructArray, Value};

use crate::eval::EOp;
use crate::{FileCtx, Interp, R};

pub struct PropInfo {
    pub name: String,
    pub decl: VarDecl,
    pub constant: bool,
    pub dependent: bool,
    pub get_private: bool,
    pub set_private: bool,
    pub owner: String,
}

pub struct MethodInfo {
    pub ctx: Rc<FileCtx>,
    pub func: Rc<Function>,
    pub is_static: bool,
    pub owner: String,
}

pub struct ClassInfo {
    pub name: String,
    pub ctx: Rc<FileCtx>,
    pub supers: Vec<Rc<ClassInfo>>,
    /// Built-in superclass such as `uint8` or `double` (enumerations).
    pub builtin_super: Option<String>,
    pub is_handle: bool,
    pub props: Vec<PropInfo>,
    pub methods: HashMap<String, Rc<MethodInfo>>,
    pub getters: HashMap<String, Rc<MethodInfo>>,
    pub setters: HashMap<String, Rc<MethodInfo>>,
    pub ctor: Option<Rc<MethodInfo>>,
    pub enums: Vec<(String, Vec<Expr>)>,
    pub events: Vec<String>,
    constants: RefCell<HashMap<String, Value>>,
}

impl ClassInfo {
    pub fn prop(&self, name: &str) -> Option<&PropInfo> {
        self.props.iter().find(|p| p.name == name)
    }
    pub fn method(&self, name: &str) -> Option<Rc<MethodInfo>> {
        self.methods.get(name).cloned()
    }
    pub fn is_a(&self, name: &str) -> bool {
        self.name == name
            || self.builtin_super.as_deref() == Some(name)
            || (name == "handle" && self.is_handle)
            || self.supers.iter().any(|s| s.is_a(name))
    }
}

fn attr_true(attrs: &[ast::Attribute], name: &str) -> bool {
    attrs.iter().any(|a| {
        a.name.name.eq_ignore_ascii_case(name)
            && !a.negated
            && !matches!(&a.value, Some(Expr { kind: ast::ExprKind::Ident(v), .. }) if v == "false")
    })
}

fn attr_private(attrs: &[ast::Attribute], which: &[&str]) -> bool {
    attrs.iter().any(|a| {
        which.iter().any(|w| a.name.name.eq_ignore_ascii_case(w))
            && match &a.value {
                Some(Expr {
                    kind: ast::ExprKind::Char(s) | ast::ExprKind::Str(s) | ast::ExprKind::Ident(s),
                    ..
                }) => s.eq_ignore_ascii_case("private") || s.eq_ignore_ascii_case("protected"),
                _ => false,
            }
    })
}

/// Class information attached to an object value.
pub fn class_of(v: &Value) -> Option<Rc<ClassInfo>> {
    let meta = match v {
        Value::Object(o) => o.meta.clone(),
        Value::HObject(o) => o.borrow().meta.clone(),
        Value::ObjArr(o) => o.meta.clone(),
        _ => None,
    }?;
    meta.downcast::<ClassInfo>().ok()
}

pub fn is_class_obj(v: &Value) -> bool {
    match v {
        Value::Object(o) => o.meta.is_some(),
        Value::HObject(o) => o.borrow().meta.is_some(),
        Value::ObjArr(o) => o.meta.is_some(),
        _ => false,
    }
}

/// Subscripts of evaluated `(...)` arguments.
fn subs_of(a: &[Value]) -> R<Vec<indy_value::index::Sub>> {
    a.iter().map(indy_value::index::to_sub).collect()
}

/// Read a stored property.
pub fn get_prop(v: &Value, name: &str) -> Option<Value> {
    match v {
        Value::Object(o) => o.get(name).cloned(),
        Value::HObject(o) => o.borrow().get(name).cloned(),
        _ => None,
    }
}

pub fn set_prop(v: &mut Value, name: &str, x: Value) {
    match v {
        Value::Object(o) => o.set(name, x),
        Value::HObject(o) => o.borrow_mut().set(name, x),
        _ => {}
    }
}

pub fn op_method(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "plus",
        BinOp::Sub => "minus",
        BinOp::Mul => "mtimes",
        BinOp::Div => "mrdivide",
        BinOp::LeftDiv => "mldivide",
        BinOp::Pow => "mpower",
        BinOp::EMul => "times",
        BinOp::EDiv => "rdivide",
        BinOp::ELeftDiv => "ldivide",
        BinOp::EPow => "power",
        BinOp::Eq => "eq",
        BinOp::Ne => "ne",
        BinOp::Lt => "lt",
        BinOp::Le => "le",
        BinOp::Gt => "gt",
        BinOp::Ge => "ge",
        BinOp::And => "and",
        BinOp::Or => "or",
        BinOp::AndAnd => "and",
        BinOp::OrOr => "or",
    }
}

pub fn unop_method(op: UnOp) -> &'static str {
    match op {
        UnOp::Neg => "uminus",
        UnOp::Plus => "uplus",
        UnOp::Not => "not",
        UnOp::Transpose => "transpose",
        UnOp::CTranspose => "ctranspose",
    }
}

fn subs_struct(eops: &[EOp]) -> Value {
    let mut s = StructArray::empty_with(vec!["type".into(), "subs".into()], vec![1, eops.len()]);
    for (i, op) in eops.iter().enumerate() {
        let (t, subs) = match op {
            EOp::Paren(a) => ("()", Value::cell_row(a.clone())),
            EOp::Brace(a) => ("{}", Value::cell_row(a.clone())),
            EOp::Field(f) => (".", Value::str(f)),
        };
        s.elems[i] = vec![Value::str(t), subs];
    }
    Value::Struct(s)
}

pub fn eops_from_subs(s: &Value) -> R<Vec<EOp>> {
    let Value::Struct(st) = s else {
        return Err(Error::new(
            "MATLAB:subsref:invalid",
            "subsref: S must be a struct with fields type and subs.",
        ));
    };
    let mut out = Vec::new();
    for i in 0..st.numel() {
        let t = st
            .get(i, "type")
            .and_then(|v| v.to_str())
            .unwrap_or_default();
        let subs = st.get(i, "subs").cloned().unwrap_or_default();
        let args = match &subs {
            Value::Cell(c) => c.data.clone(),
            other => vec![other.clone()],
        };
        out.push(match t.as_str() {
            "()" => EOp::Paren(args),
            "{}" => EOp::Brace(args),
            "." => EOp::Field(subs.to_str().unwrap_or_default()),
            _ => {
                return Err(Error::new(
                    "MATLAB:subsref:invalid",
                    format!("subsref: unknown type '{}'.", t),
                ));
            }
        });
    }
    Ok(out)
}

impl Interp {
    /// Load (or fetch) the class defined by a classdef file.
    pub fn class_from_ctx(&mut self, ctx: &Rc<FileCtx>) -> R<Rc<ClassInfo>> {
        let def = ctx
            .file
            .items
            .iter()
            .find_map(|i| match i {
                ast::Item::Class(c) => Some(c.clone()),
                _ => None,
            })
            .ok_or_else(|| Error::msg("internal: no classdef in file"))?;
        let name = {
            // Package classes are known by their dotted name.
            let mut n = def.name.name.clone();
            let mut dir = ctx.dir();
            let mut pkgs = Vec::new();
            while let Some(d) = dir {
                match d.file_name().and_then(|s| s.to_str()) {
                    Some(f) if f.starts_with('+') => {
                        pkgs.push(f[1..].to_string());
                        dir = d.parent();
                    }
                    _ => break,
                }
            }
            if !pkgs.is_empty() {
                pkgs.reverse();
                n = format!("{}.{}", pkgs.join("."), n);
            }
            n
        };
        if let Some(c) = self.classes.get(&name)
            && Rc::ptr_eq(&c.ctx, ctx)
        {
            return Ok(c.clone());
        }
        let mut supers = Vec::new();
        let mut builtin_super = None;
        let mut is_handle = false;
        for s in &def.superclasses {
            let sn = s.dotted();
            match sn.as_str() {
                "handle"
                | "dynamicprops"
                | "hgsetget"
                | "matlab.mixin.SetGet"
                | "matlab.mixin.Copyable" => is_handle = true,
                "double" | "single" | "logical" | "char" | "int8" | "int16" | "int32" | "int64"
                | "uint8" | "uint16" | "uint32" | "uint64" => builtin_super = Some(sn.clone()),
                _ if sn.starts_with("matlab.mixin.") => {}
                _ => match self.resolve_class(&sn)? {
                    Some(c) => {
                        is_handle |= c.is_handle;
                        if builtin_super.is_none() {
                            builtin_super = c.builtin_super.clone();
                        }
                        supers.push(c);
                    }
                    None => {
                        return Err(Error::new(
                            "MATLAB:class:SuperclassNotFound",
                            format!("The specified superclass '{}' could not be found.", sn),
                        ));
                    }
                },
            }
        }
        let mut props: Vec<PropInfo> = Vec::new();
        let mut methods: HashMap<String, Rc<MethodInfo>> = HashMap::new();
        let mut getters = HashMap::new();
        let mut setters = HashMap::new();
        for sup in &supers {
            for p in &sup.props {
                if !props.iter().any(|q| q.name == p.name) {
                    props.push(PropInfo {
                        name: p.name.clone(),
                        decl: p.decl.clone(),
                        constant: p.constant,
                        dependent: p.dependent,
                        get_private: p.get_private,
                        set_private: p.set_private,
                        owner: p.owner.clone(),
                    });
                }
            }
            for (k, m) in &sup.methods {
                methods.entry(k.clone()).or_insert_with(|| m.clone());
            }
            for (k, m) in &sup.getters {
                getters.entry(k.clone()).or_insert_with(|| m.clone());
            }
            for (k, m) in &sup.setters {
                setters.entry(k.clone()).or_insert_with(|| m.clone());
            }
        }
        let mut ctor = None;
        let mut enums = Vec::new();
        let mut events = Vec::new();
        let simple = def.name.name.clone();
        for sec in &def.sections {
            match sec {
                ClassSection::Properties {
                    attributes,
                    props: ps,
                    ..
                } => {
                    for d in ps {
                        let pname = d.name.dotted();
                        props.retain(|p| p.name != pname);
                        props.push(PropInfo {
                            name: pname,
                            decl: d.clone(),
                            constant: attr_true(attributes, "Constant"),
                            dependent: attr_true(attributes, "Dependent"),
                            get_private: attr_private(attributes, &["Access", "GetAccess"]),
                            set_private: attr_private(attributes, &["Access", "SetAccess"]),
                            owner: name.clone(),
                        });
                    }
                }
                ClassSection::Methods {
                    attributes,
                    functions,
                    ..
                } => {
                    let is_static = attr_true(attributes, "Static");
                    for f in functions {
                        let fname = f.name.dotted();
                        let mi = Rc::new(MethodInfo {
                            ctx: ctx.clone(),
                            func: Rc::new(f.clone()),
                            is_static,
                            owner: name.clone(),
                        });
                        if let Some(p) = fname.strip_prefix("get.") {
                            getters.insert(p.to_string(), mi);
                        } else if let Some(p) = fname.strip_prefix("set.") {
                            setters.insert(p.to_string(), mi);
                        } else if fname == simple {
                            ctor = Some(mi);
                        } else {
                            methods.insert(fname, mi);
                        }
                    }
                }
                ClassSection::Events { names, .. } => {
                    events.extend(names.iter().map(|n| n.name.clone()));
                }
                ClassSection::Enumeration { members, .. } => {
                    for m in members {
                        enums.push((m.name.name.clone(), m.args.clone()));
                    }
                }
            }
        }
        let info = Rc::new(ClassInfo {
            name: name.clone(),
            ctx: ctx.clone(),
            supers,
            builtin_super,
            is_handle,
            props,
            methods,
            getters,
            setters,
            ctor,
            enums,
            events,
            constants: RefCell::new(HashMap::new()),
        });
        self.classes.insert(name, info.clone());
        Ok(info)
    }

    pub fn resolve_class(&mut self, name: &str) -> R<Option<Rc<ClassInfo>>> {
        if let Some(c) = self.classes.get(name) {
            return Ok(Some(c.clone()));
        }
        let Some(p) = self.find_file(name) else {
            return Ok(None);
        };
        let ctx = self.load_file(&p)?;
        if ctx.file.kind != ast::FileKind::Class {
            return Ok(None);
        }
        self.class_from_ctx(&ctx).map(Some)
    }

    /// Is the current frame executing code of `cls` (or a subclass)?
    pub fn inside_class(&self, cls: &ClassInfo) -> bool {
        match &self.frame().class_ctx {
            Some(c) => c.is_a(&cls.name) || cls.is_a(&c.name),
            None => false,
        }
    }

    fn eval_in_class(&mut self, cls: &Rc<ClassInfo>, e: &Expr) -> R<Value> {
        let mut f = crate::Frame::new(cls.ctx.clone(), &cls.name);
        f.class_ctx = Some(cls.clone());
        self.frames.push(f);
        let r = self.eval(e);
        self.frames.pop();
        r
    }

    pub fn class_constant(&mut self, cls: &Rc<ClassInfo>, name: &str) -> R<Option<Value>> {
        if let Some(v) = cls.constants.borrow().get(name) {
            return Ok(Some(v.clone()));
        }
        let Some(p) = cls.prop(name).filter(|p| p.constant) else {
            return Ok(None);
        };
        let v = match &p.decl.default {
            Some(e) => {
                let e = e.clone();
                self.eval_in_class(cls, &e)?
            }
            None => Value::empty(),
        };
        cls.constants
            .borrow_mut()
            .insert(name.to_string(), v.clone());
        Ok(Some(v))
    }

    /// Fresh instance with default property values.
    pub fn default_object(&mut self, cls: &Rc<ClassInfo>) -> R<Value> {
        let mut o = Object::new(&cls.name);
        o.meta = Some(cls.clone() as Rc<dyn std::any::Any>);
        for p in &cls.props {
            if p.constant || p.dependent {
                continue;
            }
            let v = match &p.decl.default {
                Some(e) => {
                    let e = e.clone();
                    self.eval_in_class(cls, &e)?
                }
                None => Value::empty(),
            };
            o.props.push((p.name.clone(), v));
        }
        Ok(if cls.is_handle {
            Value::HObject(Rc::new(RefCell::new(o)))
        } else {
            Value::Object(Box::new(o))
        })
    }

    pub fn call_method_info(
        &mut self,
        cls: &Rc<ClassInfo>,
        m: &MethodInfo,
        args: Vec<Value>,
        nargout: usize,
    ) -> R<Vec<Value>> {
        let owner = self
            .classes
            .get(&m.owner)
            .cloned()
            .unwrap_or_else(|| cls.clone());
        self.call_user_ex(
            m.ctx.clone(),
            m.func.clone(),
            args,
            nargout,
            vec![],
            Some(owner),
        )
    }

    /// `ClassName(args)`.
    pub fn construct(
        &mut self,
        cls: &Rc<ClassInfo>,
        args: Vec<Value>,
        nargout: usize,
    ) -> R<Vec<Value>> {
        let _ = nargout;
        if !cls.enums.is_empty() {
            return Err(Error::new(
                "MATLAB:class:EnumerationConstructor",
                format!(
                    "Enumeration class '{}' cannot be constructed directly.",
                    cls.name
                ),
            ));
        }
        let obj = self.default_object(cls)?;
        self.run_constructor(cls, obj, args)
    }

    fn run_constructor(
        &mut self,
        cls: &Rc<ClassInfo>,
        obj: Value,
        args: Vec<Value>,
    ) -> R<Vec<Value>> {
        match cls.ctor.clone() {
            Some(c) => {
                let out = c
                    .func
                    .outputs
                    .first()
                    .map(|o| o.name.clone())
                    .unwrap_or_else(|| "obj".into());
                // Implicit no-argument superclass construction unless the
                // constructor calls obj@Super(...) itself.
                let mut obj = obj;
                let body_text = cls.ctx.text(c.func.span).to_string();
                for sup in cls.supers.clone() {
                    let simple = sup.name.rsplit('.').next().unwrap_or(&sup.name).to_string();
                    if !body_text.contains(&format!("@{}", simple)) && sup.ctor.is_some() {
                        obj = self
                            .run_constructor(&sup, obj, vec![])?
                            .into_iter()
                            .next()
                            .unwrap_or_default();
                    }
                }
                let r = self.call_user_ex(
                    c.ctx.clone(),
                    c.func.clone(),
                    args,
                    1,
                    vec![(out, obj)],
                    Some(cls.clone()),
                )?;
                Ok(r.into_iter().take(1).collect())
            }
            None => {
                let mut obj = obj;
                for sup in cls.supers.clone() {
                    if sup.ctor.is_some() {
                        obj = self
                            .run_constructor(&sup, obj, args.clone())?
                            .into_iter()
                            .next()
                            .unwrap_or_default();
                    }
                }
                Ok(vec![obj])
            }
        }
    }

    /// `obj@Super(args)` in a constructor, or `method@Super(obj, args)`.
    pub fn super_call(
        &mut self,
        method: &str,
        superclass: &str,
        args: Vec<Value>,
        nargout: usize,
    ) -> R<Vec<Value>> {
        let sup = self.resolve_class(superclass)?.ok_or_else(|| {
            Error::new(
                "MATLAB:class:SuperclassNotFound",
                format!("Superclass '{}' not found.", superclass),
            )
        })?;
        if let Some(obj) = self.get_var(method).cloned() {
            let r = self.run_constructor(&sup, obj, args)?;
            if let Some(v) = r.into_iter().next() {
                self.set_var(method, v);
            }
            return Ok(vec![]);
        }
        match sup.method(method) {
            Some(m) => self.call_method_info(&sup, &m, args, nargout),
            None => Err(Error::new(
                "MATLAB:class:MethodNotFound",
                format!("No method '{}' in superclass '{}'.", method, superclass),
            )),
        }
    }

    /// Method dispatch for `name(args...)` when an argument is an object.
    pub fn dispatch_method(
        &mut self,
        name: &str,
        args: &[Value],
    ) -> Option<(Rc<ClassInfo>, Rc<MethodInfo>)> {
        for a in args {
            if let Some(cls) = class_of(a)
                && let Some(m) = cls.method(name)
            {
                return Some((cls, m));
            }
        }
        None
    }

    /// `Class.member` / `Class.member(args)` for static methods,
    /// constants and enumeration members.
    pub fn class_member(
        &mut self,
        cls: &Rc<ClassInfo>,
        member: &str,
        args: Option<Vec<Value>>,
        nargout: usize,
    ) -> R<Vec<Value>> {
        if let Some(pos) = cls.enums.iter().position(|(n, _)| n == member) {
            return Ok(vec![self.enum_member(cls, pos)?]);
        }
        if let Some(v) = self.class_constant(cls, member)? {
            return Ok(vec![v]);
        }
        if let Some(m) = cls.method(member) {
            return self.call_method_info(cls, &m, args.unwrap_or_default(), nargout);
        }
        if member == "empty" {
            return Ok(vec![empty_array(cls, args.unwrap_or_default())?]);
        }
        Err(Error::new(
            "MATLAB:subscripting:classHasNoPropertyOrMethod",
            format!(
                "The class {} has no Constant property or Static method named '{}'.",
                cls.name, member
            ),
        ))
    }

    pub fn enum_member(&mut self, cls: &Rc<ClassInfo>, pos: usize) -> R<Value> {
        let key = format!("__enum__{}", cls.enums[pos].0);
        if let Some(v) = cls.constants.borrow().get(&key) {
            return Ok(v.clone());
        }
        let (name, args) = cls.enums[pos].clone();
        let mut vals = Vec::new();
        for a in &args {
            vals.push(self.eval_in_class(cls, a)?);
        }
        let mut obj = self.default_object(cls)?;
        set_prop(&mut obj, "__member__", Value::str(&name));
        if cls.builtin_super.is_some() {
            set_prop(
                &mut obj,
                "__value__",
                vals.first()
                    .cloned()
                    .unwrap_or_else(|| Value::num(pos as f64)),
            );
        } else if cls.ctor.is_some() {
            obj = self
                .run_constructor(cls, obj, vals)?
                .into_iter()
                .next()
                .unwrap_or_default();
        }
        cls.constants.borrow_mut().insert(key, obj.clone());
        Ok(obj)
    }

    /// Replace the gaps that growing an object array left (`a(3) = obj`
    /// with `a` shorter) by default objects: the no-argument constructor
    /// runs once and value objects are copied; handle objects are
    /// constructed per gap so that they stay distinct.
    pub fn fill_object_gaps(&mut self, v: &mut Value) -> R<()> {
        let Value::ObjArr(o) = v else { return Ok(()) };
        if o.arr.data.iter().all(Value::is_object_like) {
            return Ok(());
        }
        let Some(cls) = class_of(v) else {
            return Ok(());
        };
        let Value::ObjArr(o) = v else { unreachable!() };
        let mut proto: Option<Value> = None;
        for e in o.arr.data.iter_mut() {
            if e.is_object_like() {
                continue;
            }
            let d = match &proto {
                Some(p) if !cls.is_handle => p.clone(),
                _ => {
                    let d = self.construct(&cls, vec![], 1)?.into_iter().next();
                    d.unwrap_or_default()
                }
            };
            proto = Some(d.clone());
            *e = d;
        }
        Ok(())
    }

    /// Read `obj.name` with class rules.
    pub fn object_field(
        &mut self,
        obj: &Value,
        name: &str,
        call_args: Option<Vec<Value>>,
        nargout: usize,
    ) -> R<(Vec<Value>, bool)> {
        let cls = class_of(obj).unwrap();
        let inside = self.inside_class(&cls);
        if let Some(g) = cls.getters.get(name).cloned() {
            let in_getter = self.frame().fname == format!("get.{}", name);
            if !in_getter {
                let v = self.call_method_info(&cls, &g, vec![obj.clone()], 1)?;
                return Ok((v, false));
            }
        }
        if let Some(p) = cls.prop(name) {
            if p.get_private && !inside {
                return Err(Error::new(
                    "MATLAB:class:GetProhibited",
                    format!(
                        "Cannot access the private property '{}' of class '{}'.",
                        name, cls.name
                    ),
                ));
            }
            if p.constant {
                let v = self.class_constant(&cls, name)?.unwrap_or_default();
                return Ok((vec![v], false));
            }
            return Ok((vec![get_prop(obj, name).unwrap_or_default()], false));
        }
        if let Some(m) = cls.method(name) {
            let mut args = vec![obj.clone()];
            let consumed = call_args.is_some();
            if m.is_static {
                args.clear();
            }
            args.extend(call_args.unwrap_or_default());
            return Ok((self.call_method_info(&cls, &m, args, nargout)?, consumed));
        }
        if let Some(v) = get_prop(obj, name) {
            return Ok((vec![v], false));
        }
        Err(Error::new(
            "MATLAB:noSuchMethodOrField",
            format!(
                "Unrecognized method, property, or field '{}' for class '{}'.",
                name, cls.name
            ),
        ))
    }

    /// Apply indexing ops to a class instance (overloads first).
    pub fn object_ops(&mut self, obj: Value, eops: &[EOp], nargout: usize) -> R<Vec<Value>> {
        let cls = class_of(&obj).unwrap();
        if !self.inside_class(&cls)
            && let Some(m) = cls.method("subsref")
        {
            let s = subs_struct(eops);
            return self.call_method_info(&cls, &m, vec![obj, s], nargout.max(1));
        }
        self.builtin_object_ops(obj, eops, nargout)
    }

    pub fn builtin_object_ops(
        &mut self,
        obj: Value,
        eops: &[EOp],
        nargout: usize,
    ) -> R<Vec<Value>> {
        if let Value::ObjArr(arr) = &obj
            && let EOp::Field(name) = &eops[0]
        {
            let cls = class_of(&obj).unwrap();
            if cls.method(name).is_none() {
                // `objs.Prop`: comma-separated list over the elements.
                if eops.len() > 1 {
                    return Err(Error::new(
                        "MATLAB:index:expected_one_output_from_expression",
                        format!(
                            "Expected one output from a curly brace or dot indexing expression, but there were {} results.",
                            arr.arr.data.len()
                        ),
                    ));
                }
                let mut out = Vec::new();
                for e in arr.arr.data.clone() {
                    out.extend(self.object_field(&e, name, None, 1)?.0);
                }
                return Ok(out);
            }
        }
        match &eops[0] {
            EOp::Field(name) => {
                let args = match eops.get(1) {
                    Some(EOp::Paren(a)) => Some(a.clone()),
                    _ => None,
                };
                let more = eops.len() > 1 + args.is_some() as usize;
                let (vals, consumed) =
                    self.object_field(&obj, name, args.clone(), if more { 1 } else { nargout })?;
                let skip = 1 + consumed as usize;
                if skip >= eops.len() {
                    return Ok(vals);
                }
                let v = vals
                    .into_iter()
                    .next()
                    .ok_or_else(|| Error::msg("Expression produced no value."))?;
                self.apply_eops(v, &eops[skip..], nargout)
            }
            EOp::Paren(a) => {
                let v = indy_value::index::index_value(&obj, &subs_of(a)?)?;
                if eops.len() == 1 {
                    return Ok(vec![v]);
                }
                self.apply_eops(v, &eops[1..], nargout)
            }
            EOp::Brace(_) => Err(Error::new(
                "MATLAB:cellRefFromNonCell",
                "Brace indexing is not supported for variables of this type.",
            )),
        }
    }

    /// `var(...)... = rhs` where `var` holds a class instance.
    pub fn assign_object(&mut self, name: &str, eops: &[EOp], rhs: Value) -> R<()> {
        let obj = self.get_var(name).cloned().unwrap();
        let cls = class_of(&obj).unwrap();
        let inside = self.inside_class(&cls);
        if !inside && let Some(m) = cls.method("subsasgn") {
            let s = subs_struct(eops);
            let r = self.call_method_info(&cls, &m, vec![obj, s, rhs], 1)?;
            if let Some(v) = r.into_iter().next() {
                self.set_var(name, v);
            }
            return Ok(());
        }
        let mut new = self.object_assign_value(obj, &cls, eops, rhs, inside)?;
        self.fill_object_gaps(&mut new)?;
        self.set_var(name, new);
        Ok(())
    }

    fn object_assign_value(
        &mut self,
        obj: Value,
        cls: &Rc<ClassInfo>,
        eops: &[EOp],
        rhs: Value,
        inside: bool,
    ) -> R<Value> {
        let EOp::Field(pname) = &eops[0] else {
            let EOp::Paren(a) = &eops[0] else {
                return Err(Error::new(
                    "MATLAB:cellAssToNonCell",
                    "Brace indexing is not supported for variables of this type.",
                ));
            };
            let subs = subs_of(a)?;
            if eops.len() == 1 {
                // `a(k) = obj`, `a(k) = []`.
                if matches!(&rhs, Value::Num(n) if n.re.is_empty() && n.dims == [0, 0]) {
                    return indy_value::index::delete_index(obj, &subs);
                }
                let mut slot = Some(obj);
                indy_value::index::assign_index(&mut slot, &subs, rhs)?;
                return Ok(slot.unwrap_or_default());
            }
            // `a(k).Prop... = rhs`: one element, updated and put back.
            let elem = indy_value::index::index_value(&obj, &subs)?;
            if !elem.is_object_like() || matches!(elem, Value::ObjArr(_)) {
                return Err(Error::new(
                    "MATLAB:index:expected_one_output_from_expression",
                    "Assignment through an index needs exactly one object.",
                ));
            }
            let elem = self.object_assign_value(elem, cls, &eops[1..], rhs, inside)?;
            let mut slot = Some(obj);
            indy_value::index::assign_index(&mut slot, &subs, elem)?;
            return Ok(slot.unwrap_or_default());
        };
        if let Value::ObjArr(o) = &obj {
            return Err(Error::new(
                "MATLAB:index:expected_one_output_from_expression",
                format!(
                    "Assigning to property '{}' of a {} object array needs one element; index it first.",
                    pname,
                    indy_value::ops::dims_text(&o.arr.dims)
                ),
            ));
        }
        let prop = cls.prop(pname);
        match prop {
            None if !cls.setters.contains_key(pname.as_str()) => {
                return Err(Error::new(
                    "MATLAB:noPublicFieldForClass",
                    format!(
                        "Unrecognized property '{}' for class '{}'.",
                        pname, cls.name
                    ),
                ));
            }
            Some(p) if p.constant => {
                return Err(Error::new(
                    "MATLAB:class:SetProhibited",
                    format!(
                        "Cannot set the Constant property '{}' of class '{}'.",
                        pname, cls.name
                    ),
                ));
            }
            Some(p) if p.set_private && !inside => {
                return Err(Error::new(
                    "MATLAB:class:SetProhibited",
                    format!(
                        "Cannot set the read-only property '{}' of class '{}'.",
                        pname, cls.name
                    ),
                ));
            }
            _ => {}
        }
        let decl = prop.map(|p| p.decl.clone());
        let newval = if eops.len() == 1 {
            rhs
        } else {
            let cur = match self
                .object_field(&obj, pname, None, 1)?
                .0
                .into_iter()
                .next()
            {
                Some(v) => v,
                None => Value::empty(),
            };
            let mut slot = Some(cur);
            crate::assign::assign_path(&mut slot, &eops[1..], rhs)?;
            slot.unwrap_or_default()
        };
        let newval = match &decl {
            Some(d) if d.class.is_some() || d.size.is_some() || !d.validators.is_empty() => {
                crate::builtins::args::check_decl(self, d, newval, 1).map_err(|mut e| {
                    if let Some(rest) = e.message.strip_prefix("Invalid argument at position 1. ") {
                        e.message = format!(
                            "Error setting property '{}' of class '{}'. {}",
                            pname, cls.name, rest
                        );
                    }
                    e
                })?
            }
            _ => newval,
        };
        if let Some(s) = cls.setters.get(pname.as_str()).cloned() {
            let in_setter = self.frame().fname == format!("set.{}", pname);
            if !in_setter {
                let nout = s.func.outputs.len().min(1);
                let r = self.call_method_info(cls, &s, vec![obj.clone(), newval], nout)?;
                return Ok(match r.into_iter().next() {
                    Some(v) if !cls.is_handle => v,
                    _ => obj,
                });
            }
        }
        let mut obj = obj;
        set_prop(&mut obj, pname, newval);
        Ok(obj)
    }

    /// Overloaded binary operator, if any operand is an object.
    pub fn object_binary(&mut self, op: BinOp, a: &Value, b: &Value) -> R<Option<Value>> {
        if crate::builtins::control::is_lti(a) || crate::builtins::control::is_lti(b) {
            return crate::builtins::control::binary(self, op, a, b).map(Some);
        }
        let name = op_method(op);
        if let Some((cls, m)) = self.dispatch_method(name, &[a.clone(), b.clone()]) {
            let r = self.call_method_info(&cls, &m, vec![a.clone(), b.clone()], 1)?;
            return Ok(r.into_iter().next());
        }
        // Built-in comparisons for handles and enumerations.
        if matches!(op, BinOp::Eq | BinOp::Ne) {
            let eq = match (a, b) {
                (Value::HObject(x), Value::HObject(y)) => Some(Rc::ptr_eq(x, y)),
                _ if class_of(a).is_some_and(|c| !c.enums.is_empty()) && class_of(b).is_some() => {
                    Some(
                        get_prop(a, "__member__") == get_prop(b, "__member__")
                            && a.class_name() == b.class_name(),
                    )
                }
                _ => None,
            };
            if let Some(e) = eq {
                return Ok(Some(Value::bool(if op == BinOp::Eq { e } else { !e })));
            }
        }
        // Numeric enumerations behave like their values.
        let conv = |v: &Value| match get_prop(v, "__value__") {
            Some(x) if class_of(v).is_some() => Some(x),
            _ => None,
        };
        if let (Some(x), _) | (_, Some(x)) = (conv(a), conv(b)) {
            let _ = x;
            let aa = conv(a).unwrap_or_else(|| a.clone());
            let bb = conv(b).unwrap_or_else(|| b.clone());
            return Ok(Some(indy_value::ops::binary(op, &aa, &bb)?));
        }
        Err(Error::new(
            "MATLAB:UndefinedFunction",
            format!(
                "Operator '{}' is not supported for operands of type '{}' and '{}'.",
                op.text(),
                a.class_name(),
                b.class_name()
            ),
        ))
    }

    pub fn object_unary(&mut self, op: UnOp, a: &Value) -> R<Value> {
        if crate::builtins::control::is_lti(a) {
            return crate::builtins::control::unary(self, op, a);
        }
        let name = unop_method(op);
        if let Some((cls, m)) = self.dispatch_method(name, std::slice::from_ref(a)) {
            let r = self.call_method_info(&cls, &m, vec![a.clone()], 1)?;
            return r
                .into_iter()
                .next()
                .ok_or_else(|| Error::msg("operator method returned no value"));
        }
        Err(Error::new(
            "MATLAB:UndefinedFunction",
            format!(
                "Unary operator '{}' is not supported for type '{}'.",
                name,
                a.class_name()
            ),
        ))
    }

    /// `end` inside `obj(...)` when the class overloads it.
    pub fn object_end(&mut self, obj: &Value, k: usize, n: usize) -> R<Option<f64>> {
        let Some(cls) = class_of(obj) else {
            return Ok(None);
        };
        if self.inside_class(&cls) {
            return Ok(None);
        }
        let Some(m) = cls.method("end") else {
            return Ok(None);
        };
        let r = self.call_method_info(
            &cls,
            &m,
            vec![
                obj.clone(),
                Value::num(k as f64 + 1.0),
                Value::num(n as f64),
            ],
            1,
        )?;
        Ok(r.first().and_then(|v| v.scalar_f64()))
    }

    /// Display an object (statement without `;`).
    pub fn show_object(&mut self, name: &str, v: &Value) -> R<bool> {
        for m in ["display", "disp"] {
            if let Some((cls, mi)) = self.dispatch_method(m, std::slice::from_ref(v)) {
                if m == "disp" {
                    self.print(&format!("{} =\n\n", name));
                }
                self.call_method_info(&cls, &mi, vec![v.clone()], 0)?;
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// Public property names (for `properties`, `fieldnames`).
pub fn public_props(cls: &ClassInfo) -> Vec<String> {
    cls.props
        .iter()
        .filter(|p| !p.get_private)
        .map(|p| p.name.clone())
        .collect()
}

pub fn method_names(cls: &ClassInfo) -> Vec<String> {
    let mut v: Vec<String> = cls.methods.keys().cloned().collect();
    v.push(cls.name.rsplit('.').next().unwrap_or(&cls.name).to_string());
    v.sort();
    v
}

/// `Class.empty`, `Class.empty(0, n)`, `Class.empty([0 n])`.
fn empty_array(cls: &Rc<ClassInfo>, args: Vec<Value>) -> R<Value> {
    let mut dims = Vec::new();
    for a in &args {
        let v = a.to_f64_vec().ok_or_else(|| {
            Error::new(
                "MATLAB:class:emptyDims",
                "Dimensions of an empty array must be numeric.",
            )
        })?;
        for x in v {
            if x < 0.0 || x.fract() != 0.0 {
                return Err(Error::new(
                    "MATLAB:class:emptyDims",
                    "Dimensions of an empty array must be nonnegative integers.",
                ));
            }
            dims.push(x as usize);
        }
    }
    match dims.len() {
        0 => dims = vec![0, 0],
        1 => dims.push(dims[0]),
        _ => {}
    }
    if !dims.contains(&0) {
        return Err(Error::new(
            "MATLAB:class:emptyMustBeZero",
            "At least one dimension must be zero.",
        ));
    }
    let dims = indy_value::norm_dims(dims);
    Ok(Value::ObjArr(Box::new(indy_value::ObjArray {
        class: cls.name.clone(),
        arr: indy_value::Array::new(dims, vec![]),
        meta: Some(cls.clone() as Rc<dyn std::any::Any>),
    })))
}
