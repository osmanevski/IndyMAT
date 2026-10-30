//! Multivariate statistics: `pdist`, `pdist2`, `squareform`, `linkage`,
//! `cluster`, `inconsistent`, `kmeans`, `pca`, `ecdf`, `fitcknn` /
//! `predict`, `cvpartition` (+ `training`/`test`), `randsample`,
//! `mvnpdf`, `mvnrnd`.

use indy_value::{Num, Object, Value};

use super::linalg::jacobi_svd;
use super::su::*;
use super::{arg_f64, arg_num, arg_str, err, need, one, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "pdist", pdist);
    reg(it, "pdist2", pdist2);
    reg(it, "squareform", squareform);
    reg(it, "linkage", linkage);
    reg(it, "cluster", cluster);
    reg(it, "inconsistent", inconsistent);
    reg(it, "kmeans", kmeans);
    reg(it, "pca", pca);
    reg(it, "ecdf", ecdf);
    reg(it, "fitcknn", fitcknn);
    reg(it, "predict", predict);
    reg(it, "cvpartition", cvpartition);
    reg(it, "training", |_, a, _| cv_sets(&a, true));
    reg(it, "test", |_, a, _| cv_sets(&a, false));
    reg(it, "randsample", randsample);
    reg(it, "mvnpdf", mvnpdf);
    reg(it, "mvnrnd", mvnrnd);
}

// ---------------------------------------------------------------------------
// distances
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum Metric {
    Euclidean,
    SqEuclidean,
    SEuclidean(Vec<f64>),
    CityBlock,
    Minkowski(f64),
    Chebychev,
    Mahalanobis(M),
    Cosine,
    Correlation,
    Spearman,
    Hamming,
    Jaccard,
}

fn metric(name: &str, p: Option<f64>, x: &M, fname: &str) -> R<Metric> {
    Ok(match name {
        "euclidean" => Metric::Euclidean,
        "squaredeuclidean" | "sqeuclidean" => Metric::SqEuclidean,
        "seuclidean" => Metric::SEuclidean(
            (0..x.c)
                .map(|j| {
                    let c: Vec<f64> = x.col(j).to_vec();
                    var(&c)
                })
                .collect(),
        ),
        "cityblock" => Metric::CityBlock,
        "minkowski" => Metric::Minkowski(p.unwrap_or(2.0)),
        "chebychev" | "chebyshev" => Metric::Chebychev,
        "mahalanobis" => {
            // inverse covariance of the columns
            let n = x.r as f64;
            let mut cov = M::zeros(x.c, x.c);
            let means: Vec<f64> = (0..x.c).map(|j| mean(x.col(j))).collect();
            for i in 0..x.c {
                for j in 0..x.c {
                    let s: f64 = (0..x.r)
                        .map(|r| (x.at(r, i) - means[i]) * (x.at(r, j) - means[j]))
                        .sum();
                    cov.set(i, j, s / (n - 1.0));
                }
            }
            let mut inv = M::zeros(x.c, x.c);
            for j in 0..x.c {
                let mut e = vec![0.0; x.c];
                e[j] = 1.0;
                let col = chol_solve(&cov, &e).ok_or_else(|| {
                    err(
                        "stats:pdist:SingularCov",
                        format!("{fname}: the covariance matrix is not positive definite."),
                    )
                })?;
                for (i, v) in col.iter().enumerate() {
                    inv.set(i, j, *v);
                }
            }
            Metric::Mahalanobis(inv)
        }
        "cosine" => Metric::Cosine,
        "correlation" => Metric::Correlation,
        "spearman" => Metric::Spearman,
        "hamming" => Metric::Hamming,
        "jaccard" => Metric::Jaccard,
        other => {
            return Err(err(
                "stats:pdist:UnrecognizedDistance",
                format!("{fname}: unrecognized or unsupported distance '{other}'."),
            ));
        }
    })
}

fn dist_between(m: &Metric, u: &[f64], v: &[f64]) -> f64 {
    match m {
        Metric::Euclidean => u
            .iter()
            .zip(v)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>()
            .sqrt(),
        Metric::SqEuclidean => u.iter().zip(v).map(|(a, b)| (a - b).powi(2)).sum(),
        Metric::SEuclidean(s) => u
            .iter()
            .zip(v)
            .zip(s)
            .map(|((a, b), s)| (a - b).powi(2) / s)
            .sum::<f64>()
            .sqrt(),
        Metric::CityBlock => u.iter().zip(v).map(|(a, b)| (a - b).abs()).sum(),
        Metric::Minkowski(p) => u
            .iter()
            .zip(v)
            .map(|(a, b)| (a - b).abs().powf(*p))
            .sum::<f64>()
            .powf(1.0 / p),
        Metric::Chebychev => u
            .iter()
            .zip(v)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max),
        Metric::Mahalanobis(inv) => {
            let d: Vec<f64> = u.iter().zip(v).map(|(a, b)| a - b).collect();
            let mut s = 0.0;
            for i in 0..d.len() {
                for j in 0..d.len() {
                    s += d[i] * inv.at(i, j) * d[j];
                }
            }
            s.max(0.0).sqrt()
        }
        Metric::Cosine => {
            let dot: f64 = u.iter().zip(v).map(|(a, b)| a * b).sum();
            let nu: f64 = u.iter().map(|a| a * a).sum::<f64>().sqrt();
            let nv: f64 = v.iter().map(|a| a * a).sum::<f64>().sqrt();
            1.0 - dot / (nu * nv)
        }
        Metric::Correlation => {
            let (mu, mv) = (mean(u), mean(v));
            let (mut suv, mut suu, mut svv) = (0.0, 0.0, 0.0);
            for (a, b) in u.iter().zip(v) {
                suv += (a - mu) * (b - mv);
                suu += (a - mu).powi(2);
                svv += (b - mv).powi(2);
            }
            1.0 - suv / (suu * svv).sqrt()
        }
        Metric::Spearman => {
            let (ru, rv) = (ranks(u).0, ranks(v).0);
            dist_between(&Metric::Correlation, &ru, &rv)
        }
        Metric::Hamming => u.iter().zip(v).filter(|(a, b)| a != b).count() as f64 / u.len() as f64,
        Metric::Jaccard => {
            let nz = u
                .iter()
                .zip(v)
                .filter(|(a, b)| **a != 0.0 || **b != 0.0)
                .count();
            let diff = u
                .iter()
                .zip(v)
                .filter(|(a, b)| (**a != 0.0 || **b != 0.0) && a != b)
                .count();
            if nz == 0 {
                0.0
            } else {
                diff as f64 / nz as f64
            }
        }
    }
}

/// Parse `(distance, extra)` options shared by pdist/pdist2.
fn metric_args(a: &[Value], at: usize, x: &M, fname: &str) -> R<Metric> {
    let name = match a.get(at) {
        Some(v) if v.is_text() => arg_str(v, fname)?.to_lowercase(),
        Some(_) => {
            return Err(err(
                "stats:pdist:CustomDistance",
                format!("{fname}: function-handle distances are not supported."),
            ));
        }
        None => "euclidean".into(),
    };
    let p = match a.get(at + 1) {
        Some(v) if !v.is_text() => Some(arg_f64(v, fname)?),
        _ => None,
    };
    if p.is_some_and(|t| t.is_nan() || t <= 0.0) {
        return Err(err(
            "stats:pdist:BadExponent",
            format!("{fname}: the Minkowski exponent must be a positive scalar."),
        ));
    }
    metric(&name, p, x, fname)
}

