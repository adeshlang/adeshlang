//! End-to-end tests for Native Concurrency (spawn and await)
//! Verifies:
//! 1. Spawning worker thread functions with scalar arguments.
//! 2. Awaiting thread completion and returning results.
//! 3. Multiple concurrent worker threads computing in parallel.

#![allow(dead_code, unused_imports)]

use adesh_codegen::targets::create_backend;
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use adeshlang::backends::native::lower::lower_hir_module;
use adeshlang::parsing::hir_lower::ast_to_hir;
use adeshlang::parsing::lexer::Lexer;
use adeshlang::parsing::parser::Parser;
use std::process::Command;
use tempfile::tempdir;

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn compile_run_src(src: &str, test_name: &str) -> (String, i32) {
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
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if !stderr.is_empty() {
        eprintln!("STDERR: {}", stderr);
    }
    (stdout, code)
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_thread_spawn_and_await_single() {
    let src = r#"
    fn worker(x: int): int {
        return x * 2;
    }

    fn main(): int {
        let t = spawn worker(21);
        let res = await t;
        return res;
    }
    "#;
    let (_stdout, code) = compile_run_src(src, "test_thread_single");
    assert_eq!(code, 42, "Spawned worker must return 42");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_thread_spawn_and_await_parallel() {
    let src = r#"
    fn work1(n: int): int {
        return n + 10;
    }

    fn work2(n: int): int {
        return n * 3;
    }

    fn main(): int {
        let t1 = spawn work1(15);
        let t2 = spawn work2(10);
        let r1 = await t1;
        let r2 = await t2;
        return r1 + r2;
    }
    "#;
    let (_stdout, code) = compile_run_src(src, "test_thread_parallel");
    assert_eq!(code, 55, "Parallel workers (25 + 30) must return 55");
}
