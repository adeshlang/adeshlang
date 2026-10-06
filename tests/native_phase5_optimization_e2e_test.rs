//! End-to-End Native Backend Optimization, Instruction Selection, Code Quality & Differential Tests (Phase 5).
//!
//! Verifies:
//! 1. Constant folding and propagation for integer and floating-point computations.
//! 2. Dead-code elimination (unused instructions, dead variables, unreachable blocks).
//! 3. Copy propagation and redundant move elimination.
//! 4. Move coalescing and virtual register merging.
//! 5. Local CSE (Common Subexpression Elimination) and value numbering.
//! 6. Strength reduction (power-of-two multiplication -> shift, algebraic identities).
//! 7. Branch optimization (jump threading, branch inversion, block layout).
//! 8. Tail-call optimization (self/direct tail-call lowering to direct jumps).
//! 9. Stack-frame compaction and 16-byte alignment optimization.
//! 10. String literal deduplication in `.rodata`.
//! 11. Multi-level optimization differential testing (-O0 vs -O1 vs -O2 semantic equivalence).

#![allow(dead_code, unused_imports)]

use adesh_codegen::CodegenBackend;
use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    NativeModule, PhysicalRegister, RegisterClass, VirtualRegister,
};
use adesh_codegen::opt::{
    BranchOptimizationPass, ConstantFoldingPass, CopyPropagationPass, DeadCodeElimination,
    LocalCSEPass, MachinePass, MoveCoalescingPass, OptLevel, OptimizationPipeline,
    StackFrameOptimizationPass, StrengthReductionPass, TailCallOptimizationPass,
};
use adesh_codegen::targets::create_backend;
use adesh_codegen::targets::x86_64::X86_64Backend;
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use adeshlang::backends::native::lower::lower_hir_module;
use adeshlang::ir::hir::*;
use std::process::Command;
use tempfile::tempdir;

fn emit_link_and_run(native_mod: &NativeModule, opt_level: OptLevel, test_name: &str) -> i32 {
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let mut backend = X86_64Backend::new(target).with_opt_level(opt_level);

    let obj = backend.emit_object(native_mod).expect("ADOB emission");
    AdobValidator::validate(&obj).expect("emitted ADOB must validate");

    let bytes = AdobWriter::write(&obj).expect("ADOB encoding");
    let dir = tempdir().expect("tempdir");
    let adob_path = dir
        .path()
        .join(format!("{}_{:?}.adob", test_name, opt_level));
    std::fs::write(&adob_path, bytes).expect("write ADOB file");

    let exe_path = dir
        .path()
        .join(format!("{}_{:?}.exe", test_name, opt_level));
    adesh_linker::link(&[&adob_path], &exe_path, Some("x86_64-pc-windows-msvc"))
        .expect("native link");
    assert!(exe_path.exists(), "linked executable must exist");

    let out = Command::new(&exe_path)
        .output()
        .expect("execute native binary");
    out.status.code().expect("exit code")
}

