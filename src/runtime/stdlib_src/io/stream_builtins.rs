//! Stream Native Builtin Primitives
//!
//! Exposes low-level binary packing/unpacking, console I/O, byte slice ops, and UTF-8 conversion.

use crate::parsing::ast::{BuiltinEnv, Value};
use crate::stdlib::registry::BuiltinRegistry;
use std::io::{self, Read, Write};

pub fn register(registry: &mut BuiltinRegistry) {
    registry.register(
        "io_stdin_readline",
        "io",
        "Read line from stdin",
        builtin_stdin_readline,
    );
    registry.register(
        "io_stdin_readchar",
        "io",
        "Read character from stdin",
        builtin_stdin_readchar,
    );
    registry.register(
        "io_stdout_write",
        "io",
        "Write string to stdout",
        builtin_stdout_write,
    );
    registry.register(
        "io_stderr_write",
        "io",
        "Write string to stderr",
        builtin_stderr_write,
    );
    registry.register(
        "io_pack_number",
        "io",
        "Pack primitive number into byte array",
        builtin_pack_number,
    );
    registry.register(
        "io_unpack_number",
        "io",
        "Unpack primitive number from byte array",
        builtin_unpack_number,
    );
    registry.register(
        "io_bytes_concat",
        "io",
        "Concatenate two byte arrays",
        builtin_bytes_concat,
    );
    registry.register(
        "io_slice_bytes",
        "io",
        "Slice a byte array",
        builtin_slice_bytes,
    );
    registry.register(
        "io_utf8_encode",
        "io",
        "Encode string to byte array",
        builtin_utf8_encode,
    );
    registry.register(
        "io_utf8_decode",
        "io",
        "Decode byte array to UTF-8 string",
        builtin_utf8_decode,
    );
}

fn value_to_f64(v: &Value) -> f64 {
    match v {
        Value::Number(n) => *n,
        Value::U8(n) => *n as f64,
        Value::U16(n) => *n as f64,
        Value::U32(n) => *n as f64,
        Value::U64(n) => *n as f64,
        Value::U128(n) => *n as f64,
        Value::I8(n) => *n as f64,
        Value::I16(n) => *n as f64,
        Value::I32(n) => *n as f64,
        Value::I64(n) => *n as f64,
        Value::I128(n) => *n as f64,
        Value::F32(n) => *n as f64,
        Value::F64(n) => *n,
        Value::BigInt(bi) => bi.to_string().parse::<f64>().unwrap_or(0.0),
        Value::Char(c) => *c as u32 as f64,
        Value::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Value::Str(s) => s.parse::<f64>().unwrap_or(0.0),
        Value::Instance(inst) => {
            if let Some(val) = inst.get_field("_val") {
                value_to_f64(&val)
            } else if let Some(val) = inst.get_field("value") {
                value_to_f64(&val)
            } else {
                0.0
            }
        }
        _ => 0.0,
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Char(c) => c.to_string(),
        Value::Null => "null".to_string(),
        _ => format!("{:?}", v),
    }
}

fn builtin_stdin_readline(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut input = String::new();
    let stdin = io::stdin();
    match stdin.read_line(&mut input) {
        Ok(_) => {
            if input.ends_with('\n') {
                input.pop();
                if input.ends_with('\r') {
                    input.pop();
                }
            }
            Ok(Value::Str(input))
        }
        Err(e) => Err(format!("stdin read error: {}", e)),
    }
}

fn builtin_stdin_readchar(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut buf = [0u8; 1];
    let stdin = io::stdin();
    match stdin.lock().read_exact(&mut buf) {
        Ok(_) => Ok(Value::Str((buf[0] as char).to_string())),
        Err(_) => Ok(Value::Str("".to_string())),
    }
}

fn builtin_stdout_write(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let text = match args.first() {
        Some(v) => value_to_string(v),
        None => String::new(),
    };
    print!("{}", text);
    let _ = io::stdout().flush();
    Ok(Value::Null)
}

