//! Cross-platform stdout/stderr capture for tests
//!
//! This module provides stdout/stderr redirection to capture print output during test execution.
//! Implementation:
//! - Unix/Linux: Uses dup2 to redirect fd 1 (stdout) and fd 2 (stderr) to a temporary file
//! - Windows: Uses SetStdHandle to redirect stdout/stderr to a temporary file
//! - Other: No capture (fallback)

use std::io;
#[cfg(any(unix, windows))]
use std::path::PathBuf;

#[cfg(windows)]
use std::fs::File;

/// A guard that captures stdout and restores it when dropped
#[cfg(unix)]
pub struct StdoutCapture {
    original_stdout: i32,
    original_stderr: i32,
    temp_path: PathBuf,
}

/// A guard that captures stdout and restores it when dropped
#[cfg(windows)]
pub struct StdoutCapture {
    original_stdout: isize,
    original_stderr: isize,
    temp_path: PathBuf,
    _temp_file: File,
}

/// A guard that captures stdout and restores it when dropped
#[cfg(not(any(unix, windows)))]
pub struct StdoutCapture;

impl StdoutCapture {
    /// Start capturing stdout and stderr
    pub fn start() -> io::Result<Self> {
        #[cfg(unix)]
        {
            use std::fs::OpenOptions;
            use std::os::unix::io::IntoRawFd;

            let mut temp_path = std::env::temp_dir();
            temp_path.push(format!("adesh_test_{}.tmp", std::process::id()));

            let temp_file = OpenOptions::new()
                .create(true)
                .write(true)
                .read(true)
                .truncate(true)
                .open(&temp_path)?;
            let temp_fd = temp_file.into_raw_fd();

            // Save original stdout/stderr fds (1 and 2)
            let original_stdout = unsafe { libc::dup(1) };
            if original_stdout < 0 {
                return Err(io::Error::last_os_error());
            }
            let original_stderr = unsafe { libc::dup(2) };
            if original_stderr < 0 {
                unsafe {
                    libc::close(original_stdout);
                }
                return Err(io::Error::last_os_error());
            }

            // Redirect stdout/stderr (fd 1/2) to temp file (temp_fd)
            if unsafe { libc::dup2(temp_fd, 1) } < 0 {
                unsafe {
                    libc::close(original_stdout);
                    libc::close(original_stderr);
                }
                return Err(io::Error::last_os_error());
            }
            if unsafe { libc::dup2(temp_fd, 2) } < 0 {
                unsafe {
                    libc::dup2(original_stdout, 1);
                    libc::close(original_stdout);
                    libc::close(original_stderr);
                }
                return Err(io::Error::last_os_error());
            }

            // Close the temp_fd since dup2 duplicated it to fd 1
            unsafe {
                libc::close(temp_fd);
            }

            Ok(StdoutCapture {
                original_stdout,
                original_stderr,
                temp_path,
            })
        }

        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::System::Console::{
                GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
            };

            let mut temp_path = std::env::temp_dir();
            temp_path.push(format!("adesh_test_{}.tmp", std::process::id()));

            let temp_file = File::create(&temp_path)?;
            let temp_handle = temp_file.as_raw_handle() as isize;

            let original_stdout = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
            if original_stdout == 0 || original_stdout == -1 {
                return Err(io::Error::last_os_error());
            }
            let original_stderr = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
            if original_stderr == 0 || original_stderr == -1 {
                return Err(io::Error::last_os_error());
            }

            if unsafe { SetStdHandle(STD_OUTPUT_HANDLE, temp_handle) } == 0 {
                return Err(io::Error::last_os_error());
            }
            if unsafe { SetStdHandle(STD_ERROR_HANDLE, temp_handle) } == 0 {
                let _ = unsafe { SetStdHandle(STD_OUTPUT_HANDLE, original_stdout) };
                return Err(io::Error::last_os_error());
            }

            Ok(StdoutCapture {
                original_stdout,
                original_stderr,
                temp_path,
                _temp_file: temp_file,
            })
        }

        #[cfg(not(any(unix, windows)))]
        {
            // On non-Unix platforms, we can't capture stdout easily
            // This is a no-op implementation
            Ok(StdoutCapture {})
        }
    }

    /// Stop capturing and return the captured output
    pub fn stop(self) -> io::Result<String> {
        #[cfg(unix)]
        {
            // Flush stdout to ensure all output is written
            let _ = std::io::Write::flush(&mut std::io::stdout());
            let _ = std::io::Write::flush(&mut std::io::stderr());

            // Restore original stdout/stderr
            if unsafe { libc::dup2(self.original_stdout, 1) } < 0 {
                return Err(io::Error::last_os_error());
            }
            if unsafe { libc::dup2(self.original_stderr, 2) } < 0 {
                return Err(io::Error::last_os_error());
            }
            unsafe {
                libc::close(self.original_stdout);
                libc::close(self.original_stderr);
            }

            // Read captured output from temp file
            let output = std::fs::read_to_string(&self.temp_path).unwrap_or_default();
            let _ = std::fs::remove_file(&self.temp_path);

            Ok(output)
        }

        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Console::{
                STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
            };

            let _ = std::io::Write::flush(&mut std::io::stdout());
            let _ = std::io::Write::flush(&mut std::io::stderr());

            if unsafe { SetStdHandle(STD_OUTPUT_HANDLE, self.original_stdout) } == 0 {
                return Err(io::Error::last_os_error());
            }
            if unsafe { SetStdHandle(STD_ERROR_HANDLE, self.original_stderr) } == 0 {
                return Err(io::Error::last_os_error());
            }

            let output = std::fs::read_to_string(&self.temp_path).unwrap_or_default();
            let _ = std::fs::remove_file(&self.temp_path);

            Ok(output)
        }

        #[cfg(not(any(unix, windows)))]
        {
            // On non-Unix platforms, return empty string (no capture)
            Ok(String::new())
        }
    }
}

impl Drop for StdoutCapture {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            // Attempt to restore stdout if stop() wasn't called
            unsafe {
                let _ = libc::dup2(self.original_stdout, 1);
                let _ = libc::dup2(self.original_stderr, 2);
                let _ = libc::close(self.original_stdout);
                let _ = libc::close(self.original_stderr);
            }

            // Clean up temp file
            let _ = std::fs::remove_file(&self.temp_path);
        }

        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Console::{
                STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
            };
            unsafe {
                let _ = SetStdHandle(STD_OUTPUT_HANDLE, self.original_stdout);
                let _ = SetStdHandle(STD_ERROR_HANDLE, self.original_stderr);
            }
            let _ = std::fs::remove_file(&self.temp_path);
        }
    }
}
