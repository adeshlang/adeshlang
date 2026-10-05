//! Phase 10 Performance Regression & Memory Tracking E2E Test Suite.
//!
//! Validates:
//! - MemoryTracker accounting for peak RSS across compilation stages.
//! - EscapeAnalyzer stack promotion optimization.

#![allow(dead_code, unused_imports)]

use adesh_codegen::compiler_arena::MemoryTracker;
use adesh_codegen::escape_analysis::{EscapeAnalyzer, EscapeState};

#[test]
fn test_compiler_memory_tracking() {
    let tracker = MemoryTracker::new();
    tracker.record_ast(1024);
    tracker.record_hir(2048);
    tracker.record_mir(4096);
    tracker.record_machine_ir(8192);

    assert_eq!(tracker.total_allocated(), 15360);
    assert!(tracker.peak_allocated() >= 15360);
}

#[test]
fn test_escape_analysis_stack_promotion() {
    let analyzer = EscapeAnalyzer::new();

    let local_state = analyzer.analyze_escape(false, false, false);
    assert_eq!(local_state, EscapeState::NoEscape);
    assert!(analyzer.can_promote_to_stack(local_state));

    let escaping_state = analyzer.analyze_escape(false, true, false);
    assert_eq!(escaping_state, EscapeState::ThreadEscape);
    assert!(!analyzer.can_promote_to_stack(escaping_state));
}
