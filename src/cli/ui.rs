//! CLI UI components including colors and progress indicators
//!
//! Shared UI utilities for consistent command-line experience.

use std::io::{self, IsTerminal, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// ANSI color codes for terminal output
pub mod colors {
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
    pub const DIM: &str = "\x1b[2m";

    // Foreground colors
    pub const RED: &str = "\x1b[31m";
    pub const GREEN: &str = "\x1b[32m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const BLUE: &str = "\x1b[34m";
    pub const MAGENTA: &str = "\x1b[35m";
    pub const CYAN: &str = "\x1b[36m";
    pub const WHITE: &str = "\x1b[37m";

    // Bright colors
    pub const BRIGHT_GREEN: &str = "\x1b[92m";
    pub const BRIGHT_CYAN: &str = "\x1b[96m";
    pub const BRIGHT_YELLOW: &str = "\x1b[93m";
    pub const BRIGHT_MAGENTA: &str = "\x1b[95m";
}

/// Build progress spinner with colorful animation
pub struct BuildProgress {
    message: String,
    start_time: Instant,
    running: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl BuildProgress {
    /// Create and start a new progress spinner
    pub fn new(message: &str) -> Self {
        let running = Arc::new(AtomicBool::new(true));

        // In non-interactive environments (CI, redirected output, tool capture),
        // avoid background spinner threads entirely. This prevents shutdown hangs
        // from stdout synchronization behavior on certain terminals.
        if !io::stdout().is_terminal() {
            return Self {
                message: message.to_string(),
                start_time: Instant::now(),
                running,
                handle: None,
            };
        }

        let running_clone = running.clone();
        let message_clone = message.to_string();

        let handle = std::thread::spawn(move || {
            // Colorful spinner frames with gradient effect
            let frames = [
                ("⠋", colors::CYAN),
                ("⠙", colors::BRIGHT_CYAN),
                ("⠹", colors::BLUE),
                ("⠸", colors::MAGENTA),
                ("⠼", colors::BRIGHT_MAGENTA),
                ("⠴", colors::RED),
                ("⠦", colors::YELLOW),
                ("⠧", colors::BRIGHT_YELLOW),
                ("⠇", colors::GREEN),
                ("⠏", colors::BRIGHT_GREEN),
            ];

            let start = Instant::now();
            let mut frame_idx = 0;

            while running_clone.load(Ordering::Acquire) {
                let elapsed = start.elapsed();
                let (frame, color) = frames[frame_idx % frames.len()];

                // Format elapsed time
                let secs = elapsed.as_secs_f64();
                let time_str = if secs < 60.0 {
                    format!("{:.1}s", secs)
                } else {
                    format!("{}m {:.1}s", (secs / 60.0) as u32, secs % 60.0)
                };

                // Print spinner with colors
                print!(
                    "\r{}{}{} {}{}{} {}[{}]{}  ",
                    color,
                    frame,
                    colors::RESET,
                    colors::BOLD,
                    message_clone,
                    colors::RESET,
                    colors::DIM,
                    time_str,
                    colors::RESET
                );
                let _ = io::stdout().flush();

                frame_idx += 1;
                std::thread::sleep(Duration::from_millis(80));
            }
        });

        Self {
            message: message.to_string(),
            start_time: Instant::now(),
            running,
            handle: Some(handle),
        }
    }

    /// Get the current message
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Update the progress message
    #[allow(dead_code)]
    pub fn update(&self, message: &str) {
        print!(
            "\r{}{} {}{}{}  ",
            colors::CYAN,
            "⠿",
            colors::BOLD,
            message,
            colors::RESET
        );
        let _ = io::stdout().flush();
    }

    /// Stop the spinner with success
    pub fn success(mut self, message: &str) {
        self.stop_and_join_best_effort();
        let elapsed = self.start_time.elapsed();
        let (time_value, time_unit) = if elapsed.as_secs_f64() < 1.0 {
            (elapsed.as_secs_f64() * 1000.0, "ms")
        } else {
            (elapsed.as_secs_f64(), "s")
        };
        println!(
            "\r{}{}✓{} {}{}{} {}[{:.2}{}]{}",
            colors::BOLD,
            colors::GREEN,
            colors::RESET,
            colors::BOLD,
            message,
            colors::RESET,
            colors::DIM,
            time_value,
            time_unit,
            colors::RESET
        );
    }

    /// Stop the spinner with failure
    pub fn fail(mut self, message: &str) {
        self.stop_and_join_best_effort();
        let elapsed = self.start_time.elapsed();
        let (time_value, time_unit) = if elapsed.as_secs_f64() < 1.0 {
            (elapsed.as_secs_f64() * 1000.0, "ms")
        } else {
            (elapsed.as_secs_f64(), "s")
        };
        println!(
            "\r{}{}✗{} {}{}{} {}[{:.2}{}]{}",
            colors::BOLD,
            colors::RED,
            colors::RESET,
            colors::BOLD,
            message,
            colors::RESET,
            colors::DIM,
            time_value,
            time_unit,
            colors::RESET
        );
    }

    fn stop_and_join_best_effort(&mut self) {
        self.running.store(false, Ordering::Release);
        // Clear the line
        print!("\r{:80}\r", "");
        let _ = io::stdout().flush();

        if let Some(handle) = self.handle.take() {
            // Try to join promptly, but never block indefinitely.
            for _ in 0..8 {
                if handle.is_finished() {
                    let _ = handle.join();
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            // Drop handle without joining (detach) to guarantee progress.
            // Thread will observe running=false and exit shortly.
        }
    }
}

impl Drop for BuildProgress {
    fn drop(&mut self) {
        self.stop_and_join_best_effort();
    }
}
