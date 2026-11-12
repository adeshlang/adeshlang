//! Adesh source -> native codegen -> target runtime archive -> ELF -> QEMU.
//! Requires a staged AArch64 runtime, not a host-architecture fallback.
#[path = "../linker/tests/support/foreign_exec.rs"]
mod foreign_exec;

use foreign_exec::{LinuxTools, run};
use std::{ffi::OsStr, path::Path, process::Command};

fn source_case(name: &str, expected_stdout: &[u8], expected_exit: i32) {
    let Some(tools) = LinuxTools::discover("aarch64-linux-gnu-as", "qemu-aarch64") else {
        return;
    };
    // The real CLI defaults to dynamic ELF, so execute against target libc
    // rather than silently substituting a freestanding linker-only build.
    let sysroot = std::env::var("ADESH_AARCH64_SYSROOT")
        .unwrap_or_else(|_| "/usr/aarch64-linux-gnu".to_owned());
    let loader = format!("{sysroot}/lib/ld-linux-aarch64.so.1");
    let present = tools.tool("test", &[OsStr::new("-r"), OsStr::new(&loader)]);
    if !present.status.success() {
        assert!(
            std::env::var_os("ADESH_REQUIRE_FOREIGN_EXEC").is_none(),
            "required AArch64 loader missing: {loader}"
        );
        eprintln!("SKIP: install libc6-arm64-cross or set ADESH_AARCH64_SYSROOT ({loader})");
        return;
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = repo.join(format!("tests/fixtures/phase13_aarch64/{name}.adesh"));
    for fast in [false, true] {
        let dir = tempfile::Builder::new()
            .prefix("adesh source arm64 ")
            .tempdir()
            .unwrap();
        let output = dir.path().join("app.elf");
        let mut command = Command::new(env!("CARGO_BIN_EXE_adesh"));
        command
            .current_dir(repo)
            .arg("build")
            .arg(&source)
            .args(["--target", "aarch64-unknown-linux-gnu", "--emit=exe", "-o"])
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
                    "SKIP: AArch64 source execution needs its runtime archive; run \
                 bash scripts/build-runtime-archives.sh --targets aarch64-unknown-linux-gnu"
                );
                return;
            }
            panic!("{name}: native source build failed: {diagnostics}");
        }
        let bytes =
            std::fs::read(&output).expect("successful build must write its requested output");
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
            183,
            "AArch64"
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
        assert_eq!(interp, Some(b"/lib/ld-linux-aarch64.so.1\0".as_slice()));
        let output_linux = tools.path(&output);
        let executed = tools.tool(
            "qemu-aarch64",
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
    source_case("hello", b"phase13 aarch64 source\n", 0);
}

#[test]
fn main_return_becomes_process_exit_status() {
    source_case("exit", b"", 42);
}

#[test]
fn arithmetic_and_nested_calls_execute() {
    source_case("functions", b"", 50);
}

#[test]
fn loop_and_comparison_execute() {
    source_case("loop", b"", 55);
}
