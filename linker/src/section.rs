//! Section representations, flags, alignments, and section merging.

use crate::relocation::Relocation;
use std::fmt;

/// Section category and intended usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SectionKind {
    Text,
    Rodata,
    Data,
    Bss,
    TData,
    TBss,
    Got,
    Plt,
    Reloc,
    SymTab,
    StrTab,
    Debug,
    Note,
    AdeshMeta,
    Custom,
}

/// Bitflags for section permissions and attributes.
pub mod flags {
    pub const READ: u32 = 1 << 0;
    pub const WRITE: u32 = 1 << 1;
    pub const EXEC: u32 = 1 << 2;
    pub const ALLOC: u32 = 1 << 3;
    pub const TLS: u32 = 1 << 4;
    pub const MERGE: u32 = 1 << 5;
    pub const STRINGS: u32 = 1 << 6;
    pub const COMDAT: u32 = 1 << 7;
    pub const DISCARD: u32 = 1 << 8;
}

/// Unified representation of an input or intermediate object section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub name: String,
    pub kind: SectionKind,
    pub flags: u32,
    pub alignment: u64,
    pub virtual_address: u64,
    pub file_offset: u64,
    pub size: u64,
    pub data: Vec<u8>,
    pub relocations: Vec<Relocation>,
    pub comdat_group: Option<String>,
    pub file_index: Option<usize>,
    pub is_live: bool,
    pub is_folded: bool,
    pub folded_into: Option<String>,
}

impl Section {
    pub fn new_code(name: impl Into<String>, data: Vec<u8>, alignment: u64) -> Self {
        let size = data.len() as u64;
        Self {
            name: name.into(),
            kind: SectionKind::Text,
            flags: flags::READ | flags::EXEC | flags::ALLOC,
            alignment: alignment.max(16),
            virtual_address: 0,
            file_offset: 0,
            size,
            data,
            relocations: Vec::new(),
            comdat_group: None,
            file_index: None,
            is_live: true,
            is_folded: false,
            folded_into: None,
        }
    }

    pub fn new_data(
        name: impl Into<String>,
        data: Vec<u8>,
        is_writable: bool,
        alignment: u64,
    ) -> Self {
        let size = data.len() as u64;
        let kind = if is_writable {
            SectionKind::Data
        } else {
            SectionKind::Rodata
        };
        let mut f = flags::READ | flags::ALLOC;
        if is_writable {
            f |= flags::WRITE;
        }
        Self {
            name: name.into(),
            kind,
            flags: f,
            alignment: alignment.max(8),
            virtual_address: 0,
            file_offset: 0,
            size,
            data,
            relocations: Vec::new(),
            comdat_group: None,
            file_index: None,
            is_live: true,
            is_folded: false,
            folded_into: None,
        }
    }

    pub fn new_bss(name: impl Into<String>, size: u64, alignment: u64) -> Self {
        Self {
            name: name.into(),
            kind: SectionKind::Bss,
            flags: flags::READ | flags::WRITE | flags::ALLOC,
            alignment: alignment.max(8),
            virtual_address: 0,
            file_offset: 0,
            size,
            data: Vec::new(),
            relocations: Vec::new(),
            comdat_group: None,
            file_index: None,
            is_live: true,
            is_folded: false,
            folded_into: None,
        }
    }

    pub fn new_debug(name: impl Into<String>, data: Vec<u8>, alignment: u64) -> Self {
        let size = data.len() as u64;
        Self {
            name: name.into(),
            kind: SectionKind::Debug,
            flags: flags::READ,
            alignment: alignment.max(1),
            virtual_address: 0,
            file_offset: 0,
            size,
            data,
            relocations: Vec::new(),
            comdat_group: None,
            file_index: None,
            is_live: true,
            is_folded: false,
            folded_into: None,
        }
    }

    pub fn is_executable(&self) -> bool {
        (self.flags & flags::EXEC) != 0
    }

    pub fn is_writable(&self) -> bool {
        (self.flags & flags::WRITE) != 0
    }

    pub fn is_alloc(&self) -> bool {
        (self.flags & flags::ALLOC) != 0
    }
}