fn pdist(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "pdist")?;
    let xn = real_arg(&a[0], "pdist")?;
    let x = M::from_num(&xn, "pdist")?;
    let m = metric_args(&a, 1, &x, "pdist")?;
    let rows: Vec<Vec<f64>> = (0..x.r).map(|i| x.row(i)).collect();
    let mut out = Vec::with_capacity(x.r * x.r.saturating_sub(1) / 2);
    for i in 0..x.r {
        for j in i + 1..x.r {
            out.push(dist_between(&m, &rows[i], &rows[j]));
        }
    }
    let n = out.len();
    one(Value::Num(cls(Num::real(vec![1, n], out), &[&xn])))
}

fn pdist2(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "pdist2")?;
    let x = M::from_num(&real_arg(&a[0], "pdist2")?, "pdist2")?;
    let y = M::from_num(&real_arg(&a[1], "pdist2")?, "pdist2")?;
    if x.c != y.c {
        return Err(err(
            "stats:pdist2:SizeMismatch",
            "pdist2: X and Y must have the same number of columns.",
        ));
    }
    // optional 'Smallest'/'Largest', k after the distance spec
    let mut at = 2;
    let mut spec: Option<(bool, usize)> = None;
    let metric_end = a.len();
    let mut cut = metric_end;
    while at < a.len() {
        if let Some(s) = a[at].to_str() {
            let s = s.to_lowercase();
            if s == "smallest" || s == "largest" {
                let k = a
                    .get(at + 1)
                    .map(|v| arg_f64(v, "pdist2"))
                    .transpose()?
                    .ok_or_else(|| err("stats:pdist2:MissingK", "pdist2: missing K."))?;
                spec = Some((s == "smallest", k as usize));
                cut = cut.min(at);
                at += 2;
                continue;
            }
        }
        at += 1;
    }
    let m = metric_args(&a[..cut], 2, &x, "pdist2")?;
    let rx: Vec<Vec<f64>> = (0..x.r).map(|i| x.row(i)).collect();
    let ry: Vec<Vec<f64>> = (0..y.r).map(|i| y.row(i)).collect();
    // D is size(X,1) x size(Y,1)
    let mut dmat = M::zeros(x.r, y.r);
    for i in 0..x.r {
        for j in 0..y.r {
            dmat.set(i, j, dist_between(&m, &rx[i], &ry[j]));
        }
    }
    match spec {
        None => {
            let _ = nargout;
            one(dmat.value())
        }
        Some((small, k)) => {
            // result is k x size(Y,1): per observation of Y, the k nearest X
            let k = k.min(x.r);
            let mut d = M::zeros(k, y.r);
            let mut ix = M::zeros(k, y.r);
            for j in 0..y.r {
                let mut ord: Vec<usize> = (0..x.r).collect();
                ord.sort_by(|&p, &q| {
                    let (u, v) = (dmat.at(p, j), dmat.at(q, j));
                    if small {
                        u.total_cmp(&v)
                    } else {
                        v.total_cmp(&u)
                    }
                });
                for (r, &o) in ord.iter().take(k).enumerate() {
                    d.set(r, j, dmat.at(o, j));
                    ix.set(r, j, o as f64 + 1.0);
                }
            }
            Ok(vec![d.value(), ix.value()])
        }
    }
}

fn tri_len_to_m(n: usize) -> Option<usize> {
    // n = m(m-1)/2
    let m = ((1.0 + (1.0 + 8.0 * n as f64).sqrt()) / 2.0).round() as usize;
    (m * (m - 1) / 2 == n).then_some(m)
}

fn squareform(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "squareform")?;
    let x = real_arg(&a[0], "squareform")?;
    let flag = match a.get(1) {
        Some(v) => Some(arg_str(v, "squareform")?.to_lowercase()),
        None => None,
    };
    let is_vec = x.dims.len() == 2 && (x.dims[0] == 1 || x.dims[1] == 1);
    let to_matrix = match flag.as_deref() {
        Some("tomatrix") => true,
        Some("tovector") => false,
        Some(_) => {
            return Err(err(
                "stats:squareform:BadFlag",
                "squareform: second argument must be 'tovector' or 'tomatrix'.",
            ));
        }
        None => is_vec,
    };
    if to_matrix {
        if x.re.is_empty() {
            return one(Value::matrix(vec![1, 1], vec![0.0]));
        }
        let m = tri_len_to_m(x.re.len()).ok_or_else(|| {
            err(
                "stats:squareform:BadVector",
                "squareform: the length of the vector is not m*(m-1)/2 for any integer m.",
            )
        })?;
        let mut out = M::zeros(m, m);
        let mut k = 0;
        for i in 0..m {
            for j in i + 1..m {
                out.set(i, j, x.re[k]);
                out.set(j, i, x.re[k]);
                k += 1;
            }
        }
        one(out.value())
    } else {
        let xm = M::from_num(&x, "squareform")?;
        if xm.r != xm.c {
            return Err(err(
                "stats:squareform:BadMatrix",
                "squareform: the matrix must be square.",
            ));
        }
        let m = xm.r;
        if m == 1 {
            return one(Value::Num(Num::real(vec![1, 0], vec![])));
        }
        for i in 0..m {
            if xm.at(i, i) != 0.0 {
                return Err(err(
                    "stats:squareform:BadMatrix",
                    "squareform: the matrix must have a zero diagonal.",
                ));
            }
            for j in 0..i {
                if xm.at(i, j) != xm.at(j, i) {
                    return Err(err(
                        "stats:squareform:BadMatrix",
                        "squareform: the matrix must be symmetric.",
                    ));
                }
            }
        }
        let mut v = Vec::new();
        for i in 0..m {
            for j in i + 1..m {
                v.push(xm.at(i, j));
            }
        }
        one(Value::row(v))
    }
}

// ---------------------------------------------------------------------------
// hierarchical clustering
// ---------------------------------------------------------------------------

