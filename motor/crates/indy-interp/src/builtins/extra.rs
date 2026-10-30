//! Extra builtins measured as frequent "Undefined function" causes:
//! matrix structure predicates, typecast/swapbytes,
//! binary file I/O (fread/fwrite/fseek/ftell), path search, time and date
//! helpers, string escapes, and numerics (eig, expm, rank, filter, conv,
//! roots, fzero, fminbnd, integral, histcounts, ...).
//!
//! All linear algebra here is own code (Jacobi,
//! Francis QR, Padé), written from the textbook algorithms.

#![allow(
    clippy::needless_range_loop,
    clippy::too_many_arguments,
    clippy::type_complexity
)]

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use indy_value::ops::{C, c_abs, c_div, c_mul, c_sqrt};
use indy_value::{Array, Num, NumClass, StructArray, Value, norm_dims, numel_of};

use super::core::resolve_path;
use super::math::{default_dim, reduced_dims, slices, take};
use super::{OpenFile, arg_f64, arg_num, arg_str, arg_usize, err, need, one, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    // structure predicates
    reg(it, "isdiag", |_, a, _| structure(&a, "isdiag", 0));
    reg(it, "istril", |_, a, _| structure(&a, "istril", 1));
    reg(it, "istriu", |_, a, _| structure(&a, "istriu", 2));
    reg(it, "issymmetric", |_, a, _| {
        symmetric(&a, "issymmetric", false)
    });
    reg(it, "ishermitian", |_, a, _| {
        symmetric(&a, "ishermitian", true)
    });
    // bytes
    reg(it, "typecast", typecast);
    reg(it, "swapbytes", swapbytes);
    // binary file I/O
    reg(it, "fread", fread);
    reg(it, "fwrite", fwrite);
    reg(it, "fseek", fseek);
    reg(it, "ftell", ftell);
    // paths and files
    reg(it, "file_in_path", file_in_path);
    reg(it, "file_in_loadpath", file_in_loadpath);
    reg(it, "mkstemp", mkstemp);
    // misc
    reg(it, "nextpow2", nextpow2);
    reg(it, "lookup", lookup);
    reg(it, "unicode2native", unicode2native);
    reg(it, "native2unicode", native2unicode);
    reg(it, "do_string_escapes", do_string_escapes);
    reg(it, "undo_string_escapes", undo_string_escapes);
    reg(it, "dbstack", dbstack);
    // time
    reg(it, "clock", |_, _, _| {
        let t = local_now();
        one(Value::row(vec![
            t.0 as f64, t.1 as f64, t.2 as f64, t.3 as f64, t.4 as f64, t.5,
        ]))
    });
    reg(it, "cputime", |_, _, _| {
        one(Value::num(sys::cpu_seconds().unwrap_or(0.0)))
    });
    reg(it, "now", |_, _, _| {
        let t = local_now();
        one(Value::num(datenum_of(
            t.0 as f64, t.1 as f64, t.2 as f64, t.3 as f64, t.4 as f64, t.5,
        )))
    });
    reg(it, "date", |_, _, _| {
        let t = local_now();
        let m = MONTHS[(t.1 as usize).saturating_sub(1).min(11)];
        one(Value::str(&format!("{:02}-{}-{:04}", t.2, m, t.0)))
    });
    reg(it, "pause", pause);
    reg(it, "datenum", datenum);
    // numerics
    reg(it, "factor", factor);
    reg(it, "rank", rank);
    reg(it, "eig", eig);
    reg(it, "expm", expm);
    reg(it, "vecnorm", vecnorm);
    reg(it, "mode", mode);
    reg(it, "histcounts", histcounts);
    reg(it, "gradient", gradient);
    reg(it, "filter", filter);
    reg(it, "conv", conv);
    reg(it, "deconv", deconv);
    reg(it, "polyval", polyval);
    reg(it, "roots", roots);
    reg(it, "poly", poly);
    reg(it, "trapz", |_, a, _| trapz(&a, "trapz", false));
    reg(it, "cumtrapz", |_, a, _| trapz(&a, "cumtrapz", true));
    reg(it, "fzero", fzero);
    reg(it, "optimset", optimset);
    reg(it, "optimget", optimget);
    reg(it, "fminbnd", fminbnd);
    reg(it, "rescale", rescale);
    reg(it, "bounds", bounds);
    reg(it, "movmean", |_, a, _| movfun(&a, "movmean", true));
    reg(it, "movsum", |_, a, _| movfun(&a, "movsum", false));
    reg(it, "rmmissing", rmmissing);
    reg(it, "integral", |it, a, n| quad_entry(it, a, n, "integral"));
    reg(it, "quadgk", |it, a, n| quad_entry(it, a, n, "quadgk"));
    reg(it, "quad", |it, a, n| quad_entry(it, a, n, "quad"));
    reg(it, "quadl", |it, a, n| quad_entry(it, a, n, "quadl"));
    reg(it, "quadv", |it, a, n| quad_entry(it, a, n, "quadv"));
}

// ---------------------------------------------------------------------------
// small helpers
// ---------------------------------------------------------------------------

fn cdata(x: &Num) -> Vec<C> {
    (0..x.re.len()).map(|i| (x.re[i], x.im_at(i))).collect()
}

fn cvalue(dims: Vec<usize>, d: &[C]) -> Value {
    Value::Num(Num::complex(
        dims,
        d.iter().map(|c| c.0).collect(),
        d.iter().map(|c| c.1).collect(),
    ))
}

fn cadd(a: C, b: C) -> C {
    (a.0 + b.0, a.1 + b.1)
}
fn csub(a: C, b: C) -> C {
    (a.0 - b.0, a.1 - b.1)
}
fn cscale(a: C, s: f64) -> C {
    (a.0 * s, a.1 * s)
}
fn cconj(a: C) -> C {
    (a.0, -a.1)
}
const CZERO: C = (0.0, 0.0);

fn is_vector_dims(d: &[usize]) -> bool {
    d.len() == 2 && (d[0] == 1 || d[1] == 1)
}

fn dim_arg(v: &Value, fname: &str) -> R<usize> {
    let k = arg_f64(v, fname)?;
    if k < 1.0 || k.fract() != 0.0 || !k.is_finite() {
        return Err(err(
            "MATLAB:getdimarg:dimensionMustBePositiveInteger",
            "Dimension argument must be a positive integer scalar within indexing range.",
        ));
    }
    Ok(k as usize - 1)
}

/// Spacing of doubles at `x` (MATLAB `eps(x)`).
fn eps_of(x: f64) -> f64 {
    let x = x.abs();
    if !x.is_finite() {
        return f64::NAN;
    }
    if x < f64::MIN_POSITIVE {
        return f64::from_bits(1);
    }
    let e = ((x.to_bits() >> 52) & 0x7ff) as i32 - 1023;
    2f64.powi(e - 52)
}

/// Apply `f` to every 1-D slice of `data` along `dim`; each result slice has
/// `out_len` elements. `f` gets the slice number and the input slice.
fn map_slices(
    dims: &[usize],
    data: &[C],
    dim: usize,
    out_len: usize,
    mut f: impl FnMut(usize, &[C]) -> Vec<C>,
) -> (Vec<usize>, Vec<C>) {
    let mut d = dims.to_vec();
    while d.len() <= dim {
        d.push(1);
    }
    let len = d[dim];
    let stride: usize = d[..dim].iter().product();
    let outer: usize = d[dim + 1..].iter().product();
    let mut od = d.clone();
    od[dim] = out_len;
    let mut out = vec![CZERO; stride * out_len * outer];
    let mut buf = Vec::with_capacity(len);
    let mut s = 0;
    for o in 0..outer {
        for i in 0..stride {
            buf.clear();
            for k in 0..len {
                buf.push(data[o * stride * len + i + k * stride]);
            }
            let r = f(s, &buf);
            s += 1;
            for k in 0..out_len.min(r.len()) {
                out[o * stride * out_len + i + k * stride] = r[k];
            }
        }
    }
    (norm_dims(od), out)
}

fn make_struct(fields: Vec<(&str, Value)>) -> Value {
    let mut s = StructArray::new_scalar();
    for (k, v) in fields {
        s.set(0, k, v);
    }
    Value::Struct(s)
}

fn take_n(v: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    Ok(v.into_iter().take(nargout.max(1)).collect())
}

fn call1(it: &mut Interp, f: &Value, args: Vec<Value>, fname: &str) -> R<Value> {
    let mut r = it.call_value(f, args, 1)?;
    if r.is_empty() {
        return Err(err(
            &format!("MATLAB:{}:noOutput", fname),
            format!("{}: the function did not return a value.", fname),
        ));
    }
    Ok(r.swap_remove(0))
}

fn call_scalar(it: &mut Interp, f: &Value, x: f64, extra: &[Value], fname: &str) -> R<f64> {
    let mut args = vec![Value::num(x)];
    args.extend(extra.iter().cloned());
    let v = call1(it, f, args, fname)?;
    match (&v, v.scalar_f64()) {
        (Value::Num(n), Some(y)) if n.im.is_none() => Ok(y),
        (Value::Bool(_), Some(y)) => Ok(y),
        (Value::Sparse(s), Some(y)) if s.im.is_none() => Ok(y),
        _ => Err(err(
            &format!("MATLAB:{}:checkfun:NonScalarOrComplexFun", fname),
            format!("{}: the function must return a real scalar value.", fname),
        )),
    }
}

// ---------------------------------------------------------------------------
// structure predicates
// ---------------------------------------------------------------------------

fn structure(a: &[Value], fname: &str, kind: u8) -> R<Vec<Value>> {
    need(a, 1, fname)?;
    let x = arg_num(&a[0], fname)?;
    if x.dims.len() != 2 {
        return one(Value::bool(false));
    }
    let r = x.dims[0];
    for k in 0..x.re.len() {
        if x.re[k] == 0.0 && x.im_at(k) == 0.0 {
            continue;
        }
        let (i, j) = (k % r, k / r);
        let bad = match kind {
            0 => i != j,
            1 => j > i,
            _ => i > j,
        };
        if bad {
            return one(Value::bool(false));
        }
    }
    one(Value::bool(true))
}

fn symmetric(a: &[Value], fname: &str, herm: bool) -> R<Vec<Value>> {
    need(a, 1, fname)?;
    let x = arg_num(&a[0], fname)?;
    let mut skew = false;
    let mut tol = 0.0;
    for v in &a[1..] {
        if let Some(s) = v.to_str() {
            match s.to_ascii_lowercase().as_str() {
                "skew" => skew = true,
                "nonskew" => skew = false,
                other => {
                    return Err(err(
                        &format!("MATLAB:{}:invalidOption", fname),
                        format!(
                            "{}: option must be 'skew' or 'nonskew' (got '{}').",
                            fname, other
                        ),
                    ));
                }
            }
        } else {
            tol = arg_f64(v, fname)?;
            if tol.is_nan() || tol < 0.0 {
                return Err(err(
                    &format!("MATLAB:{}:invalidTolerance", fname),
                    format!("{}: tolerance must be a non-negative real scalar.", fname),
                ));
            }
        }
    }
    if x.dims.len() != 2 || x.dims[0] != x.dims[1] {
        return one(Value::bool(false));
    }
    let n = x.dims[0];
    let sgn = if skew { -1.0 } else { 1.0 };
    let mut row_diff = vec![0.0; n];
    let mut row_abs = vec![0.0; n];
    for j in 0..n {
        for i in 0..n {
            let p = (x.re[i + j * n], x.im_at(i + j * n));
            let q = (x.re[j + i * n], x.im_at(j + i * n));
            let q = if herm { cconj(q) } else { q };
            let d = csub(p, cscale(q, sgn));
            if tol == 0.0 {
                if d.0 != 0.0 || d.1 != 0.0 {
                    return one(Value::bool(false));
                }
            } else {
                row_diff[i] += c_abs(d);
                row_abs[i] += c_abs(p);
            }
        }
    }
    if tol == 0.0 {
        return one(Value::bool(true));
    }
    let nd = row_diff.iter().cloned().fold(0.0, f64::max);
    let na = row_abs.iter().cloned().fold(0.0, f64::max);
    one(Value::bool(nd <= tol * na))
}

// ---------------------------------------------------------------------------
// typecast / swapbytes
// ---------------------------------------------------------------------------

fn class_size(c: NumClass) -> usize {
    match c {
        NumClass::Int8 | NumClass::UInt8 => 1,
        NumClass::Int16 | NumClass::UInt16 => 2,
        NumClass::Int32 | NumClass::UInt32 | NumClass::Single => 4,
        _ => 8,
    }
}

/// Append the bytes of `x` stored as class `c` (value is converted first).
fn encode(c: NumClass, x: f64, be: bool, out: &mut Vec<u8>) {
    let start = out.len();
    let x = c.cast(x);
    match c {
        NumClass::Double => out.extend_from_slice(&x.to_le_bytes()),
        NumClass::Single => out.extend_from_slice(&(x as f32).to_le_bytes()),
        NumClass::Int8 => out.extend_from_slice(&(x as i8).to_le_bytes()),
        NumClass::Int16 => out.extend_from_slice(&(x as i16).to_le_bytes()),
        NumClass::Int32 => out.extend_from_slice(&(x as i32).to_le_bytes()),
        NumClass::Int64 => out.extend_from_slice(&(x as i64).to_le_bytes()),
        NumClass::UInt8 => out.extend_from_slice(&(x as u8).to_le_bytes()),
        NumClass::UInt16 => out.extend_from_slice(&(x as u16).to_le_bytes()),
        NumClass::UInt32 => out.extend_from_slice(&(x as u32).to_le_bytes()),
        NumClass::UInt64 => out.extend_from_slice(&(x as u64).to_le_bytes()),
    }
    if be {
        out[start..].reverse();
    }
}

fn decode(c: NumClass, b: &[u8], be: bool) -> f64 {
    let k = class_size(c);
    let mut buf = [0u8; 8];
    buf[..k].copy_from_slice(&b[..k]);
    if be {
        buf[..k].reverse();
    }
    let b2 = [buf[0], buf[1]];
    let b4 = [buf[0], buf[1], buf[2], buf[3]];
    match c {
        NumClass::Double => f64::from_le_bytes(buf),
        NumClass::Single => f32::from_le_bytes(b4) as f64,
        NumClass::Int8 => buf[0] as i8 as f64,
        NumClass::UInt8 => buf[0] as f64,
        NumClass::Int16 => i16::from_le_bytes(b2) as f64,
        NumClass::UInt16 => u16::from_le_bytes(b2) as f64,
        NumClass::Int32 => i32::from_le_bytes(b4) as f64,
        NumClass::UInt32 => u32::from_le_bytes(b4) as f64,
        NumClass::Int64 => i64::from_le_bytes(buf) as f64,
        NumClass::UInt64 => u64::from_le_bytes(buf) as f64,
    }
}

fn typecast(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "typecast")?;
    let bad = || {
        err(
            "MATLAB:typecast:notNumeric",
            "The first input argument must be a full, non-complex numeric value.",
        )
    };
    let Value::Num(x) = &a[0] else {
        return Err(bad());
    };
    if x.im.is_some() {
        return Err(bad());
    }
    let cname = arg_str(&a[1], "typecast")?;
    let to = NumClass::from_name(&cname.to_ascii_lowercase()).ok_or_else(|| {
        err(
            "MATLAB:typecast:UnsupportedClass",
            format!("Unsupported class '{}'.", cname),
        )
    })?;
    if !(x.dims.len() == 2 && (x.dims[0] <= 1 || x.dims[1] <= 1)) {
        return Err(err(
            "MATLAB:typecast:inputMustBeVector",
            "The first input argument must be a vector.",
        ));
    }
    let mut bytes = Vec::with_capacity(x.re.len() * class_size(x.class));
    for v in &x.re {
        encode(x.class, *v, false, &mut bytes);
    }
    let k = class_size(to);
    if bytes.len() % k != 0 {
        return Err(err(
            "MATLAB:typecast:TooFewInputValues",
            "Too few input values to make output type.",
        ));
    }
    let vals: Vec<f64> = bytes.chunks(k).map(|c| decode(to, c, false)).collect();
    let n = vals.len();
    let dims = if x.re.is_empty() {
        x.dims.clone()
    } else if x.dims[1] == 1 && x.dims[0] > 1 {
        vec![n, 1]
    } else {
        vec![1, n]
    };
    one(Value::Num(Num {
        class: to,
        dims: norm_dims(dims),
        re: vals,
        im: None,
    }))
}

fn swapbytes(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "swapbytes")?;
    let Value::Num(x) = &a[0] else {
        return Err(err(
            "MATLAB:swapbytes:invalidInput",
            "Input must be a numeric array.",
        ));
    };
    let c = x.class;
    let swap = |v: &[f64]| -> Vec<f64> {
        v.iter()
            .map(|e| {
                let mut b = Vec::with_capacity(8);
                encode(c, *e, false, &mut b);
                decode(c, &b, true)
            })
            .collect()
    };
    let mut out = x.clone();
    out.re = swap(&x.re);
    if let Some(im) = &x.im {
        out.im = Some(swap(im));
    }
    one(Value::Num(out))
}

// ---------------------------------------------------------------------------
// binary file I/O
// ---------------------------------------------------------------------------

fn file_mut<'a>(it: &'a mut Interp, v: &Value, fname: &str) -> R<&'a mut OpenFile> {
    let id = arg_f64(v, fname)?;
    it.files_open.get_mut(&(id as i32)).ok_or_else(|| {
        err(
            "MATLAB:badfid_mx",
            "Invalid file identifier. Use fopen to generate a valid file identifier.",
        )
    })
}

fn read_bytes(f: &mut OpenFile, n: usize) -> Vec<u8> {
    match &mut f.writer {
        Some(w) => {
            let mut buf = Vec::new();
            let _ = (&mut *w)
                .take(n.min(u64::MAX as usize) as u64)
                .read_to_end(&mut buf);
            buf
        }
        None => {
            let start = f.pos.min(f.data.len());
            let end = start.saturating_add(n).min(f.data.len());
            f.pos = f.pos.max(end);
            f.data[start..end].to_vec()
        }
    }
}

fn file_len(f: &OpenFile) -> u64 {
    match &f.writer {
        Some(w) => w.metadata().map(|m| m.len()).unwrap_or(0),
        None => f.data.len() as u64,
    }
}

