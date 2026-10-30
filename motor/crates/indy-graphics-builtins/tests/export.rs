//! `print`, `saveas`, `exportgraphics` and the kernel's figure PNGs.

use indy_graphics_builtins::{open_figures, register};
use indy_interp::{Interp, Value};
use std::path::PathBuf;

fn interp(tag: &str) -> (Interp, PathBuf) {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("export-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut it = Interp::new();
    it.out = Box::new(std::io::sink());
    register(&mut it);
    it.set_cwd(dir.clone());
    (it, dir)
}

fn ok(it: &mut Interp, code: &str) {
    if let Err(e) = it.run_text(code) {
        panic!("{code}\n  -> {} ({})", e.message, e.identifier);
    }
}

fn fails(it: &mut Interp, code: &str) -> (String, String) {
    match it.run_text(code) {
        Ok(_) => panic!("expected an error from: {code}"),
        Err(e) => (e.identifier.clone(), e.message.clone()),
    }
}

fn png_size(data: &[u8]) -> (u32, u32) {
    assert!(data.starts_with(b"\x89PNG\r\n\x1a\n"));
    (
        u32::from_be_bytes(data[16..20].try_into().unwrap()),
        u32::from_be_bytes(data[20..24].try_into().unwrap()),
    )
}

#[test]
fn print_writes_png_at_the_requested_resolution() {
    let (mut it, dir) = interp("print");
    ok(
        &mut it,
        "f = figure('Position', [10 10 400 300]); plot(1:3); title('çğıİöşü');",
    );
    ok(&mut it, "print(f, 'a.png', '-dpng', '-r96');");
    assert_eq!(
        png_size(&std::fs::read(dir.join("a.png")).unwrap()),
        (400, 300)
    );
    ok(&mut it, "print(f, 'b', '-dpng', '-r192');");
    assert_eq!(
        png_size(&std::fs::read(dir.join("b.png")).unwrap()),
        (800, 600)
    );
    // Command syntax, default resolution 150, current figure.
    ok(&mut it, "print -dpng c.png");
    assert_eq!(
        png_size(&std::fs::read(dir.join("c.png")).unwrap()),
        (625, 469)
    );
    ok(&mut it, "print('-f1', 'd.png', '-r0');");
    assert_eq!(
        png_size(&std::fs::read(dir.join("d.png")).unwrap()),
        (400, 300)
    );
    ok(
        &mut it,
        "x = print('-RGBImage', '-r96'); s = size(x); c = class(x);",
    );
    assert_eq!(
        it.frames[0].vars.get("s").and_then(Value::to_f64_vec),
        Some(vec![300.0, 400.0, 3.0])
    );
    assert_eq!(
        it.frames[0]
            .vars
            .get("c")
            .and_then(Value::to_str)
            .as_deref(),
        Some("uint8")
    );
    // White paper in the corner.
    ok(
        &mut it,
        "assert(isequal(squeeze(x(1, 1, :))', uint8([255 255 255])));",
    );
}

#[test]
fn every_format_writes_its_own_file() {
    let (mut it, dir) = interp("formats");
    ok(&mut it, "plot(1:3); legend('a');");
    for (code, file, magic) in [
        (
            "print('p.jpg', '-djpeg');",
            "p.jpg",
            &[0xFF, 0xD8, 0xFF][..],
        ),
        ("print('q', '-djpeg95');", "q.jpg", &[0xFF, 0xD8, 0xFF][..]),
        ("print('p.tif', '-dtiff');", "p.tif", b"II*\0"),
        ("print('p.pdf', '-dpdf');", "p.pdf", b"%PDF-"),
        ("print('p.svg', '-dsvg');", "p.svg", b"<svg"),
        ("saveas(gcf, 's.png');", "s.png", b"\x89PNG"),
        ("saveas(gcf, 's.jpg');", "s.jpg", &[0xFF, 0xD8, 0xFF][..]),
        ("saveas(gcf, 's2', 'pdf');", "s2.pdf", b"%PDF-"),
        ("saveas(gca, 's3', 'svg');", "s3.svg", b"<svg"),
        ("exportgraphics(gcf, 'e.png');", "e.png", b"\x89PNG"),
        (
            "exportgraphics(gcf, 'e.jpeg', 'Resolution', 72);",
            "e.jpeg",
            &[0xFF, 0xD8, 0xFF][..],
        ),
        ("exportgraphics(gcf, 'e.tif');", "e.tif", b"II*\0"),
        (
            "exportgraphics(gcf, 'e.pdf', 'ContentType', 'vector');",
            "e.pdf",
            b"%PDF-",
        ),
        (
            "exportgraphics(gcf, 'e2.pdf', 'ContentType', 'image');",
            "e2.pdf",
            b"%PDF-",
        ),
    ] {
        ok(&mut it, code);
        let data = std::fs::read(dir.join(file)).unwrap();
        assert!(data.starts_with(magic), "{code}");
    }
}

#[test]
fn exportgraphics_options_and_axes_crop() {
    let (mut it, dir) = interp("eg");
    ok(
        &mut it,
        "f = figure('Position', [0 0 800 300]); a = subplot(1, 2, 1); plot(1:3); subplot(1, 2, 2); plot(3:-1:1);",
    );
    ok(&mut it, "exportgraphics(f, 'f.png', 'Resolution', 96);");
    assert_eq!(
        png_size(&std::fs::read(dir.join("f.png")).unwrap()),
        (800, 300)
    );
    ok(
        &mut it,
        "exportgraphics(a, 'a.png', 'Resolution', 96, 'BackgroundColor', 'white');",
    );
    let (w, h) = png_size(&std::fs::read(dir.join("a.png")).unwrap());
    assert!(w < 500 && h <= 300, "cropped to the axes, got {w}x{h}");
    ok(
        &mut it,
        "exportgraphics(f, 'y.png', 'BackgroundColor', [1 1 0]);",
    );
    // Source geometry is untouched.
    ok(
        &mut it,
        "p = get(f, 'Position'); assert(isequal(p, [0 0 800 300]));",
    );
}

#[test]
fn unsupported_requests_are_explicit_and_write_nothing() {
    let (mut it, dir) = interp("refuse");
    ok(&mut it, "plot(1:3);");
    for (code, needle) in [
        ("print('x.eps', '-depsc');", "not supported"),
        ("print();", "printer"),
        ("print('-clipboard', '-dpng');", "clipboard"),
        ("saveas(gcf, 'x.fig');", "not supported"),
        ("saveas(gcf, 'noext');", ".fig"),
        ("exportgraphics(gcf, 'x.gif');", "not supported"),
        (
            "exportgraphics(gcf, 'x.png', 'BackgroundColor', 'none');",
            "transparent",
        ),
        ("exportgraphics(gcf, 'x.png', 'Append', true);", "appending"),
        (
            "exportgraphics(gcf, 'x.png', 'ContentType', 'vector');",
            "vector",
        ),
        (
            "exportgraphics(gcf, 'x.png', 'Resolution', 0);",
            "Resolution",
        ),
        ("exportgraphics(gcf, 'x.png', 'Wat', 1);", "unknown option"),
        ("exportgraphics(12345, 'x.png');", "invalid figure"),
        (
            "print('/nonexistent-dir-indy/x.png', '-dpng');",
            "cannot write",
        ),
    ] {
        let (_, msg) = fails(&mut it, code);
        assert!(msg.contains(needle), "{code}: {msg}");
    }
    let files: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
    assert!(files.is_empty(), "nothing written: {files:?}");
}

#[test]
fn the_kernel_gets_a_png_per_figure_and_only_redraws_changes() {
    let (mut it, _dir) = interp("kernel");
    ok(
        &mut it,
        "figure('Name', 'One'); plot(1:3); figure; surf(peaks(10)); figure;",
    );
    let first = open_figures(&mut it);
    assert_eq!(first.len(), 3);
    assert_eq!(first[0].name, "One");
    for f in &first {
        let png = f
            .png
            .as_ref()
            .expect("a PNG for every figure, also the empty one");
        assert_eq!(png_size(png), (700, 525));
    }
    // Nothing changed: the very same bytes, not a new render.
    let again = open_figures(&mut it);
    for (a, b) in first.iter().zip(&again) {
        assert!(std::sync::Arc::ptr_eq(
            a.png.as_ref().unwrap(),
            b.png.as_ref().unwrap()
        ));
    }
    // One figure changes: only its PNG is new.
    ok(&mut it, "figure(1); title('yeni');");
    let third = open_figures(&mut it);
    assert!(!std::sync::Arc::ptr_eq(
        first[0].png.as_ref().unwrap(),
        third[0].png.as_ref().unwrap()
    ));
    assert!(std::sync::Arc::ptr_eq(
        first[1].png.as_ref().unwrap(),
        third[1].png.as_ref().unwrap()
    ));
    ok(&mut it, "close all;");
    assert!(open_figures(&mut it).is_empty());
}

#[test]
fn a_contract_refusal_still_gets_a_real_png() {
    let (mut it, _dir) = interp("fallback");
    ok(&mut it, "scatter(1:5, 1:5, 40, (1:5)', 'filled');");
    let figs = open_figures(&mut it);
    assert!(figs[0].json.contains("\"supported\":false"));
    let png = figs[0].png.as_ref().unwrap();
    assert!(png.len() > 2000, "a drawn figure, {} bytes", png.len());
}

/// The pixels of an 8-bit RGB PNG written by the engine.
fn png_pixels(data: &[u8]) -> (usize, usize, Vec<u8>) {
    let (w, h) = png_size(data);
    let (w, h) = (w as usize, h as usize);
    let mut idat = Vec::new();
    let mut at = 8;
    while at + 8 <= data.len() {
        let n = u32::from_be_bytes(data[at..at + 4].try_into().unwrap()) as usize;
        if &data[at + 4..at + 8] == b"IDAT" {
            idat.extend_from_slice(&data[at + 8..at + 8 + n]);
        }
        at += 12 + n;
    }
    assert_eq!(data[24], 8, "bit depth");
    assert_eq!(data[25], 2, "colour type");
    let raw = indy_interp::builtins::matfile::zlib::inflate(&idat, usize::MAX).unwrap();
    let stride = w * 3;
    let mut out = vec![0u8; stride * h];
    for y in 0..h {
        let filter = raw[y * (stride + 1)];
        let line = &raw[y * (stride + 1) + 1..(y + 1) * (stride + 1)];
        for x in 0..stride {
            let a = if x >= 3 { out[y * stride + x - 3] } else { 0 };
            let b = if y > 0 { out[(y - 1) * stride + x] } else { 0 };
            let c = if x >= 3 && y > 0 {
                out[(y - 1) * stride + x - 3]
            } else {
                0
            };
            let predicted = match filter {
                0 => 0,
                1 => a,
                2 => b,
                3 => ((a as u16 + b as u16) / 2) as u8,
                4 => {
                    let p = a as i32 + b as i32 - c as i32;
                    let (pa, pb, pc) = (
                        (p - a as i32).abs(),
                        (p - b as i32).abs(),
                        (p - c as i32).abs(),
                    );
                    if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    }
                }
                other => panic!("filter {other}"),
            };
            out[y * stride + x] = line[x].wrapping_add(predicted);
        }
    }
    (w, h, out)
}

