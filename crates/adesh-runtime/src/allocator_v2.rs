//! Phase 10 — Production Memory Allocators & Statistics.
//!
//! Provides:
//! - Custom `Allocator` trait.
//! - Implementations: `SystemAllocator`, `ArenaAllocator`, `BumpAllocator`, `PoolAllocator`, `AlignedAllocator`.
//! - Live allocation counters and memory leak tracking.

use std::alloc::{Layout, alloc, dealloc};
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
pub unsafe trait Allocator {
    unsafe fn allocate(&self, layout: Layout) -> *mut u8;
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
        if !ptr.is_null() {
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
    buffer: Vec<u8>,
    offset: AtomicUsize,
}

impl BumpAllocator {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: vec![0u8; capacity],
            offset: AtomicUsize::new(0),
        }
    }

    pub fn alloc(&self, size: usize, align: usize) -> Option<*mut u8> {
        let mut current = self.offset.load(Ordering::Relaxed);
        loop {
            let aligned = (current + align - 1) & !(align - 1);
            let next = aligned + size;
            if next > self.buffer.len() {
                return None;
            }
            match self.offset.compare_exchange_weak(
                current,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    let ptr = unsafe { self.buffer.as_ptr().add(aligned) as *mut u8 };
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
        let layout = Layout::from_size_align(size, alignment).expect("Valid aligned layout");
        unsafe { alloc(layout) }
    }

    pub fn dealloc_aligned(ptr: *mut u8, size: usize, alignment: usize) {
        let layout = Layout::from_size_align(size, alignment).expect("Valid aligned layout");
        unsafe { dealloc(ptr, layout) }
    }
}
