// NanValue ABI Operations
//
// This module provides arithmetic and comparison operations for NanValue,
// maintaining the same semantics as the Value-based operations but operating
// on the 8-byte NaN-boxed representation.
//
// All operations maintain the unified runtime model:
// - Identical semantics to Value-based operations
// - Type coercion rules match existing behavior
// - Error handling consistent with RuntimeError
// - Inline annotations for zero-cost abstractions

use crate::runtime::abi::RuntimeError;
use crate::runtime::nanvalue::{HeapValue, NanValue};
use num_bigint::BigInt;

// Helper: Convert NanValue to f64 for numeric operations
#[inline]
fn as_f64_nan(v: &NanValue) -> Result<f64, RuntimeError> {
    if v.is_double() {
        Ok(v.as_f64().unwrap())
    } else if v.is_int48() {
        Ok(v.as_i48().unwrap() as f64)
    } else if v.is_null() {
        Ok(0.0) // null coerces to 0
    } else if v.is_bool() {
        Ok(if v.as_bool().unwrap() { 1.0 } else { 0.0 })
    } else if v.is_pointer() {
        // Check if it's a BigInt
        if let Some(arc) = v.as_heap() {
            if let HeapValue::BigInt(big) = &*arc {
                // Convert BigInt to f64 (may lose precision)
                return Ok(big.to_string().parse::<f64>().unwrap_or(f64::NAN));
            }
        }
        Err(RuntimeError::new(
            "Cannot convert non-numeric value to number",
        ))
    } else {
        Err(RuntimeError::new("Cannot convert value to number"))
    }
}

// Helper: Convert NanValue to BigInt for exact integer operations
#[inline]
fn as_bigint_nan(v: &NanValue) -> Result<BigInt, RuntimeError> {
    if v.is_int48() {
        Ok(BigInt::from(v.as_i48().unwrap()))
    } else if v.is_double() {
        let f = v.as_f64().unwrap();
        if f.is_finite() && f.fract() == 0.0 {
            Ok(BigInt::from(f as i64))
        } else {
            Err(RuntimeError::new("Cannot convert non-integer to BigInt"))
        }
    } else if v.is_pointer() {
        if let Some(arc) = v.as_heap() {
            if let HeapValue::BigInt(big) = &*arc {
                return Ok(big.clone());
            }
        }
        Err(RuntimeError::new("Cannot convert non-BigInt to BigInt"))
    } else {
        Err(RuntimeError::new("Cannot convert value to BigInt"))
    }
}

/// Add two NanValues
#[inline(always)]
pub fn abi_add_nan(left: &NanValue, right: &NanValue) -> Result<NanValue, RuntimeError> {
    // String concatenation
    if left.is_pointer() && right.is_pointer() {
        if let (Some(l_arc), Some(r_arc)) = (left.as_heap(), right.as_heap()) {
            if let (HeapValue::String(l_str), HeapValue::String(r_str)) = (&*l_arc, &*r_arc) {
                return Ok(NanValue::from_heap(HeapValue::String(format!(
                    "{}{}",
                    l_str, r_str
                ))));
            }
        }
    }

    // Array concatenation
    if left.is_pointer() && right.is_pointer() {
        if let (Some(l_arc), Some(r_arc)) = (left.as_heap(), right.as_heap()) {
            if let (HeapValue::Array(l_arr), HeapValue::Array(r_arr)) = (&*l_arc, &*r_arc) {
                let mut result = l_arr.clone();
                result.extend_from_slice(r_arr);
                return Ok(NanValue::from_heap(HeapValue::Array(result)));
            }
        }
    }

    // BigInt addition (if either is BigInt)
    if (left.is_pointer() || right.is_pointer()) && !left.is_null() && !right.is_null() {
        if let (Ok(l_big), Ok(r_big)) = (as_bigint_nan(left), as_bigint_nan(right)) {
            return Ok(NanValue::from_heap(HeapValue::BigInt(l_big + r_big)));
        }
    }

    // Numeric addition
    let l_num = as_f64_nan(left)?;
    let r_num = as_f64_nan(right)?;
    Ok(NanValue::from_f64(l_num + r_num))
}

