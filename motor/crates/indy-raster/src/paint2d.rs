//! Port of the UI's 2-D painter (`InteractiveFigure` in `frontend/figures.js`,
//! `figure_viewport_utils`, `figure_image_utils`, `figure_shape_utils`), at the
//! exported limits. Differences from the UI are deliberate and additive:
//! marker sizes scale with `marker_size`/`sizes` (the default size draws what
//! the UI draws), the legend honours its location, and records only the
//! renderer's relaxed export produces are drawn (see `Graphics::raster_record`).

use crate::color::{AxesColors, Theme, adjust, clamp01, contour_display_colors};
use crate::device::{Bitmap, Device, Path, Rgb};
use crate::font::{self, Face, Place};
use crate::rec::{Desc, dash, desc, flag, num, nums, plot_format, rgb, series, strings, text};
use crate::tex::{self, Run};
use indy_graphics::json::Json;

#[derive(Debug, Clone, Copy, Default)]
pub struct Box2 {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct Affine {
    pub tx: f64,
    pub ty: f64,
    pub cos: f64,
    pub sin: f64,
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        tx: 0.0,
        ty: 0.0,
        cos: 1.0,
        sin: 0.0,
    };

    /// Translate, then rotate by `angle` radians clockwise on screen.
    pub fn at(tx: f64, ty: f64, angle: f64) -> Affine {
        Affine {
            tx,
            ty,
            cos: angle.cos(),
            sin: angle.sin(),
        }
    }

    fn apply(&self, x: f64, y: f64) -> [f64; 2] {
        [
            self.tx + x * self.cos - y * self.sin,
            self.ty + x * self.sin + y * self.cos,
        ]
    }
}

/// Text style of one `fillText` family.
#[derive(Debug, Clone, Copy, Default)]
pub struct TextStyle {
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    pub mono: bool,
}

impl TextStyle {
    pub fn sans(size: f64) -> TextStyle {
        TextStyle {
            size,
            ..TextStyle::default()
        }
    }

    pub fn mono(size: f64) -> TextStyle {
        TextStyle {
            size,
            mono: true,
            ..TextStyle::default()
        }
    }

    pub fn face(&self, run_bold: bool) -> Face {
        if self.mono {
            Face::Mono
        } else if self.bold || run_bold {
            Face::SansBold
        } else {
            Face::Sans
        }
    }
}

pub fn measure_runs(runs: &[Run], st: TextStyle) -> f64 {
    runs.iter()
        .map(|r| {
            font::measure(
                st.face(r.bold),
                &r.text,
                if r.script != 0 {
                    st.size * 0.7
                } else {
                    st.size
                },
            )
        })
        .sum()
}

/// Draws styled runs from the left edge `x` on the baseline `y`. As in the
/// browser's text pipeline, axis-aligned baselines snap to device pixels and
/// glyph positions to quarter pixels.
pub fn draw_runs(
    dev: &mut dyn Device,
    runs: &[Run],
    x: f64,
    y: f64,
    st: TextStyle,
    color: Rgb,
    xf: Affine,
) {
    let dpr = dev.dpr();
    let horizontal = xf.sin.abs() < 1e-9;
    let vertical = xf.cos.abs() < 1e-9;
    let mut pen = x;
    let mut path = Path::new();
    for run in runs {
        let size = if run.script != 0 {
            st.size * 0.7
        } else {
            st.size
        };
        let face = st.face(run.bold);
        let oblique = !st.mono && (st.italic || run.italic);
        let shift = if run.script > 0 {
            -0.38 * st.size
        } else if run.script < 0 {
            0.18 * st.size
        } else {
            0.0
        };
        for c in run.text.chars() {
            let [mut gx, mut gy] = xf.apply(pen, y + shift);
            if horizontal {
                gy = (gy * dpr).round() / dpr;
                gx = (gx * dpr * 4.0).round() / (dpr * 4.0);
            } else if vertical {
                gx = (gx * dpr).round() / dpr;
                gy = (gy * dpr * 4.0).round() / (dpr * 4.0);
            }
            let place = Place {
                x: gx,
                y: gy,
                cos: xf.cos,
                sin: xf.sin,
                size,
                oblique,
            };
            font::glyph(&mut path, face, c, &place, dpr);
            pen += font::advance(face, c) * size;
        }
    }
    if !path.is_empty() {
        dev.fill_text(&path, st.size, color);
    }
}

/// Canvas `textAlign` plus `fillText` of a parsed string.
pub fn label(
    dev: &mut dyn Device,
    source: &str,
    interpreter: &str,
    x: f64,
    y: f64,
    align: &str,
    st: TextStyle,
    color: Rgb,
    xf: Affine,
) -> f64 {
    let runs = tex::parse(source, if interpreter == "none" { "none" } else { "tex" });
    let width = measure_runs(&runs, st);
    let left = match align {
        "center" => x - width / 2.0,
        "right" => x - width,
        _ => x,
    };
    draw_runs(dev, &runs, left, y, st, color, xf);
    width
}

/// `fitFigureViewport`.
pub fn fit_viewport(figure: Option<[f64; 2]>, width: f64, height: f64) -> Box2 {
    match figure {
        Some([fw, fh]) if fw > 0.0 && fh > 0.0 => {
            let scale = (width / fw).min(height / fh);
            let (w, h) = (fw * scale, fh * scale);
            Box2 {
                x: (width - w) / 2.0,
                y: (height - h) / 2.0,
                w,
                h,
            }
        }
        _ => Box2 {
            x: 0.0,
            y: 0.0,
            w: width,
            h: height,
        },
    }
}

/// `fitAxes`.
pub fn fit_axes(position: &[f64], viewport: Box2, reserve: f64) -> Box2 {
    if position.len() < 4 {
        return Box2 {
            x: viewport.x,
            y: viewport.y,
            w: viewport.w,
            h: viewport.h,
        };
    }
    let scale = 1.0 - reserve.clamp(0.0, 0.75);
    let (p1, p3) = (position[1] * scale, position[3] * scale);
    Box2 {
        x: viewport.x + position[0] * viewport.w,
        y: viewport.y + (1.0 - p1 - p3) * viewport.h,
        w: position[2] * viewport.w,
        h: p3 * viewport.h,
    }
}

