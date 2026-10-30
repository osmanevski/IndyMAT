//! Hypothesis tests and estimators: `ttest`, `ttest2`, `ztest`,
//! `vartest`, `vartest2`, `anova1`, `kruskalwallis`, `normfit`,
//! `ranksum`, `signrank`. MATLAB documentation is the specification.

use indy_value::{Num, Value, bool_array, norm_dims};

use super::dist::{self, Dist};
use super::su::*;
use super::{arg_f64, arg_num, arg_str, err, need, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "ttest", ttest);
    reg(it, "ttest2", ttest2);
    reg(it, "ztest", ztest);
    reg(it, "vartest", vartest);
    reg(it, "vartest2", vartest2);
    reg(it, "normfit", normfit);
    reg(it, "anova1", anova1);
    reg(it, "kruskalwallis", kruskalwallis);
    reg(it, "ranksum", ranksum);
    reg(it, "signrank", signrank);
}

fn d(name: &str) -> &'static Dist {
    dist::find(name).expect("distribution table")
}

/// (lower, upper) tail of a distribution.
fn tails(name: &str, x: f64, p: &[f64]) -> (f64, f64) {
    (d(name).cdf)(x, p)
}

fn quant(name: &str, q: f64, p: &[f64]) -> f64 {
    d(name).quantile(q, p)
}

#[derive(Clone, Copy, PartialEq)]
enum Tail {
    Both,
    Right,
    Left,
}

struct Common {
    alpha: f64,
    tail: Tail,
    dim: Option<usize>,
    vartype: String,
}

fn common(o: &[(String, Value)], fname: &str) -> R<Common> {
    let alpha = match opt(o, "alpha") {
        Some(v) => arg_f64(v, fname)?,
        None => 0.05,
    };
    if !(alpha > 0.0 && alpha < 1.0) {
        return Err(err(
            "stats:ttest:BadAlpha",
            format!("{fname}: 'Alpha' must be a scalar between 0 and 1 exclusive."),
        ));
    }
    let tail = match opt_str(o, "tail", fname)?.as_deref() {
        None | Some("both") => Tail::Both,
        Some("right") => Tail::Right,
        Some("left") => Tail::Left,
        Some(_) => {
            return Err(err(
                "stats:ttest:BadTail",
                format!("{fname}: 'Tail' must be 'both', 'right' or 'left'."),
            ));
        }
    };
    let dim = match opt(o, "dim") {
        Some(v) => Some(count_arg(v, fname, 1.0, 64.0)? - 1),
        None => None,
    };
    let vartype = opt_str(o, "vartype", fname)?.unwrap_or_else(|| "equal".into());
    if vartype != "equal" && vartype != "unequal" {
        return Err(err(
            "stats:ttest2:BadVartype",
            format!("{fname}: 'Vartype' must be 'equal' or 'unequal'."),
        ));
    }
    for (k, _) in o {
        if !["alpha", "tail", "dim", "vartype"].contains(&k.as_str()) {
            return Err(err(
                "stats:InvalidOption",
                format!("{fname}: unknown option '{k}'."),
            ));
        }
    }
    Ok(Common {
        alpha,
        tail,
        dim,
        vartype,
    })
}

/// Slices of `x` along `dim` (default first non-singleton), NaNs kept,
/// plus the shape of one result per slice.
fn slices_of(x: &Num, dim: Option<usize>) -> (Vec<Vec<f64>>, Vec<usize>, usize) {
    let dim = dim.unwrap_or_else(|| super::math::default_dim(&x.dims));
    let mut dims = x.dims.clone();
    while dims.len() <= dim {
        dims.push(1);
    }
    let mut out = Vec::new();
    super::math::slices(&dims, dim, |start, stride, len| {
        out.push((0..len).map(|k| x.re[start + k * stride]).collect());
    });
    dims[dim] = 1;
    (out, norm_dims(dims), dim)
}

/// Two vectors are compared as columns whatever their orientation
/// (unless the operating dimension is given explicitly).
fn as_columns(x: Num, y: Num, dim: Option<usize>) -> (Num, Num) {
    let is_vec = |n: &Num| n.dims.len() == 2 && (n.dims[0] == 1 || n.dims[1] == 1);
    if dim.is_none() && is_vec(&x) && is_vec(&y) {
        let col = |n: Num| Num::real(vec![n.re.len(), 1], n.re);
        (col(x), col(y))
    } else {
        (x, y)
    }
}

fn nonan(v: &[f64]) -> Vec<f64> {
    v.iter().copied().filter(|t| !t.is_nan()).collect()
}

