//! Self-Contained Native Compiler Backend & Pipeline.

pub mod lower;

pub use lower::{FunctionLoweringContext, NativeLoweringError, lower_hir_module};