/// Subtract two NanValues
#[inline(always)]
pub fn abi_sub_nan(left: &NanValue, right: &NanValue) -> Result<NanValue, RuntimeError> {
    // BigInt subtraction (if either is BigInt)
    if (left.is_pointer() || right.is_pointer()) && !left.is_null() && !right.is_null() {
        if let (Ok(l_big), Ok(r_big)) = (as_bigint_nan(left), as_bigint_nan(right)) {
            return Ok(NanValue::from_heap(HeapValue::BigInt(l_big - r_big)));
        }
    }

    let l_num = as_f64_nan(left)?;
    let r_num = as_f64_nan(right)?;
    Ok(NanValue::from_f64(l_num - r_num))
}

/// Multiply two NanValues
#[inline(always)]
pub fn abi_mul_nan(left: &NanValue, right: &NanValue) -> Result<NanValue, RuntimeError> {
    // BigInt multiplication (if either is BigInt)
    if (left.is_pointer() || right.is_pointer()) && !left.is_null() && !right.is_null() {
        if let (Ok(l_big), Ok(r_big)) = (as_bigint_nan(left), as_bigint_nan(right)) {
            return Ok(NanValue::from_heap(HeapValue::BigInt(l_big * r_big)));
        }
    }

    let l_num = as_f64_nan(left)?;
    let r_num = as_f64_nan(right)?;
    Ok(NanValue::from_f64(l_num * r_num))
}

/// Divide two NanValues
#[inline(always)]
pub fn abi_div_nan(left: &NanValue, right: &NanValue) -> Result<NanValue, RuntimeError> {
    let r_num = as_f64_nan(right)?;
    if r_num == 0.0 {
        return Err(RuntimeError::new("Division by zero"));
    }

    let l_num = as_f64_nan(left)?;
    Ok(NanValue::from_f64(l_num / r_num))
}

/// Modulo of two NanValues
#[inline(always)]
pub fn abi_mod_nan(left: &NanValue, right: &NanValue) -> Result<NanValue, RuntimeError> {
    let r_num = as_f64_nan(right)?;
    if r_num == 0.0 {
        return Err(RuntimeError::new("Modulo by zero"));
    }

    let l_num = as_f64_nan(left)?;
    Ok(NanValue::from_f64(l_num % r_num))
}

/// Negate a NanValue
#[inline(always)]
pub fn abi_negate_nan(val: &NanValue) -> Result<NanValue, RuntimeError> {
    // BigInt negation
    if val.is_pointer() {
        if let Ok(big) = as_bigint_nan(val) {
            return Ok(NanValue::from_heap(HeapValue::BigInt(-big)));
        }
    }

    let num = as_f64_nan(val)?;
    Ok(NanValue::from_f64(-num))
}

/// Compare two NanValues (less than)
#[inline(always)]
pub fn abi_cmp_lt_nan(left: &NanValue, right: &NanValue) -> Result<NanValue, RuntimeError> {
    let l_num = as_f64_nan(left)?;
    let r_num = as_f64_nan(right)?;
    Ok(NanValue::from_bool(l_num < r_num))
}

/// Compare two NanValues (less than or equal)
#[inline(always)]
pub fn abi_cmp_le_nan(left: &NanValue, right: &NanValue) -> Result<NanValue, RuntimeError> {
    let l_num = as_f64_nan(left)?;
    let r_num = as_f64_nan(right)?;
    Ok(NanValue::from_bool(l_num <= r_num))
}

/// Compare two NanValues (greater than)
#[inline(always)]
pub fn abi_cmp_gt_nan(left: &NanValue, right: &NanValue) -> Result<NanValue, RuntimeError> {
    let l_num = as_f64_nan(left)?;
    let r_num = as_f64_nan(right)?;
    Ok(NanValue::from_bool(l_num > r_num))
}

/// Compare two NanValues (greater than or equal)
#[inline(always)]
pub fn abi_cmp_ge_nan(left: &NanValue, right: &NanValue) -> Result<NanValue, RuntimeError> {
    let l_num = as_f64_nan(left)?;
    let r_num = as_f64_nan(right)?;
    Ok(NanValue::from_bool(l_num >= r_num))
}

/// Compare two NanValues for equality
#[inline(always)]
pub fn abi_cmp_eq_nan(left: &NanValue, right: &NanValue) -> Result<NanValue, RuntimeError> {
    Ok(NanValue::from_bool(abi_equals_nan(left, right)))
}

