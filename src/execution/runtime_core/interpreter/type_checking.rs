//! Type checking and coercion utilities for the interpreter.
//!
//! Provides utilities for:
//! - Type annotation matching
//! - Fixed-width type coercion
//! - Array element type checking
//! - Automatic type inference

use crate::parsing::ast::Value;
use crate::types::type_system::type_from_name;
use crate::typesystem::checker::{Ty, is_subtype};

use num_traits::ToPrimitive;

/// Check whether a runtime `Value` conforms to an optional annotation string.
///
/// Supported annotations: int, number/float, string/str, bool, null, any
/// Nullable form: `type?` (e.g., `int?`) allows `null`.
/// Array annotations: `[T]`, `[T;N]`, `[T;raw]`, `[T;N;raw]`
fn get_numeric_value(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => Some(*n),
        Value::U8(n) => Some(*n as f64),
        Value::U16(n) => Some(*n as f64),
        Value::U32(n) => Some(*n as f64),
        Value::U64(n) => Some(*n as f64),
        Value::U128(n) => Some(*n as f64),
        Value::I8(n) => Some(*n as f64),
        Value::I16(n) => Some(*n as f64),
        Value::I32(n) => Some(*n as f64),
        Value::I64(n) => Some(*n as f64),
        Value::I128(n) => Some(*n as f64),
        Value::F32(n) => Some(*n as f64),
        Value::F64(n) => Some(*n),
        _ => None,
    }
}

