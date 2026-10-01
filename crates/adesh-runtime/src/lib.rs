//! Standalone Native Runtime for Adesh Programming Language
//!
//! Provides self-contained C ABI exports for AOT compiled binaries.
//! Includes rich pretty printing, composite structures (objects, arrays, tuples, sets),
//! ARC reference counting, tracked allocations, and standard builtins.

pub mod abi;
pub mod allocator;
pub mod threading;

pub use abi::*;
pub use allocator::*;
pub use threading::*;

use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::fmt::Write as FmtWrite;
use std::io::Write;
use std::os::raw::c_char;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

// ============================================================================
// Runtime Value Definition
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Char(char),
    String(String),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    F32(f32),
    F64(f64),
    Array(Vec<RuntimeValue>),
    Tuple(Vec<RuntimeValue>),
    Set(Vec<RuntimeValue>),
    Object(BTreeMap<String, RuntimeValue>),
    Ref(Box<RuntimeValue>),
}

impl RuntimeValue {
    pub fn as_string(&self) -> String {
        match self {
            RuntimeValue::Null => "null".to_string(),
            RuntimeValue::Bool(b) => b.to_string(),
            RuntimeValue::Int(i) => i.to_string(),
            RuntimeValue::Float(f) => {
                if f.fract() == 0.0 {
                    format!("{:.1}", f)
                } else {
                    f.to_string()
                }
            }
            RuntimeValue::Char(c) => c.to_string(),
            RuntimeValue::String(s) => s.clone(),
            RuntimeValue::U8(n) => n.to_string(),
            RuntimeValue::U16(n) => n.to_string(),
            RuntimeValue::U32(n) => n.to_string(),
            RuntimeValue::U64(n) => n.to_string(),
            RuntimeValue::U128(n) => n.to_string(),
            RuntimeValue::I8(n) => n.to_string(),
            RuntimeValue::I16(n) => n.to_string(),
            RuntimeValue::I32(n) => n.to_string(),
            RuntimeValue::I64(n) => n.to_string(),
            RuntimeValue::I128(n) => n.to_string(),
            RuntimeValue::F32(n) => n.to_string(),
            RuntimeValue::F64(n) => n.to_string(),
            RuntimeValue::Array(arr) => {
                let items: Vec<String> = arr.iter().map(|v| v.as_string()).collect();
                format!("[{}]", items.join(", "))
            }
            RuntimeValue::Tuple(tup) => {
                let items: Vec<String> = tup.iter().map(|v| v.as_string()).collect();
                format!("({})", items.join(", "))
            }
            RuntimeValue::Set(set) => {
                let items: Vec<String> = set.iter().map(|v| v.as_string()).collect();
                format!("{{{}}}", items.join(", "))
            }
            RuntimeValue::Object(obj) => {
                let items: Vec<String> = obj
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v.as_string()))
                    .collect();
                format!("{{{}}}", items.join(", "))
            }
            RuntimeValue::Ref(inner) => inner.as_string(),
        }
    }
}

// ============================================================================
// Handle Storage Management
// ============================================================================

static VALUE_STORE: Mutex<Option<BTreeMap<u64, RuntimeValue>>> = Mutex::new(None);
static HANDLE_COUNTER: AtomicU64 = AtomicU64::new(100);

fn get_store_guard() -> std::sync::MutexGuard<'static, Option<BTreeMap<u64, RuntimeValue>>> {
    let mut guard = VALUE_STORE.lock().unwrap();
    if guard.is_none() {
        *guard = Some(BTreeMap::new());
    }
    guard
}

pub fn aot_store_value(value: RuntimeValue) -> u64 {
    let handle = HANDLE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut guard = get_store_guard();
    if let Some(ref mut map) = *guard {
        map.insert(handle, value);
    }
    handle
}

pub fn aot_get_value(handle: u64) -> Option<RuntimeValue> {
    let guard = get_store_guard();
    guard.as_ref().and_then(|map| map.get(&handle).cloned())
}

pub fn aot_remove_value(handle: u64) -> Option<RuntimeValue> {
    let mut guard = get_store_guard();
    guard.as_mut().and_then(|map| map.remove(&handle))
}

pub fn get_string_val(ptr_or_handle: u64) -> Option<String> {
    if ptr_or_handle == 0 {
        return None;
    }
    if let Some(v) = aot_get_value(ptr_or_handle) {
        return match v {
            RuntimeValue::String(s) => Some(s),
            _ => None,
        };
    }
    if ptr_or_handle > 0x10000 {
        unsafe {
            if let Ok(c_str) = CStr::from_ptr(ptr_or_handle as *const c_char).to_str() {
                return Some(c_str.to_string());
            }
        }
    }
    None
}

pub fn unpack_aot_arg(raw: u64) -> RuntimeValue {
    if raw == 0 {
        return RuntimeValue::Null;
    }
    if let Some(v) = aot_get_value(raw) {
        return v;
    }
    if raw > 0x10000 {
        if let Some(s) = get_string_val(raw) {
            return RuntimeValue::String(s);
        }
    }
    RuntimeValue::Int(raw as i64)
}

