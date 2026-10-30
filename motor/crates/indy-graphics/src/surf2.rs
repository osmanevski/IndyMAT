//! Surface-family commands of lane B: `shading`, `imshow`, `contour3`,
//! `surfc`/`meshc` and `bar3`.
//!
//! Measured black-box against Octave 11.3 (`octave-cli`, fltk, property
//! reads; no Octave source read) where Octave has the function:
//!
//! * `shading flat|interp|faceted` sets FaceColor flat/interp/flat and
//!   EdgeColor none/none/black on every surface and patch of the axes
//!   (hidden ones too). A face that is `none` or white (`mesh`) keeps its
//!   face colour and only gets EdgeColor flat/interp/flat. Contour, bar and
//!   other groups are not touched.
//! * `imshow`: an image (grayscale `scaled`, indexed and RGB `direct`) in
//!   an invisible axes with DataAspectRatio `[1 1 1]` (manual), YDir
//!   reverse, limits on the pixel edges; grayscale sets CLim to the display
//!   range (manual) and the axes colormap to `gray(256)`; `imshow(X,map)`
//!   puts `map` on the axes.
//! * `contour3` draws the contour levels above the minimum (as `contour`)
//!   at z = level, sets view `[-37.5 30]` and the grid on; `surfc`/`meshc`
//!   put the contours of Z on the plane z = lower z limit of the axes right
//!   after the surface is drawn, and return `[surface; contour group]`.
//!
//! `bar3` does not exist in Octave 11.3; it follows the MATLAB
//! documentation (one surface per column of Z, bar width 0.8, `detached`
//! default, view 3) and is not measured.

use crate::color::{Colormap, Rgb, colormap_by_name};
use crate::error::{GraphicsError, Result};
use crate::graphics::{Graphics, Opts};
use crate::linespec::LineSpec;
use crate::model::{Contour3, GroupKind, Handle, Kind, Line, Mode, Paint, Surface};
use crate::plots::{GridArgs, Levels};
use crate::scene::{Leaf, LeafObj};
use crate::value::{Array, NumClass};

/// An explicit "not available yet" error for an unsupported form.
pub fn not_yet(func: &str, what: impl std::fmt::Display) -> GraphicsError {
    GraphicsError::new(
        "IndyR:graphicsUnavailable",
        format!("{func}: {what} is not available in the Rust engine yet"),
    )
}

/// `shading` modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shading {
    Flat,
    Interp,
    Faceted,
}

impl Shading {
    /// Case-insensitive mode name.
    pub fn parse(s: &str) -> Option<Shading> {
        match s.to_ascii_lowercase().as_str() {
            "flat" => Some(Shading::Flat),
            "interp" => Some(Shading::Interp),
            "faceted" => Some(Shading::Faceted),
            _ => None,
        }
    }
}

/// The colours `shading` gives one face/edge pair (measured rule).
fn shade(face: &mut Paint, edge: &mut Paint, mode: Shading) {
    let edges_only = *face == Paint::None || *face == Paint::Rgb([1.0, 1.0, 1.0]);
    if edges_only {
        *edge = if mode == Shading::Interp {
            Paint::Interp
        } else {
            Paint::Flat
        };
        return;
    }
    let (f, e) = match mode {
        Shading::Flat => (Paint::Flat, Paint::None),
        Shading::Interp => (Paint::Interp, Paint::None),
        Shading::Faceted => (Paint::Flat, Paint::Rgb([0.0, 0.0, 0.0])),
    };
    *face = f;
    *edge = e;
}

/// How `imshow` maps the image data.
#[derive(Debug, Clone, PartialEq)]
pub enum ImshowKind {
    /// Intensity image shown through `gray(256)` with this display range.
    Gray { range: [f64; 2] },
    /// `imshow(X, map)`: indices into `map` (direct mapping).
    Indexed { map: Colormap },
    /// M×N×3 truecolor image.
    Truecolor,
}

