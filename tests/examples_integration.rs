use std::path::PathBuf;
use std::process::Command;

fn resolve_adesh_exe() -> PathBuf {
    if let Ok(exe) = std::env::var("CARGO_BIN_EXE_adesh") {
        return PathBuf::from(exe);
    }
    if let Ok(exe) = std::env::var("CARGO_BIN_EXE_adeshlang") {
        return PathBuf::from(exe);
    }

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    path.push("debug");
    let candidate = if cfg!(windows) {
        path.join("adesh.exe")
    } else {
        path.join("adesh")
    };
    if candidate.exists() {
        return candidate;
    }
    if cfg!(windows) {
        path.push("adeshlang.exe");
    } else {
        path.push("adeshlang");
    }
    path
}

fn run_example(path: &str) -> String {
    let exe = resolve_adesh_exe();
    let output = Command::new(exe)
        .args(["run", path])
        .output()
        .expect("failed to run adesh binary");
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn example_borrow_shared_reuse() {
    let p = PathBuf::from("examples/tests/borrow_shared_reuse.adesh");
    let out = run_example(&p.to_string_lossy());
    assert!(out.contains("hello hello hello") || out.contains("hello\nhello\nhello"));
}

#[test]
fn example_borrow_in_function() {
    let p = PathBuf::from("examples/tests/borrow_in_function.adesh");
    let out = run_example(&p.to_string_lossy());
    assert!(out.contains("inside: world") && out.contains("after: world"));
}

#[test]
fn example_multiple_shared_borrows() {
    let p = PathBuf::from("examples/tests/multiple_shared_borrows.adesh");
    let out = run_example(&p.to_string_lossy());
    // allow both single-line and multi-line prints
    assert!(out.contains("abc"));
}

#[test]
fn example_scope_drop() {
    let p = PathBuf::from("examples/tests/scope_drop.adesh");
    let out = run_example(&p.to_string_lossy());
    assert!(out.contains("scope_ok") && out.contains("scope_ok"));
}
