//! Reading Figure-Data records. Nonfinite numbers arrive as JSON `null`.

use crate::device::Rgb;
use indy_graphics::json::Json;

/// A column-major array descriptor; `null` reads as NaN.
#[derive(Debug, Clone, Default)]
pub struct Desc {
    pub shape: Vec<usize>,
    pub values: Vec<f64>,
}

impl Desc {
    pub fn at(&self, i: usize) -> f64 {
        self.values.get(i).copied().unwrap_or(f64::NAN)
    }
}

pub fn num_of(j: &Json) -> f64 {
    j.as_f64().unwrap_or(f64::NAN)
}

pub fn num(j: &Json, key: &str) -> Option<f64> {
    j.get(key).and_then(Json::as_f64)
}

pub fn text<'a>(j: &'a Json, key: &str) -> &'a str {
    j.get(key).and_then(Json::as_str).unwrap_or("")
}

pub fn flag(j: &Json, key: &str) -> bool {
    j.get(key).and_then(Json::as_bool).unwrap_or(false)
}

pub fn nums(j: &Json, key: &str) -> Vec<f64> {
    j.get(key)
        .and_then(Json::as_array)
        .map_or_else(Vec::new, |a| a.iter().map(num_of).collect())
}

pub fn strings(j: &Json, key: &str) -> Vec<String> {
    j.get(key)
        .and_then(Json::as_array)
        .map_or_else(Vec::new, |a| {
            a.iter()
                .map(|s| s.as_str().unwrap_or("").to_string())
                .collect()
        })
}

/// An RGB triple; `None` for `"none"` or anything else.
pub fn rgb(j: &Json, key: &str) -> Option<Rgb> {
    let a = j.get(key)?.as_array()?;
    if a.len() != 3 {
        return None;
    }
    let c = [a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?];
    c.iter().all(|v| v.is_finite()).then_some(c)
}

pub fn desc(j: &Json, key: &str) -> Option<Desc> {
    let d = j.get(key)?;
    let shape = d
        .get("shape")?
        .as_array()?
        .iter()
        .map(|v| num_of(v) as usize)
        .collect();
    let values = d.get("values")?.as_array()?.iter().map(num_of).collect();
    Some(Desc { shape, values })
}

pub fn series(axes: &Json) -> &[Json] {
    axes.get("series").and_then(Json::as_array).unwrap_or(&[])
}

pub fn dash(style: &str) -> &'static [f64] {
    match style {
        "--" => &[7.0, 4.0],
        ":" => &[2.0, 3.0],
        "-." => &[7.0, 3.0, 2.0, 3.0],
        _ => &[],
    }
}

/// The UI's `plotFormat`.
pub fn plot_format(v: f64) -> String {
    if !v.is_finite() {
        return if v.is_nan() {
            "NaN".into()
        } else if v > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    let a = v.abs();
    if a != 0.0 && !(1e-4..1e5).contains(&a) {
        let s = format!("{v:.4e}");
        return match s.split_once('e') {
            Some((m, e)) if !e.starts_with('-') => format!("{m}e+{e}"),
            _ => s,
        };
    }
    // toPrecision(6) then shortest form.
    let digits = (5 - if a == 0.0 {
        0
    } else {
        a.log10().floor() as i32
    })
    .max(0) as usize;
    let rounded: f64 = format!("{v:.digits$}").parse().unwrap_or(v);
    format!("{rounded}")
}