/// Package per-slice results: h, p, ci (2×slices), stat fields.
struct Res {
    h: Vec<bool>,
    p: Vec<f64>,
    lo: Vec<f64>,
    hi: Vec<f64>,
    dims: Vec<usize>,
    dim: usize,
}

impl Res {
    fn new(dims: Vec<usize>, dim: usize) -> Res {
        Res {
            h: vec![],
            p: vec![],
            lo: vec![],
            hi: vec![],
            dims,
            dim,
        }
    }
    fn push(&mut self, p: f64, alpha: f64, lo: f64, hi: f64) {
        self.h.push(if p.is_nan() { false } else { p <= alpha });
        self.p.push(p);
        self.lo.push(lo);
        self.hi.push(hi);
    }
    fn arr(&self, v: Vec<f64>) -> Value {
        Value::Num(Num::real(self.dims.clone(), v))
    }
    fn ci(&self) -> Value {
        let k = self.lo.len();
        let mut dims = self.dims.clone();
        while dims.len() <= self.dim {
            dims.push(1);
        }
        dims[self.dim] = 2;
        let mut data = vec![0.0; 2 * k];
        // 2 entries per slice stored along `dim`
        let stride: usize = dims.iter().take(self.dim).product();
        for s in 0..k {
            let o = s / stride;
            let i = s % stride;
            data[o * stride * 2 + i] = self.lo[s];
            data[o * stride * 2 + i + stride] = self.hi[s];
        }
        Value::Num(Num::real(norm_dims(dims), data))
    }
    fn finish(self, stats: Vec<(&str, Vec<f64>)>) -> Vec<Value> {
        let h = bool_array(self.dims.clone(), self.h.clone());
        let ci = self.ci();
        let fields = stats
            .into_iter()
            .map(|(k, v)| (k, self.arr(v)))
            .collect::<Vec<_>>();
        vec![h, self.arr(self.p.clone()), ci, st(fields)]
    }
}

fn p_from(tail: Tail, name: &str, stat: f64, par: &[f64]) -> f64 {
    if stat.is_nan() || par.iter().any(|v| v.is_nan()) {
        return f64::NAN;
    }
    let (lo, up) = tails(name, stat, par);
    match tail {
        Tail::Both => (2.0 * lo.min(up)).min(1.0),
        Tail::Right => up,
        Tail::Left => lo,
    }
}

/// Interval for a location parameter `est ± q·se` on distribution `name`.
fn interval(tail: Tail, alpha: f64, name: &str, par: &[f64], est: f64, se: f64) -> (f64, f64) {
    match tail {
        Tail::Both => {
            let q = quant(name, 1.0 - alpha / 2.0, par);
            (est - q * se, est + q * se)
        }
        Tail::Right => (est - quant(name, 1.0 - alpha, par) * se, f64::INFINITY),
        Tail::Left => (f64::NEG_INFINITY, est + quant(name, 1.0 - alpha, par) * se),
    }
}

fn ttest(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "ttest")?;
    let x = real_arg(&a[0], "ttest")?;
    let mut k = 1;
    let mut m = 0.0;
    let mut x = x;
    if let Some(v) = a.get(1)
        && !v.is_text()
    {
        let y = real_arg(v, "ttest")?;
        if y.re.len() == 1 {
            m = y.re[0];
        } else {
            if y.dims != x.dims {
                return Err(err(
                    "stats:ttest:InputSizeMismatch",
                    "ttest: X and Y must be the same size for a paired test.",
                ));
            }
            x = Num::real(
                x.dims.clone(),
                x.re.iter().zip(&y.re).map(|(p, q)| p - q).collect(),
            );
        }
        k = 2;
    }
    let o = name_values(&a, k, "ttest")?;
    let c = common(&o, "ttest")?;
    let (sl, dims, dim) = slices_of(&x, c.dim);
    let mut r = Res::new(dims, dim);
    let (mut ts, mut dfs, mut sds) = (vec![], vec![], vec![]);
    for s in &sl {
        let v = nonan(s);
        let n = v.len() as f64;
        let (mu, sd) = (mean(&v), var(&v).sqrt());
        let se = sd / n.sqrt();
        let t = (mu - m) / se;
        let df = n - 1.0;
        let p = if df < 1.0 {
            f64::NAN
        } else if se == 0.0 {
            // degenerate: all observations equal
            if mu == m { f64::NAN } else { 0.0 }
        } else {
            p_from(c.tail, "t", t, &[df])
        };
        let (lo, hi) = if df < 1.0 {
            (f64::NAN, f64::NAN)
        } else {
            let (l, h) = interval(c.tail, c.alpha, "t", &[df], mu, se);
            (l, h)
        };
        r.push(p, c.alpha, lo, hi);
        ts.push(t);
        dfs.push(df);
        sds.push(sd);
    }
    Ok(r.finish(vec![("tstat", ts), ("df", dfs), ("sd", sds)]))
}

