//! Runtime Bridge for Native JIT
//!
//! This module provides C-callable functions that bridge JIT-compiled code
//! with the Rust runtime for complex operations like creating and printing
//! arrays, objects, and other composite data structures.

use crate::backends::common::builtins::RuntimeValue;
use crate::utils::collections::FastMap;
use std::cell::RefCell;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::atomic::{AtomicU64, Ordering};

// Thread-local storage for runtime values created by JIT
// We use handles (u64 IDs) to reference these values from JIT code
thread_local! {
    static RUNTIME_VALUES: RefCell<FastMap<u64, RuntimeValue>> = RefCell::new(FastMap::default());
    static JIT_FUNCTIONS: RefCell<FastMap<String, usize>> = RefCell::new(FastMap::default());
}

pub fn register_jit_function(name: String, ptr: usize) {
    JIT_FUNCTIONS.with(|funcs| {
        funcs.borrow_mut().insert(name, ptr);
    });
}

// Global counter for generating unique handles
static HANDLE_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_handle() -> u64 {
    HANDLE_COUNTER.fetch_add(1, Ordering::Relaxed)
}

fn store_value(value: RuntimeValue) -> u64 {
    let handle = next_handle();
    RUNTIME_VALUES.with(|values| {
        values.borrow_mut().insert(handle, value);
    });
    handle
}

fn get_value(handle: u64) -> Option<RuntimeValue> {
    RUNTIME_VALUES.with(|values| values.borrow().get(&handle).cloned())
}

pub fn get_runtime_value(handle: u64) -> Option<RuntimeValue> {
    get_value(handle)
}

fn remove_value(handle: u64) -> Option<RuntimeValue> {
    RUNTIME_VALUES.with(|values| values.borrow_mut().remove(&handle))
}

/// Helper to get a string from either a handle (dynamic) or a pointer (const)
fn get_string_val(handle_or_ptr: u64) -> Option<String> {
    // Arbitrary threshold: Handles are small counters, Pointers are large addresses
    // On x64, user space pointers are typically > 0x10000
    if handle_or_ptr > 1_000_000_000 {
        // Assume pointer
        unsafe {
            let ptr = handle_or_ptr as *const c_char;
            if ptr.is_null() {
                return None;
            }
            CStr::from_ptr(ptr).to_str().ok().map(|s| s.to_string())
        }
    } else {
        // Assume handle
        if let Some(RuntimeValue::String(s)) = get_value(handle_or_ptr) {
            Some(s)
        } else {
            None
        }
    }
}

// ============================================================================
// C-callable runtime functions for JIT
// ============================================================================

/// Create a RuntimeValue handle from an i64
#[unsafe(no_mangle)]
pub extern "C" fn jit_wrap_i64(value: i64) -> u64 {
    store_value(RuntimeValue::Int(value))
}

/// Create a RuntimeValue handle from an i8
#[unsafe(no_mangle)]
pub extern "C" fn jit_wrap_i8(value: i64) -> u64 {
    store_value(RuntimeValue::I8(value as i8))
}

/// Create a RuntimeValue handle from an i16
#[unsafe(no_mangle)]
pub extern "C" fn jit_wrap_i16(value: i64) -> u64 {
    store_value(RuntimeValue::I16(value as i16))
}

/// Create a RuntimeValue handle from an i32
#[unsafe(no_mangle)]
pub extern "C" fn jit_wrap_i32(value: i64) -> u64 {
    store_value(RuntimeValue::I32(value as i32))
}

/// Create a RuntimeValue handle from a u8
#[unsafe(no_mangle)]
pub extern "C" fn jit_wrap_u8(value: i64) -> u64 {
    store_value(RuntimeValue::U8(value as u8))
}

/// Create a RuntimeValue handle from a u16
#[unsafe(no_mangle)]
pub extern "C" fn jit_wrap_u16(value: i64) -> u64 {
    store_value(RuntimeValue::U16(value as u16))
}

/// Create a RuntimeValue handle from a u32
#[unsafe(no_mangle)]
pub extern "C" fn jit_wrap_u32(value: i64) -> u64 {
    store_value(RuntimeValue::U32(value as u32))
}

/// Create a RuntimeValue handle from u64 bits (passed via i64 ABI)
#[unsafe(no_mangle)]
pub extern "C" fn jit_wrap_u64(value: i64) -> u64 {
    store_value(RuntimeValue::U64(value as u64))
}

/// Create a RuntimeValue handle from an f32
#[unsafe(no_mangle)]
pub extern "C" fn jit_wrap_f32(value: f64) -> u64 {
    store_value(RuntimeValue::F32(value as f32))
}

/// Create a RuntimeValue handle from an f64
#[unsafe(no_mangle)]
pub extern "C" fn jit_wrap_f64(value: f64) -> u64 {
    store_value(RuntimeValue::Float(value))
}

/// Create a RuntimeValue handle from a bool (i64)
#[unsafe(no_mangle)]
pub extern "C" fn jit_wrap_bool(value: i64) -> u64 {
    store_value(RuntimeValue::Bool(value != 0))
}

/// Create a RuntimeValue handle representing null
#[unsafe(no_mangle)]
pub extern "C" fn jit_wrap_null() -> u64 {
    store_value(RuntimeValue::Null)
}

/// Compute deep sizeof for a RuntimeValue
fn runtime_bridge_sizeof_value(value: &RuntimeValue) -> usize {
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
        RuntimeValue::Bool(_) => 1,
        RuntimeValue::Char(_) => 4,
        RuntimeValue::Null => 0,
        RuntimeValue::String(s) => s.len(),
        RuntimeValue::BigInt(bi) => {
            use num_traits::Zero;
            if bi.is_zero() {
                8
            } else {
                8 + (bi.bits() as usize / 32 + 1) * 4
            }
        }
        RuntimeValue::Array(arr) => {
            let base = 24;
            let elements: usize = arr.iter().map(runtime_bridge_sizeof_value).sum();
            base + elements
        }
        RuntimeValue::Set(set_vals) => {
            let base = 24;
            let elements: usize = set_vals.iter().map(runtime_bridge_sizeof_value).sum();
            base + elements
        }
        RuntimeValue::Tuple(tup) => {
            let base = 24;
            let elements: usize = tup.iter().map(runtime_bridge_sizeof_value).sum();
            base + elements
        }
        RuntimeValue::RawArray(_, arr) => {
            let elements: usize = arr.iter().map(runtime_bridge_sizeof_value).sum();
            elements
        }
        RuntimeValue::DynArray { data, .. } => {
            let base = 24;
            let elements: usize = data.iter().map(runtime_bridge_sizeof_value).sum();
            base + elements
        }
        RuntimeValue::Object(obj) => {
            let base = 48;
            let entries: usize = obj
                .iter()
                .map(|(k, v)| k.len() + runtime_bridge_sizeof_value(v))
                .sum();
            base + entries
        }
        RuntimeValue::Promise(_) => 8,
        RuntimeValue::Function(_) => 16,
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

/// Compute deep sizeof for a RuntimeValue handle (objects, arrays, etc.)
#[unsafe(no_mangle)]
pub extern "C" fn jit_sizeof_handle(handle: u64) -> i64 {
    if let Some(val) = get_value(handle) {
        runtime_bridge_sizeof_value(&val) as i64
    } else {
        8
    }
}

/// Returns seconds elapsed since program start (high-resolution timer)
#[unsafe(no_mangle)]
pub extern "C" fn jit_clock() -> f64 {
    use once_cell::sync::Lazy;
    use std::time::Instant;
    static START: Lazy<Instant> = Lazy::new(Instant::now);
    START.elapsed().as_secs_f64()
}

/// Create a RuntimeValue handle from a Unicode scalar code point
#[unsafe(no_mangle)]
pub extern "C" fn jit_wrap_char(value: i64) -> u64 {
    let ch = char::from_u32(value as u32).unwrap_or('\0');
    store_value(RuntimeValue::Char(ch))
}

/// Create a RuntimeValue handle from a string pointer
#[unsafe(no_mangle)]
pub extern "C" fn jit_wrap_str(ptr: i64) -> u64 {
    if ptr == 0 {
        return store_value(RuntimeValue::String(String::new()));
    }
    unsafe {
        let c_ptr = ptr as *const c_char;
        if let Ok(s) = std::ffi::CStr::from_ptr(c_ptr).to_str() {
            store_value(RuntimeValue::String(s.to_string()))
        } else {
            store_value(RuntimeValue::String(String::new()))
        }
    }
}

/// Convert a string handle/pointer to a RuntimeValue::Char (first character)
#[unsafe(no_mangle)]
pub extern "C" fn jit_char_from_str(handle_or_ptr: i64) -> u64 {
    let ch = get_string_val(handle_or_ptr as u64)
        .and_then(|s| s.chars().next())
        .unwrap_or('\0');
    store_value(RuntimeValue::Char(ch))
}

/// Create an empty array and return its handle
#[unsafe(no_mangle)]
pub extern "C" fn jit_make_array() -> u64 {
    store_value(RuntimeValue::Array(Vec::new()))
}

/// Create an array with a specific capacity
#[unsafe(no_mangle)]
pub extern "C" fn jit_make_array_capacity(capacity: u64) -> u64 {
    let arr = Vec::with_capacity(capacity as usize);
    store_value(RuntimeValue::Array(arr))
}

/// Push an integer value to an array
#[unsafe(no_mangle)]
pub extern "C" fn jit_array_push_int(arr_handle: u64, value: i64) -> u64 {
    if let Some(RuntimeValue::Array(mut arr)) = remove_value(arr_handle) {
        arr.push(RuntimeValue::Int(value));
        store_value(RuntimeValue::Array(arr))
    } else if let Some(RuntimeValue::DynArray {
        mut data,
        element_type,
        concrete_type,
        tracked_capacity,
    }) = remove_value(arr_handle)
    {
        data.push(RuntimeValue::Int(value));
        store_value(RuntimeValue::DynArray {
            data,
            element_type,
            concrete_type,
            tracked_capacity,
        })
    } else {
        0
    }
}

/// Push a float value to an array
#[unsafe(no_mangle)]
pub extern "C" fn jit_array_push_float(arr_handle: u64, value: f64) -> u64 {
    if let Some(RuntimeValue::Array(mut arr)) = remove_value(arr_handle) {
        arr.push(RuntimeValue::Float(value));
        store_value(RuntimeValue::Array(arr))
    } else if let Some(RuntimeValue::DynArray {
        mut data,
        element_type,
        concrete_type,
        tracked_capacity,
    }) = remove_value(arr_handle)
    {
        data.push(RuntimeValue::Float(value));
        store_value(RuntimeValue::DynArray {
            data,
            element_type,
            concrete_type,
            tracked_capacity,
        })
    } else {
        0
    }
}

/// Push a string value to an array (takes a C string pointer)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_array_push_str(arr_handle: u64, value: *const c_char) -> u64 {
    let s = if value.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(value).to_string_lossy().into_owned() }
    };

    if let Some(RuntimeValue::Array(mut arr)) = remove_value(arr_handle) {
        arr.push(RuntimeValue::String(s));
        store_value(RuntimeValue::Array(arr))
    } else if let Some(RuntimeValue::DynArray {
        mut data,
        element_type,
        concrete_type,
        tracked_capacity,
    }) = remove_value(arr_handle)
    {
        data.push(RuntimeValue::String(s));
        store_value(RuntimeValue::DynArray {
            data,
            element_type,
            concrete_type,
            tracked_capacity,
        })
    } else {
        0
    }
}

