//! Unified Host Symbol Resolution for Native JIT.
//!
//! Maps external runtime function names (builtins, ARC, print helpers, C library functions)
//! to their physical host memory addresses.

use std::collections::HashMap;

/// Universal symbol resolver interface for in-memory JIT execution.
pub trait HostSymbolResolver {
    /// Look up an absolute host memory address by symbol name.
    fn resolve(&self, symbol: &str) -> Option<*const u8>;
}

/// Standard Host Symbol Resolver populated with Adesh runtime bridges,
/// ARC FFI, and C runtime symbols.
pub struct StandardHostSymbolResolver {
    symbols: HashMap<String, *const u8>,
}

unsafe impl Send for StandardHostSymbolResolver {}
unsafe impl Sync for StandardHostSymbolResolver {}

impl Default for StandardHostSymbolResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl StandardHostSymbolResolver {
    /// Create a new host symbol resolver with all standard runtime symbols registered.
    pub fn new() -> Self {
        let mut symbols = HashMap::new();

        // Keep this list aligned with the symbols emitted by native/lower.rs.
        // The former resolver registered printing and composite helpers but
        // omitted scalar boxing helpers (notably `aot_make_i64`).  As a
        // result, a program containing an object literal could print its
        // first string and then fail during loading before any code ran.
        macro_rules! register_aot {
            ($($symbol:ident),+ $(,)?) => {
                $(
                    symbols.insert(
                        stringify!($symbol).to_string(),
                        crate::backends::aot::runtime_bridge::$symbol as *const u8,
                    );
                )+
            };
        }

        // 1. Standard C library functions
        symbols.insert("printf".to_string(), libc::printf as *const u8);
        symbols.insert("malloc".to_string(), libc::malloc as *const u8);
        symbols.insert("free".to_string(), libc::free as *const u8);
        symbols.insert("memcpy".to_string(), libc::memcpy as *const u8);
        symbols.insert("puts".to_string(), libc::puts as *const u8);
        symbols.insert(
            "aot_abort_str".to_string(),
            jit_aot_abort_str as *const u8,
        );
        // These adapters deliberately use the AOT bridge value table, which
        // is also used by the scalar/container boxing entries below.
        symbols.insert("aot_typeof".to_string(), jit_aot_typeof as *const u8);
        symbols.insert("aot_sizeof".to_string(), jit_aot_sizeof as *const u8);
        symbols.insert("aot_clock".to_string(), jit_aot_clock as *const u8);
        symbols.insert("aot_alloc".to_string(), adesh_runtime::aot_alloc as *const u8);
        symbols.insert("aot_free".to_string(), adesh_runtime::aot_free as *const u8);
        symbols.insert(
            "aot_get_index_handle".to_string(),
            crate::backends::aot::runtime_bridge::aot_get_index as *const u8,
        );
        symbols.insert(
            "aot_make_error".to_string(),
            crate::backends::aot::runtime_bridge::aot_make_error as *const u8,
        );
        symbols.insert(
            "aot_make_set_from".to_string(),
            crate::backends::aot::runtime_bridge::aot_make_set_from as *const u8,
        );
        symbols.insert(
            "aot_slice_array".to_string(),
            crate::backends::aot::runtime_bridge::aot_slice_array as *const u8,
        );
        symbols.insert(
            "aot_array_init".to_string(),
            crate::backends::aot::runtime_bridge::aot_array_init as *const u8,
        );
        symbols.insert("aot_input".to_string(), jit_aot_input as *const u8);
        symbols.insert(
            "aot_input_mock".to_string(),
            crate::backends::aot::runtime_bridge::aot_input_mock as *const u8,
        );
        symbols.insert(
            "aot_string_concat_cstr".to_string(),
            jit_aot_string_concat_cstr as *const u8,
        );
        symbols.insert(
            "aot_value_to_string_cstr".to_string(),
            jit_aot_value_to_string_cstr as *const u8,
        );
        symbols.insert(
            "aot_to_int".to_string(),
            crate::backends::aot::runtime_bridge::aot_to_int as *const u8,
        );
        symbols.insert(
            "aot_to_float".to_string(),
            crate::backends::aot::runtime_bridge::aot_to_float as *const u8,
        );
        symbols.insert(
            "aot_to_bool".to_string(),
            crate::backends::aot::runtime_bridge::aot_to_bool as *const u8,
        );
        symbols.insert(
            "aot_to_string".to_string(),
            crate::backends::aot::runtime_bridge::aot_to_string as *const u8,
        );
        symbols.insert(
            "aot_format_value".to_string(),
            jit_aot_format_value as *const u8,
        );
        symbols.insert("aot_assert".to_string(), jit_aot_assert as *const u8);
        symbols.insert("aot_assert_eq".to_string(), jit_aot_assert_eq as *const u8);
        symbols.insert("aot_assert_ne".to_string(), jit_aot_assert_ne as *const u8);
        symbols.insert(
            "aot_print_string".to_string(),
            jit_aot_print_string as *const u8,
        );
        symbols.insert(
            "aot_make_array_spread".to_string(),
            jit_aot_make_array_spread as *const u8,
        );
        symbols.insert("aot_unbox".to_string(), jit_aot_unbox as *const u8);

        // Scalar values, containers, object access, and the generic runtime
        // operations used by the direct HIR -> Machine IR lowering path.
        register_aot!(
            aot_make_i8,
            aot_make_i16,
            aot_make_i32,
            aot_make_i64,
            aot_make_u8,
            aot_make_u16,
            aot_make_u32,
            aot_make_u64,
            aot_make_f32,
            aot_make_f64,
            aot_make_bool,
            aot_make_char,
            aot_make_null,
            aot_make_tuple,
            aot_make_set,
            aot_make_range,
            aot_get_index,
            aot_set_index,
            aot_len,
            aot_capacity,
            aot_metadata_size,
            aot_call_method,
            aot_contains,
            aot_has_exception,
            aot_get_exception,
            aot_clear_exception,
            aot_throw_exception,
            aot_value_eq,
            aot_print_i8,
            aot_print_i16,
            aot_print_i32,
            aot_print_null,
        );

        // Native lowering uses `release` terminology while the shared AOT
        // bridge exposes the same operation as `aot_free_handle`.  It is
        // important to use this bridge (rather than the similarly named
        // function in a separate runtime crate), because all boxing helpers
        // above allocate handles in this bridge's table.
        symbols.insert(
            "aot_release_handle".to_string(),
            crate::backends::aot::runtime_bridge::aot_free_handle as *const u8,
        );

        // 2. AOT / Runtime bridge print functions
        symbols.insert(
            "aot_print_i64".to_string(),
            crate::backends::aot::runtime_bridge::aot_print_i64 as *const u8,
        );
        symbols.insert(
            "aot_print_u8".to_string(),
            crate::backends::aot::runtime_bridge::aot_print_u8 as *const u8,
        );
        symbols.insert(
            "aot_print_u16".to_string(),
            crate::backends::aot::runtime_bridge::aot_print_u16 as *const u8,
        );
        symbols.insert(
            "aot_print_u32".to_string(),
            crate::backends::aot::runtime_bridge::aot_print_u32 as *const u8,
        );
        symbols.insert(
            "aot_print_u64".to_string(),
            crate::backends::aot::runtime_bridge::aot_print_u64 as *const u8,
        );
        symbols.insert(
            "aot_print_f32".to_string(),
            crate::backends::aot::runtime_bridge::aot_print_f32 as *const u8,
        );
        symbols.insert(
            "aot_print_f64".to_string(),
            crate::backends::aot::runtime_bridge::aot_print_f64 as *const u8,
        );
        symbols.insert(
            "aot_print_bool".to_string(),
            crate::backends::aot::runtime_bridge::aot_print_bool as *const u8,
        );
        symbols.insert(
            "aot_print_str".to_string(),
            crate::backends::aot::runtime_bridge::aot_print_str as *const u8,
        );
        symbols.insert(
            "aot_print_cstr".to_string(),
            jit_aot_print_cstr as *const u8,
        );
        symbols.insert(
            "aot_make_string".to_string(),
            crate::backends::aot::runtime_bridge::aot_make_string as *const u8,
        );
        symbols.insert(
            "aot_wrap_ptr".to_string(),
            crate::backends::aot::runtime_bridge::aot_wrap_ptr as *const u8,
        );
        symbols.insert(
            "aot_make_object".to_string(),
            crate::backends::aot::runtime_bridge::aot_make_object as *const u8,
        );
        symbols.insert(
            "aot_make_dict".to_string(),
            crate::backends::aot::runtime_bridge::aot_make_dict as *const u8,
        );
        symbols.insert(
            "aot_make_array".to_string(),
            crate::backends::aot::runtime_bridge::aot_make_array as *const u8,
        );
        symbols.insert(
            "aot_string_concat".to_string(),
            crate::backends::aot::runtime_bridge::aot_string_concat as *const u8,
        );
        symbols.insert(
            "aot_set_field".to_string(),
            crate::backends::aot::runtime_bridge::aot_set_field as *const u8,
        );
        symbols.insert(
            "aot_get_field".to_string(),
            crate::backends::aot::runtime_bridge::aot_get_field as *const u8,
        );
        symbols.insert(
            "aot_set_field_str".to_string(),
            crate::backends::aot::runtime_bridge::aot_set_field_str as *const u8,
        );
        symbols.insert(
            "aot_get_field_str".to_string(),
            crate::backends::aot::runtime_bridge::aot_get_field_str as *const u8,
        );
        symbols.insert(
            "aot_print_value_pretty".to_string(),
            crate::backends::aot::runtime_bridge::aot_print_value_pretty as *const u8,
        );
        symbols.insert(
            "aot_print_newline".to_string(),
            crate::backends::aot::runtime_bridge::aot_print_newline as *const u8,
        );
        symbols.insert(
            "aot_print_space".to_string(),
            crate::backends::aot::runtime_bridge::aot_print_space as *const u8,
        );
        symbols.insert(
            "aot_print_with_options".to_string(),
            crate::backends::aot::runtime_bridge::aot_print_with_options as *const u8,
        );

        // 3. JIT runtime bridge symbols
        for (name, ptr) in crate::backends::jit::native::runtime_bridge::get_runtime_symbols() {
            symbols.insert(name.to_string(), ptr);
        }

        // 4. ARC memory management runtime functions
        use crate::execution::arc_bridge::*;
        symbols.insert("adesh_rt_arc_new".to_string(), adesh_rt_arc_new as *const u8);
        symbols.insert(
            "adesh_rt_arc_clone".to_string(),
            adesh_rt_arc_clone as *const u8,
        );
        symbols.insert(
            "adesh_rt_arc_drop".to_string(),
            adesh_rt_arc_drop as *const u8,
        );
        symbols.insert(
            "adesh_rt_weak_new".to_string(),
            adesh_rt_weak_new as *const u8,
        );
        symbols.insert(
            "adesh_rt_weak_drop".to_string(),
            adesh_rt_weak_drop as *const u8,
        );
        symbols.insert("adesh_rt_arc_get".to_string(), adesh_rt_arc_get as *const u8);
        symbols.insert("adesh_rt_arc_set".to_string(), adesh_rt_arc_set as *const u8);
        symbols.insert(
            "adesh_rt_arc_strong_count".to_string(),
            adesh_rt_arc_strong_count as *const u8,
        );
        symbols.insert(
            "adesh_rt_arc_weak_count".to_string(),
            adesh_rt_arc_weak_count as *const u8,
        );

        Self { symbols }
    }

