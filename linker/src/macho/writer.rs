//! Mach-O 64-bit Executable and Dynamic Library Writer.

use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::macho::dynamic::{
    self, BIND_TYPE_POINTER, BindTarget, LIBSYSTEM_ORDINAL, MachODynamicInfo, RebaseKind,
    RebaseTarget,
};
use crate::macho::header::*;
use crate::section::{MergedSection, SectionKind, align_to};
use crate::symbol::{Symbol, SymbolBinding};
use crate::target::{Arch, Target};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Name of the section holding one pointer slot per imported function.
/// Created by the layout stage; slot VAs are registered as
/// `__got_<symbol>` so the linker-synthesized proxy stubs can load through
/// them. dyld binds every slot eagerly while loading the image.
pub const GOT_SECTION: &str = "__got";

/// Segment for read-only data that still contains addresses dyld must write at
/// load time. See [`Placement::DataConst`].
pub const DATA_CONST_SEGMENT: &str = "__DATA_CONST";

/// Mach-O names of the thread-local sections the layout stage produces. The
/// merged TLS template (`.tls`) is split into the file-backed image and the
/// zero-fill tail, exactly as ld64 and lld emit them.
pub const TLV_VARS_SECTION: &str = "__thread_vars";
pub const THREAD_DATA_SECTION: &str = "__thread_data";
pub const THREAD_BSS_SECTION: &str = "__thread_bss";

/// Number of `section_64` records a merged section contributes to `__DATA`.
/// The TLS template becomes `__thread_data` (the file-backed per-thread
/// image) and/or `__thread_bss` (the zero-filled tail); either half can be
/// empty, so the count is what the header sizing and the emission loop must
/// agree on.
fn data_section_records(sec: &MergedSection) -> usize {
    if sec.flags & crate::section::flags::TLS == 0 {
        return 1;
    }
    let file_image = sec.data.len() as u64;
    let block = sec.size.max(file_image);
    usize::from(file_image > 0) + usize::from(block > file_image)
}

/// Which Mach-O segment a merged section is emitted into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placement {
    /// `r-x`, and `maxprot` is also `r-x`: dyld can never write here.
    Text,
    /// `rw-` while dyld applies its fixups, read-only afterwards: every
    /// non-executable, non-thread-local read-only section of a dyld image.
    DataConst,
    /// `rw-`, mutable data plus everything the loader special-cases (TLS).
    Data,
}

/// The segment a merged section is emitted into, decided with one rule that
/// the placement loop, the `section_64` emission and the symbol `n_sect`
/// numbering all share — if they disagreed the section table and the symbol
/// table would describe different layouts.
///
/// Why read-only data needs its own segment: `__TEXT` is mapped `r-x` with
/// `maxprot r-x`, so when dyld tries to apply the PIE slide it cannot get
/// write access to the page. The kernel faults and the process dies with
/// SIGBUS before `main` runs. ld64 and lld therefore keep only executable
/// code in `__TEXT` and put the rest of the read-only image in `__DATA_CONST`
/// (`rw-`): writable while dyld applies its fixups, re-protected read-only by
/// dyld afterwards.
///
/// The split is by section properties rather than per-section relocation
/// inspection because the layout stage emits every read-only group before the
/// writable ones, so each segment still owns a single contiguous VA range.
fn placement_of(sec: &MergedSection, rodata_in_data_const: bool) -> Placement {
    if sec.is_executable() {
        return Placement::Text;
    }
    if rodata_in_data_const
        && !sec.is_writable()
        && sec.kind != SectionKind::Bss
        // TLS blocks and the TLV descriptor table are fixed up and read by the
        // thread loader, which looks them up in `__DATA`.
        && sec.flags & crate::section::flags::TLS == 0
        && sec.flags & crate::section::flags::TLV_TABLE == 0
    {
        return Placement::DataConst;
    }
    // Without a `__DATA_CONST` segment the read-only groups have to share
    // `__TEXT` (a non-dyld image has no fixups to apply), which is what the
    // two-segment layout of a static object does.
    if !rodata_in_data_const && !sec.is_writable() && sec.kind != SectionKind::Bss {
        return Placement::Text;
    }
    Placement::Data
}

/// The dyld section type and indirect-symbol-table start (`reserved1`) a
/// synthesized pointer section carries, shared by `__DATA` and
/// `__DATA_CONST`: dyld reads both to decide how to interpret each slot.
fn dyld_pointer_section(name: &str) -> Option<(u32, u32)> {
    match name {
        GOT_SECTION => Some((S_NON_LAZY_SYMBOL_POINTERS, 0)),
        TLV_VARS_SECTION => Some((S_THREAD_LOCAL_VARIABLES, 0)),
        THREAD_DATA_SECTION => Some((S_THREAD_LOCAL_REGULAR, 0)),
        THREAD_BSS_SECTION => Some((S_THREAD_LOCAL_ZEROFILL, 0)),
        _ => None,
    }
}

/// Append one `section_64` record of a data segment to the load-command buffer.
///
/// The flags carry the dyld section type (low byte) and, for a pointer
/// section, the indirect-symbol-table start in `reserved1`: dyld reads both to
/// decide how to interpret the slots it binds.
fn push_data_section_record(
    cmd_buf: &mut Vec<u8>,
    seg_name: &[u8; 16],
    name: &str,
    va: u64,
    sz: u64,
    off: u32,
    align: u64,
    is_bss: bool,
) {
    let mut sname = [0u8; 16];
    let name_bytes = name.as_bytes();
    sname[0..name_bytes.len().min(16)].copy_from_slice(&name_bytes[0..name_bytes.len().min(16)]);
    cmd_buf.extend_from_slice(&sname);
    cmd_buf.extend_from_slice(seg_name);
    cmd_buf.extend_from_slice(&va.to_le_bytes());
    cmd_buf.extend_from_slice(&sz.to_le_bytes());
    cmd_buf.extend_from_slice(&off.to_le_bytes());
    let align_pow2 = if align > 1 {
        (align as f64).log2().round() as u32
    } else {
        0
    };
    cmd_buf.extend_from_slice(&align_pow2.to_le_bytes());
    cmd_buf.extend_from_slice(&0u32.to_le_bytes()); // reloff
    cmd_buf.extend_from_slice(&0u32.to_le_bytes()); // nreloc
    let (section_flags, reserved1) = match dyld_pointer_section(name) {
        Some(typed) => typed,
        None if is_bss => (S_ZEROFILL, 0u32),
        None => (S_REGULAR, 0u32),
    };
    cmd_buf.extend_from_slice(&section_flags.to_le_bytes());
    cmd_buf.extend_from_slice(&reserved1.to_le_bytes());
    cmd_buf.extend_from_slice(&0u32.to_le_bytes()); // reserved2
    cmd_buf.extend_from_slice(&0u32.to_le_bytes()); // reserved3
}

pub struct MachOWriter;

impl MachOWriter {
    pub fn write_executable(
        path: &Path,
        target: &Target,
        entry_va: u64,
        merged_sections: &[MergedSection],
        symbols: &[Symbol],
    ) -> LinkResult<()> {
        Self::write_macho(
            path,
            target,
            entry_va,
            merged_sections,
            symbols,
            false,
            &[],
            None,
        )
    }

    pub fn write_macho(
        path: &Path,
        target: &Target,
        entry_va: u64,
        merged_sections: &[MergedSection],
        symbols: &[Symbol],
        is_shared: bool,
        libraries: &[String],
        install_name: Option<&str>,
    ) -> LinkResult<()> {
        Self::write_macho_with_dynamic(
            path,
            target,
            entry_va,
            merged_sections,
            symbols,
            is_shared,
            libraries,
            install_name,
            None,
        )
    }

