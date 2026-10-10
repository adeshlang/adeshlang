use adesh_linker::object::ObjectFile;
use adesh_linker::resolver::SymbolResolver;
use adesh_linker::symbol::{Symbol, SymbolBinding, SymbolType, SymbolVisibility};
use adesh_linker::target::Target;
use std::path::PathBuf;

#[test]
fn test_symbol_resolution_strong_overrides_weak() {
    let mut resolver = SymbolResolver::new();
    let target = Target::host();

    let mut obj1 = ObjectFile::new(PathBuf::from("weak.o"), target.clone(), 0);
    obj1.add_symbol(Symbol {
        name: "foo".to_string(),
        binding: SymbolBinding::Weak,
        visibility: SymbolVisibility::Default,
        sym_type: SymbolType::Function,
        section_index: Some(0),
        value: 0x10,
        size: 32,
        is_defined: true,
        is_imported: false,
        is_exported: false,
        file_index: Some(0),
        alias_of: None,
        comdat_group: None,
        version: None,
    });

    let mut obj2 = ObjectFile::new(PathBuf::from("strong.o"), target.clone(), 1);
    obj2.add_symbol(Symbol {
        name: "foo".to_string(),
        binding: SymbolBinding::Global,
        visibility: SymbolVisibility::Default,
        sym_type: SymbolType::Function,
        section_index: Some(0),
        value: 0x50,
        size: 32,
        is_defined: true,
        is_imported: false,
        is_exported: false,
        file_index: Some(1),
        alias_of: None,
        comdat_group: None,
        version: None,
    });

    let mut objects = vec![obj1, obj2];
    assert!(resolver.resolve(&mut objects, &[]).is_ok());

    let resolved = resolver.lookup("foo").unwrap();
    assert_eq!(resolved.defined_in_file_index, 1);
    assert_eq!(resolved.symbol.value, 0x50);
}

#[test]
fn test_duplicate_symbol_error() {
    let mut resolver = SymbolResolver::new();
    let target = Target::host();

    let mut obj1 = ObjectFile::new(PathBuf::from("a.o"), target.clone(), 0);
    obj1.add_symbol(Symbol::new_defined(
        "duplicate_sym",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        10,
        0,
    ));

    let mut obj2 = ObjectFile::new(PathBuf::from("b.o"), target.clone(), 1);
    obj2.add_symbol(Symbol::new_defined(
        "duplicate_sym",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        10,
        1,
    ));

    let mut objects = vec![obj1, obj2];
    let res = resolver.resolve(&mut objects, &[]);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.code, adesh_linker::error::ErrorCode::DuplicateSymbol);
}

#[test]
fn test_unreferenced_undefined_weak_symbol_is_not_a_link_warning() {
    let mut resolver = SymbolResolver::new();
    let target = Target::host();
    let mut object = ObjectFile::new(PathBuf::from("weak_optional.o"), target.clone(), 0);
    let mut optional = Symbol::new_undefined("optional_runtime_helper", 0);
    optional.binding = SymbolBinding::Weak;
    object.add_symbol(optional);

    let mut objects = vec![object];
    resolver
        .resolve(&mut objects, &[])
        .expect("an undefined weak symbol is ABI-valid");

    assert!(
        resolver
            .weak_undefined_symbols
            .contains("optional_runtime_helper")
    );
    assert!(
        resolver.warnings.is_empty(),
        "weak declarations without relocations should not produce user-facing warnings"
    );
}

#[test]
fn test_elf_section_symbol_relocation_resolution() {
    use adesh_linker::relocation::{Relocation, RelocationKind};
    use adesh_linker::section::Section;

    let mut resolver = SymbolResolver::new();
    let target = Target::host();
    let mut object = ObjectFile::new(PathBuf::from("runtime_bss.o"), target, 0);

    let sec_name = ".bss._RNvNtCs_13adesh_runtime19TRACKED_ALLOCATIONS";
    let mut code_sec = Section::new_code(".text", vec![0x90; 16], 16);
    let mut reloc = Relocation::new(0, sec_name, RelocationKind::PcRelative32, -4);
    reloc.symbol_index = Some(1);
    reloc.file_index = Some(0);
    code_sec.relocations.push(reloc);
    object.add_section(code_sec);

    let bss_sec = Section::new_data(sec_name, vec![0u8; 8], false, 8);
    object.add_section(bss_sec);

    // Dummy symbol 0 (STN_UNDEF)
    object.add_symbol(Symbol {
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

    // Symbol 1: section symbol with section name
    object.add_symbol(Symbol {
        name: sec_name.to_string(),
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

    let mut objects = vec![object];
    assert!(
        resolver.resolve(&mut objects, &[]).is_ok(),
        "locally defined section symbol must not be marked undefined"
    );
    assert!(
        !resolver.undefined.contains(sec_name),
        "section symbol should be resolved by local definition"
    );
}
