//! Comprehensive integration tests for multi-variable declarations (`let a = 10, b = 20, c = 30;`
//! and `let a: u8 = 8, b: string = "hello", c: bool = true;`) across:
//! - Parser / AST structure
//! - Interpreter
//! - Native JIT (NJIT)
//! - WebAssembly (WASM)
//! - Native AOT (Ahead-of-Time compilation)

use adeshlang::{Interpreter, ModuleLoader};
use std::path::Path;
use tempfile::tempdir;

fn run_interp(src: &str) -> Result<(), String> {
    let src_owned = src.to_string();
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let mut loader = ModuleLoader::new(Path::new("."));
            let mut interp = Interpreter::new();
            interp
                .run_module(&src_owned, &mut loader, None)
                .map_err(|e| e.to_string())
        })
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn test_parser_multi_let_untyped() {
    use adeshlang::parsing::ast::StmtKind;
    use adeshlang::parsing::lexer::Lexer;
    use adeshlang::parsing::parser::Parser;

    let src = "let a = 10, b = 20, c = 30;";
    let tokens = Lexer::new(src).tokenize().expect("tokenize");
    let mut parser = Parser::new(tokens, None);
    let ast = parser.parse_program().expect("parse");

    assert_eq!(ast.len(), 3, "Expected 3 independent let statements");
    if let StmtKind::Let(name, Some(_), None, false, false, false) = &ast[0].kind {
        assert_eq!(name, "a");
    } else {
        panic!("Expected Let for 'a'");
    }
    if let StmtKind::Let(name, Some(_), None, false, false, false) = &ast[1].kind {
        assert_eq!(name, "b");
    } else {
        panic!("Expected Let for 'b'");
    }
    if let StmtKind::Let(name, Some(_), None, false, false, false) = &ast[2].kind {
        assert_eq!(name, "c");
    } else {
        panic!("Expected Let for 'c'");
    }
}

#[test]
fn test_parser_multi_let_typed() {
    use adeshlang::parsing::ast::StmtKind;
    use adeshlang::parsing::lexer::Lexer;
    use adeshlang::parsing::parser::Parser;

    let src = "let a: u8 = 8, b: string = \"hello\", c: bool = true;";
    let tokens = Lexer::new(src).tokenize().expect("tokenize");
    let mut parser = Parser::new(tokens, None);
    let ast = parser.parse_program().expect("parse");

    assert_eq!(ast.len(), 3, "Expected 3 independent let statements");
    if let StmtKind::Let(name, Some(_), Some(ty), false, false, false) = &ast[0].kind {
        assert_eq!(name, "a");
        assert_eq!(ty, "u8");
    } else {
        panic!("Expected typed Let for 'a'");
    }
    if let StmtKind::Let(name, Some(_), Some(ty), false, false, false) = &ast[1].kind {
        assert_eq!(name, "b");
        assert_eq!(ty, "string");
    } else {
        panic!("Expected typed Let for 'b'");
    }
    if let StmtKind::Let(name, Some(_), Some(ty), false, false, false) = &ast[2].kind {
        assert_eq!(name, "c");
        assert_eq!(ty, "bool");
    } else {
        panic!("Expected typed Let for 'c'");
    }
}

#[test]
fn test_interpreter_multi_let_untyped_and_typed() {
    let src = r#"
let a = 10, b = 20, c = 30;
let x: u8 = 8, y: string = "hello", z: bool = true;
let sum = a + b + c + x;
"#;
    let res = run_interp(src);
    assert!(res.is_ok(), "Interpreter execution failed: {:?}", res);
}

#[test]
fn test_interpreter_multi_let_in_function() {
    let src = r#"
fn compute() {
    let a = 10, b = 20, c = 30;
    let s: string = "result: ", flag: bool = true;
    return a + b + c;
}
let res = compute();
"#;
    let res = run_interp(src);
    assert!(
        res.is_ok(),
        "Interpreter function execution failed: {:?}",
        res
    );
}

#[test]
fn test_native_jit_multi_let() {
    let src = r#"
fn main() {
    let a = 10, b = 20, c = 30;
    let x: u8 = 8, y: string = "hello", z: bool = true;
    print(a);
    print(b);
    print(c);
    print(x);
    print(y);
    print(z);
}
"#;
    let res = adeshlang::backends::jit::native::native_jit_run(src);
    assert!(res.is_ok(), "Native JIT multi-let failed: {:?}", res);
}

#[test]
fn test_wasm_multi_let_compiles() {
    let src = r#"
let a = 10, b = 20, c = 30;
let x: u8 = 8, y: string = "hello", z: bool = true;
print(a);
print(b);
print(c);
"#;
    let dir = tempdir().expect("tempdir");
    let wasm_path = dir.path().join("test_multi_let.wasm");

    let res = adeshlang::backends::wasm::compiler::compile_to_file(src, &wasm_path);
    assert!(res.is_ok(), "WASM compile failed: {:?}", res);
    assert!(wasm_path.exists(), "WASM output file must exist");
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[test]
fn test_native_aot_multi_let() {
    use adesh_codegen::create_backend;
    use adesh_object::TargetDescriptor;
    use adesh_object::validator::AdobValidator;
    use adesh_object::writer::AdobWriter;
    use adeshlang::backends::native::lower_hir_module_with_base;
    use adeshlang::parsing::hir_lower::ast_to_hir;
    use adeshlang::parsing::lexer::Lexer;
    use adeshlang::parsing::parser::Parser;
    use std::process::Command;

    let src = r#"
fn main() {
    let a = 10, b = 20, c = 30;
    let x: u8 = 8, y: string = "hello", z: bool = true;
    print(a);
    print(b);
    print(c);
    print(x);
    print(y);
    print(z);
}
"#;
    let tokens = Lexer::new(src).tokenize().expect("tokenize");
    let mut parser = Parser::new(tokens, None);
    let ast = parser.parse_program().expect("parse");
    let hir = ast_to_hir(&ast, true).expect("ast_to_hir");

    let target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").expect("valid triple");
    let native_mod = lower_hir_module_with_base(&hir, &target, None).expect("native lowering");

    let mut backend = create_backend(target.clone()).expect("backend creation");
    let obj = backend.emit_object(&native_mod).expect("ADOB emission");
    AdobValidator::validate(&obj).expect("emitted ADOB must validate");

    let bytes = AdobWriter::write(&obj).expect("ADOB encoding");
    let dir = tempdir().expect("tempdir");
    let adob_path = dir.path().join("test_multi_let_aot.adob");
    std::fs::write(&adob_path, bytes).expect("write ADOB file");

    let exe_path = dir.path().join("test_multi_let_aot.exe");
    adesh_linker::link(&[&adob_path], &exe_path, Some("x86_64-pc-windows-msvc"))
        .expect("native link");
    assert!(exe_path.exists(), "linked executable must exist");

    let out = Command::new(&exe_path)
        .output()
        .expect("execute native binary");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("10"), "stdout: {}", stdout);
    assert!(stdout.contains("20"), "stdout: {}", stdout);
    assert!(stdout.contains("30"), "stdout: {}", stdout);
    assert!(stdout.contains("8"), "stdout: {}", stdout);
    assert!(stdout.contains("hello"), "stdout: {}", stdout);
    assert!(stdout.contains("true"), "stdout: {}", stdout);
}
