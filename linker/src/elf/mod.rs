//! ELF object and executable subsystem.

pub mod dynamic;
pub mod header;
pub mod notes;
pub mod reader;
pub mod reloc;
pub mod writer;
pub mod aarch64;
pub mod x86_64;

pub use dynamic::{ElfDynamicReloc, ElfDynamicRelocKind, ElfLinkInfo};
pub use reader::ElfReader;
pub use writer::ElfWriter;
