//! Colours: default colour order, MATLAB colour letters/names and colormaps.
//!
//! Colormap formulas are our own; `tests/colormaps.rs` checks them against
//! values measured from Octave 11.3 (`tests/oracle/colormaps.json` and
//! `tests/oracle/colormaps2.json`).

use crate::turbo::TURBO_256;
use crate::viridis::VIRIDIS_256;

pub type Rgb = [f64; 3];

/// Default axes `ColorOrder` (measured `get(groot,'defaultaxescolororder')`).
pub const COLOR_ORDER: [Rgb; 7] = [
    [0.0, 0.447, 0.741],
    [0.85, 0.325, 0.098],
    [0.929, 0.694, 0.125],
    [0.494, 0.184, 0.556],
    [0.466, 0.674, 0.188],
    [0.301, 0.745, 0.933],
    [0.635, 0.078, 0.184],
];

/// Resolves a MATLAB colour letter or long name (`'r'`, `'red'`).
pub fn named(name: &str) -> Option<Rgb> {
    Some(match name.to_ascii_lowercase().as_str() {
        "r" | "red" => [1.0, 0.0, 0.0],
        "g" | "green" => [0.0, 1.0, 0.0],
        "b" | "blue" => [0.0, 0.0, 1.0],
        "c" | "cyan" => [0.0, 1.0, 1.0],
        "m" | "magenta" => [1.0, 0.0, 1.0],
        "y" | "yellow" => [1.0, 1.0, 0.0],
        "k" | "black" => [0.0, 0.0, 0.0],
        "w" | "white" => [1.0, 1.0, 1.0],
        _ => return None,
    })
}

/// A colormap: rows of RGB in [0,1].
pub type Colormap = Vec<Rgb>;

pub fn default_colormap() -> Colormap {
    VIRIDIS_256.to_vec()
}

/// Names accepted by [`colormap_by_name`].
pub const COLORMAP_NAMES: [&str; 22] = [
    "viridis",
    "jet",
    "gray",
    "hot",
    "cool",
    "copper",
    "bone",
    "spring",
    "summer",
    "autumn",
    "winter",
    "turbo",
    "hsv",
    "pink",
    "lines",
    "colorcube",
    "prism",
    "flag",
    "white",
    "cubehelix",
    "ocean",
    "rainbow",
];

/// Builds a named colormap with `n` rows (`n == 0` gives an empty map).
pub fn colormap_by_name(name: &str, n: usize) -> Option<Colormap> {
    let t = |k: usize| {
        if n <= 1 {
            0.0
        } else {
            k as f64 / (n - 1) as f64
        }
    };
    let ramp = |f: &dyn Fn(f64) -> Rgb| (0..n).map(|k| f(t(k))).collect::<Colormap>();
    Some(match name.to_ascii_lowercase().as_str() {
        "viridis" => viridis(n),
        "jet" => jet(n),
        "hot" => hot(n),
        "bone" => {
            let g = ramp(&|x| [x, x, x]);
            let h = hot(n);
            g.iter()
                .zip(&h)
                .map(|(g, h)| {
                    [
                        (7.0 * g[0] + h[2]) / 8.0,
                        (7.0 * g[1] + h[1]) / 8.0,
                        (7.0 * g[2] + h[0]) / 8.0,
                    ]
                })
                .collect()
        }
        "gray" | "grey" => ramp(&|x| [x, x, x]),
        "cool" => ramp(&|x| [x, 1.0 - x, 1.0]),
        "copper" => ramp(&|x| [(1.25 * x).min(1.0), 0.7812 * x, 0.4975 * x]),
        "spring" => ramp(&|x| [1.0, x, 1.0 - x]),
        "summer" => ramp(&|x| [x, 0.5 + x / 2.0, 0.4]),
        "autumn" => ramp(&|x| [1.0, x, 0.0]),
        "winter" => ramp(&|x| [0.0, x, 1.0 - x / 2.0]),
        "turbo" => interpolate_table(&TURBO_256, n),
        // Hue `k/n` (so the map does not repeat red), full saturation/value.
        "hsv" => (0..n).map(|k| hsv_to_rgb(k as f64 / n as f64)).collect(),
        // sqrt((2*gray + hot)/3) per channel.
        "pink" => {
            let g = ramp(&|x| [x, x, x]);
            g.iter()
                .zip(hot(n))
                .map(|(g, h)| std::array::from_fn(|c| ((2.0 * g[c] + h[c]) / 3.0).sqrt()))
                .collect()
        }
        // Measured: a one-row `lines` is pure blue, not the first order colour.
        "lines" if n == 1 => vec![[0.0, 0.0, 1.0]],
        "lines" => cycle(&COLOR_ORDER, n),
        "colorcube" => colorcube(n),
        "prism" => cycle(
            &[
                [1.0, 0.0, 0.0],
                [1.0, 0.5, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [2.0 / 3.0, 0.0, 1.0],
            ],
            n,
        ),
        "flag" => cycle(
            &[
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 1.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 0.0],
            ],
            n,
        ),
        "white" => vec![[1.0; 3]; n],
        "cubehelix" => ramp(&cubehelix),
        "ocean" => ocean(n),
        "rainbow" => ramp(&rainbow),
        _ => return None,
    })
}

