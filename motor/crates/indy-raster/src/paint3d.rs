//! Port of the UI's 3-D path: `figure_camera_utils`, `figure_geometry_utils`,
//! the opaque pass of `figure_webgl.js` and the overlay of `figure3d.js`.
//!
//! The WebGL scene becomes a colour-and-depth buffer per axes with the 4-sample
//! rotated grid of the UI's multisampled canvas, resolved and placed on the
//! device as one image. Transparent triangles
//! are drawn last, far to near (the UI peels depth; the result only differs
//! where transparent surfaces intersect).

use crate::color::{AxesColors, adjust, clamp01, contour_display_colors};
use crate::device::{Bitmap, Rgb};
use crate::paint2d::{
    Affine, Box2, Figure, PatchPaint, TextStyle, alpha_value, draw_runs, fit_figure_axes,
    image_bitmap, image_bounds, label, measure_runs,
};
use crate::rec::{Desc, dash, desc, flag, num, nums, rgb, series, strings, text};
use crate::tex;
use indy_graphics::json::Json;

const KEYS: [&str; 3] = ["x", "y", "z"];
const MAX_SCENE_PIXELS: f64 = 16e6;
/// The standard 4x sample positions inside a pixel.
const SAMPLES: [(f64, f64); 4] = [
    (0.375, 0.125),
    (0.875, 0.375),
    (0.125, 0.625),
    (0.625, 0.875),
];

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn lim(axes: &Json, i: usize) -> [f64; 2] {
    let v = nums(axes, &format!("{}lim", KEYS[i]));
    if v.len() == 2 {
        [v[0], v[1]]
    } else {
        [-1.0, 1.0]
    }
}

/// Camera and data-to-box mapping of one axes.
pub struct Camera {
    extent: [f64; 3],
    lim: [[f64; 2]; 3],
    rev: [bool; 3],
    log: [bool; 3],
    right: [f64; 3],
    up: [f64; 3],
    back: [f64; 3],
    target: [f64; 3],
    half_height: f64,
    distance: f64,
    far: f64,
}

/// `extents`.
fn extents(axes: &Json, lims: &[[f64; 2]; 3], log: &[bool; 3]) -> [f64; 3] {
    let pbar = nums(axes, "plot_box_aspect_ratio");
    if text(axes, "plot_box_aspect_ratio_mode") == "manual"
        && text(axes, "data_aspect_ratio_mode") != "manual"
        && pbar.len() == 3
    {
        let largest = pbar[0].max(pbar[1]).max(pbar[2]);
        return [pbar[0] / largest, pbar[1] / largest, pbar[2] / largest];
    }
    let ratios = nums(axes, "data_aspect_ratio");
    let ratios = if ratios.len() == 3 {
        ratios
    } else {
        vec![1.0, 1.0, 1.0]
    };
    let mut logs = [0.0; 3];
    for i in 0..3 {
        let [lo, hi] = lims[i];
        let span = if log[i] && lo > 0.0 {
            hi.log10() - lo.log10()
        } else {
            hi - lo
        };
        let log_span = if span.is_finite() {
            span.ln()
        } else {
            (hi / 2.0 - lo / 2.0).ln() + std::f64::consts::LN_2
        };
        logs[i] = log_span - ratios[i].ln();
    }
    let largest = logs[0].max(logs[1]).max(logs[2]);
    [
        (logs[0] - largest).exp(),
        (logs[1] - largest).exp(),
        (logs[2] - largest).exp(),
    ]
}

impl Camera {
    pub fn of(axes: &Json) -> Camera {
        let lims = [lim(axes, 0), lim(axes, 1), lim(axes, 2)];
        let rev = [
            text(axes, "xdir") == "reverse",
            text(axes, "ydir") == "reverse",
            text(axes, "zdir") == "reverse",
        ];
        let log = [
            text(axes, "xscale") == "log",
            text(axes, "yscale") == "log",
            text(axes, "zscale") == "log",
        ];
        let extent = extents(axes, &lims, &log);
        let radius = (extent[0] * extent[0] + extent[1] * extent[1] + extent[2] * extent[2]).sqrt();
        let view = nums(axes, "view");
        let (az, el) = if view.len() == 2 {
            (view[0].to_radians(), view[1].to_radians())
        } else {
            (0.0, std::f64::consts::FRAC_PI_2)
        };
        let mut cam = Camera {
            extent,
            lim: lims,
            rev,
            log,
            right: [az.cos(), az.sin(), 0.0],
            up: [-az.sin() * el.sin(), az.cos() * el.sin(), el.cos()],
            back: [az.sin() * el.cos(), -az.cos() * el.cos(), el.sin()],
            target: [0.0; 3],
            half_height: radius * 1.05,
            distance: radius * 4.0,
            far: radius * 8.0,
        };
        let target = axes
            .get("camera")
            .map(|c| nums(c, "target"))
            .unwrap_or_default();
        cam.target = if target.len() == 3 && !cam.log.iter().any(|l| *l) {
            cam.normalised([target[0], target[1], target[2]])
        } else {
            [0.0; 3]
        };
        cam
    }

    /// `dataToNormalised`.
    pub fn normalised(&self, p: [f64; 3]) -> [f64; 3] {
        let mut out = [0.0; 3];
        for i in 0..3 {
            let [lo, hi] = self.lim[i];
            let f = if self.log[i] && lo > 0.0 && p[i] > 0.0 {
                (p[i].log10() - lo.log10()) / (hi.log10() - lo.log10())
            } else if self.log[i] {
                f64::NAN
            } else {
                let (span, offset) = (hi - lo, p[i] - lo);
                if span.is_finite() && offset.is_finite() {
                    offset / span
                } else {
                    (p[i] / 2.0 - lo / 2.0) / (hi / 2.0 - lo / 2.0)
                }
            };
            out[i] = (f * 2.0 - 1.0) * self.extent[i] * if self.rev[i] { -1.0 } else { 1.0 };
        }
        out
    }

    fn plane(&self, aspect: f64) -> (f64, f64) {
        let half_y = self.half_height / aspect.min(1.0);
        (half_y * aspect, half_y)
    }

    /// `project` into a viewport: top-left pixels, window depth.
    pub fn project(&self, vp: Box2, p: [f64; 3]) -> [f64; 3] {
        let (hx, hy) = self.plane(vp.w / vp.h);
        let r = [
            p[0] - self.target[0],
            p[1] - self.target[1],
            p[2] - self.target[2],
        ];
        [
            vp.x + (dot(r, self.right) / hx + 1.0) * vp.w / 2.0,
            vp.y + (1.0 - dot(r, self.up) / hy) * vp.h / 2.0,
            (self.distance - dot(r, self.back)) / self.far,
        ]
    }

    /// Normalised position of a viewport-local pixel at a window depth.
    fn world(&self, vp: Box2, x: f64, y: f64, depth: f64) -> [f64; 3] {
        let (hx, hy) = self.plane(vp.w / vp.h);
        let (ox, oy) = ((x * 2.0 / vp.w - 1.0) * hx, (1.0 - y * 2.0 / vp.h) * hy);
        let b = self.distance - depth * self.far;
        std::array::from_fn(|i| {
            self.target[i] + self.right[i] * ox + self.up[i] * oy + self.back[i] * b
        })
    }
}

struct AxisChoice {
    start: [f64; 3],
    end: [f64; 3],
    /// (normalised position, label).
    ticks: Vec<([f64; 3], String)>,
    label: String,
    label_anchor: [f64; 3],
}

struct BoxEdge {
    start: [f64; 3],
    end: [f64; 3],
    behind: bool,
    chosen: bool,
}

