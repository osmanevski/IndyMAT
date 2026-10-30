//! The raster device: anti-aliased polygon filling and stroking.
//!
//! Scanline filling with exact horizontal coverage on sub-scanlines, for both
//! fill rules. A stroke becomes one non-zero polygon set: a quad per segment
//! plus the join wedge (miter limit 10, as a canvas), every piece oriented the
//! same way so that overlaps never cancel. Blending is in sRGB space, as a
//! canvas does.

use crate::device::{Bitmap, Device, Path, Rgb, apply_dash};

/// Glyph dilation per unit of font size, in the raster device only. The UI's
/// text pipeline (macOS font smoothing) lays down this much more ink than the
/// bare outline: measured over ticks and titles, see the lane report.
const TEXT_DILATION: f64 = 0.035;

#[derive(Clone, Copy)]
struct Edge {
    x0: f64,
    y0: f64,
    y1: f64,
    slope: f64,
    winding: i32,
}

pub struct Raster {
    pub width: usize,
    pub height: usize,
    /// RGB bytes, top row first.
    pub rgb: Vec<u8>,
    dpr: f64,
    /// Device-pixel clip `[x0, y0, x1, y1]`.
    clip_rect: [f64; 4],
    cover: Vec<f32>,
    delta: Vec<f32>,
}

impl Raster {
    pub fn new(width: usize, height: usize, dpr: f64, background: Rgb) -> Raster {
        let bytes = [
            to_byte(background[0]),
            to_byte(background[1]),
            to_byte(background[2]),
        ];
        let mut rgb = Vec::with_capacity(width * height * 3);
        for _ in 0..width * height {
            rgb.extend_from_slice(&bytes);
        }
        Raster {
            width,
            height,
            rgb,
            dpr,
            clip_rect: [0.0, 0.0, width as f64, height as f64],
            cover: vec![0.0; width + 2],
            delta: vec![0.0; width + 2],
        }
    }

    fn blend(&mut self, x: usize, y: usize, color: [f32; 3], a: f32) {
        let i = (y * self.width + x) * 3;
        let px = &mut self.rgb[i..i + 3];
        for k in 0..3 {
            let d = px[k] as f32;
            px[k] = (d + (color[k] - d) * a + 0.5) as u8;
        }
    }

    /// Adds the polygon's edges in device coordinates.
    fn add_polygon(edges: &mut Vec<Edge>, points: &[[f64; 2]], winding: i32) {
        let n = points.len();
        for i in 0..n {
            let a = points[i];
            let b = points[(i + 1) % n];
            if !(a[0].is_finite() && a[1].is_finite() && b[0].is_finite() && b[1].is_finite()) {
                continue;
            }
            if a[1] == b[1] {
                continue;
            }
            let (top, bottom, w) = if a[1] < b[1] {
                (a, b, winding)
            } else {
                (b, a, -winding)
            };
            edges.push(Edge {
                x0: top[0],
                y0: top[1],
                y1: bottom[1],
                slope: (bottom[0] - top[0]) / (bottom[1] - top[1]),
                winding: w,
            });
        }
    }