/// `fitFigureAxes`: shared-title space comes from the whole figure.
pub fn fit_figure_axes(axes: &Json, viewport: Box2, all: &[Json]) -> Box2 {
    let position = nums(axes, "position");
    let base = nums(axes, "title_layout_position");
    if base.len() != 4 || position.len() != 4 {
        return fit_axes(&position, viewport, 0.0);
    }
    let source_reserve = (1.0 - position[3] / base[3]).max(0.0);
    let title = all
        .iter()
        .flat_map(series)
        .find(|s| flag(s, "figure_title"));
    let text_height = title.map_or(0.0, |t| {
        let lines = strings(t, "lines").len().max(1) as f64;
        num(t, "font_size").unwrap_or(10.0) * 1.2 * lines
            + num(t, "margin").unwrap_or(0.0) * 2.0
            + 36.0
    });
    let view_reserve = if viewport.h > 0.0 {
        text_height / viewport.h
    } else {
        0.0
    };
    fit_axes(&base, viewport, source_reserve.max(view_reserve))
}

/// `images.aspectBox`.
fn aspect_box(b: Box2, axes: &Json, xlim: [f64; 2], ylim: [f64; 2]) -> Box2 {
    let dar = nums(axes, "data_aspect_ratio");
    let pbar = nums(axes, "plot_box_aspect_ratio");
    let mut ratio = None;
    if text(axes, "data_aspect_ratio_mode") == "manual" && dar.len() >= 2 {
        ratio = Some(((xlim[1] - xlim[0]) / dar[0]) / ((ylim[1] - ylim[0]) / dar[1]));
    } else if text(axes, "plot_box_aspect_ratio_mode") == "manual" && pbar.len() >= 2 {
        ratio = Some(pbar[0] / pbar[1]);
    }
    match ratio {
        Some(r) if r > 0.0 && r.is_finite() => {
            let (w, h) = (b.w.min(b.h * r), b.h.min(b.w / r));
            Box2 {
                x: b.x + (b.w - w) / 2.0,
                y: b.y + (b.h - h) / 2.0,
                w,
                h,
            }
        }
        _ => b,
    }
}

pub fn is_image(s: &Json) -> bool {
    text(s, "kind") == "image"
}

/// One axes' mapping from data to pixels.
pub struct View {
    pub b: Box2,
    pub lim: [[f64; 2]; 2],
    pub log: [bool; 2],
    pub rev: [bool; 2],
}

impl View {
    pub fn of(axes: &Json, b: Box2) -> View {
        let pair = |key: &str| {
            let v = nums(axes, key);
            if v.len() == 2 {
                [v[0], v[1]]
            } else {
                [0.0, 1.0]
            }
        };
        let lim = [pair("xlim"), pair("ylim")];
        View {
            b,
            lim,
            log: [text(axes, "xscale") == "log", text(axes, "yscale") == "log"],
            rev: [
                text(axes, "xdir") == "reverse",
                text(axes, "ydir") == "reverse",
            ],
        }
    }

    fn trans(v: f64, log: bool) -> Option<f64> {
        if !v.is_finite() {
            return None;
        }
        if log {
            return (v > 0.0).then(|| v.log10());
        }
        Some(v)
    }

    pub fn pixel(&self, value: f64, axis: usize) -> Option<f64> {
        let log = self.log[axis];
        let n = Self::trans(value, log)?;
        let lo = Self::trans(self.lim[axis][0], log)?;
        let hi = Self::trans(self.lim[axis][1], log)?;
        if lo == hi {
            return None;
        }
        let mut ratio = (n - lo) / (hi - lo);
        if self.rev[axis] {
            ratio = 1.0 - ratio;
        }
        Some(if axis == 0 {
            self.b.x + ratio * self.b.w
        } else {
            self.b.y + (1.0 - ratio) * self.b.h
        })
    }
}

/// What a figure paints with.
pub struct Figure<'a> {
    pub dev: &'a mut dyn Device,
    pub axes: &'a [Json],
    pub theme: Theme,
    pub width: f64,
    pub height: f64,
    pub viewport: Box2,
    pub figure_size: Option<[f64; 2]>,
    pub version: i64,
}