fn ttest2(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "ttest2")?;
    let x = real_arg(&a[0], "ttest2")?;
    let y = real_arg(&a[1], "ttest2")?;
    let o = name_values(&a, 2, "ttest2")?;
    let c = common(&o, "ttest2")?;
    let (x, y) = as_columns(x, y, c.dim);
    let (sx, dims, dim) = slices_of(&x, c.dim);
    let (sy, dimsy, _) = slices_of(&y, Some(dim));
    if dims != dimsy {
        return Err(err(
            "stats:ttest2:InputSizeMismatch",
            "ttest2: X and Y must have the same size except along the operating dimension.",
        ));
    }
    let mut r = Res::new(dims, dim);
    let (mut ts, mut dfs) = (vec![], vec![]);
    let mut sds: Vec<f64> = vec![];
    let unequal = c.vartype == "unequal";
    for (u, v) in sx.iter().zip(&sy) {
        let (u, v) = (nonan(u), nonan(v));
        let (nx, ny) = (u.len() as f64, v.len() as f64);
        let (mx, my) = (mean(&u), mean(&v));
        let (vx, vy) = (var(&u), var(&v));
        let (t, df, se) = if unequal {
            let (a1, a2) = (vx / nx, vy / ny);
            let se = (a1 + a2).sqrt();
            let df = (a1 + a2).powi(2) / (a1 * a1 / (nx - 1.0) + a2 * a2 / (ny - 1.0));
            sds.push(vx.sqrt());
            sds.push(vy.sqrt());
            ((mx - my) / se, df, se)
        } else {
            let df = nx + ny - 2.0;
            let sp2 = ((nx - 1.0) * vx + (ny - 1.0) * vy) / df;
            let se = (sp2 * (1.0 / nx + 1.0 / ny)).sqrt();
            sds.push(sp2.sqrt());
            ((mx - my) / se, df, se)
        };
        let p = if df.is_nan() || df < 1.0 {
            f64::NAN
        } else {
            p_from(c.tail, "t", t, &[df])
        };
        let (lo, hi) = interval(c.tail, c.alpha, "t", &[df], mx - my, se);
        r.push(p, c.alpha, lo, hi);
        ts.push(t);
        dfs.push(df);
    }
    let rdims = r.dims.clone();
    let k = sx.len();
    let sd_val = if unequal && k == 1 {
        Value::col(sds)
    } else if unequal {
        mat(2, k, sds)
    } else {
        Value::Num(Num::real(rdims, sds))
    };
    let mut res = r.finish(vec![("tstat", ts), ("df", dfs)]);
    if let Value::Struct(sv) = &mut res[3] {
        sv.set(0, "sd", sd_val);
    }
    Ok(res)
}

fn ztest(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 3, "ztest")?;
    let x = real_arg(&a[0], "ztest")?;
    let m = arg_f64(&a[1], "ztest")?;
    let sigma = arg_f64(&a[2], "ztest")?;
    if sigma.is_nan() || sigma <= 0.0 {
        return Err(err(
            "stats:ztest:BadSigma",
            "ztest: SIGMA must be a positive scalar.",
        ));
    }
    let o = name_values(&a, 3, "ztest")?;
    let c = common(&o, "ztest")?;
    let (sl, dims, dim) = slices_of(&x, c.dim);
    let mut r = Res::new(dims, dim);
    let mut zs = vec![];
    for s in &sl {
        let v = nonan(s);
        let n = v.len() as f64;
        let mu = mean(&v);
        let se = sigma / n.sqrt();
        let z = (mu - m) / se;
        let p = p_from(c.tail, "norm", z, &[0.0, 1.0]);
        let (lo, hi) = interval(c.tail, c.alpha, "norm", &[0.0, 1.0], mu, se);
        r.push(p, c.alpha, lo, hi);
        zs.push(z);
    }
    let zv = r.arr(zs);
    let mut out = r.finish(vec![]);
    out[3] = zv;
    Ok(out)
}

