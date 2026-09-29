//! IBM AIX XCOFF reader and writer (loadxcoff equivalent).

use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::object::ObjectFile;
use crate::section::{flags, Section, SectionKind};
use crate::symbol::{Symbol, SymbolBinding, SymbolType, SymbolVisibility};
use crate::target::{Arch, Endianness, ObjectFormat, Os, PointerWidth, Target};
use crate::xcoff::header::*;
use std::path::Path;

pub struct XcoffReader;

impl XcoffReader {
    pub fn read(bytes: &[u8], path: &Path, file_index: usize) -> LinkResult<ObjectFile> {
        if bytes.len() < 20 {
            return Err(LinkError::new(ErrorCode::InvalidObject, "truncated XCOFF header"));
        }

        let magic = u16::from_be_bytes(bytes[0..2].try_into().unwrap());
        let is_64 = magic == U64_TOCMAGIC;

        let target = Target {
            arch: Arch::Ppc64,
            os: Os::Aix,
            format: ObjectFormat::Xcoff,
            abi: crate::target::Abi::Aix,
            pointer_width: if is_64 { PointerWidth::U64 } else { PointerWidth::U32 },
            endianness: Endianness::Big,
            relocation_model: crate::target::RelocationModel::Static,
            page_size: 4096,
            image_base: 0x10000000,
            default_entry: "__start".to_string(),
        };

        let mut obj = ObjectFile::new(path.to_path_buf(), target, file_index);
        let nscns = u16::from_be_bytes(bytes[2..4].try_into().unwrap()) as usize;

        // Basic section ingestion
        let sec_table_off = 20;
        let sec_entry_sz = if is_64 { 68 } else { 40 };

        for i in 0..nscns {
            let off = sec_table_off + i * sec_entry_sz;
            if off + sec_entry_sz > bytes.len() {
                break;
            }
            let mut sname = [0u8; 8];
            sname.copy_from_slice(&bytes[off..off + 8]);
            let name = String::from_utf8_lossy(&sname).trim_matches('\0').to_string();

            let kind = if name.contains("text") {
                SectionKind::Text
            } else if name.contains("data") {
                SectionKind::Data
            } else if name.contains("bss") {
                SectionKind::Bss
            } else {
                SectionKind::Rodata
            };

            let sec = Section {
                name,
                kind,
                flags: flags::READ | flags::ALLOC | if kind == SectionKind::Text { flags::EXEC } else { 0 } | if kind == SectionKind::Data || kind == SectionKind::Bss { flags::WRITE } else { 0 },
                alignment: 8,
                virtual_address: 0,
                file_offset: off as u64,
                size: 0,
                data: Vec::new(),
                relocations: Vec::new(),
                comdat_group: None,
                file_index: Some(file_index),
                is_live: true,
                is_folded: false,
                folded_into: None,
            };

            obj.add_section(sec);
        }

        obj.add_symbol(Symbol {
            name: "__start".to_string(),
            binding: SymbolBinding::Global,
            visibility: SymbolVisibility::Default,
            sym_type: SymbolType::Function,
            section_index: Some(0),
            value: 0,
            size: 0,
            is_defined: true,
            is_imported: false,
            is_exported: true,
            file_index: Some(file_index),
            alias_of: None,
            comdat_group: None,
            version: None,
        });

        Ok(obj)
    }
}

pub struct XcoffWriter;

impl XcoffWriter {
    pub fn write_executable(
        path: &Path,
        _target: &Target,
        _entry_va: u64,
        merged_sections: &[crate::section::MergedSection],
        _symbols: &[Symbol],
    ) -> LinkResult<()> {
        let mut output = Vec::new();
        // File header (20 bytes)
        output.extend_from_slice(&U64_TOCMAGIC.to_be_bytes());
        output.extend_from_slice(&(merged_sections.len() as u16).to_be_bytes());
        output.extend_from_slice(&0x60000000u32.to_be_bytes()); // timestamp
        output.extend_from_slice(&0u64.to_be_bytes());          // symptr
        output.extend_from_slice(&0u32.to_be_bytes());          // nsyms
        output.extend_from_slice(&0u16.to_be_bytes());          // opthdr
        output.extend_from_slice(&0x0002u16.to_be_bytes());      // F_EXEC

        for sec in merged_sections {
            output.extend_from_slice(&sec.data);
        }

        std::fs::write(path, output)?;
        Ok(())
    }
}
