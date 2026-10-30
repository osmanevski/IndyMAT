//! Embedded vector fonts (see `fonts/README.md`): subsets of Liberation Sans
//! for sans text and of JetBrains Mono for ticks, both SIL OFL 1.1. No system
//! font is ever read. Oblique is a sheared upright.

use crate::device::Path;

static SANS: &[u8] = include_bytes!("../fonts/sans-regular.bin");
static SANS_BOLD: &[u8] = include_bytes!("../fonts/sans-bold.bin");
static MONO: &[u8] = include_bytes!("../fonts/mono-regular.bin");

const HEADER: usize = 12;
const RECORD: usize = 9;
const SHEAR: f64 = 0.2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    Sans,
    SansBold,
    Mono,
}

fn data(face: Face) -> &'static [u8] {
    match face {
        Face::Sans => SANS,
        Face::SansBold => SANS_BOLD,
        Face::Mono => MONO,
    }
}

fn be16(d: &[u8], o: usize) -> u16 {
    u16::from_be_bytes([d[o], d[o + 1]])
}

fn upem(d: &[u8]) -> f64 {
    be16(d, 4) as f64
}

/// Ascent and descent (both positive) per unit of font size.
pub fn metrics(face: Face) -> (f64, f64) {
    let d = data(face);
    let u = upem(d);
    (
        be16(d, 6) as i16 as f64 / u,
        -(be16(d, 8) as i16 as f64) / u,
    )
}

fn find(d: &[u8], cp: u32) -> Option<usize> {
    let count = be16(d, 10) as usize;
    let (mut lo, mut hi) = (0usize, count);
    while lo < hi {
        let mid = (lo + hi) / 2;
        let o = HEADER + mid * RECORD;
        let c = (d[o] as u32) << 16 | (d[o + 1] as u32) << 8 | d[o + 2] as u32;
        match c.cmp(&cp) {
            std::cmp::Ordering::Equal => return Some(o),
            std::cmp::Ordering::Less => lo = mid + 1,
            std::cmp::Ordering::Greater => hi = mid,
        }
    }
    None
}

/// Close look-alikes for symbols neither font carries.
fn substitute(c: char) -> char {
    match c {
        '⋅' => '·',
        '∗' => '*',
        '∶' => ':',
        'ϕ' => 'φ',
        _ => c,
    }
}

/// The font and record of a character: the requested face, then the other
/// fonts. `None` draws an empty box.
fn resolve(face: Face, c: char) -> Option<(&'static [u8], usize)> {
    let c = substitute(c) as u32;
    for f in [face, Face::Sans, Face::Mono] {
        let d = data(f);
        if let Some(o) = find(d, c) {
            return Some((d, o));
        }
    }
    None
}

/// Whether a character has a real glyph (not the empty box).
pub fn has_glyph(face: Face, c: char) -> bool {
    resolve(face, c).is_some()
}

/// Advance of a character per unit of font size.
pub fn advance(face: Face, c: char) -> f64 {
    match resolve(face, c) {
        Some((d, o)) => be16(d, o + 3) as f64 / upem(d),
        None => 0.6,
    }
}

pub fn measure(face: Face, text: &str, size: f64) -> f64 {
    text.chars().map(|c| advance(face, c)).sum::<f64>() * size
}

struct Reader<'a> {
    d: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn varint(&mut self) -> u32 {
        let (mut v, mut shift) = (0u32, 0);
        while let Some(&b) = self.d.get(self.at) {
            self.at += 1;
            v |= ((b & 0x7F) as u32) << shift;
            if b & 0x80 == 0 {
                break;
            }
            shift += 7;
        }
        v
    }
}

fn unzigzag(v: u32) -> i32 {
    ((v >> 1) as i32) ^ -((v & 1) as i32)
}

/// Text-space to device-space placement of a glyph.
#[derive(Debug, Clone, Copy)]
pub struct Place {
    /// Pen position in logical pixels.
    pub x: f64,
    pub y: f64,
    /// Rotation of the text direction: `cos`, `sin` (y down).
    pub cos: f64,
    pub sin: f64,
    pub size: f64,
    pub oblique: bool,
}

