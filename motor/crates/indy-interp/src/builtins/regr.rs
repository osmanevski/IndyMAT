//! Linear regression (`regress`, minimal `fitlm` → `LinearModel`,
//! `coefCI`, `predict`) and probability-distribution objects
//! (`makedist`, `fitdist`) with the methods the toolbox offers on them
//! (`pdf`, `cdf`, `icdf`, `random`, `mean`, `var`, `std`, `median`,
//! `iqr`).

use std::sync::Mutex;

use indy_value::{Object, Value};

use super::dist::{self, Dist};
use super::su::*;
use super::{arg_f64, arg_str, err, need, one, reg};
use crate::{BuiltinFn, Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "regress", regress);
    reg(it, "fitlm", fitlm);
    reg(it, "coefCI", coef_ci);
    reg(it, "makedist", makedist);
    reg(it, "fitdist", fitdist);
    for (name, f) in [
        ("mean", dist_mean as BuiltinFn),
        ("median", dist_median),
        ("var", dist_var),
        ("std", dist_std),
        ("iqr", dist_iqr),
    ] {
        wrap(it, name, f);
    }
}

// ---------------------------------------------------------------------------
// overloading existing builtins for distribution objects
// ---------------------------------------------------------------------------

static PREV: Mutex<Vec<(&'static str, BuiltinFn)>> = Mutex::new(Vec::new());

fn wrap(it: &mut Interp, name: &'static str, f: BuiltinFn) {
    if let Ok(mut p) = PREV.lock()
        && !p.iter().any(|(n, _)| *n == name)
        && let Some(prev) = it.builtins.get(name)
    {
        p.push((name, *prev));
    }
    it.builtins.insert(name, f);
}

fn prev(it: &mut Interp, name: &str, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let f = PREV
        .lock()
        .ok()
        .and_then(|p| p.iter().find(|(n, _)| *n == name).map(|(_, f)| *f));
    match f {
        Some(f) => f(it, a, nargout),
        None => Err(err(
            "MATLAB:UndefinedFunction",
            format!("Undefined function or variable '{name}'."),
        )),
    }
}

/// Distribution table row and parameters of a `prob.*` object.
pub fn dist_of(o: &Object) -> Option<(&'static Dist, Vec<f64>)> {
    if !o.class.starts_with("prob.") {
        return None;
    }
    let key = o.get("DistributionName")?.to_str()?;
    let d = dist::find(&key)?;
    let spec = SPECS.iter().find(|s| s.key == d.key)?;
    let p = spec
        .params
        .iter()
        .map(|n| o.get(n).and_then(|v| v.scalar_f64()))
        .collect::<Option<Vec<_>>>()?;
    Some((d, p))
}

fn object_dist(a: &[Value]) -> Option<(&Object, &'static Dist, Vec<f64>)> {
    if let Some(Value::Object(o)) = a.first()
        && let Some((d, p)) = dist_of(o)
    {
        return Some((o, d, p));
    }
    None
}

macro_rules! moment_fn {
    ($fname:ident, $name:literal, $body:expr) => {
        fn $fname(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
            if let Some((_, d, p)) = object_dist(&a) {
                if a.len() > 1 {
                    return Err(err(
                        "stats:probdist:TooManyInputs",
                        concat!($name, ": a distribution object takes no further arguments."),
                    ));
                }
                let f: fn(&Dist, &[f64]) -> f64 = $body;
                return one(Value::num(f(d, &p)));
            }
            prev(it, $name, a, nargout)
        }
    };
}

moment_fn!(dist_mean, "mean", |d, p| (d.moments)(p).0);
moment_fn!(dist_var, "var", |d, p| (d.moments)(p).1);
moment_fn!(dist_std, "std", |d, p| (d.moments)(p).1.sqrt());
moment_fn!(dist_median, "median", |d, p| d.quantile(0.5, p));
moment_fn!(dist_iqr, "iqr", |d, p| d.quantile(0.75, p)
    - d.quantile(0.25, p));

/// `pdf/cdf/icdf/random(pd, ...)` for distribution objects: evaluates
/// the generic call with the object's name and parameters spliced in.
pub fn object_call(it: &mut Interp, fname: &str, a: &[Value]) -> Option<R<Vec<Value>>> {
    let (_, d, p) = object_dist(a)?;
    let mut args: Vec<Value> = vec![strv(d.key)];
    if fname == "random" {
        args.extend(p.iter().map(|v| Value::num(*v)));
        args.extend_from_slice(&a[1..]);
    } else {
        args.push(a.get(1).cloned().unwrap_or_default());
        args.extend(p.iter().map(|v| Value::num(*v)));
        args.extend(a.iter().skip(2).cloned());
    }
    Some(it.call_named(fname, args, 1))
}

// ---------------------------------------------------------------------------
// makedist / fitdist
// ---------------------------------------------------------------------------

struct Spec {
    key: &'static str,
    name: &'static str,
    params: &'static [&'static str],
    defaults: &'static [f64],
}

