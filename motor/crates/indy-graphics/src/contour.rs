//! Contour lines and filled contours with Octave-compatible geometry.
//!
//! The behaviour was measured against GNU Octave 11.3 as a black box (property
//! reads only; parity is checked by `tests/oracle.rs` on the `contour*` cases of
//! `tests/oracle/cases.txt`); the algorithm below is IndyMAT's own:
//!
//! * Crossings: a node is "high" when `z >= level`. An edge crosses when its two
//!   nodes differ. The crossing point is computed from the node `p` that comes
//!   first when walking the computing cell counter-clockwise (bottom and right
//!   edges start at the lower-index node, top and left at the higher-index node)
//!   as `p + (p - q) / (r - 1)` with `r = (zq - level) / (zp - level)`, in both
//!   x and y. The first point of a path is computed in the cell the path enters,
//!   every later point in the cell the path leaves. This reproduces Octave's
//!   coordinates bit for bit, including closed loops whose recomputed closing
//!   point differs from the start by one ulp (Octave then stores them open).
//! * Saddles (four crossing edges) always pair bottom with right and top with
//!   left.
//! * Open paths start on boundary edges: bottom and top rows column by column
//!   (bottom first), then left and right columns row by row (left first).
//!   Closed loops start at the first unvisited interior horizontal edge in
//!   row-major order and leave it downwards.
//! * Filled contours trace the same way on `z` padded with one ring of `-Inf`
//!   whose coordinates are extrapolated linearly (`2*x1 - x2`), so every path is
//!   a loop and boundary corners appear twice.
//!
//! Missing data (NaN) is not part of the Octave parity (Octave 11.3 emits
//! degenerate dead-end paths there): cells with a NaN corner are skipped and
//! act as boundaries for lines; for fills NaN nodes count as lower than every
//! level and loops around them become background (NaN level) patches.

#![forbid(unsafe_code)]

use std::collections::HashMap;

/// Grid coordinates: x has `cols` values, y has `rows` values (vector form),
/// or full rows×cols matrices (column-major).
#[derive(Debug, Clone, PartialEq)]
pub enum Coord {
    Vector(Vec<f64>),
    Matrix(Vec<f64>),
}

/// One native contour line patch: its level and its xdata/ydata exactly as
/// Octave stores them. Closed loops omit the repeated first point; open paths
/// end with a NaN vertex.
#[derive(Debug, Clone, PartialEq)]
pub struct ContourLine {
    pub level: f64,
    pub x: Vec<f64>,
    pub y: Vec<f64>,
}

/// One filled contour patch: its level (NaN for a background hole), its
/// vertices and its single face (1-based vertex indices).
#[derive(Debug, Clone, PartialEq)]
pub struct FilledPatch {
    pub level: f64,
    pub vertices: Vec<[f64; 2]>,
    pub face: Vec<f64>,
}

/// Octave's `linspace(a, b, m)`.
fn linspace(a: f64, b: f64, m: usize) -> Vec<f64> {
    match m {
        0 => return Vec::new(),
        1 => return vec![b],
        _ => {}
    }
    let d = (b - a) / (m - 1) as f64;
    (0..m)
        .map(|i| {
            if i == 0 {
                a
            } else if i == m - 1 {
                b
            } else if m % 2 == 1 && 2 * i == m - 1 {
                (a + b) / 2.0
            } else if 2 * i < m {
                (i as f64).mul_add(d, a)
            } else {
                (-((m - 1 - i) as f64)).mul_add(d, b)
            }
        })
        .collect()
}

/// Sorted, duplicate-free level list (NaNs last, each kept), as Octave's
/// `unique` leaves it.
fn normalize_levels(levels: &[f64]) -> Vec<f64> {
    let mut v = levels.to_vec();
    v.sort_by(|a, b| {
        a.partial_cmp(b)
            .unwrap_or_else(|| a.is_nan().cmp(&b.is_nan()))
    });
    v.dedup_by(|a, b| a == b);
    v
}

