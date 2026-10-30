//! Audio files: `audiowrite`, `audioread`, `audioinfo` for RIFF WAVE, with
//! MATLAB's documented class and bit-depth rules. Other containers and audio
//! devices give explicit errors.
//!
//! Sample scaling: a floating value `y` maps to `round(y * 2^(bits-1))`
//! saturated to the integer range, and reading divides by `2^(bits-1)`, so
//! values already on that grid survive a round trip exactly. 8-bit samples are
//! unsigned with an offset of 128. `audioread(..., 'native')` returns 24-bit
//! data as `int32` shifted to the top of the range, per MATLAB's table.

use indy_value::{Num, NumClass, StructArray, Value, norm_dims};

use super::core::resolve_path;
use super::{err, need, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    reg(it, "audiowrite", audiowrite);
    reg(it, "audioread", audioread);
    reg(it, "audioinfo", audioinfo);
    reg(it, "sound", |_, _, _| Err(no_device("sound")));
    reg(it, "soundsc", |_, _, _| Err(no_device("soundsc")));
    reg(it, "audioplayer", |_, _, _| Err(no_device("audioplayer")));
    reg(it, "audiorecorder", |_, _, _| {
        Err(no_device("audiorecorder"))
    });
}

fn no_device(fname: &str) -> Box<indy_value::Error> {
    err(
        "MATLAB:audio:noDevice",
        format!("{}: no audio output in the Rust engine", fname),
    )
}

fn fail(fname: &str, what: &str, msg: impl Into<String>) -> Box<indy_value::Error> {
    err(&format!("MATLAB:{}:{}", fname, what), msg)
}

fn extension(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    match base.rfind('.') {
        Some(k) => base[k + 1..].to_lowercase(),
        None => String::new(),
    }
}

/// Error for a container other than WAVE, `None` for `.wav` or no extension.
fn container_error(fname: &str, name: &str) -> Option<Box<indy_value::Error>> {
    let ext = extension(name);
    let label = match ext.as_str() {
        "" | "wav" | "wave" => return None,
        "flac" => "FLAC",
        "mp3" => "MP3",
        "ogg" | "oga" => "OGG",
        "opus" => "Opus",
        "m4a" | "mp4" | "aac" => "MPEG-4 audio",
        "aif" | "aiff" | "aifc" => "AIFF",
        "au" | "snd" => "AU",
        _ => {
            return Some(fail(
                fname,
                "unsupportedFormat",
                format!(
                    "{}: unsupported audio file type '.{}'; use .wav.",
                    fname, ext
                ),
            ));
        }
    };
    Some(fail(
        fname,
        "unsupportedFormat",
        format!(
            "{}: {} files are not supported yet in the Rust engine",
            fname, label
        ),
    ))
}

fn file_name_arg(v: &Value, fname: &str) -> R<String> {
    match v.to_str() {
        Some(s) if v.is_text_scalar() && !s.is_empty() => Ok(s),
        _ => Err(fail(
            fname,
            "invalidFileName",
            format!(
                "{}: the file name must be a nonempty character vector or string scalar.",
                fname
            ),
        )),
    }
}

// ---------------------------------------------------------------------------
// Sample encodings
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
enum Encoding {
    U8,
    I16,
    I24,
    I32,
    F32,
    F64,
}

impl Encoding {
    fn bytes(self) -> usize {
        match self {
            Encoding::U8 => 1,
            Encoding::I16 => 2,
            Encoding::I24 => 3,
            Encoding::I32 | Encoding::F32 => 4,
            Encoding::F64 => 8,
        }
    }
    fn is_float(self) -> bool {
        matches!(self, Encoding::F32 | Encoding::F64)
    }
    /// Value of full scale for integer encodings.
    fn scale(self) -> f64 {
        match self {
            Encoding::U8 => 128.0,
            Encoding::I16 => 32768.0,
            Encoding::I24 => 8388608.0,
            Encoding::I32 => 2147483648.0,
            Encoding::F32 | Encoding::F64 => 1.0,
        }
    }
}

// ---------------------------------------------------------------------------
// audiowrite
// ---------------------------------------------------------------------------

struct WriteOpts {
    bits: Option<usize>,
    title: Option<String>,
    artist: Option<String>,
    comment: Option<String>,
}

