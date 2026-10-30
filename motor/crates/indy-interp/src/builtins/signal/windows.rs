//! Private window helpers for the signal functions (the public window
//! builtins belong to the numerics lane, not to this module).

use std::f64::consts::PI;

/// Modified Bessel function of the first kind, order 0 (power series).
pub fn bessel_i0(x: f64) -> f64 {
    let q = x * x / 4.0;
    let mut term = 1.0;
    let mut sum = 1.0;
    let mut k = 1.0;
    while term > sum * 1e-17 && k < 500.0 {
        term *= q / (k * k);
        sum += term;
        k += 1.0;
    }
    sum
}

pub fn hamming(n: usize) -> Vec<f64> {
    cosine_window(n, 0.54, 0.46)
}

fn cosine_window(n: usize, a: f64, b: f64) -> Vec<f64> {
    match n {
        0 => vec![],
        1 => vec![1.0],
        _ => (0..n)
            .map(|i| a - b * (2.0 * PI * i as f64 / (n - 1) as f64).cos())
            .collect(),
    }
}

pub fn kaiser(n: usize, beta: f64) -> Vec<f64> {
    match n {
        0 => vec![],
        1 => vec![1.0],
        _ => {
            let d = bessel_i0(beta);
            let m = (n - 1) as f64;
            (0..n)
                .map(|i| {
                    let r = 2.0 * i as f64 / m - 1.0;
                    bessel_i0(beta * (1.0 - r * r).max(0.0).sqrt()) / d
                })
                .collect()
        }
    }
}

pub fn sinc(x: f64) -> f64 {
    if x == 0.0 {
        1.0
    } else {
        (PI * x).sin() / (PI * x)
    }
}
