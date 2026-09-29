//! Mach-O Object File Reader.

use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::macho::header::*;
use crate::object::ObjectFile;
use crate::section::{Section, SectionKind, flags};
use crate::symbol::{Symbol, SymbolBinding, SymbolType, SymbolVisibility};
use crate::target::{Arch, Endianness, ObjectFormat, Os, PointerWidth, Target};
use std::path::Path;

pub struct MachOReader;

impl MachOReader {
    pub fn read(bytes: &[u8], path: &Path, file_index: usize) -> LinkResult<ObjectFile> {
        if bytes.len() < 32 {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                format!("Mach-O file `{}` is truncated", path.display()),
            ));
        }

        let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        if magic != MH_MAGIC_64 {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                format!("unsupported Mach-O magic: 0x{:08x}", magic),
            ));
        }

        let cputype = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let arch = match cputype {
            CPU_TYPE_X86_64 => Arch::X86_64,
            CPU_TYPE_ARM64 => Arch::AArch64,
            _ => Arch::AArch64,
        };

        let target = Target {
            arch,
            os: Os::MacOS,
            format: ObjectFormat::MachO,
            abi: crate::target::Abi::Darwin,
            pointer_width: PointerWidth::U64,
            endianness: Endianness::Little,
            relocation_model: crate::target::RelocationModel::Pic,
            page_size: if arch == Arch::AArch64 { 16384 } else { 4096 },
            image_base: 0x100000000,
            default_entry: "_main".to_string(),
        };

        let mut obj = ObjectFile::new(path.to_path_buf(), target, file_index);

        let ncmds = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
        let mut offset = 32usize;

        let mut symoff = 0usize;
        let mut nsyms = 0usize;
        let mut stroff = 0usize;
        let mut strsize = 0usize;

        for _ in 0..ncmds {
            if offset + 8 > bytes.len() {
                break;
            }
            let cmd = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
            let cmdsize =
                u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;

            if cmd == LC_SEGMENT_64 && offset + 72 <= bytes.len() {
                let nsects = u32::from_le_bytes(bytes[offset + 64..offset + 68].try_into().unwrap())
                    as usize;
                let mut sect_off = offset + 72;

                for _ in 0..nsects {
                    if sect_off + 80 > bytes.len() {
                        break;
                    }
                    let mut sectname = [0u8; 16];
                    sectname.copy_from_slice(&bytes[sect_off..sect_off + 16]);
                    let end = sectname.iter().position(|&b| b == 0).unwrap_or(16);
                    let name = String::from_utf8_lossy(&sectname[0..end]).to_string();

                    let addr =
                        u64::from_le_bytes(bytes[sect_off + 32..sect_off + 40].try_into().unwrap());
                    let size =
                        u64::from_le_bytes(bytes[sect_off + 40..sect_off + 48].try_into().unwrap());
                    let file_off =
                        u32::from_le_bytes(bytes[sect_off + 48..sect_off + 52].try_into().unwrap())
                            as usize;
                    let align = 1u64 << bytes[sect_off + 52];

                    let kind = if name.contains("text") {
                        SectionKind::Text
                    } else if name.contains("data") {
                        SectionKind::Data
                    } else if name.contains("bss") {
                        SectionKind::Bss
                    } else if name.contains("adesh") {
                        SectionKind::AdeshMeta
                    } else {
                        SectionKind::Rodata
                    };

                    let data =
                        if kind != SectionKind::Bss && file_off + (size as usize) <= bytes.len() {
                            bytes[file_off..file_off + (size as usize)].to_vec()
                        } else {
                            Vec::new()
                        };

                    let sec = Section {
                        name,
                        kind,
                        flags: flags::READ
                            | flags::ALLOC
                            | if kind == SectionKind::Text {
                                flags::EXEC
                            } else {
                                0
                            }
                            | if kind == SectionKind::Data || kind == SectionKind::Bss {
                                flags::WRITE
                            } else {
                                0
                            },
                        alignment: align,
                        virtual_address: addr,
                        file_offset: file_off as u64,
                        size,
                        data,
                        relocations: Vec::new(),
                        comdat_group: None,
                        file_index: Some(file_index),
                        is_live: true,
                        is_folded: false,
                        folded_into: None,
                    };

                    obj.add_section(sec);
                    sect_off += 80;
                }
            } else if cmd == LC_SYMTAB && offset + 24 <= bytes.len() {
                symoff =
                    u32::from_le_bytes(bytes[offset + 8..offset + 12].try_into().unwrap()) as usize;
                nsyms = u32::from_le_bytes(bytes[offset + 12..offset + 16].try_into().unwrap())
                    as usize;
                stroff = u32::from_le_bytes(bytes[offset + 16..offset + 20].try_into().unwrap())
                    as usize;
                strsize = u32::from_le_bytes(bytes[offset + 20..offset + 24].try_into().unwrap())
                    as usize;
            }

            offset += cmdsize;
        }

        // Parse Symbols
        if symoff > 0
            && stroff > 0
            && symoff + nsyms * 16 <= bytes.len()
            && stroff + strsize <= bytes.len()
        {
            let strtab = &bytes[stroff..stroff + strsize];
            for i in 0..nsyms {
                let s_off = symoff + i * 16;
                let n_strx =
                    u32::from_le_bytes(bytes[s_off..s_off + 4].try_into().unwrap()) as usize;
                let n_type = bytes[s_off + 4];
                let n_sect = bytes[s_off + 5];
                let n_value = u64::from_le_bytes(bytes[s_off + 8..s_off + 16].try_into().unwrap());

                let sym_name = if n_strx < strtab.len() {
                    let end = strtab[n_strx..]
                        .iter()
                        .position(|&b| b == 0)
                        .map(|p| n_strx + p)
                        .unwrap_or(strtab.len());
                    String::from_utf8_lossy(&strtab[n_strx..end]).to_string()
                } else {
                    String::new()
                };

                let is_defined = n_sect > 0;
                let sec_idx = if is_defined && (n_sect as usize) <= obj.sections.len() {
                    Some((n_sect as usize) - 1)
                } else {
                    None
                };

                let binding = if (n_type & 0x01) != 0 {
                    SymbolBinding::Global
                } else {
                    SymbolBinding::Local
                };

                let sym = Symbol {
                    name: sym_name,
                    binding,
                    visibility: SymbolVisibility::Default,
                    sym_type: SymbolType::Function,
                    section_index: sec_idx,
                    value: n_value,
                    size: 0,
                    is_defined,
                    is_imported: false,
                    is_exported: false,
                    file_index: Some(file_index),
                    alias_of: None,
                    comdat_group: None,
                    version: None,
                };

                if !sym.name.is_empty() {
                    obj.add_symbol(sym);
                }
            }
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