    /// Register a custom host symbol address.
    pub fn register(&mut self, name: impl Into<String>, ptr: *const u8) {
        self.symbols.insert(name.into(), ptr);
    }
}

impl HostSymbolResolver for StandardHostSymbolResolver {
    fn resolve(&self, symbol: &str) -> Option<*const u8> {
        self.symbols.get(symbol).copied()
    }
}

impl adesh_codegen::jit::HostSymbolResolver for StandardHostSymbolResolver {
    fn resolve_symbol(&self, name: &str) -> Option<usize> {
        self.symbols.get(name).map(|ptr| *ptr as usize)
    }
}

pub extern "C" fn jit_aot_print_cstr(text: *const std::os::raw::c_char, newline: i64) -> u64 {
    if text.is_null() {
        if newline != 0 {
            println!();
        }
        return 0;
    }
    unsafe {
        let c_str = std::ffi::CStr::from_ptr(text);
        let s = c_str.to_string_lossy();
        if newline != 0 {
            println!("{}", s);
        } else {
            print!("{}", s);
            use std::io::Write;
            let _ = std::io::stdout().flush();
        }
    }
    0
}

/// Native-JIT equivalent of the C runtime's `aot_abort_str`.
///
/// Keeping it here avoids forcing the static AOT runtime archive member that
/// exports legacy `adesh_*_to_string` helpers; on Windows that member collides
/// with the Rust runtime crate during the final executable link.
pub extern "C" fn jit_aot_abort_str(message: *const std::os::raw::c_char) -> ! {
    let message = if message.is_null() {
        "native JIT aborted".to_string()
    } else {
        unsafe { std::ffi::CStr::from_ptr(message) }
            .to_string_lossy()
            .into_owned()
    };
    eprintln!("Native JIT runtime error: {message}");
    std::process::exit(1)
}

