//! ELF32 (ILP32) static executable writer.
//!
//! Emits a minimal but complete ELFCLASS32 ET_EXEC image with PT_LOAD segments,
//! PT_GNU_STACK, section headers, and a symbol table. This is the executable
//! path for 32-bit targets (i686, RV32, ARM32) where the main [`super::writer`]
//! is 64-bit-only.
//!
//! Memory-safe (`#![forbid(unsafe_code)]`), type-safe, no heap beyond the
//! output buffer and string tables, minimal executable size.

#![forbid(unsafe_code)]

use crate::elf::header::*;
use crate::elf::notes::create_gnu_build_id_note;
use crate::elf::reader::ElfByteOrder;
use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::section::{MergedSection, SectionKind};
use crate::symbol::{Symbol, SymbolBinding, SymbolType};
use crate::target::Target;

const EHDR_SIZE: usize = 52;
const PHDR_SIZE: usize = 32;
const SHDR_SIZE: usize = 40;
const SYM_SIZE: usize = 16;

fn u32_checked(value: u64, what: &str) -> LinkResult<u32> {
    u32::try_from(value).map_err(|_| {
        LinkError::new(
            ErrorCode::InvalidTarget,
            format!("{what} 0x{value:x} does not fit in a 32-bit ELF field"),
        )
    })
}

fn write_u16(out: &mut [u8], off: usize, value: u16, bo: ElfByteOrder) {
    let bytes = match bo {
        ElfByteOrder::Little => value.to_le_bytes(),
        ElfByteOrder::Big => value.to_be_bytes(),
    };
    out[off..off + 2].copy_from_slice(&bytes);
}

fn write_u32(out: &mut [u8], off: usize, value: u32, bo: ElfByteOrder) {
    let bytes = match bo {
        ElfByteOrder::Little => value.to_le_bytes(),
        ElfByteOrder::Big => value.to_be_bytes(),
    };
    out[off..off + 4].copy_from_slice(&bytes);
}

fn push_u16(out: &mut Vec<u8>, value: u16, bo: ElfByteOrder) {
    let bytes = match bo {
        ElfByteOrder::Little => value.to_le_bytes(),
        ElfByteOrder::Big => value.to_be_bytes(),
    };
    out.extend_from_slice(&bytes);
}

fn push_u32(out: &mut Vec<u8>, value: u32, bo: ElfByteOrder) {
    let bytes = match bo {
        ElfByteOrder::Little => value.to_le_bytes(),
        ElfByteOrder::Big => value.to_be_bytes(),
    };
    out.extend_from_slice(&bytes);
}

struct SectionPlan {
    offset: u32,
    va: u32,
    size: u32,
    alignment: u32,
    sh_type: u32,
    sh_flags: u32,
    entsize: u32,
    link: u32,
    info: u32,
    name_off: u32,
}

fn place_section(
    sec_idx: usize,
    sec: &MergedSection,
    output: &mut Vec<u8>,
    current_offset: &mut u64,
    sec_offsets: &mut [u32],
    sec_vaddrs: &mut [u32],
    image_base: u64,
) -> LinkResult<(u64, u64, u64)> {
    let section_offset = sec.virtual_address.checked_sub(image_base).ok_or_else(|| {
        LinkError::new(
            ErrorCode::InvalidSection,
            format!(
                "ELF32 section `{}` lies below image base 0x{image_base:x}",
                sec.name
            ),
        )
    })?;
    let alignment = sec.alignment.max(1);
    if section_offset % alignment != 0 {
        return Err(LinkError::new(
            ErrorCode::InvalidSection,
            format!(
                "ELF32 section `{}` address 0x{:x} is not {alignment}-byte aligned",
                sec.name, sec.virtual_address
            ),
        ));
    }

    let va32 = u32_checked(sec.virtual_address, "ELF32 section address")?;
    sec_vaddrs[sec_idx] = va32;

    if sec.kind == SectionKind::Bss || sec.kind == SectionKind::TBss {
        // SHT_NOBITS occupies virtual memory but zero bytes in the file.
        let offset32 = u32_checked(*current_offset, "ELF32 BSS section offset")?;
        sec_offsets[sec_idx] = offset32;
        let mem_end = sec.virtual_address.checked_add(sec.size).ok_or_else(|| {
            LinkError::new(
                ErrorCode::InvalidTarget,
                "ELF32 BSS size overflowed".to_string(),
            )
        })?;
        return Ok((*current_offset, *current_offset, mem_end));
    }

    if section_offset < *current_offset {
        return Err(LinkError::new(
            ErrorCode::InvalidSection,
            format!(
                "ELF32 section `{}` overlaps previously emitted content",
                sec.name
            ),
        ));
    }
    if section_offset > *current_offset {
        output.resize(section_offset as usize, 0);
        *current_offset = section_offset;
    }
    let offset32 = u32_checked(*current_offset, "ELF32 section offset")?;
    sec_offsets[sec_idx] = offset32;

    let end = current_offset
        .checked_add(sec.data.len() as u64)
        .ok_or_else(|| {
            LinkError::new(
                ErrorCode::InvalidTarget,
                "ELF32 section size overflowed".to_string(),
            )
        })?;
    if output.len() < end as usize {
        output.resize(end as usize, 0);
    }
    output[*current_offset as usize..end as usize].copy_from_slice(&sec.data);
    *current_offset = end;
    let mem_end = sec
        .virtual_address
        .checked_add(sec.size.max(sec.data.len() as u64))
        .ok_or_else(|| {
            LinkError::new(
                ErrorCode::InvalidTarget,
                "ELF32 section memory size overflowed".to_string(),
            )
        })?;
    Ok((u64::from(offset32), end, mem_end))
}

