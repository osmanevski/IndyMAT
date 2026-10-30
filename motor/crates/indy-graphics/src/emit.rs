//! Figure-Data v3 serializer.
//!
//! A Rust port of IndyMAT's own (MIT) extractor `octave/__mf_figure_data__.m`
//! and its helpers: the same record layout, version rule, budgets and
//! rejection codes, applied to this crate's object model instead of Octave
//! handles. `docs/Figure-Data-v3.md` and `frontend/figure_data_utils.cjs`
//! (the admission gate) are the contract.

use crate::color::{Colormap, Rgb, scaled_index};
use crate::graphics::Graphics;
use crate::json::Json;
use crate::model::{Handle, Kind, Paint, ScatterColor};
use crate::scene::{AxesState, Leaf, LeafObj};
use crate::value::Array;

/// Engineering budgets of the contract (`limits`).
pub mod limits {
    pub const SAMPLES: usize = 2000;
    pub const SURFACE_VERTICES: usize = 40000;
    pub const PATCH_VERTICES: usize = 40000;
    pub const PATCH_TRIANGLES: usize = 80000;
    pub const VERTICES: usize = 100000;
    pub const TRIANGLES: usize = 200000;
    pub const AXES: usize = 16;
    pub const SERIES: usize = 128;
    pub const COLORMAP: usize = 4096;
    pub const JSON_BYTES: usize = 8388608;
    pub const OBJECTS: usize = 1024;
    pub const DEPTH: usize = 16;
    pub const SOURCE_SAMPLES: usize = 1000000;
    pub const TICKS: usize = 4096;
    pub const GLOBAL_OBJECTS: usize = 12289;
    pub const TEXT_LINES: usize = 256;
    pub const TEXT_CHARS: usize = 65536;
    pub const IMAGE_PIXELS: usize = 262144;
}

const MARKERS: [&str; 12] = [
    "none", "o", "s", "square", "^", "v", ">", "<", ".", "+", "x", "*",
];

thread_local! {
    /// Set while [`Graphics::raster_record`] serializes: the wire contract's
    /// budgets and refusals give way to what the rasteriser can draw. Off
    /// (the only state the UI's records see), every output is unchanged.
    static RELAXED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn relaxed() -> bool {
    RELAXED.with(|r| r.get())
}

/// Runs `f` relaxed. A panic the interpreter catches ends it too: the UI's
/// records on this thread must not stay relaxed afterwards.
fn relaxed_while<T>(f: impl FnOnce() -> T) -> T {
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            RELAXED.with(|r| r.set(false));
        }
    }
    let _restore = Restore;
    RELAXED.with(|r| r.set(true));
    f()
}

/// Work bounds of the relaxed path, far above the wire budgets. A figure
/// beyond them stays an explicit refusal rather than an endless render.
mod raster_limits {
    pub const SAMPLES: usize = 200_000;
    pub const SOURCE_SAMPLES: usize = 50_000_000;
    pub const SURFACE_VERTICES: usize = 1_000_000;
    pub const PATCH_VERTICES: usize = 1_000_000;
    pub const VERTICES: usize = 4_000_000;
    pub const IMAGE_PIXELS: usize = 8_000_000;
}

/// Whether `n` exceeds the wire budget `limit` (or, relaxed, the work bound).
fn over(n: usize, limit: usize, raster: usize) -> bool {
    n > if relaxed() { raster } else { limit }
}

fn sample_limit() -> usize {
    if relaxed() {
        raster_limits::SAMPLES
    } else {
        limits::SAMPLES
    }
}

/// A stable producer rejection: `reason_code` plus `reason_args`.
#[derive(Debug, Clone)]
pub(crate) struct Reject {
    code: &'static str,
    args: Json,
}

fn reject(code: &'static str) -> Reject {
    Reject {
        code,
        args: Json::obj(),
    }
}

fn reject_with(code: &'static str, key: &str, value: impl Into<Json>) -> Reject {
    Reject {
        code,
        args: Json::obj().with(key, value),
    }
}

fn budget(name: &str, actual: usize, limit: usize) -> Reject {
    Reject {
        code: "budget_exceeded",
        args: Json::obj()
            .with("budget", name)
            .with("actual", actual)
            .with("limit", limit),
    }
}

pub(crate) type R<T> = std::result::Result<T, Reject>;

fn limits_json() -> Json {
    use limits::*;
    let mut o = Json::obj();
    for (k, v) in [
        ("samples", SAMPLES),
        ("surface_vertices", SURFACE_VERTICES),
        ("patch_vertices", PATCH_VERTICES),
        ("patch_triangles", PATCH_TRIANGLES),
        ("vertices", VERTICES),
        ("triangles", TRIANGLES),
        ("axes", AXES),
        ("series", SERIES),
        ("colormap", COLORMAP),
        ("json_bytes", JSON_BYTES),
        ("objects", OBJECTS),
        ("depth", DEPTH),
        ("source_samples", SOURCE_SAMPLES),
        ("ticks", TICKS),
        ("global_objects", GLOBAL_OBJECTS),
        ("text_lines", TEXT_LINES),
        ("text_chars", TEXT_CHARS),
        ("image_pixels", IMAGE_PIXELS),
    ] {
        o.set(k, v);
    }
    o
}

/// Column-major array descriptor.
fn descriptor(dims: &[usize], values: &[f64]) -> Json {
    Json::obj()
        .with(
            "shape",
            Json::Arr(dims.iter().map(|&d| Json::from(d)).collect()),
        )
        .with("order", "column-major")
        .with("values", Json::from(values))
}

fn array_descriptor(a: &Array) -> Json {
    descriptor(&a.dims, &a.data)
}

fn colormap_descriptor(map: &Colormap) -> Json {
    let n = map.len();
    let mut values = Vec::with_capacity(3 * n);
    for c in 0..3 {
        values.extend(map.iter().map(|row| row[c]));
    }
    descriptor(&[n, 3], &values)
}

fn rgb_or_none(p: Paint) -> Option<Json> {
    match p {
        Paint::None => Some(Json::from("none")),
        Paint::Rgb(c) if valid_rgb(c) => Some(Json::from(c)),
        _ => None,
    }
}

fn valid_rgb(c: Rgb) -> bool {
    c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))
}

fn strings(v: &[String]) -> Json {
    Json::Arr(v.iter().map(|s| Json::from(s.as_str())).collect())
}

/// Indexed colour of a scaled scalar through the axes colormap and CLim.
fn index_color(value: f64, st: &AxesState) -> Option<Rgb> {
    if !value.is_finite()
        || st.colormap.is_empty()
        || st.clim[1] <= st.clim[0]
        || st.clim.iter().any(|v| v.is_nan())
    {
        return None;
    }
    let i = scaled_index(value, st.clim, st.colormap.len());
    Some(st.colormap[i])
}

impl Graphics {
    /// The Figure-Data record of a figure as a JSON value.
    pub fn figure_record(&self, fig: Handle) -> Json {
        let figure_size = self
            .figure_ref(fig)
            .map(|f| [f.position[2], f.position[3]])
            .unwrap_or([560.0, 420.0]);
        let source = Json::obj()
            .with("job", "")
            .with("figure", 0.0)
            .with("figure_size", figure_size);
        let mut data = Json::obj()
            .with("version", 2.0)
            .with("supported", true)
            .with("decimated", false)
            .with("point_limit", 2000.0)
            .with("axes", Json::Arr(Vec::new()))
            .with("reason", "")
            .with("reason_code", "")
            .with("reason_args", Json::obj())
            .with("source", source)
            .with("limits", limits_json())
            .with("vertex_count", 0.0)
            .with("triangle_count", 0.0)
            .with("pixel_count", 0.0);
        let mut version = 2;
        match self.build(fig, &mut data, &mut version) {
            Ok(()) => {}
            Err(r) => {
                data.set("supported", false);
                data.set("axes", Json::Arr(Vec::new()));
                data.set("decimated", false);
                data.set("vertex_count", 0.0);
                data.set("triangle_count", 0.0);
                data.set("pixel_count", 0.0);
                data.set(
                    "reason",
                    format!("Only the PNG view is available: {}.", r.code),
                );
                data.set("reason_code", r.code);
                data.set("reason_args", r.args);
            }
        }
        data.set("version", version as f64);
        data
    }

    /// The figure's axes in the order of a record's `axes`.
    pub fn record_axes(&self, fig: Handle) -> Vec<Handle> {
        self.obj(fig).map_or_else(
            |_| Vec::new(),
            |figure| {
                figure
                    .children
                    .iter()
                    .copied()
                    .filter(|h| matches!(self.objects.get(h).map(|o| &o.kind), Some(Kind::Axes(_))))
                    .collect()
            },
        )
    }

    /// The record the rasteriser draws: [`Graphics::figure_record`] with the
    /// contract's budgets and refusals lifted, so a figure the interactive
    /// view must refuse (per-point colours, `yyaxis`, huge data, ...) still
    /// gets a complete picture. Fields only this path emits: `axes_color_none`
    /// and `yaxis_location` on axes; `point_colors`/`point_edge`/`point_face`
    /// on scatter; `patch_raster` records; `z` on text.
    pub fn raster_record(&self, fig: Handle) -> Json {
        relaxed_while(|| self.figure_record(fig))
    }

