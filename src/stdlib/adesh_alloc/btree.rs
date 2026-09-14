//! B-Tree Map and Set ordered collections
//!
//! Wrappers around std BTree collections.

use std::collections::BTreeMap as StdBTreeMap;
use std::collections::BTreeSet as StdBTreeSet;

/// An ordered map based on a B-Tree
pub struct BTreeMap<K, V> {
    inner: StdBTreeMap<K, V>,
}

impl<K, V> BTreeMap<K, V>
where
    K: Ord,
{
    /// Creates an empty BTreeMap
    pub fn new() -> Self {
        BTreeMap {
            inner: StdBTreeMap::new(),
        }
    }

    /// Inserts a key-value pair
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.inner.insert(key, value)
    }

    /// Gets a reference to the value associated with the key
    pub fn get(&self, key: &K) -> Option<&V> {
        self.inner.get(key)
    }

    /// Gets a mutable reference to the value associated with the key
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.inner.get_mut(key)
    }

    /// Removes a key
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.inner.remove(key)
    }

    /// Checks if map contains a key
    pub fn contains_key(&self, key: &K) -> bool {
        self.inner.contains_key(key)
    }

    /// Returns the number of elements
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns true if empty
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Clears the map
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Returns an iterator
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.inner.iter()
    }
}

/// An ordered set based on a B-Tree
pub struct BTreeSet<T> {
    inner: StdBTreeSet<T>,
}

impl<T> BTreeSet<T>
where
    T: Ord,
{
    /// Creates an empty BTreeSet
    pub fn new() -> Self {
        BTreeSet {
            inner: StdBTreeSet::new(),
        }
    }

    /// Inserts a value
    pub fn insert(&mut self, value: T) -> bool {
        self.inner.insert(value)
    }

    /// Removes a value
    pub fn remove(&mut self, value: &T) -> bool {
        self.inner.remove(value)
    }

    /// Checks if set contains a value
    pub fn contains(&self, value: &T) -> bool {
        self.inner.contains(value)
    }

    /// Returns the number of elements
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns true if empty
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Clears the set
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Returns an iterator
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.inner.iter()
    }
}
