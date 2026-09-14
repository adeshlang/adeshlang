//! Common Backend Module
//!
//! Shared functionality across all backends:
//! - Backend trait and common interfaces
//! - Builtins shared by all backends
//! - LIR (Low-level Intermediate Representation)
//! - Escape analysis
//! - Concurrency primitives
//! - Memory leak detection
//! - Heap management
//! - Machine learning optimizations
//! - FFI (Foreign Function Interface)

#![allow(ambiguous_glob_reexports)]

pub mod backend;
pub mod builtins;
pub mod builtins_modules;
pub mod concurrency;
pub mod escape;
pub mod ffi;
pub mod heap;
pub mod leak;
pub mod lir;
pub mod ml;
pub mod vir_adapter;
pub mod vir_lir_bridge;

pub use backend::*;
pub use builtins::*;
pub use concurrency::*;
pub use escape::*;
pub use ffi::*;
pub use heap::*;
pub use leak::*;
pub use lir::*;
pub use ml::*;
pub use vir_adapter::*;
pub use vir_lir_bridge::*;