/// Arguments of `imshow`.
#[derive(Debug, Clone)]
pub struct ImshowArgs {
    pub cdata: Array,
    pub kind: ImshowKind,
    pub x: Option<[f64; 2]>,
    pub y: Option<[f64; 2]>,
    /// `'Colormap', map` option.
    pub colormap: Option<Colormap>,
}

/// The default display range of an intensity image class (MATLAB
/// `imshow`; double/single/logical are `[0 1]`).
pub fn default_range(class: NumClass) -> Option<[f64; 2]> {
    Some(match class {
        NumClass::Double | NumClass::Single => [0.0, 1.0],
        NumClass::UInt8 => [0.0, 255.0],
        NumClass::UInt16 => [0.0, 65535.0],
        NumClass::Int16 => [-32768.0, 32767.0],
        _ => return None,
    })
}

/// `bar3` layouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bar3Style {
    Detached,
    Stacked,
}

/// Arguments of `bar3`.
#[derive(Debug, Clone)]
pub struct Bar3Args {
    /// Bar positions along y (one per row of Z); `None` = `1:m`.
    pub y: Option<Vec<f64>>,
    pub z: Array,
    pub width: f64,
    pub style: Bar3Style,
    pub color: Option<Rgb>,
}

impl Graphics {
    /// `shading(ax, mode)`.
    pub fn shading(&mut self, ax: Option<Handle>, mode: Shading) -> Result<()> {
        let ax = match ax {
            Some(h) => {
                self.axes_ref(h)?;
                h
            }
            None => self.gca(),
        };
        for h in self.obj(ax)?.children.clone() {
            match &mut self.obj_mut(h)?.kind {
                Kind::Surface(s) => shade(&mut s.face_color, &mut s.edge_color, mode),
                Kind::Patch(p) => shade(&mut p.face_color, &mut p.edge_color, mode),
                _ => {}
            }
        }
        Ok(())
    }

    /// `imshow(...)`: the image plus the axes setup of `imshow`.
    pub fn imshow(&mut self, ax: Option<Handle>, args: ImshowArgs, opts: &Opts) -> Result<Handle> {
        let c = &args.cdata;
        let rgb = c.dims.len() == 3 && c.dims[2] == 3;
        match args.kind {
            ImshowKind::Truecolor if !rgb => {
                return Err(GraphicsError::args("imshow", "RGB must be M×N×3"));
            }
            ImshowKind::Gray { .. } | ImshowKind::Indexed { .. } if c.dims.len() != 2 => {
                return Err(GraphicsError::args("imshow", "I must be a 2-D matrix"));
            }
            _ => {}
        }
        if c.is_empty() {
            return Err(GraphicsError::args("imshow", "the image must not be empty"));
        }
        if let ImshowKind::Gray { range } = args.kind
            && !(range[0].is_finite() && range[1].is_finite() && range[0] < range[1])
        {
            return Err(GraphicsError::args(
                "imshow",
                "the display range must be [LOW HIGH] with LOW < HIGH",
            ));
        }
        if let ImshowKind::Indexed { map } = &args.kind
            && (map.is_empty() || map.iter().flatten().any(|v| !(0.0..=1.0).contains(v)))
        {
            return Err(GraphicsError::args(
                "imshow",
                "MAP must be an N×3 matrix with values in [0,1]",
            ));
        }
        let scaled = matches!(args.kind, ImshowKind::Gray { .. });
        let h = self.image(ax, args.x, args.y, args.cdata, scaled, &[])?;
        let ax = self
            .ancestor_axes(h)
            .ok_or_else(GraphicsError::invalid_handle)?;
        let a = self.axes_mut(ax)?;
        a.y.reverse = true;
        a.box_on = true;
        a.dar = [1.0, 1.0, 1.0];
        a.dar_mode = Mode::Manual;
        match args.kind {
            ImshowKind::Gray { range } => {
                a.clim = range;
                a.clim_mode = Mode::Manual;
                a.colormap = colormap_by_name("gray", 256);
            }
            ImshowKind::Indexed { map } => a.colormap = Some(map),
            ImshowKind::Truecolor => {}
        }
        if let Some(map) = args.colormap {
            if map.is_empty() || map.iter().flatten().any(|v| !(0.0..=1.0).contains(v)) {
                return Err(GraphicsError::bad_value("Colormap"));
            }
            a.colormap = Some(map);
        }
        self.obj_mut(ax)?.visible = false;
        self.apply_opts(&[h], opts)?;
        Ok(h)
    }