pub extern "C" fn jit_aot_print_string(handle: u64, newline: i64) -> u64 {
    use std::io::Write;
    let s = if let Some(val) = crate::backends::aot::runtime_bridge::aot_get_value(handle) {
        val.as_string()
    } else if let Some(s) = crate::backends::aot::runtime_bridge::get_string_val(handle) {
        s
    } else if handle > 0x10000 {
        unsafe {
            std::ffi::CStr::from_ptr(handle as *const std::os::raw::c_char)
                .to_string_lossy()
                .into_owned()
        }
    } else {
        crate::backends::aot::runtime_bridge::unpack_aot_arg(handle).as_string()
    };
    if newline != 0 {
        println!("{}", s);
    } else {
        print!("{}", s);
        let _ = std::io::stdout().flush();
    }
    0
}

/// Flatten an array literal containing spread elements. The generated code
/// passes parallel arrays of handles and 0/1 spread flags.
pub unsafe extern "C" fn jit_aot_make_array_spread(
    values: *const u64,
    spread_flags: *const u64,
    count: i64,
) -> u64 {
    use crate::backends::common::builtins::RuntimeValue;

    if values.is_null() || count <= 0 {
        return crate::backends::aot::runtime_bridge::aot_store_value(RuntimeValue::Array(vec![]));
    }
    let count = count as usize;
    let values = unsafe { std::slice::from_raw_parts(values, count) };
    let flags = if spread_flags.is_null() {
        None
    } else {
        Some(unsafe { std::slice::from_raw_parts(spread_flags, count) })
    };

    let mut result = Vec::new();
    for (index, handle) in values.iter().copied().enumerate() {
        let value = crate::backends::aot::runtime_bridge::aot_get_value(handle)
            .unwrap_or(RuntimeValue::Null);
        if flags.is_some_and(|flags| flags[index] != 0) {
            match value {
                RuntimeValue::Array(items)
                | RuntimeValue::Set(items)
                | RuntimeValue::Tuple(items)
                | RuntimeValue::RawArray(_, items) => result.extend(items),
                RuntimeValue::DynArray { data, .. } => result.extend(data),
                other => result.push(other),
            }
        } else {
            result.push(value);
        }
    }
    crate::backends::aot::runtime_bridge::aot_store_value(RuntimeValue::Array(result))
}

