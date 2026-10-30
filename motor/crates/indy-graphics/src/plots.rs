//! Plotting commands: `plot plot3 line scatter scatter3 bar barh stairs
//! stem area fill patch surf mesh contour contourf imagesc image text`.
//!
//! Every command takes an optional explicit axes (`plot(ax, ...)`); `None`
//! means the current axes, created on demand, and the hold state decides
//! whether it is cleared first (MATLAB `newplot`).

use crate::color::Rgb;
use crate::error::{GraphicsError, Result};
use crate::graphics::{Graphics, Opts};
use crate::linespec::{self, LineSpec};
use crate::model::{
    GridCoord, Group, GroupKind, Handle, Image, Kind, LabelRole, Line, Mode, Paint, Patch, Scatter,
    ScatterColor, Surface, Text,
};
use crate::props::parse_paint;
use crate::value::{Array, PropValue};

/// One `x, y[, z][, LineSpec]` group of `plot`/`plot3`.
#[derive(Debug, Clone, Default)]
pub struct PlotSeries {
    /// `None` means the implicit `1:N`.
    pub x: Option<Array>,
    pub y: Array,
    pub z: Option<Array>,
    pub spec: Option<String>,
}

impl PlotSeries {
    pub fn y(y: &[f64]) -> PlotSeries {
        PlotSeries {
            y: Array::row(y),
            ..PlotSeries::default()
        }
    }

    pub fn xy(x: &[f64], y: &[f64]) -> PlotSeries {
        PlotSeries {
            x: Some(Array::row(x)),
            y: Array::row(y),
            ..PlotSeries::default()
        }
    }

    pub fn xyz(x: &[f64], y: &[f64], z: &[f64]) -> PlotSeries {
        PlotSeries {
            x: Some(Array::row(x)),
            y: Array::row(y),
            z: Some(Array::row(z)),
            spec: None,
        }
    }

    pub fn with_spec(mut self, spec: &str) -> PlotSeries {
        self.spec = Some(spec.to_string());
        self
    }
}

/// Arguments of `scatter`/`scatter3`.
#[derive(Debug, Clone, Default)]
pub struct ScatterArgs {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    pub z: Option<Vec<f64>>,
    /// Marker area in points²; empty means the default 36.
    pub sizes: Vec<f64>,
    /// Colour: a colour name/letter, an RGB 1×3, a scalar, N values or N×3.
    pub color: Option<PropValue>,
    pub filled: bool,
    pub marker: Option<String>,
}

/// Arguments of `bar`/`barh`.
#[derive(Debug, Clone, Default)]
pub struct BarArgs {
    pub x: Option<Vec<f64>>,
    /// Vector, or matrix whose columns are series.
    pub y: Array,
    pub width: Option<f64>,
    pub stacked: bool,
    pub color: Option<PropValue>,
}

/// Arguments of `surf`/`mesh`/`contour`/`contourf`: `Z`, or `X, Y, Z`
/// (vectors or matrices), optional colour data `C`.
#[derive(Debug, Clone, Default)]
pub struct GridArgs {
    pub x: Option<Array>,
    pub y: Option<Array>,
    pub z: Array,
    pub c: Option<Array>,
}

impl GridArgs {
    pub fn z(z: Array) -> GridArgs {
        GridArgs {
            z,
            ..GridArgs::default()
        }
    }

    pub fn xyz(x: Array, y: Array, z: Array) -> GridArgs {
        GridArgs {
            x: Some(x),
            y: Some(y),
            z,
            c: None,
        }
    }
}

/// Contour level request.
#[derive(Debug, Clone, PartialEq)]
pub enum Levels {
    Auto,
    Count(usize),
    Values(Vec<f64>),
}

fn vec_of(a: &Array) -> Vec<f64> {
    a.data.clone()
}

