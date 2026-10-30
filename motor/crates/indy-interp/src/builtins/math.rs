//! Numeric builtins: constructors, element-wise functions, reductions
//! (with MATLAB's empty-array rules), find/sort/unique, reshaping.

use indy_syntax::ast::{BinOp, UnOp};
use indy_value::index::{Sub, index_value};
use indy_value::ops::{self, C, c_abs, c_exp, c_log, c_mul, c_sqrt};
use indy_value::{Array, Error, Num, NumClass, Value, concat, norm_dims, numel_of};

use super::{arg_f64, arg_num, arg_str, arg_usize, dims_and_class, err, filled, need, one, reg};
use crate::{Interp, R};

pub fn register(it: &mut Interp) {
    // constructors and constants
    reg(it, "zeros", |_, a, _| ctor(&a, 0.0, "zeros"));
    reg(it, "ones", |_, a, _| ctor(&a, 1.0, "ones"));
    reg(it, "NaN", |_, a, _| ctor(&a, f64::NAN, "NaN"));
    reg(it, "nan", |_, a, _| ctor(&a, f64::NAN, "nan"));
    reg(it, "NA", |_, a, _| ctor(&a, f64::NAN, "NA"));
    reg(it, "Inf", |_, a, _| ctor(&a, f64::INFINITY, "Inf"));
    reg(it, "inf", |_, a, _| ctor(&a, f64::INFINITY, "inf"));
    reg(it, "pi", |_, a, _| ctor(&a, std::f64::consts::PI, "pi"));
    reg(it, "e", |_, a, _| ctor(&a, std::f64::consts::E, "e"));
    reg(it, "true", |_, a, _| {
        let (d, _) = dims_and_class(&a, "true")?;
        one(filled(d, 1.0, Some("logical"))?)
    });
    reg(it, "false", |_, a, _| {
        let (d, _) = dims_and_class(&a, "false")?;
        one(filled(d, 0.0, Some("logical"))?)
    });
    reg(it, "eps", eps);
    reg(it, "realmax", |_, a, _| ctor(&a, f64::MAX, "realmax"));
    reg(it, "realmin", |_, a, _| {
        ctor(&a, f64::MIN_POSITIVE, "realmin")
    });
    reg(it, "flintmax", |_, _, _| {
        one(Value::num(9007199254740992.0))
    });
    reg(it, "intmax", |_, a, _| int_limit(&a, true));
    reg(it, "intmin", |_, a, _| int_limit(&a, false));
    reg(it, "i", |_, _, _| one(Value::complex(0.0, 1.0)));
    reg(it, "j", |_, _, _| one(Value::complex(0.0, 1.0)));
    reg(it, "I", |_, _, _| one(Value::complex(0.0, 1.0)));
    reg(it, "J", |_, _, _| one(Value::complex(0.0, 1.0)));
    reg(it, "eye", eye);
    reg(it, "rand", |it, a, _| random(it, a, 0));
    reg(it, "randn", |it, a, _| random(it, a, 1));
    reg(it, "randi", randi);
    reg(it, "randperm", randperm);
    reg(it, "rng", rng);
    reg(it, "magic", magic);
    reg(it, "linspace", linspace);
    reg(it, "logspace", logspace);
    reg(it, "colon", |_, a, _| {
        need(&a, 2, "colon")?;
        let (s, st, e) = if a.len() == 3 {
            (
                arg_f64(&a[0], "colon")?,
                arg_f64(&a[1], "colon")?,
                arg_f64(&a[2], "colon")?,
            )
        } else {
            (arg_f64(&a[0], "colon")?, 1.0, arg_f64(&a[1], "colon")?)
        };
        one(Value::row(ops::range(s, st, e)?))
    });
    reg(it, "meshgrid", meshgrid);
    reg(it, "ndgrid", ndgrid);
    // conversions
    for (name, class) in [
        ("double", NumClass::Double),
        ("single", NumClass::Single),
        ("int8", NumClass::Int8),
        ("int16", NumClass::Int16),
        ("int32", NumClass::Int32),
        ("int64", NumClass::Int64),
        ("uint8", NumClass::UInt8),
        ("uint16", NumClass::UInt16),
        ("uint32", NumClass::UInt32),
        ("uint64", NumClass::UInt64),
    ] {
        let _ = class;
        reg(it, name, convert_dispatch(name));
    }
    reg(it, "logical", logical);
    reg(it, "cast", |_, a, _| {
        need(&a, 2, "cast")?;
        let c = arg_str(&a[1], "cast")?;
        one(to_class(&a[0], &c)?)
    });
    // element-wise math
    reg(it, "abs", |_, a, _| {
        unary_c(&a, "abs", |x| x.abs(), Some(|z| (c_abs(z), 0.0)), false)
    });
    reg(it, "sqrt", |_, a, _| {
        unary_c(&a, "sqrt", f64::sqrt, Some(c_sqrt), true)
    });
    reg(it, "realsqrt", |_, a, _| {
        unary_c(&a, "realsqrt", f64::sqrt, None, false)
    });
    reg(it, "exp", |_, a, _| {
        unary_c(&a, "exp", f64::exp, Some(c_exp), false)
    });
    reg(it, "log", |_, a, _| {
        unary_c(&a, "log", f64::ln, Some(c_log), true)
    });
    reg(it, "reallog", |_, a, _| {
        unary_c(&a, "reallog", f64::ln, None, false)
    });
    reg(it, "log2", |_, a, nargout| {
        if nargout >= 2 {
            return log2_frexp(&a);
        }
        unary_c(
            &a,
            "log2",
            f64::log2,
            Some(|z| {
                let l = c_log(z);
                (l.0 / std::f64::consts::LN_2, l.1 / std::f64::consts::LN_2)
            }),
            true,
        )
    });
    reg(it, "log10", |_, a, _| {
        unary_c(
            &a,
            "log10",
            f64::log10,
            Some(|z| {
                let l = c_log(z);
                (l.0 / std::f64::consts::LN_10, l.1 / std::f64::consts::LN_10)
            }),
            true,
        )
    });
    reg(it, "log1p", |_, a, _| {
        unary_c(&a, "log1p", f64::ln_1p, None, false)
    });
    reg(it, "expm1", |_, a, _| {
        unary_c(&a, "expm1", f64::exp_m1, None, false)
    });
    reg(it, "sin", |_, a, _| {
        unary_c(
            &a,
            "sin",
            f64::sin,
            Some(|z: C| (z.0.sin() * z.1.cosh(), z.0.cos() * z.1.sinh())),
            false,
        )
    });
    reg(it, "cos", |_, a, _| {
        unary_c(
            &a,
            "cos",
            f64::cos,
            Some(|z: C| (z.0.cos() * z.1.cosh(), -z.0.sin() * z.1.sinh())),
            false,
        )
    });
    reg(it, "tan", |_, a, _| {
        unary_c(&a, "tan", f64::tan, None, false)
    });
    reg(it, "asin", |_, a, _| {
        unary_c(&a, "asin", f64::asin, None, false)
    });
    reg(it, "acos", |_, a, _| {
        unary_c(&a, "acos", f64::acos, None, false)
    });
    reg(it, "atan", |_, a, _| {
        unary_c(&a, "atan", f64::atan, None, false)
    });
    reg(it, "sinh", |_, a, _| {
        unary_c(&a, "sinh", f64::sinh, None, false)
    });
    reg(it, "cosh", |_, a, _| {
        unary_c(&a, "cosh", f64::cosh, None, false)
    });
    reg(it, "tanh", |_, a, _| {
        unary_c(&a, "tanh", f64::tanh, None, false)
    });
    reg(it, "asinh", |_, a, _| {
        unary_c(&a, "asinh", f64::asinh, None, false)
    });
    reg(it, "acosh", |_, a, _| {
        unary_c(&a, "acosh", f64::acosh, None, false)
    });
    reg(it, "atanh", |_, a, _| {
        unary_c(&a, "atanh", f64::atanh, None, false)
    });
    reg(it, "sec", |_, a, _| {
        unary_c(&a, "sec", |x| 1.0 / x.cos(), None, false)
    });
    reg(it, "csc", |_, a, _| {
        unary_c(&a, "csc", |x| 1.0 / x.sin(), None, false)
    });
    reg(it, "cot", |_, a, _| {
        unary_c(&a, "cot", |x| 1.0 / x.tan(), None, false)
    });
    reg(it, "sind", |_, a, _| unary_c(&a, "sind", sind, None, false));
    reg(it, "cosd", |_, a, _| {
        unary_c(&a, "cosd", |x| sind(x + 90.0), None, false)
    });
    reg(it, "tand", |_, a, _| {
        unary_c(&a, "tand", |x| sind(x) / sind(x + 90.0), None, false)
    });
    reg(it, "deg2rad", |_, a, _| {
        unary_c(
            &a,
            "deg2rad",
            |x| x * std::f64::consts::PI / 180.0,
            None,
            false,
        )
    });
    reg(it, "rad2deg", |_, a, _| {
        unary_c(
            &a,
            "rad2deg",
            |x| x * 180.0 / std::f64::consts::PI,
            None,
            false,
        )
    });
    reg(it, "floor", |_, a, _| {
        unary_c(
            &a,
            "floor",
            f64::floor,
            Some(|z: C| (z.0.floor(), z.1.floor())),
            false,
        )
    });
    reg(it, "ceil", |_, a, _| {
        unary_c(
            &a,
            "ceil",
            f64::ceil,
            Some(|z: C| (z.0.ceil(), z.1.ceil())),
            false,
        )
    });
    reg(it, "fix", |_, a, _| {
        unary_c(
            &a,
            "fix",
            f64::trunc,
            Some(|z: C| (z.0.trunc(), z.1.trunc())),
            false,
        )
    });
    reg(it, "round", round);
    reg(it, "sign", |_, a, _| {
        unary_c(
            &a,
            "sign",
            |x| {
                if x > 0.0 {
                    1.0
                } else if x < 0.0 {
                    -1.0
                } else {
                    x
                }
            },
            None,
            false,
        )
    });
    reg(it, "gamma", |_, a, _| {
        unary_c(&a, "gamma", gamma, None, false)
    });
    reg(it, "gammaln", |_, a, _| {
        unary_c(&a, "gammaln", lgamma, None, false)
    });
    reg(it, "factorial", |_, a, _| {
        unary_c(
            &a,
            "factorial",
            |x| {
                if x < 0.0 || x.fract() != 0.0 {
                    f64::NAN
                } else {
                    gamma(x + 1.0).round()
                }
            },
            None,
            false,
        )
    });
    reg(it, "erf", |_, a, _| unary_c(&a, "erf", erf, None, false));
    reg(it, "erfc", |_, a, _| {
        unary_c(&a, "erfc", |x| 1.0 - erf(x), None, false)
    });
    reg(it, "nthroot", nthroot);
    reg(it, "real", |_, a, _| part(&a, 0));
    reg(it, "imag", |_, a, _| part(&a, 1));
    reg(it, "conj", |_, a, _| part(&a, 2));
    reg(it, "angle", |_, a, _| part(&a, 3));
    reg(it, "arg", |_, a, _| part(&a, 3));
    reg(it, "complex", complex);
    reg(it, "isnan", |_, a, _| {
        test(&a, "isnan", |x, y| x.is_nan() || y.is_nan())
    });
    reg(it, "isinf", |_, a, _| {
        test(&a, "isinf", |x, y| x.is_infinite() || y.is_infinite())
    });
    reg(it, "isfinite", |_, a, _| {
        test(&a, "isfinite", |x, y| x.is_finite() && y.is_finite())
    });
    reg(it, "isna", |_, a, _| test(&a, "isna", |_, _| false));
    // binary element-wise
    reg(it, "mod", |_, a, _| {
        binary_f(&a, "mod", |x, y| {
            if y == 0.0 {
                x
            } else if !y.is_finite() && x.is_finite() {
                if x == 0.0 || x.signum() == y.signum() {
                    x
                } else {
                    y
                }
            } else {
                let r = x - (x / y).floor() * y;
                if r != 0.0 && (r - y).abs() < y.abs() * 1e-15 {
                    0.0
                } else {
                    r
                }
            }
        })
    });
    reg(it, "rem", |_, a, _| {
        binary_f(&a, "rem", |x, y| {
            if y == 0.0 {
                f64::NAN
            } else if !y.is_finite() && x.is_finite() {
                x
            } else {
                x - (x / y).trunc() * y
            }
        })
    });
    reg(it, "atan2", |_, a, _| binary_f(&a, "atan2", f64::atan2));
    reg(it, "hypot", |_, a, _| binary_f(&a, "hypot", f64::hypot));
    reg(it, "idivide", idivide);
    reg(it, "gcd", |_, a, nargout| {
        if nargout <= 1 {
            return binary_f(&a, "gcd", gcd);
        }
        need(&a, 2, "gcd")?;
        let x = arg_num(&a[0], "gcd")?;
        let y = arg_num(&a[1], "gcd")?;
        let (d, v) = ops::zip_bcast(&x.dims, &x.re, &y.dims, &y.re, |p, q| {
            // Extended Euclid: g = s*p + t*q.
            let (mut r0, mut r1) = (p, q);
            let (mut s0, mut s1) = (1.0, 0.0);
            let (mut t0, mut t1) = (0.0, 1.0);
            while r1 != 0.0 {
                let qq = (r0 / r1).floor();
                (r0, r1) = (r1, r0 - qq * r1);
                (s0, s1) = (s1, s0 - qq * s1);
                (t0, t1) = (t1, t0 - qq * t1);
            }
            if r0 < 0.0 {
                (-r0, -s0, -t0)
            } else {
                (r0, s0, t0)
            }
        })?;
        let g: Vec<f64> = v.iter().map(|t| t.0).collect();
        let s: Vec<f64> = v.iter().map(|t| t.1).collect();
        let t: Vec<f64> = v.iter().map(|t| t.2).collect();
        Ok(vec![
            Value::Num(Num::real(d.clone(), g)),
            Value::Num(Num::real(d.clone(), s)),
            Value::Num(Num::real(d, t)),
        ])
    });
    reg(it, "lcm", |_, a, _| {
        binary_f(&a, "lcm", |x, y| {
            if x == 0.0 || y == 0.0 {
                0.0
            } else {
                (x * y).abs() / gcd(x, y)
            }
        })
    });
    reg(it, "bitand", |_, a, _| {
        binary_f(&a, "bitand", |x, y| ((x as u64) & (y as u64)) as f64)
    });
    reg(it, "bitor", |_, a, _| {
        binary_f(&a, "bitor", |x, y| ((x as u64) | (y as u64)) as f64)
    });
    reg(it, "bitxor", |_, a, _| {
        binary_f(&a, "bitxor", |x, y| ((x as u64) ^ (y as u64)) as f64)
    });
    reg(it, "bitshift", |_, a, _| {
        binary_f(&a, "bitshift", |x, y| {
            if y >= 0.0 {
                ((x as u64) << (y as u32)) as f64
            } else {
                ((x as u64) >> ((-y) as u32)) as f64
            }
        })
    });
    // operator functions
    for (name, op) in [
        ("plus", BinOp::Add),
        ("minus", BinOp::Sub),
        ("times", BinOp::EMul),
        ("rdivide", BinOp::EDiv),
        ("ldivide", BinOp::ELeftDiv),
        ("mtimes", BinOp::Mul),
        ("mrdivide", BinOp::Div),
        ("mldivide", BinOp::LeftDiv),
        ("power", BinOp::EPow),
        ("mpower", BinOp::Pow),
        ("eq", BinOp::Eq),
        ("ne", BinOp::Ne),
        ("lt", BinOp::Lt),
        ("le", BinOp::Le),
        ("gt", BinOp::Gt),
        ("ge", BinOp::Ge),
        ("and", BinOp::And),
        ("or", BinOp::Or),
    ] {
        reg(it, name, op_dispatch(op));
        let _ = op;
    }
    reg(it, "xor", |_, a, _| {
        need(&a, 2, "xor")?;
        let x = ops::to_logical_data(&a[0])?;
        let y = ops::to_logical_data(&a[1])?;
        let (d, v) = ops::zip_bcast(a[0].dims_ref(), &x, a[1].dims_ref(), &y, |p, q| p != q)?;
        one(Value::Bool(Array::new(d, v)))
    });
    reg(it, "not", |_, a, _| {
        need(&a, 1, "not")?;
        one(ops::unary(UnOp::Not, &a[0])?)
    });
    reg(it, "uminus", |_, a, _| {
        need(&a, 1, "uminus")?;
        one(ops::unary(UnOp::Neg, &a[0])?)
    });
    reg(it, "uplus", |_, a, _| {
        need(&a, 1, "uplus")?;
        one(ops::unary(UnOp::Plus, &a[0])?)
    });
    reg(it, "transpose", |_, a, _| {
        need(&a, 1, "transpose")?;
        one(ops::transpose(&a[0], false)?)
    });
    reg(it, "ctranspose", |_, a, _| {
        need(&a, 1, "ctranspose")?;
        one(ops::transpose(&a[0], true)?)
    });
    // reductions
    reg(it, "sum", |_, a, _| reduce(&a, "sum", Red::Sum));
    reg(it, "prod", |_, a, _| reduce(&a, "prod", Red::Prod));
    reg(it, "mean", |_, a, _| reduce(&a, "mean", Red::Mean));
    reg(it, "any", |_, a, _| reduce(&a, "any", Red::Any));
    reg(it, "all", |_, a, _| reduce(&a, "all", Red::All));
    reg(it, "cumsum", |_, a, _| cumulative(&a, "cumsum", false));
    reg(it, "cumprod", |_, a, _| cumulative(&a, "cumprod", true));
    reg(it, "max", |_, a, n| minmax(&a, n, true));
    reg(it, "min", |_, a, n| minmax(&a, n, false));
    reg(it, "nnz", |_, a, _| {
        need(&a, 1, "nnz")?;
        let n = a[0]
            .to_f64_vec()
            .ok_or_else(|| err("MATLAB:nnz:invalid", "nnz: numeric input required."))?;
        one(Value::num(n.iter().filter(|x| **x != 0.0).count() as f64))
    });
    reg(it, "median", median);
    reg(it, "var", |_, a, _| var_std(&a, false));
    reg(it, "std", |_, a, _| var_std(&a, true));
    reg(it, "diff", diff);
    reg(it, "dot", dot);
    reg(it, "cross", cross);
    reg(it, "kron", kron);
    // search, sort, sets
    reg(it, "find", find);
    reg(it, "sort", sort);
    reg(it, "unique", unique);
    reg(it, "ismember", ismember);
    reg(it, "setdiff", |_, a, n| setop(&a, n, 0));
    reg(it, "intersect", |_, a, n| setop(&a, n, 1));
    reg(it, "union", |_, a, n| setop(&a, n, 2));
    // shape
    reg(it, "reshape", reshape);
    reg(it, "repmat", repmat);
    reg(it, "cat", |_, mut a, _| {
        need(&a, 1, "cat")?;
        let d = arg_usize(&a.remove(0), "cat")?;
        if d == 0 {
            return Err(err(
                "MATLAB:catenate:invalidDimension",
                "cat: dimension must be positive.",
            ));
        }
        one(concat::cat(a, d - 1)?)
    });
    reg(it, "horzcat", |_, a, _| one(concat::cat(a, 1)?));
    reg(it, "vertcat", |_, a, _| one(concat::cat(a, 0)?));
    reg(it, "fliplr", |_, a, _| flip(&a, Some(1)));
    reg(it, "flipud", |_, a, _| flip(&a, Some(0)));
    reg(it, "flip", |_, a, _| flip(&a, None));
    reg(it, "circshift", circshift);
    reg(it, "permute", permute);
    reg(it, "squeeze", |_, a, _| {
        need(&a, 1, "squeeze")?;
        let d = a[0].dims();
        if d.len() <= 2 {
            return one(a[0].clone());
        }
        let nd: Vec<usize> = d.iter().copied().filter(|x| *x != 1).collect();
        one(a[0].clone().reshaped(nd))
    });
    reg(it, "diag", diag);
    reg(it, "tril", |_, a, _| tri(&a, true));
    reg(it, "triu", |_, a, _| tri(&a, false));
    reg(it, "trace", |_, a, _| {
        need(&a, 1, "trace")?;
        let n = arg_num(&a[0], "trace")?;
        let k = n.dims[0].min(n.dims[1]);
        one(Value::num((0..k).map(|i| n.re[i + i * n.dims[0]]).sum()))
    });
    reg(it, "det", det);
    reg(it, "inv", inv);
    reg(it, "norm", norm);
    reg(it, "primes", primes);
    reg(it, "isprime", |_, a, _| {
        test(&a, "isprime", |x, _| is_prime(x))
    });
    reg(it, "nchoosek", nchoosek);
    reg(it, "cummax", |_, a, _| cumext(&a, true));
    reg(it, "cummin", |_, a, _| cumext(&a, false));
    reg(it, "sub2ind", sub2ind);
    reg(it, "ind2sub", ind2sub);
    reg(it, "accumarray", accumarray);
    reg(it, "histc", histc);
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn ctor(a: &[Value], x: f64, fname: &str) -> R<Vec<Value>> {
    let (d, class) = dims_and_class(a, fname)?;
    one(filled(d, x, class.as_deref())?)
}

fn sind(x: f64) -> f64 {
    let r = x.rem_euclid(360.0);
    if r == 0.0 || r == 180.0 {
        return 0.0 * x.signum();
    }
    if r == 90.0 {
        return 1.0;
    }
    if r == 270.0 {
        return -1.0;
    }
    (x * std::f64::consts::PI / 180.0).sin()
}

fn eps(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    if let Some(v) = a.first()
        && !v.is_char()
    {
        let n = arg_num(v, "eps")?;
        let out: Vec<f64> =
            n.re.iter()
                .map(|x| {
                    let x = x.abs();
                    if !x.is_finite() {
                        return f64::NAN;
                    }
                    if x < f64::MIN_POSITIVE {
                        return 5e-324;
                    }
                    let e = x.log2().floor();
                    2f64.powf(e - 52.0)
                })
                .collect();
        return one(Value::Num(Num::real(n.dims, out)));
    }
    ctor(&a, f64::EPSILON, "eps")
}

fn int_limit(a: &[Value], max: bool) -> R<Vec<Value>> {
    let name = match a.first() {
        Some(v) => match v.to_str() {
            Some(s) => s,
            None => v.class_name(),
        },
        None => "int32".into(),
    };
    let c = NumClass::from_name(&name)
        .filter(|c| c.is_int())
        .ok_or_else(|| {
            err(
                "MATLAB:intmax:invalidClassName",
                format!("Invalid class name '{}'.", name),
            )
        })?;
    let (lo, hi) = c.int_range();
    one(Value::Num(
        Num::scalar(if max { hi } else { lo }).with_class(c),
    ))
}

fn eye(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let (d, class) = dims_and_class(&a, "eye")?;
    let (r, c) = (d[0], d[1]);
    let mut v = vec![0.0; r * c];
    for i in 0..r.min(c) {
        v[i + i * r] = 1.0;
    }
    let n = Num::real(vec![r, c], v);
    match class.as_deref() {
        None | Some("double") => one(Value::Num(n)),
        Some(cl) => one(to_class(&Value::Num(n), cl)?),
    }
}

fn random(it: &mut Interp, a: Vec<Value>, kind: u8) -> R<Vec<Value>> {
    if let Some(s) = a.first().and_then(|v| v.to_str())
        && matches!(s.as_str(), "seed" | "state" | "twister")
    {
        if let Some(v) = a.get(1) {
            let s = arg_f64(v, "rand")?;
            it.rng.reseed(s as u32);
        }
        return Ok(vec![]);
    }
    let (d, _) = dims_and_class(&a, "rand")?;
    let n = numel_of(&d);
    let mut v = Vec::with_capacity(n);
    for _ in 0..n {
        v.push(if kind == 0 {
            it.rng.uniform()
        } else {
            it.rng.normal()
        });
    }
    one(Value::Num(Num::real(d, v)))
}

fn randi(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "randi")?;
    let r = arg_num(&a[0], "randi")?;
    let (lo, hi) = if r.re.len() >= 2 {
        (r.re[0], r.re[1])
    } else {
        (1.0, r.re[0])
    };
    if hi < lo || lo.fract() != 0.0 || hi.fract() != 0.0 {
        return Err(err(
            "MATLAB:randi:invalidLimits",
            "randi: limits must be integers with imin <= imax.",
        ));
    }
    let (d, class) = dims_and_class(&a[1..], "randi")?;
    let n = numel_of(&d);
    let span = hi - lo + 1.0;
    let v: Vec<f64> = (0..n)
        .map(|_| lo + (it.rng.uniform() * span).floor())
        .collect();
    let num = Num::real(d, v);
    match class.as_deref() {
        None | Some("double") => one(Value::Num(num)),
        Some(c) => one(to_class(&Value::Num(num), c)?),
    }
}

