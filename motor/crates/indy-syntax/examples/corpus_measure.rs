//! Parse-rate measurement over a corpus directory (read-only, outside the
//! repo). Usage:
//!
//! ```text
//! cargo run --release --example corpus_measure -- <corpus-root> [--all]
//! ```
//!
//! Prints `FOLDER name ok/total`, the top failing diagnostic codes with one
//! example `file:line:col`, and `TOTAL ok/total`. With `--all` every failure
//! is listed as `FAIL path:line:col code message`; with `--commands` every
//! command-syntax statement is listed as `CMD path (command ...)`. The
//! dialect is Octave unless `--matlab` is given.

use indy_syntax::{Dialect, ast, parse_file};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect(&path, out);
            } else if path.extension().is_some_and(|x| x == "m") {
                out.push(path);
            }
        }
    }
}

fn main() {
    let Some(root) = env::args().nth(1).map(PathBuf::from) else {
        eprintln!("usage: corpus_measure <corpus-root> [--all] [--commands] [--matlab]");
        std::process::exit(2);
    };
    let all = env::args().any(|a| a == "--all");
    let commands = env::args().any(|a| a == "--commands");
    let dialect = if env::args().any(|a| a == "--matlab") {
        Dialect::Matlab
    } else {
        Dialect::Octave
    };
    let mut files = vec![];
    collect(&root, &mut files);
    files.sort();
    let mut folders: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut errors: BTreeMap<&'static str, (usize, String)> = BTreeMap::new();
    for path in &files {
        let rel = path.strip_prefix(&root).unwrap_or(path);
        let folder = rel
            .components()
            .next()
            .map(|x| x.as_os_str().to_string_lossy().into_owned())
            .unwrap_or_default();
        let count = folders.entry(folder).or_default();
        count.1 += 1;
        let bytes = fs::read(path).unwrap_or_default();
        let src = String::from_utf8_lossy(&bytes);
        let result = parse_file(&src, dialect);
        if result.ok() {
            count.0 += 1;
            if let (true, Some(file)) = (commands, &result.file) {
                let sx = ast::sexpr(file);
                for (i, _) in sx.match_indices("(command ") {
                    let rest = &sx[i..];
                    let end = rest.find(')').map_or(rest.len(), |e| e + 1);
                    println!("CMD {} {}", rel.display(), &rest[..end]);
                }
            }
        } else if let Some(d) = result.diagnostics.first() {
            let at = format!("{}:{}:{}", rel.display(), d.line, d.column);
            if all {
                println!("FAIL {at} {} {}", d.code, d.message);
            }
            errors.entry(d.code).or_insert((0, at)).0 += 1;
        }
    }
    let success: usize = folders.values().map(|x| x.0).sum();
    for (name, (ok, total)) in &folders {
        println!("FOLDER {name} {ok}/{total}");
    }
    let mut ranked: Vec<_> = errors.into_iter().collect();
    ranked.sort_by_key(|(_, (count, _))| std::cmp::Reverse(*count));
    for (code, (count, at)) in ranked.into_iter().take(15) {
        println!("ERROR {count} {code} {at}");
    }
    println!("TOTAL {success}/{}", files.len());
}