/// Unbox primitive results returned by the AOT-bridge method dispatcher.
/// Composite values retain their existing handle so they remain in the same
/// bridge table that owns them.
pub extern "C" fn jit_aot_unbox(handle: u64) -> u64 {
    use crate::backends::common::builtins::RuntimeValue;

    match crate::backends::aot::runtime_bridge::aot_get_value(handle) {
        Some(RuntimeValue::Null) => 0,
        Some(RuntimeValue::Int(value)) | Some(RuntimeValue::I64(value)) => value as u64,
        Some(RuntimeValue::I32(value)) => value as i64 as u64,
        Some(RuntimeValue::I16(value)) => value as i64 as u64,
        Some(RuntimeValue::I8(value)) => value as i64 as u64,
        Some(RuntimeValue::U8(value)) => value as u64,
        Some(RuntimeValue::U16(value)) => value as u64,
        Some(RuntimeValue::U32(value)) => value as u64,
        Some(RuntimeValue::U64(value)) => value,
        Some(RuntimeValue::Bool(value)) => u64::from(value),
        Some(RuntimeValue::Char(value)) => value as u64,
        Some(RuntimeValue::Float(value)) | Some(RuntimeValue::F64(value)) => value.to_bits(),
        Some(RuntimeValue::F32(value)) => value.to_bits() as u64,
        // Strings and composites must remain handles; large integer values
        // are also retained as handles to avoid lossy truncation.
        Some(_) | None => handle,
    }
}

