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
        assert!(compiled.status.success(), "{name}, fast={fast}: {diagnostics}");

        let bytes = std::fs::read(&output).expect("successful build writes an executable");
        assert_eq!(&bytes[..4], b"\x7fELF");
        assert_eq!(bytes[4], 1, "ELF32");
        assert_eq!(bytes[5], 1, "little endian");
        assert_eq!(u16::from_le_bytes(bytes[16..18].try_into().unwrap()), 2);
        assert_eq!(u16::from_le_bytes(bytes[18..20].try_into().unwrap()), 3);

        let executed = tools.tool(
            "qemu-i386",
            &[OsStr::new(&tools.path(&output))],
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
        assert!(executed.stderr.is_empty(), "{name}, fast={fast}: {executed:?}");
    }
}

#[test]
fn main_return_becomes_process_exit_status() {
    source_case("exit", b"", 42);
}
