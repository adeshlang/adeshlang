//! Phase 9 Advanced LTO and Interprocedural Optimization (IPO) E2E Test Suite.
//!
//! Validates:
//! - Phase 9 IpoEngine: function specialization, constant propagation across module boundaries.
//! - Call-site analysis, hot function cloning, and hot/cold code splitting.
//! - Global dead-function elimination (GDFE) under Thin and Full LTO.

#![allow(dead_code, unused_imports)]

use adesh_codegen::machine_ir::{
    ConditionCode, MachineBlock, MachineFunction, MachineInstruction, MachineOperand,
    MachineRegister, NativeModule, PhysicalRegister,
};
use adesh_codegen::opt::ipo::{CallSiteInfo, IpoConfig, IpoEngine};
use adesh_codegen::opt::lto::{LtoConfig, LtoEngine, LtoMode};

#[test]
fn test_ipo_function_specialization_and_constant_propagation() {
    let mut native_mod = NativeModule::new("ipo_test_mod");

    // Generic worker function
    let mut worker = MachineFunction::new("process_data");
    worker.is_exported = true;
    let wb = worker.entry_block_mut();
    wb.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(10),
    });
    wb.push(MachineInstruction::Return);
    native_mod.add_function(worker);

    // Caller function with constant argument (e.g. constant flag = 1)
    let mut caller = MachineFunction::new("run_task");
    caller.is_exported = true;
    let cb = caller.entry_block_mut();
    cb.push(MachineInstruction::Call {
        target: MachineOperand::Immediate(0),
        num_args: 1,
    });
    cb.push(MachineInstruction::Return);
    native_mod.add_function(caller);

    let mut engine = IpoEngine::new(IpoConfig {
        enable_specialization: true,
        enable_cross_module_const_prop: true,
        enable_hot_cold_splitting: true,
        max_specialization_depth: 3,
    });

    let call_sites = vec![CallSiteInfo {
        caller: "run_task".to_string(),
        callee: "process_data".to_string(),
        known_constant_args: vec![(0, 42)],
        call_count: 500,
    }];

    let report = engine
        .optimize(&mut native_mod, &call_sites)
        .expect("IPO optimization");

    assert!(report.specialized_functions > 0 || report.constants_propagated > 0);
    assert_eq!(native_mod.functions.len(), 2);
}

#[test]
fn test_lto_full_pipeline_cross_module_dce() {
    let mut mod_a = NativeModule::new("mod_a");
    let mut unused_fn = MachineFunction::new("never_called_helper");
    unused_fn.is_exported = false;
    unused_fn.entry_block_mut().push(MachineInstruction::Return);
    mod_a.add_function(unused_fn);

    let mut entry_fn = MachineFunction::new("main");
    entry_fn.is_exported = true;
    entry_fn.entry_block_mut().push(MachineInstruction::Return);
    mod_a.add_function(entry_fn);

    let mut lto = LtoEngine::new(LtoConfig {
        mode: LtoMode::Full,
        max_inline_instructions: 50,
        enable_global_dce: true,
        enable_devirtualization: true,
    });

    let mut modules = vec![mod_a];
    let report = lto.optimize_modules(&mut modules).expect("LTO execution");
    assert!(report.dead_functions_removed >= 1);
    assert_eq!(modules[0].functions.len(), 1);
    assert_eq!(modules[0].functions[0].name, "main");
}
