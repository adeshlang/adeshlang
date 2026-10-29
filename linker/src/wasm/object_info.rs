//! Type, import and code-layout facts the module writer needs, captured while
//! reading a relocatable WASM object.
//!
//! These cannot be recovered from merged sections: the writer has to re-emit
//! the type section, renumber every function and copy each body exactly once,
//! so it needs signatures and per-body boundaries as the object declared them.

/// A function signature, as raw value-type bytes (`0x7F` i32, `0x7E` i64,
/// `0x7D` f32, `0x7C` f64).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmFuncType {
    pub params: Vec<u8>,
    pub results: Vec<u8>,
}

/// One entry of the import section, with the detail each kind carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WasmImportDesc {
    Function {
        module: String,
        name: String,
        type_index: u32,
    },
    Table {
        module: String,
        name: String,
        min: u32,
    },
    Memory {
        module: String,
        name: String,
        min: u32,
    },
    Global {
        module: String,
        name: String,
        val_type: u8,
        mutable: bool,
    },
    Tag {
        module: String,
        name: String,
        type_index: u32,
    },
}

impl WasmImportDesc {
    pub fn module(&self) -> &str {
        match self {
            WasmImportDesc::Function { module, .. }
            | WasmImportDesc::Table { module, .. }
            | WasmImportDesc::Memory { module, .. }
            | WasmImportDesc::Global { module, .. }
            | WasmImportDesc::Tag { module, .. } => module,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            WasmImportDesc::Function { name, .. }
            | WasmImportDesc::Table { name, .. }
            | WasmImportDesc::Memory { name, .. }
            | WasmImportDesc::Global { name, .. }
            | WasmImportDesc::Tag { name, .. } => name,
        }
    }

    /// Function and tag imports share the type section.
    pub fn type_index(&self) -> Option<u32> {
        match self {
            WasmImportDesc::Function { type_index, .. }
            | WasmImportDesc::Tag { type_index, .. } => Some(*type_index),
            _ => None,
        }
    }
}

/// One active data segment's payload, located within the DATA section payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WasmDataSegmentInfo {
    pub payload_offset: u64,
    pub payload_len: u64,
}

/// Kind of a linkable external an object's export section names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmExportKind {
    Function,
    Table,
    Memory,
    Global,
    Tag,
}

/// One entry of an object's export section. Indices are object-local: a
/// function index counts the object's own imported functions first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmObjectExport {
    pub name: String,
    pub kind: WasmExportKind,
    pub index: u32,
}

/// What a relocatable object declares, beyond its raw section bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WasmObjectInfo {
    pub types: Vec<WasmFuncType>,
    /// Type index of each module-defined function, in code order.
    pub function_types: Vec<u32>,
    /// Imports in index-space order (functions first is *not* implied: wasm
    /// orders by section entry, and each kind has its own index space).
    pub imports: Vec<WasmImportDesc>,
    /// Start of each function body within the CODE payload — after its size
    /// LEB, so it shares a coordinate space with `reloc.CODE` offsets.
    pub body_offsets: Vec<u64>,
    /// End of each function body (exclusive) within the CODE payload.
    pub body_ends: Vec<u64>,
    /// Data segment payloads, in segment index order.
    pub data_segments: Vec<WasmDataSegmentInfo>,
    /// Export section entries, in section order.
    pub exports: Vec<WasmObjectExport>,
}

impl WasmObjectInfo {
    pub fn func_type(&self, type_index: u32) -> Option<&WasmFuncType> {
        self.types.get(type_index as usize)
    }

    /// Names of the imported functions, in function index-space order.
    pub fn imported_function_names(&self) -> Vec<&str> {
        self.imports
            .iter()
            .filter_map(|import| match import {
                WasmImportDesc::Function { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect()
    }
}
