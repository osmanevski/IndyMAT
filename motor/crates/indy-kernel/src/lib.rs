//! `indy-kernel`: the IndyMAT Rust kernel process.
//!
//! The protocol is frozen in `motor/docs/Kernel-Protokolu.md`. An [`Engine`]
//! runs code; [`server::serve`] speaks the JSON-lines protocol over stdio for
//! any engine. [`InterpEngine`] is the real interpreter (`indy-interp`);
//! [`StubEngine`] is a tiny honest placeholder kept for tests (`--stub`).

mod debugger;
pub mod engine;
pub mod interp_engine;
pub mod json;
mod mat_ops;
pub mod server;
mod session_builtins;
pub mod stub;
mod variables;

pub use engine::{
    Engine, EngineError, EngineInfo, ErrorCode, EvalError, EvalMode, EvalRequest, FigureEvent,
    Host, Interrupted, PageRequest, PauseRequest, SourceLocation, Stream, VariableSummary,
};
pub use indy_graphics_builtins::GRAPHICS_UNAVAILABLE;
pub use interp_engine::InterpEngine;
pub use json::Json;
pub use stub::StubEngine;