/// Default levels as Octave chooses `LevelList` for `contour(Z)` (`n = None`,
/// ten levels) and `contour(Z, n)`: `unique(linspace(zmin, zmax, n + 2)(1:n+1))`.
/// `zmin`/`zmax` are the finite minimum and maximum of Z. `n = Some(0)` returns
/// an empty list (Octave's own result for `contour(Z, 0)` is inconsistent).
pub fn auto_levels(zmin: f64, zmax: f64, n: Option<usize>) -> Vec<f64> {
    let n = n.unwrap_or(10);
    if n == 0 || n > 10_000_000 {
        return Vec::new();
    }
    let mut v = linspace(zmin, zmax, n + 2);
    v.pop();
    normalize_levels(&v)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Edge {
    H(usize, usize),
    V(usize, usize),
}

struct Grid {
    rows: usize,
    cols: usize,
    z: Vec<f64>,
    x: Vec<f64>,
    y: Vec<f64>,
    /// Curvilinear X/Y (column-major): paths are traced in 1-based index
    /// space and mapped with Octave's `interp2` arithmetic afterwards.
    curvilinear: Option<(Vec<f64>, Vec<f64>, usize, usize)>,
}

impl Grid {
    fn z(&self, i: usize, j: usize) -> f64 {
        self.z[i + j * self.rows]
    }
    fn node(&self, (i, j): (usize, usize)) -> (f64, f64, f64) {
        let k = i + j * self.rows;
        (self.z[k], self.x[k], self.y[k])
    }
    fn cell_ok(&self, (i, j): (usize, usize)) -> bool {
        i + 1 < self.rows
            && j + 1 < self.cols
            && !self.z(i, j).is_nan()
            && !self.z(i, j + 1).is_nan()
            && !self.z(i + 1, j).is_nan()
            && !self.z(i + 1, j + 1).is_nan()
    }
}

/// Expand coordinates to full column-major matrices; `None` on size mismatch.
fn expand(x: &Coord, y: &Coord, rows: usize, cols: usize) -> Option<(Vec<f64>, Vec<f64>)> {
    let n = rows.checked_mul(cols)?;
    let full = |c: &Coord, len: usize, along_cols: bool| -> Option<Vec<f64>> {
        match c {
            Coord::Vector(v) if v.len() == len => Some(
                (0..n)
                    .map(|k| v[if along_cols { k / rows } else { k % rows }])
                    .collect(),
            ),
            Coord::Matrix(m) if m.len() == n => Some(m.clone()),
            _ => None,
        }
    };
    Some((full(x, cols, true)?, full(y, rows, false)?))
}

fn grid(x: &Coord, y: &Coord, z: &[f64], rows: usize, cols: usize) -> Option<Grid> {
    if rows < 2 || cols < 2 || rows.checked_mul(cols)? != z.len() {
        return None;
    }
    let (x, y) = expand(x, y, rows, cols)?;
    // Meshgrid matrices behave exactly like vectors; anything else is curvilinear.
    let mesh = (0..rows * cols).all(|k| x[k] == x[k - k % rows] && y[k] == y[k % rows]);
    let (x, y, curvilinear) = if mesh {
        (x, y, None)
    } else {
        let idx =
            |k: usize, along_cols: bool| (1 + if along_cols { k / rows } else { k % rows }) as f64;
        (
            (0..rows * cols).map(|k| idx(k, true)).collect(),
            (0..rows * cols).map(|k| idx(k, false)).collect(),
            Some((x, y, rows, cols)),
        )
    };
    Some(Grid {
        rows,
        cols,
        z: z.to_vec(),
        x,
        y,
        curvilinear,
    })
}

/// Octave's linear `interp2(V, u, w)` on a rows×cols matrix at 1-based
/// index-space position (u = column, w = row).
fn interp2(v: &[f64], rows: usize, cols: usize, u: f64, w: f64) -> f64 {
    if !(u >= 1.0 && u <= cols as f64 && w >= 1.0 && w <= rows as f64) {
        return f64::NAN;
    }
    let j = (u.floor() as usize).clamp(1, cols - 1);
    let i = (w.floor() as usize).clamp(1, rows - 1);
    let (fu, fw) = (u - j as f64, w - i as f64);
    let at = |i: usize, j: usize| v[(i - 1) + (j - 1) * rows];
    let (v11, v12, v21, v22) = (at(i, j), at(i, j + 1), at(i + 1, j), at(i + 1, j + 1));
    let (b, c, d) = (v12 - v11, v21 - v11, (v22 - v12) - (v21 - v11));
    v11 + b * fu + c * fw + d * fu * fw
}

impl Grid {
    /// Map a traced point to user coordinates.
    fn map(&self, p: [f64; 2]) -> [f64; 2] {
        match &self.curvilinear {
            None => p,
            Some((x, y, r, c)) => [
                interp2(x, *r, *c, p[0], p[1]),
                interp2(y, *r, *c, p[0], p[1]),
            ],
        }
    }
}

/// The grid surrounded by one ring of `-Inf` nodes with linearly extrapolated
/// coordinates (NaN nodes become `-Inf` as well); used for filled contours.
fn padded(g: &Grid) -> Grid {
    let (r, c) = (g.rows + 2, g.cols + 2);
    let at = |v: &[f64], i: usize, j: usize| v[i + j * g.rows];
    // Padded indices (i, j); pad nodes extrapolate from the nearest data node.
    let coord = |v: &[f64], i: usize, j: usize| -> f64 {
        let (di, dj) = (i.clamp(1, g.rows) - 1, j.clamp(1, g.cols) - 1);
        let base = at(v, di, dj);
        if j == 0 {
            2.0 * base - at(v, di, 1)
        } else if j == c - 1 {
            2.0 * base - at(v, di, g.cols - 2)
        } else if i == 0 {
            2.0 * base - at(v, 1, dj)
        } else if i == r - 1 {
            2.0 * base - at(v, g.rows - 2, dj)
        } else {
            base
        }
    };
    let mut z = vec![f64::NEG_INFINITY; r * c];
    let mut x = vec![0.0; r * c];
    let mut y = vec![0.0; r * c];
    for j in 0..c {
        for i in 0..r {
            let k = i + j * r;
            x[k] = coord(&g.x, i, j);
            y[k] = coord(&g.y, i, j);
            if (1..=g.rows).contains(&i) && (1..=g.cols).contains(&j) {
                let v = at(&g.z, i - 1, j - 1);
                z[k] = if v.is_nan() { f64::NEG_INFINITY } else { v };
            }
        }
    }
    Grid {
        rows: r,
        cols: c,
        z,
        x,
        y,
        curvilinear: g.curvilinear.clone(),
    }
}

/// Edges of cell (i, j) counter-clockwise: bottom, right, top, left.
fn cell_edges((i, j): (usize, usize)) -> [Edge; 4] {
    [
        Edge::H(i, j),
        Edge::V(i, j + 1),
        Edge::H(i + 1, j),
        Edge::V(i, j),
    ]
}

/// Cells on either side of an edge (below/left first).
fn edge_cells(e: Edge) -> [Option<(usize, usize)>; 2] {
    match e {
        Edge::H(i, j) => [i.checked_sub(1).map(|a| (a, j)), Some((i, j))],
        Edge::V(i, j) => [j.checked_sub(1).map(|b| (i, b)), Some((i, j))],
    }
}

fn edge_nodes(e: Edge) -> ((usize, usize), (usize, usize)) {
    match e {
        Edge::H(i, j) => ((i, j), (i, j + 1)),
        Edge::V(i, j) => ((i, j), (i + 1, j)),
    }
}

/// One traced path: its points and the edges they lie on.
struct Path {
    pts: Vec<[f64; 2]>,
    edges: Vec<Edge>,
}

struct Tracer<'a> {
    g: &'a Grid,
    level: f64,
    seen_h: Vec<bool>,
    seen_v: Vec<bool>,
}

