//! Identical Code Folding (ICF) pass.

use crate::config::IcfMode;
use crate::hash::fnv1a_64;
use crate::object::ObjectFile;
use crate::symbol::SymbolBinding;
use std::collections::{HashMap, HashSet};

pub struct IcfEngine;

impl IcfEngine {
    pub fn fold_sections(objects: &mut [ObjectFile], mode: IcfMode, print_icf: bool) -> usize {
        if mode == IcfMode::None {
            return 0;
        }

        // Map: hash -> (file_index, section_index, canonical_symbol_name)
        let mut hashes: HashMap<u64, (usize, usize, String)> = HashMap::new();
        let mut folded_count = 0;
        let indexed_targets: HashSet<(usize, usize)> = objects
            .iter()
            .enumerate()
            .flat_map(|(f_idx, obj)| {
                obj.sections
                    .iter()
                    .filter(|sec| sec.is_live)
                    .flat_map(move |sec| {
                        sec.relocations.iter().filter_map(move |r| {
                            r.symbol_index
                                .and_then(|idx| obj.symbols.get(idx))
                                .and_then(|sym| sym.section_index)
                                .map(|s_idx| (f_idx, s_idx))
                        })
                    })
            })
            .collect();

        // In Safe mode, functions whose addresses are taken (referenced from data
        // sections, absolute relocations, or exported) must not be folded.
        let address_taken: HashSet<(usize, usize)> = if mode == IcfMode::Safe {
            let mut set = HashSet::new();
            for (f_idx, obj) in objects.iter().enumerate() {
                for sym in &obj.symbols {
                    if sym.is_defined && sym.is_exported {
                        if let Some(s_idx) = sym.section_index {
                            set.insert((f_idx, s_idx));
                        }
                    }
                }
            }
            for (f_idx, obj) in objects.iter().enumerate() {
                for sec in &obj.sections {
                    if !sec.is_live {
                        continue;
                    }
                    let is_data = !sec.is_executable();
                    for r in &sec.relocations {
                        let is_address_ref = is_data
                            || matches!(
                                r.kind,
                                crate::relocation::RelocationKind::Absolute64
                                    | crate::relocation::RelocationKind::Absolute32
                                    | crate::relocation::RelocationKind::Absolute16
                                    | crate::relocation::RelocationKind::SectionRelative32
                                    | crate::relocation::RelocationKind::ImageRelative32
                            );

                        if is_address_ref {
                            if let Some(si) = r.symbol_index {
                                if let Some(sym) = obj.symbols.get(si) {
                                    if sym.is_defined {
                                        if let Some(s_idx) = sym.section_index {
                                            set.insert((f_idx, s_idx));
                                        }
                                    }
                                }
                            } else {
                                for (target_f, target_obj) in objects.iter().enumerate() {
                                    for sym in &target_obj.symbols {
                                        if sym.is_defined && sym.name == r.symbol_name {
                                            if let Some(s_idx) = sym.section_index {
                                                set.insert((target_f, s_idx));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            set
        } else {
            HashSet::new()
        };

        for f_idx in 0..objects.len() {
            let num_secs = objects[f_idx].sections.len();
            for s_idx in 0..num_secs {
                let (num_relocs, is_eligible) = {
                    let sec = &objects[f_idx].sections[s_idx];
                    (
                        sec.relocations.len(),
                        sec.is_live
                            && sec.is_executable()
                            && !sec.data.is_empty()
                            && (mode != IcfMode::Safe || !address_taken.contains(&(f_idx, s_idx)))
                            && (mode == IcfMode::All || !indexed_targets.contains(&(f_idx, s_idx)))
                            // Identical raw symbol indices from different
                            // objects can designate different local targets.
                            // Folding these sections is unsafe without a
                            // relocation graph equivalence check.
                            && sec.relocations.iter().all(|r| r.symbol_index.is_none()),
                    )
                };

                if !is_eligible {
                    continue;
                }

                // Compute signature in place (no section data cloning).
                let hash = {
                    let sec = &objects[f_idx].sections[s_idx];
                    let mut hash = fnv1a_64(&sec.data);
                    hash ^= (num_relocs as u64).wrapping_mul(0x517cc1b727220a95);
                    for r in &sec.relocations {
                        hash ^= fnv1a_64(r.symbol_name.as_bytes());
                        hash ^= r.offset;
                    }
                    hash
                };

                if let Some(&(target_f, target_s, ref canon_name)) = hashes.get(&hash) {
                    if target_f != f_idx || target_s != s_idx {
                        // Byte-identical data AND identical relocation lists
                        // (same target symbols at the same offsets) are required
                        // before folding is sound.
                        let matches = {
                            let target_sec = &objects[target_f].sections[target_s];
                            let sec = &objects[f_idx].sections[s_idx];
                            target_sec.data == sec.data && target_sec.relocations == sec.relocations
                        };

                        if matches {
                            let obj_name = objects[f_idx].display_name();
                            let sec_name = objects[f_idx].sections[s_idx].name.clone();
                            objects[f_idx].sections[s_idx].is_folded = true;
                            objects[f_idx].sections[s_idx].folded_into = Some(canon_name.clone());
                            folded_count += 1;
                            if print_icf {
                                println!(
                                    "--icf: folded duplicate section `{}` in `{}` into `{}`",
                                    sec_name, obj_name, canon_name
                                );
                            }
                            continue;
                        }
                    }
                }

                // A COFF section's first symbol is usually the same-named
                // local `.text` section symbol, not the function. Using it as
                // an alias key collides with hundreds of other sections.
                let Some(sym_name) = objects[f_idx]
                    .symbols
                    .iter()
                    .find(|s| {
                        s.section_index == Some(s_idx)
                            && s.binding == SymbolBinding::Global
                            && !s.name.is_empty()
                    })
                    .map(|s| s.name.clone())
                else {
                    continue;
                };

                hashes.insert(hash, (f_idx, s_idx, sym_name));
            }
        }

        folded_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relocation::{Relocation, RelocationKind};
    use crate::section::Section;
    use crate::symbol::{Symbol, SymbolBinding, SymbolType};
    use std::path::PathBuf;

    #[test]
    fn test_icf_safe_vs_all_modes() {
        // Create an object with two identical functions `fn_a` (sec 0) and `fn_b` (sec 1)
        // and a data section (sec 2) that takes the address of `fn_b`.
        let mut obj = ObjectFile::new(PathBuf::from("test.o"), crate::target::Target::host(), 0);

        let mut sec0 =
            Section::new_code(".text.fn_a", vec![0xb8, 0x2a, 0x00, 0x00, 0x00, 0xc3], 16);
        sec0.is_live = true;
        let mut sec1 =
            Section::new_code(".text.fn_b", vec![0xb8, 0x2a, 0x00, 0x00, 0x00, 0xc3], 16);
        sec1.is_live = true;
        let mut sec2 = Section::new_data(".data", vec![0; 8], false, 8);
        sec2.is_live = true;
        sec2.relocations.push(Relocation {
            offset: 0,
            symbol_name: "fn_b".to_string(),
            symbol_index: Some(1),
            file_index: Some(0),
            kind: RelocationKind::Absolute64,
            addend: 0,
        });

        obj.add_section(sec0);
        obj.add_section(sec1);
        obj.add_section(sec2);

        obj.add_symbol(Symbol {
            name: "fn_a".to_string(),
            binding: SymbolBinding::Global,
            visibility: crate::symbol::SymbolVisibility::Default,
            sym_type: SymbolType::Function,
            section_index: Some(0),
            value: 0,
            size: 6,
            is_defined: true,
            is_imported: false,
            is_exported: false,
            file_index: Some(0),
            alias_of: None,
            comdat_group: None,
            version: None,
        });
        obj.add_symbol(Symbol {
            name: "fn_b".to_string(),
            binding: SymbolBinding::Global,
            visibility: crate::symbol::SymbolVisibility::Default,
            sym_type: SymbolType::Function,
            section_index: Some(1),
            value: 0,
            size: 6,
            is_defined: true,
            is_imported: false,
            is_exported: false,
            file_index: Some(0),
            alias_of: None,
            comdat_group: None,
            version: None,
        });

        // 1. In Safe mode: fn_b has its address taken in .data, so it must NOT be folded
        let mut objects_safe = vec![obj.clone()];
        let folded_safe = IcfEngine::fold_sections(&mut objects_safe, IcfMode::Safe, false);
        assert_eq!(folded_safe, 0);
        assert!(!objects_safe[0].sections[1].is_folded);

        // 2. In All mode: fn_b is folded despite address-taken status
        let mut objects_all = vec![obj];
        let folded_all = IcfEngine::fold_sections(&mut objects_all, IcfMode::All, false);
        assert_eq!(folded_all, 1);
        assert!(objects_all[0].sections[1].is_folded);
        assert_eq!(
            objects_all[0].sections[1].folded_into.as_deref(),
            Some("fn_a")
        );
    }
}