fn linkage(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "linkage")?;
    let y = real_arg(&a[0], "linkage")?;
    let method = match a.get(1) {
        Some(v) if v.is_text() => arg_str(v, "linkage")?.to_lowercase(),
        _ => "single".into(),
    };
    let is_dist_vec = y.dims.len() == 2 && y.dims[0] == 1 && tri_len_to_m(y.re.len()).is_some();
    // distance matrix as a full m×m
    let (m, mut d): (usize, M) = if is_dist_vec {
        let m = tri_len_to_m(y.re.len()).unwrap();
        let mut dm = M::zeros(m, m);
        let mut k = 0;
        for i in 0..m {
            for j in i + 1..m {
                dm.set(i, j, y.re[k]);
                dm.set(j, i, y.re[k]);
                k += 1;
            }
        }
        (m, dm)
    } else {
        let x = M::from_num(&y, "linkage")?;
        if x.r < 2 {
            return Err(err(
                "stats:linkage:TooFewObservations",
                "linkage: at least two observations are required.",
            ));
        }
        let name = match a.get(2) {
            Some(v) if v.is_text() => arg_str(v, "linkage")?.to_lowercase(),
            _ => "euclidean".into(),
        };
        let mt = metric(&name, None, &x, "linkage")?;
        let rows: Vec<Vec<f64>> = (0..x.r).map(|i| x.row(i)).collect();
        let mut dm = M::zeros(x.r, x.r);
        for i in 0..x.r {
            for j in i + 1..x.r {
                let v = dist_between(&mt, &rows[i], &rows[j]);
                dm.set(i, j, v);
                dm.set(j, i, v);
            }
        }
        (x.r, dm)
    };
    if !matches!(
        method.as_str(),
        "single" | "complete" | "average" | "weighted" | "centroid" | "median" | "ward"
    ) {
        return Err(err(
            "stats:linkage:BadMethod",
            format!("linkage: unrecognized method '{method}'."),
        ));
    }
    let squared = matches!(method.as_str(), "centroid" | "median" | "ward");
    if squared {
        for v in d.d.iter_mut() {
            *v = *v * *v;
        }
    }
    let mut active: Vec<bool> = vec![true; m];
    let mut label: Vec<usize> = (0..m).collect(); // current cluster number (0-based)
    let mut size = vec![1.0f64; m];
    let mut z = M::zeros(m - 1, 3);
    for step in 0..m - 1 {
        // closest active pair, ties resolved by scan order (smallest i, then j)
        let (mut bi, mut bj, mut best) = (0, 0, f64::INFINITY);
        for i in 0..m {
            if !active[i] {
                continue;
            }
            for j in i + 1..m {
                if active[j] && d.at(i, j) < best {
                    best = d.at(i, j);
                    bi = i;
                    bj = j;
                }
            }
        }
        let (la, lb) = (label[bi].min(label[bj]), label[bi].max(label[bj]));
        z.set(step, 0, la as f64 + 1.0);
        z.set(step, 1, lb as f64 + 1.0);
        z.set(step, 2, if squared { best.sqrt() } else { best });
        let (ni, nj) = (size[bi], size[bj]);
        for k in 0..m {
            if !active[k] || k == bi || k == bj {
                continue;
            }
            let (dik, djk, dij) = (d.at(bi, k), d.at(bj, k), best);
            let nk = size[k];
            let nd = match method.as_str() {
                "single" => dik.min(djk),
                "complete" => dik.max(djk),
                "average" => (ni * dik + nj * djk) / (ni + nj),
                "weighted" => (dik + djk) / 2.0,
                "centroid" => {
                    (ni * dik + nj * djk) / (ni + nj) - ni * nj * dij / ((ni + nj) * (ni + nj))
                }
                "median" => (dik + djk) / 2.0 - dij / 4.0,
                _ => ((ni + nk) * dik + (nj + nk) * djk - nk * dij) / (ni + nj + nk),
            };
            d.set(bi, k, nd);
            d.set(k, bi, nd);
        }
        active[bj] = false;
        size[bi] = ni + nj;
        label[bi] = m + step;
    }
    one(z.value())
}

/// Parse a linkage matrix into (merges (a, b, h), m).
type Merge = (usize, usize, f64);

fn parse_z(z: &Value, fname: &str) -> R<(Vec<Merge>, usize)> {
    let zn = real_arg(z, fname)?;
    let zm = M::from_num(&zn, fname)?;
    if zm.c != 3 || zm.r < 1 {
        return Err(err(
            "stats:cluster:BadLinkage",
            format!("{fname}: Z must be an (m-1)-by-3 linkage matrix."),
        ));
    }
    let m = zm.r + 1;
    let mut merges = Vec::with_capacity(zm.r);
    for i in 0..zm.r {
        let (a, b, h) = (zm.at(i, 0), zm.at(i, 1), zm.at(i, 2));
        // children must be existing clusters: observations 1..m or earlier merges
        let limit = (m + i) as f64;
        let valid = |v: f64| v.fract() == 0.0 && v >= 1.0 && v <= limit;
        if !(valid(a) && valid(b) && a != b && h.is_finite()) {
            return Err(err(
                "stats:cluster:BadLinkage",
                format!(
                    "{fname}: Z is not a valid linkage matrix (row {} is inconsistent).",
                    i + 1
                ),
            ));
        }
        merges.push((a as usize - 1, b as usize - 1, h));
    }
    Ok((merges, m))
}

/// Observation lists after performing the first `k` merges (rest of
/// the tree is cut). Cluster ids follow `Z` numbering.
fn cut_after(merges: &[(usize, usize, f64)], m: usize, k: usize) -> Vec<usize> {
    let mut parent: Vec<usize> = (0..m + merges.len()).collect();
    for (s, &(a, b, _)) in merges.iter().take(k).enumerate() {
        parent[a] = m + s;
        parent[b] = m + s;
    }
    let root = |mut i: usize| {
        while parent[i] != i {
            i = parent[i];
        }
        i
    };
    // number clusters by first appearance of their smallest observation
    let mut ids = std::collections::HashMap::new();
    let mut t = vec![0; m];
    for (i, ti) in t.iter_mut().enumerate() {
        let r = root(i);
        let next = ids.len() + 1;
        *ti = *ids.entry(r).or_insert(next);
    }
    t
}

fn inconsistent_matrix(merges: &[(usize, usize, f64)], m: usize, depth: usize) -> M {
    let mut out = M::zeros(merges.len(), 4);
    for (i, &(_, _, h)) in merges.iter().enumerate() {
        // collect link heights within `depth` levels below link i
        let mut hs = vec![h];
        let mut frontier = vec![i + m];
        for _ in 1..depth {
            let mut next = Vec::new();
            for &node in &frontier {
                if node >= m {
                    let (a, b, _) = merges[node - m];
                    for c in [a, b] {
                        if c >= m {
                            hs.push(merges[c - m].2);
                            next.push(c);
                        }
                    }
                }
            }
            frontier = next;
        }
        let n = hs.len() as f64;
        let mu = mean(&hs);
        let sd = if hs.len() > 1 { var(&hs).sqrt() } else { 0.0 };
        out.set(i, 0, mu);
        out.set(i, 1, sd);
        out.set(i, 2, n);
        out.set(i, 3, if sd > 0.0 { (h - mu) / sd } else { 0.0 });
    }
    out
}

fn inconsistent(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "inconsistent")?;
    let (merges, m) = parse_z(&a[0], "inconsistent")?;
    let depth = match a.get(1) {
        Some(v) => {
            let d = arg_f64(v, "inconsistent")?;
            if d.is_nan() || d < 1.0 || d.fract() != 0.0 {
                return Err(err(
                    "stats:inconsistent:BadDepth",
                    "inconsistent: D must be a positive integer.",
                ));
            }
            d as usize
        }
        None => 2,
    };
    one(inconsistent_matrix(&merges, m, depth).value())
}

fn cluster(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "cluster")?;
    let (merges, m) = parse_z(&a[0], "cluster")?;
    let o = name_values(&a, 1, "cluster")?;
    let depth = match opt(&o, "depth") {
        Some(v) => {
            let d = arg_f64(v, "cluster")?;
            if d.is_nan() || d < 1.0 || d.fract() != 0.0 {
                return Err(err(
                    "stats:cluster:BadDepth",
                    "cluster: 'Depth' must be a positive integer.",
                ));
            }
            d as usize
        }
        None => 2,
    };
    let t = if let Some(v) = opt(&o, "maxclust") {
        let k = arg_f64(v, "cluster")?;
        if k < 1.0 || k.fract() != 0.0 {
            return Err(err(
                "stats:cluster:BadMaxclust",
                "cluster: 'MaxClust' must be a positive integer.",
            ));
        }
        let k = (k as usize).min(m);
        cut_after(&merges, m, m - k)
    } else if let Some(v) = opt(&o, "cutoff") {
        let c = arg_f64(v, "cluster")?;
        let crit = opt_str(&o, "criterion", "cluster")?.unwrap_or_else(|| "inconsistent".into());
        let incons = inconsistent_matrix(&merges, m, depth.max(1));
        // keep merges (in order) while the criterion is below the cutoff
        let mut keep = 0;
        for (i, &(_, _, h)) in merges.iter().enumerate() {
            let val = match crit.as_str() {
                "distance" => h,
                "inconsistent" => incons.at(i, 3),
                _ => {
                    return Err(err(
                        "stats:cluster:BadCriterion",
                        "cluster: 'Criterion' must be 'inconsistent' or 'distance'.",
                    ));
                }
            };
            if val <= c {
                keep = i + 1;
            } else {
                break;
            }
        }
        cut_after(&merges, m, keep)
    } else {
        return Err(err(
            "stats:cluster:MissingCut",
            "cluster: specify 'MaxClust' or 'Cutoff'.",
        ));
    };
    one(Value::col(t.into_iter().map(|v| v as f64).collect()))
}

