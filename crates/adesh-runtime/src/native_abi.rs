//! Native-pipeline ABI surface (`ADESH_RUNTIME_NATIVE_V1`).
//!
//! Symbols required by `src/backends/native/lower.rs` that previously did not
//! exist at all (every program touching them failed at link time):
//! string concat, range construction, `in` membership, dict construction,
//! raw allocation, wall-clock time, the f64 math dispatcher, and the
//! abort-with-message helper.
//!
//! Also provides a lean console path (`aot_print_cstr`) that writes raw bytes
//! through the OS instead of Rust's formatting machinery, so programs whose
//! only runtime interaction is printing a string literal do not pay for the
//! std formatting code (~90KB of .text in the linked binary).

use std::os::raw::c_char;

use crate::{RuntimeValue, aot_store_value, get_string_val, unpack_aot_arg};

// ============================================================================
// Raw OS console output
// ============================================================================

#[cfg(windows)]
mod raw_io {
    const STD_OUTPUT_HANDLE: u32 = 0xFFFF_FFF5; // (u32)-11
    type Handle = *mut core::ffi::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetStdHandle(nStdHandle: u32) -> Handle;
        fn GetConsoleMode(hConsoleHandle: Handle, lpMode: *mut u32) -> i32;
        fn WriteFile(
            hFile: Handle,
            lpBuffer: *const u8,
            nNumberOfBytesToWrite: u32,
            lpNumberOfBytesWritten: *mut u32,
            lpOverlapped: *mut core::ffi::c_void,
        ) -> i32;
        fn WriteConsoleW(
            hConsoleOutput: Handle,
            lpBuffer: *const u16,
            nNumberOfCharsToWrite: u32,
            lpNumberOfCharsWritten: *mut u32,
            lpReserved: *mut core::ffi::c_void,
        ) -> i32;
        fn ExitProcess(uExitCode: u32) -> !;
    }

    /// Write UTF-8 bytes to stdout. Console handles get a UTF-16 conversion so
    /// non-ASCII text renders like the interpreter's output; pipes and
    /// redirection receive the raw bytes.
    pub fn stdout_write(bytes: &[u8]) {
        unsafe {
            let h = GetStdHandle(STD_OUTPUT_HANDLE);
            if h.is_null() {
                return;
            }
            let mut mode = 0u32;
            if GetConsoleMode(h, &mut mode) != 0 {
                if let Ok(s) = core::str::from_utf8(bytes) {
                    let units: Vec<u16> = s.encode_utf16().collect();
                    if !units.is_empty() {
                        let mut written = 0u32;
                        WriteConsoleW(
                            h,
                            units.as_ptr(),
                            units.len() as u32,
                            &mut written,
                            core::ptr::null_mut(),
                        );
                    }
                    return;
                }
            }
            let mut written = 0u32;
            WriteFile(
                h,
                bytes.as_ptr(),
                bytes.len() as u32,
                &mut written,
                core::ptr::null_mut(),
            );
        }
    }

    pub fn exit(code: i32) -> ! {
        unsafe { ExitProcess(code as u32) }
    }
}

#[cfg(not(windows))]
mod raw_io {
    #[link(name = "c")]
    unsafe extern "C" {
        fn write(fd: i32, buf: *const u8, count: usize) -> isize;
        fn _exit(code: i32) -> !;
    }

    pub fn stdout_write(bytes: &[u8]) {
        let mut off = 0usize;
        while off < bytes.len() {
            let n = unsafe { write(1, bytes.as_ptr().add(off), bytes.len() - off) };
            if n <= 0 {
                break;
            }
            off += n as usize;
        }
    }

    pub fn exit(code: i32) -> ! {
        unsafe { _exit(code) }
    }
}

/// Length of a NUL-terminated byte string (the native backend's `.rodata`
/// string literals are NUL-terminated `__str_N` symbols).
unsafe fn cstr_bytes(ptr: *const c_char) -> &'static [u8] {
    if ptr.is_null() {
        return &[];
    }
    let start = ptr as *const u8;
    let mut end = start;
    unsafe {
        while *end != 0 {
            end = end.add(1);
        }
    }
    unsafe { core::slice::from_raw_parts(start, end.offset_from(start) as usize) }
}