    /// Turns the contour group `h` into a 3-D contour group.
    fn make_contour3(&mut self, h: Handle, plane: Option<f64>) -> Result<()> {
        if let Kind::Group(g) = &mut self.obj_mut(h)?.kind {
            let placeholder = GroupKind::Stairs(Line::new(vec![], vec![], vec![], [0.0; 3]));
            let contour = std::mem::replace(&mut g.kind, placeholder);
            g.kind = GroupKind::Contour3(Box::new(Contour3 { contour, plane }));
        }
        Ok(())
    }

    /// `contour3(...)`: contour lines at z = level in a 3-D view. A
    /// LineSpec gives the line colour and style.
    pub fn contour3(
        &mut self,
        ax: Option<Handle>,
        args: &GridArgs,
        levels: &Levels,
        spec: Option<&LineSpec>,
        opts: &Opts,
    ) -> Result<Handle> {
        if args.z.rows() < 2 || args.z.cols() < 2 || args.z.dims.len() != 2 {
            return Err(GraphicsError::args(
                "contour3",
                "Z must be a matrix with at least 2 rows and columns",
            ));
        }
        if spec.is_some_and(|s| s.marker.is_some()) {
            return Err(not_yet("contour3", "a marker in the LineSpec"));
        }
        let target = match ax {
            Some(h) => {
                self.axes_ref(h)?;
                h
            }
            None => self.gca(),
        };
        let fresh = !self.axes_ref(target)?.hold_add;
        let h = self.contour(Some(target), args, levels, false, &[])?;
        self.make_contour3(h, None)?;
        if let Some(s) = spec {
            if let Some(c) = s.color {
                self.set(h, "LineColor", crate::value::PropValue::row(&c))?;
            }
            if let Some(ls) = &s.line_style {
                self.set(h, "LineStyle", crate::value::PropValue::str(ls))?;
            }
        }
        if fresh {
            self.set_view_3d(target)?;
            let a = self.axes_mut(target)?;
            a.x.grid = true;
            a.y.grid = true;
            a.z.grid = true;
        }
        self.apply_opts(&[h], opts)?;
        Ok(h)
    }

    /// `surfc` / `meshc` (`mesh`): the surface, then the contours of Z on
    /// the plane at the lower z limit. Options go to the surface.
    pub fn surfc(
        &mut self,
        ax: Option<Handle>,
        args: &GridArgs,
        mesh: bool,
        opts: &Opts,
    ) -> Result<Vec<Handle>> {
        let s = if mesh {
            self.mesh(ax, args, opts)?
        } else {
            self.surf(ax, args, opts)?
        };
        let ax = self
            .ancestor_axes(s)
            .ok_or_else(GraphicsError::invalid_handle)?;
        let plane = self
            .axes_state(ax)
            .ok_or_else(GraphicsError::invalid_handle)?
            .lim[2][0];
        let hold = self.axes_ref(ax)?.hold_add;
        self.axes_mut(ax)?.hold_add = true;
        let grid = GridArgs {
            c: None,
            ..args.clone()
        };
        let c = self.contour(Some(ax), &grid, &Levels::Auto, false, &[]);
        self.axes_mut(ax)?.hold_add = hold;
        let c = c?;
        self.make_contour3(c, Some(plane))?;
        Ok(vec![s, c])
    }