// ---------------------------------------------------------------------------
// k-means
// ---------------------------------------------------------------------------

fn sqd(u: &[f64], v: &[f64]) -> f64 {
    u.iter().zip(v).map(|(a, b)| (a - b).powi(2)).sum()
}

fn kmeans(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "kmeans")?;
    let x = M::from_num(&real_arg(&a[0], "kmeans")?, "kmeans")?;
    let kf = arg_f64(&a[1], "kmeans")?;
    if x.c == 0 || kf.is_nan() || kf.fract() != 0.0 {
        return Err(err(
            "stats:kmeans:BadK",
            "kmeans: X must have columns and K must be a positive integer.",
        ));
    }
    let k = kf.max(0.0) as usize;
    if k == 0 || k > x.r {
        return Err(err(
            "stats:kmeans:TooManyClusters",
            "kmeans: K must be between 1 and the number of observations.",
        ));
    }
    let o = name_values(&a, 2, "kmeans")?;
    for (name, _) in &o {
        if !["replicates", "maxiter", "start", "distance", "emptyaction"].contains(&name.as_str()) {
            return Err(err(
                "stats:kmeans:InvalidOption",
                format!("kmeans: option '{name}' is not supported."),
            ));
        }
    }
    let positive_int = |name: &str, dflt: usize| -> R<usize> {
        match opt(&o, name) {
            Some(v) => {
                let t = arg_f64(v, "kmeans")?;
                if t.is_nan() || t < 1.0 || t.fract() != 0.0 {
                    return Err(err(
                        "stats:kmeans:BadOption",
                        format!("kmeans: '{name}' must be a positive integer."),
                    ));
                }
                Ok(t as usize)
            }
            None => Ok(dflt),
        }
    };
    let reps = positive_int("replicates", 1)?;
    let maxiter = positive_int("maxiter", 100)?;
    if let Some(v) = opt(&o, "emptyaction") {
        let s = arg_str(v, "kmeans")?.to_lowercase();
        if s != "singleton" {
            return Err(err(
                "stats:kmeans:UnsupportedEmptyAction",
                "kmeans: only 'EmptyAction','singleton' is supported.",
            ));
        }
    }
    let start_v = opt(&o, "start");
    let start = match start_v {
        Some(v) if v.is_text() => arg_str(v, "kmeans")?.to_lowercase(),
        Some(_) => "matrix".into(),
        None => "plus".into(),
    };
    let distance = opt_str(&o, "distance", "kmeans")?.unwrap_or_else(|| "sqeuclidean".into());
    if distance != "sqeuclidean" {
        return Err(err(
            "stats:kmeans:UnsupportedDistance",
            "kmeans: only 'sqeuclidean' distance is supported in the Rust engine.",
        ));
    }
    if x.d.iter().any(|t| t.is_nan()) {
        return Err(err(
            "stats:kmeans:NaNData",
            "kmeans: X must not contain NaN; remove incomplete observations first.",
        ));
    }
    let rows: Vec<Vec<f64>> = (0..x.r).map(|i| x.row(i)).collect();
    let mut best: Option<(f64, Vec<usize>, Vec<Vec<f64>>)> = None;
    for rep in 0..reps.max(1) {
        let mut cent: Vec<Vec<f64>> = match start.as_str() {
            "sample" => {
                let mut idx: Vec<usize> = (0..x.r).collect();
                for i in 0..k {
                    let j = i + (it.rng.uniform() * (x.r - i) as f64) as usize;
                    idx.swap(i, j.min(x.r - 1));
                }
                idx[..k].iter().map(|&i| rows[i].clone()).collect()
            }
            "uniform" => {
                let (mut lo, mut hi) = (vec![f64::INFINITY; x.c], vec![f64::NEG_INFINITY; x.c]);
                for r in &rows {
                    for j in 0..x.c {
                        lo[j] = lo[j].min(r[j]);
                        hi[j] = hi[j].max(r[j]);
                    }
                }
                (0..k)
                    .map(|_| {
                        (0..x.c)
                            .map(|j| lo[j] + it.rng.uniform() * (hi[j] - lo[j]))
                            .collect()
                    })
                    .collect()
            }
            "matrix" => {
                let c = M::from_num(&real_arg(start_v.unwrap(), "kmeans")?, "kmeans")?;
                if c.d.iter().any(|t| t.is_nan()) {
                    return Err(err(
                        "stats:kmeans:BadStart",
                        "kmeans: the Start matrix must not contain NaN.",
                    ));
                }
                if c.r != k || c.c != x.c {
                    return Err(err(
                        "stats:kmeans:BadStart",
                        "kmeans: the Start matrix must be K-by-P.",
                    ));
                }
                (0..k).map(|i| c.row(i)).collect()
            }
            "plus" => {
                let mut c = vec![rows[(it.rng.uniform() * x.r as f64) as usize % x.r].clone()];
                while c.len() < k {
                    let dmin: Vec<f64> = rows
                        .iter()
                        .map(|r| c.iter().map(|q| sqd(r, q)).fold(f64::INFINITY, f64::min))
                        .collect();
                    let tot: f64 = dmin.iter().sum();
                    let mut u = it.rng.uniform() * tot;
                    let mut pick = x.r - 1;
                    for (i, d) in dmin.iter().enumerate() {
                        if u < *d {
                            pick = i;
                            break;
                        }
                        u -= d;
                    }
                    c.push(rows[pick].clone());
                }
                c
            }
            "cluster" => {
                // pilot run on a 10% subsample is replaced by a plain sample start
                let mut idx: Vec<usize> = (0..x.r).collect();
                for i in 0..k {
                    let j = i + (it.rng.uniform() * (x.r - i) as f64) as usize;
                    idx.swap(i, j.min(x.r - 1));
                }
                idx[..k].iter().map(|&i| rows[i].clone()).collect()
            }
            other => {
                return Err(err(
                    "stats:kmeans:BadStart",
                    format!("kmeans: unrecognized Start '{other}'."),
                ));
            }
        };
        let _ = rep;
        let mut idx = vec![usize::MAX; x.r];
        for _ in 0..maxiter {
            let mut changed = false;
            for (i, r) in rows.iter().enumerate() {
                let mut bk = 0;
                let mut bd = f64::INFINITY;
                for (c, q) in cent.iter().enumerate() {
                    let dd = sqd(r, q);
                    if dd < bd {
                        bd = dd;
                        bk = c;
                    }
                }
                if idx[i] != bk {
                    idx[i] = bk;
                    changed = true;
                }
            }
            for c in 0..k {
                let members: Vec<&Vec<f64>> = (0..x.r)
                    .filter(|&i| idx[i] == c)
                    .map(|i| &rows[i])
                    .collect();
                if members.is_empty() {
                    // empty cluster: restart it at the point farthest from its centroid
                    let (mut fi, mut fd) = (0, -1.0);
                    for (i, r) in rows.iter().enumerate() {
                        let dd = sqd(r, &cent[idx[i]]);
                        if dd > fd {
                            fd = dd;
                            fi = i;
                        }
                    }
                    cent[c] = rows[fi].clone();
                    idx[fi] = c;
                    changed = true;
                } else {
                    for j in 0..x.c {
                        cent[c][j] =
                            members.iter().map(|r| r[j]).sum::<f64>() / members.len() as f64;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let sse: f64 = rows
            .iter()
            .enumerate()
            .map(|(i, r)| sqd(r, &cent[idx[i]]))
            .sum();
        if best.as_ref().is_none_or(|b| sse < b.0) {
            best = Some((sse, idx, cent));
        }
    }
    let (_, idx, cent) = best.unwrap();
    let sumd: Vec<f64> = (0..k)
        .map(|c| {
            (0..x.r)
                .filter(|&i| idx[i] == c)
                .map(|i| sqd(&rows[i], &cent[c]))
                .sum()
        })
        .collect();
    let mut cm = M::zeros(k, x.c);
    for c in 0..k {
        for j in 0..x.c {
            cm.set(c, j, cent[c][j]);
        }
    }
    let mut dm = M::zeros(x.r, k);
    for i in 0..x.r {
        for c in 0..k {
            dm.set(i, c, sqd(&rows[i], &cent[c]));
        }
    }
    Ok(vec![
        Value::col(idx.iter().map(|&v| v as f64 + 1.0).collect()),
        cm.value(),
        Value::col(sumd),
        dm.value(),
    ])
}

// ---------------------------------------------------------------------------
// pca, ecdf
// ---------------------------------------------------------------------------

fn pca(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "pca")?;
    let xn = real_arg(&a[0], "pca")?;
    let x = M::from_num(&xn, "pca")?;
    let o = name_values(&a, 1, "pca")?;
    for (name, _) in &o {
        if !["centered", "rows", "numcomponents"].contains(&name.as_str()) {
            return Err(err(
                "stats:pca:InvalidOption",
                format!("pca: option '{name}' is not supported."),
            ));
        }
    }
    let centered = match opt(&o, "centered") {
        Some(v) => v.is_true()?,
        None => true,
    };
    let rows_opt = opt_str(&o, "rows", "pca")?.unwrap_or_else(|| "complete".into());
    let ncomp = match opt(&o, "numcomponents") {
        Some(v) => {
            let q = arg_f64(v, "pca")?;
            if !(q >= 1.0 && q.fract() == 0.0 && q.is_finite()) {
                return Err(err(
                    "stats:pca:BadNumComponents",
                    "pca: 'NumComponents' must be a positive integer.",
                ));
            }
            Some(q as usize)
        }
        None => None,
    };
    let keep: Vec<usize> = (0..x.r)
        .filter(|&i| x.row(i).iter().all(|t| !t.is_nan()))
        .collect();
    if rows_opt == "all" && keep.len() != x.r {
        return Err(err(
            "stats:pca:NaNData",
            "pca: X contains NaN; use 'Rows','complete' or 'pairwise'.",
        ));
    }
    if rows_opt == "pairwise" {
        return Err(err(
            "stats:pca:Unsupported",
            "pca: Rows='pairwise' is not supported.",
        ));
    }
    let n = keep.len();
    let p = x.c;
    if n < 2 || p < 1 {
        return Err(err(
            "stats:pca:NotEnoughData",
            "pca: X needs at least two complete observations and one variable.",
        ));
    }
    let mu: Vec<f64> = (0..p)
        .map(|j| {
            if centered {
                mean(&keep.iter().map(|&i| x.at(i, j)).collect::<Vec<_>>())
            } else {
                0.0
            }
        })
        .collect();
    let mut xc = M::zeros(n, p);
    for (r, &i) in keep.iter().enumerate() {
        for j in 0..p {
            xc.set(r, j, x.at(i, j) - mu[j]);
        }
    }
    let (_u, s, v) = jacobi_svd(n, p, &xc.d);
    let dof = if centered { n.saturating_sub(1) } else { n };
    let k = s.len().min(dof).min(p);
    let mut coeff = M::zeros(p, k);
    for c in 0..k {
        // sign convention: largest-magnitude element of each column positive
        let col: Vec<f64> = (0..p).map(|r| v[r + c * p]).collect();
        let big = col
            .iter()
            .copied()
            .fold(0.0f64, |m, t| if t.abs() > m.abs() { t } else { m });
        let sg = if big < 0.0 { -1.0 } else { 1.0 };
        for r in 0..p {
            coeff.set(r, c, sg * col[r]);
        }
    }
    let latent: Vec<f64> = s
        .iter()
        .take(k)
        .map(|sv| sv * sv / (dof.max(1) as f64))
        .collect();
    let total: f64 = latent.iter().sum();
    if ncomp.is_some_and(|q| q > k) {
        return Err(err(
            "stats:pca:BadNumComponents",
            "pca: 'NumComponents' exceeds the number of available components.",
        ));
    }
    let kk = ncomp.unwrap_or(k);
    let mut score = M::zeros(x.r, kk);
    for i in 0..x.r {
        if x.row(i).iter().any(|t| t.is_nan()) {
            for c in 0..kk {
                score.set(i, c, f64::NAN);
            }
            continue;
        }
        for c in 0..kk {
            let sc: f64 = (0..p).map(|j| (x.at(i, j) - mu[j]) * coeff.at(j, c)).sum();
            score.set(i, c, sc);
        }
    }
    let tsq: Vec<f64> = (0..x.r)
        .map(|i| {
            (0..k)
                .filter(|&c| latent[c] > latent[0] * f64::EPSILON * (n.max(p) as f64))
                .map(|c| {
                    let sc: f64 = (0..p).map(|j| (x.at(i, j) - mu[j]) * coeff.at(j, c)).sum();
                    sc * sc / latent[c]
                })
                .sum()
        })
        .collect();
    let mut coeff_out = coeff;
    if kk < k {
        let mut c2 = M::zeros(p, kk);
        for c in 0..kk {
            for r in 0..p {
                c2.set(r, c, coeff_out.at(r, c));
            }
        }
        coeff_out = c2;
    }
    Ok(vec![
        coeff_out.value(),
        score.value(),
        Value::col(latent[..kk].to_vec()),
        Value::col(tsq),
        Value::col(latent.iter().map(|l| 100.0 * l / total).take(kk).collect()),
        Value::row(mu),
    ])
}

fn ecdf(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "ecdf")?;
    let y = real_arg(&a[0], "ecdf")?;
    if y.dims.len() != 2 || (y.dims[0] != 1 && y.dims[1] != 1) {
        return Err(err("stats:ecdf:BadData", "ecdf: Y must be a vector."));
    }
    if y.re.iter().all(|t| t.is_nan()) {
        return Err(err(
            "stats:ecdf:NoData",
            "ecdf: Y must contain at least one non-NaN value.",
        ));
    }
    let o = name_values(&a, 1, "ecdf")?;
    for (k, _) in &o {
        if k != "function" {
            return Err(err(
                "stats:ecdf:Unsupported",
                format!("ecdf: option '{k}' (censoring, frequency, bounds) is not supported."),
            ));
        }
    }
    if nargout > 2 {
        return Err(err(
            "stats:ecdf:Unsupported",
            "ecdf: confidence bounds are not supported.",
        ));
    }
    let func = opt_str(&o, "function", "ecdf")?.unwrap_or_else(|| "cdf".into());
    let mut v: Vec<f64> = y.re.iter().copied().filter(|t| !t.is_nan()).collect();
    v.sort_by(f64::total_cmp);
    let n = v.len() as f64;
    let mut xs = vec![];
    let mut fs = vec![];
    let mut i = 0;
    while i < v.len() {
        let mut j = i;
        while j + 1 < v.len() && v[j + 1] == v[i] {
            j += 1;
        }
        xs.push(v[i]);
        fs.push((j + 1) as f64 / n);
        i = j + 1;
    }
    let (f, x) = match func.as_str() {
        "cdf" => {
            let mut f = vec![0.0];
            f.extend(fs);
            let mut x = vec![xs[0]];
            x.extend(xs);
            (f, x)
        }
        "survivor" => {
            let mut f = vec![1.0];
            f.extend(fs.iter().map(|t| 1.0 - t));
            let mut x = vec![xs[0]];
            x.extend(xs);
            (f, x)
        }
        "cumhazard" => {
            // -log(survivor)
            let mut f = vec![0.0];
            f.extend(fs.iter().map(|t| -(1.0 - t).ln()));
            let mut x = vec![xs[0]];
            x.extend(xs);
            (f, x)
        }
        _ => {
            return Err(err(
                "stats:ecdf:BadFunction",
                "ecdf: 'Function' must be 'cdf', 'survivor' or 'cumulative hazard'.",
            ));
        }
    };
    Ok(vec![Value::col(f), Value::col(x)])
}

// ---------------------------------------------------------------------------
// k-nearest-neighbour classification
// ---------------------------------------------------------------------------

/// Class labels as indices into `classes`; supports numeric and cellstr.
fn label_levels(v: &Value, fname: &str) -> R<(Vec<usize>, Vec<Value>)> {
    match v {
        Value::Cell(c) => {
            let strs: Vec<String> = c.data.iter().map(|e| arg_str(e, fname)).collect::<R<_>>()?;
            let mut u = strs.clone();
            u.sort();
            u.dedup();
            let idx = strs.iter().map(|s| u.binary_search(s).unwrap()).collect();
            Ok((idx, u.iter().map(|s| strv(s)).collect()))
        }
        _ => {
            let n = arg_num(v, fname)?;
            if n.re.iter().any(|t| t.is_nan()) {
                return Err(err(
                    "stats:labels:NaN",
                    format!("{fname}: class/group labels must not be NaN."),
                ));
            }
            let mut u = n.re.clone();
            u.sort_by(f64::total_cmp);
            u.dedup();
            let idx = n.re.iter().map(|t| u.partition_point(|q| q < t)).collect();
            Ok((idx, u.iter().map(|t| Value::num(*t)).collect()))
        }
    }
}

/// Group labels (numeric or cellstr) as 0-based level indices.
pub fn group_ids(v: &Value, fname: &str) -> R<Vec<usize>> {
    Ok(label_levels(v, fname)?.0)
}

fn fitcknn(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "fitcknn")?;
    let x = M::from_num(&real_arg(&a[0], "fitcknn")?, "fitcknn")?;
    if x.r == 0 || x.c == 0 {
        return Err(err("stats:fitcknn:NoData", "fitcknn: X must not be empty."));
    }
    let (idx, classes) = label_levels(&a[1], "fitcknn")?;
    if idx.len() != x.r {
        return Err(err(
            "stats:fitcknn:InputSizeMismatch",
            "fitcknn: Y must have one label per row of X.",
        ));
    }
    let o = name_values(&a, 2, "fitcknn")?;
    for (name, _) in &o {
        if !["numneighbors", "distance"].contains(&name.as_str()) {
            return Err(err(
                "stats:fitcknn:InvalidOption",
                format!("fitcknn: option '{name}' is not supported."),
            ));
        }
    }
    let k = match opt(&o, "numneighbors") {
        Some(v) => arg_f64(v, "fitcknn")?,
        None => 1.0,
    };
    if k.is_nan() || k < 1.0 || k.fract() != 0.0 {
        return Err(err(
            "stats:fitcknn:BadNumNeighbors",
            "fitcknn: 'NumNeighbors' must be a positive integer.",
        ));
    }
    let dname = opt_str(&o, "distance", "fitcknn")?.unwrap_or_else(|| "euclidean".into());
    metric(&dname, None, &x, "fitcknn")?; // validate
    let mut ob = Object::new("ClassificationKNN");
    ob.set("X", x.value());
    ob.set("Y", a[1].clone());
    ob.set("NumNeighbors", Value::num(k));
    ob.set("Distance", strv(&dname));
    ob.set(
        "ClassNames",
        match &classes[0] {
            Value::Num(_) => Value::col(classes.iter().filter_map(|c| c.scalar_f64()).collect()),
            _ => cell(vec![classes.len(), 1], classes.clone()),
        },
    );
    ob.set("NumObservations", Value::num(idx.len() as f64));
    one(Value::Object(Box::new(ob)))
}

