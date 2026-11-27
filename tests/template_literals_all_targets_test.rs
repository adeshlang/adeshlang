//! Comprehensive integration tests for template literals `${}` and format specifiers `${expr:spec}`
//! across all execution targets: Native JIT, Native AOT, and WASM.

use tempfile::tempdir;

#[test]
fn test_native_jit_template_literal() {
    let src = r#"
let item = "apples";
let count = 5;
let msg = `I have ${count} ${item}!`;
print(msg);
"#;
    let res = adeshlang::backends::jit::native::native_jit_run(src);
    assert!(res.is_ok(), "Native JIT failed: {:?}", res);
}

#[test]
fn test_native_jit_template_format() {
    let src = r#"
let num = 255;
let formatted = `Color: ${num:hex}`;
print(formatted);
"#;
    let res = adeshlang::backends::jit::native::native_jit_run(src);
    assert!(res.is_ok(), "Native JIT format failed: {:?}", res);
}

#[test]
fn test_wasm_template_literal_compiles() {
    let src = r#"
let name = "WASM";
let s = `Hello, ${name}!`;
print(s);
"#;
    let dir = tempdir().expect("tempdir");
    let wasm_path = dir.path().join("test_wasm_tpl.wasm");

    let res = adeshlang::backends::wasm::compiler::compile_to_file(src, &wasm_path);
    assert!(res.is_ok(), "WASM compile failed: {:?}", res);
    assert!(wasm_path.exists(), "WASM output file must exist");
}

#[test]
fn test_wasm_template_literal_format_spec_compiles() {
    let src = r#"
let val = 42;
let s = `Result: ${val:hex}`;
print(s);
"#;
    let dir = tempdir().expect("tempdir");
    let wasm_path = dir.path().join("test_wasm_fmt.wasm");

    let res = adeshlang::backends::wasm::compiler::compile_to_file(src, &wasm_path);
    assert!(res.is_ok(), "WASM compile format failed: {:?}", res);
    assert!(wasm_path.exists(), "WASM output file must exist");
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[test]
fn test_native_aot_template_literal_and_format() {
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
    let a = 10;
    let b = 20;
    let msg = `Sum: ${a} + ${b} = ${a + b}`;
    print(msg);
    let hex_val = 255;
    let hex_msg = `Hex: ${hex_val:hex}`;
    print(hex_msg);
    print(`${getGreeting("Alice")}`);
}
fn getGreeting(name) {
    return `Hello, ${name}!`;
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
    let adob_path = dir.path().join("test_template_aot.adob");
    std::fs::write(&adob_path, bytes).expect("write ADOB file");

    let exe_path = dir.path().join("test_template_aot.exe");
    adesh_linker::link(&[&adob_path], &exe_path, Some("x86_64-pc-windows-msvc"))
        .expect("native link");
    assert!(exe_path.exists(), "linked executable must exist");

    let out = Command::new(&exe_path)
        .output()
        .expect("execute native binary");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Sum: 10 + 20 = 30"), "stdout: {}", stdout);
    assert!(stdout.contains("Hex: ff"), "stdout: {}", stdout);
    assert!(stdout.contains("Hello, Alice!"), "stdout: {}", stdout);
}
