//! Cranelift JIT implementation modules

pub mod api;
pub(in crate::backends::jit::cranelift) mod arrays;
pub(in crate::backends::jit::cranelift) mod async_ops;
pub(in crate::backends::jit::cranelift) mod frame;
pub(in crate::backends::jit::cranelift) mod modules;
pub(crate) mod types;

pub(in crate::backends::jit::cranelift) use frame::JitFrame;
pub use types::{JitMemoryStats, JitVariableInfo};
pub(crate) use types::{expand_header_imports, runtime_type_name, sizeof_runtime_value};
