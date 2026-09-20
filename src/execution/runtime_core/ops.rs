use super::format::fmt;
use super::set_prop;
use crate::parsing::ast::TokenKind;
use crate::parsing::ast::Value;

pub fn num(v: Value) -> Result<f64, String> {
    match v {
        Value::Number(n) => Ok(n),
        // Fixed-width integer types (unsigned)
        Value::U8(n) => Ok(n as f64),
        Value::U16(n) => Ok(n as f64),
        Value::U32(n) => Ok(n as f64),
        Value::U64(n) => Ok(n as f64),
        Value::U128(n) => Ok(n as f64),
        // Fixed-width integer types (signed)
        Value::I8(n) => Ok(n as f64),
        Value::I16(n) => Ok(n as f64),
        Value::I32(n) => Ok(n as f64),
        Value::I64(n) => Ok(n as f64),
        Value::I128(n) => Ok(n as f64),
        // Fixed-width float types
        Value::F32(n) => Ok(n as f64),
        Value::F64(n) => Ok(n),
        Value::Instance(inst) => {
            if let Some(val) = inst.get_field("_val") {
                num(val)
            } else if let Some(val) = inst.get_field("value") {
                num(val)
            } else {
                Err("Expected number".to_string())
            }
        }
        Value::Ref(inner, _) => num((*inner).clone()),
        _ => Err(format!("Expected number, got: {:?}", v)),
    }
}

/// Convert any numeric Value to i64 for bitwise/shift operations
pub fn as_i64(v: &Value) -> Result<i64, String> {
    match v {
        Value::Number(n) => Ok(*n as i64),
        Value::BigInt(b) => Ok(b.try_into().map_err(|_| "BigInt too large for i64")?),
        Value::U8(n) => Ok(*n as i64),
        Value::U16(n) => Ok(*n as i64),
        Value::U32(n) => Ok(*n as i64),
        Value::U64(n) => Ok(*n as i64),
        Value::U128(n) => Ok(*n as i64),
        Value::I8(n) => Ok(*n as i64),
        Value::I16(n) => Ok(*n as i64),
        Value::I32(n) => Ok(*n as i64),
        Value::I64(n) => Ok(*n),
        Value::I128(n) => Ok(*n as i64),
        Value::F32(n) => Ok(*n as i64),
        Value::F64(n) => Ok(*n as i64),
        _ => Err("Expected numeric value".to_string()),
    }
}

#[allow(dead_code)]
pub fn big(v: Value) -> Result<num_bigint::BigInt, String> {
    if let Value::BigInt(bi) = v {
        Ok(bi)
    } else {
        Err("Expected bigint".to_string())
    }
}

pub fn promote_to_big(v: Value) -> Result<num_bigint::BigInt, String> {
    match v {
        Value::BigInt(bi) => Ok(bi),
        Value::Number(n) => {
            if (n - n.trunc()).abs() < 1e-12 {
                Ok(num_bigint::BigInt::from(n as i128))
            } else {
                Err("Cannot mix BigInt with non-integer number".to_string())
            }
        }
        Value::I8(n) => Ok(num_bigint::BigInt::from(n)),
        Value::I16(n) => Ok(num_bigint::BigInt::from(n)),
        Value::I32(n) => Ok(num_bigint::BigInt::from(n)),
        Value::I64(n) => Ok(num_bigint::BigInt::from(n)),
        Value::I128(n) => Ok(num_bigint::BigInt::from(n)),
        Value::U8(n) => Ok(num_bigint::BigInt::from(n)),
        Value::U16(n) => Ok(num_bigint::BigInt::from(n)),
        Value::U32(n) => Ok(num_bigint::BigInt::from(n)),
        Value::U64(n) => Ok(num_bigint::BigInt::from(n)),
        Value::U128(n) => Ok(num_bigint::BigInt::from(n)),
        Value::F32(n) => {
            if (n - n.trunc()).abs() < 1e-12 {
                Ok(num_bigint::BigInt::from(n as i128))
            } else {
                Err("Cannot mix BigInt with non-integer number".to_string())
            }
        }
        Value::F64(n) => {
            if (n - n.trunc()).abs() < 1e-12 {
                Ok(num_bigint::BigInt::from(n as i128))
            } else {
                Err("Cannot mix BigInt with non-integer number".to_string())
            }
        }
        _ => Err("Expected numeric value".to_string()),
    }
}