pub(in crate::execution::runtime_core) fn ann_matches_value(
    ann_opt: &Option<String>,
    v: &Value,
) -> bool {
    if ann_opt.is_none() {
        return true;
    }
    let s = ann_opt.as_ref().unwrap();
    // Try to parse annotation into Ty; if successful and it's a function type,
    // perform structural check for UserFunction/BoundMethod values.
    if let Some(ty) = type_from_name(s) {
        match &ty {
            Ty::Any => return true,
            Ty::Nullable(_) => {
                if matches!(v, Value::Null) {
                    return true;
                }
            }
            Ty::Func {
                params: exp_params,
                ret: _,
            } => {
                match v {
                    Value::UserFunction(u) => {
                        // build actual Ty from function's annotated params/ret
                        let mut actual_params: Vec<Ty> = Vec::new();
                        for p in &u.params {
                            if let Some(tn) = &p.2 {
                                actual_params.push(type_from_name(tn).unwrap_or(Ty::Any));
                            } else {
                                actual_params.push(Ty::Any);
                            }
                        }
                        let actual_ret = if let Some(rt) = &u.ret_type {
                            type_from_name(rt).unwrap_or(Ty::Any)
                        } else {
                            Ty::Any
                        };
                        let actual = Ty::Func {
                            params: actual_params,
                            ret: Box::new(actual_ret),
                        };
                        return is_subtype(
                            &actual,
                            &Ty::Func {
                                params: exp_params.clone(),
                                ret: Box::new(Ty::Any),
                            },
                        );
                    }
                    Value::BoundMethod(u, _inst) => {
                        let mut actual_params: Vec<Ty> = Vec::new();
                        for p in &u.params {
                            if let Some(tn) = &p.2 {
                                actual_params.push(type_from_name(tn).unwrap_or(Ty::Any));
                            } else {
                                actual_params.push(Ty::Any);
                            }
                        }
                        let actual_ret = if let Some(rt) = &u.ret_type {
                            type_from_name(rt).unwrap_or(Ty::Any)
                        } else {
                            Ty::Any
                        };
                        let actual = Ty::Func {
                            params: actual_params,
                            ret: Box::new(actual_ret),
                        };
                        return is_subtype(
                            &actual,
                            &Ty::Func {
                                params: exp_params.clone(),
                                ret: Box::new(Ty::Any),
                            },
                        );
                    }
                    // Native functions or unknown callable values are accepted conservatively
                    _ => return true,
                }
            }
            _ => {
                // non-function types: fall back to value-based checks below
            }
        }
    }

    let s = ann_opt.as_ref().unwrap();
    let nullable = s.ends_with('?');
    let base = if nullable {
        &s[..s.len() - 1]
    } else {
        s.as_str()
    };
    use crate::parsing::ast::Value::*;
    if let Null = v {
        return nullable || base.eq_ignore_ascii_case("null") || base.eq_ignore_ascii_case("any");
    }

    // Handle array type annotations: [T], [T;N], [T;raw], [T;N;raw]
    if base.starts_with('[') && base.ends_with(']') {
        let inner = &base[1..base.len() - 1];
        let is_raw = inner.contains(";raw") || inner.ends_with(";raw");
        // Parse element type from array annotation
        let elem_type = if let Some(semicolon_pos) = inner.find(';') {
            &inner[..semicolon_pos]
        } else {
            inner
        };

        // Match either Array or RawArray based on annotation
        // Handle array-like values depending on whether the annotation expects raw layout
        if is_raw {
            // Annotation requests a raw array: only RawArray matches
            if let RawArray(_, arr) = v {
                return arr.iter().all(|elem| element_matches_type(elem, elem_type));
            }
            return false;
        } else {
            // Non-raw annotation: accept regular Array, DynArray or RawArray (structure may vary)
            if let Array(arr) = v {
                return arr.iter().all(|elem| element_matches_type(elem, elem_type));
            }
            if let DynArray(da) = v {
                return da
                    .data
                    .iter()
                    .all(|elem| element_matches_type(elem, elem_type));
            }
            if let RawArray(_, arr) = v {
                return arr.iter().all(|elem| element_matches_type(elem, elem_type));
            }
            return false;
        }
    }

    match base.to_lowercase().as_str() {
        "any" => true,
        "char" => matches!(v, Char(_)) || matches!(v, Str(_)),
        "string" | "str" => matches!(v, Str(_)),
        "int" => {
            matches!(
                v,
                Number(_)
                    | U8(_)
                    | U16(_)
                    | U32(_)
                    | U64(_)
                    | U128(_)
                    | I8(_)
                    | I16(_)
                    | I32(_)
                    | I64(_)
                    | I128(_)
                    | BigInt(_)
            ) || {
                if let Some(n) = get_numeric_value(v) {
                    n.fract().abs() < 1e-12
                } else {
                    false
                }
            }
        }
        "number" | "float" => matches!(
            v,
            Number(_)
                | U8(_)
                | U16(_)
                | U32(_)
                | U64(_)
                | U128(_)
                | I8(_)
                | I16(_)
                | I32(_)
                | I64(_)
                | I128(_)
                | F32(_)
                | F64(_)
                | BigInt(_)
        ),
        "bool" | "boolean" => matches!(v, Bool(_)),
        "array" => matches!(v, Array(_) | RawArray(_, _) | DynArray(_)),
        "tuple" => matches!(v, Tuple(_)),
        "set" => matches!(v, Set(_)),
        "map" | "object" => matches!(v, Object(_)),
        // Fixed-width integer types (unsigned)
        "u8" => {
            matches!(v, Value::U8(_))
                || match v {
                    Value::U16(x) => *x <= u8::MAX as u16,
                    Value::U32(x) => *x <= u8::MAX as u32,
                    Value::U64(x) => *x <= u8::MAX as u64,
                    Value::U128(x) => *x <= u8::MAX as u128,
                    Value::I8(x) => *x >= 0,
                    Value::I16(x) => *x >= 0 && *x <= u8::MAX as i16,
                    Value::I32(x) => *x >= 0 && *x <= u8::MAX as i32,
                    Value::I64(x) => *x >= 0 && *x <= u8::MAX as i64,
                    Value::I128(x) => *x >= 0 && *x <= u8::MAX as i128,
                    Value::Number(n) => {
                        n.fract().abs() < 1e-12 && *n >= 0.0 && *n <= u8::MAX as f64
                    }
                    Value::BigInt(bi) => {
                        bi >= &num_bigint::BigInt::from(0u8)
                            && bi <= &num_bigint::BigInt::from(u8::MAX)
                    }
                    _ => false,
                }
        }
        "u16" => {
            matches!(v, Value::U8(_) | Value::U16(_))
                || match v {
                    Value::U32(x) => *x <= u16::MAX as u32,
                    Value::U64(x) => *x <= u16::MAX as u64,
                    Value::U128(x) => *x <= u16::MAX as u128,
                    Value::I8(x) => *x >= 0,
                    Value::I16(x) => *x >= 0,
                    Value::I32(x) => *x >= 0 && *x <= u16::MAX as i32,
                    Value::I64(x) => *x >= 0 && *x <= u16::MAX as i64,
                    Value::I128(x) => *x >= 0 && *x <= u16::MAX as i128,
                    Value::Number(n) => {
                        n.fract().abs() < 1e-12 && *n >= 0.0 && *n <= u16::MAX as f64
                    }
                    Value::BigInt(bi) => {
                        bi >= &num_bigint::BigInt::from(0u16)
                            && bi <= &num_bigint::BigInt::from(u16::MAX)
                    }
                    _ => false,
                }
        }
        "u32" => {
            matches!(v, Value::U8(_) | Value::U16(_) | Value::U32(_))
                || match v {
                    Value::U64(x) => *x <= u32::MAX as u64,
                    Value::U128(x) => *x <= u32::MAX as u128,
                    Value::I8(x) => *x >= 0,
                    Value::I16(x) => *x >= 0,
                    Value::I32(x) => *x >= 0,
                    Value::I64(x) => *x >= 0 && *x <= u32::MAX as i64,
                    Value::I128(x) => *x >= 0 && *x <= u32::MAX as i128,
                    Value::Number(n) => {
                        n.fract().abs() < 1e-12 && *n >= 0.0 && *n <= u32::MAX as f64
                    }
                    Value::BigInt(bi) => {
                        bi >= &num_bigint::BigInt::from(0u32)
                            && bi <= &num_bigint::BigInt::from(u32::MAX)
                    }
                    _ => false,
                }
        }
        "u64" => {
            matches!(
                v,
                Value::U8(_) | Value::U16(_) | Value::U32(_) | Value::U64(_)
            ) || match v {
                Value::U128(x) => *x <= u64::MAX as u128,
                Value::I8(x) => *x >= 0,
                Value::I16(x) => *x >= 0,
                Value::I32(x) => *x >= 0,
                Value::I64(x) => *x >= 0,
                Value::I128(x) => *x >= 0 && *x <= u64::MAX as i128,
                Value::Number(n) => n.fract().abs() < 1e-12 && *n >= 0.0 && *n <= u64::MAX as f64,
                Value::BigInt(bi) => {
                    bi >= &num_bigint::BigInt::from(0u64)
                        && bi <= &num_bigint::BigInt::from(u64::MAX)
                }
                _ => false,
            }
        }
        "u128" => {
            matches!(
                v,
                Value::U8(_) | Value::U16(_) | Value::U32(_) | Value::U64(_) | Value::U128(_)
            ) || match v {
                Value::I8(x) => *x >= 0,
                Value::I16(x) => *x >= 0,
                Value::I32(x) => *x >= 0,
                Value::I64(x) => *x >= 0,
                Value::I128(x) => *x >= 0,
                Value::Number(n) => n.fract().abs() < 1e-12 && *n >= 0.0,
                Value::BigInt(bi) => {
                    bi >= &num_bigint::BigInt::from(0u128)
                        && bi <= &num_bigint::BigInt::from(u128::MAX)
                }
                _ => false,
            }
        }
        // Fixed-width integer types (signed)
        "i8" => {
            matches!(v, Value::I8(_))
                || match v {
                    Value::U8(x) => *x <= i8::MAX as u8,
                    Value::I16(x) => *x >= i8::MIN as i16 && *x <= i8::MAX as i16,
                    Value::I32(x) => *x >= i8::MIN as i32 && *x <= i8::MAX as i32,
                    Value::I64(x) => *x >= i8::MIN as i64 && *x <= i8::MAX as i64,
                    Value::I128(x) => *x >= i8::MIN as i128 && *x <= i8::MAX as i128,
                    Value::Number(n) => {
                        n.fract().abs() < 1e-12 && *n >= i8::MIN as f64 && *n <= i8::MAX as f64
                    }
                    Value::BigInt(bi) => {
                        bi >= &num_bigint::BigInt::from(i8::MIN)
                            && bi <= &num_bigint::BigInt::from(i8::MAX)
                    }
                    _ => false,
                }
        }
        "i16" => {
            matches!(v, Value::I8(_) | Value::I16(_) | Value::U8(_))
                || match v {
                    Value::U16(x) => *x <= i16::MAX as u16,
                    Value::I32(x) => *x >= i16::MIN as i32 && *x <= i16::MAX as i32,
                    Value::I64(x) => *x >= i16::MIN as i64 && *x <= i16::MAX as i64,
                    Value::I128(x) => *x >= i16::MIN as i128 && *x <= i16::MAX as i128,
                    Value::Number(n) => {
                        n.fract().abs() < 1e-12 && *n >= i16::MIN as f64 && *n <= i16::MAX as f64
                    }
                    Value::BigInt(bi) => {
                        bi >= &num_bigint::BigInt::from(i16::MIN)
                            && bi <= &num_bigint::BigInt::from(i16::MAX)
                    }
                    _ => false,
                }
        }
        "i32" => {
            matches!(
                v,
                Value::I8(_) | Value::I16(_) | Value::I32(_) | Value::U8(_) | Value::U16(_)
            ) || match v {
                Value::U32(x) => *x <= i32::MAX as u32,
                Value::I64(x) => *x >= i32::MIN as i64 && *x <= i32::MAX as i64,
                Value::I128(x) => *x >= i32::MIN as i128 && *x <= i32::MAX as i128,
                Value::Number(n) => {
                    n.fract().abs() < 1e-12 && *n >= i32::MIN as f64 && *n <= i32::MAX as f64
                }
                Value::BigInt(bi) => {
                    bi >= &num_bigint::BigInt::from(i32::MIN)
                        && bi <= &num_bigint::BigInt::from(i32::MAX)
                }
                _ => false,
            }
        }
        "i64" => {
            matches!(
                v,
                Value::I8(_)
                    | Value::I16(_)
                    | Value::I32(_)
                    | Value::I64(_)
                    | Value::U8(_)
                    | Value::U16(_)
                    | Value::U32(_)
            ) || match v {
                Value::U64(x) => *x <= i64::MAX as u64,
                Value::I128(x) => *x >= i64::MIN as i128 && *x <= i64::MAX as i128,
                Value::Number(n) => {
                    n.fract().abs() < 1e-12 && *n >= i64::MIN as f64 && *n <= i64::MAX as f64
                }
                Value::BigInt(bi) => {
                    bi >= &num_bigint::BigInt::from(i64::MIN)
                        && bi <= &num_bigint::BigInt::from(i64::MAX)
                }
                _ => false,
            }
        }
        "i128" => {
            matches!(
                v,
                Value::I8(_)
                    | Value::I16(_)
                    | Value::I32(_)
                    | Value::I64(_)
                    | Value::I128(_)
                    | Value::U8(_)
                    | Value::U16(_)
                    | Value::U32(_)
                    | Value::U64(_)
            ) || match v {
                Value::U128(x) => *x <= i128::MAX as u128,
                Value::Number(n) => n.fract().abs() < 1e-12,
                Value::BigInt(bi) => {
                    bi >= &num_bigint::BigInt::from(i128::MIN)
                        && bi <= &num_bigint::BigInt::from(i128::MAX)
                }
                _ => false,
            }
        }
        // Fixed-width float types
        "f32" => matches!(v, Value::F32(_)) || get_numeric_value(v).is_some(),
        "f64" => matches!(v, Value::F64(_)) || get_numeric_value(v).is_some(),
        _ => true, // unknown annotation treated permissively at runtime
    }
}