fn randperm(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "randperm")?;
    let n = arg_usize(&a[0], "randperm")?;
    let k = match a.get(1) {
        Some(v) => arg_usize(v, "randperm")?,
        None => n,
    };
    let mut keys: Vec<(f64, usize)> = (0..n).map(|i| (it.rng.uniform(), i)).collect();
    keys.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
    one(Value::row(
        keys.iter().take(k).map(|(_, i)| *i as f64 + 1.0).collect(),
    ))
}

fn rng(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    let prev = {
        let mut s = indy_value::StructArray::new_scalar();
        s.set(0, "Type", Value::str("twister"));
        s.set(0, "Seed", Value::num(it.rng.seed as f64));
        s.set(0, "State", Value::empty());
        Value::Struct(s)
    };
    if let Some(v) = a.first() {
        match v {
            Value::Struct(s) => {
                let seed = s.get(0, "Seed").and_then(|x| x.scalar_f64()).unwrap_or(0.0);
                it.rng.reseed(seed as u32);
            }
            _ => {
                if let Some(s) = v.to_str() {
                    if s == "default" {
                        it.rng.reseed(0);
                    } else if s == "shuffle" {
                        let t = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_nanos() as u32)
                            .unwrap_or(1);
                        it.rng.reseed(t);
                    }
                } else {
                    let s = arg_f64(v, "rng")?;
                    it.rng.reseed(s as u32);
                }
            }
        }
    }
    if nargout > 0 || a.is_empty() {
        return one(prev);
    }
    Ok(vec![])
}

