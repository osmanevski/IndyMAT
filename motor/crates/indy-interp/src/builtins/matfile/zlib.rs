//! zlib streams (RFC 1950) over DEFLATE (RFC 1951), written from the RFCs.
//!
//! - [`inflate`] decodes stored, fixed-Huffman and dynamic-Huffman blocks and
//!   checks the Adler-32 trailer. Output is capped so a hostile stream cannot
//!   exhaust memory.
//! - [`deflate`] compresses with LZ77 (hash chains over a 32 KiB window) and
//!   the fixed Huffman code: a valid zlib stream every reader accepts, a
//!   little larger than a dynamic-Huffman encoder would make.

/// Largest inflated stream accepted (one MAT-file variable).
pub const MAX_OUTPUT: usize = 1 << 31;

pub fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65521;
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &x in chunk {
            a += x as u32;
            b += a;
        }
        a %= MOD;
        b %= MOD;
    }
    (b << 16) | a
}

// ---------------------------------------------------------------------------
// Tables shared by both directions (RFC 1951 section 3.2.5)
// ---------------------------------------------------------------------------

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
/// Order of the code-length code lengths in a dynamic block header.
const CL_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

fn fixed_lit_lengths() -> [u8; 288] {
    let mut l = [0u8; 288];
    for (i, v) in l.iter_mut().enumerate() {
        *v = match i {
            0..=143 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => 8,
        };
    }
    l
}

// ---------------------------------------------------------------------------
// Inflate
// ---------------------------------------------------------------------------

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    bit: u32,
    nbits: u32,
}

impl Bits<'_> {
    fn need(&mut self, n: u32) -> Result<(), String> {
        while self.nbits < n {
            let Some(&b) = self.data.get(self.pos) else {
                return Err("the compressed data ends early".into());
            };
            self.pos += 1;
            self.bit |= (b as u32) << self.nbits;
            self.nbits += 8;
        }
        Ok(())
    }
    fn take(&mut self, n: u32) -> Result<u32, String> {
        if n == 0 {
            return Ok(0);
        }
        self.need(n)?;
        let v = self.bit & ((1u32 << n) - 1);
        self.bit >>= n;
        self.nbits -= n;
        Ok(v)
    }
    fn align(&mut self) {
        self.bit = 0;
        self.nbits = 0;
    }
}

/// Canonical Huffman decoder: code counts per length and symbols in code order.
struct Huffman {
    count: [u16; 16],
    symbol: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Result<Huffman, String> {
        let mut count = [0u16; 16];
        for &l in lengths {
            count[l as usize] += 1;
        }
        count[0] = 0;
        // Over-subscribed code sets are invalid; incomplete ones are allowed
        // (a single distance code is common).
        let mut left: i32 = 1;
        for &c in &count[1..] {
            left <<= 1;
            left -= c as i32;
            if left < 0 {
                return Err("invalid Huffman code lengths".into());
            }
        }
        let mut offs = [0u16; 16];
        for len in 1..15 {
            offs[len + 1] = offs[len] + count[len];
        }
        let mut symbol = vec![0u16; lengths.len()];
        for (s, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbol[offs[l as usize] as usize] = s as u16;
                offs[l as usize] += 1;
            }
        }
        Ok(Huffman { count, symbol })
    }

    fn decode(&self, b: &mut Bits) -> Result<u16, String> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= b.take(1)? as i32;
            let count = self.count[len] as i32;
            if code - first < count {
                return self
                    .symbol
                    .get((index + code - first) as usize)
                    .copied()
                    .ok_or_else(|| "invalid Huffman code".to_string());
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err("invalid Huffman code".into())
    }
}