fn viridis(n: usize) -> Colormap {
    interpolate_table(&VIRIDIS_256, n)
}

/// A measured 256-row table resampled to `n` rows.
fn interpolate_table(table: &[Rgb], n: usize) -> Colormap {
    if n == table.len() {
        return table.to_vec();
    }
    // Measured (viridis, turbo): a one-row map is the last (brightest) row.
    if n == 1 {
        return vec![table[table.len() - 1]];
    }
    // Linear interpolation of the table at n equally spaced points.
    let last = (table.len() - 1) as f64;
    (0..n)
        .map(|k| {
            let pos = k as f64 * last / (n - 1) as f64;
            let i = (pos.floor() as usize).min(table.len() - 2);
            let f = pos - i as f64;
            let (a, b) = (table[i], table[i + 1]);
            [
                a[0] + f * (b[0] - a[0]),
                a[1] + f * (b[1] - a[1]),
                a[2] + f * (b[2] - a[2]),
            ]
        })
        .collect()
}

/// Piecewise-linear blue-cyan-yellow-red map; each channel is a trapezoid
/// of width `3n-1` rows built from `n = ceil(m/4)` ramp steps.
fn jet(m: usize) -> Colormap {
    if m == 0 {
        return Vec::new();
    }
    let n = m.div_ceil(4);
    let mut u: Vec<f64> = (1..=n).map(|k| k as f64 / n as f64).collect();
    u.extend(std::iter::repeat_n(1.0, n - 1));
    u.extend((1..=n).rev().map(|k| k as f64 / n as f64));
    let shift = n.div_ceil(2) as isize - isize::from(m % 4 == 1);
    let mut map = vec![[0.0; 3]; m];
    for (j, &value) in u.iter().enumerate() {
        let g = shift + j as isize; // zero-based row of the green trapezoid
        for (channel, row) in [(0, g + n as isize), (1, g), (2, g - n as isize)] {
            if (0..m as isize).contains(&row) {
                map[row as usize][channel] = value;
            }
        }
    }
    map
}

/// Black-red-yellow-white: red ramps over the first `fix(3m/8)` rows, then
/// green over as many, then blue over the remainder.
fn hot(m: usize) -> Colormap {
    let n = 3 * m / 8;
    let ramp = |k: usize, start: usize, len: usize| {
        if k < start {
            0.0
        } else if k - start < len {
            (k - start + 1) as f64 / len as f64
        } else {
            1.0
        }
    };
    let rest = m.saturating_sub(2 * n);
    (0..m)
        .map(|k| [ramp(k, 0, n), ramp(k, n, n), ramp(k, 2 * n, rest)])
        .collect()
}

/// `n` rows repeating `colors` from the start.
fn cycle(colors: &[Rgb], n: usize) -> Colormap {
    colors.iter().copied().cycle().take(n).collect()
}

/// HSV to RGB at full saturation and value; `h` in [0,1).
fn hsv_to_rgb(h: f64) -> Rgb {
    let h6 = 6.0 * h;
    let sector = (h6.floor() as usize).min(5);
    let f = h6 - sector as f64;
    let (up, down) = (f, 1.0 - f);
    match sector {
        0 => [1.0, up, 0.0],
        1 => [down, 1.0, 0.0],
        2 => [0.0, 1.0, up],
        3 => [0.0, down, 1.0],
        4 => [up, 0.0, 1.0],
        _ => [1.0, 0.0, down],
    }
}