/// Encode a static ELF32 executable.
pub fn encode_elf32(
    target: &Target,
    mut entry_va: u64,
    merged_sections: &[MergedSection],
    symbols: &[Symbol],
    build_id: Option<&[u8]>,
    e_machine: u16,
    bo: ElfByteOrder,
) -> LinkResult<Vec<u8>> {
    use std::collections::HashMap;

    let image_base = target.image_base;

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
                    .unwrap_or(image_base + 0x1000)
            });
    }
    let entry32 = u32_checked(entry_va, "ELF32 entry address")?;

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

    let has_note = build_id.is_some();
    let has_rw = !rw_sections.is_empty();
    let phnum: usize = 3 + usize::from(has_rw) + usize::from(has_note);
    let headers_size = EHDR_SIZE + phnum * PHDR_SIZE;
    let mut output = vec![0u8; headers_size];

    let mut current_offset = headers_size as u64;

    let note_data = build_id.map(create_gnu_build_id_note);
    let (note_offset, note_size) = if let Some(ref note) = note_data {
        let off = current_offset;
        let va = u32_checked(
            image_base.checked_add(off).ok_or_else(|| {
                LinkError::new(
                    ErrorCode::InvalidTarget,
                    "ELF32 image base + note offset overflowed".to_string(),
                )
            })?,
            "ELF32 note address",
        )?;
        output.extend_from_slice(note);
        current_offset += note.len() as u64;
        (
            u32_checked(off, "ELF32 note offset")?,
            (va, note.len() as u32),
        )
    } else {
        (0, (0, 0))
    };

    let mut sec_offsets = vec![0u32; merged_sections.len()];
    let mut sec_vaddrs = vec![0u32; merged_sections.len()];

    for &sec_idx in &rx_sections {
        place_section(
            sec_idx,
            &merged_sections[sec_idx],
            &mut output,
            &mut current_offset,
            &mut sec_offsets,
            &mut sec_vaddrs,
            image_base,
        )?;
    }
    let rx_file_end = current_offset;

    let mut rw_start_off: Option<u64> = None;
    let mut rw_start_va: Option<u64> = None;
    let mut rw_file_end = current_offset;
    let mut rw_mem_end = 0u64;
    for &sec_idx in &rw_sections {
        let sec = &merged_sections[sec_idx];
        let (sec_off, sec_file_end, sec_mem_end) = place_section(
            sec_idx,
            sec,
            &mut output,
            &mut current_offset,
            &mut sec_offsets,
            &mut sec_vaddrs,
            image_base,
        )?;
        rw_start_off.get_or_insert(sec_off);
        rw_start_va.get_or_insert(sec.virtual_address);
        if sec.kind != SectionKind::Bss && sec.kind != SectionKind::TBss {
            rw_file_end = rw_file_end.max(sec_file_end);
        }
        rw_mem_end = rw_mem_end.max(sec_mem_end);
    }

    while current_offset % 4 != 0 {
        output.push(0);
        current_offset += 1;
    }

    let mut shstr = Vec::new();
    let mut shstr_offsets = Vec::new();
    shstr.push(0);
    for sec in merged_sections {
        shstr_offsets.push(shstr.len() as u32);
        shstr.extend_from_slice(sec.name.as_bytes());
        shstr.push(0);
    }
    let note_name_off = if has_note {
        let off = shstr.len() as u32;
        shstr.extend_from_slice(b".note.gnu.build-id\0");
        off
    } else {
        0
    };
    let shstr_name_off = shstr.len() as u32;
    shstr.extend_from_slice(b".shstrtab\0");
    let symtab_name_off = shstr.len() as u32;
    shstr.extend_from_slice(b".symtab\0");
    let strtab_name_off = shstr.len() as u32;
    shstr.extend_from_slice(b".strtab\0");
    let shstrtab_idx = 1 + has_note as usize + rx_sections.len() + rw_sections.len();

    let shstr_off = u32_checked(current_offset, "ELF32 .shstrtab offset")?;
    output.extend_from_slice(&shstr);
    current_offset += shstr.len() as u64;

    while current_offset % 4 != 0 {
        output.push(0);
        current_offset += 1;
    }

    // Build plans first so we know exact output section header indices (`shndx`)
    // for every merged section before writing `.symtab`.
    let mut plans = Vec::new();
    plans.push(SectionPlan {
        offset: 0,
        va: 0,
        size: 0,
        alignment: 0,
        sh_type: SHT_NULL,
        sh_flags: 0,
        entsize: 0,
        link: 0,
        info: 0,
        name_off: 0,
    });

    if has_note {
        plans.push(SectionPlan {
            offset: note_offset,
            va: note_size.0,
            size: note_size.1,
            alignment: 4,
            sh_type: SHT_NOTE,
            sh_flags: SHF_ALLOC as u32,
            entsize: 0,
            link: 0,
            info: 0,
            name_off: note_name_off,
        });
    }

    let mut merged_to_shndx = vec![None; merged_sections.len()];
    for &idx in rx_sections.iter().chain(rw_sections.iter()) {
        let sec = &merged_sections[idx];
        let mut flags = SHF_ALLOC;
        if sec.is_writable() {
            flags |= SHF_WRITE;
        }
        if sec.is_executable() {
            flags |= SHF_EXECINSTR;
        }
        let shndx = u16::try_from(plans.len()).map_err(|_| {
            LinkError::new(
                ErrorCode::InvalidSection,
                "ELF32 section header count exceeds u16::MAX".to_string(),
            )
        })?;
        merged_to_shndx[idx] = Some(shndx);
        plans.push(SectionPlan {
            offset: sec_offsets[idx],
            va: sec_vaddrs[idx],
            size: u32_checked(sec.size, "ELF32 section size")?,
            alignment: u32_checked(sec.alignment, "ELF32 section alignment")?,
            sh_type: if sec.kind == SectionKind::Bss || sec.kind == SectionKind::TBss {
                SHT_NOBITS
            } else {
                SHT_PROGBITS
            },
            sh_flags: flags as u32,
            entsize: 0,
            link: 0,
            info: 0,
            name_off: *shstr_offsets.get(idx).unwrap_or(&0),
        });
    }

    let mut input_sec_to_out: HashMap<(usize, usize), usize> = HashMap::new();
    for (out_idx, sec) in merged_sections.iter().enumerate() {
        for &(f_idx, s_idx, _) in &sec.input_sections {
            input_sec_to_out.insert((f_idx, s_idx), out_idx);
        }
    }
    let section_index_for_symbol = |sym: &Symbol| -> Option<u16> {
        sym.file_index
            .zip(sym.section_index)
            .and_then(|key| input_sec_to_out.get(&key).copied())
            .or_else(|| {
                merged_sections.iter().position(|sec| {
                    sec.is_alloc()
                        && sec.size > 0
                        && sym.value >= sec.virtual_address
                        && sym.value < sec.virtual_address + sec.size
                })
            })
            .and_then(|out_idx| merged_to_shndx[out_idx])
    };

    let mut strtab = Vec::new();
    strtab.push(0);
    let mut local_entries: Vec<(u32, u8, u8, u16, u32, u32)> = Vec::new();
    let mut nonlocal_entries: Vec<(u32, u8, u8, u16, u32, u32)> = Vec::new();

    for sym in symbols {
        if sym.name.is_empty() {
            continue;
        }
        let name_off = strtab.len() as u32;
        strtab.extend_from_slice(sym.name.as_bytes());
        strtab.push(0);
        let bind = match sym.binding {
            SymbolBinding::Local => 0u8,
            SymbolBinding::Global => 1u8,
            SymbolBinding::Weak => 2u8,
        };
        let sym_type = match sym.sym_type {
            SymbolType::Function => 2u8,
            SymbolType::Object => 1u8,
            SymbolType::Tls => 6u8,
            SymbolType::Section => 3u8,
            SymbolType::File => 4u8,
            _ => 0u8,
        };
        let info = (bind << 4) | (sym_type & 0x0f);
        let shndx = if sym.is_defined {
            section_index_for_symbol(sym).unwrap_or(0xfff1)
        } else {
            0
        };
        let value = u32_checked(sym.value, "ELF32 symbol value")?;
        let size = u32_checked(sym.size, "ELF32 symbol size")?;
        let entry = (name_off, info, 0, shndx, value, size);
        if sym.binding == SymbolBinding::Local {
            local_entries.push(entry);
        } else {
            nonlocal_entries.push(entry);
        }
    }

    // ELF gABI requires all STB_LOCAL symbols to precede STB_GLOBAL/STB_WEAK,
    // and `.symtab` sh_info equals the index of the first non-local symbol.
    let symtab_info = (1 + local_entries.len()) as u32;
    let mut sym_entries = Vec::with_capacity(1 + local_entries.len() + nonlocal_entries.len());
    sym_entries.push((0, 0, 0, 0, 0, 0));
    sym_entries.extend(local_entries);
    sym_entries.extend(nonlocal_entries);

    let symtab_off = u32_checked(current_offset, "ELF32 .symtab offset")?;
    for (name_off, info, other, shndx, value, size) in &sym_entries {
        push_u32(&mut output, *name_off, bo);
        push_u32(&mut output, *value, bo);
        push_u32(&mut output, *size, bo);
        output.push(*info);
        output.push(*other);
        push_u16(&mut output, *shndx, bo);
    }
    current_offset += sym_entries.len() as u64 * SYM_SIZE as u64;

    let strtab_off = u32_checked(current_offset, "ELF32 .strtab offset")?;
    output.extend_from_slice(&strtab);
    current_offset += strtab.len() as u64;

    while current_offset % 4 != 0 {
        output.push(0);
        current_offset += 1;
    }

    let shoff = u32_checked(current_offset, "ELF32 section header offset")?;

    plans.push(SectionPlan {
        offset: shstr_off,
        va: u32_checked(image_base + shstr_off as u64, "ELF32 .shstrtab VA")?,
        size: shstr.len() as u32,
        alignment: 1,
        sh_type: SHT_STRTAB,
        sh_flags: 0,
        entsize: 0,
        link: 0,
        info: 0,
        name_off: shstr_name_off,
    });
    let symtab_shdr_idx = plans.len() as u32;
    plans.push(SectionPlan {
        offset: symtab_off,
        va: u32_checked(image_base + symtab_off as u64, "ELF32 .symtab VA")?,
        size: (sym_entries.len() * SYM_SIZE) as u32,
        alignment: 4,
        sh_type: SHT_SYMTAB,
        sh_flags: 0,
        entsize: SYM_SIZE as u32,
        link: (shstrtab_idx + 2) as u32,
        info: symtab_info,
        name_off: symtab_name_off,
    });
    plans.push(SectionPlan {
        offset: strtab_off,
        va: u32_checked(image_base + strtab_off as u64, "ELF32 .strtab VA")?,
        size: strtab.len() as u32,
        alignment: 1,
        sh_type: SHT_STRTAB,
        sh_flags: 0,
        entsize: 0,
        link: 0,
        info: 0,
        name_off: strtab_name_off,
    });

    for plan in &plans {
        push_u32(&mut output, plan.name_off, bo);
        push_u32(&mut output, plan.sh_type, bo);
        push_u32(&mut output, plan.sh_flags, bo);
        push_u32(&mut output, plan.va, bo);
        push_u32(&mut output, plan.offset, bo);
        push_u32(&mut output, plan.size, bo);
        push_u32(&mut output, plan.link, bo);
        push_u32(&mut output, plan.info, bo);
        push_u32(&mut output, plan.alignment, bo);
        push_u32(&mut output, plan.entsize, bo);
    }

    let shnum = plans.len() as u16;

    let rx_end = u32_checked(rx_file_end, "ELF32 RX segment end")?;
    let image_base32 = u32_checked(image_base, "ELF32 image base")?;
    let headers_end = u32_checked(headers_size as u64, "ELF32 headers size")?;

    output[0..4].copy_from_slice(&ELF_MAGIC);
    output[4] = ELFCLASS32;
    output[5] = match bo {
        ElfByteOrder::Little => ELFDATA2LSB,
        ElfByteOrder::Big => ELFDATA2MSB,
    };
    output[6] = EV_CURRENT;
    output[7] = ELFOSABI_NONE;
    write_u16(&mut output, 16, ET_EXEC, bo);
    write_u16(&mut output, 18, e_machine, bo);
    write_u32(&mut output, 20, EV_CURRENT as u32, bo);
    write_u32(&mut output, 24, entry32, bo);
    write_u32(&mut output, 28, EHDR_SIZE as u32, bo);
    write_u32(&mut output, 32, shoff, bo);
    write_u32(&mut output, 36, 0, bo);
    write_u16(&mut output, 40, EHDR_SIZE as u16, bo);
    write_u16(&mut output, 42, PHDR_SIZE as u16, bo);
    write_u16(&mut output, 44, phnum as u16, bo);
    write_u16(&mut output, 46, SHDR_SIZE as u16, bo);
    write_u16(&mut output, 48, shnum, bo);
    write_u16(&mut output, 50, shstrtab_idx as u16, bo);

    let phdr_base = EHDR_SIZE;
    let mut write_phdr = |idx: usize,
                          p_type: u32,
                          p_offset: u32,
                          p_vaddr: u32,
                          p_filesz: u32,
                          p_memsz: u32,
                          p_flags: u32,
                          p_align: u32| {
        let base = phdr_base + idx * PHDR_SIZE;
        write_u32(&mut output, base, p_type, bo);
        write_u32(&mut output, base + 4, p_offset, bo);
        write_u32(&mut output, base + 8, p_vaddr, bo);
        write_u32(&mut output, base + 12, p_vaddr, bo);
        write_u32(&mut output, base + 16, p_filesz, bo);
        write_u32(&mut output, base + 20, p_memsz, bo);
        write_u32(&mut output, base + 24, p_flags, bo);
        write_u32(&mut output, base + 28, p_align, bo);
    };

    let mut ph_idx = 0usize;
    write_phdr(
        ph_idx,
        PT_PHDR,
        EHDR_SIZE as u32,
        image_base32 + EHDR_SIZE as u32,
        (phnum * PHDR_SIZE) as u32,
        (phnum * PHDR_SIZE) as u32,
        PF_R,
        4,
    );
    ph_idx += 1;
    write_phdr(
        ph_idx,
        PT_LOAD,
        0,
        image_base32,
        rx_end,
        rx_end,
        PF_R | PF_X,
        0x1000,
    );
    ph_idx += 1;
    if has_rw {
        let rw_off = u32_checked(rw_start_off.unwrap_or(rx_file_end), "ELF32 RW offset")?;
        let rw_va = u32_checked(
            rw_start_va.unwrap_or(image_base + rx_file_end),
            "ELF32 RW VA",
        )?;
        let rw_filesz = u32_checked(
            rw_file_end.saturating_sub(u64::from(rw_off)),
            "ELF32 RW filesz",
        )?;
        let rw_memsz = u32_checked(
            rw_mem_end.saturating_sub(u64::from(rw_va)),
            "ELF32 RW memsz",
        )?;
        write_phdr(
            ph_idx,
            PT_LOAD,
            rw_off,
            rw_va,
            rw_filesz,
            rw_memsz,
            PF_R | PF_W,
            0x1000,
        );
        ph_idx += 1;
    }
    write_phdr(ph_idx, PT_GNU_STACK, 0, 0, 0, 0, PF_R | PF_W, 0x10);
    ph_idx += 1;
    if has_note {
        write_phdr(
            ph_idx,
            PT_NOTE,
            note_offset,
            note_size.0,
            note_size.1,
            note_size.1,
            PF_R,
            4,
        );
    }

    debug_assert_eq!(output.len(), shoff as usize + plans.len() * SHDR_SIZE);
    debug_assert!(headers_end <= rx_end);
    debug_assert_eq!(symtab_shdr_idx, (shstrtab_idx + 1) as u32);

    Ok(output)
}
