//! AOT Backend Module
//!
//! Ahead-Of-Time compilation backends:
//! - Cranelift: AOT compilation using Cranelift backend
//! - Cranelift_impl: Modularized Cranelift implementation components
//! - Linker: Native binary linking
//! - Memory: AOT memory management and tracking
//! - Symbols: Symbol table and resolution
//! - ABI: Application Binary Interface specifications
//! - Object Gen: Object file generation
//! - Module Linking: Cross-module dependency management

pub mod cache;
pub mod cranelift;
pub mod cranelift_impl;
pub mod linker;
pub mod memory;
pub mod runtime_bridge; // AOT runtime bridge for FFI

// Phase 6: AOT Reintegration modules
pub mod abi;
pub mod module_linking;
pub mod object_gen;
pub mod static_linker;
pub mod symbols; // Static linker (renamed to avoid conflict)

pub use cache::*;
pub use cranelift::*;
pub use linker::*;
pub use memory::*;

// Phase 6 exports
pub use abi::{Abi, CallingConvention, DataLayout};
pub use module_linking::{DependencyGraph, ModuleDependency, ModuleInterface};
pub use object_gen::{DebugInfo, ObjectFormat, ObjectGenerator};
pub use static_linker::{Linker as StaticLinker, ObjectFile, Relocation, RelocationType};
pub use symbols::{Symbol, SymbolTable, SymbolVisibility};

// Re-export common backend modules so submodules can access them via super::
pub use super::common::lir;
pub use super::common::lir::lower as lir_lower;
pub use super::linker_driver;
