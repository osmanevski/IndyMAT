//! Parameter estimation and goodness-of-fit helpers: `<dist>stat`,
//! `expfit`/`poissfit`/`binofit`/`unifit`/`lognfit`/`wblfit`/`gamfit`,
//! `kstest`, `kstest2`, `chi2gof`, `ksdensity`, `bootstrp`,
//! `datasample`, `mahal`, `knnsearch`, `clusterdata`, `cophenet`,
//! `silhouette`.

use indy_value::{Num, Value, bool_array};

use super::dist::{self, Dist};
use super::specfun::lgamma;
use super::su::*;
use super::{arg_f64, err, need, one, reg};
use crate::{Interp, R};

macro_rules! stat_fns {
    ($it:expr, $( $k:literal => $d:expr ),* $(,)?) => {
        $( reg($it, concat!($k, "stat"), |_, a, _| dist_stat($d, &a, concat!($k, "stat"))); )*
    };
}

pub fn register(it: &mut Interp) {
    stat_fns!(it,
        "norm" => "norm", "t" => "t", "chi2" => "chi2", "f" => "f", "exp" => "exp",
        "gam" => "gam", "beta" => "beta", "bino" => "bino", "poiss" => "poiss",
        "unif" => "unif", "unid" => "unid", "geo" => "geo", "nbin" => "nbin",
        "logn" => "logn", "wbl" => "wbl", "ev" => "ev", "rayl" => "rayl",
    );
    reg(it, "expfit", expfit);
    reg(it, "poissfit", poissfit);
    reg(it, "binofit", binofit);
    reg(it, "unifit", unifit);
    reg(it, "lognfit", lognfit);
    reg(it, "wblfit", |_, a, n| mle_fit(&a, n, "wbl", "wblfit"));
    reg(it, "gamfit", |_, a, n| mle_fit(&a, n, "gam", "gamfit"));
    reg(it, "kstest", kstest);
    reg(it, "kstest2", kstest2);
    reg(it, "chi2gof", chi2gof);
    reg(it, "ksdensity", ksdensity);
    reg(it, "bootstrp", bootstrp);
    reg(it, "datasample", datasample);
    reg(it, "mahal", mahal);
    reg(it, "knnsearch", knnsearch);
    reg(it, "clusterdata", clusterdata);
    reg(it, "cophenet", cophenet);
    reg(it, "silhouette", silhouette);
}

fn d(key: &str) -> &'static Dist {
    dist::find(key).expect("distribution table")
}

fn quant(key: &str, q: f64, p: &[f64]) -> f64 {
    d(key).quantile(q, p)
}

// ---------------------------------------------------------------------------
// <dist>stat
// ---------------------------------------------------------------------------

fn dist_stat(key: &str, a: &[Value], fname: &str) -> R<Vec<Value>> {
    let dd = d(key);
    let np = dd.defaults.len();
    need(a, np, fname)?;
    let ps: Vec<Num> = a[..np]
        .iter()
        .map(|v| real_arg(v, fname))
        .collect::<R<_>>()?;
    let refs: Vec<&Num> = ps.iter().collect();
    let (dims, n) = super::specfun::broadcast(&refs, fname)?;
    let (mut m, mut v) = (Vec::with_capacity(n), Vec::with_capacity(n));
    let mut buf = vec![0.0; np];
    for i in 0..n {
        for (k, p) in ps.iter().enumerate() {
            buf[k] = super::specfun::at(p, i);
        }
        if (dd.valid)(&buf) {
            let (mm, vv) = (dd.moments)(&buf);
            m.push(mm);
            v.push(vv);
        } else {
            m.push(f64::NAN);
            v.push(f64::NAN);
        }
    }
    Ok(vec![
        Value::Num(Num::real(dims.clone(), m)),
        Value::Num(Num::real(dims, v)),
    ])
}

// ---------------------------------------------------------------------------
// *fit
// ---------------------------------------------------------------------------

fn alpha_arg(v: Option<&Value>, fname: &str) -> R<f64> {
    let a = match v {
        Some(v) if !v.is_empty() => arg_f64(v, fname)?,
        _ => 0.05,
    };
    if !(a > 0.0 && a < 1.0) {
        return Err(err(
            "stats:fit:BadAlpha",
            format!("{fname}: ALPHA must be a scalar between 0 and 1 exclusive."),
        ));
    }
    Ok(a)
}

fn clean(v: &Value, fname: &str) -> R<Vec<f64>> {
    let x = real_arg(v, fname)?;
    let c: Vec<f64> = x.re.iter().copied().filter(|t| !t.is_nan()).collect();
    if c.is_empty() {
        return Err(err(
            "stats:fit:NotEnoughData",
            format!("{fname}: X must contain at least one non-NaN value."),
        ));
    }
    Ok(c)
}

fn expfit(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "expfit")?;
    let v = clean(&a[0], "expfit")?;
    if v.iter().any(|t| *t < 0.0) {
        return Err(err(
            "stats:expfit:BadData",
            "expfit: X must contain nonnegative values.",
        ));
    }
    let alpha = alpha_arg(a.get(1), "expfit")?;
    let (n, s) = (v.len() as f64, v.iter().sum::<f64>());
    let ci = vec![
        2.0 * s / quant("chi2", 1.0 - alpha / 2.0, &[2.0 * n]),
        2.0 * s / quant("chi2", alpha / 2.0, &[2.0 * n]),
    ];
    Ok(vec![Value::num(s / n), Value::col(ci)])
}

