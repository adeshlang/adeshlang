//! Phase 10 Production Async Runtime & Structured Concurrency E2E Test Suite.
//!
//! Validates:
//! - TaskScope structured lifecycle management.
//! - Cancellation token signaling.
//! - Joining tasks with timeout.

#![allow(dead_code, unused_imports)]

use adesh_runtime::concurrency_v2::TaskScope;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[test]
fn test_structured_task_scope_completion() {
    let scope = TaskScope::new();
    let counter = Arc::new(AtomicUsize::new(0));

    for _ in 0..5 {
        let cnt = counter.clone();
        scope.spawn(move |token| {
            if !token.is_cancelled() {
                cnt.fetch_add(10, Ordering::SeqCst);
            }
        });
    }

    let joined = scope.join_all(Duration::from_secs(2));
    assert!(joined);
    assert_eq!(counter.load(Ordering::SeqCst), 50);
    assert_eq!(scope.active_task_count(), 0);
}

#[test]
fn test_structured_task_scope_cancellation() {
    let scope = TaskScope::new();
    let token = scope.token();
    assert!(!token.is_cancelled());

    scope.cancel();
    assert!(token.is_cancelled());
}
