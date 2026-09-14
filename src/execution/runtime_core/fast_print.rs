//! Lightning-Fast Print Pipeline
//!
//! This module implements a high-performance, zero-allocation print path that bypasses
//! the standard Value type system for hot loops. Designed with the following principles:
//!
//! ## Architecture
//! ```text
//! Frontend semantic print
//!         ↓
//! Typed fast-path print (NO NaN boxing)
//!         ↓
//! Backend I/O sink (OS / WASM / host)
//! ```
//!
//! ## Performance Targets
//! - 1,000,000 integer prints: < 1.5s buffered
//! - 1,000,000 loop with print at end: < 10ms  
//! - Print overhead vs arithmetic: < 10x in tight loops
//!
//! ## Key Optimizations
//! 1. **Typed fast-paths**: `print_int`, `print_float`, `print_bool`, `print_str`
//!    - Bypass Value dispatch entirely
//!    - Zero heap allocation for primitives
//!    - Direct buffer writes with itoa/ryu
//!
//! 2. **Global write buffer**: Single lock, batch flushes
//!    - 64KB buffer (optimal for syscall batching)
//!    - Flush only when full or explicit
//!
//! 3. **Monomorphic print variants**: Avoid dynamic dispatch
//!    - PRINT_INT, PRINT_STR, PRINT_DYNAMIC opcodes
//!
//! 4. **Loop-aware batching**: Accumulate output, single flush per loop

use std::cell::UnsafeCell;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

// ============================================================================
// CONFIGURATION CONSTANTS
// ============================================================================

/// Buffer size for batched output (64KB - optimal for syscall batching)
const WRITE_BUFFER_SIZE: usize = 64 * 1024;

/// Newline bytes
const NEWLINE: &[u8] = b"\n";

/// Static string slices for booleans (no allocation)
const TRUE_BYTES: &[u8] = b"true";
const FALSE_BYTES: &[u8] = b"false";
const NULL_BYTES: &[u8] = b"null";
const SPACE_BYTES: &[u8] = b" ";

// ============================================================================
// THREAD-LOCAL FAST PRINT BUFFER
// ============================================================================

/// Thread-local write buffer for ultra-fast batched printing.
///
/// This is **not** protected by a mutex because:
/// 1. Each thread has its own buffer (thread_local)
/// 2. We use UnsafeCell for interior mutability
/// 3. All operations are single-threaded within the print context
///
/// # Safety
/// The buffer is only accessed from the thread that owns it.
struct FastPrintBuffer {
    /// The actual byte buffer
    data: UnsafeCell<Vec<u8>>,
    /// Whether we've initialized the buffer
    initialized: AtomicBool,
}

impl FastPrintBuffer {
    const fn new() -> Self {
        Self {
            data: UnsafeCell::new(Vec::new()),
            initialized: AtomicBool::new(false),
        }
    }

    /// Get mutable access to the buffer, initializing if needed
    ///
    /// # Safety
    /// Must only be called from the owning thread
    #[inline(always)]
    #[allow(clippy::mut_from_ref)] // UnsafeCell interior mutability; caller holds &self on owning thread only.
    unsafe fn get_buffer(&self) -> &mut Vec<u8> {
        if !self.initialized.load(Ordering::Relaxed) {
            let buf = unsafe { &mut *self.data.get() };
            buf.reserve(WRITE_BUFFER_SIZE);
            self.initialized.store(true, Ordering::Relaxed);
        }
        unsafe { &mut *self.data.get() }
    }
}

// Safety: FastPrintBuffer is only accessed from its owning thread via thread_local
unsafe impl Sync for FastPrintBuffer {}

thread_local! {
    static FAST_BUFFER: FastPrintBuffer = const { FastPrintBuffer::new() };
}

// ============================================================================
// TYPED FAST-PATH PRINT FUNCTIONS
// ============================================================================

