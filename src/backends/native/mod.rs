//! Self-Contained Native Compiler Backend & Pipeline.

pub mod lower;
pub mod vir_lower;

pub use lower::{
    FunctionLoweringContext, NativeLoweringError, lower_hir_module, lower_hir_module_with_base,
};
pub use vir_lower::vir_to_machine_ir;
