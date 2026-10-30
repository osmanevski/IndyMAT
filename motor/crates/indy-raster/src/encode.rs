//! Image file encoders: PNG (own filters, `indy_value::deflate`), baseline
//! JPEG, uncompressed TIFF. All take RGB bytes, top row first.

use indy_value::deflate::{crc32, zlib_compress};

fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// 8-bit RGB PNG. Each row takes the filter (None, Sub, Up) with the least
/// absolute sum; plots are flat, so rows compress to almost nothing.
pub fn png(width: usize, height: usize, rgb: &[u8], dpi: Option<f64>) -> Vec<u8> {
    let stride = width * 3;
    let mut raw = Vec::with_capacity((stride + 1) * height);
    let mut sub = vec![0u8; stride];
    let mut up = vec![0u8; stride];
    let zero = vec![0u8; stride];
    for y in 0..height {
        let row = &rgb[y * stride..(y + 1) * stride];
        let above = if y == 0 {
            &zero[..]
        } else {
            &rgb[(y - 1) * stride..y * stride]
        };
        let (mut s_none, mut s_sub, mut s_up) = (0u64, 0u64, 0u64);
        for x in 0..stride {
            let left = if x >= 3 { row[x - 3] } else { 0 };
            sub[x] = row[x].wrapping_sub(left);
            up[x] = row[x].wrapping_sub(above[x]);
            s_none += (row[x] as i8).unsigned_abs() as u64;
            s_sub += (sub[x] as i8).unsigned_abs() as u64;
            s_up += (up[x] as i8).unsigned_abs() as u64;
        }
        if s_up <= s_sub && s_up <= s_none {
            raw.push(2);
            raw.extend_from_slice(&up);
        } else if s_sub <= s_none {
            raw.push(1);
            raw.extend_from_slice(&sub);
        } else {
            raw.push(0);
            raw.extend_from_slice(row);
        }
    }
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&(width as u32).to_be_bytes());
    ihdr.extend_from_slice(&(height as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    png_chunk(&mut out, b"IHDR", &ihdr);
    if let Some(dpi) = dpi {
        let per_metre = (dpi / 0.0254).round() as u32;
        let mut phys = Vec::with_capacity(9);
        phys.extend_from_slice(&per_metre.to_be_bytes());
        phys.extend_from_slice(&per_metre.to_be_bytes());
        phys.push(1);
        png_chunk(&mut out, b"pHYs", &phys);
    }
    png_chunk(&mut out, b"IDAT", &zlib_compress(&raw));
    png_chunk(&mut out, b"IEND", &[]);
    out
}

/// Baseline, 8-bit, uncompressed-strip TIFF.
pub fn tiff(width: usize, height: usize, rgb: &[u8], dpi: f64) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(rgb.len() + 256);
    out.extend_from_slice(b"II");
    out.extend_from_slice(&42u16.to_le_bytes());
    let entries: u32 = 12;
    let ifd = 8u32;
    let after = ifd + 2 + entries * 12 + 4;
    let (bits_at, xres_at, yres_at) = (after, after + 6, after + 14);
    let data_at = after + 22;
    out.extend_from_slice(&ifd.to_le_bytes());
    out.extend_from_slice(&(entries as u16).to_le_bytes());
    let mut entry = |tag: u16, kind: u16, count: u32, value: u32| {
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&kind.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&value.to_le_bytes());
    };
    entry(256, 4, 1, width as u32);
    entry(257, 4, 1, height as u32);
    entry(258, 3, 3, bits_at);
    entry(259, 3, 1, 1);
    entry(262, 3, 1, 2);
    entry(273, 4, 1, data_at);
    entry(277, 3, 1, 3);
    entry(278, 4, 1, height as u32);
    entry(279, 4, 1, rgb.len() as u32);
    entry(282, 5, 1, xres_at);
    entry(283, 5, 1, yres_at);
    entry(296, 3, 1, 2);
    out.extend_from_slice(&0u32.to_le_bytes());
    for _ in 0..3 {
        out.extend_from_slice(&8u16.to_le_bytes());
    }
    for _ in 0..2 {
        out.extend_from_slice(&(dpi.round().max(1.0) as u32).to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
    }
    out.extend_from_slice(rgb);
    out
}

// ---- JPEG (ITU T.81 baseline; the tables are the standard's Annex K ones) ----

