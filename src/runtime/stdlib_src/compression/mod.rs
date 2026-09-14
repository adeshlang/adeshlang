//! AdeshLang Compression Standard Library Submodule.

pub mod adaptive;
pub mod api;
pub mod archives;
pub mod checksums;
pub mod chunking;
pub mod codecs;
pub mod dictionary;
pub mod parallel;
pub mod streaming;

pub use api::{build_compression_module_object, register_all};