// ============================================================================
// Pretty Printing Engine
// ============================================================================

#[derive(Clone, Copy)]
pub struct ColorScheme {
    pub key: &'static str,
    pub string: &'static str,
    pub number: &'static str,
    pub boolean: &'static str,
    pub null: &'static str,
    pub bracket: &'static str,
    pub colon: &'static str,
    pub comma: &'static str,
    pub reset: &'static str,
    pub type_hint: &'static str,
}

impl ColorScheme {
    pub const fn default() -> Self {
        ColorScheme {
            key: "\x1b[38;2;156;220;254m",      // Light blue for keys
            string: "\x1b[38;2;206;145;120m",   // Peach/orange for strings
            number: "\x1b[38;2;181;206;168m",   // Light green for numbers
            boolean: "\x1b[38;2;86;156;214m",   // Blue for booleans
            null: "\x1b[38;2;128;128;128m",     // Gray for null
            bracket: "\x1b[38;2;212;212;212m",  // Light gray for brackets
            colon: "\x1b[38;2;212;212;212m",    // Light gray for colons
            comma: "\x1b[38;2;212;212;212m",    // Light gray for commas
            type_hint: "\x1b[38;2;78;201;176m", // Teal for type hints
            reset: "\x1b[0m",
        }
    }

    pub const fn simple() -> Self {
        ColorScheme {
            key: "\x1b[36m",
            string: "\x1b[33m",
            number: "\x1b[32m",
            boolean: "\x1b[35m",
            null: "\x1b[90m",
            bracket: "\x1b[37m",
            colon: "\x1b[37m",
            comma: "\x1b[37m",
            type_hint: "\x1b[96m",
            reset: "\x1b[0m",
        }
    }

    pub const fn none() -> Self {
        ColorScheme {
            key: "",
            string: "",
            number: "",
            boolean: "",
            null: "",
            bracket: "",
            colon: "",
            comma: "",
            type_hint: "",
            reset: "",
        }
    }
}

#[derive(Clone)]
pub struct PrettyPrintOptions {
    pub indent_str: String,
    pub max_depth: usize,
    pub colors: ColorScheme,
    pub show_types: bool,
    pub expand_objects: bool,
    pub show_indices: bool,
    pub align_values: bool,
}

impl Default for PrettyPrintOptions {
    fn default() -> Self {
        PrettyPrintOptions {
            indent_str: "  ".to_string(),
            max_depth: 10,
            colors: ColorScheme::default(),
            show_types: true,
            expand_objects: true,
            show_indices: false,
            align_values: true,
        }
    }
}

impl PrettyPrintOptions {
    pub fn no_color() -> Self {
        PrettyPrintOptions {
            colors: ColorScheme::none(),
            ..Default::default()
        }
    }

    pub fn simple_color() -> Self {
        PrettyPrintOptions {
            colors: ColorScheme::simple(),
            ..Default::default()
        }
    }

    pub fn compact() -> Self {
        PrettyPrintOptions {
            show_types: false,
            expand_objects: true,
            show_indices: false,
            align_values: false,
            ..Default::default()
        }
    }
}

fn infer_integer_hint_from_number(n: f64) -> Option<&'static str> {
    if !n.is_finite() {
        return None;
    }
    if n.fract() != 0.0 {
        let f32_val = n as f32 as f64;
        if (f32_val - n).abs() < 1e-6 && n.abs() <= 1e7 {
            Some("f32")
        } else {
            Some("f64")
        }
    } else if n >= 0.0 {
        if n <= u8::MAX as f64 {
            Some("u8")
        } else if n <= u16::MAX as f64 {
            Some("u16")
        } else if n <= u32::MAX as f64 {
            Some("u32")
        } else {
            Some("u64")
        }
    } else if n >= i8::MIN as f64 {
        Some("i8")
    } else if n >= i16::MIN as f64 {
        Some("i16")
    } else if n >= i32::MIN as f64 {
        Some("i32")
    } else {
        Some("i64")
    }
}

fn pretty_print(value: &RuntimeValue, options: &PrettyPrintOptions) -> String {
    let mut output = String::new();
    pretty_print_inner(value, options, &mut output, 0, 0);
    output
}

