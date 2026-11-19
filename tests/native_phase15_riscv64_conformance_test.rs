//! Phase 15 RISC-V 64 language subset conformance test suite.
//! Tests compilation from Adesh source to native RISC-V 64 ELF executable and runs
//! under QEMU to assert byte-exact standard output and exit status 0 across both
//! debug and fast codegen profiles.
//! Also asserts differential parity against x86-64 native execution for the full corpus.

#[path = "../linker/tests/support/foreign_exec.rs"]
mod foreign_exec;

use foreign_exec::{LinuxTools, run};
use std::{ffi::OsStr, path::Path, process::Command};

const TRIPLE: &str = "riscv64gc-unknown-linux-gnu";

fn conformance_case(name: &str, expected_stdout: &[u8], expected_exit: i32) {
    let Some(tools) = LinuxTools::discover("riscv64-linux-gnu-as", "qemu-riscv64") else {
        eprintln!("SKIP: Linux foreign-exec tools (qemu-riscv64 / riscv64-linux-gnu-as) not found");
        return;
    };

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
        return;
    }

    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = repo.join(format!("tests/fixtures/phase14_aarch64/{name}.adesh"));

    for fast in [false, true] {
        let dir = tempfile::Builder::new()
            .prefix("adesh_phase15_riscv64_")
            .tempdir()
            .unwrap();
        let output = dir.path().join(format!("{name}.elf"));

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
                    "SKIP: RISC-V source execution needs target runtime archive; run \
                     bash scripts/build-runtime-archives.sh --targets riscv64gc-unknown-linux-gnu"
                );
                return;
            }
            panic!("{name}: native RISC-V 64 source build failed (fast={fast}): {diagnostics}");
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
            243,
            "EM_RISCV"
        );

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
fn phase15_riscv64_arithmetic_conformance() {
    conformance_case("arithmetic", b"70\n30\n3\n", 0);
}

#[test]
fn phase15_riscv64_branching_conformance() {
    conformance_case("branching", b"x is greater\nvalid branch\n", 0);
}

#[test]
fn phase15_riscv64_loops_conformance() {
    conformance_case("loops", b"55\n", 0);
}

#[test]
fn phase15_riscv64_functions_conformance() {
    conformance_case("functions", b"21\n60\n", 0);
}

#[test]
fn phase15_riscv64_arrays_conformance() {
    conformance_case("arrays", b"100\n200\n300\n", 0);
}

#[test]
fn phase15_differential_parity_x86_64_vs_riscv64() {
    let Some(tools) = LinuxTools::discover("riscv64-linux-gnu-as", "qemu-riscv64") else {
        eprintln!("SKIP: Linux foreign-exec tools not found");
        return;
    };

    let sysroot = std::env::var("ADESH_RISCV64_SYSROOT")
        .unwrap_or_else(|_| "/usr/riscv64-linux-gnu".to_owned());
    let loader = format!("{sysroot}/lib/ld-linux-riscv64-lp64d.so.1");
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
            .prefix("adesh_diff_riscv_")
            .tempdir()
            .unwrap();
        let out_riscv = dir.path().join(format!("{case}_riscv.elf"));
        let out_x86 = dir.path().join(format!("{case}_x86.elf"));

        // Build RISC-V 64
        let mut riscv_cmd = Command::new(env!("CARGO_BIN_EXE_adesh"));
        riscv_cmd
            .current_dir(repo)
            .arg("build")
            .arg(&source)
            .args(["--target", TRIPLE, "--emit=exe", "-o"])
            .arg(&out_riscv);
        let riscv_compiled = run(&mut riscv_cmd);
        assert!(
            riscv_compiled.status.success(),
            "RISC-V 64 compile failed for {case}: {:?}",
            String::from_utf8_lossy(&riscv_compiled.stderr)
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

        // Run RISC-V 64 under QEMU
        let riscv_linux = tools.path(&out_riscv);
        let riscv_exec = tools.tool(
            "qemu-riscv64",
            &[
                OsStr::new("-L"),
                OsStr::new(&sysroot),
                OsStr::new(&riscv_linux),
            ],
        );

        // Run x86_64 natively under Linux (via tools)
        let x86_linux = tools.path(&out_x86);
        let x86_exec = tools.tool(&x86_linux, &[]);

        assert_eq!(
            riscv_exec.status.code(),
            x86_exec.status.code(),
            "Exit code mismatch between x86_64 and RISC-V 64 for {case}"
        );
        assert_eq!(
            riscv_exec.stdout, x86_exec.stdout,
            "stdout mismatch between x86_64 and RISC-V 64 for {case}"
        );
    }
}