/// MATLAB pairing of X and Y in `plot(X, Y)`: vectors pair directly, a
/// vector pairs with each matching column (or row) of a matrix, two equal
/// matrices pair column by column.
fn pair_columns(x: Option<&Array>, y: &Array, func: &str) -> Result<Vec<(Vec<f64>, Vec<f64>)>> {
    let columns = |a: &Array| -> Vec<Vec<f64>> {
        if a.is_vector() {
            vec![vec_of(a)]
        } else {
            (0..a.cols()).map(|c| a.column(c).to_vec()).collect()
        }
    };
    let rows_of = |a: &Array| -> Vec<Vec<f64>> {
        (0..a.rows())
            .map(|r| (0..a.cols()).map(|c| a.at(r, c)).collect())
            .collect()
    };
    let Some(x) = x else {
        let ys = columns(y);
        let n = ys.first().map_or(0, Vec::len);
        let xs: Vec<f64> = (1..=n).map(|k| k as f64).collect();
        return Ok(ys.into_iter().map(|c| (xs.clone(), c)).collect());
    };
    let mismatch = || GraphicsError::args(func, "X and Y must have compatible sizes");
    match (x.is_vector(), y.is_vector()) {
        (true, true) => {
            if x.numel() != y.numel() {
                return Err(mismatch());
            }
            Ok(vec![(vec_of(x), vec_of(y))])
        }
        (true, false) => {
            let n = x.numel();
            let ys = if y.rows() == n {
                columns(y)
            } else if y.cols() == n {
                rows_of(y)
            } else {
                return Err(mismatch());
            };
            Ok(ys.into_iter().map(|c| (vec_of(x), c)).collect())
        }
        (false, true) => {
            let n = y.numel();
            let xs = if x.rows() == n {
                columns(x)
            } else if x.cols() == n {
                rows_of(x)
            } else {
                return Err(mismatch());
            };
            Ok(xs.into_iter().map(|c| (c, vec_of(y))).collect())
        }
        (false, false) => {
            if x.dims != y.dims {
                return Err(mismatch());
            }
            Ok(columns(x).into_iter().zip(columns(y)).collect())
        }
    }
}

fn parse_spec(spec: &Option<String>, func: &str) -> Result<LineSpec> {
    match spec {
        None => Ok(LineSpec::default()),
        Some(s) => linespec::parse(s)?
            .ok_or_else(|| GraphicsError::args(func, format!("invalid LineSpec '{s}'"))),
    }
}

impl Graphics {
    /// The colour of a new line: the LineSpec colour, or the next colour of
    /// the order. Measured (Octave 11.3, single call and under `hold on`): a
    /// LineSpec colour does not advance the order; a `'Color'` name/value
    /// does, because it overrides the cycled colour afterwards.
    pub(crate) fn spec_color(&mut self, ax: Handle, spec: &LineSpec) -> Result<Rgb> {
        match spec.color {
            Some(c) => Ok(c),
            None => Ok(self.axes_mut(ax)?.next_color()),
        }
    }

    /// Applies name/value options to each handle.
    pub(crate) fn apply_opts(&mut self, handles: &[Handle], opts: &Opts) -> Result<()> {
        for &h in handles {
            for (name, value) in opts {
                self.set(h, name, value.clone())?;
            }
        }
        Ok(())
    }

    fn make_line(
        &mut self,
        ax: Handle,
        x: Vec<f64>,
        y: Vec<f64>,
        z: Vec<f64>,
        spec: &LineSpec,
    ) -> Result<Handle> {
        let color = self.spec_color(ax, spec)?;
        let mut line = Line::new(x, y, z, color);
        if let Some(s) = &spec.line_style {
            line.line_style = s.clone();
        }
        if let Some(m) = &spec.marker {
            line.marker = m.clone();
        }
        Ok(self.add(ax, Kind::Line(line)))
    }

    /// `plot(...)`: one line per series column.
    pub fn plot(
        &mut self,
        ax: Option<Handle>,
        series: &[PlotSeries],
        opts: &Opts,
    ) -> Result<Vec<Handle>> {
        if series.is_empty() {
            return Err(GraphicsError::args("plot", "not enough input arguments"));
        }
        let ax = self.new_plot(ax)?;
        let mut out = Vec::new();
        for s in series {
            let spec = parse_spec(&s.spec, "plot")?;
            for (x, y) in pair_columns(s.x.as_ref(), &s.y, "plot")? {
                out.push(self.make_line(ax, x, y, Vec::new(), &spec)?);
            }
        }
        self.apply_opts(&out, opts)?;
        Ok(out)
    }

