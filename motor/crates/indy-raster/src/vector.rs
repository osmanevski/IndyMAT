//! Vector devices: SVG and PDF, from the same painter calls as the raster
//! device. Text is written as filled outlines, so no font is needed to view
//! the files. A 3-D scene arrives as an embedded image: its hidden-surface
//! removal is a depth buffer, not painter's vectors.

use crate::device::{Bitmap, Device, Path, Rgb, mean_color};
use crate::encode;
use indy_value::deflate::zlib_compress;
use std::fmt::Write as _;

fn fmt(v: f64) -> String {
    let s = format!("{v:.3}");
    let t = s.trim_end_matches('0').trim_end_matches('.');
    if t.is_empty() || t == "-0" {
        "0".to_string()
    } else {
        t.to_string()
    }
}

fn hex(c: Rgb) -> String {
    let b = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len() / 3 * 4 + 4);
    for chunk in data.chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

pub struct Svg {
    width: f64,
    height: f64,
    body: String,
    clips: usize,
    clip_id: Option<usize>,
    defs: String,
    region: Option<[f64; 4]>,
}

impl Svg {
    pub fn new(width: f64, height: f64) -> Svg {
        Svg {
            width,
            height,
            body: String::new(),
            clips: 0,
            clip_id: None,
            defs: String::new(),
            region: None,
        }
    }

    /// Show only `[x, y, width, height]` of the drawing.
    pub fn show_region(&mut self, region: Option<[f64; 4]>) {
        self.region = region;
    }

    fn path_data(path: &Path) -> String {
        let mut d = String::new();
        for k in 0..path.sub_count() {
            let (pts, closed) = path.sub(k);
            for (i, p) in pts.iter().enumerate() {
                let _ = write!(
                    d,
                    "{}{} {}",
                    if i == 0 { 'M' } else { 'L' },
                    fmt(p[0]),
                    fmt(p[1])
                );
            }
            if closed {
                d.push('Z');
            }
        }
        d
    }

    fn clip_attr(&self) -> String {
        self.clip_id
            .map_or(String::new(), |id| format!(" clip-path=\"url(#c{id})\""))
    }

    pub fn finish(self) -> String {
        let [x, y, w, h] = self.region.unwrap_or([0.0, 0.0, self.width, self.height]);
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{w}\" height=\"{h}\" viewBox=\"{x} {y} {w} {h}\">\n<defs>{defs}</defs>\n{body}</svg>\n",
            x = fmt(x),
            y = fmt(y),
            w = fmt(w),
            h = fmt(h),
            defs = self.defs,
            body = self.body
        )
    }
}

impl Device for Svg {
    fn dpr(&self) -> f64 {
        2.0
    }

    fn fill(&mut self, path: &Path, even_odd: bool, color: Rgb, alpha: f64) {
        if path.is_empty() {
            return;
        }
        let rule = if even_odd {
            " fill-rule=\"evenodd\""
        } else {
            ""
        };
        let opacity = if alpha < 1.0 {
            format!(" fill-opacity=\"{}\"", fmt(alpha))
        } else {
            String::new()
        };
        let _ = writeln!(
            self.body,
            "<path d=\"{}\" fill=\"{}\"{rule}{opacity}{}/>",
            Self::path_data(path),
            hex(color),
            self.clip_attr()
        );
    }

    fn fill_text(&mut self, path: &Path, _size: f64, color: Rgb) {
        self.fill(path, false, color, 1.0);
    }

    fn stroke(&mut self, path: &Path, width: f64, dash: &[f64], color: Rgb, alpha: f64) {
        if path.is_empty() {
            return;
        }
        let dashes = if dash.is_empty() {
            String::new()
        } else {
            format!(
                " stroke-dasharray=\"{}\"",
                dash.iter().map(|d| fmt(*d)).collect::<Vec<_>>().join(" ")
            )
        };
        let opacity = if alpha < 1.0 {
            format!(" stroke-opacity=\"{}\"", fmt(alpha))
        } else {
            String::new()
        };
        let _ = writeln!(
            self.body,
            "<path d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-miterlimit=\"10\"{dashes}{opacity}{}/>",
            Self::path_data(path),
            hex(color),
            fmt(width),
            self.clip_attr()
        );
    }

