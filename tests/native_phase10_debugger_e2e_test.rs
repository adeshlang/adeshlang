//! Phase 10 Production Debugger & Native Backtrace E2E Test Suite.
//!
//! Validates:
//! - Programmatic backtrace capture through NativeBacktrace.
//! - Symbol and line location formatting.

#![allow(dead_code, unused_imports)]

use adesh_runtime::backtrace::NativeBacktrace;

#[test]
fn test_native_backtrace_capture_and_formatting() {
    let trace = NativeBacktrace::capture();
    assert!(!trace.frames.is_empty());

    let formatted = trace.format_trace();
    assert!(formatted.contains("Stack Backtrace:"));
    assert!(formatted.contains("main"));
}