    /// `semilogx` / `semilogy` / `loglog`: `plot` with logarithmic axes
    /// (`log_x`, `log_y`). Log axes stay 2-D only in the contract.
    pub fn plot_log(
        &mut self,
        ax: Option<Handle>,
        series: &[PlotSeries],
        log_x: bool,
        log_y: bool,
        opts: &Opts,
    ) -> Result<Vec<Handle>> {
        let lines = self.plot(ax, series, opts)?;
        if let Some(a) = lines.first().and_then(|l| self.ancestor_axes(*l)) {
            let axes = self.axes_mut(a)?;
            if log_x {
                axes.x.scale = crate::model::Scale::Log;
            }
            if log_y {
                axes.y.scale = crate::model::Scale::Log;
            }
        }
        Ok(lines)
    }

    /// `plot3(...)`: 3-D lines; sets the default 3-D view when not holding.
    pub fn plot3(
        &mut self,
        ax: Option<Handle>,
        series: &[PlotSeries],
        opts: &Opts,
    ) -> Result<Vec<Handle>> {
        if series.is_empty() {
            return Err(GraphicsError::args("plot3", "not enough input arguments"));
        }
        let ax = self.new_plot(ax)?;
        let fresh = !self.axes_ref(ax)?.hold_add;
        let mut out = Vec::new();
        for s in series {
            let spec = parse_spec(&s.spec, "plot3")?;
            let z =
                s.z.as_ref()
                    .ok_or_else(|| GraphicsError::args("plot3", "Z data is required"))?;
            let xy = pair_columns(s.x.as_ref(), &s.y, "plot3")?;
            let zs = pair_columns(s.x.as_ref(), z, "plot3")?;
            if xy.len() != zs.len() {
                return Err(GraphicsError::args(
                    "plot3",
                    "X, Y and Z must have compatible sizes",
                ));
            }
            for ((x, y), (_, z)) in xy.into_iter().zip(zs) {
                if z.len() != x.len() {
                    return Err(GraphicsError::args(
                        "plot3",
                        "X, Y and Z must have the same length",
                    ));
                }
                out.push(self.make_line(ax, x, y, z, &spec)?);
            }
        }
        if fresh {
            self.set_view_3d(ax)?;
        }
        self.apply_opts(&out, opts)?;
        Ok(out)
    }

    pub(crate) fn set_view_3d(&mut self, ax: Handle) -> Result<()> {
        let a = self.axes_mut(ax)?;
        a.view = [-37.5, 30.0];
        Ok(())
    }

    /// `line(x, y[, z])`: low-level line, never clears the axes.
    pub fn line(
        &mut self,
        ax: Option<Handle>,
        x: &[f64],
        y: &[f64],
        z: Option<&[f64]>,
        opts: &Opts,
    ) -> Result<Handle> {
        if x.len() != y.len() || z.is_some_and(|z| z.len() != x.len()) {
            return Err(GraphicsError::args(
                "line",
                "X, Y and Z must have the same length",
            ));
        }
        let ax = match ax {
            Some(h) => h,
            None => self.gca(),
        };
        // Measured: `line(..., 'Color', c)` leaves the colour order alone.
        let explicit = opts.iter().any(|(n, _)| n.eq_ignore_ascii_case("color"));
        let color = if explicit {
            [0.0, 0.0, 0.0]
        } else {
            self.axes_mut(ax)?.next_color()
        };
        let line = Line::new(
            x.to_vec(),
            y.to_vec(),
            z.map(<[f64]>::to_vec).unwrap_or_default(),
            color,
        );
        let h = self.add(ax, Kind::Line(line));
        self.apply_opts(&[h], opts)?;
        Ok(h)
    }

