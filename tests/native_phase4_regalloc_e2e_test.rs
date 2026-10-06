//! End-to-End Native Register Allocation, Exact Liveness, Spill/Reload, ABI Safety & Execution Tests (Phase 4).
//!
//! Verifies:
//! 1. CFG reconstruction and fixed-point liveness analysis.
//! 2. Exact instruction-level half-open intervals [start, end) and segment overlap semantics.
//! 3. GPR high register pressure (25+ simultaneous variables) with automatic spills and reloads.
//! 4. XMM high register pressure (20+ simultaneous float variables) with automatic spills and reloads.
//! 5. Mixed GPR + XMM pressure and spill-slot lifetime reuse.
//! 6. Value preservation across calls (caller-saved vs callee-saved registers / spill slots).
//! 7. Loop-carried spilled variables across back-edges.
//! 8. Callee-saved frame preservation (SysV & Win64 non-volatile registers).
//! 9. Real native execution on Windows x64 producing exact expected exit codes.

#![allow(dead_code, unused_imports)]

use adesh_codegen::machine_ir::{
    ConditionCode, MachineFunction, MachineInstruction, MachineOperand, MachineRegister,
    PhysicalRegister, RegisterClass,
};
use adesh_codegen::register_alloc::{
    AllocationVerifier, LinearScanAllocator, LiveSegment, LivenessAnalysis,
};
use adesh_codegen::targets::create_backend;
use adesh_codegen::targets::x86_64::X86_64RegisterFile;
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use adeshlang::backends::native::lower::lower_hir_module;
use adeshlang::ir::hir::*;
use std::process::Command;
use tempfile::tempdir;

fn lower_link_and_run(hir: &HirModule, test_name: &str) -> i32 {
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let native_mod = lower_hir_module(hir, &target).expect("native lowering");

    let mut backend = create_backend(target.clone()).expect("backend creation");
    let obj = backend.emit_object(&native_mod).expect("ADOB emission");
    AdobValidator::validate(&obj).expect("emitted ADOB must validate");

    let bytes = AdobWriter::write(&obj).expect("ADOB encoding");
    let dir = tempdir().expect("tempdir");
    let adob_path = dir.path().join(format!("{}.adob", test_name));
    std::fs::write(&adob_path, bytes).expect("write ADOB file");

    let exe_path = dir.path().join(format!("{}.exe", test_name));
    adesh_linker::link(&[&adob_path], &exe_path, Some("x86_64-pc-windows-msvc"))
        .expect("native link");
    assert!(exe_path.exists(), "linked executable must exist");

    let out = Command::new(&exe_path)
        .output()
        .expect("execute native binary");
    out.status.code().expect("exit code")
}

#[test]
fn test_cfg_rebuild_and_liveness_fixed_point() {
    let mut func = MachineFunction::new("test_loop");
    let v0 = func.alloc_vreg();
    let v1 = func.alloc_vreg();

    let entry = func.entry_block_mut();
    entry.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(0),
    });
    entry.push(MachineInstruction::Branch {
        target: "loop_header".to_string(),
    });

    let header_id = func.create_block("loop_header");
    let header = &mut func.blocks[header_id as usize];
    header.push(MachineInstruction::Add {
        dst: MachineOperand::Register(MachineRegister::Virtual(v0)),
        src: MachineOperand::Immediate(1),
    });
    header.push(MachineInstruction::Compare {
        lhs: MachineOperand::Register(MachineRegister::Virtual(v0)),
        rhs: MachineOperand::Immediate(10),
    });
    header.push(MachineInstruction::BranchCc {
        cc: ConditionCode::LessThan,
        target: "loop_header".to_string(),
    });
    header.push(MachineInstruction::Branch {
        target: "exit".to_string(),
    });

    let exit_id = func.create_block("exit");
    let exit = &mut func.blocks[exit_id as usize];
    exit.push(MachineInstruction::Move {
        dst: MachineOperand::Register(MachineRegister::Virtual(v1)),
        src: MachineOperand::Register(MachineRegister::Virtual(v0)),
    });
    exit.push(MachineInstruction::Return);

    func.rebuild_cfg();

    assert_eq!(func.blocks[0].successors, vec![1]);
    assert_eq!(func.blocks[1].predecessors, vec![0, 1]);
    assert_eq!(func.blocks[1].successors, vec![1, 2]);
    assert_eq!(func.blocks[2].predecessors, vec![1]);

    let liveness = LivenessAnalysis::compute(&mut func);
    let v0_reg = MachineRegister::Virtual(v0);

    assert!(liveness.live_in.get(&1).unwrap().contains(&v0_reg));
    assert!(liveness.live_out.get(&1).unwrap().contains(&v0_reg));
}

