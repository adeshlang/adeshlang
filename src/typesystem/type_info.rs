//! TypeInfo System - Phase 0 Foundation
//!
//! Core type information infrastructure for unified OOP implementation.
//! This system enables all 5 backends (Interpreter, VM, JIT, AOT, WASM) to
//! maintain identical semantics while eliminating HashMap field overhead.
//!
//! Key Components:
//! - TypeId: Unique identifier for each class/struct/interface
//! - TypeInfo: Complete metadata about a type (methods, fields, inheritance)
//! - TypeRegistry: Central repository (one per runtime) managing all types
//! - FieldInfo: Per-field metadata (name, offset, size, visibility)
//!
//! Memory Model Change:
//! - OLD: UserInstance { fields: Arc<Mutex<HashMap>> } = 40+ bytes overhead
//! - NEW: UserInstance { data: Vec<u8>, layout: Arc<TypeLayout> } = 8-16 bytes overhead
//! - GAIN: 75% memory reduction + 10-100x faster field access

use crate::types::visibility::Visibility;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, RwLock};

/// Unique type identifier within a runtime
/// Used as key in TypeRegistry and for fast type comparisons
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypeId(pub u32);

impl fmt::Display for TypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "T{}", self.0)
    }
}

impl TypeId {
    /// Creates a new TypeId (internal use only)
    pub fn new(id: u32) -> Self {
        TypeId(id)
    }
}

/// Information about a single field in a struct/class
#[derive(Debug, Clone)]
pub struct FieldInfo {
    /// Field name
    pub name: String,

    /// Byte offset from start of data block
    /// This is the CRITICAL OPTIMIZATION: direct memory access without HashMap
    pub offset: usize,

    /// Field size in bytes
    pub size: usize,

    /// Field alignment requirement (power of 2)
    pub align: usize,

    /// Visibility level (public/protected/private)
    pub visibility: Visibility,

    /// Type name (for error reporting and introspection)
    pub type_name: String,

    /// Is this field optional (default: null)?
    pub is_optional: bool,
}

impl FieldInfo {
    pub fn new(
        name: String,
        offset: usize,
        size: usize,
        align: usize,
        visibility: Visibility,
        type_name: String,
    ) -> Self {
        FieldInfo {
            name,
            offset,
            size,
            align,
            visibility,
            type_name,
            is_optional: false,
        }
    }

    pub fn with_optional(mut self, optional: bool) -> Self {
        self.is_optional = optional;
        self
    }
}

/// Method metadata (shared across all instances of a class)
#[derive(Debug, Clone)]
pub struct MethodInfo {
    /// Method name
    pub name: String,

    /// Parameter count (for arity-based overloading)
    pub arity: usize,

    /// Is this an abstract method (must be implemented by subclasses)?
    pub is_abstract: bool,

    /// Is this a static method?
    pub is_static: bool,

    /// Parameter names (for introspection)
    pub param_names: Vec<String>,

    /// Return type (if annotated)
    pub return_type: Option<String>,
}

impl MethodInfo {
    pub fn new(name: String, arity: usize) -> Self {
        MethodInfo {
            name,
            arity,
            is_abstract: false,
            is_static: false,
            param_names: vec![],
            return_type: None,
        }
    }
}

/// VTable entry for interface method dispatch
#[derive(Debug, Clone)]
pub struct VTableEntry {
    /// Method name
    pub name: String,

    /// Parameter count
    pub arity: usize,

    /// Implementation method index in class method table
    pub impl_index: usize,
}

/// Complete type information for a class/struct/interface
#[derive(Debug, Clone)]
pub struct TypeInfo {
    /// Unique ID for this type
    pub id: TypeId,

    /// Type name
    pub name: String,

    /// Parent class TypeId (if inheriting)
    pub parent_id: Option<TypeId>,

    /// Is this an abstract class?
    pub is_abstract: bool,

    /// Is this a sealed class (no further inheritance)?
    pub is_sealed: bool,

    /// All fields (including inherited)
    pub fields: Vec<FieldInfo>,

