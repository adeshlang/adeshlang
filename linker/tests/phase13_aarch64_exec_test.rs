//! Real ELF input -> adeshlink CLI -> synthesized _start -> QEMU.
//! This proves linker startup, not source-language/codegen conformance.
#[path = "support/foreign_exec.rs"]
mod foreign_exec;

use foreign_exec::{LinuxTools, run};
use std::{ffi::OsStr, path::Path, process::Command};

#[test]
fn aarch64_static_startup_arguments_environment_and_exit() {
    let Some(tools) = LinuxTools::discover("aarch64-linux-gnu-as", "qemu-aarch64") else {
        return;
    };
    // A path containing spaces also exercises direct argument passing in WSL.
    let dir = tempfile::Builder::new()
        .prefix("adesh arm64 ")
        .tempdir()
        .unwrap();
    let input = dir.path().join("startup.o");
    let output = dir.path().join("startup.elf");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/aarch64_startup.s");
    let source_linux = tools.path(&source);
    // wslpath cannot canonicalize an output which has not been created.
    let input_linux = format!("{}/startup.o", tools.path(dir.path()));
    let assembled = tools.tool(
        "aarch64-linux-gnu-as",
        &[
            OsStr::new("-o"),
            OsStr::new(&input_linux),
            OsStr::new(&source_linux),
        ],
    );
    assert!(assembled.status.success(), "assembler: {assembled:?}");
    let linked = run(Command::new(env!("CARGO_BIN_EXE_adeshlink"))
        .args(["--target=aarch64-unknown-linux-gnu", "--static", "--output"])
        .arg(&output)
        .arg(&input));
    assert!(linked.status.success(), "adeshlink: {linked:?}");
    let bytes = std::fs::read(&output).unwrap();
    assert_eq!(&bytes[..4], b"\x7fELF");
    assert_eq!(u16::from_le_bytes(bytes[16..18].try_into().unwrap()), 2);
    assert_eq!(u16::from_le_bytes(bytes[18..20].try_into().unwrap()), 183);
    let output_linux = tools.path(&output);
    for args in [
        vec![],
        vec!["one argument with spaces"],
        vec!["a", "b", "c", "d", "e", "quote'\"", "last"],
    ] {
        let mut command_args = vec![
            OsStr::new("-E"),
            OsStr::new("ADESH_STARTUP_SENTINEL=ready"),
            OsStr::new(&output_linux),
        ];
        command_args.extend(args.iter().map(OsStr::new));
        let executed = tools.tool("qemu-aarch64", &command_args);
        assert_eq!(
            executed.status.code(),
            Some(37),
            "args={args:?}; startup: {executed:?}"
        );
        assert_eq!(executed.stdout, b"aarch64 startup ok\n");
        assert!(executed.stderr.is_empty(), "{executed:?}");
    }
}