    fn fill_rect(&mut self, x: f64, y: f64, w: f64, h: f64, color: Rgb, alpha: f64) {
        let opacity = if alpha < 1.0 {
            format!(" fill-opacity=\"{}\"", fmt(alpha))
        } else {
            String::new()
        };
        let _ = writeln!(
            self.body,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"{opacity}{}/>",
            fmt(x),
            fmt(y),
            fmt(w),
            fmt(h),
            hex(color),
            self.clip_attr()
        );
    }

    fn image(&mut self, b: &Bitmap, x0: f64, y0: f64, x1: f64, y1: f64) {
        let png = encode::png(b.width, b.height, &b.rgb, None);
        let (w, h) = ((x1 - x0).abs(), (y1 - y0).abs());
        let (sx, sy) = (if x1 < x0 { -1 } else { 1 }, if y1 < y0 { -1 } else { 1 });
        let _ = writeln!(
            self.body,
            "<image transform=\"translate({} {}) scale({sx} {sy})\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\" style=\"image-rendering:pixelated\"{} xlink:href=\"data:image/png;base64,{}\"/>",
            fmt(x0),
            fmt(y0),
            fmt(w),
            fmt(h),
            self.clip_attr(),
            base64(&png)
        );
    }

    fn clip(&mut self, rect: Option<[f64; 4]>) {
        self.clip_id = rect.map(|[x, y, w, h]| {
            let id = self.clips;
            self.clips += 1;
            let _ = write!(self.defs, "<clipPath id=\"c{id}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath>", fmt(x), fmt(y), fmt(w), fmt(h));
            id
        });
    }

    fn gouraud(&mut self, tri: &[[f64; 2]; 3], colors: &[Rgb; 3], alpha: f64) {
        let mut p = Path::new();
        p.polygon(tri);
        self.fill(&p, false, mean_color(colors), alpha);
    }
}

/// One-page PDF. Logical pixels map to points at 72/96, y up; alpha goes
/// through `ExtGState`s, images through flate `XObject`s.
pub struct Pdf {
    width: f64,
    height: f64,
    ops: String,
    images: Vec<(usize, usize, Vec<u8>)>,
    alphas: Vec<(u32, bool)>,
    clipped: bool,
    region: Option<[f64; 4]>,
}

impl Pdf {
    pub fn new(width: f64, height: f64) -> Pdf {
        Pdf {
            width,
            height,
            ops: String::from("q\n"),
            images: Vec::new(),
            alphas: Vec::new(),
            clipped: false,
            region: None,
        }
    }

    /// Make the page only `[x, y, width, height]` of the drawing.
    pub fn show_region(&mut self, region: Option<[f64; 4]>) {
        self.region = region;
    }

    fn y(&self, y: f64) -> f64 {
        (self.height - y) * 0.75
    }

    fn path_ops(&mut self, path: &Path) {
        for k in 0..path.sub_count() {
            let (pts, closed) = path.sub(k);
            for (i, p) in pts.iter().enumerate() {
                let (x, y) = (fmt(p[0] * 0.75), fmt(self.y(p[1])));
                let _ = writeln!(self.ops, "{x} {y} {}", if i == 0 { 'm' } else { 'l' });
            }
            if closed {
                self.ops.push_str("h\n");
            }
        }
    }

    fn set_alpha(&mut self, alpha: f64, stroke: bool) {
        let key = ((alpha.clamp(0.0, 1.0) * 1000.0).round() as u32, stroke);
        let index = match self.alphas.iter().position(|a| *a == key) {
            Some(i) => i,
            None => {
                self.alphas.push(key);
                self.alphas.len() - 1
            }
        };
        let _ = writeln!(self.ops, "/G{index} gs");
    }

