//! `datetime`, `duration`, `calendarDuration` and the classic serial date
//! functions (`datenum`, `datestr`, `datevec`, `weekday`, `eomday`).
//!
//! Storage: a datetime keeps `#v`, wall-clock seconds since 1970-01-01
//! 00:00:00 in the proleptic Gregorian calendar (`NaN` is NaT), plus the
//! public `Format` and `TimeZone`. A duration keeps `#v` in seconds and
//! `Format`. A calendarDuration keeps `#m` months, `#d` days and `#t`
//! seconds, plus `Format`.
//!
//! Time zones: `''` (unzoned), `'UTC'` and the local zone (`'local'` or its
//! name) are supported; the local offset comes from the C library
//! (`localtime_r`). Other zone names and daylight-saving transitions inside
//! arithmetic are not modelled.

use indy_syntax::ast::BinOp;
use indy_value::{Array, Num, Object, Value, ops};

use super::{
    ClassOps, bools, class_is, data, ew_map, is_native, name_values, new_obj, obj_of, set_data,
    text_list,
};
use crate::builtins::{err, need, reg};
use crate::{BuiltinFn, Interp, R};

const DAY: f64 = 86_400.0;
/// datenum of 1970-01-01.
const EPOCH_DATENUM: f64 = 719_529.0;

pub const DEFAULT_FMT: &str = "dd-MMM-uuuu HH:mm:ss";
pub const DATE_FMT: &str = "dd-MMM-uuuu";

pub static DATETIME: ClassOps = ClassOps {
    name: "datetime",
    get: dt_get,
    set: dt_set,
    convert: dt_convert,
    binary: dt_binary,
    disp: |o| grid_text(o, &dt_texts(o)),
    display: None,
    fill: |_| f64::NAN,
    methods: DT_METHODS,
};

pub static DURATION: ClassOps = ClassOps {
    name: "duration",
    get: dur_get,
    set: dur_set,
    convert: dur_convert,
    binary: dur_binary,
    disp: |o| grid_text(o, &dur_texts(o)),
    display: None,
    fill: |_| 0.0,
    methods: DUR_METHODS,
};

pub static CALDUR: ClassOps = ClassOps {
    name: "calendarDuration",
    get: cd_get,
    set: cd_set,
    convert: cd_convert,
    binary: cd_binary,
    disp: |o| grid_text(o, &cd_texts(o)),
    display: None,
    fill: |_| 0.0,
    methods: CD_METHODS,
};

// ---------------------------------------------------------------------------
// Calendar arithmetic (proleptic Gregorian; Hinnant's civil algorithms)
// ---------------------------------------------------------------------------

pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn month_days(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
    }
}

/// Seconds for (possibly out-of-range or fractional) components.
pub fn secs_of(y: f64, mo: f64, d: f64, h: f64, mi: f64, s: f64) -> f64 {
    if [y, mo, d, h, mi, s].iter().any(|x| x.is_nan()) {
        return f64::NAN;
    }
    if [y, mo, d, h, mi, s].iter().any(|x| x.is_infinite()) {
        let sum = y + mo + d + h + mi + s;
        return if sum.is_nan() {
            f64::NAN
        } else {
            sum.signum() * f64::INFINITY
        };
    }
    let m0 = mo.floor() as i64 - 1;
    let yy = y.floor() as i64 + m0.div_euclid(12);
    let mm = m0.rem_euclid(12) + 1;
    let days = days_from_civil(yy, mm, 1) as f64 + (d - 1.0);
    days * DAY + h * 3600.0 + mi * 60.0 + s
}

/// (year, month, day, hour, minute, second) of `secs`.
pub fn parts(secs: f64) -> (f64, f64, f64, f64, f64, f64) {
    if !secs.is_finite() {
        let n = if secs.is_nan() { f64::NAN } else { secs };
        return (n, n, n, n, n, n);
    }
    let days = (secs / DAY).floor();
    let mut sod = secs - days * DAY;
    if sod >= DAY {
        sod = DAY - 1e-9;
    }
    let (y, m, d) = civil_from_days(days as i64);
    let h = (sod / 3600.0).floor();
    let mi = ((sod - h * 3600.0) / 60.0).floor();
    let s = sod - h * 3600.0 - mi * 60.0;
    (y as f64, m as f64, d as f64, h, mi, s)
}

/// Add calendar months and days to a wall-clock time (MATLAB order:
/// months with end-of-month clamping, then days, then time).
fn cal_add(secs: f64, months: f64, days: f64, time: f64) -> f64 {
    if !secs.is_finite() || !months.is_finite() || !days.is_finite() {
        return secs + months + days + time;
    }
    let (y, m, d, h, mi, s) = parts(secs);
    let m0 = m as i64 - 1 + months as i64;
    let yy = y as i64 + m0.div_euclid(12);
    let mm = m0.rem_euclid(12) + 1;
    let dd = (d as i64).min(month_days(yy, mm));
    let base = days_from_civil(yy, mm, dd) as f64 * DAY + h * 3600.0 + mi * 60.0 + s;
    base + days * DAY + time
}

/// Local time now as wall-clock seconds, and the UTC offset.
fn now_local() -> (f64, f64) {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let utc = d.as_secs() as f64 + d.subsec_nanos() as f64 / 1e9;
    let off = crate::builtins::extra::sys::utc_offset(d.as_secs() as i64) as f64;
    (utc + off, off)
}

fn now_utc() -> f64 {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    d.as_secs() as f64 + d.subsec_nanos() as f64 / 1e9
}

/// Name of the local time zone (IANA name when known).
fn local_zone_name() -> String {
    if let Ok(tz) = std::env::var("TZ")
        && !tz.is_empty()
    {
        return tz.trim_start_matches(':').to_string();
    }
    if let Ok(p) = std::fs::read_link("/etc/localtime") {
        let s = p.to_string_lossy().to_string();
        if let Some(i) = s.find("zoneinfo/") {
            return s[i + 9..].to_string();
        }
    }
    "local".into()
}

/// Offset in seconds of a supported zone at wall-clock `secs`.
fn zone_offset(zone: &str, secs: f64) -> R<f64> {
    if zone.is_empty() || zone.eq_ignore_ascii_case("UTC") || zone == "Z" {
        return Ok(0.0);
    }
    if zone == "local" || zone == local_zone_name() {
        let guess = if secs.is_finite() { secs as i64 } else { 0 };
        let off = crate::builtins::extra::sys::utc_offset(guess);
        let off = crate::builtins::extra::sys::utc_offset(guess - off);
        return Ok(off as f64);
    }
    if let Some(off) = parse_offset(zone) {
        return Ok(off);
    }
    Err(err(
        "MATLAB:datetime:UnknownTimeZone",
        format!(
            "Time zone '{}' is not supported yet (supported: '', 'UTC', 'local' ({}), and fixed offsets like '+03:00').",
            zone,
            local_zone_name()
        ),
    ))
}

fn parse_offset(z: &str) -> Option<f64> {
    let (sign, rest) = match z.as_bytes().first()? {
        b'+' => (1.0, &z[1..]),
        b'-' => (-1.0, &z[1..]),
        _ => return None,
    };
    let digits: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
    let (h, m) = match digits.len() {
        1 | 2 => (digits.parse::<f64>().ok()?, 0.0),
        3 | 4 => (
            digits[..digits.len() - 2].parse::<f64>().ok()?,
            digits[digits.len() - 2..].parse::<f64>().ok()?,
        ),
        _ => return None,
    };
    Some(sign * (h * 3600.0 + m * 60.0))
}

// ---------------------------------------------------------------------------
// Names
// ---------------------------------------------------------------------------

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// 1 = Sunday.
fn weekday_of(secs: f64) -> f64 {
    let days = (secs / DAY).floor() as i64;
    // 1970-01-01 was a Thursday (5).
    ((days + 4).rem_euclid(7) + 1) as f64
}

fn day_of_year(secs: f64) -> f64 {
    let (y, _, _, _, _, _) = parts(secs);
    let start = days_from_civil(y as i64, 1, 1) as f64;
    (secs / DAY).floor() - start + 1.0
}

// ---------------------------------------------------------------------------
// Format strings (MATLAB datetime / ICU symbols)
// ---------------------------------------------------------------------------

enum Tok {
    Lit(String),
    Sym(char, usize),
}

fn tokens(fmt: &str) -> Vec<Tok> {
    let cs: Vec<char> = fmt.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c == '\'' {
            let mut lit = String::new();
            i += 1;
            while i < cs.len() {
                if cs[i] == '\'' {
                    if i + 1 < cs.len() && cs[i + 1] == '\'' {
                        lit.push('\'');
                        i += 2;
                        continue;
                    }
                    break;
                }
                lit.push(cs[i]);
                i += 1;
            }
            i += 1;
            out.push(Tok::Lit(lit));
        } else if c.is_ascii_alphabetic() {
            let mut n = 1;
            while i + n < cs.len() && cs[i + n] == c {
                n += 1;
            }
            out.push(Tok::Sym(c, n));
            i += n;
        } else {
            out.push(Tok::Lit(c.to_string()));
            i += 1;
        }
    }
    out
}

pub fn fmt_dt(secs: f64, fmt: &str) -> String {
    if secs.is_nan() {
        return "NaT".into();
    }
    if secs.is_infinite() {
        return if secs > 0.0 {
            "Inf".into()
        } else {
            "-Inf".into()
        };
    }
    let (y, m, d, h, mi, s) = parts(secs);
    let mut out = String::new();
    for t in tokens(fmt) {
        match t {
            Tok::Lit(l) => out.push_str(&l),
            Tok::Sym(c, n) => match c {
                'y' | 'u' => {
                    if n == 2 {
                        out.push_str(&format!("{:02}", (y as i64).rem_euclid(100)));
                    } else {
                        out.push_str(&format!("{:0w$}", y as i64, w = n));
                    }
                }
                'Q' => {
                    let q = ((m as i64 - 1) / 3) + 1;
                    out.push_str(&match n {
                        1 | 2 => format!("{:0w$}", q, w = n),
                        3 => format!("Q{}", q),
                        _ => format!("{} quarter", ["1st", "2nd", "3rd", "4th"][(q - 1) as usize]),
                    });
                }
                'M' | 'L' => {
                    let k = (m as usize).clamp(1, 12) - 1;
                    out.push_str(&match n {
                        1 => format!("{}", m as i64),
                        2 => format!("{:02}", m as i64),
                        3 => MONTHS[k][..3].to_string(),
                        4 => MONTHS[k].to_string(),
                        _ => MONTHS[k][..1].to_string(),
                    });
                }
                'd' => out.push_str(&format!("{:0w$}", d as i64, w = n)),
                'D' => out.push_str(&format!("{:0w$}", day_of_year(secs) as i64, w = n)),
                'e' | 'E' | 'c' => {
                    let k = weekday_of(secs) as usize - 1;
                    out.push_str(&match (c, n) {
                        ('e', 1) | ('c', 1) => format!("{}", k + 1),
                        ('e', 2) | ('c', 2) => format!("{:02}", k + 1),
                        (_, 4) => WEEKDAYS[k].to_string(),
                        (_, 5) => WEEKDAYS[k][..1].to_string(),
                        (_, 6) => WEEKDAYS[k][..2].to_string(),
                        _ => WEEKDAYS[k][..3].to_string(),
                    });
                }
                'H' => out.push_str(&format!("{:0w$}", h as i64, w = n)),
                'h' => {
                    let hh = (h as i64) % 12;
                    out.push_str(&format!("{:0w$}", if hh == 0 { 12 } else { hh }, w = n));
                }
                'k' => out.push_str(&format!(
                    "{:0w$}",
                    if h == 0.0 { 24 } else { h as i64 },
                    w = n
                )),
                'K' => out.push_str(&format!("{:0w$}", (h as i64) % 12, w = n)),
                'm' => out.push_str(&format!("{:0w$}", mi as i64, w = n)),
                's' => out.push_str(&format!("{:0w$}", s.floor() as i64, w = n)),
                'S' => {
                    let frac = s - s.floor();
                    let scaled = (frac * 10f64.powi(n as i32)).floor() as i64;
                    out.push_str(&format!("{:0w$}", scaled, w = n));
                }
                'a' => out.push_str(if h < 12.0 { "AM" } else { "PM" }),
                'G' => out.push_str(if y > 0.0 { "CE" } else { "BCE" }),
                'z' | 'Z' | 'x' | 'X' | 'O' | 'V' => {}
                other => out.push_str(&other.to_string().repeat(n)),
            },
        }
    }
    out
}

