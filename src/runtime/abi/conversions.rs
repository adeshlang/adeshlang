//! Conversion layer between AST Value and NanValue
//!
//! This module provides bidirectional conversion for the unified runtime model:
//! - Typed values (u8, i32, etc.) stay native and unboxed
//! - Dynamic values use NanValue (8-byte NaN-boxed representation)
//!
//! Conversions happen at dynamic boundaries:
//! - Array/object insertions
//! - Untyped variable assignments
//! - Dynamic function calls

use super::RuntimeError;
use crate::parsing::ast::Value;
use crate::runtime::nanvalue::{HeapValue, NanValue};
use num_bigint::BigInt;

/// Convert AST Value to NanValue (for dynamic contexts)
///
/// This function handles the most common dynamic value types.
/// Complex types (Functions, Classes, etc.) are converted to heap representations.
#[inline]
pub fn value_to_nanvalue(v: &Value) -> Result<NanValue, RuntimeError> {
    match v {
        // Primitives - direct encoding
        Value::Null => Ok(NanValue::null()),
        Value::Bool(b) => Ok(NanValue::from_bool(*b)),
        Value::Char(c) => Ok(NanValue::from_char(*c)),

        // Numbers
        Value::Number(n) => Ok(NanValue::from_f64(*n)),
        Value::F32(f) => Ok(NanValue::from_f32(*f)),
        Value::F64(f) => Ok(NanValue::from_f64(*f)),

        // Integers - exact width type preservation
        Value::I8(n) => Ok(NanValue::from_i8(*n)),
        Value::I16(n) => Ok(NanValue::from_i16(*n)),
        Value::I32(n) => Ok(NanValue::from_i32(*n)),
        Value::I64(n) => {
            if let Some(nv) = NanValue::from_i64(*n) {
                Ok(nv)
            } else {
                Ok(NanValue::from_heap(HeapValue::BigInt(BigInt::from(*n))))
            }
        }
        Value::I128(n) => {
            if let Some(nv) = NanValue::from_i128(*n) {
                Ok(nv)
            } else {
                Ok(NanValue::from_heap(HeapValue::BigInt(BigInt::from(*n))))
            }
        }
        Value::U8(n) => Ok(NanValue::from_u8(*n)),
        Value::U16(n) => Ok(NanValue::from_u16(*n)),
        Value::U32(n) => Ok(NanValue::from_u32(*n)),
        Value::U64(n) => {
            if let Some(nv) = NanValue::from_u64(*n) {
                Ok(nv)
            } else {
                Ok(NanValue::from_heap(HeapValue::BigInt(BigInt::from(*n))))
            }
        }
        Value::U128(n) => {
            if let Some(nv) = NanValue::from_u128(*n) {
                Ok(nv)
            } else {
                Ok(NanValue::from_heap(HeapValue::BigInt(BigInt::from(*n))))
            }
        }

        // BigInt - always heap
        Value::BigInt(bi) => Ok(NanValue::from_heap(HeapValue::BigInt(bi.clone()))),

        // String - always heap
        Value::Str(s) => Ok(NanValue::from_heap(HeapValue::String(s.clone()))),

        // Arrays - convert elements recursively
        Value::Array(elements) => {
            let mut nan_elements = Vec::with_capacity(elements.len());
            for elem in elements {
                nan_elements.push(value_to_nanvalue(elem)?);
            }
            Ok(NanValue::from_heap(HeapValue::Array(nan_elements)))
        }

        // Objects - convert values recursively
        Value::Object(map) => {
            let mut nan_map = std::collections::HashMap::new();
            for (k, v) in map.iter() {
                nan_map.insert(k.clone(), value_to_nanvalue(v)?);
            }
            Ok(NanValue::from_heap(HeapValue::Object(nan_map)))
        }

        // Complex types - not yet supported in NanValue
        // These will need extended HeapValue variants in the future
        Value::LazyRange(..) => Err(RuntimeError::new("cannot convert range to NanValue")),
        _ => Err(RuntimeError::new(&format!(
            "Conversion to NanValue not yet supported for type: {:?}",
            std::mem::discriminant(v)
        ))),
    }
}

