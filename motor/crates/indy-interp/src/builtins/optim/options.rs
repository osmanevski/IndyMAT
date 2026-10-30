//! Option lookup shared by the solvers, and `optimoptions`.

use indy_value::{StructArray, Value};

use super::super::{arg_str, err};
use crate::{Interp, R};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Disp {
    Off,
    Notify,
    Final,
    Iter,
}

/// Read-only view of an options struct (`optimset` or `optimoptions`).
pub(super) struct Opts<'a> {
    s: Option<&'a StructArray>,
    fname: &'static str,
}

impl<'a> Opts<'a> {
    pub(super) fn new(v: Option<&'a Value>, fname: &'static str) -> R<Opts<'a>> {
        match v {
            None => Ok(Opts { s: None, fname }),
            Some(Value::Struct(s)) if s.elems.len() == 1 => Ok(Opts { s: Some(s), fname }),
            Some(v) if v.is_empty() && v.is_numeric() => Ok(Opts { s: None, fname }),
            Some(v) => Err(err(
                &format!("MATLAB:{}:invalidOptions", fname),
                format!(
                    "{}: OPTIONS must be a scalar structure created with optimset or optimoptions, got a value of class {}.",
                    fname,
                    v.class_name()
                ),
            )),
        }
    }

    /// First non-empty field among `names` (case-insensitive), with the
    /// name under which it was found.
    pub(super) fn get(&self, names: &[&str]) -> Option<(String, &'a Value)> {
        let s = self.s?;
        for n in names {
            if let Some(i) = s.fields.iter().position(|f| f.eq_ignore_ascii_case(n)) {
                let v = &s.elems[0][i];
                if !v.is_empty() {
                    return Some((s.fields[i].clone(), v));
                }
            }
        }
        None
    }

    /// Numeric scalar option; text values such as `'100*numberOfVariables'`
    /// (the documented defaults stored by `optimoptions`) mean "default".
    pub(super) fn f64(&self, names: &[&str], default: f64) -> R<f64> {
        let Some((name, v)) = self.get(names) else {
            return Ok(default);
        };
        if v.is_text() {
            return Ok(default);
        }
        match v.scalar_f64() {
            Some(x) if !x.is_nan() => Ok(x),
            _ => Err(err(
                &format!("MATLAB:{}:invalidOption", self.fname),
                format!(
                    "{}: option '{}' must be a real numeric scalar.",
                    self.fname, name
                ),
            )),
        }
    }

    /// Positive count option (`MaxIter`, ...); `Inf` is allowed.
    pub(super) fn count(&self, names: &[&str], default: f64) -> R<f64> {
        let x = self.f64(names, default)?;
        if x < 0.0 {
            let name = self.get(names).map(|p| p.0).unwrap_or_default();
            return Err(err(
                &format!("MATLAB:{}:invalidOption", self.fname),
                format!(
                    "{}: option '{}' must be a non-negative number.",
                    self.fname, name
                ),
            ));
        }
        Ok(x.floor())
    }

    /// Text option, lower-cased.
    pub(super) fn text(&self, names: &[&str]) -> R<Option<String>> {
        match self.get(names) {
            None => Ok(None),
            Some((name, v)) => match v.to_str() {
                Some(s) => Ok(Some(s.to_ascii_lowercase())),
                None => Err(err(
                    &format!("MATLAB:{}:invalidOption", self.fname),
                    format!(
                        "{}: option '{}' must be a character vector.",
                        self.fname, name
                    ),
                )),
            },
        }
    }

    /// On/off option given as `'on'`/`'off'` or a logical/numeric scalar.
    pub(super) fn flag(&self, names: &[&str]) -> R<bool> {
        let Some((name, v)) = self.get(names) else {
            return Ok(false);
        };
        if let Some(s) = v.to_str() {
            return match s.to_ascii_lowercase().as_str() {
                "on" | "true" => Ok(true),
                "off" | "false" => Ok(false),
                _ => Err(err(
                    &format!("MATLAB:{}:invalidOption", self.fname),
                    format!("{}: option '{}' must be 'on' or 'off'.", self.fname, name),
                )),
            };
        }
        match v.scalar_f64() {
            Some(x) => Ok(x != 0.0),
            None => Err(err(
                &format!("MATLAB:{}:invalidOption", self.fname),
                format!(
                    "{}: option '{}' must be 'on', 'off' or a logical scalar.",
                    self.fname, name
                ),
            )),
        }
    }

    /// Numeric vector option (TypicalX, FiniteDifferenceStepSize); `None`
    /// when unset or given as the documented default text.
    pub(super) fn vector(&self, names: &[&str]) -> R<Option<Vec<f64>>> {
        let Some((name, v)) = self.get(names) else {
            return Ok(None);
        };
        if v.is_text() {
            return Ok(None);
        }
        match v {
            Value::Num(n) if n.im.is_none() => Ok(Some(n.re.clone())),
            Value::Bool(_) => Ok(v.to_f64_vec()),
            _ => Err(err(
                &format!("MATLAB:{}:invalidOption", self.fname),
                format!(
                    "{}: option '{}' must be a real numeric array.",
                    self.fname, name
                ),
            )),
        }
    }

    pub(super) fn display(&self, default: Disp) -> R<Disp> {
        Ok(match self.text(&["Display"])?.as_deref() {
            None => default,
            Some("off" | "none") => Disp::Off,
            Some("notify" | "notify-detailed") => Disp::Notify,
            Some("final" | "final-detailed") => Disp::Final,
            Some("iter" | "iter-detailed") => Disp::Iter,
            Some(other) => {
                return Err(err(
                    &format!("MATLAB:{}:invalidDisplay", self.fname),
                    format!(
                        "{}: invalid value '{}' for option 'Display' (expected 'off', 'none', 'iter', 'final' or 'notify').",
                        self.fname, other
                    ),
                ));
            }
        })
    }

    /// Error for options that are recognised but not implemented, when they
    /// are set to anything other than their "off" value.
    pub(super) fn reject(&self, names: &[&str]) -> R<()> {
        let Some((name, v)) = self.get(names) else {
            return Ok(());
        };
        let off = match v.to_str() {
            Some(s) => s.eq_ignore_ascii_case("off") || s.eq_ignore_ascii_case("false"),
            None => matches!(v, Value::Num(_) | Value::Bool(_)) && v.scalar_f64() == Some(0.0),
        };
        if off {
            return Ok(());
        }
        Err(err(
            &format!("MATLAB:{}:unsupportedOption", self.fname),
            format!(
                "{}: option '{}' is not supported by this engine.",
                self.fname, name
            ),
        ))
    }

    /// True when the option is unset or equal to its documented default
    /// (so messages can say "default value").
    pub(super) fn is_default(&self, names: &[&str], default: f64) -> bool {
        match self.get(names) {
            None => true,
            Some((_, v)) => v.is_text() || v.scalar_f64() == Some(default),
        }
    }
}

