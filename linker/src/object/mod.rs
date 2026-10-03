//! Object File abstraction representing an input or intermediate relocatable artifact.

pub mod adob;
pub mod binary_reader;
pub mod reader;
pub mod symbols;
pub mod writer;

pub use adob::{ADOB_MAGIC, ADOB_VERSION_2, AdobV2};
pub use binary_reader::BinaryReader;
pub use reader::ObjectReader;
pub use symbols::ObjectSymbolIndex;
pub use writer::ObjectWriter;

use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::metadata::AdeshMetadata;
use crate::section::Section;
use crate::symbol::Symbol;
use crate::target::Target;
use std::fmt;
use std::path::PathBuf;

/// Unified object file representation.
#[derive(Debug, Clone)]
pub struct ObjectFile {
    pub path: PathBuf,
    pub target: Target,
    pub sections: Vec<Section>,
    pub symbols: Vec<Symbol>,
    pub metadata: Option<AdeshMetadata>,
    pub is_archive_member: bool,
    pub archive_name: Option<String>,
    pub file_index: usize,
}

impl ObjectFile {
    pub fn new(path: PathBuf, target: Target, file_index: usize) -> Self {
        Self {
            path,
            target,
            sections: Vec::new(),
            symbols: Vec::new(),
            metadata: None,
            is_archive_member: false,
            archive_name: None,
            file_index,
        }
    }

    pub fn add_section(&mut self, mut section: Section) -> usize {
        let idx = self.sections.len();
        section.file_index = Some(self.file_index);
        self.sections.push(section);
        idx
    }

    pub fn add_symbol(&mut self, mut symbol: Symbol) -> usize {
        let idx = self.symbols.len();
        symbol.file_index = Some(self.file_index);
        self.symbols.push(symbol);
        idx
    }

    pub fn find_section(&self, name: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.name == name)
    }

    /// Re-stamp this object's identity after extraction from a pre-parsed
    /// archive member. Archive members are eagerly parsed with a placeholder
    /// file index, and every embedded symbol/section/relocation index carries
    /// that placeholder; precise `(file, index)` relocation resolution would
    /// otherwise consult the wrong object (e.g. the user's ADOB, which is
    /// what file index 0 usually is) and fall back to name-based resolution,
    /// which cannot see file-local symbols such as LLVM's `switch.table.*`.
    pub fn reassign_file_index(&mut self, new_index: usize) {
        self.file_index = new_index;
        for sec in &mut self.sections {
            sec.file_index = Some(new_index);
            for reloc in &mut sec.relocations {
                reloc.file_index = Some(new_index);
            }
        }
        for sym in &mut self.symbols {
            sym.file_index = Some(new_index);
        }
    }

    pub fn find_section_mut(&mut self, name: &str) -> Option<&mut Section> {
        self.sections.iter_mut().find(|s| s.name == name)
    }

    pub fn find_symbol(&self, name: &str) -> Option<&Symbol> {
        self.symbols.iter().find(|s| s.name == name)
    }

    /// Perform defensive validation on sections, symbols, and relocations.
    pub fn validate(&self) -> LinkResult<()> {
        for sec in &self.sections {
            for reloc in &sec.relocations {
                let size = reloc.kind.size_in_bytes();
                if sec.kind != crate::section::SectionKind::Bss
                    && reloc.offset + (size as u64) > sec.data.len() as u64
                {
                    return Err(LinkError::new(
                        ErrorCode::InvalidObject,
                        format!(
                            "object `{}` section `{}` relocation at 0x{:x} exceeds section size 0x{:x}",
                            self.display_name(),
                            sec.name,
                            reloc.offset,
                            sec.data.len()
                        ),
                    ));
                }
            }

            if sec.alignment == 0 || (sec.alignment & (sec.alignment - 1)) != 0 {
                return Err(LinkError::new(
                    ErrorCode::InvalidSection,
                    format!(
                        "object `{}` section `{}` has non-power-of-2 alignment: {}",
                        self.display_name(),
                        sec.name,
                        sec.alignment
                    ),
                ));
            }
        }

        for sym in &self.symbols {
            if let Some(sec_idx) = sym.section_index {
                if sec_idx >= self.sections.len() {
                    return Err(LinkError::new(
                        ErrorCode::InvalidObject,
                        format!(
                            "object `{}` symbol `{}` references invalid section index {}",
                            self.display_name(),
                            sym.name,
                            sec_idx
                        ),
                    ));
                }
            }
        }

        Ok(())
    }

    pub fn display_name(&self) -> String {
        if let Some(ref ar) = self.archive_name {
            format!("{}({})", ar, self.path.display())
        } else {
            self.path.display().to_string()
        }
    }
}

impl fmt::Display for ObjectFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Object: {}", self.display_name())?;
        writeln!(f, "  Target: {}", self.target)?;
        writeln!(f, "  Sections ({}):", self.sections.len())?;
        for s in &self.sections {
            writeln!(f, "    {}", s)?;
        }
        writeln!(f, "  Symbols ({}):", self.symbols.len())?;
        for s in &self.symbols {
            writeln!(f, "    {}", s)?;
        }
        if let Some(ref meta) = self.metadata {
            writeln!(f, "  Metadata: {}", meta)?;
        }
        Ok(())
    }
}
