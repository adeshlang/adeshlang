//! Phase 10 — Trait / Interface Dispatch & Advanced Devirtualization.
//!
//! Provides:
//! - VTable generation, slot layout, and object representation.
//! - Static devirtualization pass converting indirect interface calls to direct calls
//!   when exact concrete receiver types are statically provable.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Individual method entry inside a VTable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VTableSlot {
    pub method_name: String,
    pub symbol_name: String,
    pub slot_index: usize,
}

/// Complete VTable layout for a concrete type implementing a trait.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VTableLayout {
    pub trait_name: String,
    pub concrete_type: String,
    pub slots: Vec<VTableSlot>,
}

impl VTableLayout {
    pub fn new(trait_name: impl Into<String>, concrete_type: impl Into<String>) -> Self {
        Self {
            trait_name: trait_name.into(),
            concrete_type: concrete_type.into(),
            slots: Vec::new(),
        }
    }

    pub fn add_slot(&mut self, method_name: impl Into<String>, symbol_name: impl Into<String>) {
        let idx = self.slots.len();
        self.slots.push(VTableSlot {
            method_name: method_name.into(),
            symbol_name: symbol_name.into(),
            slot_index: idx,
        });
    }

    pub fn lookup(&self, method_name: &str) -> Option<&VTableSlot> {
        self.slots.iter().find(|s| s.method_name == method_name)
    }
}

/// Static Devirtualizer.
pub struct Devirtualizer {
    known_vtables: HashMap<(String, String), VTableLayout>, // (trait, type) -> layout
}

impl Devirtualizer {
    pub fn new() -> Self {
        Self {
            known_vtables: HashMap::new(),
        }
    }

    pub fn register_vtable(&mut self, layout: VTableLayout) {
        self.known_vtables
            .insert((layout.trait_name.clone(), layout.concrete_type.clone()), layout);
    }

    /// Try devirtualizing a dynamic interface call into a direct function symbol.
    pub fn devirtualize(
        &self,
        trait_name: &str,
        concrete_type: &str,
        method_name: &str,
    ) -> Option<String> {
        self.known_vtables
            .get(&(trait_name.to_string(), concrete_type.to_string()))
            .and_then(|vt| vt.lookup(method_name))
            .map(|s| s.symbol_name.clone())
    }
}

impl Default for Devirtualizer {
    fn default() -> Self {
        Self::new()
    }
}