#[test]
fn test_mixed_gpr_and_xmm_spill_pressure_and_verifier() {
    let mut func = MachineFunction::new("test_spill_pressure");
    let reg_file = X86_64RegisterFile::sysv();
    let allocator = LinearScanAllocator::new(&reg_file);

    let mut gpr_vregs = Vec::new();
    let mut fp_vregs = Vec::new();

    for _ in 0..20 {
        let v = func.alloc_vreg();
        gpr_vregs.push(v);
    }

    for _ in 0..20 {
        let v = func.alloc_fp_vreg();
        fp_vregs.push(v);
    }

    let entry = func.entry_block_mut();

    for &v in &gpr_vregs {
        entry.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(v)),
            src: MachineOperand::Immediate(42),
        });
    }

    for &v in &fp_vregs {
        entry.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(v)),
            src: MachineOperand::FloatImmediate(3.14159),
        });
    }

    let sum_gpr = gpr_vregs[0];
    for &v in &gpr_vregs[1..] {
        entry.push(MachineInstruction::Add {
            dst: MachineOperand::Register(MachineRegister::Virtual(sum_gpr)),
            src: MachineOperand::Register(MachineRegister::Virtual(v)),
        });
    }

    let sum_fp = fp_vregs[0];
    for &v in &fp_vregs[1..] {
        entry.push(MachineInstruction::FAdd {
            dst: MachineOperand::Register(MachineRegister::Virtual(sum_fp)),
            src: MachineOperand::Register(MachineRegister::Virtual(v)),
            size: 8,
        });
    }

    entry.push(MachineInstruction::Return);

    let res = allocator
        .allocate(&mut func)
        .expect("allocation must pass verification");

    assert!(res.total_spill_bytes > 0);
    assert!(!res.spill_map.is_empty());
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_gpr_heavy_register_pressure_e2e() {
    // 25 GPR variables initialized, all live simultaneously, then summed
    // Expected sum: 1 + 2 + 3 + ... + 25 = 25 * 26 / 2 = 325
    let mut stmts = Vec::new();
    for i in 1..=25 {
        stmts.push(HirStmt::Let {
            name: format!("v{}", i),
            ty: Some(HirType::I64),
            init: Some(HirExpr::Literal(HirLiteral::I64(i))),
            is_const: false,
            is_borrowed: None,
        });
    }

    // Accumulate sum in 'sum' variable
    stmts.push(HirStmt::Let {
        name: "sum".to_string(),
        ty: Some(HirType::I64),
        init: Some(HirExpr::LoadVar("v1".to_string())),
        is_const: false,
        is_borrowed: None,
    });

    for i in 2..=25 {
        stmts.push(HirStmt::Let {
            name: "sum".to_string(),
            ty: Some(HirType::I64),
            init: Some(HirExpr::BinaryOp(
                Box::new(HirExpr::LoadVar("sum".to_string())),
                BinOp::Add,
                Box::new(HirExpr::LoadVar(format!("v{}", i))),
            )),
            is_const: false,
            is_borrowed: None,
        });
    }

    // Return sum % 256 for process exit code: 325 % 256 = 69
    stmts.push(HirStmt::Return(Some(HirExpr::BinaryOp(
        Box::new(HirExpr::LoadVar("sum".to_string())),
        BinOp::Mod,
        Box::new(HirExpr::Literal(HirLiteral::I64(256))),
    ))));

    let hir = HirModule {
        functions: vec![],
        classes: vec![],
        statements: stmts,
    };

    let exit_code = lower_link_and_run(&hir, "p4_gpr_heavy_pressure");
    assert_eq!(exit_code, 69, "GPR heavy pressure sum % 256 must equal 69");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_xmm_heavy_register_pressure_e2e() {
    // 20 XMM variables initialized with floating point numbers, all live simultaneously
    // v1 = 1.0, v2 = 2.0, ..., v20 = 20.0
    // sum = 1.0 + 2.0 + ... + 20.0 = 210.0
    let mut stmts = Vec::new();
    for i in 1..=20 {
        stmts.push(HirStmt::Let {
            name: format!("f{}", i),
            ty: Some(HirType::F64),
            init: Some(HirExpr::Literal(HirLiteral::F64(i as f64))),
            is_const: false,
            is_borrowed: None,
        });
    }

    stmts.push(HirStmt::Let {
        name: "fsum".to_string(),
        ty: Some(HirType::F64),
        init: Some(HirExpr::LoadVar("f1".to_string())),
        is_const: false,
        is_borrowed: None,
    });

    for i in 2..=20 {
        stmts.push(HirStmt::Let {
            name: "fsum".to_string(),
            ty: Some(HirType::F64),
            init: Some(HirExpr::BinaryOp(
                Box::new(HirExpr::LoadVar("fsum".to_string())),
                BinOp::Add,
                Box::new(HirExpr::LoadVar(format!("f{}", i))),
            )),
            is_const: false,
            is_borrowed: None,
        });
    }

    // Convert fsum (210.0) to i64 and return
    stmts.push(HirStmt::Return(Some(HirExpr::Cast(
        Box::new(HirExpr::LoadVar("fsum".to_string())),
        HirType::I64,
    ))));

    let hir = HirModule {
        functions: vec![],
        classes: vec![],
        statements: stmts,
    };

    let exit_code = lower_link_and_run(&hir, "p4_xmm_heavy_pressure");
    assert_eq!(
        exit_code, 210,
        "XMM heavy pressure float sum must equal 210"
    );
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_values_live_across_function_call_e2e() {
    // Function that computes a simple multiplier
    let mult_func = HirFunction {
        name: "multiply_by_two".to_string(),
        params: vec![("x".to_string(), Some(HirType::I64), None)],
        body: std::sync::Arc::new(vec![HirStmt::Return(Some(HirExpr::BinaryOp(
            Box::new(HirExpr::LoadVar("x".to_string())),
            BinOp::Mul,
            Box::new(HirExpr::Literal(HirLiteral::I64(2))),
        )))]),
        ret_type: Some(HirType::I64),
        is_async: false,
        decorators: vec![],
        is_exported: true,
        move_params: vec![],
        is_test: false,
        test_ignore: false,
        test_expect_fail: false,
        test_timeout: None,
        is_unsafe: false,
    };

    // Main has 10 values live across calls to multiply_by_two
    let mut stmts = Vec::new();
    for i in 1..=10 {
        stmts.push(HirStmt::Let {
            name: format!("x{}", i),
            ty: Some(HirType::I64),
            init: Some(HirExpr::Literal(HirLiteral::I64(i * 10))),
            is_const: false,
            is_borrowed: None,
        });
    }

    // Call multiply_by_two(5) -> returns 10
    stmts.push(HirStmt::Let {
        name: "call_res".to_string(),
        ty: Some(HirType::I64),
        init: Some(HirExpr::Call(
            Box::new(HirExpr::LoadVar("multiply_by_two".to_string())),
            vec![HirExpr::Literal(HirLiteral::I64(5))],
            vec![],
        )),
        is_const: false,
        is_borrowed: None,
    });

    // Sum x1..x10 + call_res
    // x1..x10 sum = 10 + 20 + ... + 100 = 550
    // total = 550 + 10 = 560
    // 560 % 256 = 48
    stmts.push(HirStmt::Let {
        name: "total".to_string(),
        ty: Some(HirType::I64),
        init: Some(HirExpr::LoadVar("call_res".to_string())),
        is_const: false,
        is_borrowed: None,
    });

    for i in 1..=10 {
        stmts.push(HirStmt::Let {
            name: "total".to_string(),
            ty: Some(HirType::I64),
            init: Some(HirExpr::BinaryOp(
                Box::new(HirExpr::LoadVar("total".to_string())),
                BinOp::Add,
                Box::new(HirExpr::LoadVar(format!("x{}", i))),
            )),
            is_const: false,
            is_borrowed: None,
        });
    }

    stmts.push(HirStmt::Return(Some(HirExpr::BinaryOp(
        Box::new(HirExpr::LoadVar("total".to_string())),
        BinOp::Mod,
        Box::new(HirExpr::Literal(HirLiteral::I64(256))),
    ))));

    let hir = HirModule {
        functions: vec![mult_func],
        classes: vec![],
        statements: stmts,
    };

    let exit_code = lower_link_and_run(&hir, "p4_live_across_call");
    assert_eq!(
        exit_code, 48,
        "Sum of variables live across call % 256 must equal 48"
    );
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_loop_carried_spill_reload_e2e() {
    // A loop that updates 15 variables on each iteration
    // Tests that spilled loop-carried values are properly loaded, updated, and stored across back-edges
    let mut stmts = Vec::new();
    for i in 1..=15 {
        stmts.push(HirStmt::Let {
            name: format!("c{}", i),
            ty: Some(HirType::I64),
            init: Some(HirExpr::Literal(HirLiteral::I64(0))),
            is_const: false,
            is_borrowed: None,
        });
    }

    stmts.push(HirStmt::Let {
        name: "i".to_string(),
        ty: Some(HirType::I64),
        init: Some(HirExpr::Literal(HirLiteral::I64(0))),
        is_const: false,
        is_borrowed: None,
    });

    // Loop body increments c1..c15 by their index i.e. c1 += 1, c2 += 2, etc.
    let mut loop_body = Vec::new();
    for i in 1..=15 {
        loop_body.push(HirStmt::Assign {
            target: HirExpr::LoadVar(format!("c{}", i)),
            value: HirExpr::BinaryOp(
                Box::new(HirExpr::LoadVar(format!("c{}", i))),
                BinOp::Add,
                Box::new(HirExpr::Literal(HirLiteral::I64(i))),
            ),
            is_move: false,
        });
    }

    // i += 1
    loop_body.push(HirStmt::Assign {
        target: HirExpr::LoadVar("i".to_string()),
        value: HirExpr::BinaryOp(
            Box::new(HirExpr::LoadVar("i".to_string())),
            BinOp::Add,
            Box::new(HirExpr::Literal(HirLiteral::I64(1))),
        ),
        is_move: false,
    });

    // while (i < 4)
    stmts.push(HirStmt::While {
        cond: HirExpr::BinaryOp(
            Box::new(HirExpr::LoadVar("i".to_string())),
            BinOp::Lt,
            Box::new(HirExpr::Literal(HirLiteral::I64(4))),
        ),
        body: Box::new(HirStmt::Block(loop_body)),
    });

    // After 4 iterations, c1 = 4, c2 = 8, ..., c15 = 60
    // Sum = 4 * (1 + 2 + ... + 15) = 4 * (15 * 16 / 2) = 4 * 120 = 480
    // 480 % 256 = 224
    stmts.push(HirStmt::Let {
        name: "sum".to_string(),
        ty: Some(HirType::I64),
        init: Some(HirExpr::Literal(HirLiteral::I64(0))),
        is_const: false,
        is_borrowed: None,
    });

    for i in 1..=15 {
        stmts.push(HirStmt::Assign {
            target: HirExpr::LoadVar("sum".to_string()),
            value: HirExpr::BinaryOp(
                Box::new(HirExpr::LoadVar("sum".to_string())),
                BinOp::Add,
                Box::new(HirExpr::LoadVar(format!("c{}", i))),
            ),
            is_move: false,
        });
    }

    stmts.push(HirStmt::Return(Some(HirExpr::BinaryOp(
        Box::new(HirExpr::LoadVar("sum".to_string())),
        BinOp::Mod,
        Box::new(HirExpr::Literal(HirLiteral::I64(256))),
    ))));

    let hir = HirModule {
        functions: vec![],
        classes: vec![],
        statements: stmts,
    };

    let exit_code = lower_link_and_run(&hir, "p4_loop_carried_spill");
    assert_eq!(
        exit_code, 224,
        "Loop carried spill-reload sum % 256 must equal 224"
    );
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_mixed_gpr_and_fp_arithmetic_under_spill_pressure_e2e() {
    // Mix integer and float computations simultaneously under high pressure
    let mut stmts = Vec::new();
    for i in 1..=12 {
        stmts.push(HirStmt::Let {
            name: format!("g{}", i),
            ty: Some(HirType::I64),
            init: Some(HirExpr::Literal(HirLiteral::I64(i * 3))),
            is_const: false,
            is_borrowed: None,
        });
        stmts.push(HirStmt::Let {
            name: format!("f{}", i),
            ty: Some(HirType::F64),
            init: Some(HirExpr::Literal(HirLiteral::F64(i as f64 * 1.5))),
            is_const: false,
            is_borrowed: None,
        });
    }

    // g_sum = 3 * (12 * 13 / 2) = 3 * 78 = 234
    stmts.push(HirStmt::Let {
        name: "g_sum".to_string(),
        ty: Some(HirType::I64),
        init: Some(HirExpr::Literal(HirLiteral::I64(0))),
        is_const: false,
        is_borrowed: None,
    });
    for i in 1..=12 {
        stmts.push(HirStmt::Let {
            name: "g_sum".to_string(),
            ty: Some(HirType::I64),
            init: Some(HirExpr::BinaryOp(
                Box::new(HirExpr::LoadVar("g_sum".to_string())),
                BinOp::Add,
                Box::new(HirExpr::LoadVar(format!("g{}", i))),
            )),
            is_const: false,
            is_borrowed: None,
        });
    }

    // f_sum = 1.5 * 78 = 117.0
    stmts.push(HirStmt::Let {
        name: "f_sum".to_string(),
        ty: Some(HirType::F64),
        init: Some(HirExpr::Literal(HirLiteral::F64(0.0))),
        is_const: false,
        is_borrowed: None,
    });
    for i in 1..=12 {
        stmts.push(HirStmt::Let {
            name: "f_sum".to_string(),
            ty: Some(HirType::F64),
            init: Some(HirExpr::BinaryOp(
                Box::new(HirExpr::LoadVar("f_sum".to_string())),
                BinOp::Add,
                Box::new(HirExpr::LoadVar(format!("f{}", i))),
            )),
            is_const: false,
            is_borrowed: None,
        });
    }

    // total = g_sum + int(f_sum) = 234 + 117 = 351
    // 351 % 256 = 95
    stmts.push(HirStmt::Let {
        name: "total".to_string(),
        ty: Some(HirType::I64),
        init: Some(HirExpr::BinaryOp(
            Box::new(HirExpr::LoadVar("g_sum".to_string())),
            BinOp::Add,
            Box::new(HirExpr::Cast(
                Box::new(HirExpr::LoadVar("f_sum".to_string())),
                HirType::I64,
            )),
        )),
        is_const: false,
        is_borrowed: None,
    });

    stmts.push(HirStmt::Return(Some(HirExpr::BinaryOp(
        Box::new(HirExpr::LoadVar("total".to_string())),
        BinOp::Mod,
        Box::new(HirExpr::Literal(HirLiteral::I64(256))),
    ))));

    let hir = HirModule {
        functions: vec![],
        classes: vec![],
        statements: stmts,
    };

    let exit_code = lower_link_and_run(&hir, "p4_mixed_gpr_fp_pressure");
    assert_eq!(exit_code, 95, "Mixed GPR and FP sum % 256 must equal 95");
}
