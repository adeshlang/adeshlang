//! Encoding standard library runtime implementation.

pub mod api;
pub mod base64;
pub mod binary;
pub mod bom;
pub mod hex;
pub mod percent;
pub mod utf;
pub mod varint;

pub use api::{build_encoding_module_object, register_all};
