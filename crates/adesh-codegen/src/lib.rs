//! # Adesh CodeGen (`adesh-codegen`)
//!
//! Universal target backend code generation framework and native CPU/GPU/Accelerator codegen for Adesh.

#![allow(clippy::result_large_err)]

pub mod accelerators;
pub mod backend;
pub mod calling_convention;
pub mod concurrency;
pub mod cranelift_adapter;
pub mod error;
pub mod machine_ir;
pub mod opt;
pub mod register_alloc;
pub mod safety;
pub mod stdlib_builder;
pub mod targets;

pub use accelerators::{GpuBackend, TensorAcceleratorBackend};
pub use backend::{AcceleratorBackend, CodegenBackend};
pub use calling_convention::{
    Aapcs32CallingConvention, Aapcs64CallingConvention, CallingConvention, RiscVCallingConvention,
    SystemVX64CallingConvention, WasmCallingConvention, WindowsX64CallingConvention,
};
pub use concurrency::{AtomicOp, ConcurrencyPass, MemoryOrder};
pub use cranelift_adapter::CraneliftAdapter;
pub use error::CodegenError;
pub use machine_ir::{
    ConditionCode, MachineBlock, MachineFunction, MachineInstruction, MachineOperand,
    MachineRegister, NativeModule, PhysicalRegister, VirtualRegister,
};
pub use opt::{
    ArrayOptimizer, DeadCodeElimination, OptimizationPipeline, PeepholeOptimizer, SwitchCase,
    SwitchLowering, SwitchStrategy,
};
pub use register_alloc::{LinearScanAllocator, RegisterFile};
pub use safety::{ControlFlowIntegrityPass, StackCanaryPass};
pub use stdlib_builder::StdlibAdobBuilder;
pub use targets::{create_backend, x86_64::X86_64Backend};

#[cfg(test)]
mod tests {
    use super::*;
    use adesh_object::TargetDescriptor;

    #[test]
    fn test_x86_64_backend_codegen() {
        let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
        let mut backend = X86_64Backend::new(target);

        let mut module = NativeModule::new("test_mod");
        let mut func = MachineFunction::new("add");
        func.is_exported = true;

        // r0 = 42; r1 = 10; r0 = r0 + r1; return
        let r0 = func.alloc_vreg();
        let r1 = func.alloc_vreg();

        let block = func.entry_block_mut();
        block.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(r0)),
            src: MachineOperand::Immediate(42),
        });
        block.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(r1)),
            src: MachineOperand::Immediate(10),
        });
        block.push(MachineInstruction::Add {
            dst: MachineOperand::Register(MachineRegister::Virtual(r0)),
            src: MachineOperand::Register(MachineRegister::Virtual(r1)),
        });
        block.push(MachineInstruction::Return);

        module.add_function(func);

        let lowered = backend.lower_module(&module).expect("lowering succeeds");
        let adob = backend
            .emit_object(&lowered)
            .expect("adob emission succeeds");

        assert_eq!(adob.sections.len(), 1);
        assert_eq!(adob.symbols.len(), 1);
        assert_eq!(adob.symbols[0].name, "add");
        assert_eq!(adob.exports, vec!["add".to_string()]);
    }

    #[test]
    fn test_optimization_pipeline_strength_reduction_and_dce() {
        let mut func = MachineFunction::new("opt_test");
        let r0 = func.alloc_vreg();
        let r1 = func.alloc_vreg(); // unused dead reg

        let block = func.entry_block_mut();
        // r0 = 10; r0 = r0 * 8; r1 = 999; return
        block.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(r0)),
            src: MachineOperand::Immediate(10),
        });
        block.push(MachineInstruction::Mul {
            dst: MachineOperand::Register(MachineRegister::Virtual(r0)),
            src: MachineOperand::Immediate(8), // should become shl r0, 3
        });
        block.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(r1)),
            src: MachineOperand::Immediate(999), // should be eliminated by DCE
        });
        block.push(MachineInstruction::Return);

        let pipeline = OptimizationPipeline::new(2);
        let improvements = pipeline.optimize_function(&mut func);
        assert!(improvements > 0);

        // Check that mul was strength reduced to shl
        let has_shl = func.blocks[0]
            .instructions
            .iter()
            .any(|i| matches!(i, MachineInstruction::Shl { .. }));
        assert!(has_shl, "Mul by 8 should be strength-reduced to Shl by 3");
    }

    #[test]
    fn test_stack_canary_instrumentation() {
        let mut func = MachineFunction::new("secure_func");
        let block = func.entry_block_mut();
        block.push(MachineInstruction::Return);

        let canary_pass = StackCanaryPass::new();
        canary_pass.instrument_function(&mut func);

        // Verify that entry block now contains canary setup and epilogue contains canary check
        assert!(
            func.blocks[0]
                .instructions
                .iter()
                .any(|i| matches!(i, MachineInstruction::BranchCc { .. }))
        );
    }
}