fn pretty_print_inner(
    value: &RuntimeValue,
    options: &PrettyPrintOptions,
    output: &mut String,
    depth: usize,
    current_indent: usize,
) {
    if depth >= options.max_depth {
        let _ = write!(
            output,
            "{}<max depth>{}",
            options.colors.null, options.colors.reset
        );
        return;
    }

    match value {
        RuntimeValue::Null => {
            let _ = write!(
                output,
                "{}null{}",
                options.colors.null, options.colors.reset
            );
        }
        RuntimeValue::Bool(b) => {
            let _ = write!(
                output,
                "{}{}{}",
                options.colors.boolean, b, options.colors.reset
            );
            if options.show_types {
                let _ = write!(
                    output,
                    " {}⟨bool⟩{}",
                    options.colors.type_hint, options.colors.reset
                );
            }
        }
        RuntimeValue::Int(n) => {
            let _ = write!(
                output,
                "{}{}{}",
                options.colors.number, n, options.colors.reset
            );
            if options.show_types {
                let hint = infer_integer_hint_from_number(*n as f64).unwrap_or("i64");
                let _ = write!(
                    output,
                    " {}⟨{}⟩{}",
                    options.colors.type_hint, hint, options.colors.reset
                );
            }
        }
        RuntimeValue::Float(n) => {
            let _ = write!(
                output,
                "{}{}{}",
                options.colors.number, n, options.colors.reset
            );
            if options.show_types {
                let hint = infer_integer_hint_from_number(*n).unwrap_or("number");
                let _ = write!(
                    output,
                    " {}⟨{}⟩{}",
                    options.colors.type_hint, hint, options.colors.reset
                );
            }
        }
        RuntimeValue::Char(c) => {
            let _ = write!(
                output,
                "{}'{}'{}",
                options.colors.string, c, options.colors.reset
            );
            if options.show_types {
                let _ = write!(
                    output,
                    " {}⟨char⟩{}",
                    options.colors.type_hint, options.colors.reset
                );
            }
        }
        RuntimeValue::String(s) => {
            let _ = write!(
                output,
                "{}\"{}\"{}",
                options.colors.string, s, options.colors.reset
            );
            if options.show_types {
                let _ = write!(
                    output,
                    " {}⟨string⟩{}",
                    options.colors.type_hint, options.colors.reset
                );
            }
        }
        RuntimeValue::U8(n) => print_typed_num(output, n, "u8", options),
        RuntimeValue::U16(n) => print_typed_num(output, n, "u16", options),
        RuntimeValue::U32(n) => print_typed_num(output, n, "u32", options),
        RuntimeValue::U64(n) => print_typed_num(output, n, "u64", options),
        RuntimeValue::U128(n) => print_typed_num(output, n, "u128", options),
        RuntimeValue::I8(n) => print_typed_num(output, n, "i8", options),
        RuntimeValue::I16(n) => print_typed_num(output, n, "i16", options),
        RuntimeValue::I32(n) => print_typed_num(output, n, "i32", options),
        RuntimeValue::I64(n) => print_typed_num(output, n, "i64", options),
        RuntimeValue::I128(n) => print_typed_num(output, n, "i128", options),
        RuntimeValue::F32(n) => print_typed_num(output, n, "f32", options),
        RuntimeValue::F64(n) => print_typed_num(output, n, "f64", options),
        RuntimeValue::Array(arr) => {
            if arr.is_empty() {
                let _ = write!(
                    output,
                    "{}[]{}",
                    options.colors.bracket, options.colors.reset
                );
                if options.show_types {
                    let _ = write!(
                        output,
                        " {}⟨array⟩{}",
                        options.colors.type_hint, options.colors.reset
                    );
                }
            } else {
                print_array_impl(arr, options, output, depth, current_indent);
            }
        }
        RuntimeValue::Tuple(tup) => {
            let _ = write!(
                output,
                "{}({}\n",
                options.colors.bracket, options.colors.reset
            );
            let new_indent = current_indent + 1;
            for (i, item) in tup.iter().enumerate() {
                for _ in 0..new_indent {
                    output.push_str(&options.indent_str);
                }
                pretty_print_inner(item, options, output, depth + 1, new_indent);
                if i < tup.len() - 1 {
                    let _ = write!(output, "{},{}", options.colors.comma, options.colors.reset);
                }
                output.push('\n');
            }
            for _ in 0..current_indent {
                output.push_str(&options.indent_str);
            }
            let _ = write!(
                output,
                "{}){}",
                options.colors.bracket, options.colors.reset
            );
            if options.show_types {
                let _ = write!(
                    output,
                    " {}⟨tuple[{}]⟩{}",
                    options.colors.type_hint,
                    tup.len(),
                    options.colors.reset
                );
            }
        }
        RuntimeValue::Set(set) => {
            let _ = write!(
                output,
                "{}{{{}\n",
                options.colors.bracket, options.colors.reset
            );
            let new_indent = current_indent + 1;
            for (i, item) in set.iter().enumerate() {
                for _ in 0..new_indent {
                    output.push_str(&options.indent_str);
                }
                pretty_print_inner(item, options, output, depth + 1, new_indent);
                if i < set.len() - 1 {
                    let _ = write!(output, "{},{}", options.colors.comma, options.colors.reset);
                }
                output.push('\n');
            }
            for _ in 0..current_indent {
                output.push_str(&options.indent_str);
            }
            let _ = write!(
                output,
                "{}}}{}",
                options.colors.bracket, options.colors.reset
            );
            if options.show_types {
                let _ = write!(
                    output,
                    " {}⟨set[{}]⟩{}",
                    options.colors.type_hint,
                    set.len(),
                    options.colors.reset
                );
            }
        }
        RuntimeValue::Object(obj) => {
            if obj.is_empty() {
                let _ = write!(
                    output,
                    "{}{{}}{}",
                    options.colors.bracket, options.colors.reset
                );
                if options.show_types {
                    let _ = write!(
                        output,
                        " {}⟨object⟩{}",
                        options.colors.type_hint, options.colors.reset
                    );
                }
            } else {
                print_object_impl(obj, options, output, depth, current_indent);
            }
        }
        RuntimeValue::Ref(inner) => {
            pretty_print_inner(inner, options, output, depth, current_indent);
        }
    }
}

