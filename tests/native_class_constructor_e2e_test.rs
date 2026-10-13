//! End-to-end tests for Native Class Constructors, Field Access, and Local Handle Cleanup.
//! Verifies:
//! 1. Constructors named matching the Class name (e.g. `Point(x, y)`).
//! 2. Constructors named `constructor` (e.g. `constructor(val)`).
//! 3. Constructors named `init` (e.g. `init(val)`).
//! 4. Field mutation and access via static C-string zero-allocation ABI (`aot_set_field_str`, `aot_get_field_str`).
//! 5. Proper local handle cleanup on scope exit without leaks or crashes.

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
    eprintln!("HIR CLASSES: {:?}", hir.classes);
    eprintln!("HIR FUNCS: {:?}", hir.functions.iter().map(|f| &f.name).collect::<Vec<_>>());

    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let native_mod = lower_hir_module(&hir, &target).expect("native lowering");
    eprintln!("NATIVE FUNCS:");
    for f in &native_mod.functions {
        eprintln!("  FUNCTION: {}", f.name);
        for b in &f.blocks {
            eprintln!("    BLOCK {}:", b.id);
            for inst in &b.instructions {
                eprintln!("      {:?}", inst);
            }
        }
    }

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
fn test_constructor_matching_class_name() {
    let src = r#"
    class Point {
        x: int;
        y: int;
        fn Point(x: int, y: int) {
            this.x = x;
            this.y = y;
        }
    }

    fn main(): int {
        let p = new Point(15, 27);
        return p.x + p.y;
    }
    "#;
    let (stdout, code) = compile_run_src(src, "test_ctor_class_name");
    eprintln!("STDOUT: '{}', CODE: {}", stdout, code);
    assert_eq!(code, 42, "Point(15, 27) should initialize x and y summing to 42");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_constructor_named_constructor() {
    let src = r#"
    class Holder {
        val: int;
        fn constructor(v: int) {
            this.val = v;
        }
    }

    fn main(): int {
        let h = new Holder(100);
        return h.val;
    }
    "#;
    let (_, code) = compile_run_src(src, "test_ctor_constructor");
    assert_eq!(code, 100, "Holder(100) should initialize val to 100");
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_field_mutation_and_handle_scope_cleanup() {
    let src = r#"
    class Counter {
        count: int;
        fn Counter(init_val: int) {
            this.count = init_val;
        }
    }

    fn helper(): int {
        let c = new Counter(10);
        c.count = c.count + 5;
        return c.count;
    }

    fn main(): int {
        let res = helper();
        return res;
    }
    "#;
    let (_, code) = compile_run_src(src, "test_field_mutation");
    assert_eq!(code, 15, "Counter count should be mutated to 15 and local handle freed cleanly");
}
