//! MATLAB graphics builtins for the Rust engine: thin functions over the
//! `indy-graphics` object model, registered into an [`Interp`].
//!
//! Shared by the `indy-kernel` engine (which exports the figures after each
//! eval) and the `indy` CLI (headless: figures are built, never shown). It
//! depends on `indy-interp` and `indy-graphics` only, so both binaries can
//! use it without a package cycle.
//!
//! The session's [`Graphics`] lives in the interpreter's extension slot
//! ([`Interp::ext_mut`]): a new interpreter (the kernel's `reset`) starts
//! with no figures. Handles reach MATLAB code as doubles; dot access on
//! them (`h.Color`) goes through the interpreter's handle-field hooks,
//! which [`register`] installs (see `handles.rs`).

#![forbid(unsafe_code)]

mod builtins;
mod charts;
pub mod convert;
mod export;
mod handles;
mod lane_a;
mod lane_b;

use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, OnceLock};

use indy_graphics::{Graphics, Handle, PropValue};
use indy_interp::builtins::err;
use indy_interp::{BuiltinFn, Interp, R, Value};

/// The kernel shows at most this many figures (protocol `figure` events).
pub const MAX_FIGURES: usize = 12;

pub const GRAPHICS_UNAVAILABLE: &str = "graphics not yet available in the Rust engine";

/// Per-interpreter graphics state.
#[derive(Default)]
pub struct GraphicsState {
    pub graphics: Graphics,
    /// Latest Figure-Data JSON of every open figure, kept current from
    /// [`Graphics::take_changed`].
    exported: BTreeMap<Handle, String>,
    /// Latest PNG of every open figure, re-rendered only when it changed.
    pngs: BTreeMap<Handle, Arc<Vec<u8>>>,
}

/// The session's graphics model (created on first use).
pub fn graphics(it: &mut Interp) -> &mut Graphics {
    &mut it.ext_mut::<GraphicsState>().graphics
}

/// One open figure, ready for a protocol `figure` event.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenFigure {
    pub number: u64,
    pub name: String,
    /// Figure-Data JSON (`source` is the extractor default
    /// `{"job":"","figure":0,...}`; the exporter sets the job identity).
    pub json: String,
    /// The figure drawn by indy-raster, as the kernel stores it next to the
    /// JSON. `None` only if the renderer failed.
    pub png: Option<Arc<Vec<u8>>>,
}

/// Every open figure in number order (at most [`MAX_FIGURES`]). Only
/// figures that changed since the previous call are re-serialised
/// (`take_changed`); closed figures are dropped.
pub fn open_figures(it: &mut Interp) -> Vec<OpenFigure> {
    if it.ext::<GraphicsState>().is_none() {
        return Vec::new();
    }
    let st = it.ext_mut::<GraphicsState>();
    let mut changed = Vec::new();
    for (h, json) in st.graphics.take_changed() {
        match json {
            Some(json) => {
                st.exported.insert(h, json);
                changed.push(h);
            }
            None => {
                st.exported.remove(&h);
                st.pngs.remove(&h);
            }
        }
    }
    render_pngs(st, &changed);
    st.exported
        .iter()
        .take(MAX_FIGURES)
        .map(|(h, json)| {
            let name = match st.graphics.get(*h, "Name") {
                Ok(PropValue::Str(s)) if !s.is_empty() => s,
                _ => format!("Figure {}", h.0),
            };
            OpenFigure {
                number: h.0.max(0) as u64,
                name,
                json: json.clone(),
                png: st.pngs.get(h).cloned(),
            }
        })
        .collect()
}

/// Renders the changed figures that will be reported, one thread each. A
/// renderer failure (a bug) loses that figure's PNG, never the session.
fn render_pngs(st: &mut GraphicsState, changed: &[Handle]) {
    // A figure further down the list comes into view when another one closes.
    let needed: Vec<Handle> = st
        .exported
        .keys()
        .take(MAX_FIGURES)
        .filter(|h| changed.contains(h) || !st.pngs.contains_key(h))
        .copied()
        .collect();
    let jobs: Vec<(Handle, indy_graphics::json::Json, [f64; 3])> = needed
        .iter()
        .map(|h| {
            (
                *h,
                st.graphics.raster_record(*h),
                export::figure_color(&st.graphics, *h),
            )
        })
        .collect();
    let results: Vec<(Handle, Option<Vec<u8>>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = jobs
            .iter()
            .map(|(h, record, background)| {
                let h = *h;
                scope.spawn(move || (h, export::figure_png(record, *background)))
            })
            .collect();
        handles
            .into_iter()
            .zip(&jobs)
            .map(|(t, job)| match t.join() {
                Ok((h, png)) => (h, Some(png)),
                Err(_) => (job.0, None),
            })
            .collect()
    });
    for (h, png) in results {
        match png {
            Some(png) => {
                st.pngs.insert(h, Arc::new(png));
            }
            None => {
                st.pngs.remove(&h);
            }
        }
    }
}

