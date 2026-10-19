//! Native C ABI stream I/O and binary packing builtins for Adesh standard library.
//!
//! Provides native implementations of:
//! - `io_stdin_readline`
//! - `io_stdin_readchar`
//! - `io_stdout_write`
//! - `io_stderr_write`
//! - `io_pack_number`
//! - `io_unpack_number`
//! - `io_bytes_concat`
//! - `io_slice_bytes`
//! - `io_utf8_encode`
//! - `io_utf8_decode`

use crate::{RuntimeValue, aot_get_value, aot_store_value};
use std::ffi::CStr;
use std::io::{Read, Write};
use std::os::raw::c_char;

/// Helper to extract a String from either a runtime handle or a raw C-string pointer.
pub fn arg_to_string(raw: u64) -> String {
    if raw == 0 {
        return String::new();
    }
    if let Some(v) = aot_get_value(raw) {
        match v {
            RuntimeValue::String(s) => return s,
            _ => return v.as_string(),
        }
    }
    // Speculative check: if raw is above lower 64K and not a runtime handle, check if it points to valid C-string
    if raw > 0x10000 && raw < crate::HANDLE_TAG {
        let ptr = raw as *const c_char;
        if let Ok(cstr) = unsafe { CStr::from_ptr(ptr) }.to_str() {
            return cstr.to_string();
        }
    }
    (raw as i64).to_string()
}

/// Helper to extract usize integer value from a raw word or handle.
pub fn extract_usize_arg(arg: u64) -> usize {
    if let Some(val) = aot_get_value(arg) {
        if let Some(u) = val.as_usize() {
            return u;
        }
    }
    arg as usize
}

/// Helper to extract f64 numeric value from a raw word or handle.
pub fn extract_f64_arg(arg: u64) -> f64 {
    if let Some(val) = aot_get_value(arg) {
        match val {
            RuntimeValue::Int(i) | RuntimeValue::I64(i) => i as f64,
            RuntimeValue::I32(i) => i as f64,
            RuntimeValue::I16(i) => i as f64,
            RuntimeValue::I8(i) => i as f64,
            RuntimeValue::U64(u) => u as f64,
            RuntimeValue::U32(u) => u as f64,
            RuntimeValue::U16(u) => u as f64,
            RuntimeValue::U8(u) => u as f64,
            RuntimeValue::Float(f) | RuntimeValue::F64(f) => f,
            RuntimeValue::F32(f) => f as f64,
            RuntimeValue::Bool(b) => {
                if b {
                    1.0
                } else {
                    0.0
                }
            }
            _ => 0.0,
        }
    } else {
        arg as i64 as f64
    }
}

/// Helper to extract bytes from an array, string, or byte value.
pub fn extract_bytes_from_arg(raw: u64) -> Vec<u8> {
    if let Some(val) = aot_get_value(raw) {
        match val {
            RuntimeValue::Array(arr) => {
                let mut bytes = Vec::with_capacity(arr.len());
                for elem in arr {
                    let b = match elem {
                        RuntimeValue::Int(i) | RuntimeValue::I64(i) => i as u8,
                        RuntimeValue::I32(i) => i as u8,
                        RuntimeValue::I16(i) => i as u8,
                        RuntimeValue::I8(i) => i as u8,
                        RuntimeValue::U64(u) => u as u8,
                        RuntimeValue::U32(u) => u as u8,
                        RuntimeValue::U16(u) => u as u8,
                        RuntimeValue::U8(u) => u,
                        RuntimeValue::Float(f) | RuntimeValue::F64(f) => f as u8,
                        RuntimeValue::F32(f) => f as u8,
                        RuntimeValue::Char(c) => c as u8,
                        RuntimeValue::Bool(b) => {
                            if b {
                                1
                            } else {
                                0
                            }
                        }
                        _ => 0,
                    };
                    bytes.push(b);
                }
                return bytes;
            }
            RuntimeValue::String(s) => {
                return s.into_bytes();
            }
            RuntimeValue::U8(b) => return vec![b],
            _ => {}
        }
    }
    if raw > 0x10000 && raw < crate::HANDLE_TAG {
        let ptr = raw as *const c_char;
        if let Ok(cstr) = unsafe { CStr::from_ptr(ptr) }.to_str() {
            return cstr.as_bytes().to_vec();
        }
    }
    Vec::new()
}

/// Store bytes as a RuntimeValue::Array of Ints.
pub fn bytes_to_runtime_array(bytes: &[u8]) -> u64 {
    let arr = bytes.iter().map(|&b| RuntimeValue::Int(b as i64)).collect();
    aot_store_value(RuntimeValue::Array(arr))
}

#[unsafe(no_mangle)]
pub extern "C" fn io_stdin_readline() -> u64 {
    let mut input = String::new();
    let stdin = std::io::stdin();
    if stdin.read_line(&mut input).is_ok() {
        if input.ends_with('\n') {
            input.pop();
            if input.ends_with('\r') {
                input.pop();
            }
        }
    }
    aot_store_value(RuntimeValue::String(input))
}

#[unsafe(no_mangle)]
pub extern "C" fn io_stdin_readchar() -> u64 {
    let mut buf = [0u8; 1];
    let stdin = std::io::stdin();
    let s = match stdin.lock().read_exact(&mut buf) {
        Ok(_) => (buf[0] as char).to_string(),
        Err(_) => String::new(),
    };
    aot_store_value(RuntimeValue::String(s))
}

