//! Phase 18 — Source-level `extern "C"` reachability.
//!
//! An Adesh source file declares `extern "C" fn puts(...)`, native lowering
//! retains the decl as an import, marshals the string argument as a C cstring
//! (not an Adesh handle), and a dynamic ELF link resolves `puts` from libc.
//!
//! Execution is Linux x86-64 only (loud SKIP elsewhere).

use adesh_codegen::machine_ir::MachineInstruction;
use adesh_codegen::targets::create_backend;
use adesh_linker::{LinkConfig, Target};
use adesh_object::TargetDescriptor;
use adesh_object::validator::AdobValidator;
use adesh_object::writer::AdobWriter;
use adeshlang::backends::native::lower::lower_hir_module;
use adeshlang::parsing::hir_lower::ast_to_hir;
use adeshlang::parsing::lexer::Lexer;
use adeshlang::parsing::parser::Parser;
use std::path::Path;
use std::process::Command;

fn can_execute_elf() -> bool {
    cfg!(target_os = "linux")
}

fn execute_elf(path: &Path) -> std::io::Result<std::process::Output> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(path, perms);
        }
        Command::new(path).output()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "ELF execution requires Linux",
        ))
    }
}

fn lower_module(src: &str) -> adesh_codegen::machine_ir::NativeModule {
    let tokens = Lexer::new(src).tokenize().expect("tokenize");
    let mut parser = Parser::new(tokens, None);
    let ast = parser.parse_program().expect("parse");
    let hir = ast_to_hir(&ast, true).expect("ast_to_hir");
    assert!(
        !hir.externs.is_empty(),
        "Phase 18: HIR must retain extern declarations"
    );
    let target = TargetDescriptor::from_triple("x86_64-unknown-linux-gnu").expect("valid triple");
    lower_hir_module(&hir, &target).expect("native lowering")
}

fn compile_link_and_run_linux(src: &str, test_name: &str) -> (i32, Vec<u8>) {
    let native_mod = lower_module(src);
    let target = TargetDescriptor::from_triple("x86_64-unknown-linux-gnu").expect("valid triple");
    let mut backend = create_backend(target).expect("backend creation");
    let obj = backend.emit_object(&native_mod).expect("ADOB emission");
    AdobValidator::validate(&obj).expect("emitted ADOB must validate");

    let bytes = AdobWriter::write(&obj).expect("ADOB encoding");
    let test_dir = std::env::current_dir()
        .unwrap()
        .join("target")
        .join("phase18_tests");
    std::fs::create_dir_all(&test_dir).expect("create test_dir");

    static TEST_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let count = TEST_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let unique_name = format!("{test_name}_{}_{}", std::process::id(), count);

    let adob_path = test_dir.join(format!("{unique_name}.adob"));
    std::fs::write(&adob_path, bytes).expect("write ADOB file");

    let elf_path = test_dir.join(format!("{unique_name}.elf"));
    let target = Target::from_triple("x86_64-unknown-linux-gnu").expect("triple");
    let mut config = LinkConfig::new(elf_path.clone(), target);
    config.dynamic = true;
    adesh_linker::Linker::link(&[adob_path], config).expect("native ELF dynamic link");
    assert!(elf_path.exists(), "linked ELF binary must exist");

    let out = execute_elf(&elf_path).expect("execute ELF binary");
    (out.status.code().unwrap_or(-1), out.stdout)
}

fn module_calls_symbol(module: &adesh_codegen::machine_ir::NativeModule, symbol: &str) -> bool {
    module.imports.iter().any(|s| s == symbol)
        || module.functions.iter().any(|f| {
            f.blocks.iter().any(|b| {
                b.instructions.iter().any(|i| match i {
                    MachineInstruction::Call { target, .. } => {
                        matches!(target, adesh_codegen::machine_ir::MachineOperand::Symbol(s) if s == symbol)
                    }
                    _ => false,
                })
            })
        })
}

#[test]
fn phase18_hir_retains_extern_and_imports_puts() {
    let module = lower_module(
        r#"
        extern "C" fn puts(s: string) -> i32;
        fn main(): int {
            puts("phase18");
            return 0;
        }
        "#,
    );
    assert!(
        module.imports.iter().any(|s| s == "puts"),
        "extern puts must become a NativeModule import; got {:?}",
        module.imports
    );
    assert!(
        module_calls_symbol(&module, "puts"),
        "main must emit a Call to puts"
    );
}

#[test]
fn phase18_unsupported_abi_is_refused_loudly() {
    let tokens = Lexer::new(
        r#"
        extern "Wasm" fn host_add(a: i32, b: i32) -> i32;
        fn main(): int { return 0; }
        "#,
    )
    .tokenize()
    .unwrap();
    let mut parser = Parser::new(tokens, None);
    let ast = parser.parse_program().unwrap();
    let hir = ast_to_hir(&ast, true).unwrap();
    let target = TargetDescriptor::from_triple("x86_64-unknown-linux-gnu").unwrap();
    let err = lower_hir_module(&hir, &target).expect_err("Wasm ABI must be refused");
    let joined = err.diagnostics.join("\n");
    assert!(
        joined.contains("Wasm") || joined.contains("C-family"),
        "unexpected diagnostics: {joined}"
    );
}

#[test]
fn phase18_puts_executes_via_libc_on_linux() {
    if !can_execute_elf() {
        eprintln!(
            "SKIP: Phase 18 source extern execution needs Linux; host is {}/{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        return;
    }
    // `_start` exits via raw `sys_exit`, which does not flush libc stdio.
    // `fflush(null)` forces the `puts` buffer out before we return.
    let (code, stdout) = compile_link_and_run_linux(
        r#"
        extern "C" fn puts(s: string) -> i32;
        extern "C" fn fflush(stream: ptr) -> i32;
        fn main(): int {
            puts("phase18 extern c");
            fflush(null);
            return 0;
        }
        "#,
        "puts_exec",
    );
    assert_eq!(code, 0, "exit status");
    assert_eq!(
        String::from_utf8_lossy(&stdout),
        "phase18 extern c\n",
        "puts must write the cstring through libc"
    );
}
