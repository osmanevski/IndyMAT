//! Data cleaning: missing values (`ismissing`, `standardizeMissing`,
//! `rmmissing`, `fillmissing`), outliers (`isoutlier`, `rmoutliers`,
//! `filloutliers`) and `smoothdata`.
//!
//! Own code written from the MathWorks documentation of each function.
//! Registered after the older modules, so `ismissing` (strtype.rs) and
//! `rmmissing` (extra.rs) are replaced here; every behaviour of the old
//! versions is kept.
//!
//! Standard missing values: NaN for double/single (real or imaginary
//! part), `' '` for char, `''` (empty char) inside a cell array,
//! `<missing>` for string arrays. Integer and logical arrays have none.

mod fill;
mod outlier;
mod smooth;

use indy_value::{Array, Num, Value};

use super::math::{default_dim, take};
use super::{arg_f64, arg_usize, err, need, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "ismissing", ismissing);
    // strtype.rs also dispatches `ismissing` for string arguments.
    it.string_methods.insert("ismissing", ismissing);
    reg(it, "standardizeMissing", standardize_missing);
    reg(it, "rmmissing", rmmissing);
    reg(it, "fillmissing", fill::fillmissing);
    reg(it, "isoutlier", outlier::isoutlier);
    reg(it, "rmoutliers", outlier::rmoutliers);
    reg(it, "filloutliers", outlier::filloutliers);
    reg(it, "smoothdata", smooth::smoothdata);
}

// ---------------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------------

/// Linear positions of every 1-D slice along `dim` (slices in column-major
/// order of the remaining dimensions).
pub(super) fn slice_positions(dims: &[usize], dim: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    super::math::slices(dims, dim, |base, stride, len| {
        out.push((0..len).map(|k| base + k * stride).collect());
    });
    out
}

pub(super) fn take_n(v: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    Ok(v.into_iter().take(nargout.max(1)).collect())
}

pub(super) fn dim_arg(v: &Value, fname: &str) -> R<usize> {
    let k = arg_f64(v, fname)?;
    if k < 1.0 || k.fract() != 0.0 || !k.is_finite() || v.numel() != 1 {
        return Err(err(
            "MATLAB:getdimarg:dimensionMustBePositiveInteger",
            "Dimension argument must be a positive integer scalar within indexing range.",
        ));
    }
    Ok(k as usize - 1)
}

pub(super) fn first_dim(dims: &[usize]) -> usize {
    default_dim(dims)
}

/// Explicit error for table / timetable inputs (not in the Rust engine yet).
pub(super) fn no_table(v: &Value, fname: &str) -> R<()> {
    let c = v.class_name();
    if c == "table" || c == "timetable" {
        return Err(err(
            &format!("MATLAB:{}:tableUnsupported", fname),
            format!(
                "{}: {} input is not supported in the Rust engine yet.",
                fname, c
            ),
        ));
    }
    Ok(())
}

/// Text of a char row / string scalar argument, lower-cased.
pub(super) fn opt_name(v: &Value) -> Option<String> {
    if v.is_text_scalar() {
        v.to_str().map(|s| s.to_ascii_lowercase())
    } else {
        None
    }
}

pub(super) fn is_text_arg(v: &Value) -> bool {
    v.is_text_scalar()
}

/// Standard missing mask, `None` for classes without a definition of
/// missing that the caller should reject.
pub(super) fn std_missing(v: &Value) -> Option<Vec<bool>> {
    Some(match v {
        Value::Num(n) => (0..n.re.len())
            .map(|k| n.re[k].is_nan() || n.im_at(k).is_nan())
            .collect(),
        Value::Bool(b) => vec![false; b.data.len()],
        Value::Char(c) => c.data.iter().map(|ch| *ch == ' ').collect(),
        Value::Cell(c) => c
            .data
            .iter()
            .map(|e| matches!(e, Value::Char(_)) && e.is_empty())
            .collect(),
        Value::Str(s) => s.data.iter().map(|x| x.is_none()).collect(),
        _ => return None,
    })
}

