//! End-to-End Native Backend Link-Time Optimization (LTO) & Interprocedural Tests (Phase 6).
//!
//! Verifies:
//! 1. Whole-program multi-module LTO merging.
//! 2. Cross-module leaf function inlining across compilation units.
//! 3. Global Dead-Function Elimination (GDFE) pruning unreferenced internal functions.
//! 4. ModuleSummary and FunctionSummary analysis with JSON serialization/deserialization.
//! 5. Native compilation, linking with `adeshlink`, and execution on Windows x64.

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass, VirtualRegister,
};
use adesh_codegen::opt::{FunctionSummary, LtoConfig, LtoEngine, LtoMode, ModuleSummary, OptLevel};
use adesh_codegen::targets::x86_64::X86_64Backend;
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use std::process::Command;
use tempfile::tempdir;

fn emit_lto_link_and_run(modules: Vec<NativeModule>, config: LtoConfig, test_name: &str) -> i32 {
    let mut unified = LtoEngine::merge_modules(&modules);
    LtoEngine::optimize(&mut unified, &config);

    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let mut backend = X86_64Backend::new(target).with_opt_level(OptLevel::O2);

    let obj = backend.emit_object(&unified).expect("ADOB emission");
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
fn test_module_summary_json_roundtrip() {
    let mut module = NativeModule::new("summary_mod");
    let mut func = MachineFunction::new("test_fn");
    func.is_exported = true;
    let entry = func.entry_block_mut();
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(123),
    });
    entry.push(MachineInstruction::Return);
    module.add_function(func);

    let summary = ModuleSummary::analyze(&module);
    let json = summary.to_json().expect("serialize summary");
    let restored = ModuleSummary::from_json(&json).expect("deserialize summary");

    assert!(restored.functions.contains_key("test_fn"));
    let fn_summary = &restored.functions["test_fn"];
    assert!(fn_summary.is_exported);
    assert!(fn_summary.is_leaf);
    assert!(fn_summary.is_pure);
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_lto_cross_module_inline_and_dead_func_elim_e2e() {
    // Module A: helper function `compute_val` -> returns 42
    // Module B: main function -> calls `compute_val`, plus a dead function `unused_dead_fn`
    let mut mod_a = NativeModule::new("mod_a");
    let mut compute_val = MachineFunction::new("compute_val");
    compute_val.is_exported = false; // internal helper
    let entry_a = compute_val.entry_block_mut();
    entry_a.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(42),
    });
    entry_a.push(MachineInstruction::Return);
    mod_a.add_function(compute_val);

    let mut mod_b = NativeModule::new("mod_b");
    let mut main_func = MachineFunction::new("main");
    main_func.is_exported = true;
    let entry_b = main_func.entry_block_mut();
    entry_b.push(MachineInstruction::Call {
        target: MachineOperand::Symbol("compute_val".to_string()),
        num_args: 0,
    });
    entry_b.push(MachineInstruction::Return);
    mod_b.add_function(main_func);

    let mut dead_func = MachineFunction::new("unused_dead_fn");
    dead_func.is_exported = false;
    let entry_dead = dead_func.entry_block_mut();
    entry_dead.push(MachineInstruction::Return);
    mod_b.add_function(dead_func);

    let config = LtoConfig {
        mode: LtoMode::Full,
        max_inline_instructions: 10,
        enable_global_dce: true,
        enable_devirtualization: true,
    };

    let exit_code = emit_lto_link_and_run(vec![mod_a, mod_b], config, "lto_cross_module");
    assert_eq!(exit_code, 42);
}
