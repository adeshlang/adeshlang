//! Native-pipeline ABI surface (`ADESH_RUNTIME_NATIVE_V1`).
//!
//! Symbols required by `src/backends/native/lower.rs` that previously did not
//! exist at all (every program touching them failed at link time):
//! string concat, range construction, `in` membership, dict construction,
//! raw allocation, wall-clock time, the f64 math dispatcher, and the
//! abort-with-message helper.
//!
//! The print-only native entry points live in `native_output.c`, a separate
//! archive member so simple programs do not extract this broader Rust runtime.

use std::os::raw::c_char;

use crate::{RuntimeValue, aot_store_value, aot_value_as_string, get_string_val, unpack_aot_arg};

/// Length of a NUL-terminated byte string used by native runtime metadata.
unsafe fn cstr_bytes(ptr: *const c_char) -> &'static [u8] {
    if ptr.is_null() {
        return &[];
    }
    let start = ptr.cast::<u8>();
    let mut end = start;
    unsafe {
        while *end != 0 {
            end = end.add(1);
        }
    }
    unsafe { core::slice::from_raw_parts(start, end.offset_from(start) as usize) }
}

// ============================================================================
// Time
// ============================================================================

/// Wall-clock time in seconds since the Unix epoch, returned as f64 in XMM0.
/// Raw OS calls: no `std::time` machinery gets linked.
#[unsafe(no_mangle)]
pub extern "C" fn aot_clock() -> f64 {
    #[cfg(windows)]
    {
        #[repr(C)]
        #[derive(Default)]
        struct FileTime {
            lo: u32,
            hi: u32,
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetSystemTimeAsFileTime(lpSystemTimeAsFileTime: *mut FileTime) -> ();
        }
        let mut ft = FileTime::default();
        unsafe { GetSystemTimeAsFileTime(&mut ft) };
        let ticks = ((ft.hi as u64) << 32) | ft.lo as u64;
        // 100ns intervals since 1601-01-01 → seconds since 1970-01-01.
        ((ticks as i64 - 11_644_473_600_000_000) as f64) / 1e7
    }
    #[cfg(not(windows))]
    {
        #[repr(C)]
        struct TimeVal {
            sec: i64,
            usec: i64,
        }
        #[link(name = "c")]
        unsafe extern "C" {
            fn gettimeofday(tv: *mut TimeVal, tz: *mut core::ffi::c_void) -> i32;
        }
        let mut tv = TimeVal { sec: 0, usec: 0 };
        unsafe { gettimeofday(&mut tv, core::ptr::null_mut()) };
        tv.sec as f64 + (tv.usec as f64) / 1e6
    }
}

// ============================================================================
// f64 math dispatcher
// ============================================================================

/// Dispatcher for float builtins: `aot_math_f64(op, x, y) -> f64` (XMM0).
/// Opcodes match `src/backends/native/lower.rs`:
/// 1 sin, 2 cos, 3 tan, 4 asin, 5 acos, 6 atan, 7 sqrt, 8 exp, 9 log,
/// 10 log10, 11 fabs, 12 floor, 13 ceil, 14 round, 16 pow(x, y).
#[unsafe(no_mangle)]
pub extern "C" fn aot_math_f64(op: i64, x: f64, y: f64) -> f64 {
    match op {
        1 => x.sin(),
        2 => x.cos(),
        3 => x.tan(),
        4 => x.asin(),
        5 => x.acos(),
        6 => x.atan(),
        7 => x.sqrt(),
        8 => x.exp(),
        9 => x.ln(),
        10 => x.log10(),
        11 => x.abs(),
        12 => x.floor(),
        13 => x.ceil(),
        14 => x.round(),
        16 => x.powf(y),
        _ => f64::NAN,
    }
}

// ============================================================================
// Strings / ranges / dicts / membership
// ============================================================================

fn value_as_string(handle: u64) -> String {
    if let Some(text) = aot_value_as_string(handle) {
        return text;
    }
    if handle >= crate::HANDLE_TAG {
        return handle.to_string();
    }
    if let Some(text) = crate::try_cstr_abi_string(handle) {
        return text;
    }
    if handle == 0 {
        "null".to_string()
    } else {
        (handle as i64).to_string()
    }
}

