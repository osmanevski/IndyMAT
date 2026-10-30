//! Graphics object tree: root → figure → axes → plot objects.
//!
//! Children are stored in drawing order (oldest first); MATLAB's `Children`
//! property lists them newest first, which `get` reproduces.

use crate::color::{COLOR_ORDER, Colormap, Rgb, default_colormap};
use crate::value::Array;

/// A graphics handle. Root is 0, figures are their positive figure number,
/// every other object gets a negative id (never collides with a figure).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Handle(pub i64);

impl Handle {
    pub const ROOT: Handle = Handle(0);

    /// The numeric value the interpreter shows for this handle.
    pub fn value(self) -> f64 {
        self.0 as f64
    }

    pub fn from_value(v: f64) -> Option<Handle> {
        (v.is_finite() && v.fract() == 0.0 && v.abs() < 9.0e15).then_some(Handle(v as i64))
    }

    pub fn is_figure(self) -> bool {
        self.0 > 0
    }
}

/// A colour-valued property: RGB or one of the keyword states.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Paint {
    Rgb(Rgb),
    None,
    Auto,
    Flat,
    Interp,
}

impl Paint {
    pub fn rgb(self) -> Option<Rgb> {
        match self {
            Paint::Rgb(c) => Some(c),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Auto,
    Manual,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Auto => "auto",
            Mode::Manual => "manual",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Object {
    pub parent: Handle,
    pub children: Vec<Handle>,
    pub visible: bool,
    pub tag: String,
    pub kind: Kind,
    /// `setappdata` entries and `UserData`; dropped with the object.
    pub appdata: AppData,
}

/// Application data of one object: values of the embedding interpreter,
/// opaque to the model (`setappdata`/`getappdata`, `UserData`). Entries
/// keep insertion order (`getappdata(h)` lists them that way).
#[derive(Clone, Default)]
pub struct AppData {
    pub entries: Vec<(String, std::rc::Rc<dyn std::any::Any>)>,
    pub user_data: Option<std::rc::Rc<dyn std::any::Any>>,
}

impl std::fmt::Debug for AppData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.entries.iter().map(|(n, _)| n.as_str()).collect();
        f.debug_struct("AppData")
            .field("entries", &names)
            .field("user_data", &self.user_data.is_some())
            .finish()
    }
}

impl AppData {
    pub fn get(&self, name: &str) -> Option<&std::rc::Rc<dyn std::any::Any>> {
        self.entries.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    pub fn set(&mut self, name: &str, v: std::rc::Rc<dyn std::any::Any>) {
        match self.entries.iter_mut().find(|(n, _)| n == name) {
            Some(slot) => slot.1 = v,
            None => self.entries.push((name.to_string(), v)),
        }
    }

    /// Removes `name`; false when it was not present.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|(n, _)| n != name);
        self.entries.len() != before
    }
}

#[derive(Debug, Clone)]
pub enum Kind {
    Figure(Figure),
    Axes(Box<Axes>),
    Line(Line),
    Scatter(Scatter),
    Surface(Surface),
    Patch(Patch),
    Text(Text),
    Image(Image),
    Group(Box<Group>),
    Legend(Legend),
    Colorbar(Colorbar),
}

impl Kind {
    /// MATLAB `Type` property.
    pub fn type_name(&self) -> &'static str {
        match self {
            Kind::Figure(_) => "figure",
            Kind::Axes(_) => "axes",
            Kind::Line(_) => "line",
            Kind::Scatter(_) => "scatter",
            Kind::Surface(_) => "surface",
            Kind::Patch(_) => "patch",
            Kind::Text(t) if t.role == LabelRole::SuperTitle => "subplottext",
            Kind::Text(_) => "text",
            Kind::Image(_) => "image",
            Kind::Group(g) => match &g.kind {
                GroupKind::Histogram(_) => "histogram",
                GroupKind::ErrorBar(_) => "errorbar",
                GroupKind::Quiver(_) => "quiver",
                GroupKind::FunctionLine(f) if f.y_function.is_some() => "parameterizedfunctionline",
                GroupKind::FunctionLine(_) => "functionline",
                _ => "hggroup",
            },
            Kind::Legend(_) => "legend",
            Kind::Colorbar(_) => "colorbar",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Figure {
    pub number: i64,
    pub name: String,
    pub colormap: Colormap,
    pub color: Rgb,
    /// Pixels `[left bottom width height]`.
    pub position: [f64; 4],
    pub current_axes: Option<Handle>,
    pub hold_add: bool,
    /// `DefaultAxesColorOrder` set on this figure (`None` = the factory
    /// order): the colour order of axes created or reset in it.
    pub default_color_order: Option<Vec<Rgb>>,
}

impl Figure {
    pub fn new(number: i64) -> Figure {
        Figure {
            number,
            name: String::new(),
            colormap: default_colormap(),
            color: [1.0, 1.0, 1.0],
            position: [300.0, 200.0, 560.0, 420.0],
            current_axes: None,
            hold_add: false,
            default_color_order: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scale {
    Linear,
    Log,
}

#[derive(Debug, Clone)]
pub struct AxisProps {
    pub lim: [f64; 2],
    pub lim_mode: Mode,
    pub tick: Vec<f64>,
    pub tick_mode: Mode,
    pub ticklabel: Vec<String>,
    pub ticklabel_mode: Mode,
    pub scale: Scale,
    pub reverse: bool,
    pub grid: bool,
    pub minor_grid: bool,
    /// Handle of the label text object (xlabel/ylabel/zlabel).
    pub label: Handle,
}

impl AxisProps {
    pub fn new(label: Handle) -> AxisProps {
        AxisProps {
            lim: [0.0, 1.0],
            lim_mode: Mode::Auto,
            tick: Vec::new(),
            tick_mode: Mode::Auto,
            ticklabel: Vec::new(),
            ticklabel_mode: Mode::Auto,
            scale: Scale::Linear,
            reverse: false,
            grid: false,
            minor_grid: false,
            label,
        }
    }
}

/// Where a subplot/tile axes lives in its grid, so `subplot(m,n,p)` can find
/// and reuse an existing axes.
#[derive(Debug, Clone, PartialEq)]
pub struct GridSlot {
    pub rows: usize,
    pub cols: usize,
    pub cells: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct Axes {
    /// Normalized `[left bottom width height]` of the plot box.
    pub position: [f64; 4],
    /// Position before a colorbar shrank it.
    pub base_position: [f64; 4],
    pub x: AxisProps,
    pub y: AxisProps,
    pub z: AxisProps,
    pub title: Handle,
    pub view: [f64; 2],
    pub dar: [f64; 3],
    pub dar_mode: Mode,
    pub pbar: [f64; 3],
    pub pbar_mode: Mode,
    pub clim: [f64; 2],
    pub clim_mode: Mode,
    /// `None` means the figure's colormap.
    pub colormap: Option<Colormap>,
    pub color_order: Vec<Rgb>,
    pub color_order_index: usize,
    pub hold_add: bool,
    pub color: Paint,
    pub box_on: bool,
    pub font_size: f64,
    pub tick_label_interpreter: String,
    pub legend: Option<Handle>,
    pub colorbar: Option<Handle>,
    pub slot: Option<GridSlot>,
    /// `axis tight` requested while limits are automatic.
    pub tight: bool,
    /// Grid position before `sgtitle` reserved room above the axes.
    pub sg_base: Option<[f64; 4]>,
    /// Holds a `pie` chart (square plot box, see `pie.rs`).
    pub pie: bool,
    /// `yyaxis`: the other side's axes of the pair.
    pub yy_pair: Option<Handle>,
    /// `YAxisLocation 'right'` (the right side of a `yyaxis` pair).
    pub y_right: bool,
}

pub const DEFAULT_AXES_POSITION: [f64; 4] = [0.13, 0.11, 0.775, 0.815];

impl Axes {
    pub fn new(labels: [Handle; 4]) -> Axes {
        Axes {
            position: DEFAULT_AXES_POSITION,
            base_position: DEFAULT_AXES_POSITION,
            x: AxisProps::new(labels[0]),
            y: AxisProps::new(labels[1]),
            z: AxisProps::new(labels[2]),
            title: labels[3],
            view: [0.0, 90.0],
            dar: [1.0, 1.0, 1.0],
            dar_mode: Mode::Auto,
            pbar: [1.0, 1.0, 1.0],
            pbar_mode: Mode::Auto,
            clim: [0.0, 1.0],
            clim_mode: Mode::Auto,
            colormap: None,
            color_order: COLOR_ORDER.to_vec(),
            color_order_index: 0,
            hold_add: false,
            color: Paint::Rgb([1.0, 1.0, 1.0]),
            box_on: true,
            font_size: 10.0,
            tick_label_interpreter: "tex".into(),
            legend: None,
            colorbar: None,
            slot: None,
            tight: false,
            sg_base: None,
            pie: false,
            yy_pair: None,
            y_right: false,
        }
    }

    pub fn axis(&self, i: usize) -> &AxisProps {
        match i {
            0 => &self.x,
            1 => &self.y,
            _ => &self.z,
        }
    }

    pub fn axis_mut(&mut self, i: usize) -> &mut AxisProps {
        match i {
            0 => &mut self.x,
            1 => &mut self.y,
            _ => &mut self.z,
        }
    }

    pub fn is_2d_view(&self) -> bool {
        self.view == [0.0, 90.0]
    }

    /// Takes the next colour from the colour order and advances the index.
    pub fn next_color(&mut self) -> Rgb {
        let n = self.color_order.len().max(1);
        let c = self
            .color_order
            .get(self.color_order_index % n)
            .copied()
            .unwrap_or([0.0, 0.0, 0.0]);
        self.color_order_index += 1;
        c
    }
}

#[derive(Debug, Clone)]
pub struct Line {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    /// Empty for a 2-D line.
    pub z: Vec<f64>,
    pub color: Rgb,
    pub line_style: String,
    pub line_width: f64,
    pub marker: String,
    pub marker_size: f64,
    pub marker_edge: Paint,
    pub marker_face: Paint,
    pub display_name: String,
}

impl Line {
    pub fn new(x: Vec<f64>, y: Vec<f64>, z: Vec<f64>, color: Rgb) -> Line {
        Line {
            x,
            y,
            z,
            color,
            line_style: "-".into(),
            line_width: 0.5,
            marker: "none".into(),
            marker_size: 6.0,
            marker_edge: Paint::Auto,
            marker_face: Paint::None,
            display_name: String::new(),
        }
    }
}

/// Scatter colour data.
#[derive(Debug, Clone, PartialEq)]
pub enum ScatterColor {
    Rgb(Rgb),
    /// One scalar mapped through the colormap.
    Indexed(f64),
    /// Per-point values (N×1 indexed or N×3 RGB); the contract keeps these PNG-only.
    PerPoint(Array),
}

#[derive(Debug, Clone)]
pub struct Scatter {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    pub z: Vec<f64>,
    pub size_data: Vec<f64>,
    pub cdata: ScatterColor,
    pub marker: String,
    pub marker_edge: Paint,
    pub marker_face: Paint,
    pub line_width: f64,
    pub display_name: String,
}

#[derive(Debug, Clone)]
pub struct Surface {
    pub x: Array,
    pub y: Array,
    pub z: Array,
    pub c: Array,
    pub face_color: Paint,
    pub edge_color: Paint,
    pub mesh_style: String,
    pub line_style: String,
    pub line_width: f64,
    pub cdata_direct: bool,
    pub face_alpha: f64,
    pub edge_alpha: f64,
    pub marker: String,
    pub display_name: String,
}

#[derive(Debug, Clone)]
pub struct Patch {
    /// N×2 or N×3.
    pub vertices: Array,
    /// M×K, one-based vertex indices with NaN padding (MATLAB form).
    pub faces: Array,
    /// FaceVertexCData: empty, 1×1, 1×3, M×1, M×3, N×1 or N×3.
    pub fvcdata: Array,
    pub face_color: Paint,
    pub edge_color: Paint,
    pub cdata_direct: bool,
    pub line_style: String,
    pub line_width: f64,
    pub marker: String,
    pub face_alpha: f64,
    pub edge_alpha: f64,
    pub display_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelRole {
    /// Free text placed by `text`.
    Free,
    XLabel,
    YLabel,
    ZLabel,
    Title,
    /// Figure-wide title (`sgtitle`), a child of the figure.
    SuperTitle,
}

#[derive(Debug, Clone)]
pub struct Text {
    pub role: LabelRole,
    pub position: [f64; 3],
    pub lines: Vec<String>,
    pub units: String,
    pub font_size: f64,
    pub font_weight: String,
    pub font_angle: String,
    pub color: Rgb,
    pub background: Paint,
    pub edge_color: Paint,
    pub margin: f64,
    pub horizontal_alignment: String,
    pub vertical_alignment: String,
    pub rotation: f64,
    pub interpreter: String,
    pub line_style: String,
    pub line_width: f64,
    pub clipping: bool,
}

impl Text {
    pub fn new(role: LabelRole) -> Text {
        Text {
            role,
            position: [0.0, 0.0, 0.0],
            lines: Vec::new(),
            units: "data".into(),
            font_size: 10.0,
            font_weight: "normal".into(),
            font_angle: "normal".into(),
            color: [0.0, 0.0, 0.0],
            background: Paint::None,
            edge_color: Paint::None,
            margin: 2.0,
            horizontal_alignment: "left".into(),
            vertical_alignment: "middle".into(),
            rotation: 0.0,
            interpreter: "tex".into(),
            line_style: "-".into(),
            line_width: 0.5,
            clipping: false,
        }
    }

    /// The string joined with spaces, as the extractor reads label strings.
    pub fn joined(&self) -> String {
        self.lines.join(" ")
    }
}

#[derive(Debug, Clone)]
pub struct Image {
    /// rows×cols (indexed) or rows×cols×3 (truecolor).
    pub cdata: Array,
    pub x: [f64; 2],
    pub y: [f64; 2],
    pub cdata_direct: bool,
    pub display_name: String,
}

/// Grid coordinates of contour data.
#[derive(Debug, Clone, PartialEq)]
pub enum GridCoord {
    Vector(Vec<f64>),
    Matrix(Vec<f64>),
}

#[derive(Debug, Clone)]
pub enum GroupKind {
    Bar {
        x: Vec<f64>,
        y: Vec<f64>,
        horizontal: bool,
        stacked: bool,
        width: f64,
        base_value: f64,
        /// Index of this series within its `bar` call and the call's size.
        series: usize,
        series_count: usize,
        /// Bottom of each bar when stacked (same length as y).
        offsets: Vec<f64>,
        face_color: Paint,
        edge_color: Paint,
        line_width: f64,
        line_style: String,
        /// Only the first series of a call owns the visible baseline.
        has_baseline: bool,
    },
    Area {
        /// Created from a vector (one series) rather than a matrix.
        single: bool,
        x: Vec<f64>,
        y: Vec<f64>,
        offsets: Vec<f64>,
        base_value: f64,
        face_color: Paint,
        edge_color: Paint,
        line_width: f64,
        line_style: String,
    },
    /// `stem`/`stairs`: the series record carries the group's own data.
    Stem(Line, f64),
    Stairs(Line),
    Histogram(Box<Histogram>),
    Contour {
        x: GridCoord,
        y: GridCoord,
        z: Array,
        levels: Vec<f64>,
        level_mode: Mode,
        level_count: Option<usize>,
        fill: bool,
        line_color: Paint,
        line_style: String,
        line_width: f64,
    },
    /// `contour3` and the contour part of `surfc`/`meshc` (module `surf2`).
    Contour3(Box<Contour3>),
    ErrorBar(Box<ErrorBar>),
    Quiver(Box<Quiver>),
    FunctionLine(Box<FunctionLine>),
    /// The drawn grid of a `polarplot`/`polar` axes (see `lines2`).
    PolarGrid,
}

#[derive(Debug, Clone)]
pub struct Group {
    pub kind: GroupKind,
    pub display_name: String,
}

/// MATLAB `histogram` object: the data, its bin edges and how the counts
/// are shown. Bins follow `histcounts`: `[e(k), e(k+1))`, the last closed.
#[derive(Debug, Clone)]
pub struct Histogram {
    pub data: Vec<f64>,
    pub edges: Vec<f64>,
    pub counts: Vec<f64>,
    /// `count probability pdf cdf countdensity cumcount`.
    pub normalization: String,
    pub horizontal: bool,
    /// `DisplayStyle 'stairs'` (outline only) instead of `'bar'`.
    pub stairs: bool,
    /// `auto` (bin edges chosen by `histcounts`) or `manual`; also the
    /// method name given with `BinMethod`.
    pub bin_method: String,
    pub face_color: Paint,
    pub edge_color: Paint,
    /// The colour-order colour behind `FaceColor 'auto'`.
    pub series_color: Rgb,
    pub face_alpha: f64,
    pub edge_alpha: f64,
    pub line_width: f64,
    pub line_style: String,
}

impl Histogram {
    pub fn new(data: Vec<f64>, edges: Vec<f64>, series_color: Rgb) -> Histogram {
        let mut h = Histogram {
            data,
            edges,
            counts: Vec::new(),
            normalization: "count".into(),
            horizontal: false,
            stairs: false,
            bin_method: "auto".into(),
            face_color: Paint::Auto,
            edge_color: Paint::Rgb([0.0, 0.0, 0.0]),
            series_color,
            face_alpha: 0.6,
            edge_alpha: 1.0,
            line_width: 0.5,
            line_style: "-".into(),
        };
        h.recount();
        h
    }

    /// Counts of `data` in `edges` (the `histcounts` rule).
    pub fn recount(&mut self) {
        let e = &self.edges;
        let nb = e.len().saturating_sub(1);
        let mut n = vec![0.0; nb];
        if nb > 0 {
            for &v in &self.data {
                if v.is_nan() || v < e[0] || v > e[nb] {
                    continue;
                }
                let j = if v == e[nb] {
                    nb - 1
                } else {
                    e.partition_point(|x| *x <= v).saturating_sub(1).min(nb - 1)
                };
                n[j] += 1.0;
            }
        }
        self.counts = n;
    }

    /// The bar heights: the counts under `normalization`.
    pub fn values(&self) -> Vec<f64> {
        let total = self.data.len() as f64;
        let widths = self.edges.windows(2).map(|w| w[1] - w[0]);
        let cum = |v: Vec<f64>| {
            let mut s = 0.0;
            v.into_iter()
                .map(|x| {
                    s += x;
                    s
                })
                .collect()
        };
        match self.normalization.as_str() {
            "probability" => self.counts.iter().map(|c| c / total).collect(),
            "countdensity" => self.counts.iter().zip(widths).map(|(c, w)| c / w).collect(),
            "pdf" => self
                .counts
                .iter()
                .zip(widths)
                .map(|(c, w)| c / total / w)
                .collect(),
            "cumcount" => cum(self.counts.clone()),
            "cdf" => cum(self.counts.iter().map(|c| c / total).collect()),
            _ => self.counts.clone(),
        }
    }

    pub fn face(&self) -> Paint {
        match self.face_color {
            Paint::Auto => Paint::Rgb(self.series_color),
            p => p,
        }
    }

    /// Edge colour as drawn: `auto` is the series colour.
    pub fn edge(&self) -> Paint {
        match self.edge_color {
            Paint::Auto => Paint::Rgb(self.series_color),
            p => p,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Legend {
    pub peer: Handle,
    pub labels: Vec<String>,
    pub location: String,
}

#[derive(Debug, Clone)]
pub struct Colorbar {
    pub peer: Handle,
    pub location: String,
    pub label: String,
}

/// A contour group drawn in 3-D: `contour3` puts each line at its level,
/// `surfc`/`meshc` put every line on one plane (`plane`). `contour` holds
/// the data, levels and line properties as a [`GroupKind::Contour`].
#[derive(Debug, Clone)]
pub struct Contour3 {
    pub contour: GroupKind,
    /// `None`: z = level (`contour3`); `Some(z)`: all lines at z.
    pub plane: Option<f64>,
}

/// MATLAB `errorbar` object: the data line and its error deltas (empty
/// vectors mean no bars in that direction). `CapSize` is in points.
#[derive(Debug, Clone)]
pub struct ErrorBar {
    pub line: Line,
    pub y_neg: Vec<f64>,
    pub y_pos: Vec<f64>,
    pub x_neg: Vec<f64>,
    pub x_pos: Vec<f64>,
    pub cap_size: f64,
}

/// MATLAB `quiver` object: arrows from `(x, y)` along `(u, v)`. `grid` is
/// the (rows, cols) count used for the autoscale spacing; `line` carries
/// the style (its x/y are the bases, its marker is drawn at the bases).
#[derive(Debug, Clone)]
pub struct Quiver {
    pub line: Line,
    pub u: Vec<f64>,
    pub v: Vec<f64>,
    pub grid: [f64; 2],
    pub auto_scale: bool,
    pub auto_scale_factor: f64,
    pub max_head_size: f64,
    pub show_arrow_head: bool,
}

/// MATLAB `fplot` line: samples already evaluated by the interpreter.
#[derive(Debug, Clone)]
pub struct FunctionLine {
    pub line: Line,
    /// Text of the function (`func2str`), the X function when parametric.
    pub function: String,
    /// The Y function of a parametric line.
    pub y_function: Option<String>,
    /// `XRange` (or `TRange` when parametric).
    pub range: [f64; 2],
    pub mesh_density: f64,
}