fn print_typed_num<T: std::fmt::Display>(
    output: &mut String,
    val: &T,
    type_name: &str,
    options: &PrettyPrintOptions,
) {
    let _ = write!(
        output,
        "{}{}{}",
        options.colors.number, val, options.colors.reset
    );
    if options.show_types {
        let _ = write!(
            output,
            " {}⟨{}⟩{}",
            options.colors.type_hint, type_name, options.colors.reset
        );
    }
}

fn print_array_impl(
    arr: &[RuntimeValue],
    options: &PrettyPrintOptions,
    output: &mut String,
    depth: usize,
    current_indent: usize,
) {
    let _ = write!(
        output,
        "{}[{}\n",
        options.colors.bracket, options.colors.reset
    );

    let new_indent = current_indent + 1;
    for (i, item) in arr.iter().enumerate() {
        for _ in 0..new_indent {
            output.push_str(&options.indent_str);
        }
        if options.show_indices {
            let _ = write!(
                output,
                "{}{}:{} ",
                options.colors.type_hint, i, options.colors.reset
            );
        }
        pretty_print_inner(item, options, output, depth + 1, new_indent);
        if i < arr.len() - 1 {
            let _ = write!(output, "{},{}", options.colors.comma, options.colors.reset);
        }
        output.push('\n');
    }

    for _ in 0..current_indent {
        output.push_str(&options.indent_str);
    }
    let _ = write!(
        output,
        "{}]{}",
        options.colors.bracket, options.colors.reset
    );

    if options.show_types {
        let _ = write!(
            output,
            " {}⟨array[{}]⟩{}",
            options.colors.type_hint,
            arr.len(),
            options.colors.reset
        );
    }
}

fn print_object_impl(
    obj: &BTreeMap<String, RuntimeValue>,
    options: &PrettyPrintOptions,
    output: &mut String,
    depth: usize,
    current_indent: usize,
) {
    let _ = write!(
        output,
        "{}{{{}\n",
        options.colors.bracket, options.colors.reset
    );

    let new_indent = current_indent + 1;
    let mut keys: Vec<_> = obj.keys().collect();
    keys.sort();

    let max_key_len = if options.align_values {
        keys.iter().map(|k| k.len()).max().unwrap_or(0)
    } else {
        0
    };

    for (i, key) in keys.iter().enumerate() {
        for _ in 0..new_indent {
            output.push_str(&options.indent_str);
        }

        let _ = write!(output, "{}", options.colors.key);
        if options.align_values {
            let _ = write!(output, "{:<width$}", key, width = max_key_len);
        } else {
            let _ = write!(output, "{}", key);
        }
        let _ = write!(output, "{}", options.colors.reset);

        let _ = write!(output, "{}: {}", options.colors.colon, options.colors.reset);

        if let Some(value) = obj.get(*key) {
            pretty_print_inner(value, options, output, depth + 1, new_indent);
        }

        if i < keys.len() - 1 {
            let _ = write!(output, "{},{}", options.colors.comma, options.colors.reset);
        }
        output.push('\n');
    }

    for _ in 0..current_indent {
        output.push_str(&options.indent_str);
    }
    let _ = write!(
        output,
        "{}}}{}",
        options.colors.bracket, options.colors.reset
    );

    if options.show_types {
        let _ = write!(
            output,
            " {}⟨object⟩{}",
            options.colors.type_hint, options.colors.reset
        );
    }
}

// ============================================================================
// C-ABI Exports for Object, Array, Value Construction
// ============================================================================