const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];
const LUMA_Q: [u16; 64] = [
    16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69, 56,
    14, 17, 22, 29, 51, 87, 80, 62, 18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55, 64, 81, 104, 113,
    92, 49, 64, 78, 87, 103, 121, 120, 101, 72, 92, 95, 98, 112, 100, 103, 99,
];
const CHROMA_Q: [u16; 64] = [
    17, 18, 24, 47, 99, 99, 99, 99, 18, 21, 26, 66, 99, 99, 99, 99, 24, 26, 56, 99, 99, 99, 99, 99,
    47, 66, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
];
const DC_LUMA_BITS: [u8; 16] = [0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0];
const DC_CHROMA_BITS: [u8; 16] = [0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0];
const DC_VALUES: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
const AC_LUMA_BITS: [u8; 16] = [0, 2, 1, 3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 0x7D];
const AC_CHROMA_BITS: [u8; 16] = [0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 0x77];
const AC_LUMA_VALUES: [u8; 162] = [
    0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07,
    0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xA1, 0x08, 0x23, 0x42, 0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0,
    0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0A, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x25, 0x26, 0x27, 0x28,
    0x29, 0x2A, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49,
    0x4A, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69,
    0x6A, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89,
    0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7,
    0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3, 0xC4, 0xC5,
    0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2,
    0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8,
    0xF9, 0xFA,
];
const AC_CHROMA_VALUES: [u8; 162] = [
    0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x31, 0x06, 0x12, 0x41, 0x51, 0x07, 0x61, 0x71,
    0x13, 0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91, 0xA1, 0xB1, 0xC1, 0x09, 0x23, 0x33, 0x52, 0xF0,
    0x15, 0x62, 0x72, 0xD1, 0x0A, 0x16, 0x24, 0x34, 0xE1, 0x25, 0xF1, 0x17, 0x18, 0x19, 0x1A, 0x26,
    0x27, 0x28, 0x29, 0x2A, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48,
    0x49, 0x4A, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68,
    0x69, 0x6A, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87,
    0x88, 0x89, 0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5,
    0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3,
    0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA,
    0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8,
    0xF9, 0xFA,
];

/// Code and length per symbol of a JPEG Huffman table.
fn huffman(bits: &[u8; 16], values: &[u8]) -> [(u16, u8); 256] {
    let mut table = [(0u16, 0u8); 256];
    let (mut code, mut k) = (0u16, 0usize);
    for (i, &count) in bits.iter().enumerate() {
        for _ in 0..count {
            table[values[k] as usize] = (code, i as u8 + 1);
            code += 1;
            k += 1;
        }
        code <<= 1;
    }
    table
}

struct JpegBits {
    out: Vec<u8>,
    acc: u32,
    n: u32,
}

impl JpegBits {
    fn put(&mut self, code: u16, len: u8) {
        for i in (0..len).rev() {
            self.acc = self.acc << 1 | ((code >> i) & 1) as u32;
            self.n += 1;
            if self.n == 8 {
                self.out.push(self.acc as u8);
                if self.acc as u8 == 0xFF {
                    self.out.push(0);
                }
                self.acc = 0;
                self.n = 0;
            }
        }
    }

    fn flush(&mut self) {
        while self.n != 0 {
            self.put(1, 1);
        }
    }
}

fn magnitude(v: i32) -> (u8, u16) {
    let size = 32 - v.unsigned_abs().leading_zeros();
    let bits = if v < 0 { v - 1 } else { v } & ((1i32 << size) - 1);
    (size as u8, bits as u16)
}

fn fdct(block: &[f32; 64], q: &[f32; 64], out: &mut [i32; 64]) {
    let mut cosines = [[0.0f32; 8]; 8];
    for (x, row) in cosines.iter_mut().enumerate() {
        for (u, c) in row.iter_mut().enumerate() {
            *c = ((2 * x + 1) as f32 * u as f32 * std::f32::consts::PI / 16.0).cos();
        }
    }
    let mut temp = [0.0f32; 64];
    for y in 0..8 {
        for u in 0..8 {
            let mut s = 0.0;
            for x in 0..8 {
                s += block[y * 8 + x] * cosines[x][u];
            }
            temp[y * 8 + u] = s;
        }
    }
    for u in 0..8 {
        for v in 0..8 {
            let mut s = 0.0;
            for y in 0..8 {
                s += temp[y * 8 + u] * cosines[y][v];
            }
            let cu = if u == 0 {
                std::f32::consts::FRAC_1_SQRT_2
            } else {
                1.0
            };
            let cv = if v == 0 {
                std::f32::consts::FRAC_1_SQRT_2
            } else {
                1.0
            };
            out[v * 8 + u] = (s * cu * cv / 4.0 / q[v * 8 + u]).round() as i32;
        }
    }
}

