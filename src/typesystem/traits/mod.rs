//! Trait System for the Language Runtime
//!
//! Implements marker traits for concurrency safety:
//! - Send: Type can be transferred between threads
//! - Sync: References to type can be shared between threads
//!
//! These traits are automatically implemented for most types and are checked
//! at compile-time to prevent data races.

use crate::parsing::hir::{HirClass, HirModule, HirType};
use std::collections::{HashMap, HashSet};

/// Marker trait identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TraitId {
    /// Type can be transferred between threads
    Send,
    /// References to type can be shared between threads
    Sync,
}

/// Trait implementation record
#[derive(Debug, Clone)]
pub struct TraitImpl {
    pub trait_id: TraitId,
    pub type_name: String,
    pub is_auto: bool,   // Auto-implemented vs explicit
    pub is_unsafe: bool, // Requires unsafe impl
}

/// Trait checker for compile-time validation
pub struct TraitChecker {
    /// Trait implementations: type_name -> Set<TraitId>
    impls: HashMap<String, HashSet<TraitId>>,

    /// Negative impls (explicitly NOT implemented): type_name -> Set<TraitId>
    negative_impls: HashMap<String, HashSet<TraitId>>,

    /// Type definitions for analysis
    type_defs: HashMap<String, TypeDef>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct TypeDef {
    name: String,
    fields: Vec<(String, HirType)>,
    is_generic: bool,
}

impl TraitChecker {
    pub fn new() -> Self {
        let mut checker = TraitChecker {
            impls: HashMap::new(),
            negative_impls: HashMap::new(),
            type_defs: HashMap::new(),
        };

        // Auto-implement Send/Sync for primitive types
        checker.auto_impl_primitives();

        checker
    }

    /// Auto-implement Send and Sync for primitive types
    fn auto_impl_primitives(&mut self) {
        let primitives = vec![
            "int", "float", "bool", "char", "string", "u8", "u16", "u32", "u64", "u128", "i8",
            "i16", "i32", "i64", "i128", "f32", "f64",
        ];

        for prim in primitives {
            self.impls
                .entry(prim.to_string())
                .or_insert_with(HashSet::new)
                .insert(TraitId::Send);
            self.impls
                .entry(prim.to_string())
                .or_insert_with(HashSet::new)
                .insert(TraitId::Sync);
        }

        for atomic in [
            "AtomicBool",
            "AtomicI8",
            "AtomicI16",
            "AtomicI32",
            "AtomicI64",
            "AtomicU8",
            "AtomicU16",
            "AtomicU32",
            "AtomicU64",
            "AtomicPtr",
            "Arc",
            "Shared",
            "Mutex",
            "RwLock",
        ] {
            self.impls
                .entry(atomic.to_string())
                .or_insert_with(HashSet::new)
                .insert(TraitId::Send);
            self.impls
                .entry(atomic.to_string())
                .or_insert_with(HashSet::new)
                .insert(TraitId::Sync);
        }

        // Types that are NOT Send/Sync
        // Rc<T> is not Send/Sync (single-threaded only)
        self.negative_impls
            .entry("Rc".to_string())
            .or_insert_with(HashSet::new)
            .insert(TraitId::Send);
        self.negative_impls
            .entry("Rc".to_string())
            .or_insert_with(HashSet::new)
            .insert(TraitId::Sync);
        self.negative_impls
            .entry("RefCell".to_string())
            .or_insert_with(HashSet::new)
            .insert(TraitId::Send);
        self.negative_impls
            .entry("RefCell".to_string())
            .or_insert_with(HashSet::new)
            .insert(TraitId::Sync);

        // Raw pointers are not Send/Sync by default
        self.negative_impls
            .entry("*T".to_string())
            .or_insert_with(HashSet::new)
            .insert(TraitId::Send);
        self.negative_impls
            .entry("*T".to_string())
            .or_insert_with(HashSet::new)
            .insert(TraitId::Sync);
    }

