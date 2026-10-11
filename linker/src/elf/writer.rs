//! ELF Executable, Shared Library, and Binary Writer.
//!
//! Emits ET_EXEC/ET_DYN images with full dynamic-linking metadata
//! (`.dynsym`, `.dynstr`, `.hash`, `.gnu.hash`, `.rela.dyn`, `.rela.plt`,
//! `.dynamic`, `PT_INTERP`, `PT_DYNAMIC`, `PT_GNU_RELRO`) driven by the
//! [`crate::elf::dynamic::ElfLinkInfo`] produced during layout.
//!
//! Invariant used throughout: every allocated byte satisfies
//! `file_offset = virtual_address - image_base`, so `.bss` VA ranges appear
//! as zero-filled file holes and the loader's VA-based pointers always match
//! the file contents.

use crate::elf::dynamic::{ElfDynamicRelocKind, ElfLinkInfo, GnuHashTable};
use crate::elf::header::*;
use crate::elf::notes::create_gnu_build_id_note;
use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::section::{MergedSection, SectionKind, align_to};
use crate::symbol::{Symbol, SymbolBinding, SymbolType};
use crate::target::{Arch, Target};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

pub struct ElfWriter;

/// Placement record for a file/VA region.
#[derive(Debug, Clone, Copy, Default)]
struct Placement {
    offset: u64,
    va: u64,
    size: u64,
}

/// Write `bytes` into `output` at the cursor, maintaining the
/// `offset = va - image_base` invariant.
fn place_table(
    output: &mut Vec<u8>,
    bytes: &[u8],
    alignment: u64,
    image_base: u64,
    cursor: &mut (u64, u64), // (offset, va)
) -> Placement {
    let (off, va) = *cursor;
    let aligned_off = align_to(off, alignment);
    let aligned_va = align_to(va, alignment);
    let pad = (aligned_off - off) as usize;
    if pad > 0 {
        output.resize(output.len() + pad, 0);
    }
    let start = aligned_off as usize;
    let end = start + bytes.len();
    if output.len() < end {
        output.resize(end, 0);
    }
    output[start..end].copy_from_slice(bytes);
    *cursor = (
        aligned_off + bytes.len() as u64,
        aligned_va + bytes.len() as u64,
    );
    let _ = image_base;
    Placement {
        offset: aligned_off,
        va: aligned_va,
        size: bytes.len() as u64,
    }
}

/// Build one `Elf64_Sym` and append its name to a string table.
fn build_sym(
    name: &str,
    sym: Option<&Symbol>,
    output_section_index: &dyn Fn(&Symbol) -> Option<u16>,
    strtab: &mut Vec<u8>,
) -> Elf64_Sym {
    let st_name = strtab.len() as u32;
    strtab.extend_from_slice(name.as_bytes());
    strtab.push(0);
    let (bind, st_type, st_shndx, st_value, st_size) = match sym {
        Some(s) => {
            let bind = match s.binding {
                SymbolBinding::Local => STB_LOCAL,
                SymbolBinding::Global => STB_GLOBAL,
                SymbolBinding::Weak => STB_WEAK,
            };
            let st_type = match s.sym_type {
                SymbolType::Function => STT_FUNC,
                SymbolType::Object => STT_OBJECT,
                SymbolType::Tls => STT_TLS,
                SymbolType::Section => STT_SECTION,
                SymbolType::File => STT_FILE,
                _ => STT_NOTYPE,
            };
            (
                bind,
                st_type,
                output_section_index(s).unwrap_or(SHN_ABS),
                s.value,
                s.size,
            )
        }
        None => (STB_GLOBAL, STT_NOTYPE, SHN_UNDEF, 0, 0),
    };
    let st_info = (bind << 4) | (st_type & 0xF);
    Elf64_Sym {
        st_name,
        st_info,
        st_other: 0,
        st_shndx,
        st_value,
        st_size,
    }
}

fn encode_sym(s: &Elf64_Sym, out: &mut Vec<u8>) {
    out.extend_from_slice(&s.st_name.to_le_bytes());
    out.push(s.st_info);
    out.push(s.st_other);
    out.extend_from_slice(&s.st_shndx.to_le_bytes());
    out.extend_from_slice(&s.st_value.to_le_bytes());
    out.extend_from_slice(&s.st_size.to_le_bytes());
}

fn encode_rela(off: u64, info: u64, addend: i64, out: &mut Vec<u8>) {
    out.extend_from_slice(&off.to_le_bytes());
    out.extend_from_slice(&info.to_le_bytes());
    out.extend_from_slice(&addend.to_le_bytes());
}

fn encode_dyn(d: &Elf64_Dyn, out: &mut Vec<u8>) {
    out.extend_from_slice(&d.d_tag.to_le_bytes());
    out.extend_from_slice(&d.d_val.to_le_bytes());
}

