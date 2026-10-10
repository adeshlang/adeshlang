//! Safe Rust wrappers for the standalone C output ABI.
//!
//! The exported `aot_*` symbols are implemented directly by
//! `native_output.c`, so native programs can resolve them without extracting
//! Rust formatting or runtime code from the archive.

use std::os::raw::c_char;

unsafe extern "C" {
    #[link_name = "aot_print_cstr"]
    fn native_print_cstr(text: *const c_char, newline: i64) -> u64;
    #[link_name = "aot_abort_str"]
    fn native_abort_str(message: *const c_char) -> !;
    #[link_name = "aot_panic_at"]
    fn native_panic_at(message: *const c_char, file: *const c_char, line: u32) -> !;
    #[link_name = "aot_print_i64"]
    fn native_print_i64(value: i64, newline: i64) -> u64;
    #[link_name = "aot_print_u64"]
    fn native_print_u64(value: u64, newline: i64) -> u64;
    #[link_name = "aot_print_bool"]
    fn native_print_bool(value: i64, newline: i64) -> u64;
    #[link_name = "aot_print_space"]
    fn native_print_space() -> u64;
    #[link_name = "aot_print_newline"]
    fn native_print_newline() -> u64;
}

pub fn aot_print_cstr(text: *const c_char, newline: i64) -> u64 {
    unsafe { native_print_cstr(text, newline) }
}

pub fn aot_abort_str(message: *const c_char) -> ! {
    unsafe { native_abort_str(message) }
}

pub fn aot_panic_at(message: *const c_char, file: *const c_char, line: u32) -> ! {
    unsafe { native_panic_at(message, file, line) }
}

pub fn aot_print_i64(value: i64, newline: i64) -> u64 {
    unsafe { native_print_i64(value, newline) }
}

pub fn aot_print_u64(value: u64, newline: i64) -> u64 {
    unsafe { native_print_u64(value, newline) }
}

pub fn aot_print_bool(value: i64, newline: i64) -> u64 {
    unsafe { native_print_bool(value, newline) }
}

pub fn aot_print_space() -> u64 {
    unsafe { native_print_space() }
}

pub fn aot_print_newline() -> u64 {
    unsafe { native_print_newline() }
}
