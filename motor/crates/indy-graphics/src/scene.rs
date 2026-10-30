//! Resolution of an axes for drawing: drawable leaves (groups flattened the
//! way IndyMAT's extractor flattens Octave hggroups), data extents and the
//! automatic properties (limits, ticks, colour limits, aspect, camera).

use crate::color::Colormap;
use crate::contour::{self, Coord};
use crate::graphics::Graphics;
use crate::layout::{self, Camera};
use crate::model::{
    Axes, GridCoord, GroupKind, Handle, Image, Kind, Line, Mode, Paint, Patch, Scale, Scatter,
    Surface, Text,
};
use crate::value::{Array, finite_range};

/// Per-face data of a bar series, for data tips.
#[derive(Debug, Clone)]
pub(crate) struct BarInfo {
    pub horizontal: bool,
    pub layout: &'static str,
    pub width: f64,
    pub base_value: f64,
    pub positions: Vec<f64>,
    pub values: Vec<f64>,
}

#[derive(Debug, Clone)]
pub(crate) enum LeafObj {
    /// kind is `line`, `stem` or `stairs`; the f64 is the base value.
    Line(Line, &'static str, f64),
    /// A bar baseline spanning the current limits.
    Baseline {
        horizontal: bool,
        base: f64,
    },
    Scatter(Scatter),
    Surface(Surface),
    Patch(Patch, Option<BarInfo>),
    Text(Text),
    Image(Image),
    ContourLine {
        level: f64,
        x: Vec<f64>,
        y: Vec<f64>,
        color: Paint,
        line_style: String,
        line_width: f64,
    },
    ContourFill {
        level: f64,
        vertices: Vec<[f64; 2]>,
        face: Vec<f64>,
        edge: Paint,
        line_style: String,
        line_width: f64,
    },
    /// A contour line of `contour3`/`surfc`/`meshc` (module `surf2`).
    Contour3 {
        level: f64,
        x: Vec<f64>,
        y: Vec<f64>,
        z: Vec<f64>,
        color: Paint,
        line_style: String,
        line_width: f64,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct Leaf {
    /// Group role (`bar`, `area`, `contour`, `contourf`) or `None` for a
    /// bare object.
    pub role: Option<&'static str>,
    /// Display name of the owning group (flattened children use it).
    pub group_name: Option<String>,
    /// Index of the owning contour group among the axes' groups.
    pub group: Option<usize>,
    /// Measured Octave quirk: `stairs` groups and single-series `area`
    /// groups pull the limits to include [0,1] in x (and in y).
    pub unit_box: [bool; 2],
    pub obj: LeafObj,
}

impl Graphics {
    /// Drawable leaves of an axes in drawing order (oldest first).
    pub(crate) fn leaves(&self, ax: Handle) -> Vec<Leaf> {
        let mut out = Vec::new();
        let Ok(o) = self.obj(ax) else { return out };
        let mut group_index = 0;
        let mut polar = false;
        for &h in &o.children {
            let Some(child) = self.objects.get(&h) else {
                continue;
            };
            if !child.visible {
                continue;
            }
            let bare = |obj| Leaf {
                role: None,
                group_name: None,
                group: None,
                unit_box: [false; 2],
                obj,
            };
            match &child.kind {
                Kind::Line(l) => out.push(bare(LeafObj::Line(l.clone(), "line", 0.0))),
                Kind::Scatter(s) => out.push(bare(LeafObj::Scatter(s.clone()))),
                Kind::Surface(s) => out.push(bare(LeafObj::Surface(s.clone()))),
                Kind::Patch(p) => out.push(bare(LeafObj::Patch(p.clone(), None))),
                Kind::Text(t) => out.push(bare(LeafObj::Text(t.clone()))),
                Kind::Image(i) => out.push(bare(LeafObj::Image(i.clone()))),
                Kind::Group(g) => {
                    let name = Some(g.display_name.clone());
                    let flat = |role, obj| Leaf {
                        role: Some(role),
                        group_name: name.clone(),
                        group: None,
                        unit_box: [false; 2],
                        obj,
                    };
                    match &g.kind {
                        GroupKind::Stem(line, base) => {
                            let mut l = line.clone();
                            l.display_name = g.display_name.clone();
                            out.push(bare(LeafObj::Line(l, "stem", *base)));
                        }
                        GroupKind::Stairs(line) => {
                            let mut l = line.clone();
                            l.display_name = g.display_name.clone();
                            let mut leaf = bare(LeafObj::Line(l, "stairs", 0.0));
                            leaf.unit_box = [true, true];
                            out.push(leaf);
                        }
                        GroupKind::Bar { .. } => {
                            let (patch, info, baseline) = bar_patch(&g.kind);
                            out.push(flat("bar", LeafObj::Patch(patch, Some(info))));
                            if let Some(b) = baseline {
                                out.push(flat("bar", b));
                            }
                        }
                        GroupKind::Histogram(hg) => {
                            out.push(flat(
                                if hg.stairs { "histogram" } else { "bar" },
                                histogram_leaf(hg),
                            ));
                        }
                        GroupKind::Area {
                            single, base_value, ..
                        } => {
                            let mut leaf = flat("area", LeafObj::Patch(area_patch(&g.kind), None));
                            leaf.unit_box = [*single, *single && *base_value == 0.0];
                            out.push(leaf);
                        }
                        GroupKind::Contour { .. } => {
                            for mut leaf in contour_leaves(&g.kind) {
                                leaf.group_name = name.clone();
                                leaf.group = Some(group_index);
                                out.push(leaf);
                            }
                            group_index += 1;
                        }
                        GroupKind::Contour3(c) => {
                            let flat = contour_leaves(&c.contour);
                            for mut leaf in crate::surf2::contour3_leaves(c, flat) {
                                leaf.group_name = name.clone();
                                leaf.group = Some(group_index);
                                out.push(leaf);
                            }
                            group_index += 1;
                        }
                        GroupKind::ErrorBar(e) => {
                            out.extend(crate::lines2::errorbar_leaves(e, &g.display_name))
                        }
                        GroupKind::Quiver(q) => {
                            out.extend(crate::lines2::quiver_leaves(q, &g.display_name))
                        }
                        GroupKind::FunctionLine(f) => {
                            let mut l = f.line.clone();
                            l.display_name = g.display_name.clone();
                            out.push(bare(LeafObj::Line(l, "line", 0.0)));
                        }
                        GroupKind::PolarGrid => polar = true,
                    }
                }
                Kind::Figure(_) | Kind::Axes(_) | Kind::Legend(_) | Kind::Colorbar(_) => {}
            }
        }
        if polar {
            let grid = crate::lines2::polar_leaves(&out);
            out.extend(grid);
        }
        out
    }

    /// Resolved drawing state of an axes.
    pub(crate) fn axes_state(&self, ax: Handle) -> Option<AxesState> {
        let a = self.axes_ref(ax).ok()?;
        let fig = self.ancestor_figure(ax).ok()?;
        let figure = self.figure_ref(fig).ok()?;
        let leaves = self.leaves(ax);
        Some(resolve(
            a,
            &leaves,
            figure.colormap.clone(),
            figure.position,
        ))
    }
}

/// Resolved automatic state of one axes.
pub(crate) struct AxesState {
    pub lim: [[f64; 2]; 3],
    pub tick: [Vec<f64>; 3],
    pub ticklabel: [Vec<String>; 3],
    pub dar: [f64; 3],
    pub pbar: [f64; 3],
    pub clim: [f64; 2],
    pub colormap: Colormap,
    pub camera: Camera,
    pub dimension: u8,
    /// Pixel size of the figure.
    pub figure_size: [f64; 2],
}

#[derive(Default)]
struct Extents {
    axis: [Option<[f64; 2]>; 3],
    color: Option<[f64; 2]>,
    has_z_object: bool,
    has_image: bool,
    any: bool,
}

impl Extents {
    fn add(&mut self, i: usize, values: &[f64]) {
        if let Some(r) = finite_range(values) {
            self.axis[i] = Some(union(self.axis[i], r));
        }
    }

    fn add_color(&mut self, values: &[f64]) {
        if let Some(r) = finite_range(values) {
            self.color = Some(union(self.color, r));
        }
    }
}

fn union(a: Option<[f64; 2]>, b: [f64; 2]) -> [f64; 2] {
    match a {
        None => b,
        Some(a) => [a[0].min(b[0]), a[1].max(b[1])],
    }
}

fn extents(leaves: &[Leaf]) -> Extents {
    let mut e = Extents::default();
    for leaf in leaves {
        e.any = true;
        for (i, on) in leaf.unit_box.iter().enumerate() {
            if *on {
                e.add(i, &[0.0, 1.0]);
            }
        }
        match &leaf.obj {
            LeafObj::Line(l, kind, base) => {
                e.add(0, &l.x);
                e.add(1, &l.y);
                if !l.z.is_empty() {
                    e.add(2, &l.z);
                    e.has_z_object = true;
                }
                match *kind {
                    "stem" => {
                        e.add(1, &[*base]);
                        e.add_color(&[0.0]);
                    }
                    // Measured: stairs groups always pull both limits to 0.
                    "stairs" => e.add_color(&[0.0]),
                    _ => {}
                }
            }
            LeafObj::Baseline { .. } => {}
            LeafObj::Scatter(s) => {
                e.add(0, &s.x);
                e.add(1, &s.y);
                if !s.z.is_empty() {
                    e.add(2, &s.z);
                    e.has_z_object = true;
                }
                // Measured: Octave's automatic CLim includes scatter CData,
                // even an RGB triple (scatter([..]) gives CLim [0 0.741]).
                match &s.cdata {
                    crate::model::ScatterColor::Rgb(c) => e.add_color(c),
                    crate::model::ScatterColor::Indexed(v) => e.add_color(&[*v]),
                    crate::model::ScatterColor::PerPoint(a) => e.add_color(&a.data),
                }
            }
            LeafObj::Surface(s) => {
                e.add(0, &s.x.data);
                e.add(1, &s.y.data);
                e.add(2, &s.z.data);
                e.has_z_object = true;
                if s.c.pages() == 1 {
                    e.add_color(&s.c.data);
                }
            }
            LeafObj::Patch(p, _) => {
                let v = &p.vertices;
                for i in 0..v.cols().min(3) {
                    e.add(i, v.column(i));
                }
                if v.cols() >= 3 && v.column(2).iter().any(|z| *z != 0.0) {
                    e.has_z_object = true;
                }
                if p.fvcdata.cols() == 1 && !p.fvcdata.is_empty() {
                    e.add_color(&p.fvcdata.data);
                } else {
                    // Measured: RGB/bar/area patches give CLim [-1 1].
                    e.add_color(&[0.0]);
                }
            }
            LeafObj::Text(_) => {}
            LeafObj::Image(img) => {
                let (rows, cols) = (img.cdata.rows(), img.cdata.cols());
                let half = |v: [f64; 2], n: usize| {
                    let step = if n > 1 {
                        (v[1] - v[0]) / (n - 1) as f64
                    } else {
                        1.0
                    };
                    [v[0] - step / 2.0, v[1] + step / 2.0]
                };
                e.add(0, &half(img.x, cols));
                e.add(1, &half(img.y, rows));
                e.has_image = true;
                if img.cdata.pages() == 1 && !img.cdata_direct {
                    e.add_color(&img.cdata.data);
                }
            }
            LeafObj::ContourLine { level, x, y, .. } => {
                e.add(0, x);
                e.add(1, y);
                e.add_color(&[*level]);
            }
            LeafObj::Contour3 { level, x, y, z, .. } => {
                e.add(0, x);
                e.add(1, y);
                e.add(2, z);
                e.has_z_object = true;
                e.add_color(&[*level]);
            }
            LeafObj::ContourFill {
                level, vertices, ..
            } => {
                let xs: Vec<f64> = vertices.iter().map(|v| v[0]).collect();
                let ys: Vec<f64> = vertices.iter().map(|v| v[1]).collect();
                e.add(0, &xs);
                e.add(1, &ys);
                e.add_color(&[*level]);
            }
        }
    }
    e
}

pub(crate) fn resolve(
    a: &Axes,
    leaves: &[Leaf],
    figure_map: Colormap,
    figure_pos: [f64; 4],
) -> AxesState {
    let e = extents(leaves);
    let mut lim = [[0.0, 1.0]; 3];
    let mut tick: [Vec<f64>; 3] = Default::default();
    let mut from_auto = [false; 3];
    for i in 0..3 {
        let p = a.axis(i);
        // An axes with children but no z data behaves as z = 0.
        let range = match e.axis[i] {
            None if i == 2 && e.any => Some([0.0, 0.0]),
            r => r,
        };
        if p.lim_mode == Mode::Manual {
            lim[i] = p.lim;
        } else if p.scale == Scale::Log {
            let positive = range.map(|[lo, hi]| [lo.max(f64::MIN_POSITIVE), hi]);
            lim[i] = layout::auto_log_limits(positive);
        } else if a.tight || (e.has_image && i < 2) {
            lim[i] = match range {
                Some([lo, hi]) if lo < hi => [lo, hi],
                Some([v, _]) => layout::auto_limits(Some([v, v])).0,
                None => [0.0, 1.0],
            };
        } else {
            let (l, t) = layout::auto_limits(range);
            lim[i] = l;
            tick[i] = t;
            from_auto[i] = true;
        }
    }
    let mut pbar = a.pbar;
    let dar = if a.dar_mode == Mode::Manual && a.pbar_mode == Mode::Auto && e.has_image {
        // `axis image` (imshow): limits stay on the data, the plot box
        // takes their aspect (relative to the smallest).
        let r: Vec<f64> = (0..3).map(|i| (lim[i][1] - lim[i][0]) / a.dar[i]).collect();
        let s = r.iter().copied().fold(f64::INFINITY, f64::min);
        if s.is_finite() && s > 0.0 {
            pbar = [r[0] / s, r[1] / s, r[2] / s];
        }
        a.dar
    } else if a.dar_mode == Mode::Manual {
        // Stretch automatic limits so the box keeps the requested aspect
        // (measured for `axis equal` in 2-D and 3-D).
        let r: Vec<f64> = (0..3)
            .map(|i| (lim[i][1] - lim[i][0]) / (a.dar[i] * pbar[i]))
            .collect();
        let s = r.iter().cloned().fold(0.0, f64::max);
        for i in 0..3 {
            if a.axis(i).lim_mode == Mode::Auto && s.is_finite() && s > 0.0 {
                let c = (lim[i][0] + lim[i][1]) / 2.0;
                let half = s * a.dar[i] * pbar[i] / 2.0;
                if (half * 2.0 - (lim[i][1] - lim[i][0])).abs() > 1e-12 * half.abs() {
                    lim[i] = [c - half, c + half];
                    from_auto[i] = false;
                }
            }
        }
        a.dar
    } else {
        layout::auto_dar(lim, pbar)
    };
    let mut ticklabel: [Vec<String>; 3] = Default::default();
    for i in 0..3 {
        let p = a.axis(i);
        tick[i] = if p.tick_mode == Mode::Manual {
            p.tick.clone()
        } else if p.scale == Scale::Log {
            layout::log_ticks(lim[i])
        } else if from_auto[i] {
            std::mem::take(&mut tick[i])
        } else {
            layout::ticks_for(lim[i])
        };
        ticklabel[i] = if p.ticklabel_mode == Mode::Manual {
            p.ticklabel.clone()
        } else {
            let log = p.scale == Scale::Log;
            tick[i]
                .iter()
                .map(|v| {
                    if log {
                        layout::log_tick_label(*v)
                    } else {
                        layout::tick_label(*v)
                    }
                })
                .collect()
        };
    }
    let clim = if a.clim_mode == Mode::Manual {
        a.clim
    } else {
        match e.color {
            None => [0.0, 1.0],
            Some([lo, hi]) if lo == hi => [lo - 1.0, hi + 1.0],
            Some(r) => r,
        }
    };
    let has_fill = leaves
        .iter()
        .any(|l| matches!(l.obj, LeafObj::ContourFill { .. }));
    let dimension = if !a.is_2d_view() || e.has_z_object || has_fill {
        3
    } else {
        2
    };
    let camera = layout::camera(
        lim,
        dar,
        pbar,
        a.view,
        [a.x.reverse, a.y.reverse, a.z.reverse],
        (a.position[2] * figure_pos[2]) / (a.position[3] * figure_pos[3]),
    );
    AxesState {
        lim,
        tick,
        ticklabel,
        dar,
        pbar,
        clim,
        colormap: a.colormap.clone().unwrap_or(figure_map),
        camera,
        dimension,
        figure_size: [figure_pos[2], figure_pos[3]],
    }
}

/// Patch geometry, data-tip info and optional baseline of one bar series.
fn bar_patch(kind: &GroupKind) -> (Patch, BarInfo, Option<LeafObj>) {
    let GroupKind::Bar {
        x,
        y,
        horizontal,
        stacked,
        width,
        base_value,
        series,
        series_count,
        offsets,
        face_color,
        edge_color,
        line_width,
        line_style,
        has_baseline,
    } = kind
    else {
        unreachable!("bar_patch on a non-bar group")
    };
    let mut sorted: Vec<f64> = x.iter().copied().filter(|v| v.is_finite()).collect();
    sorted.sort_by(f64::total_cmp);
    let dx = sorted
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|d| *d > 0.0)
        .fold(f64::INFINITY, f64::min);
    let dx = if dx.is_finite() { dx } else { 1.0 };
    let n = *series_count as f64;
    let w = *width;
    // Fitted to measured Octave geometry (1-4 series, BarWidth 0.4-1.0).
    let (bar_width, offset) = if *stacked || *series_count <= 1 {
        (w * dx, 0.0)
    } else {
        let spacing = w * (1.0 + w) / (2.0 * n) * dx;
        (
            w * (3.0 * w - 1.0) / (2.0 * n) * dx,
            (*series as f64 - (n - 1.0) / 2.0) * spacing,
        )
    };
    let m = x.len();
    let mut vx = Vec::with_capacity(4 * m);
    let mut vy = Vec::with_capacity(4 * m);
    for k in 0..m {
        let c = x[k] + offset;
        let (l, r) = (c - bar_width / 2.0, c + bar_width / 2.0);
        let bottom = if *stacked { offsets[k] } else { *base_value };
        let top = if *stacked { offsets[k] + y[k] } else { y[k] };
        for (px, py) in [(l, bottom), (l, top), (r, top), (r, bottom)] {
            vx.push(px);
            vy.push(py);
        }
    }
    let (cx, cy) = if *horizontal { (vy, vx) } else { (vx, vy) };
    let mut vertices = cx;
    vertices.extend(cy);
    let mut faces = vec![0.0; 4 * m];
    for k in 0..m {
        for j in 0..4 {
            faces[k + j * m] = (4 * k + j + 1) as f64;
        }
    }
    let patch = Patch {
        vertices: Array::matrix(4 * m, 2, vertices),
        faces: Array::matrix(m, 4, faces),
        fvcdata: Array::empty(),
        face_color: *face_color,
        edge_color: *edge_color,
        cdata_direct: false,
        line_style: line_style.clone(),
        line_width: *line_width,
        marker: "none".into(),
        face_alpha: 1.0,
        edge_alpha: 1.0,
        display_name: String::new(),
    };
    let info = BarInfo {
        horizontal: *horizontal,
        layout: if *stacked { "stacked" } else { "grouped" },
        width: w,
        base_value: *base_value,
        positions: x.clone(),
        values: y.clone(),
    };
    let baseline = has_baseline.then_some(LeafObj::Baseline {
        horizontal: *horizontal,
        base: *base_value,
    });
    (patch, info, baseline)
}

/// A histogram as drawn: one rectangle per bin from 0 to the value (no
/// baseline, unlike `bar`), or the outline of the bins for
/// `DisplayStyle 'stairs'`.
fn histogram_leaf(h: &crate::model::Histogram) -> LeafObj {
    let e = &h.edges;
    let v = h.values();
    let m = v.len();
    if h.stairs {
        let (mut px, mut py) = (Vec::with_capacity(2 * m + 2), Vec::with_capacity(2 * m + 2));
        if m > 0 {
            px.push(e[0]);
            py.push(0.0);
            for k in 0..m {
                px.extend([e[k], e[k + 1]]);
                py.extend([v[k], v[k]]);
            }
            px.push(e[m]);
            py.push(0.0);
        }
        let (x, y) = if h.horizontal { (py, px) } else { (px, py) };
        let color = h.edge().rgb().unwrap_or(h.series_color);
        let mut l = Line::new(x, y, Vec::new(), color);
        l.line_width = h.line_width;
        l.line_style = if h.edge_color == Paint::None {
            "none".into()
        } else {
            h.line_style.clone()
        };
        return LeafObj::Line(l, "line", 0.0);
    }
    let (mut vx, mut vy) = (Vec::with_capacity(4 * m), Vec::with_capacity(4 * m));
    for k in 0..m {
        for (px, py) in [(e[k], 0.0), (e[k], v[k]), (e[k + 1], v[k]), (e[k + 1], 0.0)] {
            vx.push(px);
            vy.push(py);
        }
    }
    let (cx, cy) = if h.horizontal { (vy, vx) } else { (vx, vy) };
    let mut vertices = cx;
    vertices.extend(cy);
    let mut faces = vec![0.0; 4 * m];
    for k in 0..m {
        for j in 0..4 {
            faces[k + j * m] = (4 * k + j + 1) as f64;
        }
    }
    let patch = Patch {
        vertices: Array::matrix(4 * m, 2, vertices),
        faces: Array::matrix(m, 4, faces),
        fvcdata: Array::empty(),
        face_color: h.face(),
        edge_color: h.edge(),
        cdata_direct: false,
        line_style: h.line_style.clone(),
        line_width: h.line_width,
        marker: "none".into(),
        face_alpha: h.face_alpha,
        edge_alpha: h.edge_alpha,
        display_name: String::new(),
    };
    let info = BarInfo {
        horizontal: h.horizontal,
        layout: "grouped",
        width: 1.0,
        base_value: 0.0,
        positions: e.windows(2).map(|w| (w[0] + w[1]) / 2.0).collect(),
        values: v,
    };
    LeafObj::Patch(patch, Some(info))
}

fn area_patch(kind: &GroupKind) -> Patch {
    let GroupKind::Area {
        x,
        y,
        offsets,
        face_color,
        edge_color,
        line_width,
        line_style,
        ..
    } = kind
    else {
        unreachable!("area_patch on a non-area group")
    };
    let n = x.len();
    let mut px = Vec::with_capacity(2 * n + 1);
    let mut py = Vec::with_capacity(2 * n + 1);
    if n > 0 {
        px.push(x[0]);
        py.push(offsets[0]);
        for k in 0..n {
            px.push(x[k]);
            py.push(offsets[k] + y[k]);
        }
        for k in (0..n).rev() {
            px.push(x[k]);
            py.push(offsets[k]);
        }
    }
    let count = px.len();
    let mut vertices = px;
    vertices.extend(py);
    Patch {
        vertices: Array::matrix(count, 2, vertices),
        faces: Array::matrix(1, count, (1..=count).map(|k| k as f64).collect()),
        fvcdata: Array::empty(),
        face_color: *face_color,
        edge_color: *edge_color,
        cdata_direct: false,
        line_style: line_style.clone(),
        line_width: *line_width,
        marker: "none".into(),
        face_alpha: 1.0,
        edge_alpha: 1.0,
        display_name: String::new(),
    }
}

fn coord(c: &GridCoord) -> Coord {
    match c {
        GridCoord::Vector(v) => Coord::Vector(v.clone()),
        GridCoord::Matrix(v) => Coord::Matrix(v.clone()),
    }
}

/// Resolved contour levels of a contour group.
pub(crate) fn contour_levels(kind: &GroupKind) -> Vec<f64> {
    let GroupKind::Contour {
        z,
        levels,
        level_mode,
        level_count,
        ..
    } = kind
    else {
        return Vec::new();
    };
    if *level_mode == Mode::Manual {
        return levels.clone();
    }
    match z.finite_range() {
        Some([lo, hi]) => contour::auto_levels(lo, hi, *level_count),
        None => Vec::new(),
    }
}

fn contour_leaves(kind: &GroupKind) -> Vec<Leaf> {
    let GroupKind::Contour {
        x,
        y,
        z,
        fill,
        line_color,
        line_style,
        line_width,
        ..
    } = kind
    else {
        return Vec::new();
    };
    let levels = contour_levels(kind);
    let (rows, cols) = (z.rows(), z.cols());
    let (cx, cy) = (coord(x), coord(y));
    let leaf = |role, obj| Leaf {
        role: Some(role),
        group_name: None,
        group: None,
        unit_box: [false; 2],
        obj,
    };
    if *fill {
        contour::contourf_patches(&cx, &cy, &z.data, rows, cols, &levels)
            .into_iter()
            // The module returns get(h,'children') order; drawing order is reversed.
            .rev()
            .map(|p| {
                leaf(
                    "contourf",
                    LeafObj::ContourFill {
                        level: p.level,
                        vertices: p.vertices,
                        face: p.face,
                        edge: *line_color,
                        line_style: line_style.clone(),
                        line_width: *line_width,
                    },
                )
            })
            .collect()
    } else {
        contour::contour_lines(&cx, &cy, &z.data, rows, cols, &levels)
            .into_iter()
            .rev()
            .map(|c| {
                leaf(
                    "contour",
                    LeafObj::ContourLine {
                        level: c.level,
                        x: c.x,
                        y: c.y,
                        color: *line_color,
                        line_style: line_style.clone(),
                        line_width: *line_width,
                    },
                )
            })
            .collect()
    }
}
