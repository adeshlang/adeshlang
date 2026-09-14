//! Process Management
//!
//! Production-grade process spawning, management, and control.
//!
//! This module provides safe abstractions for:
//! - Process creation and spawning
//! - Standard I/O redirection
//! - Process termination and wait
//! - Environment variable management
//! - Exit status handling

use std::ffi::OsStr;
use std::io;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};

/// Re-export from runtime stdlib for compatibility
pub use crate::runtime::stdlib_src::system::args::*;

/// Result type for process operations
pub type ProcessResult<T> = Result<T, ProcessError>;

/// Process-related errors
#[derive(Debug, Clone)]
pub enum ProcessError {
    /// Failed to spawn process
    SpawnFailed(String),
    /// Process not found
    NotFound,
    /// I/O error during process operation
    IoError(String),
    /// Process already terminated
    AlreadyTerminated,
    /// Invalid UTF-8 in process output
    InvalidUtf8,
    /// Process did not complete
    Incomplete,
}

impl std::fmt::Display for ProcessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessError::SpawnFailed(msg) => write!(f, "Failed to spawn process: {}", msg),
            ProcessError::NotFound => write!(f, "Process not found"),
            ProcessError::IoError(msg) => write!(f, "I/O error: {}", msg),
            ProcessError::AlreadyTerminated => write!(f, "Process already terminated"),
            ProcessError::InvalidUtf8 => write!(f, "Invalid UTF-8 in process output"),
            ProcessError::Incomplete => write!(f, "Process did not complete"),
        }
    }
}

impl std::error::Error for ProcessError {}

impl From<io::Error> for ProcessError {
    fn from(err: io::Error) -> Self {
        ProcessError::IoError(err.to_string())
    }
}

/// Process builder for configuring and spawning processes
pub struct ProcessBuilder {
    command: Command,
}

impl ProcessBuilder {
    /// Creates a new process builder for the given program
    pub fn new<S: AsRef<OsStr>>(program: S) -> Self {
        ProcessBuilder {
            command: Command::new(program),
        }
    }

    /// Adds an argument to the command
    pub fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Self {
        self.command.arg(arg);
        self
    }

    /// Adds multiple arguments to the command
    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.command.args(args);
        self
    }

    /// Sets an environment variable for the process
    pub fn env<K, V>(&mut self, key: K, val: V) -> &mut Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.command.env(key, val);
        self
    }

    /// Clears all environment variables
    pub fn env_clear(&mut self) -> &mut Self {
        self.command.env_clear();
        self
    }

    /// Sets the working directory for the process
    pub fn current_dir<P: AsRef<Path>>(&mut self, dir: P) -> &mut Self {
        self.command.current_dir(dir);
        self
    }

    /// Configures stdin
    pub fn stdin(&mut self, cfg: Stdio) -> &mut Self {
        self.command.stdin(cfg);
        self
    }

    /// Configures stdout
    pub fn stdout(&mut self, cfg: Stdio) -> &mut Self {
        self.command.stdout(cfg);
        self
    }

    /// Configures stderr
    pub fn stderr(&mut self, cfg: Stdio) -> &mut Self {
        self.command.stderr(cfg);
        self
    }

    /// Spawns the process
    pub fn spawn(&mut self) -> ProcessResult<Process> {
        match self.command.spawn() {
            Ok(child) => Ok(Process { child: Some(child) }),
            Err(e) => Err(ProcessError::SpawnFailed(e.to_string())),
        }
    }

    /// Spawns the process and waits for it to complete
    pub fn status(&mut self) -> ProcessResult<ExitStatus> {
        self.command
            .status()
            .map_err(|e| ProcessError::SpawnFailed(e.to_string()))
    }

    /// Spawns the process and captures its output
    pub fn output(&mut self) -> ProcessResult<ProcessOutput> {
        match self.command.output() {
            Ok(output) => Ok(ProcessOutput {
                status: output.status,
                stdout: output.stdout,
                stderr: output.stderr,
            }),
            Err(e) => Err(ProcessError::SpawnFailed(e.to_string())),
        }
    }
}

/// Captured output from a process
pub struct ProcessOutput {
    /// Exit status
    pub status: ExitStatus,
    /// Standard output bytes
    pub stdout: Vec<u8>,
    /// Standard error bytes
    pub stderr: Vec<u8>,
}

