//! Runtime Bridge for AOT Compilation
//!
//! This module provides C-callable functions that bridge AOT-compiled code
//! with the Rust runtime for complex operations like creating and printing
//! arrays, objects, and other composite data structures.
//!
//! This ensures unified behavior across all backends through VIR.

use crate::backends::common::builtins::RuntimeValue;
use crate::parsing::ast::Value;
use crate::utils::collections::FastMap;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::io::Write;
use std::os::raw::c_char;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

// Thread-local storage for runtime values created by AOT
thread_local! {
    static AOT_RUNTIME_VALUES: RefCell<FastMap<u64, RuntimeValue>> = RefCell::new(FastMap::default());
}

// Global counter for generating unique handles
static AOT_HANDLE_COUNTER: AtomicU64 = AtomicU64::new(1);

fn aot_next_handle() -> u64 {
    AOT_HANDLE_COUNTER.fetch_add(1, Ordering::Relaxed)
}

pub fn aot_store_value(value: RuntimeValue) -> u64 {
    let handle = aot_next_handle();
    AOT_RUNTIME_VALUES.with(|values| {
        values.borrow_mut().insert(handle, value);
    });
    handle
}

pub fn aot_get_value(handle: u64) -> Option<RuntimeValue> {
    AOT_RUNTIME_VALUES.with(|values| values.borrow().get(&handle).cloned())
}

pub(super) fn ast_value_to_runtime_value(v: &Value) -> RuntimeValue {
    match v {
        Value::Number(n) => RuntimeValue::Float(*n),
        Value::I64(n) => RuntimeValue::Int(*n),
        Value::U64(n) => RuntimeValue::U64(*n),
        Value::Bool(b) => RuntimeValue::Bool(*b),
        Value::Char(c) => RuntimeValue::Char(*c),
        Value::Str(s) => RuntimeValue::String(s.clone()),
        Value::Object(m) => {
            let mut map = FxHashMap::default();
            for (k, val) in m.iter() {
                map.insert(k.clone(), ast_value_to_runtime_value(val));
            }
            RuntimeValue::Object(map)
        }
        Value::Array(a) => {
            let mut arr = Vec::new();
            for val in a {
                arr.push(ast_value_to_runtime_value(val));
            }
            RuntimeValue::Array(arr)
        }
        _ => RuntimeValue::Null,
    }
}

pub fn aot_resolve_value(handle: u64) -> RuntimeValue {
    // First check if handle is a global ARC handle (raw u64 ARC ID)
    if let Ok(guard) = crate::execution::arc_bridge::arc_manager().lock() {
        if let Ok(v) = guard.get_value(handle) {
            return ast_value_to_runtime_value(&v);
        }
    }

    // Fall back to thread-local AOT values
    if let Some(v) = aot_get_value(handle) {
        v
    } else {
        RuntimeValue::Null
    }
}

fn aot_remove_value(handle: u64) -> Option<RuntimeValue> {
    AOT_RUNTIME_VALUES.with(|values| values.borrow_mut().remove(&handle))
}

fn infer_int_value_type(n: i64) -> Value {
    if n >= 0 {
        let un = n as u64;
        if un <= u8::MAX as u64 {
            Value::U8(un as u8)
        } else if un <= u16::MAX as u64 {
            Value::U16(un as u16)
        } else if un <= u32::MAX as u64 {
            Value::U32(un as u32)
        } else {
            Value::U64(un)
        }
    } else if n >= i8::MIN as i64 {
        Value::I8(n as i8)
    } else if n >= i16::MIN as i64 {
        Value::I16(n as i16)
    } else if n >= i32::MIN as i64 {
        Value::I32(n as i32)
    } else {
        Value::I64(n)
    }
}

/// Convert RuntimeValue to ast::Value for pretty printing with depth limiting
fn runtime_value_to_ast_value(rv: &RuntimeValue) -> Value {
    runtime_value_to_ast_value_impl(rv, 0, 10)
}

fn normalize_runtime_string(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        let first = bytes[0] as char;
        let last = bytes[trimmed.len() - 1] as char;
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            return trimmed[1..trimmed.len() - 1].to_string();
        }
    }
    trimmed.to_string()
}

/// Internal implementation with depth tracking to prevent stack overflow
fn runtime_value_to_ast_value_impl(rv: &RuntimeValue, depth: usize, max_depth: usize) -> Value {
    if depth >= max_depth {
        // Prevent stack overflow by stopping at max depth
        return Value::Str("<max depth reached>".to_string());
    }

    match rv {
        RuntimeValue::Int(i) => infer_int_value_type(*i),
        RuntimeValue::Float(f) => Value::Number(*f),
        RuntimeValue::Bool(b) => Value::Bool(*b),
        RuntimeValue::Char(c) => Value::Char(*c),
        RuntimeValue::String(s) => Value::Str(normalize_runtime_string(s)),
        RuntimeValue::U8(n) => Value::U8(*n),
        RuntimeValue::U16(n) => Value::U16(*n),
        RuntimeValue::U32(n) => Value::U32(*n),
        RuntimeValue::U64(n) => Value::U64(*n),
        RuntimeValue::U128(n) => Value::U128(*n),
        RuntimeValue::I8(n) => Value::I8(*n),
        RuntimeValue::I16(n) => Value::I16(*n),
        RuntimeValue::I32(n) => Value::I32(*n),
        RuntimeValue::I64(n) => Value::I64(*n),
        RuntimeValue::I128(n) => Value::I128(*n),
        RuntimeValue::F32(n) => Value::F32(*n),
        RuntimeValue::F64(n) => Value::F64(*n),
        RuntimeValue::BigInt(bi) => Value::BigInt(bi.clone()),
        RuntimeValue::Null => Value::Null,
        RuntimeValue::Array(arr) | RuntimeValue::RawArray(_, arr) => Value::Array(
            arr.iter()
                .map(|v| runtime_value_to_ast_value_impl(v, depth + 1, max_depth))
                .collect(),
        ),
        RuntimeValue::Tuple(arr) => Value::Tuple(
            arr.iter()
                .map(|v| runtime_value_to_ast_value_impl(v, depth + 1, max_depth))
                .collect(),
        ),
        RuntimeValue::Set(set_vals) => Value::Set(
            set_vals
                .iter()
                .map(|v| runtime_value_to_ast_value_impl(v, depth + 1, max_depth))
                .collect(),
        ),
        RuntimeValue::Object(obj) => {
            let mut map = FxHashMap::default();
            for (k, v) in obj.iter() {
                map.insert(
                    k.clone(),
                    runtime_value_to_ast_value_impl(v, depth + 1, max_depth),
                );
            }
            Value::Object(Arc::new(map))
        }
        _ => Value::Null,
    }
}

