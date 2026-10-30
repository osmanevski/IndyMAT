//! `indy-kernel` binary: the kernel protocol over stdio with the real
//! interpreter (`InterpEngine`). `--stub`, or `INDYMAT_R_STUB=1` in the
//! environment, selects `StubEngine` instead (tests).
//!
//! SIGINT interrupts the running eval (see `docs/Kernel-Protokolu.md`); it
//! never ends the process. EOF on stdin or a `close` request does.

use indy_kernel::{Engine, InterpEngine, StubEngine, server};
use std::io::{self, BufReader};
use std::os::raw::c_int;
use std::sync::atomic::{AtomicBool, Ordering};

static INTERRUPT: AtomicBool = AtomicBool::new(false);

const SIGINT: c_int = 2;

extern "C" fn on_sigint(_signal: c_int) {
    // Only an atomic store: async-signal-safe.
    INTERRUPT.store(true, Ordering::SeqCst);
}

unsafe extern "C" {
    // POSIX `signal`; on macOS and glibc it installs a persistent BSD-style
    // handler with SA_RESTART. The previous handler is not needed.
    fn signal(signum: c_int, handler: extern "C" fn(c_int)) -> usize;
}

fn main() {
    // SAFETY: installs a handler that performs one atomic store.
    unsafe {
        signal(SIGINT, on_sigint);
    }
    let start = std::env::current_dir()
        .and_then(|p| p.canonicalize())
        .unwrap_or_else(|_| "/".into());
    let stub = std::env::args().skip(1).any(|a| a == "--stub")
        || std::env::var("INDYMAT_R_STUB").is_ok_and(|v| v == "1");
    let mut engine: Box<dyn Engine> = if stub {
        Box::new(StubEngine::new(start))
    } else {
        // An engine bug must end one eval, not print a panic dump.
        std::panic::set_hook(Box::new(|info| {
            eprintln!("indy-kernel: internal engine error: {info}");
        }));
        Box::new(InterpEngine::new(start))
    };
    let input = BufReader::new(io::stdin());
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if let Err(error) = server::serve(engine.as_mut(), input, &mut out, &INTERRUPT) {
        // stdout is gone: the client closed the pipe. Nothing more to say there.
        eprintln!("indy-kernel: {error}");
    }
}