struct AxisBox {
    /// (axis, sign, away).
    faces: Vec<(usize, f64, bool)>,
    edges: Vec<BoxEdge>,
    axes: Vec<AxisChoice>,
}

/// `axisBox`.
fn axis_box(cam: &Camera, axes: &Json) -> AxisBox {
    let extent = cam.extent;
    let mut faces = Vec::with_capacity(6);
    for i in 0..3 {
        for sign in [-1.0, 1.0] {
            let mut n = [0.0; 3];
            n[i] = sign;
            faces.push((i, sign, dot(n, cam.back) <= 0.0));
        }
    }
    let mut edges = Vec::with_capacity(12);
    let mut chosen_axes = Vec::with_capacity(3);
    for i in 0..3 {
        let other: Vec<usize> = (0..3).filter(|j| *j != i).collect();
        let mut choices: Vec<(usize, [f64; 3], [f64; 3], usize)> = Vec::with_capacity(4);
        for s0 in [-1.0, 1.0] {
            for s1 in [-1.0, 1.0] {
                let mut start = [0.0; 3];
                start[i] = -extent[i];
                start[other[0]] = s0 * extent[other[0]];
                start[other[1]] = s1 * extent[other[1]];
                let mut end = start;
                end[i] = extent[i];
                let a = other[0] * 2 + usize::from(s0 > 0.0);
                let b = other[1] * 2 + usize::from(s1 > 0.0);
                let away = usize::from(faces[a].2) + usize::from(faces[b].2);
                edges.push(BoxEdge {
                    start,
                    end,
                    behind: away > 0,
                    chosen: false,
                });
                choices.push((edges.len() - 1, start, end, away));
            }
        }
        // On a silhouette edge: X/Y the lowest on screen, Z the leftmost.
        let mut pool: Vec<_> = choices.iter().filter(|c| c.3 == 1).cloned().collect();
        if pool.is_empty() {
            pool = choices.clone();
        }
        let order = if i == 2 {
            [cam.right, cam.up]
        } else {
            [cam.up, cam.right]
        };
        let measure = |start: [f64; 3], direction: [f64; 3]| -> i64 {
            let mut s = start;
            s[i] = 0.0;
            (dot(s, direction) * 1e9).round() as i64
        };
        pool.sort_by_key(|c| (measure(c.1, order[0]), measure(c.1, order[1])));
        let (edge_index, start, end, _) = pool[0];
        edges[edge_index].chosen = true;
        let key = KEYS[i];
        let [lo, hi] = lim(axes, i);
        let tick_values = nums(axes, &format!("{key}tick"));
        let tick_labels = strings(axes, &format!("{key}ticklabel"));
        let mut ticks = Vec::new();
        for (index, &value) in tick_values.iter().enumerate() {
            if !(value >= lo && value <= hi) {
                continue;
            }
            let mut position = start;
            let mut probe = [lim(axes, 0)[0], lim(axes, 1)[0], lim(axes, 2)[0]];
            probe[i] = value;
            position[i] = cam.normalised(probe)[i];
            let text = tick_labels
                .get(index)
                .cloned()
                .unwrap_or_else(|| format!("{value}"));
            ticks.push((position, text));
        }
        let mut label_anchor = start;
        for (j, slot) in label_anchor.iter_mut().enumerate() {
            *slot = if j == i { 0.0 } else { *slot * 1.12 };
        }
        chosen_axes.push(AxisChoice {
            start,
            end,
            ticks,
            label: text(axes, &format!("{key}label")).to_string(),
            label_anchor,
        });
    }
    AxisBox {
        faces,
        edges,
        axes: chosen_axes,
    }
}

#[derive(Clone, Copy)]
struct Vert {
    x: f64,
    y: f64,
    d: f64,
    c: [f32; 3],
}

struct TransparentTri {
    v: [Vert; 3],
    alpha: f32,
    clip: f64,
    bias: f32,
}

/// The per-axes colour and depth buffer.
struct Scene<'a> {
    w: usize,
    h: usize,
    color: Vec<[f32; 3]>,
    depth: Vec<f32>,
    /// Device pixels per local logical pixel.
    sx: f64,
    sy: f64,
    cam: &'a Camera,
    local: Box2,
    extent: [f64; 3],
    transparent: Vec<TransparentTri>,
}