/// Create an object with key-value pairs
/// args: [key1_handle, value1_handle, key2_handle, value2_handle, ...]
/// Returns handle to the created object
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_make_object(args_ptr: *const u64, arg_count: usize) -> u64 {
    use crate::utils::collections::FastMap;

    if arg_count == 0 {
        return aot_store_value(RuntimeValue::Object(FastMap::default()));
    }

    if args_ptr.is_null() || arg_count % 2 != 0 {
        return 0;
    }

    let args = unsafe { std::slice::from_raw_parts(args_ptr, arg_count) };

    let mut obj = FastMap::default();

    // Process key-value pairs
    for chunk in args.chunks(2) {
        if chunk.len() == 2 {
            let key_handle = chunk[0];
            let val_handle = chunk[1];

            // Get key string
            if let Some(RuntimeValue::String(key)) = aot_get_value(key_handle) {
                // Get value
                let value = aot_get_value(val_handle).unwrap_or(RuntimeValue::Null);
                obj.insert(key, value);
            }
        }
    }

    aot_store_value(RuntimeValue::Object(obj))
}

/// Set an object field and return a new object handle.
/// Expects handles: object, field-name string, value.
#[unsafe(no_mangle)]
pub extern "C" fn aot_set_field(obj_handle: u64, field_handle: u64, val_handle: u64) -> u64 {
    let field_name = match aot_get_value(field_handle) {
        Some(RuntimeValue::String(s)) => s,
        _ => return 0,
    };

    let value = aot_get_value(val_handle).unwrap_or(RuntimeValue::Null);

    match aot_get_value(obj_handle) {
        Some(RuntimeValue::Object(mut obj)) => {
            obj.insert(field_name, value);
            aot_store_value(RuntimeValue::Object(obj))
        }
        _ => 0,
    }
}

/// Get an object field and return it as a value handle.
#[unsafe(no_mangle)]
pub extern "C" fn aot_get_field(obj_handle: u64, field_handle: u64) -> u64 {
    let field_name = match aot_get_value(field_handle) {
        Some(RuntimeValue::String(s)) => s,
        _ => return aot_store_value(RuntimeValue::Null),
    };

    let resolved_obj = Some(aot_resolve_value(obj_handle));

    match resolved_obj {
        Some(RuntimeValue::Object(obj)) => {
            let value = obj.get(&field_name).cloned().unwrap_or(RuntimeValue::Null);
            aot_store_value(value)
        }
        Some(RuntimeValue::Array(arr)) => {
            let value = match field_name.as_str() {
                "len" | "length" => RuntimeValue::Int(arr.len() as i64),
                "capacity" => RuntimeValue::Int(arr.capacity() as i64),
                "metadata_size" => RuntimeValue::Int(24),
                _ => RuntimeValue::Null,
            };
            aot_store_value(value)
        }
        Some(RuntimeValue::DynArray {
            data,
            element_type,
            tracked_capacity,
            ..
        }) => {
            let value = match field_name.as_str() {
                "len" | "length" => RuntimeValue::Int(data.len() as i64),
                "capacity" => {
                    RuntimeValue::Int(tracked_capacity.unwrap_or_else(|| data.capacity()) as i64)
                }
                "metadata_size" => {
                    let metadata = if element_type.starts_with("u8")
                        || element_type.starts_with("i8")
                        || element_type.starts_with("u16")
                        || element_type.starts_with("i16")
                        || element_type.starts_with("u32")
                        || element_type.starts_with("i32")
                        || element_type.starts_with("f32")
                    {
                        16
                    } else {
                        24
                    };
                    RuntimeValue::Int(metadata as i64)
                }
                _ => RuntimeValue::Null,
            };
            aot_store_value(value)
        }
        Some(RuntimeValue::RawArray(_, arr)) => {
            let value = match field_name.as_str() {
                "len" | "length" => RuntimeValue::Int(arr.len() as i64),
                "capacity" => RuntimeValue::Int(arr.len() as i64),
                "metadata_size" => RuntimeValue::Int(0),
                _ => RuntimeValue::Null,
            };
            aot_store_value(value)
        }
        Some(RuntimeValue::Tuple(tup)) => {
            let value = match field_name.as_str() {
                "len" | "length" => RuntimeValue::Int(tup.len() as i64),
                "capacity" => RuntimeValue::Int(tup.len() as i64),
                "metadata_size" => RuntimeValue::Int(0),
                _ => RuntimeValue::Null,
            };
            aot_store_value(value)
        }
        Some(RuntimeValue::String(s)) => {
            let value = match field_name.as_str() {
                "len" | "length" => RuntimeValue::Int(s.len() as i64),
                _ => RuntimeValue::Null,
            };
            aot_store_value(value)
        }
        _ => aot_store_value(RuntimeValue::Null),
    }
}

/// Create an array from handles
/// args_ptr: pointer to array of value handles
/// arg_count: number of elements
/// Returns handle to the created array
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_make_array(args_ptr: *const u64, arg_count: usize) -> u64 {
    if args_ptr.is_null() {
        return aot_store_value(RuntimeValue::Array(Vec::new()));
    }

    let args = unsafe { std::slice::from_raw_parts(args_ptr, arg_count) };

    let mut arr = Vec::new();
    for &handle in args {
        if let Some(value) = aot_get_value(handle) {
            arr.push(value);
        } else {
            arr.push(RuntimeValue::Null);
        }
    }

    aot_store_value(RuntimeValue::Array(arr))
}