    /// `bar3(...)`: one closed-box surface per column of Z.
    pub fn bar3(
        &mut self,
        ax: Option<Handle>,
        args: &Bar3Args,
        opts: &Opts,
    ) -> Result<Vec<Handle>> {
        let z = &args.z;
        if z.is_empty() || z.dims.len() != 2 {
            return Err(GraphicsError::args(
                "bar3",
                "Z must be a nonempty vector or matrix",
            ));
        }
        let (m, n) = if z.is_vector() {
            (z.numel(), 1)
        } else {
            (z.rows(), z.cols())
        };
        let y: Vec<f64> = match &args.y {
            Some(y) if y.len() == m => y.clone(),
            Some(_) => {
                return Err(GraphicsError::args(
                    "bar3",
                    "Y must have one value per row of Z",
                ));
            }
            None => (1..=m).map(|k| k as f64).collect(),
        };
        if y.iter().any(|v| !v.is_finite()) {
            return Err(GraphicsError::args("bar3", "Y must be finite"));
        }
        let w = args.width;
        if !(w.is_finite() && w > 0.0) {
            return Err(GraphicsError::args("bar3", "WIDTH must be positive"));
        }
        let mut sorted = y.clone();
        sorted.sort_by(f64::total_cmp);
        let dy = sorted
            .windows(2)
            .map(|p| p[1] - p[0])
            .filter(|d| *d > 0.0)
            .fold(f64::INFINITY, f64::min);
        let dy = if dy.is_finite() { dy } else { 1.0 };
        let ax = self.new_plot(ax)?;
        let fresh = !self.axes_ref(ax)?.hold_add;
        let mut bottom = vec![0.0; m];
        let mut out = Vec::with_capacity(n);
        for j in 0..n {
            let col = &z.data[j * m..(j + 1) * m];
            let xc = match args.style {
                Bar3Style::Detached => (j + 1) as f64,
                Bar3Style::Stacked => 1.0,
            };
            let mut base = vec![0.0; m];
            let mut top = col.to_vec();
            if args.style == Bar3Style::Stacked {
                for i in 0..m {
                    base[i] = bottom[i];
                    let v = if col[i].is_nan() { 0.0 } else { col[i] };
                    top[i] = bottom[i] + v;
                    bottom[i] = top[i];
                }
            }
            let s = bar3_surface(&y, xc, w / 2.0, w * dy / 2.0, &base, &top, (j + 1) as f64);
            let mut s = s;
            if let Some(c) = args.color {
                s.face_color = Paint::Rgb(c);
            }
            out.push(self.add(ax, Kind::Surface(s)));
        }
        if fresh {
            self.set_view_3d(ax)?;
            let a = self.axes_mut(ax)?;
            a.x.grid = true;
            a.y.grid = true;
            a.z.grid = true;
            a.y.reverse = true;
        }
        self.apply_opts(&out, opts)?;
        Ok(out)
    }
}

/// One `bar3` series as a surface: each bar is a 4×4 vertex block (rows
/// y0 y0 y1 y1, columns x0 x0 x1 x1, the inner 2×2 at the top, the outer
/// ring at the base), blocks separated by a NaN row. A NaN height leaves
/// its block NaN (no bar).
fn bar3_surface(
    y: &[f64],
    xc: f64,
    hx: f64,
    hy: f64,
    base: &[f64],
    top: &[f64],
    series: f64,
) -> Surface {
    let m = y.len();
    let rows = 5 * m - 1;
    let (mut xs, mut ys, mut zs) = (
        vec![f64::NAN; rows * 4],
        vec![f64::NAN; rows * 4],
        vec![f64::NAN; rows * 4],
    );
    let xcol = [xc - hx, xc - hx, xc + hx, xc + hx];
    for i in 0..m {
        let yrow = [y[i] - hy, y[i] - hy, y[i] + hy, y[i] + hy];
        let missing = top[i].is_nan() || base[i].is_nan();
        for (r, &yv) in yrow.iter().enumerate() {
            for (c, &xv) in xcol.iter().enumerate() {
                let k = (5 * i + r) + c * rows;
                xs[k] = xv;
                ys[k] = yv;
                let inner = (1..=2).contains(&r) && (1..=2).contains(&c);
                zs[k] = if missing {
                    f64::NAN
                } else if inner {
                    top[i]
                } else {
                    base[i]
                };
            }
        }
    }
    Surface {
        x: Array::matrix(rows, 4, xs),
        y: Array::matrix(rows, 4, ys),
        z: Array::matrix(rows, 4, zs),
        c: Array::matrix(rows, 4, vec![series; rows * 4]),
        face_color: Paint::Flat,
        edge_color: Paint::Rgb([0.0, 0.0, 0.0]),
        mesh_style: "both".into(),
        line_style: "-".into(),
        line_width: 0.5,
        cdata_direct: false,
        face_alpha: 1.0,
        edge_alpha: 1.0,
        marker: "none".into(),
        display_name: String::new(),
    }
}

