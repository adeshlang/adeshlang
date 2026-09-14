//! Inline cache system for fast property and call site caching
//!
//! This module implements inline caches (ICs) that speed up property access
//! and method calls by caching results based on hidden class IDs. When the
//! same hidden class is encountered again, the cached offset can be used directly.

use crate::utils::collections::FastMap;

/// Inline cache entry for property access
#[derive(Debug, Clone)]
pub struct InlineCacheEntry {
    /// Expected hidden class ID
    pub expected_class: u64,
    /// Cached property offset
    pub offset: u32,
    /// Hit count
    pub hits: u64,
}

/// Inline cache system for fast property access
pub struct InlineCacheSystem {
    /// Property access caches: (instruction_addr) -> entry
    property_caches: FastMap<u32, InlineCacheEntry>,
    /// Call site caches: (instruction_addr) -> target function
    call_caches: FastMap<u32, String>,
    /// Statistics
    hits: u64,
    misses: u64,
}

impl InlineCacheSystem {
    pub fn new() -> Self {
        InlineCacheSystem {
            property_caches: FastMap::default(),
            call_caches: FastMap::default(),
            hits: 0,
            misses: 0,
        }
    }

    /// Try to get cached property offset
    pub fn get_property(&mut self, addr: u32, class_id: u64) -> Option<u32> {
        if let Some(entry) = self.property_caches.get_mut(&addr) {
            if entry.expected_class == class_id {
                self.hits += 1;
                entry.hits += 1;
                return Some(entry.offset);
            }
        }
        self.misses += 1;
        None
    }

    /// Cache property access
    pub fn cache_property(&mut self, addr: u32, class_id: u64, offset: u32) {
        self.property_caches.insert(
            addr,
            InlineCacheEntry {
                expected_class: class_id,
                offset,
                hits: 0,
            },
        );
    }

    /// Get cache statistics
    pub fn stats(&self) -> (u64, u64, f64) {
        let total = self.hits + self.misses;
        let hit_rate = if total > 0 {
            self.hits as f64 / total as f64
        } else {
            0.0
        };
        (self.hits, self.misses, hit_rate)
    }
}

impl Default for InlineCacheSystem {
    fn default() -> Self {
        Self::new()
    }
}
