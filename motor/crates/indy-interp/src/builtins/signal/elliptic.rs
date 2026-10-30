//! Jacobi elliptic functions and the elliptic analog lowpass prototype.
//! Real-argument functions use the arithmetic–geometric mean descent;
//! complex arguments go through the addition theorem with Jacobi's
//! imaginary transformation.

use std::f64::consts::PI;

use super::{C, CZ, cadd, cdiv, cmul, cscale};

fn agm_seq(k: f64, kp: f64) -> (Vec<f64>, Vec<f64>) {
    // a_n, c_n with a_0 = 1, b_0 = k', c_0 = k
    let mut a = vec![1.0];
    let mut c = vec![k];
    let (mut an, mut bn) = (1.0_f64, kp);
    for _ in 0..40 {
        let cn = (an - bn) / 2.0;
        let a1 = (an + bn) / 2.0;
        bn = (an * bn).sqrt();
        an = a1;
        a.push(an);
        c.push(cn);
        if cn.abs() < 1e-17 * an {
            break;
        }
    }
    (a, c)
}

/// Complete elliptic integral K from modulus `k` and complement `kp`.
pub fn ellipk(kp: f64) -> f64 {
    let (mut a, mut b) = (1.0_f64, kp);
    for _ in 0..60 {
        let a1 = (a + b) / 2.0;
        b = (a * b).sqrt();
        a = a1;
        if (a - b).abs() < 1e-16 * a {
            break;
        }
    }
    PI / (2.0 * a)
}

/// `(sn, cn, dn)` of real `u` with modulus `k` (complement `kp`).
pub fn sncndn(u: f64, k: f64, kp: f64) -> (f64, f64, f64) {
    if k == 0.0 {
        return (u.sin(), u.cos(), 1.0);
    }
    let (a, c) = agm_seq(k, kp);
    let n = a.len() - 1;
    let mut phi = (2.0_f64).powi(n as i32) * a[n] * u;
    for i in (1..=n).rev() {
        phi = (phi + (c[i] * phi.sin() / a[i]).asin()) / 2.0;
    }
    let sn = phi.sin();
    let cn = phi.cos();
    let dn = (1.0 - k * k * sn * sn).max(0.0).sqrt();
    (sn, cn, dn)
}

/// Incomplete elliptic integral of the first kind F(phi | k).
pub fn ellipf(phi: f64, k: f64, kp: f64) -> f64 {
    let (mut a, mut b) = (1.0_f64, kp);
    let mut p = phi;
    let mut pow = 1.0;
    for _ in 0..60 {
        let t = (b / a * p.tan()).atan();
        let adj = PI * ((p + PI / 2.0) / PI).floor();
        p = p + t + adj;
        let a1 = (a + b) / 2.0;
        b = (a * b).sqrt();
        a = a1;
        pow *= 2.0;
        if (a - b).abs() < 1e-16 * a {
            break;
        }
    }
    let _ = k;
    p / (pow * a)
}

/// cd(a + i b | k) for real `a`, `b`.
pub fn cd_complex(a: f64, b: f64, k: f64, kp: f64) -> C {
    let (s, c, d) = sncndn(a, k, kp);
    let (s1, c1, d1) = sncndn(b, kp, k);
    let num = (c * c1, -s * d * s1 * d1);
    let den = (d * d1 * c1, -k * k * s * c * s1);
    cdiv(num, den)
}

fn nome_k(q: f64) -> f64 {
    // k = (theta2/theta3)^2
    let mut t2 = 0.0;
    let mut t3 = 1.0;
    for n in 0..30 {
        let nf = n as f64;
        t2 += q.powf(nf * (nf + 1.0));
        if n >= 1 {
            t3 += 2.0 * q.powf(nf * nf);
        }
    }
    t2 *= 2.0 * q.powf(0.25);
    (t2 / t3).powi(2)
}

/// Elliptic analog lowpass prototype `(z, p, k)`, passband edge 1 rad/s.
pub fn ellipap(n: usize, rp: f64, rs: f64) -> (Vec<C>, Vec<C>, f64) {
    if n == 1 {
        let e = (10f64.powf(rp / 10.0) - 1.0).sqrt();
        return (vec![], vec![(-1.0 / e, 0.0)], 1.0 / e);
    }
    let eps = (10f64.powf(rp / 10.0) - 1.0).sqrt();
    let k1 = eps / (10f64.powf(rs / 10.0) - 1.0).sqrt();
    let k1p = (1.0 - k1 * k1).sqrt();
    let kk1 = ellipk(k1p);
    let kk1p = ellipk(k1);
    let q = (-PI * kk1p / (n as f64 * kk1)).exp();
    let k = nome_k(q);
    let kp = (1.0 - k * k).sqrt();
    let kk = ellipk(kp);
    let v0 = ellipf((1.0 / eps).atan(), k1p, k1) / (n as f64 * kk1);
    let l = n / 2;
    let odd = !n.is_multiple_of(2);
    let mut zeros = Vec::new();
    let mut poles = Vec::new();
    for i in 1..=l {
        let u = (2.0 * i as f64 - 1.0) / n as f64;
        let (s, c, d) = sncndn(u * kk, k, kp);
        let _ = (s, d);
        let cdv = c / d;
        let zi = 1.0 / (k * cdv);
        zeros.push((0.0, zi));
        zeros.push((0.0, -zi));
        let mut p = cd_complex(u * kk, -v0 * kk, k, kp);
        p = (-p.1, p.0); // multiply by j
        if p.0 > 0.0 {
            p = (-p.0, p.1);
        }
        poles.push(p);
        poles.push((p.0, -p.1));
    }
    if odd {
        let p0 = cd_complex(kk, -v0 * kk, k, kp);
        let r = -(p0.1.abs());
        poles.push((r, 0.0));
    }
    let mut num = (1.0, 0.0);
    for p in &poles {
        num = cmul(num, cscale(*p, -1.0));
    }
    let mut den = (1.0, 0.0);
    for z in &zeros {
        den = cmul(den, cscale(*z, -1.0));
    }
    let mut gain = cdiv(num, den).0;
    if !odd {
        gain /= (1.0 + eps * eps).sqrt();
    }
    let _ = (CZ, cadd);
    (zeros, poles, gain)
}
