use adesh_linker::abi::mangle::{demangle_symbol, mangle_symbol};
use adesh_linker::abi::{FunctionUnwindDescriptor, ScopeUnwindAction, UnwindActionTable};
use adesh_linker::target::{Target, TargetTier};

#[test]
fn test_symbol_mangling_and_demangling_roundtrip() {
    let mangled = mangle_symbol("std", &["collections", "hash_map"], "insert", Some(0x12345678));
    assert!(mangled.starts_with("_A3std"));
    assert!(mangled.contains("11collections"));
    assert!(mangled.contains("8hash_map"));
    assert!(mangled.contains("6insert"));

    let demangled = demangle_symbol(&mangled);
    assert_eq!(demangled, Some("std::collections::hash_map::insert".to_string()));
}

#[test]
fn test_scope_aware_raii_unwind_action_lookup() {
    let mut table = UnwindActionTable::new();

    let func_name = "foo_process";
    let desc = FunctionUnwindDescriptor {
        function_name: func_name.to_string(),
        function_va: 0x401000,
        function_size: 100,
        personality_fn: "__adesh_personality_v0".to_string(),
        scope_actions: vec![
            ScopeUnwindAction {
                start_offset: 10,
                end_offset: 40,
                cleanup_state_id: 1,
                // Only 'a' is alive in bytes [10..40)
                drop_targets: vec!["drop_a".to_string()],
            },
            ScopeUnwindAction {
                start_offset: 40,
                end_offset: 80,
                cleanup_state_id: 2,
                // 'b' and 'a' are alive in bytes [40..80) -> drop 'b' then 'a' (LIFO)
                drop_targets: vec!["drop_b".to_string(), "drop_a".to_string()],
            },
        ],
    };

    table.add_function(desc);

    // PC offset 5: before scope -> no drops
    assert_eq!(table.find_actions_for_pc(func_name, 5), None);

    // PC offset 25: inside scope 1 -> drops [drop_a]
    let actions_s1 = table.find_actions_for_pc(func_name, 25).unwrap();
    assert_eq!(actions_s1, &["drop_a".to_string()]);

    // PC offset 55: inside scope 2 -> drops [drop_b, drop_a] (LIFO order)
    let actions_s2 = table.find_actions_for_pc(func_name, 55).unwrap();
    assert_eq!(actions_s2, &["drop_b".to_string(), "drop_a".to_string()]);

    // PC offset 95: after scope -> no drops
    assert_eq!(table.find_actions_for_pc(func_name, 95), None);
}

#[test]
fn test_unwind_action_table_binary_encoding() {
    let mut table = UnwindActionTable::new();
    table.add_function(FunctionUnwindDescriptor {
        function_name: "test_fn".to_string(),
        function_va: 0x500000,
        function_size: 64,
        personality_fn: "__adesh_personality_v0".to_string(),
        scope_actions: vec![ScopeUnwindAction {
            start_offset: 8,
            end_offset: 32,
            cleanup_state_id: 1,
            drop_targets: vec!["drop_resource".to_string()],
        }],
    });

    let encoded = table.encode_binary();
    assert!(encoded.starts_with(b"AUNWND\x01\x00"));
    assert!(encoded.len() > 20);
}

#[test]
fn test_target_maturity_tier_classification() {
    // Tier 1 Targets
    let linux_x64 = Target::from_triple("x86_64-linux").unwrap();
    assert_eq!(linux_x64.tier(), TargetTier::Tier1Supported);

    let win_x64 = Target::from_triple("x86_64-windows").unwrap();
    assert_eq!(win_x64.tier(), TargetTier::Tier1Supported);

    let mac_arm64 = Target::from_triple("aarch64-macos").unwrap();
    assert_eq!(mac_arm64.tier(), TargetTier::Tier1Supported);

    let wasm_wasi = Target::from_triple("wasm32-wasi").unwrap();
    assert_eq!(wasm_wasi.tier(), TargetTier::Tier1Supported);

    // Tier 2 Targets
    let riscv = Target::from_triple("riscv64-linux").unwrap();
    assert_eq!(riscv.tier(), TargetTier::Tier2Experimental);

    let cuda = Target::from_triple("nvptx64-cuda").unwrap();
    assert_eq!(cuda.tier(), TargetTier::Tier2Experimental);

    let qpu = Target::from_triple("qpu-quantum").unwrap();
    assert_eq!(qpu.tier(), TargetTier::Tier2Experimental);

    // Tier 3 Targets
    let s390x = Target::from_triple("s390x-linux").unwrap();
    assert_eq!(s390x.tier(), TargetTier::Tier3Declared);
}
