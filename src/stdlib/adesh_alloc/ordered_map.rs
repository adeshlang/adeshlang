//! OrderedMap - Map preserving insertion order
//!
//! Maintains keys in insertion order alongside fast hash-based lookup.

use crate::stdlib::adesh_alloc::hashmap::HashMap;
use crate::stdlib::adesh_alloc::vec::Vec;
use std::hash::Hash;

pub struct OrderedMap<K, V> {
    keys_order: Vec<K>,
    map: HashMap<K, V>,
}

impl<K: Clone + Eq + Hash, V> OrderedMap<K, V> {
    pub fn new() -> Self {
        OrderedMap {
            keys_order: Vec::new(),
            map: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if !self.map.contains_key(&key) {
            self.keys_order.push(key.clone());
        }
        self.map.insert(key, value)
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.map.get(key)
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.map.get_mut(key)
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let val = self.map.remove(key)?;
        if let Some(pos) = self.keys_order.as_slice().iter().position(|k| k == key) {
            self.keys_order.remove(pos);
        }
        Some(val)
    }

    pub fn len(&self) -> usize {
        self.keys_order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys_order.is_empty()
    }

    pub fn clear(&mut self) {
        self.keys_order.clear();
        self.map.clear();
    }

    pub fn keys(&self) -> &[K] {
        self.keys_order.as_slice()
    }
}

impl<K: Clone + Eq + Hash, V> Default for OrderedMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}
