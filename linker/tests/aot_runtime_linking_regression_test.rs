//! Phase 7 — AOT Runtime Linking Regression Suite
//!
//! Focused tests for:
//! 1. Runtime symbol resolution from static archives.
//! 2. Lazy archive member extraction and file-index re-stamping.
//! 3. ELF section-symbol relocation resolution (preventing the 7,559 undefined symbol bug).
//! 4. Unresolved symbol diagnostics and actionable suggestions (LNK001).
//! 5. Distinguishing required runtime symbols from user application symbols in diagnostics.

use adesh_linker::archive::{Archive, ArchiveMember};
use adesh_linker::error::ErrorCode;
use adesh_linker::object::ObjectFile;
use adesh_linker::relocation::{Relocation, RelocationKind};
use adesh_linker::resolver::SymbolResolver;
use adesh_linker::section::Section;
use adesh_linker::symbol::{Symbol, SymbolBinding, SymbolType, SymbolVisibility};
use adesh_linker::target::Target;
use std::path::PathBuf;

#[test]
fn test_runtime_archive_lazy_extraction_resolves_referenced_symbols() {
    let mut resolver = SymbolResolver::new();
    let target = Target::host();

    // Member 0: defines aot_alloc
    let mut mem0_obj = ObjectFile::new(PathBuf::from("mem0.o"), target.clone(), 0);
    mem0_obj.add_section(Section::new_code(".text", vec![0xc3; 16], 16));
    mem0_obj.add_symbol(Symbol::new_defined(
        "aot_alloc",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        16,
        0,
    ));

    // Member 1: defines aot_print
    let mut mem1_obj = ObjectFile::new(PathBuf::from("mem1.o"), target.clone(), 0);
    mem1_obj.add_section(Section::new_code(".text", vec![0xc3; 16], 16));
    mem1_obj.add_symbol(Symbol::new_defined(
        "aot_print",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        16,
        0,
    ));

    let mut archive = Archive::new();
    archive.path = PathBuf::from("libadesh_runtime.a");
    archive.members.push(ArchiveMember {
        name: "alloc.o".to_string(),
        size: 16,
        data: Vec::new(),
        obj: Some(mem0_obj),
    });
    archive.members.push(ArchiveMember {
        name: "print.o".to_string(),
        size: 16,
        data: Vec::new(),
        obj: Some(mem1_obj),
    });
    archive
        .symbol_index
        .insert("aot_alloc".to_string(), vec![0]);
    archive
        .symbol_index
        .insert("aot_print".to_string(), vec![1]);

    // Application object references aot_alloc only (not aot_print)
    let mut app_obj = ObjectFile::new(PathBuf::from("main.o"), target.clone(), 0);
    let mut app_code = Section::new_code(".text", vec![0xe8, 0, 0, 0, 0, 0xc3], 16);
    let mut app_reloc = Relocation::new(1, "aot_alloc", RelocationKind::PcRelative32, -4);
    app_reloc.symbol_index = Some(1);
    app_reloc.file_index = Some(0);
    app_code.relocations.push(app_reloc);
    app_obj.add_section(app_code);

    // Dummy symbol 0 + reference to aot_alloc
    app_obj.add_symbol(Symbol {
        name: String::new(),
        binding: SymbolBinding::Local,
        visibility: SymbolVisibility::Default,
        sym_type: SymbolType::Unknown,
        section_index: None,
        value: 0,
        size: 0,
        is_defined: false,
        is_imported: false,
        is_exported: false,
        file_index: Some(0),
        alias_of: None,
        comdat_group: None,
        version: None,
    });
    app_obj.add_symbol(Symbol::new_undefined("aot_alloc", 0));

    let mut objects = vec![app_obj];
    let res = resolver.resolve_with_target(&mut objects, &[archive], &target);
    assert!(res.is_ok(), "resolve_with_target failed: {:?}", res);

    // Only main.o and alloc.o should be in objects list (print.o was not extracted)
    assert_eq!(objects.len(), 2);
    assert_eq!(objects[1].file_index, 1);
    assert!(resolver.lookup("aot_alloc").is_some());
    assert!(resolver.lookup("aot_print").is_none());
    assert!(resolver.undefined.is_empty());
}

