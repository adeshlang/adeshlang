//! Phase 9 Cross-Platform Target Spec and Assembly Emission E2E Test Suite.
//!
//! Validates:
//! - TargetSpec configurations across Windows x64, Linux SysV, and cross-platform targets.
//! - Calling convention shadow space and register rules.
//! - AssemblyEmitter output generation across targets.

#![allow(dead_code, unused_imports)]

use adesh_codegen::asm::AssemblyEmitter;
use adesh_codegen::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, NativeModule,
    PhysicalRegister,
};
use adesh_codegen::target_spec::TargetSpec;

#[test]
fn test_target_spec_abi_and_shadow_space() {
    let win = TargetSpec::x86_64_windows();
    assert!(win.is_windows());
    assert_eq!(win.shadow_space_bytes(), 32);

    let linux = TargetSpec::x86_64_linux();
    assert!(linux.is_sysv());
    assert_eq!(linux.shadow_space_bytes(), 0);
}

#[test]
fn test_assembly_emitter_emission() {
    let mut native_mod = NativeModule::new("test_asm_module");
    let mut func = MachineFunction::new("add_values");
    func.is_exported = true;

    let b = func.entry_block_mut();
    b.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(42),
    });
    b.push(MachineInstruction::Return);

    native_mod.add_function(func);

    let asm_text = AssemblyEmitter::emit_module(&native_mod);
    assert!(asm_text.contains(".globl add_values"));
    assert!(asm_text.contains("ret"));
}