/// Fast binary numeric operation with short-circuit for Number+Number case
/// This is the hot path for arithmetic - avoid num() calls when possible
#[inline(always)]
pub fn bin_num(a: Value, b: Value, f: fn(f64, f64) -> f64) -> Result<Value, String> {
    // FAST PATH: Both are Value::Number - avoid num() entirely
    // This is the most common case in arithmetic loops
    if let (Value::Number(x), Value::Number(y)) = (&a, &b) {
        return Ok(Value::Number(f(*x, *y)));
    }
    // FALLBACK: One or both values need conversion
    Ok(Value::Number(f(num(a)?, num(b)?)))
}

pub fn cmp_num(a: Value, b: Value, f: fn(f64, f64) -> bool) -> Result<Value, String> {
    Ok(Value::Bool(f(num(a)?, num(b)?)))
}

/// Extract a valid non-negative `usize` index from any integer-like Value
/// without going through `f64`, preventing precision loss for large integers.
pub fn to_index(v: &Value) -> Result<usize, String> {
    match v {
        Value::Ref(inner, _) => to_index(inner),
        Value::U8(n) => Ok(*n as usize),
        Value::U16(n) => Ok(*n as usize),
        Value::U32(n) => Ok(*n as usize),
        Value::U64(n) => {
            usize::try_from(*n).map_err(|_| "index exceeds usize addressable range".to_string())
        }
        Value::U128(n) => {
            usize::try_from(*n).map_err(|_| "index exceeds usize addressable range".to_string())
        }
        Value::I8(n) => {
            if *n >= 0 {
                Ok(*n as usize)
            } else {
                Err("negative index not supported".to_string())
            }
        }
        Value::I16(n) => {
            if *n >= 0 {
                Ok(*n as usize)
            } else {
                Err("negative index not supported".to_string())
            }
        }
        Value::I32(n) => {
            if *n >= 0 {
                Ok(*n as usize)
            } else {
                Err("negative index not supported".to_string())
            }
        }
        Value::I64(n) => {
            if *n >= 0 {
                usize::try_from(*n).map_err(|_| "index exceeds usize range".to_string())
            } else {
                Err("negative index not supported".to_string())
            }
        }
        Value::I128(n) => {
            if *n >= 0 {
                usize::try_from(*n).map_err(|_| "index exceeds usize range".to_string())
            } else {
                Err("negative index not supported".to_string())
            }
        }
        Value::BigInt(b) => {
            use num_traits::ToPrimitive;
            if b.sign() == num_bigint::Sign::Minus {
                Err("negative index not supported".to_string())
            } else if let Some(idx) = b.to_usize() {
                Ok(idx)
            } else {
                Err("index too large for addressable memory".to_string())
            }
        }
        Value::Number(n) | Value::F64(n) => {
            if n.is_nan() || n.is_infinite() {
                Err("index cannot be NaN or Infinity".to_string())
            } else if *n < 0.0 {
                Err("negative index not supported".to_string())
            } else if (n - n.trunc()).abs() > 1e-12 {
                Err("index must be an integer".to_string())
            } else {
                Ok(*n as usize)
            }
        }
        Value::F32(n) => {
            let n = *n as f64;
            if n.is_nan() || n.is_infinite() {
                Err("index cannot be NaN or Infinity".to_string())
            } else if n < 0.0 {
                Err("negative index not supported".to_string())
            } else if (n - n.trunc()).abs() > 1e-6 {
                Err("index must be an integer".to_string())
            } else {
                Ok(n as usize)
            }
        }
        _ => Err("index must be a non-negative integer".to_string()),
    }
}

