//! Stack Maps and Runtime Safepoints Infrastructure.
//!
//! Provides abstract stack map tables recording live references, GC safepoints,
//! and stack slot locations at safepoint call sites.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LiveLocationKind {
    Register(u8),
    StackSlot(i32),
}

/// A recorded live variable or object pointer at a safepoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveLocationRecord {
    pub location: LiveLocationKind,
    pub is_pointer: bool,
}

/// A single safepoint in a function body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafepointRecord {
    pub code_offset: u32,
    pub live_locations: Vec<LiveLocationRecord>,
}

/// Function-level Stack Map.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionStackMap {
    pub function_name: String,
    pub safepoints: Vec<SafepointRecord>,
}

impl FunctionStackMap {
    pub fn new(function_name: impl Into<String>) -> Self {
        Self {
            function_name: function_name.into(),
            safepoints: Vec::new(),
        }
    }

    pub fn add_safepoint(&mut self, code_offset: u32, locations: Vec<LiveLocationRecord>) {
        self.safepoints.push(SafepointRecord {
            code_offset,
            live_locations: locations,
        });
    }
}