fn magic(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "magic")?;
    let n = arg_usize(&a[0], "magic")?;
    let mut m = vec![0.0; n * n];
    let at = |i: usize, j: usize| i + j * n;
    if n == 0 {
        return one(Value::empty());
    }
    if n == 2 {
        return one(Value::matrix(vec![2, 2], vec![4.0, 3.0, 1.0, 2.0]));
    }
    if n % 2 == 1 {
        // Siamese method as MATLAB lays it out.
        let mut i = 0usize;
        let mut j = n / 2;
        for k in 1..=n * n {
            m[at(i, j)] = k as f64;
            let ni = (i + n - 1) % n;
            let nj = (j + 1) % n;
            if m[at(ni, nj)] != 0.0 {
                i = (i + 1) % n;
            } else {
                i = ni;
                j = nj;
            }
        }
    } else if n % 4 == 0 {
        for i in 0..n {
            for j in 0..n {
                let v = (i * n + j + 1) as f64;
                let keep = (((i + 1) % 4) / 2) == (((j + 1) % 4) / 2);
                m[at(i, j)] = if keep { (n * n + 1) as f64 - v } else { v };
            }
        }
    } else {
        // LUX method for singly even order.
        let h = n / 2;
        let sub = {
            let mut s = vec![0.0; h * h];
            let mut i = 0usize;
            let mut j = h / 2;
            for k in 1..=h * h {
                s[i + j * h] = k as f64;
                let ni = (i + h - 1) % h;
                let nj = (j + 1) % h;
                if s[ni + nj * h] != 0.0 {
                    i = (i + 1) % h;
                } else {
                    i = ni;
                    j = nj;
                }
            }
            s
        };
        let add = (h * h) as f64;
        for i in 0..h {
            for j in 0..h {
                let v = sub[i + j * h];
                m[at(i, j)] = v;
                m[at(i + h, j + h)] = v + add;
                m[at(i, j + h)] = v + 2.0 * add;
                m[at(i + h, j)] = v + 3.0 * add;
            }
        }
        let k = (n - 2) / 4;
        for i in 0..h {
            let cols: Vec<usize> = if i == h / 2 {
                (1..=k).collect()
            } else {
                (0..k).collect()
            };
            for &j in &cols {
                m.swap(at(i, j), at(i + h, j));
            }
            for j in (n - k + 1)..n {
                m.swap(at(i, j), at(i + h, j));
            }
        }
    }
    one(Value::matrix(vec![n, n], m))
}

fn linspace(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "linspace")?;
    let x = arg_f64(&a[0], "linspace")?;
    let y = arg_f64(&a[1], "linspace")?;
    let n = match a.get(2) {
        Some(v) => arg_f64(v, "linspace")?.floor().max(1.0) as usize,
        None => 100,
    };
    if n == 1 {
        return one(Value::num(y));
    }
    let v: Vec<f64> = (0..n)
        .map(|k| {
            if k == n - 1 {
                y
            } else {
                x + (y - x) * (k as f64) / ((n - 1) as f64)
            }
        })
        .collect();
    one(Value::row(v))
}

fn logspace(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "logspace")?;
    let x = arg_f64(&a[0], "logspace")?;
    let y = arg_f64(&a[1], "logspace")?;
    let n = match a.get(2) {
        Some(v) => arg_usize(v, "logspace")?,
        None => 50,
    };
    let v: Vec<f64> = (0..n)
        .map(|k| {
            10f64.powf(if n == 1 {
                y
            } else {
                x + (y - x) * k as f64 / (n - 1) as f64
            })
        })
        .collect();
    one(Value::row(v))
}

fn meshgrid(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "meshgrid")?;
    let x = arg_num(&a[0], "meshgrid")?.re;
    let y = match a.get(1) {
        Some(v) => arg_num(v, "meshgrid")?.re,
        None => x.clone(),
    };
    let (r, c) = (y.len(), x.len());
    let mut xx = vec![0.0; r * c];
    let mut yy = vec![0.0; r * c];
    for j in 0..c {
        for i in 0..r {
            xx[i + j * r] = x[j];
            yy[i + j * r] = y[i];
        }
    }
    let mut out = vec![Value::matrix(vec![r, c], xx)];
    if nargout > 1 {
        out.push(Value::matrix(vec![r, c], yy));
    }
    Ok(out)
}

fn ndgrid(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "ndgrid")?;
    let b = if a.len() == 1 {
        vec![a[0].clone(), a[0].clone()]
    } else {
        a.clone()
    };
    let mut out = meshgrid(it, vec![b[1].clone(), b[0].clone()], 2)?;
    out.swap(0, 1);
    out.truncate(nargout.max(1));
    Ok(out)
}

fn convert_dispatch(name: &'static str) -> crate::BuiltinFn {
    match name {
        "double" => |_, a, _| conv1(&a, "double"),
        "single" => |_, a, _| conv1(&a, "single"),
        "int8" => |_, a, _| conv1(&a, "int8"),
        "int16" => |_, a, _| conv1(&a, "int16"),
        "int32" => |_, a, _| conv1(&a, "int32"),
        "int64" => |_, a, _| conv1(&a, "int64"),
        "uint8" => |_, a, _| conv1(&a, "uint8"),
        "uint16" => |_, a, _| conv1(&a, "uint16"),
        "uint32" => |_, a, _| conv1(&a, "uint32"),
        _ => |_, a, _| conv1(&a, "uint64"),
    }
}

fn conv1(a: &[Value], class: &str) -> R<Vec<Value>> {
    if a.is_empty() {
        return one(to_class(&Value::num(0.0), class)?);
    }
    one(to_class(&a[0], class)?)
}

pub fn to_class(v: &Value, class: &str) -> R<Value> {
    if let Some(x) =
        crate::classes::get_prop(v, "__value__").filter(|_| crate::classes::is_class_obj(v))
    {
        return to_class(&x, class);
    }
    if class == "logical" {
        return logical_of(v);
    }
    if class == "char" {
        let n = arg_num(v, "char")?;
        return Ok(Value::Char(Array::new(
            n.dims.clone(),
            n.re.iter()
                .map(|x| char::from_u32(*x as u32).unwrap_or('\0'))
                .collect(),
        )));
    }
    let c = NumClass::from_name(class).ok_or_else(|| {
        err(
            "MATLAB:cast:UnsupportedClass",
            format!("Unsupported class '{}'.", class),
        )
    })?;
    match v {
        Value::Num(_) | Value::Bool(_) | Value::Char(_) => {
            let mut n = v.to_num().unwrap();
            if c.is_int() && n.im.is_some() {
                n.im = None;
            }
            Ok(Value::Num(n.with_class(c)))
        }
        Value::Cell(_) if class == "double" => Err(err(
            "MATLAB:invalidConversion",
            "Conversion to double from cell is not possible.",
        )),
        other => Err(err(
            "MATLAB:invalidConversion",
            format!(
                "Conversion to {} from {} is not possible.",
                class,
                other.class_name()
            ),
        )),
    }
}

fn logical_of(v: &Value) -> R<Value> {
    match v {
        Value::Bool(_) => Ok(v.clone()),
        Value::Num(n) => {
            if n.re.iter().any(|x| x.is_nan()) {
                return Err(err(
                    "MATLAB:nologicalnan",
                    "NaN's cannot be converted to logicals.",
                ));
            }
            Ok(Value::Bool(Array::new(
                n.dims.clone(),
                (0..n.re.len())
                    .map(|i| n.re[i] != 0.0 || n.im_at(i) != 0.0)
                    .collect(),
            )))
        }
        Value::Char(a) => Ok(Value::Bool(Array::new(
            a.dims.clone(),
            a.data.iter().map(|c| *c != '\0').collect(),
        ))),
        other => Err(err(
            "MATLAB:invalidConversion",
            format!(
                "Conversion to logical from {} is not possible.",
                other.class_name()
            ),
        )),
    }
}

fn logical(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "logical")?;
    one(logical_of(&a[0])?)
}

/// Element-wise unary with optional complex version; `domain` makes
/// out-of-domain reals (negative for sqrt/log) go complex.
fn unary_c(
    a: &[Value],
    fname: &str,
    f: fn(f64) -> f64,
    cf: Option<fn(C) -> C>,
    domain: bool,
) -> R<Vec<Value>> {
    need(a, 1, fname)?;
    let n = arg_num(&a[0], fname)?;
    let class = n.class;
    let keep_class = class;
    if let Some(cf) = cf {
        let needs_c = n.im.is_some() || (domain && n.re.iter().any(|x| *x < 0.0));
        if needs_c {
            let out: Vec<C> = (0..n.re.len()).map(|i| cf((n.re[i], n.im_at(i)))).collect();
            let re = out.iter().map(|z| z.0).collect();
            let im = out.iter().map(|z| z.1).collect();
            return one(Value::Num(
                Num::complex(n.dims, re, im).with_class(keep_class),
            ));
        }
    } else if n.im.is_some() {
        return Err(err(
            "MATLAB:complexNotSupported",
            format!(
                "{}: complex input is not supported yet (phase 2 engine).",
                fname
            ),
        ));
    }
    let re: Vec<f64> = n.re.iter().map(|x| f(*x)).collect();
    one(Value::Num(Num::real(n.dims, re).with_class(keep_class)))
}

