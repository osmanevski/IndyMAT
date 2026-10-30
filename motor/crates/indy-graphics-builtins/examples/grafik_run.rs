//! Runs a MATLAB script headless through the interpreter and these
//! builtins (like `indy run`) and writes every open figure's Figure-Data
//! JSON to `<out_dir>/<script>-<n>.json`, for the UI gate
//! (`motor/scripts/grafik_validate.cjs`).
//!
//!   cargo run -p indy-graphics-builtins --example grafik_run -- <file.m> <out_dir>

use std::path::PathBuf;
use std::process::ExitCode;

use indy_interp::Interp;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [file, out] = args.as_slice() else {
        eprintln!("usage: grafik_run <file.m> <out_dir>");
        return ExitCode::from(2);
    };
    let file = PathBuf::from(file);
    let file = file.canonicalize().unwrap_or(file);
    let mut it = Interp::new();
    indy_graphics_builtins::register(&mut it);
    if let Some(dir) = file.parent() {
        it.set_cwd(dir.to_path_buf());
    }
    let r = it.run_file(&file);
    it.flush();
    let stem = file
        .file_stem()
        .map_or("figure".into(), |s| s.to_string_lossy().to_string());
    std::fs::create_dir_all(out).expect("output directory");
    let figures = indy_graphics_builtins::open_figures(&mut it);
    for f in &figures {
        let path = PathBuf::from(out).join(format!("{stem}-{}.json", f.number));
        std::fs::write(&path, &f.json).expect("write figure");
        println!("figure {} \"{}\" -> {}", f.number, f.name, path.display());
    }
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {}", e.message);
            ExitCode::from(1)
        }
    }
}
