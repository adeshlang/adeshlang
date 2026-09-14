//! Core Stdlib
//!
//! Registers essential builtins used across programs:
//! - `types`: type queries/utilities
//! - `print`: printing functions
//! - `operators`: operator helpers used by runtime
pub mod operators;
pub mod print;
pub mod regex;
pub mod types;

use crate::stdlib::registry::BuiltinRegistry;

pub fn register_all(registry: &mut BuiltinRegistry) {
    types::register(registry);
    print::register(registry);
    operators::register(registry);
    regex::register_all(registry);
}
