//! Mach-O 64-bit Executable and Dynamic Library Writer.

use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::macho::dynamic::{
    self, BIND_TYPE_POINTER, BindTarget, DYLD_STUB_BINDER, LIBSYSTEM_ORDINAL, LazyBindTarget,
    MachODynamicInfo, RebaseKind, RebaseTarget,
};
use crate::macho::header::*;
use crate::section::{MergedSection, SectionKind, align_to};
use crate::symbol::{Symbol, SymbolBinding};
use crate::target::{Arch, Target};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Name of the synthesized section holding one lazy pointer per imported
/// function. Created by the layout stage; slot VAs are registered as
/// `__la_symbol_ptr_<symbol>` so the linker-synthesized proxy stubs can
/// `jmp *[rip+slot]`.
pub const LA_SYMBOL_PTR_SECTION: &str = "__la_symbol_ptr";

/// Name of the writer-synthesized section holding `dyld_stub_binder`'s
/// non-lazy GOT slot (consumed by the stub helper tail).
pub const GOT_SECTION: &str = "__got";

/// Name of the writer-synthesized section holding the lazy-binding helper
/// code (one push+jmp entry per import plus the binder tail).
pub const STUB_HELPER_SECTION: &str = "__stub_helper";

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
        let mut bytes = Self::encode_macho_with_dynamic(
            target,
            entry_va,
            merged_sections,
            symbols,
            is_shared,
            libraries,
            install_name,
            dynamic,
        )?;

        // Apple Silicon (ARM64) and macOS 11+ require every executable to be
        // signed with at least an ad-hoc code signature to be launched by XNU.
        // Apply our built-in pure-Rust signer instead of shelling out to the
        // external `codesign` tool, enabling cross-compilation.
        if !is_shared && (target.arch == crate::target::Arch::AArch64) {
            let ident = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("a.out");
            // Best-effort: if we cannot insert the signature (e.g. no free
            // header space), still write the binary without one rather than
            // failing the link.
            let _ = crate::macho::codesign::apply_to_macho_bytes(&mut bytes, ident);
        }

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

        // 1. Partition merged sections into __TEXT and __DATA
        let mut text_sections = Vec::new();
        let mut data_sections = Vec::new();

        for sec in merged_sections {
            if sec.is_executable() || (!sec.is_writable() && sec.kind != SectionKind::Bss) {
                text_sections.push(sec.clone());
            } else {
                data_sections.push(sec.clone());
            }
        }

        // Dynamic images carry `LC_DYLD_INFO_ONLY` (rebasing is meaningful
        // even with no imports); the stub-helper and GOT sections exist only
        // when there are imports to bind.
        let has_dynamic = dynamic.is_some();
        let imports = dynamic.map(|d| d.imports.as_slice()).unwrap_or(&[]);
        let has_stub_sections = has_dynamic && !imports.is_empty();

        // 2. Load Commands Size Calculation
        let cmd_pagezero_sz = if is_shared { 0usize } else { 72usize };
        let cmd_text_sz = 72 + (text_sections.len() + usize::from(has_stub_sections)) * 80;
        let cmd_data_sz = 72 + (data_sections.len() + usize::from(has_stub_sections)) * 80;
        let cmd_linkedit_sz = 72usize;
        let cmd_build_version_sz = 24usize;
        let dylinker_str = b"/usr/lib/dyld\0\0\0\0\0\0\0"; // padded to 20 bytes so 12 + 20 = 32 (8-byte aligned)
        let cmd_dylinker_sz = 12 + dylinker_str.len();
        let cmd_main_sz = if is_shared { 0usize } else { 24usize };
        let cmd_symtab_sz = 24usize;
        let cmd_dysymtab_sz = 80usize;
        let cmd_dyld_info_sz = if has_dynamic { 48usize } else { 0usize };

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

        let sizeofcmds = (cmd_pagezero_sz
            + cmd_text_sz
            + cmd_data_sz
            + cmd_linkedit_sz
            + cmd_build_version_sz
            + cmd_dylinker_sz
            + cmd_dylibs_sz
            + cmd_id_dylib_sz
            + cmd_main_sz
            + cmd_symtab_sz
            + cmd_dysymtab_sz
            + cmd_dyld_info_sz) as u32;

        let headers_size = 32 + (sizeofcmds as usize);
        let text_file_offset_start = 0u64; // __TEXT starts at file offset 0 including headers

        output.resize(headers_size, 0);

        // 3. Layout __TEXT segment payload
        let mut current_file_offset = headers_size as u64;
        let mut current_vmaddr = text_vmaddr + current_file_offset;

        let mut text_sec_records = Vec::new();
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

        let mut text_va_end = current_vmaddr;
        let mut data_file_end = current_file_offset;
        let mut data_va_end = current_vmaddr;

        // Authoritative placement when virtual addresses are assigned
        let has_layout_vas = merged_sections.iter().any(|s| s.virtual_address != 0);
        if has_layout_vas {
            output.truncate(headers_size);

            let mut text_file_end = headers_size as u64;
            text_va_end = text_vmaddr + headers_size as u64;
            let mut data_off_min: Option<u64> = None;
            let mut data_va_min: Option<u64> = None;
            data_file_end = 0u64;
            data_va_end = 0u64;
            text_sec_records.clear();
            data_sec_records.clear();

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
                            format!("section `{}` offset 0x{:x} exceeds address space", sec.name, off),
                        )
                    })?;
                    let len = usize::try_from(file_len).map_err(|_| {
                        LinkError::new(
                            ErrorCode::InvalidSection,
                            format!("section `{}` length 0x{:x} exceeds address space", sec.name, file_len),
                        )
                    })?;
                    let end = start.checked_add(len).ok_or_else(|| {
                        LinkError::new(
                            ErrorCode::InvalidSection,
                            format!("section `{}` offset overflow (0x{:x} + 0x{:x})", sec.name, start, len),
                        )
                    })?;
                    if output.len() < end {
                        output.resize(end, 0);
                    }
                    output[start..end].copy_from_slice(&sec.data);
                }

                let is_text =
                    sec.is_executable() || (!sec.is_writable() && sec.kind != SectionKind::Bss);
                if is_text {
                    text_sec_records.push((
                        sec.name.clone(),
                        sec.virtual_address,
                        mem_len,
                        off as u32,
                        sec.alignment,
                    ));
                    text_file_end = text_file_end.max(off + file_len);
                    text_va_end = text_va_end.max(sec.virtual_address + mem_len);
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
                    data_off_min = Some(data_off_min.map_or(off, |m| m.min(off)));
                    data_va_min = Some(
                        data_va_min.map_or(sec.virtual_address, |m| m.min(sec.virtual_address)),
                    );
                    data_file_end = data_file_end.max(off + file_len);
                    data_va_end = data_va_end.max(sec.virtual_address + mem_len);
                }
            }

            let (data_off_start, data_va_start) = match (data_off_min, data_va_min) {
                (Some(off), Some(va)) => (off, va),
                _ => (
                    align_to(text_file_end, target.page_size),
                    align_to(text_va_end, target.page_size),
                ),
            };

            text_vmsize = align_to(text_va_end - text_vmaddr, target.page_size);
            text_filesize = text_vmsize.min(data_off_start);
            data_file_offset_start = data_off_start;
            data_vmaddr_start = data_va_start;
            data_filesize = data_file_end.saturating_sub(data_off_start);
            data_vmsize = align_to(data_va_end.saturating_sub(data_va_start), target.page_size);
        }

        // ── Dynamic-linking synthesis (dyld stubs, GOT, lazy-bind info) ──
        //
        // Everything below extends the already-placed segments: the stub
        // helper goes at the end of __TEXT, the `__got` slot at the end of
        // __DATA, and each `__la_symbol_ptr` slot (created by the layout
        // stage) is patched to point at its helper entry.
        let mut dyld_rebase_off = 0u32;
        let mut dyld_rebase_size = 0u32;
        let mut dyld_bind_off = 0u32;
        let mut dyld_bind_size = 0u32;
        let mut dyld_lazy_off = 0u32;
        let mut dyld_lazy_size = 0u32;
        let mut dyld_export_off = 0u32;
        let mut dyld_export_size = 0u32;
        let mut rebase_bytes: Vec<u8> = Vec::new();
        let mut bind_bytes: Vec<u8> = Vec::new();
        let mut export_bytes: Vec<u8> = Vec::new();
        let mut rebase_targets: Vec<RebaseTarget> = Vec::new();
        let mut indirect_symbols: Vec<u32> = Vec::new();

        // Segment indexes dyld uses in rebase/bind streams: LC_SEGMENT_64
        // commands in order. Executables put __PAGEZERO first.
        let text_seg_index: u8 = if is_shared { 0 } else { 1 };
        let data_seg_index: u8 = text_seg_index + 1;

        // (base VA of the file offset mapping — records use off = va - base)
        let layout_base_va = text_vmaddr;

        let la_section: Option<&MergedSection> = if has_dynamic {
            if !has_layout_vas {
                return Err(LinkError::new(
                    ErrorCode::InvalidSection,
                    "dynamic Mach-O output requires layout-assigned section \
                     addresses (the linker must run the layout stage)",
                ));
            }
            match merged_sections
                .iter()
                .find(|s| s.name == LA_SYMBOL_PTR_SECTION)
            {
                Some(section) => Some(section),
                None if imports.is_empty() => None,
                None => {
                    return Err(LinkError::new(
                        ErrorCode::InvalidSection,
                        format!(
                            "dynamic Mach-O link has {} imports but the layout stage did not \
                             create the `{}` section",
                            imports.len(),
                            LA_SYMBOL_PTR_SECTION
                        ),
                    ));
                }
            }
        } else {
            None
        };

        // Reserve the lazy-bind stream first: the helper entries embed each
        // import's byte offset into this stream so `dyld_stub_binder` can
        // resume parsing exactly there.
        let (lazy_bind_bytes, lazy_entry_offsets) = if has_dynamic {
            let targets: Vec<LazyBindTarget> = imports
                .iter()
                .map(|imp| {
                    let slot_in_seg = imp.la_slot_va - data_vmaddr_start;
                    LazyBindTarget {
                        segment_index: data_seg_index,
                        offset: slot_in_seg,
                        ordinal: LIBSYSTEM_ORDINAL,
                        symbol: imp.bind_name.clone(),
                    }
                })
                .collect();
            dynamic::encode_lazy_bind_opcodes(&targets)
        } else {
            (Vec::new(), Vec::new())
        };

        if has_stub_sections {
            let (la_va, la_len) = match la_section {
                Some(sec) => (sec.virtual_address, sec.data.len() as u64),
                None => (0, 0),
            };
            if !imports.is_empty() {
                let expected_len = (imports.len() * 8) as u64;
                if la_len != expected_len {
                    return Err(LinkError::new(
                        ErrorCode::InvalidSection,
                        format!(
                            "`{}` section is {} bytes but {} imports need {} bytes",
                            LA_SYMBOL_PTR_SECTION,
                            la_len,
                            imports.len(),
                            expected_len
                        ),
                    ));
                }
                for (i, imp) in imports.iter().enumerate() {
                    if imp.la_slot_va != la_va + (i as u64) * 8 {
                        return Err(LinkError::new(
                            ErrorCode::InvalidSection,
                            format!(
                                "import `{}` lazy slot 0x{:x} does not match the `{}` \
                                 layout (0x{:x} + {}*8)",
                                imp.raw_name, imp.la_slot_va, LA_SYMBOL_PTR_SECTION, la_va, i
                            ),
                        ));
                    }
                }
            }

            // __stub_helper placement: right after the last __TEXT section.
            let entry_size = if target.arch == Arch::AArch64 {
                12u64
            } else {
                10u64
            };
            let stub_helper_va = align_to(text_va_end, 16);
            let tail_off = (imports.len() as u64) * entry_size;
            let stub_helper_file_off = stub_helper_va - layout_base_va;

            // __got placement: right after the last __DATA section.
            let got_va = align_to(data_va_end, 8);
            // File offset: the GOT must be in the file, so base it on actual
            // file-backed data end rather than data_va_end. Using data_va_end
            // would materialize BSS bytes into the file when BSS lies between
            // the last file-backed section and data_va_end in VA space.
            let got_file_off = align_to(data_file_end, 8);

            // Build the helper machine code now that every address is known.
            let mut stub_helper_bytes =
                Vec::with_capacity((imports.len() * (entry_size as usize) + 20) as usize);
            let tail_va = stub_helper_va + tail_off;
            if target.arch == Arch::AArch64 {
                for (i, _imp) in imports.iter().enumerate() {
                    let lazy_offset = lazy_entry_offsets[i];
                    let entry_va = stub_helper_va + (i as u64) * 12;
                    let branch_offset_words = ((tail_va as i64 - (entry_va as i64 + 4)) / 4) as i32;
                    let b_instr = 0x14000000u32 | ((branch_offset_words as u32) & 0x03FFFFFF);
                    // ldr w16, [pc, #8]
                    stub_helper_bytes.extend_from_slice(&0x58000040u32.to_le_bytes());
                    // b tail
                    stub_helper_bytes.extend_from_slice(&b_instr.to_le_bytes());
                    // .long lazy_offset
                    stub_helper_bytes.extend_from_slice(&(lazy_offset as u32).to_le_bytes());
                }
                let pc = tail_va;
                // In AArch64 hardware, ADRP always computes the address of a 4KB (12-bit) page.
                let page_mask = !0xFFFu64;

                let helper_page = stub_helper_va & page_mask;
                let pc_page = pc & page_mask;
                let page_delta = ((helper_page as i64) - (pc_page as i64)) >> 12;
                let immhi = ((page_delta >> 2) & 0x7FFFF) as u32;
                let immlo = (page_delta & 0x3) as u32;
                let adrp_x17 = 0x90000011u32 | (immlo << 29) | (immhi << 5);

                let helper_off = (stub_helper_va & 0xFFF) as u32;
                let add_x17 = 0x91000231u32 | (helper_off << 10);

                let pc_got = tail_va + 8;
                let pc_got_page = pc_got & page_mask;
                let got_page = got_va & page_mask;
                let got_page_delta = ((got_page as i64) - (pc_got_page as i64)) >> 12;
                let got_immhi = ((got_page_delta >> 2) & 0x7FFFF) as u32;
                let got_immlo = (got_page_delta & 0x3) as u32;
                let adrp_x16 = 0x90000010u32 | (got_immlo << 29) | (got_immhi << 5);

                let got_off = (got_va & 0xFFF) as u32;
                let pimm = (got_off >> 3) & 0xFFF;
                // ldr x16, [x16, #pimm<<3] (Rt = x16 = 0x10)
                let ldr_x16 = 0xF9400210u32 | (pimm << 10);

                let br_x16 = 0xD61F0200u32;

                stub_helper_bytes.extend_from_slice(&adrp_x17.to_le_bytes());
                stub_helper_bytes.extend_from_slice(&add_x17.to_le_bytes());
                stub_helper_bytes.extend_from_slice(&adrp_x16.to_le_bytes());
                stub_helper_bytes.extend_from_slice(&ldr_x16.to_le_bytes());
                stub_helper_bytes.extend_from_slice(&br_x16.to_le_bytes());
            } else {
                for (i, _imp) in imports.iter().enumerate() {
                    let lazy_offset = lazy_entry_offsets[i];
                    // push $lazy_offset  (68 imm32)
                    stub_helper_bytes.extend_from_slice(&[0x68]);
                    stub_helper_bytes.extend_from_slice(&(lazy_offset as u32).to_le_bytes());
                    // jmp tail  (E9 rel32)
                    let entry_va = stub_helper_va + (i as u64) * 10;
                    let rel = (tail_va as i64 - (entry_va as i64 + 10)) as i32;
                    stub_helper_bytes.extend_from_slice(&[0xE9]);
                    stub_helper_bytes.extend_from_slice(&rel.to_le_bytes());
                }
                // tail: push $helper_start  (68 imm32) — rebased as
                // REBASE_TYPE_TEXT_ABSOLUTE32 when the image slides.
                stub_helper_bytes.extend_from_slice(&[0x68]);
                stub_helper_bytes.extend_from_slice(&(stub_helper_va as u32).to_le_bytes());
                // jmp *dyld_stub_binder@GOT(%rip)  (FF 25 rel32)
                let jmp_place_va = tail_va + 5;
                let rel = (got_va as i64 - (jmp_place_va as i64 + 6)) as i32;
                stub_helper_bytes.extend_from_slice(&[0xFF, 0x25]);
                stub_helper_bytes.extend_from_slice(&rel.to_le_bytes());
            }

            // Write the helper into the file image and grow __TEXT.
            if (stub_helper_file_off as usize) > output.len() {
                output.resize(stub_helper_file_off as usize, 0);
            }
            let end = stub_helper_file_off as usize + stub_helper_bytes.len();
            if output.len() < end {
                output.resize(end, 0);
            }
            output[stub_helper_file_off as usize..end].copy_from_slice(&stub_helper_bytes);
            text_sec_records.push((
                STUB_HELPER_SECTION.to_string(),
                stub_helper_va,
                stub_helper_bytes.len() as u64,
                stub_helper_file_off as u32,
                16,
            ));
            text_va_end = text_va_end.max(stub_helper_va + stub_helper_bytes.len() as u64);
            text_vmsize = align_to(text_va_end - text_vmaddr, target.page_size);
            text_filesize = text_vmsize.min(data_file_offset_start);

            // Write the __got slot (zero: dyld fills it from the non-lazy
            // bind entry before any code runs).
            if (got_file_off as usize) > output.len() {
                output.resize(got_file_off as usize, 0);
            }
            if output.len() < got_file_off as usize + 8 {
                output.resize(got_file_off as usize + 8, 0);
            }
            data_sec_records.push((GOT_SECTION.to_string(), got_va, 8, got_file_off as u32, 8));
            data_file_end = data_file_end.max(got_file_off + 8);
            data_va_end = data_va_end.max(got_va + 8);
            data_filesize = data_file_end.saturating_sub(data_file_offset_start);
            data_vmsize =
                align_to(data_va_end - data_vmaddr_start, target.page_size).max(data_vmsize);

            // Patch every lazy slot to its helper entry (the pre-binding
            // target). These are absolute pointers into __TEXT and must be
            // rebased for PIE.
            if let Some(section) = la_section {
                let la_file_off = section.virtual_address - layout_base_va;
                for (i, _imp) in imports.iter().enumerate() {
                    let entry_va = stub_helper_va + (i as u64) * entry_size;
                    let slot_off = la_file_off + (i as u64) * 8;
                    output[slot_off as usize..slot_off as usize + 8]
                        .copy_from_slice(&entry_va.to_le_bytes());
                }
            }

            // Rebase stream, part 1: the helper's `push $helper_start`
            // immediate (x86_64) and every patched lazy slot.
            if target.arch == Arch::X86_64 {
                let tail_push_imm_va = tail_va + 1;
                rebase_targets.push(RebaseTarget {
                    segment_index: text_seg_index,
                    offset: tail_push_imm_va - text_vmaddr,
                    kind: RebaseKind::TextAbsolute32,
                });
            }
            for (i, _imp) in imports.iter().enumerate() {
                let slot_va = la_va + (i as u64) * 8;
                rebase_targets.push(RebaseTarget {
                    segment_index: data_seg_index,
                    offset: slot_va - data_vmaddr_start,
                    kind: RebaseKind::Pointer,
                });
            }

            // Non-lazy binds: dyld_stub_binder's GOT slot (data imports are
            // rejected by the layout stage; this slot is the writer's own).
            bind_bytes = dynamic::encode_bind_opcodes(&[BindTarget {
                segment_index: data_seg_index,
                offset: got_va - data_vmaddr_start,
                bind_type: BIND_TYPE_POINTER,
                ordinal: LIBSYSTEM_ORDINAL,
                symbol: DYLD_STUB_BINDER.to_string(),
                addend: 0,
            }]);
        }

        // Rebase stream, part 2 (with or without imports): every link-time
        // absolute pointer the layout stage recorded must be rebased when
        // the PIE image slides.
        if has_dynamic {
            if let Some(info) = dynamic {
                for &va in &info.rebases {
                    let (seg_index, seg_base) =
                        if va >= text_vmaddr && va < text_vmaddr + text_vmsize {
                            (text_seg_index, text_vmaddr)
                        } else if va >= data_vmaddr_start && va < data_vmaddr_start + data_vmsize {
                            (data_seg_index, data_vmaddr_start)
                        } else {
                            return Err(LinkError::new(
                                ErrorCode::InvalidSection,
                                format!(
                                    "cannot rebase address 0x{:x}: it lies outside every \
                                     mapped segment (a link-time absolute pointer must live in \
                                     __TEXT or __DATA)",
                                    va
                                ),
                            ));
                        };
                    rebase_targets.push(RebaseTarget {
                        segment_index: seg_index,
                        offset: va - seg_base,
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
        let is_text_flags: Vec<bool> = merged_sections
            .iter()
            .map(|sec| sec.is_executable() || (!sec.is_writable() && sec.kind != SectionKind::Bss))
            .collect();
        let text_count = is_text_flags.iter().filter(|t| **t).count();
        let mut n_sect_for: Vec<u8> = Vec::with_capacity(merged_sections.len());
        let mut text_seen = 0usize;
        let mut data_seen = 0usize;
        for is_text in is_text_flags {
            if is_text {
                text_seen += 1;
                n_sect_for.push(text_seen as u8);
            } else {
                data_seen += 1;
                n_sect_for.push((text_count + data_seen) as u8);
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

        // Synthetic undefined entries: the lazy-binding GOT anchor and one
        // per import. Two-level namespace: the library ordinal lives in the
        // high byte of n_desc (libSystem is ordinal 1, the first LOAD_DYLIB).
        let mut synthetic_undefs: Vec<(String, u16)> = Vec::new();
        if has_stub_sections {
            synthetic_undefs.push((DYLD_STUB_BINDER.to_string(), 0x0100));
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

        // Indirect symbol table: one entry per `__got` slot followed by one
        // per `__la_symbol_ptr` slot, in section order, each the symtab
        // index of the symbol the slot binds.
        if has_stub_sections {
            let binder_index = iundefsym + (undef_syms.len() as u32) + 0;
            let first_import_index = binder_index + 1;
            indirect_symbols.push(binder_index);
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
            (dyld_rebase_off, dyld_rebase_size) = push_blob(&rebase_bytes);
            (dyld_bind_off, dyld_bind_size) = push_blob(&bind_bytes);
            (dyld_lazy_off, dyld_lazy_size) = push_blob(&lazy_bind_bytes);
            (dyld_export_off, dyld_export_size) = push_blob(&export_bytes);
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
            let is_code =
                name.contains("text") || name == STUB_HELPER_SECTION || name.contains("code");
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
            let mut sname = [0u8; 16];
            let name_bytes = name.as_bytes();
            sname[0..name_bytes.len().min(16)]
                .copy_from_slice(&name_bytes[0..name_bytes.len().min(16)]);
            cmd_buf.extend_from_slice(&sname);
            cmd_buf.extend_from_slice(&data_name);
            cmd_buf.extend_from_slice(&va.to_le_bytes());
            cmd_buf.extend_from_slice(&sz.to_le_bytes());
            cmd_buf.extend_from_slice(&off.to_le_bytes());
            let align_pow2 = if *align > 1 {
                (*align as f64).log2().round() as u32
            } else {
                0
            };
            cmd_buf.extend_from_slice(&align_pow2.to_le_bytes());
            // Section flags carry the dyld section TYPE (low byte) plus the
            // indirect-symbol-table start (reserved1) for pointer sections:
            // dyld uses both to interpret the slots it binds.
            let is_bss = merged_sections
                .iter()
                .any(|s| s.name == *name && s.kind == SectionKind::Bss);
            let (section_flags, reserved1) = if *name == GOT_SECTION {
                (S_NON_LAZY_SYMBOL_POINTERS, 0u32)
            } else if *name == LA_SYMBOL_PTR_SECTION {
                (S_LAZY_SYMBOL_POINTERS, 1u32)
            } else if is_bss {
                (S_ZEROFILL, 0u32)
            } else {
                (S_REGULAR, 0u32)
            };
            cmd_buf.extend_from_slice(&0u32.to_le_bytes()); // reloff
            cmd_buf.extend_from_slice(&0u32.to_le_bytes()); // nreloc
            cmd_buf.extend_from_slice(&section_flags.to_le_bytes());
            cmd_buf.extend_from_slice(&reserved1.to_le_bytes());
            cmd_buf.extend_from_slice(&0u32.to_le_bytes()); // reserved2
            cmd_buf.extend_from_slice(&0u32.to_le_bytes()); // reserved3
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

        output[32..32 + cmd_buf.len()].copy_from_slice(&cmd_buf);

        Ok(output)
    }
}
