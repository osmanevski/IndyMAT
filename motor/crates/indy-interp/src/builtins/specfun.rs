//! Special functions shared by the statistics modules: log-gamma,
//! regularised incomplete gamma/beta (both tails), erf inverses and a
//! generic monotone CDF inverter. Series and continued fractions are
//! textbook (Stirling, power series, modified Lentz); no external code.

use std::f64::consts::PI;

use indy_value::{Num, NumClass, Value};

use super::{arg_num, arg_str, err, need, one, reg};
use crate::{Interp, R};

const LN_SQRT_2PI: f64 = 0.918_938_533_204_672_8;

/// ln|Γ(x)| (Stirling series for x ≥ 8, upward recurrence below,
/// reflection for x < 0.5).
pub fn lgamma(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x.is_infinite() {
        return f64::INFINITY;
    }
    if x <= 0.0 && x.fract() == 0.0 {
        return f64::INFINITY;
    }
    if x < 0.5 {
        return (PI / (PI * x).sin().abs()).ln() - lgamma(1.0 - x);
    }
    if x == 0.5 {
        return 0.572_364_942_924_700_1; // ln √π
    }
    if x == 1.0 || x == 2.0 {
        return 0.0;
    }
    // Upward recurrence to z ≥ 8 (compensated sum of logs), then Stirling.
    let (mut shift, mut comp) = (0.0f64, 0.0f64);
    let mut z = x;
    while z < 8.0 {
        let y = z.ln() - comp;
        let t = shift + y;
        comp = (t - shift) - y;
        shift = t;
        z += 1.0;
    }
    const C: [f64; 10] = [
        1.0 / 12.0,
        -1.0 / 360.0,
        1.0 / 1260.0,
        -1.0 / 1680.0,
        1.0 / 1188.0,
        -691.0 / 360_360.0,
        1.0 / 156.0,
        -3617.0 / 122_400.0,
        43_867.0 / 244_188.0,
        -174_611.0 / 125_400.0,
    ];
    let z2 = 1.0 / (z * z);
    let mut series = 0.0;
    for c in C.iter().rev() {
        series = series * z2 + c;
    }
    series /= z;
    (z - 0.5) * z.ln() - z + LN_SQRT_2PI + series - shift
}

/// Stirling remainder: ln Γ(n+1) − (n+½) ln n + n − ln √(2π).
pub fn stirlerr(n: f64) -> f64 {
    if n <= 15.0 {
        return lgamma(n + 1.0) - (n + 0.5) * n.ln() + n - LN_SQRT_2PI;
    }
    let n2 = n * n;
    (1.0 / 12.0
        - (1.0 / 360.0 - (1.0 / 1260.0 - (1.0 / 1680.0 - (1.0 / 1188.0) / n2) / n2) / n2) / n2)
        / n
}

/// Deviance term x ln(x/np) + np − x without cancellation.
pub fn bd0(x: f64, np: f64) -> f64 {
    if x == 0.0 {
        return np;
    }
    if (x - np).abs() < 0.1 * (x + np) {
        let v = (x - np) / (x + np);
        let mut s = (x - np) * v;
        let mut ej = 2.0 * x * v;
        let v2 = v * v;
        for j in 1..1000 {
            ej *= v2;
            let s1 = s + ej / (2 * j + 1) as f64;
            if s1 == s {
                return s1;
            }
            s = s1;
        }
    }
    let r = x / np;
    // ratios that over/underflow (denormal arguments) go through the logs
    let lr = if r.is_finite() && r > 0.0 {
        r.ln()
    } else {
        x.ln() - np.ln()
    };
    x * lr + np - x
}

/// λ^k e^-λ / Γ(k+1), accurate for large arguments (saddlepoint form).
pub fn dpois_raw(k: f64, lambda: f64) -> f64 {
    if lambda == 0.0 {
        return (k == 0.0) as u8 as f64;
    }
    if k == 0.0 {
        return (-lambda).exp();
    }
    (-stirlerr(k) - bd0(k, lambda)).exp() / (2.0 * PI * k).sqrt()
}

