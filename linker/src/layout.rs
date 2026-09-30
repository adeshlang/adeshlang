//! Section Layout, Virtual Address assignment, W^X permission checking, and Relocation application.

use crate::arch::get_handler;
use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::object::ObjectFile;
use crate::relocation::RelocationKind;
use crate::resolver::SymbolResolver;
use crate::section::{MergedSection, SectionKind, align_to, flags};
use crate::symbol::Symbol;
use crate::target::Target;
use std::collections::HashMap;

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
    /// RVAs of all 64-bit absolute relocations applied (for the PE `.reloc`
    /// base relocation table, enabling working ASLR).
    pub base_relocs: Vec<u32>,
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
            base_relocs: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Merge sections, assign virtual addresses, resolve symbols, and apply all relocations.
    pub fn layout_and_relocate(
        &mut self,
        objects: &[ObjectFile],
        resolver: &SymbolResolver,
        target: &Target,
        entry_name: &str,
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
        let mut meta_merged = MergedSection::new(
            ".adesh.meta",
            SectionKind::AdeshMeta,
            flags::READ | flags::ALLOC,
            4,
        );

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        enum SectionCat {
            Text,
            Rodata,
            Data,
            Bss,
            Meta,
        }

        // Map: (file_index, section_index) -> (SectionCat, offset_in_merged)
        let mut sec_placement: HashMap<(usize, usize), (SectionCat, u64)> = HashMap::new();

        // Map: symbol name -> (file_index, section_index) of its definition.
        // Used to alias ICF-folded sections to their canonical twin's placement.
        let mut sym_def_loc: HashMap<&str, (usize, usize)> = HashMap::new();
        for (f_idx, obj) in objects.iter().enumerate() {
            for sym in &obj.symbols {
                if sym.is_defined {
                    if let Some(s_idx) = sym.section_index {
                        sym_def_loc.entry(sym.name.as_str()).or_insert((f_idx, s_idx));
                    }
                }
            }
        }

        for (f_idx, obj) in objects.iter().enumerate() {
            for (s_idx, sec) in obj.sections.iter().enumerate() {
                if !sec.is_live {
                    continue;
                }

                // ICF-folded section: alias to the canonical section's placement
                // so every symbol defined here resolves to the canonical copy's
                // address (the bytes and relocations are verified identical).
                if sec.is_folded {
                    if let Some(ref canon_sym) = sec.folded_into {
                        if let Some(&(cf, cs)) = sym_def_loc.get(canon_sym.as_str()) {
                            if let Some(&placement) = sec_placement.get(&(cf, cs)) {
                                sec_placement.insert((f_idx, s_idx), placement);
                            }
                        }
                    }
                    continue;
                }

                if sec.name == ".adesh.meta" || sec.kind == SectionKind::AdeshMeta {
                    let off = meta_merged.append_section(sec, f_idx, s_idx);
                    sec_placement.insert((f_idx, s_idx), (SectionCat::Meta, off));
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
                    let export_name = if crate::os_router::OsApiRouter::windows_dll_for(clean).is_some() {
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
        }

        let has_explicit_pe_entry = objects.iter().any(|o| {
            o.symbols
                .iter()
                .any(|s| s.is_defined && (s.name == "mainCRTStartup" || s.name == "__adesh_windows_start"))
        }) || resolver.table.contains_key("mainCRTStartup")
            || resolver.table.contains_key("__adesh_windows_start");

        let has_main = objects.iter().any(|o| {
            o.symbols.iter().any(|s| s.is_defined && s.name == "main")
        }) || resolver.table.contains_key("main");

        let pe_thunks_start_off = if target.format == crate::target::ObjectFormat::Pe && !pe_imports.is_empty() {
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

        let mut merged_list = Vec::new();
        let mut cat_to_idx: HashMap<SectionCat, usize> = HashMap::new();

        if text_merged.size > 0 || !text_merged.data.is_empty() {
            cat_to_idx.insert(SectionCat::Text, merged_list.len());
            merged_list.push(text_merged);
        }
        if rodata_merged.size > 0 || !rodata_merged.data.is_empty() {
            cat_to_idx.insert(SectionCat::Rodata, merged_list.len());
            merged_list.push(rodata_merged);
        }
        if data_merged.size > 0 || !data_merged.data.is_empty() {
            cat_to_idx.insert(SectionCat::Data, merged_list.len());
            merged_list.push(data_merged);
        }
        if bss_merged.size > 0 || !bss_merged.data.is_empty() {
            cat_to_idx.insert(SectionCat::Bss, merged_list.len());
            merged_list.push(bss_merged);
        }
        if meta_merged.size > 0 || !meta_merged.data.is_empty() {
            cat_to_idx.insert(SectionCat::Meta, merged_list.len());
            merged_list.push(meta_merged);
        }

        // 2. Assign Virtual Addresses
        let mut current_va = target.image_base + 0x1000; // Start at base + 4KB
        for merged in &mut merged_list {
            current_va = align_to(current_va, merged.alignment.max(target.page_size));
            merged.virtual_address = current_va;
            current_va += merged.size;
        }

        let mut pe_imp_result = None;
        if target.format == crate::target::ObjectFormat::Pe && !pe_imports.is_empty() {
            let idata_va = align_to(current_va, target.page_size.max(0x1000));
            let idata_rva = (idata_va - target.image_base) as u32;
            let imp_res = crate::pe::import::build_import_table(
                &pe_imports,
                target.image_base,
                idata_rva,
            );
            let mut idata_sec = MergedSection::new(
                ".idata",
                SectionKind::Data,
                flags::READ | flags::WRITE | flags::ALLOC,
                8,
            );
            idata_sec.virtual_address = idata_va;
            idata_sec.data = imp_res.data.clone();
            idata_sec.size = imp_res.data.len() as u64;
            merged_list.push(idata_sec);
            current_va = idata_va + imp_res.data.len() as u64;
            pe_imp_result = Some(imp_res);
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
        let mut file_local_va_map: HashMap<(usize, String), u64> = HashMap::new();
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

        // Deterministic first-definition-wins name index for the relocation
        // fallback below (replaces a per-relocation linear scan of every
        // file-local symbol, which made linking O(relocs * symbols)).
        let mut local_name_to_va: HashMap<String, u64> = HashMap::new();

        // Compute addresses of all defined symbols across all object files
        for (f_idx, obj) in objects.iter().enumerate() {
            for sym in &obj.symbols {
                if sym.is_defined {
                    if let Some(sec_idx) = sym.section_index {
                        if let Some(&(cat, sec_off)) = sec_placement.get(&(f_idx, sec_idx)) {
                            if let Some(&m_idx) = cat_to_idx.get(&cat) {
                                let sec_va = merged_list[m_idx].virtual_address;
                                let sym_va = sec_va + sec_off + sym.value;
                                if sym.binding != crate::symbol::SymbolBinding::Local {
                                    symbol_va_map.insert(sym.name.clone(), sym_va);
                                } else {
                                    symbol_va_map.entry(sym.name.clone()).or_insert(sym_va);
                                }
                                file_local_va_map.insert((f_idx, sym.name.clone()), sym_va);
                                local_name_to_va.entry(sym.name.clone()).or_insert(sym_va);
                            }
                        }
                    }
                }
            }
        }

        // Add globals from resolver table
        for (sym_name, resolved) in &resolver.table {
            let mut final_sym = resolved.symbol.clone();
            if let Some(&va) = symbol_va_map.get(sym_name) {
                final_sym.value = va;
            }
            final_symbols.push(final_sym);
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

        // Find Entry Point Virtual Address
        if let Some(&entry_va) = symbol_va_map.get(entry_name) {
            self.entry_va = entry_va;
        } else if let Some(&main_crt_va) = symbol_va_map.get("mainCRTStartup") {
            self.entry_va = main_crt_va;
        } else if let Some(&start_va) = symbol_va_map.get("_start") {
            self.entry_va = start_va;
        } else if let Some(&main_va) = symbol_va_map.get("main") {
            self.entry_va = main_va;
        } else if let Some(first_text) = merged_list.iter().find(|s| s.is_executable()) {
            self.entry_va = first_text.virtual_address;
        } else {
            self.entry_va = target.image_base + 0x1000;
        }

        // Helper to resolve symbol VA with fallbacks (exact local -> global -> unmangled / prefixed)
        let resolve_sym_va = |f_idx_opt: Option<usize>, name: &str| -> Option<u64> {
            if let Some(f_idx) = f_idx_opt {
                if let Some(&va) = file_local_va_map.get(&(f_idx, name.to_string())) {
                    return Some(va);
                }
            }
            if let Some(&va) = symbol_va_map.get(name) {
                return Some(va);
            }
            if let Some(stripped) = name.strip_prefix("__imp_") {
                if let Some(&va) = symbol_va_map.get(stripped) {
                    return Some(va);
                }
            }
            if let Some(stripped) = name.strip_prefix('_') {
                if let Some(&va) = symbol_va_map.get(stripped) {
                    return Some(va);
                }
            } else if let Some(&va) = symbol_va_map.get(&format!("_{}", name)) {
                return Some(va);
            }
            // Fall back to the first definition of any file-local symbol
            // with this name (deterministic: lowest file index wins).
            if let Some(&va) = local_name_to_va.get(name) {
                return Some(va);
            }
            None
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
        let mut base_relocs: Vec<u32> = Vec::new();

        // 5. Apply Relocations to Merged Sections
        let handler = get_handler(target.arch);

        for merged in &mut merged_list {
            if merged.kind == SectionKind::Bss || merged.data.is_empty() {
                continue;
            }

            for reloc in &merged.relocations {
                let sym_va_opt = resolve_sym_va(reloc.symbol_index, &reloc.symbol_name);
                let is_weak_or_internal = reloc.symbol_name.starts_with("__weak_")
                    || reloc.symbol_name.starts_with("_ZN")
                    || reloc.symbol_name.starts_with("ZN")
                    || reloc.symbol_name.starts_with("_R")
                    || reloc.symbol_name.starts_with("R_")
                    || reloc.symbol_name.starts_with("__rust")
                    || reloc.symbol_name.starts_with("___rust")
                    || reloc.symbol_name.starts_with("rust_")
                    || reloc.symbol_name.starts_with("_rust_")
                    || reloc.symbol_name.contains("__rust_")
                    || reloc.symbol_name.contains("___rust")
                    || reloc.symbol_name.starts_with("anon.")
                    || reloc.symbol_name.starts_with("??")
                    || reloc.symbol_name.contains("..")
                    || reloc.symbol_name.starts_with("__extend")
                    || reloc.symbol_name.starts_with("__trunc")
                    || reloc.symbol_name.starts_with("__float")
                    || reloc.symbol_name.starts_with("__fix")
                    || reloc.symbol_name.starts_with("__gnu_")
                    || reloc.symbol_name.starts_with("__aeabi_")
                    || crate::os_router::OsApiRouter::is_internal(&reloc.symbol_name);

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
                                    if warned_syms.insert(reloc.symbol_name.clone(), ()).is_none()
                                    {
                                        reloc_warnings.push(format!(
                                            "unresolved weak/internal symbol `{}` (referenced from {}+0x{:x}) routed to trap stub; \
                                             if this symbol is expected to be called at runtime, the runtime library is missing a definition",
                                            reloc.symbol_name, merged.name, reloc.offset
                                        ));
                                    }
                                    (tva, true)
                                }
                                _ => {
                                    if warned_syms.insert(reloc.symbol_name.clone(), ()).is_none()
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
                            return Err(LinkError::undefined_symbol(
                                &reloc.symbol_name,
                                &merged.name,
                                None,
                                Some(reloc.offset),
                            ));
                        }
                    }
                };

                match handler.apply(reloc, place_va, sym_va, reloc.addend, &mut merged.data) {
                    Ok(()) => {
                        // Record 64-bit absolute addresses for the PE base
                        // relocation table so ASLR can rebase the image safely.
                        if !routed_to_trap
                            && target.format == crate::target::ObjectFormat::Pe
                            && reloc.kind == RelocationKind::Absolute64
                            && target.image_base <= place_va
                        {
                            base_relocs.push((place_va - target.image_base) as u32);
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

        self.merged_sections = merged_list;
        self.resolved_symbols = final_symbols;
        self.pe_import_info = pe_imp_result;
        self.base_relocs = base_relocs;
        self.warnings = reloc_warnings;

        Ok(())
    }
}