const SPECS: &[Spec] = &[
    Spec {
        key: "norm",
        name: "Normal",
        params: &["mu", "sigma"],
        defaults: &[0.0, 1.0],
    },
    Spec {
        key: "exp",
        name: "Exponential",
        params: &["mu"],
        defaults: &[1.0],
    },
    Spec {
        key: "gam",
        name: "Gamma",
        params: &["a", "b"],
        defaults: &[1.0, 1.0],
    },
    Spec {
        key: "beta",
        name: "Beta",
        params: &["a", "b"],
        defaults: &[1.0, 1.0],
    },
    Spec {
        key: "bino",
        name: "Binomial",
        params: &["N", "p"],
        defaults: &[1.0, 0.5],
    },
    Spec {
        key: "poiss",
        name: "Poisson",
        params: &["lambda"],
        defaults: &[1.0],
    },
    Spec {
        key: "unif",
        name: "Uniform",
        params: &["Lower", "Upper"],
        defaults: &[0.0, 1.0],
    },
    Spec {
        key: "logn",
        name: "Lognormal",
        params: &["mu", "sigma"],
        defaults: &[0.0, 1.0],
    },
    Spec {
        key: "wbl",
        name: "Weibull",
        params: &["A", "B"],
        defaults: &[1.0, 1.0],
    },
    Spec {
        key: "rayl",
        name: "Rayleigh",
        params: &["B"],
        defaults: &[1.0],
    },
    Spec {
        key: "nbin",
        name: "NegativeBinomial",
        params: &["R", "P"],
        defaults: &[1.0, 0.5],
    },
    Spec {
        key: "ev",
        name: "ExtremeValue",
        params: &["mu", "sigma"],
        defaults: &[0.0, 1.0],
    },
];

fn spec_for(name: &str, fname: &str) -> R<&'static Spec> {
    let d = dist::find(name).ok_or_else(|| {
        err(
            "stats:makedist:UnrecognizedName",
            format!("{fname}: unrecognized distribution name '{name}'."),
        )
    })?;
    SPECS.iter().find(|s| s.key == d.key).ok_or_else(|| {
        err(
            "stats:makedist:UnsupportedName",
            format!("{fname}: distribution '{name}' is not supported as an object."),
        )
    })
}

fn build(spec: &Spec, p: &[f64]) -> Value {
    let mut o = Object::new(&format!("prob.{}Distribution", spec.name));
    o.set("DistributionName", strv(spec.name));
    o.set("NumParameters", Value::num(spec.params.len() as f64));
    o.set(
        "ParameterNames",
        cell(
            vec![1, spec.params.len()],
            spec.params.iter().map(|s| strv(s)).collect(),
        ),
    );
    o.set("ParameterValues", Value::row(p.to_vec()));
    for (n, v) in spec.params.iter().zip(p) {
        o.set(n, Value::num(*v));
    }
    Value::Object(Box::new(o))
}

