//! Safe Rust wrappers for the standalone C output ABI.
//!
//! The exported `aot_*` symbols are implemented directly by
//! `native_output.c`, so native programs can resolve them without extracting
//! Rust formatting or runtime code from the archive.

use std::os::raw::c_char;

#[cfg(not(target_family = "wasm"))]
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

// On wasm32-unknown-unknown the C object is not compiled (no libc), and the
// native AOT codegen path that references these symbols cannot run in the
// browser. Provide unreachable stubs so the module still links; any real
// call is a programming error and panics loudly instead of silently.
#[cfg(target_family = "wasm")]
mod native_stubs {
    use std::os::raw::c_char;

    #[unsafe(no_mangle)]
    pub extern "C" fn aot_print_cstr(_text: *const c_char, _newline: i64) -> u64 {
        panic!("aot_print_cstr is not available in WebAssembly builds")
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn aot_abort_str(_message: *const c_char) -> ! {
        panic!("aot_abort_str is not available in WebAssembly builds")
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn aot_panic_at(
        _message: *const c_char,
        _file: *const c_char,
        _line: u32,
    ) -> ! {
        panic!("aot_panic_at is not available in WebAssembly builds")
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn aot_print_i64(_value: i64, _newline: i64) -> u64 {
        panic!("aot_print_i64 is not available in WebAssembly builds")
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn aot_print_u64(_value: u64, _newline: i64) -> u64 {
        panic!("aot_print_u64 is not available in WebAssembly builds")
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn aot_print_bool(_value: i64, _newline: i64) -> u64 {
        panic!("aot_print_bool is not available in WebAssembly builds")
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn aot_print_space() -> u64 {
        panic!("aot_print_space is not available in WebAssembly builds")
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn aot_print_newline() -> u64 {
        panic!("aot_print_newline is not available in WebAssembly builds")
    }
}

pub fn aot_print_cstr(text: *const c_char, newline: i64) -> u64 {
    #[cfg(target_family = "wasm")]
    {
        let _ = (text, newline);
        panic!("aot_print_cstr is not available in WebAssembly builds");
    }
    #[cfg(not(target_family = "wasm"))]
    unsafe {
        native_print_cstr(text, newline)
    }
}

pub fn aot_abort_str(message: *const c_char) -> ! {
    #[cfg(target_family = "wasm")]
    {
        let _ = message;
        panic!("aot_abort_str is not available in WebAssembly builds");
    }
    #[cfg(not(target_family = "wasm"))]
    unsafe {
        native_abort_str(message)
    }
}

pub fn aot_panic_at(message: *const c_char, file: *const c_char, line: u32) -> ! {
    #[cfg(target_family = "wasm")]
    {
        let _ = (message, file, line);
        panic!("aot_panic_at is not available in WebAssembly builds");
    }
    #[cfg(not(target_family = "wasm"))]
    unsafe {
        native_panic_at(message, file, line)
    }
}

pub fn aot_print_i64(value: i64, newline: i64) -> u64 {
    #[cfg(target_family = "wasm")]
    {
        let _ = (value, newline);
        panic!("aot_print_i64 is not available in WebAssembly builds");
    }
    #[cfg(not(target_family = "wasm"))]
    unsafe {
        native_print_i64(value, newline)
    }
}

pub fn aot_print_u64(value: u64, newline: i64) -> u64 {
    #[cfg(target_family = "wasm")]
    {
        let _ = (value, newline);
        panic!("aot_print_u64 is not available in WebAssembly builds");
    }
    #[cfg(not(target_family = "wasm"))]
    unsafe {
        native_print_u64(value, newline)
    }
}

pub fn aot_print_bool(value: i64, newline: i64) -> u64 {
    #[cfg(target_family = "wasm")]
    {
        let _ = (value, newline);
        panic!("aot_print_bool is not available in WebAssembly builds");
    }
    #[cfg(not(target_family = "wasm"))]
    unsafe {
        native_print_bool(value, newline)
    }
}

pub fn aot_print_space() -> u64 {
    #[cfg(target_family = "wasm")]
    {
        panic!("aot_print_space is not available in WebAssembly builds");
    }
    #[cfg(not(target_family = "wasm"))]
    unsafe {
        native_print_space()
    }
}

pub fn aot_print_newline() -> u64 {
    #[cfg(target_family = "wasm")]
    {
        panic!("aot_print_newline is not available in WebAssembly builds");
    }
    #[cfg(not(target_family = "wasm"))]
    unsafe {
        native_print_newline()
    }
}
