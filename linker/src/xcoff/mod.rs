//! IBM AIX XCOFF subsystem (loadxcoff).

pub mod header;
pub mod reader;

pub use reader::{XcoffReader, XcoffWriter};