/// Print an i64 integer with zero heap allocation.
/// Uses itoa for blazing fast integer->string conversion.
#[inline(always)]
pub fn print_int(value: i64) {
    FAST_BUFFER.with(|fb| {
        // Safety: We're in the owning thread
        let buf = unsafe { fb.get_buffer() };

        // Use stack-allocated itoa buffer
        let mut itoa_buf = itoa::Buffer::new();
        let formatted = itoa_buf.format(value);

        buf.extend_from_slice(formatted.as_bytes());
        buf.extend_from_slice(NEWLINE);

        // Immediate flush so terminal output appears right away
        flush_fast_buffer_inner(buf);
    });
}

/// Print an i64 integer WITHOUT newline (for use in loops that batch output).
#[inline(always)]
pub fn print_int_no_newline(value: i64) {
    FAST_BUFFER.with(|fb| {
        let buf = unsafe { fb.get_buffer() };
        let mut itoa_buf = itoa::Buffer::new();
        let formatted = itoa_buf.format(value);
        buf.extend_from_slice(formatted.as_bytes());

        if buf.len() >= WRITE_BUFFER_SIZE - 64 {
            flush_fast_buffer_inner(buf);
        }
    });
}

/// Print an f64 float with zero heap allocation.
/// Uses ryu for ultra-fast float->string conversion.
#[inline(always)]
pub fn print_float(value: f64) {
    FAST_BUFFER.with(|fb| {
        let buf = unsafe { fb.get_buffer() };

        // Optimize: check if it's actually an integer
        if value.fract() == 0.0 && value.is_finite() && value.abs() < 1e15 {
            let mut itoa_buf = itoa::Buffer::new();
            let formatted = itoa_buf.format(value as i64);
            buf.extend_from_slice(formatted.as_bytes());
        } else {
            let mut ryu_buf = ryu::Buffer::new();
            let formatted = ryu_buf.format(value);
            buf.extend_from_slice(formatted.as_bytes());
        }
        buf.extend_from_slice(NEWLINE);

        if buf.len() >= WRITE_BUFFER_SIZE - 64 {
            flush_fast_buffer_inner(buf);
        }
    });
}

/// Print a boolean with zero allocation.
#[inline(always)]
pub fn print_bool(value: bool) {
    FAST_BUFFER.with(|fb| {
        let buf = unsafe { fb.get_buffer() };
        buf.extend_from_slice(if value { TRUE_BYTES } else { FALSE_BYTES });
        buf.extend_from_slice(NEWLINE);

        if buf.len() >= WRITE_BUFFER_SIZE - 64 {
            flush_fast_buffer_inner(buf);
        }
    });
}

/// Print a string slice with minimal overhead.
#[inline(always)]
pub fn print_str(value: &str) {
    FAST_BUFFER.with(|fb| {
        let buf = unsafe { fb.get_buffer() };
        buf.extend_from_slice(value.as_bytes());
        buf.extend_from_slice(NEWLINE);

        if buf.len() >= WRITE_BUFFER_SIZE - 64 {
            flush_fast_buffer_inner(buf);
        }
    });
}

/// Print a string slice WITHOUT newline.
#[inline(always)]
pub fn print_str_no_newline(value: &str) {
    FAST_BUFFER.with(|fb| {
        let buf = unsafe { fb.get_buffer() };
        buf.extend_from_slice(value.as_bytes());

        if buf.len() >= WRITE_BUFFER_SIZE - 64 {
            flush_fast_buffer_inner(buf);
        }
    });
}

/// Print null with zero allocation.
#[inline(always)]
pub fn print_null() {
    FAST_BUFFER.with(|fb| {
        let buf = unsafe { fb.get_buffer() };
        buf.extend_from_slice(NULL_BYTES);
        buf.extend_from_slice(NEWLINE);

        if buf.len() >= WRITE_BUFFER_SIZE - 64 {
            flush_fast_buffer_inner(buf);
        }
    });
}

