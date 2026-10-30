//! Probability distributions (Statistics and Machine Learning Toolbox
//! semantics): `<name>pdf/cdf/inv/rnd` for each family, generic
//! `pdf/cdf/icdf/random`, `normfit`-style helpers live in `htest.rs`.
//! Every family is a table row ([`Dist`]); random draws are inverse
//! transforms of the engine's seeded uniform stream (normal: its normal
//! stream).

use std::f64::consts::PI;

use indy_value::{Num, NumClass, Value};

use super::specfun::{
    at, beta_pq, broadcast, dbinom_raw, dpois_raw, erfc, gamma_pq, invert, lbeta, lgamma, norminv,
};
use super::{arg_f64, arg_str, err, need, one, reg};
use crate::{Interp, R};

const SQRT_2: f64 = std::f64::consts::SQRT_2;
/// Largest number of random draws one call may request (2 GiB of doubles).
pub const MAX_DRAWS: usize = 1 << 28;

const SQRT_2PI: f64 = 2.506_628_274_631_000_7;

pub struct Dist {
    pub key: &'static str,
    /// Lower-case names with spaces/dashes removed.
    pub names: &'static [&'static str],
    /// One entry per parameter; `None` = required.
    pub defaults: &'static [Option<f64>],
    pub discrete: bool,
    pub valid: fn(&[f64]) -> bool,
    pub support: fn(&[f64]) -> (f64, f64),
    pub pdf: fn(f64, &[f64]) -> f64,
    /// (lower tail, upper tail)
    pub cdf: fn(f64, &[f64]) -> (f64, f64),
    pub inv: Option<fn(f64, &[f64]) -> f64>,
    /// (mean, variance)
    pub moments: fn(&[f64]) -> (f64, f64),
}

// ---------------------------------------------------------------------------
// families
// ---------------------------------------------------------------------------

fn pos(x: f64) -> bool {
    x > 0.0 && !x.is_nan()
}

fn fpos(x: f64) -> bool {
    x > 0.0 && x.is_finite()
}

fn gam_pdf(x: f64, a: f64, b: f64) -> f64 {
    if x < 0.0 {
        0.0
    } else if x == 0.0 {
        if a < 1.0 {
            f64::INFINITY
        } else if a == 1.0 {
            1.0 / b
        } else {
            0.0
        }
    } else if x.is_infinite() {
        0.0
    } else {
        a * dpois_raw(a, x / b) / x
    }
}

fn gam_cdf(x: f64, a: f64, b: f64) -> (f64, f64) {
    if x <= 0.0 {
        (0.0, 1.0)
    } else {
        gamma_pq(x / b, a)
    }
}

fn ent(x: f64) -> bool {
    x.fract() == 0.0
}

static NORM: Dist = Dist {
    key: "norm",
    names: &["normal", "gaussian", "norm"],
    defaults: &[Some(0.0), Some(1.0)],
    discrete: false,
    valid: |p| p[0].is_finite() && fpos(p[1]),
    support: |_| (f64::NEG_INFINITY, f64::INFINITY),
    pdf: |x, p| {
        let z = (x - p[0]) / p[1];
        (-0.5 * z * z).exp() / (SQRT_2PI * p[1])
    },
    cdf: |x, p| {
        let z = (x - p[0]) / p[1];
        (0.5 * erfc(-z / SQRT_2), 0.5 * erfc(z / SQRT_2))
    },
    inv: Some(|q, p| p[0] + p[1] * norminv(q)),
    moments: |p| (p[0], p[1] * p[1]),
};

fn t_cdf(x: f64, v: f64) -> (f64, f64) {
    if v.is_infinite() {
        return (0.5 * erfc(-x / SQRT_2), 0.5 * erfc(x / SQRT_2));
    }
    if x.is_infinite() {
        return if x > 0.0 { (1.0, 0.0) } else { (0.0, 1.0) };
    }
    let w = v / (v + x * x);
    let (i, c) = beta_pq(w, v / 2.0, 0.5);
    let (tail, rest) = (0.5 * i, 1.0 - 0.5 * i);
    let _ = c;
    if x > 0.0 {
        (rest, tail)
    } else if x < 0.0 {
        (tail, rest)
    } else {
        (0.5, 0.5)
    }
}

fn t_inv(q: f64, p: &[f64]) -> f64 {
    if q == 0.5 {
        return 0.0;
    }
    if q > 0.5 {
        return -t_inv(1.0 - q, p);
    }
    invert(&|x| t_cdf(x, p[0]).0, q, f64::NEG_INFINITY, 0.0)
}

static T: Dist = Dist {
    key: "t",
    names: &["t", "student", "studentt"],
    defaults: &[None],
    discrete: false,
    valid: |p| pos(p[0]),
    support: |_| (f64::NEG_INFINITY, f64::INFINITY),
    pdf: |x, p| {
        let v = p[0];
        if v.is_infinite() {
            return (-0.5 * x * x).exp() / SQRT_2PI;
        }
        (lgamma((v + 1.0) / 2.0)
            - lgamma(v / 2.0)
            - 0.5 * (v * PI).ln()
            - (v + 1.0) / 2.0 * (x * x / v).ln_1p())
        .exp()
    },
    cdf: |x, p| t_cdf(x, p[0]),
    inv: Some(t_inv),
    moments: |p| {
        (
            if p[0] > 1.0 { 0.0 } else { f64::NAN },
            if p[0] > 2.0 {
                p[0] / (p[0] - 2.0)
            } else if p[0] > 1.0 {
                f64::INFINITY
            } else {
                f64::NAN
            },
        )
    },
};

