//! Phase 4 — LTO wired through CompilerDriver:
//! `adesh build --lto` routes ADOB emission through `CompilerDriver`, which
//! runs the IR-level `LtoEngine` (global dead-function elimination + inlining)
//! before the per-module optimization pipeline. Validates:
//! 1. An `--lto` build removes an uncalled, unreferenced function that the
//!    section-level linker GC cannot remove (all functions of a module share
//!    one `.text` section), so the LTO ADOB must be smaller.
//! 2. An `--lto` executable still runs correctly (exit code asserted).

use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn resolve_adesh_exe() -> PathBuf {
    if let Ok(exe) = std::env::var("CARGO_BIN_EXE_adesh") {
        return PathBuf::from(exe);
    }
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    path.push("debug");
    if cfg!(windows) {
        path.join("adesh.exe")
    } else {
        path.join("adesh")
    }
}

fn unique_temp_path(prefix: &str, ext: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before UNIX_EPOCH")
        .as_nanos();
    std::env::temp_dir().join(format!("{}_{}.{}", prefix, nanos, ext))
}

const PROGRAM: &str = r#"
fn dead_uncalled(): int {
    let x = 111 + 222 + 333;
    return x * 2;
}

fn live_helper(x: int): int {
    return x + 40;
}

fn main(): int {
    return live_helper(2);
}
"#;

#[test]
fn test_lto_removes_dead_function_and_shrinks_adob() {
    let adesh = resolve_adesh_exe();
    let src_path = unique_temp_path("adesh_lto_src", "adesh");
    std::fs::write(&src_path, PROGRAM).expect("write source");

    let plain_adob = unique_temp_path("adesh_lto_plain", "adob");
    let lto_adob = unique_temp_path("adesh_lto_opt", "adob");

    let out = Command::new(&adesh)
        .arg("build")
        .arg(&src_path)
        .arg("--emit=adob")
        .arg("-o")
        .arg(&plain_adob)
        .output()
        .expect("run adesh build (plain)");
    assert!(
        out.status.success(),
        "plain build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = Command::new(&adesh)
        .arg("build")
        .arg(&src_path)
        .arg("--emit=adob")
        .arg("--lto")
        .arg("-o")
        .arg(&lto_adob)
        .output()
        .expect("run adesh build (lto)");
    assert!(
        out.status.success(),
        "LTO build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let plain_bytes = std::fs::read(&plain_adob).expect("read plain ADOB");
    let lto_bytes = std::fs::read(&lto_adob).expect("read LTO ADOB");
    assert!(
        lto_bytes.len() < plain_bytes.len(),
        "LTO ADOB ({} bytes) must be smaller than plain ADOB ({} bytes): \
         global DCE should remove the uncalled dead_uncalled function",
        lto_bytes.len(),
        plain_bytes.len()
    );

    let _ = std::fs::remove_file(&src_path);
    let _ = std::fs::remove_file(&plain_adob);
    let _ = std::fs::remove_file(&lto_adob);
}

/// Executing the produced binary is only possible on Windows x86-64 (the
/// native pipeline emits PE there); the ADOB comparison above runs on every
/// host.
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[test]
fn test_lto_executable_still_runs_correctly() {
    let adesh = resolve_adesh_exe();
    let src_path = unique_temp_path("adesh_lto_run_src", "adesh");
    std::fs::write(&src_path, PROGRAM).expect("write source");

    let exe_path = unique_temp_path("adesh_lto_run", "exe");

    let out = Command::new(&adesh)
        .arg("build")
        .arg(&src_path)
        .arg("--lto")
        .arg("-o")
        .arg(&exe_path)
        .output()
        .expect("run adesh build (lto exe)");
    assert!(
        out.status.success(),
        "LTO executable build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let run = Command::new(&exe_path)
        .output()
        .expect("execute LTO binary");
    assert_eq!(
        run.status.code(),
        Some(42),
        "LTO binary must preserve program semantics"
    );

    let _ = std::fs::remove_file(&src_path);
    let _ = std::fs::remove_file(&exe_path);
}