/// origin: 0 = bof, 1 = cof, 2 = eof.
fn seek_to(f: &mut OpenFile, off: i64, origin: u8) -> bool {
    let base = match origin {
        0 => 0,
        1 => f.position() as i64,
        _ => file_len(f) as i64,
    };
    let target = base.saturating_add(off);
    if target < 0 {
        return false;
    }
    match &mut f.writer {
        Some(w) => w.seek(SeekFrom::Start(target as u64)).is_ok(),
        None => {
            if target as u64 > f.data.len() as u64 {
                return false;
            }
            f.pos = target as usize;
            true
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Elt {
    N(NumClass),
    Char,
}

impl Elt {
    fn storage(self) -> NumClass {
        match self {
            Elt::N(c) => c,
            Elt::Char => NumClass::UInt8,
        }
    }
    fn size(self) -> usize {
        class_size(self.storage())
    }
}

fn elt_name(s: &str) -> Option<Elt> {
    use NumClass::*;
    Some(match s.trim() {
        "uint8" | "uchar" | "unsigned char" => Elt::N(UInt8),
        "int8" | "schar" | "signed char" | "integer*1" => Elt::N(Int8),
        "uint16" | "ushort" | "unsigned short" => Elt::N(UInt16),
        "int16" | "short" | "integer*2" => Elt::N(Int16),
        "uint32" | "uint" | "ulong" | "unsigned int" | "unsigned long" => Elt::N(UInt32),
        "int32" | "int" | "long" | "integer*4" => Elt::N(Int32),
        "uint64" => Elt::N(UInt64),
        "int64" | "integer*8" => Elt::N(Int64),
        "single" | "float32" | "float" | "real*4" => Elt::N(Single),
        "double" | "float64" | "real*8" => Elt::N(Double),
        "char" | "char*1" => Elt::Char,
        _ => return None,
    })
}

/// Parse a precision string into (source element type, output type).
fn parse_precision(s: &str, fname: &str) -> R<(Elt, Elt)> {
    let low = s.to_ascii_lowercase();
    let bad = || {
        err(
            &format!("MATLAB:{}:InvalidPrecision", fname),
            format!("{}: invalid or unsupported precision '{}'.", fname, s),
        )
    };
    let name = |t: &str| elt_name(t).ok_or_else(bad);
    if let Some(rest) = low.strip_prefix('*') {
        let e = name(rest)?;
        return Ok((e, e));
    }
    if let Some((l, r)) = low.split_once("=>") {
        return Ok((name(l)?, name(r)?));
    }
    Ok((name(&low)?, Elt::N(NumClass::Double)))
}

pub(super) fn machine_be(s: &str, fname: &str) -> R<bool> {
    match s.to_ascii_lowercase().as_str() {
        "n" | "native" | "l" | "ieee-le" | "ieee-le.l64" | "a" | "s" => Ok(false),
        "b" | "ieee-be" | "ieee-be.l64" => Ok(true),
        other => Err(err(
            &format!("MATLAB:{}:InvalidMachineFormat", fname),
            format!("{}: invalid machine format '{}'.", fname, other),
        )),
    }
}

fn fread(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "fread")?;
    let mut i = 1;
    let mut size: Option<Vec<f64>> = None;
    if let Some(v) = a.get(1)
        && !v.is_char()
    {
        size = Some(arg_num(v, "fread")?.re);
        i = 2;
    }
    let mut prec = "uint8".to_string();
    if let Some(v) = a.get(i)
        && v.is_char()
    {
        prec = arg_str(v, "fread")?;
        i += 1;
    }
    let mut skip = 0;
    if let Some(v) = a.get(i)
        && !v.is_char()
    {
        skip = arg_usize(v, "fread")?;
        i += 1;
    }
    let be = match a.get(i) {
        Some(v) => Some(machine_be(&arg_str(v, "fread")?, "fread")?),
        None => None,
    };
    // size: None/[]/Inf -> all as column; n -> up to n; [m n] -> m rows
    let bad_size = || {
        err(
            "MATLAB:fread:invalidSize",
            "fread: SIZE must be Inf, a non-negative integer, or [M N].",
        )
    };
    let to_count = |x: f64| -> R<usize> {
        if x.is_nan() || x < 0.0 || (x.is_finite() && x.fract() != 0.0) {
            return Err(bad_size());
        }
        Ok(if x.is_infinite() {
            usize::MAX
        } else {
            x as usize
        })
    };
    let (limit, rows): (usize, Option<usize>) = match size.as_deref() {
        None | Some([]) => (usize::MAX, None),
        Some([n]) => (to_count(*n)?, None),
        Some([m, n]) => {
            if m.is_infinite() {
                return Err(bad_size());
            }
            let m = to_count(*m)?;
            let n = to_count(*n)?;
            (m.saturating_mul(n), Some(m))
        }
        _ => return Err(bad_size()),
    };
    let (src, dst) = parse_precision(&prec, "fread")?;
    let es = src.size();
    let f = file_mut(it, &a[0], "fread")?;
    let be = be.unwrap_or(f.big_endian);
    let mut vals = Vec::new();
    // `char` ('char', '*char', 'char=>char', 'char*1') is text: elements are
    // characters decoded in the file's encoding, SIZE counts characters, and a
    // BOM stays visible as char(65279). `uchar` and `uint8` are bytes, so
    // 'uchar=>char' deliberately gives one char per byte.
    if src == Elt::Char {
        let enc = f.enc;
        let rest = f.rest();
        let mut used = 0;
        while vals.len() < limit && used < rest.len() {
            let (c, n) = enc.next_char(&rest[used..]);
            vals.push(c as u32 as f64);
            used = (used + n).saturating_add(skip).min(rest.len());
        }
        f.advance(used);
    } else if skip == 0 {
        let bytes = read_bytes(f, limit.saturating_mul(es));
        for c in bytes.chunks_exact(es) {
            vals.push(decode(src.storage(), c, be));
        }
    } else {
        while vals.len() < limit {
            let b = read_bytes(f, es);
            if b.len() < es {
                break;
            }
            vals.push(decode(src.storage(), &b, be));
            if !seek_to(f, skip as i64, 1) {
                break;
            }
        }
    }
    let count = vals.len();
    let dims = match rows {
        None => {
            if count == 0 {
                vec![0, 0]
            } else {
                vec![count, 1]
            }
        }
        Some(0) => vec![0, 0],
        Some(m) => {
            let cols = count.div_ceil(m);
            vals.resize(m * cols, 0.0);
            vec![m, cols]
        }
    };
    let out = match dst {
        Elt::Char => Value::Char(Array::new(
            dims,
            vals.iter()
                .map(|v| char::from_u32(*v as u32).unwrap_or('\u{FFFD}'))
                .collect(),
        )),
        Elt::N(NumClass::Double) => Value::Num(Num::real(dims, vals)),
        Elt::N(c) => Value::Num(Num::real(dims, vals).with_class(c)),
    };
    take_n(vec![out, Value::num(count as f64)], nargout)
}

/// The character for a numeric element written with `char` precision.
fn text_char(v: f64) -> char {
    char::from_u32(v.round() as u32).unwrap_or('\u{FFFD}')
}

fn fwrite(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "fwrite")?;
    let data = match &a[1] {
        v @ Value::Str(_) => arg_num(&Value::str(&arg_str(v, "fwrite")?), "fwrite")?,
        v => arg_num(v, "fwrite")?,
    };
    let prec = match a.get(2) {
        Some(v) if !v.is_empty() => arg_str(v, "fwrite")?,
        _ => "uint8".to_string(),
    };
    let skip = match a.get(3) {
        Some(v) => arg_usize(v, "fwrite")?,
        None => 0,
    };
    let be = match a.get(4) {
        Some(v) => Some(machine_be(&arg_str(v, "fwrite")?, "fwrite")?),
        None => None,
    };
    let (src, _) = parse_precision(&prec, "fwrite")?;
    let st = src.storage();
    let id = arg_f64(&a[0], "fwrite")? as i32;
    if id == 1 || id == 2 {
        let s = if src == Elt::Char {
            data.re.iter().map(|v| text_char(*v)).collect()
        } else {
            let mut bytes = Vec::new();
            for v in &data.re {
                encode(st, *v, be.unwrap_or(false), &mut bytes);
            }
            String::from_utf8_lossy(&bytes).to_string()
        };
        if id == 1 {
            it.print(&s);
        } else {
            it.eprint(&s);
        }
        return one(Value::num(data.re.len() as f64));
    }
    let f = file_mut(it, &a[0], "fwrite")?;
    if f.writer.is_none() {
        return one(Value::num(0.0));
    }
    let be = be.unwrap_or(f.big_endian);
    let enc = f.enc;
    let mut count = 0usize;
    let mut buf = Vec::with_capacity(8);
    for v in &data.re {
        if skip > 0 && !seek_to(f, skip as i64, 1) {
            break;
        }
        buf.clear();
        // The count returned is in elements (characters), not bytes.
        if src == Elt::Char {
            enc.push_char(text_char(*v), &mut buf);
        } else {
            encode(st, *v, be, &mut buf);
        }
        let Some(w) = &mut f.writer else { break };
        if w.write_all(&buf).is_err() {
            break;
        }
        count += 1;
    }
    one(Value::num(count as f64))
}

fn fseek(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "fseek")?;
    let off = arg_f64(&a[1], "fseek")?;
    if !off.is_finite() || off.fract() != 0.0 {
        return Err(err(
            "MATLAB:fseek:invalidOffset",
            "fseek: offset must be an integer.",
        ));
    }
    let origin = match a.get(2) {
        None => 0,
        Some(v) => match v.to_str() {
            Some(s) => match s.to_ascii_lowercase().as_str() {
                "bof" => 0,
                "cof" => 1,
                "eof" => 2,
                _ => {
                    return Err(err(
                        "MATLAB:fseek:invalidOrigin",
                        "fseek: ORIGIN must be 'bof', 'cof' or 'eof'.",
                    ));
                }
            },
            None => match arg_f64(v, "fseek")? as i64 {
                -1 => 0,
                0 => 1,
                1 => 2,
                _ => {
                    return Err(err(
                        "MATLAB:fseek:invalidOrigin",
                        "fseek: ORIGIN must be -1, 0 or 1.",
                    ));
                }
            },
        },
    };
    let f = file_mut(it, &a[0], "fseek")?;
    let ok = seek_to(f, off as i64, origin);
    one(Value::num(if ok { 0.0 } else { -1.0 }))
}

fn ftell(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "ftell")?;
    let f = file_mut(it, &a[0], "ftell")?;
    one(Value::num(f.position() as f64))
}

// ---------------------------------------------------------------------------
// paths
// ---------------------------------------------------------------------------

fn file_names(v: &Value, fname: &str) -> R<Vec<String>> {
    match v {
        Value::Cell(c) => c.data.iter().map(|e| arg_str(e, fname)).collect(),
        _ => Ok(vec![arg_str(v, fname)?]),
    }
}

fn all_flag(v: Option<&Value>, fname: &str) -> R<bool> {
    match v {
        None => Ok(false),
        Some(v) => {
            let s = arg_str(v, fname)?;
            if s.eq_ignore_ascii_case("all") {
                Ok(true)
            } else {
                Err(err(
                    &format!("MATLAB:{}:invalidOption", fname),
                    format!("{}: optional third argument must be \"all\".", fname),
                ))
            }
        }
    }
}

fn path_search(dirs: &[PathBuf], files: &[String], all: bool) -> Value {
    let mut found = Vec::new();
    'outer: for d in dirs {
        for f in files {
            let p = d.join(f);
            if p.exists() {
                found.push(p.display().to_string());
                if !all {
                    break 'outer;
                }
            }
        }
    }
    if all {
        if found.is_empty() {
            return Value::empty_cell();
        }
        return Value::cell_col(found.iter().map(|s| Value::str(s)).collect());
    }
    match found.first() {
        Some(s) => Value::str(s),
        None => Value::empty(),
    }
}

fn file_in_path(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "file_in_path")?;
    let path = arg_str(&a[0], "file_in_path")?;
    let files = file_names(&a[1], "file_in_path")?;
    let all = all_flag(a.get(2), "file_in_path")?;
    let dirs: Vec<PathBuf> = path
        .split(':')
        .filter(|s| !s.is_empty())
        .map(|s| resolve_path(it, s))
        .collect();
    one(path_search(&dirs, &files, all))
}

fn file_in_loadpath(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "file_in_loadpath")?;
    let files = file_names(&a[0], "file_in_loadpath")?;
    let all = all_flag(a.get(1), "file_in_loadpath")?;
    if let [f] = files.as_slice()
        && PathBuf::from(f).is_absolute()
    {
        let p = PathBuf::from(f);
        return one(if p.exists() {
            if all {
                Value::cell_col(vec![Value::str(f)])
            } else {
                Value::str(f)
            }
        } else if all {
            Value::empty_cell()
        } else {
            Value::empty()
        });
    }
    let mut dirs = vec![it.cwd.clone()];
    for p in &it.path {
        if !dirs.contains(p) {
            dirs.push(p.clone());
        }
    }
    one(path_search(&dirs, &files, all))
}

fn rand_suffix(salt: u64) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static CTR: AtomicU64 = AtomicU64::new(0);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut x = t
        ^ ((std::process::id() as u64) << 32)
        ^ CTR
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ salt.wrapping_mul(0xD1B5_4A32_D192_ED03);
    if x == 0 {
        x = 0x2545_F491_4F6C_DD1D;
    }
    const CH: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    (0..6)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            CH[(x % 62) as usize] as char
        })
        .collect()
}

fn mkstemp(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "mkstemp")?;
    let template = arg_str(&a[0], "mkstemp")?;
    let fail = |msg: &str| {
        take_n(
            vec![Value::num(-1.0), Value::str(""), Value::str(msg)],
            nargout,
        )
    };
    let Some(base) = template.strip_suffix("XXXXXX") else {
        return fail("mkstemp: TEMPLATE argument must end with XXXXXX");
    };
    for attempt in 0..100u64 {
        let name = format!("{}{}", base, rand_suffix(attempt));
        let path = resolve_path(it, &name);
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => {
                let mut id = 3;
                while it.files_open.contains_key(&id) {
                    id += 1;
                }
                it.files_open.insert(
                    id,
                    OpenFile {
                        name: name.clone(),
                        path,
                        mode: "w+b".into(),
                        data: vec![],
                        pos: 0,
                        writer: Some(file),
                        enc: super::io::TextEnc::Utf8,
                        big_endian: false,
                    },
                );
                return take_n(
                    vec![Value::num(id as f64), Value::str(&name), Value::str("")],
                    nargout,
                );
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return fail(&format!("mkstemp: {}", e)),
        }
    }
    fail("mkstemp: could not create a unique file name")
}

// ---------------------------------------------------------------------------
// misc
// ---------------------------------------------------------------------------

fn np2(v: f64) -> f64 {
    let a = v.abs();
    if a.is_nan() {
        return f64::NAN;
    }
    if a.is_infinite() {
        return f64::INFINITY;
    }
    if a == 0.0 {
        return 0.0;
    }
    let mut p = a.log2().ceil();
    if 2f64.powf(p - 1.0) >= a {
        p -= 1.0;
    } else if 2f64.powf(p) < a {
        p += 1.0;
    }
    p
}

fn nextpow2(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "nextpow2")?;
    let x = arg_num(&a[0], "nextpow2")?;
    let re: Vec<f64> = (0..x.re.len())
        .map(|i| np2(c_abs((x.re[i], x.im_at(i)))))
        .collect();
    let class = if matches!(a[0], Value::Num(_)) {
        x.class
    } else {
        NumClass::Double
    };
    one(Value::Num(Num::real(x.dims.clone(), re).with_class(class)))
}

fn lookup(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "lookup")?;
    let opt = match a.get(2) {
        Some(v) => arg_str(v, "lookup")?,
        None => String::new(),
    };
    let (om, ob, ol, or) = (
        opt.contains('m'),
        opt.contains('b'),
        opt.contains('l'),
        opt.contains('r'),
    );
    // (index, exact match) per query
    let (dims, res, n): (Vec<usize>, Vec<(usize, bool)>, usize) = if let Value::Cell(t) = &a[0] {
        let table: Vec<String> = t
            .data
            .iter()
            .map(|v| arg_str(v, "lookup"))
            .collect::<R<_>>()?;
        let (dims, ys): (Vec<usize>, Vec<String>) = match &a[1] {
            Value::Cell(c) => (
                c.dims.clone(),
                c.data
                    .iter()
                    .map(|v| arg_str(v, "lookup"))
                    .collect::<R<_>>()?,
            ),
            v => (vec![1, 1], vec![arg_str(v, "lookup")?]),
        };
        let res = ys
            .iter()
            .map(|y| {
                let i = table.partition_point(|t| t.as_str() <= y.as_str());
                (i, i > 0 && table[i - 1] == *y)
            })
            .collect();
        (dims, res, table.len())
    } else {
        let t = arg_num(&a[0], "lookup")?.re;
        let y = arg_num(&a[1], "lookup")?;
        let n = t.len();
        let desc = n > 1 && t[0] > t[n - 1];
        let res =
            y.re.iter()
                .map(|&yv| {
                    let i = if yv.is_nan() {
                        n
                    } else if desc {
                        t.partition_point(|v| *v >= yv)
                    } else {
                        t.partition_point(|v| *v <= yv)
                    };
                    (i, i > 0 && t[i - 1] == yv)
                })
                .collect();
        (y.dims.clone(), res, n)
    };
    if ob {
        let d: Vec<bool> = res.iter().map(|(_, m)| *m).collect();
        return one(Value::Bool(Array::new(dims, d)));
    }
    let out: Vec<f64> = res
        .iter()
        .map(|&(i, m)| {
            let mut i = i;
            if om && !m {
                i = 0;
            }
            if ol && n > 0 && i == 0 && !om {
                i = 1;
            }
            if or && n > 1 && i >= n && !om {
                i = n - 1;
            }
            i as f64
        })
        .collect();
    one(Value::matrix(dims, out))
}

/// 0 = UTF-8, 1 = Latin-1, 2 = US-ASCII, 3/4 = UTF-16LE/BE, 5/6 = UTF-32LE/BE.
fn encoding(v: Option<&Value>, fname: &str) -> R<u8> {
    let Some(v) = v else { return Ok(0) };
    let s = arg_str(v, fname)?.to_ascii_lowercase().replace('_', "-");
    match s.as_str() {
        "" | "utf-8" | "utf8" => Ok(0),
        "latin1" | "latin-1" | "iso-8859-1" | "iso8859-1" | "windows-1252" | "cp1252" => Ok(1),
        "us-ascii" | "ascii" => Ok(2),
        "utf-16le" | "utf16le" => Ok(3),
        "utf-16be" | "utf16be" | "utf-16" | "utf16" => Ok(4),
        "utf-32le" | "utf32le" => Ok(5),
        "utf-32be" | "utf32be" | "utf-32" | "utf32" => Ok(6),
        _ => Err(err(
            &format!("MATLAB:{}:UnsupportedEncoding", fname),
            format!("{}: unsupported character encoding '{}'.", fname, s),
        )),
    }
}

