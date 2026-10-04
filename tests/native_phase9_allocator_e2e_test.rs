//! Phase 9 Memory Allocator Safety and Heap Instrumentation E2E Test Suite.
//!
//! Validates:
//! - Memory allocation safety and poisoning patterns.
//! - Use-after-free canary checks.
//! - Bounds protection checks.

#![allow(dead_code, unused_imports)]

use adesh_runtime::sanitizer_rt::{
    adesh_sanitizer_check_bounds, adesh_sanitizer_check_overflow, adesh_sanitizer_check_uaf,
    adesh_sanitizer_poison_memory,
};

#[test]
fn test_allocator_bounds_and_overflow_guards() {
    // Valid in-bounds access
    adesh_sanitizer_check_bounds(0, 10);
    adesh_sanitizer_check_bounds(9, 10);

    // Valid non-overflow arithmetic
    adesh_sanitizer_check_overflow(100, 1000);
}

#[test]
fn test_allocator_memory_poisoning_and_uaf_canary() {
    let mut buffer = vec![0u8; 32];
    let ptr = buffer.as_mut_ptr();

    // Prior to poisoning, UAF check must return false (not poisoned)
    assert!(!adesh_sanitizer_check_uaf(ptr));

    // Poison the buffer (simulating free / deallocation)
    adesh_sanitizer_poison_memory(ptr, 32);

    // After poisoning, UAF check must detect poison byte 0xAA
    assert!(adesh_sanitizer_check_uaf(ptr));
}
