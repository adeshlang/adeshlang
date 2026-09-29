//! Mach-O relocations definitions.

pub const X86_64_RELOC_UNSIGNED: u32 = 0;
pub const X86_64_RELOC_SIGNED: u32 = 1;
pub const X86_64_RELOC_BRANCH: u32 = 2;
pub const X86_64_RELOC_GOT_LOAD: u32 = 3;
pub const X86_64_RELOC_GOT: u32 = 4;

pub const ARM64_RELOC_UNSIGNED: u32 = 0;
pub const ARM64_RELOC_BRANCH26: u32 = 2;
pub const ARM64_RELOC_PAGE21: u32 = 3;
pub const ARM64_RELOC_PAGEOFF12: u32 = 4;
pub const ARM64_RELOC_GOT_LOAD_PAGE21: u32 = 5;
pub const ARM64_RELOC_GOT_LOAD_PAGEOFF12: u32 = 6;
