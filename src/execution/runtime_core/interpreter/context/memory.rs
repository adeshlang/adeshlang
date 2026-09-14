//! Memory and Performance Context
//!
//! Contains memory management and performance tracking state.

use crate::memory::adaptive::AdaptiveMemoryConfig;
use crate::parsing::ast::UserFn;
use rustc_hash::FxHashMap as HashMap;

/// Memory and performance optimization context.
///
/// Manages:
/// - Method caching for performance
/// - Adaptive memory configuration
/// - Other performance-related state
pub struct MemoryContext {
    /// Method cache for faster lookups
    pub(crate) method_cache: HashMap<String, UserFn>,

    /// Adaptive memory configuration for recursion limits
    pub(crate) adaptive_memory: AdaptiveMemoryConfig,
}

impl MemoryContext {
    /// Creates a new memory context with default configuration
    pub fn new() -> Self {
        Self {
            method_cache: HashMap::default(),
            adaptive_memory: AdaptiveMemoryConfig::default(),
        }
    }

    /// Clears the method cache
    #[inline]
    pub fn clear_method_cache(&mut self) {
        self.method_cache.clear();
    }

    /// Gets the adaptive memory config
    #[inline]
    pub fn adaptive_memory(&self) -> &AdaptiveMemoryConfig {
        &self.adaptive_memory
    }

    /// Gets mutable adaptive memory config
    #[inline]
    pub fn adaptive_memory_mut(&mut self) -> &mut AdaptiveMemoryConfig {
        &mut self.adaptive_memory
    }
}

impl Default for MemoryContext {
    fn default() -> Self {
        Self::new()
    }
}