impl Scene<'_> {
    fn inside(&self, p: [f64; 3], slack: f64) -> bool {
        (0..3).all(|i| p[i] >= -self.extent[i] - slack && p[i] <= self.extent[i] + slack)
    }

    /// One triangle in local logical pixels. `clip` is the box slack (`None`
    /// draws unclipped); `test` is the depth test, `write` the depth write.
    fn tri(
        &mut self,
        v: [Vert; 3],
        clip: Option<f64>,
        test: bool,
        write: bool,
        bias: f32,
        alpha: f32,
    ) {
        let (ax, ay) = (v[0].x * self.sx, v[0].y * self.sy);
        let (bx, by) = (v[1].x * self.sx, v[1].y * self.sy);
        let (cx, cy) = (v[2].x * self.sx, v[2].y * self.sy);
        let area = (bx - ax) * (cy - ay) - (cx - ax) * (by - ay);
        if area == 0.0 || !area.is_finite() {
            return;
        }
        let x0 = ax.min(bx).min(cx).floor().max(0.0) as usize;
        let x1 = (ax.max(bx).max(cx).ceil().max(0.0) as usize).min(self.w);
        let y0 = ay.min(by).min(cy).floor().max(0.0) as usize;
        let y1 = (ay.max(by).max(cy).ceil().max(0.0) as usize).min(self.h);
        // The box only needs per-sample tests when a corner may leave it.
        let check = clip.filter(|slack| {
            v.iter()
                .any(|q| !self.inside(self.cam.world(self.local, q.x, q.y, q.d), *slack))
        });
        let same = v[0].c == v[1].c && v[1].c == v[2].c;
        let inv = 1.0 / area;
        for (py, px, s) in pixels(x0, x1, y0, y1) {
            {
                let (sx, sy) = (px as f64 + SAMPLES[s].0, py as f64 + SAMPLES[s].1);
                let w1 = ((sx - ax) * (cy - ay) - (cx - ax) * (sy - ay)) * inv;
                let w2 = ((bx - ax) * (sy - ay) - (sx - ax) * (by - ay)) * inv;
                let w0 = 1.0 - w1 - w2;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let d = w0 * v[0].d + w1 * v[1].d + w2 * v[2].d;
                let i = (py * self.w + px) * 4 + s;
                let dd = d as f32 + bias;
                if test && dd > self.depth[i] {
                    continue;
                }
                if let Some(slack) = check {
                    let w = self.cam.world(self.local, sx / self.sx, sy / self.sy, d);
                    if !self.inside(w, slack) {
                        continue;
                    }
                }
                let c = if same {
                    v[0].c
                } else {
                    let (a, b, e) = (w0 as f32, w1 as f32, w2 as f32);
                    [
                        a * v[0].c[0] + b * v[1].c[0] + e * v[2].c[0],
                        a * v[0].c[1] + b * v[1].c[1] + e * v[2].c[1],
                        a * v[0].c[2] + b * v[1].c[2] + e * v[2].c[2],
                    ]
                };
                if alpha >= 1.0 {
                    self.color[i] = c;
                } else {
                    let o = self.color[i];
                    self.color[i] = [
                        o[0] + (c[0] - o[0]) * alpha,
                        o[1] + (c[1] - o[1]) * alpha,
                        o[2] + (c[2] - o[2]) * alpha,
                    ];
                }
                if write {
                    self.depth[i] = dd;
                }
            }
        }
    }

    /// glPolygonOffset(1, 1) of a mesh triangle, in window depth.
    fn offset(&self, v: &[Vert; 3]) -> f32 {
        let (dx1, dy1) = (v[1].x - v[0].x, v[1].y - v[0].y);
        let (dx2, dy2) = (v[2].x - v[0].x, v[2].y - v[0].y);
        let area = dx1 * dy2 - dx2 * dy1;
        if area == 0.0 {
            return 1e-6;
        }
        let (dz1, dz2) = (v[1].d - v[0].d, v[2].d - v[0].d);
        let gx = (dz1 * dy2 - dz2 * dy1) / area / self.sx;
        let gy = (dz2 * dx1 - dz1 * dx2) / area / self.sy;
        (gx.abs().max(gy.abs()) + 1e-6) as f32
    }

    fn mesh_tri(&mut self, v: [Vert; 3], alpha: f32) {
        let bias = self.offset(&v);
        if alpha < 1.0 {
            self.transparent.push(TransparentTri {
                v,
                alpha,
                clip: 1e-5,
                bias,
            });
        } else {
            self.tri(v, Some(1e-5), true, true, bias, 1.0);
        }
    }

    /// A butt-ended segment quad in local pixels.
    fn segment(
        &mut self,
        a: Vert,
        b: Vert,
        width: f64,
        clip: Option<f64>,
        test: bool,
        write: bool,
        alpha: f32,
    ) {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len = (dx * dx + dy * dy).sqrt();
        if !(len > 0.0) || !len.is_finite() {
            return;
        }
        let (nx, ny) = (-dy / len * width / 2.0, dx / len * width / 2.0);
        let pa = Vert {
            x: a.x + nx,
            y: a.y + ny,
            ..a
        };
        let qa = Vert {
            x: a.x - nx,
            y: a.y - ny,
            ..a
        };
        let pb = Vert {
            x: b.x + nx,
            y: b.y + ny,
            ..b
        };
        let qb = Vert {
            x: b.x - nx,
            y: b.y - ny,
            ..b
        };
        if alpha < 1.0 {
            let slack = clip.unwrap_or(f64::INFINITY);
            self.transparent.push(TransparentTri {
                v: [pa, qa, pb],
                alpha,
                clip: slack,
                bias: 0.0,
            });
            self.transparent.push(TransparentTri {
                v: [pb, qa, qb],
                alpha,
                clip: slack,
                bias: 0.0,
            });
        } else {
            self.tri([pa, qa, pb], clip, test, write, 0.0, 1.0);
            self.tri([pb, qa, qb], clip, test, write, 0.0, 1.0);
        }
    }

    /// `appendScreenPolyline` for one run; the dash phase is continuous.
    fn polyline(
        &mut self,
        pts: &[Vert],
        width: f64,
        style: &str,
        clip: Option<f64>,
        test: bool,
        write: bool,
        alpha: f32,
    ) {
        if style == "none" || !(width > 0.0) {
            return;
        }
        let pattern = dash(style);
        if pattern.is_empty() {
            for k in 1..pts.len() {
                self.segment(pts[k - 1], pts[k], width, clip, test, write, alpha);
            }
            return;
        }
        let cycle: f64 = pattern.iter().sum();
        let mut phase = 0.0f64;
        for k in 1..pts.len() {
            let (a, b) = (pts[k - 1], pts[k]);
            let len = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
            if !(len > 0.0) || !len.is_finite() {
                continue;
            }
            let mix = |t: f64| Vert {
                x: a.x + (b.x - a.x) * t,
                y: a.y + (b.y - a.y) * t,
                d: a.d + (b.d - a.d) * t,
                c: [
                    a.c[0] + (b.c[0] - a.c[0]) * t as f32,
                    a.c[1] + (b.c[1] - a.c[1]) * t as f32,
                    a.c[2] + (b.c[2] - a.c[2]) * t as f32,
                ],
            };
            let mut offset = 0.0;
            while offset < len {
                let mut remaining = (phase + offset) % cycle;
                let mut part = 0;
                while remaining >= pattern[part] {
                    remaining -= pattern[part];
                    part += 1;
                }
                let step = (len - offset).min(pattern[part] - remaining);
                if part.is_multiple_of(2) {
                    self.segment(
                        mix(offset / len),
                        mix((offset + step) / len),
                        width,
                        clip,
                        test,
                        write,
                        alpha,
                    );
                }
                if offset + step == offset {
                    break;
                }
                offset += step;
            }
            phase = (phase + len) % cycle;
        }
    }

    /// A simple or non-convex polygon: parity of a triangle fan, then the
    /// covered samples at the fan's interpolated depth and colour.
    fn polygon(&mut self, pts: &[Vert], alpha: f32) {
        if pts.len() < 3 {
            return;
        }
        if pts.len() == 3 {
            self.mesh_tri([pts[0], pts[1], pts[2]], alpha);
            return;
        }
        if pts.len() == 4 && alpha >= 1.0 {
            // The common convex quad, without the parity pass.
            let cross = |a: &Vert, b: &Vert, c: &Vert| {
                (b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y)
            };
            let s: Vec<f64> = (0..4)
                .map(|i| cross(&pts[i], &pts[(i + 1) % 4], &pts[(i + 2) % 4]))
                .collect();
            if s.iter().all(|v| *v >= 0.0) || s.iter().all(|v| *v <= 0.0) {
                self.mesh_tri([pts[0], pts[1], pts[2]], 1.0);
                self.mesh_tri([pts[0], pts[2], pts[3]], 1.0);
                return;
            }
        }
        let bx = |f: fn(f64, f64) -> f64, get: fn(&Vert) -> f64| {
            pts.iter().map(get).fold(get(&pts[0]), f)
        };
        let x0 = (bx(f64::min, |p| p.x) * self.sx).floor().max(0.0) as usize;
        let x1 = ((bx(f64::max, |p| p.x) * self.sx).ceil().max(0.0) as usize).min(self.w);
        let y0 = (bx(f64::min, |p| p.y) * self.sy).floor().max(0.0) as usize;
        let y1 = ((bx(f64::max, |p| p.y) * self.sy).ceil().max(0.0) as usize).min(self.h);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        // A plane through the polygon for depth and colour.
        let (a, mut b, mut c) = (pts[0], pts[1], pts[2]);
        let mut best = 0.0;
        for i in 1..pts.len().min(65) {
            for j in i + 1..pts.len() {
                let area = ((pts[i].x - a.x) * (pts[j].y - a.y)
                    - (pts[j].x - a.x) * (pts[i].y - a.y))
                    .abs();
                if area > best {
                    best = area;
                    b = pts[i];
                    c = pts[j];
                }
            }
        }
        let area = (b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y);
        let same = pts.iter().all(|p| p.c == pts[0].c);
        let bias = if area == 0.0 {
            1e-6
        } else {
            self.offset(&[a, b, c])
        };
        // Even-odd crossings per sample row.
        let mut crossings: Vec<f64> = Vec::new();
        for py in y0..y1 {
            for (s, offset) in SAMPLES.iter().enumerate() {
                let sy = (py as f64 + offset.1) / self.sy;
                crossings.clear();
                for k in 0..pts.len() {
                    let (p, q) = (&pts[k], &pts[(k + 1) % pts.len()]);
                    if (p.y > sy) != (q.y > sy) {
                        crossings.push((p.x + (sy - p.y) * (q.x - p.x) / (q.y - p.y)) * self.sx);
                    }
                }
                crossings.sort_unstable_by(f64::total_cmp);
                for pair in crossings.as_chunks::<2>().0 {
                    let s0 = (pair[0] - offset.0).ceil().max(x0 as f64) as usize;
                    let s1 = ((pair[1] - offset.0).floor() + 1.0).max(0.0).min(x1 as f64) as usize;
                    for px in s0..s1.max(s0) {
                        let i = (py * self.w + px) * 4 + s;
                        let (sx, sy) = ((px as f64 + offset.0) / self.sx, sy);
                        let (w1, w2) = if area == 0.0 {
                            (0.0, 0.0)
                        } else {
                            (
                                ((sx - a.x) * (c.y - a.y) - (c.x - a.x) * (sy - a.y)) / area,
                                ((b.x - a.x) * (sy - a.y) - (sx - a.x) * (b.y - a.y)) / area,
                            )
                        };
                        let w0 = 1.0 - w1 - w2;
                        let d = w0 * a.d + w1 * b.d + w2 * c.d;
                        let dd = d as f32 + bias;
                        if dd > self.depth[i] {
                            continue;
                        }
                        let w = self.cam.world(self.local, sx, sy, d);
                        if !self.inside(w, 1e-5) {
                            continue;
                        }
                        let ink = if same {
                            pts[0].c
                        } else {
                            let (x, y, z) = (w0 as f32, w1 as f32, w2 as f32);
                            [
                                x * a.c[0] + y * b.c[0] + z * c.c[0],
                                x * a.c[1] + y * b.c[1] + z * c.c[1],
                                x * a.c[2] + y * b.c[2] + z * c.c[2],
                            ]
                        };
                        if alpha >= 1.0 {
                            self.color[i] = ink;
                            self.depth[i] = dd;
                        } else {
                            let o = self.color[i];
                            self.color[i] = [
                                o[0] + (ink[0] - o[0]) * alpha,
                                o[1] + (ink[1] - o[1]) * alpha,
                                o[2] + (ink[2] - o[2]) * alpha,
                            ];
                        }
                    }
                }
            }
        }
    }

    /// A marker quad, shaded per sample as the UI's fragment shader does.
    fn marker(
        &mut self,
        p: [f64; 3],
        size: f64,
        kind: &str,
        edge: Option<[f32; 3]>,
        face: Option<[f32; 3]>,
    ) {
        let border = (2.0 * 96.0 / 72.0 / size.max(1.0)).min(1.0);
        let half = size / 2.0;
        let (x0, x1) = (
            ((p[0] - half) * self.sx).floor().max(0.0) as usize,
            (((p[0] + half) * self.sx).ceil().max(0.0) as usize).min(self.w),
        );
        let (y0, y1) = (
            ((p[1] - half) * self.sy).floor().max(0.0) as usize,
            (((p[1] + half) * self.sy).ceil().max(0.0) as usize).min(self.h),
        );
        let depth = p[2] as f32;
        for (py, px, s) in pixels(x0, x1, y0, y1) {
            {
                let u = ((px as f64 + SAMPLES[s].0) / self.sx - p[0]) / half;
                let v = ((py as f64 + SAMPLES[s].1) / self.sy - p[1]) / half;
                if u.abs() > 1.0 || v.abs() > 1.0 {
                    continue;
                }
                let i = (py * self.w + px) * 4 + s;
                if depth > self.depth[i] {
                    continue;
                }
                let ink = match kind {
                    "+" | "x" | "*" => {
                        let cross = u.abs().min(v.abs());
                        let diagonal =
                            (u - v).abs().min((u + v).abs()) * std::f64::consts::FRAC_1_SQRT_2;
                        let d = match kind {
                            "+" => cross,
                            "x" => diagonal,
                            _ => cross.min(diagonal),
                        };
                        if d > border {
                            continue;
                        }
                        match edge {
                            Some(c) => c,
                            None => continue,
                        }
                    }
                    _ => {
                        let d = marker_shape(kind, u, v);
                        if d > 1.0 {
                            continue;
                        }
                        match (edge, face) {
                            (Some(c), _) if d > 1.0 - border || kind == "." => c,
                            (_, Some(c)) => c,
                            _ => continue,
                        }
                    }
                };
                self.color[i] = ink;
                self.depth[i] = depth;
            }
        }
    }
}