/// Print a single char with minimal overhead.
#[inline(always)]
pub fn print_char(value: char) {
    FAST_BUFFER.with(|fb| {
        let buf = unsafe { fb.get_buffer() };
        let mut char_buf = [0u8; 4];
        let s = value.encode_utf8(&mut char_buf);
        buf.extend_from_slice(s.as_bytes());
        buf.extend_from_slice(NEWLINE);

        if buf.len() >= WRITE_BUFFER_SIZE - 64 {
            flush_fast_buffer_inner(buf);
        }
    });
}

/// Append a separator to the buffer (for multi-value prints).
#[inline(always)]
pub fn append_separator() {
    FAST_BUFFER.with(|fb| {
        let buf = unsafe { fb.get_buffer() };
        buf.extend_from_slice(SPACE_BYTES);
    });
}

/// Append a newline to the buffer.
#[inline(always)]
pub fn append_newline() {
    FAST_BUFFER.with(|fb| {
        let buf = unsafe { fb.get_buffer() };
        buf.extend_from_slice(NEWLINE);
        flush_fast_buffer_inner(buf);
    });
}

/// Append arbitrary bytes to the buffer.
#[inline(always)]
pub fn append_bytes(bytes: &[u8]) {
    FAST_BUFFER.with(|fb| {
        let buf = unsafe { fb.get_buffer() };
        buf.extend_from_slice(bytes);

        if buf.len() >= WRITE_BUFFER_SIZE - 64 {
            flush_fast_buffer_inner(buf);
        }
    });
}

// ============================================================================
// FLUSH OPERATIONS
// ============================================================================

/// Flush the thread-local buffer to stdout.
/// This is the ONLY place where syscalls happen for buffered output.
#[inline(never)] // Keep this out of hot paths
fn flush_fast_buffer_inner(buf: &mut Vec<u8>) {
    if buf.is_empty() {
        return;
    }

    crate::execution::runtime_core::stdio::write_stdout(buf);
    buf.clear();
}

/// Public flush function - call at end of program or before input.
pub fn flush_fast_buffer() {
    FAST_BUFFER.with(|fb| {
        let buf = unsafe { fb.get_buffer() };
        if !buf.is_empty() {
            crate::execution::runtime_core::stdio::write_stdout(buf);
            crate::execution::runtime_core::stdio::flush_stdout();
            buf.clear();
        }
    });
}

/// Force flush to ensure output is visible (e.g., before input or on explicit request).
pub fn flush_force() {
    flush_fast_buffer();
    let _ = io::stdout().flush();
}

// ============================================================================
// VALUE-BASED PRINT (FALLBACK)
// ============================================================================

use crate::parsing::ast::Value;

/// Print a Value using the fast path where possible.
/// Falls back to format::fmt() for complex types.
#[inline]
pub fn print_value(value: &Value) {
    match value {
        Value::Number(n) => {
            if n.fract() == 0.0 && n.is_finite() && n.abs() < 1e15 {
                print_int(*n as i64);
            } else {
                print_float(*n);
            }
        }
        Value::I64(n) => print_int(*n),
        Value::I32(n) => print_int(*n as i64),
        Value::I16(n) => print_int(*n as i64),
        Value::I8(n) => print_int(*n as i64),
        Value::U64(n) => {
            FAST_BUFFER.with(|fb| {
                let buf = unsafe { fb.get_buffer() };
                let mut itoa_buf = itoa::Buffer::new();
                let formatted = itoa_buf.format(*n);
                buf.extend_from_slice(formatted.as_bytes());
                buf.extend_from_slice(NEWLINE);
                if buf.len() >= WRITE_BUFFER_SIZE - 64 {
                    flush_fast_buffer_inner(buf);
                }
            });
        }
        Value::U32(n) => print_int(*n as i64),
        Value::U16(n) => print_int(*n as i64),
        Value::U8(n) => print_int(*n as i64),
        Value::F64(n) => print_float(*n),
        Value::F32(n) => print_float(*n as f64),
        Value::Bool(b) => print_bool(*b),
        Value::Str(s) => print_str(s),
        Value::Char(c) => print_char(*c),
        Value::Null => print_null(),
        // Complex types - fall back to formatted output
        _ => {
            let formatted = crate::execution::runtime::format::fmt(value);
            print_str(&formatted);
        }
    }
}