impl fmt::Display for Section {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let r = if (self.flags & flags::READ) != 0 {
            "R"
        } else {
            "-"
        };
        let w = if (self.flags & flags::WRITE) != 0 {
            "W"
        } else {
            "-"
        };
        let x = if (self.flags & flags::EXEC) != 0 {
            "X"
        } else {
            "-"
        };
        write!(
            f,
            "{:18} 0x{:08x} size=0x{:06x} align={:4} [{}{}{}] relocs={}",
            self.name,
            self.virtual_address,
            self.size,
            self.alignment,
            r,
            w,
            x,
            self.relocations.len()
        )
    }
}

/// A merged output section consisting of aggregated input section chunks.
#[derive(Debug, Clone)]
pub struct MergedSection {
    pub name: String,
    pub kind: SectionKind,
    pub flags: u32,
    pub alignment: u64,
    pub virtual_address: u64,
    pub file_offset: u64,
    pub size: u64,
    pub data: Vec<u8>,
    pub input_sections: Vec<(usize, usize, u64)>, // (file_index, section_index, offset_in_merged)
    pub relocations: Vec<Relocation>,
}

impl MergedSection {
    pub fn new(name: impl Into<String>, kind: SectionKind, flags: u32, alignment: u64) -> Self {
        Self {
            name: name.into(),
            kind,
            flags,
            alignment: alignment.max(1),
            virtual_address: 0,
            file_offset: 0,
            size: 0,
            data: Vec::new(),
            input_sections: Vec::new(),
            relocations: Vec::new(),
        }
    }

    /// Append an input section into this merged section, aligning according to requirements.
    pub fn append_section(&mut self, sec: &Section, file_idx: usize, sec_idx: usize) -> u64 {
        self.alignment = self.alignment.max(sec.alignment);
        let offset = align_to(self.size.max(self.data.len() as u64), sec.alignment);

        if !matches!(sec.kind, SectionKind::Bss | SectionKind::TBss)
            && offset > self.data.len() as u64
        {
            self.data.resize(offset as usize, 0);
        }

        let section_offset_in_merged = offset;
        self.input_sections
            .push((file_idx, sec_idx, section_offset_in_merged));

        if !matches!(sec.kind, SectionKind::Bss | SectionKind::TBss) {
            self.data.extend_from_slice(&sec.data);
            self.size = offset + sec.size.max(sec.data.len() as u64);
        } else {
            self.size = offset + sec.size;
        }

        // Copy and adjust relocations
        for r in &sec.relocations {
            let mut adjusted = r.clone();
            adjusted.offset += section_offset_in_merged;
            // Record which object file supplied this relocation, and keep the
            // object-local symbol index intact. Resolving relocations by
            // (file, symbol index) is exact even when the source object has
            // many same-named sections (`.text`, `.rdata`, ...), which LLVM
            // COFF objects always do; name-based resolution picks an
            // arbitrary one of those sections.
            adjusted.file_index = Some(file_idx);
            self.relocations.push(adjusted);
        }

        section_offset_in_merged
    }

    /// Reserve an uninitialized block of `size` bytes with `alignment` in this
    /// merged section. Used for COMMON (tentative definition) storage, which
    /// has no input section to record in `input_sections`.
    pub fn append_common(&mut self, size: u64, alignment: u64) -> u64 {
        self.alignment = self.alignment.max(alignment.max(1));
        let offset = align_to(self.size.max(self.data.len() as u64), alignment);
        self.size = offset + size;
        offset
    }

    pub fn is_executable(&self) -> bool {
        (self.flags & flags::EXEC) != 0
    }

    pub fn is_writable(&self) -> bool {
        (self.flags & flags::WRITE) != 0
    }

    pub fn is_alloc(&self) -> bool {
        (self.flags & flags::ALLOC) != 0
    }
}

/// Helper function to align a value up to the given alignment.
#[inline]
pub fn align_to(value: u64, alignment: u64) -> u64 {
    if alignment <= 1 {
        value
    } else {
        (value + alignment - 1) & !(alignment - 1)
    }
}