fn round(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "round")?;
    let n = arg_num(&a[0], "round")?;
    let digits = match a.get(1) {
        Some(v) => arg_f64(v, "round")?,
        None => 0.0,
    };
    let f = |x: f64| -> f64 {
        if digits == 0.0 {
            x.round()
        } else {
            let p = 10f64.powf(digits);
            let r = (x * p).round() / p;
            if r.is_finite() { r } else { x }
        }
    };
    let re: Vec<f64> = n.re.iter().map(|x| f(*x)).collect();
    let im = n.im.as_ref().map(|v| v.iter().map(|x| f(*x)).collect());
    let mut out = Num {
        class: n.class,
        dims: n.dims,
        re,
        im,
    };
    out.narrow();
    one(Value::Num(out))
}

fn log2_frexp(a: &[Value]) -> R<Vec<Value>> {
    need(a, 1, "log2")?;
    let n = arg_num(&a[0], "log2")?;
    let mut f = Vec::new();
    let mut e = Vec::new();
    for x in &n.re {
        if *x == 0.0 || !x.is_finite() {
            f.push(*x);
            e.push(0.0);
            continue;
        }
        let mut ex = x.abs().log2().floor() + 1.0;
        let mut m = x / 2f64.powf(ex);
        if m.abs() >= 1.0 {
            m /= 2.0;
            ex += 1.0;
        }
        if m.abs() < 0.5 {
            m *= 2.0;
            ex -= 1.0;
        }
        f.push(m);
        e.push(ex);
    }
    Ok(vec![
        Value::Num(Num::real(n.dims.clone(), f)),
        Value::Num(Num::real(n.dims, e)),
    ])
}

fn nthroot(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    binary_f(&a, "nthroot", |x, n| {
        if x < 0.0 && n.fract() == 0.0 && (n as i64) % 2 != 0 {
            -(-x).powf(1.0 / n)
        } else {
            let r = x.powf(1.0 / n);
            // Polish integer roots.
            let rr = r.round();
            if (rr.powf(n) - x).abs() == 0.0 { rr } else { r }
        }
    })
}

pub fn gamma(x: f64) -> f64 {
    if x.fract() == 0.0 && x <= 0.0 {
        return f64::INFINITY;
    }
    if x.fract() == 0.0 && x > 0.0 && x < 171.0 {
        let mut r = 1.0;
        let mut k = 2.0;
        while k < x {
            r *= k;
            k += 1.0;
        }
        return r;
    }
    if x < 0.5 {
        return std::f64::consts::PI / ((std::f64::consts::PI * x).sin() * gamma(1.0 - x));
    }
    lgamma(x).exp()
}

/// Lanczos approximation (g = 7, n = 9).
pub fn lgamma(x: f64) -> f64 {
    const G: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if x < 0.5 {
        return (std::f64::consts::PI / (std::f64::consts::PI * x).sin())
            .abs()
            .ln()
            - lgamma(1.0 - x);
    }
    let x = x - 1.0;
    let mut a = G[0];
    let t = x + 7.5;
    for (i, g) in G.iter().enumerate().skip(1) {
        a += g / (x + i as f64);
    }
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
}

/// erf with ~1e-15 accuracy (series / continued fraction).
pub fn erf(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    let ax = x.abs();
    if ax < 2.5 {
        // Maclaurin series.
        let mut sum = ax;
        let mut term = ax;
        let x2 = ax * ax;
        let mut n = 0.0;
        loop {
            n += 1.0;
            term *= -x2 / n;
            let add = term / (2.0 * n + 1.0);
            sum += add;
            if add.abs() < 1e-17 * sum.abs() {
                break;
            }
            if n > 200.0 {
                break;
            }
        }
        let r = 2.0 / std::f64::consts::PI.sqrt() * sum;
        return r.copysign(x);
    }
    // Continued fraction for erfc.
    let mut f = 0.0;
    for k in (1..60).rev() {
        f = (k as f64 / 2.0) / (ax + f);
    }
    let erfc = (-ax * ax).exp() / std::f64::consts::PI.sqrt() / (ax + f);
    (1.0 - erfc).copysign(x)
}

fn part(a: &[Value], which: u8) -> R<Vec<Value>> {
    need(a, 1, "real/imag")?;
    let n = arg_num(&a[0], "real")?;
    let out = match which {
        0 => Num::real(n.dims.clone(), n.re.clone()),
        1 => Num::real(
            n.dims.clone(),
            n.im.clone().unwrap_or_else(|| vec![0.0; n.re.len()]),
        ),
        2 => {
            let mut m = n.clone();
            if let Some(im) = &mut m.im {
                for v in im.iter_mut() {
                    *v = -*v;
                }
            }
            m
        }
        _ => Num::real(
            n.dims.clone(),
            (0..n.re.len()).map(|i| n.im_at(i).atan2(n.re[i])).collect(),
        ),
    };
    one(Value::Num(out.with_class(n.class)))
}

fn complex(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "complex")?;
    let x = arg_num(&a[0], "complex")?;
    let y = match a.get(1) {
        Some(v) => arg_num(v, "complex")?,
        None => Num::scalar(0.0),
    };
    let (d, v) = ops::zip_bcast(&x.dims, &x.re, &y.dims, &y.re, |p, q| (p, q))?;
    let re: Vec<f64> = v.iter().map(|p| p.0).collect();
    let im: Vec<f64> = v.iter().map(|p| p.1).collect();
    // complex() keeps an explicit zero imaginary part in MATLAB; we narrow.
    one(Value::Num(Num {
        class: NumClass::Double,
        dims: d,
        re,
        im: Some(im),
    }))
}

fn test(a: &[Value], fname: &str, f: fn(f64, f64) -> bool) -> R<Vec<Value>> {
    need(a, 1, fname)?;
    match &a[0] {
        Value::Num(n) => one(Value::Bool(Array::new(
            n.dims.clone(),
            (0..n.re.len()).map(|i| f(n.re[i], n.im_at(i))).collect(),
        ))),
        Value::Bool(_) | Value::Char(_) => {
            let n = a[0].to_num().unwrap();
            one(Value::Bool(Array::new(
                n.dims.clone(),
                n.re.iter().map(|x| f(*x, 0.0)).collect(),
            )))
        }
        other => Err(err(
            "MATLAB:invalidType",
            format!(
                "{}: input must be numeric, got {}.",
                fname,
                other.class_name()
            ),
        )),
    }
}

fn binary_f(a: &[Value], fname: &str, f: fn(f64, f64) -> f64) -> R<Vec<Value>> {
    need(a, 2, fname)?;
    let x = arg_num(&a[0], fname)?;
    let y = arg_num(&a[1], fname)?;
    let class = ops::result_class(x.class, y.class)?;
    let (d, v) = ops::zip_bcast(&x.dims, &x.re, &y.dims, &y.re, f)?;
    one(Value::Num(Num::real(d, v).with_class(class)))
}

fn gcd(a: f64, b: f64) -> f64 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b > 0.0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

fn idivide(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "idivide")?;
    let mode = match a.get(2) {
        Some(v) => arg_str(v, "idivide")?,
        None => "fix".into(),
    };
    let x = arg_num(&a[0], "idivide")?;
    let y = arg_num(&a[1], "idivide")?;
    let class = ops::result_class(x.class, y.class)?;
    let f: fn(f64) -> f64 = match mode.as_str() {
        "ceil" => f64::ceil,
        "floor" => f64::floor,
        "round" => f64::round,
        _ => f64::trunc,
    };
    let (d, v) = ops::zip_bcast(&x.dims, &x.re, &y.dims, &y.re, |p, q| f(p / q))?;
    one(Value::Num(Num::real(d, v).with_class(class)))
}

fn op_dispatch(op: BinOp) -> crate::BuiltinFn {
    macro_rules! f {
        ($op:expr) => {
            |_, a, _| {
                need(&a, 2, "operator")?;
                let mut acc = ops::binary($op, &a[0], &a[1])?;
                for b in &a[2..] {
                    acc = ops::binary($op, &acc, b)?;
                }
                one(acc)
            }
        };
    }
    match op {
        BinOp::Add => f!(BinOp::Add),
        BinOp::Sub => f!(BinOp::Sub),
        BinOp::EMul => f!(BinOp::EMul),
        BinOp::EDiv => f!(BinOp::EDiv),
        BinOp::ELeftDiv => f!(BinOp::ELeftDiv),
        BinOp::Mul => f!(BinOp::Mul),
        BinOp::Div => f!(BinOp::Div),
        BinOp::LeftDiv => f!(BinOp::LeftDiv),
        BinOp::EPow => f!(BinOp::EPow),
        BinOp::Pow => f!(BinOp::Pow),
        BinOp::Eq => f!(BinOp::Eq),
        BinOp::Ne => f!(BinOp::Ne),
        BinOp::Lt => f!(BinOp::Lt),
        BinOp::Le => f!(BinOp::Le),
        BinOp::Gt => f!(BinOp::Gt),
        BinOp::Ge => f!(BinOp::Ge),
        BinOp::And => f!(BinOp::And),
        _ => f!(BinOp::Or),
    }
}

// ---------------------------------------------------------------------------
// reductions
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Red {
    Sum,
    Prod,
    Mean,
    Any,
    All,
}

/// Default reduction dimension: first non-singleton (0-based).
pub fn default_dim(d: &[usize]) -> usize {
    d.iter().position(|x| *x != 1).unwrap_or(0)
}

/// Parse `(x, dim)` / `(x, 'all')` and flags like 'omitnan'.
fn red_args(a: &[Value], fname: &str) -> R<(Num, Option<usize>, bool, bool)> {
    need(a, 1, fname)?;
    let x = arg_num(&a[0], fname)?;
    let mut dim = None;
    let mut all = false;
    let mut omitnan = false;
    for v in &a[1..] {
        if let Some(s) = v.to_str() {
            match s.to_ascii_lowercase().as_str() {
                "all" => all = true,
                "omitnan" => omitnan = true,
                "includenan" | "double" | "native" | "default" => {}
                other => {
                    return Err(err(
                        "MATLAB:invalidOption",
                        format!("{}: unknown option '{}'.", fname, other),
                    ));
                }
            }
        } else if !v.is_empty() {
            let k = arg_f64(v, fname)?;
            if k < 1.0 || k.fract() != 0.0 {
                return Err(err(
                    "MATLAB:getdimarg:dimensionMustBePositiveInteger",
                    "Dimension argument must be a positive integer scalar within indexing range.",
                ));
            }
            dim = Some(k as usize - 1);
        }
    }
    Ok((x, dim, all, omitnan))
}

/// Iterate over the slices of `d` along `dim`: calls `f(start, stride, len)`.
pub fn slices(d: &[usize], dim: usize, mut f: impl FnMut(usize, usize, usize)) {
    let len = *d.get(dim).unwrap_or(&1);
    let stride: usize = d.iter().take(dim).product();
    let outer: usize = d.iter().skip(dim + 1).product();
    for o in 0..outer {
        for i in 0..stride {
            f(o * stride * len + i, stride, len);
        }
    }
}

pub fn reduced_dims(d: &[usize], dim: usize) -> Vec<usize> {
    let mut r = d.to_vec();
    while r.len() <= dim {
        r.push(1);
    }
    r[dim] = 1;
    norm_dims(r)
}

