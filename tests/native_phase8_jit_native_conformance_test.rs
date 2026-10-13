//! Phase 8 — Native JIT (`--jit-native`) execution conformance and
//! loud-failure regression tests.
//!
//! The native JIT historically diverged silently from the interpreter and
//! the other JIT paths in two ways:
//! 1. Raw-pointer programs (`alloc` / `p[i] = v` / `p[i]` / `free`) lowered
//!    to raw `malloc` memory and shipped raw addresses into the managed
//!    runtime bridge, printing `null` / wrong values with exit code 0.
//! 2. Array-indexing results (managed handles) fed the raw shared
//!    arithmetic lowering, summing handle bits instead of values.
//!
//! Both are fixed: raw pointers now live on the validated shared heap with
//! element-based indexing (mirroring the interpreter), managed handles are
//! unwrapped before arithmetic, and every violation (out-of-bounds store,
//! double free, non-numeric operand, unresolved callee, imported module)
//! fails loudly with a nonzero exit code.
//!
//! These tests execute the real CLI binary per backend and assert stdout
//! parity and exit codes — no library-internal shortcuts.

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

/// Run a command and return (combined stdout+stderr, exit code).
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
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let combined = format!("{}\n{}", stdout, stderr);
                return (
                    strip_ansi(&combined.replace('\r', "")),
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

fn unique_tag(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    format!("{}_{}", prefix, nanos)
}

fn run_backend(exe: &PathBuf, backend_args: &[&str], src: &PathBuf) -> (String, i32) {
    let mut cmd = Command::new(exe);
    let mut args: Vec<String> = vec!["run".to_string()];
    args.extend(backend_args.iter().map(|s| s.to_string()));
    args.push(src.to_string_lossy().to_string());
    cmd.args(&args);
    run_command_with_timeout(&mut cmd, Duration::from_secs(30))
}

/// Assert interpreter, --jit and --jit-native all exit 0 and print the
/// expected lines identically.
fn assert_three_backend_parity(program: &str, test_name: &str, expected_lines: &[&str]) {
    let tag = unique_tag(test_name);
    let src = std::env::temp_dir().join(format!("{}.adesh", tag));
    std::fs::write(&src, program).expect("write src");

    let exe = resolve_adesh_exe();
    assert!(exe.exists(), "adesh binary missing at {}", exe.display());

    let (interp_out, interp_code) = run_backend(&exe, &["--quiet"], &src);
    assert_eq!(interp_code, 0, "Interpreter failed: {}", interp_out);

    let (jit_out, jit_code) = run_backend(&exe, &["--quiet", "--jit"], &src);
    assert_eq!(jit_code, 0, "JIT failed: {}", jit_out);

    let (njit_out, njit_code) = run_backend(&exe, &["--quiet", "--njit"], &src);
    assert_eq!(njit_code, 0, "Native JIT failed: {}", njit_out);

    for expected in expected_lines {
        assert!(
            interp_out.contains(expected),
            "Interpreter output missing '{}':\n{}",
            expected,
            interp_out
        );
        assert!(
            jit_out.contains(expected),
            "JIT output missing '{}':\n{}",
            expected,
            jit_out
        );
        assert!(
            njit_out.contains(expected),
            "Native JIT output missing '{}':\n{}",
            expected,
            njit_out
        );
    }

    let _ = std::fs::remove_file(&src);
}

#[test]
fn test_jit_native_raw_pointer_indexing_parity() {
    // Byte-element pointers: alloc(16) gives 16 byte elements; p[0] and
    // p[1] are distinct elements (interpreter semantics).
    assert_three_backend_parity(
        r#"
        unsafe {
            let buf = alloc(16);
            buf[0] = 42;
            buf[1] = 7;
            let x = buf[0];
            let y = buf[1];
            println("x =", x);
            println("y =", y);
            free(buf);
            println("done");
        }
        "#,
        "njit_raw_ptr",
        &["x = 42", "y = 7", "done"],
    );
}

#[test]
fn test_jit_native_array_indexing_arithmetic_parity() {
    // Array elements flow through managed handles; the raw arithmetic
    // lowering must unwrap them (previously summed handle bits).
    assert_three_backend_parity(
        r#"
        fn main() {
            let arr = [0, 0];
            arr[0] = 42;
            arr[1] = 7;
            let a = arr[0];
            let b = arr[1];
            print("a =", a);
            print("b =", b);
            print("sum =", a + b);
        }
        "#,
        "njit_array_arith",
        &["a = 42", "b = 7", "sum = 49"],
    );
}

#[test]
fn test_jit_native_pointer_loop_explicit_free_parity() {
    assert_three_backend_parity(
        r#"
        unsafe {
            let i = 0;
            let total_sum = 0;
            while (i < 10) {
                let buf = alloc(16);
                buf[0] = i * 2;
                buf[1] = i * 3;
                total_sum = total_sum + buf[0] + buf[1];
                free(buf);
                i = i + 1;
            }
            println("total:", total_sum);
        }
        "#,
        "njit_ptr_loop_free",
        &["total: 225"],
    );
}

#[test]
fn test_jit_native_pointer_loop_raii_parity() {
    // No explicit free: allocation lifetimes are managed, output identical.
    assert_three_backend_parity(
        r#"
        unsafe {
            let i = 0;
            let total_sum = 0;
            while (i < 10) {
                let buf = alloc(16);
                buf[0] = i * 2;
                buf[1] = i * 3;
                total_sum = total_sum + buf[0] + buf[1];
                i = i + 1;
            }
            println("total:", total_sum);
        }
        "#,
        "njit_ptr_loop_raii",
        &["total: 225"],
    );
}

#[test]
fn test_jit_native_pointer_loop_control_flow_parity() {
    // break/continue paths with explicit frees on both sides.
    assert_three_backend_parity(
        r#"
        unsafe {
            let i = 0;
            let total_sum = 0;
            while (i < 10) {
                let buf = alloc(16);
                buf[0] = i * 2;
                buf[1] = i * 3;
                if (i == 3) {
                    total_sum = total_sum + buf[0];
                    free(buf);
                    i = i + 1;
                    continue;
                }
                if (i == 7) {
                    total_sum = total_sum + buf[0];
                    free(buf);
                    break;
                }
                total_sum = total_sum + buf[0] + buf[1];
                free(buf);
                i = i + 1;
            }
            println("final:", total_sum);
        }
        "#,
        "njit_ptr_loop_flow",
        &["final: 110"],
    );
}

#[test]
fn test_jit_native_runtime_oob_store_is_loud() {
    // A dynamic out-of-bounds pointer store (invisible to the compile-time
    // gate) must fail loudly with a nonzero exit on BOTH JIT backends, not
    // exit 0 silently.
    let tag = unique_tag("njit_oob");
    let src = std::env::temp_dir().join(format!("{}.adesh", tag));
    std::fs::write(
        &src,
        r#"
        unsafe {
            let buf = alloc(2);
            buf[5] = 1;
            println("no error - SILENT WRONG CODE");
            free(buf);
        }
        "#,
    )
    .expect("write src");

    let exe = resolve_adesh_exe();

    let (jit_out, jit_code) = run_backend(&exe, &["--quiet", "--jit"], &src);
    assert_ne!(jit_code, 0, "JIT must fail on OOB store, got: {}", jit_out);
    assert!(
        jit_out.to_lowercase().contains("out of bounds"),
        "JIT OOB failure must be loud, got:\n{}",
        jit_out
    );

    let (njit_out, njit_code) = run_backend(&exe, &["--quiet", "--njit"], &src);
    assert_ne!(
        njit_code, 0,
        "Native JIT must fail on OOB store, got: {}",
        njit_out
    );
    assert!(
        njit_out.to_lowercase().contains("out of bounds"),
        "Native JIT OOB failure must be loud, got:\n{}",
        njit_out
    );

    let _ = std::fs::remove_file(&src);
}

#[test]
fn test_jit_native_double_free_is_loud() {
    // Compile-time memory safety gate must reject the program on the
    // native JIT path with a nonzero exit.
    let tag = unique_tag("njit_dfree");
    let src = std::env::temp_dir().join(format!("{}.adesh", tag));
    std::fs::write(
        &src,
        r#"
        unsafe {
            let p = alloc(16);
            free(p);
            free(p);
            println("unreachable");
        }
        "#,
    )
    .expect("write src");

    let exe = resolve_adesh_exe();
    let (njit_out, njit_code) = run_backend(&exe, &["--quiet", "--njit"], &src);
    assert_ne!(
        njit_code, 0,
        "Native JIT must reject double free, got: {}",
        njit_out
    );
    assert!(
        njit_out.to_lowercase().contains("double free"),
        "Double-free rejection must be loud, got:\n{}",
        njit_out
    );

    let _ = std::fs::remove_file(&src);
}

#[test]
fn test_jit_native_import_fails_loudly_not_silently() {
    // Imported modules are not linked into the native JIT's compile unit.
    // Using one must fail loudly with a clear error — previously the import
    // silently resolved to null and the program printed x = 0, exit 0.
    let tag = unique_tag("njit_import");
    let src = std::env::temp_dir().join(format!("{}.adesh", tag));
    let helper = std::env::temp_dir().join(format!("{}_helper.adesh", tag));

    std::fs::write(&helper, "fn helper_value(): int {\n    return 7;\n}\n").expect("write helper");

    let import_line = format!("import \"{}\";", helper.to_string_lossy());
    let program = format!(
        "{}\nunsafe {{\n    let buf = alloc(16);\n    buf[0] = helper_value() * 6;\n    println(\"x =\", buf[0]);\n    free(buf);\n}}\n",
        import_line
    );
    std::fs::write(&src, program).expect("write src");

    let exe = resolve_adesh_exe();

    let (njit_out, njit_code) = run_backend(&exe, &["--quiet", "--njit"], &src);
    assert_ne!(
        njit_code, 0,
        "Native JIT must fail loudly on imported module use, got: {}",
        njit_out
    );
    assert!(
        njit_out.contains("not supported by --jit-native"),
        "Import failure must name the unsupported feature, got:\n{}",
        njit_out
    );
    assert!(
        !njit_out.contains("x = 0"),
        "Imported function must not silently evaluate to 0, got:\n{}",
        njit_out
    );

    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&helper);
}