fn makedist(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "makedist")?;
    let name = arg_str(&a[0], "makedist")?;
    let spec = spec_for(&name, "makedist")?;
    let d = dist::find(&name).unwrap();
    let mut p = spec.defaults.to_vec();
    let o = name_values(&a, 1, "makedist")?;
    for (k, v) in &o {
        let i = spec
            .params
            .iter()
            .position(|n| n.to_lowercase() == *k)
            .ok_or_else(|| {
                err(
                    "stats:makedist:UnknownParameter",
                    format!(
                        "makedist: '{k}' is not a parameter of the {} distribution.",
                        spec.name
                    ),
                )
            })?;
        p[i] = arg_f64(v, "makedist")?;
    }
    if !(d.valid)(&p) {
        return Err(err(
            "stats:makedist:BadParameters",
            format!(
                "makedist: invalid parameter values for the {} distribution.",
                spec.name
            ),
        ));
    }
    one(build(spec, &p))
}

fn digamma(mut x: f64) -> f64 {
    let mut r = 0.0;
    while x < 8.0 {
        r -= 1.0 / x;
        x += 1.0;
    }
    let f = 1.0 / (x * x);
    r + x.ln()
        - 0.5 / x
        - f * (1.0 / 12.0 - f * (1.0 / 120.0 - f * (1.0 / 252.0 - f * (1.0 / 240.0 - f / 132.0))))
}

fn trigamma(mut x: f64) -> f64 {
    let mut r = 0.0;
    while x < 8.0 {
        r += 1.0 / (x * x);
        x += 1.0;
    }
    let f = 1.0 / (x * x);
    r + 1.0 / x
        + f / 2.0
        + f / x * (1.0 / 6.0 - f * (1.0 / 30.0 - f * (1.0 / 42.0 - f * (1.0 / 30.0))))
}

