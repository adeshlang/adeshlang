//! ELF Object File Reader.

use crate::elf::header::*;
use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::object::ObjectFile;
use crate::relocation::{Relocation, RelocationKind};
use crate::section::{Section, SectionKind, flags};
use crate::symbol::{Symbol, SymbolBinding, SymbolType, SymbolVisibility};
use crate::target::{Arch, Endianness, ObjectFormat, Os, PointerWidth, Target};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfByteOrder {
    Little,
    Big,
}

impl ElfByteOrder {
    #[inline]
    pub fn read_u16(self, b: &[u8]) -> u16 {
        match self {
            ElfByteOrder::Little => u16::from_le_bytes(b[..2].try_into().unwrap()),
            ElfByteOrder::Big => u16::from_be_bytes(b[..2].try_into().unwrap()),
        }
    }

    #[inline]
    pub fn read_u32(self, b: &[u8]) -> u32 {
        match self {
            ElfByteOrder::Little => u32::from_le_bytes(b[..4].try_into().unwrap()),
            ElfByteOrder::Big => u32::from_be_bytes(b[..4].try_into().unwrap()),
        }
    }

    #[inline]
    pub fn read_i32(self, b: &[u8]) -> i32 {
        match self {
            ElfByteOrder::Little => i32::from_le_bytes(b[..4].try_into().unwrap()),
            ElfByteOrder::Big => i32::from_be_bytes(b[..4].try_into().unwrap()),
        }
    }

    #[inline]
    pub fn read_u64(self, b: &[u8]) -> u64 {
        match self {
            ElfByteOrder::Little => u64::from_le_bytes(b[..8].try_into().unwrap()),
            ElfByteOrder::Big => u64::from_be_bytes(b[..8].try_into().unwrap()),
        }
    }

    #[inline]
    pub fn read_i64(self, b: &[u8]) -> i64 {
        match self {
            ElfByteOrder::Little => i64::from_le_bytes(b[..8].try_into().unwrap()),
            ElfByteOrder::Big => i64::from_be_bytes(b[..8].try_into().unwrap()),
        }
    }

    // ── Write helpers (symmetric to the read helpers above) ──────────────────

    #[inline]
    pub fn write_u16(self, v: u16) -> [u8; 2] {
        match self {
            ElfByteOrder::Little => v.to_le_bytes(),
            ElfByteOrder::Big => v.to_be_bytes(),
        }
    }

    #[inline]
    pub fn write_u32(self, v: u32) -> [u8; 4] {
        match self {
            ElfByteOrder::Little => v.to_le_bytes(),
            ElfByteOrder::Big => v.to_be_bytes(),
        }
    }

    #[inline]
    pub fn write_i32(self, v: i32) -> [u8; 4] {
        match self {
            ElfByteOrder::Little => v.to_le_bytes(),
            ElfByteOrder::Big => v.to_be_bytes(),
        }
    }

    #[inline]
    pub fn write_u64(self, v: u64) -> [u8; 8] {
        match self {
            ElfByteOrder::Little => v.to_le_bytes(),
            ElfByteOrder::Big => v.to_be_bytes(),
        }
    }

    #[inline]
    pub fn write_i64(self, v: i64) -> [u8; 8] {
        match self {
            ElfByteOrder::Little => v.to_le_bytes(),
            ElfByteOrder::Big => v.to_be_bytes(),
        }
    }

    /// Return `ELFDATA2LSB` (1) for little-endian or `ELFDATA2MSB` (2) for big-endian.
    #[inline]
    pub fn ei_data_byte(self) -> u8 {
        match self {
            ElfByteOrder::Little => 1, // ELFDATA2LSB
            ElfByteOrder::Big => 2,    // ELFDATA2MSB
        }
    }
}

pub struct ElfReader;