impl ElfWriter {
    pub fn write_executable(
        path: &Path,
        target: &Target,
        entry_va: u64,
        merged_sections: &[MergedSection],
        symbols: &[Symbol],
        build_id: Option<&[u8]>,
    ) -> LinkResult<()> {
        Self::write_elf(
            path,
            target,
            entry_va,
            merged_sections,
            symbols,
            build_id,
            false,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn write_elf(
        path: &Path,
        target: &Target,
        entry_va: u64,
        merged_sections: &[MergedSection],
        symbols: &[Symbol],
        build_id: Option<&[u8]>,
        is_shared: bool,
        elf_info: Option<&ElfLinkInfo>,
    ) -> LinkResult<()> {
        let bytes = Self::encode_elf(
            target,
            entry_va,
            merged_sections,
            symbols,
            build_id,
            is_shared,
            elf_info,
        )?;
        fs::write(path, bytes)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = fs::metadata(path) {
                let mut perms = meta.permissions();
                perms.set_mode(0o755);
                let _ = fs::set_permissions(path, perms);
            }
        }

        Ok(())
    }

    pub fn encode_executable(
        target: &Target,
        entry_va: u64,
        merged_sections: &[MergedSection],
        symbols: &[Symbol],
        build_id: Option<&[u8]>,
    ) -> LinkResult<Vec<u8>> {
        Self::encode_elf(
            target,
            entry_va,
            merged_sections,
            symbols,
            build_id,
            false,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn encode_elf(
        target: &Target,
        mut entry_va: u64,
        merged_sections: &[MergedSection],
        symbols: &[Symbol],
        build_id: Option<&[u8]>,
        is_shared: bool,
        elf_info: Option<&ElfLinkInfo>,
    ) -> LinkResult<Vec<u8>> {
        let e_machine = match target.arch {
            Arch::X86_64 => EM_X86_64,
            Arch::AArch64 => EM_AARCH64,
            Arch::Arm => EM_ARM,
            Arch::Riscv64 | Arch::Riscv32 => EM_RISCV,
            _ => EM_X86_64,
        };

        let is_dynamic = is_shared || elf_info.is_some_and(ElfLinkInfo::is_dynamic);
        let is_pie = !is_shared && elf_info.is_some_and(|info| info.pie);

        // A dynamic *executable* requires an interpreter (PT_INTERP). A
        // missing interpreter path is a link-time error, never a silently
        // static emission or a guessed path.
        let interp_str: Option<&str> = if is_dynamic && !is_shared {
            match elf_info.and_then(|info| info.interpreter.as_deref()) {
                Some(p) if !p.is_empty() => Some(p),
                _ => {
                    return Err(LinkError::new(
                        ErrorCode::InvalidTarget,
                        format!(
                            "target `{}` has no known dynamic loader (PT_INTERP path) for a \
                             dynamic executable; link statically, or use a target triple with a \
                             supported libc (gnu/musl)",
                            target
                        ),
                    ));
                }
            }
        } else {
            None
        };

        let mut output = Vec::new();

        // 1. Plan headers.
        let ehdr_size = 64usize;
        let phdr_size = 56usize;
        let mut phnum = 4usize; // PT_PHDR, PT_LOAD(RX), PT_LOAD(RW), PT_GNU_STACK
        if interp_str.is_some() {
            phnum += 1;
        }
        if is_dynamic {
            phnum += 2; // PT_DYNAMIC, PT_GNU_RELRO
        }
        if build_id.is_some() {
            phnum += 1; // PT_NOTE
        }
        let headers_size = ehdr_size + phnum * phdr_size;
        output.resize(headers_size, 0);

        // 2. Resolve ELF entry address if not explicitly set.
        if entry_va == 0 {
            entry_va = symbols
                .iter()
                .find(|s| (s.name == "_start" || s.name == "main") && s.is_defined)
                .map(|s| s.value)
                .unwrap_or_else(|| {
                    merged_sections
                        .iter()
                        .find(|s| s.is_executable())
                        .map(|s| s.virtual_address)
                        .unwrap_or(target.image_base + 0x1000)
                });
        }

        // 3. Partition sections into RX / RW groups.
        let mut rx_sections = Vec::new();
        let mut rw_sections = Vec::new();
        for (idx, sec) in merged_sections.iter().enumerate() {
            if !sec.is_alloc() {
                continue;
            }
            if sec.is_executable() || (!sec.is_writable() && sec.kind != SectionKind::Bss) {
                rx_sections.push(idx);
            } else {
                rw_sections.push(idx);
            }
        }

        let mut sec_offsets = vec![0u64; merged_sections.len()];
        let mut sec_vaddrs = vec![0u64; merged_sections.len()];

        let rx_vaddr_start = target.image_base;
        let mut current_offset = headers_size as u64;
        let mut current_va = rx_vaddr_start + current_offset;

        let mut interp_offset = 0u64;
        let mut interp_va = 0u64;
        let mut interp_size = 0u64;
        if let Some(interp) = interp_str {
            let mut interp_data = interp.as_bytes().to_vec();
            interp_data.push(0);
            interp_offset = current_offset;
            interp_va = current_va;
            interp_size = interp_data.len() as u64;
            output.extend_from_slice(&interp_data);
            current_offset += interp_size;
            current_va += interp_size;
        }

        for &sec_idx in &rx_sections {
            let sec = &merged_sections[sec_idx];
            let aligned_offset = align_to(current_offset, sec.alignment);
            let aligned_va = align_to(current_va, sec.alignment);
            if aligned_offset > current_offset {
                let pad = (aligned_offset - current_offset) as usize;
                output.resize(output.len() + pad, 0);
                current_offset = aligned_offset;
                current_va = aligned_va;
            }
            sec_offsets[sec_idx] = current_offset;
            sec_vaddrs[sec_idx] = aligned_va;
            output.extend_from_slice(&sec.data);
            current_offset += sec.data.len() as u64;
            current_va += sec.data.len() as u64;
        }

        let mut note_offset = 0u64;
        let mut note_size = 0u64;
        if let Some(bid) = build_id {
            let note_data = create_gnu_build_id_note(bid);
            let aligned_offset = align_to(current_offset, 4);
            let aligned_va = align_to(current_va, 4);
            if aligned_offset > current_offset {
                let pad = (aligned_offset - current_offset) as usize;
                output.resize(output.len() + pad, 0);
                current_offset = aligned_offset;
                current_va = aligned_va;
            }
            note_offset = current_offset;
            note_size = note_data.len() as u64;
            output.extend_from_slice(&note_data);
            current_offset += note_size;
            current_va += note_size;
        }

        let mut rx_file_size = current_offset;
        let mut rx_mem_size = current_offset;

        let mut rw_file_offset_start = align_to(current_offset, target.page_size);
        let mut rw_vaddr_start = align_to(current_va, target.page_size);
        if rw_file_offset_start > current_offset {
            let pad = (rw_file_offset_start - current_offset) as usize;
            output.resize(output.len() + pad, 0);
            current_offset = rw_file_offset_start;
            current_va = rw_vaddr_start;
        }

        let mut rw_file_size = 0u64;
        let mut rw_mem_size = 0u64;
        for &sec_idx in &rw_sections {
            let sec = &merged_sections[sec_idx];
            let aligned_offset = align_to(current_offset, sec.alignment);
            let aligned_va = align_to(current_va, sec.alignment);
            if aligned_offset > current_offset {
                let pad = (aligned_offset - current_offset) as usize;
                output.resize(output.len() + pad, 0);
                current_offset = aligned_offset;
                current_va = aligned_va;
            }
            sec_offsets[sec_idx] = current_offset;
            sec_vaddrs[sec_idx] = aligned_va;
            if sec.kind != SectionKind::Bss {
                output.extend_from_slice(&sec.data);
                current_offset += sec.data.len() as u64;
                rw_file_size = current_offset - rw_file_offset_start;
            }
            current_va += sec.size;
            rw_mem_size = current_va - rw_vaddr_start;
        }

        // 3b. Authoritative placement when the layout engine assigned VAs.
        // File offsets derive from VAs (off = va - image_base).
        let non_alloc_sections: Vec<usize> = merged_sections
            .iter()
            .enumerate()
            .filter(|(_, sec)| !sec.is_alloc())
            .map(|(idx, _)| idx)
            .collect();

        let has_layout_vas = merged_sections.iter().any(|s| s.virtual_address != 0);
        if has_layout_vas {
            output.truncate(headers_size);

            // Re-emit the interpreter string at the offset the prologue
            // reserved for it: the truncate above discarded that copy, and
            // the loader reads the path from this exact file offset
            // (`off = va - image_base`). Sections start at
            // `image_base + page_size` at the earliest, so the header page
            // holds the ELF header, program headers, interpreter string and
            // build-id note without overlap.
            if let Some(interp) = interp_str {
                let start = interp_offset as usize;
                let end = start + interp_size as usize;
                if output.len() < end {
                    output.resize(end, 0);
                }
                // interp_size counts the NUL terminator; interp.as_bytes()
                // does not include it.
                output[start..end - 1].copy_from_slice(interp.as_bytes());
                output[end - 1] = 0;
            }

            let mut rx_file_end = headers_size as u64;
            let mut rx_mem_end = headers_size as u64;
            let mut rw_off_min: Option<u64> = None;
            let mut rw_va_min: Option<u64> = None;
            let mut rw_file_end = 0u64;
            let mut rw_mem_end = 0u64;

            for (idx, sec) in merged_sections.iter().enumerate() {
                if !sec.is_alloc() {
                    continue;
                }
                if sec.virtual_address < target.image_base {
                    return Err(LinkError::new(
                        ErrorCode::InvalidSection,
                        format!(
                            "section `{}` virtual address 0x{:x} is below the image base 0x{:x}",
                            sec.name, sec.virtual_address, target.image_base
                        ),
                    ));
                }
                let off = sec.virtual_address - target.image_base;
                sec_vaddrs[idx] = sec.virtual_address;
                sec_offsets[idx] = off;

                let is_rx =
                    sec.is_executable() || (!sec.is_writable() && sec.kind != SectionKind::Bss);
                let file_len = if sec.kind == SectionKind::Bss {
                    0
                } else {
                    sec.data.len() as u64
                };
                let mem_len = sec.size.max(sec.data.len() as u64);

                if file_len > 0 {
                    let start = off as usize;
                    let end = start + file_len as usize;
                    if output.len() < end {
                        output.resize(end, 0);
                    }
                    output[start..end].copy_from_slice(&sec.data);
                }

                if is_rx {
                    rx_file_end = rx_file_end.max(off + file_len);
                    rx_mem_end = rx_mem_end.max(off + mem_len);
                } else {
                    rw_off_min = Some(rw_off_min.map_or(off, |m| m.min(off)));
                    rw_va_min =
                        Some(rw_va_min.map_or(sec.virtual_address, |m| m.min(sec.virtual_address)));
                    rw_file_end = rw_file_end.max(off + file_len);
                    rw_mem_end = rw_mem_end.max(sec.virtual_address + mem_len);
                }
            }

            if let Some(bid) = build_id {
                let note_data = create_gnu_build_id_note(bid);
                // Placed after the interpreter string inside the reserved
                // header page; a fixed `headers_size` placement would
                // overwrite the interpreter string when both are present.
                let off = align_to(
                    if interp_size > 0 {
                        interp_offset + interp_size
                    } else {
                        headers_size as u64
                    },
                    4,
                );
                let end = off as usize + note_data.len();
                if output.len() < end {
                    output.resize(end, 0);
                }
                output[off as usize..end].copy_from_slice(&note_data);
                note_offset = off;
                note_size = note_data.len() as u64;
                rx_file_end = rx_file_end.max(end as u64);
                rx_mem_end = rx_mem_end.max(off + note_data.len() as u64);
            }

            let (rw_off, rw_va) = match (rw_off_min, rw_va_min) {
                (Some(off), Some(va)) => (off, va),
                _ => (
                    align_to(rx_file_end, target.page_size),
                    align_to(target.image_base + rx_mem_end, target.page_size),
                ),
            };

            rx_file_size = rx_file_end;
            rx_mem_size = rx_mem_end;
            rw_file_size = rw_file_end.saturating_sub(rw_off);
            rw_mem_size = rw_mem_end.saturating_sub(rw_va);
            rw_file_offset_start = rw_off;
            rw_vaddr_start = rw_va;

            // End of the last allocated section's VA range; the dynamic
            // region is placed (page-aligned) after it.
            current_va = merged_sections
                .iter()
                .filter(|s| s.is_alloc() && s.virtual_address != 0)
                .map(|s| s.virtual_address + s.size.max(s.data.len() as u64))
                .max()
                .unwrap_or(target.image_base + headers_size as u64);
        }

        // 4. Dynamic-linking tables. Computed in full, then placed.
        let mut rela_dyn = Placement::default();
        let mut rela_plt = Placement::default();
        let mut dynsym = Placement::default();
        let mut dynstr = Placement::default();
        let mut hash = Placement::default();
        let mut gnu_hash = Placement::default();
        let mut dynamic = Placement::default();
        let mut relro_range: Option<(u64, u64)> = None;

        let info_imports: Vec<String> = elf_info
            .map(|info| info.imports.clone())
            .unwrap_or_default();
        let import_set: HashSet<&str> = info_imports.iter().map(|s| s.as_str()).collect();

        let mut dynsym_bytes: Vec<u8> = Vec::new();
        let mut dynstr_buf: Vec<u8> = Vec::new();
        let mut hash_bytes: Vec<u8> = Vec::new();
        let mut gnu_hash_bytes: Vec<u8> = Vec::new();
        let mut rela_dyn_bytes: Vec<u8> = Vec::new();
        let mut rela_plt_bytes: Vec<u8> = Vec::new();
        let mut dynamic_bytes: Vec<u8> = Vec::new();
        let mut needed_str_offsets: Vec<u64> = Vec::new();
        let mut soname_str_offset: Option<u64> = None;

        if is_dynamic {
            // 4a. Dynamic string table: library names first, then symbols.
            dynstr_buf.push(0);
            let needed: Vec<String> = elf_info
                .map(|info| info.needed_libs.clone())
                .unwrap_or_default();
            for lib_name in &needed {
                needed_str_offsets.push(dynstr_buf.len() as u64);
                dynstr_buf.extend_from_slice(lib_name.as_bytes());
                dynstr_buf.push(0);
            }
            if let Some(soname) = elf_info.and_then(|info| info.soname.as_deref()) {
                soname_str_offset = Some(dynstr_buf.len() as u64);
                dynstr_buf.extend_from_slice(soname.as_bytes());
                dynstr_buf.push(0);
            }

            // 4b. Dynamic symbol table: [null] + imports ∪ exports in
            // `.gnu.hash` order.
            let mut input_sec_to_out: HashMap<(usize, usize), usize> = HashMap::new();
            for (out_idx, sec) in merged_sections.iter().enumerate() {
                for &(f_idx, s_idx, _) in &sec.input_sections {
                    input_sec_to_out.insert((f_idx, s_idx), out_idx);
                }
            }
            let output_section_index = |sym: &Symbol| -> Option<u16> {
                sym.file_index
                    .zip(sym.section_index)
                    .and_then(|key| input_sec_to_out.get(&key).copied())
                    .or_else(|| {
                        merged_sections.iter().position(|sec| {
                            sec.size > 0
                                && sym.value >= sec.virtual_address
                                && sym.value < sec.virtual_address + sec.size
                        })
                    })
                    .map(|idx| (idx + 1) as u16)
            };

            let mut export_names: Vec<String> = Vec::new();
            for sym in symbols {
                if sym.name.is_empty() || import_set.contains(sym.name.as_str()) {
                    continue;
                }
                if sym.is_defined && (sym.is_exported || sym.binding == SymbolBinding::Global) {
                    export_names.push(sym.name.clone());
                }
            }
            export_names.sort();
            export_names.dedup();

            let mut hashed_names: Vec<String> = info_imports.clone();
            hashed_names.extend(export_names.iter().cloned());
            hashed_names.sort();
            hashed_names.dedup();

            let hashed_strs: Vec<&str> = hashed_names.iter().map(|s| s.as_str()).collect();
            let gnu: Option<GnuHashTable> = if hashed_strs.is_empty() {
                None
            } else {
                Some(crate::elf::dynamic::build_gnu_hash(&hashed_strs, 1))
            };

            let null_sym = Elf64_Sym {
                st_name: 0,
                st_info: 0,
                st_other: 0,
                st_shndx: 0,
                st_value: 0,
                st_size: 0,
            };
            encode_sym(&null_sym, &mut dynsym_bytes);
            let mut dynsym_count = 1usize;

            // The dynamic symbol index of a name is its position in the
            // *emitted* table (GNU-hash order), not its position in the
            // sorted input list. Both `.gnu.hash` bucket/chain data and
            // JUMP_SLOT `r_info` symbol indices refer to emitted indices.
            let name_to_dynsym_idx: HashMap<&str, usize> = gnu
                .as_ref()
                .map(|g| {
                    g.order
                        .iter()
                        .enumerate()
                        .map(|(k, &name_idx)| (hashed_strs[name_idx], k + 1))
                        .collect()
                })
                .unwrap_or_default();

            if let Some(gnu) = &gnu {
                for &name_idx in &gnu.order {
                    let name = hashed_strs[name_idx];
                    if import_set.contains(name) {
                        // Undefined import.
                        let s = build_sym(name, None, &output_section_index, &mut dynstr_buf);
                        encode_sym(&s, &mut dynsym_bytes);
                    } else if let Some(sym) = symbols
                        .iter()
                        .find(|s| s.name.as_str() == name && s.is_defined)
                    {
                        let s = build_sym(name, Some(sym), &output_section_index, &mut dynstr_buf);
                        encode_sym(&s, &mut dynsym_bytes);
                    } else {
                        return Err(LinkError::new(
                            ErrorCode::InvalidSection,
                            format!(
                                "dynamic export `{}` has no defining symbol record; cannot emit \
                                 a consistent .dynsym",
                                name
                            ),
                        ));
                    }
                    dynsym_count += 1;
                }
            }

            // 4c. Split dynamic relocations.
            let mut rela_dyn_count = 0usize;
            let mut rela_plt_count = 0usize;
            if let Some(info) = elf_info {
                for reloc in &info.dynamic_relocs {
                    let abi_num = reloc.kind.abi_number(target.arch).ok_or_else(|| {
                        LinkError::new(
                            ErrorCode::UnsupportedRelocation,
                            format!(
                                "dynamic relocation {:?} is not supported on {}",
                                reloc.kind,
                                target.arch.as_str()
                            ),
                        )
                    })?;
                    match reloc.kind {
                        ElfDynamicRelocKind::Relative => {
                            // RELATIVE relocations carry no symbol index:
                            // r_info = (0 << 32) | r_type, addend = target VA.
                            encode_rela(
                                reloc.offset,
                                abi_num as u64,
                                reloc.addend,
                                &mut rela_dyn_bytes,
                            );
                            rela_dyn_count += 1;
                        }
                        ElfDynamicRelocKind::JumpSlot => {
                            let sym_idx = reloc
                                .symbol
                                .as_deref()
                                .and_then(|name| name_to_dynsym_idx.get(name))
                                .copied()
                                .ok_or_else(|| {
                                    LinkError::new(
                                        ErrorCode::InvalidSection,
                                        format!(
                                            "JUMP_SLOT relocation has no matching dynamic \
                                             symbol (symbol `{}` missing from .dynsym)",
                                            reloc.symbol.clone().unwrap_or_default()
                                        ),
                                    )
                                })?;
                            encode_rela(
                                reloc.offset,
                                ((sym_idx as u64) << 32) | abi_num as u64,
                                reloc.addend,
                                &mut rela_plt_bytes,
                            );
                            rela_plt_count += 1;
                        }
                    }
                }
            }

            // 4d. SYSV .hash over the final dynsym order.
            let nbucket = dynsym_count.max(1) as u32;
            let nchain = dynsym_count as u32;
            let mut buckets = vec![0u32; nbucket as usize];
            let mut chains = vec![0u32; nchain as usize];
            // Decode names from dynsym_bytes/dynstr_buf pairs.
            for i in 1..dynsym_count {
                let base = i * 24;
                let st_name = u32::from_le_bytes(
                    dynsym_bytes[base..base + 4]
                        .try_into()
                        .expect("dynsym entry"),
                ) as usize;
                let end = dynstr_buf[st_name..]
                    .iter()
                    .position(|&b| b == 0)
                    .map(|p| st_name + p)
                    .unwrap_or(dynstr_buf.len());
                let h = (elf_hash(&dynstr_buf[st_name..end]) % nbucket) as usize;
                chains[i] = buckets[h];
                buckets[h] = i as u32;
            }
            hash_bytes.extend_from_slice(&nbucket.to_le_bytes());
            hash_bytes.extend_from_slice(&nchain.to_le_bytes());
            for b in &buckets {
                hash_bytes.extend_from_slice(&b.to_le_bytes());
            }
            for c in &chains {
                hash_bytes.extend_from_slice(&c.to_le_bytes());
            }

            if let Some(gnu) = &gnu {
                gnu_hash_bytes = gnu.encode(1);
            }

            // 4e. Compute placements (sizes are final now), then build
            // `.dynamic` with the resolved VAs.
            let region_va = align_to(current_va, target.page_size);
            let mut cursor = (region_va - target.image_base, region_va);

            let mut dynamic_entries: Vec<Elf64_Dyn> = Vec::new();
            if !rela_dyn_bytes.is_empty() {
                rela_dyn = place_table(
                    &mut output,
                    &rela_dyn_bytes,
                    8,
                    target.image_base,
                    &mut cursor,
                );
            }
            if !rela_plt_bytes.is_empty() {
                rela_plt = place_table(
                    &mut output,
                    &rela_plt_bytes,
                    8,
                    target.image_base,
                    &mut cursor,
                );
            }
            if dynsym_count > 1 {
                dynsym = place_table(
                    &mut output,
                    &dynsym_bytes,
                    8,
                    target.image_base,
                    &mut cursor,
                );
            }
            dynstr = place_table(&mut output, &dynstr_buf, 1, target.image_base, &mut cursor);
            hash = place_table(&mut output, &hash_bytes, 4, target.image_base, &mut cursor);
            if !gnu_hash_bytes.is_empty() {
                gnu_hash = place_table(
                    &mut output,
                    &gnu_hash_bytes,
                    4,
                    target.image_base,
                    &mut cursor,
                );
            }

            for str_off in &needed_str_offsets {
                dynamic_entries.push(Elf64_Dyn {
                    d_tag: DT_NEEDED,
                    d_val: *str_off,
                });
            }
            if let Some(soname_off) = soname_str_offset {
                dynamic_entries.push(Elf64_Dyn {
                    d_tag: DT_SONAME,
                    d_val: soname_off,
                });
            }
            if hash.size > 0 {
                dynamic_entries.push(Elf64_Dyn {
                    d_tag: DT_HASH,
                    d_val: hash.va,
                });
            }
            if gnu_hash.size > 0 {
                dynamic_entries.push(Elf64_Dyn {
                    d_tag: DT_GNU_HASH,
                    d_val: gnu_hash.va,
                });
            }
            dynamic_entries.push(Elf64_Dyn {
                d_tag: DT_STRTAB,
                d_val: dynstr.va,
            });
            dynamic_entries.push(Elf64_Dyn {
                d_tag: DT_SYMTAB,
                d_val: dynsym.va,
            });
            dynamic_entries.push(Elf64_Dyn {
                d_tag: DT_STRSZ,
                d_val: dynstr.size,
            });
            dynamic_entries.push(Elf64_Dyn {
                d_tag: DT_SYMENT,
                d_val: 24,
            });
            if rela_dyn_count > 0 {
                dynamic_entries.push(Elf64_Dyn {
                    d_tag: DT_RELA,
                    d_val: rela_dyn.va,
                });
                dynamic_entries.push(Elf64_Dyn {
                    d_tag: DT_RELASZ,
                    d_val: rela_dyn.size,
                });
                dynamic_entries.push(Elf64_Dyn {
                    d_tag: DT_RELAENT,
                    d_val: 24,
                });
            }
            if rela_plt_count > 0 {
                dynamic_entries.push(Elf64_Dyn {
                    d_tag: DT_JMPREL,
                    d_val: rela_plt.va,
                });
                dynamic_entries.push(Elf64_Dyn {
                    d_tag: DT_PLTRELSZ,
                    d_val: rela_plt.size,
                });
                dynamic_entries.push(Elf64_Dyn {
                    d_tag: DT_PLTREL,
                    d_val: DT_RELA as u64,
                });
            }
            let got_sec = merged_sections
                .iter()
                .find(|s| s.name == ".got" && s.is_alloc() && s.virtual_address != 0);
            if let Some(got) = got_sec {
                // PLTGOT points at the GOT; GOT[0] holds &_DYNAMIC per the
                // x86-64/AArch64 psABI.
                dynamic_entries.push(Elf64_Dyn {
                    d_tag: DT_PLTGOT,
                    d_val: got.virtual_address,
                });
            }
            // Eager binding: the PLT stubs are plain `jmp *slot` sequences
            // with no lazy-resolution trampoline, so the loader MUST resolve
            // every JUMP_SLOT before execution.
            let mut flags_1 = DF_1_NOW;
            if is_pie {
                flags_1 |= DF_1_PIE;
            }
            dynamic_entries.push(Elf64_Dyn {
                d_tag: DT_FLAGS,
                d_val: DF_BIND_NOW,
            });
            dynamic_entries.push(Elf64_Dyn {
                d_tag: DT_FLAGS_1,
                d_val: flags_1,
            });
            dynamic_entries.push(Elf64_Dyn {
                d_tag: DT_DEBUG,
                d_val: 0,
            });
            dynamic_entries.push(Elf64_Dyn {
                d_tag: DT_NULL,
                d_val: 0,
            });
            for e in &dynamic_entries {
                encode_dyn(e, &mut dynamic_bytes);
            }
            dynamic = place_table(
                &mut output,
                &dynamic_bytes,
                8,
                target.image_base,
                &mut cursor,
            );

            // Patch GOT[0] = &_DYNAMIC.
            if let Some(got) = got_sec {
                let slot0_off = (got.virtual_address - target.image_base) as usize;
                if slot0_off + 8 <= output.len() {
                    output[slot0_off..slot0_off + 8].copy_from_slice(&dynamic.va.to_le_bytes());
                }
            }

            // 4f. Extend the RW segment over the dynamic region.
            rw_file_size = rw_file_size.max(dynamic.offset + dynamic.size - rw_file_offset_start);
            rw_mem_size = rw_mem_size.max(dynamic.va + dynamic.size - rw_vaddr_start);

            // 4g. PT_GNU_RELRO: cover `.got` (page-aligned by the layout
            // engine) plus the whole dynamic region, making the relocation
            // and dynamic tables read-only after relocation processing.
            let relro_start = match got_sec {
                Some(got)
                    if got.virtual_address % target.page_size == 0
                        && got.virtual_address < region_va =>
                {
                    got.virtual_address
                }
                _ => region_va,
            };
            relro_range = Some((relro_start, dynamic.va + dynamic.size));
        }

        // 5. Non-alloc sections (debug info) after the dynamic region.
        for idx in &non_alloc_sections {
            let sec = &merged_sections[*idx];
            let aligned_offset = align_to(output.len() as u64, sec.alignment.max(1));
            if aligned_offset > output.len() as u64 {
                output.resize(aligned_offset as usize, 0);
            }
            sec_offsets[*idx] = aligned_offset;
            sec_vaddrs[*idx] = 0;
            output.extend_from_slice(&sec.data);
        }

        // 6. Static symbol table and section-name table.
        let mut shstrtab = vec![0u8];
        let mut shdr_names = Vec::new();
        shdr_names.push(0);
        for sec in merged_sections {
            let name_off = shstrtab.len() as u32;
            shstrtab.extend_from_slice(sec.name.as_bytes());
            shstrtab.push(0);
            shdr_names.push(name_off);
        }

        let interp_shdr_name = interp_str.map(|_| {
            let off = shstrtab.len() as u32;
            shstrtab.extend_from_slice(b".interp\0");
            off
        });

        let dyn_section_names: Option<[u32; 7]> = is_dynamic.then(|| {
            [
                {
                    let off = shstrtab.len() as u32;
                    shstrtab.extend_from_slice(b".rela.dyn\0");
                    off
                },
                {
                    let off = shstrtab.len() as u32;
                    shstrtab.extend_from_slice(b".rela.plt\0");
                    off
                },
                {
                    let off = shstrtab.len() as u32;
                    shstrtab.extend_from_slice(b".dynsym\0");
                    off
                },
                {
                    let off = shstrtab.len() as u32;
                    shstrtab.extend_from_slice(b".dynstr\0");
                    off
                },
                {
                    let off = shstrtab.len() as u32;
                    shstrtab.extend_from_slice(b".hash\0");
                    off
                },
                {
                    let off = shstrtab.len() as u32;
                    shstrtab.extend_from_slice(b".gnu.hash\0");
                    off
                },
                {
                    let off = shstrtab.len() as u32;
                    shstrtab.extend_from_slice(b".dynamic\0");
                    off
                },
            ]
        });

        let symtab_name_off = {
            let off = shstrtab.len() as u32;
            shstrtab.extend_from_slice(b".symtab\0");
            off
        };
        let strtab_name_off = {
            let off = shstrtab.len() as u32;
            shstrtab.extend_from_slice(b".strtab\0");
            off
        };
        let shstrtab_name_off = {
            let off = shstrtab.len() as u32;
            shstrtab.extend_from_slice(b".shstrtab\0");
            off
        };

        let mut strtab = vec![0u8];
        let mut elf_syms = Vec::new();
        let null_sym = Elf64_Sym {
            st_name: 0,
            st_info: 0,
            st_other: 0,
            st_shndx: 0,
            st_value: 0,
            st_size: 0,
        };
        elf_syms.push(null_sym);

        let mut input_sec_to_out: HashMap<(usize, usize), usize> = HashMap::new();
        for (out_idx, sec) in merged_sections.iter().enumerate() {
            for &(f_idx, s_idx, _) in &sec.input_sections {
                input_sec_to_out.insert((f_idx, s_idx), out_idx);
            }
        }
        let output_section_index = |sym: &Symbol| -> Option<u16> {
            sym.file_index
                .zip(sym.section_index)
                .and_then(|key| input_sec_to_out.get(&key).copied())
                .or_else(|| {
                    merged_sections.iter().position(|sec| {
                        sec.size > 0
                            && sym.value >= sec.virtual_address
                            && sym.value < sec.virtual_address + sec.size
                    })
                })
                .map(|idx| (idx + 1) as u16)
        };

        for sym in symbols {
            if sym.name.is_empty() {
                continue;
            }
            let s = build_sym(&sym.name, Some(sym), &output_section_index, &mut strtab);
            elf_syms.push(s);
        }

        let symtab_offset = align_to(output.len() as u64, 8);
        if symtab_offset > output.len() as u64 {
            output.resize(symtab_offset as usize, 0);
        }
        let symtab_size = (elf_syms.len() * 24) as u64;
        for s in &elf_syms {
            encode_sym(s, &mut output);
        }

        let strtab_offset = output.len() as u64;
        let strtab_size = strtab.len() as u64;
        output.extend_from_slice(&strtab);

        let shstrtab_offset = output.len() as u64;
        let shstrtab_size = shstrtab.len() as u64;
        output.extend_from_slice(&shstrtab);

        let shoff = align_to(output.len() as u64, 8);
        if shoff > output.len() as u64 {
            output.resize(shoff as usize, 0);
        }

        // 7. Section headers.
        let mut section_headers = Vec::new();
        section_headers.push(Elf64_Shdr {
            sh_name: 0,
            sh_type: SHT_NULL,
            sh_flags: 0,
            sh_addr: 0,
            sh_offset: 0,
            sh_size: 0,
            sh_link: 0,
            sh_info: 0,
            sh_addralign: 0,
            sh_entsize: 0,
        });

        for (i, sec) in merged_sections.iter().enumerate() {
            let sh_type = match sec.kind {
                SectionKind::Bss => SHT_NOBITS,
                SectionKind::Note => SHT_NOTE,
                _ => SHT_PROGBITS,
            };
            let mut sh_flags = 0u64;
            if sec.is_alloc() {
                sh_flags |= SHF_ALLOC;
            }
            if sec.is_writable() {
                sh_flags |= SHF_WRITE;
            }
            if sec.is_executable() {
                sh_flags |= SHF_EXECINSTR;
            }
            section_headers.push(Elf64_Shdr {
                sh_name: shdr_names[i + 1],
                sh_type,
                sh_flags,
                sh_addr: if sec.is_alloc() { sec_vaddrs[i] } else { 0 },
                sh_offset: sec_offsets[i],
                sh_size: sec.size.max(sec.data.len() as u64),
                sh_link: 0,
                sh_info: 0,
                sh_addralign: sec.alignment.max(1),
                sh_entsize: 0,
            });
        }

        if let Some(name_off) = interp_shdr_name {
            section_headers.push(Elf64_Shdr {
                sh_name: name_off,
                sh_type: SHT_PROGBITS,
                sh_flags: SHF_ALLOC,
                sh_addr: interp_va,
                sh_offset: interp_offset,
                sh_size: interp_size,
                sh_link: 0,
                sh_info: 0,
                sh_addralign: 1,
                sh_entsize: 0,
            });
        }

        if let Some(names) = dyn_section_names {
            // .rela.dyn / .rela.plt precede .dynsym in the header table.
            let dynsym_hdr_index_base = section_headers.len() as u32
                + (rela_dyn.size > 0) as u32
                + (rela_plt.size > 0) as u32;
            let dynsym_shndx = dynsym_hdr_index_base;
            let dynstr_shndx = dynsym_shndx + 1;

            if rela_dyn.size > 0 {
                section_headers.push(Elf64_Shdr {
                    sh_name: names[0],
                    sh_type: SHT_RELA,
                    sh_flags: SHF_ALLOC,
                    sh_addr: rela_dyn.va,
                    sh_offset: rela_dyn.offset,
                    sh_size: rela_dyn.size,
                    sh_link: dynsym_shndx,
                    sh_info: 0,
                    sh_addralign: 8,
                    sh_entsize: 24,
                });
            }
            if rela_plt.size > 0 {
                section_headers.push(Elf64_Shdr {
                    sh_name: names[1],
                    sh_type: SHT_RELA,
                    sh_flags: SHF_ALLOC,
                    sh_addr: rela_plt.va,
                    sh_offset: rela_plt.offset,
                    sh_size: rela_plt.size,
                    sh_link: dynsym_shndx,
                    sh_info: 0,
                    sh_addralign: 8,
                    sh_entsize: 24,
                });
            }
            if dynsym.size > 0 {
                section_headers.push(Elf64_Shdr {
                    sh_name: names[2],
                    sh_type: SHT_DYNSYM,
                    sh_flags: SHF_ALLOC,
                    sh_addr: dynsym.va,
                    sh_offset: dynsym.offset,
                    sh_size: dynsym.size,
                    sh_link: dynstr_shndx,
                    sh_info: 1,
                    sh_addralign: 8,
                    sh_entsize: 24,
                });
            }
            if dynstr.size > 0 {
                section_headers.push(Elf64_Shdr {
                    sh_name: names[3],
                    sh_type: SHT_STRTAB,
                    sh_flags: SHF_ALLOC,
                    sh_addr: dynstr.va,
                    sh_offset: dynstr.offset,
                    sh_size: dynstr.size,
                    sh_link: 0,
                    sh_info: 0,
                    sh_addralign: 1,
                    sh_entsize: 0,
                });
            }
            if hash.size > 0 {
                section_headers.push(Elf64_Shdr {
                    sh_name: names[4],
                    sh_type: SHT_HASH,
                    sh_flags: SHF_ALLOC,
                    sh_addr: hash.va,
                    sh_offset: hash.offset,
                    sh_size: hash.size,
                    sh_link: dynsym_shndx,
                    sh_info: 0,
                    sh_addralign: 4,
                    sh_entsize: 4,
                });
            }
            if gnu_hash.size > 0 {
                section_headers.push(Elf64_Shdr {
                    sh_name: names[5],
                    sh_type: SHT_GNU_HASH,
                    sh_flags: SHF_ALLOC,
                    sh_addr: gnu_hash.va,
                    sh_offset: gnu_hash.offset,
                    sh_size: gnu_hash.size,
                    sh_link: dynsym_shndx,
                    sh_info: 0,
                    sh_addralign: 4,
                    sh_entsize: 0,
                });
            }
            if dynamic.size > 0 {
                section_headers.push(Elf64_Shdr {
                    sh_name: names[6],
                    sh_type: SHT_DYNAMIC,
                    sh_flags: SHF_ALLOC | SHF_WRITE,
                    sh_addr: dynamic.va,
                    sh_offset: dynamic.offset,
                    sh_size: dynamic.size,
                    sh_link: dynstr_shndx,
                    sh_info: 0,
                    sh_addralign: 8,
                    sh_entsize: 16,
                });
            }
        }

        let strtab_shndx = section_headers.len() as u32 + 1;
        section_headers.push(Elf64_Shdr {
            sh_name: symtab_name_off,
            sh_type: SHT_SYMTAB,
            sh_flags: 0,
            sh_addr: 0,
            sh_offset: symtab_offset,
            sh_size: symtab_size,
            sh_link: strtab_shndx,
            sh_info: 1,
            sh_addralign: 8,
            sh_entsize: 24,
        });
        section_headers.push(Elf64_Shdr {
            sh_name: strtab_name_off,
            sh_type: SHT_STRTAB,
            sh_flags: 0,
            sh_addr: 0,
            sh_offset: strtab_offset,
            sh_size: strtab_size,
            sh_link: 0,
            sh_info: 0,
            sh_addralign: 1,
            sh_entsize: 0,
        });

        let shstrtab_shndx = section_headers.len() as u16;
        section_headers.push(Elf64_Shdr {
            sh_name: shstrtab_name_off,
            sh_type: SHT_STRTAB,
            sh_flags: 0,
            sh_addr: 0,
            sh_offset: shstrtab_offset,
            sh_size: shstrtab_size,
            sh_link: 0,
            sh_info: 0,
            sh_addralign: 1,
            sh_entsize: 0,
        });

        for shdr in &section_headers {
            output.extend_from_slice(&shdr.sh_name.to_le_bytes());
            output.extend_from_slice(&shdr.sh_type.to_le_bytes());
            output.extend_from_slice(&shdr.sh_flags.to_le_bytes());
            output.extend_from_slice(&shdr.sh_addr.to_le_bytes());
            output.extend_from_slice(&shdr.sh_offset.to_le_bytes());
            output.extend_from_slice(&shdr.sh_size.to_le_bytes());
            output.extend_from_slice(&shdr.sh_link.to_le_bytes());
            output.extend_from_slice(&shdr.sh_info.to_le_bytes());
            output.extend_from_slice(&shdr.sh_addralign.to_le_bytes());
            output.extend_from_slice(&shdr.sh_entsize.to_le_bytes());
        }

        // 8. ELF header.
        let mut e_ident = [0u8; 16];
        e_ident[0..4].copy_from_slice(&ELF_MAGIC);
        e_ident[4] = ELFCLASS64;
        e_ident[5] = ELFDATA2LSB;
        e_ident[6] = EV_CURRENT;
        e_ident[7] = ELFOSABI_LINUX;

        let e_type = if is_shared || is_pie { ET_DYN } else { ET_EXEC };

        let ehdr = Elf64_Ehdr {
            e_ident,
            e_type,
            e_machine,
            e_version: 1,
            e_entry: entry_va,
            e_phoff: ehdr_size as u64,
            e_shoff: shoff,
            e_flags: 0,
            e_ehsize: ehdr_size as u16,
            e_phentsize: phdr_size as u16,
            e_phnum: phnum as u16,
            e_shentsize: 64,
            e_shnum: section_headers.len() as u16,
            e_shstrndx: shstrtab_shndx,
        };

        let mut ehdr_buf = Vec::with_capacity(64);
        ehdr_buf.extend_from_slice(&ehdr.e_ident);
        ehdr_buf.extend_from_slice(&ehdr.e_type.to_le_bytes());
        ehdr_buf.extend_from_slice(&ehdr.e_machine.to_le_bytes());
        ehdr_buf.extend_from_slice(&ehdr.e_version.to_le_bytes());
        ehdr_buf.extend_from_slice(&ehdr.e_entry.to_le_bytes());
        ehdr_buf.extend_from_slice(&ehdr.e_phoff.to_le_bytes());
        ehdr_buf.extend_from_slice(&ehdr.e_shoff.to_le_bytes());
        ehdr_buf.extend_from_slice(&ehdr.e_flags.to_le_bytes());
        ehdr_buf.extend_from_slice(&ehdr.e_ehsize.to_le_bytes());
        ehdr_buf.extend_from_slice(&ehdr.e_phentsize.to_le_bytes());
        ehdr_buf.extend_from_slice(&ehdr.e_phnum.to_le_bytes());
        ehdr_buf.extend_from_slice(&ehdr.e_shentsize.to_le_bytes());
        ehdr_buf.extend_from_slice(&ehdr.e_shnum.to_le_bytes());
        ehdr_buf.extend_from_slice(&ehdr.e_shstrndx.to_le_bytes());
        output[0..64].copy_from_slice(&ehdr_buf);

        // 9. Program headers.
        let mut phdrs = Vec::new();
        phdrs.push(Elf64_Phdr {
            p_type: PT_PHDR,
            p_flags: PF_R,
            p_offset: ehdr_size as u64,
            p_vaddr: rx_vaddr_start + (ehdr_size as u64),
            p_paddr: rx_vaddr_start + (ehdr_size as u64),
            p_filesz: (phnum * phdr_size) as u64,
            p_memsz: (phnum * phdr_size) as u64,
            p_align: 8,
        });
        if interp_str.is_some() {
            phdrs.push(Elf64_Phdr {
                p_type: PT_INTERP,
                p_flags: PF_R,
                p_offset: interp_offset,
                p_vaddr: interp_va,
                p_paddr: interp_va,
                p_filesz: interp_size,
                p_memsz: interp_size,
                p_align: 1,
            });
        }
        phdrs.push(Elf64_Phdr {
            p_type: PT_LOAD,
            p_flags: PF_R | PF_X,
            p_offset: 0,
            p_vaddr: rx_vaddr_start,
            p_paddr: rx_vaddr_start,
            p_filesz: rx_file_size,
            p_memsz: rx_mem_size,
            p_align: target.page_size,
        });
        phdrs.push(Elf64_Phdr {
            p_type: PT_LOAD,
            p_flags: PF_R | PF_W,
            p_offset: rw_file_offset_start,
            p_vaddr: rw_vaddr_start,
            p_paddr: rw_vaddr_start,
            p_filesz: rw_file_size,
            p_memsz: rw_mem_size,
            p_align: target.page_size,
        });
        if is_dynamic {
            phdrs.push(Elf64_Phdr {
                p_type: PT_DYNAMIC,
                p_flags: PF_R | PF_W,
                p_offset: dynamic.offset,
                p_vaddr: dynamic.va,
                p_paddr: dynamic.va,
                p_filesz: dynamic.size,
                p_memsz: dynamic.size,
                p_align: 8,
            });
            if let Some((relro_start, relro_end)) = relro_range {
                phdrs.push(Elf64_Phdr {
                    p_type: PT_GNU_RELRO,
                    p_flags: PF_R,
                    p_offset: relro_start - target.image_base,
                    p_vaddr: relro_start,
                    p_paddr: relro_start,
                    p_filesz: relro_end - relro_start,
                    p_memsz: relro_end - relro_start,
                    p_align: 1,
                });
            }
        }
        phdrs.push(Elf64_Phdr {
            p_type: PT_GNU_STACK,
            p_flags: PF_R | PF_W,
            p_offset: 0,
            p_vaddr: 0,
            p_paddr: 0,
            p_filesz: 0,
            p_memsz: 0,
            p_align: 16,
        });
        if build_id.is_some() {
            phdrs.push(Elf64_Phdr {
                p_type: PT_NOTE,
                p_flags: PF_R,
                p_offset: note_offset,
                p_vaddr: rx_vaddr_start + note_offset,
                p_paddr: rx_vaddr_start + note_offset,
                p_filesz: note_size,
                p_memsz: note_size,
                p_align: 4,
            });
        }

        let mut phdr_buf = Vec::with_capacity(phnum * phdr_size);
        for p in &phdrs {
            phdr_buf.extend_from_slice(&p.p_type.to_le_bytes());
            phdr_buf.extend_from_slice(&p.p_flags.to_le_bytes());
            phdr_buf.extend_from_slice(&p.p_offset.to_le_bytes());
            phdr_buf.extend_from_slice(&p.p_vaddr.to_le_bytes());
            phdr_buf.extend_from_slice(&p.p_paddr.to_le_bytes());
            phdr_buf.extend_from_slice(&p.p_filesz.to_le_bytes());
            phdr_buf.extend_from_slice(&p.p_memsz.to_le_bytes());
            phdr_buf.extend_from_slice(&p.p_align.to_le_bytes());
        }
        output[64..64 + phdr_buf.len()].copy_from_slice(&phdr_buf);

        Ok(output)
    }
}
