//! OrderedSet - Set preserving insertion order
//!
//! Set interface preserving element insertion order.

use crate::stdlib::adesh_alloc::ordered_map::OrderedMap;
use std::hash::Hash;

pub struct OrderedSet<T> {
    map: OrderedMap<T, ()>,
}

impl<T: Clone + Eq + Hash> OrderedSet<T> {
    pub fn new() -> Self {
        OrderedSet {
            map: OrderedMap::new(),
        }
    }

    pub fn insert(&mut self, value: T) -> bool {
        if self.map.contains_key(&value) {
            false
        } else {
            self.map.insert(value, ());
            true
        }
    }

    pub fn contains(&self, value: &T) -> bool {
        self.map.contains_key(value)
    }

    pub fn remove(&mut self, value: &T) -> bool {
        self.map.remove(value).is_some()
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }

    pub fn elements(&self) -> &[T] {
        self.map.keys()
    }
}

impl<T: Clone + Eq + Hash> Default for OrderedSet<T> {
    fn default() -> Self {
        Self::new()
    }
}