/// Print a Value WITHOUT a trailing newline.
#[inline]
pub fn print_value_no_newline(value: &Value) {
    match value {
        Value::Number(n) => {
            FAST_BUFFER.with(|fb| {
                let buf = unsafe { fb.get_buffer() };
                if n.fract() == 0.0 && n.is_finite() && n.abs() < 1e15 {
                    let mut itoa_buf = itoa::Buffer::new();
                    buf.extend_from_slice(itoa_buf.format(*n as i64).as_bytes());
                } else {
                    let mut ryu_buf = ryu::Buffer::new();
                    buf.extend_from_slice(ryu_buf.format(*n).as_bytes());
                }
                if buf.len() >= WRITE_BUFFER_SIZE - 64 {
                    flush_fast_buffer_inner(buf);
                }
            });
        }
        Value::I64(n) => print_int_no_newline(*n),
        Value::I32(n) => print_int_no_newline(*n as i64),
        Value::I16(n) => print_int_no_newline(*n as i64),
        Value::I8(n) => print_int_no_newline(*n as i64),
        Value::U64(n) => {
            FAST_BUFFER.with(|fb| {
                let buf = unsafe { fb.get_buffer() };
                let mut itoa_buf = itoa::Buffer::new();
                buf.extend_from_slice(itoa_buf.format(*n).as_bytes());
                if buf.len() >= WRITE_BUFFER_SIZE - 64 {
                    flush_fast_buffer_inner(buf);
                }
            });
        }
        Value::U32(n) => print_int_no_newline(*n as i64),
        Value::U16(n) => print_int_no_newline(*n as i64),
        Value::U8(n) => print_int_no_newline(*n as i64),
        Value::F64(n) => {
            FAST_BUFFER.with(|fb| {
                let buf = unsafe { fb.get_buffer() };
                if n.fract() == 0.0 && n.is_finite() && n.abs() < 1e15 {
                    let mut itoa_buf = itoa::Buffer::new();
                    buf.extend_from_slice(itoa_buf.format(*n as i64).as_bytes());
                } else {
                    let mut ryu_buf = ryu::Buffer::new();
                    buf.extend_from_slice(ryu_buf.format(*n).as_bytes());
                }
                if buf.len() >= WRITE_BUFFER_SIZE - 64 {
                    flush_fast_buffer_inner(buf);
                }
            });
        }
        Value::F32(n) => {
            FAST_BUFFER.with(|fb| {
                let buf = unsafe { fb.get_buffer() };
                let mut ryu_buf = ryu::Buffer::new();
                buf.extend_from_slice(ryu_buf.format(*n).as_bytes());
                if buf.len() >= WRITE_BUFFER_SIZE - 64 {
                    flush_fast_buffer_inner(buf);
                }
            });
        }
        Value::Bool(b) => {
            FAST_BUFFER.with(|fb| {
                let buf = unsafe { fb.get_buffer() };
                buf.extend_from_slice(if *b { TRUE_BYTES } else { FALSE_BYTES });
            });
        }
        Value::Str(s) => print_str_no_newline(s),
        Value::Char(c) => {
            FAST_BUFFER.with(|fb| {
                let buf = unsafe { fb.get_buffer() };
                let mut char_buf = [0u8; 4];
                let s = c.encode_utf8(&mut char_buf);
                buf.extend_from_slice(s.as_bytes());
            });
        }
        Value::Null => {
            FAST_BUFFER.with(|fb| {
                let buf = unsafe { fb.get_buffer() };
                buf.extend_from_slice(NULL_BYTES);
            });
        }
        _ => {
            let formatted = crate::execution::runtime::format::fmt(value);
            print_str_no_newline(&formatted);
        }
    }
}