static CHI2: Dist = Dist {
    key: "chi2",
    names: &["chisquare", "chisquared", "chi2"],
    defaults: &[None],
    discrete: false,
    valid: |p| fpos(p[0]),
    support: |_| (0.0, f64::INFINITY),
    pdf: |x, p| gam_pdf(x, p[0] / 2.0, 2.0),
    cdf: |x, p| gam_cdf(x, p[0] / 2.0, 2.0),
    inv: None,
    moments: |p| (p[0], 2.0 * p[0]),
};

fn f_cdf(x: f64, d1: f64, d2: f64) -> (f64, f64) {
    if x <= 0.0 {
        return (0.0, 1.0);
    }
    if x.is_infinite() {
        return (1.0, 0.0);
    }
    let u = d1 * x / (d1 * x + d2);
    if u < 0.5 {
        beta_pq(u, d1 / 2.0, d2 / 2.0)
    } else {
        let (q, p) = beta_pq(d2 / (d2 + d1 * x), d2 / 2.0, d1 / 2.0);
        (p, q)
    }
}

static F: Dist = Dist {
    key: "f",
    names: &["f", "fisher", "fishersf"],
    defaults: &[None, None],
    discrete: false,
    valid: |p| fpos(p[0]) && fpos(p[1]),
    support: |_| (0.0, f64::INFINITY),
    pdf: |x, p| {
        let (d1, d2) = (p[0], p[1]);
        if x < 0.0 {
            0.0
        } else if x == 0.0 {
            if d1 < 2.0 {
                f64::INFINITY
            } else if d1 == 2.0 {
                1.0
            } else {
                0.0
            }
        } else {
            (0.5 * d1 * d1.ln() + 0.5 * d2 * d2.ln() + (0.5 * d1 - 1.0) * x.ln()
                - 0.5 * (d1 + d2) * (d2 + d1 * x).ln()
                - lbeta(d1 / 2.0, d2 / 2.0))
            .exp()
        }
    },
    cdf: |x, p| f_cdf(x, p[0], p[1]),
    inv: None,
    moments: |p| {
        (
            if p[1] > 2.0 {
                p[1] / (p[1] - 2.0)
            } else {
                f64::NAN
            },
            if p[1] > 4.0 {
                2.0 * p[1] * p[1] * (p[0] + p[1] - 2.0)
                    / (p[0] * (p[1] - 2.0).powi(2) * (p[1] - 4.0))
            } else {
                f64::NAN
            },
        )
    },
};

static EXP: Dist = Dist {
    key: "exp",
    names: &["exponential", "exp"],
    defaults: &[Some(1.0)],
    discrete: false,
    valid: |p| fpos(p[0]),
    support: |_| (0.0, f64::INFINITY),
    pdf: |x, p| {
        if x < 0.0 {
            0.0
        } else {
            (-x / p[0]).exp() / p[0]
        }
    },
    cdf: |x, p| {
        if x <= 0.0 {
            (0.0, 1.0)
        } else {
            (-(-x / p[0]).exp_m1(), (-x / p[0]).exp())
        }
    },
    inv: Some(|q, p| -p[0] * (-q).ln_1p()),
    moments: |p| (p[0], p[0] * p[0]),
};

static GAM: Dist = Dist {
    key: "gam",
    names: &["gamma", "gam"],
    defaults: &[None, Some(1.0)],
    discrete: false,
    valid: |p| fpos(p[0]) && fpos(p[1]),
    support: |_| (0.0, f64::INFINITY),
    pdf: |x, p| gam_pdf(x, p[0], p[1]),
    cdf: |x, p| gam_cdf(x, p[0], p[1]),
    inv: None,
    moments: |p| (p[0] * p[1], p[0] * p[1] * p[1]),
};

static BETA: Dist = Dist {
    key: "beta",
    names: &["beta"],
    defaults: &[None, None],
    discrete: false,
    valid: |p| fpos(p[0]) && fpos(p[1]),
    support: |_| (0.0, 1.0),
    pdf: |x, p| {
        let (a, b) = (p[0], p[1]);
        if !(0.0..=1.0).contains(&x) {
            0.0
        } else if x == 0.0 {
            if a < 1.0 {
                f64::INFINITY
            } else if a == 1.0 {
                b
            } else {
                0.0
            }
        } else if x == 1.0 {
            if b < 1.0 {
                f64::INFINITY
            } else if b == 1.0 {
                a
            } else {
                0.0
            }
        } else {
            a * b / (a + b) * dbinom_raw(a, a + b, x, 1.0 - x) / (x * (1.0 - x))
        }
    },
    cdf: |x, p| beta_pq(x, p[0], p[1]),
    inv: None,
    moments: |p| {
        let s = p[0] + p[1];
        (p[0] / s, p[0] * p[1] / (s * s * (s + 1.0)))
    },
};

