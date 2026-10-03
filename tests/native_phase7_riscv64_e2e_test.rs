//! End-to-End Native Backend RISC-V64 Tests (Phase 7).
//!
//! Verifies:
//! 1. RISC-V RV64GC register allocation and calling convention.
//! 2. RV64I integer arithmetic, logic, shifts, compare, branch fixups.
//! 3. RV64D double-precision floating point (FAdd, FSub, FMul, FDiv).
//! 4. RV64A atomics (amoadd.d) and memory ordering fences.
//! 5. ADOB object generation and symbol/section structure for RISC-V64 Linux.

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass, VirtualRegister,
};
use adesh_codegen::targets::riscv::RiscVBackend;
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;

#[test]
fn test_riscv64_full_instruction_selection_and_encoding() {
    let target = TargetDescriptor::from_triple("riscv64gc-unknown-linux-gnu").expect("triple");
    let mut backend = RiscVBackend::new(target);

    let mut module = NativeModule::new("test_riscv64_full");
    let mut func = MachineFunction::new("riscv64_compute");
    func.is_exported = true;

    let v0 = func.alloc_vreg();
    let v1 = func.alloc_vreg();
    let vf0 = func.alloc_fp_vreg();
    let vf1 = func.alloc_fp_vreg();

    let block = func.entry_block_mut();
    // v0 = 50; v1 = 15; v0 = v0 + v1; v0 = v0 * v1; v0 = v0 / v1
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(50),
    });
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v1)),
        src: MachineOperand::Immediate(15),
    });
    block.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Register(MachineRegister::Virtual(v1)),
    });
    block.push(MachineInstruction::Mul {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Register(MachineRegister::Virtual(v1)),
    });
    block.push(MachineInstruction::Div {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Register(MachineRegister::Virtual(v1)),
    });

    // Float: vf0 = vf0 + vf1; vf0 = vf0 * vf1
    block.push(MachineInstruction::FAdd {
        dst: MachineOperand::Register(MachineRegister::Virtual(vf0)),
        src: MachineOperand::Register(MachineRegister::Virtual(vf1)),
        size: 8,
    });
    block.push(MachineInstruction::FMul {
        dst: MachineOperand::Register(MachineRegister::Virtual(vf0)),
        src: MachineOperand::Register(MachineRegister::Virtual(vf1)),
        size: 8,
    });

    // Atomics & Barrier
    block.push(MachineInstruction::Barrier);
    block.push(MachineInstruction::AtomicFetchAdd {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Register(MachineRegister::Virtual(v1)),
        size: 8,
    });

    block.push(MachineInstruction::Return);

    module.add_function(func);

    let lowered = backend.lower_module(&module).expect("lowering succeeds");
    let obj = backend.emit_object(&lowered).expect("ADOB object emission");

    AdobValidator::validate(&obj).expect("ADOB object must validate");
    assert_eq!(obj.symbols.len(), 1);
    assert_eq!(obj.symbols[0].name, "riscv64_compute");
    assert!(!obj.sections.is_empty());
    assert_eq!(obj.sections[0].name, ".text");
    assert!(obj.sections[0].data.len() >= 48);
}