/// Print a NUL-terminated string literal, optionally appending a newline.
/// Both `print` and `println` append the newline (interpreter `print`
/// semantics); the backend decides by passing `newline`.
#[unsafe(no_mangle)]
pub extern "C" fn aot_print_cstr(string_ptr: *const c_char, newline: i64) -> u64 {
    let bytes = unsafe { cstr_bytes(string_ptr) };
    raw_io::stdout_write(bytes);
    if newline != 0 {
        raw_io::stdout_write(b"\n");
    }
    0
}

/// Print a NUL-terminated message and terminate the process. Used by the
/// backend's loud-abort paths (division guards, unsupported constructs).
#[unsafe(no_mangle)]
pub extern "C" fn aot_abort_str(msg_ptr: *const c_char) -> ! {
    let bytes = unsafe { cstr_bytes(msg_ptr) };
    if !bytes.is_empty() {
        raw_io::stdout_write(bytes);
        raw_io::stdout_write(b"\n");
    }
    raw_io::exit(101)
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
    get_string_val(handle).unwrap_or_else(|| unpack_aot_arg(handle).as_string())
}

/// Concatenate two values as strings (the `+` operator on strings).
#[unsafe(no_mangle)]
pub extern "C" fn aot_string_concat(a_handle: u64, b_handle: u64) -> u64 {
    let a = value_as_string(a_handle);
    let b = value_as_string(b_handle);
    aot_store_value(RuntimeValue::String(a + &b))
}

/// Materialize `start..end` (inclusive == 0) or `start..=end` (inclusive
/// != 0) as an array of integers, matching the interpreter/Cranelift backend
/// range semantics. Descending ranges count down.
#[unsafe(no_mangle)]
pub extern "C" fn aot_make_range(start: i64, end: i64, inclusive: i64) -> u64 {
    let mut items = Vec::new();
    if start <= end {
        let stop = if inclusive != 0 { end + 1 } else { end };
        let mut i = start;
        while i < stop {
            items.push(RuntimeValue::Int(i));
            i += 1;
        }
    } else {
        let stop = if inclusive != 0 { end - 1 } else { end };
        let mut i = start;
        while i > stop {
            items.push(RuntimeValue::Int(i));
            i -= 1;
        }
    }
    aot_store_value(RuntimeValue::Array(items))
}

/// Create an empty dict; entries are added via `aot_set_index`.
#[unsafe(no_mangle)]
pub extern "C" fn aot_make_dict() -> u64 {
    aot_store_value(RuntimeValue::Object(std::collections::BTreeMap::new()))
}

/// `item in container` membership test. Returns a raw 0/1 (never a handle).
/// Arrays/tuples/sets test element membership, strings test substring
/// containment, objects/dicts test key presence.
#[unsafe(no_mangle)]
pub extern "C" fn aot_contains(container_handle: u64, item_handle: u64) -> i64 {
    let container =
        crate::aot_get_value(container_handle).unwrap_or_else(|| unpack_aot_arg(container_handle));
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
}

// ============================================================================
// Raw allocation
// ============================================================================

/// Raw byte allocation for `alloc<T>(size)`. Returns a heap pointer.
#[unsafe(no_mangle)]
pub extern "C" fn aot_alloc(size: i64) -> *mut core::ffi::c_void {
    if size <= 0 {
        return core::ptr::null_mut();
    }
    unsafe { libc::malloc(size as usize) as *mut core::ffi::c_void }
}

/// Free a pointer previously returned by `aot_alloc`.
#[unsafe(no_mangle)]
pub extern "C" fn aot_free(ptr: *mut core::ffi::c_void) {
    if !ptr.is_null() {
        unsafe { libc::free(ptr as *mut core::ffi::c_void) }
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