/// Push a boolean value to an array
#[unsafe(no_mangle)]
pub extern "C" fn jit_array_push_bool(arr_handle: u64, value: i64) -> u64 {
    if let Some(RuntimeValue::Array(mut arr)) = remove_value(arr_handle) {
        arr.push(RuntimeValue::Bool(value != 0));
        store_value(RuntimeValue::Array(arr))
    } else if let Some(RuntimeValue::DynArray {
        mut data,
        element_type,
        concrete_type,
        tracked_capacity,
    }) = remove_value(arr_handle)
    {
        data.push(RuntimeValue::Bool(value != 0));
        store_value(RuntimeValue::DynArray {
            data,
            element_type,
            concrete_type,
            tracked_capacity,
        })
    } else {
        0
    }
}

/// Push null to an array
#[unsafe(no_mangle)]
pub extern "C" fn jit_array_push_null(arr_handle: u64) -> u64 {
    if let Some(RuntimeValue::Array(mut arr)) = remove_value(arr_handle) {
        arr.push(RuntimeValue::Null);
        store_value(RuntimeValue::Array(arr))
    } else if let Some(RuntimeValue::DynArray {
        mut data,
        element_type,
        concrete_type,
        tracked_capacity,
    }) = remove_value(arr_handle)
    {
        data.push(RuntimeValue::Null);
        store_value(RuntimeValue::DynArray {
            data,
            element_type,
            concrete_type,
            tracked_capacity,
        })
    } else {
        0
    }
}

/// Push a nested value (another handle) to an array
#[unsafe(no_mangle)]
pub extern "C" fn jit_array_push_handle(arr_handle: u64, value_handle: u64) -> u64 {
    let nested_value = get_value(value_handle).unwrap_or(RuntimeValue::Null);

    if let Some(RuntimeValue::Array(mut arr)) = remove_value(arr_handle) {
        arr.push(nested_value);
        store_value(RuntimeValue::Array(arr))
    } else if let Some(RuntimeValue::DynArray {
        mut data,
        element_type,
        concrete_type,
        tracked_capacity,
    }) = remove_value(arr_handle)
    {
        data.push(nested_value);
        store_value(RuntimeValue::DynArray {
            data,
            element_type,
            concrete_type,
            tracked_capacity,
        })
    } else {
        0
    }
}

/// Convert an array handle to a set handle (deduplicated, preserves insertion order)
#[unsafe(no_mangle)]
pub extern "C" fn jit_make_set_from_array(arr_handle: u64) -> u64 {
    match remove_value(arr_handle) {
        Some(RuntimeValue::Array(arr)) => {
            let mut unique: Vec<RuntimeValue> = Vec::new();
            for value in arr {
                if !unique.iter().any(|existing| existing == &value) {
                    unique.push(value);
                }
            }
            store_value(RuntimeValue::Set(unique))
        }
        Some(RuntimeValue::Set(set_vals)) => store_value(RuntimeValue::Set(set_vals)),
        Some(RuntimeValue::Tuple(values)) => {
            let mut unique: Vec<RuntimeValue> = Vec::new();
            for value in values {
                if !unique.iter().any(|existing| existing == &value) {
                    unique.push(value);
                }
            }
            store_value(RuntimeValue::Set(unique))
        }
        Some(other) => store_value(RuntimeValue::Set(vec![other])),
        None => 0,
    }
}

/// Create an empty object and return its handle
#[unsafe(no_mangle)]
pub extern "C" fn jit_make_object() -> u64 {
    store_value(RuntimeValue::Object(FastMap::default()))
}

/// Create an empty tuple and return its handle
#[unsafe(no_mangle)]
pub extern "C" fn jit_make_tuple() -> u64 {
    store_value(RuntimeValue::Tuple(Vec::new()))
}

/// Push a value to a tuple (similar to array push functions)
#[unsafe(no_mangle)]
pub extern "C" fn jit_tuple_push_int(tuple_handle: u64, value: i64) -> u64 {
    if let Some(RuntimeValue::Tuple(mut tup)) = remove_value(tuple_handle) {
        tup.push(RuntimeValue::Int(value));
        store_value(RuntimeValue::Tuple(tup))
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_tuple_push_float(tuple_handle: u64, value: f64) -> u64 {
    if let Some(RuntimeValue::Tuple(mut tup)) = remove_value(tuple_handle) {
        tup.push(RuntimeValue::Float(value));
        store_value(RuntimeValue::Tuple(tup))
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_tuple_push_str(tuple_handle: u64, value: *const c_char) -> u64 {
    let s = if value.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(value).to_string_lossy().into_owned() }
    };

    if let Some(RuntimeValue::Tuple(mut tup)) = remove_value(tuple_handle) {
        tup.push(RuntimeValue::String(s));
        store_value(RuntimeValue::Tuple(tup))
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_tuple_push_bool(tuple_handle: u64, value: i64) -> u64 {
    if let Some(RuntimeValue::Tuple(mut tup)) = remove_value(tuple_handle) {
        tup.push(RuntimeValue::Bool(value != 0));
        store_value(RuntimeValue::Tuple(tup))
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_tuple_push_null(tuple_handle: u64) -> u64 {
    if let Some(RuntimeValue::Tuple(mut tup)) = remove_value(tuple_handle) {
        tup.push(RuntimeValue::Null);
        store_value(RuntimeValue::Tuple(tup))
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_tuple_push_handle(tuple_handle: u64, value_handle: u64) -> u64 {
    let nested_value = get_value(value_handle).unwrap_or(RuntimeValue::Null);

    if let Some(RuntimeValue::Tuple(mut tup)) = remove_value(tuple_handle) {
        tup.push(nested_value);
        store_value(RuntimeValue::Tuple(tup))
    } else {
        0
    }
}

/// Set a field on an object (string value)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_object_set_str(
    obj_handle: u64,
    field: *const c_char,
    value: *const c_char,
) -> u64 {
    let field_name = if field.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(field).to_string_lossy().into_owned() }
    };

    let field_value = if value.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(value).to_string_lossy().into_owned() }
    };

    if let Some(RuntimeValue::Object(mut obj)) = remove_value(obj_handle) {
        obj.insert(field_name, RuntimeValue::String(field_value));
        store_value(RuntimeValue::Object(obj))
    } else {
        0
    }
}

/// Set a field on an object (int value)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_object_set_int(
    obj_handle: u64,
    field: *const c_char,
    value: i64,
) -> u64 {
    let field_name = if field.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(field).to_string_lossy().into_owned() }
    };

    if let Some(RuntimeValue::Object(mut obj)) = remove_value(obj_handle) {
        obj.insert(field_name, RuntimeValue::Int(value));
        store_value(RuntimeValue::Object(obj))
    } else {
        0
    }
}

/// Set a field on an object (float value)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_object_set_float(
    obj_handle: u64,
    field: *const c_char,
    value: f64,
) -> u64 {
    let field_name = if field.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(field).to_string_lossy().into_owned() }
    };

    if let Some(RuntimeValue::Object(mut obj)) = remove_value(obj_handle) {
        obj.insert(field_name, RuntimeValue::Float(value));
        store_value(RuntimeValue::Object(obj))
    } else {
        0
    }
}

/// Set a field on an object (bool value)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_object_set_bool(
    obj_handle: u64,
    field: *const c_char,
    value: i64,
) -> u64 {
    let field_name = if field.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(field).to_string_lossy().into_owned() }
    };

    if let Some(RuntimeValue::Object(mut obj)) = remove_value(obj_handle) {
        obj.insert(field_name, RuntimeValue::Bool(value != 0));
        store_value(RuntimeValue::Object(obj))
    } else {
        0
    }
}

/// Set a field on an object (nested handle value)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_object_set_handle(
    obj_handle: u64,
    field: *const c_char,
    value_handle: u64,
) -> u64 {
    let field_name = if field.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(field).to_string_lossy().into_owned() }
    };

    let nested_value = get_value(value_handle).unwrap_or(RuntimeValue::Null);

    if let Some(RuntimeValue::Object(mut obj)) = remove_value(obj_handle) {
        obj.insert(field_name, nested_value);
        store_value(RuntimeValue::Object(obj))
    } else {
        0
    }
}

/// Free a runtime value by its handle
#[unsafe(no_mangle)]
pub extern "C" fn jit_free_handle(handle: u64) {
    remove_value(handle);
}

// ============================================================================
// Pretty print functions
// ============================================================================

