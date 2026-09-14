//! Comparison and equality operations for runtime values.
//!
//! This module provides builtin functions for comparing and checking equality of runtime values.
//! It includes both loose equality (with type coercion) and strict equality (type-sensitive),
//! as well as membership testing and type checking operations.

use super::RuntimeValue;

/// Loose equality operator (==).
///
/// Compares two values with type coercion where reasonable.
/// Numeric types are compared as floats for maximum compatibility.
pub(crate) fn runtime_eq(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Bool(false);
    }
    // Loose equality: coerces types where reasonable
    let result = match (&args[0], &args[1]) {
        (RuntimeValue::Bool(a), RuntimeValue::Bool(b)) => *a == *b,
        (RuntimeValue::String(a), RuntimeValue::String(b)) => a == b,
        (RuntimeValue::Null, RuntimeValue::Null) => true,
        (RuntimeValue::BigInt(a), RuntimeValue::BigInt(b)) => a == b,
        _ => {
            // For numeric types, try to compare as floats for maximum compatibility
            if let (Some(a), Some(b)) = (args[0].as_float(), args[1].as_float()) {
                a == b
            } else {
                false // Different types that can't be coerced = not equal
            }
        }
    };
    RuntimeValue::Bool(result)
}

/// Loose inequality operator (!=).
///
/// Returns true if values are not equal with type coercion.
pub(crate) fn runtime_ne(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Bool(true);
    }
    // Loose inequality: coerces types where reasonable
    let result = match (&args[0], &args[1]) {
        (RuntimeValue::Bool(a), RuntimeValue::Bool(b)) => *a != *b,
        (RuntimeValue::String(a), RuntimeValue::String(b)) => a != b,
        (RuntimeValue::Null, RuntimeValue::Null) => false,
        (RuntimeValue::BigInt(a), RuntimeValue::BigInt(b)) => a != b,
        _ => {
            // For numeric types, try to compare as floats for maximum compatibility
            if let (Some(a), Some(b)) = (args[0].as_float(), args[1].as_float()) {
                a != b
            } else {
                true // Different types that can't be coerced = not equal
            }
        }
    };
    RuntimeValue::Bool(result)
}

/// Strict equality operator (===).
///
/// Compares both type and value - no type coercion is performed.
pub(crate) fn runtime_strict_eq(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Bool(false);
    }
    // Strict equality: both type and value must match
    let result = match (&args[0], &args[1]) {
        (RuntimeValue::Int(a), RuntimeValue::Int(b)) => a == b,
        (RuntimeValue::Float(a), RuntimeValue::Float(b)) => a == b,
        (RuntimeValue::Bool(a), RuntimeValue::Bool(b)) => a == b,
        (RuntimeValue::String(a), RuntimeValue::String(b)) => a == b,
        (RuntimeValue::Null, RuntimeValue::Null) => true,
        _ => false, // Different types = not strictly equal
    };
    RuntimeValue::Bool(result)
}

/// Strict inequality operator (!==).
///
/// Returns true if type or value differs - no type coercion is performed.
pub(crate) fn runtime_strict_ne(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Bool(true);
    }
    // Strict inequality: type or value differs
    let result = match (&args[0], &args[1]) {
        (RuntimeValue::Int(a), RuntimeValue::Int(b)) => a != b,
        (RuntimeValue::Float(a), RuntimeValue::Float(b)) => a != b,
        (RuntimeValue::Bool(a), RuntimeValue::Bool(b)) => a != b,
        (RuntimeValue::String(a), RuntimeValue::String(b)) => a != b,
        (RuntimeValue::Null, RuntimeValue::Null) => false,
        _ => true, // Different types = strictly not equal
    };
    RuntimeValue::Bool(result)
}

/// Null coalescing operator (??).
///
/// Returns left value if not null, otherwise returns right value.
pub(crate) fn runtime_null_coalesce(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Null;
    }
    // Return left value if not null, otherwise right value
    match &args[0] {
        RuntimeValue::Null => args.get(1).cloned().unwrap_or(RuntimeValue::Null),
        other => other.clone(),
    }
}

/// Membership test operator (in).
///
/// Tests if a value exists in an array, object, or string.
/// - For arrays: checks if needle equals any element
/// - For objects: checks if key exists as a property name
/// - For strings: checks if substring is contained
pub(crate) fn runtime_in(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Bool(false);
    }
    let needle = &args[0];
    let haystack = &args[1];

    let result = match haystack {
        RuntimeValue::Array(arr) => arr.iter().any(|item| runtime_values_equal(needle, item)),
        RuntimeValue::Object(obj) => {
            if let RuntimeValue::String(key) = needle {
                obj.contains_key(key)
            } else {
                false
            }
        }
        RuntimeValue::String(s) => {
            if let RuntimeValue::String(substr) = needle {
                s.contains(substr.as_str())
            } else {
                false
            }
        }
        _ => false,
    };
    RuntimeValue::Bool(result)
}