/// Parse `text` with a datetime format; wall-clock seconds.
pub fn parse_dt(text: &str, fmt: &str, pivot: i64) -> Option<f64> {
    let cs: Vec<char> = text.trim().chars().collect();
    let mut i = 0usize;
    let (mut y, mut mo, mut d, mut h, mut mi, mut s) = (1970.0, 1.0, 1.0, 0.0, 0.0, 0.0);
    let mut pm: Option<bool> = None;
    let mut doy: Option<f64> = None;
    let mut off: Option<f64> = None;
    let toks = tokens(fmt);
    let digits = |i: &mut usize, min: usize, max: usize| -> Option<f64> {
        let st = *i;
        while *i < cs.len() && cs[*i].is_ascii_digit() && *i - st < max {
            *i += 1;
        }
        if *i - st < min {
            return None;
        }
        cs[st..*i].iter().collect::<String>().parse::<f64>().ok()
    };
    let word = |i: &mut usize, names: &[&str], len: Option<usize>| -> Option<usize> {
        let rest: String = cs[*i..].iter().collect::<String>().to_lowercase();
        for (k, n) in names.iter().enumerate() {
            let cand = match len {
                Some(l) => n[..l.min(n.len())].to_lowercase(),
                None => n.to_lowercase(),
            };
            if rest.starts_with(&cand) {
                *i += cand.chars().count();
                return Some(k);
            }
        }
        None
    };
    for (ti, t) in toks.iter().enumerate() {
        let next_is_digit_sym =
            matches!(toks.get(ti + 1), Some(Tok::Sym(c, _)) if "yuMdHhmsSD".contains(*c));
        match t {
            Tok::Lit(l) => {
                for lc in l.chars() {
                    if lc == ' ' {
                        while i < cs.len() && cs[i] == ' ' {
                            i += 1;
                        }
                        continue;
                    }
                    if i < cs.len() && cs[i].eq_ignore_ascii_case(&lc) {
                        i += 1;
                    } else {
                        return None;
                    }
                }
            }
            Tok::Sym(c, n) => {
                let n = *n;
                let fixed = next_is_digit_sym;
                match c {
                    'y' | 'u' => {
                        if n == 2 {
                            let v = digits(&mut i, 2, 2)?;
                            let century = pivot.div_euclid(100) * 100;
                            let mut yy = century + v as i64;
                            if yy >= pivot + 100 {
                                yy -= 100;
                            }
                            if yy < pivot {
                                yy += 100;
                            }
                            y = yy as f64;
                        } else {
                            let neg = i < cs.len() && cs[i] == '-';
                            if neg {
                                i += 1;
                            }
                            let v = digits(&mut i, 1, if fixed { n.max(4) } else { 9 })?;
                            y = if neg { -v } else { v };
                        }
                    }
                    'M' | 'L' => {
                        mo = match n {
                            1 | 2 => digits(&mut i, 1, 2)?,
                            3 => word(&mut i, &MONTHS, Some(3))? as f64 + 1.0,
                            _ => {
                                let k = word(&mut i, &MONTHS, None)
                                    .or_else(|| word(&mut i, &MONTHS, Some(3)))?;
                                k as f64 + 1.0
                            }
                        };
                    }
                    'd' => d = digits(&mut i, 1, if fixed { n.max(2) } else { 2 })?,
                    'D' => doy = Some(digits(&mut i, 1, 3)?),
                    'e' | 'E' | 'c' => {
                        if n <= 2 && *c != 'E' {
                            digits(&mut i, 1, 2)?;
                        } else {
                            word(&mut i, &WEEKDAYS, None)
                                .or_else(|| word(&mut i, &WEEKDAYS, Some(3)))?;
                        }
                    }
                    'H' | 'k' => h = digits(&mut i, 1, 2)?,
                    'h' | 'K' => h = digits(&mut i, 1, 2)?,
                    'm' => mi = digits(&mut i, 1, 2)?,
                    's' => s = digits(&mut i, 1, 2)?,
                    'S' => {
                        let st = i;
                        while i < cs.len() && cs[i].is_ascii_digit() {
                            i += 1;
                        }
                        if i == st {
                            return None;
                        }
                        let frac: String = cs[st..i].iter().collect();
                        s += format!("0.{}", frac).parse::<f64>().ok()?;
                    }
                    'a' => {
                        let rest: String = cs[i..].iter().collect::<String>().to_uppercase();
                        if rest.starts_with("AM") {
                            pm = Some(false);
                        } else if rest.starts_with("PM") {
                            pm = Some(true);
                        } else {
                            return None;
                        }
                        i += 2;
                    }
                    'z' | 'Z' | 'x' | 'X' | 'O' => {
                        if i < cs.len() && (cs[i] == 'Z' || cs[i] == 'z') {
                            off = Some(0.0);
                            i += 1;
                        } else {
                            let st = i;
                            if i < cs.len() && (cs[i] == '+' || cs[i] == '-') {
                                i += 1;
                                while i < cs.len() && (cs[i].is_ascii_digit() || cs[i] == ':') {
                                    i += 1;
                                }
                                let t: String = cs[st..i].iter().collect();
                                off = parse_offset(&t);
                            }
                        }
                    }
                    _ => return None,
                }
            }
        }
    }
    if i != cs.len() {
        return None;
    }
    if let Some(p) = pm {
        if p && h < 12.0 {
            h += 12.0;
        } else if !p && h == 12.0 {
            h = 0.0;
        }
    }
    if !(1.0..=12.0).contains(&mo) || d < 1.0 || d > month_days(y as i64, mo as i64) as f64 {
        return None;
    }
    if h > 24.0 || mi > 59.0 || s >= 61.0 {
        return None;
    }
    let mut v = match doy {
        Some(n) => secs_of(y, 1.0, n, h, mi, s),
        None => secs_of(y, mo, d, h, mi, s),
    };
    if let Some(o) = off {
        // Convert to the local wall clock of an unzoned result: keep UTC.
        v -= o;
    }
    Some(v)
}

const GUESS_FORMATS: &[&str] = &[
    "yyyy-MM-dd'T'HH:mm:ss.SSSXXX",
    "yyyy-MM-dd'T'HH:mm:ssXXX",
    "yyyy-MM-dd'T'HH:mm:ss.SSS",
    "yyyy-MM-dd'T'HH:mm:ss",
    "yyyy-MM-dd'T'HH:mm",
    "yyyy-MM-dd HH:mm:ss.SSS",
    "yyyy-MM-dd HH:mm:ss",
    "yyyy-MM-dd HH:mm",
    "yyyy-MM-dd",
    "dd-MMM-yyyy HH:mm:ss.SSS",
    "dd-MMM-yyyy HH:mm:ss",
    "dd-MMM-yyyy HH:mm",
    "dd-MMM-yyyy",
    "dd-MMM-yy",
    "MM/dd/yyyy HH:mm:ss",
    "MM/dd/yyyy HH:mm",
    "MM/dd/yyyy",
    "yyyy/MM/dd HH:mm:ss",
    "yyyy/MM/dd",
    "dd.MM.yyyy HH:mm:ss",
    "dd.MM.yyyy HH:mm",
    "dd.MM.yyyy",
    "dd MMMM yyyy",
    "dd MMM yyyy",
    "MMMM d, yyyy",
    "MMM d, yyyy",
    "yyyyMMdd'T'HHmmss",
    "HH:mm:ss",
    "HH:mm",
];

fn default_pivot() -> i64 {
    let (y, ..) = parts(now_local().0);
    y as i64 - 50
}

/// Parse text without a known format: (secs, has time of day, format).
fn guess_dt(text: &str) -> Option<(f64, bool, &'static str)> {
    let t = text.trim();
    let pivot = default_pivot();
    for f in GUESS_FORMATS {
        if let Some(v) = parse_dt(t, f, pivot) {
            let v = if f.starts_with("HH") {
                // Time only: today's date.
                let today = (now_local().0 / DAY).floor() * DAY;
                today + v - secs_of(1970.0, 1.0, 1.0, 0.0, 0.0, 0.0)
            } else {
                v
            };
            return Some((v, f.contains("HH"), f));
        }
    }
    None
}

/// Whether text reads as a calendar date (with or without a time), as
/// `readtable` detects datetime columns.
pub fn looks_like_date(text: &str) -> bool {
    matches!(guess_dt(text), Some((_, _, f)) if !f.starts_with("HH"))
}

// ---------------------------------------------------------------------------
// Construction helpers
// ---------------------------------------------------------------------------

pub fn dt_obj(dims: Vec<usize>, v: Vec<f64>, fmt: &str, zone: &str) -> Object {
    let mut o = new_obj("datetime", dims);
    set_data(&mut o, "#v", v);
    o.set("Format", Value::str(fmt));
    o.set("TimeZone", Value::str(zone));
    o
}

pub fn dt_value(dims: Vec<usize>, v: Vec<f64>, fmt: &str) -> Value {
    Value::Object(Box::new(dt_obj(dims, v, fmt, "")))
}

pub fn nat_array(dims: Vec<usize>) -> Value {
    let n = dims.iter().product();
    dt_value(dims, vec![f64::NAN; n], DEFAULT_FMT)
}

pub fn dur_obj(dims: Vec<usize>, v: Vec<f64>, fmt: &str) -> Object {
    let mut o = new_obj("duration", dims);
    set_data(&mut o, "#v", v);
    o.set("Format", Value::str(fmt));
    o
}

pub fn duration_value(dims: Vec<usize>, v: Vec<f64>) -> Value {
    Value::Object(Box::new(dur_obj(dims, v, "hh:mm:ss")))
}

fn cd_obj(dims: Vec<usize>, m: Vec<f64>, d: Vec<f64>, t: Vec<f64>, fmt: &str) -> Object {
    let mut o = new_obj("calendarDuration", dims);
    set_data(&mut o, "#m", m);
    set_data(&mut o, "#d", d);
    set_data(&mut o, "#t", t);
    o.set("Format", Value::str(fmt));
    o
}

fn sval(o: &Object, k: &str) -> String {
    o.get(k).and_then(|v| v.to_str()).unwrap_or_default()
}

fn wrap(o: Object) -> Value {
    Value::Object(Box::new(o))
}

/// Broadcast two shapes: result dims and index pairs.
/// Result shape and index pairs of a broadcast.
type Pairs = (Vec<usize>, Vec<(usize, usize)>);

fn bcast(ad: &[usize], bd: &[usize]) -> R<Pairs> {
    let ia: Vec<usize> = (0..ad.iter().product()).collect();
    let ib: Vec<usize> = (0..bd.iter().product()).collect();
    ops::zip_bcast(ad, &ia, bd, &ib, |i, j| (i, j))
}

fn num_data(v: &Value) -> Option<(Vec<usize>, Vec<f64>)> {
    match v {
        Value::Num(n) if n.im.is_none() => Some((n.dims.clone(), n.re.clone())),
        Value::Bool(b) => Some((
            b.dims.clone(),
            b.data.iter().map(|x| if *x { 1.0 } else { 0.0 }).collect(),
        )),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// datetime: properties, conversion, operators, text
// ---------------------------------------------------------------------------

fn comp_array(o: &Object, k: usize) -> Value {
    let v: Vec<f64> = data(o, "#v")
        .iter()
        .map(|&s| {
            let p = parts(s);
            [p.0, p.1, p.2, p.3, p.4, p.5][k]
        })
        .collect();
    Value::Num(Num::real(o.dims.clone(), v))
}

fn dt_get(o: &Object, name: &str) -> R<Value> {
    Ok(match name {
        "Format" | "TimeZone" => o.get(name).cloned().unwrap_or_else(|| Value::str("")),
        "Year" => comp_array(o, 0),
        "Month" => comp_array(o, 1),
        "Day" => comp_array(o, 2),
        "Hour" => comp_array(o, 3),
        "Minute" => comp_array(o, 4),
        "Second" => comp_array(o, 5),
        "SystemTimeZone" => Value::str(&local_zone_name()),
        _ => {
            return Err(err(
                "MATLAB:datetime:UnrecognizedProperty",
                format!(
                    "Unrecognized method, property, or field '{}' for class 'datetime'.",
                    name
                ),
            ));
        }
    })
}

fn dt_set(o: &mut Object, name: &str, v: Value) -> R<()> {
    match name {
        "Format" => {
            let f = v.to_str().ok_or_else(|| {
                err(
                    "MATLAB:datetime:InvalidFormat",
                    "Format must be a character vector or string.",
                )
            })?;
            let f = match f.as_str() {
                "default" => DEFAULT_FMT.to_string(),
                "defaultdate" => DATE_FMT.to_string(),
                _ => f,
            };
            o.set("Format", Value::str(&f));
        }
        "TimeZone" => {
            let z = v.to_str().unwrap_or_default();
            let z = if z == "local" { local_zone_name() } else { z };
            let old = sval(o, "TimeZone");
            if !old.is_empty() && !z.is_empty() {
                let vals = data(o, "#v");
                let mut out = Vec::with_capacity(vals.len());
                for s in vals {
                    let utc = s - zone_offset(&old, s)?;
                    out.push(utc + zone_offset(&z, utc)?);
                }
                set_data(o, "#v", out);
            } else if !z.is_empty() {
                zone_offset(&z, 0.0)?;
            }
            o.set("TimeZone", Value::str(&z));
        }
        "Year" | "Month" | "Day" | "Hour" | "Minute" | "Second" => {
            let k = ["Year", "Month", "Day", "Hour", "Minute", "Second"]
                .iter()
                .position(|n| *n == name)
                .unwrap();
            let (nd, nv) = num_data(&v).ok_or_else(|| {
                err(
                    "MATLAB:datetime:InvalidComponent",
                    format!("{} must be numeric.", name),
                )
            })?;
            let vals = data(o, "#v");
            let (dims, pairs) = bcast(&o.dims, &nd)?;
            if dims != o.dims {
                return Err(err(
                    "MATLAB:datetime:InvalidComponent",
                    format!(
                        "{} must be a scalar or the size of the datetime array.",
                        name
                    ),
                ));
            }
            let out = pairs
                .iter()
                .map(|&(i, j)| {
                    let p = parts(vals[i]);
                    let mut c = [p.0, p.1, p.2, p.3, p.4, p.5];
                    c[k] = nv[j];
                    secs_of(c[0], c[1], c[2], c[3], c[4], c[5])
                })
                .collect();
            set_data(o, "#v", out);
        }
        _ => {
            return Err(err(
                "MATLAB:datetime:UnrecognizedProperty",
                format!("Unrecognized property '{}' for class 'datetime'.", name),
            ));
        }
    }
    Ok(())
}

/// Text to datetime with the formats of `like` first.
fn text_to_dt(items: &[String], like_fmt: Option<&str>) -> R<Vec<f64>> {
    items
        .iter()
        .map(|t| {
            if t.is_empty() || t.eq_ignore_ascii_case("NaT") {
                return Ok(f64::NAN);
            }
            if let Some(f) = like_fmt
                && let Some(v) = parse_dt(t, f, default_pivot())
            {
                return Ok(v);
            }
            match keyword_dt(t) {
                Some(v) => Ok(v),
                None => guess_dt(t).map(|g| g.0).ok_or_else(|| {
                    err(
                        "MATLAB:datetime:ParseErr",
                        format!(
                            "Could not recognize the date/time format of '{}'. Specify a format using the 'InputFormat' parameter.",
                            t
                        ),
                    )
                }),
            }
        })
        .collect()
}

fn keyword_dt(t: &str) -> Option<f64> {
    let (now, _) = now_local();
    let today = (now / DAY).floor() * DAY;
    Some(match t.to_ascii_lowercase().as_str() {
        "now" => now,
        "today" => today,
        "yesterday" => today - DAY,
        "tomorrow" => today + DAY,
        _ => return None,
    })
}

fn dt_convert(like: &Object, v: &Value) -> R<Object> {
    if let Some(o) = obj_of(v) {
        if o.class == "datetime" {
            let mut o = o.clone();
            let (lz, oz) = (sval(like, "TimeZone"), sval(&o, "TimeZone"));
            if lz.is_empty() != oz.is_empty() {
                return Err(err(
                    "MATLAB:datetime:IncompatibleTZ",
                    "Cannot combine datetime arrays with and without time zones.",
                ));
            }
            if lz != oz {
                dt_set(&mut o, "TimeZone", Value::str(&lz))?;
            }
            return Ok(o);
        }
        return Err(err(
            "MATLAB:datetime:InvalidComparison",
            format!("Cannot combine a datetime with a {}.", o.class),
        ));
    }
    if v.is_text() || matches!(v, Value::Cell(_)) {
        let items = text_list(v).ok_or_else(|| {
            err(
                "MATLAB:datetime:InvalidText",
                "Text must be character vectors or strings.",
            )
        })?;
        let vals = text_to_dt(&items, Some(&sval(like, "Format")))?;
        let dims = match v {
            Value::Char(_) => vec![items.len(), 1],
            other => other.dims(),
        };
        let dims = if items.len() == 1 { vec![1, 1] } else { dims };
        return Ok(dt_obj(
            dims,
            vals,
            &sval(like, "Format"),
            &sval(like, "TimeZone"),
        ));
    }
    if let Some((d, x)) = num_data(v)
        && x.iter().all(|e| e.is_nan())
    {
        return Ok(dt_obj(d, x, &sval(like, "Format"), &sval(like, "TimeZone")));
    }
    Err(err(
        "MATLAB:datetime:InvalidConversion",
        format!(
            "Cannot convert a value of class {} to datetime (numbers are not dates; use datetime(..., 'ConvertFrom', 'datenum')).",
            v.class_name()
        ),
    ))
}

/// Seconds of a duration-like operand (duration, or numbers as days).
fn as_secs(v: &Value) -> Option<(Vec<usize>, Vec<f64>)> {
    if class_is(v, "duration") {
        let o = obj_of(v).unwrap();
        return Some((o.dims.clone(), data(o, "#v")));
    }
    num_data(v).map(|(d, x)| (d, x.into_iter().map(|e| e * DAY).collect()))
}

fn cmp_op(op: BinOp) -> bool {
    matches!(
        op,
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
    )
}

fn num_val(dims: Vec<usize>, v: Vec<f64>) -> Value {
    Value::Num(Num::real(dims, v))
}

fn compare(op: BinOp, ad: Vec<usize>, a: Vec<f64>, bd: Vec<usize>, b: Vec<f64>) -> R<Value> {
    ops::binary(op, &num_val(ad, a), &num_val(bd, b))
}

fn dt_binary(op: BinOp, a: &Value, b: &Value) -> R<Value> {
    let (da, db) = (class_is(a, "datetime"), class_is(b, "datetime"));
    if cmp_op(op) {
        let like = obj_of(if da { a } else { b }).unwrap();
        let x = dt_convert(like, a)?;
        let y = dt_convert(like, b)?;
        return compare(
            op,
            x.dims.clone(),
            data(&x, "#v"),
            y.dims.clone(),
            data(&y, "#v"),
        );
    }
    match op {
        BinOp::Add | BinOp::Sub => {}
        _ => return Err(super::undefined_op(op, a, b)),
    }
    if da && db {
        if op == BinOp::Add {
            return Err(err(
                "MATLAB:datetime:DatetimeAddition",
                "Addition is not defined for two datetime values; subtract them to get a duration.",
            ));
        }
        let x = obj_of(a).unwrap();
        let y = dt_convert(x, b)?;
        let (xv, yv) = (data(x, "#v"), data(&y, "#v"));
        let (dims, pairs) = bcast(&x.dims, &y.dims)?;
        let v = pairs.iter().map(|&(i, j)| xv[i] - yv[j]).collect();
        return Ok(wrap(dur_obj(dims, v, "hh:mm:ss")));
    }
    let (dt, other, dt_first) = if da { (a, b, true) } else { (b, a, false) };
    if op == BinOp::Sub && !dt_first {
        return Err(err(
            "MATLAB:datetime:SubtractionNotDefined",
            format!(
                "Subtracting a datetime from a {} is not defined.",
                other.class_name()
            ),
        ));
    }
    let o = obj_of(dt).unwrap();
    let sign = if op == BinOp::Sub { -1.0 } else { 1.0 };
    let xv = data(o, "#v");
    if class_is(other, "calendarDuration") {
        let c = obj_of(other).unwrap();
        let (m, d, t) = (data(c, "#m"), data(c, "#d"), data(c, "#t"));
        let (dims, pairs) = bcast(&o.dims, &c.dims)?;
        let v = pairs
            .iter()
            .map(|&(i, j)| cal_add(xv[i], sign * m[j], sign * d[j], sign * t[j]))
            .collect();
        let mut r = o.clone();
        r.dims = dims;
        set_data(&mut r, "#v", v);
        return Ok(wrap(r));
    }
    let (sd, sv) = as_secs(other).ok_or_else(|| super::undefined_op(op, a, b))?;
    let (dims, pairs) = bcast(&o.dims, &sd)?;
    let v = pairs.iter().map(|&(i, j)| xv[i] + sign * sv[j]).collect();
    let mut r = o.clone();
    r.dims = dims;
    set_data(&mut r, "#v", v);
    Ok(wrap(r))
}

fn dt_texts(o: &Object) -> Vec<String> {
    let f = sval(o, "Format");
    data(o, "#v").iter().map(|&s| fmt_dt(s, &f)).collect()
}

/// MATLAB grid layout of element texts (3-space indent and gaps).
fn grid_text(o: &Object, texts: &[String]) -> String {
    let d = &o.dims;
    if texts.is_empty() {
        return String::new();
    }
    let r = d[0];
    let c: usize = d[1..].iter().product();
    let w = texts.iter().map(|t| t.chars().count()).max().unwrap_or(0);
    let mut s = String::new();
    for i in 0..r {
        s.push_str("   ");
        let row: Vec<String> = (0..c)
            .map(|j| {
                let t = &texts[i + j * r];
                format!("{}{}", " ".repeat(w - t.chars().count()), t)
            })
            .collect();
        s.push_str(&row.join("   "));
        s.push('\n');
    }
    s
}

// ---------------------------------------------------------------------------
// duration
// ---------------------------------------------------------------------------

fn dur_get(o: &Object, name: &str) -> R<Value> {
    match name {
        "Format" => Ok(o
            .get("Format")
            .cloned()
            .unwrap_or_else(|| Value::str("hh:mm:ss"))),
        _ => Err(err(
            "MATLAB:duration:UnrecognizedProperty",
            format!(
                "Unrecognized method, property, or field '{}' for class 'duration'.",
                name
            ),
        )),
    }
}

fn valid_dur_fmt(f: &str) -> bool {
    let base = f.split('.').next().unwrap_or("");
    let frac_ok = f
        .split('.')
        .nth(1)
        .is_none_or(|x| !x.is_empty() && x.chars().all(|c| c == 'S'));
    matches!(base, "y" | "d" | "h" | "m" | "s") && !f.contains('.')
        || (matches!(base, "dd:hh:mm:ss" | "hh:mm:ss" | "mm:ss" | "hh:mm") && frac_ok)
        || (matches!(base, "y" | "d" | "h" | "m" | "s") && frac_ok)
}

fn dur_set(o: &mut Object, name: &str, v: Value) -> R<()> {
    match name {
        "Format" => {
            let f = v.to_str().unwrap_or_default();
            if !valid_dur_fmt(&f) {
                return Err(err(
                    "MATLAB:duration:UnrecognizedFormat",
                    format!(
                        "'{}' is not a valid duration format (use 'y', 'd', 'h', 'm', 's', 'dd:hh:mm:ss', 'hh:mm:ss', 'mm:ss' or 'hh:mm', optionally with '.SSS').",
                        f
                    ),
                ));
            }
            o.set("Format", Value::str(&f));
            Ok(())
        }
        _ => Err(err(
            "MATLAB:duration:UnrecognizedProperty",
            format!("Unrecognized property '{}' for class 'duration'.", name),
        )),
    }
}

fn dur_convert(like: &Object, v: &Value) -> R<Object> {
    if let Some(o) = obj_of(v)
        && o.class == "duration"
    {
        return Ok(o.clone());
    }
    if let Some((d, x)) = num_data(v) {
        return Ok(dur_obj(
            d,
            x.into_iter().map(|e| e * DAY).collect(),
            &sval(like, "Format"),
        ));
    }
    if v.is_text() || matches!(v, Value::Cell(_)) {
        let items = text_list(v).unwrap_or_default();
        let vals = items
            .iter()
            .map(|t| {
                parse_dur_text(t).ok_or_else(|| {
                    err(
                        "MATLAB:duration:ParseErr",
                        format!("Could not convert '{}' to a duration.", t),
                    )
                })
            })
            .collect::<R<Vec<f64>>>()?;
        let dims = match v {
            Value::Char(_) => vec![1, 1],
            other => other.dims(),
        };
        return Ok(dur_obj(dims, vals, &sval(like, "Format")));
    }
    Err(err(
        "MATLAB:duration:InvalidConversion",
        format!(
            "Cannot convert a value of class {} to duration.",
            v.class_name()
        ),
    ))
}

/// `[-][dd:]hh:mm:ss[.fff]` or `mm:ss` text.
fn parse_dur_text(t: &str) -> Option<f64> {
    let t = t.trim();
    let (neg, t) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t),
    };
    let parts: Vec<f64> = t
        .split(':')
        .map(|p| p.trim().parse::<f64>().ok())
        .collect::<Option<_>>()?;
    let v = match parts.len() {
        2 => parts[0] * 3600.0 + parts[1] * 60.0,
        3 => parts[0] * 3600.0 + parts[1] * 60.0 + parts[2],
        4 => parts[0] * DAY + parts[1] * 3600.0 + parts[2] * 60.0 + parts[3],
        _ => return None,
    };
    Some(if neg { -v } else { v })
}