fn reduce(a: &[Value], fname: &str, kind: Red) -> R<Vec<Value>> {
    let (x, dim, all, omitnan) = red_args(a, fname)?;
    let is_logic = matches!(kind, Red::Any | Red::All);
    if is_logic && x.re.iter().any(|v| v.is_nan()) && false {
        unreachable!();
    }
    let (x, dim) = if all {
        let n = x.re.len();
        (
            Num {
                dims: vec![n, 1],
                ..x
            },
            0,
        )
    } else {
        let d = dim.unwrap_or_else(|| default_dim_empty(&x.dims));
        (x, d)
    };
    // MATLAB empty rule: a 0x0 input reduces to a scalar.
    let special_empty = dim_is_default_and_0x0(&x.dims, a);
    let out_dims = if special_empty {
        vec![1, 1]
    } else {
        reduced_dims(&x.dims, dim)
    };
    let complex = x.im.is_some() && matches!(kind, Red::Sum | Red::Mean | Red::Prod);
    let mut re = Vec::with_capacity(numel_of(&out_dims));
    let mut im = Vec::new();
    let mut bools = Vec::new();
    let mut run = |start: usize, stride: usize, len: usize| {
        let idx = (0..len).map(|k| start + k * stride);
        match kind {
            Red::Sum | Red::Mean => {
                let mut s = 0.0;
                let mut si = 0.0;
                let mut cnt = 0usize;
                for p in idx {
                    if omitnan && x.re[p].is_nan() {
                        continue;
                    }
                    s += x.re[p];
                    si += x.im_at(p);
                    cnt += 1;
                }
                if kind == Red::Mean {
                    let c = cnt as f64;
                    re.push(s / c);
                    im.push(si / c);
                } else {
                    re.push(s);
                    im.push(si);
                }
            }
            Red::Prod => {
                let mut z: C = (1.0, 0.0);
                for p in idx {
                    if omitnan && x.re[p].is_nan() {
                        continue;
                    }
                    z = c_mul(z, (x.re[p], x.im_at(p)));
                }
                re.push(z.0);
                im.push(z.1);
            }
            Red::Any => bools.push(
                idx.into_iter()
                    .any(|p| (x.re[p] != 0.0 && !x.re[p].is_nan()) || x.im_at(p) != 0.0),
            ),
            Red::All => bools.push(idx.into_iter().all(|p| x.re[p] != 0.0 || x.im_at(p) != 0.0)),
        }
    };
    if special_empty {
        run(0, 1, 0);
    } else {
        slices(&x.dims, dim, &mut run);
    }
    if is_logic {
        return one(Value::Bool(Array::new(out_dims, bools)));
    }
    let class = if x.class.is_int() || x.class == NumClass::Single {
        x.class
    } else {
        NumClass::Double
    };
    let n = if complex {
        Num::complex(out_dims, re, im)
    } else {
        Num::real(out_dims, re)
    };
    one(Value::Num(n.with_class(class)))
}

fn default_dim_empty(d: &[usize]) -> usize {
    default_dim(d)
}

fn dim_is_default_and_0x0(d: &[usize], a: &[Value]) -> bool {
    d == [0, 0] && a.len() == 1
}

fn cumulative(a: &[Value], fname: &str, prod: bool) -> R<Vec<Value>> {
    let reverse = a.iter().skip(1).any(|v| {
        v.to_str()
            .is_some_and(|s| s.eq_ignore_ascii_case("reverse"))
    });
    let rest: Vec<Value> = a
        .iter()
        .filter(|v| {
            !v.to_str().is_some_and(|s| {
                s.eq_ignore_ascii_case("reverse") || s.eq_ignore_ascii_case("forward")
            })
        })
        .cloned()
        .collect();
    let (x, dim, _, _) = red_args(&rest, fname)?;
    let dim = dim.unwrap_or_else(|| default_dim(&x.dims));
    let mut re = x.re.clone();
    slices(&x.dims, dim, |start, stride, len| {
        let mut acc = if prod { 1.0 } else { 0.0 };
        for kk in 0..len {
            let k = if reverse { len - 1 - kk } else { kk };
            let p = start + k * stride;
            acc = if prod { acc * x.re[p] } else { acc + x.re[p] };
            re[p] = acc;
        }
    });
    one(Value::Num(
        Num::real(x.dims.clone(), re).with_class(x.class),
    ))
}

fn cumext(a: &[Value], max: bool) -> R<Vec<Value>> {
    let (x, dim, _, _) = red_args(a, "cummax")?;
    let dim = dim.unwrap_or_else(|| default_dim(&x.dims));
    let mut re = x.re.clone();
    slices(&x.dims, dim, |start, stride, len| {
        let mut acc = f64::NAN;
        for k in 0..len {
            let p = start + k * stride;
            let v = x.re[p];
            if acc.is_nan() || (max && v > acc) || (!max && v < acc) {
                acc = v;
            }
            re[p] = acc;
        }
    });
    one(Value::Num(
        Num::real(x.dims.clone(), re).with_class(x.class),
    ))
}

fn minmax(a: &[Value], nargout: usize, max: bool) -> R<Vec<Value>> {
    let fname = if max { "max" } else { "min" };
    need(a, 1, fname)?;
    let better = |v: f64, best: f64| if max { v > best } else { v < best };
    if a.len() >= 2 && !a[1].is_empty() {
        // Element-wise two-argument form.
        let x = arg_num(&a[0], fname)?;
        let y = arg_num(&a[1], fname)?;
        let class = ops::result_class(x.class, y.class)?;
        let (d, v) = ops::zip_bcast(&x.dims, &x.re, &y.dims, &y.re, |p, q| {
            if p.is_nan() {
                q
            } else if q.is_nan() {
                p
            } else if better(q, p) {
                q
            } else {
                p
            }
        })?;
        return one(Value::Num(Num::real(d, v).with_class(class)));
    }
    let mut rest: Vec<Value> = vec![a[0].clone()];
    if a.len() >= 3 {
        rest.extend(a[2..].iter().cloned());
    }
    let include_nan = rest.iter().any(|v| {
        v.to_str()
            .is_some_and(|s| s.eq_ignore_ascii_case("includenan"))
    });
    let (x, dim, all, _) = red_args(&rest, fname)?;
    let is_bool = matches!(a[0], Value::Bool(_));
    let is_char = matches!(a[0], Value::Char(_));
    let (x, dim) = if all {
        let n = x.re.len();
        (
            Num {
                dims: vec![n, 1],
                ..x
            },
            0,
        )
    } else {
        let d = dim.unwrap_or_else(|| default_dim(&x.dims));
        (x, d)
    };
    if x.dims == [0, 0] {
        return Ok(vec![Value::empty(), Value::empty()]
            .into_iter()
            .take(nargout.max(1))
            .collect());
    }
    // A 0xN input has no elements to reduce: the result is 0xN, not 1xN.
    let out_dims = if x.re.is_empty() {
        x.dims.clone()
    } else {
        reduced_dims(&x.dims, dim)
    };
    let mags: Vec<f64> = if x.im.is_some() {
        (0..x.re.len())
            .map(|i| c_abs((x.re[i], x.im_at(i))))
            .collect()
    } else {
        x.re.clone()
    };
    let mut vals = Vec::new();
    let mut ims = Vec::new();
    let mut idx = Vec::new();
    slices(&x.dims, dim, |start, stride, len| {
        let mut best: Option<(usize, f64)> = None;
        for k in 0..len {
            let p = start + k * stride;
            let v = mags[p];
            if v.is_nan() {
                if include_nan {
                    best = Some((k, f64::NAN));
                    break;
                }
                continue;
            }
            match best {
                None => best = Some((k, v)),
                Some((_, b)) if better(v, b) => best = Some((k, v)),
                _ => {}
            }
        }
        let (k, _) = best.unwrap_or((0, f64::NAN));
        let p = start + k * stride;
        if len == 0 {
            return;
        }
        if best.is_none() || best.is_some_and(|b| b.1.is_nan()) {
            vals.push(f64::NAN);
            ims.push(0.0);
        } else {
            vals.push(x.re[p]);
            ims.push(x.im_at(p));
        }
        idx.push(k as f64 + 1.0);
    });
    let mut first = if x.im.is_some() {
        Value::Num(Num::complex(out_dims.clone(), vals, ims).with_class(x.class))
    } else {
        Value::Num(Num::real(out_dims.clone(), vals).with_class(x.class))
    };
    if is_bool || is_char {
        first = match first {
            Value::Num(n) => Value::Num(n),
            v => v,
        };
    }
    let mut out = vec![first];
    if nargout > 1 {
        out.push(Value::Num(Num::real(out_dims, idx)));
    }
    Ok(out)
}

fn median(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    let (x, dim, all, omit) = red_args(&a, "median")?;
    let (x, dim) = if all {
        let n = x.re.len();
        (
            Num {
                dims: vec![n, 1],
                ..x
            },
            0,
        )
    } else {
        let d = dim.unwrap_or_else(|| default_dim(&x.dims));
        (x, d)
    };
    if x.dims == [0, 0] {
        return one(Value::num(f64::NAN));
    }
    let out_dims = reduced_dims(&x.dims, dim);
    let mut out = Vec::new();
    slices(&x.dims, dim, |start, stride, len| {
        let mut v: Vec<f64> = (0..len).map(|k| x.re[start + k * stride]).collect();
        if omit {
            v.retain(|x| !x.is_nan());
        } else if v.iter().any(|x| x.is_nan()) {
            out.push(f64::NAN);
            return;
        }
        v.sort_by(|p, q| p.partial_cmp(q).unwrap());
        let n = v.len();
        out.push(if n == 0 {
            f64::NAN
        } else if n % 2 == 1 {
            v[n / 2]
        } else {
            (v[n / 2 - 1] + v[n / 2]) / 2.0
        });
    });
    one(Value::Num(Num::real(out_dims, out)))
}

fn var_std(a: &[Value], sd: bool) -> R<Vec<Value>> {
    need(a, 1, "var")?;
    let x = arg_num(&a[0], "var")?;
    let w = match a.get(1) {
        Some(v) if !v.is_empty() => arg_f64(v, "var")?,
        _ => 0.0,
    };
    let dim = match a.get(2) {
        Some(v) => arg_usize(v, "var")? - 1,
        None => default_dim(&x.dims),
    };
    if x.dims == [0, 0] {
        return one(Value::num(f64::NAN));
    }
    let out_dims = reduced_dims(&x.dims, dim);
    let mut out = Vec::new();
    slices(&x.dims, dim, |start, stride, len| {
        let v: Vec<f64> = (0..len).map(|k| x.re[start + k * stride]).collect();
        let n = v.len() as f64;
        let m = v.iter().sum::<f64>() / n;
        let ss: f64 = v.iter().map(|x| (x - m) * (x - m)).sum();
        let denom = if w == 1.0 || n == 1.0 { n } else { n - 1.0 };
        let r = ss / denom;
        out.push(if sd { r.sqrt() } else { r });
    });
    one(Value::Num(Num::real(out_dims, out)))
}

fn diff(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "diff")?;
    let mut x = arg_num(&a[0], "diff")?;
    let order = match a.get(1) {
        Some(v) if !v.is_empty() => arg_usize(v, "diff")?,
        _ => 1,
    };
    let given_dim = match a.get(2) {
        Some(v) => Some(arg_usize(v, "diff")? - 1),
        None => None,
    };
    for _ in 0..order {
        let dim = given_dim.unwrap_or_else(|| default_dim(&x.dims));
        let len = *x.dims.get(dim).unwrap_or(&1);
        let mut nd = x.dims.clone();
        while nd.len() <= dim {
            nd.push(1);
        }
        nd[dim] = len.saturating_sub(1);
        let mut out = vec![0.0; numel_of(&nd)];
        let stride: usize = x.dims.iter().take(dim).product();
        let outer: usize = x.dims.iter().skip(dim + 1).product();
        let nlen = nd[dim];
        for o in 0..outer {
            for i in 0..stride {
                for k in 0..nlen {
                    let p = o * stride * len + i + k * stride;
                    out[o * stride * nlen + i + k * stride] = x.re[p + stride] - x.re[p];
                }
            }
        }
        x = Num::real(nd, out);
    }
    one(Value::Num(x))
}

