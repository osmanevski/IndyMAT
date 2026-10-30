//! Built-in functions. Each module registers its own names.
//!
//! Signature: `fn(&mut Interp, args, nargout) -> R<Vec<Value>>`. A builtin
//! returns at least one value when `nargout >= 1` (or when it has a
//! natural result, so `ans` is set), and never panics on bad input.

use indy_value::{Array, Error, Num, NumClass, Value};

use crate::{BuiltinFn, Interp, R};

pub mod args;
pub mod audio;
pub mod cells;
pub mod clinalg;
pub mod control;
pub mod core;
pub mod datatypes;
pub mod descr;
pub mod dist;
pub mod extra;
pub mod fft;
pub mod fits;
pub mod handles;
pub mod htest;
pub mod image;
pub mod interp;
pub mod io;
pub mod linalg;
pub mod matfile;
pub mod math;
pub mod missing;
pub mod more;
pub mod multi;
pub mod ode;
pub mod optim;
pub mod pkg;
pub mod polywin;
pub mod regex;
pub mod regr;
pub mod signal;
pub mod sparse;
pub mod specfun;
pub mod stats;
pub mod strings;
pub mod strtype;
pub mod su;
pub mod timer;
pub mod xlsx;

pub use io::OpenFile;

pub fn register(it: &mut Interp) {
    core::register(it);
    args::register(it);
    cells::register(it);
    math::register(it);
    fft::register(it);
    strings::register(it);
    strtype::register(it);
    io::register(it);
    linalg::register(it);
    ode::register(it);
    more::register(it);
    handles::register(it);
    regex::register(it);
    extra::register(it);
    interp::register(it);
    polywin::register(it);
    stats::register(it);
    clinalg::register(it);
    signal::register(it);
    pkg::register(it);
    control::register(it);
    timer::register(it);
    datatypes::register(it);
    optim::register(it);
    missing::register(it);
    matfile::register(it);
    specfun::register(it);
    dist::register(it);
    descr::register(it);
    htest::register(it);
    multi::register(it);
    regr::register(it);
    fits::register(it);
    sparse::register(it);
    audio::register(it);
    image::register(it);
}