    fn fill_edges(&mut self, edges: &mut [Edge], even_odd: bool, color: Rgb, alpha: f64) {
        if edges.is_empty() {
            return;
        }
        let [cx0, cy0, cx1, cy1] = self.clip_rect;
        if cx1 <= cx0 || cy1 <= cy0 {
            return;
        }
        edges.sort_unstable_by(|a, b| a.y0.total_cmp(&b.y0));
        let ymax = edges.iter().map(|e| e.y1).fold(f64::NEG_INFINITY, f64::max);
        let first = edges[0].y0.max(cy0).floor().max(0.0) as usize;
        let last = (ymax.min(cy1).ceil().min(self.height as f64)) as usize;
        let subs: usize = if edges.len() <= 256 { 16 } else { 8 };
        let weight = 1.0 / subs as f32;
        let ink = [
            (color[0].clamp(0.0, 1.0) * 255.0) as f32,
            (color[1].clamp(0.0, 1.0) * 255.0) as f32,
            (color[2].clamp(0.0, 1.0) * 255.0) as f32,
        ];
        let alpha = alpha.clamp(0.0, 1.0) as f32;
        let (xlo, xhi) = (cx0.max(0.0), cx1.min(self.width as f64));
        let mut next = 0usize;
        let mut active: Vec<usize> = Vec::new();
        let mut crossings: Vec<(f64, i32)> = Vec::new();
        for y in first..last {
            let mut touched_lo = usize::MAX;
            let mut touched_hi = 0usize;
            for s in 0..subs {
                let sy = y as f64 + (s as f64 + 0.5) / subs as f64;
                while next < edges.len() && edges[next].y0 <= sy {
                    active.push(next);
                    next += 1;
                }
                active.retain(|&i| edges[i].y1 > sy);
                crossings.clear();
                for &i in &active {
                    let e = &edges[i];
                    crossings.push((e.x0 + (sy - e.y0) * e.slope, e.winding));
                }
                crossings.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
                let mut w = 0i32;
                for k in 0..crossings.len().saturating_sub(1) {
                    w += crossings[k].1;
                    let inside = if even_odd { (k + 1) % 2 == 1 } else { w != 0 };
                    if !inside {
                        continue;
                    }
                    let x0 = crossings[k].0.max(xlo);
                    let x1 = crossings[k + 1].0.min(xhi);
                    if x1 <= x0 {
                        continue;
                    }
                    let (i0, i1) = (x0 as usize, x1 as usize);
                    touched_lo = touched_lo.min(i0);
                    touched_hi = touched_hi.max(i1);
                    if i0 == i1 {
                        self.cover[i0] += (x1 - x0) as f32;
                    } else {
                        self.cover[i0] += (i0 as f64 + 1.0 - x0) as f32;
                        self.cover[i1] += (x1 - i1 as f64) as f32;
                        self.delta[i0 + 1] += 1.0;
                        self.delta[i1] -= 1.0;
                    }
                }
            }
            if touched_lo == usize::MAX {
                continue;
            }
            let row_clip = overlap(y as f64, cy0, cy1) as f32;
            let hi = touched_hi.min(self.width.saturating_sub(1));
            let mut run = 0.0f32;
            for x in touched_lo..=hi {
                run += self.delta[x];
                self.delta[x] = 0.0;
                let a = (self.cover[x] + run) * weight;
                self.cover[x] = 0.0;
                if a <= 0.0 {
                    continue;
                }
                self.blend(x, y, ink, a.min(1.0) * alpha * row_clip);
            }
            // The cells one past the row's end.
            self.delta[touched_hi.min(self.width + 1)] = 0.0;
            self.cover[touched_hi.min(self.width + 1)] = 0.0;
        }
    }

    fn scaled(&self, pts: &[[f64; 2]]) -> Vec<[f64; 2]> {
        pts.iter()
            .map(|p| [p[0] * self.dpr, p[1] * self.dpr])
            .collect()
    }

