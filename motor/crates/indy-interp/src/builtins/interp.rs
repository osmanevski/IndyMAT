//! Interpolation and piecewise polynomials: `interp1`, `interp2`,
//! `spline`, `pchip`, `makima`, `mkpp`, `unmkpp`, `ppval`.
//!
//! Own code from the textbook definitions (cubic Hermite form with
//! not-a-knot / clamped end conditions, Fritsch–Carlson slopes, modified
//! Akima weights). Piecewise polynomials use the MATLAB `pp` struct layout:
//! `coefs` is `(prod(dim)*pieces) x order`, row `comp + prod(dim)*piece`,
//! highest power first, local variable `x - breaks(piece)`.

use indy_value::{Num, NumClass, StructArray, Value};

use super::{arg_f64, arg_num, arg_str, err, need, one, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "mkpp", mkpp);
    reg(it, "unmkpp", unmkpp);
    reg(it, "ppval", ppval);
    reg(it, "spline", |_, a, _| fit_fn(&a, "spline"));
    reg(it, "pchip", |_, a, _| fit_fn(&a, "pchip"));
    reg(it, "makima", |_, a, _| fit_fn(&a, "makima"));
    reg(it, "interp1", interp1);
    reg(it, "interp2", interp2);
}

// ---------------------------------------------------------------------------
// pp representation
// ---------------------------------------------------------------------------

/// Piecewise polynomial for `d` value components.
struct Pp {
    breaks: Vec<f64>,
    /// `coefs[comp][piece]` = coefficients, highest power first.
    coefs: Vec<Vec<Vec<f64>>>,
    order: usize,
    dim: Vec<usize>,
}

impl Pp {
    fn pieces(&self) -> usize {
        self.breaks.len() - 1
    }
    fn d(&self) -> usize {
        self.coefs.len()
    }
    fn piece_of(&self, x: f64) -> usize {
        let p = self.pieces();
        // last k with breaks[k] <= x (clamped to a valid piece)
        let k = self.breaks[1..p].partition_point(|b| *b <= x);
        k.min(p - 1)
    }
    fn eval(&self, comp: usize, x: f64) -> f64 {
        if x.is_nan() {
            return f64::NAN;
        }
        let k = self.piece_of(x);
        let t = x - self.breaks[k];
        self.coefs[comp][k].iter().fold(0.0, |acc, c| acc * t + c)
    }
    fn to_value(&self) -> Value {
        let (p, d, o) = (self.pieces(), self.d(), self.order);
        let rows = p * d;
        let mut c = vec![0.0; rows * o];
        for comp in 0..d {
            for k in 0..p {
                for j in 0..o {
                    c[(comp + d * k) + j * rows] = self.coefs[comp][k][j];
                }
            }
        }
        let mut s = StructArray::new_scalar();
        s.set(0, "form", Value::str("pp"));
        s.set(0, "breaks", Value::row(self.breaks.clone()));
        s.set(0, "coefs", Value::matrix(vec![rows, o], c));
        s.set(0, "pieces", Value::num(p as f64));
        s.set(0, "order", Value::num(o as f64));
        s.set(
            0,
            "dim",
            Value::row(self.dim.iter().map(|x| *x as f64).collect()),
        );
        Value::Struct(s)
    }
    fn from_value(v: &Value, fname: &str) -> R<Pp> {
        let bad = |m: &str| err("MATLAB:ppval:InvalidPP", format!("{}: {}", fname, m));
        let Value::Struct(s) = v else {
            return Err(bad("first argument must be a piecewise polynomial."));
        };
        let g = |f: &str| {
            s.get(0, f)
                .cloned()
                .ok_or_else(|| bad(&format!("missing field {}.", f)))
        };
        let breaks = g("breaks")?.to_f64_vec().unwrap_or_default();
        let coefs = g("coefs")?.to_num().unwrap_or_else(Num::empty);
        let pieces = g("pieces")?.scalar_f64().unwrap_or(0.0) as usize;
        let order = g("order")?.scalar_f64().unwrap_or(0.0) as usize;
        let dim: Vec<usize> = g("dim")?
            .to_f64_vec()
            .unwrap_or_default()
            .iter()
            .map(|x| *x as usize)
            .collect();
        let dim = if dim.is_empty() { vec![1] } else { dim };
        let d: usize = dim.iter().product();
        if pieces == 0 || breaks.len() != pieces + 1 || coefs.re.len() != d * pieces * order {
            return Err(bad("inconsistent piecewise polynomial."));
        }
        let rows = d * pieces;
        let mut out = vec![vec![vec![0.0; order]; pieces]; d];
        for comp in 0..d {
            for k in 0..pieces {
                for j in 0..order {
                    out[comp][k][j] = coefs.re[(comp + d * k) + j * rows];
                }
            }
        }
        Ok(Pp {
            breaks,
            coefs: out,
            order,
            dim,
        })
    }
}

