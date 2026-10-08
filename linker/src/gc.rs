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

        // Build symbol definition maps:
        // 1. File-local definitions: (file_index, symbol_name) -> section_index
        let mut file_local_defs: HashMap<(usize, &str), usize> = HashMap::new();
        // 2. Authoritative global definitions: symbol_name -> (file_index, section_index)
        // Strong definitions override weak definitions; first strong definition wins.
        let mut global_defs: HashMap<&str, (usize, usize)> = HashMap::new();
        let mut global_is_weak: HashMap<&str, bool> = HashMap::new();

        for (f_idx, obj) in objects.iter().enumerate() {
            for sym in &obj.symbols {
                if sym.is_defined {
                    if let Some(s_idx) = sym.section_index {
                        if sym.is_local() {
                            file_local_defs.insert((f_idx, sym.name.as_str()), s_idx);
                        } else {
                            let is_weak = sym.binding == crate::symbol::SymbolBinding::Weak;
                            if let Some(&existing_weak) = global_is_weak.get(sym.name.as_str()) {
                                if existing_weak && !is_weak {
                                    global_defs.insert(sym.name.as_str(), (f_idx, s_idx));
                                    global_is_weak.insert(sym.name.as_str(), false);
                                }
                            } else {
                                global_defs.insert(sym.name.as_str(), (f_idx, s_idx));
                                global_is_weak.insert(sym.name.as_str(), is_weak);
                            }
                        }
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
                    || (sec.flags & crate::section::flags::TLS) != 0
                    || sec.name.starts_with(".CRT$")
                {
                    live_sections.insert((f_idx, s_idx));
                    worklist.push_back((f_idx, s_idx));
                }
            }

            for sym in &obj.symbols {
                if sym.is_defined {
                    // Public symbols from extracted static-archive members
                    // are not executable roots by themselves. Keep archive
                    // code only when referenced (or explicitly named as a
                    // root); otherwise a runtime archive's entire public API
                    // defeats section-level dead-code elimination.
                    if root_set.contains(sym.name.as_str())
                        || (sym.is_exported && !obj.is_archive_member)
                    {
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
                let mut resolved_precisely = false;
                if let Some(si) = reloc.symbol_index {
                    if let Some(obj) = objects.get(f_idx) {
                        if let Some(sym) = obj.symbols.get(si) {
                            if sym.is_defined {
                                if sym.is_local() {
                                    if let Some(target_s_idx) = sym.section_index {
                                        if target_s_idx < obj.sections.len()
                                            && live_sections.insert((f_idx, target_s_idx))
                                        {
                                            worklist.push_back((f_idx, target_s_idx));
                                        }
                                        resolved_precisely = true;
                                    }
                                } else if let Some(&(target_f_idx, target_s_idx)) =
                                    global_defs.get(sym.name.as_str())
                                {
                                    if live_sections.insert((target_f_idx, target_s_idx)) {
                                        worklist.push_back((target_f_idx, target_s_idx));
                                    }
                                    resolved_precisely = true;
                                }
                            }
                        }
                    }
                }

                if !resolved_precisely {
                    // Name-only fallback (all ADOB relocations are name-only).
                    // A name may resolve to a file-local def in the referencing
                    // file *or* to the authoritative global def; which one the
                    // resolver binds cannot be told apart from the name alone.
                    // Retain BOTH candidates: retaining an extra section is
                    // harmless, while dropping the section the resolver
                    // actually binds to would produce a broken binary.
                    if let Some(&target_s_idx) =
                        file_local_defs.get(&(f_idx, reloc.symbol_name.as_str()))
                    {
                        if live_sections.insert((f_idx, target_s_idx)) {
                            worklist.push_back((f_idx, target_s_idx));
                        }
                    }
                    if let Some(&(target_f_idx, target_s_idx)) =
                        global_defs.get(reloc.symbol_name.as_str())
                    {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::ObjectFile;
    use crate::relocation::{Relocation, RelocationKind};
    use crate::section::Section;
    use crate::symbol::{Symbol, SymbolBinding, SymbolType};
    use crate::target::Target;
    use std::path::PathBuf;

    /// Build an object with one `.text`-like code section per function name.
    /// `exported` functions become global, exported symbols; the rest are
    /// global, non-exported symbols.
    fn object_with_funcs(path: &str, file_index: usize, funcs: &[(&str, bool)]) -> ObjectFile {
        let mut obj = ObjectFile::new(
            PathBuf::from(path),
            Target::from_triple("x86_64-unknown-linux-gnu").unwrap(),
            file_index,
        );
        for (name, exported) in funcs {
            let s_idx = obj.add_section(Section::new_code(
                format!(".text.{name}"),
                vec![0x90, 0xc3],
                16,
            ));
            let mut sym = Symbol::new_defined(
                *name,
                SymbolBinding::Global,
                SymbolType::Function,
                s_idx,
                0,
                2,
                file_index,
            );
            sym.is_exported = *exported;
            obj.symbols.push(sym);
        }
        obj
    }

    #[test]
    fn gc_removes_unreachable_function_section() {
        let mut objects = vec![
            object_with_funcs("main.o", 0, &[("main", true)]),
            object_with_funcs("dead.o", 1, &[("dead_func", false)]),
        ];
        let removed =
            GarbageCollector::collect_dead_sections(&mut objects, &["main".to_string()], false);
        assert_eq!(removed, 1, "exactly the dead section must be collected");
        assert!(objects[0].sections[0].is_live, "main must stay live");
        assert!(
            !objects[1].sections[0].is_live,
            "unreferenced dead_func must be collected"
        );
    }

    #[test]
    fn gc_keeps_section_reachable_via_name_only_reloc() {
        let mut main_obj = object_with_funcs("main.o", 0, &[("main", true)]);
        main_obj.sections[0].relocations.push(Relocation::new(
            0,
            "helper",
            RelocationKind::PcRelative32,
            -4,
        ));
        let mut objects = vec![
            main_obj,
            object_with_funcs("helper.o", 1, &[("helper", false), ("unused", false)]),
        ];
        let removed =
            GarbageCollector::collect_dead_sections(&mut objects, &["main".to_string()], false);
        assert_eq!(removed, 1);
        assert!(objects[1].sections[0].is_live, "helper must be retained");
        assert!(
            !objects[1].sections[1].is_live,
            "unused must be collected even though it shares a file with helper"
        );
    }

    #[test]
    fn gc_discards_unreferenced_exported_archive_member_sections() {
        let mut main_obj = object_with_funcs("main.o", 0, &[("main", true)]);
        main_obj.sections[0].relocations.push(Relocation::new(
            0,
            "needed_runtime",
            RelocationKind::PcRelative32,
            -4,
        ));
        let mut runtime_obj = object_with_funcs(
            "runtime.o",
            1,
            &[("needed_runtime", false), ("unused_runtime", false)],
        );
        runtime_obj.is_archive_member = true;
        runtime_obj.symbols[0].is_exported = true;
        runtime_obj.symbols[1].is_exported = true;
        let mut objects = vec![main_obj, runtime_obj];

        let removed =
            GarbageCollector::collect_dead_sections(&mut objects, &["main".to_string()], false);
        assert_eq!(removed, 1);
        assert!(objects[1].sections[0].is_live);
        assert!(!objects[1].sections[1].is_live);
    }

    #[test]
    fn gc_exported_symbols_are_roots() {
        let mut objects = vec![object_with_funcs(
            "lib.o",
            0,
            &[("api_fn", true), ("internal_fn", false)],
        )];
        let removed = GarbageCollector::collect_dead_sections(&mut objects, &[], false);
        assert_eq!(removed, 1);
        assert!(objects[0].sections[0].is_live, "exported fn is a GC root");
        assert!(!objects[0].sections[1].is_live);
    }

    #[test]
    fn gc_name_fallback_retains_both_local_and_global_defs() {
        // Referencing file has a file-local symbol named `dup`; another file
        // defines a global `dup`. The name-only fallback cannot tell which one
        // the resolver will bind, so both sections must survive.
        let mut main_obj = object_with_funcs("main.o", 0, &[("main", true)]);
        main_obj.sections[0].relocations.push(Relocation::new(
            0,
            "dup",
            RelocationKind::PcRelative32,
            -4,
        ));
        // File-local `dup` in the referencing file.
        let local_s_idx = main_obj.add_section(Section::new_code(".text.dup", vec![0xc3], 16));
        main_obj.symbols.push(Symbol::new_defined(
            "dup",
            SymbolBinding::Local,
            SymbolType::Function,
            local_s_idx,
            0,
            1,
            0,
        ));
        let mut objects = vec![main_obj, object_with_funcs("other.o", 1, &[("dup", false)])];
        let removed =
            GarbageCollector::collect_dead_sections(&mut objects, &["main".to_string()], false);
        assert_eq!(removed, 0, "both candidates must be retained");
        assert!(objects[0].sections[1].is_live, "file-local def retained");
        assert!(objects[1].sections[0].is_live, "global def retained");
    }

    #[test]
    fn gc_strong_def_wins_over_weak_for_reachability() {
        let mut main_obj = object_with_funcs("main.o", 0, &[("main", true)]);
        main_obj.sections[0].relocations.push(Relocation::new(
            0,
            "maybe_weak",
            RelocationKind::PcRelative32,
            -4,
        ));
        let mut weak_obj = object_with_funcs("weak.o", 1, &[("maybe_weak", false)]);
        weak_obj.symbols[0].binding = SymbolBinding::Weak;
        let mut objects = vec![
            main_obj,
            weak_obj,
            object_with_funcs("strong.o", 2, &[("maybe_weak", false)]),
        ];
        let removed =
            GarbageCollector::collect_dead_sections(&mut objects, &["main".to_string()], false);
        // The strong def is authoritative; the weak def is not retained by the
        // fallback, only the strong one is.
        assert!(
            objects[2].sections[0].is_live,
            "strong def must be retained"
        );
        assert!(
            !objects[1].sections[0].is_live,
            "weak def shadowed by strong def is collectable"
        );
        assert_eq!(removed, 1);
    }
}
