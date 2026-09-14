//! Semantics Module - Ownership, borrow checking, lifetimes, and decorators
//!
//! This module provides a clean interface to semantic analysis components.
//! The actual implementations are in the parsing module until full migration is complete.

// Semantic engine for IDE/LSP use
pub mod engine;

// Re-export key types from the engine
pub use engine::{
    CompletionCandidate, ImportInfo, ImportKind, ResolvedSymbol, ScopeInfo, ScopeKind,
    SemanticIndex, SemanticSymbolKind, SymbolEntry, TypeDefinition, TypeMember, VisibilityKind,
    index_source, index_source_in,
};

// Re-export semantic analysis components from parsing module
pub use crate::parsing::borrow_check;
pub use crate::parsing::borrow_inference;
pub use crate::parsing::closure_capture;
pub use crate::parsing::compile_time_memory_safety;
pub use crate::parsing::decorator_compile;
pub use crate::parsing::decorator_pipeline;
pub use crate::parsing::decorator_registry;
pub use crate::parsing::drop_insertion;
pub use crate::parsing::escape_analysis;
pub use crate::parsing::interprocedural;
pub use crate::parsing::lifetime_tracking;
pub use crate::parsing::ownership;
pub use crate::parsing::ownership_enhanced;
pub use crate::parsing::safety_hir_adapter;
pub use crate::parsing::unified_safety_pass;
pub use crate::parsing::unsafe_pointer_tracking;
pub use crate::parsing::variance;

// Re-export commonly used types
pub use crate::parsing::borrow_check::*;
pub use crate::parsing::ownership::*;