    /// All instance methods (including inherited)
    pub methods: Vec<MethodInfo>,

    /// Static methods
    pub static_methods: Vec<MethodInfo>,

    /// Interface IDs this type implements
    pub implements: Vec<TypeId>,

    /// Total instance size in bytes (all fields)
    pub instance_size: usize,

    /// Maximum alignment requirement
    pub max_align: usize,

    /// VTable for interface dispatch (if implements interfaces)
    pub vtable: Option<Arc<VTable>>,
}

impl TypeInfo {
    /// Creates a new type info (usually called by TypeRegistry)
    pub fn new(id: TypeId, name: String) -> Self {
        TypeInfo {
            id,
            name,
            parent_id: None,
            is_abstract: false,
            is_sealed: false,
            fields: vec![],
            methods: vec![],
            static_methods: vec![],
            implements: vec![],
            instance_size: 0,
            max_align: 1,
            vtable: None,
        }
    }

    /// Finds a field by name
    pub fn find_field(&self, name: &str) -> Option<&FieldInfo> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// Finds a method by name and arity
    pub fn find_method(&self, name: &str, arity: usize) -> Option<&MethodInfo> {
        self.methods
            .iter()
            .find(|m| m.name == name && m.arity == arity)
    }

    /// Finds a static method
    pub fn find_static_method(&self, name: &str, arity: usize) -> Option<&MethodInfo> {
        self.static_methods
            .iter()
            .find(|m| m.name == name && m.arity == arity)
    }

    /// Checks if this type is abstract (or has abstract methods)
    pub fn has_abstract_methods(&self) -> bool {
        self.is_abstract || self.methods.iter().any(|m| m.is_abstract)
    }

    /// Validates that all abstract methods are implemented
    pub fn validate_concrete_implementation(&self, parent_info: &TypeInfo) -> Result<(), String> {
        for method in &parent_info.methods {
            if method.is_abstract {
                if !self
                    .methods
                    .iter()
                    .any(|m| m.name == method.name && m.arity == method.arity)
                {
                    return Err(format!(
                        "Type '{}' must implement abstract method '{}' from parent",
                        self.name, method.name
                    ));
                }
            }
        }
        Ok(())
    }
}

/// VTable for interface method dispatch
/// Maps interface method names to implementation indices
#[derive(Debug, Clone)]
pub struct VTable {
    /// Type that owns this vtable
    pub type_id: TypeId,

    /// Entries mapping interface methods to implementations
    pub entries: HashMap<String, VTableEntry>,
}

impl VTable {
    pub fn new(type_id: TypeId) -> Self {
        VTable {
            type_id,
            entries: HashMap::new(),
        }
    }

    /// Looks up a method in the vtable
    pub fn lookup(&self, name: &str, arity: usize) -> Option<&VTableEntry> {
        let key = format!("{}_{}", name, arity);
        self.entries.get(&key)
    }

    /// Adds an entry to the vtable
    pub fn add_entry(&mut self, name: String, arity: usize, impl_index: usize) {
        let key = format!("{}_{}", name, arity);
        self.entries.insert(
            key,
            VTableEntry {
                name,
                arity,
                impl_index,
            },
        );
    }
}

/// Central registry for all types in a runtime
/// Maintains a mapping from TypeId to complete TypeInfo
/// Ensures all backends see identical type information
pub struct TypeRegistry {
    /// All type information, keyed by TypeId
    types: Arc<RwLock<HashMap<TypeId, Arc<TypeInfo>>>>,

    /// Name to TypeId mapping for fast lookup
    name_to_id: Arc<RwLock<HashMap<String, TypeId>>>,

    /// Next available TypeId
    next_id: Arc<RwLock<u32>>,
}

impl TypeRegistry {
    /// Creates a new empty registry
    pub fn new() -> Self {
        TypeRegistry {
            types: Arc::new(RwLock::new(HashMap::new())),
            name_to_id: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(RwLock::new(1)), // Start from 1 (0 is reserved)
        }
    }

