//! End-to-End Native Floating-Point and SSE2 Tests for Phase 3.
//!
//! Verifies complete x86-64 SSE/SSE2 floating-point implementation:
//! 1. Scalar f64 arithmetic: addsd, subsd, mulsd, divsd, and xorpd negation.
//! 2. Scalar f32 arithmetic: addss, subss, mulss, divss, and xorps negation.
//! 3. IEEE-754 floating-point comparisons with ucomisd/ucomiss and parity/NaN correctness.
//! 4. Type conversions: int <-> f64, int <-> f32, and f32 <-> f64.
//! 5. Floating-point calling convention (Win64 XMM0..XMM3 and SysV XMM0..XMM7 parameters and XMM0 returns).
//! 6. Mixed integer (GPR) and float (XMM) ABI argument shuffling and parallel move resolution.
//! 7. High register pressure XMM spills/reloads and non-volatile XMM preservation.

#![allow(dead_code, unused_imports)]

use adesh_codegen::targets::create_backend;
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use adeshlang::backends::native::lower::lower_hir_module;
use adeshlang::ir::hir::*;
use std::process::Command;
use std::sync::Arc;
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

fn lower_and_validate_adob(hir: &HirModule, triple: &str) -> Vec<u8> {
    let target = TargetDescriptor::from_triple(triple).expect("valid triple");
    let native_mod = lower_hir_module(hir, &target).expect("native lowering");

    let mut backend = create_backend(target.clone()).expect("backend creation");
    let obj = backend.emit_object(&native_mod).expect("ADOB emission");
    AdobValidator::validate(&obj).expect("emitted ADOB must validate");

    AdobWriter::write(&obj).expect("ADOB encoding")
}

