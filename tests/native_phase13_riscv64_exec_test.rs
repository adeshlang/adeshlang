//! Adesh source -> native codegen -> target runtime archive -> ELF -> QEMU.
//! Requires a staged RISC-V runtime, not a host-architecture fallback.
//!
//! Hello-world and a bare integer return execute today. Programs that spill to
//! stack slots (nested-call locals, loops) are refused loudly by the RV64
//! backend until Phase 15 implements register<->StackSlot moves; that refusal is
//! pinned here so a silent miscompile cannot slip in behind this suite.
#[path = "../linker/tests/support/foreign_exec.rs"]
mod foreign_exec;

use foreign_exec::{LinuxTools, run};
use std::{ffi::OsStr, path::Path, process::Command};

const TRIPLE: &str = "riscv64gc-unknown-linux-gnu";

fn tools_and_sysroot() -> Option<(LinuxTools, String)> {
    let tools = LinuxTools::discover("riscv64-linux-gnu-as", "qemu-riscv64")?;
    let sysroot = std::env::var("ADESH_RISCV64_SYSROOT")
        .unwrap_or_else(|_| "/usr/riscv64-linux-gnu".to_owned());
    let loader = format!("{sysroot}/lib/ld-linux-riscv64-lp64d.so.1");
    let present = tools.tool("test", &[OsStr::new("-r"), OsStr::new(&loader)]);
    if !present.status.success() {
        assert!(
            std::env::var_os("ADESH_REQUIRE_FOREIGN_EXEC").is_none(),
            "required RISC-V loader missing: {loader}"
        );
        eprintln!("SKIP: install libc6-riscv64-cross or set ADESH_RISCV64_SYSROOT ({loader})");
        return None;
    }
    Some((tools, sysroot))
}

/// Build a fixture through the real CLI in one mode, returning the diagnostics,
/// the output path and the owning tempdir (the caller must keep it alive until
/// the image has been read). A missing archive is a loud local SKIP, not a panic.
fn build_fixture(
    name: &str,
    fast: bool,
) -> Option<(std::process::Output, std::path::PathBuf, tempfile::TempDir)> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = repo.join(format!("tests/fixtures/phase13_riscv64/{name}.adesh"));
    let dir = tempfile::Builder::new()
        .prefix("adesh source riscv64 ")
        .tempdir()
        .unwrap();
    let output = dir.path().join("app.elf");
    let mut command = Command::new(env!("CARGO_BIN_EXE_adesh"));
    command
        .current_dir(repo)
        .arg("build")
        .arg(&source)
        .args(["--target", TRIPLE, "--emit=exe", "-o"])
        .arg(&output);
    if fast {
        command.arg("--fast");
    }
    let compiled = run(&mut command);
    if !compiled.status.success() {
        let diagnostics = format!(
            "{}\n{}",
            String::from_utf8_lossy(&compiled.stdout),
            String::from_utf8_lossy(&compiled.stderr)
        );
        if diagnostics.contains("No runtime archive for target")
            && std::env::var_os("ADESH_REQUIRE_FOREIGN_EXEC").is_none()
        {
            eprintln!(
                "SKIP: RISC-V source execution needs its runtime archive; run \
                 bash scripts/build-runtime-archives.sh --targets {TRIPLE}"
            );
            return None;
        }
    }
    Some((compiled, output, dir))
}

fn assert_riscv_elf(bytes: &[u8]) {
    assert_eq!(&bytes[..4], b"\x7fELF");
    assert_eq!(bytes[4], 2, "ELF64");
    assert_eq!(bytes[5], 1, "little endian");
    assert_eq!(
        u16::from_le_bytes(bytes[16..18].try_into().unwrap()),
        2,
        "ET_EXEC"
    );
    assert_eq!(
        u16::from_le_bytes(bytes[18..20].try_into().unwrap()),
        243,
        "RISC-V"
    );
    let phoff = u64::from_le_bytes(bytes[32..40].try_into().unwrap()) as usize;
    let phsize = u16::from_le_bytes(bytes[54..56].try_into().unwrap()) as usize;
    let phcount = u16::from_le_bytes(bytes[56..58].try_into().unwrap()) as usize;
    let interp = (0..phcount).find_map(|i| {
        let ph = phoff + i * phsize;
        if u32::from_le_bytes(bytes[ph..ph + 4].try_into().unwrap()) != 3 {
            return None;
        }
        let offset = u64::from_le_bytes(bytes[ph + 8..ph + 16].try_into().unwrap()) as usize;
        let size = u64::from_le_bytes(bytes[ph + 32..ph + 40].try_into().unwrap()) as usize;
        Some(&bytes[offset..offset + size])
    });
    assert_eq!(
        interp,
        Some(b"/lib/ld-linux-riscv64-lp64d.so.1\0".as_slice())
    );
}

fn source_case(name: &str, expected_stdout: &[u8], expected_exit: i32) {
    let Some((tools, sysroot)) = tools_and_sysroot() else {
        return;
    };
    for fast in [false, true] {
        let Some((compiled, output, _dir)) = build_fixture(name, fast) else {
            return;
        };
        assert!(
            compiled.status.success(),
            "{name}, fast={fast}: native source build failed: {}\n{}",
            String::from_utf8_lossy(&compiled.stdout),
            String::from_utf8_lossy(&compiled.stderr)
        );
        let bytes =
            std::fs::read(&output).expect("successful build must write its requested output");
        assert_riscv_elf(&bytes);
        let output_linux = tools.path(&output);
        let executed = tools.tool(
            "qemu-riscv64",
            &[
                OsStr::new("-L"),
                OsStr::new(&sysroot),
                OsStr::new(&output_linux),
            ],
        );
        assert_eq!(
            executed.status.code(),
            Some(expected_exit),
            "{name}, fast={fast}: {executed:?}"
        );
        assert_eq!(
            executed.stdout, expected_stdout,
            "{name}, fast={fast}: {executed:?}"
        );
        assert!(executed.stderr.is_empty(), "{name}: {executed:?}");
    }
}

#[test]
fn hello_uses_target_runtime_and_prints() {
    source_case("hello", b"phase13 riscv64 source\n", 0);
}

#[test]
fn main_return_becomes_process_exit_status() {
    source_case("exit", b"", 42);
}

/// Programs that need stack slots must fail loudly, not silently miscompile.
/// The RV64 backend has no register<->StackSlot move form yet (Phase 15); until
/// then a build of these fixtures must refuse with a codegen error naming the
/// StackSlot operand and must write no image. When Phase 15 lands the stack
/// moves, flip these into `source_case` executions (functions -> 50, loop -> 55).
#[test]
fn stack_slot_programs_are_refused_loudly_until_phase15() {
    if tools_and_sysroot().is_none() {
        return;
    }
    for name in ["functions", "loop"] {
        for fast in [false, true] {
            let Some((compiled, output, _dir)) = build_fixture(name, fast) else {
                return;
            };
            let diagnostics = format!(
                "{}\n{}",
                String::from_utf8_lossy(&compiled.stdout),
                String::from_utf8_lossy(&compiled.stderr)
            );
            assert!(
                !compiled.status.success(),
                "{name}, fast={fast}: RV64 stack-slot program built; Phase 15 lowering may have \
                 landed — flip this pin into an execution case:\n{diagnostics}"
            );
            assert!(
                diagnostics.contains("StackSlot")
                    && diagnostics.contains("unsupported machine instruction"),
                "{name}, fast={fast}: expected a loud StackSlot codegen refusal, got:\n{diagnostics}"
            );
            assert!(
                !output.exists(),
                "{name}, fast={fast}: a refused build must not write an image"
            );
        }
    }
}