fn inflate_codes(
    b: &mut Bits,
    out: &mut Vec<u8>,
    lit: &Huffman,
    dist: &Huffman,
    limit: usize,
) -> Result<(), String> {
    loop {
        let sym = lit.decode(b)? as usize;
        if sym < 256 {
            if out.len() >= limit {
                return Err("the decompressed data is too large".into());
            }
            out.push(sym as u8);
        } else if sym == 256 {
            return Ok(());
        } else {
            let i = sym - 257;
            if i >= 29 {
                return Err("invalid length code".into());
            }
            let len = LEN_BASE[i] as usize + b.take(LEN_EXTRA[i] as u32)? as usize;
            let d = dist.decode(b)? as usize;
            if d >= 30 {
                return Err("invalid distance code".into());
            }
            let back = DIST_BASE[d] as usize + b.take(DIST_EXTRA[d] as u32)? as usize;
            if back > out.len() {
                return Err("distance too far back".into());
            }
            if out.len() + len > limit {
                return Err("the decompressed data is too large".into());
            }
            let start = out.len() - back;
            for k in 0..len {
                let c = out[start + k];
                out.push(c);
            }
        }
    }
}

/// Raw DEFLATE data to bytes.
pub fn inflate_raw(data: &[u8], limit: usize) -> Result<(Vec<u8>, usize), String> {
    let mut b = Bits {
        data,
        pos: 0,
        bit: 0,
        nbits: 0,
    };
    let mut out = Vec::new();
    loop {
        let last = b.take(1)?;
        match b.take(2)? {
            0 => {
                b.align();
                let p = b.pos;
                let hdr = data.get(p..p + 4).ok_or("the compressed data ends early")?;
                let len = u16::from_le_bytes([hdr[0], hdr[1]]) as usize;
                let nlen = u16::from_le_bytes([hdr[2], hdr[3]]) as usize;
                if len != (!nlen & 0xffff) {
                    return Err("invalid stored block length".into());
                }
                let body = data
                    .get(p + 4..p + 4 + len)
                    .ok_or("the compressed data ends early")?;
                if out.len() + len > limit {
                    return Err("the decompressed data is too large".into());
                }
                out.extend_from_slice(body);
                b.pos = p + 4 + len;
            }
            1 => {
                let lit = Huffman::new(&fixed_lit_lengths())?;
                let dist = Huffman::new(&[5u8; 30])?;
                inflate_codes(&mut b, &mut out, &lit, &dist, limit)?;
            }
            2 => {
                let nlen = b.take(5)? as usize + 257;
                let ndist = b.take(5)? as usize + 1;
                let ncode = b.take(4)? as usize + 4;
                if nlen > 286 || ndist > 30 {
                    return Err("invalid dynamic block header".into());
                }
                let mut cl = [0u8; 19];
                for &k in &CL_ORDER[..ncode] {
                    cl[k] = b.take(3)? as u8;
                }
                let clh = Huffman::new(&cl)?;
                let mut lengths = vec![0u8; nlen + ndist];
                let mut i = 0;
                while i < nlen + ndist {
                    let sym = clh.decode(&mut b)?;
                    let (val, rep) = match sym {
                        0..=15 => (sym as u8, 1),
                        16 => {
                            if i == 0 {
                                return Err("repeat with no previous length".into());
                            }
                            (lengths[i - 1], 3 + b.take(2)? as usize)
                        }
                        17 => (0, 3 + b.take(3)? as usize),
                        _ => (0, 11 + b.take(7)? as usize),
                    };
                    if i + rep > nlen + ndist {
                        return Err("too many code lengths".into());
                    }
                    for _ in 0..rep {
                        lengths[i] = val;
                        i += 1;
                    }
                }
                if lengths[256] == 0 {
                    return Err("no end-of-block code".into());
                }
                let lit = Huffman::new(&lengths[..nlen])?;
                let dist = Huffman::new(&lengths[nlen..])?;
                inflate_codes(&mut b, &mut out, &lit, &dist, limit)?;
            }
            _ => return Err("invalid block type".into()),
        }
        if last == 1 {
            break;
        }
    }
    // Bytes consumed (a partially used last byte counts).
    Ok((out, b.pos))
}