    /// The Figure-Data JSON text of a figure (the artifact the UI renders).
    pub fn figure_json(&self, fig: Handle) -> String {
        let text = self.figure_record(fig).to_json_string();
        if text.len() <= limits::JSON_BYTES {
            return text;
        }
        // The exporter's cap: replace an oversized record by a small fallback.
        let mut small = self.figure_record_rejected(fig, "json_budget");
        small.set(
            "reason_args",
            Json::obj()
                .with("actual", text.len())
                .with("limit", limits::JSON_BYTES),
        );
        small.to_json_string()
    }

    fn figure_record_rejected(&self, fig: Handle, code: &'static str) -> Json {
        let mut d = self.figure_record(fig);
        d.set("supported", false);
        d.set("axes", Json::Arr(Vec::new()));
        d.set("decimated", false);
        d.set("vertex_count", 0.0);
        d.set("triangle_count", 0.0);
        d.set("pixel_count", 0.0);
        d.set("reason_code", code);
        d.set("reason", format!("Only the PNG view is available: {code}."));
        d
    }

    fn build(&self, fig: Handle, data: &mut Json, version: &mut u8) -> R<()> {
        let figure = self.obj(fig).map_err(|_| reject("no_axes"))?;
        let axes: Vec<Handle> = figure
            .children
            .iter()
            .copied()
            .filter(|h| matches!(self.objects.get(h).map(|o| &o.kind), Some(Kind::Axes(_))))
            .collect();
        if !relaxed() && self.objects.len() + 1 > limits::GLOBAL_OBJECTS {
            return Err(budget(
                "global_objects",
                self.objects.len() + 1,
                limits::GLOBAL_OBJECTS,
            ));
        }
        let overlay = usize::from(self.sgtitle_of(fig).is_some());
        if !relaxed() && axes.len() + overlay > limits::AXES {
            return Err(budget("axes", axes.len() + overlay, limits::AXES));
        }
        if axes.is_empty() {
            return Err(reject("no_axes"));
        }
        let colorbars: Vec<Handle> = figure
            .children
            .iter()
            .copied()
            .filter(|h| {
                matches!(self.objects.get(h), Some(o) if o.visible && matches!(o.kind, Kind::Colorbar(_)))
            })
            .collect();
        if !colorbars.is_empty() {
            *version = 3;
        }
        // Preflight aggregate upper bounds before building records.
        let mut all_leaves = Vec::with_capacity(axes.len());
        let (mut vertices, mut triangles, mut pixels, mut count) = (0usize, 0usize, 0usize, 0usize);
        for &ax in &axes {
            if !relaxed() && self.is_yyaxis(ax) {
                return Err(reject_with("unsupported_object", "type", "yyaxis"));
            }
            let a = self.axes_ref(ax).map_err(|_| reject("no_axes"))?;
            if !a.is_2d_view() || !self.obj(ax).map(|o| o.visible).unwrap_or(true) {
                *version = 3;
            }
            let leaves = self.leaves(ax);
            for leaf in &leaves {
                count += 1;
                if leaf.role.is_some() {
                    *version = 3;
                }
                match &leaf.obj {
                    LeafObj::Line(l, ..) => {
                        if !l.z.is_empty() {
                            *version = 3;
                        }
                        vertices += l.x.len().min(sample_limit());
                    }
                    LeafObj::Baseline { .. } => vertices += 2,
                    LeafObj::Scatter(s) => {
                        if !s.z.is_empty() {
                            *version = 3;
                        }
                        vertices += s.x.len().min(sample_limit());
                    }
                    LeafObj::ContourLine { x, .. } | LeafObj::Contour3 { x, .. } => {
                        let n = x.len() + 1;
                        if over(n, limits::SAMPLES, raster_limits::SAMPLES) {
                            return Err(budget("samples", n, limits::SAMPLES));
                        }
                        vertices += n;
                    }
                    LeafObj::Surface(s) => {
                        let n = s.z.numel();
                        if over(n, limits::SURFACE_VERTICES, raster_limits::SURFACE_VERTICES) {
                            return Err(budget("surface_vertices", n, limits::SURFACE_VERTICES));
                        }
                        vertices += n;
                        triangles +=
                            2 * s.z.rows().saturating_sub(1) * s.z.cols().saturating_sub(1);
                    }
                    LeafObj::Patch(p, _) => {
                        let n = p.vertices.rows();
                        if over(n, limits::PATCH_VERTICES, raster_limits::PATCH_VERTICES) {
                            return Err(budget("patch_vertices", n, limits::PATCH_VERTICES));
                        }
                        let fan = p.faces.rows() * p.faces.cols().saturating_sub(2);
                        if !relaxed() && fan > limits::PATCH_TRIANGLES {
                            return Err(budget("patch_triangles", fan, limits::PATCH_TRIANGLES));
                        }
                        vertices += n;
                        triangles += fan;
                    }
                    LeafObj::ContourFill {
                        vertices: v, face, ..
                    } => {
                        if over(v.len(), limits::VERTICES, raster_limits::VERTICES) {
                            return Err(budget("vertices", v.len(), limits::VERTICES));
                        }
                        vertices += v.len();
                        triangles += face.len().saturating_sub(2);
                    }
                    LeafObj::Image(img) => {
                        pixels += img.cdata.rows() * img.cdata.cols();
                        if over(pixels, limits::IMAGE_PIXELS, raster_limits::IMAGE_PIXELS) {
                            return Err(budget("image_pixels", pixels, limits::IMAGE_PIXELS));
                        }
                    }
                    LeafObj::Text(_) => {}
                }
                if matches!(
                    leaf.obj,
                    LeafObj::Surface(_)
                        | LeafObj::Patch(..)
                        | LeafObj::Image(_)
                        | LeafObj::Text(_)
                        | LeafObj::ContourFill { .. }
                ) {
                    *version = 3;
                }
            }
            all_leaves.push(leaves);
        }
        if !relaxed() && count > limits::SERIES {
            return Err(budget("series", count, limits::SERIES));
        }
        if over(vertices, limits::VERTICES, raster_limits::VERTICES) {
            return Err(budget("vertices", vertices, limits::VERTICES));
        }
        if !relaxed() && triangles > limits::TRIANGLES {
            return Err(budget("triangles", triangles, limits::TRIANGLES));
        }
        let (mut vertex_count, mut triangle_count, mut pixel_count) = (0usize, 0usize, 0usize);
        let mut decimated = false;
        let mut records = Vec::with_capacity(axes.len());
        for (k, (&ax, leaves)) in axes.iter().zip(&all_leaves).enumerate() {
            let (item, dim) = self.axes_record(ax, k, leaves)?;
            if dim == 3 {
                *version = 3;
            }
            if let Some(series) = item.get("series").and_then(Json::as_array) {
                for s in series {
                    vertex_count += s
                        .get("rendered_points")
                        .and_then(Json::as_f64)
                        .unwrap_or(0.0) as usize;
                    let kind = s.get("kind").and_then(Json::as_str).unwrap_or("");
                    if kind == "image" {
                        pixel_count +=
                            s.get("pixel_count").and_then(Json::as_f64).unwrap_or(0.0) as usize;
                    }
                    if kind == "surface" || kind == "patch3d" {
                        triangle_count += s
                            .get("triangle_count")
                            .and_then(Json::as_f64)
                            .unwrap_or(0.0) as usize;
                        *version = 3;
                    }
                }
            }
            if over(vertex_count, limits::VERTICES, raster_limits::VERTICES) {
                return Err(budget("vertices", vertex_count, limits::VERTICES));
            }
            if !relaxed() && triangle_count > limits::TRIANGLES {
                return Err(budget("triangles", triangle_count, limits::TRIANGLES));
            }
            decimated |= item
                .get("decimated")
                .and_then(Json::as_bool)
                .unwrap_or(false);
            records.push(item);
        }
        if let Some(rec) = self.sgtitle_overlay(fig, records.len())? {
            *version = 3;
            if let Some(series) = rec.get("series").and_then(Json::as_array) {
                vertex_count += series
                    .iter()
                    .filter_map(|s| s.get("rendered_points").and_then(Json::as_f64))
                    .sum::<f64>() as usize;
            }
            records.push(rec);
        }
        for &cb in &colorbars {
            let Some(Kind::Colorbar(c)) = self.objects.get(&cb).map(|o| &o.kind) else {
                continue;
            };
            let Some(index) = axes.iter().position(|a| *a == c.peer) else {
                return Err(reject_with(
                    "unsupported_colorbar",
                    "property",
                    "__axes_handle__",
                ));
            };
            let bar = self.colorbar_record(cb, index)?;
            if let Some(Json::Arr(list)) = records[index].get_mut("colorbars") {
                list.push(bar);
            }
        }
        data.set("axes", Json::Arr(records));
        data.set("decimated", decimated);
        data.set("vertex_count", vertex_count);
        data.set("triangle_count", triangle_count);
        data.set("pixel_count", pixel_count);
        Ok(())
    }

