//! Fixed-Size Pool / Slab Allocator.

use std::alloc::Layout;
use std::ptr::null_mut;

/// Fixed-size Pool / Slab Allocator with $O(1)$ allocate and $O(1)$ free via free-list.
pub struct PoolAllocator {
    chunk_size: usize,
    capacity: usize,
    buffer: *mut u8,
    free_list: Vec<*mut u8>,
    free_slots: Vec<bool>,
}

impl PoolAllocator {
    pub fn new(chunk_size: usize, capacity: usize) -> Self {
        let requested_chunk_size = chunk_size.max(std::mem::size_of::<usize>());
        // Keep every chunk aligned to the allocator's 16-byte base alignment.
        let chunk_size = requested_chunk_size
            .checked_add(15)
            .map(|size| size & !15)
            .unwrap_or(usize::MAX);
        // Audit fix: `capacity == 0` used to ask the allocator for a
        // zero-size layout, which is undefined behavior for `alloc`; the
        // old `Layout::unwrap()`s could also panic on overflow. An empty
        // pool now simply has no buffer and no free entries.
        let total_bytes = chunk_size.saturating_mul(capacity);
        let (buffer, cap) = if total_bytes == 0 {
            (null_mut(), 0)
        } else {
            match Layout::from_size_align(total_bytes, 16) {
                Ok(layout) => {
                    // SAFETY: layout was just validated.
                    let ptr = unsafe { std::alloc::alloc(layout) };
                    if ptr.is_null() {
                        (null_mut(), 0)
                    } else {
                        (ptr, capacity)
                    }
                }
                Err(_) => (null_mut(), 0),
            }
        };

        let mut free_list = Vec::with_capacity(cap);
        if !buffer.is_null() {
            for i in 0..cap {
                // SAFETY: i < cap, so the offset stays inside the buffer.
                unsafe {
                    free_list.push(buffer.add(i * chunk_size));
                }
            }
        }

        Self {
            chunk_size,
            capacity: cap,
            buffer,
            free_list,
            free_slots: vec![true; cap],
        }
    }

    pub fn alloc(&mut self) -> *mut u8 {
        let Some(ptr) = self.free_list.pop() else {
            return null_mut();
        };
        let offset = (ptr as usize).wrapping_sub(self.buffer as usize);
        let slot = offset / self.chunk_size;
        if let Some(is_free) = self.free_slots.get_mut(slot) {
            *is_free = false;
        }
        ptr
    }

    pub fn free(&mut self, ptr: *mut u8) {
        if ptr.is_null() {
            return;
        }
        // Audit fix: only accept pointers that live inside this pool's
        // buffer and are chunk-aligned; foreign or stale pointers used to be
        // pushed blindly, silently corrupting the free list.
        let base = self.buffer as usize;
        let end = base.saturating_add(self.chunk_size.saturating_mul(self.capacity));
        let offset = (ptr as usize).wrapping_sub(base);
        let in_range = (ptr as usize) >= base && (ptr as usize) < end;
        let chunk_aligned = offset % self.chunk_size == 0;
        if in_range && chunk_aligned {
            let slot = offset / self.chunk_size;
            if let Some(is_free) = self.free_slots.get_mut(slot) {
                if !*is_free && self.free_list.len() < self.capacity {
                    *is_free = true;
                    self.free_list.push(ptr);
                }
            }
        }
    }

    pub fn available(&self) -> usize {
        self.free_list.len()
    }
}

impl Drop for PoolAllocator {
    fn drop(&mut self) {
        if !self.buffer.is_null() {
            let total_bytes = self.chunk_size.saturating_mul(self.capacity);
            if let Ok(layout) = Layout::from_size_align(total_bytes, 16) {
                // SAFETY: buffer was allocated with exactly this layout.
                unsafe { std::alloc::dealloc(self.buffer, layout) };
            }
            self.buffer = null_mut();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PoolAllocator;

    #[test]
    fn pool_rejects_duplicate_and_foreign_frees() {
        let mut pool = PoolAllocator::new(16, 2);
        let a = pool.alloc();
        let b = pool.alloc();
        assert!(!a.is_null() && !b.is_null() && a != b);
        assert_eq!(pool.available(), 0);

        pool.free(a);
        pool.free(a);
        pool.free(1usize as *mut u8);
        assert_eq!(pool.available(), 1);
        assert_eq!(pool.alloc(), a);
        assert_eq!(pool.alloc(), std::ptr::null_mut());
    }

    #[test]
    fn pool_zero_capacity_is_empty() {
        let mut pool = PoolAllocator::new(0, 0);
        assert!(pool.alloc().is_null());
        assert_eq!(pool.available(), 0);
    }

    #[test]
    fn pool_rounds_stride_for_aligned_chunks() {
        let mut pool = PoolAllocator::new(24, 2);
        let a = pool.alloc();
        let b = pool.alloc();
        assert_eq!(a as usize % 16, 0);
        assert_eq!(b as usize % 16, 0);
    }
}
