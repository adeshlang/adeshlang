//! HashMap - Hash Table Implementation
//!
//! A hash map using separate chaining for collision resolution.

use std::collections::HashMap as StdHashMap;

/// A hash map with owned keys and values
pub struct HashMap<K, V> {
    inner: StdHashMap<K, V>,
}

impl<K, V> HashMap<K, V>
where
    K: Eq + std::hash::Hash,
{
    /// Creates a new empty HashMap
    pub fn new() -> Self {
        HashMap {
            inner: StdHashMap::new(),
        }
    }

    /// Creates a HashMap with the specified capacity
    pub fn with_capacity(capacity: usize) -> Self {
        HashMap {
            inner: StdHashMap::with_capacity(capacity),
        }
    }

    /// Inserts a key-value pair into the map
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.inner.insert(key, value)
    }

    /// Gets a reference to the value corresponding to the key
    pub fn get(&self, key: &K) -> Option<&V> {
        self.inner.get(key)
    }

    /// Gets a mutable reference to the value corresponding to the key
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.inner.get_mut(key)
    }

    /// Removes a key-value pair from the map
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.inner.remove(key)
    }

    /// Returns true if the map contains the key
    pub fn contains_key(&self, key: &K) -> bool {
        self.inner.contains_key(key)
    }

    /// Returns the number of elements in the map
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns true if the map is empty
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Clears the map, removing all key-value pairs
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Returns an iterator over the map's keys
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.inner.keys()
    }

    /// Returns an iterator over the map's values
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.inner.values()
    }

    /// Returns an iterator over the map's key-value pairs
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.inner.iter()
    }
}

impl<K, V> Default for HashMap<K, V>
where
    K: Eq + std::hash::Hash,
{
    fn default() -> Self {
        Self::new()
    }
}
