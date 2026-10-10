//! ELF Executable, Shared Library, and Binary Writer.

use crate::elf::header::*;
use crate::elf::notes::create_gnu_build_id_note;
use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::section::{MergedSection, SectionKind, align_to};
use crate::symbol::{Symbol, SymbolBinding, SymbolType};
use crate::target::{Arch, Target};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub struct ElfWriter;

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
            &[],
            None,
        )
    }

    pub fn write_elf(
        path: &Path,
        target: &Target,
        entry_va: u64,
        merged_sections: &[MergedSection],
        symbols: &[Symbol],
        build_id: Option<&[u8]>,
        is_shared: bool,
        needed_libs: &[String],
        soname: Option<&str>,
    ) -> LinkResult<()> {
        let bytes = Self::encode_elf(
            target,
            entry_va,
            merged_sections,
            symbols,
            build_id,
            is_shared,
            needed_libs,
            soname,
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
            &[],
            None,
        )
    }

    pub fn encode_elf(
        target: &Target,
        mut entry_va: u64,
        merged_sections: &[MergedSection],
        symbols: &[Symbol],
        build_id: Option<&[u8]>,
        is_shared: bool,
        needed_libs: &[String],
        soname: Option<&str>,
    ) -> LinkResult<Vec<u8>> {
        let e_machine = match target.arch {
            Arch::X86_64 => EM_X86_64,
            Arch::AArch64 => EM_AARCH64,
            Arch::Arm => EM_ARM,
            Arch::Riscv64 | Arch::Riscv32 => EM_RISCV,
            _ => EM_X86_64,
        };

        let is_dynamic = is_shared || !needed_libs.is_empty();
        let interp_str: Option<&str> = if is_dynamic && !is_shared {
            match target.arch {
                Arch::AArch64 => Some("/lib/ld-linux-aarch64.so.1"),
                Arch::Riscv64 => Some("/lib/ld-linux-riscv64-lp64d.so.1"),
                _ => Some("/lib64/ld-linux-x86-64.so.2"),
            }
        } else {
            None
        };

        let mut output = Vec::new();

        // 1. Plan Headers
        let ehdr_size = 64usize;
        let phdr_size = 56usize;

        let mut phnum = 4usize; // PT_PHDR, PT_LOAD(RX), PT_LOAD(RW), PT_GNU_STACK
        if interp_str.is_some() {
            phnum += 1; // PT_INTERP
        }
        if is_dynamic {
            phnum += 2; // PT_DYNAMIC, PT_GNU_RELRO
        }
        if build_id.is_some() {
            phnum += 1; // PT_NOTE
        }

        let headers_size = ehdr_size + phnum * phdr_size;
        output.resize(headers_size, 0);

        // 2. Resolve ELF entry address if not explicitly set
        let text_sections: &[MergedSection] = merged_sections;
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

        // 3. Lay out sections

        let mut rx_sections = Vec::new();
        let mut rw_sections = Vec::new();

        for (idx, sec) in text_sections.iter().enumerate() {
            if !sec.is_alloc() {
                continue;
            }
            if sec.is_executable() || (!sec.is_writable() && sec.kind != SectionKind::Bss) {
                rx_sections.push(idx);
            } else {
                rw_sections.push(idx);
            }
        }

        let mut sec_offsets = vec![0u64; text_sections.len()];
        let mut sec_vaddrs = vec![0u64; text_sections.len()];

        // RX Segment: place immediately after headers
        let mut current_offset = headers_size as u64;
        let rx_vaddr_start = target.image_base;
        let mut current_va = rx_vaddr_start + current_offset;

        // PT_INTERP section if dynamic executable
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
            let sec = &text_sections[sec_idx];
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

        // Build ID Note if requested
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

        // RW Segment: page-align file offset and virtual address
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
            let sec = &text_sections[sec_idx];
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

        // Non-alloc sections (e.g. .debug_line, .debug_info, .debug_abbrev, .debug_str)
        for (idx, sec) in text_sections.iter().enumerate() {
            if !sec.is_alloc() {
                let aligned_offset = align_to(current_offset, sec.alignment.max(1));
                if aligned_offset > current_offset {
                    let pad = (aligned_offset - current_offset) as usize;
                    output.resize(output.len() + pad, 0);
                    current_offset = aligned_offset;
                }
                sec_offsets[idx] = current_offset;
                sec_vaddrs[idx] = 0;
                output.extend_from_slice(&sec.data);
                current_offset += sec.data.len() as u64;
            }
        }

        // Authoritative placement when virtual addresses are assigned
        let has_layout_vas = text_sections.iter().any(|s| s.virtual_address != 0);
        if has_layout_vas {
            output.truncate(headers_size);

            let mut rx_file_end = headers_size as u64;
            let mut rx_mem_end = headers_size as u64;
            let mut rw_off_min: Option<u64> = None;
            let mut rw_va_min: Option<u64> = None;
            let mut rw_file_end = 0u64;
            let mut rw_mem_end = 0u64;
            let mut non_alloc_sections = Vec::new();

            for (idx, sec) in text_sections.iter().enumerate() {
                if !sec.is_alloc() {
                    non_alloc_sections.push(idx);
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
                    rw_mem_end = rw_mem_end.max(off + mem_len);
                }
            }

            if let Some(bid) = build_id {
                let note_data = create_gnu_build_id_note(bid);
                let off = align_to(headers_size as u64, 8);
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
            rw_file_offset_start = rw_off;
            rw_vaddr_start = rw_va;
            rw_file_size = rw_file_end.saturating_sub(rw_off);
            rw_mem_size = rw_mem_end.saturating_sub(rw_va);

            // Emit non-alloc sections after loadable segments
            for idx in non_alloc_sections {
                let sec = &text_sections[idx];
                let off = align_to(output.len() as u64, sec.alignment.max(1));
                if off > output.len() as u64 {
                    output.resize(off as usize, 0);
                }
                sec_offsets[idx] = off;
                sec_vaddrs[idx] = 0;
                output.extend_from_slice(&sec.data);
            }
        }

        // 4. Dynamic Linking Syntheses (.dynstr, .dynsym, .hash, .dynamic)
        let mut dynstr = vec![0u8];
        let mut dynsyms = Vec::new();
        dynsyms.push(Elf64_Sym {
            st_name: 0,
            st_info: 0,
            st_other: 0,
            st_shndx: 0,
            st_value: 0,
            st_size: 0,
        });

        let mut dyn_offset = 0u64;
        let mut dyn_size = 0u64;
        let mut dyn_va = 0u64;
        let mut dynstr_offset = 0u64;
        let mut dynstr_size = 0u64;
        let mut dynstr_va = 0u64;
        let mut dynsym_offset = 0u64;
        let mut dynsym_size = 0u64;
        let mut dynsym_va = 0u64;
        let mut hash_offset = 0u64;
        let mut hash_size = 0u64;
        let mut hash_va = 0u64;

        if is_dynamic {
            let mut dyn_entries: Vec<Elf64_Dyn> = Vec::new();

            // Needed libraries (DT_NEEDED)
            let mut all_libs = vec![
                "libc.so.6".to_string(),
                "libm.so.6".to_string(),
                "libgcc_s.so.1".to_string(),
            ];
            for lib in needed_libs {
                let name = if lib.ends_with(".so") || lib.contains(".so.") {
                    lib.clone()
                } else {
                    format!("lib{}.so", lib)
                };
                if !all_libs.contains(&name) {
                    all_libs.push(name);
                }
            }

            for lib_name in &all_libs {
                let str_idx = dynstr.len() as u64;
                dynstr.extend_from_slice(lib_name.as_bytes());
                dynstr.push(0);
                dyn_entries.push(Elf64_Dyn {
                    d_tag: DT_NEEDED,
                    d_val: str_idx,
                });
            }

            // Soname (DT_SONAME)
            if let Some(sn) = soname {
                let str_idx = dynstr.len() as u64;
                dynstr.extend_from_slice(sn.as_bytes());
                dynstr.push(0);
                dyn_entries.push(Elf64_Dyn {
                    d_tag: DT_SONAME,
                    d_val: str_idx,
                });
            }

            // Dynamic symbols (exported global functions and variables)
            for sym in symbols {
                if !sym.name.is_empty() && (sym.is_exported || sym.binding == SymbolBinding::Global)
                {
                    let st_name = dynstr.len() as u32;
                    dynstr.extend_from_slice(sym.name.as_bytes());
                    dynstr.push(0);

                    let bind = STB_GLOBAL;
                    let st_type = match sym.sym_type {
                        SymbolType::Function => STT_FUNC,
                        SymbolType::Object => STT_OBJECT,
                        _ => STT_NOTYPE,
                    };
                    let st_info = (bind << 4) | (st_type & 0xF);

                    dynsyms.push(Elf64_Sym {
                        st_name,
                        st_info,
                        st_other: 0,
                        st_shndx: if sym.is_defined { 1 } else { 0 },
                        st_value: sym.value,
                        st_size: sym.size,
                    });
                }
            }

            // Layout dynamic tables in RW segment
            if (output.len() as u64) < rw_file_offset_start {
                output.resize(rw_file_offset_start as usize, 0);
            }
            let dyn_align_off = align_to(output.len() as u64, 8);
            if dyn_align_off > output.len() as u64 {
                output.resize(dyn_align_off as usize, 0);
            }

            // .dynsym
            dynsym_offset = output.len() as u64;
            dynsym_va = rw_vaddr_start + (dynsym_offset - rw_file_offset_start);
            dynsym_size = (dynsyms.len() * 24) as u64;
            for s in &dynsyms {
                output.extend_from_slice(&s.st_name.to_le_bytes());
                output.push(s.st_info);
                output.push(s.st_other);
                output.extend_from_slice(&s.st_shndx.to_le_bytes());
                output.extend_from_slice(&s.st_value.to_le_bytes());
                output.extend_from_slice(&s.st_size.to_le_bytes());
            }

            // .dynstr
            dynstr_offset = output.len() as u64;
            dynstr_va = rw_vaddr_start + (dynstr_offset - rw_file_offset_start);
            dynstr_size = dynstr.len() as u64;
            output.extend_from_slice(&dynstr);

            // .hash table (SYSV ELF Hash)
            hash_offset = align_to(output.len() as u64, 4);
            if hash_offset > output.len() as u64 {
                output.resize(hash_offset as usize, 0);
            }
            hash_va = rw_vaddr_start + (hash_offset - rw_file_offset_start);
            let nbucket = (dynsyms.len().max(1)) as u32;
            let nchain = dynsyms.len() as u32;
            let mut buckets = vec![0u32; nbucket as usize];
            let mut chains = vec![0u32; nchain as usize];

            for i in 1..dynsyms.len() {
                let name_off = dynsyms[i].st_name as usize;
                let end = dynstr[name_off..]
                    .iter()
                    .position(|&b| b == 0)
                    .map(|p| name_off + p)
                    .unwrap_or(dynstr.len());
                let name = &dynstr[name_off..end];
                let h = (elf_hash(name) % nbucket) as usize;
                chains[i] = buckets[h];
                buckets[h] = i as u32;
            }

            output.extend_from_slice(&nbucket.to_le_bytes());
            output.extend_from_slice(&nchain.to_le_bytes());
            for b in &buckets {
                output.extend_from_slice(&b.to_le_bytes());
            }
            for c in &chains {
                output.extend_from_slice(&c.to_le_bytes());
            }
            hash_size = (output.len() as u64) - hash_offset;

            // .dynamic section
            dyn_offset = align_to(output.len() as u64, 8);
            if dyn_offset > output.len() as u64 {
                output.resize(dyn_offset as usize, 0);
            }
            dyn_va = rw_vaddr_start + (dyn_offset - rw_file_offset_start);

            dyn_entries.push(Elf64_Dyn {
                d_tag: DT_STRTAB,
                d_val: dynstr_va,
            });
            dyn_entries.push(Elf64_Dyn {
                d_tag: DT_SYMTAB,
                d_val: dynsym_va,
            });
            dyn_entries.push(Elf64_Dyn {
                d_tag: DT_STRSZ,
                d_val: dynstr_size,
            });
            dyn_entries.push(Elf64_Dyn {
                d_tag: DT_SYMENT,
                d_val: 24,
            });
            dyn_entries.push(Elf64_Dyn {
                d_tag: DT_HASH,
                d_val: hash_va,
            });
            if is_shared {
                dyn_entries.push(Elf64_Dyn {
                    d_tag: DT_FLAGS_1,
                    d_val: DF_1_PIE,
                });
            }
            dyn_entries.push(Elf64_Dyn {
                d_tag: DT_NULL,
                d_val: 0,
            });

            dyn_size = (dyn_entries.len() * 16) as u64;
            for e in &dyn_entries {
                output.extend_from_slice(&e.d_tag.to_le_bytes());
                output.extend_from_slice(&e.d_val.to_le_bytes());
            }

            rw_file_size = (output.len() as u64).saturating_sub(rw_file_offset_start);
            rw_mem_size = rw_mem_size.max(rw_file_size);
        }

        // 5. String tables and Symbol tables
        let mut shstrtab = vec![0u8];
        let mut shdr_names = Vec::new();
        shdr_names.push(0);

        for sec in text_sections {
            let name_off = shstrtab.len() as u32;
            shstrtab.extend_from_slice(sec.name.as_bytes());
            shstrtab.push(0);
            shdr_names.push(name_off);
        }

        let interp_shdr_name = if interp_str.is_some() {
            let off = shstrtab.len() as u32;
            shstrtab.extend_from_slice(b".interp\0");
            Some(off)
        } else {
            None
        };

        let (dynsym_shdr_name, dynstr_shdr_name, hash_shdr_name, dynamic_shdr_name) = if is_dynamic
        {
            let s1 = shstrtab.len() as u32;
            shstrtab.extend_from_slice(b".dynsym\0");
            let s2 = shstrtab.len() as u32;
            shstrtab.extend_from_slice(b".dynstr\0");
            let s3 = shstrtab.len() as u32;
            shstrtab.extend_from_slice(b".hash\0");
            let s4 = shstrtab.len() as u32;
            shstrtab.extend_from_slice(b".dynamic\0");
            (Some(s1), Some(s2), Some(s3), Some(s4))
        } else {
            (None, None, None, None)
        };

        let symtab_name_off = shstrtab.len() as u32;
        shstrtab.extend_from_slice(b".symtab\0");

        let strtab_name_off = shstrtab.len() as u32;
        shstrtab.extend_from_slice(b".strtab\0");

        let shstrtab_name_off = shstrtab.len() as u32;
        shstrtab.extend_from_slice(b".shstrtab\0");

        let mut strtab = vec![0u8];
        let mut elf_syms = Vec::new();
        elf_syms.push(Elf64_Sym {
            st_name: 0,
            st_info: 0,
            st_other: 0,
            st_shndx: 0,
            st_value: 0,
            st_size: 0,
        });

        let mut input_sec_to_out: HashMap<(usize, usize), usize> = HashMap::new();
        for (out_idx, sec) in text_sections.iter().enumerate() {
            for &(f_idx, s_idx, _) in &sec.input_sections {
                input_sec_to_out.insert((f_idx, s_idx), out_idx);
            }
        }
        let output_section_index = |sym: &Symbol| -> Option<u16> {
            sym.file_index
                .zip(sym.section_index)
                .and_then(|key| input_sec_to_out.get(&key).copied())
                .or_else(|| {
                    text_sections.iter().position(|sec| {
                        sec.size > 0
                            && sym.value >= sec.virtual_address
                            && sym.value < sec.virtual_address + sec.size
                    })
                })
                .map(|idx| (idx + 1) as u16)
        };

        for sym in symbols {
            if !sym.name.is_empty() {
                let st_name = strtab.len() as u32;
                strtab.extend_from_slice(sym.name.as_bytes());
                strtab.push(0);

                let bind = match sym.binding {
                    SymbolBinding::Local => STB_LOCAL,
                    SymbolBinding::Global => STB_GLOBAL,
                    SymbolBinding::Weak => STB_WEAK,
                };
                let st_type = match sym.sym_type {
                    SymbolType::Function => STT_FUNC,
                    SymbolType::Object => STT_OBJECT,
                    SymbolType::Tls => STT_TLS,
                    SymbolType::Section => STT_SECTION,
                    SymbolType::File => STT_FILE,
                    _ => STT_NOTYPE,
                };

                let st_info = (bind << 4) | (st_type & 0xF);
                let st_shndx = if sym.is_defined {
                    output_section_index(sym).unwrap_or(0)
                } else {
                    0
                };

                elf_syms.push(Elf64_Sym {
                    st_name,
                    st_info,
                    st_other: 0,
                    st_shndx,
                    st_value: sym.value,
                    st_size: sym.size,
                });
            }
        }

        // Align and write symtab, strtab, shstrtab
        let symtab_offset = align_to(output.len() as u64, 8);
        if symtab_offset > output.len() as u64 {
            output.resize(symtab_offset as usize, 0);
        }
        let symtab_size = (elf_syms.len() * 24) as u64;
        for s in &elf_syms {
            output.extend_from_slice(&s.st_name.to_le_bytes());
            output.push(s.st_info);
            output.push(s.st_other);
            output.extend_from_slice(&s.st_shndx.to_le_bytes());
            output.extend_from_slice(&s.st_value.to_le_bytes());
            output.extend_from_slice(&s.st_size.to_le_bytes());
        }

        let strtab_offset = output.len() as u64;
        let strtab_size = strtab.len() as u64;
        output.extend_from_slice(&strtab);

        let shstrtab_offset = output.len() as u64;
        let shstrtab_size = shstrtab.len() as u64;
        output.extend_from_slice(&shstrtab);

        // Section header table offset
        let shoff = align_to(output.len() as u64, 8);
        if shoff > output.len() as u64 {
            output.resize(shoff as usize, 0);
        }

        let mut section_headers = Vec::new();

        // Shdr 0: NULL
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

        // Shdrs for merged sections
        for (i, sec) in text_sections.iter().enumerate() {
            let sh_type = if sec.kind == SectionKind::Bss {
                SHT_NOBITS
            } else if sec.kind == SectionKind::Note {
                SHT_NOTE
            } else {
                SHT_PROGBITS
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

        // Shdr: .interp
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

        // Shdrs: .dynsym, .dynstr, .hash, .dynamic
        let dynstr_shndx = (section_headers.len() + 1) as u32;
        if let (Some(s1), Some(s2), Some(s3), Some(s4)) = (
            dynsym_shdr_name,
            dynstr_shdr_name,
            hash_shdr_name,
            dynamic_shdr_name,
        ) {
            section_headers.push(Elf64_Shdr {
                sh_name: s1,
                sh_type: SHT_DYNSYM,
                sh_flags: SHF_ALLOC,
                sh_addr: dynsym_va,
                sh_offset: dynsym_offset,
                sh_size: dynsym_size,
                sh_link: dynstr_shndx,
                sh_info: 1,
                sh_addralign: 8,
                sh_entsize: 24,
            });
            section_headers.push(Elf64_Shdr {
                sh_name: s2,
                sh_type: SHT_STRTAB,
                sh_flags: SHF_ALLOC,
                sh_addr: dynstr_va,
                sh_offset: dynstr_offset,
                sh_size: dynstr_size,
                sh_link: 0,
                sh_info: 0,
                sh_addralign: 1,
                sh_entsize: 0,
            });
            section_headers.push(Elf64_Shdr {
                sh_name: s3,
                sh_type: SHT_HASH,
                sh_flags: SHF_ALLOC,
                sh_addr: hash_va,
                sh_offset: hash_offset,
                sh_size: hash_size,
                sh_link: (section_headers.len() - 2) as u32,
                sh_info: 0,
                sh_addralign: 4,
                sh_entsize: 4,
            });
            section_headers.push(Elf64_Shdr {
                sh_name: s4,
                sh_type: SHT_DYNAMIC,
                sh_flags: SHF_ALLOC | SHF_WRITE,
                sh_addr: dyn_va,
                sh_offset: dyn_offset,
                sh_size: dyn_size,
                sh_link: dynstr_shndx,
                sh_info: 0,
                sh_addralign: 8,
                sh_entsize: 16,
            });
        }

        let strtab_shndx = (section_headers.len() + 1) as u32;

        // Shdr: .symtab
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

        // Shdr: .strtab
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

        // Shdr: .shstrtab
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

        // Append Section Headers to output
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

        // 6. Fill in ELF Header (Ehdr) at offset 0
        let mut e_ident = [0u8; 16];
        e_ident[0..4].copy_from_slice(&ELF_MAGIC);
        e_ident[4] = ELFCLASS64;
        e_ident[5] = ELFDATA2LSB;
        e_ident[6] = EV_CURRENT;
        e_ident[7] = ELFOSABI_LINUX;

        let e_type = if is_shared { ET_DYN } else { ET_EXEC };

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

        // 7. Fill in Program Headers (Phdrs) at offset 64
        let mut phdrs = Vec::new();

        // Phdr 0: PT_PHDR
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

        // PT_INTERP
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

        // PT_LOAD (RX)
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

        // PT_LOAD (RW)
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

        // PT_DYNAMIC
        if is_dynamic {
            phdrs.push(Elf64_Phdr {
                p_type: PT_DYNAMIC,
                p_flags: PF_R | PF_W,
                p_offset: dyn_offset,
                p_vaddr: dyn_va,
                p_paddr: dyn_va,
                p_filesz: dyn_size,
                p_memsz: dyn_size,
                p_align: 8,
            });

            // PT_GNU_RELRO
            phdrs.push(Elf64_Phdr {
                p_type: PT_GNU_RELRO,
                p_flags: PF_R,
                p_offset: dyn_offset,
                p_vaddr: dyn_va,
                p_paddr: dyn_va,
                p_filesz: dyn_size,
                p_memsz: dyn_size,
                p_align: 1,
            });
        }

        // PT_GNU_STACK (NX stack)
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

        // PT_NOTE (GNU Build ID)
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
