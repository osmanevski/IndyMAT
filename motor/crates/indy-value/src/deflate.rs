//! DEFLATE (RFC 1951), zlib wrapper (RFC 1950), Adler-32 and CRC-32.
//!
//! Self-contained and written from the RFCs. Decoding never panics on
//! corrupt input: every failure is an [`InflateError`]. Output growth is
//! bounded by the input (at most 1032 bytes per input byte) and, in the
//! `_limited` variants, by an explicit ceiling, so header fields of a file
//! format can never trigger an unbounded allocation.
//!
//! The compressor runs LZ77 with hash chains and one step of lazy matching
//! over a 32 KiB window. Tokens are grouped into blocks and each block is
//! emitted in whichever of stored, fixed Huffman or dynamic Huffman form is
//! smallest. Dynamic code lengths come from package-merge, which gives
//! optimal length-limited codes.

use std::fmt;

/// Why a stream could not be decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InflateError(pub &'static str);

impl fmt::Display for InflateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for InflateError {}

type Res<T> = Result<T, InflateError>;

const MAX_BITS: usize = 15;
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
const CLEN_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

// ---------------------------------------------------------------------------
// Checksums
// ---------------------------------------------------------------------------

const fn crc_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}

static CRC_TABLE: [u32; 256] = crc_table();

/// CRC-32 (the IEEE polynomial used by PNG and ZIP).
pub fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = CRC_TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

/// Adler-32 (the zlib stream checksum).
pub fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65521;
    // Largest run for which the sums cannot overflow a u32.
    const RUN: usize = 5552;
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(RUN) {
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
// Inflate
// ---------------------------------------------------------------------------

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u64,
    n: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReader {
            data,
            pos: 0,
            buf: 0,
            n: 0,
        }
    }
    fn refill(&mut self) {
        while self.n <= 56 && self.pos < self.data.len() {
            self.buf |= (self.data[self.pos] as u64) << self.n;
            self.pos += 1;
            self.n += 8;
        }
    }
    fn consume(&mut self, k: u32) -> Res<()> {
        if k > self.n {
            return Err(InflateError("unexpected end of compressed data"));
        }
        self.buf >>= k;
        self.n -= k;
        Ok(())
    }
    fn bits(&mut self, k: u32) -> Res<u32> {
        self.refill();
        let v = (self.buf & ((1u64 << k) - 1)) as u32;
        self.consume(k)?;
        Ok(v)
    }
    /// Input bytes used so far; a partly read byte counts as used.
    fn consumed(&self) -> usize {
        self.pos - (self.n / 8) as usize
    }
}

/// Canonical Huffman decoder: a direct table indexed by the next input bits.
struct Decoder {
    table: Vec<u32>,
    mask: u64,
}

impl Decoder {
    fn new(lens: &[u8]) -> Res<Decoder> {
        let mut count = [0u32; MAX_BITS + 1];
        for &l in lens {
            count[l as usize] += 1;
        }
        count[0] = 0;
        let used: u32 = count.iter().sum();
        let mut left = 1i64;
        for &c in &count[1..] {
            left = left * 2 - c as i64;
            if left < 0 {
                return Err(InflateError("over-subscribed Huffman code"));
            }
        }
        // RFC 1951 allows an incomplete code only for a single one-bit code
        // (or for no code at all, when the alphabet is never used).
        if left > 0 && used > 1 {
            return Err(InflateError("incomplete Huffman code"));
        }
        if used == 1 && count[1] != 1 {
            return Err(InflateError("incomplete Huffman code"));
        }
        let max = (1..=MAX_BITS).rev().find(|&l| count[l] > 0).unwrap_or(1);
        let mut next = [0u32; MAX_BITS + 2];
        let mut code = 0u32;
        for l in 1..=MAX_BITS {
            code = (code + count[l - 1]) << 1;
            next[l] = code;
        }
        let mut table = vec![0u32; 1usize << max];
        for (sym, &l) in lens.iter().enumerate() {
            let l = l as usize;
            if l == 0 {
                continue;
            }
            let c = next[l];
            next[l] += 1;
            let rev = (c.reverse_bits() >> (32 - l)) as usize;
            let mut i = rev;
            while i < table.len() {
                table[i] = ((sym as u32) << 4) | l as u32;
                i += 1usize << l;
            }
        }
        Ok(Decoder {
            mask: (1u64 << max) - 1,
            table,
        })
    }
    fn decode(&self, r: &mut BitReader<'_>) -> Res<usize> {
        r.refill();
        let e = self.table[(r.buf & self.mask) as usize];
        if e == 0 {
            return Err(InflateError("invalid Huffman code"));
        }
        r.consume(e & 15)?;
        Ok((e >> 4) as usize)
    }
}

