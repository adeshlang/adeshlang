//! Universal Object File reader with automatic format detection.

use crate::elf::reader::ElfReader;
use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::macho::reader::MachOReader;
use crate::object::ObjectFile;
use crate::pe::reader::PeReader;
use crate::target::Target;
use crate::wasm::sections::WasmReader;
use std::fs;
use std::path::Path;

pub struct ObjectReader;

impl ObjectReader {
    /// Read an object file from disk, auto-detecting the underlying format.
    pub fn read_from_file(path: &Path, default_target: &Target, file_index: usize) -> LinkResult<ObjectFile> {
        let bytes = fs::read(path).map_err(|e| {
            LinkError::new(
                ErrorCode::IoError,
                format!("failed to read object file `{}`: {}", path.display(), e),
            )
        })?;

        Self::read_from_memory(&bytes, path, default_target, file_index)
    }

    /// Read an object file from an in-memory byte slice.
    pub fn read_from_memory(
        bytes: &[u8],
        path: &Path,
        default_target: &Target,
        file_index: usize,
    ) -> LinkResult<ObjectFile> {
        if bytes.len() < 4 {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                format!("object file `{}` is truncated ({} bytes)", path.display(), bytes.len()),
            ));
        }

        let magic = &bytes[0..4];

        // 1. ELF format: \x7fELF
        if magic == b"\x7fELF" {
            return ElfReader::read(bytes, path, file_index);
        }

        // 2. WebAssembly format: \0asm
        if magic == b"\0asm" {
            return WasmReader::read(bytes, path, file_index);
        }

        // 3. Mach-O 64-bit format: 0xFEEDFACF or reversed endian 0xCFFAEDFE
        if magic == b"\xfe\xed\xfa\xcf" || magic == b"\xcf\xfa\xed\xfe" {
            return MachOReader::read(bytes, path, file_index);
        }

        // 4. PE/COFF: MS-DOS "MZ" or COFF headers (x86_64: 0x8664, ARM64: 0xAA64, i386: 0x014c)
        if &bytes[0..2] == b"MZ"
            || (bytes[0] == 0x64 && bytes[1] == 0x86)
            || (bytes[0] == 0x64 && bytes[1] == 0xAA)
            || (bytes[0] == 0x4c && bytes[1] == 0x01)
        {
            return PeReader::read(bytes, path, file_index);
        }

        // 5. Adesh Native Object format: ADOB
        if magic == b"ADOB" {
            return Self::read_adesh_native(bytes, path, default_target, file_index);
        }

        Err(LinkError::new(
            ErrorCode::InvalidObject,
            format!(
                "unrecognized object file format for `{}` (magic: {:02x?})",
                path.display(), magic
            ),
        ).with_suggestion("Ensure the file is a valid ELF, PE/COFF, Mach-O, WASM, or Adesh object file."))
    }

    /// Read Adesh native portable object file format.
    fn read_adesh_native(
        bytes: &[u8],
        path: &Path,
        default_target: &Target,
        file_index: usize,
    ) -> LinkResult<ObjectFile> {
        let mut obj = ObjectFile::new(path.to_path_buf(), default_target.clone(), file_index);
        let mut offset = 4;

        if bytes.len() < 16 {
            return Err(LinkError::new(ErrorCode::InvalidObject, "truncated Adesh object header"));
        }

        let sec_count = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        let sym_count = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        offset += 8;

        for _ in 0..sec_count {
            if offset + 32 > bytes.len() {
                return Err(LinkError::new(ErrorCode::InvalidObject, "truncated section header"));
            }
            let name_len = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
            offset += 4;
            let name = String::from_utf8_lossy(&bytes[offset..offset + name_len]).to_string();
            offset += name_len;

            let kind_id = bytes[offset];
            let flags = u32::from_le_bytes(bytes[offset + 1..offset + 5].try_into().unwrap());
            let align = u64::from_le_bytes(bytes[offset + 5..offset + 13].try_into().unwrap());
            let data_len = u64::from_le_bytes(bytes[offset + 13..offset + 21].try_into().unwrap()) as usize;
            let reloc_count = u32::from_le_bytes(bytes[offset + 21..offset + 25].try_into().unwrap()) as usize;
            offset += 25;

            let data = bytes[offset..offset + data_len].to_vec();
            offset += data_len;

            let kind = match kind_id {
                0 => crate::section::SectionKind::Text,
                1 => crate::section::SectionKind::Rodata,
                2 => crate::section::SectionKind::Data,
                3 => crate::section::SectionKind::Bss,
                4 => crate::section::SectionKind::AdeshMeta,
                _ => crate::section::SectionKind::Custom,
            };

            let mut sec = crate::section::Section {
                name,
                kind,
                flags,
                alignment: align,
                virtual_address: 0,
                file_offset: 0,
                size: data_len as u64,
                data,
                relocations: Vec::new(),
                comdat_group: None,
                file_index: Some(file_index),
                is_live: true,
                is_folded: false,
                folded_into: None,
            };

            for _ in 0..reloc_count {
                let r_off = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
                let r_kind_id = bytes[offset + 8];
                let r_addend = i64::from_le_bytes(bytes[offset + 9..offset + 17].try_into().unwrap());
                let sym_len = u32::from_le_bytes(bytes[offset + 17..offset + 21].try_into().unwrap()) as usize;
                offset += 21;
                let sym_name = String::from_utf8_lossy(&bytes[offset..offset + sym_len]).to_string();
                offset += sym_len;

                let r_kind = match r_kind_id {
                    0 => crate::relocation::RelocationKind::Absolute64,
                    1 => crate::relocation::RelocationKind::Absolute32,
                    2 => crate::relocation::RelocationKind::PcRelative32,
                    3 => crate::relocation::RelocationKind::PcRelative64,
                    4 => crate::relocation::RelocationKind::PltRelative32,
                    5 => crate::relocation::RelocationKind::GotRelative32,
                    _ => crate::relocation::RelocationKind::Absolute64,
                };

                sec.relocations.push(crate::relocation::Relocation::new(
                    r_off,
                    sym_name,
                    r_kind,
                    r_addend,
                ));
            }

            obj.add_section(sec);
        }

        for _ in 0..sym_count {
            let name_len = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
            offset += 4;
            let name = String::from_utf8_lossy(&bytes[offset..offset + name_len]).to_string();
            offset += name_len;

            let binding_id = bytes[offset];
            let type_id = bytes[offset + 1];
            let is_def = bytes[offset + 2] != 0;
            let sec_idx = u32::from_le_bytes(bytes[offset + 3..offset + 7].try_into().unwrap());
            let val = u64::from_le_bytes(bytes[offset + 7..offset + 15].try_into().unwrap());
            let sz = u64::from_le_bytes(bytes[offset + 15..offset + 23].try_into().unwrap());
            offset += 23;

            let binding = match binding_id {
                0 => crate::symbol::SymbolBinding::Local,
                1 => crate::symbol::SymbolBinding::Global,
                _ => crate::symbol::SymbolBinding::Weak,
            };

            let sym_type = match type_id {
                0 => crate::symbol::SymbolType::Function,
                1 => crate::symbol::SymbolType::Object,
                2 => crate::symbol::SymbolType::Tls,
                _ => crate::symbol::SymbolType::Unknown,
            };

            let sym = crate::symbol::Symbol {
                name,
                binding,
                visibility: crate::symbol::SymbolVisibility::Default,
                sym_type,
                section_index: if is_def { Some(sec_idx as usize) } else { None },
                value: val,
                size: sz,
                is_defined: is_def,
                is_imported: false,
                is_exported: false,
                file_index: Some(file_index),
                alias_of: None,
                comdat_group: None,
                version: None,
            };
            obj.add_symbol(sym);
        }

        if let Some(meta_sec) = obj.find_section(".adesh.meta") {
            if let Ok(meta) = crate::metadata::AdeshMetadata::decode(&meta_sec.data) {
                obj.metadata = Some(meta);
            }
        }

        obj.validate()?;
        Ok(obj)
    }
}
