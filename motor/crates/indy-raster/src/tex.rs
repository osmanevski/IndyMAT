//! The UI's TeX subset (`frontend/figure_shape_utils.cjs`): styled runs from
//! Greek letters, common symbols, `^`/`_` with one character or a braced
//! group, `\bf \it \sl \rm`. `\fontname`, `\fontsize`, `\color` are dropped
//! with their arguments; an unknown command stays as typed.

#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub text: String,
    /// 1 superscript, -1 subscript, 0 baseline.
    pub script: i8,
    pub bold: bool,
    pub italic: bool,
}

#[derive(Clone, Copy)]
struct Style {
    script: i8,
    bold: bool,
    italic: bool,
}

fn symbol(name: &str) -> Option<&'static str> {
    Some(match name {
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" => "ε",
        "zeta" => "ζ",
        "eta" => "η",
        "theta" => "θ",
        "vartheta" => "ϑ",
        "iota" => "ι",
        "kappa" => "κ",
        "lambda" => "λ",
        "mu" => "μ",
        "nu" => "ν",
        "xi" => "ξ",
        "o" => "ο",
        "pi" => "π",
        "varpi" => "ϖ",
        "rho" => "ρ",
        "sigma" => "σ",
        "varsigma" => "ς",
        "tau" => "τ",
        "upsilon" => "υ",
        "phi" => "φ",
        "chi" => "χ",
        "psi" => "ψ",
        "omega" => "ω",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Xi" => "Ξ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Upsilon" => "Υ",
        "Phi" => "Φ",
        "Psi" => "Ψ",
        "Omega" => "Ω",
        "pm" => "±",
        "times" => "×",
        "cdot" => "·",
        "div" => "÷",
        "leq" => "≤",
        "geq" => "≥",
        "neq" => "≠",
        "approx" => "≈",
        "equiv" => "≡",
        "propto" => "∝",
        "infty" => "∞",
        "partial" => "∂",
        "nabla" => "∇",
        "int" => "∫",
        "sum" => "∑",
        "prod" => "∏",
        "surd" => "√",
        "circ" => "∘",
        "deg" => "°",
        "bullet" => "•",
        "ldots" => "…",
        "leftarrow" => "←",
        "rightarrow" => "→",
        "uparrow" => "↑",
        "downarrow" => "↓",
        "leftrightarrow" => "↔",
        "Leftarrow" => "⇐",
        "Rightarrow" => "⇒",
        "in" => "∈",
        "cap" => "∩",
        "cup" => "∪",
        "forall" => "∀",
        "exists" => "∃",
        "neg" => "¬",
        "angle" => "∠",
        "perp" => "⊥",
        "mid" => "|",
        "sim" => "∼",
        "copyright" => "©",
        _ => return None,
    })
}

struct Parser {
    text: Vec<char>,
    i: usize,
    runs: Vec<Run>,
}

impl Parser {
    fn push(&mut self, value: &str, style: Style) {
        if value.is_empty() {
            return;
        }
        if let Some(last) = self.runs.last_mut()
            && last.script == style.script
            && last.bold == style.bold
            && last.italic == style.italic
        {
            last.text.push_str(value);
            return;
        }
        self.runs.push(Run {
            text: value.to_string(),
            script: style.script,
            bold: style.bold,
            italic: style.italic,
        });
    }

    /// `text[i]` is the backslash: the command name, or a literal.
    fn command(&mut self) -> Result<String, String> {
        let Some(&next) = self.text.get(self.i + 1) else {
            self.i += 1;
            return Err("\\".to_string());
        };
        if !next.is_ascii_alphabetic() {
            self.i += 2;
            return Err(next.to_string());
        }
        let mut j = self.i + 1;
        while j < self.text.len() && self.text[j].is_ascii_alphabetic() {
            j += 1;
        }
        let name: String = self.text[self.i + 1..j].iter().collect();
        self.i = j;
        Ok(name)
    }

