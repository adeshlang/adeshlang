//! ELF object and executable subsystem.

pub mod header;
pub mod notes;
pub mod reader;
pub mod reloc;
pub mod writer;

pub use reader::ElfReader;
pub use writer::ElfWriter;