/// Every sample of a pixel rectangle: (row, column, sample).
fn pixels(
    x0: usize,
    x1: usize,
    y0: usize,
    y1: usize,
) -> impl Iterator<Item = (usize, usize, usize)> {
    (y0..y1).flat_map(move |py| (x0..x1).flat_map(move |px| (0..4).map(move |s| (py, px, s))))
}

/// The shader's `shape`, plus the MATLAB shapes it lacks.
fn marker_shape(kind: &str, u: f64, v: f64) -> f64 {
    let (x, y) = (u, v);
    match kind {
        "s" | "square" => x.abs().max(y.abs()),
        "^" | "v" | ">" | "<" => {
            let (mut px, mut py) = (x, -y);
            match kind {
                "v" => {
                    px = -px;
                    py = -py;
                }
                ">" => {
                    let t = px;
                    px = -py;
                    py = t;
                }
                "<" => {
                    let t = px;
                    px = py;
                    py = -t;
                }
                _ => {}
            }
            (-py)
                .max(0.8660254 * px + 0.5 * py)
                .max(-0.8660254 * px + 0.5 * py)
                * 2.0
        }
        "d" | "diamond" => x.abs() / 0.7 + y.abs(),
        "p" | "pentagram" => star_distance(x, y, 5, 0.382),
        "h" | "hexagram" => star_distance(x, y, 6, 0.577),
        _ => (x * x + y * y).sqrt(),
    }
}

fn star_distance(x: f64, y: f64, tips: usize, inner: f64) -> f64 {
    let r = (x * x + y * y).sqrt();
    if r == 0.0 {
        return 0.0;
    }
    let sector = std::f64::consts::PI / tips as f64;
    let angle = (x.atan2(-y)).rem_euclid(sector * 2.0);
    let t = (angle - sector).abs() / sector;
    // Boundary radius along straight star edges (tip at t=1, notch at t=0).
    let boundary = inner * (1.0 - t) + t;
    r / boundary
}

fn f32c(c: Rgb) -> [f32; 3] {
    [c[0] as f32, c[1] as f32, c[2] as f32]
}

fn surface_point(s: &Json, x: &Desc, y: &Desc, z: &Desc, rows: usize, i: usize) -> [f64; 3] {
    let layout = s.get("coordinate_layout");
    let vector =
        |key: &str| layout.and_then(|l| l.get(key)).and_then(Json::as_str) == Some("vector");
    [
        x.at(if vector("x") { i / rows } else { i }),
        y.at(if vector("y") { i % rows } else { i }),
        z.at(i),
    ]
}

