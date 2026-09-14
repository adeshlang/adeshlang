//! Associated methods for core value types.
//!
//! These helpers deliberately use receiver-first dispatch.  They are called by
//! the interpreter after evaluating `value.method(...)`; they are not exposed
//! as free functions in the language.

use super::super::super::format::fmt;
use super::super::super::interpreter::err;
use super::super::super::ops::equals;
use crate::parsing::ast::Value;
use num_traits::ToPrimitive;
use rustc_hash::FxHashMap as HashMap;

fn number(v: &Value) -> Option<f64> {
    v.as_f64()
}

/// Convert a Value to a non-negative usize index.
///
/// This handles integer-typed Values (I64, U64, I32, U32, BigInt, etc.) directly
/// without going through f64, avoiding precision loss for large integer values.
/// Floats are accepted only if they are finite, whole, and non-negative.
fn integer(v: &Value) -> Result<usize, String> {
    // Fast path: use the actual integer variant when available — no f64 round-trip.
    match v {
        Value::U64(n) => {
            if let Ok(idx) = usize::try_from(*n) {
                return Ok(idx);
            }
            return Err(err("integer value exceeds usize range"));
        }
        Value::I64(n) => {
            if *n >= 0 {
                return Ok(*n as usize);
            }
            return Err(err("expected a non-negative integer"));
        }
        Value::U32(n) => return Ok(*n as usize),
        Value::I32(n) => {
            if *n >= 0 {
                return Ok(*n as usize);
            }
            return Err(err("expected a non-negative integer"));
        }
        Value::U16(n) => return Ok(*n as usize),
        Value::I16(n) => {
            if *n >= 0 {
                return Ok(*n as usize);
            }
            return Err(err("expected a non-negative integer"));
        }
        Value::U8(n) => return Ok(*n as usize),
        Value::I8(n) => {
            if *n >= 0 {
                return Ok(*n as usize);
            }
            return Err(err("expected a non-negative integer"));
        }
        Value::U128(n) => {
            if let Ok(idx) = usize::try_from(*n) {
                return Ok(idx);
            }
            return Err(err("integer value exceeds usize range"));
        }
        Value::I128(n) => {
            if *n >= 0 {
                if let Ok(idx) = usize::try_from(*n) {
                    return Ok(idx);
                }
                return Err(err("integer value exceeds usize range"));
            }
            return Err(err("expected a non-negative integer"));
        }
        Value::BigInt(bi) => {
            if let Some(idx) = bi.to_usize() {
                return Ok(idx);
            }
            return Err(err("expected a non-negative integer"));
        }
        // Fallback: Number(f64) — validate it is a finite, whole, non-negative value.
        Value::Number(n) | Value::F64(n) => {
            if !n.is_finite() || n.fract() != 0.0 || *n < 0.0 {
                return Err(err("expected a non-negative integer"));
            }
            Ok(*n as usize)
        }
        Value::F32(n) => {
            let n = *n as f64;
            if !n.is_finite() || n.fract() != 0.0 || n < 0.0 {
                return Err(err("expected a non-negative integer"));
            }
            Ok(n as usize)
        }
        _ => Err(err("expected an integer")),
    }
}

pub fn call_tuple_method(obj: &Value, name: &str, args: &[Value]) -> Result<Value, String> {
    let values = match obj {
        Value::Tuple(v) => v,
        _ => return Err(err("tuple method requires a tuple")),
    };
    match name {
        "length" | "len" => Ok(Value::Number(values.len() as f64)),
        "isEmpty" | "is_empty" => Ok(Value::Bool(values.is_empty())),
        "first" => Ok(values.first().cloned().unwrap_or(Value::Null)),
        "last" => Ok(values.last().cloned().unwrap_or(Value::Null)),
        "get" => {
            let i = integer(args.first().ok_or_else(|| err("get(index)"))?)?;
            Ok(values.get(i).cloned().unwrap_or(Value::Null))
        }
        "contains" | "includes" => {
            let v = args.first().ok_or_else(|| err("contains(value)"))?;
            Ok(Value::Bool(values.iter().any(|x| equals(x, v))))
        }
        "indexOf" => {
            let v = args.first().ok_or_else(|| err("indexOf(value)"))?;
            Ok(Value::Number(
                values
                    .iter()
                    .position(|x| equals(x, v))
                    .map(|i| i as f64)
                    .unwrap_or(-1.0),
            ))
        }
        "slice" => {
            let start = args.first().map(integer).transpose()?.unwrap_or(0);
            let end = args
                .get(1)
                .map(integer)
                .transpose()?
                .unwrap_or(values.len());
            // Normalize bounds: clamp to [0, len], then ensure start <= end.
            let len = values.len();
            let start = start.min(len);
            let end = end.min(len).max(start);
            Ok(Value::Tuple(values[start..end].to_vec()))
        }
        "toArray" => Ok(Value::Array(values.clone())),
        _ => Err(err(format!("Unknown tuple method: '{}'", name))),
    }
}