fn short(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Inf".into() } else { "-Inf".into() };
    }
    if x.fract() == 0.0 && x.abs() < 1e15 {
        return format!("{}", x as i64);
    }
    indy_value::format::fmt_g(x, 5, false, false)
}

pub fn fmt_dur(s: f64, f: &str) -> String {
    if s.is_nan() {
        return "NaN".into();
    }
    let (base, frac) = match f.split_once('.') {
        Some((b, fr)) => (b, fr.len()),
        None => (f, 0),
    };
    if s.is_infinite() {
        return if s > 0.0 { "Inf".into() } else { "-Inf".into() };
    }
    let unit = |x: f64, one: &str, many: &str| {
        let v = if frac > 0 {
            format!("{:.*}", frac, x)
        } else {
            short(x)
        };
        format!("{} {}", v, if x.abs() == 1.0 { one } else { many })
    };
    match base {
        "y" => unit(s / (365.2425 * DAY), "yr", "yrs"),
        "d" => unit(s / DAY, "day", "days"),
        "h" => unit(s / 3600.0, "hr", "hr"),
        "m" => unit(s / 60.0, "min", "min"),
        "s" => unit(s, "sec", "sec"),
        _ => {
            let neg = s < 0.0;
            let mut a = s.abs();
            let scale = 10f64.powi(frac as i32);
            // Round to the displayed precision first so 59.9999 shows as 1:00.
            a = (a * scale).round() / scale;
            let fields: Vec<&str> = base.split(':').collect();
            let mut out = Vec::new();
            let mut rest = a;
            for (k, fl) in fields.iter().enumerate() {
                let unit = match *fl {
                    "dd" => DAY,
                    "hh" => 3600.0,
                    "mm" => 60.0,
                    _ => 1.0,
                };
                let last = k + 1 == fields.len();
                if last {
                    if frac > 0 {
                        let w = 3 + frac;
                        out.push(format!("{:0w$.p$}", rest / unit, w = w, p = frac));
                    } else {
                        out.push(format!("{:02}", (rest / unit).floor() as i64));
                    }
                } else {
                    let q = (rest / unit).floor();
                    rest -= q * unit;
                    out.push(format!("{:02}", q as i64));
                }
            }
            format!("{}{}", if neg { "-" } else { "" }, out.join(":"))
        }
    }
}

fn dur_texts(o: &Object) -> Vec<String> {
    let f = sval(o, "Format");
    data(o, "#v").iter().map(|&s| fmt_dur(s, &f)).collect()
}

fn dur_binary(op: BinOp, a: &Value, b: &Value) -> R<Value> {
    let (da, db) = (class_is(a, "duration"), class_is(b, "duration"));
    let fmt = sval(obj_of(if da { a } else { b }).unwrap(), "Format");
    let side = |v: &Value| -> R<(Vec<usize>, Vec<f64>)> {
        as_secs(v).ok_or_else(|| super::undefined_op(op, a, b))
    };
    match op {
        BinOp::Add | BinOp::Sub => {
            let (ad, av) = side(a)?;
            let (bd, bv) = side(b)?;
            let (dims, pairs) = bcast(&ad, &bd)?;
            let v = pairs
                .iter()
                .map(|&(i, j)| {
                    if op == BinOp::Add {
                        av[i] + bv[j]
                    } else {
                        av[i] - bv[j]
                    }
                })
                .collect();
            Ok(wrap(dur_obj(dims, v, &fmt)))
        }
        _ if cmp_op(op) => {
            if !(da && db) && !(a.is_text() || b.is_text()) {
                return Err(err(
                    "MATLAB:duration:InvalidComparison",
                    "Comparison is only defined between durations (or duration text).",
                ));
            }
            let like = obj_of(if da { a } else { b }).unwrap();
            let x = dur_convert(like, a)?;
            let y = dur_convert(like, b)?;
            compare(
                op,
                x.dims.clone(),
                data(&x, "#v"),
                y.dims.clone(),
                data(&y, "#v"),
            )
        }
        BinOp::Mul | BinOp::EMul => {
            if da && db {
                return Err(err(
                    "MATLAB:duration:DurationMultiplication",
                    "Multiplying two durations is not defined.",
                ));
            }
            let (dv, other) = if da { (a, b) } else { (b, a) };
            let o = obj_of(dv).unwrap();
            let (nd, nv) = num_data(other).ok_or_else(|| super::undefined_op(op, a, b))?;
            if op == BinOp::Mul && o.dims.iter().product::<usize>() != 1 && nv.len() != 1 {
                return Err(err(
                    "MATLAB:duration:MatrixMultiplication",
                    "Matrix multiplication of durations is not supported; use .*",
                ));
            }
            let x = data(o, "#v");
            let (dims, pairs) = bcast(&o.dims, &nd)?;
            Ok(wrap(dur_obj(
                dims,
                pairs.iter().map(|&(i, j)| x[i] * nv[j]).collect(),
                &fmt,
            )))
        }
        BinOp::Div | BinOp::EDiv => {
            if !da {
                return Err(err(
                    "MATLAB:duration:DurationDivision",
                    "Dividing a number by a duration is not defined.",
                ));
            }
            let o = obj_of(a).unwrap();
            let x = data(o, "#v");
            if db {
                let y = obj_of(b).unwrap();
                let yv = data(y, "#v");
                let (dims, pairs) = bcast(&o.dims, &y.dims)?;
                return Ok(num_val(
                    dims,
                    pairs.iter().map(|&(i, j)| x[i] / yv[j]).collect(),
                ));
            }
            let (nd, nv) = num_data(b).ok_or_else(|| super::undefined_op(op, a, b))?;
            let (dims, pairs) = bcast(&o.dims, &nd)?;
            Ok(wrap(dur_obj(
                dims,
                pairs.iter().map(|&(i, j)| x[i] / nv[j]).collect(),
                &fmt,
            )))
        }
        _ => Err(super::undefined_op(op, a, b)),
    }
}

// ---------------------------------------------------------------------------
// calendarDuration
// ---------------------------------------------------------------------------

fn cd_get(o: &Object, name: &str) -> R<Value> {
    match name {
        "Format" => Ok(o
            .get("Format")
            .cloned()
            .unwrap_or_else(|| Value::str("ymdt"))),
        _ => Err(err(
            "MATLAB:calendarDuration:UnrecognizedProperty",
            format!(
                "Unrecognized method, property, or field '{}' for class 'calendarDuration'.",
                name
            ),
        )),
    }
}

fn cd_set(o: &mut Object, name: &str, v: Value) -> R<()> {
    match name {
        "Format" => {
            let f = v.to_str().unwrap_or_default();
            let ok = !f.is_empty()
                && f.chars().all(|c| "yqmwdt".contains(c))
                && f.contains('d')
                && f.contains('m');
            if !ok {
                return Err(err(
                    "MATLAB:calendarDuration:UnrecognizedFormat",
                    format!(
                        "'{}' is not a valid calendarDuration format (letters from 'yqmwdt' including 'm' and 'd').",
                        f
                    ),
                ));
            }
            o.set("Format", Value::str(&f));
            Ok(())
        }
        _ => Err(err(
            "MATLAB:calendarDuration:UnrecognizedProperty",
            format!(
                "Unrecognized property '{}' for class 'calendarDuration'.",
                name
            ),
        )),
    }
}

fn cd_convert(like: &Object, v: &Value) -> R<Object> {
    if let Some(o) = obj_of(v) {
        match o.class.as_str() {
            "calendarDuration" => return Ok(o.clone()),
            "duration" => {
                let n = data(o, "#v").len();
                return Ok(cd_obj(
                    o.dims.clone(),
                    vec![0.0; n],
                    vec![0.0; n],
                    data(o, "#v"),
                    &sval(like, "Format"),
                ));
            }
            _ => {}
        }
    }
    if let Some((d, x)) = num_data(v) {
        let n = x.len();
        return Ok(cd_obj(
            d,
            vec![0.0; n],
            x,
            vec![0.0; n],
            &sval(like, "Format"),
        ));
    }
    Err(err(
        "MATLAB:calendarDuration:InvalidConversion",
        format!(
            "Cannot convert a value of class {} to calendarDuration.",
            v.class_name()
        ),
    ))
}

