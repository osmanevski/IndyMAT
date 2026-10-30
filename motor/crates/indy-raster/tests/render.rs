//! Rendering through the public API: real figures from the graphics model.

use indy_graphics::json::{Json, parse};
use indy_graphics::{Graphics, PlotSeries, PropValue, ScatterArgs, opt};
use indy_raster::{Image, Options, Theme, figure_size, jpeg, render, render_pdf, render_svg, tiff};
use indy_value::deflate::zlib_decompress;

fn paper(record: &Json) -> Image {
    render(
        record,
        &Options {
            theme: Theme::paper(),
            ..Options::export(record, 96.0)
        },
    )
}

fn px(img: &Image, x: usize, y: usize) -> [u8; 3] {
    let i = (y * img.width + x) * 3;
    [img.rgb[i], img.rgb[i + 1], img.rgb[i + 2]]
}

fn inked(img: &Image) -> usize {
    img.rgb
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|p| *p != &[255, 255, 255])
        .count()
}

fn differing(a: &Image, b: &Image) -> usize {
    assert_eq!((a.width, a.height), (b.width, b.height));
    a.rgb
        .as_chunks::<3>()
        .0
        .iter()
        .zip(b.rgb.as_chunks::<3>().0.iter())
        .filter(|(p, q)| p != q)
        .count()
}

/// Minimal PNG reader: the chunks this crate writes, filters 0 to 2.
fn decode_png(data: &[u8]) -> (usize, usize, Vec<u8>) {
    assert_eq!(&data[..8], b"\x89PNG\r\n\x1a\n");
    let (mut at, mut w, mut h, mut idat) = (8, 0, 0, Vec::new());
    while at < data.len() {
        let len = u32::from_be_bytes(data[at..at + 4].try_into().unwrap()) as usize;
        let kind = &data[at + 4..at + 8];
        let body = &data[at + 8..at + 8 + len];
        let crc = u32::from_be_bytes(data[at + 8 + len..at + 12 + len].try_into().unwrap());
        assert_eq!(crc, indy_value::deflate::crc32(&data[at + 4..at + 8 + len]));
        if kind == b"IHDR" {
            w = u32::from_be_bytes(body[..4].try_into().unwrap()) as usize;
            h = u32::from_be_bytes(body[4..8].try_into().unwrap()) as usize;
            assert_eq!(&body[8..13], &[8, 2, 0, 0, 0]);
        } else if kind == b"IDAT" {
            idat.extend_from_slice(body);
        }
        at += 12 + len;
    }
    let raw = zlib_decompress(&idat).unwrap();
    let stride = w * 3;
    assert_eq!(raw.len(), (stride + 1) * h);
    let mut out = vec![0u8; stride * h];
    for y in 0..h {
        let filter = raw[y * (stride + 1)];
        let line = &raw[y * (stride + 1) + 1..(y + 1) * (stride + 1)];
        for x in 0..stride {
            let left = if x >= 3 { out[y * stride + x - 3] } else { 0 };
            let up = if y > 0 { out[(y - 1) * stride + x] } else { 0 };
            out[y * stride + x] = match filter {
                0 => line[x],
                1 => line[x].wrapping_add(left),
                2 => line[x].wrapping_add(up),
                other => panic!("filter {other}"),
            };
        }
    }
    (w, h, out)
}

fn fig_of(g: &Graphics) -> indy_graphics::Handle {
    let figs = g.figure_handles();
    *figs.last().expect("a figure")
}

fn line_figure() -> Graphics {
    let mut g = Graphics::new();
    g.plot(
        None,
        &[PlotSeries::xy(&[0.0, 1.0], &[0.0, 1.0])],
        &[opt("LineWidth", 2.0)],
    )
    .unwrap();
    g
}

#[test]
fn png_round_trips_exactly() {
    let g = line_figure();
    let record = g.raster_record(fig_of(&g));
    let img = paper(&record);
    let (w, h, rgb) = decode_png(&img.to_png(Some(96.0)));
    assert_eq!((w, h), (img.width, img.height));
    assert_eq!(rgb, img.rgb);
}

#[test]
fn a_line_lands_where_the_axes_say() {
    let g = line_figure();
    let record = g.raster_record(fig_of(&g));
    let [fw, fh] = figure_size(&record);
    assert_eq!((fw, fh), (560.0, 420.0));
    let img = paper(&record);
    assert_eq!((img.width, img.height), (560, 420));
    // Corner of the figure: paper. The middle of a [0 1]x[0 1] diagonal: ink.
    assert_eq!(px(&img, 2, 2), [255, 255, 255]);
    let (x, y) = (
        560.0 * (0.13 + 0.775 / 2.0),
        420.0 * (1.0 - 0.11 - 0.815 / 2.0),
    );
    let c = px(&img, x as usize, y as usize);
    assert!(
        c[2] > c[0] + 40,
        "default blue line at the centre, got {c:?}"
    );
}

