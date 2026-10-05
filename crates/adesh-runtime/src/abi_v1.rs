//! Phase 10 — Adesh Runtime ABI Version 1 Specification.
//!
//! Defines:
//! - Canonical calling conventions, stack alignment rules, and symbol mangling.
//! - Binary compatibility headers ensuring older binaries load safely into newer runtimes.

pub const ADESH_RUNTIME_ABI_VERSION_1: u32 = 1;

/// Adesh Runtime ABI Descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdeshRuntimeAbiV1 {
    pub abi_version: u32,
    pub pointer_width_bits: u8,
    pub stack_alignment_bytes: u8,
    pub symbol_prefix: String,
    pub error_convention: String,
}

impl AdeshRuntimeAbiV1 {
    pub fn current() -> Self {
        Self {
            abi_version: ADESH_RUNTIME_ABI_VERSION_1,
            pointer_width_bits: (std::mem::size_of::<usize>() * 8) as u8,
            stack_alignment_bytes: 16,
            symbol_prefix: "adesh_".to_string(),
            error_convention: "ResultOrPanic".to_string(),
        }
    }

    /// Verify binary compatibility between an incoming object/plugin ABI and the host runtime.
    pub fn is_compatible(&self, other: &AdeshRuntimeAbiV1) -> bool {
        self.abi_version == other.abi_version
            && self.pointer_width_bits == other.pointer_width_bits
            && self.stack_alignment_bytes <= other.stack_alignment_bytes
    }
}

impl Default for AdeshRuntimeAbiV1 {
    fn default() -> Self {
        Self::current()
    }
}
