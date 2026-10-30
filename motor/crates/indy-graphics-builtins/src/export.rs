//! `print`, `saveas`, `exportgraphics`: figures to files through indy-raster.
//!
//! PNG, JPEG and TIFF are rendered pixels; SVG and PDF are vector output (a
//! 3-D scene inside them is an embedded image). Any other format is an
//! explicit error; no empty or fake file is ever written.

use indy_graphics::json::Json;
use indy_graphics::{Graphics, Handle, PropValue};
use indy_interp::builtins::core::resolve_path;
use indy_interp::builtins::err;
use indy_interp::{Interp, R, Value};
use indy_raster::{
    Options, Theme, jpeg, only_axes, render, render_pdf_region, render_svg_region, tiff,
};
use indy_value::{Num, NumClass};

use crate::convert::{gerr, to_prop};
use crate::graphics;

const DEFAULT_DPI: f64 = 150.0;
const JPEG_QUALITY: u32 = 90;
/// The session's figure PNGs, as the Octave kernel exports them.
const SESSION_DPI: f64 = 120.0;
/// Their largest side; the UI shows them in a panel, not a poster.
const SESSION_SIDE: f64 = 4096.0;
/// The largest image an export writes.
const MAX_PIXELS: f64 = 100e6;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Format {
    Png,
    Jpeg(u32),
    Tiff,
    Pdf,
    Svg,
}

impl Format {
    fn extension(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg(_) => "jpg",
            Format::Tiff => "tif",
            Format::Pdf => "pdf",
            Format::Svg => "svg",
        }
    }

    fn from_extension(ext: &str) -> Option<Format> {
        Some(match ext.to_ascii_lowercase().as_str() {
            "png" => Format::Png,
            "jpg" | "jpeg" => Format::Jpeg(JPEG_QUALITY),
            "tif" | "tiff" => Format::Tiff,
            "pdf" => Format::Pdf,
            "svg" => Format::Svg,
            _ => return None,
        })
    }
}

fn not_supported(fname: &str, what: &str) -> Box<indy_interp::Error> {
    err(
        "IndyR:exportUnsupported",
        format!(
            "{fname}: {what} is not supported in the Rust engine yet (use png, jpg, tif, pdf or svg)"
        ),
    )
}

fn extension_of(file: &str) -> Option<&str> {
    let name = file.rsplit(['/', '\\']).next().unwrap_or(file);
    name.rsplit_once('.')
        .map(|(_, e)| e)
        .filter(|e| !e.is_empty())
}

/// The figure's own colour, for the paper the export is drawn on.
pub(crate) fn figure_color(g: &Graphics, fig: Handle) -> [f64; 3] {
    match g.get(fig, "Color") {
        Ok(PropValue::Array(a))
            if a.data.len() == 3 && a.data.iter().all(|v| (0.0..=1.0).contains(v)) =>
        {
            [a.data[0], a.data[1], a.data[2]]
        }
        _ => [1.0, 1.0, 1.0],
    }
}

/// The session's PNG of a figure, as the kernel stores it next to the JSON.
pub(crate) fn figure_png(record: &Json, background: [f64; 3]) -> Vec<u8> {
    let [w, h] = indy_raster::figure_size(record);
    let dpi = SESSION_DPI.min(SESSION_SIDE * indy_raster::SCREEN_DPI / w.max(h));
    let mut o = Options::export(record, dpi);
    o.theme = Theme::paper().with_background(background);
    render(record, &o).to_png(Some(dpi))
}

struct Job {
    record: Json,
    /// Crop to the drawn content (one axes).
    crop: bool,
    dpi: f64,
    background: [f64; 3],
    format: Format,
    /// Embed a rendered image in a PDF (`ContentType` `image`).
    pdf_image: bool,
}

fn encode(job: &Job) -> Vec<u8> {
    let mut o = Options::export(&job.record, job.dpi);
    o.theme = Theme::paper().with_background(job.background);
    // A vector page shows the region a cropped image would hold.
    let region = || {
        job.crop
            .then(|| render(&job.record, &o))
            .and_then(|image| image.ink_box(job.background, (o.dpr * 4.0).round() as usize))
            .map(|pixels| pixels.map(|v| v as f64 / o.dpr))
    };
    match job.format {
        Format::Svg => render_svg_region(&job.record, &o, region()).into_bytes(),
        Format::Pdf if !job.pdf_image => render_pdf_region(&job.record, &o, region()),
        format => {
            let mut image = render(&job.record, &o);
            if job.crop {
                image = image.crop_to_ink(job.background, (o.dpr * 4.0).round() as usize);
            }
            match format {
                Format::Png => image.to_png(Some(job.dpi)),
                Format::Jpeg(q) => jpeg(image.width, image.height, &image.rgb, q, job.dpi),
                Format::Tiff => tiff(image.width, image.height, &image.rgb, job.dpi),
                _ => {
                    let page = Options {
                        width: image.width as f64 / o.dpr,
                        height: image.height as f64 / o.dpr,
                        ..o
                    };
                    indy_raster::pdf_of_image(&image, &page)
                }
            }
        }
    }
}