fn dot(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "dot")?;
    let x = arg_num(&a[0], "dot")?;
    let y = arg_num(&a[1], "dot")?;
    if x.re.len() != y.re.len() {
        return Err(err(
            "MATLAB:dot:InputSizeMismatch",
            "Vectors must be the same length.",
        ));
    }
    let is_vec = |d: &[usize]| d.len() == 2 && (d[0] == 1 || d[1] == 1);
    if is_vec(&x.dims) && is_vec(&y.dims) {
        return one(Value::num(x.re.iter().zip(&y.re).map(|(p, q)| p * q).sum()));
    }
    let p = ops::binary(BinOp::EMul, &Value::Num(x), &Value::Num(y))?;
    reduce(&[p], "dot", Red::Sum)
}

fn cross(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "cross")?;
    let x = arg_num(&a[0], "cross")?;
    let y = arg_num(&a[1], "cross")?;
    if x.re.len() != 3 || y.re.len() != 3 {
        return Err(err(
            "MATLAB:cross:InvalidDimAorBForCrossProd",
            "cross: vectors must have 3 elements.",
        ));
    }
    let (p, q) = (&x.re, &y.re);
    let r = vec![
        p[1] * q[2] - p[2] * q[1],
        p[2] * q[0] - p[0] * q[2],
        p[0] * q[1] - p[1] * q[0],
    ];
    one(Value::Num(Num::real(x.dims.clone(), r)))
}

fn kron(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "kron")?;
    let x = arg_num(&a[0], "kron")?;
    let y = arg_num(&a[1], "kron")?;
    let (m, n) = (x.dims[0], x.dims[1]);
    let (p, q) = (y.dims[0], y.dims[1]);
    let (r, c) = (m * p, n * q);
    let mut out = vec![0.0; r * c];
    for i in 0..m {
        for j in 0..n {
            for k in 0..p {
                for l in 0..q {
                    out[(i * p + k) + (j * q + l) * r] = x.re[i + j * m] * y.re[k + l * p];
                }
            }
        }
    }
    one(Value::matrix(vec![r, c], out))
}

// ---------------------------------------------------------------------------
// find, sort, unique, sets
// ---------------------------------------------------------------------------

fn find(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "find")?;
    let v = &a[0];
    let n = arg_num(v, "find")?;
    let limit = match a.get(1) {
        Some(x) => Some(arg_usize(x, "find")?),
        None => None,
    };
    let from_end = a
        .get(2)
        .and_then(|x| x.to_str())
        .is_some_and(|s| s == "last");
    let mut hits: Vec<usize> = (0..n.re.len())
        .filter(|&i| n.re[i] != 0.0 || n.im_at(i) != 0.0)
        .collect();
    if let Some(k) = limit {
        if from_end {
            let s = hits.len().saturating_sub(k);
            hits = hits[s..].to_vec();
        } else {
            hits.truncate(k);
        }
    }
    let row = n.dims.len() == 2 && n.dims[0] == 1 && n.dims[1] != 1;
    let shape = |k: usize| -> Vec<usize> {
        if n.dims == [0, 0] && k == 0 {
            vec![0, 0]
        } else if row {
            vec![1, k]
        } else {
            vec![k, 1]
        }
    };
    let k = hits.len();
    if nargout <= 1 {
        return one(Value::Num(Num::real(
            shape(k),
            hits.iter().map(|i| *i as f64 + 1.0).collect(),
        )));
    }
    let r = n.dims[0].max(1);
    let rows: Vec<f64> = hits.iter().map(|i| (i % r) as f64 + 1.0).collect();
    let cols: Vec<f64> = hits.iter().map(|i| (i / r) as f64 + 1.0).collect();
    let mut out = vec![
        Value::Num(Num::real(shape(k), rows)),
        Value::Num(Num::real(shape(k), cols)),
    ];
    if nargout > 2 {
        let vals = index_value(
            v,
            &[Sub::List {
                idx: hits.clone(),
                shape: shape(k),
            }],
        )?;
        out.push(vals);
    }
    Ok(out)
}

fn cmp_f64(x: f64, y: f64) -> std::cmp::Ordering {
    match (x.is_nan(), y.is_nan()) {
        (true, true) => std::cmp::Ordering::Equal,
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        _ => x.partial_cmp(&y).unwrap(),
    }
}

fn sort(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "sort")?;
    let mut desc = false;
    let mut dim = None;
    for v in &a[1..] {
        if let Some(s) = v.to_str() {
            desc = s.eq_ignore_ascii_case("descend");
        } else {
            dim = Some(arg_usize(v, "sort")? - 1);
        }
    }
    if let Value::Cell(_) = &a[0] {
        let strs = super::cells::strings_of(&a[0], "sort")?;
        let mut idx: Vec<usize> = (0..strs.len()).collect();
        idx.sort_by(|&x, &y| {
            let o = strs[x].cmp(&strs[y]);
            if desc { o.reverse() } else { o }
        });
        let d = a[0].dims();
        let sorted: Vec<Value> = idx.iter().map(|&i| Value::str(&strs[i])).collect();
        let mut out = vec![Value::Cell(Array::new(d.clone(), sorted))];
        if nargout > 1 {
            out.push(Value::Num(Num::real(
                d,
                idx.iter().map(|i| *i as f64 + 1.0).collect(),
            )));
        }
        return Ok(out);
    }
    let x = arg_num(&a[0], "sort")?;
    let dim = dim.unwrap_or_else(|| default_dim(&x.dims));
    let mut re = x.re.clone();
    let mut im = x.im.clone();
    let mut ix = vec![0.0; x.re.len()];
    let key = |p: usize| -> f64 {
        if x.im.is_some() {
            c_abs((x.re[p], x.im_at(p)))
        } else {
            x.re[p]
        }
    };
    slices(&x.dims, dim, |start, stride, len| {
        let mut order: Vec<usize> = (0..len).collect();
        order.sort_by(|&p, &q| {
            let (u, v) = (key(start + p * stride), key(start + q * stride));
            if desc {
                // NaN first in descending order.
                cmp_f64(v, u)
            } else {
                cmp_f64(u, v)
            }
        });
        for (k, &o) in order.iter().enumerate() {
            let dst = start + k * stride;
            let src = start + o * stride;
            re[dst] = x.re[src];
            if let Some(im) = &mut im {
                im[dst] = x.im_at(src);
            }
            ix[dst] = o as f64 + 1.0;
        }
    });
    let sorted = Num {
        class: x.class,
        dims: x.dims.clone(),
        re,
        im,
    };
    let mut first = Value::Num(sorted);
    if let Value::Char(_) = &a[0] {
        first = to_class(&first, "char")?;
    } else if let Value::Bool(_) = &a[0] {
        first = logical_of(&first)?;
    }
    let mut out = vec![first];
    if nargout > 1 {
        out.push(Value::Num(Num::real(x.dims.clone(), ix)));
    }
    Ok(out)
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
enum Key {
    N(f64),
    S(String),
}

fn key_cmp(x: &Key, y: &Key) -> std::cmp::Ordering {
    match (x, y) {
        (Key::N(p), Key::N(q)) => cmp_f64(*p, *q),
        (Key::S(p), Key::S(q)) => p.cmp(q),
        (Key::N(_), Key::S(_)) => std::cmp::Ordering::Less,
        (Key::S(_), Key::N(_)) => std::cmp::Ordering::Greater,
    }
}

fn keys_of(v: &Value, fname: &str) -> R<Vec<Key>> {
    match v {
        Value::Cell(_) => Ok(super::cells::strings_of(v, fname)?
            .into_iter()
            .map(Key::S)
            .collect()),
        _ => Ok(arg_num(v, fname)?.re.into_iter().map(Key::N).collect()),
    }
}

fn keys_value(keys: &[Key], like: &Value, col: bool) -> Value {
    let n = keys.len();
    let dims = if col { vec![n, 1] } else { vec![1, n] };
    match like {
        Value::Cell(_) => Value::Cell(Array::new(
            dims,
            keys.iter()
                .map(|k| match k {
                    Key::S(s) => Value::str(s),
                    Key::N(x) => Value::num(*x),
                })
                .collect(),
        )),
        _ => {
            let re: Vec<f64> = keys
                .iter()
                .map(|k| if let Key::N(x) = k { *x } else { 0.0 })
                .collect();
            let v = Value::Num(Num::real(dims, re));
            match like {
                Value::Char(_) => to_class(&v, "char").unwrap_or(v),
                Value::Bool(_) => logical_of(&v).unwrap_or(v),
                Value::Num(n) if n.class != NumClass::Double => {
                    to_class(&v, n.class.name()).unwrap_or(v)
                }
                _ => v,
            }
        }
    }
}

fn unique(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 1, "unique")?;
    let stable = a[1..]
        .iter()
        .any(|v| v.to_str().is_some_and(|s| s == "stable"));
    let keys = keys_of(&a[0], "unique")?;
    let n = keys.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&x, &y| key_cmp(&keys[x], &keys[y]).then(x.cmp(&y)));
    // groups in sorted order; first occurrence (MATLAB returns last by
    // default in old versions; current MATLAB returns first).
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for &i in &order {
        match groups.last_mut() {
            Some(g)
                if key_cmp(&keys[g[0]], &keys[i]) == std::cmp::Ordering::Equal
                    && !matches!(keys[i], Key::N(x) if x.is_nan()) =>
            {
                g.push(i)
            }
            _ => groups.push(vec![i]),
        }
    }
    if stable {
        groups.sort_by_key(|g| g[0]);
    }
    let uniq: Vec<Key> = groups.iter().map(|g| keys[g[0]].clone()).collect();
    let d = a[0].dims();
    let row = d.len() == 2 && d[0] == 1 && !matches!(a[0], Value::Cell(_))
        || (matches!(a[0], Value::Cell(_)) && d.len() == 2 && d[0] == 1);
    let mut out = vec![keys_value(&uniq, &a[0], !row)];
    if nargout > 1 {
        let first: Vec<f64> = groups.iter().map(|g| g[0] as f64 + 1.0).collect();
        let k = first.len();
        out.push(Value::Num(Num::real(vec![k, 1], first)));
    }
    if nargout > 2 {
        let mut j = vec![0.0; n];
        for (gi, g) in groups.iter().enumerate() {
            for &i in g {
                j[i] = gi as f64 + 1.0;
            }
        }
        out.push(Value::Num(Num::real(vec![n, 1], j)));
    }
    Ok(out)
}

fn ismember(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "ismember")?;
    if a.get(2)
        .and_then(|v| v.to_str())
        .is_some_and(|s| s == "rows")
    {
        let x = arg_num(&a[0], "ismember")?;
        let s = arg_num(&a[1], "ismember")?;
        let (r, c) = (x.dims[0], x.dims[1]);
        let (sr, sc) = (s.dims[0], s.dims[1]);
        let mut tf = Vec::with_capacity(r);
        let mut loc = Vec::with_capacity(r);
        for i in 0..r {
            let hit = if c != sc {
                None
            } else {
                (0..sr).find(|&k| (0..c).all(|j| x.re[i + j * r] == s.re[k + j * sr]))
            };
            tf.push(hit.is_some());
            loc.push(hit.map_or(0.0, |k| k as f64 + 1.0));
        }
        let mut out = vec![Value::Bool(Array::new(vec![r, 1], tf))];
        if nargout > 1 {
            out.push(Value::col(loc));
        }
        return Ok(out);
    }
    let set = keys_of(&a[1], "ismember")?;
    let (items, dims) = match &a[0] {
        Value::Char(_) if matches!(a[1], Value::Cell(_)) => {
            (vec![Key::S(a[0].to_str().unwrap())], vec![1, 1])
        }
        v => (keys_of(v, "ismember")?, v.dims()),
    };
    let mut tf = Vec::with_capacity(items.len());
    let mut loc = Vec::with_capacity(items.len());
    for it in &items {
        match set.iter().position(|s| {
            key_cmp(s, it) == std::cmp::Ordering::Equal && !matches!(it, Key::N(x) if x.is_nan())
        }) {
            Some(p) => {
                tf.push(true);
                loc.push(p as f64 + 1.0);
            }
            None => {
                tf.push(false);
                loc.push(0.0);
            }
        }
    }
    let mut out = vec![Value::Bool(Array::new(dims.clone(), tf))];
    if nargout > 1 {
        out.push(Value::Num(Num::real(dims, loc)));
    }
    Ok(out)
}

