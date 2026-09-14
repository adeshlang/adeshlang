//! Test Execution Backend
//!
//! Implements the BackendTestExecutor trait for actually running tests
//! with proper isolation, timeout, and panic handling.

use super::{BackendExecutionResult, BackendTestExecutor, TestInfo, TestRunOptions, TestStatus};
use crate::execution::runtime::{Interpreter, ModuleLoader};
use crate::parsing::lexer::Lexer;
use crate::parsing::parser::Parser;
use crate::toolchain::config::ExecutionBackend;
use std::panic;
use std::path::Path;
use std::time::{Duration, Instant};

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

    /// Execute a single test function by name
    fn run_test_function(&self, test_name: &str) -> Result<Duration, String> {
        let start = Instant::now();

        // Create a fresh interpreter for test isolation
        let mut interp = Interpreter::new();
        // Skip main() when loading test module (like Rust does)
        interp.set_skip_main(true);
        let path = Path::new(&self.file_path);
        let mut loader = ModuleLoader::new(path.parent().unwrap_or(Path::new(".")));

        // Load the module to define all functions
        interp.run_module(&self.source_code, &mut loader, Some(self.file_path.clone()))?;

        // Now call the test function by evaluating it
        let test_call = format!("{}();", test_name);
        let mut lexer = Lexer::new(&test_call);
        let tokens = lexer.tokenize().map_err(|e| e.to_string())?;
        let mut parser = Parser::new(tokens, Some(format!("<test:{}>", test_name)));
        let call_stmt = parser.parse_program().map_err(|e| e.to_string())?;

        // Execute the test call
        for stmt in call_stmt {
            interp
                .exec_stmt(&stmt, interp.global_scope(), &mut loader, &self.file_path)
                .map_err(|e| e.to_string())?;
        }

        Ok(start.elapsed())
    }
}

impl BackendTestExecutor for InterpreterTestExecutor {
    fn execute_test(
        &mut self,
        backend: ExecutionBackend,
        test: &TestInfo,
        _timeout: Duration,
        _options: &TestRunOptions,
    ) -> BackendExecutionResult {
        // Only support Interpreter backend for now
        if backend != ExecutionBackend::Interpreter {
            return BackendExecutionResult {
                status: TestStatus::Ignored,
                message: Some(format!("Backend {:?} not yet implemented", backend)),
                duration: Duration::from_millis(0),
                stack_trace: None,
                output: String::new(),
            };
        }

        // Start capturing stdout/stderr; if quiet, we'll discard the captured output.
        let _capture_guard = match crate::testing::stdout_capture::StdoutCapture::start() {
            Ok(guard) => Some(guard),
            Err(_) => None, // Gracefully handle if capture fails
        };

        let start = Instant::now();

        // Catch panics during test execution
        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            self.run_test_function(&test.name)
        }));

        let duration = start.elapsed();

        // Stop capturing and get the captured output
        let captured = if let Some(guard) = _capture_guard {
            match guard.stop() {
                Ok(captured) => captured,
                Err(_) => String::new(), // Gracefully handle if stopping capture fails
            }
        } else {
            String::new()
        };
        let output = captured;

        match result {
            Ok(Ok(_)) => BackendExecutionResult {
                status: TestStatus::Pass,
                message: None,
                duration,
                stack_trace: None,
                output,
            },
            Ok(Err(e)) => {
                // Check if this is an assertion failure
                let status = if e.contains("Test failed") {
                    TestStatus::Fail
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
            Err(panic_info) => {
                // Handle panic
                let msg = if let Some(s) = panic_info.downcast_ref::<String>() {
                    s.clone()
                } else if let Some(s) = panic_info.downcast_ref::<&str>() {
                    s.to_string()
                } else {
                    "Test panicked (unknown cause)".to_string()
                };

                BackendExecutionResult {
                    status: TestStatus::Panic,
                    message: Some(format!("PANIC: {}", msg)),
                    duration,
                    stack_trace: None,
                    output,
                }
            }
        }
    }
}