fn predict(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "predict")?;
    let Value::Object(o) = &a[0] else {
        return Err(err(
            "stats:predict:BadModel",
            "predict: the first argument must be a model object.",
        ));
    };
    match o.class.as_str() {
        "ClassificationKNN" => predict_knn(o, &a[1], nargout),
        "LinearModel" => super::regr::predict_linear(it, o, &a[1..], nargout),
        other => Err(err(
            "stats:predict:UnsupportedModel",
            format!("predict: models of class '{other}' are not supported."),
        )),
    }
}

fn predict_knn(o: &Object, xnew: &Value, nargout: usize) -> R<Vec<Value>> {
    let x = M::from_num(
        &real_arg(o.get("X").unwrap_or(&Value::empty()), "predict")?,
        "predict",
    )?;
    let (idx, classes) = label_levels(o.get("Y").unwrap_or(&Value::empty()), "predict")?;
    let xq = M::from_num(&real_arg(xnew, "predict")?, "predict")?;
    if xq.c != x.c {
        return Err(err(
            "stats:predict:BadPredictor",
            "predict: Xnew must have the same number of columns as the training data.",
        ));
    }
    let k = o
        .get("NumNeighbors")
        .and_then(|v| v.scalar_f64())
        .unwrap_or(1.0) as usize;
    let dname = o
        .get("Distance")
        .and_then(|v| v.to_str())
        .unwrap_or_else(|| "euclidean".into());
    let mt = metric(&dname, None, &x, "predict")?;
    let rows: Vec<Vec<f64>> = (0..x.r).map(|i| x.row(i)).collect();
    let nc = classes.len();
    let mut labels = Vec::new();
    let mut score = M::zeros(xq.r, nc);
    for q in 0..xq.r {
        let qr = xq.row(q);
        let mut ord: Vec<(f64, usize)> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| (dist_between(&mt, r, &qr), i))
            .collect();
        ord.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let mut votes = vec![0.0; nc];
        for &(_, i) in ord.iter().take(k.min(rows.len())) {
            votes[idx[i]] += 1.0;
        }
        let kk = k.min(rows.len()) as f64;
        let mut best = 0;
        for c in 1..nc {
            if votes[c] > votes[best] {
                best = c;
            }
        }
        for c in 0..nc {
            score.set(q, c, votes[c] / kk);
        }
        labels.push(best);
    }
    let lab = match &classes[0] {
        Value::Num(_) => Value::col(
            labels
                .iter()
                .map(|&c| classes[c].scalar_f64().unwrap())
                .collect(),
        ),
        _ => cell(
            vec![labels.len(), 1],
            labels.iter().map(|&c| classes[c].clone()).collect(),
        ),
    };
    let mut res = vec![lab];
    if nargout > 1 {
        res.push(score.value());
    }
    Ok(res)
}