fn var_interval(tail: Tail, alpha: f64, s2: f64, df: f64) -> (f64, f64) {
    let hi_q = |a: f64| quant("chi2", a, &[df]);
    match tail {
        Tail::Both => (
            df * s2 / hi_q(1.0 - alpha / 2.0),
            df * s2 / hi_q(alpha / 2.0),
        ),
        Tail::Right => (df * s2 / hi_q(1.0 - alpha), f64::INFINITY),
        Tail::Left => (0.0, df * s2 / hi_q(alpha)),
    }
}

fn vartest(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "vartest")?;
    let x = real_arg(&a[0], "vartest")?;
    let v0 = arg_f64(&a[1], "vartest")?;
    if v0.is_nan() || v0 <= 0.0 {
        return Err(err(
            "stats:vartest:BadVariance",
            "vartest: V must be a positive scalar.",
        ));
    }
    let o = name_values(&a, 2, "vartest")?;
    let c = common(&o, "vartest")?;
    let (sl, dims, dim) = slices_of(&x, c.dim);
    let mut r = Res::new(dims, dim);
    let (mut chis, mut dfs) = (vec![], vec![]);
    for s in &sl {
        let v = nonan(s);
        let df = v.len() as f64 - 1.0;
        let s2 = var(&v);
        let chi = df * s2 / v0;
        let p = p_from(c.tail, "chi2", chi, &[df]);
        let (lo, hi) = var_interval(c.tail, c.alpha, s2, df);
        r.push(p, c.alpha, lo, hi);
        chis.push(chi);
        dfs.push(df);
    }
    Ok(r.finish(vec![("chisqstat", chis), ("df", dfs)]))
}

fn vartest2(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "vartest2")?;
    let x = real_arg(&a[0], "vartest2")?;
    let y = real_arg(&a[1], "vartest2")?;
    let o = name_values(&a, 2, "vartest2")?;
    let c = common(&o, "vartest2")?;
    let (x, y) = as_columns(x, y, c.dim);
    let (sx, dims, dim) = slices_of(&x, c.dim);
    let (sy, _, _) = slices_of(&y, Some(dim));
    let mut r = Res::new(dims, dim);
    let (mut fs, mut d1s, mut d2s) = (vec![], vec![], vec![]);
    for (u, v) in sx.iter().zip(&sy) {
        let (u, v) = (nonan(u), nonan(v));
        let (d1, d2) = (u.len() as f64 - 1.0, v.len() as f64 - 1.0);
        let f = var(&u) / var(&v);
        let p = p_from(c.tail, "f", f, &[d1, d2]);
        let (lo, hi) = match c.tail {
            Tail::Both => (
                f / quant("f", 1.0 - c.alpha / 2.0, &[d1, d2]),
                f / quant("f", c.alpha / 2.0, &[d1, d2]),
            ),
            Tail::Right => (f / quant("f", 1.0 - c.alpha, &[d1, d2]), f64::INFINITY),
            Tail::Left => (0.0, f / quant("f", c.alpha, &[d1, d2])),
        };
        r.push(p, c.alpha, lo, hi);
        fs.push(f);
        d1s.push(d1);
        d2s.push(d2);
    }
    Ok(r.finish(vec![("fstat", fs), ("df1", d1s), ("df2", d2s)]))
}

fn normfit(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "normfit")?;
    let x = real_arg(&a[0], "normfit")?;
    let alpha = match a.get(1) {
        Some(v) if !v.is_empty() => arg_f64(v, "normfit")?,
        _ => 0.05,
    };
    if !(alpha > 0.0 && alpha < 1.0) {
        return Err(err(
            "stats:normfit:BadAlpha",
            "normfit: ALPHA must be a scalar between 0 and 1 exclusive.",
        ));
    }
    // normfit(x, alpha, censoring, freq, options) beyond alpha is not supported
    if a.len() > 2 && a[2..].iter().any(|v| !v.is_empty()) {
        return Err(err(
            "stats:normfit:Unsupported",
            "normfit: censoring, frequency and options arguments are not supported.",
        ));
    }
    let xm = if x.dims.len() == 2 && (x.dims[0] == 1 || x.dims[1] == 1) {
        M {
            r: x.re.len(),
            c: 1,
            d: x.re.clone(),
        }
    } else {
        M::from_num(&x, "normfit")?
    };
    let k = xm.c;
    let (mut mu, mut sg) = (vec![], vec![]);
    let (mut muci, mut sgci) = (vec![0.0; 2 * k], vec![0.0; 2 * k]);
    for j in 0..k {
        let v = nonan(xm.col(j));
        let n = v.len() as f64;
        let (m, s) = (mean(&v), var(&v).sqrt());
        mu.push(m);
        sg.push(s);
        let df = n - 1.0;
        let q = quant("t", 1.0 - alpha / 2.0, &[df]);
        muci[2 * j] = m - q * s / n.sqrt();
        muci[2 * j + 1] = m + q * s / n.sqrt();
        sgci[2 * j] = s * (df / quant("chi2", 1.0 - alpha / 2.0, &[df])).sqrt();
        sgci[2 * j + 1] = s * (df / quant("chi2", alpha / 2.0, &[df])).sqrt();
    }
    Ok(vec![
        Value::row(mu),
        Value::row(sg),
        mat(2, k, muci),
        mat(2, k, sgci),
    ])
}