/// `typeof` adapter for code generated by the direct native lowering path.
/// It must read the AOT bridge's handle table, not the standalone runtime's
/// table, because this JIT uses the former for all boxed values.
pub extern "C" fn jit_aot_typeof(handle: u64) -> *const std::os::raw::c_char {
    use crate::backends::common::builtins::RuntimeValue;

    let value = crate::backends::aot::runtime_bridge::unpack_aot_arg(handle);
    let type_name = match value {
        RuntimeValue::Int(_) => "int",
        RuntimeValue::Float(_) => "float",
        RuntimeValue::Bool(_) => "bool",
        RuntimeValue::Char(_) => "char",
        RuntimeValue::String(_) => "string",
        RuntimeValue::Array(_) | RuntimeValue::RawArray(_, _) | RuntimeValue::DynArray { .. } => "array",
        RuntimeValue::Set(_) => "set",
        RuntimeValue::Tuple(_) => "tuple",
        RuntimeValue::Object(_) => "object",
        RuntimeValue::Function(_) => "function",
        RuntimeValue::Promise(_) => "promise",
        RuntimeValue::BigInt(_) => "bigint",
        RuntimeValue::U8(_) => "u8",
        RuntimeValue::U16(_) => "u16",
        RuntimeValue::U32(_) => "u32",
        RuntimeValue::U64(_) => "u64",
        RuntimeValue::U128(_) => "u128",
        RuntimeValue::I8(_) => "i8",
        RuntimeValue::I16(_) => "i16",
        RuntimeValue::I32(_) => "i32",
        RuntimeValue::I64(_) => "i64",
        RuntimeValue::I128(_) => "i128",
        RuntimeValue::F32(_) => "f32",
        RuntimeValue::F64(_) => "f64",
        RuntimeValue::Null => "null",
    };
    std::ffi::CString::new(type_name)
        .unwrap_or_default()
        .into_raw()
}

fn assertion_message(message_handle: u64) -> String {
    if message_handle == 0 {
        String::new()
    } else {
        format!(
            ": {}",
            crate::backends::aot::runtime_bridge::unpack_aot_arg(message_handle).as_string()
        )
    }
}

fn fail_assertion(message: String) -> ! {
    eprintln!("Assertion failed{message}");
    std::process::exit(1)
}

pub extern "C" fn jit_aot_assert(condition_handle: u64, message_handle: u64) -> i64 {
    let condition = crate::backends::aot::runtime_bridge::unpack_aot_arg(condition_handle);
    if !condition.as_bool().unwrap_or(false) {
        fail_assertion(assertion_message(message_handle));
    }
    0
}