fn builtin_stderr_write(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let text = match args.first() {
        Some(v) => value_to_string(v),
        None => String::new(),
    };
    eprint!("{}", text);
    let _ = io::stderr().flush();
    Ok(Value::Null)
}

fn extract_bytes(val: &Value) -> Result<Vec<u8>, String> {
    match val {
        Value::Array(arr) => {
            let mut bytes = Vec::with_capacity(arr.len());
            for elem in arr {
                bytes.push(value_to_f64(elem) as u8);
            }
            Ok(bytes)
        }
        Value::RawArray(_, arr) => {
            let mut bytes = Vec::with_capacity(arr.len());
            for elem in arr {
                bytes.push(value_to_f64(elem) as u8);
            }
            Ok(bytes)
        }
        Value::DynArray(da) => {
            let mut bytes = Vec::with_capacity(da.data.len());
            for elem in &da.data {
                bytes.push(value_to_f64(elem) as u8);
            }
            Ok(bytes)
        }
        Value::Str(s) => Ok(s.as_bytes().to_vec()),
        _ => Err(format!("Expected array or string of bytes, got {:?}", val)),
    }
}

fn to_value_array(bytes: &[u8]) -> Value {
    Value::Array(bytes.iter().map(|&b| Value::Number(b as f64)).collect())
}

