//! Parsing Pipeline
//!
//! Transforms source text into intermediate representations:
//! - `lexer`: tokenizes input with rich tokens and doc-comments
//! - `parser`: builds `AST` supporting classes, functions, decorators, imports
//! - `ast_optimizer`: folds constants and simplifies expressions/statements
//! - `hir`: typed high-level IR for semantic analyses
//! - `hir_lower`: lowers `AST` → `HIR` before backend consumption
//! - `hir_passes`: HIR analysis and optimization passes (lifetime validation, escape analysis, etc.)
//! - `unified_safety_pass`: SINGLE compile-time memory safety validation point (ALL CHECKS HERE)
//! - `ownership`: ownership and borrow checking system (Rust-like memory safety)
//! - `ownership_enhanced`: CFG-based move tracking and use-after-move detection (Phase 2)
//! - `borrow_check`: compile-time borrow enforcement for memory safety
//! - `borrow_inference`: automatic &T and &mut T inference
//! - `lifetime_tracking`: interprocedural lifetime validation
//! - `interprocedural`: cross-function borrow and lifetime analysis
//! - `closure_capture`: complete closure capture validation (move vs borrow)
//! - `escape_analysis`: complete escape analysis for stack vs heap optimization (Phase 2)
//! - `unsafe_pointer_tracking`: provenance tracking and unsafe block enforcement (Phase 4)

pub mod ast;
pub mod ast_optimizer;
pub mod borrow_check;
pub mod borrow_inference;
pub mod cfg_borrow;
pub mod closure_capture;
pub mod compile_time_memory_safety;
pub mod decorator_compile;
pub mod decorator_pipeline;
pub mod decorator_registry;
pub mod drop_insertion;
pub mod error;
pub mod escape_analysis;
pub mod hir;
pub mod hir_lower;
pub mod hir_passes;
pub mod interprocedural;
pub mod lexer;
pub mod lifetime_tracking;
pub mod ownership;
pub mod ownership_enhanced;
pub mod parser;
pub mod safety_hir_adapter;
pub mod unified_safety_pass;
pub mod unsafe_pointer_tracking;
pub mod unused_warnings;
pub mod variance;
