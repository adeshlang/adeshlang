//! Number and Math Builtin Methods
//!
//! This module contains all built-in number and Math namespace methods for the language.
//! Includes mathematical operations, trigonometry, logarithms, random number generation,
//! and more.

use super::super::super::interpreter::err;
use crate::parsing::ast::Value;

/// Math constants
pub fn get_math_constant(name: &str) -> Option<Value> {
    match name {
        "PI" => Some(Value::Number(std::f64::consts::PI)),
        "E" => Some(Value::Number(std::f64::consts::E)),
        "TAU" => Some(Value::Number(std::f64::consts::TAU)),
        "SQRT2" => Some(Value::Number(std::f64::consts::SQRT_2)),
        "SQRT1_2" => Some(Value::Number(std::f64::consts::FRAC_1_SQRT_2)),
        "LN2" => Some(Value::Number(std::f64::consts::LN_2)),
        "LN10" => Some(Value::Number(std::f64::consts::LN_10)),
        _ => None,
    }
}

fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => Some(*n),
        Value::I8(n) => Some(*n as f64),
        Value::I16(n) => Some(*n as f64),
        Value::I32(n) => Some(*n as f64),
        Value::I64(n) => Some(*n as f64),
        Value::I128(n) => Some(*n as f64),
        Value::U8(n) => Some(*n as f64),
        Value::U16(n) => Some(*n as f64),
        Value::U32(n) => Some(*n as f64),
        Value::U64(n) => Some(*n as f64),
        Value::U128(n) => Some(*n as f64),
        Value::F32(n) => Some(*n as f64),
        Value::F64(n) => Some(*n),
        _ => None,
    }
}