/// Dave Green's cubehelix (2011) with start 0.5, rotations -1.5, hue 1,
/// gamma 1, clipped to [0,1].
fn cubehelix(x: f64) -> Rgb {
    let (start, rotations, hue) = (0.5, -1.5, 1.0);
    let amp = hue * x * (1.0 - x) / 2.0;
    let phi = 2.0 * std::f64::consts::PI * (start / 3.0 + rotations * x);
    let (s, c) = phi.sin_cos();
    [
        x + amp * (-0.14861 * c + 1.78277 * s),
        x + amp * (-0.29227 * c - 0.90649 * s),
        x + amp * (1.97294 * c),
    ]
    .map(|v| v.clamp(0.0, 1.0))
}

/// Piecewise-linear red-yellow-green-blue-violet map (fitted breakpoints
/// 0.4, 0.6, 0.8).
fn rainbow(x: f64) -> Rgb {
    let r = if x <= 0.4 {
        1.0
    } else if x <= 0.6 {
        1.0 - 5.0 * (x - 0.4)
    } else if x <= 0.8 {
        0.0
    } else {
        (x - 0.8) * 10.0 / 3.0
    };
    let g = if x <= 0.4 {
        2.5 * x
    } else if x <= 0.6 {
        1.0
    } else if x <= 0.8 {
        1.0 - 5.0 * (x - 0.6)
    } else {
        0.0
    };
    let b = if x <= 0.6 {
        0.0
    } else if x <= 0.8 {
        5.0 * (x - 0.6)
    } else {
        1.0
    };
    [r, g, b]
}

/// Black-blue-white: blue ramps over all rows, green over the last
/// `2*fix(n/3)` rows and red over the last `fix(n/3)` rows.
fn ocean(n: usize) -> Colormap {
    // Octave errors for n = 2; we give black and white.
    if n == 2 {
        return vec![[0.0; 3], [1.0; 3]];
    }
    let s = n / 3;
    let tail = |k: usize, len: usize| {
        let start = n - 1 - len; // last row with value 0
        if len == 0 || k <= start {
            0.0
        } else {
            (k - start) as f64 / len as f64
        }
    };
    (0..n)
        .map(|k| {
            let b = if n <= 1 {
                0.0
            } else {
                k as f64 / (n - 1) as f64
            };
            [tail(k, s), tail(k, 2 * s), b]
        })
        .collect()
}

/// Regular RGB cube colours (without greys and pure red/green/blue shades),
/// then extra red, green, blue and grey ramps. Fitted to measurements:
/// `k = floor(cbrt(n))` steps per channel (blue gets `k-1` when `n == k^3`);
/// with `rest` rows left, each colour ramp has `(rest-1)/4` rows and grey the
/// remainder; `n < 9` is a plain grey ramp.
fn colorcube(n: usize) -> Colormap {
    let grey = |len: usize| -> Colormap {
        (0..len)
            .map(|k| {
                let v = if len <= 1 {
                    0.0
                } else {
                    k as f64 / (len - 1) as f64
                };
                [v; 3]
            })
            .collect()
    };
    if n < 9 {
        return grey(n);
    }
    let mut k = 2;
    while (k + 1) * (k + 1) * (k + 1) <= n {
        k += 1;
    }
    let kb = if k * k * k == n { k - 1 } else { k };
    let level = |i: usize, steps: usize| i as f64 / (steps - 1) as f64;
    let mut map = Colormap::new();
    for b in 0..kb {
        for r in 0..k {
            for g in 0..k {
                let c = [level(r, k), level(g, k), level(b, kb)];
                let nonzero = c.iter().filter(|&&v| v != 0.0).count();
                let is_grey = c[0] == c[1] && c[1] == c[2];
                if nonzero >= 2 && !is_grey {
                    map.push(c);
                }
            }
        }
    }
    let rest = n - map.len();
    let len = (rest - 1) / 4;
    for channel in 0..3 {
        for j in 1..=len {
            let mut c = [0.0; 3];
            c[channel] = j as f64 / len as f64;
            map.push(c);
        }
    }
    map.extend(grey(rest - 3 * len));
    map
}

/// Index into a colormap for a scaled scalar, the IndyMAT extractor rule
/// `1+fix(N*(c-clim(1))/(clim(2)-clim(1)))` clipped to `[1,N]`, returned
/// zero-based.
pub fn scaled_index(value: f64, clim: [f64; 2], rows: usize) -> usize {
    let n = rows as f64;
    let raw = 1.0 + (n * (value - clim[0]) / (clim[1] - clim[0])).trunc();
    (raw.clamp(1.0, n) as usize).saturating_sub(1)
}
