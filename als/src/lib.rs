//! Adesh Language Server (ALS)
//!
//! A high-performance Language Server Protocol implementation for the Adesh programming language.
//! Uses the shared semantic engine from the compiler crate for type-aware IDE features.
//!
//! ## Features
//! - Type-aware auto-completion (resolves member types, not text matching)
//! - Real-time diagnostics (syntax, semantic, type, ownership errors)
//! - Hover information (resolved types, signatures, docs)
//! - Go-to-definition (semantic resolution via symbol table)
//! - Find references (semantic search, not text matching)
//! - Safe semantic rename
//! - Symbol navigation (document + workspace symbols)
//! - Semantic syntax highlighting (semantic tokens)
//! - Inlay hints (inferred types, parameter names)
//! - Signature help (from resolved function types)
//! - Code formatting
//! - Code actions and quick fixes

pub mod adl;
mod analysis;
mod completion;
mod diagnostics;
mod document;
mod formatting;
mod hover;
pub mod inlay_hints;
pub mod semantic_tokens;
mod server;
mod symbols;
mod workspace;

pub use document::Document;
pub use server::AdeshLanguageServer;
pub use workspace::WorkspaceIndex;