fn count_color(data: &[u8], test: impl Fn(u8, u8, u8) -> bool) -> usize {
    let (_, _, rgb) = png_pixels(data);
    rgb.chunks(3).filter(|p| test(p[0], p[1], p[2])).count()
}

fn is_red(r: u8, g: u8, b: u8) -> bool {
    r > 240 && g < 15 && b < 15
}

#[test]
fn exportgraphics_of_the_right_yyaxis_keeps_the_left_side() {
    let (mut it, dir) = interp("yy_right");
    ok(
        &mut it,
        "figure; yyaxis left; plot(1:3, 'Color', [1 0 0]); yyaxis right; plot(3:-1:1, 'Color', [0 0 1]);
         exportgraphics(gca, 'right.png');",
    );
    let png = std::fs::read(dir.join("right.png")).unwrap();
    // Thin lines are antialiased: what counts is the hue.
    let reddish = |r: u8, g: u8, b: u8| r as i32 > g as i32 + 60 && r as i32 > b as i32 + 60;
    let bluish = |r: u8, g: u8, b: u8| b as i32 > r as i32 + 60 && b as i32 > g as i32 + 60;
    assert!(count_color(&png, reddish) > 100, "the left plot is missing");
    assert!(count_color(&png, bluish) > 100);
}

#[test]
fn exportgraphics_of_one_axes_ignores_a_neighbour_at_the_same_place_without_yyaxis() {
    let (mut it, dir) = interp("overlaid");
    ok(
        &mut it,
        "figure; a = axes; plot(1:3, 'Color', [1 0 0]); b = axes('Position', get(a, 'Position'), 'Color', 'none');
         plot(b, 3:-1:1, 'Color', [0 0 1]); exportgraphics(b, 'b.png');",
    );
    let png = std::fs::read(dir.join("b.png")).unwrap();
    assert_eq!(count_color(&png, is_red), 0);
}

