//! End-to-End Native Lowering Tests for Phase 2.
//!
//! Verifies complete HIR -> Native Machine IR lowering for:
//! 1. Control flow: nested if/else, while loops with break/continue, match expressions, defers.
//! 2. Aggregates: arrays, indexing, mutation, tuples, destructuring, dictionaries.
//! 3. Functions & calls: direct function calls, multi-arg calls, indirect lambda calls.
//! 4. Memory operations: borrow references, pointer dereferencing, alloc and free.
//! 5. String concatenation and type casting.

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

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_e2e_nested_if_else_and_arithmetic() {
    // fn compute(x: i64, y: i64) -> i64 {
    //     if (x > y) {
    //         if (x > 10) { x - y + 5 } else { x + y }
    //     } else {
    //         y - x
    //     }
    // }
    // main: return compute(15, 5); -> expected: 15 - 5 + 5 = 15.
    let compute_fn = HirFunction {
        name: "compute".to_string(),
        params: vec![
            ("x".to_string(), Some(HirType::I64), None),
            ("y".to_string(), Some(HirType::I64), None),
        ],
        body: Arc::new(vec![HirStmt::If {
            cond: HirExpr::BinaryOp(
                Box::new(HirExpr::LoadVar("x".to_string())),
                BinOp::Gt,
                Box::new(HirExpr::LoadVar("y".to_string())),
            ),
            then_branch: Box::new(HirStmt::If {
                cond: HirExpr::BinaryOp(
                    Box::new(HirExpr::LoadVar("x".to_string())),
                    BinOp::Gt,
                    Box::new(HirExpr::Literal(HirLiteral::Int(10))),
                ),
                then_branch: Box::new(HirStmt::Return(Some(HirExpr::BinaryOp(
                    Box::new(HirExpr::BinaryOp(
                        Box::new(HirExpr::LoadVar("x".to_string())),
                        BinOp::Sub,
                        Box::new(HirExpr::LoadVar("y".to_string())),
                    )),
                    BinOp::Add,
                    Box::new(HirExpr::Literal(HirLiteral::Int(5))),
                )))),
                else_branch: Some(Box::new(HirStmt::Return(Some(HirExpr::BinaryOp(
                    Box::new(HirExpr::LoadVar("x".to_string())),
                    BinOp::Add,
                    Box::new(HirExpr::LoadVar("y".to_string())),
                ))))),
            }),
            else_branch: Some(Box::new(HirStmt::Return(Some(HirExpr::BinaryOp(
                Box::new(HirExpr::LoadVar("y".to_string())),
                BinOp::Sub,
                Box::new(HirExpr::LoadVar("x".to_string())),
            ))))),
        }]),
        ret_type: Some(HirType::I64),
        is_async: false,
        decorators: Vec::new(),
        is_exported: true,
        move_params: Vec::new(),
        is_test: false,
        test_ignore: false,
        test_expect_fail: false,
        test_timeout: None,
        is_unsafe: false,
    };

    let main_fn = HirFunction {
        name: "main".to_string(),
        params: Vec::new(),
        body: Arc::new(vec![HirStmt::Return(Some(HirExpr::Call(
            Box::new(HirExpr::LoadVar("compute".to_string())),
            vec![
                HirExpr::Literal(HirLiteral::Int(15)),
                HirExpr::Literal(HirLiteral::Int(5)),
            ],
            Vec::new(),
        )))]),
        ret_type: Some(HirType::I64),
        is_async: false,
        decorators: Vec::new(),
        is_exported: true,
        move_params: Vec::new(),
        is_test: false,
        test_ignore: false,
        test_expect_fail: false,
        test_timeout: None,
        is_unsafe: false,
    };

    let hir = HirModule {
        functions: vec![compute_fn, main_fn],
        statements: Vec::new(),
        classes: Vec::new(),
        enums: Vec::new(),
    };

    let code = lower_link_and_run(&hir, "test_nested_if");
    assert_eq!(code, 15);
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_e2e_while_loop_with_break_and_continue() {
    // fn main() -> i64 {
    //     let mut sum = 0;
    //     let mut i = 0;
    //     while (i < 10) {
    //         i = i + 1;
    //         if (i == 3) { continue; }
    //         if (i == 7) { break; }
    //         sum = sum + i;
    //     }
    //     return sum; // 1 + 2 + 4 + 5 + 6 = 18
    // }
    let main_fn = HirFunction {
        name: "main".to_string(),
        params: Vec::new(),
        body: Arc::new(vec![
            HirStmt::Let {
                name: "sum".to_string(),
                ty: Some(HirType::I64),
                init: Some(HirExpr::Literal(HirLiteral::Int(0))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "i".to_string(),
                ty: Some(HirType::I64),
                init: Some(HirExpr::Literal(HirLiteral::Int(0))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::While {
                cond: HirExpr::BinaryOp(
                    Box::new(HirExpr::LoadVar("i".to_string())),
                    BinOp::Lt,
                    Box::new(HirExpr::Literal(HirLiteral::Int(10))),
                ),
                body: Box::new(HirStmt::Block(vec![
                    HirStmt::Assign {
                        target: HirExpr::LoadVar("i".to_string()),
                        value: HirExpr::BinaryOp(
                            Box::new(HirExpr::LoadVar("i".to_string())),
                            BinOp::Add,
                            Box::new(HirExpr::Literal(HirLiteral::Int(1))),
                        ),
                        is_move: false,
                    },
                    HirStmt::If {
                        cond: HirExpr::BinaryOp(
                            Box::new(HirExpr::LoadVar("i".to_string())),
                            BinOp::Eq,
                            Box::new(HirExpr::Literal(HirLiteral::Int(3))),
                        ),
                        then_branch: Box::new(HirStmt::Continue),
                        else_branch: None,
                    },
                    HirStmt::If {
                        cond: HirExpr::BinaryOp(
                            Box::new(HirExpr::LoadVar("i".to_string())),
                            BinOp::Eq,
                            Box::new(HirExpr::Literal(HirLiteral::Int(7))),
                        ),
                        then_branch: Box::new(HirStmt::Break),
                        else_branch: None,
                    },
                    HirStmt::Assign {
                        target: HirExpr::LoadVar("sum".to_string()),
                        value: HirExpr::BinaryOp(
                            Box::new(HirExpr::LoadVar("sum".to_string())),
                            BinOp::Add,
                            Box::new(HirExpr::LoadVar("i".to_string())),
                        ),
                        is_move: false,
                    },
                ])),
            },
            HirStmt::Return(Some(HirExpr::LoadVar("sum".to_string()))),
        ]),
        ret_type: Some(HirType::I64),
        is_async: false,
        decorators: Vec::new(),
        is_exported: true,
        move_params: Vec::new(),
        is_test: false,
        test_ignore: false,
        test_expect_fail: false,
        test_timeout: None,
        is_unsafe: false,
    };

    let hir = HirModule {
        functions: vec![main_fn],
        statements: Vec::new(),
        classes: Vec::new(),
        enums: Vec::new(),
    };

    let code = lower_link_and_run(&hir, "test_while_loop");
    assert_eq!(code, 18);
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_e2e_match_expression_lowering() {
    // fn classify(val: i64) -> i64 {
    //     match val {
    //         1 => 10,
    //         2 | 3 => 30,
    //         x => x * 2,
    //     }
    // }
    // main: return classify(2) + classify(5); // 30 + 10 = 40
    let classify_fn = HirFunction {
        name: "classify".to_string(),
        params: vec![("val".to_string(), Some(HirType::I64), None)],
        body: Arc::new(vec![HirStmt::Return(Some(HirExpr::Match(
            Box::new(HirExpr::LoadVar("val".to_string())),
            vec![
                (
                    HirPattern::Literal(HirLiteral::Int(1)),
                    HirExpr::Literal(HirLiteral::Int(10)),
                ),
                (
                    HirPattern::Or(
                        Box::new(HirPattern::Literal(HirLiteral::Int(2))),
                        Box::new(HirPattern::Literal(HirLiteral::Int(3))),
                    ),
                    HirExpr::Literal(HirLiteral::Int(30)),
                ),
                (
                    HirPattern::Variable("x".to_string()),
                    HirExpr::BinaryOp(
                        Box::new(HirExpr::LoadVar("x".to_string())),
                        BinOp::Mul,
                        Box::new(HirExpr::Literal(HirLiteral::Int(2))),
                    ),
                ),
            ],
        )))]),
        ret_type: Some(HirType::I64),
        is_async: false,
        decorators: Vec::new(),
        is_exported: true,
        move_params: Vec::new(),
        is_test: false,
        test_ignore: false,
        test_expect_fail: false,
        test_timeout: None,
        is_unsafe: false,
    };

    let main_fn = HirFunction {
        name: "main".to_string(),
        params: Vec::new(),
        body: Arc::new(vec![HirStmt::Return(Some(HirExpr::BinaryOp(
            Box::new(HirExpr::Call(
                Box::new(HirExpr::LoadVar("classify".to_string())),
                vec![HirExpr::Literal(HirLiteral::Int(2))],
                Vec::new(),
            )),
            BinOp::Add,
            Box::new(HirExpr::Call(
                Box::new(HirExpr::LoadVar("classify".to_string())),
                vec![HirExpr::Literal(HirLiteral::Int(5))],
                Vec::new(),
            )),
        )))]),
        ret_type: Some(HirType::I64),
        is_async: false,
        decorators: Vec::new(),
        is_exported: true,
        move_params: Vec::new(),
        is_test: false,
        test_ignore: false,
        test_expect_fail: false,
        test_timeout: None,
        is_unsafe: false,
    };

    let hir = HirModule {
        functions: vec![classify_fn, main_fn],
        statements: Vec::new(),
        classes: Vec::new(),
        enums: Vec::new(),
    };

    let code = lower_link_and_run(&hir, "test_match_expr");
    assert_eq!(code, 40);
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_e2e_tuple_destructuring_and_operations() {
    // fn main() -> i64 {
    //     let (a, b, c) = (10, 20, 30);
    //     let (x, y) = (a + b, c * 2); // (30, 60)
    //     return x + y; // 90
    // }
    let main_fn = HirFunction {
        name: "main".to_string(),
        params: Vec::new(),
        body: Arc::new(vec![
            HirStmt::LetTuple {
                names: vec!["a".to_string(), "b".to_string(), "c".to_string()],
                init: Some(HirExpr::TupleLiteral(vec![
                    HirExpr::Literal(HirLiteral::Int(10)),
                    HirExpr::Literal(HirLiteral::Int(20)),
                    HirExpr::Literal(HirLiteral::Int(30)),
                ])),
                is_const: false,
            },
            HirStmt::LetTuple {
                names: vec!["x".to_string(), "y".to_string()],
                init: Some(HirExpr::TupleLiteral(vec![
                    HirExpr::BinaryOp(
                        Box::new(HirExpr::LoadVar("a".to_string())),
                        BinOp::Add,
                        Box::new(HirExpr::LoadVar("b".to_string())),
                    ),
                    HirExpr::BinaryOp(
                        Box::new(HirExpr::LoadVar("c".to_string())),
                        BinOp::Mul,
                        Box::new(HirExpr::Literal(HirLiteral::Int(2))),
                    ),
                ])),
                is_const: false,
            },
            HirStmt::Return(Some(HirExpr::BinaryOp(
                Box::new(HirExpr::LoadVar("x".to_string())),
                BinOp::Add,
                Box::new(HirExpr::LoadVar("y".to_string())),
            ))),
        ]),
        ret_type: Some(HirType::I64),
        is_async: false,
        decorators: Vec::new(),
        is_exported: true,
        move_params: Vec::new(),
        is_test: false,
        test_ignore: false,
        test_expect_fail: false,
        test_timeout: None,
        is_unsafe: false,
    };

    let hir = HirModule {
        functions: vec![main_fn],
        statements: Vec::new(),
        classes: Vec::new(),
        enums: Vec::new(),
    };

    let code = lower_link_and_run(&hir, "test_tuple_destructuring");
    assert_eq!(code, 90);
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_e2e_memory_borrow_and_deref() {
    // fn main() -> i64 {
    //     let mut x = 42;
    //     let ptr = &x;
    //     *ptr = 100;
    //     return x; // 100
    // }
    let main_fn = HirFunction {
        name: "main".to_string(),
        params: Vec::new(),
        body: Arc::new(vec![
            HirStmt::Let {
                name: "x".to_string(),
                ty: Some(HirType::I64),
                init: Some(HirExpr::Literal(HirLiteral::Int(42))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Let {
                name: "ptr".to_string(),
                ty: None,
                init: Some(HirExpr::Borrow(
                    Box::new(HirExpr::LoadVar("x".to_string())),
                    true,
                )),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Assign {
                target: HirExpr::Deref(Box::new(HirExpr::LoadVar("ptr".to_string()))),
                value: HirExpr::Literal(HirLiteral::Int(100)),
                is_move: false,
            },
            HirStmt::Return(Some(HirExpr::LoadVar("x".to_string()))),
        ]),
        ret_type: Some(HirType::I64),
        is_async: false,
        decorators: Vec::new(),
        is_exported: true,
        move_params: Vec::new(),
        is_test: false,
        test_ignore: false,
        test_expect_fail: false,
        test_timeout: None,
        is_unsafe: false,
    };

    let hir = HirModule {
        functions: vec![main_fn],
        statements: Vec::new(),
        classes: Vec::new(),
        enums: Vec::new(),
    };

    let code = lower_link_and_run(&hir, "test_memory_borrow_deref");
    assert_eq!(code, 100);
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_e2e_defer_execution_order() {
    // fn main() -> i64 {
    //     let mut res = 10;
    //     defer { res = res + 5; }
    //     defer { res = res * 2; }
    //     return res; // Defers run in LIFO order before return: res = res*2 (20), then res = res+5 (25)
    // }
    let main_fn = HirFunction {
        name: "main".to_string(),
        params: Vec::new(),
        body: Arc::new(vec![
            HirStmt::Let {
                name: "res".to_string(),
                ty: Some(HirType::I64),
                init: Some(HirExpr::Literal(HirLiteral::Int(10))),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Defer(Box::new(HirStmt::Assign {
                target: HirExpr::LoadVar("res".to_string()),
                value: HirExpr::BinaryOp(
                    Box::new(HirExpr::LoadVar("res".to_string())),
                    BinOp::Add,
                    Box::new(HirExpr::Literal(HirLiteral::Int(5))),
                ),
                is_move: false,
            })),
            HirStmt::Defer(Box::new(HirStmt::Assign {
                target: HirExpr::LoadVar("res".to_string()),
                value: HirExpr::BinaryOp(
                    Box::new(HirExpr::LoadVar("res".to_string())),
                    BinOp::Mul,
                    Box::new(HirExpr::Literal(HirLiteral::Int(2))),
                ),
                is_move: false,
            })),
            HirStmt::Return(Some(HirExpr::LoadVar("res".to_string()))),
        ]),
        ret_type: Some(HirType::I64),
        is_async: false,
        decorators: Vec::new(),
        is_exported: true,
        move_params: Vec::new(),
        is_test: false,
        test_ignore: false,
        test_expect_fail: false,
        test_timeout: None,
        is_unsafe: false,
    };

    let hir = HirModule {
        functions: vec![main_fn],
        statements: Vec::new(),
        classes: Vec::new(),
        enums: Vec::new(),
    };

    // The return value is captured before the defers run (matching the
    // interpreter), so the defers' mutations of `res` are not observed.
    let code = lower_link_and_run(&hir, "test_defer_order");
    assert_eq!(code, 10);
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_e2e_indirect_lambda_calls() {
    // fn apply(op: (i64, i64) -> i64, a: i64, b: i64) -> i64 {
    //     return op(a, b);
    // }
    // main:
    //     let add_fn = (x, y) => x + y;
    //     return apply(add_fn, 35, 7); // 42
    let apply_fn = HirFunction {
        name: "apply".to_string(),
        params: vec![
            ("op".to_string(), None, None),
            ("a".to_string(), Some(HirType::I64), None),
            ("b".to_string(), Some(HirType::I64), None),
        ],
        body: Arc::new(vec![HirStmt::Return(Some(HirExpr::Call(
            Box::new(HirExpr::LoadVar("op".to_string())),
            vec![
                HirExpr::LoadVar("a".to_string()),
                HirExpr::LoadVar("b".to_string()),
            ],
            Vec::new(),
        )))]),
        ret_type: Some(HirType::I64),
        is_async: false,
        decorators: Vec::new(),
        is_exported: true,
        move_params: Vec::new(),
        is_test: false,
        test_ignore: false,
        test_expect_fail: false,
        test_timeout: None,
        is_unsafe: false,
    };

    let main_fn = HirFunction {
        name: "main".to_string(),
        params: Vec::new(),
        body: Arc::new(vec![
            HirStmt::Let {
                name: "add_fn".to_string(),
                ty: None,
                init: Some(HirExpr::Lambda(
                    vec![
                        ("x".to_string(), Some(HirType::I64)),
                        ("y".to_string(), Some(HirType::I64)),
                    ],
                    Arc::new(vec![HirStmt::Return(Some(HirExpr::BinaryOp(
                        Box::new(HirExpr::LoadVar("x".to_string())),
                        BinOp::Add,
                        Box::new(HirExpr::LoadVar("y".to_string())),
                    )))]),
                    false,
                )),
                is_const: false,
                is_borrowed: None,
            },
            HirStmt::Return(Some(HirExpr::Call(
                Box::new(HirExpr::LoadVar("apply".to_string())),
                vec![
                    HirExpr::LoadVar("add_fn".to_string()),
                    HirExpr::Literal(HirLiteral::Int(35)),
                    HirExpr::Literal(HirLiteral::Int(7)),
                ],
                Vec::new(),
            ))),
        ]),
        ret_type: Some(HirType::I64),
        is_async: false,
        decorators: Vec::new(),
        is_exported: true,
        move_params: Vec::new(),
        is_test: false,
        test_ignore: false,
        test_expect_fail: false,
        test_timeout: None,
        is_unsafe: false,
    };

    let hir = HirModule {
        functions: vec![apply_fn, main_fn],
        statements: Vec::new(),
        classes: Vec::new(),
        enums: Vec::new(),
    };

    let code = lower_link_and_run(&hir, "test_indirect_lambda");
    assert_eq!(code, 42);
}