/// Check if an element value matches the expected type string.
///
/// Used for array element type checking after coercion.
pub(in crate::execution::runtime_core) fn element_matches_type(v: &Value, type_str: &str) -> bool {
    use crate::parsing::ast::Value::*;
    match type_str.to_lowercase().as_str() {
        "u8" => matches!(v, U8(_)),
        "u16" => matches!(v, U16(_)),
        "u32" => matches!(v, U32(_)),
        "u64" => matches!(v, U64(_)),
        "u128" => matches!(v, U128(_)),
        "i8" => matches!(v, I8(_)),
        "i16" => matches!(v, I16(_)),
        "i32" => matches!(v, I32(_)),
        "i64" => matches!(v, I64(_)),
        "i128" => matches!(v, I128(_)),
        "f32" => matches!(v, F32(_)),
        "f64" => matches!(v, F64(_)),
        "int" | "number" | "float" => matches!(
            v,
            Number(_)
                | U8(_)
                | U16(_)
                | U32(_)
                | U64(_)
                | U128(_)
                | I8(_)
                | I16(_)
                | I32(_)
                | I64(_)
                | I128(_)
                | F32(_)
                | F64(_)
                | BigInt(_)
        ),
        "string" | "str" => matches!(v, Str(_)),
        "bool" | "boolean" => matches!(v, Bool(_)),
        "char" => matches!(v, Char(_)),
        "any" => true,
        _ => true, // unknown type treated permissively
    }
}

