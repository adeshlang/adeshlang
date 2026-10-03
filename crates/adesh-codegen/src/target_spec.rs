//! Centralized Target Specification and Machine Configuration for AdeshLang.
//!
//! Encapsulates architecture, OS, ABI, pointer width, endianness, calling convention,
//! stack alignment, object format, and code models to avoid scattered target checks across the backend.

use adesh_object::{Architecture, OperatingSystem, TargetDescriptor, TargetFeatures};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TargetAbi {
    SysV,
    Win64,
    Aapcs64,
    WasmUnknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Endianness {
    Little,
    Big,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ObjectFormatKind {
    Adob,
    Elf64,
    PeCoff,
    MachO64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RelocationModel {
    Static,
    Pic,
    Pie,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CodeModel {
    Small,
    Medium,
    Large,
}

/// Comprehensive Central Target Specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetSpec {
    pub descriptor: TargetDescriptor,
    pub architecture: Architecture,
    pub os: OperatingSystem,
    pub abi: TargetAbi,
    pub pointer_width_bits: u8,
    pub endianness: Endianness,
    pub stack_alignment_bytes: usize,
    pub object_format: ObjectFormatKind,
    pub relocation_model: RelocationModel,
    pub code_model: CodeModel,
    pub features: TargetFeatures,
}

impl TargetSpec {
    /// Construct TargetSpec from a TargetDescriptor.
    pub fn for_descriptor(descriptor: TargetDescriptor) -> Self {
        let arch = descriptor.architecture.clone();
        let os = descriptor.operating_system;

        let abi = match (&arch, os) {
            (Architecture::X86_64, OperatingSystem::Windows) => TargetAbi::Win64,
            (Architecture::X86_64, _) => TargetAbi::SysV,
            (Architecture::AArch64, _) => TargetAbi::Aapcs64,
            _ => TargetAbi::SysV,
        };

        let pointer_width_bits = match &arch {
            Architecture::X86_64
            | Architecture::AArch64
            | Architecture::RiscV64
            | Architecture::Wasm64 => 64,
            _ => 32,
        };

        let object_format = match os {
            OperatingSystem::Windows => ObjectFormatKind::PeCoff,
            OperatingSystem::MacOS | OperatingSystem::Ios => ObjectFormatKind::MachO64,
            _ => ObjectFormatKind::Elf64,
        };

        Self {
            descriptor: descriptor.clone(),
            architecture: arch,
            os,
            abi,
            pointer_width_bits,
            endianness: Endianness::Little,
            stack_alignment_bytes: 16,
            object_format,
            relocation_model: RelocationModel::Pic,
            code_model: CodeModel::Small,
            features: descriptor.features,
        }
    }

    /// Convenience constructor for x86-64 Windows MSVC.
    pub fn x86_64_windows() -> Self {
        let desc = TargetDescriptor::from_triple("x86_64-pc-windows-msvc")
            .unwrap_or_else(|_| TargetDescriptor::host());
        Self::for_descriptor(desc)
    }

    /// Convenience constructor for x86-64 Linux GNU (SysV).
    pub fn x86_64_linux() -> Self {
        let desc = TargetDescriptor::from_triple("x86_64-unknown-linux-gnu")
            .unwrap_or_else(|_| TargetDescriptor::host());
        Self::for_descriptor(desc)
    }

    #[inline]
    pub fn is_windows(&self) -> bool {
        self.os == OperatingSystem::Windows
    }

    #[inline]
    pub fn is_sysv(&self) -> bool {
        self.abi == TargetAbi::SysV
    }

    #[inline]
    pub fn shadow_space_bytes(&self) -> usize {
        if self.abi == TargetAbi::Win64 { 32 } else { 0 }
    }
}