    fn axes_record(&self, ax: Handle, id: usize, leaves: &[Leaf]) -> R<(Json, u8)> {
        let a = self.axes_ref(ax).map_err(|_| reject("no_axes"))?;
        let st = self.axes_state(ax).ok_or_else(|| reject("no_axes"))?;
        let visible = self.obj(ax).map(|o| o.visible).unwrap_or(true);
        let position = self.inset_position(ax, a.position);
        if position != a.position {
            let mut moved = a.clone();
            moved.position = position;
            return self.axes_record_of(&moved, st, visible, id, leaves);
        }
        self.axes_record_of(a, st, visible, id, leaves)
    }

    /// The record of an axes `a` (possibly not in the tree, such as the
    /// `sgtitle` overlay) with its resolved state.
    pub(crate) fn axes_record_of(
        &self,
        a: &crate::model::Axes,
        st: AxesState,
        visible: bool,
        id: usize,
        leaves: &[Leaf],
    ) -> R<(Json, u8)> {
        check_state(&st)?;
        let label = |h: Handle| match self.objects.get(&h) {
            Some(o) if o.visible => match &o.kind {
                Kind::Text(t) => t.joined(),
                _ => String::new(),
            },
            _ => String::new(),
        };
        let interp = |h: Handle| match self.objects.get(&h).map(|o| &o.kind) {
            Some(Kind::Text(t)) => t.interpreter.clone(),
            _ => "tex".into(),
        };
        let legend = match a.legend.and_then(|l| self.objects.get(&l)) {
            Some(o) => match &o.kind {
                Kind::Legend(l) => Json::obj()
                    .with("visible", o.visible)
                    .with("location", l.location.as_str())
                    .with("labels", strings(&l.labels)),
                _ => empty_legend(),
            },
            None => empty_legend(),
        };
        let mut rec = Json::obj()
            .with("id", id)
            .with("dimension", st.dimension as f64)
            .with("supported", true)
            .with("reason", "")
            .with(
                "position",
                if a.pie {
                    crate::pie::square_box(a.position, st.figure_size)
                } else {
                    a.position
                },
            )
            .with("xlabel", label(a.x.label))
            .with("ylabel", label(a.y.label))
            .with("zlabel", label(a.z.label))
            .with("title", label(a.title))
            .with(
                "grid",
                Json::obj()
                    .with("x", a.x.grid)
                    .with("y", a.y.grid)
                    .with("z", a.z.grid),
            )
            .with("legend", legend)
            .with("decimated", false)
            .with("series", Json::Arr(Vec::new()))
            .with("colorbars", Json::Arr(Vec::new()))
            .with("visible", visible)
            .with(
                "interpreters",
                Json::obj()
                    .with("title", interp(a.title))
                    .with("xlabel", interp(a.x.label))
                    .with("ylabel", interp(a.y.label))
                    .with("ticks", a.tick_label_interpreter.as_str()),
            );
        for (i, key) in ["x", "y", "z"].iter().enumerate() {
            let p = a.axis(i);
            if !relaxed() && st.tick[i].len() > limits::TICKS {
                return Err(budget("ticks", st.tick[i].len(), limits::TICKS));
            }
            rec.set(&format!("{key}lim"), st.lim[i]);
            rec.set(
                &format!("{key}scale"),
                if p.scale == crate::model::Scale::Log {
                    "log"
                } else {
                    "linear"
                },
            );
            rec.set(
                &format!("{key}dir"),
                if p.reverse { "reverse" } else { "normal" },
            );
            rec.set(&format!("{key}tick"), st.tick[i].clone());
            rec.set(&format!("{key}ticklabel"), strings(&st.ticklabel[i]));
            rec.set(&format!("{key}tickmode"), p.tick_mode.name());
            rec.set(&format!("{key}ticklabelmode"), p.ticklabel_mode.name());
        }
        if let Some(base) = a.sg_base {
            rec.set("title_layout_position", base);
        }
        rec.set("view", a.view);
        rec.set(
            "camera",
            Json::obj()
                .with("projection", "orthographic")
                .with("position", st.camera.position)
                .with("target", st.camera.target)
                .with("up_vector", st.camera.up)
                .with("view_angle", st.camera.view_angle)
                .with("position_mode", "auto")
                .with("target_mode", "auto")
                .with("upvector_mode", "auto")
                .with("viewangle_mode", "auto"),
        );
        rec.set("data_aspect_ratio", st.dar);
        rec.set("data_aspect_ratio_mode", a.dar_mode.name());
        rec.set("plot_box_aspect_ratio", st.pbar);
        rec.set("plot_box_aspect_ratio_mode", a.pbar_mode.name());
        rec.set("clim", st.clim);
        if !relaxed() && st.colormap.len() > limits::COLORMAP {
            return Err(budget("colormap", st.colormap.len(), limits::COLORMAP));
        }
        if st.colormap.is_empty() || !st.colormap.iter().all(|c| valid_rgb(*c)) {
            return Err(reject_with("invalid_data", "property", "colormap"));
        }
        rec.set("colormap", colormap_descriptor(&st.colormap));
        let log = [a.x.scale, a.y.scale, a.z.scale].contains(&crate::model::Scale::Log);
        if !relaxed() && st.dimension == 3 && log {
            return Err(reject("log_3d"));
        }
        if relaxed() {
            if a.y_right {
                rec.set("yaxis_location", "right");
            }
            match a.color {
                Paint::None => rec.set("axes_color_none", true),
                Paint::Rgb(c) if valid_rgb(c) => rec.set("axes_color", c),
                _ => {}
            }
        }
        // A pie's record position is its drawn square box (pie.rs), so its
        // slices keep their shape in the stretch-to-box viewer.
        let fixed_aspect = !a.pie
            && (a.dar_mode == crate::model::Mode::Manual
                || a.pbar_mode == crate::model::Mode::Manual);
        let log_xy = a.x.scale == crate::model::Scale::Log || a.y.scale == crate::model::Scale::Log;
        let mut series = Vec::with_capacity(leaves.len());
        let mut decimated = false;
        for leaf in leaves {
            let capped = self.errorbar_caps(a, leaf, &st);
            let leaf = capped.as_ref().unwrap_or(leaf);
            let mut s = self.series_record(leaf, &st, fixed_aspect, log_xy)?;
            if let Some(g) = leaf.group
                && matches!(leaf.role, Some("contour") | Some("contourf"))
            {
                s.set("contour_group", g);
            }
            s.set("id", series.len());
            decimated |= s.get("decimated").and_then(Json::as_bool).unwrap_or(false);
            series.push(s);
        }
        rec.set("decimated", decimated);
        rec.set("series", Json::Arr(series));
        Ok((rec, st.dimension))
    }

    fn series_record(
        &self,
        leaf: &Leaf,
        st: &AxesState,
        fixed_aspect: bool,
        log_xy: bool,
    ) -> R<Json> {
        let dim = st.dimension;
        match &leaf.obj {
            LeafObj::Line(l, kind, base) => {
                if dim == 3 && *kind != "line" && !l.z.is_empty() {
                    return Err(reject_with("unsupported_group", "type", "3d_hggroup"));
                }
                line_record(l, kind, *base, leaf)
            }
            LeafObj::Baseline { horizontal, base } => {
                let (x, y) = if *horizontal {
                    (vec![*base, *base], st.lim[1].to_vec())
                } else {
                    (st.lim[0].to_vec(), vec![*base, *base])
                };
                let mut l = crate::model::Line::new(x, y, Vec::new(), [0.0, 0.0, 0.0]);
                l.display_name.clear();
                let mut s = line_record(&l, "line", 0.0, leaf)?;
                s.set(
                    "span",
                    if *horizontal {
                        "vertical"
                    } else {
                        "horizontal"
                    },
                );
                Ok(s)
            }
            LeafObj::Scatter(sc) => scatter_record(sc, st),
            LeafObj::Surface(s) => surface_record(s),
            LeafObj::Patch(p, bar) => {
                if relaxed() {
                    let single = |c: Paint| match c {
                        Paint::None | Paint::Rgb(_) => true,
                        Paint::Flat => p.fvcdata.rows() == 1,
                        _ => false,
                    };
                    if dim == 3 || !single(p.face_color) || !single(p.edge_color) {
                        return raster_patch_record(p, leaf, st);
                    }
                } else if dim == 3 {
                    return Err(reject_with("unsupported_object", "type", "patch"));
                }
                if !relaxed() && log_xy {
                    return Err(reject_with("unsupported_patch", "property", "scale"));
                }
                if !relaxed() && fixed_aspect {
                    return Err(reject_with(
                        "unsupported_patch",
                        "property",
                        "dataaspectratio",
                    ));
                }
                patch_record(p, bar.as_ref(), leaf, st)
            }
            LeafObj::Text(t) => {
                if !relaxed() && dim == 3 {
                    return Err(reject_with("unsupported_object", "type", "text"));
                }
                text_record(t)
            }
            LeafObj::Image(img) => {
                if !relaxed() && (dim != 2 || log_xy) {
                    return Err(reject_with("unsupported_object", "type", "image"));
                }
                image_record(img)
            }
            LeafObj::ContourLine {
                level,
                x,
                y,
                color,
                line_style,
                line_width,
            } => contour_line_record(*level, x, y, *color, line_style, *line_width, leaf, st),
            LeafObj::Contour3 {
                level,
                x,
                y,
                z,
                color,
                line_style,
                line_width,
            } => {
                let mut s =
                    contour_line_record(*level, x, y, *color, line_style, *line_width, leaf, st)?;
                s.set("z", crate::surf2::contour3_z(x, y, z));
                Ok(s)
            }
            LeafObj::ContourFill {
                level,
                vertices,
                face,
                edge,
                line_style,
                line_width,
            } => contourf_record(*level, vertices, face, *edge, line_style, *line_width, leaf),
        }
    }

