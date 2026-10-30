//! Descriptive statistics of the Statistics and Machine Learning Toolbox:
//! skewness, kurtosis, geomean, harmmean, trimmean, mad, range, zscore,
//! tabulate, crosstab, grpstats and corr.

use indy_value::{Num, Value, to_string_array};

use super::dist::Dist;
use super::su::*;
use super::{arg_f64, arg_num, arg_str, err, need, one, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "skewness", |_, a, _| shape_stat(&a, "skewness", false));
    reg(it, "kurtosis", |_, a, _| shape_stat(&a, "kurtosis", true));
    reg(it, "geomean", |_, a, _| means(&a, "geomean"));
    reg(it, "harmmean", |_, a, _| means(&a, "harmmean"));
    reg(it, "trimmean", |_, a, _| trimmean(&a));
    reg(it, "mad", |_, a, _| mad(&a));
    reg(it, "range", |_, a, _| range(&a));
    reg(it, "zscore", zscore);
    reg(it, "tabulate", tabulate);
    reg(it, "crosstab", crosstab);
    reg(it, "grpstats", grpstats);
    reg(it, "corr", corr);
}

/// `(x, <extra args>)` with a trailing `'omitnan'`/`'includenan'` removed.
fn split_nan(a: &[Value]) -> (Vec<Value>, bool) {
    let mut omit = false;
    let rest = a
        .iter()
        .filter(|v| match v.to_str().map(|s| s.to_lowercase()).as_deref() {
            Some("omitnan") => {
                omit = true;
                false
            }
            Some("includenan") => false,
            _ => true,
        })
        .cloned()
        .collect();
    (rest, omit)
}

fn keep(slice: &[f64], omit: bool) -> Vec<f64> {
    slice
        .iter()
        .copied()
        .filter(|v| !(omit && v.is_nan()))
        .collect()
}

fn shape_stat(a: &[Value], fname: &str, kurt: bool) -> R<Vec<Value>> {
    need(a, 1, fname)?;
    let (a, omit) = split_nan(a);
    let x = real_arg(&a[0], fname)?;
    let flag = match a.get(1) {
        Some(v) if !v.is_empty() => arg_f64(v, fname)?,
        _ => 1.0,
    };
    if flag != 0.0 && flag != 1.0 {
        return Err(err(
            "stats:skewness:BadFlag",
            format!("{fname}: FLAG must be 0 or 1."),
        ));
    }
    let dim = dim_arg(&x, a.get(2), fname)?;
    let out = along(&x, dim, 1, |s| {
        let v = keep(s, omit);
        let n = v.len() as f64;
        if v.is_empty() {
            return vec![f64::NAN];
        }
        let m = mean(&v);
        let m2 = v.iter().map(|t| (t - m).powi(2)).sum::<f64>() / n;
        if m2 == 0.0 || m2.is_nan() {
            return vec![f64::NAN];
        }
        if !kurt {
            let m3 = v.iter().map(|t| (t - m).powi(3)).sum::<f64>() / n;
            let mut r = m3 / m2.powf(1.5);
            if flag == 0.0 {
                r = if n > 2.0 {
                    r * (n * (n - 1.0)).sqrt() / (n - 2.0)
                } else {
                    f64::NAN
                };
            }
            vec![r]
        } else {
            let m4 = v.iter().map(|t| (t - m).powi(4)).sum::<f64>() / n;
            let mut r = m4 / (m2 * m2);
            if flag == 0.0 {
                r = if n > 3.0 {
                    3.0 + (n - 1.0) / ((n - 2.0) * (n - 3.0)) * ((n + 1.0) * r - 3.0 * (n - 1.0))
                } else {
                    f64::NAN
                };
            }
            vec![r]
        }
    });
    one(Value::Num(cls(out, &[&x])))
}