    /// `scatter`/`scatter3`.
    pub fn scatter(
        &mut self,
        ax: Option<Handle>,
        args: &ScatterArgs,
        opts: &Opts,
    ) -> Result<Handle> {
        let n = args.x.len();
        if args.y.len() != n || args.z.as_ref().is_some_and(|z| z.len() != n) {
            return Err(GraphicsError::args(
                "scatter",
                "X, Y and Z must have the same length",
            ));
        }
        let sizes = if args.sizes.is_empty() {
            vec![36.0]
        } else {
            args.sizes.clone()
        };
        if !(sizes.len() == 1 || sizes.len() == n) {
            return Err(GraphicsError::args(
                "scatter",
                "S must be a scalar or match X",
            ));
        }
        let ax = self.new_plot(ax)?;
        let fresh = !self.axes_ref(ax)?.hold_add;
        let cdata = match &args.color {
            None => ScatterColor::Rgb(self.axes_mut(ax)?.next_color()),
            Some(v) => scatter_color(v, n)?,
        };
        let marker = match &args.marker {
            None => "o".to_string(),
            Some(m) => linespec::canonical_marker(m)
                .ok_or_else(|| GraphicsError::args("scatter", format!("invalid marker '{m}'")))?
                .to_string(),
        };
        let (edge, face) = if args.filled {
            (Paint::None, Paint::Flat)
        } else {
            (Paint::Flat, Paint::None)
        };
        let is3 = args.z.is_some();
        let s = Scatter {
            x: args.x.clone(),
            y: args.y.clone(),
            z: args.z.clone().unwrap_or_default(),
            size_data: sizes,
            cdata,
            marker,
            marker_edge: edge,
            marker_face: face,
            line_width: 0.5,
            display_name: String::new(),
        };
        let h = self.add(ax, Kind::Scatter(s));
        if is3 && fresh {
            self.set_view_3d(ax)?;
            let a = self.axes_mut(ax)?;
            a.x.grid = true;
            a.y.grid = true;
            a.z.grid = true;
        }
        self.apply_opts(&[h], opts)?;
        Ok(h)
    }

    /// `stairs(x, y)` / `stairs(y)`: one group per column.
    pub fn stairs(
        &mut self,
        ax: Option<Handle>,
        series: &PlotSeries,
        opts: &Opts,
    ) -> Result<Vec<Handle>> {
        let ax = self.new_plot(ax)?;
        let spec = parse_spec(&series.spec, "stairs")?;
        let mut out = Vec::new();
        for (x, y) in pair_columns(series.x.as_ref(), &series.y, "stairs")? {
            let line = self.spec_line(ax, x, y, &spec)?;
            out.push(self.add(
                ax,
                Kind::Group(Box::new(Group {
                    kind: GroupKind::Stairs(line),
                    display_name: String::new(),
                })),
            ));
        }
        self.apply_opts(&out, opts)?;
        Ok(out)
    }

    fn spec_line(&mut self, ax: Handle, x: Vec<f64>, y: Vec<f64>, spec: &LineSpec) -> Result<Line> {
        let color = self.spec_color(ax, spec)?;
        let mut line = Line::new(x, y, Vec::new(), color);
        if let Some(s) = &spec.line_style {
            line.line_style = s.clone();
        }
        if let Some(m) = &spec.marker {
            line.marker = m.clone();
        }
        Ok(line)
    }

    /// `stem(x, y)` / `stem(y)`; `filled` fills the markers.
    pub fn stem(
        &mut self,
        ax: Option<Handle>,
        series: &PlotSeries,
        filled: bool,
        opts: &Opts,
    ) -> Result<Vec<Handle>> {
        let ax = self.new_plot(ax)?;
        let spec = parse_spec(&series.spec, "stem")?;
        let mut out = Vec::new();
        for (x, y) in pair_columns(series.x.as_ref(), &series.y, "stem")? {
            let mut line = self.spec_line(ax, x, y, &spec)?;
            if spec.marker.is_none() {
                line.marker = "o".into();
            }
            if spec.line_style.as_deref() == Some("none") || spec.line_style.is_none() {
                line.line_style = "-".into();
            }
            // Measured: stem fixes the marker colours at creation; a later
            // 'Color' (name/value or set) recolours only the stems.
            line.marker_edge = Paint::Rgb(line.color);
            if filled {
                line.marker_face = Paint::Rgb(line.color);
            }
            out.push(self.add(
                ax,
                Kind::Group(Box::new(Group {
                    kind: GroupKind::Stem(line, 0.0),
                    display_name: String::new(),
                })),
            ));
        }
        self.apply_opts(&out, opts)?;
        Ok(out)
    }