/// Baseline 4:4:4 JPEG (no chroma subsampling: thin coloured lines stay clean).
pub fn jpeg(width: usize, height: usize, rgb: &[u8], quality: u32, dpi: f64) -> Vec<u8> {
    let q = quality.clamp(1, 100);
    let scale = if q < 50 { 5000 / q } else { 200 - 2 * q };
    let table = |base: &[u16; 64]| -> [u8; 64] {
        let mut t = [0u8; 64];
        for (slot, b) in t.iter_mut().zip(base) {
            *slot = ((*b as u32 * scale + 50) / 100).clamp(1, 255) as u8;
        }
        t
    };
    let (ql, qc) = (table(&LUMA_Q), table(&CHROMA_Q));
    let mut out = vec![
        0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0, 1, 1, 1,
    ];
    let d = (dpi.round().clamp(1.0, 65535.0)) as u16;
    out.extend_from_slice(&d.to_be_bytes());
    out.extend_from_slice(&d.to_be_bytes());
    out.extend_from_slice(&[0, 0]);
    for (id, t) in [(0u8, &ql), (1u8, &qc)] {
        out.extend_from_slice(&[0xFF, 0xDB, 0, 67, id]);
        for &z in &ZIGZAG {
            out.push(t[z]);
        }
    }
    out.extend_from_slice(&[0xFF, 0xC0, 0, 17, 8]);
    out.extend_from_slice(&(height as u16).to_be_bytes());
    out.extend_from_slice(&(width as u16).to_be_bytes());
    out.extend_from_slice(&[3, 1, 0x11, 0, 2, 0x11, 1, 3, 0x11, 1]);
    for (class, bits, values) in [
        (0x00u8, &DC_LUMA_BITS, &DC_VALUES[..]),
        (0x10, &AC_LUMA_BITS, &AC_LUMA_VALUES[..]),
        (0x01, &DC_CHROMA_BITS, &DC_VALUES[..]),
        (0x11, &AC_CHROMA_BITS, &AC_CHROMA_VALUES[..]),
    ] {
        let len = 19 + values.len();
        out.extend_from_slice(&[0xFF, 0xC4]);
        out.extend_from_slice(&(len as u16).to_be_bytes());
        out.push(class);
        out.extend_from_slice(bits);
        out.extend_from_slice(values);
    }
    out.extend_from_slice(&[0xFF, 0xDA, 0, 12, 3, 1, 0x00, 2, 0x11, 3, 0x11, 0, 63, 0]);
    let dc = [
        huffman(&DC_LUMA_BITS, &DC_VALUES),
        huffman(&DC_CHROMA_BITS, &DC_VALUES),
    ];
    let ac = [
        huffman(&AC_LUMA_BITS, &AC_LUMA_VALUES),
        huffman(&AC_CHROMA_BITS, &AC_CHROMA_VALUES),
    ];
    let floats = |t: &[u8; 64]| -> [f32; 64] {
        let mut f = [0.0; 64];
        for (slot, v) in f.iter_mut().zip(t) {
            *slot = *v as f32;
        }
        f
    };
    let (fl, fc) = (floats(&ql), floats(&qc));
    let mut w = JpegBits { out, acc: 0, n: 0 };
    let mut pred = [0i32; 3];
    let mut coef = [0i32; 64];
    for my in (0..height).step_by(8) {
        for mx in (0..width).step_by(8) {
            let mut blocks = [[0.0f32; 64]; 3];
            for y in 0..8 {
                for x in 0..8 {
                    let py = (my + y).min(height - 1);
                    let px = (mx + x).min(width - 1);
                    let i = (py * width + px) * 3;
                    let (r, g, b) = (rgb[i] as f32, rgb[i + 1] as f32, rgb[i + 2] as f32);
                    blocks[0][y * 8 + x] = 0.299 * r + 0.587 * g + 0.114 * b - 128.0;
                    blocks[1][y * 8 + x] = -0.168736 * r - 0.331264 * g + 0.5 * b;
                    blocks[2][y * 8 + x] = 0.5 * r - 0.418688 * g - 0.081312 * b;
                }
            }
            for c in 0..3 {
                let t = usize::from(c != 0);
                fdct(&blocks[c], if c == 0 { &fl } else { &fc }, &mut coef);
                let diff = coef[0] - pred[c];
                pred[c] = coef[0];
                let (size, bits) = magnitude(diff);
                let (code, len) = dc[t][size as usize];
                w.put(code, len);
                if size > 0 {
                    w.put(bits, size);
                }
                let mut run = 0;
                for &z in &ZIGZAG[1..] {
                    let v = coef[z];
                    if v == 0 {
                        run += 1;
                        continue;
                    }
                    while run >= 16 {
                        let (code, len) = ac[t][0xF0];
                        w.put(code, len);
                        run -= 16;
                    }
                    let (size, bits) = magnitude(v);
                    let (code, len) = ac[t][(run << 4) | size as usize];
                    w.put(code, len);
                    w.put(bits, size);
                    run = 0;
                }
                if run > 0 {
                    let (code, len) = ac[t][0];
                    w.put(code, len);
                }
            }
        }
    }
    w.flush();
    w.out.extend_from_slice(&[0xFF, 0xD9]);
    w.out
}