#[test]
fn exportgraphics_of_one_axes_crops_vector_files_too() {
    let (mut it, dir) = interp("vector_crop");
    ok(
        &mut it,
        "f = figure('Position', [10 10 560 420]); a = subplot(1, 2, 1); plot(1:3); subplot(1, 2, 2); plot(3:-1:1);
         exportgraphics(a, 'a.svg'); exportgraphics(a, 'a.pdf'); exportgraphics(a, 'a.png', 'Resolution', 96);
         exportgraphics(f, 'f.svg'); exportgraphics(f, 'f.pdf');",
    );
    let svg = std::fs::read_to_string(dir.join("a.svg")).unwrap();
    let head = svg.lines().next().unwrap();
    let number = |key: &str| -> f64 {
        let from = head.find(key).unwrap() + key.len();
        head[from..].split('"').next().unwrap().parse().unwrap()
    };
    let (w, _) = png_size(&std::fs::read(dir.join("a.png")).unwrap());
    assert!(number(" width=\"") < 300.0, "{head}");
    assert!(
        (number(" width=\"") - w as f64).abs() <= 3.0,
        "{head} against {w}"
    );
    assert!(!head.contains("viewBox=\"0 0 560 420\""), "{head}");
    let media = |name: &str| -> Vec<f64> {
        let pdf = std::fs::read(dir.join(name)).unwrap();
        let text = String::from_utf8_lossy(&pdf).into_owned();
        let from = text.find("/MediaBox [").unwrap() + 11;
        let end = from + text[from..].find(']').unwrap();
        text[from..end]
            .split_whitespace()
            .map(|v| v.parse().unwrap())
            .collect()
    };
    let page = media("a.pdf");
    assert!(page[2] - page[0] < 225.0, "{page:?}");
    assert!(
        page[0] >= 0.0 && page[1] >= 0.0 && page[3] <= 315.0,
        "{page:?}"
    );
    // A figure target is not cropped.
    assert_eq!(media("f.pdf"), vec![0.0, 0.0, 420.0, 315.0]);
    let whole = std::fs::read_to_string(dir.join("f.svg")).unwrap();
    assert!(whole.contains("viewBox=\"0 0 560 420\""));
}