static BINO: Dist = Dist {
    key: "bino",
    names: &["binomial", "bino"],
    defaults: &[None, None],
    discrete: true,
    valid: |p| p[0] >= 0.0 && ent(p[0]) && (0.0..=1.0).contains(&p[1]),
    support: |p| (0.0, p[0]),
    pdf: |x, p| {
        let (n, pr) = (p[0], p[1]);
        if !ent(x) || x < 0.0 || x > n {
            0.0
        } else if pr == 0.0 {
            (x == 0.0) as u8 as f64
        } else if pr == 1.0 {
            (x == n) as u8 as f64
        } else {
            dbinom_raw(x, n, pr, 1.0 - pr)
        }
    },
    cdf: |x, p| {
        let (n, pr) = (p[0], p[1]);
        let k = x.floor();
        if k < 0.0 {
            (0.0, 1.0)
        } else if k >= n || pr == 0.0 {
            (1.0, 0.0)
        } else if pr == 1.0 {
            (0.0, 1.0)
        } else {
            let (up, lo) = beta_pq(pr, k + 1.0, n - k);
            (lo, up)
        }
    },
    inv: None,
    moments: |p| (p[0] * p[1], p[0] * p[1] * (1.0 - p[1])),
};

static POISS: Dist = Dist {
    key: "poiss",
    names: &["poisson", "poiss"],
    defaults: &[None],
    discrete: true,
    valid: |p| p[0] >= 0.0 && p[0].is_finite(),
    support: |_| (0.0, f64::INFINITY),
    pdf: |x, p| {
        let l = p[0];
        if !ent(x) || x < 0.0 {
            0.0
        } else if l == 0.0 {
            (x == 0.0) as u8 as f64
        } else {
            dpois_raw(x, l)
        }
    },
    cdf: |x, p| {
        let k = x.floor();
        if k < 0.0 {
            (0.0, 1.0)
        } else if p[0] == 0.0 || k.is_infinite() {
            (1.0, 0.0)
        } else {
            let (lo, up) = gamma_pq(p[0], k + 1.0);
            (up, lo)
        }
    },
    inv: None,
    moments: |p| (p[0], p[0]),
};

static UNIF: Dist = Dist {
    key: "unif",
    names: &["uniform", "unif"],
    defaults: &[Some(0.0), Some(1.0)],
    discrete: false,
    valid: |p| p[0].is_finite() && p[1].is_finite() && p[0] < p[1],
    support: |p| (p[0], p[1]),
    pdf: |x, p| {
        if x >= p[0] && x <= p[1] {
            1.0 / (p[1] - p[0])
        } else {
            0.0
        }
    },
    cdf: |x, p| {
        if x <= p[0] {
            (0.0, 1.0)
        } else if x >= p[1] {
            (1.0, 0.0)
        } else {
            let f = (x - p[0]) / (p[1] - p[0]);
            (f, 1.0 - f)
        }
    },
    inv: Some(|q, p| p[0] + q * (p[1] - p[0])),
    moments: |p| ((p[0] + p[1]) / 2.0, (p[1] - p[0]).powi(2) / 12.0),
};

static UNID: Dist = Dist {
    key: "unid",
    names: &["discreteuniform", "unid"],
    defaults: &[None],
    discrete: true,
    valid: |p| p[0] >= 1.0 && ent(p[0]),
    support: |p| (1.0, p[0]),
    pdf: |x, p| {
        if ent(x) && x >= 1.0 && x <= p[0] {
            1.0 / p[0]
        } else {
            0.0
        }
    },
    cdf: |x, p| {
        let k = x.floor();
        if k < 1.0 {
            (0.0, 1.0)
        } else if k >= p[0] {
            (1.0, 0.0)
        } else {
            (k / p[0], 1.0 - k / p[0])
        }
    },
    inv: Some(|q, p| (q * p[0]).ceil()),
    moments: |p| ((p[0] + 1.0) / 2.0, (p[0] * p[0] - 1.0) / 12.0),
};

static GEO: Dist = Dist {
    key: "geo",
    names: &["geometric", "geo"],
    defaults: &[None],
    discrete: true,
    valid: |p| p[0] > 0.0 && p[0] <= 1.0,
    support: |_| (0.0, f64::INFINITY),
    pdf: |x, p| {
        if !ent(x) || x < 0.0 {
            0.0
        } else {
            p[0] * (x * (-p[0]).ln_1p()).exp()
        }
    },
    cdf: |x, p| {
        let k = x.floor();
        if k < 0.0 {
            (0.0, 1.0)
        } else if p[0] == 1.0 || k.is_infinite() {
            (1.0, 0.0)
        } else {
            let l = ((k + 1.0) * (-p[0]).ln_1p()).exp();
            (1.0 - l, l)
        }
    },
    inv: None,
    moments: |p| ((1.0 - p[0]) / p[0], (1.0 - p[0]) / (p[0] * p[0])),
};

