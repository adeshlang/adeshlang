//! IO Stdlib
//!
//! File and IO-related helpers. See `file` for reading; writing can be added
//! with care to avoid side effects in restricted environments.
pub mod file;
pub mod stream_builtins;

use crate::stdlib::registry::BuiltinRegistry;

pub fn register_all(registry: &mut BuiltinRegistry) {
    file::register(registry);
    stream_builtins::register(registry);
}