fn cd_binary(op: BinOp, a: &Value, b: &Value) -> R<Value> {
    let like = obj_of(if class_is(a, "calendarDuration") {
        a
    } else {
        b
    })
    .unwrap()
    .clone();
    let fmt = sval(&like, "Format");
    match op {
        BinOp::Add | BinOp::Sub => {
            let x = cd_convert(&like, a)?;
            let y = cd_convert(&like, b)?;
            let s = if op == BinOp::Sub { -1.0 } else { 1.0 };
            let (dims, pairs) = bcast(&x.dims, &y.dims)?;
            let f = |k: &str| -> Vec<f64> {
                let (p, q) = (data(&x, k), data(&y, k));
                pairs.iter().map(|&(i, j)| p[i] + s * q[j]).collect()
            };
            Ok(wrap(cd_obj(dims, f("#m"), f("#d"), f("#t"), &fmt)))
        }
        BinOp::Mul | BinOp::EMul => {
            let (c, other) = if class_is(a, "calendarDuration") {
                (a, b)
            } else {
                (b, a)
            };
            let o = obj_of(c).unwrap();
            let (nd, nv) = num_data(other).ok_or_else(|| super::undefined_op(op, a, b))?;
            if nv.iter().any(|x| x.fract() != 0.0) {
                return Err(err(
                    "MATLAB:calendarDuration:NonIntegerMultiplier",
                    "calendarDuration values can only be multiplied by integers.",
                ));
            }
            let (dims, pairs) = bcast(&o.dims, &nd)?;
            let f = |k: &str| -> Vec<f64> {
                let p = data(o, k);
                pairs.iter().map(|&(i, j)| p[i] * nv[j]).collect()
            };
            Ok(wrap(cd_obj(dims, f("#m"), f("#d"), f("#t"), &fmt)))
        }
        BinOp::Eq | BinOp::Ne => {
            let x = cd_convert(&like, a)?;
            let y = cd_convert(&like, b)?;
            let (dims, pairs) = bcast(&x.dims, &y.dims)?;
            let (xm, xd, xt) = (data(&x, "#m"), data(&x, "#d"), data(&x, "#t"));
            let (ym, yd, yt) = (data(&y, "#m"), data(&y, "#d"), data(&y, "#t"));
            let eq: Vec<bool> = pairs
                .iter()
                .map(|&(i, j)| {
                    let e = xm[i] == ym[j] && xd[i] == yd[j] && xt[i] == yt[j];
                    if op == BinOp::Eq { e } else { !e }
                })
                .collect();
            Ok(bools(dims, eq))
        }
        _ => Err(super::undefined_op(op, a, b)),
    }
}

fn fmt_cd(m: f64, d: f64, t: f64, fmt: &str) -> String {
    if m.is_nan() || d.is_nan() || t.is_nan() {
        return "NaN".into();
    }
    let mut parts = Vec::new();
    let mut mm = m;
    if fmt.contains('y') {
        let y = (mm / 12.0).trunc();
        mm -= y * 12.0;
        if y != 0.0 {
            parts.push(format!("{}y", short(y)));
        }
    }
    if fmt.contains('q') {
        let q = (mm / 3.0).trunc();
        mm -= q * 3.0;
        if q != 0.0 {
            parts.push(format!("{}q", short(q)));
        }
    }
    if mm != 0.0 {
        parts.push(format!("{}mo", short(mm)));
    }
    let mut dd = d;
    if fmt.contains('w') {
        let w = (dd / 7.0).trunc();
        dd -= w * 7.0;
        if w != 0.0 {
            parts.push(format!("{}w", short(w)));
        }
    }
    if dd != 0.0 {
        parts.push(format!("{}d", short(dd)));
    }
    if t != 0.0 {
        if fmt.contains('t') {
            let neg = t < 0.0;
            let a = t.abs();
            let h = (a / 3600.0).floor();
            let mi = ((a - h * 3600.0) / 60.0).floor();
            let s = a - h * 3600.0 - mi * 60.0;
            let sg = if neg { "-" } else { "" };
            if h != 0.0 {
                parts.push(format!("{}{}h", sg, short(h)));
            }
            if mi != 0.0 {
                parts.push(format!("{}{}m", sg, short(mi)));
            }
            if s != 0.0 {
                parts.push(format!("{}{}s", sg, short(s)));
            }
        } else {
            parts.push(format!("{}d", short(t / DAY)));
        }
    }
    if parts.is_empty() {
        return "0d".into();
    }
    parts.join(" ")
}

fn cd_texts(o: &Object) -> Vec<String> {
    let f = sval(o, "Format");
    let (m, d, t) = (data(o, "#m"), data(o, "#d"), data(o, "#t"));
    (0..m.len()).map(|i| fmt_cd(m[i], d[i], t[i], &f)).collect()
}

// ---------------------------------------------------------------------------
// Constructors
// ---------------------------------------------------------------------------

/// Broadcast numeric component arguments into rows of six components.
fn components(args: &[Value], fname: &str) -> R<(Vec<usize>, Vec<[f64; 7]>)> {
    let mut dims = vec![1, 1];
    let mut n = 1;
    let mut cols = Vec::new();
    for v in args {
        let (d, x) = num_data(v).ok_or_else(|| {
            err(
                "MATLAB:datetime:InvalidComponents",
                format!("{}: date and time components must be numeric.", fname),
            )
        })?;
        if x.len() != 1 {
            if n != 1 && x.len() != n {
                return Err(err(
                    "MATLAB:datetime:InputSizeMismatch",
                    format!(
                        "{}: numeric inputs must be the same size, or any of them can be a scalar.",
                        fname
                    ),
                ));
            }
            n = x.len();
            dims = d;
        }
        cols.push(x);
    }
    let rows = (0..n)
        .map(|i| {
            let mut r = [0.0; 7];
            for (k, c) in cols.iter().enumerate().take(7) {
                r[k] = if c.len() == 1 { c[0] } else { c[i] };
            }
            r
        })
        .collect();
    Ok((dims, rows))
}

const DT_OPTS: &[&str] = &[
    "InputFormat",
    "Format",
    "TimeZone",
    "Locale",
    "PivotYear",
    "ConvertFrom",
    "Epoch",
    "TicksPerSecond",
];

fn split_opts(a: &[Value]) -> (usize, Vec<(String, Value)>) {
    let mut start = a.len();
    for i in 0..a.len() {
        if let Some(s) = a[i].to_str()
            && a[i].is_text()
            && DT_OPTS.iter().any(|k| k.eq_ignore_ascii_case(&s))
            && (a.len() - i).is_multiple_of(2)
        {
            start = i;
            break;
        }
    }
    let opts = a[start..]
        .chunks(2)
        .map(|p| {
            (
                p[0].to_str().unwrap_or_default().to_ascii_lowercase(),
                p[1].clone(),
            )
        })
        .collect();
    (start, opts)
}

fn opt<'a>(o: &'a [(String, Value)], k: &str) -> Option<&'a Value> {
    o.iter().rev().find(|(n, _)| n == k).map(|(_, v)| v)
}

fn datetime_fn(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let (start, opts) = split_opts(&a);
    let data_args = &a[..start];
    let fmt_opt = opt(&opts, "format").and_then(|v| v.to_str());
    let mut zone = opt(&opts, "timezone")
        .and_then(|v| v.to_str())
        .unwrap_or_default();
    if zone == "local" {
        zone = local_zone_name();
    }
    if !zone.is_empty() {
        zone_offset(&zone, 0.0)?;
    }
    let pivot = match opt(&opts, "pivotyear").and_then(|v| v.scalar_f64()) {
        Some(p) => p as i64,
        None => default_pivot(),
    };
    let finish = |dims: Vec<usize>, v: Vec<f64>, default: &str| -> R<Vec<Value>> {
        let f = match &fmt_opt {
            Some(f) if f == "default" => DEFAULT_FMT.to_string(),
            Some(f) if f == "defaultdate" => DATE_FMT.to_string(),
            Some(f) => f.clone(),
            None => default.to_string(),
        };
        Ok(vec![wrap(dt_obj(dims, v, &f, &zone))])
    };
    if data_args.is_empty() {
        let v = if zone.is_empty() {
            now_local().0
        } else {
            let u = now_utc();
            u + zone_offset(&zone, u)?
        };
        return finish(vec![1, 1], vec![v], DEFAULT_FMT);
    }
    if let Some(conv) = opt(&opts, "convertfrom") {
        let kind = conv.to_str().unwrap_or_default().to_ascii_lowercase();
        let (d, x) = num_data(&data_args[0]).ok_or_else(|| {
            err(
                "MATLAB:datetime:InvalidConvertFrom",
                "datetime: 'ConvertFrom' needs numeric input.",
            )
        })?;
        let epoch = match opt(&opts, "epoch") {
            Some(e) if obj_of(e).is_some() => data(obj_of(e).unwrap(), "#v")[0],
            Some(e) if e.is_text() => text_to_dt(&[e.to_str().unwrap_or_default()], None)?[0],
            _ => 0.0,
        };
        let tps = opt(&opts, "tickspersecond")
            .and_then(|v| v.scalar_f64())
            .unwrap_or(1.0);
        let v: Vec<f64> = x
            .iter()
            .map(|&e| match kind.as_str() {
                "datenum" => Ok((e - EPOCH_DATENUM) * DAY),
                "posixtime" => Ok(e),
                "excel" => Ok((e - 25_569.0) * DAY),
                "excel1904" => Ok((e - 24_107.0) * DAY),
                "juliandate" => Ok((e - 2_440_587.5) * DAY),
                "modifiedjuliandate" => Ok((e - 40_587.0) * DAY),
                "yyyymmdd" => Ok(secs_of(
                    (e / 10_000.0).floor(),
                    ((e / 100.0).floor()).rem_euclid(100.0),
                    e.rem_euclid(100.0),
                    0.0,
                    0.0,
                    0.0,
                )),
                "epochtime" => Ok(epoch + e / tps),
                _ => Err(err(
                    "MATLAB:datetime:InvalidConvertFrom",
                    format!("datetime: unsupported 'ConvertFrom' value '{}'.", kind),
                )),
            })
            .collect::<R<_>>()?;
        let v = if !zone.is_empty() && kind == "posixtime" {
            v.into_iter()
                .map(|u| Ok(u + zone_offset(&zone, u)?))
                .collect::<R<Vec<_>>>()?
        } else {
            v
        };
        let date_only = kind == "yyyymmdd";
        return finish(d, v, if date_only { DATE_FMT } else { DEFAULT_FMT });
    }
    let first = &data_args[0];
    if first.is_text() || matches!(first, Value::Cell(_)) {
        let items = text_list(first).ok_or_else(|| {
            err(
                "MATLAB:datetime:InvalidText",
                "datetime: text must be character vectors or strings.",
            )
        })?;
        let dims = match first {
            Value::Char(c) if c.dims[0] > 1 => vec![items.len(), 1],
            Value::Char(_) => vec![1, 1],
            other => other.dims(),
        };
        if let Some(inf) = opt(&opts, "inputformat").and_then(|v| v.to_str()) {
            let v = items
                .iter()
                .map(|t| {
                    if t.is_empty() || t == "NaT" {
                        return Ok(f64::NAN);
                    }
                    parse_dt(t, &inf, pivot).ok_or_else(|| {
                        err(
                            "MATLAB:datetime:ParseErr",
                            format!(
                                "Unable to convert '{}' to datetime using the format '{}'.",
                                t, inf
                            ),
                        )
                    })
                })
                .collect::<R<Vec<_>>>()?;
            return finish(dims, v, &inf);
        }
        let mut has_time = false;
        let mut v = Vec::with_capacity(items.len());
        for t in &items {
            if t.is_empty() || t.eq_ignore_ascii_case("NaT") {
                v.push(f64::NAN);
                continue;
            }
            match t.to_ascii_lowercase().as_str() {
                "now" => {
                    has_time = true;
                    let x = if zone.is_empty() {
                        now_local().0
                    } else {
                        let u = now_utc();
                        u + zone_offset(&zone, u)?
                    };
                    v.push(x);
                    continue;
                }
                "today" | "yesterday" | "tomorrow" => {
                    v.push(keyword_dt(t).unwrap());
                    continue;
                }
                _ => {}
            }
            let (x, ht, _) = guess_dt(t).ok_or_else(|| {
                err(
                    "MATLAB:datetime:ParseErr",
                    format!(
                        "Could not recognize the date/time format of '{}'. Specify a format using the 'InputFormat' parameter.",
                        t
                    ),
                )
            })?;
            has_time |= ht;
            v.push(x);
        }
        return finish(dims, v, if has_time { DEFAULT_FMT } else { DATE_FMT });
    }
    if let Some(o) = obj_of(first)
        && o.class == "datetime"
    {
        let mut o = o.clone();
        if let Some(f) = fmt_opt {
            o.set("Format", Value::str(&f));
        }
        return Ok(vec![wrap(o)]);
    }
    if data_args.len() == 1 {
        // datetime(DateVectors): N×3 or N×6.
        let (d, x) = num_data(first).ok_or_else(|| {
            err(
                "MATLAB:datetime:InvalidInput",
                "datetime: expected text, date vectors or components.",
            )
        })?;
        let (r, c) = (d[0], d.get(1).copied().unwrap_or(1));
        if d.len() != 2 || !(c == 3 || c == 6) {
            return Err(err(
                "MATLAB:datetime:InvalidDateVectors",
                "datetime: a numeric input must be an N×3 or N×6 matrix of date vectors (use 'ConvertFrom' for serial numbers).",
            ));
        }
        let at = |i: usize, j: usize| if j < c { x[i + j * r] } else { 0.0 };
        let v = (0..r)
            .map(|i| secs_of(at(i, 0), at(i, 1), at(i, 2), at(i, 3), at(i, 4), at(i, 5)))
            .collect();
        return finish(vec![r, 1], v, if c == 3 { DATE_FMT } else { DEFAULT_FMT });
    }
    if data_args.len() == 2 || data_args.len() > 7 {
        return Err(err(
            "MATLAB:datetime:InvalidInput",
            "datetime: use datetime(Y,M,D), datetime(Y,M,D,H,MI,S) or datetime(Y,M,D,H,MI,S,MS).",
        ));
    }
    let (dims, rows) = components(data_args, "datetime")?;
    let v = rows
        .iter()
        .map(|r| secs_of(r[0], r[1], r[2], r[3], r[4], r[5] + r[6] / 1000.0))
        .collect();
    let fmt = if data_args.len() == 3 {
        DATE_FMT
    } else if data_args.len() == 7 {
        "dd-MMM-uuuu HH:mm:ss.SSS"
    } else {
        DEFAULT_FMT
    };
    finish(dims, v, fmt)
}

fn nat_fn(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let (dims, _) = crate::builtins::dims_and_class(&a, "NaT")?;
    Ok(vec![nat_array(dims)])
}

