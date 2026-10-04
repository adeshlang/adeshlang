//! Shared Unix Platform Implementation (POSIX, Linux, macOS, BSD).
#[allow(unused_imports)]

use std::ffi::{c_void, CString};
use std::ptr::{null, null_mut};

/// Allocate virtual memory using POSIX mmap.
pub fn unix_virtual_alloc(size: usize, executable: bool) -> *mut u8 {
    #[cfg(unix)]
    unsafe {
        let mut prot = libc::PROT_READ | libc::PROT_WRITE;
        if executable {
            prot |= libc::PROT_EXEC;
        }
        let flags = libc::MAP_PRIVATE | libc::MAP_ANONYMOUS;
        let ptr = libc::mmap(null_mut(), size, prot, flags, -1, 0);
        if ptr == libc::MAP_FAILED {
            null_mut()
        } else {
            ptr as *mut u8
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (size, executable);
        null_mut()
    }
}

/// Free virtual memory using POSIX munmap.
pub fn unix_virtual_free(ptr: *mut u8, size: usize) -> bool {
    #[cfg(unix)]
    unsafe {
        libc::munmap(ptr as *mut c_void, size) == 0
    }
    #[cfg(not(unix))]
    {
        let _ = (ptr, size);
        false
    }
}

/// Load dynamic library using dlopen.
pub fn unix_dlopen(name: &str) -> Option<*mut c_void> {
    #[cfg(unix)]
    unsafe {
        let c_name = CString::new(name).ok()?;
        let handle = libc::dlopen(c_name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
        if handle.is_null() {
            None
        } else {
            Some(handle)
        }
    }
    #[cfg(not(unix))]
    {
        let _ = name;
        None
    }
}

/// Lookup symbol using dlsym.
pub fn unix_dlsym(handle: *mut c_void, symbol: &str) -> Option<*mut c_void> {
    #[cfg(unix)]
    unsafe {
        let c_sym = CString::new(symbol).ok()?;
        let addr = libc::dlsym(handle, c_sym.as_ptr());
        if addr.is_null() {
            None
        } else {
            Some(addr)
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (handle, symbol);
        None
    }
}

/// Close dynamic library using dlclose.
pub fn unix_dlclose(handle: *mut c_void) -> bool {
    #[cfg(unix)]
    unsafe {
        libc::dlclose(handle) == 0
    }
    #[cfg(not(unix))]
    {
        let _ = handle;
        false
    }
}

/// Query system error code (errno).
pub fn unix_last_error() -> i32 {
    #[cfg(unix)]
    unsafe {
        *libc::__errno_location()
    }
    #[cfg(not(unix))]
    0
}