/// An export that large is a mistyped resolution, not a figure.
fn check_size(fname: &str, record: &Json, dpi: f64) -> R<()> {
    let [w, h] = indy_raster::figure_size(record);
    let scale = dpi / indy_raster::SCREEN_DPI;
    if w * h * scale * scale > MAX_PIXELS || w.max(h) * scale > indy_raster::MAX_SIDE {
        return Err(err(
            "MATLAB:print:InvalidOption",
            format!(
                "{fname}: {dpi} dpi gives an image of {:.0}x{:.0} pixels, which is too large",
                w * scale,
                h * scale
            ),
        ));
    }
    Ok(())
}

fn write_file(it: &Interp, fname: &str, file: &str, data: &[u8]) -> R<()> {
    let path = resolve_path(it, file);
    std::fs::write(&path, data).map_err(|e| {
        err(
            "MATLAB:print:CannotCreateOutputFile",
            format!("{fname}: cannot write '{}': {e}", path.display()),
        )
    })
}

/// A figure handle, or the figure of an axes handle.
fn target_figure(g: &mut Graphics, v: &Value, fname: &str) -> R<(Handle, Option<Handle>)> {
    let h = match to_prop(v, fname)? {
        PropValue::Num(n) => Handle::from_value(n).ok_or_else(|| {
            err(
                "MATLAB:hg:InvalidHandle",
                format!("{fname}: invalid figure or axes handle"),
            )
        })?,
        PropValue::Handle(h) => h,
        _ => {
            return Err(err(
                "MATLAB:hg:InvalidArgument",
                format!("{fname}: the first argument must be a figure or axes handle"),
            ));
        }
    };
    let Ok(fig) = g.ancestor_figure(h) else {
        return Err(err(
            "MATLAB:hg:InvalidHandle",
            format!("{fname}: invalid figure or axes handle"),
        ));
    };
    Ok((fig, (fig != h).then_some(h)))
}

fn text_arg(v: &Value) -> Option<String> {
    if matches!(v, Value::Char(_) | Value::Str(_)) {
        v.to_str()
    } else {
        None
    }
}

/// `print(fig, file, '-dpng', '-r300')`, `print -dpng file`, `print('-f2', ...)`,
/// `cdata = print('-RGBImage')`.
pub(crate) fn print(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let g = graphics(it);
    let mut fig = None;
    let mut file: Option<String> = None;
    let mut format = None;
    let mut dpi = DEFAULT_DPI;
    let mut rgb_image = false;
    for (i, v) in a.iter().enumerate() {
        let Some(s) = text_arg(v) else {
            if i == 0 {
                fig = Some(target_figure(g, v, "print")?.0);
                continue;
            }
            return Err(err(
                "MATLAB:print:InvalidOption",
                "print: options must be text",
            ));
        };
        let Some(opt) = s.strip_prefix('-') else {
            if file.is_some() {
                return Err(err(
                    "MATLAB:print:InvalidOption",
                    format!("print: unexpected argument '{s}'"),
                ));
            }
            file = Some(s);
            continue;
        };
        let lower = opt.to_ascii_lowercase();
        if let Some(device) = lower.strip_prefix('d') {
            format = Some(match device {
                "png" => Format::Png,
                "jpeg" | "jpg" => Format::Jpeg(JPEG_QUALITY),
                "tiff" | "tiffn" | "tif" => Format::Tiff,
                "pdf" => Format::Pdf,
                "svg" => Format::Svg,
                _ => match device.strip_prefix("jpeg").and_then(|q| q.parse().ok()) {
                    Some(q) => Format::Jpeg(q),
                    None => return Err(not_supported("print", &format!("the device -d{device}"))),
                },
            });
        } else if let Some(r) = lower.strip_prefix('r') {
            if lower == "rgbimage" {
                rgb_image = true;
            } else {
                let parsed: f64 = r.parse().map_err(|_| {
                    err(
                        "MATLAB:print:InvalidOption",
                        format!("print: invalid resolution '-{opt}'"),
                    )
                })?;
                if parsed.is_nan() || parsed < 0.0 || !parsed.is_finite() {
                    return Err(err(
                        "MATLAB:print:InvalidOption",
                        format!("print: invalid resolution '-{opt}'"),
                    ));
                }
                // -r0 is screen resolution.
                dpi = if parsed == 0.0 {
                    indy_raster::SCREEN_DPI
                } else {
                    parsed
                };
            }
        } else if let Some(n) = lower
            .strip_prefix('f')
            .filter(|n| !n.is_empty() && n != &"illpage")
        {
            let number: f64 = n.parse().map_err(|_| {
                err(
                    "MATLAB:print:InvalidOption",
                    format!("print: invalid figure '-{opt}'"),
                )
            })?;
            let (h, _) = target_figure(g, &Value::num(number), "print")?;
            fig = Some(h);
        } else if lower == "clipboard"
            || lower.starts_with('p') && lower != "painters" && lower != "portrait"
        {
            return Err(not_supported(
                "print",
                "printing to a printer or the clipboard",
            ));
        } else if lower.starts_with('s') {
            return Err(not_supported("print", "printing a Simulink system"));
        } else if !matches!(
            lower.as_str(),
            "painters"
                | "opengl"
                | "vector"
                | "image"
                | "noui"
                | "loose"
                | "tight"
                | "fillpage"
                | "bestfit"
                | "portrait"
                | "landscape"
                | "append"
                | "cmyk"
                | "rgb"
                | "tiffpreview"
        ) {
            return Err(err(
                "MATLAB:print:InvalidOption",
                format!("print: unknown option '-{opt}'"),
            ));
        }
    }
    let fig = match fig {
        Some(f) => f,
        None => g.gcf(),
    };
    let background = figure_color(g, fig);
    let record = g.raster_record(fig);
    check_size("print", &record, dpi)?;
    if rgb_image {
        let mut o = Options::export(&record, dpi);
        o.theme = Theme::paper().with_background(background);
        let image = render(&record, &o);
        return Ok(vec![rgb_value(&image)]);
    }
    let _ = nargout;
    let Some(mut file) = file else {
        return Err(not_supported(
            "print",
            "printing to a printer (give a file name)",
        ));
    };
    let format = match format {
        Some(f) => f,
        None => match extension_of(&file) {
            Some(ext) => Format::from_extension(ext)
                .ok_or_else(|| not_supported("print", &format!("the file type .{ext}")))?,
            None => Format::Png,
        },
    };
    if extension_of(&file).is_none() {
        file = format!("{file}.{}", format.extension());
    }
    let job = Job {
        record,
        crop: false,
        dpi,
        background,
        format,
        pdf_image: false,
    };
    let bytes = encode(&job);
    write_file(it, "print", &file, &bytes)?;
    Ok(Vec::new())
}

