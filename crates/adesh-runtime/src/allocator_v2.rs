//! Phase 10 — Production Memory Allocators & Statistics.
//!
//! Provides:
//! - Custom `Allocator` trait.
//! - Implementations: `SystemAllocator`, `ArenaAllocator`, `BumpAllocator`, `PoolAllocator`, `AlignedAllocator`.
//! - Live allocation counters and memory leak tracking.

use std::alloc::{Layout, alloc, dealloc};
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Allocation metrics tracking memory consumption.
#[derive(Debug, Default)]
pub struct AllocStats {
    pub allocations: AtomicUsize,
    pub deallocations: AtomicUsize,
    pub bytes_allocated: AtomicUsize,
    pub bytes_deallocated: AtomicUsize,
}

impl AllocStats {
    pub fn live_bytes(&self) -> usize {
        let alloc = self.bytes_allocated.load(Ordering::Relaxed);
        let dealloc = self.bytes_deallocated.load(Ordering::Relaxed);
        alloc.saturating_sub(dealloc)
    }

    pub fn live_allocations(&self) -> usize {
        let alloc = self.allocations.load(Ordering::Relaxed);
        let dealloc = self.deallocations.load(Ordering::Relaxed);
        alloc.saturating_sub(dealloc)
    }
}

/// Standard interface for custom memory allocators in Adesh runtime.
///
/// # Safety
/// Implementors must return pointers valid for the requested `Layout` and must
/// only free memory they allocated themselves.
pub unsafe trait Allocator {
    /// # Safety
    /// `layout` must be a valid allocation request; the result must be
    /// released through `deallocate` with the same layout.
    unsafe fn allocate(&self, layout: Layout) -> *mut u8;
    /// # Safety
    /// `ptr` must come from `allocate` on this allocator with the same
    /// `layout`, and must not be freed twice.
    unsafe fn deallocate(&self, ptr: *mut u8, layout: Layout);
}

/// Standard OS Heap Allocator with tracking statistics.
pub struct SystemAllocator {
    pub stats: AllocStats,
}

impl SystemAllocator {
    pub fn new() -> Self {
        Self {
            stats: AllocStats::default(),
        }
    }
}

impl Default for SystemAllocator {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Allocator for SystemAllocator {
    unsafe fn allocate(&self, layout: Layout) -> *mut u8 {
        // Zero-sized layouts must not reach `std::alloc::alloc` (UB).
        if layout.size() == 0 {
            return core::ptr::null_mut();
        }
        let ptr = unsafe { alloc(layout) };
        if !ptr.is_null() {
            self.stats.allocations.fetch_add(1, Ordering::Relaxed);
            self.stats
                .bytes_allocated
                .fetch_add(layout.size(), Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn deallocate(&self, ptr: *mut u8, layout: Layout) {
        if !ptr.is_null() && layout.size() != 0 {
            self.stats.deallocations.fetch_add(1, Ordering::Relaxed);
            self.stats
                .bytes_deallocated
                .fetch_add(layout.size(), Ordering::Relaxed);
            unsafe {
                dealloc(ptr, layout);
            }
        }
    }
}

/// Fast monotonic bump allocator.
pub struct BumpAllocator {
    /// Backing storage. Interior mutability via `UnsafeCell` so `alloc` can
    /// hand out `*mut u8` into the buffer through `&self`.
    buffer: UnsafeCell<Vec<u8>>,
    offset: AtomicUsize,
}

// SAFETY: `offset` serializes bump allocation; callers must not alias
// overlapping mutable slices from concurrent `alloc` results.
unsafe impl Sync for BumpAllocator {}

impl BumpAllocator {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: UnsafeCell::new(vec![0u8; capacity]),
            offset: AtomicUsize::new(0),
        }
    }

    pub fn alloc(&self, size: usize, align: usize) -> Option<*mut u8> {
        if size == 0 || align == 0 || !align.is_power_of_two() {
            return None;
        }
        // Align the absolute address (base + offset), not just the offset —
        // aligning the offset alone is wrong when `base` is not align-aligned.
        let base = unsafe { (*self.buffer.get()).as_ptr() as usize };
        let capacity = unsafe { (*self.buffer.get()).len() };
        let mut current = self.offset.load(Ordering::Relaxed);
        loop {
            let abs = base.checked_add(current)?;
            let aligned_abs = (abs + align - 1) & !(align - 1);
            let padding = aligned_abs - abs;
            let next = current.checked_add(padding)?.checked_add(size)?;
            if next > capacity {
                return None;
            }
            match self.offset.compare_exchange_weak(
                current,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    let ptr = aligned_abs as *mut u8;
                    return Some(ptr);
                }
                Err(actual) => current = actual,
            }
        }
    }

    pub fn reset(&self) {
        self.offset.store(0, Ordering::Relaxed);
    }

    pub fn bytes_used(&self) -> usize {
        self.offset.load(Ordering::Relaxed)
    }
}

/// Aligned allocator ensuring cache-line (64-byte) or page alignment for SIMD and DMA.
pub struct AlignedAllocator;

impl AlignedAllocator {
    pub fn alloc_aligned(size: usize, alignment: usize) -> *mut u8 {
        if size == 0 {
            return core::ptr::null_mut();
        }
        let Ok(layout) = Layout::from_size_align(size, alignment) else {
            return core::ptr::null_mut();
        };
        unsafe { alloc(layout) }
    }

    pub fn dealloc_aligned(ptr: *mut u8, size: usize, alignment: usize) {
        if ptr.is_null() || size == 0 {
            return;
        }
        if let Ok(layout) = Layout::from_size_align(size, alignment) {
            unsafe { dealloc(ptr, layout) }
        }
    }
}
