//! Adesh source -> IA-32 codegen -> static ELF32 -> qemu-i386.
#[path = "../linker/tests/support/foreign_exec.rs"]
mod foreign_exec;

use foreign_exec::{LinuxTools, run};
use std::{ffi::OsStr, path::Path, process::Command};

const TRIPLE: &str = "i686-unknown-linux-gnu";

fn source_case(name: &str, expected_stdout: &[u8], expected_exit: i32) {
    let Some(tools) = LinuxTools::discover("as", "qemu-i386") else {
        return;
    };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = repo.join(format!("tests/fixtures/phase22_i686/{name}.adesh"));
    for fast in [false, true] {
        let dir = tempfile::Builder::new()
            .prefix("adesh source i686 ")
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
        let diagnostics = format!(
            "{}\n{}",
            String::from_utf8_lossy(&compiled.stdout),
            String::from_utf8_lossy(&compiled.stderr)
        );
        if diagnostics.contains("No runtime archive for target")
            && std::env::var_os("ADESH_REQUIRE_FOREIGN_EXEC").is_none()
        {
            eprintln!(
                "SKIP: i686 source execution needs its runtime archive; run \
                 bash scripts/build-runtime-archives.sh --targets {TRIPLE}"
            );
            return;
        }
        assert!(
            compiled.status.success(),
            "{name}, fast={fast}: {diagnostics}"
        );

        let bytes = std::fs::read(&output).expect("successful build writes an executable");
        assert_eq!(&bytes[..4], b"\x7fELF");
        assert_eq!(bytes[4], 1, "ELF32");
        assert_eq!(bytes[5], 1, "little endian");
        assert_eq!(u16::from_le_bytes(bytes[16..18].try_into().unwrap()), 2);
        assert_eq!(u16::from_le_bytes(bytes[18..20].try_into().unwrap()), 3);

        let executed = tools.tool("qemu-i386", &[OsStr::new(&tools.path(&output))]);
        assert_eq!(
            executed.status.code(),
            Some(expected_exit),
            "{name}, fast={fast}: {executed:?}"
        );
        assert_eq!(
            executed.stdout, expected_stdout,
            "{name}, fast={fast}: {executed:?}"
        );
        assert!(
            executed.stderr.is_empty(),
            "{name}, fast={fast}: {executed:?}"
        );
    }
}

#[test]
fn main_return_becomes_process_exit_status() {
    source_case("exit", b"", 42);
}

#[test]
fn string_output_executes() {
    source_case("hello", b"hello i686\n", 0);
}

#[test]
fn local_arithmetic_and_loop_execute() {
    source_case("loop", b"", 3);
}

#[test]
fn parameters_and_calls_execute() {
    source_case("functions", b"", 50);
}

#[test]
fn integer_operators_execute() {
    source_case("arithmetic", b"", 46);
}

#[test]
fn negative_narrow_and_int_values_print_signed() {
    source_case("negative_print", b"-160\n-42\n-7\n", 0);
}

#[test]
fn register_pressure_and_six_stack_arguments_execute() {
    source_case("pressure", b"", 112);
}

#[test]
fn x87_double_arithmetic_comparisons_and_printing_execute() {
    source_case(
        "fp_arithmetic",
        b"0.30000000000000004\n25\n2.5\n-0.1\n7.5\n15\ngt\nlt\nne\n0.3333333333333333\n10.5\n9\n",
        0,
    );
}

#[test]
fn x87_calls_returns_globals_and_conversions_execute() {
    source_case(
        "fp_calls_and_conversions",
        b"5\n5\n3628800\ninf\n-inf\nNaN\nnan ne\nnan !lt\n-7\n3000000000\n3000000\n1 2.5\n2475\n",
        7,
    );
}

#[test]
fn static_tls_and_writable_globals_next_to_strings_execute() {
    source_case("tls_static", b"tls\n45\n2\n21\n103\n", 5);
}

#[test]
fn sixty_four_bit_runtime_helper_parameters_are_marshalled_on_i686() {
    source_case(
        "helper_marshalling",
        b"s 7 200 true\ntrue\nx 5 -3\n7\n",
        0,
    );
}

#[test]
fn f32_is_refused_on_i686() {
    let Some(_tools) = LinuxTools::discover("as", "qemu-i386") else {
        return;
    };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = tempfile::Builder::new()
        .prefix("adesh i686 f32 ")
        .tempdir()
        .unwrap();
    let output = dir.path().join("app.elf");
    let mut command = Command::new(env!("CARGO_BIN_EXE_adesh"));
    command
        .current_dir(repo)
        .arg("build")
        .arg("tests/fixtures/phase22_i686/f32_refused.adesh")
        .args(["--target", TRIPLE, "--emit=exe", "-o"])
        .arg(&output);
    let compiled = run(&mut command);
    assert!(!compiled.status.success());
    let diagnostics = format!(
        "{}\n{}",
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert!(diagnostics.contains("f32"), "{diagnostics}");
}

#[test]
fn genuine_64_bit_integer_is_refused() {
    let Some(_tools) = LinuxTools::discover("as", "qemu-i386") else {
        return;
    };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = tempfile::Builder::new()
        .prefix("adesh i686 wide ")
        .tempdir()
        .unwrap()
        .path()
        .join("app.elf");
    let mut command = Command::new(env!("CARGO_BIN_EXE_adesh"));
    command
        .current_dir(repo)
        .arg("build")
        .arg("tests/fixtures/phase22_i686/wide_refused.adesh")
        .args(["--target", TRIPLE, "--emit=exe", "-o"])
        .arg(&output);
    let compiled = run(&mut command);
    assert!(!compiled.status.success());
    let diagnostics = format!(
        "{}\n{}",
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert!(diagnostics.contains("64-bit integer"), "{diagnostics}");
}