// ---------------------------------------------------------------------------
// one-way ANOVA / Kruskal-Wallis
// ---------------------------------------------------------------------------

struct Groups {
    data: Vec<Vec<f64>>,
    names: Vec<String>,
    matrix: bool,
}

fn groups(a: &[Value], fname: &str) -> R<Groups> {
    need(a, 1, fname)?;
    let y = real_arg(&a[0], fname)?;
    let g = a.get(1).filter(|v| !v.is_empty());
    match g {
        None => {
            let ym = M::from_num(&y, fname)?;
            let (r, c) = if ym.r == 1 { (ym.c, 1) } else { (ym.r, ym.c) };
            let d: Vec<Vec<f64>> = (0..c).map(|j| nonan(&ym.d[j * r..(j + 1) * r])).collect();
            Ok(Groups {
                data: d,
                names: (1..=c).map(|i| i.to_string()).collect(),
                matrix: true,
            })
        }
        Some(gv) => {
            let n = y.re.len();
            let (idx, names): (Vec<usize>, Vec<String>) = match gv {
                Value::Cell(_) | Value::Str(_) | Value::Char(_) => {
                    let strs: Vec<String> = match gv {
                        Value::Cell(c) => {
                            c.data.iter().map(|e| arg_str(e, fname)).collect::<R<_>>()?
                        }
                        Value::Str(_) => indy_value::to_string_array(gv)
                            .data
                            .iter()
                            .map(|s| s.clone().unwrap_or_default())
                            .collect(),
                        _ => gv.char_rows().unwrap_or_default(),
                    };
                    let mut u = strs.clone();
                    u.sort();
                    u.dedup();
                    (
                        strs.iter()
                            .map(|s| u.binary_search(s).unwrap_or(0))
                            .collect(),
                        u,
                    )
                }
                _ => {
                    let gn = arg_num(gv, fname)?;
                    if gn.re.iter().any(|t| t.is_nan()) || gn.re.is_empty() {
                        return Err(err(
                            "stats:anova1:BadGroup",
                            format!("{fname}: GROUP must be non-empty and free of NaN."),
                        ));
                    }
                    let mut u: Vec<f64> = gn.re.iter().copied().filter(|t| !t.is_nan()).collect();
                    u.sort_by(f64::total_cmp);
                    u.dedup();
                    (
                        gn.re.iter().map(|t| u.partition_point(|q| q < t)).collect(),
                        u.iter().map(|t| format!("{t}")).collect(),
                    )
                }
            };
            if idx.len() != n {
                return Err(err(
                    "stats:anova1:InputSizeMismatch",
                    format!("{fname}: GROUP must have one entry per observation."),
                ));
            }
            let mut data = vec![Vec::new(); names.len()];
            for (i, &gi) in idx.iter().enumerate() {
                if !y.re[i].is_nan() {
                    data[gi].push(y.re[i]);
                }
            }
            Ok(Groups {
                data,
                names,
                matrix: false,
            })
        }
    }
}

fn display_off(a: &[Value], at: usize, fname: &str) -> R<()> {
    if let Some(v) = a.get(at) {
        let s = arg_str(v, fname)?.to_lowercase();
        if s != "on" && s != "off" {
            return Err(err(
                "stats:anova1:BadDisplayOpt",
                format!("{fname}: DISPLAYOPT must be 'on' or 'off'."),
            ));
        }
    }
    Ok(())
}

fn tbl(rows: Vec<Vec<Value>>) -> Value {
    let r = rows.len();
    let c = rows[0].len();
    let mut data = Vec::with_capacity(r * c);
    for j in 0..c {
        for row in &rows {
            data.push(row[j].clone());
        }
    }
    cell(vec![r, c], data)
}