pub fn call_object_method(obj: &Value, name: &str, args: &[Value]) -> Result<Value, String> {
    let map = match obj {
        Value::Object(map) => map,
        _ => return Err(err("dictionary method requires an object")),
    };
    match name {
        "length" | "len" => Ok(Value::Number(map.len() as f64)),
        "isEmpty" | "is_empty" => Ok(Value::Bool(map.is_empty())),
        "has" | "containsKey" | "contains_key" => {
            let key = match args.first().ok_or_else(|| err("has(key)"))? {
                Value::Str(s) => s,
                other => {
                    return Err(err(format!(
                        "dictionary keys must be strings, got {}",
                        fmt(other)
                    )));
                }
            };
            Ok(Value::Bool(map.contains_key(key)))
        }
        "get" => {
            let key = match args.first().ok_or_else(|| err("get(key)"))? {
                Value::Str(s) => s,
                other => {
                    return Err(err(format!(
                        "dictionary keys must be strings, got {}",
                        fmt(other)
                    )));
                }
            };
            Ok(map.get(key).cloned().unwrap_or(Value::Null))
        }
        "getOr" => {
            let key = match args.first().ok_or_else(|| err("getOr(key, default)"))? {
                Value::Str(s) => s,
                other => {
                    return Err(err(format!(
                        "dictionary keys must be strings, got {}",
                        fmt(other)
                    )));
                }
            };
            Ok(map
                .get(key)
                .cloned()
                .or_else(|| args.get(1).cloned())
                .unwrap_or(Value::Null))
        }
        "keys" => Ok(Value::Array(map.keys().cloned().map(Value::Str).collect())),
        "values" => Ok(Value::Array(map.values().cloned().collect())),
        "entries" => Ok(Value::Array(
            map.iter()
                .map(|(k, v)| Value::Tuple(vec![Value::Str(k.clone()), v.clone()]))
                .collect(),
        )),
        "merge" => {
            let other = match args.first().ok_or_else(|| err("merge(dictionary)"))? {
                Value::Object(v) => v,
                _ => return Err(err("merge expects a dictionary")),
            };
            let mut merged: HashMap<String, Value> = (**map).clone();
            merged.extend((**other).clone());
            Ok(Value::Object(merged.into()))
        }
        "toArray" => Ok(Value::Array(map.values().cloned().collect())),
        _ => Err(err(format!("Unknown dictionary method: '{}'", name))),
    }
}

