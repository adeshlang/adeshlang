//! VTable System - Phase 0.3
//!
//! Virtual method table implementation for interface dispatch.
//! Enables polymorphic behavior while maintaining performance through caching.
//!
//! This is the foundation for Phase 2 (Interface Implementation):
//! - Interface typing
//! - Virtual dispatch
//! - Downcasting
//! - Fat pointers for interface types
//!
//! Current Status: Foundation laid, ready for Phase 2 integration
//!
//! Architecture:
//! - VTable: Maps interface method signatures to implementation indices
//! - VTableCache: Caches recent lookups to optimize repeated dispatch
//! - InterfaceObject: Fat pointer (data pointer + vtable pointer)

use crate::types::type_info::TypeId;
use std::collections::HashMap;
use std::sync::Arc;

/// A single entry in a virtual method table
#[derive(Debug, Clone)]
pub struct VTableEntry {
    /// Method name
    pub name: String,

    /// Parameter count (arity) for method overloading
    pub arity: usize,

    /// Index into the implementing class's method table
    pub impl_index: usize,

    /// Return type for the method (optional)
    pub return_type: Option<String>,
}

impl VTableEntry {
    pub fn new(name: String, arity: usize, impl_index: usize) -> Self {
        VTableEntry {
            name,
            arity,
            impl_index,
            return_type: None,
        }
    }

    /// Creates a key for this entry (used in HashMap)
    pub fn key(&self) -> String {
        format!("{}_{}", self.name, self.arity)
    }

    pub fn with_return_type(mut self, return_type: String) -> Self {
        self.return_type = Some(return_type);
        self
    }
}

/// Virtual method table for an interface
/// Maps method names to implementation indices
#[derive(Debug, Clone)]
pub struct VTable {
    /// Type ID of the class implementing this vtable
    pub type_id: TypeId,

    /// Type ID of the interface being implemented
    pub interface_id: TypeId,

    /// All method entries (name_arity -> VTableEntry)
    entries: HashMap<String, VTableEntry>,
}

impl VTable {
    /// Creates a new empty vtable
    pub fn new(type_id: TypeId, interface_id: TypeId) -> Self {
        VTable {
            type_id,
            interface_id,
            entries: HashMap::new(),
        }
    }

    /// Adds an entry to the vtable
    pub fn add_entry(&mut self, entry: VTableEntry) {
        self.entries.insert(entry.key(), entry);
    }

    /// Looks up a method entry by name and arity
    pub fn lookup(&self, name: &str, arity: usize) -> Option<&VTableEntry> {
        let key = format!("{}_{}", name, arity);
        self.entries.get(&key)
    }

    /// Gets all entries
    pub fn entries(&self) -> &HashMap<String, VTableEntry> {
        &self.entries
    }

    /// Returns the number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Checks if vtable is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Cache for recent virtual method lookups
/// Reduces repeated HashMap lookups for hot paths
#[derive(Debug, Clone)]
pub struct VTableCache {
    /// Recent lookups (limited size for cache locality)
    cache: HashMap<(TypeId, String, usize), usize>,

    /// Maximum cache size
    max_entries: usize,
}

impl VTableCache {
    pub fn new(max_entries: usize) -> Self {
        VTableCache {
            cache: HashMap::new(),
            max_entries,
        }
    }

    /// Looks up in cache, returns Some(impl_index) if found
    pub fn get(&self, type_id: TypeId, name: &str, arity: usize) -> Option<usize> {
        let key = (type_id, name.to_string(), arity);
        self.cache.get(&key).copied()
    }

    /// Adds an entry to cache
    pub fn insert(&mut self, type_id: TypeId, name: String, arity: usize, impl_index: usize) {
        // Simple cache eviction: clear if at capacity
        if self.cache.len() >= self.max_entries {
            self.cache.clear();
        }
        self.cache.insert((type_id, name, arity), impl_index);
    }

    /// Clears the cache
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Returns cache statistics
    pub fn stats(&self) -> (usize, usize) {
        (self.cache.len(), self.max_entries)
    }
}

impl Default for VTableCache {
    fn default() -> Self {
        VTableCache::new(1024) // Default to 1024 entries
    }
}

/// Global VTable collection for a runtime
/// Maps (type_id, interface_id) -> VTable
pub struct VTableRegistry {
    tables: Arc<std::sync::RwLock<HashMap<(TypeId, TypeId), Arc<VTable>>>>,
}

impl VTableRegistry {
    pub fn new() -> Self {
        VTableRegistry {
            tables: Arc::new(std::sync::RwLock::new(HashMap::new())),
        }
    }

    /// Registers a new vtable
    pub fn register(&self, vtable: VTable) {
        let key = (vtable.type_id, vtable.interface_id);
        let vtable_arc = Arc::new(vtable);
        self.tables.write().unwrap().insert(key, vtable_arc);
    }