    /// Check if a type implements a trait
    pub fn implements(&self, type_name: &str, trait_id: TraitId) -> bool {
        // Check negative impls first
        if let Some(neg) = self.negative_impls.get(type_name) {
            if neg.contains(&trait_id) {
                return false;
            }
        }

        // Check positive impls
        if let Some(traits) = self.impls.get(type_name) {
            if traits.contains(&trait_id) {
                return true;
            }
        }

        // Auto-derive for compatible types
        self.can_auto_derive(type_name, trait_id)
    }

    /// Check if a type can auto-derive a trait
    fn can_auto_derive(&self, type_name: &str, trait_id: TraitId) -> bool {
        // Look up type definition
        if let Some(type_def) = self.type_defs.get(type_name) {
            // A type implements Send/Sync if all its fields do
            for (_, field_type) in &type_def.fields {
                if !self.type_implements_trait(field_type, trait_id) {
                    return false;
                }
            }
            return true;
        }

        // Unknown types default to false (conservative)
        false
    }

    /// Check if a HirType implements a trait
    pub fn type_implements_trait(&self, hir_type: &HirType, trait_id: TraitId) -> bool {
        match hir_type {
            // Primitives: always Send + Sync
            HirType::Int
            | HirType::Float
            | HirType::Bool
            | HirType::String
            | HirType::Char
            | HirType::U8
            | HirType::U16
            | HirType::U32
            | HirType::U64
            | HirType::U128
            | HirType::I8
            | HirType::I16
            | HirType::I32
            | HirType::I64
            | HirType::I128
            | HirType::F32
            | HirType::F64 => true,

            // Null is Send + Sync
            HirType::Null => true,

            // Arrays/collections: Send/Sync if element type is
            HirType::Array(elem_ty, _) => self.type_implements_trait(elem_ty, trait_id),
            HirType::Dict(k, v) => {
                self.type_implements_trait(k, trait_id) && self.type_implements_trait(v, trait_id)
            }
            HirType::Set(elem_ty) => self.type_implements_trait(elem_ty, trait_id),
            HirType::Tuple(types) => types
                .iter()
                .all(|t| self.type_implements_trait(t, trait_id)),

            // Functions: Send but not Sync (mutable state via closure captures)
            HirType::Function(_, _) => trait_id == TraitId::Send,

            // Borrows: &T is Send if T: Sync, &mut T is Send if T: Send
            HirType::Borrow(inner, is_exclusive) => {
                if *is_exclusive {
                    // &mut T is Send if T is Send
                    trait_id == TraitId::Send && self.type_implements_trait(inner, TraitId::Send)
                } else {
                    // &T is Send if T is Sync
                    trait_id == TraitId::Send && self.type_implements_trait(inner, TraitId::Sync)
                }
            }
            HirType::BorrowImmut(inner) => {
                trait_id == TraitId::Send && self.type_implements_trait(inner, TraitId::Sync)
            }
            HirType::BorrowMut(inner) => {
                trait_id == TraitId::Send && self.type_implements_trait(inner, TraitId::Send)
            }

            // Shared (Arc): Send + Sync if T is Send + Sync
            HirType::Shared(inner) => self.type_implements_trait(inner, trait_id),

            // Weak: Same as Shared
            HirType::Weak(inner) => self.type_implements_trait(inner, trait_id),

            // Class/Instance: check type definition
            HirType::Class(name) | HirType::Instance(name) => self.implements(name, trait_id),

            // Promise: Send if T is Send, not Sync (mutable state)
            HirType::Promise(inner) => {
                trait_id == TraitId::Send && self.type_implements_trait(inner, TraitId::Send)
            }

            // SIMD vectors: Send + Sync if element type is (stack-allocated value type)
            HirType::Simd(elem_ty, _) => self.type_implements_trait(elem_ty, trait_id),

            // Unknown/Any: conservative - assume not Send/Sync
            HirType::Any | HirType::Unknown | HirType::Object => false,
        }
    }

    /// Register a type definition for auto-derivation
    pub fn register_type(&mut self, name: String, fields: Vec<(String, HirType)>) {
        self.type_defs.insert(
            name.clone(),
            TypeDef {
                name: name.clone(),
                fields,
                is_generic: false,
            },
        );

        // Try to auto-derive Send/Sync
        if self.can_auto_derive(&name, TraitId::Send) {
            self.impls
                .entry(name.clone())
                .or_insert_with(HashSet::new)
                .insert(TraitId::Send);
        }
        if self.can_auto_derive(&name, TraitId::Sync) {
            self.impls
                .entry(name.clone())
                .or_insert_with(HashSet::new)
                .insert(TraitId::Sync);
        }
    }

