//! Phase 9 Sanitizer Runtime Support Hooks.
//!
//! Provides the runtime panic and abort endpoints invoked by compiler-generated
//! sanitizer checks for bounds, integer overflow, memory poisoning, and use-after-free.

use std::sync::atomic::{AtomicUsize, Ordering};

static BOUNDS_VIOLATIONS: AtomicUsize = AtomicUsize::new(0);
static OVERFLOW_VIOLATIONS: AtomicUsize = AtomicUsize::new(0);
static UAF_VIOLATIONS: AtomicUsize = AtomicUsize::new(0);

/// Check array/buffer indexing bounds. If index >= length, triggers bounds abort.
#[unsafe(no_mangle)]
pub extern "C" fn adesh_sanitizer_check_bounds(index: usize, length: usize) {
    if index >= length {
        BOUNDS_VIOLATIONS.fetch_add(1, Ordering::SeqCst);
        eprintln!(
            "Adesh Sanitizer Error: Out-of-bounds access! index={}, length={}",
            index, length
        );
        std::process::abort();
    }
}

/// Check signed integer arithmetic for overflow.
#[unsafe(no_mangle)]
pub extern "C" fn adesh_sanitizer_check_overflow(val: i64, limit: i64) {
    if val > limit {
        OVERFLOW_VIOLATIONS.fetch_add(1, Ordering::SeqCst);
        eprintln!(
            "Adesh Sanitizer Error: Integer overflow detected! value={}, limit={}",
            val, limit
        );
        std::process::abort();
    }
}

/// Poison memory after deallocation to detect use-after-free.
#[unsafe(no_mangle)]
pub extern "C" fn adesh_sanitizer_poison_memory(ptr: *mut u8, size: usize) {
    if !ptr.is_null() && size > 0 {
        unsafe {
            // Poison byte 0xAA (AddressSanitizer dead/freed memory pattern)
            std::ptr::write_bytes(ptr, 0xAA, size);
        }
    }
}

/// Inspect memory canary to verify pointer is not poisoned (UAF check).
#[unsafe(no_mangle)]
pub extern "C" fn adesh_sanitizer_check_uaf(ptr: *const u8) -> bool {
    if ptr.is_null() {
        return false;
    }
    unsafe {
        // If memory contains poison byte 0xAA, report UAF violation
        if *ptr == 0xAA {
            UAF_VIOLATIONS.fetch_add(1, Ordering::SeqCst);
            eprintln!(
                "Adesh Sanitizer Error: Use-after-free detected at address {:p}",
                ptr
            );
            return true;
        }
    }
    false
}

/// Retrieve runtime violation statistics.
pub fn get_sanitizer_stats() -> (usize, usize, usize) {
    (
        BOUNDS_VIOLATIONS.load(Ordering::SeqCst),
        OVERFLOW_VIOLATIONS.load(Ordering::SeqCst),
        UAF_VIOLATIONS.load(Ordering::SeqCst),
    )
}