fn setop(a: &[Value], nargout: usize, kind: u8) -> R<Vec<Value>> {
    need(a, 2, "setop")?;
    let x = keys_of(&a[0], "setop")?;
    let y = keys_of(&a[1], "setop")?;
    let stable = a[2..]
        .iter()
        .any(|v| v.to_str().is_some_and(|s| s == "stable"));
    let contains =
        |s: &[Key], k: &Key| s.iter().any(|t| key_cmp(t, k) == std::cmp::Ordering::Equal);
    let mut res: Vec<(Key, usize)> = Vec::new();
    match kind {
        0 => {
            for (i, k) in x.iter().enumerate() {
                if !contains(&y, k)
                    && !res
                        .iter()
                        .any(|(r, _)| key_cmp(r, k) == std::cmp::Ordering::Equal)
                {
                    res.push((k.clone(), i));
                }
            }
        }
        1 => {
            for (i, k) in x.iter().enumerate() {
                if contains(&y, k)
                    && !res
                        .iter()
                        .any(|(r, _)| key_cmp(r, k) == std::cmp::Ordering::Equal)
                {
                    res.push((k.clone(), i));
                }
            }
        }
        _ => {
            for (i, k) in x.iter().chain(y.iter()).enumerate() {
                if !res
                    .iter()
                    .any(|(r, _)| key_cmp(r, k) == std::cmp::Ordering::Equal)
                {
                    res.push((k.clone(), i));
                }
            }
        }
    }
    if !stable {
        res.sort_by(|p, q| key_cmp(&p.0, &q.0));
    }
    let keys: Vec<Key> = res.iter().map(|r| r.0.clone()).collect();
    let both_rows = [&a[0], &a[1]].iter().all(|v| {
        let d = v.dims_ref();
        d.len() == 2 && (d[0] == 1 || v.is_empty())
    });
    let mut out = vec![keys_value(&keys, &a[0], !both_rows)];
    if nargout > 1 {
        out.push(Value::col(res.iter().map(|r| r.1 as f64 + 1.0).collect()));
    }
    if nargout > 2 && kind == 1 {
        let ib: Vec<f64> = keys
            .iter()
            .map(|k| {
                y.iter()
                    .position(|t| key_cmp(t, k) == std::cmp::Ordering::Equal)
                    .map_or(0.0, |p| p as f64 + 1.0)
            })
            .collect();
        out.push(Value::col(ib));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// shape
// ---------------------------------------------------------------------------

/// Gather elements of `v` at linear positions into an array of `dims`.
pub fn take(v: &Value, pos: Vec<usize>, dims: Vec<usize>) -> R<Value> {
    let n = pos.len();
    let r = index_value(
        v,
        &[Sub::List {
            idx: pos,
            shape: vec![n, 1],
        }],
    )?;
    Ok(r.reshaped(dims))
}

fn reshape(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "reshape")?;
    let total = a[0].numel();
    let mut dims: Vec<Option<usize>> = Vec::new();
    if a.len() == 2 {
        for x in a[1].to_f64_vec().unwrap_or_default() {
            dims.push(Some(x as usize));
        }
    } else {
        for v in &a[1..] {
            if v.is_empty() {
                dims.push(None);
            } else {
                dims.push(Some(arg_usize(v, "reshape")?));
            }
        }
    }
    let known: usize = dims.iter().flatten().product();
    let holes = dims.iter().filter(|d| d.is_none()).count();
    if holes > 1 {
        return Err(err(
            "MATLAB:getReshapeDims:unknownDim",
            "Size can only have one unknown dimension.",
        ));
    }
    let dims: Vec<usize> = dims
        .iter()
        .map(|d| d.unwrap_or(total.checked_div(known).unwrap_or(0)))
        .collect();
    if numel_of(&dims) != total {
        return Err(err(
            "MATLAB:getReshapeDims:notSameNumel",
            format!(
                "Number of elements must not change. Use [] as one of the size inputs to automatically calculate the appropriate size for that dimension (from {} to {}).",
                total,
                numel_of(&dims)
            ),
        ));
    }
    one(a[0].clone().reshaped(dims))
}

fn repmat(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "repmat")?;
    let reps: Vec<usize> = if a.len() == 2 {
        let v = a[1].to_f64_vec().unwrap_or_default();
        if v.len() == 1 {
            vec![v[0].max(0.0) as usize; 2]
        } else {
            v.iter().map(|x| x.max(0.0) as usize).collect()
        }
    } else {
        a[1..]
            .iter()
            .map(|v| arg_f64(v, "repmat").map(|x| x.max(0.0) as usize))
            .collect::<R<Vec<_>>>()?
    };
    let src = &a[0];
    let sd = src.dims();
    let nd = reps.len().max(sd.len());
    let sdim: Vec<usize> = (0..nd).map(|k| *sd.get(k).unwrap_or(&1)).collect();
    let rdim: Vec<usize> = (0..nd).map(|k| *reps.get(k).unwrap_or(&1)).collect();
    let out: Vec<usize> = (0..nd).map(|k| sdim[k] * rdim[k]).collect();
    let total = numel_of(&out);
    let mut pos = Vec::with_capacity(total);
    let mut ctr = vec![0usize; nd];
    for _ in 0..total {
        let mut p = 0;
        let mut stride = 1;
        for k in 0..nd {
            p += (ctr[k] % sdim[k]) * stride;
            stride *= sdim[k];
        }
        pos.push(p);
        for k in 0..nd {
            ctr[k] += 1;
            if ctr[k] < out[k] {
                break;
            }
            ctr[k] = 0;
        }
    }
    if total == 0 {
        return one(take(src, vec![], out)?);
    }
    one(take(src, pos, out)?)
}

fn flip(a: &[Value], dim: Option<usize>) -> R<Vec<Value>> {
    need(a, 1, "flip")?;
    let v = &a[0];
    let d = v.dims();
    let dim = match a.get(1) {
        Some(x) => arg_usize(x, "flip")? - 1,
        None => dim.unwrap_or_else(|| default_dim(&d)),
    };
    let len = *d.get(dim).unwrap_or(&1);
    let mut pos: Vec<usize> = (0..v.numel()).collect();
    slices(&d, dim, |start, stride, l| {
        for k in 0..l {
            pos[start + k * stride] = start + (len - 1 - k) * stride;
        }
    });
    one(take(v, pos, d)?)
}

fn circshift(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "circshift")?;
    let v = &a[0];
    let d = v.dims();
    let shifts = a[1].to_f64_vec().unwrap_or_default();
    let mut per_dim = vec![0i64; d.len()];
    if let Some(x) = a.get(2) {
        let k = arg_usize(x, "circshift")? - 1;
        if k < per_dim.len() {
            per_dim[k] = shifts.first().copied().unwrap_or(0.0) as i64;
        }
    } else if shifts.len() == 1 {
        let k = default_dim(&d);
        per_dim[k] = shifts[0] as i64;
    } else {
        for (k, s) in shifts.iter().enumerate() {
            if k < per_dim.len() {
                per_dim[k] = *s as i64;
            }
        }
    }
    let total = v.numel();
    let mut pos = vec![0; total];
    for (dst, p) in pos.iter_mut().enumerate() {
        let mut rem = dst;
        let mut src = 0;
        let mut stride = 1;
        for k in 0..d.len() {
            let c = rem % d[k];
            rem /= d[k];
            let n = d[k] as i64;
            let sc = ((c as i64 - per_dim[k]) % n + n) % n;
            src += sc as usize * stride;
            stride *= d[k];
        }
        *p = src;
    }
    one(take(v, pos, d)?)
}

fn permute(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "permute")?;
    let v = &a[0];
    let order: Vec<usize> = a[1]
        .to_f64_vec()
        .unwrap_or_default()
        .iter()
        .map(|x| *x as usize - 1)
        .collect();
    let d = v.dims();
    let nd = order.len().max(d.len());
    let sd: Vec<usize> = (0..nd).map(|k| *d.get(k).unwrap_or(&1)).collect();
    let od: Vec<usize> = order.iter().map(|&k| *sd.get(k).unwrap_or(&1)).collect();
    let mut sstride = vec![1usize; nd];
    for k in 1..nd {
        sstride[k] = sstride[k - 1] * sd[k - 1];
    }
    let total = v.numel();
    let mut pos = Vec::with_capacity(total);
    let mut ctr = vec![0usize; od.len()];
    for _ in 0..total {
        let mut p = 0;
        for (k, &o) in order.iter().enumerate() {
            p += ctr[k] * sstride[o];
        }
        pos.push(p);
        for k in 0..od.len() {
            ctr[k] += 1;
            if ctr[k] < od[k] {
                break;
            }
            ctr[k] = 0;
        }
    }
    one(take(v, pos, od)?)
}

fn diag(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "diag")?;
    let x = arg_num(&a[0], "diag")?;
    let k = match a.get(1) {
        Some(v) => arg_f64(v, "diag")? as i64,
        None => 0,
    };
    let d = &x.dims;
    if d.len() == 2 && (d[0] == 1 || d[1] == 1) {
        let n = x.re.len();
        let m = n + k.unsigned_abs() as usize;
        let mut out = vec![0.0; m * m];
        for i in 0..n {
            let (r, c) = if k >= 0 {
                (i, i + k as usize)
            } else {
                (i + (-k) as usize, i)
            };
            out[r + c * m] = x.re[i];
        }
        return one(Value::Num(Num::real(vec![m, m], out).with_class(x.class)));
    }
    let (r, c) = (d[0], d[1]);
    let mut out = Vec::new();
    let mut i = 0usize;
    loop {
        let (row, col) = if k >= 0 {
            (i, i + k as usize)
        } else {
            (i + (-k) as usize, i)
        };
        if row >= r || col >= c {
            break;
        }
        out.push(x.re[row + col * r]);
        i += 1;
    }
    one(Value::col(out))
}

fn tri(a: &[Value], lower: bool) -> R<Vec<Value>> {
    need(a, 1, "tril")?;
    let x = arg_num(&a[0], "tril")?;
    let k = match a.get(1) {
        Some(v) => arg_f64(v, "tril")? as i64,
        None => 0,
    };
    let (r, c) = (x.dims[0], x.dims[1]);
    let mut out = x.re.clone();
    for j in 0..c {
        for i in 0..r {
            let keep = if lower {
                (j as i64 - i as i64) <= k
            } else {
                (j as i64 - i as i64) >= k
            };
            if !keep {
                out[i + j * r] = 0.0;
            }
        }
    }
    one(Value::Num(
        Num::real(x.dims.clone(), out).with_class(x.class),
    ))
}

fn square(x: &Num, fname: &str) -> R<usize> {
    if x.dims.len() != 2 || x.dims[0] != x.dims[1] {
        return Err(err(
            "MATLAB:square",
            format!("{}: matrix must be square.", fname),
        ));
    }
    Ok(x.dims[0])
}

fn det(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "det")?;
    let x = arg_num(&a[0], "det")?;
    let n = square(&x, "det")?;
    let mut m = x.re.clone();
    let mut d = 1.0;
    for c in 0..n {
        let mut p = c;
        for r in c + 1..n {
            if m[r + c * n].abs() > m[p + c * n].abs() {
                p = r;
            }
        }
        if m[p + c * n] == 0.0 {
            return one(Value::num(0.0));
        }
        if p != c {
            for k in 0..n {
                m.swap(c + k * n, p + k * n);
            }
            d = -d;
        }
        let piv = m[c + c * n];
        d *= piv;
        for r in c + 1..n {
            let f = m[r + c * n] / piv;
            for k in c..n {
                m[r + k * n] -= f * m[c + k * n];
            }
        }
    }
    one(Value::num(d))
}