fn write_opts(a: &[Value]) -> R<WriteOpts> {
    let fname = "audiowrite";
    if !a.len().is_multiple_of(2) {
        return Err(fail(
            fname,
            "invalidNameValue",
            "audiowrite: name-value arguments must come in pairs.",
        ));
    }
    let mut o = WriteOpts {
        bits: None,
        title: None,
        artist: None,
        comment: None,
    };
    for p in a.chunks(2) {
        let name = match p[0].to_str() {
            Some(s) if p[0].is_text() => s,
            _ => {
                return Err(fail(
                    fname,
                    "invalidNameValue",
                    "audiowrite: expected an option name (text).",
                ));
            }
        };
        let text = |v: &Value| -> R<String> {
            match v.to_str() {
                Some(s) if v.is_text() => Ok(s),
                _ => Err(fail(
                    fname,
                    "invalidOption",
                    format!("audiowrite: '{}' must be text.", name),
                )),
            }
        };
        match name.to_lowercase().as_str() {
            "bitspersample" => {
                let b = p[1].scalar_f64().filter(|_| p[1].is_numeric());
                o.bits = Some(match b {
                    Some(x) if [8.0, 16.0, 24.0, 32.0, 64.0].contains(&x) => x as usize,
                    _ => {
                        return Err(fail(
                            fname,
                            "invalidBitsPerSample",
                            "audiowrite: 'BitsPerSample' must be 8, 16, 24, 32 or 64 for .wav files.",
                        ));
                    }
                });
            }
            "title" => o.title = Some(text(&p[1])?),
            "artist" => o.artist = Some(text(&p[1])?),
            "comment" => o.comment = Some(text(&p[1])?),
            _ => {
                return Err(fail(
                    fname,
                    "unsupportedOption",
                    format!(
                        "audiowrite: parameter '{}' is not supported for .wav files in the Rust engine.",
                        name
                    ),
                ));
            }
        }
    }
    Ok(o)
}

/// Input class: (normalising divisor, offset) for integer classes.
fn input_scale(class: NumClass) -> Option<(f64, f64)> {
    match class {
        NumClass::Double | NumClass::Single => Some((1.0, 0.0)),
        NumClass::UInt8 => Some((128.0, 128.0)),
        NumClass::Int16 => Some((32768.0, 0.0)),
        NumClass::Int32 => Some((2147483648.0, 0.0)),
        _ => None,
    }
}

/// MATLAB's table: 32 bits is PCM for `int32` input and float otherwise.
fn encoding_for(bits: usize, class: NumClass) -> Encoding {
    match bits {
        8 => Encoding::U8,
        24 => Encoding::I24,
        32 if class == NumClass::Int32 => Encoding::I32,
        32 => Encoding::F32,
        64 => Encoding::F64,
        _ => Encoding::I16,
    }
}

/// Whether the input class already is the file's integer representation.
fn is_raw(class: NumClass, e: Encoding) -> bool {
    matches!(
        (class, e),
        (NumClass::UInt8, Encoding::U8)
            | (NumClass::Int16, Encoding::I16)
            | (NumClass::Int32, Encoding::I32)
    )
}

fn put_sample(out: &mut Vec<u8>, e: Encoding, x: f64) {
    match e {
        Encoding::U8 => out.push(x as u8),
        Encoding::I16 => out.extend_from_slice(&(x as i16).to_le_bytes()),
        Encoding::I24 => out.extend_from_slice(&(x as i32).to_le_bytes()[..3]),
        Encoding::I32 => out.extend_from_slice(&(x as i32).to_le_bytes()),
        Encoding::F32 => out.extend_from_slice(&(x as f32).to_le_bytes()),
        Encoding::F64 => out.extend_from_slice(&x.to_le_bytes()),
    }
}

