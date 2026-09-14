//! Global state and static variables for the interpreter runtime.
//!
//! This module manages:
//! - Promise counter for unique promise IDs
//! - Program arguments
//! - Runtime environment variables
//! - Input record/playback for testing
//! - Generic type context for input<T>() builtins

use rustc_hash::FxHashMap as HashMap;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

/// Global atomic counter for promise IDs.
pub(in crate::execution::runtime_core) static PROMISE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Program command-line arguments.
pub(in crate::execution::runtime_core) static PROGRAM_ARGS: OnceLock<Vec<String>> = OnceLock::new();

/// Program name (argv[0]).
static PROGRAM_NAME: OnceLock<String> = OnceLock::new();

/// Runtime environment variables (separate from system env).
pub(in crate::execution::runtime_core) static RUNTIME_ENV: OnceLock<
    Mutex<HashMap<String, String>>,
> = OnceLock::new();

/// Recording of input() calls for test playback.
pub(in crate::execution::runtime_core) static INPUT_RECORD: OnceLock<Mutex<Option<Vec<String>>>> =
    OnceLock::new();

/// Playback queue for input() calls in test mode.
pub static INPUT_PLAYBACK: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();

// Thread-local storage for generic type context (used by input<T>() and similar builtins)
thread_local! {
    /// Generic type context stack for input<T>() and similar builtins.
    ///
    /// This allows builtins to know which type parameter was passed
    /// (e.g., input<int>() pushes "int" onto the stack).
    pub static GENERIC_TYPE_CONTEXT: std::cell::RefCell<Vec<String>> = std::cell::RefCell::new(Vec::new());
}

/// Generate a new unique promise ID.
#[inline]
#[allow(dead_code)]
pub(crate) fn next_promise_id() -> u64 {
    PROMISE_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Set program arguments (called at startup).
pub fn set_program_args(args: Vec<String>, name: Option<String>) {
    let _ = PROGRAM_ARGS.set(args);
    if let Some(n) = name {
        let _ = PROGRAM_NAME.set(n);
    }
    // Initialize other statics
    if INPUT_RECORD.get().is_none() {
        let _ = INPUT_RECORD.set(Mutex::new(None));
    }
    if INPUT_PLAYBACK.get().is_none() {
        let _ = INPUT_PLAYBACK.set(Mutex::new(VecDeque::new()));
    }
    if RUNTIME_ENV.get().is_none() {
        let _ = RUNTIME_ENV.set(Mutex::new(HashMap::default()));
    }
}

/// Get program arguments.
pub fn get_program_args() -> Vec<String> {
    PROGRAM_ARGS.get().cloned().unwrap_or_default()
}

/// Get program name.
pub fn get_program_name() -> String {
    PROGRAM_NAME.get().cloned().unwrap_or_default()
}

/// Get runtime environment variable.
pub fn runtime_env_get(key: &str) -> Option<String> {
    RUNTIME_ENV.get()?.lock().ok()?.get(key).cloned()
}

/// Check if runtime environment variable exists.
pub fn runtime_env_has(key: &str) -> bool {
    RUNTIME_ENV
        .get()
        .and_then(|m| m.lock().ok())
        .map_or(false, |m| m.contains_key(key))
}

/// Get all runtime environment variables.
pub fn runtime_env_all() -> HashMap<String, String> {
    RUNTIME_ENV
        .get()
        .and_then(|m| m.lock().ok())
        .map(|m| m.clone())
        .unwrap_or_default()
}

/// Load environment variables from a map.
///
/// Returns the number of variables loaded.
pub fn runtime_env_load(map: HashMap<String, String>, overwrite: bool) -> usize {
    let env = RUNTIME_ENV.get_or_init(|| Mutex::new(HashMap::default()));
    let mut guard = env.lock().unwrap();
    let mut count = 0;
    for (k, v) in map {
        if overwrite || !guard.contains_key(&k) {
            guard.insert(k, v);
            count += 1;
        }
    }
    count
}

/// Set a runtime environment variable and system environment variable.
pub fn runtime_env_set(key: &str, value: &str) {
    let env = RUNTIME_ENV.get_or_init(|| Mutex::new(HashMap::default()));
    if let Ok(mut guard) = env.lock() {
        guard.insert(key.to_string(), value.to_string());
    }
    unsafe {
        std::env::set_var(key, value);
    }
}

/// Remove a runtime environment variable and system environment variable.
pub fn runtime_env_remove(key: &str) {
    if let Some(env) = RUNTIME_ENV.get() {
        if let Ok(mut guard) = env.lock() {
            guard.remove(key);
        }
    }
    unsafe {
        std::env::remove_var(key);
    }
}

/// Clear all runtime environment variables.
pub fn runtime_env_clear() {
    if let Some(env) = RUNTIME_ENV.get() {
        if let Ok(mut guard) = env.lock() {
            let keys: Vec<String> = guard.keys().cloned().collect();
            guard.clear();
            for k in keys {
                unsafe {
                    std::env::remove_var(&k);
                }
            }
        }
    }
}
