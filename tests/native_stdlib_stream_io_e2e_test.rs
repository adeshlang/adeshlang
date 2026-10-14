//! End-to-end test for Native Stdlib Stream I/O & Binary Packing
//!
//! Verifies:
//! 1. `io_stdout_write` writes directly to stdout in a compiled native executable.
//! 2. `io_pack_number` and `io_unpack_number` correctly serialize/deserialize numbers.
//! 3. `io_utf8_encode`, `io_slice_bytes`, and `io_utf8_decode` roundtrip bytes and strings.

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
fn test_native_stream_io_stdout_and_pack() {
    let src = r#"
    fn main(): int {
        io_stdout_write("STREAM_NATIVE_HELLO\n");
        let bytes = io_pack_number("u32", 42, "LE");
        let val = io_unpack_number("u32", bytes, "LE");
        return val;
    }
    "#;
    let (stdout, code) = compile_run_src(src, "test_stream_io_stdout_pack");
    assert_eq!(code, 42, "Unpacked u32 must equal 42");
    assert!(
        stdout.contains("STREAM_NATIVE_HELLO"),
        "Stdout must contain STREAM_NATIVE_HELLO, got: {stdout}"
    );
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_native_stream_io_utf8_slice_concat() {
    let src = r#"
    fn main(): int {
        let b1 = io_utf8_encode("Hello ");
        let b2 = io_utf8_encode("World!");
        let all = io_bytes_concat(b1, b2);
        let sliced = io_slice_bytes(all, 6, 5);
        let s = io_utf8_decode(sliced);
        io_stdout_write(s);
        return len(all);
    }
    "#;
    let (stdout, code) = compile_run_src(src, "test_stream_io_utf8_slice");
    assert_eq!(code, 12, "Total concatenated bytes length should be 12");
    assert!(
        stdout.contains("World"),
        "Stdout must contain sliced 'World', got: {stdout}"
    );
}