/// Encode samples; the flag reports whether anything had to be clipped.
fn encode(n: &Num, e: Encoding) -> R<(Vec<u8>, bool)> {
    let Some((div, offset)) = input_scale(n.class) else {
        return Err(fail(
            "audiowrite",
            "invalidType",
            format!(
                "audiowrite: input of class {} is not supported; use double, single, uint8, int16 or int32.",
                n.class.name()
            ),
        ));
    };
    let mut out = Vec::with_capacity(n.re.len().saturating_mul(e.bytes()));
    let mut clipped = false;
    let raw = is_raw(n.class, e);
    let (lo, hi) = match e {
        Encoding::U8 => (-128.0, 127.0),
        Encoding::I16 => (-32768.0, 32767.0),
        Encoding::I24 => (-8388608.0, 8388607.0),
        Encoding::I32 => (-2147483648.0, 2147483647.0),
        Encoding::F32 | Encoding::F64 => (-1.0, 1.0),
    };
    for &v in &n.re {
        if raw {
            put_sample(&mut out, e, v);
            continue;
        }
        let y = (v - offset) / div;
        if y.is_nan() {
            clipped = true;
            let zero = if e == Encoding::U8 { 128.0 } else { 0.0 };
            put_sample(&mut out, e, zero);
            continue;
        }
        if e.is_float() {
            if !(-1.0..=1.0).contains(&y) {
                clipped = true;
            }
            put_sample(&mut out, e, y.clamp(-1.0, 1.0));
            continue;
        }
        let scaled = (y * e.scale()).round();
        // Only values past +-1 count as clipping: +1 itself saturates to the
        // largest code by the asymmetry of two's complement.
        if !(-1.0..=1.0).contains(&y) {
            clipped = true;
        }
        let s = scaled.clamp(lo, hi);
        put_sample(&mut out, e, if e == Encoding::U8 { s + 128.0 } else { s });
    }
    Ok((out, clipped))
}

fn info_chunk(o: &WriteOpts) -> Vec<u8> {
    let mut body = Vec::new();
    for (id, v) in [
        (b"INAM", &o.title),
        (b"IART", &o.artist),
        (b"ICMT", &o.comment),
    ] {
        if let Some(s) = v {
            let mut text = s.as_bytes().to_vec();
            text.push(0);
            body.extend_from_slice(id);
            body.extend_from_slice(&(text.len() as u32).to_le_bytes());
            body.extend_from_slice(&text);
            if !text.len().is_multiple_of(2) {
                body.push(0);
            }
        }
    }
    if body.is_empty() {
        return body;
    }
    let mut out = b"LIST".to_vec();
    out.extend_from_slice(&((body.len() + 4) as u32).to_le_bytes());
    out.extend_from_slice(b"INFO");
    out.extend_from_slice(&body);
    out
}

fn wav_bytes(pcm: &[u8], e: Encoding, channels: usize, rate: u32, o: &WriteOpts) -> R<Vec<u8>> {
    let info = info_chunk(o);
    let pad = pcm.len() % 2;
    let float = e.is_float();
    let fmt_len: usize = if float { 18 } else { 16 };
    let fact_len: usize = if float { 12 } else { 0 };
    let total = 4 + (8 + fmt_len) + fact_len + 8 + pcm.len() + pad + info.len();
    if total > u32::MAX as usize {
        return Err(fail(
            "audiowrite",
            "fileTooLarge",
            "audiowrite: the data does not fit the 4 GB limit of a .wav file.",
        ));
    }
    let block = channels * e.bytes();
    let mut w = Vec::with_capacity(total + 8);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(total as u32).to_le_bytes());
    w.extend_from_slice(b"WAVE");
    w.extend_from_slice(b"fmt ");
    w.extend_from_slice(&(fmt_len as u32).to_le_bytes());
    w.extend_from_slice(&(if float { 3u16 } else { 1u16 }).to_le_bytes());
    w.extend_from_slice(&(channels as u16).to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate.wrapping_mul(block as u32)).to_le_bytes());
    w.extend_from_slice(&(block as u16).to_le_bytes());
    w.extend_from_slice(&((e.bytes() * 8) as u16).to_le_bytes());
    if float {
        w.extend_from_slice(&0u16.to_le_bytes());
        w.extend_from_slice(b"fact");
        w.extend_from_slice(&4u32.to_le_bytes());
        w.extend_from_slice(&((pcm.len() / block) as u32).to_le_bytes());
    }
    w.extend_from_slice(b"data");
    w.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    w.extend_from_slice(pcm);
    if pad == 1 {
        w.push(0);
    }
    w.extend_from_slice(&info);
    Ok(w)
}

