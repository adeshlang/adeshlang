//! Decorator Runtime Registry
//!
//! Stores compiled decorator pipelines for runtime execution across all backends.
//! This registry allows VM, JIT, AOT, and WASM backends to access decorator
//! pipeline metadata for optimized execution.

use crate::parsing::decorator_pipeline::CompiledPipeline;
use rustc_hash::FxHashMap as HashMap;

/// Registry of compiled decorator pipelines
/// This can be used by backends for consistent decorator execution
pub struct DecoratorRegistry {
    pipelines: HashMap<u64, CompiledPipeline>,
}

impl DecoratorRegistry {
    /// Create a new decorator registry
    pub fn new() -> Self {
        Self {
            pipelines: HashMap::default(),
        }
    }

    /// Register a compiled pipeline
    pub fn register(&mut self, pipeline: CompiledPipeline) -> u64 {
        let hash = pipeline.pipeline_hash;
        self.pipelines.insert(hash, pipeline);
        hash
    }

    /// Get a pipeline by hash
    pub fn get(&self, hash: u64) -> Option<&CompiledPipeline> {
        self.pipelines.get(&hash)
    }

    /// Check if a pipeline exists
    pub fn contains(&self, hash: u64) -> bool {
        self.pipelines.contains_key(&hash)
    }

    /// Get the number of registered pipelines
    pub fn len(&self) -> usize {
        self.pipelines.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.pipelines.is_empty()
    }

    /// Clear all registered pipelines
    pub fn clear(&mut self) {
        self.pipelines.clear();
    }
}

impl Default for DecoratorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::decorator_pipeline::CompiledPipeline;

    #[test]
    fn test_registry_operations() {
        let mut registry = DecoratorRegistry::new();

        let pipeline = CompiledPipeline {
            fn_id: "test".to_string(),
            stages: vec![],
            pipeline_hash: 12345,
            has_compile_phases: false,
            has_runtime_phases: true,
        };

        let hash = registry.register(pipeline.clone());
        assert_eq!(hash, 12345);
        assert!(registry.contains(hash));
        assert_eq!(registry.len(), 1);

        let retrieved = registry.get(hash);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().fn_id, "test");

        registry.clear();
        assert_eq!(registry.len(), 0);
    }
}
