//! Section Layout, Virtual Address assignment, W^X permission checking, and Relocation application.

use crate::arch::get_handler;
use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::object::ObjectFile;
use crate::relocation::RelocationKind;
use crate::resolver::SymbolResolver;
use crate::section::{MergedSection, SectionKind, align_to, flags};
use crate::symbol::Symbol;
use crate::target::Target;
use std::collections::{HashMap, HashSet};

/// PE TLS directory information consumed by the PE writer.
#[derive(Debug, Clone, Copy)]
pub struct PeTlsInfo {
    pub directory_rva: u32,
    pub directory_size: u32,
}

/// Section layout and memory placement engine.
pub struct LayoutEngine {
    pub merged_sections: Vec<MergedSection>,
    pub resolved_symbols: Vec<Symbol>,
    pub entry_va: u64,
    /// The PE import table built during layout (single source of truth for the
    /// `.idata` contents and every `__imp_*` / import-thunk address).
    /// The PE writer must emit these bytes verbatim; rebuilding the table with
    /// a differently-ordered import list invalidates all patched thunks.
    pub pe_import_info: Option<crate::pe::import::ImportTableResult>,
    /// PE TLS directory information, when input objects contain TLS data.
    pub pe_tls_info: Option<PeTlsInfo>,
    /// RVAs of all 64-bit absolute relocations applied (for the PE `.reloc`
    /// base relocation table, enabling working ASLR).
    pub base_relocs: Vec<u32>,
    /// RVAs of all 32-bit absolute relocations applied. Emitted as
    /// IMAGE_REL_BASED_HIGHLOW entries for 32-bit x86 PE targets.
    pub base_relocs32: Vec<u32>,
    /// ELF dynamic-linking results consumed by the writer: virtual address of
    /// each imported symbol's GOT slot.
    pub elf_got_slots: std::collections::HashMap<String, u64>,
    /// ELF dynamic relocations (`R_X86_64_JUMP_SLOT` / `R_X86_64_RELATIVE`)
    /// recorded while applying static relocations.
    pub elf_dynamic_relocs: Vec<crate::elf::dynamic::ElfDynamicReloc>,
    /// Mach-O dynamic-linking results consumed by the writer: one entry per
    /// imported function (index i owns `__got` slot i).
    pub macho_imports: Vec<crate::macho::dynamic::MachOImport>,
    /// Pointer words the loader must rebase when the (always-PIE) image
    /// slides, each carrying the input that produced it.
    pub macho_rebases: Vec<crate::macho::dynamic::MachORebaseTarget>,
    /// Pointer-sized slots the Mach-O writer must add to the dyld bind stream
    /// (TLV descriptor thunks bound to dyld's `__tlv_bootstrap`).
    pub macho_binds: Vec<crate::macho::dynamic::MachODataBind>,
    /// Non-fatal issues encountered while relocating (e.g. weak/internal
    /// symbols that could not be resolved and were routed to a trap stub).
    pub warnings: Vec<String>,
}