#[test]
fn resolution_scales_the_device() {
    let g = line_figure();
    let record = g.raster_record(fig_of(&g));
    let img = render(&record, &Options::export(&record, 192.0));
    assert_eq!((img.width, img.height), (1120, 840));
    let low = render(&record, &Options::export(&record, 48.0));
    assert_eq!((low.width, low.height), (280, 210));
}

#[test]
fn turkish_text_and_tex_draw_real_glyphs() {
    for c in "çğıİöşüÇĞÖŞÜαθμωπ".chars() {
        assert!(
            indy_raster::font::has_glyph(indy_raster::font::Face::Sans, c),
            "{c}"
        );
        assert!(
            indy_raster::font::has_glyph(indy_raster::font::Face::SansBold, c),
            "{c}"
        );
    }
    let mut g = line_figure();
    let plain = paper(&g.raster_record(fig_of(&g)));
    g.label(
        None,
        indy_graphics::LabelRole::Title,
        &["çğıİöşü x^{2} \\pi".to_string()],
        &[],
    )
    .unwrap();
    let titled = paper(&g.raster_record(fig_of(&g)));
    let changed = differing(&plain, &titled);
    assert!(changed > 100, "title ink, {changed} px");
    // The rows above the axes hold the title, not the plot.
    let top: usize = (0..60)
        .map(|y| {
            (0..560)
                .filter(|x| px(&titled, *x, y) != [255, 255, 255])
                .count()
        })
        .sum();
    assert!(top > 50, "{top}");
}

#[test]
fn surface_and_contract_refusals_get_pictures() {
    // 3-D.
    let mut g = Graphics::new();
    let z = indy_graphics::Array {
        dims: vec![3, 3],
        data: vec![0.0, 1.0, 0.0, 1.0, 2.0, 1.0, 0.0, 1.0, 0.0],
        class: indy_graphics::NumClass::Double,
    };
    g.surf(None, &indy_graphics::GridArgs::z(z), &[]).unwrap();
    let img = paper(&g.raster_record(fig_of(&g)));
    assert!(inked(&img) > 5000);

    // A diamond marker is refused by the wire contract, drawn here.
    let mut g = Graphics::new();
    g.plot(
        None,
        &[PlotSeries::xy(&[0.0, 1.0], &[0.0, 1.0]).with_spec("d")],
        &[],
    )
    .unwrap();
    let fig = fig_of(&g);
    assert_eq!(
        g.figure_record(fig)
            .get("supported")
            .and_then(Json::as_bool),
        Some(false)
    );
    let record = g.raster_record(fig);
    assert_eq!(record.get("supported").and_then(Json::as_bool), Some(true));
    assert!(inked(&paper(&record)) > 300);

    // Per-point scatter colours are refused by the wire contract, drawn here.
    let mut g = Graphics::new();
    let cdata = PropValue::Array(indy_graphics::Array {
        dims: vec![3, 1],
        data: vec![0.0, 0.5, 1.0],
        class: indy_graphics::NumClass::Double,
    });
    let args = ScatterArgs {
        x: vec![1.0, 2.0, 3.0],
        y: vec![1.0, 2.0, 3.0],
        z: None,
        sizes: Vec::new(),
        color: Some(cdata),
        filled: true,
        marker: None,
    };
    g.scatter(None, &args, &[]).unwrap();
    let fig = fig_of(&g);
    assert_eq!(
        g.figure_record(fig)
            .get("reason_code")
            .and_then(Json::as_str),
        Some("scatter_colors")
    );
    let img = paper(&g.raster_record(fig));
    let colours: std::collections::HashSet<[u8; 3]> = img
        .rgb
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| [p[0], p[1], p[2]])
        .filter(|c| c[0] != c[1] || c[1] != c[2])
        .collect();
    assert!(
        colours.len() > 3,
        "several marker colours, got {}",
        colours.len()
    );

    // yyaxis: both rulers drawn.
    let mut g = Graphics::new();
    g.plot(None, &[PlotSeries::xy(&[0.0, 1.0], &[0.0, 1.0])], &[])
        .unwrap();
    g.yyaxis(None, true).unwrap();
    g.plot(None, &[PlotSeries::xy(&[0.0, 1.0], &[100.0, 50.0])], &[])
        .unwrap();
    let fig = fig_of(&g);
    assert_eq!(
        g.figure_record(fig)
            .get("reason_code")
            .and_then(Json::as_str),
        Some("unsupported_object")
    );
    let img = paper(&g.raster_record(fig));
    let right: usize = (0..420)
        .map(|y| {
            (510..560)
                .filter(|x| px(&img, *x, y) != [255, 255, 255])
                .count()
        })
        .sum();
    assert!(right > 30, "right-hand tick labels, {right} px");

    // An empty figure is a blank page.
    let mut g = Graphics::new();
    let fig = g.figure(None).unwrap();
    assert_eq!(inked(&paper(&g.raster_record(fig))), 0);
}

