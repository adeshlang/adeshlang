//! Phase 10 Native Crash & Signal Handling E2E Test Suite.
//!
//! Validates:
//! - Crash reporter signal classification and formatted diagnostic report.

#![allow(dead_code, unused_imports)]

use adesh_codegen::crash::{CrashReporter, CrashSignal, CrashStackFrame};

#[test]
fn test_crash_reporter_diagnostics() {
    let reporter = CrashReporter::new("phase10-build", "x86_64-pc-windows-msvc");
    let report = reporter.create_report(
        CrashSignal::AccessViolation,
        0xDEAD_BEEF,
        0x7FFF_1234_5678,
        1,
        &[0x7FFF_1234_5678, 0x7FFF_1234_0000],
        None,
    );

    let diagnostic = report.format_diagnostic();
    assert!(diagnostic.contains("Adesh Native Crash Report"));
    assert!(diagnostic.contains("Access Violation"));
    assert!(diagnostic.contains("0x00000000deadbeef"));
}