/// zlib stream to bytes; checks the header and the Adler-32 trailer.
pub fn inflate(data: &[u8], limit: usize) -> Result<Vec<u8>, String> {
    if data.len() < 2 {
        return Err("the compressed data ends early".into());
    }
    let (cmf, flg) = (data[0], data[1]);
    if cmf & 0x0f != 8 || cmf >> 4 > 7 || !((cmf as u16) << 8 | flg as u16).is_multiple_of(31) {
        return Err("not a zlib stream".into());
    }
    if flg & 0x20 != 0 {
        return Err("zlib preset dictionaries are not supported".into());
    }
    let (out, used) = inflate_raw(&data[2..], limit)?;
    let trailer = data.get(2 + used..2 + used + 4);
    match trailer {
        Some(t) if u32::from_be_bytes([t[0], t[1], t[2], t[3]]) == adler32(&out) => Ok(out),
        Some(_) => Err("checksum mismatch".into()),
        None => Err("the compressed data ends early".into()),
    }
}

// ---------------------------------------------------------------------------
// Deflate (LZ77 + fixed Huffman)
// ---------------------------------------------------------------------------

struct BitOut {
    out: Vec<u8>,
    bit: u64,
    nbits: u32,
}

impl BitOut {
    /// `n` bits of `v`, least significant first.
    fn put(&mut self, v: u32, n: u32) {
        self.bit |= (v as u64) << self.nbits;
        self.nbits += n;
        while self.nbits >= 8 {
            self.out.push(self.bit as u8);
            self.bit >>= 8;
            self.nbits -= 8;
        }
    }
    /// A Huffman code (defined most significant bit first).
    fn code(&mut self, code: u32, len: u32) {
        let mut r = 0;
        for i in 0..len {
            r |= ((code >> i) & 1) << (len - 1 - i);
        }
        self.put(r, len);
    }
    fn finish(mut self) -> Vec<u8> {
        if self.nbits > 0 {
            self.out.push(self.bit as u8);
        }
        self.out
    }
}

fn put_literal(w: &mut BitOut, sym: u32) {
    match sym {
        0..=143 => w.code(0x30 + sym, 8),
        144..=255 => w.code(0x190 + sym - 144, 9),
        256..=279 => w.code(sym - 256, 7),
        _ => w.code(0xc0 + sym - 280, 8),
    }
}

fn put_match(w: &mut BitOut, len: usize, dist: usize) {
    let li = LEN_BASE
        .iter()
        .rposition(|&b| b as usize <= len)
        .unwrap_or(0);
    put_literal(w, 257 + li as u32);
    w.put((len - LEN_BASE[li] as usize) as u32, LEN_EXTRA[li] as u32);
    let di = DIST_BASE
        .iter()
        .rposition(|&b| b as usize <= dist)
        .unwrap_or(0);
    w.code(di as u32, 5);
    w.put(
        (dist - DIST_BASE[di] as usize) as u32,
        DIST_EXTRA[di] as u32,
    );
}

const WINDOW: usize = 32768;
const HASH_BITS: u32 = 15;
const MAX_CHAIN: usize = 16;

fn hash3(d: &[u8], i: usize) -> usize {
    let v = (d[i] as u32) << 16 | (d[i + 1] as u32) << 8 | d[i + 2] as u32;
    (v.wrapping_mul(2654435761) >> (32 - HASH_BITS)) as usize
}