static NBIN: Dist = Dist {
    key: "nbin",
    names: &["negativebinomial", "nbin"],
    defaults: &[None, None],
    discrete: true,
    valid: |p| fpos(p[0]) && p[1] > 0.0 && p[1] <= 1.0,
    support: |_| (0.0, f64::INFINITY),
    pdf: |x, p| {
        let (r, pr) = (p[0], p[1]);
        if !ent(x) || x < 0.0 {
            0.0
        } else if pr == 1.0 {
            (x == 0.0) as u8 as f64
        } else {
            r / (r + x) * dbinom_raw(r, r + x, pr, 1.0 - pr)
        }
    },
    cdf: |x, p| {
        let k = x.floor();
        if k < 0.0 {
            (0.0, 1.0)
        } else if p[1] == 1.0 || k.is_infinite() {
            (1.0, 0.0)
        } else {
            beta_pq(p[1], p[0], k + 1.0)
        }
    },
    inv: None,
    moments: |p| {
        (
            p[0] * (1.0 - p[1]) / p[1],
            p[0] * (1.0 - p[1]) / (p[1] * p[1]),
        )
    },
};

static LOGN: Dist = Dist {
    key: "logn",
    names: &["lognormal", "logn"],
    defaults: &[Some(0.0), Some(1.0)],
    discrete: false,
    valid: |p| p[0].is_finite() && fpos(p[1]),
    support: |_| (0.0, f64::INFINITY),
    pdf: |x, p| {
        if x <= 0.0 {
            0.0
        } else {
            let z = (x.ln() - p[0]) / p[1];
            (-0.5 * z * z).exp() / (x * p[1] * SQRT_2PI)
        }
    },
    cdf: |x, p| {
        if x <= 0.0 {
            (0.0, 1.0)
        } else {
            let z = (x.ln() - p[0]) / p[1];
            (0.5 * erfc(-z / SQRT_2), 0.5 * erfc(z / SQRT_2))
        }
    },
    inv: Some(|q, p| (p[0] + p[1] * norminv(q)).exp()),
    moments: |p| {
        let s2 = p[1] * p[1];
        (
            (p[0] + s2 / 2.0).exp(),
            s2.exp_m1() * (2.0 * p[0] + s2).exp(),
        )
    },
};

static WBL: Dist = Dist {
    key: "wbl",
    names: &["weibull", "wbl"],
    defaults: &[Some(1.0), Some(1.0)],
    discrete: false,
    valid: |p| fpos(p[0]) && fpos(p[1]),
    support: |_| (0.0, f64::INFINITY),
    pdf: |x, p| {
        let (a, b) = (p[0], p[1]);
        if x < 0.0 {
            0.0
        } else if x == 0.0 {
            if b < 1.0 {
                f64::INFINITY
            } else if b == 1.0 {
                1.0 / a
            } else {
                0.0
            }
        } else {
            let z = x / a;
            b / a * z.powf(b - 1.0) * (-z.powf(b)).exp()
        }
    },
    cdf: |x, p| {
        if x <= 0.0 {
            (0.0, 1.0)
        } else {
            let e = -(x / p[0]).powf(p[1]);
            (-e.exp_m1(), e.exp())
        }
    },
    inv: Some(|q, p| p[0] * (-(-q).ln_1p()).powf(1.0 / p[1])),
    moments: |p| {
        let g1 = lgamma(1.0 + 1.0 / p[1]).exp();
        let g2 = lgamma(1.0 + 2.0 / p[1]).exp();
        (p[0] * g1, p[0] * p[0] * (g2 - g1 * g1))
    },
};

static EV: Dist = Dist {
    key: "ev",
    names: &["extremevalue", "ev"],
    defaults: &[Some(0.0), Some(1.0)],
    discrete: false,
    valid: |p| p[0].is_finite() && fpos(p[1]),
    support: |_| (f64::NEG_INFINITY, f64::INFINITY),
    pdf: |x, p| {
        let z = (x - p[0]) / p[1];
        (z - z.exp()).exp() / p[1]
    },
    cdf: |x, p| {
        let z = (x - p[0]) / p[1];
        let e = (-z.exp()).exp();
        (-(-z.exp()).exp_m1(), e)
    },
    inv: Some(|q, p| p[0] + p[1] * (-(-q).ln_1p()).ln()),
    moments: |p| {
        (
            p[0] - std::f64::consts::EULER_GAMMA * p[1],
            PI * PI / 6.0 * p[1] * p[1],
        )
    },
};

static RAYL: Dist = Dist {
    key: "rayl",
    names: &["rayleigh", "rayl"],
    defaults: &[Some(1.0)],
    discrete: false,
    valid: |p| fpos(p[0]),
    support: |_| (0.0, f64::INFINITY),
    pdf: |x, p| {
        if x < 0.0 {
            0.0
        } else {
            x / (p[0] * p[0]) * (-x * x / (2.0 * p[0] * p[0])).exp()
        }
    },
    cdf: |x, p| {
        if x <= 0.0 {
            (0.0, 1.0)
        } else {
            let e = -x * x / (2.0 * p[0] * p[0]);
            (-e.exp_m1(), e.exp())
        }
    },
    inv: Some(|q, p| p[0] * (-2.0 * (-q).ln_1p()).sqrt()),
    moments: |p| (p[0] * (PI / 2.0).sqrt(), (4.0 - PI) / 2.0 * p[0] * p[0]),
};