/// Create a string from a C string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_make_string(string_ptr: *const c_char) -> u64 {
    if string_ptr.is_null() {
        return aot_store_value(RuntimeValue::String(String::new()));
    }

    unsafe {
        if let Ok(s) = std::ffi::CStr::from_ptr(string_ptr).to_str() {
            aot_store_value(RuntimeValue::String(s.to_string()))
        } else {
            aot_store_value(RuntimeValue::String(String::new()))
        }
    }
}

/// Wrap a raw pointer-like value into a runtime handle.
/// - Returns existing handle as-is if already known to runtime store.
/// - Converts null to RuntimeValue::Null.
/// - Otherwise interprets pointer as C string and wraps as RuntimeValue::String.
#[unsafe(no_mangle)]
pub extern "C" fn aot_wrap_ptr(value: u64) -> u64 {
    if value == 0 {
        return aot_store_value(RuntimeValue::Null);
    }

    if aot_get_value(value).is_some() {
        return value;
    }

    // Avoid dereferencing obviously invalid low addresses.
    if value < 0x10000 {
        return aot_store_value(RuntimeValue::Int(value as i64));
    }

    unsafe { aot_make_string(value as *const c_char) }
}

/// Create an unsigned 8-bit integer value (u8)
#[unsafe(no_mangle)]
pub extern "C" fn aot_make_u8(value: u64) -> u64 {
    aot_store_value(RuntimeValue::U8(value as u8))
}

/// Create an unsigned 16-bit integer value (u16)
#[unsafe(no_mangle)]
pub extern "C" fn aot_make_u16(value: u64) -> u64 {
    aot_store_value(RuntimeValue::U16(value as u16))
}

/// Create an unsigned 32-bit integer value (u32)
#[unsafe(no_mangle)]
pub extern "C" fn aot_make_u32(value: u64) -> u64 {
    aot_store_value(RuntimeValue::U32(value as u32))
}

/// Create an unsigned 64-bit integer value (u64)
#[unsafe(no_mangle)]
pub extern "C" fn aot_make_u64(value: u64) -> u64 {
    aot_store_value(RuntimeValue::U64(value))
}

/// Create a signed 8-bit integer value (i8)
#[unsafe(no_mangle)]
pub extern "C" fn aot_make_i8(value: i64) -> u64 {
    aot_store_value(RuntimeValue::I8(value as i8))
}

/// Create a signed 16-bit integer value (i16)
#[unsafe(no_mangle)]
pub extern "C" fn aot_make_i16(value: i64) -> u64 {
    aot_store_value(RuntimeValue::I16(value as i16))
}

/// Create a signed 32-bit integer value (i32)
#[unsafe(no_mangle)]
pub extern "C" fn aot_make_i32(value: i64) -> u64 {
    aot_store_value(RuntimeValue::I32(value as i32))
}

/// Create an integer value (i64)
#[unsafe(no_mangle)]
pub extern "C" fn aot_make_i64(value: i64) -> u64 {
    aot_store_value(RuntimeValue::Int(value))
}

/// Create a 32-bit float value (f32)
#[unsafe(no_mangle)]
pub extern "C" fn aot_make_f32(value: f64) -> u64 {
    aot_store_value(RuntimeValue::F32(value as f32))
}

/// Create a float value (f64)
#[unsafe(no_mangle)]
pub extern "C" fn aot_make_f64(value: f64) -> u64 {
    aot_store_value(RuntimeValue::Float(value))
}

/// Create a boolean value
#[unsafe(no_mangle)]
pub extern "C" fn aot_make_bool(value: i64) -> u64 {
    aot_store_value(RuntimeValue::Bool(value != 0))
}

/// Create a character value
#[unsafe(no_mangle)]
pub extern "C" fn aot_make_char(value: u64) -> u64 {
    let ch = char::from_u32(value as u32).unwrap_or('\0');
    aot_store_value(RuntimeValue::Char(ch))
}

/// Create a tuple from element handles
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_make_tuple(args_ptr: *const u64, arg_count: usize) -> u64 {
    if args_ptr.is_null() {
        return aot_store_value(RuntimeValue::Tuple(Vec::new()));
    }
    let args = unsafe { std::slice::from_raw_parts(args_ptr, arg_count) };
    let mut arr = Vec::with_capacity(arg_count);
    for &handle in args {
        arr.push(aot_resolve_value(handle));
    }
    aot_store_value(RuntimeValue::Tuple(arr))
}

/// Create a set from element handles
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_make_set(args_ptr: *const u64, arg_count: usize) -> u64 {
    if args_ptr.is_null() {
        return aot_store_value(RuntimeValue::Set(Vec::new()));
    }
    let args = unsafe { std::slice::from_raw_parts(args_ptr, arg_count) };
    let mut arr = Vec::with_capacity(arg_count);
    for &handle in args {
        arr.push(aot_resolve_value(handle));
    }
    aot_store_value(RuntimeValue::Set(arr))
}

/// Create a null value
#[unsafe(no_mangle)]
pub extern "C" fn aot_make_null() -> u64 {
    aot_store_value(RuntimeValue::Null)
}

