//! Phase 10 Memory Allocators & Leak Tracking E2E Test Suite.
//!
//! Validates:
//! - Custom Allocator trait with SystemAllocator tracking live allocations.
//! - BumpAllocator monotonic sequential allocation and reset.
//! - AlignedAllocator cache-line/page alignment guarantees.

#![allow(dead_code, unused_imports)]

use adesh_runtime::allocator_v2::{AlignedAllocator, Allocator, BumpAllocator, SystemAllocator};
use std::alloc::Layout;

#[test]
fn test_system_allocator_tracking() {
    let alloc = SystemAllocator::new();
    let layout = Layout::from_size_align(64, 8).unwrap();

    unsafe {
        let ptr = alloc.allocate(layout);
        assert!(!ptr.is_null());
        assert_eq!(alloc.stats.live_allocations(), 1);
        assert_eq!(alloc.stats.live_bytes(), 64);

        alloc.deallocate(ptr, layout);
        assert_eq!(alloc.stats.live_allocations(), 0);
        assert_eq!(alloc.stats.live_bytes(), 0);
    }
}

#[test]
fn test_bump_allocator_sequential() {
    let bump = BumpAllocator::with_capacity(1024);

    let ptr1 = bump.alloc(32, 8).expect("bump alloc 1");
    let ptr2 = bump.alloc(64, 8).expect("bump alloc 2");

    assert!(!ptr1.is_null());
    assert!(!ptr2.is_null());
    assert!(bump.bytes_used() >= 96);

    bump.reset();
    assert_eq!(bump.bytes_used(), 0);
}

#[test]
fn test_aligned_allocator_alignment() {
    let size = 256;
    let align = 64; // Cache-line alignment
    let ptr = AlignedAllocator::alloc_aligned(size, align);
    assert!(!ptr.is_null());
    assert_eq!((ptr as usize) % align, 0);

    AlignedAllocator::dealloc_aligned(ptr, size, align);
}