pub static ALL: &[&Dist] = &[
    &NORM, &T, &CHI2, &F, &EXP, &GAM, &BETA, &BINO, &POISS, &UNIF, &UNID, &GEO, &NBIN, &LOGN, &WBL,
    &EV, &RAYL,
];

pub fn find(name: &str) -> Option<&'static Dist> {
    let k: String = name
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_lowercase();
    ALL.iter().find(|d| d.names.contains(&k.as_str())).copied()
}

// ---------------------------------------------------------------------------
// evaluation
// ---------------------------------------------------------------------------

impl Dist {
    pub fn quantile(&self, q: f64, p: &[f64]) -> f64 {
        if q.is_nan() || !(0.0..=1.0).contains(&q) || !(self.valid)(p) {
            return f64::NAN;
        }
        let (lo, hi) = (self.support)(p);
        if q == 0.0 {
            return lo;
        }
        if q == 1.0 {
            return hi;
        }
        if let Some(f) = self.inv {
            return f(q, p);
        }
        let bad = std::cell::Cell::new(false);
        let watch = |v: f64| {
            if v.is_nan() {
                bad.set(true);
            }
            v
        };
        if self.discrete {
            let target = q * (1.0 - 4.0 * f64::EPSILON);
            let ok = |k: f64| watch((self.cdf)(k, p).0) >= target;
            let mut hi_k = if hi.is_finite() { hi } else { 1.0 };
            while !ok(hi_k) && hi_k < 1e300 {
                hi_k *= 2.0;
            }
            if !ok(hi_k) {
                return f64::INFINITY;
            }
            let mut lo_k = lo - 1.0; // not ok
            // beyond 2^53 neighbouring doubles are more than 1 apart: stop when
            // the midpoint no longer separates the bracket
            while hi_k - lo_k > 1.0 {
                let m = ((lo_k + hi_k) / 2.0).floor();
                if m <= lo_k || m >= hi_k {
                    break;
                }
                if ok(m) {
                    hi_k = m;
                } else {
                    lo_k = m;
                }
            }
            return if bad.get() { f64::NAN } else { hi_k };
        }
        let x = if q <= 0.5 {
            invert(&|x| watch((self.cdf)(x, p).0), q, lo, hi)
        } else {
            invert(&|x| -watch((self.cdf)(x, p).1), -(1.0 - q), lo, hi)
        };
        if bad.get() {
            return f64::NAN;
        }
        x.clamp(lo, hi)
    }

    pub fn random(&self, it: &mut Interp, p: &[f64]) -> f64 {
        if !(self.valid)(p) {
            return f64::NAN;
        }
        if self.key == "norm" {
            return p[0] + p[1] * it.rng.normal();
        }
        let u = it.rng.uniform();
        self.quantile(u, p)
    }
}

struct Call {
    dims: Vec<usize>,
    n: usize,
    x: Num,
    params: Vec<Num>,
    single: bool,
}

fn real_num(v: &Value, f: &str) -> R<Num> {
    super::specfun::real(v, f)
}

/// Parse `(x, p1, p2, ...)` with defaults; returns also leftover options.
fn parse(d: &Dist, a: &[Value], fname: &str) -> R<(Call, Vec<Value>)> {
    need(a, 1, fname)?;
    let x = real_num(&a[0], fname)?;
    let np = d.defaults.len();
    let mut params = Vec::new();
    let mut rest = Vec::new();
    let mut k = 1;
    for dflt in d.defaults {
        if k < a.len() && !a[k].is_text() {
            params.push(real_num(&a[k], fname)?);
            k += 1;
        } else if let Some(v) = dflt {
            params.push(Num::scalar(*v));
        } else {
            return Err(err(
                "MATLAB:minrhs",
                format!(
                    "{fname}: not enough input arguments; {np} distribution parameter(s) needed."
                ),
            ));
        }
    }
    rest.extend_from_slice(&a[k.min(a.len())..]);
    let mut all: Vec<&Num> = vec![&x];
    all.extend(params.iter());
    let (dims, n) = broadcast(&all, fname)?;
    let single = all.iter().any(|n| n.class == NumClass::Single);
    Ok((
        Call {
            dims,
            n,
            x,
            params,
            single,
        },
        rest,
    ))
}

fn params_at(c: &Call, i: usize, buf: &mut Vec<f64>) {
    buf.clear();
    buf.extend(c.params.iter().map(|p| at(p, i)));
}

fn result(dims: Vec<usize>, out: Vec<f64>, single: bool) -> Value {
    let n = Num::real(dims, out);
    Value::Num(if single {
        n.with_class(NumClass::Single)
    } else {
        n
    })
}