/// Pretty print a RuntimeValue with colors and formatting
/// mode: 0 = normal (no pretty), 1 = full (colors + types), 2 = compact (inline, no colors), 3 = simple (basic colors)
fn pretty_format(value: &RuntimeValue, mode: i64, depth: usize) -> String {
    use std::fmt::Write;

    // Color codes based on mode
    let (
        key_color,
        str_color,
        num_color,
        bool_color,
        null_color,
        bracket_color,
        type_hint_color,
        reset,
    ) = if mode == 3 {
        // Simple colors
        (
            "\x1b[36m", "\x1b[33m", "\x1b[32m", "\x1b[35m", "\x1b[90m", "\x1b[37m", "\x1b[96m",
            "\x1b[0m",
        )
    } else if mode == 2 {
        // Compact - no colors
        ("", "", "", "", "", "", "", "")
    } else {
        // Full colors (default for mode 1)
        (
            "\x1b[38;2;156;220;254m",
            "\x1b[38;2;206;145;120m",
            "\x1b[38;2;181;206;168m",
            "\x1b[38;2;86;156;214m",
            "\x1b[38;2;128;128;128m",
            "\x1b[38;2;212;212;212m",
            "\x1b[38;2;78;201;176m",
            "\x1b[0m",
        )
    };

    let show_types = mode == 1; // Full mode shows types
    let is_compact = mode == 2; // Compact mode is inline (no newlines/indentation)
    let indent_str = "  ";
    let max_depth = 10;

    if depth >= max_depth {
        return format!("{}<max depth>{}", null_color, reset);
    }

    match value {
        RuntimeValue::Null => format!("{}null{}", null_color, reset),
        RuntimeValue::Bool(b) => {
            let mut s = format!("{}{}{}", bool_color, b, reset);
            if show_types {
                write!(s, " {}⟨bool⟩{}", type_hint_color, reset).unwrap();
            }
            s
        }
        RuntimeValue::Char(c) => {
            let mut s = format!("{}'{}'{}", str_color, c, reset);
            if show_types {
                write!(s, " {}⟨char⟩{}", type_hint_color, reset).unwrap();
            }
            s
        }
        RuntimeValue::Int(n) => {
            let mut s = format!("{}{}{}", num_color, n, reset);
            if show_types {
                write!(s, " {}⟨int⟩{}", type_hint_color, reset).unwrap();
            }
            s
        }
        RuntimeValue::Float(n) => {
            let mut s = format!("{}{}{}", num_color, n, reset);
            if show_types {
                write!(s, " {}⟨number⟩{}", type_hint_color, reset).unwrap();
            }
            s
        }
        RuntimeValue::String(st) => {
            let mut s = format!("{}\"{}\"{}", str_color, st, reset);
            if show_types {
                write!(s, " {}⟨string⟩{}", type_hint_color, reset).unwrap();
            }
            s
        }
        RuntimeValue::Array(arr) | RuntimeValue::RawArray(_, arr) => format_array(
            arr,
            mode,
            depth,
            is_compact,
            indent_str,
            bracket_color,
            reset,
            show_types,
            type_hint_color,
        ),
        RuntimeValue::Set(set_vals) => {
            if set_vals.is_empty() {
                let mut s = format!("{}{{}}{}", bracket_color, reset);
                if show_types {
                    write!(s, " {}⟨set⟩{}", type_hint_color, reset).unwrap();
                }
                s
            } else if is_compact {
                let items: Vec<String> = set_vals
                    .iter()
                    .map(|v| pretty_format(v, mode, depth + 1))
                    .collect();
                format!("{}{{{}}}{}", bracket_color, items.join(", "), reset)
            } else {
                let new_indent = indent_str.repeat(depth + 1);
                let close_indent = indent_str.repeat(depth);
                let items: Vec<String> = set_vals
                    .iter()
                    .map(|v| format!("{}{}", new_indent, pretty_format(v, mode, depth + 1)))
                    .collect();
                let mut s = format!(
                    "{}{{{}\n{}\n{}}}{}",
                    bracket_color,
                    reset,
                    items.join(",\n"),
                    close_indent,
                    reset
                );
                if show_types {
                    write!(s, " {}⟨set[{}]⟩{}", type_hint_color, set_vals.len(), reset).unwrap();
                }
                s
            }
        }
        RuntimeValue::DynArray { data, .. } => format_array(
            data,
            mode,
            depth,
            is_compact,
            indent_str,
            bracket_color,
            reset,
            show_types,
            type_hint_color,
        ),
        RuntimeValue::Tuple(tup) => {
            if tup.is_empty() {
                let mut s = format!("{}(){}", bracket_color, reset);
                if show_types {
                    write!(s, " {}⟨tuple⟩{}", type_hint_color, reset).unwrap();
                }
                return s;
            }
            if is_compact {
                // Inline format for compact mode
                let items: Vec<String> = tup
                    .iter()
                    .map(|v| pretty_format(v, mode, depth + 1))
                    .collect();
                format!("{}({}){}", bracket_color, items.join(", "), reset)
            } else {
                let new_indent = indent_str.repeat(depth + 1);
                let close_indent = indent_str.repeat(depth);
                let items: Vec<String> = tup
                    .iter()
                    .map(|v| format!("{}{}", new_indent, pretty_format(v, mode, depth + 1)))
                    .collect();
                let mut s = format!(
                    "{}({}\n{}\n{}){}",
                    bracket_color,
                    reset,
                    items.join(",\n"),
                    close_indent,
                    reset
                );
                if show_types {
                    write!(s, " {}⟨tuple[{}]⟩{}", type_hint_color, tup.len(), reset).unwrap();
                }
                s
            }
        }
        RuntimeValue::Object(obj) => format_object(
            obj,
            mode,
            depth,
            is_compact,
            indent_str,
            key_color,
            bracket_color,
            reset,
            show_types,
            type_hint_color,
        ),
        // Fixed-width types with type hints
        RuntimeValue::I8(n) => {
            format_fixed_int(n, "i8", num_color, reset, show_types, type_hint_color)
        }
        RuntimeValue::I16(n) => {
            format_fixed_int(n, "i16", num_color, reset, show_types, type_hint_color)
        }
        RuntimeValue::I32(n) => {
            format_fixed_int(n, "i32", num_color, reset, show_types, type_hint_color)
        }
        RuntimeValue::I64(n) => {
            format_fixed_int(n, "i64", num_color, reset, show_types, type_hint_color)
        }
        RuntimeValue::I128(n) => {
            format_fixed_int(n, "i128", num_color, reset, show_types, type_hint_color)
        }
        RuntimeValue::U8(n) => {
            format_fixed_int(n, "u8", num_color, reset, show_types, type_hint_color)
        }
        RuntimeValue::U16(n) => {
            format_fixed_int(n, "u16", num_color, reset, show_types, type_hint_color)
        }
        RuntimeValue::U32(n) => {
            format_fixed_int(n, "u32", num_color, reset, show_types, type_hint_color)
        }
        RuntimeValue::U64(n) => {
            format_fixed_int(n, "u64", num_color, reset, show_types, type_hint_color)
        }
        RuntimeValue::U128(n) => {
            format_fixed_int(n, "u128", num_color, reset, show_types, type_hint_color)
        }
        RuntimeValue::F32(n) => {
            format_fixed_int(n, "f32", num_color, reset, show_types, type_hint_color)
        }
        RuntimeValue::F64(n) => {
            format_fixed_int(n, "f64", num_color, reset, show_types, type_hint_color)
        }
        RuntimeValue::BigInt(bi) => {
            let mut s = format!("{}{}{}", num_color, bi, reset);
            if show_types {
                use std::fmt::Write;
                write!(s, " {}⟨bigint⟩{}", type_hint_color, reset).unwrap();
            }
            s
        }
        RuntimeValue::Promise(id) => format!("{}Promise({}){}", type_hint_color, id, reset),
        RuntimeValue::Function(f) => format!("{}⟨function {}⟩{}", type_hint_color, f.name, reset),
    }
}

fn format_fixed_int<T: std::fmt::Display>(
    value: &T,
    type_name: &str,
    num_color: &str,
    reset: &str,
    show_types: bool,
    type_hint_color: &str,
) -> String {
    use std::fmt::Write;
    let mut s = format!("{}{}{}", num_color, value, reset);
    if show_types {
        write!(s, " {}⟨{}⟩{}", type_hint_color, type_name, reset).unwrap();
    }
    s
}

fn format_array(
    arr: &[RuntimeValue],
    mode: i64,
    depth: usize,
    is_compact: bool,
    indent_str: &str,
    bracket_color: &str,
    reset: &str,
    show_types: bool,
    type_hint_color: &str,
) -> String {
    use std::fmt::Write;
    if arr.is_empty() {
        let mut s = format!("{}[]{}", bracket_color, reset);
        if show_types {
            write!(s, " {}⟨array⟩{}", type_hint_color, reset).unwrap();
        }
        return s;
    }

    if is_compact {
        // Inline format for compact mode
        let items: Vec<String> = arr
            .iter()
            .map(|v| pretty_format(v, mode, depth + 1))
            .collect();
        format!("{}[{}]{}", bracket_color, items.join(", "), reset)
    } else {
        let new_indent = indent_str.repeat(depth + 1);
        let close_indent = indent_str.repeat(depth);
        let items: Vec<String> = arr
            .iter()
            .map(|v| format!("{}{}", new_indent, pretty_format(v, mode, depth + 1)))
            .collect();
        let mut s = format!(
            "{}[{}\n{}\n{}]{}",
            bracket_color,
            reset,
            items.join(",\n"),
            close_indent,
            reset
        );
        if show_types {
            write!(s, " {}⟨array[{}]⟩{}", type_hint_color, arr.len(), reset).unwrap();
        }
        s
    }
}

fn format_object(
    obj: &FastMap<String, RuntimeValue>,
    mode: i64,
    depth: usize,
    is_compact: bool,
    indent_str: &str,
    key_color: &str,
    bracket_color: &str,
    reset: &str,
    show_types: bool,
    type_hint_color: &str,
) -> String {
    use std::fmt::Write;
    if obj.is_empty() {
        let mut s = format!("{}{{}}{}", bracket_color, reset);
        if show_types {
            write!(s, " {}⟨object⟩{}", type_hint_color, reset).unwrap();
        }
        return s;
    }

    if is_compact {
        // Inline format for compact mode
        let items: Vec<String> = obj
            .iter()
            .map(|(k, v)| format!("{}: {}", k, pretty_format(v, mode, depth + 1)))
            .collect();
        format!("{}{{{}}}{}", bracket_color, items.join(", "), reset)
    } else {
        let new_indent = indent_str.repeat(depth + 1);
        let close_indent = indent_str.repeat(depth);
        let items: Vec<String> = obj
            .iter()
            .map(|(k, v)| {
                format!(
                    "{}{}{}{}: {}",
                    new_indent,
                    key_color,
                    k,
                    reset,
                    pretty_format(v, mode, depth + 1)
                )
            })
            .collect();
        let mut s = format!(
            "{}{{{}\n{}\n{}}}{}",
            bracket_color,
            reset,
            items.join(",\n"),
            close_indent,
            reset
        );
        if show_types {
            write!(s, " {}⟨object⟩{}", type_hint_color, reset).unwrap();
        }
        s
    }
}

