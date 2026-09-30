//! Section Garbage Collection (`--gc-sections`) using reachability graph analysis.

use crate::object::ObjectFile;
use std::collections::{HashMap, HashSet, VecDeque};

pub struct GarbageCollector;

impl GarbageCollector {
    /// Perform dead code elimination by tracing reachability from roots.
    pub fn collect_dead_sections(
        objects: &mut [ObjectFile],
        root_symbols: &[String],
        print_gc: bool,
    ) -> usize {
        let mut live_sections: HashSet<(usize, usize)> = HashSet::new(); // (file_index, section_index)
        let mut worklist: VecDeque<(usize, usize)> = VecDeque::new();

        // Build a symbol name -> defining (file, section) index once, so the
        // BFS below is O(1) per relocation instead of scanning every object
        // and every symbol for each relocation (previously O(relocs * symbols)).
        let mut sym_defs: HashMap<&str, Vec<(usize, usize)>> = HashMap::new();
        for (f_idx, obj) in objects.iter().enumerate() {
            for sym in &obj.symbols {
                if sym.is_defined {
                    if let Some(s_idx) = sym.section_index {
                        sym_defs
                            .entry(sym.name.as_str())
                            .or_default()
                            .push((f_idx, s_idx));
                    }
                }
            }
        }

        let root_set: HashSet<&str> = root_symbols.iter().map(|s| s.as_str()).collect();

        // 1. Identify roots (entry point, exported symbols, init sections, metadata)
        for (f_idx, obj) in objects.iter().enumerate() {
            for (s_idx, sec) in obj.sections.iter().enumerate() {
                // Keep metadata, notes, and init sections unconditionally
                if sec.name == ".adesh.meta"
                    || sec.name.starts_with(".init")
                    || sec.name.starts_with(".fini")
                    || sec.name.starts_with(".note")
                {
                    live_sections.insert((f_idx, s_idx));
                    worklist.push_back((f_idx, s_idx));
                }
            }

            for sym in &obj.symbols {
                if sym.is_defined {
                    if root_set.contains(sym.name.as_str()) || sym.is_exported {
                        if let Some(s_idx) = sym.section_index {
                            if live_sections.insert((f_idx, s_idx)) {
                                worklist.push_back((f_idx, s_idx));
                            }
                        }
                    }
                }
            }
        }

        // 2. Breadth-First Graph Traversal of Relocation References
        while let Some((f_idx, s_idx)) = worklist.pop_front() {
            let sec = &objects[f_idx].sections[s_idx];
            for reloc in &sec.relocations {
                if let Some(defs) = sym_defs.get(reloc.symbol_name.as_str()) {
                    for &(target_f_idx, target_s_idx) in defs {
                        if live_sections.insert((target_f_idx, target_s_idx)) {
                            worklist.push_back((target_f_idx, target_s_idx));
                        }
                    }
                }
            }
        }

        // 3. Mark unreachable sections as dead
        let mut removed_count = 0;
        for (f_idx, obj) in objects.iter_mut().enumerate() {
            let obj_name = obj.display_name();
            for (s_idx, sec) in obj.sections.iter_mut().enumerate() {
                if !live_sections.contains(&(f_idx, s_idx)) {
                    sec.is_live = false;
                    removed_count += 1;
                    if print_gc {
                        println!(
                            "--gc-sections: removed dead section `{}` from `{}`",
                            sec.name, obj_name
                        );
                    }
                }
            }
        }

        removed_count
    }
}
