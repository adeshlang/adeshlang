//! Cross-Platform Interactive TUI Input Subsystem
//!
//! Powered by Ratatui & Crossterm for all operating systems (Windows, macOS, Linux).
//! Provides rich terminal interfaces, robust terminal state restoration,
//! mock/playback test automation fallback, and revolutionary input widgets.

#[cfg(not(target_arch = "wasm32"))]
pub mod widgets;

#[cfg(not(target_arch = "wasm32"))]
pub use widgets::*;

#[cfg(not(target_arch = "wasm32"))]
use std::io::{self, Stdout, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(not(target_arch = "wasm32"))]
use crossterm::{
    cursor,
    event::{self, Event, KeyEvent, KeyEventKind},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
#[cfg(not(target_arch = "wasm32"))]
use ratatui::{
    backend::CrosstermBackend,
    Terminal, TerminalOptions, Viewport,
};

#[cfg(not(target_arch = "wasm32"))]
/// Helper to drain any pending input events from the console buffer
pub fn drain_events() {
    while event::poll(Duration::from_millis(5)).unwrap_or(false) {
        let _ = event::read();
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// Terminal guard that guarantees restoration of raw mode and screen buffer on exit or panic.
pub struct TerminalGuard {
    is_alternate: bool,
    is_raw: bool,
    height: u16,
}

#[cfg(not(target_arch = "wasm32"))]
impl TerminalGuard {
    /// Initialize inline raw terminal mode for quick prompts
    pub fn new_inline(height: u16) -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, cursor::Hide)?;
        drain_events();
        Ok(Self {
            is_alternate: false,
            is_raw: true,
            height,
        })
    }

    /// Initialize full alternate-screen terminal mode for complex widgets (tables, forms, trees)
    pub fn new_alternate() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, cursor::Hide)?;
        drain_events();
        Ok(Self {
            is_alternate: true,
            is_raw: true,
            height: 0,
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        let _ = execute!(stdout, cursor::Show);
        if self.is_alternate {
            let _ = execute!(stdout, LeaveAlternateScreen);
        } else {
            if self.height > 0 {
                let _ = execute!(stdout, cursor::MoveToColumn(0), cursor::MoveDown(self.height));
            }
            let _ = writeln!(stdout);
            let _ = stdout.flush();
        }
        if self.is_raw {
            let _ = terminal::disable_raw_mode();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// Create an inline Ratatui terminal instance with fixed viewport height
pub fn init_inline_terminal(height: u16) -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(height),
        },
    )
}

#[cfg(not(target_arch = "wasm32"))]
/// Create a full-screen Ratatui terminal instance
pub fn init_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

#[cfg(not(target_arch = "wasm32"))]
/// Poll for a key event with a timeout, ignoring Release events
pub fn poll_key_event(timeout: Duration) -> io::Result<Option<KeyEvent>> {
    let start = std::time::Instant::now();
    let mut remaining = timeout;
    while event::poll(remaining)? {
        match event::read()? {
            Event::Key(key_event) => {
                if key_event.kind == KeyEventKind::Press || key_event.kind == KeyEventKind::Repeat {
                    return Ok(Some(key_event));
                }
            }
            _ => {}
        }
        let elapsed = start.elapsed();
        if elapsed >= timeout {
            break;
        }
        remaining = timeout - elapsed;
    }
    Ok(None)
}

#[cfg(target_arch = "wasm32")]
pub mod wasm_stubs;

#[cfg(target_arch = "wasm32")]
pub use wasm_stubs::*;