pub extern "C" fn jit_aot_assert_eq(left_handle: u64, right_handle: u64, message_handle: u64) -> i64 {
    let left = crate::backends::aot::runtime_bridge::unpack_aot_arg(left_handle);
    let right = crate::backends::aot::runtime_bridge::unpack_aot_arg(right_handle);
    if left != right {
        fail_assertion(format!(
            "{}\n  left: {}\n right: {}",
            assertion_message(message_handle),
            left.as_string(),
            right.as_string(),
        ));
    }
    0
}

pub extern "C" fn jit_aot_assert_ne(left_handle: u64, right_handle: u64, message_handle: u64) -> i64 {
    let left = crate::backends::aot::runtime_bridge::unpack_aot_arg(left_handle);
    let right = crate::backends::aot::runtime_bridge::unpack_aot_arg(right_handle);
    if left == right {
        fail_assertion(format!(
            "{}\n  both: {}",
            assertion_message(message_handle),
            left.as_string(),
        ));
    }
    0
}

pub extern "C" fn jit_aot_sizeof(handle: u64) -> i64 {
    use crate::backends::aot::runtime_bridge::{runtime_value_to_ast_value, unpack_aot_arg};
    use crate::execution::runtime_core::sizeof_value;
    let rv = unpack_aot_arg(handle);
    let ast_val = runtime_value_to_ast_value(&rv);
    sizeof_value(&ast_val) as i64
}

pub extern "C" fn jit_aot_clock() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

pub extern "C" fn jit_aot_input(prompt_handle: u64) -> u64 {
    crate::backends::aot::runtime_bridge::aot_input(prompt_handle, 0)
}

pub extern "C" fn jit_aot_string_concat_cstr(l_h: u64, r_h: u64) -> *const std::os::raw::c_char {
    let l_val = crate::backends::aot::runtime_bridge::unpack_aot_arg(l_h);
    let r_val = crate::backends::aot::runtime_bridge::unpack_aot_arg(r_h);
    let combined = format!("{}{}", l_val.as_string(), r_val.as_string());
    std::ffi::CString::new(combined)
        .unwrap_or_default()
        .into_raw()
}

pub extern "C" fn jit_aot_value_to_string_cstr(handle: u64) -> *const std::os::raw::c_char {
    let val = crate::backends::aot::runtime_bridge::unpack_aot_arg(handle);
    std::ffi::CString::new(val.as_string())
        .unwrap_or_default()
        .into_raw()
}

pub extern "C" fn jit_aot_format_value(
    val_handle: u64,
    spec_cstr: *const std::os::raw::c_char,
) -> *const std::os::raw::c_char {
    let val = crate::backends::aot::runtime_bridge::unpack_aot_arg(val_handle);
    let formatted = if spec_cstr.is_null() {
        val.as_string()
    } else {
        let spec = unsafe { std::ffi::CStr::from_ptr(spec_cstr).to_string_lossy() };
        let ast_val = crate::backends::aot::runtime_bridge::runtime_value_to_ast_value(&val);
        let fmt_val = crate::execution::runtime_core::format::ast_value_to_format_value(&ast_val);
        adesh_format::format_value(&fmt_val, &spec).unwrap_or_else(|_| val.as_string())
    };
    std::ffi::CString::new(formatted)
        .unwrap_or_default()
        .into_raw()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_symbol_resolution() {
        let resolver = StandardHostSymbolResolver::new();
        assert!(resolver.resolve("printf").is_some());
        assert!(resolver.resolve("malloc").is_some());
        assert!(resolver.resolve("aot_print_i64").is_some());
        assert!(resolver.resolve("aot_make_i64").is_some());
        assert!(resolver.resolve("aot_make_bool").is_some());
        assert!(resolver.resolve("aot_release_handle").is_some());
        assert!(resolver.resolve("aot_typeof").is_some());
        assert!(resolver.resolve("aot_assert_eq").is_some());
        assert!(resolver.resolve("adesh_rt_arc_new").is_some());
        assert!(resolver.resolve("non_existent_symbol_123").is_none());
    }
}
