//! `integral2`, `integral3`, `quad2d`.
//!
//! * Method `'tiled'` (and `quad2d`): adaptive tensor-product
//!   Gauss–Kronrod (7, 15) cubature on the rectangle obtained by mapping
//!   `y` from `[ymin(x), ymax(x)]` to a fixed interval; both directions use
//!   the endpoint-smoothing cubic substitution that MATLAB's `integral`
//!   uses, which weakens endpoint singularities. Tiles failing a local
//!   error test proportional to their area are split in four; all new
//!   tiles of a pass are evaluated in one vectorised call of FUN (FUN
//!   receives matrices, as MATLAB requires). Finite limits only.
//! * Method `'iterated'`: nested adaptive Gauss–Kronrod (7, 15) quadrature
//!   (one dimension at a time); infinite limits are handled with the same
//!   variable maps as `integral`.
//! * `'auto'` uses `'tiled'` when all limits are finite, `'iterated'`
//!   otherwise. `integral3` integrates the outermost variable with the
//!   one-dimensional adaptive rule and the inner (y, z) integral with the
//!   chosen method.
//!
//! Integrands must be real (complex values raise an explicit error).

use indy_value::Value;

use super::super::{arg_f64, arg_str, err, need};
use super::{call_n, take_n};
use crate::{Interp, R};

const XGK: [f64; 7] = [
    0.991_455_371_120_812_6,
    0.949_107_912_342_758_5,
    0.864_864_423_359_769_1,
    0.741_531_185_599_394_4,
    0.586_087_235_467_691_1,
    0.405_845_151_377_397_2,
    0.207_784_955_007_898_5,
];
const WGK: [f64; 8] = [
    0.022_935_322_010_529_22,
    0.063_092_092_629_978_55,
    0.104_790_010_322_250_18,
    0.140_653_259_715_525_92,
    0.169_004_726_639_267_9,
    0.190_350_578_064_785_4,
    0.204_432_940_075_298_9,
    0.209_482_141_084_727_83,
];
const WG: [f64; 4] = [
    0.129_484_966_168_869_7,
    0.279_705_391_489_276_7,
    0.381_830_050_505_118_9,
    0.417_959_183_673_469_4,
];

/// 15 nodes on [-1, 1] with Kronrod and (embedded) Gauss weights.
fn gk15() -> ([f64; 15], [f64; 15], [f64; 15]) {
    let mut x = [0.0; 15];
    let mut wk = [0.0; 15];
    let mut wg = [0.0; 15];
    for j in 0..7 {
        x[j] = -XGK[j];
        x[14 - j] = XGK[j];
        wk[j] = WGK[j];
        wk[14 - j] = WGK[j];
        if j % 2 == 1 {
            wg[j] = WG[j / 2];
            wg[14 - j] = WG[j / 2];
        }
    }
    wk[7] = WGK[7];
    wg[7] = WG[3];
    (x, wk, wg)
}

/// Cubic endpoint-smoothing map of [-1, 1] onto [lo, hi]: (point, dx/du).
fn smooth(lo: f64, hi: f64, u: f64) -> (f64, f64) {
    let m = 0.5 * (lo + hi);
    let h = 0.5 * (hi - lo);
    (m + h * 0.5 * u * (3.0 - u * u), h * 1.5 * (1.0 - u * u))
}

/// Integrand at points (xs, ys) given with the matrix shape `dims`.
type F2<'a> = dyn FnMut(&mut Interp, &[f64], &[f64], &[usize]) -> R<Vec<f64>> + 'a;
/// Limit function of x at points `xs` with shape `dims`.
type L1<'a> = dyn FnMut(&mut Interp, &[f64], &[usize]) -> R<Vec<f64>> + 'a;
/// One-dimensional integrand at points.
type F1<'a> = dyn FnMut(&mut Interp, &[f64]) -> R<Vec<f64>> + 'a;