    fn colorbar_record(&self, cb: Handle, peer_index: usize) -> R<Json> {
        let Some(Kind::Colorbar(c)) = self.objects.get(&cb).map(|o| &o.kind) else {
            return Err(reject_with("unsupported_colorbar", "property", "children"));
        };
        self.axes_ref(c.peer)
            .map_err(|_| reject_with("unsupported_colorbar", "property", "__axes_handle__"))?;
        let st = self
            .axes_state(c.peer)
            .ok_or_else(|| reject_with("unsupported_colorbar", "property", "__axes_handle__"))?;
        let base = self.inset_base(c.peer);
        let (peer_pos, mut position, vertical) =
            crate::layout::colorbar_geometry(&c.location, base)
                .ok_or_else(|| reject_with("unsupported_colorbar", "property", "location"))?;
        // Measured: next to a fixed-aspect 2-D box (`axis equal`, `daspect`,
        // `pbaspect`, `axis image`) an outside colorbar spans the drawn box
        // and keeps its gap to the box instead of to the position.
        let b = self.plot_box(c.peer, peer_pos);
        if b != peer_pos {
            match c.location.as_str() {
                "eastoutside" => position[0] += b[0] + b[2] - peer_pos[0] - peer_pos[2],
                "westoutside" => position[0] += b[0] - peer_pos[0],
                "northoutside" => position[1] += b[1] + b[3] - peer_pos[1] - peer_pos[3],
                "southoutside" => position[1] += b[1] - peer_pos[1],
                _ => {}
            }
            if c.location.ends_with("outside") {
                if vertical {
                    (position[1], position[3]) = (b[1], b[3]);
                } else {
                    (position[0], position[2]) = (b[0], b[2]);
                }
            }
        }
        let ticks = crate::layout::ticks_for(st.clim);
        if !relaxed() && ticks.len() > limits::TICKS {
            return Err(budget("ticks", ticks.len(), limits::TICKS));
        }
        let labels: Vec<String> = ticks
            .iter()
            .map(|v| crate::layout::tick_label(*v))
            .collect();
        let axis_location = match c.location.as_str() {
            "eastoutside" | "west" => "right",
            "westoutside" | "east" => "left",
            "northoutside" | "south" => "top",
            _ => "bottom",
        };
        Ok(Json::obj()
            .with("peer_axes", peer_index)
            .with("position", position)
            .with(
                "orientation",
                if vertical { "vertical" } else { "horizontal" },
            )
            .with("location", c.location.as_str())
            .with("limits", st.clim)
            .with("ticks", ticks)
            .with("tick_labels", labels)
            .with("direction", "normal")
            .with("axis_location", axis_location)
            .with("label", c.label.as_str())
            .with("colormap", colormap_descriptor(&st.colormap)))
    }
}

/// Extreme data (ranges near the f64 limit) can make automatic limits or the
/// camera non-finite; such a figure is an explicit `invalid_data` fallback.
fn check_state(st: &AxesState) -> R<()> {
    for (i, key) in ["xlim", "ylim", "zlim"].iter().enumerate() {
        let l = st.lim[i];
        if !(l[0].is_finite() && l[1].is_finite() && l[0] < l[1]) {
            return Err(reject_with("invalid_data", "property", *key));
        }
        if st.tick[i].iter().any(|t| !t.is_finite()) {
            return Err(reject_with("invalid_data", "property", "ticks"));
        }
    }
    let c = &st.camera;
    let finite = |v: &[f64]| v.iter().all(|x| x.is_finite());
    let moved = c.position.iter().zip(&c.target).any(|(p, t)| p != t);
    if !(finite(&c.position) && finite(&c.target) && finite(&c.up) && moved)
        || !(c.view_angle > 0.0 && c.view_angle < 180.0)
        || !finite(&st.dar)
        || st.dar.iter().any(|d| *d <= 0.0)
        || !(st.clim[0].is_finite() && st.clim[1].is_finite() && st.clim[0] < st.clim[1])
    {
        return Err(reject_with("invalid_data", "property", "camera"));
    }
    Ok(())
}

fn empty_legend() -> Json {
    Json::obj()
        .with("visible", false)
        .with("location", "")
        .with("labels", Json::Arr(Vec::new()))
}

/// Sample selection for long series (port of `__mf_sample__`): keeps run
/// endpoints, gaps, global and per-bucket extrema. One-based → zero-based.
fn sample(x: &[f64], y: &[f64], z: &[f64], limit: usize) -> Option<Vec<usize>> {
    let n = x.len();
    if n <= limit {
        return Some((0..n).collect());
    }
    let valid: Vec<bool> = (0..n)
        .map(|i| x[i].is_finite() && y[i].is_finite() && (z.is_empty() || z[i].is_finite()))
        .collect();
    let mut anchors = vec![0, n - 1];
    for i in 0..n {
        let prev = i > 0 && valid[i - 1];
        let next = i + 1 < n && valid[i + 1];
        if valid[i] && !prev {
            anchors.push(i);
        }
        if valid[i] && !next {
            anchors.push(i);
        }
        if !valid[i] && (i == 0 || valid[i - 1]) {
            anchors.push(i);
        }
    }
    let valid_idx: Vec<usize> = (0..n).filter(|&i| valid[i]).collect();
    let axes: Vec<&[f64]> = if z.is_empty() {
        vec![x, y]
    } else {
        vec![x, y, z]
    };
    let extrema = |idx: &[usize], v: &[f64], out: &mut Vec<usize>| {
        if idx.is_empty() {
            return;
        }
        let (mut lo, mut hi) = (idx[0], idx[0]);
        for &i in idx {
            if v[i] < v[lo] {
                lo = i;
            }
            if v[i] > v[hi] {
                hi = i;
            }
        }
        out.push(lo);
        out.push(hi);
    };
    for v in &axes {
        extrema(&valid_idx, v, &mut anchors);
    }
    anchors.sort_unstable();
    anchors.dedup();
    if anchors.len() > limit {
        return None;
    }
    let stride = if z.is_empty() { 4 } else { 6 };
    let buckets = (limit - anchors.len()) / stride;
    let mut idx = anchors;
    if buckets > 0 {
        // round(linspace(1, n+1, buckets+1)), one-based.
        let boundary = |k: usize| -> usize {
            let t = 1.0 + (n as f64) * k as f64 / buckets as f64;
            t.round() as usize - 1
        };
        for k in 0..buckets {
            let r: Vec<usize> = (boundary(k)..boundary(k + 1))
                .filter(|&i| valid[i])
                .collect();
            for v in &axes {
                extrema(&r, v, &mut idx);
            }
        }
    }
    idx.sort_unstable();
    idx.dedup();
    Some(idx)
}

fn pick(v: &[f64], idx: &[usize]) -> Vec<f64> {
    idx.iter().map(|&i| v[i]).collect()
}

fn marker_ok(marker: &str) -> R<()> {
    if relaxed() || MARKERS.contains(&marker) {
        Ok(())
    } else {
        Err(reject_with("unsupported_marker", "marker", marker))
    }
}