// ---------------------------------------------------------------------------
// optimoptions
// ---------------------------------------------------------------------------

enum Kind {
    Text(&'static [&'static str]),
    Display,
    Num,
    Count,
    Logical,
    Vector,
    /// accepted only as empty / false (feature not implemented)
    Unsupported,
}

struct Spec {
    name: &'static str,
    kind: Kind,
    default: fn() -> Value,
}

fn t(s: &'static str) -> fn() -> Value {
    // function pointers cannot capture; use a small table instead
    match s {
        "final" => || Value::str("final"),
        "notify" => || Value::str("notify"),
        "forward" => || Value::str("forward"),
        "dogleg" => || Value::str("trust-region-dogleg"),
        "trr" => || Value::str("trust-region-reflective"),
        "qn" => || Value::str("quasi-newton"),
        "bfgs" => || Value::str("bfgs"),
        "100n" => || Value::str("100*numberOfVariables"),
        "sqrteps" => || Value::str("sqrt(eps)"),
        "typx" => || Value::str("ones(numberOfVariables,1)"),
        "nnlstol" => || Value::str("10*eps*norm(C,1)*length(C)"),
        _ => Value::empty,
    }
}

const DISPLAY_VALUES: &[&str] = &[
    "off",
    "none",
    "iter",
    "iter-detailed",
    "final",
    "final-detailed",
    "notify",
    "notify-detailed",
];

fn common_nl(algorithms: &'static [&'static str], alg_default: &'static str) -> Vec<Spec> {
    vec![
        Spec {
            name: "Algorithm",
            kind: Kind::Text(algorithms),
            default: t(alg_default),
        },
        Spec {
            name: "CheckGradients",
            kind: Kind::Unsupported,
            default: || Value::bool(false),
        },
        Spec {
            name: "Display",
            kind: Kind::Display,
            default: t("final"),
        },
        Spec {
            name: "FiniteDifferenceStepSize",
            kind: Kind::Vector,
            default: t("sqrteps"),
        },
        Spec {
            name: "FiniteDifferenceType",
            kind: Kind::Text(&["forward", "central"]),
            default: t("forward"),
        },
        Spec {
            name: "FunctionTolerance",
            kind: Kind::Num,
            default: || Value::num(1e-6),
        },
        Spec {
            name: "MaxFunctionEvaluations",
            kind: Kind::Count,
            default: t("100n"),
        },
        Spec {
            name: "MaxIterations",
            kind: Kind::Count,
            default: || Value::num(400.0),
        },
        Spec {
            name: "OptimalityTolerance",
            kind: Kind::Num,
            default: || Value::num(1e-6),
        },
        Spec {
            name: "OutputFcn",
            kind: Kind::Unsupported,
            default: Value::empty,
        },
        Spec {
            name: "PlotFcn",
            kind: Kind::Unsupported,
            default: Value::empty,
        },
        Spec {
            name: "SpecifyObjectiveGradient",
            kind: Kind::Logical,
            default: || Value::bool(false),
        },
        Spec {
            name: "StepTolerance",
            kind: Kind::Num,
            default: || Value::num(1e-6),
        },
        Spec {
            name: "TypicalX",
            kind: Kind::Vector,
            default: t("typx"),
        },
        Spec {
            name: "UseParallel",
            kind: Kind::Unsupported,
            default: || Value::bool(false),
        },
    ]
}

