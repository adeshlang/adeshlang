use serde::{Deserialize, Serialize};
use std::sync::{Mutex, MutexGuard};
#[cfg(unix)]
use std::thread::JoinHandle;
use once_cell::sync::Lazy;

use crate::diagnostics::Diagnostic;

static CAPTURE_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionStatus {
    #[serde(rename = "idle")]
    Idle,
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "cancelled")]
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub status: ExecutionStatus,
    pub stdout: String,
    pub stderr: String,
    pub execution_time_ms: f64,
    pub diagnostics: Vec<Diagnostic>,
    pub backend_used: String,
}

impl ExecutionResult {
    pub fn success(stdout: String, stderr: String, elapsed_ms: f64) -> Self {
        Self {
            status: ExecutionStatus::Completed,
            stdout,
            stderr,
            execution_time_ms: elapsed_ms,
            diagnostics: Vec::new(),
            backend_used: "Interpreter".to_string(),
        }
    }

    pub fn failure(
        stdout: String,
        stderr: String,
        elapsed_ms: f64,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        Self {
            status: ExecutionStatus::Failed,
            stdout,
            stderr,
            execution_time_ms: elapsed_ms,
            diagnostics,
            backend_used: "Interpreter".to_string(),
        }
    }

    pub fn cancelled(stdout: String, stderr: String, elapsed_ms: f64) -> Self {
        Self {
            status: ExecutionStatus::Cancelled,
            stdout,
            stderr,
            execution_time_ms: elapsed_ms,
            diagnostics: vec![Diagnostic {
                severity: crate::diagnostics::DiagnosticSeverity::Info,
                message: "Execution cancelled by user".to_string(),
                line: 1,
                column: 1,
                length: 1,
                code: Some("Cancelled".to_string()),
            }],
            backend_used: "Interpreter".to_string(),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            r#"{"status":"failed","stdout":"","stderr":"Serialization error","execution_time_ms":0.0,"diagnostics":[],"backend_used":"Interpreter"}"#.to_string()
        })
    }
}

/// Helper struct to capture stdout and stderr safely during code execution.
/// Avoids pipe deadlock by reading from the pipe concurrently on a background thread.
/// Implements `Drop` to guarantee file descriptors are restored on panics or early returns.
pub struct OutputCaptureGuard {
    _lock: MutexGuard<'static, ()>,
    #[cfg(unix)]
    saved_stdout: i32,
    #[cfg(unix)]
    saved_stderr: i32,
    #[cfg(unix)]
    reader_thread: Option<JoinHandle<Vec<u8>>>,
    #[cfg(unix)]
    pipe_write_fd: i32,
}

impl OutputCaptureGuard {
    #[cfg(unix)]
    pub fn start() -> Result<Self, String> {
        let lock = CAPTURE_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            // Flush any existing stdout/stderr buffers
            adeshlang::execution::runtime_core::stdio::flush_all();
            adeshlang::execution::runtime_core::fast_print::flush_fast_buffer();

            let mut fds = [0i32; 2];
            if libc::pipe(fds.as_mut_ptr()) != 0 {
                return Err("Failed to create stdout pipe".to_string());
            }

            let saved_stdout = libc::dup(libc::STDOUT_FILENO);
            let saved_stderr = libc::dup(libc::STDERR_FILENO);

            // Redirect stdout and stderr to write end of the pipe
            libc::dup2(fds[1], libc::STDOUT_FILENO);
            libc::dup2(fds[1], libc::STDERR_FILENO);

            let read_fd = fds[0];
            let write_fd = fds[1];

            // Spawn background reader thread to drain pipe concurrently, preventing deadlocks on large outputs (>64KB)
            let reader_thread = std::thread::spawn(move || {
                let mut captured = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    let n = libc::read(read_fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len());
                    if n > 0 {
                        captured.extend_from_slice(&buf[..n as usize]);
                    } else {
                        break;
                    }
                }
                libc::close(read_fd);
                captured
            });

            Ok(Self {
                _lock: lock,
                saved_stdout,
                saved_stderr,
                reader_thread: Some(reader_thread),
                pipe_write_fd: write_fd,
            })
        }
    }

    #[cfg(not(unix))]
    pub fn start() -> Result<Self, String> {
        let lock = CAPTURE_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        Ok(Self { _lock: lock })
    }

    #[cfg(unix)]
    pub fn finish(mut self) -> (String, String) {
        unsafe {
            // Flush all Rust buffers
            adeshlang::execution::runtime_core::stdio::flush_all();
            adeshlang::execution::runtime_core::fast_print::flush_fast_buffer();

            // Restore original stdout/stderr descriptors
            libc::dup2(self.saved_stdout, libc::STDOUT_FILENO);
            libc::dup2(self.saved_stderr, libc::STDERR_FILENO);

            // Close duplicated write descriptor so reader thread receives EOF
            if self.pipe_write_fd >= 0 {
                libc::close(self.pipe_write_fd);
                self.pipe_write_fd = -1;
            }
        }

        let output = if let Some(handle) = self.reader_thread.take() {
            handle.join().unwrap_or_default()
        } else {
            Vec::new()
        };

        let captured = String::from_utf8_lossy(&output).to_string();
        (captured, String::new())
    }

    #[cfg(not(unix))]
    pub fn finish(self) -> (String, String) {
        (String::new(), String::new())
    }
}

impl Drop for OutputCaptureGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            // Restore stdout and stderr if finish() wasn't called (e.g. on panic)
            if self.saved_stdout >= 0 {
                libc::dup2(self.saved_stdout, libc::STDOUT_FILENO);
                libc::close(self.saved_stdout);
                self.saved_stdout = -1;
            }
            if self.saved_stderr >= 0 {
                libc::dup2(self.saved_stderr, libc::STDERR_FILENO);
                libc::close(self.saved_stderr);
                self.saved_stderr = -1;
            }
            if self.pipe_write_fd >= 0 {
                libc::close(self.pipe_write_fd);
                self.pipe_write_fd = -1;
            }
        }
    }
}
