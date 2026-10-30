//! Renders saved Figure-Data JSON artifacts, for measurement.
//!
//! raster_json <figure.json> <out.png> [--ui WIDTH HEIGHT | --dpi N] [--dark]
//! raster_json <figure.json> <out.svg|out.pdf> [--dpi N]

use indy_graphics::json::parse;
use indy_raster::{Options, Theme, render, render_pdf, render_svg};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: raster_json <figure.json> <out> [--ui W H | --dpi N] [--dark]");
        std::process::exit(2);
    }
    let text = std::fs::read_to_string(&args[0]).expect("read");
    let record = parse(&text).expect("json");
    let mut options = Options::export(&record, 120.0);
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--ui" => {
                options = Options::ui(
                    args[i + 1].parse().expect("w"),
                    args[i + 2].parse().expect("h"),
                );
                i += 3;
            }
            "--dpi" => {
                options = Options::export(&record, args[i + 1].parse().expect("dpi"));
                i += 2;
            }
            "--dark" => {
                options.theme = Theme::ui_dark();
                i += 1;
            }
            other => panic!("unknown option {other}"),
        }
    }
    let t = std::time::Instant::now();
    let bytes = if args[1].ends_with(".svg") {
        render_svg(&record, &options).into_bytes()
    } else if args[1].ends_with(".pdf") {
        render_pdf(&record, &options)
    } else {
        let image = render(&record, &options);
        let t1 = t.elapsed();
        let png = image.to_png(None);
        eprintln!(
            "{}x{} render {:?} total {:?} {} bytes",
            image.width,
            image.height,
            t1,
            t.elapsed(),
            png.len()
        );
        png
    };
    std::fs::write(&args[1], bytes).expect("write");
}