/// Moving-window half widths `(before, after)` from a window argument
/// (MATLAB `movmean` rules: odd k centred, even k one more before,
/// non-integer k uses floor(k/2) on both sides, `[b f]` explicit).
pub(super) fn window_arg(v: &Value, fname: &str) -> R<(usize, usize)> {
    let bad = || {
        err(
            &format!("MATLAB:{}:wrongWindowLength", fname),
            format!(
                "{}: window length must be a finite positive scalar or a 2-element vector of nonnegative integers.",
                fname
            ),
        )
    };
    let k = match v {
        Value::Num(n) if n.im.is_none() => n.re.clone(),
        Value::Bool(_) => v.to_f64_vec().unwrap_or_default(),
        _ => return Err(bad()),
    };
    match k.as_slice() {
        [k0] => {
            if !k0.is_finite() || *k0 <= 0.0 {
                return Err(bad());
            }
            if k0.fract() == 0.0 {
                let k = *k0 as usize;
                if k % 2 == 1 {
                    Ok(((k - 1) / 2, (k - 1) / 2))
                } else {
                    Ok((k / 2, k / 2 - 1))
                }
            } else {
                let h = (k0 / 2.0).floor() as usize;
                Ok((h, h))
            }
        }
        [b, f] => {
            let ok = |x: f64| x.is_finite() && x >= 0.0 && x.fract() == 0.0;
            if !(ok(*b) && ok(*f)) {
                return Err(bad());
            }
            Ok((*b as usize, *f as usize))
        }
        _ => Err(bad()),
    }
}

/// Median of the non-NaN values (NaN when there are none).
pub(super) fn median_omit(v: &[f64]) -> f64 {
    let mut s: Vec<f64> = v.iter().copied().filter(|e| !e.is_nan()).collect();
    if s.is_empty() {
        return f64::NAN;
    }
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = s.len();
    if n % 2 == 1 {
        s[n / 2]
    } else {
        0.5 * (s[n / 2 - 1] + s[n / 2])
    }
}

pub(super) fn mean_omit(v: &[f64]) -> f64 {
    let s: Vec<f64> = v.iter().copied().filter(|e| !e.is_nan()).collect();
    if s.is_empty() {
        f64::NAN
    } else {
        s.iter().sum::<f64>() / s.len() as f64
    }
}

/// Sample standard deviation (n-1) of the non-NaN values; 0 for one value.
pub(super) fn std_omit(v: &[f64]) -> f64 {
    let s: Vec<f64> = v.iter().copied().filter(|e| !e.is_nan()).collect();
    let n = s.len();
    if n == 0 {
        return f64::NAN;
    }
    if n == 1 {
        return 0.0;
    }
    let m = s.iter().sum::<f64>() / n as f64;
    (s.iter().map(|e| (e - m).powi(2)).sum::<f64>() / (n - 1) as f64).sqrt()
}

// ---------------------------------------------------------------------------
// ismissing / standardizeMissing
// ---------------------------------------------------------------------------

/// Missing-value indicators, split by kind.
#[derive(Default)]
struct Indicators {
    nums: Vec<(f64, f64)>,
    chars: Vec<char>,
    texts: Vec<Option<String>>,
}

fn collect_indicators(v: &Value, ind: &mut Indicators, fname: &str) -> R<()> {
    match v {
        Value::Num(n) => {
            for k in 0..n.re.len() {
                ind.nums.push((n.re[k], n.im_at(k)));
            }
        }
        Value::Bool(b) => {
            for x in &b.data {
                ind.nums.push((*x as u8 as f64, 0.0));
            }
        }
        Value::Char(c) => {
            ind.chars.extend(c.data.iter().copied());
            if let Some(rows) = v.char_rows() {
                if c.data.is_empty() {
                    ind.texts.push(Some(String::new()));
                }
                ind.texts.extend(rows.into_iter().map(Some));
            }
        }
        Value::Str(s) => {
            for x in &s.data {
                if let Some(t) = x {
                    let cs: Vec<char> = t.chars().collect();
                    if cs.len() == 1 {
                        ind.chars.push(cs[0]);
                    }
                }
                ind.texts.push(x.clone());
            }
        }
        Value::Cell(c) => {
            for e in &c.data {
                if matches!(e, Value::Cell(_)) {
                    return Err(err(
                        &format!("MATLAB:{}:IndicatorsInvalidType", fname),
                        format!("{}: indicators cannot be nested cell arrays.", fname),
                    ));
                }
                collect_indicators(e, ind, fname)?;
            }
        }
        other => {
            return Err(err(
                &format!("MATLAB:{}:IndicatorsInvalidType", fname),
                format!(
                    "{}: missing-value indicators of class {} are not supported.",
                    fname,
                    other.class_name()
                ),
            ));
        }
    }
    Ok(())
}