fn poissfit(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "poissfit")?;
    let v = clean(&a[0], "poissfit")?;
    if v.iter().any(|t| *t < 0.0 || t.fract() != 0.0) {
        return Err(err(
            "stats:poissfit:BadData",
            "poissfit: X must contain nonnegative integers.",
        ));
    }
    let alpha = alpha_arg(a.get(1), "poissfit")?;
    let (n, s) = (v.len() as f64, v.iter().sum::<f64>());
    let lo = if s == 0.0 {
        0.0
    } else {
        0.5 * quant("chi2", alpha / 2.0, &[2.0 * s]) / n
    };
    let hi = 0.5 * quant("chi2", 1.0 - alpha / 2.0, &[2.0 * s + 2.0]) / n;
    Ok(vec![Value::num(s / n), Value::col(vec![lo, hi])])
}

fn binofit(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "binofit")?;
    let x = real_arg(&a[0], "binofit")?;
    let nn = real_arg(&a[1], "binofit")?;
    let alpha = alpha_arg(a.get(2), "binofit")?;
    let (dims, n) = super::specfun::broadcast(&[&x, &nn], "binofit")?;
    let (mut ph, mut ci) = (Vec::new(), Vec::new());
    for i in 0..n {
        let (xi, ni) = (super::specfun::at(&x, i), super::specfun::at(&nn, i));
        if xi < 0.0 || xi > ni || xi.fract() != 0.0 || ni.fract() != 0.0 || ni < 1.0 {
            return Err(err(
                "stats:binofit:InvalidData",
                "binofit: X must be integers between 0 and N, and N a positive integer.",
            ));
        }
        ph.push(xi / ni);
        ci.push(if xi == 0.0 {
            0.0
        } else {
            quant("beta", alpha / 2.0, &[xi, ni - xi + 1.0])
        });
        ci.push(if xi == ni {
            1.0
        } else {
            quant("beta", 1.0 - alpha / 2.0, &[xi + 1.0, ni - xi])
        });
    }
    // ci is n-by-2
    let k = ph.len();
    let mut m = vec![0.0; 2 * k];
    for i in 0..k {
        m[i] = ci[2 * i];
        m[i + k] = ci[2 * i + 1];
    }
    Ok(vec![Value::Num(Num::real(dims, ph)), mat(k, 2, m)])
}

fn unifit(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "unifit")?;
    if nargout > 2 {
        return Err(err(
            "stats:unifit:Unsupported",
            "unifit: confidence intervals are not supported.",
        ));
    }
    let v = clean(&a[0], "unifit")?;
    Ok(vec![
        Value::num(v.iter().copied().fold(f64::INFINITY, f64::min)),
        Value::num(v.iter().copied().fold(f64::NEG_INFINITY, f64::max)),
    ])
}

fn lognfit(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "lognfit")?;
    let v = clean(&a[0], "lognfit")?;
    if v.iter().any(|t| *t <= 0.0) {
        return Err(err(
            "stats:lognfit:BadData",
            "lognfit: X must contain positive values.",
        ));
    }
    let alpha = alpha_arg(a.get(1), "lognfit")?;
    let l: Vec<f64> = v.iter().map(|t| t.ln()).collect();
    let (n, m, s) = (l.len() as f64, mean(&l), var(&l).sqrt());
    let mut res = vec![Value::row(vec![m, s])];
    if nargout > 1 {
        let df = n - 1.0;
        let q = quant("t", 1.0 - alpha / 2.0, &[df]);
        let (lo_s, hi_s) = (
            s * (df / quant("chi2", 1.0 - alpha / 2.0, &[df])).sqrt(),
            s * (df / quant("chi2", alpha / 2.0, &[df])).sqrt(),
        );
        res.push(mat(
            2,
            2,
            vec![m - q * s / n.sqrt(), m + q * s / n.sqrt(), lo_s, hi_s],
        ));
    }
    Ok(res)
}

fn mle_fit(a: &[Value], nargout: usize, key: &str, fname: &str) -> R<Vec<Value>> {
    need(a, 1, fname)?;
    if nargout > 1 || a.len() > 1 {
        return Err(err(
            "stats:fit:Unsupported",
            format!("{fname}: confidence intervals, censoring and options are not supported."),
        ));
    }
    let v = clean(&a[0], fname)?;
    let p = super::regr::estimate(key, &v)?;
    one(Value::row(p))
}

// ---------------------------------------------------------------------------
// Kolmogorov-Smirnov
// ---------------------------------------------------------------------------

/// P(D_n < d), Marsaglia-Tsang-Wang matrix method (exact); the closed
/// approximation of the same paper in the far tail.
fn ks_cdf_exact(n: usize, d: f64) -> f64 {
    let nf = n as f64;
    let s = nf * d * d;
    if d <= 0.0 {
        return 0.0;
    }
    if d >= 1.0 {
        return 1.0;
    }
    if s > 7.24 || (s > 3.76 && n > 99) {
        return 1.0 - 2.0 * (-(2.000_071 + 0.331 / nf.sqrt() + 1.409 / nf) * s).exp();
    }
    if n > 1000 {
        // Kolmogorov limit distribution with the standard n-correction
        let l = (nf.sqrt() + 0.12 + 0.11 / nf.sqrt()) * d;
        return 1.0 - kolmogorov_q(l);
    }
    let k = (nf * d) as usize + 1;
    let m = 2 * k - 1;
    let h = k as f64 - nf * d;
    let mut hm = M::zeros(m, m);
    for i in 0..m {
        for j in 0..m {
            hm.set(i, j, if i + 1 >= j { 1.0 } else { 0.0 });
        }
    }
    for i in 0..m {
        let v = hm.at(i, 0) - h.powi(i as i32 + 1);
        hm.set(i, 0, v);
        let w = hm.at(m - 1, i) - h.powi((m - i) as i32);
        hm.set(m - 1, i, w);
    }
    if 2.0 * h - 1.0 > 0.0 {
        let v = hm.at(m - 1, 0) + (2.0 * h - 1.0).powi(m as i32);
        hm.set(m - 1, 0, v);
    }
    for i in 0..m {
        for j in 0..m {
            if i + 1 > j {
                let mut f = 1.0;
                for g in 1..=(i + 1 - j) {
                    f *= g as f64;
                }
                let v = hm.at(i, j) / f;
                hm.set(i, j, v);
            }
        }
    }
    // power by repeated squaring with exponent scaling to avoid overflow
    let mut eq = 0i32;
    let (q, e) = mat_pow(&hm, n, &mut eq);
    let _ = e;
    let mut s = q.at(k - 1, k - 1);
    let mut ex = eq;
    for i in 1..=n {
        s = s * i as f64 / nf;
        if s < 1e-140 {
            s *= 1e140;
            ex -= 140;
        }
    }
    s * 10f64.powi(ex)
}