    pub fn finish(mut self) -> Vec<u8> {
        self.ops.push_str("Q\n");
        if self.clipped {
            self.ops.push_str("Q\n");
        }
        let content = zlib_compress(self.ops.as_bytes());
        // PDF's y axis runs up from the bottom of the page.
        let page = match self.region {
            Some([x, y, w, h]) => [x, self.height - y - h, x + w, self.height - y],
            None => [0.0, 0.0, self.width, self.height],
        };
        let mut objects: Vec<Vec<u8>> = Vec::new();
        // 1 catalog, 2 pages, 3 page, 4 content, then images.
        let first_image = 5;
        let mut xobjects = String::new();
        for i in 0..self.images.len() {
            let _ = write!(xobjects, "/I{i} {} 0 R ", first_image + i);
        }
        let mut gs = String::new();
        for (i, (a, stroke)) in self.alphas.iter().enumerate() {
            let key = if *stroke { "CA" } else { "ca" };
            let _ = write!(gs, "/G{i} << /{key} {} >> ", fmt(*a as f64 / 1000.0));
        }
        objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
        objects.push(b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec());
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [{} {} {} {}] /Contents 4 0 R /Resources << /XObject << {xobjects}>> /ExtGState << {gs}>> >> >>",
                fmt(page[0] * 0.75),
                fmt(page[1] * 0.75),
                fmt(page[2] * 0.75),
                fmt(page[3] * 0.75)
            )
            .into_bytes(),
        );
        let mut stream = format!(
            "<< /Length {} /Filter /FlateDecode >>\nstream\n",
            content.len()
        )
        .into_bytes();
        stream.extend_from_slice(&content);
        stream.extend_from_slice(b"\nendstream");
        objects.push(stream);
        for (w, h, rgb) in &self.images {
            let data = zlib_compress(rgb);
            let mut o = format!(
                "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
                data.len()
            )
            .into_bytes();
            o.extend_from_slice(&data);
            o.extend_from_slice(b"\nendstream");
            objects.push(o);
        }
        let mut out = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
        let mut offsets = Vec::with_capacity(objects.len());
        for (i, o) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            out.extend_from_slice(o);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref = out.len();
        out.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
        );
        for o in &offsets {
            out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        out
    }
}

impl Device for Pdf {
    fn dpr(&self) -> f64 {
        2.0
    }

    fn fill(&mut self, path: &Path, even_odd: bool, color: Rgb, alpha: f64) {
        if path.is_empty() {
            return;
        }
        self.ops.push_str("q\n");
        if alpha < 1.0 {
            self.set_alpha(alpha, false);
        }
        let _ = writeln!(
            self.ops,
            "{} {} {} rg",
            fmt(color[0]),
            fmt(color[1]),
            fmt(color[2])
        );
        self.path_ops(path);
        self.ops
            .push_str(if even_odd { "f*\nQ\n" } else { "f\nQ\n" });
    }

    fn fill_text(&mut self, path: &Path, _size: f64, color: Rgb) {
        self.fill(path, false, color, 1.0);
    }

    fn stroke(&mut self, path: &Path, width: f64, dash: &[f64], color: Rgb, alpha: f64) {
        if path.is_empty() {
            return;
        }
        self.ops.push_str("q\n");
        if alpha < 1.0 {
            self.set_alpha(alpha, true);
        }
        let _ = writeln!(
            self.ops,
            "{} {} {} RG {} w",
            fmt(color[0]),
            fmt(color[1]),
            fmt(color[2]),
            fmt(width * 0.75)
        );
        if !dash.is_empty() {
            let list = dash
                .iter()
                .map(|d| fmt(d * 0.75))
                .collect::<Vec<_>>()
                .join(" ");
            let _ = writeln!(self.ops, "[{list}] 0 d");
        }
        self.path_ops(path);
        self.ops.push_str("S\nQ\n");
    }

    fn fill_rect(&mut self, x: f64, y: f64, w: f64, h: f64, color: Rgb, alpha: f64) {
        let mut p = Path::new();
        p.rect(x, y, w, h);
        self.fill(&p, false, color, alpha);
    }

    fn image(&mut self, b: &Bitmap, x0: f64, y0: f64, x1: f64, y1: f64) {
        let index = self.images.len();
        self.images.push((b.width, b.height, b.rgb.clone()));
        // The unit square's top edge is the image's first row.
        let (a, d) = ((x1 - x0) * 0.75, (y1 - y0) * 0.75);
        let _ = writeln!(
            self.ops,
            "q {} 0 0 {} {} {} cm /I{index} Do Q",
            fmt(a),
            fmt(d),
            fmt(x0 * 0.75),
            fmt(self.y(y1))
        );
    }

    fn clip(&mut self, rect: Option<[f64; 4]>) {
        if self.clipped {
            self.ops.push_str("Q\n");
            self.clipped = false;
        }
        if let Some([x, y, w, h]) = rect {
            let _ = writeln!(
                self.ops,
                "q {} {} {} {} re W n",
                fmt(x * 0.75),
                fmt(self.y(y + h)),
                fmt(w * 0.75),
                fmt(h * 0.75)
            );
            self.clipped = true;
        }
    }

    fn gouraud(&mut self, tri: &[[f64; 2]; 3], colors: &[Rgb; 3], alpha: f64) {
        let mut p = Path::new();
        p.polygon(tri);
        self.fill(&p, false, mean_color(colors), alpha);
    }
}