fn line_record(l: &crate::model::Line, kind: &str, base: f64, leaf: &Leaf) -> R<Json> {
    let n = l.x.len();
    if l.y.len() != n || (!l.z.is_empty() && l.z.len() != n) {
        return Err(reject_with("invalid_data", "property", "coordinates"));
    }
    if over(n, limits::SOURCE_SAMPLES, raster_limits::SOURCE_SAMPLES) {
        return Err(budget("source_samples", n, limits::SOURCE_SAMPLES));
    }
    let idx = sample(&l.x, &l.y, &l.z, sample_limit())
        .ok_or_else(|| budget("segment_anchors", n, limits::SAMPLES))?;
    if !valid_rgb(l.color) {
        return Err(reject("unsupported_color"));
    }
    let edge = match l.marker_edge {
        Paint::Auto | Paint::Flat => Paint::Rgb(l.color),
        p => p,
    };
    let face = match l.marker_face {
        Paint::Auto => Paint::Rgb([1.0, 1.0, 1.0]),
        Paint::Flat => Paint::None,
        p => p,
    };
    let (Some(edge), Some(face)) = (rgb_or_none(edge), rgb_or_none(face)) else {
        return Err(reject("unsupported_color"));
    };
    marker_ok(&l.marker)?;
    let mut s = Json::obj()
        .with("kind", kind)
        .with("x", pick(&l.x, &idx))
        .with("y", pick(&l.y, &idx))
        .with("line_color", l.color)
        .with("marker_edge_color", edge)
        .with("marker_face_color", face)
        .with("marker_face_auto", l.marker_face == Paint::Auto)
        .with("line_style", l.line_style.as_str())
        .with("line_width", l.line_width)
        .with("marker", l.marker.as_str())
        .with("marker_size", l.marker_size)
        .with("display_name", l.display_name.as_str())
        .with("base_value", base)
        .with("decimated", idx.len() < n)
        .with("original_points", n)
        .with("rendered_points", idx.len())
        .with(
            "source_indices",
            idx.iter().map(|&i| i as f64).collect::<Vec<f64>>(),
        );
    if !l.z.is_empty() {
        s.set("z", pick(&l.z, &idx));
    }
    if let Some(role) = leaf.role {
        s.set("role", role);
        // Like the extractor: an errorbar (or quiver) group's name labels
        // its first line only, which alone carries the name.
        let own = matches!(role, "errorbar" | "quiver");
        s.set(
            "display_name",
            if own { l.display_name.as_str() } else { "" },
        );
        if n == 2 && l.z.is_empty() && l.x.iter().chain(&l.y).all(|v| v.is_finite()) {
            if l.x[0] == l.x[1] && l.y[0] != l.y[1] {
                s.set("span", "vertical");
            } else if l.y[0] == l.y[1] && l.x[0] != l.x[1] {
                s.set("span", "horizontal");
            }
        }
    }
    Ok(s)
}

fn scatter_record(sc: &crate::model::Scatter, st: &AxesState) -> R<Json> {
    let n = sc.x.len();
    if sc.y.len() != n || (!sc.z.is_empty() && sc.z.len() != n) {
        return Err(reject_with("invalid_data", "property", "coordinates"));
    }
    let is3 = !sc.z.is_empty();
    if over(n, limits::SOURCE_SAMPLES, raster_limits::SOURCE_SAMPLES) {
        return Err(budget("source_samples", n, limits::SOURCE_SAMPLES));
    }
    if is3 && over(n, limits::SAMPLES, raster_limits::SAMPLES) {
        return Err(budget("samples", n, limits::SAMPLES));
    }
    let base = match &sc.cdata {
        ScatterColor::PerPoint(_) if relaxed() => Paint::Rgb([0.0, 0.0, 0.0]),
        ScatterColor::PerPoint(_) => return Err(reject("scatter_colors")),
        ScatterColor::Rgb(c) => Paint::Rgb(*c),
        ScatterColor::Indexed(v) => {
            if !is3 && !relaxed() {
                return Err(reject("unsupported_color"));
            }
            Paint::Rgb(index_color(*v, st).ok_or_else(|| reject("unsupported_color"))?)
        }
    };
    let idx = sample(&sc.x, &sc.y, &sc.z, sample_limit())
        .ok_or_else(|| budget("segment_anchors", n, limits::SAMPLES))?;
    let edge = match sc.marker_edge {
        Paint::Auto | Paint::Flat => base,
        p => p,
    };
    let face = match sc.marker_face {
        Paint::Flat => base,
        Paint::Auto => Paint::Rgb([1.0, 1.0, 1.0]),
        p => p,
    };
    let (Some(edge), Some(face)) = (rgb_or_none(edge), rgb_or_none(face)) else {
        return Err(reject("unsupported_color"));
    };
    marker_ok(&sc.marker)?;
    if !(sc.size_data.len() == 1 || sc.size_data.len() == n)
        || sc.size_data.iter().any(|v| !v.is_finite() || *v < 0.0)
    {
        return Err(reject_with("invalid_data", "property", "sizedata"));
    }
    let sizes = if sc.size_data.len() == 1 {
        sc.size_data.clone()
    } else {
        pick(&sc.size_data, &idx)
    };
    let mut s = Json::obj()
        .with("kind", "scatter")
        .with("x", pick(&sc.x, &idx))
        .with("y", pick(&sc.y, &idx))
        .with("line_color", "none")
        .with("marker_edge_color", edge)
        .with("marker_face_color", face)
        .with("marker_face_auto", sc.marker_face == Paint::Auto)
        .with("line_style", "none")
        .with("line_width", sc.line_width)
        .with("marker", sc.marker.as_str())
        .with("marker_size", 6.0)
        .with("display_name", sc.display_name.as_str())
        .with("base_value", 0.0)
        .with("decimated", idx.len() < n)
        .with("original_points", n)
        .with("rendered_points", idx.len())
        .with(
            "source_indices",
            idx.iter().map(|&i| i as f64).collect::<Vec<f64>>(),
        );
    if is3 {
        s.set("z", pick(&sc.z, &idx));
    }
    s.set("sizes", sizes);
    s.set("size_units", "points-squared");
    if let ScatterColor::PerPoint(c) = &sc.cdata {
        let mut table = vec![f64::NAN; idx.len() * 3];
        for (k, &i) in idx.iter().enumerate() {
            let rgb = if c.cols() == 3 && c.rows() == n {
                Some([c.at(i, 0), c.at(i, 1), c.at(i, 2)])
            } else if c.numel() == n {
                index_color(c.data[i], st)
            } else {
                None
            };
            if let Some(rgb) = rgb {
                for (ch, v) in rgb.iter().enumerate() {
                    table[k + ch * idx.len()] = *v;
                }
            }
        }
        s.set("point_colors", descriptor(&[idx.len(), 3], &table));
        s.set(
            "point_edge",
            matches!(sc.marker_edge, Paint::Auto | Paint::Flat),
        );
        s.set("point_face", sc.marker_face == Paint::Flat);
    } else if is3 {
        let (encoding, data) = match &sc.cdata {
            ScatterColor::Rgb(c) => ("truecolor", descriptor(&[1, 3], c)),
            ScatterColor::Indexed(v) => ("indexed", descriptor(&[1, 1], &[*v])),
            ScatterColor::PerPoint(_) => unreachable!("handled above"),
        };
        s.set(
            "color_data",
            Json::obj()
                .with("encoding", encoding)
                .with("mapping", "scaled")
                .with("cdata_class", "double")
                .with("data", data),
        );
    }
    Ok(s)
}

fn alpha_descriptor(v: f64) -> Json {
    Json::obj()
        .with("mode", "constant")
        .with("association", "constant")
        .with("value", v)
}

fn surface_color(p: Paint, c: &Array, mapping: &str, association: &str) -> R<Json> {
    Ok(match p {
        Paint::None => Json::obj()
            .with("mode", "none")
            .with("association", "constant"),
        Paint::Rgb(rgb) => {
            if !valid_rgb(rgb) {
                return Err(reject("unsupported_color"));
            }
            Json::obj()
                .with("mode", "constant")
                .with("association", "constant")
                .with("rgb", rgb)
        }
        Paint::Flat | Paint::Interp => {
            let (mode, association) = if p == Paint::Interp {
                ("interp", "vertex")
            } else {
                ("flat", association)
            };
            Json::obj()
                .with("mode", mode)
                .with("association", association)
                .with(
                    "encoding",
                    if c.pages() == 3 {
                        "truecolor"
                    } else {
                        "indexed"
                    },
                )
                .with("mapping", mapping)
                .with("cdata_class", c.class.name())
                .with("data", array_descriptor(c))
        }
        Paint::Auto => return Err(reject("unsupported_surface")),
    })
}

