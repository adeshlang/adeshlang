//! Phase 18 — Source-level `extern "C"` reachability.
//!
//! An Adesh source file declares `extern "C" fn puts(...)`, native lowering
//! retains the decl as an import, marshals the string argument as a C cstring
//! (not an Adesh handle), and a dynamic link resolves `puts` from the host C
//! library (glibc on Linux, libSystem on macOS).
//!
//! Execution: Linux x86-64 (light gate) and host Apple triples (macho-dyld-gate).
//! Loud SKIP elsewhere (or fail under `ADESH_REQUIRE_FOREIGN_EXEC=1`).

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

fn require_or_skip(ok: bool, why: &str) -> bool {
    if ok {
        return true;
    }
    assert!(
        std::env::var_os("ADESH_REQUIRE_FOREIGN_EXEC").is_none(),
        "required Phase 18 source-extern execution: {why} (host is {}/{})",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    eprintln!(
        "SKIP: Phase 18 source-extern execution: {why}; host is {}/{}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    false
}

fn macos_host_triple() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        _ => None,
    }
}

fn execute_native(path: &Path) -> std::io::Result<std::process::Output> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(path, perms);
        }
        Command::new(path).output()
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "native execution requires a Unix host for this suite",
        ))
    }
}

fn lower_module_for(src: &str, triple: &str) -> adesh_codegen::machine_ir::NativeModule {
    let tokens = Lexer::new(src).tokenize().expect("tokenize");
    let mut parser = Parser::new(tokens, None);
    let ast = parser.parse_program().expect("parse");
    let hir = ast_to_hir(&ast, true).expect("ast_to_hir");
    assert!(
        !hir.externs.is_empty(),
        "Phase 18: HIR must retain extern declarations"
    );
    let target = TargetDescriptor::from_triple(triple).expect("valid triple");
    lower_hir_module(&hir, &target).expect("native lowering")
}

fn compile_link_and_run(src: &str, test_name: &str, triple: &str) -> (i32, Vec<u8>) {
    let native_mod = lower_module_for(src, triple);
    let target = TargetDescriptor::from_triple(triple).expect("valid triple");
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

    let out_path = test_dir.join(format!("{unique_name}.bin"));
    let link_target = Target::from_triple(triple).expect("triple");
    let mut config = LinkConfig::new(out_path.clone(), link_target);
    // ELF needs an explicit dynamic link for libc imports; Mach-O is always
    // dyld-bound for MH_EXECUTE+PIE.
    config.dynamic = !triple.contains("apple-darwin");
    adesh_linker::Linker::link(&[adob_path], config).expect("native dynamic link");
    assert!(out_path.exists(), "linked binary must exist");

    let out = execute_native(&out_path).expect("execute linked binary");
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

const PUTS_SOURCE: &str = r#"
        extern "C" fn puts(s: string) -> i32;
        extern "C" fn fflush(stream: ptr) -> i32;
        fn main(): int {
            puts("phase18 extern c");
            fflush(null);
            return 0;
        }
        "#;

#[test]
fn phase18_hir_retains_extern_and_imports_puts() {
    let module = lower_module_for(
        r#"
        extern "C" fn puts(s: string) -> i32;
        fn main(): int {
            puts("phase18");
            return 0;
        }
        "#,
        "x86_64-unknown-linux-gnu",
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
    if !require_or_skip(cfg!(target_os = "linux"), "needs Linux") {
        return;
    }
    // `_start` exits via raw `sys_exit`, which does not flush libc stdio.
    // `fflush(null)` forces the `puts` buffer out before we return.
    let (code, stdout) =
        compile_link_and_run(PUTS_SOURCE, "puts_exec_linux", "x86_64-unknown-linux-gnu");
    assert_eq!(code, 0, "exit status");
    assert_eq!(
        String::from_utf8_lossy(&stdout),
        "phase18 extern c\n",
        "puts must write the cstring through libc"
    );
}

#[test]
fn phase18_puts_executes_via_libsystem_on_macos() {
    let Some(triple) = macos_host_triple() else {
        let _ = require_or_skip(false, "needs macOS host");
        return;
    };
    let (code, stdout) = compile_link_and_run(PUTS_SOURCE, "puts_exec_macos", triple);
    assert_eq!(code, 0, "{triple}: exit status");
    assert_eq!(
        String::from_utf8_lossy(&stdout),
        "phase18 extern c\n",
        "{triple}: puts must write the cstring through libSystem"
    );
}
