//! Phase 9 Cooperative Async Runtime E2E Test Suite.
//!
//! Validates:
//! - Cooperative async task executor (`AsyncExecutor`).
//! - Multi-task scheduling, non-blocking polling, and task completion tracking.
//! - Event multiplexer platform detection (`EventMuxKind`).

#![allow(dead_code, unused_imports)]

use adesh_runtime::async_rt::{AsyncExecutor, EventMuxKind};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn test_async_executor_multi_task_execution() {
    let exec = AsyncExecutor::new();
    let counter = Arc::new(AtomicUsize::new(0));

    for i in 1..=5 {
        let cnt = counter.clone();
        exec.spawn(async move {
            cnt.fetch_add(i, Ordering::SeqCst);
        });
    }

    assert_eq!(exec.active_tasks(), 5);
    exec.run_until_stalled();

    assert_eq!(exec.active_tasks(), 0);
    // Sum of 1..=5 is 15
    assert_eq!(counter.load(Ordering::SeqCst), 15);
}

#[test]
fn test_event_multiplexer_platform_selection() {
    let mux = EventMuxKind::for_current_platform();
    if cfg!(target_os = "linux") {
        assert_eq!(mux, EventMuxKind::Epoll);
    } else if cfg!(target_os = "macos") {
        assert_eq!(mux, EventMuxKind::Kqueue);
    } else if cfg!(target_os = "windows") {
        assert_eq!(mux, EventMuxKind::Iocp);
    }
}