/// Pretty print a value by handle
/// handle: handle to a RuntimeValue
/// mode_ptr: pointer to mode string ("none", "compact", "simple", "full")
/// Pretty print a value with the specified mode
/// mode: 0 = no pretty, 1 = full (colors + types), 2 = compact (inline, no colors), 3 = simple (basic colors)
/// Returns 0 on success
#[unsafe(no_mangle)]
pub extern "C" fn aot_print_value_pretty(handle: u64, mode: i64) -> u64 {
    use crate::execution::runtime_core::pretty_print::{PrettyPrintOptions, pretty_print};

    // Get the value
    let value = aot_resolve_value(handle);
    let ast_value = runtime_value_to_ast_value(&value);

    // Select pretty print options based on mode
    let pretty_opts = match mode {
        2 => PrettyPrintOptions::compact(),
        3 => PrettyPrintOptions::simple_color(),
        1 => PrettyPrintOptions::default(), // full mode
        _ => {
            // mode 0 or any other value: use as_string()
            print!("{}", value.as_string());
            let _ = std::io::stdout().flush();
            return 0;
        }
    };

    // Pretty print the value
    let output = pretty_print(&ast_value, &pretty_opts);
    print!("{}", output);
    let _ = std::io::stdout().flush();

    0
}

/// Print a newline to stdout
#[unsafe(no_mangle)]
pub extern "C" fn aot_print_newline() -> u64 {
    println!();
    let _ = std::io::stdout().flush();
    0
}

/// Print a space to stdout
#[unsafe(no_mangle)]
pub extern "C" fn aot_print_space() -> u64 {
    print!(" ");
    let _ = std::io::stdout().flush();
    0
}

/// Print an i8 integer with optional newline
#[unsafe(no_mangle)]
pub extern "C" fn aot_print_i8(value: i8, newline: i64) -> u64 {
    if newline != 0 {
        println!("{}", value);
    } else {
        print!("{}", value);
    }
    let _ = std::io::stdout().flush();
    0
}

/// Print an i16 integer with optional newline
#[unsafe(no_mangle)]
pub extern "C" fn aot_print_i16(value: i16, newline: i64) -> u64 {
    if newline != 0 {
        println!("{}", value);
    } else {
        print!("{}", value);
    }
    let _ = std::io::stdout().flush();
    0
}

/// Print an i32 integer with optional newline
#[unsafe(no_mangle)]
pub extern "C" fn aot_print_i32(value: i32, newline: i64) -> u64 {
    if newline != 0 {
        println!("{}", value);
    } else {
        print!("{}", value);
    }
    let _ = std::io::stdout().flush();
    0
}

/// Print an i64 integer with optional newline
#[unsafe(no_mangle)]
pub extern "C" fn aot_print_i64(value: i64, newline: i64) -> u64 {
    if newline != 0 {
        println!("{}", value);
    } else {
        print!("{}", value);
    }
    let _ = std::io::stdout().flush();
    0
}

/// Print a u8 integer with optional newline
#[unsafe(no_mangle)]
pub extern "C" fn aot_print_u8(value: u8, newline: i64) -> u64 {
    if newline != 0 {
        println!("{}", value);
    } else {
        print!("{}", value);
    }
    let _ = std::io::stdout().flush();
    0
}

/// Print a u16 integer with optional newline
#[unsafe(no_mangle)]
pub extern "C" fn aot_print_u16(value: u16, newline: i64) -> u64 {
    if newline != 0 {
        println!("{}", value);
    } else {
        print!("{}", value);
    }
    let _ = std::io::stdout().flush();
    0
}

/// Print a u32 integer with optional newline
#[unsafe(no_mangle)]
pub extern "C" fn aot_print_u32(value: u32, newline: i64) -> u64 {
    if newline != 0 {
        println!("{}", value);
    } else {
        print!("{}", value);
    }
    let _ = std::io::stdout().flush();
    0
}

/// Print a u64 integer with optional newline
#[unsafe(no_mangle)]
pub extern "C" fn aot_print_u64(value: u64, newline: i64) -> u64 {
    if newline != 0 {
        println!("{}", value);
    } else {
        print!("{}", value);
    }
    let _ = std::io::stdout().flush();
    0
}

/// Print an f32 float with optional newline
#[unsafe(no_mangle)]
pub extern "C" fn aot_print_f32(value: f32, newline: i64) -> u64 {
    if newline != 0 {
        println!("{}", value);
    } else {
        print!("{}", value);
    }
    let _ = std::io::stdout().flush();
    0
}

/// Print an f64 float with optional newline
#[unsafe(no_mangle)]
pub extern "C" fn aot_print_f64(value: f64, newline: i64) -> u64 {
    if newline != 0 {
        println!("{}", value);
    } else {
        print!("{}", value);
    }
    let _ = std::io::stdout().flush();
    0
}

/// Print a string with optional newline
#[unsafe(no_mangle)]
pub extern "C" fn aot_print_str(string_ptr: i64, newline: i64) -> u64 {
    if string_ptr == 0 {
        return 0;
    }

    unsafe {
        let ptr = string_ptr as *const std::os::raw::c_char;
        if let Ok(s) = std::ffi::CStr::from_ptr(ptr).to_str() {
            if newline != 0 {
                println!("{}", s);
            } else {
                print!("{}", s);
            }
        }
    }
    let _ = std::io::stdout().flush();
    0
}

/// Print a boolean with optional newline
#[unsafe(no_mangle)]
pub extern "C" fn aot_print_bool(value: i64, newline: i64) -> u64 {
    let bool_val = value != 0;
    if newline != 0 {
        println!("{}", bool_val);
    } else {
        print!("{}", bool_val);
    }
    let _ = std::io::stdout().flush();
    0
}

/// Print null with optional newline
#[unsafe(no_mangle)]
pub extern "C" fn aot_print_null(newline: i64) -> u64 {
    if newline != 0 {
        println!("null");
    } else {
        print!("null");
    }
    let _ = std::io::stdout().flush();
    0
}

