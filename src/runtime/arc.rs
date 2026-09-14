//! Automatic Reference Counting (ARC) Runtime
//!
//! Thread-safe reference counting for heap-allocated objects.
//! Each allocation has a 16-byte header containing the reference count
//! and an optional destructor function pointer.
//!
//! Memory Layout:
//! ```
//! [ref_count: usize (8 bytes)][drop_fn: *mut u8 (8 bytes)][object data...]
//!  ^                          ^                           ^
//!  header (16 bytes total)                                user pointer (returned to user)
//! ```
//!
//! When the last strong reference is released, the destructor (if set)
//! is called *before* the memory is deallocated. This allows user-defined
//! cleanup (closing file handles, freeing nested allocations, etc.).

use std::alloc::{Layout, alloc, dealloc};
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Size of the ARC header: ref_count (8) + drop_fn pointer (8) = 16 bytes
const HEADER_SIZE: usize = 16;

/// Offset of the reference count within the header
const REFCOUNT_OFFSET: usize = 0;

/// Offset of the drop function pointer within the header
const DROPFN_OFFSET: usize = 8;

/// Minimum alignment for ARC allocations
const MIN_ALIGN: usize = 8;

/// Type of the destructor function called when an ARC object is freed.
///
/// # Arguments
/// * `ptr` - Pointer to the object data (not the header)
/// * `size` - Size of the object data (excluding header)
///
/// The destructor must not free the object itself (the ARC runtime handles
/// deallocation after the destructor returns). It should only release any
/// resources owned by the object (nested allocations, file handles, etc.).
pub type ArcDropFn = unsafe extern "C" fn(ptr: *mut u8, size: usize);

/// Allocate memory with ARC header (no destructor)
///
/// # Arguments
/// * `size` - Size of the object data (excluding header)
/// * `align` - Alignment requirement (minimum 8 bytes)
///
/// # Returns
/// Pointer to the object data (not the header)
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
/// Pointer to the object data (not the header)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn arc_alloc_with_drop(
    size: usize,
    align: usize,
    drop_fn: Option<ArcDropFn>,
) -> *mut u8 {
    unsafe {
        let align = align.max(MIN_ALIGN);
        let total_size = HEADER_SIZE + size;

        // Create layout for allocation (header + data)
        let layout = match Layout::from_size_align(total_size, align) {
            Ok(layout) => layout,
            Err(_) => return ptr::null_mut(),
        };

        // Allocate memory
        let ptr = alloc(layout);
        if ptr.is_null() {
            return ptr::null_mut();
        }

        // Initialize reference count to 1
        let ref_count_ptr = ptr.add(REFCOUNT_OFFSET) as *mut AtomicUsize;
        (*ref_count_ptr).store(1, Ordering::Relaxed);

        // Store destructor function pointer (null if none)
        let drop_fn_ptr = ptr.add(DROPFN_OFFSET) as *mut *mut u8;
        (*drop_fn_ptr) = drop_fn
            .map(|f| f as *mut u8)
            .unwrap_or(ptr::null_mut());

        // Return pointer to data (after header)
        ptr.add(HEADER_SIZE)
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

        // Get pointer to reference count (before data)
        let ref_count_ptr = ptr.sub(HEADER_SIZE).add(REFCOUNT_OFFSET) as *mut AtomicUsize;

        // Atomic increment with overflow protection
        let old_count = (*ref_count_ptr).fetch_add(1, Ordering::Relaxed);
        if old_count >= (usize::MAX / 2) {
            panic!("arc_retain: reference count overflow");
        }

        // Sanity check: reference count should never be zero when retaining
        debug_assert!(old_count > 0, "arc_retain called on deallocated object");
    }
}

/// Decrement reference count (release)
///
/// When the reference count reaches zero:
/// 1. The destructor (if registered) is called with the object pointer and size
/// 2. The memory is deallocated
///
/// # Arguments
/// * `ptr` - Pointer to the object data (not the header)
/// * `size` - Size of the object data (for deallocation and destructor)
/// * `align` - Alignment of the object (for deallocation)
///
/// # Safety
/// If reference count reaches zero, the destructor runs and memory is deallocated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn arc_release(ptr: *mut u8, size: usize, align: usize) {
    unsafe {
        if ptr.is_null() {
            return;
        }

        let align = align.max(MIN_ALIGN);

        // Get pointer to reference count
        let header_ptr = ptr.sub(HEADER_SIZE);
        let ref_count_ptr = header_ptr.add(REFCOUNT_OFFSET) as *mut AtomicUsize;

        // Atomic decrement
        let old_count = (*ref_count_ptr).fetch_sub(1, Ordering::Release);

        debug_assert!(
            old_count > 0,
            "arc_release called with zero reference count"
        );

        // If this was the last reference, call destructor and deallocate
        if old_count == 1 {
            // Acquire fence to synchronize with other threads
            std::sync::atomic::fence(Ordering::Acquire);

            // Call user-defined destructor if registered
            let drop_fn_ptr = header_ptr.add(DROPFN_OFFSET) as *mut *mut u8;
            let drop_fn_raw = *drop_fn_ptr;
            if !drop_fn_raw.is_null() {
                let drop_fn: ArcDropFn = std::mem::transmute(drop_fn_raw);
                drop_fn(ptr, size);
            }

            // Deallocate the entire allocation (header + data)
            let total_size = HEADER_SIZE + size;
            let layout = Layout::from_size_align_unchecked(total_size, align);
            dealloc(header_ptr, layout);
        }
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

        let ref_count_ptr = ptr
            .sub(HEADER_SIZE)
            .add(REFCOUNT_OFFSET) as *mut AtomicUsize;
        (*ref_count_ptr).load(Ordering::Relaxed)
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
///
/// This allows attaching a drop function after allocation, which is useful
/// for types where the destructor is determined after construction (e.g.,
/// trait objects or dynamically dispatched cleanup).
///
/// # Arguments
/// * `ptr` - Pointer to the object data (not the header)
/// * `drop_fn` - The destructor to set, or None to clear
///
/// # Safety
/// The caller must ensure `ptr` is a valid ARC allocation and that no
/// other thread is concurrently releasing the same pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn arc_set_drop_fn(ptr: *mut u8, drop_fn: Option<ArcDropFn>) {
    unsafe {
        if ptr.is_null() {
            return;
        }

        let drop_fn_ptr = ptr
            .sub(HEADER_SIZE)
            .add(DROPFN_OFFSET) as *mut *mut u8;
        (*drop_fn_ptr) = drop_fn
            .map(|f| f as *mut u8)
            .unwrap_or(ptr::null_mut());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize};

    // Track destructor calls for testing
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
}
