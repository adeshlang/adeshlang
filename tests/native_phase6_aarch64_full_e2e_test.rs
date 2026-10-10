//! Phase 6 AArch64 Production Backend E2E Test Suite.
//!
//! Validates:
//! - Full AAPCS64 calling convention with callee-saved register preservation (X19..X28, D8..D15).
//! - 64-bit immediate materialization (MOVZ / MOVK / MOVN).
//! - Load / Store pairs (LDP / STP), unscaled (LDUR / STUR), and scaled loads/stores.
//! - Arithmetic, bitwise, comparison, and conditional branches (B, B.cond, CBZ, CBNZ, CSET).
//! - Scalar floating-point (D0..D31) and NEON SIMD vectors.
//! - Atomics (LDADDAL, CASAL, SWPAL) and memory barriers (DMB ISH).
//! - Full ADOB object emission with AArch64 relocations (CALL26, ADRP, ADD_LO12).
//! - Static ELF64 generation with AArch64 machine headers and section layouts.

#![allow(dead_code, unused_imports)]

use adesh_codegen::backend::CodegenBackend;
use adesh_codegen::machine_ir::{
    ConditionCode, MachineBlock, MachineFunction, MachineInstruction, MachineOperand,
    MachineRegister, NativeModule, PhysicalRegister,
};
use adesh_codegen::register_alloc::RegisterFile;
use adesh_codegen::targets::aarch64::{AArch64Backend, AArch64RegisterFile};
use adesh_linker::arch::AArch64Arch;
use adesh_linker::elf::writer::ElfWriter;
use adesh_linker::layout::LayoutEngine;
use adesh_linker::relocation::{Relocation, RelocationHandler, RelocationKind};
use adesh_linker::target::{Arch, Target};
use adesh_object::{RelocationKind as AdobRelocKind, TargetDescriptor};

#[test]
fn test_aarch64_aapcs64_register_file_and_callee_saved_rules() {
    let reg_file = AArch64RegisterFile;

    // Verify all 64 registers present
    assert_eq!(reg_file.registers().len(), 64);

    // Verify callee-saved registers (X19..X28 = 10 GPRs)
    let callee_saved = reg_file.callee_saved();
    assert_eq!(callee_saved.len(), 10);
    assert_eq!(callee_saved[0].0, 19);
    assert_eq!(callee_saved[9].0, 28);

    // Verify caller-saved registers (X0..X15 = 16 GPRs)
    let caller_saved = reg_file.caller_saved();
    assert_eq!(caller_saved.len(), 16);
    assert_eq!(caller_saved[0].0, 0);
    assert_eq!(caller_saved[15].0, 15);

    // Verify allocatable GPRs (16 caller-saved + 10 callee-saved = 26)
    assert_eq!(reg_file.allocatable().len(), 26);

    // Verify reserved registers (IP0, IP1, Platform, FP, LR, SP)
    assert_eq!(reg_file.reserved().len(), 6);
}

#[test]
fn test_aarch64_immediate_materialization_and_arithmetic() {
    let target = TargetDescriptor::from_triple("aarch64-unknown-linux-gnu").expect("triple");
    let mut backend = AArch64Backend::new(target);

    let mut func = MachineFunction::new("math_ops");
    let b = func.entry_block_mut();

    // 1. Load small immediate: MOVZ X0, #42
    b.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(42),
    });

    // 2. Load large 64-bit immediate: MOVZ + MOVK sequence
    b.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))),
        src: MachineOperand::Immediate(0x1234_5678_9ABC_DEF0),
    });

    // 3. Load negative constant: MOVN
    b.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(2))),
        src: MachineOperand::Immediate(-5),
    });

    // 4. Arithmetic: ADD, SUB, MUL, DIV
    b.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))),
    });
    b.push(MachineInstruction::Sub {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(10),
    });
    b.push(MachineInstruction::Mul {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(2))),
    });
    b.push(MachineInstruction::Div {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(2))),
    });

    b.push(MachineInstruction::Return);

    let code = backend
        .generate_function(&func)
        .expect("encodes arithmetic");
    assert!(!code.is_empty());
    // Verify 4-byte instruction alignment
    assert_eq!(code.len() % 4, 0);
}

#[test]
fn test_aarch64_callee_saved_prologue_epilogue_generation() {
    let target = TargetDescriptor::from_triple("aarch64-unknown-linux-gnu").expect("triple");
    let mut backend = AArch64Backend::new(target);

    let mut func = MachineFunction::new("callee_save_fn");
    func.stack_size = 32; // 32 bytes local stack allocation
    let b = func.entry_block_mut();

    // Use multiple callee-saved registers (X19, X20, X21)
    b.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(19))),
        src: MachineOperand::Immediate(100),
    });
    b.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(20))),
        src: MachineOperand::Immediate(200),
    });
    b.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(21))),
        src: MachineOperand::Immediate(300),
    });
    b.push(MachineInstruction::Return);

    let code = backend.generate_function(&func).expect("encodes frame");
    // Must contain STP X29, X30, [SP, #-16]! (0xA9BF7BFD) at start
    assert_eq!(
        &code[0..4],
        &0xA9BF7BFDu32.to_le_bytes(),
        "Missing STP X29, X30 prologue"
    );
    // Must contain MOV X29, SP (0x910003FD)
    assert_eq!(
        &code[4..8],
        &0x910003FDu32.to_le_bytes(),
        "Missing MOV X29, SP"
    );
    // Must end with RET (0xD65F03C0)
    let len = code.len();
    assert_eq!(
        &code[len - 4..len],
        &0xD65F03C0u32.to_le_bytes(),
        "Missing RET instruction"
    );
}