impl Tracer<'_> {
    fn high(&self, v: f64) -> bool {
        v >= self.level
    }
    fn valid(&self, c: Option<(usize, usize)>) -> Option<(usize, usize)> {
        c.filter(|&c| self.g.cell_ok(c))
    }
    fn valid_cells(&self, e: Edge) -> usize {
        let [a, b] = edge_cells(e);
        usize::from(self.valid(a).is_some()) + usize::from(self.valid(b).is_some())
    }
    fn crosses(&self, e: Edge) -> bool {
        let (a, b) = edge_nodes(e);
        let (za, zb) = (self.g.node(a).0, self.g.node(b).0);
        !za.is_nan() && !zb.is_nan() && self.high(za) != self.high(zb) && self.valid_cells(e) > 0
    }
    fn seen(&mut self, e: Edge) -> &mut bool {
        let c = self.g.cols;
        match e {
            Edge::H(i, j) => &mut self.seen_h[i * (c - 1) + j],
            Edge::V(i, j) => &mut self.seen_v[i * c + j],
        }
    }
    /// Crossing point on `e` as computed from `cell`.
    fn point(&self, e: Edge, cell: (usize, usize)) -> [f64; 2] {
        let k = cell_edges(cell).iter().position(|&x| x == e).unwrap_or(0);
        let (lo, hi) = edge_nodes(e);
        let (p, q) = if k < 2 { (lo, hi) } else { (hi, lo) };
        let (zp, xp, yp) = self.g.node(p);
        let (zq, xq, yq) = self.g.node(q);
        let r = (zq - self.level) / (zp - self.level);
        [xp + (xp - xq) / (r - 1.0), yp + (yp - yq) / (r - 1.0)]
    }
    fn exit(&self, cell: (usize, usize), entry: Edge) -> Option<Edge> {
        let es = cell_edges(cell);
        let k = es.iter().position(|&x| x == entry)?;
        let mut others = (0..4).filter(|&m| m != k && self.crosses(es[m]));
        let first = others.next()?;
        if others.next().is_none() {
            return Some(es[first]);
        }
        Some(es[[1, 0, 3, 2][k]])
    }
    fn trace(&mut self, start: Edge, first: (usize, usize)) -> Path {
        *self.seen(start) = true;
        let mut path = Path {
            pts: vec![self.point(start, first)],
            edges: vec![start],
        };
        let (mut cell, mut cur) = (first, start);
        let limit = self.seen_h.len() + self.seen_v.len() + 2;
        while path.pts.len() <= limit {
            let Some(nx) = self.exit(cell, cur) else {
                break;
            };
            path.pts.push(self.point(nx, cell));
            path.edges.push(nx);
            if nx == start {
                break;
            }
            *self.seen(nx) = true;
            let [a, b] = edge_cells(nx);
            let other = if a == Some(cell) { b } else { a };
            match self.valid(other) {
                Some(c) => {
                    cell = c;
                    cur = nx;
                }
                None => break,
            }
        }
        path
    }
}