fn audiowrite(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let fname = "audiowrite";
    need(&a, 3, fname)?;
    let mut name = file_name_arg(&a[0], fname)?;
    if let Some(e) = container_error(fname, &name) {
        return Err(e);
    }
    if extension(&name).is_empty() {
        name.push_str(".wav");
    }
    let Value::Num(y) = &a[1] else {
        return Err(fail(
            fname,
            "invalidType",
            format!(
                "audiowrite: input of class {} is not supported; use double, single, uint8, int16 or int32.",
                a[1].class_name()
            ),
        ));
    };
    if y.is_complex() {
        return Err(fail(
            fname,
            "invalidData",
            "audiowrite: the data must be real.",
        ));
    }
    if y.re.is_empty() {
        return Err(fail(
            fname,
            "invalidData",
            "audiowrite: the data must not be empty.",
        ));
    }
    if y.dims.len() != 2 {
        return Err(fail(
            fname,
            "invalidData",
            "audiowrite: the data must be an M-by-N matrix (samples by channels).",
        ));
    }
    let fs = a[2].scalar_f64().filter(|_| a[2].is_numeric());
    let rate = match fs {
        Some(x) if x >= 1.0 && x.fract() == 0.0 && x <= u32::MAX as f64 => x as u32,
        _ => {
            return Err(fail(
                fname,
                "invalidFs",
                "audiowrite: the sample rate must be a positive integer scalar.",
            ));
        }
    };
    let o = write_opts(&a[3..])?;
    // A row vector is one channel, not one sample of many channels.
    let (samples, channels) = if y.dims[0] == 1 {
        (y.re.len(), 1)
    } else {
        (y.dims[0], y.dims[1])
    };
    if channels > u16::MAX as usize || channels * 8 > u16::MAX as usize {
        return Err(fail(fname, "invalidData", "audiowrite: too many channels."));
    }
    let e = encoding_for(o.bits.unwrap_or(16), y.class);
    // Column-major data of an M-by-N matrix must become frame-interleaved.
    let mut inter = Num {
        class: y.class,
        dims: norm_dims(vec![samples * channels, 1]),
        re: Vec::with_capacity(y.re.len()),
        im: None,
    };
    for s in 0..samples {
        for c in 0..channels {
            inter.re.push(y.re[s + c * samples]);
        }
    }
    let (pcm, clipped) = encode(&inter, e)?;
    if clipped {
        it.warning(
            "MATLAB:audiowrite:dataClipped",
            "Data clipped when writing file.",
        );
    }
    let bytes = wav_bytes(&pcm, e, channels, rate, &o)?;
    let path = resolve_path(it, &name);
    std::fs::write(&path, bytes).map_err(|e| {
        fail(
            fname,
            "fileOpenError",
            format!("audiowrite: unable to write file '{}': {}", name, e),
        )
    })?;
    Ok(vec![])
}

// ---------------------------------------------------------------------------
// WAVE parsing
// ---------------------------------------------------------------------------

struct Wave<'a> {
    encoding: Encoding,
    channels: usize,
    rate: u32,
    bits: usize,
    data: &'a [u8],
    frames: usize,
    title: Option<String>,
    artist: Option<String>,
    comment: Option<String>,
}

fn le16(b: &[u8], p: usize) -> u16 {
    u16::from_le_bytes([b[p], b[p + 1]])
}

fn le32(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes([b[p], b[p + 1], b[p + 2], b[p + 3]])
}

fn info_text(d: &[u8]) -> String {
    let end = d.iter().position(|&x| x == 0).unwrap_or(d.len());
    String::from_utf8_lossy(&d[..end]).into_owned()
}

