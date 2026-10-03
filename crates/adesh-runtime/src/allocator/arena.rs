//! High-Performance Arena / Bump Allocator.

use std::alloc::Layout;
use std::ptr::null_mut;

/// Monotonic Bump / Arena Allocator for rapid scoped allocations with $O(1)$ batch free.
pub struct ArenaAllocator {
    /// Each chunk records the exact size it was allocated with. Audit fix:
    /// chunks larger than `chunk_size` (from oversized `alloc` calls) used
    /// to be deallocated with the *default* chunk layout, which is
    /// undefined behavior.
    chunks: Vec<(*mut u8, usize, usize)>,
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
        arena.grow(chunk_size, 16);
        arena
    }

    /// Allocate a new chunk of at least `min_size` bytes.
    /// Returns `false` (leaving the arena untouched) if the allocation failed.
    fn grow(&mut self, min_size: usize, min_align: usize) -> bool {
        let alloc_sz = min_size.max(self.chunk_size);
        let alignment = min_align.max(16);
        let Ok(layout) = Layout::from_size_align(alloc_sz, alignment) else {
            return false;
        };
        // SAFETY: layout was just validated.
        let ptr = unsafe { std::alloc::alloc(layout) };
        if ptr.is_null() {
            return false;
        }
        self.chunks.push((ptr, alloc_sz, alignment));
        self.current_offset = 0;
        true
    }

    pub fn alloc(&mut self, size: usize, align: usize) -> *mut u8 {
        if size == 0 {
            return null_mut();
        }
        let align = align.max(1);
        if !align.is_power_of_two() {
            return null_mut();
        }
        let Some(&(current_chunk, current_size, current_align)) = self.chunks.last() else {
            return null_mut();
        };

        // Align the current offset (overflow-safe: checked adds below bail
        // out to a fresh chunk instead of wrapping).
        let Some(rounded_offset) = self.current_offset.checked_add(align - 1) else {
            return self.alloc_new_chunk(size, align);
        };
        let aligned_offset = rounded_offset & !(align - 1);
        let Some(fit_end) = aligned_offset.checked_add(size) else {
            // Offset arithmetic would overflow; fall through to growing.
            return self.alloc_new_chunk(size, align);
        };
        if current_align >= align && fit_end <= current_size {
            self.current_offset = fit_end;
            // SAFETY: fit_end is within [0, chunk_size] of the current chunk.
            unsafe { current_chunk.add(aligned_offset) }
        } else {
            self.alloc_new_chunk(size, align)
        }
    }

    /// Service an allocation that does not fit in the current chunk by
    /// growing. Audit fix: when `grow` failed (OOM), the old code still
    /// returned the *current* chunk's base pointer while advancing the
    /// bump offset past its end, handing out memory that was already
    /// allocated. On failure we now return null.
    fn alloc_new_chunk(&mut self, size: usize, align: usize) -> *mut u8 {
        if !align.is_power_of_two() || !self.grow(size, align) {
            return null_mut();
        }
        let Some(&(new_chunk, new_size, _)) = self.chunks.last() else {
            return null_mut();
        };
        debug_assert!(size <= new_size);
        self.current_offset = size;
        new_chunk
    }

    pub fn reset(&mut self) {
        self.current_offset = 0;
        if self.chunks.len() > 1 {
            // Keep first chunk, deallocate the rest — each with the exact
            // layout it was allocated with.
            let first = self.chunks[0];
            for &(chunk, size, align) in &self.chunks[1..] {
                if let Ok(layout) = Layout::from_size_align(size, align) {
                    // SAFETY: chunk was allocated with exactly this layout.
                    unsafe { std::alloc::dealloc(chunk, layout) };
                }
            }
            self.chunks.clear();
            self.chunks.push(first);
        }
    }
}

impl Drop for ArenaAllocator {
    fn drop(&mut self) {
        for &(chunk, size, align) in &self.chunks {
            if let Ok(layout) = Layout::from_size_align(size, align) {
                // SAFETY: chunk was allocated with exactly this layout.
                unsafe { std::alloc::dealloc(chunk, layout) };
            }
        }
        self.chunks.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::ArenaAllocator;

    #[test]
    fn arena_honors_large_alignment_and_oversized_chunks() {
        let mut arena = ArenaAllocator::new(4096);
        let ptr = arena.alloc(8192, 64);
        assert!(!ptr.is_null());
        assert_eq!(ptr as usize % 64, 0);
    }

    #[test]
    fn arena_reset_reuses_first_chunk_and_rejects_bad_alignment() {
        let mut arena = ArenaAllocator::new(4096);
        let first = arena.alloc(32, 16);
        assert!(!first.is_null());
        assert!(arena.alloc(32, 3).is_null());
        arena.reset();
        assert_eq!(arena.alloc(32, 16), first);
    }
}