fn anova1(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let g = groups(&a, "anova1")?;
    display_off(&a, 2, "anova1")?;
    let keep: Vec<usize> = (0..g.data.len())
        .filter(|&i| !g.data[i].is_empty())
        .collect();
    let k = keep.len() as f64;
    let n: f64 = keep.iter().map(|&i| g.data[i].len() as f64).sum();
    let all: Vec<f64> = keep.iter().flat_map(|&i| g.data[i].clone()).collect();
    let gm = mean(&all);
    let ssb: f64 = keep
        .iter()
        .map(|&i| g.data[i].len() as f64 * (mean(&g.data[i]) - gm).powi(2))
        .sum();
    let sse: f64 = keep
        .iter()
        .map(|&i| {
            let m = mean(&g.data[i]);
            g.data[i].iter().map(|v| (v - m).powi(2)).sum::<f64>()
        })
        .sum();
    let (dfb, dfe) = (k - 1.0, n - k);
    let (msb, mse) = (ssb / dfb, sse / dfe);
    let f = msb / mse;
    let p = if dfb < 1.0 || dfe < 1.0 {
        f64::NAN
    } else {
        tails("f", f, &[dfb, dfe]).1
    };
    let e = Value::empty();
    let nv = Value::num;
    let table = tbl(vec![
        ["Source", "SS", "df", "MS", "F", "Prob>F"]
            .iter()
            .map(|s| strv(s))
            .collect(),
        vec![
            strv(if g.matrix { "Columns" } else { "Groups" }),
            nv(ssb),
            nv(dfb),
            nv(msb),
            nv(f),
            nv(p),
        ],
        vec![
            strv("Error"),
            nv(sse),
            nv(dfe),
            nv(mse),
            e.clone(),
            e.clone(),
        ],
        vec![
            strv("Total"),
            nv(ssb + sse),
            nv(n - 1.0),
            e.clone(),
            e.clone(),
            e,
        ],
    ]);
    let stats = st(vec![
        (
            "gnames",
            cell(
                vec![keep.len(), 1],
                keep.iter().map(|&i| strv(&g.names[i])).collect(),
            ),
        ),
        (
            "n",
            Value::row(keep.iter().map(|&i| g.data[i].len() as f64).collect()),
        ),
        ("source", strv("anova1")),
        (
            "means",
            Value::row(keep.iter().map(|&i| mean(&g.data[i])).collect()),
        ),
        ("df", nv(dfe)),
        ("s", nv(mse.sqrt())),
    ]);
    Ok(vec![nv(p), table, stats])
}

fn kruskalwallis(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let g = groups(&a, "kruskalwallis")?;
    display_off(&a, 2, "kruskalwallis")?;
    let keep: Vec<usize> = (0..g.data.len())
        .filter(|&i| !g.data[i].is_empty())
        .collect();
    let all: Vec<f64> = keep.iter().flat_map(|&i| g.data[i].clone()).collect();
    let n = all.len() as f64;
    let k = keep.len() as f64;
    let (rk, _) = ranks(&all);
    let rbar = (n + 1.0) / 2.0;
    let mut off = 0;
    let (mut ssb, mut means, mut ns) = (0.0, vec![], vec![]);
    for &i in &keep {
        let m = g.data[i].len();
        let rm = mean(&rk[off..off + m]);
        ssb += m as f64 * (rm - rbar).powi(2);
        means.push(rm);
        ns.push(m as f64);
        off += m;
    }
    let sst: f64 = rk.iter().map(|r| (r - rbar).powi(2)).sum();
    let chi = if sst == 0.0 {
        f64::NAN
    } else {
        (n - 1.0) * ssb / sst
    };
    let (dfb, dfe) = (k - 1.0, n - k);
    let p = tails("chi2", chi, &[dfb]).1;
    let e = Value::empty();
    let nv = Value::num;
    let table = tbl(vec![
        ["Source", "SS", "df", "MS", "Chi-sq", "Prob>Chi-sq"]
            .iter()
            .map(|s| strv(s))
            .collect(),
        vec![
            strv(if g.matrix { "Columns" } else { "Groups" }),
            nv(ssb),
            nv(dfb),
            nv(ssb / dfb),
            nv(chi),
            nv(p),
        ],
        vec![
            strv("Error"),
            nv(sst - ssb),
            nv(dfe),
            nv((sst - ssb) / dfe),
            e.clone(),
            e.clone(),
        ],
        vec![strv("Total"), nv(sst), nv(n - 1.0), e.clone(), e.clone(), e],
    ]);
    let stats = st(vec![
        (
            "gnames",
            cell(
                vec![keep.len(), 1],
                keep.iter().map(|&i| strv(&g.names[i])).collect(),
            ),
        ),
        ("n", Value::row(ns)),
        ("source", strv("kruskalwallis")),
        ("meanranks", Value::row(means)),
        ("sumt", nv(0.0)),
    ]);
    Ok(vec![nv(p), table, stats])
}