/// The 3-D leaves of a `Contour3` group from the 2-D contour leaves of its
/// inner contour: every vertex gets z = level (or the group's plane).
pub(crate) fn contour3_leaves(c: &Contour3, flat: Vec<Leaf>) -> Vec<Leaf> {
    flat.into_iter()
        .filter_map(|leaf| {
            let LeafObj::ContourLine {
                level,
                x,
                y,
                color,
                line_style,
                line_width,
            } = leaf.obj
            else {
                return None;
            };
            let zv = c.plane.unwrap_or(level);
            Some(Leaf {
                obj: LeafObj::Contour3 {
                    level,
                    z: vec![zv; x.len()],
                    x,
                    y,
                    color,
                    line_style,
                    line_width,
                },
                ..leaf
            })
        })
        .collect()
}

/// The `z` of a 3-D contour record: closed paths repeat the first vertex
/// (as `contour_line_record` does for x and y).
pub(crate) fn contour3_z(x: &[f64], y: &[f64], z: &[f64]) -> Vec<f64> {
    let closed = x.len() >= 2 && x.iter().chain(y).all(|v| v.is_finite());
    let mut z = z.to_vec();
    if closed && let Some(&first) = z.first() {
        z.push(first);
    }
    z
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shading_rule_matches_octave() {
        let run = |f: Paint, e: Paint, m| {
            let (mut f, mut e) = (f, e);
            shade(&mut f, &mut e, m);
            (f, e)
        };
        let black = Paint::Rgb([0.0; 3]);
        let white = Paint::Rgb([1.0; 3]);
        assert_eq!(
            run(Paint::Flat, black, Shading::Interp),
            (Paint::Interp, Paint::None)
        );
        assert_eq!(
            run(Paint::Rgb([1., 0., 0.]), black, Shading::Faceted),
            (Paint::Flat, black)
        );
        assert_eq!(
            run(white, Paint::Flat, Shading::Interp),
            (white, Paint::Interp)
        );
        assert_eq!(
            run(Paint::None, Paint::None, Shading::Faceted),
            (Paint::None, Paint::Flat)
        );
    }

    #[test]
    fn bar3_block_is_a_closed_box() {
        let s = bar3_surface(&[1.0, 2.0], 1.0, 0.4, 0.4, &[0.0, 0.0], &[3.0, 5.0], 1.0);
        assert_eq!(s.z.dims, vec![9, 4]);
        // Top of bar 1 at rows 1..2, columns 1..2; ring at 0; separator NaN.
        assert_eq!(s.z.at(1, 1), 3.0);
        assert_eq!(s.z.at(0, 1), 0.0);
        assert!(s.z.at(4, 0).is_nan());
        assert_eq!(s.z.at(6, 2), 5.0);
        assert_eq!((s.x.at(0, 0), s.x.at(0, 3)), (0.6, 1.4));
        assert!((s.y.at(5, 0) - 1.6).abs() < 1e-12);
    }
}