/// Helper function to check if two runtime values are equal.
///
/// Used by `runtime_in` for array membership testing.
/// Handles cross-type numeric comparisons (Int and Float).
fn runtime_values_equal(a: &RuntimeValue, b: &RuntimeValue) -> bool {
    match (a, b) {
        (RuntimeValue::Int(x), RuntimeValue::Int(y)) => x == y,
        (RuntimeValue::Float(x), RuntimeValue::Float(y)) => x == y,
        (RuntimeValue::Int(x), RuntimeValue::Float(y)) => (*x as f64) == *y,
        (RuntimeValue::Float(x), RuntimeValue::Int(y)) => *x == (*y as f64),
        (RuntimeValue::Bool(x), RuntimeValue::Bool(y)) => x == y,
        (RuntimeValue::String(x), RuntimeValue::String(y)) => x == y,
        (RuntimeValue::Null, RuntimeValue::Null) => true,
        _ => false,
    }
}

/// Type checking operator (instanceof).
///
/// Tests if an instance is of a specific type or class.
/// Supports checking:
/// - Class instances (via __class__ property)
/// - Primitive type names (number, string, boolean, etc.)
/// - Constructor functions
/// - Special inheritance (UserError instanceof Error)
pub(crate) fn runtime_instanceof(args: &[RuntimeValue]) -> RuntimeValue {
    if args.len() < 2 {
        return RuntimeValue::Bool(false);
    }
    let instance = &args[0];
    let type_check = &args[1];

    // For class instances (Object with __class__), check class name
    if let RuntimeValue::Object(obj) = instance {
        if let Some(RuntimeValue::String(class_name)) = obj.get("__class__") {
            // Check against string type name
            if let RuntimeValue::String(type_name) = type_check {
                // Special handling for error types: UserError is instanceof both UserError and Error
                if class_name == "UserError" && (type_name == "UserError" || type_name == "Error") {
                    return RuntimeValue::Bool(true);
                }
                return RuntimeValue::Bool(class_name == type_name);
            }
            // Check against function (constructor)
            if let RuntimeValue::Function(func_info) = type_check {
                // Special handling: UserError instances are instanceof Error
                if &func_info.name == "Error" && class_name == "UserError" {
                    return RuntimeValue::Bool(true);
                }
                return RuntimeValue::Bool(&func_info.name == class_name);
            }
            // Check against object with name field (class object)
            if let RuntimeValue::Object(type_obj) = type_check {
                if let Some(RuntimeValue::String(type_name)) = type_obj.get("name") {
                    return RuntimeValue::Bool(class_name == type_name);
                }
            }
        }
    }

    // Check against type name string for primitive types
    if let RuntimeValue::String(type_name) = type_check {
        let instance_type = match instance {
            RuntimeValue::Int(_) | RuntimeValue::Float(_) => "number",
            RuntimeValue::Bool(_) => "boolean",
            RuntimeValue::Char(_) => "char",
            RuntimeValue::String(_) => "string",
            RuntimeValue::Array(_) => "array",
            RuntimeValue::Set(_) => "set",
            RuntimeValue::Tuple(_) => "tuple",
            RuntimeValue::Object(_) => "object",
            RuntimeValue::Promise(_) => "promise",
            RuntimeValue::Function(_) => "function",
            RuntimeValue::BigInt(_) => "bigint",
            RuntimeValue::Null => "null",
            // Fixed-width integer types (unsigned)
            RuntimeValue::U8(_) => "u8",
            RuntimeValue::U16(_) => "u16",
            RuntimeValue::U32(_) => "u32",
            RuntimeValue::U64(_) => "u64",
            RuntimeValue::U128(_) => "u128",
            // Fixed-width integer types (signed)
            RuntimeValue::I8(_) => "i8",
            RuntimeValue::I16(_) => "i16",
            RuntimeValue::I32(_) => "i32",
            RuntimeValue::I64(_) => "i64",
            RuntimeValue::I128(_) => "i128",
            // Fixed-width float types
            RuntimeValue::F32(_) => "f32",
            RuntimeValue::F64(_) => "f64",
            RuntimeValue::RawArray(_, _) => "array",
            RuntimeValue::DynArray { .. } => "array",
        };
        return RuntimeValue::Bool(instance_type == type_name.as_str() || type_name == "Object");
    }

    RuntimeValue::Bool(false)
}