// ---------------------------------------------------------------------------
// rank tests
// ---------------------------------------------------------------------------

/// Exact distribution of the rank sum of a sample of size `m` among
/// `n_tot` untied ranks: counts[s] = number of subsets with sum s.
fn rank_sum_counts(m: usize, n_tot: usize) -> Vec<f64> {
    let max = n_tot * (n_tot + 1) / 2;
    // dp[j][s]: subsets of size j with sum s
    let mut dp = vec![vec![0.0f64; max + 1]; m + 1];
    dp[0][0] = 1.0;
    for r in 1..=n_tot {
        for j in (1..=m.min(r)).rev() {
            for s in (r..=max).rev() {
                dp[j][s] += dp[j - 1][s - r];
            }
        }
    }
    dp[m].clone()
}

fn ranksum(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "ranksum")?;
    let x = nonan(&real_arg(&a[0], "ranksum")?.re);
    let y = nonan(&real_arg(&a[1], "ranksum")?.re);
    let o = name_values(&a, 2, "ranksum")?;
    let alpha = match opt(&o, "alpha") {
        Some(v) => arg_f64(v, "ranksum")?,
        None => 0.05,
    };
    let tail = match opt_str(&o, "tail", "ranksum")?.as_deref() {
        None | Some("both") => Tail::Both,
        Some("right") => Tail::Right,
        Some("left") => Tail::Left,
        Some(_) => {
            return Err(err(
                "stats:ranksum:BadTail",
                "ranksum: 'Tail' must be 'both', 'right' or 'left'.",
            ));
        }
    };
    let (m, n) = (x.len(), y.len());
    if m == 0 || n == 0 {
        return Err(err(
            "stats:ranksum:NotEnoughData",
            "ranksum: X and Y must contain at least one non-NaN value.",
        ));
    }
    let all: Vec<f64> = x.iter().chain(&y).copied().collect();
    let (rk, tie) = ranks(&all);
    let w: f64 = rk[..m].iter().sum();
    let method = opt_str(&o, "method", "ranksum")?;
    let exact = match method.as_deref() {
        Some("exact") => true,
        Some("approximate") => false,
        None => m.min(n) <= 10 && m + n < 20 && tie == 0.0,
        Some(_) => {
            return Err(err(
                "stats:ranksum:BadMethod",
                "ranksum: 'Method' must be 'exact' or 'approximate'.",
            ));
        }
    };
    if exact && tie != 0.0 {
        return Err(err(
            "stats:ranksum:ExactWithTies",
            "ranksum: the exact method needs data without ties.",
        ));
    }
    let nt = (m + n) as f64;
    let (p, zval) = if exact {
        let counts = rank_sum_counts(m, m + n);
        let total: f64 = counts.iter().sum();
        let ws = w.round() as usize;
        let le: f64 = counts[..=ws].iter().sum::<f64>() / total;
        let ge: f64 = counts[ws..].iter().sum::<f64>() / total;
        let p = match tail {
            Tail::Both => (2.0 * le.min(ge)).min(1.0),
            Tail::Left => le,
            Tail::Right => ge,
        };
        (p, f64::NAN)
    } else {
        let mu = m as f64 * (nt + 1.0) / 2.0;
        let sg = (m as f64 * n as f64 / 12.0 * ((nt + 1.0) - tie / (nt * (nt - 1.0)))).sqrt();
        let diff = w - mu;
        let cc = match tail {
            Tail::Both => 0.5 * diff.signum(),
            Tail::Right => 0.5,
            Tail::Left => -0.5,
        };
        let z = if diff == 0.0 { 0.0 } else { (diff - cc) / sg };
        let (lo, up) = tails("norm", z, &[0.0, 1.0]);
        let p = match tail {
            Tail::Both => (2.0 * lo.min(up)).min(1.0),
            Tail::Left => lo,
            Tail::Right => up,
        };
        (p, z)
    };
    let mut fields = vec![("ranksum", Value::num(w))];
    if !exact {
        fields.insert(0, ("zval", Value::num(zval)));
    }
    Ok(vec![Value::num(p), Value::bool(p <= alpha), st(fields)])
}

