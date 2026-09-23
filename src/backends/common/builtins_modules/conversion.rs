//! Type conversion builtin functions

use super::RuntimeValue;

// ============================================================================
// Memory Safety Stubs
// ============================================================================

#[allow(dead_code)]
pub(super) fn runtime_borrow_immut(args: &[RuntimeValue]) -> RuntimeValue {
    // Borrow checking is done at compile-time in borrow_check.rs
    // This is a runtime no-op that just passes the value through
    if !args.is_empty() {
        args[0].clone()
    } else {
        RuntimeValue::Null
    }
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_borrow_mut(args: &[RuntimeValue]) -> RuntimeValue {
    // Borrow checking is done at compile-time in borrow_check.rs
    // This is a runtime no-op that just passes the value through
    if !args.is_empty() {
        args[0].clone()
    } else {
        RuntimeValue::Null
    }
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_borrow_release(args: &[RuntimeValue]) -> RuntimeValue {
    // Borrow checking is done at compile-time in borrow_check.rs
    // This is a runtime no-op that marks borrow release scope end
    if !args.is_empty() {
        args[0].clone()
    } else {
        RuntimeValue::Null
    }
}

// ============================================================================
// Type Conversion Functions
// ============================================================================

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_int(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Int(0);
    }
    match &args[0] {
        RuntimeValue::Int(n) => RuntimeValue::Int(*n),
        RuntimeValue::Float(n) => RuntimeValue::Int(*n as i64),
        RuntimeValue::Bool(b) => RuntimeValue::Int(if *b { 1 } else { 0 }),
        RuntimeValue::String(s) => RuntimeValue::Int(s.parse::<i64>().unwrap_or(0)),
        _ => RuntimeValue::Int(0),
    }
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_float(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Float(0.0);
    }
    match &args[0] {
        RuntimeValue::Int(n) => RuntimeValue::Float(*n as f64),
        RuntimeValue::Float(n) => RuntimeValue::Float(*n),
        RuntimeValue::Bool(b) => RuntimeValue::Float(if *b { 1.0 } else { 0.0 }),
        RuntimeValue::String(s) => RuntimeValue::Float(s.parse::<f64>().unwrap_or(0.0)),
        RuntimeValue::BigInt(bi) => {
            use num_traits::ToPrimitive;
            RuntimeValue::Float(bi.to_f64().unwrap_or(0.0))
        }
        _ => RuntimeValue::Float(0.0),
    }
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_f32(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::F32(0.0);
    }
    match &args[0] {
        RuntimeValue::Int(n) => RuntimeValue::F32(*n as f32),
        RuntimeValue::Float(n) => RuntimeValue::F32(*n as f32),
        RuntimeValue::F32(n) => RuntimeValue::F32(*n),
        RuntimeValue::F64(n) => RuntimeValue::F32(*n as f32),
        RuntimeValue::Bool(b) => RuntimeValue::F32(if *b { 1.0 } else { 0.0 }),
        RuntimeValue::String(s) => RuntimeValue::F32(s.parse::<f32>().unwrap_or(0.0)),
        RuntimeValue::U8(n) => RuntimeValue::F32(*n as f32),
        RuntimeValue::U16(n) => RuntimeValue::F32(*n as f32),
        RuntimeValue::U32(n) => RuntimeValue::F32(*n as f32),
        RuntimeValue::U64(n) => RuntimeValue::F32(*n as f32),
        RuntimeValue::I8(n) => RuntimeValue::F32(*n as f32),
        RuntimeValue::I16(n) => RuntimeValue::F32(*n as f32),
        RuntimeValue::I32(n) => RuntimeValue::F32(*n as f32),
        RuntimeValue::I64(n) => RuntimeValue::F32(*n as f32),
        _ => RuntimeValue::F32(0.0),
    }
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_f64(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::F64(0.0);
    }
    match &args[0] {
        RuntimeValue::Int(n) => RuntimeValue::F64(*n as f64),
        RuntimeValue::Float(n) => RuntimeValue::F64(*n),
        RuntimeValue::F32(n) => RuntimeValue::F64(*n as f64),
        RuntimeValue::F64(n) => RuntimeValue::F64(*n),
        RuntimeValue::Bool(b) => RuntimeValue::F64(if *b { 1.0 } else { 0.0 }),
        RuntimeValue::String(s) => RuntimeValue::F64(s.parse::<f64>().unwrap_or(0.0)),
        RuntimeValue::U8(n) => RuntimeValue::F64(*n as f64),
        RuntimeValue::U16(n) => RuntimeValue::F64(*n as f64),
        RuntimeValue::U32(n) => RuntimeValue::F64(*n as f64),
        RuntimeValue::U64(n) => RuntimeValue::F64(*n as f64),
        RuntimeValue::I8(n) => RuntimeValue::F64(*n as f64),
        RuntimeValue::I16(n) => RuntimeValue::F64(*n as f64),
        RuntimeValue::I32(n) => RuntimeValue::F64(*n as f64),
        RuntimeValue::I64(n) => RuntimeValue::F64(*n as f64),
        _ => RuntimeValue::F64(0.0),
    }
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_u8(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::U8(0);
    }
    match &args[0] {
        RuntimeValue::Int(n) => RuntimeValue::U8(*n as u8),
        RuntimeValue::Float(n) => RuntimeValue::U8(*n as u8),
        RuntimeValue::F32(n) => RuntimeValue::U8(*n as u8),
        RuntimeValue::F64(n) => RuntimeValue::U8(*n as u8),
        RuntimeValue::Bool(b) => RuntimeValue::U8(if *b { 1 } else { 0 }),
        RuntimeValue::U8(n) => RuntimeValue::U8(*n),
        RuntimeValue::U16(n) => RuntimeValue::U8(*n as u8),
        RuntimeValue::U32(n) => RuntimeValue::U8(*n as u8),
        RuntimeValue::U64(n) => RuntimeValue::U8(*n as u8),
        RuntimeValue::I8(n) => RuntimeValue::U8(*n as u8),
        RuntimeValue::I16(n) => RuntimeValue::U8(*n as u8),
        RuntimeValue::I32(n) => RuntimeValue::U8(*n as u8),
        RuntimeValue::I64(n) => RuntimeValue::U8(*n as u8),
        _ => RuntimeValue::U8(0),
    }
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_u16(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::U16(0);
    }
    match &args[0] {
        RuntimeValue::Int(n) => RuntimeValue::U16(*n as u16),
        RuntimeValue::Float(n) => RuntimeValue::U16(*n as u16),
        RuntimeValue::F32(n) => RuntimeValue::U16(*n as u16),
        RuntimeValue::F64(n) => RuntimeValue::U16(*n as u16),
        RuntimeValue::Bool(b) => RuntimeValue::U16(if *b { 1 } else { 0 }),
        RuntimeValue::U8(n) => RuntimeValue::U16(*n as u16),
        RuntimeValue::U16(n) => RuntimeValue::U16(*n),
        RuntimeValue::U32(n) => RuntimeValue::U16(*n as u16),
        RuntimeValue::U64(n) => RuntimeValue::U16(*n as u16),
        RuntimeValue::I8(n) => RuntimeValue::U16(*n as u16),
        RuntimeValue::I16(n) => RuntimeValue::U16(*n as u16),
        RuntimeValue::I32(n) => RuntimeValue::U16(*n as u16),
        RuntimeValue::I64(n) => RuntimeValue::U16(*n as u16),
        _ => RuntimeValue::U16(0),
    }
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_u32(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::U32(0);
    }
    match &args[0] {
        RuntimeValue::Int(n) => RuntimeValue::U32(*n as u32),
        RuntimeValue::Float(n) => RuntimeValue::U32(*n as u32),
        RuntimeValue::F32(n) => RuntimeValue::U32(*n as u32),
        RuntimeValue::F64(n) => RuntimeValue::U32(*n as u32),
        RuntimeValue::Bool(b) => RuntimeValue::U32(if *b { 1 } else { 0 }),
        RuntimeValue::U8(n) => RuntimeValue::U32(*n as u32),
        RuntimeValue::U16(n) => RuntimeValue::U32(*n as u32),
        RuntimeValue::U32(n) => RuntimeValue::U32(*n),
        RuntimeValue::U64(n) => RuntimeValue::U32(*n as u32),
        RuntimeValue::BigInt(bi) => {
            RuntimeValue::U32(num_traits::ToPrimitive::to_u32(bi).unwrap_or(0))
        }
        RuntimeValue::I8(n) => RuntimeValue::U32(*n as u32),
        RuntimeValue::I16(n) => RuntimeValue::U32(*n as u32),
        RuntimeValue::I32(n) => RuntimeValue::U32(*n as u32),
        RuntimeValue::I64(n) => RuntimeValue::U32(*n as u32),
        _ => RuntimeValue::U32(0),
    }
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_u64(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::U64(0);
    }
    match &args[0] {
        RuntimeValue::Int(n) => RuntimeValue::U64(*n as u64),
        RuntimeValue::Float(n) => RuntimeValue::U64(*n as u64),
        RuntimeValue::F32(n) => RuntimeValue::U64(*n as u64),
        RuntimeValue::F64(n) => RuntimeValue::U64(*n as u64),
        RuntimeValue::Bool(b) => RuntimeValue::U64(if *b { 1 } else { 0 }),
        RuntimeValue::U8(n) => RuntimeValue::U64(*n as u64),
        RuntimeValue::U16(n) => RuntimeValue::U64(*n as u64),
        RuntimeValue::U32(n) => RuntimeValue::U64(*n as u64),
        RuntimeValue::U64(n) => RuntimeValue::U64(*n),
        RuntimeValue::BigInt(bi) => {
            RuntimeValue::U64(num_traits::ToPrimitive::to_u64(bi).unwrap_or(0))
        }
        RuntimeValue::I8(n) => RuntimeValue::U64(*n as u64),
        RuntimeValue::I16(n) => RuntimeValue::U64(*n as u64),
        RuntimeValue::I32(n) => RuntimeValue::U64(*n as u64),
        RuntimeValue::I64(n) => RuntimeValue::U64(*n as u64),
        _ => RuntimeValue::U64(0),
    }
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_u128(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::U128(0);
    }
    match &args[0] {
        RuntimeValue::Int(n) => RuntimeValue::U128(*n as u128),
        RuntimeValue::Float(n) => RuntimeValue::U128(*n as u128),
        RuntimeValue::F32(n) => RuntimeValue::U128(*n as u128),
        RuntimeValue::F64(n) => RuntimeValue::U128(*n as u128),
        RuntimeValue::Bool(b) => RuntimeValue::U128(if *b { 1 } else { 0 }),
        RuntimeValue::U8(n) => RuntimeValue::U128(*n as u128),
        RuntimeValue::U16(n) => RuntimeValue::U128(*n as u128),
        RuntimeValue::U32(n) => RuntimeValue::U128(*n as u128),
        RuntimeValue::U64(n) => RuntimeValue::U128(*n as u128),
        RuntimeValue::U128(n) => RuntimeValue::U128(*n),
        RuntimeValue::I8(n) => RuntimeValue::U128(*n as u128),
        RuntimeValue::I16(n) => RuntimeValue::U128(*n as u128),
        RuntimeValue::I32(n) => RuntimeValue::U128(*n as u128),
        RuntimeValue::I64(n) => RuntimeValue::U128(*n as u128),
        RuntimeValue::I128(n) => RuntimeValue::U128(*n as u128),
        _ => RuntimeValue::U128(0),
    }
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_i8(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::I8(0);
    }
    match &args[0] {
        RuntimeValue::Int(n) => RuntimeValue::I8(*n as i8),
        RuntimeValue::Float(n) => RuntimeValue::I8(*n as i8),
        RuntimeValue::F32(n) => RuntimeValue::I8(*n as i8),
        RuntimeValue::F64(n) => RuntimeValue::I8(*n as i8),
        RuntimeValue::Bool(b) => RuntimeValue::I8(if *b { 1 } else { 0 }),
        RuntimeValue::U8(n) => RuntimeValue::I8(*n as i8),
        RuntimeValue::U16(n) => RuntimeValue::I8(*n as i8),
        RuntimeValue::U32(n) => RuntimeValue::I8(*n as i8),
        RuntimeValue::U64(n) => RuntimeValue::I8(*n as i8),
        RuntimeValue::I8(n) => RuntimeValue::I8(*n),
        RuntimeValue::I16(n) => RuntimeValue::I8(*n as i8),
        RuntimeValue::I32(n) => RuntimeValue::I8(*n as i8),
        RuntimeValue::I64(n) => RuntimeValue::I8(*n as i8),
        _ => RuntimeValue::I8(0),
    }
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_i16(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::I16(0);
    }
    match &args[0] {
        RuntimeValue::Int(n) => RuntimeValue::I16(*n as i16),
        RuntimeValue::Float(n) => RuntimeValue::I16(*n as i16),
        RuntimeValue::F32(n) => RuntimeValue::I16(*n as i16),
        RuntimeValue::F64(n) => RuntimeValue::I16(*n as i16),
        RuntimeValue::Bool(b) => RuntimeValue::I16(if *b { 1 } else { 0 }),
        RuntimeValue::U8(n) => RuntimeValue::I16(*n as i16),
        RuntimeValue::U16(n) => RuntimeValue::I16(*n as i16),
        RuntimeValue::U32(n) => RuntimeValue::I16(*n as i16),
        RuntimeValue::U64(n) => RuntimeValue::I16(*n as i16),
        RuntimeValue::I8(n) => RuntimeValue::I16(*n as i16),
        RuntimeValue::I16(n) => RuntimeValue::I16(*n),
        RuntimeValue::I32(n) => RuntimeValue::I16(*n as i16),
        RuntimeValue::I64(n) => RuntimeValue::I16(*n as i16),
        _ => RuntimeValue::I16(0),
    }
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_i32(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::I32(0);
    }
    match &args[0] {
        RuntimeValue::Int(n) => RuntimeValue::I32(*n as i32),
        RuntimeValue::Float(n) => RuntimeValue::I32(*n as i32),
        RuntimeValue::F32(n) => RuntimeValue::I32(*n as i32),
        RuntimeValue::F64(n) => RuntimeValue::I32(*n as i32),
        RuntimeValue::Bool(b) => RuntimeValue::I32(if *b { 1 } else { 0 }),
        RuntimeValue::U8(n) => RuntimeValue::I32(*n as i32),
        RuntimeValue::U16(n) => RuntimeValue::I32(*n as i32),
        RuntimeValue::U32(n) => RuntimeValue::I32(*n as i32),
        RuntimeValue::U64(n) => RuntimeValue::I32(*n as i32),
        RuntimeValue::BigInt(bi) => {
            RuntimeValue::I32(num_traits::ToPrimitive::to_i32(bi).unwrap_or(0))
        }
        RuntimeValue::I8(n) => RuntimeValue::I32(*n as i32),
        RuntimeValue::I16(n) => RuntimeValue::I32(*n as i32),
        RuntimeValue::I32(n) => RuntimeValue::I32(*n),
        RuntimeValue::I64(n) => RuntimeValue::I32(*n as i32),
        _ => RuntimeValue::I32(0),
    }
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_i64_conv(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::I64(0);
    }
    match &args[0] {
        RuntimeValue::Int(n) => RuntimeValue::I64(*n),
        RuntimeValue::Float(n) => RuntimeValue::I64(*n as i64),
        RuntimeValue::F32(n) => RuntimeValue::I64(*n as i64),
        RuntimeValue::F64(n) => RuntimeValue::I64(*n as i64),
        RuntimeValue::Bool(b) => RuntimeValue::I64(if *b { 1 } else { 0 }),
        RuntimeValue::U8(n) => RuntimeValue::I64(*n as i64),
        RuntimeValue::U16(n) => RuntimeValue::I64(*n as i64),
        RuntimeValue::U32(n) => RuntimeValue::I64(*n as i64),
        RuntimeValue::U64(n) => RuntimeValue::I64(*n as i64),
        RuntimeValue::I8(n) => RuntimeValue::I64(*n as i64),
        RuntimeValue::I16(n) => RuntimeValue::I64(*n as i64),
        RuntimeValue::I32(n) => RuntimeValue::I64(*n as i64),
        RuntimeValue::I64(n) => RuntimeValue::I64(*n),
        _ => RuntimeValue::I64(0),
    }
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_i128(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::I128(0);
    }
    match &args[0] {
        RuntimeValue::Int(n) => RuntimeValue::I128(*n as i128),
        RuntimeValue::Float(n) => RuntimeValue::I128(*n as i128),
        RuntimeValue::F32(n) => RuntimeValue::I128(*n as i128),
        RuntimeValue::F64(n) => RuntimeValue::I128(*n as i128),
        RuntimeValue::Bool(b) => RuntimeValue::I128(if *b { 1 } else { 0 }),
        RuntimeValue::U8(n) => RuntimeValue::I128(*n as i128),
        RuntimeValue::U16(n) => RuntimeValue::I128(*n as i128),
        RuntimeValue::U32(n) => RuntimeValue::I128(*n as i128),
        RuntimeValue::U64(n) => RuntimeValue::I128(*n as i128),
        RuntimeValue::U128(n) => RuntimeValue::I128(*n as i128),
        RuntimeValue::I8(n) => RuntimeValue::I128(*n as i128),
        RuntimeValue::I16(n) => RuntimeValue::I128(*n as i128),
        RuntimeValue::I32(n) => RuntimeValue::I128(*n as i128),
        RuntimeValue::I64(n) => RuntimeValue::I128(*n as i128),
        RuntimeValue::I128(n) => RuntimeValue::I128(*n),
        _ => RuntimeValue::I128(0),
    }
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_str(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::String(String::new());
    }
    RuntimeValue::String(args[0].as_string())
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_char(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Char('\0');
    }

    match &args[0] {
        RuntimeValue::Char(c) => RuntimeValue::Char(*c),
        RuntimeValue::String(s) => RuntimeValue::Char(s.chars().next().unwrap_or('\0')),
        RuntimeValue::Int(n) => RuntimeValue::Char(char::from_u32(*n as u32).unwrap_or('\0')),
        RuntimeValue::U8(n) => RuntimeValue::Char(char::from_u32(*n as u32).unwrap_or('\0')),
        RuntimeValue::U16(n) => RuntimeValue::Char(char::from_u32(*n as u32).unwrap_or('\0')),
        RuntimeValue::U32(n) => RuntimeValue::Char(char::from_u32(*n).unwrap_or('\0')),
        RuntimeValue::U64(n) => RuntimeValue::Char(char::from_u32(*n as u32).unwrap_or('\0')),
        RuntimeValue::I8(n) => RuntimeValue::Char(char::from_u32(*n as u32).unwrap_or('\0')),
        RuntimeValue::I16(n) => RuntimeValue::Char(char::from_u32(*n as u32).unwrap_or('\0')),
        RuntimeValue::I32(n) => RuntimeValue::Char(char::from_u32(*n as u32).unwrap_or('\0')),
        RuntimeValue::I64(n) => RuntimeValue::Char(char::from_u32(*n as u32).unwrap_or('\0')),
        _ => RuntimeValue::Char('\0'),
    }
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(super) fn runtime_bool(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Bool(false);
    }
    RuntimeValue::Bool(args[0].as_bool().unwrap_or(false))
}

// ============================================================================
// Type Introspection Functions
// ============================================================================

#[allow(dead_code)]
#[allow(dead_code)]
pub(crate) fn runtime_type(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::String("undefined".to_string());
    }
    let type_name = match &args[0] {
        RuntimeValue::Int(n) => {
            if *n >= 0 {
                if *n <= u8::MAX as i64 {
                    "u8"
                } else if *n <= u16::MAX as i64 {
                    "u16"
                } else if *n <= u32::MAX as i64 {
                    "u32"
                } else {
                    "u64"
                }
            } else if *n >= i8::MIN as i64 && *n <= i8::MAX as i64 {
                "i8"
            } else if *n >= i16::MIN as i64 && *n <= i16::MAX as i64 {
                "i16"
            } else if *n >= i32::MIN as i64 && *n <= i32::MAX as i64 {
                "i32"
            } else {
                "i64"
            }
        }
        RuntimeValue::Float(n) => {
            if n.fract().abs() < 1e-12 {
                let i = *n as i128;
                if i >= 0 {
                    if i <= u8::MAX as i128 {
                        "u8"
                    } else if i <= u16::MAX as i128 {
                        "u16"
                    } else if i <= u32::MAX as i128 {
                        "u32"
                    } else if i <= u64::MAX as i128 {
                        "u64"
                    } else {
                        "u128"
                    }
                } else if i >= i8::MIN as i128 && i <= i8::MAX as i128 {
                    "i8"
                } else if i >= i16::MIN as i128 && i <= i16::MAX as i128 {
                    "i16"
                } else if i >= i32::MIN as i128 && i <= i32::MAX as i128 {
                    "i32"
                } else if i >= i64::MIN as i128 && i <= i64::MAX as i128 {
                    "i64"
                } else {
                    "i128"
                }
            } else {
                "f64"
            }
        }
        RuntimeValue::Bool(_) => "boolean",
        RuntimeValue::Char(_) => "char",
        RuntimeValue::String(_) => "string",
        RuntimeValue::Array(_) => "array",
        RuntimeValue::Set(_) => "set",
        RuntimeValue::Tuple(_) => "tuple",
        RuntimeValue::RawArray(elem_type, _) => {
            return RuntimeValue::String(format!("[{};raw]", elem_type));
        }
        RuntimeValue::DynArray { concrete_type, .. } => {
            let inner = if concrete_type.is_empty() {
                "any"
            } else {
                concrete_type.as_str()
            };
            if inner.starts_with('[') && inner.ends_with(']') {
                return RuntimeValue::String(inner.to_string());
            }
            return RuntimeValue::String(format!("[{}]", inner));
        }
        RuntimeValue::Object(obj) => {
            // Check if this is a class instance with __class__
            if let Some(RuntimeValue::String(class_name)) = obj.get("__class__") {
                return RuntimeValue::String(class_name.clone());
            }
            "object"
        }
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
    };
    RuntimeValue::String(type_name.to_string())
}

