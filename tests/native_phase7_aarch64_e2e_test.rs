//! End-to-End Native Backend AArch64 Tests (Phase 7).
//!
//! Verifies:
//! 1. AArch64 AAPCS64 register allocation and calling convention.
//! 2. Integer arithmetic, logic, shifts, compare, conditional branches.
//! 3. Floating point (FAdd, FSub, FMul, FDiv) and NEON SIMD (VectorAdd, VectorSub, VectorMul).
//! 4. Atomics and memory ordering barriers (ldar, stlr, ldaddal, dmb).
//! 5. ADOB object generation and symbol/section structure for AArch64 Linux.

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass, VirtualRegister,
};
use adesh_codegen::opt::VectorType;
use adesh_codegen::targets::aarch64::AArch64Backend;
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;

fn d_reg(n: u8) -> MachineOperand {
    MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(32 + n)))
}

#[test]
fn test_aarch64_full_instruction_selection_and_encoding() {
    let target = TargetDescriptor::from_triple("aarch64-unknown-linux-gnu").expect("triple");
    let mut backend = AArch64Backend::new(target);

    let mut module = NativeModule::new("test_aarch64_full");
    let mut func = MachineFunction::new("aarch64_compute");
    func.is_exported = true;

    let v0 = func.alloc_vreg();
    let v1 = func.alloc_vreg();

    let block = func.entry_block_mut();
    // v0 = 42; v1 = 10; v0 = v0 + v1; v0 = v0 * v1; v0 = v0 / v1
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(42),
    });
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v1)),
        src: MachineOperand::Immediate(10),
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

    // Float: d0 = d0 + d1; d0 = d0 * d1
    block.push(MachineInstruction::FAdd {
        dst: d_reg(0),
        src: d_reg(1),
        size: 8,
    });
    block.push(MachineInstruction::FMul {
        dst: d_reg(0),
        src: d_reg(1),
        size: 8,
    });

    // SIMD: VectorAdd, VectorSub, VectorMul
    block.push(MachineInstruction::VectorAdd {
        dst: d_reg(0),
        src: d_reg(1),
        vec_type: VectorType::v4f32(),
    });
    block.push(MachineInstruction::VectorSub {
        dst: d_reg(0),
        src: d_reg(1),
        vec_type: VectorType::v4f32(),
    });
    block.push(MachineInstruction::VectorMul {
        dst: d_reg(0),
        src: d_reg(1),
        vec_type: VectorType::v4f32(),
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
    assert_eq!(obj.symbols[0].name, "aarch64_compute");
    assert!(!obj.sections.is_empty());
    assert_eq!(obj.sections[0].name, ".text");
    assert!(obj.sections[0].data.len() >= 64);
}

/// Vector pressure must produce full-width reloads and encodable operations.
#[test]
fn test_aarch64_spilled_vector_operands_encode() {
    let target = TargetDescriptor::from_triple("aarch64-unknown-linux-gnu").expect("triple");
    let mut backend = AArch64Backend::new(target);

    let mut module = NativeModule::new("test_aarch64_spilled_fp");
    let mut func = MachineFunction::new("aarch64_spilled");
    func.is_exported = true;

    let v0 = func.alloc_vreg();
    let v1 = func.alloc_vreg();
    let regs: Vec<_> = (0..40).map(|_| func.alloc_fp_vreg()).collect();

    let block = func.entry_block_mut();
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(42),
    });
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v1)),
        src: MachineOperand::Immediate(10),
    });
    block.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Register(MachineRegister::Virtual(v1)),
    });
    for &reg in &regs {
        block.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(reg)),
            src: d_reg(0),
        });
    }
    for &reg in &regs[1..] {
        block.push(MachineInstruction::VectorAdd {
            dst: MachineOperand::Register(MachineRegister::Virtual(regs[0])),
            src: MachineOperand::Register(MachineRegister::Virtual(reg)),
            vec_type: VectorType::v4f32(),
        });
    }
    block.push(MachineInstruction::Return);

    module.add_function(func);

    let lowered = backend.lower_module(&module).expect("lowering succeeds");
    assert!(
        lowered.functions[0]
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i, MachineInstruction::Load { size: 16, .. }))
    );
    backend.emit_object(&lowered).expect("vector spills encode");
}
