//! Phase 18 — Source-level `thread_local let` reachability.
//!
//! An Adesh source file declares `thread_local let counter = 42;`, native
//! lowering places it in `.tls` with `SymbolKind::TLS`, loads go through
//! `TlsAddress` (SysV IE / Win TEB / Mach-O TLV), and a host-appropriate
//! link exits 42.

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
        !hir.tls_vars.is_empty(),
        "Phase 18: HIR must retain thread_local declarations"
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

fn macos_host_triple() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        _ => None,
    }
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

    let ext = if triple.contains("windows") {
        "exe"
    } else {
        "bin"
    };
    let out_path = test_dir.join(format!("{unique_name}.{ext}"));
    let link_target = Target::from_triple(triple).expect("triple");
    let mut config = LinkConfig::new(out_path.clone(), link_target);
    // ELF needs a dynamic link for IE GOT slots; Mach-O is always dyld-bound;
    // PE TLS directory synthesis does not require `dynamic`.
    config.dynamic = triple.contains("linux");

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

    adesh_linker::Linker::link(&inputs, config).expect("native link");
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

#[test]
fn phase18_thread_local_exits_42_on_linux_aarch64_qemu() {
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
        TLS_SOURCE,
        "phase18_tls_exit42_aarch64",
        "aarch64-unknown-linux-gnu",
    );
    let out =
        execute_under_qemu(&out_path, "qemu-aarch64", &sysroot).expect("qemu execute TLS binary");
    assert_eq!(
        out.status.code(),
        Some(42),
        "aarch64 thread_local let counter=42 must exit 42 under qemu (stderr: {:?})",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn phase18_thread_local_emits_win64_teb_sequence() {
    let triple = "x86_64-pc-windows-msvc";
    let module = lower_module_for(TLS_SOURCE, triple);
    assert!(
        module
            .tls_sections
            .iter()
            .any(|(name, _)| name.contains("counter")),
        "windows triple must retain TLS sections"
    );
    let target = TargetDescriptor::from_triple(triple).expect("triple");
    let mut backend = create_backend(target).expect("backend");
    let lowered = backend.lower_module(&module).expect("lower");
    let obj = backend
        .emit_object(&lowered)
        .expect("Win64 TEB TLS must emit");
    AdobValidator::validate(&obj).expect("validate");
    assert!(
        obj.symbols.iter().any(|s| s.name == "_tls_index"),
        "PE TLS needs undefined _tls_index; got {:?}",
        obj.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    assert!(
        obj.symbols
            .iter()
            .any(|s| s.name.contains("counter") && matches!(s.kind, adesh_object::SymbolKind::TLS)),
        "counter must be SymbolKind::TLS"
    );
}

#[test]
fn phase18_thread_local_exits_42_on_windows_x86_64() {
    if !require_or_skip(
        cfg!(all(target_os = "windows", target_arch = "x86_64")),
        "needs Windows x86-64 host",
    ) {
        return;
    }
    let out_path = compile_and_link(
        TLS_SOURCE,
        "phase18_tls_exit42_win",
        "x86_64-pc-windows-msvc",
    );
    let out = Command::new(&out_path)
        .output()
        .expect("execute Windows TLS binary");
    assert_eq!(
        out.status.code(),
        Some(42),
        "Windows thread_local let counter=42 must exit 42 (stderr: {:?})",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn phase18_thread_local_emits_macho_tlv_descriptors() {
    for triple in ["x86_64-apple-darwin", "aarch64-apple-darwin"] {
        let module = lower_module_for(TLS_SOURCE, triple);
        let target = TargetDescriptor::from_triple(triple).expect("triple");
        let mut backend = create_backend(target).expect("backend");
        let lowered = backend.lower_module(&module).expect("lower");
        let obj = backend
            .emit_object(&lowered)
            .unwrap_or_else(|e| panic!("Darwin TLV emit failed for {triple}: {e}"));
        AdobValidator::validate(&obj).expect("validate");
        assert!(
            obj.sections.iter().any(|s| {
                s.name == "__thread_vars" && (s.flags & adesh_object::section_flags::TLV_TABLE) != 0
            }),
            "{triple}: must emit __thread_vars with TLV_TABLE; sections={:?}",
            obj.sections.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        assert!(
            obj.symbols
                .iter()
                .any(|s| s.name == "__adesh_tls_counter$tlv"),
            "{triple}: missing __adesh_tls_counter$tlv descriptor; got {:?}",
            obj.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        assert!(
            obj.symbols.iter().any(|s| s.name == "__tlv_bootstrap")
                || obj.imports.iter().any(|s| s == "__tlv_bootstrap"),
            "{triple}: missing __tlv_bootstrap import"
        );
        let has_tlv_reloc = obj.sections.iter().any(|sec| {
            sec.relocations.iter().any(|r| {
                matches!(
                    r.kind,
                    adesh_object::RelocationKind::X86_64_TlvDescriptorRel32
                        | adesh_object::RelocationKind::AArch64_TlvDescriptorLo12
                )
            })
        });
        assert!(
            has_tlv_reloc,
            "{triple}: text must carry a TLV descriptor reloc"
        );
    }
}

#[test]
fn phase18_thread_local_exits_42_on_macos_host() {
    let Some(triple) = macos_host_triple() else {
        let _ = require_or_skip(false, "needs macOS host for dyld TLV");
        return;
    };
    let out_path = compile_and_link(TLS_SOURCE, "phase18_tls_exit42_macos", triple);
    chmod_executable(&out_path);
    let out = Command::new(&out_path)
        .output()
        .expect("execute macOS TLV binary");
    assert_eq!(
        out.status.code(),
        Some(42),
        "macOS thread_local let counter=42 must exit 42 via TLV (stderr: {:?})",
        String::from_utf8_lossy(&out.stderr)
    );
}
