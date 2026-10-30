//! MATLAB `LineSpec` strings such as `'r--o'`, `':sk'`, `'x'`.

use crate::color::{Rgb, named};
use crate::error::GraphicsError;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LineSpec {
    /// `-`, `--`, `:`, `-.` or `none` when only a marker was given.
    pub line_style: Option<String>,
    /// Canonical marker name (`o`, `s`, `^`, `d`, `p`, `h`, ...).
    pub marker: Option<String>,
    pub color: Option<Rgb>,
}

impl LineSpec {
    pub fn is_empty(&self) -> bool {
        self.line_style.is_none() && self.marker.is_none() && self.color.is_none()
    }
}

/// Canonicalises a marker property value (`'square'` → `s`, `'diamond'` → `d`).
pub fn canonical_marker(name: &str) -> Option<&'static str> {
    Some(match name.to_ascii_lowercase().as_str() {
        "none" => "none",
        "o" => "o",
        "+" => "+",
        "*" => "*",
        "." => ".",
        "x" => "x",
        "s" | "square" => "s",
        "d" | "diamond" => "d",
        "^" => "^",
        "v" => "v",
        ">" => ">",
        "<" => "<",
        "p" | "pentagram" => "p",
        "h" | "hexagram" => "h",
        "|" => "|",
        "_" => "_",
        _ => return None,
    })
}

pub fn canonical_line_style(name: &str) -> Option<&'static str> {
    Some(match name {
        "-" => "-",
        "--" => "--",
        ":" => ":",
        "-." => "-.",
        "none" | "None" | "NONE" => "none",
        _ => return None,
    })
}

/// Parses a LineSpec. Returns `Ok(None)` when the string is not a LineSpec
/// at all (so callers can treat it as a property name instead).
pub fn parse(spec: &str) -> Result<Option<LineSpec>, GraphicsError> {
    // Octave's `;key;` legend suffix is accepted and ignored.
    let body = match spec.find(';') {
        Some(i) => &spec[..i],
        None => spec,
    };
    let mut out = LineSpec::default();
    let bytes = body.as_bytes();
    let mut i = 0;
    let duplicate = || {
        Err(GraphicsError::new(
            "MATLAB:plot:InvalidLineSpec",
            format!("plot: invalid LineSpec '{spec}'"),
        ))
    };
    let set = |slot: &mut Option<String>, value: &str| -> bool {
        if slot.is_some() {
            return false;
        }
        *slot = Some(value.to_string());
        true
    };
    // Long marker names are accepted inside a LineSpec too.
    for long in ["square", "diamond", "pentagram", "hexagram"] {
        if body.eq_ignore_ascii_case(long) {
            out.marker = canonical_marker(long).map(str::to_string);
            out.line_style = Some("none".into());
            return Ok(Some(out));
        }
    }
    // Anything outside the LineSpec alphabet is a property name, not an error.
    if !body
        .chars()
        .all(|c| "-:.rgbcmykwo+*xsd^v><ph ".contains(c.to_ascii_lowercase()))
    {
        return Ok(None);
    }
    while i < bytes.len() {
        let rest = &body[i..];
        let c = bytes[i] as char;
        if rest.starts_with("--") || rest.starts_with("-.") {
            if !set(&mut out.line_style, &rest[..2]) {
                return duplicate();
            }
            i += 2;
        } else if c == '-' || c == ':' {
            if !set(&mut out.line_style, &rest[..1]) {
                return duplicate();
            }
            i += 1;
        } else if let Some(rgb) = named(&rest[..1]).filter(|_| c.is_ascii_alphabetic()) {
            if out.color.is_some() {
                return duplicate();
            }
            out.color = Some(rgb);
            i += 1;
        } else if let Some(m) = canonical_marker(&rest[..1]).filter(|m| *m != "none") {
            if !set(&mut out.marker, m) {
                return duplicate();
            }
            i += 1;
        } else if c == ' ' {
            i += 1;
        } else {
            return Ok(None);
        }
    }
    if out.is_empty() {
        return Ok(None);
    }
    if out.line_style.is_none() && out.marker.is_some() {
        out.line_style = Some("none".into());
    }
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> LineSpec {
        parse(s).unwrap().unwrap()
    }

    #[test]
    fn parses_combinations() {
        let s = p("r--o");
        assert_eq!(s.line_style.as_deref(), Some("--"));
        assert_eq!(s.marker.as_deref(), Some("o"));
        assert_eq!(s.color, Some([1.0, 0.0, 0.0]));
        assert_eq!(p("-.").line_style.as_deref(), Some("-."));
        assert_eq!(p(":sk").marker.as_deref(), Some("s"));
        assert_eq!(p("x").line_style.as_deref(), Some("none"));
        assert_eq!(p("g").line_style, None);
        assert_eq!(p("o-").line_style.as_deref(), Some("-"));
        assert_eq!(p("diamond").marker.as_deref(), Some("d"));
        assert_eq!(p("b;data;").color, Some([0.0, 0.0, 1.0]));
    }

    #[test]
    fn rejects_and_passes_through() {
        assert!(parse("LineWidth").unwrap().is_none());
        assert!(parse("Color").unwrap().is_none());
        assert!(parse("Marker").unwrap().is_none());
        assert!(parse("box").is_err()); // all LineSpec characters, two markers
        assert!(parse("rr").is_err());
        assert!(parse("--:").is_err());
        assert!(parse("").unwrap().is_none());
    }
}