    /// Explicitly implement a trait for a type (used for unsafe impl)
    pub fn explicit_impl(&mut self, type_name: String, trait_id: TraitId) {
        self.impls
            .entry(type_name)
            .or_insert_with(HashSet::new)
            .insert(trait_id);
    }

    /// Analyze a HIR module and register all type definitions
    pub fn analyze_module(&mut self, module: &HirModule) {
        for class in &module.classes {
            self.register_class(class);
        }
    }

    /// Register a class definition
    fn register_class(&mut self, class: &HirClass) {
        let fields = Vec::new();

        // Extract field types from constructor/methods
        // Note: This is a simplified version - full implementation would
        // analyze the class body more thoroughly

        self.register_type(class.name.clone(), fields);
    }

    /// Validate that a value of given type can be sent to another thread
    pub fn validate_send(&self, hir_type: &HirType, context: &str) -> Result<(), String> {
        if !self.type_implements_trait(hir_type, TraitId::Send) {
            let ty = format!("{:?}", hir_type);
            let rc_help = if ty.contains("Rc") {
                "\nhelp: `Rc<T>` cannot be transferred to another thread\n\
                 help: use `Arc<T>` / `Shared<T>` for shared ownership across threads\n\
                 help: use `Mutex<T>` or `RwLock<T>` for synchronized mutation"
            } else if ty.contains("RefCell") {
                "\nhelp: `RefCell<T>` is thread-confined (not Sync)\n\
                 help: use `Mutex<T>` or `RwLock<T>` for synchronized mutation"
            } else {
                "\nhelp: consider `Arc<Mutex<T>>` for shared mutable state, or move owned data"
            };
            return Err(format!(
                "error[E0277]: type does not implement Send\n\
                 = what: attempted to transfer a value to another thread in {}\n\
                 = why: the type is not safe to move across threads (`Send`)\n\
                 = type: {}\n{}",
                context, ty, rc_help
            ));
        }
        Ok(())
    }

    /// Validate that a reference of given type can be shared across threads
    pub fn validate_sync(&self, hir_type: &HirType, context: &str) -> Result<(), String> {
        if !self.type_implements_trait(hir_type, TraitId::Sync) {
            return Err(format!(
                "error[E0277]: type does not implement Sync\n\
                 = note: required for shared references in {}\n\
                 = help: consider using Arc<RwLock<T>> for shared data",
                context
            ));
        }
        Ok(())
    }
}

impl Default for TraitChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primitives_are_send_sync() {
        let checker = TraitChecker::new();

        assert!(checker.implements("int", TraitId::Send));
        assert!(checker.implements("int", TraitId::Sync));
        assert!(checker.implements("string", TraitId::Send));
        assert!(checker.implements("string", TraitId::Sync));
    }

    #[test]
    fn test_rc_not_send_sync() {
        let checker = TraitChecker::new();

        assert!(!checker.implements("Rc", TraitId::Send));
        assert!(!checker.implements("Rc", TraitId::Sync));
    }

    #[test]
    fn test_array_send_if_element_send() {
        let checker = TraitChecker::new();

        let int_array = HirType::Array(
            Box::new(HirType::Int),
            crate::parsing::hir::ArrayKind::Dynamic,
        );
        assert!(checker.type_implements_trait(&int_array, TraitId::Send));
        assert!(checker.type_implements_trait(&int_array, TraitId::Sync));
    }

    #[test]
    fn test_shared_borrow_send_requires_sync() {
        let checker = TraitChecker::new();

        // &int is Send because int is Sync
        let shared_borrow = HirType::Borrow(Box::new(HirType::Int), false);
        assert!(checker.type_implements_trait(&shared_borrow, TraitId::Send));

        // &mut int is Send because int is Send
        let exclusive_borrow = HirType::Borrow(Box::new(HirType::Int), true);
        assert!(checker.type_implements_trait(&exclusive_borrow, TraitId::Send));
    }
}
