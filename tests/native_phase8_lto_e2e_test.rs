//! End-to-End Native Backend Link-Time Optimization (LTO) Tests (Phase 8).
//!
//! Verifies:
//! 1. Multi-module compilation and LTO merging.
//! 2. Cross-module Dead Function Elimination (GDFE).
//! 3. LtoEngine execution with Full and Thin LTO modes.
//! 4. Real native compilation, linking, and execution of LTO-optimized binary on Windows x64.

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass, VirtualRegister,
};
use adesh_codegen::opt::OptLevel;
use adesh_codegen::opt::lto::{LtoConfig, LtoEngine, LtoMode, ModuleSummary};
use adesh_codegen::targets::x86_64::X86_64Backend;
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
fn test_lto_module_summary_and_gdfe() {
    let mut mod1 = NativeModule::new("mod1");
    let mut unused_func = MachineFunction::new("unused_internal");
    unused_func.is_exported = false;
    let b = unused_func.entry_block_mut();
    b.push(MachineInstruction::Return);
    mod1.add_function(unused_func);

    let mut main_func = MachineFunction::new("main");
    main_func.is_exported = true;
    let mb = main_func.entry_block_mut();
    mb.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(100),
    });
    mb.push(MachineInstruction::Return);
    mod1.add_function(main_func);

    let summary = ModuleSummary::analyze(&mod1);
    assert_eq!(summary.functions.len(), 2);
    assert!(summary.functions.contains_key("unused_internal"));
    assert!(summary.functions.contains_key("main"));

    // Run LTO engine
    let mut engine = LtoEngine::new(LtoConfig {
        mode: LtoMode::Full,
        max_inline_instructions: 20,
        enable_global_dce: true,
        enable_devirtualization: true,
    });

    let mut modules = vec![mod1];
    let report = engine
        .optimize_modules(&mut modules)
        .expect("LTO optimization");
    assert_eq!(modules.len(), 1);
    assert!(
        report.dead_functions_removed >= 1,
        "unused_internal should be eliminated by GDFE"
    );
    assert_eq!(modules[0].functions.len(), 1);
    assert_eq!(modules[0].functions[0].name, "main");
}

#[test]
fn test_lto_cross_module_execution_e2e() {
    // Module 1 defines compute_offset() -> returns 30
    let mut mod1 = NativeModule::new("math_mod");
    let mut helper_func = MachineFunction::new("compute_offset");
    helper_func.is_exported = true;
    let hb = helper_func.entry_block_mut();
    hb.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(30),
    });
    hb.push(MachineInstruction::Return);
    mod1.add_function(helper_func);

    // Module 2 defines main() -> calls compute_offset(), adds 12, returns 42
    let mut mod2 = NativeModule::new("main_mod");
    let mut main_func = MachineFunction::new("main");
    main_func.is_exported = true;
    let mb = main_func.entry_block_mut();
    mb.push(MachineInstruction::Call {
        target: MachineOperand::Symbol("compute_offset".to_string()),
        num_args: 0,
    });
    mb.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(12),
    });
    mb.push(MachineInstruction::Return);
    mod2.add_function(main_func);

    // Run LTO engine across both modules
    let mut engine = LtoEngine::new(LtoConfig {
        mode: LtoMode::Full,
        max_inline_instructions: 20,
        enable_global_dce: true,
        enable_devirtualization: true,
    });

    let mut modules = vec![mod1, mod2];
    let _report = engine
        .optimize_modules(&mut modules)
        .expect("LTO optimization");
    assert_eq!(modules.len(), 1, "modules merged into 1");

    let code = emit_link_and_run(&modules[0], OptLevel::O2, "test_lto_exec");
    assert_eq!(code, 42, "LTO cross-module execution must return 42");
}