fn mm(a: &M, b: &M) -> M {
    let m = a.r;
    let mut c = M::zeros(m, m);
    for i in 0..m {
        for j in 0..m {
            let mut s = 0.0;
            for k in 0..m {
                s += a.at(i, k) * b.at(k, j);
            }
            c.set(i, j, s);
        }
    }
    c
}

/// A^e with the result rescaled by powers of ten tracked in `ex`.
fn mat_pow(a: &M, e: usize, ex: &mut i32) -> (M, i32) {
    if e == 1 {
        return (a.clone(), 0);
    }
    let mut eb = 0;
    let (b, _) = mat_pow(a, e / 2, &mut eb);
    let mut c = mm(&b, &b);
    let mut ec = 2 * eb;
    if e % 2 == 1 {
        c = mm(a, &c);
    }
    let big = c.d.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    if big > 1e140 {
        for v in c.d.iter_mut() {
            *v *= 1e-140;
        }
        ec += 140;
    }
    *ex = ec;
    (c, ec)
}

fn kolmogorov_q(l: f64) -> f64 {
    if l < 0.2 {
        return 1.0;
    }
    let mut s = 0.0;
    for j in 1..=100 {
        let jf = j as f64;
        s += (-1.0f64).powi(j - 1) * (-2.0 * jf * jf * l * l).exp();
    }
    (2.0 * s).clamp(0.0, 1.0)
}

/// Exact one-sided P(D+ >= d) (Birnbaum-Tingey).
fn ks_one_sided(n: usize, d: f64) -> f64 {
    let nf = n as f64;
    if d <= 0.0 {
        return 1.0;
    }
    if d >= 1.0 {
        return 0.0;
    }
    if n > 1000 {
        return (-2.0 * nf * d * d).exp();
    }
    let top = (nf * (1.0 - d)).floor() as usize;
    let mut s = 0.0;
    for j in 0..=top {
        let jf = j as f64;
        let lc = lgamma(nf + 1.0) - lgamma(jf + 1.0) - lgamma(nf - jf + 1.0);
        let a = 1.0 - d - jf / nf;
        let b = d + jf / nf;
        if a <= 0.0 && (nf - jf) > 0.0 {
            continue;
        }
        s += (lc + (nf - jf) * a.ln() + (jf - 1.0) * b.ln()).exp();
    }
    (d * s).clamp(0.0, 1.0)
}

#[derive(Clone, Copy, PartialEq)]
enum KsTail {
    Unequal,
    Larger,
    Smaller,
}

fn ks_tail(o: &[(String, Value)], fname: &str) -> R<KsTail> {
    Ok(match opt_str(o, "tail", fname)?.as_deref() {
        None | Some("unequal") => KsTail::Unequal,
        Some("larger") => KsTail::Larger,
        Some("smaller") => KsTail::Smaller,
        Some(_) => {
            return Err(err(
                "stats:kstest:BadTail",
                format!("{fname}: 'Tail' must be 'unequal', 'larger' or 'smaller'."),
            ));
        }
    })
}

fn ks_alpha(o: &[(String, Value)], fname: &str) -> R<f64> {
    let a = match opt(o, "alpha") {
        Some(v) => arg_f64(v, fname)?,
        None => 0.05,
    };
    if !(a > 0.0 && a < 1.0) {
        return Err(err(
            "stats:kstest:BadAlpha",
            format!("{fname}: 'Alpha' must be a scalar between 0 and 1 exclusive."),
        ));
    }
    Ok(a)
}