fn do_pdf(d: &Dist, a: &[Value], fname: &str) -> R<Vec<Value>> {
    let (c, _) = parse(d, a, fname)?;
    let mut buf = Vec::new();
    let out = (0..c.n)
        .map(|i| {
            params_at(&c, i, &mut buf);
            let x = at(&c.x, i);
            if x.is_nan() || !(d.valid)(&buf) {
                f64::NAN
            } else {
                (d.pdf)(x, &buf)
            }
        })
        .collect();
    one(result(c.dims, out, c.single))
}

fn do_cdf(d: &Dist, a: &[Value], fname: &str) -> R<Vec<Value>> {
    let (c, rest) = parse(d, a, fname)?;
    let upper = match rest.first() {
        Some(t) => match arg_str(t, fname)?.to_lowercase().as_str() {
            "upper" => true,
            "lower" => false,
            _ => {
                return Err(err(
                    "stats:cdf:InvalidTail",
                    format!("{fname}: tail must be 'lower' or 'upper'."),
                ));
            }
        },
        None => false,
    };
    let mut buf = Vec::new();
    let out = (0..c.n)
        .map(|i| {
            params_at(&c, i, &mut buf);
            let x = at(&c.x, i);
            if x.is_nan() || !(d.valid)(&buf) {
                f64::NAN
            } else {
                let (lo, up) = (d.cdf)(x, &buf);
                if upper { up } else { lo }
            }
        })
        .collect();
    one(result(c.dims, out, c.single))
}

fn do_inv(d: &Dist, a: &[Value], fname: &str) -> R<Vec<Value>> {
    let (c, _) = parse(d, a, fname)?;
    let mut buf = Vec::new();
    let out = (0..c.n)
        .map(|i| {
            params_at(&c, i, &mut buf);
            d.quantile(at(&c.x, i), &buf)
        })
        .collect();
    one(result(c.dims, out, c.single))
}

/// Size arguments after the parameters: `m, n, ...`, `[m n]`, or none.
pub fn size_args(rest: &[Value], fname: &str) -> R<Option<Vec<usize>>> {
    let rest: Vec<&Value> = rest.iter().filter(|v| !v.is_text()).collect();
    if rest.is_empty() {
        return Ok(None);
    }
    let dim = |x: f64| -> R<usize> {
        if x.is_nan() || x.fract() != 0.0 {
            return Err(err(
                "MATLAB:NonIntegerInput",
                format!("{fname}: size inputs must be integers."),
            ));
        }
        Ok(x.max(0.0) as usize)
    };
    let mut dims = Vec::new();
    if rest.len() == 1 {
        let v = rest[0].to_f64_vec().unwrap_or_default();
        if v.len() == 1 {
            let k = dim(v[0])?;
            dims = vec![k, k];
        } else {
            for x in v {
                dims.push(dim(x)?);
            }
        }
    } else {
        for r in rest {
            dims.push(dim(arg_f64(r, fname)?)?);
        }
    }
    Ok(Some(indy_value::norm_dims(dims)))
}

fn do_rnd(it: &mut Interp, d: &Dist, a: &[Value], fname: &str) -> R<Vec<Value>> {
    let np = d.defaults.len();
    let mut params = Vec::new();
    // random generators take every parameter explicitly
    for k in 0..np {
        if k < a.len() && !a[k].is_text() {
            params.push(real_num(&a[k], fname)?);
        } else {
            return Err(err(
                "MATLAB:minrhs",
                format!(
                    "{fname}: not enough input arguments; {np} distribution parameter(s) needed."
                ),
            ));
        }
    }
    let sizes = size_args(a.get(np.min(a.len())..).unwrap_or(&[]), fname)?;
    let refs: Vec<&Num> = params.iter().collect();
    let (pd, _) = broadcast(&refs, fname)?;
    let dims = match sizes {
        Some(s) => {
            if params.iter().any(|p| p.re.len() != 1) && s != pd {
                return Err(err(
                    "stats:rnd:ArgSizeMismatch",
                    format!("{fname}: size information is inconsistent with the parameters."),
                ));
            }
            s
        }
        None => pd,
    };
    let n = dims
        .iter()
        .try_fold(1usize, |acc, d| acc.checked_mul(*d))
        .filter(|n| *n <= MAX_DRAWS)
        .ok_or_else(|| {
            err(
                "MATLAB:pmaxsize",
                "Out of memory. The requested array exceeds the maximum size supported by the Rust engine.",
            )
        })?;
    let single = params.iter().any(|p| p.class == NumClass::Single);
    let mut buf = Vec::new();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        buf.clear();
        buf.extend(params.iter().map(|p| at(p, i)));
        out.push(d.random(it, &buf));
    }
    one(result(dims, out, single))
}

macro_rules! reg_dist {
    ($it:expr, $k:literal, $d:expr) => {
        reg($it, concat!($k, "pdf"), |_, a, _| {
            do_pdf(&$d, &a, concat!($k, "pdf"))
        });
        reg($it, concat!($k, "inv"), |_, a, _| {
            do_inv(&$d, &a, concat!($k, "inv"))
        });
        reg($it, concat!($k, "rnd"), |it, a, _| {
            do_rnd(it, &$d, &a, concat!($k, "rnd"))
        });
    };
}