/// Paths of one level in Octave's contour-matrix order.
fn trace_level(g: &Grid, level: f64) -> Vec<Path> {
    let (r, c) = (g.rows, g.cols);
    let mut t = Tracer {
        g,
        level,
        seen_h: vec![false; r * (c - 1)],
        seen_v: vec![false; (r - 1) * c],
    };
    let mut starts = Vec::new();
    for j in 0..c - 1 {
        starts.push(Edge::H(0, j));
        starts.push(Edge::H(r - 1, j));
    }
    for i in 0..r - 1 {
        starts.push(Edge::V(i, 0));
        starts.push(Edge::V(i, c - 1));
    }
    // Edges bordering a missing cell are boundaries too (not part of the parity).
    for i in 1..r - 1 {
        for j in 0..c - 1 {
            starts.push(Edge::H(i, j));
        }
    }
    for i in 0..r - 1 {
        for j in 1..c - 1 {
            starts.push(Edge::V(i, j));
        }
    }
    let mut out = Vec::new();
    for s in starts {
        if t.crosses(s) && !*t.seen(s) && t.valid_cells(s) == 1 {
            let [a, b] = edge_cells(s);
            if let Some(cell) = t.valid(a).or(t.valid(b)) {
                out.push(t.trace(s, cell));
            }
        }
    }
    for i in 1..r - 1 {
        for j in 0..c - 1 {
            let s = Edge::H(i, j);
            if t.crosses(s) && !*t.seen(s) && t.valid_cells(s) == 2 {
                out.push(t.trace(s, (i - 1, j)));
            }
        }
    }
    out
}

/// All line patches of `contour(X, Y, Z, levels)` in `get(h, 'children')` order,
/// i.e. the reverse of the contour matrix (levels descending; within a level
/// the last traced path first). Levels are sorted and de-duplicated first, as
/// Octave does for `LevelList`. Invalid input gives an empty result.
pub fn contour_lines(
    x: &Coord,
    y: &Coord,
    z: &[f64],
    rows: usize,
    cols: usize,
    levels: &[f64],
) -> Vec<ContourLine> {
    let Some(g) = grid(x, y, z, rows, cols) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for &level in &normalize_levels(levels) {
        for mut p in trace_level(&g, level) {
            p.pts.iter_mut().for_each(|q| *q = g.map(*q));
            let n = p.pts.len();
            let closed = n >= 2 && p.pts[0] == p.pts[n - 1];
            let keep = if closed { n - 1 } else { n };
            let mut line = ContourLine {
                level,
                x: p.pts[..keep].iter().map(|q| q[0]).collect(),
                y: p.pts[..keep].iter().map(|q| q[1]).collect(),
            };
            if !closed {
                line.x.push(f64::NAN);
                line.y.push(f64::NAN);
            }
            out.push(line);
        }
    }
    out.reverse();
    out
}

/// Octave's `polyarea`: `abs(sum(x .* (y(next) - y(prev)))) / 2`.
fn polyarea(p: &[[f64; 2]]) -> f64 {
    let n = p.len();
    let mut a = 0.0;
    for i in 0..n {
        a += p[i][0] * (p[(i + 1) % n][1] - p[(i + n - 1) % n][1]);
    }
    a.abs() / 2.0
}