fn kstest(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "kstest")?;
    let x = clean(&a[0], "kstest")?;
    let mut k = 1;
    // kstest(x, pd): distribution object as second argument
    let mut pd_obj: Option<Value> = None;
    if let Some(v @ Value::Object(_)) = a.get(1) {
        pd_obj = Some(v.clone());
        k = 2;
    }
    let o = name_values(&a, k, "kstest")?;
    for (name, _) in &o {
        if !["cdf", "alpha", "tail"].contains(&name.as_str()) {
            return Err(err(
                "stats:kstest:InvalidOption",
                format!("kstest: unknown option '{name}'."),
            ));
        }
    }
    let alpha = ks_alpha(&o, "kstest")?;
    let tail = ks_tail(&o, "kstest")?;
    let n = x.len();
    let mut xs = x.clone();
    xs.sort_by(f64::total_cmp);
    // null CDF at the sorted sample
    let cdf: Vec<f64> = if let Some(pd) = pd_obj {
        it.call_named("cdf", vec![pd, Value::col(xs.clone())], 1)?
            .remove(0)
            .to_f64_vec()
            .unwrap_or_default()
    } else if let Some(c) = opt(&o, "cdf") {
        if c.is_text() || matches!(c, Value::Object(_)) {
            return Err(err(
                "stats:kstest:Unsupported",
                "kstest: 'CDF' must be a two-column matrix [x F(x)].",
            ));
        }
        let cm = M::from_num(&real_arg(c, "kstest")?, "kstest")?;
        if cm.c != 2 {
            return Err(err(
                "stats:kstest:BadCDF",
                "kstest: 'CDF' must be a two-column matrix [x F(x)].",
            ));
        }
        let mut out = Vec::with_capacity(n);
        for v in &xs {
            let pos = (0..cm.r).find(|&i| cm.at(i, 0) == *v).ok_or_else(|| {
                err(
                    "stats:kstest:BadCDF",
                    "kstest: the CDF matrix must contain every sample value in its first column.",
                )
            })?;
            out.push(cm.at(pos, 1));
        }
        out
    } else {
        xs.iter()
            .map(|v| (d("norm").cdf)(*v, &[0.0, 1.0]).0)
            .collect()
    };
    if cdf.len() != n {
        return Err(err(
            "stats:kstest:BadCDF",
            "kstest: the CDF could not be evaluated at the sample.",
        ));
    }
    let nf = n as f64;
    let (mut dplus, mut dminus) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for (i, f) in cdf.iter().enumerate() {
        dplus = dplus.max((i + 1) as f64 / nf - f);
        dminus = dminus.max(f - i as f64 / nf);
    }
    let stat = match tail {
        KsTail::Unequal => dplus.max(dminus),
        KsTail::Larger => dplus,
        KsTail::Smaller => dminus,
    };
    let pcalc = |dv: f64| match tail {
        KsTail::Unequal => 1.0 - ks_cdf_exact(n, dv),
        _ => ks_one_sided(n, dv),
    };
    let p = pcalc(stat).clamp(0.0, 1.0);
    // critical value: stat at which p == alpha
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if pcalc(mid) > alpha {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Ok(vec![
        Value::bool(p <= alpha),
        Value::num(p),
        Value::num(stat),
        Value::num(0.5 * (lo + hi)),
    ])
}

fn kstest2(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "kstest2")?;
    let x = clean(&a[0], "kstest2")?;
    let y = clean(&a[1], "kstest2")?;
    let o = name_values(&a, 2, "kstest2")?;
    let alpha = ks_alpha(&o, "kstest2")?;
    let tail = ks_tail(&o, "kstest2")?;
    let (n1, n2) = (x.len() as f64, y.len() as f64);
    let mut all: Vec<f64> = x.iter().chain(&y).copied().collect();
    all.sort_by(f64::total_cmp);
    all.dedup();
    let (mut dp, mut dm) = (0.0f64, 0.0f64);
    let ecdf = |s: &[f64], t: f64, n: f64| s.iter().filter(|v| **v <= t).count() as f64 / n;
    for t in &all {
        let diff = ecdf(&x, *t, n1) - ecdf(&y, *t, n2);
        dp = dp.max(-diff);
        dm = dm.max(diff);
    }
    // Tail 'larger' tests F_x > F_y: statistic max(F_x - F_y)
    let stat = match tail {
        KsTail::Unequal => dp.max(dm),
        KsTail::Larger => dm,
        KsTail::Smaller => dp,
    };
    let ne = n1 * n2 / (n1 + n2);
    let lambda = ((ne.sqrt() + 0.12 + 0.11 / ne.sqrt()) * stat).max(0.0);
    let p = match tail {
        KsTail::Unequal => kolmogorov_q(lambda),
        _ => (-2.0 * lambda * lambda).exp().min(1.0),
    };
    Ok(vec![
        Value::bool(p <= alpha),
        Value::num(p),
        Value::num(stat),
    ])
}

// ---------------------------------------------------------------------------
// chi2gof
// ---------------------------------------------------------------------------