fn means(a: &[Value], fname: &str) -> R<Vec<Value>> {
    need(a, 1, fname)?;
    let (a, omit) = split_nan(a);
    let x = real_arg(&a[0], fname)?;
    let dim = dim_arg(&x, a.get(1), fname)?;
    let geo = fname == "geomean";
    let out = along(&x, dim, 1, |s| {
        let v = keep(s, omit);
        if v.is_empty() {
            return vec![f64::NAN];
        }
        let n = v.len() as f64;
        vec![if geo {
            if v.iter().any(|t| *t < 0.0) {
                f64::NAN
            } else {
                (v.iter().map(|t| t.ln()).sum::<f64>() / n).exp()
            }
        } else {
            n / v.iter().map(|t| 1.0 / t).sum::<f64>()
        }]
    });
    one(Value::Num(cls(out, &[&x])))
}

fn trimmean(a: &[Value]) -> R<Vec<Value>> {
    need(a, 2, "trimmean")?;
    let x = real_arg(&a[0], "trimmean")?;
    let pct = arg_f64(&a[1], "trimmean")?;
    if !(0.0..100.0).contains(&pct) {
        return Err(err(
            "stats:trimmean:BadPercent",
            "trimmean: PERCENT must be a scalar in [0, 100).",
        ));
    }
    let mut mode = "round".to_string();
    let mut dim_v = None;
    for v in &a[2..] {
        match v.to_str() {
            Some(s) => mode = s.to_lowercase(),
            None => dim_v = Some(v),
        }
    }
    if !matches!(mode.as_str(), "round" | "floor") {
        return Err(err(
            "stats:trimmean:BadFlag",
            "trimmean: FLAG must be 'round' or 'floor'.",
        ));
    }
    let dim = dim_arg(&x, dim_v, "trimmean")?;
    let out = along(&x, dim, 1, |s| {
        let mut v: Vec<f64> = s.iter().copied().filter(|t| !t.is_nan()).collect();
        let n = v.len() as f64;
        if v.is_empty() {
            return vec![f64::NAN];
        }
        v.sort_by(f64::total_cmp);
        let half = n * pct / 100.0 / 2.0;
        let k = if mode == "round" {
            half.round()
        } else {
            half.floor()
        } as usize;
        if 2 * k >= v.len() {
            return vec![f64::NAN];
        }
        vec![mean(&v[k..v.len() - k])]
    });
    one(Value::Num(cls(out, &[&x])))
}

fn mad(a: &[Value]) -> R<Vec<Value>> {
    need(a, 1, "mad")?;
    let (a, omit) = split_nan(a);
    let mut x = real_arg(&a[0], "mad")?;
    let flag = match a.get(1) {
        Some(v) if !v.is_empty() => arg_f64(v, "mad")?,
        _ => 0.0,
    };
    let mut dim_v = a.get(2);
    if let Some(v) = dim_v
        && v.to_str().is_some_and(|s| s.eq_ignore_ascii_case("all"))
    {
        x = Num::real(vec![x.re.len(), 1], x.re.clone());
        dim_v = None;
    }
    let dim = dim_arg(&x, dim_v, "mad")?;
    let out = along(&x, dim, 1, |s| {
        let v = keep(s, omit);
        if v.is_empty() || v.iter().any(|t| t.is_nan()) {
            return vec![f64::NAN];
        }
        if flag == 0.0 {
            let m = mean(&v);
            vec![v.iter().map(|t| (t - m).abs()).sum::<f64>() / v.len() as f64]
        } else {
            let m = median(&v);
            let d: Vec<f64> = v.iter().map(|t| (t - m).abs()).collect();
            vec![median(&d)]
        }
    });
    one(Value::Num(cls(out, &[&x])))
}

fn range(a: &[Value]) -> R<Vec<Value>> {
    need(a, 1, "range")?;
    let (a, omit) = split_nan(a);
    let x = real_arg(&a[0], "range")?;
    let dim = dim_arg(&x, a.get(1), "range")?;
    let out = along(&x, dim, 1, |s| {
        let v = keep(s, true);
        if v.is_empty() || (!omit && v.len() != s.len()) {
            return vec![f64::NAN];
        }
        let mx = v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let mn = v.iter().copied().fold(f64::INFINITY, f64::min);
        vec![mx - mn]
    });
    one(Value::Num(cls(out, &[&x])))
}