/// Convert a Number value to a fixed-width type based on the type annotation.
///
/// Returns Ok(converted_value) if successful, or Err(error_message) if out of range.
/// Also handles array type annotations like [u8], [u8;raw], [u8;4], [u8;4;raw] by
/// coercing each element to the specified element type.
/// When `raw` is specified, creates a RawArray without metadata overhead.
pub(in crate::execution::runtime_core) fn coerce_to_fixed_width(
    v: &Value,
    ann: &str,
) -> Result<Option<Value>, String> {
    use crate::parsing::ast::Value::*;

    // Handle array type annotations: [T], [T;N], [T;raw], [T;N;raw]
    if ann.starts_with('[') && ann.ends_with(']') {
        let inner = &ann[1..ann.len() - 1];

        // Split parts: [ elem_type ; maybe_len ; maybe_raw ]
        let parts: Vec<&str> = inner.split(';').collect();
        let elem_type = parts.first().cloned().unwrap_or("");
        let mut fixed_len: Option<usize> = None;
        let mut is_raw = false;
        for p in parts.iter().skip(1) {
            if p.trim().eq_ignore_ascii_case("raw") {
                is_raw = true;
            } else if let Ok(n) = p.trim().parse::<usize>() {
                fixed_len = Some(n);
            }
        }

        // If value is an array, raw array, or dynamic array, coerce each element
        let arr: &Vec<Value> = match v {
            Array(arr) => arr,
            RawArray(_, arr) => arr,
            DynArray(da) => &da.data,
            _ => return Ok(None),
        };

        let mut coerced_elements = Vec::with_capacity(arr.len());
        for (i, elem) in arr.iter().enumerate() {
            match coerce_to_fixed_width(elem, elem_type)? {
                Some(converted) => coerced_elements.push(converted),
                None => {
                    // Element is already the correct type or doesn't need coercion
                    // Check if it matches the expected type
                    if !element_matches_type(elem, elem_type) {
                        return Err(format!(
                            "array element {} has incompatible type for {}",
                            i, elem_type
                        ));
                    }
                    coerced_elements.push(elem.clone());
                }
            }
        }

        // Return RawArray if raw annotation, otherwise DynArray with proper element type
        if is_raw {
            // For raw arrays, keep elements as-is; capacity equals length
            return Ok(Some(RawArray(elem_type.to_string(), coerced_elements)));
        } else {
            // Create a DynArray with proper element type and track declared capacity if provided
            if let Some(n) = fixed_len {
                return Ok(Some(DynArray(Box::new(
                    crate::parsing::ast::DynamicArray::with_type_and_capacity(
                        coerced_elements,
                        elem_type,
                        n,
                    ),
                ))));
            } else {
                return Ok(Some(DynArray(Box::new(
                    crate::parsing::ast::DynamicArray::with_type(coerced_elements, elem_type),
                ))));
            }
        }
    }

    // Handle BigInt values directly
    if let Value::BigInt(bi) = v {
        return match ann.to_lowercase().as_str() {
            "u8" => bi
                .to_u8()
                .map(U8)
                .map(Some)
                .ok_or_else(|| format!("value {} is out of range for u8 (0..{})", bi, u8::MAX)),
            "u16" => {
                bi.to_u16().map(U16).map(Some).ok_or_else(|| {
                    format!("value {} is out of range for u16 (0..{})", bi, u16::MAX)
                })
            }
            "u32" => {
                bi.to_u32().map(U32).map(Some).ok_or_else(|| {
                    format!("value {} is out of range for u32 (0..{})", bi, u32::MAX)
                })
            }
            "u64" => {
                bi.to_u64().map(U64).map(Some).ok_or_else(|| {
                    format!("value {} is out of range for u64 (0..{})", bi, u64::MAX)
                })
            }
            "u128" => bi
                .to_u128()
                .map(U128)
                .map(Some)
                .ok_or_else(|| format!("value {} is out of range for u128", bi)),
            "i8" => bi.to_i8().map(I8).map(Some).ok_or_else(|| {
                format!(
                    "value {} is out of range for i8 ({}..{})",
                    bi,
                    i8::MIN,
                    i8::MAX
                )
            }),
            "i16" => bi.to_i16().map(I16).map(Some).ok_or_else(|| {
                format!(
                    "value {} is out of range for i16 ({}..{})",
                    bi,
                    i16::MIN,
                    i16::MAX
                )
            }),
            "i32" => bi.to_i32().map(I32).map(Some).ok_or_else(|| {
                format!(
                    "value {} is out of range for i32 ({}..{})",
                    bi,
                    i32::MIN,
                    i32::MAX
                )
            }),
            "i64" => bi.to_i64().map(I64).map(Some).ok_or_else(|| {
                format!(
                    "value {} is out of range for i64 ({}..{})",
                    bi,
                    i64::MIN,
                    i64::MAX
                )
            }),
            "i128" => bi
                .to_i128()
                .map(I128)
                .map(Some)
                .ok_or_else(|| format!("value {} is out of range for i128", bi)),
            "f32" => bi
                .to_f32()
                .map(F32)
                .map(Some)
                .ok_or_else(|| format!("value {} is out of range for f32", bi)),
            "f64" => bi
                .to_f64()
                .map(F64)
                .map(Some)
                .ok_or_else(|| format!("value {} is out of range for f64", bi)),
            "uint" => {
                if *bi < num_bigint::BigInt::from(0u8) {
                    return Err(format!("uint requires a non-negative integer, got {}", bi));
                }
                if let Some(x) = bi.to_u8() {
                    Ok(Some(U8(x)))
                } else if let Some(x) = bi.to_u16() {
                    Ok(Some(U16(x)))
                } else if let Some(x) = bi.to_u32() {
                    Ok(Some(U32(x)))
                } else if let Some(x) = bi.to_u64() {
                    Ok(Some(U64(x)))
                } else if let Some(x) = bi.to_u128() {
                    Ok(Some(U128(x)))
                } else {
                    Ok(Some(BigInt(bi.clone())))
                }
            }
            "int" => {
                if let Some(x) = bi.to_i8() {
                    Ok(Some(I8(x)))
                } else if let Some(x) = bi.to_i16() {
                    Ok(Some(I16(x)))
                } else if let Some(x) = bi.to_i32() {
                    Ok(Some(I32(x)))
                } else if let Some(x) = bi.to_i64() {
                    Ok(Some(I64(x)))
                } else if let Some(x) = bi.to_i128() {
                    Ok(Some(I128(x)))
                } else {
                    Ok(Some(BigInt(bi.clone())))
                }
            }
            _ => Ok(None),
        };
    }

    #[derive(Clone, Copy)]
    enum IntegerOrFloat {
        Unsigned(u128),
        Signed(i128),
        Float(f64),
    }

    let num_val = match v {
        Number(n) => {
            if n.fract().abs() < 1e-12 {
                if *n >= 0.0 {
                    IntegerOrFloat::Unsigned(*n as u128)
                } else {
                    IntegerOrFloat::Signed(*n as i128)
                }
            } else {
                IntegerOrFloat::Float(*n)
            }
        }
        U8(x) => IntegerOrFloat::Unsigned(*x as u128),
        U16(x) => IntegerOrFloat::Unsigned(*x as u128),
        U32(x) => IntegerOrFloat::Unsigned(*x as u128),
        U64(x) => IntegerOrFloat::Unsigned(*x as u128),
        U128(x) => IntegerOrFloat::Unsigned(*x),
        I8(x) => IntegerOrFloat::Signed(*x as i128),
        I16(x) => IntegerOrFloat::Signed(*x as i128),
        I32(x) => IntegerOrFloat::Signed(*x as i128),
        I64(x) => IntegerOrFloat::Signed(*x as i128),
        I128(x) => IntegerOrFloat::Signed(*x),
        F32(x) => IntegerOrFloat::Float(*x as f64),
        F64(x) => IntegerOrFloat::Float(*x),
        _ => return Ok(None),
    };

    match ann.to_lowercase().as_str() {
        "u8" => match num_val {
            IntegerOrFloat::Unsigned(x) if x <= u8::MAX as u128 => Ok(Some(U8(x as u8))),
            IntegerOrFloat::Signed(x) if x >= 0 && x <= u8::MAX as i128 => Ok(Some(U8(x as u8))),
            IntegerOrFloat::Float(f)
                if f.fract().abs() < 1e-12 && f >= 0.0 && f <= u8::MAX as f64 =>
            {
                Ok(Some(U8(f as u8)))
            }
            IntegerOrFloat::Float(f) => Err(format!("u8 requires an integer, got {}", f)),
            _ => Err(format!("value is out of range for u8 (0..{})", u8::MAX)),
        },
        "u16" => match num_val {
            IntegerOrFloat::Unsigned(x) if x <= u16::MAX as u128 => Ok(Some(U16(x as u16))),
            IntegerOrFloat::Signed(x) if x >= 0 && x <= u16::MAX as i128 => Ok(Some(U16(x as u16))),
            IntegerOrFloat::Float(f)
                if f.fract().abs() < 1e-12 && f >= 0.0 && f <= u16::MAX as f64 =>
            {
                Ok(Some(U16(f as u16)))
            }
            IntegerOrFloat::Float(f) => Err(format!("u16 requires an integer, got {}", f)),
            _ => Err(format!("value is out of range for u16 (0..{})", u16::MAX)),
        },
        "u32" => match num_val {
            IntegerOrFloat::Unsigned(x) if x <= u32::MAX as u128 => Ok(Some(U32(x as u32))),
            IntegerOrFloat::Signed(x) if x >= 0 && x <= u32::MAX as i128 => Ok(Some(U32(x as u32))),
            IntegerOrFloat::Float(f)
                if f.fract().abs() < 1e-12 && f >= 0.0 && f <= u32::MAX as f64 =>
            {
                Ok(Some(U32(f as u32)))
            }
            IntegerOrFloat::Float(f) => Err(format!("u32 requires an integer, got {}", f)),
            _ => Err(format!("value is out of range for u32 (0..{})", u32::MAX)),
        },
        "u64" => match num_val {
            IntegerOrFloat::Unsigned(x) if x <= u64::MAX as u128 => Ok(Some(U64(x as u64))),
            IntegerOrFloat::Signed(x) if x >= 0 && (x as u128) <= u64::MAX as u128 => {
                Ok(Some(U64(x as u64)))
            }
            IntegerOrFloat::Float(f)
                if f.fract().abs() < 1e-12 && f >= 0.0 && f <= u64::MAX as f64 =>
            {
                Ok(Some(U64(f as u64)))
            }
            IntegerOrFloat::Float(f) => Err(format!("u64 requires an integer, got {}", f)),
            _ => Err(format!("value is out of range for u64 (0..{})", u64::MAX)),
        },
        "u128" => match num_val {
            IntegerOrFloat::Unsigned(x) => Ok(Some(U128(x))),
            IntegerOrFloat::Signed(x) if x >= 0 => Ok(Some(U128(x as u128))),
            IntegerOrFloat::Float(f) if f.fract().abs() < 1e-12 && f >= 0.0 => {
                Ok(Some(U128(f as u128)))
            }
            IntegerOrFloat::Float(f) => Err(format!("u128 requires an integer, got {}", f)),
            _ => Err("value is out of range for u128".to_string()),
        },
        "i8" => match num_val {
            IntegerOrFloat::Unsigned(x) if x <= i8::MAX as u128 => Ok(Some(I8(x as i8))),
            IntegerOrFloat::Signed(x) if x >= i8::MIN as i128 && x <= i8::MAX as i128 => {
                Ok(Some(I8(x as i8)))
            }
            IntegerOrFloat::Float(f)
                if f.fract().abs() < 1e-12 && f >= i8::MIN as f64 && f <= i8::MAX as f64 =>
            {
                Ok(Some(I8(f as i8)))
            }
            IntegerOrFloat::Float(f) => Err(format!("i8 requires an integer, got {}", f)),
            _ => Err(format!(
                "value is out of range for i8 ({}..{})",
                i8::MIN,
                i8::MAX
            )),
        },
        "i16" => match num_val {
            IntegerOrFloat::Unsigned(x) if x <= i16::MAX as u128 => Ok(Some(I16(x as i16))),
            IntegerOrFloat::Signed(x) if x >= i16::MIN as i128 && x <= i16::MAX as i128 => {
                Ok(Some(I16(x as i16)))
            }
            IntegerOrFloat::Float(f)
                if f.fract().abs() < 1e-12 && f >= i16::MIN as f64 && f <= i16::MAX as f64 =>
            {
                Ok(Some(I16(f as i16)))
            }
            IntegerOrFloat::Float(f) => Err(format!("i16 requires an integer, got {}", f)),
            _ => Err(format!(
                "value is out of range for i16 ({}..{})",
                i16::MIN,
                i16::MAX
            )),
        },
        "i32" => match num_val {
            IntegerOrFloat::Unsigned(x) if x <= i32::MAX as u128 => Ok(Some(I32(x as i32))),
            IntegerOrFloat::Signed(x) if x >= i32::MIN as i128 && x <= i32::MAX as i128 => {
                Ok(Some(I32(x as i32)))
            }
            IntegerOrFloat::Float(f)
                if f.fract().abs() < 1e-12 && f >= i32::MIN as f64 && f <= i32::MAX as f64 =>
            {
                Ok(Some(I32(f as i32)))
            }
            IntegerOrFloat::Float(f) => Err(format!("i32 requires an integer, got {}", f)),
            _ => Err(format!(
                "value is out of range for i32 ({}..{})",
                i32::MIN,
                i32::MAX
            )),
        },
        "i64" => match num_val {
            IntegerOrFloat::Unsigned(x) if x <= i64::MAX as u128 => Ok(Some(I64(x as i64))),
            IntegerOrFloat::Signed(x) if x >= i64::MIN as i128 && x <= i64::MAX as i128 => {
                Ok(Some(I64(x as i64)))
            }
            IntegerOrFloat::Float(f)
                if f.fract().abs() < 1e-12 && f >= i64::MIN as f64 && f <= i64::MAX as f64 =>
            {
                Ok(Some(I64(f as i64)))
            }
            IntegerOrFloat::Float(f) => Err(format!("i64 requires an integer, got {}", f)),
            _ => Err(format!(
                "value is out of range for i64 ({}..{})",
                i64::MIN,
                i64::MAX
            )),
        },
        "i128" => match num_val {
            IntegerOrFloat::Unsigned(x) if x <= i128::MAX as u128 => Ok(Some(I128(x as i128))),
            IntegerOrFloat::Signed(x) => Ok(Some(I128(x))),
            IntegerOrFloat::Float(f) if f.fract().abs() < 1e-12 => Ok(Some(I128(f as i128))),
            IntegerOrFloat::Float(f) => Err(format!("i128 requires an integer, got {}", f)),
            _ => Err("value is out of range for i128".to_string()),
        },
        "f32" => match num_val {
            IntegerOrFloat::Unsigned(x) => Ok(Some(F32(x as f32))),
            IntegerOrFloat::Signed(x) => Ok(Some(F32(x as f32))),
            IntegerOrFloat::Float(f) => Ok(Some(F32(f as f32))),
        },
        "f64" => match num_val {
            IntegerOrFloat::Unsigned(x) => Ok(Some(F64(x as f64))),
            IntegerOrFloat::Signed(x) => Ok(Some(F64(x as f64))),
            IntegerOrFloat::Float(f) => Ok(Some(F64(f))),
        },
        "uint" => match num_val {
            IntegerOrFloat::Unsigned(x) => {
                if x <= u8::MAX as u128 {
                    Ok(Some(U8(x as u8)))
                } else if x <= u16::MAX as u128 {
                    Ok(Some(U16(x as u16)))
                } else if x <= u32::MAX as u128 {
                    Ok(Some(U32(x as u32)))
                } else if x <= u64::MAX as u128 {
                    Ok(Some(U64(x as u64)))
                } else {
                    Ok(Some(U128(x)))
                }
            }
            IntegerOrFloat::Signed(x) if x >= 0 => {
                let u = x as u128;
                if u <= u8::MAX as u128 {
                    Ok(Some(U8(u as u8)))
                } else if u <= u16::MAX as u128 {
                    Ok(Some(U16(u as u16)))
                } else if u <= u32::MAX as u128 {
                    Ok(Some(U32(u as u32)))
                } else if u <= u64::MAX as u128 {
                    Ok(Some(U64(u as u64)))
                } else {
                    Ok(Some(U128(u)))
                }
            }
            IntegerOrFloat::Signed(x) => {
                Err(format!("uint requires a non-negative integer, got {}", x))
            }
            IntegerOrFloat::Float(f) if f.fract().abs() < 1e-12 && f >= 0.0 => {
                let u = f as u128;
                if u <= u8::MAX as u128 {
                    Ok(Some(U8(u as u8)))
                } else if u <= u16::MAX as u128 {
                    Ok(Some(U16(u as u16)))
                } else if u <= u32::MAX as u128 {
                    Ok(Some(U32(u as u32)))
                } else if u <= u64::MAX as u128 {
                    Ok(Some(U64(u as u64)))
                } else {
                    Ok(Some(U128(u)))
                }
            }
            IntegerOrFloat::Float(f) => {
                Err(format!("uint requires a non-negative integer, got {}", f))
            }
        },
        "int" => match num_val {
            IntegerOrFloat::Unsigned(x) => {
                if x <= i8::MAX as u128 {
                    Ok(Some(I8(x as i8)))
                } else if x <= i16::MAX as u128 {
                    Ok(Some(I16(x as i16)))
                } else if x <= i32::MAX as u128 {
                    Ok(Some(I32(x as i32)))
                } else if x <= i64::MAX as u128 {
                    Ok(Some(I64(x as i64)))
                } else if x <= i128::MAX as u128 {
                    Ok(Some(I128(x as i128)))
                } else {
                    Ok(Some(U128(x)))
                }
            }
            IntegerOrFloat::Signed(x) => {
                if x >= i8::MIN as i128 && x <= i8::MAX as i128 {
                    Ok(Some(I8(x as i8)))
                } else if x >= i16::MIN as i128 && x <= i16::MAX as i128 {
                    Ok(Some(I16(x as i16)))
                } else if x >= i32::MIN as i128 && x <= i32::MAX as i128 {
                    Ok(Some(I32(x as i32)))
                } else if x >= i64::MIN as i128 && x <= i64::MAX as i128 {
                    Ok(Some(I64(x as i64)))
                } else {
                    Ok(Some(I128(x)))
                }
            }
            IntegerOrFloat::Float(f) if f.fract().abs() < 1e-12 => {
                let x = f as i128;
                if x >= i8::MIN as i128 && x <= i8::MAX as i128 {
                    Ok(Some(I8(x as i8)))
                } else if x >= i16::MIN as i128 && x <= i16::MAX as i128 {
                    Ok(Some(I16(x as i16)))
                } else if x >= i32::MIN as i128 && x <= i32::MAX as i128 {
                    Ok(Some(I32(x as i32)))
                } else if x >= i64::MIN as i128 && x <= i64::MAX as i128 {
                    Ok(Some(I64(x as i64)))
                } else {
                    Ok(Some(I128(x)))
                }
            }
            IntegerOrFloat::Float(f) => Err(format!("int requires an integer, got {}", f)),
        },
        _ => Ok(None),
    }
}