fn inv(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "inv")?;
    let x = arg_num(&a[0], "inv")?;
    square(&x, "inv")?;
    let (r, singular) = ops::inverse(&x)?;
    if singular {
        it.warning(
            "MATLAB:singularMatrix",
            "Matrix is singular to working precision.",
        );
    }
    one(Value::Num(r))
}

fn norm(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "norm")?;
    let x = arg_num(&a[0], "norm")?;
    let p = match a.get(1) {
        None => Key::N(2.0),
        Some(v) => match v.to_str() {
            Some(s) => Key::S(s.to_ascii_lowercase()),
            None => Key::N(arg_f64(v, "norm")?),
        },
    };
    let mags: Vec<f64> = (0..x.re.len())
        .map(|i| c_abs((x.re[i], x.im_at(i))))
        .collect();
    let is_vec = x.dims.len() == 2 && (x.dims[0] == 1 || x.dims[1] == 1);
    if is_vec || x.re.is_empty() {
        let r = match p {
            Key::N(2.0) => mags.iter().fold(0.0, |a: f64, b| a.hypot(*b)),
            Key::N(1.0) => mags.iter().sum(),
            Key::N(q) if q == f64::INFINITY => mags.iter().cloned().fold(0.0, f64::max),
            Key::N(q) if q == f64::NEG_INFINITY => {
                mags.iter().cloned().fold(f64::INFINITY, f64::min)
            }
            Key::N(q) => {
                // Scale by the largest magnitude to avoid overflow.
                let m = mags.iter().cloned().fold(0.0, f64::max);
                if m == 0.0 || !m.is_finite() {
                    m
                } else {
                    m * mags
                        .iter()
                        .map(|v| (v / m).powf(q))
                        .sum::<f64>()
                        .powf(1.0 / q)
                }
            }
            Key::S(s) if s == "fro" => mags.iter().map(|v| v * v).sum::<f64>().sqrt(),
            Key::S(s) if s == "inf" => mags.iter().cloned().fold(0.0, f64::max),
            Key::S(s) => {
                return Err(err(
                    "MATLAB:norm:unknownNorm",
                    format!(
                        "The only matrix norms available are 1, 2, inf, and 'fro' (got '{}').",
                        s
                    ),
                ));
            }
        };
        return one(Value::num(r));
    }
    let (r, c) = (x.dims[0], x.dims[1]);
    let res = match p {
        Key::N(1.0) => (0..c)
            .map(|j| (0..r).map(|i| mags[i + j * r]).sum::<f64>())
            .fold(0.0, f64::max),
        Key::N(q) if q == f64::INFINITY => (0..r)
            .map(|i| (0..c).map(|j| mags[i + j * r]).sum::<f64>())
            .fold(0.0, f64::max),
        Key::S(s) if s == "inf" => (0..r)
            .map(|i| (0..c).map(|j| mags[i + j * r]).sum::<f64>())
            .fold(0.0, f64::max),
        Key::S(s) if s == "fro" => mags.iter().map(|v| v * v).sum::<f64>().sqrt(),
        Key::N(2.0) => {
            if x.im.is_some() {
                return Err(err(
                    "MATLAB:unsupported",
                    "norm: complex matrix 2-norm is not supported yet.",
                ));
            }
            // Largest singular value: power iteration on A'A.
            let ata = |v: &[f64]| -> Vec<f64> {
                let mut av = vec![0.0; r];
                for j in 0..c {
                    for i in 0..r {
                        av[i] += x.re[i + j * r] * v[j];
                    }
                }
                let mut out = vec![0.0; c];
                for j in 0..c {
                    for i in 0..r {
                        out[j] += x.re[i + j * r] * av[i];
                    }
                }
                out
            };
            let mut v = vec![1.0; c];
            let mut lam = 0.0;
            for _ in 0..1000 {
                let w = ata(&v);
                let nw = w.iter().map(|t| t * t).sum::<f64>().sqrt();
                if nw == 0.0 {
                    break;
                }
                let new_lam = nw / v.iter().map(|t| t * t).sum::<f64>().sqrt();
                v = w.iter().map(|t| t / nw).collect();
                if (new_lam - lam).abs() <= 1e-15 * new_lam {
                    lam = new_lam;
                    break;
                }
                lam = new_lam;
            }
            lam.sqrt()
        }
        _ => {
            return Err(err(
                "MATLAB:norm:unknownNorm",
                "The only matrix norms available are 1, 2, inf, and 'fro'.",
            ));
        }
    };
    one(Value::num(res))
}

fn is_prime(x: f64) -> bool {
    if x < 2.0 || x.fract() != 0.0 {
        return false;
    }
    let n = x as u64;
    if n < 4 {
        return true;
    }
    if n.is_multiple_of(2) {
        return false;
    }
    let mut k = 3;
    while k * k <= n {
        if n.is_multiple_of(k) {
            return false;
        }
        k += 2;
    }
    true
}

fn primes(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 1, "primes")?;
    let n = arg_f64(&a[0], "primes")?.floor().max(0.0) as usize;
    let mut sieve = vec![true; n + 1];
    let mut out = Vec::new();
    for i in 2..=n {
        if sieve[i] {
            out.push(i as f64);
            let mut k = i * i;
            while k <= n {
                sieve[k] = false;
                k += i;
            }
        }
    }
    one(Value::row(out))
}

fn nchoosek(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "nchoosek")?;
    let k = arg_usize(&a[1], "nchoosek")?;
    if a[0].numel() == 1 {
        let n = arg_f64(&a[0], "nchoosek")?;
        let mut r = 1.0;
        for i in 0..k {
            r = r * (n - i as f64) / (i as f64 + 1.0);
        }
        return one(Value::num(r.round()));
    }
    let v = arg_num(&a[0], "nchoosek")?.re;
    let n = v.len();
    let mut combos: Vec<Vec<f64>> = Vec::new();
    let mut idx: Vec<usize> = (0..k).collect();
    if k <= n {
        loop {
            combos.push(idx.iter().map(|&i| v[i]).collect());
            let mut i = k;
            let mut done = true;
            while i > 0 {
                i -= 1;
                if idx[i] != i + n - k {
                    idx[i] += 1;
                    for j in i + 1..k {
                        idx[j] = idx[j - 1] + 1;
                    }
                    done = false;
                    break;
                }
            }
            if done || k == 0 {
                break;
            }
        }
    }
    let r = combos.len();
    let mut out = vec![0.0; r * k];
    for (i, cmb) in combos.iter().enumerate() {
        for (j, x) in cmb.iter().enumerate() {
            out[i + j * r] = *x;
        }
    }
    one(Value::matrix(vec![r, k], out))
}

fn sub2ind(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "sub2ind")?;
    let d: Vec<usize> = a[0]
        .to_f64_vec()
        .unwrap_or_default()
        .iter()
        .map(|x| *x as usize)
        .collect();
    let subs: Vec<Num> = a[1..]
        .iter()
        .map(|v| arg_num(v, "sub2ind"))
        .collect::<R<Vec<_>>>()?;
    let n = subs[0].re.len();
    let mut out = vec![0.0; n];
    for (i, o) in out.iter_mut().enumerate() {
        let mut p = 0usize;
        let mut stride = 1usize;
        for (k, s) in subs.iter().enumerate() {
            let v = s.re[i] as usize;
            let lim = if k + 1 == subs.len() {
                d[k..].iter().product::<usize>().max(1)
            } else {
                *d.get(k).unwrap_or(&1)
            };
            if v < 1 || v > lim {
                return Err(err(
                    "MATLAB:sub2ind:IndexOutOfRange",
                    "Out of range subscript.",
                ));
            }
            p += (v - 1) * stride;
            stride *= *d.get(k).unwrap_or(&1);
        }
        *o = p as f64 + 1.0;
    }
    one(Value::Num(Num::real(subs[0].dims.clone(), out)))
}

fn ind2sub(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R<Vec<Value>> {
    need(&a, 2, "ind2sub")?;
    let d: Vec<usize> = a[0]
        .to_f64_vec()
        .unwrap_or_default()
        .iter()
        .map(|x| *x as usize)
        .collect();
    let idx = arg_num(&a[1], "ind2sub")?;
    let nout = nargout.max(1);
    let dd = indy_value::index::collapse_dims(&d, nout);
    let mut outs = vec![vec![0.0; idx.re.len()]; nout];
    for (i, x) in idx.re.iter().enumerate() {
        let mut rem = *x as usize - 1;
        for k in 0..nout {
            if k + 1 == nout {
                outs[k][i] = rem as f64 + 1.0;
            } else {
                outs[k][i] = (rem % dd[k]) as f64 + 1.0;
                rem /= dd[k];
            }
        }
    }
    Ok(outs
        .into_iter()
        .map(|o| Value::Num(Num::real(idx.dims.clone(), o)))
        .collect())
}

fn accumarray(it: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "accumarray")?;
    let subs = arg_num(&a[0], "accumarray")?;
    let vals = arg_num(&a[1], "accumarray")?;
    let ncols = if subs.dims[1] > 1 && subs.dims[0] > 1
        || (subs.dims[0] == 1 && subs.dims[1] > 1 && vals.re.len() == 1)
    {
        subs.dims[1]
    } else {
        1
    };
    let nrows = subs.re.len() / ncols;
    let mut dims: Vec<usize> = (0..ncols)
        .map(|k| {
            (0..nrows)
                .map(|i| subs.re[i + k * nrows] as usize)
                .max()
                .unwrap_or(0)
        })
        .collect();
    if let Some(sz) = a.get(2)
        && !sz.is_empty()
    {
        dims = sz
            .to_f64_vec()
            .unwrap_or_default()
            .iter()
            .map(|x| *x as usize)
            .collect();
    }
    if dims.len() == 1 {
        dims.push(1);
    }
    let total = numel_of(&dims);
    let mut groups: Vec<Vec<f64>> = vec![Vec::new(); total];
    for i in 0..nrows {
        let mut p = 0;
        let mut stride = 1;
        for k in 0..ncols {
            p += (subs.re[i + k * nrows] as usize - 1) * stride;
            stride *= dims[k];
        }
        let v = if vals.re.len() == 1 {
            vals.re[0]
        } else {
            vals.re[i]
        };
        if p < total {
            groups[p].push(v);
        }
    }
    let fun = a.get(3).filter(|v| !v.is_empty()).cloned();
    let fill = a.get(4).and_then(|v| v.scalar_f64()).unwrap_or(0.0);
    let mut out = vec![fill; total];
    for (p, g) in groups.iter().enumerate() {
        if g.is_empty() {
            continue;
        }
        out[p] = match &fun {
            None => g.iter().sum(),
            Some(f) => {
                let r = it.call_value(f, vec![Value::col(g.clone())], 1)?;
                r.first().and_then(|v| v.scalar_f64()).unwrap_or(f64::NAN)
            }
        };
    }
    one(Value::Num(Num::real(dims, out)))
}

fn histc(_: &mut Interp, a: Vec<Value>, _: usize) -> R<Vec<Value>> {
    need(&a, 2, "histc")?;
    let x = arg_num(&a[0], "histc")?;
    let edges = arg_num(&a[1], "histc")?.re;
    let mut counts = vec![0.0; edges.len()];
    for v in &x.re {
        for k in 0..edges.len() {
            let last = k + 1 == edges.len();
            if (last && *v == edges[k]) || (!last && *v >= edges[k] && *v < edges[k + 1]) {
                counts[k] += 1.0;
                break;
            }
        }
    }
    let row = x.dims[0] == 1;
    one(if row {
        Value::row(counts)
    } else {
        Value::col(counts)
    })
}

#[allow(dead_code)]
fn unused(_: &Error) {}
