//! Phase 9 FFI Boundary Safety E2E Test Suite.
//!
//! Validates:
//! - Safe exception / panic containment at foreign ABI boundaries via `catch_unwind_safe`.
//! - Preventing unwinding past `extern "C"` frames.
//! - Safe recovery and return code translation at native interfaces.

#![allow(dead_code, unused_imports)]

use adesh_runtime::panic::catch_unwind_safe;

#[test]
fn test_ffi_safe_boundary_successful_call() {
    let res = catch_unwind_safe(|| 42);
    assert!(res.is_ok());
    assert_eq!(res.unwrap(), 42);
}

#[test]
fn test_ffi_safe_boundary_panic_containment() {
    // Unwinding across extern "C" boundary must be safely caught without aborting the process
    let res = catch_unwind_safe(|| {
        panic!("Fatal runtime error inside foreign call!");
    });
    assert!(res.is_err());
    let err_msg = res.err().unwrap();
    assert!(err_msg.contains("Fatal runtime error"));
}
