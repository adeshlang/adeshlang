//! Runtime ABI (Application Binary Interface)
//!
//! This module provides a unified interface for all runtime operations,
//! ensuring consistent semantics across interpreter, VM, JIT, and AOT backends.
//!
//! ## Design Principles
//!
//! 1. **Single Source of Truth**: All backends call the same ABI functions
//! 2. **No Duplication**: Operations implemented once, used everywhere
//! 3. **Type Safety**: Strong typing with clear error handling
//! 4. **Performance**: Designed for inlining and optimization
//!
//! ## Module Organization
//!
//! - `ops`: Arithmetic and comparison operations (Value-based)
//! - `conversions`: Value ↔ NanValue conversion layer
//! - `arrays`: Array manipulation and access
//! - `objects`: Object property access and manipulation
//! - `strings`: String operations
//! - `math`: Mathematical functions
//! - `async_ops`: Promise and async primitives

pub mod bitwise;
pub mod conversions;
pub mod ops;
pub mod ops_nanvalue;
pub mod versioning;

// Re-export commonly used items
pub use conversions::{nanvalue_to_value, value_to_nanvalue};
pub use ops::{abi_add, abi_div, abi_mod, abi_mul, abi_sub};
pub use ops::{abi_cmp_eq, abi_cmp_ge, abi_cmp_gt, abi_cmp_le, abi_cmp_lt, abi_cmp_ne};
pub use ops::{abi_equals, abi_negate, abi_not};
pub use ops_nanvalue::{abi_add_nan, abi_div_nan, abi_mod_nan, abi_mul_nan, abi_sub_nan};
pub use ops_nanvalue::{
    abi_cmp_eq_nan, abi_cmp_ge_nan, abi_cmp_gt_nan, abi_cmp_le_nan, abi_cmp_lt_nan, abi_cmp_ne_nan,
};
pub use ops_nanvalue::{abi_equals_nan, abi_negate_nan, abi_not_nan, is_falsy_nan};
pub use versioning::{AbiAttribute, AbiVersion, CallingConvention, StructLayout, TypeRepr};

/// Runtime error type for ABI operations
#[derive(Debug, Clone)]
pub struct RuntimeError {
    pub message: String,
}

impl RuntimeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl From<RuntimeError> for String {
    fn from(err: RuntimeError) -> String {
        err.message
    }
}

impl From<String> for RuntimeError {
    fn from(message: String) -> Self {
        RuntimeError { message }
    }
}