fn zscore(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "zscore")?;
    let x = real_arg(&a[0], "zscore")?;
    let flag = match a.get(1) {
        Some(v) if !v.is_empty() => arg_f64(v, "zscore")?,
        _ => 0.0,
    };
    if flag != 0.0 && flag != 1.0 {
        return Err(err("stats:zscore:BadFlag", "zscore: FLAG must be 0 or 1."));
    }
    let dim = dim_arg(&x, a.get(2), "zscore")?;
    let (mut mus, mut sds) = (Vec::new(), Vec::new());
    let len = x.dims.get(dim).copied().unwrap_or(1);
    let z = along(&x, dim, len, |s| {
        let n = s.len() as f64;
        let mu = mean(s);
        let ss: f64 = s.iter().map(|t| (t - mu).powi(2)).sum();
        let sd = if flag == 1.0 || n <= 1.0 {
            (ss / n).sqrt()
        } else {
            (ss / (n - 1.0)).sqrt()
        };
        mus.push(mu);
        sds.push(sd);
        let div = if sd == 0.0 { 1.0 } else { sd };
        s.iter().map(|t| (t - mu) / div).collect()
    });
    let mut od = x.dims.clone();
    while od.len() <= dim {
        od.push(1);
    }
    od[dim] = 1;
    let od = indy_value::norm_dims(od);
    Ok(vec![
        Value::Num(cls(z, &[&x])),
        Value::Num(cls(Num::real(od.clone(), mus), &[&x])),
        Value::Num(cls(Num::real(od, sds), &[&x])),
    ])
}

// ---------------------------------------------------------------------------
// tabulate / crosstab / grpstats
// ---------------------------------------------------------------------------

/// A grouping variable reduced to level indices and level labels.
struct Levels {
    idx: Vec<usize>,
    labels: Vec<String>,
    numeric: Option<Vec<f64>>,
}

fn levels_of(v: &Value, fname: &str) -> R<Levels> {
    match v {
        Value::Cell(c) => {
            let strs: Vec<String> = c.data.iter().map(|e| arg_str(e, fname)).collect::<R<_>>()?;
            Ok(from_strings(strs))
        }
        Value::Str(_) => {
            let arr = to_string_array(v);
            let strs = arr
                .data
                .iter()
                .map(|s| s.clone().unwrap_or_default())
                .collect();
            Ok(from_strings(strs))
        }
        Value::Char(_) => {
            let rows = v.char_rows().unwrap_or_default();
            Ok(from_strings(rows))
        }
        _ => {
            let n = arg_num(v, fname)?;
            let mut u: Vec<f64> = n.re.iter().copied().filter(|t| !t.is_nan()).collect();
            u.sort_by(f64::total_cmp);
            u.dedup();
            let idx =
                n.re.iter()
                    .map(|t| {
                        if t.is_nan() {
                            usize::MAX
                        } else {
                            u.partition_point(|q| q < t)
                        }
                    })
                    .collect();
            let labels = u.iter().map(|t| fmt_g(*t)).collect();
            Ok(Levels {
                idx,
                labels,
                numeric: Some(u),
            })
        }
    }
}

fn from_strings(strs: Vec<String>) -> Levels {
    let mut u = strs.clone();
    u.sort();
    u.dedup();
    let idx = strs
        .iter()
        .map(|s| u.binary_search(s).unwrap_or(usize::MAX))
        .collect();
    Levels {
        idx,
        labels: u,
        numeric: None,
    }
}