impl Figure<'_> {
    fn plot_color(&self, c: Rgb) -> Rgb {
        adjust(c, &self.theme)
    }

    pub fn line_width_px(&self, points: Option<f64>) -> f64 {
        let scale = match self.figure_size {
            Some([fw, _]) if fw > 0.0 && self.viewport.w > 0.0 => self.viewport.w / fw,
            _ => 1.0,
        };
        let pt = points.filter(|p| p.is_finite() && *p > 0.0).unwrap_or(0.5);
        (1.0 / self.dev.dpr()).max(pt * 96.0 / 72.0 * scale)
    }

    pub fn axes_box(&self, index: usize) -> Box2 {
        let a = &self.axes[index];
        let fitted = fit_figure_axes(a, self.viewport, self.axes);
        if series(a).iter().any(is_image) {
            let v = View::of(a, fitted);
            aspect_box(fitted, a, v.lim[0], v.lim[1])
        } else {
            fitted
        }
    }

    fn stroke_rect(&mut self, b: Box2, width: f64, dashes: &[f64], color: Rgb, alpha: f64) {
        let mut p = Path::new();
        p.rect(b.x, b.y, b.w, b.h);
        self.dev.stroke(&p, width, dashes, color, alpha);
    }

    /// `drawAxis`.
    pub fn draw_axis(&mut self, index: usize) {
        let a = &self.axes[index];
        let b = self.axes_box(index);
        let view = View::of(a, b);
        let visible = a.get("visible").and_then(Json::as_bool).unwrap_or(true);
        let interpreters = a.get("interpreters").cloned().unwrap_or(Json::obj());
        let ticks_interp = text(&interpreters, "ticks").to_string();
        let colors = AxesColors::of(a);
        let contours = contour_display_colors(a, &colors, &self.theme);
        let p = self.theme;
        let y_right = text(a, "yaxis_location") == "right";
        if visible {
            if !flag(a, "axes_color_none") {
                let bg = rgb(a, "axes_color").unwrap_or(p.background);
                self.dev.fill_rect(b.x, b.y, b.w, b.h, bg, 1.0);
            }
            let mono = TextStyle::mono(10.0);
            let grid = a.get("grid").cloned().unwrap_or(Json::obj());
            let mut edge = f64::NEG_INFINITY;
            for (value, tick_label) in axis_ticks(a, "x", view.lim[0]) {
                let Some(px) = view.pixel(value, 0) else {
                    continue;
                };
                if flag(&grid, "x") {
                    let mut l = Path::new();
                    l.move_to(px, b.y);
                    l.line_to(px, b.y + b.h);
                    self.dev.stroke(&l, 1.0, &[], p.grid, 1.0);
                }
                let s = tick_label.unwrap_or_else(|| plot_format(value));
                let width = measure_runs(&tex::parse(&s, &ticks_interp), mono);
                if px - width / 2.0 < edge + 4.0 {
                    continue;
                }
                label(
                    self.dev,
                    &s,
                    &ticks_interp,
                    px,
                    b.y + b.h + 13.0,
                    "center",
                    mono,
                    p.text,
                    Affine::IDENTITY,
                );
                edge = px + width / 2.0;
            }
            let mut last_y: Option<f64> = None;
            for (value, tick_label) in axis_ticks(a, "y", view.lim[1]) {
                let Some(py) = view.pixel(value, 1) else {
                    continue;
                };
                if flag(&grid, "y") {
                    let mut l = Path::new();
                    l.move_to(b.x, py);
                    l.line_to(b.x + b.w, py);
                    self.dev.stroke(&l, 1.0, &[], p.grid, 1.0);
                }
                if last_y.is_some_and(|e| (py - e).abs() < 11.0) {
                    continue;
                }
                let s = tick_label.unwrap_or_else(|| plot_format(value));
                if y_right {
                    label(
                        self.dev,
                        &s,
                        &ticks_interp,
                        b.x + b.w + 4.0,
                        py + 3.0,
                        "left",
                        mono,
                        p.text,
                        Affine::IDENTITY,
                    );
                } else {
                    label(
                        self.dev,
                        &s,
                        &ticks_interp,
                        b.x - 4.0,
                        py + 3.0,
                        "right",
                        mono,
                        p.text,
                        Affine::IDENTITY,
                    );
                }
                last_y = Some(py);
            }
            self.stroke_rect(b, 1.0, &[], p.axis, 1.0);
        }
        // Children in stacking order, then text in non-data units.
        let list = series(a);
        for pass in [false, true] {
            for (j, s) in list.iter().enumerate() {
                let kind = text(s, "kind");
                let late = kind == "text" && text(s, "units") != "data";
                if late != pass {
                    continue;
                }
                if kind != "text" || flag(s, "clipping") {
                    self.dev.clip(Some([b.x, b.y, b.w, b.h]));
                }
                match kind {
                    "image" => self.draw_image(s, &colors, &view),
                    "patch2d" => self.draw_patch(s, &view),
                    "patch_raster" => self.draw_raster_patch(s, &view),
                    "text" => self.draw_text(s, a, j, &view),
                    _ => self.draw_series(s, contours.get(j).copied().flatten(), &view),
                }
                self.dev.clip(None);
            }
        }
        if !text(a, "title").is_empty() {
            label(
                self.dev,
                text(a, "title"),
                text(&interpreters, "title"),
                b.x + b.w / 2.0,
                (b.y - 8.0).max(12.0),
                "center",
                TextStyle {
                    size: 12.0,
                    bold: true,
                    ..TextStyle::default()
                },
                p.text,
                Affine::IDENTITY,
            );
        }
        if !text(a, "xlabel").is_empty() {
            label(
                self.dev,
                text(a, "xlabel"),
                text(&interpreters, "xlabel"),
                b.x + b.w / 2.0,
                (b.y + b.h + 27.0).min(self.height - 2.0),
                "center",
                TextStyle::sans(11.0),
                p.text,
                Affine::IDENTITY,
            );
        }
        if !text(a, "ylabel").is_empty() {
            let x = if y_right {
                (b.x + b.w + 34.0).min(self.width - 3.0)
            } else {
                (b.x - 34.0).max(9.0)
            };
            let xf = Affine::at(x, b.y + b.h / 2.0, -std::f64::consts::FRAC_PI_2);
            label(
                self.dev,
                text(a, "ylabel"),
                text(&interpreters, "ylabel"),
                0.0,
                0.0,
                "center",
                TextStyle::sans(11.0),
                p.text,
                xf,
            );
        }
        self.draw_legend(a, b, &contours);
        if self.version == 3 {
            self.draw_colorbars(a);
        }
    }

    fn draw_image(&mut self, s: &Json, colors: &AxesColors, view: &View) {
        let Some(bitmap) = image_bitmap(s, colors) else {
            return;
        };
        let (cx, cy) = (nums(s, "x"), nums(s, "y"));
        if cx.len() != 2 || cy.len() != 2 {
            return;
        }
        let (bx, by) = (
            image_bounds(&cx, bitmap.width),
            image_bounds(&cy, bitmap.height),
        );
        let (Some(x0), Some(x1), Some(y0), Some(y1)) = (
            view.pixel(bx[0], 0),
            view.pixel(bx[1], 0),
            view.pixel(by[0], 1),
            view.pixel(by[1], 1),
        ) else {
            return;
        };
        self.dev.image(&bitmap, x0, y0, x1, y1);
    }

    /// `drawPatch`.
    fn draw_patch(&mut self, s: &Json, view: &View) {
        let face = rgb(s, "face_color");
        let style = text(s, "line_style");
        let edge = if style == "none" {
            None
        } else {
            rgb(s, "edge_color").map(|c| self.plot_color(c))
        };
        let (fa, ea) = (alpha_value(s, "face_alpha"), alpha_value(s, "edge_alpha"));
        let width = self.line_width_px(num(s, "line_width"));
        let dashes = dash(style);
        let (Some(v), Some(f)) = (desc(s, "vertices"), desc(s, "faces")) else {
            return;
        };
        for polygon in face_polygons(&v, &f, 2) {
            let mut pts = Vec::with_capacity(polygon.len());
            for q in &polygon {
                let (Some(px), Some(py)) = (view.pixel(q[0], 0), view.pixel(q[1], 1)) else {
                    break;
                };
                pts.push([px, py]);
            }
            if pts.len() != polygon.len() {
                continue;
            }
            let mut path = Path::new();
            path.polygon(&pts);
            if let Some(c) = face
                && pts.len() > 2
            {
                self.dev.fill(&path, true, clamp01(c), fa);
            }
            if let Some(c) = edge {
                self.dev.stroke(&path, width, dashes, c, ea);
            }
        }
    }

    /// Patches the UI's contract cannot carry: per-face or per-vertex colours,
    /// and any patch in a log or fixed-aspect axes.
    fn draw_raster_patch(&mut self, s: &Json, view: &View) {
        let (Some(v), Some(f)) = (desc(s, "vertices"), desc(s, "faces")) else {
            return;
        };
        let (fa, ea) = (alpha_value(s, "face_alpha"), alpha_value(s, "edge_alpha"));
        let style = text(s, "line_style");
        let width = self.line_width_px(num(s, "line_width"));
        let face_paint = PatchPaint::of(s.get("face"));
        let edge_paint = PatchPaint::of(s.get("edge"));
        let rows = f.shape.first().copied().unwrap_or(0);
        let cols = f.shape.get(1).copied().unwrap_or(0);
        for r in 0..rows {
            let mut idx = Vec::with_capacity(cols);
            for c in 0..cols {
                let k = f.at(r + c * rows);
                if k.is_nan() {
                    break;
                }
                idx.push(k as usize);
            }
            let n = v.shape.first().copied().unwrap_or(0);
            let mut pts = Vec::with_capacity(idx.len());
            for &i in &idx {
                let (Some(px), Some(py)) = (view.pixel(v.at(i), 0), view.pixel(v.at(i + n), 1))
                else {
                    break;
                };
                pts.push([px, py]);
            }
            if pts.len() != idx.len() || pts.len() < 2 {
                continue;
            }
            let mut path = Path::new();
            path.polygon(&pts);
            if pts.len() > 2 && face_paint.colored(r, &idx) {
                if face_paint.interp() {
                    let colors: Vec<Rgb> = idx.iter().map(|&i| face_paint.vertex(i)).collect();
                    for k in 1..pts.len() - 1 {
                        self.dev.gouraud(
                            &[pts[0], pts[k], pts[k + 1]],
                            &[colors[0], colors[k], colors[k + 1]],
                            fa,
                        );
                    }
                } else if let Some(c) = face_paint.face(r, idx[0]) {
                    self.dev.fill(&path, true, clamp01(c), fa);
                }
            }
            if style != "none" && edge_paint.colored(r, &idx) {
                if edge_paint.interp() {
                    for k in 0..pts.len() {
                        let c = edge_paint.vertex(idx[k]);
                        let mut seg = Path::new();
                        seg.move_to(pts[k][0], pts[k][1]);
                        seg.line_to(pts[(k + 1) % pts.len()][0], pts[(k + 1) % pts.len()][1]);
                        self.dev
                            .stroke(&seg, width, dash(style), self.plot_color(c), ea);
                    }
                } else if let Some(c) = edge_paint.face(r, idx[0]) {
                    let c = self.plot_color(c);
                    self.dev.stroke(&path, width, dash(style), c, ea);
                }
            }
        }
    }

    /// `drawText`.
    fn draw_text(&mut self, s: &Json, axes: &Json, j: usize, view: &View) {
        let b = view.b;
        let position = nums(s, "position");
        if position.len() < 2 {
            return;
        }
        let units = text(s, "units");
        let (x, y);
        if units == "data" {
            let (Some(px), Some(py)) = (view.pixel(position[0], 0), view.pixel(position[1], 1))
            else {
                return;
            };
            let inside = px >= b.x - 0.5
                && px <= b.x + b.w + 0.5
                && py >= b.y - 0.5
                && py <= b.y + b.h + 0.5;
            let within = |v: f64, r: [f64; 2]| v >= r[0].min(r[1]) && v <= r[0].max(r[1]);
            let margin = !within(position[0], view.lim[0]) || !within(position[1], view.lim[1]);
            if !inside && !(margin && !flag(s, "clipping")) {
                return;
            }
            (x, y) = (px, py);
        } else if units == "normalized" {
            x = b.x + position[0] * b.w;
            let ny = b.y + (1.0 - position[1]) * b.h;
            y = if flag(s, "figure_title") {
                ny.max(6.0)
            } else {
                ny
            };
        } else {
            // Relaxed export: physical units from the axes' lower-left corner.
            let unit = match units {
                "points" => 96.0 / 72.0,
                "inches" => 96.0,
                "centimeters" => 96.0 / 2.54,
                _ => 1.0,
            };
            x = b.x + position[0] * unit;
            y = b.y + b.h - position[1] * unit;
        }
        self.text_at(s, axes, j, x, y);
    }

    /// The anchored part of `drawText`, shared with text in 3-D axes.
    pub fn text_at(&mut self, s: &Json, axes: &Json, j: usize, x: f64, y: f64) {
        let size = num(s, "font_size").unwrap_or(10.0);
        let st = TextStyle {
            size,
            bold: text(s, "font_weight") == "bold",
            italic: text(s, "font_angle") == "italic",
            mono: false,
        };
        let interp = if text(s, "interpreter") == "none" {
            "none"
        } else {
            "tex"
        };
        let lines: Vec<Vec<Run>> = strings(s, "lines")
            .iter()
            .map(|l| tex::parse(l, interp))
            .collect();
        if !lines.iter().any(|r| !r.is_empty()) {
            return;
        }
        let widths: Vec<f64> = lines.iter().map(|r| measure_runs(r, st)).collect();
        let (halign, valign) = (
            text(s, "horizontal_alignment"),
            text(s, "vertical_alignment"),
        );
        let margin = num(s, "margin").unwrap_or(0.0);
        let layout = layout_text(
            &widths,
            size * 1.2,
            size * 0.8,
            size * 0.2,
            halign,
            valign,
            margin,
        );
        let rotation = num(s, "rotation").unwrap_or(0.0);
        let xf = Affine::at(x, y, -rotation.to_radians());
        let list = series(axes);
        let frame = if text(s, "role") == "textbox" && j > 0 {
            list.get(j - 1)
        } else {
            None
        };
        let background = rgb(s, "background_color");
        let framed = frame.is_some_and(|f| {
            text(f, "kind") == "patch2d"
                && text(f, "role") == "textbox"
                && rgb(f, "face_color").is_some()
        });
        let boxed = background.is_some() || framed;
        let corners = |bx: f64, by: f64, bw: f64, bh: f64| -> Vec<[f64; 2]> {
            vec![
                xf.apply(bx, by),
                xf.apply(bx + bw, by),
                xf.apply(bx + bw, by + bh),
                xf.apply(bx, by + bh),
            ]
        };
        let [bx, by, bw, bh] = layout.box_rect;
        if let Some(c) = background {
            let mut p = Path::new();
            p.polygon(&corners(bx, by, bw, bh));
            self.dev.fill(&p, false, clamp01(c), 1.0);
        }
        let style = text(s, "line_style");
        if let Some(c) = rgb(s, "edge_color")
            && style != "none"
        {
            let mut p = Path::new();
            p.polygon(&corners(bx, by, bw, bh));
            let width = self.line_width_px(num(s, "line_width"));
            let ink = self.plot_color(c);
            self.dev.stroke(&p, width, dash(style), ink, 1.0);
        }
        let color = rgb(s, "color").unwrap_or(self.theme.text);
        let ink = if boxed {
            clamp01(color)
        } else {
            self.plot_color(color)
        };
        for (runs, (lx, baseline)) in lines.iter().zip(&layout.lines) {
            draw_runs(self.dev, runs, *lx, *baseline, st, ink, xf);
        }
    }

    /// `seriesStyle` for line-like records: (line, edge, face, face α, edge α).
    fn series_style(
        &self,
        s: &Json,
        contour: Option<Rgb>,
    ) -> (Option<Rgb>, Option<Rgb>, Option<Rgb>, f64, f64) {
        let kind = text(s, "kind");
        let line = if kind != "scatter" && text(s, "line_style") != "none" {
            contour.or_else(|| rgb(s, "line_color").map(|c| self.plot_color(c)))
        } else {
            None
        };
        let edge = rgb(s, "marker_edge_color").map(|c| self.plot_color(c));
        let face = if flag(s, "marker_face_auto") {
            Some(self.theme.background)
        } else {
            rgb(s, "marker_face_color").map(|c| self.plot_color(c))
        };
        (
            line,
            edge,
            face,
            alpha_value(s, "marker_face_alpha"),
            alpha_value(s, "marker_edge_alpha"),
        )
    }

    /// `drawSeries`.
    fn draw_series(&mut self, s: &Json, contour: Option<Rgb>, view: &View) {
        let (line, edge, face, fa, ea) = self.series_style(s, contour);
        let kind = text(s, "kind");
        let (xs, ys) = (nums(s, "x"), nums(s, "y"));
        let n = xs.len().min(ys.len());
        let points: Vec<Option<[f64; 2]>> = (0..n)
            .map(|i| match (view.pixel(xs[i], 0), view.pixel(ys[i], 1)) {
                (Some(x), Some(y)) => Some([x, y]),
                _ => None,
            })
            .collect();
        let width = self.line_width_px(num(s, "line_width"));
        let dashes = dash(text(s, "line_style"));
        let b = view.b;
        let span = text(s, "span");
        if let Some(c) = line
            && !span.is_empty()
        {
            let across = span == "horizontal";
            if let Some(Some(p0)) = points.first() {
                let at = if across { p0[1] } else { p0[0] };
                let mut p = Path::new();
                if across {
                    p.move_to(b.x, at);
                    p.line_to(b.x + b.w, at);
                } else {
                    p.move_to(at, b.y);
                    p.line_to(at, b.y + b.h);
                }
                self.dev.stroke(&p, width, dashes, c, 1.0);
            }
            return;
        }
        if let Some(c) = line {
            if kind == "stem" {
                if let Some(base) = view.pixel(num(s, "base_value").unwrap_or(0.0), 1) {
                    for p in points.iter().flatten() {
                        let mut path = Path::new();
                        path.move_to(p[0], base);
                        path.line_to(p[0], p[1]);
                        self.dev.stroke(&path, width, dashes, c, 1.0);
                    }
                }
            } else {
                let mut path = Path::new();
                let mut previous: Option<[f64; 2]> = None;
                for p in &points {
                    let Some(p) = p else {
                        previous = None;
                        continue;
                    };
                    match previous {
                        None => path.move_to(p[0], p[1]),
                        Some(q) if kind == "stairs" => {
                            path.line_to(p[0], q[1]);
                            path.line_to(p[0], p[1]);
                        }
                        Some(_) => path.line_to(p[0], p[1]),
                    }
                    previous = Some(*p);
                }
                self.dev.stroke(&path, width, dashes, c, 1.0);
            }
        }
        let marker = text(s, "marker");
        if marker.is_empty() || marker == "none" {
            return;
        }
        let sizes = nums(s, "sizes");
        let base = num(s, "marker_size").unwrap_or(6.0);
        let point_colors = desc(s, "point_colors");
        let (per_edge, per_face) = (flag(s, "point_edge"), flag(s, "point_face"));
        for (i, p) in points.iter().enumerate() {
            let Some(p) = p else { continue };
            let k = if text(s, "kind") == "scatter" {
                let area = if sizes.len() == 1 {
                    sizes[0]
                } else {
                    sizes.get(i).copied().unwrap_or(36.0)
                };
                area.max(0.0).sqrt() / 6.0
            } else {
                base.max(0.0) / 6.0
            };
            let (mut e, mut f) = (edge, face);
            if let Some(pc) = &point_colors {
                let n = pc.shape.first().copied().unwrap_or(0);
                let c = [pc.at(i), pc.at(i + n), pc.at(i + 2 * n)];
                if c.iter().all(|v| v.is_finite()) {
                    if per_edge {
                        e = Some(self.plot_color(c));
                    }
                    if per_face {
                        f = Some(self.plot_color(c));
                    }
                }
            }
            self.marker(p[0], p[1], marker, e, f, fa, ea, k, width);
        }
    }

    /// `marker`, plus the MATLAB shapes the UI lacks. `k` scales the UI's
    /// default-size radii.
    pub fn marker(
        &mut self,
        x: f64,
        y: f64,
        kind: &str,
        edge: Option<Rgb>,
        face: Option<Rgb>,
        fa: f64,
        ea: f64,
        k: f64,
        width: f64,
    ) -> bool {
        if kind.is_empty() || kind == "none" || (edge.is_none() && face.is_none()) {
            return false;
        }
        let open = matches!(kind, "+" | "x" | "*" | ".");
        if open && edge.is_none() {
            return false;
        }
        // MATLAB's point is a third of the marker size; the UI's is 2 px.
        let r = if kind == "." {
            (k * 6.0 * 0.222).max(2.0)
        } else {
            3.5 * k
        };
        let dpr = self.dev.dpr();
        let mut path = Path::new();
        match kind {
            "+" | "x" | "*" => {
                if kind != "x" {
                    path.move_to(x - r, y);
                    path.line_to(x + r, y);
                    path.move_to(x, y - r);
                    path.line_to(x, y + r);
                }
                if kind != "+" {
                    path.move_to(x - r, y - r);
                    path.line_to(x + r, y + r);
                    path.move_to(x - r, y + r);
                    path.line_to(x + r, y - r);
                }
            }
            "s" | "square" => path.rect(x - r, y - r, r * 2.0, r * 2.0),
            "^" | "v" | ">" | "<" => {
                let angle = match kind {
                    "^" => -std::f64::consts::FRAC_PI_2,
                    "v" => std::f64::consts::FRAC_PI_2,
                    ">" => 0.0,
                    _ => std::f64::consts::PI,
                };
                let pts: Vec<[f64; 2]> = (0..3)
                    .map(|i| {
                        let a = angle + i as f64 * std::f64::consts::TAU / 3.0;
                        [x + a.cos() * r * 1.3, y + a.sin() * r * 1.3]
                    })
                    .collect();
                path.polygon(&pts);
            }
            "d" | "diamond" => path.polygon(&[
                [x, y - r * 1.25],
                [x + r * 0.85, y],
                [x, y + r * 1.25],
                [x - r * 0.85, y],
            ]),
            "p" | "pentagram" => path.polygon(&star(x, y, r * 1.35, 5, 0.382)),
            "h" | "hexagram" => path.polygon(&star(x, y, r * 1.3, 6, 0.577)),
            _ => path.circle(x, y, r, dpr),
        }
        if kind == "." {
            if let Some(c) = edge {
                self.dev.fill(&path, false, c, ea);
            }
            return true;
        }
        if !open && let Some(c) = face {
            self.dev.fill(&path, false, c, fa);
        }
        if let Some(c) = edge {
            self.dev.stroke(&path, width, &[], c, ea);
        }
        true
    }

    /// `drawLegend`.
    fn draw_legend(&mut self, a: &Json, b: Box2, contours: &[Option<Rgb>]) {
        let Some(legend) = a.get("legend") else {
            return;
        };
        if !flag(legend, "visible") {
            return;
        }
        let labels = strings(legend, "labels");
        let list = series(a);
        struct Item {
            label: String,
            index: usize,
        }
        let mut items = Vec::new();
        for (index, s) in list.iter().enumerate() {
            let kind = text(s, "kind");
            if kind == "text"
                || kind == "image"
                || (!text(s, "span").is_empty() && text(s, "role") == "bar")
            {
                continue;
            }
            let j = items.len();
            let own = text(s, "display_name");
            let label = if !own.is_empty() {
                own.to_string()
            } else {
                labels.get(j).cloned().unwrap_or_default()
            };
            items.push(Item { label, index });
        }
        items.retain(|i| !i.label.is_empty());
        if items.is_empty() {
            return;
        }
        let st = TextStyle::sans(10.0);
        let widest = items
            .iter()
            .map(|i| font::measure(Face::Sans, &tex::plain(&i.label, "tex"), 10.0))
            .fold(0.0, f64::max);
        let width = (b.w * 0.55).min(widest + 35.0);
        let height = items.len() as f64 * 16.0 + 8.0;
        let location = text(legend, "location");
        let x = if location.contains("west") {
            b.x + 6.0
        } else {
            b.x + b.w - width - 6.0
        };
        let y = if location.contains("south") {
            b.y + b.h - height - 6.0
        } else {
            b.y + 6.0
        };
        let p = self.theme;
        self.dev.fill_rect(x, y, width, height, p.background, 1.0);
        self.stroke_rect(
            Box2 {
                x,
                y,
                w: width,
                h: height,
            },
            1.0,
            &[],
            p.axis,
            1.0,
        );
        for (n, item) in items.iter().enumerate() {
            let s = &list[item.index];
            let yy = y + 13.0 + n as f64 * 16.0;
            let kind = text(s, "kind");
            let runs = tex::parse(&item.label, "tex");
            if kind == "patch2d" || kind == "patch_raster" {
                let (face, edge) = if kind == "patch_raster" {
                    (
                        PatchPaint::of(s.get("face")).swatch(),
                        PatchPaint::of(s.get("edge")).swatch(),
                    )
                } else {
                    (rgb(s, "face_color"), rgb(s, "edge_color"))
                };
                let face = face.map(clamp01);
                let edge = if text(s, "line_style") == "none" {
                    None
                } else {
                    edge.map(|c| self.plot_color(c))
                };
                if let Some(c) = face {
                    self.dev.fill_rect(
                        x + 6.0,
                        yy - 8.0,
                        17.0,
                        9.0,
                        c,
                        alpha_value(s, "face_alpha"),
                    );
                }
                if let Some(c) = edge {
                    self.stroke_rect(
                        Box2 {
                            x: x + 6.0,
                            y: yy - 8.0,
                            w: 17.0,
                            h: 9.0,
                        },
                        1.0,
                        &[],
                        c,
                        alpha_value(s, "edge_alpha"),
                    );
                }
                draw_runs(self.dev, &runs, x + 27.0, yy, st, p.text, Affine::IDENTITY);
                continue;
            }
            let (line, edge, face, fa, ea) =
                self.series_style(s, contours.get(item.index).copied().flatten());
            if let Some(c) = line {
                let mut path = Path::new();
                path.move_to(x + 6.0, yy - 3.0);
                path.line_to(x + 23.0, yy - 3.0);
                self.dev
                    .stroke(&path, 1.0, dash(text(s, "line_style")), c, 1.0);
            }
            self.marker(
                x + 14.5,
                yy - 3.0,
                text(s, "marker"),
                edge,
                face,
                fa,
                ea,
                1.0,
                1.0,
            );
            draw_runs(self.dev, &runs, x + 27.0, yy, st, p.text, Affine::IDENTITY);
        }
    }

    /// `drawColorbars`.
    pub fn draw_colorbars(&mut self, axes: &Json) {
        let Some(bars) = axes.get("colorbars").and_then(Json::as_array) else {
            return;
        };
        let p = self.theme;
        let st = TextStyle::mono(10.0);
        for bar in bars {
            let b = fit_axes(&nums(bar, "position"), self.viewport, 0.0);
            let vertical = text(bar, "orientation") == "vertical";
            let reverse = text(bar, "direction") == "reverse";
            let map = desc(bar, "colormap").unwrap_or_default();
            let count = map.shape.first().copied().unwrap_or(0);
            for i in 0..count {
                let e = if reverse { count - 1 - i } else { i };
                let c = [map.at(e), map.at(e + count), map.at(e + 2 * count)];
                let c = [
                    (c[0] * 255.0).round() / 255.0,
                    (c[1] * 255.0).round() / 255.0,
                    (c[2] * 255.0).round() / 255.0,
                ];
                let (low, high) = (i as f64 / count as f64, (i + 1) as f64 / count as f64);
                if vertical {
                    self.dev.fill_rect(
                        b.x,
                        b.y + (1.0 - high) * b.h,
                        b.w,
                        (high - low) * b.h + 0.5,
                        c,
                        1.0,
                    );
                } else {
                    self.dev
                        .fill_rect(b.x + low * b.w, b.y, (high - low) * b.w + 0.5, b.h, c, 1.0);
                }
            }
            let frame = adjust(p.axis, &p);
            self.stroke_rect(b, 1.0, &[], frame, 1.0);
            let location = text(bar, "axis_location");
            let opposite = location == "left" || location == "top";
            let limits = nums(bar, "limits");
            let ticks = nums(bar, "ticks");
            let labels = strings(bar, "tick_labels");
            if limits.len() == 2 {
                for (i, &t) in ticks.iter().enumerate() {
                    let mut fraction = (t - limits[0]) / (limits[1] - limits[0]);
                    if !(0.0..=1.0).contains(&fraction) {
                        continue;
                    }
                    if reverse {
                        fraction = 1.0 - fraction;
                    }
                    let s = labels.get(i).cloned().unwrap_or_else(|| plot_format(t));
                    let mut mark = Path::new();
                    if vertical {
                        let x = if opposite { b.x } else { b.x + b.w };
                        let y = b.y + (1.0 - fraction) * b.h;
                        mark.move_to(x, y);
                        mark.line_to(x + if opposite { -4.0 } else { 4.0 }, y);
                        let align = if opposite { "right" } else { "left" };
                        label(
                            self.dev,
                            &s,
                            "none",
                            x + if opposite { -6.0 } else { 6.0 },
                            y + 3.0,
                            align,
                            st,
                            p.text,
                            Affine::IDENTITY,
                        );
                    } else {
                        let x = b.x + fraction * b.w;
                        let y = if opposite { b.y } else { b.y + b.h };
                        mark.move_to(x, y);
                        mark.line_to(x, y + if opposite { -4.0 } else { 4.0 });
                        label(
                            self.dev,
                            &s,
                            "none",
                            x,
                            y + if opposite { -7.0 } else { 14.0 },
                            "center",
                            st,
                            p.text,
                            Affine::IDENTITY,
                        );
                    }
                    // The strokes carry the colour left in place by the frame.
                    self.dev.stroke(&mark, 1.0, &[], frame, 1.0);
                }
            }
            let title = text(bar, "label");
            if !title.is_empty() {
                if vertical {
                    let xf = Affine::at(
                        if opposite {
                            b.x - 42.0
                        } else {
                            b.x + b.w + 42.0
                        },
                        b.y + b.h / 2.0,
                        -std::f64::consts::FRAC_PI_2,
                    );
                    label(self.dev, title, "tex", 0.0, 0.0, "center", st, p.text, xf);
                } else {
                    label(
                        self.dev,
                        title,
                        "tex",
                        b.x + b.w / 2.0,
                        if opposite {
                            b.y - 24.0
                        } else {
                            b.y + b.h + 28.0
                        },
                        "center",
                        st,
                        p.text,
                        Affine::IDENTITY,
                    );
                }
            }
        }
    }
}

