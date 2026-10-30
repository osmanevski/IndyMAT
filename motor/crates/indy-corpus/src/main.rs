use indy_syntax::{Diagnostic, Dialect, ParseResult};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Clone)]
struct Entry {
    path: String,
    ok: bool,
    panic: bool,
    diagnostic: Option<Diagnostic>,
    micros: u128,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0].starts_with('-') {
        usage();
        std::process::exit(2);
    }
    let root = PathBuf::from(&args[0]);
    let mut dialect = Dialect::Octave;
    let mut json = None;
    let mut fail_list = None;
    let mut baseline = None;
    let mut i = 1;
    while i < args.len() {
        let value = args.get(i + 1).cloned();
        if value.is_none() {
            eprintln!("missing value for {}", args[i]);
            std::process::exit(2);
        }
        match args[i].as_str() {
            "--dialect" => {
                dialect = match value.as_deref() {
                    Some("octave") => Dialect::Octave,
                    Some("matlab") => Dialect::Matlab,
                    _ => {
                        eprintln!("--dialect expects octave or matlab");
                        std::process::exit(2);
                    }
                }
            }
            "--json" => json = value.map(PathBuf::from),
            "--fail-list" => fail_list = value.map(PathBuf::from),
            "--baseline" => baseline = value.map(PathBuf::from),
            _ => {
                eprintln!("unknown option: {}", args[i]);
                usage();
                std::process::exit(2);
            }
        }
        i += 2;
    }
    if !root.is_dir() {
        eprintln!("not a directory: {}", root.display());
        std::process::exit(2);
    }
    let (entries, lossy) = scan(&root, dialect);
    print_report(&root, &entries, lossy);
    if let Some(path) = json
        && let Err(e) = write_json(&path, &entries)
    {
        eprintln!("{}: {e}", path.display());
        std::process::exit(2);
    }
    if let Some(path) = fail_list {
        let body = entries
            .iter()
            .filter(|e| !e.ok)
            .map(|e| e.path.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if let Err(e) = fs::write(
            &path,
            format!("{body}{}", if body.is_empty() { "" } else { "\n" }),
        ) {
            eprintln!("{}: {e}", path.display());
            std::process::exit(2);
        }
    }
    let regression = if let Some(path) = baseline {
        match fs::read_to_string(&path).and_then(|s| {
            parse_baseline(&s).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        }) {
            Ok(old) => print_baseline(&old, &entries),
            Err(e) => {
                eprintln!("cannot read baseline {}: {e}", path.display());
                std::process::exit(2);
            }
        }
    } else {
        false
    };
    if regression {
        std::process::exit(1);
    }
}

fn usage() {
    eprintln!(
        "usage: indy-corpus <corpus_dir> [--dialect octave|matlab] [--json out.json] [--fail-list out.txt] [--baseline old.json]"
    );
}

fn files(root: &Path) -> Vec<PathBuf> {
    fn visit(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        for item in rd.flatten() {
            let p = item.path();
            if p.is_dir() {
                visit(&p, out);
            } else if p.extension().is_some_and(|x| x == "m") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    visit(root, &mut out);
    out.sort();
    out
}

fn scan(root: &Path, dialect: Dialect) -> (Vec<Entry>, usize) {
    scan_with(root, |src| indy_syntax::parse_file(src, dialect))
}

fn scan_with<F>(root: &Path, parse: F) -> (Vec<Entry>, usize)
where
    F: Fn(&str) -> ParseResult,
{
    let mut lossy = 0;
    let entries = files(root)
        .into_iter()
        .map(|path| {
            let display = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let bytes = fs::read(&path).unwrap_or_default();
            let src = match String::from_utf8(bytes.clone()) {
                Ok(s) => s,
                Err(_) => {
                    lossy += 1;
                    bytes.iter().map(|b| char::from(*b)).collect()
                }
            };
            let start = Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| parse(&src)));
            let micros = start.elapsed().as_micros();
            match result {
                Ok(result) => Entry {
                    path: display,
                    ok: result.ok(),
                    panic: false,
                    diagnostic: result.diagnostics.into_iter().next(),
                    micros,
                },
                Err(_) => Entry {
                    path: display,
                    ok: false,
                    panic: true,
                    diagnostic: None,
                    micros,
                },
            }
        })
        .collect();
    (entries, lossy)
}