#[test]
fn all_markers_and_styles_draw() {
    let specs = [
        "-o", "--s", ":^", "-.v", "->", "-<", "-d", "-p", "-h", "-.", "-+", "-x", "-*",
    ];
    let mut last = None;
    for spec in specs {
        let mut g = Graphics::new();
        g.plot(
            None,
            &[PlotSeries::xy(&[0.0, 1.0, 2.0], &[0.0, 1.0, 0.0]).with_spec(spec)],
            &[],
        )
        .unwrap();
        let img = paper(&g.raster_record(fig_of(&g)));
        if let Some(previous) = &last {
            assert!(differing(previous, &img) > 10, "{spec}");
        }
        last = Some(img);
    }
}

#[test]
fn encoders_write_their_formats() {
    let g = line_figure();
    let record = g.raster_record(fig_of(&g));
    let options = Options::export(&record, 96.0);
    let img = render(&record, &options);
    let j = jpeg(img.width, img.height, &img.rgb, 90, 96.0);
    assert_eq!(&j[..3], &[0xFF, 0xD8, 0xFF]);
    assert_eq!(&j[j.len() - 2..], &[0xFF, 0xD9]);
    let t = tiff(img.width, img.height, &img.rgb, 96.0);
    assert_eq!(&t[..4], b"II*\0");
    assert_eq!(&t[t.len() - img.rgb.len()..], &img.rgb[..]);
    let s = render_svg(&record, &options);
    assert!(
        s.starts_with("<svg") && s.trim_end().ends_with("</svg>") && s.contains("stroke-width")
    );
    let p = render_pdf(&record, &options);
    assert!(p.starts_with(b"%PDF-1.4"));
    assert!(p.ends_with(b"%%EOF\n"));
}

#[test]
fn malformed_records_never_panic() {
    let options = Options::ui(200.0, 100.0);
    for text in [
        "{}",
        "null",
        "[]",
        r#"{"axes":7}"#,
        r#"{"axes":[{}]}"#,
        r#"{"axes":[{"dimension":3}]}"#,
        r#"{"axes":[{"position":[0,0,1,1],"series":[{"kind":"line"},{"kind":"image"},{"kind":"patch2d"},{"kind":"text"},{"kind":"surface"},{"kind":"patch3d"},{"kind":"patch_raster"}]}]}"#,
        r#"{"axes":[{"dimension":3,"position":[0,0,1,1],"xlim":[0,0],"series":[{"kind":"surface","shape":[2,2],"cell_origins":[0,100],"x":{"shape":[2],"values":[]},"y":{"shape":[2],"values":[]},"z":{"shape":[2,2],"values":[]},"face_color":{"mode":"flat"}}]}]}"#,
        r#"{"axes":[{"position":[0,0,1,1],"colorbars":[{"position":[0,0,1,1],"limits":[0,0],"ticks":[0],"colormap":{"shape":[0,3],"values":[]}}],"legend":{"visible":true,"labels":["a"]},"series":[{"kind":"patch2d","vertices":{"shape":[1,2],"values":[null,null]},"faces":{"shape":[1,3],"values":[5,6,7]},"display_name":"x"}]}],"version":3}"#,
    ] {
        let record = parse(text).unwrap();
        let _ = render(&record, &options);
        let _ = render_svg(&record, &options);
        let _ = render_pdf(&record, &options);
    }
}

#[test]
fn ui_theme_applies_the_contrast_rule_paper_does_not() {
    // Near-background yellow: the UI swaps it for plot ink, paper keeps it.
    let mut g = Graphics::new();
    let h = g
        .plot(
            None,
            &[PlotSeries::xy(&[0.0, 1.0], &[0.5, 0.5])],
            &[opt("LineWidth", 4.0)],
        )
        .unwrap();
    g.set(h[0], "Color", PropValue::from([1.0, 1.0, 0.8]))
        .unwrap();
    let record = g.figure_record(fig_of(&g));
    // The line's row in the middle column of the axes.
    let x = (560.0 * (0.13 + 0.775 / 2.0)) as usize;
    let ui = render(&record, &Options::ui(560.0, 420.0));
    let darkest = (40..380)
        .map(|y| px(&ui, x, y))
        .min_by_key(|c| c[0])
        .unwrap();
    assert!(darkest[0] < 60, "UI: plot ink, got {darkest:?}");
    let img = paper(&record);
    assert!(
        (40..380).any(|y| px(&img, x, y) == [255, 255, 204]),
        "paper keeps the colour"
    );
}