fn duration_fn(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let mut a = a;
    let mut fmt = "hh:mm:ss".to_string();
    if let Some(p) = a
        .iter()
        .position(|v| v.to_str().is_some_and(|s| s.eq_ignore_ascii_case("Format")))
    {
        fmt = a.get(p + 1).and_then(|v| v.to_str()).unwrap_or(fmt);
        a.truncate(p);
    }
    if !valid_dur_fmt(&fmt) {
        return Err(err(
            "MATLAB:duration:UnrecognizedFormat",
            format!("'{}' is not a valid duration format.", fmt),
        ));
    }
    if a.is_empty() {
        return Ok(vec![wrap(dur_obj(vec![1, 1], vec![0.0], &fmt))]);
    }
    if a[0].is_text() || matches!(a[0], Value::Cell(_)) {
        let like = dur_obj(vec![1, 1], vec![0.0], &fmt);
        let mut o = dur_convert(&like, &a[0])?;
        o.dims = if matches!(a[0], Value::Char(_)) {
            vec![1, 1]
        } else {
            a[0].dims()
        };
        return Ok(vec![wrap(o)]);
    }
    if a.len() == 1 {
        let (d, x) = num_data(&a[0]).ok_or_else(|| {
            err(
                "MATLAB:duration:InvalidInput",
                "duration: numeric input expected.",
            )
        })?;
        let (r, c) = (d[0], d.get(1).copied().unwrap_or(1));
        if c != 3 && c != 4 {
            return Err(err(
                "MATLAB:duration:InvalidInput",
                "duration: a single numeric input must be an N×3 matrix [h m s] (use hours/minutes/seconds for numbers).",
            ));
        }
        let v = (0..r)
            .map(|i| {
                x[i] * 3600.0
                    + x[i + r] * 60.0
                    + x[i + 2 * r]
                    + if c == 4 { x[i + 3 * r] / 1000.0 } else { 0.0 }
            })
            .collect();
        return Ok(vec![wrap(dur_obj(vec![r, 1], v, &fmt))]);
    }
    if a.len() < 3 {
        return Err(err(
            "MATLAB:duration:InvalidInput",
            "duration: use duration(H,MI,S) or duration(H,MI,S,MS).",
        ));
    }
    let (dims, rows) = components(&a, "duration")?;
    let v = rows
        .iter()
        .map(|r| r[0] * 3600.0 + r[1] * 60.0 + r[2] + r[3] / 1000.0)
        .collect();
    Ok(vec![wrap(dur_obj(dims, v, &fmt))])
}

/// years/days/hours/minutes/seconds/milliseconds: number → duration,
/// duration → number of units.
fn unit_fn(a: &[Value], fname: &str, unit: f64, fmt: &str) -> R<Vec<Value>> {
    let v = a.first().cloned().unwrap_or_else(|| Value::num(1.0));
    if let Some(o) = obj_of(&v)
        && o.class == "duration"
    {
        let x: Vec<f64> = data(o, "#v").iter().map(|s| s / unit).collect();
        return Ok(vec![num_val(o.dims.clone(), x)]);
    }
    let (d, x) = num_data(&v).ok_or_else(|| {
        err(
            "MATLAB:duration:InvalidInput",
            format!("{}: input must be numeric or a duration.", fname),
        )
    })?;
    Ok(vec![wrap(dur_obj(
        d,
        x.iter().map(|e| e * unit).collect(),
        fmt,
    ))])
}

macro_rules! unit_fns {
    ($($f:ident => $name:literal, $unit:expr, $fmt:literal);* $(;)?) => {
        $(fn $f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
            unit_fn(&a, $name, $unit, $fmt)
        })*
    };
}

unit_fns!(
    years_f => "years", 365.2425 * DAY, "y";
    days_f => "days", DAY, "d";
    hours_f => "hours", 3600.0, "h";
    minutes_f => "minutes", 60.0, "m";
    seconds_f => "seconds", 1.0, "s";
    ms_f => "milliseconds", 0.001, "s";
);

fn caldur_fn(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let mut a = a;
    let mut fmt = "ymdt".to_string();
    if let Some(p) = a
        .iter()
        .position(|v| v.to_str().is_some_and(|s| s.eq_ignore_ascii_case("Format")))
    {
        fmt = a.get(p + 1).and_then(|v| v.to_str()).unwrap_or(fmt);
        a.truncate(p);
    }
    let rows: (Vec<usize>, Vec<[f64; 7]>) = match a.len() {
        0 => (vec![1, 1], vec![[0.0; 7]]),
        1 => {
            let (d, x) = num_data(&a[0]).ok_or_else(|| {
                err(
                    "MATLAB:calendarDuration:InvalidInput",
                    "calendarDuration: numeric input expected.",
                )
            })?;
            let (r, c) = (d[0], d.get(1).copied().unwrap_or(1));
            if c != 3 && c != 6 {
                return Err(err(
                    "MATLAB:calendarDuration:InvalidInput",
                    "calendarDuration: a single input must be an N×3 or N×6 matrix.",
                ));
            }
            (
                vec![r, 1],
                (0..r)
                    .map(|i| {
                        let mut row = [0.0; 7];
                        for (j, slot) in row.iter_mut().enumerate().take(c) {
                            *slot = x[i + j * r];
                        }
                        row
                    })
                    .collect(),
            )
        }
        3 | 6 => components(&a, "calendarDuration")?,
        4 => {
            // (Y, M, D, T) with T a duration.
            let t = obj_of(&a[3])
                .filter(|o| o.class == "duration")
                .ok_or_else(|| {
                    err(
                        "MATLAB:calendarDuration:InvalidInput",
                        "calendarDuration(Y,M,D,T): T must be a duration.",
                    )
                })?;
            let tv = data(t, "#v");
            let tnum = num_val(t.dims.clone(), tv);
            let mut args = a[..3].to_vec();
            args.extend([Value::num(0.0), Value::num(0.0), tnum]);
            components(&args, "calendarDuration")?
        }
        _ => {
            return Err(err(
                "MATLAB:calendarDuration:InvalidInput",
                "calendarDuration: use calendarDuration(Y,M,D), (Y,M,D,H,MI,S) or (Y,M,D,T).",
            ));
        }
    };
    let (dims, rows) = rows;
    if rows
        .iter()
        .any(|r| r[0].fract() != 0.0 || r[1].fract() != 0.0 || r[2].fract() != 0.0)
    {
        return Err(err(
            "MATLAB:calendarDuration:NonIntegerCalendarComponents",
            "calendarDuration: years, months and days must be integers.",
        ));
    }
    let m = rows.iter().map(|r| r[0] * 12.0 + r[1]).collect();
    let d = rows.iter().map(|r| r[2]).collect();
    let t = rows
        .iter()
        .map(|r| r[3] * 3600.0 + r[4] * 60.0 + r[5])
        .collect();
    Ok(vec![wrap(cd_obj(dims, m, d, t, &fmt))])
}

fn cal_unit(a: &[Value], fname: &str, months: f64, days: f64) -> R<Vec<Value>> {
    let v = a.first().cloned().unwrap_or_else(|| Value::num(1.0));
    if let Some(o) = obj_of(&v)
        && o.class == "calendarDuration"
    {
        let src = if months > 0.0 {
            data(o, "#m")
        } else {
            data(o, "#d")
        };
        let unit = if months > 0.0 { months } else { days };
        let x: Vec<f64> = src.iter().map(|s| (s / unit).trunc()).collect();
        return Ok(vec![num_val(o.dims.clone(), x)]);
    }
    let (d, x) = num_data(&v).ok_or_else(|| {
        err(
            "MATLAB:calendarDuration:InvalidInput",
            format!("{}: input must be numeric.", fname),
        )
    })?;
    if x.iter().any(|e| e.fract() != 0.0) {
        return Err(err(
            "MATLAB:calendarDuration:NonIntegerCalendarComponents",
            format!("{}: input must be integer valued.", fname),
        ));
    }
    let n = x.len();
    let m: Vec<f64> = x.iter().map(|e| e * months).collect();
    let dd: Vec<f64> = x.iter().map(|e| e * days).collect();
    let fmt = match fname {
        "calquarters" => "qmdt",
        "calweeks" => "ymwdt",
        _ => "ymdt",
    };
    Ok(vec![wrap(cd_obj(d, m, dd, vec![0.0; n], fmt))])
}

macro_rules! cal_fns {
    ($($f:ident => $name:literal, $m:expr, $d:expr);* $(;)?) => {
        $(fn $f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
            cal_unit(&a, $name, $m, $d)
        })*
    };
}

cal_fns!(
    calyears_f => "calyears", 12.0, 0.0;
    calquarters_f => "calquarters", 3.0, 0.0;
    calmonths_f => "calmonths", 1.0, 0.0;
    calweeks_f => "calweeks", 0.0, 7.0;
    caldays_f => "caldays", 0.0, 1.0;
);

// ---------------------------------------------------------------------------
// Functions on datetime
// ---------------------------------------------------------------------------

fn arg_dt<'a>(v: &'a Value, fname: &str) -> R<&'a Object> {
    match obj_of(v) {
        Some(o) if o.class == "datetime" => Ok(o),
        _ => Err(err(
            "MATLAB:datetime:InvalidInput",
            format!(
                "{}: input must be a datetime array, got {}.",
                fname,
                v.class_name()
            ),
        )),
    }
}

fn map_dt(o: &Object, f: impl Fn(f64) -> f64) -> Value {
    num_val(o.dims.clone(), data(o, "#v").into_iter().map(f).collect())
}

fn names_cell(o: &Object, list: &[&str], idx: impl Fn(f64) -> f64, short: bool) -> Value {
    let items: Vec<Value> = data(o, "#v")
        .iter()
        .map(|&s| {
            let k = idx(s);
            if k.is_nan() {
                return Value::str("");
            }
            let n = list[(k as usize).saturating_sub(1).min(list.len() - 1)];
            Value::str(if short { &n[..3] } else { n })
        })
        .collect();
    Value::Cell(Array::new(o.dims.clone(), items))
}

fn year_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "year")?;
    let o = arg_dt(&a[0], "year")?;
    Ok(vec![map_dt(o, |s| parts(s).0)])
}

fn month_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "month")?;
    let o = arg_dt(&a[0], "month")?;
    match a.get(1).and_then(|v| v.to_str()).as_deref() {
        Some("name") => Ok(vec![names_cell(o, &MONTHS, |s| parts(s).1, false)]),
        Some("shortname") => Ok(vec![names_cell(o, &MONTHS, |s| parts(s).1, true)]),
        _ => Ok(vec![map_dt(o, |s| parts(s).1)]),
    }
}

fn day_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "day")?;
    let o = arg_dt(&a[0], "day")?;
    match a.get(1).and_then(|v| v.to_str()).as_deref() {
        Some("dayofweek") => Ok(vec![map_dt(o, weekday_of)]),
        Some("dayofyear") => Ok(vec![map_dt(o, day_of_year)]),
        Some("name") => Ok(vec![names_cell(o, &WEEKDAYS, weekday_of, false)]),
        Some("shortname") => Ok(vec![names_cell(o, &WEEKDAYS, weekday_of, true)]),
        _ => Ok(vec![map_dt(o, |s| parts(s).2)]),
    }
}

fn hour_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "hour")?;
    Ok(vec![map_dt(arg_dt(&a[0], "hour")?, |s| parts(s).3)])
}

fn minute_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "minute")?;
    Ok(vec![map_dt(arg_dt(&a[0], "minute")?, |s| parts(s).4)])
}

fn second_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "second")?;
    Ok(vec![map_dt(arg_dt(&a[0], "second")?, |s| parts(s).5)])
}

fn quarter_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "quarter")?;
    Ok(vec![map_dt(arg_dt(&a[0], "quarter")?, |s| {
        ((parts(s).1 - 1.0) / 3.0).floor() + 1.0
    })])
}

fn week_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "week")?;
    let o = arg_dt(&a[0], "week")?;
    // Week of year: weeks start on Sunday, week 1 contains January 1.
    Ok(vec![map_dt(o, |s| {
        if s.is_nan() {
            return f64::NAN;
        }
        let (y, ..) = parts(s);
        let jan1 = days_from_civil(y as i64, 1, 1) as f64 * DAY;
        let off = weekday_of(jan1) - 1.0;
        ((day_of_year(s) - 1.0 + off) / 7.0).floor() + 1.0
    })])
}

fn ymd_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "ymd")?;
    let o = arg_dt(&a[0], "ymd")?;
    Ok(vec![
        map_dt(o, |s| parts(s).0),
        map_dt(o, |s| parts(s).1),
        map_dt(o, |s| parts(s).2),
    ])
}

fn hms_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "hms")?;
    if let Some(o) = obj_of(&a[0])
        && o.class == "duration"
    {
        let v = data(o, "#v");
        let h: Vec<f64> = v.iter().map(|s| (s / 3600.0).trunc()).collect();
        let m: Vec<f64> = v
            .iter()
            .zip(&h)
            .map(|(s, h)| ((s - h * 3600.0) / 60.0).trunc())
            .collect();
        let sec: Vec<f64> = v
            .iter()
            .zip(&h)
            .zip(&m)
            .map(|((s, h), m)| s - h * 3600.0 - m * 60.0)
            .collect();
        let d = o.dims.clone();
        return Ok(vec![
            num_val(d.clone(), h),
            num_val(d.clone(), m),
            num_val(d, sec),
        ]);
    }
    let o = arg_dt(&a[0], "hms")?;
    Ok(vec![
        map_dt(o, |s| parts(s).3),
        map_dt(o, |s| parts(s).4),
        map_dt(o, |s| parts(s).5),
    ])
}

fn timeofday_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "timeofday")?;
    let o = arg_dt(&a[0], "timeofday")?;
    let v = data(o, "#v")
        .iter()
        .map(|s| s - (s / DAY).floor() * DAY)
        .collect();
    Ok(vec![wrap(dur_obj(o.dims.clone(), v, "hh:mm:ss"))])
}

fn isweekend_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "isweekend")?;
    let o = arg_dt(&a[0], "isweekend")?;
    let b = data(o, "#v")
        .iter()
        .map(|&s| matches!(weekday_of(s) as i64, 1 | 7))
        .collect();
    Ok(vec![bools(o.dims.clone(), b)])
}

fn isnat_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "isnat")?;
    let o = arg_dt(&a[0], "isnat")?;
    Ok(vec![bools(
        o.dims.clone(),
        data(o, "#v").iter().map(|x| x.is_nan()).collect(),
    )])
}

fn posixtime_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "posixtime")?;
    let o = arg_dt(&a[0], "posixtime")?;
    let z = sval(o, "TimeZone");
    let v = data(o, "#v")
        .into_iter()
        .map(|s| Ok(s - zone_offset(&z, s)?))
        .collect::<R<Vec<_>>>()?;
    Ok(vec![num_val(o.dims.clone(), v)])
}

fn juliandate_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "juliandate")?;
    let o = arg_dt(&a[0], "juliandate")?;
    Ok(vec![map_dt(o, |s| s / DAY + 2_440_587.5)])
}

fn exceltime_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "exceltime")?;
    let o = arg_dt(&a[0], "exceltime")?;
    Ok(vec![map_dt(o, |s| s / DAY + 25_569.0)])
}

fn yyyymmdd_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "yyyymmdd")?;
    let o = arg_dt(&a[0], "yyyymmdd")?;
    Ok(vec![map_dt(o, |s| {
        let p = parts(s);
        p.0 * 10_000.0 + p.1 * 100.0 + p.2
    })])
}

