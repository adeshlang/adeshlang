//! Target-Independent Debug Metadata and Line Number Mapping.
//!
//! Provides DWARF and CodeView/PDB abstractions for source locations, lexical scopes,
//! variables, and instruction-to-source line mappings.

use serde::{Deserialize, Serialize};

/// Source code location (1-indexed line and column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceLocation {
    pub file_id: u32,
    pub line: u32,
    pub column: u32,
}

impl SourceLocation {
    pub fn new(file_id: u32, line: u32, column: u32) -> Self {
        Self {
            file_id,
            line,
            column,
        }
    }
}

/// A line table mapping an instruction byte offset to a SourceLocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineTableEntry {
    pub code_offset: u32,
    pub location: SourceLocation,
    pub is_statement: bool,
}

/// Debug Variable representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugVariable {
    pub name: String,
    pub type_name: String,
    pub location_offset: i32, // RBP offset or register
}

/// Debug Lexical Scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugScope {
    pub scope_id: u32,
    pub parent_scope_id: Option<u32>,
    pub start_offset: u32,
    pub end_offset: u32,
    pub variables: Vec<DebugVariable>,
}

/// Function-level Debug Metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDebugMetadata {
    pub function_name: String,
    pub source_file: String,
    pub line_table: Vec<LineTableEntry>,
    pub scopes: Vec<DebugScope>,
}

impl FunctionDebugMetadata {
    pub fn new(function_name: impl Into<String>, source_file: impl Into<String>) -> Self {
        Self {
            function_name: function_name.into(),
            source_file: source_file.into(),
            line_table: Vec::new(),
            scopes: Vec::new(),
        }
    }

    pub fn add_line_mapping(&mut self, code_offset: u32, file_id: u32, line: u32, column: u32) {
        self.line_table.push(LineTableEntry {
            code_offset,
            location: SourceLocation::new(file_id, line, column),
            is_statement: true,
        });
    }
}

/// Module-level Debug Information.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModuleDebugInfo {
    pub source_files: Vec<String>,
    pub functions: Vec<FunctionDebugMetadata>,
}
