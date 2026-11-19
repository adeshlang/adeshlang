//! Phase 14 AArch64 language subset conformance test suite.
//! Tests compilation from Adesh source to native AArch64 ELF executable and runs
//! under QEMU to assert byte-exact standard output and exit status 0 across both
//! debug and fast codegen profiles.
//! Also asserts differential parity against x86-64 native execution for the full corpus.

#[path = "../linker/tests/support/foreign_exec.rs"]
mod foreign_exec;

use foreign_exec::{run, LinuxTools};
use std::{ffi::OsStr, path::Path, process::Command};

fn conformance_case(name: &str, expected_stdout: &[u8], expected_exit: i32) {
    let Some(tools) = LinuxTools::discover("aarch64-linux-gnu-as", "qemu-aarch64") else {
        eprintln!("SKIP: Linux foreign-exec tools (qemu-aarch64 / aarch64-linux-gnu-as) not found");
        return;
    };

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
    let source = repo.join(format!("tests/fixtures/phase14_aarch64/{name}.adesh"));

    for fast in [false, true] {
        let dir = tempfile::Builder::new()
            .prefix("adesh_phase14_arm64_")
            .tempdir()
            .unwrap();
        let output = dir.path().join(format!("{name}.elf"));

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
                    "SKIP: AArch64 source execution needs target runtime archive; run \
                     bash scripts/build-runtime-archives.sh --targets aarch64-unknown-linux-gnu"
                );
                return;
            }
            panic!("{name}: native AArch64 source build failed (fast={fast}): {diagnostics}");
        }

        let bytes =
            std::fs::read(&output).expect("successful build must write its requested output");
        assert_eq!(&bytes[..4], b"\x7fELF", "{name}: magic check");
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
            "EM_AARCH64"
        );

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
            "{name}, fast={fast}: unexpected exit code: {executed:?}"
        );
        assert_eq!(
            executed.stdout, expected_stdout,
            "{name}, fast={fast}: stdout mismatch: {executed:?}"
        );
        assert!(
            executed.stderr.is_empty(),
            "{name}, fast={fast}: expected empty stderr: {executed:?}"
        );
    }
}

#[test]
fn phase14_aarch64_arithmetic_conformance() {
    conformance_case("arithmetic", b"70\n30\n3\n", 0);
}

#[test]
fn phase14_aarch64_branching_conformance() {
    conformance_case("branching", b"x is greater\nvalid branch\n", 0);
}

#[test]
fn phase14_aarch64_loops_conformance() {
    conformance_case("loops", b"55\n", 0);
}

#[test]
fn phase14_aarch64_functions_conformance() {
    conformance_case("functions", b"21\n60\n", 0);
}

#[test]
fn phase14_aarch64_arrays_conformance() {
    conformance_case("arrays", b"100\n200\n300\n", 0);
}

#[test]
fn phase14_differential_parity_x86_64_vs_aarch64() {
    let Some(tools) = LinuxTools::discover("aarch64-linux-gnu-as", "qemu-aarch64") else {
        eprintln!("SKIP: Linux foreign-exec tools not found");
        return;
    };

    let sysroot = std::env::var("ADESH_AARCH64_SYSROOT")
        .unwrap_or_else(|_| "/usr/aarch64-linux-gnu".to_owned());
    let loader = format!("{sysroot}/lib/ld-linux-aarch64.so.1");
    let present = tools.tool("test", &[OsStr::new("-r"), OsStr::new(&loader)]);
    if !present.status.success() {
        eprintln!("SKIP: loader missing: {loader}");
        return;
    }

    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let cases = ["arithmetic", "branching", "loops", "functions", "arrays"];

    for case in cases {
        let source = repo.join(format!("tests/fixtures/phase14_aarch64/{case}.adesh"));

        let dir = tempfile::Builder::new()
            .prefix("adesh_diff_")
            .tempdir()
            .unwrap();
        let out_arm = dir.path().join(format!("{case}_arm.elf"));
        let out_x86 = dir.path().join(format!("{case}_x86.elf"));

        // Build AArch64
        let mut arm_cmd = Command::new(env!("CARGO_BIN_EXE_adesh"));
        arm_cmd
            .current_dir(repo)
            .arg("build")
            .arg(&source)
            .args(["--target", "aarch64-unknown-linux-gnu", "--emit=exe", "-o"])
            .arg(&out_arm);
        let arm_compiled = run(&mut arm_cmd);
        assert!(
            arm_compiled.status.success(),
            "AArch64 compile failed for {case}: {:?}",
            String::from_utf8_lossy(&arm_compiled.stderr)
        );

        // Build x86_64
        let mut x86_cmd = Command::new(env!("CARGO_BIN_EXE_adesh"));
        x86_cmd
            .current_dir(repo)
            .arg("build")
            .arg(&source)
            .args(["--target", "x86_64-unknown-linux-gnu", "--emit=exe", "-o"])
            .arg(&out_x86);
        let x86_compiled = run(&mut x86_cmd);
        assert!(
            x86_compiled.status.success(),
            "x86_64 compile failed for {case}: {:?}",
            String::from_utf8_lossy(&x86_compiled.stderr)
        );

        // Run AArch64 under QEMU
        let arm_linux = tools.path(&out_arm);
        let arm_exec = tools.tool(
            "qemu-aarch64",
            &[
                OsStr::new("-L"),
                OsStr::new(&sysroot),
                OsStr::new(&arm_linux),
            ],
        );

        // Run x86_64 natively under Linux (via tools)
        let x86_linux = tools.path(&out_x86);
        let x86_exec = tools.tool(&x86_linux, &[]);

        assert_eq!(
            arm_exec.status.code(),
            x86_exec.status.code(),
            "Exit code mismatch between x86_64 and AArch64 for {case}"
        );
        assert_eq!(
            arm_exec.stdout, x86_exec.stdout,
            "stdout mismatch between x86_64 and AArch64 for {case}"
        );
    }
}
