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
        if layout.size() == 0 {
            return std::ptr::null_mut();
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
        if !ptr.is_null() && layout.size() > 0 {
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

/// Fast monotonic bump allocator backed by a raw heap buffer (no `&Vec` aliasing UB).
pub struct BumpAllocator {
    base: *mut u8,
    capacity: usize,
    layout: Option<Layout>,
    offset: AtomicUsize,
}

unsafe impl Send for BumpAllocator {}
unsafe impl Sync for BumpAllocator {}

impl BumpAllocator {
    pub fn with_capacity(capacity: usize) -> Self {
        if capacity == 0 {
            return Self {
                base: std::ptr::null_mut(),
                capacity: 0,
                layout: None,
                offset: AtomicUsize::new(0),
            };
        }
        // Align backing buffer to 64 bytes so common alignments up to cache-line size
        // need minimal padding; `alloc()` also aligns the actual pointer address.
        let layout =
            Layout::from_size_align(capacity, 64).or_else(|_| Layout::from_size_align(capacity, 1));
        match layout {
            Ok(layout) => {
                let base = unsafe { std::alloc::alloc_zeroed(layout) };
                if base.is_null() {
                    Self {
                        base: std::ptr::null_mut(),
                        capacity: 0,
                        layout: None,
                        offset: AtomicUsize::new(0),
                    }
                } else {
                    Self {
                        base,
                        capacity,
                        layout: Some(layout),
                        offset: AtomicUsize::new(0),
                    }
                }
            }
            Err(_) => Self {
                base: std::ptr::null_mut(),
                capacity: 0,
                layout: None,
                offset: AtomicUsize::new(0),
            },
        }
    }

    pub fn alloc(&self, size: usize, align: usize) -> Option<*mut u8> {
        if self.base.is_null() || size == 0 || align == 0 || !align.is_power_of_two() {
            return None;
        }
        let base_addr = self.base as usize;
        let mut current = self.offset.load(Ordering::Acquire);
        loop {
            let cur_addr = base_addr.checked_add(current)?;
            let aligned_addr = cur_addr.checked_add(align - 1)? & !(align - 1);
            let aligned_off = aligned_addr.checked_sub(base_addr)?;
            let next = aligned_off.checked_add(size)?;
            if next > self.capacity {
                return None;
            }
            match self.offset.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return Some(unsafe { self.base.add(aligned_off) });
                }
                Err(actual) => current = actual,
            }
        }
    }

    /// Reset the bump cursor to 0. Callers must ensure no previously returned
    /// pointers remain in use.
    pub fn reset(&self) {
        self.offset.store(0, Ordering::Release);
    }

    pub fn bytes_used(&self) -> usize {
        self.offset.load(Ordering::Acquire)
    }
}

impl Drop for BumpAllocator {
    fn drop(&mut self) {
        if let Some(layout) = self.layout
            && !self.base.is_null()
            && layout.size() > 0
        {
            unsafe {
                dealloc(self.base, layout);
            }
        }
    }
}

/// Aligned allocator ensuring cache-line (64-byte) or page alignment for SIMD and DMA.
pub struct AlignedAllocator;

impl AlignedAllocator {
    pub fn alloc_aligned(size: usize, alignment: usize) -> *mut u8 {
        if size == 0 {
            return std::ptr::null_mut();
        }
        match Layout::from_size_align(size, alignment) {
            Ok(layout) => unsafe { alloc(layout) },
            Err(_) => std::ptr::null_mut(),
        }
    }

    pub fn dealloc_aligned(ptr: *mut u8, size: usize, alignment: usize) {
        if ptr.is_null() || size == 0 {
            return;
        }
        if let Ok(layout) = Layout::from_size_align(size, alignment) {
            unsafe { dealloc(ptr, layout) };
        }
    }
}
