//! Self-Contained Native Compiler Backend & Pipeline.

pub mod lower;

pub use lower::{FunctionLoweringContext, lower_hir_module};