pub fn equals(a: &Value, b: &Value) -> bool {
    use crate::parsing::ast::Value::*;

    // Unwrap shared borrow references first
    if let Value::Ref(inner, _) = a {
        return equals(inner, b);
    }
    if let Value::Ref(inner, _) = b {
        return equals(a, inner);
    }

    // Helper: Exact BigInt conversion for integers and integral floats
    fn to_bigint_opt(v: &Value) -> Option<num_bigint::BigInt> {
        match v {
            BigInt(bi) => Some(bi.clone()),
            I8(n) => Some(num_bigint::BigInt::from(*n)),
            I16(n) => Some(num_bigint::BigInt::from(*n)),
            I32(n) => Some(num_bigint::BigInt::from(*n)),
            I64(n) => Some(num_bigint::BigInt::from(*n)),
            I128(n) => Some(num_bigint::BigInt::from(*n)),
            U8(n) => Some(num_bigint::BigInt::from(*n)),
            U16(n) => Some(num_bigint::BigInt::from(*n)),
            U32(n) => Some(num_bigint::BigInt::from(*n)),
            U64(n) => Some(num_bigint::BigInt::from(*n)),
            U128(n) => Some(num_bigint::BigInt::from(*n)),
            Number(n) | F64(n) => {
                if n.is_finite() && (n - n.trunc()).abs() < 1e-12 {
                    Some(num_bigint::BigInt::from(*n as i128))
                } else {
                    None
                }
            }
            F32(n) => {
                let n = *n as f64;
                if n.is_finite() && (n - n.trunc()).abs() < 1e-6 {
                    Some(num_bigint::BigInt::from(n as i128))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    // Helper to extract numeric value as f64 for comparison
    fn as_f64_opt(v: &Value) -> Option<f64> {
        match v {
            Number(n) => Some(*n),
            I8(n) => Some(*n as f64),
            I16(n) => Some(*n as f64),
            I32(n) => Some(*n as f64),
            I64(n) => Some(*n as f64),
            I128(n) => Some(*n as f64),
            U8(n) => Some(*n as f64),
            U16(n) => Some(*n as f64),
            U32(n) => Some(*n as f64),
            U64(n) => Some(*n as f64),
            U128(n) => Some(*n as f64),
            F32(n) => Some(*n as f64),
            F64(n) => Some(*n),
            _ => None,
        }
    }

    // Check if either is a BigInt
    if matches!(a, BigInt(_)) || matches!(b, BigInt(_)) {
        if let (Some(x), Some(y)) = (to_bigint_opt(a), to_bigint_opt(b)) {
            return x == y;
        }
        return false;
    }

    // Try numeric comparison across standard numeric types
    if let (Some(x), Some(y)) = (as_f64_opt(a), as_f64_opt(b)) {
        if x.is_nan() || y.is_nan() {
            return false;
        }
        return (x - y).abs() < 1e-12;
    }

    match (a, b) {
        (Bool(x), Bool(y)) => x == y,
        (Char(x), Char(y)) => x == y,
        (Char(x), Str(y)) => y.len() == 1 && y.chars().next() == Some(*x),
        (Str(x), Char(y)) => x.len() == 1 && x.chars().next() == Some(*y),
        (Str(x), Str(y)) => std::ptr::eq(x.as_str(), y.as_str()) || x == y,
        (Null, Null) => true,
        (Array(x), Array(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(a, b)| equals(a, b))
        }
        (DynArray(x), DynArray(y)) => {
            x.data.len() == y.data.len()
                && x.data.iter().zip(y.data.iter()).all(|(a, b)| equals(a, b))
        }
        (RawArray(_, x), RawArray(_, y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(a, b)| equals(a, b))
        }
        (Tuple(x), Tuple(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(xi, yi)| equals(xi, yi))
        }
        (Set(x), Set(y)) => {
            if x.len() != y.len() {
                return false;
            }
            x.iter().all(|xi| y.iter().any(|yi| equals(xi, yi)))
        }
        (Object(x), Object(y)) => {
            if x.len() != y.len() {
                return false;
            }
            x.iter()
                .all(|(k, v)| y.get(k).map_or(false, |yv| equals(v, yv)))
        }
        (Complex(ar, ai), Complex(br, bi)) => (ar - br).abs() < 1e-12 && (ai - bi).abs() < 1e-12,
        (Share(x), Share(y)) => x.ptr == y.ptr,
        (Weak(x), Weak(y)) => x.ptr == y.ptr,
        _ => false,
    }
}

pub fn strict_equals(a: &Value, b: &Value) -> bool {
    use crate::parsing::ast::Value::*;
    match (a, b) {
        (Number(x), Number(y)) => (x - y).abs() < 1e-12,
        (BigInt(x), BigInt(y)) => x == y,
        (Bool(x), Bool(y)) => x == y,
        (Char(x), Char(y)) => x == y,
        (Str(x), Str(y)) => x == y,
        (Null, Null) => true,
        (Array(x), Array(y)) => std::ptr::eq(x, y),
        (Tuple(x), Tuple(y)) => std::ptr::eq(x, y),
        (Set(x), Set(y)) => std::ptr::eq(x, y),
        (Object(x), Object(y)) => std::sync::Arc::ptr_eq(x, y),
        (Class(cx), Class(cy)) => cx.name == cy.name,
        (Instance(ix), Instance(iy)) => ix.class.name == iy.class.name && std::ptr::eq(ix, iy),
        (Enum(ax), Enum(ay)) => ax.name == ay.name,
        (EnumCtor(ax, vx), EnumCtor(ay, vy)) => ax.name == ay.name && vx == vy,
        (Function(_), Function(_)) => false,
        (UserFunction(_), UserFunction(_)) => false,
        (BoundMethod(_, _), BoundMethod(_, _)) => false,
        (Share(x), Share(y)) => x.ptr == y.ptr,
        (Weak(x), Weak(y)) => x.ptr == y.ptr,
        _ => false,
    }
}

pub fn set_index_prop(_env: usize, obj: Value, idx: Value, val: Value) -> Result<Value, String> {
    use crate::parsing::ast::Value::*;
    match (obj, idx) {
        (Object(marc), Str(k)) => {
            let mut m = (*marc).clone();
            m.insert(k, val);
            let v = Object(std::sync::Arc::new(m));
            Ok(v)
        }
        (Array(mut a), idx) => {
            let i = to_index(&idx)?;
            if i < a.len() {
                a[i] = val;
                Ok(Array(a))
            } else {
                Err("index out of bounds".to_string())
            }
        }
        (DynArray(da), idx) => {
            let i = to_index(&idx)?;
            if i < da.data.len() {
                let mut new_data = da.data.clone();
                // Coerce the value to match the array's element type
                let num_val = match &val {
                    Value::Number(n) => Some(*n),
                    _ => None,
                };
                let coerced_val = if let Some(num_val) = num_val {
                    match da.concrete_type.to_lowercase().as_str() {
                        "i8" => {
                            if num_val >= i8::MIN as f64 && num_val <= i8::MAX as f64 {
                                Value::I8(num_val as i8)
                            } else {
                                return Err(format!("{} out of range for i8", num_val));
                            }
                        }
                        "i16" => {
                            if num_val >= i16::MIN as f64 && num_val <= i16::MAX as f64 {
                                Value::I16(num_val as i16)
                            } else {
                                return Err(format!("{} out of range for i16", num_val));
                            }
                        }
                        "i32" => {
                            if num_val >= i32::MIN as f64 && num_val <= i32::MAX as f64 {
                                Value::I32(num_val as i32)
                            } else {
                                return Err(format!("{} out of range for i32", num_val));
                            }
                        }
                        "i64" => {
                            if num_val >= i64::MIN as f64 && num_val <= i64::MAX as f64 {
                                Value::I64(num_val as i64)
                            } else {
                                return Err(format!("{} out of range for i64", num_val));
                            }
                        }
                        "i128" => Value::I128(num_val as i128),
                        "u8" => {
                            if num_val >= 0.0 && num_val <= u8::MAX as f64 {
                                Value::U8(num_val as u8)
                            } else {
                                return Err(format!("{} out of range for u8", num_val));
                            }
                        }
                        "u16" => {
                            if num_val >= 0.0 && num_val <= u16::MAX as f64 {
                                Value::U16(num_val as u16)
                            } else {
                                return Err(format!("{} out of range for u16", num_val));
                            }
                        }
                        "u32" => {
                            if num_val >= 0.0 && num_val <= u32::MAX as f64 {
                                Value::U32(num_val as u32)
                            } else {
                                return Err(format!("{} out of range for u32", num_val));
                            }
                        }
                        "u64" => {
                            if num_val >= 0.0 && num_val <= u64::MAX as f64 {
                                Value::U64(num_val as u64)
                            } else {
                                return Err(format!("{} out of range for u64", num_val));
                            }
                        }
                        "u128" => Value::U128(num_val as u128),
                        "f32" => Value::F32(num_val as f32),
                        "f64" => Value::F64(num_val),
                        _ => val,
                    }
                } else {
                    val
                };
                new_data[i] = coerced_val;
                Ok(DynArray(Box::new(crate::parsing::ast::DynamicArray {
                    data: new_data,
                    element_type: da.element_type.clone(),
                    concrete_type: da.concrete_type.clone(),
                    tracked_capacity: da.tracked_capacity,
                })))
            } else {
                Err("index out of bounds".to_string())
            }
        }
        (RawArray(t, mut a), idx_val) => {
            let i = match idx_val {
                Number(n) | F64(n) => {
                    if (n - n.trunc()).abs() > 1e-12 {
                        return Err("index must be integer".to_string());
                    }
                    if n < 0.0 {
                        return Err("negative index".to_string());
                    }
                    n as usize
                }
                F32(n) => {
                    if (n - n.trunc()).abs() > 1e-6 {
                        return Err("index must be integer".to_string());
                    }
                    if n < 0.0 {
                        return Err("negative index".to_string());
                    }
                    n as usize
                }
                U8(n) => n as usize,
                U16(n) => n as usize,
                U32(n) => n as usize,
                U64(n) => n as usize,
                U128(n) => n as usize,
                I8(n) => {
                    if n < 0 {
                        return Err("negative index".to_string());
                    }
                    n as usize
                }
                I16(n) => {
                    if n < 0 {
                        return Err("negative index".to_string());
                    }
                    n as usize
                }
                I32(n) => {
                    if n < 0 {
                        return Err("negative index".to_string());
                    }
                    n as usize
                }
                I64(n) => {
                    if n < 0 {
                        return Err("negative index".to_string());
                    }
                    n as usize
                }
                I128(n) => {
                    if n < 0 {
                        return Err("negative index".to_string());
                    }
                    n as usize
                }
                _ => return Err("index must be integer".to_string()),
            };
            if i < a.len() {
                // Coerce the value to match the array's element type if it's a Number
                let coerced_val = if let Value::Number(num_val) = val {
                    match t.to_lowercase().as_str() {
                        "i8" => {
                            if num_val >= i8::MIN as f64 && num_val <= i8::MAX as f64 {
                                Value::I8(num_val as i8)
                            } else {
                                return Err(format!("{} out of range for i8", num_val));
                            }
                        }
                        "i16" => {
                            if num_val >= i16::MIN as f64 && num_val <= i16::MAX as f64 {
                                Value::I16(num_val as i16)
                            } else {
                                return Err(format!("{} out of range for i16", num_val));
                            }
                        }
                        "i32" => {
                            if num_val >= i32::MIN as f64 && num_val <= i32::MAX as f64 {
                                Value::I32(num_val as i32)
                            } else {
                                return Err(format!("{} out of range for i32", num_val));
                            }
                        }
                        "i64" => {
                            if num_val >= i64::MIN as f64 && num_val <= i64::MAX as f64 {
                                Value::I64(num_val as i64)
                            } else {
                                return Err(format!("{} out of range for i64", num_val));
                            }
                        }
                        "i128" => Value::I128(num_val as i128),
                        "u8" => {
                            if num_val >= 0.0 && num_val <= u8::MAX as f64 {
                                Value::U8(num_val as u8)
                            } else {
                                return Err(format!("{} out of range for u8", num_val));
                            }
                        }
                        "u16" => {
                            if num_val >= 0.0 && num_val <= u16::MAX as f64 {
                                Value::U16(num_val as u16)
                            } else {
                                return Err(format!("{} out of range for u16", num_val));
                            }
                        }
                        "u32" => {
                            if num_val >= 0.0 && num_val <= u32::MAX as f64 {
                                Value::U32(num_val as u32)
                            } else {
                                return Err(format!("{} out of range for u32", num_val));
                            }
                        }
                        "u64" => {
                            if num_val >= 0.0 && num_val <= u64::MAX as f64 {
                                Value::U64(num_val as u64)
                            } else {
                                return Err(format!("{} out of range for u64", num_val));
                            }
                        }
                        "u128" => Value::U128(num_val as u128),
                        "f32" => Value::F32(num_val as f32),
                        "f64" => Value::F64(num_val),
                        _ => val,
                    }
                } else {
                    val
                };
                a[i] = coerced_val;
                Ok(RawArray(t, a))
            } else {
                Err("index out of bounds".to_string())
            }
        }
        (Value::I64(ptr), idx_val) => {
            if !crate::execution::runtime_core::in_unsafe_context() {
                return Err("pointer store requires unsafe { ... } block".to_string());
            }
            let ptr_u64 = ptr as u64;
            let offset = match idx_val {
                Value::Number(n) => {
                    if (n - n.trunc()).abs() > 1e-12 || n < 0.0 {
                        return Err("index must be integer and non-negative".to_string());
                    }
                    n as usize
                }
                Value::I64(n) if n >= 0 => n as usize,
                Value::U64(n) => n as usize,
                Value::U8(n) => n as usize,
                Value::U16(n) => n as usize,
                Value::U32(n) => n as usize,
                _ => return Err("index must be integer and non-negative".to_string()),
            };

            let elem_size = crate::backends::unsafe_heap::elem_size_of_ptr(ptr_u64)?;
            if elem_size == 1 {
                let byte_val: u8 = match val {
                    Value::Number(n) => {
                        if (n - n.trunc()).abs() > 1e-12 || n < 0.0 || n > 255.0 {
                            return Err("value must be 0..255".to_string());
                        }
                        n as u8
                    }
                    Value::I64(n) => {
                        if n < 0 || n > 255 {
                            return Err("value must be 0..255".to_string());
                        }
                        n as u8
                    }
                    Value::U64(n) => {
                        if n > 255 {
                            return Err("value must be 0..255".to_string());
                        }
                        n as u8
                    }
                    Value::U8(b) => b,
                    Value::I8(b) => b as u8,
                    _ => return Err("value must be byte".to_string()),
                };
                crate::backends::unsafe_heap::store_u8(ptr_u64, offset, byte_val)?;
            } else {
                let bytes: Vec<u8> = match elem_size {
                    2 => {
                        let v = as_i64(&val)? as i16;
                        v.to_le_bytes().to_vec()
                    }
                    4 => {
                        let v = as_i64(&val)? as i32;
                        v.to_le_bytes().to_vec()
                    }
                    8 => {
                        let v = as_i64(&val)?;
                        v.to_le_bytes().to_vec()
                    }
                    _ => {
                        let v = as_i64(&val)? as u8;
                        vec![v]
                    }
                };
                crate::backends::unsafe_heap::store_typed(ptr_u64, offset, &bytes)?;
            }
            Ok(Value::I64(ptr))
        }
        (Value::U64(ptr), idx_val) => {
            if !crate::execution::runtime_core::in_unsafe_context() {
                return Err("pointer store requires unsafe { ... } block".to_string());
            }
            let ptr_u64 = ptr;
            let offset = match idx_val {
                Value::Number(n) => {
                    if (n - n.trunc()).abs() > 1e-12 || n < 0.0 {
                        return Err("index must be integer and non-negative".to_string());
                    }
                    n as usize
                }
                Value::I64(n) if n >= 0 => n as usize,
                Value::U64(n) => n as usize,
                Value::U8(n) => n as usize,
                Value::U16(n) => n as usize,
                Value::U32(n) => n as usize,
                _ => return Err("index must be integer and non-negative".to_string()),
            };

            let elem_size = crate::backends::unsafe_heap::elem_size_of_ptr(ptr_u64)?;
            if elem_size == 1 {
                let byte_val: u8 = match val {
                    Value::Number(n) => {
                        if (n - n.trunc()).abs() > 1e-12 || n < 0.0 || n > 255.0 {
                            return Err("value must be 0..255".to_string());
                        }
                        n as u8
                    }
                    Value::I64(n) => {
                        if n < 0 || n > 255 {
                            return Err("value must be 0..255".to_string());
                        }
                        n as u8
                    }
                    Value::U64(n) => {
                        if n > 255 {
                            return Err("value must be 0..255".to_string());
                        }
                        n as u8
                    }
                    Value::U8(b) => b,
                    Value::I8(b) => b as u8,
                    _ => return Err("value must be byte".to_string()),
                };
                crate::backends::unsafe_heap::store_u8(ptr_u64, offset, byte_val)?;
            } else {
                let bytes: Vec<u8> = match elem_size {
                    2 => {
                        let v = as_i64(&val)? as i16;
                        v.to_le_bytes().to_vec()
                    }
                    4 => {
                        let v = as_i64(&val)? as i32;
                        v.to_le_bytes().to_vec()
                    }
                    8 => {
                        let v = as_i64(&val)?;
                        v.to_le_bytes().to_vec()
                    }
                    _ => {
                        let v = as_i64(&val)? as u8;
                        vec![v]
                    }
                };
                crate::backends::unsafe_heap::store_typed(ptr_u64, offset, &bytes)?;
            }
            Ok(Value::U64(ptr_u64))
        }
        (Instance(inst), idx) => {
            if let Some(fns) = inst.class.methods.get("operator[]=") {
                if let Some(sel) = fns.first() {
                    use crate::execution::runtime_core::interpreter_core::call_user_with_this;
                    let (_res, updated) =
                        call_user_with_this(sel.clone(), vec![idx, val], inst.clone(), None, None)?;
                    return Ok(Instance(updated));
                }
            }
            Err("class does not implement operator[]=".to_string())
        }
        _ => Err("unsupported index set".to_string()),
    }
}

pub fn set_object_prop(env: usize, mut obj: Value, key: String, val: Value) -> Result<(), String> {
    let _ = env;
    set_prop(&mut obj, &key, val, None)
}

pub fn apply_assign_op(cur: Value, op: TokenKind, rhs: Value) -> Result<Value, String> {
    // Compound assignment may read the current value through a shared borrow.
    let cur = match cur {
        Value::Ref(inner, _) => (*inner).clone(),
        other => other,
    };
    if let Some(op) = crate::runtime::abi::bitwise::BitOp::from_token(op) {
        return crate::runtime::abi::bitwise::binary(op, &cur, &rhs);
    }
    match op {
        TokenKind::Equal => Ok(rhs),
        TokenKind::PlusEqual => match (cur, rhs) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a + b)),
            (Value::BigInt(a), Value::BigInt(b)) => Ok(Value::BigInt(a + b)),
            (Value::Str(a), v) => Ok(Value::Str(format!("{}{}", a, fmt(&v)))),
            (a, Value::Str(b)) => Ok(Value::Str(format!("{}{}", fmt(&a), b))),
            _ => Err("+= type error".to_string()),
        },
        TokenKind::MinusEqual => bin_num(cur, rhs, |a, b| a - b),
        TokenKind::StarEqual => bin_num(cur, rhs, |a, b| a * b),
        TokenKind::SlashEqual => bin_num(cur, rhs, |a, b| a / b),
        TokenKind::PercentEqual => bin_num(cur, rhs, |a, b| a % b),
        TokenKind::StarStarEqual => match (cur, rhs) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a.powf(b))),
            (Value::BigInt(a), Value::BigInt(b)) => {
                use num_traits::ToPrimitive;
                let e = b.to_usize().ok_or_else(|| "bigint exponent".to_string())?;
                Ok(Value::BigInt(a.pow(e.try_into().unwrap())))
            }
            _ => Err("**= type error".to_string()),
        },
        TokenKind::NullCoalesceEqual => match cur {
            Value::Null => Ok(rhs),
            _ => Ok(cur),
        },
        _ => Ok(rhs),
    }
}