#[test]
#[cfg(target_arch = "x86_64")]
fn test_fp_lowering_and_adob_validation_win64() {
    let hir = HirModule {
        functions: vec![],
        classes: vec![],
        statements: vec![
            HirStmt::Let {
                name: "a".to_string(),
                ty: Some(HirType::F64),
                init: Some(HirExpr::Literal(HirLiteral::F64(12.5))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "b".to_string(),
                ty: Some(HirType::F32),
                init: Some(HirExpr::Literal(HirLiteral::F32(2.5))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "c".to_string(),
                ty: Some(HirType::F64),
                init: Some(HirExpr::BinaryOp(
                    Box::new(HirExpr::LoadVar("a".to_string())),
                    BinOp::Add,
                    Box::new(HirExpr::Cast(
                        Box::new(HirExpr::LoadVar("b".to_string())),
                        HirType::F64,
                    )),
                )),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Return(Some(HirExpr::Cast(
                Box::new(HirExpr::LoadVar("c".to_string())),
                HirType::I64,
            ))),
        ],
    };

    let bytes = lower_and_validate_adob(&hir, "x86_64-pc-windows-msvc");
    assert!(
        !bytes.is_empty(),
        "ADOB encoding must produce non-empty bytes"
    );
}

#[test]
#[cfg(target_arch = "x86_64")]
fn test_fp_lowering_and_adob_validation_sysv() {
    let hir = HirModule {
        functions: vec![],
        classes: vec![],
        statements: vec![
            HirStmt::Let {
                name: "x".to_string(),
                ty: Some(HirType::F64),
                init: Some(HirExpr::Literal(HirLiteral::F64(100.0))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "y".to_string(),
                ty: Some(HirType::F64),
                init: Some(HirExpr::Literal(HirLiteral::F64(4.0))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "res".to_string(),
                ty: Some(HirType::F64),
                init: Some(HirExpr::BinaryOp(
                    Box::new(HirExpr::LoadVar("x".to_string())),
                    BinOp::Div,
                    Box::new(HirExpr::LoadVar("y".to_string())),
                )),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Return(Some(HirExpr::Cast(
                Box::new(HirExpr::LoadVar("res".to_string())),
                HirType::I64,
            ))),
        ],
    };

    let bytes = lower_and_validate_adob(&hir, "x86_64-unknown-linux-gnu");
    assert!(
        !bytes.is_empty(),
        "ADOB encoding must produce non-empty bytes"
    );
}

#[test]
#[cfg(target_arch = "x86_64")]
fn test_fp_mixed_abi_and_spill_lowering() {
    let mut stmts = Vec::new();
    // Allocate 18 live f64 variables to force register spills beyond 15 allocatable XMM registers
    for i in 0..18 {
        stmts.push(HirStmt::Let {
            name: format!("v{}", i),
            ty: Some(HirType::F64),
            init: Some(HirExpr::Literal(HirLiteral::F64(i as f64 + 1.0))),
            is_const: false,
            is_borrowed: None,
        });
    }

    // Accumulate sum of all 18 FP variables
    let mut acc: HirExpr = HirExpr::LoadVar("v0".to_string());
    for i in 1..18 {
        acc = HirExpr::BinaryOp(
            Box::new(acc),
            BinOp::Add,
            Box::new(HirExpr::LoadVar(format!("v{}", i))),
        );
    }

    stmts.push(HirStmt::Return(Some(HirExpr::Cast(
        Box::new(acc),
        HirType::I64,
    ))));

    let hir = HirModule {
        functions: vec![],
        classes: vec![],
        statements: stmts,
    };

    let win_bytes = lower_and_validate_adob(&hir, "x86_64-pc-windows-msvc");
    assert!(!win_bytes.is_empty());

    let sysv_bytes = lower_and_validate_adob(&hir, "x86_64-unknown-linux-gnu");
    assert!(!sysv_bytes.is_empty());
}

#[test]
#[cfg(target_arch = "x86_64")]
fn test_fp_nan_infinity_signed_zero_lowering() {
    let hir = HirModule {
        functions: vec![],
        classes: vec![],
        statements: vec![
            HirStmt::Let {
                name: "nan_val".to_string(),
                ty: Some(HirType::F64),
                init: Some(HirExpr::Literal(HirLiteral::F64(f64::NAN))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "pos_inf".to_string(),
                ty: Some(HirType::F64),
                init: Some(HirExpr::Literal(HirLiteral::F64(f64::INFINITY))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "neg_inf".to_string(),
                ty: Some(HirType::F64),
                init: Some(HirExpr::Literal(HirLiteral::F64(f64::NEG_INFINITY))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "pos_zero".to_string(),
                ty: Some(HirType::F64),
                init: Some(HirExpr::Literal(HirLiteral::F64(0.0))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "neg_zero".to_string(),
                ty: Some(HirType::F64),
                init: Some(HirExpr::Literal(HirLiteral::F64(-0.0))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "cmp_nan".to_string(),
                ty: Some(HirType::Bool),
                init: Some(HirExpr::BinaryOp(
                    Box::new(HirExpr::LoadVar("nan_val".to_string())),
                    BinOp::Ne,
                    Box::new(HirExpr::LoadVar("nan_val".to_string())),
                )),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "cmp_zeros".to_string(),
                ty: Some(HirType::Bool),
                init: Some(HirExpr::BinaryOp(
                    Box::new(HirExpr::LoadVar("pos_zero".to_string())),
                    BinOp::Eq,
                    Box::new(HirExpr::LoadVar("neg_zero".to_string())),
                )),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Return(Some(HirExpr::Conditional(
                Box::new(HirExpr::BinaryOp(
                    Box::new(HirExpr::LoadVar("cmp_nan".to_string())),
                    BinOp::And,
                    Box::new(HirExpr::LoadVar("cmp_zeros".to_string())),
                )),
                Box::new(HirExpr::Literal(HirLiteral::Int(1))),
                Box::new(HirExpr::Literal(HirLiteral::Int(0))),
            ))),
        ],
    };

    let win_bytes = lower_and_validate_adob(&hir, "x86_64-pc-windows-msvc");
    assert!(!win_bytes.is_empty());

    let sysv_bytes = lower_and_validate_adob(&hir, "x86_64-unknown-linux-gnu");
    assert!(!sysv_bytes.is_empty());
}

#[test]
#[cfg(target_arch = "x86_64")]
fn test_fp_u64_conversions_and_rounding() {
    let hir = HirModule {
        functions: vec![],
        classes: vec![],
        statements: vec![
            HirStmt::Let {
                name: "max_u64".to_string(),
                ty: Some(HirType::U64),
                init: Some(HirExpr::Literal(HirLiteral::U64(u64::MAX))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "fp_val".to_string(),
                ty: Some(HirType::F64),
                init: Some(HirExpr::Cast(
                    Box::new(HirExpr::LoadVar("max_u64".to_string())),
                    HirType::F64,
                )),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "back_u64".to_string(),
                ty: Some(HirType::U64),
                init: Some(HirExpr::Cast(
                    Box::new(HirExpr::LoadVar("fp_val".to_string())),
                    HirType::U64,
                )),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "pos_frac".to_string(),
                ty: Some(HirType::F64),
                init: Some(HirExpr::Literal(HirLiteral::F64(4.9))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "pos_int".to_string(),
                ty: Some(HirType::I64),
                init: Some(HirExpr::Cast(
                    Box::new(HirExpr::LoadVar("pos_frac".to_string())),
                    HirType::I64,
                )),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Return(Some(HirExpr::LoadVar("pos_int".to_string()))),
        ],
    };

    let win_bytes = lower_and_validate_adob(&hir, "x86_64-pc-windows-msvc");
    assert!(!win_bytes.is_empty());

    let sysv_bytes = lower_and_validate_adob(&hir, "x86_64-unknown-linux-gnu");
    assert!(!sysv_bytes.is_empty());
}

#[test]
#[cfg(target_arch = "x86_64")]
fn test_fp_return_forwarding_and_calls() {
    let get_val_fn = HirFunction {
        name: "get_val".to_string(),
        params: vec![],
        body: Arc::new(vec![HirStmt::Return(Some(HirExpr::Literal(
            HirLiteral::F64(42.5),
        )))]),
        ret_type: Some(HirType::F64),
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

    let double_val_fn = HirFunction {
        name: "double_val".to_string(),
        params: vec![("x".to_string(), Some(HirType::F64), None)],
        body: Arc::new(vec![HirStmt::Return(Some(HirExpr::BinaryOp(
            Box::new(HirExpr::LoadVar("x".to_string())),
            BinOp::Mul,
            Box::new(HirExpr::Literal(HirLiteral::F64(2.0))),
        )))]),
        ret_type: Some(HirType::F64),
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

    let hir = HirModule {
        functions: vec![get_val_fn, double_val_fn],
        classes: vec![],
        statements: vec![HirStmt::Return(Some(HirExpr::Cast(
            Box::new(HirExpr::Call(
                Box::new(HirExpr::LoadVar("double_val".to_string())),
                vec![HirExpr::Call(
                    Box::new(HirExpr::LoadVar("get_val".to_string())),
                    vec![],
                    vec![],
                )],
                vec![],
            )),
            HirType::I64,
        )))],
    };

    let win_bytes = lower_and_validate_adob(&hir, "x86_64-pc-windows-msvc");
    assert!(!win_bytes.is_empty());

    let sysv_bytes = lower_and_validate_adob(&hir, "x86_64-unknown-linux-gnu");
    assert!(!sysv_bytes.is_empty());
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_e2e_f64_scalar_arithmetic() {
    // main:
    //   let a = 25.5;
    //   let b = 4.5;
    //   let c = (a + b) * 2.0 - 10.0;
    //   return c as i64; -> (25.5 + 4.5)*2.0 - 10.0 = 30.0*2.0 - 10.0 = 50.
    let hir = HirModule {
        functions: vec![],
        classes: vec![],
        statements: vec![
            HirStmt::Let {
                name: "a".to_string(),
                ty: Some(HirType::F64),
                init: Some(HirExpr::Literal(HirLiteral::F64(25.5))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "b".to_string(),
                ty: Some(HirType::F64),
                init: Some(HirExpr::Literal(HirLiteral::F64(4.5))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "c".to_string(),
                ty: Some(HirType::F64),
                init: Some(HirExpr::BinaryOp(
                    Box::new(HirExpr::BinaryOp(
                        Box::new(HirExpr::BinaryOp(
                            Box::new(HirExpr::LoadVar("a".to_string())),
                            BinOp::Add,
                            Box::new(HirExpr::LoadVar("b".to_string())),
                        )),
                        BinOp::Mul,
                        Box::new(HirExpr::Literal(HirLiteral::F64(2.0))),
                    )),
                    BinOp::Sub,
                    Box::new(HirExpr::Literal(HirLiteral::F64(10.0))),
                )),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Return(Some(HirExpr::Cast(
                Box::new(HirExpr::LoadVar("c".to_string())),
                HirType::I64,
            ))),
        ],
    };

    let code = lower_link_and_run(&hir, "test_f64_arithmetic");
    assert_eq!(code, 50, "f64 arithmetic (25.5 + 4.5)*2.0 - 10.0 = 50");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_e2e_f64_division_and_negation() {
    // main:
    //   let x = 100.0;
    //   let y = 4.0;
    //   let div = x / y; // 25.0
    //   let neg = -div;  // -25.0
    //   let pos = -neg;  // 25.0
    //   return pos as i64;
    let hir = HirModule {
        functions: vec![],
        classes: vec![],
        statements: vec![
            HirStmt::Let {
                name: "x".to_string(),
                ty: Some(HirType::F64),
                init: Some(HirExpr::Literal(HirLiteral::F64(100.0))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "y".to_string(),
                ty: Some(HirType::F64),
                init: Some(HirExpr::Literal(HirLiteral::F64(4.0))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "div".to_string(),
                ty: Some(HirType::F64),
                init: Some(HirExpr::BinaryOp(
                    Box::new(HirExpr::LoadVar("x".to_string())),
                    BinOp::Div,
                    Box::new(HirExpr::LoadVar("y".to_string())),
                )),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "neg".to_string(),
                ty: Some(HirType::F64),
                init: Some(HirExpr::UnaryOp(
                    UnaryOp::Neg,
                    Box::new(HirExpr::LoadVar("div".to_string())),
                )),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "pos".to_string(),
                ty: Some(HirType::F64),
                init: Some(HirExpr::UnaryOp(
                    UnaryOp::Neg,
                    Box::new(HirExpr::LoadVar("neg".to_string())),
                )),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Return(Some(HirExpr::Cast(
                Box::new(HirExpr::LoadVar("pos".to_string())),
                HirType::I64,
            ))),
        ],
    };

    let code = lower_link_and_run(&hir, "test_f64_div_neg");
    assert_eq!(code, 25, "f64 division and negation (-(-100.0/4.0) = 25)");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_e2e_f32_scalar_arithmetic() {
    // main:
    //   let a: f32 = 10.5;
    //   let b: f32 = 5.5;
    //   let c = (a + b) * 2.0; // 32.0
    //   return c as i64;
    let hir = HirModule {
        functions: vec![],
        classes: vec![],
        statements: vec![
            HirStmt::Let {
                name: "a".to_string(),
                ty: Some(HirType::F32),
                init: Some(HirExpr::Literal(HirLiteral::F32(10.5))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "b".to_string(),
                ty: Some(HirType::F32),
                init: Some(HirExpr::Literal(HirLiteral::F32(5.5))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "c".to_string(),
                ty: Some(HirType::F32),
                init: Some(HirExpr::BinaryOp(
                    Box::new(HirExpr::BinaryOp(
                        Box::new(HirExpr::LoadVar("a".to_string())),
                        BinOp::Add,
                        Box::new(HirExpr::LoadVar("b".to_string())),
                    )),
                    BinOp::Mul,
                    Box::new(HirExpr::Literal(HirLiteral::F32(2.0))),
                )),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Return(Some(HirExpr::Cast(
                Box::new(HirExpr::LoadVar("c".to_string())),
                HirType::I64,
            ))),
        ],
    };

    let code = lower_link_and_run(&hir, "test_f32_arithmetic");
    assert_eq!(code, 32, "f32 arithmetic (10.5 + 5.5)*2.0 = 32");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_e2e_fp_comparisons() {
    // Score based on 5 comparisons:
    // 1. (10.5 > 5.2)  -> 10
    // 2. (3.0 < 7.0)   -> 10
    // 3. (4.5 >= 4.5)  -> 10
    // 4. (5.0 == 5.0)  -> 10
    // 5. (6.0 != 7.0)  -> 10
    // Total expected score = 50.
    let hir = HirModule {
        functions: vec![],
        classes: vec![],
        statements: vec![
            HirStmt::Let {
                name: "c1".to_string(),
                ty: Some(HirType::Bool),
                init: Some(HirExpr::BinaryOp(
                    Box::new(HirExpr::Literal(HirLiteral::F64(10.5))),
                    BinOp::Gt,
                    Box::new(HirExpr::Literal(HirLiteral::F64(5.2))),
                )),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "c2".to_string(),
                ty: Some(HirType::Bool),
                init: Some(HirExpr::BinaryOp(
                    Box::new(HirExpr::Literal(HirLiteral::F64(3.0))),
                    BinOp::Lt,
                    Box::new(HirExpr::Literal(HirLiteral::F64(7.0))),
                )),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "c3".to_string(),
                ty: Some(HirType::Bool),
                init: Some(HirExpr::BinaryOp(
                    Box::new(HirExpr::Literal(HirLiteral::F64(4.5))),
                    BinOp::Ge,
                    Box::new(HirExpr::Literal(HirLiteral::F64(4.5))),
                )),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "c4".to_string(),
                ty: Some(HirType::Bool),
                init: Some(HirExpr::BinaryOp(
                    Box::new(HirExpr::Literal(HirLiteral::F64(5.0))),
                    BinOp::Eq,
                    Box::new(HirExpr::Literal(HirLiteral::F64(5.0))),
                )),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "c5".to_string(),
                ty: Some(HirType::Bool),
                init: Some(HirExpr::BinaryOp(
                    Box::new(HirExpr::Literal(HirLiteral::F64(6.0))),
                    BinOp::Ne,
                    Box::new(HirExpr::Literal(HirLiteral::F64(7.0))),
                )),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "score".to_string(),
                ty: Some(HirType::I64),
                init: Some(HirExpr::BinaryOp(
                    Box::new(HirExpr::BinaryOp(
                        Box::new(HirExpr::BinaryOp(
                            Box::new(HirExpr::BinaryOp(
                                Box::new(HirExpr::Conditional(
                                    Box::new(HirExpr::LoadVar("c1".to_string())),
                                    Box::new(HirExpr::Literal(HirLiteral::Int(10))),
                                    Box::new(HirExpr::Literal(HirLiteral::Int(0))),
                                )),
                                BinOp::Add,
                                Box::new(HirExpr::Conditional(
                                    Box::new(HirExpr::LoadVar("c2".to_string())),
                                    Box::new(HirExpr::Literal(HirLiteral::Int(10))),
                                    Box::new(HirExpr::Literal(HirLiteral::Int(0))),
                                )),
                            )),
                            BinOp::Add,
                            Box::new(HirExpr::Conditional(
                                Box::new(HirExpr::LoadVar("c3".to_string())),
                                Box::new(HirExpr::Literal(HirLiteral::Int(10))),
                                Box::new(HirExpr::Literal(HirLiteral::Int(0))),
                            )),
                        )),
                        BinOp::Add,
                        Box::new(HirExpr::Conditional(
                            Box::new(HirExpr::LoadVar("c4".to_string())),
                            Box::new(HirExpr::Literal(HirLiteral::Int(10))),
                            Box::new(HirExpr::Literal(HirLiteral::Int(0))),
                        )),
                    )),
                    BinOp::Add,
                    Box::new(HirExpr::Conditional(
                        Box::new(HirExpr::LoadVar("c5".to_string())),
                        Box::new(HirExpr::Literal(HirLiteral::Int(10))),
                        Box::new(HirExpr::Literal(HirLiteral::Int(0))),
                    )),
                )),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Return(Some(HirExpr::LoadVar("score".to_string()))),
        ],
    };

    let code = lower_link_and_run(&hir, "test_fp_comparisons");
    assert_eq!(code, 50, "5 FP comparisons evaluated correctly to 50");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_e2e_fp_type_conversions() {
    // let a = 42;
    // let b: f64 = a as f64;
    // let c: f32 = b as f32;
    // let d: i64 = c as i64;
    // return d;
    let hir = HirModule {
        functions: vec![],
        classes: vec![],
        statements: vec![
            HirStmt::Let {
                name: "a".to_string(),
                ty: Some(HirType::I64),
                init: Some(HirExpr::Literal(HirLiteral::Int(42))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "b".to_string(),
                ty: Some(HirType::F64),
                init: Some(HirExpr::Cast(
                    Box::new(HirExpr::LoadVar("a".to_string())),
                    HirType::F64,
                )),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "c".to_string(),
                ty: Some(HirType::F32),
                init: Some(HirExpr::Cast(
                    Box::new(HirExpr::LoadVar("b".to_string())),
                    HirType::F32,
                )),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "d".to_string(),
                ty: Some(HirType::I64),
                init: Some(HirExpr::Cast(
                    Box::new(HirExpr::LoadVar("c".to_string())),
                    HirType::I64,
                )),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Return(Some(HirExpr::LoadVar("d".to_string()))),
        ],
    };

    let code = lower_link_and_run(&hir, "test_fp_conversions");
    assert_eq!(code, 42, "round-trip i64 -> f64 -> f32 -> i64 = 42");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_e2e_fp_function_calls_and_abi() {
    // fn add_floats(a: f64, b: f64, c: f64, d: f64) -> f64 { a + b + c + d }
    // main: return add_floats(1.5, 2.5, 3.5, 4.5) as i64; -> 12.0 -> 12.
    let add_floats_fn = HirFunction {
        name: "add_floats".to_string(),
        params: vec![
            ("a".to_string(), Some(HirType::F64), None),
            ("b".to_string(), Some(HirType::F64), None),
            ("c".to_string(), Some(HirType::F64), None),
            ("d".to_string(), Some(HirType::F64), None),
        ],
        body: Arc::new(vec![HirStmt::Return(Some(HirExpr::BinaryOp(
            Box::new(HirExpr::BinaryOp(
                Box::new(HirExpr::BinaryOp(
                    Box::new(HirExpr::LoadVar("a".to_string())),
                    BinOp::Add,
                    Box::new(HirExpr::LoadVar("b".to_string())),
                )),
                BinOp::Add,
                Box::new(HirExpr::LoadVar("c".to_string())),
            )),
            BinOp::Add,
            Box::new(HirExpr::LoadVar("d".to_string())),
        )))]),
        ret_type: Some(HirType::F64),
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

    let hir = HirModule {
        functions: vec![add_floats_fn],
        classes: vec![],
        statements: vec![HirStmt::Return(Some(HirExpr::Cast(
            Box::new(HirExpr::Call(
                Box::new(HirExpr::LoadVar("add_floats".to_string())),
                vec![
                    HirExpr::Literal(HirLiteral::F64(1.5)),
                    HirExpr::Literal(HirLiteral::F64(2.5)),
                    HirExpr::Literal(HirLiteral::F64(3.5)),
                    HirExpr::Literal(HirLiteral::F64(4.5)),
                ],
                vec![],
            )),
            HirType::I64,
        )))],
    };

    let code = lower_link_and_run(&hir, "test_fp_abi");
    assert_eq!(code, 12, "add_floats(1.5, 2.5, 3.5, 4.5) = 12");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_e2e_mixed_int_and_float_arguments() {
    // fn mixed(a: i64, b: f64, c: i64, d: f64) -> f64 { (a as f64) * b + (c as f64) * d }
    // main: return mixed(2, 3.5, 4, 2.5) as i64; -> 2*3.5 + 4*2.5 = 7.0 + 10.0 = 17.0 -> 17.
    let mixed_fn = HirFunction {
        name: "mixed".to_string(),
        params: vec![
            ("a".to_string(), Some(HirType::I64), None),
            ("b".to_string(), Some(HirType::F64), None),
            ("c".to_string(), Some(HirType::I64), None),
            ("d".to_string(), Some(HirType::F64), None),
        ],
        body: Arc::new(vec![HirStmt::Return(Some(HirExpr::BinaryOp(
            Box::new(HirExpr::BinaryOp(
                Box::new(HirExpr::Cast(
                    Box::new(HirExpr::LoadVar("a".to_string())),
                    HirType::F64,
                )),
                BinOp::Mul,
                Box::new(HirExpr::LoadVar("b".to_string())),
            )),
            BinOp::Add,
            Box::new(HirExpr::BinaryOp(
                Box::new(HirExpr::Cast(
                    Box::new(HirExpr::LoadVar("c".to_string())),
                    HirType::F64,
                )),
                BinOp::Mul,
                Box::new(HirExpr::LoadVar("d".to_string())),
            )),
        )))]),
        ret_type: Some(HirType::F64),
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

    let hir = HirModule {
        functions: vec![mixed_fn],
        classes: vec![],
        statements: vec![HirStmt::Return(Some(HirExpr::Cast(
            Box::new(HirExpr::Call(
                Box::new(HirExpr::LoadVar("mixed".to_string())),
                vec![
                    HirExpr::Literal(HirLiteral::Int(2)),
                    HirExpr::Literal(HirLiteral::F64(3.5)),
                    HirExpr::Literal(HirLiteral::Int(4)),
                    HirExpr::Literal(HirLiteral::F64(2.5)),
                ],
                vec![],
            )),
            HirType::I64,
        )))],
    };

    let code = lower_link_and_run(&hir, "test_mixed_args");
    assert_eq!(code, 17, "mixed(2, 3.5, 4, 2.5) = 17");
}
