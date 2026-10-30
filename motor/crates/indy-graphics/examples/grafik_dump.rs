//! Writes the Figure-Data JSON of every test case to a directory:
//! `cargo run -p indy-graphics --example grafik_dump -- <dir>`.
//! Used by `motor/scripts/grafik_validate.sh`.

#[path = "../tests/cases/mod.rs"]
mod cases;

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "grafik-out".to_string());
    std::fs::create_dir_all(&dir).expect("create output directory");
    let all = cases::cases().into_iter().chain(cases::extra_cases());
    let mut n = 0;
    for (name, build) in all {
        std::fs::write(format!("{dir}/{name}.json"), cases::run(build)).expect("write case");
        n += 1;
    }
    println!("wrote {n} figures to {dir}");
}