fn rgb_value(image: &indy_raster::Image) -> Value {
    let (w, h) = (image.width, image.height);
    let mut data = vec![0.0; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 3;
            for c in 0..3 {
                data[y + x * h + c * w * h] = image.rgb[i + c] as f64;
            }
        }
    }
    Value::Num(Num::real(vec![h, w, 3], data).with_class(NumClass::UInt8))
}

/// `saveas(h, file)`, `saveas(h, file, format)`.
pub(crate) fn saveas(it: &mut Interp, a: Vec<Value>, _nargout: usize) -> R<Vec<Value>> {
    if a.len() < 2 || a.len() > 3 {
        return Err(err(
            "MATLAB:narginchk:notEnoughInputs",
            "saveas: usage saveas(h, filename, format)",
        ));
    }
    let g = graphics(it);
    let (fig, _) = target_figure(g, &a[0], "saveas")?;
    let Some(mut file) = text_arg(&a[1]) else {
        return Err(err(
            "MATLAB:saveas:invalidFilename",
            "saveas: the file name must be text",
        ));
    };
    let requested = match a.get(2) {
        Some(v) => Some(text_arg(v).ok_or_else(|| {
            err(
                "MATLAB:saveas:invalidFormat",
                "saveas: the format must be text",
            )
        })?),
        None => None,
    };
    let word = requested
        .clone()
        .or_else(|| extension_of(&file).map(str::to_string));
    let Some(word) = word else {
        return Err(not_supported(
            "saveas",
            "the .fig format (give a file extension or format)",
        ));
    };
    let word = word.trim_start_matches('.').to_string();
    let format = match word.to_ascii_lowercase().as_str() {
        "tiffn" => Format::Tiff,
        lower => Format::from_extension(lower)
            .ok_or_else(|| not_supported("saveas", &format!("the format {lower}")))?,
    };
    if extension_of(&file).is_none() {
        file = format!("{file}.{}", format.extension());
    }
    let background = figure_color(g, fig);
    let record = g.raster_record(fig);
    check_size("saveas", &record, DEFAULT_DPI)?;
    let job = Job {
        record,
        crop: false,
        dpi: DEFAULT_DPI,
        background,
        format,
        pdf_image: false,
    };
    let bytes = encode(&job);
    write_file(it, "saveas", &file, &bytes)?;
    Ok(Vec::new())
}