#[test]
fn saveas_refuses_a_figure_too_large_to_draw() {
    let (mut it, dir) = interp("saveas_huge");
    ok(&mut it, "figure('Position', [0 0 12000 12000]); plot(1:3);");
    let (id, message) = fails(&mut it, "saveas(gcf, 'huge.png');");
    assert_eq!(id, "MATLAB:print:InvalidOption");
    assert!(message.contains("too large"), "{message}");
    assert!(!dir.join("huge.png").exists());
}

#[test]
fn nan_colour_data_leaves_the_face_out() {
    let (mut it, dir) = interp("patch_nan");
    ok(
        &mut it,
        "figure; colormap([1 0 0; 0 0 1]);
         patch('Vertices', [0 0; 1 0; 0 1; 1 1], 'Faces', [1 2 3; 2 3 4], 'FaceVertexCData', [NaN; 2], 'CDataMapping', 'direct', 'FaceColor', 'flat', 'EdgeColor', 'none');
         print('direct.png', '-dpng');
         figure; patch('Vertices', [0 0; 1 0; 0 1; 1 1], 'Faces', [1 2 3; 2 3 4], 'FaceVertexCData', [NaN; 1], 'FaceColor', 'flat', 'EdgeColor', 'none');
         print('scaled.png', '-dpng');
         figure; patch('Vertices', [0 0 0; 1 0 1; 0 1 0; 1 1 1], 'Faces', [1 2 3; 2 3 4], 'FaceVertexCData', [NaN; 1], 'FaceColor', 'flat', 'EdgeColor', 'none'); view(3);
         print('three.png', '-dpng');",
    );
    for name in ["direct.png", "scaled.png", "three.png"] {
        let png = std::fs::read(dir.join(name)).unwrap();
        let (w, h, rgb) = png_pixels(&png);
        let dark = rgb
            .chunks(3)
            .filter(|p| p[0] < 30 && p[1] < 30 && p[2] < 30)
            .count();
        assert!(
            dark * 20 < w * h,
            "{name}: a NaN face was filled black ({dark} of {})",
            w * h
        );
    }
}

