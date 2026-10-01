//! Fixed-Size Pool / Slab Allocator.

use std::alloc::Layout;
use std::ptr::null_mut;

/// Fixed-size Pool / Slab Allocator with $O(1)$ allocate and $O(1)$ free via free-list.
pub struct PoolAllocator {
    chunk_size: usize,
    capacity: usize,
    buffer: *mut u8,
    free_list: Vec<*mut u8>,
}

impl PoolAllocator {
    pub fn new(chunk_size: usize, capacity: usize) -> Self {
        let chunk_size = chunk_size.max(std::mem::size_of::<usize>());
        let total_bytes = chunk_size * capacity;
        let layout = Layout::from_size_align(total_bytes, 16).unwrap();
        let buffer = unsafe { std::alloc::alloc(layout) };

        let mut free_list = Vec::with_capacity(capacity);
        if !buffer.is_null() {
            for i in 0..capacity {
                unsafe {
                    free_list.push(buffer.add(i * chunk_size));
                }
            }
        }

        Self {
            chunk_size,
            capacity,
            buffer,
            free_list,
        }
    }

    pub fn alloc(&mut self) -> *mut u8 {
        self.free_list.pop().unwrap_or(null_mut())
    }

    pub fn free(&mut self, ptr: *mut u8) {
        if !ptr.is_null() {
            self.free_list.push(ptr);
        }
    }

    pub fn available(&self) -> usize {
        self.free_list.len()
    }
}

impl Drop for PoolAllocator {
    fn drop(&mut self) {
        if !self.buffer.is_null() {
            let total_bytes = self.chunk_size * self.capacity;
            let layout = Layout::from_size_align(total_bytes, 16).unwrap();
            unsafe { std::alloc::dealloc(self.buffer, layout) };
            self.buffer = null_mut();
        }
    }
}
