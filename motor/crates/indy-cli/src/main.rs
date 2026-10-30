//! `indy` command line: `indy run file.m [--path dir]...`.
//!
//! Exit status 0 on success; on an uncaught error prints
//! `error: <message>` to stderr and exits with 1. A panic inside the
//! engine is reported as an internal error (exit 2), never a crash dump.

use std::path::PathBuf;
use std::process::ExitCode;

use indy_interp::{Interp, catch_panic};

fn usage() -> ExitCode {
    eprintln!("usage: indy run <file.m> [--path <dir>]... [--cd <dir>]");
    eprintln!("       indy eval '<code>' [--path <dir>]...");
    ExitCode::from(64)
}

fn main() -> ExitCode {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("indy: internal engine error: {}", info);
    }));
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        return usage();
    }
    let mode = args[0].clone();
    let target = args[1].clone();
    let mut paths = Vec::new();
    let mut cwd = None;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--path" | "-p" if i + 1 < args.len() => {
                paths.push(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--cd" if i + 1 < args.len() => {
                cwd = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            _ => return usage(),
        }
    }
    let result = std::panic::catch_unwind(move || {
        let mut it = Interp::new();
        // Graphics build figures in memory; the CLI never shows them.
        indy_graphics_builtins::register(&mut it);
        if let Some(d) = cwd {
            it.set_cwd(d);
        }
        for p in paths {
            let p = p.canonicalize().unwrap_or(p);
            it.add_path(p, false);
        }
        let r = catch_panic(|| match mode.as_str() {
            "run" => {
                let p = PathBuf::from(&target);
                let p = if p.is_absolute() { p } else { it.cwd.join(p) };
                it.run_file(&p)
            }
            "eval" => it.run_text(&target).map(|_| ()),
            _ => Err(indy_interp::Error::new(
                "Indy:usage",
                format!("unknown mode '{}'", mode),
            )),
        });
        it.flush();
        r
    });
    match result {
        Ok(Ok(())) => ExitCode::SUCCESS,
        Ok(Err(e)) => {
            eprintln!("error: {}", e.message);
            ExitCode::from(1)
        }
        Err(_) => {
            eprintln!("error: internal engine error (panic)");
            ExitCode::from(2)
        }
    }
}