fn fixed_lit_lens() -> [u8; 288] {
    let mut l = [8u8; 288];
    l[144..256].fill(9);
    l[256..280].fill(7);
    l
}

fn read_dynamic(r: &mut BitReader<'_>) -> Res<(Decoder, Decoder)> {
    let hlit = r.bits(5)? as usize + 257;
    let hdist = r.bits(5)? as usize + 1;
    let hclen = r.bits(4)? as usize + 4;
    if hlit > 286 || hdist > 30 {
        return Err(InflateError("too many length or distance codes"));
    }
    let mut clen = [0u8; 19];
    for &slot in &CLEN_ORDER[..hclen] {
        clen[slot] = r.bits(3)? as u8;
    }
    let cl = Decoder::new(&clen)?;
    let total = hlit + hdist;
    let mut lens = [0u8; 320];
    let mut i = 0;
    while i < total {
        let sym = cl.decode(r)?;
        let (value, repeat) = match sym {
            0..=15 => (sym as u8, 1),
            16 => {
                if i == 0 {
                    return Err(InflateError("length repeat with no previous length"));
                }
                (lens[i - 1], 3 + r.bits(2)? as usize)
            }
            17 => (0, 3 + r.bits(3)? as usize),
            18 => (0, 11 + r.bits(7)? as usize),
            _ => return Err(InflateError("invalid code length symbol")),
        };
        if i + repeat > total {
            return Err(InflateError("code length repeat overflows the alphabet"));
        }
        lens[i..i + repeat].fill(value);
        i += repeat;
    }
    if lens[256] == 0 {
        return Err(InflateError("missing end-of-block code"));
    }
    Ok((
        Decoder::new(&lens[..hlit])?,
        Decoder::new(&lens[hlit..total])?,
    ))
}

fn inflate_codes(
    r: &mut BitReader<'_>,
    lit: &Decoder,
    dist: &Decoder,
    out: &mut Vec<u8>,
    max_out: usize,
) -> Res<()> {
    loop {
        let sym = lit.decode(r)?;
        match sym {
            0..=255 => {
                if out.len() >= max_out {
                    return Err(InflateError("decompressed data exceeds the expected size"));
                }
                out.push(sym as u8);
            }
            256 => return Ok(()),
            257..=285 => {
                let k = sym - 257;
                let len = LEN_BASE[k] as usize + r.bits(LEN_EXTRA[k] as u32)? as usize;
                let d = dist.decode(r)?;
                if d >= 30 {
                    return Err(InflateError("invalid distance code"));
                }
                let back = DIST_BASE[d] as usize + r.bits(DIST_EXTRA[d] as u32)? as usize;
                if back > out.len() {
                    return Err(InflateError(
                        "distance reaches before the start of the data",
                    ));
                }
                if len > max_out - out.len() {
                    return Err(InflateError("decompressed data exceeds the expected size"));
                }
                // Byte by byte: source and destination may overlap on purpose.
                let start = out.len() - back;
                for i in 0..len {
                    let b = out[start + i];
                    out.push(b);
                }
            }
            _ => return Err(InflateError("invalid literal/length code")),
        }
    }
}

fn inflate_stored(r: &mut BitReader<'_>, out: &mut Vec<u8>, max_out: usize) -> Res<()> {
    let partial = r.n % 8;
    r.consume(partial)?;
    let len = r.bits(16)? as usize;
    let nlen = r.bits(16)? as usize;
    if len != (!nlen & 0xFFFF) {
        return Err(InflateError("stored block length check failed"));
    }
    if len > max_out - out.len() {
        return Err(InflateError("decompressed data exceeds the expected size"));
    }
    let mut left = len;
    while left > 0 && r.n >= 8 {
        out.push((r.buf & 0xFF) as u8);
        r.consume(8)?;
        left -= 1;
    }
    if left > r.data.len() - r.pos {
        return Err(InflateError("unexpected end of compressed data"));
    }
    out.extend_from_slice(&r.data[r.pos..r.pos + left]);
    r.pos += left;
    Ok(())
}

