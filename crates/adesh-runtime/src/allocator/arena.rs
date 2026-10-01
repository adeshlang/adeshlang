//! High-Performance Arena / Bump Allocator.

use std::alloc::Layout;
use std::ptr::null_mut;

/// Monotonic Bump / Arena Allocator for rapid scoped allocations with $O(1)$ batch free.
pub struct ArenaAllocator {
    chunks: Vec<*mut u8>,
    chunk_size: usize,
    current_offset: usize,
}

impl ArenaAllocator {
    pub fn new(default_chunk_size: usize) -> Self {
        let chunk_size = default_chunk_size.max(4096);
        let mut arena = Self {
            chunks: Vec::new(),
            chunk_size,
            current_offset: 0,
        };
        arena.grow(chunk_size);
        arena
    }

    fn grow(&mut self, min_size: usize) {
        let alloc_sz = min_size.max(self.chunk_size);
        if let Ok(layout) = Layout::from_size_align(alloc_sz, 16) {
            let ptr = unsafe { std::alloc::alloc(layout) };
            if !ptr.is_null() {
                self.chunks.push(ptr);
                self.current_offset = 0;
            }
        }
    }

    pub fn alloc(&mut self, size: usize, align: usize) -> *mut u8 {
        if size == 0 {
            return null_mut();
        }
        let align = align.max(1);
        let current_chunk = match self.chunks.last() {
            Some(&p) => p,
            None => return null_mut(),
        };

        // Align current offset
        let aligned_offset = (self.current_offset + align - 1) & !(align - 1);
        if aligned_offset + size <= self.chunk_size {
            self.current_offset = aligned_offset + size;
            unsafe { current_chunk.add(aligned_offset) }
        } else {
            self.grow(size + align);
            let new_chunk = match self.chunks.last() {
                Some(&p) => p,
                None => return null_mut(),
            };
            self.current_offset = size;
            new_chunk
        }
    }

    pub fn reset(&mut self) {
        self.current_offset = 0;
        if self.chunks.len() > 1 {
            // Keep first chunk, deallocate rest
            let layout = Layout::from_size_align(self.chunk_size, 16).unwrap();
            for &chunk in &self.chunks[1..] {
                unsafe { std::alloc::dealloc(chunk, layout) };
            }
            self.chunks.truncate(1);
        }
    }
}

impl Drop for ArenaAllocator {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.chunk_size, 16).unwrap();
        for &chunk in &self.chunks {
            unsafe { std::alloc::dealloc(chunk, layout) };
        }
        self.chunks.clear();
    }
}
