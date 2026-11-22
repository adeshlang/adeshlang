//! Adesh Language Server (ALS)
//!
//! A high-performance Language Server Protocol implementation for the Adesh programming language.
//! Uses the shared semantic engine from the compiler crate for type-aware IDE features.
//!
//! ## Features
//! - Type-aware auto-completion (chained member resolution, workspace auto-imports, postfix templates)
//! - Real-time diagnostics (syntax, semantic, type, arity, interface conformance, ownership, strict mode)
//! - Rich hover information (resolved & inferred types, members, doc comments, stdlib docs)
//! - Go-to-definition, Go-to-type-definition, Go-to-implementation (cross-file & member-aware)
//! - Scope-aware Find references, Document highlights (READ/WRITE), Safe semantic rename
//! - Call hierarchy & Type hierarchy
//! - Code lens (`▶ Run`, `▶ Run Test`, reference & implementation counts)
//! - Code actions (quick fixes, workspace/stdlib auto-imports, interface implementation, refactorings, organize imports)
//! - Symbol navigation (nested document symbols + fuzzy-ranked workspace symbols)
//! - Semantic syntax highlighting (sorted, panic-free UTF-16 delta tokens + ARC/ownership modifiers)
//! - Rich inlay hints (clickable inferred types, parameter names, return types, ownership/ARC hints)
//! - Context-aware signature help (user functions, constructors, methods, builtins, stdlib)
//! - Accurate folding ranges, smart selection ranges, document links, linked editing ranges
//! - Configurable code formatting

pub mod adl;
mod analysis;
pub mod code_actions;
pub mod code_lens;
mod completion;
pub mod config;
mod diagnostics;
mod document;
mod formatting;
pub mod hierarchy;
mod hover;
pub mod inlay_hints;
pub mod semantic_tokens;
mod server;
pub mod signature_help;
mod symbols;
pub mod type_utils;
mod workspace;

pub use config::AlsConfig;
pub use document::Document;
pub use server::AdeshLanguageServer;
pub use workspace::WorkspaceIndex;