fn lower_link_and_run_opt(hir: &HirModule, opt_level: OptLevel, test_name: &str) -> i32 {
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let native_mod = lower_hir_module(hir, &target).expect("native lowering");
    emit_link_and_run(&native_mod, opt_level, test_name)
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_constant_folding_and_propagation_e2e() {
    // Computes: a = 20; b = 22; c = a + b; return c (should fold to 42)
    let mut func = MachineFunction::new("main");
    func.is_exported = true;
    let v0 = func.alloc_vreg();
    let v1 = func.alloc_vreg();
    let v2 = func.alloc_vreg();

    let entry = func.entry_block_mut();
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(20),
    });
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v1)),
        src: MachineOperand::Immediate(22),
    });
    entry.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Register(MachineRegister::Virtual(v1)),
    });
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v2)),
        src: MachineOperand::Register(MachineRegister::Virtual(v0)),
    });
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))), // RAX
        src: MachineOperand::Register(MachineRegister::Virtual(v2)),
    });
    entry.push(MachineInstruction::Return);

    let mut module = NativeModule::new("test_const_fold");
    module.add_function(func);

    let exit_code = emit_link_and_run(&module, OptLevel::O2, "const_fold");
    assert_eq!(exit_code, 42);
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_dead_code_elimination_e2e() {
    // Computes: dead_1 = 100; dead_2 = 200; res = 37; return res
    let mut func = MachineFunction::new("main");
    func.is_exported = true;
    let d1 = func.alloc_vreg();
    let d2 = func.alloc_vreg();
    let res = func.alloc_vreg();

    let entry = func.entry_block_mut();
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(d1)),
        src: MachineOperand::Immediate(100),
    });
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(d2)),
        src: MachineOperand::Immediate(200),
    });
    entry.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(d1)),
        src: MachineOperand::Register(MachineRegister::Virtual(d2)),
    });
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(res)),
        src: MachineOperand::Immediate(37),
    });
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(res)),
    });
    entry.push(MachineInstruction::Return);

    let mut module = NativeModule::new("test_dce");
    module.add_function(func);

    let exit_code = emit_link_and_run(&module, OptLevel::O2, "dce_test");
    assert_eq!(exit_code, 37);
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_strength_reduction_and_identities_e2e() {
    // Tests: val = 5; val = val * 8 (shl 3 -> 40); val = val + 0; val = val * 1; val = val + 2; return val (42)
    let mut func = MachineFunction::new("main");
    func.is_exported = true;
    let v0 = func.alloc_vreg();

    let entry = func.entry_block_mut();
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(5),
    });
    entry.push(MachineInstruction::Mul {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(8),
    });
    entry.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(0),
    });
    entry.push(MachineInstruction::Mul {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(1),
    });
    entry.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(2),
    });
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(v0)),
    });
    entry.push(MachineInstruction::Return);

    let mut module = NativeModule::new("test_strength_red");
    module.add_function(func);

    let exit_code = emit_link_and_run(&module, OptLevel::O2, "strength_red");
    assert_eq!(exit_code, 42);
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_branch_optimization_and_jump_threading_e2e() {
    // Creates a jump chain: entry -> b1 -> b2 -> b3 (exit)
    // Branch optimization should thread entry directly to b3 or collapse the chain.
    let mut func = MachineFunction::new("main");
    func.is_exported = true;
    let v0 = func.alloc_vreg();

    let entry = func.entry_block_mut();
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(55),
    });
    entry.push(MachineInstruction::Branch {
        target: "b1".to_string(),
    });

    let b1_id = func.create_block("b1");
    let b1 = &mut func.blocks[b1_id as usize];
    b1.push(MachineInstruction::Branch {
        target: "b2".to_string(),
    });

    let b2_id = func.create_block("b2");
    let b2 = &mut func.blocks[b2_id as usize];
    b2.push(MachineInstruction::Branch {
        target: "b3".to_string(),
    });

    let b3_id = func.create_block("b3");
    let b3 = &mut func.blocks[b3_id as usize];
    b3.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(v0)),
    });
    b3.push(MachineInstruction::Return);

    func.rebuild_cfg();

    let mut module = NativeModule::new("test_branch_opt");
    module.add_function(func);

    let exit_code = emit_link_and_run(&module, OptLevel::O2, "branch_opt");
    assert_eq!(exit_code, 55);
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_copy_propagation_and_coalescing_e2e() {
    // Long copy chain: v0 = 88; v1 = v0; v2 = v1; v3 = v2; v4 = v3; return v4
    let mut func = MachineFunction::new("main");
    func.is_exported = true;
    let v0 = func.alloc_vreg();
    let v1 = func.alloc_vreg();
    let v2 = func.alloc_vreg();
    let v3 = func.alloc_vreg();
    let v4 = func.alloc_vreg();

    let entry = func.entry_block_mut();
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(88),
    });
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v1)),
        src: MachineOperand::Register(MachineRegister::Virtual(v0)),
    });
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v2)),
        src: MachineOperand::Register(MachineRegister::Virtual(v1)),
    });
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v3)),
        src: MachineOperand::Register(MachineRegister::Virtual(v2)),
    });
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v4)),
        src: MachineOperand::Register(MachineRegister::Virtual(v3)),
    });
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(v4)),
    });
    entry.push(MachineInstruction::Return);

    let mut module = NativeModule::new("test_copy_prop");
    module.add_function(func);

    let exit_code = emit_link_and_run(&module, OptLevel::O2, "copy_prop");
    assert_eq!(exit_code, 88);
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_local_cse_e2e() {
    // a = 15; b = 7; x = a + b (22); y = a + b (reused 22); res = x + y (44)
    let mut func = MachineFunction::new("main");
    func.is_exported = true;
    let a = func.alloc_vreg();
    let b = func.alloc_vreg();
    let x = func.alloc_vreg();
    let y = func.alloc_vreg();
    let res = func.alloc_vreg();

    let entry = func.entry_block_mut();
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(a)),
        src: MachineOperand::Immediate(15),
    });
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(b)),
        src: MachineOperand::Immediate(7),
    });
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(x)),
        src: MachineOperand::Register(MachineRegister::Virtual(a)),
    });
    entry.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(x)),
        src: MachineOperand::Register(MachineRegister::Virtual(b)),
    });
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(y)),
        src: MachineOperand::Register(MachineRegister::Virtual(a)),
    });
    entry.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(y)),
        src: MachineOperand::Register(MachineRegister::Virtual(b)),
    });
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(res)),
        src: MachineOperand::Register(MachineRegister::Virtual(x)),
    });
    entry.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(res)),
        src: MachineOperand::Register(MachineRegister::Virtual(y)),
    });
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Register(MachineRegister::Virtual(res)),
    });
    entry.push(MachineInstruction::Return);

    let mut module = NativeModule::new("test_cse");
    module.add_function(func);

    let exit_code = emit_link_and_run(&module, OptLevel::O2, "cse_test");
    assert_eq!(exit_code, 44);
}