fn numeric_out(v: &Value, n: usize, fname: &str, what: &str) -> R<Vec<f64>> {
    let full;
    let v = match v {
        Value::Sparse(s) => {
            full = s.to_dense()?;
            &full
        }
        other => other,
    };
    let data = match v {
        Value::Num(nn) if nn.im.is_some() => {
            return Err(err(
                &format!("MATLAB:{}:complexValues", fname),
                format!("{}: complex-valued {} are not supported.", fname, what),
            ));
        }
        Value::Num(_) | Value::Bool(_) => v.to_f64_vec().unwrap_or_default(),
        other => {
            return Err(err(
                &format!("MATLAB:{}:invalidFunctionOutput", fname),
                format!(
                    "{}: {} must be numeric, got a value of class {}.",
                    fname,
                    what,
                    other.class_name()
                ),
            ));
        }
    };
    if data.len() == n {
        Ok(data)
    } else if data.len() == 1 && what != "integrand values" {
        Ok(vec![data[0]; n])
    } else {
        Err(err(
            &format!("MATLAB:{}:funSizeMismatch", fname),
            format!(
                "{}: output of the {} must be the same size as the input ({} elements expected, got {}). Make sure the function is vectorized (use .*, ./ and .^).",
                fname,
                if what == "integrand values" {
                    "integrand"
                } else {
                    "limit function"
                },
                n,
                data.len()
            ),
        ))
    }
}

// ---------------------------------------------------------------------------
// One-dimensional adaptive Gauss–Kronrod
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Map {
    Finite(f64, f64),
    RightInf(f64),
    LeftInf(f64),
    BothInf,
}

fn map_point(map: Map, t: f64) -> (f64, f64) {
    match map {
        Map::Finite(a, b) => smooth(a, b, t),
        Map::RightInf(a) => (a + t / (1.0 - t), 1.0 / ((1.0 - t) * (1.0 - t))),
        Map::LeftInf(b) => (b - t / (1.0 - t), 1.0 / ((1.0 - t) * (1.0 - t))),
        Map::BothInf => {
            let d = 1.0 - t * t;
            (t / d, (1.0 + t * t) / (d * d))
        }
    }
}