fn parse_wave(b: &[u8]) -> Result<Wave<'_>, String> {
    if b.len() < 12 || &b[8..12] != b"WAVE" {
        return Err("the file is not a WAVE file".into());
    }
    match &b[..4] {
        b"RIFF" => {}
        b"RF64" => return Err("RF64 WAVE files are not supported yet in the Rust engine".into()),
        b"RIFX" => {
            return Err(
                "big-endian RIFX WAVE files are not supported yet in the Rust engine".into(),
            );
        }
        _ => return Err("the file is not a WAVE file".into()),
    }
    let mut pos = 12;
    let mut fmt: Option<(u16, usize, u32, usize)> = None;
    let mut data: Option<&[u8]> = None;
    let (mut title, mut artist, mut comment) = (None, None, None);
    while pos + 8 <= b.len() {
        let id = &b[pos..pos + 4];
        let size = le32(b, pos + 4) as usize;
        let start = pos + 8;
        let avail = b.len() - start;
        if id == b"data" {
            // Streaming writers may leave a size past the end: take what exists.
            data = Some(&b[start..start + size.min(avail)]);
        } else if size > avail {
            return Err("a WAVE chunk runs past the end of the file".into());
        } else if id == b"fmt " {
            if size < 16 {
                return Err("the WAVE format chunk is too short".into());
            }
            let f = &b[start..start + size];
            let mut tag = le16(f, 0);
            let bits = le16(f, 14) as usize;
            if tag == 0xFFFE {
                if size < 40 {
                    return Err("the extensible WAVE format chunk is too short".into());
                }
                tag = le16(f, 24);
            }
            fmt = Some((tag, le16(f, 2) as usize, le32(f, 4), bits));
        } else if id == b"LIST" && size >= 4 && &b[start..start + 4] == b"INFO" {
            let end = start + size;
            let mut p = start + 4;
            while p + 8 <= end {
                let n = le32(b, p + 4) as usize;
                if n > end - p - 8 {
                    break;
                }
                let d = &b[p + 8..p + 8 + n];
                match &b[p..p + 4] {
                    b"INAM" => title = Some(info_text(d)),
                    b"IART" => artist = Some(info_text(d)),
                    b"ICMT" => comment = Some(info_text(d)),
                    _ => {}
                }
                p += 8 + n + n % 2;
            }
        }
        let step = size.min(avail);
        pos = start + step + step % 2;
    }
    let Some((tag, channels, rate, bits)) = fmt else {
        return Err("the WAVE file has no format chunk".into());
    };
    let Some(data) = data else {
        return Err("the WAVE file has no data chunk".into());
    };
    if channels == 0 {
        return Err("the WAVE file declares no channels".into());
    }
    let encoding = match (tag, bits) {
        (1, 8) => Encoding::U8,
        (1, 16) => Encoding::I16,
        (1, 24) => Encoding::I24,
        (1, 32) => Encoding::I32,
        (3, 32) => Encoding::F32,
        (3, 64) => Encoding::F64,
        (1 | 3, _) => return Err(format!("{}-bit WAVE samples are not supported", bits)),
        (6, _) => return Err("A-law WAVE files are not supported yet in the Rust engine".into()),
        (7, _) => return Err("mu-law WAVE files are not supported yet in the Rust engine".into()),
        _ => {
            return Err(format!(
                "WAVE compression format {} is not supported yet in the Rust engine",
                tag
            ));
        }
    };
    let frames = data.len() / (channels * encoding.bytes());
    Ok(Wave {
        encoding,
        channels,
        rate,
        bits,
        data,
        frames,
        title,
        artist,
        comment,
    })
}

fn load(it: &Interp, fname: &str, name: &str) -> R<Vec<u8>> {
    if let Some(e) = container_error(fname, name) {
        return Err(e);
    }
    let path = resolve_path(it, name);
    let tried = if extension(name).is_empty() && !path.exists() {
        resolve_path(it, &format!("{}.wav", name))
    } else {
        path
    };
    std::fs::read(&tried).map_err(|_| {
        fail(
            fname,
            "fileNotFound",
            format!(
                "{}: the file '{}' does not exist or cannot be read.",
                fname, name
            ),
        )
    })
}

/// Raw sample `i` of the data chunk, as stored.
fn raw_sample(w: &Wave<'_>, i: usize) -> f64 {
    let b = w.data;
    let n = w.encoding.bytes();
    let s = &b[i * n..(i + 1) * n];
    match w.encoding {
        Encoding::U8 => s[0] as f64,
        Encoding::I16 => i16::from_le_bytes([s[0], s[1]]) as f64,
        Encoding::I24 => (i32::from_le_bytes([0, s[0], s[1], s[2]]) >> 8) as f64,
        Encoding::I32 => i32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f64,
        Encoding::F32 => f32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f64,
        Encoding::F64 => f64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]),
    }
}

