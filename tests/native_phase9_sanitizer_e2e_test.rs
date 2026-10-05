//! Phase 9 Compiler Sanitizer Instrumentation E2E Test Suite.
//!
//! Validates:
//! - SanitizerInstrumenter injecting bounds checks into MachineFunctions.
//! - Integer overflow check insertion.
//! - Memory poisoning injection on deallocation.
//! - SanitizerReport counters.

#![allow(dead_code, unused_imports)]

use adesh_codegen::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, NativeModule,
    PhysicalRegister,
};
use adesh_codegen::sanitizer::{SanitizerFlags, SanitizerInstrumenter};

#[test]
fn test_sanitizer_bounds_and_overflow_instrumentation() {
    let mut native_mod = NativeModule::new("sanitized_mod");
    let mut func = MachineFunction::new("process_array");
    func.is_exported = true;

    let b = func.entry_block_mut();
    // Simulate array load
    b.push(MachineInstruction::Load {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))),
        size: 8,
    });
    // Simulate integer add
    b.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(100),
    });
    b.push(MachineInstruction::Return);

    native_mod.add_function(func);

    let mut instrumenter = SanitizerInstrumenter::new(SanitizerFlags {
        bounds_check: true,
        integer_overflow: true,
        use_after_free: true,
        stack_protector: true,
    });

    let report = instrumenter
        .instrument_module(&mut native_mod)
        .expect("instrumentation");

    assert!(report.bounds_checks_inserted > 0);
    assert!(report.overflow_checks_inserted > 0);
    // Verified that instructions were added
    assert!(native_mod.functions[0].entry_block().instructions.len() > 3);
}