fn specs(solver: &str) -> Option<(&'static str, Vec<Spec>)> {
    Some(match solver {
        "fsolve" => {
            let mut v = common_nl(
                &["trust-region-dogleg", "trust-region", "levenberg-marquardt"],
                "dogleg",
            );
            v.push(Spec {
                name: "InitDamping",
                kind: Kind::Num,
                default: || Value::num(0.01),
            });
            ("fsolve", v)
        }
        "lsqnonlin" | "lsqcurvefit" => {
            let mut v = common_nl(&["trust-region-reflective", "levenberg-marquardt"], "trr");
            v.push(Spec {
                name: "InitDamping",
                kind: Kind::Num,
                default: || Value::num(0.01),
            });
            (
                if solver == "lsqnonlin" {
                    "lsqnonlin"
                } else {
                    "lsqcurvefit"
                },
                v,
            )
        }
        "fminunc" => {
            let mut v = common_nl(&["quasi-newton"], "qn");
            v.push(Spec {
                name: "HessUpdate",
                kind: Kind::Text(&["bfgs"]),
                default: t("bfgs"),
            });
            v.push(Spec {
                name: "ObjectiveLimit",
                kind: Kind::Num,
                default: || Value::num(-1e20),
            });
            ("fminunc", v)
        }
        "lsqnonneg" => (
            "lsqnonneg",
            vec![
                Spec {
                    name: "Display",
                    kind: Kind::Display,
                    default: t("notify"),
                },
                Spec {
                    name: "StepTolerance",
                    kind: Kind::Num,
                    default: t("nnlstol"),
                },
            ],
        ),
        _ => return None,
    })
}

/// Old (optimset) names accepted by `optimoptions`, mapped per solver.
fn legacy_name(old: &str, solver: &str) -> Option<&'static str> {
    Some(match old.to_ascii_lowercase().as_str() {
        "tolfun" => {
            if solver == "fminunc" {
                "OptimalityTolerance"
            } else {
                "FunctionTolerance"
            }
        }
        "tolx" => "StepTolerance",
        "maxiter" => "MaxIterations",
        "maxfunevals" => "MaxFunctionEvaluations",
        "jacobian" | "gradobj" => "SpecifyObjectiveGradient",
        "derivativecheck" => "CheckGradients",
        "findifftype" => "FiniteDifferenceType",
        "findiffrelstep" => "FiniteDifferenceStepSize",
        "plotfcns" => "PlotFcn",
        _ => return None,
    })
}

fn check_value(spec: &Spec, v: &Value, solver: &str) -> R<Value> {
    let bad = |what: &str| {
        err(
            "optim:optimoptions:InvalidValue",
            format!(
                "optimoptions: invalid value for option '{}' of {}: {}.",
                spec.name,
                solver.to_ascii_uppercase(),
                what
            ),
        )
    };
    Ok(match &spec.kind {
        Kind::Text(_) | Kind::Display => {
            let list: &[&str] = match &spec.kind {
                Kind::Text(a) => a,
                _ => DISPLAY_VALUES,
            };
            let Some(s) = v.to_str() else {
                return Err(bad("expected a character vector"));
            };
            match list.iter().find(|a| a.eq_ignore_ascii_case(&s)) {
                Some(a) => Value::str(a),
                None => return Err(bad(&format!("expected one of {}", list.join(", ")))),
            }
        }
        Kind::Num => match v.scalar_f64() {
            Some(x) if !x.is_nan() && !v.is_text() => Value::num(x),
            _ => return Err(bad("expected a real scalar")),
        },
        Kind::Count => match v.scalar_f64() {
            Some(x) if x >= 0.0 && !v.is_text() => Value::num(x),
            _ => return Err(bad("expected a non-negative scalar")),
        },
        Kind::Logical => {
            let b = match v.to_str() {
                Some(s) if s.eq_ignore_ascii_case("on") || s.eq_ignore_ascii_case("true") => true,
                Some(s) if s.eq_ignore_ascii_case("off") || s.eq_ignore_ascii_case("false") => {
                    false
                }
                Some(_) => return Err(bad("expected true or false")),
                None => match v.scalar_f64() {
                    Some(x) => x != 0.0,
                    None => return Err(bad("expected true or false")),
                },
            };
            Value::bool(b)
        }
        Kind::Vector => match v {
            Value::Num(n) if n.im.is_none() && !n.re.is_empty() => v.clone(),
            _ => return Err(bad("expected a real numeric array")),
        },
        Kind::Unsupported => {
            let off = v.is_empty()
                || v.to_str().is_some_and(|s| {
                    s.eq_ignore_ascii_case("off") || s.eq_ignore_ascii_case("false")
                })
                || (matches!(v, Value::Num(_) | Value::Bool(_)) && v.scalar_f64() == Some(0.0));
            if !off {
                return Err(err(
                    "optim:optimoptions:UnsupportedOption",
                    format!(
                        "optimoptions: option '{}' is not supported by this engine.",
                        spec.name
                    ),
                ));
            }
            v.clone()
        }
    })
}