fn unicode2native(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "unicode2native")?;
    let s = arg_str(&a[0], "unicode2native")?;
    let enc = encoding(a.get(1), "unicode2native")?;
    let bytes: Vec<f64> = match enc {
        0 => s.as_bytes().iter().map(|b| *b as f64).collect(),
        3 | 4 => s
            .encode_utf16()
            .flat_map(|u| {
                let [hi, lo] = u.to_be_bytes();
                if enc == 3 { [lo, hi] } else { [hi, lo] }
            })
            .map(|b| b as f64)
            .collect(),
        5 | 6 => s
            .chars()
            .flat_map(|c| {
                let b = (c as u32).to_be_bytes();
                if enc == 5 {
                    [b[3], b[2], b[1], b[0]]
                } else {
                    b
                }
            })
            .map(|b| b as f64)
            .collect(),
        1 => s
            .chars()
            .map(|c| {
                if (c as u32) < 256 {
                    c as u32 as f64
                } else {
                    26.0
                }
            })
            .collect(),
        _ => s
            .chars()
            .map(|c| {
                if (c as u32) < 128 {
                    c as u32 as f64
                } else {
                    26.0
                }
            })
            .collect(),
    };
    let n = bytes.len();
    one(Value::Num(
        Num::real(vec![1, n], bytes).with_class(NumClass::UInt8),
    ))
}

fn native2unicode(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "native2unicode")?;
    let x = arg_num(&a[0], "native2unicode")?;
    let enc = encoding(a.get(1), "native2unicode")?;
    let bytes: Vec<u8> =
        x.re.iter()
            .map(|v| NumClass::UInt8.cast(*v) as u8)
            .collect();
    let chars: Vec<char> = match enc {
        0 => String::from_utf8_lossy(&bytes).chars().collect(),
        1 => bytes.iter().map(|b| *b as char).collect(),
        3 | 4 => {
            let units: Vec<u16> = bytes
                .chunks(2)
                .map(|p| {
                    let (a, b) = (p[0], *p.get(1).unwrap_or(&0));
                    if enc == 3 {
                        u16::from_le_bytes([a, b])
                    } else {
                        u16::from_be_bytes([a, b])
                    }
                })
                .collect();
            char::decode_utf16(units)
                .map(|r| r.unwrap_or('\u{FFFD}'))
                .collect()
        }
        5 | 6 => bytes
            .chunks(4)
            .map(|p| {
                let mut b = [0u8; 4];
                b[..p.len()].copy_from_slice(p);
                let v = if enc == 5 {
                    u32::from_le_bytes(b)
                } else {
                    u32::from_be_bytes(b)
                };
                char::from_u32(v).unwrap_or('\u{FFFD}')
            })
            .collect(),
        _ => bytes
            .iter()
            .map(|b| if *b < 128 { *b as char } else { '\u{FFFD}' })
            .collect(),
    };
    let n = chars.len();
    one(Value::Char(Array::new(vec![1, n], chars)))
}

fn do_string_escapes(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "do_string_escapes")?;
    let s = arg_str(&a[0], "do_string_escapes")?;
    let c: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < c.len() {
        if c[i] != '\\' || i + 1 == c.len() {
            out.push(c[i]);
            i += 1;
            continue;
        }
        let e = c[i + 1];
        i += 2;
        match e {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            'a' => out.push('\x07'),
            'b' => out.push('\x08'),
            'f' => out.push('\x0c'),
            'v' => out.push('\x0b'),
            '\\' | '"' | '\'' => out.push(e),
            '0'..='7' => {
                let mut v = e.to_digit(8).unwrap_or(0);
                let mut k = 1;
                while k < 3 && i < c.len() && c[i].is_digit(8) {
                    v = v * 8 + c[i].to_digit(8).unwrap_or(0);
                    i += 1;
                    k += 1;
                }
                out.push(char::from_u32(v).unwrap_or('\0'));
            }
            'x' => {
                let mut v = 0u32;
                let mut k = 0;
                while k < 2 && i < c.len() && c[i].is_ascii_hexdigit() {
                    v = v * 16 + c[i].to_digit(16).unwrap_or(0);
                    i += 1;
                    k += 1;
                }
                if k == 0 {
                    it.warning(
                        "Octave:do_string_escapes:malformedHex",
                        "malformed hex escape sequence '\\x' -- converting to '\\0'",
                    );
                }
                out.push(char::from_u32(v).unwrap_or('\0'));
            }
            other => {
                it.warning(
                    "Octave:do_string_escapes:unrecognized",
                    &format!(
                        "unrecognized escape sequence '\\{}' -- converting to '{}'",
                        other, other
                    ),
                );
                out.push(other);
            }
        }
    }
    one(Value::str(&out))
}

fn undo_string_escapes(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "undo_string_escapes")?;
    let s = arg_str(&a[0], "undo_string_escapes")?;
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '\0' => out.push_str("\\0"),
            '\x07' => out.push_str("\\a"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x0b' => out.push_str("\\v"),
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            other => out.push(other),
        }
    }
    one(Value::str(&out))
}

fn dbstack(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let mut omit = 0;
    let mut complete = false;
    for v in &a {
        if let Some(s) = v.to_str() {
            if s.eq_ignore_ascii_case("-completenames") {
                complete = true;
            }
        } else {
            omit = arg_usize(v, "dbstack")?;
        }
    }
    let frames: Vec<indy_value::StackFrame> =
        it.frames.iter().skip(1).rev().map(|f| it.loc(f)).collect();
    let frames: Vec<_> = frames.into_iter().skip(omit).collect();
    if nargout == 0 {
        for f in &frames {
            let s = format!("> In {} (line {})\n", f.name, f.line);
            it.print(&s);
        }
        return Ok(vec![]);
    }
    let elems: Vec<Vec<Value>> = frames
        .iter()
        .map(|f| {
            let file = if complete {
                f.file.clone()
            } else {
                PathBuf::from(&f.file)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default()
            };
            vec![
                Value::str(&file),
                Value::str(&f.name),
                Value::num(f.line as f64),
            ]
        })
        .collect();
    let st = StructArray {
        dims: vec![elems.len(), 1],
        fields: vec!["file".into(), "name".into(), "line".into()],
        elems,
    };
    take_n(vec![Value::Struct(st), Value::num(1.0)], nargout)
}

// ---------------------------------------------------------------------------
// time
// ---------------------------------------------------------------------------

#[cfg(all(unix, target_pointer_width = "64"))]
pub(crate) mod sys {
    use std::os::raw::{c_char, c_int, c_long};

    #[repr(C)]
    struct Tm {
        tm_sec: c_int,
        tm_min: c_int,
        tm_hour: c_int,
        tm_mday: c_int,
        tm_mon: c_int,
        tm_year: c_int,
        tm_wday: c_int,
        tm_yday: c_int,
        tm_isdst: c_int,
        tm_gmtoff: c_long,
        tm_zone: *const c_char,
    }

    unsafe extern "C" {
        fn localtime_r(t: *const i64, out: *mut Tm) -> *mut Tm;
        fn clock() -> c_long;
    }

    /// Offset of local time from UTC in seconds at `secs` since the epoch.
    pub fn utc_offset(secs: i64) -> i64 {
        let mut tm = Tm {
            tm_sec: 0,
            tm_min: 0,
            tm_hour: 0,
            tm_mday: 0,
            tm_mon: 0,
            tm_year: 0,
            tm_wday: 0,
            tm_yday: 0,
            tm_isdst: 0,
            tm_gmtoff: 0,
            tm_zone: std::ptr::null(),
        };
        // SAFETY: both pointers refer to valid, properly aligned locals.
        let r = unsafe { localtime_r(&secs, &mut tm) };
        if r.is_null() { 0 } else { tm.tm_gmtoff as i64 }
    }

    pub fn cpu_seconds() -> Option<f64> {
        // SAFETY: `clock` has no preconditions.
        let c = unsafe { clock() };
        if c < 0 { None } else { Some(c as f64 / 1e6) }
    }
}

#[cfg(not(all(unix, target_pointer_width = "64")))]
pub(crate) mod sys {
    pub fn utc_offset(_: i64) -> i64 {
        0
    }
    pub fn cpu_seconds() -> Option<f64> {
        None
    }
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Days since 1970-01-01 to (year, month, day) — proleptic Gregorian.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

/// Local time now: (year, month, day, hour, minute, seconds with fraction).
fn local_now() -> (i64, u32, u32, i64, i64, f64) {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs() as i64;
    let frac = d.subsec_nanos() as f64 / 1e9;
    let local = secs + sys::utc_offset(secs);
    let days = local.div_euclid(86_400);
    let sod = local.rem_euclid(86_400);
    let (y, m, dd) = civil_from_days(days);
    (
        y,
        m,
        dd,
        sod / 3600,
        (sod % 3600) / 60,
        (sod % 60) as f64 + frac,
    )
}

fn is_leap(y: f64) -> bool {
    (y.rem_euclid(4.0) == 0.0 && y.rem_euclid(100.0) != 0.0) || y.rem_euclid(400.0) == 0.0
}

/// MATLAB serial date number (day 1 = 1-Jan-0000).
fn datenum_of(y: f64, mo: f64, d: f64, h: f64, mi: f64, s: f64) -> f64 {
    const CUM: [f64; 12] = [
        0.0, 31.0, 59.0, 90.0, 120.0, 151.0, 181.0, 212.0, 243.0, 273.0, 304.0, 334.0,
    ];
    if !(y.is_finite() && mo.is_finite()) {
        return f64::NAN;
    }
    let m0 = mo.floor() - 1.0;
    let yy = y + (m0 / 12.0).floor();
    let mi_ = m0.rem_euclid(12.0) as usize;
    let mut days = 365.0 * yy + (yy / 4.0).ceil() - (yy / 100.0).ceil()
        + (yy / 400.0).ceil()
        + CUM[mi_.min(11)]
        + d;
    if mi_ >= 2 && is_leap(yy) {
        days += 1.0;
    }
    days + h / 24.0 + mi / 1440.0 + s / 86_400.0
}

fn datenum(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "datenum")?;
    if a.iter()
        .any(|v| matches!(v, Value::Char(_) | Value::Cell(_)))
    {
        return Err(err(
            "MATLAB:datenum:unsupportedInput",
            "datenum: date strings are not supported yet; use numeric arguments.",
        ));
    }
    if a.len() == 1 {
        let x = arg_num(&a[0], "datenum")?;
        let (r, c) = (x.dims[0], x.dims.get(1).copied().unwrap_or(1));
        if x.dims.len() == 2 && (c == 3 || c == 6) {
            let at = |i: usize, j: usize| if j < c { x.re[i + j * r] } else { 0.0 };
            let out: Vec<f64> = (0..r)
                .map(|i| datenum_of(at(i, 0), at(i, 1), at(i, 2), at(i, 3), at(i, 4), at(i, 5)))
                .collect();
            return one(Value::col(out));
        }
        return one(Value::Num(Num::real(x.dims.clone(), x.re.clone())));
    }
    need(&a, 3, "datenum")?;
    let parts: Vec<Num> = a
        .iter()
        .take(6)
        .map(|v| arg_num(v, "datenum"))
        .collect::<R<_>>()?;
    let mut dims = vec![1, 1];
    let mut n = 1;
    for p in &parts {
        if p.re.len() != 1 {
            if n != 1 && p.re.len() != n {
                return Err(err(
                    "MATLAB:datenum:InputSizeMismatch",
                    "datenum: inputs must be the same size or scalars.",
                ));
            }
            n = p.re.len();
            dims = p.dims.clone();
        }
    }
    let get = |k: usize, i: usize| -> f64 {
        match parts.get(k) {
            Some(p) if p.re.len() == 1 => p.re[0],
            Some(p) => p.re.get(i).copied().unwrap_or(0.0),
            None => 0.0,
        }
    };
    let out: Vec<f64> = (0..n)
        .map(|i| {
            datenum_of(
                get(0, i),
                get(1, i),
                get(2, i),
                get(3, i),
                get(4, i),
                get(5, i),
            )
        })
        .collect();
    one(Value::matrix(dims, out))
}

fn pause(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let Some(v) = a.first() else {
        return Ok(vec![]);
    };
    if let Some(s) = v.to_str() {
        return match s.to_ascii_lowercase().as_str() {
            "on" | "off" | "query" => {
                if nargout > 0 || s.eq_ignore_ascii_case("query") {
                    one(Value::str("on"))
                } else {
                    Ok(vec![])
                }
            }
            _ => Err(err(
                "MATLAB:pause:invalidInput",
                "pause: argument must be a number, 'on', 'off' or 'query'.",
            )),
        };
    }
    let x = arg_f64(v, "pause")?;
    if x.is_nan() || x < 0.0 {
        return Err(err(
            "MATLAB:pause:invalidInput",
            "pause: time must be a non-negative real scalar.",
        ));
    }
    if super::timer::any_scheduled(it) {
        // Scheduled timers fire during the whole pause.
        return super::timer::pause_for(it, x).map(|_| vec![]);
    }
    let t = x.min(1.0);
    if t > 0.0 {
        std::thread::sleep(std::time::Duration::from_secs_f64(t));
    }
    Ok(vec![])
}

// ---------------------------------------------------------------------------
// factor
// ---------------------------------------------------------------------------

fn factor(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "factor")?;
    let x = arg_num(&a[0], "factor")?;
    if x.re.len() != 1 {
        return Err(err("MATLAB:factor:NonScalarInput", "N must be a scalar."));
    }
    let v = x.re[0];
    if x.im.is_some() || v < 0.0 || v.fract() != 0.0 || !v.is_finite() {
        return Err(err(
            "MATLAB:factor:InputNotPosInt",
            "N must be a real nonnegative integer.",
        ));
    }
    if v > 9_007_199_254_740_992.0 {
        return Err(err(
            "MATLAB:factor:InputTooLarge",
            "The maximum value of N allowed is flintmax.",
        ));
    }
    let mut n = v as u64;
    let mut f: Vec<u64> = Vec::new();
    if n < 4 {
        f.push(n);
    } else {
        let mut p = 2u64;
        while p * p <= n {
            while n.is_multiple_of(p) {
                f.push(p);
                n /= p;
            }
            p += if p == 2 { 1 } else { 2 };
        }
        if n > 1 {
            f.push(n);
        }
    }
    let class = if matches!(a[0], Value::Num(_)) {
        x.class
    } else {
        NumClass::Double
    };
    let mk = |v: Vec<f64>| Value::Num(Num::real(vec![1, v.len()], v).with_class(class));
    if nargout >= 2 {
        let mut p: Vec<f64> = Vec::new();
        let mut k: Vec<f64> = Vec::new();
        for q in &f {
            if p.last() == Some(&(*q as f64)) {
                *k.last_mut().unwrap_or(&mut 0.0) += 1.0;
            } else {
                p.push(*q as f64);
                k.push(1.0);
            }
        }
        let n = k.len();
        return Ok(vec![mk(p), Value::Num(Num::real(vec![1, n], k))]);
    }
    one(mk(f.iter().map(|q| *q as f64).collect()))
}

// ---------------------------------------------------------------------------
// dense complex matrix helper
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct CMat {
    r: usize,
    c: usize,
    d: Vec<C>,
}

impl CMat {
    fn zeros(r: usize, c: usize) -> CMat {
        CMat {
            r,
            c,
            d: vec![CZERO; r * c],
        }
    }
    fn eye(n: usize) -> CMat {
        let mut m = CMat::zeros(n, n);
        for i in 0..n {
            m.d[i + i * n] = (1.0, 0.0);
        }
        m
    }
    fn from_num(x: &Num) -> CMat {
        CMat {
            r: x.dims[0],
            c: x.dims.get(1).copied().unwrap_or(1),
            d: cdata(x),
        }
    }
    fn at(&self, i: usize, j: usize) -> C {
        self.d[i + j * self.r]
    }
    fn set(&mut self, i: usize, j: usize, v: C) {
        self.d[i + j * self.r] = v;
    }
    fn is_real(&self) -> bool {
        self.d.iter().all(|c| c.1 == 0.0)
    }
    fn mul(&self, o: &CMat) -> CMat {
        let mut out = CMat::zeros(self.r, o.c);
        for j in 0..o.c {
            for k in 0..self.c {
                let b = o.at(k, j);
                if b == CZERO {
                    continue;
                }
                for i in 0..self.r {
                    let p = i + j * self.r;
                    out.d[p] = cadd(out.d[p], c_mul(self.at(i, k), b));
                }
            }
        }
        out
    }
    /// s*self + t*o
    fn lin(&self, s: f64, o: &CMat, t: f64) -> CMat {
        CMat {
            r: self.r,
            c: self.c,
            d: self
                .d
                .iter()
                .zip(&o.d)
                .map(|(a, b)| cadd(cscale(*a, s), cscale(*b, t)))
                .collect(),
        }
    }
    fn norm1(&self) -> f64 {
        (0..self.c)
            .map(|j| (0..self.r).map(|i| c_abs(self.at(i, j))).sum::<f64>())
            .fold(0.0, f64::max)
    }
    fn into_value(self) -> Value {
        cvalue(vec![self.r, self.c], &self.d)
    }
    /// Solve self * X = b by LU with partial pivoting (square self).
    /// Exactly zero pivots are replaced by a tiny value; returns (X, singular).
    fn solve(&self, b: &CMat) -> (CMat, bool) {
        let n = self.r;
        let mut a = self.clone();
        let mut x = b.clone();
        let nb = b.c;
        let mut singular = false;
        let tiny = (f64::EPSILON * self.norm1()).max(f64::MIN_POSITIVE);
        for k in 0..n {
            let mut p = k;
            let mut best = c_abs(a.at(k, k));
            for i in k + 1..n {
                let v = c_abs(a.at(i, k));
                if v > best {
                    best = v;
                    p = i;
                }
            }
            if best == 0.0 || best.is_nan() {
                singular = true;
                a.set(k, k, (tiny, 0.0));
            } else if p != k {
                for j in 0..n {
                    a.d.swap(k + j * n, p + j * n);
                }
                for j in 0..nb {
                    x.d.swap(k + j * n, p + j * n);
                }
            }
            let piv = a.at(k, k);
            for i in k + 1..n {
                let m = c_div(a.at(i, k), piv);
                if m == CZERO {
                    continue;
                }
                for j in k + 1..n {
                    let v = csub(a.at(i, j), c_mul(m, a.at(k, j)));
                    a.set(i, j, v);
                }
                for j in 0..nb {
                    let v = csub(x.at(i, j), c_mul(m, x.at(k, j)));
                    x.set(i, j, v);
                }
                a.set(i, k, CZERO);
            }
        }
        for j in 0..nb {
            for i in (0..n).rev() {
                let mut s = x.at(i, j);
                for k in i + 1..n {
                    s = csub(s, c_mul(a.at(i, k), x.at(k, j)));
                }
                x.set(i, j, c_div(s, a.at(i, i)));
            }
        }
        (x, singular)
    }
}

// ---------------------------------------------------------------------------
// singular values and rank
// ---------------------------------------------------------------------------