fn dateshift_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 3, "dateshift")?;
    let o = arg_dt(&a[0], "dateshift")?;
    let kind = a[1].to_str().unwrap_or_default().to_ascii_lowercase();
    let rule = a.get(3).cloned();
    let n_rule: f64 = match &rule {
        None => 0.0,
        Some(v) => match v.to_str().map(|s| s.to_ascii_lowercase()).as_deref() {
            Some("current") => 0.0,
            Some("next") => 1.0,
            Some("previous") => -1.0,
            Some("nearest") => f64::NAN,
            Some(other) => {
                return Err(err(
                    "MATLAB:datetime:dateshift:InvalidRule",
                    format!("dateshift: unknown rule '{}'.", other),
                ));
            }
            None => v.scalar_f64().unwrap_or(0.0),
        },
    };
    if kind == "dayofweek" {
        let target = match a[2].to_str() {
            Some(s) => WEEKDAYS
                .iter()
                .position(|w| w.eq_ignore_ascii_case(&s) || w[..3].eq_ignore_ascii_case(&s))
                .map(|k| k as f64 + 1.0)
                .ok_or_else(|| {
                    err(
                        "MATLAB:datetime:dateshift:InvalidDOW",
                        format!("dateshift: unknown day name '{}'.", s),
                    )
                })?,
            None => a[2].scalar_f64().unwrap_or(1.0),
        };
        let r = if rule.is_none() { 1.0 } else { n_rule };
        let mut out = o.clone();
        let v = data(o, "#v")
            .iter()
            .map(|&s| {
                let start = (s / DAY).floor() * DAY;
                let w = weekday_of(s);
                let fwd = (target - w).rem_euclid(7.0);
                if r >= 1.0 {
                    let first = if fwd == 0.0 { 7.0 } else { fwd };
                    start + (first + 7.0 * (r - 1.0)) * DAY
                } else if r == 0.0 {
                    start + fwd * DAY
                } else {
                    let back = (w - target).rem_euclid(7.0);
                    let first = if back == 0.0 { 7.0 } else { back };
                    start - (first + 7.0 * (-r - 1.0)) * DAY
                }
            })
            .collect();
        set_data(&mut out, "#v", v);
        return Ok(vec![wrap(out)]);
    }
    if kind != "start" && kind != "end" {
        return Err(err(
            "MATLAB:datetime:dateshift:InvalidType",
            "dateshift: the second argument must be 'start', 'end' or 'dayofweek'.",
        ));
    }
    let unit = a[2].to_str().unwrap_or_default().to_ascii_lowercase();
    let unit = unit.trim_end_matches('s').to_string();
    let start_of = |s: f64, k: f64| -> R<f64> {
        let (y, m, d, h, mi, sec) = parts(s);
        Ok(match unit.as_str() {
            "year" => secs_of(y + k, 1.0, 1.0, 0.0, 0.0, 0.0),
            "quarter" => secs_of(
                y,
                ((m - 1.0) / 3.0).floor() * 3.0 + 1.0 + 3.0 * k,
                1.0,
                0.0,
                0.0,
                0.0,
            ),
            "month" => secs_of(y, m + k, 1.0, 0.0, 0.0, 0.0),
            "week" => {
                let day0 = (s / DAY).floor() - (weekday_of(s) - 1.0);
                (day0 + 7.0 * k) * DAY
            }
            "day" => ((s / DAY).floor() + k) * DAY,
            "hour" => secs_of(y, m, d, h + k, 0.0, 0.0),
            "minute" => secs_of(y, m, d, h, mi + k, 0.0),
            "second" => secs_of(y, m, d, h, mi, sec.floor() + k),
            _ => {
                return Err(err(
                    "MATLAB:datetime:dateshift:InvalidUnit",
                    format!("dateshift: unknown unit '{}'.", unit),
                ));
            }
        })
    };
    let sub_unit = match unit.as_str() {
        "year" | "quarter" | "month" | "week" | "day" => DAY,
        "hour" => 60.0,
        "minute" | "second" => 1.0,
        _ => 0.0,
    };
    let mut out = o.clone();
    let mut v = Vec::new();
    for s in data(o, "#v") {
        if !s.is_finite() {
            v.push(s);
            continue;
        }
        let x = if n_rule.is_nan() {
            let a0 = start_of(s, 0.0)?;
            let a1 = start_of(s, 1.0)?;
            if kind == "start" {
                if s - a0 < a1 - s { a0 } else { a1 }
            } else {
                let e0 = a0 - sub_unit;
                let e1 = a1 - sub_unit;
                if (s - e0).abs() < (e1 - s).abs() {
                    e0
                } else {
                    e1
                }
            }
        } else if kind == "start" {
            start_of(s, n_rule)?
        } else {
            start_of(s, n_rule + 1.0)? - sub_unit
        };
        v.push(x);
    }
    set_data(&mut out, "#v", v);
    Ok(vec![wrap(out)])
}

fn between_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "between")?;
    let x = arg_dt(&a[0], "between")?;
    let y = dt_convert(x, &a[1])?;
    let (xv, yv) = (data(x, "#v"), data(&y, "#v"));
    let (dims, pairs) = bcast(&x.dims, &y.dims)?;
    let mut m = Vec::new();
    let mut d = Vec::new();
    let mut t = Vec::new();
    for (i, j) in pairs {
        let (s, e) = (xv[i], yv[j]);
        let (y1, m1, ..) = parts(s);
        let (y2, m2, ..) = parts(e);
        let mut months = (y2 - y1) * 12.0 + (m2 - m1);
        if cal_add(s, months, 0.0, 0.0) > e && months > 0.0 {
            months -= 1.0;
        }
        if cal_add(s, months, 0.0, 0.0) < e && months < 0.0 {
            months += 1.0;
        }
        let base = cal_add(s, months, 0.0, 0.0);
        let days = ((e - base) / DAY).trunc();
        m.push(months);
        d.push(days);
        t.push(e - base - days * DAY);
    }
    Ok(vec![wrap(cd_obj(dims, m, d, t, "ymdt"))])
}

fn isdatetime_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    Ok(vec![Value::bool(
        a.first().is_some_and(|v| class_is(v, "datetime")),
    )])
}

fn isduration_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    Ok(vec![Value::bool(
        a.first().is_some_and(|v| class_is(v, "duration")),
    )])
}

fn iscaldur_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    Ok(vec![Value::bool(
        a.first().is_some_and(|v| class_is(v, "calendarDuration")),
    )])
}

// ---------------------------------------------------------------------------
// Methods shared by datetime and duration: text, sorting, reductions
// ---------------------------------------------------------------------------

pub fn texts_of(o: &Object) -> Vec<String> {
    match o.class.as_str() {
        "datetime" => dt_texts(o),
        "duration" => dur_texts(o),
        _ => cd_texts(o),
    }
}

/// Apply a format argument (`char(d, fmt)`, `string(d, fmt)`).
fn with_format(o: &Object, a: &[Value]) -> Object {
    let mut o = o.clone();
    if let Some(f) = a.get(1).and_then(|v| v.to_str()) {
        o.set("Format", Value::str(&f));
    }
    o
}

fn char_m(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let o = with_format(obj_of(&a[0]).unwrap(), &a);
    let t = texts_of(&o);
    if t.len() == 1 {
        return Ok(vec![Value::str(&t[0])]);
    }
    let w = t.iter().map(|s| s.chars().count()).max().unwrap_or(0);
    let n = t.len();
    let mut data = vec![' '; n * w];
    for (i, s) in t.iter().enumerate() {
        for (j, c) in s.chars().enumerate() {
            data[i + j * n] = c;
        }
    }
    Ok(vec![Value::Char(Array::new(vec![n, w], data))])
}

fn string_m(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let o = with_format(obj_of(&a[0]).unwrap(), &a);
    let nat = |s: &str| s == "NaT" || s == "NaN";
    let t = texts_of(&o);
    Ok(vec![Value::Str(Array::new(
        o.dims.clone(),
        t.into_iter()
            .map(|s| if nat(&s) { None } else { Some(s) })
            .collect(),
    ))])
}

fn cellstr_m(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let o = with_format(obj_of(&a[0]).unwrap(), &a);
    let t = texts_of(&o);
    Ok(vec![Value::Cell(Array::new(
        o.dims.clone(),
        t.iter().map(|s| Value::str(s)).collect(),
    ))])
}

/// Run a numeric builtin on the `#v` data of every datetime/duration
/// argument; outputs listed in `wrap_out` become the class again.
fn on_data(
    it: &mut Interp,
    name: &str,
    a: Vec<Value>,
    nargout: usize,
    wrap_out: &[usize],
    as_dur: bool,
) -> R<Vec<Value>> {
    let Some(p) = a.iter().position(is_native) else {
        return Err(err(
            "MATLAB:invalidType",
            format!("{}: no datetime argument", name),
        ));
    };
    let like = obj_of(&a[p]).unwrap().clone();
    let ops_ = super::ops_of(&like.class).unwrap();
    let mut args = Vec::with_capacity(a.len());
    for v in &a {
        if is_native(v)
            || (v.is_text()
                && like.class == "datetime"
                && !matches!(v, Value::Char(c) if c.data.len() < 3))
        {
            let c = if is_native(v) && obj_of(v).unwrap().class == like.class {
                obj_of(v).unwrap().clone()
            } else if is_native(v) {
                (ops_.convert)(&like, v)?
            } else {
                match (ops_.convert)(&like, v) {
                    Ok(c) => c,
                    Err(_) => {
                        args.push(v.clone());
                        continue;
                    }
                }
            };
            args.push(num_val(c.dims.clone(), data(&c, "#v")));
        } else {
            args.push(v.clone());
        }
    }
    let f = *it
        .builtins
        .get(name)
        .ok_or_else(|| Interp::undefined(name))?;
    let mut out = f(it, args, nargout.max(1))?;
    for k in wrap_out {
        if let Some(Value::Num(n)) = out.get(*k).cloned() {
            let o = if as_dur {
                let f = if like.class == "duration" {
                    sval(&like, "Format")
                } else {
                    "hh:mm:ss".to_string()
                };
                dur_obj(n.dims.clone(), n.re.clone(), &f)
            } else {
                let mut o = like.clone();
                o.dims = n.dims.clone();
                set_data(&mut o, "#v", n.re.clone());
                o
            };
            out[*k] = wrap(o);
        }
    }
    Ok(out)
}

macro_rules! data_fns {
    ($($f:ident => $name:literal, [$($w:expr),*], $dur:expr);* $(;)?) => {
        $(fn $f(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
            on_data(it, $name, a, n, &[$($w),*], $dur)
        })*
    };
}

data_fns!(
    sort_m => "sort", [0], false;
    min_m => "min", [0], false;
    max_m => "max", [0], false;
    unique_m => "unique", [0], false;
    issorted_m => "issorted", [], false;
    ismember_m => "ismember", [], false;
    median_m => "median", [0], false;
    cummin_m => "cummin", [0], false;
    cummax_m => "cummax", [0], false;
    find_m => "find", [], false;
    isnan_m => "isnan", [], false;
    isinf_m => "isinf", [], false;
    isfinite_m => "isfinite", [], false;
    diff_dt => "diff", [0], true;
    mean_dt => "mean", [0], false;
    sum_d => "sum", [0], false;
    mean_d => "mean", [0], false;
    cumsum_d => "cumsum", [0], false;
    abs_d => "abs", [0], false;
    diff_d => "diff", [0], false;
    std_d => "std", [0], false;
    mode_m => "mode", [0], false;
);

fn sum_dt(_: &mut Interp, _: Vec<Value>, _: usize) -> R<Vec<Value>> {
    Err(err(
        "MATLAB:datetime:sum",
        "Summing datetime values is not defined; subtract them to get durations.",
    ))
}

fn round_d(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    unit_round(it, a, n, "round")
}
fn floor_d(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    unit_round(it, a, n, "floor")
}
fn ceil_d(it: &mut Interp, a: Vec<Value>, n: usize) -> R<Vec<Value>> {
    unit_round(it, a, n, "ceil")
}

/// round/floor/ceil of a duration, to whole seconds or to a unit.
fn unit_round(_: &mut Interp, a: Vec<Value>, _: usize, how: &str) -> R<Vec<Value>> {
    let o = obj_of(&a[0]).unwrap();
    let unit = match a.get(1).and_then(|v| v.to_str()).as_deref() {
        Some("years") => 365.2425 * DAY,
        Some("days") => DAY,
        Some("hours") => 3600.0,
        Some("minutes") => 60.0,
        _ => 1.0,
    };
    let f = |x: f64| match how {
        "floor" => (x / unit).floor() * unit,
        "ceil" => (x / unit).ceil() * unit,
        _ => (x / unit).round() * unit,
    };
    Ok(vec![wrap(ew_map(o, |d| {
        let (dd, x) = num_data(&d).unwrap_or_default();
        Ok(num_val(dd, x.into_iter().map(f).collect()))
    })?)])
}

fn split_cd(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "split")?;
    let o = obj_of(&a[0]).unwrap();
    let units = text_list(&a[1]).unwrap_or_default();
    let (m, d, t) = (data(o, "#m"), data(o, "#d"), data(o, "#t"));
    let mut rm = m.clone();
    let mut rd = d.clone();
    let mut out = Vec::new();
    for u in &units {
        let v: Vec<f64> = match u.as_str() {
            "years" => rm
                .iter_mut()
                .map(|x| {
                    let y = (*x / 12.0).trunc();
                    *x -= y * 12.0;
                    y
                })
                .collect(),
            "quarters" => rm
                .iter_mut()
                .map(|x| {
                    let q = (*x / 3.0).trunc();
                    *x -= q * 3.0;
                    q
                })
                .collect(),
            "months" => rm
                .iter_mut()
                .map(|x| {
                    let v = *x;
                    *x = 0.0;
                    v
                })
                .collect(),
            "weeks" => rd
                .iter_mut()
                .map(|x| {
                    let w = (*x / 7.0).trunc();
                    *x -= w * 7.0;
                    w
                })
                .collect(),
            "days" => rd
                .iter_mut()
                .map(|x| {
                    let v = *x;
                    *x = 0.0;
                    v
                })
                .collect(),
            "time" => {
                out.push(wrap(dur_obj(o.dims.clone(), t.clone(), "hh:mm:ss")));
                continue;
            }
            _ => {
                return Err(err(
                    "MATLAB:calendarDuration:split:InvalidUnit",
                    format!(
                        "split: unknown unit '{}' (use years, quarters, months, weeks, days, time).",
                        u
                    ),
                ));
            }
        };
        out.push(num_val(o.dims.clone(), v));
    }
    Ok(out)
}

fn split_dt(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "split")?;
    let o = arg_dt(&a[0], "split")?;
    let units = text_list(&a[1]).unwrap_or_default();
    let mut out = Vec::new();
    for u in units {
        let k = match u.as_str() {
            "years" | "year" => 0,
            "months" | "month" => 1,
            "days" | "day" => 2,
            "hours" | "hour" => 3,
            "minutes" | "minute" => 4,
            "seconds" | "second" => 5,
            _ => {
                return Err(err(
                    "MATLAB:datetime:split:InvalidUnit",
                    format!("split: unknown unit '{}'.", u),
                ));
            }
        };
        out.push(comp_array(o, k));
    }
    Ok(out)
}