impl LayoutEngine {
    pub fn new() -> Self {
        Self {
            merged_sections: Vec::new(),
            resolved_symbols: Vec::new(),
            entry_va: 0,
            pe_import_info: None,
            pe_tls_info: None,
            base_relocs: Vec::new(),
            base_relocs32: Vec::new(),
            elf_got_slots: std::collections::HashMap::new(),
            elf_dynamic_relocs: Vec::new(),
            macho_imports: Vec::new(),
            macho_rebases: Vec::new(),
            macho_binds: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Merge sections, assign virtual addresses, resolve symbols, and apply all relocations.
    ///
    /// `elf_dynamic` requests ELF dynamic-linking metadata: the layout
    /// reserves GOT slots for imported symbols and TLS GOT slots.
    /// `elf_position_independent` (PIE executables and shared libraries)
    /// additionally records `R_X86_64_RELATIVE` relocations for absolute
    /// pointers, which the loader rebases at its runtime-chosen base; a
    /// fixed-base `ET_EXEC` never needs them and must not carry any (the
    /// loader would write them into read-only load segments).
    pub fn layout_and_relocate(
        &mut self,
        objects: &[ObjectFile],
        resolver: &SymbolResolver,
        target: &Target,
        entry_name: &str,
        elf_dynamic: bool,
        elf_position_independent: bool,
    ) -> LinkResult<()> {
        // 1. Group input sections by category
        let mut text_merged = MergedSection::new(
            ".text",
            SectionKind::Text,
            flags::READ | flags::EXEC | flags::ALLOC,
            16,
        );
        let mut rodata_merged = MergedSection::new(
            ".rodata",
            SectionKind::Rodata,
            flags::READ | flags::ALLOC,
            8,
        );
        let mut data_merged = MergedSection::new(
            ".data",
            SectionKind::Data,
            flags::READ | flags::WRITE | flags::ALLOC,
            8,
        );
        let mut bss_merged = MergedSection::new(
            ".bss",
            SectionKind::Bss,
            flags::READ | flags::WRITE | flags::ALLOC,
            8,
        );
        let mut tls_merged = MergedSection::new(
            ".tls",
            SectionKind::TData,
            flags::READ | flags::WRITE | flags::ALLOC | flags::TLS,
            16,
        );
        let mut meta_merged = MergedSection::new(
            ".adesh.meta",
            SectionKind::AdeshMeta,
            flags::READ | flags::ALLOC,
            4,
        );
        // Mach-O `__DATA,__thread_vars`: the table of 24-byte TLV descriptors
        // the static linker rewrites. Kept separate from the TLS block because
        // its bytes are link-time descriptors, not per-thread storage.
        let mut tlv_merged = MergedSection::new(
            "__thread_vars",
            SectionKind::Data,
            flags::READ | flags::WRITE | flags::ALLOC | flags::TLV_TABLE,
            8,
        );

        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        enum SectionCat {
            Text,
            Rodata,
            Data,
            Bss,
            Tls,
            TlvVars,
            Meta,
            EhFrame,
            EhFrameHdr,
            Debug(String),
        }

        /// Collect the functions an `.eh_frame` can honestly describe: every
        /// defined function symbol placed in a text section whose size is
        /// known. Returns `(offset_within_merged_text, size)` in
        /// deterministic object/symbol order.
        struct AdeshFrameEntry {
            file_index: usize,
            text_offset: u64,
            size: u64,
            info: Option<crate::unwind::AdeshFdeInfo>,
            is_terminal_entry: bool,
        }

        struct KeptInputCie {
            orig_offset: usize,
            bytes: Vec<u8>,
            relocs: Vec<crate::relocation::Relocation>,
            pc_begin_encoding: u8,
        }

        struct KeptInputFde {
            orig_offset: usize,
            orig_cie_offset: usize,
            bytes: Vec<u8>,
            relocs: Vec<crate::relocation::Relocation>,
            pc_begin_reloc_idx: usize,
        }

        struct KeptInputEhSection {
            file_index: usize,
            sec_index: usize,
            cies: Vec<KeptInputCie>,
            fdes: Vec<KeptInputFde>,
        }

        fn collect_adesh_frame_entries(
            objects: &[ObjectFile],
            sec_placement: &HashMap<(usize, usize), (SectionCat, u64)>,
            files_with_input_eh: &HashSet<usize>,
            fde_info_map: &HashMap<(usize, String), crate::unwind::AdeshFdeInfo>,
        ) -> Vec<AdeshFrameEntry> {
            let mut out = Vec::new();
            for (f_idx, obj) in objects.iter().enumerate() {
                if files_with_input_eh.contains(&f_idx) {
                    continue;
                }
                let is_linker_synth =
                    obj.path == std::path::Path::new("__runtime_intrinsics_and_stubs.o");
                for sym in &obj.symbols {
                    if !sym.is_defined
                        || sym.sym_type != crate::symbol::SymbolType::Function
                        || sym.size == 0
                    {
                        continue;
                    }
                    if is_linker_synth && sym.name != "_start" {
                        // Do not fabricate `push rbp` FDEs for PLT `jmp` stubs
                        // or frameless runtime intrinsic thunks.
                        continue;
                    }
                    let Some(s_idx) = sym.section_index else {
                        continue;
                    };
                    let Some((cat, sec_off)) = sec_placement.get(&(f_idx, s_idx)) else {
                        continue;
                    };
                    if *cat != SectionCat::Text {
                        continue;
                    }
                    let info = fde_info_map.get(&(f_idx, sym.name.clone())).cloned();
                    let is_terminal_entry = is_linker_synth && sym.name == "_start" && info.is_none();
                    out.push(AdeshFrameEntry {
                        file_index: f_idx,
                        text_offset: sec_off + sym.value,
                        size: sym.size,
                        info,
                        is_terminal_entry,
                    });
                }
            }
            out
        }

        fn collect_kept_input_eh(
            objects: &[ObjectFile],
            sec_placement: &HashMap<(usize, usize), (SectionCat, u64)>,
            input_eh_frame_sections: &[(usize, usize)],
        ) -> Vec<KeptInputEhSection> {
            let mut kept_sections = Vec::new();
            for &(f_idx, s_idx) in input_eh_frame_sections {
                let obj = &objects[f_idx];
                let sec = &obj.sections[s_idx];
                let Some(records) = crate::unwind::parse_input_eh_frame_records(&sec.data) else {
                    continue;
                };
                let mut cie_map: HashMap<usize, KeptInputCie> = HashMap::new();
                let mut fdes = Vec::new();

                for rec in records {
                    match rec {
                        crate::unwind::InputEhRecord::Cie {
                            orig_offset,
                            bytes,
                            pc_begin_encoding,
                        } => {
                            let end = orig_offset + bytes.len();
                            let relocs = sec
                                .relocations
                                .iter()
                                .filter(|r| {
                                    (r.offset as usize) >= orig_offset && (r.offset as usize) < end
                                })
                                .cloned()
                                .collect();
                            cie_map.insert(
                                orig_offset,
                                KeptInputCie {
                                    orig_offset,
                                    bytes,
                                    relocs,
                                    pc_begin_encoding,
                                },
                            );
                        }
                        crate::unwind::InputEhRecord::Fde {
                            orig_offset,
                            orig_cie_offset,
                            bytes,
                        } => {
                            let end = orig_offset + bytes.len();
                            let relocs: Vec<_> = sec
                                .relocations
                                .iter()
                                .filter(|r| {
                                    (r.offset as usize) >= orig_offset && (r.offset as usize) < end
                                })
                                .cloned()
                                .collect();
                            let pc_reloc_pos = relocs
                                .iter()
                                .position(|r| r.offset as usize == orig_offset + 8);
                            let Some(pc_begin_reloc_idx) = pc_reloc_pos else {
                                continue;
                            };
                            let pc_reloc = &relocs[pc_begin_reloc_idx];
                            // Only retain FDEs whose target text section is live & non-folded.
                            let target_live = if let Some(si) = pc_reloc.symbol_index
                                && let Some(sym) = obj.symbols.get(si)
                                && sym.is_defined
                                && let Some(target_sec_idx) = sym.section_index
                            {
                                let target_sec = &obj.sections[target_sec_idx];
                                target_sec.is_live
                                    && !target_sec.is_folded
                                    && matches!(
                                        sec_placement.get(&(f_idx, target_sec_idx)),
                                        Some((SectionCat::Text, _))
                                    )
                            } else {
                                false
                            };
                            if !target_live || !cie_map.contains_key(&orig_cie_offset) {
                                continue;
                            }
                            fdes.push(KeptInputFde {
                                orig_offset,
                                orig_cie_offset,
                                bytes,
                                relocs,
                                pc_begin_reloc_idx,
                            });
                        }
                    }
                }

                if fdes.is_empty() {
                    continue;
                }
                let used_cie_offsets: HashSet<usize> =
                    fdes.iter().map(|f| f.orig_cie_offset).collect();
                let mut cies: Vec<KeptInputCie> = cie_map
                    .into_values()
                    .filter(|c| used_cie_offsets.contains(&c.orig_offset))
                    .collect();
                cies.sort_by_key(|c| c.orig_offset);
                kept_sections.push(KeptInputEhSection {
                    file_index: f_idx,
                    sec_index: s_idx,
                    cies,
                    fdes,
                });
            }
            kept_sections
        }

        let mut debug_merged_map: HashMap<String, MergedSection> = HashMap::new();

        // Map: (file_index, section_index) -> (SectionCat, offset_in_merged)
        let mut sec_placement: HashMap<(usize, usize), (SectionCat, u64)> = HashMap::new();
        let mut tls_sections: Vec<(usize, usize)> = Vec::new();
        let mut tls_callback_sections: Vec<(usize, usize)> = Vec::new();
        let mut input_eh_frame_sections: Vec<(usize, usize)> = Vec::new();
        let mut input_pdata_sections: Vec<(usize, usize)> = Vec::new();
        let mut adesh_fde_info_map: HashMap<(usize, String), crate::unwind::AdeshFdeInfo> =
            HashMap::new();

        // Map: symbol name -> (file_index, section_index) of its definition.
        // Used to alias ICF-folded sections to their canonical twin's placement.
        let mut sym_def_loc: HashMap<&str, (usize, usize)> = HashMap::new();
        for (f_idx, obj) in objects.iter().enumerate() {
            for sym in &obj.symbols {
                if sym.is_defined {
                    if let Some(s_idx) = sym.section_index {
                        sym_def_loc
                            .entry(sym.name.as_str())
                            .or_insert((f_idx, s_idx));
                    }
                }
            }
        }

        // COMDAT groups already merged. Sections of a COMDAT group are
        // discarded together, so the first object that contributes a group is
        // linked and later copies of the same group are dropped together with
        // their relocations — the resolver already folded their symbol
        // definitions onto the first copy, so every reference to the group
        // leader resolves there.
        let mut merged_comdat_groups: HashSet<&str> = HashSet::new();

        for (f_idx, obj) in objects.iter().enumerate() {
            for (s_idx, sec) in obj.sections.iter().enumerate() {
                if !sec.is_live {
                    continue;
                }

                // COMDAT dedup. Checked before the ICF aliasing below so that a
                // folded section also claims its group (a later copy would
                // otherwise reintroduce the discarded bytes). A garbage
                // collected first copy never claims anything, leaving the
                // group to the next live one.
                if let Some(group) = sec.comdat_group.as_deref()
                    && !merged_comdat_groups.insert(group)
                {
                    continue;
                }

                // ICF-folded section: alias to the canonical section's placement
                // so every symbol defined here resolves to the canonical copy's
                // address (the bytes and relocations are verified identical).
                if sec.is_folded {
                    if let Some(ref canon_sym) = sec.folded_into {
                        if let Some(&(cf, cs)) = sym_def_loc.get(canon_sym.as_str()) {
                            if let Some(placement) = sec_placement.get(&(cf, cs)) {
                                sec_placement.insert((f_idx, s_idx), placement.clone());
                            }
                        }
                    }
                    continue;
                }

                if sec.flags & crate::section::flags::TLV_TABLE != 0 {
                    // Mach-O `S_THREAD_LOCAL_VARIABLES`: a table of linker-
                    // rewritten descriptors. It is read/write *image* data, not
                    // per-thread storage, so it must stay out of the TLS block.
                    let off = tlv_merged.append_section(sec, f_idx, s_idx);
                    sec_placement.insert((f_idx, s_idx), (SectionCat::TlvVars, off));
                } else if matches!(sec.kind, SectionKind::TData | SectionKind::TBss)
                    || (sec.flags & crate::section::flags::TLS != 0)
                    || sec.name.starts_with(".tls")
                {
                    // COFF TLS contribution order is lexical by full section
                    // name (`.tls$AAA` ... `.tls$ZZZ`), not archive order.
                    // Preserve each contribution so the loader sees one
                    // contiguous, correctly ordered TLS template.
                    tls_sections.push((f_idx, s_idx));
                } else if target.format == crate::target::ObjectFormat::Pe
                    && sec.name.starts_with(".CRT$XL")
                {
                    // PE TLS callbacks are an ordered null-terminated pointer
                    // array. Keep them out of ordinary rodata so lexical COFF
                    // subsection ordering is preserved.
                    tls_callback_sections.push((f_idx, s_idx));
                } else if sec.name == ".adesh.fde_info" {
                    if let Some(entries) = crate::unwind::parse_adesh_fde_info(&sec.data) {
                        for entry in entries {
                            adesh_fde_info_map.insert((f_idx, entry.func_name.clone()), entry);
                        }
                    }
                    continue;
                } else if target.format == crate::target::ObjectFormat::Elf
                    && target.arch == crate::target::Arch::X86_64
                    && sec.name == ".eh_frame"
                {
                    input_eh_frame_sections.push((f_idx, s_idx));
                    continue;
                } else if target.format == crate::target::ObjectFormat::Pe
                    && target.arch == crate::target::Arch::X86_64
                    && (sec.name == ".pdata" || sec.name.starts_with(".pdata$"))
                {
                    input_pdata_sections.push((f_idx, s_idx));
                    continue;
                } else if sec.name == ".adesh.meta" || sec.kind == SectionKind::AdeshMeta {
                    let off = meta_merged.append_section(sec, f_idx, s_idx);
                    sec_placement.insert((f_idx, s_idx), (SectionCat::Meta, off));
                } else if sec.kind == SectionKind::Debug || sec.name.starts_with(".debug") {
                    let debug_sec = debug_merged_map.entry(sec.name.clone()).or_insert_with(|| {
                        MergedSection::new(
                            sec.name.clone(),
                            SectionKind::Debug,
                            sec.flags,
                            sec.alignment.max(1),
                        )
                    });
                    let off = debug_sec.append_section(sec, f_idx, s_idx);
                    sec_placement
                        .insert((f_idx, s_idx), (SectionCat::Debug(sec.name.clone()), off));
                } else if sec.is_executable() {
                    let off = text_merged.append_section(sec, f_idx, s_idx);
                    sec_placement.insert((f_idx, s_idx), (SectionCat::Text, off));
                } else if sec.kind == SectionKind::Bss {
                    let off = bss_merged.append_section(sec, f_idx, s_idx);
                    sec_placement.insert((f_idx, s_idx), (SectionCat::Bss, off));
                } else if sec.is_writable() {
                    let off = data_merged.append_section(sec, f_idx, s_idx);
                    sec_placement.insert((f_idx, s_idx), (SectionCat::Data, off));
                } else {
                    let off = rodata_merged.append_section(sec, f_idx, s_idx);
                    sec_placement.insert((f_idx, s_idx), (SectionCat::Rodata, off));
                }
            }
        }

        // TLS contribution order: initialized sections (`.tdata`-like) first
        // in stable lexical order (preserving COFF `.tls$AAA`... ordering
        // among them), then zero-fill (`.tbss`-like) contributions after.
        // The file image must be a PREFIX of the block: PT_TLS `p_filesz`
        // covers exactly the initialized bytes, and the loader zero-fills
        // from there to `p_memsz`. A purely lexical sort would place
        // `.tbss` before `.tdata`, materializing the zero-fill as file
        // bytes ahead of the image (GNU ld concatenates .tdata* then
        // .tbss* for exactly this reason).
        tls_sections.sort_by(|(left_file, left_sec), (right_file, right_sec)| {
            let left = &objects[*left_file].sections[*left_sec];
            let right = &objects[*right_file].sections[*right_sec];
            let left_zero =
                matches!(left.kind, SectionKind::Bss | SectionKind::TBss) || left.name == ".tbss";
            let right_zero =
                matches!(right.kind, SectionKind::Bss | SectionKind::TBss) || right.name == ".tbss";
            left_zero
                .cmp(&right_zero)
                .then_with(|| left.name.cmp(&right.name))
        });
        for (f_idx, s_idx) in tls_sections {
            let sec = &objects[f_idx].sections[s_idx];
            let off = tls_merged.append_section(sec, f_idx, s_idx);
            sec_placement.insert((f_idx, s_idx), (SectionCat::Tls, off));
        }
        // ICF aliases can precede their canonical section in object/section
        // order. The first-pass inline aliasing above only succeeds when the
        // canonical placement already exists; resolve remaining aliases now,
        // iterating to support chains of folded sections.
        loop {
            let mut added_alias = false;
            for (f_idx, obj) in objects.iter().enumerate() {
                for (s_idx, sec) in obj.sections.iter().enumerate() {
                    if !sec.is_live || !sec.is_folded || sec_placement.contains_key(&(f_idx, s_idx))
                    {
                        continue;
                    }
                    let Some(canon_sym) = sec.folded_into.as_deref() else {
                        continue;
                    };
                    let Some(&(canon_file, canon_sec)) = sym_def_loc.get(canon_sym) else {
                        continue;
                    };
                    if let Some(placement) = sec_placement.get(&(canon_file, canon_sec)).cloned() {
                        sec_placement.insert((f_idx, s_idx), placement);
                        added_alias = true;
                    }
                }
            }
            if !added_alias {
                break;
            }
        }
        if target.format != crate::target::ObjectFormat::Pe
            && target.format != crate::target::ObjectFormat::Elf
            && target.format != crate::target::ObjectFormat::MachO
            && tls_merged.size > 0
        {
            // Writers other than PE, ELF and Mach-O do not yet emit their
            // runtime TLS descriptors. Refuse a binary with thread-local data
            // in an ordinary loadable section rather than pretending it has
            // per-thread storage.
            return Err(LinkError::new(
                ErrorCode::UnsupportedRelocation,
                format!(
                    "TLS input sections are not supported by the {:?} output writer",
                    target.format
                ),
            ));
        }

        tls_callback_sections.sort_by(|(left_file, left_sec), (right_file, right_sec)| {
            objects[*left_file].sections[*left_sec]
                .name
                .cmp(&objects[*right_file].sections[*right_sec].name)
        });
        let tls_callbacks_off = if tls_callback_sections.is_empty() {
            None
        } else {
            let mut first_callback_off = None;
            for (f_idx, s_idx) in tls_callback_sections {
                let sec = &objects[f_idx].sections[s_idx];
                let section_off = rodata_merged.append_section(sec, f_idx, s_idx);
                first_callback_off.get_or_insert(section_off);
                sec_placement.insert((f_idx, s_idx), (SectionCat::Rodata, section_off));
            }
            // Required terminator for IMAGE_TLS_DIRECTORY::AddressOfCallbacks.
            rodata_merged.data.extend_from_slice(&0u64.to_le_bytes());
            rodata_merged.size = rodata_merged.data.len() as u64;
            first_callback_off
        };

        let mut pe_imports = Vec::new();
        let mut seen_imports = std::collections::HashSet::new();

        if target.format == crate::target::ObjectFormat::Pe {
            let mut collect_sym = |name: &str| {
                let clean = name
                    .strip_prefix("__imp_")
                    .or_else(|| name.strip_prefix("_imp_"))
                    .unwrap_or(name);
                let clean_unprefixed = clean.trim_start_matches('_');

                let dll_opt = crate::os_router::OsApiRouter::windows_dll_for(clean)
                    .or_else(|| crate::os_router::OsApiRouter::windows_dll_for(clean_unprefixed));

                if let Some(dll) = dll_opt {
                    let export_name =
                        if crate::os_router::OsApiRouter::windows_dll_for(clean).is_some() {
                            clean
                        } else {
                            clean_unprefixed
                        };
                    if !export_name.is_empty()
                        && export_name != "fltused"
                        && seen_imports.insert(export_name.to_string())
                    {
                        pe_imports.push(crate::pe::import::ImportSymbol {
                            dll_name: dll.to_string(),
                            symbol_name: export_name.to_string(),
                            ordinal: None,
                        });
                    }
                }
            };

            // Always ensure ExitProcess is in pe_imports for Windows binaries
            // (referenced by the synthesized CRT startup code).
            collect_sym("ExitProcess");

            // The resolver table is the single source of truth for DLL
            // imports: exactly the symbols that were routed to
            // `SymbolRoute::DllImport` are marked `is_imported` there.
            //
            // Do NOT import symbols merely because they are undefined in an
            // object file: undefined `trunc`/`__chkstk`/`memcpy` etc. are
            // routed to the intrinsics engine and synthesized as native code,
            // and Rust-internal mangled names are dead stubs. Importing them
            // anyway produces bogus msvcrt.dll imports (msvcrt exports neither
            // `trunc` nor `__chkstk`) and the OS loader then refuses to start
            // the image with STATUS_ENTRYPOINT_NOT_FOUND (0xC0000139).
            for (sym_name, resolved) in &resolver.table {
                if resolved.symbol.is_imported {
                    collect_sym(sym_name);
                }
            }

            // Object-level `__imp_*` references (COFF import thunks) that the
            // resolver did not route — e.g. symbols that only ever appeared as
            // `__imp_X` references inside archive members.
            for obj in objects {
                for sym in &obj.symbols {
                    if sym.is_imported
                        || sym.name.starts_with("__imp_")
                        || sym.name.starts_with("_imp_")
                    {
                        let clean = sym
                            .name
                            .strip_prefix("__imp_")
                            .or_else(|| sym.name.strip_prefix("_imp_"))
                            .unwrap_or(&sym.name);
                        let clean_unprefixed = clean.trim_start_matches('_');
                        // Skip anything the resolver deliberately did NOT route
                        // to a DLL (intrinsics, internal stubs, synthesized).
                        let routed_elsewhere = resolver
                            .table
                            .get(clean)
                            .or_else(|| resolver.table.get(clean_unprefixed))
                            .map_or(false, |r| !r.symbol.is_imported);
                        let synth = crate::intrinsics::IntrinsicsEngine::is_intrinsic(clean)
                            || crate::intrinsics::IntrinsicsEngine::is_intrinsic(clean_unprefixed)
                            || crate::os_router::OsApiRouter::is_internal(clean)
                            || crate::os_router::OsApiRouter::is_internal(clean_unprefixed);
                        if !routed_elsewhere && !synth {
                            collect_sym(&sym.name);
                        }
                    }
                }
            }
            // Resolver symbols live in a HashMap. Sort once before creating
            // both import thunks and .idata so identical inputs yield the
            // same executable bytes, independent of hash iteration order.
            pe_imports
                .sort_by(|a, b| (&a.dll_name, &a.symbol_name).cmp(&(&b.dll_name, &b.symbol_name)));
        }

        let mut elf_imports: Vec<String> = Vec::new();
        if target.format == crate::target::ObjectFormat::Elf && elf_dynamic {
            // The resolver records every symbol routed to a system shared
            // library. Deterministic order: the dynsym, GOT, and PLT layouts
            // must be identical for identical inputs.
            elf_imports = resolver.dynamic_imports.clone();
            elf_imports.sort();
            elf_imports.dedup();
        }

        // Mach-O links are always dynamic (macOS has no static libSystem):
        // every symbol the resolver routed to a dylib becomes an eagerly
        // bound import with one `__got` slot. The bind name carries Mach-O's
        // leading-underscore convention.
        let mut macho_imports: Vec<crate::macho::dynamic::MachOImport> = Vec::new();
        if target.format == crate::target::ObjectFormat::MachO {
            let mut raw: Vec<String> = resolver.dynamic_imports.clone();
            raw.sort();
            raw.dedup();
            for raw_name in raw {
                let (bind_name, dylib, is_data) =
                    match crate::os_router::OsApiRouter::classify(&raw_name, target) {
                        crate::os_router::SymbolRoute::DllImport { name, dll, is_data } => {
                            (name, dll.to_string(), is_data)
                        }
                        other => {
                            return Err(LinkError::new(
                                ErrorCode::InvalidSection,
                                format!(
                                    "import `{}` was routed to a dynamic library by the resolver \
                                 but no longer classifies as one ({:?}); the Mach-O bind name \
                                 cannot be derived",
                                    raw_name, other
                                ),
                            ));
                        }
                    };
                macho_imports.push(crate::macho::dynamic::MachOImport {
                    raw_name,
                    bind_name,
                    dylib,
                    got_slot_va: 0, // assigned when the `__got` section is placed
                    is_data,
                });
            }
        }

        // Data-object imports can never be served by a function stub.
        // Reject any that are still imports: an imported data object whose
        // address is taken has no slot to bind to. Dynamic x86-64 links
        // synthesize a COPY-relocation slot for each of these in the link
        // driver (before layout), which redefines the symbol in the
        // resolver table — those are not imports anymore and pass here.
        if target.format == crate::target::ObjectFormat::Elf {
            let mut data_imports: Vec<&String> = resolver
                .dynamic_data_imports
                .iter()
                .filter(|name| {
                    resolver
                        .table
                        .get(*name)
                        .is_some_and(|r| r.symbol.is_imported)
                })
                .collect();
            if !data_imports.is_empty() {
                data_imports.sort();
                let shown: Vec<&str> = data_imports.iter().map(|s| s.as_str()).collect();
                return Err(LinkError::new(
                    ErrorCode::UnsupportedRelocation,
                    format!(
                        "imported data symbol(s) are not supported: {} — Adesh links cannot \
                         take the address of C library data objects (no COPY relocations or \
                         GOT-relative loads are emitted yet)",
                        shown.join(", ")
                    ),
                ));
            }
        }
        if target.format == crate::target::ObjectFormat::MachO {
            // Mach-O imported data symbols (e.g. `environ`) bind via non-lazy
            // pointer / GOT slots or data binds. If an imported data symbol is
            // present, verify that it is known (in macho_imports).
            let data_imports: Vec<&String> = resolver
                .dynamic_data_imports
                .iter()
                .filter(|name| {
                    resolver
                        .table
                        .get(*name)
                        .is_some_and(|r| r.symbol.is_imported)
                })
                .collect();
            for name in data_imports {
                if !macho_imports.iter().any(|imp| &imp.raw_name == name) {
                    return Err(LinkError::new(
                        ErrorCode::UnsupportedRelocation,
                        format!("imported data symbol `{}` has no Mach-O import entry", name),
                    ));
                }
            }
        }

        let has_explicit_pe_entry = objects.iter().any(|o| {
            o.symbols.iter().any(|s| {
                s.is_defined && (s.name == "mainCRTStartup" || s.name == "__adesh_windows_start")
            })
        }) || resolver.table.contains_key("mainCRTStartup")
            || resolver.table.contains_key("__adesh_windows_start");

        let has_main = objects.iter().any(|o| {
            o.symbols
                .iter()
                .any(|s| s.is_defined && (s.name == "main" || s.name == "_main"))
        }) || resolver.table.contains_key("main")
            || resolver.table.contains_key("_main");

        let pe_thunks_start_off =
            if target.format == crate::target::ObjectFormat::Pe && !pe_imports.is_empty() {
                let off = text_merged.data.len() as u64;
                let sz = (pe_imports.len() * 8) as u64;
                text_merged.data.resize((off + sz) as usize, 0x90);
                text_merged.size = text_merged.data.len() as u64;
                Some(off)
            } else {
                None
            };

        let pe_entry_stub_off = if target.format == crate::target::ObjectFormat::Pe
            && has_main
            && !has_explicit_pe_entry
        {
            let off = text_merged.data.len() as u64;
            text_merged.data.resize((off + 32) as usize, 0x90);
            text_merged.size = text_merged.data.len() as u64;
            Some(off)
        } else {
            None
        };

        // Trap stub for relocations against symbols that are unresolvable but
        // classified as weak/internal. Routing PC-relative call sites here
        // (instead of zero-filling them, which turns `call sym` into
        // `call $+5` and silently corrupts the stack) makes any genuinely
        // live bad reference fail deterministically and debuggably.
        let trap_stub_off = if target.format == crate::target::ObjectFormat::Pe
            && target.arch == crate::target::Arch::X86_64
        {
            let off = text_merged.data.len() as u64;
            // 16 bytes of UD2 (0x0F 0x0B): guaranteed illegal instruction.
            text_merged.data.resize((off + 16) as usize, 0x90);
            text_merged.data[off as usize] = 0x0F;
            text_merged.data[off as usize + 1] = 0x0B;
            text_merged.size = text_merged.data.len() as u64;
            Some(off)
        } else {
            None
        };

        // PE's loader writes the TLS module index into this process-global
        // slot. MSVC/Rust COFF objects commonly leave `_tls_index` undefined,
        // expecting the linker to provide it.
        let has_input_tls_index = objects.iter().any(|obj| {
            obj.symbols.iter().any(|sym| {
                sym.is_defined && sym.name == "_tls_index" && sym.section_index.is_some()
            })
        });
        let has_input_tls_used = objects.iter().any(|obj| {
            obj.symbols.iter().any(|sym| {
                sym.is_defined
                    && (sym.name == "_tls_used" || sym.name == "__tls_used")
                    && sym.section_index.is_some()
            })
        });
        let needs_pe_tls = target.format == crate::target::ObjectFormat::Pe
            && (tls_merged.size > 0 || tls_callbacks_off.is_some());
        let synthesized_tls_index_off = if needs_pe_tls && !has_input_tls_index {
            let off = align_to(data_merged.data.len() as u64, 4);
            if off > data_merged.data.len() as u64 {
                data_merged.data.resize(off as usize, 0);
            }
            data_merged.data.extend_from_slice(&0u32.to_le_bytes());
            data_merged.size = data_merged.data.len() as u64;
            Some(off)
        } else {
            None
        };

        // IMAGE_TLS_DIRECTORY64. Its four VA fields are patched after virtual
        // addresses are assigned and recorded for ASLR rebasing.
        let synthesized_tls_directory_off = if needs_pe_tls && !has_input_tls_used {
            let off = align_to(rodata_merged.data.len() as u64, 8);
            if off > rodata_merged.data.len() as u64 {
                rodata_merged.data.resize(off as usize, 0);
            }
            rodata_merged.data.resize((off + 40) as usize, 0);
            rodata_merged.size = rodata_merged.data.len() as u64;
            Some(off)
        } else {
            None
        };

        // COMMON (tentative definition) blocks are allocated in `.bss`: the
        // first object that declares a name decides its size and alignment.
        // `Symbol.value` carries the required alignment for a COMMON symbol
        // and `Symbol.size` the requested size (see the ELF/COFF readers).
        let mut common_allocs: HashMap<String, u64> = HashMap::new();
        for obj in objects.iter() {
            for sym in &obj.symbols {
                if !sym.is_defined || sym.sym_type != crate::symbol::SymbolType::Common {
                    continue;
                }
                if sym.name.is_empty() || common_allocs.contains_key(&sym.name) {
                    continue;
                }
                let off = bss_merged.append_common(sym.size.max(1), sym.value.max(1));
                common_allocs.insert(sym.name.clone(), off);
            }
        }

        let mut merged_list = Vec::new();
        let mut cat_to_idx: HashMap<SectionCat, usize> = HashMap::new();

        // ELF x86-64: preserve real input `.eh_frame` records for ELF objects
        // that provide them (e.g. `libadesh_runtime.a`) and synthesize `.eh_frame`
        // + `.eh_frame_hdr` records (with callee-saved register restoration and
        // optional personality/LSDA metadata) for Adesh objects.
        let files_with_input_eh: HashSet<usize> =
            input_eh_frame_sections.iter().map(|&(f, _)| f).collect();
        let (kept_input_eh, adesh_frame_entries) =
            if target.format == crate::target::ObjectFormat::Elf
                && target.arch == crate::target::Arch::X86_64
            {
                (
                    collect_kept_input_eh(objects, &sec_placement, &input_eh_frame_sections),
                    collect_adesh_frame_entries(
                        objects,
                        &sec_placement,
                        &files_with_input_eh,
                        &adesh_fde_info_map,
                    ),
                )
            } else {
                (Vec::new(), Vec::new())
            };

        // Order matters for the ELF and Mach-O writers: they map the first
        // group of sections into a read/execute segment and the rest into a
        // read/write segment. Emitting every read-only section before the
        // writable ones keeps those two groups contiguous, so the layout
        // engine's virtual addresses can be used verbatim by both writers.
        if text_merged.size > 0 || !text_merged.data.is_empty() {
            cat_to_idx.insert(SectionCat::Text, merged_list.len());
            merged_list.push(text_merged);
        }
        if rodata_merged.size > 0 || !rodata_merged.data.is_empty() {
            cat_to_idx.insert(SectionCat::Rodata, merged_list.len());
            merged_list.push(rodata_merged);
        }
        if meta_merged.size > 0 || !meta_merged.data.is_empty() {
            cat_to_idx.insert(SectionCat::Meta, merged_list.len());
            merged_list.push(meta_merged);
        }

        // Mach-O local GOT slots (`__got_local` in `__DATA_CONST`): holds one
        // 8-byte pointer per defined or weak-undefined symbol accessed via a
        // GOT-relative relocation (`ARM64_RELOC_GOT_LOAD_PAGE21` /
        // `GOT_LOAD_PAGEOFF12` / `POINTER_TO_GOT` / `X86_64_RELOC_GOT*`).
        // Kept separate from `__got` (dyld non-lazy import pointers) because
        // dyld binds every slot in `__got` as an external import.
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        enum MachoGotKey {
            Global(String),
            Local(usize, String),
        }
        let macho_got_key = |reloc: &crate::relocation::Relocation| -> MachoGotKey {
            if let (Some(f_idx), Some(si)) = (reloc.file_index, reloc.symbol_index)
                && objects
                    .get(f_idx)
                    .and_then(|o| o.symbols.get(si))
                    .is_some_and(|s| s.is_local())
            {
                return MachoGotKey::Local(f_idx, reloc.symbol_name.clone());
            }
            MachoGotKey::Global(reloc.symbol_name.clone())
        };
        let macho_got_local_keys: std::collections::BTreeSet<MachoGotKey> =
            if target.format == crate::target::ObjectFormat::MachO {
                let import_names: std::collections::HashSet<&str> = macho_imports
                    .iter()
                    .map(|imp| imp.raw_name.as_str())
                    .collect();
                let mut keys = std::collections::BTreeSet::new();
                for obj in objects {
                    for sec in &obj.sections {
                        if !sec.is_live || sec.kind == SectionKind::Debug {
                            continue;
                        }
                        for reloc in &sec.relocations {
                            if matches!(
                                reloc.kind,
                                RelocationKind::GotRelative32
                                    | RelocationKind::AArch64AdrGotPage
                                    | RelocationKind::AArch64Ld64GotLo12
                            ) && !reloc.symbol_name.is_empty()
                                && !import_names.contains(reloc.symbol_name.as_str())
                            {
                                keys.insert(macho_got_key(reloc));
                            }
                        }
                    }
                }
                keys
            } else {
                std::collections::BTreeSet::new()
            };
        let mut macho_got_local_section_index: Option<usize> = None;
        if !macho_got_local_keys.is_empty() {
            let mut got_local_sec = MergedSection::new(
                "__got_local",
                SectionKind::Rodata,
                flags::READ | flags::ALLOC,
                8,
            );
            let sz = (macho_got_local_keys.len() * 8) as u64;
            got_local_sec.data = vec![0u8; sz as usize];
            got_local_sec.size = sz;
            macho_got_local_section_index = Some(merged_list.len());
            merged_list.push(got_local_sec);
        }

        let input_fde_count: usize = kept_input_eh.iter().map(|s| s.fdes.len()).sum();
        let total_fde_count = input_fde_count + adesh_frame_entries.len();
        let mut planned_eh_frame_size = 0u64;
        if total_fde_count > 0 {
            for sec in &kept_input_eh {
                for cie in &sec.cies {
                    planned_eh_frame_size += cie.bytes.len() as u64;
                }
                for fde in &sec.fdes {
                    planned_eh_frame_size += fde.bytes.len() as u64;
                }
            }
            let any_adesh_plain = adesh_frame_entries.iter().any(|e| {
                e.info
                    .as_ref()
                    .and_then(|i| i.personality_symbol.as_ref())
                    .is_none()
            });
            let any_adesh_pers = adesh_frame_entries.iter().any(|e| {
                e.info
                    .as_ref()
                    .and_then(|i| i.personality_symbol.as_ref())
                    .is_some()
            });
            if any_adesh_plain {
                planned_eh_frame_size += crate::unwind::EhFrameGenerator::CIE_SIZE;
            }
            if any_adesh_pers {
                planned_eh_frame_size += crate::unwind::EhFrameGenerator::PERSONALITY_CIE_SIZE;
            }
            for e in &adesh_frame_entries {
                let spec = if let Some(ref info) = e.info {
                    crate::unwind::AdeshFdeSpec {
                        func_va: 0,
                        func_size: e.size,
                        callee_saves_done_offset: info.callee_saves_done_offset,
                        saved_gprs: info.saved_gprs.clone(),
                        lsda_va: None,
                        has_personality: info.personality_symbol.is_some(),
                        is_terminal_entry: e.is_terminal_entry,
                    }
                } else {
                    crate::unwind::AdeshFdeSpec {
                        func_va: 0,
                        func_size: e.size,
                        callee_saves_done_offset: 4,
                        saved_gprs: Vec::new(),
                        lsda_va: None,
                        has_personality: false,
                        is_terminal_entry: e.is_terminal_entry,
                    }
                };
                planned_eh_frame_size += crate::unwind::EhFrameGenerator::fde_size_for_spec(&spec);
            }
            planned_eh_frame_size += crate::unwind::EhFrameGenerator::TERMINATOR_SIZE;

            let hdr_size = crate::unwind::EhFrameGenerator::eh_frame_hdr_size(total_fde_count);
            let mut eh_frame_sec = MergedSection::new(
                ".eh_frame",
                SectionKind::Rodata,
                flags::READ | flags::ALLOC,
                4,
            );
            eh_frame_sec.data = vec![0u8; planned_eh_frame_size as usize];
            eh_frame_sec.size = planned_eh_frame_size;
            cat_to_idx.insert(SectionCat::EhFrame, merged_list.len());
            merged_list.push(eh_frame_sec);

            let mut hdr_sec = MergedSection::new(
                ".eh_frame_hdr",
                SectionKind::Rodata,
                flags::READ | flags::ALLOC,
                4,
            );
            hdr_sec.data = vec![0u8; hdr_size as usize];
            hdr_sec.size = hdr_size;
            cat_to_idx.insert(SectionCat::EhFrameHdr, merged_list.len());
            merged_list.push(hdr_sec);
        }
        if data_merged.size > 0 || !data_merged.data.is_empty() {
            cat_to_idx.insert(SectionCat::Data, merged_list.len());
            merged_list.push(data_merged);
        }
        // The TLV descriptor table is ordinary writable image data whose bytes
        // the linker rewrites, so it belongs with the other data sections and
        // before the zero-fill groups.
        if tlv_merged.size > 0 || !tlv_merged.data.is_empty() {
            cat_to_idx.insert(SectionCat::TlvVars, merged_list.len());
            merged_list.push(tlv_merged);
        }
        // Mach-O import pointers (`__got`): one 8-byte slot per imported symbol,
        // bound eagerly by dyld at load. Placed before the zero-fill TLS/BSS
        // sections so every file-backed `__DATA` section precedes zero-fill
        // sections in the segment.
        let has_macho_file_data = target.format == crate::target::ObjectFormat::MachO
            && (cat_to_idx.contains_key(&SectionCat::Data)
                || cat_to_idx.contains_key(&SectionCat::TlvVars));
        let mut macho_la_section_index: Option<usize> = None;
        if target.format == crate::target::ObjectFormat::MachO && !macho_imports.is_empty() {
            let mut la_sec = MergedSection::new(
                "__got",
                SectionKind::Data,
                flags::READ | flags::WRITE | flags::ALLOC,
                8,
            );
            la_sec.data = vec![0u8; macho_imports.len() * 8];
            la_sec.size = la_sec.data.len() as u64;
            macho_la_section_index = Some(merged_list.len());
            merged_list.push(la_sec);
        }
        // Keep the shared TLS layout for every output format (before `.bss` so
        // file-backed `.tdata` precedes `SHT_NOBITS` sections in the RW segment).
        if tls_merged.size > 0 || !tls_merged.data.is_empty() {
            cat_to_idx.insert(SectionCat::Tls, merged_list.len());
            merged_list.push(tls_merged);
        }
        if bss_merged.size > 0 || !bss_merged.data.is_empty() {
            cat_to_idx.insert(SectionCat::Bss, merged_list.len());
            merged_list.push(bss_merged);
        }
        let mut debug_names: Vec<String> = debug_merged_map.keys().cloned().collect();
        debug_names.sort();
        for name in debug_names {
            if let Some(sec) = debug_merged_map.remove(&name) {
                if sec.size > 0 || !sec.data.is_empty() {
                    cat_to_idx.insert(SectionCat::Debug(name), merged_list.len());
                    merged_list.push(sec);
                }
            }
        }

        // 2. Assign Virtual Addresses
        let mut current_va = target.image_base + 0x1000; // Start at base + 4KB

        for (idx, merged) in merged_list.iter_mut().enumerate() {
            if !merged.is_alloc() {
                merged.virtual_address = 0;
                continue;
            }
            let align = if Some(idx) == macho_la_section_index && has_macho_file_data {
                8
            } else {
                merged.alignment.max(target.page_size)
            };
            current_va = align_to(current_va, align);
            merged.virtual_address = current_va;
            current_va += merged.size;
        }

        if let Some(got_idx) = macho_la_section_index {
            let la_va = merged_list[got_idx].virtual_address;
            for (idx, import) in macho_imports.iter_mut().enumerate() {
                import.got_slot_va = la_va + (idx as u64) * 8;
            }
        }
        let mut macho_got_local_map: HashMap<MachoGotKey, u64> = HashMap::new();
        if let Some(got_local_idx) = macho_got_local_section_index {
            let got_local_va = merged_list[got_local_idx].virtual_address;
            for (idx, key) in macho_got_local_keys.iter().enumerate() {
                macho_got_local_map.insert(key.clone(), got_local_va + (idx as u64) * 8);
            }
        }

        let mut pe_imp_result = None;
        if target.format == crate::target::ObjectFormat::Pe && !pe_imports.is_empty() {
            let idata_va = align_to(current_va, target.page_size.max(0x1000));
            let idata_rva = (idata_va - target.image_base) as u32;
            let imp_res =
                crate::pe::import::build_import_table(&pe_imports, target.image_base, idata_rva);
            let mut idata_sec = MergedSection::new(
                ".idata",
                SectionKind::Data,
                flags::READ | flags::WRITE | flags::ALLOC,
                8,
            );
            idata_sec.virtual_address = idata_va;
            idata_sec.data = imp_res.data.clone();
            idata_sec.size = imp_res.data.len() as u64;
            current_va = idata_va + idata_sec.size;
            merged_list.push(idata_sec);
            pe_imp_result = Some(imp_res);
        }

        let mut elf_got_map: HashMap<String, u64> = HashMap::new();
        let mut elf_tls_got_map: HashMap<String, u64> = HashMap::new();
        let mut elf_tls_gd_map: HashMap<String, u64> = HashMap::new();
        let mut elf_got_section_index: Option<usize> = None;
        let has_global_offset_table_reloc = if target.format == crate::target::ObjectFormat::Elf {
            objects
                .iter()
                .flat_map(|obj| obj.sections.iter())
                .filter(|sec| sec.is_live)
                .flat_map(|sec| sec.relocations.iter())
                .any(|reloc| {
                    ((reloc.kind == RelocationKind::RiscvGotHi20
                        || reloc.kind == RelocationKind::RiscvPcrelHi20)
                        && reloc.symbol_name == "_GLOBAL_OFFSET_TABLE_")
                        || matches!(
                            reloc.kind,
                            RelocationKind::X86GotPc32
                                | RelocationKind::X86GotOff32
                                | RelocationKind::X86Got32
                        )
                })
        } else {
            false
        };

        let elf_got_local_names: std::collections::BTreeSet<String> =
            if target.format == crate::target::ObjectFormat::Elf {
                objects
                    .iter()
                    .flat_map(|obj| obj.sections.iter())
                    .filter(|sec| sec.is_live)
                    .flat_map(|sec| sec.relocations.iter())
                    .filter(|reloc| {
                        reloc.kind == RelocationKind::GotRelative32
                            || reloc.kind == RelocationKind::X86Got32
                            || reloc.kind == RelocationKind::AArch64AdrGotPage
                            || reloc.kind == RelocationKind::AArch64Ld64GotLo12
                            || reloc.kind == RelocationKind::RiscvGotHi20
                    })
                    .map(|reloc| reloc.symbol_name.clone())
                    .filter(|name| {
                        !name.is_empty()
                            && !elf_imports.contains(name)
                            && name != "_GLOBAL_OFFSET_TABLE_"
                    })
                    .collect()
            } else {
                std::collections::BTreeSet::new()
            };
        // Pre-scan for initial-exec TLS references (R_X86_64_GOTTPOFF): every
        // referenced TLS variable needs a GOT slot holding the
        // thread-pointer-relative offset. The slot is filled at link time for
        // static images and by the dynamic loader (R_X86_64_TPOFF64) for
        // dynamic ones, so slots are needed in both modes. Deterministic
        // (BTreeSet) order keeps the GOT layout stable for identical inputs.
        //
        // General-dynamic references (R_X86_64_TLSGD) to TLS variables
        // defined in this link are collected too: the reloc loop relaxes
        // their code sequences into the initial-exec form (the GNU ld
        // approach for executables), which reads the same GOT slots.
        // GD references to variables not defined here still fail loudly
        // there — they would need a DTV and __tls_get_addr runtime.
        let elf_tls_ie_names: std::collections::BTreeSet<String> =
            if target.format == crate::target::ObjectFormat::Elf {
                let mut names = std::collections::BTreeSet::new();
                for obj in objects {
                    for sec in &obj.sections {
                        for reloc in &sec.relocations {
                            let collect = match reloc.kind {
                                RelocationKind::TlsInitialExec
                                | RelocationKind::RiscvTlsGotHi20 => true,
                                RelocationKind::TlsGeneralDynamic => resolver
                                    .table
                                    .get(&reloc.symbol_name)
                                    .is_some_and(|r| r.defined_in_sec_index.is_some()),
                                _ => false,
                            };
                            if collect && !reloc.symbol_name.is_empty() {
                                names.insert(reloc.symbol_name.clone());
                            }
                        }
                    }
                }
                names
            } else {
                std::collections::BTreeSet::new()
            };
        let elf_tls_gd_names: std::collections::BTreeSet<String> =
            if target.format == crate::target::ObjectFormat::Elf {
                let mut names = std::collections::BTreeSet::new();
                for obj in objects {
                    for sec in &obj.sections {
                        for reloc in &sec.relocations {
                            if reloc.kind == RelocationKind::RiscvTlsGdHi20
                                && !reloc.symbol_name.is_empty()
                            {
                                names.insert(reloc.symbol_name.clone());
                            }
                        }
                    }
                }
                names
            } else {
                std::collections::BTreeSet::new()
            };
        if target.format == crate::target::ObjectFormat::Elf
            && (!elf_imports.is_empty()
                || !elf_tls_ie_names.is_empty()
                || !elf_tls_gd_names.is_empty()
                || !elf_got_local_names.is_empty()
                || has_global_offset_table_reloc)
        {
            // Page-aligned start so `PT_GNU_RELRO` can cover the GOT (and the
            // dynamic region after it) without also mprotecting preceding
            // writable user data that shares its final page.
            let got_slot_size: usize = if target.pointer_width.bits() == 32 {
                4
            } else {
                8
            };
            let got_va = align_to(current_va, target.page_size.max(got_slot_size as u64));
            let mut got_sec = MergedSection::new(
                ".got",
                SectionKind::Data,
                flags::READ | flags::WRITE | flags::ALLOC,
                got_slot_size as u64,
            );
            got_sec.virtual_address = got_va;
            // GOT prologue: slot 0 holds &_DYNAMIC (patched by the writer once
            // the .dynamic address is known, or left 0 for static links);
            // slots 1 and 2 are reserved for the dynamic loader's link-map and
            // resolver pointers (unused with DT_BIND_NOW, reserved per ABI).
            // One slot per imported symbol, local GOT-referenced symbol, and
            // initial-exec TLS variable follows.
            // General-dynamic TLS variables take 2 slots for struct tls_index.
            let n_header_slots = 3usize;
            got_sec.data = vec![
                0u8;
                (n_header_slots
                    + elf_imports.len()
                    + elf_got_local_names.len()
                    + elf_tls_ie_names.len()
                    + elf_tls_gd_names.len() * 2)
                    * got_slot_size
            ];
            got_sec.size = got_sec.data.len() as u64;
            elf_got_map.insert("_GLOBAL_OFFSET_TABLE_".to_string(), got_va);
            for (idx, sym) in elf_imports.iter().enumerate() {
                let slot_va = got_va + ((n_header_slots + idx) * got_slot_size) as u64;
                elf_got_map.insert(sym.clone(), slot_va);
            }
            for (idx, name) in elf_got_local_names.iter().enumerate() {
                let slot_va =
                    got_va + ((n_header_slots + elf_imports.len() + idx) * got_slot_size) as u64;
                elf_got_map.insert(name.clone(), slot_va);
            }
            for (idx, name) in elf_tls_ie_names.iter().enumerate() {
                let slot_va = got_va
                    + ((n_header_slots + elf_imports.len() + elf_got_local_names.len() + idx)
                        * got_slot_size) as u64;
                elf_tls_got_map.insert(name.clone(), slot_va);
            }
            let gd_base_idx = n_header_slots
                + elf_imports.len()
                + elf_got_local_names.len()
                + elf_tls_ie_names.len();
            for (idx, name) in elf_tls_gd_names.iter().enumerate() {
                let slot_va = got_va + ((gd_base_idx + idx * 2) * got_slot_size) as u64;
                elf_tls_gd_map.insert(name.clone(), slot_va);
            }
            current_va = got_va + got_sec.size;
            elf_got_section_index = Some(merged_list.len());
            merged_list.push(got_sec);
        }

        let _ = macho_la_section_index;

        // Fill the pre-sized .eh_frame / .eh_frame_hdr sections with the
        // synthesized standard-frame CFI. The sections were sized before
        // virtual addresses existed; now that every function's final
        // address is known, the CIE/FDE bytes and the .eh_frame_hdr binary
        // search index can be written for real.
        // `.eh_frame` and `.eh_frame_hdr` are populated below once `resolve_sym_va`
        // is ready so input `.eh_frame` relocations and personality/LSDA symbols
        // resolve against exact final VAs.

        // Windows x64 SEH `.pdata` and `.xdata` are populated below once
        // `resolve_sym_va` is ready so input `.pdata` relocations,
        // `__adesh_win64_personality`, and per-function LSDA symbols resolve
        // against exact final RVAs.

        // Final base VA of every input section, keyed (file, section).
        // ICF-folded sections alias their canonical twin's placement. Used
        // by the precise (file, symbol-index) relocation resolution below.
        let mut sec_va_map: HashMap<(usize, usize), u64> = HashMap::new();
        // COFF SECREL32 offsets are relative to the *output* section, not
        // the source object's contribution. TLS variables from different
        // `.tls$` contributions must have distinct offsets in the one merged
        // per-thread template.
        let mut output_sec_base: HashMap<(usize, usize), u64> = HashMap::new();
        for ((f_idx, s_idx), (cat, off)) in &sec_placement {
            if let Some(&m_idx) = cat_to_idx.get(cat) {
                let base = merged_list[m_idx].virtual_address;
                sec_va_map.insert((*f_idx, *s_idx), base + off);
                output_sec_base.insert((*f_idx, *s_idx), base);
            }
        }
        let mut symbol_output_base: HashMap<&str, u64> = HashMap::new();
        for (f_idx, obj) in objects.iter().enumerate() {
            for sym in &obj.symbols {
                if sym.is_defined {
                    if let Some(s_idx) = sym.section_index {
                        if let Some(&base) = output_sec_base.get(&(f_idx, s_idx)) {
                            symbol_output_base.entry(sym.name.as_str()).or_insert(base);
                        }
                    }
                }
            }
        }

        // 3. Security Validation: W^X Check
        for merged in &merged_list {
            if merged.is_writable() && merged.is_executable() {
                return Err(LinkError::new(
                    ErrorCode::SecurityViolation,
                    format!(
                        "W^X violation: section `{}` is both writable and executable",
                        merged.name
                    ),
                ));
            }
        }

        // 4. Compute Final Symbol Virtual Addresses (both local and global)
        let mut symbol_va_map: HashMap<String, u64> = HashMap::new();
        // Keyed by a borrowed symbol name so the per-relocation fallback does
        // not allocate a `String` for every relocation in the link.
        let mut file_local_va_map: HashMap<(usize, &str), u64> = HashMap::new();
        let mut final_symbols = Vec::new();

        // Standard linker-defined module base symbols
        if target.format == crate::target::ObjectFormat::Pe {
            symbol_va_map.insert("__ImageBase".to_string(), target.image_base);
            symbol_va_map.insert("_IMAGE_DOS_HEADER".to_string(), target.image_base);
            symbol_va_map.insert("ImageBase".to_string(), target.image_base);
        } else if target.format == crate::target::ObjectFormat::Elf {
            symbol_va_map.insert("__ehdr_start".to_string(), target.image_base);
            symbol_va_map.insert("__dso_handle".to_string(), target.image_base);
        }

        let input_symbol_va = |names: &[&str]| -> Option<u64> {
            objects.iter().enumerate().find_map(|(f_idx, obj)| {
                obj.symbols.iter().find_map(|sym| {
                    if sym.is_defined && names.contains(&sym.name.as_str()) {
                        sym.section_index.and_then(|s_idx| {
                            sec_va_map.get(&(f_idx, s_idx)).map(|base| base + sym.value)
                        })
                    } else {
                        None
                    }
                })
            })
        };

        let mut pe_tls_info = None;
        let mut tls_directory_base_relocs = Vec::new();
        if needs_pe_tls {
            let tls_va = cat_to_idx
                .get(&SectionCat::Tls)
                .map(|idx| merged_list[*idx].virtual_address)
                .ok_or_else(|| {
                    LinkError::new(
                        ErrorCode::InvalidSection,
                        "PE TLS layout missing .tls section",
                    )
                })?;
            let tls_size = cat_to_idx
                .get(&SectionCat::Tls)
                .map(|idx| merged_list[*idx].size)
                .expect("PE TLS layout missing .tls section");
            let tls_initialized_size = cat_to_idx
                .get(&SectionCat::Tls)
                .map(|idx| merged_list[*idx].data.len() as u64)
                .expect("PE TLS layout missing .tls section");
            let tls_zero_fill = u32::try_from(tls_size - tls_initialized_size).map_err(|_| {
                LinkError::new(
                    ErrorCode::InvalidSection,
                    "PE TLS zero-fill exceeds 32 bits",
                )
            })?;
            let tls_index_va = synthesized_tls_index_off
                .and_then(|off| {
                    cat_to_idx
                        .get(&SectionCat::Data)
                        .map(|idx| merged_list[*idx].virtual_address + off)
                })
                .or_else(|| input_symbol_va(&["_tls_index"]))
                .ok_or_else(|| {
                    LinkError::new(
                        ErrorCode::InvalidSection,
                        "PE TLS layout missing _tls_index",
                    )
                })?;
            let tls_directory_va = synthesized_tls_directory_off
                .and_then(|off| {
                    cat_to_idx
                        .get(&SectionCat::Rodata)
                        .map(|idx| merged_list[*idx].virtual_address + off)
                })
                .or_else(|| input_symbol_va(&["_tls_used", "__tls_used"]))
                .ok_or_else(|| {
                    LinkError::new(ErrorCode::InvalidSection, "PE TLS layout missing _tls_used")
                })?;

            symbol_va_map.insert("_tls_index".to_string(), tls_index_va);
            symbol_va_map.insert("_tls_used".to_string(), tls_directory_va);
            symbol_va_map.insert("__tls_used".to_string(), tls_directory_va);

            if let Some(dir_off) = synthesized_tls_directory_off {
                let rodata_idx = *cat_to_idx
                    .get(&SectionCat::Rodata)
                    .expect("TLS directory requires a rodata section");
                let callbacks_va = tls_callbacks_off
                    .map(|off| merged_list[rodata_idx].virtual_address + off)
                    .unwrap_or(0);
                let tls_end_va = tls_va + tls_initialized_size;
                let dir =
                    &mut merged_list[rodata_idx].data[dir_off as usize..dir_off as usize + 40];
                dir[0..8].copy_from_slice(&tls_va.to_le_bytes());
                dir[8..16].copy_from_slice(&tls_end_va.to_le_bytes());
                dir[16..24].copy_from_slice(&tls_index_va.to_le_bytes());
                dir[24..32].copy_from_slice(&callbacks_va.to_le_bytes());
                dir[32..36].copy_from_slice(&tls_zero_fill.to_le_bytes());
                dir[36..40].copy_from_slice(&0u32.to_le_bytes());

                let dir_rva =
                    (merged_list[rodata_idx].virtual_address - target.image_base + dir_off) as u32;
                for field_off in [0u32, 8, 16] {
                    tls_directory_base_relocs.push(dir_rva + field_off);
                }
                if callbacks_va != 0 {
                    tls_directory_base_relocs.push(dir_rva + 24);
                }
                pe_tls_info = Some(PeTlsInfo {
                    directory_rva: dir_rva,
                    directory_size: 40,
                });
            } else {
                pe_tls_info = Some(PeTlsInfo {
                    directory_rva: (tls_directory_va - target.image_base) as u32,
                    directory_size: 40,
                });
            }
        }

        // Deterministic first-definition-wins name index for the relocation
        // fallback below (replaces a per-relocation linear scan of every
        // file-local symbol, which made linking O(relocs * symbols)).
        let mut local_name_to_va: HashMap<String, u64> = HashMap::new();

        // Compute addresses of all defined symbols across all object files
        for (f_idx, obj) in objects.iter().enumerate() {
            for sym in &obj.symbols {
                if !sym.is_defined {
                    continue;
                }

                // COMMON (tentative definition) storage lives in .bss.
                if sym.sym_type == crate::symbol::SymbolType::Common {
                    if let Some(&off) = common_allocs.get(&sym.name)
                        && let Some(&m_idx) = cat_to_idx.get(&SectionCat::Bss)
                    {
                        let sym_va = merged_list[m_idx].virtual_address + off;
                        if sym.binding != crate::symbol::SymbolBinding::Local {
                            symbol_va_map.insert(sym.name.clone(), sym_va);
                        } else {
                            symbol_va_map.entry(sym.name.clone()).or_insert(sym_va);
                        }
                        file_local_va_map.insert((f_idx, sym.name.as_str()), sym_va);
                        local_name_to_va.entry(sym.name.clone()).or_insert(sym_va);
                    }
                    continue;
                }

                if let Some(sec_idx) = sym.section_index {
                    if let Some((cat, sec_off)) = sec_placement.get(&(f_idx, sec_idx)) {
                        if let Some(&m_idx) = cat_to_idx.get(cat) {
                            let sec_va = merged_list[m_idx].virtual_address;
                            let sym_va = sec_va + *sec_off + sym.value;
                            if sym.binding != crate::symbol::SymbolBinding::Local {
                                symbol_va_map.insert(sym.name.clone(), sym_va);
                            } else {
                                symbol_va_map.entry(sym.name.clone()).or_insert(sym_va);
                            }
                            file_local_va_map.insert((f_idx, sym.name.as_str()), sym_va);
                            local_name_to_va.entry(sym.name.clone()).or_insert(sym_va);
                        }
                    }
                }
            }
        }

        // Add globals from resolver table in deterministic sorted order so
        // symbol tables, string tables, and Mach-O code signatures are stable.
        let mut sorted_global_names: Vec<&String> = resolver.table.keys().collect();
        sorted_global_names.sort();
        for sym_name in sorted_global_names {
            let resolved = &resolver.table[sym_name];
            // Filter out synthetic alias keys (e.g., `_foo` alias for `foo` in Mach-O)
            // so each defined symbol is emitted into final_symbols exactly once.
            if sym_name != &resolved.symbol.name {
                continue;
            }
            let mut final_sym = resolved.symbol.clone();
            if let Some(&va) = symbol_va_map.get(sym_name) {
                final_sym.value = va;
            }
            final_symbols.push(final_sym);
        }

        // Alias keys (e.g. a stale-hash LTO-promoted name fuzzy-matched by the
        // resolver onto the current definition) need a VA too, or relocations
        // referencing the alias name fall through to the trap stub.
        for (sym_name, resolved) in &resolver.table {
            if sym_name == &resolved.symbol.name {
                continue;
            }
            if let Some(&va) = symbol_va_map.get(resolved.symbol.name.as_str()) {
                symbol_va_map.entry(sym_name.clone()).or_insert(va);
            } else if resolved.symbol.is_defined
                && let Some(s_idx) = resolved.defined_in_sec_index
                && let Some((cat, sec_off)) =
                    sec_placement.get(&(resolved.defined_in_file_index, s_idx))
                && let Some(&m_idx) = cat_to_idx.get(cat)
            {
                // The canonical name has no VA of its own (e.g. its defining
                // member was pulled in for the alias only). Fall back to the
                // recorded definition placement.
                let sec_va = merged_list[m_idx].virtual_address;
                symbol_va_map
                    .entry(sym_name.clone())
                    .or_insert(sec_va + *sec_off + resolved.symbol.value);
            }
        }

        // Synthesized linker symbols without an input section need explicit
        // addresses. Undefined weak placeholders stay out of this map so the
        // relocation pass can diagnose only retained references and route
        // unresolved calls to the trap stub.
        for (sym_name, resolved) in &resolver.table {
            if resolver.weak_undefined_symbols.contains(sym_name) {
                continue;
            }
            if resolved.symbol.is_defined
                && resolved.symbol.section_index.is_none()
                && resolved.symbol.sym_type != crate::symbol::SymbolType::Common
            {
                symbol_va_map
                    .entry(sym_name.clone())
                    .or_insert(resolved.symbol.value);
            }
        }

        // Patch PE Import Thunks and map IAT symbols
        if let Some(ref imp_res) = pe_imp_result {
            let text_va = merged_list
                .iter()
                .find(|s| s.name == ".text")
                .map(|s| s.virtual_address)
                .unwrap_or(target.image_base + 0x1000);

            for (sym_name, sym_iat_rva) in &imp_res.symbol_iat_rvas {
                let iat_va = target.image_base + (*sym_iat_rva as u64);
                symbol_va_map.insert(format!("__imp_{}", sym_name), iat_va);
                symbol_va_map.insert(format!("__imp__{}", sym_name), iat_va);
                symbol_va_map.insert(format!("_imp_{}", sym_name), iat_va);
                symbol_va_map.insert(format!("_imp__{}", sym_name), iat_va);
            }

            if let Some(thunks_off) = pe_thunks_start_off {
                if let Some(&text_idx) = cat_to_idx.get(&SectionCat::Text) {
                    for (i, sym) in pe_imports.iter().enumerate() {
                        let thunk_va = text_va + thunks_off + (i as u64 * 8);
                        if let Some(&sym_iat_rva) = imp_res.symbol_iat_rvas.get(&sym.symbol_name) {
                            let iat_va = target.image_base + (sym_iat_rva as u64);
                            // FF 25 <disp32> : RIP during execution is thunk_va + 6
                            let disp32 = (iat_va as i64 - (thunk_va as i64 + 6)) as i32;
                            let thunk_bytes = [
                                0xFF,
                                0x25,
                                (disp32 & 0xFF) as u8,
                                ((disp32 >> 8) & 0xFF) as u8,
                                ((disp32 >> 16) & 0xFF) as u8,
                                ((disp32 >> 24) & 0xFF) as u8,
                                0x90,
                                0x90,
                            ];
                            let off = (thunks_off as usize) + i * 8;
                            merged_list[text_idx].data[off..off + 8].copy_from_slice(&thunk_bytes);

                            symbol_va_map.insert(sym.symbol_name.clone(), thunk_va);
                            symbol_va_map.insert(format!("_{}", sym.symbol_name), thunk_va);
                        }
                    }
                }
            }

            if let Some(entry_stub_off) = pe_entry_stub_off {
                if let Some(&text_idx) = cat_to_idx.get(&SectionCat::Text) {
                    let entry_va = text_va + entry_stub_off;
                    let main_va = *symbol_va_map.get("main").unwrap_or(&text_va);
                    let exit_va = *symbol_va_map.get("ExitProcess").unwrap_or(&text_va);

                    // disp32 from end of call main (at entry_va + 16)
                    let disp_main = (main_va as i64 - (entry_va as i64 + 16)) as i32;
                    // disp32 from end of call ExitProcess (at entry_va + 23)
                    let disp_exit = (exit_va as i64 - (entry_va as i64 + 23)) as i32;

                    let mut stub = [0x90u8; 32];
                    // sub rsp, 40 (4 bytes)
                    stub[0..4].copy_from_slice(&[0x48, 0x83, 0xEC, 0x28]);
                    // xor ecx, ecx (2 bytes)
                    stub[4..6].copy_from_slice(&[0x31, 0xC9]);
                    // xor edx, edx (2 bytes)
                    stub[6..8].copy_from_slice(&[0x31, 0xD2]);
                    // xor r8d, r8d (3 bytes)
                    stub[8..11].copy_from_slice(&[0x45, 0x31, 0xC0]);
                    // call main (5 bytes)
                    stub[11] = 0xE8;
                    stub[12..16].copy_from_slice(&disp_main.to_le_bytes());
                    // mov ecx, eax (2 bytes)
                    stub[16..18].copy_from_slice(&[0x89, 0xC1]);
                    // call ExitProcess (5 bytes)
                    stub[18] = 0xE8;
                    stub[19..23].copy_from_slice(&disp_exit.to_le_bytes());
                    // add rsp, 40 (4 bytes)
                    stub[23..27].copy_from_slice(&[0x48, 0x83, 0xC4, 0x28]);
                    // ret (1 byte)
                    stub[27] = 0xC3;

                    let off = entry_stub_off as usize;
                    merged_list[text_idx].data[off..off + 32].copy_from_slice(&stub);

                    symbol_va_map.insert("mainCRTStartup".to_string(), entry_va);
                    symbol_va_map.insert("_start".to_string(), entry_va);
                    symbol_va_map.insert("__adesh_windows_start".to_string(), entry_va);
                }
            }
        }

        // `__ImageBase` is linker-defined: its *address* is the image base.
        if target.format == crate::target::ObjectFormat::Pe {
            symbol_va_map.insert("__ImageBase".to_string(), target.image_base);
            symbol_va_map.insert("___ImageBase".to_string(), target.image_base);
        }

        // Map ELF GOT symbol slots so PLT stubs and GOTPCREL relocations resolve to their slot VA
        if target.format == crate::target::ObjectFormat::Elf {
            if let Some(sec_idx) = elf_got_section_index {
                let got_va = merged_list[sec_idx].virtual_address;
                symbol_va_map.insert("_GLOBAL_OFFSET_TABLE_".to_string(), got_va);
            }
            for (sym_name, got_slot_va) in &elf_got_map {
                symbol_va_map.insert(format!("__got_{}", sym_name), *got_slot_va);
                symbol_va_map.insert(format!("_got_{}", sym_name), *got_slot_va);
            }
        }

        // Map Mach-O import slots so the proxy stubs synthesized for imports
        // (`adrp`/`ldr`/`br` on arm64, `jmp *[rip + slot]` on x86_64) resolve
        // to the slot VA.
        if target.format == crate::target::ObjectFormat::MachO {
            for import in &macho_imports {
                symbol_va_map.insert(format!("__got_{}", import.raw_name), import.got_slot_va);
            }
        }

        // Find Entry Point Virtual Address
        if let Some(&entry_va) = symbol_va_map.get(entry_name) {
            self.entry_va = entry_va;
        } else if let Some(&main_crt_va) = symbol_va_map.get("mainCRTStartup") {
            self.entry_va = main_crt_va;
        } else if let Some(&start_va) = symbol_va_map.get("_start") {
            self.entry_va = start_va;
        } else if let Some(&main_va) = symbol_va_map.get("main") {
            self.entry_va = main_va;
        } else if let Some(&main_va) = symbol_va_map.get("_main") {
            self.entry_va = main_va;
        } else if let Some(first_text) = merged_list.iter().find(|s| s.is_executable()) {
            self.entry_va = first_text.virtual_address;
        } else {
            self.entry_va = target.image_base + 0x1000;
        }

        // Resolve a relocation target to a VA. Returns the symbol's VA plus,
        // when resolved via the precise path, the VA of the section that
        // contains it (needed by SECREL32-style relocations).
        //
        // Precise path first: resolve local symbols by their exact
        // object-local symbol index. This is required for correctness on
        // LLVM COFF objects, which contain many same-named sections
        // (`.text`, `.rdata`, ...): jump-table entries reference the `.text`
        // section symbol and code references the `.rdata` section symbol, and
        // name-based resolution would pick an arbitrary one of the dozens of
        // same-named sections, producing code that jumps into data.
        let resolve_sym_va = |file_idx_opt: Option<usize>,
                              sym_idx_opt: Option<usize>,
                              name: &str|
         -> (Option<u64>, Option<u64>) {
            if let (Some(f_idx), Some(si)) = (file_idx_opt, sym_idx_opt) {
                if let Some(obj) = objects.get(f_idx) {
                    if let Some(sym) = obj.symbols.get(si) {
                        if sym.is_defined {
                            if let Some(s_idx) = sym.section_index {
                                if let Some(&base) = sec_va_map.get(&(f_idx, s_idx)) {
                                    return (
                                        Some(base + sym.value),
                                        output_sec_base.get(&(f_idx, s_idx)).copied(),
                                    );
                                }
                            }
                        }
                    }
                }
            }

            // Name-based fallback for global/weak/imported symbols and for
            // formats whose readers do not record symbol indices.
            if let Some(f_idx) = file_idx_opt {
                if let Some(&va) = file_local_va_map.get(&(f_idx, name)) {
                    return (Some(va), symbol_output_base.get(name).copied());
                }
            }
            if let Some(&va) = symbol_va_map.get(name) {
                return (Some(va), symbol_output_base.get(name).copied());
            }
            // `__imp_X` is the address of an import *slot*, not the address of
            // `X`. Falling back to `X` makes a synthesized `jmp [__imp_X]`
            // thunk read its own bytes as a function pointer, so the thunk
            // jumps to itself. Import slots are registered explicitly above.
            //
            // The `_`-prefix equivalence is a 32-bit x86 (COFF/ELF) calling
            // convention only. On 64-bit targets `_foo` and `foo` are distinct
            // symbols; aliasing them silently binds the wrong definition.
            if target.arch == crate::target::Arch::X86 {
                if let Some(stripped) = name.strip_prefix('_') {
                    if let Some(&va) = symbol_va_map.get(stripped) {
                        return (Some(va), None);
                    }
                } else if let Some(&va) = symbol_va_map.get(&format!("_{}", name)) {
                    return (Some(va), None);
                }
            }
            // Fall back to the first definition of any file-local symbol
            // with this name (deterministic: lowest file index wins).
            if let Some(&va) = local_name_to_va.get(name) {
                return (Some(va), None);
            }
            (None, None)
        };

        // Populate `.eh_frame` and `.eh_frame_hdr` now that `resolve_sym_va` is
        // available for input `.eh_frame` relocations, `__adesh_personality_v0`,
        // and per-function `.gcc_except_table` LSDA symbols.
        if total_fde_count > 0 {
            let text_base_va = cat_to_idx
                .get(&SectionCat::Text)
                .map(|&idx| merged_list[idx].virtual_address)
                .unwrap_or(0);
            let eh_frame_idx = cat_to_idx
                .get(&SectionCat::EhFrame)
                .copied()
                .expect("FDEs require a pre-sized .eh_frame section");
            let eh_frame_hdr_idx = cat_to_idx
                .get(&SectionCat::EhFrameHdr)
                .copied()
                .expect("FDEs require a pre-sized .eh_frame_hdr section");
            let eh_frame_va = merged_list[eh_frame_idx].virtual_address;
            let hdr_va = merged_list[eh_frame_hdr_idx].virtual_address;

            let mut eh_bytes = Vec::with_capacity(planned_eh_frame_size as usize);
            let mut hdr_entries: Vec<(u64, u64)> = Vec::with_capacity(total_fde_count);

            let apply_input_eh_reloc = |buf: &mut [u8],
                                        rec_orig_off: usize,
                                        rec_new_va: u64,
                                        f_idx: usize,
                                        reloc: &crate::relocation::Relocation|
             -> Option<u64> {
                let in_rec_off = (reloc.offset as usize).checked_sub(rec_orig_off)?;
                let place_va = rec_new_va + in_rec_off as u64;
                let (sym_va_opt, _) =
                    resolve_sym_va(Some(f_idx), reloc.symbol_index, &reloc.symbol_name);
                let sym_va = sym_va_opt.unwrap_or(0);
                let target_va = (sym_va as i64).wrapping_add(reloc.addend) as u64;
                match reloc.kind {
                    crate::relocation::RelocationKind::PcRelative32
                    | crate::relocation::RelocationKind::PltRelative32
                    | crate::relocation::RelocationKind::GotRelative32 => {
                        let disp = (target_va as i64).wrapping_sub(place_va as i64) as i32;
                        if in_rec_off + 4 <= buf.len() {
                            buf[in_rec_off..in_rec_off + 4].copy_from_slice(&disp.to_le_bytes());
                        }
                    }
                    crate::relocation::RelocationKind::PcRelative64 => {
                        let disp = (target_va as i64).wrapping_sub(place_va as i64) as u64;
                        if in_rec_off + 8 <= buf.len() {
                            buf[in_rec_off..in_rec_off + 8].copy_from_slice(&disp.to_le_bytes());
                        }
                    }
                    crate::relocation::RelocationKind::Absolute64 => {
                        if in_rec_off + 8 <= buf.len() {
                            buf[in_rec_off..in_rec_off + 8]
                                .copy_from_slice(&target_va.to_le_bytes());
                        }
                    }
                    crate::relocation::RelocationKind::Absolute32 => {
                        if in_rec_off + 4 <= buf.len() {
                            buf[in_rec_off..in_rec_off + 4]
                                .copy_from_slice(&(target_va as u32).to_le_bytes());
                        }
                    }
                    _ => {}
                }
                Some(target_va)
            };

            // 1. Copy kept input `.eh_frame` records (e.g. `libadesh_runtime.a`)
            for sec in &kept_input_eh {
                let mut orig_to_new_cie: HashMap<usize, (usize, u8)> = HashMap::new();
                for cie in &sec.cies {
                    let new_cie_off = eh_bytes.len();
                    let new_cie_va = eh_frame_va + new_cie_off as u64;
                    let mut buf = cie.bytes.clone();
                    for reloc in &cie.relocs {
                        let _ = apply_input_eh_reloc(
                            &mut buf,
                            cie.orig_offset,
                            new_cie_va,
                            sec.file_index,
                            reloc,
                        );
                    }
                    eh_bytes.extend_from_slice(&buf);
                    orig_to_new_cie.insert(cie.orig_offset, (new_cie_off, cie.pc_begin_encoding));
                }

                for fde in &sec.fdes {
                    let (new_cie_off, _enc) = orig_to_new_cie[&fde.orig_cie_offset];
                    let new_fde_off = eh_bytes.len();
                    let new_fde_va = eh_frame_va + new_fde_off as u64;
                    let mut buf = fde.bytes.clone();
                    // Rewrite CIE_pointer at offset 4 within the FDE record.
                    let cie_ptr = ((new_fde_off + 4) - new_cie_off) as u32;
                    buf[4..8].copy_from_slice(&cie_ptr.to_le_bytes());

                    let mut func_initial_va = 0u64;
                    for (idx, reloc) in fde.relocs.iter().enumerate() {
                        if let Some(target_va) = apply_input_eh_reloc(
                            &mut buf,
                            fde.orig_offset,
                            new_fde_va,
                            sec.file_index,
                            reloc,
                        ) && idx == fde.pc_begin_reloc_idx
                        {
                            func_initial_va = target_va;
                        }
                    }
                    eh_bytes.extend_from_slice(&buf);
                    hdr_entries.push((func_initial_va, new_fde_va));
                }
            }

            // 2. Emit Adesh `.adob` CIE(s) and FDEs
            let plain_entries: Vec<&AdeshFrameEntry> = adesh_frame_entries
                .iter()
                .filter(|e| {
                    e.info
                        .as_ref()
                        .and_then(|i| i.personality_symbol.as_ref())
                        .is_none()
                })
                .collect();
            let pers_entries: Vec<&AdeshFrameEntry> = adesh_frame_entries
                .iter()
                .filter(|e| {
                    e.info
                        .as_ref()
                        .and_then(|i| i.personality_symbol.as_ref())
                        .is_some()
                })
                .collect();

            if !plain_entries.is_empty() {
                let plain_cie_va = eh_frame_va + eh_bytes.len() as u64;
                eh_bytes.extend_from_slice(&crate::unwind::EhFrameGenerator::build_cie());
                for e in plain_entries {
                    let func_va = text_base_va + e.text_offset;
                    let fde_va = eh_frame_va + eh_bytes.len() as u64;
                    let spec = if let Some(ref info) = e.info {
                        crate::unwind::AdeshFdeSpec {
                            func_va,
                            func_size: e.size,
                            callee_saves_done_offset: info.callee_saves_done_offset,
                            saved_gprs: info.saved_gprs.clone(),
                            lsda_va: None,
                            has_personality: false,
                            is_terminal_entry: e.is_terminal_entry,
                        }
                    } else {
                        crate::unwind::AdeshFdeSpec {
                            func_va,
                            func_size: e.size,
                            callee_saves_done_offset: 4,
                            saved_gprs: Vec::new(),
                            lsda_va: None,
                            has_personality: false,
                            is_terminal_entry: e.is_terminal_entry,
                        }
                    };
                    eh_bytes.extend_from_slice(
                        &crate::unwind::EhFrameGenerator::build_fde_for_spec(
                            plain_cie_va,
                            fde_va,
                            &spec,
                        ),
                    );
                    hdr_entries.push((func_va, fde_va));
                }
            }

            if !pers_entries.is_empty() {
                let pers_cie_va = eh_frame_va + eh_bytes.len() as u64;
                let pers_name = pers_entries[0]
                    .info
                    .as_ref()
                    .and_then(|i| i.personality_symbol.as_deref())
                    .unwrap_or("__adesh_personality_v0");
                let personality_va = resolve_sym_va(None, None, pers_name).0.ok_or_else(|| {
                    LinkError::new(
                        ErrorCode::UndefinedSymbol,
                        format!(
                            "ELF unwind personality symbol `{}` could not be resolved",
                            pers_name
                        ),
                    )
                })?;
                eh_bytes.extend_from_slice(
                    &crate::unwind::EhFrameGenerator::build_cie_with_personality(
                        pers_cie_va,
                        personality_va,
                    ),
                );
                for e in pers_entries {
                    let func_va = text_base_va + e.text_offset;
                    let fde_va = eh_frame_va + eh_bytes.len() as u64;
                    let info = e.info.as_ref().expect("pers_entries must have AdeshFdeInfo");
                    let lsda_va = info.lsda_symbol.as_deref().and_then(|lsym| {
                        resolve_sym_va(Some(e.file_index), None, lsym).0
                    });
                    let spec = crate::unwind::AdeshFdeSpec {
                        func_va,
                        func_size: e.size,
                        callee_saves_done_offset: info.callee_saves_done_offset,
                        saved_gprs: info.saved_gprs.clone(),
                        lsda_va,
                        has_personality: true,
                        is_terminal_entry: e.is_terminal_entry,
                    };
                    eh_bytes.extend_from_slice(
                        &crate::unwind::EhFrameGenerator::build_fde_for_spec(
                            pers_cie_va,
                            fde_va,
                            &spec,
                        ),
                    );
                    hdr_entries.push((func_va, fde_va));
                }
            }

            eh_bytes.extend_from_slice(&0u32.to_le_bytes());

            debug_assert_eq!(eh_bytes.len() as u64, planned_eh_frame_size);
            merged_list[eh_frame_idx].data = eh_bytes;
            merged_list[eh_frame_hdr_idx].data =
                crate::unwind::EhFrameHdrGenerator::build(hdr_va, eh_frame_va, hdr_entries);
            debug_assert_eq!(
                merged_list[eh_frame_hdr_idx].data.len() as u64,
                crate::unwind::EhFrameGenerator::eh_frame_hdr_size(total_fde_count)
            );
        }

        // Synthesize and merge Windows x64 SEH `.pdata` (RUNTIME_FUNCTION) and `.xdata` (UNWIND_INFO)
        // now that `resolve_sym_va` is available for input `.pdata` relocations,
        // `__adesh_win64_personality`, and per-function LSDA symbols.
        if target.format == crate::target::ObjectFormat::Pe
            && target.arch == crate::target::Arch::X86_64
        {
            let text_size = cat_to_idx
                .get(&SectionCat::Text)
                .map(|&idx| merged_list[idx].size)
                .unwrap_or(0);
            if text_size > 0 {
                let end_va = merged_list
                    .iter()
                    .map(|s| s.virtual_address + s.size)
                    .max()
                    .unwrap_or(target.image_base + 0x1000);
                let xdata_va = align_to(end_va, target.page_size.max(0x1000));
                let xdata_base_rva = (xdata_va - target.image_base) as u32;

                let mut pdata_entries: Vec<crate::unwind::PdataEntry> = Vec::new();
                let mut covered_begin_rvas: HashSet<u32> = HashSet::new();
                let mut xdata_bytes: Vec<u8> = Vec::new();
                let mut xdata_cache: HashMap<Vec<u8>, u32> = HashMap::new();

                let mut intern_xdata = |record: Vec<u8>| -> Result<u32, LinkError> {
                    if let Err(msg) = crate::unwind::WindowsPdataGenerator::validate_xdata(&record) {
                        return Err(LinkError::new(
                            ErrorCode::InvalidSection,
                            format!("invalid synthesized Win64 UNWIND_INFO: {}", msg),
                        ));
                    }
                    if let Some(&rva) = xdata_cache.get(&record) {
                        return Ok(rva);
                    }
                    let off = (xdata_bytes.len() + 3) & !3;
                    xdata_bytes.resize(off, 0);
                    let rva = xdata_base_rva + off as u32;
                    xdata_bytes.extend_from_slice(&record);
                    xdata_cache.insert(record, rva);
                    Ok(rva)
                };

                // 1. Relocate and retain live input COFF `.pdata` entries (e.g. `adesh_runtime.lib`)
                for &(f_idx, s_idx) in &input_pdata_sections {
                    let sec = &objects[f_idx].sections[s_idx];
                    if !sec.is_live {
                        continue;
                    }
                    let mut entry_off = 0usize;
                    while entry_off + 12 <= sec.data.len() {
                        let mut begin_rva = u32::from_le_bytes(
                            sec.data[entry_off..entry_off + 4].try_into().unwrap(),
                        );
                        let mut end_rva = u32::from_le_bytes(
                            sec.data[entry_off + 4..entry_off + 8].try_into().unwrap(),
                        );
                        let mut unwind_rva = u32::from_le_bytes(
                            sec.data[entry_off + 8..entry_off + 12].try_into().unwrap(),
                        );
                        for reloc in &sec.relocations {
                            let r_off = reloc.offset as usize;
                            if r_off != entry_off && r_off != entry_off + 4 && r_off != entry_off + 8
                            {
                                continue;
                            }
                            let (sym_va_opt, _) = resolve_sym_va(
                                Some(f_idx),
                                reloc.symbol_index,
                                &reloc.symbol_name,
                            );
                            if let Some(sym_va) = sym_va_opt {
                                let rva = ((sym_va as i64)
                                    .wrapping_add(reloc.addend)
                                    .wrapping_sub(target.image_base as i64))
                                    as u32;
                                if r_off == entry_off {
                                    begin_rva = rva;
                                } else if r_off == entry_off + 4 {
                                    end_rva = rva;
                                } else {
                                    unwind_rva = rva;
                                }
                            }
                        }
                        if begin_rva != 0 && end_rva > begin_rva && unwind_rva != 0 {
                            if covered_begin_rvas.insert(begin_rva) {
                                pdata_entries.push(crate::unwind::PdataEntry {
                                    begin_rva,
                                    end_rva,
                                    unwind_info_rva: unwind_rva,
                                });
                            }
                        }
                        entry_off += 12;
                    }
                }

                // 2. Emit exact `.pdata`/`.xdata` for Adesh `.adob` functions, CRT startup stubs,
                //    and sized functions not already covered by input `.pdata`.
                for (f_idx, obj) in objects.iter().enumerate() {
                    for sym in &obj.symbols {
                        if !sym.is_defined || sym.sym_type != crate::symbol::SymbolType::Function {
                            continue;
                        }
                        let Some(s_idx) = sym.section_index else {
                            continue;
                        };
                        let Some((cat, _)) = sec_placement.get(&(f_idx, s_idx)) else {
                            continue;
                        };
                        if *cat != SectionCat::Text {
                            continue;
                        }
                        let Some(&sec_va) = sec_va_map.get(&(f_idx, s_idx)) else {
                            continue;
                        };
                        let func_va = sec_va + sym.value;
                        let func_rva = (func_va - target.image_base) as u32;
                        if covered_begin_rvas.contains(&func_rva) {
                            continue;
                        }
                        let fde_info = adesh_fde_info_map.get(&(f_idx, sym.name.clone()));
                        if sym.size == 0 && fde_info.is_none() {
                            // Unsized COFF leaf functions without input `.pdata` need no entry.
                            continue;
                        }
                        let func_len = sym.size as u32;
                        if func_len == 0 {
                            continue;
                        }

                        let xdata_record = if let Some(info) = fde_info
                            && info.win64_prolog.is_some()
                        {
                            let lsda_rva = info
                                .lsda_symbol
                                .as_deref()
                                .and_then(|lsym| resolve_sym_va(Some(f_idx), None, lsym).0)
                                .map(|va| (va - target.image_base) as u32);
                            let handler_rva = if lsda_rva.is_some() {
                                let pers = info
                                    .personality_symbol
                                    .as_deref()
                                    .unwrap_or("__adesh_win64_personality");
                                let va = resolve_sym_va(None, None, pers).0.ok_or_else(|| {
                                    LinkError::new(
                                        ErrorCode::UndefinedSymbol,
                                        format!(
                                            "Win64 unwind personality symbol `{}` could not be resolved",
                                            pers
                                        ),
                                    )
                                })?;
                                Some((va - target.image_base) as u32)
                            } else {
                                None
                            };
                            crate::unwind::WindowsPdataGenerator::build_xdata_from_prolog(
                                info.win64_prolog.as_ref(),
                                handler_rva,
                                lsda_rva,
                            )
                        } else if sym.name == "mainCRTStartup" {
                            crate::unwind::WindowsPdataGenerator::build_win64_entry_xdata()
                        } else if sym.name == "__adesh_windows_start" {
                            crate::unwind::WindowsPdataGenerator::build_win64_windows_start_xdata()
                        } else {
                            crate::unwind::WindowsPdataGenerator::build_default_xdata()
                        };

                        let unwind_info_rva = intern_xdata(xdata_record)?;
                        covered_begin_rvas.insert(func_rva);
                        pdata_entries.push(crate::unwind::PdataEntry {
                            begin_rva: func_rva,
                            end_rva: func_rva + func_len,
                            unwind_info_rva,
                        });
                    }
                }

                if !pdata_entries.is_empty() {
                    let pdata_bytes =
                        crate::unwind::WindowsPdataGenerator::build_pdata(pdata_entries);
                    let mut next_va = xdata_va;
                    if !xdata_bytes.is_empty() {
                        let xdata_size = xdata_bytes.len() as u64;
                        let mut xdata_sec = MergedSection::new(
                            ".xdata",
                            SectionKind::Rodata,
                            flags::READ | flags::ALLOC,
                            8,
                        );
                        xdata_sec.virtual_address = xdata_va;
                        xdata_sec.data = xdata_bytes;
                        xdata_sec.size = xdata_size;
                        merged_list.push(xdata_sec);
                        next_va = align_to(xdata_va + xdata_size, target.page_size.max(0x1000));
                    }

                    let mut pdata_sec = MergedSection::new(
                        ".pdata",
                        SectionKind::Rodata,
                        flags::READ | flags::ALLOC,
                        4,
                    );
                    pdata_sec.virtual_address = next_va;
                    pdata_sec.data = pdata_bytes;
                    pdata_sec.size = pdata_sec.data.len() as u64;
                    merged_list.push(pdata_sec);
                }
            }
        }

        // VA of the synthesized UD2 trap stub used for unresolvable
        // weak/internal call targets (PE x86_64 only).
        let trap_va: Option<u64> = trap_stub_off.and_then(|off| {
            merged_list
                .iter()
                .find(|s| s.name == ".text")
                .map(|s| s.virtual_address + off)
        });

        let mut reloc_warnings: Vec<String> = Vec::new();
        let mut warned_syms: HashMap<String, ()> = HashMap::new();
        let mut base_relocs: Vec<u32> = tls_directory_base_relocs;
        let mut base_relocs32: Vec<u32> = Vec::new();
        let tls_sec_va = cat_to_idx
            .get(&SectionCat::Tls)
            .map(|idx| merged_list[*idx].virtual_address);
        // Static TLS block bounds for the thread-pointer-relative math below:
        // Variant II (x86 / x86-64) places the thread pointer past
        // `align_to(p_memsz, p_align)`, so a variable at block offset `off`
        // lives at `%gs/%fs:(off - align_to(memsz, align))`.
        let tls_sec_memsz = cat_to_idx.get(&SectionCat::Tls).map(|idx| {
            merged_list[*idx]
                .size
                .max(merged_list[*idx].data.len() as u64)
        });
        let tls_sec_tp_span = cat_to_idx.get(&SectionCat::Tls).map(|idx| {
            let memsz = merged_list[*idx]
                .size
                .max(merged_list[*idx].data.len() as u64);
            let align = merged_list[*idx].alignment.max(1);
            align_to(memsz, align)
        });

        // Validate variable fields against original section boundaries, not merged padding.
        for object in objects {
            object.validate()?;
        }

        // 5. Apply Relocations to Merged Sections
        let handler = get_handler(target.arch);

        // Mach-O dynamic-linking bookkeeping: which symbols are imports (for
        // the relocation-shape check) and which absolute words need a dyld
        // rebase opcode (PIE).
        let macho_import_set: std::collections::HashSet<&str> = macho_imports
            .iter()
            .map(|imp| imp.raw_name.as_str())
            .collect();
        let mut macho_rebases: Vec<crate::macho::dynamic::MachORebaseTarget> = Vec::new();
        let mut macho_binds: Vec<crate::macho::dynamic::MachODataBind> = Vec::new();
        let mut macho_got_local_values: HashMap<MachoGotKey, u64> = HashMap::new();

        let elf_got_sec_va = elf_got_section_index.map(|idx| merged_list[idx].virtual_address);

        for merged in &mut merged_list {
            if merged.kind == SectionKind::Debug
                && target.format == crate::target::ObjectFormat::MachO
            {
                continue;
            }
            if merged.kind == SectionKind::Bss {
                if !merged.relocations.is_empty() {
                    // Relocations cannot be applied to zero-initialized
                    // storage (there is no file-backed data to patch). Keep
                    // skipping them, but say so instead of silently dropping
                    // them.
                    reloc_warnings.push(format!(
                        "{} relocation(s) in zero-initialized section `{}` were skipped: \
                         relocations cannot be applied to .bss/.tbss storage",
                        merged.relocations.len(),
                        merged.name
                    ));
                }
                continue;
            }
            if merged.data.is_empty() {
                continue;
            }

            // Byte ranges of this section already rewritten by a TLS
            // general-/local-dynamic relaxation below. The sequences'
            // own `__tls_get_addr` call-site relocations land inside
            // them and must not be applied afterwards.
            let mut relaxed_tls_ranges: Vec<(u64, u64)> = Vec::new();

            // RISC-V psABI label convention: a `PCREL_LO12_*` names the local
            // label at its paired `PCREL_HI20`'s AUIPC, so the low half must be
            // computed against the HI20's ultimate target, not the LO12's own
            // PC. Map each HI20's place VA -> (target VA + addend) so the LO12
            // pass below can resolve the pairing. HI20s whose target is not yet
            // resolvable are left out; their LO12s fall back to the generic path.
            let mut riscv_hi20_targets: HashMap<u64, u64> = HashMap::new();
            for r in &merged.relocations {
                if r.kind == RelocationKind::RiscvPcrelHi20 {
                    if let (Some(tva), _) =
                        resolve_sym_va(r.file_index, r.symbol_index, &r.symbol_name)
                    {
                        riscv_hi20_targets.insert(
                            merged.virtual_address + r.offset,
                            tva.wrapping_add(r.addend as u64),
                        );
                    }
                } else if r.kind == RelocationKind::RiscvGotHi20 {
                    let slot_va = if r.symbol_name == "_GLOBAL_OFFSET_TABLE_" {
                        elf_got_sec_va
                    } else {
                        elf_got_map.get(r.symbol_name.as_str()).copied()
                    };
                    if let Some(sva) = slot_va {
                        riscv_hi20_targets.insert(
                            merged.virtual_address + r.offset,
                            sva.wrapping_add(r.addend as u64),
                        );
                    }
                } else if r.kind == RelocationKind::RiscvTlsGotHi20 {
                    if let Some(sva) = elf_tls_got_map.get(r.symbol_name.as_str()).copied() {
                        riscv_hi20_targets.insert(
                            merged.virtual_address + r.offset,
                            sva.wrapping_add(r.addend as u64),
                        );
                    }
                } else if r.kind == RelocationKind::RiscvTlsGdHi20 {
                    if let Some(sva) = elf_tls_gd_map.get(r.symbol_name.as_str()).copied() {
                        riscv_hi20_targets.insert(
                            merged.virtual_address + r.offset,
                            sva.wrapping_add(r.addend as u64),
                        );
                    }
                }
            }

            let mut relocs = merged.relocations.iter();
            while let Some(reloc) = relocs.next() {
                if reloc.kind.is_riscv_uleb128() {
                    let sub = relocs.next().ok_or_else(|| {
                        LinkError::new(ErrorCode::InvalidObject, "orphan RISC-V ULEB128 relocation")
                    })?;
                    crate::relocation::validate_riscv_uleb128_pair(reloc, sub)?;
                    let resolve = |r: &crate::relocation::Relocation| {
                        resolve_sym_va(r.file_index, r.symbol_index, &r.symbol_name)
                            .0
                            .ok_or_else(|| {
                                LinkError::undefined_symbol(
                                    &r.symbol_name,
                                    &merged.name,
                                    None,
                                    Some(r.offset),
                                )
                            })
                    };
                    let set_va = resolve(reloc)?;
                    let sub_va = resolve(sub)?;
                    // Retain the exact input field span, even if another relocation
                    // has touched neighboring output bytes. Never scan merged padding.
                    let (section, original_offset) = merged
                        .input_sections
                        .iter()
                        .filter(|(f, _, base)| {
                            Some(*f) == reloc.file_index && *base <= reloc.offset
                        })
                        .filter_map(|(f, s, base)| {
                            objects
                                .get(*f)?
                                .sections
                                .get(*s)
                                .map(|section| (section, reloc.offset - base))
                        })
                        .find(|(section, offset)| *offset < section.data.len() as u64)
                        .ok_or_else(|| {
                            LinkError::new(
                                ErrorCode::InvalidObject,
                                "RISC-V ULEB128 has no original section field",
                            )
                        })?;
                    let span =
                        crate::relocation::riscv_uleb128_span(&section.data, original_offset)?;
                    let mut field = section.data
                        [original_offset as usize..original_offset as usize + span]
                        .to_vec();
                    let mut set_field = reloc.clone();
                    let mut sub_field = sub.clone();
                    set_field.offset = 0;
                    sub_field.offset = 0;
                    crate::relocation::apply_riscv_uleb128_pair(
                        &set_field, &sub_field, set_va, sub_va, &mut field,
                    )?;
                    let start = usize::try_from(reloc.offset).map_err(|_| {
                        LinkError::new(
                            ErrorCode::InvalidObject,
                            "RISC-V ULEB128 output offset overflow",
                        )
                    })?;
                    let destination = start
                        .checked_add(span)
                        .and_then(|end| merged.data.get_mut(start..end))
                        .ok_or_else(|| {
                            LinkError::new(
                                ErrorCode::InvalidObject,
                                "RISC-V ULEB128 exceeds output section",
                            )
                        })?;
                    destination.copy_from_slice(&field);
                    continue;
                }
                if relaxed_tls_ranges
                    .iter()
                    .any(|(lo, hi)| reloc.offset >= *lo && reloc.offset < *hi)
                {
                    // Absorbed by a relaxed TLS sequence rewrite.
                    continue;
                }
                let (sym_va_opt, sym_sec_base) =
                    resolve_sym_va(reloc.file_index, reloc.symbol_index, &reloc.symbol_name);
                // Paired RISC-V LO12 (archive label convention): the symbol is
                // the label at a HI20's AUIPC. Compute the low half against the
                // HI20's target and skip the generic S+A-P path, which would
                // subtract the LO12's own PC and mis-address the operand.
                if matches!(
                    reloc.kind,
                    RelocationKind::RiscvPcrelLo12I | RelocationKind::RiscvPcrelLo12S
                ) {
                    if let Some(label_va) = sym_va_opt {
                        if let Some(&hi20_target) = riscv_hi20_targets.get(&label_va) {
                            let off = usize::try_from(reloc.offset).map_err(|_| {
                                LinkError::new(
                                    ErrorCode::InvalidObject,
                                    "RISC-V PCREL_LO12 output offset overflow",
                                )
                            })?;
                            let slice = merged.data.get_mut(off..off + 4).ok_or_else(|| {
                                LinkError::new(
                                    ErrorCode::RelocationOverflow,
                                    format!(
                                        "RISC-V {} at {}+0x{:x} exceeds section size",
                                        reloc.kind.name(),
                                        merged.name,
                                        reloc.offset
                                    ),
                                )
                            })?;
                            crate::relocation::apply_riscv_pcrel_lo12_pair(
                                reloc,
                                label_va,
                                hi20_target,
                                slice,
                            )?;
                            continue;
                        }
                    }
                }
                // Only names that are unambiguously linker/compiler-internal may
                // be silently stubbed. The previous heuristic matched loose
                // substrings (`..`) and broad prefixes (`__float`, `__fix`,
                // `__gnu_`, `R_`, ...), which could swallow a genuine undefined
                // application symbol and zero-fill or trap-stub it instead of
                // reporting a link error.
                let is_weak_or_internal =
                    resolver.weak_undefined_symbols.contains(&reloc.symbol_name)
                        || is_known_internal_stub(&reloc.symbol_name);

                let place_va = merged.virtual_address + reloc.offset;

                let (sym_va, routed_to_trap) = match sym_va_opt {
                    Some(va) => (va, false),
                    None => {
                        if is_weak_or_internal {
                            // PC-relative references in executable sections are
                            // almost certainly call sites. Zero-filling them
                            // produces `call $+5`, which pushes a bogus return
                            // address and corrupts the frame. Route them to a
                            // UD2 trap stub instead so a live bad reference
                            // traps deterministically at the call site.
                            let is_code_ref = merged.is_executable()
                                && matches!(
                                    reloc.kind,
                                    RelocationKind::PcRelative32
                                        | RelocationKind::PcRelative64
                                        | RelocationKind::PltRelative32
                                );
                            match (is_code_ref, trap_va) {
                                (true, Some(tva)) => {
                                    if warned_syms.insert(reloc.symbol_name.clone(), ()).is_none() {
                                        reloc_warnings.push(format!(
                                            "unresolved weak/internal symbol `{}` (referenced from {}+0x{:x}) routed to trap stub; \
                                             if this symbol is expected to be called at runtime, the runtime library is missing a definition",
                                            reloc.symbol_name, merged.name, reloc.offset
                                        ));
                                    }
                                    (tva, true)
                                }
                                _ => {
                                    // Benign internal metadata symbols like MSVC type_info vftable (??_7type_info@@6B@)
                                    // resolve to NULL in data sections by design and must not emit a warning.
                                    let is_benign_rtti = reloc.symbol_name == "??_7type_info@@6B@"
                                        || reloc.symbol_name.starts_with("??_7type_info");
                                    if !is_benign_rtti
                                        && warned_syms
                                            .insert(reloc.symbol_name.clone(), ())
                                            .is_none()
                                    {
                                        reloc_warnings.push(format!(
                                            "unresolved weak/internal symbol `{}` (referenced from {}+0x{:x}) resolved to NULL",
                                            reloc.symbol_name, merged.name, reloc.offset
                                        ));
                                    }
                                    (0, false)
                                }
                            }
                        } else {
                            let mut dbg = LinkError::undefined_symbol(
                                &reloc.symbol_name,
                                &merged.name,
                                None,
                                Some(reloc.offset),
                            );
                            // Debug detail: why did precise resolution fail?
                            if let (Some(f_idx), Some(si)) = (reloc.file_index, reloc.symbol_index)
                                && let Some(obj) = objects.get(f_idx)
                                && let Some(sym) = obj.symbols.get(si)
                            {
                                dbg = dbg.with_note(format!(
                                    "precise lookup: file {} symbol {} -> defined={} local={} section={:?} placed={}",
                                    f_idx,
                                    si,
                                    sym.is_defined,
                                    sym.is_local(),
                                    sym.section_index,
                                    sym.section_index
                                        .and_then(|s| sec_va_map.get(&(f_idx, s)).copied())
                                        .is_some()
                                ));
                            }
                            return Err(dbg);
                        }
                    }
                };

                // Mach-O TLV descriptors (`__DATA,__thread_vars`): the 24-byte
                // records are `(thunk, key, offset)` and the static linker
                // rewrites the two relocated fields. Neither is an ordinary
                // absolute address:
                //   thunk  — dyld's bootstrap accessor lives in libSystem,
                //              whose load address is unknown here, so the word
                //              is zeroed and recorded in the bind stream.
                //   offset — the variable's offset within the per-thread block
                //              (`_tlv_get_addr() + offset`), so it is a
                //              constant that survives ASLR and must be written
                //              without a rebase entry.
                if target.format == crate::target::ObjectFormat::MachO
                    && merged.flags & crate::section::flags::TLV_TABLE != 0
                    && reloc.kind == RelocationKind::Absolute64
                {
                    let off = reloc.offset as usize;
                    if off + 8 > merged.data.len() {
                        return Err(LinkError::new(
                            ErrorCode::RelocationOverflow,
                            format!(
                                "TLV descriptor relocation at {}+0x{:x} exceeds section size",
                                merged.name, reloc.offset
                            ),
                        ));
                    }
                    if reloc.symbol_name.trim_start_matches('_') == "tlv_bootstrap" {
                        merged.data[off..off + 8].copy_from_slice(&0u64.to_le_bytes());
                        macho_binds.push(crate::macho::dynamic::MachODataBind {
                            va: place_va,
                            symbol: crate::macho::dynamic::TLV_BOOTSTRAP.to_string(),
                            addend: reloc.addend,
                        });
                        continue;
                    }
                    if let (Some(tls_base), Some(tls_memsz)) = (tls_sec_va, tls_sec_memsz)
                        && sym_va >= tls_base
                        && sym_va < tls_base + tls_memsz
                    {
                        let val = (sym_va as i64)
                            .wrapping_add(reloc.addend)
                            .wrapping_sub(tls_base as i64) as u64;
                        merged.data[off..off + 8].copy_from_slice(&val.to_le_bytes());
                        continue;
                    }
                }

                // Mach-O imports are reachable ONLY through their proxy
                // stub by a PC-relative call/jump. Any other relocation
                // shape is an address-of-import (or a data load): binding
                // it to the stub's address would be silent wrong code.
                if !routed_to_trap
                    && target.format == crate::target::ObjectFormat::MachO
                    && macho_import_set.contains(reloc.symbol_name.as_str())
                {
                    // Mach-O imported data symbols (or address-of-import references)
                    // in writable or allocated data sections bind via dyld non-lazy data binds.
                    if reloc.kind == RelocationKind::Absolute64 {
                        let off = reloc.offset as usize;
                        if off + 8 <= merged.data.len() {
                            let bind_name = macho_imports
                                .iter()
                                .find(|imp| imp.raw_name == reloc.symbol_name)
                                .map(|imp| imp.bind_name.clone())
                                .unwrap_or_else(|| format!("_{}", reloc.symbol_name));
                            merged.data[off..off + 8].copy_from_slice(&0u64.to_le_bytes());
                            macho_binds.push(crate::macho::dynamic::MachODataBind {
                                va: place_va,
                                symbol: bind_name,
                                addend: reloc.addend,
                            });
                            continue;
                        }
                    }

                    // `movq/call *sym@GOTPCREL(%rip)` and `adrp+ldr @GOTPAGE`
                    // must address the `__got` slot dyld binds, not the proxy
                    // stub. Binding GOT_LOAD to the stub VA loads stub machine
                    // code as a function pointer and SIGSEGVs on the indirect
                    // call (Phase 13 x86/arm64 runtime).
                    if matches!(
                        reloc.kind,
                        RelocationKind::GotRelative32
                            | RelocationKind::AArch64AdrGotPage
                            | RelocationKind::AArch64Ld64GotLo12
                    ) {
                        let slot_va = macho_imports
                            .iter()
                            .find(|imp| imp.raw_name == reloc.symbol_name)
                            .map(|imp| imp.got_slot_va)
                            .ok_or_else(|| {
                                LinkError::new(
                                    ErrorCode::InvalidSection,
                                    format!(
                                        "GOT-relative relocation against import `{}` in `{}` \
                                         has no allocated __got slot",
                                        reloc.symbol_name, merged.name
                                    ),
                                )
                            })?;
                        handler.apply(reloc, place_va, slot_va, reloc.addend, &mut merged.data)?;
                        continue;
                    }

                    let is_call = matches!(
                        reloc.kind,
                        RelocationKind::PcRelative32
                            | RelocationKind::PcRelative64
                            | RelocationKind::PltRelative32
                            | RelocationKind::AArch64Call26
                            | RelocationKind::AArch64Adrp
                            | RelocationKind::AArch64AddLo12
                            | RelocationKind::AArch64LdSt64Lo12
                    );
                    if !is_call {
                        return Err(LinkError::new(
                            ErrorCode::UnsupportedRelocation,
                            format!(
                                "imported symbol `{}` is referenced by an unsupported non-call relocation ({:?}) in \
                                 `{}` — Mach-O imports bind through lazy stubs, GOT references, or pointer binds",
                                reloc.symbol_name, reloc.kind, merged.name
                            ),
                        ));
                    }
                }

                // Mach-O GOT-relative references to defined or weak-undefined
                // symbols use `__got_local` slots (rebased by dyld when
                // non-zero, or zeroed for weak-undefined NULL).
                if target.format == crate::target::ObjectFormat::MachO
                    && matches!(
                        reloc.kind,
                        RelocationKind::GotRelative32
                            | RelocationKind::AArch64AdrGotPage
                            | RelocationKind::AArch64Ld64GotLo12
                    )
                {
                    let key = macho_got_key(reloc);
                    let slot_va = macho_got_local_map.get(&key).copied().ok_or_else(|| {
                        LinkError::new(
                            ErrorCode::InvalidSection,
                            format!(
                                "Mach-O local GOT relocation against `{}` in `{}` has no allocated __got_local slot",
                                reloc.symbol_name, merged.name
                            ),
                        )
                    })?;
                    macho_got_local_values.insert(key, sym_va);
                    handler.apply(reloc, place_va, slot_va, reloc.addend, &mut merged.data)?;
                    continue;
                }

                // PE image-relative (ADDR32NB -> RVA) and section-relative
                // (SECREL32) relocations need bases the generic handler does
                // not know (image base / target section base). Apply them
                // here directly.
                if target.format == crate::target::ObjectFormat::Pe {
                    match reloc.kind {
                        RelocationKind::ImageRelative32 => {
                            let val = (sym_va as i64)
                                .wrapping_add(reloc.addend)
                                .wrapping_sub(target.image_base as i64)
                                as u32;
                            let off = usize::try_from(reloc.offset).unwrap_or(usize::MAX);
                            if off.checked_add(4).is_some_and(|e| e <= merged.data.len()) {
                                merged.data[off..off + 4].copy_from_slice(&val.to_le_bytes());
                                continue;
                            }
                            return Err(LinkError::new(
                                crate::error::ErrorCode::RelocationOverflow,
                                format!(
                                    "image-relative relocation at {}+0x{:x} exceeds section size",
                                    merged.name, reloc.offset
                                ),
                            ));
                        }
                        RelocationKind::SectionRelative32
                        | RelocationKind::TlsLocalExec
                        | RelocationKind::TlsInitialExec => {
                            // Offset within the merged output section. An
                            // unknown base cannot safely become an RVA.
                            let sec_base = sym_sec_base.or(tls_sec_va).ok_or_else(|| {
                                LinkError::new(
                                    ErrorCode::InvalidSection,
                                    format!(
                                        "section-relative / TLS relocation against `{}` has no output section",
                                        reloc.symbol_name
                                    ),
                                )
                            })?;
                            let val = (sym_va as i64)
                                .wrapping_sub(sec_base as i64)
                                .wrapping_add(reloc.addend)
                                as u32;
                            let off = usize::try_from(reloc.offset).unwrap_or(usize::MAX);
                            if off.checked_add(4).is_some_and(|e| e <= merged.data.len()) {
                                merged.data[off..off + 4].copy_from_slice(&val.to_le_bytes());
                                continue;
                            }
                            return Err(LinkError::new(
                                crate::error::ErrorCode::RelocationOverflow,
                                format!(
                                    "section-relative / TLS relocation at {}+0x{:x} exceeds section size",
                                    merged.name, reloc.offset
                                ),
                            ));
                        }
                        _ => {}
                    }
                }

                // ELF TLS relocations: thread-pointer-relative math the
                // generic handler does not know. The static TLS block runs
                // BELOW the thread pointer (x86-64: %fs addresses the block's
                // end), so a variable at block offset `off` lives at
                // `%fs:(off - memsz)` — a negative displacement.
                if target.format == crate::target::ObjectFormat::Elf {
                    match reloc.kind {
                        RelocationKind::GotRelative32 => {
                            if target.arch != crate::target::Arch::X86_64 {
                                return Err(LinkError::new(
                                    ErrorCode::UnsupportedRelocation,
                                    format!(
                                        "GOT-relative relocation against `{}` in `{}` is only \
                                         implemented for x86-64 ELF",
                                        reloc.symbol_name, merged.name
                                    ),
                                ));
                            }
                            let slot_va = match elf_got_map.get(reloc.symbol_name.as_str()).copied()
                            {
                                Some(va) => va,
                                None => {
                                    return Err(LinkError::new(
                                        ErrorCode::InvalidSection,
                                        format!(
                                            "GOT-relative relocation against `{}` in `{}` has \
                                             no allocated GOT slot",
                                            reloc.symbol_name, merged.name
                                        ),
                                    ));
                                }
                            };
                            let value = (slot_va as i64)
                                .wrapping_add(reloc.addend)
                                .wrapping_sub(place_va as i64);
                            if value < i32::MIN as i64 || value > i32::MAX as i64 {
                                return Err(LinkError::new(
                                    ErrorCode::RelocationOverflow,
                                    format!(
                                        "GOT-relative relocation against `{}` in `{}` is out \
                                         of range: {}",
                                        reloc.symbol_name, merged.name, value
                                    ),
                                ));
                            }
                            let off = usize::try_from(reloc.offset).unwrap_or(usize::MAX);
                            if off.checked_add(4).is_none_or(|e| e > merged.data.len()) {
                                return Err(LinkError::new(
                                    ErrorCode::RelocationOverflow,
                                    format!(
                                        "GOT-relative relocation at {}+0x{:x} exceeds section \
                                         size",
                                        merged.name, reloc.offset
                                    ),
                                ));
                            }
                            merged.data[off..off + 4]
                                .copy_from_slice(&(value as i32).to_le_bytes());
                            continue;
                        }
                        RelocationKind::X86GotPc32 => {
                            if target.arch != crate::target::Arch::X86 {
                                return Err(LinkError::new(
                                    ErrorCode::UnsupportedRelocation,
                                    format!(
                                        "R_386_GOTPC relocation against `{}` in `{}` is only \
                                         valid for IA-32 (i686) ELF",
                                        reloc.symbol_name, merged.name
                                    ),
                                ));
                            }
                            let got_va = elf_got_sec_va.ok_or_else(|| {
                                LinkError::new(
                                    ErrorCode::InvalidSection,
                                    format!(
                                        "R_386_GOTPC relocation against `{}` in `{}` has no .got section",
                                        reloc.symbol_name, merged.name
                                    ),
                                )
                            })?;
                            let value = (got_va as i64)
                                .wrapping_add(reloc.addend)
                                .wrapping_sub(place_va as i64);
                            if value < i32::MIN as i64 || value > i32::MAX as i64 {
                                return Err(LinkError::relocation_overflow(
                                    reloc.kind.name(),
                                    &reloc.symbol_name,
                                    value,
                                    i32::MIN as i64,
                                    i32::MAX as i64,
                                    Some(&merged.name),
                                    Some(reloc.offset),
                                ));
                            }
                            let off = usize::try_from(reloc.offset).unwrap_or(usize::MAX);
                            if off.checked_add(4).is_none_or(|e| e > merged.data.len()) {
                                return Err(LinkError::new(
                                    ErrorCode::RelocationOverflow,
                                    format!(
                                        "R_386_GOTPC relocation at {}+0x{:x} exceeds section size",
                                        merged.name, reloc.offset
                                    ),
                                ));
                            }
                            merged.data[off..off + 4]
                                .copy_from_slice(&(value as i32).to_le_bytes());
                            continue;
                        }
                        RelocationKind::X86GotOff32 => {
                            if target.arch != crate::target::Arch::X86 {
                                return Err(LinkError::new(
                                    ErrorCode::UnsupportedRelocation,
                                    format!(
                                        "R_386_GOTOFF relocation against `{}` in `{}` is only \
                                         valid for IA-32 (i686) ELF",
                                        reloc.symbol_name, merged.name
                                    ),
                                ));
                            }
                            let got_va = elf_got_sec_va.ok_or_else(|| {
                                LinkError::new(
                                    ErrorCode::InvalidSection,
                                    format!(
                                        "R_386_GOTOFF relocation against `{}` in `{}` has no .got section",
                                        reloc.symbol_name, merged.name
                                    ),
                                )
                            })?;
                            let sym_va = sym_va_opt.ok_or_else(|| {
                                LinkError::new(
                                    ErrorCode::UndefinedSymbol,
                                    format!(
                                        "R_386_GOTOFF relocation references undefined symbol `{}`",
                                        reloc.symbol_name
                                    ),
                                )
                            })?;
                            let value = (sym_va as i64)
                                .wrapping_add(reloc.addend)
                                .wrapping_sub(got_va as i64);
                            if value < i32::MIN as i64 || value > i32::MAX as i64 {
                                return Err(LinkError::relocation_overflow(
                                    reloc.kind.name(),
                                    &reloc.symbol_name,
                                    value,
                                    i32::MIN as i64,
                                    i32::MAX as i64,
                                    Some(&merged.name),
                                    Some(reloc.offset),
                                ));
                            }
                            let off = usize::try_from(reloc.offset).unwrap_or(usize::MAX);
                            if off.checked_add(4).is_none_or(|e| e > merged.data.len()) {
                                return Err(LinkError::new(
                                    ErrorCode::RelocationOverflow,
                                    format!(
                                        "R_386_GOTOFF relocation at {}+0x{:x} exceeds section size",
                                        merged.name, reloc.offset
                                    ),
                                ));
                            }
                            merged.data[off..off + 4]
                                .copy_from_slice(&(value as i32).to_le_bytes());
                            continue;
                        }
                        RelocationKind::X86Got32 => {
                            if target.arch != crate::target::Arch::X86 {
                                return Err(LinkError::new(
                                    ErrorCode::UnsupportedRelocation,
                                    format!(
                                        "R_386_GOT32 relocation against `{}` in `{}` is only \
                                         valid for IA-32 (i686) ELF",
                                        reloc.symbol_name, merged.name
                                    ),
                                ));
                            }
                            let got_va = elf_got_sec_va.ok_or_else(|| {
                                LinkError::new(
                                    ErrorCode::InvalidSection,
                                    format!(
                                        "R_386_GOT32 relocation against `{}` in `{}` has no .got section",
                                        reloc.symbol_name, merged.name
                                    ),
                                )
                            })?;
                            let slot_va = elf_got_map
                                .get(reloc.symbol_name.as_str())
                                .copied()
                                .ok_or_else(|| {
                                    LinkError::new(
                                        ErrorCode::InvalidSection,
                                        format!(
                                            "R_386_GOT32 relocation against `{}` in `{}` has no allocated GOT slot",
                                            reloc.symbol_name, merged.name
                                        ),
                                    )
                                })?;
                            let off = usize::try_from(reloc.offset).unwrap_or(usize::MAX);
                            if off.checked_add(4).is_none_or(|e| e > merged.data.len()) {
                                return Err(LinkError::new(
                                    ErrorCode::RelocationOverflow,
                                    format!(
                                        "R_386_GOT32 relocation at {}+0x{:x} exceeds section size",
                                        merged.name, reloc.offset
                                    ),
                                ));
                            }
                            // R_386_GOT32X without a base register uses ModR/M mod=00, rm=101 (disp32);
                            // with a base register (standard PIC `%ebx`), it uses `G + A - GOT`.
                            let no_base_reg = off >= 1 && (merged.data[off - 1] & 0xC7) == 0x05;
                            let value = if no_base_reg {
                                (slot_va as i64).wrapping_add(reloc.addend)
                            } else {
                                (slot_va as i64)
                                    .wrapping_add(reloc.addend)
                                    .wrapping_sub(got_va as i64)
                            };
                            let (min_v, max_v) = if no_base_reg {
                                (i32::MIN as i64, u32::MAX as i64)
                            } else {
                                (i32::MIN as i64, i32::MAX as i64)
                            };
                            if value < min_v || value > max_v {
                                return Err(LinkError::relocation_overflow(
                                    reloc.kind.name(),
                                    &reloc.symbol_name,
                                    value,
                                    min_v,
                                    max_v,
                                    Some(&merged.name),
                                    Some(reloc.offset),
                                ));
                            }
                            merged.data[off..off + 4]
                                .copy_from_slice(&(value as u32).to_le_bytes());
                            continue;
                        }
                        RelocationKind::AArch64AdrGotPage => {
                            let slot_va = match elf_got_map.get(reloc.symbol_name.as_str()).copied()
                            {
                                Some(va) => va,
                                None => {
                                    return Err(LinkError::new(
                                        ErrorCode::InvalidSection,
                                        format!(
                                            "AArch64 ADR_GOT_PAGE relocation against `{}` in `{}` \
                                             has no allocated GOT slot",
                                            reloc.symbol_name, merged.name
                                        ),
                                    ));
                                }
                            };
                            let page_sym = (slot_va as i128 + reloc.addend as i128) >> 12;
                            let page_place = (place_va as i128) >> 12;
                            let page_offset = page_sym - page_place;
                            const MIN_PAGE21: i128 = -(1i128 << 20);
                            const MAX_PAGE21: i128 = (1i128 << 20) - 1;
                            if !(MIN_PAGE21..=MAX_PAGE21).contains(&page_offset) {
                                return Err(LinkError::relocation_overflow(
                                    reloc.kind.name(),
                                    &reloc.symbol_name,
                                    page_offset.clamp(i64::MIN as i128, i64::MAX as i128) as i64,
                                    MIN_PAGE21 as i64,
                                    MAX_PAGE21 as i64,
                                    None,
                                    Some(reloc.offset),
                                ));
                            }
                            let imm21 = (page_offset & 0x1F_FFFF) as u32;
                            let immlo = (imm21 & 0x3) << 29;
                            let immhi = ((imm21 >> 2) & 0x7FFFF) << 5;
                            let off = usize::try_from(reloc.offset).unwrap_or(usize::MAX);
                            if off.checked_add(4).is_none_or(|e| e > merged.data.len()) {
                                return Err(LinkError::new(
                                    ErrorCode::RelocationOverflow,
                                    format!(
                                        "AArch64 ADR_GOT_PAGE relocation at {}+0x{:x} exceeds section size",
                                        merged.name, reloc.offset
                                    ),
                                ));
                            }
                            let mut insn =
                                u32::from_le_bytes(merged.data[off..off + 4].try_into().unwrap());
                            insn = (insn & !((0x3 << 29) | (0x7FFFF << 5))) | immlo | immhi;
                            merged.data[off..off + 4].copy_from_slice(&insn.to_le_bytes());
                            continue;
                        }
                        RelocationKind::AArch64Ld64GotLo12 => {
                            let slot_va = match elf_got_map.get(reloc.symbol_name.as_str()).copied()
                            {
                                Some(va) => va,
                                None => {
                                    return Err(LinkError::new(
                                        ErrorCode::InvalidSection,
                                        format!(
                                            "AArch64 LD64_GOT_LO12 relocation against `{}` in `{}` \
                                             has no allocated GOT slot",
                                            reloc.symbol_name, merged.name
                                        ),
                                    ));
                                }
                            };
                            let pageoff = (slot_va as i64 + reloc.addend) & 0xFFF;
                            if (pageoff & 0x7) != 0 {
                                return Err(LinkError::new(
                                    ErrorCode::RelocationOverflow,
                                    format!(
                                        "AArch64 LD64_GOT_LO12 target offset 0x{:x} is not 8-byte aligned",
                                        pageoff
                                    ),
                                ));
                            }
                            let imm12 = ((pageoff >> 3) as u32) << 10;
                            let off = usize::try_from(reloc.offset).unwrap_or(usize::MAX);
                            if off.checked_add(4).is_none_or(|e| e > merged.data.len()) {
                                return Err(LinkError::new(
                                    ErrorCode::RelocationOverflow,
                                    format!(
                                        "AArch64 LD64_GOT_LO12 relocation at {}+0x{:x} exceeds section size",
                                        merged.name, reloc.offset
                                    ),
                                ));
                            }
                            let mut insn =
                                u32::from_le_bytes(merged.data[off..off + 4].try_into().unwrap());
                            insn = (insn & !(0xFFF << 10)) | imm12;
                            merged.data[off..off + 4].copy_from_slice(&insn.to_le_bytes());
                            continue;
                        }
                        RelocationKind::RiscvGotHi20 => {
                            let slot_va = if reloc.symbol_name == "_GLOBAL_OFFSET_TABLE_" {
                                elf_got_sec_va
                            } else {
                                elf_got_map.get(reloc.symbol_name.as_str()).copied()
                            };
                            let slot_va = match slot_va {
                                Some(va) => va,
                                None => {
                                    return Err(LinkError::new(
                                        ErrorCode::InvalidSection,
                                        format!(
                                            "RISC-V GOT_HI20 relocation against `{}` in `{}` has no allocated GOT slot",
                                            reloc.symbol_name, merged.name
                                        ),
                                    ));
                                }
                            };
                            let val = slot_va as i128 + reloc.addend as i128 - place_va as i128;
                            const MIN_HI20_VAL: i128 = -(1i128 << 31) - 0x800;
                            const MAX_HI20_VAL: i128 = (1i128 << 31) - 0x801;
                            if !(MIN_HI20_VAL..=MAX_HI20_VAL).contains(&val) {
                                return Err(LinkError::relocation_overflow(
                                    reloc.kind.name(),
                                    &reloc.symbol_name,
                                    val.clamp(i64::MIN as i128, i64::MAX as i128) as i64,
                                    MIN_HI20_VAL as i64,
                                    MAX_HI20_VAL as i64,
                                    None,
                                    Some(reloc.offset),
                                ));
                            }
                            let hi20 = (((val + 0x800) >> 12) & 0xFFFFF) as u32;
                            let off = usize::try_from(reloc.offset).unwrap_or(usize::MAX);
                            if off.checked_add(4).is_none_or(|e| e > merged.data.len()) {
                                return Err(LinkError::new(
                                    ErrorCode::RelocationOverflow,
                                    format!(
                                        "RISC-V GOT_HI20 relocation at {}+0x{:x} exceeds section size",
                                        merged.name, reloc.offset
                                    ),
                                ));
                            }
                            let mut insn =
                                u32::from_le_bytes(merged.data[off..off + 4].try_into().unwrap());
                            insn = (insn & 0xFFF) | (hi20 << 12);
                            merged.data[off..off + 4].copy_from_slice(&insn.to_le_bytes());
                            continue;
                        }
                        RelocationKind::RiscvTlsGotHi20 => {
                            let slot_va = match elf_tls_got_map
                                .get(reloc.symbol_name.as_str())
                                .copied()
                            {
                                Some(va) => va,
                                None => {
                                    return Err(LinkError::new(
                                        ErrorCode::InvalidSection,
                                        format!(
                                            "RISC-V TLS_GOT_HI20 relocation against `{}` in `{}` has no allocated TLS GOT slot",
                                            reloc.symbol_name, merged.name
                                        ),
                                    ));
                                }
                            };
                            let val = slot_va as i128 + reloc.addend as i128 - place_va as i128;
                            const MIN_HI20_VAL: i128 = -(1i128 << 31) - 0x800;
                            const MAX_HI20_VAL: i128 = (1i128 << 31) - 0x801;
                            if !(MIN_HI20_VAL..=MAX_HI20_VAL).contains(&val) {
                                return Err(LinkError::relocation_overflow(
                                    reloc.kind.name(),
                                    &reloc.symbol_name,
                                    val.clamp(i64::MIN as i128, i64::MAX as i128) as i64,
                                    MIN_HI20_VAL as i64,
                                    MAX_HI20_VAL as i64,
                                    None,
                                    Some(reloc.offset),
                                ));
                            }
                            let hi20 = (((val + 0x800) >> 12) & 0xFFFFF) as u32;
                            let off = usize::try_from(reloc.offset).unwrap_or(usize::MAX);
                            if off.checked_add(4).is_none_or(|e| e > merged.data.len()) {
                                return Err(LinkError::new(
                                    ErrorCode::RelocationOverflow,
                                    format!(
                                        "RISC-V TLS_GOT_HI20 relocation at {}+0x{:x} exceeds section size",
                                        merged.name, reloc.offset
                                    ),
                                ));
                            }
                            let mut insn =
                                u32::from_le_bytes(merged.data[off..off + 4].try_into().unwrap());
                            insn = (insn & 0xFFF) | (hi20 << 12);
                            merged.data[off..off + 4].copy_from_slice(&insn.to_le_bytes());
                            continue;
                        }
                        RelocationKind::RiscvTlsGdHi20 => {
                            let slot_va = match elf_tls_gd_map
                                .get(reloc.symbol_name.as_str())
                                .copied()
                            {
                                Some(va) => va,
                                None => {
                                    return Err(LinkError::new(
                                        ErrorCode::InvalidSection,
                                        format!(
                                            "RISC-V TLS_GD_HI20 relocation against `{}` in `{}` has no allocated TLS GD GOT slot",
                                            reloc.symbol_name, merged.name
                                        ),
                                    ));
                                }
                            };
                            let val = slot_va as i128 + reloc.addend as i128 - place_va as i128;
                            const MIN_HI20_VAL: i128 = -(1i128 << 31) - 0x800;
                            const MAX_HI20_VAL: i128 = (1i128 << 31) - 0x801;
                            if !(MIN_HI20_VAL..=MAX_HI20_VAL).contains(&val) {
                                return Err(LinkError::relocation_overflow(
                                    reloc.kind.name(),
                                    &reloc.symbol_name,
                                    val.clamp(i64::MIN as i128, i64::MAX as i128) as i64,
                                    MIN_HI20_VAL as i64,
                                    MAX_HI20_VAL as i64,
                                    None,
                                    Some(reloc.offset),
                                ));
                            }
                            let hi20 = (((val + 0x800) >> 12) & 0xFFFFF) as u32;
                            let off = usize::try_from(reloc.offset).unwrap_or(usize::MAX);
                            if off.checked_add(4).is_none_or(|e| e > merged.data.len()) {
                                return Err(LinkError::new(
                                    ErrorCode::RelocationOverflow,
                                    format!(
                                        "RISC-V TLS_GD_HI20 relocation at {}+0x{:x} exceeds section size",
                                        merged.name, reloc.offset
                                    ),
                                ));
                            }
                            let mut insn =
                                u32::from_le_bytes(merged.data[off..off + 4].try_into().unwrap());
                            insn = (insn & 0xFFF) | (hi20 << 12);
                            merged.data[off..off + 4].copy_from_slice(&insn.to_le_bytes());
                            continue;
                        }
                        RelocationKind::AArch64TlsDescAdrPage21
                        | RelocationKind::AArch64TlsDescLd64Lo12
                        | RelocationKind::AArch64TlsDescAddLo12
                        | RelocationKind::AArch64TlsDescCall => {
                            let (tls_start, tls_memsz) = match (tls_sec_va, tls_sec_memsz) {
                                (Some(start), Some(size)) => (start, size),
                                _ => {
                                    return Err(LinkError::new(
                                        ErrorCode::InvalidSection,
                                        format!(
                                            "AArch64 TLSDESC relocation against `{}` in `{}` \
                                             but the link has no TLS section",
                                            reloc.symbol_name, merged.name
                                        ),
                                    ));
                                }
                            };
                            if sym_va < tls_start || sym_va >= tls_start + tls_memsz {
                                return Err(LinkError::new(
                                    ErrorCode::InvalidSection,
                                    format!(
                                        "AArch64 TLSDESC relocation against `{}` resolves to \
                                         0x{:x}, outside TLS block 0x{:x}..0x{:x}",
                                        reloc.symbol_name,
                                        sym_va,
                                        tls_start,
                                        tls_start + tls_memsz
                                    ),
                                ));
                            }
                            // AArch64 TLS Variant I: TPIDR_EL0 points to 16-byte TCB.
                            // Static TLS block starts at TPIDR_EL0 + 16.
                            let tpoff = (sym_va as i64 - tls_start as i64) + 16 + reloc.addend;
                            let off = usize::try_from(reloc.offset).unwrap_or(usize::MAX);
                            if off.checked_add(4).is_none_or(|e| e > merged.data.len()) {
                                return Err(LinkError::new(
                                    ErrorCode::RelocationOverflow,
                                    format!(
                                        "AArch64 TLSDESC relocation at {}+0x{:x} exceeds section size",
                                        merged.name, reloc.offset
                                    ),
                                ));
                            }
                            // Relax TLSDESC to Local-Exec:
                            // 562 (ADR_PAGE21): movz x0, #((tpoff >> 16) & 0xffff), lsl #16
                            // 563 (LD64_LO12):  movk x0, #(tpoff & 0xffff)
                            // 564 (ADD_LO12):   nop
                            // 569 (CALL):       nop
                            let insn: u32 = match reloc.kind {
                                RelocationKind::AArch64TlsDescAdrPage21 => {
                                    0xd2a00000u32 | (((tpoff >> 16) as u32 & 0xffff) << 5)
                                }
                                RelocationKind::AArch64TlsDescLd64Lo12 => {
                                    0xf2800000u32 | ((tpoff as u32 & 0xffff) << 5)
                                }
                                RelocationKind::AArch64TlsDescAddLo12
                                | RelocationKind::AArch64TlsDescCall => 0xd503201fu32,
                                _ => unreachable!(),
                            };
                            merged.data[off..off + 4].copy_from_slice(&insn.to_le_bytes());
                            continue;
                        }
                        RelocationKind::TlsGeneralDynamic => {
                            // Both x86-64 dynamic TLS forms land here (the ELF
                            // reader folds R_X86_64_TLSLD into this kind) and
                            // are relaxed like GNU ld relaxes them for
                            // executables — TLS variables defined in this
                            // link always live in the executable's static
                            // TLS block, so no DTV / __tls_get_addr runtime
                            // is needed.
                            //
                            // General-dynamic (16 bytes):
                            //   66 48 8d 3d <rel32>  leaq x@tlsgd(%rip),%rdi
                            //   66 66 48 e8 <rel32>  call __tls_get_addr@plt
                            // becomes the length-preserving initial-exec
                            // form
                            //   48 8b 05 <rel32>     movq x@gottpoff(%rip),%rax
                            //   64 48 03 04 25 00 00 00 00  addq %fs:0,%rax
                            // whose GOT slot math is the initial-exec arm's
                            // above (slot content filled after this loop —
                            // TPOFF64 for dynamic images).
                            //
                            // Local-dynamic (12 bytes):
                            //   48 8d 3d <rel32>     leaq x@tlsld(%rip),%rdi
                            //   e8 <rel32>           call __tls_get_addr@plt
                            // becomes the local-exec form
                            //   64 48 8b 04 25 00 00 00 00  movq %fs:0,%rax
                            //   0f 1f 00                     nopl (%rax)
                            // putting the thread pointer in %rax; the paired
                            // DTPOFF32 disp32s (handled below as
                            // TlsDtpOff32) then carry full thread-pointer-
                            // relative offsets.
                            //
                            // The sequences' own `__tls_get_addr` call-site
                            // relocations fall inside the rewritten ranges
                            // and are absorbed by the range skip above.
                            // Anything else stays loud: a variable not
                            // defined in this link, an unusual addend, or a
                            // nonstandard code shape would need a DTV and
                            // __tls_get_addr runtime this toolchain does
                            // not provide.
                            let addend_ok = reloc.addend == 0 || reloc.addend == -4;
                            let gd_start = (reloc.offset as usize).wrapping_sub(4);
                            let gd_ok = addend_ok
                                && reloc.offset >= 4
                                && sym_va_opt.is_some()
                                && gd_start + 16 <= merged.data.len()
                                && merged.data[gd_start..gd_start + 4] == [0x66, 0x48, 0x8d, 0x3d]
                                && merged.data[gd_start + 8..gd_start + 12]
                                    == [0x66, 0x66, 0x48, 0xe8];
                            let ld_start = (reloc.offset as usize).wrapping_sub(3);
                            let ld_ok = addend_ok
                                && reloc.offset >= 3
                                && sym_va_opt.is_some()
                                && ld_start + 12 <= merged.data.len()
                                && merged.data[ld_start..ld_start + 3] == [0x48, 0x8d, 0x3d]
                                && merged.data[ld_start + 7] == 0xe8;
                            if gd_ok {
                                let slot_va = match elf_tls_got_map
                                    .get(reloc.symbol_name.as_str())
                                    .copied()
                                {
                                    Some(va) => va,
                                    None => {
                                        return Err(LinkError::new(
                                            ErrorCode::InvalidSection,
                                            format!(
                                                "general-dynamic TLS relocation against `{}` in \
                                                 `{}` has no GOT slot",
                                                reloc.symbol_name, merged.name
                                            ),
                                        ));
                                    }
                                };
                                merged.data[gd_start..gd_start + 3]
                                    .copy_from_slice(&[0x48, 0x8b, 0x05]);
                                // The movq's disp32 sits one byte before the
                                // original leaq's; its RIP base is the movq's
                                // end (7 bytes from the sequence start).
                                let disp = (slot_va as i64)
                                    .wrapping_sub(place_va as i64)
                                    .wrapping_sub(3);
                                let disp_off = gd_start + 3;
                                merged.data[disp_off..disp_off + 4]
                                    .copy_from_slice(&(disp as i32).to_le_bytes());
                                merged.data[gd_start + 7..gd_start + 16].copy_from_slice(&[
                                    0x64, 0x48, 0x03, 0x04, 0x25, 0x00, 0x00, 0x00, 0x00,
                                ]);
                                relaxed_tls_ranges.push((gd_start as u64, (gd_start + 16) as u64));
                                continue;
                            }
                            if ld_ok {
                                merged.data[ld_start..ld_start + 9].copy_from_slice(&[
                                    0x64, 0x48, 0x8b, 0x04, 0x25, 0x00, 0x00, 0x00, 0x00,
                                ]);
                                merged.data[ld_start + 9..ld_start + 12]
                                    .copy_from_slice(&[0x0f, 0x1f, 0x00]);
                                relaxed_tls_ranges.push((ld_start as u64, (ld_start + 12) as u64));
                                continue;
                            }
                            return Err(LinkError::new(
                                ErrorCode::UnsupportedRelocation,
                                format!(
                                    "general-/local-dynamic TLS relocation (R_X86_64_TLSGD or \
                                     R_X86_64_TLSLD) against `{}` in `{}` cannot be relaxed to \
                                     the initial-/local-exec form: the variable is not defined \
                                     in this link, the addend is unusual, or the code is not \
                                     the standard compiler-emitted sequence; serving it as-is \
                                     requires a DTV and __tls_get_addr runtime that the Adesh \
                                     toolchain does not provide",
                                    reloc.symbol_name, merged.name
                                ),
                            ));
                        }
                        RelocationKind::TlsLocalExec => {
                            let (tls_start, tls_memsz) = match (tls_sec_va, tls_sec_memsz) {
                                (Some(start), Some(size)) => (start, size),
                                _ => {
                                    return Err(LinkError::new(
                                        ErrorCode::InvalidSection,
                                        format!(
                                            "local-exec TLS relocation against `{}` in `{}` \
                                                 but the link has no TLS section",
                                            reloc.symbol_name, merged.name
                                        ),
                                    ));
                                }
                            };
                            // The referenced symbol must live inside the TLS
                            // block; anything else is silent wrong code.
                            if sym_va < tls_start || sym_va >= tls_start + tls_memsz {
                                return Err(LinkError::new(
                                    ErrorCode::InvalidSection,
                                    format!(
                                        "local-exec TLS relocation against `{}` resolves to \
                                         0x{:x}, outside the TLS block 0x{:x}..0x{:x}",
                                        reloc.symbol_name,
                                        sym_va,
                                        tls_start,
                                        tls_start + tls_memsz
                                    ),
                                ));
                            }
                            let tp_off = match target.arch {
                                // Variant I: TPIDR_EL0 → 16-byte TCB, then TLS.
                                crate::target::Arch::AArch64 => {
                                    (sym_va as i64 - tls_start as i64) + 16
                                }
                                crate::target::Arch::Riscv64 | crate::target::Arch::Riscv32 => {
                                    sym_va as i64 - tls_start as i64
                                }
                                // Variant II (x86 / x86-64): TP points past the TLS block.
                                // On i686 (`Arch::X86`), our static TLS prologue aligns the TP
                                // span to `align_to(memsz, align)`; x86-64 static local-exec
                                // uses `tls_memsz` directly to match `__tls_end`.
                                crate::target::Arch::X86 => {
                                    let span = tls_sec_tp_span.unwrap_or(tls_memsz);
                                    (sym_va as i64 - tls_start as i64) - span as i64
                                }
                                _ => (sym_va as i64 - tls_start as i64) - tls_memsz as i64,
                            };
                            let val = tp_off.wrapping_add(reloc.addend);
                            let off = usize::try_from(reloc.offset).unwrap_or(usize::MAX);
                            if off.checked_add(4).is_none_or(|e| e > merged.data.len()) {
                                return Err(LinkError::new(
                                    crate::error::ErrorCode::RelocationOverflow,
                                    format!(
                                        "local-exec TLS relocation at {}+0x{:x} exceeds section size",
                                        merged.name, reloc.offset
                                    ),
                                ));
                            }
                            if target.arch == crate::target::Arch::AArch64 {
                                // Patch ADD Xd, Xn, #imm12 (and reject out-of-range).
                                let mut insn = u32::from_le_bytes(
                                    merged.data[off..off + 4].try_into().unwrap(),
                                );
                                if (insn & 0xFF80_0000) != 0x9100_0000 {
                                    return Err(LinkError::new(
                                        ErrorCode::UnsupportedRelocation,
                                        format!(
                                            "AArch64 local-exec TLS relocation against `{}` in `{}` \
                                             expects an ADD (imm12) instruction at the reloc site",
                                            reloc.symbol_name, merged.name
                                        ),
                                    ));
                                }
                                if !(0..=4095).contains(&val) {
                                    return Err(LinkError::new(
                                        ErrorCode::RelocationOverflow,
                                        format!(
                                            "AArch64 local-exec TLS offset {val} for `{}` does not \
                                             fit ADD imm12 (0..4095); larger TLS needs MOVZ/MOVK",
                                            reloc.symbol_name
                                        ),
                                    ));
                                }
                                insn = (insn & !0x003F_FC00) | ((val as u32) << 10);
                                merged.data[off..off + 4].copy_from_slice(&insn.to_le_bytes());
                            } else {
                                merged.data[off..off + 4]
                                    .copy_from_slice(&(val as i32).to_le_bytes());
                            }
                            continue;
                        }
                        RelocationKind::TlsInitialExec => {
                            // The instruction reads the variable's offset
                            // through a GOT slot
                            // (`mov reg, [rip + slot]` then `reg: %fs`), so the
                            // relocation itself is a PC-relative reference to
                            // the slot. The slot's content is filled after
                            // this loop.
                            let slot_va = match elf_tls_got_map.get(reloc.symbol_name.as_str()) {
                                Some(&va) => va,
                                None => {
                                    return Err(LinkError::new(
                                        ErrorCode::InvalidSection,
                                        format!(
                                            "initial-exec TLS relocation against `{}` in `{}` \
                                             has no GOT slot",
                                            reloc.symbol_name, merged.name
                                        ),
                                    ));
                                }
                            };
                            let val = (slot_va as i64)
                                .wrapping_sub(place_va as i64)
                                .wrapping_sub(4)
                                .wrapping_add(reloc.addend)
                                as i32;
                            let off = usize::try_from(reloc.offset).unwrap_or(usize::MAX);
                            if off.checked_add(4).is_none_or(|e| e > merged.data.len()) {
                                return Err(LinkError::new(
                                    crate::error::ErrorCode::RelocationOverflow,
                                    format!(
                                        "initial-exec TLS relocation at {}+0x{:x} exceeds section size",
                                        merged.name, reloc.offset
                                    ),
                                ));
                            }
                            merged.data[off..off + 4].copy_from_slice(&val.to_le_bytes());
                            continue;
                        }
                        RelocationKind::TlsDtpOff32 => {
                            // The DTPOFF32 disp32 was module-relative (the
                            // base register held the module's TLS block base
                            // from __tls_get_addr). A local-dynamic
                            // relaxation in this same section replaced that
                            // with `movq %fs:0,%rax` — the thread pointer —
                            // so the disp32 must become the full
                            // thread-pointer-relative offset. Without a
                            // relaxation the base register holds no defined
                            // value the toolchain can compute against, so
                            // this stays loud.
                            if relaxed_tls_ranges.is_empty() {
                                return Err(LinkError::new(
                                    ErrorCode::UnsupportedRelocation,
                                    format!(
                                        "TLS module-relative relocation (R_X86_64_DTPOFF32) \
                                         against `{}` in `{}` has no preceding relaxed \
                                         local-dynamic sequence: its base register holds a \
                                         __tls_get_addr result the Adesh toolchain cannot \
                                         compute",
                                        reloc.symbol_name, merged.name
                                    ),
                                ));
                            }
                            let (tls_start, tls_memsz) = match (tls_sec_va, tls_sec_memsz) {
                                (Some(start), Some(size)) => (start, size),
                                _ => {
                                    return Err(LinkError::new(
                                        ErrorCode::InvalidSection,
                                        format!(
                                            "TLS module-relative relocation against `{}` in `{}` \
                                             but the link has no TLS section",
                                            reloc.symbol_name, merged.name
                                        ),
                                    ));
                                }
                            };
                            if sym_va < tls_start || sym_va >= tls_start + tls_memsz {
                                return Err(LinkError::new(
                                    ErrorCode::InvalidSection,
                                    format!(
                                        "TLS module-relative relocation against `{}` resolves to \
                                         0x{:x}, outside the TLS block 0x{:x}..0x{:x}",
                                        reloc.symbol_name,
                                        sym_va,
                                        tls_start,
                                        tls_start + tls_memsz
                                    ),
                                ));
                            }
                            let span = if target.arch == crate::target::Arch::X86 {
                                tls_sec_tp_span.unwrap_or(tls_memsz)
                            } else {
                                tls_memsz
                            };
                            let tp_off = (sym_va as i64 - tls_start as i64) - span as i64;
                            let val = tp_off.wrapping_add(reloc.addend) as i32;
                            let off = usize::try_from(reloc.offset).unwrap_or(usize::MAX);
                            if off.checked_add(4).is_none_or(|e| e > merged.data.len()) {
                                return Err(LinkError::new(
                                    crate::error::ErrorCode::RelocationOverflow,
                                    format!(
                                        "TLS module-relative relocation at {}+0x{:x} exceeds \
                                         section size",
                                        merged.name, reloc.offset
                                    ),
                                ));
                            }
                            merged.data[off..off + 4].copy_from_slice(&val.to_le_bytes());
                            continue;
                        }
                        _ => {}
                    }
                }

                match handler.apply(reloc, place_va, sym_va, reloc.addend, &mut merged.data) {
                    Ok(()) => {
                        // Record absolute addresses for the PE base relocation
                        // table so ASLR can rebase the image safely. The width
                        // must match the target pointer size: a 64-bit field
                        // needs an IMAGE_REL_BASED_DIR64 entry, a 32-bit field
                        // an IMAGE_REL_BASED_HIGHLOW entry.
                        if !routed_to_trap
                            && target.format == crate::target::ObjectFormat::Pe
                            && target.image_base <= place_va
                        {
                            let rva = (place_va - target.image_base) as u32;
                            match reloc.kind {
                                RelocationKind::Absolute64
                                    if target.pointer_width == crate::target::PointerWidth::U64 =>
                                {
                                    base_relocs.push(rva);
                                }
                                RelocationKind::Absolute32
                                    if target.pointer_width == crate::target::PointerWidth::U32 =>
                                {
                                    base_relocs32.push(rva);
                                }
                                _ => {}
                            }
                        }

                        // ET_DYN images (PIE executables and shared libraries)
                        // are mapped at a runtime-chosen base: every absolute
                        // address baked into the image must carry a dynamic
                        // `R_X86_64_RELATIVE` relocation so the loader can
                        // rebase it. A fixed-base ET_EXEC dynamic link must
                        // carry none — the loader would write the (already
                        // correct) values into read-only load segments.
                        if !routed_to_trap
                            && elf_position_independent
                            && target.format == crate::target::ObjectFormat::Elf
                            && merged.is_alloc()
                        {
                            match reloc.kind {
                                RelocationKind::Absolute64 => {
                                    let value = (sym_va as i64).wrapping_add(reloc.addend) as u64;
                                    self.elf_dynamic_relocs.push(
                                        crate::elf::dynamic::ElfDynamicReloc {
                                            offset: place_va,
                                            kind:
                                                crate::elf::dynamic::ElfDynamicRelocKind::Relative,
                                            symbol: None,
                                            addend: value as i64,
                                        },
                                    );
                                }
                                RelocationKind::Absolute32
                                | RelocationKind::Absolute16
                                | RelocationKind::Absolute8 => {
                                    // A truncated absolute address cannot be
                                    // rebased by any dynamic relocation
                                    // (`R_X86_64_RELATIVE` rewrites a full
                                    // pointer word). Emitting one would be
                                    // silent wrong code; fail the link.
                                    return Err(LinkError::new(
                                        ErrorCode::UnsupportedRelocation,
                                        format!(
                                            "{}-bit absolute relocation against `{}` in `{}` is not \
                                             representable in a position-independent image; use a \
                                             64-bit pointer or a PC-relative form",
                                            reloc.kind.size_in_bytes() * 8,
                                            reloc.symbol_name,
                                            merged.name
                                        ),
                                    ));
                                }
                                _ => {}
                            }
                        }

                        // Mach-O executables are always PIE (the writer sets
                        // MH_PIE): every link-time absolute address baked
                        // into the image must appear in the dyld rebase
                        // stream. 32-bit absolutes cannot be rebased (the
                        // stream rewrites pointer words), so they fail the
                        // link instead of silently breaking under ASLR.
                        if !routed_to_trap
                            && target.format == crate::target::ObjectFormat::MachO
                            && merged.is_alloc()
                        {
                            match reloc.kind {
                                RelocationKind::Absolute64 => {
                                    // Name the contributor: the writer may
                                    // have to refuse this address (a fixup
                                    // slot in a non-writable segment), and
                                    // only the input that baked the pointer
                                    // makes that refusal actionable.
                                    let object = reloc
                                        .file_index
                                        .and_then(|f_idx| objects.get(f_idx))
                                        .map(|obj| obj.path.display().to_string())
                                        .unwrap_or_else(|| "<unknown object>".to_string());
                                    macho_rebases.push(crate::macho::dynamic::MachORebaseTarget {
                                        va: place_va,
                                        object,
                                        section: merged.name.clone(),
                                        offset: reloc.offset,
                                        symbol: reloc.symbol_name.clone(),
                                    });
                                }
                                RelocationKind::Absolute32
                                | RelocationKind::Absolute16
                                | RelocationKind::Absolute8 => {
                                    return Err(LinkError::new(
                                        ErrorCode::UnsupportedRelocation,
                                        format!(
                                            "{}-bit absolute relocation against `{}` in `{}` is not \
                                             representable in a PIE Mach-O image; use a 64-bit \
                                             pointer or a PC-relative form",
                                            reloc.kind.size_in_bytes() * 8,
                                            reloc.symbol_name,
                                            merged.name
                                        ),
                                    ));
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(e) => {
                        let is_overflow =
                            matches!(e.code, crate::error::ErrorCode::RelocationOverflow);
                        if is_overflow && is_weak_or_internal {
                            let off = usize::try_from(reloc.offset).unwrap_or(usize::MAX);
                            let sz = reloc.kind.size_in_bytes();
                            if off.checked_add(sz).is_some_and(|e| e <= merged.data.len()) {
                                merged.data[off..off + sz].fill(0);
                            }
                            if warned_syms.insert(reloc.symbol_name.clone(), ()).is_none() {
                                reloc_warnings.push(format!(
                                    "relocation against weak/internal symbol `{}` at {}+0x{:x} overflowed and was zero-filled",
                                    reloc.symbol_name, merged.name, reloc.offset
                                ));
                            }
                        } else {
                            return Err(e);
                        }
                    }
                }
            }
        }

        // ELF dynamic import bookkeeping: fill each import's GOT slot with
        // its PLT-stub address (the file-time initial value; eager
        // `DT_BIND_NOW` loaders overwrite every slot before execution) and
        // record one `R_X86_64_JUMP_SLOT` dynamic relocation per slot so the
        // loader performs that write.
        if let Some(got_idx) = elf_got_section_index {
            let got_slot_size: usize = if target.pointer_width.bits() == 32 {
                4
            } else {
                8
            };
            // Borrow dance: patch the merged .got in place.
            let (got_base, got_len) = {
                let got_sec = &merged_list[got_idx];
                (got_sec.virtual_address, got_sec.data.len())
            };
            for (name, slot_va) in &elf_got_map {
                let off = (*slot_va - got_base) as usize;
                if off + got_slot_size <= got_len {
                    let got_sec = &mut merged_list[got_idx];
                    if elf_imports.contains(name) {
                        // Dynamic imports use a PLT-stub initial value; the
                        // loader replaces it through JUMP_SLOT before entry.
                        let stub_va = symbol_va_map.get(name).copied().unwrap_or(0);
                        if got_slot_size == 4 {
                            got_sec.data[off..off + 4]
                                .copy_from_slice(&(stub_va as u32).to_le_bytes());
                        } else {
                            got_sec.data[off..off + 8].copy_from_slice(&stub_va.to_le_bytes());
                        }
                        self.elf_dynamic_relocs
                            .push(crate::elf::dynamic::ElfDynamicReloc {
                                offset: *slot_va,
                                kind: crate::elf::dynamic::ElfDynamicRelocKind::JumpSlot,
                                symbol: Some(name.clone()),
                                addend: 0,
                            });
                    } else {
                        // Locally defined symbols have a real GOT slot too.
                        // PIE/shared images need a RELATIVE relocation so
                        // the loader can add the runtime load bias; fixed-base
                        // ET_EXEC images can store the address directly.
                        let weak_undefined = resolver.weak_undefined_symbols.contains(name);
                        let value = if weak_undefined {
                            // ELF weak undefined symbols resolve to NULL.
                            // Their GOT slots intentionally remain zero.
                            0
                        } else {
                            symbol_va_map
                                .get(name)
                                .or_else(|| local_name_to_va.get(name))
                                .copied()
                                .ok_or_else(|| {
                                    LinkError::new(
                                        ErrorCode::InvalidSection,
                                        format!(
                                            "local GOT symbol `{}` has no resolved address",
                                            name
                                        ),
                                    )
                                })?
                        };
                        if got_slot_size == 4 {
                            got_sec.data[off..off + 4]
                                .copy_from_slice(&(value as u32).to_le_bytes());
                        } else {
                            got_sec.data[off..off + 8].copy_from_slice(&value.to_le_bytes());
                        }
                        if elf_position_independent && !weak_undefined {
                            self.elf_dynamic_relocs
                                .push(crate::elf::dynamic::ElfDynamicReloc {
                                    offset: *slot_va,
                                    kind: crate::elf::dynamic::ElfDynamicRelocKind::Relative,
                                    symbol: None,
                                    addend: value as i64,
                                });
                        }
                    }
                } else {
                    return Err(LinkError::new(
                        ErrorCode::InvalidSection,
                        format!(
                            "GOT slot for `{}` at 0x{:x} is outside the .got section",
                            name, slot_va
                        ),
                    ));
                }
            }
        }
        // Initial-exec TLS GOT slots: each holds the variable's
        // thread-pointer-relative offset (`off - memsz`, a negative value —
        // the static TLS block lies below the thread pointer).
        //
        // - Dynamic images leave the slot zeroed and emit an
        //   `R_X86_64_TPOFF64` dynamic relocation on it; the loader writes
        //   the real per-thread offset at startup.
        // - Static images write the link-time value directly (the
        //   single-threaded image's TLS block is the live one until a
        //   runtime installs per-thread copies).
        if !elf_tls_got_map.is_empty() {
            let (tls_start, tls_memsz) = match (tls_sec_va, tls_sec_memsz) {
                (Some(start), Some(size)) => (start, size),
                _ => {
                    return Err(LinkError::new(
                        ErrorCode::InvalidSection,
                        "initial-exec TLS GOT slots exist but the link has no TLS section",
                    ));
                }
            };
            let got_idx = elf_got_section_index.expect("TLS IE slots require a .got section");
            let (got_base, got_len) = {
                let got_sec = &merged_list[got_idx];
                (got_sec.virtual_address, got_sec.data.len())
            };
            for (name, slot_va) in &elf_tls_got_map {
                let sym_va = match symbol_va_map.get(name).copied() {
                    Some(va) => va,
                    None => {
                        return Err(LinkError::new(
                            ErrorCode::InvalidSection,
                            format!(
                                "initial-exec TLS variable `{}` has no resolved address",
                                name
                            ),
                        ));
                    }
                };
                let is_weak_undef = resolver.weak_undefined_symbols.contains(name);
                let tp_off = if is_weak_undef {
                    0i64
                } else {
                    if sym_va < tls_start || sym_va >= tls_start + tls_memsz {
                        return Err(LinkError::new(
                            ErrorCode::InvalidSection,
                            format!(
                                "initial-exec TLS variable `{}` resolves to 0x{:x}, outside the \
                                 TLS block 0x{:x}..0x{:x}",
                                name,
                                sym_va,
                                tls_start,
                                tls_start + tls_memsz
                            ),
                        ));
                    }
                    match target.arch {
                        crate::target::Arch::AArch64 => (sym_va as i64 - tls_start as i64) + 16,
                        crate::target::Arch::Riscv64 | crate::target::Arch::Riscv32 => {
                            sym_va as i64 - tls_start as i64
                        }
                        crate::target::Arch::X86 => {
                            let span = tls_sec_tp_span.unwrap_or(tls_memsz);
                            (sym_va as i64 - tls_start as i64) - span as i64
                        }
                        _ => (sym_va as i64 - tls_start as i64) - tls_memsz as i64,
                    }
                };

                let is_global = resolver
                    .table
                    .get(name)
                    .map(|r| r.symbol.binding == crate::symbol::SymbolBinding::Global)
                    .unwrap_or(false);

                if elf_dynamic && is_global {
                    // The loader owns the slot at runtime.
                    self.elf_dynamic_relocs
                        .push(crate::elf::dynamic::ElfDynamicReloc {
                            offset: *slot_va,
                            kind: crate::elf::dynamic::ElfDynamicRelocKind::Tpoff64,
                            symbol: Some(name.clone()),
                            addend: 0,
                        });
                } else {
                    let off = (*slot_va - got_base) as usize;
                    if off + 8 > got_len {
                        return Err(LinkError::new(
                            ErrorCode::InvalidSection,
                            format!(
                                "GOT slot for TLS variable `{}` at 0x{:x} is outside the .got \
                                 section",
                                name, slot_va
                            ),
                        ));
                    }
                    let got_sec = &mut merged_list[got_idx];
                    got_sec.data[off..off + 8].copy_from_slice(&(tp_off as u64).to_le_bytes());
                }
            }
        }
        if !elf_tls_gd_map.is_empty() {
            let (tls_start, tls_memsz) = match (tls_sec_va, tls_sec_memsz) {
                (Some(start), Some(size)) => (start, size),
                _ => {
                    return Err(LinkError::new(
                        ErrorCode::InvalidSection,
                        "general-dynamic TLS GOT slots exist but the link has no TLS section",
                    ));
                }
            };
            let got_idx = elf_got_section_index.expect("TLS GD slots require a .got section");
            let (got_base, got_len) = {
                let got_sec = &merged_list[got_idx];
                (got_sec.virtual_address, got_sec.data.len())
            };
            for (name, slot_va) in &elf_tls_gd_map {
                let sym_va = match symbol_va_map.get(name).copied() {
                    Some(va) => va,
                    None => {
                        return Err(LinkError::new(
                            ErrorCode::InvalidSection,
                            format!(
                                "general-dynamic TLS variable `{}` has no resolved address",
                                name
                            ),
                        ));
                    }
                };
                let is_weak_undef = resolver.weak_undefined_symbols.contains(name);
                let ti_offset = if is_weak_undef {
                    0i64
                } else {
                    if sym_va < tls_start || sym_va >= tls_start + tls_memsz {
                        return Err(LinkError::new(
                            ErrorCode::InvalidSection,
                            format!(
                                "general-dynamic TLS variable `{}` resolves to 0x{:x}, outside the \
                                 TLS block 0x{:x}..0x{:x}",
                                name,
                                sym_va,
                                tls_start,
                                tls_start + tls_memsz
                            ),
                        ));
                    }
                    (sym_va as i64 - tls_start as i64) - 2048
                };

                let off = (*slot_va - got_base) as usize;
                if off + 16 > got_len {
                    return Err(LinkError::new(
                        ErrorCode::InvalidSection,
                        format!(
                            "GOT slot for TLS GD variable `{}` at 0x{:x} is outside the .got section",
                            name, slot_va
                        ),
                    ));
                }
                let got_sec = &mut merged_list[got_idx];
                // struct tls_index { size_t ti_module; size_t ti_offset; };
                // ti_module = 1 (main executable)
                got_sec.data[off..off + 8].copy_from_slice(&1u64.to_le_bytes());
                got_sec.data[off + 8..off + 16].copy_from_slice(&(ti_offset as u64).to_le_bytes());
            }
        }
        if let Some(got_local_idx) = macho_got_local_section_index {
            let got_local_base = merged_list[got_local_idx].virtual_address;
            let got_local_sec = &mut merged_list[got_local_idx];
            for key in &macho_got_local_keys {
                let slot_va = macho_got_local_map[key];
                let off = (slot_va - got_local_base) as usize;
                let val = macho_got_local_values.get(key).copied().unwrap_or(0);
                if off + 8 <= got_local_sec.data.len() {
                    got_local_sec.data[off..off + 8].copy_from_slice(&val.to_le_bytes());
                }
                if val != 0 {
                    let symbol = match key {
                        MachoGotKey::Global(name) => name.clone(),
                        MachoGotKey::Local(_, name) => name.clone(),
                    };
                    macho_rebases.push(crate::macho::dynamic::MachORebaseTarget {
                        va: slot_va,
                        object: "<__got_local>".to_string(),
                        section: "__got_local".to_string(),
                        offset: off as u64,
                        symbol,
                    });
                }
            }
        }
        self.elf_got_slots = elf_got_map;
        self.macho_imports = macho_imports;
        self.macho_rebases = macho_rebases;
        self.macho_binds = macho_binds;

        self.merged_sections = merged_list;
        self.resolved_symbols = final_symbols;
        self.pe_import_info = pe_imp_result;
        self.pe_tls_info = pe_tls_info;
        self.base_relocs = base_relocs;
        self.base_relocs32 = base_relocs32;
        self.warnings = reloc_warnings;

        Ok(())
    }
}

/// True only for symbol names that are unambiguously linker/compiler internal:
/// Rust and MSVC mangled identities, plus the compiler-rt helpers the
/// intrinsics engine already knows how to synthesize.
///
/// Anchored prefix families only. A name that merely *contains* `..`, or starts
/// with a broad family prefix such as `__float`/`__gnu_`, is a genuine
/// application symbol and must surface as an undefined-symbol error rather than
/// being silently zero-filled or trap-stubbed.
fn is_known_internal_stub(name: &str) -> bool {
    crate::intrinsics::IntrinsicsEngine::is_intrinsic(name)
        || crate::os_router::OsApiRouter::is_mangled_internal(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::ObjectFile;
    use crate::relocation::{Relocation, RelocationKind};
    use crate::section::Section;
    use crate::symbol::{Symbol, SymbolBinding, SymbolType};
    use crate::target::Target;
    use std::path::PathBuf;

    #[test]
    fn referenced_weak_code_symbol_is_diagnosed_during_relocation() {
        let target = Target::x86_64_windows();
        let mut object = ObjectFile::new(PathBuf::from("weak_call.obj"), target.clone(), 0);
        let mut text = Section::new_code(".text", vec![0xE8, 0, 0, 0, 0, 0xC3], 16);
        text.relocations.push(Relocation::new(
            1,
            "optional_call",
            RelocationKind::PcRelative32,
            -4,
        ));
        object.add_section(text);
        object.add_symbol(Symbol::new_defined(
            "main",
            SymbolBinding::Global,
            SymbolType::Function,
            0,
            0,
            6,
            0,
        ));
        let mut optional_call = Symbol::new_undefined("optional_call", 0);
        optional_call.binding = SymbolBinding::Weak;
        object.add_symbol(optional_call);

        let mut objects = vec![object];
        let mut resolver = SymbolResolver::new();
        resolver
            .resolve_with_target(&mut objects, &[], &target)
            .expect("weak undefined symbol is permitted");
        assert!(resolver.warnings.is_empty());

        let mut layout = LayoutEngine::new();
        layout
            .layout_and_relocate(&objects, &resolver, &target, "main", false, false)
            .expect("weak call should be routed to the trap stub");
        assert!(
            layout
                .warnings
                .iter()
                .any(|warning| warning.contains("optional_call") && warning.contains("trap stub")),
            "a live unresolved weak call remains diagnosable"
        );
    }

    #[test]
    fn type_info_symbol_does_not_emit_unresolved_weak_warning() {
        let target = Target::x86_64_windows();
        let mut object = ObjectFile::new(PathBuf::from("type_info_test.obj"), target.clone(), 0);
        let text = Section::new_code(".text", vec![0xC3], 16);
        object.add_section(text);
        object.add_symbol(Symbol::new_defined(
            "main",
            SymbolBinding::Global,
            SymbolType::Function,
            0,
            0,
            1,
            0,
        ));

        let mut data = Section::new_data(".data", vec![0u8; 16], true, 8);
        data.relocations.push(Relocation::new(
            0,
            "??_7type_info@@6B@",
            RelocationKind::Absolute64,
            0,
        ));
        object.add_section(data);

        let mut type_info_sym = Symbol::new_undefined("??_7type_info@@6B@", 0);
        type_info_sym.binding = SymbolBinding::Weak;
        object.add_symbol(type_info_sym);

        let mut objects = vec![object];
        let mut resolver = SymbolResolver::new();
        resolver
            .resolve_with_target(&mut objects, &[], &target)
            .expect("resolution succeeds");

        let mut layout = LayoutEngine::new();
        layout
            .layout_and_relocate(&objects, &resolver, &target, "main", false, false)
            .expect("layout succeeds");

        assert!(
            !layout
                .warnings
                .iter()
                .any(|w| w.contains("??_7type_info@@6B@")),
            "??_7type_info@@6B@ must not emit an unresolved weak warning, got: {:?}",
            layout.warnings
        );
    }
}