fn infer_int_value_type(n: i64) -> crate::parsing::ast::Value {
    use crate::parsing::ast::Value;

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

/// Print a value by handle
/// mode: 0 = no formatting, 1 = full pretty print, 2 = compact, 3 = simple colors
#[unsafe(no_mangle)]
pub extern "C" fn jit_print_value(handle: u64, mode: i64) -> u64 {
    use crate::execution::runtime::pretty_print::{PrettyPrintOptions, pretty_print};
    use crate::parsing::ast::{ArrayElementType, DynamicArray, NativeFn, Value};
    use std::io::Write;
    use std::sync::Arc;

    // Helper to convert RuntimeValue to Value for pretty printing
    fn runtime_value_to_value(v: &RuntimeValue) -> Value {
        match v {
            RuntimeValue::Int(n) => infer_int_value_type(*n),
            RuntimeValue::Float(n) => Value::Number(*n),
            RuntimeValue::Bool(b) => Value::Bool(*b),
            RuntimeValue::Char(c) => Value::Char(*c),
            RuntimeValue::String(s) => Value::Str(s.clone()),
            RuntimeValue::BigInt(bi) => Value::BigInt(bi.clone()),
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
            RuntimeValue::Array(arr) => {
                Value::Array(arr.iter().map(runtime_value_to_value).collect())
            }
            RuntimeValue::Set(set_vals) => {
                Value::Set(set_vals.iter().map(runtime_value_to_value).collect())
            }
            RuntimeValue::Tuple(tup) => {
                Value::Tuple(tup.iter().map(runtime_value_to_value).collect())
            }
            RuntimeValue::Object(obj) => {
                let mut map: rustc_hash::FxHashMap<String, Value> =
                    rustc_hash::FxHashMap::default();
                for (k, v) in obj.iter() {
                    map.insert(k.clone(), runtime_value_to_value(v));
                }
                Value::Object(Arc::new(map))
            }
            RuntimeValue::Promise(id) => Value::Promise(*id),
            RuntimeValue::Function(_) => {
                Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Null))))
            }
            RuntimeValue::RawArray(elem_ty, values) => Value::RawArray(
                elem_ty.clone(),
                values.iter().map(runtime_value_to_value).collect(),
            ),
            RuntimeValue::DynArray {
                data,
                element_type,
                concrete_type,
                tracked_capacity,
            } => {
                let converted: Vec<Value> = data.iter().map(runtime_value_to_value).collect();
                Value::DynArray(Box::new(DynamicArray {
                    data: converted,
                    element_type: ArrayElementType::from_type_name(element_type),
                    concrete_type: concrete_type.clone(),
                    tracked_capacity: tracked_capacity.unwrap_or(data.len()),
                }))
            }
            RuntimeValue::Null => Value::Null,
        }
    }

    let value = if let Some(value) = get_value(handle) {
        value
    } else if let Some(str_val) = get_string_val(handle) {
        RuntimeValue::String(str_val)
    } else {
        RuntimeValue::Int(handle as i64)
    };

    // Use pretty print if mode > 0
    let output = if mode > 0 {
        let pretty_opts = match mode {
            2 => PrettyPrintOptions::compact(),
            3 => PrettyPrintOptions::simple_color(),
            _ => PrettyPrintOptions::default(), // 1 = full
        };
        let ast_value = runtime_value_to_value(&value);
        pretty_print(&ast_value, &pretty_opts)
    } else {
        // mode 0 = standard runtime formatting to match interpreter output shape
        let ast_value = runtime_value_to_value(&value);
        crate::execution::runtime_core::format::fmt(&ast_value)
    };

    print!("{}", output);
    let _ = std::io::stdout().flush();
    0
}