/// `exportgraphics(target, file, Name, Value, ...)`: `Resolution`,
/// `ContentType`, `BackgroundColor`. An axes target is cropped to its content.
pub(crate) fn exportgraphics(it: &mut Interp, a: Vec<Value>, _nargout: usize) -> R<Vec<Value>> {
    if a.len() < 2 {
        return Err(err(
            "MATLAB:narginchk:notEnoughInputs",
            "exportgraphics: usage exportgraphics(target, filename, ...)",
        ));
    }
    let g = graphics(it);
    let (fig, axes) = target_figure(g, &a[0], "exportgraphics")?;
    let Some(file) = text_arg(&a[1]) else {
        return Err(err(
            "MATLAB:exportgraphics:invalidFilename",
            "exportgraphics: the file name must be text",
        ));
    };
    if !(a.len() - 2).is_multiple_of(2) {
        return Err(err(
            "MATLAB:exportgraphics:invalidOption",
            "exportgraphics: options must be name/value pairs",
        ));
    }
    let Some(ext) = extension_of(&file) else {
        return Err(err(
            "MATLAB:exportgraphics:invalidFilename",
            "exportgraphics: the file name needs an extension (png, jpg, tif, pdf or svg)",
        ));
    };
    let format = Format::from_extension(ext)
        .ok_or_else(|| not_supported("exportgraphics", &format!("the file type .{ext}")))?;
    let mut dpi = DEFAULT_DPI;
    let mut content = "auto".to_string();
    let mut background = figure_color(g, fig);
    for pair in a[2..].as_chunks::<2>().0 {
        let Some(name) = text_arg(&pair[0]) else {
            return Err(err(
                "MATLAB:exportgraphics:invalidOption",
                "exportgraphics: option names must be text",
            ));
        };
        match name.to_ascii_lowercase().as_str() {
            "resolution" => {
                let v = pair[1].to_f64_vec().filter(|v| v.len() == 1).map(|v| v[0]);
                match v {
                    Some(r) if r >= 1.0 && r.is_finite() && r.fract() == 0.0 => dpi = r,
                    _ => {
                        return Err(err(
                            "MATLAB:exportgraphics:invalidOption",
                            "exportgraphics: Resolution must be a positive integer",
                        ));
                    }
                }
            }
            "contenttype" => {
                let v = text_arg(&pair[1]).unwrap_or_default().to_ascii_lowercase();
                if !matches!(v.as_str(), "auto" | "image" | "vector") {
                    return Err(err(
                        "MATLAB:exportgraphics:invalidOption",
                        "exportgraphics: ContentType must be auto, image or vector",
                    ));
                }
                content = v;
            }
            "backgroundcolor" => {
                let v = to_prop(&pair[1], "exportgraphics")?;
                if matches!(&v, PropValue::Str(s) if s.eq_ignore_ascii_case("none")) {
                    return Err(not_supported(
                        "exportgraphics",
                        "a transparent BackgroundColor",
                    ));
                }
                if !matches!(&v, PropValue::Str(s) if s.eq_ignore_ascii_case("current")) {
                    background = match indy_graphics::props::parse_paint(&v, "BackgroundColor", &[])
                        .map_err(gerr)?
                    {
                        indy_graphics::model::Paint::Rgb(c) => c,
                        _ => background,
                    };
                }
            }
            "colorspace" => {
                let v = text_arg(&pair[1]).unwrap_or_default();
                if !v.eq_ignore_ascii_case("rgb") {
                    return Err(not_supported(
                        "exportgraphics",
                        &format!("the color space {v}"),
                    ));
                }
            }
            "append" => {
                if pair[1]
                    .to_f64_vec()
                    .is_some_and(|v| v.iter().any(|x| *x != 0.0))
                {
                    return Err(not_supported("exportgraphics", "appending to a file"));
                }
            }
            "padding" => return Err(not_supported("exportgraphics", "the Padding option")),
            other => {
                return Err(err(
                    "MATLAB:exportgraphics:invalidOption",
                    format!("exportgraphics: unknown option {other}"),
                ));
            }
        }
    }
    if content == "vector" && !matches!(format, Format::Pdf | Format::Svg) {
        return Err(err(
            "MATLAB:exportgraphics:invalidOption",
            format!("exportgraphics: ContentType vector needs a vector file type, not .{ext}"),
        ));
    }
    let mut record = g.raster_record(fig);
    check_size("exportgraphics", &record, dpi)?;
    if let Some(ax) = axes {
        let list = g.record_axes(fig);
        if let Some(i) = list.iter().position(|h| *h == ax) {
            record = only_axes(&record, i);
        }
    }
    let job = Job {
        record,
        crop: axes.is_some(),
        dpi,
        background,
        format,
        pdf_image: content == "image",
    };
    let bytes = encode(&job);
    write_file(it, "exportgraphics", &file, &bytes)?;
    Ok(Vec::new())
}