fn surface_record(s: &crate::model::Surface) -> R<Json> {
    let z = &s.z;
    let (rows, cols) = (z.rows(), z.cols());
    let n = z.numel();
    if z.dims.len() != 2 || rows < 2 || cols < 2 {
        return Err(reject_with("invalid_data", "property", "zdata"));
    }
    if over(n, limits::SURFACE_VERTICES, raster_limits::SURFACE_VERTICES) {
        return Err(budget("surface_vertices", n, limits::SURFACE_VERTICES));
    }
    let layout_of = |a: &Array, len: usize, name: &str| -> R<&'static str> {
        if a.is_vector() && a.numel() == len {
            Ok("vector")
        } else if a.dims == z.dims {
            Ok("matrix")
        } else {
            Err(reject_with("invalid_data", "property", name.to_string()))
        }
    };
    let xl = layout_of(&s.x, cols, "xdata")?;
    let yl = layout_of(&s.y, rows, "ydata")?;
    for (v, key) in [(s.face_alpha, "facealpha"), (s.edge_alpha, "edgealpha")] {
        if !(v.is_finite() && (0.0..=1.0).contains(&v)) {
            return Err(reject_with("transparency", "property", key));
        }
    }
    let c = &s.c;
    if !(c.dims == z.dims || (c.dims.len() == 3 && c.dims[..2] == z.dims[..] && c.dims[2] == 3)) {
        return Err(reject_with("invalid_data", "property", "cdata"));
    }
    if c.pages() == 3 {
        let max = match c.class {
            crate::value::NumClass::UInt8 => 255.0,
            crate::value::NumClass::UInt16 => 65535.0,
            k if k.is_float() => 1.0,
            _ => return Err(reject("unsupported_color")),
        };
        if c.data.iter().any(|v| *v < 0.0 || *v > max) {
            return Err(reject("unsupported_color"));
        }
    }
    let mapping = if s.cdata_direct { "direct" } else { "scaled" };
    let face = surface_color(s.face_color, c, mapping, "cell")?;
    let edge = surface_color(s.edge_color, c, mapping, "vertex")?;
    if !matches!(s.mesh_style.as_str(), "both" | "row" | "column") {
        return Err(reject_with("unsupported_surface", "property", "meshstyle"));
    }
    if !relaxed() && s.marker != "none" {
        return Err(reject_with("unsupported_surface", "property", "marker"));
    }
    let xv = |i: usize| {
        if xl == "vector" {
            s.x.data[i / rows]
        } else {
            s.x.data[i]
        }
    };
    let yv = |i: usize| {
        if yl == "vector" {
            s.y.data[i % rows]
        } else {
            s.y.data[i]
        }
    };
    let valid: Vec<bool> = (0..n)
        .map(|i| z.data[i].is_finite() && xv(i).is_finite() && yv(i).is_finite())
        .collect();
    let mut origins = Vec::new();
    for col in 0..cols - 1 {
        for row in 0..rows - 1 {
            let i = row + col * rows;
            if valid[i] && valid[i + 1] && valid[i + rows] && valid[i + rows + 1] {
                origins.push(i);
            }
        }
    }
    let mut edges: Vec<[usize; 2]> = Vec::new();
    if s.mesh_style != "row" {
        for col in 0..cols {
            for row in 0..rows - 1 {
                let i = row + col * rows;
                if valid[i] && valid[i + 1] {
                    edges.push([i, i + 1]);
                }
            }
        }
    }
    if s.mesh_style != "column" {
        for col in 0..cols - 1 {
            for row in 0..rows {
                let i = row + col * rows;
                if valid[i] && valid[i + rows] {
                    edges.push([i, i + rows]);
                }
            }
        }
    }
    // Nonfinite CData only where no rendered face/edge reads it.
    let channels = c.pages();
    let finite_at = |i: usize| (0..channels).all(|ch| c.data[i + ch * n].is_finite());
    let read_ok = |mode: Paint, face: bool| match mode {
        Paint::Flat => {
            if face {
                origins.iter().all(|&i| finite_at(i))
            } else {
                edges.iter().all(|e| finite_at(e[0]))
            }
        }
        Paint::Interp => {
            if face {
                origins.iter().all(|&i| {
                    [i, i + 1, i + rows, i + rows + 1]
                        .iter()
                        .all(|&k| finite_at(k))
                })
            } else {
                edges.iter().all(|e| finite_at(e[0]) && finite_at(e[1]))
            }
        }
        _ => true,
    };
    if !read_ok(s.face_color, true) || !read_ok(s.edge_color, false) {
        return Err(reject("unsupported_color"));
    }
    let mut edge_values = Vec::with_capacity(edges.len() * 2);
    edge_values.extend(edges.iter().map(|e| e[0] as f64));
    edge_values.extend(edges.iter().map(|e| e[1] as f64));
    let mut rec = Json::obj()
        .with("cdata_class", c.class.name())
        .with("cdata_mapping", mapping)
        .with("cdata", array_descriptor(c))
        .with("kind", "surface")
        .with("shape", [rows as f64, cols as f64])
        .with("coordinate_layout", Json::obj().with("x", xl).with("y", yl))
        .with("x", array_descriptor(&s.x))
        .with("y", array_descriptor(&s.y))
        .with("z", array_descriptor(z))
        .with("face_color", face)
        .with("edge_color", edge)
        .with("cell_color_owner", "row-column-origin")
        .with("edge_color_owner", "first-vertex")
        .with("mesh_style", s.mesh_style.as_str())
        .with("line_style", s.line_style.as_str())
        .with("line_width", s.line_width)
        .with("display_name", s.display_name.as_str())
        .with("decimated", false)
        .with("original_points", n)
        .with("rendered_points", n)
        .with(
            "cell_origins",
            origins.iter().map(|&i| i as f64).collect::<Vec<f64>>(),
        )
        .with("edge_indices", descriptor(&[edges.len(), 2], &edge_values))
        .with("index_base", 0.0)
        .with("triangle_count", 2 * origins.len());
    if s.face_alpha != 1.0 {
        rec.set("face_alpha", alpha_descriptor(s.face_alpha));
    }
    if s.edge_alpha != 1.0 {
        rec.set("edge_alpha", alpha_descriptor(s.edge_alpha));
    }
    Ok(rec)
}

/// One colour for a whole 2-D patch (port of `__mf_patch_color__`).
fn patch_color(p: Paint, patch: &crate::model::Patch, st: &AxesState) -> R<Json> {
    match p {
        Paint::None => Ok(Json::from("none")),
        Paint::Rgb(c) => rgb_or_none(Paint::Rgb(c)).ok_or_else(|| reject("unsupported_color")),
        Paint::Interp => Err(reject("interpolated_color")),
        Paint::Auto => Err(reject("unsupported_color")),
        Paint::Flat => {
            let c = &patch.fvcdata;
            if c.is_empty() {
                return Err(reject("unsupported_color"));
            }
            if c.rows() != 1 || !(c.cols() == 1 || c.cols() == 3) {
                return Err(reject("patch_colors"));
            }
            if c.cols() == 3 {
                let rgb = [c.data[0], c.data[1], c.data[2]];
                return rgb_or_none(Paint::Rgb(rgb)).ok_or_else(|| reject("unsupported_color"));
            }
            if patch.cdata_direct {
                return Err(reject("unsupported_color"));
            }
            index_color(c.data[0], st)
                .map(Json::from)
                .ok_or_else(|| reject("unsupported_color"))
        }
    }
}

fn patch_record(
    p: &crate::model::Patch,
    bar: Option<&crate::scene::BarInfo>,
    leaf: &Leaf,
    st: &AxesState,
) -> R<Json> {
    let v = &p.vertices;
    let n = v.rows();
    if v.dims.len() != 2 || n == 0 || !(v.cols() == 2 || v.cols() == 3) {
        return Err(reject_with("invalid_data", "property", "vertices"));
    }
    if over(n, limits::PATCH_VERTICES, raster_limits::PATCH_VERTICES) {
        return Err(budget("patch_vertices", n, limits::PATCH_VERTICES));
    }
    let f = &p.faces;
    if f.is_empty() || f.dims.len() != 2 {
        return Err(reject_with("invalid_data", "property", "faces"));
    }
    let fan = f.rows() * f.cols().saturating_sub(2);
    if !relaxed() && fan > limits::PATCH_TRIANGLES {
        return Err(budget("patch_triangles", fan, limits::PATCH_TRIANGLES));
    }
    if v.cols() == 3 && v.column(2).iter().any(|z| *z != 0.0) {
        return Err(reject_with("unsupported_patch", "property", "vertices"));
    }
    for r in 0..f.rows() {
        let mut ended = false;
        for c in 0..f.cols() {
            let k = f.at(r, c);
            if k.is_nan() {
                ended = true;
            } else if ended || k < 1.0 || k > n as f64 || k.fract() != 0.0 {
                return Err(reject_with("invalid_data", "property", "faces"));
            }
        }
    }
    for (a, key) in [(p.face_alpha, "facealpha"), (p.edge_alpha, "edgealpha")] {
        if !(a.is_finite() && (0.0..=1.0).contains(&a)) {
            return Err(reject_with("transparency", "property", key));
        }
    }
    if !relaxed() && p.marker != "none" {
        return Err(reject_with("unsupported_patch", "property", "marker"));
    }
    let face = patch_color(p.face_color, p, st)?;
    let edge = patch_color(p.edge_color, p, st)?;
    let name = leaf
        .group_name
        .clone()
        .unwrap_or_else(|| p.display_name.clone());
    let faces: Vec<f64> = f.data.iter().map(|k| k - 1.0).collect();
    let mut s = Json::obj()
        .with("kind", "patch2d")
        .with("role", leaf.role.unwrap_or("patch"))
        .with("vertices", descriptor(&[n, 2], &v.data[..2 * n]))
        .with("faces", descriptor(&f.dims, &faces))
        .with("index_base", 0.0)
        .with("face_color", face)
        .with("edge_color", edge)
        .with("line_style", p.line_style.as_str())
        .with("line_width", p.line_width)
        .with("display_name", name)
        .with("decimated", false)
        .with("original_points", n)
        .with("rendered_points", n);
    if p.face_alpha != 1.0 {
        s.set("face_alpha", alpha_descriptor(p.face_alpha));
    }
    if p.edge_alpha != 1.0 {
        s.set("edge_alpha", alpha_descriptor(p.edge_alpha));
    }
    if let Some(b) = bar
        && b.positions.len() == f.rows()
        && b.values.len() == f.rows()
    {
        s.set(
            "bar",
            Json::obj()
                .with("horizontal", b.horizontal)
                .with("layout", b.layout)
                .with("width", if b.width.is_finite() { b.width } else { 0.0 })
                .with(
                    "base_value",
                    if b.base_value.is_finite() {
                        b.base_value
                    } else {
                        0.0
                    },
                )
                .with("positions", b.positions.clone())
                .with("values", b.values.clone()),
        );
    }
    Ok(s)
}

