//! Execution Backends
//!
//! Aggregates all execution backends:
//! - `jit`: JIT compilation (Cranelift, tiered, adaptive, optimizations, array ops)
//! - `aot`: Ahead-of-time compilation (Cranelift AOT, linker, memory tracking)
//! - `wasm_backend`: WebAssembly compilation and linking
//! - `mlir`: MLIR backend (GPU acceleration, LLVM lowering)
//! - `lowering`: Direct VIR to backend lowering (bypassing LIR)
//! - `common`: Shared backend infrastructure (LIR, builtins, FFI, escape analysis, VIR adapter, etc.)
//!
//! Backends consume `parsing::HIR` or `ir::VIR` via common infrastructure.

#[cfg(not(target_arch = "wasm32"))]
pub mod aot;
pub mod common;
pub mod interpreter_backend;
#[cfg(not(target_arch = "wasm32"))]
pub mod jit;
#[cfg(not(target_arch = "wasm32"))]
pub mod llvm;
pub mod lowering;
#[cfg(not(target_arch = "wasm32"))]
pub mod mlir;
pub mod wasm;
#[cfg(not(target_arch = "wasm32"))]
pub mod wasm_backend;

// Re-export common modules at the backends level for convenience
pub use common::*;

// Backward compatibility re-exports for old module names
#[cfg(not(target_arch = "wasm32"))]
pub use jit::adaptive as adaptive_jit;
#[cfg(not(target_arch = "wasm32"))]
pub use jit::array_ops as jit_array_ops;
#[cfg(not(target_arch = "wasm32"))]
pub use jit::optimizations as jit_opt;
#[cfg(not(target_arch = "wasm32"))]
pub use jit::optimizations::recursion as recursion_opt;
#[cfg(not(target_arch = "wasm32"))]
pub use jit::tiered as tiered_jit;

#[cfg(not(target_arch = "wasm32"))]
pub use aot::cranelift as cranelift_aot;
#[cfg(not(target_arch = "wasm32"))]
pub use aot::linker as linker_driver;
#[cfg(not(target_arch = "wasm32"))]
pub use aot::memory as aot_memory;

pub use common::backend;
pub use common::builtins;
pub use common::concurrency;
pub use common::escape as escape_analysis;
pub use common::ffi::c_header_parser;
pub use common::ffi::generator as ffi_generator;
pub use common::ffi::import as ffi_import;
pub use common::heap as unsafe_heap;
pub use common::leak as leak_detector;
pub use common::lir;
pub use common::lir::lower as lir_lower;
pub use common::ml;

#[cfg(not(target_arch = "wasm32"))]
pub use wasm_backend::linker as wasm_linker;