fn chi2gof(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "chi2gof")?;
    let x = clean(&a[0], "chi2gof")?;
    let o = name_values(&a, 1, "chi2gof")?;
    for (name, _) in &o {
        if ![
            "nbins",
            "edges",
            "ctrs",
            "expected",
            "cdf",
            "nparams",
            "emin",
            "alpha",
            "frequency",
        ]
        .contains(&name.as_str())
        {
            return Err(err(
                "stats:chi2gof:InvalidOption",
                format!("chi2gof: unknown option '{name}'."),
            ));
        }
    }
    if opt(&o, "frequency").is_some() {
        return Err(err(
            "stats:chi2gof:Unsupported",
            "chi2gof: the 'Frequency' option is not supported.",
        ));
    }
    let alpha = ks_alpha(&o, "chi2gof")?;
    let emin = match opt(&o, "emin") {
        Some(v) => arg_f64(v, "chi2gof")?,
        None => 5.0,
    };
    let n = x.len() as f64;
    let (mn, mx) = (
        x.iter().copied().fold(f64::INFINITY, f64::min),
        x.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    );
    // inner edges (between bins); the outer bins extend to ±Inf
    let inner: Vec<f64> = if let Some(e) = opt(&o, "edges") {
        real_arg(e, "chi2gof")?.re
    } else if let Some(c) = opt(&o, "ctrs") {
        let c = real_arg(c, "chi2gof")?.re;
        c.windows(2).map(|w| 0.5 * (w[0] + w[1])).collect()
    } else {
        let nb = match opt(&o, "nbins") {
            Some(v) => count_arg(v, "chi2gof", 2.0, 1e6)?,
            None => 10,
        };
        if nb < 2 {
            return Err(err(
                "stats:chi2gof:BadNBins",
                "chi2gof: 'NBins' must be at least 2.",
            ));
        }
        let w = (mx - mn) / nb as f64;
        (1..nb).map(|i| mn + w * i as f64).collect()
    };
    let nb = inner.len() + 1;
    let mut observed = vec![0.0; nb];
    for v in &x {
        observed[inner.partition_point(|e| e < v)] += 1.0;
    }
    let mut nparams = 0.0;
    let expected: Vec<f64> = if let Some(e) = opt(&o, "expected") {
        let e = real_arg(e, "chi2gof")?.re;
        if e.len() != nb {
            return Err(err(
                "stats:chi2gof:BadExpected",
                "chi2gof: 'Expected' must have one entry per bin.",
            ));
        }
        e
    } else {
        // cumulative probabilities at the inner edges
        let cdf_at: Vec<f64> = if let Some(c) = opt(&o, "cdf") {
            let mut args: Vec<Value> = match c {
                Value::Cell(cc) => cc.data.clone(),
                other => vec![other.clone()],
            };
            if args.is_empty() {
                return Err(err(
                    "stats:chi2gof:BadCDF",
                    "chi2gof: empty 'CDF' specification.",
                ));
            }
            let f = args.remove(0);
            let mut call = vec![Value::col(inner.clone())];
            call.extend(args);
            let r = if f.is_text() {
                let mut c2 = vec![f];
                c2.extend(call);
                it.call_named("cdf", c2, 1)?
            } else {
                it.call_value(&f, call, 1)?
            };
            r[0].to_f64_vec().unwrap_or_default()
        } else {
            let (m, s) = (mean(&x), var(&x).sqrt());
            nparams = 2.0;
            inner
                .iter()
                .map(|e| (d("norm").cdf)(*e, &[m, s]).0)
                .collect()
        };
        if cdf_at.len() != inner.len() {
            return Err(err(
                "stats:chi2gof:BadCDF",
                "chi2gof: the CDF must return one value per edge.",
            ));
        }
        let mut cum = vec![0.0];
        cum.extend(cdf_at.iter().copied());
        cum.push(1.0);
        (0..nb).map(|i| n * (cum[i + 1] - cum[i])).collect()
    };
    if let Some(v) = opt(&o, "nparams") {
        nparams = arg_f64(v, "chi2gof")?;
    }
    if expected.iter().any(|e| *e < emin) {
        return Err(err(
            "stats:chi2gof:LowExpected",
            "chi2gof: bins with expected count below 'Emin' would have to be merged; this is not supported, choose wider bins or fewer 'NBins'.",
        ));
    }
    let chi: f64 = observed
        .iter()
        .zip(&expected)
        .map(|(o, e)| (o - e).powi(2) / e)
        .sum();
    let df = nb as f64 - 1.0 - nparams;
    let (h, p) = if df >= 1.0 {
        let p = (d("chi2").cdf)(chi, &[df]).1;
        (Value::bool(p <= alpha), p)
    } else {
        (Value::bool(false), f64::NAN)
    };
    let mut edges = vec![mn.min(inner.first().copied().unwrap_or(mn))];
    edges.extend(inner.iter().copied());
    edges.push(mx.max(inner.last().copied().unwrap_or(mx)));
    Ok(vec![
        h,
        Value::num(p),
        st(vec![
            ("chi2stat", Value::num(chi)),
            ("df", Value::num(df)),
            ("edges", Value::row(edges)),
            ("O", Value::row(observed)),
            ("E", Value::row(expected)),
        ]),
    ])
}

// ---------------------------------------------------------------------------
// ksdensity
// ---------------------------------------------------------------------------

fn kernel(name: &str, u: f64) -> (f64, f64) {
    // (density, cumulative)
    match name {
        "normal" => (
            (-0.5 * u * u).exp() / (2.0 * std::f64::consts::PI).sqrt(),
            0.5 * super::specfun::erfc(-u / std::f64::consts::SQRT_2),
        ),
        "box" => {
            if u.abs() <= 3.0f64.sqrt() {
                (0.5 / 3.0f64.sqrt(), 0.5 + u / (2.0 * 3.0f64.sqrt()))
            } else {
                (0.0, if u < 0.0 { 0.0 } else { 1.0 })
            }
        }
        "triangle" => {
            let s6 = 6.0f64.sqrt();
            let t = u / s6;
            if t.abs() <= 1.0 {
                (
                    (1.0 - t.abs()) / s6,
                    if t < 0.0 {
                        0.5 * (1.0 + t).powi(2)
                    } else {
                        1.0 - 0.5 * (1.0 - t).powi(2)
                    },
                )
            } else {
                (0.0, if u < 0.0 { 0.0 } else { 1.0 })
            }
        }
        _ => {
            // epanechnikov
            let s5 = 5.0f64.sqrt();
            let t = u / s5;
            if t.abs() <= 1.0 {
                (
                    0.75 * (1.0 - t * t) / s5,
                    0.5 + 0.75 * (t - t * t * t / 3.0),
                )
            } else {
                (0.0, if u < 0.0 { 0.0 } else { 1.0 })
            }
        }
    }
}