/// Adaptive quadrature of `g` over [a, b] (infinite limits allowed).
/// Returns (value, error estimate, converged).
fn adapt1(
    it: &mut Interp,
    g: &mut F1,
    a: f64,
    b: f64,
    abstol: f64,
    reltol: f64,
) -> R<(f64, f64, bool)> {
    if a.is_nan() || b.is_nan() {
        return Ok((f64::NAN, f64::NAN, false));
    }
    if a == b {
        return Ok((0.0, 0.0, true));
    }
    let (lo, hi, sign) = if a < b { (a, b, 1.0) } else { (b, a, -1.0) };
    let (map, tl, th) = match (lo.is_finite(), hi.is_finite()) {
        (true, true) => (Map::Finite(lo, hi), -1.0, 1.0),
        (true, false) => (Map::RightInf(lo), 0.0, 1.0),
        (false, true) => (Map::LeftInf(hi), 0.0, 1.0),
        (false, false) => (Map::BothInf, -1.0, 1.0),
    };
    let (nodes, wk, wg) = gk15();
    // evaluate a batch of intervals in one call
    let mut eval = |it: &mut Interp, iv: &[(f64, f64)]| -> R<Vec<(f64, f64)>> {
        let mut xs = Vec::with_capacity(15 * iv.len());
        let mut ws = Vec::with_capacity(15 * iv.len());
        for &(l, r) in iv {
            let c = 0.5 * (l + r);
            let h = 0.5 * (r - l);
            for k in 0..15 {
                let (x, w) = map_point(map, c + h * nodes[k]);
                xs.push(x);
                ws.push(w * h);
            }
        }
        let fx = g(it, &xs)?;
        let mut out = Vec::with_capacity(iv.len());
        for i in 0..iv.len() {
            let mut qk = 0.0;
            let mut qg = 0.0;
            for k in 0..15 {
                let v = fx[i * 15 + k] * ws[i * 15 + k];
                qk += wk[k] * v;
                qg += wg[k] * v;
            }
            out.push((qk, (qk - qg).abs()));
        }
        Ok(out)
    };
    let pieces = 10usize;
    let mut iv: Vec<(f64, f64)> = (0..pieces)
        .map(|k| {
            (
                tl + (th - tl) * k as f64 / pieces as f64,
                if k + 1 == pieces {
                    th
                } else {
                    tl + (th - tl) * (k + 1) as f64 / pieces as f64
                },
            )
        })
        .collect();
    let mut qe = eval(it, &iv)?;
    let max_intervals = 650;
    loop {
        let q: f64 = qe.iter().map(|p| p.0).sum();
        let e: f64 = qe.iter().map(|p| p.1).sum();
        if !q.is_finite() || !e.is_finite() {
            return Ok((sign * q, e, false));
        }
        let tol = abstol.max(reltol * q.abs());
        if e <= tol {
            return Ok((sign * q, e, true));
        }
        if iv.len() >= max_intervals {
            return Ok((sign * q, e, false));
        }
        // split every interval failing its share of the tolerance
        let total = th - tl;
        let mut split: Vec<usize> = (0..iv.len())
            .filter(|&i| qe[i].1 > tol * (iv[i].1 - iv[i].0) / total)
            .collect();
        if split.is_empty() {
            let worst = (0..iv.len())
                .max_by(|&i, &j| {
                    qe[i]
                        .1
                        .partial_cmp(&qe[j].1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap_or(0);
            split.push(worst);
        }
        split.truncate(max_intervals.saturating_sub(iv.len()).max(1));
        let mut kids = Vec::with_capacity(2 * split.len());
        for &i in &split {
            let (l, r) = iv[i];
            let m = 0.5 * (l + r);
            if m <= l || m >= r {
                return Ok((sign * q, e, false));
            }
            kids.push((l, m));
            kids.push((m, r));
        }
        let kq = eval(it, &kids)?;
        for (k, &i) in split.iter().enumerate() {
            iv[i] = kids[2 * k];
            qe[i] = kq[2 * k];
            iv.push(kids[2 * k + 1]);
            qe.push(kq[2 * k + 1]);
        }
    }
}

// ---------------------------------------------------------------------------
// Two-dimensional tiled cubature
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Tile {
    u0: f64,
    u1: f64,
    v0: f64,
    v1: f64,
    q: f64,
    e: f64,
}

struct TiledResult {
    q: f64,
    e: f64,
    converged: bool,
    nonfinite: bool,
}

/// Integral over a <= x <= b, c(x) <= y <= d(x) (finite limits).
fn tiled2(
    it: &mut Interp,
    f: &mut F2,
    a: f64,
    b: f64,
    c: &mut L1,
    d: &mut L1,
    abstol: f64,
    reltol: f64,
    max_tiles: usize,
    fname: &str,
) -> R<TiledResult> {
    if !a.is_finite() || !b.is_finite() {
        return Err(err(
            &format!("MATLAB:{}:infiniteLimits", fname),
            format!(
                "{}: Method 'tiled' requires finite limits; use Method 'iterated' for infinite limits.",
                fname
            ),
        ));
    }
    if a == b {
        return Ok(TiledResult {
            q: 0.0,
            e: 0.0,
            converged: true,
            nonfinite: false,
        });
    }
    let (nodes, wk, wg) = gk15();
    let mut eval = |it: &mut Interp, tiles: &mut [Tile]| -> R<()> {
        let k = tiles.len();
        let dims = vec![15, 15 * k];
        let mut xs = vec![0.0; 225 * k];
        let mut wx = vec![0.0; 225 * k];
        let mut vs = vec![0.0; 225 * k];
        // column-major 15 x (15k): row = v index, column = tile*15 + u index
        for (t, tl) in tiles.iter().enumerate() {
            let cu = 0.5 * (tl.u0 + tl.u1);
            let hu = 0.5 * (tl.u1 - tl.u0);
            let cv = 0.5 * (tl.v0 + tl.v1);
            let hv = 0.5 * (tl.v1 - tl.v0);
            for i in 0..15 {
                let (x, dx) = smooth(a, b, cu + hu * nodes[i]);
                for j in 0..15 {
                    let p = (t * 15 + i) * 15 + j;
                    xs[p] = x;
                    wx[p] = dx * hu * hv;
                    vs[p] = cv + hv * nodes[j];
                }
            }
        }
        let lo = c(it, &xs, &dims)?;
        let hi = d(it, &xs, &dims)?;
        let mut ys = vec![0.0; 225 * k];
        let mut w = vec![0.0; 225 * k];
        for p in 0..225 * k {
            if !lo[p].is_finite() || !hi[p].is_finite() {
                return Err(err(
                    &format!("MATLAB:{}:infiniteLimits", fname),
                    format!(
                        "{}: Method 'tiled' requires finite limits; use Method 'iterated' for infinite limits.",
                        fname
                    ),
                ));
            }
            let (y, dy) = smooth(lo[p], hi[p], vs[p]);
            ys[p] = y;
            w[p] = wx[p] * dy;
        }
        let fx = f(it, &xs, &ys, &dims)?;
        for (t, tl) in tiles.iter_mut().enumerate() {
            let mut qk = 0.0;
            let mut qg = 0.0;
            for i in 0..15 {
                for j in 0..15 {
                    let p = (t * 15 + i) * 15 + j;
                    let v = fx[p] * w[p];
                    qk += wk[i] * wk[j] * v;
                    qg += wg[i] * wg[j] * v;
                }
            }
            tl.q = qk;
            tl.e = (qk - qg).abs();
        }
        Ok(())
    };
    let mut tiles: Vec<Tile> = Vec::new();
    for (u0, u1) in [(-1.0, 0.0), (0.0, 1.0)] {
        for (v0, v1) in [(-1.0, 0.0), (0.0, 1.0)] {
            tiles.push(Tile {
                u0,
                u1,
                v0,
                v1,
                q: 0.0,
                e: 0.0,
            });
        }
    }
    eval(it, &mut tiles)?;
    loop {
        let q: f64 = tiles.iter().map(|t| t.q).sum();
        let e: f64 = tiles.iter().map(|t| t.e).sum();
        if !q.is_finite() || !e.is_finite() {
            return Ok(TiledResult {
                q,
                e,
                converged: false,
                nonfinite: true,
            });
        }
        let tol = abstol.max(reltol * q.abs());
        if e <= tol {
            return Ok(TiledResult {
                q,
                e,
                converged: true,
                nonfinite: false,
            });
        }
        if tiles.len() + 3 > max_tiles {
            return Ok(TiledResult {
                q,
                e,
                converged: false,
                nonfinite: false,
            });
        }
        let mut split: Vec<usize> = (0..tiles.len())
            .filter(|&i| {
                let t = &tiles[i];
                t.e > tol * (t.u1 - t.u0) * (t.v1 - t.v0) / 4.0
            })
            .collect();
        split.sort_by(|&i, &j| {
            tiles[j]
                .e
                .partial_cmp(&tiles[i].e)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if split.is_empty() {
            let worst = (0..tiles.len())
                .max_by(|&i, &j| {
                    tiles[i]
                        .e
                        .partial_cmp(&tiles[j].e)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap_or(0);
            split.push(worst);
        }
        let room = (max_tiles - tiles.len()) / 3;
        split.truncate(room.max(1));
        let mut kids = Vec::with_capacity(4 * split.len());
        for &i in &split {
            let t = tiles[i];
            let um = 0.5 * (t.u0 + t.u1);
            let vm = 0.5 * (t.v0 + t.v1);
            if um <= t.u0 || um >= t.u1 || vm <= t.v0 || vm >= t.v1 {
                return Ok(TiledResult {
                    q,
                    e,
                    converged: false,
                    nonfinite: false,
                });
            }
            for (u0, u1) in [(t.u0, um), (um, t.u1)] {
                for (v0, v1) in [(t.v0, vm), (vm, t.v1)] {
                    kids.push(Tile {
                        u0,
                        u1,
                        v0,
                        v1,
                        q: 0.0,
                        e: 0.0,
                    });
                }
            }
        }
        eval(it, &mut kids)?;
        for (k, &i) in split.iter().enumerate() {
            tiles[i] = kids[4 * k];
            tiles.extend_from_slice(&kids[4 * k + 1..4 * k + 4]);
        }
    }
}

/// Iterated (nested 1-D) version of the same integral.
fn iter2(
    it: &mut Interp,
    f: &mut F2,
    a: f64,
    b: f64,
    c: &mut L1,
    d: &mut L1,
    abstol: f64,
    reltol: f64,
    ok: &mut bool,
) -> R<(f64, f64)> {
    let mut outer = |it: &mut Interp, xs: &[f64]| -> R<Vec<f64>> {
        let lo = c(it, xs, &[xs.len(), 1])?;
        let hi = d(it, xs, &[xs.len(), 1])?;
        let mut out = Vec::with_capacity(xs.len());
        for (k, &x) in xs.iter().enumerate() {
            let mut inner = |it: &mut Interp, ys: &[f64]| -> R<Vec<f64>> {
                let xr = vec![x; ys.len()];
                f(it, &xr, ys, &[1, ys.len()])
            };
            let (q, _, conv) = adapt1(it, &mut inner, lo[k], hi[k], abstol, reltol)?;
            if !conv {
                *ok = false;
            }
            out.push(q);
        }
        Ok(out)
    };
    let (q, e, conv) = adapt1(it, &mut outer, a, b, abstol, reltol)?;
    if !conv {
        *ok = false;
    }
    Ok((q, e))
}

// ---------------------------------------------------------------------------
// Front ends
// ---------------------------------------------------------------------------

enum Lim {
    Const(f64),
    Fun(Value),
}

fn limit(v: &Value, fname: &str, what: &str) -> R<Lim> {
    match v {
        Value::Func(_) => Ok(Lim::Fun(v.clone())),
        Value::Num(n) if n.im.is_some() => Err(err(
            &format!("MATLAB:{}:complexLimits", fname),
            format!("{}: complex limits are not supported.", fname),
        )),
        _ => match v.scalar_f64() {
            Some(x) if !v.is_char() => Ok(Lim::Const(x)),
            _ => Err(err(
                &format!("MATLAB:{}:invalidLimit", fname),
                format!(
                    "{}: {} must be a real scalar or a function handle.",
                    fname, what
                ),
            )),
        },
    }
}

fn scalar_limit(v: &Value, fname: &str, what: &str) -> R<f64> {
    match v {
        Value::Num(n) if n.im.is_some() => Err(err(
            &format!("MATLAB:{}:complexLimits", fname),
            format!("{}: complex limits are not supported.", fname),
        )),
        _ if v.is_char() => Err(err(
            &format!("MATLAB:{}:invalidLimit", fname),
            format!("{}: {} must be a real scalar.", fname, what),
        )),
        _ => arg_f64(v, fname),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Method {
    Auto,
    Tiled,
    Iterated,
}

struct Cfg {
    abstol: f64,
    reltol: f64,
    method: Method,
    max_evals: usize,
}

fn parse_opts(a: &[Value], fname: &str, quad2d: bool) -> R<Cfg> {
    let mut cfg = Cfg {
        abstol: if quad2d { 1e-5 } else { 1e-10 },
        reltol: if quad2d { 100.0 * f64::EPSILON } else { 1e-6 },
        method: Method::Auto,
        max_evals: 2000,
    };
    let mut i = 0;
    while i < a.len() {
        let name = arg_str(&a[i], fname)?;
        let Some(v) = a.get(i + 1) else {
            return Err(err(
                &format!("MATLAB:{}:nameValuePairs", fname),
                format!("{}: name-value arguments must come in pairs.", fname),
            ));
        };
        match name.to_ascii_lowercase().as_str() {
            "abstol" => cfg.abstol = arg_f64(v, fname)?,
            "reltol" => cfg.reltol = arg_f64(v, fname)?,
            "method" if !quad2d => {
                cfg.method = match arg_str(v, fname)?.to_ascii_lowercase().as_str() {
                    "auto" => Method::Auto,
                    "tiled" => Method::Tiled,
                    "iterated" => Method::Iterated,
                    other => {
                        return Err(err(
                            &format!("MATLAB:{}:invalidMethod", fname),
                            format!(
                                "{}: invalid Method '{}' (expected 'auto', 'tiled' or 'iterated').",
                                fname, other
                            ),
                        ));
                    }
                }
            }
            "maxfunevals" if quad2d => {
                let x = arg_f64(v, fname)?;
                if !(x >= 1.0) {
                    return Err(err(
                        "MATLAB:quad2d:invalidMaxFunEvals",
                        "quad2d: MaxFunEvals must be a positive integer.",
                    ));
                }
                cfg.max_evals = x.min(1e7) as usize;
            }
            // the singularity-weakening transformation is always applied
            "singular" if quad2d => {
                v.is_true()?;
            }
            "failureplot" if quad2d => {
                if v.is_true()? {
                    return Err(err(
                        "MATLAB:quad2d:unsupportedOption",
                        "quad2d: option 'FailurePlot' is not supported by this engine.",
                    ));
                }
            }
            other => {
                return Err(err(
                    &format!("MATLAB:{}:unknownParameter", fname),
                    format!("{}: unknown parameter '{}'.", fname, other),
                ));
            }
        }
        i += 2;
    }
    if !(cfg.abstol >= 0.0) || !(cfg.reltol >= 0.0) {
        return Err(err(
            &format!("MATLAB:{}:invalidTolerance", fname),
            "Tolerances must be non-negative real scalars.",
        ));
    }
    Ok(cfg)
}

fn call_integrand(
    it: &mut Interp,
    f: &Value,
    args: Vec<Value>,
    n: usize,
    fname: &str,
) -> R<Vec<f64>> {
    let r = call_n(it, f, args, 1, fname)?;
    numeric_out(&r[0], n, fname, "integrand values")
}

fn lim_fn<'a>(
    l: &'a Lim,
    fname: &'a str,
) -> impl FnMut(&mut Interp, &[f64], &[usize]) -> R<Vec<f64>> + 'a {
    move |it: &mut Interp, xs: &[f64], dims: &[usize]| match l {
        Lim::Const(c) => Ok(vec![*c; xs.len()]),
        Lim::Fun(h) => {
            let r = call_n(
                it,
                h,
                vec![Value::matrix(dims.to_vec(), xs.to_vec())],
                1,
                fname,
            )?;
            numeric_out(&r[0], xs.len(), fname, "limit values")
        }
    }
}

fn finish(it: &mut Interp, fname: &str, converged: bool, nonfinite: bool, e: f64, budget: usize) {
    if nonfinite {
        it.warning(
            &format!("MATLAB:{}:nonFiniteValue", fname),
            "Non-finite result. The integration was unsuccessful. Singularity likely.",
        );
    } else if !converged {
        it.warning(
            &format!("MATLAB:{}:maxFunEvalsFail", fname),
            &format!(
                "Reached the maximum number of function evaluations ({}). The result fails the global error test. Approximate error bound {}.",
                budget,
                super::cfmt("%.1e", e)
            ),
        );
    }
}

/// z limit of integral3 at fixed x, as a function of y.
fn zlim_fn<'a>(
    l: &'a Lim,
    x: f64,
    fname: &'a str,
) -> impl FnMut(&mut Interp, &[f64], &[usize]) -> R<Vec<f64>> + 'a {
    move |it: &mut Interp, ys: &[f64], dims: &[usize]| match l {
        Lim::Const(c) => Ok(vec![*c; ys.len()]),
        Lim::Fun(h) => {
            let r = call_n(
                it,
                h,
                vec![
                    Value::matrix(dims.to_vec(), vec![x; ys.len()]),
                    Value::matrix(dims.to_vec(), ys.to_vec()),
                ],
                1,
                fname,
            )?;
            numeric_out(&r[0], ys.len(), fname, "limit values")
        }
    }
}

/// Tiles allowed for `integral2`/`integral3` inner integrals.
const MAX_TILES: usize = 4000;

fn is_inf_const(l: &Lim) -> bool {
    matches!(l, Lim::Const(c) if !c.is_finite())
}

/// `q = integral2(fun, xmin, xmax, ymin, ymax, Name, Value)`; `[q, errbnd]`.
pub(super) fn integral2(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 5, "integral2")?;
    double_entry(it, a, nargout, "integral2")
}

/// `[q, errbnd] = quad2d(fun, a, b, c, d, Name, Value)`
pub(super) fn quad2d(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 5, "quad2d")?;
    double_entry(it, a, nargout, "quad2d")
}

fn double_entry(
    it: &mut Interp,
    a: Vec<Value>,
    nargout: usize,
    fname: &'static str,
) -> R<Vec<Value>> {
    let quad2d = fname == "quad2d";
    let f = a[0].clone();
    super::need_function(&f, fname)?;
    let xa = scalar_limit(&a[1], fname, "XMIN")?;
    let xb = scalar_limit(&a[2], fname, "XMAX")?;
    let yl = limit(&a[3], fname, "YMIN")?;
    let yu = limit(&a[4], fname, "YMAX")?;
    let cfg = parse_opts(&a[5..], fname, quad2d)?;
    let infinite = !xa.is_finite() || !xb.is_finite() || is_inf_const(&yl) || is_inf_const(&yu);
    if quad2d && infinite {
        return Err(err(
            "MATLAB:quad2d:infiniteLimits",
            "quad2d: the integration limits must be finite.",
        ));
    }
    let method = match cfg.method {
        Method::Auto if infinite => Method::Iterated,
        Method::Auto => Method::Tiled,
        m => m,
    };
    let mut fun = |it: &mut Interp, xs: &[f64], ys: &[f64], dims: &[usize]| -> R<Vec<f64>> {
        call_integrand(
            it,
            &f,
            vec![
                Value::matrix(dims.to_vec(), xs.to_vec()),
                Value::matrix(dims.to_vec(), ys.to_vec()),
            ],
            xs.len(),
            fname,
        )
    };
    let mut c = lim_fn(&yl, fname);
    let mut d = lim_fn(&yu, fname);
    let (q, e) = if method == Method::Tiled {
        let budget = if quad2d { cfg.max_evals } else { MAX_TILES };
        let r = tiled2(
            it, &mut fun, xa, xb, &mut c, &mut d, cfg.abstol, cfg.reltol, budget, fname,
        )?;
        finish(it, fname, r.converged, r.nonfinite, r.e, budget * 225);
        (r.q, r.e)
    } else {
        let mut ok = true;
        let (q, e) = iter2(
            it, &mut fun, xa, xb, &mut c, &mut d, cfg.abstol, cfg.reltol, &mut ok,
        )?;
        finish(it, fname, ok, !q.is_finite(), e, 650 * 15);
        (q, e)
    };
    take_n(vec![Value::num(q), Value::num(e)], nargout)
}

/// `q = integral3(fun, xmin, xmax, ymin, ymax, zmin, zmax, Name, Value)`
pub(super) fn integral3(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let fname = "integral3";
    need(&a, 7, fname)?;
    let f = a[0].clone();
    super::need_function(&f, fname)?;
    let xa = scalar_limit(&a[1], fname, "XMIN")?;
    let xb = scalar_limit(&a[2], fname, "XMAX")?;
    let yl = limit(&a[3], fname, "YMIN")?;
    let yu = limit(&a[4], fname, "YMAX")?;
    let zl = limit(&a[5], fname, "ZMIN")?;
    let zu = limit(&a[6], fname, "ZMAX")?;
    let cfg = parse_opts(&a[7..], fname, false)?;
    let inner_inf =
        is_inf_const(&yl) || is_inf_const(&yu) || is_inf_const(&zl) || is_inf_const(&zu);
    let method = match cfg.method {
        Method::Auto if inner_inf => Method::Iterated,
        Method::Auto => Method::Tiled,
        m => m,
    };
    let mut ok = true;
    let mut nonfinite = false;
    let mut outer = |it: &mut Interp, xs: &[f64]| -> R<Vec<f64>> {
        let mut out = Vec::with_capacity(xs.len());
        for &x in xs {
            let mut fun =
                |it: &mut Interp, ys: &[f64], zs: &[f64], dims: &[usize]| -> R<Vec<f64>> {
                    call_integrand(
                        it,
                        &f,
                        vec![
                            Value::matrix(dims.to_vec(), vec![x; ys.len()]),
                            Value::matrix(dims.to_vec(), ys.to_vec()),
                            Value::matrix(dims.to_vec(), zs.to_vec()),
                        ],
                        ys.len(),
                        fname,
                    )
                };
            let ylim = |l: &Lim, it: &mut Interp| -> R<f64> {
                match l {
                    Lim::Const(c) => Ok(*c),
                    Lim::Fun(h) => {
                        let r = call_n(it, h, vec![Value::num(x)], 1, fname)?;
                        Ok(numeric_out(&r[0], 1, fname, "limit values")?[0])
                    }
                }
            };
            let ya = ylim(&yl, it)?;
            let yb = ylim(&yu, it)?;
            let mut c = zlim_fn(&zl, x, fname);
            let mut d = zlim_fn(&zu, x, fname);
            let q = if method == Method::Tiled {
                let r = tiled2(
                    it, &mut fun, ya, yb, &mut c, &mut d, cfg.abstol, cfg.reltol, MAX_TILES, fname,
                )?;
                if !r.converged {
                    ok = false;
                }
                if r.nonfinite {
                    nonfinite = true;
                }
                r.q
            } else {
                iter2(
                    it, &mut fun, ya, yb, &mut c, &mut d, cfg.abstol, cfg.reltol, &mut ok,
                )?
                .0
            };
            out.push(q);
        }
        Ok(out)
    };
    let (q, e, conv) = adapt1(it, &mut outer, xa, xb, cfg.abstol, cfg.reltol)?;
    finish(
        it,
        fname,
        ok && conv,
        nonfinite || !q.is_finite(),
        e,
        MAX_TILES * 225,
    );
    take_n(vec![Value::num(q), Value::num(e)], nargout)
}