    /// Non-zero polygon pieces of one stroked polyline, device coordinates.
    fn stroke_pieces(edges: &mut Vec<Edge>, pts: &[[f64; 2]], closed: bool, half: f64) {
        // Consecutive duplicates have no direction.
        let mut p: Vec<[f64; 2]> = Vec::with_capacity(pts.len());
        for q in pts {
            if !(q[0].is_finite() && q[1].is_finite()) {
                continue;
            }
            if p.last()
                .is_none_or(|l: &[f64; 2]| l[0] != q[0] || l[1] != q[1])
            {
                p.push(*q);
            }
        }
        if closed && p.len() > 1 && p[0] == p[p.len() - 1] {
            p.pop();
        }
        if p.len() < 2 {
            return;
        }
        let n = p.len();
        let segs = if closed { n } else { n - 1 };
        let dir = |i: usize| {
            let a = p[i % n];
            let b = p[(i + 1) % n];
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let l = (dx * dx + dy * dy).sqrt();
            [dx / l, dy / l]
        };
        let positive = |edges: &mut Vec<Edge>, poly: &[[f64; 2]]| {
            let mut area = 0.0;
            for i in 0..poly.len() {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                area += a[0] * b[1] - b[0] * a[1];
            }
            Self::add_polygon(edges, poly, if area >= 0.0 { 1 } else { -1 });
        };
        for i in 0..segs {
            let d = dir(i);
            let (nx, ny) = (-d[1] * half, d[0] * half);
            let (a, b) = (p[i], p[(i + 1) % n]);
            positive(
                edges,
                &[
                    [a[0] + nx, a[1] + ny],
                    [b[0] + nx, b[1] + ny],
                    [b[0] - nx, b[1] - ny],
                    [a[0] - nx, a[1] - ny],
                ],
            );
            let has_join = closed || i + 1 < segs;
            if !has_join {
                continue;
            }
            let e = dir(i + 1);
            let cross = d[0] * e[1] - d[1] * e[0];
            if cross.abs() < 1e-9 && d[0] * e[0] + d[1] * e[1] > 0.0 {
                continue;
            }
            let side = if cross > 0.0 { -1.0 } else { 1.0 };
            let (mx, my) = (nx * side, ny * side);
            let (ox, oy) = (-e[1] * half * side, e[0] * half * side);
            let c = b;
            let cos_half = ((1.0 + d[0] * e[0] + d[1] * e[1]) / 2.0).max(0.0).sqrt();
            // Miter length over line width is 1/sin(theta/2) = 1/cos(turn/2).
            if cos_half > 0.1 {
                let (sx, sy) = (mx + ox, my + oy);
                let l = (sx * sx + sy * sy).sqrt();
                let reach = half / cos_half;
                positive(
                    edges,
                    &[
                        c,
                        [c[0] + mx, c[1] + my],
                        [c[0] + sx / l * reach, c[1] + sy / l * reach],
                        [c[0] + ox, c[1] + oy],
                    ],
                );
            } else {
                positive(edges, &[c, [c[0] + mx, c[1] + my], [c[0] + ox, c[1] + oy]]);
            }
        }
    }
}

fn to_byte(v: f64) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// The part of the unit cell at `at` inside `[lo, hi]`.
fn overlap(at: f64, lo: f64, hi: f64) -> f64 {
    ((at + 1.0).min(hi) - at.max(lo)).clamp(0.0, 1.0)
}

impl Device for Raster {
    fn dpr(&self) -> f64 {
        self.dpr
    }

    fn fill(&mut self, path: &Path, even_odd: bool, color: Rgb, alpha: f64) {
        let mut edges = Vec::new();
        for k in 0..path.sub_count() {
            let (pts, _) = path.sub(k);
            Self::add_polygon(&mut edges, &self.scaled(pts), 1);
        }
        self.fill_edges(&mut edges, even_odd, color, alpha);
    }

    fn fill_text(&mut self, path: &Path, size: f64, color: Rgb) {
        let half = TEXT_DILATION * size * self.dpr / 2.0;
        let mut edges = Vec::new();
        for k in 0..path.sub_count() {
            let (pts, _) = path.sub(k);
            let scaled = self.scaled(pts);
            Self::add_polygon(&mut edges, &scaled, 1);
            Self::stroke_pieces(&mut edges, &scaled, true, half);
        }
        self.fill_edges(&mut edges, false, color, 1.0);
    }