fn star(x: f64, y: f64, r: f64, tips: usize, inner: f64) -> Vec<[f64; 2]> {
    (0..tips * 2)
        .map(|i| {
            let a = -std::f64::consts::FRAC_PI_2 + i as f64 * std::f64::consts::PI / tips as f64;
            let rr = if i % 2 == 0 { r } else { r * inner };
            [x + a.cos() * rr, y + a.sin() * rr]
        })
        .collect()
}

/// `axisTicks` at the exported limits: (value, label or None = format it).
pub fn axis_ticks(axis: &Json, which: &str, range: [f64; 2]) -> Vec<(f64, Option<String>)> {
    let Some(ticks) = axis.get(&format!("{which}tick")).and_then(Json::as_array) else {
        // An older artifact without tick data: five evenly spaced ticks.
        return (0..5)
            .map(|i| (range[0] + (range[1] - range[0]) * i as f64 / 4.0, None))
            .collect();
    };
    let labels = axis
        .get(&format!("{which}ticklabel"))
        .and_then(Json::as_array);
    let (lo, hi) = (range[0].min(range[1]), range[0].max(range[1]));
    let slack = (hi - lo) * 1e-9;
    ticks
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            let value = t.as_f64()?;
            let label = match labels {
                Some(l) if l.len() == ticks.len() => Some(l[i].as_str().unwrap_or("").to_string()),
                Some([]) => Some(String::new()),
                _ => None,
            };
            (value >= lo - slack && value <= hi + slack).then_some((value, label))
        })
        .collect()
}