// ---------------------------------------------------------------------------
// cross-validation partitions
// ---------------------------------------------------------------------------

fn shuffled(it: &mut Interp, n: usize) -> Vec<usize> {
    let mut p: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        let j = (it.rng.uniform() * (i + 1) as f64) as usize;
        p.swap(i, j.min(i));
    }
    p
}

fn cvpartition(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "cvpartition")?;
    let (n, groups) = if a[0].numel() == 1 && a[0].is_numeric() {
        let nf = arg_f64(&a[0], "cvpartition")?;
        if !(nf.is_finite() && nf >= 1.0 && nf.fract() == 0.0 && nf <= 1e8) {
            return Err(err(
                "stats:cvpartition:BadN",
                "cvpartition: N must be a positive integer (at most 1e8).",
            ));
        }
        (nf as usize, None)
    } else {
        let (idx, _) = label_levels(&a[0], "cvpartition")?;
        (idx.len(), Some(idx))
    };
    let kind = arg_str(&a[1], "cvpartition")?.to_lowercase();
    let mut ob = Object::new("cvpartition");
    ob.set("NumObservations", Value::num(n as f64));
    // fold id per observation (1-based); holdout: 1 = test
    let (ids, nsets, ty) = match kind.as_str() {
        "kfold" => {
            let kf = match a.get(2) {
                Some(v) => arg_f64(v, "cvpartition")?,
                None => 10.0,
            };
            let k = if kf.is_finite() && kf.fract() == 0.0 && kf >= 0.0 {
                kf as usize
            } else {
                0
            };
            if k < 2 || k > n {
                return Err(err(
                    "stats:cvpartition:BadK",
                    "cvpartition: K must be an integer between 2 and the number of observations.",
                ));
            }
            let mut ids = vec![0usize; n];
            match &groups {
                None => {
                    let perm = shuffled(it, n);
                    for (r, &i) in perm.iter().enumerate() {
                        ids[i] = r % k + 1;
                    }
                }
                Some(g) => {
                    let ng = g.iter().max().map_or(0, |m| m + 1);
                    let mut counter = 0;
                    for c in 0..ng {
                        let members: Vec<usize> = (0..n).filter(|&i| g[i] == c).collect();
                        let perm = shuffled(it, members.len());
                        for &pi in &perm {
                            ids[members[pi]] = counter % k + 1;
                            counter += 1;
                        }
                    }
                }
            }
            (ids, k, "kfold")
        }
        "holdout" => {
            let frac = match a.get(2) {
                Some(v) => arg_f64(v, "cvpartition")?,
                None => 0.1,
            };
            let nt = if frac >= 1.0 && frac.fract() == 0.0 {
                frac as usize
            } else if frac > 0.0 && frac < 1.0 {
                (frac * n as f64).round() as usize
            } else {
                0
            };
            if nt < 1 || nt >= n {
                return Err(err(
                    "stats:cvpartition:BadHoldout",
                    "cvpartition: 'HoldOut' must be a fraction in (0, 1) or an integer in [1, N-1] leaving both sets non-empty.",
                ));
            }
            let perm = shuffled(it, n);
            let mut ids = vec![0usize; n];
            for &i in perm.iter().take(nt) {
                ids[i] = 1;
            }
            (ids, 1, "holdout")
        }
        "leaveout" => ((1..=n).collect(), n, "leaveout"),
        "resubstitution" => (vec![1; n], 1, "resubstitution"),
        _ => {
            return Err(err(
                "stats:cvpartition:BadType",
                "cvpartition: type must be 'KFold', 'HoldOut', 'LeaveOut' or 'Resubstitution'.",
            ));
        }
    };
    let (train, testn): (Vec<f64>, Vec<f64>) = if ty == "resubstitution" {
        (vec![n as f64], vec![n as f64])
    } else {
        (
            (1..=nsets)
                .map(|s| {
                    ids.iter()
                        .filter(|&&v| if ty == "holdout" { v != 1 } else { v != s })
                        .count() as f64
                })
                .collect(),
            (1..=nsets)
                .map(|s| {
                    ids.iter()
                        .filter(|&&v| if ty == "holdout" { v == 1 } else { v == s })
                        .count() as f64
                })
                .collect(),
        )
    };
    ob.set("NumTestSets", Value::num(nsets as f64));
    ob.set("TrainSize", Value::col(train));
    ob.set("TestSize", Value::col(testn));
    ob.set("Type", strv(ty));
    ob.set(
        "Fold__",
        Value::col(ids.iter().map(|&v| v as f64).collect()),
    );
    one(Value::Object(Box::new(ob)))
}