fn ksdensity(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "ksdensity")?;
    let x = clean(&a[0], "ksdensity")?;
    let mut k = 1;
    let mut pts: Option<Vec<f64>> = None;
    if let Some(v) = a.get(1)
        && !v.is_text()
    {
        pts = Some(real_arg(v, "ksdensity")?.re);
        k = 2;
    }
    let o = name_values(&a, k, "ksdensity")?;
    for (name, _) in &o {
        if !["bandwidth", "kernel", "function", "npoints", "support"].contains(&name.as_str()) {
            return Err(err(
                "stats:ksdensity:InvalidOption",
                format!("ksdensity: option '{name}' is not supported."),
            ));
        }
    }
    if let Some(s) = opt(&o, "support")
        && s.to_str()
            .is_none_or(|t| !t.eq_ignore_ascii_case("unbounded"))
    {
        return Err(err(
            "stats:ksdensity:Unsupported",
            "ksdensity: only 'Support','unbounded' is supported.",
        ));
    }
    let kname = opt_str(&o, "kernel", "ksdensity")?.unwrap_or_else(|| "normal".into());
    if !["normal", "box", "triangle", "epanechnikov"].contains(&kname.as_str()) {
        return Err(err(
            "stats:ksdensity:BadKernel",
            "ksdensity: 'Kernel' must be 'normal', 'box', 'triangle' or 'epanechnikov'.",
        ));
    }
    let func = opt_str(&o, "function", "ksdensity")?.unwrap_or_else(|| "pdf".into());
    if !["pdf", "cdf", "survivor"].contains(&func.as_str()) {
        return Err(err(
            "stats:ksdensity:Unsupported",
            "ksdensity: 'Function' must be 'pdf', 'cdf' or 'survivor'.",
        ));
    }
    let n = x.len() as f64;
    let bw = match opt(&o, "bandwidth") {
        Some(v) => arg_f64(v, "ksdensity")?,
        None => {
            let med = median(&x);
            let dev: Vec<f64> = x.iter().map(|t| (t - med).abs()).collect();
            let mut sigma = median(&dev) / 0.6745;
            if sigma <= 0.0 {
                sigma = 1.0 / 0.6745;
            }
            sigma * (4.0 / (3.0 * n)).powf(0.2)
        }
    };
    if bw.is_nan() || bw <= 0.0 {
        return Err(err(
            "stats:ksdensity:BadBandwidth",
            "ksdensity: 'Bandwidth' must be a positive scalar.",
        ));
    }
    let xi = pts.unwrap_or_else(|| {
        let np = match opt(&o, "npoints") {
            Some(v) => count_arg(v, "ksdensity", 2.0, 1e7).unwrap_or(100),
            None => 100,
        };
        let (lo, hi) = (
            x.iter().copied().fold(f64::INFINITY, f64::min) - 3.0 * bw,
            x.iter().copied().fold(f64::NEG_INFINITY, f64::max) + 3.0 * bw,
        );
        (0..np)
            .map(|i| lo + (hi - lo) * i as f64 / (np as f64 - 1.0))
            .collect()
    });
    let f: Vec<f64> = xi
        .iter()
        .map(|t| {
            let s: f64 = x
                .iter()
                .map(|xv| {
                    let (dd, cc) = kernel(&kname, (t - xv) / bw);
                    match func.as_str() {
                        "pdf" => dd / bw,
                        "cdf" => cc,
                        _ => 1.0 - cc,
                    }
                })
                .sum();
            s / n
        })
        .collect();
    if nargout == 0 {
        it.call_named("plot", vec![Value::col(xi), Value::col(f)], 0)?;
        return Ok(vec![]);
    }
    Ok(vec![Value::col(f), Value::col(xi), Value::num(bw)])
}

// ---------------------------------------------------------------------------
// resampling
// ---------------------------------------------------------------------------

fn rows_of(v: &Value, idx: &[usize]) -> R<Value> {
    // rows of a numeric/bool/char/cell matrix selected by `idx`
    let dims = v.dims();
    if dims.len() != 2 {
        return Err(err("stats:resample:BadData", "resampling needs 2-D data."));
    }
    let (r, c) = (dims[0], dims[1]);
    let pick = |i: usize, j: usize| idx[i] + j * r;
    let m = idx.len();
    Ok(match v {
        Value::Num(nn) => {
            let mut re = Vec::with_capacity(m * c);
            for j in 0..c {
                for i in 0..m {
                    re.push(nn.re[pick(i, j)]);
                }
            }
            let mut out = Num::real(vec![m, c], re);
            if let Some(im) = &nn.im {
                let mut iv = Vec::with_capacity(m * c);
                for j in 0..c {
                    for i in 0..m {
                        iv.push(im[pick(i, j)]);
                    }
                }
                out = Num::complex(vec![m, c], out.re, iv);
            }
            Value::Num(out.with_class(nn.class))
        }
        Value::Bool(b) => {
            let mut d = Vec::with_capacity(m * c);
            for j in 0..c {
                for i in 0..m {
                    d.push(b.data[pick(i, j)]);
                }
            }
            bool_array(vec![m, c], d)
        }
        Value::Cell(cl) => {
            let mut d = Vec::with_capacity(m * c);
            for j in 0..c {
                for i in 0..m {
                    d.push(cl.data[pick(i, j)].clone());
                }
            }
            cell(vec![m, c], d)
        }
        _ => {
            return Err(err(
                "stats:resample:BadData",
                "resampling supports numeric, logical and cell data.",
            ));
        }
    })
}

