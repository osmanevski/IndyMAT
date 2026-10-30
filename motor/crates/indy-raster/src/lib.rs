//! indy-raster: draws a Figure-Data record to RGB pixels, PNG, JPEG, TIFF,
//! SVG or PDF. Pure Rust, no external crates, no system fonts.
//!
//! The input is the same Figure-Data v2/v3 record the UI draws (a parsed
//! `indy_graphics::json::Json`), not the object model: `paint2d` and `paint3d`
//! are ports of the UI's painters, so the picture agrees with the interactive
//! view by construction, saved JSON artifacts can be re-rendered for pixel
//! comparison, and there is one layout and colour logic to maintain. Figures
//! the wire contract refuses come through `Graphics::raster_record`, the same
//! serializer with the contract's limits lifted.

#![allow(clippy::too_many_arguments)]
// `!(x > 0.0)` is the deliberate NaN-rejecting test throughout this crate.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

mod color;
mod device;
mod encode;
pub mod font;
mod paint2d;
mod paint3d;
mod raster;
mod rec;
mod tex;
mod vector;

pub use color::Theme;
pub use device::{Device, Rgb};
pub use encode::{jpeg, png, tiff};

use indy_graphics::json::Json;
use paint2d::{Affine, Figure, TextStyle, fit_viewport, label};
use rec::{num, nums, text};

/// Canvas size in logical pixels and the device-pixel ratio.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub width: f64,
    pub height: f64,
    pub dpr: f64,
    pub theme: Theme,
}

/// Screen pixels per inch of a figure's `Position` (MATLAB's convention).
pub const SCREEN_DPI: f64 = 96.0;
/// The largest side of a rendered image.
pub const MAX_SIDE: f64 = 16384.0;

impl Options {
    /// An exported figure: its own size at `dpi`, on white paper.
    pub fn export(record: &Json, dpi: f64) -> Options {
        let [w, h] = figure_size(record);
        let dpr = (dpi / SCREEN_DPI).clamp(0.05, MAX_SIDE / w.max(h).max(1.0));
        Options {
            width: w,
            height: h,
            dpr,
            theme: Theme::paper(),
        }
    }

    /// What the UI's canvas of that CSS size shows, in its light theme.
    pub fn ui(width: f64, height: f64) -> Options {
        Options {
            width,
            height,
            dpr: 1.0,
            theme: Theme::ui_light(),
        }
    }

    fn device_size(&self) -> (usize, usize) {
        (
            ((self.width * self.dpr).round().clamp(1.0, MAX_SIDE)) as usize,
            ((self.height * self.dpr).round().clamp(1.0, MAX_SIDE)) as usize,
        )
    }
}

/// `source.figure_size`, or MATLAB's default figure size.
pub fn figure_size(record: &Json) -> [f64; 2] {
    let v = record
        .get("source")
        .map(|s| nums(s, "figure_size"))
        .unwrap_or_default();
    if v.len() == 2 && v[0] > 0.0 && v[1] > 0.0 && v[0].is_finite() && v[1].is_finite() {
        [v[0].min(MAX_SIDE), v[1].min(MAX_SIDE)]
    } else {
        [560.0, 420.0]
    }
}

/// An RGB image, top row first.
#[derive(Debug, Clone)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
}

impl Image {
    pub fn to_rgba(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.width * self.height * 4);
        for p in self.rgb.as_chunks::<3>().0 {
            out.extend_from_slice(p);
            out.push(255);
        }
        out
    }

    pub fn to_png(&self, dpi: Option<f64>) -> Vec<u8> {
        encode::png(self.width, self.height, &self.rgb, dpi)
    }

    /// The bounding box of everything that differs from `background`, grown
    /// by `pad` pixels (`exportgraphics` of one axes).
    pub fn crop_to_ink(&self, background: Rgb, pad: usize) -> Image {
        let Some([x0, y0, w, h]) = self.ink_box(background, pad) else {
            return self.clone();
        };
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in y0..y0 + h {
            rgb.extend_from_slice(
                &self.rgb[(y * self.width + x0) * 3..(y * self.width + x0 + w) * 3],
            );
        }
        Image {
            width: w,
            height: h,
            rgb,
        }
    }

    /// `[x, y, width, height]` in pixels of what differs from `background`,
    /// with `pad` around it; `None` when nothing does.
    pub fn ink_box(&self, background: Rgb, pad: usize) -> Option<[usize; 4]> {
        let bg = background.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as i32);
        let (mut x0, mut y0, mut x1, mut y1) = (self.width, self.height, 0usize, 0usize);
        for y in 0..self.height {
            for x in 0..self.width {
                let i = (y * self.width + x) * 3;
                let near = (0..3).all(|k| (self.rgb[i + k] as i32 - bg[k]).abs() <= 2);
                if !near {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x + 1);
                    y1 = y1.max(y + 1);
                }
            }
        }
        if x0 >= x1 || y0 >= y1 {
            return None;
        }
        let (x0, y0) = (x0.saturating_sub(pad), y0.saturating_sub(pad));
        let (x1, y1) = ((x1 + pad).min(self.width), (y1 + pad).min(self.height));
        Some([x0, y0, x1 - x0, y1 - y0])
    }
}