#[unsafe(no_mangle)]
pub extern "C" fn io_stdout_write(str_arg: u64) -> u64 {
    let text = arg_to_string(str_arg);
    print!("{}", text);
    let _ = std::io::stdout().flush();
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn io_stderr_write(str_arg: u64) -> u64 {
    let text = arg_to_string(str_arg);
    eprint!("{}", text);
    let _ = std::io::stderr().flush();
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn io_pack_number(type_str_arg: u64, num_arg: u64, endian_str_arg: u64) -> u64 {
    let type_str = arg_to_string(type_str_arg);
    let endian_str = arg_to_string(endian_str_arg);
    let is_big_endian = endian_str == "big" || endian_str == "BE";
    let num = extract_f64_arg(num_arg);

    let mut bytes = Vec::new();
    match type_str.as_str() {
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
            let b = if is_big_endian {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            };
            bytes.extend_from_slice(&b);
        }
        "u64" | "i64" => {
            let v = num as i64;
            let b = if is_big_endian {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            };
            bytes.extend_from_slice(&b);
        }
        "f32" => {
            let v = num as f32;
            let b = if is_big_endian {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            };
            bytes.extend_from_slice(&b);
        }
        "f64" => {
            let v = num;
            let b = if is_big_endian {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            };
            bytes.extend_from_slice(&b);
        }
        _ => {}
    }
    bytes_to_runtime_array(&bytes)
}

#[unsafe(no_mangle)]
pub extern "C" fn io_unpack_number(type_str_arg: u64, bytes_arg: u64, endian_str_arg: u64) -> u64 {
    let type_str = arg_to_string(type_str_arg);
    let endian_str = arg_to_string(endian_str_arg);
    let is_big_endian = endian_str == "big" || endian_str == "BE";
    let bytes = extract_bytes_from_arg(bytes_arg);

    match type_str.as_str() {
        "u8" => *bytes.first().unwrap_or(&0) as u64,
        "i8" => (*bytes.first().unwrap_or(&0) as i8 as i64) as u64,
        "bool" => {
            if *bytes.first().unwrap_or(&0) != 0 {
                1
            } else {
                0
            }
        }
        "u16" => {
            if bytes.len() < 2 {
                return 0;
            }
            let arr = [bytes[0], bytes[1]];
            let v = if is_big_endian {
                u16::from_be_bytes(arr)
            } else {
                u16::from_le_bytes(arr)
            };
            v as u64
        }
        "i16" => {
            if bytes.len() < 2 {
                return 0;
            }
            let arr = [bytes[0], bytes[1]];
            let v = if is_big_endian {
                i16::from_be_bytes(arr)
            } else {
                i16::from_le_bytes(arr)
            };
            (v as i64) as u64
        }
        "u32" => {
            if bytes.len() < 4 {
                return 0;
            }
            let arr = [bytes[0], bytes[1], bytes[2], bytes[3]];
            let v = if is_big_endian {
                u32::from_be_bytes(arr)
            } else {
                u32::from_le_bytes(arr)
            };
            v as u64
        }
        "i32" => {
            if bytes.len() < 4 {
                return 0;
            }
            let arr = [bytes[0], bytes[1], bytes[2], bytes[3]];
            let v = if is_big_endian {
                i32::from_be_bytes(arr)
            } else {
                i32::from_le_bytes(arr)
            };
            (v as i64) as u64
        }
        "u64" => {
            if bytes.len() < 8 {
                return 0;
            }
            let arr = [
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ];
            if is_big_endian {
                u64::from_be_bytes(arr)
            } else {
                u64::from_le_bytes(arr)
            }
        }
        "i64" => {
            if bytes.len() < 8 {
                return 0;
            }
            let arr = [
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ];
            let v = if is_big_endian {
                i64::from_be_bytes(arr)
            } else {
                i64::from_le_bytes(arr)
            };
            v as u64
        }
        "f32" => {
            if bytes.len() < 4 {
                return 0;
            }
            let arr = [bytes[0], bytes[1], bytes[2], bytes[3]];
            let v = if is_big_endian {
                f32::from_be_bytes(arr)
            } else {
                f32::from_le_bytes(arr)
            };
            aot_store_value(RuntimeValue::F32(v))
        }
        "f64" => {
            if bytes.len() < 8 {
                return 0;
            }
            let arr = [
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ];
            let v = if is_big_endian {
                f64::from_be_bytes(arr)
            } else {
                f64::from_le_bytes(arr)
            };
            aot_store_value(RuntimeValue::Float(v))
        }
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn io_bytes_concat(arr1_arg: u64, arr2_arg: u64) -> u64 {
    let mut b1 = extract_bytes_from_arg(arr1_arg);
    let b2 = extract_bytes_from_arg(arr2_arg);
    b1.extend(b2);
    bytes_to_runtime_array(&b1)
}

#[unsafe(no_mangle)]
pub extern "C" fn io_slice_bytes(bytes_arg: u64, start_arg: u64, len_arg: u64) -> u64 {
    let bytes = extract_bytes_from_arg(bytes_arg);
    let start = extract_usize_arg(start_arg);
    let len = extract_usize_arg(len_arg);
    let end = (start + len).min(bytes.len());
    if start >= bytes.len() {
        bytes_to_runtime_array(&[])
    } else {
        bytes_to_runtime_array(&bytes[start..end])
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn io_utf8_encode(str_arg: u64) -> u64 {
    let s = arg_to_string(str_arg);
    bytes_to_runtime_array(s.as_bytes())
}

#[unsafe(no_mangle)]
pub extern "C" fn io_utf8_decode(bytes_arg: u64) -> u64 {
    let bytes = extract_bytes_from_arg(bytes_arg);
    let s = String::from_utf8_lossy(&bytes).into_owned();
    aot_store_value(RuntimeValue::String(s))
}
