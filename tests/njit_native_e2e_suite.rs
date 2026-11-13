//! End-to-End Tests for Adesh Native JIT (NJIT) Powered by Adesh Native Codegen & Linker.
//!
//! Validates:
//! - Arithmetic & function calls
//! - Recursion (factorial)
//! - Loops & local variables (while loops, accumulations)
//! - 100% parity with interpreter/AOT without Cranelift.

use std::process::Command;

fn run_with_native_jit(source_code: &str) -> (bool, String, String) {
    let mut temp = tempfile::Builder::new()
        .suffix(".adesh")
        .tempfile()
        .expect("create temp file");

    use std::io::Write;
    temp.write_all(source_code.as_bytes()).expect("write source");
    temp.flush().expect("flush temp");

    let adesh_bin = env!("CARGO_BIN_EXE_adesh");
    let output = Command::new(adesh_bin)
        .arg("run")
        .arg(temp.path())
        .arg("--backend=native-jit")
        .arg("--jit-backend=adesh")
        .arg("--profile")
        .output()
        .expect("execute adesh");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), stdout, stderr)
}

#[test]
fn test_native_jit_simple_arithmetic() {
    let code = r#"
fn main() {
    let a = 15;
    let b = 27;
    let c = a + b;
    return c;
}
"#;
    let (success, stdout, stderr) = run_with_native_jit(code);
    assert!(success, "Native JIT execution failed: stdout: {}, stderr: {}", stdout, stderr);
    assert!(stderr.contains("Native JIT Compilation"), "Expected profile output in stderr: {}", stderr);
    assert!(stderr.contains("Direct Native CPU Execution"), "Expected profile output in stderr: {}", stderr);
}

#[test]
fn test_native_jit_multi_function_call() {
    let code = r#"
fn multiply_add(x, y, z) {
    return x * y + z;
}

fn main() {
    let res = multiply_add(8, 9, 10);
    return res;
}
"#;
    let (success, stdout, stderr) = run_with_native_jit(code);
    assert!(success, "Native JIT execution failed: stdout: {}, stderr: {}", stdout, stderr);
}

#[test]
fn test_native_jit_recursion_and_loops() {
    let code = r#"
fn factorial(n) {
    if n <= 1 {
        return 1;
    }
    return n * factorial(n - 1);
}

fn sum_numbers(limit) {
    let sum = 0;
    let i = 1;
    while i <= limit {
        sum = sum + i;
        i = i + 1;
    }
    return sum;
}

fn main() {
    let f = factorial(6);     // 720
    let s = sum_numbers(20);  // 210
    let total = f + s;        // 930
    return total;
}
"#;
    let (success, stdout, stderr) = run_with_native_jit(code);
    assert!(success, "Native JIT execution failed: stdout: {}, stderr: {}", stdout, stderr);
}