fn dims_vec(v: &Value, fname: &str) -> R<Vec<usize>> {
    let x = arg_num(v, fname)?.re;
    if x.is_empty() || x.iter().any(|e| *e < 1.0 || e.fract() != 0.0) {
        return Err(err(
            "MATLAB:mkpp:invalidD",
            format!("{}: D must be a vector of positive integers.", fname),
        ));
    }
    Ok(x.iter().map(|e| *e as usize).collect())
}

fn mkpp(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "mkpp")?;
    let breaks = arg_num(&a[0], "mkpp")?.re;
    let coefs = arg_num(&a[1], "mkpp")?;
    if breaks.len() < 2 {
        return Err(err(
            "MATLAB:mkpp:InvalidBreaks",
            "mkpp: at least two breaks are required.",
        ));
    }
    if breaks
        .windows(2)
        .any(|w| w[1].partial_cmp(&w[0]) != Some(std::cmp::Ordering::Greater))
    {
        return Err(err(
            "MATLAB:mkpp:InvalidBreaks",
            "mkpp: breaks must be strictly increasing.",
        ));
    }
    let pieces = breaks.len() - 1;
    let dim = match a.get(2) {
        Some(v) => dims_vec(v, "mkpp")?,
        None => vec![1],
    };
    let d: usize = dim.iter().product();
    let total = coefs.re.len();
    if total == 0 || total % (pieces * d) != 0 {
        return Err(err(
            "MATLAB:mkpp:CoefsBreaksMismatch",
            "mkpp: coefficients do not match the breaks.",
        ));
    }
    let order = total / (pieces * d);
    // Reject real pp with complex coefficients explicitly (not modelled).
    if coefs.im.is_some() {
        return Err(err(
            "MATLAB:unsupported",
            "mkpp: complex coefficients are not supported.",
        ));
    }
    let rows = pieces * d;
    let mut s = StructArray::new_scalar();
    s.set(0, "form", Value::str("pp"));
    s.set(0, "breaks", Value::row(breaks));
    s.set(0, "coefs", Value::matrix(vec![rows, order], coefs.re));
    s.set(0, "pieces", Value::num(pieces as f64));
    s.set(0, "order", Value::num(order as f64));
    s.set(
        0,
        "dim",
        Value::row(dim.iter().map(|x| *x as f64).collect()),
    );
    one(Value::Struct(s))
}

fn unmkpp(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "unmkpp")?;
    let pp = Pp::from_value(&a[0], "unmkpp")?;
    let Value::Struct(s) = &a[0] else {
        unreachable!()
    };
    let out = vec![
        Value::row(pp.breaks.clone()),
        s.get(0, "coefs").cloned().unwrap_or_default(),
        Value::num(pp.pieces() as f64),
        Value::num(pp.order as f64),
        if pp.dim.len() == 1 {
            Value::num(pp.dim[0] as f64)
        } else {
            Value::row(pp.dim.iter().map(|x| *x as f64).collect())
        },
    ];
    Ok(out.into_iter().take(nargout.max(1)).collect())
}

/// Evaluate `pp` at every element of `x`; shape `[dim..., size(x)]`
/// (`size(x)` only when `dim` is 1).
fn pp_eval_value(pp: &Pp, x: &Num) -> Value {
    let d = pp.d();
    let mut out = Vec::with_capacity(x.re.len() * d);
    for &xv in &x.re {
        for comp in 0..d {
            out.push(pp.eval(comp, xv));
        }
    }
    let scalar_dim = pp.dim.len() == 1 && pp.dim[0] == 1;
    if scalar_dim {
        return Value::Num(Num::real(x.dims.clone(), out));
    }
    let mut dims = pp.dim.clone();
    let xvec = x.dims.len() == 2 && (x.dims[0] == 1 || x.dims[1] == 1);
    if xvec {
        dims.push(x.re.len());
    } else {
        dims.extend_from_slice(&x.dims);
    }
    Value::Num(Num::real(dims, out))
}

