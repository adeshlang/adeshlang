//! # Adesh CodeGen (`adesh-codegen`)
//!
//! Universal target backend code generation framework and native CPU/GPU/Accelerator codegen for Adesh.

#![allow(clippy::result_large_err)]

pub mod abi;
pub mod accelerators;
pub mod backend;
pub mod calling_convention;
pub mod concurrency;
pub mod cranelift_adapter;
pub mod debug_info;
pub mod driver;
pub mod error;
pub mod ffi;
pub mod machine_ir;
pub mod opt;
pub mod register_alloc;
pub mod safety;
pub mod stack_maps;
pub mod stdlib_builder;
pub mod target_spec;
pub mod targets;
pub mod unwind_info;

pub use abi::{
    Aapcs64Abi, AbiSpec, AbiType, AggregateReturnRules, CalleeSavedSet, CallerSavedSet,
    RedZone, ReturnLocation, RiscV64Abi, ShadowSpace, StackAlignment, StackArgument,
    StackFrameLayout, StructPassingRules, SystemVX64Abi, UnwindRules, VariadicRules,
    WindowsX64Abi, create_abi_spec,
};
pub use accelerators::{GpuBackend, TensorAcceleratorBackend};
pub use backend::{AcceleratorBackend, CodegenBackend};
pub use calling_convention::{
    Aapcs32CallingConvention, Aapcs64CallingConvention, ArgumentLocation, CallingConvention,
    MoveLocation, MoveOperation, ParallelMoveResolver, RiscVCallingConvention,
    SystemVX64CallingConvention, WasmCallingConvention, WindowsX64CallingConvention,
    resolve_call_arguments,
};
pub use concurrency::{AtomicOp, ConcurrencyPass, MemoryOrder};
pub use cranelift_adapter::CraneliftAdapter;
pub use debug_info::{FunctionDebugMetadata, LineTableEntry, ModuleDebugInfo, SourceLocation};
pub use driver::{
    CompilationCacheKey, CompilerDriver, CompilerResourceLimits, CompilerStats, DriverConfig,
};
pub use error::CodegenError;
pub use ffi::{
    FfiCallLowerer, ForeignCallingConvention, ForeignFunctionDeclaration, ForeignParam,
    ForeignSignature, ForeignType,
};
pub use machine_ir::{
    ConditionCode, MachineBlock, MachineFunction, MachineInstruction, MachineOperand,
    MachineRegister, NativeModule, PhysicalRegister, VirtualRegister,
};
pub use opt::{
    AliasAnalysis, ArrayOptimizer, AutoVectorizePass, BasicBlockScheduler, BlockProfile,
    CpuFeatures, DeadCodeElimination, DominatorTree, EdgeProfile, FunctionProfile, FunctionSummary,
    LtoConfig, LtoEngine, LtoMode, LtoReport, ModuleSummary, OptLevel, OptimizationPipeline,
    PeepholeOptimizer, PgoInstrumentationPass, PgoOptimizationPass, ProfileData, SwitchCase,
    SwitchLowering, SwitchStrategy, VectorCostModel, VectorElementType, VectorType,
};
pub use register_alloc::{LinearScanAllocator, RegisterFile};
pub use safety::{ControlFlowIntegrityPass, StackCanaryPass};
pub use stack_maps::{FunctionStackMap, LiveLocationKind, LiveLocationRecord, SafepointRecord};
pub use stdlib_builder::StdlibAdobBuilder;
pub use target_spec::{
    CodeModel, Endianness, ObjectFormatKind, RelocationModel, TargetAbi, TargetSpec,
};
pub use targets::{
    aarch64::AArch64Backend, create_backend, riscv::RiscVBackend, x86_64::X86_64Backend,
};
pub use unwind_info::{DwarfCallFrameInfo, FunctionUnwindDescriptor, Win64UnwindInfo};

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
        // r0 = load [rsp]; r0 = r0 * 8; r1 = 999; return
        block.push(MachineInstruction::Load {
            dst: MachineOperand::Register(MachineRegister::Virtual(r0)),
            src: MachineOperand::StackSlot(-8),
            size: 8,
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

        let mut pipeline = OptimizationPipeline::new(OptLevel::O2);
        let improvements = pipeline
            .optimize_function_pre_alloc(&mut func)
            .expect("opt succeeds");
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
