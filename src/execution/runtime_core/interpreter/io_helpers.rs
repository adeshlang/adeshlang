//! Terminal I/O helpers for input() and related builtins.
//!
//! Provides wrappers around crossterm for raw mode terminal input.

#![allow(dead_code)]

#[cfg(not(target_arch = "wasm32"))]
use crossterm::event::{Event as CtEvent, poll, read};
#[cfg(not(target_arch = "wasm32"))]
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
pub use crossterm::event::{Event, KeyCode};

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyCode {
    Char(char),
    Enter,
    Backspace,
    Esc,
    Null,
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Key(KeyCode),
}

#[cfg(not(target_arch = "wasm32"))]
/// Enable raw terminal mode for character-by-character input.
#[inline]
pub(crate) fn ct_enable_raw() -> Result<(), std::io::Error> {
    enable_raw_mode()
}

#[cfg(target_arch = "wasm32")]
#[inline]
pub(crate) fn ct_enable_raw() -> Result<(), std::io::Error> {
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
/// Disable raw terminal mode, restoring line-buffered input.
#[inline]
pub(crate) fn ct_disable_raw() -> Result<(), std::io::Error> {
    disable_raw_mode()
}

#[cfg(target_arch = "wasm32")]
#[inline]
pub(crate) fn ct_disable_raw() -> Result<(), std::io::Error> {
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
/// Poll for terminal input with timeout.
#[inline]
pub(crate) fn ct_poll(timeout: Duration) -> Result<bool, std::io::Error> {
    poll(timeout)
}

#[cfg(target_arch = "wasm32")]
#[inline]
pub(crate) fn ct_poll(_timeout: Duration) -> Result<bool, std::io::Error> {
    Ok(false)
}

#[cfg(not(target_arch = "wasm32"))]
/// Read a terminal event (blocking).
#[inline]
pub(crate) fn ct_read() -> Result<CtEvent, std::io::Error> {
    read()
}

#[cfg(target_arch = "wasm32")]
#[inline]
pub(crate) fn ct_read() -> Result<Event, std::io::Error> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "Terminal raw reading is not supported in browser WASM"))
}
