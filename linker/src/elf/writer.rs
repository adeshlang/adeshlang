//! ELF Executable and Binary Writer.

use crate::elf::header::*;
use crate::elf::notes::create_gnu_build_id_note;
use crate::error::LinkResult;
use crate::section::{align_to, MergedSection, SectionKind};
use crate::symbol::Symbol;
use crate::target::{Arch, Target};
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
        let bytes = Self::encode_executable(target, entry_va, merged_sections, symbols, build_id)?;
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
        let e_machine = match target.arch {
            Arch::X86_64 => EM_X86_64,
            Arch::AArch64 => EM_AARCH64,
            Arch::Arm => EM_ARM,
            Arch::Riscv64 | Arch::Riscv32 => EM_RISCV,
            _ => EM_X86_64,
        };

        let mut output = Vec::new();

        // 1. Plan Headers
        let ehdr_size = 64usize;
        let phdr_size = 56usize;
        let phnum = 4usize; // PT_PHDR, PT_LOAD(RX), PT_LOAD(RW), PT_GNU_STACK
        let headers_size = ehdr_size + phnum * phdr_size;

        // Reserve space for Ehdr and Phdrs
        output.resize(headers_size, 0);

        // 2. Lay out sections
        let mut rx_sections = Vec::new();
        let mut rw_sections = Vec::new();

        for (idx, sec) in merged_sections.iter().enumerate() {
            if sec.is_executable() || (!sec.is_writable() && sec.kind != SectionKind::Bss) {
                rx_sections.push(idx);
            } else {
                rw_sections.push(idx);
            }
        }

        // Section layout tracking
        let mut sec_offsets = vec![0u64; merged_sections.len()];
        let mut sec_vaddrs = vec![0u64; merged_sections.len()];

        // RX Segment: place immediately after headers
        let mut current_offset = headers_size as u64;
        let rx_vaddr_start = target.image_base;
        let mut current_va = rx_vaddr_start + current_offset;

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

        let mut rx_file_size = current_offset;
        let mut rx_mem_size = current_offset;

        // Build ID Note if requested
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
            output.extend_from_slice(&note_data);
            current_offset += note_data.len() as u64;
            current_va += note_data.len() as u64;
            rx_file_size = current_offset;
            rx_mem_size = current_offset;
        }

        // RW Segment: page-align file offset and virtual address
        let rw_file_offset_start = align_to(current_offset, target.page_size);
        let rw_vaddr_start = align_to(current_va, target.page_size);

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

        // 3. String tables and Symbol tables
        let mut shstrtab = vec![0u8]; // start with null byte
        let mut shdr_names = Vec::new();
        shdr_names.push(0); // NULL section name

        for sec in merged_sections {
            let name_off = shstrtab.len() as u32;
            shstrtab.extend_from_slice(sec.name.as_bytes());
            shstrtab.push(0);
            shdr_names.push(name_off);
        }

        let symtab_name_off = shstrtab.len() as u32;
        shstrtab.extend_from_slice(b".symtab\0");

        let strtab_name_off = shstrtab.len() as u32;
        shstrtab.extend_from_slice(b".strtab\0");

        let shstrtab_name_off = shstrtab.len() as u32;
        shstrtab.extend_from_slice(b".shstrtab\0");

        let mut strtab = vec![0u8];
        let mut elf_syms = Vec::new();
        // Index 0: undef symbol
        elf_syms.push(Elf64_Sym {
            st_name: 0,
            st_info: 0,
            st_other: 0,
            st_shndx: 0,
            st_value: 0,
            st_size: 0,
        });

        for sym in symbols {
            if !sym.name.is_empty() {
                let st_name = strtab.len() as u32;
                strtab.extend_from_slice(sym.name.as_bytes());
                strtab.push(0);

                let bind = match sym.binding {
                    crate::symbol::SymbolBinding::Local => STB_LOCAL,
                    crate::symbol::SymbolBinding::Global => STB_GLOBAL,
                    crate::symbol::SymbolBinding::Weak => STB_WEAK,
                };
                let st_type = match sym.sym_type {
                    crate::symbol::SymbolType::Function => STT_FUNC,
                    crate::symbol::SymbolType::Object => STT_OBJECT,
                    crate::symbol::SymbolType::Tls => STT_TLS,
                    crate::symbol::SymbolType::Section => STT_SECTION,
                    crate::symbol::SymbolType::File => STT_FILE,
                    _ => STT_NOTYPE,
                };

                let st_info = (bind << 4) | (st_type & 0xF);
                let st_shndx = if sym.is_defined {
                    (sym.section_index.unwrap_or(0) + 1) as u16
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

        let total_shdrs = 1 + merged_sections.len() + 3; // NULL + sections + symtab + strtab + shstrtab
        let symtab_shndx = (1 + merged_sections.len()) as u32;
        let strtab_shndx = symtab_shndx + 1;
        let shstrtab_shndx = (total_shdrs - 1) as u16;

        let mut section_headers = Vec::with_capacity(total_shdrs);

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
        for (i, sec) in merged_sections.iter().enumerate() {
            let sh_type = if sec.kind == SectionKind::Bss {
                SHT_NOBITS
            } else if sec.kind == SectionKind::Note {
                SHT_NOTE
            } else {
                SHT_PROGBITS
            };

            let mut sh_flags = 0u64;
            if sec.is_alloc() { sh_flags |= SHF_ALLOC; }
            if sec.is_writable() { sh_flags |= SHF_WRITE; }
            if sec.is_executable() { sh_flags |= SHF_EXECINSTR; }

            section_headers.push(Elf64_Shdr {
                sh_name: shdr_names[i + 1],
                sh_type,
                sh_flags,
                sh_addr: sec_vaddrs[i],
                sh_offset: sec_offsets[i],
                sh_size: sec.size,
                sh_link: 0,
                sh_info: 0,
                sh_addralign: sec.alignment,
                sh_entsize: 0,
            });
        }

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

        // 4. Fill in ELF Header (Ehdr) at offset 0
        let mut e_ident = [0u8; 16];
        e_ident[0..4].copy_from_slice(&ELF_MAGIC);
        e_ident[4] = ELFCLASS64;
        e_ident[5] = ELFDATA2LSB;
        e_ident[6] = EV_CURRENT;
        e_ident[7] = ELFOSABI_LINUX;

        let ehdr = Elf64_Ehdr {
            e_ident,
            e_type: ET_EXEC,
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
            e_shnum: total_shdrs as u16,
            e_shstrndx: shstrtab_shndx,
        };

        // Write Ehdr into byte buffer at 0
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

        // 5. Fill in Program Headers (Phdrs) at offset 64
        let phdrs = [
            // Phdr 0: PT_PHDR
            Elf64_Phdr {
                p_type: PT_PHDR,
                p_flags: PF_R,
                p_offset: ehdr_size as u64,
                p_vaddr: rx_vaddr_start + (ehdr_size as u64),
                p_paddr: rx_vaddr_start + (ehdr_size as u64),
                p_filesz: (phnum * phdr_size) as u64,
                p_memsz: (phnum * phdr_size) as u64,
                p_align: 8,
            },
            // Phdr 1: PT_LOAD (RX)
            Elf64_Phdr {
                p_type: PT_LOAD,
                p_flags: PF_R | PF_X,
                p_offset: 0,
                p_vaddr: rx_vaddr_start,
                p_paddr: rx_vaddr_start,
                p_filesz: rx_file_size,
                p_memsz: rx_mem_size,
                p_align: target.page_size,
            },
            // Phdr 2: PT_LOAD (RW)
            Elf64_Phdr {
                p_type: PT_LOAD,
                p_flags: PF_R | PF_W,
                p_offset: rw_file_offset_start,
                p_vaddr: rw_vaddr_start,
                p_paddr: rw_vaddr_start,
                p_filesz: rw_file_size,
                p_memsz: rw_mem_size,
                p_align: target.page_size,
            },
            // Phdr 3: PT_GNU_STACK (NX stack)
            Elf64_Phdr {
                p_type: PT_GNU_STACK,
                p_flags: PF_R | PF_W,
                p_offset: 0,
                p_vaddr: 0,
                p_paddr: 0,
                p_filesz: 0,
                p_memsz: 0,
                p_align: 16,
            },
        ];

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
        output[64..64 + phnum * phdr_size].copy_from_slice(&phdr_buf);

        Ok(output)
    }
}