/// Decode a raw DEFLATE stream of at most `max_out` bytes. Returns the data
/// and the number of input bytes the stream occupied.
pub fn inflate_limited(data: &[u8], max_out: usize) -> Res<(Vec<u8>, usize)> {
    let mut r = BitReader::new(data);
    let mut out = Vec::new();
    let mut fixed: Option<(Decoder, Decoder)> = None;
    loop {
        let last = r.bits(1)? == 1;
        match r.bits(2)? {
            0 => inflate_stored(&mut r, &mut out, max_out)?,
            1 => {
                if fixed.is_none() {
                    // All 32 five-bit codes exist; 30 and 31 are rejected on use.
                    fixed = Some((Decoder::new(&fixed_lit_lens())?, Decoder::new(&[5u8; 32])?));
                }
                if let Some((lit, dist)) = &fixed {
                    inflate_codes(&mut r, lit, dist, &mut out, max_out)?;
                }
            }
            2 => {
                let (lit, dist) = read_dynamic(&mut r)?;
                inflate_codes(&mut r, &lit, &dist, &mut out, max_out)?;
            }
            _ => return Err(InflateError("invalid block type")),
        }
        if last {
            break;
        }
    }
    let used = r.consumed();
    Ok((out, used))
}

/// Decode a raw DEFLATE stream.
pub fn inflate(data: &[u8]) -> Res<Vec<u8>> {
    inflate_limited(data, usize::MAX).map(|x| x.0)
}

/// Decode a zlib stream of at most `max_out` bytes, checking its Adler-32.
pub fn zlib_decompress_limited(data: &[u8], max_out: usize) -> Res<Vec<u8>> {
    if data.len() < 6 {
        return Err(InflateError("zlib stream is too short"));
    }
    let (cmf, flg) = (data[0] as u32, data[1] as u32);
    if cmf & 15 != 8 || cmf >> 4 > 7 {
        return Err(InflateError("not a deflate zlib stream"));
    }
    if (cmf * 256 + flg) % 31 != 0 {
        return Err(InflateError("zlib header check failed"));
    }
    if flg & 32 != 0 {
        return Err(InflateError("zlib preset dictionaries are not supported"));
    }
    let (out, used) = inflate_limited(&data[2..], max_out)?;
    let end = 2 + used;
    let Some(tail) = data.get(end..end + 4) else {
        return Err(InflateError("zlib stream has no Adler-32 checksum"));
    };
    let want = u32::from_be_bytes([tail[0], tail[1], tail[2], tail[3]]);
    if want != adler32(&out) {
        return Err(InflateError("zlib Adler-32 checksum mismatch"));
    }
    Ok(out)
}

/// Decode a zlib stream.
pub fn zlib_decompress(data: &[u8]) -> Res<Vec<u8>> {
    zlib_decompress_limited(data, usize::MAX)
}

// ---------------------------------------------------------------------------
// Deflate
// ---------------------------------------------------------------------------

const WINDOW: usize = 32768;
const HASH_BITS: u32 = 15;
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
const MAX_CHAIN: usize = 64;
/// Matches at least this long are taken without trying a lazy match.
const GOOD_ENOUGH: usize = 32;
/// A 3-byte match further away than this costs more than three literals.
const FAR_SHORT_MATCH: usize = 4096;
const BLOCK_TOKENS: usize = 1 << 16;
/// Bit set in a token holding a match (otherwise the token is a literal).
const MATCH_FLAG: u32 = 1 << 31;