fn signrank(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "signrank")?;
    let x = real_arg(&a[0], "signrank")?;
    let mut k = 1;
    let mut diffs: Vec<f64> = x.re.clone();
    if let Some(v) = a.get(1)
        && !v.is_text()
    {
        let y = real_arg(v, "signrank")?;
        if y.re.len() == 1 {
            let m = y.re[0];
            diffs.iter_mut().for_each(|t| *t -= m);
        } else {
            if y.re.len() != x.re.len() {
                return Err(err(
                    "stats:signrank:InputSizeMismatch",
                    "signrank: X and Y must have the same number of elements.",
                ));
            }
            diffs = x.re.iter().zip(&y.re).map(|(p, q)| p - q).collect();
        }
        k = 2;
    }
    let o = name_values(&a, k, "signrank")?;
    let alpha = match opt(&o, "alpha") {
        Some(v) => arg_f64(v, "signrank")?,
        None => 0.05,
    };
    let tail = match opt_str(&o, "tail", "signrank")?.as_deref() {
        None | Some("both") => Tail::Both,
        Some("right") => Tail::Right,
        Some("left") => Tail::Left,
        Some(_) => {
            return Err(err(
                "stats:signrank:BadTail",
                "signrank: 'Tail' must be 'both', 'right' or 'left'.",
            ));
        }
    };
    if diffs.iter().all(|t| t.is_nan()) {
        return Err(err(
            "stats:signrank:NotEnoughData",
            "signrank: X must contain at least one non-NaN value.",
        ));
    }
    let diffs: Vec<f64> = diffs
        .into_iter()
        .filter(|t| !t.is_nan() && *t != 0.0)
        .collect();
    let n = diffs.len();
    if n == 0 {
        return Ok(vec![
            Value::num(1.0),
            Value::bool(false),
            st(vec![("signedrank", Value::num(0.0))]),
        ]);
    }
    let abs: Vec<f64> = diffs.iter().map(|t| t.abs()).collect();
    let (rk, tie) = ranks(&abs);
    let wp: f64 = diffs
        .iter()
        .zip(&rk)
        .filter(|(d, _)| **d > 0.0)
        .map(|(_, r)| *r)
        .sum();
    let method = opt_str(&o, "method", "signrank")?;
    let exact = match method.as_deref() {
        Some("exact") => true,
        Some("approximate") => false,
        None => n <= 15 && tie == 0.0,
        Some(_) => {
            return Err(err(
                "stats:signrank:BadMethod",
                "signrank: 'Method' must be 'exact' or 'approximate'.",
            ));
        }
    };
    if exact && tie != 0.0 {
        return Err(err(
            "stats:signrank:ExactWithTies",
            "signrank: the exact method needs data without ties.",
        ));
    }
    let (p, zval) = if exact {
        let max = n * (n + 1) / 2;
        let mut cnt = vec![0.0f64; max + 1];
        cnt[0] = 1.0;
        for r in 1..=n {
            for s in (r..=max).rev() {
                cnt[s] += cnt[s - r];
            }
        }
        let total = 2f64.powi(n as i32);
        let ws = wp.round() as usize;
        let le: f64 = cnt[..=ws].iter().sum::<f64>() / total;
        let ge: f64 = cnt[ws..].iter().sum::<f64>() / total;
        let p = match tail {
            Tail::Both => (2.0 * le.min(ge)).min(1.0),
            Tail::Left => le,
            Tail::Right => ge,
        };
        (p, f64::NAN)
    } else {
        let nf = n as f64;
        let mu = nf * (nf + 1.0) / 4.0;
        let sg = (nf * (nf + 1.0) * (2.0 * nf + 1.0) / 24.0 - tie / 48.0).sqrt();
        let diff = wp - mu;
        let cc = match tail {
            Tail::Both => 0.5 * diff.signum(),
            Tail::Right => 0.5,
            Tail::Left => -0.5,
        };
        let z = if diff == 0.0 { 0.0 } else { (diff - cc) / sg };
        let (lo, up) = tails("norm", z, &[0.0, 1.0]);
        let p = match tail {
            Tail::Both => (2.0 * lo.min(up)).min(1.0),
            Tail::Left => lo,
            Tail::Right => up,
        };
        (p, z)
    };
    let mut fields = vec![("signedrank", Value::num(wp))];
    if !exact {
        fields.insert(0, ("zval", Value::num(zval)));
    }
    Ok(vec![Value::num(p), Value::bool(p <= alpha), st(fields)])
}