    /// Write a dynamically linked Mach-O image (dyld binding, lazy stubs,
    /// rebasing, export trie) as produced by the layout stage.
    pub fn write_macho_with_dynamic(
        path: &Path,
        target: &Target,
        entry_va: u64,
        merged_sections: &[MergedSection],
        symbols: &[Symbol],
        is_shared: bool,
        libraries: &[String],
        install_name: Option<&str>,
        dynamic: Option<&MachODynamicInfo>,
    ) -> LinkResult<()> {
        let bytes = Self::encode_macho_with_dynamic(
            target,
            entry_va,
            merged_sections,
            symbols,
            is_shared,
            libraries,
            install_name,
            dynamic,
        )?;

        fs::write(path, &bytes)?;

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
    ) -> LinkResult<Vec<u8>> {
        Self::encode_macho_with_dynamic(
            target,
            entry_va,
            merged_sections,
            symbols,
            false,
            &[],
            None,
            None,
        )
    }

    pub fn encode_macho(
        target: &Target,
        entry_va: u64,
        merged_sections: &[MergedSection],
        symbols: &[Symbol],
        is_shared: bool,
        libraries: &[String],
        install_name: Option<&str>,
    ) -> LinkResult<Vec<u8>> {
        Self::encode_macho_with_dynamic(
            target,
            entry_va,
            merged_sections,
            symbols,
            is_shared,
            libraries,
            install_name,
            None,
        )
    }

