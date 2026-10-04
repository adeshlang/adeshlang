//! Phase 9 Production Panic & Error Runtime.
//!
//! Provides:
//! - Configurable panic strategy: `panic=abort` vs `panic=unwind`
//! - Thread-safe panic hook and diagnostic registration
//! - RAII cleanup and drop handler execution during unwinding
//! - FFI boundary defense: prevents panics from escaping across C/foreign ABI boundaries

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static PANIC_IS_ABORT: AtomicBool = AtomicBool::new(false);
static CLEANUP_STACK: Mutex<Vec<Box<dyn Fn() + Send + 'static>>> = Mutex::new(Vec::new());

/// Panic behavior configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanicStrategy {
    Abort,
    Unwind,
}

/// Set global panic strategy.
pub fn set_panic_strategy(strategy: PanicStrategy) {
    match strategy {
        PanicStrategy::Abort => PANIC_IS_ABORT.store(true, Ordering::SeqCst),
        PanicStrategy::Unwind => PANIC_IS_ABORT.store(false, Ordering::SeqCst),
    }
}

/// Get current panic strategy.
pub fn get_panic_strategy() -> PanicStrategy {
    if PANIC_IS_ABORT.load(Ordering::SeqCst) {
        PanicStrategy::Abort
    } else {
        PanicStrategy::Unwind
    }
}

/// Register a cleanup / destructor callback to execute during panic unwinding.
pub fn register_cleanup_handler<F>(cleanup: F)
where
    F: Fn() + Send + 'static,
{
    let mut stack = CLEANUP_STACK.lock().unwrap();
    stack.push(Box::new(cleanup));
}

/// Execute all registered cleanup handlers in LIFO order.
pub fn run_unwind_cleanups() {
    let mut handlers = {
        let mut stack = CLEANUP_STACK.lock().unwrap();
        std::mem::take(&mut *stack)
    };
    while let Some(handler) = handlers.pop() {
        handler();
    }
}

/// Trigger an Adesh runtime panic.
/// If strategy is Abort, immediately terminates the process with diagnostic.
/// If strategy is Unwind, executes RAII cleanups and bubbles up safely.
pub fn adesh_panic(message: &str) -> ! {
    eprintln!("Adesh runtime panic: {}", message);
    run_unwind_cleanups();

    if get_panic_strategy() == PanicStrategy::Abort {
        std::process::abort();
    } else {
        panic!("Adesh unwound: {}", message);
    }
}

/// FFI boundary guard: catches any unwinding panics originating from Adesh code
/// and converts them to error codes, preventing illegal unwinding across foreign ABIs.
pub fn catch_unwind_safe<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce() -> R + std::panic::UnwindSafe,
{
    std::panic::catch_unwind(f).map_err(|e| {
        if let Some(s) = e.downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = e.downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic in Adesh execution".to_string()
        }
    })
}