/// Compare two NanValues for inequality
#[inline(always)]
pub fn abi_cmp_ne_nan(left: &NanValue, right: &NanValue) -> Result<NanValue, RuntimeError> {
    Ok(NanValue::from_bool(!abi_equals_nan(left, right)))
}

/// Check equality of two NanValues (deep comparison)
#[inline]
pub fn abi_equals_nan(left: &NanValue, right: &NanValue) -> bool {
    // Fast path: exact same bits
    if left.as_bits() == right.as_bits() {
        // Special case: Float NaN != NaN
        if left.is_double() && left.as_f64().unwrap().is_nan() {
            return false;
        }
        return true;
    }

    if left.is_null() && right.is_null() {
        return true;
    }

    if left.is_bool() && right.is_bool() {
        return left.as_bool().unwrap() == right.as_bool().unwrap();
    }

    if left.is_char() && right.is_char() {
        return left.as_char().unwrap() == right.as_char().unwrap();
    }

    // Char vs 1-character String comparison
    if left.is_char() && right.is_pointer() {
        if let Some(r_arc) = right.as_heap() {
            if let HeapValue::String(s) = &*r_arc {
                let c = left.as_char().unwrap();
                return s.len() == 1 && s.chars().next() == Some(c);
            }
        }
    }
    if left.is_pointer() && right.is_char() {
        if let Some(l_arc) = left.as_heap() {
            if let HeapValue::String(s) = &*l_arc {
                let c = right.as_char().unwrap();
                return s.len() == 1 && s.chars().next() == Some(c);
            }
        }
    }

    // BigInt exact comparison
    if let (Ok(l_big), Ok(r_big)) = (as_bigint_nan(left), as_bigint_nan(right)) {
        return l_big == r_big;
    }

    // Numeric cross-comparison (double, int48)
    if let (Ok(l), Ok(r)) = (as_f64_nan(left), as_f64_nan(right)) {
        if l.is_nan() || r.is_nan() {
            return false;
        }
        return (l - r).abs() < 1e-12;
    }

    // Heap value comparison
    if left.is_pointer() && right.is_pointer() {
        if let (Some(l_arc), Some(r_arc)) = (left.as_heap(), right.as_heap()) {
            match (&*l_arc, &*r_arc) {
                (HeapValue::String(l_str), HeapValue::String(r_str)) => return l_str == r_str,
                (HeapValue::BigInt(l_big), HeapValue::BigInt(r_big)) => return l_big == r_big,
                (HeapValue::Array(l_arr), HeapValue::Array(r_arr)) => {
                    if l_arr.len() != r_arr.len() {
                        return false;
                    }
                    for (l_elem, r_elem) in l_arr.iter().zip(r_arr.iter()) {
                        if !abi_equals_nan(l_elem, r_elem) {
                            return false;
                        }
                    }
                    return true;
                }
                (HeapValue::Object(l_obj), HeapValue::Object(r_obj)) => {
                    if l_obj.len() != r_obj.len() {
                        return false;
                    }
                    for (key, l_val) in l_obj.iter() {
                        if let Some(r_val) = r_obj.get(key) {
                            if !abi_equals_nan(l_val, r_val) {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    }
                    return true;
                }
                _ => return false,
            }
        }
    }

    false
}

/// Logical NOT of a NanValue
#[inline(always)]
pub fn abi_not_nan(val: &NanValue) -> NanValue {
    NanValue::from_bool(is_falsy_nan(val))
}

/// Check if a NanValue is falsy (null, false, 0, NaN, "")
#[inline]
pub fn is_falsy_nan(val: &NanValue) -> bool {
    if val.is_null() {
        return true;
    }

    if val.is_bool() {
        return !val.as_bool().unwrap();
    }

    if val.is_double() {
        let f = val.as_f64().unwrap();
        return f == 0.0 || f.is_nan();
    }

    if val.is_int48() {
        return val.as_i48().unwrap() == 0;
    }

    if val.is_pointer() {
        if let Some(arc) = val.as_heap() {
            match &*arc {
                HeapValue::String(s) => return s.is_empty(),
                HeapValue::BigInt(b) => return *b == BigInt::from(0),
                HeapValue::Array(a) => return a.is_empty(),
                HeapValue::Object(o) => return o.is_empty(),
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abi_add_nan_numbers() {
        let a = NanValue::from_f64(5.0);
        let b = NanValue::from_f64(3.0);
        let result = abi_add_nan(&a, &b).unwrap();
        assert!(result.is_double());
        assert_eq!(result.as_f64().unwrap(), 8.0);
    }

    #[test]
    fn test_abi_add_nan_strings() {
        let a = NanValue::from_heap(HeapValue::String("Hello".to_string()));
        let b = NanValue::from_heap(HeapValue::String(" World".to_string()));
        let result = abi_add_nan(&a, &b).unwrap();
        assert!(result.is_pointer());
        if let Some(arc) = result.as_heap() {
            if let HeapValue::String(s) = &*arc {
                assert_eq!(s, "Hello World");
            } else {
                panic!("Expected string");
            }
        } else {
            panic!("Expected heap value");
        }
    }

    #[test]
    fn test_abi_sub_nan() {
        let a = NanValue::from_f64(10.0);
        let b = NanValue::from_f64(3.0);
        let result = abi_sub_nan(&a, &b).unwrap();
        assert_eq!(result.as_f64().unwrap(), 7.0);
    }

    #[test]
    fn test_abi_mul_nan() {
        let a = NanValue::from_f64(4.0);
        let b = NanValue::from_f64(3.0);
        let result = abi_mul_nan(&a, &b).unwrap();
        assert_eq!(result.as_f64().unwrap(), 12.0);
    }

    #[test]
    fn test_abi_div_nan() {
        let a = NanValue::from_f64(12.0);
        let b = NanValue::from_f64(3.0);
        let result = abi_div_nan(&a, &b).unwrap();
        assert_eq!(result.as_f64().unwrap(), 4.0);
    }

    #[test]
    fn test_abi_div_nan_by_zero() {
        let a = NanValue::from_f64(12.0);
        let b = NanValue::from_f64(0.0);
        let result = abi_div_nan(&a, &b);
        assert!(result.is_err());
    }

    #[test]
    fn test_abi_mod_nan() {
        let a = NanValue::from_f64(10.0);
        let b = NanValue::from_f64(3.0);
        let result = abi_mod_nan(&a, &b).unwrap();
        assert_eq!(result.as_f64().unwrap(), 1.0);
    }

    #[test]
    fn test_abi_negate_nan() {
        let a = NanValue::from_f64(5.0);
        let result = abi_negate_nan(&a).unwrap();
        assert_eq!(result.as_f64().unwrap(), -5.0);
    }

    #[test]
    fn test_abi_cmp_lt_nan() {
        let a = NanValue::from_f64(3.0);
        let b = NanValue::from_f64(5.0);
        let result = abi_cmp_lt_nan(&a, &b).unwrap();
        assert!(result.as_bool().unwrap());

        let result2 = abi_cmp_lt_nan(&b, &a).unwrap();
        assert!(!result2.as_bool().unwrap());
    }

    #[test]
    fn test_abi_cmp_eq_nan() {
        let a = NanValue::from_f64(5.0);
        let b = NanValue::from_f64(5.0);
        let result = abi_cmp_eq_nan(&a, &b).unwrap();
        assert!(result.as_bool().unwrap());

        let c = NanValue::from_f64(3.0);
        let result2 = abi_cmp_eq_nan(&a, &c).unwrap();
        assert!(!result2.as_bool().unwrap());
    }

    #[test]
    fn test_abi_not_nan() {
        let t = NanValue::from_bool(true);
        let result = abi_not_nan(&t);
        assert!(!result.as_bool().unwrap());

        let f = NanValue::from_bool(false);
        let result2 = abi_not_nan(&f);
        assert!(result2.as_bool().unwrap());
    }

    #[test]
    fn test_is_falsy_nan() {
        assert!(is_falsy_nan(&NanValue::null()));
        assert!(is_falsy_nan(&NanValue::from_bool(false)));
        assert!(is_falsy_nan(&NanValue::from_f64(0.0)));
        assert!(is_falsy_nan(&NanValue::from_int(0)));
        assert!(!is_falsy_nan(&NanValue::from_bool(true)));
        assert!(!is_falsy_nan(&NanValue::from_f64(1.0)));
        assert!(!is_falsy_nan(&NanValue::from_int(1)));
    }
}