struct BitWriter {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl BitWriter {
    fn put(&mut self, v: u32, k: u32) {
        self.acc |= (v as u64) << self.n;
        self.n += k;
        while self.n >= 8 {
            self.out.push((self.acc & 0xFF) as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }
    fn align(&mut self) {
        if self.n > 0 {
            self.put(0, 8 - self.n);
        }
    }
}

struct Matcher<'a> {
    data: &'a [u8],
    head: Vec<i32>,
    prev: Vec<i32>,
}

impl Matcher<'_> {
    fn hash(&self, i: usize) -> usize {
        let d = self.data;
        let v = (d[i] as u32) | ((d[i + 1] as u32) << 8) | ((d[i + 2] as u32) << 16);
        (v.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize
    }
    fn insert(&mut self, i: usize) {
        if i + MIN_MATCH <= self.data.len() {
            let h = self.hash(i);
            self.prev[i & (WINDOW - 1)] = self.head[h];
            self.head[h] = i as i32;
        }
    }
    /// Longest earlier match at `i`, as (length, distance); length 0 if none.
    fn find(&self, i: usize) -> (usize, usize) {
        let d = self.data;
        if i + MIN_MATCH > d.len() {
            return (0, 0);
        }
        let limit = MAX_MATCH.min(d.len() - i);
        let (mut best, mut best_dist) = (0, 0);
        let mut cand = self.head[self.hash(i)];
        let mut chain = MAX_CHAIN;
        while cand >= 0 && chain > 0 {
            let c = cand as usize;
            if c >= i || i - c > WINDOW {
                break;
            }
            if best == 0 || (best < limit && d[c + best] == d[i + best]) {
                let mut l = 0;
                while l < limit && d[c + l] == d[i + l] {
                    l += 1;
                }
                if l > best {
                    best = l;
                    best_dist = i - c;
                    if l == limit {
                        break;
                    }
                }
            }
            let next = self.prev[c & (WINDOW - 1)];
            if next >= cand {
                break;
            }
            cand = next;
            chain -= 1;
        }
        if best < MIN_MATCH || (best == MIN_MATCH && best_dist > FAR_SHORT_MATCH) {
            (0, 0)
        } else {
            (best, best_dist)
        }
    }
}

fn len_symbol(len: usize) -> usize {
    LEN_BASE
        .iter()
        .rposition(|&b| b as usize <= len)
        .unwrap_or_default()
}

fn dist_symbol(dist: usize) -> usize {
    let d = (dist - 1) as u32;
    if d < 4 {
        return d as usize;
    }
    let top = 31 - d.leading_zeros();
    (2 * top + ((d >> (top - 1)) & 1)) as usize
}

/// Optimal code lengths no longer than `max` (package-merge).
fn limited_lengths(freq: &[u32], max: usize) -> Vec<u8> {
    let mut lens = vec![0u8; freq.len()];
    let mut items: Vec<(u64, u16)> = freq
        .iter()
        .enumerate()
        .filter(|(_, f)| **f > 0)
        .map(|(s, f)| (*f as u64, s as u16))
        .collect();
    if items.is_empty() {
        return lens;
    }
    if items.len() == 1 {
        lens[items[0].1 as usize] = 1;
        return lens;
    }
    items.sort_unstable();
    let mut level: Vec<(u64, Vec<u16>)> = Vec::new();
    for _ in 0..max {
        let packages: Vec<(u64, Vec<u16>)> = level
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[x, y]| {
                let mut syms = x.1.clone();
                syms.extend_from_slice(&y.1);
                (x.0 + y.0, syms)
            })
            .collect();
        let mut merged = Vec::with_capacity(items.len() + packages.len());
        let (mut a, mut b) = (0, 0);
        while a < items.len() || b < packages.len() {
            if b >= packages.len() || (a < items.len() && items[a].0 <= packages[b].0) {
                merged.push((items[a].0, vec![items[a].1]));
                a += 1;
            } else {
                merged.push(packages[b].clone());
                b += 1;
            }
        }
        level = merged;
    }
    for (_, syms) in level.iter().take(2 * items.len() - 2) {
        for &s in syms {
            lens[s as usize] += 1;
        }
    }
    lens
}

/// Canonical codes, bit-reversed for DEFLATE's LSB-first packing.
fn canonical_codes(lens: &[u8]) -> Vec<u32> {
    let mut count = [0u32; MAX_BITS + 2];
    for &l in lens {
        count[l as usize] += 1;
    }
    count[0] = 0;
    let mut next = [0u32; MAX_BITS + 2];
    let mut code = 0;
    for l in 1..=MAX_BITS {
        code = (code + count[l - 1]) << 1;
        next[l] = code;
    }
    lens.iter()
        .map(|&l| {
            if l == 0 {
                return 0;
            }
            let c = next[l as usize];
            next[l as usize] += 1;
            c.reverse_bits() >> (32 - l as u32)
        })
        .collect()
}

/// Run-length encode code lengths as (symbol, extra value, extra bits).
fn rle_lengths(lens: &[u8]) -> Vec<(u8, u8, u8)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lens.len() {
        let v = lens[i];
        let mut run = 1;
        while i + run < lens.len() && lens[i + run] == v {
            run += 1;
        }
        i += run;
        if v == 0 {
            while run >= 11 {
                let k = run.min(138);
                out.push((18, (k - 11) as u8, 7));
                run -= k;
            }
            if run >= 3 {
                out.push((17, (run - 3) as u8, 3));
                run = 0;
            }
        } else {
            out.push((v, 0, 0));
            run -= 1;
            while run >= 3 {
                let k = run.min(6);
                out.push((16, (k - 3) as u8, 2));
                run -= k;
            }
        }
        for _ in 0..run {
            out.push((v, 0, 0));
        }
    }
    out
}

