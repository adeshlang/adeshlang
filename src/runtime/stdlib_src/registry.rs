//! Builtin Registry
//!
//! Central registry for built-in functions:
//! - Stores function callbacks, categories and descriptions
//! - Allows lookup, listing by category, and metadata access
//! - Used by stdlib modules to register names like `Math.random`, `json_parse`
use crate::parsing::ast::{BuiltinEnv, Value};
use crate::utils::collections::FastMap;
use std::sync::Arc;

pub type BuiltinFn =
    Arc<dyn Fn(&mut dyn BuiltinEnv, Vec<Value>) -> Result<Value, String> + Send + Sync + 'static>;

#[derive(Clone)]
pub struct BuiltinRegistry {
    functions: FastMap<String, BuiltinFn>,
    categories: FastMap<String, Vec<String>>,
    descriptions: FastMap<String, String>,
}

impl BuiltinRegistry {
    pub fn new() -> Self {
        Self {
            functions: FastMap::default(),
            categories: FastMap::default(),
            descriptions: FastMap::default(),
        }
    }
    pub fn register<F>(&mut self, name: &str, category: &str, description: &str, func: F)
    where
        F: Fn(&mut dyn BuiltinEnv, Vec<Value>) -> Result<Value, String> + Send + Sync + 'static,
    {
        self.functions.insert(name.to_string(), Arc::new(func));
        self.categories
            .entry(category.to_string())
            .or_insert_with(Vec::new)
            .push(name.to_string());
        self.descriptions
            .insert(name.to_string(), description.to_string());
    }
    pub fn get(&self, name: &str) -> Option<BuiltinFn> {
        self.functions.get(name).cloned()
    }
    pub fn list_category(&self, category: &str) -> Vec<&str> {
        self.categories
            .get(category)
            .map(|v| v.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }
    pub fn all_names(&self) -> Vec<&str> {
        self.functions.keys().map(|s| s.as_str()).collect()
    }
    pub fn description(&self, name: &str) -> Option<&str> {
        self.descriptions.get(name).map(|s| s.as_str())
    }
}

impl Default for BuiltinRegistry {
    fn default() -> Self {
        Self::new()
    }
}