fn builtin_pack_number(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 3 {
        return Err("io_pack_number requires (type_str, val, endian_str)".to_string());
    }
    let type_str = match &args[0] {
        Value::Str(s) => s.as_str(),
        _ => return Err("type_str must be a string".to_string()),
    };
    let is_big_endian = match &args[2] {
        Value::Str(s) => s == "big" || s == "BE",
        _ => false,
    };

    let num = value_to_f64(&args[1]);
    let mut bytes = Vec::new();

    match type_str {
        "u8" | "i8" | "byte" | "bool" => {
            bytes.push(num as u8);
        }
        "u16" | "i16" => {
            let v = (num as i32) as i16;
            let b = if is_big_endian {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            };
            bytes.extend_from_slice(&b);
        }
        "u32" | "i32" | "isize" | "usize" => {
            let v = num as i32;
            if is_big_endian {
                bytes.extend_from_slice(&v.to_be_bytes());
            } else {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        "u64" | "i64" => {
            let v = num as i64;
            if is_big_endian {
                bytes.extend_from_slice(&v.to_be_bytes());
            } else {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        "f32" => {
            let v = num as f32;
            if is_big_endian {
                bytes.extend_from_slice(&v.to_be_bytes());
            } else {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        "f64" => {
            let v = num;
            if is_big_endian {
                bytes.extend_from_slice(&v.to_be_bytes());
            } else {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        _ => return Err(format!("Unsupported type for packing: {}", type_str)),
    }
    Ok(to_value_array(&bytes))
}

fn builtin_unpack_number(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 3 {
        return Err("io_unpack_number requires (type_str, bytes_array, endian_str)".to_string());
    }
    let type_str = match &args[0] {
        Value::Str(s) => s.as_str(),
        _ => return Err("type_str must be string".to_string()),
    };
    let bytes = extract_bytes(&args[1])?;
    let is_big_endian = match &args[2] {
        Value::Str(s) => s == "big" || s == "BE",
        _ => false,
    };

    match type_str {
        "u8" => Ok(Value::Number(*bytes.first().unwrap_or(&0) as f64)),
        "i8" => Ok(Value::Number((*bytes.first().unwrap_or(&0) as i8) as f64)),
        "bool" => Ok(Value::Bool(*bytes.first().unwrap_or(&0) != 0)),
        "u16" => {
            if bytes.len() < 2 {
                return Err("Needs 2 bytes".to_string());
            }
            let arr = [bytes[0], bytes[1]];
            let v = if is_big_endian {
                u16::from_be_bytes(arr)
            } else {
                u16::from_le_bytes(arr)
            };
            Ok(Value::Number(v as f64))
        }
        "i16" => {
            if bytes.len() < 2 {
                return Err("Needs 2 bytes".to_string());
            }
            let arr = [bytes[0], bytes[1]];
            let v = if is_big_endian {
                i16::from_be_bytes(arr)
            } else {
                i16::from_le_bytes(arr)
            };
            Ok(Value::Number(v as f64))
        }
        "u32" => {
            if bytes.len() < 4 {
                return Err("Needs 4 bytes".to_string());
            }
            let arr = [bytes[0], bytes[1], bytes[2], bytes[3]];
            let v = if is_big_endian {
                u32::from_be_bytes(arr)
            } else {
                u32::from_le_bytes(arr)
            };
            Ok(Value::Number(v as f64))
        }
        "i32" => {
            if bytes.len() < 4 {
                return Err("Needs 4 bytes".to_string());
            }
            let arr = [bytes[0], bytes[1], bytes[2], bytes[3]];
            let v = if is_big_endian {
                i32::from_be_bytes(arr)
            } else {
                i32::from_le_bytes(arr)
            };
            Ok(Value::Number(v as f64))
        }
        "u64" => {
            if bytes.len() < 8 {
                return Err("Needs 8 bytes".to_string());
            }
            let arr = [
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ];
            let v = if is_big_endian {
                u64::from_be_bytes(arr)
            } else {
                u64::from_le_bytes(arr)
            };
            Ok(Value::Number(v as f64))
        }
        "i64" => {
            if bytes.len() < 8 {
                return Err("Needs 8 bytes".to_string());
            }
            let arr = [
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ];
            let v = if is_big_endian {
                i64::from_be_bytes(arr)
            } else {
                i64::from_le_bytes(arr)
            };
            Ok(Value::Number(v as f64))
        }
        "f32" => {
            if bytes.len() < 4 {
                return Err("Needs 4 bytes".to_string());
            }
            let arr = [bytes[0], bytes[1], bytes[2], bytes[3]];
            let v = if is_big_endian {
                f32::from_be_bytes(arr)
            } else {
                f32::from_le_bytes(arr)
            };
            Ok(Value::Number(v as f64))
        }
        "f64" => {
            if bytes.len() < 8 {
                return Err("Needs 8 bytes".to_string());
            }
            let arr = [
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ];
            let v = if is_big_endian {
                f64::from_be_bytes(arr)
            } else {
                f64::from_le_bytes(arr)
            };
            Ok(Value::Number(v))
        }
        _ => Err(format!("Unsupported type for unpacking: {}", type_str)),
    }
}

fn builtin_bytes_concat(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let mut result = Vec::new();
    for arg in args {
        let b = extract_bytes(&arg)?;
        result.extend_from_slice(&b);
    }
    Ok(to_value_array(&result))
}

fn builtin_slice_bytes(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("io_slice_bytes requires (bytes_array, start, [len])".to_string());
    }
    let bytes = extract_bytes(&args[0])?;
    let start = value_to_f64(&args[1]) as usize;
    let len = if args.len() >= 3 {
        value_to_f64(&args[2]) as usize
    } else {
        bytes.len().saturating_sub(start)
    };
    let end = (start + len).min(bytes.len());
    if start >= bytes.len() {
        Ok(to_value_array(&[]))
    } else {
        Ok(to_value_array(&bytes[start..end]))
    }
}

fn builtin_utf8_encode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let text = match args.first() {
        Some(Value::Str(s)) => s.as_bytes(),
        _ => &[],
    };
    Ok(to_value_array(text))
}

fn builtin_utf8_decode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = match args.first() {
        Some(v) => extract_bytes(v)?,
        None => Vec::new(),
    };
    String::from_utf8(bytes)
        .map(Value::Str)
        .map_err(|e| format!("Invalid UTF-8 sequence: {}", e))
}