struct DynamicHeader {
    lit: Vec<u8>,
    dist: Vec<u8>,
    clen: [u8; 19],
    rle: Vec<(u8, u8, u8)>,
    hclen: usize,
    bits: u64,
}

fn dynamic_header(lit_freq: &[u32; 286], dist_freq: &[u32; 30]) -> DynamicHeader {
    let lit = limited_lengths(lit_freq, MAX_BITS);
    let dist = limited_lengths(dist_freq, MAX_BITS);
    let hlit = lit
        .iter()
        .rposition(|&l| l != 0)
        .map_or(257, |k| (k + 1).max(257));
    let hdist = dist.iter().rposition(|&l| l != 0).map_or(1, |k| k + 1);
    let mut joined = lit[..hlit].to_vec();
    joined.extend_from_slice(&dist[..hdist]);
    let rle = rle_lengths(&joined);
    let mut freq = [0u32; 19];
    for r in &rle {
        freq[r.0 as usize] += 1;
    }
    // zlib rejects an incomplete code-length code: keep at least two symbols.
    if freq.iter().filter(|f| **f > 0).count() < 2 {
        let spare = if freq[0] == 0 { 0 } else { 18 };
        freq[spare] = 1;
    }
    let lens = limited_lengths(&freq, 7);
    let mut clen = [0u8; 19];
    clen.copy_from_slice(&lens);
    let hclen = CLEN_ORDER
        .iter()
        .rposition(|&s| clen[s] != 0)
        .map_or(4, |k| (k + 1).max(4));
    let mut bits = 14 + 3 * hclen as u64;
    for r in &rle {
        bits += clen[r.0 as usize] as u64 + r.2 as u64;
    }
    DynamicHeader {
        lit: lit[..hlit].to_vec(),
        dist: dist[..hdist].to_vec(),
        clen,
        rle,
        hclen,
        bits,
    }
}

fn write_tokens(w: &mut BitWriter, tokens: &[u32], lit: &[u8], dist: &[u8]) {
    let lit_codes = canonical_codes(lit);
    let dist_codes = canonical_codes(dist);
    for &t in tokens {
        if t & MATCH_FLAG == 0 {
            let s = t as usize;
            w.put(lit_codes[s], lit[s] as u32);
        } else {
            let len = ((t >> 16) & 0xFF) as usize + MIN_MATCH;
            let d = (t & 0xFFFF) as usize + 1;
            let ls = len_symbol(len);
            w.put(lit_codes[257 + ls], lit[257 + ls] as u32);
            w.put((len - LEN_BASE[ls] as usize) as u32, LEN_EXTRA[ls] as u32);
            let ds = dist_symbol(d);
            w.put(dist_codes[ds], dist[ds] as u32);
            w.put((d - DIST_BASE[ds] as usize) as u32, DIST_EXTRA[ds] as u32);
        }
    }
    w.put(lit_codes[256], lit[256] as u32);
}