/// Appends a glyph's flattened outline to `path` (logical pixels).
pub fn glyph(path: &mut Path, face: Face, c: char, place: &Place, dpr: f64) {
    let Some((d, record)) = resolve(face, c) else {
        tofu(path, place);
        return;
    };
    let u = upem(d);
    let count = be16(d, 10) as usize;
    let offset =
        u32::from_be_bytes([d[record + 5], d[record + 6], d[record + 7], d[record + 8]]) as usize;
    let stream = HEADER + count * RECORD + offset;
    let mut r = Reader { d, at: stream };
    let scale = place.size / u;
    let steps = ((place.size * dpr / 5.0).ceil() as usize).clamp(2, 10);
    let to_device = |gx: f64, gy: f64| -> [f64; 2] {
        let sx = gx * scale
            + if place.oblique {
                gy * scale * SHEAR
            } else {
                0.0
            };
        let sy = -gy * scale;
        [
            place.x + sx * place.cos - sy * place.sin,
            place.y + sx * place.sin + sy * place.cos,
        ]
    };
    let contours = r.varint();
    for _ in 0..contours {
        let n = r.varint() as usize;
        let mut pts: Vec<(f64, f64, bool)> = Vec::with_capacity(n);
        let (mut x, mut y) = (0i32, 0i32);
        for _ in 0..n {
            let a = r.varint();
            let b = r.varint();
            x += unzigzag(a >> 1);
            y += unzigzag(b);
            pts.push((x as f64, y as f64, a & 1 == 1));
        }
        if n < 2 {
            continue;
        }
        // The first on-curve point (or the midpoint of two controls).
        let (start, first) = match pts.iter().position(|p| p.2) {
            Some(i) => ((pts[i].0, pts[i].1), i + 1),
            None => (
                (
                    (pts[0].0 + pts[n - 1].0) / 2.0,
                    (pts[0].1 + pts[n - 1].1) / 2.0,
                ),
                0,
            ),
        };
        let p0 = to_device(start.0, start.1);
        path.move_to(p0[0], p0[1]);
        let mut cur = start;
        let mut ctrl: Option<(f64, f64)> = None;
        let emit_quad = |path: &mut Path, a: (f64, f64), c: (f64, f64), b: (f64, f64)| {
            for k in 1..=steps {
                let t = k as f64 / steps as f64;
                let m = 1.0 - t;
                let gx = m * m * a.0 + 2.0 * m * t * c.0 + t * t * b.0;
                let gy = m * m * a.1 + 2.0 * m * t * c.1 + t * t * b.1;
                let p = to_device(gx, gy);
                path.line_to(p[0], p[1]);
            }
        };
        for k in 0..n {
            let (px, py, on) = pts[(first + k) % n];
            if on {
                if let Some(c) = ctrl.take() {
                    emit_quad(path, cur, c, (px, py));
                } else {
                    let p = to_device(px, py);
                    path.line_to(p[0], p[1]);
                }
                cur = (px, py);
            } else {
                if let Some(c) = ctrl {
                    let mid = ((c.0 + px) / 2.0, (c.1 + py) / 2.0);
                    emit_quad(path, cur, c, mid);
                    cur = mid;
                }
                ctrl = Some((px, py));
            }
        }
        if let Some(c) = ctrl {
            emit_quad(path, cur, c, start);
        }
        path.close();
    }
}

/// An empty box for a character no embedded font has: visibly not a letter.
fn tofu(path: &mut Path, place: &Place) {
    let s = place.size;
    let at = |gx: f64, gy: f64| -> [f64; 2] {
        [
            place.x + gx * s * place.cos + gy * s * place.sin,
            place.y + gx * s * place.sin - gy * s * place.cos,
        ]
    };
    let outer = [at(0.08, 0.0), at(0.52, 0.0), at(0.52, 0.72), at(0.08, 0.72)];
    let inner = [
        at(0.14, 0.06),
        at(0.14, 0.66),
        at(0.46, 0.66),
        at(0.46, 0.06),
    ];
    path.polygon(&outer);
    path.polygon(&inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turkish_and_greek_glyphs_exist_in_every_face() {
        for face in [Face::Sans, Face::SansBold, Face::Mono] {
            for c in "çğıİöşüÇĞÖŞÜαβγθμωπΣΩ±×°≤≥∞→".chars() {
                assert!(has_glyph(face, c), "{c}");
                let mut p = Path::new();
                glyph(
                    &mut p,
                    face,
                    c,
                    &Place {
                        x: 0.0,
                        y: 0.0,
                        cos: 1.0,
                        sin: 0.0,
                        size: 10.0,
                        oblique: false,
                    },
                    1.0,
                );
                assert!(!p.is_empty(), "{c}");
            }
        }
        assert!((advance(Face::Mono, 'x') - 0.6).abs() < 1e-9);
        // A space has an advance and no outline.
        let mut p = Path::new();
        glyph(
            &mut p,
            Face::Sans,
            ' ',
            &Place {
                x: 0.0,
                y: 0.0,
                cos: 1.0,
                sin: 0.0,
                size: 10.0,
                oblique: false,
            },
            1.0,
        );
        assert!(p.is_empty());
    }
}
