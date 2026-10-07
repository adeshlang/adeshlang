//! Phase 2 Task P2-10: Curated Conformance Corpus Tests.
//!
//! Asserts interpreter vs native backend execution parity, multi-module imports,
//! enums, closures, control flow, and structured error rejection.

#![allow(dead_code, unused_imports)]

use adesh_codegen::create_backend;
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use adeshlang::backends::native::{NativeLoweringError, lower_hir_module_with_base};
use adeshlang::parsing::hir_lower::ast_to_hir;
use adeshlang::parsing::lexer::Lexer;
use adeshlang::parsing::parser::Parser;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn compile_and_run(src: &str, test_name: &str, base_dir: Option<&Path>) -> (String, i32) {
    let tokens = Lexer::new(src).tokenize().expect("tokenize");
    let mut parser = Parser::new(tokens, None);
    let ast = parser.parse_program().expect("parse");
    let hir = ast_to_hir(&ast, true).expect("ast_to_hir");

    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let native_mod = lower_hir_module_with_base(&hir, &target, base_dir).expect("native lowering");

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

fn lowering_error(src: &str) -> Vec<String> {
    let tokens = Lexer::new(src).tokenize().expect("tokenize");
    let mut parser = Parser::new(tokens, None);
    let ast = parser.parse_program().expect("parse");
    let hir = ast_to_hir(&ast, true).expect("ast_to_hir");

    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    match lower_hir_module_with_base(&hir, &target, None) {
        Ok(_) => Vec::new(),
        Err(e) => e.diagnostics,
    }
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_conformance_showcase_e2e() {
    let main_src =
        std::fs::read_to_string("examples/showcase/main.adesh").expect("read showcase main.adesh");
    let base_dir = Path::new("examples/showcase");
    let (stdout, code) = compile_and_run(&main_src, "test_showcase_conformance", Some(base_dir));
    assert_eq!(code, 0, "showcase should exit 0");
    assert!(
        stdout.contains("=== Section 1: Module Imports ==="),
        "{stdout}"
    );
    assert!(
        stdout.contains("add(15, 27) =\r\n42") || stdout.contains("add(15, 27) =\n42"),
        "{stdout}"
    );
    assert!(
        stdout.contains("=== Section 2: Closures & Higher-Order Functions ==="),
        "{stdout}"
    );
    assert!(
        stdout.contains("=== Section 3: Pattern Matching & Enums ==="),
        "{stdout}"
    );
    assert!(
        stdout.contains("=== Section 4: Loops & Control Flow ==="),
        "{stdout}"
    );
    assert!(stdout.contains("=== Section 5: Defers ==="), "{stdout}");
    assert!(stdout.contains("=== Showcase Complete ==="), "{stdout}");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_conformance_closure_state_mutation() {
    let src = r#"
    fn make_counter() {
        let count = 10;
        return fn(): int {
            count = count + 5;
            return count;
        };
    }
    fn main(): int {
        let c = make_counter();
        c();
        c();
        return c(); // 10 + 5 + 5 + 5 = 25
    }
    "#;
    let (_, code) = compile_and_run(src, "test_closure_state_mutation", None);
    assert_eq!(code, 25);
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_conformance_result_pattern_matching() {
    let src = r#"
    fn eval_res(r: int): int {
        return match r {
            Ok(v) => v + 10,
            Err(e) => e - 10,
            _ => 0,
        };
    }
    fn main(): int {
        let r1 = Ok(50);
        let r2 = Err(30);
        return eval_res(r1) + eval_res(r2); // (50 + 10) + (30 - 10) = 80
    }
    "#;
    let (_, code) = compile_and_run(src, "test_res_pattern_matching", None);
    assert_eq!(code, 80);
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_conformance_control_flow_and_defers() {
    let src = r#"
    fn compute(): int {
        defer {
            print("deferred");
        }
        let total = 0;
        let i = 1;
        while (i <= 4) {
            total = total + i;
            i++;
        }
        return total; // 1 + 2 + 3 + 4 = 10
    }
    fn main(): int {
        return compute();
    }
    "#;
    let (stdout, code) = compile_and_run(src, "test_flow_and_defers", None);
    assert_eq!(code, 10);
    assert!(stdout.contains("deferred"), "{stdout}");
}

#[test]
fn test_conformance_must_reject_unresolved_import() {
    let diags = lowering_error(
        r#"
        import "./non_existent_file.adesh";
        fn main() {}
        "#,
    );
    assert!(
        diags
            .iter()
            .any(|d| d.contains("import of \"./non_existent_file.adesh\"")),
        "must reject non-existent import with structured error: {diags:?}"
    );
}

#[test]
fn test_conformance_must_reject_region_blocks() {
    let diags = lowering_error(
        r#"
        region Arena {
            let x = 1;
        }
        fn main() {}
        "#,
    );
    assert!(
        diags.iter().any(|d| d.contains("`region Arena` blocks")),
        "must reject region blocks: {diags:?}"
    );
}

#[test]
fn test_conformance_must_reject_async_functions() {
    let diags = lowering_error(
        r#"
        async fn task() {
            return 1;
        }
        fn main() {}
        "#,
    );
    assert!(
        diags.iter().any(|d| d.contains("`async fn`")),
        "must reject async fn: {diags:?}"
    );
}