/// Bytes to a zlib stream (one fixed-Huffman block).
pub fn deflate(data: &[u8]) -> Vec<u8> {
    let mut w = BitOut {
        out: vec![0x78, 0x9c],
        bit: 0,
        nbits: 0,
    };
    w.put(1, 1); // last block
    w.put(1, 2); // fixed Huffman
    let n = data.len();
    let mut head = vec![usize::MAX; 1 << HASH_BITS];
    let mut prev = vec![usize::MAX; WINDOW];
    let insert = |head: &mut Vec<usize>, prev: &mut Vec<usize>, i: usize| {
        if i + 2 < n {
            let h = hash3(data, i);
            prev[i % WINDOW] = head[h];
            head[h] = i;
        }
    };
    let mut i = 0;
    while i < n {
        let (mut best_len, mut best_dist) = (0usize, 0usize);
        if i + 2 < n {
            let mut cand = head[hash3(data, i)];
            let max_len = (n - i).min(258);
            let mut chain = 0;
            while cand != usize::MAX && i - cand <= WINDOW && chain < MAX_CHAIN {
                let mut l = 0;
                while l < max_len && data[cand + l] == data[i + l] {
                    l += 1;
                }
                if l > best_len {
                    best_len = l;
                    best_dist = i - cand;
                    if l == max_len {
                        break;
                    }
                }
                let next = prev[cand % WINDOW];
                if next == usize::MAX || next >= cand {
                    break;
                }
                cand = next;
                chain += 1;
            }
        }
        if best_len >= 3 {
            put_match(&mut w, best_len, best_dist);
            for k in i..i + best_len {
                insert(&mut head, &mut prev, k);
            }
            i += best_len;
        } else {
            put_literal(&mut w, data[i] as u32);
            insert(&mut head, &mut prev, i);
            i += 1;
        }
    }
    put_literal(&mut w, 256);
    let mut out = w.finish();
    if out.len() > data.len() + data.len() / 64 + 16 {
        // Incompressible data (random doubles): stored blocks are smaller.
        out = stored(data);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

/// zlib header and stored blocks (no trailer).
fn stored(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 65535 * 5 + 16);
    out.extend_from_slice(&[0x78, 0x01]);
    let mut chunks = data.chunks(65535).peekable();
    if chunks.peek().is_none() {
        out.extend_from_slice(&[1, 0, 0, 0xff, 0xff]);
    }
    while let Some(c) = chunks.next() {
        out.push(chunks.peek().is_none() as u8);
        let n = c.len() as u16;
        out.extend_from_slice(&n.to_le_bytes());
        out.extend_from_slice(&(!n).to_le_bytes());
        out.extend_from_slice(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut samples: Vec<Vec<u8>> = vec![
            vec![],
            b"a".to_vec(),
            b"abcabcabcabcabcabcabcabc".to_vec(),
            vec![0u8; 100_000],
        ];
        let mut x: u32 = 12345;
        let noise: Vec<u8> = (0..70_000)
            .map(|_| {
                x = x.wrapping_mul(1103515245).wrapping_add(12345);
                (x >> 16) as u8 % 7
            })
            .collect();
        samples.push(noise);
        let random: Vec<u8> = (0..200_000)
            .map(|_| {
                x = x.wrapping_mul(1103515245).wrapping_add(12345);
                (x >> 16) as u8
            })
            .collect();
        assert!(deflate(&random).len() < random.len() + 64);
        assert_eq!(&stored(b"")[2..], &[1, 0, 0, 0xff, 0xff]);
        samples.push(random);
        for s in samples {
            let z = deflate(&s);
            assert_eq!(inflate(&z, MAX_OUTPUT).unwrap(), s);
        }
    }

    #[test]
    fn stored_block_and_errors() {
        // zlib header, one stored block "hi", Adler-32.
        let mut z = vec![0x78, 0x01, 0x01, 2, 0, 0xfd, 0xff, b'h', b'i'];
        z.extend_from_slice(&adler32(b"hi").to_be_bytes());
        assert_eq!(inflate(&z, MAX_OUTPUT).unwrap(), b"hi");
        let n = z.len();
        z[n - 1] ^= 1;
        assert!(inflate(&z, MAX_OUTPUT).is_err());
        assert!(inflate(&[0x78], MAX_OUTPUT).is_err());
        assert!(inflate(&[0x78, 0x9c, 0xff, 0xff, 0xff], MAX_OUTPUT).is_err());
        // Output cap.
        let big = deflate(&vec![7u8; 10_000]);
        assert!(inflate(&big, 100).is_err());
    }
}