pub fn register(it: &mut Interp) {
    reg_dist!(it, "norm", NORM);
    reg_dist!(it, "t", T);
    reg_dist!(it, "chi2", CHI2);
    reg_dist!(it, "f", F);
    reg_dist!(it, "exp", EXP);
    reg_dist!(it, "gam", GAM);
    reg_dist!(it, "beta", BETA);
    reg_dist!(it, "bino", BINO);
    reg_dist!(it, "poiss", POISS);
    reg_dist!(it, "unif", UNIF);
    reg_dist!(it, "unid", UNID);
    reg_dist!(it, "geo", GEO);
    reg_dist!(it, "nbin", NBIN);
    reg_dist!(it, "logn", LOGN);
    reg_dist!(it, "wbl", WBL);
    reg_dist!(it, "ev", EV);
    reg_dist!(it, "rayl", RAYL);
    // cdf entry points (tail option); normcdf also has confidence bounds
    reg(it, "tcdf", |_, a, _| do_cdf(&T, &a, "tcdf"));
    reg(it, "chi2cdf", |_, a, _| do_cdf(&CHI2, &a, "chi2cdf"));
    reg(it, "fcdf", |_, a, _| do_cdf(&F, &a, "fcdf"));
    reg(it, "expcdf", |_, a, _| do_cdf(&EXP, &a, "expcdf"));
    reg(it, "gamcdf", |_, a, _| do_cdf(&GAM, &a, "gamcdf"));
    reg(it, "betacdf", |_, a, _| do_cdf(&BETA, &a, "betacdf"));
    reg(it, "binocdf", |_, a, _| do_cdf(&BINO, &a, "binocdf"));
    reg(it, "poisscdf", |_, a, _| do_cdf(&POISS, &a, "poisscdf"));
    reg(it, "unifcdf", |_, a, _| do_cdf(&UNIF, &a, "unifcdf"));
    reg(it, "unidcdf", |_, a, _| do_cdf(&UNID, &a, "unidcdf"));
    reg(it, "geocdf", |_, a, _| do_cdf(&GEO, &a, "geocdf"));
    reg(it, "nbincdf", |_, a, _| do_cdf(&NBIN, &a, "nbincdf"));
    reg(it, "logncdf", |_, a, _| do_cdf(&LOGN, &a, "logncdf"));
    reg(it, "wblcdf", |_, a, _| do_cdf(&WBL, &a, "wblcdf"));
    reg(it, "evcdf", |_, a, _| do_cdf(&EV, &a, "evcdf"));
    reg(it, "raylcdf", |_, a, _| do_cdf(&RAYL, &a, "raylcdf"));
    reg(it, "normcdf", normcdf);
    // norminv has its own wrapper (confidence bounds): override the macro's
    reg(it, "norminv", norminv_fn);
    reg(it, "pdf", |it, a, _| generic(it, &a, "pdf", do_pdf));
    reg(it, "cdf", |it, a, _| generic(it, &a, "cdf", do_cdf));
    reg(it, "icdf", |it, a, _| generic(it, &a, "icdf", do_inv));
    reg(it, "random", random);
}

fn dist_arg(v: &Value, fname: &str) -> R<&'static Dist> {
    let name = arg_str(v, fname)?;
    find(&name).ok_or_else(|| {
        err(
            "stats:pdf:UnrecognizedName",
            format!("{fname}: unrecognized distribution name '{name}'."),
        )
    })
}

fn generic(
    it: &mut Interp,
    a: &[Value],
    fname: &'static str,
    f: fn(&Dist, &[Value], &str) -> R<Vec<Value>>,
) -> R<Vec<Value>> {
    need(a, 2, fname)?;
    if !a[0].is_text() {
        return super::regr::object_call(it, fname, a).unwrap_or_else(|| {
            Err(err(
                "stats:pdf:BadDistribution",
                format!("{fname}: the first argument must be a distribution name or object."),
            ))
        });
    }
    let d = dist_arg(&a[0], fname)?;
    // generic form requires every parameter (no defaults)
    let np = d.defaults.len();
    let given = a[2..].iter().take_while(|v| !v.is_text()).count();
    if given < np {
        return Err(err(
            "MATLAB:minrhs",
            format!("{fname}: not enough input arguments; {np} distribution parameter(s) needed."),
        ));
    }
    f(d, &a[1..], fname)
}

fn random(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "random")?;
    if !a[0].is_text() {
        return super::regr::object_call(it, "random", &a).unwrap_or_else(|| {
            Err(err(
                "stats:random:BadDistribution",
                "random: the first argument must be a distribution name or object.",
            ))
        });
    }
    let d = dist_arg(&a[0], "random")?;
    do_rnd(it, d, &a[1..], "random")
}

// ---------------------------------------------------------------------------
// normal with confidence bounds (delta method)
// ---------------------------------------------------------------------------

