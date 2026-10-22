//! Mach-O Object File Reader.

use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::macho::header::*;
use crate::object::ObjectFile;
use crate::relocation::{Relocation, RelocationKind};
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
            libc_flavor: crate::target::LibcFlavor::Gnu,
        };

        let mut obj = ObjectFile::new(path.to_path_buf(), target, file_index);

        let ncmds = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
        let mut offset = 32usize;

        let mut symoff = 0usize;
        let mut nsyms = 0usize;
        let mut stroff = 0usize;
        let mut strsize = 0usize;
        let mut pending_relocs: Vec<(usize, usize, usize)> = Vec::new();

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

                    let reloff = if sect_off + 64 <= bytes.len() {
                        u32::from_le_bytes(bytes[sect_off + 56..sect_off + 60].try_into().unwrap())
                            as usize
                    } else {
                        0
                    };
                    let nreloc = if sect_off + 64 <= bytes.len() {
                        u32::from_le_bytes(bytes[sect_off + 60..sect_off + 64].try_into().unwrap())
                            as usize
                    } else {
                        0
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

                    let sec_idx = obj.sections.len();
                    obj.add_section(sec);
                    if reloff > 0 && nreloc > 0 {
                        pending_relocs.push((sec_idx, reloff, nreloc));
                    }
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
                // Mach-O's external symbol table adds one ABI underscore to
                // every C/global name. Keep linker symbols in their logical
                // form so portable ADOB references (for example `aot_print`)
                // resolve against Rust/C static-library members (`_aot_print`).
                // Local assembler labels are not subject to this convention.
                let sym_name = if n_type & 0x01 != 0 {
                    sym_name.strip_prefix('_').unwrap_or(&sym_name).to_string()
                } else {
                    sym_name
                };

                let is_defined = n_sect > 0;
                let sec_idx = if is_defined && (n_sect as usize) <= obj.sections.len() {
                    Some((n_sect as usize) - 1)
                } else if is_defined {
                    return Err(LinkError::new(
                        ErrorCode::InvalidObject,
                        format!(
                            "malformed Mach-O object `{}`: symbol `{}` references section index {} but object has only {} sections",
                            path.display(),
                            sym_name,
                            n_sect,
                            obj.sections.len()
                        ),
                    ));
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
                    is_imported: !is_defined,
                    is_exported: is_defined && binding == SymbolBinding::Global,
                    file_index: Some(file_index),
                    alias_of: None,
                    comdat_group: None,
                    version: None,
                };

                obj.add_symbol(sym);
            }
        }

        // Anonymous (empty-named) symbols do occur in Mach-O objects emitted
        // by the Rust toolchain (and other LLVM-based producers) for private
        // labels and unwind placeholders. A relocation that names its target
        // by an empty string can never resolve, and previously surfaced as
        // `LNK001: undefined reference to \`\`` (an empty symbol name) on the
        // macOS AOT pipeline. Rewrite every empty-named symbol to a unique
        // synthetic name so relocation references stay index-aligned AND
        // resolve by name:
        //
        // - Defined anonymous symbols become file-unique locals; their
        //   relocations resolve through the local-symbol maps.
        // - Undefined anonymous symbols are meaningless as imports (no other
        //   object can resolve a name that was never emitted), so they are
        //   marked weak: the resolver binds them to NULL per the weak-undef
        //   ABI rule instead of failing the whole link.
        for (i, sym) in obj.symbols.iter_mut().enumerate() {
            if sym.name.is_empty() {
                if sym.is_defined {
                    sym.name = format!("__macho_anon_{file_index}_{i}");
                } else {
                    sym.name = format!("__macho_anon_undef_{file_index}_{i}");
                    sym.binding = SymbolBinding::Weak;
                }
            }
        }

        // Non-extern ("section-based") Mach-O relocations reference a section
        // by its 1-based number, not by a symbol. Register one defined local
        // symbol per section so such references resolve to the section base.
        // The names are unique per (file, section) because the same object can
        // contain repeated section names (e.g. two `__const` sections). These
        // are appended after the real symbol table, so `r_symbolnum` indices
        // of extern relocations remain valid.
        for s in 0..obj.sections.len() {
            let sec_sym = Symbol {
                name: format!("__macho_sec_{file_index}_{s}"),
                binding: SymbolBinding::Local,
                visibility: SymbolVisibility::Default,
                sym_type: SymbolType::Object,
                section_index: Some(s),
                value: 0,
                size: obj.sections[s].size,
                is_defined: true,
                is_imported: false,
                is_exported: false,
                file_index: Some(file_index),
                alias_of: None,
                comdat_group: None,
                version: None,
            };
            obj.add_symbol(sec_sym);
        }

        // Parse section relocations (relocation_info: 8 bytes each)
        for (sec_idx, reloff, nreloc) in pending_relocs {
            if sec_idx >= obj.sections.len() {
                continue;
            }
            let mut r_i = 0;
            let mut pending_addend: Option<i64> = None;
            while r_i < nreloc {
                let r_off = reloff + r_i * 8;
                if r_off + 8 > bytes.len() {
                    break;
                }
                let r_address = u32::from_le_bytes(bytes[r_off..r_off + 4].try_into().unwrap());
                let r_word1 = u32::from_le_bytes(bytes[r_off + 4..r_off + 8].try_into().unwrap());

                // Mach-O relocation_info bitfields (little-endian):
                // r_symbolnum: 24 bits
                // r_pcrel: 1 bit (bit 24)
                // r_length: 2 bits (bits 25..26)
                // r_extern: 1 bit (bit 27)
                // r_type: 4 bits (bits 28..31)
                let r_symbolnum = (r_word1 & 0x00FF_FFFF) as usize;
                let r_pcrel = ((r_word1 >> 24) & 1) != 0;
                let r_length = (r_word1 >> 25) & 3;
                let r_extern = ((r_word1 >> 27) & 1) != 0;
                let r_type = ((r_word1 >> 28) & 0xF) as u8;

                // Handle ARM64_RELOC_ADDEND (type 10 on AArch64): r_symbolnum carries a 24-bit addend
                // for the following relocation entry rather than a symbol index or section number.
                if arch == Arch::AArch64 && r_type == 10 {
                    let addend_24 = (r_word1 & 0x00FF_FFFF) as i32;
                    let addend = if (addend_24 & 0x0080_0000) != 0 {
                        (addend_24 | !0x00FF_FFFF) as i64
                    } else {
                        addend_24 as i64
                    };
                    pending_addend = Some(addend);
                    r_i += 1;
                    continue;
                }

                let is_subtractor =
                    (arch == Arch::X86_64 && r_type == 5) || (arch == Arch::AArch64 && r_type == 1);

                if is_subtractor {
                    if r_i + 1 >= nreloc {
                        return Err(LinkError::new(
                            ErrorCode::InvalidObject,
                            format!(
                                "malformed Mach-O relocation pair in section `{}`: SUBTRACTOR at address 0x{:x} has no paired relocation",
                                obj.sections[sec_idx].name, r_address
                            ),
                        ));
                    }
                    let r_off2 = reloff + (r_i + 1) * 8;
                    if r_off2 + 8 > bytes.len() {
                        return Err(LinkError::new(
                            ErrorCode::InvalidObject,
                            format!(
                                "truncated Mach-O relocation pair in section `{}` at offset 0x{:x}",
                                obj.sections[sec_idx].name, r_off2
                            ),
                        ));
                    }
                    let r_address2 =
                        u32::from_le_bytes(bytes[r_off2..r_off2 + 4].try_into().unwrap());
                    let r_word1_2 =
                        u32::from_le_bytes(bytes[r_off2 + 4..r_off2 + 8].try_into().unwrap());
                    let r_pcrel2 = ((r_word1_2 >> 24) & 1) != 0;
                    let r_length2 = (r_word1_2 >> 25) & 3;
                    let r_type2 = ((r_word1_2 >> 28) & 0xF) as u8;

                    if r_address2 != r_address
                        || r_type2 != 0
                        || r_pcrel2 != false
                        || r_length2 != r_length
                    {
                        return Err(LinkError::new(
                            ErrorCode::InvalidObject,
                            format!(
                                "malformed Mach-O relocation pair in section `{}`: SUBTRACTOR at address 0x{:x} (type {}, len {}) paired with invalid relocation at address 0x{:x} (type {}, len {})",
                                obj.sections[sec_idx].name,
                                r_address,
                                r_type,
                                r_length,
                                r_address2,
                                r_type2,
                                r_length2
                            ),
                        ));
                    }

                    // Pair is validated; section payload pre-calculated relative offsets remain valid
                    r_i += 2;
                    continue;
                }

                // Extern relocations name their target by symbol-table index.
                // Resolve through the index first (immune to duplicate names),
                // and reject out-of-range indices loudly: silently emitting an
                // empty symbol name produces an unexplainable LNK001 later.
                let sym_name = if r_extern {
                    if r_symbolnum < obj.symbols.len() {
                        obj.symbols[r_symbolnum].name.clone()
                    } else {
                        return Err(LinkError::new(
                            ErrorCode::InvalidObject,
                            format!(
                                "malformed Mach-O object `{}`: extern relocation in section `{}` at address 0x{:x} references symbol index {} but the symbol table has only {} entries",
                                path.display(),
                                obj.sections[sec_idx].name,
                                r_address,
                                r_symbolnum,
                                obj.symbols.len()
                            ),
                        ));
                    }
                } else if r_symbolnum > 0 && r_symbolnum <= obj.sections.len() {
                    // Section-based relocation: target the synthetic
                    // per-section local symbol registered above.
                    format!("__macho_sec_{file_index}_{}", r_symbolnum - 1)
                } else {
                    return Err(LinkError::new(
                        ErrorCode::InvalidObject,
                        format!(
                            "malformed Mach-O object `{}`: non-extern relocation in section `{}` at address 0x{:x} references invalid section number {} (object has {} sections)",
                            path.display(),
                            obj.sections[sec_idx].name,
                            r_address,
                            r_symbolnum,
                            obj.sections.len()
                        ),
                    ));
                };

                let (kind, extra_addend) = match (arch, r_type, r_pcrel, r_length) {
                    (Arch::AArch64, 2, true, _) => (RelocationKind::AArch64Call26, 0), // ARM64_RELOC_BRANCH26
                    (Arch::AArch64, 3, true, _) => (RelocationKind::AArch64Adrp, 0), // ARM64_RELOC_PAGE21
                    (Arch::AArch64, 4, false, _) => (RelocationKind::AArch64AddLo12, 0), // ARM64_RELOC_PAGEOFF12
                    (Arch::AArch64, 5, true, _) => (RelocationKind::AArch64Adrp, 0), // ARM64_RELOC_GOT_LOAD_PAGE21
                    (Arch::AArch64, 6, false, _) => (RelocationKind::AArch64AddLo12, 0), // ARM64_RELOC_GOT_LOAD_PAGEOFF12
                    (Arch::AArch64, 0, false, 3) => (RelocationKind::Absolute64, 0), // ARM64_RELOC_UNSIGNED (len 3 = 8 bytes)
                    (Arch::AArch64, 0, false, 2) => (RelocationKind::Absolute32, 0), // ARM64_RELOC_UNSIGNED (len 2 = 4 bytes)
                    (Arch::X86_64, 1, true, 2) => (RelocationKind::PcRelative32, 0), // X86_64_RELOC_SIGNED
                    (Arch::X86_64, 2, true, 2) => (RelocationKind::PltRelative32, 0), // X86_64_RELOC_BRANCH
                    (Arch::X86_64, 3, true, 2) => (RelocationKind::GotRelative32, 0), // X86_64_RELOC_GOT_LOAD
                    (Arch::X86_64, 4, true, 2) => (RelocationKind::GotRelative32, 0), // X86_64_RELOC_GOT
                    (Arch::X86_64, 6, true, 2) => (RelocationKind::PcRelative32, -1), // X86_64_RELOC_SIGNED_1
                    (Arch::X86_64, 7, true, 2) => (RelocationKind::PcRelative32, -2), // X86_64_RELOC_SIGNED_2
                    (Arch::X86_64, 8, true, 2) => (RelocationKind::PcRelative32, -4), // X86_64_RELOC_SIGNED_4
                    (Arch::X86_64, 0, false, 3) => (RelocationKind::Absolute64, 0), // X86_64_RELOC_UNSIGNED (len 3 = 8 bytes)
                    (Arch::X86_64, 0, false, 2) => (RelocationKind::Absolute64, 0), // Mach-O 64-bit non-extern relocations with r_length=2 on x86_64 carry a 64-bit virtual address (target_va) in the section payload; mapping as Absolute64 prevents false ABS32 overflow (0x100000000+ VAs)
                    _ => {
                        let k = if r_pcrel {
                            RelocationKind::PcRelative32
                        } else if r_length == 3 {
                            RelocationKind::Absolute64
                        } else {
                            RelocationKind::Absolute32
                        };
                        (k, 0)
                    }
                };

                let reloc_addend = pending_addend.take().unwrap_or(0) + extra_addend;
                let mut reloc = Relocation::new(r_address as u64, sym_name, kind, reloc_addend);
                reloc.file_index = Some(file_index);
                if r_extern {
                    reloc.symbol_index = Some(r_symbolnum);
                }
                obj.sections[sec_idx].relocations.push(reloc);
                r_i += 1;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal x86_64 Mach-O relocatable object containing the
    /// pathological shapes that occur in real rustc-produced runtime members:
    ///
    /// - a defined local symbol with an EMPTY name (anonymous label),
    /// - an undefined symbol with an EMPTY name,
    /// - an extern relocation against the empty-named defined symbol,
    /// - an extern relocation against the empty-named undefined symbol,
    /// - a non-extern section-based relocation (target = the section).
    ///
    /// Regression for the macOS AOT failure `LNK001: undefined reference to \`\``.
    fn build_anonymous_symbol_object() -> Vec<u8> {
        let mut b = Vec::new();
        // Mach-O 64 header
        b.extend_from_slice(&0xfeedfacfu32.to_le_bytes()); // magic
        b.extend_from_slice(&0x0100_0007u32.to_le_bytes()); // cputype x86_64
        b.extend_from_slice(&3u32.to_le_bytes()); // cpusubtype
        b.extend_from_slice(&1u32.to_le_bytes()); // filetype MH_OBJECT
        b.extend_from_slice(&2u32.to_le_bytes()); // ncmds
        b.extend_from_slice(&176u32.to_le_bytes()); // sizeofcmds
        b.extend_from_slice(&0u32.to_le_bytes()); // flags
        b.extend_from_slice(&0u32.to_le_bytes()); // reserved

        // LC_SEGMENT_64 with one __text section
        let seg_start = b.len() as u32;
        b.extend_from_slice(&0x19u32.to_le_bytes()); // LC_SEGMENT_64
        b.extend_from_slice(&152u32.to_le_bytes()); // cmdsize (72 + 80)
        let mut segname = [0u8; 16];
        segname[..6].copy_from_slice(b"__TEXT");
        b.extend_from_slice(&segname);
        b.extend_from_slice(&0u64.to_le_bytes()); // vmaddr
        b.extend_from_slice(&208u64.to_le_bytes()); // vmsize
        b.extend_from_slice(&208u64.to_le_bytes()); // fileoff
        b.extend_from_slice(&16u64.to_le_bytes()); // filesize
        b.extend_from_slice(&7u32.to_le_bytes()); // maxprot
        b.extend_from_slice(&5u32.to_le_bytes()); // initprot
        b.extend_from_slice(&1u32.to_le_bytes()); // nsects
        b.extend_from_slice(&0u32.to_le_bytes()); // flags
        let sect_start = b.len();
        let mut sectname = [0u8; 16];
        sectname[..6].copy_from_slice(b"__text");
        b.extend_from_slice(&sectname);
        b.extend_from_slice(&segname); // segname
        b.extend_from_slice(&0u64.to_le_bytes()); // addr
        b.extend_from_slice(&16u64.to_le_bytes()); // size
        b.extend_from_slice(&208u32.to_le_bytes()); // offset (file offset of code)
        b.extend_from_slice(&4u32.to_le_bytes()); // align (log2 = 4)
        b.extend_from_slice(&224u32.to_le_bytes()); // reloff
        b.extend_from_slice(&3u32.to_le_bytes()); // nreloc
        b.extend_from_slice(&0u32.to_le_bytes()); // flags
        b.extend_from_slice(&[0u8; 12]); // reserved1..3
        assert_eq!(b.len() - sect_start, 80);
        assert_eq!(b.len() as u32 - seg_start, 152);

        // LC_SYMTAB
        b.extend_from_slice(&0x2u32.to_le_bytes());
        b.extend_from_slice(&24u32.to_le_bytes());
        b.extend_from_slice(&248u32.to_le_bytes()); // symoff
        b.extend_from_slice(&3u32.to_le_bytes()); // nsyms
        b.extend_from_slice(&296u32.to_le_bytes()); // stroff
        b.extend_from_slice(&7u32.to_le_bytes()); // strsize

        // __text payload: 16 bytes
        assert_eq!(b.len(), 208);
        b.extend_from_slice(&[0x90u8; 16]);

        // Relocations: 3 * 8 bytes at offset 224
        assert_eq!(b.len(), 224);
        // A: extern -> sym 0 (defined anonymous), pcrel, len 2 (32-bit), type 0
        b.extend_from_slice(&0u32.to_le_bytes()); // r_address
        b.extend_from_slice(&0x0A00_0000u32.to_le_bytes());
        // B: extern -> sym 1 (undefined anonymous)
        b.extend_from_slice(&4u32.to_le_bytes());
        b.extend_from_slice(&0x0A00_0001u32.to_le_bytes());
        // C: non-extern, section 1, abs64 (len 3), type 0
        b.extend_from_slice(&8u32.to_le_bytes());
        b.extend_from_slice(&0x6000_0001u32.to_le_bytes());

        // Symbol table: 3 * 16 bytes at offset 248
        assert_eq!(b.len(), 248);
        // sym 0: defined local, empty name, section 1
        b.extend_from_slice(&0u32.to_le_bytes()); // n_strx = 0 -> empty name
        b.extend_from_slice(&[0x0E, 0x01]); // N_SECT, n_sect 1
        b.extend_from_slice(&0u16.to_le_bytes()); // n_desc
        b.extend_from_slice(&0u64.to_le_bytes()); // n_value
        // sym 1: undefined, empty name
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&[0x00, 0x00]); // N_UNDF, section 0
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(&0u64.to_le_bytes());
        // sym 2: extern global `_main`, defined in section 1
        b.extend_from_slice(&1u32.to_le_bytes()); // n_strx -> "_main"
        b.extend_from_slice(&[0x0F, 0x01]); // N_SECT | N_EXT
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(&0u64.to_le_bytes());

        // String table: "\0_main\0" = 7 bytes at offset 296
        assert_eq!(b.len(), 296);
        b.extend_from_slice(&[0, b'_', b'm', b'a', b'i', b'n', 0]);
        b
    }

    #[test]
    fn test_anonymous_symbols_get_resolvable_names() {
        let bytes = build_anonymous_symbol_object();
        let obj = MachOReader::read(&bytes, std::path::Path::new("anon.o"), 0)
            .expect("anonymous-symbol object must parse");

        // No symbol may keep an empty name; indices are preserved.
        assert_eq!(obj.symbols.len(), 3 + 1); // 3 real + 1 synthetic section symbol
        assert!(
            obj.symbols.iter().all(|s| !s.name.is_empty()),
            "every symbol must carry a resolvable name"
        );
        // The defined anonymous symbol was renamed, still local+defined.
        assert_eq!(obj.symbols[0].name, "__macho_anon_0_0");
        assert!(obj.symbols[0].is_defined);
        // The undefined anonymous symbol was renamed and marked weak so the
        // resolver binds it to NULL instead of failing the link.
        assert_eq!(obj.symbols[1].name, "__macho_anon_undef_0_1");
        assert!(!obj.symbols[1].is_defined);
        assert_eq!(obj.symbols[1].binding, SymbolBinding::Weak);
        // The C-ABI underscore is still stripped for real extern names.
        assert_eq!(obj.symbols[2].name, "main");

        // No relocation may carry an empty symbol name.
        let text_relocs = &obj.sections[0].relocations;
        assert_eq!(text_relocs.len(), 3);
        assert_eq!(text_relocs[0].symbol_name, "__macho_anon_0_0");
        assert_eq!(text_relocs[0].symbol_index, Some(0));
        assert_eq!(text_relocs[1].symbol_name, "__macho_anon_undef_0_1");
        assert_eq!(text_relocs[1].symbol_index, Some(1));
        // Section-based relocation resolves to the synthetic section symbol.
        assert_eq!(text_relocs[2].symbol_name, "__macho_sec_0_0");
        assert!(text_relocs[2].symbol_index.is_none());
    }

    #[test]
    fn test_invalid_section_index_in_relocation_returns_lnk006() {
        let mut bytes = build_anonymous_symbol_object();
        // Modify relocation 2 (non-extern, offset 240) to reference section 548 instead of 1.
        // Reloc 2 is at offset 224 + 2*8 = 240.
        // Address: bytes 240..244; r_word1: bytes 244..248.
        // r_word1 was 0x6000_0001 (r_symbolnum = 1). Set r_symbolnum = 548 (0x224).
        let r_word1 = (0x6000_0000u32) | 548u32;
        bytes[244..248].copy_from_slice(&r_word1.to_le_bytes());

        let err = MachOReader::read(&bytes, std::path::Path::new("bad_sec.o"), 0)
            .expect_err("Mach-O with section 548 relocation must fail validation");

        assert_eq!(err.code, ErrorCode::InvalidObject);
        assert_eq!(err.code.as_str(), "LNK006");
        assert!(
            err.message
                .contains("references invalid section number 548"),
            "Error message should mention section 548: {}",
            err.message
        );
    }

    #[test]
    fn test_invalid_symbol_index_in_extern_relocation_returns_lnk006() {
        let mut bytes = build_anonymous_symbol_object();
        // Modify relocation 0 (extern, offset 224) to reference symbol 99 instead of 0.
        let r_word1 = (0x0A00_0000u32) | 99u32;
        bytes[228..232].copy_from_slice(&r_word1.to_le_bytes());

        let err = MachOReader::read(&bytes, std::path::Path::new("bad_sym.o"), 0)
            .expect_err("Mach-O with symbol 99 relocation must fail validation");

        assert_eq!(err.code, ErrorCode::InvalidObject);
        assert_eq!(err.code.as_str(), "LNK006");
        assert!(
            err.message.contains("references symbol index 99"),
            "Error message should mention symbol 99: {}",
            err.message
        );
    }

    #[test]
    fn test_symbol_referencing_out_of_range_section_returns_lnk006() {
        let mut bytes = build_anonymous_symbol_object();
        // Modify symbol 2 (offset 248 + 2*16 = 280) to reference section 200 instead of 1.
        // Symbol struct: n_strx (4), n_type (1), n_sect (1) at offset 280+5 = 285.
        bytes[285] = 200;

        let err = MachOReader::read(&bytes, std::path::Path::new("bad_sym_sec.o"), 0)
            .expect_err("Mach-O symbol referencing section 200 must fail validation");

        assert_eq!(err.code, ErrorCode::InvalidObject);
        assert_eq!(err.code.as_str(), "LNK006");
        assert!(
            err.message.contains("references section index 200"),
            "Error message should mention section index 200: {}",
            err.message
        );
    }
}
