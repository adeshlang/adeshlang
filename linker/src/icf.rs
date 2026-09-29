//! Identical Code Folding (ICF) pass.

use crate::config::IcfMode;
use crate::hash::fnv1a_64;
use crate::object::ObjectFile;
use std::collections::HashMap;

pub struct IcfEngine;

impl IcfEngine {
    pub fn fold_sections(objects: &mut [ObjectFile], mode: IcfMode, print_icf: bool) -> usize {
        if mode == IcfMode::None {
            return 0;
        }

        // Map: hash -> (file_index, section_index, canonical_symbol_name)
        let mut hashes: HashMap<u64, (usize, usize, String)> = HashMap::new();
        let mut folded_count = 0;

        for f_idx in 0..objects.len() {
            let num_secs = objects[f_idx].sections.len();
            for s_idx in 0..num_secs {
                let (sec_data, sec_name, num_relocs, is_eligible) = {
                    let sec = &objects[f_idx].sections[s_idx];
                    (
                        sec.data.clone(),
                        sec.name.clone(),
                        sec.relocations.len(),
                        sec.is_live && sec.is_executable() && !sec.data.is_empty(),
                    )
                };

                if !is_eligible {
                    continue;
                }

                // Compute signature: data hash + relocation count
                let mut hash = fnv1a_64(&sec_data);
                hash ^= (num_relocs as u64).wrapping_mul(0x517cc1b727220a95);
                {
                    let sec = &objects[f_idx].sections[s_idx];
                    for r in &sec.relocations {
                        hash ^= fnv1a_64(r.symbol_name.as_bytes());
                        hash ^= r.offset;
                    }
                }

                if let Some(&(target_f, target_s, ref canon_name)) = hashes.get(&hash) {
                    if target_f != f_idx || target_s != s_idx {
                        let matches = {
                            let target_sec = &objects[target_f].sections[target_s];
                            target_sec.data == sec_data
                                && target_sec.relocations.len() == num_relocs
                        };

                        if matches {
                            let obj_name = objects[f_idx].display_name();
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

                // First instance becomes canonical
                let sym_name = objects[f_idx]
                    .symbols
                    .iter()
                    .find(|s| s.section_index == Some(s_idx))
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| sec_name.clone());

                hashes.insert(hash, (f_idx, s_idx, sym_name));
            }
        }

        folded_count
    }
}
