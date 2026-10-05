//! Phase 9 Native Crash Reporting Framework.
//!
//! Provides:
//! - Crash context capture (signal/exception, fault address, IP, thread ID, build ID)
//! - Stack frame symbolication with debug metadata integration
//! - Clean, reproducible human-readable and structured JSON crash reports

use crate::debug::{DebugEngine, SourceLocation};
use serde::{Deserialize, Serialize};

/// Signal or hardware exception kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrashSignal {
    AccessViolation,      // SIGSEGV / EXCEPTION_ACCESS_VIOLATION
    SegmentationFault,   // Alias for AccessViolation
    IllegalInstruction,   // SIGILL / EXCEPTION_ILLEGAL_INSTRUCTION
    IntegerDivideByZero,  // SIGFPE / EXCEPTION_INT_DIVIDE_BY_ZERO
    StackOverflow,        // EXCEPTION_STACK_OVERFLOW
    Abort,                // SIGABRT
    Breakpoint,           // SIGTRAP / EXCEPTION_BREAKPOINT
}

impl std::fmt::Display for CrashSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CrashSignal::AccessViolation => write!(f, "Access Violation (SIGSEGV)"),
            CrashSignal::SegmentationFault => write!(f, "SegmentationFault"),
            CrashSignal::IllegalInstruction => write!(f, "Illegal Instruction (SIGILL)"),
            CrashSignal::IntegerDivideByZero => write!(f, "Integer Division by Zero (SIGFPE)"),
            CrashSignal::StackOverflow => write!(f, "Stack Overflow"),
            CrashSignal::Abort => write!(f, "Abort (SIGABRT)"),
            CrashSignal::Breakpoint => write!(f, "Breakpoint Trap"),
        }
    }
}

/// Stack frame captured during a crash.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CrashStackFrame {
    pub frame_index: usize,
    pub instruction_address: u64,
    pub function_name: Option<String>,
    pub source_location: Option<SourceLocation>,
    pub instruction_pointer: u64,
    pub symbol_name: Option<String>,
    pub source_file: Option<String>,
    pub line_number: Option<u32>,
}

/// Comprehensive native crash report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashReport {
    pub signal: CrashSignal,
    pub fault_address: u64,
    pub instruction_address: u64,
    pub thread_id: u64,
    pub build_id: String,
    pub target_triple: String,
    pub stack_frames: Vec<CrashStackFrame>,
    pub stack_trace: Vec<CrashStackFrame>,
    pub timestamp_utc: String,
}

impl CrashReport {
    /// Format crash report as a developer-friendly diagnostic text.
    pub fn format_diagnostic(&self) -> String {
        let mut s = String::new();
        s.push_str("======================================================================\n");
        s.push_str("                        Adesh Native Crash Report                     \n");
        s.push_str("======================================================================\n");
        s.push_str(&format!("Signal:             {}\n", self.signal));
        s.push_str(&format!("Fault Address:      0x{:016x}\n", self.fault_address));
        s.push_str(&format!("Instruction (PC):   0x{:016x}\n", self.instruction_address));
        s.push_str(&format!("Thread ID:          {}\n", self.thread_id));
        s.push_str(&format!("Target:             {}\n", self.target_triple));
        s.push_str(&format!("Build ID:           {}\n", self.build_id));
        s.push_str("\nStack Trace:\n");

        for frame in &self.stack_frames {
            let func_str = frame.function_name.as_deref().unwrap_or("<unknown>");
            let loc_str = if let Some(loc) = &frame.source_location {
                format!("{}:{}:{}", loc.file, loc.line, loc.column)
            } else {
                format!("pc:0x{:x}", frame.instruction_address)
            };
            s.push_str(&format!("  #{:02} {} at {}\n", frame.frame_index, func_str, loc_str));
        }
        s.push_str("======================================================================\n");
        s
    }
}

/// Crash Reporter Engine.
pub struct CrashReporter {
    build_id: String,
    target_triple: String,
}

impl CrashReporter {
    pub fn new(build_id: impl Into<String>, target_triple: impl Into<String>) -> Self {
        Self {
            build_id: build_id.into(),
            target_triple: target_triple.into(),
        }
    }

    /// Generate a report directly from frames.
    pub fn generate_report(
        signal: CrashSignal,
        fault_address: u64,
        frames: Vec<CrashStackFrame>,
    ) -> CrashReport {
        let ip = frames
            .first()
            .map(|f| {
                if f.instruction_pointer != 0 {
                    f.instruction_pointer
                } else {
                    f.instruction_address
                }
            })
            .unwrap_or(0);
        CrashReport {
            signal,
            fault_address,
            instruction_address: ip,
            thread_id: 1,
            build_id: "default-build-id".to_string(),
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            stack_frames: frames.clone(),
            stack_trace: frames,
            timestamp_utc: "2026-10-04T00:00:00Z".to_string(),
        }
    }

    /// Format a crash report as text.
    pub fn format_report(report: &CrashReport) -> String {
        report.format_diagnostic()
    }

    /// Symbolicate a raw stack trace using the provided DebugEngine.
    pub fn create_report(
        &self,
        signal: CrashSignal,
        fault_address: u64,
        instruction_address: u64,
        thread_id: u64,
        raw_stack_pcs: &[u64],
        debug_engine: Option<&DebugEngine>,
    ) -> CrashReport {
        let mut stack_frames = Vec::new();

        for (idx, &pc) in raw_stack_pcs.iter().enumerate() {
            let (func, loc) = if let Some(engine) = debug_engine {
                if let Some((f, l)) = engine.resolve_address(pc) {
                    (Some(f), Some(l))
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            };

            let symbol_name = func.clone();
            let source_file = loc.as_ref().map(|l| l.file.clone());
            let line_number = loc.as_ref().map(|l| l.line);

            stack_frames.push(CrashStackFrame {
                frame_index: idx,
                instruction_address: pc,
                function_name: func,
                source_location: loc,
                instruction_pointer: pc,
                symbol_name,
                source_file,
                line_number,
            });
        }

        CrashReport {
            signal,
            fault_address,
            instruction_address,
            thread_id,
            build_id: self.build_id.clone(),
            target_triple: self.target_triple.clone(),
            stack_frames: stack_frames.clone(),
            stack_trace: stack_frames,
            timestamp_utc: "2026-10-04T00:00:00Z".to_string(),
        }
    }
}
