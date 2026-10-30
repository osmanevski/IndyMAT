//! Graphics object model for the Rust engine.
//!
//! MATLAB-style figures, axes and plot objects in plain Rust, emitted as the
//! Figure-Data v3 JSON that IndyMAT's browser viewer renders — so plots work
//! without Octave. The API takes plain Rust types (`f64` slices, [`Array`],
//! strings, [`PropValue`]) and is independent of the interpreter's value
//! type; see `motor/docs/Grafik.md` for the interpreter hook.
//!
//! ```
//! use indy_graphics::{Graphics, PlotSeries};
//! let mut g = Graphics::new();
//! g.plot(None, &[PlotSeries::xy(&[1.0, 2.0, 4.0], &[3.0, 5.0, 4.0]).with_spec("-o")], &[]).unwrap();
//! let fig = g.gcf();
//! assert!(g.figure_json(fig).starts_with("{\"version\":2"));
//! ```

#![forbid(unsafe_code)]

pub mod annot;
pub mod callargs;
pub mod color;
pub mod contour;
pub mod emit;
pub mod error;
mod figtext;
pub mod graphics;
pub mod hist;
mod inset;
pub mod json;
pub mod layout;
pub mod lines2;
pub mod linespec;
pub mod model;
pub mod pie;
pub mod plots;
pub mod props;
mod scene;
pub mod surf2;
mod turbo;
pub mod value;
mod viridis;
pub mod yy;

pub use annot::{AxisCmd, ColormapArg};
pub use error::{GraphicsError, Result};
pub use graphics::{CloseTarget, Graphics, Opts, opt};
pub use hist::{HistBins, HistCounts, hist_counts};
pub use model::{AppData, Handle, LabelRole};
pub use plots::{BarArgs, GridArgs, Levels, PlotSeries, ScatterArgs};
pub use surf2::{Bar3Args, Bar3Style, ImshowArgs, ImshowKind, Shading};
pub use value::{Array, NumClass, PropValue};