fn write_stored(w: &mut BitWriter, raw: &[u8], last: bool) {
    let mut chunks = raw.chunks(65535).peekable();
    if raw.is_empty() {
        w.put(last as u32, 1);
        w.put(0, 2);
        w.align();
        w.put(0, 16);
        w.put(0xFFFF, 16);
        return;
    }
    while let Some(c) = chunks.next() {
        w.put((last && chunks.peek().is_none()) as u32, 1);
        w.put(0, 2);
        w.align();
        w.put(c.len() as u32, 16);
        w.put(!(c.len() as u32) & 0xFFFF, 16);
        w.out.extend_from_slice(c);
    }
}

fn write_block(w: &mut BitWriter, tokens: &[u32], raw: &[u8], last: bool) {
    let mut lit_freq = [0u32; 286];
    let mut dist_freq = [0u32; 30];
    lit_freq[256] = 1;
    let mut extra = 0u64;
    for &t in tokens {
        if t & MATCH_FLAG == 0 {
            lit_freq[t as usize] += 1;
        } else {
            let ls = len_symbol(((t >> 16) & 0xFF) as usize + MIN_MATCH);
            let ds = dist_symbol((t & 0xFFFF) as usize + 1);
            lit_freq[257 + ls] += 1;
            dist_freq[ds] += 1;
            extra += LEN_EXTRA[ls] as u64 + DIST_EXTRA[ds] as u64;
        }
    }
    let fixed_lit = fixed_lit_lens();
    let fixed_dist = [5u8; 30];
    let cost = |lit: &[u8], dist: &[u8]| -> u64 {
        let l: u64 = lit
            .iter()
            .zip(lit_freq.iter())
            .map(|(a, b)| *a as u64 * *b as u64)
            .sum();
        let d: u64 = dist
            .iter()
            .zip(dist_freq.iter())
            .map(|(a, b)| *a as u64 * *b as u64)
            .sum();
        l + d + extra
    };
    let fixed_bits = 3 + cost(&fixed_lit, &fixed_dist);
    let stored_blocks = raw.len().div_ceil(65535).max(1) as u64;
    let stored_bits = stored_blocks * 40 + 7 + 8 * raw.len() as u64;
    // A dynamic code needs two literal/length symbols to be a complete code.
    let header = (lit_freq.iter().filter(|f| **f > 0).count() >= 2)
        .then(|| dynamic_header(&lit_freq, &dist_freq));
    let dynamic_bits = header
        .as_ref()
        .map_or(u64::MAX, |h| 3 + h.bits + cost(&h.lit, &h.dist));
    if stored_bits < fixed_bits && stored_bits < dynamic_bits {
        write_stored(w, raw, last);
    } else if let Some(h) = header.as_ref().filter(|_| dynamic_bits < fixed_bits) {
        w.put(last as u32, 1);
        w.put(2, 2);
        w.put((h.lit.len() - 257) as u32, 5);
        w.put((h.dist.len() - 1) as u32, 5);
        w.put((h.hclen - 4) as u32, 4);
        for &s in &CLEN_ORDER[..h.hclen] {
            w.put(h.clen[s] as u32, 3);
        }
        let clen_codes = canonical_codes(&h.clen);
        for r in &h.rle {
            w.put(clen_codes[r.0 as usize], h.clen[r.0 as usize] as u32);
            w.put(r.1 as u32, r.2 as u32);
        }
        write_tokens(w, tokens, &h.lit, &h.dist);
    } else {
        w.put(last as u32, 1);
        w.put(1, 2);
        write_tokens(w, tokens, &fixed_lit, &fixed_dist);
    }
}

/// Compress into a raw DEFLATE stream.
pub fn deflate(data: &[u8]) -> Vec<u8> {
    let mut w = BitWriter {
        out: Vec::with_capacity(data.len() / 2 + 16),
        acc: 0,
        n: 0,
    };
    let mut m = Matcher {
        data,
        head: vec![-1; 1 << HASH_BITS],
        prev: vec![-1; WINDOW],
    };
    let mut tokens: Vec<u32> = Vec::with_capacity(BLOCK_TOKENS.min(data.len() + 1));
    let mut block_start = 0;
    let mut i = 0;
    while i < data.len() {
        let (len, dist) = m.find(i);
        m.insert(i);
        if len == 0 {
            tokens.push(data[i] as u32);
            i += 1;
        } else if len < GOOD_ENOUGH && m.find(i + 1).0 > len {
            // Lazy matching: a literal now buys a longer match next.
            tokens.push(data[i] as u32);
            i += 1;
        } else {
            tokens.push(MATCH_FLAG | (((len - MIN_MATCH) as u32) << 16) | (dist - 1) as u32);
            for k in i + 1..i + len {
                m.insert(k);
            }
            i += len;
        }
        if tokens.len() >= BLOCK_TOKENS {
            write_block(&mut w, &tokens, &data[block_start..i], i == data.len());
            tokens.clear();
            block_start = i;
            if i == data.len() {
                w.align();
                return w.out;
            }
        }
    }
    write_block(&mut w, &tokens, &data[block_start..], true);
    w.align();
    w.out
}