pub fn reg(it: &mut Interp, name: &'static str, f: BuiltinFn) {
    it.builtins.insert(name, f);
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

pub fn err(id: &str, msg: impl Into<String>) -> Box<Error> {
    Error::new(id, msg)
}

pub fn need(args: &[Value], n: usize, fname: &str) -> R<()> {
    if args.len() < n {
        return Err(Error::new(
            "MATLAB:minrhs",
            format!("Not enough input arguments (function '{}').", fname),
        ));
    }
    Ok(())
}

pub fn at_most(args: &[Value], n: usize, fname: &str) -> R<()> {
    if args.len() > n {
        return Err(Error::new(
            "MATLAB:TooManyInputs",
            format!("Too many input arguments (function '{}').", fname),
        ));
    }
    Ok(())
}

pub fn one(v: Value) -> R<Vec<Value>> {
    Ok(vec![v])
}

pub fn arg_str(v: &Value, fname: &str) -> R<String> {
    v.to_str().ok_or_else(|| {
        Error::new(
            "MATLAB:invalidType",
            format!(
                "{}: expected a character vector, got {}.",
                fname,
                v.class_name()
            ),
        )
    })
}

pub fn arg_f64(v: &Value, fname: &str) -> R<f64> {
    v.scalar_f64().ok_or_else(|| {
        Error::new(
            "MATLAB:invalidType",
            format!(
                "{}: expected a numeric scalar, got {} {}.",
                fname,
                indy_value::ops::dims_text(v.dims_ref()),
                v.class_name()
            ),
        )
    })
}

pub fn arg_usize(v: &Value, fname: &str) -> R<usize> {
    let x = arg_f64(v, fname)?;
    if x < 0.0 || x.fract() != 0.0 || !x.is_finite() {
        return Err(Error::new(
            "MATLAB:invalidType",
            format!("{}: expected a non-negative integer, got {}.", fname, x),
        ));
    }
    Ok(x as usize)
}

pub fn arg_num(v: &Value, fname: &str) -> R<Num> {
    v.to_num().ok_or_else(|| {
        Error::new(
            "MATLAB:invalidType",
            format!("{}: expected numeric input, got {}.", fname, v.class_name()),
        )
    })
}

pub fn arg_bool(v: &Value, fname: &str) -> R<bool> {
    match v.scalar_f64() {
        Some(x) => Ok(x != 0.0),
        None => Err(Error::new(
            "MATLAB:invalidType",
            format!("{}: expected a logical scalar.", fname),
        )),
    }
}

/// Dims from `f(n)`, `f(m, n, ...)`, `f([m n])`; trailing class name and
/// `'like', x` are returned separately.
pub fn dims_and_class(args: &[Value], fname: &str) -> R<(Vec<usize>, Option<String>)> {
    let mut class = None;
    let mut nums = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if let Some(s) = a.to_str() {
            if s.eq_ignore_ascii_case("like") {
                if let Some(x) = args.get(i + 1) {
                    class = Some(x.class_name());
                }
                i += 2;
                continue;
            }
            class = Some(s);
            i += 1;
            continue;
        }
        nums.push(a);
        i += 1;
    }
    let to_dim = |x: f64| -> R<usize> {
        if x.is_nan() {
            return Err(Error::new(
                "MATLAB:NonIntegerInput",
                format!("{}: size inputs must be integers (got NaN).", fname),
            ));
        }
        if x.fract() != 0.0 {
            return Err(Error::new(
                "MATLAB:NonIntegerInput",
                format!("{}: size inputs must be integers.", fname),
            ));
        }
        Ok(if x < 0.0 { 0 } else { x as usize })
    };
    let dims = match nums.len() {
        0 => vec![1, 1],
        1 => {
            let v = nums[0].to_f64_vec().ok_or_else(|| {
                Error::new(
                    "MATLAB:invalidType",
                    format!("{}: size must be numeric.", fname),
                )
            })?;
            match v.len() {
                0 => vec![0, 0],
                1 => {
                    let n = to_dim(v[0])?;
                    vec![n, n]
                }
                _ => v.iter().map(|x| to_dim(*x)).collect::<R<Vec<_>>>()?,
            }
        }
        _ => {
            let mut d = Vec::new();
            for v in nums {
                let x = v.scalar_f64().ok_or_else(|| {
                    Error::new(
                        "MATLAB:invalidType",
                        format!("{}: size arguments must be scalars.", fname),
                    )
                })?;
                d.push(to_dim(x)?);
            }
            d
        }
    };
    Ok((indy_value::norm_dims(dims), class))
}

/// Build a numeric/logical/char array of a class name.
pub fn filled(dims: Vec<usize>, x: f64, class: Option<&str>) -> R<Value> {
    let n = indy_value::numel_of(&dims);
    match class {
        None | Some("double") => Ok(Value::Num(Num::real(dims, vec![x; n]))),
        Some("logical") => Ok(Value::Bool(Array::new(dims, vec![x != 0.0; n]))),
        Some("char") => Ok(Value::Char(Array::new(
            dims,
            vec![char::from_u32(x as u32).unwrap_or('\0'); n],
        ))),
        Some(c) => match NumClass::from_name(c) {
            Some(cl) => Ok(Value::Num(Num::real(dims, vec![x; n]).with_class(cl))),
            None => Err(Error::new(
                "MATLAB:invalidClass",
                format!("Unsupported class name '{}'.", c),
            )),
        },
    }
}

// ---------------------------------------------------------------------------
// Random numbers: MT19937 (MATLAB's default 'twister' generator).
// ---------------------------------------------------------------------------

pub struct Rng {
    mt: Vec<u32>,
    idx: usize,
    pub seed: u32,
    spare: Option<f64>,
}