/// Mask of `v` entries matching any indicator (MATLAB: an explicit
/// indicator replaces the standard missing values).
fn indicator_mask(v: &Value, indv: &Value, fname: &str) -> R<Vec<bool>> {
    let mut ind = Indicators::default();
    collect_indicators(indv, &mut ind, fname)?;
    let is_cell = matches!(indv, Value::Cell(_));
    let mismatch = |what: &str| {
        err(
            &format!("MATLAB:{}:IndicatorsInvalidType", fname),
            format!(
                "{}: indicators must be {} for {} input.",
                fname,
                what,
                v.class_name()
            ),
        )
    };
    let num_eq = |x: (f64, f64), y: &(f64, f64)| {
        let eq = |a: f64, b: f64| a == b || (a.is_nan() && b.is_nan());
        eq(x.0, y.0) && eq(x.1, y.1)
    };
    Ok(match v {
        Value::Num(n) => {
            if !is_cell && !matches!(indv, Value::Num(_) | Value::Bool(_)) {
                return Err(mismatch("numeric"));
            }
            (0..n.re.len())
                .map(|k| ind.nums.iter().any(|y| num_eq((n.re[k], n.im_at(k)), y)))
                .collect()
        }
        Value::Bool(b) => {
            if !is_cell && !matches!(indv, Value::Num(_) | Value::Bool(_)) {
                return Err(mismatch("numeric or logical"));
            }
            b.data
                .iter()
                .map(|x| ind.nums.iter().any(|y| num_eq((*x as u8 as f64, 0.0), y)))
                .collect()
        }
        Value::Char(c) => {
            if !is_cell && !indv.is_text() {
                return Err(mismatch("text"));
            }
            c.data.iter().map(|x| ind.chars.contains(x)).collect()
        }
        Value::Cell(c) => {
            if !is_cell && !indv.is_text() {
                return Err(mismatch("text"));
            }
            let mut r = Vec::with_capacity(c.data.len());
            for e in &c.data {
                r.push(match e {
                    Value::Char(_) => {
                        let s = if e.is_empty() {
                            Some(String::new())
                        } else {
                            e.char_rows()
                                .filter(|rows| rows.len() == 1)
                                .map(|mut rows| rows.remove(0))
                        };
                        s.is_some() && ind.texts.contains(&s)
                    }
                    _ => false,
                });
            }
            r
        }
        Value::Str(s) => {
            if !is_cell && !indv.is_text() {
                return Err(mismatch("text"));
            }
            s.data.iter().map(|x| ind.texts.contains(x)).collect()
        }
        other => {
            return Err(err(
                &format!("MATLAB:{}:FirstInputInvalid", fname),
                format!(
                    "{}: missing-value indicators are not supported for {} input.",
                    fname,
                    other.class_name()
                ),
            ));
        }
    })
}

fn ismissing(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "ismissing")?;
    super::at_most(&a, 2, "ismissing")?;
    let v = &a[0];
    no_table(v, "ismissing")?;
    let r = match a.get(1) {
        Some(ind) => indicator_mask(v, ind, "ismissing")?,
        // Other classes (struct, cell of non-text, handles) have no
        // missing value: all false, as before.
        None => std_missing(v).unwrap_or_else(|| vec![false; v.numel()]),
    };
    Ok(vec![Value::Bool(Array::new(v.dims(), r))])
}

fn standardize_missing(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let f = "standardizeMissing";
    need(&a, 2, f)?;
    if a.len() > 2 {
        return Err(err(
            "MATLAB:standardizeMissing:unsupportedOption",
            "standardizeMissing: name-value options (DataVariables, ReplaceValues) apply to tables, which are not supported in the Rust engine yet.",
        ));
    }
    let v = &a[0];
    no_table(v, f)?;
    let mask = indicator_mask(v, &a[1], f)?;
    let out = match v {
        Value::Num(n) => {
            if n.class.is_int() {
                return Err(err(
                    "MATLAB:standardizeMissing:integerInput",
                    format!(
                        "standardizeMissing: {} arrays have no standard missing value.",
                        n.class.name()
                    ),
                ));
            }
            let mut n = n.clone();
            for (k, m) in mask.iter().enumerate() {
                if *m {
                    n.re[k] = f64::NAN;
                    if let Some(im) = &mut n.im {
                        im[k] = 0.0;
                    }
                }
            }
            n.narrow();
            Value::Num(n)
        }
        Value::Bool(_) => {
            return Err(err(
                "MATLAB:standardizeMissing:logicalInput",
                "standardizeMissing: logical arrays have no standard missing value.",
            ));
        }
        Value::Char(c) => {
            let mut c = c.clone();
            for (k, m) in mask.iter().enumerate() {
                if *m {
                    c.data[k] = ' ';
                }
            }
            Value::Char(c)
        }
        Value::Cell(c) => {
            let mut c = c.clone();
            for (k, m) in mask.iter().enumerate() {
                if *m {
                    c.data[k] = Value::str("");
                }
            }
            Value::Cell(c)
        }
        Value::Str(s) => {
            let mut s = s.clone();
            for (k, m) in mask.iter().enumerate() {
                if *m {
                    s.data[k] = None;
                }
            }
            Value::Str(s)
        }
        // indicator_mask already rejected the other classes.
        other => other.clone(),
    };
    Ok(vec![out])
}