/// Singular values of a real m×n matrix (one-sided Jacobi), descending.
fn singular_values(m: usize, n: usize, a: &[f64]) -> Vec<f64> {
    let (rows, cols, mut u) = if m >= n {
        (m, n, a.to_vec())
    } else {
        let mut t = vec![0.0; m * n];
        for i in 0..m {
            for j in 0..n {
                t[j + i * n] = a[i + j * m];
            }
        }
        (n, m, t)
    };
    for _ in 0..80 {
        let mut rotated = false;
        for p in 0..cols {
            for q in p + 1..cols {
                let (mut al, mut be, mut ga) = (0.0, 0.0, 0.0);
                for i in 0..rows {
                    let (x, y) = (u[i + p * rows], u[i + q * rows]);
                    al += x * x;
                    be += y * y;
                    ga += x * y;
                }
                if ga == 0.0 || ga.abs() <= f64::EPSILON * (al * be).sqrt() {
                    continue;
                }
                rotated = true;
                let zeta = (be - al) / (2.0 * ga);
                let t = zeta.signum() / (zeta.abs() + (1.0 + zeta * zeta).sqrt());
                let t = if zeta == 0.0 { 1.0 } else { t };
                let c = 1.0 / (1.0 + t * t).sqrt();
                let s = c * t;
                for i in 0..rows {
                    let (x, y) = (u[i + p * rows], u[i + q * rows]);
                    u[i + p * rows] = c * x - s * y;
                    u[i + q * rows] = s * x + c * y;
                }
            }
        }
        if !rotated {
            break;
        }
    }
    let mut sv: Vec<f64> = (0..cols)
        .map(|j| {
            (0..rows)
                .map(|i| u[i + j * rows].powi(2))
                .sum::<f64>()
                .sqrt()
        })
        .collect();
    sv.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    sv
}

fn rank(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "rank")?;
    let x = arg_num(&a[0], "rank")?;
    if x.dims.len() != 2 {
        return Err(err(
            "MATLAB:rank:inputMustBe2D",
            "Input must be a 2-D matrix.",
        ));
    }
    let (m, n) = (x.dims[0], x.dims[1]);
    if x.re.is_empty() {
        return one(Value::num(0.0));
    }
    if x.re
        .iter()
        .chain(x.im.iter().flatten())
        .any(|v| !v.is_finite())
    {
        return Err(err(
            "MATLAB:svd:matrixWithNaNInf",
            "Input to SVD must not contain NaN or Inf.",
        ));
    }
    let (sv, dup) = match &x.im {
        None => (singular_values(m, n, &x.re), 1.0),
        Some(im) => {
            // [Re -Im; Im Re] has every singular value of A twice.
            let (mm, nn) = (2 * m, 2 * n);
            let mut e = vec![0.0; mm * nn];
            for j in 0..n {
                for i in 0..m {
                    let (re, iv) = (x.re[i + j * m], im[i + j * m]);
                    e[i + j * mm] = re;
                    e[(i + m) + (j + n) * mm] = re;
                    e[(i + m) + j * mm] = iv;
                    e[i + (j + n) * mm] = -iv;
                }
            }
            (singular_values(mm, nn, &e), 2.0)
        }
    };
    let smax = sv.first().copied().unwrap_or(0.0);
    let tol = match a.get(1) {
        Some(v) => arg_f64(v, "rank")?,
        None => m.max(n) as f64 * eps_of(smax),
    };
    let r = sv.iter().filter(|s| **s > tol).count() as f64 / dup;
    one(Value::num(r.round()))
}

// ---------------------------------------------------------------------------
// eigenvalues
// ---------------------------------------------------------------------------

/// Cyclic Jacobi for a real symmetric matrix: ascending values and vectors.
fn jacobi_eig(n: usize, mut a: Vec<f64>) -> (Vec<f64>, Vec<f64>) {
    let mut v = vec![0.0; n * n];
    for i in 0..n {
        v[i + i * n] = 1.0;
    }
    let fro: f64 = a.iter().map(|x| x * x).sum::<f64>();
    for _ in 0..100 {
        let mut off = 0.0;
        for q in 0..n {
            for p in 0..q {
                off += a[p + q * n] * a[p + q * n];
            }
        }
        if off == 0.0 || off <= (f64::EPSILON * f64::EPSILON) * fro * 1e-4 {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                let apq = a[p + q * n];
                if apq == 0.0 {
                    continue;
                }
                let theta = (a[q + q * n] - a[p + p * n]) / (2.0 * apq);
                let t = if theta.is_infinite() {
                    0.0
                } else {
                    let t = 1.0 / (theta.abs() + (theta * theta + 1.0).sqrt());
                    if theta < 0.0 { -t } else { t }
                };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for k in 0..n {
                    let (akp, akq) = (a[k + p * n], a[k + q * n]);
                    a[k + p * n] = c * akp - s * akq;
                    a[k + q * n] = s * akp + c * akq;
                }
                for k in 0..n {
                    let (apk, aqk) = (a[p + k * n], a[q + k * n]);
                    a[p + k * n] = c * apk - s * aqk;
                    a[q + k * n] = s * apk + c * aqk;
                }
                a[p + q * n] = 0.0;
                a[q + p * n] = 0.0;
                for k in 0..n {
                    let (vkp, vkq) = (v[k + p * n], v[k + q * n]);
                    v[k + p * n] = c * vkp - s * vkq;
                    v[k + q * n] = s * vkp + c * vkq;
                }
            }
        }
    }
    let vals: Vec<f64> = (0..n).map(|i| a[i + i * n]).collect();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|x, y| {
        vals[*x]
            .partial_cmp(&vals[*y])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let sv: Vec<f64> = order.iter().map(|i| vals[*i]).collect();
    let mut sv_vecs = vec![0.0; n * n];
    for (newj, oldj) in order.iter().enumerate() {
        // sign convention: make the largest-magnitude component positive
        let col = &v[oldj * n..oldj * n + n];
        let big = col
            .iter()
            .cloned()
            .fold(0.0f64, |m, x| if x.abs() > m.abs() { x } else { m });
        let sg = if big < 0.0 { -1.0 } else { 1.0 };
        for i in 0..n {
            sv_vecs[i + newj * n] = sg * col[i];
        }
    }
    (sv, sv_vecs)
}

/// Real Hessenberg reduction by stabilised elimination (similarity).
fn real_hessenberg(n: usize, h: &mut [f64]) {
    for k in 0..n.saturating_sub(2) {
        let mut p = k + 1;
        let mut best = h[(k + 1) + k * n].abs();
        for i in k + 2..n {
            if h[i + k * n].abs() > best {
                best = h[i + k * n].abs();
                p = i;
            }
        }
        if best == 0.0 {
            continue;
        }
        if p != k + 1 {
            for j in 0..n {
                h.swap(p + j * n, (k + 1) + j * n);
            }
            for i in 0..n {
                h.swap(i + p * n, i + (k + 1) * n);
            }
        }
        let piv = h[(k + 1) + k * n];
        for i in k + 2..n {
            let m = h[i + k * n] / piv;
            if m == 0.0 {
                continue;
            }
            for j in k..n {
                h[i + j * n] -= m * h[(k + 1) + j * n];
            }
            for j in 0..n {
                h[j + (k + 1) * n] += m * h[j + i * n];
            }
        }
    }
}

/// Eigenvalues of a real 2×2 block in LAPACK's standardised order.
fn two_by_two(a: f64, b: f64, c: f64, d: f64) -> (C, C) {
    let p = 0.5 * (a - d);
    let bc = b * c;
    let q = p * p + bc;
    if q >= 0.0 {
        let z = p + q.sqrt().copysign(p);
        let l1 = d + z;
        let l2 = if z != 0.0 { d - bc / z } else { d };
        ((l1, 0.0), (l2, 0.0))
    } else {
        let re = d + p;
        let im = (-q).sqrt();
        ((re, im), (re, -im))
    }
}

/// Apply a Householder reflector built from `v` (len 2 or 3) at rows/cols
/// k.. of the active window [l, hi] of the n×n matrix `h`.
fn reflect(h: &mut [f64], n: usize, k: usize, v: &[f64], l: usize, hi: usize) {
    let r = v.len();
    let norm = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm == 0.0 || !norm.is_finite() {
        return;
    }
    let alpha = if v[0] >= 0.0 { -norm } else { norm };
    let mut u = v.to_vec();
    u[0] -= alpha;
    let uu: f64 = u.iter().map(|x| x * x).sum();
    if uu == 0.0 {
        return;
    }
    let beta = 2.0 / uu;
    let c0 = if k > l { k - 1 } else { l };
    for j in c0..=hi {
        let dot: f64 = (0..r).map(|i| u[i] * h[(k + i) + j * n]).sum();
        for i in 0..r {
            h[(k + i) + j * n] -= beta * u[i] * dot;
        }
    }
    let rmax = (k + r).min(hi);
    for i in l..=rmax {
        let dot: f64 = (0..r).map(|j| h[i + (k + j) * n] * u[j]).sum();
        for j in 0..r {
            h[i + (k + j) * n] -= beta * dot * u[j];
        }
    }
}

/// Eigenvalues of a real square matrix by Hessenberg reduction and the
/// Francis double-shift QR iteration. Values are in Schur-diagonal order;
/// NaN marks non-convergence.
fn francis(n: usize, mut h: Vec<f64>) -> Vec<C> {
    real_hessenberg(n, &mut h);
    let idx = |i: usize, j: usize| i + j * n;
    let mut ev = vec![(f64::NAN, f64::NAN); n];
    let anorm = h
        .iter()
        .map(|v| v.abs())
        .sum::<f64>()
        .max(f64::MIN_POSITIVE);
    let mut hi = n as isize - 1;
    let mut its = 0;
    let mut total = 0usize;
    while hi >= 0 {
        let hu = hi as usize;
        let mut l = hu;
        while l > 0 {
            let s = h[idx(l - 1, l - 1)].abs() + h[idx(l, l)].abs();
            let s = if s == 0.0 { anorm } else { s };
            if h[idx(l, l - 1)].abs() <= f64::EPSILON * s {
                h[idx(l, l - 1)] = 0.0;
                break;
            }
            l -= 1;
        }
        if l == hu {
            ev[hu] = (h[idx(hu, hu)], 0.0);
            hi -= 1;
            its = 0;
            continue;
        }
        if l + 1 == hu {
            let (p, q) = two_by_two(h[idx(l, l)], h[idx(l, hu)], h[idx(hu, l)], h[idx(hu, hu)]);
            ev[l] = p;
            ev[hu] = q;
            hi -= 2;
            its = 0;
            continue;
        }
        its += 1;
        total += 1;
        if its > 60 || total > 60 * n {
            break;
        }
        let (s, t) = if its % 10 == 0 {
            let w = h[idx(hu, hu - 1)].abs() + h[idx(hu - 1, hu - 2)].abs();
            let d = h[idx(hu, hu)];
            (
                2.0 * d + 1.5 * w,
                (d + 0.75 * w) * (d + 0.75 * w) + 0.4375 * w * w,
            )
        } else {
            let (a11, a12, a21, a22) = (
                h[idx(hu - 1, hu - 1)],
                h[idx(hu - 1, hu)],
                h[idx(hu, hu - 1)],
                h[idx(hu, hu)],
            );
            (a11 + a22, a11 * a22 - a12 * a21)
        };
        let (h00, h10) = (h[idx(l, l)], h[idx(l + 1, l)]);
        let mut x = h00 * h00 + h[idx(l, l + 1)] * h10 - s * h00 + t;
        let mut y = h10 * (h00 + h[idx(l + 1, l + 1)] - s);
        let mut z = h10 * h[idx(l + 2, l + 1)];
        for k in l..=hu - 2 {
            reflect(&mut h, n, k, &[x, y, z], l, hu);
            if k > l {
                h[idx(k + 1, k - 1)] = 0.0;
                h[idx(k + 2, k - 1)] = 0.0;
            }
            x = h[idx(k + 1, k)];
            y = h[idx(k + 2, k)];
            if k + 3 <= hu {
                z = h[idx(k + 3, k)];
            }
        }
        reflect(&mut h, n, hu - 1, &[x, y], l, hu);
        h[idx(hu, hu - 2)] = 0.0;
    }
    ev
}

/// Eigenvalues of a complex square matrix: Hessenberg + single-shift QR.
fn complex_eigs(m: &CMat) -> Vec<C> {
    let n = m.r;
    let mut h = m.clone();
    // Hessenberg by elimination
    for k in 0..n.saturating_sub(2) {
        let mut p = k + 1;
        let mut best = c_abs(h.at(k + 1, k));
        for i in k + 2..n {
            if c_abs(h.at(i, k)) > best {
                best = c_abs(h.at(i, k));
                p = i;
            }
        }
        if best == 0.0 {
            continue;
        }
        if p != k + 1 {
            for j in 0..n {
                h.d.swap(p + j * n, (k + 1) + j * n);
            }
            for i in 0..n {
                h.d.swap(i + p * n, i + (k + 1) * n);
            }
        }
        let piv = h.at(k + 1, k);
        for i in k + 2..n {
            let mlt = c_div(h.at(i, k), piv);
            if mlt == CZERO {
                continue;
            }
            for j in k..n {
                let v = csub(h.at(i, j), c_mul(mlt, h.at(k + 1, j)));
                h.set(i, j, v);
            }
            for j in 0..n {
                let v = cadd(h.at(j, k + 1), c_mul(mlt, h.at(j, i)));
                h.set(j, k + 1, v);
            }
        }
    }
    let anorm =
        h.d.iter()
            .map(|c| c_abs(*c))
            .sum::<f64>()
            .max(f64::MIN_POSITIVE);
    let mut ev = vec![(f64::NAN, f64::NAN); n];
    let mut hi = n as isize - 1;
    let mut its = 0;
    let mut total = 0usize;
    while hi >= 0 {
        let hu = hi as usize;
        let mut l = hu;
        while l > 0 {
            let s = c_abs(h.at(l - 1, l - 1)) + c_abs(h.at(l, l));
            let s = if s == 0.0 { anorm } else { s };
            if c_abs(h.at(l, l - 1)) <= f64::EPSILON * s {
                h.set(l, l - 1, CZERO);
                break;
            }
            l -= 1;
        }
        if l == hu {
            ev[hu] = h.at(hu, hu);
            hi -= 1;
            its = 0;
            continue;
        }
        its += 1;
        total += 1;
        if its > 60 || total > 60 * n {
            break;
        }
        let (a, b, c, d) = (
            h.at(hu - 1, hu - 1),
            h.at(hu - 1, hu),
            h.at(hu, hu - 1),
            h.at(hu, hu),
        );
        let mu = if its % 10 == 0 {
            cadd(d, (c_abs(c), 0.0))
        } else {
            let half = cscale(csub(a, d), 0.5);
            let disc = c_sqrt(cadd(c_mul(half, half), c_mul(b, c)));
            let mid = cscale(cadd(a, d), 0.5);
            let (m1, m2) = (cadd(mid, disc), csub(mid, disc));
            if c_abs(csub(m1, d)) <= c_abs(csub(m2, d)) {
                m1
            } else {
                m2
            }
        };
        for k in l..=hu {
            let v = csub(h.at(k, k), mu);
            h.set(k, k, v);
        }
        let mut rots: Vec<(C, C)> = Vec::with_capacity(hu - l);
        for k in l..hu {
            let x = h.at(k, k);
            let y = h.at(k + 1, k);
            let r = (c_abs(x).powi(2) + c_abs(y).powi(2)).sqrt();
            let (cg, sg) = if r == 0.0 {
                ((1.0, 0.0), CZERO)
            } else {
                (cscale(x, 1.0 / r), cscale(y, 1.0 / r))
            };
            for j in k..=hu {
                let (p, q) = (h.at(k, j), h.at(k + 1, j));
                h.set(k, j, cadd(c_mul(cconj(cg), p), c_mul(cconj(sg), q)));
                h.set(k + 1, j, cadd(c_mul((-sg.0, -sg.1), p), c_mul(cg, q)));
            }
            rots.push((cg, sg));
        }
        for (t, k) in (l..hu).enumerate() {
            let (cg, sg) = rots[t];
            for i in l..=hu {
                let (p, q) = (h.at(i, k), h.at(i, k + 1));
                h.set(i, k, cadd(c_mul(p, cg), c_mul(q, sg)));
                let nsc = (-sg.0, sg.1); // -conj(s)
                h.set(i, k + 1, cadd(c_mul(p, nsc), c_mul(q, cconj(cg))));
            }
        }
        for k in l..=hu {
            let v = cadd(h.at(k, k), mu);
            h.set(k, k, v);
        }
    }
    ev
}

fn is_hermitian(m: &CMat) -> bool {
    if m.r != m.c {
        return false;
    }
    for j in 0..m.c {
        for i in 0..=j {
            if m.at(i, j) != cconj(m.at(j, i)) {
                return false;
            }
        }
    }
    true
}

/// Eigenvalues of a square matrix (values only).
fn eig_values(m: &CMat) -> Vec<C> {
    let n = m.r;
    if m.is_real() {
        let re: Vec<f64> = m.d.iter().map(|c| c.0).collect();
        if is_hermitian(m) {
            return jacobi_eig(n, re).0.into_iter().map(|v| (v, 0.0)).collect();
        }
        return francis(n, re);
    }
    let mut ev = complex_eigs(m);
    if is_hermitian(m) {
        for v in ev.iter_mut() {
            v.1 = 0.0;
        }
        ev.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    }
    ev
}

/// Eigenvector of `a` for eigenvalue `lam` by inverse iteration, orthogonal
/// to the `prev` vectors (same eigenvalue cluster). Unit norm, largest
/// component real.
fn inverse_iteration(a: &CMat, lam: C, prev: &[Vec<C>], seed: usize) -> Vec<C> {
    let n = a.r;
    let scale = a.norm1().max(f64::MIN_POSITIVE);
    let shift = cadd(lam, (scale * 1e-13, 0.0));
    let mut m = a.clone();
    for i in 0..n {
        m.set(i, i, csub(m.at(i, i), shift));
    }
    let mut v: Vec<C> = (0..n)
        .map(|i| (1.0 + ((i * 7 + seed * 13) % 11) as f64 * 0.1, 0.0))
        .collect();
    let orth = |v: &mut Vec<C>| {
        for p in prev {
            let mut dot = CZERO;
            for i in 0..n {
                dot = cadd(dot, c_mul(cconj(p[i]), v[i]));
            }
            for i in 0..n {
                v[i] = csub(v[i], c_mul(dot, p[i]));
            }
        }
        let nrm = v.iter().map(|c| c_abs(*c).powi(2)).sum::<f64>().sqrt();
        if nrm > 0.0 && nrm.is_finite() {
            for c in v.iter_mut() {
                *c = cscale(*c, 1.0 / nrm);
            }
        }
    };
    orth(&mut v);
    for _ in 0..3 {
        let b = CMat {
            r: n,
            c: 1,
            d: v.clone(),
        };
        let (x, _) = m.solve(&b);
        v = x.d;
        if v.iter().any(|c| !c.0.is_finite() || !c.1.is_finite()) {
            v = (0..n)
                .map(|i| {
                    if i == seed % n.max(1) {
                        (1.0, 0.0)
                    } else {
                        CZERO
                    }
                })
                .collect();
        }
        orth(&mut v);
    }
    // phase: largest component real and positive
    let mut big = 0;
    for i in 0..n {
        if c_abs(v[i]) > c_abs(v[big]) {
            big = i;
        }
    }
    if n > 0 {
        let b = v[big];
        let mag = c_abs(b);
        if mag > 0.0 {
            let ph = (b.0 / mag, -b.1 / mag);
            for c in v.iter_mut() {
                *c = c_mul(*c, ph);
            }
            v[big] = (v[big].0, 0.0);
        }
    }
    v
}