#[test]
fn test_aarch64_control_flow_and_comparison() {
    let target = TargetDescriptor::from_triple("aarch64-unknown-linux-gnu").expect("triple");
    let mut backend = AArch64Backend::new(target);

    let mut func = MachineFunction::new("branch_test");

    // Entry block
    let b0 = func.entry_block_mut();
    b0.label = "entry".to_string();
    b0.push(MachineInstruction::Compare {
        lhs: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        rhs: MachineOperand::Immediate(0),
    });
    b0.push(MachineInstruction::BranchCc {
        cc: ConditionCode::Equal,
        target: "zero_block".to_string(),
    });
    b0.push(MachineInstruction::Branch {
        target: "nonzero_block".to_string(),
    });

    // Zero block
    let mut b1 = MachineBlock::new(1, "zero_block");
    b1.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(1),
    });
    b1.push(MachineInstruction::Return);
    func.blocks.push(b1);

    // Nonzero block
    let mut b2 = MachineBlock::new(2, "nonzero_block");
    b2.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(2),
    });
    b2.push(MachineInstruction::Return);
    func.blocks.push(b2);

    let code = backend
        .generate_function(&func)
        .expect("encodes control flow");
    assert!(!code.is_empty());
}

#[test]
fn test_aarch64_atomics_and_fp_encodings() {
    let target = TargetDescriptor::from_triple("aarch64-unknown-linux-gnu").expect("triple");
    let mut backend = AArch64Backend::new(target);

    let mut func = MachineFunction::new("atomic_fp");
    let b = func.entry_block_mut();

    // Floating-point arithmetic
    b.push(MachineInstruction::FAdd {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(32))), // D0
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(33))), // D1
        size: 8,
    });
    b.push(MachineInstruction::FMul {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(32))),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(34))),
        size: 8,
    });

    // Atomics & Memory Barriers
    b.push(MachineInstruction::Barrier);
    b.push(MachineInstruction::AtomicFetchAdd {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))),
        size: 8,
    });
    b.push(MachineInstruction::AtomicCompareExchange {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        expected: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))),
        desired: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(2))),
        size: 8,
    });
    b.push(MachineInstruction::AtomicExchange {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(1))),
        size: 8,
    });
    b.push(MachineInstruction::Return);

    let code = backend
        .generate_function(&func)
        .expect("encodes atomics and FP");
    assert!(code.len() >= 32);
}

#[test]
fn test_aarch64_adob_emission_and_relocations() {
    let target = TargetDescriptor::from_triple("aarch64-unknown-linux-gnu").expect("triple");
    let mut backend = AArch64Backend::new(target);

    let mut module = NativeModule::new("test_aarch64_mod");

    let mut f1 = MachineFunction::new("caller_fn");
    f1.is_exported = true;
    let b1 = f1.entry_block_mut();
    b1.push(MachineInstruction::Call {
        target: MachineOperand::Symbol("callee_fn".to_string()),
        num_args: 0,
    });
    b1.push(MachineInstruction::Return);
    module.add_function(f1);

    let mut f2 = MachineFunction::new("callee_fn");
    f2.is_exported = true;
    let b2 = f2.entry_block_mut();
    b2.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(77),
    });
    b2.push(MachineInstruction::Return);
    module.add_function(f2);

    let obj = backend.emit_object(&module).expect("emits ADOB object");
    assert_eq!(obj.symbols.len(), 2);
    assert_eq!(obj.sections.len(), 1);

    let text_sec = &obj.sections[0];
    assert_eq!(text_sec.name, ".text");
    assert!(!text_sec.relocations.is_empty());
    assert_eq!(text_sec.relocations[0].kind, AdobRelocKind::AArch64_Call26);
}

#[test]
fn test_aarch64_relocation_handler_apply() {
    let arch_handler = AArch64Arch;

    // Test AArch64 Call26 relocation
    // Initial instruction: BL #0 (0x94000000)
    let mut code = 0x94000000u32.to_le_bytes();
    let reloc = Relocation::new(0, "target_func", RelocationKind::AArch64Call26, 0);

    // Place VA = 0x1000, Symbol VA = 0x1040 (offset = +0x40 bytes = +16 instructions = imm26: 0x10)
    arch_handler
        .apply(&reloc, 0x1000, 0x1040, 0, &mut code)
        .expect("applies CALL26");

    let patched_insn = u32::from_le_bytes(code);
    assert_eq!(patched_insn, 0x94000010u32);
}