fn solver_name(v: &Value) -> Option<String> {
    match v {
        Value::Func(h) if h.anon.is_none() => Some(h.name.clone()),
        _ => v.to_str(),
    }
}

/// `optimoptions(solver, Name, Value, ...)`, `optimoptions(oldopts, ...)`,
/// `optimoptions(solver, oldopts, ...)`. Returns a struct, not an object.
pub(super) fn optimoptions(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let Some(first) = a.first() else {
        return Err(err(
            "MATLAB:minrhs",
            "Not enough input arguments (function 'optimoptions').",
        ));
    };
    let (solver_in, base): (String, Option<&StructArray>) = match first {
        Value::Struct(s) if s.elems.len() == 1 => {
            match s.get(0, "SolverName").and_then(|v| v.to_str()) {
                Some(n) => (n, Some(s)),
                None => {
                    return Err(err(
                        "optim:optimoptions:InvalidSolver",
                        "optimoptions: the first argument must be a solver name, a function handle to a solver, or options created by optimoptions.",
                    ));
                }
            }
        }
        v => match solver_name(v) {
            Some(n) => (n, None),
            None => {
                return Err(err(
                    "optim:optimoptions:InvalidSolver",
                    "optimoptions: the first argument must be a solver name, a function handle to a solver, or options created by optimoptions.",
                ));
            }
        },
    };
    let lower = solver_in.to_ascii_lowercase();
    let Some((solver, specs)) = specs(&lower) else {
        return Err(err(
            "optim:optimoptions:InvalidSolver",
            format!(
                "optimoptions: solver '{}' is not supported by this engine (supported: fsolve, fminunc, lsqnonlin, lsqcurvefit, lsqnonneg; fminsearch, fzero and fminbnd use optimset).",
                solver_in
            ),
        ));
    };
    let mut s = StructArray::new_scalar();
    s.set(0, "SolverName", Value::str(solver));
    for sp in &specs {
        s.set(0, sp.name, (sp.default)());
    }
    let find = |name: &str| -> Option<usize> {
        if let Some(i) = specs
            .iter()
            .position(|sp| sp.name.eq_ignore_ascii_case(name))
        {
            return Some(i);
        }
        let new = legacy_name(name, solver)?;
        specs.iter().position(|sp| sp.name == new)
    };
    if let Some(b) = base {
        for (fi, f) in b.fields.iter().enumerate() {
            if let Some(i) = find(f) {
                s.set(0, specs[i].name, b.elems[0][fi].clone());
            }
        }
    }
    let mut i = 1;
    // optimoptions(solver, oldopts, ...): copy the matching options
    if base.is_none()
        && let Some(Value::Struct(old)) = a.get(1)
    {
        if let Some(e) = old.elems.first() {
            for (fi, f) in old.fields.iter().enumerate() {
                if let Some(k) = find(f)
                    && !e[fi].is_empty()
                {
                    s.set(0, specs[k].name, check_value(&specs[k], &e[fi], solver)?);
                }
            }
        }
        i = 2;
    }
    while i < a.len() {
        let name = arg_str(&a[i], "optimoptions")?;
        let Some(v) = a.get(i + 1) else {
            return Err(err(
                "optim:optimoptions:PairsRequired",
                "optimoptions: arguments must occur in name-value pairs.",
            ));
        };
        let Some(k) = find(&name) else {
            return Err(err(
                "optim:optimoptions:InvalidOption",
                format!(
                    "optimoptions: '{}' is not an option for {} (or is not supported by this engine).",
                    name,
                    solver.to_ascii_uppercase()
                ),
            ));
        };
        s.set(0, specs[k].name, check_value(&specs[k], v, solver)?);
        i += 2;
    }
    Ok(vec![Value::Struct(s)])
}