pub fn call_number_method(obj: &Value, name: &str, args: &[Value]) -> Result<Value, String> {
    let n = number(obj).ok_or_else(|| err("numeric method requires a number"))?;
    match name {
        "abs" => Ok(Value::Number(n.abs())),
        "floor" => Ok(Value::Number(n.floor())),
        "ceil" => Ok(Value::Number(n.ceil())),
        "round" => Ok(Value::Number(n.round())),
        "trunc" => Ok(Value::Number(n.trunc())),
        "fract" => Ok(Value::Number(n.fract())),
        "sqrt" => Ok(Value::Number(n.sqrt())),
        "cbrt" => Ok(Value::Number(n.cbrt())),
        "sign" => Ok(Value::Number(n.signum())),
        "isFinite" | "isfinite" => Ok(Value::Bool(n.is_finite())),
        "isInfinite" | "isinf" => Ok(Value::Bool(n.is_infinite())),
        "isNaN" | "isnan" => Ok(Value::Bool(n.is_nan())),
        "isInteger" | "is_integer" => Ok(Value::Bool(n.is_finite() && n.fract() == 0.0)),
        "isEven" | "is_even" => Ok(Value::Bool(n.fract() == 0.0 && (n as i128) % 2 == 0)),
        "isOdd" | "is_odd" => Ok(Value::Bool(n.fract() == 0.0 && (n as i128) % 2 != 0)),
        "pow" => Ok(Value::Number(
            n.powf(
                number(args.first().ok_or_else(|| err("pow(exponent)"))?)
                    .ok_or_else(|| err("exponent must be numeric"))?,
            ),
        )),
        "clamp" => {
            let min = number(args.first().ok_or_else(|| err("clamp(min, max)"))?)
                .ok_or_else(|| err("min must be numeric"))?;
            let max = number(args.get(1).ok_or_else(|| err("clamp(min, max)"))?)
                .ok_or_else(|| err("max must be numeric"))?;
            // Validate that min <= max — Rust's f64::clamp panics if min > max,
            // so we return a language-level error instead.
            if min > max {
                return Err(err(
                    "clamp(min, max): min must be less than or equal to max",
                ));
            }
            Ok(Value::Number(n.clamp(min, max)))
        }
        "toString" => Ok(Value::Str(fmt(obj))),
        _ => Err(err(format!("Unknown numeric method: '{}'", name))),
    }
}

pub fn call_complex_method(obj: &Value, name: &str, args: &[Value]) -> Result<Value, String> {
    let (real, imag) = match obj {
        Value::Complex(r, i) => (*r, *i),
        _ => return Err(err("complex method requires a complex number")),
    };
    match name {
        "real" => Ok(Value::Number(real)),
        "imag" | "imaginary" => Ok(Value::Number(imag)),
        "magnitude" | "abs" => Ok(Value::Number(real.hypot(imag))),
        "phase" | "argument" => Ok(Value::Number(imag.atan2(real))),
        "conjugate" => Ok(Value::Complex(real, -imag)),
        "toString" => Ok(Value::Str(fmt(obj))),
        "pow" => {
            let arg = args.first().ok_or_else(|| err("pow(exponent)"))?;
            let (exp_r, exp_i) = match arg {
                Value::Complex(cr, ci) => (*cr, *ci),
                other => {
                    let r =
                        number(other).ok_or_else(|| err("exponent must be numeric or complex"))?;
                    (r, 0.0)
                }
            };
            // Mathematical validation for complex power: z^w
            if real == 0.0 && imag == 0.0 {
                if exp_r == 0.0 && exp_i == 0.0 {
                    return Ok(Value::Complex(1.0, 0.0));
                } else if exp_r > 0.0 {
                    return Ok(Value::Complex(0.0, 0.0));
                } else if exp_r < 0.0 {
                    return Ok(Value::Complex(f64::INFINITY, 0.0));
                } else {
                    return Ok(Value::Complex(f64::NAN, f64::NAN));
                }
            }

            let r = real.hypot(imag);
            let theta = imag.atan2(real);
            let ln_r = r.ln();

            let log_real = exp_r * ln_r - exp_i * theta;
            let log_imag = exp_i * ln_r + exp_r * theta;

            let mag = log_real.exp();
            let res_real = mag * log_imag.cos();
            let res_imag = mag * log_imag.sin();

            Ok(Value::Complex(res_real, res_imag))
        }
        "sqrt" => {
            let r = real.hypot(imag);
            let res_r = ((r + real) / 2.0).sqrt();
            let res_i = imag.signum() * ((r - real) / 2.0).sqrt();
            Ok(Value::Complex(res_r, res_i))
        }
        "exp" => {
            let mag = real.exp();
            Ok(Value::Complex(mag * imag.cos(), mag * imag.sin()))
        }
        "ln" => {
            let r = real.hypot(imag);
            let theta = imag.atan2(real);
            Ok(Value::Complex(r.ln(), theta))
        }
        _ => Err(err(format!("Unknown complex method: '{}'", name))),
    }
}
