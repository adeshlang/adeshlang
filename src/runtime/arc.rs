//! Automatic Reference Counting (ARC) Runtime
//!
//! Thread-safe reference counting for heap-allocated objects.
//! Each allocation has an aligned header containing reference count,
//! optional destructor function pointer, object size, alignment, and
//! base allocation pointer for safe deallocation.
//!
//! Memory Layout:
//! ```text
//! [alloc_ptr ... optional padding] [ArcHeader (48B)] [user data (aligned to `align`)...]
//!                                  ^                 ^
//!                                  header_ptr        user_ptr (returned to user)
//! ```
//!
//! When the last strong reference is released, the destructor (if set)
//! is called *before* the memory is deallocated.

use std::alloc::{Layout, alloc, dealloc};
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Minimum alignment for ARC allocations
pub const MIN_ALIGN: usize = 8;

/// ARC Allocation Header
/// Stored immediately preceding user memory.
#[repr(C)]
pub struct ArcHeader {
    /// Atomic reference count (strong references)
    pub ref_count: AtomicUsize,
    /// User destructor function pointer (null if none)
    pub drop_fn: *mut u8,
    /// Size of the user data (excluding header and padding)
    pub size: usize,
    /// Alignment of user data
    pub align: usize,
    /// Base address of the underlying system allocation
    pub alloc_ptr: *mut u8,
    /// Total bytes allocated from the system allocator
    pub alloc_size: usize,
}

/// Size of the ARC header in bytes
pub const HEADER_SIZE: usize = std::mem::size_of::<ArcHeader>();

/// Type of the destructor function called when an ARC object is freed.
///
/// # Arguments
/// * `ptr` - Pointer to the object data (not the header)
/// * `size` - Size of the object data (excluding header)
pub type ArcDropFn = unsafe extern "C" fn(ptr: *mut u8, size: usize);

/// Allocate memory with ARC header (no destructor)
///
/// # Arguments
/// * `size` - Size of the object data (excluding header)
/// * `align` - Alignment requirement (minimum 8 bytes)
///
/// # Returns
/// Pointer to the object data (guaranteed aligned to `align`)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn arc_alloc(size: usize, align: usize) -> *mut u8 {
    unsafe { arc_alloc_with_drop(size, align, None) }
}

/// Allocate memory with ARC header and optional destructor
///
/// # Arguments
/// * `size` - Size of the object data (excluding header)
/// * `align` - Alignment requirement (minimum 8 bytes)
/// * `drop_fn` - Optional destructor function pointer
///
/// # Returns
/// Pointer to the object data (guaranteed aligned to `align`)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn arc_alloc_with_drop(
    size: usize,
    align: usize,
    drop_fn: Option<ArcDropFn>,
) -> *mut u8 {
    unsafe {
        let align = align.max(MIN_ALIGN).next_power_of_two();
        let alloc_align = align.max(std::mem::align_of::<ArcHeader>());
        let max_padding = align.saturating_sub(1);
        let total_size = match HEADER_SIZE
            .checked_add(max_padding)
            .and_then(|s| s.checked_add(size))
        {
            Some(t) => t,
            None => return ptr::null_mut(),
        };

        // Create layout for base allocation
        let layout = match Layout::from_size_align(total_size, alloc_align) {
            Ok(layout) => layout,
            Err(_) => return ptr::null_mut(),
        };

        // Allocate memory
        let base_ptr = alloc(layout);
        if base_ptr.is_null() {
            return ptr::null_mut();
        }

        // Calculate user pointer aligned to `align`
        let min_user_addr = (base_ptr as usize).checked_add(HEADER_SIZE);
        let min_user_addr = match min_user_addr {
            Some(a) => a,
            None => {
                dealloc(base_ptr, layout);
                return ptr::null_mut();
            }
        };

        let user_addr = (min_user_addr + align - 1) & !(align - 1);
        let user_ptr = user_addr as *mut u8;

        // Header sits immediately before user_ptr
        let header_ptr = user_ptr.sub(HEADER_SIZE) as *mut ArcHeader;
        debug_assert!(
            header_ptr as usize >= base_ptr as usize,
            "header_ptr must be within allocated block"
        );
        debug_assert_eq!(
            (header_ptr as usize) % std::mem::align_of::<ArcHeader>(),
            0,
            "header_ptr must satisfy ArcHeader alignment"
        );
        debug_assert_eq!(
            (user_ptr as usize) % align,
            0,
            "user_ptr must satisfy requested alignment"
        );

        // Initialize header
        ptr::write(
            header_ptr,
            ArcHeader {
                ref_count: AtomicUsize::new(1),
                drop_fn: drop_fn.map(|f| f as *mut u8).unwrap_or(ptr::null_mut()),
                size,
                align,
                alloc_ptr: base_ptr,
                alloc_size: total_size,
            },
        );

        user_ptr
    }
}