/// Draws the opaque and transparent scene of one 3-D axes into a bitmap.
fn render_scene(fig: &Figure, axes: &Json, cam: &Camera, vp: Box2, dpr: f64) -> Bitmap {
    let theme = fig.theme;
    let width = (vp.w * dpr).round().max(1.0) as usize;
    let height = (vp.h * dpr).round().max(1.0) as usize;
    let (w, h) = (width, height);
    let local = Box2 {
        x: 0.0,
        y: 0.0,
        w: vp.w,
        h: vp.h,
    };
    let mut sc = Scene {
        w,
        h,
        color: vec![f32c(theme.background); w * h * 4],
        depth: vec![1.0; w * h * 4],
        sx: w as f64 / vp.w,
        sy: h as f64 / vp.h,
        cam,
        local,
        extent: cam.extent,
        transparent: Vec::new(),
    };
    let project = |p: [f64; 3]| cam.project(local, p);
    let vert = |p: [f64; 3], c: [f32; 3]| {
        let q = cam.project(local, p);
        Vert {
            x: q[0],
            y: q[1],
            d: q[2],
            c,
        }
    };
    let (hx, _) = cam.plane(vp.w / vp.h);
    // Normalised units per local pixel, for the box slack of strokes.
    let unit = 2.0 * hx / vp.w;
    let slack = |pixels: f64| 0.0001 + unit * pixels / 2.0;
    let colors = AxesColors::of(axes);
    let adj = |c: Rgb| f32c(adjust(c, &theme));
    if axes.get("visible").and_then(Json::as_bool).unwrap_or(true) {
        // Back walls: decoration, unclipped and without depth.
        let bx = axis_box(cam, axes);
        let grid = axes.get("grid").cloned().unwrap_or(Json::obj());
        let (g, ax) = (f32c(theme.grid), f32c(theme.axis));
        for &(fixed, sign, away) in &bx.faces {
            if !away {
                continue;
            }
            for axis in 0..3 {
                if axis == fixed || !flag(&grid, KEYS[axis]) {
                    continue;
                }
                let along = (0..3).find(|i| *i != axis && *i != fixed).unwrap_or(0);
                for (position, _) in &bx.axes[axis].ticks {
                    let mut start = [0.0; 3];
                    start[fixed] = sign * bx_extent(cam, fixed);
                    start[axis] = position[axis];
                    start[along] = -bx_extent(cam, along);
                    let mut end = start;
                    end[along] = bx_extent(cam, along);
                    sc.segment(vert(start, g), vert(end, g), 1.0, None, false, false, 1.0);
                }
            }
        }
        for edge in bx.edges.iter().filter(|e| !e.chosen && e.behind) {
            sc.segment(
                vert(edge.start, g),
                vert(edge.end, g),
                1.0,
                None,
                false,
                false,
                1.0,
            );
        }
        for edge in bx.edges.iter().filter(|e| e.chosen) {
            sc.segment(
                vert(edge.start, ax),
                vert(edge.end, ax),
                1.0,
                None,
                false,
                false,
                1.0,
            );
        }
    }
    let contour_colors = contour_display_colors(axes, &colors, &theme);
    let list = series(axes);
    // Fills in series order; contourf borders with their fill; then other
    // edges, lines and markers.
    for s in list {
        let kind = text(s, "kind");
        match kind {
            "surface" => surface_faces(&mut sc, s, &colors, &vert),
            "patch3d" => {
                let (Some(v), Some(f)) = (desc(s, "vertices"), desc(s, "faces")) else {
                    continue;
                };
                let n = v.shape.first().copied().unwrap_or(0);
                let ring: Vec<usize> = f
                    .values
                    .iter()
                    .filter(|k| k.is_finite())
                    .map(|k| *k as usize)
                    .collect();
                let at = |i: usize| [v.at(i), v.at(i + n), v.at(i + 2 * n)];
                let face = s.get("face_color");
                if let Some(c) = face.and_then(|d| colors.patch(d, 0))
                    && !flag(s, "no_area")
                {
                    let pts: Vec<Vert> = ring
                        .iter()
                        .map(|&i| vert(cam.normalised(at(i)), f32c(clamp01(c))))
                        .collect();
                    sc.polygon(&pts, alpha_value(s, "face_alpha") as f32);
                }
                // A filled contour's border belongs to its fill's slot.
                if let Some(c) = s.get("edge_color").and_then(|d| colors.patch(d, 0))
                    && text(s, "line_style") != "none"
                    && num(s, "line_width").unwrap_or(0.0) > 0.0
                {
                    let faced =
                        face.is_some_and(|d| text(d, "mode") != "none") && !flag(s, "no_area");
                    let ink = if faced { f32c(clamp01(c)) } else { adj(c) };
                    let mut pts: Vec<Vert> = ring
                        .iter()
                        .map(|&i| vert(cam.normalised(at(i)), ink))
                        .collect();
                    if let Some(first) = pts.first().copied() {
                        pts.push(first);
                    }
                    let width = num(s, "line_width").unwrap_or(0.5) * 96.0 / 72.0;
                    sc.polyline(
                        &pts,
                        width,
                        text(s, "line_style"),
                        Some(slack(width)),
                        true,
                        false,
                        alpha_value(s, "edge_alpha") as f32,
                    );
                }
            }
            "patch_raster" => {
                let (Some(v), Some(f)) = (desc(s, "vertices"), desc(s, "faces")) else {
                    continue;
                };
                let n = v.shape.first().copied().unwrap_or(0);
                let dim = v.shape.get(1).copied().unwrap_or(2);
                let rows = f.shape.first().copied().unwrap_or(0);
                let cols = f.shape.get(1).copied().unwrap_or(0);
                let paint = PatchPaint::of(s.get("face"));
                let edge = PatchPaint::of(s.get("edge"));
                let at = |i: usize| {
                    [
                        v.at(i),
                        v.at(i + n),
                        if dim > 2 { v.at(i + 2 * n) } else { 0.0 },
                    ]
                };
                let fa = alpha_value(s, "face_alpha") as f32;
                let ea = alpha_value(s, "edge_alpha") as f32;
                let width = num(s, "line_width").unwrap_or(0.5) * 96.0 / 72.0;
                for r in 0..rows {
                    let idx: Vec<usize> = (0..cols)
                        .map(|c| f.at(r + c * rows))
                        .take_while(|k| k.is_finite())
                        .map(|k| k as usize)
                        .collect();
                    if idx.len() < 2 || idx.iter().any(|&i| at(i).iter().any(|c| !c.is_finite())) {
                        continue;
                    }
                    if !paint.is_none() && idx.len() > 2 && paint.colored(r, &idx) {
                        let solid = paint.face(r, idx[0]);
                        let pts: Vec<Vert> = idx
                            .iter()
                            .map(|&i| {
                                let c = if paint.interp() {
                                    paint.vertex(i)
                                } else {
                                    solid.unwrap_or([0.0, 0.0, 0.0])
                                };
                                vert(cam.normalised(at(i)), f32c(clamp01(c)))
                            })
                            .collect();
                        sc.polygon(&pts, fa);
                    }
                    if !edge.is_none() && text(s, "line_style") != "none" && edge.colored(r, &idx) {
                        let solid = edge.face(r, idx[0]);
                        let mut pts: Vec<Vert> = idx
                            .iter()
                            .map(|&i| {
                                let c = if edge.interp() {
                                    edge.vertex(i)
                                } else {
                                    solid.unwrap_or([0.0, 0.0, 0.0])
                                };
                                vert(
                                    cam.normalised(at(i)),
                                    if paint.is_none() {
                                        adj(c)
                                    } else {
                                        f32c(clamp01(c))
                                    },
                                )
                            })
                            .collect();
                        pts.push(pts[0]);
                        sc.polyline(
                            &pts,
                            width,
                            text(s, "line_style"),
                            Some(slack(width)),
                            true,
                            true,
                            ea,
                        );
                    }
                }
            }
            "image" => {
                // Relaxed export: an image in a 3-D view lies in the plane z = 0.
                let Some(b) = image_bitmap(s, &colors) else {
                    continue;
                };
                let (cx, cy) = (nums(s, "x"), nums(s, "y"));
                if cx.len() != 2 || cy.len() != 2 {
                    continue;
                }
                let (bx, by) = (image_bounds(&cx, b.width), image_bounds(&cy, b.height));
                let (dx, dy) = (
                    (bx[1] - bx[0]) / b.width as f64,
                    (by[1] - by[0]) / b.height as f64,
                );
                for row in 0..b.height {
                    for col in 0..b.width {
                        let o = (row * b.width + col) * 3;
                        let c = [
                            b.rgb[o] as f32 / 255.0,
                            b.rgb[o + 1] as f32 / 255.0,
                            b.rgb[o + 2] as f32 / 255.0,
                        ];
                        let (x0, y0) = (bx[0] + dx * col as f64, by[0] + dy * row as f64);
                        let q = |x: f64, y: f64| vert(cam.normalised([x, y, 0.0]), c);
                        sc.mesh_tri([q(x0, y0), q(x0 + dx, y0), q(x0, y0 + dy)], 1.0);
                        sc.mesh_tri([q(x0 + dx, y0), q(x0 + dx, y0 + dy), q(x0, y0 + dy)], 1.0);
                    }
                }
            }
            _ => {}
        }
    }
    for s in list {
        if text(s, "kind") == "surface" {
            surface_edges(&mut sc, s, &colors, &theme, &vert, &slack);
        }
    }
    for (j, s) in list.iter().enumerate() {
        let kind = text(s, "kind");
        if !matches!(kind, "line" | "scatter" | "stem" | "stairs") {
            continue;
        }
        let (xs, ys, zs) = (nums(s, "x"), nums(s, "y"), nums(s, "z"));
        let point = |i: usize| [xs[i], ys[i], zs.get(i).copied().unwrap_or(0.0)];
        let finite = |p: [f64; 3]| p.iter().all(|v| v.is_finite());
        let line_width = num(s, "line_width").unwrap_or(0.5);
        let style = text(s, "line_style");
        if kind != "scatter"
            && style != "none"
            && line_width > 0.0
            && let Some(raw) = contour_colors
                .get(j)
                .copied()
                .flatten()
                .map(f32c)
                .or_else(|| rgb(s, "line_color").map(adj))
        {
            let width = line_width * 96.0 / 72.0;
            let clip = Some(slack(width));
            let mut run: Vec<Vert> = Vec::new();
            let mut previous: Option<[f64; 3]> = None;
            let base = num(s, "base_value").unwrap_or(0.0);
            for i in 0..xs.len().min(ys.len()) {
                let q = point(i);
                if !finite(q) {
                    sc.polyline(&run, width, style, clip, true, true, 1.0);
                    run.clear();
                    previous = None;
                    continue;
                }
                if kind == "stem" {
                    let stem = [
                        vert(cam.normalised([q[0], base, q[2]]), raw),
                        vert(cam.normalised(q), raw),
                    ];
                    sc.polyline(&stem, width, style, clip, true, true, 1.0);
                    continue;
                }
                if kind == "stairs"
                    && let Some(p) = previous
                {
                    run.push(vert(cam.normalised([q[0], p[1], p[2]]), raw));
                }
                run.push(vert(cam.normalised(q), raw));
                previous = Some(q);
            }
            sc.polyline(&run, width, style, clip, true, true, 1.0);
        }
        let marker = text(s, "marker");
        if marker.is_empty() || marker == "none" {
            continue;
        }
        let auto = flag(s, "marker_face_auto");
        let edge = rgb(s, "marker_edge_color").map(adj);
        let face = if auto {
            Some(adj(theme.background))
        } else {
            rgb(s, "marker_face_color").map(adj)
        };
        if edge.is_none() && face.is_none() {
            continue;
        }
        let sizes = nums(s, "sizes");
        let default_size = num(s, "marker_size").unwrap_or(6.0);
        let point_colors = desc(s, "point_colors");
        let (per_edge, per_face) = (flag(s, "point_edge"), flag(s, "point_face"));
        for i in 0..xs.len().min(ys.len()) {
            let q = point(i);
            if !finite(q) {
                continue;
            }
            let centre = cam.normalised(q);
            if !sc.inside(centre, 1e-5) {
                continue;
            }
            let pts = if kind == "scatter" {
                (if sizes.len() == 1 {
                    sizes[0]
                } else {
                    sizes.get(i).copied().unwrap_or(default_size * default_size)
                })
                .max(0.0)
                .sqrt()
            } else {
                default_size
            };
            let size = pts * 96.0 / 72.0;
            if !(size > 0.0) {
                continue;
            }
            let (mut e, mut f) = (edge, face);
            if let Some(pc) = &point_colors {
                let n = pc.shape.first().copied().unwrap_or(0);
                let c = [pc.at(i), pc.at(i + n), pc.at(i + 2 * n)];
                if c.iter().all(|v| v.is_finite()) {
                    if per_edge {
                        e = Some(adj(c));
                    }
                    if per_face {
                        f = Some(adj(c));
                    }
                }
            }
            sc.marker(project(centre), size, marker, e, f);
        }
    }
    // Transparent triangles, far to near, tested against the opaque depth.
    let mut transparent = std::mem::take(&mut sc.transparent);
    transparent.sort_by(|a, b| {
        let da = a.v[0].d + a.v[1].d + a.v[2].d;
        let db = b.v[0].d + b.v[1].d + b.v[2].d;
        db.total_cmp(&da)
    });
    for t in transparent {
        let clip = t.clip.is_finite().then_some(t.clip);
        sc.tri(t.v, clip, true, false, t.bias, t.alpha);
    }
    // Resolve the samples.
    let mut rgb_out = vec![0u8; width * height * 3];
    for (i, samples) in sc.color.as_chunks::<4>().0.iter().enumerate() {
        for k in 0..3 {
            let mean = (samples[0][k] + samples[1][k] + samples[2][k] + samples[3][k]) * 0.25;
            rgb_out[i * 3 + k] = (mean * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
        }
    }
    Bitmap {
        width,
        height,
        rgb: rgb_out,
    }
}

fn bx_extent(cam: &Camera, i: usize) -> f64 {
    cam.extent[i]
}

fn surface_faces(
    sc: &mut Scene,
    s: &Json,
    colors: &AxesColors,
    vert: &dyn Fn([f64; 3], [f32; 3]) -> Vert,
) {
    let Some(face) = s.get("face_color") else {
        return;
    };
    let mode = text(face, "mode");
    if mode == "none" {
        return;
    }
    let (Some(x), Some(y), Some(z)) = (desc(s, "x"), desc(s, "y"), desc(s, "z")) else {
        return;
    };
    let shape = nums(s, "shape");
    if shape.len() != 2 {
        return;
    }
    let rows = shape[0] as usize;
    let alpha = alpha_value(s, "face_alpha") as f32;
    let interp = mode == "interp";
    let face = colors.resolve(face);
    let origins = nums(s, "cell_origins");
    let cam = sc.cam;
    let total = rows * shape[1] as usize;
    let mut verts: Vec<Option<[f64; 3]>> = vec![None; total];
    let mut vertex_colors: Vec<Option<[f32; 3]>> = vec![None; if interp { total } else { 0 }];
    let at = |i: usize, verts: &mut Vec<Option<[f64; 3]>>| -> [f64; 3] {
        if let Some(p) = verts[i] {
            return p;
        }
        let p = cam.normalised(surface_point(s, &x, &y, &z, rows, i));
        verts[i] = Some(p);
        p
    };
    for &origin in &origins {
        let i = origin as usize;
        if i + rows + 1 >= total {
            continue;
        }
        let corners = [i, i + 1, i + rows, i + rows + 1];
        let flat = if interp { None } else { face.at(i) };
        if !interp && flat.is_none() {
            continue;
        }
        let mut v = [Vert {
            x: 0.0,
            y: 0.0,
            d: 0.0,
            c: [0.0; 3],
        }; 4];
        let mut ok = true;
        for (k, &corner) in corners.iter().enumerate() {
            let p = at(corner, &mut verts);
            let c = if interp {
                match vertex_colors[corner] {
                    Some(c) => c,
                    None => match face.at(corner) {
                        Some(c) => {
                            let c = f32c(c);
                            vertex_colors[corner] = Some(c);
                            c
                        }
                        None => {
                            ok = false;
                            break;
                        }
                    },
                }
            } else {
                f32c(flat.unwrap_or([0.0; 3]))
            };
            v[k] = vert(p, c);
        }
        if !ok
            || v.iter()
                .any(|q| !q.x.is_finite() || !q.y.is_finite() || !q.d.is_finite())
        {
            continue;
        }
        sc.mesh_tri([v[0], v[2], v[1]], alpha);
        sc.mesh_tri([v[1], v[2], v[3]], alpha);
    }
}

fn surface_edges(
    sc: &mut Scene,
    s: &Json,
    colors: &AxesColors,
    theme: &crate::color::Theme,
    vert: &dyn Fn([f64; 3], [f32; 3]) -> Vert,
    slack: &dyn Fn(f64) -> f64,
) {
    let Some(edge) = s.get("edge_color") else {
        return;
    };
    let mode = text(edge, "mode");
    let style = text(s, "line_style");
    let line_width = num(s, "line_width").unwrap_or(0.5);
    if mode == "none" || style == "none" || !(line_width > 0.0) {
        return;
    }
    let (Some(x), Some(y), Some(z), Some(idx)) = (
        desc(s, "x"),
        desc(s, "y"),
        desc(s, "z"),
        desc(s, "edge_indices"),
    ) else {
        return;
    };
    let shape = nums(s, "shape");
    if shape.len() != 2 {
        return;
    }
    let rows = shape[0] as usize;
    let count = idx.shape.first().copied().unwrap_or(0);
    let faced = s
        .get("face_color")
        .is_some_and(|f| text(f, "mode") != "none");
    let width = line_width * 96.0 / 72.0;
    let clip = Some(slack(width));
    let alpha = alpha_value(s, "edge_alpha") as f32;
    let interp = mode == "interp";
    let edge = colors.resolve(edge);
    let cam = sc.cam;
    let ink = |c: Rgb| {
        if faced {
            f32c(clamp01(c))
        } else {
            f32c(adjust(c, theme))
        }
    };
    for e in 0..count {
        let (a, b) = (idx.at(e) as usize, idx.at(e + count) as usize);
        let ca = edge.at(a);
        let cb = if interp { edge.at(b) } else { ca };
        let (Some(ca), Some(cb)) = (ca, cb) else {
            continue;
        };
        let pa = cam.normalised(surface_point(s, &x, &y, &z, rows, a));
        let pb = cam.normalised(surface_point(s, &x, &y, &z, rows, b));
        let pts = [vert(pa, ink(ca)), vert(pb, ink(cb))];
        sc.polyline(&pts, width, style, clip, true, true, alpha);
    }
}

fn mono_baseline_middle(st: TextStyle) -> f64 {
    let (asc, desc) = crate::font::metrics(st.face(false));
    (asc - desc) / 2.0 * st.size
}

impl Figure<'_> {
    /// Every 3-D scene first, as one WebGL canvas under the overlay.
    pub fn draw_scenes(&mut self) {
        for i in 0..self.axes.len() {
            let a = &self.axes[i];
            if num(a, "dimension") != Some(3.0) {
                continue;
            }
            let vp = fit_figure_axes(a, self.viewport, self.axes);
            if !(vp.w > 0.0 && vp.h > 0.0) {
                continue;
            }
            let cam = Camera::of(a);
            let dpr = self.dev.dpr();
            // A poster-sized export keeps the scene's buffers within bounds;
            // the image is then enlarged onto its box.
            let scene_dpr = dpr.min((MAX_SCENE_PIXELS / (vp.w * vp.h)).sqrt());
            let bitmap = render_scene(self, a, &cam, vp, scene_dpr);
            // gl.viewport rounds from the bottom edge of the canvas.
            let canvas = (self.height * dpr).round();
            let bottom = ((self.height - vp.y - vp.h) * dpr).round();
            let x0 = (vp.x * dpr).round() / dpr;
            let y0 = (canvas - bottom - (vp.h * dpr).round()) / dpr;
            let (x1, y1) = (
                x0 + (vp.w * dpr).round() / dpr,
                y0 + (vp.h * dpr).round() / dpr,
            );
            self.dev.image(&bitmap, x0, y0, x1, y1);
        }
    }

    /// `drawAxes`: tick marks, text and the legend over the scene.
    pub fn draw_axes_overlay(&mut self, index: usize) {
        let a = &self.axes[index];
        let vp = fit_figure_axes(a, self.viewport, self.axes);
        if !(vp.w > 0.0 && vp.h > 0.0) {
            return;
        }
        let cam = Camera::of(a);
        let bx = axis_box(&cam, a);
        let project = |p: [f64; 3]| cam.project(vp, p);
        let theme = self.theme;
        let center = project([0.0; 3]);
        let visible = a.get("visible").and_then(Json::as_bool).unwrap_or(true);
        let tick_style = TextStyle::mono(10.0);
        let middle = mono_baseline_middle(tick_style);
        let colors = AxesColors::of(a);
        for (key, axis) in bx.axes.iter().enumerate() {
            let from = project(axis.start);
            let to = project(axis.end);
            if ((to[0] - from[0]).powi(2) + (to[1] - from[1]).powi(2)).sqrt() < 24.0 {
                continue;
            }
            let anchor = project(axis.label_anchor);
            let (dx, dy) = (anchor[0] - center[0], anchor[1] - center[1]);
            let distance = (dx * dx + dy * dy).sqrt();
            let distance = if distance > 0.0 { distance } else { 1.0 };
            let (ox, oy) = (dx / distance, dy / distance);
            let align = if ox > 0.35 {
                "left"
            } else if ox < -0.35 {
                "right"
            } else {
                "center"
            };
            let mut widest = 0.0f64;
            let mut last: Option<(f64, f64, f64)> = None;
            for (position, s) in &axis.ticks {
                let runs = tex::parse(s, "tex");
                let width = measure_runs(&runs, tick_style);
                if !visible {
                    widest = widest.max(width);
                    continue;
                }
                let p = project(*position);
                let mut mark = crate::device::Path::new();
                mark.move_to(p[0], p[1]);
                mark.line_to(p[0] + ox * 4.0, p[1] + oy * 4.0);
                self.dev.stroke(&mark, 1.0, &[], theme.axis, 1.0);
                let x = p[0] + ox * 8.0;
                let y = p[1] + oy * 12.0;
                let left = match align {
                    "left" => x,
                    "right" => x - width,
                    _ => x - width / 2.0,
                };
                if last.is_some_and(|(l, r, ly)| {
                    left < r + 3.0 && left + width > l - 3.0 && (y - ly).abs() < 11.0
                }) {
                    continue;
                }
                draw_runs(
                    self.dev,
                    &runs,
                    left,
                    y + middle,
                    tick_style,
                    theme.text,
                    Affine::IDENTITY,
                );
                last = Some((left, left + width, y));
                widest = widest.max(width);
            }
            if !axis.label.is_empty() {
                let reach = 12.0 + ox.abs() * (widest + 6.0) + oy.abs() * 16.0;
                let mid: [f64; 3] = std::array::from_fn(|j| (axis.start[j] + axis.end[j]) / 2.0);
                let m = project(mid);
                let st = TextStyle::sans(11.0);
                let (lx, ly) = (m[0] + ox * reach, m[1] + oy * reach);
                let middle_sans = mono_baseline_middle(st);
                if key == 2 && ox.abs() > 0.5 {
                    let xf = Affine::at(lx, ly, -std::f64::consts::FRAC_PI_2);
                    label(
                        self.dev,
                        &axis.label,
                        "tex",
                        0.0,
                        middle_sans,
                        "center",
                        st,
                        theme.text,
                        xf,
                    );
                } else {
                    let al = if ox.abs() > 0.35 {
                        if ox > 0.0 { "left" } else { "right" }
                    } else {
                        "center"
                    };
                    label(
                        self.dev,
                        &axis.label,
                        "tex",
                        lx,
                        ly + middle_sans,
                        al,
                        st,
                        theme.text,
                        Affine::IDENTITY,
                    );
                }
            }
        }
        // Relaxed export: free text in a 3-D axes, projected to its anchor.
        for (j, s) in series(a).iter().enumerate() {
            if text(s, "kind") != "text" {
                continue;
            }
            let position = nums(s, "position");
            if text(s, "units") == "data" && position.len() >= 2 {
                let z = num(s, "z").unwrap_or(0.0);
                let q = project(cam.normalised([position[0], position[1], z]));
                self.text_at(s, a, j, q[0], q[1]);
            } else if position.len() >= 2 {
                let (x, y) = (vp.x + position[0] * vp.w, vp.y + (1.0 - position[1]) * vp.h);
                self.text_at(s, a, j, x, y);
            }
        }
        if !text(a, "title").is_empty() {
            let st = TextStyle {
                size: 12.0,
                bold: true,
                ..TextStyle::default()
            };
            label(
                self.dev,
                text(a, "title"),
                "tex",
                vp.x + vp.w / 2.0,
                (vp.y - 8.0).max(14.0),
                "center",
                st,
                theme.text,
                Affine::IDENTITY,
            );
        }
        self.draw_legend_3d(a, vp, &colors);
    }

    /// `Figure3D.drawLegend`.
    fn draw_legend_3d(&mut self, a: &Json, vp: Box2, colors: &AxesColors) {
        let Some(legend) = a.get("legend") else {
            return;
        };
        if !flag(legend, "visible") {
            return;
        }
        let labels = strings(legend, "labels");
        let list = series(a);
        let mut seen: Vec<String> = Vec::new();
        let mut items: Vec<(usize, String)> = Vec::new();
        for (i, s) in list.iter().enumerate() {
            let role = text(s, "role");
            let contour = role == "contour" || role == "contourf";
            let key = if contour && s.has("contour_group") {
                format!("contour:{}", num(s, "contour_group").unwrap_or(0.0))
            } else {
                format!("series:{}", num(s, "id").unwrap_or(i as f64))
            };
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            let logical = seen.len() - 1;
            let label_text = if logical < labels.len() {
                labels[logical].clone()
            } else {
                text(s, "display_name").to_string()
            };
            if !label_text.is_empty() {
                items.push((i, label_text));
            }
        }
        if items.is_empty() {
            return;
        }
        let st = TextStyle::sans(10.0);
        let widest = items
            .iter()
            .map(|(_, l)| measure_runs(&tex::parse(l, "tex"), st))
            .fold(0.0, f64::max);
        let width = vp.w.min(widest + 36.0);
        let height = items.len() as f64 * 16.0 + 8.0;
        let location = text(legend, "location");
        let x = if location.contains("west") {
            vp.x + 6.0
        } else {
            vp.x + vp.w - width - 6.0
        };
        let y = if location.contains("south") {
            vp.y + vp.h - height - 6.0
        } else {
            vp.y + 6.0
        };
        let theme = self.theme;
        let frame = adjust(theme.axis, &theme);
        self.dev
            .fill_rect(x, y, width, height, theme.background, 1.0);
        let mut outline = crate::device::Path::new();
        outline.rect(x, y, width, height);
        self.dev.stroke(&outline, 1.0, &[], frame, 1.0);
        let contours = contour_display_colors(a, colors, &theme);
        for (n, (i, label_text)) in items.iter().enumerate() {
            let s = &list[*i];
            let yy = y + 13.0 + n as f64 * 16.0;
            let kind = text(s, "kind");
            let swatch = |dev: &mut dyn crate::device::Device,
                          face: Option<Rgb>,
                          edge: Option<Rgb>,
                          lw: f64,
                          dashes: &[f64]| {
                if let Some(c) = face {
                    dev.fill_rect(x + 6.0, yy - 9.0, 17.0, 9.0, c, 1.0);
                }
                if let Some(c) = edge {
                    let mut p = crate::device::Path::new();
                    p.rect(x + 6.0, yy - 9.0, 17.0, 9.0);
                    dev.stroke(&p, lw, dashes, c, 1.0);
                }
            };
            if kind == "surface" {
                let origin = nums(s, "cell_origins").first().copied().unwrap_or(0.0) as usize;
                let face = s
                    .get("face_color")
                    .and_then(|d| colors.descriptor(d, origin));
                let first_edge = desc(s, "edge_indices").map_or(0, |e| e.at(0) as usize);
                let edge = s
                    .get("edge_color")
                    .and_then(|d| colors.descriptor(d, first_edge))
                    .map(|c| adjust(c, &theme));
                swatch(
                    self.dev,
                    Some(face.unwrap_or(theme.background)),
                    edge,
                    1.0,
                    &[],
                );
            } else if kind == "patch_raster" {
                let face = PatchPaint::of(s.get("face")).swatch().map(clamp01);
                let edge = PatchPaint::of(s.get("edge"))
                    .swatch()
                    .filter(|_| text(s, "line_style") != "none")
                    .map(|c| adjust(c, &theme));
                swatch(self.dev, face, edge, 1.0, dash(text(s, "line_style")));
            } else if kind == "patch3d" {
                let face = s.get("face_color").and_then(|d| colors.patch(d, 0));
                let edge = s.get("edge_color").and_then(|d| colors.patch(d, 0));
                let lw = (num(s, "line_width").unwrap_or(0.5) * 96.0 / 72.0).max(1.0);
                let dashes = dash(text(s, "line_style"));
                let has_edge =
                    text(s, "line_style") != "none" && num(s, "line_width").unwrap_or(0.0) > 0.0;
                match face {
                    Some(_) => swatch(
                        self.dev,
                        face,
                        if has_edge { edge } else { None },
                        lw,
                        dashes,
                    ),
                    None => {
                        if let (Some(c), true) = (edge, has_edge) {
                            let mut p = crate::device::Path::new();
                            p.move_to(x + 6.0, yy - 3.0);
                            p.line_to(x + 23.0, yy - 3.0);
                            self.dev.stroke(&p, lw, dashes, adjust(c, &theme), 1.0);
                        }
                    }
                }
            } else {
                if text(s, "line_style") != "none"
                    && let Some(raw) = rgb(s, "line_color")
                {
                    let ink = contours
                        .get(*i)
                        .copied()
                        .flatten()
                        .unwrap_or_else(|| adjust(raw, &theme));
                    let lw = (num(s, "line_width").unwrap_or(0.5) * 96.0 / 72.0).max(1.0);
                    let mut p = crate::device::Path::new();
                    p.move_to(x + 6.0, yy - 3.0);
                    p.line_to(x + 23.0, yy - 3.0);
                    self.dev
                        .stroke(&p, lw, dash(text(s, "line_style")), ink, 1.0);
                }
                let edge = rgb(s, "marker_edge_color").map(|c| adjust(c, &theme));
                let face = if flag(s, "marker_face_auto") {
                    Some(adjust(theme.background, &theme))
                } else {
                    rgb(s, "marker_face_color").map(|c| adjust(c, &theme))
                };
                self.marker(
                    x + 14.5,
                    yy - 3.0,
                    text(s, "marker"),
                    edge,
                    face,
                    1.0,
                    1.0,
                    1.0,
                    1.0,
                );
            }
            label(
                self.dev,
                label_text,
                "tex",
                x + 27.0,
                yy,
                "left",
                st,
                theme.text,
                Affine::IDENTITY,
            );
        }
    }
}
