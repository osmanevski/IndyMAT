//! `pkg`: Octave's package command. Packages whose functions are built
//! into the engine (see [`native_package`]) load without anything to
//! install; every other package reports an explicit error.

use std::collections::BTreeSet;

use indy_value::{Array, StructArray, Value};

use super::{err, reg};
use crate::{Interp, R};

/// Engine-native packages: name, version reported, description.
/// One line per package.
const NATIVE: &[(&str, &str, &str)] = &[
    (
        "control",
        "builtin",
        "Control System Toolbox functions (tf, zpk, ss, pid, step, bode, ...) built into the Rust engine",
    ),
    (
        "signal",
        "builtin",
        "Signal processing functions built into the Rust engine",
    ),
    (
        "datatypes",
        "builtin",
        "table, datetime, duration, categorical and string types built into the Rust engine",
    ),
    (
        "statistics",
        "builtin",
        "Statistics and Machine Learning Toolbox functions (distributions, tests, clustering, regression) built into the Rust engine",
    ),
];

/// Whether `name` is a package built into the engine.
pub fn native_package(name: &str) -> bool {
    NATIVE.iter().any(|(n, _, _)| *n == name)
}

#[derive(Default)]
struct Loaded(BTreeSet<String>);

pub fn register(it: &mut Interp) {
    reg(it, "pkg", pkg);
}

fn unsupported(name: &str) -> Box<indy_value::Error> {
    err(
        "Octave:pkg:notSupported",
        format!(
            "package {} is not available in the Rust engine: Octave packages are not supported by the engine (built in: {}).",
            name,
            NATIVE.iter().map(|p| p.0).collect::<Vec<_>>().join(", ")
        ),
    )
}

fn pkg(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let words: Vec<String> = a.iter().filter_map(|v| v.to_str()).collect();
    let Some(cmd) = words.first() else {
        return Err(err("Octave:invalid-fun-call", "pkg: missing command."));
    };
    let names: Vec<&String> = words[1..].iter().filter(|w| !w.starts_with('-')).collect();
    match cmd.as_str() {
        "load" | "unload" => {
            if names.is_empty() {
                return Err(err(
                    "Octave:invalid-fun-call",
                    format!("pkg {}: package name required.", cmd),
                ));
            }
            for n in &names {
                if n.as_str() == "all" {
                    continue;
                }
                if !native_package(n) {
                    return Err(unsupported(n));
                }
            }
            let st = &mut it.ext_mut::<Loaded>().0;
            for n in names {
                let all: Vec<String> = if n.as_str() == "all" {
                    NATIVE.iter().map(|p| p.0.to_string()).collect()
                } else {
                    vec![n.clone()]
                };
                for x in all {
                    if cmd == "load" {
                        st.insert(x);
                    } else {
                        st.remove(&x);
                    }
                }
            }
            Ok(vec![])
        }
        "list" => {
            let loaded = it.ext::<Loaded>().map(|l| l.0.clone()).unwrap_or_default();
            if nargout > 0 {
                let items: Vec<Value> = NATIVE
                    .iter()
                    .map(|(n, v, d)| {
                        let mut s = StructArray::new_scalar();
                        s.set(0, "name", Value::str(n));
                        s.set(0, "version", Value::str(v));
                        s.set(0, "description", Value::str(d));
                        s.set(0, "loaded", Value::bool(loaded.contains(*n)));
                        s.set(0, "dir", Value::str("(built into the Rust engine)"));
                        Value::Struct(s)
                    })
                    .collect();
                return Ok(vec![Value::Cell(Array::new(vec![1, items.len()], items))]);
            }
            let mut t = String::from(
                "Package Name  | Version | Installation directory\n--------------+---------+-----------------------\n",
            );
            for (n, v, _) in NATIVE {
                let mark = if loaded.contains(*n) { "*" } else { " " };
                t.push_str(&format!(
                    "{:>13}{}| {:>7} | (built into the Rust engine)\n",
                    n, mark, v
                ));
            }
            it.print(&t);
            Ok(vec![])
        }
        "describe" => {
            let loaded = it.ext::<Loaded>().map(|l| l.0.clone()).unwrap_or_default();
            let mut t = String::new();
            for n in &names {
                let Some((name, _, desc)) = NATIVE.iter().find(|p| p.0 == n.as_str()) else {
                    return Err(unsupported(n));
                };
                t.push_str(&format!(
                    "---\nPackage name:\n\t{}\nShort description:\n\t{}\nStatus:\n\t{}\n",
                    name,
                    desc,
                    if loaded.contains(*name) {
                        "Loaded"
                    } else {
                        "Not loaded"
                    }
                ));
            }
            it.print(&t);
            Ok(vec![])
        }
        _ => Err(err(
            "Octave:pkg:notSupported",
            format!(
                "pkg {}: Octave packages are not supported by the Rust engine (built-in packages: {}).",
                words.join(" "),
                NATIVE.iter().map(|p| p.0).collect::<Vec<_>>().join(", ")
            ),
        )),
    }
}
