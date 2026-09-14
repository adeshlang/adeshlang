//! HashSet - Set Implementation
//!
//! A hash set based on HashMap.

use crate::stdlib::adesh_alloc::hashmap::HashMap;

/// A hash set with owned keys
pub struct HashSet<T> {
    map: HashMap<T, ()>,
}

impl<T> HashSet<T>
where
    T: Eq + std::hash::Hash,
{
    /// Creates a new empty HashSet
    pub fn new() -> Self {
        HashSet {
            map: HashMap::new(),
        }
    }

    /// Creates a HashSet with the specified capacity
    pub fn with_capacity(capacity: usize) -> Self {
        HashSet {
            map: HashMap::with_capacity(capacity),
        }
    }

    /// Inserts a value into the set. Returns true if the value was not already present.
    pub fn insert(&mut self, value: T) -> bool {
        self.map.insert(value, ()).is_none()
    }

    /// Removes a value from the set. Returns true if the value was present.
    pub fn remove(&mut self, value: &T) -> bool {
        self.map.remove(value).is_some()
    }

    /// Returns true if the set contains the value
    pub fn contains(&self, value: &T) -> bool {
        self.map.contains_key(value)
    }

    /// Returns the number of elements in the set
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns true if the set is empty
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Clears the set
    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// Returns an iterator over the set's elements
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.map.keys()
    }
}

impl<T> Default for HashSet<T>
where
    T: Eq + std::hash::Hash,
{
    fn default() -> Self {
        Self::new()
    }
}