#[test]
fn test_tail_call_optimization_pass() {
    let mut func = MachineFunction::new("factorial_tail");
    let entry = func.entry_block_mut();
    entry.push(MachineInstruction::Call {
        target: MachineOperand::Symbol("factorial_tail".to_string()),
        num_args: 2,
    });
    entry.push(MachineInstruction::Return);

    let mut tco = TailCallOptimizationPass::new();
    let changed = tco.run_on_function(&mut func).expect("tco success");
    assert!(changed, "Self-tail-call must be converted to direct branch");

    assert!(matches!(
        func.blocks[0].instructions[0],
        MachineInstruction::Branch { .. }
    ));
}

#[test]
fn test_rodata_string_deduplication() {
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let mut backend = X86_64Backend::new(target).with_opt_level(OptLevel::O2);

    let mut module = NativeModule::new("test_strings");
    module.string_pool.push("hello world".to_string());
    module.string_pool.push("unique string".to_string());
    module.string_pool.push("hello world".to_string()); // duplicate string literal

    let mut func = MachineFunction::new("main");
    func.is_exported = true;
    let entry = func.entry_block_mut();
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Physical(PhysicalRegister(0))),
        src: MachineOperand::Immediate(0),
    });
    entry.push(MachineInstruction::Return);
    module.add_function(func);

    let obj = backend.emit_object(&module).expect("ADOB emission");
    let (_, rodata) = obj.find_section(".rodata").expect("rodata section present");

    // "hello world\0" is 12 bytes, "unique string\0" is 14 bytes.
    // Total deduplicated bytes should be 12 + 14 = 26 bytes, not 12 + 14 + 12 = 38 bytes!
    assert_eq!(
        rodata.data.len(),
        26,
        "Duplicate string literals must be deduplicated in .rodata"
    );
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_differential_opt_levels_equivalence_e2e() {
    // HIR loop summing 1 to 10: sum = 0; i = 1; while (i < 11) { sum = sum + i; i = i + 1 }; return sum (55)
    let mut stmts = Vec::new();

    stmts.push(HirStmt::Let {
        name: "sum".to_string(),
        ty: Some(HirType::I64),
        init: Some(HirExpr::Literal(HirLiteral::I64(0))),
        is_const: false,
        is_borrowed: None,
    });

    stmts.push(HirStmt::Let {
        name: "i".to_string(),
        ty: Some(HirType::I64),
        init: Some(HirExpr::Literal(HirLiteral::I64(1))),
        is_const: false,
        is_borrowed: None,
    });

    let loop_body = vec![
        HirStmt::Assign {
            target: HirExpr::LoadVar("sum".to_string()),
            value: HirExpr::BinaryOp(
                Box::new(HirExpr::LoadVar("sum".to_string())),
                BinOp::Add,
                Box::new(HirExpr::LoadVar("i".to_string())),
            ),
            is_move: false,
        },
        HirStmt::Assign {
            target: HirExpr::LoadVar("i".to_string()),
            value: HirExpr::BinaryOp(
                Box::new(HirExpr::LoadVar("i".to_string())),
                BinOp::Add,
                Box::new(HirExpr::Literal(HirLiteral::I64(1))),
            ),
            is_move: false,
        },
    ];

    stmts.push(HirStmt::While {
        cond: HirExpr::BinaryOp(
            Box::new(HirExpr::LoadVar("i".to_string())),
            BinOp::Lt,
            Box::new(HirExpr::Literal(HirLiteral::I64(11))),
        ),
        body: Box::new(HirStmt::Block(loop_body)),
    });

    stmts.push(HirStmt::Return(Some(HirExpr::LoadVar("sum".to_string()))));

    let hir_module = HirModule {
        functions: vec![],
        classes: vec![],
        statements: stmts,
    };

    let res_o0 = lower_link_and_run_opt(&hir_module, OptLevel::O0, "diff_o0");
    let res_o1 = lower_link_and_run_opt(&hir_module, OptLevel::O1, "diff_o1");
    let res_o2 = lower_link_and_run_opt(&hir_module, OptLevel::O2, "diff_o2");

    assert_eq!(res_o0, 55, "-O0 should compute sum(1..=10) == 55");
    assert_eq!(res_o1, 55, "-O1 should compute sum(1..=10) == 55");
    assert_eq!(res_o2, 55, "-O2 should compute sum(1..=10) == 55");
    assert_eq!(
        res_o0, res_o1,
        "Optimization levels must preserve semantics (-O0 == -O1)"
    );
    assert_eq!(
        res_o1, res_o2,
        "Optimization levels must preserve semantics (-O1 == -O2)"
    );
}
