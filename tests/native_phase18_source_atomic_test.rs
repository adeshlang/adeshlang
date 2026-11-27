//! Phase 18 — Source-level `atomic let` + `atomic_fetch_add`.
//!
//! An Adesh source file declares `atomic let counter = 20;`, calls
//! `atomic_fetch_add(counter, 22)`, and exits 42 on Linux x86-64 via
//! MachineIR `AtomicFetchAdd` (`lock xadd`).

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
        "required Phase 18 source-atomic execution: {why} (host is {}/{})",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    eprintln!(
        "SKIP: Phase 18 source-atomic execution: {why}; host is {}/{}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    false
}

fn chmod_executable(path: &Path) {
    #[cfg(not(unix))]
    let _ = path;
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
        hir.functions.iter().any(|f| {
            f.body.iter().any(|s| {
                matches!(
                    s,
                    adeshlang::parsing::hir::HirStmt::Let {
                        is_atomic: true,
                        ..
                    }
                )
            })
        }),
        "Phase 18: HIR must retain atomic let"
    );
    let target = TargetDescriptor::from_triple(triple).expect("valid triple");
    lower_hir_module(&hir, &target).expect("native lowering")
}

fn qemu_available(emulator: &str) -> bool {
    Command::new(emulator)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn execute_under_qemu(
    path: &Path,
    emulator: &str,
    sysroot: &str,
) -> std::io::Result<std::process::Output> {
    chmod_executable(path);
    Command::new(emulator)
        .args(["-L", sysroot])
        .arg(path)
        .output()
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
        .join("phase18_atomic_tests");
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

    let host = Target::host();
    let is_host = Target::from_triple(triple)
        .map(|t| {
            t.arch == host.arch
                && t.os == host.os
                && t.abi == host.abi
                && t.libc_flavor == host.libc_flavor
        })
        .unwrap_or(false);
    let mut inputs = vec![adob_path];
    if !is_host && triple.contains("linux") {
        let profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        let runtime = std::env::current_dir()
            .unwrap()
            .join("target")
            .join(triple)
            .join(profile)
            .join("libadesh_runtime.a");
        if !runtime.is_file() {
            panic!(
                "cross link for {triple} needs {}; run \
                 bash scripts/build-runtime-archives.sh --targets {triple}",
                runtime.display()
            );
        }
        inputs.push(runtime);
    }

    adesh_linker::Linker::link(&inputs, config).expect("native dynamic link");
    assert!(out_path.exists(), "linked binary must exist");
    out_path
}

const ATOMIC_SOURCE: &str = r#"
        fn main(): int {
            atomic let counter = 20;
            atomic_fetch_add(counter, 22);
            return counter;
        }
"#;

#[test]
fn phase18_atomic_let_emits_atomic_fetch_add() {
    let triple = "x86_64-unknown-linux-gnu";
    let module = lower_module_for(ATOMIC_SOURCE, triple);
    let has_xadd = module.functions.iter().any(|f| {
        f.blocks.iter().any(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i, MachineInstruction::AtomicFetchAdd { .. }))
        })
    });
    assert!(
        has_xadd,
        "atomic_fetch_add must lower to MachineInstruction::AtomicFetchAdd"
    );
}

#[test]
fn phase18_atomic_fetch_add_exits_42_on_linux_x86_64() {
    if !require_or_skip(
        cfg!(all(target_os = "linux", target_arch = "x86_64")),
        "needs Linux x86-64 host",
    ) {
        return;
    }
    let out_path = compile_and_link(
        ATOMIC_SOURCE,
        "phase18_atomic_exit42",
        "x86_64-unknown-linux-gnu",
    );
    chmod_executable(&out_path);
    let out = Command::new(&out_path)
        .output()
        .expect("execute linked atomic binary");
    assert_eq!(
        out.status.code(),
        Some(42),
        "atomic_fetch_add(20, 22) must exit 42 (stderr: {:?})",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn phase18_atomic_fetch_add_exits_42_on_linux_aarch64_qemu() {
    if !require_or_skip(cfg!(target_os = "linux"), "needs Linux host for qemu-user") {
        return;
    }
    let sysroot = std::env::var("ADESH_AARCH64_SYSROOT")
        .unwrap_or_else(|_| "/usr/aarch64-linux-gnu".to_owned());
    let loader = format!("{sysroot}/lib/ld-linux-aarch64.so.1");
    if !require_or_skip(
        Path::new(&loader).is_file() && qemu_available("qemu-aarch64"),
        "needs qemu-aarch64 + aarch64 libc sysroot (libc6-dev-arm64-cross)",
    ) {
        return;
    }
    let out_path = compile_and_link(
        ATOMIC_SOURCE,
        "phase18_atomic_exit42_aarch64",
        "aarch64-unknown-linux-gnu",
    );
    let out = execute_under_qemu(&out_path, "qemu-aarch64", &sysroot)
        .expect("qemu execute atomic binary");
    assert_eq!(
        out.status.code(),
        Some(42),
        "aarch64 atomic_fetch_add(20, 22) must exit 42 under qemu (stderr: {:?})",
        String::from_utf8_lossy(&out.stderr)
    );
}