/// Compress into a zlib stream.
pub fn zlib_compress(data: &[u8]) -> Vec<u8> {
    // CMF: deflate with a 32 KiB window; FLG: default level, valid check bits.
    let mut out = vec![0x78, 0x9C];
    out.extend_from_slice(&deflate(data));
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Generated with python3 zlib from the data of `fixture_text`: level 0,
    // Z_FIXED and the default strategy (block types 0, 1 and 2 respectively).
    const STORED: &[u8] = &[
        120, 1, 1, 40, 0, 215, 255, 97, 97, 97, 98, 99, 100, 101, 103, 97, 99, 101, 104, 99, 102,
        97, 101, 97, 101, 97, 102, 99, 104, 101, 99, 97, 103, 101, 100, 99, 98, 97, 97, 97, 97, 97,
        98, 99, 100, 101, 103, 62, 143, 15, 137,
    ];
    const FIXED: &[u8] = &[
        120, 1, 75, 76, 76, 76, 74, 78, 73, 77, 79, 76, 78, 205, 72, 78, 75, 76, 5, 194, 180, 228,
        140, 212, 228, 196, 244, 212, 148, 228, 164, 68, 16, 0, 203, 3, 0, 62, 143, 15, 137,
    ];
    const DYNAMIC: &[u8] = &[
        120, 218, 237, 201, 49, 1, 0, 48, 8, 196, 64, 173, 225, 121, 192, 191, 130, 182, 18, 186,
        147, 140, 7, 132, 210, 141, 60, 42, 124, 47, 141, 69, 59, 21, 188, 214, 215, 191, 253, 0,
        204, 199, 233, 57,
    ];

    fn fixture_text(n: usize) -> Vec<u8> {
        (0..n).map(|i| 97 + (((i * i) >> 3) & 7) as u8).collect()
    }

    struct Rand(u64);
    impl Rand {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    fn round_trip(data: &[u8]) {
        let z = zlib_compress(data);
        assert_eq!(zlib_decompress(&z).unwrap(), data);
        assert_eq!(inflate(&deflate(data)).unwrap(), data);
    }

    #[test]
    fn checksums_match_known_values() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        assert_eq!(adler32(b""), 1);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn decodes_zlib_fixtures_of_each_block_type() {
        assert_eq!(STORED[2] >> 1 & 3, 0);
        assert_eq!(FIXED[2] >> 1 & 3, 1);
        assert_eq!(DYNAMIC[2] >> 1 & 3, 2);
        assert_eq!(zlib_decompress(STORED).unwrap(), fixture_text(40));
        assert_eq!(zlib_decompress(FIXED).unwrap(), fixture_text(40));
        assert_eq!(zlib_decompress(DYNAMIC).unwrap(), fixture_text(600));
    }

    #[test]
    fn round_trips_edge_sizes_and_patterns() {
        round_trip(b"");
        round_trip(b"a");
        round_trip(b"ab");
        round_trip(b"abc");
        round_trip(&vec![0u8; 100_000]);
        round_trip(&fixture_text(70_000));
        let run: Vec<u8> = (0..300_000).map(|i| (i / 1000) as u8).collect();
        round_trip(&run);
    }

    #[test]
    fn round_trips_random_data() {
        let mut r = Rand(0x1234_5678_9ABC_DEF1);
        for k in 0..60 {
            let n = (r.next() % 5000) as usize + k;
            let alphabet = [2u64, 4, 16, 256][k % 4];
            let data: Vec<u8> = (0..n).map(|_| (r.next() % alphabet) as u8).collect();
            round_trip(&data);
        }
        // Incompressible and larger than one stored block and one token block.
        let big: Vec<u8> = (0..200_000).map(|_| r.next() as u8).collect();
        round_trip(&big);
    }

    #[test]
    fn distances_beyond_the_window_are_never_emitted() {
        let mut r = Rand(77);
        let chunk: Vec<u8> = (0..1000).map(|_| r.next() as u8).collect();
        let mut data = chunk.clone();
        data.extend((0..40_000).map(|_| r.next() as u8));
        data.extend_from_slice(&chunk);
        round_trip(&data);
    }

    #[test]
    fn output_compresses_and_picks_a_compact_block() {
        let text = fixture_text(50_000);
        assert!(deflate(&text).len() < text.len() / 2);
        let mut r = Rand(5);
        let noise: Vec<u8> = (0..70_000).map(|_| r.next() as u8).collect();
        assert!(deflate(&noise).len() <= noise.len() + 64);
        // Skewed data with no repeats beats the fixed code: dynamic block.
        let d = deflate(&fixture_text(600));
        assert_eq!(d[0] >> 1 & 3, 2);
    }

    #[test]
    fn package_merge_respects_the_limit_and_kraft() {
        let mut freq = [0u32; 30];
        let mut a = 1u32;
        for f in freq.iter_mut() {
            *f = a;
            a = a.saturating_mul(2);
        }
        for (symbols, max) in [(19usize, 7usize), (30, 15)] {
            let lens = limited_lengths(&freq[..symbols], max);
            assert!(lens.iter().all(|&l| l as usize <= max && l > 0));
            let kraft: f64 = lens.iter().map(|&l| 0.5f64.powi(l as i32)).sum();
            assert_eq!(kraft, 1.0);
        }
    }

    #[test]
    fn corrupt_input_is_an_error_and_never_panics() {
        for (fixture, size) in [(STORED, 40), (FIXED, 40), (DYNAMIC, 600)] {
            let want = fixture_text(size);
            for n in 0..fixture.len() {
                assert!(zlib_decompress(&fixture[..n]).is_err());
                for bit in 0..8 {
                    let mut c = fixture.to_vec();
                    c[n] ^= 1 << bit;
                    // Padding bits are ignored, so a flip may still decode,
                    // but it can never yield different data unnoticed.
                    if let Ok(v) = zlib_decompress(&c) {
                        assert_eq!(v, want, "byte {} bit {}", n, bit);
                    }
                }
            }
        }
        let mut r = Rand(99);
        for _ in 0..3000 {
            let n = (r.next() % 300) as usize;
            let noise: Vec<u8> = (0..n).map(|_| r.next() as u8).collect();
            let _ = inflate_limited(&noise, 1 << 20);
            let _ = zlib_decompress_limited(&noise, 1 << 20);
        }
    }

    #[test]
    fn specific_malformed_streams_are_rejected() {
        // Reserved block type 3.
        assert!(inflate(&[0x07]).is_err());
        // Stored block whose NLEN is wrong.
        assert!(inflate(&[0x01, 1, 0, 0, 0, 0x41]).is_err());
        // Stored block promising more bytes than present.
        assert!(inflate(&[0x01, 9, 0, 0xF6, 0xFF, 0x41]).is_err());
        // Fixed block: a match with distance 1 at the very start.
        let mut w = BitWriter {
            out: Vec::new(),
            acc: 0,
            n: 0,
        };
        w.put(1, 1);
        w.put(1, 2);
        let lit = fixed_lit_lens();
        let codes = canonical_codes(&lit);
        w.put(codes[257], lit[257] as u32);
        w.put(0, 5);
        w.align();
        assert!(inflate(&w.out).is_err());
    }

    #[test]
    fn the_output_limit_is_enforced() {
        let z = zlib_compress(&vec![7u8; 10_000]);
        assert_eq!(zlib_decompress_limited(&z, 10_000).unwrap().len(), 10_000);
        assert!(zlib_decompress_limited(&z, 9_999).is_err());
        let stored = zlib_compress(&{
            let mut r = Rand(3);
            (0..5000).map(|_| r.next() as u8).collect::<Vec<u8>>()
        });
        assert!(zlib_decompress_limited(&stored, 100).is_err());
    }
}
