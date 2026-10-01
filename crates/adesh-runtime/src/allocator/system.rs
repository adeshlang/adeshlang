//! System Page Allocator.

use std::alloc::{Layout, alloc, dealloc, realloc};
use std::ptr::null_mut;

/// Zero-dependency System Page Allocator wrapping global OS virtual memory.
pub struct SystemAllocator;

impl SystemAllocator {
    pub const fn new() -> Self {
        Self
    }

    #[inline]
    pub fn allocate(&self, size: usize, align: usize) -> *mut u8 {
        let align = if align == 0 { 8 } else { align };
        if let Ok(layout) = Layout::from_size_align(size, align) {
            unsafe { alloc(layout) }
        } else {
            null_mut()
        }
    }

    #[inline]
    pub fn deallocate(&self, ptr: *mut u8, size: usize, align: usize) {
        if ptr.is_null() || size == 0 {
            return;
        }
        let align = if align == 0 { 8 } else { align };
        if let Ok(layout) = Layout::from_size_align(size, align) {
            unsafe { dealloc(ptr, layout) };
        }
    }

    #[inline]
    pub fn reallocate(
        &self,
        ptr: *mut u8,
        old_size: usize,
        new_size: usize,
        align: usize,
    ) -> *mut u8 {
        let align = if align == 0 { 8 } else { align };
        if ptr.is_null() {
            return self.allocate(new_size, align);
        }
        if let Ok(old_layout) = Layout::from_size_align(old_size, align) {
            unsafe { realloc(ptr, old_layout, new_size) }
        } else {
            null_mut()
        }
    }
}

impl Default for SystemAllocator {
    fn default() -> Self {
        Self::new()
    }
}