/// Implements all Math namespace methods
pub fn call_math_method(method_name: &str, args: &[Value]) -> Result<Value, String> {
    match method_name {
        "random" => {
            use rand::Rng;
            Ok(Value::Number(rand::thread_rng().gen_range(0.0..1.0)))
        }
        "seed" => {
            // Note: Seeding is handled in stdlib registry - this is for compatibility
            Ok(Value::Null)
        }
        "randomInt" => {
            use rand::Rng;
            if args.len() != 2 {
                return Err(err("Math.randomInt(min, max)"));
            }
            let min = as_f64(&args[0]).ok_or_else(|| err("min must be number"))? as i64;
            let max = as_f64(&args[1]).ok_or_else(|| err("max must be number"))? as i64;
            Ok(Value::Number(rand::thread_rng().gen_range(min..=max) as f64))
        }
        "randomRange" => {
            use rand::Rng;
            if args.len() != 2 {
                return Err(err("Math.randomRange(min, max)"));
            }
            let min = as_f64(&args[0]).ok_or_else(|| err("min must be number"))?;
            let max = as_f64(&args[1]).ok_or_else(|| err("max must be number"))?;
            Ok(Value::Number(rand::thread_rng().gen_range(min..max)))
        }
        "floor" => {
            if args.len() != 1 {
                return Err(err("Math.floor(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n.floor()))
        }
        "ceil" => {
            if args.len() != 1 {
                return Err(err("Math.ceil(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n.ceil()))
        }
        "round" => {
            if args.len() != 1 {
                return Err(err("Math.round(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n.round()))
        }
        "abs" => {
            if args.len() != 1 {
                return Err(err("Math.abs(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n.abs()))
        }
        "min" => {
            if args.is_empty() {
                return Err(err("Math.min(a, b, ...)"));
            }
            let mut m = f64::INFINITY;
            for v in args {
                let n = as_f64(v).ok_or_else(|| err("args must be numbers"))?;
                if n < m {
                    m = n;
                }
            }
            Ok(Value::Number(m))
        }
        "max" => {
            if args.is_empty() {
                return Err(err("Math.max(a, b, ...)"));
            }
            let mut m = f64::NEG_INFINITY;
            for v in args {
                let n = as_f64(v).ok_or_else(|| err("args must be numbers"))?;
                if n > m {
                    m = n;
                }
            }
            Ok(Value::Number(m))
        }
        "pow" => {
            if args.len() != 2 {
                return Err(err("Math.pow(x, y)"));
            }
            let x = as_f64(&args[0]).ok_or_else(|| err("x must be number"))?;
            let y = as_f64(&args[1]).ok_or_else(|| err("y must be number"))?;
            Ok(Value::Number(x.powf(y)))
        }
        "sqrt" => {
            if args.len() != 1 {
                return Err(err("Math.sqrt(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n.sqrt()))
        }
        "sin" => {
            if args.len() != 1 {
                return Err(err("Math.sin(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n.sin()))
        }
        "cos" => {
            if args.len() != 1 {
                return Err(err("Math.cos(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n.cos()))
        }
        "tan" => {
            if args.len() != 1 {
                return Err(err("Math.tan(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n.tan()))
        }
        "asin" => {
            if args.len() != 1 {
                return Err(err("Math.asin(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n.asin()))
        }
        "acos" => {
            if args.len() != 1 {
                return Err(err("Math.acos(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n.acos()))
        }
        "atan" => {
            if args.len() != 1 {
                return Err(err("Math.atan(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n.atan()))
        }
        "atan2" => {
            if args.len() != 2 {
                return Err(err("Math.atan2(y, x)"));
            }
            let y = as_f64(&args[0]).ok_or_else(|| err("y must be number"))?;
            let x = as_f64(&args[1]).ok_or_else(|| err("x must be number"))?;
            Ok(Value::Number(y.atan2(x)))
        }
        "exp" => {
            if args.len() != 1 {
                return Err(err("Math.exp(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n.exp()))
        }
        "log" => {
            if args.len() != 1 {
                return Err(err("Math.log(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n.ln()))
        }
        "log10" => {
            if args.len() != 1 {
                return Err(err("Math.log10(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n.log10()))
        }
        "log2" => {
            if args.len() != 1 {
                return Err(err("Math.log2(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n.log2()))
        }
        "trunc" => {
            if args.len() != 1 {
                return Err(err("Math.trunc(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n.trunc()))
        }
        "sign" => {
            if args.len() != 1 {
                return Err(err("Math.sign(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            let s = if n > 0.0 {
                1.0
            } else if n < 0.0 {
                -1.0
            } else {
                0.0
            };
            Ok(Value::Number(s))
        }
        "sinh" => {
            if args.len() != 1 {
                return Err(err("Math.sinh(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n.sinh()))
        }
        "cosh" => {
            if args.len() != 1 {
                return Err(err("Math.cosh(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n.cosh()))
        }
        "tanh" => {
            if args.len() != 1 {
                return Err(err("Math.tanh(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n.tanh()))
        }
        "asinh" => {
            if args.len() != 1 {
                return Err(err("Math.asinh(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n.asinh()))
        }
        "acosh" => {
            if args.len() != 1 {
                return Err(err("Math.acosh(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n.acosh()))
        }
        "atanh" => {
            if args.len() != 1 {
                return Err(err("Math.atanh(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n.atanh()))
        }
        "degToRad" => {
            if args.len() != 1 {
                return Err(err("Math.degToRad(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n * std::f64::consts::PI / 180.0))
        }
        "radToDeg" => {
            if args.len() != 1 {
                return Err(err("Math.radToDeg(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n * 180.0 / std::f64::consts::PI))
        }
        "clamp" => {
            if args.len() != 3 {
                return Err(err("Math.clamp(val, min, max)"));
            }
            let val = as_f64(&args[0]).ok_or_else(|| err("val must be number"))?;
            let min = as_f64(&args[1]).ok_or_else(|| err("min must be number"))?;
            let max = as_f64(&args[2]).ok_or_else(|| err("max must be number"))?;
            Ok(Value::Number(val.clamp(min, max)))
        }
        "lerp" => {
            if args.len() != 3 {
                return Err(err("Math.lerp(a, b, t)"));
            }
            let a = as_f64(&args[0]).ok_or_else(|| err("a must be number"))?;
            let b = as_f64(&args[1]).ok_or_else(|| err("b must be number"))?;
            let t = as_f64(&args[2]).ok_or_else(|| err("t must be number"))?;
            Ok(Value::Number(a + (b - a) * t))
        }
        "hypot" => {
            if args.len() != 2 {
                return Err(err("Math.hypot(x, y)"));
            }
            let x = as_f64(&args[0]).ok_or_else(|| err("x must be number"))?;
            let y = as_f64(&args[1]).ok_or_else(|| err("y must be number"))?;
            Ok(Value::Number(x.hypot(y)))
        }
        "cbrt" => {
            if args.len() != 1 {
                return Err(err("Math.cbrt(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n.cbrt()))
        }
        "gcd" => {
            if args.len() != 2 {
                return Err(err("Math.gcd(a, b)"));
            }
            let a = as_f64(&args[0]).ok_or_else(|| err("a must be number"))? as i64;
            let b = as_f64(&args[1]).ok_or_else(|| err("b must be number"))? as i64;
            Ok(Value::Number(gcd(a, b) as f64))
        }
        "lcm" => {
            if args.len() != 2 {
                return Err(err("Math.lcm(a, b)"));
            }
            let a = as_f64(&args[0]).ok_or_else(|| err("a must be number"))? as i64;
            let b = as_f64(&args[1]).ok_or_else(|| err("b must be number"))? as i64;
            Ok(Value::Number(lcm(a, b) as f64))
        }
        "factorial" => {
            if args.len() != 1 {
                return Err(err("Math.factorial(n)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("n must be number"))? as i64;
            Ok(Value::Number(factorial(n) as f64))
        }
        "comb" => {
            if args.len() != 2 {
                return Err(err("Math.comb(n, k)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("n must be number"))? as i64;
            let k = as_f64(&args[1]).ok_or_else(|| err("k must be number"))? as i64;
            Ok(Value::Number(comb(n, k)))
        }
        "perm" => {
            if args.len() != 2 {
                return Err(err("Math.perm(n, k)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("n must be number"))? as i64;
            let k = as_f64(&args[1]).ok_or_else(|| err("k must be number"))? as i64;
            Ok(Value::Number(perm(n, k)))
        }
        "isqrt" => {
            if args.len() != 1 {
                return Err(err("Math.isqrt(n)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("n must be number"))? as i64;
            Ok(Value::Number(isqrt(n) as f64))
        }
        "fabs" => {
            if args.len() != 1 {
                return Err(err("Math.fabs(x)"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("arg must be number"))?;
            Ok(Value::Number(n.abs()))
        }
        "fma" => {
            if args.len() != 3 {
                return Err(err("Math.fma(x, y, z)"));
            }
            let x = as_f64(&args[0]).ok_or_else(|| err("x must be number"))?;
            let y = as_f64(&args[1]).ok_or_else(|| err("y must be number"))?;
            let z = as_f64(&args[2]).ok_or_else(|| err("z must be number"))?;
            Ok(Value::Number(x.mul_add(y, z)))
        }
        "fmod" => {
            if args.len() != 2 {
                return Err(err("Math.fmod(x, y)"));
            }
            let x = as_f64(&args[0]).ok_or_else(|| err("x must be number"))?;
            let y = as_f64(&args[1]).ok_or_else(|| err("y must be number"))?;
            Ok(Value::Number(x % y))
        }
        "modf" => {
            if args.len() != 1 {
                return Err(err("Math.modf(x)"));
            }
            let x = as_f64(&args[0]).ok_or_else(|| err("x must be number"))?;
            let integral = x.trunc();
            let fract = x - integral;
            Ok(Value::Tuple(vec![
                Value::Number(fract),
                Value::Number(integral),
            ]))
        }
        "remainder" => {
            if args.len() != 2 {
                return Err(err("Math.remainder(x, y)"));
            }
            let x = as_f64(&args[0]).ok_or_else(|| err("x must be number"))?;
            let y = as_f64(&args[1]).ok_or_else(|| err("y must be number"))?;
            Ok(Value::Number(remainder(x, y)))
        }
        "copysign" => {
            if args.len() != 2 {
                return Err(err("Math.copysign(x, y)"));
            }
            let x = as_f64(&args[0]).ok_or_else(|| err("x must be number"))?;
            let y = as_f64(&args[1]).ok_or_else(|| err("y must be number"))?;
            Ok(Value::Number(x.copysign(y)))
        }
        "frexp" => {
            if args.len() != 1 {
                return Err(err("Math.frexp(x)"));
            }
            let x = as_f64(&args[0]).ok_or_else(|| err("x must be number"))?;
            let (m, e) = frexp(x);
            Ok(Value::Tuple(vec![
                Value::Number(m),
                Value::Number(e as f64),
            ]))
        }
        "ldexp" => {
            if args.len() != 2 {
                return Err(err("Math.ldexp(x, i)"));
            }
            let x = as_f64(&args[0]).ok_or_else(|| err("x must be number"))?;
            let i = as_f64(&args[1]).ok_or_else(|| err("i must be number"))? as i32;
            Ok(Value::Number(x * 2.0f64.powi(i)))
        }
        "isclose" => {
            if args.len() < 2 {
                return Err(err("Math.isclose(a, b, [rel_tol], [abs_tol])"));
            }
            let a = as_f64(&args[0]).ok_or_else(|| err("a must be number"))?;
            let b = as_f64(&args[1]).ok_or_else(|| err("b must be number"))?;
            let rel_tol = if args.len() > 2 {
                as_f64(&args[2]).unwrap_or(1e-09)
            } else {
                1e-09
            };
            let abs_tol = if args.len() > 3 {
                as_f64(&args[3]).unwrap_or(0.0)
            } else {
                0.0
            };
            let close = (a - b).abs() <= f64::max(rel_tol * f64::max(a.abs(), b.abs()), abs_tol);
            Ok(Value::Bool(close))
        }
        "isfinite" | "isFinite" => {
            if args.len() != 1 {
                return Err(err("Math.isfinite(x)"));
            }
            let x = as_f64(&args[0]).ok_or_else(|| err("x must be number"))?;
            Ok(Value::Bool(x.is_finite()))
        }
        "isinf" | "isInf" => {
            if args.len() != 1 {
                return Err(err("Math.isinf(x)"));
            }
            let x = as_f64(&args[0]).ok_or_else(|| err("x must be number"))?;
            Ok(Value::Bool(x.is_infinite()))
        }
        "isnan" | "isNaN" => {
            if args.len() != 1 {
                return Err(err("Math.isnan(x)"));
            }
            let x = as_f64(&args[0]).ok_or_else(|| err("x must be number"))?;
            Ok(Value::Bool(x.is_nan()))
        }
        "nextafter" => {
            if args.len() < 2 {
                return Err(err("Math.nextafter(x, y, [steps])"));
            }
            let mut x = as_f64(&args[0]).ok_or_else(|| err("x must be number"))?;
            let y = as_f64(&args[1]).ok_or_else(|| err("y must be number"))?;
            let steps = if args.len() > 2 {
                as_f64(&args[2]).unwrap_or(1.0) as i64
            } else {
                1
            };
            for _ in 0..steps {
                x = nextafter(x, y);
            }
            Ok(Value::Number(x))
        }
        "ulp" => {
            if args.len() != 1 {
                return Err(err("Math.ulp(x)"));
            }
            let x = as_f64(&args[0]).ok_or_else(|| err("x must be number"))?;
            Ok(Value::Number(ulp(x)))
        }
        "exp2" => {
            if args.len() != 1 {
                return Err(err("Math.exp2(x)"));
            }
            let x = as_f64(&args[0]).ok_or_else(|| err("x must be number"))?;
            Ok(Value::Number(x.exp2()))
        }
        "expm1" => {
            if args.len() != 1 {
                return Err(err("Math.expm1(x)"));
            }
            let x = as_f64(&args[0]).ok_or_else(|| err("x must be number"))?;
            Ok(Value::Number(x.exp_m1()))
        }
        "log1p" => {
            if args.len() != 1 {
                return Err(err("Math.log1p(x)"));
            }
            let x = as_f64(&args[0]).ok_or_else(|| err("x must be number"))?;
            Ok(Value::Number(x.ln_1p()))
        }
        "dist" => {
            if args.len() != 2 {
                return Err(err("Math.dist(p, q)"));
            }
            let p = val_to_vec_f64(&args[0])?;
            let q = val_to_vec_f64(&args[1])?;
            Ok(Value::Number(dist(&p, &q)))
        }
        "fsum" => {
            if args.len() != 1 {
                return Err(err("Math.fsum(iterable)"));
            }
            let arr = val_to_vec_f64(&args[0])?;
            Ok(Value::Number(fsum(&arr)))
        }
        "prod" => {
            if args.len() < 1 {
                return Err(err("Math.prod(iterable, [start])"));
            }
            let arr = val_to_vec_f64(&args[0])?;
            let start = if args.len() > 1 {
                as_f64(&args[1]).unwrap_or(1.0)
            } else {
                1.0
            };
            Ok(Value::Number(prod(&arr, start)))
        }
        "sumprod" => {
            if args.len() != 2 {
                return Err(err("Math.sumprod(p, q)"));
            }
            let p = val_to_vec_f64(&args[0])?;
            let q = val_to_vec_f64(&args[1])?;
            Ok(Value::Number(sumprod(&p, &q)))
        }
        "degrees" => {
            if args.len() != 1 {
                return Err(err("Math.degrees(x)"));
            }
            let x = as_f64(&args[0]).ok_or_else(|| err("x must be number"))?;
            Ok(Value::Number(x * 180.0 / std::f64::consts::PI))
        }
        "radians" => {
            if args.len() != 1 {
                return Err(err("Math.radians(x)"));
            }
            let x = as_f64(&args[0]).ok_or_else(|| err("x must be number"))?;
            Ok(Value::Number(x * std::f64::consts::PI / 180.0))
        }
        "erf" => {
            if args.len() != 1 {
                return Err(err("Math.erf(x)"));
            }
            let x = as_f64(&args[0]).ok_or_else(|| err("x must be number"))?;
            Ok(Value::Number(erf(x)))
        }
        "erfc" => {
            if args.len() != 1 {
                return Err(err("Math.erfc(x)"));
            }
            let x = as_f64(&args[0]).ok_or_else(|| err("x must be number"))?;
            Ok(Value::Number(1.0 - erf(x)))
        }
        "gamma" => {
            if args.len() != 1 {
                return Err(err("Math.gamma(x)"));
            }
            let x = as_f64(&args[0]).ok_or_else(|| err("x must be number"))?;
            Ok(Value::Number(gamma(x)))
        }
        "lgamma" => {
            if args.len() != 1 {
                return Err(err("Math.lgamma(x)"));
            }
            let x = as_f64(&args[0]).ok_or_else(|| err("x must be number"))?;
            Ok(Value::Number(gamma(x).abs().ln()))
        }
        "formatDecimal" => {
            if args.len() < 1 {
                return Err(err("Math.formatDecimal(n, [precision])"));
            }
            let n = as_f64(&args[0]).ok_or_else(|| err("n must be number"))?;
            let precision = if args.len() > 1 {
                as_f64(&args[1]).unwrap_or(6.0) as usize
            } else {
                6
            };
            Ok(Value::Str(format!("{:.*}", precision, n)))
        }
        _ => Err(err(format!("Unknown Math method: {}", method_name))),
    }
}

pub fn call_cmath_method(method_name: &str, args: &[Value]) -> Result<Value, String> {
    let err = |msg: &str| -> String { msg.to_string() };
    let to_complex = |v: &Value| -> Result<(f64, f64), String> {
        match v {
            Value::Complex(r, i) => Ok((*r, *i)),
            _ => {
                if let Some(n) = as_f64(v) {
                    Ok((n, 0.0))
                } else {
                    Err("Expected complex or real number".to_string())
                }
            }
        }
    };

    match method_name {
        "cos" => {
            if args.len() != 1 {
                return Err(err("cmath.cos(z)"));
            }
            let (r, i) = to_complex(&args[0])?;
            Ok(Value::Complex(r.cos() * i.cosh(), -r.sin() * i.sinh()))
        }
        "sin" => {
            if args.len() != 1 {
                return Err(err("cmath.sin(z)"));
            }
            let (r, i) = to_complex(&args[0])?;
            Ok(Value::Complex(r.sin() * i.cosh(), r.cos() * i.sinh()))
        }
        "tan" => {
            if args.len() != 1 {
                return Err(err("cmath.tan(z)"));
            }
            let (r, i) = to_complex(&args[0])?;
            let denom = (2.0 * r).cos() + (2.0 * i).cosh();
            if denom == 0.0 {
                return Ok(Value::Complex(std::f64::NAN, std::f64::NAN));
            }
            Ok(Value::Complex(
                (2.0 * r).sin() / denom,
                (2.0 * i).sinh() / denom,
            ))
        }
        "acos" => {
            if args.len() != 1 {
                return Err(err("cmath.acos(z)"));
            }
            let (r, i) = to_complex(&args[0])?;
            // acos(z) = -i * log(z + i * sqrt(1 - z^2))
            // Let's implement via standard formula
            let z_sq_r = r * r - i * i;
            let z_sq_i = 2.0 * r * i;
            let one_minus_z_sq_r = 1.0 - z_sq_r;
            let one_minus_z_sq_i = -z_sq_i;
            let (sr, si) = complex_sqrt(one_minus_z_sq_r, one_minus_z_sq_i);
            let sum_r = r - si;
            let sum_i = i + sr;
            let (lr, li) = complex_log(sum_r, sum_i);
            Ok(Value::Complex(li, -lr))
        }
        "asin" => {
            if args.len() != 1 {
                return Err(err("cmath.asin(z)"));
            }
            let (r, i) = to_complex(&args[0])?;
            // asin(z) = -i * log(i*z + sqrt(1 - z^2))
            let z_sq_r = r * r - i * i;
            let z_sq_i = 2.0 * r * i;
            let one_minus_z_sq_r = 1.0 - z_sq_r;
            let one_minus_z_sq_i = -z_sq_i;
            let (sr, si) = complex_sqrt(one_minus_z_sq_r, one_minus_z_sq_i);
            let sum_r = -i + sr;
            let sum_i = r + si;
            let (lr, li) = complex_log(sum_r, sum_i);
            Ok(Value::Complex(li, -lr))
        }
        "atan" => {
            if args.len() != 1 {
                return Err(err("cmath.atan(z)"));
            }
            let (r, i) = to_complex(&args[0])?;
            // atan(z) = 0.5 * i * (log(1 - i*z) - log(1 + i*z))
            let (lr1, li1) = complex_log(1.0 + i, -r);
            let (lr2, li2) = complex_log(1.0 - i, r);
            Ok(Value::Complex(0.5 * (li2 - li1), 0.5 * (lr1 - lr2)))
        }
        "cosh" => {
            if args.len() != 1 {
                return Err(err("cmath.cosh(z)"));
            }
            let (r, i) = to_complex(&args[0])?;
            Ok(Value::Complex(r.cosh() * i.cos(), r.sinh() * i.sin()))
        }
        "sinh" => {
            if args.len() != 1 {
                return Err(err("cmath.sinh(z)"));
            }
            let (r, i) = to_complex(&args[0])?;
            Ok(Value::Complex(r.sinh() * i.cos(), r.cosh() * i.sin()))
        }
        "tanh" => {
            if args.len() != 1 {
                return Err(err("cmath.tanh(z)"));
            }
            let (r, i) = to_complex(&args[0])?;
            let denom = r.cosh() * 2.0 + i.cos() * 2.0;
            if denom == 0.0 {
                return Ok(Value::Complex(std::f64::NAN, std::f64::NAN));
            }
            Ok(Value::Complex(
                r.sinh() * 2.0 / denom,
                i.sin() * 2.0 / denom,
            ))
        }
        "acosh" => {
            if args.len() != 1 {
                return Err(err("cmath.acosh(z)"));
            }
            let (r, i) = to_complex(&args[0])?;
            // acosh(z) = log(z + sqrt(z^2 - 1))
            let z_sq_r = r * r - i * i;
            let z_sq_i = 2.0 * r * i;
            let (sr, si) = complex_sqrt(z_sq_r - 1.0, z_sq_i);
            let (lr, li) = complex_log(r + sr, i + si);
            Ok(Value::Complex(lr, li))
        }
        "asinh" => {
            if args.len() != 1 {
                return Err(err("cmath.asinh(z)"));
            }
            let (r, i) = to_complex(&args[0])?;
            // asinh(z) = log(z + sqrt(z^2 + 1))
            let z_sq_r = r * r - i * i;
            let z_sq_i = 2.0 * r * i;
            let (sr, si) = complex_sqrt(z_sq_r + 1.0, z_sq_i);
            let (lr, li) = complex_log(r + sr, i + si);
            Ok(Value::Complex(lr, li))
        }
        "atanh" => {
            if args.len() != 1 {
                return Err(err("cmath.atanh(z)"));
            }
            let (r, i) = to_complex(&args[0])?;
            // atanh(z) = 0.5 * (log(1 + z) - log(1 - z))
            let (lr1, li1) = complex_log(1.0 + r, i);
            let (lr2, li2) = complex_log(1.0 - r, -i);
            Ok(Value::Complex(0.5 * (lr1 - lr2), 0.5 * (li1 - li2)))
        }
        "exp" => {
            if args.len() != 1 {
                return Err(err("cmath.exp(z)"));
            }
            let (r, i) = to_complex(&args[0])?;
            Ok(Value::Complex(r.exp() * i.cos(), r.exp() * i.sin()))
        }
        "log" => {
            if args.len() < 1 {
                return Err(err("cmath.log(z, [base])"));
            }
            let (r, i) = to_complex(&args[0])?;
            let (mut lr, mut li) = complex_log(r, i);
            if args.len() > 1 {
                let base = as_f64(&args[1]).ok_or_else(|| err("base must be number"))?;
                let denom = base.ln();
                lr /= denom;
                li /= denom;
            }
            Ok(Value::Complex(lr, li))
        }
        "log10" => {
            if args.len() != 1 {
                return Err(err("cmath.log10(z)"));
            }
            let (r, i) = to_complex(&args[0])?;
            let (mut lr, mut li) = complex_log(r, i);
            let denom = 10.0f64.ln();
            lr /= denom;
            li /= denom;
            Ok(Value::Complex(lr, li))
        }
        "sqrt" => {
            if args.len() != 1 {
                return Err(err("cmath.sqrt(z)"));
            }
            let (r, i) = to_complex(&args[0])?;
            let (sr, si) = complex_sqrt(r, i);
            Ok(Value::Complex(sr, si))
        }
        "phase" => {
            if args.len() != 1 {
                return Err(err("cmath.phase(z)"));
            }
            let (r, i) = to_complex(&args[0])?;
            Ok(Value::Number(i.atan2(r)))
        }
        "polar" => {
            if args.len() != 1 {
                return Err(err("cmath.polar(z)"));
            }
            let (r, i) = to_complex(&args[0])?;
            let rad = (r * r + i * i).sqrt();
            let phi = i.atan2(r);
            Ok(Value::Tuple(vec![Value::Number(rad), Value::Number(phi)]))
        }
        "rect" => {
            if args.len() != 2 {
                return Err(err("cmath.rect(r, phi)"));
            }
            let r = as_f64(&args[0]).ok_or_else(|| err("r must be number"))?;
            let phi = as_f64(&args[1]).ok_or_else(|| err("phi must be number"))?;
            Ok(Value::Complex(r * phi.cos(), r * phi.sin()))
        }
        "isfinite" | "isFinite" => {
            if args.len() != 1 {
                return Err(err("cmath.isfinite(z)"));
            }
            let (r, i) = to_complex(&args[0])?;
            Ok(Value::Bool(r.is_finite() && i.is_finite()))
        }
        "isinf" | "isInf" => {
            if args.len() != 1 {
                return Err(err("cmath.isinf(z)"));
            }
            let (r, i) = to_complex(&args[0])?;
            Ok(Value::Bool(r.is_infinite() || i.is_infinite()))
        }
        "isnan" | "isNaN" => {
            if args.len() != 1 {
                return Err(err("cmath.isnan(z)"));
            }
            let (r, i) = to_complex(&args[0])?;
            Ok(Value::Bool(r.is_nan() || i.is_nan()))
        }
        "isclose" => {
            if args.len() < 2 {
                return Err(err("cmath.isclose(a, b, [rel_tol], [abs_tol])"));
            }
            let (ar, ai) = to_complex(&args[0])?;
            let (br, bi) = to_complex(&args[1])?;
            let rel_tol = if args.len() > 2 {
                as_f64(&args[2]).unwrap_or(1e-09)
            } else {
                1e-09
            };
            let abs_tol = if args.len() > 3 {
                as_f64(&args[3]).unwrap_or(0.0)
            } else {
                0.0
            };
            let diff_sq = (ar - br) * (ar - br) + (ai - bi) * (ai - bi);
            let a_mag = (ar * ar + ai * ai).sqrt();
            let b_mag = (br * br + bi * bi).sqrt();
            let limit = rel_tol * f64::max(a_mag, b_mag);
            let limit = f64::max(limit, abs_tol);
            Ok(Value::Bool(diff_sq.sqrt() <= limit))
        }
        _ => Err(format!("Unknown cmath method: {}", method_name)),
    }
}

fn complex_sqrt(r: f64, i: f64) -> (f64, f64) {
    let mag = (r * r + i * i).sqrt();
    let phi = i.atan2(r);
    (
        mag.sqrt() * (phi / 2.0).cos(),
        mag.sqrt() * (phi / 2.0).sin(),
    )
}

fn complex_log(r: f64, i: f64) -> (f64, f64) {
    let mag = (r * r + i * i).sqrt();
    let phi = i.atan2(r);
    (mag.ln(), phi)
}

fn gcd(mut a: i64, mut b: i64) -> i64 {
    while b != 0 {
        let temp = b;
        b = a % b;
        a = temp;
    }
    a.abs()
}

fn lcm(a: i64, b: i64) -> i64 {
    if a == 0 || b == 0 {
        return 0;
    }
    (a * b).abs() / gcd(a, b)
}

fn factorial(n: i64) -> i64 {
    if n < 0 {
        return 0;
    }
    let mut res = 1;
    for i in 1..=n {
        res *= i;
    }
    res
}

fn comb(n: i64, k: i64) -> f64 {
    if k < 0 || k > n {
        return 0.0;
    }
    let k = k.min(n - k);
    let mut res = 1.0;
    for i in 1..=k {
        res = res * (n - k + i) as f64 / i as f64;
    }
    res.round()
}

fn perm(n: i64, k: i64) -> f64 {
    if k < 0 || k > n {
        return 0.0;
    }
    let mut res = 1.0;
    for i in (n - k + 1)..=n {
        res *= i as f64;
    }
    res.round()
}

fn isqrt(n: i64) -> i64 {
    if n < 0 {
        return 0;
    }
    (n as f64).sqrt() as i64
}

fn remainder(x: f64, y: f64) -> f64 {
    if y == 0.0 || x.is_infinite() || y.is_nan() {
        return std::f64::NAN;
    }
    let q = (x / y).round();
    x - q * y
}

fn frexp(x: f64) -> (f64, i32) {
    if x == 0.0 || !x.is_finite() {
        (x, 0)
    } else {
        let lg = x.abs().log2().floor() as i32 + 1;
        let mantissa = x / 2.0f64.powi(lg);
        (mantissa, lg)
    }
}

fn nextafter(x: f64, y: f64) -> f64 {
    if x == y {
        return y;
    }
    let bits = x.to_bits();
    if y > x {
        if x >= 0.0 {
            f64::from_bits(bits + 1)
        } else {
            f64::from_bits(bits - 1)
        }
    } else {
        if x > 0.0 {
            f64::from_bits(bits - 1)
        } else if x < 0.0 {
            f64::from_bits(bits + 1)
        } else {
            -f64::from_bits(1)
        }
    }
}

fn ulp(x: f64) -> f64 {
    if !x.is_finite() {
        if x.is_nan() { x } else { std::f64::INFINITY }
    } else {
        let x_abs = x.abs();
        let next = nextafter(x_abs, std::f64::INFINITY);
        next - x_abs
    }
}

fn val_to_vec_f64(v: &Value) -> Result<Vec<f64>, String> {
    match v {
        Value::Array(arr) => {
            let mut res = Vec::new();
            for item in arr.iter() {
                res.push(
                    as_f64(item).ok_or_else(|| "iterable elements must be numbers".to_string())?,
                );
            }
            Ok(res)
        }
        _ => Err("Expected array/iterable".to_string()),
    }
}

fn dist(p: &[f64], q: &[f64]) -> f64 {
    let mut sum = 0.0;
    for (pi, qi) in p.iter().zip(q.iter()) {
        sum += (pi - qi) * (pi - qi);
    }
    sum.sqrt()
}

fn fsum(arr: &[f64]) -> f64 {
    let mut sum = 0.0;
    let mut c = 0.0;
    for &x in arr {
        let y = x - c;
        let t = sum + y;
        c = (t - sum) - y;
        sum = t;
    }
    sum
}

fn prod(arr: &[f64], start: f64) -> f64 {
    let mut p = start;
    for &x in arr {
        p *= x;
    }
    p
}

fn sumprod(p: &[f64], q: &[f64]) -> f64 {
    let mut sum = 0.0;
    for (pi, qi) in p.iter().zip(q.iter()) {
        sum += pi * qi;
    }
    sum
}

fn erf(x: f64) -> f64 {
    if x < 0.0 {
        return -erf(-x);
    }
    let p = 0.3275911;
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;
    let t = 1.0 / (1.0 + p * x);
    let y = 1.0 - ((((a5 * t + a4) * t + a3) * t + a2) * t + a1) * t * (-x * x).exp();
    y
}

fn gamma(x: f64) -> f64 {
    if x < 0.5 {
        std::f64::consts::PI / ((std::f64::consts::PI * x).sin() * gamma(1.0 - x))
    } else {
        let x = x - 1.0;
        let p = [
            0.99999999999980993,
            676.5203681218851,
            -1259.1392167224028,
            771.32342877765313,
            -176.61502916214059,
            12.507343278686905,
            -0.13857109526572012,
            9.9843695780195716e-6,
            1.505632730049959e-7,
        ];
        let mut y = p[0];
        for i in 1..9 {
            y += p[i] / (x + i as f64);
        }
        let t = x + 7.5;
        (2.0 * std::f64::consts::PI).sqrt() * t.powf(x + 0.5) * (-t).exp() * y
    }
}