fn group(root: &Path, path: &str) -> String {
    let p = root.join(path);
    p.strip_prefix(root)
        .ok()
        .and_then(|r| r.components().next())
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .filter(|s| s != path)
        .unwrap_or_else(|| "(root)".to_string())
}

fn print_report(root: &Path, entries: &[Entry], lossy: usize) {
    let mut groups: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
    for e in entries {
        let g = groups.entry(group(root, &e.path)).or_default();
        g.2 += 1;
        if e.panic {
            g.1 += 1;
        } else if e.ok {
            g.0 += 1;
        }
    }
    println!("folder\tok\tfail\tpanic\ttotal\t% ok");
    for (name, (ok, panic, total)) in groups {
        print_row(&name, ok, total - ok - panic, panic, total);
    }
    let ok = entries.iter().filter(|e| e.ok).count();
    let panics = entries.iter().filter(|e| e.panic).count();
    print_row(
        "OVERALL",
        ok,
        entries.len() - ok - panics,
        panics,
        entries.len(),
    );
    println!("lossy Latin-1 fallback: {lossy}");
    let mut codes: BTreeMap<&'static str, (usize, &Entry)> = BTreeMap::new();
    for e in entries {
        if let Some(d) = &e.diagnostic {
            let v = codes.entry(d.code).or_insert((0, e));
            v.0 += 1;
        } else if e.panic {
            let v = codes.entry("PANIC").or_insert((0, e));
            v.0 += 1;
        }
    }
    let mut codes: Vec<_> = codes.into_iter().collect();
    codes.sort_by(|a, b| b.1.0.cmp(&a.1.0).then_with(|| a.0.cmp(b.0)));
    println!("\ntop diagnostic codes\tcount\tfirst example");
    for (code, (count, e)) in codes.into_iter().take(20) {
        let sample = e
            .diagnostic
            .as_ref()
            .map(|d| format!("{}:{}:{} {}", e.path, d.line, d.column, d.message))
            .unwrap_or_else(|| format!("{} parser panicked", e.path));
        println!("{code}\t{count}\t{sample}");
    }
    let mut slow: Vec<_> = entries.iter().collect();
    slow.sort_by_key(|e| std::cmp::Reverse(e.micros));
    println!("\n10 slowest files\tmicros\tpath");
    for e in slow.into_iter().take(10) {
        println!("{}\t{}", e.micros, e.path);
    }
}

fn print_row(name: &str, ok: usize, fail: usize, panic: usize, total: usize) {
    let pct = if total == 0 {
        0.0
    } else {
        ok as f64 * 100.0 / total as f64
    };
    println!("{name}\t{ok}\t{fail}\t{panic}\t{total}\t{pct:.1}");
}

fn escape(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn write_json(path: &Path, entries: &[Entry]) -> std::io::Result<()> {
    let mut out = String::from("{\n  \"files\": [\n");
    for (i, e) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(",\n");
        }
        let (code, line, col, message) = if e.panic {
            ("PANIC", 0, 0, "parser panicked".to_string())
        } else if let Some(d) = &e.diagnostic {
            (d.code, d.line, d.column, d.message.clone())
        } else {
            ("", 0, 0, String::new())
        };
        out.push_str(&format!("    {{\"path\":{},\"ok\":{},\"panic\":{},\"code\":{},\"line\":{},\"col\":{},\"message\":{},\"micros\":{}}}", escape(&e.path), e.ok, e.panic, escape(code), line, col, escape(&message), e.micros));
    }
    out.push_str("\n  ]\n}\n");
    fs::write(path, out)
}

fn parse_baseline(json: &str) -> Result<HashMap<String, bool>, String> {
    let mut map = HashMap::new();
    let mut rest = json;
    while let Some(pos) = rest.find("\"path\"") {
        rest = &rest[pos + 6..];
        let (path, consumed) = parse_string(rest)?;
        rest = &rest[consumed..];
        let okpos = rest.find("\"ok\"").ok_or("missing ok field")?;
        let val = rest[okpos + 4..].trim_start();
        let ok = val
            .strip_prefix(':')
            .ok_or("invalid ok field")?
            .trim_start()
            .starts_with("true");
        map.insert(path, ok);
    }
    Ok(map)
}

