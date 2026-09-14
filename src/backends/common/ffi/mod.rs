//! FFI Module - Foreign Function Interface
//!
//! Contains FFI generation, import handling, C header parsing,
//! safety validation, and Rust interoperability

pub mod c_header_parser;
pub mod generator;
pub mod import;
pub mod rust_interop;
pub mod safety;

pub use c_header_parser::*;
pub use generator::*;
pub use import::*;
pub use rust_interop::*;
pub use safety::*;
