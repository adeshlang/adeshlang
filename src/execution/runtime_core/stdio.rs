use std::io::{self, BufWriter, Stdout, Write};
use std::sync::{Mutex, OnceLock};

// Global standard output buffer (16KB) to dramatically reduce syscalls
// for high-frequency printing (e.g., tight loops).
//
// We use a high-performance buffering strategy:
// 1. All `print()` calls write to this buffer.
// 2. The buffer flushes ONLY when full or explicitly requested.
// 3. We must manually flush on program exit and before input.

static STDOUT_BUFFER: OnceLock<Mutex<BufWriter<Stdout>>> = OnceLock::new();
static OUTPUT_CAPTURE: OnceLock<Mutex<Option<Vec<u8>>>> = OnceLock::new();

fn get_capture_lock() -> &'static Mutex<Option<Vec<u8>>> {
    OUTPUT_CAPTURE.get_or_init(|| Mutex::new(None))
}

/// Begin capturing stdout into an in-memory buffer (for WASM / testing).
pub fn start_output_capture() {
    if let Ok(mut lock) = get_capture_lock().lock() {
        *lock = Some(Vec::new());
    }
}

/// Finish capturing stdout and return the captured output as a UTF-8 String.
pub fn finish_output_capture() -> String {
    if let Ok(mut lock) = get_capture_lock().lock() {
        if let Some(buf) = lock.take() {
            return String::from_utf8_lossy(&buf).to_string();
        }
    }
    String::new()
}

/// Get the global stdout buffer, initializing it if necessary.
pub fn get_stdout_buffer() -> &'static Mutex<BufWriter<Stdout>> {
    STDOUT_BUFFER.get_or_init(|| {
        // 16KB buffer - larger than standard 8KB for better bulk performance
        // while still being reasonable for memory.
        Mutex::new(BufWriter::with_capacity(16 * 1024, io::stdout()))
    })
}

/// Write data to the global stdout buffer or capture buffer.
pub fn write_stdout(data: &[u8]) {
    if let Ok(mut cap_lock) = get_capture_lock().lock() {
        if let Some(buf) = cap_lock.as_mut() {
            buf.extend_from_slice(data);
            return;
        }
    }
    if let Ok(mut lock) = get_stdout_buffer().lock() {
        let _ = lock.write_all(data);
        // Note: We do NOT flush here. That's the key optimization.
    }
}

/// Flush the global stdout buffer.
pub fn flush_stdout() {
    if let Some(mutex) = STDOUT_BUFFER.get() {
        if let Ok(mut lock) = mutex.lock() {
            let _ = lock.flush();
        }
    }
}

/// Ensure everything is flushed (global buffer + underlying stdout).
pub fn flush_all() {
    flush_stdout();
    let _ = io::stdout().flush();
}
