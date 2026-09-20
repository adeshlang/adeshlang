//! Hidden class system for fast property access
//!
//! This module implements a hidden class (shape) system similar to V8's Maps
//! or SpiderMonkey's Shapes. Objects with the same property layout share a
//! hidden class, enabling fast property access through constant offsets.

use crate::utils::collections::FastMap;

/// Hidden class descriptor for fast property access
#[derive(Debug, Clone)]
pub struct HiddenClass {
    /// Unique ID
    pub id: u64,
    /// Property names in order
    pub properties: Vec<String>,
    /// Property offsets (index in object's property array)
    pub offsets: FastMap<String, u32>,
    /// Transitions to other hidden classes when property is added
    pub transitions: FastMap<String, u64>,
    /// Parent class (for inheritance)
    pub parent: Option<u64>,
}

impl HiddenClass {
    pub fn new(id: u64) -> Self {
        HiddenClass {
            id,
            properties: Vec::new(),
            offsets: FastMap::default(),
            transitions: FastMap::default(),
            parent: None,
        }
    }

    /// Get property offset
    pub fn get_offset(&self, name: &str) -> Option<u32> {
        self.offsets.get(name).copied()
    }

    /// Add property and return new hidden class ID (if transition exists)
    pub fn add_property(&mut self, name: &str) -> u32 {
        let offset = self.properties.len() as u32;
        self.properties.push(name.to_string());
        self.offsets.insert(name.to_string(), offset);
        offset
    }
}

/// Hidden class system for managing object shapes
pub struct HiddenClassSystem {
    /// All hidden classes
    classes: FastMap<u64, HiddenClass>,
    /// Next class ID
    next_id: u64,
    /// Root class (empty object)
    root_id: u64,
}

impl HiddenClassSystem {
    pub fn new() -> Self {
        let mut system = HiddenClassSystem {
            classes: FastMap::default(),
            next_id: 1,
            root_id: 0,
        };

        // Create root hidden class (empty object)
        let root = HiddenClass::new(0);
        system.classes.insert(0, root);

        system
    }

    /// Get or create a class with additional property
    pub fn add_property(&mut self, class_id: u64, property: &str) -> u64 {
        // Check if transition exists
        if let Some(class) = self.classes.get(&class_id) {
            if let Some(&new_id) = class.transitions.get(property) {
                return new_id;
            }
        }

        // Create new class
        let new_id = self.next_id;
        self.next_id += 1;

        let mut new_class = if let Some(parent) = self.classes.get(&class_id) {
            let mut new_class = parent.clone();
            new_class.id = new_id;
            new_class.parent = Some(class_id);
            new_class.transitions.clear();
            new_class
        } else {
            HiddenClass::new(new_id)
        };

        new_class.add_property(property);

        // Record transition
        if let Some(parent) = self.classes.get_mut(&class_id) {
            parent.transitions.insert(property.to_string(), new_id);
        }

        self.classes.insert(new_id, new_class);
        new_id
    }

    /// Get a hidden class by ID
    pub fn get_class(&self, id: u64) -> Option<&HiddenClass> {
        self.classes.get(&id)
    }

    /// Get root class ID
    pub fn root_id(&self) -> u64 {
        self.root_id
    }
}

impl Default for HiddenClassSystem {
    fn default() -> Self {
        Self::new()
    }
}
