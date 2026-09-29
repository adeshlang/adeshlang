use adesh_linker::config::IcfMode;
use adesh_linker::gc::GarbageCollector;
use adesh_linker::icf::IcfEngine;
use adesh_linker::object::ObjectFile;
use adesh_linker::section::Section;
use adesh_linker::symbol::{Symbol, SymbolBinding, SymbolType};
use adesh_linker::target::Target;
use std::path::PathBuf;

#[test]
fn test_gc_sections_purges_unreferenced() {
    let target = Target::host();
    let mut obj = ObjectFile::new(PathBuf::from("test.o"), target, 0);

    let sec_live = Section::new_code(".text.main", vec![0x90, 0xC3], 16);
    let sec_dead = Section::new_code(".text.unused", vec![0xCC, 0xCC], 16);

    obj.add_section(sec_live);
    obj.add_section(sec_dead);

    obj.add_symbol(Symbol::new_defined(
        "main",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        2,
        0,
    ));
    obj.add_symbol(Symbol::new_defined(
        "unused_func",
        SymbolBinding::Global,
        SymbolType::Function,
        1,
        0,
        2,
        0,
    ));

    let mut objects = vec![obj];
    let removed =
        GarbageCollector::collect_dead_sections(&mut objects, &["main".to_string()], false);

    assert_eq!(removed, 1);
    assert!(objects[0].sections[0].is_live);
    assert!(!objects[0].sections[1].is_live);
}

#[test]
fn test_icf_folds_identical_code() {
    let target = Target::host();
    let mut obj = ObjectFile::new(PathBuf::from("test.o"), target, 0);

    let code = vec![0x48, 0x31, 0xC0, 0xC3]; // xor rax, rax; ret
    let sec1 = Section::new_code(".text.func_a", code.clone(), 16);
    let sec2 = Section::new_code(".text.func_b", code.clone(), 16);

    obj.add_section(sec1);
    obj.add_section(sec2);

    obj.add_symbol(Symbol::new_defined(
        "func_a",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        4,
        0,
    ));
    obj.add_symbol(Symbol::new_defined(
        "func_b",
        SymbolBinding::Global,
        SymbolType::Function,
        1,
        0,
        4,
        0,
    ));

    let mut objects = vec![obj];
    let folded = IcfEngine::fold_sections(&mut objects, IcfMode::Safe, false);

    assert_eq!(folded, 1);
    assert!(!objects[0].sections[0].is_folded);
    assert!(objects[0].sections[1].is_folded);
}
