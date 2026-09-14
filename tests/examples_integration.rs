use std::path::PathBuf;
use std::process::Command;

fn run_example(path: &str) -> String {
    let exe = std::env::var("CARGO_BIN_EXE_adeshlang").expect("CARGO_BIN_EXE_adeshlang not set; run `cargo test` from workspace root which builds the binary");
    let output = Command::new(exe)
        .args(["run", path])
        .output()
        .expect("failed to run adeshlang binary");
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