fn ppval(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "ppval")?;
    let pp = Pp::from_value(&a[0], "ppval")?;
    let x = arg_num(&a[1], "ppval")?;
    one(pp_eval_value(&pp, &x))
}

// ---------------------------------------------------------------------------
// Fitting (one component at a time)
// ---------------------------------------------------------------------------

/// Cubic Hermite pieces from slopes.
fn hermite(x: &[f64], y: &[f64], s: &[f64]) -> Vec<Vec<f64>> {
    (0..x.len() - 1)
        .map(|i| {
            let h = x[i + 1] - x[i];
            let del = (y[i + 1] - y[i]) / h;
            vec![
                (s[i] + s[i + 1] - 2.0 * del) / (h * h),
                (3.0 * del - 2.0 * s[i] - s[i + 1]) / h,
                s[i],
                y[i],
            ]
        })
        .collect()
}

/// Solve a tridiagonal system (Thomas algorithm).
fn tridiag(lo: &[f64], di: &[f64], up: &[f64], rhs: &[f64]) -> Vec<f64> {
    let n = di.len();
    let mut c = vec![0.0; n];
    let mut d = vec![0.0; n];
    let mut denom = di[0];
    c[0] = if n > 1 { up[0] / denom } else { 0.0 };
    d[0] = rhs[0] / denom;
    for i in 1..n {
        denom = di[i] - lo[i - 1] * c[i - 1];
        if i < n - 1 {
            c[i] = up[i] / denom;
        }
        d[i] = (rhs[i] - lo[i - 1] * d[i - 1]) / denom;
    }
    let mut x = vec![0.0; n];
    x[n - 1] = d[n - 1];
    for i in (0..n - 1).rev() {
        x[i] = d[i] - c[i] * x[i + 1];
    }
    x
}

/// Spline pieces. `y` has `n` values (not-a-knot) or `n + 2` (clamped).
fn spline_pieces(x: &[f64], y: &[f64]) -> (Vec<f64>, Vec<Vec<f64>>) {
    let n = x.len();
    let clamped = y.len() == n + 2;
    let (ends, y) = if clamped {
        (Some((y[0], y[n + 1])), &y[1..=n])
    } else {
        (None, y)
    };
    let h: Vec<f64> = x.windows(2).map(|w| w[1] - w[0]).collect();
    let del: Vec<f64> = (0..n - 1).map(|i| (y[i + 1] - y[i]) / h[i]).collect();
    if ends.is_none() && n == 2 {
        return (x.to_vec(), vec![vec![del[0], y[0]]]);
    }
    if ends.is_none() && n == 3 {
        // single parabola through three points, in Newton form around x0
        let c = (del[1] - del[0]) / (x[2] - x[0]);
        let b = del[0] - c * h[0];
        return (vec![x[0], x[2]], vec![vec![c, b, y[0]]]);
    }
    let mut lo = vec![0.0; n - 1];
    let mut di = vec![0.0; n];
    let mut up = vec![0.0; n - 1];
    let mut rhs = vec![0.0; n];
    for i in 1..n - 1 {
        lo[i - 1] = h[i];
        di[i] = 2.0 * (h[i - 1] + h[i]);
        up[i] = h[i - 1];
        rhs[i] = 3.0 * (h[i] * del[i - 1] + h[i - 1] * del[i]);
    }
    match ends {
        Some((s0, sn)) => {
            di[0] = 1.0;
            up[0] = 0.0;
            rhs[0] = s0;
            di[n - 1] = 1.0;
            lo[n - 2] = 0.0;
            rhs[n - 1] = sn;
        }
        None => {
            di[0] = h[1];
            up[0] = h[0] + h[1];
            rhs[0] = ((h[0] + 2.0 * up[0]) * h[1] * del[0] + h[0] * h[0] * del[1]) / up[0];
            let (a, b) = (h[n - 2], h[n - 3]);
            di[n - 1] = b;
            lo[n - 2] = a + b;
            rhs[n - 1] = (a * a * del[n - 3] + (2.0 * (a + b) + a) * b * del[n - 2]) / (a + b);
        }
    }
    // row 0 : [h1, h0+h1] on (s0, s1); row n-1 : [h_{n-2}+h_{n-3}, h_{n-3}]
    let s = tridiag(&lo, &di, &up, &rhs);
    (x.to_vec(), hermite(x, y, &s))
}