fn split_dur(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "split")?;
    let o = obj_of(&a[0]).unwrap();
    let units = text_list(&a[1]).unwrap_or_default();
    let mut rest = data(o, "#v");
    let mut out = Vec::new();
    for u in units {
        let unit = match u.as_str() {
            "years" => 365.2425 * DAY,
            "days" => DAY,
            "hours" => 3600.0,
            "minutes" => 60.0,
            "seconds" => 1.0,
            _ => {
                return Err(err(
                    "MATLAB:duration:split:InvalidUnit",
                    format!("split: unknown unit '{}'.", u),
                ));
            }
        };
        let v: Vec<f64> = rest
            .iter_mut()
            .map(|x| {
                let q = if unit == 1.0 { *x } else { (*x / unit).trunc() };
                *x -= q * unit;
                q
            })
            .collect();
        out.push(num_val(o.dims.clone(), v));
    }
    Ok(out)
}

fn datenum_m(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let o = arg_dt(&a[0], "datenum")?;
    Ok(vec![map_dt(o, |s| s / DAY + EPOCH_DATENUM)])
}

fn datevec_m(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let o = arg_dt(&a[0], "datevec")?;
    let v = data(o, "#v");
    datevec_out(
        v.into_iter().map(|s| s / DAY + EPOCH_DATENUM).collect(),
        nargout,
    )
}

fn datestr_m(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let o = arg_dt(&a[0], "datestr")?;
    let nums: Vec<f64> = data(o, "#v")
        .iter()
        .map(|s| s / DAY + EPOCH_DATENUM)
        .collect();
    let mut args = vec![Value::col(nums)];
    args.extend(a[1..].iter().cloned());
    datestr_f_inner(&args)
}

fn dt_colon(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "colon")?;
    let (step, stop) = if a.len() >= 3 {
        (Some(&a[1]), &a[2])
    } else {
        (None, &a[1])
    };
    Ok(vec![range(&a[0], step, stop)?])
}

/// `start:step:stop` for datetime and duration values. The default step
/// is one day; a calendarDuration step advances by calendar units.
pub fn range(a: &Value, step: Option<&Value>, b: &Value) -> R<Value> {
    const MAX: f64 = 1e8;
    let lead = if class_is(a, "datetime") || class_is(b, "datetime") {
        "datetime"
    } else if class_is(a, "duration") || class_is(b, "duration") {
        "duration"
    } else {
        return Err(err(
            "MATLAB:colon:UnsupportedType",
            format!(
                "Ranges with {} and {} values are not supported.",
                a.class_name(),
                b.class_name()
            ),
        ));
    };
    let like = obj_of(if class_is(a, lead) { a } else { b })
        .unwrap()
        .clone();
    let conv = |v: &Value| -> R<f64> {
        let o = if lead == "datetime" {
            dt_convert(&like, v)?
        } else {
            dur_convert(&like, v)?
        };
        let d = data(&o, "#v");
        if d.len() != 1 {
            return Err(err(
                "MATLAB:colon:NonScalarInputs",
                "Colon operands must be scalars.",
            ));
        }
        Ok(d[0])
    };
    let (s, e) = (conv(a)?, conv(b)?);
    let mut out = Vec::new();
    match step {
        Some(st) if class_is(st, "calendarDuration") => {
            if lead != "datetime" {
                return Err(err(
                    "MATLAB:colon:UnsupportedType",
                    "A calendarDuration step needs datetime endpoints.",
                ));
            }
            let c = obj_of(st).unwrap();
            let (m, d, t) = (data(c, "#m")[0], data(c, "#d")[0], data(c, "#t")[0]);
            let first = cal_add(s, m, d, t);
            if first == s || !first.is_finite() {
                return Err(err(
                    "MATLAB:colon:ZeroStep",
                    "The step of a datetime range must not be zero.",
                ));
            }
            let fwd = first > s;
            let mut k = 0.0;
            loop {
                let x = cal_add(s, m * k, d * k, t * k);
                if (fwd && x > e) || (!fwd && x < e) || k > MAX {
                    break;
                }
                out.push(x);
                k += 1.0;
            }
        }
        _ => {
            let h = match step {
                None => DAY,
                Some(st) => {
                    let (_, v) = as_secs(st).ok_or_else(|| {
                        err(
                            "MATLAB:colon:UnsupportedType",
                            format!("Invalid range step of class {}.", st.class_name()),
                        )
                    })?;
                    v.first().copied().unwrap_or(f64::NAN)
                }
            };
            if h.is_finite() && h != 0.0 && s.is_finite() && e.is_finite() {
                let n = ((e - s) / h + 1e-10).floor();
                if n > MAX {
                    return Err(err(
                        "MATLAB:colon:TooManyElements",
                        "The range has too many elements.",
                    ));
                }
                if n >= 0.0 {
                    out = (0..=n as usize).map(|k| s + h * k as f64).collect();
                }
            }
        }
    }
    let mut o = like;
    o.dims = vec![1, out.len()];
    set_data(&mut o, "#v", out);
    Ok(wrap(o))
}

fn linspace_dt(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "linspace")?;
    let x = arg_dt(&a[0], "linspace")?;
    let y = dt_convert(x, &a[1])?;
    let n = a
        .get(2)
        .and_then(|v| v.scalar_f64())
        .unwrap_or(100.0)
        .max(1.0) as usize;
    let (s, e) = (data(x, "#v")[0], data(&y, "#v")[0]);
    let v: Vec<f64> = (0..n)
        .map(|k| {
            if n == 1 {
                e
            } else {
                s + (e - s) * k as f64 / (n - 1) as f64
            }
        })
        .collect();
    let mut o = x.clone();
    o.dims = vec![1, n];
    set_data(&mut o, "#v", v);
    Ok(vec![wrap(o)])
}

const DT_METHODS: &[(&str, BuiltinFn)] = &[
    ("char", char_m),
    ("string", string_m),
    ("cellstr", cellstr_m),
    ("sort", sort_m),
    ("min", min_m),
    ("max", max_m),
    ("unique", unique_m),
    ("issorted", issorted_m),
    ("ismember", ismember_m),
    ("median", median_m),
    ("mean", mean_dt),
    ("mode", mode_m),
    ("cummin", cummin_m),
    ("cummax", cummax_m),
    ("diff", diff_dt),
    ("sum", sum_dt),
    ("find", find_m),
    ("isnan", isnat_f),
    ("isinf", isinf_m),
    ("isfinite", isfinite_m),
    ("split", split_dt),
    ("datenum", datenum_m),
    ("datevec", datevec_m),
    ("datestr", datestr_m),
    ("colon", dt_colon),
    ("linspace", linspace_dt),
];

const DUR_METHODS: &[(&str, BuiltinFn)] = &[
    ("char", char_m),
    ("string", string_m),
    ("cellstr", cellstr_m),
    ("sort", sort_m),
    ("min", min_m),
    ("max", max_m),
    ("unique", unique_m),
    ("issorted", issorted_m),
    ("ismember", ismember_m),
    ("median", median_m),
    ("mean", mean_d),
    ("mode", mode_m),
    ("sum", sum_d),
    ("cumsum", cumsum_d),
    ("cummin", cummin_m),
    ("cummax", cummax_m),
    ("abs", abs_d),
    ("diff", diff_d),
    ("std", std_d),
    ("find", find_m),
    ("isnan", isnan_m),
    ("isinf", isinf_m),
    ("isfinite", isfinite_m),
    ("round", round_d),
    ("floor", floor_d),
    ("ceil", ceil_d),
    ("split", split_dur),
    ("hms", hms_f),
    ("colon", dt_colon),
];

const CD_METHODS: &[(&str, BuiltinFn)] = &[
    ("char", char_m),
    ("string", string_m),
    ("cellstr", cellstr_m),
    ("split", split_cd),
];

// ---------------------------------------------------------------------------
// Classic serial dates: datenum, datestr, datevec, weekday, eomday
// ---------------------------------------------------------------------------

fn dn_to_secs(dn: f64) -> f64 {
    (dn - EPOCH_DATENUM) * DAY
}

/// Rounded components of a datenum (milliseconds rounding like MATLAB).
fn dn_parts(dn: f64) -> (f64, f64, f64, f64, f64, f64) {
    let s = (dn_to_secs(dn) * 1000.0).round() / 1000.0;
    parts(s)
}

/// Classic format tokens (datestr/datenum/datevec).
fn classic_tokens(fmt: &str) -> Vec<(String, bool)> {
    let toks = [
        "yyyy", "yy", "mmmm", "mmm", "mm", "dddd", "ddd", "dd", "HH", "MM", "SS", "FFF", "AM",
        "PM", "m", "d",
    ];
    let mut out = Vec::new();
    let mut i = 0;
    let cs: Vec<char> = fmt.chars().collect();
    while i < cs.len() {
        let rest: String = cs[i..].iter().collect();
        if let Some(t) = toks.iter().find(|t| rest.starts_with(**t)) {
            out.push((t.to_string(), true));
            i += t.len();
        } else {
            out.push((cs[i].to_string(), false));
            i += 1;
        }
    }
    out
}

pub fn classic_format(dn: f64, fmt: &str) -> String {
    if dn.is_nan() {
        return "NaN".into();
    }
    let (y, m, d, h, mi, s) = dn_parts(dn);
    let toks = classic_tokens(fmt);
    let ampm = toks.iter().any(|(t, k)| *k && (t == "AM" || t == "PM"));
    let mut out = String::new();
    let secs = dn_to_secs(dn);
    for (t, is_tok) in toks {
        if !is_tok {
            out.push_str(&t);
            continue;
        }
        let k = (m as usize).clamp(1, 12) - 1;
        match t.as_str() {
            "yyyy" => out.push_str(&format!("{:04}", y as i64)),
            "yy" => out.push_str(&format!("{:02}", (y as i64).rem_euclid(100))),
            "mmmm" => out.push_str(MONTHS[k]),
            "mmm" => out.push_str(&MONTHS[k][..3]),
            "mm" => out.push_str(&format!("{:02}", m as i64)),
            "m" => out.push_str(&MONTHS[k][..1]),
            "dddd" => out.push_str(WEEKDAYS[weekday_of(secs) as usize - 1]),
            "ddd" => out.push_str(&WEEKDAYS[weekday_of(secs) as usize - 1][..3]),
            "d" => out.push_str(&WEEKDAYS[weekday_of(secs) as usize - 1][..1]),
            "dd" => out.push_str(&format!("{:02}", d as i64)),
            "HH" => {
                if ampm {
                    let hh = (h as i64) % 12;
                    out.push_str(&format!("{:2}", if hh == 0 { 12 } else { hh }));
                } else {
                    out.push_str(&format!("{:02}", h as i64));
                }
            }
            "MM" => out.push_str(&format!("{:02}", mi as i64)),
            "SS" => out.push_str(&format!("{:02}", s.floor() as i64)),
            "FFF" => out.push_str(&format!(
                "{:03}",
                ((s - s.floor()) * 1000.0).round().min(999.0) as i64
            )),
            "AM" | "PM" => out.push_str(if h < 12.0 { "AM" } else { "PM" }),
            _ => {}
        }
    }
    out
}

fn classic_code(k: i64) -> Option<&'static str> {
    Some(match k {
        0 => "dd-mmm-yyyy HH:MM:SS",
        1 => "dd-mmm-yyyy",
        2 => "mm/dd/yy",
        3 => "mmm",
        4 => "m",
        5 => "mm",
        6 => "mm/dd",
        7 => "dd",
        8 => "ddd",
        9 => "d",
        10 => "yyyy",
        11 => "yy",
        12 => "mmmyy",
        13 => "HH:MM:SS",
        14 => "HH:MM:SS PM",
        15 => "HH:MM",
        16 => "HH:MM PM",
        17 => "QQ-YY",
        18 => "QQ",
        19 => "dd/mm",
        20 => "dd/mm/yy",
        21 => "mmm.dd,yyyy HH:MM:SS",
        22 => "mmm.dd,yyyy",
        23 => "mm/dd/yyyy",
        24 => "dd/mm/yyyy",
        25 => "yy/mm/dd",
        26 => "yyyy/mm/dd",
        27 => "QQ-YYYY",
        28 => "mmmyyyy",
        29 => "yyyy-mm-dd",
        30 => "yyyymmddTHHMMSS",
        31 => "yyyy-mm-dd HH:MM:SS",
        _ => return None,
    })
}

/// Classic format text → numeric value (datenum) or `None`.
fn classic_parse(text: &str, fmt: &str) -> Option<f64> {
    let cs: Vec<char> = text.trim().chars().collect();
    let mut i = 0;
    let (mut y, mut mo, mut d, mut h, mut mi, mut s) = (f64::NAN, 1.0, 1.0, 0.0, 0.0, 0.0);
    let mut pm = None;
    let num = |i: &mut usize, max: usize| -> Option<f64> {
        let st = *i;
        while *i < cs.len() && cs[*i].is_ascii_digit() && *i - st < max {
            *i += 1;
        }
        if *i == st {
            return None;
        }
        cs[st..*i].iter().collect::<String>().parse().ok()
    };
    for (t, is_tok) in classic_tokens(fmt) {
        if !is_tok {
            if t == " " {
                while i < cs.len() && cs[i] == ' ' {
                    i += 1;
                }
                continue;
            }
            if i < cs.len() && cs[i].to_string().eq_ignore_ascii_case(&t) {
                i += 1;
                continue;
            }
            return None;
        }
        match t.as_str() {
            "yyyy" => y = num(&mut i, 4)?,
            "yy" => {
                let v = num(&mut i, 2)?;
                y = if v < 50.0 { 2000.0 + v } else { 1900.0 + v };
            }
            "mmmm" | "mmm" => {
                let rest: String = cs[i..].iter().collect::<String>().to_lowercase();
                let k = MONTHS
                    .iter()
                    .position(|n| rest.starts_with(&n.to_lowercase()[..3]))?;
                let full = MONTHS[k].to_lowercase();
                i += if t == "mmmm" && rest.starts_with(&full) {
                    full.len()
                } else {
                    3
                };
                mo = k as f64 + 1.0;
            }
            "mm" => mo = num(&mut i, 2)?,
            "dd" => d = num(&mut i, 2)?,
            "HH" => h = num(&mut i, 2)?,
            "MM" => mi = num(&mut i, 2)?,
            "SS" => s = num(&mut i, 2)?,
            "FFF" => s += num(&mut i, 3)? / 1000.0,
            "AM" | "PM" => {
                let rest: String = cs[i..].iter().collect::<String>().to_uppercase();
                pm = Some(rest.starts_with("PM"));
                i += 2;
            }
            _ => {
                // ddd / dddd / d / m: skip letters.
                while i < cs.len() && cs[i].is_alphabetic() {
                    i += 1;
                }
            }
        }
    }
    if i != cs.len() {
        return None;
    }
    if let Some(p) = pm {
        if p && h < 12.0 {
            h += 12.0;
        } else if !p && h == 12.0 {
            h = 0.0;
        }
    }
    if y.is_nan() {
        y = parts(now_local().0).0;
    }
    Some(secs_of(y, mo, d, h, mi, s) / DAY + EPOCH_DATENUM)
}

/// Text without a format: classic formats, then datetime guesses.
fn classic_guess(text: &str) -> Option<f64> {
    for k in [0, 1, 31, 29, 23, 13, 15, 2, 26, 24, 21, 22, 30] {
        if let Some(v) = classic_parse(text, classic_code(k).unwrap()) {
            return Some(v);
        }
    }
    guess_dt(text).map(|g| g.0 / DAY + EPOCH_DATENUM)
}