    fn stroke(&mut self, path: &Path, width: f64, dash: &[f64], color: Rgb, alpha: f64) {
        if !(width > 0.0) {
            return;
        }
        let half = width * self.dpr / 2.0;
        let mut edges = Vec::new();
        for k in 0..path.sub_count() {
            let (pts, closed) = path.sub(k);
            if dash.is_empty() {
                Self::stroke_pieces(&mut edges, &self.scaled(pts), closed, half);
            } else {
                for piece in apply_dash(pts, closed, dash) {
                    Self::stroke_pieces(&mut edges, &self.scaled(&piece), false, half);
                }
            }
        }
        self.fill_edges(&mut edges, false, color, alpha);
    }

    fn fill_rect(&mut self, x: f64, y: f64, w: f64, h: f64, color: Rgb, alpha: f64) {
        let [cx0, cy0, cx1, cy1] = self.clip_rect;
        let (x0, x1) = (
            (x.min(x + w) * self.dpr).max(cx0),
            (x.max(x + w) * self.dpr).min(cx1),
        );
        let (y0, y1) = (
            (y.min(y + h) * self.dpr).max(cy0),
            (y.max(y + h) * self.dpr).min(cy1),
        );
        if !(x1 > x0 && y1 > y0) {
            return;
        }
        let ink = [
            (color[0].clamp(0.0, 1.0) * 255.0) as f32,
            (color[1].clamp(0.0, 1.0) * 255.0) as f32,
            (color[2].clamp(0.0, 1.0) * 255.0) as f32,
        ];
        let alpha = alpha.clamp(0.0, 1.0) as f32;
        let (ix0, ix1) = (x0.floor() as usize, (x1.ceil() as usize).min(self.width));
        let (iy0, iy1) = (y0.floor() as usize, (y1.ceil() as usize).min(self.height));
        for py in iy0..iy1 {
            let ay = overlap(py as f64, y0, y1) as f32;
            for px in ix0..ix1 {
                let ax = overlap(px as f64, x0, x1) as f32;
                self.blend(px, py, ink, ax * ay * alpha);
            }
        }
    }

    fn image(&mut self, b: &Bitmap, x0: f64, y0: f64, x1: f64, y1: f64) {
        let d = self.dpr;
        let (dx0, dy0, dx1, dy1) = (x0 * d, y0 * d, x1 * d, y1 * d);
        if dx0 == dx1 || dy0 == dy1 || b.width == 0 || b.height == 0 {
            return;
        }
        let [cx0, cy0, cx1, cy1] = self.clip_rect;
        let (lx, hx) = (dx0.min(dx1).max(cx0), dx0.max(dx1).min(cx1));
        let (ly, hy) = (dy0.min(dy1).max(cy0), dy0.max(dy1).min(cy1));
        if !(hx > lx && hy > ly) {
            return;
        }
        for py in ly.floor() as usize..(hy.ceil() as usize).min(self.height) {
            let v = (py as f64 + 0.5 - dy0) / (dy1 - dy0);
            let ay = overlap(py as f64, ly, hy) as f32;
            let row = ((v * b.height as f64) as isize).clamp(0, b.height as isize - 1) as usize;
            for px in lx.floor() as usize..(hx.ceil() as usize).min(self.width) {
                let u = (px as f64 + 0.5 - dx0) / (dx1 - dx0);
                let col = ((u * b.width as f64) as isize).clamp(0, b.width as isize - 1) as usize;
                let i = (row * b.width + col) * 3;
                let ink = [b.rgb[i] as f32, b.rgb[i + 1] as f32, b.rgb[i + 2] as f32];
                let a = overlap(px as f64, lx, hx) as f32 * ay;
                self.blend(px, py, ink, a);
            }
        }
    }

    fn clip(&mut self, rect: Option<[f64; 4]>) {
        let (w, h) = (self.width as f64, self.height as f64);
        self.clip_rect = match rect {
            Some([x, y, cw, ch]) => {
                let d = self.dpr;
                [
                    (x * d).max(0.0),
                    (y * d).max(0.0),
                    ((x + cw) * d).min(w),
                    ((y + ch) * d).min(h),
                ]
            }
            None => [0.0, 0.0, w, h],
        };
    }