/// Validate a 2x2 covariance of the estimates (mu, sigma) and alpha.
fn cov_alpha(a: &[Value], fname: &str) -> R<([f64; 3], f64)> {
    let cov = match a.get(3) {
        Some(v) if !v.is_text() => {
            let n = real_num(v, fname)?;
            if n.dims != [2, 2] {
                return Err(err(
                    "stats:norminv:BadCovarianceSymPos",
                    format!(
                        "{fname}: PCOV must be a 2-by-2 symmetric positive semi-definite matrix."
                    ),
                ));
            }
            let (v1, c12, c21, v2) = (n.re[0], n.re[2], n.re[1], n.re[3]);
            let scale = v1.abs().max(v2.abs()).max(c12.abs()).max(1e-300);
            let sym = (c12 - c21).abs() <= 1e-10 * scale;
            let psd = v1 >= 0.0 && v2 >= 0.0 && v1 * v2 - c12 * c21 >= -1e-10 * scale * scale;
            if !(sym && psd) || n.re.iter().any(|x| x.is_nan()) {
                return Err(err(
                    "stats:norminv:BadCovarianceSymPos",
                    format!(
                        "{fname}: PCOV must be a 2-by-2 symmetric positive semi-definite matrix."
                    ),
                ));
            }
            [v1, c12, v2]
        }
        _ => {
            return Err(err(
                "stats:norminv:CovarianceRequired",
                format!("{fname}: PCOV is required for confidence bounds."),
            ));
        }
    };
    let alpha = match a.get(4) {
        Some(v) => arg_f64(v, fname)?,
        None => 0.05,
    };
    if !(alpha > 0.0 && alpha < 1.0) {
        return Err(err(
            "stats:norminv:BadAlpha",
            format!("{fname}: ALPHA must be a scalar between 0 and 1 exclusive."),
        ));
    }
    Ok((cov, alpha))
}

fn norminv_fn(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    if nargout <= 1 {
        let head: Vec<Value> = a.iter().take(3).cloned().collect();
        return do_inv(&NORM, &head, "norminv");
    }
    let head: Vec<Value> = a.iter().take(3).cloned().collect();
    let (cov, alpha) = cov_alpha(&a, "norminv")?;
    let (c, _) = parse(&NORM, &head, "norminv")?;
    let zc = norminv(1.0 - alpha / 2.0);
    let (mut x, mut lo, mut hi) = (Vec::new(), Vec::new(), Vec::new());
    let mut buf = Vec::new();
    for i in 0..c.n {
        params_at(&c, i, &mut buf);
        let p = at(&c.x, i);
        let z = if (NORM.valid)(&buf) {
            norminv(p)
        } else {
            f64::NAN
        };
        let xv = if (NORM.valid)(&buf) && (0.0..=1.0).contains(&p) {
            buf[0] + buf[1] * z
        } else {
            f64::NAN
        };
        let var = if z.is_infinite() {
            f64::NAN
        } else {
            cov[0] + z * z * cov[2] + 2.0 * z * cov[1]
        };
        let h = zc * var.max(0.0).sqrt();
        x.push(xv);
        lo.push(xv - h);
        hi.push(xv + h);
    }
    Ok(vec![
        Value::Num(Num::real(c.dims.clone(), x)),
        Value::Num(Num::real(c.dims.clone(), lo)),
        Value::Num(Num::real(c.dims, hi)),
    ])
}

fn normcdf(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    // [p, pLo, pUp] = normcdf(x, mu, sigma, pCov, alpha); tail option last
    let tail = a.last().filter(|v| v.is_text()).cloned();
    let upper = tail
        .as_ref()
        .and_then(|v| v.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("upper"));
    if nargout <= 1 {
        let mut head: Vec<Value> = a.iter().take(3).filter(|v| !v.is_text()).cloned().collect();
        head.extend(tail);
        return do_cdf(&NORM, &head, "normcdf");
    }
    let head: Vec<Value> = a.iter().take(3).cloned().collect();
    let (cov, alpha) = cov_alpha(&a, "normcdf")?;
    let (c, _) = parse(&NORM, &head, "normcdf")?;
    let zc = norminv(1.0 - alpha / 2.0);
    let (mut p, mut lo, mut hi) = (Vec::new(), Vec::new(), Vec::new());
    let mut buf = Vec::new();
    let cdfz = |z: f64| {
        let (l, u) = (0.5 * erfc(-z / SQRT_2), 0.5 * erfc(z / SQRT_2));
        if upper { u } else { l }
    };
    for i in 0..c.n {
        params_at(&c, i, &mut buf);
        let x = at(&c.x, i);
        if !(NORM.valid)(&buf) || x.is_nan() {
            p.push(f64::NAN);
            lo.push(f64::NAN);
            hi.push(f64::NAN);
            continue;
        }
        let z = (x - buf[0]) / buf[1];
        let var = (cov[0] + z * z * cov[2] + 2.0 * z * cov[1]).max(0.0) / (buf[1] * buf[1]);
        let h = zc * var.sqrt();
        p.push(cdfz(z));
        let (a1, a2) = (cdfz(z - h), cdfz(z + h));
        lo.push(a1.min(a2));
        hi.push(a1.max(a2));
    }
    Ok(vec![
        Value::Num(Num::real(c.dims.clone(), p)),
        Value::Num(Num::real(c.dims.clone(), lo)),
        Value::Num(Num::real(c.dims, hi)),
    ])
}
