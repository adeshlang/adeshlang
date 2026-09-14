//! Fast Collections
//!
//! Type aliases and constructors for `FxHashMap` and `FxHashSet` based maps/sets
//! for speed. Provides common string-keyed helpers used across the crate.
// Fast collection types using FxHash for better performance
// FxHash is 2-3x faster than default SipHash for most workloads

pub use rustc_hash::{FxHashMap as FastMap, FxHashSet as FastSet};

// Type aliases for common patterns
pub type StringMap<V> = FastMap<String, V>;
pub type StringSet = FastSet<String>;

// Pre-configured builders with common capacities
pub fn new_string_map<V>() -> StringMap<V> {
    FastMap::default()
}

pub fn new_string_map_with_capacity<V>(capacity: usize) -> StringMap<V> {
    FastMap::with_capacity_and_hasher(capacity, Default::default())
}

pub fn new_string_set() -> StringSet {
    FastSet::default()
}

pub fn new_string_set_with_capacity(capacity: usize) -> StringSet {
    FastSet::with_capacity_and_hasher(capacity, Default::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fast_map_creation() {
        let mut map = new_string_map::<i32>();
        map.insert("test".to_string(), 42);
        assert_eq!(map.get("test"), Some(&42));
    }

    #[test]
    fn test_fast_set_creation() {
        let mut set = new_string_set();
        set.insert("test".to_string());
        assert!(set.contains("test"));
    }
}