/// Smart type inference: automatically infer the smallest appropriate fixed-width type
/// based on the value (positive/negative, integer/float, and range).
///
/// This is called when no type annotation is provided for a literal.
pub(in crate::execution::runtime_core) fn infer_numeric_type(v: Value) -> Value {
    use crate::parsing::ast::Value::*;

    let n = match &v {
        Number(n) => *n,
        // Already a fixed-width type, keep it
        U8(_) | U16(_) | U32(_) | U64(_) | U128(_) | I8(_) | I16(_) | I32(_) | I64(_) | I128(_)
        | F32(_) | F64(_) => return v,
        _ => return v,
    };

    // Check if it's an integer (no fractional part)
    let is_integer = n.fract().abs() < 1e-12;

    if is_integer {
        let i = n as i128;
        if i >= i64::MIN as i128 && i <= i64::MAX as i128 {
            I64(i as i64)
        } else if i >= 0 && i <= u64::MAX as i128 {
            U64(i as u64)
        } else if i >= 0 {
            U128(i as u128)
        } else {
            I128(i)
        }
    } else {
        F64(n)
    }
}

pub(in crate::execution::runtime_core) fn coerce_and_apply_defaults(
    envs: &Vec<crate::execution::runtime_core::interpreter::env::Env>,
    global: usize,
    start_env: Option<usize>,
    v: &Value,
    ann: &str,
) -> Result<Option<Value>, String> {
    // 1. First, try standard coercion (e.g. for numbers or arrays)
    let coerced = coerce_to_fixed_width(v, ann)?;
    let working_v = coerced.as_ref().unwrap_or(v);

    // 2. If it's an object, check if we need to apply type alias defaults
    if let Value::Object(obj_map) = working_v {
        let alias_name = if let Some(idx) = ann.find('<') {
            &ann[..idx]
        } else {
            ann
        };

        let mut curr_env = start_env.or(Some(global));
        while let Some(env_id) = curr_env {
            if env_id < envs.len() {
                if let Some(val) = envs[env_id].values.get(alias_name) {
                    if let Value::Object(alias_map) = val {
                        if alias_map.get("__type_alias__").is_some() {
                            // Found type alias! Check for default values
                            if let Some(Value::Object(defaults_map)) = alias_map.get("__defaults__")
                            {
                                let mut merged = (**obj_map).clone();
                                let mut changed = false;
                                for (k, def_val) in defaults_map.iter() {
                                    if !merged.contains_key(k) {
                                        merged.insert(k.clone(), def_val.clone());
                                        changed = true;
                                    }
                                }
                                if changed {
                                    return Ok(Some(Value::Object(std::sync::Arc::new(merged))));
                                }
                            }
                            break;
                        }
                    }
                }
                curr_env = envs[env_id].enclosing;
            } else {
                break;
            }
        }
    }

    Ok(coerced)
}
