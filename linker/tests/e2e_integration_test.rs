use adesh_linker::abi::{FunctionUnwindDescriptor, ScopeUnwindAction, UnwindActionTable};
use adesh_linker::archive::Archive;
use adesh_linker::config::{IcfMode, LinkConfig};
use adesh_linker::gc::GarbageCollector;
use adesh_linker::icf::IcfEngine;
use adesh_linker::linker::Linker;
use adesh_linker::object::{ObjectFile, ObjectWriter};
use adesh_linker::relocation::{Relocation, RelocationKind};
use adesh_linker::section::Section;
use adesh_linker::symbol::{Symbol, SymbolBinding, SymbolType};
use adesh_linker::target::Target;
use std::path::PathBuf;
use tempfile::tempdir;

/// Helper to create a synthetic native code object file.
fn create_code_object(
    path: PathBuf,
    target: Target,
    fn_name: &str,
    code_bytes: Vec<u8>,
    calls: Vec<(&str, u64)>, // (callee_symbol, relocation_offset)
) -> ObjectFile {
    let mut obj = ObjectFile::new(path, target.clone(), 0);

    let mut text_sec = Section::new_code(".text", code_bytes, 16);
    for (callee, off) in calls {
        text_sec.relocations.push(Relocation::new(
            off,
            callee,
            RelocationKind::PcRelative32,
            -4,
        ));
    }

    let sym = Symbol::new_defined(
        fn_name,
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        text_sec.size,
        0,
    );

    obj.sections.push(text_sec);
    obj.symbols.push(sym);
    obj
}

#[test]
fn test_e2e_hello_world_executable_pipeline() {
    let dir = tempdir().expect("Failed to create tempdir");
    let target = Target::x86_64_linux();

    // 1. main.o (calls println)
    let main_code = vec![
        0x48, 0x83, 0xec, 0x08, 0xe8, 0x00, 0x00, 0x00, 0x00, 0x48, 0x83, 0xc4, 0x08, 0xc3,
    ];
    let main_obj_file = dir.path().join("main.o");
    let main_obj = create_code_object(
        main_obj_file.clone(),
        target.clone(),
        "main",
        main_code,
        vec![("println", 5)],
    );
    let bytes = ObjectWriter::encode(&main_obj).expect("Failed to write main.o");
    std::fs::write(&main_obj_file, bytes).unwrap();

    // 2. runtime.o (defines println)
    let rt_code = vec![0x48, 0x31, 0xc0, 0xc3]; // xor rax, rax; ret
    let rt_obj_file = dir.path().join("runtime.o");
    let rt_obj = create_code_object(
        rt_obj_file.clone(),
        target.clone(),
        "println",
        rt_code,
        vec![],
    );
    let rt_bytes = ObjectWriter::encode(&rt_obj).expect("Failed to write runtime.o");
    std::fs::write(&rt_obj_file, rt_bytes).unwrap();

    // 3. Link executable using adeshlink
    let out_file = dir.path().join("app.out");
    let mut config = LinkConfig::new(out_file.clone(), target.clone());
    config.entry_point = Some("main".to_string());

    Linker::link(&[main_obj_file, rt_obj_file], config).expect("Failed to link executable");

    assert!(out_file.exists());
    let bin_data = std::fs::read(&out_file).unwrap();
    // Validate ELF magic
    assert!(bin_data.starts_with(b"\x7fELF"));
}

#[test]
fn test_e2e_multi_module_static_archive_resolution() {
    let dir = tempdir().expect("Failed to create tempdir");
    let target = Target::x86_64_linux();

    // Math module 1
    let math_obj = create_code_object(
        dir.path().join("math.o"),
        target.clone(),
        "adesh_add",
        vec![0x48, 0x01, 0xf0, 0xc3],
        vec![],
    );
    let math_bytes = ObjectWriter::encode(&math_obj).unwrap();

    // String module 2
    let str_obj = create_code_object(
        dir.path().join("string.o"),
        target.clone(),
        "adesh_strlen",
        vec![0x48, 0x89, 0xf8, 0xc3],
        vec![],
    );
    let str_bytes = ObjectWriter::encode(&str_obj).unwrap();

    // Pack into static archive `libstd.a`
    let ar_path = dir.path().join("libstd.a");
    let mut archive = Archive::new();
    archive.add_file("math.o", math_bytes);
    archive.add_file("string.o", str_bytes);
    let ar_encoded = archive.encode_gnu();
    std::fs::write(&ar_path, ar_encoded).unwrap();

    // Parse archive and verify member symbol indexing
    let parsed_ar = Archive::parse(&std::fs::read(&ar_path).unwrap(), &ar_path).unwrap();
    assert_eq!(parsed_ar.members.len(), 2);
    assert_eq!(parsed_ar.members[0].name, "math.o");
    assert_eq!(parsed_ar.members[1].name, "string.o");
}

