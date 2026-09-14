//! Type System Module
//!
//! This module contains the complete type system for the language runtime:
//! - Type information and definitions
//! - Type checking and inference
//! - Traits system
//! - VTable implementation
//! - Type layouts (memory representation)
//! - Visibility rules
//! - Safe references
//! - Standard library safeguards
//! - Array types
//! - Value optimizations

#![allow(ambiguous_glob_reexports)]

pub mod checker;
pub mod layouts;
pub mod references;
pub mod safeguards;
pub mod traits;
pub mod visibility;
pub mod vtable;

// Re-export core type modules
pub mod array_types;
pub mod type_info;
pub mod type_system;
pub mod value_optimized;

// Re-export commonly used types

pub use array_types::*;
pub use checker::*;
pub use layouts::*;
pub use references::*;
pub use safeguards::*;
pub use traits::*;
pub use type_info::*;
pub use type_system::*;
pub use value_optimized::*;
pub use visibility::*;
pub use vtable::*;
