//! Evaluation Helper Functions (Phase 3: PR3)
//!
//! Common utilities for expression evaluation. These helpers reduce code duplication
//! and provide a single place to optimize common patterns.

use crate::parsing::ast::Value;
use std::fmt::Write as _;

/// Creates a formatted error message
#[inline]
pub fn err(msg: impl Into<String>) -> String {
    msg.into()
}

/// Creates a formatted error with context
#[inline]
pub fn err_with_context(operation: &str, detail: &str) -> String {
    format!("{}: {}", operation, detail)
}

/// Formats a value for error messages
///
/// This is optimized to avoid repeated string allocations in error paths.
pub fn format_value_type(val: &Value) -> &'static str {
    match val {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::Str(_) => "string",
        Value::Char(_) => "char",
        Value::Array(_) | Value::DynArray(_) | Value::RawArray(_, _) => "array",
        Value::Tuple(_) => "tuple",
        Value::Object(_) => "object",
        Value::Set(_) => "set",
        Value::LazyRange(..) => "range",
        Value::Complex(_, _) => "complex",
        Value::U8(_) | Value::U16(_) | Value::U32(_) | Value::U64(_) | Value::U128(_) => "uint",
        Value::I8(_) | Value::I16(_) | Value::I32(_) | Value::I64(_) | Value::I128(_) => "int",
        Value::F32(_) | Value::F64(_) => "float",
        Value::BigInt(_) => "bigint",
        Value::Function(_) | Value::BoundNative(_, _) => "function",
        Value::UserFunction(_) => "function",
        Value::Class(_) => "class",
        Value::Struct(_) => "struct",
        Value::Enum(_) => "enum",
        Value::Interface(_) => "interface",
        Value::EnumCtor(_, _) => "enum constructor",
        Value::Instance(_) => "instance",
        Value::BoundMethod(_, _) => "method",
        Value::Super(_, _) => "super",
        Value::Promise(_) => "promise",
        Value::Error(_) => "error",
        Value::Ref(_, _) => "reference",
        Value::Share(_) => "share",
        Value::Weak(_) => "weak",
    }
}

/// Checks if a value is truthy (JavaScript-like semantics)
#[inline]
pub fn is_truthy(val: &Value) -> bool {
    match val {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::Number(n) => *n != 0.0 && !n.is_nan(),
        Value::Str(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::DynArray(d) => !d.data.is_empty(),
        Value::Object(o) => !o.is_empty(),
        Value::LazyRange(..) => true,
        _ => true,
    }
}

/// Checks if a value is numeric
#[inline]
pub fn is_numeric(val: &Value) -> bool {
    matches!(val, Value::Number(_) | Value::BigInt(_))
}

/// Checks if a value is a string
#[inline]
pub fn is_string(val: &Value) -> bool {
    matches!(val, Value::Str(_))
}

/// Checks if a value is callable
#[inline]
pub fn is_callable(val: &Value) -> bool {
    matches!(
        val,
        Value::Function(_) | Value::UserFunction(_) | Value::BoundMethod(_, _) | Value::Class(_)
    )
}

/// Creates a type mismatch error message
pub fn type_mismatch_error(operation: &str, expected: &str, got: &Value) -> String {
    format!(
        "{} expects {}, got {}",
        operation,
        expected,
        format_value_type(got)
    )
}

/// Creates an arity error message
pub fn arity_error(function: &str, expected: usize, got: usize) -> String {
    format!("{} expects {} argument(s), got {}", function, expected, got)
}

/// Converts a value to a string for display
///
/// This is optimized to avoid unnecessary allocations.
pub fn value_to_display_string(val: &Value) -> String {
    match val {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => {
            // Optimize common cases
            if n.fract() == 0.0 && n.abs() < 1e10 {
                format!("{:.0}", n)
            } else {
                n.to_string()
            }
        }
        Value::Str(s) => s.clone(),
        Value::BigInt(b) => b.to_string(),
        Value::Array(arr) => {
            let mut s = String::from("[");
            for (i, v) in arr.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                s.push_str(&value_to_display_string(v));
                // Limit size for very large arrays
                if i > 100 {
                    s.push_str(", ...");
                    break;
                }
            }
            s.push(']');
            s
        }
        Value::Object(obj) => {
            let mut s = String::from("{");
            for (i, (k, v)) in obj.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                write!(s, "{}: {}", k, value_to_display_string(v)).ok();
                // Limit size for very large objects
                if i > 20 {
                    s.push_str(", ...");
                    break;
                }
            }
            s.push('}');
            s
        }
        _ => format!("[{}]", format_value_type(val)),
    }
}

/// Optimized equality check helper
///
/// Avoids unnecessary allocations for common cases.
#[inline]
pub fn values_equal(left: &Value, right: &Value) -> bool {
    // Fast path for identical references
    if std::ptr::eq(left, right) {
        return true;
    }

    // Manual equality check since Value doesn't derive PartialEq
    match (left, right) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Number(a), Value::Number(b)) => {
            // Handle NaN specially
            if a.is_nan() && b.is_nan() {
                return true;
            }
            a == b
        }
        (Value::Str(a), Value::Str(b)) => a == b,
        (Value::BigInt(a), Value::BigInt(b)) => a == b,
        // For complex types, return false (caller should use proper comparison)
        // This is a simplified helper; full equality requires the interpreter's equals() function
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_truthy() {
        assert!(is_truthy(&Value::Bool(true)));
        assert!(!is_truthy(&Value::Bool(false)));
        assert!(!is_truthy(&Value::Null));
        assert!(is_truthy(&Value::Number(1.0)));
        assert!(!is_truthy(&Value::Number(0.0)));
        assert!(is_truthy(&Value::Str("hello".to_string())));
        assert!(!is_truthy(&Value::Str(String::new())));
    }

    #[test]
    fn test_format_value_type() {
        assert_eq!(format_value_type(&Value::Null), "null");
        assert_eq!(format_value_type(&Value::Bool(true)), "bool");
        assert_eq!(format_value_type(&Value::Number(42.0)), "number");
        assert_eq!(format_value_type(&Value::Str("test".to_string())), "string");
    }

    #[test]
    fn test_is_callable() {
        assert!(!is_callable(&Value::Null));
        assert!(!is_callable(&Value::Number(42.0)));
        // UserFunction would need proper construction for full test
    }

    #[test]
    fn test_values_equal() {
        assert!(values_equal(&Value::Null, &Value::Null));
        assert!(values_equal(&Value::Bool(true), &Value::Bool(true)));
        assert!(!values_equal(&Value::Bool(true), &Value::Bool(false)));
        assert!(values_equal(&Value::Number(42.0), &Value::Number(42.0)));
        assert!(values_equal(
            &Value::Str("test".to_string()),
            &Value::Str("test".to_string())
        ));
    }
}
