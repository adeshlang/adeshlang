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
                            && !indexed_targets.contains(&(f_idx, s_idx))
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
