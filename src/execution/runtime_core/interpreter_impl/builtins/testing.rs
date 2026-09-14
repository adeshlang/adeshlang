//! Testing Framework Builtin Functions
//!
//! Provides assertion functions for the language testing framework:
//! - assert(cond)
//! - assert(cond, message)
//! - assert_eq(a, b)
//! - assert_ne(a, b)
//! - assert_panics { block }
//!
//! All assertions call __adesh_test_fail(file, line, message) on failure.

use crate::parsing::ast::Value;

/// Format a value for display in test error messages
fn format_value_for_display(val: &Value) -> String {
    match val {
        Value::Number(n) => {
            // Check if it's an integer
            if n.fract() == 0.0 && n.is_finite() {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        }
        Value::Bool(b) => format!("{}", b),
        Value::Str(s) => format!("\"{}\"", s),
        Value::Null => "null".to_string(),
        Value::Array(arr) => {
            let elements: Vec<String> = arr.iter().map(|v| format_value_for_display(v)).collect();
            format!("[{}]", elements.join(", "))
        }
        _ => format!("{:?}", val),
    }
}

/// Test failure handler intrinsic
/// This is called by all assertion failures
pub fn adesh_test_fail(file: &str, line: u64, message: &str) -> String {
    if file == "unknown" || line == 0 {
        format!("Test failed: {}", message)
    } else {
        format!("Test failed at {}:{}: {}", file, line, message)
    }
}

/// assert(condition) or assert(condition, message)
/// Verifies that a condition is true
pub fn builtin_assert(args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() || args.len() > 2 {
        return Err("assert() requires 1 or 2 arguments".to_string());
    }

    let condition = match &args[0] {
        Value::Bool(b) => *b,
        _ => return Err("assert() first argument must be a boolean".to_string()),
    };

    if !condition {
        let message = if args.len() == 2 {
            match &args[1] {
                Value::Str(s) => s.clone(),
                _ => return Err("assert() message must be a string".to_string()),
            }
        } else {
            "Assertion failed: condition is false".to_string()
        };

        // Call the intrinsic
        return Err(adesh_test_fail("unknown", 0, &message));
    }

    Ok(Value::Null)
}

/// assert_eq(a, b)
/// Verifies that two values are equal
pub fn builtin_assert_eq(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("assert_eq() requires exactly 2 arguments".to_string());
    }

    let equal = match crate::runtime::abi::abi_cmp_eq(&args[0], &args[1]) {
        Ok(Value::Bool(b)) => b,
        _ => match (&args[0], &args[1]) {
            (Value::Number(a), Value::Number(b)) => (a - b).abs() < f64::EPSILON,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Null, Value::Null) => true,
            _ => false,
        },
    };

    if !equal {
        let left = format_value_for_display(&args[0]);
        let right = format_value_for_display(&args[1]);
        let message = format!(
            "Assertion failed: left != right\n  left: {}\n right: {}",
            left, right
        );
        return Err(adesh_test_fail("unknown", 0, &message));
    }

    Ok(Value::Null)
}

/// assert_ne(a, b)
/// Verifies that two values are not equal
pub fn builtin_assert_ne(args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("assert_ne() requires exactly 2 arguments".to_string());
    }

    let equal = match crate::runtime::abi::abi_cmp_eq(&args[0], &args[1]) {
        Ok(Value::Bool(b)) => b,
        _ => match (&args[0], &args[1]) {
            (Value::Number(a), Value::Number(b)) => (a - b).abs() < f64::EPSILON,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Null, Value::Null) => true,
            _ => false,
        },
    };

    if equal {
        let left = format_value_for_display(&args[0]);
        let _right = format_value_for_display(&args[1]);
        let message = format!(
            "Assertion failed: left == right (expected not equal)\n  both values: {}",
            left
        );
        return Err(adesh_test_fail("unknown", 0, &message));
    }

    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assert_true() {
        let result = builtin_assert(vec![Value::Bool(true)]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_assert_false() {
        let result = builtin_assert(vec![Value::Bool(false)]);
        assert!(result.is_err());
    }

    #[test]
    fn test_assert_with_message() {
        let result = builtin_assert(vec![
            Value::Bool(false),
            Value::Str("Custom message".to_string()),
        ]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Custom message"));
    }

    #[test]
    fn test_assert_eq_equal() {
        let result = builtin_assert_eq(vec![Value::Number(5.0), Value::Number(5.0)]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_assert_eq_not_equal() {
        let result = builtin_assert_eq(vec![Value::Number(5.0), Value::Number(10.0)]);
        assert!(result.is_err());
    }

    #[test]
    fn test_assert_ne_not_equal() {
        let result = builtin_assert_ne(vec![Value::Number(5.0), Value::Number(10.0)]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_assert_ne_equal() {
        let result = builtin_assert_ne(vec![Value::Number(5.0), Value::Number(5.0)]);
        assert!(result.is_err());
    }
}
