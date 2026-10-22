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
    /// imported function (index i owns `__la_symbol_ptr` slot i).
    pub macho_imports: Vec<crate::macho::dynamic::MachOImport>,
    /// Virtual addresses of 64-bit pointer words baked at link time that
    /// dyld must rebase when the (always-PIE) image slides.
    pub macho_rebases: Vec<u64>,
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

        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        enum SectionCat {
            Text,
            Rodata,
            Data,
            Bss,
            Tls,
            Meta,
            EhFrame,
            EhFrameHdr,
            Debug(String),
        }

        /// Collect the functions an `.eh_frame` can honestly describe: every
        /// defined function symbol placed in a text section whose size is
        /// known. Returns `(offset_within_merged_text, size)` in
        /// deterministic object/symbol order.
        ///
        /// Both the size-planning pass (before virtual addresses are
        /// assigned) and the content-fill pass (after) call this with the
        /// same inputs, so the FDE count and the FDE set always match.
        fn collect_frame_functions(
            objects: &[ObjectFile],
            sec_placement: &HashMap<(usize, usize), (SectionCat, u64)>,
        ) -> Vec<(u64, u64)> {
            let mut out = Vec::new();
            for (f_idx, obj) in objects.iter().enumerate() {
                for sym in &obj.symbols {
                    if !sym.is_defined
                        || sym.sym_type != crate::symbol::SymbolType::Function
                        || sym.size == 0
                    {
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
                    // Offset of the function inside the merged .text: the
                    // input section's placement plus the symbol's offset
                    // within it.
                    out.push((sec_off + sym.value, sym.size));
                }
            }
            out
        }

        let mut debug_merged_map: HashMap<String, MergedSection> = HashMap::new();

        // Map: (file_index, section_index) -> (SectionCat, offset_in_merged)
        let mut sec_placement: HashMap<(usize, usize), (SectionCat, u64)> = HashMap::new();
        let mut tls_sections: Vec<(usize, usize)> = Vec::new();
        let mut tls_callback_sections: Vec<(usize, usize)> = Vec::new();

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

                if matches!(sec.kind, SectionKind::TData | SectionKind::TBss)
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
            && tls_merged.size > 0
        {
            // Other output writers do not yet emit their runtime TLS
            // descriptors (e.g. Mach-O TLV). Refuse a binary with
            // thread-local data in an ordinary loadable section rather
            // than pretending it has per-thread storage.
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
        // every symbol the resolver routed to a dylib becomes a lazy-bound
        // import with one `__la_symbol_ptr` slot. The bind name carries
        // Mach-O's leading-underscore convention.
        let mut macho_imports: Vec<crate::macho::dynamic::MachOImport> = Vec::new();
        if target.format == crate::target::ObjectFormat::MachO {
            let mut raw: Vec<String> = resolver.dynamic_imports.clone();
            raw.sort();
            raw.dedup();
            for raw_name in raw {
                let bind_name = match crate::os_router::OsApiRouter::classify(&raw_name, target) {
                    crate::os_router::SymbolRoute::DllImport { name, .. } => name,
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
                    la_slot_va: 0, // assigned when the lazy-pointer section is placed
                    is_data: false,
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
            // Mach-O's router cannot always tell a data object from a
            // function by name alone, so references are checked by relocation
            // shape below (anything but a PC-relative call into an import is
            // a data reference). Names that classify as intrinsics never get
            // here (they are synthesized, not imported).
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
            if !data_imports.is_empty() {
                let shown: Vec<&str> = data_imports.iter().map(|s| s.as_str()).collect();
                return Err(LinkError::new(
                    ErrorCode::UnsupportedRelocation,
                    format!(
                        "imported data symbol(s) are not supported: {} — Mach-O links bind \
                         functions through lazy stubs only; taking the address of a C library \
                         data object is not implemented",
                        shown.join(", ")
                    ),
                ));
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

        // ELF x86-64: synthesize `.eh_frame` + `.eh_frame_hdr` with honest
        // standard-frame CFI for every function whose address and size are
        // both known (see `collect_frame_functions`). Functions without a
        // size get no FDE: fabricating a range would be silent wrong
        // unwinding. Sizes are address-independent (fixed CIE + fixed-size
        // FDEs), so the sections are pre-sized here and filled once the
        // virtual addresses are assigned below.
        let frame_functions = if target.format == crate::target::ObjectFormat::Elf
            && target.arch == crate::target::Arch::X86_64
        {
            collect_frame_functions(objects, &sec_placement)
        } else {
            Vec::new()
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
        if !frame_functions.is_empty() {
            let fde_count = frame_functions.len();
            let eh_frame_size = crate::unwind::EhFrameGenerator::eh_frame_size(fde_count);
            let hdr_size = crate::unwind::EhFrameGenerator::eh_frame_hdr_size(fde_count);
            let mut eh_frame_sec = MergedSection::new(
                ".eh_frame",
                SectionKind::Rodata,
                flags::READ | flags::ALLOC,
                4,
            );
            eh_frame_sec.data = vec![0u8; eh_frame_size as usize];
            eh_frame_sec.size = eh_frame_size;
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
        if bss_merged.size > 0 || !bss_merged.data.is_empty() {
            cat_to_idx.insert(SectionCat::Bss, merged_list.len());
            merged_list.push(bss_merged);
        }
        // Keep the shared TLS layout for every output format. Only the PE
        // loader metadata is format-specific.
        if tls_merged.size > 0 || !tls_merged.data.is_empty() {
            cat_to_idx.insert(SectionCat::Tls, merged_list.len());
            merged_list.push(tls_merged);
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
        for merged in &mut merged_list {
            if !merged.is_alloc() {
                merged.virtual_address = 0;
                continue;
            }
            current_va = align_to(current_va, merged.alignment.max(target.page_size));
            merged.virtual_address = current_va;
            current_va += merged.size;
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
        let mut elf_got_section_index: Option<usize> = None;
        let elf_got_local_names: std::collections::BTreeSet<String> =
            if target.format == crate::target::ObjectFormat::Elf {
                objects
                    .iter()
                    .flat_map(|obj| obj.sections.iter())
                    .filter(|sec| sec.is_live)
                    .flat_map(|sec| sec.relocations.iter())
                    .filter(|reloc| reloc.kind == RelocationKind::GotRelative32)
                    .map(|reloc| reloc.symbol_name.clone())
                    .filter(|name| !name.is_empty() && !elf_imports.contains(name))
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
                                RelocationKind::TlsInitialExec => true,
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
        if target.format == crate::target::ObjectFormat::Elf
            && (!elf_imports.is_empty()
                || !elf_tls_ie_names.is_empty()
                || !elf_got_local_names.is_empty())
        {
            // Page-aligned start so `PT_GNU_RELRO` can cover the GOT (and the
            // dynamic region after it) without also mprotecting preceding
            // writable user data that shares its final page.
            let got_va = align_to(current_va, target.page_size.max(8));
            let mut got_sec = MergedSection::new(
                ".got",
                SectionKind::Data,
                flags::READ | flags::WRITE | flags::ALLOC,
                8,
            );
            got_sec.virtual_address = got_va;
            // x86-64/AArch64 GOT prologue: slot 0 holds &_DYNAMIC (patched by
            // the writer once the .dynamic address is known); slots 1 and 2
            // are reserved for the dynamic loader's link-map/resolver pointers
            // (unused with DT_BIND_NOW, reserved per ABI). One slot per
            // imported symbol, local GOT-referenced symbol, and
            // initial-exec TLS variable follows.
            let n_header_slots = 3usize;
            got_sec.data = vec![
                0u8;
                (n_header_slots
                    + elf_imports.len()
                    + elf_got_local_names.len()
                    + elf_tls_ie_names.len())
                    * 8
            ];
            got_sec.size = got_sec.data.len() as u64;
            for (idx, sym) in elf_imports.iter().enumerate() {
                let slot_va = got_va + ((n_header_slots + idx) * 8) as u64;
                elf_got_map.insert(sym.clone(), slot_va);
            }
            for (idx, name) in elf_tls_ie_names.iter().enumerate() {
                let slot_va = got_va
                    + ((n_header_slots + elf_imports.len() + elf_got_local_names.len() + idx) * 8)
                        as u64;
                elf_tls_got_map.insert(name.clone(), slot_va);
            }
            for (idx, name) in elf_got_local_names.iter().enumerate() {
                let slot_va = got_va + ((n_header_slots + elf_imports.len() + idx) * 8) as u64;
                elf_got_map.insert(name.clone(), slot_va);
            }
            current_va = got_va + got_sec.size;
            elf_got_section_index = Some(merged_list.len());
            merged_list.push(got_sec);
        }

        // Mach-O lazy symbol pointers: one 8-byte slot per imported function.
        // The slots start zeroed; the writer patches each to its
        // `__stub_helper` entry (the pre-binding target) and dyld overwrites
        // them lazily on first call. Slot VAs are registered as
        // `__la_symbol_ptr_<name>` symbols so the proxy stubs synthesized by
        // the linker can `jmp *[rip + slot]`.
        let mut macho_la_section_index: Option<usize> = None;
        if target.format == crate::target::ObjectFormat::MachO && !macho_imports.is_empty() {
            let has_data = merged_list.iter().any(|s| {
                (s.kind == SectionKind::Data || s.kind == SectionKind::Bss) && s.is_alloc()
            });
            let la_va = if has_data {
                align_to(current_va, 8)
            } else {
                align_to(current_va, target.page_size.max(8))
            };
            let mut la_sec = MergedSection::new(
                "__la_symbol_ptr",
                SectionKind::Data,
                flags::READ | flags::WRITE | flags::ALLOC,
                8,
            );
            la_sec.virtual_address = la_va;
            la_sec.data = vec![0u8; macho_imports.len() * 8];
            la_sec.size = la_sec.data.len() as u64;
            for (idx, import) in macho_imports.iter_mut().enumerate() {
                import.la_slot_va = la_va + (idx as u64) * 8;
            }
            current_va = la_va + la_sec.size;
            macho_la_section_index = Some(merged_list.len());
            merged_list.push(la_sec);
        }
        let _ = macho_la_section_index;

        // Fill the pre-sized .eh_frame / .eh_frame_hdr sections with the
        // synthesized standard-frame CFI. The sections were sized before
        // virtual addresses existed; now that every function's final
        // address is known, the CIE/FDE bytes and the .eh_frame_hdr binary
        // search index can be written for real.
        if !frame_functions.is_empty() {
            let text_base_va = cat_to_idx
                .get(&SectionCat::Text)
                .map(|&idx| merged_list[idx].virtual_address)
                .unwrap_or(0);
            let fdes: Vec<(u64, u64)> = frame_functions
                .iter()
                .map(|&(off, size)| (text_base_va + off, size))
                .collect();

            let eh_frame_idx = cat_to_idx
                .get(&SectionCat::EhFrame)
                .copied()
                .expect("frame functions require a pre-sized .eh_frame section");
            let eh_frame_hdr_idx = cat_to_idx
                .get(&SectionCat::EhFrameHdr)
                .copied()
                .expect("frame functions require a pre-sized .eh_frame_hdr section");
            let eh_frame_va = merged_list[eh_frame_idx].virtual_address;
            let hdr_va = merged_list[eh_frame_hdr_idx].virtual_address;

            merged_list[eh_frame_idx].data =
                crate::unwind::EhFrameGenerator::assemble_eh_frame(eh_frame_va, &fdes);

            // One index entry per FDE: (function initial location, FDE
            // address). The generator sorts by initial location and encodes
            // both as data-relative sdata4 values.
            let entries: Vec<(u64, u64)> = fdes
                .iter()
                .enumerate()
                .map(|(i, &(func_va, _))| {
                    (
                        func_va,
                        eh_frame_va
                            + crate::unwind::EhFrameGenerator::CIE_SIZE
                            + crate::unwind::EhFrameGenerator::FDE_SIZE * i as u64,
                    )
                })
                .collect();
            merged_list[eh_frame_hdr_idx].data =
                crate::unwind::EhFrameHdrGenerator::build(hdr_va, eh_frame_va, entries);

            // Guard the section sizes against drift between the sizing pass
            // and the fill pass: a mismatch would silently corrupt every
            // VA-dependent offset recorded above (PT_GNU_EH_FRAME bounds,
            // data-relative index entries).
            debug_assert_eq!(
                merged_list[eh_frame_idx].data.len() as u64,
                crate::unwind::EhFrameGenerator::eh_frame_size(fdes.len())
            );
            debug_assert_eq!(
                merged_list[eh_frame_hdr_idx].data.len() as u64,
                crate::unwind::EhFrameGenerator::eh_frame_hdr_size(fdes.len())
            );
        }

        // Synthesize Windows x64 SEH .pdata (RUNTIME_FUNCTION) and .xdata (UNWIND_INFO)
        if target.format == crate::target::ObjectFormat::Pe
            && target.arch == crate::target::Arch::X86_64
        {
            let has_input_pdata = objects.iter().any(|obj| {
                obj.sections
                    .iter()
                    .any(|sec| sec.name.starts_with(".pdata") && sec.is_live)
            });
            if !has_input_pdata {
                let text_base_va = cat_to_idx
                    .get(&SectionCat::Text)
                    .map(|&idx| merged_list[idx].virtual_address)
                    .unwrap_or(0);
                let text_size = cat_to_idx
                    .get(&SectionCat::Text)
                    .map(|&idx| merged_list[idx].size)
                    .unwrap_or(0);
                if text_size > 0 {
                    let mut pdata_entries = Vec::new();
                    let text_base_rva = (text_base_va - target.image_base) as u32;

                    let xdata_bytes =
                        crate::unwind::WindowsPdataGenerator::build_standard_frame_xdata(0, &[]);
                    let xdata_va = align_to(current_va, target.page_size.max(0x1000));
                    let xdata_rva = (xdata_va - target.image_base) as u32;
                    let xdata_size = xdata_bytes.len() as u64;

                    for (f_idx, obj) in objects.iter().enumerate() {
                        for sym in &obj.symbols {
                            if sym.is_defined && sym.sym_type == crate::symbol::SymbolType::Function
                            {
                                if let Some(s_idx) = sym.section_index {
                                    if let Some((cat, sec_off)) = sec_placement.get(&(f_idx, s_idx))
                                    {
                                        if *cat == SectionCat::Text {
                                            let func_start =
                                                text_base_rva + (*sec_off + sym.value) as u32;
                                            let func_len =
                                                if sym.size > 0 { sym.size as u32 } else { 16 };
                                            pdata_entries.push(crate::unwind::PdataEntry {
                                                begin_rva: func_start,
                                                end_rva: func_start + func_len,
                                                unwind_info_rva: xdata_rva,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if pdata_entries.is_empty() {
                        pdata_entries.push(crate::unwind::PdataEntry {
                            begin_rva: text_base_rva,
                            end_rva: text_base_rva + text_size as u32,
                            unwind_info_rva: xdata_rva,
                        });
                    }

                    let pdata_bytes =
                        crate::unwind::WindowsPdataGenerator::build_pdata(pdata_entries);
                    let pdata_va = align_to(xdata_va + xdata_size, target.page_size.max(0x1000));

                    let mut xdata_sec = MergedSection::new(
                        ".xdata",
                        SectionKind::Rodata,
                        flags::READ | flags::ALLOC,
                        8,
                    );
                    xdata_sec.virtual_address = xdata_va;
                    xdata_sec.data = xdata_bytes;
                    xdata_sec.size = xdata_size;

                    let mut pdata_sec = MergedSection::new(
                        ".pdata",
                        SectionKind::Rodata,
                        flags::READ | flags::ALLOC,
                        4,
                    );
                    pdata_sec.virtual_address = pdata_va;
                    pdata_sec.data = pdata_bytes;
                    pdata_sec.size = pdata_sec.data.len() as u64;

                    merged_list.push(xdata_sec);
                    merged_list.push(pdata_sec);
                }
            }
        }

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

        // Add globals from resolver table
        for (sym_name, resolved) in &resolver.table {
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
            for (sym_name, got_slot_va) in &elf_got_map {
                symbol_va_map.insert(format!("__got_{}", sym_name), *got_slot_va);
                symbol_va_map.insert(format!("_got_{}", sym_name), *got_slot_va);
            }
        }

        // Map Mach-O lazy-pointer slots so the proxy stubs synthesized for
        // imports (`jmp *[rip + slot]`) resolve to their slot VA.
        if target.format == crate::target::ObjectFormat::MachO {
            for import in &macho_imports {
                symbol_va_map.insert(
                    format!("__la_symbol_ptr_{}", import.raw_name),
                    import.la_slot_va,
                );
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
        // the thread pointer (x86-64 %fs base) addresses the END of the block
        // by the Adesh/ld.so convention, so a variable at block offset `off`
        // lives at `%fs:(off - memsz)`.
        let tls_sec_memsz = cat_to_idx.get(&SectionCat::Tls).map(|idx| {
            merged_list[*idx]
                .size
                .max(merged_list[*idx].data.len() as u64)
        });

        // 5. Apply Relocations to Merged Sections
        let handler = get_handler(target.arch);

        // Mach-O dynamic-linking bookkeeping: which symbols are imports (for
        // the relocation-shape check) and which absolute words need a dyld
        // rebase opcode (PIE).
        let macho_import_set: std::collections::HashSet<&str> = macho_imports
            .iter()
            .map(|imp| imp.raw_name.as_str())
            .collect();
        let mut macho_rebases: Vec<u64> = Vec::new();

        for merged in &mut merged_list {
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

            for reloc in &merged.relocations {
                if relaxed_tls_ranges
                    .iter()
                    .any(|(lo, hi)| reloc.offset >= *lo && reloc.offset < *hi)
                {
                    // Absorbed by a relaxed TLS sequence rewrite.
                    continue;
                }
                let (sym_va_opt, sym_sec_base) =
                    resolve_sym_va(reloc.file_index, reloc.symbol_index, &reloc.symbol_name);
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

                // Mach-O imports are reachable ONLY through their proxy
                // stub by a PC-relative call/jump. Any other relocation
                // shape is an address-of-import (or a data load): binding
                // it to the stub's address would be silent wrong code.
                if !routed_to_trap
                    && target.format == crate::target::ObjectFormat::MachO
                    && macho_import_set.contains(reloc.symbol_name.as_str())
                {
                    let is_call = matches!(
                        reloc.kind,
                        RelocationKind::PcRelative32
                            | RelocationKind::PcRelative64
                            | RelocationKind::PltRelative32
                            | RelocationKind::GotRelative32
                            | RelocationKind::AArch64Call26
                            | RelocationKind::AArch64Adrp
                            | RelocationKind::AArch64AddLo12
                            | RelocationKind::AArch64LdSt64Lo12
                    );
                    if !is_call {
                        return Err(LinkError::new(
                            ErrorCode::UnsupportedRelocation,
                            format!(
                                "imported symbol `{}` is referenced by a non-call relocation in \
                                 `{}` — Mach-O imports bind only through lazy call stubs; taking \
                                 the address of (or loading from) a C library symbol is not \
                                 implemented",
                                reloc.symbol_name, merged.name
                            ),
                        ));
                    }
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
                            let off = reloc.offset as usize;
                            if off + 4 <= merged.data.len() {
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
                            let off = reloc.offset as usize;
                            if off + 4 <= merged.data.len() {
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
                            let off = reloc.offset as usize;
                            if off + 4 > merged.data.len() {
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
                            let tp_off = (sym_va as i64 - tls_start as i64) - tls_memsz as i64;
                            let val = tp_off.wrapping_add(reloc.addend) as i32;
                            let off = reloc.offset as usize;
                            if off + 4 > merged.data.len() {
                                return Err(LinkError::new(
                                    crate::error::ErrorCode::RelocationOverflow,
                                    format!(
                                        "local-exec TLS relocation at {}+0x{:x} exceeds section size",
                                        merged.name, reloc.offset
                                    ),
                                ));
                            }
                            merged.data[off..off + 4].copy_from_slice(&val.to_le_bytes());
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
                            let off = reloc.offset as usize;
                            if off + 4 > merged.data.len() {
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
                            let tp_off = (sym_va as i64 - tls_start as i64) - tls_memsz as i64;
                            let val = tp_off.wrapping_add(reloc.addend) as i32;
                            let off = reloc.offset as usize;
                            if off + 4 > merged.data.len() {
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
                                    macho_rebases.push(place_va);
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
                            let off = reloc.offset as usize;
                            let sz = reloc.kind.size_in_bytes();
                            if off + sz <= merged.data.len() {
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
            // Borrow dance: patch the merged .got in place.
            let (got_base, got_len) = {
                let got_sec = &merged_list[got_idx];
                (got_sec.virtual_address, got_sec.data.len())
            };
            for (name, slot_va) in &elf_got_map {
                let off = (*slot_va - got_base) as usize;
                if off + 8 <= got_len {
                    let got_sec = &mut merged_list[got_idx];
                    if elf_imports.contains(name) {
                        // Dynamic imports use a PLT-stub initial value; the
                        // loader replaces it through JUMP_SLOT before entry.
                        let stub_va = symbol_va_map.get(name).copied().unwrap_or(0);
                        got_sec.data[off..off + 8].copy_from_slice(&stub_va.to_le_bytes());
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
                        got_sec.data[off..off + 8].copy_from_slice(&value.to_le_bytes());
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
                    (sym_va as i64 - tls_start as i64) - tls_memsz as i64
                };

                if elf_dynamic {
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
        self.elf_got_slots = elf_got_map;
        self.macho_imports = macho_imports;
        self.macho_rebases = macho_rebases;

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
