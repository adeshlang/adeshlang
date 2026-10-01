use adesh_linker::config::{IcfMode, LinkConfig, OptLevel};
use adesh_linker::gc::GarbageCollector;
use adesh_linker::icf::IcfEngine;
use adesh_linker::layout::LayoutEngine;
use adesh_linker::object::ObjectFile;
use adesh_linker::relocation::{Relocation, RelocationKind};
use adesh_linker::resolver::SymbolResolver;
use adesh_linker::section::{MergedSection, Section, SectionKind, flags};
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

#[test]
fn test_indexed_local_relocation_marks_only_its_section_live() {
    let target = Target::host();
    let mut obj = ObjectFile::new(PathBuf::from("sections.o"), target, 0);
    let mut main = Section::new_code(".text", vec![0, 0, 0, 0], 4);
    let mut reloc = Relocation::new(0, ".text", RelocationKind::PcRelative32, -4);
    reloc.symbol_index = Some(2);
    main.relocations.push(reloc);
    obj.add_section(main);
    obj.add_section(Section::new_code(".text", vec![0xCC], 1));
    obj.add_section(Section::new_code(".text", vec![0xC3], 1));
    obj.add_symbol(Symbol::new_defined(
        "main",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        4,
        0,
    ));
    obj.add_symbol(Symbol::new_defined(
        ".text",
        SymbolBinding::Local,
        SymbolType::Object,
        1,
        0,
        1,
        0,
    ));
    obj.add_symbol(Symbol::new_defined(
        ".text",
        SymbolBinding::Local,
        SymbolType::Object,
        2,
        0,
        1,
        0,
    ));
    let mut objects = vec![obj];
    GarbageCollector::collect_dead_sections(&mut objects, &["main".into()], false);
    assert!(!objects[0].sections[1].is_live);
    assert!(objects[0].sections[2].is_live);
}

#[test]
fn test_tls_zero_fill_and_alignment_survive_section_merge() {
    let mut merged = MergedSection::new(".tls", SectionKind::TData, flags::TLS, 8);
    let initialized = Section::new_data(".tls$AAA", vec![1, 2], true, 8);
    let mut zero_fill = Section::new_bss(".tls$ZZZ", 16, 8);
    zero_fill.kind = SectionKind::TBss;
    assert_eq!(merged.append_section(&initialized, 0, 0), 0);
    assert_eq!(merged.append_section(&zero_fill, 0, 1), 8);
    assert_eq!(merged.data.len(), 2);
    assert_eq!(merged.size, 24);
}

#[test]
fn test_optimization_levels_select_portable_passes() {
    let mut config = LinkConfig::default();
    config.apply_optimization_level(OptLevel::O0);
    assert!(!config.gc_sections);
    assert_eq!(config.icf, IcfMode::None);
    config.apply_optimization_level(OptLevel::O2);
    assert!(config.gc_sections);
    assert_eq!(config.icf, IcfMode::Safe);
    config.apply_optimization_level(OptLevel::Oz);
    assert!(config.strip);
    assert_eq!(config.icf, IcfMode::All);
}

#[test]
fn test_elf_tls_requires_loader_metadata_instead_of_silent_shared_storage() {
    let target = Target::x86_64_linux();
    let mut obj = ObjectFile::new(PathBuf::from("tls.o"), target.clone(), 0);
    let mut tls = Section::new_data(".tdata", vec![42], true, 8);
    tls.kind = SectionKind::TData;
    tls.flags |= flags::TLS;
    obj.add_section(tls);
    let error = LayoutEngine::new()
        .layout_and_relocate(&[obj], &SymbolResolver::new(), &target, "main")
        .unwrap_err();
    assert!(error.to_string().contains("TLS input sections"));
}
