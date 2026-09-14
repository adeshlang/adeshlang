use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use adeshlang::cli::{ExecutionBackend, ParsedArgs, RuntimeConfig};
use adeshlang::execution::runtime::{Interpreter, ModuleLoader};

use crate::diagnostics::{Diagnostic, parse_diagnostic_from_error};
use crate::output::{ExecutionResult, OutputCaptureGuard};

pub struct AdeshSession {
    cancelled: Arc<AtomicBool>,
    config: RuntimeConfig,
}

impl AdeshSession {
    pub fn new() -> Self {
        let mut config = RuntimeConfig::new();
        // CRITICAL REQUIREMENT: Force ExecutionBackend::Interpreter unconditionally
        config.backend = ExecutionBackend::Interpreter;

        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            config,
        }
    }

    /// Reset cancellation token before starting a run
    pub fn reset_cancel(&self) {
        self.cancelled.store(false, Ordering::SeqCst);
    }

    /// Cancel current execution
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Returns true if execution cancellation was requested
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Get current execution backend name - MUST ALWAYS BE "Interpreter"
    pub fn get_backend_name(&self) -> &'static str {
        // Explicit safeguard
        assert_eq!(self.config.backend, ExecutionBackend::Interpreter);
        "Interpreter"
    }

    /// Check code without running
    pub fn check(&self, src: &str, filename: Option<&str>) -> Vec<Diagnostic> {
        let path_str = filename.unwrap_or("<mobile>");
        let path = PathBuf::from(path_str);
        let mut diagnostics = Vec::new();

        // 1. Syntax and semantic indexing
        let semantic_index =
            adeshlang::semantics::index_source_in(src, Some(&path.to_string_lossy()));
        for err in &semantic_index.errors {
            diagnostics.push(parse_diagnostic_from_error(&err.to_string()));
        }

        if !diagnostics.is_empty() {
            return diagnostics;
        }

        // 2. Type checking
        if let Err(e) =
            adeshlang::types::type_system::check_module_in(src, Some(&path.to_string_lossy()))
        {
            diagnostics.push(parse_diagnostic_from_error(&e.to_string()));
            return diagnostics;
        }

        // 3. Ownership and memory safety validation
        let parsed_args = ParsedArgs {
            command: "check".to_string(),
            input_file: Some(path_str.to_string()),
            output_file: None,
            config: self.config.clone(),
            program_args: Vec::new(),
            show_help: false,
            show_version: false,
            toolchain_preference: None,
            env_json: false,
            env_shell: false,
            eval_code: None,
        };

        if let Err(e) = adeshlang::cli::parsing::check_ownership_and_parse_in(
            src,
            &parsed_args.config,
            Some(&path.to_string_lossy()),
        ) {
            diagnostics.push(parse_diagnostic_from_error(&e.to_string()));
        }

        diagnostics
    }

    /// Run code using ONLY the AdeshLang Interpreter
    pub fn run(&self, src: &str, filename: Option<&str>) -> ExecutionResult {
        self.reset_cancel();
        let start_time = Instant::now();

        // Guarantee ExecutionBackend::Interpreter
        assert_eq!(
            self.config.backend,
            ExecutionBackend::Interpreter,
            "CRITICAL: Mobile bridge must only execute via ExecutionBackend::Interpreter"
        );

        let path_str = filename.unwrap_or("<mobile>");
        let path = PathBuf::from(path_str);

        // Pre-flight checks (semantics, types, ownership)
        let diagnostics = self.check(src, filename);
        if !diagnostics.is_empty() {
            let elapsed = start_time.elapsed().as_secs_f64() * 1000.0;
            return ExecutionResult::failure(
                String::new(),
                "Static verification failed".to_string(),
                elapsed,
                diagnostics,
            );
        }

        if self.is_cancelled() {
            let elapsed = start_time.elapsed().as_secs_f64() * 1000.0;
            return ExecutionResult::cancelled(String::new(), String::new(), elapsed);
        }

        // Capture stdout/stderr during interpreter execution
        let capture = match OutputCaptureGuard::start() {
            Ok(c) => c,
            Err(e) => {
                let elapsed = start_time.elapsed().as_secs_f64() * 1000.0;
                return ExecutionResult::failure(
                    String::new(),
                    format!("Failed to start output capture: {}", e),
                    elapsed,
                    vec![Diagnostic::new_error("Output capture failed", 1, 1)],
                );
            }
        };

        // Execute code via Interpreter
        let run_src = src.to_string();
        let run_path = path.clone();
        let cancelled_flag = self.cancelled.clone();

        // Set program args in runtime
        adeshlang::execution::runtime::set_program_args(
            Vec::new(),
            Some(run_path.to_string_lossy().to_string()),
        );

        let res = std::thread::Builder::new()
            .name("adesh-mobile-interp".to_string())
            .stack_size(32 * 1024 * 1024)
            .spawn(move || {
                let mut interp = Interpreter::new();
                let mut ldr = ModuleLoader::new(
                    run_path
                        .parent()
                        .unwrap_or_else(|| std::path::Path::new(".")),
                );

                if cancelled_flag.load(Ordering::Relaxed) {
                    return Err("Cancelled".to_string());
                }

                match interp.run_module(
                    &run_src,
                    &mut ldr,
                    Some(run_path.to_string_lossy().to_string()),
                ) {
                    Ok(()) => {
                        interp.run_event_loop_until_idle();
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            });

        let exec_res = match res {
            Ok(handle) => match handle.join() {
                Ok(res) => res,
                Err(_) => Err("Execution thread panicked".to_string()),
            },
            Err(e) => Err(format!("Failed to spawn interpreter thread: {}", e)),
        };

        let (stdout, stderr) = capture.finish();
        let elapsed_ms = start_time.elapsed().as_secs_f64() * 1000.0;

        if self.is_cancelled() {
            return ExecutionResult::cancelled(stdout, stderr, elapsed_ms);
        }

        match exec_res {
            Ok(()) => ExecutionResult::success(stdout, stderr, elapsed_ms),
            Err(e) => {
                let diags = vec![parse_diagnostic_from_error(&e)];
                ExecutionResult::failure(
                    stdout,
                    if stderr.is_empty() {
                        e.clone()
                    } else {
                        format!("{}\n{}", stderr, e)
                    },
                    elapsed_ms,
                    diags,
                )
            }
        }
    }

    /// Format source code using the existing AdeshLang formatter
    pub fn format(&self, src: &str) -> Result<String, String> {
        let config = adeshlang::utils::formatter::FormatConfig {
            indent_size: 4,
            use_tabs: false,
            ..Default::default()
        };

        adeshlang::utils::formatter::format_source(src, Some(config))
            .map_err(|e| format!("Formatting error: {}", e))
    }
}

impl Default for AdeshSession {
    fn default() -> Self {
        Self::new()
    }
}
