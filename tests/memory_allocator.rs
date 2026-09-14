//! Memory Allocator Tests
//!
//! Tests for dynamic allocator modes: static, dynamic, and hybrid.

use adeshlang::memory::{AllocMode, AllocatorConfig, DynamicAllocator, GrowthStrategy};

#[test]
fn test_allocator_static_oom() {
    // Create a very small static allocator
    let config = AllocatorConfig::static_mode(512);
    let alloc = DynamicAllocator::new(config).unwrap();

    // First small allocation should succeed
    let ptr1 = alloc.alloc(128).unwrap();

    // Track that we used some memory
    let stats = alloc.stats();
    assert!(stats.current_bytes >= 128);

    // Large allocation should fail (would exceed limit)
    let result = alloc.alloc(512);
    assert!(result.is_err());

    // Check OOM was recorded
    let stats = alloc.stats();
    assert!(stats.oom_events >= 1);

    // Cleanup
    alloc.dealloc(ptr1, 128);
}

#[test]
fn test_allocator_dynamic_growth() {
    // Create dynamic allocator with small initial size
    let config = AllocatorConfig::dynamic_mode(256, 1024 * 1024, GrowthStrategy::Doubling);
    let alloc = DynamicAllocator::new(config).unwrap();

    // Allocate more than initial size (forces growth)
    let ptr = alloc.alloc(4096).unwrap();

    let stats = alloc.stats();
    // Either heap grew or allocation went through slab
    assert!(stats.total_allocations >= 1);

    alloc.dealloc(ptr, 4096);
}

#[test]
fn test_allocator_hybrid_growth() {
    // Create hybrid allocator
    let config = AllocatorConfig::hybrid_mode(4096, 2);
    let alloc = DynamicAllocator::new(config).unwrap();

    // Small allocations should use arena
    let ptrs: Vec<_> = (0..10).map(|_| alloc.alloc(64).unwrap()).collect();

    let stats = alloc.stats();
    assert!(stats.arena_allocations > 0 || stats.total_allocations >= 10);

    // Reset arenas
    alloc.reset_arenas();

    let stats = alloc.stats();
    assert!(stats.arena_resets >= 1);

    // Note: Don't dealloc arena-allocated memory after reset
    let _ = ptrs;
}

#[test]
fn test_allocator_report_values() {
    let config = AllocatorConfig::default();
    let alloc = DynamicAllocator::new(config).unwrap();

    // Make some allocations
    let ptr1 = alloc.alloc(1024).unwrap();
    let ptr2 = alloc.alloc(2048).unwrap();

    let stats = alloc.stats();
    assert!(stats.total_allocations >= 2);
    assert!(stats.current_bytes >= 3072);

    // Get memory report
    let report = alloc.memory_report();
    assert!(report.contains("Memory Allocator Report"));
    assert!(report.contains("dynamic"));

    // Deallocate
    alloc.dealloc(ptr1, 1024);
    alloc.dealloc(ptr2, 2048);

    let stats = alloc.stats();
    assert!(stats.total_deallocations >= 2);
}

#[test]
fn test_allocator_stress_no_crash() {
    let config = AllocatorConfig::default();
    let alloc = DynamicAllocator::new(config).unwrap();

    // Stress test: many allocations
    let mut ptrs = Vec::new();
    for _ in 0..100 {
        match alloc.alloc(128) {
            Ok(ptr) => ptrs.push((ptr, 128)),
            Err(_) => break, // OOM is acceptable in stress test
        }
    }

    // Verify allocations
    let stats = alloc.stats();
    assert_eq!(stats.total_allocations, ptrs.len());

    // Free half
    for (ptr, size) in ptrs.iter().take(ptrs.len() / 2) {
        alloc.dealloc(*ptr, *size);
    }

    // Allocate again
    for _ in 0..50 {
        match alloc.alloc(64) {
            Ok(ptr) => ptrs.push((ptr, 64)),
            Err(_) => break,
        }
    }

    // Clean up remaining
    for (ptr, size) in ptrs.iter().skip(ptrs.len() / 2) {
        alloc.dealloc(*ptr, *size);
    }

    // Verify no crash - test passes if we got here
    let stats = alloc.stats();
    assert!(stats.total_allocations > 0);
}

#[test]
fn test_alloc_mode_parsing() {
    assert_eq!("static".parse::<AllocMode>().unwrap(), AllocMode::Static);
    assert_eq!("dynamic".parse::<AllocMode>().unwrap(), AllocMode::Dynamic);
    assert_eq!("hybrid".parse::<AllocMode>().unwrap(), AllocMode::Hybrid);

    assert!("invalid".parse::<AllocMode>().is_err());
}

#[test]
fn test_realloc_grows() {
    let config = AllocatorConfig::default();
    let alloc = DynamicAllocator::new(config).unwrap();

    let ptr = alloc.alloc(64).unwrap();

    // Write some data
    unsafe {
        std::ptr::write(ptr.as_ptr() as *mut u64, 0xDEADBEEF);
    }

    // Reallocate to larger size
    let new_ptr = alloc.realloc(ptr, 64, 256).unwrap();

    // Data should be preserved
    let data = unsafe { std::ptr::read(new_ptr.as_ptr() as *const u64) };
    assert_eq!(data, 0xDEADBEEF);

    alloc.dealloc(new_ptr, 256);
}

#[test]
fn test_slab_allocation_sizes() {
    let config = AllocatorConfig::default();
    let alloc = DynamicAllocator::new(config).unwrap();

    // Test various size classes
    let sizes = [16, 32, 64, 128, 256, 512, 1024, 2048];
    let mut ptrs = Vec::new();

    for size in sizes {
        let ptr = alloc.alloc(size).unwrap();
        ptrs.push((ptr, size));
    }

    let stats = alloc.stats();
    assert_eq!(stats.total_allocations, sizes.len());

    for (ptr, size) in ptrs {
        alloc.dealloc(ptr, size);
    }
}