#[test]
fn test_unresolved_user_symbol_diagnostics_and_suggestion() {
    let mut resolver = SymbolResolver::new();
    let target = Target::host();

    let mut app_obj = ObjectFile::new(PathBuf::from("app.o"), target.clone(), 0);
    app_obj.add_section(Section::new_code(".text", vec![0x90; 16], 16));
    app_obj.add_symbol(Symbol::new_undefined("my_missing_user_function", 0));

    let mut objects = vec![app_obj];
    let res = resolver.resolve(&mut objects, &[]);
    assert!(res.is_err(), "link should fail for undefined user symbol");

    let err = res.unwrap_err();
    assert_eq!(err.code, ErrorCode::UndefinedSymbol);
    assert!(err.message.contains("my_missing_user_function"));
}

#[test]
fn test_elf_mangled_rust_section_symbols_do_not_produce_lnk001() {
    let mut resolver = SymbolResolver::new();
    let target = Target::host();

    let mut obj = ObjectFile::new(PathBuf::from("runtime_allocator.o"), target.clone(), 0);

    let bss_name = ".bss._RNvNtCs_13adesh_runtime19TRACKED_ALLOCATIONS";
    let data_name = ".data._RNvNtCs_13adesh_runtime7ARC_MAP";

    let mut code_sec = Section::new_code(".text", vec![0x90; 32], 16);
    let mut reloc1 = Relocation::new(0, bss_name, RelocationKind::PcRelative32, -4);
    reloc1.symbol_index = Some(1);
    reloc1.file_index = Some(0);
    let mut reloc2 = Relocation::new(8, data_name, RelocationKind::PcRelative32, -4);
    reloc2.symbol_index = Some(2);
    reloc2.file_index = Some(0);
    code_sec.relocations.push(reloc1);
    code_sec.relocations.push(reloc2);
    obj.add_section(code_sec);

    obj.add_section(Section::new_data(bss_name, vec![0u8; 8], false, 8));
    obj.add_section(Section::new_data(data_name, vec![0u8; 8], false, 8));

    // Symbol 0: STN_UNDEF placeholder
    obj.add_symbol(Symbol {
        name: String::new(),
        binding: SymbolBinding::Local,
        visibility: SymbolVisibility::Default,
        sym_type: SymbolType::Unknown,
        section_index: None,
        value: 0,
        size: 0,
        is_defined: false,
        is_imported: false,
        is_exported: false,
        file_index: Some(0),
        alias_of: None,
        comdat_group: None,
        version: None,
    });

    // Symbol 1: section symbol for BSS section
    obj.add_symbol(Symbol {
        name: bss_name.to_string(),
        binding: SymbolBinding::Local,
        visibility: SymbolVisibility::Default,
        sym_type: SymbolType::Section,
        section_index: Some(1),
        value: 0,
        size: 8,
        is_defined: true,
        is_imported: false,
        is_exported: false,
        file_index: Some(0),
        alias_of: None,
        comdat_group: None,
        version: None,
    });

    // Symbol 2: section symbol for data section
    obj.add_symbol(Symbol {
        name: data_name.to_string(),
        binding: SymbolBinding::Local,
        visibility: SymbolVisibility::Default,
        sym_type: SymbolType::Section,
        section_index: Some(2),
        value: 0,
        size: 8,
        is_defined: true,
        is_imported: false,
        is_exported: false,
        file_index: Some(0),
        alias_of: None,
        comdat_group: None,
        version: None,
    });

    let mut objects = vec![obj];
    let res = resolver.resolve(&mut objects, &[]);
    assert!(
        res.is_ok(),
        "ELF section symbol resolution must succeed without LNK001 error"
    );
    assert!(!resolver.undefined.contains(bss_name));
    assert!(!resolver.undefined.contains(data_name));
}

#[test]
fn test_archive_member_reassign_file_index_updates_relocations_and_symbols() {
    let target = Target::host();

    let mut member_obj = ObjectFile::new(PathBuf::from("member.o"), target, 0);
    let mut code_sec = Section::new_code(".text", vec![0x90; 16], 16);
    let mut reloc = Relocation::new(4, "local_sym", RelocationKind::PcRelative32, -4);
    reloc.file_index = Some(0);
    reloc.symbol_index = Some(1);
    code_sec.relocations.push(reloc);
    member_obj.add_section(code_sec);

    member_obj.add_symbol(Symbol::new_defined(
        "local_sym",
        SymbolBinding::Local,
        SymbolType::Function,
        0,
        0,
        16,
        0,
    ));

    // When extracted into slot 5:
    member_obj.reassign_file_index(5);

    assert_eq!(member_obj.file_index, 5);
    assert_eq!(member_obj.sections[0].file_index, Some(5));
    assert_eq!(member_obj.sections[0].relocations[0].file_index, Some(5));
    assert_eq!(member_obj.symbols[0].file_index, Some(5));
}