/// Print values with optional options object (for dynamic print calls)
/// values_ptr: pointer to array of value handles
/// values_count: number of values
/// options_handle: handle to options object, or 0 for no options
/// Returns 0 on success
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_print_with_options(
    values_ptr: *const u64,
    values_count: i64,
    options_handle: u64,
) -> u64 {
    fn normalize_key(key: &str) -> String {
        let k = key.trim();
        if k.len() >= 2 {
            let bytes = k.as_bytes();
            let first = bytes[0] as char;
            let last = bytes[k.len() - 1] as char;
            if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
                return k[1..k.len() - 1].to_string();
            }
        }
        k.to_string()
    }

    fn parse_hex_color(hex: &str) -> Option<(u8, u8, u8)> {
        let mut norm = hex.trim();
        if norm.len() >= 2 {
            let bytes = norm.as_bytes();
            let first = bytes[0] as char;
            let last = bytes[norm.len() - 1] as char;
            if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
                norm = &norm[1..norm.len() - 1];
            }
        }
        let h = norm.strip_prefix('#').unwrap_or(norm);
        if h.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&h[0..2], 16).ok()?;
        let g = u8::from_str_radix(&h[2..4], 16).ok()?;
        let b = u8::from_str_radix(&h[4..6], 16).ok()?;
        Some((r, g, b))
    }

    fn build_style_prefix(opts: &FastMap<String, RuntimeValue>) -> String {
        let mut codes: Vec<String> = Vec::new();
        let get_bool = |name: &str| -> bool {
            opts.iter()
                .any(|(k, v)| normalize_key(k) == name && matches!(v, RuntimeValue::Bool(true)))
        };
        let get_str = |name: &str| -> Option<String> {
            opts.iter().find_map(|(k, v)| {
                if normalize_key(k) == name {
                    if let RuntimeValue::String(s) = v {
                        Some(s.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        };

        if get_bool("bold") {
            codes.push("1".to_string());
        }
        if get_bool("italic") {
            codes.push("3".to_string());
        }
        if get_bool("underline") {
            codes.push("4".to_string());
        }
        if get_bool("strikethrough") {
            codes.push("9".to_string());
        }

        if let Some(color) = get_str("color") {
            if let Some((r, g, b)) = parse_hex_color(&color) {
                codes.push(format!("38;2;{};{};{}", r, g, b));
            }
        }
        if let Some(bg) = get_str("background") {
            if let Some((r, g, b)) = parse_hex_color(&bg) {
                codes.push(format!("48;2;{};{};{}", r, g, b));
            }
        }

        if codes.is_empty() {
            String::new()
        } else {
            format!("\x1b[{}m", codes.join(";"))
        }
    }

    if values_count < 0 {
        return 0;
    }

    let values_count = values_count as usize;
    let values_array: &[u64] = if values_count == 0 {
        &[]
    } else {
        if values_ptr.is_null() {
            return 0;
        }
        unsafe { std::slice::from_raw_parts(values_ptr, values_count) }
    };

    // Convert handles to RuntimeValues
    let runtime_values: Vec<RuntimeValue> = values_array
        .iter()
        .map(|&handle| aot_get_value(handle).unwrap_or(RuntimeValue::Null))
        .collect();

    // Get options from handle
    let mut pretty_mode = 0i64; // default: no pretty
    let mut sep = " ".to_string();
    let mut end = "\n".to_string();
    let mut style_prefix = String::new();
    let mut style_reset = String::new();
    let mut file_opt: Option<String> = None;
    let mut has_options = false;
    if let Some(RuntimeValue::Object(opts_obj)) = aot_get_value(options_handle) {
        has_options = opts_obj.keys().any(|k| {
            let key = normalize_key(k);
            matches!(
                key.as_str(),
                "pretty"
                    | "sep"
                    | "end"
                    | "file"
                    | "color"
                    | "background"
                    | "bold"
                    | "italic"
                    | "underline"
                    | "strikethrough"
                    | "flush"
            )
        });

        if !has_options {
            // Not a recognized options object; keep defaults and treat all values as print args.
            style_prefix.clear();
            style_reset.clear();
        } else {
            let get_str = |name: &str| -> Option<String> {
                opts_obj.iter().find_map(|(k, v)| {
                    if normalize_key(k) == name {
                        if let RuntimeValue::String(s) = v {
                            Some(s.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
            };
            let get_bool = |name: &str| -> Option<bool> {
                opts_obj.iter().find_map(|(k, v)| {
                    if normalize_key(k) == name {
                        if let RuntimeValue::Bool(b) = v {
                            Some(*b)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
            };

            if let Some(pretty_val) = get_str("pretty") {
                pretty_mode = match pretty_val {
                    s => match s.as_str() {
                        "compact" => 2,
                        "simple" => 3,
                        "full" | "true" => 1,
                        _ => 0,
                    },
                };
            } else if let Some(pretty_bool) = get_bool("pretty") {
                pretty_mode = if pretty_bool { 1 } else { 0 };
            }
            if let Some(s) = get_str("sep") {
                sep = s.clone();
            }
            if let Some(s) = get_str("end") {
                end = s.clone();
            }
            if let Some(s) = get_str("file") {
                file_opt = Some(s.clone());
            }

            style_prefix = build_style_prefix(&opts_obj);
            if !style_prefix.is_empty() {
                style_reset = "\x1b[0m".to_string();
            }
        }
    }

    // Direct printing with appropriate formatting
    use crate::execution::runtime_core::pretty_print::{PrettyPrintOptions, pretty_print};

    let with_pretty = pretty_mode > 0;
    let pretty_opts = if with_pretty {
        match pretty_mode {
            2 => PrettyPrintOptions::compact(),
            3 => PrettyPrintOptions::simple_color(),
            _ => PrettyPrintOptions::default(),
        }
    } else {
        // Don't use pretty print options
        PrettyPrintOptions::no_color()
    };

    let effective_len = if has_options
        && !runtime_values.is_empty()
        && values_array.last() == Some(&options_handle)
    {
        runtime_values.len().saturating_sub(1)
    } else {
        runtime_values.len()
    };

    if let Some(file_path) = file_opt {
        use std::fs::OpenOptions;
        use std::io::Write;
        let mut file_output = String::new();
        for (i, value) in runtime_values.iter().take(effective_len).enumerate() {
            if i > 0 {
                file_output.push_str(&sep);
            }
            if !style_prefix.is_empty() {
                file_output.push_str(&style_prefix);
            }
            let ast_value = runtime_value_to_ast_value(value);
            if with_pretty {
                let output = pretty_print(&ast_value, &pretty_opts);
                file_output.push_str(&output);
            } else {
                file_output.push_str(&crate::execution::runtime_core::format::fmt(&ast_value));
            }
            if !style_reset.is_empty() {
                file_output.push_str(&style_reset);
            }
        }
        if !end.is_empty() {
            file_output.push_str(&end);
        }
        if let Ok(mut f) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
        {
            let _ = f.write_all(file_output.as_bytes());
            let _ = f.flush();
        }
        return 0;
    }

    for (i, value) in runtime_values.iter().take(effective_len).enumerate() {
        if i > 0 {
            print!("{}", sep);
        }
        if !style_prefix.is_empty() {
            print!("{}", style_prefix);
        }
        let ast_value = runtime_value_to_ast_value(value);
        if with_pretty {
            let output = pretty_print(&ast_value, &pretty_opts);
            print!("{}", output);
        } else {
            print!(
                "{}",
                crate::execution::runtime_core::format::fmt(&ast_value)
            );
        }
        if !style_reset.is_empty() {
            print!("{}", style_reset);
        }
    }
    if !end.is_empty() {
        print!("{}", end);
    }
    let _ = std::io::stdout().flush();
    0
}

/// Free a runtime value by its handle
#[unsafe(no_mangle)]
pub extern "C" fn aot_free_handle(handle: u64) {
    aot_remove_value(handle);
}

// ============================================================================
// Filesystem Support for AOT Compiled Code
// ============================================================================

fn get_string_value(val: u64) -> Option<String> {
    if let Some(rv) = aot_get_value(val) {
        match rv {
            RuntimeValue::String(s) => Some(s),
            _ => None,
        }
    } else if val != 0 {
        let c_str = unsafe { std::ffi::CStr::from_ptr(val as *const std::os::raw::c_char) };
        c_str.to_str().ok().map(|s| s.to_string())
    } else {
        None
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_read(path_val: u64) -> u64 {
    let path = match get_string_value(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::Null),
    };
    match std::fs::read_to_string(&path) {
        Ok(s) => aot_store_value(RuntimeValue::String(s)),
        Err(_) => aot_store_value(RuntimeValue::Null),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_write(path_val: u64, content_val: u64) -> u64 {
    let path = match get_string_value(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    let content = match get_string_value(content_val) {
        Some(c) => c,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    match std::fs::write(&path, content) {
        Ok(_) => aot_store_value(RuntimeValue::Bool(true)),
        Err(_) => aot_store_value(RuntimeValue::Bool(false)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_exists(path_val: u64) -> u64 {
    let path = match get_string_value(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    let exists = std::path::Path::new(&path).exists();
    aot_store_value(RuntimeValue::Bool(exists))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_is_file(path_val: u64) -> u64 {
    let path = match get_string_value(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    let is_file = std::path::Path::new(&path).is_file();
    aot_store_value(RuntimeValue::Bool(is_file))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_is_dir(path_val: u64) -> u64 {
    let path = match get_string_value(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    let is_dir = std::path::Path::new(&path).is_dir();
    aot_store_value(RuntimeValue::Bool(is_dir))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_mkdir(path_val: u64) -> u64 {
    let path = match get_string_value(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    match std::fs::create_dir_all(&path) {
        Ok(_) => aot_store_value(RuntimeValue::Bool(true)),
        Err(_) => aot_store_value(RuntimeValue::Bool(false)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_copy(src_val: u64, dst_val: u64) -> u64 {
    let src = match get_string_value(src_val) {
        Some(s) => s,
        None => return aot_store_value(RuntimeValue::Null),
    };
    let dst = match get_string_value(dst_val) {
        Some(d) => d,
        None => return aot_store_value(RuntimeValue::Null),
    };
    match std::fs::copy(&src, &dst) {
        Ok(n) => aot_store_value(RuntimeValue::Float(n as f64)),
        Err(_) => aot_store_value(RuntimeValue::Null),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_move(src_val: u64, dst_val: u64) -> u64 {
    let src = match get_string_value(src_val) {
        Some(s) => s,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    let dst = match get_string_value(dst_val) {
        Some(d) => d,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    match std::fs::rename(&src, &dst) {
        Ok(_) => aot_store_value(RuntimeValue::Bool(true)),
        Err(_) => aot_store_value(RuntimeValue::Bool(false)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_delete(path_val: u64) -> u64 {
    let path = match get_string_value(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    let p = std::path::Path::new(&path);
    let res = if p.is_dir() {
        std::fs::remove_dir_all(&path).is_ok()
    } else {
        std::fs::remove_file(&path).is_ok()
    };
    aot_store_value(RuntimeValue::Bool(res))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_fs_path_join(args_ptr: *const u64, arg_count: usize) -> u64 {
    if args_ptr.is_null() || arg_count == 0 {
        return aot_store_value(RuntimeValue::String(String::new()));
    }
    let args = unsafe { std::slice::from_raw_parts(args_ptr, arg_count) };
    let mut path = std::path::PathBuf::new();
    for &arg in args {
        if let Some(s) = get_string_value(arg) {
            path.push(s);
        }
    }
    aot_store_value(RuntimeValue::String(path.to_string_lossy().into_owned()))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_path_basename(path_val: u64) -> u64 {
    let path_str = match get_string_value(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::String(String::new())),
    };
    let p = std::path::Path::new(&path_str);
    let base = p
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(String::new);
    aot_store_value(RuntimeValue::String(base))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_path_dirname(path_val: u64) -> u64 {
    let path_str = match get_string_value(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::String(String::new())),
    };
    let p = std::path::Path::new(&path_str);
    let dir = p
        .parent()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(String::new);
    aot_store_value(RuntimeValue::String(dir))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_path_extname(path_val: u64) -> u64 {
    let path_str = match get_string_value(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::String(String::new())),
    };
    let p = std::path::Path::new(&path_str);
    let ext = p
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_else(String::new);
    aot_store_value(RuntimeValue::String(ext))
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_value_to_string(value: u64) -> *mut std::os::raw::c_char {
    let s = if let Some(rv) = aot_get_value(value) {
        match rv {
            RuntimeValue::String(s) => s,
            _ => {
                let ast = runtime_value_to_ast_value(&rv);
                crate::execution::runtime_core::format::fmt(&ast)
            }
        }
    } else if value > 1000 {
        let c_str = unsafe { std::ffi::CStr::from_ptr(value as *const std::os::raw::c_char) };
        if let Ok(s) = c_str.to_str() {
            s.to_string()
        } else {
            value.to_string()
        }
    } else {
        value.to_string()
    };

    let c_str = std::ffi::CString::new(s).unwrap();
    let bytes = c_str.as_bytes_with_nul();
    unsafe {
        let ptr = libc::malloc(bytes.len()) as *mut std::os::raw::c_char;
        if !ptr.is_null() {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr() as *const std::os::raw::c_char,
                ptr,
                bytes.len(),
            );
        }
        ptr
    }
}

/// AOT C-ABI bridge for input()
#[unsafe(no_mangle)]
pub extern "C" fn aot_input(prompt_val: u64, opts_val: u64) -> u64 {
    let prompt = get_string_value(prompt_val).unwrap_or_default();
    let opts = aot_get_value(opts_val);
    let mut args = vec![RuntimeValue::String(prompt)];
    if let Some(opt) = opts {
        args.push(opt);
    }
    let registry = crate::backends::common::builtins::BuiltinRegistry::new();
    let res = if let Some(func) = registry.get("input") {
        func(&args)
    } else {
        RuntimeValue::Null
    };
    aot_store_value(res)
}

/// AOT C-ABI bridge for generic typed input<T>()
#[unsafe(no_mangle)]
pub extern "C" fn aot_input_generic(
    prompt_val: u64,
    opts_val: u64,
    type_name_ptr: *const std::os::raw::c_char,
) -> u64 {
    let prompt = get_string_value(prompt_val).unwrap_or_default();
    let opts = aot_get_value(opts_val);
    let mut args = vec![RuntimeValue::String(prompt)];
    if let Some(opt) = opts {
        args.push(opt);
    }
    if !type_name_ptr.is_null() {
        if let Ok(type_name) = unsafe { std::ffi::CStr::from_ptr(type_name_ptr).to_str() } {
            crate::backends::common::builtins::set_jit_generic_type(type_name.to_string());
        }
    }
    let registry = crate::backends::common::builtins::BuiltinRegistry::new();
    let res = if let Some(func) = registry.get("input") {
        func(&args)
    } else {
        RuntimeValue::Null
    };
    crate::backends::common::builtins::clear_jit_generic_type();
    aot_store_value(res)
}

fn unpack_aot_arg(raw: u64) -> RuntimeValue {
    if let Some(val) = aot_get_value(raw) {
        val
    } else if let Some(s) = get_string_value(raw) {
        RuntimeValue::String(s)
    } else if let Ok(guard) = crate::execution::arc_bridge::arc_manager().lock() {
        if let Ok(v) = guard.get_value(raw) {
            ast_value_to_runtime_value(&v)
        } else {
            RuntimeValue::Int(raw as i64)
        }
    } else {
        RuntimeValue::Int(raw as i64)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_input_mock(arg0: u64) -> u64 {
    let args = vec![unpack_aot_arg(arg0)];
    let registry = crate::backends::common::builtins::BuiltinRegistry::new();
    let res = if let Some(func) = registry.get("input.mock") {
        func(&args)
    } else {
        RuntimeValue::Null
    };
    aot_store_value(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_input_confirm(arg0: u64, arg1: u64) -> u64 {
    let args = vec![unpack_aot_arg(arg0), unpack_aot_arg(arg1)];
    let registry = crate::backends::common::builtins::BuiltinRegistry::new();
    let res = if let Some(func) = registry.get("input.confirm") {
        func(&args)
    } else {
        RuntimeValue::Null
    };
    aot_store_value(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_input_password(arg0: u64, arg1: u64) -> u64 {
    let args = vec![unpack_aot_arg(arg0), unpack_aot_arg(arg1)];
    let registry = crate::backends::common::builtins::BuiltinRegistry::new();
    let res = if let Some(func) = registry.get("input.password") {
        func(&args)
    } else {
        RuntimeValue::Null
    };
    aot_store_value(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_input_select(arg0: u64, arg1: u64, arg2: u64) -> u64 {
    let args = vec![
        unpack_aot_arg(arg0),
        unpack_aot_arg(arg1),
        unpack_aot_arg(arg2),
    ];
    let registry = crate::backends::common::builtins::BuiltinRegistry::new();
    let res = if let Some(func) = registry.get("input.select") {
        func(&args)
    } else {
        RuntimeValue::Null
    };
    aot_store_value(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_input_checkbox(arg0: u64, arg1: u64, arg2: u64) -> u64 {
    let args = vec![
        unpack_aot_arg(arg0),
        unpack_aot_arg(arg1),
        unpack_aot_arg(arg2),
    ];
    let registry = crate::backends::common::builtins::BuiltinRegistry::new();
    let res = if let Some(func) = registry.get("input.checkbox") {
        func(&args)
    } else {
        RuntimeValue::Null
    };
    aot_store_value(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_input_radio(arg0: u64, arg1: u64, arg2: u64) -> u64 {
    let args = vec![
        unpack_aot_arg(arg0),
        unpack_aot_arg(arg1),
        unpack_aot_arg(arg2),
    ];
    let registry = crate::backends::common::builtins::BuiltinRegistry::new();
    let res = if let Some(func) = registry.get("input.radio") {
        func(&args)
    } else {
        RuntimeValue::Null
    };
    aot_store_value(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_input_fuzzy(arg0: u64, arg1: u64) -> u64 {
    let args = vec![unpack_aot_arg(arg0), unpack_aot_arg(arg1)];
    let registry = crate::backends::common::builtins::BuiltinRegistry::new();
    let res = if let Some(func) = registry.get("input.fuzzy") {
        func(&args)
    } else {
        RuntimeValue::Null
    };
    aot_store_value(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_input_slider(arg0: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64) -> u64 {
    let args = vec![
        unpack_aot_arg(arg0),
        unpack_aot_arg(arg1),
        unpack_aot_arg(arg2),
        unpack_aot_arg(arg3),
        unpack_aot_arg(arg4),
    ];
    let registry = crate::backends::common::builtins::BuiltinRegistry::new();
    let res = if let Some(func) = registry.get("input.slider") {
        func(&args)
    } else {
        RuntimeValue::Null
    };
    aot_store_value(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_input_tree(arg0: u64, arg1: u64, arg2: u64) -> u64 {
    let args = vec![
        unpack_aot_arg(arg0),
        unpack_aot_arg(arg1),
        unpack_aot_arg(arg2),
    ];
    let registry = crate::backends::common::builtins::BuiltinRegistry::new();
    let res = if let Some(func) = registry.get("input.tree") {
        func(&args)
    } else {
        RuntimeValue::Null
    };
    aot_store_value(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_input_table(arg0: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    let args = vec![
        unpack_aot_arg(arg0),
        unpack_aot_arg(arg1),
        unpack_aot_arg(arg2),
        unpack_aot_arg(arg3),
    ];
    let registry = crate::backends::common::builtins::BuiltinRegistry::new();
    let res = if let Some(func) = registry.get("input.table") {
        func(&args)
    } else {
        RuntimeValue::Null
    };
    aot_store_value(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_input_datepicker(arg0: u64, arg1: u64) -> u64 {
    let args = vec![unpack_aot_arg(arg0), unpack_aot_arg(arg1)];
    let registry = crate::backends::common::builtins::BuiltinRegistry::new();
    let res = if let Some(func) = registry.get("input.datepicker") {
        func(&args)
    } else {
        RuntimeValue::Null
    };
    aot_store_value(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_input_datetime(arg0: u64, arg1: u64) -> u64 {
    let args = vec![unpack_aot_arg(arg0), unpack_aot_arg(arg1)];
    let registry = crate::backends::common::builtins::BuiltinRegistry::new();
    let res = if let Some(func) = registry.get("input.datetime") {
        func(&args)
    } else {
        RuntimeValue::Null
    };
    aot_store_value(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_input_timepicker(arg0: u64, arg1: u64) -> u64 {
    let args = vec![unpack_aot_arg(arg0), unpack_aot_arg(arg1)];
    let registry = crate::backends::common::builtins::BuiltinRegistry::new();
    let res = if let Some(func) = registry.get("input.timepicker") {
        func(&args)
    } else {
        RuntimeValue::Null
    };
    aot_store_value(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_input_color(arg0: u64, arg1: u64) -> u64 {
    let args = vec![unpack_aot_arg(arg0), unpack_aot_arg(arg1)];
    let registry = crate::backends::common::builtins::BuiltinRegistry::new();
    let res = if let Some(func) = registry.get("input.color") {
        func(&args)
    } else {
        RuntimeValue::Null
    };
    aot_store_value(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_input_pin(arg0: u64, arg1: u64, arg2: u64) -> u64 {
    let args = vec![
        unpack_aot_arg(arg0),
        unpack_aot_arg(arg1),
        unpack_aot_arg(arg2),
    ];
    let registry = crate::backends::common::builtins::BuiltinRegistry::new();
    let res = if let Some(func) = registry.get("input.pin") {
        func(&args)
    } else {
        RuntimeValue::Null
    };
    aot_store_value(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_input_diff(arg0: u64, arg1: u64, arg2: u64) -> u64 {
    let args = vec![
        unpack_aot_arg(arg0),
        unpack_aot_arg(arg1),
        unpack_aot_arg(arg2),
    ];
    let registry = crate::backends::common::builtins::BuiltinRegistry::new();
    let res = if let Some(func) = registry.get("input.diff") {
        func(&args)
    } else {
        RuntimeValue::Null
    };
    aot_store_value(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_input_hotkey(arg0: u64, arg1: u64) -> u64 {
    let args = vec![unpack_aot_arg(arg0), unpack_aot_arg(arg1)];
    let registry = crate::backends::common::builtins::BuiltinRegistry::new();
    let res = if let Some(func) = registry.get("input.hotkey") {
        func(&args)
    } else {
        RuntimeValue::Null
    };
    aot_store_value(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_input_form(arg0: u64, arg1: u64) -> u64 {
    let args = vec![unpack_aot_arg(arg0), unpack_aot_arg(arg1)];
    let registry = crate::backends::common::builtins::BuiltinRegistry::new();
    let res = if let Some(func) = registry.get("input.form") {
        func(&args)
    } else {
        RuntimeValue::Null
    };
    aot_store_value(res)
}

/// AOT C-ABI wrapper for input.* methods
#[unsafe(no_mangle)]
pub extern "C" fn aot_input_method(
    method_name_ptr: *const std::os::raw::c_char,
    arg0: u64,
    arg1: u64,
    arg2: u64,
) -> u64 {
    if method_name_ptr.is_null() {
        return aot_store_value(RuntimeValue::Null);
    }
    let method_name = match unsafe { std::ffi::CStr::from_ptr(method_name_ptr).to_str() } {
        Ok(s) => s,
        Err(_) => return aot_store_value(RuntimeValue::Null),
    };

    let args = vec![
        unpack_aot_arg(arg0),
        unpack_aot_arg(arg1),
        unpack_aot_arg(arg2),
    ];

    let registry = crate::backends::common::builtins::BuiltinRegistry::new();
    let res = if let Some(func) = registry.get(method_name) {
        func(&args)
    } else {
        let prefixed = format!("input.{}", method_name);
        if let Some(func) = registry.get(&prefixed) {
            func(&args)
        } else {
            RuntimeValue::Null
        }
    };
    aot_store_value(res)
}