/// Convert NanValue back to AST Value (when needed by existing code)
///
/// This function reconstructs the AST Value from the NanValue representation.
/// Used at boundaries where existing code expects Value enum.
#[inline]
pub fn nanvalue_to_value(nv: &NanValue) -> Result<Value, RuntimeError> {
    if nv.is_null() {
        return Ok(Value::Null);
    }

    if nv.is_bool() {
        return Ok(Value::Bool(nv.as_bool().unwrap()));
    }

    if nv.is_double() {
        return Ok(Value::Number(nv.as_f64().unwrap()));
    }

    if nv.is_i8() {
        return Ok(Value::I8(nv.as_i8().unwrap()));
    }
    if nv.is_i16() {
        return Ok(Value::I16(nv.as_i16().unwrap()));
    }
    if nv.is_i32() {
        return Ok(Value::I32(nv.as_i32().unwrap()));
    }
    if nv.is_i64() {
        return Ok(Value::I64(nv.as_i64().unwrap()));
    }
    if nv.is_u8() {
        return Ok(Value::U8(nv.as_u8().unwrap()));
    }
    if nv.is_u16() {
        return Ok(Value::U16(nv.as_u16().unwrap()));
    }
    if nv.is_u32() {
        return Ok(Value::U32(nv.as_u32().unwrap()));
    }
    if nv.is_u64() {
        return Ok(Value::U64(nv.as_u64().unwrap()));
    }
    if nv.is_f32() {
        return Ok(Value::F32(nv.as_f32().unwrap()));
    }
    if nv.is_i128() {
        return Ok(Value::I128(nv.as_i128().unwrap()));
    }
    if nv.is_u128() {
        return Ok(Value::U128(nv.as_u128().unwrap()));
    }

    if nv.is_char() {
        return Ok(Value::Char(nv.as_char().unwrap()));
    }

    if nv.is_pointer() {
        let heap = nv
            .as_heap()
            .ok_or_else(|| RuntimeError::new("Failed to extract heap value"))?;
        match heap.as_ref() {
            HeapValue::String(s) => Ok(Value::Str(s.clone())),
            HeapValue::BigInt(bi) => Ok(Value::BigInt(bi.clone())),
            HeapValue::Array(elements) => {
                let mut value_elements = Vec::with_capacity(elements.len());
                for elem in elements {
                    value_elements.push(nanvalue_to_value(&elem)?);
                }
                Ok(Value::Array(value_elements))
            }
            HeapValue::Object(map) => {
                use rustc_hash::FxHasher;
                use std::collections::HashMap;
                use std::hash::BuildHasherDefault;

                let mut value_map: HashMap<String, Value, BuildHasherDefault<FxHasher>> =
                    HashMap::default();
                for (k, v) in map.iter() {
                    value_map.insert(k.clone(), nanvalue_to_value(v)?);
                }
                Ok(Value::Object(std::sync::Arc::new(value_map)))
            }
        }
    } else {
        Err(RuntimeError::new("Unknown NanValue type"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_null_conversion() {
        let v = Value::Null;
        let nv = value_to_nanvalue(&v).unwrap();
        assert!(nv.is_null());

        let v2 = nanvalue_to_value(&nv).unwrap();
        assert!(matches!(v2, Value::Null));
    }

    #[test]
    fn test_bool_conversion() {
        let v = Value::Bool(true);
        let nv = value_to_nanvalue(&v).unwrap();
        assert!(nv.is_bool());
        assert!(nv.as_bool().unwrap());

        let v2 = nanvalue_to_value(&nv).unwrap();
        assert!(matches!(v2, Value::Bool(true)));
    }

    #[test]
    fn test_number_conversion() {
        let v = Value::Number(42.5);
        let nv = value_to_nanvalue(&v).unwrap();
        assert!(nv.is_double());
        assert_eq!(nv.as_f64().unwrap(), 42.5);

        let v2 = nanvalue_to_value(&nv).unwrap();
        assert!(matches!(v2, Value::Number(n) if n == 42.5));
    }

    #[test]
    fn test_small_int_conversion() {
        let v = Value::I32(42);
        let nv = value_to_nanvalue(&v).unwrap();
        assert!(nv.is_int48());
        assert_eq!(nv.as_i48().unwrap(), 42);

        let v2 = nanvalue_to_value(&nv).unwrap();
        assert!(matches!(v2, Value::I32(42)));
    }

    #[test]
    fn test_string_conversion() {
        let v = Value::Str("hello".to_string());
        let nv = value_to_nanvalue(&v).unwrap();
        assert!(nv.is_pointer());

        let v2 = nanvalue_to_value(&nv).unwrap();
        assert!(matches!(v2, Value::Str(s) if s == "hello"));
    }

    #[test]
    fn test_array_conversion() {
        let v = Value::Array(vec![
            Value::Number(1.0),
            Value::Number(2.0),
            Value::Number(3.0),
        ]);
        let nv = value_to_nanvalue(&v).unwrap();
        assert!(nv.is_pointer());

        let v2 = nanvalue_to_value(&nv).unwrap();
        if let Value::Array(arr) = v2 {
            assert_eq!(arr.len(), 3);
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_bigint_conversion() {
        let v = Value::BigInt(BigInt::from(123456789i64));
        let nv = value_to_nanvalue(&v).unwrap();
        assert!(nv.is_pointer());

        let v2 = nanvalue_to_value(&nv).unwrap();
        assert!(matches!(v2, Value::BigInt(_)));
    }
}
