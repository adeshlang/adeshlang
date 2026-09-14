use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct CmdResult {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

fn resolve_adesh_exe() -> PathBuf {
    if let Ok(exe) = std::env::var("CARGO_BIN_EXE_adeshlang") {
        return PathBuf::from(exe);
    }

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    path.push("debug");
    if cfg!(windows) {
        path.push("adeshlang.exe");
    } else {
        path.push("adeshlang");
    }
    path
}

fn unique_temp_file(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before UNIX_EPOCH")
        .as_nanos();
    std::env::temp_dir().join(format!("{}_{}.adesh", prefix, nanos))
}

fn run_adesh_with_timeout(args: &[String], timeout: Duration) -> CmdResult {
    let exe = resolve_adesh_exe();
    assert!(
        exe.exists(),
        "adeshlang binary not found at {}",
        exe.display()
    );

    run_command_with_timeout(&exe, args, timeout)
}

fn strip_ansi_codes(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}'
            && chars.peek() == Some(&'[') {
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

fn run_command_with_timeout(exe: &PathBuf, args: &[String], timeout: Duration) -> CmdResult {
    assert!(exe.exists(), "executable not found at {}", exe.display());

    let mut child = Command::new(exe)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn process {}: {}", exe.display(), e));

    let start = Instant::now();
    loop {
        match child.try_wait().expect("failed to poll child process") {
            Some(status) => {
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();

                if let Some(mut out) = child.stdout.take() {
                    out.read_to_end(&mut stdout)
                        .expect("failed reading child stdout");
                }
                if let Some(mut err) = child.stderr.take() {
                    err.read_to_end(&mut stderr)
                        .expect("failed reading child stderr");
                }

                return CmdResult {
                    status,
                    stdout: String::from_utf8_lossy(&stdout).to_string(),
                    stderr: String::from_utf8_lossy(&stderr).to_string(),
                };
            }
            None => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!(
                        "adeshlang command timed out after {:?}: {:?}",
                        timeout, args
                    );
                }
                thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

#[test]
fn regression_cli_run_paths_do_not_hang() {
    let program = r#"
fn main() {
    return 0;
}
"#;

    let path = unique_temp_file("adesh_day1_hang_regression");
    std::fs::write(&path, program).expect("failed to write temp program");

    let path_str = path.to_string_lossy().to_string();
    let backends: Vec<Vec<String>> = vec![
        vec!["run".into(), path_str.clone(), "--verbose".into()],
        vec![
            "run".into(),
            "--jit".into(),
            path_str.clone(),
            "--verbose".into(),
        ],
        vec![
            "run".into(),
            "--bytecode".into(),
            path_str.clone(),
            "--verbose".into(),
        ],
    ];

    for args in backends {
        let result = run_adesh_with_timeout(&args, Duration::from_secs(20));
        assert!(
            result.status.success(),
            "command failed: {:?}\nstdout:\n{}\nstderr:\n{}",
            args,
            result.stdout,
            result.stderr
        );
        assert!(
            result
                .stderr
                .contains("Compile-time memory safety validation passed!"),
            "memory safety checks did not run as expected for {:?}\nstderr:\n{}",
            args,
            result.stderr
        );
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn regression_native_jit_prints_bool_and_numbers() {
    let program = r#"
fn main() {
    print(42);
    print(3.5);
    print(true);
    print(false);
    println(7, 8.25, "ok");
}
"#;

    let path = unique_temp_file("adesh_day1_njit_print_regression");
    std::fs::write(&path, program).expect("failed to write temp program");

    let args = vec![
        "run".to_string(),
        "--njit".to_string(),
        path.to_string_lossy().to_string(),
        "--quiet".to_string(),
    ];

    let result = run_adesh_with_timeout(&args, Duration::from_secs(20));
    assert!(
        result.status.success(),
        "native jit command failed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let lines: Vec<&str> = result.stdout.lines().collect();
    assert!(
        lines.contains(&"42"),
        "missing integer print in output: {}",
        result.stdout
    );
    assert!(
        lines.contains(&"3.5"),
        "missing float print in output: {}",
        result.stdout
    );
    assert!(
        lines.contains(&"true"),
        "missing bool true print in output: {}",
        result.stdout
    );
    assert!(
        lines.contains(&"false"),
        "missing bool false print in output: {}",
        result.stdout
    );
    assert!(
        lines.contains(&"7 8.25 ok"),
        "missing multi-arg print output: {}",
        result.stdout
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn regression_aot_pretty_print_objects_and_nested_values() {
    let program = r#"
fn main() {
    let person = {
        name: "John Doe",
        age: 28,
        email: "john@example.com"
    };

    let numbers = [10, 20, 30, 40, 50];

    let company = {
        name: "Tech Corp",
        employees: 150,
        departments: ["Engineering", "Sales", "HR"],
        headquarters: {
            city: "San Francisco",
            country: "USA"
        }
    };

    print(person, { pretty: true });
    print(numbers, { pretty: true });
    print(company, { pretty: true });
}
"#;

    let src_path = unique_temp_file("adesh_day1_aot_pretty_src");
    std::fs::write(&src_path, program).expect("failed to write temp program");

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before UNIX_EPOCH")
        .as_nanos();
    let exe_name = if cfg!(windows) {
        format!("adesh_day1_aot_pretty_{}.exe", nanos)
    } else {
        format!("adesh_day1_aot_pretty_{}", nanos)
    };
    let exe_path = std::env::temp_dir().join(exe_name);

    let compile_args = vec![
        "compile-aot".to_string(),
        src_path.to_string_lossy().to_string(),
        exe_path.to_string_lossy().to_string(),
    ];
    let compile_result = run_adesh_with_timeout(&compile_args, Duration::from_secs(120));
    assert!(
        compile_result.status.success(),
        "AOT compile failed\nstdout:\n{}\nstderr:\n{}",
        compile_result.stdout,
        compile_result.stderr
    );

    let run_result = run_command_with_timeout(&exe_path, &[], Duration::from_secs(20));
    assert!(
        run_result.status.success(),
        "AOT executable failed\nstdout:\n{}\nstderr:\n{}",
        run_result.stdout,
        run_result.stderr
    );

    let out = strip_ansi_codes(&run_result.stdout.replace('\r', ""));
    assert!(
        out.contains("John Doe"),
        "missing person string value: {}",
        out
    );
    assert!(
        out.contains("john@example.com"),
        "missing person email value: {}",
        out
    );
    assert!(
        out.contains("10 ⟨u8⟩"),
        "missing array pretty output: {}",
        out
    );
    assert!(
        out.contains("Tech Corp"),
        "missing nested object top-level value: {}",
        out
    );
    assert!(
        out.contains("Engineering"),
        "missing nested array string value: {}",
        out
    );
    assert!(
        out.contains("San Francisco"),
        "missing nested object inner value: {}",
        out
    );

    let _ = std::fs::remove_file(src_path);
    let _ = std::fs::remove_file(exe_path);
}

#[test]
fn regression_aot_pretty_print_integer_type_ranges() {
    let program = r#"
fn main() {
    let nums = [255, 256, -128, -129];
    print(nums, { pretty: true });
}
"#;

    let src_path = unique_temp_file("adesh_day1_aot_type_ranges_src");
    std::fs::write(&src_path, program).expect("failed to write temp program");

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before UNIX_EPOCH")
        .as_nanos();
    let exe_name = if cfg!(windows) {
        format!("adesh_day1_aot_type_ranges_{}.exe", nanos)
    } else {
        format!("adesh_day1_aot_type_ranges_{}", nanos)
    };
    let exe_path = std::env::temp_dir().join(exe_name);

    let compile_args = vec![
        "compile-aot".to_string(),
        src_path.to_string_lossy().to_string(),
        exe_path.to_string_lossy().to_string(),
    ];
    let compile_result = run_adesh_with_timeout(&compile_args, Duration::from_secs(120));
    assert!(
        compile_result.status.success(),
        "AOT compile failed\nstdout:\n{}\nstderr:\n{}",
        compile_result.stdout,
        compile_result.stderr
    );

    let run_result = run_command_with_timeout(&exe_path, &[], Duration::from_secs(20));
    assert!(
        run_result.status.success(),
        "AOT executable failed\nstdout:\n{}\nstderr:\n{}",
        run_result.stdout,
        run_result.stderr
    );

    let out = strip_ansi_codes(&run_result.stdout.replace('\r', ""));
    assert!(
        out.contains("255 ⟨u8⟩"),
        "expected u8 hint for 255: {}",
        out
    );
    assert!(
        out.contains("256 ⟨u16⟩"),
        "expected u16 hint for 256: {}",
        out
    );
    assert!(
        out.contains("-128 ⟨i8⟩"),
        "expected i8 hint for -128: {}",
        out
    );
    assert!(
        out.contains("-129 ⟨i16⟩"),
        "expected i16 hint for -129: {}",
        out
    );

    let _ = std::fs::remove_file(src_path);
    let _ = std::fs::remove_file(exe_path);
}

#[test]
fn regression_print_compact_options_parity_njit_aot() {
    let src_path = PathBuf::from("examples/print/05_combined_options.adesh");
    assert!(src_path.exists(), "example missing: {}", src_path.display());

    let run_njit_args = vec![
        "run".to_string(),
        "--njit".to_string(),
        src_path.to_string_lossy().to_string(),
        "--quiet".to_string(),
    ];
    let njit_result = run_adesh_with_timeout(&run_njit_args, Duration::from_secs(60));
    assert!(
        njit_result.status.success(),
        "native jit run failed\nstdout:\n{}\nstderr:\n{}",
        njit_result.stdout,
        njit_result.stderr
    );
    let njit_out = strip_ansi_codes(&njit_result.stdout.replace('\r', ""));

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before UNIX_EPOCH")
        .as_nanos();
    let exe_name = if cfg!(windows) {
        format!("adesh_day1_print_compact_{}.exe", nanos)
    } else {
        format!("adesh_day1_print_compact_{}", nanos)
    };
    let exe_path = std::env::temp_dir().join(exe_name);

    let compile_args = vec![
        "compile-aot".to_string(),
        src_path.to_string_lossy().to_string(),
        exe_path.to_string_lossy().to_string(),
    ];
    let compile_result = run_adesh_with_timeout(&compile_args, Duration::from_secs(120));
    assert!(
        compile_result.status.success(),
        "AOT compile failed\nstdout:\n{}\nstderr:\n{}",
        compile_result.stdout,
        compile_result.stderr
    );

    let run_aot_result = run_command_with_timeout(&exe_path, &[], Duration::from_secs(20));
    assert!(
        run_aot_result.status.success(),
        "AOT executable failed\nstdout:\n{}\nstderr:\n{}",
        run_aot_result.stdout,
        run_aot_result.stderr
    );
    let aot_out = strip_ansi_codes(&run_aot_result.stdout.replace('\r', ""));

    for out in [&njit_out, &aot_out] {
        assert!(
            out.contains("\"Result:\" => {"),
            "compact pretty prefix mismatch:\n{}",
            out
        );
        assert!(
            out.contains("status: \"active\""),
            "status string formatting mismatch:\n{}",
            out
        );
        assert!(
            out.contains("} [DONE]"),
            "missing compact print end marker:\n{}",
            out
        );
        assert!(
            !out.contains("\"\"active\"\""),
            "double-quoted string regression detected:\n{}",
            out
        );
        assert!(
            !out.contains("Result: =>"),
            "missing string quotes regression detected:\n{}",
            out
        );
    }

    let _ = std::fs::remove_file(exe_path);
}