/// Print a value by handle using an options object handle (supports dynamic pretty mode)
#[unsafe(no_mangle)]
pub extern "C" fn jit_print_with_options(handle: u64, options_handle: u64) -> u64 {
    use std::io::Write;

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

    let (mode, end, style_prefix, style_reset) = match get_value(options_handle) {
        Some(RuntimeValue::Object(opts))
            if opts.keys().any(|k| {
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
            }) =>
        {
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
            let get_bool = |name: &str| -> Option<bool> {
                opts.iter().find_map(|(k, v)| {
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
            let get_int = |name: &str| -> Option<i64> {
                opts.iter().find_map(|(k, v)| {
                    if normalize_key(k) == name {
                        if let RuntimeValue::Int(n) = v {
                            Some(*n)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
            };

            let mode = match get_str("pretty") {
                Some(s) => match s.as_str() {
                    "compact" => 2,
                    "simple" => 3,
                    "full" | "true" => 1,
                    "none" | "false" => 0,
                    _ => 1,
                },
                None => get_bool("pretty")
                    .map(|b| if b { 1 } else { 0 })
                    .unwrap_or_else(|| {
                        get_int("pretty")
                            .map(|n| if n == 0 { 0 } else { 1 })
                            .unwrap_or(0)
                    }),
            };

            let end = get_str("end").unwrap_or_else(|| "\n".to_string());

            let style_prefix = build_style_prefix(&opts);
            let style_reset = if style_prefix.is_empty() {
                String::new()
            } else {
                "\x1b[0m".to_string()
            };

            (mode, end, style_prefix, style_reset)
        }
        _ => (0, "\n".to_string(), String::new(), String::new()),
    };

    if !style_prefix.is_empty() {
        print!("{}", style_prefix);
    }
    let _ = jit_print_value(handle, mode);
    if !style_reset.is_empty() {
        print!("{}", style_reset);
    }
    if !end.is_empty() {
        print!("{}", end);
    }
    let _ = std::io::stdout().flush();
    0
}

/// Print multiple value handles with options object (pretty/sep/end).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_print_values_with_options(
    values_ptr: *const u64,
    values_count: i64,
    options_handle: u64,
) -> u64 {
    use std::io::Write;

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

    let count = values_count as usize;
    let values: &[u64] = if count == 0 {
        &[]
    } else {
        if values_ptr.is_null() {
            return 0;
        }
        unsafe { std::slice::from_raw_parts(values_ptr, count) }
    };

    let (has_options, mode, sep, end, style_prefix, style_reset, file_opt) =
        match get_value(options_handle) {
            Some(RuntimeValue::Object(opts))
                if opts.keys().any(|k| {
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
                }) =>
            {
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
                let get_bool = |name: &str| -> Option<bool> {
                    opts.iter().find_map(|(k, v)| {
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
                let get_int = |name: &str| -> Option<i64> {
                    opts.iter().find_map(|(k, v)| {
                        if normalize_key(k) == name {
                            if let RuntimeValue::Int(n) = v {
                                Some(*n)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                };

                let mode = match get_str("pretty") {
                    Some(s) => match s.as_str() {
                        "compact" => 2,
                        "simple" => 3,
                        "full" | "true" => 1,
                        "none" | "false" => 0,
                        _ => 1,
                    },
                    None => get_bool("pretty")
                        .map(|b| if b { 1 } else { 0 })
                        .unwrap_or_else(|| {
                            get_int("pretty")
                                .map(|n| if n == 0 { 0 } else { 1 })
                                .unwrap_or(0)
                        }),
                };

                let sep = get_str("sep").unwrap_or_else(|| " ".to_string());
                let end = get_str("end").unwrap_or_else(|| "\n".to_string());
                let file_opt = get_str("file");

                let style_prefix = build_style_prefix(&opts);
                let style_reset = if style_prefix.is_empty() {
                    String::new()
                } else {
                    "\x1b[0m".to_string()
                };

                (true, mode, sep, end, style_prefix, style_reset, file_opt)
            }
            _ => (
                false,
                0,
                " ".to_string(),
                "\n".to_string(),
                String::new(),
                String::new(),
                None,
            ),
        };

    let effective_values: &[u64] = if has_options && values.last() == Some(&options_handle) {
        &values[..values.len().saturating_sub(1)]
    } else {
        values
    };

    if let Some(file_path) = file_opt {
        use std::fs::OpenOptions;
        use std::io::Write;
        let mut file_content = String::new();
        for (i, handle) in effective_values.iter().enumerate() {
            if i > 0 {
                file_content.push_str(&sep);
            }
            if !style_prefix.is_empty() {
                file_content.push_str(&style_prefix);
            }
            if let Some(v) = get_value(*handle) {
                let ast_value = runtime_value_to_ast_value(&v);
                if mode > 0 {
                    use crate::execution::runtime_core::pretty_print::{
                        PrettyPrintOptions, pretty_print,
                    };
                    let pretty_opts = match mode {
                        2 => PrettyPrintOptions::compact(),
                        3 => PrettyPrintOptions::simple_color(),
                        _ => PrettyPrintOptions::default(),
                    };
                    file_content.push_str(&pretty_print(&ast_value, &pretty_opts));
                } else {
                    file_content.push_str(&crate::execution::runtime_core::format::fmt(&ast_value));
                }
            }
            if !style_reset.is_empty() {
                file_content.push_str(&style_reset);
            }
        }
        if !end.is_empty() {
            file_content.push_str(&end);
        }
        if let Ok(mut f) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
        {
            let _ = f.write_all(file_content.as_bytes());
            let _ = f.flush();
        }
        return 0;
    }

    for (i, handle) in effective_values.iter().enumerate() {
        if i > 0 {
            print!("{}", sep);
        }
        if !style_prefix.is_empty() {
            print!("{}", style_prefix);
        }
        let _ = jit_print_value(*handle, mode);
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

/// Print a newline to stdout - used after multi-arg print calls
#[unsafe(no_mangle)]
pub extern "C" fn jit_print_newline() -> u64 {
    use std::io::Write;
    println!();
    let _ = std::io::stdout().flush();
    0
}

/// Print a space to stdout - used between multi-arg print values
#[unsafe(no_mangle)]
pub extern "C" fn jit_print_space() -> u64 {
    use std::io::Write;
    print!(" ");
    let _ = std::io::stdout().flush();
    0
}

/// Print an i64 integer with optional newline
#[unsafe(no_mangle)]
pub extern "C" fn jit_print_i64(value: i64, newline: i64) -> u64 {
    use std::io::Write;
    if newline != 0 {
        println!("{}", value);
    } else {
        print!("{}", value);
    }
    let _ = std::io::stdout().flush();
    0
}

/// Print a u64 integer with optional newline (value passed via i64 ABI bits)
#[unsafe(no_mangle)]
pub extern "C" fn jit_print_u64(value: i64, newline: i64) -> u64 {
    use std::io::Write;
    let unsigned = value as u64;
    if newline != 0 {
        println!("{}", unsigned);
    } else {
        print!("{}", unsigned);
    }
    let _ = std::io::stdout().flush();
    0
}

/// Print a boolean with optional newline
#[unsafe(no_mangle)]
pub extern "C" fn jit_print_bool(value: i64, newline: i64) -> u64 {
    use std::io::Write;
    let bool_str = if value != 0 { "true" } else { "false" };
    if newline != 0 {
        println!("{}", bool_str);
    } else {
        print!("{}", bool_str);
    }
    let _ = std::io::stdout().flush();
    0
}

/// Print an f64 float with optional newline
#[unsafe(no_mangle)]
pub extern "C" fn jit_print_f64(value: f64, newline: i64) -> u64 {
    use std::io::Write;
    if newline != 0 {
        println!("{}", value);
    } else {
        print!("{}", value);
    }
    let _ = std::io::stdout().flush();
    0
}

/// Print a string pointer with optional newline
#[unsafe(no_mangle)]
pub extern "C" fn jit_print_str_raw(string_ptr: i64, newline: i64) -> u64 {
    use std::io::Write;
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

/// Print an object by handle with optional newline
/// handle: handle to a RuntimeValue::Object
/// newline: 0 = no newline, 1 = with newline
#[unsafe(no_mangle)]
pub extern "C" fn jit_print_object(handle: u64, newline: i64) -> u64 {
    use std::io::Write;

    // Use the standard as_string() which handles nested objects with depth limiting (max 20)
    let formatted = if let Some(value) = get_value(handle) {
        value.as_string()
    } else {
        format!("{}", handle as i64)
    };

    if newline != 0 {
        println!("{}", formatted);
    } else {
        print!("{}", formatted);
    }
    let _ = std::io::stdout().flush();
    0
}

/// Print a value by handle to a string and return pointer (caller must free)
#[unsafe(no_mangle)]
pub extern "C" fn jit_value_to_string(handle: u64) -> *const c_char {
    if let Some(value) = get_value(handle) {
        let s = value.as_string();
        let c_string = std::ffi::CString::new(s).unwrap_or_default();
        c_string.into_raw()
    } else {
        std::ptr::null()
    }
}

/// Pretty print a value by handle to a string and return pointer (caller must free)
#[unsafe(no_mangle)]
pub extern "C" fn jit_value_pretty_string(handle: u64, mode: i64) -> *const c_char {
    if let Some(value) = get_value(handle) {
        let s = if mode == 0 {
            value.as_string()
        } else {
            pretty_format(&value, mode, 0)
        };
        let c_string = std::ffi::CString::new(s).unwrap_or_default();
        c_string.into_raw()
    } else {
        std::ptr::null()
    }
}

/// Pretty print a value using the unified pretty_print module
/// handle: RuntimeValue handle
/// mode_str: mode string ("full", "compact", "simple", or "none")  
/// Returns: 0 on success
#[unsafe(no_mangle)]
pub extern "C" fn jit_print_value_pretty(handle: u64, mode_ptr: i64) -> u64 {
    use crate::execution::runtime::pretty_print::{PrettyPrintOptions, pretty_print};
    use crate::parsing::ast::{ArrayElementType, DynamicArray, NativeFn, Value};
    use std::io::Write;
    use std::sync::Arc;

    // Helper to convert RuntimeValue to Value
    fn runtime_value_to_value(v: &RuntimeValue) -> Value {
        match v {
            RuntimeValue::Int(n) => infer_int_value_type(*n),
            RuntimeValue::Float(n) => Value::Number(*n),
            RuntimeValue::Bool(b) => Value::Bool(*b),
            RuntimeValue::Char(c) => Value::Char(*c),
            RuntimeValue::String(s) => Value::Str(s.clone()),
            RuntimeValue::BigInt(bi) => Value::BigInt(bi.clone()),
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
            RuntimeValue::Array(arr) => {
                Value::Array(arr.iter().map(runtime_value_to_value).collect())
            }
            RuntimeValue::Set(set_vals) => {
                Value::Set(set_vals.iter().map(runtime_value_to_value).collect())
            }
            RuntimeValue::Tuple(tup) => {
                Value::Tuple(tup.iter().map(runtime_value_to_value).collect())
            }
            RuntimeValue::Object(obj) => {
                let mut map: rustc_hash::FxHashMap<String, Value> =
                    rustc_hash::FxHashMap::default();
                for (k, v) in obj.iter() {
                    map.insert(k.clone(), runtime_value_to_value(v));
                }
                Value::Object(Arc::new(map))
            }
            RuntimeValue::Promise(id) => Value::Promise(*id),
            RuntimeValue::Function(_) => {
                Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Null))))
            }
            RuntimeValue::RawArray(elem_ty, values) => Value::RawArray(
                elem_ty.clone(),
                values.iter().map(runtime_value_to_value).collect(),
            ),
            RuntimeValue::DynArray {
                data,
                element_type,
                concrete_type,
                tracked_capacity,
            } => {
                let converted: Vec<Value> = data.iter().map(runtime_value_to_value).collect();
                Value::DynArray(Box::new(DynamicArray {
                    data: converted,
                    element_type: ArrayElementType::from_type_name(element_type),
                    concrete_type: concrete_type.clone(),
                    tracked_capacity: tracked_capacity.unwrap_or(data.len()),
                }))
            }
            RuntimeValue::Null => Value::Null,
        }
    }

    let value = match get_value(handle) {
        Some(v) => v,
        None => return 1, // Error
    };

    // Get mode string
    let mode = if mode_ptr == 0 {
        "none"
    } else {
        unsafe {
            let ptr = mode_ptr as *const c_char;
            std::ffi::CStr::from_ptr(ptr).to_str().unwrap_or("none")
        }
    };

    // Determine pretty print options based on mode
    let pretty_opts = match mode {
        "compact" => PrettyPrintOptions::compact(),
        "simple" => PrettyPrintOptions::simple_color(),
        "full" => PrettyPrintOptions::default(),
        _ => return 1, // No pretty printing
    };

    // Convert to Value and pretty print
    let ast_value = runtime_value_to_value(&value);
    let output = pretty_print(&ast_value, &pretty_opts);

    // Print to stdout
    print!("{}", output);
    let _ = std::io::stdout().flush();

    0
}

/// Free a string returned by jit_value_to_string or jit_value_pretty_string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            drop(std::ffi::CString::from_raw(ptr));
        }
    }
}

/// Create a class object (alias for make_object)
#[unsafe(no_mangle)]
pub extern "C" fn jit_make_class_object() -> u64 {
    jit_make_object()
}

/// Set class name
#[unsafe(no_mangle)]
pub extern "C" fn jit_set_class_name(class_handle: u64, name_handle: u64) -> u64 {
    let name = get_string_val(name_handle).unwrap_or_default();

    if let Some(RuntimeValue::Object(mut obj)) = remove_value(class_handle) {
        obj.insert("name".to_string(), RuntimeValue::String(name));
        store_value(RuntimeValue::Object(obj))
    } else {
        0
    }
}

/// Set a field on an object with handle keys/values
#[unsafe(no_mangle)]
pub extern "C" fn jit_set_field(obj_handle: u64, field_handle: u64, val_handle: u64) -> u64 {
    let field = if let Some(s) = get_string_val(field_handle) {
        s
    } else {
        return 0;
    };
    let val = if let Some(v) = get_value(val_handle) {
        v
    } else {
        RuntimeValue::Null
    };

    if let Some(RuntimeValue::Object(mut obj)) = remove_value(obj_handle) {
        obj.insert(field, val);
        store_value(RuntimeValue::Object(obj))
    } else {
        0
    }
}

/// Get a field from an object and return it as a new runtime handle.
#[unsafe(no_mangle)]
pub extern "C" fn jit_get_field(obj_handle: u64, field_handle: u64) -> u64 {
    let field = if let Some(s) = get_string_val(field_handle) {
        s
    } else {
        return store_value(RuntimeValue::Null);
    };

    // First check if obj_handle is a global ARC handle itself (raw u64 ARC ID)
    if let Ok(guard) = crate::execution::runtime_core::arc_bridge::arc_manager().lock() {
        if let Ok(val) = guard.get_value(obj_handle) {
            let convert_val = |v: &crate::parsing::ast::Value| -> RuntimeValue {
                match v {
                    crate::parsing::ast::Value::Number(n) => RuntimeValue::Float(*n),
                    crate::parsing::ast::Value::I64(n) => RuntimeValue::Int(*n),
                    crate::parsing::ast::Value::U64(n) => RuntimeValue::Int(*n as i64),
                    crate::parsing::ast::Value::Str(s) => RuntimeValue::String(s.clone()),
                    crate::parsing::ast::Value::Bool(b) => RuntimeValue::Bool(*b),
                    crate::parsing::ast::Value::Object(m) => {
                        let mut map = crate::utils::collections::FastMap::default();
                        for (k, val_inner) in m.iter() {
                            let v_rv = match val_inner {
                                crate::parsing::ast::Value::Number(n) => RuntimeValue::Float(*n),
                                crate::parsing::ast::Value::I64(n) => RuntimeValue::Int(*n),
                                crate::parsing::ast::Value::U64(n) => RuntimeValue::Int(*n as i64),
                                crate::parsing::ast::Value::Str(s) => {
                                    RuntimeValue::String(s.clone())
                                }
                                crate::parsing::ast::Value::Bool(b) => RuntimeValue::Bool(*b),
                                _ => RuntimeValue::Null,
                            };
                            map.insert(k.clone(), v_rv);
                        }
                        RuntimeValue::Object(map)
                    }
                    _ => RuntimeValue::Null,
                }
            };
            let rv = convert_val(&val);
            match rv {
                RuntimeValue::Object(obj) => {
                    let value = obj.get(&field).cloned().unwrap_or(RuntimeValue::Null);
                    return store_value(value);
                }
                _ => return store_value(RuntimeValue::Null),
            }
        }
    }

    match get_value(obj_handle) {
        Some(RuntimeValue::U64(handle)) => jit_get_field(handle, field_handle),
        Some(RuntimeValue::Object(obj)) => {
            let value = obj.get(&field).cloned().unwrap_or(RuntimeValue::Null);
            store_value(value)
        }
        Some(RuntimeValue::Array(arr)) => {
            let value = match field.as_str() {
                "len" | "length" => RuntimeValue::Int(arr.len() as i64),
                "capacity" => RuntimeValue::Int(arr.capacity() as i64),
                "metadata_size" => RuntimeValue::Int(24),
                _ => RuntimeValue::Null,
            };
            store_value(value)
        }
        Some(RuntimeValue::DynArray {
            data,
            element_type,
            tracked_capacity,
            ..
        }) => {
            let value = match field.as_str() {
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
            store_value(value)
        }
        Some(RuntimeValue::RawArray(_, arr)) => {
            let value = match field.as_str() {
                "len" | "length" => RuntimeValue::Int(arr.len() as i64),
                "capacity" => RuntimeValue::Int(arr.len() as i64),
                "metadata_size" => RuntimeValue::Int(0),
                _ => RuntimeValue::Null,
            };
            store_value(value)
        }
        Some(RuntimeValue::Tuple(tup)) => {
            let value = match field.as_str() {
                "len" | "length" => RuntimeValue::Int(tup.len() as i64),
                "capacity" => RuntimeValue::Int(tup.len() as i64),
                "metadata_size" => RuntimeValue::Int(0),
                _ => RuntimeValue::Null,
            };
            store_value(value)
        }
        Some(RuntimeValue::String(s)) => {
            let value = match field.as_str() {
                "len" | "length" => RuntimeValue::Int(s.len() as i64),
                _ => RuntimeValue::Null,
            };
            store_value(value)
        }
        _ => store_value(RuntimeValue::Null),
    }
}

/// Check whether an object contains a field.
#[unsafe(no_mangle)]
pub extern "C" fn jit_object_has_key(obj_handle: u64, field_handle: u64) -> i64 {
    let field = if let Some(s) = get_string_val(field_handle) {
        s
    } else {
        return 0;
    };

    match get_value(obj_handle) {
        Some(RuntimeValue::Object(obj)) => {
            if obj.contains_key(&field) {
                1
            } else {
                0
            }
        }
        _ => 0,
    }
}

/// Create a new instance of a class
#[unsafe(no_mangle)]
pub extern "C" fn jit_new_class(class_handle: u64) -> u64 {
    let class_val = if let Some(v) = get_value(class_handle) {
        v
    } else {
        return 0;
    };

    // Create new object with __class__ pointing to class_val
    let mut map = FastMap::default();
    map.insert("__class__".to_string(), class_val);
    store_value(RuntimeValue::Object(map))
}

/// Look up a method address for an object
#[unsafe(no_mangle)]
pub extern "C" fn jit_get_method(obj_handle: u64, method_name_handle: u64) -> u64 {
    let method_name = if let Some(s) = get_string_val(method_name_handle) {
        s
    } else {
        return 0;
    };

    let obj_val = if let Some(v) = get_value(obj_handle) {
        v
    } else {
        return 0;
    };

    // Helper to look up in object or chain
    fn find_method_name(val: &RuntimeValue, name: &str) -> Option<String> {
        if let RuntimeValue::Object(map) = val {
            if let Some(field) = map.get(name) {
                if let RuntimeValue::String(mangled) = field {
                    return Some(mangled.clone());
                }
            }
            // Check prototype/__class__
            if let Some(proto) = map.get("__class__") {
                return find_method_name(proto, name);
            }
        }
        None
    }

    if let Some(mangled) = find_method_name(&obj_val, &method_name) {
        // Look up address
        JIT_FUNCTIONS.with(|funcs| *funcs.borrow().get(&mangled).unwrap_or(&0) as u64)
    } else {
        0
    }
}

// ============================================================================
// Command-line argument functions
// ============================================================================

/// jit_argc() - Returns the number of command-line arguments
#[unsafe(no_mangle)]
pub extern "C" fn jit_argc() -> i64 {
    let args = crate::execution::runtime::get_program_args();
    args.len() as i64
}

/// jit_argv() - Returns all command-line arguments as a handle to an array
#[unsafe(no_mangle)]
pub extern "C" fn jit_argv() -> u64 {
    let args = crate::execution::runtime::get_program_args();
    let arr: Vec<RuntimeValue> = args.into_iter().map(RuntimeValue::String).collect();
    store_value(RuntimeValue::Array(arr))
}

/// jit_arg(index) - Returns a specific argument by index as a string handle
#[unsafe(no_mangle)]
pub extern "C" fn jit_arg(index: i64) -> u64 {
    let program_args = crate::execution::runtime::get_program_args();
    if index >= 0 && (index as usize) < program_args.len() {
        let s = program_args[index as usize].clone();
        store_value(RuntimeValue::String(s))
    } else {
        store_value(RuntimeValue::Null)
    }
}

/// jit_exec_name() - Returns the executable/script name as a string handle
#[unsafe(no_mangle)]
pub extern "C" fn jit_exec_name() -> u64 {
    let name = crate::execution::runtime::get_program_name();
    store_value(RuntimeValue::String(name))
}

/// jit_args_count() - Returns the number of arguments (alias for jit_argc)
#[unsafe(no_mangle)]
pub extern "C" fn jit_args_count() -> i64 {
    jit_argc()
}

// ============================================================================
// Initialization
// ============================================================================

// Helper for reading .env file
fn read_dotenv_file(path: &str) -> Option<FastMap<String, String>> {
    let s = std::fs::read_to_string(path).ok()?;
    let mut out: FastMap<String, String> = FastMap::default();

    for raw_line in s.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = if line.starts_with("export ") {
            &line[7..]
        } else {
            line
        };
        if let Some(eq) = line.find('=') {
            let key = line[..eq].trim();
            if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                continue;
            }
            let mut val = line[eq + 1..].trim().to_string();
            // Handle quotes
            if val.starts_with('"') && val.ends_with('"') && val.len() >= 2 {
                val = val[1..val.len() - 1].to_string();
                val = val
                    .replace("\\n", "\n")
                    .replace("\\t", "\t")
                    .replace("\\r", "\r");
            } else if val.starts_with('\'') && val.ends_with('\'') && val.len() >= 2 {
                val = val[1..val.len() - 1].to_string();
            } else if let Some(hash) = val.find('#') {
                let before = &val[..hash];
                if before.ends_with(' ') {
                    val = before.trim_end().to_string();
                }
            }
            out.insert(key.to_string(), val);
        }
    }
    Some(out)
}

// ============================================================================
// Environment variable functions
// ============================================================================

/// jit_env(name) - Get environment variable
#[unsafe(no_mangle)]
pub extern "C" fn jit_env(name_handle: u64) -> u64 {
    let name = if let Some(s) = get_string_val(name_handle) {
        s
    } else {
        return 0; // Null
    };

    // Check runtime env first
    if let Some(v) = crate::execution::runtime::runtime_env_get(&name) {
        return store_value(RuntimeValue::String(v));
    }

    // Then check system environment
    match std::env::var(&name) {
        Ok(v) => store_value(RuntimeValue::String(v)),
        Err(_) => 0, // Null
    }
}

/// jit_env_has(name) - Check if environment variable exists
#[unsafe(no_mangle)]
pub extern "C" fn jit_env_has(name_handle: u64) -> i64 {
    let name = if let Some(s) = get_string_val(name_handle) {
        s
    } else {
        return 0;
    };

    if crate::execution::runtime::runtime_env_has(&name) || std::env::var(&name).is_ok() {
        1
    } else {
        0
    }
}

/// jit_env_all() - Get all environment variables as object
#[unsafe(no_mangle)]
pub extern "C" fn jit_env_all() -> u64 {
    let mut m: FastMap<String, RuntimeValue> = FastMap::default();

    // Runtime env first
    for (k, v) in crate::execution::runtime::runtime_env_all() {
        m.insert(k, RuntimeValue::String(v));
    }

    // System env (don't overwrite runtime env)
    for (k, v) in std::env::vars() {
        if !m.contains_key(&k) {
            m.insert(k, RuntimeValue::String(v));
        }
    }

    store_value(RuntimeValue::Object(m))
}

/// jit_env_from_file(path) - Read .env file into object
#[unsafe(no_mangle)]
pub extern "C" fn jit_env_from_file(path_handle: u64) -> u64 {
    let path = get_string_val(path_handle).unwrap_or_else(|| ".env".to_string());

    if let Some(map) = read_dotenv_file(&path) {
        let mut obj: FastMap<String, RuntimeValue> = FastMap::default();
        for (k, v) in map {
            obj.insert(k, RuntimeValue::String(v));
        }
        store_value(RuntimeValue::Object(obj))
    } else {
        0 // Null
    }
}

/// jit_env_runtime_get(name) - Get runtime environment variable
#[unsafe(no_mangle)]
pub extern "C" fn jit_env_runtime_get(name_handle: u64) -> u64 {
    let name = if let Some(s) = get_string_val(name_handle) {
        s
    } else {
        return 0;
    };

    if let Some(v) = crate::execution::runtime::runtime_env_get(&name) {
        store_value(RuntimeValue::String(v))
    } else {
        0 // Null
    }
}

/// jit_env_runtime_has(name) - Check runtime environment variable presence
#[unsafe(no_mangle)]
pub extern "C" fn jit_env_runtime_has(name_handle: u64) -> i64 {
    let name = if let Some(s) = get_string_val(name_handle) {
        s
    } else {
        return 0;
    };

    if crate::execution::runtime::runtime_env_has(&name) {
        1
    } else {
        0
    }
}

/// jit_env_runtime_all() - Get all runtime environment variables
#[unsafe(no_mangle)]
pub extern "C" fn jit_env_runtime_all() -> u64 {
    let mut m: FastMap<String, RuntimeValue> = FastMap::default();
    for (k, v) in crate::execution::runtime::runtime_env_all() {
        m.insert(k, RuntimeValue::String(v));
    }
    store_value(RuntimeValue::Object(m))
}

/// jit_env_runtime_load(handle, overwrite) - Load runtime env from object or file
#[unsafe(no_mangle)]
pub extern "C" fn jit_env_runtime_load(handle: u64, overwrite: i64) -> i64 {
    let overwrite = overwrite != 0;
    let mut map = rustc_hash::FxHashMap::default();

    // Check if it's a string (path) - could be pointer or handle to string
    if let Some(path) = get_string_val(handle) {
        if let Some(file_map) = read_dotenv_file(&path) {
            for (k, v) in file_map {
                map.insert(k, v);
            }
        }
    } else if let Some(RuntimeValue::Object(obj)) = get_value(handle) {
        // It's an object handle
        for (k, v) in obj {
            if let RuntimeValue::String(s) = v {
                map.insert(k, s);
            }
        }
    } else {
        return 0;
    }

    crate::execution::runtime::runtime_env_load(map, overwrite) as i64
}

// ============================================================================
// Extended Argument Parsing
// ============================================================================

/// jit_parse_args() - Parse arguments into flags and positionals
#[unsafe(no_mangle)]
pub extern "C" fn jit_parse_args() -> u64 {
    let src = crate::execution::runtime::get_program_args();
    let mut flags: FastMap<String, RuntimeValue> = FastMap::default();
    let mut positionals: Vec<RuntimeValue> = Vec::new();
    let mut i = 0usize;

    while i < src.len() {
        let s = &src[i];
        if s.starts_with("--") {
            let rest = &s[2..];
            if let Some(eq) = rest.find('=') {
                flags.insert(
                    rest[..eq].to_string(),
                    RuntimeValue::String(rest[eq + 1..].to_string()),
                );
            } else if i + 1 < src.len() && !src[i + 1].starts_with('-') {
                flags.insert(rest.to_string(), RuntimeValue::String(src[i + 1].clone()));
                i += 1;
            } else {
                flags.insert(rest.to_string(), RuntimeValue::Bool(true));
            }
        } else if s.starts_with('-') && s.len() > 1 {
            let rest = &s[1..];
            if rest.len() > 1 && rest.chars().all(|c: char| c.is_ascii_alphabetic()) {
                for ch in rest.chars() {
                    let key: String = ch.to_string();
                    flags.insert(key, RuntimeValue::Bool(true));
                }
            } else {
                let ch: char = rest.chars().next().unwrap();
                let tail: String = rest.chars().skip(1).collect();
                if !tail.is_empty() {
                    let key: String = ch.to_string();
                    flags.insert(key, RuntimeValue::String(tail));
                } else if i + 1 < src.len() && !src[i + 1].starts_with('-') {
                    let key: String = ch.to_string();
                    flags.insert(key, RuntimeValue::String(src[i + 1].clone()));
                    i += 1;
                } else {
                    let key: String = ch.to_string();
                    flags.insert(key, RuntimeValue::Bool(true));
                }
            }
        } else {
            positionals.push(RuntimeValue::String(s.clone()));
        }
        i += 1;
    }

    let mut out: FastMap<String, RuntimeValue> = FastMap::default();
    out.insert("flags".into(), RuntimeValue::Object(flags));
    out.insert("positionals".into(), RuntimeValue::Array(positionals));
    store_value(RuntimeValue::Object(out))
}

/// jit_arg_get(name_handle)
#[unsafe(no_mangle)]
pub extern "C" fn jit_arg_get(name_handle: u64) -> u64 {
    let name = if let Some(s) = get_string_val(name_handle) {
        s
    } else {
        return 0;
    };

    let src = crate::execution::runtime::get_program_args();
    let mut flags: FastMap<String, RuntimeValue> = FastMap::default();
    let mut i = 0usize;

    while i < src.len() {
        let s = &src[i];
        if s.starts_with("--") {
            let rest = &s[2..];
            if let Some(eq) = rest.find('=') {
                flags.insert(
                    rest[..eq].to_string(),
                    RuntimeValue::String(rest[eq + 1..].to_string()),
                );
            } else if i + 1 < src.len() && !src[i + 1].starts_with('-') {
                flags.insert(rest.to_string(), RuntimeValue::String(src[i + 1].clone()));
                i += 1;
            } else {
                flags.insert(rest.to_string(), RuntimeValue::Bool(true));
            }
        } else if s.starts_with('-') && s.len() > 1 {
            let rest = &s[1..];
            if rest.len() > 1 && rest.chars().all(|c: char| c.is_ascii_alphabetic()) {
                for ch in rest.chars() {
                    let key: String = ch.to_string();
                    flags.insert(key, RuntimeValue::Bool(true));
                }
            } else {
                let ch: char = rest.chars().next().unwrap();
                let tail: String = rest.chars().skip(1).collect();
                if !tail.is_empty() {
                    let key: String = ch.to_string();
                    flags.insert(key, RuntimeValue::String(tail));
                } else if i + 1 < src.len() && !src[i + 1].starts_with('-') {
                    let key: String = ch.to_string();
                    flags.insert(key, RuntimeValue::String(src[i + 1].clone()));
                    i += 1;
                } else {
                    let key: String = ch.to_string();
                    flags.insert(key, RuntimeValue::Bool(true));
                }
            }
        }
        i += 1;
    }

    if let Some(v) = flags.get(&name) {
        store_value(v.clone())
    } else {
        0
    }
}

/// jit_arg_has(name_handle)
#[unsafe(no_mangle)]
pub extern "C" fn jit_arg_has(name_handle: u64) -> i64 {
    let name = if let Some(s) = get_string_val(name_handle) {
        s
    } else {
        return 0;
    };
    let name_ref: &str = name.as_str();

    let src = crate::execution::runtime::get_program_args();
    for s in src.iter() {
        if s == &format!("--{}", name_ref)
            || s.starts_with(&format!("--{}=", name_ref))
            || s == &format!("-{}", name_ref)
        {
            return 1;
        }
        // Handle grouped short flags like -abc
        if s.starts_with('-') && !s.starts_with("--") && s.len() > 2 {
            if name.len() == 1 && s[1..].contains(name_ref) {
                return 1;
            }
        }
    }
    0
}

/// jit_args_slice(start)
#[unsafe(no_mangle)]
pub extern "C" fn jit_args_slice(start: i64) -> u64 {
    let start = start.max(0) as usize;
    let args = crate::execution::runtime::get_program_args();
    let arr: Vec<RuntimeValue> = args
        .into_iter()
        .skip(start)
        .map(RuntimeValue::String)
        .collect();
    store_value(RuntimeValue::Array(arr))
}

/// jit_args_join(sep_handle)
#[unsafe(no_mangle)]
pub extern "C" fn jit_args_join(sep_handle: u64) -> u64 {
    let sep = get_string_val(sep_handle).unwrap_or_else(|| " ".to_string());
    let args = crate::execution::runtime::get_program_args();
    let s = args.join(&sep);
    store_value(RuntimeValue::String(s))
}

/// jit_args_index_of(val_handle)
#[unsafe(no_mangle)]
pub extern "C" fn jit_args_index_of(val_handle: u64) -> i64 {
    let val = if let Some(s) = get_string_val(val_handle) {
        s
    } else {
        return -1;
    };
    let args = crate::execution::runtime::get_program_args();
    for (i, s) in args.iter().enumerate() {
        if s == &val {
            return i as i64;
        }
    }
    -1
}

/// jit_str_concat_typed(a, a_type, b, b_type) - Concatenate two values as strings with type info
/// Type tags: 0=int, 1=float, 2=string/ptr, 3=bool, 4=handle
#[unsafe(no_mangle)]
pub extern "C" fn jit_str_concat_typed(a: u64, a_type: i64, b: u64, b_type: i64) -> u64 {
    let str_a = get_string_val_typed(a, a_type);
    let str_b = get_string_val_typed(b, b_type);

    // Concatenate and store result
    let mut result = String::with_capacity(str_a.len() + str_b.len());
    result.push_str(&str_a);
    result.push_str(&str_b);

    store_value(RuntimeValue::String(result))
}

/// Helper to get string value based on type tag
fn get_string_val_typed(val: u64, type_tag: i64) -> String {
    match type_tag {
        1 => {
            // Float
            // Floats are passed as u64 bits
            let f = f64::from_bits(val);
            format!("{}", f)
        }
        2 => {
            // String/Ptr
            if let Some(s) = get_string_val(val) {
                s
            } else {
                "".to_string()
            }
        }
        3 => {
            // Bool
            if val != 0 {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        4 => {
            // Handle
            if let Some(v) = get_value(val) {
                value_to_simple_string(&v)
            } else {
                // If handle not found, might be a bug or stale handle, show as such
                format!("<handle:{}>", val)
            }
        }
        _ => {
            // Int (0) or default
            format!("{}", val as i64)
        }
    }
}

/// jit_format_value(value, spec) - Format a value with a format specifier (for template literals with format specs)
#[unsafe(no_mangle)]
pub extern "C" fn jit_format_value(value: u64, spec: u64) -> u64 {
    use crate::execution::runtime_core::format::apply_format_spec;
    use crate::parsing::ast::Value;

    // Get the format spec string
    let spec_str = if let Some(s) = get_string_val(spec) {
        s
    } else {
        String::new()
    };

    // Convert the value to a Value enum
    let val = if let Some(rv) = get_value(value) {
        runtime_value_to_ast_value(&rv)
    } else if let Some(s) = get_string_val(value) {
        Value::Str(s)
    } else {
        Value::Number(value as f64)
    };

    // Apply format spec
    let formatted = apply_format_spec(&val, &spec_str);
    store_value(RuntimeValue::String(formatted))
}

/// Helper to convert RuntimeValue to AST Value for formatting
fn runtime_value_to_ast_value(rv: &RuntimeValue) -> crate::parsing::ast::Value {
    use crate::parsing::ast::Value;
    use crate::utils::collections::FastMap;

    match rv {
        RuntimeValue::Int(i) => infer_int_value_type(*i),
        RuntimeValue::Float(f) => Value::Number(*f),
        RuntimeValue::String(s) => Value::Str(s.clone()),
        RuntimeValue::Bool(b) => Value::Bool(*b),
        RuntimeValue::Char(c) => Value::Char(*c),
        RuntimeValue::Null => Value::Null,
        RuntimeValue::Array(arr) => {
            let values: Vec<Value> = arr.iter().map(runtime_value_to_ast_value).collect();
            Value::Array(values)
        }
        RuntimeValue::Set(set_vals) => {
            let values: Vec<Value> = set_vals.iter().map(runtime_value_to_ast_value).collect();
            Value::Set(values)
        }
        RuntimeValue::Object(obj) => {
            let mut map = FastMap::default();
            for (k, v) in obj.iter() {
                map.insert(k.clone(), runtime_value_to_ast_value(v));
            }
            Value::Object(std::sync::Arc::new(map))
        }
        RuntimeValue::Tuple(tuple) => {
            let values: Vec<Value> = tuple.iter().map(runtime_value_to_ast_value).collect();
            Value::Tuple(values)
        }
        // For other types, convert to string representation
        RuntimeValue::BigInt(bi) => Value::Str(bi.to_string()),
        RuntimeValue::Promise(id) => Value::Str(format!("Promise({})", id)),
        RuntimeValue::Function(f) => Value::Str(format!("Function({})", f.name)),
        RuntimeValue::RawArray(_, values) | RuntimeValue::DynArray { data: values, .. } => {
            let converted: Vec<Value> = values.iter().map(runtime_value_to_ast_value).collect();
            Value::Array(converted)
        }
        // Fixed-width types
        RuntimeValue::U8(n) => Value::Number(*n as f64),
        RuntimeValue::U16(n) => Value::Number(*n as f64),
        RuntimeValue::U32(n) => Value::Number(*n as f64),
        RuntimeValue::U64(n) => Value::Number(*n as f64),
        RuntimeValue::U128(n) => Value::Number(*n as f64),
        RuntimeValue::I8(n) => Value::Number(*n as f64),
        RuntimeValue::I16(n) => Value::Number(*n as f64),
        RuntimeValue::I32(n) => Value::Number(*n as f64),
        RuntimeValue::I64(n) => Value::Number(*n as f64),
        RuntimeValue::I128(n) => Value::Number(*n as f64),
        RuntimeValue::F32(n) => Value::Number(*n as f64),
        RuntimeValue::F64(n) => Value::Number(*n),
    }
}

/// Helper to convert RuntimeValue to simple string representation
fn value_to_simple_string(rv: &RuntimeValue) -> String {
    match rv {
        RuntimeValue::Int(i) => format!("{}", i),
        RuntimeValue::Float(f) => format!("{}", f),
        RuntimeValue::Char(c) => format!("{}", c),
        RuntimeValue::String(s) => format!("\"{}\"", s),
        RuntimeValue::Bool(b) => format!("{}", b),
        RuntimeValue::Null => "null".to_string(),
        RuntimeValue::Array(_) => "[...]".to_string(),
        RuntimeValue::Set(_) => "{...}".to_string(),
        RuntimeValue::Object(_) => "{...}".to_string(),
        RuntimeValue::Tuple(_) => "(...".to_string(),
        RuntimeValue::BigInt(bi) => bi.to_string(),
        RuntimeValue::Promise(id) => format!("Promise({})", id),
        RuntimeValue::Function(f) => format!("Function({})", f.name),
        RuntimeValue::RawArray(_, _) | RuntimeValue::DynArray { .. } => "[...]".to_string(),
        // Fixed-width types
        RuntimeValue::U8(n) => format!("{}", n),
        RuntimeValue::U16(n) => format!("{}", n),
        RuntimeValue::U32(n) => format!("{}", n),
        RuntimeValue::U64(n) => format!("{}", n),
        RuntimeValue::U128(n) => format!("{}", n),
        RuntimeValue::I8(n) => format!("{}", n),
        RuntimeValue::I16(n) => format!("{}", n),
        RuntimeValue::I32(n) => format!("{}", n),
        RuntimeValue::I64(n) => format!("{}", n),
        RuntimeValue::I128(n) => format!("{}", n),
        RuntimeValue::F32(n) => format!("{}", n),
        RuntimeValue::F64(n) => format!("{}", n),
    }
}

/// jit_str_eq(a, b) - Check string equality (handles vs pointers)
#[unsafe(no_mangle)]
pub extern "C" fn jit_str_eq(a: u64, b: u64) -> i64 {
    let sa = get_string_val(a);
    let sb = get_string_val(b);
    match (sa, sb) {
        (Some(sa), Some(sb)) => {
            if sa == sb {
                1
            } else {
                0
            }
        }
        (None, None) => {
            if a == b {
                1
            } else {
                0
            }
        }
        _ => {
            if a == b {
                1
            } else {
                0
            }
        }
    }
}

/// JIT C-ABI wrapper for input()
#[unsafe(no_mangle)]
pub extern "C" fn jit_runtime_input(prompt_handle_or_ptr: u64, opts_handle: u64) -> u64 {
    let prompt = get_string_val(prompt_handle_or_ptr).unwrap_or_default();
    let opts = get_value(opts_handle);
    let mut args = vec![RuntimeValue::String(prompt)];
    if let Some(opt_val) = opts {
        args.push(opt_val);
    }
    let registry = crate::backends::common::builtins::BuiltinRegistry::new();
    let res = if let Some(func) = registry.get("input") {
        func(&args)
    } else {
        RuntimeValue::Null
    };
    store_value(res)
}

/// JIT C-ABI wrapper for generic typed input<T>()
#[unsafe(no_mangle)]
pub extern "C" fn jit_runtime_input_generic(
    prompt_handle_or_ptr: u64,
    opts_handle: u64,
    type_name_ptr: *const c_char,
) -> u64 {
    let prompt = get_string_val(prompt_handle_or_ptr).unwrap_or_default();
    let opts = get_value(opts_handle);
    let mut args = vec![RuntimeValue::String(prompt)];
    if let Some(opt_val) = opts {
        args.push(opt_val);
    }
    if !type_name_ptr.is_null() {
        if let Ok(type_name) = unsafe { CStr::from_ptr(type_name_ptr).to_str() } {
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
    store_value(res)
}

fn ast_value_to_runtime_value(v: &crate::parsing::ast::Value) -> RuntimeValue {
    use crate::parsing::ast::Value;
    match v {
        Value::Number(n) => RuntimeValue::Float(*n),
        Value::I64(n) => RuntimeValue::Int(*n),
        Value::U64(n) => RuntimeValue::U64(*n),
        Value::Bool(b) => RuntimeValue::Bool(*b),
        Value::Char(c) => RuntimeValue::Char(*c),
        Value::Str(s) => RuntimeValue::String(s.clone()),
        Value::Object(m) => {
            let mut map: FastMap<String, RuntimeValue> = FastMap::default();
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

fn unpack_jit_arg(raw: u64) -> RuntimeValue {
    if let Some(val) = get_value(raw) {
        val
    } else if let Some(s) = get_string_val(raw) {
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

/// JIT C-ABI wrapper for input.* methods
#[unsafe(no_mangle)]
pub extern "C" fn jit_runtime_call_input_builtin(
    method_name_ptr: *const c_char,
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
    a4: u64,
    argc: u64,
) -> u64 {
    if method_name_ptr.is_null() {
        return store_value(RuntimeValue::Null);
    }
    let method_name = match unsafe { CStr::from_ptr(method_name_ptr).to_str() } {
        Ok(s) => s,
        Err(_) => return store_value(RuntimeValue::Null),
    };

    let raw_args = [a0, a1, a2, a3, a4];
    let count = (argc as usize).min(5);
    let mut args = Vec::with_capacity(count);
    for i in 0..count {
        args.push(unpack_jit_arg(raw_args[i]));
    }

    let registry = crate::backends::common::builtins::BuiltinRegistry::new();
    let res = if let Some(func) = registry.get(method_name) {
        func(&args)
    } else {
        let prefixed = if method_name.starts_with("input.") {
            method_name.to_string()
        } else {
            format!("input.{}", method_name)
        };
        if let Some(func) = registry.get(&prefixed) {
            func(&args)
        } else {
            RuntimeValue::Null
        }
    };
    store_value(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_runtime_input_method(
    method_name_ptr: *const c_char,
    arg0: u64,
    arg1: u64,
    arg2: u64,
) -> u64 {
    jit_runtime_call_input_builtin(method_name_ptr, arg0, arg1, arg2, 0, 0, 3)
}

/// Get all runtime bridge function pointers for registration with JIT
pub fn get_runtime_symbols() -> Vec<(&'static str, *const u8)> {
    vec![
        ("jit_runtime_input", jit_runtime_input as *const u8),
        (
            "jit_runtime_input_generic",
            jit_runtime_input_generic as *const u8,
        ),
        (
            "jit_runtime_input_method",
            jit_runtime_input_method as *const u8,
        ),
        (
            "jit_runtime_call_input_builtin",
            jit_runtime_call_input_builtin as *const u8,
        ),
        ("jit_clock", jit_clock as *const u8),
        ("jit_sizeof_handle", jit_sizeof_handle as *const u8),
        ("jit_wrap_i64", jit_wrap_i64 as *const u8),
        ("jit_wrap_i8", jit_wrap_i8 as *const u8),
        ("jit_wrap_i16", jit_wrap_i16 as *const u8),
        ("jit_wrap_i32", jit_wrap_i32 as *const u8),
        ("jit_wrap_u8", jit_wrap_u8 as *const u8),
        ("jit_wrap_u16", jit_wrap_u16 as *const u8),
        ("jit_wrap_u32", jit_wrap_u32 as *const u8),
        ("jit_wrap_u64", jit_wrap_u64 as *const u8),
        ("jit_wrap_f32", jit_wrap_f32 as *const u8),
        ("jit_wrap_f64", jit_wrap_f64 as *const u8),
        ("jit_wrap_bool", jit_wrap_bool as *const u8),
        ("jit_wrap_null", jit_wrap_null as *const u8),
        ("jit_wrap_char", jit_wrap_char as *const u8),
        ("jit_wrap_str", jit_wrap_str as *const u8),
        ("jit_char_from_str", jit_char_from_str as *const u8),
        ("jit_make_array", jit_make_array as *const u8),
        (
            "jit_make_array_capacity",
            jit_make_array_capacity as *const u8,
        ),
        ("jit_array_push_int", jit_array_push_int as *const u8),
        ("jit_array_push_float", jit_array_push_float as *const u8),
        ("jit_array_push_str", jit_array_push_str as *const u8),
        ("jit_array_push_bool", jit_array_push_bool as *const u8),
        ("jit_array_push_null", jit_array_push_null as *const u8),
        ("jit_array_push_handle", jit_array_push_handle as *const u8),
        (
            "jit_make_set_from_array",
            jit_make_set_from_array as *const u8,
        ),
        ("jit_make_object", jit_make_object as *const u8),
        ("jit_object_set_str", jit_object_set_str as *const u8),
        ("jit_object_set_int", jit_object_set_int as *const u8),
        ("jit_object_set_float", jit_object_set_float as *const u8),
        ("jit_object_set_bool", jit_object_set_bool as *const u8),
        ("jit_object_set_handle", jit_object_set_handle as *const u8),
        ("jit_object_has_key", jit_object_has_key as *const u8),
        ("jit_make_tuple", jit_make_tuple as *const u8),
        ("jit_tuple_push_int", jit_tuple_push_int as *const u8),
        ("jit_tuple_push_float", jit_tuple_push_float as *const u8),
        ("jit_tuple_push_str", jit_tuple_push_str as *const u8),
        ("jit_tuple_push_bool", jit_tuple_push_bool as *const u8),
        ("jit_tuple_push_null", jit_tuple_push_null as *const u8),
        ("jit_tuple_push_handle", jit_tuple_push_handle as *const u8),
        ("jit_free_handle", jit_free_handle as *const u8),
        ("jit_print_value", jit_print_value as *const u8),
        (
            "jit_print_with_options",
            jit_print_with_options as *const u8,
        ),
        (
            "jit_print_values_with_options",
            jit_print_values_with_options as *const u8,
        ),
        ("jit_print_i64", jit_print_i64 as *const u8),
        ("jit_print_u64", jit_print_u64 as *const u8),
        ("jit_print_bool", jit_print_bool as *const u8),
        ("jit_print_f64", jit_print_f64 as *const u8),
        ("jit_print_str_raw", jit_print_str_raw as *const u8),
        ("jit_print_object", jit_print_object as *const u8),
        ("jit_print_newline", jit_print_newline as *const u8),
        ("jit_print_space", jit_print_space as *const u8),
        (
            "jit_print_value_pretty",
            jit_print_value_pretty as *const u8,
        ),
        ("jit_value_to_string", jit_value_to_string as *const u8),
        (
            "jit_value_pretty_string",
            jit_value_pretty_string as *const u8,
        ),
        ("jit_free_string", jit_free_string as *const u8),
        ("jit_make_class_object", jit_make_class_object as *const u8),
        ("jit_set_class_name", jit_set_class_name as *const u8),
        ("jit_set_field", jit_set_field as *const u8),
        ("jit_get_field", jit_get_field as *const u8),
        ("jit_new_class", jit_new_class as *const u8),
        ("jit_get_method", jit_get_method as *const u8),
        ("jit_parse_args", jit_parse_args as *const u8),
        ("jit_arg_get", jit_arg_get as *const u8),
        ("jit_arg_has", jit_arg_has as *const u8),
        ("jit_args_slice", jit_args_slice as *const u8),
        ("jit_args_join", jit_args_join as *const u8),
        ("jit_args_index_of", jit_args_index_of as *const u8),
        ("jit_str_eq", jit_str_eq as *const u8),
        ("jit_str_concat_typed", jit_str_concat_typed as *const u8),
        ("jit_format_value", jit_format_value as *const u8),
        // Command-line argument functions
        ("jit_argc", jit_argc as *const u8),
        ("jit_argv", jit_argv as *const u8),
        ("jit_arg", jit_arg as *const u8),
        ("jit_exec_name", jit_exec_name as *const u8),
        ("jit_args_count", jit_args_count as *const u8),
        // Environment variable functions
        ("jit_env", jit_env as *const u8),
        ("jit_env_has", jit_env_has as *const u8),
        ("jit_env_all", jit_env_all as *const u8),
        ("jit_env_from_file", jit_env_from_file as *const u8),
        ("jit_env_runtime_get", jit_env_runtime_get as *const u8),
        ("jit_env_runtime_has", jit_env_runtime_has as *const u8),
        ("jit_env_runtime_all", jit_env_runtime_all as *const u8),
        ("jit_env_runtime_load", jit_env_runtime_load as *const u8),
    ]
}
