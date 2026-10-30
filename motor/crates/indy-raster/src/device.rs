//! Paths and the drawing surface the painters talk to.
//!
//! Coordinates are logical (CSS) pixels, y down, exactly as the UI's canvas
//! code uses them. A device scales them by its own device-pixel ratio. The
//! raster device fills pixels; the SVG and PDF devices write vector output
//! from the same calls.

pub type Rgb = [f64; 3];

#[derive(Debug, Clone, Default)]
pub struct Path {
    pub pts: Vec<[f64; 2]>,
    /// (first point, closed) of every subpath.
    pub subs: Vec<(usize, bool)>,
}

impl Path {
    pub fn new() -> Path {
        Path::default()
    }

    pub fn is_empty(&self) -> bool {
        self.pts.is_empty()
    }

    pub fn move_to(&mut self, x: f64, y: f64) {
        self.subs.push((self.pts.len(), false));
        self.pts.push([x, y]);
    }

    pub fn line_to(&mut self, x: f64, y: f64) {
        if self.subs.is_empty() {
            self.move_to(x, y);
        } else {
            self.pts.push([x, y]);
        }
    }

    pub fn close(&mut self) {
        if let Some(last) = self.subs.last_mut() {
            last.1 = true;
        }
    }

    pub fn rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.move_to(x, y);
        self.line_to(x + w, y);
        self.line_to(x + w, y + h);
        self.line_to(x, y + h);
        self.close();
    }

    pub fn polygon(&mut self, points: &[[f64; 2]]) {
        for (i, p) in points.iter().enumerate() {
            if i == 0 {
                self.move_to(p[0], p[1]);
            } else {
                self.line_to(p[0], p[1]);
            }
        }
        self.close();
    }

    /// A full circle as a polygon, fine enough at `scale` device px per unit.
    pub fn circle(&mut self, x: f64, y: f64, r: f64, scale: f64) {
        let n = ((r * scale).sqrt() * 8.0).ceil().clamp(12.0, 96.0) as usize;
        for i in 0..n {
            let a = i as f64 * std::f64::consts::TAU / n as f64;
            let (px, py) = (x + r * a.cos(), y + r * a.sin());
            if i == 0 {
                self.move_to(px, py);
            } else {
                self.line_to(px, py);
            }
        }
        self.close();
    }

    /// The points of subpath `k` and whether it is closed.
    pub fn sub(&self, k: usize) -> (&[[f64; 2]], bool) {
        let (start, closed) = self.subs[k];
        let end = self.subs.get(k + 1).map_or(self.pts.len(), |s| s.0);
        (&self.pts[start..end], closed)
    }

    pub fn sub_count(&self) -> usize {
        self.subs.len()
    }
}

/// RGB bytes, top row first.
#[derive(Debug, Clone)]
pub struct Bitmap {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
}

pub trait Device {
    /// Device pixels per logical pixel (used to choose curve flattening).
    fn dpr(&self) -> f64;
    fn fill(&mut self, path: &Path, even_odd: bool, color: Rgb, alpha: f64);
    /// Glyph outlines at a font size, always non-zero.
    fn fill_text(&mut self, path: &Path, size: f64, color: Rgb);
    fn stroke(&mut self, path: &Path, width: f64, dash: &[f64], color: Rgb, alpha: f64);
    fn fill_rect(&mut self, x: f64, y: f64, w: f64, h: f64, color: Rgb, alpha: f64);
    /// Nearest-neighbour image over the rectangle (x0,y0)-(x1,y1); `x1 < x0`
    /// or `y1 < y0` mirror it.
    fn image(&mut self, bitmap: &Bitmap, x0: f64, y0: f64, x1: f64, y1: f64);
    /// Rectangular clip `[x, y, w, h]`, replacing the previous one.
    fn clip(&mut self, rect: Option<[f64; 4]>);
    /// Triangle with per-vertex colours. Vector devices use the mean colour.
    fn gouraud(&mut self, tri: &[[f64; 2]; 3], colors: &[Rgb; 3], alpha: f64);
}

pub fn mean_color(colors: &[Rgb; 3]) -> Rgb {
    [
        (colors[0][0] + colors[1][0] + colors[2][0]) / 3.0,
        (colors[0][1] + colors[1][1] + colors[2][1]) / 3.0,
        (colors[0][2] + colors[1][2] + colors[2][2]) / 3.0,
    ]
}

/// Splits a polyline into its dash pieces. The pattern restarts at every
/// subpath, as on a canvas; the closing edge of a closed path is included.
pub fn apply_dash(points: &[[f64; 2]], closed: bool, dash: &[f64]) -> Vec<Vec<[f64; 2]>> {
    let cycle: f64 = dash.iter().sum();
    let mut out = Vec::new();
    if !(cycle > 0.0) || points.len() < 2 {
        return out;
    }
    let mut part = 0usize;
    let mut left = dash[0];
    let mut current: Vec<[f64; 2]> = vec![points[0]];
    let count = if closed {
        points.len()
    } else {
        points.len() - 1
    };
    for i in 0..count {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = (dx * dx + dy * dy).sqrt();
        if !(len > 0.0) || !len.is_finite() {
            continue;
        }
        let mut at = 0.0;
        while len - at > left {
            at += left;
            let p = [a[0] + dx * at / len, a[1] + dy * at / len];
            if part.is_multiple_of(2) {
                current.push(p);
                out.push(std::mem::take(&mut current));
            }
            part = (part + 1) % dash.len();
            left = dash[part];
            if part.is_multiple_of(2) {
                current = vec![p];
            }
            // A guard for dashes that vanish against a huge coordinate.
            if out.len() > 2_000_000 {
                return out;
            }
        }
        left -= len - at;
        if part.is_multiple_of(2) {
            current.push(b);
        }
    }
    if part.is_multiple_of(2) && current.len() >= 2 {
        out.push(current);
    }
    out
}