fn allocate_c_string(text: &str) -> *mut c_char {
    let mut bytes = Vec::with_capacity(text.len().saturating_add(1));
    for byte in text.bytes() {
        if byte == 0 {
            bytes.extend_from_slice(b"\\0");
        } else {
            bytes.push(byte);
        }
    }
    bytes.push(0);
    unsafe {
        let ptr = crate::wasm_alloc::raw_malloc(bytes.len()) as *mut c_char;
        if !ptr.is_null() {
            std::ptr::copy_nonoverlapping(bytes.as_ptr().cast::<c_char>(), ptr, bytes.len());
        }
        ptr
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_value_to_string_cstr(value: u64) -> *mut c_char {
    crate::ffi_guard(core::ptr::null_mut(), || {
        allocate_c_string(&value_as_string(value))
    })
}

/// Concatenate two values as strings (the `+` operator on strings).
#[unsafe(no_mangle)]
pub extern "C" fn aot_string_concat(a_handle: u64, b_handle: u64) -> u64 {
    crate::ffi_guard(0, || {
        let a = value_as_string(a_handle);
        let b = value_as_string(b_handle);
        let mut combined = String::with_capacity(a.len().saturating_add(b.len()));
        combined.push_str(&a);
        combined.push_str(&b);
        aot_store_value(RuntimeValue::String(combined))
    })
}

/// Concatenate two string pointers / handles and return a null-terminated C string pointer.
#[unsafe(no_mangle)]
pub extern "C" fn aot_string_concat_cstr(left: u64, right: u64) -> *mut std::os::raw::c_char {
    crate::ffi_guard(core::ptr::null_mut(), || {
        let left_str = value_as_string(left);
        let right_str = value_as_string(right);
        let mut combined = String::with_capacity(
            left_str
                .len()
                .saturating_add(right_str.len())
                .saturating_add(1),
        );
        combined.push_str(&left_str);
        combined.push_str(&right_str);
        allocate_c_string(&combined)
    })
}

fn runtime_value_to_format_value(v: &RuntimeValue) -> adesh_format::FormatValue {
    match v {
        RuntimeValue::Null => adesh_format::FormatValue::Null,
        RuntimeValue::Bool(b) => adesh_format::FormatValue::Bool(*b),
        RuntimeValue::Int(n) | RuntimeValue::I64(n) => adesh_format::FormatValue::Int(*n as i128),
        RuntimeValue::I32(n) => adesh_format::FormatValue::Int(*n as i128),
        RuntimeValue::I16(n) => adesh_format::FormatValue::Int(*n as i128),
        RuntimeValue::I8(n) => adesh_format::FormatValue::Int(*n as i128),
        RuntimeValue::I128(n) => adesh_format::FormatValue::Int(*n),
        RuntimeValue::U64(n) => adesh_format::FormatValue::UInt(*n as u128),
        RuntimeValue::U32(n) => adesh_format::FormatValue::UInt(*n as u128),
        RuntimeValue::U16(n) => adesh_format::FormatValue::UInt(*n as u128),
        RuntimeValue::U8(n) => adesh_format::FormatValue::UInt(*n as u128),
        RuntimeValue::U128(n) => adesh_format::FormatValue::UInt(*n),
        RuntimeValue::Float(f) | RuntimeValue::F64(f) => adesh_format::FormatValue::Number(*f),
        RuntimeValue::F32(f) => adesh_format::FormatValue::Number(*f as f64),
        RuntimeValue::Char(c) => adesh_format::FormatValue::Char(*c),
        RuntimeValue::String(s) => adesh_format::FormatValue::Str(s.clone()),
        RuntimeValue::Array(arr) => adesh_format::FormatValue::Array(
            arr.iter().map(runtime_value_to_format_value).collect(),
        ),
        RuntimeValue::Tuple(tup) => adesh_format::FormatValue::Tuple(
            tup.iter().map(runtime_value_to_format_value).collect(),
        ),
        RuntimeValue::Object(map) => adesh_format::FormatValue::Object(
            map.iter()
                .map(|(k, val)| (k.clone(), runtime_value_to_format_value(val)))
                .collect(),
        ),
        other => adesh_format::FormatValue::Other(other.as_string()),
    }
}

fn get_runtime_value_or_cstr(handle: u64) -> RuntimeValue {
    // Delegate to `unpack_aot_arg`: handle lookup first, then the provenance-safe
    // CStr heuristic (never dereference raw small-integer words).
    unpack_aot_arg(handle)
}

fn apply_runtime_format_aot(v: &RuntimeValue, spec: &str) -> String {
    if spec.is_empty() {
        return v.as_string();
    }
    let fv = runtime_value_to_format_value(v);
    adesh_format::format_value(&fv, spec).unwrap_or_else(|_| {
        adesh_format::FormatSpec::parse(spec)
            .ok()
            .and_then(|parsed| fv.render(&parsed).ok())
            .unwrap_or_else(|| v.as_string())
    })
}

/// Format a value with a format specifier for AOT native code.
#[unsafe(no_mangle)]
pub extern "C" fn aot_format_value(handle: u64, spec_ptr: *const core::ffi::c_char) -> u64 {
    let spec = if spec_ptr.is_null() {
        ""
    } else {
        unsafe { core::ffi::CStr::from_ptr(spec_ptr) }
            .to_str()
            .unwrap_or("")
    };
    let val = get_runtime_value_or_cstr(handle);
    let formatted = apply_runtime_format_aot(&val, spec);
    aot_store_value(RuntimeValue::String(formatted))
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_format_value_str(
    handle: u64,
    spec_ptr: *const core::ffi::c_char,
) -> *mut core::ffi::c_char {
    let spec = if spec_ptr.is_null() {
        ""
    } else {
        unsafe { core::ffi::CStr::from_ptr(spec_ptr) }
            .to_str()
            .unwrap_or("")
    };
    let val = get_runtime_value_or_cstr(handle);
    let formatted = apply_runtime_format_aot(&val, spec);
    let c_str = match std::ffi::CString::new(formatted.clone()) {
        Ok(c) => c,
        Err(_) => std::ffi::CString::new(formatted.replace('\0', "\\0"))
            .unwrap_or_else(|_| std::ffi::CString::new("").unwrap()),
    };
    let bytes = c_str.as_bytes_with_nul();
    unsafe {
        let ptr = crate::wasm_alloc::raw_malloc(bytes.len()) as *mut core::ffi::c_char;
        if !ptr.is_null() {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr() as *const core::ffi::c_char,
                ptr,
                bytes.len(),
            );
        }
        ptr
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_make_error(msg_handle_or_ptr: u64) -> u64 {
    crate::ffi_guard(0, || {
        let msg = get_string_val(msg_handle_or_ptr)
            .or_else(|| {
                if let Some(RuntimeValue::String(s)) = crate::aot_get_value(msg_handle_or_ptr) {
                    Some(s)
                } else {
                    crate::try_cstr_abi_string(msg_handle_or_ptr)
                }
            })
            .unwrap_or_else(|| "Error".to_string());
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            crate::CLASS_TAG_KEY.to_string(),
            RuntimeValue::String("Error".to_string()),
        );
        map.insert("message".to_string(), RuntimeValue::String(msg));
        aot_store_value(RuntimeValue::Object(map))
    })
}

// ============================================================================
// Native closures
// ============================================================================

// Native-backend function values are heap closures `[fn_ptr, captures...]`
// invoked with the closure pointer in R10. Generated code registers each one
// so the runtime can tell them apart from raw function pointers (and from C
// strings when boxing an untyped word).
static NATIVE_CLOSURES: crate::RuntimeMutex<std::collections::BTreeSet<u64>> =
    crate::RuntimeMutex::new(std::collections::BTreeSet::new());

#[unsafe(no_mangle)]
pub extern "C" fn aot_register_closure(closure: u64) -> u64 {
    if closure != 0 {
        NATIVE_CLOSURES.lock().insert(closure);
    }
    closure
}

pub fn unregister_native_closure(closure: u64) {
    if closure != 0 {
        NATIVE_CLOSURES.lock().remove(&closure);
    }
}

pub fn is_native_closure(ptr: u64) -> bool {
    ptr != 0 && NATIVE_CLOSURES.lock().contains(&ptr)
}

/// Call a function value (native closure or raw function pointer) with up
/// to four integer-register arguments.
pub fn call_callable(fp: usize, args: &[i64]) -> u64 {
    let mut a = [0u64; 4];
    for (slot, v) in a.iter_mut().zip(args) {
        *slot = *v as u64;
    }
    if is_native_closure(fp as u64) {
        // SAFETY: registered closures start with the address of a generated
        // function using the platform C ABI plus R10 as the closure pointer.
        return unsafe { call_native_closure(fp as u64, a) };
    }
    // SAFETY: a raw function value is the address of a C-ABI function taking
    // integer arguments; extra argument registers are ignored by the callee.
    let f = unsafe { std::mem::transmute::<usize, extern "C" fn(u64, u64, u64, u64) -> u64>(fp) };
    f(a[0], a[1], a[2], a[3])
}

#[cfg(all(target_arch = "x86_64", windows))]
unsafe fn call_native_closure(closure: u64, a: [u64; 4]) -> u64 {
    let f = unsafe { *(closure as *const u64) };
    let ret: u64;
    // R12 (callee-saved) keeps the caller's RSP across the aligned call with
    // its 32-byte shadow space.
    unsafe {
        core::arch::asm!(
            "mov r12, rsp",
            "and rsp, -16",
            "sub rsp, 32",
            "call {f}",
            "mov rsp, r12",
            f = in(reg) f,
            in("r10") closure,
            in("rcx") a[0],
            in("rdx") a[1],
            in("r8") a[2],
            in("r9") a[3],
            out("r12") _,
            lateout("rax") ret,
            clobber_abi("win64"),
        );
    }
    ret
}

#[cfg(all(target_arch = "x86_64", not(windows)))]
unsafe fn call_native_closure(closure: u64, a: [u64; 4]) -> u64 {
    let f = unsafe { *(closure as *const u64) };
    let ret: u64;
    unsafe {
        core::arch::asm!(
            "mov r12, rsp",
            "and rsp, -16",
            "call {f}",
            "mov rsp, r12",
            f = in(reg) f,
            in("r10") closure,
            in("rdi") a[0],
            in("rsi") a[1],
            in("rdx") a[2],
            in("rcx") a[3],
            out("r12") _,
            lateout("rax") ret,
            clobber_abi("sysv64"),
        );
    }
    ret
}

#[cfg(not(target_arch = "x86_64"))]
unsafe fn call_native_closure(closure: u64, a: [u64; 4]) -> u64 {
    // Without a closure-register trampoline only capture-free closures work.
    // Read the code pointer as `usize` so this compiles on 32-bit targets
    // where function pointers are 4 bytes (transmuting from `u64` is E0512).
    let code = unsafe { *(closure as *const usize) };
    let f = unsafe { std::mem::transmute::<usize, extern "C" fn(u64, u64, u64, u64) -> u64>(code) };
    f(a[0], a[1], a[2], a[3])
}

// ============================================================================
// Class tags (instanceof)
// ============================================================================

static CLASS_PARENTS: crate::RuntimeMutex<std::collections::BTreeMap<String, String>> =
    crate::RuntimeMutex::new(std::collections::BTreeMap::new());

fn cstr_to_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let bytes = unsafe { cstr_bytes(ptr) };
    core::str::from_utf8(bytes).ok().map(str::to_string)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_register_class(name: *const c_char, parent: *const c_char) -> i64 {
    if let (Some(name), Some(parent)) = (cstr_to_string(name), cstr_to_string(parent)) {
        CLASS_PARENTS.lock().insert(name, parent);
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_set_class_tag(obj: u64, name: *const c_char) -> u64 {
    if let Some(name) = cstr_to_string(name)
        && let Some(RuntimeValue::Object(mut map)) = crate::aot_get_value(obj)
    {
        map.insert(crate::CLASS_TAG_KEY.to_string(), RuntimeValue::String(name));
        crate::aot_update_value(obj, RuntimeValue::Object(map));
    }
    obj
}

/// `obj instanceof class`: `class` is a class-name C string or string handle.
#[unsafe(no_mangle)]
pub extern "C" fn aot_instanceof(obj: u64, class: u64) -> i64 {
    let RuntimeValue::String(target) = get_runtime_value_or_cstr(class) else {
        return 0;
    };
    let mut current = match crate::aot_get_value(obj) {
        Some(RuntimeValue::Object(map)) => match map.get(crate::CLASS_TAG_KEY) {
            Some(RuntimeValue::String(tag)) => Some(tag.clone()),
            _ => None,
        },
        _ => None,
    };
    let parents = CLASS_PARENTS.lock();
    let mut depth = 0;
    while let Some(name) = current {
        if name == target {
            return 1;
        }
        depth += 1;
        if depth > 256 {
            break;
        }
        current = parents.get(&name).cloned();
    }
    0
}

fn aot_rt_val_to_bool(rv: &RuntimeValue) -> bool {
    match rv {
        RuntimeValue::Bool(b) => *b,
        RuntimeValue::Int(n) | RuntimeValue::I64(n) => *n != 0,
        RuntimeValue::I32(n) => *n != 0,
        RuntimeValue::I16(n) => *n != 0,
        RuntimeValue::I8(n) => *n != 0,
        RuntimeValue::U64(n) => *n != 0,
        RuntimeValue::U32(n) => *n != 0,
        RuntimeValue::U16(n) => *n != 0,
        RuntimeValue::U8(n) => *n != 0,
        RuntimeValue::Float(f) | RuntimeValue::F64(f) => *f != 0.0,
        RuntimeValue::F32(f) => *f != 0.0,
        RuntimeValue::String(s) => !s.is_empty(),
        RuntimeValue::Null => false,
        _ => true,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_assert(cond_handle: u64, msg_handle: u64) -> i64 {
    let cond = crate::aot_get_value(cond_handle).unwrap_or_else(|| unpack_aot_arg(cond_handle));
    if !aot_rt_val_to_bool(&cond) {
        let msg = if msg_handle != 0 {
            let m = crate::aot_get_value(msg_handle).unwrap_or_else(|| unpack_aot_arg(msg_handle));
            m.as_string()
        } else {
            "condition is false".to_string()
        };
        eprintln!("Assertion failed: {}", msg);
        std::process::exit(1);
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_assert_eq(left_handle: u64, right_handle: u64, msg_handle: u64) -> i64 {
    let left = crate::aot_get_value(left_handle).unwrap_or_else(|| unpack_aot_arg(left_handle));
    let right = crate::aot_get_value(right_handle).unwrap_or_else(|| unpack_aot_arg(right_handle));
    if left != right {
        let str_l = left.as_string();
        let str_r = right.as_string();
        let custom_msg = if msg_handle != 0 {
            let m = crate::aot_get_value(msg_handle).unwrap_or_else(|| unpack_aot_arg(msg_handle));
            format!(": {}", m.as_string())
        } else {
            String::new()
        };
        eprintln!(
            "Assertion failed: left == right{}\n  left: {}\n right: {}",
            custom_msg, str_l, str_r
        );
        std::process::exit(1);
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn cbrt(x: f64) -> f64 {
    x.cbrt()
}

#[unsafe(no_mangle)]
pub extern "C" fn cbrtf(x: f32) -> f32 {
    x.cbrt()
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_assert_ne(left_handle: u64, right_handle: u64, msg_handle: u64) -> i64 {
    let left = crate::aot_get_value(left_handle).unwrap_or_else(|| unpack_aot_arg(left_handle));
    let right = crate::aot_get_value(right_handle).unwrap_or_else(|| unpack_aot_arg(right_handle));
    if left == right {
        let str_l = left.as_string();
        let str_r = right.as_string();
        let custom_msg = if msg_handle != 0 {
            let m = crate::aot_get_value(msg_handle).unwrap_or_else(|| unpack_aot_arg(msg_handle));
            format!(": {}", m.as_string())
        } else {
            String::new()
        };
        eprintln!(
            "Assertion failed: left != right{}\n  left: {}\n right: {}",
            custom_msg, str_l, str_r
        );
        std::process::exit(1);
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_value_eq(left_handle: u64, right_handle: u64) -> i64 {
    if left_handle == right_handle {
        return 1;
    }
    let left = crate::aot_get_value(left_handle).unwrap_or_else(|| unpack_aot_arg(left_handle));
    let right = crate::aot_get_value(right_handle).unwrap_or_else(|| unpack_aot_arg(right_handle));
    if left == right { 1 } else { 0 }
}

const MAX_MATERIALIZED_COLLECTION_LEN: usize = 16_777_216;

/// Materialize `start..end` (inclusive == 0) or `start..=end` (inclusive
/// != 0) as an array of integers, matching the interpreter/Cranelift backend
/// range semantics. Descending ranges count down.
#[unsafe(no_mangle)]
pub extern "C" fn aot_make_range(start: i64, end: i64, inclusive: i64) -> u64 {
    crate::ffi_guard(0, || {
        let mut items = Vec::new();
        let is_inc = inclusive != 0;
        let count_u128: u128 = if start <= end {
            let diff = (end as i128 - start as i128) as u128;
            if is_inc { diff + 1 } else { diff }
        } else {
            let diff = (start as i128 - end as i128) as u128;
            if is_inc { diff + 1 } else { diff }
        };
        let count = count_u128.min(MAX_MATERIALIZED_COLLECTION_LEN as u128) as usize;
        items.reserve(count);
        if start <= end {
            for offset in 0..count {
                items.push(RuntimeValue::Int(start.wrapping_add(offset as i64)));
            }
        } else {
            for offset in 0..count {
                items.push(RuntimeValue::Int(start.wrapping_sub(offset as i64)));
            }
        }
        aot_store_value(RuntimeValue::Array(items))
    })
}

/// Create an empty dict; entries are added via `aot_set_index`.
#[unsafe(no_mangle)]
pub extern "C" fn aot_make_dict() -> u64 {
    crate::ffi_guard(0, || {
        aot_store_value(RuntimeValue::Object(std::collections::BTreeMap::new()))
    })
}

/// `item in container` membership test. Returns a raw 0/1 (never a handle).
/// Arrays/tuples/sets test element membership, strings test substring
/// containment, objects/dicts test key presence.
#[unsafe(no_mangle)]
pub extern "C" fn aot_contains(container_handle: u64, item_handle: u64) -> i64 {
    crate::ffi_guard(0, || {
        let container = crate::aot_get_value(container_handle)
            .unwrap_or_else(|| unpack_aot_arg(container_handle));
        let item = crate::aot_get_value(item_handle).unwrap_or_else(|| unpack_aot_arg(item_handle));
        match container {
            RuntimeValue::Array(v) => v.contains(&item) as i64,
            RuntimeValue::Tuple(v) => v.contains(&item) as i64,
            RuntimeValue::Set(v) => v.contains(&item) as i64,
            RuntimeValue::String(s) => s.contains(&item.as_string()) as i64,
            RuntimeValue::Object(obj) => obj.contains_key(&item.as_string()) as i64,
            RuntimeValue::BTreeMap(map) => map.contains_key(&item.as_string()) as i64,
            RuntimeValue::HashSet(set) => set.contains(&item.as_string()) as i64,
            _ => 0,
        }
    })
}

// ============================================================================
// Dynamic array initialisation  (`let a: [u8; n]` / `[elem; count]`)
// ============================================================================

/// Create a new `RuntimeValue::Array` containing `count` copies of the value
/// referenced by `elem_handle`.  Used by the `__array_init` / `__array_repeat`
/// builtins in the AOT backend.
///
/// * `elem_handle` – a handle returned by `aot_store_value`, or a raw inline
///   integer (passed through `unpack_aot_arg`).
/// * `count`       – number of elements to produce; clamped to 0 if negative.
///
/// Returns a fresh handle to the new array.
#[unsafe(no_mangle)]
pub extern "C" fn aot_array_init(elem_handle: u64, count: i64) -> u64 {
    crate::ffi_guard(0, || {
        let elem = crate::aot_get_value(elem_handle).unwrap_or_else(|| unpack_aot_arg(elem_handle));
        let count_usize = if count <= 0 {
            0
        } else {
            (count as usize).min(MAX_MATERIALIZED_COLLECTION_LEN)
        };
        let data = vec![elem; count_usize];
        crate::aot_store_value(RuntimeValue::Array(data))
    })
}

/// Alias for `aot_array_init` used by the `__array_repeat` builtin.
#[unsafe(no_mangle)]
pub extern "C" fn aot_array_repeat(elem_handle: u64, count: i64) -> u64 {
    aot_array_init(elem_handle, count)
}

// ============================================================================
// Raw allocation
// ============================================================================

/// Raw byte allocation for `alloc<T>(size)`. Returns a heap pointer.
#[unsafe(no_mangle)]
pub extern "C" fn aot_alloc(size: i64) -> *mut core::ffi::c_void {
    crate::ffi_guard(core::ptr::null_mut(), || {
        if size <= 0 {
            return core::ptr::null_mut();
        }
        unsafe { crate::wasm_alloc::raw_malloc(size as usize) }
    })
}

/// Free a pointer previously returned by `aot_alloc`.
/// If the pointer is tracked in `TRACKED_ALLOCATIONS`, removes it safely via
/// `adesh_rt_free_tracked` so `scope_exit` can never double-free it.
#[unsafe(no_mangle)]
pub extern "C" fn aot_free(ptr: *mut core::ffi::c_void) {
    if ptr.is_null() {
        return;
    }
    if crate::adesh_rt_validate_ptr(ptr as i64, 0) == 1 {
        let _ = crate::adesh_rt_free_tracked(ptr as i64, 0);
        return;
    }
    unsafe { crate::wasm_alloc::raw_free(ptr) }
}

/// Release a runtime handle from the global handle table.
#[unsafe(no_mangle)]
pub extern "C" fn aot_release_handle(handle: u64) {
    if handle > 0 {
        crate::aot_remove_value(handle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_free_roundtrip_across_sizes() {
        for size in [1i64, 8, 24, 256, 4096, 1 << 20] {
            let p = aot_alloc(size) as *mut u8;
            assert!(!p.is_null(), "size {size}");
            unsafe {
                p.write(0xAB);
                p.add(size as usize - 1).write(0xCD);
            }
            aot_free(p as *mut core::ffi::c_void);
        }
        assert!(aot_alloc(0).is_null());
        assert!(aot_alloc(-5).is_null());
        aot_free(core::ptr::null_mut());
    }
}

// ============================================================================
// Phase 19.7 Native Standard Library Runtime Primitives (`ADESH_STD_ABI_V1`)
// ============================================================================

/// Returns the runtime's native stdlib ABI version (1).
#[unsafe(no_mangle)]
pub extern "C" fn aot_std_abi_version() -> i64 {
    1
}

/// Write a string handle or C-string pointer to stderr (with optional newline)
/// and flush stderr. Returns 0 on success, -1 on write/flush failure.
#[unsafe(no_mangle)]
pub extern "C" fn aot_eprint_string(value: u64, newline: i64) -> i64 {
    use std::io::Write;
    crate::ffi_guard(-1, || {
        let text = if value == 0 {
            if newline != 0 { String::new() } else { "null".to_string() }
        } else {
            value_as_string(value)
        };
        let mut err = std::io::stderr().lock();
        let res = if newline != 0 {
            writeln!(err, "{}", text)
        } else {
            write!(err, "{}", text)
        };
        if res.is_err() || err.flush().is_err() {
            -1
        } else {
            0
        }
    })
}

/// Explicitly flush stdout (stream=1) or stderr (stream=2). Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn aot_io_flush(stream: i64) -> i64 {
    use std::io::Write;
    crate::ffi_guard(-1, || match stream {
        2 => {
            if std::io::stderr().flush().is_ok() {
                0
            } else {
                -1
            }
        }
        _ => {
            if std::io::stdout().flush().is_ok() {
                0
            } else {
                -1
            }
        }
    })
}

/// UTF-8 boundary-checked and bounds-checked string slice `s[start..end]`.
/// Records an exception on `CURRENT_EXCEPTIONS` and returns `Null` handle if
/// `start`/`end` are out of bounds or do not fall on UTF-8 char boundaries.
#[unsafe(no_mangle)]
pub extern "C" fn aot_str_slice_utf8(s_handle: u64, start: i64, end: i64) -> u64 {
    crate::ffi_guard(0, || {
        let s = value_as_string(s_handle);
        if start < 0 || end < start {
            crate::with_current_exception(|exc| {
                *exc = Some(RuntimeValue::String(format!(
                    "string slice bounds error: invalid range {}..{} for len {}",
                    start,
                    end,
                    s.len()
                )));
            });
            return aot_store_value(RuntimeValue::Null);
        }
        let start_u = start as usize;
        let end_u = end as usize;
        if end_u > s.len() || !s.is_char_boundary(start_u) || !s.is_char_boundary(end_u) {
            crate::with_current_exception(|exc| {
                *exc = Some(RuntimeValue::String(format!(
                    "string slice UTF-8 boundary error: range {}..{} is not a valid UTF-8 boundary in string of {} bytes",
                    start_u,
                    end_u,
                    s.len()
                )));
            });
            return aot_store_value(RuntimeValue::Null);
        }
        aot_store_value(RuntimeValue::String(s[start_u..end_u].to_string()))
    })
}

/// Bounds-checked Unicode scalar character access at index `idx`.
#[unsafe(no_mangle)]
pub extern "C" fn aot_str_char_at(s_handle: u64, idx: i64) -> u64 {
    crate::ffi_guard(0, || {
        let s = value_as_string(s_handle);
        if idx < 0 {
            crate::with_current_exception(|exc| {
                *exc = Some(RuntimeValue::String(format!(
                    "string index out of bounds: {}",
                    idx
                )));
            });
            return aot_store_value(RuntimeValue::Null);
        }
        match s.chars().nth(idx as usize) {
            Some(ch) => aot_store_value(RuntimeValue::String(ch.to_string())),
            None => {
                crate::with_current_exception(|exc| {
                    *exc = Some(RuntimeValue::String(format!(
                        "string index {} out of bounds for {} chars",
                        idx,
                        s.chars().count()
                    )));
                });
                aot_store_value(RuntimeValue::Null)
            }
        }
    })
}

/// Byte length of a string value as `i64`.
#[unsafe(no_mangle)]
pub extern "C" fn aot_str_len(s_handle: u64) -> i64 {
    crate::ffi_guard(0, || value_as_string(s_handle).len() as i64)
}

/// Lexicographical comparison of two strings: returns -1, 0, or 1.
#[unsafe(no_mangle)]
pub extern "C" fn aot_str_compare(a_handle: u64, b_handle: u64) -> i64 {
    crate::ffi_guard(0, || {
        let a = value_as_string(a_handle);
        let b = value_as_string(b_handle);
        match a.cmp(&b) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        }
    })
}

/// Byte offset of first occurrence of `needle` in `haystack`, or -1 if absent.
#[unsafe(no_mangle)]
pub extern "C" fn aot_str_index_of(haystack: u64, needle: u64) -> i64 {
    crate::ffi_guard(-1, || {
        let h = value_as_string(haystack);
        let n = value_as_string(needle);
        h.find(&n).map(|idx| idx as i64).unwrap_or(-1)
    })
}

/// Create an empty vector (`RuntimeValue::Array(Vec::new())`).
#[unsafe(no_mangle)]
pub extern "C" fn aot_col_vec_new() -> u64 {
    crate::ffi_guard(0, || aot_store_value(RuntimeValue::Array(Vec::new())))
}

/// Push `val_handle` onto vector `vec_handle`; returns new length or -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn aot_col_vec_push(vec_handle: u64, val_handle: u64) -> i64 {
    crate::ffi_guard(-1, || {
        let elem = crate::aot_get_value(val_handle).unwrap_or_else(|| unpack_aot_arg(val_handle));
        let mut guard = crate::lock_shard(vec_handle);
        match guard.get_mut(&vec_handle) {
            Some(RuntimeValue::Array(items)) => {
                items.push(elem);
                items.len() as i64
            }
            _ => -1,
        }
    })
}

/// Pop the last element from vector `vec_handle`, or return `Null` handle if empty.
#[unsafe(no_mangle)]
pub extern "C" fn aot_col_vec_pop(vec_handle: u64) -> u64 {
    crate::ffi_guard(0, || {
        let popped = {
            let mut guard = crate::lock_shard(vec_handle);
            match guard.get_mut(&vec_handle) {
                Some(RuntimeValue::Array(items)) => items.pop(),
                _ => None,
            }
        };
        match popped {
            Some(v) => aot_store_value(v),
            None => aot_store_value(RuntimeValue::Null),
        }
    })
}

/// Bounds-checked vector element get.
#[unsafe(no_mangle)]
pub extern "C" fn aot_col_vec_get(vec_handle: u64, index: i64) -> u64 {
    crate::ffi_guard(0, || {
        if index < 0 {
            return aot_store_value(RuntimeValue::Null);
        }
        let item = {
            let guard = crate::lock_shard(vec_handle);
            match guard.get(&vec_handle) {
                Some(RuntimeValue::Array(items)) => items.get(index as usize).cloned(),
                _ => None,
            }
        };
        match item {
            Some(v) => aot_store_value(v),
            None => aot_store_value(RuntimeValue::Null),
        }
    })
}

/// Bounds-checked vector element set; returns 1 on success, 0 on out-of-bounds.
#[unsafe(no_mangle)]
pub extern "C" fn aot_col_vec_set(vec_handle: u64, index: i64, val_handle: u64) -> i64 {
    crate::ffi_guard(0, || {
        if index < 0 {
            return 0;
        }
        let elem = crate::aot_get_value(val_handle).unwrap_or_else(|| unpack_aot_arg(val_handle));
        let mut guard = crate::lock_shard(vec_handle);
        match guard.get_mut(&vec_handle) {
            Some(RuntimeValue::Array(items)) => {
                if let Some(slot) = items.get_mut(index as usize) {
                    *slot = elem;
                    1
                } else {
                    0
                }
            }
            _ => 0,
        }
    })
}

/// Insert `(key, val)` into a dict/map handle; returns 1 if inserted/updated, 0 on error.
#[unsafe(no_mangle)]
pub extern "C" fn aot_col_map_set(map_handle: u64, key_handle: u64, val_handle: u64) -> i64 {
    crate::ffi_guard(0, || {
        let key = value_as_string(key_handle);
        let val = crate::aot_get_value(val_handle).unwrap_or_else(|| unpack_aot_arg(val_handle));
        let mut guard = crate::lock_shard(map_handle);
        match guard.get_mut(&map_handle) {
            Some(RuntimeValue::Object(m)) | Some(RuntimeValue::BTreeMap(m)) => {
                m.insert(key, val);
                1
            }
            _ => 0,
        }
    })
}

/// Get value for `key` from dict/map handle, or `Null` handle if missing.
#[unsafe(no_mangle)]
pub extern "C" fn aot_col_map_get(map_handle: u64, key_handle: u64) -> u64 {
    crate::ffi_guard(0, || {
        let key = value_as_string(key_handle);
        let val = {
            let guard = crate::lock_shard(map_handle);
            match guard.get(&map_handle) {
                Some(RuntimeValue::Object(m)) | Some(RuntimeValue::BTreeMap(m)) => {
                    m.get(&key).cloned()
                }
                _ => None,
            }
        };
        match val {
            Some(v) => aot_store_value(v),
            None => aot_store_value(RuntimeValue::Null),
        }
    })
}

/// Remove `key` from dict/map handle; returns 1 if present, 0 otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn aot_col_map_remove(map_handle: u64, key_handle: u64) -> i64 {
    crate::ffi_guard(0, || {
        let key = value_as_string(key_handle);
        let mut guard = crate::lock_shard(map_handle);
        match guard.get_mut(&map_handle) {
            Some(RuntimeValue::Object(m)) | Some(RuntimeValue::BTreeMap(m)) => {
                if m.remove(&key).is_some() { 1 } else { 0 }
            }
            _ => 0,
        }
    })
}

/// Create an empty set (`RuntimeValue::Set(Vec::new())`).
#[unsafe(no_mangle)]
pub extern "C" fn aot_col_set_new() -> u64 {
    crate::ffi_guard(0, || aot_store_value(RuntimeValue::Set(Vec::new())))
}

/// Add `item_handle` to set `set_handle`; returns 1 if newly inserted, 0 if already present.
#[unsafe(no_mangle)]
pub extern "C" fn aot_col_set_add(set_handle: u64, item_handle: u64) -> i64 {
    crate::ffi_guard(0, || {
        let item = crate::aot_get_value(item_handle).unwrap_or_else(|| unpack_aot_arg(item_handle));
        let mut guard = crate::lock_shard(set_handle);
        match guard.get_mut(&set_handle) {
            Some(RuntimeValue::Set(items)) => {
                if items.contains(&item) {
                    0
                } else {
                    items.push(item);
                    1
                }
            }
            Some(RuntimeValue::HashSet(items)) => {
                if items.insert(item.as_string()) { 1 } else { 0 }
            }
            _ => 0,
        }
    })
}

/// Remove `item_handle` from set `set_handle`; returns 1 if removed, 0 otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn aot_col_set_remove(set_handle: u64, item_handle: u64) -> i64 {
    crate::ffi_guard(0, || {
        let item = crate::aot_get_value(item_handle).unwrap_or_else(|| unpack_aot_arg(item_handle));
        let mut guard = crate::lock_shard(set_handle);
        match guard.get_mut(&set_handle) {
            Some(RuntimeValue::Set(items)) => {
                if let Some(pos) = items.iter().position(|x| x == &item) {
                    items.remove(pos);
                    1
                } else {
                    0
                }
            }
            Some(RuntimeValue::HashSet(items)) => {
                if items.remove(&item.as_string()) { 1 } else { 0 }
            }
            _ => 0,
        }
    })
}

/// Join two path segments cleanly into a string handle.
#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_path_join2(a_val: u64, b_val: u64) -> u64 {
    crate::ffi_guard(0, || {
        let a = value_as_string(a_val);
        let b = value_as_string(b_val);
        let mut p = std::path::PathBuf::from(a);
        p.push(b);
        aot_store_value(RuntimeValue::String(p.to_string_lossy().into_owned()))
    })
}

/// Lexically normalize a filesystem path (resolving `.` and `..` components).
#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_path_normalize(path_val: u64) -> u64 {
    crate::ffi_guard(0, || {
        let raw = value_as_string(path_val);
        let mut out = std::path::PathBuf::new();
        for comp in std::path::Path::new(&raw).components() {
            match comp {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    if !out.pop() {
                        out.push("..");
                    }
                }
                other => out.push(other.as_os_str()),
            }
        }
        let normalized = if out.as_os_str().is_empty() {
            ".".to_string()
        } else {
            out.to_string_lossy().into_owned()
        };
        aot_store_value(RuntimeValue::String(normalized))
    })
}

/// Read directory entries (sorted names), returning an `Array` handle or `Null` on error.
#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_readdir(path_val: u64) -> u64 {
    crate::ffi_guard(0, || {
        let path = value_as_string(path_val);
        match std::fs::read_dir(&path) {
            Ok(rd) => {
                let mut names: Vec<String> = rd
                    .filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect();
                names.sort();
                let vals = names.into_iter().map(RuntimeValue::String).collect();
                aot_store_value(RuntimeValue::Array(vals))
            }
            Err(_) => aot_store_value(RuntimeValue::Null),
        }
    })
}

/// Return file size in bytes as `i64`, or `-1` on error.
#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_metadata_size(path_val: u64) -> i64 {
    crate::ffi_guard(-1, || {
        let path = value_as_string(path_val);
        match std::fs::metadata(&path) {
            Ok(meta) => i64::try_from(meta.len()).unwrap_or(i64::MAX),
            Err(_) => -1,
        }
    })
}

/// Raw boolean (`0` / `1`) filesystem predicates & mutations for native lowering.
#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_write_bool(path_val: u64, content_val: u64) -> i64 {
    crate::ffi_guard(0, || {
        let path = value_as_string(path_val);
        let content = value_as_string(content_val);
        if std::fs::write(&path, content).is_ok() { 1 } else { 0 }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_exists_bool(path_val: u64) -> i64 {
    crate::ffi_guard(0, || {
        let path = value_as_string(path_val);
        if std::path::Path::new(&path).exists() { 1 } else { 0 }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_is_file_bool(path_val: u64) -> i64 {
    crate::ffi_guard(0, || {
        let path = value_as_string(path_val);
        if std::path::Path::new(&path).is_file() { 1 } else { 0 }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_is_dir_bool(path_val: u64) -> i64 {
    crate::ffi_guard(0, || {
        let path = value_as_string(path_val);
        if std::path::Path::new(&path).is_dir() { 1 } else { 0 }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_mkdir_bool(path_val: u64) -> i64 {
    crate::ffi_guard(0, || {
        let path = value_as_string(path_val);
        if std::fs::create_dir_all(&path).is_ok() { 1 } else { 0 }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_fs_delete_bool(path_val: u64) -> i64 {
    crate::ffi_guard(0, || {
        let path = value_as_string(path_val);
        let p = std::path::Path::new(&path);
        let ok = if p.is_dir() {
            std::fs::remove_dir_all(p).is_ok()
        } else {
            std::fs::remove_file(p).is_ok()
        };
        if ok { 1 } else { 0 }
    })
}

// ============================================================================
// PGO profile dump (Phase 4, `--pgo=generate`)
// ============================================================================

/// One `__pgo_table` entry: `{name_rva, counter_rva}` image RVAs. The table
/// is zero-terminated and emitted by the backend's `emit_object` whenever
/// `pgo_inc` instrumentation is present.
#[repr(C)]
pub struct PgoTableEntry {
    name_rva: u32,
    counter_rva: u32,
}

/// Per-function counters aggregated from the table: the entry-block count
/// (`entry_count`) and per-block execution counts keyed by block id.
#[derive(Default)]
struct PgoFnProfile {
    entry_count: u64,
    blocks: std::collections::BTreeMap<u32, u64>,
}

/// Called by the `__adesh_windows_start` stub after `main` returns, before
/// `ExitProcess`: walks the `__pgo_table` and writes
/// `adesh_pgo_profile.json` (in the process working directory) in exactly
/// the `ProfileData` JSON shape the compiler consumes via
/// `--pgo=use=<path>`.
///
/// Best-effort and panic-isolated (unwinding through the FFI boundary from
/// the synthesized startup stub would be undefined behaviour); failures go
/// to stderr and never alter the program's exit code.
#[unsafe(no_mangle)]
pub extern "C" fn aot_pgo_dump(table_ptr: *const PgoTableEntry) {
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| pgo_dump_impl(table_ptr)));
    if result.is_err() {
        eprintln!("adesh pgo: profile dump failed (instrumented process panicked while dumping)");
    }
}

fn pgo_dump_impl(table_ptr: *const PgoTableEntry) {
    if table_ptr.is_null() {
        return;
    }
    let base = match image_base() {
        Some(b) => b,
        None => {
            eprintln!("adesh pgo: image base unavailable; profile not dumped");
            return;
        }
    };

    let mut funcs: std::collections::BTreeMap<String, PgoFnProfile> =
        std::collections::BTreeMap::new();
    // Sanity bound: a corrupt table must never walk off the mapping.
    const MAX_ENTRIES: usize = 1 << 20;
    unsafe {
        for i in 0..MAX_ENTRIES {
            let entry = &*table_ptr.add(i);
            if entry.name_rva == 0 && entry.counter_rva == 0 {
                break;
            }
            let name = cstr_bytes((base + entry.name_rva as usize) as *const c_char);
            let count = ((base + entry.counter_rva as usize) as *const u64).read_unaligned();
            pgo_record(&mut funcs, name, count);
        }
    }
    pgo_write_profile(&funcs);
}

/// Map one counter observation onto the aggregate. Counter names follow the
/// instrumentation pass grammar: `__pgo_entry_<fn>_<block_id>` (the entry
/// block) and `__pgo_counter_<fn>_<block_id>`; the block id is the last
/// `_`-separated component so function names may contain underscores.
fn pgo_record(
    funcs: &mut std::collections::BTreeMap<String, PgoFnProfile>,
    name: &[u8],
    count: u64,
) {
    let (is_entry, rest) = if let Some(r) = name.strip_prefix(b"__pgo_entry_") {
        (true, r)
    } else if let Some(r) = name.strip_prefix(b"__pgo_counter_") {
        (false, r)
    } else {
        eprintln!(
            "adesh pgo: unrecognized counter name `{}`, skipping",
            String::from_utf8_lossy(name)
        );
        return;
    };
    let rest = String::from_utf8_lossy(rest);
    let Some((fn_name, id_str)) = rest.rsplit_once('_') else {
        eprintln!("adesh pgo: malformed counter name `{}`, skipping", rest);
        return;
    };
    let Ok(block_id) = id_str.parse::<u32>() else {
        eprintln!(
            "adesh pgo: counter `{}` has a non-numeric block id, skipping",
            rest
        );
        return;
    };
    let prof = funcs.entry(fn_name.to_string()).or_default();
    if is_entry {
        prof.entry_count = prof.entry_count.saturating_add(count);
    }
    prof.blocks.insert(block_id, count);
}

/// Minimal JSON string escaping (identifiers only need `"` and `\`).
fn pgo_json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn pgo_write_profile(funcs: &std::collections::BTreeMap<String, PgoFnProfile>) {
    if funcs.is_empty() {
        return;
    }
    // Hand-rolled JSON in the exact `ProfileData` serde shape (all fields
    // are required on deserialize); keeps serde_json out of every linked
    // program's binary.
    let mut json = String::from("{\n  \"functions\": {");
    for (i, (fname, prof)) in funcs.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        let esc = pgo_json_escape(fname);
        json.push_str(&format!(
            "\n    \"{esc}\": {{\n      \"name\": \"{esc}\",\n      \"entry_count\": {},\n      \"block_profiles\": {{",
            prof.entry_count
        ));
        for (j, (block_id, count)) in prof.blocks.iter().enumerate() {
            if j > 0 {
                json.push(',');
            }
            json.push_str(&format!(
                "\n        \"{block_id}\": {{\"execution_count\": {count}}}"
            ));
        }
        json.push_str("\n      },\n      \"edge_profiles\": {}\n    }");
    }
    json.push_str("\n  }\n}");

    if let Err(e) = std::fs::write("adesh_pgo_profile.json", json) {
        eprintln!("adesh pgo: failed to write adesh_pgo_profile.json: {e}");
    }
}

/// Image base of the main module (needed to turn table RVAs into VAs).
#[cfg(windows)]
fn image_base() -> Option<usize> {
    unsafe {
        let handle = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(core::ptr::null());
        if handle == 0 {
            None
        } else {
            Some(handle as usize)
        }
    }
}

/// Non-Windows hosts never link an instrumented PE (the backend rejects
/// `--pgo=generate` for non-PE targets loudly), so there is no image base
/// to query.
#[cfg(not(windows))]
fn image_base() -> Option<usize> {
    None
}
