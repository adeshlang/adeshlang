//! PE / COFF Object File Reader.

use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::object::ObjectFile;
use crate::pe::header::*;
use crate::section::{flags, Section, SectionKind};
use crate::symbol::{Symbol, SymbolBinding, SymbolType, SymbolVisibility};
use crate::target::{Arch, Endianness, ObjectFormat, Os, PointerWidth, Target};
use std::path::Path;

pub struct PeReader;

impl PeReader {
    pub fn read(bytes: &[u8], path: &Path, file_index: usize) -> LinkResult<ObjectFile> {
        if bytes.len() < 20 {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                format!("COFF object `{}` is too small ({} bytes)", path.display(), bytes.len()),
            ));
        }

        let mut offset = 0;
        // Check if MS-DOS header present
        if &bytes[0..2] == b"MZ" {
            if bytes.len() < 0x40 {
                return Err(LinkError::new(ErrorCode::InvalidObject, "truncated DOS header"));
            }
            let lfanew = u32::from_le_bytes(bytes[0x3C..0x40].try_into().unwrap()) as usize;
            if lfanew + 24 > bytes.len() || &bytes[lfanew..lfanew + 4] != &PE_SIGNATURE {
                return Err(LinkError::new(ErrorCode::InvalidObject, "invalid PE signature in MS-DOS binary"));
            }
            offset = lfanew + 4;
        }

        let machine = u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
        let num_sections = u16::from_le_bytes(bytes[offset + 2..offset + 4].try_into().unwrap()) as usize;
        let sym_ptr = u32::from_le_bytes(bytes[offset + 8..offset + 12].try_into().unwrap()) as usize;
        let num_symbols = u32::from_le_bytes(bytes[offset + 12..offset + 16].try_into().unwrap()) as usize;
        let opt_hdr_size = u16::from_le_bytes(bytes[offset + 16..offset + 18].try_into().unwrap()) as usize;

        let arch = match machine {
            IMAGE_FILE_MACHINE_AMD64 => Arch::X86_64,
            IMAGE_FILE_MACHINE_ARM64 => Arch::AArch64,
            IMAGE_FILE_MACHINE_I386 => Arch::X86,
            _ => Arch::X86_64,
        };

        let target = Target {
            arch,
            os: Os::Windows,
            format: ObjectFormat::Pe,
            abi: crate::target::Abi::WindowsMsvc,
            pointer_width: if arch == Arch::X86 { PointerWidth::U32 } else { PointerWidth::U64 },
            endianness: Endianness::Little,
            relocation_model: crate::target::RelocationModel::Static,
            page_size: 4096,
            image_base: if arch == Arch::X86 { 0x00400000 } else { 0x140000000 },
            default_entry: "mainCRTStartup".to_string(),
        };

        let mut obj = ObjectFile::new(path.to_path_buf(), target, file_index);

        let section_table_offset = offset + 20 + opt_hdr_size;
        let section_entry_size = 40;

        if section_table_offset + num_sections * section_entry_size > bytes.len() {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                format!("COFF section table in `{}` exceeds file bounds", path.display()),
            ));
        }

        // String table is located immediately after the COFF symbol table
        let string_table_offset = sym_ptr + num_symbols * 18;
        let string_table = if string_table_offset < bytes.len() {
            &bytes[string_table_offset..]
        } else {
            &[]
        };

        let get_string = |name_bytes: &[u8; 8]| -> String {
            if name_bytes[0] == b'/' {
                // String table offset: /1234
                if let Ok(st_off_str) = std::str::from_utf8(&name_bytes[1..]) {
                    if let Ok(st_off) = st_off_str.trim_matches('\0').trim().parse::<usize>() {
                        if st_off < string_table.len() {
                            let end = string_table[st_off..].iter().position(|&b| b == 0).map(|p| st_off + p).unwrap_or(string_table.len());
                            return String::from_utf8_lossy(&string_table[st_off..end]).to_string();
                        }
                    }
                }
            }
            let end = name_bytes.iter().position(|&b| b == 0).unwrap_or(8);
            String::from_utf8_lossy(&name_bytes[0..end]).to_string()
        };

        // Parse section headers
        for i in 0..num_sections {
            let s_off = section_table_offset + i * section_entry_size;
            let mut name_raw = [0u8; 8];
            name_raw.copy_from_slice(&bytes[s_off..s_off + 8]);
            let name = get_string(&name_raw);

            let virtual_size = u32::from_le_bytes(bytes[s_off + 8..s_off + 12].try_into().unwrap()) as u64;
            let raw_data_size = u32::from_le_bytes(bytes[s_off + 16..s_off + 20].try_into().unwrap()) as usize;
            let raw_data_ptr = u32::from_le_bytes(bytes[s_off + 20..s_off + 24].try_into().unwrap()) as usize;
            let _relocs_ptr = u32::from_le_bytes(bytes[s_off + 24..s_off + 28].try_into().unwrap()) as usize;
            let _num_relocs = u16::from_le_bytes(bytes[s_off + 32..s_off + 34].try_into().unwrap()) as usize;
            let characteristics = u32::from_le_bytes(bytes[s_off + 36..s_off + 40].try_into().unwrap());

            let kind = if (characteristics & IMAGE_SCN_CNT_CODE) != 0 || name.starts_with(".text") {
                SectionKind::Text
            } else if (characteristics & IMAGE_SCN_CNT_UNINITIALIZED_DATA) != 0 || name.starts_with(".bss") {
                SectionKind::Bss
            } else if (characteristics & IMAGE_SCN_MEM_WRITE) != 0 {
                SectionKind::Data
            } else if name == ".adesh.meta" {
                SectionKind::AdeshMeta
            } else {
                SectionKind::Rodata
            };

            let mut sec_flags = flags::READ | flags::ALLOC;
            if (characteristics & IMAGE_SCN_MEM_WRITE) != 0 { sec_flags |= flags::WRITE; }
            if (characteristics & IMAGE_SCN_MEM_EXECUTE) != 0 { sec_flags |= flags::EXEC; }

            let data = if kind != SectionKind::Bss && raw_data_ptr + raw_data_size <= bytes.len() {
                bytes[raw_data_ptr..raw_data_ptr + raw_data_size].to_vec()
            } else {
                Vec::new()
            };

            let sec = Section {
                name,
                kind,
                flags: sec_flags,
                alignment: 16,
                virtual_address: 0,
                file_offset: raw_data_ptr as u64,
                size: if virtual_size > 0 { virtual_size } else { raw_data_size as u64 },
                data,
                relocations: Vec::new(),
                comdat_group: None,
                file_index: Some(file_index),
                is_live: true,
                is_folded: false,
                folded_into: None,
            };

            obj.add_section(sec);
        }

        // Parse COFF symbol table
        let mut raw_symbol_names = vec![String::new(); num_symbols];
        if sym_ptr > 0 && sym_ptr + num_symbols * 18 <= bytes.len() {
            let mut s_idx = 0;
            while s_idx < num_symbols {
                let off = sym_ptr + s_idx * 18;
                let mut name_raw = [0u8; 8];
                name_raw.copy_from_slice(&bytes[off..off + 8]);

                let sym_name = if name_raw[0] == 0 && name_raw[1] == 0 && name_raw[2] == 0 && name_raw[3] == 0 {
                    let st_off = u32::from_le_bytes(name_raw[4..8].try_into().unwrap()) as usize;
                    if st_off < string_table.len() {
                        let end = string_table[st_off..].iter().position(|&b| b == 0).map(|p| st_off + p).unwrap_or(string_table.len());
                        String::from_utf8_lossy(&string_table[st_off..end]).to_string()
                    } else {
                        String::new()
                    }
                } else {
                    get_string(&name_raw)
                };

                let value = u32::from_le_bytes(bytes[off + 8..off + 12].try_into().unwrap()) as u64;
                let sec_num = i16::from_le_bytes(bytes[off + 12..off + 14].try_into().unwrap());
                let storage_class = bytes[off + 16];
                let num_aux = bytes[off + 17] as usize;

                let is_defined = sec_num > 0;
                let sec_idx = if is_defined && (sec_num as usize) <= obj.sections.len() {
                    Some((sec_num as usize) - 1)
                } else {
                    None
                };

                let binding = match storage_class {
                    2 => SymbolBinding::Global, // IMAGE_SYM_CLASS_EXTERNAL
                    3 => SymbolBinding::Local,  // IMAGE_SYM_CLASS_STATIC
                    105 => SymbolBinding::Weak, // IMAGE_SYM_CLASS_WEAK_EXTERNAL
                    _ => SymbolBinding::Global,
                };

                let sym = Symbol {
                    name: sym_name.clone(),
                    binding,
                    visibility: SymbolVisibility::Default,
                    sym_type: SymbolType::Function,
                    section_index: sec_idx,
                    value,
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

                raw_symbol_names[s_idx] = sym_name.clone();
                for aux_i in 1..=num_aux {
                    if s_idx + aux_i < num_symbols {
                        raw_symbol_names[s_idx + aux_i] = sym_name.clone();
                    }
                }

                s_idx += 1 + num_aux; // Skip auxiliary symbol records
            }
        }

        // Parse relocations for each section
        for (i, sec) in obj.sections.iter_mut().enumerate() {
            let s_off = section_table_offset + i * section_entry_size;
            let relocs_ptr = u32::from_le_bytes(bytes[s_off + 24..s_off + 28].try_into().unwrap()) as usize;
            let num_relocs = u16::from_le_bytes(bytes[s_off + 32..s_off + 34].try_into().unwrap()) as usize;

            if num_relocs > 0 && relocs_ptr > 0 && relocs_ptr + num_relocs * 10 <= bytes.len() {
                for r_i in 0..num_relocs {
                    let r_off = relocs_ptr + r_i * 10;
                    let vaddr = u32::from_le_bytes(bytes[r_off..r_off + 4].try_into().unwrap()) as u64;
                    let sym_idx = u32::from_le_bytes(bytes[r_off + 4..r_off + 8].try_into().unwrap()) as usize;
                    let reloc_type = u16::from_le_bytes(bytes[r_off + 8..r_off + 10].try_into().unwrap());

                    let sym_name = if sym_idx < raw_symbol_names.len() {
                        raw_symbol_names[sym_idx].clone()
                    } else {
                        String::new()
                    };

                    let v_usize = vaddr as usize;

                    let (kind, addend) = match (arch, reloc_type) {
                        (Arch::X86_64, 0x0001) => {
                            // IMAGE_REL_AMD64_ADDR64
                            let add = if v_usize + 8 <= sec.data.len() {
                                i64::from_le_bytes(sec.data[v_usize..v_usize + 8].try_into().unwrap())
                            } else {
                                0
                            };
                            (crate::relocation::RelocationKind::Absolute64, add)
                        }
                        (Arch::X86_64, 0x0002) | (Arch::X86_64, 0x0003) | (Arch::X86_64, 0x000B) => {
                            // ADDR32 / ADDR32NB / SECREL
                            let add = if v_usize + 4 <= sec.data.len() {
                                i32::from_le_bytes(sec.data[v_usize..v_usize + 4].try_into().unwrap()) as i64
                            } else {
                                0
                            };
                            (crate::relocation::RelocationKind::Absolute32, add)
                        }
                        (Arch::X86_64, 0x0004) => {
                            // IMAGE_REL_AMD64_REL32 (PC-relative call/jmp/mov)
                            let add = if v_usize + 4 <= sec.data.len() {
                                i32::from_le_bytes(sec.data[v_usize..v_usize + 4].try_into().unwrap()) as i64
                            } else {
                                -4
                            };
                            (crate::relocation::RelocationKind::PcRelative32, add)
                        }
                        (Arch::X86_64, 0x0005..=0x0009) => {
                            // IMAGE_REL_AMD64_REL32_1.._5
                            let sub = (reloc_type - 4) as i64;
                            let add = if v_usize + 4 <= sec.data.len() {
                                i32::from_le_bytes(sec.data[v_usize..v_usize + 4].try_into().unwrap()) as i64
                            } else {
                                -4 - sub
                            };
                            (crate::relocation::RelocationKind::PcRelative32, add)
                        }
                        (Arch::X86_64, 0x000E) => {
                            // IMAGE_REL_AMD64_PCR32
                            let add = if v_usize + 4 <= sec.data.len() {
                                i32::from_le_bytes(sec.data[v_usize..v_usize + 4].try_into().unwrap()) as i64
                            } else {
                                -4
                            };
                            (crate::relocation::RelocationKind::PcRelative32, add)
                        }
                        (Arch::AArch64, 0x0001) | (Arch::AArch64, 0x0002) | (Arch::AArch64, 0x0008) => {
                            (crate::relocation::RelocationKind::Absolute32, 0)
                        }
                        (Arch::AArch64, 0x000E) => {
                            (crate::relocation::RelocationKind::Absolute64, 0)
                        }
                        (Arch::AArch64, 0x0003) => {
                            (crate::relocation::RelocationKind::AArch64Call26, 0)
                        }
                        (Arch::AArch64, 0x0004) | (Arch::AArch64, 0x0005) => {
                            (crate::relocation::RelocationKind::AArch64Adrp, 0)
                        }
                        (Arch::AArch64, 0x0006) | (Arch::AArch64, 0x0007) => {
                            (crate::relocation::RelocationKind::AArch64AddLo12, 0)
                        }
                        (Arch::X86, 0x0006) | (Arch::X86, 0x0007) => {
                            let add = if v_usize + 4 <= sec.data.len() {
                                i32::from_le_bytes(sec.data[v_usize..v_usize + 4].try_into().unwrap()) as i64
                            } else {
                                0
                            };
                            (crate::relocation::RelocationKind::Absolute32, add)
                        }
                        (Arch::X86, 0x0014) => {
                            let add = if v_usize + 4 <= sec.data.len() {
                                i32::from_le_bytes(sec.data[v_usize..v_usize + 4].try_into().unwrap()) as i64
                            } else {
                                -4
                            };
                            (crate::relocation::RelocationKind::PcRelative32, add)
                        }
                        _ => (crate::relocation::RelocationKind::Absolute64, 0),
                    };

                    if !sym_name.is_empty() {
                        sec.relocations.push(crate::relocation::Relocation::new(
                            vaddr,
                            sym_name,
                            kind,
                            addend,
                        ));
                    }
                }
            }
        }

        // Check for Adesh metadata section
        if let Some(meta_sec) = obj.find_section(".adesh.meta") {
            if let Ok(meta) = crate::metadata::AdeshMetadata::decode(&meta_sec.data) {
                obj.metadata = Some(meta);
            }
        }

        obj.validate()?;
        Ok(obj)
    }
}