/// Pair conjugate eigenvalues of a real matrix exactly; tiny imaginary
/// parts become zero.
fn clean_real_eigs(ev: &mut [C], scale: f64) {
    let tol = 1e3 * f64::EPSILON * scale.max(f64::MIN_POSITIVE);
    let n = ev.len();
    let mut used = vec![false; n];
    for i in 0..n {
        if ev[i].1.abs() <= tol {
            ev[i].1 = 0.0;
            used[i] = true;
        }
    }
    for i in 0..n {
        if used[i] {
            continue;
        }
        used[i] = true;
        let mut best: Option<usize> = None;
        for j in 0..n {
            if !used[j] {
                let d = c_abs(csub(ev[j], cconj(ev[i])));
                if best.is_none_or(|b| d < c_abs(csub(ev[b], cconj(ev[i])))) {
                    best = Some(j);
                }
            }
        }
        if let Some(j) = best {
            used[j] = true;
            let re = 0.5 * (ev[i].0 + ev[j].0);
            let im = 0.5 * (ev[i].1.abs() + ev[j].1.abs());
            ev[i] = (re, im);
            ev[j] = (re, -im);
        }
    }
}

fn square_num(x: &Num, fname: &str) -> R<usize> {
    if x.dims.len() != 2 || x.dims[0] != x.dims[1] {
        return Err(err(
            &format!("MATLAB:{}:inputMustBeSquareMatrix", fname),
            "Input matrix must be square.",
        ));
    }
    Ok(x.dims[0])
}

fn diag_matrix(v: &[C]) -> Value {
    let n = v.len();
    let mut d = vec![CZERO; n * n];
    for i in 0..n {
        d[i + i * n] = v[i];
    }
    cvalue(vec![n, n], &d)
}

fn eig(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "eig")?;
    let x = arg_num(&a[0], "eig")?;
    let mut bmat: Option<Num> = None;
    let mut vector_out: Option<bool> = None;
    for v in &a[1..] {
        if let Some(s) = v.to_str() {
            match s.to_ascii_lowercase().as_str() {
                "vector" => vector_out = Some(true),
                "matrix" => vector_out = Some(false),
                "nobalance" | "balance" | "chol" | "qz" => {}
                other => {
                    return Err(err(
                        "MATLAB:eig:invalidOption",
                        format!("eig: unknown option '{}'.", other),
                    ));
                }
            }
        } else {
            bmat = Some(arg_num(v, "eig")?);
        }
    }
    let n = square_num(&x, "eig")?;
    let finite = |x: &Num| {
        x.re.iter()
            .chain(x.im.iter().flatten())
            .all(|v| v.is_finite())
    };
    if !finite(&x) {
        return Err(err(
            "MATLAB:eig:matrixWithNaNInf",
            "Input to EIG must not contain NaN or Inf.",
        ));
    }
    let mut m = CMat::from_num(&x);
    let mut herm_result = is_hermitian(&m);
    if let Some(b) = &bmat {
        if square_num(b, "eig")? != n {
            return Err(err(
                "MATLAB:eig:sizeMismatch",
                "Matrices must be the same size.",
            ));
        }
        if !finite(b) {
            return Err(err(
                "MATLAB:eig:matrixWithNaNInf",
                "Input to EIG must not contain NaN or Inf.",
            ));
        }
        let bm = CMat::from_num(b);
        herm_result = herm_result && is_hermitian(&bm);
        m = bm.solve(&m).0;
    }
    let real = m.is_real();
    let (vals, vecs): (Vec<C>, Option<CMat>) = if real && is_hermitian(&m) {
        let (v, vecs) = jacobi_eig(n, m.d.iter().map(|c| c.0).collect());
        (
            v.into_iter().map(|e| (e, 0.0)).collect(),
            Some(CMat {
                r: n,
                c: n,
                d: vecs.into_iter().map(|e| (e, 0.0)).collect(),
            }),
        )
    } else {
        let mut ev = if real {
            francis(n, m.d.iter().map(|c| c.0).collect())
        } else {
            complex_eigs(&m)
        };
        if real {
            clean_real_eigs(&mut ev, m.norm1());
        }
        if herm_result {
            for v in ev.iter_mut() {
                v.1 = 0.0;
            }
            ev.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        }
        let vecs = if nargout >= 2 {
            let scale = m.norm1().max(f64::MIN_POSITIVE);
            let mut cols: Vec<Vec<C>> = Vec::with_capacity(n);
            for (k, lam) in ev.iter().enumerate() {
                let prev: Vec<Vec<C>> = (0..k)
                    .filter(|j| c_abs(csub(ev[*j], *lam)) <= 1e-8 * scale)
                    .map(|j| cols[j].clone())
                    .collect();
                if lam.0.is_nan() {
                    cols.push(vec![(f64::NAN, 0.0); n]);
                } else {
                    cols.push(inverse_iteration(&m, *lam, &prev, k));
                }
            }
            Some(CMat {
                r: n,
                c: n,
                d: cols.into_iter().flatten().collect(),
            })
        } else {
            None
        };
        (ev, vecs)
    };
    if nargout <= 1 {
        if vector_out == Some(false) {
            return one(diag_matrix(&vals));
        }
        return one(cvalue(vec![n, 1], &vals));
    }
    let d = if vector_out == Some(true) {
        cvalue(vec![n, 1], &vals)
    } else {
        diag_matrix(&vals)
    };
    let v = vecs.map(|v| v.into_value()).unwrap_or_default();
    Ok(vec![v, d])
}

// ---------------------------------------------------------------------------
// expm
// ---------------------------------------------------------------------------

fn expm(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "expm")?;
    let x = arg_num(&a[0], "expm")?;
    let n = square_num(&x, "expm")?;
    if n == 0 {
        return one(Value::matrix(vec![0, 0], vec![]));
    }
    let m = CMat::from_num(&x);
    if m.d.iter().any(|c| !c.0.is_finite() || !c.1.is_finite()) {
        return one(Value::matrix(vec![n, n], vec![f64::NAN; n * n]));
    }
    const B: [f64; 14] = [
        64_764_752_532_480_000.0,
        32_382_376_266_240_000.0,
        7_771_770_303_897_600.0,
        1_187_353_796_428_800.0,
        129_060_195_264_000.0,
        10_559_470_521_600.0,
        670_442_572_800.0,
        33_522_128_640.0,
        1_323_241_920.0,
        40_840_800.0,
        960_960.0,
        16_380.0,
        182.0,
        1.0,
    ];
    let theta13 = 5.371_920_351_148_152;
    let nrm = m.norm1();
    let s = if nrm > theta13 {
        (nrm / theta13).log2().ceil().clamp(0.0, 1100.0) as i32
    } else {
        0
    };
    let a1 = m.lin(2f64.powi(-s), &m, 0.0);
    let id = CMat::eye(n);
    let a2 = a1.mul(&a1);
    let a4 = a2.mul(&a2);
    let a6 = a4.mul(&a2);
    // U = A1 [A6 (b13 A6 + b11 A4 + b9 A2) + b7 A6 + b5 A4 + b3 A2 + b1 I]
    let inner_u = a6.lin(B[13], &a4, B[11]).lin(1.0, &a2, B[9]);
    let u_poly = a6
        .mul(&inner_u)
        .lin(1.0, &a6, B[7])
        .lin(1.0, &a4, B[5])
        .lin(1.0, &a2, B[3])
        .lin(1.0, &id, B[1]);
    let u = a1.mul(&u_poly);
    // V = A6 (b12 A6 + b10 A4 + b8 A2) + b6 A6 + b4 A4 + b2 A2 + b0 I
    let inner_v = a6.lin(B[12], &a4, B[10]).lin(1.0, &a2, B[8]);
    let v = a6
        .mul(&inner_v)
        .lin(1.0, &a6, B[6])
        .lin(1.0, &a4, B[4])
        .lin(1.0, &a2, B[2])
        .lin(1.0, &id, B[0]);
    let p = v.lin(1.0, &u, 1.0);
    let q = v.lin(1.0, &u, -1.0);
    let (mut r, _) = q.solve(&p);
    for _ in 0..s {
        r = r.mul(&r);
    }
    if m.is_real() {
        for c in r.d.iter_mut() {
            c.1 = 0.0;
        }
    }
    one(r.into_value())
}

// ---------------------------------------------------------------------------
// vecnorm, mode, bounds, rescale
// ---------------------------------------------------------------------------

fn vecnorm(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "vecnorm")?;
    let x = arg_num(&a[0], "vecnorm")?;
    let p = match a.get(1) {
        Some(v) if !v.is_empty() => arg_f64(v, "vecnorm")?,
        _ => 2.0,
    };
    if p.is_nan() || p <= 0.0 {
        return Err(err(
            "MATLAB:vecnorm:unknownNorm",
            "The only norms available are positive real values and Inf.",
        ));
    }
    let dim = match a.get(2) {
        Some(v) => dim_arg(v, "vecnorm")?,
        None => default_dim(&x.dims),
    };
    let mags: Vec<f64> = (0..x.re.len())
        .map(|i| c_abs((x.re[i], x.im_at(i))))
        .collect();
    let mut out = Vec::new();
    slices(&x.dims, dim, |start, stride, len| {
        let it = (0..len).map(|k| mags[start + k * stride]);
        let r = if p.is_infinite() {
            it.fold(0.0, f64::max)
        } else if p == 1.0 {
            it.sum()
        } else if p == 2.0 {
            it.map(|v| v * v).sum::<f64>().sqrt()
        } else {
            it.map(|v| v.powf(p)).sum::<f64>().powf(1.0 / p)
        };
        out.push(r);
    });
    one(Value::matrix(reduced_dims(&x.dims, dim), out))
}

fn mode(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "mode")?;
    let x = arg_num(&a[0], "mode")?;
    let class = if matches!(a[0], Value::Num(_)) {
        x.class
    } else {
        NumClass::Double
    };
    if x.dims == [0, 0] && a.len() == 1 {
        let c = Value::cell_row(vec![Value::matrix(vec![0, 1], vec![])]);
        return take_n(
            vec![
                Value::Num(Num::scalar(f64::NAN).with_class(class)),
                Value::num(0.0),
                c,
            ],
            nargout,
        );
    }
    let dim = match a.get(1) {
        Some(v) => dim_arg(v, "mode")?,
        None => default_dim(&x.dims),
    };
    let mut m = Vec::new();
    let mut f = Vec::new();
    let mut ties = Vec::new();
    slices(&x.dims, dim, |start, stride, len| {
        let mut v: Vec<f64> = (0..len)
            .map(|k| x.re[start + k * stride])
            .filter(|v| !v.is_nan())
            .collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mut runs: Vec<(f64, usize)> = Vec::new();
        for e in v {
            match runs.last_mut() {
                Some((val, cnt)) if *val == e => *cnt += 1,
                _ => runs.push((e, 1)),
            }
        }
        let best = runs.iter().map(|r| r.1).max().unwrap_or(0);
        if best == 0 {
            m.push(f64::NAN);
            f.push(0.0);
            ties.push(Value::matrix(vec![0, 1], vec![]));
        } else {
            let t: Vec<f64> = runs.iter().filter(|r| r.1 == best).map(|r| r.0).collect();
            m.push(t[0]);
            f.push(best as f64);
            ties.push(Value::Num(Num::real(vec![t.len(), 1], t).with_class(class)));
        }
    });
    let rd = reduced_dims(&x.dims, dim);
    let out_m = Value::Num(Num::real(rd.clone(), m).with_class(class));
    let out_f = Value::matrix(rd.clone(), f);
    let out_c = Value::Cell(Array::new(rd, ties));
    take_n(vec![out_m, out_f, out_c], nargout)
}

fn bounds(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "bounds")?;
    let x = arg_num(&a[0], "bounds")?;
    let class = if matches!(a[0], Value::Num(_)) {
        x.class
    } else {
        NumClass::Double
    };
    let mut dim = None;
    let mut all = false;
    let mut include = false;
    for v in &a[1..] {
        if let Some(s) = v.to_str() {
            match s.to_ascii_lowercase().as_str() {
                "all" => all = true,
                "omitnan" | "omitmissing" => include = false,
                "includenan" | "includemissing" => include = true,
                other => {
                    return Err(err(
                        "MATLAB:bounds:unknownOption",
                        format!("bounds: unknown option '{}'.", other),
                    ));
                }
            }
        } else {
            dim = Some(dim_arg(v, "bounds")?);
        }
    }
    if x.re.is_empty() && dim.is_none() {
        return take_n(vec![Value::empty(), Value::empty()], nargout);
    }
    let dims = if all {
        vec![x.re.len(), 1]
    } else {
        x.dims.clone()
    };
    let d = if all {
        0
    } else {
        dim.unwrap_or_else(|| default_dim(&dims))
    };
    let (mut lo, mut hi) = (Vec::new(), Vec::new());
    slices(&dims, d, |start, stride, len| {
        let vals = (0..len).map(|k| x.re[start + k * stride]);
        let (mut l, mut h, mut nan, mut any) = (f64::INFINITY, f64::NEG_INFINITY, false, false);
        for v in vals {
            if v.is_nan() {
                nan = true;
                continue;
            }
            any = true;
            l = l.min(v);
            h = h.max(v);
        }
        if (nan && include) || !any {
            lo.push(f64::NAN);
            hi.push(f64::NAN);
        } else {
            lo.push(l);
            hi.push(h);
        }
    });
    let rd = reduced_dims(&dims, d);
    take_n(
        vec![
            Value::Num(Num::real(rd.clone(), lo).with_class(class)),
            Value::Num(Num::real(rd, hi).with_class(class)),
        ],
        nargout,
    )
}

fn rescale(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "rescale")?;
    let x = arg_num(&a[0], "rescale")?;
    if x.im.is_some() {
        return Err(err(
            "MATLAB:rescale:complexInput",
            "rescale: input must be real.",
        ));
    }
    let (mut l, mut u) = (0.0, 1.0);
    let mut i = 1;
    if a.len() >= 3 && !a[1].is_char() {
        l = arg_f64(&a[1], "rescale")?;
        u = arg_f64(&a[2], "rescale")?;
        i = 3;
    }
    let (mut inmin, mut inmax) = (None, None);
    while i < a.len() {
        let name = arg_str(&a[i], "rescale")?.to_ascii_lowercase();
        let Some(v) = a.get(i + 1) else {
            return Err(err(
                "MATLAB:rescale:nameValuePairs",
                "rescale: name-value arguments must come in pairs.",
            ));
        };
        match name.as_str() {
            "inputmin" => inmin = Some(arg_f64(v, "rescale")?),
            "inputmax" => inmax = Some(arg_f64(v, "rescale")?),
            other => {
                return Err(err(
                    "MATLAB:rescale:unknownOption",
                    format!("rescale: unknown parameter '{}'.", other),
                ));
            }
        }
        i += 2;
    }
    let fin = x.re.iter().filter(|v| !v.is_nan());
    let lo = inmin.unwrap_or_else(|| fin.clone().cloned().fold(f64::INFINITY, f64::min));
    let hi = inmax.unwrap_or_else(|| fin.cloned().fold(f64::NEG_INFINITY, f64::max));
    let out: Vec<f64> =
        x.re.iter()
            .map(|&v| {
                if v.is_nan() {
                    return f64::NAN;
                }
                let v = if v < lo {
                    lo
                } else if v > hi {
                    hi
                } else {
                    v
                };
                if hi == lo {
                    l
                } else {
                    l + (v - lo) / (hi - lo) * (u - l)
                }
            })
            .collect();
    let class = if x.class == NumClass::Single {
        NumClass::Single
    } else {
        NumClass::Double
    };
    one(Value::Num(Num::real(x.dims.clone(), out).with_class(class)))
}

// ---------------------------------------------------------------------------
// histcounts
// ---------------------------------------------------------------------------

fn nice_edges(lo: f64, hi: f64, nbins: Option<usize>, raw: f64) -> Vec<f64> {
    let scale = lo.abs().max(hi.abs());
    let range = hi - lo;
    let mut raw = if let Some(nb) = nbins {
        range / nb as f64
    } else {
        raw
    };
    if !raw.is_finite() || raw <= 0.0 {
        raw = range;
    }
    raw = raw.max(eps_of(scale));
    let build = |left: f64, width: f64, nb: usize, right: f64| -> Vec<f64> {
        let mut e: Vec<f64> = (0..nb).map(|k| left + k as f64 * width).collect();
        e.push(right);
        e
    };
    if range > eps_of(scale).sqrt().max(f64::MIN_POSITIVE) {
        let p10 = 10f64.powf(raw.log10().floor());
        let rel = raw / p10;
        match nbins {
            None => {
                let mult = if rel < 1.5 {
                    1.0
                } else if rel < 2.5 {
                    2.0
                } else if rel < 4.0 {
                    3.0
                } else if rel < 7.5 {
                    5.0
                } else {
                    10.0
                };
                let w = mult * p10;
                let left = (w * (lo / w).floor()).min(lo);
                let nb = ((hi - left) / w).ceil().clamp(1.0, 1e7) as usize;
                let right = (left + nb as f64 * w).max(hi);
                build(left, w, nb, right)
            }
            Some(nb) => {
                let mut w = p10 * rel.floor();
                let left = (w * (lo / w).floor()).min(lo);
                if nb > 1 {
                    let ll = (hi - left) / nb as f64;
                    let ul = (hi - left) / (nb - 1) as f64;
                    let p = 10f64.powf((ul - ll).log10().floor());
                    w = p * (ll / p).ceil();
                }
                let right = (left + nb as f64 * w).max(hi);
                build(left, w, nb, right)
            }
        }
    } else {
        let nb = nbins.unwrap_or(1).max(1);
        let br = (nb as f64 * eps_of(scale)).ceil().max(1.0);
        let left = (2.0 * (lo - br / 4.0)).floor() / 2.0;
        let right = (2.0 * (hi + br / 4.0)).ceil() / 2.0;
        let w = (right - left) / nb as f64;
        build(left, w, nb, right)
    }
}

