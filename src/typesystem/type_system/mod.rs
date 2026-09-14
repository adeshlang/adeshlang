//! AST-Integrated Type Checking
//!
//! A conservative, lightweight pass that:
//! - Collects function/class/extension method signatures and generics
//! - Supports simple type aliases and generic alias instantiation
//! - Checks callsites and return statements against annotations
//! - Performs basic flow-sensitive narrowing (null checks, typeof)
//! - Provides helpers to resolve type names and parse function/generic syntax

pub(crate) mod entry;
pub(crate) mod environment;
pub(crate) mod expression_inference;
pub(crate) mod narrowing;
pub(crate) mod resolution;
pub(crate) mod statement_checking;

#[cfg(test)]
mod tests;

// Re-export public API to maintain backward compatibility
pub use entry::{check_module, check_module_in};
pub use resolution::type_from_name;