fn cv_sets(a: &[Value], training: bool) -> R<Vec<Value>> {
    let fname = if training { "training" } else { "test" };
    need(a, 1, fname)?;
    let Value::Object(o) = &a[0] else {
        return Err(err(
            "stats:cvpartition:BadObject",
            format!("{fname}: the first argument must be a cvpartition."),
        ));
    };
    if o.class != "cvpartition" {
        return Err(err(
            "stats:cvpartition:BadObject",
            format!("{fname}: the first argument must be a cvpartition."),
        ));
    }
    let ids = o
        .get("Fold__")
        .and_then(|v| v.to_f64_vec())
        .unwrap_or_default();
    let ty = o.get("Type").and_then(|v| v.to_str()).unwrap_or_default();
    let which = match a.get(1) {
        Some(v) => arg_f64(v, fname)? as usize,
        None => 1,
    };
    let nsets = o
        .get("NumTestSets")
        .and_then(|v| v.scalar_f64())
        .unwrap_or(1.0) as usize;
    if which < 1 || which > nsets {
        return Err(err(
            "stats:cvpartition:BadIndex",
            format!("{fname}: the partition index must be between 1 and NumTestSets."),
        ));
    }
    let w = which as f64;
    let flags: Vec<bool> = ids
        .iter()
        .map(|&v| {
            let in_test = match ty.as_str() {
                "holdout" => v == 1.0,
                "resubstitution" => true,
                _ => v == w,
            };
            if ty == "resubstitution" {
                true
            } else if training {
                !in_test
            } else {
                in_test
            }
        })
        .collect();
    one(indy_value::bool_array(vec![flags.len(), 1], flags))
}

// ---------------------------------------------------------------------------
// sampling, multivariate normal
// ---------------------------------------------------------------------------