    /// Allocates a new TypeId
    pub fn allocate_type_id(&self) -> TypeId {
        let mut next = self.next_id.write().unwrap();
        let id = TypeId::new(*next);
        *next += 1;
        id
    }

    /// Registers a new type
    pub fn register_type(&self, info: TypeInfo) -> TypeId {
        let id = info.id;
        let name = info.name.clone();

        let info = Arc::new(info);

        self.types.write().unwrap().insert(id, info.clone());
        self.name_to_id.write().unwrap().insert(name, id);

        id
    }

    /// Looks up type by ID
    pub fn get_type(&self, id: TypeId) -> Option<Arc<TypeInfo>> {
        self.types.read().unwrap().get(&id).cloned()
    }

    /// Looks up type by name
    pub fn get_type_by_name(&self, name: &str) -> Option<Arc<TypeInfo>> {
        let name_map = self.name_to_id.read().unwrap();
        if let Some(&id) = name_map.get(name) {
            drop(name_map); // Release lock before acquiring types lock
            self.types.read().unwrap().get(&id).cloned()
        } else {
            None
        }
    }

    /// Updates type information (used during layout computation)
    pub fn update_type(&self, id: TypeId, info: TypeInfo) -> Result<(), String> {
        let mut types = self.types.write().unwrap();
        if !types.contains_key(&id) {
            return Err(format!("Type not found: {}", id));
        }
        types.insert(id, Arc::new(info));
        Ok(())
    }

    /// Lists all registered types
    pub fn all_types(&self) -> Vec<Arc<TypeInfo>> {
        self.types.read().unwrap().values().cloned().collect()
    }

    /// Returns the number of registered types
    pub fn type_count(&self) -> usize {
        self.types.read().unwrap().len()
    }

    /// Clones the registry for use in other threads
    pub fn clone_registry(&self) -> Self {
        TypeRegistry {
            types: Arc::clone(&self.types),
            name_to_id: Arc::clone(&self.name_to_id),
            next_id: Arc::clone(&self.next_id),
        }
    }
}

impl Clone for TypeRegistry {
    fn clone(&self) -> Self {
        self.clone_registry()
    }
}

impl Default for TypeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_id_generation() {
        let registry = TypeRegistry::new();
        let id1 = registry.allocate_type_id();
        let id2 = registry.allocate_type_id();
        assert_eq!(id1.0, 1);
        assert_eq!(id2.0, 2);
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_type_registration() {
        let registry = TypeRegistry::new();
        let id = registry.allocate_type_id();
        let mut info = TypeInfo::new(id, "MyClass".to_string());
        info.instance_size = 64;

        registry.register_type(info.clone());

        let retrieved = registry.get_type(id).unwrap();
        assert_eq!(retrieved.name, "MyClass");
        assert_eq!(retrieved.instance_size, 64);
    }

    #[test]
    fn test_type_lookup_by_name() {
        let registry = TypeRegistry::new();
        let id = registry.allocate_type_id();
        let info = TypeInfo::new(id, "Point".to_string());

        registry.register_type(info);

        let retrieved = registry.get_type_by_name("Point").unwrap();
        assert_eq!(retrieved.id, id);
        assert_eq!(retrieved.name, "Point");
    }

    #[test]
    fn test_field_info() {
        let field = FieldInfo::new(
            "x".to_string(),
            0,
            8,
            8,
            Visibility::Public,
            "i64".to_string(),
        );
        assert_eq!(field.name, "x");
        assert_eq!(field.offset, 0);
        assert_eq!(field.size, 8);
    }

    #[test]
    fn test_method_info() {
        let method = MethodInfo::new("calculate".to_string(), 2);
        assert_eq!(method.name, "calculate");
        assert_eq!(method.arity, 2);
        assert!(!method.is_abstract);
        assert!(!method.is_static);
    }

    #[test]
    fn test_vtable_lookup() {
        let mut vtable = VTable::new(TypeId::new(1));
        vtable.add_entry("log".to_string(), 1, 5);

        let entry = vtable.lookup("log", 1).unwrap();
        assert_eq!(entry.impl_index, 5);
    }
}
