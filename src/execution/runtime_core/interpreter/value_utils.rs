//! Value utility functions for the interpreter.
//!
//! Provides utility functions for working with runtime values including:
//! - Size calculation
//! - Type name extraction
//! - Copy trait detection
//! - JSON conversion

use crate::execution::runtime_core::ops::equals;
use crate::parsing::ast::Value;
use rustc_hash::FxHashMap as HashMap;
use std::sync::Arc;

/// Check if two values are equal (public API).
pub fn public_equals(a: &Value, b: &Value) -> bool {
    equals(a, b)
}

/// Convert a serde_json::Value to a language Value.
pub fn json_to_value(v: &serde_json::Value) -> Value {
    use serde_json::Value as JV;
    match v {
        JV::Null => Value::Null,
        JV::Bool(b) => Value::Bool(*b),
        JV::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Number(i as f64)
            } else if let Some(f) = n.as_f64() {
                Value::Number(f)
            } else {
                Value::Null
            }
        }
        JV::String(s) => Value::Str(s.clone()),
        JV::Array(a) => Value::Array(a.iter().map(json_to_value).collect()),
        JV::Object(m) => {
            let mut hm = HashMap::default();
            for (k, vv) in m.iter() {
                hm.insert(k.clone(), json_to_value(vv));
            }
            Value::Object(Arc::new(hm))
        }
    }
}

/// Calculate the size of a Value in bytes.
///
/// Provides approximate memory footprint for memory statistics.
pub(in crate::execution::runtime_core) fn sizeof_value(value: &Value) -> usize {
    match value {
        Value::Number(n) => {
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
        Value::BigInt(_) => 16,   // BigInt typically ~16 bytes minimum
        Value::Bool(_) => 1,      // bool = 1 byte
        Value::Char(_) => 4,      // char = up to 4 bytes (UTF-8)
        Value::Null => 0,         // null = 0 bytes (no data)
        Value::Str(s) => s.len(), // String size in bytes
        Value::Ref(inner, _) => sizeof_value(inner.as_ref()) + 8, // borrowed view + handle
        // Fixed-width integer types
        Value::U8(_) => 1,
        Value::U16(_) => 2,
        Value::U32(_) => 4,
        Value::U64(_) => 8,
        Value::U128(_) => 16,
        Value::I8(_) => 1,
        Value::I16(_) => 2,
        Value::I32(_) => 4,
        Value::I64(_) => 8,
        Value::I128(_) => 16,
        Value::F32(_) => 4,
        Value::F64(_) => 8,
        Value::Array(arr) => {
            // 24 bytes base (Vec metadata: ptr, len, capacity)
            let base = 24;
            let elements: usize = arr.iter().map(sizeof_value).sum();
            base + elements
        }
        Value::RawArray(_elem_type, arr) => {
            // Raw array: NO metadata overhead, just raw element data (like C arrays)
            arr.iter().map(sizeof_value).sum()
        }
        Value::Tuple(tup) => {
            let base = 24;
            let elements: usize = tup.iter().map(sizeof_value).sum();
            base + elements
        }
        Value::Set(set) => {
            let base = 24;
            let elements: usize = set.iter().map(sizeof_value).sum();
            base + elements
        }
        Value::Object(obj) => {
            // 48 bytes base (HashMap metadata)
            let base = 48;
            let contents: usize = obj.iter().map(|(k, v)| k.len() + sizeof_value(v)).sum();
            base + contents
        }
        Value::Class(c) => {
            // Class name plus method info
            c.name.len() + 100 // Approximate overhead
        }
        Value::Instance(i) => {
            // Class name plus fields
            let base = 50; // Approximate struct overhead
            // Prefer counting packed layout when available
            if let (Some(layout), Some(_raw)) = (&i.layout, &i.raw) {
                base + layout.get_final_size()
            } else {
                let fields: usize = i
                    .fields
                    .read()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| k.len() + sizeof_value(v))
                    .sum();
                base + fields
            }
        }
        Value::UserFunction(f) => {
            // Function name + params + body reference
            f.name.len() + 50 // Approximate
        }
        Value::Function(_) => 8, // Native function pointer
        Value::BoundMethod(f, _) => f.name.len() + 50,
        Value::Promise(_) => 8, // Promise ID
        Value::Enum(_) => 100,  // Approximate enum size
        Value::EnumCtor(_, _) => 50,
        Value::Super(_, _) => 50,
        Value::Complex(_, _) => 16, // Two f64 values
        Value::Struct(_) => 100,
        Value::Interface(_) => 100,
        Value::BoundNative(_, _) => 16,
        Value::Error(_) => 50,
        Value::DynArray(da) => da.total_bytes(),
        Value::Share(strong_ref) => unsafe {
            let obj = &*strong_ref.ptr;
            16 + sizeof_value(&obj.value)
        },
        Value::Weak(_) => 8,
        Value::LazyRange(..) => 24,
    }
}

/// Decide if a runtime Value should be treated as Copy by default.
#[inline]
pub(in crate::execution::runtime_core) fn is_copy_value(v: &Value) -> bool {
    match v {
        // Primitive scalars: default Copy
        Value::Null
        | Value::Bool(_)
        | Value::Number(_)
        | Value::Char(_)
        // Fixed-width numeric types
        | Value::U8(_)
        | Value::U16(_)
        | Value::U32(_)
        | Value::U64(_)
        | Value::U128(_)
        | Value::I8(_)
        | Value::I16(_)
        | Value::I32(_)
        | Value::I64(_)
        | Value::I128(_)
        | Value::F32(_)
        | Value::F64(_)
        // Enum constructors (like Some, None) are copyable
        | Value::EnumCtor(_, _) => true,
        Value::LazyRange(..) => false,
        // Everything else considered move-only by default
        _ => false,
    }
}

/// Get the type name of a Value for memory stats display.
pub(in crate::execution::runtime_core) fn value_type_name(value: &Value) -> &'static str {
    match value {
        Value::Number(_) => "number",
        Value::BigInt(_) => "bigint",
        Value::Bool(_) => "bool",
        Value::Char(_) => "char",
        Value::Null => "null",
        Value::Str(_) => "string",
        Value::Array(_) => "array",
        Value::RawArray(_, _) => "raw_array",
        Value::DynArray(_) => "dyn_array",
        Value::Tuple(_) => "tuple",
        Value::Set(_) => "set",
        Value::Object(_) => "object",
        Value::Class(_) => "class",
        Value::Instance(_) => "instance",
        Value::UserFunction(_) => "function",
        Value::Function(_) => "native_fn",
        Value::BoundMethod(_, _) => "method",
        Value::Promise(_) => "promise",
        Value::Enum(_) => "enum",
        Value::EnumCtor(_, _) => "enum_ctor",
        Value::Super(_, _) => "super",
        Value::Complex(_, _) => "complex",
        Value::Struct(_) => "struct",
        Value::Interface(_) => "interface",
        Value::BoundNative(_, _) => "bound_native",
        Value::Error(_) => "error",
        Value::Ref(_, _) => "ref",
        // Fixed-width integer types
        Value::U8(_) => "u8",
        Value::U16(_) => "u16",
        Value::U32(_) => "u32",
        Value::U64(_) => "u64",
        Value::U128(_) => "u128",
        Value::I8(_) => "i8",
        Value::I16(_) => "i16",
        Value::I32(_) => "i32",
        Value::I64(_) => "i64",
        Value::I128(_) => "i128",
        Value::F32(_) => "f32",
        Value::F64(_) => "f64",
        Value::Share(_) => "share",
        Value::Weak(_) => "weak",
        Value::LazyRange(..) => "range",
    }
}
