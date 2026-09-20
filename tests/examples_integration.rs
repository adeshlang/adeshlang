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

#[test]
fn example_bitwise_operations() {
    let p = PathBuf::from("examples/operators/bitwise_operations.adesh");
    let out = run_example(&p.to_string_lossy());
    assert!(out.contains("true"), "permission check missing: {out}");
    assert!(out.contains("false"), "cleared flag missing: {out}");
    assert!(out.contains("16"), "rotate_left missing: {out}");
    assert!(out.contains("188"), "bit_extract missing: {out}");
}

#[test]
fn example_bitwise_rgba_packing() {
    let p = PathBuf::from("examples/operators/bitwise_rgba_packing.adesh");
    let out = run_example(&p.to_string_lossy());
    assert!(out.contains("2685411376"), "packed value missing: {out}");
    assert!(out.contains("160"), "alpha channel missing: {out}");
    assert!(out.contains("16"), "red channel missing: {out}");
    assert!(out.contains("1056816"), "alpha mask missing: {out}");
    assert!(out.contains("true"), "bit 31 missing: {out}");
}

#[test]
fn example_bitwise_hashes() {
    let p = PathBuf::from("examples/operators/bitwise_hashes.adesh");
    let out = run_example(&p.to_string_lossy());
    assert!(out.contains("204"), "xor fold missing: {out}");
    assert!(out.contains("1632"), "rotate missing: {out}");
    assert!(out.contains("56"), "leading zeros missing: {out}");
    assert!(out.contains("15"), "mask missing: {out}");
}

#[test]
fn example_bitwise_toggle_and_test() {
    let p = PathBuf::from("examples/operators/bitwise_toggle_and_test.adesh");
    let out = run_example(&p.to_string_lossy());
    assert!(out.contains("5"), "flags value missing: {out}");
    assert!(out.contains("false"), "toggled flag missing: {out}");
    assert!(out.contains("2"), "bit_count missing: {out}");
}

#[test]
fn example_bitwise_ipv4_packing() {
    let p = PathBuf::from("examples/operators/bitwise_ipv4_packing.adesh");
    let out = run_example(&p.to_string_lossy());
    assert!(out.contains("3232235786"), "packed ip missing: {out}");
    assert!(out.contains("192"), "octet a missing: {out}");
    assert!(out.contains("10"), "octet d missing: {out}");
    assert!(out.contains("true"), "round trip missing: {out}");
}

#[test]
fn example_bitwise_rotations_endianness() {
    let p = PathBuf::from("examples/operators/bitwise_rotations_endianness.adesh");
    let out = run_example(&p.to_string_lossy());
    assert!(out.contains("2385920"), "rotate_left missing: {out}");
    assert!(out.contains("31"), "bit_mask missing: {out}");
    assert!(out.contains("61440"), "bit_mask_at missing: {out}");
    assert!(out.contains("true"), "round trip missing: {out}");
}

#[test]
fn example_bitwise_twos_complement() {
    let p = PathBuf::from("examples/operators/bitwise_twos_complement.adesh");
    let out = run_example(&p.to_string_lossy());
    assert!(out.contains("-1"), "~0 missing: {out}");
    assert!(out.contains("-2"), "~1 missing: {out}");
    assert!(out.contains("-32"), "arithmetic shift missing: {out}");
    assert!(out.contains("8"), "bit_count missing: {out}");
}