/// C(n, k) p^k q^(n-k) for real 0 ≤ k ≤ n, q = 1 − p.
pub fn dbinom_raw(k: f64, n: f64, p: f64, q: f64) -> f64 {
    if p == 0.0 {
        return (k == 0.0) as u8 as f64;
    }
    if q == 0.0 {
        return (k == n) as u8 as f64;
    }
    if k == 0.0 {
        return if p < 0.1 {
            (-bd0(n, n * q) - n * p).exp()
        } else {
            (n * q.ln()).exp()
        };
    }
    if k == n {
        return if q < 0.1 {
            (-bd0(n, n * p) - n * q).exp()
        } else {
            (n * p.ln()).exp()
        };
    }
    let lc = stirlerr(n) - stirlerr(k) - stirlerr(n - k) - bd0(k, n * p) - bd0(n - k, n * q);
    lc.exp() * (n / (2.0 * PI * k * (n - k))).sqrt()
}

/// x^a e^-x / Γ(a).
fn gamma_lead(x: f64, a: f64) -> f64 {
    a * dpois_raw(a, x)
}

/// x^a (1-x)^b / B(a, b) for 0 < x < 1.
fn beta_lead(x: f64, a: f64, b: f64) -> f64 {
    a * b / (a + b) * dbinom_raw(a, a + b, x, 1.0 - x)
}

/// ln B(a, b).
pub fn lbeta(a: f64, b: f64) -> f64 {
    lgamma(a) + lgamma(b) - lgamma(a + b)
}

/// ln of the binomial coefficient C(n, k) for real arguments.
pub fn lchoose(n: f64, k: f64) -> f64 {
    lgamma(n + 1.0) - lgamma(k + 1.0) - lgamma(n - k + 1.0)
}

/// Regularised incomplete gamma: (P(a, x), Q(a, x)).
pub fn gamma_pq(x: f64, a: f64) -> (f64, f64) {
    if x.is_nan() || a.is_nan() || a < 0.0 || x < 0.0 {
        return (f64::NAN, f64::NAN);
    }
    if a == 0.0 {
        return (1.0, 0.0);
    }
    if x == 0.0 {
        return (0.0, 1.0);
    }
    if x.is_infinite() {
        return (1.0, 0.0);
    }
    let lead = gamma_lead(x, a);
    if a.is_infinite() {
        return (0.0, 1.0);
    }
    if x < a + 1.0 {
        let mut ap = a;
        let mut term = 1.0 / a;
        let mut sum = term;
        let mut converged = false;
        for _ in 0..3_000_000 {
            ap += 1.0;
            term *= x / ap;
            sum += term;
            if term.abs() < sum.abs() * 1e-17 {
                converged = true;
                break;
            }
        }
        if !converged {
            return (f64::NAN, f64::NAN);
        }
        let p = (sum * lead).clamp(0.0, 1.0);
        (p, 1.0 - p)
    } else {
        let q = (lead * gamma_cf(x, a)).clamp(0.0, 1.0);
        (1.0 - q, q)
    }
}