/// The resolved colours of a patch in the relaxed path: `none`, `constant`
/// `rgb`, or a `table` of RGB rows per `face` / per vertex (`interp`, or
/// `first_vertex` for flat colours given per vertex).
fn raster_patch_paint(paint: Paint, p: &crate::model::Patch, st: &AxesState) -> R<Json> {
    let c = &p.fvcdata;
    let (faces, verts) = (p.faces.rows(), p.vertices.rows());
    let none = || Json::obj().with("mode", "none");
    let constant = |rgb: Rgb| Json::obj().with("mode", "constant").with("rgb", rgb);
    match paint {
        Paint::None => Ok(none()),
        Paint::Rgb(rgb) => Ok(if valid_rgb(rgb) {
            constant(rgb)
        } else {
            none()
        }),
        Paint::Auto => Err(reject("unsupported_color")),
        Paint::Flat | Paint::Interp => {
            if c.is_empty() {
                return Err(reject("unsupported_color"));
            }
            let rgb_row = |r: usize| -> Rgb {
                if c.cols() == 3 {
                    [c.at(r, 0), c.at(r, 1), c.at(r, 2)]
                } else if p.cdata_direct {
                    let m = &st.colormap;
                    if c.at(r, 0).is_nan() {
                        return [f64::NAN; 3];
                    }
                    let k = (c.at(r, 0).trunc() as i64).saturating_sub(1);
                    m[k.clamp(0, m.len() as i64 - 1) as usize]
                } else {
                    index_color(c.at(r, 0), st).unwrap_or([f64::NAN; 3])
                }
            };
            let rows = c.rows();
            if rows == 1 {
                let only = rgb_row(0);
                return Ok(if only.iter().any(|v| v.is_nan()) {
                    none()
                } else {
                    constant(only)
                });
            }
            let mode = if paint == Paint::Interp && rows == verts {
                "interp"
            } else if rows == faces {
                "face"
            } else if rows == verts {
                "first_vertex"
            } else {
                return Err(reject("patch_colors"));
            };
            let mut table = vec![0.0; rows * 3];
            for r in 0..rows {
                let v = rgb_row(r);
                for ch in 0..3 {
                    table[r + ch * rows] = v[ch];
                }
            }
            Ok(Json::obj()
                .with("mode", mode)
                .with("table", descriptor(&[rows, 3], &table)))
        }
    }
}

/// A patch the wire contract cannot carry: any colours, 2-D or 3-D vertices.
fn raster_patch_record(p: &crate::model::Patch, leaf: &Leaf, st: &AxesState) -> R<Json> {
    let v = &p.vertices;
    let n = v.rows();
    if v.dims.len() != 2 || n == 0 || !(v.cols() == 2 || v.cols() == 3) {
        return Err(reject_with("invalid_data", "property", "vertices"));
    }
    if over(n, limits::PATCH_VERTICES, raster_limits::PATCH_VERTICES) {
        return Err(budget("patch_vertices", n, limits::PATCH_VERTICES));
    }
    let f = &p.faces;
    if f.is_empty()
        || f.dims.len() != 2
        || f.data
            .iter()
            .any(|k| k.is_finite() && (*k < 1.0 || *k > n as f64))
    {
        return Err(reject_with("invalid_data", "property", "faces"));
    }
    let faces: Vec<f64> = f.data.iter().map(|k| k - 1.0).collect();
    let name = leaf
        .group_name
        .clone()
        .unwrap_or_else(|| p.display_name.clone());
    let mut s = Json::obj()
        .with("kind", "patch_raster")
        .with("role", leaf.role.unwrap_or("patch"))
        .with("vertices", array_descriptor(v))
        .with("faces", descriptor(&f.dims, &faces))
        .with("face", raster_patch_paint(p.face_color, p, st)?)
        .with("edge", raster_patch_paint(p.edge_color, p, st)?)
        .with("line_style", p.line_style.as_str())
        .with("line_width", p.line_width)
        .with("display_name", name)
        .with("decimated", false)
        .with("original_points", n)
        .with("rendered_points", n);
    if p.face_alpha != 1.0 {
        s.set("face_alpha", alpha_descriptor(p.face_alpha));
    }
    if p.edge_alpha != 1.0 {
        s.set("edge_alpha", alpha_descriptor(p.edge_alpha));
    }
    Ok(s)
}

fn text_record(t: &crate::model::Text) -> R<Json> {
    if !relaxed() && !matches!(t.units.as_str(), "data" | "normalized") {
        return Err(reject_with("unsupported_text", "property", "units"));
    }
    if !relaxed() && !matches!(t.interpreter.as_str(), "tex" | "none") {
        return Err(reject_with("unsupported_text", "property", "interpreter"));
    }
    if !(t.position[0].is_finite() && t.position[1].is_finite()) {
        return Err(reject_with("invalid_data", "property", "text.position"));
    }
    if !t.rotation.is_finite() {
        return Err(reject_with("invalid_data", "property", "text.rotation"));
    }
    let lines: Vec<String> = t
        .lines
        .iter()
        .flat_map(|l| l.split('\n').map(str::to_string).collect::<Vec<_>>())
        .collect();
    if !relaxed() && lines.len() > limits::TEXT_LINES {
        return Err(budget("text_lines", lines.len(), limits::TEXT_LINES));
    }
    let chars: usize = lines.iter().map(|l| l.chars().count()).sum();
    if !relaxed() && chars > limits::TEXT_CHARS {
        return Err(budget("text_chars", chars, limits::TEXT_CHARS));
    }
    let (Some(bg), Some(edge)) = (rgb_or_none(t.background), rgb_or_none(t.edge_color)) else {
        return Err(reject("unsupported_color"));
    };
    if !valid_rgb(t.color) {
        return Err(reject("unsupported_color"));
    }
    if !(t.font_size.is_finite() && t.font_size > 0.0 && t.margin.is_finite() && t.margin >= 0.0) {
        return Err(reject_with("invalid_data", "property", "text.fontsize"));
    }
    let weight = if matches!(t.font_weight.as_str(), "bold" | "demi") {
        "bold"
    } else {
        "normal"
    };
    let angle = if matches!(t.font_angle.as_str(), "italic" | "oblique") {
        "italic"
    } else {
        "normal"
    };
    let mut extra = Json::obj();
    if relaxed() && t.position[2].is_finite() && t.position[2] != 0.0 {
        extra.set("z", t.position[2]);
    }
    let record = Json::obj()
        .with("kind", "text")
        .with("role", "text")
        .with("units", t.units.as_str())
        .with("position", [t.position[0], t.position[1]])
        .with("lines", lines)
        .with("interpreter", t.interpreter.as_str())
        .with("horizontal_alignment", t.horizontal_alignment.as_str())
        .with("vertical_alignment", t.vertical_alignment.as_str())
        .with("rotation", t.rotation.rem_euclid(360.0))
        .with("font_size", t.font_size)
        .with("font_weight", weight)
        .with("font_angle", angle)
        .with("color", t.color)
        .with("background_color", bg)
        .with("edge_color", edge)
        .with("margin", t.margin)
        .with("line_style", t.line_style.as_str())
        .with("line_width", t.line_width)
        .with("clipping", t.clipping)
        .with("display_name", "")
        .with("decimated", false)
        .with("original_points", 0.0)
        .with("rendered_points", 0.0);
    Ok(match (record, extra) {
        (mut r, Json::Obj(items)) => {
            for (k, v) in items {
                r.set(&k, v);
            }
            r
        }
        (r, _) => r,
    })
}

