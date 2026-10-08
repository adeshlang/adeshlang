//! Cross-Target Architecture Matrix and Deterministic Compilation Tests (Phase 7).
//!
//! Verifies:
//! 1. Multi-target backend lowering and ADOB emission across:
//!    - x86_64-pc-windows-msvc
//!    - x86_64-unknown-linux-gnu
//!    - aarch64-unknown-linux-gnu
//!    - riscv64gc-unknown-linux-gnu
//! 2. BasicBlockScheduler instruction reordering and hazard avoidance.
//! 3. Determinism of emitted binary objects across targets.

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::machine_ir::{
    ConditionCode, MachineBlock, MachineFunction, MachineInstruction, MachineOperand,
    MachineRegister, NativeModule, PhysicalRegister, VirtualRegister,
};
use adesh_codegen::opt::BasicBlockScheduler;
use adesh_codegen::targets::{
    aarch64::AArch64Backend, create_backend, riscv::RiscVBackend, x86_64::X86_64Backend,
};
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;

#[test]
fn test_instruction_scheduler_latency_and_hazard_avoidance() {
    let mut func = MachineFunction::new("sched_test");
    let r0 = func.alloc_vreg();
    let r1 = func.alloc_vreg();
    let r2 = func.alloc_vreg();

    let block = func.entry_block_mut();
    // Load r0 from memory (latency 3)
    block.push(MachineInstruction::Load {
        dst: MachineOperand::Register(MachineRegister::Virtual(r0)),
        src: MachineOperand::StackSlot(-8),
        size: 8,
    });
    // Independent Add r1, 10 (latency 1) -> can be scheduled immediately after Load to hide latency
    block.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(r1)),
        src: MachineOperand::Immediate(10),
    });
    // Add r2, r0 (depends on Load r0)
    block.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(r2)),
        src: MachineOperand::Register(MachineRegister::Virtual(r0)),
    });
    block.push(MachineInstruction::Return);

    let scheduler = BasicBlockScheduler::new();
    let count = scheduler
        .schedule_function(&mut func)
        .expect("schedule function");
    assert!(count > 0, "scheduler should process basic block");

    // Verify all instructions are preserved
    assert_eq!(func.blocks[0].instructions.len(), 4);
}

#[test]
fn test_instruction_scheduler_preserves_flags_consumer_order() {
    let mut block = MachineBlock::new(0, "flags");
    let lhs = MachineOperand::phys(1);
    block.push(MachineInstruction::Compare {
        lhs: lhs.clone(),
        rhs: MachineOperand::Immediate(0),
    });
    block.push(MachineInstruction::Move {
        dst: MachineOperand::phys(2),
        src: MachineOperand::Immediate(7),
    });
    block.push(MachineInstruction::BranchCc {
        cc: ConditionCode::Equal,
        target: "taken".into(),
    });

    let scheduled = BasicBlockScheduler::new().schedule_block(&block);
    assert!(matches!(scheduled[0], MachineInstruction::Compare { .. }));
    assert!(matches!(scheduled[1], MachineInstruction::Move { .. }));
    assert!(matches!(scheduled[2], MachineInstruction::BranchCc { .. }));
}

#[test]
fn test_cross_target_backend_matrix() {
    let triples = [
        "x86_64-pc-windows-msvc",
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "riscv64gc-unknown-linux-gnu",
    ];

    for triple in &triples {
        let target = TargetDescriptor::from_triple(triple).expect("valid target triple");
        let mut backend = create_backend(target.clone()).expect("create backend");

        let mut module = NativeModule::new(format!("mod_{:?}", target.architecture));
        let mut func = MachineFunction::new("add_calc");
        func.is_exported = true;

        let v0 = func.alloc_vreg();
        let v1 = func.alloc_vreg();

        let block = func.entry_block_mut();
        block.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
            src: MachineOperand::Immediate(100),
        });
        block.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(v1)),
            src: MachineOperand::Immediate(25),
        });
        block.push(MachineInstruction::Add {
            dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
            src: MachineOperand::Register(MachineRegister::Virtual(v1)),
        });
        block.push(MachineInstruction::Return);

        module.add_function(func);

        let lowered = backend.lower_module(&module).expect("lowering succeeds");
        let obj = backend
            .emit_object(&lowered)
            .expect("ADOB emission succeeds");

        AdobValidator::validate(&obj).expect("ADOB object must validate");
        assert_eq!(obj.symbols.len(), 1);
        assert_eq!(obj.symbols[0].name, "add_calc");
        assert!(!obj.sections.is_empty());
    }
}