#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_make_object(args_ptr: *const u64, arg_count: usize) -> u64 {
    if arg_count == 0 {
        return aot_store_value(RuntimeValue::Object(BTreeMap::new()));
    }
    if args_ptr.is_null() || arg_count % 2 != 0 {
        return 0;
    }

    let args = unsafe { std::slice::from_raw_parts(args_ptr, arg_count) };
    let mut obj = BTreeMap::new();

    for chunk in args.chunks(2) {
        if chunk.len() == 2 {
            let key_handle = chunk[0];
            let val_handle = chunk[1];

            let key_str = if let Some(s) = get_string_val(key_handle) {
                s
            } else if let Some(RuntimeValue::String(s)) = aot_get_value(key_handle) {
                s
            } else {
                continue;
            };

            let val = aot_get_value(val_handle).unwrap_or_else(|| unpack_aot_arg(val_handle));
            obj.insert(key_str, val);
        }
    }

    aot_store_value(RuntimeValue::Object(obj))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_set_field(obj_handle: u64, field_handle: u64, val_handle: u64) -> u64 {
    let field_name = match get_string_val(field_handle) {
        Some(s) => s,
        None => return 0,
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

#[unsafe(no_mangle)]
pub extern "C" fn aot_get_field(obj_handle: u64, field_handle: u64) -> u64 {
    let field_name = match get_string_val(field_handle) {
        Some(s) => s,
        None => return aot_store_value(RuntimeValue::Null),
    };

    let resolved = aot_get_value(obj_handle).unwrap_or(RuntimeValue::Null);
    match resolved {
        RuntimeValue::Object(obj) => {
            let val = obj.get(&field_name).cloned().unwrap_or(RuntimeValue::Null);
            aot_store_value(val)
        }
        RuntimeValue::Array(arr) => {
            let val = match field_name.as_str() {
                "len" | "length" => RuntimeValue::Int(arr.len() as i64),
                "capacity" => RuntimeValue::Int(arr.capacity() as i64),
                _ => RuntimeValue::Null,
            };
            aot_store_value(val)
        }
        RuntimeValue::String(s) => {
            let val = match field_name.as_str() {
                "len" | "length" => RuntimeValue::Int(s.len() as i64),
                _ => RuntimeValue::Null,
            };
            aot_store_value(val)
        }
        _ => aot_store_value(RuntimeValue::Null),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_make_array(args_ptr: *const u64, arg_count: usize) -> u64 {
    if args_ptr.is_null() || arg_count == 0 {
        return aot_store_value(RuntimeValue::Array(Vec::new()));
    }

    let args = unsafe { std::slice::from_raw_parts(args_ptr, arg_count) };
    let mut arr = Vec::with_capacity(arg_count);

    for &handle in args {
        let val = aot_get_value(handle).unwrap_or_else(|| unpack_aot_arg(handle));
        arr.push(val);
    }

    aot_store_value(RuntimeValue::Array(arr))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_make_string(string_ptr: *const c_char) -> u64 {
    if string_ptr.is_null() {
        return aot_store_value(RuntimeValue::String(String::new()));
    }
    let s = unsafe {
        CStr::from_ptr(string_ptr)
            .to_str()
            .unwrap_or("")
            .to_string()
    };
    aot_store_value(RuntimeValue::String(s))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_wrap_ptr(value: u64) -> u64 {
    if value == 0 {
        return aot_store_value(RuntimeValue::Null);
    }
    if aot_get_value(value).is_some() {
        return value;
    }
    if value < 0x10000 {
        return aot_store_value(RuntimeValue::Int(value as i64));
    }
    unsafe { aot_make_string(value as *const c_char) }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_u8(value: u64) -> u64 {
    aot_store_value(RuntimeValue::U8(value as u8))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_u16(value: u64) -> u64 {
    aot_store_value(RuntimeValue::U16(value as u16))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_u32(value: u64) -> u64 {
    aot_store_value(RuntimeValue::U32(value as u32))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_u64(value: u64) -> u64 {
    aot_store_value(RuntimeValue::U64(value))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_i8(value: i64) -> u64 {
    aot_store_value(RuntimeValue::I8(value as i8))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_i16(value: i64) -> u64 {
    aot_store_value(RuntimeValue::I16(value as i16))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_i32(value: i64) -> u64 {
    aot_store_value(RuntimeValue::I32(value as i32))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_i64(value: i64) -> u64 {
    aot_store_value(RuntimeValue::Int(value))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_f32(value: f64) -> u64 {
    aot_store_value(RuntimeValue::F32(value as f32))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_f64(value: f64) -> u64 {
    aot_store_value(RuntimeValue::Float(value))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_bool(value: i64) -> u64 {
    aot_store_value(RuntimeValue::Bool(value != 0))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_char(value: u64) -> u64 {
    let ch = char::from_u32(value as u32).unwrap_or('\0');
    aot_store_value(RuntimeValue::Char(ch))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_make_tuple(args_ptr: *const u64, arg_count: usize) -> u64 {
    if args_ptr.is_null() || arg_count == 0 {
        return aot_store_value(RuntimeValue::Tuple(Vec::new()));
    }
    let args = unsafe { std::slice::from_raw_parts(args_ptr, arg_count) };
    let mut arr = Vec::with_capacity(arg_count);
    for &handle in args {
        arr.push(aot_get_value(handle).unwrap_or_else(|| unpack_aot_arg(handle)));
    }
    aot_store_value(RuntimeValue::Tuple(arr))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_make_set(args_ptr: *const u64, arg_count: usize) -> u64 {
    if args_ptr.is_null() || arg_count == 0 {
        return aot_store_value(RuntimeValue::Set(Vec::new()));
    }
    let args = unsafe { std::slice::from_raw_parts(args_ptr, arg_count) };
    let mut arr = Vec::with_capacity(arg_count);
    for &handle in args {
        arr.push(aot_get_value(handle).unwrap_or_else(|| unpack_aot_arg(handle)));
    }
    aot_store_value(RuntimeValue::Set(arr))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_null() -> u64 {
    aot_store_value(RuntimeValue::Null)
}

// ============================================================================
// Pretty Printing & Console Output Exports
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn aot_print_value_pretty(handle: u64, mode: i64) -> u64 {
    let val = aot_get_value(handle).unwrap_or_else(|| unpack_aot_arg(handle));
    let opts = match mode {
        2 => PrettyPrintOptions::compact(),
        3 => PrettyPrintOptions::simple_color(),
        1 => PrettyPrintOptions::default(),
        _ => {
            print!("{}", val.as_string());
            let _ = std::io::stdout().flush();
            return 0;
        }
    };
    let output = pretty_print(&val, &opts);
    print!("{}", output);
    let _ = std::io::stdout().flush();
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_print_newline() -> u64 {
    println!();
    let _ = std::io::stdout().flush();
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_print_space() -> u64 {
    print!(" ");
    let _ = std::io::stdout().flush();
    0
}

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

#[unsafe(no_mangle)]
pub extern "C" fn aot_print_str(string_ptr: i64, newline: i64) -> u64 {
    if string_ptr == 0 {
        return 0;
    }
    unsafe {
        let ptr = string_ptr as *const c_char;
        if let Ok(s) = CStr::from_ptr(ptr).to_str() {
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_print_with_options(
    values_ptr: *const u64,
    values_count: i64,
    options_handle: u64,
) -> u64 {
    if values_count <= 0 {
        return 0;
    }
    let count = values_count as usize;
    if values_ptr.is_null() {
        return 0;
    }
    let handles = unsafe { std::slice::from_raw_parts(values_ptr, count) };

    // Check options object
    let mut pretty_mode = 0i64;
    let mut sep = " ".to_string();
    let mut end = "\n".to_string();
    let mut has_options = false;

    if let Some(RuntimeValue::Object(ref opts_obj)) = aot_get_value(options_handle) {
        has_options = opts_obj.contains_key("pretty")
            || opts_obj.contains_key("sep")
            || opts_obj.contains_key("end")
            || opts_obj.contains_key("color")
            || opts_obj.contains_key("bold");

        if let Some(pv) = opts_obj.get("pretty") {
            pretty_mode = match pv {
                RuntimeValue::Bool(true) => 1,
                RuntimeValue::String(s) => match s.as_str() {
                    "compact" => 2,
                    "simple" => 3,
                    "full" | "true" => 1,
                    _ => 0,
                },
                _ => 0,
            };
        }
        if let Some(RuntimeValue::String(s)) = opts_obj.get("sep") {
            sep = s.clone();
        }
        if let Some(RuntimeValue::String(s)) = opts_obj.get("end") {
            end = s.clone();
        }
    }

    let effective_len = if has_options && handles.last() == Some(&options_handle) {
        count.saturating_sub(1)
    } else {
        count
    };

    let pretty_opts = match pretty_mode {
        2 => PrettyPrintOptions::compact(),
        3 => PrettyPrintOptions::simple_color(),
        1 => PrettyPrintOptions::default(),
        _ => PrettyPrintOptions::no_color(),
    };

    for (i, &handle) in handles.iter().take(effective_len).enumerate() {
        if i > 0 {
            print!("{}", sep);
        }
        let val = aot_get_value(handle).unwrap_or_else(|| unpack_aot_arg(handle));
        if pretty_mode > 0 {
            let formatted = pretty_print(&val, &pretty_opts);
            print!("{}", formatted);
        } else {
            print!("{}", val.as_string());
        }
    }

    if !end.is_empty() {
        print!("{}", end);
    }
    let _ = std::io::stdout().flush();
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_free_handle(handle: u64) {
    aot_remove_value(handle);
}

// ============================================================================
// Sequence / Collection Operations
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn aot_len(handle: u64) -> i64 {
    let val = aot_get_value(handle).unwrap_or_else(|| unpack_aot_arg(handle));
    match val {
        RuntimeValue::Array(v) => v.len() as i64,
        RuntimeValue::Tuple(v) => v.len() as i64,
        RuntimeValue::Set(v) => v.len() as i64,
        RuntimeValue::String(s) => s.len() as i64,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_capacity(handle: u64) -> i64 {
    let val = aot_get_value(handle).unwrap_or_else(|| unpack_aot_arg(handle));
    match val {
        RuntimeValue::Array(v) => v.capacity() as i64,
        RuntimeValue::String(s) => s.capacity() as i64,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_metadata_size(_handle: u64) -> i64 {
    24
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_first(handle: u64) -> u64 {
    let val = aot_get_value(handle).unwrap_or_else(|| unpack_aot_arg(handle));
    match val {
        RuntimeValue::Array(v) => {
            if let Some(first) = v.first() {
                aot_store_value(first.clone())
            } else {
                aot_store_value(RuntimeValue::Null)
            }
        }
        _ => aot_store_value(RuntimeValue::Null),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_last(handle: u64) -> u64 {
    let val = aot_get_value(handle).unwrap_or_else(|| unpack_aot_arg(handle));
    match val {
        RuntimeValue::Array(v) => {
            if let Some(last) = v.last() {
                aot_store_value(last.clone())
            } else {
                aot_store_value(RuntimeValue::Null)
            }
        }
        _ => aot_store_value(RuntimeValue::Null),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_get_index(container_handle: u64, index_handle: u64) -> u64 {
    let container =
        aot_get_value(container_handle).unwrap_or_else(|| unpack_aot_arg(container_handle));
    let index = match aot_get_value(index_handle).unwrap_or_else(|| unpack_aot_arg(index_handle)) {
        RuntimeValue::Int(i) => i as usize,
        RuntimeValue::U64(u) => u as usize,
        _ => 0,
    };

    match container {
        RuntimeValue::Array(arr) => {
            if index < arr.len() {
                aot_store_value(arr[index].clone())
            } else {
                aot_store_value(RuntimeValue::Null)
            }
        }
        RuntimeValue::Tuple(tup) => {
            if index < tup.len() {
                aot_store_value(tup[index].clone())
            } else {
                aot_store_value(RuntimeValue::Null)
            }
        }
        RuntimeValue::String(s) => {
            if let Some(ch) = s.chars().nth(index) {
                aot_store_value(RuntimeValue::Char(ch))
            } else {
                aot_store_value(RuntimeValue::Null)
            }
        }
        _ => aot_store_value(RuntimeValue::Null),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_set_index(
    container_handle: u64,
    index_handle: u64,
    val_handle: u64,
) -> u64 {
    let mut container = aot_get_value(container_handle).unwrap_or(RuntimeValue::Null);
    let index = match aot_get_value(index_handle).unwrap_or_else(|| unpack_aot_arg(index_handle)) {
        RuntimeValue::Int(i) => i as usize,
        RuntimeValue::U64(u) => u as usize,
        _ => 0,
    };
    let val = aot_get_value(val_handle).unwrap_or_else(|| unpack_aot_arg(val_handle));

    match container {
        RuntimeValue::Array(ref mut arr) => {
            if index < arr.len() {
                arr[index] = val;
            } else if index == arr.len() {
                arr.push(val);
            }
            aot_store_value(container)
        }
        _ => container_handle,
    }
}

// ============================================================================
// ARC / Reference Counting Runtime
// ============================================================================

struct ArcControlBlock {
    strong: AtomicI64,
    weak: AtomicI64,
    value: RuntimeValue,
}

static ARC_MAP: Mutex<Option<BTreeMap<u64, Arc<ArcControlBlock>>>> = Mutex::new(None);
static ARC_COUNTER: AtomicU64 = AtomicU64::new(1000);

fn get_arc_map() -> std::sync::MutexGuard<'static, Option<BTreeMap<u64, Arc<ArcControlBlock>>>> {
    let mut guard = ARC_MAP.lock().unwrap();
    if guard.is_none() {
        *guard = Some(BTreeMap::new());
    }
    guard
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_arc_new(val_handle: u64) -> u64 {
    let val = aot_get_value(val_handle).unwrap_or_else(|| unpack_aot_arg(val_handle));
    let block = Arc::new(ArcControlBlock {
        strong: AtomicI64::new(1),
        weak: AtomicI64::new(0),
        value: val,
    });
    let id = ARC_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut map = get_arc_map();
    if let Some(ref mut m) = *map {
        m.insert(id, block);
    }
    id
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_arc_clone(handle: u64) -> u64 {
    let map = get_arc_map();
    if let Some(ref m) = *map {
        if let Some(block) = m.get(&handle) {
            block.strong.fetch_add(1, Ordering::SeqCst);
        }
    }
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_arc_drop(handle: u64) -> u64 {
    let mut map = get_arc_map();
    if let Some(ref mut m) = *map {
        let should_remove = if let Some(block) = m.get(&handle) {
            let prev = block.strong.fetch_sub(1, Ordering::SeqCst);
            prev <= 1
        } else {
            false
        };
        if should_remove {
            m.remove(&handle);
        }
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_arc_get(handle: u64) -> u64 {
    let map = get_arc_map();
    if let Some(ref m) = *map {
        if let Some(block) = m.get(&handle) {
            return aot_store_value(block.value.clone());
        }
    }
    aot_store_value(RuntimeValue::Null)
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_arc_set(_handle: u64, _val: u64) -> u64 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_arc_strong_count(handle: u64) -> i64 {
    let map = get_arc_map();
    if let Some(ref m) = *map {
        if let Some(block) = m.get(&handle) {
            return block.strong.load(Ordering::SeqCst);
        }
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_arc_weak_count(handle: u64) -> i64 {
    let map = get_arc_map();
    if let Some(ref m) = *map {
        if let Some(block) = m.get(&handle) {
            return block.weak.load(Ordering::SeqCst);
        }
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_weak_new(handle: u64) -> u64 {
    let map = get_arc_map();
    if let Some(ref m) = *map {
        if let Some(block) = m.get(&handle) {
            block.weak.fetch_add(1, Ordering::SeqCst);
        }
    }
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_weak_drop(handle: u64) -> u64 {
    let map = get_arc_map();
    if let Some(ref m) = *map {
        if let Some(block) = m.get(&handle) {
            block.weak.fetch_sub(1, Ordering::SeqCst);
        }
    }
    0
}

// ============================================================================
// Memory Tracking / Heap Guard
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_assert_heap_allowed() -> i32 {
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_alloc_tracked(size: i64, _metadata_ptr: i64) -> i64 {
    unsafe {
        let ptr = libc::malloc(size as usize);
        ptr as i64
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_free_tracked(ptr: i64, _metadata_ptr: i64) -> i64 {
    if ptr != 0 {
        unsafe {
            libc::free(ptr as *mut libc::c_void);
        }
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_scope_exit(_scope_id: i64) -> i64 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_rt_validate_ptr(ptr: i64, _metadata_ptr: i64) -> i32 {
    if ptr != 0 { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_init_args(_argc: i32, _argv: *const *const c_char) {
    // Initialized command line args
}

#[unsafe(no_mangle)]
pub extern "C" fn adesh_value_to_string(value: u64) -> *mut c_char {
    let val = aot_get_value(value).unwrap_or_else(|| unpack_aot_arg(value));
    let s = val.as_string();
    let c_str = CString::new(s).unwrap_or_default();
    c_str.into_raw()
}

// ============================================================================
// Filesystem Support
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_read(path_val: u64) -> u64 {
    let path = match get_string_val(path_val) {
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
    let path = match get_string_val(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    let content = match get_string_val(content_val) {
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
    let path = match get_string_val(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    aot_store_value(RuntimeValue::Bool(std::path::Path::new(&path).exists()))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_is_file(path_val: u64) -> u64 {
    let path = match get_string_val(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    aot_store_value(RuntimeValue::Bool(std::path::Path::new(&path).is_file()))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_is_dir(path_val: u64) -> u64 {
    let path = match get_string_val(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    aot_store_value(RuntimeValue::Bool(std::path::Path::new(&path).is_dir()))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_mkdir(path_val: u64) -> u64 {
    let path = match get_string_val(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    match std::fs::create_dir_all(&path) {
        Ok(_) => aot_store_value(RuntimeValue::Bool(true)),
        Err(_) => aot_store_value(RuntimeValue::Bool(false)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_delete(path_val: u64) -> u64 {
    let path = match get_string_val(path_val) {
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
pub extern "C" fn aot_fs_copy(src_val: u64, dst_val: u64) -> u64 {
    let src = match get_string_val(src_val) {
        Some(s) => s,
        None => return aot_store_value(RuntimeValue::Null),
    };
    let dst = match get_string_val(dst_val) {
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
    let src = match get_string_val(src_val) {
        Some(s) => s,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    let dst = match get_string_val(dst_val) {
        Some(d) => d,
        None => return aot_store_value(RuntimeValue::Bool(false)),
    };
    match std::fs::rename(&src, &dst) {
        Ok(_) => aot_store_value(RuntimeValue::Bool(true)),
        Err(_) => aot_store_value(RuntimeValue::Bool(false)),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn aot_fs_path_join(args_ptr: *const u64, arg_count: usize) -> u64 {
    if args_ptr.is_null() || arg_count == 0 {
        return aot_store_value(RuntimeValue::String(String::new()));
    }
    let args = unsafe { std::slice::from_raw_parts(args_ptr, arg_count) };
    let mut path = std::path::PathBuf::new();
    for &arg in args {
        if let Some(s) = get_string_val(arg) {
            path.push(s);
        }
    }
    aot_store_value(RuntimeValue::String(path.to_string_lossy().into_owned()))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_path_basename(path_val: u64) -> u64 {
    let path_str = match get_string_val(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::String(String::new())),
    };
    let p = std::path::Path::new(&path_str);
    let base = p
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    aot_store_value(RuntimeValue::String(base))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_path_dirname(path_val: u64) -> u64 {
    let path_str = match get_string_val(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::String(String::new())),
    };
    let p = std::path::Path::new(&path_str);
    let dir = p
        .parent()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    aot_store_value(RuntimeValue::String(dir))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_path_extname(path_val: u64) -> u64 {
    let path_str = match get_string_val(path_val) {
        Some(p) => p,
        None => return aot_store_value(RuntimeValue::String(String::new())),
    };
    let p = std::path::Path::new(&path_str);
    let ext = p
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    aot_store_value(RuntimeValue::String(ext))
}

// ============================================================================
// Exception Handling Bridge
// ============================================================================

use std::cell::RefCell;

thread_local! {
    static CURRENT_EXCEPTION: RefCell<Option<RuntimeValue>> = const { RefCell::new(None) };
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_has_exception() -> i64 {
    CURRENT_EXCEPTION.with(|exc| if exc.borrow().is_some() { 1 } else { 0 })
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_get_exception() -> u64 {
    CURRENT_EXCEPTION.with(|exc| {
        if let Some(val) = exc.borrow_mut().take() {
            aot_store_value(val)
        } else {
            aot_store_value(RuntimeValue::Null)
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_clear_exception() -> i64 {
    CURRENT_EXCEPTION.with(|exc| {
        *exc.borrow_mut() = None;
    });
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_throw_exception(val_handle: u64) -> i64 {
    let val = unpack_aot_arg(val_handle);
    CURRENT_EXCEPTION.with(|exc| {
        *exc.borrow_mut() = Some(val);
    });
    0
}
