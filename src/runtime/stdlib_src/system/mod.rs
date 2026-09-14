//! System Stdlib
//!
//! Command-line argument helpers, environment variable access, and time/date.
//! These utilities integrate with the runtime's program args and runtime env.
pub mod args;
pub mod process;
pub mod time;

use crate::stdlib::registry::BuiltinRegistry;

pub fn register_all(registry: &mut BuiltinRegistry) {
    args::register(registry);
    process::register(registry);
    time::register(registry);
}