fn fmt_g(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        let s = format!("{:.5}", x);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn tabulate(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "tabulate")?;
    let lv = levels_of(&a[0], "tabulate")?;
    let k = lv.labels.len();
    let mut counts = vec![0.0; k];
    for &i in &lv.idx {
        if i != usize::MAX {
            counts[i] += 1.0;
        }
    }
    let n: f64 = counts.iter().sum();
    let pct: Vec<f64> = counts.iter().map(|c| 100.0 * c / n).collect();
    if nargout == 0 {
        let mut s = String::from("  Value    Count   Percent\n");
        for i in 0..k {
            s.push_str(&format!(
                " {:>6} {:>8} {:>8.2}%\n",
                lv.labels[i], counts[i] as i64, pct[i]
            ));
        }
        it.print(&s);
        return Ok(vec![]);
    }
    let out = if let Some(u) = &lv.numeric {
        let mut d = Vec::with_capacity(3 * k);
        d.extend(u.iter());
        d.extend(counts.iter());
        d.extend(pct.iter());
        mat(k, 3, d)
    } else {
        let mut c = Vec::with_capacity(3 * k);
        for l in &lv.labels {
            c.push(strv(l));
        }
        for v in &counts {
            c.push(Value::num(*v));
        }
        for v in &pct {
            c.push(Value::num(*v));
        }
        cell(vec![k, 3], c)
    };
    let mut res = vec![out];
    if nargout > 1 {
        res.push(Value::empty());
    }
    Ok(res)
}

fn crosstab(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "crosstab")?;
    let lvs: Vec<Levels> = a
        .iter()
        .map(|v| levels_of(v, "crosstab"))
        .collect::<R<_>>()?;
    let n = lvs[0].idx.len();
    if lvs.iter().any(|l| l.idx.len() != n) {
        return Err(err(
            "stats:crosstab:InputSizeMismatch",
            "crosstab: all inputs must have the same number of observations.",
        ));
    }
    let dims: Vec<usize> = lvs.iter().map(|l| l.labels.len()).collect();
    let total: usize = dims.iter().product();
    let mut tab = vec![0.0; total];
    let mut used = 0.0;
    'obs: for i in 0..n {
        let mut lin = 0;
        let mut stride = 1;
        for (l, d) in lvs.iter().zip(&dims) {
            let ix = l.idx[i];
            if ix == usize::MAX {
                continue 'obs;
            }
            lin += ix * stride;
            stride *= d;
        }
        tab[lin] += 1.0;
        used += 1.0;
    }
    // chi-square test of independence
    let mut marg: Vec<Vec<f64>> = dims.iter().map(|d| vec![0.0; *d]).collect();
    for (lin, c) in tab.iter().enumerate() {
        let mut rem = lin;
        for (m, d) in marg.iter_mut().zip(&dims) {
            m[rem % d] += c;
            rem /= d;
        }
    }
    let mut chi = 0.0;
    for (lin, c) in tab.iter().enumerate() {
        let mut rem = lin;
        let mut e = used;
        for (m, d) in marg.iter().zip(&dims) {
            e *= m[rem % d] / used;
            rem /= d;
        }
        if e > 0.0 {
            chi += (c - e).powi(2) / e;
        }
    }
    let df = (total as f64 - 1.0) - dims.iter().map(|d| *d as f64 - 1.0).sum::<f64>();
    let chi2 = super::dist::find("chi2").map_or(f64::NAN, |d: &Dist| (d.cdf)(chi, &[df]).1);
    let p = if df > 0.0 { chi2 } else { f64::NAN };
    let shape = if dims.len() == 1 {
        vec![dims[0], 1]
    } else {
        dims.clone()
    };
    let maxl = lvs.len();
    let rows = dims.iter().copied().max().unwrap_or(0);
    let mut labs = Vec::with_capacity(rows * maxl);
    for j in 0..maxl {
        for i in 0..rows {
            labs.push(if i < dims[j] {
                strv(&lvs[j].labels[i])
            } else {
                Value::str("")
            });
        }
    }
    Ok(vec![
        Value::Num(Num::real(shape, tab)),
        Value::num(if df > 0.0 { chi } else { 0.0 }),
        Value::num(p),
        cell(vec![rows, maxl], labs),
    ])
}