fn sgn(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

fn pchip_slopes(x: &[f64], y: &[f64]) -> Vec<f64> {
    let n = x.len();
    let h: Vec<f64> = x.windows(2).map(|w| w[1] - w[0]).collect();
    let del: Vec<f64> = (0..n - 1).map(|i| (y[i + 1] - y[i]) / h[i]).collect();
    if n == 2 {
        return vec![del[0]; 2];
    }
    let mut s = vec![0.0; n];
    for k in 1..n - 1 {
        if del[k - 1] * del[k] > 0.0 {
            let w1 = 2.0 * h[k] + h[k - 1];
            let w2 = h[k] + 2.0 * h[k - 1];
            s[k] = (w1 + w2) / (w1 / del[k - 1] + w2 / del[k]);
        }
    }
    let end = |h0: f64, h1: f64, d0: f64, d1: f64| {
        let mut v = ((2.0 * h0 + h1) * d0 - h0 * d1) / (h0 + h1);
        if sgn(v) != sgn(d0) {
            v = 0.0;
        } else if sgn(d0) != sgn(d1) && v.abs() > 3.0 * d0.abs() {
            v = 3.0 * d0;
        }
        v
    };
    s[0] = end(h[0], h[1], del[0], del[1]);
    s[n - 1] = end(h[n - 2], h[n - 3], del[n - 2], del[n - 3]);
    s
}

fn makima_slopes(x: &[f64], y: &[f64]) -> Vec<f64> {
    let n = x.len();
    let del: Vec<f64> = (0..n - 1)
        .map(|i| (y[i + 1] - y[i]) / (x[i + 1] - x[i]))
        .collect();
    if n == 2 {
        return vec![del[0]; 2];
    }
    if n == 3 {
        // Akima end rules degenerate; MATLAB uses the same extension.
    }
    let left = 2.0 * del[0] - del[1];
    let right = 2.0 * del[n - 2] - del[n - 3];
    let mut m = Vec::with_capacity(n + 3);
    m.push(2.0 * left - del[0]);
    m.push(left);
    m.extend_from_slice(&del);
    m.push(right);
    m.push(2.0 * right - del[n - 2]);
    (0..n)
        .map(|i| {
            let w1 = (m[i + 3] - m[i + 2]).abs() + (m[i + 3] + m[i + 2]).abs() / 2.0;
            let w2 = (m[i + 1] - m[i]).abs() + (m[i + 1] + m[i]).abs() / 2.0;
            let tot = w1 + w2;
            if tot > 0.0 {
                (w1 * m[i + 1] + w2 * m[i + 2]) / tot
            } else {
                0.0
            }
        })
        .collect()
}

/// Build a pp for `method` from one column.
fn build_pieces(method: &str, x: &[f64], y: &[f64]) -> (Vec<f64>, Vec<Vec<f64>>) {
    match method {
        "spline" => spline_pieces(x, y),
        "pchip" => {
            let s = pchip_slopes(x, y);
            (x.to_vec(), hermite(x, y, &s))
        }
        _ => {
            let s = makima_slopes(x, y);
            (x.to_vec(), hermite(x, y, &s))
        }
    }
}

fn check_sites(x: &[f64], fname: &str) -> R<()> {
    if x.iter().any(|v| !v.is_finite()) {
        return Err(err(
            "MATLAB:chckxy:nonFiniteX",
            format!("{}: sample points must be finite.", fname),
        ));
    }
    if x.len() < 2 {
        return Err(err(
            "MATLAB:chckxy:NotEnoughPts",
            format!("{}: there should be at least two data points.", fname),
        ));
    }
    Ok(())
}

/// Sort sites ascending (permutation returned); error on duplicates.
fn sort_sites(x: &[f64], fname: &str) -> R<(Vec<f64>, Vec<usize>)> {
    let mut idx: Vec<usize> = (0..x.len()).collect();
    idx.sort_by(|&a, &b| x[a].partial_cmp(&x[b]).unwrap_or(std::cmp::Ordering::Equal));
    let xs: Vec<f64> = idx.iter().map(|&i| x[i]).collect();
    if xs.windows(2).any(|w| w[1] == w[0]) {
        return Err(err(
            "MATLAB:chckxy:RepeatedSites",
            format!("{}: sample points must be distinct.", fname),
        ));
    }
    Ok((xs, idx))
}

/// `spline`/`pchip`/`makima` (x, y [, xq]): `y` is a vector or has the
/// sample dimension last.
fn fit_fn(a: &[Value], fname: &str) -> R<Vec<Value>> {
    need(a, 2, fname)?;
    if a.len() > 3 {
        return Err(err(
            "MATLAB:TooManyInputs",
            format!("Too many input arguments (function '{}').", fname),
        ));
    }
    let x = arg_num(&a[0], fname)?;
    let y = arg_num(&a[1], fname)?;
    if x.im.is_some() || y.im.is_some() {
        return Err(err(
            "MATLAB:unsupported",
            format!("{}: complex data is not supported.", fname),
        ));
    }
    let n = x.re.len();
    check_sites(&x.re, fname)?;
    let yvec = y.dims.len() == 2 && (y.dims[0] == 1 || y.dims[1] == 1);
    let (lead, ny): (Vec<usize>, usize) = if yvec {
        (vec![1], y.re.len())
    } else {
        (
            y.dims[..y.dims.len() - 1].to_vec(),
            *y.dims.last().unwrap_or(&0),
        )
    };
    let extra_ok = fname == "spline" && ny == n + 2;
    if ny != n && !extra_ok {
        let m = if fname == "spline" && ny == n + 1 {
            "The number of sample points is incompatible with the number of values."
        } else {
            "The number of sample points must match the number of values."
        };
        return Err(err("MATLAB:chckxy:NumSitesMismatchValues", m));
    }
    if fname != "spline" && ny != n {
        return Err(err(
            "MATLAB:chckxy:NumSitesMismatchValues",
            "The number of sample points must match the number of values.",
        ));
    }
    let (xs, order) = sort_sites(&x.re, fname)?;
    let d: usize = lead.iter().product();
    let mut coefs = Vec::with_capacity(d);
    let mut breaks = xs.clone();
    for comp in 0..d {
        // y(comp, :) = y.re[comp + d*j]
        let col: Vec<f64> = (0..ny).map(|j| y.re[comp + d * j]).collect();
        let col = if ny == n {
            order.iter().map(|&i| col[i]).collect()
        } else {
            // clamped: keep the end slopes in place
            let mut c = vec![col[0]];
            c.extend(order.iter().map(|&i| col[i + 1]));
            c.push(col[n + 1]);
            c
        };
        let (b, p) = build_pieces(fname, &xs, &col);
        breaks = b;
        coefs.push(p);
    }
    let order_k = coefs[0][0].len();
    let pp = Pp {
        breaks,
        coefs,
        order: order_k,
        dim: lead,
    };
    if a.len() == 3 {
        let xq = arg_num(&a[2], fname)?;
        return one(pp_eval_value(&pp, &xq));
    }
    one(pp.to_value())
}

// ---------------------------------------------------------------------------
// interp1
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Method {
    Linear,
    Nearest,
    Next,
    Previous,
    Pchip,
    Spline,
    Makima,
}

fn parse_method(s: &str) -> Option<Method> {
    Some(match s.to_ascii_lowercase().as_str() {
        "linear" => Method::Linear,
        "nearest" => Method::Nearest,
        "next" => Method::Next,
        "previous" => Method::Previous,
        "pchip" | "cubic" => Method::Pchip,
        "spline" => Method::Spline,
        "makima" => Method::Makima,
        _ => return None,
    })
}

enum Extrap {
    Nan,
    Value(f64),
    Extrapolate,
}

/// Evaluate a direct (non-pp) method at `q` on sorted sites.
fn direct_eval(m: Method, x: &[f64], y: &[f64], q: f64, extrap: &Extrap) -> f64 {
    let n = x.len();
    if q.is_nan() {
        return f64::NAN;
    }
    let outside = q < x[0] || q > x[n - 1];
    if outside {
        match extrap {
            Extrap::Nan => return f64::NAN,
            Extrap::Value(v) => return *v,
            Extrap::Extrapolate => {}
        }
    }
    // k = index of the last site <= q (clamped to [0, n-2] for intervals)
    let ge = x.partition_point(|s| *s <= q);
    let k = ge.saturating_sub(1).min(n - 2);
    match m {
        Method::Linear => {
            let t = (q - x[k]) / (x[k + 1] - x[k]);
            if t == 1.0 {
                return y[k + 1];
            }
            y[k] + t * (y[k + 1] - y[k])
        }
        Method::Nearest => {
            if q <= x[0] {
                return y[0];
            }
            if q >= x[n - 1] {
                return y[n - 1];
            }
            // midpoint rounds up
            if q - x[k] < x[k + 1] - q {
                y[k]
            } else {
                y[k + 1]
            }
        }
        Method::Previous => {
            if q < x[0] {
                return f64::NAN;
            }
            y[ge - 1]
        }
        Method::Next => {
            if q > x[n - 1] {
                return f64::NAN;
            }
            let lt = x.partition_point(|s| *s < q);
            y[lt]
        }
        _ => unreachable!(),
    }
}

fn interp1(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let mut pos: Vec<&Value> = Vec::new();
    let mut method = Method::Linear;
    let mut method_given = false;
    let mut extrap: Option<Extrap> = None;
    let mut pp_out = false;
    for v in &a {
        if v.is_text() {
            let s = arg_str(v, "interp1")?;
            match s.to_ascii_lowercase().as_str() {
                "extrap" => extrap = Some(Extrap::Extrapolate),
                "pp" => pp_out = true,
                other => match parse_method(other) {
                    Some(m) if !method_given => {
                        method = m;
                        method_given = true;
                    }
                    _ => {
                        return Err(err(
                            "MATLAB:interp1:InvalidMethod",
                            format!("interp1: invalid interpolation method '{}'.", s),
                        ));
                    }
                },
            }
        } else if method_given && pos.len() >= 2 {
            // scalar extrapolation value after the method
            extrap = Some(Extrap::Value(arg_f64(v, "interp1")?));
        } else {
            pos.push(v);
        }
    }
    let (xv, vv, qv): (Option<&Value>, &Value, Option<&Value>) = match (pos.len(), pp_out) {
        (1, true) => (None, pos[0], None),
        (2, true) => (Some(pos[0]), pos[1], None),
        (2, false) => (None, pos[0], Some(pos[1])),
        (3, false) => (Some(pos[0]), pos[1], Some(pos[2])),
        _ => {
            return Err(err(
                "MATLAB:minrhs",
                "Not enough input arguments (function 'interp1').",
            ));
        }
    };
    let v = arg_num(vv, "interp1")?;
    if v.im.is_some() {
        return Err(err(
            "MATLAB:unsupported",
            "interp1: complex sample values are not supported.",
        ));
    }
    let vvec = v.dims.len() == 2 && (v.dims[0] == 1 || v.dims[1] == 1);
    let n = if vvec { v.re.len() } else { v.dims[0] };
    let m = v.re.len().checked_div(n).unwrap_or(0);
    let xs_raw: Vec<f64> = match xv {
        Some(x) => {
            let x = arg_num(x, "interp1")?;
            if x.re.len() != n {
                return Err(err(
                    "MATLAB:interp1:YInvalidNumRows",
                    "interp1: the number of sample points must match the number of values.",
                ));
            }
            x.re
        }
        None => (1..=n).map(|i| i as f64).collect(),
    };
    if n < 2 {
        return Err(err(
            "MATLAB:interp1:NotEnoughPts",
            "interp1: there should be at least two data points.",
        ));
    }
    if xs_raw.iter().any(|x| x.is_nan()) {
        return Err(err(
            "MATLAB:interp1:NaNinX",
            "interp1: sample points must not contain NaN.",
        ));
    }
    let mut ord: Vec<usize> = (0..n).collect();
    ord.sort_by(|&p, &q| {
        xs_raw[p]
            .partial_cmp(&xs_raw[q])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let xs: Vec<f64> = ord.iter().map(|&i| xs_raw[i]).collect();
    let dup = xs.windows(2).any(|w| w[1] == w[0]);
    if dup
        && !matches!(
            method,
            Method::Linear | Method::Nearest | Method::Next | Method::Previous
        )
    {
        return Err(err(
            "MATLAB:interp1:RepeatedValuesX",
            "interp1: sample points must be distinct.",
        ));
    }
    let col = |c: usize| -> Vec<f64> { ord.iter().map(|&i| v.re[i + n * c]).collect() };
    let pp_method = matches!(method, Method::Pchip | Method::Spline | Method::Makima);
    if pp_out {
        let name = match method {
            Method::Pchip => "pchip",
            Method::Spline => "spline",
            Method::Makima => "makima",
            _ => {
                return Err(err(
                    "MATLAB:interp1:ppUnsupported",
                    "interp1: 'pp' output requires spline, pchip or makima.",
                ));
            }
        };
        let mut coefs = Vec::new();
        let mut breaks = xs.clone();
        for c in 0..m {
            let (b, p) = build_pieces(name, &xs, &col(c));
            breaks = b;
            coefs.push(p);
        }
        let order = coefs[0][0].len();
        let dim: Vec<usize> = if vvec { vec![1] } else { v.dims[1..].to_vec() };
        return one(Pp {
            breaks,
            coefs,
            order,
            dim,
        }
        .to_value());
    }
    let q = arg_num(qv.unwrap_or(vv), "interp1")?;
    let extrap = extrap.unwrap_or(if pp_method {
        Extrap::Extrapolate
    } else {
        Extrap::Nan
    });
    let nq = q.re.len();
    let mut out = vec![0.0; nq * m];
    for c in 0..m {
        let y = col(c);
        if pp_method {
            let name = match method {
                Method::Pchip => "pchip",
                Method::Spline => "spline",
                _ => "makima",
            };
            let (b, p) = build_pieces(name, &xs, &y);
            let pp = Pp {
                order: p[0].len(),
                breaks: b,
                coefs: vec![p],
                dim: vec![1],
            };
            for (i, &qq) in q.re.iter().enumerate() {
                let out_of = qq < xs[0] || qq > xs[n - 1];
                out[i + nq * c] = if out_of && let Extrap::Value(e) = extrap {
                    e
                } else if out_of && matches!(extrap, Extrap::Nan) {
                    f64::NAN
                } else {
                    pp.eval(0, qq)
                };
            }
        } else {
            for (i, &qq) in q.re.iter().enumerate() {
                out[i + nq * c] = direct_eval(method, &xs, &y, qq, &extrap);
            }
        }
    }
    let qvec = q.dims.len() == 2 && (q.dims[0] == 1 || q.dims[1] == 1);
    let dims: Vec<usize> = if vvec {
        q.dims.clone()
    } else {
        let mut d = if qvec { vec![nq] } else { q.dims.clone() };
        d.extend_from_slice(&v.dims[1..]);
        d
    };
    let mut res = Num::real(dims, out);
    if v.class == NumClass::Single {
        res = res.with_class(NumClass::Single);
    }
    one(Value::Num(res))
}

// ---------------------------------------------------------------------------
// interp2 (linear / nearest)
// ---------------------------------------------------------------------------

fn interp2(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let mut nums: Vec<&Value> = Vec::new();
    let mut method = "linear".to_string();
    let mut extrap: Option<f64> = None;
    for (i, v) in a.iter().enumerate() {
        if v.is_text() {
            method = arg_str(v, "interp2")?.to_ascii_lowercase();
        } else if !method.is_empty()
            && i > 0
            && nums.len() >= 3
            && a[..i].iter().any(|p| p.is_text())
        {
            extrap = Some(arg_f64(v, "interp2")?);
        } else {
            nums.push(v);
        }
    }
    if !matches!(method.as_str(), "linear" | "nearest") {
        return Err(err(
            "MATLAB:unsupported",
            format!(
                "interp2: method '{}' is not supported (linear, nearest).",
                method
            ),
        ));
    }
    let (xv, yv, zv, xq, yq) = match nums.len() {
        3 => (None, None, nums[0], nums[1], nums[2]),
        5 => (Some(nums[0]), Some(nums[1]), nums[2], nums[3], nums[4]),
        _ => {
            return Err(err(
                "MATLAB:minrhs",
                "Not enough input arguments (function 'interp2').",
            ));
        }
    };
    let z = arg_num(zv, "interp2")?;
    if z.dims.len() != 2 || z.im.is_some() {
        return Err(err(
            "MATLAB:unsupported",
            "interp2: V must be a real 2-D matrix.",
        ));
    }
    let (nr, nc) = (z.dims[0], z.dims[1]);
    // grid vectors from vectors or meshgrid matrices
    let grid = |v: Option<&Value>, rows: bool, len: usize| -> R<Vec<f64>> {
        match v {
            None => Ok((1..=len).map(|i| i as f64).collect()),
            Some(v) => {
                let g = arg_num(v, "interp2")?;
                if g.re.len() == len {
                    Ok(g.re)
                } else if g.re.len() == nr * nc && g.dims.len() == 2 {
                    Ok(if rows {
                        (0..len).map(|i| g.re[i * nr]).collect() // x varies along columns
                    } else {
                        (0..len).map(|i| g.re[i]).collect()
                    })
                } else {
                    Err(err(
                        "MATLAB:interp2:XYsizeMismatch",
                        "interp2: X and Y must match the size of V.",
                    ))
                }
            }
        }
    };
    let xs = grid(xv, true, nc)?;
    let ys = grid(yv, false, nr)?;
    for g in [&xs, &ys] {
        if g.windows(2)
            .any(|w| w[1].partial_cmp(&w[0]) != Some(std::cmp::Ordering::Greater))
        {
            return Err(err(
                "MATLAB:interp2:NonMonotonic",
                "interp2: grid vectors must be strictly increasing.",
            ));
        }
    }
    let xq = arg_num(xq, "interp2")?;
    let yq = arg_num(yq, "interp2")?;
    // vector query pair of different orientation -> meshgrid
    let (xqs, yqs, dims) = if xq.re.len() == yq.re.len() && xq.dims == yq.dims {
        (xq.re.clone(), yq.re.clone(), xq.dims.clone())
    } else if xq.dims.iter().filter(|d| **d > 1).count() <= 1
        && yq.dims.iter().filter(|d| **d > 1).count() <= 1
    {
        let (p, q) = (xq.re.len(), yq.re.len());
        let mut xa = Vec::with_capacity(p * q);
        let mut ya = Vec::with_capacity(p * q);
        for i in 0..p {
            for j in 0..q {
                xa.push(xq.re[i]);
                ya.push(yq.re[j]);
            }
        }
        // meshgrid layout is q x p (rows follow y): transpose the order
        let mut xo = vec![0.0; p * q];
        let mut yo = vec![0.0; p * q];
        for i in 0..p {
            for j in 0..q {
                xo[j + q * i] = xa[i * q + j];
                yo[j + q * i] = ya[i * q + j];
            }
        }
        (xo, yo, vec![q, p])
    } else {
        return Err(err(
            "MATLAB:interp2:sizeMismatch",
            "interp2: XI and YI must have the same size, or be vectors.",
        ));
    };
    let fill = extrap.unwrap_or(f64::NAN);
    let at = |i: usize, j: usize| z.re[i + nr * j];
    let find = |g: &[f64], q: f64| -> usize {
        g.partition_point(|s| *s <= q)
            .saturating_sub(1)
            .min(g.len() - 2)
    };
    let out: Vec<f64> = xqs
        .iter()
        .zip(&yqs)
        .map(|(&px, &py)| {
            if px.is_nan()
                || py.is_nan()
                || px < xs[0]
                || px > xs[nc - 1]
                || py < ys[0]
                || py > ys[nr - 1]
            {
                return fill;
            }
            if nr < 2 || nc < 2 {
                return f64::NAN;
            }
            let (j, i) = (find(&xs, px), find(&ys, py));
            let tx = (px - xs[j]) / (xs[j + 1] - xs[j]);
            let ty = (py - ys[i]) / (ys[i + 1] - ys[i]);
            if method == "nearest" {
                let jj = if tx >= 0.5 { j + 1 } else { j };
                let ii = if ty >= 0.5 { i + 1 } else { i };
                return at(ii, jj);
            }
            let top = at(i, j) * (1.0 - tx) + at(i, j + 1) * tx;
            let bot = at(i + 1, j) * (1.0 - tx) + at(i + 1, j + 1) * tx;
            if tx == 0.0 && ty == 0.0 {
                return at(i, j);
            }
            top * (1.0 - ty) + bot * ty
        })
        .collect();
    one(Value::Num(Num::real(dims, out)))
}