// ============================================================================
// MULTI-VALUE PRINT (OPTIMIZED)
// ============================================================================

/// Print multiple values with a separator and end string.
/// Optimized for the common case of simple values.
#[inline]
pub fn print_values(values: &[Value], sep: &str, end: &str) {
    if values.is_empty() {
        append_bytes(end.as_bytes());
        return;
    }

    FAST_BUFFER.with(|fb| {
        let buf = unsafe { fb.get_buffer() };
        let sep_bytes = sep.as_bytes();

        for (i, value) in values.iter().enumerate() {
            if i > 0 {
                buf.extend_from_slice(sep_bytes);
            }

            // Inline fast paths for primitives
            match value {
                Value::Number(n) => {
                    if n.fract() == 0.0 && n.is_finite() && n.abs() < 1e15 {
                        let mut itoa_buf = itoa::Buffer::new();
                        buf.extend_from_slice(itoa_buf.format(*n as i64).as_bytes());
                    } else {
                        let mut ryu_buf = ryu::Buffer::new();
                        buf.extend_from_slice(ryu_buf.format(*n).as_bytes());
                    }
                }
                Value::I64(n) => {
                    let mut itoa_buf = itoa::Buffer::new();
                    buf.extend_from_slice(itoa_buf.format(*n).as_bytes());
                }
                Value::I32(n) => {
                    let mut itoa_buf = itoa::Buffer::new();
                    buf.extend_from_slice(itoa_buf.format(*n).as_bytes());
                }
                Value::Bool(b) => {
                    buf.extend_from_slice(if *b { TRUE_BYTES } else { FALSE_BYTES });
                }
                Value::Str(s) => {
                    buf.extend_from_slice(s.as_bytes());
                }
                Value::Char(c) => {
                    let mut char_buf = [0u8; 4];
                    let s = c.encode_utf8(&mut char_buf);
                    buf.extend_from_slice(s.as_bytes());
                }
                Value::Null => {
                    buf.extend_from_slice(NULL_BYTES);
                }
                _ => {
                    let formatted = crate::execution::runtime::format::fmt(value);
                    buf.extend_from_slice(formatted.as_bytes());
                }
            }
        }

        buf.extend_from_slice(end.as_bytes());

        // Auto-flush if buffer is getting full
        if buf.len() >= WRITE_BUFFER_SIZE - 256 {
            flush_fast_buffer_inner(buf);
        }
    });
}

// ============================================================================
// BENCHMARKING HELPERS
// ============================================================================

#[cfg(feature = "print-bench")]
pub mod bench {
    use std::time::Instant;

    /// Benchmark integer printing
    pub fn bench_int_print(count: usize) -> std::time::Duration {
        let start = Instant::now();
        for i in 0..count {
            super::print_int(i as i64);
        }
        super::flush_fast_buffer();
        start.elapsed()
    }

    /// Benchmark float printing
    pub fn bench_float_print(count: usize) -> std::time::Duration {
        let start = Instant::now();
        for i in 0..count {
            super::print_float(i as f64 + 0.5);
        }
        super::flush_fast_buffer();
        start.elapsed()
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_print_int_format() {
        // Just ensure it doesn't panic
        print_int(42);
        print_int(-12345);
        print_int(0);
        print_int(i64::MAX);
        print_int(i64::MIN);
        flush_fast_buffer();
    }

    #[test]
    fn test_print_float_format() {
        print_float(3.14159);
        print_float(0.0);
        print_float(-1e10);
        print_float(42.0); // Should use int path
        flush_fast_buffer();
    }

    #[test]
    fn test_print_str() {
        print_str("Hello, World!");
        print_str("");
        print_str("Unicode: 你好世界 🚀");
        flush_fast_buffer();
    }

    #[test]
    fn test_buffer_auto_flush() {
        // Fill buffer to trigger auto-flush
        for i in 0..10000 {
            print_int(i);
        }
        flush_fast_buffer();
    }
}