fn histcounts(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "histcounts")?;
    let x = arg_num(&a[0], "histcounts")?;
    let data = &x.re;
    let mut i = 1;
    let mut edges: Option<Vec<f64>> = None;
    let mut nbins: Option<usize> = None;
    let nb_of = |v: &Value| -> R<usize> {
        let n = arg_usize(v, "histcounts")?;
        if n == 0 || n > 10_000_000 {
            return Err(err(
                "MATLAB:histcounts:InvalidNumBins",
                "Number of bins must be a positive integer.",
            ));
        }
        Ok(n)
    };
    if let Some(v) = a.get(1)
        && !v.is_char()
    {
        let e = arg_num(v, "histcounts")?.re;
        if e.len() == 1 {
            nbins = Some(nb_of(v)?);
        } else {
            edges = Some(e);
        }
        i = 2;
    }
    let (mut width, mut limits, mut norm, mut method) =
        (None, None, "count".to_string(), "auto".to_string());
    while i < a.len() {
        let name = arg_str(&a[i], "histcounts")?.to_ascii_lowercase();
        let Some(v) = a.get(i + 1) else {
            return Err(err(
                "MATLAB:histcounts:ArgumentWithoutValue",
                "histcounts: name-value arguments must come in pairs.",
            ));
        };
        match name.as_str() {
            "binedges" => edges = Some(arg_num(v, "histcounts")?.re),
            "numbins" => nbins = Some(nb_of(v)?),
            "binwidth" => {
                let w = arg_f64(v, "histcounts")?;
                if w.is_nan() || w <= 0.0 {
                    return Err(err(
                        "MATLAB:histcounts:InvalidBinWidth",
                        "Bin width must be a positive scalar.",
                    ));
                }
                width = Some(w)
            }
            "binlimits" => {
                let l = arg_num(v, "histcounts")?.re;
                if l.len() != 2 || l[0].is_nan() || l[1].is_nan() || l[0] > l[1] {
                    return Err(err(
                        "MATLAB:histcounts:InvalidBinLimits",
                        "Bin limits must be a 2-element ascending vector.",
                    ));
                }
                limits = Some((l[0], l[1]))
            }
            "normalization" => norm = arg_str(v, "histcounts")?.to_ascii_lowercase(),
            "binmethod" => method = arg_str(v, "histcounts")?.to_ascii_lowercase(),
            other => {
                return Err(err(
                    "MATLAB:histcounts:UnknownParameter",
                    format!("histcounts: unknown parameter '{}'.", other),
                ));
            }
        }
        i += 2;
    }
    let edges = match edges {
        Some(e) => {
            if e.windows(2)
                .any(|w| w[0] > w[1] || w[0].is_nan() || w[1].is_nan())
            {
                return Err(err(
                    "MATLAB:histcounts:InvalidEdges",
                    "Bin edges must be a vector of nondecreasing values.",
                ));
            }
            e
        }
        None => {
            let fin: Vec<f64> = data
                .iter()
                .cloned()
                .filter(|v| v.is_finite() && limits.is_none_or(|(l, h)| *v >= l && *v <= h))
                .collect();
            if let Some((l, h)) = limits {
                let nb = if let Some(n) = nbins {
                    n
                } else if let Some(w) = width {
                    ((h - l) / w).ceil().clamp(1.0, 1e7) as usize
                } else {
                    let e = nice_edges(l, h, None, scott(&fin));
                    e.len().saturating_sub(1).max(1)
                };
                (0..=nb)
                    .map(|k| {
                        if k == nb {
                            h
                        } else {
                            l + (h - l) * k as f64 / nb as f64
                        }
                    })
                    .collect()
            } else if fin.is_empty() {
                vec![0.0, 1.0]
            } else {
                let lo = fin.iter().cloned().fold(f64::INFINITY, f64::min);
                let hi = fin.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                if let Some(w) = width {
                    let left = (lo / w).floor() * w;
                    let nb = ((hi - left) / w).ceil().clamp(1.0, 1e7) as usize;
                    let mut e: Vec<f64> = (0..=nb).map(|k| left + k as f64 * w).collect();
                    if e[nb] < hi {
                        e.push(e[nb] + w);
                    }
                    e
                } else if nbins.is_none() && method == "integers" {
                    let l = lo.floor() - 0.5;
                    let nb = ((hi.ceil() + 0.5 - l).round().clamp(1.0, 1e7)) as usize;
                    (0..=nb).map(|k| l + k as f64).collect()
                } else {
                    let raw = match method.as_str() {
                        "sqrt" => (hi - lo) / (fin.len() as f64).sqrt(),
                        "sturges" => (hi - lo) / ((fin.len() as f64).log2() + 1.0),
                        _ => scott(&fin),
                    };
                    nice_edges(lo, hi, nbins, raw)
                }
            }
        }
    };
    let nb = edges.len().saturating_sub(1);
    let mut n = vec![0.0; nb];
    let mut bin = vec![0.0; data.len()];
    for (k, &v) in data.iter().enumerate() {
        if v.is_nan() || nb == 0 || v < edges[0] || v > edges[nb] {
            continue;
        }
        let j = if v == edges[nb] {
            nb - 1
        } else {
            edges
                .partition_point(|e| *e <= v)
                .saturating_sub(1)
                .min(nb - 1)
        };
        n[j] += 1.0;
        bin[k] = (j + 1) as f64;
    }
    let total = data.len() as f64;
    let w: Vec<f64> = edges.windows(2).map(|e| e[1] - e[0]).collect();
    let cum = |v: &[f64]| {
        let mut s = 0.0;
        v.iter()
            .map(|x| {
                s += x;
                s
            })
            .collect::<Vec<f64>>()
    };
    let n = match norm.as_str() {
        "count" => n,
        "probability" => n.iter().map(|c| c / total).collect(),
        "countdensity" => n.iter().zip(&w).map(|(c, w)| c / w).collect(),
        "pdf" => n.iter().zip(&w).map(|(c, w)| c / total / w).collect(),
        "cumcount" => cum(&n),
        "cdf" => cum(&n.iter().map(|c| c / total).collect::<Vec<_>>()),
        other => {
            return Err(err(
                "MATLAB:histcounts:InvalidNormalization",
                format!("histcounts: unknown normalization '{}'.", other),
            ));
        }
    };
    take_n(
        vec![
            Value::row(n),
            Value::row(edges),
            Value::matrix(x.dims.clone(), bin),
        ],
        nargout,
    )
}

fn scott(v: &[f64]) -> f64 {
    let n = v.len() as f64;
    if v.len() < 2 {
        return 0.0;
    }
    let mean = v.iter().sum::<f64>() / n;
    let sd = (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
    3.5 * sd / n.cbrt()
}

// ---------------------------------------------------------------------------
// gradient, trapz, filter, conv, polynomials
// ---------------------------------------------------------------------------

fn grad1(f: &[C], x: &[f64]) -> Vec<C> {
    let n = f.len();
    if n < 2 {
        return vec![CZERO; n];
    }
    let mut g = vec![CZERO; n];
    g[0] = cscale(csub(f[1], f[0]), 1.0 / (x[1] - x[0]));
    g[n - 1] = cscale(csub(f[n - 1], f[n - 2]), 1.0 / (x[n - 1] - x[n - 2]));
    for i in 1..n - 1 {
        g[i] = cscale(csub(f[i + 1], f[i - 1]), 1.0 / (x[i + 1] - x[i - 1]));
    }
    g
}

fn gradient(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "gradient")?;
    let x = arg_num(&a[0], "gradient")?;
    let data = cdata(&x);
    let dims = x.dims.clone();
    let is_vec = is_vector_dims(&dims);
    let order: Vec<usize> = if is_vec {
        vec![if dims[0] == 1 { 1 } else { 0 }]
    } else {
        let mut o = vec![1, 0];
        o.extend(2..dims.len());
        o
    };
    let nout = if is_vec {
        1
    } else {
        nargout.max(1).min(order.len())
    };
    let sp = &a[1..];
    let mut outs = Vec::new();
    for (k, &d) in order.iter().take(nout).enumerate() {
        let len = dims.get(d).copied().unwrap_or(1);
        let h = if sp.is_empty() {
            None
        } else if sp.len() == 1 {
            Some(&sp[0])
        } else {
            match sp.get(k) {
                Some(v) => Some(v),
                None => {
                    return Err(err(
                        "MATLAB:gradient:InvalidInputs",
                        "gradient: spacing must be given for every dimension.",
                    ));
                }
            }
        };
        let coords: Vec<f64> = match h {
            None => (0..len).map(|i| i as f64).collect(),
            Some(v) => {
                let hv = arg_num(v, "gradient")?.re;
                if hv.len() == 1 {
                    (0..len).map(|i| i as f64 * hv[0]).collect()
                } else if hv.len() == len {
                    hv
                } else {
                    return Err(err(
                        "MATLAB:gradient:InvalidGridSpacing",
                        "gradient: spacing vector length must match the size of F in that dimension.",
                    ));
                }
            }
        };
        let (od, out) = map_slices(&dims, &data, d, len, |_, s| grad1(s, &coords));
        outs.push(cvalue(od, &out));
    }
    Ok(outs)
}

fn trapz(a: &[Value], fname: &str, cumulative: bool) -> R<Vec<Value>> {
    need(a, 1, fname)?;
    let (xv, y, dim) = match a.len() {
        1 => {
            let y = arg_num(&a[0], fname)?;
            let d = default_dim(&y.dims);
            (None, y, d)
        }
        2 if a[1].is_scalar() => {
            let y = arg_num(&a[0], fname)?;
            (None, y, dim_arg(&a[1], fname)?)
        }
        2 => {
            let y = arg_num(&a[1], fname)?;
            let d = default_dim(&y.dims);
            (Some(arg_num(&a[0], fname)?), y, d)
        }
        _ => (
            Some(arg_num(&a[0], fname)?),
            arg_num(&a[1], fname)?,
            dim_arg(&a[2], fname)?,
        ),
    };
    if !cumulative && y.dims == [0, 0] && a.len() == 1 {
        return one(Value::num(0.0));
    }
    let len = y.dims.get(dim).copied().unwrap_or(1);
    let coords: Vec<f64> = match &xv {
        None => (0..len).map(|i| i as f64).collect(),
        Some(x) if x.re.len() == 1 => (0..len).map(|i| i as f64 * x.re[0]).collect(),
        Some(x) if x.re.len() == len => x.re.clone(),
        Some(_) => {
            return Err(err(
                &format!("MATLAB:{}:LengthXMismatchY", fname),
                "LENGTH(X) must match the length of the integration dimension of Y.",
            ));
        }
    };
    let data = cdata(&y);
    let out_len = if cumulative { len } else { 1 };
    let (od, out) = map_slices(&y.dims, &data, dim, out_len, |_, s| {
        let mut acc = CZERO;
        let mut r = Vec::with_capacity(s.len());
        if cumulative && !s.is_empty() {
            r.push(CZERO);
        }
        for i in 1..s.len() {
            let h = coords[i] - coords[i - 1];
            acc = cadd(acc, cscale(cadd(s[i], s[i - 1]), h / 2.0));
            if cumulative {
                r.push(acc);
            }
        }
        if !cumulative {
            r.push(acc);
        }
        r
    });
    one(cvalue(od, &out))
}

fn filter(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 3, "filter")?;
    let mut b = cdata(&arg_num(&a[0], "filter")?);
    let mut av = cdata(&arg_num(&a[1], "filter")?);
    let x = arg_num(&a[2], "filter")?;
    if b.is_empty() || av.is_empty() {
        return Err(err(
            "MATLAB:filter:emptyInput",
            "filter: coefficient vectors must not be empty.",
        ));
    }
    let a0 = av[0];
    if a0 == CZERO {
        return Err(err(
            "MATLAB:filter:firstCoeffZero",
            "First denominator filter coefficient must be non-zero.",
        ));
    }
    for v in b.iter_mut().chain(av.iter_mut()) {
        *v = c_div(*v, a0);
    }
    let n = b.len().max(av.len());
    b.resize(n, CZERO);
    av.resize(n, CZERO);
    let dim = match a.get(4) {
        Some(v) => dim_arg(v, "filter")?,
        None => default_dim(&x.dims),
    };
    let len = x.dims.get(dim).copied().unwrap_or(1);
    let nslices = if len == 0 { 0 } else { x.re.len() / len.max(1) };
    let zi: Option<Vec<C>> = match a.get(3) {
        Some(v) if !v.is_empty() => {
            let z = cdata(&arg_num(v, "filter")?);
            if z.len() != (n - 1) * nslices.max(1) {
                return Err(err(
                    "MATLAB:filter:invalidInitialConditions",
                    "Initial conditions must be a vector of length max(length(a),length(b))-1, or an array with the leading dimension of that size and the remaining dimensions matching those of x.",
                ));
            }
            Some(z)
        }
        _ => None,
    };
    let xd = cdata(&x);
    let mut zf: Vec<C> = Vec::new();
    let (od, y) = map_slices(&x.dims, &xd, dim, len, |s, slice| {
        let mut z: Vec<C> = match &zi {
            Some(zz) => zz[s * (n - 1)..(s + 1) * (n - 1)].to_vec(),
            None => vec![CZERO; n - 1],
        };
        let mut out = Vec::with_capacity(slice.len());
        for &xk in slice {
            let yk = cadd(c_mul(b[0], xk), z.first().copied().unwrap_or(CZERO));
            for i in 0..n - 1 {
                let next = if i + 1 < n - 1 { z[i + 1] } else { CZERO };
                z[i] = csub(cadd(c_mul(b[i + 1], xk), next), c_mul(av[i + 1], yk));
            }
            out.push(yk);
        }
        zf.extend(z);
        out
    });
    let mut outs = vec![cvalue(od, &y)];
    if nargout >= 2 {
        let mut zd = vec![n - 1];
        if !is_vector_dims(&x.dims) {
            zd.extend(
                x.dims
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != dim)
                    .map(|(_, d)| *d),
            );
        }
        let zd = norm_dims(zd);
        if numel_of(&zd) != zf.len() {
            let k = zf.len();
            outs.push(cvalue(vec![k, 1], &zf));
        } else {
            outs.push(cvalue(zd, &zf));
        }
    }
    Ok(outs)
}

fn vector_arg(v: &Value, fname: &str) -> R<Num> {
    let x = arg_num(v, fname)?;
    if !(x.re.is_empty() || is_vector_dims(&x.dims)) {
        return Err(err(
            &format!("MATLAB:{}:AorBNotVector", fname),
            "Inputs must be vectors.",
        ));
    }
    Ok(x)
}

fn is_col(x: &Num) -> bool {
    x.dims.len() == 2 && x.dims[1] == 1 && x.dims[0] > 1
}

fn oriented(v: &[C], col: bool) -> Value {
    let n = v.len();
    cvalue(if col { vec![n, 1] } else { vec![1, n] }, v)
}

fn conv_full(u: &[C], v: &[C]) -> Vec<C> {
    if u.is_empty() || v.is_empty() {
        return vec![];
    }
    let mut w = vec![CZERO; u.len() + v.len() - 1];
    for (i, a) in u.iter().enumerate() {
        if *a == CZERO {
            continue;
        }
        for (j, b) in v.iter().enumerate() {
            w[i + j] = cadd(w[i + j], c_mul(*a, *b));
        }
    }
    w
}

fn conv(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "conv")?;
    let u = vector_arg(&a[0], "conv")?;
    let v = vector_arg(&a[1], "conv")?;
    let shape = match a.get(2) {
        Some(s) => arg_str(s, "conv")?.to_ascii_lowercase(),
        None => "full".into(),
    };
    let (ud, vd) = (cdata(&u), cdata(&v));
    let full = conv_full(&ud, &vd);
    let (nu, nv) = (ud.len(), vd.len());
    match shape.as_str() {
        "full" => {
            let col = if nu > nv { is_col(&u) } else { is_col(&v) };
            one(oriented(&full, col))
        }
        "same" => {
            let off = nv / 2;
            let w: Vec<C> = (0..nu)
                .map(|k| full.get(k + off).copied().unwrap_or(CZERO))
                .collect();
            one(oriented(&w, is_col(&u)))
        }
        "valid" => {
            let len = if nv == 0 {
                0
            } else {
                (nu + 1).saturating_sub(nv)
            };
            let w: Vec<C> = (0..len).map(|k| full[k + nv - 1]).collect();
            one(oriented(&w, is_col(&u)))
        }
        other => Err(err(
            "MATLAB:conv:unknownShape",
            format!(
                "conv: SHAPE must be 'full', 'same' or 'valid' (got '{}').",
                other
            ),
        )),
    }
}

fn deconv(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "deconv")?;
    let y = vector_arg(&a[0], "deconv")?;
    let d = vector_arg(&a[1], "deconv")?;
    let (yd, dd) = (cdata(&y), cdata(&d));
    if dd.is_empty() || dd[0] == CZERO {
        return Err(err(
            "MATLAB:deconv:ZeroCoef1",
            "First coefficient of A must be non-zero.",
        ));
    }
    let col = is_col(&y);
    if yd.len() < dd.len() {
        return take_n(vec![Value::num(0.0), cvalue(y.dims.clone(), &yd)], nargout);
    }
    let nq = yd.len() - dd.len() + 1;
    let mut r = yd.clone();
    let mut q = vec![CZERO; nq];
    for k in 0..nq {
        q[k] = c_div(r[k], dd[0]);
        for (j, dj) in dd.iter().enumerate() {
            r[k + j] = csub(r[k + j], c_mul(q[k], *dj));
        }
        r[k] = CZERO;
    }
    take_n(vec![oriented(&q, col), cvalue(y.dims.clone(), &r)], nargout)
}

pub(super) fn polyval(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "polyval")?;
    let p = vector_arg(&a[0], "polyval")?;
    let x = arg_num(&a[1], "polyval")?;
    if nargout > 1 {
        return Err(err(
            "MATLAB:polyval:unsupported",
            "polyval: the error estimate output is not supported yet.",
        ));
    }
    let (mut mu0, mut mu1) = (0.0, 1.0);
    if let Some(mu) = a.get(3) {
        let m = arg_num(mu, "polyval")?.re;
        if m.len() < 2 {
            return Err(err(
                "MATLAB:polyval:InvalidMu",
                "MU must be a 2-element vector.",
            ));
        }
        (mu0, mu1) = (m[0], m[1]);
    }
    let pd = cdata(&p);
    let out: Vec<C> = cdata(&x)
        .iter()
        .map(|&xv| {
            let xv = cscale(csub(xv, (mu0, 0.0)), 1.0 / mu1);
            pd.iter().fold(CZERO, |acc, c| cadd(c_mul(acc, xv), *c))
        })
        .collect();
    one(cvalue(x.dims.clone(), &out))
}