fn image_record(img: &crate::model::Image) -> R<Json> {
    let c = &img.cdata;
    let (rows, cols) = (c.rows(), c.cols());
    let n = rows * cols;
    if over(n, limits::IMAGE_PIXELS, raster_limits::IMAGE_PIXELS) {
        return Err(budget("image_pixels", n, limits::IMAGE_PIXELS));
    }
    let class_ok = matches!(
        c.class,
        crate::value::NumClass::Double
            | crate::value::NumClass::Single
            | crate::value::NumClass::UInt8
            | crate::value::NumClass::UInt16
    );
    if c.is_empty()
        || !class_ok
        || !(c.dims.len() == 2 || (c.dims.len() == 3 && c.dims[2] == 3))
        || c.data.iter().any(|v| !v.is_finite())
    {
        return Err(reject_with("invalid_data", "property", "image.cdata"));
    }
    let (x, y) = (img.x, img.y);
    if x.iter().chain(&y).any(|v| !v.is_finite()) {
        return Err(reject_with("invalid_data", "property", "image.coordinates"));
    }
    if (cols == 1 && x[0] != x[1])
        || (rows == 1 && y[0] != y[1])
        || (cols > 1 && x[0] == x[1])
        || (rows > 1 && y[0] == y[1])
    {
        return Err(reject_with(
            "unsupported_object",
            "type",
            "image_coordinates",
        ));
    }
    Ok(Json::obj()
        .with("kind", "image")
        .with("shape", [rows as f64, cols as f64])
        .with("x", x)
        .with("y", y)
        .with("cdata", array_descriptor(c))
        .with(
            "encoding",
            if c.dims.len() == 3 {
                "truecolor"
            } else {
                "indexed"
            },
        )
        .with(
            "mapping",
            if img.cdata_direct { "direct" } else { "scaled" },
        )
        .with("cdata_class", c.class.name())
        .with("pixel_count", n)
        .with("display_name", img.display_name.as_str())
        .with("line_style", "none")
        .with("decimated", false)
        .with("original_points", 0.0)
        .with("rendered_points", 0.0))
}

#[allow(clippy::too_many_arguments)]
fn contour_line_record(
    level: f64,
    x: &[f64],
    y: &[f64],
    color: Paint,
    line_style: &str,
    line_width: f64,
    leaf: &Leaf,
    st: &AxesState,
) -> R<Json> {
    // Port of __mf_figure_contours__: a finite path is a closed polygon whose
    // closing vertex is appended; a terminal NaN keeps the path open.
    let n = x.len();
    if over(n + 1, limits::SAMPLES, raster_limits::SAMPLES) {
        return Err(budget("samples", n + 1, limits::SAMPLES));
    }
    if y.len() != n || !level.is_finite() {
        return Err(reject_with("invalid_data", "property", "contour.level"));
    }
    let closed = n >= 2 && x.iter().chain(y).all(|v| v.is_finite());
    let (mut x, mut y) = (x.to_vec(), y.to_vec());
    if closed {
        x.push(x[0]);
        y.push(y[0]);
    }
    let (ink, contour_color) = match color {
        Paint::Flat => {
            let rgb = index_color(level, st).ok_or_else(|| reject("unsupported_color"))?;
            (
                Json::from(rgb),
                Json::obj()
                    .with("mode", "flat")
                    .with("association", "level")
                    .with("encoding", "indexed")
                    .with("mapping", "scaled")
                    .with("cdata_class", "double")
                    .with("data", descriptor(&[1, 1], &[level])),
            )
        }
        Paint::None => (
            Json::from("none"),
            Json::obj()
                .with("mode", "none")
                .with("association", "constant"),
        ),
        Paint::Rgb(c) if valid_rgb(c) => (
            Json::from(c),
            Json::obj()
                .with("mode", "constant")
                .with("association", "constant")
                .with("rgb", c),
        ),
        _ => return Err(reject("unsupported_color")),
    };
    let m = x.len();
    Ok(Json::obj()
        .with("kind", "line")
        .with("role", "contour")
        .with("level", level)
        .with("contour_closed", closed)
        .with("x", x)
        .with("y", y)
        .with("line_color", ink)
        .with("marker", "none")
        .with("marker_size", 0.0)
        .with("marker_edge_color", "none")
        .with("marker_face_color", "none")
        .with("marker_face_auto", false)
        .with("line_style", line_style)
        .with("line_width", line_width)
        .with("display_name", leaf.group_name.clone().unwrap_or_default())
        .with("decimated", false)
        .with("original_points", m)
        .with("rendered_points", m)
        .with(
            "source_indices",
            (0..m).map(|i| i as f64).collect::<Vec<f64>>(),
        )
        .with("contour_color", contour_color))
}

/// Port of `__mf_patch3d__` for contourf rings (`native-ring-v1`) together
/// with `__mf_figure_contour_ring__`.
fn contourf_record(
    level: f64,
    vertices: &[[f64; 2]],
    face: &[f64],
    edge: Paint,
    line_style: &str,
    line_width: f64,
    leaf: &Leaf,
) -> R<Json> {
    let fail = |detail: &str| Reject {
        code: "contour_filled",
        args: Json::obj()
            .with("property", "vertices")
            .with("detail", detail),
    };
    if vertices.is_empty()
        || vertices
            .iter()
            .any(|v| !(v[0].is_finite() && v[1].is_finite()))
    {
        return Err(fail("holes_or_nonfinite"));
    }
    if over(vertices.len(), limits::VERTICES, raster_limits::VERTICES) {
        return Err(budget("vertices", vertices.len(), limits::VERTICES));
    }
    if face.is_empty() {
        return Err(Reject {
            code: "contour_filled",
            args: Json::obj()
                .with("property", "faces")
                .with("detail", "compound_faces"),
        });
    }
    let mut ring: Vec<usize> = Vec::with_capacity(face.len());
    let mut ended = false;
    for &k in face {
        if k.is_nan() {
            ended = true;
            continue;
        }
        if ended || k.fract() != 0.0 || k < 1.0 || k > vertices.len() as f64 {
            return Err(reject_with("invalid_index", "property", "faces"));
        }
        ring.push(k as usize - 1);
    }
    // Drop consecutive duplicates and a repeated closing vertex.
    let mut clean: Vec<usize> = Vec::with_capacity(ring.len());
    for i in ring {
        if clean.last().is_none_or(|&j| vertices[j] != vertices[i]) {
            clean.push(i);
        }
    }
    if clean.len() > 1 && vertices[clean[0]] == vertices[*clean.last().unwrap_or(&0)] {
        clean.pop();
    }
    let points: Vec<[f64; 2]> = clean.iter().map(|&i| vertices[i]).collect();
    let second = points.iter().position(|p| *p != points[0]);
    let no_area = match second {
        None => true,
        Some(s) => points.iter().all(|p| *p == points[0] || *p == points[s]),
    };
    let background = level.is_nan();
    let face_color = if background {
        // A background hole repaints the axes colour.
        Json::obj()
            .with("mode", "constant")
            .with("association", "constant")
            .with("rgb", [1.0, 1.0, 1.0])
    } else {
        Json::obj()
            .with("mode", "flat")
            .with("association", "face")
            .with("encoding", "indexed")
            .with("mapping", "scaled")
            .with("cdata_class", "double")
            .with("data", descriptor(&[1, 1], &[level]))
    };
    let edge_color = match edge {
        Paint::None => Json::obj()
            .with("mode", "none")
            .with("association", "constant"),
        Paint::Rgb(c) if valid_rgb(c) => Json::obj()
            .with("mode", "constant")
            .with("association", "constant")
            .with("rgb", c),
        Paint::Flat if !background => Json::obj()
            .with("mode", "flat")
            .with("association", "face")
            .with("encoding", "indexed")
            .with("mapping", "scaled")
            .with("cdata_class", "double")
            .with("data", descriptor(&[1, 1], &[level])),
        _ => Json::obj()
            .with("mode", "none")
            .with("association", "constant"),
    };
    let n = vertices.len();
    let mut values = Vec::with_capacity(3 * n);
    values.extend(vertices.iter().map(|v| v[0]));
    values.extend(vertices.iter().map(|v| v[1]));
    values.extend(std::iter::repeat_n(0.0, n));
    let ring_values: Vec<f64> = clean.iter().map(|&i| i as f64).collect();
    let source: Vec<f64> = face.iter().map(|k| k - 1.0).collect();
    let triangles = if no_area {
        0
    } else {
        clean.len().saturating_sub(2)
    };
    Ok(Json::obj()
        .with("kind", "patch3d")
        .with("role", "contourf")
        .with("vertices", descriptor(&[n, 3], &values))
        .with("faces", descriptor(&[1, clean.len()], &ring_values))
        .with("index_base", 0.0)
        .with("triangulation", "native-ring-v1")
        .with("triangle_count", triangles)
        .with("face_color", face_color)
        .with("edge_color", edge_color)
        .with("line_style", line_style)
        .with("line_width", line_width)
        .with("display_name", leaf.group_name.clone().unwrap_or_default())
        .with("decimated", false)
        .with("original_points", n)
        .with("rendered_points", n)
        .with(
            "level",
            if background {
                Json::Null
            } else {
                Json::from(level)
            },
        )
        .with("no_area", no_area)
        .with("background_hole", background)
        .with("source_faces", descriptor(&[1, face.len()], &source)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_does_not_leave_the_thread_relaxed() {
        let caught = std::panic::catch_unwind(|| {
            relaxed_while(|| {
                assert!(relaxed());
                panic!("while serializing");
            })
        });
        assert!(caught.is_err());
        assert!(!relaxed());
    }
}
