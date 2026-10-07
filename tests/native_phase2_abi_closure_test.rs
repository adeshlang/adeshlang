//! Phase 2 Task P2-9: Closures & Captured Environments Tests.
//!
//! Verifies:
//! 1. Single variable capture across function boundary (heap-allocated indefinite extent).
//! 2. Multiple captured variables in closure environment tuple.
//! 3. Stateful mutating closures (environment mutation across multiple calls).
//! 4. Multi-level nested closures (closure capturing from enclosing closure).
//! 5. Passing functions and closures as higher-order arguments to indirect call sites.
//! 6. Real execution of linked PE binaries on x86-64 Windows asserting exact exit codes.

#![allow(dead_code, unused_imports)]

use adesh_codegen::create_backend;
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use adeshlang::backends::native::lower_hir_module;
use adeshlang::parsing::hir_lower::ast_to_hir;
use adeshlang::parsing::lexer::Lexer;
use adeshlang::parsing::parser::Parser;
use std::process::Command;
use tempfile::tempdir;

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn compile_and_run(src: &str, test_name: &str) -> (String, i32) {
    let tokens = Lexer::new(src).tokenize().expect("tokenize");
    let mut parser = Parser::new(tokens, None);
    let ast = parser.parse_program().expect("parse");
    let hir = ast_to_hir(&ast, true).expect("ast_to_hir");

    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let native_mod = lower_hir_module(&hir, &target).expect("native lowering");

    let mut backend = create_backend(target.clone()).expect("backend creation");
    let obj = backend.emit_object(&native_mod).expect("ADOB emission");
    AdobValidator::validate(&obj).expect("emitted ADOB must validate");

    let bytes = AdobWriter::write(&obj).expect("ADOB encoding");
    let dir = tempdir().expect("tempdir");
    let adob_path = dir.path().join(format!("{test_name}.adob"));
    std::fs::write(&adob_path, bytes).expect("write ADOB file");

    let exe_path = dir.path().join(format!("{test_name}.exe"));
    adesh_linker::link(&[&adob_path], &exe_path, Some("x86_64-pc-windows-msvc"))
        .expect("native link");
    assert!(exe_path.exists(), "linked executable must exist");

    let out = Command::new(&exe_path)
        .output()
        .expect("execute native binary");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    (stdout, code)
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_closure_single_capture_e2e() {
    let src = r#"
    fn make_adder(x: int) {
        return fn(y: int): int {
            return x + y;
        };
    }

    fn main(): int {
        let add15 = make_adder(15);
        return add15(30); // 45
    }
    "#;
    let (_, code) = compile_and_run(src, "test_closure_single_capture");
    assert_eq!(code, 45, "make_adder(15)(30) should return 45");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_closure_multiple_captures_e2e() {
    let src = r#"
    fn make_linear(slope: int, intercept: int) {
        return fn(x: int): int {
            return slope * x + intercept;
        };
    }

    fn main(): int {
        let f = make_linear(4, 7);
        return f(10); // 4 * 10 + 7 = 47
    }
    "#;
    let (_, code) = compile_and_run(src, "test_closure_multiple_captures");
    assert_eq!(code, 47, "make_linear(4, 7)(10) should return 47");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_closure_state_mutation_counter_e2e() {
    let src = r#"
    fn make_counter(start: int) {
        let c = start;
        return fn(): int {
            c = c + 1;
            return c;
        };
    }

    fn main(): int {
        let counter = make_counter(20);
        let a = counter(); // 21
        let b = counter(); // 22
        let c = counter(); // 23
        return a + b + c; // 21 + 22 + 23 = 66
    }
    "#;
    let (_, code) = compile_and_run(src, "test_closure_mutation_counter");
    assert_eq!(code, 66, "Sum of three increments should equal 66");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_closure_nested_currying_e2e() {
    let src = r#"
    fn curried_add(a: int) {
        return fn(b: int) {
            return fn(c: int): int {
                return a + b + c;
            };
        };
    }

    fn main(): int {
        let step1 = curried_add(10);
        let step2 = step1(20);
        return step2(35); // 10 + 20 + 35 = 65
    }
    "#;
    let (_, code) = compile_and_run(src, "test_closure_nested_currying");
    assert_eq!(
        code, 65,
        "Curried additions across 3 closures should return 65"
    );
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_closure_higher_order_dispatch_e2e() {
    let src = r#"
    fn apply_binary(op, x: int, y: int): int {
        return op(x, y);
    }

    fn main(): int {
        let scale = 3;
        let scaled_sum = fn(a: int, b: int): int {
            return (a + b) * scale;
        };
        return apply_binary(scaled_sum, 4, 6); // (4 + 6) * 3 = 30
    }
    "#;
    let (_, code) = compile_and_run(src, "test_closure_higher_order_dispatch");
    assert_eq!(code, 30, "Higher-order closure dispatch should return 30");
}
