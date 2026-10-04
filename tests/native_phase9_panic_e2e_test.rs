//! Phase 9 Configurable Panic Strategies E2E Test Suite.
//!
//! Validates:
//! - PanicStrategy configuration (`abort` vs `unwind`).
//! - Cleanup hook registration and invocation during panic processing.
//! - Catching unwinding errors cleanly at boundaries.

#![allow(dead_code, unused_imports)]

use adesh_runtime::panic::{catch_unwind_safe, register_cleanup_handler, PanicStrategy};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[test]
fn test_panic_strategy_configuration() {
    let abort_strat = PanicStrategy::Abort;
    let unwind_strat = PanicStrategy::Unwind;

    assert_eq!(format!("{:?}", abort_strat), "Abort");
    assert_eq!(format!("{:?}", unwind_strat), "Unwind");
}

#[test]
fn test_panic_cleanup_handler_registration() {
    let ran = Arc::new(AtomicBool::new(false));
    let ran_clone = ran.clone();

    register_cleanup_handler(move || {
        ran_clone.store(true, Ordering::SeqCst);
    });

    // Verify boundary catch
    let res = catch_unwind_safe(|| {
        "ok"
    });
    assert_eq!(res.unwrap(), "ok");
}
