//! Section table and MemoryRegion definitions for ADOB.

use crate::relocation::AdobRelocation;

/// Section Kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SectionKind {
    #[default]
    Text,
    Rodata,
    Data,
    Bss,
    Tls,
    Unwind,
    Debug,
    AdeshMeta,
    Comdat,
    Extension,
    Custom,
}

/// Bitflags for section properties.
pub mod section_flags {
    pub const READ: u32 = 1 << 0;
    pub const WRITE: u32 = 1 << 1;
    pub const EXECUTE: u32 = 1 << 2;
    pub const ALLOC: u32 = 1 << 3;
    pub const TLS: u32 = 1 << 4;
    pub const MERGE: u32 = 1 << 5;
    pub const STRINGS: u32 = 1 << 6;
    pub const EXCLUDE: u32 = 1 << 7;
}

/// Memory permissions for embedded memory regions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MemoryPermissions {
    R,
    Rw,
    #[default]
    Rx,
    Rwx,
}

/// Embedded / bare-metal memory region descriptor (e.g. Flash, RAM, MMIO).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRegion {
    pub name: String,
    pub address: u64,
    pub size: u64,
    pub permissions: MemoryPermissions,
}

impl MemoryRegion {
    pub fn new(
        name: impl Into<String>,
        address: u64,
        size: u64,
        permissions: MemoryPermissions,
    ) -> Self {
        Self {
            name: name.into(),
            address,
            size,
            permissions,
        }
    }
}

/// ADOB Section definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdobSection {
    pub name: String,
    pub kind: SectionKind,
    pub flags: u32,
    pub alignment: u64,
    pub data: Vec<u8>,
    pub relocations: Vec<AdobRelocation>,
    pub comdat_group: Option<String>,
    pub memory_region: Option<String>,
}

impl AdobSection {
    pub fn new(name: impl Into<String>, kind: SectionKind) -> Self {
        let (flags, align) = match kind {
            SectionKind::Text => (
                section_flags::READ | section_flags::EXECUTE | section_flags::ALLOC,
                16,
            ),
            SectionKind::Rodata => (section_flags::READ | section_flags::ALLOC, 8),
            SectionKind::Data => (
                section_flags::READ | section_flags::WRITE | section_flags::ALLOC,
                8,
            ),
            SectionKind::Bss => (
                section_flags::READ | section_flags::WRITE | section_flags::ALLOC,
                8,
            ),
            SectionKind::Tls => (
                section_flags::READ
                    | section_flags::WRITE
                    | section_flags::ALLOC
                    | section_flags::TLS,
                8,
            ),
            SectionKind::Unwind => (section_flags::READ | section_flags::ALLOC, 8),
            SectionKind::Debug => (section_flags::READ, 1),
            _ => (section_flags::READ | section_flags::ALLOC, 4),
        };

        Self {
            name: name.into(),
            kind,
            flags,
            alignment: align,
            data: Vec::new(),
            relocations: Vec::new(),
            comdat_group: None,
            memory_region: None,
        }
    }

    pub fn with_data(mut self, data: Vec<u8>) -> Self {
        self.data = data;
        self
    }

    pub fn with_alignment(mut self, alignment: u64) -> Self {
        self.alignment = alignment;
        self
    }

    pub fn with_flags(mut self, flags: u32) -> Self {
        self.flags = flags;
        self
    }

    pub fn with_comdat(mut self, group: impl Into<String>) -> Self {
        self.comdat_group = Some(group.into());
        self
    }

    pub fn with_memory_region(mut self, region: impl Into<String>) -> Self {
        self.memory_region = Some(region.into());
        self
    }

    pub fn add_relocation(&mut self, reloc: AdobRelocation) {
        self.relocations.push(reloc);
    }
}
