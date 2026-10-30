//! Colour rules of the UI (`frontend/figure_color_utils.cjs`).

use crate::device::Rgb;
use crate::rec::{Desc, desc};
use indy_graphics::json::Json;

/// Plot colours. `adjust` is the UI's 3:1 contrast rule against the themed
/// canvas; the paper theme of exported files keeps every colour as given.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub background: Rgb,
    pub text: Rgb,
    pub grid: Rgb,
    pub axis: Rgb,
    pub adjust: bool,
}

fn hex(v: u32) -> Rgb {
    [
        ((v >> 16) & 255) as f64 / 255.0,
        ((v >> 8) & 255) as f64 / 255.0,
        (v & 255) as f64 / 255.0,
    ]
}

impl Theme {
    /// The UI's light theme (`--plot-*` tokens of `static/style.css`).
    pub fn ui_light() -> Theme {
        Theme {
            background: hex(0xdbe8f3),
            text: hex(0x111a32),
            grid: hex(0x92aec6),
            axis: hex(0x3d5162),
            adjust: true,
        }
    }

    /// The UI's dark theme.
    pub fn ui_dark() -> Theme {
        Theme {
            background: hex(0x050719),
            text: hex(0xe0e0e0),
            grid: hex(0x35424c),
            axis: hex(0x465c6d),
            adjust: true,
        }
    }

    /// White paper, for exported figures.
    pub fn paper() -> Theme {
        Theme {
            background: [1.0, 1.0, 1.0],
            text: hex(0x111111),
            grid: hex(0xd9d9d9),
            axis: hex(0x262626),
            adjust: false,
        }
    }

    pub fn with_background(mut self, background: Rgb) -> Theme {
        self.background = background;
        self
    }
}

pub fn clamp01(c: Rgb) -> Rgb {
    [
        c[0].clamp(0.0, 1.0),
        c[1].clamp(0.0, 1.0),
        c[2].clamp(0.0, 1.0),
    ]
}

pub fn luminance(rgb: Rgb) -> f64 {
    const K: [f64; 3] = [0.2126, 0.7152, 0.0722];
    (0..3)
        .map(|i| {
            let v = rgb[i];
            K[i] * if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        })
        .sum()
}

pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// The UI's `adjustForContrast`: plot ink when below 3:1 on the canvas.
pub fn adjust(rgb: Rgb, theme: &Theme) -> Rgb {
    let bounded = clamp01(rgb);
    if theme.adjust && contrast(bounded, theme.background) < 3.0 {
        theme.text
    } else {
        bounded
    }
}

fn map_color(map: &Desc, index: i64) -> Rgb {
    let n = map.shape.first().copied().unwrap_or(0);
    if n == 0 {
        return [0.0, 0.0, 0.0];
    }
    let i = index.clamp(0, n as i64 - 1) as usize;
    [map.at(i), map.at(i + n), map.at(i + 2 * n)]
}

/// Colour context of one axes.
pub struct AxesColors {
    pub clim: [f64; 2],
    pub map: Desc,
}

impl AxesColors {
    pub fn of(axes: &Json) -> AxesColors {
        let clim = crate::rec::nums(axes, "clim");
        AxesColors {
            clim: if clim.len() == 2 {
                [clim[0], clim[1]]
            } else {
                [0.0, 1.0]
            },
            map: desc(axes, "colormap").unwrap_or_default(),
        }
    }

    /// The UI's `descriptorColor`, for one lookup. Loops use [`Resolved`].
    pub fn descriptor(&self, d: &Json, owner: usize) -> Option<Rgb> {
        self.resolve(d).at(owner)
    }

    /// A colour descriptor read once, for many lookups.
    pub fn resolve<'a>(&'a self, d: &Json) -> Resolved<'a> {
        let mode = d.get("mode").and_then(Json::as_str).unwrap_or("none");
        let class = d
            .get("cdata_class")
            .and_then(Json::as_str)
            .unwrap_or("double")
            .to_string();
        let data = if mode == "none" || mode == "constant" {
            Desc::default()
        } else {
            desc(d, "data").unwrap_or_default()
        };
        Resolved {
            axes: self,
            none: mode == "none",
            constant: if mode == "constant" {
                crate::rec::rgb(d, "rgb")
            } else {
                None
            },
            is_constant: mode == "constant",
            truecolor: d.get("encoding").and_then(Json::as_str) == Some("truecolor"),
            direct: d.get("mapping").and_then(Json::as_str) == Some("direct"),
            single: mode == "flat" && data.shape.first() == Some(&1),
            divisor: match class.as_str() {
                "uint8" => 255.0,
                "uint16" => 65535.0,
                _ => 1.0,
            },
            class,
            data,
        }
    }

    pub fn index(&self, value: f64, direct: bool, class: &str) -> Option<Rgb> {
        if !value.is_finite() {
            return None;
        }
        let n = self.map.shape.first().copied().unwrap_or(0) as f64;
        let integer = class.contains("int");
        let [low, high] = self.clim;
        let span = high - low;
        let fraction = if span.is_finite() {
            (value - low) / span
        } else {
            (value / 2.0 - low / 2.0) / (high / 2.0 - low / 2.0)
        };
        let index = if direct {
            if integer {
                value as i64
            } else {
                (value.trunc() as i64).saturating_sub(1)
            }
        } else if value <= low {
            0
        } else if value >= high {
            n as i64 - 1
        } else {
            (n * fraction).floor() as i64
        };
        Some(map_color(&self.map, index))
    }

    /// The first colour of a one-row descriptor (or the entry at `owner`).
    pub fn patch(&self, d: &Json, owner: usize) -> Option<Rgb> {
        let r = self.resolve(d);
        r.at(if r.single { 0 } else { owner })
    }
}

