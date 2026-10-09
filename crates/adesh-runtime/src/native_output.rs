//! Small Rust ABI wrappers around the standalone native console implementation.
//!
//! Keeping these wrappers in their own module lets the linker extract them
//! without pulling in the general-purpose runtime ABI.

use std::os::raw::c_char;

unsafe extern "C" {
    fn adesh_native_print_cstr(text: *const c_char, newline: i64) -> u64;
    fn adesh_native_abort_str(message: *const c_char) -> !;
}

/// Print a NUL-terminated native string, optionally appending a newline.
#[unsafe(no_mangle)]
pub extern "C" fn aot_print_cstr(text: *const c_char, newline: i64) -> u64 {
    unsafe { adesh_native_print_cstr(text, newline) }
}

/// Print a NUL-terminated message and terminate the process.
#[unsafe(no_mangle)]
pub extern "C" fn aot_abort_str(message: *const c_char) -> ! {
    unsafe { adesh_native_abort_str(message) }
}