fn bootstrp(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 3, "bootstrp")?;
    let nboot = count_arg(&a[0], "bootstrp", 1.0, 1e7)?;
    let fun = a[1].clone();
    let data = &a[2..];
    let n = data[0].dims()[0];
    if data.iter().any(|v| v.dims()[0] != n) {
        return Err(err(
            "stats:bootstrp:InputSizeMismatch",
            "bootstrp: all data arguments must have the same number of rows.",
        ));
    }
    if n == 0 {
        return Err(err(
            "stats:bootstrp:NoData",
            "bootstrp: the data are empty.",
        ));
    }
    if n.saturating_mul(nboot) > super::dist::MAX_DRAWS {
        return Err(err(
            "MATLAB:pmaxsize",
            "Out of memory. The requested array exceeds the maximum size supported by the Rust engine.",
        ));
    }
    let mut rows: Vec<Vec<f64>> = Vec::new();
    let mut sam = M::zeros(n, nboot);
    for b in 0..nboot {
        let idx: Vec<usize> = (0..n)
            .map(|_| ((it.rng.uniform() * n as f64) as usize).min(n - 1))
            .collect();
        for (i, &ix) in idx.iter().enumerate() {
            sam.set(i, b, ix as f64 + 1.0);
        }
        let args: Vec<Value> = data.iter().map(|v| rows_of(v, &idx)).collect::<R<_>>()?;
        let r = it.call_value(&fun, args, 1)?;
        let v = r.first().and_then(|v| v.to_f64_vec()).ok_or_else(|| {
            err(
                "stats:bootstrp:BadResult",
                "bootstrp: BOOTFUN must return numeric values.",
            )
        })?;
        if let Some(first) = rows.first()
            && first.len() != v.len()
        {
            return Err(err(
                "stats:bootstrp:BadResult",
                "bootstrp: BOOTFUN must return the same number of values on every call.",
            ));
        }
        rows.push(v);
    }
    let w = rows.first().map_or(0, |r| r.len());
    let mut m = M::zeros(nboot, w);
    for (b, r) in rows.iter().enumerate() {
        for (j, v) in r.iter().enumerate() {
            m.set(b, j, *v);
        }
    }
    let mut res = vec![m.value()];
    if nargout > 1 {
        res.push(sam.value());
    }
    Ok(res)
}

fn datasample(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "datasample")?;
    let n = a[0].dims()[0];
    let (k, from) = match a.get(1) {
        Some(v) if !v.is_text() => (
            count_arg(v, "datasample", 0.0, super::dist::MAX_DRAWS as f64)?,
            2,
        ),
        _ => (n, 1),
    };
    let o = name_values(&a, from, "datasample")?;
    let replace = match opt(&o, "replace") {
        Some(v) => v.is_true()?,
        None => true,
    };
    let weights = match opt(&o, "weights") {
        Some(v) => Some(real_arg(v, "datasample")?.re),
        None => None,
    };
    if !replace && k > n {
        return Err(err(
            "stats:datasample:SampleTooBig",
            "datasample: K must not exceed the number of observations when sampling without replacement.",
        ));
    }
    let mut w = weights.unwrap_or_else(|| vec![1.0; n]);
    if w.len() != n {
        return Err(err(
            "stats:datasample:BadWeights",
            "datasample: 'Weights' must have one entry per observation.",
        ));
    }
    let mut idx = Vec::with_capacity(k);
    for _ in 0..k {
        let tot: f64 = w.iter().sum();
        if tot <= 0.0 {
            return Err(err(
                "stats:datasample:BadWeights",
                "datasample: 'Weights' must contain a positive entry for every draw.",
            ));
        }
        let mut u = it.rng.uniform() * tot;
        let mut pick = n - 1;
        for (i, wi) in w.iter().enumerate() {
            if u < *wi {
                pick = i;
                break;
            }
            u -= wi;
        }
        idx.push(pick);
        if !replace {
            w[pick] = 0.0;
        }
    }
    let mut res = vec![rows_of(&a[0], &idx)?];
    if nargout > 1 {
        res.push(Value::col(idx.iter().map(|&i| i as f64 + 1.0).collect()));
    }
    Ok(res)
}

// ---------------------------------------------------------------------------
// multivariate helpers
// ---------------------------------------------------------------------------

fn mahal(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "mahal")?;
    let y = M::from_num(&real_arg(&a[0], "mahal")?, "mahal")?;
    let x = M::from_num(&real_arg(&a[1], "mahal")?, "mahal")?;
    if y.c != x.c {
        return Err(err(
            "stats:mahal:InputSizeMismatch",
            "mahal: X and Y must have the same number of columns.",
        ));
    }
    if x.r <= x.c {
        return Err(err(
            "stats:mahal:TooFewRows",
            "mahal: X must have more rows than columns.",
        ));
    }
    let p = x.c;
    let mu: Vec<f64> = (0..p).map(|j| mean(x.col(j))).collect();
    let mut cov = M::zeros(p, p);
    for i in 0..p {
        for j in 0..p {
            let s: f64 = (0..x.r)
                .map(|r| (x.at(r, i) - mu[i]) * (x.at(r, j) - mu[j]))
                .sum();
            cov.set(i, j, s / (x.r as f64 - 1.0));
        }
    }
    let out: Vec<f64> = (0..y.r)
        .map(|r| {
            let dv: Vec<f64> = (0..p).map(|j| y.at(r, j) - mu[j]).collect();
            match chol_solve(&cov, &dv) {
                Some(z) => dv.iter().zip(&z).map(|(a, b)| a * b).sum(),
                None => f64::NAN,
            }
        })
        .collect();
    if out.iter().any(|v| v.is_nan()) {
        return Err(err(
            "stats:mahal:RankDeficient",
            "mahal: the covariance matrix of X is singular.",
        ));
    }
    one(Value::col(out))
}

fn knnsearch(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "knnsearch")?;
    let o = name_values(&a, 2, "knnsearch")?;
    let k = match opt(&o, "k") {
        Some(v) => count_arg(v, "knnsearch", 1.0, 1e9)?,
        None => 1,
    };
    let x = real_arg(&a[0], "knnsearch")?;
    if k == 0 || k > x.dims[0] {
        return Err(err(
            "stats:knnsearch:BadK",
            "knnsearch: 'K' must be between 1 and the number of rows of X.",
        ));
    }
    // pdist2(X, Y, ..., 'Smallest', k) gives the k nearest X for every Y
    let args2 = {
        let mut v = vec![a[0].clone(), a[1].clone()];
        if let Some(dn) = opt(&o, "distance") {
            v.push(dn.clone());
        }
        v.push(strv("Smallest"));
        v.push(Value::num(k as f64));
        v
    };
    let r = it.call_named("pdist2", args2, 2)?;
    // r: k-by-m(Y); knnsearch returns m-by-k
    let transposed: Vec<Value> = r
        .iter()
        .map(|v| {
            let m = M::from_num(&v.to_num().unwrap_or_else(Num::empty), "knnsearch")?;
            let mut t = M::zeros(m.c, m.r);
            for i in 0..m.r {
                for j in 0..m.c {
                    t.set(j, i, m.at(i, j));
                }
            }
            Ok(t.value())
        })
        .collect::<R<_>>()?;
    let mut out = vec![transposed[1].clone()];
    if nargout > 1 {
        out.push(transposed[0].clone());
    }
    Ok(out)
}