/// Maximum-likelihood (or the toolbox's unbiased) estimates of the
/// parameters of family `key` from NaN-free data `v`.
pub(super) fn estimate(key: &str, v: &[f64]) -> R<Vec<f64>> {
    let n = v.len() as f64;
    let p: Vec<f64> = match key {
        "norm" => vec![mean(v), var(v).sqrt()],
        "exp" => vec![mean(v)],
        "poiss" => vec![mean(v)],
        "unif" => vec![
            v.iter().copied().fold(f64::INFINITY, f64::min),
            v.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        ],
        "logn" => {
            if v.iter().any(|t| *t <= 0.0) {
                return Err(err(
                    "stats:fitdist:BadData",
                    "fitdist: Lognormal data must be positive.",
                ));
            }
            let l: Vec<f64> = v.iter().map(|t| t.ln()).collect();
            vec![mean(&l), var(&l).sqrt()]
        }
        "rayl" => vec![(v.iter().map(|t| t * t).sum::<f64>() / (2.0 * n)).sqrt()],
        "gam" => {
            if v.iter().any(|t| *t <= 0.0) {
                return Err(err(
                    "stats:fitdist:BadData",
                    "fitdist: Gamma data must be positive.",
                ));
            }
            let m = mean(v);
            let s = m.ln() - v.iter().map(|t| t.ln()).sum::<f64>() / n;
            let mut a = (3.0 - s + ((s - 3.0).powi(2) + 24.0 * s).sqrt()) / (12.0 * s);
            for _ in 0..100 {
                let step = (a.ln() - digamma(a) - s) / (1.0 / a - trigamma(a));
                let na = (a - step).max(a / 10.0);
                if (na - a).abs() < 1e-14 * a {
                    a = na;
                    break;
                }
                a = na;
            }
            vec![a, m / a]
        }
        "wbl" => {
            if v.iter().any(|t| *t <= 0.0) {
                return Err(err(
                    "stats:fitdist:BadData",
                    "fitdist: Weibull data must be positive.",
                ));
            }
            let ml = v.iter().map(|t| t.ln()).sum::<f64>() / n;
            // root of g(k) = Σ x^k ln x / Σ x^k − 1/k − mean(ln x)
            let g = |k: f64| {
                let (mut s0, mut s1) = (0.0, 0.0);
                for t in v {
                    let w = t.powf(k);
                    s0 += w;
                    s1 += w * t.ln();
                }
                s1 / s0 - 1.0 / k - ml
            };
            let (mut lo, mut hi) = (1e-3, 1.0);
            while g(hi) < 0.0 && hi < 1e6 {
                hi *= 2.0;
            }
            for _ in 0..200 {
                let mid = 0.5 * (lo + hi);
                if g(mid) < 0.0 {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let k = 0.5 * (lo + hi);
            vec![
                (v.iter().map(|t| t.powf(k)).sum::<f64>() / n).powf(1.0 / k),
                k,
            ]
        }
        other => {
            return Err(err(
                "stats:fitdist:Unsupported",
                format!("fitdist: fitting the '{other}' distribution is not supported."),
            ));
        }
    };
    Ok(p)
}

fn fitdist(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "fitdist")?;
    let x = real_arg(&a[0], "fitdist")?;
    let name = arg_str(&a[1], "fitdist")?;
    let spec = spec_for(&name, "fitdist")?;
    if a.len() > 2 {
        return Err(err(
            "stats:fitdist:Unsupported",
            "fitdist: options ('By', 'Censoring', 'Frequency', ...) are not supported.",
        ));
    }
    let v: Vec<f64> = x.re.iter().copied().filter(|t| !t.is_nan()).collect();
    if v.is_empty() {
        return Err(err(
            "stats:fitdist:NotEnoughData",
            "fitdist: X must contain at least one non-NaN value.",
        ));
    }
    let p = estimate(spec.key, &v)?;
    one(build(spec, &p))
}

// ---------------------------------------------------------------------------
// regression
// ---------------------------------------------------------------------------

struct Fit {
    beta: Vec<f64>,
    cov_unscaled: M,
    resid: Vec<f64>,
    fitted: Vec<f64>,
    rank: usize,
    sse: f64,
    leverage: Vec<f64>,
}

fn fit(x: &M, y: &[f64]) -> Fit {
    let (beta, rank, cov) = lstsq(x, y);
    let fitted: Vec<f64> = (0..x.r)
        .map(|i| (0..x.c).map(|j| x.at(i, j) * beta[j]).sum())
        .collect();
    let resid: Vec<f64> = y.iter().zip(&fitted).map(|(a, b)| a - b).collect();
    let sse = resid.iter().map(|r| r * r).sum();
    // h_i = x_i' (X'X)^-1 x_i
    let leverage = (0..x.r)
        .map(|i| {
            let mut s = 0.0;
            for p in 0..x.c {
                for q in 0..x.c {
                    s += x.at(i, p) * cov.at(p, q) * x.at(i, q);
                }
            }
            s
        })
        .collect();
    Fit {
        beta,
        cov_unscaled: cov,
        resid,
        fitted,
        rank,
        sse,
        leverage,
    }
}

fn tq(q: f64, df: f64) -> f64 {
    dist::find("t").map_or(f64::NAN, |d| d.quantile(q, &[df]))
}

fn regress(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "regress")?;
    let yn = real_arg(&a[0], "regress")?;
    let xm = M::from_num(&real_arg(&a[1], "regress")?, "regress")?;
    let alpha = match a.get(2) {
        Some(v) if !v.is_empty() => arg_f64(v, "regress")?,
        _ => 0.05,
    };
    if !(alpha > 0.0 && alpha < 1.0) {
        return Err(err(
            "stats:regress:BadAlpha",
            "regress: ALPHA must be a scalar between 0 and 1 exclusive.",
        ));
    }
    if yn.dims.len() != 2 || yn.dims[1] != 1 || yn.dims[0] != xm.r {
        return Err(err(
            "stats:regress:InvalidData",
            "regress: Y must be a column vector with as many rows as X.",
        ));
    }
    // drop rows with NaN
    let keep: Vec<usize> = (0..xm.r)
        .filter(|&i| !yn.re[i].is_nan() && xm.row(i).iter().all(|t| !t.is_nan()))
        .collect();
    let n = keep.len();
    let p = xm.c;
    let mut x = M::zeros(n, p);
    for (r, &i) in keep.iter().enumerate() {
        for j in 0..p {
            x.set(r, j, xm.at(i, j));
        }
    }
    let y: Vec<f64> = keep.iter().map(|&i| yn.re[i]).collect();
    let f = fit(&x, &y);
    let dfe = n as f64 - f.rank as f64;
    let mse = f.sse / dfe;
    let t = tq(1.0 - alpha / 2.0, dfe);
    let mut bint = M::zeros(p, 2);
    for j in 0..p {
        let se = (f.cov_unscaled.at(j, j) * mse).sqrt();
        bint.set(j, 0, f.beta[j] - t * se);
        bint.set(j, 1, f.beta[j] + t * se);
    }
    // residual vectors have the full length (NaN rows give NaN)
    let expand = |v: &[f64]| {
        let mut o = vec![f64::NAN; xm.r];
        for (r, &i) in keep.iter().enumerate() {
            o[i] = v[r];
        }
        o
    };
    let mut res = vec![Value::col(f.beta.clone()), bint.value()];
    if nargout > 2 {
        res.push(Value::col(expand(&f.resid)));
    }
    if nargout > 3 {
        let tr = t; // the toolbox uses the residual degrees of freedom here
        let mut ri = M::zeros(xm.r, 2);
        for i in 0..xm.r {
            ri.set(i, 0, f64::NAN);
            ri.set(i, 1, f64::NAN);
        }
        for (r, &i) in keep.iter().enumerate() {
            let h = f.leverage[r];
            let e = f.resid[r];
            let s2 = (dfe * mse - e * e / (1.0 - h)) / (dfe - 1.0);
            let w = tr * (s2 * (1.0 - h)).max(0.0).sqrt();
            ri.set(i, 0, e - w);
            ri.set(i, 1, e + w);
        }
        res.push(ri.value());
    }
    if nargout > 4 {
        let has_const = (0..p).any(|j| {
            let c = x.col(j);
            c.iter().all(|v| *v == c[0]) && c[0] != 0.0
        });
        let (r2, fstat, dfm) = if has_const {
            let ym = mean(&y);
            let sst: f64 = y.iter().map(|v| (v - ym).powi(2)).sum();
            let r2 = 1.0 - f.sse / sst;
            let dfm = f.rank as f64 - 1.0;
            (r2, (r2 / dfm) / ((1.0 - r2) / dfe), dfm)
        } else {
            let sst: f64 = y.iter().map(|v| v * v).sum();
            let r2 = 1.0 - f.sse / sst;
            let dfm = f.rank as f64;
            (r2, (r2 / dfm) / ((1.0 - r2) / dfe), dfm)
        };
        let pv = if dfm >= 1.0 {
            dist::find("f").map_or(f64::NAN, |d| (d.cdf)(fstat, &[dfm, dfe]).1)
        } else {
            f64::NAN
        };
        res.push(Value::row(vec![r2, fstat, pv, mse]));
    }
    Ok(res)
}

