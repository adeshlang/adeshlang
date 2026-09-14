//! WASM Backend Module
//!
//! WebAssembly compilation backend

#![allow(ambiguous_glob_reexports)]

pub mod linker;
pub mod old;

pub use linker::*;

// Also re-export the wasm subdirectory modules
pub use super::wasm::*;
