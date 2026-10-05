//! End-to-End Native Backend Unwind Metadata Tests (Phase 8).
//!
//! Verifies:
//! 1. Target-specific unwind rules (Win64PData for Windows, DwarfCfi for Linux/SysV, CompactUnwind).
//! 2. Scope-aware RAII drop & unwind action tables (ScopeUnwindAction, FunctionUnwindDescriptor, UnwindActionTable).
//! 3. Binary encoding of unwind maps into executable sections.
//! 4. Real native compilation, linking, and execution on Windows x64.

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::abi::{
    Aapcs64Abi, AbiSpec, RiscV64Abi, SystemVX64Abi, UnwindRules, WindowsX64Abi,
};
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass, VirtualRegister,
};
use adesh_codegen::opt::OptLevel;
use adesh_codegen::targets::x86_64::X86_64Backend;
use adesh_linker::abi::{FunctionUnwindDescriptor, ScopeUnwindAction, UnwindActionTable};
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use std::process::Command;
use tempfile::tempdir;

fn emit_link_and_run(native_mod: &NativeModule, opt_level: OptLevel, test_name: &str) -> i32 {
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let mut backend = X86_64Backend::new(target).with_opt_level(opt_level);

    let obj = backend.emit_object(native_mod).expect("ADOB emission");
    AdobValidator::validate(&obj).expect("emitted ADOB must validate");

    let bytes = AdobWriter::write(&obj).expect("ADOB encoding");
    let dir = tempdir().expect("tempdir");
    let adob_path = dir.path().join(format!("{}.adob", test_name));
    std::fs::write(&adob_path, bytes).expect("write ADOB file");

    let exe_path = dir.path().join(format!("{}.exe", test_name));
    adesh_linker::link(&[&adob_path], &exe_path, Some("x86_64-pc-windows-msvc"))
        .expect("native link");
    assert!(exe_path.exists(), "linked executable must exist");

    let out = Command::new(&exe_path)
        .output()
        .expect("execute native binary");
    out.status.code().expect("exit code")
}

#[test]
fn test_target_unwind_rules() {
    assert_eq!(WindowsX64Abi.unwind_rules(), UnwindRules::Win64PData);
    assert_eq!(SystemVX64Abi.unwind_rules(), UnwindRules::DwarfCfi);
    assert_eq!(Aapcs64Abi.unwind_rules(), UnwindRules::DwarfCfi);
    assert_eq!(RiscV64Abi.unwind_rules(), UnwindRules::DwarfCfi);
}

#[test]
fn test_scope_aware_unwind_action_table_encoding() {
    let mut table = UnwindActionTable::new();

    let func_desc = FunctionUnwindDescriptor {
        function_name: "test_cleanup_scope".to_string(),
        function_va: 0x140001000,
        function_size: 128,
        personality_fn: "__adesh_personality_v0".to_string(),
        scope_actions: vec![
            ScopeUnwindAction {
                start_offset: 16,
                end_offset: 64,
                cleanup_state_id: 1,
                drop_targets: vec!["drop_resource_a".to_string(), "drop_resource_b".to_string()],
            },
            ScopeUnwindAction {
                start_offset: 64,
                end_offset: 112,
                cleanup_state_id: 2,
                drop_targets: vec!["drop_resource_c".to_string()],
            },
        ],
    };

    table.add_function(func_desc);

    // Verify PC lookup
    let actions_in_scope = table.find_actions_for_pc("test_cleanup_scope", 32);
    assert_eq!(
        actions_in_scope,
        Some(&["drop_resource_a".to_string(), "drop_resource_b".to_string()][..])
    );

    let actions_out_of_scope = table.find_actions_for_pc("test_cleanup_scope", 8);
    assert_eq!(actions_out_of_scope, None);

    // Verify binary encoding
    let encoded = table.encode_binary();
    assert!(
        encoded.starts_with(b"AUNWND\x01\x00"),
        "must start with AUNWND magic header"
    );
    assert!(encoded.len() > 16);
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_execution_with_unwind_frame() {
    let mut module = NativeModule::new("test_unwind");
    let mut main_func = MachineFunction::new("main");
    main_func.is_exported = true;
    main_func.stack_size = 32;

    let b = main_func.entry_block_mut();
    b.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(91),
    });
    b.push(MachineInstruction::Return);
    module.add_function(main_func);

    let code = emit_link_and_run(&module, OptLevel::O2, "test_unwind_exec");
    assert_eq!(code, 91, "unwind-framed executable must exit with code 91");
}