#[allow(dead_code)]
#[allow(dead_code)]
pub(crate) fn runtime_sizeof(args: &[RuntimeValue]) -> RuntimeValue {
    if args.is_empty() {
        return RuntimeValue::Int(0);
    }
    RuntimeValue::Int(sizeof_value(&args[0]) as i64)
}

/// Helper function to calculate size of a RuntimeValue
#[allow(dead_code)]
#[allow(dead_code)]
pub(crate) fn sizeof_value(value: &RuntimeValue) -> usize {
    match value {
        RuntimeValue::Int(n) => {
            if *n >= 0 {
                if *n <= u8::MAX as i64 {
                    1
                } else if *n <= u16::MAX as i64 {
                    2
                } else if *n <= u32::MAX as i64 {
                    4
                } else {
                    8
                }
            } else if *n >= i8::MIN as i64 && *n <= i8::MAX as i64 {
                1
            } else if *n >= i16::MIN as i64 && *n <= i16::MAX as i64 {
                2
            } else if *n >= i32::MIN as i64 && *n <= i32::MAX as i64 {
                4
            } else {
                8
            }
        }
        RuntimeValue::Float(n) => {
            if n.fract().abs() < 1e-12 {
                let i = *n as i128;
                if i >= 0 {
                    if i <= u8::MAX as i128 {
                        1
                    } else if i <= u16::MAX as i128 {
                        2
                    } else if i <= u32::MAX as i128 {
                        4
                    } else if i <= u64::MAX as i128 {
                        8
                    } else {
                        16
                    }
                } else if i >= i8::MIN as i128 && i <= i8::MAX as i128 {
                    1
                } else if i >= i16::MIN as i128 && i <= i16::MAX as i128 {
                    2
                } else if i >= i32::MIN as i128 && i <= i32::MAX as i128 {
                    4
                } else if i >= i64::MIN as i128 && i <= i64::MAX as i128 {
                    8
                } else {
                    16
                }
            } else {
                8
            }
        }
        RuntimeValue::Bool(_) => 1,         // bool = 1 byte
        RuntimeValue::Char(_) => 4,         // char = 4 bytes (UTF-32)
        RuntimeValue::Null => 0,            // null = 0 bytes (no data)
        RuntimeValue::String(s) => s.len(), // String size in bytes
        RuntimeValue::BigInt(bi) => {
            // BigInt size varies based on magnitude
            // Rough estimate: 8 bytes base + 4 bytes per 32-bit limb
            use num_traits::Zero;
            if bi.is_zero() {
                8
            } else {
                8 + (bi.bits() as usize / 32 + 1) * 4
            }
        }
        RuntimeValue::Array(arr) => {
            // 24 bytes base (Vec metadata: ptr, len, capacity)
            // plus sum of element sizes
            let base = 24;
            let elements: usize = arr.iter().map(sizeof_value).sum();
            base + elements
        }
        RuntimeValue::Set(set_vals) => {
            let base = 24;
            let elements: usize = set_vals.iter().map(sizeof_value).sum();
            base + elements
        }
        RuntimeValue::Tuple(tup) => {
            // Same as array: 24 bytes base + sum of element sizes
            let base = 24;
            let elements: usize = tup.iter().map(sizeof_value).sum();
            base + elements
        }
        RuntimeValue::RawArray(_, arr) => {
            // Raw arrays have no metadata overhead
            let elements: usize = arr.iter().map(sizeof_value).sum();
            elements
        }
        RuntimeValue::DynArray {
            data,
            concrete_type,
            element_type,
            ..
        } => {
            let ty_str = if !concrete_type.is_empty() {
                concrete_type.as_str()
            } else {
                element_type.as_str()
            };
            let elem_size = match ty_str {
                "u8" | "i8" | "bool" | "byte" => 1,
                "u16" | "i16" | "short" => 2,
                "u32" | "i32" | "f32" | "word" => 4,
                "u64" | "i64" | "f64" | "long" => 8,
                "u128" | "i128" | "extended" => 16,
                _ => {
                    if data.is_empty() {
                        8
                    } else {
                        sizeof_value(&data[0])
                    }
                }
            };
            let metadata = if elem_size <= 4 { 16 } else { 24 };
            (data.len() * elem_size) + metadata
        }
        RuntimeValue::Object(obj) => {
            // Object: HashMap overhead + key/value sizes
            let base = 48; // Rough HashMap overhead
            let entries: usize = obj.iter().map(|(k, v)| k.len() + sizeof_value(v)).sum();
            base + entries
        }
        RuntimeValue::Promise(_) => 8,   // PromiseId = u64 = 8 bytes
        RuntimeValue::Function(_) => 16, // Function pointer estimate
        // Fixed-width types
        RuntimeValue::U8(_) => 1,
        RuntimeValue::U16(_) => 2,
        RuntimeValue::U32(_) => 4,
        RuntimeValue::U64(_) => 8,
        RuntimeValue::U128(_) => 16,
        RuntimeValue::I8(_) => 1,
        RuntimeValue::I16(_) => 2,
        RuntimeValue::I32(_) => 4,
        RuntimeValue::I64(_) => 8,
        RuntimeValue::I128(_) => 16,
        RuntimeValue::F32(_) => 4,
        RuntimeValue::F64(_) => 8,
    }
}