impl Rng {
    pub fn new(seed: u32) -> Rng {
        let mut r = Rng {
            mt: vec![0; 624],
            idx: 624,
            seed,
            spare: None,
        };
        r.reseed(seed);
        r
    }
    /// MATLAB `rng(s)`: seed 0 means the default state (5489).
    pub fn reseed(&mut self, seed: u32) {
        self.seed = seed;
        let s = if seed == 0 { 5489 } else { seed };
        self.mt[0] = s;
        for i in 1..624 {
            let prev = self.mt[i - 1];
            self.mt[i] = 1812433253u32
                .wrapping_mul(prev ^ (prev >> 30))
                .wrapping_add(i as u32);
        }
        self.idx = 624;
        self.spare = None;
    }
    fn next_u32(&mut self) -> u32 {
        if self.idx >= 624 {
            for i in 0..624 {
                let y = (self.mt[i] & 0x8000_0000) | (self.mt[(i + 1) % 624] & 0x7fff_ffff);
                let mut v = self.mt[(i + 397) % 624] ^ (y >> 1);
                if y & 1 != 0 {
                    v ^= 0x9908_b0df;
                }
                self.mt[i] = v;
            }
            self.idx = 0;
        }
        let mut y = self.mt[self.idx];
        self.idx += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^= y >> 18;
        y
    }
    /// Uniform (0,1) double with 53 random bits.
    pub fn uniform(&mut self) -> f64 {
        let a = (self.next_u32() >> 5) as f64;
        let b = (self.next_u32() >> 6) as f64;
        (a * 67108864.0 + b) / 9007199254740992.0
    }
    /// Standard normal (polar Box–Muller; not MATLAB's exact sequence).
    pub fn normal(&mut self) -> f64 {
        if let Some(s) = self.spare.take() {
            return s;
        }
        loop {
            let u = 2.0 * self.uniform() - 1.0;
            let v = 2.0 * self.uniform() - 1.0;
            let s = u * u + v * v;
            if s > 0.0 && s < 1.0 {
                let m = (-2.0 * s.ln() / s).sqrt();
                self.spare = Some(v * m);
                return u * m;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// isequal
// ---------------------------------------------------------------------------

/// Same pattern and elements (canonical storage has no explicit zeros).
fn sparse_equal(
    x: &indy_value::sparse::Sparse,
    y: &indy_value::sparse::Sparse,
    nan_eq: bool,
) -> bool {
    let eq = |p: f64, q: f64| p == q || (nan_eq && p.is_nan() && q.is_nan());
    x.colptr == y.colptr
        && x.rowidx == y.rowidx
        && (0..x.re.len()).all(|k| eq(x.re[k], y.re[k]) && eq(x.entry_im(k), y.entry_im(k)))
}

pub fn isequal_values(a: &Value, b: &Value, nan_eq: bool) -> bool {
    if (matches!(a, Value::Str(_)) || matches!(b, Value::Str(_))) && a.is_text() && b.is_text() {
        let x = indy_value::to_string_array(a);
        let y = indy_value::to_string_array(b);
        return x.dims == y.dims && x.data == y.data && !x.data.iter().any(|e| e.is_none());
    }
    if matches!(a, Value::Str(_)) != matches!(b, Value::Str(_)) {
        return false;
    }
    if a.dims_ref() != b.dims_ref() {
        return false;
    }
    match (a, b) {
        (Value::Sparse(x), Value::Sparse(y)) => sparse_equal(x, y, nan_eq),
        (Value::Cell(x), Value::Cell(y)) => x
            .data
            .iter()
            .zip(&y.data)
            .all(|(p, q)| isequal_values(p, q, nan_eq)),
        (Value::Struct(x), Value::Struct(y)) => {
            if x.fields.len() != y.fields.len() {
                return false;
            }
            for (fi, f) in x.fields.iter().enumerate() {
                let Some(gi) = y.field_index(f) else {
                    return false;
                };
                for (ex, ey) in x.elems.iter().zip(&y.elems) {
                    if !isequal_values(&ex[fi], &ey[gi], nan_eq) {
                        return false;
                    }
                }
            }
            true
        }
        (Value::Func(x), Value::Func(y)) => x == y,
        (Value::Object(_), Value::Object(_)) | (Value::HObject(_), Value::HObject(_)) => {
            handles::objects_equal(a, b).unwrap_or(false)
        }
        _ => {
            let (Some(x), Some(y)) = (a.to_num(), b.to_num()) else {
                return false;
            };
            if matches!(a, Value::Cell(_) | Value::Struct(_))
                || matches!(b, Value::Cell(_) | Value::Struct(_))
            {
                return false;
            }
            for i in 0..x.re.len() {
                let (p, q) = (x.re[i], y.re[i]);
                let (pi, qi) = (x.im_at(i), y.im_at(i));
                let eq = |u: f64, v: f64| u == v || (nan_eq && u.is_nan() && v.is_nan());
                if !eq(p, q) || !eq(pi, qi) {
                    return false;
                }
            }
            true
        }
    }
}