pub fn alpha_value(s: &Json, key: &str) -> f64 {
    s.get(key)
        .and_then(|d| num(d, "value"))
        .map_or(1.0, |v| v.clamp(0.0, 1.0))
}

/// `images.bounds`.
pub fn image_bounds(centers: &[f64], count: usize) -> [f64; 2] {
    let step = if count > 1 {
        (centers[1] - centers[0]) / (count - 1) as f64
    } else {
        1.0
    };
    [centers[0] - step / 2.0, centers[1] + step / 2.0]
}

/// `images.rgba`.
pub fn image_bitmap(s: &Json, colors: &AxesColors) -> Option<Bitmap> {
    let shape = nums(s, "shape");
    if shape.len() != 2 {
        return None;
    }
    let (rows, cols) = (shape[0] as usize, shape[1] as usize);
    if rows == 0 || cols == 0 {
        return None;
    }
    let data = desc(s, "cdata")?;
    let class = text(s, "cdata_class");
    let mut rgb_out = vec![0u8; rows * cols * 3];
    let truecolor = text(s, "encoding") == "truecolor";
    let direct = text(s, "mapping") == "direct";
    let divisor = match class {
        "uint8" => 255.0,
        "uint16" => 65535.0,
        _ => 1.0,
    };
    let n = rows * cols;
    for row in 0..rows {
        for col in 0..cols {
            let i = row + col * rows;
            let c = if truecolor {
                Some([
                    data.at(i) / divisor,
                    data.at(i + n) / divisor,
                    data.at(i + 2 * n) / divisor,
                ])
            } else {
                colors.index(data.at(i), direct, class)
            };
            let c = c.unwrap_or([0.0, 0.0, 0.0]);
            let o = (row * cols + col) * 3;
            for k in 0..3 {
                rgb_out[o + k] = (c[k].clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
    Some(Bitmap {
        width: cols,
        height: rows,
        rgb: rgb_out,
    })
}

/// `facePolygons`: a face that reads a missing vertex is left out; `null`
/// entries are trailing padding. `dim` columns of `vertices` are returned.
pub fn face_polygons(v: &Desc, f: &Desc, dim: usize) -> Vec<Vec<Vec<f64>>> {
    let rows = f.shape.first().copied().unwrap_or(0);
    let cols = f.shape.get(1).copied().unwrap_or(0);
    let n = v.shape.first().copied().unwrap_or(0);
    let mut out = Vec::with_capacity(rows);
    for r in 0..rows {
        let mut points = Vec::with_capacity(cols);
        let mut missing = false;
        for c in 0..cols {
            let k = f.at(r + c * rows);
            if k.is_nan() {
                break;
            }
            let i = k as usize;
            let q: Vec<f64> = (0..dim).map(|d| v.at(i + d * n)).collect();
            if q.iter().any(|x| !x.is_finite()) {
                missing = true;
                break;
            }
            points.push(q);
        }
        if !missing && points.len() >= 2 {
            out.push(points);
        }
    }
    out
}

pub struct Layout {
    pub box_rect: [f64; 4],
    /// (x, baseline) per line.
    pub lines: Vec<(f64, f64)>,
}

/// `layoutText`.
pub fn layout_text(
    widths: &[f64],
    line_height: f64,
    ascent: f64,
    descent: f64,
    halign: &str,
    valign: &str,
    margin: f64,
) -> Layout {
    let count = widths.len() as f64;
    let width = widths.iter().copied().fold(0.0, f64::max);
    let height = count * line_height;
    let x0 = match halign {
        "center" => -width / 2.0,
        "right" => -width,
        _ => 0.0,
    };
    let lead = (line_height - (ascent + descent)) / 2.0;
    let y0 = match valign {
        "top" | "cap" => 0.0,
        "bottom" => -height,
        "baseline" => -(height - lead - descent),
        _ => -height / 2.0,
    };
    let lines = widths
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let x = x0
                + match halign {
                    "center" => (width - w) / 2.0,
                    "right" => width - w,
                    _ => 0.0,
                };
            (x, y0 + i as f64 * line_height + lead + ascent)
        })
        .collect();
    Layout {
        box_rect: [
            x0 - margin,
            y0 - margin,
            width + 2.0 * margin,
            height + 2.0 * margin,
        ],
        lines,
    }
}

/// The colours of a relaxed patch record (`Graphics::raster_record`).
pub struct PatchPaint {
    mode: String,
    rgb: Option<Rgb>,
    table: Desc,
}

impl PatchPaint {
    pub fn of(j: Option<&Json>) -> PatchPaint {
        let Some(j) = j else {
            return PatchPaint {
                mode: "none".into(),
                rgb: None,
                table: Desc::default(),
            };
        };
        PatchPaint {
            mode: text(j, "mode").to_string(),
            rgb: rgb(j, "rgb"),
            table: desc(j, "table").unwrap_or_default(),
        }
    }

    pub fn interp(&self) -> bool {
        self.mode == "interp"
    }

    pub fn is_none(&self) -> bool {
        self.mode == "none"
    }

    fn stored(&self, i: usize) -> Option<Rgb> {
        let n = self.table.shape.first().copied().unwrap_or(0);
        let c = [
            self.table.at(i),
            self.table.at(i + n),
            self.table.at(i + 2 * n),
        ];
        c.iter().all(|v| v.is_finite()).then_some(c)
    }

    fn row(&self, i: usize) -> Rgb {
        self.stored(i).unwrap_or([0.0, 0.0, 0.0])
    }

    /// Whether a face has colours to draw with: NaN colour data leaves it out.
    pub fn colored(&self, r: usize, idx: &[usize]) -> bool {
        if self.interp() {
            idx.iter().all(|&i| self.stored(i).is_some())
        } else {
            idx.first().is_some_and(|&i| self.face(r, i).is_some())
        }
    }

    /// The colour a legend shows for this paint.
    pub fn swatch(&self) -> Option<Rgb> {
        match self.mode.as_str() {
            "constant" => self.rgb,
            "face" | "first_vertex" | "interp" => {
                let n = self.table.shape.first().copied().unwrap_or(0);
                (0..n).find_map(|i| self.stored(i))
            }
            _ => None,
        }
    }

    pub fn vertex(&self, i: usize) -> Rgb {
        if self.mode == "interp" {
            self.row(i)
        } else {
            self.rgb.unwrap_or([0.0, 0.0, 0.0])
        }
    }

    /// The single colour of face `r`, whose first vertex is `first`.
    pub fn face(&self, r: usize, first: usize) -> Option<Rgb> {
        match self.mode.as_str() {
            "constant" => self.rgb,
            "face" => self.stored(r),
            "first_vertex" | "interp" => self.stored(first),
            _ => None,
        }
    }
}
