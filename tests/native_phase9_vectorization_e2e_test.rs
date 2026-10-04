//! Phase 9 Advanced Vectorization & SIMD Runtime Dispatch E2E Test Suite.
//!
//! Validates:
//! - SimdDispatcher multi-versioning with runtime CPU feature dispatch (AVX2, NEON, Baseline).
//! - Vector reduction analysis (Sum, Product, Min, Max).
//! - Generation of dynamic branch dispatchers based on target features.

#![allow(dead_code, unused_imports)]

use adesh_codegen::machine_ir::{
    MachineFunction, MachineInstruction, MachineOperand, MachineRegister, VirtualRegister,
};
use adesh_codegen::opt::vector_dispatch::{
    SimdArchitecture, SimdDispatcher, VectorReduction, VectorReductionKind,
};

#[test]
fn test_simd_multi_version_dispatcher_generation() {
    let mut original = MachineFunction::new("vector_dot_product");
    original.is_exported = true;
    let vreg = original.alloc_vreg();
    let b = original.entry_block_mut();
    b.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(vreg)),
        src: MachineOperand::Immediate(1),
    });
    b.push(MachineInstruction::Return);

    let dispatcher_engine = SimdDispatcher::new(SimdArchitecture::X86Avx2);
    let (dispatcher, baseline, specialized) =
        dispatcher_engine.create_dispatched_multiversion(&original);

    assert_eq!(dispatcher.name, "vector_dot_product");
    assert!(dispatcher.is_exported);
    assert_eq!(baseline.name, "vector_dot_product_baseline");
    assert_eq!(specialized.name, "vector_dot_product_avx2");

    // Dispatcher should contain feature check and branch instructions
    assert!(!dispatcher.blocks[0].instructions.is_empty());
}

#[test]
fn test_vector_reduction_analysis() {
    let mut func = MachineFunction::new("reduce_array");
    let v0 = func.alloc_vreg();
    let v1 = func.alloc_vreg();
    let b = func.entry_block_mut();

    b.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(5),
    });
    b.push(MachineInstruction::Mul {
        dst: MachineOperand::Register(MachineRegister::Virtual(v1)),
        src: MachineOperand::Immediate(2),
    });

    let dispatcher_engine = SimdDispatcher::new(SimdArchitecture::ArmNeon);
    let reductions = dispatcher_engine.analyze_reductions(&func);

    assert_eq!(reductions.len(), 2);
    assert_eq!(reductions[0].kind, VectorReductionKind::Sum);
    assert_eq!(reductions[1].kind, VectorReductionKind::Product);
}
