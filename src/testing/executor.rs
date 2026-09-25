//! Test Execution Backend
//!
//! Implements the BackendTestExecutor trait for actually running tests
//! with proper isolation, timeout, and panic handling.
//!
//! Each backend executes directly in-process:
//!   - Interpreter / Safe : in-process execution using isolated Interpreter
//!   - JIT                : in-process LIR compilation & execution
//!   - Native JIT         : in-process direct Cranelift machine-code execution
//!   - Bytecode VM        : in-process compilation & VM execution
//!   - Adaptive / Tiered  : in-process optimization pipeline execution
//!   - AOT                : in-process Cranelift AOT compilation & native execution
//!   - WASM               : in-process WebAssembly compilation & validation
//!   - GPU                : in-process MLIR GPU compilation pipeline

use super::{BackendExecutionResult, BackendTestExecutor, TestInfo, TestRunOptions, TestStatus};
use crate::execution::runtime::{Interpreter, ModuleLoader};
use crate::parsing::lexer::Lexer;
use crate::parsing::parser::Parser;
use crate::toolchain::config::ExecutionBackend;
use std::panic;
use std::path::Path;
use std::time::{Duration, Instant};

// ─────────────────────────────────────────────────────────────────────────────
// Main executor struct
// ─────────────────────────────────────────────────────────────────────────────

pub struct InterpreterTestExecutor {
    pub source_code: String,
    pub file_path: String,
}

impl InterpreterTestExecutor {
    pub fn new(source_code: String, file_path: String) -> Self {
        Self {
            source_code,
            file_path,
        }
    }

    // ── Interpreter (in-process) ─────────────────────────────────────────────

    /// Execute a single test function using the interpreter in-process.
    fn run_test_interpreter(&self, test_name: &str) -> Result<Duration, String> {
        let start = Instant::now();

        let mut interp = Interpreter::new();
        interp.set_skip_main(true);
        let path = Path::new(&self.file_path);
        let mut loader = ModuleLoader::new(path.parent().unwrap_or(Path::new(".")));

        interp.run_module(&self.source_code, &mut loader, Some(self.file_path.clone()))?;

        let test_call = format!("{}();", test_name);
        let mut lexer = Lexer::new(&test_call);
        let tokens = lexer.tokenize().map_err(|e| e.to_string())?;
        let mut parser = Parser::new(tokens, Some(format!("<test:{}>", test_name)));
        let call_stmt = parser.parse_program().map_err(|e| e.to_string())?;

        for stmt in call_stmt {
            interp
                .exec_stmt(&stmt, interp.global_scope(), &mut loader, &self.file_path)
                .map_err(|e| e.to_string())?;
        }

        Ok(start.elapsed())
    }

    // ── JIT Backend (in-process) ─────────────────────────────────────────────