    /// Looks up a vtable
    pub fn get(&self, type_id: TypeId, interface_id: TypeId) -> Option<Arc<VTable>> {
        self.tables
            .read()
            .unwrap()
            .get(&(type_id, interface_id))
            .cloned()
    }

    /// Lists all vtables for a type
    pub fn get_all_for_type(&self, type_id: TypeId) -> Vec<Arc<VTable>> {
        self.tables
            .read()
            .unwrap()
            .iter()
            .filter(|((t_id, _), _)| *t_id == type_id)
            .map(|(_, vtable)| Arc::clone(vtable))
            .collect()
    }

    /// Clones the registry for thread-safe sharing
    pub fn clone_registry(&self) -> Self {
        VTableRegistry {
            tables: Arc::clone(&self.tables),
        }
    }
}

impl Clone for VTableRegistry {
    fn clone(&self) -> Self {
        self.clone_registry()
    }
}

impl Default for VTableRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Fat pointer for interface-typed references
/// Allows passing interface-typed parameters to functions
#[derive(Debug, Clone)]
pub struct InterfaceObject {
    /// Pointer to actual data (the implementing instance)
    pub data_ptr: usize,

    /// Pointer to vtable for method dispatch
    pub vtable: Arc<VTable>,

    /// Type ID of actual object (for downcasting)
    pub actual_type_id: TypeId,
}

impl InterfaceObject {
    pub fn new(data_ptr: usize, vtable: Arc<VTable>, actual_type_id: TypeId) -> Self {
        InterfaceObject {
            data_ptr,
            vtable,
            actual_type_id,
        }
    }

    /// Performs a virtual method call
    /// Returns the impl_index for the actual implementation
    pub fn dispatch(&self, method_name: &str, arity: usize) -> Option<usize> {
        self.vtable.lookup(method_name, arity).map(|e| e.impl_index)
    }

    /// Attempts to downcast to a specific type
    pub fn downcast<T>(&self, target_type_id: TypeId) -> Option<&T> {
        if self.actual_type_id == target_type_id {
            unsafe { Some(&*(self.data_ptr as *const T)) }
        } else {
            None
        }
    }

    /// Attempts mutable downcast
    pub fn downcast_mut<T>(&mut self, target_type_id: TypeId) -> Option<&mut T> {
        if self.actual_type_id == target_type_id {
            unsafe { Some(&mut *(self.data_ptr as *mut T)) }
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vtable_entry_key() {
        let entry = VTableEntry::new("log".to_string(), 1, 0);
        assert_eq!(entry.key(), "log_1");
    }

    #[test]
    fn test_vtable_creation() {
        let type_id = TypeId::new(1);
        let interface_id = TypeId::new(2);
        let vtable = VTable::new(type_id, interface_id);

        assert_eq!(vtable.type_id, type_id);
        assert_eq!(vtable.interface_id, interface_id);
        assert!(vtable.is_empty());
    }

    #[test]
    fn test_vtable_add_and_lookup() {
        let type_id = TypeId::new(1);
        let interface_id = TypeId::new(2);
        let mut vtable = VTable::new(type_id, interface_id);

        let entry = VTableEntry::new("log".to_string(), 1, 5);
        vtable.add_entry(entry);

        let found = vtable.lookup("log", 1).unwrap();
        assert_eq!(found.impl_index, 5);
    }

    #[test]
    fn test_vtable_cache() {
        let mut cache = VTableCache::new(10);
        let type_id = TypeId::new(1);

        cache.insert(type_id, "log".to_string(), 1, 5);
        assert_eq!(cache.get(type_id, "log", 1), Some(5));

        // Different arity should miss
        assert_eq!(cache.get(type_id, "log", 2), None);
    }

    #[test]
    fn test_vtable_registry() {
        let registry = VTableRegistry::new();
        let type_id = TypeId::new(1);
        let interface_id = TypeId::new(2);

        let mut vtable = VTable::new(type_id, interface_id);
        vtable.add_entry(VTableEntry::new("test".to_string(), 0, 10));

        registry.register(vtable);

        let retrieved = registry.get(type_id, interface_id).unwrap();
        assert_eq!(retrieved.lookup("test", 0).unwrap().impl_index, 10);
    }

    #[test]
    fn test_vtable_cache_eviction() {
        let mut cache = VTableCache::new(2);
        let type_id = TypeId::new(1);

        cache.insert(type_id, "m1".to_string(), 1, 1);
        cache.insert(type_id, "m2".to_string(), 1, 2);
        assert_eq!(cache.get(type_id, "m1", 1), Some(1));

        // This should trigger eviction
        cache.insert(type_id, "m3".to_string(), 1, 3);
        let (current, max) = cache.stats();
        assert!(current <= max);
    }
}