fn paint(dev: &mut dyn Device, record: &Json, o: &Options) {
    let empty: &[Json] = &[];
    let axes = record.get("axes").and_then(Json::as_array).unwrap_or(empty);
    let size = record
        .get("source")
        .map(|s| nums(s, "figure_size"))
        .unwrap_or_default();
    let figure_size =
        (size.len() == 2 && size[0] > 0.0 && size[1] > 0.0).then(|| [size[0], size[1]]);
    let viewport = fit_viewport(figure_size, o.width, o.height);
    let version = num(record, "version").unwrap_or(2.0) as i64;
    let mut fig = Figure {
        dev,
        axes,
        theme: o.theme,
        width: o.width,
        height: o.height,
        viewport,
        figure_size,
        version,
    };
    if axes.is_empty() {
        // Nothing to draw. A figure with no axes is blank; a figure the
        // serializer still refuses says why instead of looking empty.
        let code = text(record, "reason_code");
        if !code.is_empty() && code != "no_axes" {
            let note = format!("The Rust engine could not draw this figure: {code}");
            label(
                fig.dev,
                &note,
                "none",
                o.width / 2.0,
                o.height / 2.0,
                "center",
                TextStyle::sans(11.0),
                o.theme.text,
                Affine::IDENTITY,
            );
        }
        return;
    }
    let is_3d = axes.iter().any(|a| num(a, "dimension") == Some(3.0));
    if is_3d {
        fig.draw_scenes();
        for (i, a) in axes.iter().enumerate() {
            if num(a, "dimension") == Some(3.0) {
                fig.draw_axes_overlay(i);
            } else {
                // The 2-D painter, whose version is 3 in this path; the 3-D
                // viewer then draws the colorbars once more on top.
                fig.version = 3;
                fig.draw_axis(i);
            }
            fig.draw_colorbars(a);
        }
    } else {
        for i in 0..axes.len() {
            fig.draw_axis(i);
        }
    }
}

/// Renders to pixels. Never panics on a malformed record: what cannot be read
/// is left out.
pub fn render(record: &Json, options: &Options) -> Image {
    let (w, h) = options.device_size();
    let dpr = w as f64 / options.width.max(1e-9);
    let mut r = raster::Raster::new(w, h, dpr, options.theme.background);
    paint(&mut r, record, options);
    Image {
        width: w,
        height: h,
        rgb: std::mem::take(&mut r.rgb),
    }
}

/// Renders to an SVG document.
pub fn render_svg(record: &Json, options: &Options) -> String {
    render_svg_region(record, options, None)
}

/// Renders to an SVG document showing `region` (`[x, y, width, height]` in
/// figure pixels) of the figure, or all of it.
pub fn render_svg_region(record: &Json, options: &Options, region: Option<[f64; 4]>) -> String {
    let mut d = vector::Svg::new(options.width, options.height);
    d.show_region(region);
    d.fill_rect(
        0.0,
        0.0,
        options.width,
        options.height,
        options.theme.background,
        1.0,
    );
    paint(&mut d, record, options);
    d.finish()
}

/// Renders to a one-page PDF.
pub fn render_pdf(record: &Json, options: &Options) -> Vec<u8> {
    render_pdf_region(record, options, None)
}

/// Renders to a one-page PDF whose page is `region` (as in
/// [`render_svg_region`]) of the figure, or all of it.
pub fn render_pdf_region(record: &Json, options: &Options, region: Option<[f64; 4]>) -> Vec<u8> {
    let mut d = vector::Pdf::new(options.width, options.height);
    d.show_region(region);
    d.fill_rect(
        0.0,
        0.0,
        options.width,
        options.height,
        options.theme.background,
        1.0,
    );
    paint(&mut d, record, options);
    d.finish()
}

/// A one-page PDF holding a rendered image.
pub fn pdf_of_image(image: &Image, page: &Options) -> Vec<u8> {
    let mut d = vector::Pdf::new(page.width, page.height);
    let bitmap = device::Bitmap {
        width: image.width,
        height: image.height,
        rgb: image.rgb.clone(),
    };
    d.image(&bitmap, 0.0, 0.0, page.width, page.height);
    d.finish()
}

/// A record reduced to one of its axes (with its peers on the same box, such
/// as a `yyaxis` partner), for `exportgraphics(ax, ...)`.
pub fn only_axes(record: &Json, index: usize) -> Json {
    let mut out = record.clone();
    if let Some(list) = record.get("axes").and_then(Json::as_array) {
        let Some(chosen) = list.get(index) else {
            return out;
        };
        let position = nums(chosen, "position");
        // Only the right-hand axes of a `yyaxis` pair carries the mark.
        let pair = !text(chosen, "yaxis_location").is_empty();
        let keep: Vec<Json> = list
            .iter()
            .enumerate()
            .filter(|(i, a)| {
                *i == index
                    || ((pair || !text(a, "yaxis_location").is_empty())
                        && nums(a, "position") == position)
            })
            .map(|(_, a)| a.clone())
            .collect();
        out.set("axes", Json::Arr(keep));
    }
    out
}