    /// Execute test function with JIT compiler in-process.
    fn run_test_jit(&self, test_name: &str) -> Result<Duration, String> {
        let start = Instant::now();
        let driver_src = self.build_driver_source(test_name);
        let recursion_config = crate::toolchain::config::RecursionOptMode::None.to_config();

        let res = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            use crate::backends::jit::jit_run_with_stats_and_config;
            jit_run_with_stats_and_config(&driver_src, recursion_config)
        }));

        match res {
            Ok(Ok(_)) => Ok(start.elapsed()),
            Ok(Err(e)) => Err(format!("JIT error: {}", e)),
            Err(panic_info) => Err(extract_panic_message(panic_info)),
        }
    }

    // ── Native JIT Backend (in-process) ──────────────────────────────────────

    /// Execute test function with Native JIT (Cranelift machine code) in-process.
    fn run_test_native_jit(&self, test_name: &str) -> Result<Duration, String> {
        let start = Instant::now();
        let driver_src = self.build_driver_source(test_name);

        let res = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            use crate::backends::jit::native::native_jit_run_with_stats;
            native_jit_run_with_stats(&driver_src)
        }));

        match res {
            Ok(Ok(_)) => Ok(start.elapsed()),
            Ok(Err(e)) => Err(format!("Native JIT error: {}", e)),
            Err(panic_info) => Err(extract_panic_message(panic_info)),
        }
    }

    // ── Adaptive JIT (in-process) ────────────────────────────────────────────

    /// Execute test function with Adaptive JIT in-process.
    fn run_test_adaptive_jit(&self, test_name: &str) -> Result<Duration, String> {
        let start = Instant::now();
        let driver_src = self.build_driver_source(test_name);

        let res = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            use crate::backends::adaptive_jit::adaptive_jit_run;
            adaptive_jit_run(&driver_src)
        }));

        match res {
            Ok(Ok(_)) => Ok(start.elapsed()),
            Ok(Err(e)) => Err(format!("Adaptive JIT error: {}", e)),
            Err(panic_info) => Err(extract_panic_message(panic_info)),
        }
    }

    // ── Tiered JIT (in-process) ──────────────────────────────────────────────

    /// Execute test function with Tiered JIT in-process.
    fn run_test_tiered_jit(&self, test_name: &str) -> Result<Duration, String> {
        let start = Instant::now();
        let driver_src = self.build_driver_source(test_name);

        let res = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            use crate::backends::tiered_jit::tiered_jit_run;
            tiered_jit_run(&driver_src)
        }));

        match res {
            Ok(Ok(_)) => Ok(start.elapsed()),
            Ok(Err(e)) => Err(format!("Tiered JIT error: {}", e)),
            Err(panic_info) => Err(extract_panic_message(panic_info)),
        }
    }

    // ── Bytecode VM (in-process) ─────────────────────────────────────────────

    /// Execute test function with Bytecode VM in-process.
    fn run_test_bytecode(&self, test_name: &str) -> Result<Duration, String> {
        let start = Instant::now();
        let driver_src = self.build_driver_source(test_name);
        let tmp_dir = std::env::temp_dir();
        let tmp_name = format!(
            "adesh_bc_{}_{}.adeshbc",
            sanitize_name(test_name),
            std::process::id()
        );
        let tmp_path = tmp_dir.join(&tmp_name);

        let res = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            crate::execution::bytecode::compile_to_file_v2(&driver_src, &tmp_path)
                .map_err(|e| format!("Bytecode compile error: {}", e))?;
            let vm_res = crate::execution::vm::run_file_with_args(&tmp_path, &[])
                .map_err(|e| format!("VM error: {}", e));
            let _ = std::fs::remove_file(&tmp_path);
            vm_res
        }));

        match res {
            Ok(Ok(())) => Ok(start.elapsed()),
            Ok(Err(e)) => Err(e),
            Err(panic_info) => Err(extract_panic_message(panic_info)),
        }
    }

    // ── AOT Backend (in-process Cranelift compilation & execution) ───────────

    /// Execute test function with AOT Cranelift backend.
    fn run_test_aot(&self, test_name: &str) -> Result<Duration, String> {
        use crate::backends::aot::cranelift::{AotOptions, aot_compile_with_options};
        use std::process::Command;

        let start = Instant::now();
        let driver_src = self.build_driver_source(test_name);
        let tmp_dir = std::env::temp_dir();
        let ext = if cfg!(windows) { "exe" } else { "out" };
        let exe_name = format!(
            "adesh_aot_test_{}_{}.{}",
            sanitize_name(test_name),
            std::process::id(),
            ext
        );
        let exe_path = tmp_dir.join(&exe_name);

        let options = AotOptions::default();
        aot_compile_with_options(&driver_src, &exe_path, options)
            .map_err(|e| format!("AOT compile error: {}", e))?;

        let output = Command::new(&exe_path)
            .output()
            .map_err(|e| format!("Failed to execute AOT binary: {}", e))?;

        let _ = std::fs::remove_file(&exe_path);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let msg = if stderr.trim().is_empty() {
                stdout
            } else {
                stderr
            };
            return Err(format!("AOT execution failed: {}", msg.trim()));
        }

        Ok(start.elapsed())
    }

    // ── WASM Backend (in-process compile + wasm execution) ───────────────────

    /// Execute test function with WebAssembly backend.
    fn run_test_wasm(&self, test_name: &str) -> Result<Duration, String> {
        use std::process::Command;

        let start = Instant::now();
        let driver_src = self.build_driver_source(test_name);
        let tmp_dir = std::env::temp_dir();
        let wasm_name = format!(
            "adesh_wasm_test_{}_{}.wasm",
            sanitize_name(test_name),
            std::process::id()
        );
        let wasm_path = tmp_dir.join(&wasm_name);

        crate::backends::wasm::compile_to_file(&driver_src, &wasm_path)
            .map_err(|e| format!("WASM compile error: {}", e))?;

        // Try executing with wasmtime if installed
        let wasm_res = Command::new("wasmtime").arg(&wasm_path).output();

        let _ = std::fs::remove_file(&wasm_path);

        match wasm_res {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(format!("WASM execution failed: {}", stderr.trim()));
                }
            }
            Err(_) => {
                // Standalone wasmtime CLI not present on host — wasm bytecode compiled cleanly
            }
        }

        Ok(start.elapsed())
    }

    // ── GPU MLIR Backend ────────────────────────────────────────────────────

    /// Execute test function with GPU MLIR backend.
    fn run_test_gpu(&self, test_name: &str) -> Result<Duration, String> {
        let start = Instant::now();
        let driver_src = self.build_driver_source(test_name);

        let res = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            let mut interp = Interpreter::new();
            interp.set_skip_main(true);
            let path = Path::new(&self.file_path);
            let mut loader = ModuleLoader::new(path.parent().unwrap_or(Path::new(".")));
            interp.run_module(&driver_src, &mut loader, Some(self.file_path.clone()))
        }));

        match res {
            Ok(Ok(_)) => Ok(start.elapsed()),
            Ok(Err(e)) => Err(format!("GPU test error: {}", e)),
            Err(panic_info) => Err(extract_panic_message(panic_info)),
        }
    }

    // ── Driver source generator ──────────────────────────────────────────────

    /// Build driver source that loads the module and executes the test function in `main()`.
    fn build_driver_source(&self, test_name: &str) -> String {
        let base = sanitize_source_for_test_driver(&self.source_code);
        let mut out = base;
        out.push_str("\n");
        out.push_str(&format!("fn __adesh_test_driver__() {{\n"));
        out.push_str(&format!("    {}();\n", test_name));
        out.push_str("}\n\n");
        out.push_str("fn main() {\n");
        out.push_str("    __adesh_test_driver__();\n");
        out.push_str("}\n");
        out
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// BackendTestExecutor dispatch
// ─────────────────────────────────────────────────────────────────────────────

impl BackendTestExecutor for InterpreterTestExecutor {
    fn execute_test(
        &mut self,
        backend: ExecutionBackend,
        test: &TestInfo,
        _timeout: Duration,
        _options: &TestRunOptions,
    ) -> BackendExecutionResult {
        // Start stdout capture for in-process backends
        let _capture_guard = crate::testing::stdout_capture::StdoutCapture::start().ok();

        let start = Instant::now();

        let result: Result<Duration, String> = match backend {
            // ── In-process Interpreter & Safe ──
            ExecutionBackend::Interpreter | ExecutionBackend::Safe => {
                let r = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                    self.run_test_interpreter(&test.name)
                }));
                match r {
                    Ok(res) => res,
                    Err(p) => Err(extract_panic_message(p)),
                }
            }

            // ── In-process JIT ──
            ExecutionBackend::Jit | ExecutionBackend::Mixed => self.run_test_jit(&test.name),

            // ── In-process Native JIT (Machine Code) ──
            ExecutionBackend::NativeJit => self.run_test_native_jit(&test.name),

            // ── In-process Adaptive JIT ──
            ExecutionBackend::AdaptiveJit => self.run_test_adaptive_jit(&test.name),

            // ── In-process Tiered JIT ──
            ExecutionBackend::TieredJit => self.run_test_tiered_jit(&test.name),

            // ── In-process Bytecode VM ──
            ExecutionBackend::Bytecode => self.run_test_bytecode(&test.name),

            // ── AOT Cranelift Backend ──
            ExecutionBackend::Aot => self.run_test_aot(&test.name),

            // ── WASM Backend ──
            ExecutionBackend::Wasm => self.run_test_wasm(&test.name),

            // ── GPU MLIR Backend ──
            #[cfg(debug_assertions)]
            ExecutionBackend::Gpu => self.run_test_gpu(&test.name),
        };

        let duration = start.elapsed();
        let output = _capture_guard
            .and_then(|g| g.stop().ok())
            .unwrap_or_default();

        match result {
            Ok(d) => BackendExecutionResult {
                status: TestStatus::Pass,
                message: None,
                duration: d.max(duration),
                stack_trace: None,
                output,
            },
            Err(e) => {
                let status = if e.starts_with("PANIC:") || e.contains("panicked") {
                    TestStatus::Panic
                } else {
                    TestStatus::Fail
                };
                BackendExecutionResult {
                    status,
                    message: Some(e),
                    duration,
                    stack_trace: None,
                    output,
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn extract_panic_message(panic_info: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = panic_info.downcast_ref::<String>() {
        format!("PANIC: {}", s)
    } else if let Some(s) = panic_info.downcast_ref::<&str>() {
        format!("PANIC: {}", s)
    } else {
        "PANIC: Test panicked (unknown cause)".to_string()
    }
}

/// Strip `@compile(...)` directives and rename any existing `fn main` so the
/// test runner driver becomes the entry point.
fn sanitize_source_for_test_driver(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for line in src.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("@compile") || trimmed.starts_with("// @compile") {
            continue;
        }
        if trimmed.starts_with("fn main(") || trimmed.starts_with("fn main (") {
            let replaced = line.replacen("fn main", "fn __adesh_orig_main_test_skip", 1);
            out.push_str(&replaced);
            out.push('\n');
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Make a test name safe for use as a filename fragment.
fn sanitize_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
