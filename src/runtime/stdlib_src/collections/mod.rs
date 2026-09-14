//! Collections Stdlib
//!
//! Array utilities like `map`, `filter`, `reduce` registered under the
//! `collections` category.
pub mod array;
pub mod collections_builtins;

use crate::stdlib::registry::BuiltinRegistry;

pub fn register_all(registry: &mut BuiltinRegistry) {
    array::register(registry);
    collections_builtins::register(registry);
}