/// Increment reference count (retain)
///
/// # Arguments
/// * `ptr` - Pointer to the object data (not the header)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn arc_retain(ptr: *mut u8) {
    unsafe {
        if ptr.is_null() {
            return;
        }

        let header_ptr = ptr.sub(HEADER_SIZE) as *mut ArcHeader;
        let old_count = (*header_ptr).ref_count.fetch_add(1, Ordering::Relaxed);
        if old_count >= (usize::MAX / 2) {
            panic!("arc_retain: reference count overflow");
        }

        debug_assert!(old_count > 0, "arc_retain called on deallocated object");
    }
}

/// Decrement reference count (release)
///
/// When the reference count reaches zero:
/// 1. The destructor (if registered) is called with the object pointer and size
/// 2. The memory is deallocated using metadata stored in the header
///
/// # Arguments
/// * `ptr` - Pointer to the object data
/// * `_size` - Ignored (retrieved directly from the header for memory safety)
/// * `_align` - Ignored (retrieved directly from the header for memory safety)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn arc_release(ptr: *mut u8, _size: usize, _align: usize) {
    unsafe {
        if ptr.is_null() {
            return;
        }

        let header_ptr = ptr.sub(HEADER_SIZE) as *mut ArcHeader;
        let old_count = (*header_ptr).ref_count.fetch_sub(1, Ordering::Release);

        debug_assert!(
            old_count > 0,
            "arc_release called with zero reference count"
        );

        // If this was the last reference, call destructor and deallocate
        if old_count == 1 {
            // Acquire fence to synchronize with all other releasing threads
            std::sync::atomic::fence(Ordering::Acquire);

            // Read metadata safely from header
            let drop_fn_raw = (*header_ptr).drop_fn;
            let actual_size = (*header_ptr).size;
            let alloc_ptr = (*header_ptr).alloc_ptr;
            let alloc_size = (*header_ptr).alloc_size;
            let alloc_align = (*header_ptr).align.max(std::mem::align_of::<ArcHeader>());

            // Call user-defined destructor if registered
            if !drop_fn_raw.is_null() {
                let drop_fn: ArcDropFn = std::mem::transmute(drop_fn_raw);
                drop_fn(ptr, actual_size);
            }

            // Deallocate using the exact base allocation layout
            if let Ok(layout) = Layout::from_size_align(alloc_size, alloc_align) {
                dealloc(alloc_ptr, layout);
            }
        }
    }
}

/// Decrement reference count without requiring dummy size/align arguments
#[unsafe(no_mangle)]
pub unsafe extern "C" fn arc_release_ptr(ptr: *mut u8) {
    unsafe {
        arc_release(ptr, 0, 0);
    }
}

/// Get current reference count (for debugging)
///
/// # Arguments
/// * `ptr` - Pointer to the object data
///
/// # Returns
/// Current reference count, or 0 if ptr is null
#[unsafe(no_mangle)]
pub unsafe extern "C" fn arc_get_count(ptr: *mut u8) -> usize {
    unsafe {
        if ptr.is_null() {
            return 0;
        }

        let header_ptr = ptr.sub(HEADER_SIZE) as *mut ArcHeader;
        (*header_ptr).ref_count.load(Ordering::Relaxed)
    }
}

/// Clone operation (returns a new reference to the same object)
///
/// # Arguments
/// * `ptr` - Pointer to the object data
///
/// # Returns
/// The same pointer with incremented reference count
#[unsafe(no_mangle)]
pub unsafe extern "C" fn arc_clone(ptr: *mut u8) -> *mut u8 {
    unsafe {
        if !ptr.is_null() {
            arc_retain(ptr);
        }
        ptr
    }
}