fn text_datenums(v: &Value, fmt: Option<&str>, fname: &str) -> R<Vec<f64>> {
    let items = text_list(v).ok_or_else(|| {
        err(
            "MATLAB:datenum:InvalidInput",
            format!("{}: text expected.", fname),
        )
    })?;
    items
        .iter()
        .map(|t| {
            let r = match fmt {
                Some(f) => classic_parse(t, f),
                None => classic_guess(t),
            };
            r.ok_or_else(|| {
                err(
                    "MATLAB:datenum:ConvertDateString",
                    format!(
                        "{}: DATENUM failed to convert '{}'{}.",
                        fname,
                        t,
                        fmt.map(|f| format!(" with format '{}'", f))
                            .unwrap_or_default()
                    ),
                )
            })
        })
        .collect()
}

fn datenum_f(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "datenum")?;
    if let Some(o) = obj_of(&a[0])
        && o.class == "datetime"
    {
        return datenum_m(it, a, nargout);
    }
    if a[0].is_text() || matches!(a[0], Value::Cell(_)) {
        let fmt = match a.get(1) {
            Some(v) if v.is_text() => Some(v.to_str().unwrap_or_default()),
            Some(v) => v
                .scalar_f64()
                .and_then(|k| classic_code(k as i64))
                .map(String::from),
            None => None,
        };
        let v = text_datenums(&a[0], fmt.as_deref(), "datenum")?;
        let n = v.len();
        return Ok(vec![if n == 1 {
            Value::num(v[0])
        } else {
            Value::col(v)
        }]);
    }
    if a.len() == 1 {
        let (d, x) = num_data(&a[0]).ok_or_else(|| {
            err(
                "MATLAB:datenum:InvalidInput",
                "datenum: numeric input expected.",
            )
        })?;
        let (r, c) = (d[0], d.get(1).copied().unwrap_or(1));
        if d.len() == 2 && (c == 3 || c == 6) {
            let at = |i: usize, j: usize| if j < c { x[i + j * r] } else { 0.0 };
            let out: Vec<f64> = (0..r)
                .map(|i| {
                    secs_of(at(i, 0), at(i, 1), at(i, 2), at(i, 3), at(i, 4), at(i, 5)) / DAY
                        + EPOCH_DATENUM
                })
                .collect();
            return Ok(vec![Value::col(out)]);
        }
        return Ok(vec![num_val(d, x)]);
    }
    let (dims, rows) = components(&a, "datenum")?;
    let v = rows
        .iter()
        .map(|r| secs_of(r[0], r[1], r[2], r[3], r[4], r[5]) / DAY + EPOCH_DATENUM)
        .collect();
    Ok(vec![num_val(dims, v)])
}

fn datestr_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "datestr")?;
    if let Some(o) = obj_of(&a[0])
        && o.class == "datetime"
    {
        let nums: Vec<f64> = data(o, "#v")
            .iter()
            .map(|s| s / DAY + EPOCH_DATENUM)
            .collect();
        let mut args = vec![Value::col(nums)];
        args.extend(a[1..].iter().cloned());
        return datestr_f_inner(&args);
    }
    datestr_f_inner(&a)
}

fn datestr_f_inner(a: &[Value]) -> R<Vec<Value>> {
    let nums: Vec<f64> = if a[0].is_text() || matches!(a[0], Value::Cell(_)) {
        text_datenums(&a[0], None, "datestr")?
    } else {
        let (d, x) = num_data(&a[0]).ok_or_else(|| {
            err(
                "MATLAB:datestr:InvalidInput",
                "datestr: numeric or text input expected.",
            )
        })?;
        let c = d.get(1).copied().unwrap_or(1);
        if d.len() == 2
            && (c == 3 || c == 6)
            && x.iter().take(d[0]).all(|y| *y > 31.0 && *y < 10_000.0)
        {
            let r = d[0];
            let at = |i: usize, j: usize| if j < c { x[i + j * r] } else { 0.0 };
            (0..r)
                .map(|i| {
                    secs_of(at(i, 0), at(i, 1), at(i, 2), at(i, 3), at(i, 4), at(i, 5)) / DAY
                        + EPOCH_DATENUM
                })
                .collect()
        } else {
            x
        }
    };
    let fmt = match a.get(1) {
        Some(v) if v.is_text() => v.to_str().unwrap_or_default(),
        Some(v) => {
            let k = v.scalar_f64().unwrap_or(0.0) as i64;
            classic_code(k)
                .ok_or_else(|| {
                    err(
                        "MATLAB:datestr:DateNumber",
                        format!("datestr: unknown format number {}.", k),
                    )
                })?
                .to_string()
        }
        None => {
            if nums
                .iter()
                .all(|n| n.is_nan() || ((n * DAY).round() / DAY).fract() == 0.0)
            {
                "dd-mmm-yyyy".into()
            } else {
                "dd-mmm-yyyy HH:MM:SS".into()
            }
        }
    };
    let rows: Vec<String> = nums.iter().map(|&n| classic_format(n, &fmt)).collect();
    if rows.len() == 1 {
        return Ok(vec![Value::str(&rows[0])]);
    }
    let w = rows.iter().map(|s| s.chars().count()).max().unwrap_or(0);
    let n = rows.len();
    let mut data = vec![' '; n * w];
    for (i, s) in rows.iter().enumerate() {
        for (j, c) in s.chars().enumerate() {
            data[i + j * n] = c;
        }
    }
    Ok(vec![Value::Char(Array::new(vec![n, w], data))])
}

fn datevec_out(nums: Vec<f64>, nargout: usize) -> R<Vec<Value>> {
    let n = nums.len();
    let mut cols = vec![vec![0.0; n]; 6];
    for (i, &dn) in nums.iter().enumerate() {
        let p = if dn.is_nan() {
            (f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN)
        } else {
            dn_parts(dn)
        };
        for (k, v) in [p.0, p.1, p.2, p.3, p.4, p.5].iter().enumerate() {
            cols[k][i] = *v;
        }
    }
    if nargout <= 1 {
        let mut all = Vec::with_capacity(6 * n);
        for c in &cols {
            all.extend_from_slice(c);
        }
        return Ok(vec![num_val(vec![n, 6], all)]);
    }
    Ok(cols.into_iter().take(nargout).map(Value::col).collect())
}

fn datevec_f(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "datevec")?;
    if let Some(o) = obj_of(&a[0])
        && o.class == "datetime"
    {
        return datevec_m(it, a, nargout);
    }
    let nums = if a[0].is_text() || matches!(a[0], Value::Cell(_)) {
        let fmt = match a.get(1) {
            Some(v) if v.is_text() => Some(v.to_str().unwrap_or_default()),
            Some(v) => v
                .scalar_f64()
                .and_then(|k| classic_code(k as i64))
                .map(String::from),
            None => None,
        };
        text_datenums(&a[0], fmt.as_deref(), "datevec")?
    } else {
        num_data(&a[0])
            .ok_or_else(|| {
                err(
                    "MATLAB:datevec:InvalidInput",
                    "datevec: numeric or text input expected.",
                )
            })?
            .1
    };
    datevec_out(nums, nargout)
}

fn weekday_f(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "weekday")?;
    let (dims, nums) = if let Some(o) = obj_of(&a[0])
        && o.class == "datetime"
    {
        (
            o.dims.clone(),
            data(o, "#v")
                .iter()
                .map(|s| s / DAY + EPOCH_DATENUM)
                .collect::<Vec<_>>(),
        )
    } else if a[0].is_text() || matches!(a[0], Value::Cell(_)) {
        let v = text_datenums(&a[0], None, "weekday")?;
        (vec![v.len(), 1], v)
    } else {
        num_data(&a[0]).ok_or_else(|| {
            err(
                "MATLAB:weekday:InvalidInput",
                "weekday: numeric input expected.",
            )
        })?
    };
    let w: Vec<f64> = nums
        .iter()
        .map(|&n| {
            if n.is_nan() {
                f64::NAN
            } else {
                weekday_of(dn_to_secs(n.floor()))
            }
        })
        .collect();
    let mut out = vec![num_val(dims.clone(), w.clone())];
    if nargout > 1 {
        let long = a.get(1).and_then(|v| v.to_str()).as_deref() == Some("long");
        let names: Vec<String> = w
            .iter()
            .map(|k| {
                let n = WEEKDAYS[(*k as usize).clamp(1, 7) - 1];
                if long {
                    n.to_string()
                } else {
                    n[..3].to_string()
                }
            })
            .collect();
        let width = names.iter().map(|s| s.len()).max().unwrap_or(0);
        let r = names.len();
        let mut data = vec![' '; r * width];
        for (i, s) in names.iter().enumerate() {
            for (j, c) in s.chars().enumerate() {
                data[i + j * r] = c;
            }
        }
        out.push(Value::Char(Array::new(vec![r, width], data)));
    }
    Ok(out)
}

fn eomday_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "eomday")?;
    let (dims, rows) = components(&a, "eomday")?;
    let v = rows
        .iter()
        .map(|r| {
            if r[0].is_nan() || r[1].is_nan() {
                f64::NAN
            } else {
                month_days(r[0] as i64, r[1] as i64) as f64
            }
        })
        .collect();
    Ok(vec![num_val(dims, v)])
}

fn today_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let dn = (now_local().0 / DAY).floor() + EPOCH_DATENUM;
    if a.first().and_then(|v| v.to_str()).as_deref() == Some("datetime") {
        return Ok(vec![dt_value(
            vec![1, 1],
            vec![(dn - EPOCH_DATENUM) * DAY],
            DATE_FMT,
        )]);
    }
    Ok(vec![Value::num(dn)])
}

fn addtodate_f(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 3, "addtodate")?;
    let (d, x) = num_data(&a[0]).ok_or_else(|| {
        err(
            "MATLAB:addtodate:InvalidInput",
            "addtodate: date number expected.",
        )
    })?;
    let q = a[1].scalar_f64().unwrap_or(0.0);
    let f = a[2].to_str().unwrap_or_default().to_ascii_lowercase();
    let v = x
        .iter()
        .map(|&dn| {
            let s = dn_to_secs(dn);
            let r = match f.as_str() {
                "year" => cal_add(s, 12.0 * q, 0.0, 0.0),
                "month" => cal_add(s, q, 0.0, 0.0),
                "week" => s + 7.0 * q * DAY,
                "day" => s + q * DAY,
                "hour" => s + q * 3600.0,
                "minute" => s + q * 60.0,
                "second" => s + q,
                "millisecond" => s + q / 1000.0,
                _ => f64::NAN,
            };
            r / DAY + EPOCH_DATENUM
        })
        .collect();
    if !matches!(
        f.as_str(),
        "year" | "month" | "week" | "day" | "hour" | "minute" | "second" | "millisecond"
    ) {
        return Err(err(
            "MATLAB:addtodate:InvalidDateField",
            format!("addtodate: unknown field '{}'.", f),
        ));
    }
    Ok(vec![num_val(d, v)])
}

fn isdur_like(v: &Value) -> bool {
    class_is(v, "duration")
}

fn opts_unused(a: &[Value]) -> R<()> {
    let _ = name_values(a, "datetime")?;
    let _ = isdur_like;
    Ok(())
}

pub fn register(it: &mut Interp) {
    let _ = opts_unused;
    reg(it, "datetime", datetime_fn);
    reg(it, "NaT", nat_fn);
    reg(it, "duration", duration_fn);
    reg(it, "calendarDuration", caldur_fn);
    reg(it, "years", years_f);
    reg(it, "days", days_f);
    reg(it, "hours", hours_f);
    reg(it, "minutes", minutes_f);
    reg(it, "seconds", seconds_f);
    reg(it, "milliseconds", ms_f);
    reg(it, "calyears", calyears_f);
    reg(it, "calquarters", calquarters_f);
    reg(it, "calmonths", calmonths_f);
    reg(it, "calweeks", calweeks_f);
    reg(it, "caldays", caldays_f);
    reg(it, "year", year_f);
    reg(it, "month", month_f);
    reg(it, "day", day_f);
    reg(it, "hour", hour_f);
    reg(it, "minute", minute_f);
    reg(it, "second", second_f);
    reg(it, "quarter", quarter_f);
    reg(it, "week", week_f);
    reg(it, "ymd", ymd_f);
    reg(it, "hms", hms_f);
    reg(it, "timeofday", timeofday_f);
    reg(it, "isweekend", isweekend_f);
    reg(it, "isnat", isnat_f);
    reg(it, "posixtime", posixtime_f);
    reg(it, "juliandate", juliandate_f);
    reg(it, "exceltime", exceltime_f);
    reg(it, "yyyymmdd", yyyymmdd_f);
    reg(it, "dateshift", dateshift_f);
    reg(it, "between", between_f);
    reg(it, "isdatetime", isdatetime_f);
    reg(it, "isduration", isduration_f);
    reg(it, "iscalendarduration", iscaldur_f);
    reg(it, "datenum", datenum_f);
    reg(it, "datestr", datestr_f);
    reg(it, "datevec", datevec_f);
    reg(it, "weekday", weekday_f);
    reg(it, "eomday", eomday_f);
    reg(it, "today", today_f);
    reg(it, "addtodate", addtodate_f);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_round_trip() {
        for z in [-800_000i64, -1, 0, 1, 19_000, 2_000_000] {
            let (y, m, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, m, d), z);
        }
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(
            secs_of(2000.0, 1.0, 1.0, 0.0, 0.0, 0.0) / DAY + EPOCH_DATENUM,
            730_486.0
        );
    }

    #[test]
    fn formats() {
        let s = secs_of(2024.0, 3.0, 5.0, 14.0, 7.0, 9.5);
        assert_eq!(fmt_dt(s, "dd-MMM-uuuu HH:mm:ss"), "05-Mar-2024 14:07:09");
        assert_eq!(
            fmt_dt(s, "yyyy-MM-dd'T'HH:mm:ss.SSS"),
            "2024-03-05T14:07:09.500"
        );
        assert_eq!(fmt_dt(s, "eeee, MMMM d"), "Tuesday, March 5");
        assert_eq!(
            parse_dt("2024-03-05", "yyyy-MM-dd", 1970),
            Some(secs_of(2024.0, 3.0, 5.0, 0.0, 0.0, 0.0))
        );
        assert_eq!(
            parse_dt("05.03.2024 14:07", "dd.MM.yyyy HH:mm", 1970),
            Some(secs_of(2024.0, 3.0, 5.0, 14.0, 7.0, 0.0))
        );
        assert!(parse_dt("2024-02-30", "yyyy-MM-dd", 1970).is_none());
        assert_eq!(fmt_dur(6.0 * 3600.0, "hh:mm:ss"), "06:00:00");
        assert_eq!(fmt_dur(-90.5, "mm:ss.SS"), "-01:30.50");
        assert_eq!(fmt_dur(2.0 * DAY, "d"), "2 days");
        assert_eq!(
            fmt_cd(14.0, 4.0, 22_090.125, "ymdt"),
            "1y 2mo 4d 6h 8m 10.125s"
        );
        assert_eq!(
            classic_format(730_486.5, "yyyy-mm-dd HH:MM"),
            "2000-01-01 12:00"
        );
    }

    #[test]
    fn month_arithmetic() {
        let s = secs_of(2024.0, 1.0, 31.0, 0.0, 0.0, 0.0);
        let r = cal_add(s, 1.0, 0.0, 0.0);
        assert_eq!(parts(r).1, 2.0);
        assert_eq!(parts(r).2, 29.0);
    }
}
