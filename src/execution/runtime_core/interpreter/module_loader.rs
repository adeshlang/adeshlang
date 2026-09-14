//! Module loader for import/export functionality.
//!
//! Provides a cache-based module loading system for the language's module system.

use crate::parsing::ast::Value;
use rustc_hash::FxHashMap as HashMap;
use std::path::PathBuf;

/// Module loader with caching support.
///
/// Manages module loading and caching for the import system.
pub struct ModuleLoader {
    #[allow(dead_code)]
    pub root: PathBuf,
    pub(in crate::execution::runtime_core) cache: HashMap<String, Value>, // module path -> exported object
}

impl ModuleLoader {
    /// Create a new module loader with the given root directory.
    pub fn new(root: &std::path::Path) -> Self {
        Self {
            root: root.to_path_buf(),
            cache: HashMap::default(),
        }
    }
}
