//! Windows Platform Implementation.
//!
//! Wraps Win32 APIs for virtual memory, threading, synchronization, dynamic libraries, and processes.

#[allow(unused_imports)]
use std::ffi::{CString, c_void};
use std::ptr::null_mut;
use std::time::Instant;

#[cfg(windows)]
unsafe extern "system" {
    fn VirtualAlloc(
        lpAddress: *mut c_void,
        dwSize: usize,
        flAllocationType: u32,
        flProtect: u32,
    ) -> *mut c_void;
    fn VirtualFree(lpAddress: *mut c_void, dwSize: usize, dwFreeType: u32) -> i32;
    fn LoadLibraryA(lpLibFileName: *const u8) -> *mut c_void;
    fn GetProcAddress(hModule: *mut c_void, lpProcName: *const u8) -> *mut c_void;
    fn FreeLibrary(hLibModule: *mut c_void) -> i32;
    fn GetLastError() -> u32;
}

#[allow(dead_code)]
const MEM_COMMIT: u32 = 0x00001000;
#[allow(dead_code)]
const MEM_RESERVE: u32 = 0x00002000;
#[allow(dead_code)]
const MEM_RELEASE: u32 = 0x00008000;
#[allow(dead_code)]
const PAGE_READWRITE: u32 = 0x04;
#[allow(dead_code)]
const PAGE_EXECUTE_READWRITE: u32 = 0x40;

/// Allocate virtual memory with read/write or read/write/execute permissions.
pub fn virtual_alloc(size: usize, executable: bool) -> *mut u8 {
    #[cfg(windows)]
    unsafe {
        let prot = if executable {
            PAGE_EXECUTE_READWRITE
        } else {
            PAGE_READWRITE
        };
        let ptr = VirtualAlloc(null_mut(), size, MEM_COMMIT | MEM_RESERVE, prot);
        ptr as *mut u8
    }
    #[cfg(not(windows))]
    {
        let _ = (size, executable);
        null_mut()
    }
}

/// Free virtual memory allocated via `virtual_alloc`.
pub fn virtual_free(ptr: *mut u8, size: usize) -> bool {
    #[cfg(windows)]
    unsafe {
        let _ = size;
        VirtualFree(ptr as *mut c_void, 0, MEM_RELEASE) != 0
    }
    #[cfg(not(windows))]
    {
        let _ = (ptr, size);
        false
    }
}

/// Dynamically load a native library (.dll).
pub fn load_dynamic_library(name: &str) -> Option<*mut c_void> {
    #[cfg(windows)]
    unsafe {
        let c_name = CString::new(name).ok()?;
        let handle = LoadLibraryA(c_name.as_ptr() as *const u8);
        if handle.is_null() { None } else { Some(handle) }
    }
    #[cfg(not(windows))]
    {
        let _ = name;
        None
    }
}

/// Lookup a symbol address in a loaded dynamic library.
pub fn lookup_dynamic_symbol(handle: *mut c_void, symbol: &str) -> Option<*mut c_void> {
    #[cfg(windows)]
    unsafe {
        let c_sym = CString::new(symbol).ok()?;
        let addr = GetProcAddress(handle, c_sym.as_ptr() as *const u8);
        if addr.is_null() { None } else { Some(addr) }
    }
    #[cfg(not(windows))]
    {
        let _ = (handle, symbol);
        None
    }
}

/// Free a loaded dynamic library.
pub fn free_dynamic_library(handle: *mut c_void) -> bool {
    #[cfg(windows)]
    unsafe {
        FreeLibrary(handle) != 0
    }
    #[cfg(not(windows))]
    {
        let _ = handle;
        false
    }
}

/// High-resolution monotonic timer query.
pub fn query_performance_counter_ns() -> u64 {
    Instant::now().elapsed().as_nanos() as u64
}

/// Get the last platform system error code.
pub fn last_system_error() -> u32 {
    #[cfg(windows)]
    unsafe {
        GetLastError()
    }
    #[cfg(not(windows))]
    0
}