/// Whether the region enclosed by a loop lies above its level. Loops leave
/// their start edge `H(i, j)` downwards, so the interior is the node left of
/// travel, `(i, j + 1)`, when the loop runs counter-clockwise in index space
/// and `(i, j)` otherwise. Also returns that node.
fn loop_interior(g: &Grid, p: &Path, level: f64) -> (bool, (usize, usize)) {
    let mid = |e: Edge| -> (i128, i128) {
        match e {
            Edge::H(i, j) => (2 * j as i128 + 1, 2 * i as i128),
            Edge::V(i, j) => (2 * j as i128, 2 * i as i128 + 1),
        }
    };
    let n = p.edges.len().saturating_sub(1);
    let mut twice: i128 = 0;
    for k in 0..n {
        let (a, b) = (mid(p.edges[k]), mid(p.edges[(k + 1) % n]));
        twice += a.0 * b.1 - b.0 * a.1;
    }
    let (i, j) = match p.edges.first() {
        Some(Edge::H(i, j) | Edge::V(i, j)) => (*i, *j),
        None => return (true, (0, 0)),
    };
    let node = if twice > 0 { (i, j + 1) } else { (i, j) };
    (g.z(node.0, node.1) >= level, node)
}

/// All patches of `contourf(X, Y, Z, levels)` in `get(h, 'children')` order:
/// ascending `polyarea` of the traced loop (stable in contour-matrix order),
/// i.e. the reverse of Octave's painting order. A loop enclosing values above
/// its level gets that level; one enclosing lower values (a hole) gets the
/// previous level, or NaN below the lowest level (background hole). Loops with
/// identical vertices at several levels (the whole-domain border) are kept
/// once, at the highest such level. A patch keeps all traced points unless the
/// loop closed exactly or it is a background hole; its face lists all but the
/// last traced point. Invalid input gives an empty result (Octave raises an
/// error when no patch exists at all).
pub fn contourf_patches(
    x: &Coord,
    y: &Coord,
    z: &[f64],
    rows: usize,
    cols: usize,
    levels: &[f64],
) -> Vec<FilledPatch> {
    let Some(g) = grid(x, y, z, rows, cols) else {
        return Vec::new();
    };
    let pg = padded(&g);
    let levels = normalize_levels(levels);
    let mut found: Vec<(FilledPatch, f64)> = Vec::new();
    for (li, &level) in levels.iter().enumerate() {
        for mut p in trace_level(&pg, level) {
            p.pts.iter_mut().for_each(|q| *q = pg.map(*q));
            let n = p.pts.len();
            if n < 2 {
                continue;
            }
            let (above, node) = loop_interior(&pg, &p, level);
            let missing = (1..=g.rows).contains(&node.0)
                && (1..=g.cols).contains(&node.1)
                && g.z(node.0 - 1, node.1 - 1).is_nan();
            let patch_level = if above {
                level
            } else if li == 0 || missing {
                f64::NAN
            } else {
                levels[li - 1]
            };
            let exact = p.pts[0] == p.pts[n - 1];
            let keep = if exact || patch_level.is_nan() {
                n - 1
            } else {
                n
            };
            let patch = FilledPatch {
                level: patch_level,
                vertices: p.pts[..keep].to_vec(),
                face: (1..n).map(|k| k as f64).collect(),
            };
            found.push((patch, polyarea(&p.pts)));
        }
    }
    // Identical loops: keep the last (highest-level) one.
    let mut last: HashMap<Vec<u64>, usize> = HashMap::new();
    for (k, (p, _)) in found.iter().enumerate() {
        let key = p
            .vertices
            .iter()
            .flat_map(|v| [v[0].to_bits(), v[1].to_bits()])
            .collect();
        last.insert(key, k);
    }
    let mut keep = vec![false; found.len()];
    for &k in last.values() {
        keep[k] = true;
    }
    let mut order: Vec<usize> = (0..found.len()).filter(|&k| keep[k]).collect();
    order.sort_by(|&a, &b| found[a].1.total_cmp(&found[b].1));
    let mut slots: Vec<Option<FilledPatch>> = found.into_iter().map(|(p, _)| Some(p)).collect();
    order.into_iter().filter_map(|k| slots[k].take()).collect()
}

/// Raw contour-matrix segments (closed loops repeat their first point).
#[doc(hidden)]
pub fn raw_contourc(
    x: &Coord,
    y: &Coord,
    z: &[f64],
    rows: usize,
    cols: usize,
    levels: &[f64],
    filled: bool,
) -> Vec<(f64, Vec<[f64; 2]>)> {
    let Some(g) = grid(x, y, z, rows, cols) else {
        return Vec::new();
    };
    let g = if filled { padded(&g) } else { g };
    let mut out = Vec::new();
    for &l in &normalize_levels(levels) {
        for p in trace_level(&g, l) {
            out.push((l, p.pts.iter().map(|q| g.map(*q)).collect()));
        }
    }
    out
}
