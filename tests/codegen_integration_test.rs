use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule,
};
use adesh_codegen::opt::{
    ArrayOptimizer, DeadCodeElimination, OptimizationPipeline, PeepholeOptimizer,
};
use adesh_codegen::safety::StackCanaryPass;
use adesh_codegen::targets::create_backend;
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;

#[test]
fn test_x86_64_backend_codegen_and_adob() {
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let mut backend = create_backend(target).expect("backend creation succeeds");

    let mut module = NativeModule::new("test_module");
    let mut func = MachineFunction::new("add_func");
    func.is_exported = true;

    let v0 = func.alloc_vreg();
    let v1 = func.alloc_vreg();

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
    block.push(MachineInstruction::Return);

    module.add_function(func);

    let lowered = backend
        .lower_module(&module)
        .expect("Failed to lower module");
    let adob = backend.emit_object(&lowered).expect("Failed to emit ADOB");

    assert!(!adob.sections.is_empty());
    assert!(!adob.symbols.is_empty());
    assert_eq!(adob.symbols[0].name, "add_func");

    AdobValidator::validate(&adob).expect("Generated ADOB must be valid");
}

#[test]
fn test_aarch64_backend_codegen() {
    let target = TargetDescriptor::from_triple("aarch64-unknown-linux-gnu").expect("valid triple");
    let mut backend = create_backend(target).expect("backend creation succeeds");

    let mut module = NativeModule::new("test_aarch64");
    let mut func = MachineFunction::new("foo");
    let block = func.entry_block_mut();
    block.push(MachineInstruction::Return);
    module.add_function(func);

    let lowered = backend
        .lower_module(&module)
        .expect("Failed to lower module");
    let adob = backend.emit_object(&lowered).expect("Failed to emit ADOB");

    AdobValidator::validate(&adob).expect("Generated AArch64 ADOB must be valid");
}

#[test]
fn test_riscv_backend_codegen() {
    let target =
        TargetDescriptor::from_triple("riscv64gc-unknown-linux-gnu").expect("valid triple");
    let mut backend = create_backend(target).expect("backend creation succeeds");

    let mut module = NativeModule::new("test_riscv");
    let mut func = MachineFunction::new("bar");
    let block = func.entry_block_mut();
    block.push(MachineInstruction::Return);
    module.add_function(func);

    let lowered = backend
        .lower_module(&module)
        .expect("Failed to lower module");
    let adob = backend.emit_object(&lowered).expect("Failed to emit ADOB");

    AdobValidator::validate(&adob).expect("Generated RISC-V ADOB must be valid");
}

#[test]
fn test_binary_optimization_and_size_reduction() {
    let mut func = MachineFunction::new("optimized_calc");
    let r0 = func.alloc_vreg();
    let r1 = func.alloc_vreg();
    let dead_r = func.alloc_vreg();

    let block = func.entry_block_mut();
    // r0 = 100; r0 = r0 * 4 (strength reduced to shl 2); dead_r = 500 (DCE); return
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(r0)),
        src: MachineOperand::Immediate(100),
    });
    block.push(MachineInstruction::Mul {
        dst: MachineOperand::Register(MachineRegister::Virtual(r0)),
        src: MachineOperand::Immediate(4),
    });
    block.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(dead_r)),
        src: MachineOperand::Immediate(500),
    });
    block.push(MachineInstruction::Return);

    let pipeline = OptimizationPipeline::new(3);
    let total_savings = pipeline.optimize_function(&mut func);
    assert!(
        total_savings > 0,
        "Optimization passes should improve instruction stream"
    );

    let has_shl = func.blocks[0]
        .instructions
        .iter()
        .any(|i| matches!(i, MachineInstruction::Shl { .. }));
    assert!(
        has_shl,
        "Mul by 4 must be strength reduced to shift left by 2"
    );
}

#[test]
fn test_stack_canary_memory_safety() {
    let mut func = MachineFunction::new("protected_entry");
    let block = func.entry_block_mut();
    block.push(MachineInstruction::Return);

    let canary_pass = StackCanaryPass::new();
    canary_pass.instrument_function(&mut func);

    // Verify stack protection
    let has_canary_branch = func.blocks[0].instructions.iter().any(|i| {
        if let MachineInstruction::BranchCc { target, .. } = i {
            target == "__stack_chk_fail"
        } else {
            false
        }
    });
    assert!(
        has_canary_branch,
        "Function must contain stack canary guard comparison and trap branch"
    );
}

#[test]
fn test_array_bounds_check_elimination() {
    let mut func = MachineFunction::new("array_access_fn");
    let block = func.entry_block_mut();

    // Constant in-bounds access: index 3 < len 10
    block.push(MachineInstruction::Compare {
        lhs: MachineOperand::Immediate(3),
        rhs: MachineOperand::Immediate(10),
    });
    block.push(MachineInstruction::BranchCc {
        cc: ConditionCode::AboveOrEqual,
        target: "panic_out_of_bounds".to_string(),
    });
    block.push(MachineInstruction::Return);

    let bce = ArrayOptimizer::new();
    let eliminated = bce.optimize_function(&mut func);
    assert_eq!(
        eliminated, 1,
        "Statically proven in-bounds access check must be eliminated"
    );
    assert_eq!(
        func.blocks[0].instructions.len(),
        1,
        "Only Return instruction should remain"
    );
}