    fn gouraud(&mut self, tri: &[[f64; 2]; 3], colors: &[Rgb; 3], alpha: f64) {
        let d = self.dpr;
        let t: Vec<[f64; 2]> = tri.iter().map(|p| [p[0] * d, p[1] * d]).collect();
        let area =
            (t[1][0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[2][0] - t[0][0]) * (t[1][1] - t[0][1]);
        if area == 0.0 || !area.is_finite() {
            return;
        }
        let [cx0, cy0, cx1, cy1] = self.clip_rect;
        let lx = t
            .iter()
            .map(|p| p[0])
            .fold(f64::INFINITY, f64::min)
            .max(cx0)
            .floor() as usize;
        let hx = (t
            .iter()
            .map(|p| p[0])
            .fold(f64::NEG_INFINITY, f64::max)
            .min(cx1)
            .ceil() as usize)
            .min(self.width);
        let ly = t
            .iter()
            .map(|p| p[1])
            .fold(f64::INFINITY, f64::min)
            .max(cy0)
            .floor() as usize;
        let hy = (t
            .iter()
            .map(|p| p[1])
            .fold(f64::NEG_INFINITY, f64::max)
            .min(cy1)
            .ceil() as usize)
            .min(self.height);
        for py in ly..hy {
            for px in lx..hx {
                let (mut hits, mut sum) = (0u32, [0.0f64; 3]);
                for (sx, sy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                    let (x, y) = (px as f64 + sx, py as f64 + sy);
                    let w1 = ((x - t[0][0]) * (t[2][1] - t[0][1])
                        - (t[2][0] - t[0][0]) * (y - t[0][1]))
                        / area;
                    let w2 = ((t[1][0] - t[0][0]) * (y - t[0][1])
                        - (x - t[0][0]) * (t[1][1] - t[0][1]))
                        / area;
                    let w0 = 1.0 - w1 - w2;
                    if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                        hits += 1;
                        for (k, slot) in sum.iter_mut().enumerate() {
                            *slot += w0 * colors[0][k] + w1 * colors[1][k] + w2 * colors[2][k];
                        }
                    }
                }
                if hits > 0 {
                    let n = hits as f64;
                    let ink = [
                        (sum[0] / n * 255.0) as f32,
                        (sum[1] / n * 255.0) as f32,
                        (sum[2] / n * 255.0) as f32,
                    ];
                    self.blend(px, py, ink, n as f32 / 4.0 * alpha as f32);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: Rgb = [1.0, 1.0, 1.0];
    const BLACK: Rgb = [0.0, 0.0, 0.0];

    fn at(r: &Raster, x: usize, y: usize) -> u8 {
        r.rgb[(y * r.width + x) * 3]
    }

    #[test]
    fn rect_coverage_is_exact() {
        let mut r = Raster::new(4, 4, 1.0, WHITE);
        r.fill_rect(1.0, 1.0, 1.5, 1.0, BLACK, 1.0);
        assert_eq!(
            (at(&r, 1, 1), at(&r, 2, 1), at(&r, 3, 1), at(&r, 1, 0)),
            (0, 128, 255, 255)
        );
        // Half alpha over half coverage.
        let mut r = Raster::new(2, 1, 1.0, WHITE);
        r.fill_rect(0.0, 0.0, 0.5, 1.0, BLACK, 0.5);
        assert_eq!(at(&r, 0, 0), 191);
    }

    #[test]
    fn polygon_coverage_matches_area() {
        // A triangle covering half of one pixel.
        let mut r = Raster::new(1, 1, 1.0, WHITE);
        let mut p = Path::new();
        p.polygon(&[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
        r.fill(&p, false, BLACK, 1.0);
        assert!((at(&r, 0, 0) as i32 - 128).abs() <= 2, "{}", at(&r, 0, 0));
    }

    #[test]
    fn fill_rules_differ_on_a_nested_square() {
        let nested = |p: &mut Path| {
            p.polygon(&[[0.0, 0.0], [6.0, 0.0], [6.0, 6.0], [0.0, 6.0]]);
            p.polygon(&[[2.0, 2.0], [4.0, 2.0], [4.0, 4.0], [2.0, 4.0]]);
        };
        let mut p = Path::new();
        nested(&mut p);
        let mut r = Raster::new(6, 6, 1.0, WHITE);
        r.fill(&p, true, BLACK, 1.0);
        assert_eq!((at(&r, 3, 3), at(&r, 1, 1)), (255, 0));
        let mut r = Raster::new(6, 6, 1.0, WHITE);
        r.fill(&p, false, BLACK, 1.0);
        assert_eq!((at(&r, 3, 3), at(&r, 1, 1)), (0, 0));
    }

    #[test]
    fn stroke_width_and_dash() {
        // A 2 px wide horizontal line centred on y = 5 covers rows 4 and 5.
        let mut p = Path::new();
        p.move_to(0.0, 5.0);
        p.line_to(10.0, 5.0);
        let mut r = Raster::new(10, 10, 1.0, WHITE);
        r.stroke(&p, 2.0, &[], BLACK, 1.0);
        assert_eq!(
            (at(&r, 5, 3), at(&r, 5, 4), at(&r, 5, 5), at(&r, 5, 6)),
            (255, 0, 0, 255)
        );
        // Dashes [2 2]: on, off, on.
        let mut r = Raster::new(10, 10, 1.0, WHITE);
        r.stroke(&p, 2.0, &[2.0, 2.0], BLACK, 1.0);
        assert_eq!((at(&r, 1, 4), at(&r, 3, 4), at(&r, 5, 4)), (0, 255, 0));
        // A device-pixel ratio scales the width.
        let mut r = Raster::new(20, 20, 2.0, WHITE);
        r.stroke(&p, 2.0, &[], BLACK, 1.0);
        assert_eq!(
            (at(&r, 10, 7), at(&r, 10, 8), at(&r, 10, 11), at(&r, 10, 12)),
            (255, 0, 0, 255)
        );
    }

    #[test]
    fn a_miter_fills_the_outer_corner() {
        let mut p = Path::new();
        p.move_to(1.0, 6.0);
        p.line_to(5.0, 6.0);
        p.line_to(5.0, 1.0);
        let mut r = Raster::new(8, 8, 1.0, WHITE);
        r.stroke(&p, 2.0, &[], BLACK, 1.0);
        assert_eq!(at(&r, 5, 6), 0, "outer corner pixel");
    }

    #[test]
    fn clip_limits_ink_with_coverage() {
        let mut r = Raster::new(4, 1, 1.0, WHITE);
        r.clip(Some([0.5, 0.0, 2.0, 1.0]));
        let mut p = Path::new();
        p.rect(0.0, 0.0, 4.0, 1.0);
        r.fill(&p, false, BLACK, 1.0);
        assert_eq!(
            (at(&r, 0, 0), at(&r, 1, 0), at(&r, 2, 0), at(&r, 3, 0)),
            (128, 0, 128, 255)
        );
    }

    #[test]
    fn image_is_nearest_neighbour_and_mirrors() {
        let b = Bitmap {
            width: 2,
            height: 1,
            rgb: vec![0, 0, 0, 200, 200, 200],
        };
        let mut r = Raster::new(4, 1, 1.0, WHITE);
        r.image(&b, 0.0, 0.0, 4.0, 1.0);
        assert_eq!(
            (at(&r, 0, 0), at(&r, 1, 0), at(&r, 2, 0), at(&r, 3, 0)),
            (0, 0, 200, 200)
        );
        let mut r = Raster::new(4, 1, 1.0, WHITE);
        r.image(&b, 4.0, 0.0, 0.0, 1.0);
        assert_eq!((at(&r, 0, 0), at(&r, 3, 0)), (200, 0));
    }
}