#[test]
fn test_e2e_dead_code_elimination_size_reduction() {
    let target = Target::x86_64_linux();
    let mut obj = ObjectFile::new(PathBuf::from("gc_test.o"), target, 0);

    // Live main section
    let mut main_sec = Section::new_code(".text.main", vec![0x90; 32], 16);
    main_sec.relocations.push(Relocation::new(
        0,
        "used_helper",
        RelocationKind::PcRelative32,
        -4,
    ));
    obj.sections.push(main_sec);
    obj.symbols.push(Symbol::new_defined(
        "main",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        32,
        0,
    ));

    // Live helper section
    let helper_sec = Section::new_code(".text.used_helper", vec![0x90; 32], 16);
    obj.sections.push(helper_sec);
    obj.symbols.push(Symbol::new_defined(
        "used_helper",
        SymbolBinding::Global,
        SymbolType::Function,
        1,
        0,
        32,
        0,
    ));

    // Dead section (unreferenced)
    let dead_sec = Section::new_code(".text.dead_function", vec![0xcc; 1024], 16);
    obj.sections.push(dead_sec);
    obj.symbols.push(Symbol::new_defined(
        "dead_function",
        SymbolBinding::Global,
        SymbolType::Function,
        2,
        0,
        1024,
        0,
    ));

    let mut objs = vec![obj];
    let roots = vec!["main".to_string()];
    let removed = GarbageCollector::collect_dead_sections(&mut objs, &roots, false);

    assert_eq!(removed, 1);
    assert!(objs[0].sections[0].is_live); // main
    assert!(objs[0].sections[1].is_live); // used_helper
    assert!(!objs[0].sections[2].is_live); // dead_function stripped
}

#[test]
fn test_e2e_safe_identical_code_folding() {
    let target = Target::x86_64_linux();
    let mut obj = ObjectFile::new(PathBuf::from("icf_test.o"), target, 0);

    // Function A: ret 0
    let sec_a = Section::new_code(".text.func_a", vec![0x31, 0xc0, 0xc3], 16);
    obj.sections.push(sec_a);
    obj.symbols.push(Symbol::new_defined(
        "func_a",
        SymbolBinding::Global,
        SymbolType::Function,
        0,
        0,
        3,
        0,
    ));

    // Function B: ret 0 (identical code bytes)
    let sec_b = Section::new_code(".text.func_b", vec![0x31, 0xc0, 0xc3], 16);
    obj.sections.push(sec_b);
    obj.symbols.push(Symbol::new_defined(
        "func_b",
        SymbolBinding::Global,
        SymbolType::Function,
        1,
        0,
        3,
        0,
    ));

    let mut objs = vec![obj];
    let folded_count = IcfEngine::fold_sections(&mut objs, IcfMode::Safe, false);
    assert_eq!(folded_count, 1);
}

#[test]
fn test_e2e_scope_aware_raii_unwind_exact_lifo_drop_execution() {
    let mut unwind_table = UnwindActionTable::new();

    // Function `process_transaction`:
    // [0..20)   : db_conn created
    // [20..50)  : lock acquired
    // [50..80)  : tx started
    // [80..100) : completed / committed
    let desc = FunctionUnwindDescriptor {
        function_name: "process_transaction".to_string(),
        function_va: 0x402000,
        function_size: 100,
        personality_fn: "__adesh_personality_v0".to_string(),
        scope_actions: vec![
            ScopeUnwindAction {
                start_offset: 0,
                end_offset: 20,
                cleanup_state_id: 1,
                drop_targets: vec!["drop_db_conn".to_string()],
            },
            ScopeUnwindAction {
                start_offset: 20,
                end_offset: 50,
                cleanup_state_id: 2,
                drop_targets: vec!["drop_mutex_lock".to_string(), "drop_db_conn".to_string()],
            },
            ScopeUnwindAction {
                start_offset: 50,
                end_offset: 80,
                cleanup_state_id: 3,
                drop_targets: vec![
                    "drop_tx".to_string(),
                    "drop_mutex_lock".to_string(),
                    "drop_db_conn".to_string(),
                ],
            },
        ],
    };

    unwind_table.add_function(desc);

    // If panic happens at instruction offset 35 (inside lock scope):
    let drops_at_35 = unwind_table
        .find_actions_for_pc("process_transaction", 35)
        .unwrap();
    assert_eq!(
        drops_at_35,
        &["drop_mutex_lock".to_string(), "drop_db_conn".to_string()]
    );

    // If panic happens at instruction offset 65 (inside tx scope):
    let drops_at_65 = unwind_table
        .find_actions_for_pc("process_transaction", 65)
        .unwrap();
    assert_eq!(
        drops_at_65,
        &[
            "drop_tx".to_string(),
            "drop_mutex_lock".to_string(),
            "drop_db_conn".to_string()
        ]
    );
}
