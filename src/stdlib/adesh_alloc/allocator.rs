//! Allocator Trait
//!
//! Defines the allocator abstraction for heap allocation.
//! Backends can plug in their own allocator implementations.

use crate::stdlib::adesh_core::layout::Layout;

/// Memory allocation error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllocError;

/// Core allocator trait
pub trait Allocator {
    /// Allocates memory with the given layout
    ///
    /// # Safety
    /// The returned pointer, if non-null, must point to valid memory
    /// matching the requested layout.
    unsafe fn alloc(&self, layout: Layout) -> Result<*mut u8, AllocError>;

    /// Deallocates memory at the given pointer
    ///
    /// # Safety
    /// - `ptr` must have been allocated by this allocator
    /// - `layout` must be the same as used for allocation
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout);

    /// Reallocates memory
    ///
    /// # Safety
    /// - `ptr` must have been allocated by this allocator
    /// - `old_layout` must be the layout used for the original allocation
    /// - The returned pointer, if non-null, must point to valid memory
    unsafe fn realloc(
        &self,
        ptr: *mut u8,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<*mut u8, AllocError> {
        // Default implementation: allocate new, copy, deallocate old
        let new_ptr = unsafe { self.alloc(new_layout)? };
        if !new_ptr.is_null() && !ptr.is_null() {
            let size = old_layout.size().min(new_layout.size());
            unsafe {
                std::ptr::copy_nonoverlapping(ptr, new_ptr, size);
                self.dealloc(ptr, old_layout);
            }
        }
        Ok(new_ptr)
    }

    /// Allocates zeroed memory
    ///
    /// # Safety
    /// Same as `alloc`, but memory is zeroed
    unsafe fn alloc_zeroed(&self, layout: Layout) -> Result<*mut u8, AllocError> {
        let ptr = unsafe { self.alloc(layout)? };
        if !ptr.is_null() {
            unsafe { std::ptr::write_bytes(ptr, 0, layout.size()) };
        }
        Ok(ptr)
    }
}

/// Global allocator using the system allocator
pub struct GlobalAllocator;

unsafe impl Send for GlobalAllocator {}
unsafe impl Sync for GlobalAllocator {}

impl Allocator for GlobalAllocator {
    unsafe fn alloc(&self, layout: Layout) -> Result<*mut u8, AllocError> {
        unsafe {
            let ptr = std::alloc::alloc(std::alloc::Layout::from_size_align_unchecked(
                layout.size(),
                layout.align(),
            ));
            if ptr.is_null() {
                Err(AllocError)
            } else {
                Ok(ptr)
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe {
            std::alloc::dealloc(
                ptr,
                std::alloc::Layout::from_size_align_unchecked(layout.size(), layout.align()),
            );
        }
    }

    unsafe fn realloc(
        &self,
        ptr: *mut u8,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<*mut u8, AllocError> {
        unsafe {
            let new_ptr = std::alloc::realloc(
                ptr,
                std::alloc::Layout::from_size_align_unchecked(
                    old_layout.size(),
                    old_layout.align(),
                ),
                new_layout.size(),
            );
            if new_ptr.is_null() {
                Err(AllocError)
            } else {
                Ok(new_ptr)
            }
        }
    }
}

/// Returns a reference to the global allocator
pub fn global_allocator() -> &'static GlobalAllocator {
    &GlobalAllocator
}
