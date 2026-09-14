//! JIT Backend Module
//!
//! Just-In-Time compilation backends for AdeshLang:
//! - Cranelift: Main JIT compilation engine
//! - Tiered: Multi-tier JIT with interpreter fallback
//! - Adaptive: Adaptive JIT with profiling and optimization
//! - Optimizations: JIT-specific optimizations
//! - Array operations: Specialized array operation handling

#![allow(ambiguous_glob_reexports)]

pub mod adaptive;
pub mod array_ops;
pub mod cranelift;
pub mod native;
pub mod optimizations;
pub mod tiered;

pub use adaptive::*;
pub use array_ops::*;
pub use cranelift::*;
pub use native::*;
pub use optimizations::*;
pub use tiered::*;

// Re-export common backend modules so submodules can access them via super::
pub use self::optimizations as recursion_opt;
pub use super::common::builtins;
pub use super::common::heap as unsafe_heap;
pub use super::common::lir;
pub use super::common::lir::lower as lir_lower;
