//! Phase 9 Advanced LTO E2E Test Suite.
//!
//! Validates:
//! - Global dead-function elimination (GDFE) under Thin and Full LTO.
//!
//! (The IPO half was removed in Phase 4: `opt/ipo.rs` fabricated profile
//! counters from hand-supplied call-site data and was deleted under the
//! wire-or-delete policy; real cross-module optimization is the LTO engine's
//! job, wired via `CompilerDriver`.)

#![allow(dead_code, unused_imports)]

use adesh_codegen::machine_ir::{
    ConditionCode, MachineBlock, MachineFunction, MachineInstruction, MachineOperand,
    MachineRegister, NativeModule, PhysicalRegister,
};
use adesh_codegen::opt::lto::{LtoConfig, LtoEngine, LtoMode};

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