/// See [`AxesColors::resolve`].
pub struct Resolved<'a> {
    axes: &'a AxesColors,
    none: bool,
    constant: Option<Rgb>,
    is_constant: bool,
    truecolor: bool,
    direct: bool,
    single: bool,
    divisor: f64,
    class: String,
    data: Desc,
}

impl Resolved<'_> {
    pub fn at(&self, owner: usize) -> Option<Rgb> {
        if self.none {
            return None;
        }
        if self.is_constant {
            return self.constant;
        }
        if self.truecolor {
            let n = self.data.shape.first().copied().unwrap_or(0)
                * self.data.shape.get(1).copied().unwrap_or(0);
            let c = [
                self.data.at(owner),
                self.data.at(owner + n),
                self.data.at(owner + 2 * n),
            ];
            if c.iter().any(|v| !v.is_finite()) {
                return None;
            }
            return Some([
                c[0] / self.divisor,
                c[1] / self.divisor,
                c[2] / self.divisor,
            ]);
        }
        self.axes
            .index(self.data.at(owner), self.direct, &self.class)
    }
}

fn at_luminance(rgb: Rgb, target: f64) -> Rgb {
    let min = rgb[0].min(rgb[1]).min(rgb[2]);
    let max = rgb[0].max(rgb[1]).max(rgb[2]);
    let light = (min + max) / 2.0;
    let saturation = if max == min {
        0.0
    } else {
        (max - min) / (1.0 - (2.0 * light - 1.0).abs())
    };
    let at = |l: f64| -> Rgb {
        let chroma = (1.0 - (2.0 * l - 1.0).abs()) * saturation;
        let f = |v: f64| {
            l - chroma / 2.0
                + if max == min {
                    0.0
                } else {
                    (v - min) / (max - min)
                } * chroma
        };
        [f(rgb[0]), f(rgb[1]), f(rgb[2])]
    };
    let (mut low, mut high) = (0.0f64, 1.0f64);
    for _ in 0..52 {
        let mid = (low + high) / 2.0;
        if luminance(at(mid)) < target {
            low = mid;
        } else {
            high = mid;
        }
    }
    clamp01(at((low + high) / 2.0))
}

/// The UI's `adjustContourPalette` for one contour group.
pub fn adjust_contour_palette(rgbs: &[Rgb], theme: &Theme) -> Vec<Rgb> {
    const MINIMUM: f64 = 3.0;
    let bg = theme.background;
    if !theme.adjust || rgbs.iter().all(|c| contrast(*c, bg) >= MINIMUM) {
        return rgbs.to_vec();
    }
    let lum = luminance(bg);
    let lighter = contrast([1.0, 1.0, 1.0], bg) >= contrast([0.0, 0.0, 0.0], bg);
    let threshold = if lighter {
        MINIMUM * (lum + 0.05) - 0.05
    } else {
        (lum + 0.05) / MINIMUM - 0.05
    };
    if !(0.0..=1.0).contains(&threshold) {
        return rgbs
            .iter()
            .map(|c| {
                if contrast(*c, bg) >= MINIMUM {
                    *c
                } else {
                    theme.text
                }
            })
            .collect();
    }
    let t = if lighter { threshold } else { 1.0 - threshold };
    let xs: Vec<f64> = rgbs
        .iter()
        .map(|c| {
            if lighter {
                luminance(*c)
            } else {
                1.0 - luminance(*c)
            }
        })
        .collect();
    let start = xs.iter().copied().fold(f64::INFINITY, f64::min);
    let end = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let pivot = if end > t {
        (t + 0.20).min(end)
    } else {
        (t + 0.20).min(1.0)
    };
    rgbs.iter()
        .zip(&xs)
        .map(|(c, &x)| {
            if x >= pivot {
                return *c;
            }
            let v = if start == end {
                t
            } else {
                t + (x - start) * (pivot - t) / (pivot - start)
            };
            at_luminance(*c, if lighter { v } else { 1.0 - v })
        })
        .collect()
}

/// The display colour of every contour line of an axes, by series index.
pub fn contour_display_colors(axes: &Json, colors: &AxesColors, theme: &Theme) -> Vec<Option<Rgb>> {
    let series = axes.get("series").and_then(Json::as_array).unwrap_or(&[]);
    let mut out = vec![None; series.len()];
    let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
    for (i, s) in series.iter().enumerate() {
        if s.get("role").and_then(Json::as_str) != Some("contour") {
            continue;
        }
        let Some(cc) = s.get("contour_color") else {
            continue;
        };
        if cc.get("association").and_then(Json::as_str) != Some("level") {
            continue;
        }
        let key = s
            .get("contour_group")
            .map_or("legacy".to_string(), Json::to_json_string);
        match groups.iter_mut().find(|g| g.0 == key) {
            Some(g) => g.1.push(i),
            None => groups.push((key, vec![i])),
        }
    }
    for (_, members) in groups {
        let base: Vec<Rgb> = members
            .iter()
            .map(|&i| {
                series[i]
                    .get("contour_color")
                    .and_then(|c| colors.descriptor(c, 0))
                    .unwrap_or(theme.text)
            })
            .collect();
        for (&i, c) in members.iter().zip(adjust_contour_palette(&base, theme)) {
            out[i] = Some(c);
        }
    }
    out
}
