//! JSON Stdlib
//!
//! JSON parsing and stringification helpers registered under `json_*` names.
pub mod extended;
pub mod parse;
pub mod stringify;

use crate::stdlib::registry::BuiltinRegistry;

pub fn register_all(registry: &mut BuiltinRegistry) {
    parse::register(registry);
    stringify::register(registry);
    extended::register(registry);
}