    fn skip_argument(&mut self) {
        if self.text.get(self.i) == Some(&'[')
            && let Some(end) = self.text[self.i..].iter().position(|c| *c == ']')
        {
            self.i += end + 1;
        }
        if self.text.get(self.i) == Some(&'{') {
            match self.text[self.i..].iter().position(|c| *c == '}') {
                Some(end) => self.i += end + 1,
                None => self.i = self.text.len(),
            }
        }
    }

    /// Applies a command; a font command returns the changed style.
    fn apply(&mut self, name: &str, style: Style) -> Option<Style> {
        if let Some(s) = symbol(name) {
            self.push(s, style);
        } else if matches!(name, "fontname" | "fontsize" | "color") {
            self.skip_argument();
        } else {
            match name {
                "bf" => {
                    return Some(Style {
                        bold: true,
                        ..style
                    });
                }
                "it" | "sl" => {
                    return Some(Style {
                        italic: true,
                        ..style
                    });
                }
                "rm" => {
                    return Some(Style {
                        bold: false,
                        italic: false,
                        ..style
                    });
                }
                _ => self.push(&format!("\\{name}"), style),
            }
        }
        None
    }

    fn group(&mut self, style: Style, braced: bool, depth: usize) {
        let mut current = style;
        while self.i < self.text.len() {
            let ch = self.text[self.i];
            if ch == '}' {
                self.i += 1;
                if braced {
                    return;
                }
                continue;
            }
            if ch == '{' && depth < 16 {
                self.i += 1;
                self.group(current, true, depth + 1);
            } else if ch == '\\' {
                match self.command() {
                    Err(literal) => self.push(&literal, current),
                    Ok(name) => {
                        if let Some(changed) = self.apply(&name, current) {
                            current = changed;
                        }
                    }
                }
            } else if (ch == '^' || ch == '_') && self.i + 1 < self.text.len() {
                let scripted = Style {
                    script: if ch == '^' { 1 } else { -1 },
                    ..current
                };
                self.i += 1;
                if self.text[self.i] == '{' && depth < 16 {
                    self.i += 1;
                    self.group(scripted, true, depth + 1);
                } else if self.text[self.i] == '\\' {
                    match self.command() {
                        Err(literal) => self.push(&literal, scripted),
                        Ok(name) => {
                            self.apply(&name, scripted);
                        }
                    }
                } else {
                    let point = self.text[self.i].to_string();
                    self.push(&point, scripted);
                    self.i += 1;
                }
            } else {
                self.push(&ch.to_string(), current);
                self.i += 1;
            }
        }
    }
}

/// `interpreter` is `none` for plain text; anything else (`tex`, also
/// `latex`, whose math this subset reads where it can) is parsed.
pub fn parse(source: &str, interpreter: &str) -> Vec<Run> {
    if interpreter == "none" {
        if source.is_empty() {
            return Vec::new();
        }
        return vec![Run {
            text: source.to_string(),
            script: 0,
            bold: false,
            italic: false,
        }];
    }
    let mut p = Parser {
        text: source.chars().collect(),
        i: 0,
        runs: Vec::new(),
    };
    p.group(
        Style {
            script: 0,
            bold: false,
            italic: false,
        },
        false,
        0,
    );
    p.runs
}

pub fn plain(source: &str, interpreter: &str) -> String {
    parse(source, interpreter)
        .iter()
        .map(|r| r.text.as_str())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_and_symbols() {
        let r = parse("x^{2} \\alpha_1 \\bfB", "tex");
        assert_eq!(r[0].text, "x");
        assert_eq!((r[1].text.as_str(), r[1].script), ("2", 1));
        assert_eq!(r[2].text, " α");
        assert_eq!((r[3].text.as_str(), r[3].script), ("1", -1));
        assert_eq!(plain("\\unknown{a}\\color{red}b", "tex"), "\\unknownab");
        assert_eq!(parse("a_b", "none")[0].text, "a_b");
        assert_eq!(plain("trailing\\", "tex"), "trailing\\");
        assert_eq!(plain("^", "tex"), "^");
    }
}
