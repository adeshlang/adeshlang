//! Formal Adesh Binary ABI Specification (v1.0).
//!
//! Defines the standard binary contracts between the Adesh compiler frontend,
//! code generation, runtime, and the `adeshlink` linker:
//!
//! 1. Object Metadata (`.adesh.meta` v1)
//! 2. Symbol Mangling Standard (`_A...`)
//! 3. Scope-Aware Instruction-Range RAII Drop & Unwind Action Tables
//! 4. Calling Conventions & Register Allocations
//! 5. Static Initialization & Destructor Arrays (.init_array / .fini_array)

pub mod ffi;
pub mod mangle;

pub use ffi::{
    ADESH_RUNTIME_ABI_VERSION, ADESH_RUNTIME_CONTRACTS, CFieldLayout, CStructLayout,
    CallingConvention, FfiPanicPolicy,
};

use std::collections::BTreeMap;

pub const ADESH_ABI_VERSION: u32 = 1;
pub const ADESH_ABI_MAGIC: [u8; 8] = *b"\x7fADESH\x01\x00";

/// Adesh Object ABI Header embedded in `.adesh.meta`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdeshAbiHeader {
    pub magic: [u8; 8],
    pub abi_version: u32,
    pub compiler_version_hash: u32,
    pub target_arch: u16,
    pub target_os: u16,
    pub pointer_width: u8,
    pub endianness: u8,
    pub memory_model_flags: u16, // Bit 0: Ownership/RAII (1 = enabled, GC = 0)
    pub feature_flags: u32,
}

impl Default for AdeshAbiHeader {
    fn default() -> Self {
        Self {
            magic: ADESH_ABI_MAGIC,
            abi_version: ADESH_ABI_VERSION,
            compiler_version_hash: 0x2026_0901,
            target_arch: 1, // x86_64 default
            target_os: 1,   // Linux default
            pointer_width: 64,
            endianness: 1,              // Little-endian
            memory_model_flags: 0x0001, // Ownership & Borrowing RAII (GC-Free)
            feature_flags: 0,
        }
    }
}

/// Verify ABI compatibility between two object headers prior to linking (Section 45).
pub fn verify_abi_compatibility(
    header_a: &AdeshAbiHeader,
    header_b: &AdeshAbiHeader,
) -> Result<(), crate::error::LinkError> {
    if header_a.abi_version != header_b.abi_version {
        return Err(crate::error::LinkError::new(
            crate::error::ErrorCode::AbiMismatch,
            format!(
                "ABI version mismatch: object A uses v{}, object B uses v{}",
                header_a.abi_version, header_b.abi_version
            ),
        ));
    }

    if header_a.target_arch != header_b.target_arch {
        return Err(crate::error::LinkError::new(
            crate::error::ErrorCode::ArchitectureMismatch,
            format!(
                "Target architecture mismatch: object A arch {}, object B arch {}",
                header_a.target_arch, header_b.target_arch
            ),
        ));
    }

    if header_a.pointer_width != header_b.pointer_width {
        return Err(crate::error::LinkError::new(
            crate::error::ErrorCode::AbiMismatch,
            format!(
                "Pointer width mismatch: object A has {} bits, object B has {} bits",
                header_a.pointer_width, header_b.pointer_width
            ),
        ));
    }

    if header_a.endianness != header_b.endianness {
        return Err(crate::error::LinkError::new(
            crate::error::ErrorCode::AbiMismatch,
            format!(
                "Endianness mismatch: object A has endian {}, object B has endian {}",
                header_a.endianness, header_b.endianness
            ),
        ));
    }

    Ok(())
}

/// Scope-Aware Instruction-Range RAII Cleanup Record.
///
/// Maps a specific instruction interval `[start_pc, end_pc)` within a function
/// to the precise sequence of drop functions that must be executed in LIFO order
/// if a panic or unwinding event occurs while the program counter is inside that range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeUnwindAction {
    /// Start offset in bytes relative to function entry point.
    pub start_offset: u32,
    /// End offset in bytes relative to function entry point.
    pub end_offset: u32,
    /// Cleanup state ID.
    pub cleanup_state_id: u32,
    /// List of drop function virtual addresses / symbol names in LIFO order.
    pub drop_targets: Vec<String>,
}

/// Function-level Unwind and Drop Action Descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionUnwindDescriptor {
    pub function_name: String,
    pub function_va: u64,
    pub function_size: u32,
    pub personality_fn: String,
    pub scope_actions: Vec<ScopeUnwindAction>,
}

/// Complete Table of Scope-Aware RAII Unwind Actions for the Binary.
#[derive(Debug, Clone, Default)]
pub struct UnwindActionTable {
    pub functions: BTreeMap<String, FunctionUnwindDescriptor>,
}

impl UnwindActionTable {
    pub fn new() -> Self {
        Self {
            functions: BTreeMap::new(),
        }
    }

    pub fn add_function(&mut self, desc: FunctionUnwindDescriptor) {
        self.functions.insert(desc.function_name.clone(), desc);
    }

    /// Locate the exact drop actions for an instruction offset within a function.
    pub fn find_actions_for_pc(&self, func_name: &str, pc_offset: u32) -> Option<&[String]> {
        if let Some(desc) = self.functions.get(func_name) {
            for action in &desc.scope_actions {
                if pc_offset >= action.start_offset && pc_offset < action.end_offset {
                    return Some(&action.drop_targets);
                }
            }
        }
        None
    }

    /// Encode the unwind action table into binary section bytes (`.adesh.unwind_map`).
    pub fn encode_binary(&self) -> Vec<u8> {
        let mut out = Vec::new();
        // Magic header: b"AUNWND\x01\x00" (8 bytes)
        out.extend_from_slice(b"AUNWND\x01\x00");
        // Function count
        out.extend_from_slice(&(self.functions.len() as u32).to_le_bytes());

        for (_, func) in &self.functions {
            // Function name length + bytes
            let name_bytes = func.function_name.as_bytes();
            out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            out.extend_from_slice(name_bytes);

            out.extend_from_slice(&func.function_va.to_le_bytes());
            out.extend_from_slice(&func.function_size.to_le_bytes());

            // Scope action count
            out.extend_from_slice(&(func.scope_actions.len() as u32).to_le_bytes());
            for action in &func.scope_actions {
                out.extend_from_slice(&action.start_offset.to_le_bytes());
                out.extend_from_slice(&action.end_offset.to_le_bytes());
                out.extend_from_slice(&action.cleanup_state_id.to_le_bytes());
                out.extend_from_slice(&(action.drop_targets.len() as u16).to_le_bytes());
                for target in &action.drop_targets {
                    let tb = target.as_bytes();
                    out.extend_from_slice(&(tb.len() as u16).to_le_bytes());
                    out.extend_from_slice(tb);
                }
            }
        }

        out
    }
}
