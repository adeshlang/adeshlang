//! Section Layout, Virtual Address assignment, W^X permission checking, and Relocation application.

use crate::arch::get_handler;
use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::object::ObjectFile;
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
}

impl LayoutEngine {
    pub fn new() -> Self {
        Self {
            merged_sections: Vec::new(),
            resolved_symbols: Vec::new(),
            entry_va: 0,
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

        for (f_idx, obj) in objects.iter().enumerate() {
            for (s_idx, sec) in obj.sections.iter().enumerate() {
                if !sec.is_live || sec.is_folded {
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

        // If Target is PE, compute .idata placement and map __imp_* IAT virtual addresses
        if target.format == crate::target::ObjectFormat::Pe {
            let mut pe_imports = Vec::new();
            let mut seen_imports = std::collections::HashSet::new();

            let mut collect_sym = |name: &str| {
                if let Some(dll) = crate::os_router::OsApiRouter::windows_dll_for(name) {
                    let clean = name
                        .strip_prefix("__imp_")
                        .or_else(|| name.strip_prefix("_imp_"))
                        .unwrap_or(name);
                    if seen_imports.insert(clean.to_string()) {
                        pe_imports.push(crate::pe::import::ImportSymbol {
                            dll_name: dll.to_string(),
                            symbol_name: clean.to_string(),
                            ordinal: None,
                        });
                    }
                }
            };

            for obj in objects {
                for sym in &obj.symbols {
                    if sym.is_imported
                        || !sym.is_defined
                        || sym.name.starts_with("__imp_")
                        || sym.name.starts_with("_imp_")
                    {
                        collect_sym(&sym.name);
                    }
                }
            }

            for (sym_name, resolved) in &resolver.table {
                if resolved.symbol.is_imported
                    || !resolved.symbol.is_defined
                    || sym_name.starts_with("__imp_")
                    || sym_name.starts_with("_imp_")
                {
                    collect_sym(sym_name);
                }
            }

            if !pe_imports.is_empty() {
                let idata_rva = align_to(current_va - target.image_base, 0x1000) as u32;
                let imp_res = crate::pe::import::build_import_table(
                    &pe_imports,
                    target.image_base,
                    idata_rva,
                );
                for (sym_name, sym_iat_rva) in &imp_res.symbol_iat_rvas {
                    let iat_va = target.image_base + (*sym_iat_rva as u64);
                    symbol_va_map.insert(format!("__imp_{}", sym_name), iat_va);
                    symbol_va_map.insert(format!("__imp__{}", sym_name), iat_va);
                    symbol_va_map.insert(format!("_imp_{}", sym_name), iat_va);
                    symbol_va_map.insert(format!("_imp__{}", sym_name), iat_va);
                }
            }
        }

        // Find Entry Point Virtual Address
        if let Some(&entry_va) = symbol_va_map.get(entry_name) {
            self.entry_va = entry_va;
        } else if let Some(&main_va) = symbol_va_map.get("main") {
            self.entry_va = main_va;
        } else if let Some(&start_va) = symbol_va_map.get("_start") {
            self.entry_va = start_va;
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
            // Check any file_local_va_map entry with matching name
            for ((_, sym_name), &va) in &file_local_va_map {
                if sym_name == name {
                    return Some(va);
                }
            }
            None
        };

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
                    || reloc.symbol_name.starts_with("__rust")
                    || reloc.symbol_name.starts_with("rust_")
                    || reloc.symbol_name.starts_with("anon.")
                    || reloc.symbol_name.starts_with("??")
                    || reloc.symbol_name.contains("..")
                    || reloc.symbol_name.starts_with("__extend")
                    || reloc.symbol_name.starts_with("__trunc")
                    || reloc.symbol_name.starts_with("__float")
                    || reloc.symbol_name.starts_with("__fix")
                    || reloc.symbol_name.starts_with("__gnu_")
                    || reloc.symbol_name.starts_with("__aeabi_");

                let sym_va = match sym_va_opt {
                    Some(va) => va,
                    None => {
                        if is_weak_or_internal {
                            0
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
                let place_va = merged.virtual_address + reloc.offset;

                match handler.apply(reloc, place_va, sym_va, reloc.addend, &mut merged.data) {
                    Ok(()) => {}
                    Err(e) => {
                        let is_overflow =
                            matches!(e.code, crate::error::ErrorCode::RelocationOverflow);
                        if is_overflow && is_weak_or_internal {
                            let off = reloc.offset as usize;
                            let sz = reloc.kind.size_in_bytes();
                            if off + sz <= merged.data.len() {
                                merged.data[off..off + sz].fill(0);
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

        Ok(())
    }
}