/// Graphics names the model does not cover yet: each fails with an
/// explicit error instead of silently doing nothing.
pub const UNAVAILABLE: &[&str] = &[
    "shg",
    "refresh",
    "ezplot",
    "annotation",
    "light",
    "lighting",
    "camlight",
    "rotate3d",
    "zoom",
    "pan",
    "ginput",
    "uicontrol",
    "uifigure",
    "waitbar",
    "linkaxes",
];

macro_rules! unavailable_builtins {
    ($($name:literal),* $(,)?) => {
        &[$(($name, {
            fn f(_: &mut Interp, _: Vec<Value>, _: usize) -> R<Vec<Value>> {
                Err(unavailable($name))
            }
            f as BuiltinFn
        })),*]
    };
}

fn unavailable(name: &str) -> Box<indy_interp::Error> {
    err(
        "IndyR:graphicsUnavailable",
        format!("{name}: {GRAPHICS_UNAVAILABLE}"),
    )
}

fn unavailable_table() -> &'static [(&'static str, BuiltinFn)] {
    unavailable_builtins!(
        "shg",
        "refresh",
        "ezplot",
        "annotation",
        "light",
        "lighting",
        "camlight",
        "rotate3d",
        "zoom",
        "pan",
        "ginput",
        "uicontrol",
        "uifigure",
        "waitbar",
        "linkaxes",
    )
}

/// Registers the graphics builtins: the covered names over the model, the
/// uncovered ones as explicit errors. Names the interpreter already defines
/// are kept, except `delete`, which is wrapped: graphics handles go to the
/// model, everything else to the interpreter's own `delete`.
pub fn register(it: &mut Interp) {
    builtins::register(it);
    let mut stubs = Vec::new();
    for &(name, f) in unavailable_table() {
        if !it.builtins.contains_key(name) {
            it.builtins.insert(name, f);
            stubs.push(name);
        }
    }
    it.ext_mut::<Stubs>().0.extend(stubs);
    for (name, plain, wrapped) in [
        ("exist", &PLAIN_EXIST, exist as BuiltinFn),
        ("which", &PLAIN_WHICH, which as BuiltinFn),
    ] {
        if let Some(&f) = it.builtins.get(name)
            && f as usize != wrapped as usize
        {
            let _ = plain.set(f);
            it.builtins.insert(name, wrapped);
        }
    }
}

/// Names registered as "not yet available" stubs in this interpreter.
#[derive(Default)]
struct Stubs(HashSet<&'static str>);

static PLAIN_EXIST: OnceLock<BuiltinFn> = OnceLock::new();
static PLAIN_WHICH: OnceLock<BuiltinFn> = OnceLock::new();

fn is_stub(it: &Interp, a: &[Value]) -> bool {
    let Some(name) = a.first().and_then(Value::to_str) else {
        return false;
    };
    it.ext::<Stubs>()
        .is_some_and(|s| s.0.contains(name.as_str()))
}

/// `exist`: a graphics name that only has the "not yet available" stub is
/// not a usable builtin, so `exist(name)` / `exist(name,'builtin')` give 0
/// (scripts that check before calling take their fallback branch). A user
/// file of that name still counts (2).
fn exist(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let stub = is_stub(it, &a);
    let Some(plain) = PLAIN_EXIST.get() else {
        return Err(err("MATLAB:UndefinedFunction", "exist is not available"));
    };
    let out = plain(it, a, nargout)?;
    if stub && out.first().and_then(Value::to_f64_vec).as_deref() == Some(&[5.0]) {
        return Ok(vec![Value::num(0.0)]);
    }
    Ok(out)
}

/// `which`: a stub-only graphics name is not found (`''`), as for any
/// unknown name.
fn which(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let stub = is_stub(it, &a);
    let Some(plain) = PLAIN_WHICH.get() else {
        return Err(err("MATLAB:UndefinedFunction", "which is not available"));
    };
    let out = plain(it, a, nargout)?;
    let builtin = out
        .first()
        .and_then(Value::to_str)
        .is_some_and(|s| s.starts_with("built-in ("));
    if stub && builtin {
        return Ok(vec![Value::str("")]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_list_matches_table() {
        let names: Vec<&str> = unavailable_table().iter().map(|(n, _)| *n).collect();
        assert_eq!(names, UNAVAILABLE);
    }
}