fn roots_of(c: &[C]) -> R<Vec<C>> {
    if c.iter().any(|v| !v.0.is_finite() || !v.1.is_finite()) {
        return Err(err(
            "MATLAB:roots:NonFiniteInput",
            "Input to ROOTS must not contain NaN or Inf.",
        ));
    }
    let first = c.iter().position(|v| *v != CZERO);
    let Some(first) = first else {
        return Ok(vec![]);
    };
    let last = c.iter().rposition(|v| *v != CZERO).unwrap_or(first);
    let trailing = c.len() - 1 - last;
    let p = &c[first..=last];
    let n = p.len();
    let mut r = Vec::new();
    if n > 1 {
        let m = n - 1;
        let mut cm = CMat::zeros(m, m);
        for j in 0..m {
            cm.set(0, j, cscale(c_div(p[j + 1], p[0]), -1.0));
        }
        for i in 1..m {
            cm.set(i, i - 1, (1.0, 0.0));
        }
        let mut ev = eig_values(&cm);
        if cm.is_real() {
            clean_real_eigs(&mut ev, cm.norm1());
        }
        r = ev;
    }
    r.extend(std::iter::repeat_n(CZERO, trailing));
    Ok(r)
}

fn roots(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "roots")?;
    let p = vector_arg(&a[0], "roots")?;
    let r = roots_of(&cdata(&p))?;
    let n = r.len();
    one(cvalue(vec![n, 1], &r))
}

fn poly(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "poly")?;
    let x = arg_num(&a[0], "poly")?;
    let (r, real_input): (Vec<C>, bool) = if x.re.is_empty() {
        (vec![], true)
    } else if is_vector_dims(&x.dims) {
        (cdata(&x), false)
    } else if x.dims.len() == 2 && x.dims[0] == x.dims[1] {
        if x.re
            .iter()
            .chain(x.im.iter().flatten())
            .any(|v| !v.is_finite())
        {
            return Err(err(
                "MATLAB:poly:InputNaNInf",
                "Input to POLY must not contain NaN or Inf.",
            ));
        }
        let m = CMat::from_num(&x);
        let real = m.is_real();
        let mut ev = eig_values(&m);
        if real {
            clean_real_eigs(&mut ev, m.norm1());
        }
        (ev, real)
    } else {
        return Err(err(
            "MATLAB:poly:InputSize",
            "Argument must be a vector or a square matrix.",
        ));
    };
    let mut c = vec![CZERO; r.len() + 1];
    c[0] = (1.0, 0.0);
    for (k, root) in r.iter().enumerate() {
        for j in (1..=k + 1).rev() {
            c[j] = csub(c[j], c_mul(*root, c[j - 1]));
        }
    }
    // real result when the roots are closed under conjugation
    let mut pos: Vec<C> = r.iter().filter(|v| v.1 > 0.0).cloned().collect();
    let mut neg: Vec<C> = r.iter().filter(|v| v.1 < 0.0).map(|v| cconj(*v)).collect();
    let key = |a: &C, b: &C| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal);
    pos.sort_by(key);
    neg.sort_by(key);
    if real_input || pos == neg {
        for v in c.iter_mut() {
            v.1 = 0.0;
        }
    }
    let n = c.len();
    one(cvalue(vec![1, n], &c))
}

// ---------------------------------------------------------------------------
// moving windows, rmmissing
// ---------------------------------------------------------------------------

enum Ends {
    Shrink,
    Discard,
    Fill(f64),
}

fn movfun(a: &[Value], fname: &str, mean: bool) -> R<Vec<Value>> {
    need(a, 2, fname)?;
    let x = arg_num(&a[0], fname)?;
    let k = arg_num(&a[1], fname)?.re;
    let bad_k = || {
        err(
            &format!("MATLAB:{}:wrongWindowLength", fname),
            "Window length must be a finite positive scalar or a 2-element vector of nonnegative integers.",
        )
    };
    let (kb, kf) = match k.as_slice() {
        [k0] => {
            if !k0.is_finite() || *k0 <= 0.0 {
                return Err(bad_k());
            }
            if k0.fract() == 0.0 {
                let k = *k0 as usize;
                if k % 2 == 1 {
                    ((k - 1) / 2, (k - 1) / 2)
                } else {
                    (k / 2, k / 2 - 1)
                }
            } else {
                let h = (k0 / 2.0).floor() as usize;
                (h, h)
            }
        }
        [b, f] => {
            if !(b.is_finite()
                && f.is_finite()
                && *b >= 0.0
                && *f >= 0.0
                && b.fract() == 0.0
                && f.fract() == 0.0)
            {
                return Err(bad_k());
            }
            (*b as usize, *f as usize)
        }
        _ => return Err(bad_k()),
    };
    let mut dim = None;
    let mut omit = false;
    let mut ends = Ends::Shrink;
    let mut i = 2;
    while i < a.len() {
        if let Some(s) = a[i].to_str() {
            match s.to_ascii_lowercase().as_str() {
                "omitnan" | "omitmissing" => omit = true,
                "includenan" | "includemissing" => omit = false,
                "endpoints" => {
                    let Some(v) = a.get(i + 1) else {
                        return Err(err(
                            &format!("MATLAB:{}:missingValue", fname),
                            "Endpoints requires a value.",
                        ));
                    };
                    ends = match v.to_str() {
                        Some(e) => match e.to_ascii_lowercase().as_str() {
                            "shrink" => Ends::Shrink,
                            "discard" => Ends::Discard,
                            "fill" => Ends::Fill(f64::NAN),
                            other => {
                                return Err(err(
                                    &format!("MATLAB:{}:invalidEndpoints", fname),
                                    format!(
                                        "Endpoints must be 'shrink', 'discard', 'fill' or a numeric scalar (got '{}').",
                                        other
                                    ),
                                ));
                            }
                        },
                        None => Ends::Fill(arg_f64(v, fname)?),
                    };
                    i += 1;
                }
                "samplepoints" => {
                    return Err(err(
                        &format!("MATLAB:{}:unsupported", fname),
                        format!("{}: 'SamplePoints' is not supported yet.", fname),
                    ));
                }
                other => {
                    return Err(err(
                        &format!("MATLAB:{}:unknownOption", fname),
                        format!("{}: unknown option '{}'.", fname, other),
                    ));
                }
            }
        } else {
            dim = Some(dim_arg(&a[i], fname)?);
        }
        i += 1;
    }
    let dim = dim.unwrap_or_else(|| default_dim(&x.dims));
    let len = x.dims.get(dim).copied().unwrap_or(1);
    let out_len = match ends {
        Ends::Discard => len.saturating_sub(kb + kf),
        _ => len,
    };
    let data = cdata(&x);
    let (od, out) = map_slices(&x.dims, &data, dim, out_len, |_, s| {
        let n = s.len();
        let mut r = Vec::with_capacity(n);
        for i in 0..n {
            let full = i >= kb && i + kf < n;
            if !full {
                match ends {
                    Ends::Discard => continue,
                    Ends::Fill(v) => {
                        r.push((v, 0.0));
                        continue;
                    }
                    Ends::Shrink => {}
                }
            }
            let lo = i.saturating_sub(kb);
            let hi = (i + kf).min(n - 1);
            let (mut sum, mut cnt, mut nan) = (CZERO, 0usize, false);
            for v in &s[lo..=hi] {
                if v.0.is_nan() || v.1.is_nan() {
                    nan = true;
                    continue;
                }
                sum = cadd(sum, *v);
                cnt += 1;
            }
            let val = if nan && !omit {
                (f64::NAN, 0.0)
            } else if mean {
                if cnt == 0 {
                    (f64::NAN, 0.0)
                } else {
                    cscale(sum, 1.0 / cnt as f64)
                }
            } else {
                sum
            };
            r.push(val);
        }
        r
    });
    let v = cvalue(od, &out);
    if x.class == NumClass::Single
        && let Value::Num(n) = v
    {
        return one(Value::Num(n.with_class(NumClass::Single)));
    }
    one(v)
}

fn rmmissing(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "rmmissing")?;
    let v = &a[0];
    let mut dim: Option<usize> = None;
    let mut minnum = 1usize;
    let mut i = 1;
    while i < a.len() {
        if let Some(s) = a[i].to_str() {
            if s.eq_ignore_ascii_case("minnummissing") {
                let Some(n) = a.get(i + 1) else {
                    return Err(err(
                        "MATLAB:rmmissing:missingValue",
                        "MinNumMissing requires a value.",
                    ));
                };
                minnum = arg_usize(n, "rmmissing")?.max(1);
                i += 2;
                continue;
            }
            return Err(err(
                "MATLAB:rmmissing:unknownOption",
                format!("rmmissing: unknown option '{}'.", s),
            ));
        }
        dim = Some(dim_arg(&a[i], "rmmissing")?);
        i += 1;
    }
    let missing: Vec<bool> = match v {
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
        _ => {
            return Err(err(
                "MATLAB:rmmissing:FirstInputInvalid",
                "rmmissing: first input must be an array or a cell array of character vectors.",
            ));
        }
    };
    let dims = v.dims();
    if dims.len() > 2 {
        return Err(err(
            "MATLAB:rmmissing:NDArray",
            "rmmissing: input must be a vector or a matrix.",
        ));
    }
    let (r, c) = (dims[0], dims[1]);
    let d = dim.unwrap_or(if r == 1 && c != 1 { 1 } else { 0 });
    if d > 1 {
        return Err(err(
            "MATLAB:rmmissing:InvalidDim",
            "rmmissing: dimension must be 1 or 2.",
        ));
    }
    let (pos, odims, tf) = if d == 0 {
        let rm: Vec<bool> = (0..r)
            .map(|i| (0..c).filter(|j| missing[i + j * r]).count() >= minnum)
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
            .map(|j| (0..r).filter(|i| missing[i + j * r]).count() >= minnum)
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
    let out = take(v, pos, odims)?;
    take_n(vec![out, tf], nargout)
}

// ---------------------------------------------------------------------------
// fzero, fminbnd, optimset
// ---------------------------------------------------------------------------

const OPT_FIELDS: [&str; 8] = [
    "Display",
    "MaxFunEvals",
    "MaxIter",
    "TolFun",
    "TolX",
    "FunValCheck",
    "OutputFcn",
    "PlotFcns",
];

fn opt_value(opts: Option<&Value>, name: &str) -> Option<Value> {
    let Some(Value::Struct(s)) = opts else {
        return None;
    };
    if s.elems.is_empty() {
        return None;
    }
    let i = s.fields.iter().position(|f| f.eq_ignore_ascii_case(name))?;
    let v = &s.elems[0][i];
    if v.is_empty() { None } else { Some(v.clone()) }
}

fn opt_f64(opts: Option<&Value>, name: &str) -> Option<f64> {
    opt_value(opts, name).and_then(|v| v.scalar_f64())
}

fn optimset(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let mut s = StructArray::new_scalar();
    for f in OPT_FIELDS {
        s.set(0, f, Value::empty());
    }
    if a.is_empty() {
        if nargout == 0 {
            let mut t = String::new();
            for f in OPT_FIELDS {
                t.push_str(&format!("{:>20}: [ ]\n", f));
            }
            it.print(&t);
            return Ok(vec![]);
        }
        return one(Value::Struct(s));
    }
    if a.len() == 1
        && let Some(name) = a[0].to_str()
    {
        let defaults: Vec<(&str, Value)> = match name.to_ascii_lowercase().as_str() {
            "fzero" => vec![
                ("Display", Value::str("notify")),
                ("TolX", Value::num(f64::EPSILON)),
            ],
            "fminbnd" => vec![
                ("Display", Value::str("notify")),
                ("MaxFunEvals", Value::num(500.0)),
                ("MaxIter", Value::num(500.0)),
                ("TolX", Value::num(1e-4)),
            ],
            "fminsearch" => vec![
                ("Display", Value::str("notify")),
                ("MaxFunEvals", Value::str("200*numberofvariables")),
                ("MaxIter", Value::str("200*numberofvariables")),
                ("TolFun", Value::num(1e-4)),
                ("TolX", Value::num(1e-4)),
            ],
            _ => vec![],
        };
        for (k, v) in defaults {
            s.set(0, k, v);
        }
        return one(Value::Struct(s));
    }
    let canonical = |s: &StructArray, name: &str| -> String {
        if let Some(f) = s.fields.iter().find(|f| f.eq_ignore_ascii_case(name)) {
            return f.clone();
        }
        name.to_string()
    };
    let mut i = 0;
    let mut first_struct = true;
    while i < a.len() {
        match &a[i] {
            Value::Struct(old) => {
                if let Some(e) = old.elems.first() {
                    for (fi, f) in old.fields.iter().enumerate() {
                        let v = &e[fi];
                        if !first_struct && v.is_empty() {
                            continue;
                        }
                        let k = canonical(&s, f);
                        s.set(0, &k, v.clone());
                    }
                }
                first_struct = false;
                i += 1;
            }
            Value::Char(_) => {
                let name = arg_str(&a[i], "optimset")?;
                let Some(v) = a.get(i + 1) else {
                    return Err(err(
                        "MATLAB:optimset:ArgPairs",
                        "Arguments must occur in name-value pairs.",
                    ));
                };
                let k = canonical(&s, &name);
                s.set(0, &k, v.clone());
                i += 2;
            }
            _ => {
                return Err(err(
                    "MATLAB:optimset:invalidInput",
                    "optimset: expected an options structure or a parameter name.",
                ));
            }
        }
    }
    one(Value::Struct(s))
}

fn optimget(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "optimget")?;
    if !matches!(a[0], Value::Struct(_)) {
        return Err(err(
            "MATLAB:optimget:invalidOptions",
            "First argument must be an options structure created with optimset.",
        ));
    }
    let name = arg_str(&a[1], "optimget")?;
    match opt_value(Some(&a[0]), &name) {
        Some(v) => one(v),
        None => one(a.get(2).cloned().unwrap_or_default()),
    }
}

fn fzero(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "fzero")?;
    let f = a[0].clone();
    let opts = a.get(2).filter(|v| matches!(v, Value::Struct(_)));
    let extra: Vec<Value> = if a.len() > 3 { a[3..].to_vec() } else { vec![] };
    let tolx = opt_f64(opts, "TolX").unwrap_or(f64::EPSILON);
    let x0 = arg_num(&a[1], "fzero")?;
    if x0.im.is_some() || x0.re.iter().any(|v| !v.is_finite()) {
        return Err(err(
            "MATLAB:fzero:Arg2NotFinite",
            "Second argument must be finite.",
        ));
    }
    let fcount = std::cell::Cell::new(0usize);
    let eval = |it: &mut Interp, x: f64| -> R<f64> {
        fcount.set(fcount.get() + 1);
        call_scalar(it, &f, x, &extra, "fzero")
    };
    let mut interval_its = 0usize;
    let (lo, flo, hi, fhi);
    match x0.re.as_slice() {
        [p, q] => {
            (lo, hi) = (*p, *q);
            flo = eval(it, lo)?;
            fhi = eval(it, hi)?;
            if !flo.is_finite() || !fhi.is_finite() {
                return Err(err(
                    "MATLAB:fzero:ValueAtEndPtComplexOrNotFinite",
                    "Function values at the interval endpoints must be finite and real.",
                ));
            }
            if flo.signum() * fhi.signum() > 0.0 {
                return Err(err(
                    "MATLAB:fzero:ValuesAtEndPtsSameSign",
                    "The function values at the interval endpoints must differ in sign.",
                ));
            }
        }
        [x] => {
            let x = *x;
            let fx = eval(it, x)?;
            if !fx.is_finite() {
                return Err(err(
                    "MATLAB:fzero:ValueAtInitPtComplexOrNotFinite",
                    "Function value at starting guess must be finite and real.",
                ));
            }
            if fx == 0.0 {
                let out = make_struct(vec![
                    ("intervaliterations", Value::num(0.0)),
                    ("iterations", Value::num(0.0)),
                    ("funcCount", Value::num(fcount.get() as f64)),
                    ("algorithm", Value::str("bisection, interpolation")),
                    ("message", Value::str("Zero found at the starting point.")),
                ]);
                return take_n(
                    vec![Value::num(x), Value::num(0.0), Value::num(1.0), out],
                    nargout,
                );
            }
            let mut dx = if x == 0.0 { 1.0 / 50.0 } else { x.abs() / 50.0 };
            let (mut found, mut l, mut fl, mut h, mut fh) = (false, x, fx, x, fx);
            while dx < 1e300 {
                interval_its += 1;
                let (a1, b1) = (x - dx, x + dx);
                let fa = eval(it, a1)?;
                let fb = eval(it, b1)?;
                if !fa.is_finite() || !fb.is_finite() {
                    break;
                }
                if fa.signum() * fx.signum() <= 0.0 {
                    (l, fl, h, fh) = (a1, fa, x, fx);
                    found = true;
                    break;
                }
                if fb.signum() * fx.signum() <= 0.0 {
                    (l, fl, h, fh) = (x, fx, b1, fb);
                    found = true;
                    break;
                }
                if fa.signum() * fb.signum() <= 0.0 {
                    (l, fl, h, fh) = (a1, fa, b1, fb);
                    found = true;
                    break;
                }
                dx *= std::f64::consts::SQRT_2;
            }
            if !found {
                let out = make_struct(vec![
                    ("intervaliterations", Value::num(interval_its as f64)),
                    ("iterations", Value::num(0.0)),
                    ("funcCount", Value::num(fcount.get() as f64)),
                    ("algorithm", Value::str("bisection, interpolation")),
                    (
                        "message",
                        Value::str(
                            "Exiting fzero: aborting search for an interval containing a sign change.",
                        ),
                    ),
                ]);
                return take_n(
                    vec![
                        Value::num(f64::NAN),
                        Value::num(f64::NAN),
                        Value::num(-6.0),
                        out,
                    ],
                    nargout,
                );
            }
            (lo, flo, hi, fhi) = (l, fl, h, fh);
        }
        _ => {
            return Err(err(
                "MATLAB:fzero:LengthArg2",
                "Second argument must be a scalar or vector of length 2.",
            ));
        }
    }
    // Brent's method (zeroin)
    let (mut a_, mut fa, mut b, mut fb) = (lo, flo, hi, fhi);
    let (mut c, mut fc) = (a_, fa);
    let mut d = b - a_;
    let mut e = d;
    let mut iters = 0usize;
    if fa == 0.0 {
        b = a_;
        fb = fa;
    }
    while fb != 0.0 && iters < 1000 {
        if fb.signum() == fc.signum() {
            c = a_;
            fc = fa;
            d = b - a_;
            e = d;
        }
        if fc.abs() < fb.abs() {
            a_ = b;
            b = c;
            c = a_;
            fa = fb;
            fb = fc;
            fc = fa;
        }
        let tol = 2.0 * f64::EPSILON * b.abs() + 0.5 * tolx;
        let m = 0.5 * (c - b);
        if m.abs() <= tol || fb == 0.0 {
            break;
        }
        iters += 1;
        if e.abs() < tol || fa.abs() <= fb.abs() {
            d = m;
            e = m;
        } else {
            let s = fb / fa;
            let (mut p, mut q);
            if a_ == c {
                p = 2.0 * m * s;
                q = 1.0 - s;
            } else {
                let q0 = fa / fc;
                let r = fb / fc;
                p = s * (2.0 * m * q0 * (q0 - r) - (b - a_) * (r - 1.0));
                q = (q0 - 1.0) * (r - 1.0) * (s - 1.0);
            }
            if p > 0.0 {
                q = -q;
            } else {
                p = -p;
            }
            if 2.0 * p < 3.0 * m * q - (tol * q).abs() && p < (0.5 * e * q).abs() {
                e = d;
                d = p / q;
            } else {
                d = m;
                e = m;
            }
        }
        a_ = b;
        fa = fb;
        b += if d.abs() > tol {
            d
        } else if m > 0.0 {
            tol
        } else {
            -tol
        };
        fb = eval(it, b)?;
        if !fb.is_finite() {
            break;
        }
    }
    let out = make_struct(vec![
        ("intervaliterations", Value::num(interval_its as f64)),
        ("iterations", Value::num(iters as f64)),
        ("funcCount", Value::num(fcount.get() as f64)),
        ("algorithm", Value::str("bisection, interpolation")),
        ("message", Value::str("Zero found in the interval")),
    ]);
    take_n(
        vec![Value::num(b), Value::num(fb), Value::num(1.0), out],
        nargout,
    )
}