impl ProcessOutput {
    /// Converts stdout to a String
    pub fn stdout_str(&self) -> ProcessResult<String> {
        String::from_utf8(self.stdout.clone()).map_err(|_| ProcessError::InvalidUtf8)
    }

    /// Converts stderr to a String
    pub fn stderr_str(&self) -> ProcessResult<String> {
        String::from_utf8(self.stderr.clone()).map_err(|_| ProcessError::InvalidUtf8)
    }

    /// Returns true if the process exited successfully
    pub fn success(&self) -> bool {
        self.status.success()
    }

    /// Returns the exit code if available
    pub fn code(&self) -> Option<i32> {
        self.status.code()
    }
}

/// A running or completed process
pub struct Process {
    child: Option<Child>,
}

impl Process {
    /// Gets the current process ID
    pub fn id() -> u32 {
        std::process::id()
    }

    /// Exits the current process with the given code
    pub fn exit(code: i32) -> ! {
        std::process::exit(code)
    }

    /// Aborts the current process
    pub fn abort() -> ! {
        std::process::abort()
    }

    /// Gets the process ID of this child process
    pub fn child_id(&self) -> Option<u32> {
        self.child.as_ref().map(|c| c.id())
    }

    /// Waits for the process to complete and returns its exit status
    pub fn wait(&mut self) -> ProcessResult<ExitStatus> {
        match &mut self.child {
            Some(child) => child.wait().map_err(ProcessError::from),
            None => Err(ProcessError::AlreadyTerminated),
        }
    }

    /// Attempts to collect the exit status if the process has terminated
    pub fn try_wait(&mut self) -> ProcessResult<Option<ExitStatus>> {
        match &mut self.child {
            Some(child) => child.try_wait().map_err(ProcessError::from),
            None => Err(ProcessError::AlreadyTerminated),
        }
    }

    /// Kills the process
    pub fn kill(&mut self) -> ProcessResult<()> {
        match &mut self.child {
            Some(child) => child.kill().map_err(ProcessError::from),
            None => Err(ProcessError::AlreadyTerminated),
        }
    }

    /// Takes ownership of stdin
    pub fn stdin(&mut self) -> Option<std::process::ChildStdin> {
        self.child.as_mut().and_then(|c| c.stdin.take())
    }

    /// Takes ownership of stdout
    pub fn stdout(&mut self) -> Option<std::process::ChildStdout> {
        self.child.as_mut().and_then(|c| c.stdout.take())
    }

    /// Takes ownership of stderr
    pub fn stderr(&mut self) -> Option<std::process::ChildStderr> {
        self.child.as_mut().and_then(|c| c.stderr.take())
    }

    /// Waits for the process and captures its output
    pub fn wait_with_output(mut self) -> ProcessResult<ProcessOutput> {
        match self.child.take() {
            Some(child) => {
                let output = child.wait_with_output().map_err(ProcessError::from)?;
                Ok(ProcessOutput {
                    status: output.status,
                    stdout: output.stdout,
                    stderr: output.stderr,
                })
            }
            None => Err(ProcessError::AlreadyTerminated),
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        // Ensure process is cleaned up
        if let Some(ref mut child) = self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_id() {
        let pid = Process::id();
        assert!(pid > 0);
    }

    #[test]
    fn test_process_builder_echo() {
        let (cmd, args) = if cfg!(target_os = "windows") {
            ("cmd", vec!["/C", "echo hello"])
        } else {
            ("echo", vec!["hello"])
        };
        let mut builder = ProcessBuilder::new(cmd);
        for arg in args {
            builder.arg(arg);
        }
        let output = builder.output().expect("Failed to run echo");

        assert!(output.success());
        assert_eq!(output.stdout_str().unwrap().trim(), "hello");
    }

    #[test]
    fn test_process_spawn_and_wait() {
        let (cmd, args) = if cfg!(target_os = "windows") {
            ("cmd", vec!["/C", "echo test"])
        } else {
            ("echo", vec!["test"])
        };
        let mut builder = ProcessBuilder::new(cmd);
        for arg in args {
            builder.arg(arg);
        }
        let mut process = builder
            .stdout(Stdio::null())
            .spawn()
            .expect("Failed to spawn");

        let status = process.wait().expect("Failed to wait");
        assert!(status.success());
    }
}