/// Set or clear the destructor for an existing ARC object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn arc_set_drop_fn(ptr: *mut u8, drop_fn: Option<ArcDropFn>) {
    unsafe {
        if ptr.is_null() {
            return;
        }

        let header_ptr = ptr.sub(HEADER_SIZE) as *mut ArcHeader;
        (*header_ptr).drop_fn = drop_fn.map(|f| f as *mut u8).unwrap_or(ptr::null_mut());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize};

    // Track destructor calls for testing
    static TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());
    static DROP_CALLED: AtomicBool = AtomicBool::new(false);
    static DROP_PTR_RECEIVED: AtomicUsize = AtomicUsize::new(0);
    static DROP_SIZE_RECEIVED: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "C" fn test_drop_fn(ptr: *mut u8, size: usize) {
        DROP_CALLED.store(true, Ordering::SeqCst);
        DROP_PTR_RECEIVED.store(ptr as usize, Ordering::SeqCst);
        DROP_SIZE_RECEIVED.store(size, Ordering::SeqCst);
    }

    #[test]
    fn test_arc_alloc_dealloc() {
        unsafe {
            let ptr = arc_alloc(64, 8);
            assert!(!ptr.is_null());
            assert_eq!(arc_get_count(ptr), 1);
            arc_release(ptr, 64, 8);
        }
    }

    #[test]
    fn test_arc_retain_release() {
        unsafe {
            let ptr = arc_alloc(64, 8);
            assert_eq!(arc_get_count(ptr), 1);

            arc_retain(ptr);
            assert_eq!(arc_get_count(ptr), 2);

            arc_release(ptr, 64, 8);
            assert_eq!(arc_get_count(ptr), 1);

            arc_release(ptr, 64, 8);
            // ptr is now deallocated
        }
    }

    #[test]
    fn test_arc_clone() {
        unsafe {
            let ptr = arc_alloc(64, 8);
            let ptr2 = arc_clone(ptr);

            assert_eq!(ptr, ptr2);
            assert_eq!(arc_get_count(ptr), 2);

            arc_release(ptr, 64, 8);
            arc_release(ptr2, 64, 8);
        }
    }

    #[test]
    fn test_arc_null_safe() {
        unsafe {
            // These should not crash
            arc_retain(ptr::null_mut());
            arc_release(ptr::null_mut(), 0, 8);
            assert_eq!(arc_get_count(ptr::null_mut()), 0);
            assert_eq!(arc_clone(ptr::null_mut()), ptr::null_mut());
        }
    }

    #[test]
    fn test_arc_destructor_called() {
        let _guard = TEST_MUTEX.lock().unwrap();
        DROP_CALLED.store(false, Ordering::SeqCst);
        unsafe {
            let ptr = arc_alloc_with_drop(32, 8, Some(test_drop_fn));
            assert!(!ptr.is_null());
            assert_eq!(arc_get_count(ptr), 1);

            // Release should call the destructor then deallocate
            arc_release(ptr, 32, 8);

            assert!(DROP_CALLED.load(Ordering::SeqCst));
            assert_eq!(DROP_PTR_RECEIVED.load(Ordering::SeqCst), ptr as usize);
            assert_eq!(DROP_SIZE_RECEIVED.load(Ordering::SeqCst), 32);
        }
    }

    #[test]
    fn test_arc_destructor_not_called_on_retain_release() {
        let _guard = TEST_MUTEX.lock().unwrap();
        DROP_CALLED.store(false, Ordering::SeqCst);
        unsafe {
            let ptr = arc_alloc_with_drop(32, 8, Some(test_drop_fn));

            // Retain (should NOT call destructor)
            arc_retain(ptr);
            assert!(!DROP_CALLED.load(Ordering::SeqCst));

            // Release one ref (should NOT call destructor, count goes to 1)
            arc_release(ptr, 32, 8);
            assert!(!DROP_CALLED.load(Ordering::SeqCst));

            // Release last ref (SHOULD call destructor)
            arc_release(ptr, 32, 8);
            assert!(DROP_CALLED.load(Ordering::SeqCst));
        }
    }

    #[test]
    fn test_arc_set_drop_fn_after_alloc() {
        let _guard = TEST_MUTEX.lock().unwrap();
        DROP_CALLED.store(false, Ordering::SeqCst);
        unsafe {
            let ptr = arc_alloc(32, 8);
            assert_eq!(arc_get_count(ptr), 1);

            // Attach destructor after allocation
            arc_set_drop_fn(ptr, Some(test_drop_fn));

            arc_release(ptr, 32, 8);
            assert!(DROP_CALLED.load(Ordering::SeqCst));
        }
    }

    #[test]
    fn test_arc_no_destructor() {
        // Without a destructor, release should just deallocate without issues
        unsafe {
            let ptr = arc_alloc(48, 8);
            arc_release(ptr, 48, 8);
            // No crash = success
        }
    }

    #[test]
    fn test_arc_alignment_high() {
        unsafe {
            // Test 16-byte, 32-byte (AVX), and 64-byte (cache line) alignments
            for &align in &[16, 32, 64] {
                let ptr = arc_alloc(128, align);
                assert!(!ptr.is_null());
                assert_eq!(
                    (ptr as usize) % align,
                    0,
                    "ptr must be aligned to {}",
                    align
                );
                arc_release_ptr(ptr);
            }
        }
    }

    #[test]
    fn test_arc_release_ptr_and_mismatched_caller_params() {
        unsafe {
            // arc_release should read true size from header and ignore caller mismatches
            let ptr = arc_alloc(100, 32);
            assert_eq!((ptr as usize) % 32, 0);
            // Pass deliberately wrong size and align
            arc_release(ptr, 99999, 128);
        }
    }
}