/// Continued fraction (modified Lentz) for Γ(a, x)/(x^a e^-x) factor of Q.
fn gamma_cf(x: f64, a: f64) -> f64 {
    let tiny = 1e-300;
    let mut b = x + 1.0 - a;
    let mut c = 1.0 / tiny;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..3_000_000 {
        let an = -(i as f64) * (i as f64 - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < tiny {
            d = tiny;
        }
        c = b + an / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 1e-16 {
            return h;
        }
    }
    f64::NAN
}

fn betacf(x: f64, a: f64, b: f64) -> f64 {
    let tiny = 1e-300;
    let (qab, qap, qam) = (a + b, a + 1.0, a - 1.0);
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < tiny {
        d = tiny;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..3_000_000 {
        let m = m as f64;
        let m2 = 2.0 * m;
        let aa = m * (b - m) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < tiny {
            d = tiny;
        }
        c = 1.0 + aa / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        h *= d * c;
        let aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < tiny {
            d = tiny;
        }
        c = 1.0 + aa / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 1e-16 {
            return h;
        }
    }
    f64::NAN
}

/// Regularised incomplete beta: (I_x(a, b), 1 − I_x(a, b)), both tails
/// computed on their own side of the symmetry point.
pub fn beta_pq(x: f64, a: f64, b: f64) -> (f64, f64) {
    if x.is_nan() || a.is_nan() || b.is_nan() || a < 0.0 || b < 0.0 {
        return (f64::NAN, f64::NAN);
    }
    if x <= 0.0 {
        return (0.0, 1.0);
    }
    if x >= 1.0 {
        return (1.0, 0.0);
    }
    if a == 0.0 && b == 0.0 {
        return (0.5, 0.5);
    }
    if a == 0.0 {
        return (1.0, 0.0);
    }
    if b == 0.0 {
        return (0.0, 1.0);
    }
    let lead = beta_lead(x, a, b);
    if x < (a + 1.0) / (a + b + 2.0) {
        let p = (lead * betacf(x, a, b) / a).clamp(0.0, 1.0);
        (p, 1.0 - p)
    } else {
        let q = (lead * betacf(1.0 - x, b, a) / b).clamp(0.0, 1.0);
        (1.0 - q, q)
    }
}

/// erfc with full relative accuracy in the tails.
pub fn erfc(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x < 0.0 {
        return 2.0 - erfc(-x);
    }
    if x.is_infinite() {
        return 0.0;
    }
    // erfc(x) = Q(1/2, x^2); the fraction avoids 1 - erf cancellation for x ≥ 0.7
    let x2 = x * x;
    if (0.5..1e6).contains(&x2) {
        (-x2).exp() / PI.sqrt() * x * gamma_cf(x2, 0.5)
    } else {
        gamma_pq(x2, 0.5).1
    }
}

pub fn erf(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x < 0.0 {
        return -erf(-x);
    }
    if x.is_infinite() {
        return 1.0;
    }
    gamma_pq(x * x, 0.5).0
}

/// Scaled complementary error function exp(x²)·erfc(x).
pub fn erfcx(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x < 25.0 {
        let e = erfc(x);
        if e > 1e-290 {
            return (x * x).exp() * e;
        }
    }
    // asymptotic continued fraction: erfcx(x) = 1/√π / (x + ½/(x + 1/(x + …)))
    let mut f = 0.0;
    for k in (1..80).rev() {
        f = k as f64 / 2.0 / (x + f);
    }
    1.0 / (PI.sqrt() * (x + f))
}

/// Inverse of erfc on (0, 2): Halley refinement of a rational start.
pub fn erfcinv(y: f64) -> f64 {
    if y.is_nan() || !(0.0..=2.0).contains(&y) {
        return f64::NAN;
    }
    if y == 0.0 {
        return f64::INFINITY;
    }
    if y == 2.0 {
        return f64::NEG_INFINITY;
    }
    if y == 1.0 {
        return 0.0;
    }
    if y > 1.0 {
        return -erfcinv(2.0 - y);
    }
    // y in (0, 1): x > 0. Winitzki start, then Halley steps.
    let c = 0.147;
    let ln1 = (y * (2.0 - y)).ln();
    let p = 2.0 / (PI * c) + ln1 / 2.0;
    let mut x = ((p * p - ln1 / c).sqrt() - p).sqrt();
    for _ in 0..60 {
        let f = erfc(x) - y;
        let d = -2.0 / PI.sqrt() * (-x * x).exp();
        if d == 0.0 {
            break;
        }
        let dx = f / d;
        let nx = x - dx / (1.0 + x * dx);
        let nx = if nx.is_finite() && nx > 0.0 {
            nx
        } else {
            x - dx
        };
        if (nx - x).abs() <= 1e-16 * nx.abs() {
            x = nx;
            break;
        }
        x = nx;
    }
    x
}

pub fn erfinv(y: f64) -> f64 {
    if y.is_nan() || !(-1.0..=1.0).contains(&y) {
        return f64::NAN;
    }
    if y.abs() < 0.5 {
        // Newton on erf for small |y| keeps absolute accuracy near zero.
        let a = 0.147;
        let ln1 = (1.0 - y * y).ln();
        let p = 2.0 / (PI * a) + ln1 / 2.0;
        let mut x = ((p * p - ln1 / a).sqrt() - p).sqrt().copysign(y);
        for _ in 0..50 {
            let f = erf(x) - y;
            let d = 2.0 / PI.sqrt() * (-x * x).exp();
            let dx = f / d;
            let nx = x - dx / (1.0 + x * dx);
            if (nx - x).abs() <= 1e-16 * nx.abs().max(1e-300) {
                x = nx;
                break;
            }
            x = nx;
        }
        return x;
    }
    if y > 0.0 {
        erfcinv(1.0 - y)
    } else {
        -erfcinv(1.0 + y)
    }
}

/// Standard normal quantile, accurate in both tails.
pub fn norminv(p: f64) -> f64 {
    if p.is_nan() || !(0.0..=1.0).contains(&p) {
        return f64::NAN;
    }
    -std::f64::consts::SQRT_2 * erfcinv(2.0 * p)
}

/// Invert a non-decreasing CDF on `[lo, hi]` (either may be infinite):
/// bracket, then bisect to full precision. `cdf(x) ≥ p` defines the
/// answer (smallest such x for discrete laws when `discrete`).
pub fn invert(cdf: &dyn Fn(f64) -> f64, p: f64, lo: f64, hi: f64) -> f64 {
    let (mut a, mut b) = (lo, hi);
    if lo == 0.0 && hi > 0.0 {
        // positive support: bracket from above, then shrink geometrically so
        // results far below 1e-120 keep full relative accuracy
        let tiny = f64::from_bits(1); // smallest subnormal
        if cdf(tiny) >= p {
            return 0.0; // the quantile underflows
        }
        let mut top = hi;
        if top.is_infinite() {
            top = 1.0;
            while cdf(top) < p && top < 1e300 {
                top *= 2.0;
            }
        }
        let (mut g0, mut g1) = (tiny, top);
        for _ in 0..200 {
            if g1.ln() - g0.ln() <= 1e-6 {
                break;
            }
            // geometric midpoint (a product of tiny numbers would underflow)
            let m = (0.5 * (g0.ln() + g1.ln())).exp();
            if m <= g0 || m >= g1 {
                break;
            }
            if cdf(m) < p {
                g0 = m;
            } else {
                g1 = m;
            }
        }
        a = g0;
        b = g1;
    }
    if a.is_infinite() {
        a = -1.0;
        let mut step = 1.0;
        while cdf(a) > p && a > -1e300 {
            step *= 2.0;
            a = -step;
        }
    }
    if b.is_infinite() {
        b = if a < 1.0 { 1.0 } else { a + 1.0 };
        let mut step = b.abs().max(1.0);
        while cdf(b) < p && b < 1e300 {
            step *= 2.0;
            b = step;
        }
    }
    for _ in 0..400 {
        let m = 0.5 * (a + b);
        if m == a || m == b {
            break;
        }
        if cdf(m) < p {
            a = m;
        } else {
            b = m;
        }
    }
    0.5 * (a + b)
}

// ---------------------------------------------------------------------------
// builtins
// ---------------------------------------------------------------------------

pub fn register(it: &mut Interp) {
    // math.rs computes erfc as 1 - erf, which loses every tail digit.
    reg(it, "erf", |_, a, _| map1(&a, "erf", erf));
    reg(it, "erfc", |_, a, _| map1(&a, "erfc", erfc));
    reg(it, "erfcx", |_, a, _| map1(&a, "erfcx", erfcx));
    reg(it, "gammaln", |_, a, _| map1(&a, "gammaln", lgamma));
    reg(it, "erfinv", |_, a, _| map1(&a, "erfinv", erfinv));
    reg(it, "erfcinv", |_, a, _| map1(&a, "erfcinv", erfcinv));
    reg(it, "betaln", |_, a, _| map2(&a, "betaln", lbeta));
    reg(it, "gammainc", gammainc);
    reg(it, "betainc", betainc);
}

/// Real floating-point (single/double/logical) argument.
pub fn real(v: &Value, f: &str) -> R<Num> {
    if v.is_text() {
        return Err(err(
            "MATLAB:invalidType",
            format!("{f}: arguments must be numeric, got {}.", v.class_name()),
        ));
    }
    let n = arg_num(v, f)?;
    if n.is_complex() {
        return Err(err(
            "MATLAB:invalidType",
            format!("{f}: arguments must be real."),
        ));
    }
    if n.class.is_int() {
        return Err(err(
            "MATLAB:invalidType",
            format!("{f}: arguments must be single or double."),
        ));
    }
    Ok(n)
}

fn map1(a: &[Value], f: &str, g: fn(f64) -> f64) -> R<Vec<Value>> {
    need(a, 1, f)?;
    let x = real(&a[0], f)?;
    let class = x.class;
    let out = Num::real(x.dims.clone(), x.re.iter().map(|&v| g(v)).collect());
    one(Value::Num(match class {
        NumClass::Single => out.with_class(NumClass::Single),
        _ => out,
    }))
}

/// Broadcast `n` arrays of equal size or scalars into a common shape.
pub fn broadcast(arrs: &[&Num], f: &str) -> R<(Vec<usize>, usize)> {
    let mut dims: Option<&Vec<usize>> = None;
    for a in arrs {
        if a.re.len() != 1 {
            match dims {
                None => dims = Some(&a.dims),
                Some(d) if *d == a.dims => {}
                _ => {
                    return Err(err(
                        "MATLAB:sizeDimensionsMustMatch",
                        format!("{f}: non-scalar arguments must match in size."),
                    ));
                }
            }
        }
    }
    let d = dims.cloned().unwrap_or_else(|| vec![1, 1]);
    let n = d.iter().product();
    Ok((d, n))
}

pub fn at(n: &Num, i: usize) -> f64 {
    if n.re.len() == 1 { n.re[0] } else { n.re[i] }
}

fn map2(a: &[Value], f: &str, g: fn(f64, f64) -> f64) -> R<Vec<Value>> {
    need(a, 2, f)?;
    let x = real(&a[0], f)?;
    let y = real(&a[1], f)?;
    let single = x.class == NumClass::Single || y.class == NumClass::Single;
    let (d, n) = broadcast(&[&x, &y], f)?;
    one(single_if(
        Num::real(d, (0..n).map(|i| g(at(&x, i), at(&y, i))).collect()),
        single,
    ))
}

fn gammainc(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "gammainc")?;
    let x = real(&a[0], "gammainc")?;
    let s = real(&a[1], "gammainc")?;
    let tail = match a.get(2) {
        Some(t) => arg_str(t, "gammainc")?.to_lowercase(),
        None => "lower".into(),
    };
    if !matches!(
        tail.as_str(),
        "lower" | "upper" | "scaledlower" | "scaledupper"
    ) {
        return Err(err(
            "MATLAB:gammainc:InvalidTailArg",
            "gammainc: invalid tail, expected 'lower', 'upper', 'scaledlower' or 'scaledupper'.",
        ));
    }
    if x.re.iter().any(|v| *v < 0.0) || s.re.iter().any(|v| *v < 0.0) {
        return Err(err(
            "MATLAB:gammainc:NegativeInput",
            "gammainc: X and A must be nonnegative.",
        ));
    }
    let single = x.class == NumClass::Single || s.class == NumClass::Single;
    let (d, n) = broadcast(&[&x, &s], "gammainc")?;
    let out = (0..n)
        .map(|i| {
            let (xi, ai) = (at(&x, i), at(&s, i));
            let (p, q) = gamma_pq(xi, ai);
            match tail.as_str() {
                "lower" => p,
                "upper" => q,
                t => {
                    // scaled by Γ(a+1) e^x / x^a
                    let sc = (lgamma(ai + 1.0) + xi - ai * xi.ln()).exp();
                    sc * if t == "scaledlower" { p } else { q }
                }
            }
        })
        .collect();
    one(single_if(Num::real(d, out), single))
}

fn single_if(n: Num, single: bool) -> Value {
    Value::Num(if single {
        n.with_class(NumClass::Single)
    } else {
        n
    })
}

fn betainc(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 3, "betainc")?;
    let x = real(&a[0], "betainc")?;
    let z = real(&a[1], "betainc")?;
    let w = real(&a[2], "betainc")?;
    let upper = match a.get(3) {
        Some(t) => match arg_str(t, "betainc")?.to_lowercase().as_str() {
            "lower" => false,
            "upper" => true,
            _ => {
                return Err(err(
                    "MATLAB:betainc:InvalidTailArg",
                    "betainc: invalid tail, expected 'lower' or 'upper'.",
                ));
            }
        },
        None => false,
    };
    if x.re
        .iter()
        .any(|&v| !(0.0..=1.0).contains(&v) && !v.is_nan())
    {
        return Err(err(
            "MATLAB:betainc:XOutOfRange",
            "betainc: X must be in the interval [0, 1].",
        ));
    }
    let single = [&x, &z, &w].iter().any(|n| n.class == NumClass::Single);
    let (d, n) = broadcast(&[&x, &z, &w], "betainc")?;
    let out = (0..n)
        .map(|i| {
            let (p, q) = beta_pq(at(&x, i), at(&z, i), at(&w, i));
            if upper { q } else { p }
        })
        .collect();
    one(single_if(Num::real(d, out), single))
}