fn fminbnd(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 3, "fminbnd")?;
    let f = a[0].clone();
    let mut lo = arg_f64(&a[1], "fminbnd")?;
    let mut hi = arg_f64(&a[2], "fminbnd")?;
    let opts = a.get(3).filter(|v| matches!(v, Value::Struct(_)));
    let extra: Vec<Value> = if a.len() > 4 { a[4..].to_vec() } else { vec![] };
    if !lo.is_finite() || !hi.is_finite() {
        return Err(err(
            "MATLAB:fminbnd:NonFiniteBounds",
            "Bounds must be finite.",
        ));
    }
    if lo > hi {
        return Err(err(
            "MATLAB:fminbnd:LowerGTUpper",
            "The lower bound exceeds the upper bound.",
        ));
    }
    let tolx = opt_f64(opts, "TolX").unwrap_or(1e-4);
    let maxfun = opt_f64(opts, "MaxFunEvals").unwrap_or(500.0) as usize;
    let maxit = opt_f64(opts, "MaxIter").unwrap_or(500.0) as usize;
    let fcount = std::cell::Cell::new(0usize);
    let eval = |it: &mut Interp, x: f64| -> R<f64> {
        fcount.set(fcount.get() + 1);
        call_scalar(it, &f, x, &extra, "fminbnd")
    };
    let cg = 0.5 * (3.0 - 5f64.sqrt());
    let mut x = lo + cg * (hi - lo);
    let (mut v, mut w) = (x, x);
    let (mut d, mut e) = (0.0f64, 0.0f64);
    let mut fx = eval(it, x)?;
    let (mut fv, mut fw) = (fx, fx);
    let mut iters = 0usize;
    let mut flag = 1.0;
    loop {
        let xm = 0.5 * (lo + hi);
        let tol1 = f64::EPSILON.sqrt() * x.abs() + tolx / 3.0;
        let tol2 = 2.0 * tol1;
        if (x - xm).abs() <= tol2 - 0.5 * (hi - lo) {
            break;
        }
        if iters >= maxit || fcount.get() >= maxfun {
            flag = 0.0;
            break;
        }
        iters += 1;
        let mut golden = true;
        if e.abs() > tol1 {
            let r = (x - w) * (fx - fv);
            let mut q = (x - v) * (fx - fw);
            let mut p = (x - v) * q - (x - w) * r;
            q = 2.0 * (q - r);
            if q > 0.0 {
                p = -p;
            }
            q = q.abs();
            let r = e;
            e = d;
            if p.abs() < (0.5 * q * r).abs() && p > q * (lo - x) && p < q * (hi - x) {
                d = p / q;
                let u = x + d;
                if (u - lo) < tol2 || (hi - u) < tol2 {
                    d = if xm >= x { tol1 } else { -tol1 };
                }
                golden = false;
            }
        }
        if golden {
            e = if x >= xm { lo - x } else { hi - x };
            d = cg * e;
        }
        let u = x + if d.abs() >= tol1 {
            d
        } else if d >= 0.0 {
            tol1
        } else {
            -tol1
        };
        let fu = eval(it, u)?;
        if fu <= fx {
            if u >= x {
                lo = x;
            } else {
                hi = x;
            }
            v = w;
            fv = fw;
            w = x;
            fw = fx;
            x = u;
            fx = fu;
        } else {
            if u < x {
                lo = u;
            } else {
                hi = u;
            }
            if fu <= fw || w == x {
                v = w;
                fv = fw;
                w = u;
                fw = fu;
            } else if fu <= fv || v == x || v == w {
                v = u;
                fv = fu;
            }
        }
    }
    let out = make_struct(vec![
        ("iterations", Value::num(iters as f64)),
        ("funcCount", Value::num(fcount.get() as f64)),
        (
            "algorithm",
            Value::str("golden section search, parabolic interpolation"),
        ),
        (
            "message",
            Value::str(if flag == 1.0 {
                "Optimization terminated."
            } else {
                "Exiting: maximum number of iterations or function evaluations exceeded."
            }),
        ),
    ]);
    take_n(
        vec![Value::num(x), Value::num(fx), Value::num(flag), out],
        nargout,
    )
}

// ---------------------------------------------------------------------------
// integral (adaptive Gauss–Kronrod 7-15)
// ---------------------------------------------------------------------------

const XGK: [f64; 8] = [
    0.991_455_371_120_812_6,
    0.949_107_912_342_758_5,
    0.864_864_423_359_769_1,
    0.741_531_185_599_394_4,
    0.586_087_235_467_691_1,
    0.405_845_151_377_397_2,
    0.207_784_955_007_898_5,
    0.0,
];
const WGK: [f64; 8] = [
    0.022_935_322_010_529_22,
    0.063_092_092_629_978_55,
    0.104_790_010_322_250_18,
    0.140_653_259_715_525_92,
    0.169_004_726_639_267_9,
    0.190_350_578_064_785_4,
    0.204_432_940_075_298_9,
    0.209_482_141_084_727_83,
];
const WG: [f64; 4] = [
    0.129_484_966_168_869_7,
    0.279_705_391_489_276_7,
    0.381_830_050_505_118_9,
    0.417_959_183_673_469_4,
];

#[derive(Clone, Copy)]
enum Map {
    /// [a, b] with the endpoint-smoothing cubic map from t in [-1, 1].
    Finite(f64, f64),
    RightInf(f64),
    LeftInf(f64),
    BothInf,
}

struct Integrand<'a> {
    f: &'a Value,
    extra: &'a [Value],
    array_valued: bool,
    map: Map,
    dims: Option<Vec<usize>>,
    fname: &'a str,
}

impl Integrand<'_> {
    fn xw(&self, t: f64) -> (f64, f64) {
        match self.map {
            Map::Finite(a, b) => {
                let h = 0.25 * (b - a);
                (
                    0.5 * (a + b) + h * t * (3.0 - t * t),
                    3.0 * h * (1.0 - t * t),
                )
            }
            Map::RightInf(a) => (a + t / (1.0 - t), 1.0 / ((1.0 - t) * (1.0 - t))),
            Map::LeftInf(b) => (b - t / (1.0 - t), 1.0 / ((1.0 - t) * (1.0 - t))),
            Map::BothInf => {
                let d = 1.0 - t * t;
                (t / d, (1.0 + t * t) / (d * d))
            }
        }
    }

    fn call(&mut self, it: &mut Interp, x: Value) -> R<Num> {
        let mut args = vec![x];
        args.extend(self.extra.iter().cloned());
        let v = call1(it, self.f, args, self.fname)?;
        v.to_num().ok_or_else(|| {
            err(
                &format!("MATLAB:{}:invalidFunctionOutput", self.fname),
                format!("{}: the integrand must return numeric values.", self.fname),
            )
        })
    }

    /// Weighted integrand values at the points `ts`; one vector per point.
    fn eval(&mut self, it: &mut Interp, ts: &[f64]) -> R<Vec<Vec<C>>> {
        let xw: Vec<(f64, f64)> = ts.iter().map(|t| self.xw(*t)).collect();
        let mut out = Vec::with_capacity(ts.len());
        if !self.array_valued {
            let r = self.call(it, Value::row(xw.iter().map(|p| p.0).collect()))?;
            if r.re.len() == ts.len() {
                for (k, (_, w)) in xw.iter().enumerate() {
                    out.push(vec![cscale((r.re[k], r.im_at(k)), *w)]);
                }
                return Ok(out);
            }
            if r.re.len() != 1 {
                return Err(err(
                    &format!("MATLAB:{}:FxNotSameSizeAsX", self.fname),
                    "Output of the function must be the same size as the input. If FUN is an array-valued integrand, set the 'ArrayValued' option to true.",
                ));
            }
            // a scalar answer for a vector input: evaluate point by point
        }
        for (x, w) in &xw {
            let r = self.call(it, Value::num(*x))?;
            match &self.dims {
                None => self.dims = Some(r.dims.clone()),
                Some(d) if numel_of(d) != r.re.len() => {
                    return Err(err(
                        &format!("MATLAB:{}:inconsistentSize", self.fname),
                        "The integrand must return arrays of the same size at every point.",
                    ));
                }
                _ => {}
            }
            if !self.array_valued && r.re.len() != 1 {
                return Err(err(
                    &format!("MATLAB:{}:FxNotSameSizeAsX", self.fname),
                    "Output of the function must be the same size as the input.",
                ));
            }
            out.push(
                (0..r.re.len())
                    .map(|k| cscale((r.re[k], r.im_at(k)), *w))
                    .collect(),
            );
        }
        Ok(out)
    }

    fn gk(&mut self, it: &mut Interp, l: f64, r: f64) -> R<(Vec<C>, f64)> {
        let c = 0.5 * (l + r);
        let h = 0.5 * (r - l);
        let mut ts = Vec::with_capacity(15);
        for x in &XGK[..7] {
            ts.push(c - h * x);
        }
        ts.push(c);
        for x in XGK[..7].iter().rev() {
            ts.push(c + h * x);
        }
        let v = self.eval(it, &ts)?;
        let m = v.first().map_or(0, |e| e.len());
        let mut k = vec![CZERO; m];
        let mut g = vec![CZERO; m];
        for j in 0..7 {
            for q in 0..m {
                k[q] = cadd(k[q], cscale(cadd(v[j][q], v[14 - j][q]), WGK[j]));
            }
            if j % 2 == 1 {
                for q in 0..m {
                    g[q] = cadd(g[q], cscale(cadd(v[j][q], v[14 - j][q]), WG[j / 2]));
                }
            }
        }
        for q in 0..m {
            k[q] = cadd(k[q], cscale(v[7][q], WGK[7]));
            g[q] = cadd(g[q], cscale(v[7][q], WG[3]));
        }
        let k: Vec<C> = k.into_iter().map(|e| cscale(e, h)).collect();
        let e = k
            .iter()
            .zip(&g)
            .map(|(a, b)| c_abs(csub(*a, cscale(*b, h))))
            .fold(0.0, f64::max);
        Ok((k, e))
    }
}

fn integrate(
    it: &mut Interp,
    f: &Value,
    extra: &[Value],
    a: f64,
    b: f64,
    abstol: f64,
    reltol: f64,
    array_valued: bool,
    waypoints: &[f64],
    max_intervals: usize,
    fname: &str,
) -> R<(Vec<C>, Vec<usize>, f64)> {
    let mut ig = Integrand {
        f,
        extra,
        array_valued,
        map: Map::BothInf,
        dims: None,
        fname,
    };
    if a.is_nan() || b.is_nan() {
        return Ok((vec![(f64::NAN, 0.0)], vec![1, 1], f64::NAN));
    }
    let (lo, hi, sign) = if a <= b { (a, b, 1.0) } else { (b, a, -1.0) };
    if lo == hi {
        let dims = if array_valued && lo.is_finite() {
            let r = ig.call(it, Value::num(lo))?;
            r.dims
        } else {
            vec![1, 1]
        };
        let n = numel_of(&dims);
        return Ok((vec![CZERO; n], dims, 0.0));
    }
    // segments (each with its own variable map), split into ~10 pieces
    let segs: Vec<(Map, f64, f64)> = match (lo.is_finite(), hi.is_finite()) {
        (true, true) => {
            let mut pts = vec![lo];
            let mut wp: Vec<f64> = waypoints
                .iter()
                .cloned()
                .filter(|w| *w > lo && *w < hi)
                .collect();
            wp.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
            pts.extend(wp);
            pts.push(hi);
            pts.windows(2)
                .map(|w| (Map::Finite(w[0], w[1]), -1.0, 1.0))
                .collect()
        }
        (true, false) => vec![(Map::RightInf(lo), 0.0, 1.0)],
        (false, true) => vec![(Map::LeftInf(hi), 0.0, 1.0)],
        (false, false) => vec![(Map::BothInf, -1.0, 1.0)],
    };
    let pieces = 10usize.div_ceil(segs.len()).max(1);
    let mut ivals: Vec<(Map, f64, f64, Vec<C>, f64)> = Vec::new();
    for (map, tl, th) in segs {
        ig.map = map;
        for k in 0..pieces {
            let l = tl + (th - tl) * k as f64 / pieces as f64;
            let r = if k + 1 == pieces {
                th
            } else {
                tl + (th - tl) * (k + 1) as f64 / pieces as f64
            };
            let (q, e) = ig.gk(it, l, r)?;
            ivals.push((map, l, r, q, e));
        }
    }
    let m = ivals.first().map_or(1, |v| v.3.len());
    let total = |iv: &[(Map, f64, f64, Vec<C>, f64)]| -> (Vec<C>, f64) {
        let mut q = vec![CZERO; m];
        let mut e = 0.0;
        for v in iv {
            for k in 0..m.min(v.3.len()) {
                q[k] = cadd(q[k], v.3[k]);
            }
            e += v.4;
        }
        (q, e)
    };
    loop {
        let (q, e) = total(&ivals);
        let qabs = q.iter().map(|c| c_abs(*c)).fold(0.0, f64::max);
        if !e.is_finite() || q.iter().any(|c| !c.0.is_finite() || !c.1.is_finite()) {
            it.warning(
                &format!("MATLAB:{}:NonFiniteValue", fname),
                "Inf or NaN value encountered.",
            );
            break;
        }
        if e <= abstol.max(reltol * qabs) {
            break;
        }
        if ivals.len() >= max_intervals {
            it.warning(
                &format!("MATLAB:{}:MaxIntervalCountReached", fname),
                &format!(
                    "Reached the limit on the maximum number of intervals in use. Approximate bound on error is {:9.1e}. The integral may not exist, or it may be difficult to approximate numerically to the requested accuracy.",
                    e
                ),
            );
            break;
        }
        let (worst, _) =
            ivals.iter().enumerate().fold(
                (0, -1.0),
                |(bi, be), (i, v)| if v.4 > be { (i, v.4) } else { (bi, be) },
            );
        let (map, l, r) = (ivals[worst].0, ivals[worst].1, ivals[worst].2);
        let mid = 0.5 * (l + r);
        if mid <= l || mid >= r {
            it.warning(
                &format!("MATLAB:{}:MinStepSize", fname),
                "Minimum step size reached; singularity possible.",
            );
            break;
        }
        ig.map = map;
        let (q1, e1) = ig.gk(it, l, mid)?;
        let (q2, e2) = ig.gk(it, mid, r)?;
        ivals[worst] = (map, l, mid, q1, e1);
        ivals.push((map, mid, r, q2, e2));
    }
    let (q, e) = total(&ivals);
    let dims = if array_valued {
        ig.dims.clone().unwrap_or(vec![1, 1])
    } else {
        vec![1, 1]
    };
    Ok((q.into_iter().map(|c| cscale(c, sign)).collect(), dims, e))
}

fn quad_entry(it: &mut Interp, a: Vec<Value>, nargout: usize, fname: &str) -> R<Vec<Value>> {
    need(&a, 3, fname)?;
    let f = a[0].clone();
    let lim = |v: &Value| -> R<f64> {
        match v {
            Value::Num(n) if n.im.is_some() => Err(err(
                &format!("MATLAB:{}:complexLimits", fname),
                format!(
                    "{}: complex integration limits are not supported yet.",
                    fname
                ),
            )),
            _ => arg_f64(v, fname),
        }
    };
    let lo = lim(&a[1])?;
    let hi = lim(&a[2])?;
    let mut abstol = 1e-10;
    let mut reltol = 1e-6;
    let mut array_valued = fname == "quadv";
    let mut waypoints: Vec<f64> = Vec::new();
    let mut max_intervals = 650usize;
    let mut extra: Vec<Value> = Vec::new();
    if matches!(fname, "quad" | "quadl" | "quadv") {
        abstol = 1e-6;
        reltol = 0.0;
        if let Some(v) = a.get(3)
            && !v.is_empty()
        {
            abstol = arg_f64(v, fname)?;
        }
        if a.len() > 5 {
            extra = a[5..].to_vec();
        }
    } else {
        let mut i = 3;
        while i < a.len() {
            let name = arg_str(&a[i], fname)?.to_ascii_lowercase();
            let Some(v) = a.get(i + 1) else {
                return Err(err(
                    &format!("MATLAB:{}:nameValuePairs", fname),
                    format!("{}: name-value arguments must come in pairs.", fname),
                ));
            };
            match name.as_str() {
                "abstol" => abstol = arg_f64(v, fname)?,
                "reltol" => reltol = arg_f64(v, fname)?,
                "arrayvalued" => array_valued = v.is_true()?,
                "waypoints" => waypoints = arg_num(v, fname)?.re,
                "maxintervalcount" => max_intervals = arg_usize(v, fname)?.max(1),
                other => {
                    return Err(err(
                        &format!("MATLAB:{}:unknownParameter", fname),
                        format!("{}: unknown parameter '{}'.", fname, other),
                    ));
                }
            }
            i += 2;
        }
    }
    if abstol.is_nan() || abstol < 0.0 || reltol.is_nan() || reltol < 0.0 {
        return Err(err(
            &format!("MATLAB:{}:invalidTolerance", fname),
            "Tolerances must be non-negative real scalars.",
        ));
    }
    let (q, dims, e) = integrate(
        it,
        &f,
        &extra,
        lo,
        hi,
        abstol,
        reltol,
        array_valued,
        &waypoints,
        max_intervals,
        fname,
    )?;
    let qv = cvalue(dims, &q);
    if fname == "quadgk" || fname == "quad" || fname == "quadl" {
        return take_n(vec![qv, Value::num(e)], nargout);
    }
    one(qv)
}
