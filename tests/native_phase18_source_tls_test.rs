//! Phase 18 — Source-level `thread_local let` reachability.
//!
//! An Adesh source file declares `thread_local let counter = 42;`, native
//! lowering places it in `.tls` with `SymbolKind::TLS`, loads go through
//! `TlsAddress` (SysV initial-exec), and a dynamic Linux x86-64 link exits 42.

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
        "required Phase 18 source-TLS execution: {why} (host is {}/{})",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    eprintln!(
        "SKIP: Phase 18 source-TLS execution: {why}; host is {}/{}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    false
}

fn chmod_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(path, perms);
        }
    }
}

fn lower_module_for(src: &str, triple: &str) -> adesh_codegen::machine_ir::NativeModule {
    let tokens = Lexer::new(src).tokenize().expect("tokenize");
    let mut parser = Parser::new(tokens, None);
    let ast = parser.parse_program().expect("parse");
    let hir = ast_to_hir(&ast, true).expect("ast_to_hir");
    assert!(
        !hir.tls_vars.is_empty(),
        "Phase 18: HIR must retain thread_local declarations"
    );
    let target = TargetDescriptor::from_triple(triple).expect("valid triple");
    lower_hir_module(&hir, &target).expect("native lowering")
}

fn compile_and_link(src: &str, test_name: &str, triple: &str) -> std::path::PathBuf {
    let native_mod = lower_module_for(src, triple);
    let target = TargetDescriptor::from_triple(triple).expect("valid triple");
    let mut backend = create_backend(target).expect("backend creation");
    let lowered = backend
        .lower_module(&native_mod)
        .expect("register allocation / backend lower");
    let obj = backend.emit_object(&lowered).expect("ADOB emission");
    AdobValidator::validate(&obj).expect("emitted ADOB must validate");

    let bytes = AdobWriter::write(&obj).expect("ADOB encoding");
    let test_dir = std::env::current_dir()
        .unwrap()
        .join("target")
        .join("phase18_tls_tests");
    std::fs::create_dir_all(&test_dir).expect("create test_dir");

    static TEST_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let count = TEST_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let unique_name = format!("{test_name}_{}_{}", std::process::id(), count);

    let adob_path = test_dir.join(format!("{unique_name}.adob"));
    std::fs::write(&adob_path, bytes).expect("write ADOB file");

    let out_path = test_dir.join(format!("{unique_name}.bin"));
    let link_target = Target::from_triple(triple).expect("triple");
    let mut config = LinkConfig::new(out_path.clone(), link_target);
    config.dynamic = true;
    adesh_linker::Linker::link(&[adob_path], config).expect("native dynamic link");
    assert!(out_path.exists(), "linked binary must exist");
    out_path
}

const TLS_SOURCE: &str = r#"
        thread_local let counter = 42;
        fn main(): int {
            return counter;
        }
"#;

#[test]
fn phase18_thread_local_retained_in_hir_and_tls_sections() {
    let triple = "x86_64-unknown-linux-gnu";
    let module = lower_module_for(TLS_SOURCE, triple);
    assert!(
        module
            .tls_sections
            .iter()
            .any(|(name, bytes)| name.contains("counter") && bytes == &42i64.to_le_bytes()),
        "tls_sections must bake counter=42: {:?}",
        module.tls_sections
    );
    let has_tls_addr = module.functions.iter().any(|f| {
        f.blocks.iter().any(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i, MachineInstruction::TlsAddress { .. }))
        })
    });
    assert!(has_tls_addr, "loading a thread_local must emit TlsAddress");
}

#[test]
fn phase18_thread_local_exits_42_on_linux_x86_64() {
    if !require_or_skip(
        cfg!(all(target_os = "linux", target_arch = "x86_64")),
        "needs Linux x86-64 host",
    ) {
        return;
    }
    let out_path = compile_and_link(TLS_SOURCE, "phase18_tls_exit42", "x86_64-unknown-linux-gnu");
    chmod_executable(&out_path);
    let out = Command::new(&out_path)
        .output()
        .expect("execute linked TLS binary");
    assert_eq!(
        out.status.code(),
        Some(42),
        "thread_local let counter=42 must exit 42 (stderr: {:?})",
        String::from_utf8_lossy(&out.stderr)
    );
}
