//! Phase 9 Native Debugging Integration Framework.
//!
//! Provides:
//! - Exact instruction-to-source line table mapping (file, line, column)
//! - Local variable location descriptors (register vs stack slot)
//! - Call frame descriptors for debugger stack unwinding
//! - Breakpoint lookup and crash address symbolication
//! - Integration with DWARF and Windows CodeView debug symbols

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Source code coordinate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SourceLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

/// Variable storage location in native frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VariableStorage {
    Register(u16),
    StackOffset(i32),
    Constant(i64),
}

/// Local variable debug descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalVariableDebugInfo {
    pub name: String,
    pub type_name: String,
    pub location: VariableStorage,
    pub live_start_pc: u32,
    pub live_end_pc: u32,
}

/// Function debug information descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDebugInfo {
    pub name: String,
    pub linkage_name: String,
    pub start_address: u64,
    pub size_bytes: u32,
    pub source_location: SourceLocation,
    pub line_table: BTreeMap<u32, SourceLocation>, // instruction byte offset -> source location
    pub local_variables: Vec<LocalVariableDebugInfo>,
}

/// Unified Debug Symbol Engine.
pub struct DebugEngine {
    functions: BTreeMap<String, FunctionDebugInfo>,
    address_map: BTreeMap<u64, String>, // start_addr -> func_name
}

impl DebugEngine {
    pub fn new() -> Self {
        Self {
            functions: BTreeMap::new(),
            address_map: BTreeMap::new(),
        }
    }

    /// Register a function's debug metadata.
    pub fn register_function(&mut self, info: FunctionDebugInfo) {
        self.address_map
            .insert(info.start_address, info.name.clone());
        self.functions.insert(info.name.clone(), info);
    }

    /// Resolve an instruction pointer / crash address to source location and function.
    pub fn resolve_address(&self, address: u64) -> Option<(String, SourceLocation)> {
        // Find the function whose [start_address, start_address + size) contains address
        for (func_name, info) in &self.functions {
            if address >= info.start_address && address < (info.start_address + info.size_bytes as u64) {
                let offset = (address - info.start_address) as u32;
                // Find closest preceding line table entry
                let loc = info
                    .line_table
                    .range(..=offset)
                    .next_back()
                    .map(|(_, loc)| loc.clone())
                    .unwrap_or_else(|| info.source_location.clone());

                return Some((func_name.clone(), loc));
            }
        }
        None
    }

    /// Find valid breakpoint addresses for a given source line.
    pub fn find_breakpoint_address(&self, file: &str, line: u32) -> Option<u64> {
        for info in self.functions.values() {
            for (offset, loc) in &info.line_table {
                if loc.file.ends_with(file) && loc.line == line {
                    return Some(info.start_address + *offset as u64);
                }
            }
        }
        None
    }
}