fn randsample(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "randsample")?;
    let pop = real_arg(&a[0], "randsample")?;
    let (n, values): (usize, Option<Vec<f64>>) = if pop.re.len() == 1 {
        let v = pop.re[0];
        if !(v.is_finite() && v >= 1.0 && v.fract() == 0.0 && v < 9.0e15) {
            return Err(err(
                "stats:randsample:BadPopulation",
                "randsample: a scalar population must be a positive integer.",
            ));
        }
        (v as usize, None)
    } else if pop.re.is_empty() {
        return Err(err(
            "stats:randsample:BadPopulation",
            "randsample: the population must not be empty.",
        ));
    } else {
        (pop.re.len(), Some(pop.re.clone()))
    };
    let kf = arg_f64(&a[1], "randsample")?;
    if !(kf.is_finite() && kf >= 0.0 && kf.fract() == 0.0) || kf > super::dist::MAX_DRAWS as f64 {
        return Err(err(
            "stats:randsample:BadK",
            "randsample: K must be a nonnegative integer within the supported size.",
        ));
    }
    let k = kf as usize;
    let mut replace = false;
    let mut weights: Option<Vec<f64>> = None;
    for (i, v) in a.iter().enumerate().skip(2) {
        if i == 2 && v.numel() == 1 && !v.is_text() {
            replace = v.is_true()?;
        } else if i == 3 && !v.is_text() {
            weights = Some(real_arg(v, "randsample")?.re);
        } else {
            return Err(err(
                "stats:randsample:BadArgument",
                "randsample: arguments after K must be REPLACE and W.",
            ));
        }
    }
    if !replace && k > n {
        return Err(err(
            "stats:randsample:SampleTooBig",
            "randsample: K must be less than or equal to the population size when sampling without replacement.",
        ));
    }
    let mut picks: Vec<usize> = Vec::with_capacity(k);
    match &weights {
        None if replace => {
            for _ in 0..k {
                picks.push(((it.rng.uniform() * n as f64) as usize).min(n - 1));
            }
        }
        None => {
            // Fisher-Yates on a virtual identity array: O(k) memory for any n
            let mut moved: std::collections::HashMap<usize, usize> =
                std::collections::HashMap::new();
            for i in 0..k {
                let j = i + ((it.rng.uniform() * (n - i) as f64) as usize).min(n - i - 1);
                let vi = *moved.get(&i).unwrap_or(&i);
                let vj = *moved.get(&j).unwrap_or(&j);
                moved.insert(j, vi);
                moved.insert(i, vj);
                picks.push(vj);
            }
        }
        Some(w) => {
            if w.len() != n {
                return Err(err(
                    "stats:randsample:BadWeights",
                    "randsample: W must have one weight per population member.",
                ));
            }
            if w.iter().any(|t| !t.is_finite() || *t < 0.0) || w.iter().sum::<f64>() <= 0.0 {
                return Err(err(
                    "stats:randsample:BadWeights",
                    "randsample: W must be finite, nonnegative and not all zero.",
                ));
            }
            if !replace && w.iter().filter(|t| **t > 0.0).count() < k {
                return Err(err(
                    "stats:randsample:BadWeights",
                    "randsample: W has fewer positive weights than K.",
                ));
            }
            let mut w = w.clone();
            for _ in 0..k {
                let tot: f64 = w.iter().sum();
                let mut u = it.rng.uniform() * tot;
                let mut pick = w.iter().rposition(|t| *t > 0.0).unwrap_or(n - 1);
                for (i, wi) in w.iter().enumerate() {
                    if u < *wi {
                        pick = i;
                        break;
                    }
                    u -= wi;
                }
                picks.push(pick);
                if !replace {
                    w[pick] = 0.0;
                }
            }
        }
    }
    let out: Vec<f64> = picks
        .iter()
        .map(|&i| values.as_ref().map_or(i as f64 + 1.0, |v| v[i]))
        .collect();
    // column for a scalar population size, orientation of the population otherwise
    let row = pop.re.len() > 1 && pop.dims[0] == 1;
    one(if row {
        Value::row(out)
    } else {
        Value::col(out)
    })
}

fn cholesky(s: &M) -> Option<M> {
    let n = s.r;
    let mut l = M::zeros(n, n);
    for j in 0..n {
        for i in j..n {
            let mut v = s.at(i, j);
            for k in 0..j {
                v -= l.at(i, k) * l.at(j, k);
            }
            if i == j {
                if v <= 0.0 {
                    return None;
                }
                l.set(i, j, v.sqrt());
            } else {
                l.set(i, j, v / l.at(j, j));
            }
        }
    }
    Some(l)
}

fn mvnpdf(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "mvnpdf")?;
    let x = M::from_num(&real_arg(&a[0], "mvnpdf")?, "mvnpdf")?;
    let d = x.c;
    let mu: Vec<f64> = match a.get(1) {
        Some(v) if !v.is_empty() => {
            let m = real_arg(v, "mvnpdf")?.re;
            if m.len() == d {
                m
            } else if m.len() == 1 {
                vec![m[0]; d]
            } else {
                return Err(err(
                    "stats:mvnpdf:InputSizeMismatch",
                    "mvnpdf: MU must have one entry per column of X.",
                ));
            }
        }
        _ => vec![0.0; d],
    };
    let sigma = match a.get(2) {
        Some(v) if !v.is_empty() => M::from_num(&real_arg(v, "mvnpdf")?, "mvnpdf")?,
        _ => {
            let mut s = M::zeros(d, d);
            for i in 0..d {
                s.set(i, i, 1.0);
            }
            s
        }
    };
    if sigma.r != d || sigma.c != d {
        return Err(err(
            "stats:mvnpdf:BadCovariance",
            "mvnpdf: SIGMA must be a D-by-D symmetric positive definite matrix.",
        ));
    }
    let l = cholesky(&sigma).ok_or_else(|| {
        err(
            "stats:mvnpdf:BadMatrixSigma",
            "mvnpdf: SIGMA must be symmetric and positive definite.",
        )
    })?;
    let logdet: f64 = (0..d).map(|i| l.at(i, i).ln()).sum::<f64>() * 2.0;
    let out: Vec<f64> = (0..x.r)
        .map(|r| {
            // solve L z = (x - mu)
            let mut z = vec![0.0; d];
            for i in 0..d {
                let mut s = x.at(r, i) - mu[i];
                for k in 0..i {
                    s -= l.at(i, k) * z[k];
                }
                z[i] = s / l.at(i, i);
            }
            let q: f64 = z.iter().map(|t| t * t).sum();
            (-0.5 * (q + logdet + d as f64 * (2.0 * std::f64::consts::PI).ln())).exp()
        })
        .collect();
    one(Value::col(out))
}

fn mvnrnd(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "mvnrnd")?;
    let mu = M::from_num(&real_arg(&a[0], "mvnrnd")?, "mvnrnd")?;
    let sigma = M::from_num(&real_arg(&a[1], "mvnrnd")?, "mvnrnd")?;
    let d = sigma.r;
    if sigma.c != d {
        return Err(err(
            "stats:mvnrnd:BadCovariance",
            "mvnrnd: SIGMA must be a square matrix.",
        ));
    }
    let n = match a.get(2) {
        Some(v) => count_arg(v, "mvnrnd", 0.0, 1e8)?,
        None => mu.r.max(1),
    };
    let l = cholesky(&sigma).ok_or_else(|| {
        err(
            "stats:mvnrnd:BadCovariance",
            "mvnrnd: SIGMA must be symmetric and positive definite.",
        )
    })?;
    let mus: Vec<f64> = if mu.d.len() == d {
        mu.d.clone()
    } else if mu.d.len() == 1 {
        vec![mu.d[0]; d]
    } else {
        return Err(err(
            "stats:mvnrnd:InputSizeMismatch",
            "mvnrnd: MU must have D elements.",
        ));
    };
    if n.saturating_mul(d) > super::dist::MAX_DRAWS {
        return Err(err(
            "MATLAB:pmaxsize",
            "Out of memory. The requested array exceeds the maximum size supported by the Rust engine.",
        ));
    }
    let mut out = M::zeros(n, d);
    for r in 0..n {
        let z: Vec<f64> = (0..d).map(|_| it.rng.normal()).collect();
        for i in 0..d {
            let s: f64 = (0..=i).map(|k| l.at(i, k) * z[k]).sum();
            out.set(r, i, mus[i] + s);
        }
    }
    one(out.value())
}