    /// `bar`/`barh`: one bar group per column of Y.
    pub fn bar(
        &mut self,
        ax: Option<Handle>,
        args: &BarArgs,
        horizontal: bool,
        opts: &Opts,
    ) -> Result<Vec<Handle>> {
        let y = &args.y;
        let columns: Vec<Vec<f64>> = if y.is_vector() {
            vec![vec_of(y)]
        } else {
            (0..y.cols()).map(|c| y.column(c).to_vec()).collect()
        };
        let m = columns.first().map_or(0, Vec::len);
        let x = match &args.x {
            Some(x) if x.len() == m => x.clone(),
            Some(_) => {
                return Err(GraphicsError::args(
                    "bar",
                    "X must match the number of rows of Y",
                ));
            }
            None => (1..=m).map(|k| k as f64).collect(),
        };
        let width = args.width.unwrap_or(0.8);
        if !(width.is_finite() && width > 0.0) {
            return Err(GraphicsError::args("bar", "WIDTH must be positive"));
        }
        let fixed = match &args.color {
            Some(v) => Some(parse_paint(v, "FaceColor", &[])?),
            None => None,
        };
        let ax = self.new_plot(ax)?;
        let fresh = !self.axes_ref(ax)?.hold_add;
        let n = columns.len();
        let mut pos = vec![0.0; m];
        let mut neg = vec![0.0; m];
        let mut out = Vec::new();
        for (k, col) in columns.into_iter().enumerate() {
            let offsets: Vec<f64> = if args.stacked {
                col.iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let slot = if *v < 0.0 { &mut neg[i] } else { &mut pos[i] };
                        let o = *slot;
                        if v.is_finite() {
                            *slot += v;
                        }
                        o
                    })
                    .collect()
            } else {
                vec![0.0; m]
            };
            let face = match fixed {
                Some(p) => p,
                None => Paint::Rgb(self.axes_mut(ax)?.next_color()),
            };
            let kind = GroupKind::Bar {
                x: x.clone(),
                y: col,
                horizontal,
                stacked: args.stacked,
                width,
                base_value: 0.0,
                series: k,
                series_count: n,
                offsets,
                face_color: face,
                edge_color: Paint::Rgb([0.0, 0.0, 0.0]),
                line_width: 0.5,
                line_style: "-".into(),
                has_baseline: k == 0,
            };
            out.push(self.add(
                ax,
                Kind::Group(Box::new(Group {
                    kind,
                    display_name: String::new(),
                })),
            ));
        }
        // Measured: bar puts ticks at the bar positions (manual mode) when
        // they are all integers; `bar([1.5 2.5], ...)` keeps automatic ticks.
        if fresh && x.iter().all(|v| v.is_finite() && v.fract() == 0.0) {
            let a = self.axes_mut(ax)?;
            let axis = if horizontal { &mut a.y } else { &mut a.x };
            let mut ticks = x.clone();
            ticks.sort_by(f64::total_cmp);
            ticks.dedup();
            axis.tick = ticks;
            axis.tick_mode = Mode::Manual;
        }
        self.apply_opts(&out, opts)?;
        Ok(out)
    }

    /// `area(x, y)` / `area(y)`: stacked filled areas, one per column.
    pub fn area(
        &mut self,
        ax: Option<Handle>,
        x: Option<&[f64]>,
        y: &Array,
        opts: &Opts,
    ) -> Result<Vec<Handle>> {
        let columns: Vec<Vec<f64>> = if y.is_vector() {
            vec![vec_of(y)]
        } else {
            (0..y.cols()).map(|c| y.column(c).to_vec()).collect()
        };
        let m = columns.first().map_or(0, Vec::len);
        let x: Vec<f64> = match x {
            Some(x) if x.len() == m => x.to_vec(),
            Some(_) => {
                return Err(GraphicsError::args(
                    "area",
                    "X and Y must have compatible sizes",
                ));
            }
            None => (1..=m).map(|k| k as f64).collect(),
        };
        let single = y.is_vector();
        let ax = self.new_plot(ax)?;
        let mut stack = vec![0.0; m];
        let mut out = Vec::new();
        for col in columns {
            let offsets = stack.clone();
            for (s, v) in stack.iter_mut().zip(&col) {
                *s += v;
            }
            let face = Paint::Rgb(self.axes_mut(ax)?.next_color());
            out.push(self.add(
                ax,
                Kind::Group(Box::new(Group {
                    kind: GroupKind::Area {
                        single,
                        x: x.clone(),
                        y: col,
                        offsets,
                        base_value: 0.0,
                        face_color: face,
                        edge_color: Paint::Rgb([0.0, 0.0, 0.0]),
                        line_width: 0.5,
                        line_style: "-".into(),
                    },
                    display_name: String::new(),
                })),
            ));
        }
        self.apply_opts(&out, opts)?;
        Ok(out)
    }

    /// `fill(x, y, c)`: a filled polygon (clears the axes unless held).
    pub fn fill(
        &mut self,
        ax: Option<Handle>,
        x: &[f64],
        y: &[f64],
        color: &PropValue,
        opts: &Opts,
    ) -> Result<Handle> {
        let ax = self.new_plot(ax)?;
        self.polygon(ax, x, y, color, opts)
    }

    /// `patch(x, y, c)`: like `fill` but never clears the axes.
    pub fn patch(
        &mut self,
        ax: Option<Handle>,
        x: &[f64],
        y: &[f64],
        color: &PropValue,
        opts: &Opts,
    ) -> Result<Handle> {
        let ax = match ax {
            Some(h) => h,
            None => self.gca(),
        };
        self.polygon(ax, x, y, color, opts)
    }

    /// `patch('Vertices', V, 'Faces', F, ...)`: faces one-based, NaN padded.
    pub fn patch_faces(
        &mut self,
        ax: Option<Handle>,
        vertices: Array,
        faces: Array,
        opts: &Opts,
    ) -> Result<Handle> {
        let ax = match ax {
            Some(h) => h,
            None => self.gca(),
        };
        if !(vertices.cols() == 2 || vertices.cols() == 3) {
            return Err(GraphicsError::args("patch", "Vertices must be N×2 or N×3"));
        }
        let p = Patch {
            vertices,
            faces,
            fvcdata: Array::empty(),
            face_color: Paint::Rgb([0.0, 0.0, 0.0]),
            edge_color: Paint::Rgb([0.0, 0.0, 0.0]),
            cdata_direct: false,
            line_style: "-".into(),
            line_width: 0.5,
            marker: "none".into(),
            face_alpha: 1.0,
            edge_alpha: 1.0,
            display_name: String::new(),
        };
        let h = self.add(ax, Kind::Patch(p));
        self.apply_opts(&[h], opts)?;
        Ok(h)
    }

    fn polygon(
        &mut self,
        ax: Handle,
        x: &[f64],
        y: &[f64],
        color: &PropValue,
        opts: &Opts,
    ) -> Result<Handle> {
        if x.len() != y.len() || x.is_empty() {
            return Err(GraphicsError::args(
                "fill",
                "X and Y must be nonempty vectors of the same length",
            ));
        }
        let n = x.len();
        let mut v = x.to_vec();
        v.extend_from_slice(y);
        let (face, fvcdata) = match color {
            PropValue::Str(s) => match parse_paint(color, "FaceColor", &["none", "flat", "interp"])
            {
                Ok(p) => (p, Array::empty()),
                Err(_) => {
                    return Err(GraphicsError::args("fill", format!("invalid colour '{s}'")));
                }
            },
            other => {
                let c = other
                    .as_array()
                    .ok_or_else(|| GraphicsError::args("fill", "invalid colour argument"))?;
                if c.rows() == 1 && c.cols() == 3 {
                    (Paint::Flat, Array::matrix(1, 3, c.data.clone()))
                } else if c.numel() == 1 {
                    (Paint::Flat, Array::scalar(c.data[0]))
                } else if c.numel() == n {
                    (Paint::Interp, Array::col(&c.data))
                } else {
                    return Err(GraphicsError::args(
                        "fill",
                        "colour data does not match the vertices",
                    ));
                }
            }
        };
        let p = Patch {
            vertices: Array::matrix(n, 2, v),
            faces: Array::matrix(1, n, (1..=n).map(|k| k as f64).collect()),
            fvcdata,
            face_color: face,
            edge_color: Paint::Rgb([0.0, 0.0, 0.0]),
            cdata_direct: false,
            line_style: "-".into(),
            line_width: 0.5,
            marker: "none".into(),
            face_alpha: 1.0,
            edge_alpha: 1.0,
            display_name: String::new(),
        };
        let h = self.add(ax, Kind::Patch(p));
        self.apply_opts(&[h], opts)?;
        Ok(h)
    }

    fn grid_coords(args: &GridArgs, func: &str) -> Result<(Array, Array)> {
        let (rows, cols) = (args.z.rows(), args.z.cols());
        match (&args.x, &args.y) {
            (None, None) => Ok((
                Array::row(&(1..=cols).map(|k| k as f64).collect::<Vec<_>>()),
                Array::col(&(1..=rows).map(|k| k as f64).collect::<Vec<_>>()),
            )),
            (Some(x), Some(y)) => {
                let x = if x.is_vector() && x.numel() == cols {
                    Array::row(&x.data)
                } else if x.dims == args.z.dims {
                    x.clone()
                } else {
                    return Err(GraphicsError::args(func, "X must match the columns of Z"));
                };
                let y = if y.is_vector() && y.numel() == rows {
                    Array::col(&y.data)
                } else if y.dims == args.z.dims {
                    y.clone()
                } else {
                    return Err(GraphicsError::args(func, "Y must match the rows of Z"));
                };
                Ok((x, y))
            }
            _ => Err(GraphicsError::args(func, "X and Y must be given together")),
        }
    }

    /// `surf`.
    pub fn surf(&mut self, ax: Option<Handle>, args: &GridArgs, opts: &Opts) -> Result<Handle> {
        self.surface_like(ax, args, false, opts)
    }

    /// `mesh`: white faces with flat-coloured edges.
    pub fn mesh(&mut self, ax: Option<Handle>, args: &GridArgs, opts: &Opts) -> Result<Handle> {
        self.surface_like(ax, args, true, opts)
    }

    fn surface_like(
        &mut self,
        ax: Option<Handle>,
        args: &GridArgs,
        mesh: bool,
        opts: &Opts,
    ) -> Result<Handle> {
        let func = if mesh { "mesh" } else { "surf" };
        if args.z.rows() < 2 || args.z.cols() < 2 || args.z.dims.len() != 2 {
            return Err(GraphicsError::args(
                func,
                "Z must be a matrix with at least 2 rows and columns",
            ));
        }
        let (x, y) = Self::grid_coords(args, func)?;
        let c = match &args.c {
            Some(c) => {
                if c.dims[..2] != args.z.dims[..] {
                    return Err(GraphicsError::args(func, "C must have the same size as Z"));
                }
                c.clone()
            }
            None => args.z.clone(),
        };
        let ax = self.new_plot(ax)?;
        let fresh = !self.axes_ref(ax)?.hold_add;
        let s = Surface {
            x,
            y,
            z: args.z.clone(),
            c,
            face_color: if mesh {
                Paint::Rgb([1.0, 1.0, 1.0])
            } else {
                Paint::Flat
            },
            edge_color: if mesh {
                Paint::Flat
            } else {
                Paint::Rgb([0.0, 0.0, 0.0])
            },
            mesh_style: "both".into(),
            line_style: "-".into(),
            line_width: 0.5,
            cdata_direct: false,
            face_alpha: 1.0,
            edge_alpha: 1.0,
            marker: "none".into(),
            display_name: String::new(),
        };
        let h = self.add(ax, Kind::Surface(s));
        if fresh {
            self.set_view_3d(ax)?;
            // Measured: surf/mesh switch the grid on.
            let a = self.axes_mut(ax)?;
            a.x.grid = true;
            a.y.grid = true;
            a.z.grid = true;
        }
        self.apply_opts(&[h], opts)?;
        Ok(h)
    }

    /// `contour`/`contourf`: a contour group.
    pub fn contour(
        &mut self,
        ax: Option<Handle>,
        args: &GridArgs,
        levels: &Levels,
        filled: bool,
        opts: &Opts,
    ) -> Result<Handle> {
        let func = if filled { "contourf" } else { "contour" };
        if args.z.rows() < 2 || args.z.cols() < 2 || args.z.dims.len() != 2 {
            return Err(GraphicsError::args(
                func,
                "Z must be a matrix with at least 2 rows and columns",
            ));
        }
        let (x, y) = Self::grid_coords(args, func)?;
        let coord = |a: Array| {
            if a.is_vector() {
                GridCoord::Vector(a.data)
            } else {
                GridCoord::Matrix(a.data)
            }
        };
        let (list, mode, count) = match levels {
            Levels::Auto => (Vec::new(), Mode::Auto, None),
            Levels::Count(n) => (Vec::new(), Mode::Auto, Some(*n)),
            Levels::Values(v) => (v.clone(), Mode::Manual, None),
        };
        let ax = self.new_plot(ax)?;
        let kind = GroupKind::Contour {
            x: coord(x),
            y: coord(y),
            z: args.z.clone(),
            levels: list,
            level_mode: mode,
            level_count: count,
            fill: filled,
            line_color: if filled {
                Paint::Rgb([0.0, 0.0, 0.0])
            } else {
                Paint::Flat
            },
            line_style: "-".into(),
            line_width: 0.5,
        };
        let h = self.add(
            ax,
            Kind::Group(Box::new(Group {
                kind,
                display_name: String::new(),
            })),
        );
        self.apply_opts(&[h], opts)?;
        Ok(h)
    }

    /// `imagesc(C)` / `imagesc(x, y, C)` (scaled) and `image` (direct).
    pub fn image(
        &mut self,
        ax: Option<Handle>,
        x: Option<[f64; 2]>,
        y: Option<[f64; 2]>,
        c: Array,
        scaled: bool,
        opts: &Opts,
    ) -> Result<Handle> {
        if c.is_empty() || !(c.dims.len() == 2 || (c.dims.len() == 3 && c.dims[2] == 3)) {
            return Err(GraphicsError::args(
                "image",
                "C must be an M×N or M×N×3 array",
            ));
        }
        let x = x.unwrap_or([1.0, c.cols() as f64]);
        let y = y.unwrap_or([1.0, c.rows() as f64]);
        let ax = self.new_plot(ax)?;
        let fresh = !self.axes_ref(ax)?.hold_add;
        let img = Image {
            cdata: c,
            x,
            y,
            cdata_direct: !scaled,
            display_name: String::new(),
        };
        let h = self.add(ax, Kind::Image(img));
        if fresh {
            let a = self.axes_mut(ax)?;
            a.y.reverse = true;
            a.box_on = true;
        }
        self.apply_opts(&[h], opts)?;
        Ok(h)
    }

    /// `text(x, y[, z], str)` in data units.
    pub fn text(
        &mut self,
        ax: Option<Handle>,
        position: [f64; 3],
        lines: &[String],
        opts: &Opts,
    ) -> Result<Handle> {
        let ax = match ax {
            Some(h) => h,
            None => self.gca(),
        };
        let mut t = Text::new(LabelRole::Free);
        t.position = position;
        t.lines = lines.to_vec();
        t.margin = 3.0;
        let h = self.add(ax, Kind::Text(t));
        self.apply_opts(&[h], opts)?;
        Ok(h)
    }
}

/// Interprets a scatter colour argument.
fn scatter_color(v: &PropValue, n: usize) -> Result<ScatterColor> {
    match v {
        PropValue::Str(_) => match parse_paint(v, "CData", &[])? {
            Paint::Rgb(c) => Ok(ScatterColor::Rgb(c)),
            _ => Err(GraphicsError::args("scatter", "invalid colour")),
        },
        other => {
            let a = other
                .as_array()
                .ok_or_else(|| GraphicsError::args("scatter", "invalid colour"))?;
            if a.numel() == 3 && a.rows() == 1 {
                let c: Rgb = [a.data[0], a.data[1], a.data[2]];
                Ok(ScatterColor::Rgb(c))
            } else if a.numel() == 1 {
                Ok(ScatterColor::Indexed(a.data[0]))
            } else if a.numel() == n || (a.rows() == n && a.cols() == 3) {
                Ok(ScatterColor::PerPoint(a))
            } else {
                Err(GraphicsError::args(
                    "scatter",
                    "C must be a colour, a scalar, N values or N×3",
                ))
            }
        }
    }
}
