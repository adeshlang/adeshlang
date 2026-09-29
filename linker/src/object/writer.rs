//! Object file format serializer and binary generator.

use crate::error::LinkResult;
use crate::object::ObjectFile;
use std::fs;
use std::path::Path;

pub struct ObjectWriter;

impl ObjectWriter {
    /// Write an ObjectFile into the Adesh native object binary format.
    pub fn write_to_file(obj: &ObjectFile, path: &Path) -> LinkResult<()> {
        let bytes = Self::encode(obj)?;
        fs::write(path, bytes)?;
        Ok(())
    }

    /// Encode an ObjectFile into bytes.
    pub fn encode(obj: &ObjectFile) -> LinkResult<Vec<u8>> {
        let mut buf = Vec::new();
        // Header: Magic "ADOB"
        buf.extend_from_slice(b"ADOB");
        buf.extend_from_slice(&(obj.sections.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(obj.symbols.len() as u32).to_le_bytes());

        // Sections
        for sec in &obj.sections {
            let name_bytes = sec.name.as_bytes();
            buf.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
            buf.extend_from_slice(name_bytes);

            let kind_id: u8 = match sec.kind {
                crate::section::SectionKind::Text => 0,
                crate::section::SectionKind::Rodata => 1,
                crate::section::SectionKind::Data => 2,
                crate::section::SectionKind::Bss => 3,
                crate::section::SectionKind::AdeshMeta => 4,
                _ => 5,
            };
            buf.push(kind_id);
            buf.extend_from_slice(&sec.flags.to_le_bytes());
            buf.extend_from_slice(&sec.alignment.to_le_bytes());
            buf.extend_from_slice(&(sec.data.len() as u64).to_le_bytes());
            buf.extend_from_slice(&(sec.relocations.len() as u32).to_le_bytes());
            buf.extend_from_slice(&sec.data);

            for reloc in &sec.relocations {
                buf.extend_from_slice(&reloc.offset.to_le_bytes());
                let r_kind_id: u8 = match reloc.kind {
                    crate::relocation::RelocationKind::Absolute64 => 0,
                    crate::relocation::RelocationKind::Absolute32 => 1,
                    crate::relocation::RelocationKind::PcRelative32 => 2,
                    crate::relocation::RelocationKind::PcRelative64 => 3,
                    crate::relocation::RelocationKind::PltRelative32 => 4,
                    crate::relocation::RelocationKind::GotRelative32 => 5,
                    _ => 0,
                };
                buf.push(r_kind_id);
                buf.extend_from_slice(&reloc.addend.to_le_bytes());
                let sym_bytes = reloc.symbol_name.as_bytes();
                buf.extend_from_slice(&(sym_bytes.len() as u32).to_le_bytes());
                buf.extend_from_slice(sym_bytes);
            }
        }

        // Symbols
        for sym in &obj.symbols {
            let name_bytes = sym.name.as_bytes();
            buf.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
            buf.extend_from_slice(name_bytes);

            let bind_id: u8 = match sym.binding {
                crate::symbol::SymbolBinding::Local => 0,
                crate::symbol::SymbolBinding::Global => 1,
                crate::symbol::SymbolBinding::Weak => 2,
            };
            let type_id: u8 = match sym.sym_type {
                crate::symbol::SymbolType::Function => 0,
                crate::symbol::SymbolType::Object => 1,
                crate::symbol::SymbolType::Tls => 2,
                _ => 3,
            };
            buf.push(bind_id);
            buf.push(type_id);
            buf.push(if sym.is_defined { 1 } else { 0 });
            let sec_idx = sym.section_index.unwrap_or(0) as u32;
            buf.extend_from_slice(&sec_idx.to_le_bytes());
            buf.extend_from_slice(&sym.value.to_le_bytes());
            buf.extend_from_slice(&sym.size.to_le_bytes());
        }

        Ok(buf)
    }
}
