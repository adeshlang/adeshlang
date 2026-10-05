//! ELF Object File Reader.

use crate::elf::header::*;
use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::object::ObjectFile;
use crate::relocation::{Relocation, RelocationKind};
use crate::section::{Section, SectionKind, flags};
use crate::symbol::{Symbol, SymbolBinding, SymbolType, SymbolVisibility};
use crate::target::{Arch, Endianness, ObjectFormat, Os, PointerWidth, Target};
use std::path::Path;

pub struct ElfReader;

impl ElfReader {
    pub fn read(bytes: &[u8], path: &Path, file_index: usize) -> LinkResult<ObjectFile> {
        if bytes.len() < 64 {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                format!(
                    "ELF file `{}` is too small for ELF header ({} bytes)",
                    path.display(),
                    bytes.len()
                ),
            ));
        }

        if bytes[0..4] != ELF_MAGIC {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                format!("ELF file `{}` has invalid magic", path.display()),
            ));
        }

        let is_64 = bytes[4] == ELFCLASS64;
        let is_little = bytes[5] == ELFDATA2LSB;
        if !is_little {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                "Big-endian ELF parsing is currently unsupported",
            ));
        }

        let e_machine = u16::from_le_bytes(bytes[18..20].try_into().unwrap());
        let arch = match e_machine {
            EM_X86_64 => Arch::X86_64,
            EM_AARCH64 => Arch::AArch64,
            EM_ARM => Arch::Arm,
            EM_RISCV => {
                if is_64 {
                    Arch::Riscv64
                } else {
                    Arch::Riscv32
                }
            }
            EM_386 => Arch::X86,
            other => {
                return Err(LinkError::new(
                    ErrorCode::ArchitectureMismatch,
                    format!("unsupported ELF machine type: 0x{:x} ({})", other, other),
                ));
            }
        };

        let target = Target {
            arch,
            os: Os::Linux,
            format: ObjectFormat::Elf,
            abi: crate::target::Abi::SystemV,
            pointer_width: if is_64 {
                PointerWidth::U64
            } else {
                PointerWidth::U32
            },
            endianness: Endianness::Little,
            relocation_model: crate::target::RelocationModel::Static,
            page_size: if arch == Arch::AArch64 { 65536 } else { 4096 },
            image_base: if is_64 { 0x400000 } else { 0x08048000 },
            default_entry: "_start".to_string(),
        };

        let mut obj = ObjectFile::new(path.to_path_buf(), target, file_index);

        let shoff = u64::from_le_bytes(bytes[40..48].try_into().unwrap()) as usize;
        let shentsize = u16::from_le_bytes(bytes[58..60].try_into().unwrap()) as usize;
        let shnum = u16::from_le_bytes(bytes[60..62].try_into().unwrap()) as usize;
        let shstrndx = u16::from_le_bytes(bytes[62..64].try_into().unwrap()) as usize;

        if shoff + shnum * shentsize > bytes.len() {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                format!(
                    "ELF section header table in `{}` exceeds file bounds",
                    path.display()
                ),
            ));
        }

        // Parse section headers raw table
        let mut raw_shdrs = Vec::with_capacity(shnum);
        for i in 0..shnum {
            let off = shoff + i * shentsize;
            let sh_name = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap());
            let sh_type = u32::from_le_bytes(bytes[off + 4..off + 8].try_into().unwrap());
            let sh_flags = u64::from_le_bytes(bytes[off + 8..off + 16].try_into().unwrap());
            let sh_addr = u64::from_le_bytes(bytes[off + 16..off + 24].try_into().unwrap());
            let sh_offset = u64::from_le_bytes(bytes[off + 24..off + 32].try_into().unwrap());
            let sh_size = u64::from_le_bytes(bytes[off + 32..off + 40].try_into().unwrap());
            let sh_link = u32::from_le_bytes(bytes[off + 40..off + 44].try_into().unwrap());
            let sh_info = u32::from_le_bytes(bytes[off + 44..off + 48].try_into().unwrap());
            let sh_addralign = u64::from_le_bytes(bytes[off + 48..off + 56].try_into().unwrap());
            let sh_entsize = u64::from_le_bytes(bytes[off + 56..off + 64].try_into().unwrap());

            raw_shdrs.push(Elf64_Shdr {
                sh_name,
                sh_type,
                sh_flags,
                sh_addr,
                sh_offset,
                sh_size,
                sh_link,
                sh_info,
                sh_addralign,
                sh_entsize,
            });
        }

        // Extract section header string table
        let shstr_slice = if shstrndx < raw_shdrs.len() {
            let shstr_hdr = &raw_shdrs[shstrndx];
            let start = shstr_hdr.sh_offset as usize;
            let end = start + shstr_hdr.sh_size as usize;
            if end <= bytes.len() {
                &bytes[start..end]
            } else {
                &[]
            }
        } else {
            &[]
        };

        let get_string = |strtab: &[u8], offset: u32| -> String {
            let off = offset as usize;
            if off >= strtab.len() {
                return String::new();
            }
            let end = strtab[off..]
                .iter()
                .position(|&b| b == 0)
                .map(|p| off + p)
                .unwrap_or(strtab.len());
            String::from_utf8_lossy(&strtab[off..end]).to_string()
        };

        // Create Section objects (mapping ELF section index to Object section index)
        let mut elf_to_obj_sec_map: Vec<Option<usize>> = vec![None; shnum];
        let mut symtab_idx = None;
        let mut strtab_idx = None;
        let mut rela_sections = Vec::new();
        let mut group_sections = Vec::new();

        for (i, shdr) in raw_shdrs.iter().enumerate() {
            if i == 0 || shdr.sh_type == SHT_NULL {
                continue;
            }
            let name = get_string(shstr_slice, shdr.sh_name);

            if shdr.sh_type == SHT_SYMTAB {
                symtab_idx = Some(i);
                strtab_idx = Some(shdr.sh_link as usize);
                continue;
            }

            if shdr.sh_type == SHT_DYNSYM {
                if symtab_idx.is_none() {
                    symtab_idx = Some(i);
                    strtab_idx = Some(shdr.sh_link as usize);
                }
                continue;
            }

            if shdr.sh_type == SHT_RELA || shdr.sh_type == SHT_REL {
                rela_sections.push(i);
                continue;
            }

            // COMDAT group descriptions are link metadata, not content: they
            // must not be merged into the output image as ordinary data.
            if shdr.sh_type == SHT_GROUP {
                group_sections.push(i);
                continue;
            }

            if shdr.sh_type == SHT_STRTAB && i != shstrndx {
                continue;
            }

            let is_tls = (shdr.sh_flags & SHF_TLS) != 0;
            let kind = if is_tls {
                // Thread-local data must keep its TLS section kind so the
                // layout engine routes it to the TLS template (and so the
                // "no runtime TLS descriptor" guard can fire) instead of
                // silently merging it as ordinary writable data.
                if shdr.sh_type == SHT_NOBITS {
                    SectionKind::TBss
                } else {
                    SectionKind::TData
                }
            } else if name.starts_with(".text") || (shdr.sh_flags & SHF_EXECINSTR) != 0 {
                SectionKind::Text
            } else if name.starts_with(".rodata")
                || (shdr.sh_flags & (SHF_WRITE | SHF_ALLOC)) == SHF_ALLOC
            {
                SectionKind::Rodata
            } else if name.starts_with(".data")
                || (shdr.sh_flags & (SHF_WRITE | SHF_ALLOC)) == (SHF_WRITE | SHF_ALLOC)
                    && shdr.sh_type == SHT_PROGBITS
            {
                SectionKind::Data
            } else if name.starts_with(".bss") || shdr.sh_type == SHT_NOBITS {
                SectionKind::Bss
            } else if name == ".adesh.meta" {
                SectionKind::AdeshMeta
            } else if name.starts_with(".debug") {
                SectionKind::Debug
            } else if shdr.sh_type == SHT_NOTE {
                SectionKind::Note
            } else {
                SectionKind::Custom
            };

            let mut sec_flags = 0;
            if (shdr.sh_flags & SHF_ALLOC) != 0 {
                sec_flags |= flags::ALLOC;
            }
            if (shdr.sh_flags & SHF_WRITE) != 0 {
                sec_flags |= flags::WRITE;
            }
            if (shdr.sh_flags & SHF_EXECINSTR) != 0 {
                sec_flags |= flags::EXEC;
            }
            if (shdr.sh_flags & SHF_TLS) != 0 {
                sec_flags |= flags::TLS;
            }
            sec_flags |= flags::READ;

            let data = if shdr.sh_type != SHT_NOBITS {
                let start = shdr.sh_offset as usize;
                let end = start + shdr.sh_size as usize;
                if end <= bytes.len() {
                    bytes[start..end].to_vec()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            let sec = Section {
                name,
                kind,
                flags: sec_flags,
                alignment: shdr.sh_addralign.max(1),
                virtual_address: shdr.sh_addr,
                file_offset: shdr.sh_offset,
                size: shdr.sh_size,
                data,
                relocations: Vec::new(),
                comdat_group: None,
                file_index: Some(file_index),
                is_live: true,
                is_folded: false,
                folded_into: None,
            };

            let obj_sec_idx = obj.add_section(sec);
            elf_to_obj_sec_map[i] = Some(obj_sec_idx);
        }

        // Parse symbols
        let mut raw_symbols = Vec::new();
        if let (Some(sym_sec_idx), Some(str_sec_idx)) = (symtab_idx, strtab_idx) {
            let sym_hdr = &raw_shdrs[sym_sec_idx];
            let str_hdr = &raw_shdrs[str_sec_idx];

            let str_start = str_hdr.sh_offset as usize;
            let str_end = str_start + str_hdr.sh_size as usize;
            let sym_strtab = if str_end <= bytes.len() {
                &bytes[str_start..str_end]
            } else {
                &[]
            };

            let sym_start = sym_hdr.sh_offset as usize;
            let num_syms = (sym_hdr.sh_size / 24) as usize;

            for s_i in 0..num_syms {
                let off = sym_start + s_i * 24;
                if off + 24 > bytes.len() {
                    break;
                }
                let st_name = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap());
                let st_info = bytes[off + 4];
                let _st_other = bytes[off + 5];
                let st_shndx = u16::from_le_bytes(bytes[off + 6..off + 8].try_into().unwrap());
                let st_value = u64::from_le_bytes(bytes[off + 8..off + 16].try_into().unwrap());
                let st_size = u64::from_le_bytes(bytes[off + 16..off + 24].try_into().unwrap());

                let sym_name = get_string(sym_strtab, st_name);
                let bind_raw = st_info >> 4;
                let type_raw = st_info & 0xF;

                let binding = match bind_raw {
                    STB_LOCAL => SymbolBinding::Local,
                    STB_GLOBAL => SymbolBinding::Global,
                    STB_WEAK => SymbolBinding::Weak,
                    _ => SymbolBinding::Global,
                };

                let sym_type = match type_raw {
                    STT_FUNC => SymbolType::Function,
                    STT_OBJECT => SymbolType::Object,
                    STT_SECTION => SymbolType::Section,
                    STT_FILE => SymbolType::File,
                    STT_TLS => SymbolType::Tls,
                    _ => SymbolType::Unknown,
                };

                let is_common = st_shndx == SHN_COMMON;
                let is_defined = (st_shndx != SHN_UNDEF && st_shndx < SHN_LORESERVE) || is_common;
                let mapped_sec_idx =
                    if !is_common && is_defined && (st_shndx as usize) < elf_to_obj_sec_map.len() {
                        elf_to_obj_sec_map[st_shndx as usize]
                    } else {
                        None
                    };

                let sym = Symbol {
                    name: sym_name,
                    binding,
                    visibility: SymbolVisibility::Default,
                    // SHN_COMMON is a tentative definition: `st_value` is the
                    // required alignment and `st_size` the requested size. The
                    // layout engine allocates the block in .bss.
                    sym_type: if is_common {
                        SymbolType::Common
                    } else {
                        sym_type
                    },
                    section_index: mapped_sec_idx,
                    value: st_value,
                    size: st_size,
                    is_defined,
                    is_imported: !is_defined,
                    is_exported: is_defined && binding == SymbolBinding::Global,
                    file_index: Some(file_index),
                    alias_of: None,
                    comdat_group: None,
                    version: None,
                };

                raw_symbols.push(sym.clone());
                if !sym.name.is_empty() || sym.sym_type == SymbolType::Section {
                    obj.add_symbol(sym);
                }
            }
        }

        // Apply SHT_GROUP COMDAT keys to the member sections (and the symbols
        // they define) so duplicate definitions of an inline function or
        // template instantiation can be folded by the resolver instead of
        // being reported as a duplicate-symbol error.
        for g_idx in group_sections {
            let g_hdr = &raw_shdrs[g_idx];
            let start = g_hdr.sh_offset as usize;
            let end = start + g_hdr.sh_size as usize;
            if g_hdr.sh_size < 4 || end > bytes.len() {
                continue;
            }
            let grp_flags = u32::from_le_bytes(bytes[start..start + 4].try_into().unwrap());
            if (grp_flags & GRP_COMDAT) == 0 {
                continue;
            }
            let key = raw_symbols
                .get(g_hdr.sh_info as usize)
                .map(|s| s.name.clone())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| format!(".group{}", g_idx));

            let mut off = start + 4;
            while off + 4 <= end {
                let member = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()) as usize;
                off += 4;
                if let Some(Some(obj_sec)) = elf_to_obj_sec_map.get(member).copied() {
                    obj.sections[obj_sec].comdat_group = Some(key.clone());
                    obj.sections[obj_sec].flags |= flags::COMDAT;
                }
            }
            for sym in obj.symbols.iter_mut() {
                if let Some(si) = sym.section_index
                    && obj.sections[si].comdat_group.as_deref() == Some(key.as_str())
                {
                    sym.comdat_group = Some(key.clone());
                }
            }
        }

        // Parse relocations (SHT_RELA)
        for r_sec_idx in rela_sections {
            let r_hdr = &raw_shdrs[r_sec_idx];
            let target_elf_sec = r_hdr.sh_info as usize;

            if let Some(target_obj_sec) = elf_to_obj_sec_map.get(target_elf_sec).and_then(|&x| x) {
                let start = r_hdr.sh_offset as usize;
                let num_relocs = (r_hdr.sh_size / 24) as usize;

                for r_i in 0..num_relocs {
                    let off = start + r_i * 24;
                    if off + 24 > bytes.len() {
                        break;
                    }
                    let r_offset = u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap());
                    let r_info = u64::from_le_bytes(bytes[off + 8..off + 16].try_into().unwrap());
                    let r_addend =
                        i64::from_le_bytes(bytes[off + 16..off + 24].try_into().unwrap());

                    let sym_idx = (r_info >> 32) as usize;
                    let reloc_type = (r_info & 0xFFFFFFFF) as u32;

                    let sym_name = if sym_idx < raw_symbols.len() {
                        let s = &raw_symbols[sym_idx];
                        if s.sym_type == SymbolType::Section {
                            if let Some(sec_idx) = s.section_index {
                                obj.sections[sec_idx].name.clone()
                            } else {
                                s.name.clone()
                            }
                        } else {
                            s.name.clone()
                        }
                    } else {
                        String::new()
                    };

                    let kind = match (arch, reloc_type) {
                        (Arch::X86_64, R_X86_64_64) => RelocationKind::Absolute64,
                        (Arch::X86_64, R_X86_64_32) | (Arch::X86_64, R_X86_64_32S) => {
                            RelocationKind::Absolute32
                        }
                        (Arch::X86_64, R_X86_64_PC32) => RelocationKind::PcRelative32,
                        (Arch::X86_64, R_X86_64_PLT32) => RelocationKind::PltRelative32,
                        (Arch::X86_64, R_X86_64_GOT32) | (Arch::X86_64, R_X86_64_GOTPCREL) => {
                            RelocationKind::GotRelative32
                        }
                        (Arch::X86_64, R_X86_64_PC64) => RelocationKind::PcRelative64,
                        (Arch::AArch64, R_AARCH64_ABS64) => RelocationKind::Absolute64,
                        (Arch::AArch64, R_AARCH64_ABS32) => RelocationKind::Absolute32,
                        (Arch::AArch64, R_AARCH64_CALL26) | (Arch::AArch64, R_AARCH64_JUMP26) => {
                            RelocationKind::AArch64Call26
                        }
                        (Arch::AArch64, R_AARCH64_ADR_PREL_PG_HI21) => RelocationKind::AArch64Adrp,
                        (Arch::AArch64, R_AARCH64_ADD_ABS_LO12_NC) => {
                            RelocationKind::AArch64AddLo12
                        }
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_64) => RelocationKind::Absolute64,
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_32) => RelocationKind::Absolute32,
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_CALL)
                        | (Arch::Riscv64 | Arch::Riscv32, R_RISCV_CALL_PLT) => {
                            RelocationKind::RiscvCall
                        }
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_BRANCH) => {
                            RelocationKind::RiscvBranch
                        }
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_HI20) => RelocationKind::RiscvHi20,
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_LO12_I) => {
                            RelocationKind::RiscvLo12I
                        }
                        _ => RelocationKind::Absolute64,
                    };

                    obj.sections[target_obj_sec]
                        .relocations
                        .push(Relocation::new(r_offset, sym_name, kind, r_addend));
                }
            }
        }

        // Check for Adesh metadata section
        if let Some(meta_sec) = obj.find_section(".adesh.meta")
            && let Ok(meta) = crate::metadata::AdeshMetadata::decode(&meta_sec.data)
        {
            obj.metadata = Some(meta);
        }

        obj.validate()?;
        Ok(obj)
    }
}