// ---------------------------------------------------------------------------
// rmmissing
// ---------------------------------------------------------------------------

/// Remove rows (dim 1) or columns (dim 2) of a vector/matrix whose count
/// of flagged entries reaches `minnum`. Returns `[B, TF]`.
pub(super) fn remove_flagged(
    v: &Value,
    flagged: &[bool],
    dim: Option<usize>,
    minnum: usize,
    fname: &str,
) -> R<(Value, Value)> {
    let dims = v.dims();
    if dims.len() > 2 {
        return Err(err(
            &format!("MATLAB:{}:NDArray", fname),
            format!("{}: input must be a vector or a matrix.", fname),
        ));
    }
    let (r, c) = (dims[0], dims[1]);
    let d = dim.unwrap_or(if r == 1 && c != 1 { 1 } else { 0 });
    if d > 1 {
        return Err(err(
            &format!("MATLAB:{}:InvalidDim", fname),
            format!("{}: dimension must be 1 or 2.", fname),
        ));
    }
    let (pos, odims, tf) = if d == 0 {
        let rm: Vec<bool> = (0..r)
            .map(|i| (0..c).filter(|j| flagged[i + j * r]).count() >= minnum)
            .collect();
        let kept: Vec<usize> = (0..r).filter(|i| !rm[*i]).collect();
        let pos: Vec<usize> = (0..c)
            .flat_map(|j| kept.iter().map(move |i| i + j * r))
            .collect();
        (
            pos,
            vec![kept.len(), c],
            Value::Bool(Array::new(vec![r, 1], rm)),
        )
    } else {
        let rm: Vec<bool> = (0..c)
            .map(|j| (0..r).filter(|i| flagged[i + j * r]).count() >= minnum)
            .collect();
        let kept: Vec<usize> = (0..c).filter(|j| !rm[*j]).collect();
        let pos: Vec<usize> = kept
            .iter()
            .flat_map(|j| (0..r).map(move |i| i + j * r))
            .collect();
        (
            pos,
            vec![r, kept.len()],
            Value::Bool(Array::new(vec![1, c], rm)),
        )
    };
    Ok((take(v, pos, odims)?, tf))
}

fn rmmissing(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let f = "rmmissing";
    need(&a, 1, f)?;
    let v = &a[0];
    no_table(v, f)?;
    let mut dim: Option<usize> = None;
    let mut minnum = 1usize;
    let mut i = 1;
    while i < a.len() {
        if let Some(s) = opt_name(&a[i]) {
            match s.as_str() {
                "minnummissing" => {
                    let Some(n) = a.get(i + 1) else {
                        return Err(err(
                            "MATLAB:rmmissing:missingValue",
                            "MinNumMissing requires a value.",
                        ));
                    };
                    minnum = arg_usize(n, f)?.max(1);
                    i += 2;
                    continue;
                }
                "datavariables" => {
                    return Err(err(
                        "MATLAB:rmmissing:tableUnsupported",
                        "rmmissing: 'DataVariables' applies to table input, which is not supported in the Rust engine yet.",
                    ));
                }
                _ => {
                    return Err(err(
                        "MATLAB:rmmissing:unknownOption",
                        format!(
                            "rmmissing: unknown option '{}'.",
                            a[i].to_str().unwrap_or(s)
                        ),
                    ));
                }
            }
        }
        dim = Some(dim_arg(&a[i], f)?);
        i += 1;
    }
    let Some(missing) = std_missing(v) else {
        return Err(err(
            "MATLAB:rmmissing:FirstInputInvalid",
            "rmmissing: first input must be an array or a cell array of character vectors.",
        ));
    };
    let (out, tf) = remove_flagged(v, &missing, dim, minnum, f)?;
    take_n(vec![out, tf], nargout)
}

/// Real f64 data of a numeric/logical input (complex and text rejected).
pub(super) fn real_data(v: &Value, fname: &str) -> R<Num> {
    no_table(v, fname)?;
    match v {
        Value::Num(n) if n.im.is_none() => Ok(n.clone()),
        Value::Bool(_) => Ok(v.to_num().unwrap_or_else(Num::empty)),
        Value::Num(_) => Err(err(
            &format!("MATLAB:{}:ComplexInput", fname),
            format!("{}: complex input is not supported.", fname),
        )),
        other => Err(err(
            &format!("MATLAB:{}:FirstInputInvalid", fname),
            format!(
                "{}: input must be numeric or logical, not {}.",
                fname,
                other.class_name()
            ),
        )),
    }
}