#[test]
fn infinite_direct_colour_data_clamps_to_the_ends_of_the_colormap() {
    let (mut it, dir) = interp("patch_inf");
    ok(
        &mut it,
        "figure; colormap([1 0 0; 0 0 1]);
         patch('Vertices', [0 0; 1 0; 0 1; 1 1], 'Faces', [1 2 3; 2 3 4], 'FaceVertexCData', [-Inf; -Inf], 'CDataMapping', 'direct', 'FaceColor', 'flat', 'EdgeColor', 'none');
         print('p.png', '-dpng');",
    );
    let png = std::fs::read(dir.join("p.png")).unwrap();
    assert!(
        count_color(&png, is_red) > 1000,
        "-Inf is the first colormap entry"
    );
}

#[test]
fn a_patch_drawn_by_the_rasteriser_shows_its_colour_in_the_legend() {
    let (mut it, dir) = interp("patch_legend");
    // The legend box sits in the northeast corner; the patches stay away from it.
    ok(
        &mut it,
        "figure; patch('Vertices', [0 0 0; 0.2 0 0.2; 0 0.2 0.2], 'Faces', [1 2 3], 'FaceColor', [1 0 0], 'EdgeColor', 'none', 'DisplayName', 'three');
         xlim([0 1]); ylim([0 1]); zlim([0 1]); view(3); legend('show'); print('three.png', '-dpng');
         figure; patch('Vertices', [0 0; 0.1 0; 0 0.1; 0.1 0.1], 'Faces', [1 2 3; 2 3 4], 'FaceVertexCData', [1 0 0; 0 1 0], 'FaceColor', 'flat', 'EdgeColor', 'none', 'DisplayName', 'two');
         xlim([0 1]); ylim([0 1]); legend('show'); print('two.png', '-dpng');",
    );
    for name in ["three.png", "two.png"] {
        let (w, h, rgb) = png_pixels(&std::fs::read(dir.join(name)).unwrap());
        let mut red = 0;
        for y in 0..h / 4 {
            for x in w * 3 / 5..w {
                let p = &rgb[(y * w + x) * 3..(y * w + x) * 3 + 3];
                if is_red(p[0], p[1], p[2]) {
                    red += 1;
                }
            }
        }
        assert!(red > 20, "{name}: no colour swatch in the legend ({red})");
    }
}

#[test]
fn a_flat_patch_above_the_plane_is_drawn_in_a_two_dimensional_view() {
    let (mut it, dir) = interp("patch_z");
    ok(
        &mut it,
        "figure; patch('Vertices', [0 0 1; 1 0 1; 1 1 1], 'Faces', [1 2 3], 'FaceColor', [1 0 0]); view(2);
         print('p.png', '-dpng');",
    );
    let png = std::fs::read(dir.join("p.png")).unwrap();
    assert!(count_color(&png, is_red) > 1000);
}