fn clusterdata(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "clusterdata")?;
    let o = name_values(&a, 1, "clusterdata")?;
    for (name, _) in &o {
        if ![
            "distance",
            "linkage",
            "maxclust",
            "cutoff",
            "criterion",
            "depth",
        ]
        .contains(&name.as_str())
        {
            return Err(err(
                "stats:clusterdata:InvalidOption",
                format!("clusterdata: unknown option '{name}'."),
            ));
        }
    }
    let dist_name = opt(&o, "distance")
        .cloned()
        .unwrap_or_else(|| strv("euclidean"));
    let link = opt(&o, "linkage").cloned().unwrap_or_else(|| strv("ward"));
    let y = it
        .call_named("pdist", vec![a[0].clone(), dist_name], 1)?
        .remove(0);
    let z = it.call_named("linkage", vec![y, link], 1)?.remove(0);
    let mut cargs = vec![z];
    for key in ["maxclust", "cutoff", "criterion", "depth"] {
        if let Some(v) = opt(&o, key) {
            cargs.push(strv(key));
            cargs.push(v.clone());
        }
    }
    if opt(&o, "maxclust").is_none() && opt(&o, "cutoff").is_none() {
        return Err(err(
            "stats:clusterdata:MissingCut",
            "clusterdata: specify 'MaxClust' or 'Cutoff'.",
        ));
    }
    it.call_named("cluster", cargs, 1)
}

fn cophenet(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "cophenet")?;
    let zm = M::from_num(&real_arg(&a[0], "cophenet")?, "cophenet")?;
    if zm.c != 3 {
        return Err(err(
            "stats:cophenet:BadLinkage",
            "cophenet: Z must be an (m-1)-by-3 linkage matrix.",
        ));
    }
    let yv = real_arg(&a[1], "cophenet")?.re;
    let m = zm.r + 1;
    if yv.len() != m * (m - 1) / 2 {
        return Err(err(
            "stats:cophenet:InputSizeMismatch",
            "cophenet: Y must have m(m-1)/2 distances.",
        ));
    }
    let mut members: Vec<Vec<usize>> = (0..m).map(|i| vec![i]).collect();
    let mut coph = vec![0.0; m * m];
    for s in 0..zm.r {
        let (i, j, h) = (
            zm.at(s, 0) as usize - 1,
            zm.at(s, 1) as usize - 1,
            zm.at(s, 2),
        );
        let (ma, mb) = (members[i].clone(), members[j].clone());
        for &p in &ma {
            for &q in &mb {
                coph[p * m + q] = h;
                coph[q * m + p] = h;
            }
        }
        let mut merged = ma;
        merged.extend(mb);
        members.push(merged);
    }
    let mut dvec = Vec::with_capacity(yv.len());
    for i in 0..m {
        for j in i + 1..m {
            dvec.push(coph[i * m + j]);
        }
    }
    let (mu, mv) = (mean(&yv), mean(&dvec));
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for (u, v) in yv.iter().zip(&dvec) {
        sxy += (u - mu) * (v - mv);
        sxx += (u - mu).powi(2);
        syy += (v - mv).powi(2);
    }
    let mut res = vec![Value::num(sxy / (sxx * syy).sqrt())];
    if nargout > 1 {
        res.push(Value::row(dvec));
    }
    Ok(res)
}

fn silhouette(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "silhouette")?;
    if nargout == 0 {
        return Err(err(
            "stats:silhouette:Unsupported",
            "silhouette: request an output; the silhouette plot is not available.",
        ));
    }
    let x = real_arg(&a[0], "silhouette")?;
    let n = x.dims[0];
    let cl = super::multi::group_ids(&a[1], "silhouette")?;
    if cl.len() != n {
        return Err(err(
            "stats:silhouette:InputSizeMismatch",
            "silhouette: CLUST must have one entry per row of X.",
        ));
    }
    let dn = a.get(2).cloned().unwrap_or_else(|| strv("sqeuclidean"));
    let dmat = it.call_named("pdist", vec![a[0].clone(), dn], 1)?.remove(0);
    let sq = it.call_named("squareform", vec![dmat], 1)?.remove(0);
    let dm = M::from_num(&real_arg(&sq, "silhouette")?, "silhouette")?;
    let k = cl.iter().max().map_or(0, |m| m + 1);
    let mut sizes = vec![0.0; k];
    for &c in &cl {
        sizes[c] += 1.0;
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let mut sums = vec![0.0; k];
        for j in 0..n {
            if j != i {
                sums[cl[j]] += dm.at(i, j);
            }
        }
        let own = cl[i];
        if sizes[own] <= 1.0 {
            out.push(1.0);
            continue;
        }
        let av = sums[own] / (sizes[own] - 1.0);
        let bv = (0..k)
            .filter(|&c| c != own && sizes[c] > 0.0)
            .map(|c| sums[c] / sizes[c])
            .fold(f64::INFINITY, f64::min);
        out.push((bv - av) / av.max(bv));
    }
    let mut res = vec![Value::col(out)];
    if nargout > 1 {
        res.push(Value::empty());
    }
    Ok(res)
}