    pub fn encode_macho_with_dynamic(
        target: &Target,
        entry_va: u64,
        merged_sections: &[MergedSection],
        symbols: &[Symbol],
        is_shared: bool,
        libraries: &[String],
        install_name: Option<&str>,
        dynamic: Option<&MachODynamicInfo>,
    ) -> LinkResult<Vec<u8>> {
        // The stub helper and proxy-stub glue are x86_64 machine code. Any
        // other architecture must fail loudly rather than emit a binary
        // dyld cannot bind.
        if let Some(info) = dynamic
            && !info.imports.is_empty()
            && target.arch != Arch::X86_64
            && target.arch != Arch::AArch64
        {
            return Err(LinkError::new(
                ErrorCode::InvalidTarget,
                format!(
                    "Mach-O dyld binding is only implemented for x86_64 and AArch64; {} imports require \
                     stub-helper code this linker cannot synthesize yet",
                    info.imports.len()
                ),
            ));
        }
        let (cputype, cpusubtype) = if target.arch == Arch::Arm || target.arch == Arch::AArch64 {
            (CPU_TYPE_ARM64, CPU_SUBTYPE_ARM64_ALL)
        } else {
            (CPU_TYPE_X86_64, CPU_SUBTYPE_X86_64_ALL | CPU_SUBTYPE_LIB64)
        };

        // PAGEZERO is 4GB for executables, 0 for dylibs
        let pagezero_size = if is_shared { 0u64 } else { 0x100000000u64 };
        let text_vmaddr = pagezero_size;

        let mut output = Vec::new();

        // 1. Partition merged sections into __TEXT, __DATA_CONST and __DATA
        //
        // The `__DATA_CONST` split needs final section addresses to keep each
        // segment a contiguous VA range, so it only applies when the layout
        // stage assigned them. A dynamic link without them is rejected below,
        // which makes the two conditions equivalent for every image produced.
        let has_layout_vas = merged_sections.iter().any(|s| s.virtual_address != 0);
        let rodata_in_data_const = dynamic.is_some() && has_layout_vas;

        let mut text_sections = Vec::new();
        let mut data_const_sections = Vec::new();
        let mut data_sections = Vec::new();

        for sec in merged_sections {
            match placement_of(sec, rodata_in_data_const) {
                Placement::Text => text_sections.push(sec.clone()),
                Placement::DataConst => data_const_sections.push(sec.clone()),
                Placement::Data => data_sections.push(sec.clone()),
            }
        }
        let has_data_const = !data_const_sections.is_empty();

        // Dynamic images carry `LC_DYLD_INFO_ONLY` (rebasing is meaningful
        // even with no imports); the `__got` section exists only when there
        // are imports to bind.
        let has_dynamic = dynamic.is_some();
        let imports = dynamic.map(|d| d.imports.as_slice()).unwrap_or(&[]);

        // 2. Load Commands Size Calculation
        let cmd_pagezero_sz = if is_shared { 0usize } else { 72usize };
        let cmd_text_sz = 72 + text_sections.len() * 80;
        let cmd_data_const_sz = if has_data_const {
            72 + data_const_sections.len() * 80
        } else {
            0
        };
        // The TLS template expands into `__thread_data` + `__thread_bss`, so
        // the `__DATA` section count is the per-section record total, not the
        // merged-section count.
        let data_record_count: usize = data_sections.iter().map(data_section_records).sum();
        let cmd_data_sz = 72 + data_record_count * 80;
        let cmd_linkedit_sz = 72usize;
        let cmd_build_version_sz = 24usize;
        let dylinker_str = b"/usr/lib/dyld\0\0\0\0\0\0\0"; // padded to 20 bytes so 12 + 20 = 32 (8-byte aligned)
        let cmd_dylinker_sz = 12 + dylinker_str.len();
        let cmd_main_sz = if is_shared { 0usize } else { 24usize };
        let cmd_symtab_sz = 24usize;
        let cmd_dysymtab_sz = 80usize;
        let cmd_dyld_info_sz = if has_dynamic { 48usize } else { 0usize };
        let cmd_code_signature_sz = if !is_shared { 16usize } else { 0usize };

        // Dylib dependencies: always load /usr/lib/libSystem.B.dylib, plus any user libraries
        let mut dylib_commands: Vec<(String, Vec<u8>)> = Vec::new();
        let default_lib = "/usr/lib/libSystem.B.dylib".to_string();
        let mut all_libs = vec![default_lib];
        for lib in libraries {
            let path_str = if lib.starts_with('/') {
                lib.clone()
            } else {
                format!("/usr/lib/lib{}.dylib", lib)
            };
            if !all_libs.contains(&path_str) {
                all_libs.push(path_str);
            }
        }

        for lib_path in &all_libs {
            let mut str_bytes = lib_path.as_bytes().to_vec();
            str_bytes.push(0);
            while str_bytes.len() % 8 != 0 {
                str_bytes.push(0);
            }
            dylib_commands.push((lib_path.clone(), str_bytes));
        }

        let cmd_dylibs_sz: usize = dylib_commands.iter().map(|(_, b)| 24 + b.len()).sum();

        // LC_ID_DYLIB for shared libraries
        let default_install_name = format!("@rpath/{}.dylib", "libadesh_out");
        let id_dylib_str = install_name.unwrap_or(&default_install_name);
        let id_dylib_bytes = {
            let mut b = id_dylib_str.as_bytes().to_vec();
            b.push(0);
            while b.len() % 8 != 0 {
                b.push(0);
            }
            b
        };
        let cmd_id_dylib_sz = if is_shared {
            24 + id_dylib_bytes.len()
        } else {
            0
        };

        let mut ncmds = 0u32;
        if !is_shared {
            ncmds += 1; // LC_SEGMENT_64 (__PAGEZERO)
        }
        ncmds += 1; // LC_SEGMENT_64 (__TEXT)
        if has_data_const {
            ncmds += 1; // LC_SEGMENT_64 (__DATA_CONST)
        }
        ncmds += 1; // LC_SEGMENT_64 (__DATA)
        ncmds += 1; // LC_SEGMENT_64 (__LINKEDIT)
        ncmds += 1; // LC_BUILD_VERSION
        ncmds += 1; // LC_LOAD_DYLINKER
        ncmds += dylib_commands.len() as u32; // LC_LOAD_DYLIB(s)
        if is_shared {
            ncmds += 1; // LC_ID_DYLIB
        } else {
            ncmds += 1; // LC_MAIN
        }
        ncmds += 1; // LC_SYMTAB
        ncmds += 1; // LC_DYSYMTAB
        if has_dynamic {
            ncmds += 1; // LC_DYLD_INFO_ONLY
        }
        if !is_shared {
            ncmds += 1; // LC_CODE_SIGNATURE
        }

        let sizeofcmds = (cmd_pagezero_sz
            + cmd_text_sz
            + cmd_data_const_sz
            + cmd_data_sz
            + cmd_linkedit_sz
            + cmd_build_version_sz
            + cmd_dylinker_sz
            + cmd_dylibs_sz
            + cmd_id_dylib_sz
            + cmd_main_sz
            + cmd_symtab_sz
            + cmd_dysymtab_sz
            + cmd_dyld_info_sz
            + cmd_code_signature_sz) as u32;

        let headers_size = 32 + (sizeofcmds as usize);
        let text_file_offset_start = 0u64; // __TEXT starts at file offset 0 including headers

        output.resize(headers_size, 0);

        // 3. Layout __TEXT segment payload
        let mut current_file_offset = headers_size as u64;
        let mut current_vmaddr = text_vmaddr + current_file_offset;

        let mut text_sec_records = Vec::new();
        // Read-only sections that carry dyld fixups; empty unless this is a
        // dynamic link (see `placement_of`), and a dynamic link always takes
        // the authoritative placement path below.
        let mut data_const_sec_records: Vec<(String, u64, u64, u32, u64)> = Vec::new();
        let mut data_const_file_offset_start = 0u64;
        let mut data_const_vmaddr_start = 0u64;
        let mut data_const_filesize = 0u64;
        let mut data_const_vmsize = 0u64;
        for sec in &text_sections {
            let aligned_off = align_to(current_file_offset, sec.alignment);
            let aligned_va = align_to(current_vmaddr, sec.alignment);
            if aligned_off > current_file_offset {
                let pad = (aligned_off - current_file_offset) as usize;
                output.resize(output.len() + pad, 0);
                current_file_offset = aligned_off;
                current_vmaddr = aligned_va;
            }

            text_sec_records.push((
                sec.name.clone(),
                aligned_va,
                sec.data.len() as u64,
                aligned_off as u32,
                sec.alignment,
            ));
            output.extend_from_slice(&sec.data);
            current_file_offset += sec.data.len() as u64;
            current_vmaddr += sec.data.len() as u64;
        }

        let mut text_filesize = current_file_offset;
        let mut text_vmsize = align_to(current_vmaddr - text_vmaddr, target.page_size);

        // 4. Layout __DATA segment payload
        let mut data_file_offset_start = align_to(current_file_offset, target.page_size);
        let mut data_vmaddr_start = text_vmaddr + text_vmsize;

        if data_file_offset_start > current_file_offset {
            let pad = (data_file_offset_start - current_file_offset) as usize;
            output.resize(output.len() + pad, 0);
            current_file_offset = data_file_offset_start;
            current_vmaddr = data_vmaddr_start;
        }

        let mut data_sec_records = Vec::new();
        for sec in &data_sections {
            let aligned_off = align_to(current_file_offset, sec.alignment);
            let aligned_va = align_to(current_vmaddr, sec.alignment);
            if aligned_off > current_file_offset {
                let pad = (aligned_off - current_file_offset) as usize;
                output.resize(output.len() + pad, 0);
                current_file_offset = aligned_off;
                current_vmaddr = aligned_va;
            }

            data_sec_records.push((
                sec.name.clone(),
                aligned_va,
                sec.size,
                if sec.kind == SectionKind::Bss {
                    0
                } else {
                    aligned_off as u32
                },
                sec.alignment,
            ));
            if sec.kind != SectionKind::Bss {
                output.extend_from_slice(&sec.data);
                current_file_offset += sec.data.len() as u64;
            }
            current_vmaddr += sec.size;
        }

        let mut data_filesize = current_file_offset - data_file_offset_start;
        let mut data_vmsize = align_to(current_vmaddr - data_vmaddr_start, target.page_size);

        // Authoritative placement when virtual addresses are assigned
        let has_layout_vas = merged_sections.iter().any(|s| s.virtual_address != 0);
        if has_layout_vas {
            output.truncate(headers_size);

            let mut text_file_end = headers_size as u64;
            let mut text_va_end = text_vmaddr + headers_size as u64;
            let mut data_off_min: Option<u64> = None;
            let mut data_va_min: Option<u64> = None;
            let mut data_const_off_min: Option<u64> = None;
            let mut data_const_va_min: Option<u64> = None;
            let mut data_const_file_end = 0u64;
            let mut data_const_va_end = 0u64;
            let mut data_file_end = 0u64;
            let mut data_va_end = 0u64;
            text_sec_records.clear();
            data_sec_records.clear();
            data_const_sec_records.clear();

            let base_va = if is_shared
                && merged_sections
                    .iter()
                    .all(|s| s.virtual_address >= 0x100000000)
            {
                0x100000000u64
            } else {
                text_vmaddr
            };

            for sec in merged_sections {
                if sec.virtual_address < base_va {
                    return Err(LinkError::new(
                        ErrorCode::InvalidSection,
                        format!(
                            "section `{}` virtual address 0x{:x} is below the __TEXT vmaddr 0x{:x}",
                            sec.name, sec.virtual_address, base_va
                        ),
                    ));
                }
                let off = sec.virtual_address.checked_sub(base_va).ok_or_else(|| {
                    LinkError::new(
                        ErrorCode::InvalidSection,
                        format!(
                            "section `{}` virtual address 0x{:x} underflows base 0x{:x}",
                            sec.name, sec.virtual_address, base_va
                        ),
                    )
                })?;
                let file_len = if sec.kind == SectionKind::Bss {
                    0
                } else {
                    sec.data.len() as u64
                };
                let mem_len = sec.size.max(sec.data.len() as u64);

                if file_len > 0 {
                    let start = usize::try_from(off).map_err(|_| {
                        LinkError::new(
                            ErrorCode::InvalidSection,
                            format!(
                                "section `{}` offset 0x{:x} exceeds address space",
                                sec.name, off
                            ),
                        )
                    })?;
                    let len = usize::try_from(file_len).map_err(|_| {
                        LinkError::new(
                            ErrorCode::InvalidSection,
                            format!(
                                "section `{}` length 0x{:x} exceeds address space",
                                sec.name, file_len
                            ),
                        )
                    })?;
                    let end = start.checked_add(len).ok_or_else(|| {
                        LinkError::new(
                            ErrorCode::InvalidSection,
                            format!(
                                "section `{}` offset overflow (0x{:x} + 0x{:x})",
                                sec.name, start, len
                            ),
                        )
                    })?;
                    if output.len() < end {
                        output.resize(end, 0);
                    }
                    output[start..end].copy_from_slice(&sec.data);
                }

                let placement = placement_of(sec, rodata_in_data_const);
                if placement == Placement::Text {
                    text_sec_records.push((
                        sec.name.clone(),
                        sec.virtual_address,
                        mem_len,
                        off as u32,
                        sec.alignment,
                    ));
                    text_file_end = text_file_end.max(off + file_len);
                    text_va_end = text_va_end.max(sec.virtual_address + mem_len);
                } else if placement == Placement::DataConst {
                    data_const_sec_records.push((
                        sec.name.clone(),
                        sec.virtual_address,
                        mem_len,
                        off as u32,
                        sec.alignment,
                    ));
                    data_const_off_min = Some(data_const_off_min.map_or(off, |m| m.min(off)));
                    data_const_va_min = Some(
                        data_const_va_min
                            .map_or(sec.virtual_address, |m| m.min(sec.virtual_address)),
                    );
                    data_const_file_end = data_const_file_end.max(off + file_len);
                    data_const_va_end = data_const_va_end.max(sec.virtual_address + mem_len);
                } else {
                    let is_tlv = sec.flags & crate::section::flags::TLV_TABLE != 0;
                    let is_tls = sec.flags & crate::section::flags::TLS != 0;
                    if is_tlv {
                        // The descriptor table is one file-backed section.
                        data_sec_records.push((
                            TLV_VARS_SECTION.to_string(),
                            sec.virtual_address,
                            mem_len,
                            off as u32,
                            sec.alignment,
                        ));
                    } else if is_tls {
                        // Split the merged template into the file-backed image
                        // (`__thread_data`) and the zero-filled tail
                        // (`__thread_bss`), matching how the loader sizes the
                        // per-thread block. Either half may be empty.
                        let file_image = sec.data.len() as u64;
                        let mut idx = 0usize;
                        if file_image > 0 {
                            data_sec_records.push((
                                THREAD_DATA_SECTION.to_string(),
                                sec.virtual_address,
                                file_image,
                                off as u32,
                                sec.alignment,
                            ));
                            idx += 1;
                        }
                        if mem_len > file_image {
                            data_sec_records.push((
                                THREAD_BSS_SECTION.to_string(),
                                sec.virtual_address + file_image,
                                mem_len - file_image,
                                0,
                                sec.alignment,
                            ));
                            idx += 1;
                        }
                        debug_assert_eq!(idx, data_section_records(sec));
                    } else {
                        data_sec_records.push((
                            sec.name.clone(),
                            sec.virtual_address,
                            mem_len,
                            if sec.kind == SectionKind::Bss {
                                0
                            } else {
                                off as u32
                            },
                            sec.alignment,
                        ));
                    }
                    data_off_min = Some(data_off_min.map_or(off, |m| m.min(off)));
                    data_va_min = Some(
                        data_va_min.map_or(sec.virtual_address, |m| m.min(sec.virtual_address)),
                    );
                    data_file_end = data_file_end.max(off + file_len);
                    data_va_end = data_va_end.max(sec.virtual_address + mem_len);
                }
            }

            if has_data_const {
                let off = data_const_off_min.unwrap_or(0);
                let va = data_const_va_min.unwrap_or(0);
                data_const_file_offset_start = off;
                data_const_vmaddr_start = va;
                data_const_filesize = data_const_file_end.saturating_sub(off);
                data_const_vmsize =
                    align_to(data_const_va_end.saturating_sub(va), target.page_size);
            }

            let (data_off_start, data_va_start) = match (data_off_min, data_va_min) {
                (Some(off), Some(va)) => (off, va),
                _ => (
                    align_to(text_file_end.max(data_const_file_end), target.page_size),
                    align_to(text_va_end.max(data_const_va_end), target.page_size),
                ),
            };

            text_vmsize = align_to(text_va_end - text_vmaddr, target.page_size);
            text_filesize = text_vmsize.min(data_const_file_offset_start.max(data_off_start));
            data_file_offset_start = data_off_start;
            data_vmaddr_start = data_va_start;
            data_filesize = data_file_end.saturating_sub(data_off_start);
            data_vmsize = align_to(data_va_end.saturating_sub(data_va_start), target.page_size);
        }

        // ── Dynamic-linking fixup synthesis ──
        //
        // The layout stage reserved one `__got` slot per imported function.
        // `nonlazy_binds` (built below) asks dyld to write every import's
        // real address into its slot while loading the image, so the
        // synthesized proxy stubs never do any work of their own.
        let mut dyld_rebase_off = 0u32;
        let mut dyld_rebase_size = 0u32;
        let mut dyld_bind_off = 0u32;
        let mut dyld_bind_size = 0u32;
        let dyld_lazy_off = 0u32;
        let dyld_lazy_size = 0u32;
        let mut dyld_export_off = 0u32;
        let mut dyld_export_size = 0u32;
        let mut rebase_bytes: Vec<u8> = Vec::new();
        let mut bind_bytes: Vec<u8> = Vec::new();
        let mut export_bytes: Vec<u8> = Vec::new();
        let mut rebase_targets: Vec<RebaseTarget> = Vec::new();
        let mut indirect_symbols: Vec<u32> = Vec::new();

        // Segment indexes dyld uses in rebase/bind streams: LC_SEGMENT_64
        // commands in order. Executables put __PAGEZERO first, and
        // __DATA_CONST only counts when it actually exists.
        let text_seg_index: u8 = if is_shared { 0 } else { 1 };
        let data_const_seg_index: u8 = text_seg_index + 1;
        let data_seg_index: u8 = text_seg_index + 1 + u8::from(has_data_const);

        // The layout stage reserves one `__got` slot per imported function;
        // the proxy stubs the linker synthesizes resolve `__got_<name>` to
        // those slots. Verify the handshake before binding any of them.
        let got_section: Option<&MergedSection> = if has_dynamic {
            if !has_layout_vas {
                return Err(LinkError::new(
                    ErrorCode::InvalidSection,
                    "dynamic Mach-O output requires layout-assigned section \
                     addresses (the linker must run the layout stage)",
                ));
            }
            match merged_sections.iter().find(|s| s.name == GOT_SECTION) {
                Some(section) => Some(section),
                None if imports.is_empty() => None,
                None => {
                    return Err(LinkError::new(
                        ErrorCode::InvalidSection,
                        format!(
                            "dynamic Mach-O link has {} imports but the layout stage did not \
                             create the `{}` section",
                            imports.len(),
                            GOT_SECTION
                        ),
                    ));
                }
            }
        } else {
            None
        };

        if let Some(section) = got_section {
            let expected_len = (imports.len() * 8) as u64;
            if section.data.len() as u64 != expected_len {
                return Err(LinkError::new(
                    ErrorCode::InvalidSection,
                    format!(
                        "`{}` section is {} bytes but {} imports need {} bytes",
                        GOT_SECTION,
                        section.data.len(),
                        imports.len(),
                        expected_len
                    ),
                ));
            }
            for (i, imp) in imports.iter().enumerate() {
                let expected = section.virtual_address + (i as u64) * 8;
                if imp.got_slot_va != expected {
                    return Err(LinkError::new(
                        ErrorCode::InvalidSection,
                        format!(
                            "import `{}` got slot 0x{:x} does not match the `{}` layout \
                             (0x{:x} + {}*8)",
                            imp.raw_name, imp.got_slot_va, GOT_SECTION, section.virtual_address, i
                        ),
                    ));
                }
            }
        }

        let mut nonlazy_binds: Vec<BindTarget> = Vec::new();

        // Layout-recorded data binds (TLV descriptor thunks bound to
        // `__tlv_bootstrap`). These live in a fixup-writable segment and must
        // be emitted even when there are no function imports to synthesize
        // stubs for.
        //
        // Map an address dyld must write to its `(segment index, offset within
        // that segment)` pair. `__TEXT` is deliberately not an option: it is
        // mapped `r-x` with `maxprot r-x`, so dyld cannot gain write access to
        // apply the fixup and the process dies with SIGBUS while launching.
        let locate_fixup = |va: u64, what: &str| -> LinkResult<(u8, u64)> {
            if has_data_const
                && va >= data_const_vmaddr_start
                && va < data_const_vmaddr_start + data_const_vmsize
            {
                return Ok((data_const_seg_index, va - data_const_vmaddr_start));
            }
            if va >= data_vmaddr_start && va < data_vmaddr_start + data_vmsize {
                return Ok((data_seg_index, va - data_vmaddr_start));
            }
            Err(LinkError::new(
                ErrorCode::InvalidSection,
                format!(
                    "cannot {what} address 0x{va:x}: a dyld fixup target must live in \
                     `__DATA_CONST` or `__DATA` (`__TEXT` is mapped `r-x` with \
                     `maxprot r-x`, so dyld cannot write the link-time pointer there)",
                ),
            ))
        };
        // One eager bind per imported function: dyld writes the resolved
        // address into the import's `__got` slot at load time, before any
        // code runs, so the proxy stubs never need a binding trampoline.
        if got_section.is_some() {
            for imp in imports {
                let (segment_index, offset) = locate_fixup(imp.got_slot_va, "bind")?;
                nonlazy_binds.push(BindTarget {
                    segment_index,
                    offset,
                    bind_type: BIND_TYPE_POINTER,
                    ordinal: LIBSYSTEM_ORDINAL,
                    symbol: imp.bind_name.clone(),
                    addend: 0,
                });
            }
        }

        if has_dynamic {
            if let Some(info) = dynamic {
                for b in &info.binds {
                    let (segment_index, offset) = locate_fixup(b.va, "bind")?;
                    nonlazy_binds.push(BindTarget {
                        segment_index,
                        offset,
                        bind_type: BIND_TYPE_POINTER,
                        ordinal: LIBSYSTEM_ORDINAL,
                        symbol: b.symbol.clone(),
                        addend: b.addend,
                    });
                }
            }
        }
        if !nonlazy_binds.is_empty() {
            bind_bytes = dynamic::encode_bind_opcodes(&nonlazy_binds);
        }

        // Rebase stream, part 2 (with or without imports): every link-time
        // absolute pointer the layout stage recorded must be rebased when
        // the PIE image slides.
        if has_dynamic {
            if let Some(info) = dynamic {
                for &va in &info.rebases {
                    let (segment_index, offset) = locate_fixup(va, "rebase")?;
                    rebase_targets.push(RebaseTarget {
                        segment_index,
                        offset,
                        kind: RebaseKind::Pointer,
                    });
                }
            }
            rebase_targets.sort_by_key(|t| (t.segment_index, t.offset, t.kind.type_imm()));
            rebase_bytes = dynamic::encode_rebase_opcodes(&rebase_targets);

            // Export trie: shared libraries export their global symbols;
            // executables export nothing (dyld accepts an empty trie).
            export_bytes = if is_shared {
                dynamic::build_export_trie(&dynamic.map(|d| d.exports.clone()).unwrap_or_default())
            } else {
                vec![0, 0]
            };
        }

        // 5. Layout __LINKEDIT (Symbols, Strings, and Dysymtab)
        current_file_offset = output.len() as u64;
        let linkedit_file_offset = align_to(current_file_offset, target.page_size);
        let linkedit_vmaddr = data_vmaddr_start + data_vmsize;

        if linkedit_file_offset > current_file_offset {
            let pad = (linkedit_file_offset - current_file_offset) as usize;
            output.resize(output.len() + pad, 0);
        }

        let mut strtab = vec![0u8, 0u8, 0u8, 0u8]; // start with 4-byte padding

        let mut input_sec_to_out: HashMap<(usize, usize), usize> = HashMap::new();
        for (out_idx, sec) in merged_sections.iter().enumerate() {
            for &(f_idx, s_idx, _) in &sec.input_sections {
                input_sec_to_out.insert((f_idx, s_idx), out_idx);
            }
        }
        // `n_sect` is the 1-based index of the section in the load-command
        // section table, so it has to be counted in the same segment order the
        // commands are emitted in and with the same partition rule.
        let placements: Vec<Placement> = merged_sections
            .iter()
            .map(|sec| placement_of(sec, rodata_in_data_const))
            .collect();
        let text_count = placements.iter().filter(|p| **p == Placement::Text).count();
        let data_const_count = placements
            .iter()
            .filter(|p| **p == Placement::DataConst)
            .count();
        let mut n_sect_for: Vec<u8> = Vec::with_capacity(merged_sections.len());
        let mut text_seen = 0usize;
        let mut data_const_seen = 0usize;
        let mut data_seen = 0usize;
        for placement in placements {
            match placement {
                Placement::Text => {
                    text_seen += 1;
                    n_sect_for.push(text_seen as u8);
                }
                Placement::DataConst => {
                    data_const_seen += 1;
                    n_sect_for.push((text_count + data_const_seen) as u8);
                }
                Placement::Data => {
                    data_seen += 1;
                    n_sect_for.push((text_count + data_const_count + data_seen) as u8);
                }
            }
        }
        let output_section_number = |sym: &Symbol| -> Option<u8> {
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
                .and_then(|idx| n_sect_for.get(idx).copied())
        };

        // Partition symbols into: locals, external defined, undefined (for LC_DYSYMTAB)
        let mut local_syms = Vec::new();
        let mut extdef_syms = Vec::new();
        let mut undef_syms = Vec::new();

        for sym in symbols {
            if sym.name.is_empty() {
                continue;
            }
            if !sym.is_defined {
                undef_syms.push(sym);
            } else if sym.binding == SymbolBinding::Local {
                local_syms.push(sym);
            } else {
                extdef_syms.push(sym);
            }
        }

        // Dynamic imports must not appear as defined symbols: the proxy
        // stubs synthesized before layout are implementation glue, and the
        // symtab entry dyld binds against is the underscore-prefixed name
        // as undefined.
        if has_dynamic {
            let import_names: std::collections::HashSet<&str> =
                imports.iter().map(|imp| imp.raw_name.as_str()).collect();
            extdef_syms.retain(|sym| !import_names.contains(sym.name.as_str()));
        }

        // Synthetic undefined entries: one per eagerly bound import.
        // Two-level namespace: the library ordinal lives in the high byte of
        // n_desc (libSystem is ordinal 1, the first LOAD_DYLIB).
        let mut synthetic_undefs: Vec<(String, u16)> = Vec::new();
        if got_section.is_some() {
            for imp in imports {
                synthetic_undefs.push((imp.bind_name.clone(), 0x0100));
            }
        }

        let ilocalsym = 0u32;
        let nlocalsym = local_syms.len() as u32;
        let iextdefsym = nlocalsym;
        let nextdefsym = extdef_syms.len() as u32;
        let iundefsym = iextdefsym + nextdefsym;
        let nundefsym = (undef_syms.len() + synthetic_undefs.len()) as u32;

        let mut nlists = Vec::new();

        let mut encode_sym = |sym: &Symbol, n_type: u8, n_desc: u16| {
            let n_strx = strtab.len() as u32;
            strtab.extend_from_slice(sym.name.as_bytes());
            strtab.push(0);

            let n_sect = if sym.is_defined {
                output_section_number(sym).unwrap_or(0)
            } else {
                0
            };

            nlists.push(Nlist64 {
                n_strx,
                n_type,
                n_sect,
                n_desc,
                n_value: sym.value,
            });
        };

        for sym in &local_syms {
            encode_sym(sym, N_SECT, 0); // local section symbol
        }
        for sym in &extdef_syms {
            encode_sym(sym, N_SECT | N_EXT, 0);
        }
        for sym in &undef_syms {
            encode_sym(sym, N_UNDF | N_EXT, 0);
        }
        for (name, n_desc) in &synthetic_undefs {
            let sym = Symbol {
                name: name.clone(),
                binding: SymbolBinding::Global,
                visibility: crate::symbol::SymbolVisibility::Default,
                sym_type: crate::symbol::SymbolType::Function,
                section_index: None,
                value: 0,
                size: 0,
                is_defined: false,
                is_imported: true,
                is_exported: false,
                file_index: None,
                alias_of: None,
                comdat_group: None,
                version: None,
            };
            encode_sym(&sym, N_UNDF | N_EXT, *n_desc);
        }

        // Indirect symbol table: one entry per `__got` slot, in section
        // order, each the symtab index of the symbol the slot binds.
        if got_section.is_some() {
            let first_import_index = iundefsym + (undef_syms.len() as u32);
            for i in 0..imports.len() as u32 {
                indirect_symbols.push(first_import_index + i);
            }
        }

        // __LINKEDIT payload order: dyld info streams (rebase, bind, lazy
        // bind, export trie), indirect symbol table, symbol table, string
        // table. dyld only requires self-consistent offsets; this is the
        // conventional order.
        let mut push_blob = |bytes: &[u8]| -> (u32, u32) {
            let off = output.len() as u32;
            output.extend_from_slice(bytes);
            while output.len() % 8 != 0 {
                output.push(0);
            }
            (off, bytes.len() as u32)
        };
        if has_dynamic {
            // An unused stream is encoded as offset 0 / size 0; dyld ignores
            // the offset once the size is zero.
            if !rebase_bytes.is_empty() {
                (dyld_rebase_off, dyld_rebase_size) = push_blob(&rebase_bytes);
            }
            if !bind_bytes.is_empty() {
                (dyld_bind_off, dyld_bind_size) = push_blob(&bind_bytes);
            }
            if !export_bytes.is_empty() {
                (dyld_export_off, dyld_export_size) = push_blob(&export_bytes);
            }
        }

        let indirectsymoff = if indirect_symbols.is_empty() {
            0u32
        } else {
            let off = output.len() as u32;
            for index in &indirect_symbols {
                output.extend_from_slice(&index.to_le_bytes());
            }
            off
        };
        let nindirectsyms = indirect_symbols.len() as u32;

        let symoff = output.len() as u32;
        for nl in &nlists {
            output.extend_from_slice(&nl.n_strx.to_le_bytes());
            output.push(nl.n_type);
            output.push(nl.n_sect);
            output.extend_from_slice(&nl.n_desc.to_le_bytes());
            output.extend_from_slice(&nl.n_value.to_le_bytes());
        }

        let stroff = output.len() as u32;
        let strsize = strtab.len() as u32;
        output.extend_from_slice(&strtab);

        let linkedit_filesize = (output.len() as u64) - linkedit_file_offset;
        let linkedit_vmsize = align_to(linkedit_filesize, target.page_size);

        // 6. Fill Mach-O Header at offset 0 (32 bytes)
        let filetype = if is_shared { MH_DYLIB } else { MH_EXECUTE };
        // MH_NOUNDEFS may only be set when nothing is left for dyld to
        // resolve; imports and pre-existing undefineds make it a lie.
        let has_undefineds = nundefsym > 0;
        let flags = if is_shared {
            MH_DYLDLINK | MH_TWOLEVEL | if has_undefineds { 0 } else { MH_NOUNDEFS }
        } else {
            MH_DYLDLINK | MH_TWOLEVEL | MH_PIE | if has_undefineds { 0 } else { MH_NOUNDEFS }
        };

        let hdr = MachHeader64 {
            magic: MH_MAGIC_64,
            cputype,
            cpusubtype,
            filetype,
            ncmds,
            sizeofcmds,
            flags,
            reserved: 0,
        };

        let mut hdr_buf = Vec::with_capacity(32);
        hdr_buf.extend_from_slice(&hdr.magic.to_le_bytes());
        hdr_buf.extend_from_slice(&hdr.cputype.to_le_bytes());
        hdr_buf.extend_from_slice(&hdr.cpusubtype.to_le_bytes());
        hdr_buf.extend_from_slice(&hdr.filetype.to_le_bytes());
        hdr_buf.extend_from_slice(&hdr.ncmds.to_le_bytes());
        hdr_buf.extend_from_slice(&hdr.sizeofcmds.to_le_bytes());
        hdr_buf.extend_from_slice(&hdr.flags.to_le_bytes());
        hdr_buf.extend_from_slice(&hdr.reserved.to_le_bytes());
        output[0..32].copy_from_slice(&hdr_buf);

        // 7. Write Load Commands
        let mut cmd_buf = Vec::new();

        // LC_SEGMENT_64: __PAGEZERO (executables only)
        if !is_shared {
            cmd_buf.extend_from_slice(&LC_SEGMENT_64.to_le_bytes());
            cmd_buf.extend_from_slice(&(cmd_pagezero_sz as u32).to_le_bytes());
            let mut pz_name = [0u8; 16];
            pz_name[0..10].copy_from_slice(b"__PAGEZERO");
            cmd_buf.extend_from_slice(&pz_name);
            cmd_buf.extend_from_slice(&0u64.to_le_bytes()); // vmaddr = 0
            cmd_buf.extend_from_slice(&pagezero_size.to_le_bytes()); // vmsize = 4GB
            cmd_buf.extend_from_slice(&0u64.to_le_bytes()); // fileoff = 0
            cmd_buf.extend_from_slice(&0u64.to_le_bytes()); // filesize = 0
            cmd_buf.extend_from_slice(&VM_PROT_NONE.to_le_bytes()); // maxprot = 0
            cmd_buf.extend_from_slice(&VM_PROT_NONE.to_le_bytes()); // initprot = 0
            cmd_buf.extend_from_slice(&0u32.to_le_bytes()); // nsects = 0
            cmd_buf.extend_from_slice(&0u32.to_le_bytes()); // flags = 0
        }

        // LC_SEGMENT_64: __TEXT
        cmd_buf.extend_from_slice(&LC_SEGMENT_64.to_le_bytes());
        cmd_buf.extend_from_slice(&(cmd_text_sz as u32).to_le_bytes());
        let mut text_name = [0u8; 16];
        text_name[0..6].copy_from_slice(b"__TEXT");
        cmd_buf.extend_from_slice(&text_name);
        cmd_buf.extend_from_slice(&text_vmaddr.to_le_bytes());
        cmd_buf.extend_from_slice(&text_vmsize.to_le_bytes());
        cmd_buf.extend_from_slice(&text_file_offset_start.to_le_bytes());
        cmd_buf.extend_from_slice(&text_filesize.to_le_bytes());
        cmd_buf.extend_from_slice(&(VM_PROT_READ | VM_PROT_EXECUTE).to_le_bytes());
        cmd_buf.extend_from_slice(&(VM_PROT_READ | VM_PROT_EXECUTE).to_le_bytes());
        cmd_buf.extend_from_slice(&(text_sec_records.len() as u32).to_le_bytes());
        cmd_buf.extend_from_slice(&0u32.to_le_bytes());

        for (name, va, sz, off, align) in &text_sec_records {
            let mut sname = [0u8; 16];
            let name_bytes = name.as_bytes();
            sname[0..name_bytes.len().min(16)]
                .copy_from_slice(&name_bytes[0..name_bytes.len().min(16)]);
            cmd_buf.extend_from_slice(&sname);
            cmd_buf.extend_from_slice(&text_name);
            cmd_buf.extend_from_slice(&va.to_le_bytes());
            cmd_buf.extend_from_slice(&sz.to_le_bytes());
            cmd_buf.extend_from_slice(&off.to_le_bytes());
            let align_pow2 = if *align > 1 {
                (*align as f64).log2().round() as u32
            } else {
                0
            };
            cmd_buf.extend_from_slice(&align_pow2.to_le_bytes());
            cmd_buf.extend_from_slice(&0u32.to_le_bytes()); // reloff
            cmd_buf.extend_from_slice(&0u32.to_le_bytes()); // nreloc
            let is_code = name.contains("text") || name.contains("code");
            let section_flags = if is_code {
                S_ATTR_PURE_INSTRUCTIONS | S_ATTR_SOME_INSTRUCTIONS
            } else {
                S_REGULAR
            };
            cmd_buf.extend_from_slice(&section_flags.to_le_bytes());
            cmd_buf.extend_from_slice(&0u32.to_le_bytes()); // reserved1
            cmd_buf.extend_from_slice(&0u32.to_le_bytes()); // reserved2
            cmd_buf.extend_from_slice(&0u32.to_le_bytes()); // reserved3
        }

        // LC_SEGMENT_64: __DATA_CONST
        //
        // Read-only data of a dyld image. It is `rw-` only for the moment
        // dyld applies the slide and the binds; the program itself never
        // writes it. It has to be its own segment because __TEXT's `maxprot`
        // is `r-x`, which would make those fixups fault.
        if has_data_const {
            let mut dc_name = [0u8; 16];
            dc_name[0..DATA_CONST_SEGMENT.len()].copy_from_slice(DATA_CONST_SEGMENT.as_bytes());
            cmd_buf.extend_from_slice(&LC_SEGMENT_64.to_le_bytes());
            cmd_buf.extend_from_slice(&(cmd_data_const_sz as u32).to_le_bytes());
            cmd_buf.extend_from_slice(&dc_name);
            cmd_buf.extend_from_slice(&data_const_vmaddr_start.to_le_bytes());
            cmd_buf.extend_from_slice(&data_const_vmsize.to_le_bytes());
            cmd_buf.extend_from_slice(&data_const_file_offset_start.to_le_bytes());
            cmd_buf.extend_from_slice(&data_const_filesize.to_le_bytes());
            cmd_buf.extend_from_slice(&(VM_PROT_READ | VM_PROT_WRITE).to_le_bytes());
            cmd_buf.extend_from_slice(&(VM_PROT_READ | VM_PROT_WRITE).to_le_bytes());
            cmd_buf.extend_from_slice(&(data_const_sec_records.len() as u32).to_le_bytes());
            cmd_buf.extend_from_slice(&0u32.to_le_bytes());

            for (name, va, sz, off, align) in &data_const_sec_records {
                push_data_section_record(
                    &mut cmd_buf,
                    &dc_name,
                    name,
                    *va,
                    *sz,
                    *off,
                    *align,
                    false,
                );
            }
        }

        // LC_SEGMENT_64: __DATA
        cmd_buf.extend_from_slice(&LC_SEGMENT_64.to_le_bytes());
        cmd_buf.extend_from_slice(&(cmd_data_sz as u32).to_le_bytes());
        let mut data_name = [0u8; 16];
        data_name[0..6].copy_from_slice(b"__DATA");
        cmd_buf.extend_from_slice(&data_name);
        cmd_buf.extend_from_slice(&data_vmaddr_start.to_le_bytes());
        cmd_buf.extend_from_slice(&data_vmsize.to_le_bytes());
        cmd_buf.extend_from_slice(&data_file_offset_start.to_le_bytes());
        cmd_buf.extend_from_slice(&data_filesize.to_le_bytes());
        cmd_buf.extend_from_slice(&(VM_PROT_READ | VM_PROT_WRITE).to_le_bytes());
        cmd_buf.extend_from_slice(&(VM_PROT_READ | VM_PROT_WRITE).to_le_bytes());
        cmd_buf.extend_from_slice(&(data_sec_records.len() as u32).to_le_bytes());
        cmd_buf.extend_from_slice(&0u32.to_le_bytes());

        for (name, va, sz, off, align) in &data_sec_records {
            let is_bss = merged_sections
                .iter()
                .any(|s| s.name == *name && s.kind == SectionKind::Bss);
            push_data_section_record(
                &mut cmd_buf,
                &data_name,
                name,
                *va,
                *sz,
                *off,
                *align,
                is_bss,
            );
        }

        // LC_SEGMENT_64: __LINKEDIT
        cmd_buf.extend_from_slice(&LC_SEGMENT_64.to_le_bytes());
        cmd_buf.extend_from_slice(&(cmd_linkedit_sz as u32).to_le_bytes());
        let mut linkedit_name = [0u8; 16];
        linkedit_name[0..10].copy_from_slice(b"__LINKEDIT");
        cmd_buf.extend_from_slice(&linkedit_name);
        cmd_buf.extend_from_slice(&linkedit_vmaddr.to_le_bytes());
        cmd_buf.extend_from_slice(&linkedit_vmsize.to_le_bytes());
        cmd_buf.extend_from_slice(&linkedit_file_offset.to_le_bytes());
        cmd_buf.extend_from_slice(&linkedit_filesize.to_le_bytes());
        cmd_buf.extend_from_slice(&VM_PROT_READ.to_le_bytes());
        cmd_buf.extend_from_slice(&VM_PROT_READ.to_le_bytes());
        cmd_buf.extend_from_slice(&0u32.to_le_bytes());
        cmd_buf.extend_from_slice(&0u32.to_le_bytes());

        // LC_BUILD_VERSION (24 bytes)
        cmd_buf.extend_from_slice(&LC_BUILD_VERSION.to_le_bytes());
        cmd_buf.extend_from_slice(&(cmd_build_version_sz as u32).to_le_bytes());
        cmd_buf.extend_from_slice(&PLATFORM_MACOS.to_le_bytes());
        cmd_buf.extend_from_slice(&0x000B0000u32.to_le_bytes()); // minos = macOS 11.0
        cmd_buf.extend_from_slice(&0x000B0000u32.to_le_bytes()); // sdk = macOS 11.0
        cmd_buf.extend_from_slice(&0u32.to_le_bytes()); // ntools = 0

        // LC_LOAD_DYLINKER
        cmd_buf.extend_from_slice(&LC_LOAD_DYLINKER.to_le_bytes());
        cmd_buf.extend_from_slice(&(cmd_dylinker_sz as u32).to_le_bytes());
        cmd_buf.extend_from_slice(&12u32.to_le_bytes()); // offset to string
        cmd_buf.extend_from_slice(dylinker_str);

        // LC_ID_DYLIB (if shared)
        if is_shared {
            let cmd_sz = (24 + id_dylib_bytes.len()) as u32;
            cmd_buf.extend_from_slice(&LC_ID_DYLIB.to_le_bytes());
            cmd_buf.extend_from_slice(&cmd_sz.to_le_bytes());
            cmd_buf.extend_from_slice(&24u32.to_le_bytes()); // name offset
            cmd_buf.extend_from_slice(&1u32.to_le_bytes()); // timestamp
            cmd_buf.extend_from_slice(&0x010000u32.to_le_bytes()); // current_version = 1.0.0
            cmd_buf.extend_from_slice(&0x010000u32.to_le_bytes()); // compatibility_version = 1.0.0
            cmd_buf.extend_from_slice(&id_dylib_bytes);
        }

        // LC_LOAD_DYLIB commands
        for (_, str_bytes) in &dylib_commands {
            let cmd_sz = (24 + str_bytes.len()) as u32;
            cmd_buf.extend_from_slice(&LC_LOAD_DYLIB.to_le_bytes());
            cmd_buf.extend_from_slice(&cmd_sz.to_le_bytes());
            cmd_buf.extend_from_slice(&24u32.to_le_bytes()); // name offset
            cmd_buf.extend_from_slice(&2u32.to_le_bytes()); // timestamp
            cmd_buf.extend_from_slice(&0x05000000u32.to_le_bytes()); // current_version
            cmd_buf.extend_from_slice(&0x01000000u32.to_le_bytes()); // compatibility_version
            cmd_buf.extend_from_slice(str_bytes);
        }

        // LC_MAIN (executable only)
        if !is_shared {
            let entryoff = if entry_va >= text_vmaddr {
                entry_va - text_vmaddr
            } else {
                headers_size as u64
            };
            cmd_buf.extend_from_slice(&LC_MAIN.to_le_bytes());
            cmd_buf.extend_from_slice(&(cmd_main_sz as u32).to_le_bytes());
            cmd_buf.extend_from_slice(&entryoff.to_le_bytes());
            cmd_buf.extend_from_slice(&0u64.to_le_bytes()); // stacksize default
        }

        // LC_DYLD_INFO_ONLY: everything dyld needs to rebase the image,
        // bind the GOT, lazily bind the stubs, and (for dylibs) resolve
        // exports. Emitted exactly when the link produced dynamic info.
        if has_dynamic {
            cmd_buf.extend_from_slice(&LC_DYLD_INFO_ONLY.to_le_bytes());
            cmd_buf.extend_from_slice(&(cmd_dyld_info_sz as u32).to_le_bytes());
            cmd_buf.extend_from_slice(&dyld_rebase_off.to_le_bytes());
            cmd_buf.extend_from_slice(&dyld_rebase_size.to_le_bytes());
            cmd_buf.extend_from_slice(&dyld_bind_off.to_le_bytes());
            cmd_buf.extend_from_slice(&dyld_bind_size.to_le_bytes());
            // weak binding is unused: no weak imports are ever emitted.
            cmd_buf.extend_from_slice(&0u32.to_le_bytes());
            cmd_buf.extend_from_slice(&0u32.to_le_bytes());
            cmd_buf.extend_from_slice(&dyld_lazy_off.to_le_bytes());
            cmd_buf.extend_from_slice(&dyld_lazy_size.to_le_bytes());
            cmd_buf.extend_from_slice(&dyld_export_off.to_le_bytes());
            cmd_buf.extend_from_slice(&dyld_export_size.to_le_bytes());
        }

        // LC_SYMTAB
        cmd_buf.extend_from_slice(&LC_SYMTAB.to_le_bytes());
        cmd_buf.extend_from_slice(&(cmd_symtab_sz as u32).to_le_bytes());
        cmd_buf.extend_from_slice(&symoff.to_le_bytes());
        cmd_buf.extend_from_slice(&(nlists.len() as u32).to_le_bytes());
        cmd_buf.extend_from_slice(&stroff.to_le_bytes());
        cmd_buf.extend_from_slice(&strsize.to_le_bytes());

        // LC_DYSYMTAB (80 bytes)
        let dysym = DysymtabCommand {
            cmd: LC_DYSYMTAB,
            cmdsize: 80,
            ilocalsym,
            nlocalsym,
            iextdefsym,
            nextdefsym,
            iundefsym,
            nundefsym,
            tocoff: 0,
            ntoc: 0,
            modtaboff: 0,
            nmodtab: 0,
            extrefsymoff: 0,
            nextrefsyms: 0,
            indirectsymoff,
            nindirectsyms,
            extreloff: 0,
            nextrel: 0,
            locreloff: 0,
            nlocrel: 0,
        };
        cmd_buf.extend_from_slice(&dysym.cmd.to_le_bytes());
        cmd_buf.extend_from_slice(&dysym.cmdsize.to_le_bytes());
        cmd_buf.extend_from_slice(&dysym.ilocalsym.to_le_bytes());
        cmd_buf.extend_from_slice(&dysym.nlocalsym.to_le_bytes());
        cmd_buf.extend_from_slice(&dysym.iextdefsym.to_le_bytes());
        cmd_buf.extend_from_slice(&dysym.nextdefsym.to_le_bytes());
        cmd_buf.extend_from_slice(&dysym.iundefsym.to_le_bytes());
        cmd_buf.extend_from_slice(&dysym.nundefsym.to_le_bytes());
        cmd_buf.extend_from_slice(&dysym.tocoff.to_le_bytes());
        cmd_buf.extend_from_slice(&dysym.ntoc.to_le_bytes());
        cmd_buf.extend_from_slice(&dysym.modtaboff.to_le_bytes());
        cmd_buf.extend_from_slice(&dysym.nmodtab.to_le_bytes());
        cmd_buf.extend_from_slice(&dysym.extrefsymoff.to_le_bytes());
        cmd_buf.extend_from_slice(&dysym.nextrefsyms.to_le_bytes());
        cmd_buf.extend_from_slice(&dysym.indirectsymoff.to_le_bytes());
        cmd_buf.extend_from_slice(&dysym.nindirectsyms.to_le_bytes());
        cmd_buf.extend_from_slice(&dysym.extreloff.to_le_bytes());
        cmd_buf.extend_from_slice(&dysym.nextrel.to_le_bytes());
        cmd_buf.extend_from_slice(&dysym.locreloff.to_le_bytes());
        cmd_buf.extend_from_slice(&dysym.nlocrel.to_le_bytes());

        // LC_CODE_SIGNATURE (executables only, patched below by apply_to_macho_bytes)
        if !is_shared {
            cmd_buf.extend_from_slice(&crate::macho::codesign::LC_CODE_SIGNATURE.to_le_bytes());
            cmd_buf.extend_from_slice(&16u32.to_le_bytes()); // cmdsize = 16
            cmd_buf.extend_from_slice(&0u32.to_le_bytes()); // dataoff (patched by apply_to_macho_bytes)
            cmd_buf.extend_from_slice(&0u32.to_le_bytes()); // datasize (patched by apply_to_macho_bytes)
        }

        output[32..32 + cmd_buf.len()].copy_from_slice(&cmd_buf);

        // macOS 11+ and Apple Silicon require every executable binary to be
        // ad-hoc code signed; apply pure-Rust signer to output.
        if !is_shared {
            crate::macho::codesign::apply_to_macho_bytes(&mut output, "a.out").map_err(|e| {
                LinkError::new(
                    ErrorCode::SecurityViolation,
                    format!("failed to ad-hoc code sign Mach-O executable: {}", e),
                )
            })?;
        }

        Ok(output)
    }
}