fn audioread(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let fname = "audioread";
    need(&a, 1, fname)?;
    let name = file_name_arg(&a[0], fname)?;
    let mut range: Option<&Value> = None;
    let mut native = false;
    for v in &a[1..] {
        if v.is_text() {
            match v.to_str().unwrap_or_default().to_lowercase().as_str() {
                "native" => native = true,
                "double" => native = false,
                other => {
                    return Err(fail(
                        fname,
                        "invalidDataType",
                        format!(
                            "audioread: data type '{}' must be 'double' or 'native'.",
                            other
                        ),
                    ));
                }
            }
        } else if range.is_none() {
            range = Some(v);
        } else {
            return Err(fail(
                fname,
                "invalidArgs",
                "audioread: too many sample ranges.",
            ));
        }
    }
    let bytes = load(it, fname, &name)?;
    let w = parse_wave(&bytes).map_err(|m| {
        fail(
            fname,
            "invalidFile",
            format!("audioread: {} ('{}').", m, name),
        )
    })?;
    let (first, last) = sample_range(range, w.frames)?;
    let frames = last - first;
    let n = frames * w.channels;
    let mut re = vec![0.0; n];
    for f in 0..frames {
        for c in 0..w.channels {
            let x = raw_sample(&w, (first + f) * w.channels + c);
            re[f + c * frames] = if native {
                if w.encoding == Encoding::I24 {
                    x * 256.0
                } else {
                    x
                }
            } else {
                match w.encoding {
                    Encoding::U8 => (x - 128.0) / 128.0,
                    e => x / e.scale(),
                }
            };
        }
    }
    let class = if !native {
        NumClass::Double
    } else {
        match w.encoding {
            Encoding::U8 => NumClass::UInt8,
            Encoding::I16 => NumClass::Int16,
            Encoding::I24 | Encoding::I32 => NumClass::Int32,
            Encoding::F32 => NumClass::Single,
            Encoding::F64 => NumClass::Double,
        }
    };
    let y = Value::Num(Num {
        class,
        dims: norm_dims(vec![frames, w.channels]),
        re,
        im: None,
    });
    Ok(vec![y, Value::num(w.rate as f64)])
}

/// Zero-based half-open frame range from `[start finish]`.
fn sample_range(range: Option<&Value>, total: usize) -> R<(usize, usize)> {
    let bad = || {
        fail(
            "audioread",
            "invalidRange",
            format!(
                "audioread: the sample range must be [start finish] with 1 <= start <= finish <= {}.",
                total
            ),
        )
    };
    let Some(v) = range else {
        return Ok((0, total));
    };
    let x = v.to_f64_vec().ok_or_else(bad)?;
    let [s, e] = x.as_slice() else {
        return Err(bad());
    };
    let whole = |t: f64| t.fract() == 0.0 && t >= 1.0;
    if !s.is_finite() || !whole(*s) || !(whole(*e) || *e == f64::INFINITY) || s > e {
        return Err(bad());
    }
    let finish = if e.is_infinite() { total as f64 } else { *e };
    if *s > total as f64 || finish > total as f64 {
        return Err(bad());
    }
    Ok((*s as usize - 1, finish as usize))
}

fn audioinfo(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let fname = "audioinfo";
    need(&a, 1, fname)?;
    let name = file_name_arg(&a[0], fname)?;
    let bytes = load(it, fname, &name)?;
    let w = parse_wave(&bytes).map_err(|m| {
        fail(
            fname,
            "invalidFile",
            format!("audioinfo: {} ('{}').", m, name),
        )
    })?;
    let text = |t: &Option<String>| t.as_deref().map_or(Value::empty(), Value::str);
    let mut s = StructArray::new_scalar();
    s.set(
        0,
        "Filename",
        Value::str(&resolve_path(it, &name).display().to_string()),
    );
    s.set(0, "CompressionMethod", Value::str("Uncompressed"));
    s.set(0, "NumChannels", Value::num(w.channels as f64));
    s.set(0, "SampleRate", Value::num(w.rate as f64));
    s.set(0, "TotalSamples", Value::num(w.frames as f64));
    let duration = if w.rate > 0 {
        w.frames as f64 / w.rate as f64
    } else {
        0.0
    };
    s.set(0, "Duration", Value::num(duration));
    s.set(0, "Title", text(&w.title));
    s.set(0, "Comment", text(&w.comment));
    s.set(0, "Artist", text(&w.artist));
    s.set(0, "BitsPerSample", Value::num(w.bits as f64));
    Ok(vec![Value::Struct(s)])
}
