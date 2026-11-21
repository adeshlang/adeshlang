//! Phase 13 Mach-O link-and-run gate: Adesh source -> native backend/linker ->
//! real dyld, on a macOS runner, in default and fast modes.
//!
//! Reachable subset asserted here: a source program cross-builds through the
//! real CLI to a Mach-O64 `MH_EXECUTE`+`MH_PIE` image for the runner's native
//! Apple triple, dyld accepts it (`codesign --verify --strict` on the exact
//! bytes the CLI produced, never re-signed), and it executes with byte-exact
//! stdout and exit status. `hello`/`print` exercises a real dyld-bound libSystem
//! import: the runtime's `aot_print_cstr` reaches `_write` through `__got`, so a
//! passing print case is proof that a bound import resolved and ran, not just
//! that the image parsed.
//!
//! NOT asserted (blocked, not skipped silently): a user-source `extern "C"`
//! `puts` call, a TLS access, and a `--shared` dylib loaded with
//! `dlopen`/`dlsym`. The native backend lowers an `extern` declaration to an
//! empty block and has no verified source-level FFI/TLS/dynamic-loading path
//! (Phase 18 source ABI surface; Mach-O TLS/dylib execution). Asserting those
//! here would wire a test for an unimplemented feature, so they stay open and
//! are recorded in NEXT_IMPLEMENTATION_PLAN.md Phase 13 item 4.
//!
//! macOS-only: on any other host every case prints a loud SKIP (or fails under
//! ADESH_REQUIRE_FOREIGN_EXEC=1). The desktop-execution CI job runs this on the
//! macos-15-intel / macos-15 matrix entries.
#[path = "../linker/tests/support/foreign_exec.rs"]
#[allow(dead_code)] // shared harness; this suite uses only `run`, not LinuxTools
mod foreign_exec;

use foreign_exec::run;
use std::{path::Path, process::Command};

/// The runner's native Apple triple, or None off macOS. A Mach-O source build
/// needs the host runtime archive and dyld; neither exists on a Windows/Linux
/// host, so this is a loud local SKIP rather than a silent pass.
fn macos_target() -> Option<&'static str> {
    let triple = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        _ => {
            assert!(
                std::env::var_os("ADESH_REQUIRE_FOREIGN_EXEC").is_none(),
                "required Mach-O execution needs a macOS runner (host is {}/{})",
                std::env::consts::OS,
                std::env::consts::ARCH
            );
            eprintln!(
                "SKIP: Mach-O link-and-run needs macOS; host is {}/{}",
                std::env::consts::OS,
                std::env::consts::ARCH
            );
            return None;
        }
    };
    Some(triple)
}

fn check_macho(path: &Path, target: &str) {
    let bytes = std::fs::read(path).expect("successful CLI build must write the requested image");
    let u32_at = |p| u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap());
    assert_eq!(u32_at(0), 0xfeedfacf, "Mach-O 64");
    assert_eq!(
        u32_at(4),
        if target.starts_with("aarch64") {
            0x0100000c
        } else {
            0x01000007
        },
        "cputype must match the requested Apple triple"
    );
    assert_eq!(u32_at(12), 2, "MH_EXECUTE");
    assert_ne!(u32_at(24) & 0x00200000, 0, "MH_PIE");
    // Verify the exact image the CLI produced; re-signing would hide a broken
    // native signer (same rule as native_desktop_exec_test).
    let verified = run(Command::new("/usr/bin/codesign")
        .args(["--verify", "--strict", "--verbose=2"])
        .arg(path));
    assert!(
        verified.status.success(),
        "codesign rejected {path:?}: {verified:?}"
    );
}

fn source_case(source: &str, stdout: &str, exit: i32) {
    let Some(target) = macos_target() else {
        return;
    };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    for fast in [false, true] {
        let dir = tempfile::Builder::new()
            .prefix("adesh macho execution ")
            .tempdir_in(repo.join("target"))
            .unwrap();
        let input = dir.path().join("macho source.adesh");
        let image = dir.path().join("macho app");
        std::fs::write(&input, source).unwrap();
        let mut build = Command::new(env!("CARGO_BIN_EXE_adesh"));
        build
            .current_dir(repo)
            .arg("build")
            .arg(&input)
            .args(["--target", target, "--emit=exe", "-o"])
            .arg(&image);
        if fast {
            build.arg("--fast");
        }
        let built = run(&mut build);
        assert!(
            built.status.success(),
            "{target}, fast={fast}: native Mach-O build failed: {}\n{}",
            String::from_utf8_lossy(&built.stdout),
            String::from_utf8_lossy(&built.stderr)
        );
        check_macho(&image, target);
        let executed = run(Command::new(&image).current_dir(dir.path()));
        assert_eq!(
            executed.status.code(),
            Some(exit),
            "{target}, fast={fast}: {executed:?}"
        );
        assert_eq!(
            executed.stdout,
            stdout.as_bytes(),
            "{target}, fast={fast}: {executed:?}"
        );
        assert!(
            executed.stderr.is_empty(),
            "{target}, fast={fast}: {executed:?}"
        );
    }
}

#[test]
fn hello_executes_a_dyld_bound_libsystem_import() {
    // print -> aot_print_cstr -> _write through __got: a passing case proves a
    // real dyld-bound libSystem import resolved and executed.
    source_case(
        "print(\"phase13 macho source\");",
        "phase13 macho source\n",
        0,
    );
}

#[test]
fn main_return_is_the_process_exit_code() {
    source_case("fn main(): int { return 42; }", "", 42);
}

#[test]
fn nested_calls_and_arithmetic() {
    source_case(
        include_str!("fixtures/phase13_aarch64/functions.adesh"),
        "",
        50,
    );
}

#[test]
fn loops_and_comparisons() {
    source_case(include_str!("fixtures/phase13_aarch64/loop.adesh"), "", 55);
}
