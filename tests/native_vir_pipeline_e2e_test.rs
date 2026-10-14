//! End-to-end test for Middle-End VIR to Machine IR Pipeline:
//! AST -> HIR -> MIR -> VIR -> Machine IR -> Codegen (x86_64) -> ADOB v3 -> Linker -> Execution

#![allow(dead_code, unused_imports)]

use adesh_codegen::targets::create_backend;
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use adeshlang::backends::native::vir_lower::vir_to_machine_ir;
use adeshlang::ir::mir::lower::lower_hir_to_mir;
use adeshlang::ir::vir::lower::lower_mir_to_vir;
use adeshlang::parsing::hir_lower::ast_to_hir;
use adeshlang::parsing::lexer::Lexer;
use adeshlang::parsing::parser::Parser;
use std::process::Command;
use tempfile::tempdir;

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn compile_run_vir_pipeline(src: &str, test_name: &str) -> (String, i32) {
    let tokens = Lexer::new(src).tokenize().expect("tokenize");
    let mut parser = Parser::new(tokens, None);
    let ast = parser.parse_program().expect("parse");
    let hir = ast_to_hir(&ast, true).expect("ast_to_hir");
    let mir = lower_hir_to_mir(&hir).expect("lower_hir_to_mir");
    let vir = lower_mir_to_vir(&mir).expect("lower_mir_to_vir");
    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let native_mod = vir_to_machine_ir(&vir, &target).expect("vir_to_machine_ir");

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
fn test_vir_pipeline_arithmetic_and_function_call() {
    let src = r#"
    fn add(a: int, b: int): int {
        return a + b;
    }

    fn main(): int {
        let x = 20;
        let y = 22;
        return add(x, y);
    }
    "#;
    let (_stdout, code) = compile_run_vir_pipeline(src, "test_vir_arithmetic");
    assert_eq!(
        code, 42,
        "Pipeline AST->HIR->MIR->VIR->MachineIR->ADOB->Linker must exit with 42"
    );
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_vir_pipeline_conditional_branching() {
    let src = r#"
    fn max(a: int, b: int): int {
        if a > b {
            return a;
        } else {
            return b;
        }
    }

    fn main(): int {
        return max(15, 37);
    }
    "#;
    let (_stdout, code) = compile_run_vir_pipeline(src, "test_vir_branching");
    assert_eq!(
        code, 37,
        "Pipeline must handle branches/phi nodes and exit with 37"
    );
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn test_vir_pipeline_memory_drop_and_reassignment() {
    let src = r#"
    fn compute(): int {
        let a = "initial";
        a = "reassigned";
        let b = 100;
        return b;
    }

    fn main(): int {
        return compute();
    }
    "#;
    let (_stdout, code) = compile_run_vir_pipeline(src, "test_vir_memory_drop");
    assert_eq!(
        code, 100,
        "Must exit with 100 with drops properly inserted on reassignment and function return"
    );
}