fn fitlm(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "fitlm")?;
    if !matches!(a[0], Value::Num(_) | Value::Bool(_))
        || !matches!(a[1], Value::Num(_) | Value::Bool(_))
    {
        return Err(err(
            "stats:fitlm:Unsupported",
            "fitlm: only the numeric form fitlm(X, y) is supported (no tables or formulas).",
        ));
    }
    let xn = real_arg(&a[0], "fitlm")?;
    let yn = real_arg(&a[1], "fitlm")?;
    if a.len() % 2 == 1 && a[2].is_text() {
        return Err(err(
            "stats:fitlm:Unsupported",
            "fitlm: model specifications ('linear', 'quadratic', formulas) are not supported; fit a linear model on the given columns.",
        ));
    }
    let o = name_values(&a, 2, "fitlm")?;
    for (k, _) in &o {
        if k != "intercept" {
            return Err(err(
                "stats:fitlm:Unsupported",
                format!("fitlm: option '{k}' is not supported (only 'Intercept')."),
            ));
        }
    }
    let intercept = match opt(&o, "intercept") {
        Some(v) => v.is_true()?,
        None => true,
    };
    let xm =
        if xn.dims.len() == 2 && xn.dims[0] == 1 && yn.re.len() == xn.re.len() && xn.re.len() > 1 {
            M {
                r: xn.re.len(),
                c: 1,
                d: xn.re.clone(),
            } // row vector predictor
        } else {
            M::from_num(&xn, "fitlm")?
        };
    if yn.re.len() != xm.r {
        return Err(err(
            "stats:fitlm:InputSizeMismatch",
            "fitlm: X and y must have the same number of rows.",
        ));
    }
    let keep: Vec<usize> = (0..xm.r)
        .filter(|&i| !yn.re[i].is_nan() && xm.row(i).iter().all(|t| !t.is_nan()))
        .collect();
    let n = keep.len();
    let np = xm.c;
    let p = np + intercept as usize;
    if n == 0 || p == 0 || n < p {
        return Err(err(
            "stats:fitlm:NotEnoughData",
            "fitlm: the data need more complete observations than coefficients.",
        ));
    }
    let mut x = M::zeros(n, p);
    for (r, &i) in keep.iter().enumerate() {
        let mut c = 0;
        if intercept {
            x.set(r, 0, 1.0);
            c = 1;
        }
        for j in 0..np {
            x.set(r, c + j, xm.at(i, j));
        }
    }
    let y: Vec<f64> = keep.iter().map(|&i| yn.re[i]).collect();
    let f = fit(&x, &y);
    let dfe = n as f64 - f.rank as f64;
    let mse = f.sse / dfe;
    let ym = mean(&y);
    let sst: f64 = if intercept {
        y.iter().map(|v| (v - ym).powi(2)).sum()
    } else {
        y.iter().map(|v| v * v).sum()
    };
    let dft = if intercept { n as f64 - 1.0 } else { n as f64 };
    let se: Vec<f64> = (0..p)
        .map(|j| (f.cov_unscaled.at(j, j) * mse).sqrt())
        .collect();
    let tstat: Vec<f64> = f.beta.iter().zip(&se).map(|(b, s)| b / s).collect();
    let tdist = dist::find("t").unwrap();
    let pval: Vec<f64> = tstat
        .iter()
        .map(|t| 2.0 * (tdist.cdf)(-t.abs(), &[dfe]).0)
        .collect();
    let names: Vec<Value> = (0..p)
        .map(|j| {
            if intercept && j == 0 {
                strv("(Intercept)")
            } else {
                strv(&format!("x{}", j + 1 - intercept as usize))
            }
        })
        .collect();
    let r2 = 1.0 - f.sse / sst;
    let adj = 1.0 - (f.sse / dfe) / (sst / dft);
    let mut ob = Object::new("LinearModel");
    ob.set(
        "Coefficients",
        st(vec![
            ("Estimate", Value::col(f.beta.clone())),
            ("SE", Value::col(se)),
            ("tStat", Value::col(tstat)),
            ("pValue", Value::col(pval)),
        ]),
    );
    ob.set("CoefficientNames", cell(vec![1, p], names));
    ob.set("NumCoefficients", Value::num(p as f64));
    ob.set("NumEstimatedCoefficients", Value::num(f.rank as f64));
    ob.set("NumObservations", Value::num(n as f64));
    ob.set("NumPredictors", Value::num(np as f64));
    ob.set("DFE", Value::num(dfe));
    ob.set("SSE", Value::num(f.sse));
    ob.set("SST", Value::num(sst));
    ob.set("SSR", Value::num(sst - f.sse));
    ob.set("MSE", Value::num(mse));
    ob.set("RMSE", Value::num(mse.sqrt()));
    ob.set(
        "Rsquared",
        st(vec![
            ("Ordinary", Value::num(r2)),
            ("Adjusted", Value::num(adj)),
        ]),
    );
    ob.set("Fitted", Value::col(f.fitted.clone()));
    ob.set(
        "Residuals",
        st(vec![
            ("Raw", Value::col(f.resid.clone())),
            (
                "Standardized",
                Value::col(
                    f.resid
                        .iter()
                        .zip(&f.leverage)
                        .map(|(e, h)| e / (mse * (1.0 - h)).sqrt())
                        .collect(),
                ),
            ),
        ]),
    );
    ob.set("Intercept__", Value::bool(intercept));
    ob.set("CovUnscaled__", f.cov_unscaled.value());
    one(Value::Object(Box::new(ob)))
}

