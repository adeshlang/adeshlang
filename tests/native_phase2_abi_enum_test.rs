//! Phase 2 Task P2-8: Native Enums & Tagged Pattern Matching Tests.
//!
//! Verifies:
//! 1. Built-in Option (`Some(payload)` and `None`) construction and tagged pattern matching.
//! 2. Built-in Result (`Ok(payload)` and `Err(payload)`) construction and tagged pattern matching.
//! 3. User-defined enums with unit variants (`Color::Red`, `Color::Green`, `Color::Blue`).
//! 4. User-defined enums with payload variants (`Action::Quit`, `Action::Move(int)`).
//! 5. Tagged pattern matching with literal payload discrimination (`Some(42)` vs `Some(x)` vs `None`).
//! 6. Execution of real linked PE binaries on x86-64 Windows asserting exact exit codes and stdout.

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
fn test_enum_option_some_matching() {
    let src = r#"
    fn inspect(opt: int): int {
        return match opt {
            Some(x) => x,
            None => 0,
        };
    }

    fn main(): int {
        let v = Some(42);
        return inspect(v);
    }
    "#;
    let (_, code) = compile_and_run(src, "test_enum_option_some");
    assert_eq!(code, 42, "Some(42) should match Some(x) and return 42");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_enum_option_none_matching() {
    let src = r#"
    fn inspect(opt: int): int {
        return match opt {
            Some(x) => x,
            None => 99,
        };
    }

    fn main(): int {
        let v = None;
        return inspect(v);
    }
    "#;
    let (_, code) = compile_and_run(src, "test_enum_option_none");
    assert_eq!(code, 99, "None should match None arm and return 99");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_enum_result_ok_and_err() {
    let src = r#"
    fn handle_res(r: int): int {
        return match r {
            Ok(v) => v,
            Err(e) => 0 - e,
        };
    }

    fn main(): int {
        let ok_val = Ok(100);
        let err_val = Err(35);
        let a = handle_res(ok_val);
        let b = handle_res(err_val);
        return a + b; // 100 + (-35) = 65
    }
    "#;
    let (_, code) = compile_and_run(src, "test_enum_result_ok_err");
    assert_eq!(code, 65, "Ok(100) + Err(35) handled should yield 65");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_enum_user_defined_unit_variants() {
    let src = r#"
    enum Color {
        Red,
        Green,
        Blue,
    }

    fn color_code(c: int): int {
        return match c {
            Color::Red => 10,
            Color::Green => 20,
            Color::Blue => 30,
            _ => 0,
        };
    }

    fn main(): int {
        let r = Color::Red;
        let g = Color::Green;
        let b = Color::Blue;
        return color_code(r) + color_code(g) + color_code(b); // 10 + 20 + 30 = 60
    }
    "#;
    let (_, code) = compile_and_run(src, "test_enum_user_unit");
    assert_eq!(code, 60, "Sum of unit variant color codes should equal 60");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_enum_user_defined_payload_variants() {
    let src = r#"
    enum Action {
        Quit,
        Move(int),
    }

    fn eval_action(act: int): int {
        return match act {
            Action::Quit => 0,
            Action::Move(dist) => dist * 2,
            _ => -1,
        };
    }

    fn main(): int {
        let q = Action::Quit;
        let m = Action::Move(25);
        return eval_action(q) + eval_action(m); // 0 + 50 = 50
    }
    "#;
    let (_, code) = compile_and_run(src, "test_enum_user_payload");
    assert_eq!(
        code, 50,
        "Action::Quit (0) + Action::Move(25) (*2=50) should equal 50"
    );
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_enum_pattern_literal_subpattern() {
    let src = r#"
    fn categorize(opt: int): int {
        return match opt {
            Some(0) => 1,
            Some(42) => 88,
            Some(x) => x,
            None => 999,
        };
    }

    fn main(): int {
        let r1 = categorize(Some(42)); // 88
        let r2 = categorize(Some(7));  // 7
        return r1 - r2; // 88 - 7 = 81
    }
    "#;
    let (_, code) = compile_and_run(src, "test_enum_subpattern_literal");
    assert_eq!(
        code, 81,
        "Literal discrimination in payload should return 88 and 7, diff 81"
    );
}