fn parse_string(s: &str) -> Result<(String, usize), String> {
    let quote = s.find('"').ok_or("missing string")?;
    let start = quote + 1;
    let bytes = s.as_bytes();
    let mut i = start;
    let mut out = String::new();
    while i < bytes.len() {
        match bytes[i] {
            b'"' => return Ok((out, i + 1)),
            b'\\' => {
                i += 1;
                if i >= bytes.len() {
                    return Err("bad escape".into());
                }
                match bytes[i] {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    _ => return Err("unsupported escape".into()),
                }
            }
            b if b < 0x80 => out.push(b as char),
            _ => {
                let c = s[i..].chars().next().ok_or("invalid UTF-8")?;
                out.push(c);
                i += c.len_utf8() - 1;
            }
        }
        i += 1;
    }
    Err("unterminated string".into())
}

fn print_baseline(old: &HashMap<String, bool>, new: &[Entry]) -> bool {
    let now: HashMap<_, _> = new.iter().map(|e| (e.path.as_str(), e.ok)).collect();
    let mut regressions = Vec::new();
    let mut recovered = Vec::new();
    for (path, was_ok) in old {
        if *was_ok && now.get(path.as_str()) == Some(&false) {
            regressions.push(path);
        }
    }
    for e in new {
        if !old.get(&e.path).copied().unwrap_or(false) && e.ok {
            recovered.push(&e.path);
        }
    }
    regressions.sort();
    recovered.sort();
    println!("\nnewly failing files (baseline passed, current failed):");
    for p in &regressions {
        println!("  {p}");
    }
    println!("newly passing files (baseline failed or absent, current passed):");
    for p in &recovered {
        println!("  {p}");
    }
    !regressions.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use indy_syntax::Span;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    fn fixture() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "indy-corpus-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }
    fn good() -> ParseResult {
        let result = indy_syntax::parse_file("", Dialect::Octave);
        assert!(result.ok());
        result
    }
    #[test]
    fn grouping_and_fallback() {
        let root = fixture();
        fs::create_dir_all(root.join("pkg")).unwrap();
        fs::write(root.join("pkg/a.m"), [0xe9]).unwrap();
        fs::write(root.join("b.m"), "x").unwrap();
        let (entries, lossy) = scan_with(&root, |_| good());
        assert_eq!(lossy, 1);
        assert_eq!(entries.len(), 2);
        assert_eq!(group(&root, "pkg/a.m"), "pkg");
        assert_eq!(group(&root, "b.m"), "(root)");
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn panic_is_recorded() {
        let root = fixture();
        fs::write(root.join("x.m"), "x").unwrap();
        let (es, _) = scan_with(&root, |_| panic!("fixture panic"));
        assert!(es[0].panic);
        assert!(!es[0].ok);
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn escaping_and_baseline_regression() {
        let p = fixture().join("out.json");
        let e = Entry {
            path: "q\\\"é\n.m".into(),
            ok: false,
            panic: false,
            diagnostic: Some(Diagnostic {
                span: Span { start: 0, end: 0 },
                line: 2,
                column: 3,
                code: "E",
                message: "say \"hi\"\\there\né".into(),
            }),
            micros: 7,
        };
        write_json(&p, &[e]).unwrap();
        let text = fs::read_to_string(&p).unwrap();
        assert!(text.contains("\\\\\\\"é\\n.m"));
        assert!(text.contains("\\\"hi\\\"\\\\there\\né"));
        let baseline = r#"{"files":[{"path":"x.m","ok":true}]}"#;
        let old = parse_baseline(baseline).unwrap();
        assert!(print_baseline(
            &old,
            &[Entry {
                path: "x.m".into(),
                ok: false,
                panic: false,
                diagnostic: None,
                micros: 0
            }]
        ));
        let _ = fs::remove_file(p);
    }
}