impl ElfReader {
    pub fn read(bytes: &[u8], path: &Path, file_index: usize) -> LinkResult<ObjectFile> {
        if bytes.len() < 16 {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                format!(
                    "ELF file `{}` is too small for ELF identification ({} bytes)",
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

        let is_64 = match bytes[4] {
            ELFCLASS32 => false,
            ELFCLASS64 => true,
            other => {
                return Err(LinkError::new(
                    ErrorCode::InvalidObject,
                    format!("unsupported ELF class: 0x{:02x} ({})", other, other),
                ));
            }
        };

        let min_hdr_len = if is_64 { 64 } else { 52 };
        if bytes.len() < min_hdr_len {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                format!(
                    "ELF file `{}` is too small for ELF header (expected at least {} bytes, got {})",
                    path.display(),
                    min_hdr_len,
                    bytes.len()
                ),
            ));
        }

        let endianness = match bytes[5] {
            ELFDATA2LSB => Endianness::Little,
            ELFDATA2MSB => Endianness::Big,
            other => {
                return Err(LinkError::new(
                    ErrorCode::InvalidObject,
                    format!("unsupported ELF data encoding: 0x{:02x} ({})", other, other),
                ));
            }
        };

        let bo = match endianness {
            Endianness::Little => ElfByteOrder::Little,
            Endianness::Big => ElfByteOrder::Big,
        };

        let e_machine = bo.read_u16(&bytes[18..20]);
        let arch = match e_machine {
            EM_X86_64 => {
                if !is_64 {
                    return Err(LinkError::new(
                        ErrorCode::InvalidObject,
                        "x86-64 ELF must be 64-bit (ELFCLASS64)",
                    ));
                }
                if endianness == Endianness::Big {
                    return Err(LinkError::new(
                        ErrorCode::ArchitectureMismatch,
                        "x86-64 does not support big-endian ELF",
                    ));
                }
                Arch::X86_64
            }
            EM_386 => {
                if is_64 {
                    return Err(LinkError::new(
                        ErrorCode::InvalidObject,
                        "x86 ELF must be 32-bit (ELFCLASS32)",
                    ));
                }
                if endianness == Endianness::Big {
                    return Err(LinkError::new(
                        ErrorCode::ArchitectureMismatch,
                        "x86 does not support big-endian ELF",
                    ));
                }
                Arch::X86
            }
            EM_AARCH64 => {
                if !is_64 {
                    return Err(LinkError::new(
                        ErrorCode::InvalidObject,
                        "AArch64 ELF must be 64-bit (ELFCLASS64)",
                    ));
                }
                Arch::AArch64
            }
            EM_ARM => {
                if is_64 {
                    return Err(LinkError::new(
                        ErrorCode::InvalidObject,
                        "ARM 32-bit ELF must be 32-bit (ELFCLASS32)",
                    ));
                }
                Arch::Arm
            }
            EM_RISCV => {
                if is_64 {
                    Arch::Riscv64
                } else {
                    Arch::Riscv32
                }
            }
            EM_PPC64 => {
                if !is_64 {
                    return Err(LinkError::new(
                        ErrorCode::InvalidObject,
                        "PPC64 ELF must be 64-bit (ELFCLASS64)",
                    ));
                }
                if endianness == Endianness::Big {
                    Arch::Ppc64
                } else {
                    Arch::Ppc64le
                }
            }
            EM_PPC => {
                if is_64 {
                    return Err(LinkError::new(
                        ErrorCode::InvalidObject,
                        "PPC 32-bit ELF must be 32-bit (ELFCLASS32)",
                    ));
                }
                Arch::Ppc64
            }
            EM_S390 => {
                if !is_64 {
                    return Err(LinkError::new(
                        ErrorCode::InvalidObject,
                        "s390x ELF must be 64-bit (ELFCLASS64)",
                    ));
                }
                if endianness == Endianness::Little {
                    return Err(LinkError::new(
                        ErrorCode::ArchitectureMismatch,
                        "s390x does not support little-endian ELF",
                    ));
                }
                Arch::S390x
            }
            EM_SPARC | EM_SPARCV9 => {
                if endianness == Endianness::Little {
                    return Err(LinkError::new(
                        ErrorCode::ArchitectureMismatch,
                        "Sparc64 does not support little-endian ELF",
                    ));
                }
                Arch::Sparc64
            }
            EM_MIPS => {
                if is_64 {
                    if endianness == Endianness::Big {
                        Arch::Mips64
                    } else {
                        Arch::Mips64le
                    }
                } else if endianness == Endianness::Big {
                    Arch::Mips
                } else {
                    Arch::Mipsle
                }
            }
            EM_LOONGARCH => Arch::Loong64,
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
            endianness,
            relocation_model: crate::target::RelocationModel::Static,
            page_size: if arch == Arch::AArch64 { 65536 } else { 4096 },
            image_base: if is_64 { 0x400000 } else { 0x08048000 },
            default_entry: "_start".to_string(),
            libc_flavor: crate::target::LibcFlavor::Gnu,
        };

        let mut obj = ObjectFile::new(path.to_path_buf(), target, file_index);

        let (shoff, shentsize, shnum, shstrndx) = if is_64 {
            let shoff = bo.read_u64(&bytes[40..48]) as usize;
            let shentsize = bo.read_u16(&bytes[58..60]) as usize;
            let shnum = bo.read_u16(&bytes[60..62]) as usize;
            let shstrndx = bo.read_u16(&bytes[62..64]) as usize;
            (shoff, shentsize, shnum, shstrndx)
        } else {
            let shoff = bo.read_u32(&bytes[32..36]) as usize;
            let shentsize = bo.read_u16(&bytes[46..48]) as usize;
            let shnum = bo.read_u16(&bytes[48..50]) as usize;
            let shstrndx = bo.read_u16(&bytes[50..52]) as usize;
            (shoff, shentsize, shnum, shstrndx)
        };

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
            let (
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
            ) = if is_64 {
                (
                    bo.read_u32(&bytes[off..off + 4]),
                    bo.read_u32(&bytes[off + 4..off + 8]),
                    bo.read_u64(&bytes[off + 8..off + 16]),
                    bo.read_u64(&bytes[off + 16..off + 24]),
                    bo.read_u64(&bytes[off + 24..off + 32]),
                    bo.read_u64(&bytes[off + 32..off + 40]),
                    bo.read_u32(&bytes[off + 40..off + 44]),
                    bo.read_u32(&bytes[off + 44..off + 48]),
                    bo.read_u64(&bytes[off + 48..off + 56]),
                    bo.read_u64(&bytes[off + 56..off + 64]),
                )
            } else {
                (
                    bo.read_u32(&bytes[off..off + 4]),
                    bo.read_u32(&bytes[off + 4..off + 8]),
                    bo.read_u32(&bytes[off + 8..off + 12]) as u64,
                    bo.read_u32(&bytes[off + 12..off + 16]) as u64,
                    bo.read_u32(&bytes[off + 16..off + 20]) as u64,
                    bo.read_u32(&bytes[off + 20..off + 24]) as u64,
                    bo.read_u32(&bytes[off + 24..off + 28]),
                    bo.read_u32(&bytes[off + 28..off + 32]),
                    bo.read_u32(&bytes[off + 32..off + 36]) as u64,
                    bo.read_u32(&bytes[off + 36..off + 40]) as u64,
                )
            };

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

            if shdr.sh_type == SHT_GROUP {
                group_sections.push(i);
                continue;
            }

            if shdr.sh_type == SHT_STRTAB && i != shstrndx {
                continue;
            }

            let is_tls = (shdr.sh_flags & SHF_TLS) != 0;
            let kind = if is_tls {
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
            let sym_entsize = if is_64 { 24 } else { 16 };
            let num_syms = if sym_entsize > 0 {
                (sym_hdr.sh_size / sym_entsize as u64) as usize
            } else {
                0
            };

            for s_i in 0..num_syms {
                let off = sym_start + s_i * sym_entsize;
                if off + sym_entsize > bytes.len() {
                    break;
                }

                let (st_name, st_info, st_shndx, st_value, st_size) = if is_64 {
                    let st_name = bo.read_u32(&bytes[off..off + 4]);
                    let st_info = bytes[off + 4];
                    let _st_other = bytes[off + 5];
                    let st_shndx = bo.read_u16(&bytes[off + 6..off + 8]);
                    let st_value = bo.read_u64(&bytes[off + 8..off + 16]);
                    let st_size = bo.read_u64(&bytes[off + 16..off + 24]);
                    (st_name, st_info, st_shndx, st_value, st_size)
                } else {
                    let st_name = bo.read_u32(&bytes[off..off + 4]);
                    let st_value = bo.read_u32(&bytes[off + 4..off + 8]) as u64;
                    let st_size = bo.read_u32(&bytes[off + 8..off + 12]) as u64;
                    let st_info = bytes[off + 12];
                    let _st_other = bytes[off + 13];
                    let st_shndx = bo.read_u16(&bytes[off + 14..off + 16]);
                    (st_name, st_info, st_shndx, st_value, st_size)
                };

                let mut sym_name = get_string(sym_strtab, st_name);
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

                if sym_type == SymbolType::Section && sym_name.is_empty() {
                    if let Some(sec_idx) = mapped_sec_idx {
                        sym_name = obj.sections[sec_idx].name.clone();
                    }
                }

                let sym = Symbol {
                    name: sym_name,
                    binding,
                    visibility: SymbolVisibility::Default,
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
                obj.add_symbol(sym);
            }
        }

        // Apply SHT_GROUP COMDAT keys
        for g_idx in group_sections {
            let g_hdr = &raw_shdrs[g_idx];
            let start = g_hdr.sh_offset as usize;
            let end = start + g_hdr.sh_size as usize;
            if g_hdr.sh_size < 4 || end > bytes.len() {
                continue;
            }
            let grp_flags = bo.read_u32(&bytes[start..start + 4]);
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
                let member = bo.read_u32(&bytes[off..off + 4]) as usize;
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

        // Parse relocations (SHT_RELA and SHT_REL)
        for r_sec_idx in rela_sections {
            let r_hdr = &raw_shdrs[r_sec_idx];
            let target_elf_sec = r_hdr.sh_info as usize;

            if let Some(target_obj_sec) = elf_to_obj_sec_map.get(target_elf_sec).and_then(|&x| x) {
                let start = r_hdr.sh_offset as usize;
                let is_rela = r_hdr.sh_type == SHT_RELA;

                let r_entsize = if is_64 {
                    if is_rela { 24 } else { 16 }
                } else if is_rela {
                    12
                } else {
                    8
                };

                let num_relocs = if r_entsize > 0 {
                    (r_hdr.sh_size / r_entsize as u64) as usize
                } else {
                    0
                };

                let mut pending_uleb_offset = None;
                for r_i in 0..num_relocs {
                    let off = start + r_i * r_entsize;
                    if off + r_entsize > bytes.len() {
                        break;
                    }

                    let (r_offset, sym_idx, reloc_type, r_addend) = if is_64 {
                        let r_offset = bo.read_u64(&bytes[off..off + 8]);
                        let r_info = bo.read_u64(&bytes[off + 8..off + 16]);
                        let r_addend = if is_rela {
                            bo.read_i64(&bytes[off + 16..off + 24])
                        } else {
                            0i64
                        };
                        let sym_idx = (r_info >> 32) as usize;
                        let reloc_type = (r_info & 0xFFFFFFFF) as u32;
                        (r_offset, sym_idx, reloc_type, r_addend)
                    } else {
                        let r_offset = bo.read_u32(&bytes[off..off + 4]) as u64;
                        let r_info = bo.read_u32(&bytes[off + 4..off + 8]);
                        let r_addend = if is_rela {
                            bo.read_i32(&bytes[off + 8..off + 12]) as i64
                        } else {
                            0i64
                        };
                        let sym_idx = (r_info >> 8) as usize;
                        let reloc_type = r_info & 0xFF;
                        (r_offset, sym_idx, reloc_type, r_addend)
                    };

                    // Check raw adjacency before discarding NONE/RELAX markers.
                    if matches!(arch, Arch::Riscv32 | Arch::Riscv64) {
                        if !is_rela && matches!(reloc_type, R_RISCV_SET_ULEB128 | R_RISCV_SUB_ULEB128) {
                            return Err(LinkError::new(ErrorCode::UnsupportedRelocation,
                                "RISC-V ULEB128 requires SHT_RELA; SHT_REL implicit addends are unsupported"));
                        }
                        if let Some(expected) = pending_uleb_offset.take() {
                            if reloc_type != R_RISCV_SUB_ULEB128 || r_offset != expected {
                                return Err(LinkError::new(ErrorCode::InvalidObject,
                                    "RISC-V SET_ULEB128 must be immediately followed by SUB_ULEB128 at the same offset"));
                            }
                        } else if reloc_type == R_RISCV_SUB_ULEB128 {
                            return Err(LinkError::new(ErrorCode::InvalidObject,
                                "orphan RISC-V SUB_ULEB128 relocation"));
                        }
                        if reloc_type == R_RISCV_SET_ULEB128 {
                            pending_uleb_offset = Some(r_offset);
                        }
                    }

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
                        (Arch::X86_64, R_X86_64_16) => RelocationKind::Absolute16,
                        (Arch::X86_64, R_X86_64_8) => RelocationKind::Absolute8,
                        (Arch::X86_64, R_X86_64_PC32) => RelocationKind::PcRelative32,
                        (Arch::X86_64, R_X86_64_PLT32) => RelocationKind::PltRelative32,
                        (Arch::X86_64, R_X86_64_GOT32)
                        | (Arch::X86_64, R_X86_64_GOTPCREL)
                        | (Arch::X86_64, R_X86_64_GOTPCRELX)
                        | (Arch::X86_64, R_X86_64_REX_GOTPCRELX) => RelocationKind::GotRelative32,
                        (Arch::X86_64, R_X86_64_PC64) => RelocationKind::PcRelative64,
                        (Arch::X86_64, R_X86_64_TLSGD) | (Arch::X86_64, R_X86_64_TLSLD) => {
                            RelocationKind::TlsGeneralDynamic
                        }
                        (Arch::X86_64, R_X86_64_GOTTPOFF) => RelocationKind::TlsInitialExec,
                        (Arch::X86_64, R_X86_64_TPOFF32) => RelocationKind::TlsLocalExec,
                        (Arch::X86_64, R_X86_64_DTPOFF32) => RelocationKind::TlsDtpOff32,
                        (Arch::AArch64, R_AARCH64_ABS64) => RelocationKind::Absolute64,
                        (Arch::AArch64, R_AARCH64_ABS32) => RelocationKind::Absolute32,
                        (Arch::AArch64, R_AARCH64_CALL26) | (Arch::AArch64, R_AARCH64_JUMP26) => {
                            RelocationKind::AArch64Call26
                        }
                        (Arch::AArch64, R_AARCH64_ADR_PREL_PG_HI21) => RelocationKind::AArch64Adrp,
                        (Arch::AArch64, R_AARCH64_ADD_ABS_LO12_NC) => {
                            RelocationKind::AArch64AddLo12
                        }
                        (Arch::AArch64, R_AARCH64_LD64_GOT_LO12_NC) => {
                            RelocationKind::AArch64LdSt64Lo12
                        }
                        // No relaxation is performed; these markers patch no bytes.
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_NONE | R_RISCV_RELAX) => continue,
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_SET_ULEB128) => RelocationKind::RiscvSetUleb128,
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_SUB_ULEB128) => RelocationKind::RiscvSubUleb128,
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_ADD8) => RelocationKind::RiscvAdd8,
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_ADD16) => {
                            RelocationKind::RiscvAdd16
                        }
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_ADD32) => {
                            RelocationKind::RiscvAdd32
                        }
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_ADD64) => {
                            RelocationKind::RiscvAdd64
                        }
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_SUB8) => RelocationKind::RiscvSub8,
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_SUB16) => {
                            RelocationKind::RiscvSub16
                        }
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_SUB32) => {
                            RelocationKind::RiscvSub32
                        }
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_SUB64) => {
                            RelocationKind::RiscvSub64
                        }
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_RVC_BRANCH) => {
                            RelocationKind::RiscvRvcBranch
                        }
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_RVC_JUMP) => {
                            RelocationKind::RiscvRvcJump
                        }
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_64) => RelocationKind::Absolute64,
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_32) => RelocationKind::Absolute32,
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_32_PCREL) => RelocationKind::PcRelative32,
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_CALL)
                        | (Arch::Riscv64 | Arch::Riscv32, R_RISCV_CALL_PLT) => {
                            RelocationKind::RiscvCall
                        }
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_BRANCH) => {
                            RelocationKind::RiscvBranch
                        }
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_JAL) => RelocationKind::RiscvJal,
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_HI20) => RelocationKind::RiscvHi20,
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_LO12_I) => {
                            RelocationKind::RiscvLo12I
                        }
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_LO12_S) => {
                            RelocationKind::RiscvLo12S
                        }
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_PCREL_HI20) => {
                            RelocationKind::RiscvPcrelHi20
                        }
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_PCREL_LO12_I) => {
                            RelocationKind::RiscvPcrelLo12I
                        }
                        (Arch::Riscv64 | Arch::Riscv32, R_RISCV_PCREL_LO12_S) => {
                            RelocationKind::RiscvPcrelLo12S
                        }
                        (Arch::Riscv64 | Arch::Riscv32, _) => {
                            return Err(LinkError::new(
                                ErrorCode::UnsupportedRelocation,
                                format!(
                                    "unsupported RISC-V ELF relocation type {} at offset 0x{:x}",
                                    reloc_type, r_offset
                                ),
                            ));
                        }
                        _ => {
                            if is_64 {
                                RelocationKind::Absolute64
                            } else {
                                RelocationKind::Absolute32
                            }
                        }
                    };

                    let mut reloc_rec = Relocation::new(r_offset, sym_name, kind, r_addend);
                    reloc_rec.symbol_index = Some(sym_idx);
                    reloc_rec.file_index = Some(file_index);
                    obj.sections[target_obj_sec].relocations.push(reloc_rec);
                }
                if pending_uleb_offset.is_some() {
                    return Err(LinkError::new(ErrorCode::InvalidObject,
                        "orphan RISC-V SET_ULEB128 relocation"));
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
