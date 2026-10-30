//! Raw allocation helpers for the native ABI surface.
//!
//! On native targets these map directly to `libc::malloc`/`libc::free`.
//! wasm32-unknown-unknown ships no libc, so the wasm variants back the same
//! ABI with the Rust global allocator and remember each block's size in a
//! side table so frees stay symmetrical. In the browser this ABI surface is
//! compiled for link-compatibility with the native codegen modules but is
//! never exercised by the interpreter (interpreted `alloc()` uses the pure
//! Rust heap in `backends::unsafe_heap`).

#[cfg(target_family = "wasm")]
use std::alloc::{Layout, alloc, dealloc};
#[cfg(target_family = "wasm")]
use std::collections::HashMap;
#[cfg(target_family = "wasm")]
use std::sync::{Mutex, OnceLock};

#[cfg(target_family = "wasm")]
fn allocation_registry() -> &'static Mutex<HashMap<usize, usize>> {
    static REGISTRY: OnceLock<Mutex<HashMap<usize, usize>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Allocate `size` bytes with malloc-compatible semantics.
/// Returns a null pointer on failure or zero-size requests.
#[allow(clippy::missing_safety_doc)]
pub unsafe fn raw_malloc(size: usize) -> *mut core::ffi::c_void {
    if size == 0 {
        return core::ptr::null_mut();
    }

    #[cfg(not(target_family = "wasm"))]
    {
        unsafe { libc::malloc(size) as *mut core::ffi::c_void }
    }

    #[cfg(target_family = "wasm")]
    {
        let Ok(layout) = Layout::from_size_align(size, 8) else {
            return core::ptr::null_mut();
        };
        let ptr = unsafe { alloc(layout) };
        if !ptr.is_null() {
            let mut registry = allocation_registry()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            registry.insert(ptr as usize, size);
        }
        ptr as *mut core::ffi::c_void
    }
}

/// Free a pointer previously returned by [`raw_malloc`].
/// Null and unknown pointers are ignored, mirroring `free(3)` tolerance.
#[allow(clippy::missing_safety_doc)]
pub unsafe fn raw_free(ptr: *mut core::ffi::c_void) {
    if ptr.is_null() {
        return;
    }

    #[cfg(not(target_family = "wasm"))]
    {
        unsafe { libc::free(ptr) }
    }

    #[cfg(target_family = "wasm")]
    {
        let size = {
            let mut registry = allocation_registry()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            registry.remove(&(ptr as usize))
        };
        if let Some(size) = size {
            if let Ok(layout) = Layout::from_size_align(size, 8) {
                unsafe { dealloc(ptr as *mut u8, layout) };
            }
        }
    }
}
