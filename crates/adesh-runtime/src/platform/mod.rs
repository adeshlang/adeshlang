//! Centralized Operating System & Platform Abstraction Layer.
//!
//! Provides a unified platform interface across Windows, Linux, macOS, and Unix.
//! Abstracts virtual memory, threads, synchronization, atomics, timers, dynamic libraries,
//! and OS error codes without scattering `#[cfg(target_os = ...)]` throughout compiler modules.

pub mod linux;
pub mod macos;
pub mod unix;
pub mod windows;

use std::ffi::c_void;
use std::time::Instant;

/// Unified Virtual Memory Allocator.
pub struct VirtualMemory;

impl VirtualMemory {
    /// Allocate virtual memory of the given size. If `executable` is true,
    /// sets memory protection to RX or RWX.
    pub fn allocate(size: usize, executable: bool) -> *mut u8 {
        #[cfg(windows)]
        {
            windows::virtual_alloc(size, executable)
        }
        #[cfg(unix)]
        {
            unix::unix_virtual_alloc(size, executable)
        }
        #[cfg(not(any(windows, unix)))]
        {
            let _ = (size, executable);
            std::ptr::null_mut()
        }
    }

    /// Free virtual memory previously allocated by `allocate`.
    pub fn free(ptr: *mut u8, size: usize) -> bool {
        #[cfg(windows)]
        {
            windows::virtual_free(ptr, size)
        }
        #[cfg(unix)]
        {
            unix::unix_virtual_free(ptr, size)
        }
        #[cfg(not(any(windows, unix)))]
        {
            let _ = (ptr, size);
            false
        }
    }
}

/// Unified Dynamic Library Loader (`.dll`, `.so`, `.dylib`).
pub struct DynamicLibrary {
    handle: *mut c_void,
}

impl DynamicLibrary {
    pub fn load(name: &str) -> Option<Self> {
        #[cfg(windows)]
        let handle = windows::load_dynamic_library(name)?;
        #[cfg(unix)]
        let handle = unix::unix_dlopen(name)?;
        #[cfg(not(any(windows, unix)))]
        let handle = {
            let _ = name;
            return None;
        };

        Some(Self { handle })
    }

    pub fn symbol(&self, name: &str) -> Option<*mut c_void> {
        #[cfg(windows)]
        {
            windows::lookup_dynamic_symbol(self.handle, name)
        }
        #[cfg(unix)]
        {
            unix::unix_dlsym(self.handle, name)
        }
        #[cfg(not(any(windows, unix)))]
        {
            let _ = name;
            None
        }
    }
}

impl Drop for DynamicLibrary {
    fn drop(&mut self) {
        #[cfg(windows)]
        windows::free_dynamic_library(self.handle);
        #[cfg(unix)]
        unix::unix_dlclose(self.handle);
    }
}

/// High-resolution platform timer.
pub struct PlatformTimer;

impl PlatformTimer {
    #[inline]
    pub fn now() -> Instant {
        Instant::now()
    }

    #[inline]
    pub fn elapsed_nanos(start: Instant) -> u64 {
        start.elapsed().as_nanos() as u64
    }
}

/// Query current system error code (GetLastError on Windows, errno on Unix).
pub fn system_error_code() -> u32 {
    #[cfg(windows)]
    {
        windows::last_system_error()
    }
    #[cfg(unix)]
    {
        unix::unix_last_error() as u32
    }
    #[cfg(not(any(windows, unix)))]
    {
        0
    }
}