fn grpstats(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "grpstats")?;
    let x = real_arg(&a[0], "grpstats")?;
    let xm = M::from_num(&x, "grpstats")?;
    let lv = levels_of(&a[1], "grpstats")?;
    if lv.idx.len() != xm.r {
        return Err(err(
            "stats:grpstats:InputSizeMismatch",
            "grpstats: GROUP must have one entry per row of X.",
        ));
    }
    let default = ["mean", "sem", "numel", "gname", "std", "var"];
    let wanted: Vec<String> = match a.get(2) {
        Some(Value::Cell(c)) => c
            .data
            .iter()
            .map(|e| arg_str(e, "grpstats").map(|s| s.to_lowercase()))
            .collect::<R<_>>()?,
        Some(v) if v.is_text() => vec![arg_str(v, "grpstats")?.to_lowercase()],
        Some(_) => {
            return Err(err(
                "stats:grpstats:BadStats",
                "grpstats: WHICHSTATS must be a name or a cell array of names.",
            ));
        }
        None => default.iter().map(|s| s.to_string()).collect(),
    };
    let g = lv.labels.len();
    let p = xm.c;
    let mut out = Vec::new();
    for w in wanted.iter().take(nargout.max(1)) {
        if w == "gname" {
            out.push(cell(
                vec![g, 1],
                lv.labels.iter().map(|l| strv(l)).collect(),
            ));
            continue;
        }
        let mut d = vec![0.0; g * p];
        for j in 0..p {
            for gi in 0..g {
                let v: Vec<f64> = (0..xm.r)
                    .filter(|&i| lv.idx[i] == gi)
                    .map(|i| xm.at(i, j))
                    .collect();
                let n = v.len() as f64;
                d[gi + j * g] = match w.as_str() {
                    "mean" => mean(&v),
                    "numel" => n,
                    "count" => v.iter().filter(|t| !t.is_nan()).count() as f64,
                    "sem" => (var(&v) / n).sqrt(),
                    "std" => var(&v).sqrt(),
                    "var" => var(&v),
                    "min" => v.iter().copied().fold(f64::INFINITY, f64::min),
                    "max" => v.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                    "range" => {
                        v.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                            - v.iter().copied().fold(f64::INFINITY, f64::min)
                    }
                    other => {
                        return Err(err(
                            "stats:grpstats:BadStats",
                            format!("grpstats: unsupported statistic '{other}'."),
                        ));
                    }
                };
            }
        }
        out.push(mat(g, p, d));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// corr
// ---------------------------------------------------------------------------

fn pearson(u: &[f64], v: &[f64]) -> f64 {
    let (mu, mv) = (mean(u), mean(v));
    let (mut suv, mut suu, mut svv) = (0.0, 0.0, 0.0);
    for (a, b) in u.iter().zip(v) {
        suv += (a - mu) * (b - mv);
        suu += (a - mu).powi(2);
        svv += (b - mv).powi(2);
    }
    let r = suv / (suu * svv).sqrt();
    r.clamp(-1.0, 1.0)
}

fn kendall(u: &[f64], v: &[f64]) -> f64 {
    let n = u.len();
    let (mut s, mut tu, mut tv) = (0.0, 0.0, 0.0);
    for i in 0..n {
        for j in i + 1..n {
            let (a, b) = ((u[i] - u[j]).signum(), (v[i] - v[j]).signum());
            let (a, b) = (
                if u[i] == u[j] { 0.0 } else { a },
                if v[i] == v[j] { 0.0 } else { b },
            );
            s += a * b;
            tu += a.abs();
            tv += b.abs();
        }
    }
    s / (tu * tv).sqrt()
}

fn corr(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "corr")?;
    let xn = real_arg(&a[0], "corr")?;
    let x = M::from_num(&xn, "corr")?;
    let mut k = 1;
    let mut ins = vec![xn];
    let y = match a.get(1) {
        Some(v) if !v.is_text() => {
            k = 2;
            let yn = real_arg(v, "corr")?;
            let ym = M::from_num(&yn, "corr")?;
            ins.push(yn);
            Some(ym)
        }
        _ => None,
    };
    let o = name_values(&a, k, "corr")?;
    for (name, _) in &o {
        if !["type", "rows", "tail"].contains(&name.as_str()) {
            return Err(err(
                "stats:corr:InvalidOption",
                format!("corr: unknown option '{name}'."),
            ));
        }
    }
    let tail = opt_str(&o, "tail", "corr")?.unwrap_or_else(|| "both".into());
    if !matches!(tail.as_str(), "both" | "right" | "left") {
        return Err(err(
            "stats:corr:BadTail",
            "corr: 'Tail' must be 'both', 'right' or 'left'.",
        ));
    }
    let kind = opt_str(&o, "type", "corr")?.unwrap_or_else(|| "pearson".into());
    let rows = opt_str(&o, "rows", "corr")?.unwrap_or_else(|| "all".into());
    if !matches!(kind.as_str(), "pearson" | "spearman" | "kendall") {
        return Err(err(
            "stats:corr:BadType",
            "corr: 'Type' must be 'Pearson', 'Spearman' or 'Kendall'.",
        ));
    }
    if !matches!(rows.as_str(), "all" | "complete" | "pairwise") {
        return Err(err(
            "stats:corr:BadRows",
            "corr: 'Rows' must be 'all', 'complete' or 'pairwise'.",
        ));
    }
    if nargout > 1 && kind != "pearson" {
        return Err(err(
            "stats:corr:PValueUnsupported",
            "corr: p-values are only available for Type 'Pearson' in the Rust engine.",
        ));
    }
    let same = y.is_none();
    let yy = y.as_ref().unwrap_or(&x);
    if yy.r != x.r {
        return Err(err(
            "stats:corr:InputSizeMismatch",
            "corr: X and Y must have the same number of rows.",
        ));
    }
    let n = x.r;
    let complete: Vec<bool> = (0..n)
        .map(|i| x.row(i).iter().all(|t| !t.is_nan()) && yy.row(i).iter().all(|t| !t.is_nan()))
        .collect();
    let mut rho = M::zeros(x.c, yy.c);
    let mut pv = M::zeros(x.c, yy.c);
    for i in 0..x.c {
        for j in 0..yy.c {
            let sel: Vec<usize> = (0..n)
                .filter(|&r| match rows.as_str() {
                    "all" => true,
                    "complete" => complete[r],
                    _ => !x.at(r, i).is_nan() && !yy.at(r, j).is_nan(),
                })
                .collect();
            let u: Vec<f64> = sel.iter().map(|&r| x.at(r, i)).collect();
            let v: Vec<f64> = sel.iter().map(|&r| yy.at(r, j)).collect();
            let r = if u.iter().chain(&v).any(|t| t.is_nan()) || u.len() < 2 {
                f64::NAN
            } else {
                match kind.as_str() {
                    "pearson" => pearson(&u, &v),
                    "spearman" => pearson(&ranks(&u).0, &ranks(&v).0),
                    _ => kendall(&u, &v),
                }
            };
            rho.set(i, j, r);
            let df = u.len() as f64 - 2.0;
            let p = if r.is_nan() || df < 1.0 {
                f64::NAN
            } else if r.abs() >= 1.0 {
                match (tail.as_str(), r > 0.0) {
                    ("right", false) | ("left", true) => 1.0,
                    _ => 0.0,
                }
            } else {
                let t = r * (df / (1.0 - r * r)).sqrt();
                let (lo, up) = super::dist::find("t")
                    .map_or((f64::NAN, f64::NAN), |d: &Dist| (d.cdf)(t, &[df]));
                match tail.as_str() {
                    "right" => up,
                    "left" => lo,
                    _ => (2.0 * lo.min(up)).min(1.0),
                }
            };
            pv.set(i, j, p);
        }
    }
    if same {
        for i in 0..x.c {
            if !rho.at(i, i).is_nan() {
                rho.set(i, i, 1.0);
            }
        }
    }
    let refs: Vec<&Num> = ins.iter().collect();
    let as_num = |m: M| cls(Num::real(vec![m.r, m.c], m.d), &refs);
    let mut res = vec![Value::Num(as_num(rho))];
    if nargout > 1 {
        res.push(Value::Num(as_num(pv)));
    }
    Ok(res)
}