fn linear_parts(o: &Object) -> R<(Vec<f64>, M, f64, f64, bool)> {
    let est = o
        .get("Coefficients")
        .and_then(|c| match c {
            Value::Struct(s) => s.get(0, "Estimate").and_then(|v| v.to_f64_vec()),
            _ => None,
        })
        .ok_or_else(|| {
            err(
                "stats:LinearModel:Corrupt",
                "LinearModel: missing coefficients.",
            )
        })?;
    let cov = M::from_num(
        &real_arg(
            o.get("CovUnscaled__").unwrap_or(&Value::empty()),
            "LinearModel",
        )?,
        "LinearModel",
    )?;
    let mse = o
        .get("MSE")
        .and_then(|v| v.scalar_f64())
        .unwrap_or(f64::NAN);
    let dfe = o
        .get("DFE")
        .and_then(|v| v.scalar_f64())
        .unwrap_or(f64::NAN);
    let intercept = o
        .get("Intercept__")
        .is_some_and(|v| v.is_true().unwrap_or(true));
    Ok((est, cov, mse, dfe, intercept))
}

pub fn predict_linear(_: &mut Interp, o: &Object, a: &[Value], nargout: usize) -> R<Vec<Value>> {
    let (beta, cov, mse, dfe, intercept) = linear_parts(o)?;
    let xn = real_arg(&a[0], "predict")?;
    let xq = M::from_num(&xn, "predict")?;
    let np = beta.len() - intercept as usize;
    let xq = if xq.c != np && xq.r == np && np == 1 {
        M {
            r: xq.c,
            c: 1,
            d: xq.d,
        }
    } else {
        xq
    };
    if xq.c != np {
        return Err(err(
            "stats:predict:BadPredictor",
            "predict: Xnew must have one column per predictor.",
        ));
    }
    let o2 = name_values(a, 1, "predict")?;
    let alpha = match opt(&o2, "alpha") {
        Some(v) => arg_f64(v, "predict")?,
        None => 0.05,
    };
    if !(alpha > 0.0 && alpha < 1.0) {
        return Err(err(
            "stats:predict:BadAlpha",
            "predict: 'Alpha' must be a scalar between 0 and 1 exclusive.",
        ));
    }
    let t = tq(1.0 - alpha / 2.0, dfe);
    let mut y = Vec::new();
    let mut ci = M::zeros(xq.r, 2);
    for i in 0..xq.r {
        let mut row = Vec::new();
        if intercept {
            row.push(1.0);
        }
        row.extend(xq.row(i));
        let yh: f64 = row.iter().zip(&beta).map(|(a, b)| a * b).sum();
        let mut q = 0.0;
        for p in 0..row.len() {
            for r in 0..row.len() {
                q += row[p] * cov.at(p, r) * row[r];
            }
        }
        let w = t * (q * mse).sqrt();
        y.push(yh);
        ci.set(i, 0, yh - w);
        ci.set(i, 1, yh + w);
    }
    let mut res = vec![Value::col(y)];
    if nargout > 1 {
        res.push(ci.value());
    }
    Ok(res)
}

fn coef_ci(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "coefCI")?;
    let Value::Object(o) = &a[0] else {
        return Err(err(
            "stats:coefCI:BadModel",
            "coefCI: the first argument must be a LinearModel.",
        ));
    };
    if o.class != "LinearModel" {
        return Err(err(
            "stats:coefCI:BadModel",
            "coefCI: the first argument must be a LinearModel.",
        ));
    }
    let alpha = match a.get(1) {
        Some(v) => arg_f64(v, "coefCI")?,
        None => 0.05,
    };
    if !(alpha > 0.0 && alpha < 1.0) {
        return Err(err(
            "stats:coefCI:BadAlpha",
            "coefCI: ALPHA must be a scalar between 0 and 1 exclusive.",
        ));
    }
    let (beta, cov, mse, dfe, _) = linear_parts(o)?;
    let t = tq(1.0 - alpha / 2.0, dfe);
    let p = beta.len();
    let mut ci = M::zeros(p, 2);
    for j in 0..p {
        let se = (cov.at(j, j) * mse).sqrt();
        ci.set(j, 0, beta[j] - t * se);
        ci.set(j, 1, beta[j] + t * se);
    }
    one(ci.value())
}
