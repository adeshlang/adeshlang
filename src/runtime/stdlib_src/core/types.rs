use crate::parsing::ast::{BuiltinEnv, Value};
use crate::stdlib::registry::BuiltinRegistry;

pub fn register(registry: &mut BuiltinRegistry) {
    registry.register("type", "core", "Return type name of value", builtin_type);
    registry.register(
        "hasKey",
        "core",
        "Check whether an object contains a key",
        builtin_has_key,
    );
    registry.register(
        "len",
        "core",
        "Length of arrays, tuples, sets, strings, objects",
        builtin_len,
    );
    registry.register(
        "capacity",
        "core",
        "Get capacity of a dynamic array",
        builtin_capacity,
    );
    registry.register(
        "metadata_size",
        "core",
        "Get metadata overhead of an array in bytes",
        builtin_metadata_size,
    );
}

fn builtin_type(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("type(value)".to_string());
    }
    let t = match &args[0] {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) => {
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
        Value::BigInt(_) => "bigint",
        Value::Char(_) => "char",
        Value::Str(_) => "string",
        Value::Array(_) => "array",
        Value::RawArray(elem_type, _) => return Ok(Value::Str(format!("[{};raw]", elem_type))),
        Value::DynArray(da) => return Ok(Value::Str(format!("[{}]", da.concrete_type))),
        Value::Tuple(_) => "tuple",
        Value::Set(_) => "set",
        Value::Object(_) => "object",
        Value::Class(c) => &c.name,
        Value::Instance(i) => &i.class.name,
        Value::BoundMethod(_, _) => "function",
        Value::UserFunction(_) | Value::Function(_) => "function",
        Value::Enum(_) => "enum",
        Value::EnumCtor(_, _) => "enumctor",
        Value::Promise(_) => "promise",
        Value::Super(_, _) => "super",
        Value::Complex(_, _) => "complex",
        Value::Struct(_) => "struct",
        Value::Interface(_) => "interface",
        Value::BoundNative(_, _) => "function",
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
        Value::Error(le) => le.kind.label(),
        Value::Share(_) => "share",
        Value::Weak(_) => "weak",
        Value::LazyRange(..) => "range",
    };
    Ok(Value::Str(t.to_string()))
}

fn builtin_len(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("len(value)".to_string());
    }
    match &args[0] {
        Value::Array(a) => Ok(Value::Number(a.len() as f64)),
        Value::RawArray(_, a) => Ok(Value::Number(a.len() as f64)),
        Value::DynArray(da) => Ok(Value::Number(da.len() as f64)),
        Value::Tuple(t) => Ok(Value::Number(t.len() as f64)),
        Value::Set(s) => Ok(Value::Number(s.len() as f64)),
        Value::Str(s) => Ok(Value::Number(s.len() as f64)),
        Value::Object(m) => Ok(Value::Number(m.len() as f64)),
        _ => Err("len unsupported".to_string()),
    }
}

fn builtin_has_key(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("hasKey(object, key)".to_string());
    }

    let key = match &args[1] {
        Value::Str(s) => s.clone(),
        Value::Number(n) => {
            if n.fract() == 0.0 {
                (*n as i64).to_string()
            } else {
                n.to_string()
            }
        }
        Value::Char(c) => c.to_string(),
        Value::U8(n) => n.to_string(),
        Value::U16(n) => n.to_string(),
        Value::U32(n) => n.to_string(),
        Value::U64(n) => n.to_string(),
        Value::U128(n) => n.to_string(),
        Value::I8(n) => n.to_string(),
        Value::I16(n) => n.to_string(),
        Value::I32(n) => n.to_string(),
        Value::I64(n) => n.to_string(),
        Value::I128(n) => n.to_string(),
        Value::F32(n) => {
            if n.fract() == 0.0 {
                (*n as i64).to_string()
            } else {
                n.to_string()
            }
        }
        Value::F64(n) => {
            if n.fract() == 0.0 {
                (*n as i64).to_string()
            } else {
                n.to_string()
            }
        }
        _ => return Ok(Value::Bool(false)),
    };

    match &args[0] {
        Value::Object(m) => Ok(Value::Bool(m.contains_key(&key))),
        _ => Ok(Value::Bool(false)),
    }
}

/// Get the capacity of a dynamic array
fn builtin_capacity(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("capacity(array)".to_string());
    }
    match &args[0] {
        Value::Array(a) => Ok(Value::Number(a.capacity() as f64)),
        Value::DynArray(da) => Ok(Value::Number(da.capacity() as f64)),
        Value::RawArray(_, a) => Ok(Value::Number(a.len() as f64)), // Raw arrays have no extra capacity
        _ => Err("capacity() only works on arrays".to_string()),
    }
}

/// Get the metadata size overhead of an array in bytes
fn builtin_metadata_size(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("metadata_size(array)".to_string());
    }
    match &args[0] {
        Value::Array(_) => Ok(Value::Number(24.0)), // Standard Vec: ptr(8) + len(8) + cap(8)
        Value::DynArray(da) => Ok(Value::Number(da.metadata_bytes() as f64)),
        Value::RawArray(_, _) => Ok(Value::Number(0.0)), // Raw arrays have no metadata
        Value::Tuple(t) => Ok(Value::Number((8 + t.len() * 8) as f64)), // Approximate tuple metadata
        _ => Err("metadata_size() only works on arrays and tuples".to_string()),
    }
}
