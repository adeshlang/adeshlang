//! Adesh Link Metadata (`.adesh.meta`) binary specification and ABI validation.

use crate::error::{ErrorCode, LinkError, LinkResult};
use std::fmt;

pub const ADESH_META_MAGIC: [u8; 4] = *b"ADLN";
pub const ADESH_CURRENT_ABI_VERSION: u32 = 1;

/// Flags describing Adesh runtime capabilities and compilation features.
pub mod flags {
    pub const FEATURE_GC: u32 = 1 << 0;
    pub const FEATURE_ASYNC: u32 = 1 << 1;
    pub const FEATURE_SIMD: u32 = 1 << 2;
    pub const FEATURE_HARDENED: u32 = 1 << 3;
    pub const FEATURE_THREADING: u32 = 1 << 4;
}

/// Binary metadata stored in `.adesh.meta` sections of Adesh object files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdeshMetadata {
    pub abi_version: u32,
    pub compiler_version: (u16, u16, u16),
    pub runtime_abi_version: u32,
    pub feature_flags: u32,
    pub target_triple: String,
    pub source_hash: [u8; 32],
}

impl Default for AdeshMetadata {
    fn default() -> Self {
        Self {
            abi_version: ADESH_CURRENT_ABI_VERSION,
            compiler_version: (0, 3, 0),
            runtime_abi_version: 1,
            feature_flags: flags::FEATURE_GC | flags::FEATURE_HARDENED,
            target_triple: "x86_64-linux".to_string(),
            source_hash: [0u8; 32],
        }
    }
}

impl AdeshMetadata {
    /// Encode metadata into a compact binary byte slice for inclusion in object files.
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(64 + self.target_triple.len());
        buf.extend_from_slice(&ADESH_META_MAGIC);
        buf.extend_from_slice(&self.abi_version.to_le_bytes());
        buf.extend_from_slice(&self.compiler_version.0.to_le_bytes());
        buf.extend_from_slice(&self.compiler_version.1.to_le_bytes());
        buf.extend_from_slice(&self.compiler_version.2.to_le_bytes());
        buf.extend_from_slice(&self.runtime_abi_version.to_le_bytes());
        buf.extend_from_slice(&self.feature_flags.to_le_bytes());
        buf.extend_from_slice(&self.source_hash);

        let triple_bytes = self.target_triple.as_bytes();
        let triple_len = triple_bytes.len() as u32;
        buf.extend_from_slice(&triple_len.to_le_bytes());
        buf.extend_from_slice(triple_bytes);
        buf
    }

    /// Decode metadata from a `.adesh.meta` section byte slice with bounds checking.
    pub fn decode(bytes: &[u8]) -> LinkResult<Self> {
        if bytes.len() < 58 {
            return Err(LinkError::new(
                ErrorCode::MetadataMismatch,
                "corrupted `.adesh.meta` section: header too short",
            ));
        }

        if &bytes[0..4] != &ADESH_META_MAGIC {
            return Err(LinkError::new(
                ErrorCode::MetadataMismatch,
                "invalid `.adesh.meta` section magic bytes",
            ));
        }

        let abi_version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let comp_major = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
        let comp_minor = u16::from_le_bytes(bytes[10..12].try_into().unwrap());
        let comp_patch = u16::from_le_bytes(bytes[12..14].try_into().unwrap());
        let runtime_abi_version = u32::from_le_bytes(bytes[14..18].try_into().unwrap());
        let feature_flags = u32::from_le_bytes(bytes[18..22].try_into().unwrap());

        let mut source_hash = [0u8; 32];
        source_hash.copy_from_slice(&bytes[22..54]);

        let triple_len = u32::from_le_bytes(bytes[54..58].try_into().unwrap()) as usize;
        if bytes.len() < 58 + triple_len {
            return Err(LinkError::new(
                ErrorCode::MetadataMismatch,
                "corrupted `.adesh.meta` section: truncated target triple string",
            ));
        }

        let target_triple = String::from_utf8_lossy(&bytes[58..58 + triple_len]).to_string();

        Ok(Self {
            abi_version,
            compiler_version: (comp_major, comp_minor, comp_patch),
            runtime_abi_version,
            feature_flags,
            target_triple,
            source_hash,
        })
    }

    /// Validate metadata compatibility across linked object files.
    pub fn validate_compatibility(
        &self,
        other: &AdeshMetadata,
        this_file: &str,
        other_file: &str,
    ) -> LinkResult<()> {
        if self.abi_version != other.abi_version {
            return Err(LinkError::new(
                ErrorCode::MetadataMismatch,
                format!(
                    "Adesh ABI version mismatch between `{}` (ABI v{}) and `{}` (ABI v{})",
                    this_file, self.abi_version, other_file, other.abi_version
                ),
            )
            .with_suggestion(
                "Recompile both source files with the same version of the Adesh compiler.",
            ));
        }

        if self.runtime_abi_version != other.runtime_abi_version {
            return Err(LinkError::new(
                ErrorCode::MetadataMismatch,
                format!(
                    "Adesh runtime ABI mismatch between `{}` (v{}) and `{}` (v{})",
                    this_file, self.runtime_abi_version, other_file, other.runtime_abi_version
                ),
            ));
        }

        Ok(())
    }
}

impl fmt::Display for AdeshMetadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Adesh ABI v{} (Compiler {}.{}.{}, Runtime ABI v{}, Target: {})",
            self.abi_version,
            self.compiler_version.0,
            self.compiler_version.1,
            self.compiler_version.2,
            self.runtime_abi_version,
            self.target_triple
        )
    }
}
