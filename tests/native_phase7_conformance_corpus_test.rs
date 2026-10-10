//! Phase 7 — Multi-Backend Language Conformance Corpus
//!
//! Tests repeatable language program behavior across:
//! 1. The Interpreter (`adesh run <file>`)
//! 2. The Native JIT (`adesh run --njit <file>`)
//! 3. The Native AOT Compiler (`adesh compile-aot <file> <exe>`)
//!
//! Asserts stdout parity and successful execution across all backends.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

fn run_command_with_timeout(cmd: &mut Command, timeout: Duration) -> (String, i32) {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn command");

    let start = Instant::now();
    loop {
        match child.try_wait().expect("try_wait failed") {
            Some(status) => {
                let output = child.wait_with_output().expect("wait_with_output");
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                return (
                    strip_ansi(&stdout.replace('\r', "")),
                    status.code().unwrap_or(-1),
                );
            }
            None => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("command timed out after {:?}", timeout);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

fn assert_backend_conformance(program: &str, test_name: &str, expected_lines: &[&str]) {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let src_file = std::env::temp_dir().join(format!("{}_{}.adesh", test_name, nanos));
    std::fs::write(&src_file, program).expect("write src");

    let exe = resolve_adesh_exe();
    assert!(exe.exists(), "adesh binary missing at {}", exe.display());

    // 1. Interpreter: adesh run <file> --quiet
    let mut interp_cmd = Command::new(&exe);
    interp_cmd.args(["run", &src_file.to_string_lossy(), "--quiet"]);
    let (interp_out, interp_code) =
        run_command_with_timeout(&mut interp_cmd, Duration::from_secs(30));
    assert_eq!(interp_code, 0, "Interpreter failed: {}", interp_out);

    // 2. Native JIT: adesh run --njit <file> --quiet
    let mut njit_cmd = Command::new(&exe);
    njit_cmd.args(["run", "--njit", &src_file.to_string_lossy(), "--quiet"]);
    let (njit_out, njit_code) = run_command_with_timeout(&mut njit_cmd, Duration::from_secs(30));
    assert_eq!(njit_code, 0, "NJIT failed: {}", njit_out);

    // 3. Native AOT: adesh compile-aot <file> <out>
    let exe_name = if cfg!(windows) {
        format!("{}_{}.exe", test_name, nanos)
    } else {
        format!("{}_{}", test_name, nanos)
    };
    let aot_exe = std::env::temp_dir().join(exe_name);
    let mut compile_cmd = Command::new(&exe);
    compile_cmd.args([
        "compile-aot",
        &src_file.to_string_lossy(),
        &aot_exe.to_string_lossy(),
    ]);
    let (compile_out, compile_code) =
        run_command_with_timeout(&mut compile_cmd, Duration::from_secs(60));
    assert_eq!(compile_code, 0, "AOT compile failed: {}", compile_out);

    let mut aot_run_cmd = Command::new(&aot_exe);
    let (aot_out, aot_code) = run_command_with_timeout(&mut aot_run_cmd, Duration::from_secs(20));
    assert_eq!(aot_code, 0, "AOT execution failed: {}", aot_out);

    // Assert expected lines across all backends
    for expected in expected_lines {
        assert!(
            interp_out.contains(expected),
            "Interpreter output missing '{}':\n{}",
            expected,
            interp_out
        );
        assert!(
            njit_out.contains(expected),
            "NJIT output missing '{}':\n{}",
            expected,
            njit_out
        );
        assert!(
            aot_out.contains(expected),
            "AOT output missing '{}':\n{}",
            expected,
            aot_out
        );
    }

    let _ = std::fs::remove_file(src_file);
    let _ = std::fs::remove_file(aot_exe);
}

#[test]
fn test_conformance_arithmetic_and_expressions() {
    let program = r#"
fn main() {
    let a = 10 + 20 * 3;
    let b = (100 - 40) / 2;
    let c = 7 % 4;
    print(a);
    print(b);
    print(c);
}
"#;
    assert_backend_conformance(program, "arithmetic", &["70", "30", "3"]);
}

#[test]
fn test_conformance_conditionals_and_branching() {
    let program = r#"
fn main() {
    let x = 42;
    if x > 40 {
        print("x is greater");
    } else {
        print("x is smaller");
    }

    let is_valid = true;
    if is_valid {
        print("valid branch");
    }
}
"#;
    assert_backend_conformance(program, "branching", &["x is greater", "valid branch"]);
}

#[test]
fn test_conformance_loops_and_accumulation() {
    let program = r#"
fn main() {
    let sum = 0;
    let i = 1;
    while i <= 10 {
        sum = sum + i;
        i = i + 1;
    }
    print(sum);
}
"#;
    assert_backend_conformance(program, "loops", &["55"]);
}

#[test]
fn test_conformance_user_functions_and_recursion() {
    let program = r#"
fn fib(n) {
    if n <= 1 {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

fn add3(a, b, c) {
    return a + b + c;
}

fn main() {
    print(fib(8));
    print(add3(10, 20, 30));
}
"#;
    assert_backend_conformance(program, "functions", &["21", "60"]);
}

#[test]
fn test_conformance_arrays_and_indexing() {
    let program = r#"
fn main() {
    let numbers = [100, 200, 300];
    let first = numbers[0];
    let second = numbers[1];
    let third = numbers[2];
    print(first);
    print(second);
    print(third);
}
"#;
    assert_backend_conformance(program, "arrays", &["100", "200", "300"]);
}
