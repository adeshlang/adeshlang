//! PE / COFF Object File Reader.

use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::object::ObjectFile;
use crate::pe::header::*;
use crate::section::{Section, SectionKind, flags};
use crate::symbol::{Symbol, SymbolBinding, SymbolType, SymbolVisibility};
use crate::target::{Arch, Endianness, ObjectFormat, Os, PointerWidth, Target};
use std::path::Path;

pub struct PeReader;

impl PeReader {
    pub fn read(bytes: &[u8], path: &Path, file_index: usize) -> LinkResult<ObjectFile> {
        if bytes.len() < 20 {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                format!(
                    "COFF object `{}` is too small ({} bytes)",
                    path.display(),
                    bytes.len()
                ),
            ));
        }

        // Check if COFF Short Import Header (Sig1 = 0, Sig2 = 0xFFFF)
        if bytes.len() >= 20
            && bytes[0] == 0
            && bytes[1] == 0
            && bytes[2] == 0xFF
            && bytes[3] == 0xFF
        {
            let machine_val = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
            let arch = match machine_val {
                IMAGE_FILE_MACHINE_AMD64 => Arch::X86_64,
                IMAGE_FILE_MACHINE_ARM64 => Arch::AArch64,
                IMAGE_FILE_MACHINE_I386 => Arch::X86,
                _ => Arch::X86_64,
            };
            let target = Target::from_triple(if arch == Arch::AArch64 {
                "aarch64-pc-windows-msvc"
            } else {
                "x86_64-pc-windows-msvc"
            })
            .unwrap_or_else(|_| Target::host());
            let size_of_data = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
            if 20 + size_of_data <= bytes.len() {
                let data = &bytes[20..20 + size_of_data];
                let parts: Vec<&[u8]> = data.split(|&b| b == 0).collect();
                if !parts.is_empty() {
                    let sym_name = String::from_utf8_lossy(parts[0]).to_string();
                    let mut obj = ObjectFile::new(path.to_path_buf(), target, file_index);
                    obj.add_symbol(Symbol {
                        name: sym_name.clone(),
                        binding: SymbolBinding::Global,
                        visibility: SymbolVisibility::Default,
                        sym_type: SymbolType::Function,
                        section_index: None,
                        value: 0,
                        size: 0,
                        is_defined: false,
                        is_imported: true,
                        is_exported: false,
                        file_index: Some(file_index),
                        alias_of: None,
                        comdat_group: None,
                        version: None,
                    });
                    obj.add_symbol(Symbol {
                        name: format!("__imp_{}", sym_name),
                        binding: SymbolBinding::Global,
                        visibility: SymbolVisibility::Default,
                        sym_type: SymbolType::Object,
                        section_index: None,
                        value: 0,
                        size: 8,
                        is_defined: false,
                        is_imported: true,
                        is_exported: false,
                        file_index: Some(file_index),
                        alias_of: None,
                        comdat_group: None,
                        version: None,
                    });
                    return Ok(obj);
                }
            }
        }

        let mut offset = 0;
        // Check if MS-DOS header present
        if &bytes[0..2] == b"MZ" {
            if bytes.len() < 0x40 {
                return Err(LinkError::new(
                    ErrorCode::InvalidObject,
                    "truncated DOS header",
                ));
            }
            let lfanew = u32::from_le_bytes(bytes[0x3C..0x40].try_into().unwrap()) as usize;
            if lfanew + 24 > bytes.len() || &bytes[lfanew..lfanew + 4] != &PE_SIGNATURE {
                return Err(LinkError::new(
                    ErrorCode::InvalidObject,
                    "invalid PE signature in MS-DOS binary",
                ));
            }
            offset = lfanew + 4;
        }

        let machine = u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
        let num_sections =
            u16::from_le_bytes(bytes[offset + 2..offset + 4].try_into().unwrap()) as usize;
        let sym_ptr =
            u32::from_le_bytes(bytes[offset + 8..offset + 12].try_into().unwrap()) as usize;
        let num_symbols =
            u32::from_le_bytes(bytes[offset + 12..offset + 16].try_into().unwrap()) as usize;
        let opt_hdr_size =
            u16::from_le_bytes(bytes[offset + 16..offset + 18].try_into().unwrap()) as usize;

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
            pointer_width: if arch == Arch::X86 {
                PointerWidth::U32
            } else {
                PointerWidth::U64
            },
            endianness: Endianness::Little,
            relocation_model: crate::target::RelocationModel::Static,
            page_size: 4096,
            image_base: if arch == Arch::X86 {
                0x00400000
            } else {
                0x140000000
            },
            default_entry: "mainCRTStartup".to_string(),
        };

        // COFF does not record an alignment for COMMON (tentative) definitions;
        // the natural pointer width is the conservative default.
        let common_alignment: u64 = if arch == Arch::X86 { 4 } else { 8 };

        let mut obj = ObjectFile::new(path.to_path_buf(), target.clone(), file_index);

        let section_table_offset = offset + 20 + opt_hdr_size;
        let section_entry_size = 40;

        if section_table_offset + num_sections * section_entry_size > bytes.len() {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                format!(
                    "COFF section table in `{}` exceeds file bounds",
                    path.display()
                ),
            ));
        }

        // Per-section COMDAT metadata, indexed by COFF section number - 1.
        // `comdat_select` holds the Selection field of the section symbol's
        // auxiliary record; `comdat_assoc` holds the associated section for
        // IMAGE_COMDAT_SELECT_ASSOCIATIVE sections.
        let mut comdat_select: Vec<Option<u8>> = vec![None; num_sections];
        let mut comdat_assoc: Vec<Option<usize>> = vec![None; num_sections];

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
                            let end = string_table[st_off..]
                                .iter()
                                .position(|&b| b == 0)
                                .map(|p| st_off + p)
                                .unwrap_or(string_table.len());
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

            let virtual_size =
                u32::from_le_bytes(bytes[s_off + 8..s_off + 12].try_into().unwrap()) as u64;
            let virtual_address =
                u32::from_le_bytes(bytes[s_off + 12..s_off + 16].try_into().unwrap()) as u64;
            let raw_data_size =
                u32::from_le_bytes(bytes[s_off + 16..s_off + 20].try_into().unwrap()) as usize;
            let raw_data_ptr =
                u32::from_le_bytes(bytes[s_off + 20..s_off + 24].try_into().unwrap()) as usize;
            let _relocs_ptr =
                u32::from_le_bytes(bytes[s_off + 24..s_off + 28].try_into().unwrap()) as usize;
            let _num_relocs =
                u16::from_le_bytes(bytes[s_off + 32..s_off + 34].try_into().unwrap()) as usize;
            let characteristics =
                u32::from_le_bytes(bytes[s_off + 36..s_off + 40].try_into().unwrap());

            let kind = if name.starts_with(".tls") {
                if (characteristics & IMAGE_SCN_CNT_UNINITIALIZED_DATA) != 0 {
                    SectionKind::TBss
                } else {
                    SectionKind::TData
                }
            } else if (characteristics & IMAGE_SCN_CNT_CODE) != 0 || name.starts_with(".text") {
                SectionKind::Text
            } else if (characteristics & IMAGE_SCN_CNT_UNINITIALIZED_DATA) != 0
                || name.starts_with(".bss")
            {
                SectionKind::Bss
            } else if (characteristics & IMAGE_SCN_MEM_WRITE) != 0 {
                SectionKind::Data
            } else if name == ".adesh.meta" {
                SectionKind::AdeshMeta
            } else {
                SectionKind::Rodata
            };

            let mut sec_flags = flags::READ | flags::ALLOC;
            if (characteristics & IMAGE_SCN_MEM_WRITE) != 0 {
                sec_flags |= flags::WRITE;
            }
            if (characteristics & IMAGE_SCN_MEM_EXECUTE) != 0 {
                sec_flags |= flags::EXEC;
            }
            if matches!(kind, SectionKind::TData | SectionKind::TBss) {
                sec_flags |= flags::TLS | flags::WRITE;
            }
            if (characteristics & IMAGE_SCN_LNK_COMDAT) != 0 {
                // Marked so the resolver can fold duplicate COMDAT
                // definitions (inline functions, templates, vftables)
                // instead of reporting a duplicate-symbol error.
                sec_flags |= flags::COMDAT;
            }

            let data = if !matches!(kind, SectionKind::Bss | SectionKind::TBss)
                && raw_data_ptr + raw_data_size <= bytes.len()
            {
                bytes[raw_data_ptr..raw_data_ptr + raw_data_size].to_vec()
            } else {
                Vec::new()
            };

            let sec = Section {
                name,
                kind,
                flags: sec_flags,
                alignment: 16,
                virtual_address,
                file_offset: raw_data_ptr as u64,
                size: if virtual_size > 0 {
                    virtual_size
                } else {
                    raw_data_size as u64
                },
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

        // Parse COFF symbol table.
        //
        // `obj.symbols` MUST stay index-aligned with the raw COFF symbol
        // table: relocations reference symbols by raw symbol-table index, and
        // LLVM COFF objects contain many same-named sections (`.text`,
        // `.rdata`, ...) whose symbols are only distinguishable by index.
        // Aux records get placeholder entries so that
        // `obj.symbols[raw_index]` is always the referenced symbol.
        let mut raw_symbol_names = vec![String::new(); num_symbols];
        if sym_ptr > 0 && sym_ptr + num_symbols * 18 <= bytes.len() {
            let mut s_idx = 0;
            while s_idx < num_symbols {
                let off = sym_ptr + s_idx * 18;
                let mut name_raw = [0u8; 8];
                name_raw.copy_from_slice(&bytes[off..off + 8]);

                let sym_name =
                    if name_raw[0] == 0 && name_raw[1] == 0 && name_raw[2] == 0 && name_raw[3] == 0
                    {
                        let st_off =
                            u32::from_le_bytes(name_raw[4..8].try_into().unwrap()) as usize;
                        if st_off < string_table.len() {
                            let end = string_table[st_off..]
                                .iter()
                                .position(|&b| b == 0)
                                .map(|p| st_off + p)
                                .unwrap_or(string_table.len());
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

                // A tentative definition (`COMMON` block) is encoded as an
                // external symbol with section number 0 whose Value field
                // holds the requested size. Section number 0 with Value 0 is a
                // genuine undefined reference.
                let is_common =
                    storage_class == IMAGE_SYM_CLASS_EXTERNAL && sec_num == 0 && value > 0;

                let is_defined = sec_num > 0 || is_common;
                let sec_idx = if sec_num > 0 && (sec_num as usize) <= obj.sections.len() {
                    Some((sec_num as usize) - 1)
                } else {
                    None
                };

                let binding = match storage_class {
                    IMAGE_SYM_CLASS_EXTERNAL => SymbolBinding::Global,
                    IMAGE_SYM_CLASS_STATIC => SymbolBinding::Local,
                    IMAGE_SYM_CLASS_WEAK_EXTERNAL => SymbolBinding::Weak,
                    _ => SymbolBinding::Global,
                };

                // COMDAT section symbols carry the selection type (and, for
                // associative sections, the associated section number) in
                // their auxiliary record.
                if storage_class == IMAGE_SYM_CLASS_STATIC
                    && sec_num > 0
                    && value == 0
                    && num_aux >= 1
                {
                    let s0 = (sec_num as usize) - 1;
                    let aux_off = off + 18;
                    if s0 < comdat_select.len() && aux_off + 18 <= bytes.len() {
                        let selection = bytes[aux_off];
                        comdat_select[s0] = Some(selection);
                        if selection == IMAGE_COMDAT_SELECT_ASSOCIATIVE {
                            let assoc = u16::from_le_bytes(
                                bytes[aux_off + 2..aux_off + 4].try_into().unwrap(),
                            );
                            if assoc > 0 {
                                comdat_assoc[s0] = Some((assoc as usize) - 1);
                            }
                        }
                    }
                }

                let sym = Symbol {
                    name: sym_name.clone(),
                    binding,
                    visibility: SymbolVisibility::Default,
                    sym_type: if is_common {
                        SymbolType::Common
                    } else {
                        SymbolType::Function
                    },
                    section_index: sec_idx,
                    // For a COMMON block the reader repurposes `value` as the
                    // required alignment (the layout engine allocates the block
                    // in .bss) and keeps the requested size in `size`.
                    value: if is_common { common_alignment } else { value },
                    size: if is_common { value } else { 0 },
                    is_defined,
                    is_imported: false,
                    is_exported: false,
                    file_index: Some(file_index),
                    alias_of: None,
                    comdat_group: None,
                    version: None,
                };

                obj.add_symbol(sym);

                raw_symbol_names[s_idx] = sym_name.clone();

                // Placeholder slots for auxiliary records so that raw symbol
                // indices stay aligned with `obj.symbols`.
                for aux_i in 1..=num_aux {
                    if s_idx + aux_i < num_symbols {
                        raw_symbol_names[s_idx + aux_i] = sym_name.clone();
                        obj.add_symbol(Symbol {
                            name: String::new(),
                            binding: SymbolBinding::Local,
                            visibility: SymbolVisibility::Default,
                            sym_type: SymbolType::Function,
                            section_index: None,
                            value: 0,
                            size: 0,
                            is_defined: false,
                            is_imported: false,
                            is_exported: false,
                            file_index: Some(file_index),
                            alias_of: None,
                            comdat_group: None,
                            version: None,
                        });
                    }
                }

                s_idx += 1 + num_aux; // Skip auxiliary symbol records
            }
        }

        // If this is a PE executable/DLL with an Export Directory, parse exported symbols
        if &bytes[0..2] == b"MZ" && opt_hdr_size >= 120 {
            let opt_off = offset + 20;
            let exp_rva_off = opt_off + 112 + IMAGE_DIRECTORY_ENTRY_EXPORT * 8;
            if exp_rva_off + 8 <= bytes.len() {
                let exp_rva =
                    u32::from_le_bytes(bytes[exp_rva_off..exp_rva_off + 4].try_into().unwrap())
                        as usize;
                let exp_size =
                    u32::from_le_bytes(bytes[exp_rva_off + 4..exp_rva_off + 8].try_into().unwrap())
                        as usize;
                if exp_rva > 0 && exp_size > 0 {
                    let mut exported_symbols = Vec::new();
                    for s in &obj.sections {
                        let sec_va = s.virtual_address as usize;
                        let sec_sz = (s.size as usize).max(s.data.len());
                        if exp_rva >= sec_va && exp_rva < sec_va + sec_sz {
                            let file_off = (s.file_offset as usize) + (exp_rva - sec_va);
                            let rva_to_file = |rva: usize| -> Option<usize> {
                                for sec in &obj.sections {
                                    let s_va = sec.virtual_address as usize;
                                    let s_sz = (sec.size as usize).max(sec.data.len());
                                    if rva >= s_va && rva < s_va + s_sz {
                                        return Some((sec.file_offset as usize) + (rva - s_va));
                                    }
                                }
                                None
                            };

                            if file_off + 40 <= bytes.len() {
                                let num_names = u32::from_le_bytes(
                                    bytes[file_off + 24..file_off + 28].try_into().unwrap(),
                                ) as usize;
                                let addr_funcs = u32::from_le_bytes(
                                    bytes[file_off + 28..file_off + 32].try_into().unwrap(),
                                ) as usize;
                                let addr_names = u32::from_le_bytes(
                                    bytes[file_off + 32..file_off + 36].try_into().unwrap(),
                                ) as usize;
                                let addr_ords = u32::from_le_bytes(
                                    bytes[file_off + 36..file_off + 40].try_into().unwrap(),
                                ) as usize;

                                if let (Some(names_off), Some(funcs_off), Some(ords_off)) = (
                                    rva_to_file(addr_names),
                                    rva_to_file(addr_funcs),
                                    rva_to_file(addr_ords),
                                ) {
                                    for i in 0..num_names {
                                        if names_off + (i + 1) * 4 <= bytes.len()
                                            && ords_off + (i + 1) * 2 <= bytes.len()
                                        {
                                            let name_rva = u32::from_le_bytes(
                                                bytes[names_off + i * 4..names_off + (i + 1) * 4]
                                                    .try_into()
                                                    .unwrap(),
                                            )
                                                as usize;
                                            let ord = u16::from_le_bytes(
                                                bytes[ords_off + i * 2..ords_off + (i + 1) * 2]
                                                    .try_into()
                                                    .unwrap(),
                                            )
                                                as usize;
                                            let func_rva =
                                                if funcs_off + (ord + 1) * 4 <= bytes.len() {
                                                    u32::from_le_bytes(
                                                        bytes[funcs_off + ord * 4
                                                            ..funcs_off + (ord + 1) * 4]
                                                            .try_into()
                                                            .unwrap(),
                                                    )
                                                        as u64
                                                } else {
                                                    0
                                                };

                                            if let Some(name_file_off) = rva_to_file(name_rva) {
                                                let end = bytes[name_file_off..]
                                                    .iter()
                                                    .position(|&b| b == 0)
                                                    .unwrap_or(0);
                                                let exp_name = String::from_utf8_lossy(
                                                    &bytes[name_file_off..name_file_off + end],
                                                )
                                                .to_string();
                                                if !exp_name.is_empty() {
                                                    exported_symbols.push(Symbol {
                                                        name: exp_name,
                                                        binding: SymbolBinding::Global,
                                                        visibility: SymbolVisibility::Default,
                                                        sym_type: SymbolType::Function,
                                                        section_index: None,
                                                        value: func_rva + target.image_base,
                                                        size: 0,
                                                        is_defined: true,
                                                        is_imported: false,
                                                        is_exported: true,
                                                        file_index: Some(file_index),
                                                        alias_of: None,
                                                        comdat_group: None,
                                                        version: None,
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            break;
                        }
                    }

                    for sym in exported_symbols {
                        if !obj.symbols.iter().any(|s| s.name == sym.name) {
                            obj.add_symbol(sym);
                        }
                    }
                }
            }
        }

        // Resolve the COMDAT group key of every IMAGE_SCN_LNK_COMDAT section.
        //
        // The key is the section's leader symbol (the external symbol defined
        // at offset 0). Associative COMDAT sections inherit their associated
        // section's key, because they are discarded together with it and are
        // not independent duplicates. Recording the key lets
        // `SymbolResolver` fold repeated definitions (inline functions,
        // templates, vftables) across objects.
        //
        // Sections WITHOUT an external leader symbol must NOT fall back to
        // the section name: LLVM emits many per-function switch tables and
        // per-static sections all literally named `.rdata`/`.data` (COMDAT,
        // NoDuplicates, only local alias symbols like `switch.table.f.rel`).
        // Keying those by name made every one of them collide across all
        // objects, so all but the first were discarded as "duplicates" and
        // their relocations failed with LNK001/LNK009. Without an external
        // leader there is nothing to deduplicate on, so such sections get NO
        // comdat group and are always placed.
        {
            let mut group_keys: Vec<Option<String>> = vec![None; obj.sections.len()];
            for (i, sec) in obj.sections.iter().enumerate() {
                if (sec.flags & flags::COMDAT) == 0 {
                    continue;
                }
                if comdat_select.get(i).copied().flatten() == Some(IMAGE_COMDAT_SELECT_ASSOCIATIVE)
                    && let Some(assoc) = comdat_assoc.get(i).copied().flatten()
                    && let Some(Some(key)) = group_keys.get(assoc)
                {
                    group_keys[i] = Some(key.clone());
                    continue;
                }
                let leader = obj.symbols.iter().find(|s| {
                    s.section_index == Some(i)
                        && s.is_defined
                        && !s.name.is_empty()
                        && s.value == 0
                        && matches!(s.binding, SymbolBinding::Global | SymbolBinding::Weak)
                });
                group_keys[i] = match leader {
                    Some(s) => Some(s.name.clone()),
                    None => None,
                };
            }

            for (i, key) in group_keys.into_iter().enumerate() {
                let Some(key) = key else { continue };
                obj.sections[i].comdat_group = Some(key.clone());
                for sym in obj.symbols.iter_mut() {
                    if sym.section_index == Some(i) {
                        sym.comdat_group = Some(key.clone());
                    }
                }
            }
        }

        // Parse relocations for each section
        for (i, sec) in obj.sections.iter_mut().enumerate() {
            let s_off = section_table_offset + i * section_entry_size;
            let relocs_ptr =
                u32::from_le_bytes(bytes[s_off + 24..s_off + 28].try_into().unwrap()) as usize;
            let num_relocs =
                u16::from_le_bytes(bytes[s_off + 32..s_off + 34].try_into().unwrap()) as usize;

            if num_relocs > 0 && relocs_ptr > 0 && relocs_ptr + num_relocs * 10 <= bytes.len() {
                for r_i in 0..num_relocs {
                    let r_off = relocs_ptr + r_i * 10;
                    let vaddr =
                        u32::from_le_bytes(bytes[r_off..r_off + 4].try_into().unwrap()) as u64;
                    let sym_idx =
                        u32::from_le_bytes(bytes[r_off + 4..r_off + 8].try_into().unwrap())
                            as usize;
                    let reloc_type =
                        u16::from_le_bytes(bytes[r_off + 8..r_off + 10].try_into().unwrap());

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
                                i64::from_le_bytes(
                                    sec.data[v_usize..v_usize + 8].try_into().unwrap(),
                                )
                            } else {
                                0
                            };
                            (crate::relocation::RelocationKind::Absolute64, add)
                        }
                        (Arch::X86_64, 0x0002) => {
                            // IMAGE_REL_AMD64_ADDR32: full 32-bit absolute
                            let add = if v_usize + 4 <= sec.data.len() {
                                i32::from_le_bytes(
                                    sec.data[v_usize..v_usize + 4].try_into().unwrap(),
                                ) as i64
                            } else {
                                0
                            };
                            (crate::relocation::RelocationKind::Absolute32, add)
                        }
                        (Arch::X86_64, 0x0003) => {
                            // IMAGE_REL_AMD64_ADDR32NB: image-base-relative (RVA)
                            let add = if v_usize + 4 <= sec.data.len() {
                                i32::from_le_bytes(
                                    sec.data[v_usize..v_usize + 4].try_into().unwrap(),
                                ) as i64
                            } else {
                                0
                            };
                            (crate::relocation::RelocationKind::ImageRelative32, add)
                        }
                        (Arch::X86_64, 0x000B) => {
                            // IMAGE_REL_AMD64_SECREL32: offset of the target
                            // within its own section
                            let add = if v_usize + 4 <= sec.data.len() {
                                i32::from_le_bytes(
                                    sec.data[v_usize..v_usize + 4].try_into().unwrap(),
                                ) as i64
                            } else {
                                0
                            };
                            (crate::relocation::RelocationKind::SectionRelative32, add)
                        }
                        (Arch::X86_64, 0x0004) => {
                            // IMAGE_REL_AMD64_REL32 (PC-relative call/jmp/mov)
                            // In COFF x86_64, displacement is relative to (place_va + 4).
                            let embedded = if v_usize + 4 <= sec.data.len() {
                                i32::from_le_bytes(
                                    sec.data[v_usize..v_usize + 4].try_into().unwrap(),
                                ) as i64
                            } else {
                                0
                            };
                            (
                                crate::relocation::RelocationKind::PcRelative32,
                                embedded - 4,
                            )
                        }
                        (Arch::X86_64, 0x0005..=0x0009) => {
                            // IMAGE_REL_AMD64_REL32_1.._5 (displacement is relative to place_va + 4 + distance)
                            let sub = (reloc_type - 4) as i64;
                            let embedded = if v_usize + 4 <= sec.data.len() {
                                i32::from_le_bytes(
                                    sec.data[v_usize..v_usize + 4].try_into().unwrap(),
                                ) as i64
                            } else {
                                0
                            };
                            (
                                crate::relocation::RelocationKind::PcRelative32,
                                embedded - 4 - sub,
                            )
                        }
                        (Arch::X86_64, 0x000E) => {
                            // IMAGE_REL_AMD64_PCR32
                            let embedded = if v_usize + 4 <= sec.data.len() {
                                i32::from_le_bytes(
                                    sec.data[v_usize..v_usize + 4].try_into().unwrap(),
                                ) as i64
                            } else {
                                0
                            };
                            (
                                crate::relocation::RelocationKind::PcRelative32,
                                embedded - 4,
                            )
                        }
                        (Arch::AArch64, 0x0001)
                        | (Arch::AArch64, 0x0002)
                        | (Arch::AArch64, 0x0008) => {
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
                                i32::from_le_bytes(
                                    sec.data[v_usize..v_usize + 4].try_into().unwrap(),
                                ) as i64
                            } else {
                                0
                            };
                            (crate::relocation::RelocationKind::Absolute32, add)
                        }
                        (Arch::X86, 0x0014) => {
                            // IMAGE_REL_I386_REL32: the embedded displacement is
                            // relative to the *end* of the 4-byte field, and the
                            // handler computes `S + A - P`, so the addend must
                            // carry the -4 (exactly like IMAGE_REL_AMD64_REL32).
                            let embedded = if v_usize + 4 <= sec.data.len() {
                                i32::from_le_bytes(
                                    sec.data[v_usize..v_usize + 4].try_into().unwrap(),
                                ) as i64
                            } else {
                                0
                            };
                            (
                                crate::relocation::RelocationKind::PcRelative32,
                                embedded - 4,
                            )
                        }
                        _ => (crate::relocation::RelocationKind::Absolute64, 0),
                    };

                    // Record the raw COFF symbol-table index so the layout
                    // engine can resolve the exact referenced symbol: LLVM
                    // COFF objects contain many same-named sections whose
                    // symbols (e.g. the `.text`/`.rdata` section symbols
                    // referenced by jump tables) collide under name-based
                    // resolution.
                    let mut reloc_rec =
                        crate::relocation::Relocation::new(vaddr, sym_name, kind, addend);
                    reloc_rec.symbol_index = Some(sym_idx);
                    sec.relocations.push(reloc_rec);
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
