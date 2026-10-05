//! Phase 9 Native Debugging and Source-Level Line Tables E2E Test Suite.
//!
//! Validates:
//! - DebugEngine line-number and column-level source location tracking.
//! - Function debug info and local variable scope registration.
//! - Instruction address to source location lookup.
//! - Crash reporting (`CrashReporter`) with stack frames.

#![allow(dead_code, unused_imports)]

use adesh_codegen::crash::{CrashReporter, CrashSignal, CrashStackFrame};
use adesh_codegen::debug::{DebugEngine, FunctionDebugInfo, LocalVariableDebugInfo, SourceLocation};

#[test]
fn test_debug_engine_line_and_variable_tracking() {
    let mut engine = DebugEngine::new();
    let mut func_debug = FunctionDebugInfo::new("compute_metrics", "src/math.adesh", 10);

    func_debug.add_line_mapping(0x1000, 12, 5);
    func_debug.add_line_mapping(0x1008, 13, 8);
    func_debug.add_line_mapping(0x1010, 14, 1);

    func_debug.add_local(LocalVariableDebugInfo {
        name: "result".to_string(),
        type_name: "i64".to_string(),
        stack_offset: Some(-16),
        scope_start_line: 12,
        scope_end_line: 15,
        ..Default::default()
    });

    engine.register_function(func_debug);

    let loc = engine.lookup_location(0x1008).expect("lookup line");
    assert_eq!(loc.file, "src/math.adesh");
    assert_eq!(loc.line, 13);
    assert_eq!(loc.column, 8);

    let func_info = engine.get_function("compute_metrics").expect("function info");
    assert_eq!(func_info.locals.len(), 1);
    assert_eq!(func_info.locals[0].name, "result");
}

#[test]
fn test_crash_reporter_generation() {
    let frames = vec![
        CrashStackFrame {
            frame_index: 0,
            instruction_pointer: 0x7FFF0010,
            symbol_name: Some("process_frame".to_string()),
            source_file: Some("src/render.adesh".to_string()),
            line_number: Some(42),
            ..Default::default()
        },
        CrashStackFrame {
            frame_index: 1,
            instruction_pointer: 0x7FFF00A0,
            symbol_name: Some("main".to_string()),
            source_file: Some("src/main.adesh".to_string()),
            line_number: Some(15),
            ..Default::default()
        },
    ];

    let report = CrashReporter::generate_report(CrashSignal::SegmentationFault, 0x00000004, frames);
    assert_eq!(report.signal, CrashSignal::SegmentationFault);
    assert_eq!(report.fault_address, 0x00000004);
    assert_eq!(report.stack_trace.len(), 2);

    let text = CrashReporter::format_report(&report);
    assert!(text.contains("SegmentationFault"));
    assert!(text.contains("process_frame"));
}
