//! Adesh source -> IA-32 codegen -> static ELF32 -> qemu-i386.
#[path = "../linker/tests/support/foreign_exec.rs"]
mod foreign_exec;

use foreign_exec::{LinuxTools, run};
use std::{ffi::OsStr, path::Path, process::Command};

const TRIPLE: &str = "i686-unknown-linux-gnu";

fn source_case(name: &str, expected_stdout: &[u8], expected_exit: i32) {
    let Some(tools) = LinuxTools::discover("as", "qemu-i386") else {
        return;
    };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = repo.join(format!("tests/fixtures/phase22_i686/{name}.adesh"));
    for fast in [false, true] {
        let dir = tempfile::Builder::new()
            .prefix("adesh source i686 ")
            .tempdir()
            .unwrap();
        let output = dir.path().join("app.elf");
        let mut command = Command::new(env!("CARGO_BIN_EXE_adesh"));
        command
            .current_dir(repo)
            .arg("build")
            .arg(&source)
            .args(["--target", TRIPLE, "--emit=exe", "-o"])
            .arg(&output);
        if fast {
            command.arg("--fast");
        }
        let compiled = run(&mut command);
        let diagnostics = format!(
            "{}\n{}",
            String::from_utf8_lossy(&compiled.stdout),
            String::from_utf8_lossy(&compiled.stderr)
        );
        if diagnostics.contains("No runtime archive for target")
            && std::env::var_os("ADESH_REQUIRE_FOREIGN_EXEC").is_none()
        {
            eprintln!(
                "SKIP: i686 source execution needs its runtime archive; run \
                 bash scripts/build-runtime-archives.sh --targets {TRIPLE}"
            );
            return;
        }
        assert!(
            compiled.status.success(),
            "{name}, fast={fast}: {diagnostics}"
        );

        let bytes = std::fs::read(&output).expect("successful build writes an executable");
        assert_eq!(&bytes[..4], b"\x7fELF");
        assert_eq!(bytes[4], 1, "ELF32");
        assert_eq!(bytes[5], 1, "little endian");
        assert_eq!(u16::from_le_bytes(bytes[16..18].try_into().unwrap()), 2);
        assert_eq!(u16::from_le_bytes(bytes[18..20].try_into().unwrap()), 3);

        let executed = tools.tool("qemu-i386", &[OsStr::new(&tools.path(&output))]);
        assert_eq!(
            executed.status.code(),
            Some(expected_exit),
            "{name}, fast={fast}: {executed:?}"
        );
        assert_eq!(
            executed.stdout, expected_stdout,
            "{name}, fast={fast}: {executed:?}"
        );
        assert!(
            executed.stderr.is_empty(),
            "{name}, fast={fast}: {executed:?}"
        );
    }
}

#[test]
fn main_return_becomes_process_exit_status() {
    source_case("exit", b"", 42);
}

#[test]
fn string_output_executes() {
    source_case("hello", b"hello i686\n", 0);
}

#[test]
fn local_arithmetic_and_loop_execute() {
    source_case("loop", b"", 3);
}

#[test]
fn parameters_and_calls_execute() {
    source_case("functions", b"", 50);
}

#[test]
fn integer_operators_execute() {
    source_case("arithmetic", b"", 46);
}

#[test]
fn negative_narrow_and_int_values_print_signed() {
    source_case("negative_print", b"-160\n-42\n-7\n", 0);
}

#[test]
fn register_pressure_and_six_stack_arguments_execute() {
    source_case("pressure", b"", 112);
}

#[test]
fn x87_double_arithmetic_comparisons_and_printing_execute() {
    source_case(
        "fp_arithmetic",
        b"0.30000000000000004\n25\n2.5\n-0.1\n7.5\n15\ngt\nlt\nne\n0.3333333333333333\n10.5\n9\n",
        0,
    );
}

#[test]
fn x87_calls_returns_globals_and_conversions_execute() {
    source_case(
        "fp_calls_and_conversions",
        b"5\n5\n3628800\ninf\n-inf\nNaN\nnan ne\nnan !lt\n-7\n3000000000\n3000000\n1 2.5\n2475\n",
        7,
    );
}

#[test]
fn static_tls_and_writable_globals_next_to_strings_execute() {
    source_case("tls_static", b"tls\n45\n2\n21\n103\n", 5);
}

#[test]
fn sixty_four_bit_runtime_helper_parameters_are_marshalled_on_i686() {
    source_case("helper_marshalling", b"s 7 200 true\ntrue\nx 5 -3\n7\n", 0);
}

#[test]
fn f32_is_refused_on_i686() {
    let Some(_tools) = LinuxTools::discover("as", "qemu-i386") else {
        return;
    };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = tempfile::Builder::new()
        .prefix("adesh i686 f32 ")
        .tempdir()
        .unwrap();
    let output = dir.path().join("app.elf");
    let mut command = Command::new(env!("CARGO_BIN_EXE_adesh"));
    command
        .current_dir(repo)
        .arg("build")
        .arg("tests/fixtures/phase22_i686/f32_refused.adesh")
        .args(["--target", TRIPLE, "--emit=exe", "-o"])
        .arg(&output);
    let compiled = run(&mut command);
    assert!(!compiled.status.success());
    let diagnostics = format!(
        "{}\n{}",
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert!(diagnostics.contains("f32"), "{diagnostics}");
}

#[test]
fn genuine_64_bit_integer_is_refused() {
    let Some(_tools) = LinuxTools::discover("as", "qemu-i386") else {
        return;
    };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = tempfile::Builder::new()
        .prefix("adesh i686 wide ")
        .tempdir()
        .unwrap()
        .path()
        .join("app.elf");
    let mut command = Command::new(env!("CARGO_BIN_EXE_adesh"));
    command
        .current_dir(repo)
        .arg("build")
        .arg("tests/fixtures/phase22_i686/wide_refused.adesh")
        .args(["--target", TRIPLE, "--emit=exe", "-o"])
        .arg(&output);
    let compiled = run(&mut command);
    assert!(!compiled.status.success());
    let diagnostics = format!(
        "{}\n{}",
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert!(diagnostics.contains("64-bit integer"), "{diagnostics}");
}

fn inline_case(name: &str, src: &str, expected_stdout: &[u8], expected_exit: i32) {
    let Some(tools) = LinuxTools::discover("as", "qemu-i386") else {
        return;
    };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    for fast in [false, true] {
        let dir = tempfile::Builder::new()
            .prefix(&format!("adesh_i686_{name}_"))
            .tempdir()
            .unwrap();
        let src_path = dir.path().join(format!("{name}.adesh"));
        std::fs::write(&src_path, src).unwrap();
        let output = dir.path().join("app.elf");
        let mut command = Command::new(env!("CARGO_BIN_EXE_adesh"));
        command
            .current_dir(repo)
            .arg("build")
            .arg(&src_path)
            .args(["--target", TRIPLE, "--emit=exe", "-o"])
            .arg(&output);
        if fast {
            command.arg("--fast");
        }
        let compiled = run(&mut command);
        let diagnostics = format!(
            "{}\n{}",
            String::from_utf8_lossy(&compiled.stdout),
            String::from_utf8_lossy(&compiled.stderr)
        );
        if diagnostics.contains("No runtime archive for target")
            && std::env::var_os("ADESH_REQUIRE_FOREIGN_EXEC").is_none()
        {
            eprintln!("SKIP: missing i686 runtime archive");
            return;
        }
        assert!(
            compiled.status.success(),
            "{name}, fast={fast}: {diagnostics}"
        );
        let executed = tools.tool("qemu-i386", &[OsStr::new(&tools.path(&output))]);
        assert_eq!(
            executed.status.code(),
            Some(expected_exit),
            "{name}, fast={fast}: {executed:?}"
        );
        assert_eq!(
            executed.stdout, expected_stdout,
            "{name}, fast={fast}: {executed:?}"
        );
        assert!(
            executed.stderr.is_empty(),
            "{name}, fast={fast}: {executed:?}"
        );
    }
}

fn expect_build_failure(name: &str, src: &str, extra_args: &[&str], expected_substrings: &[&str]) {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = tempfile::Builder::new()
        .prefix(&format!("adesh_i686_neg_{name}_"))
        .tempdir()
        .unwrap();
    let src_path = dir.path().join(format!("{name}.adesh"));
    std::fs::write(&src_path, src).unwrap();
    let output = dir.path().join("app.elf");
    let mut command = Command::new(env!("CARGO_BIN_EXE_adesh"));
    command
        .current_dir(repo)
        .arg("build")
        .arg(&src_path)
        .args(["--target", TRIPLE, "--emit=exe", "-o"])
        .arg(&output)
        .args(extra_args);
    let compiled = run(&mut command);
    let diagnostics = format!(
        "{}\n{}",
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert!(
        !compiled.status.success(),
        "{name} unexpectedly succeeded:\n{diagnostics}"
    );
    for needle in expected_substrings {
        assert!(
            diagnostics.contains(needle),
            "{name}: expected `{needle}` in diagnostics:\n{diagnostics}"
        );
    }
}

#[test]
fn x87_unused_call_results_in_loop_do_not_overflow_stack() {
    inline_case(
        "x87_unused_loop",
        r#"
fn step(x: f64, k: i32) -> f64 {
    if k < 0 {
        return -x;
    }
    if k == 0 {
        return 1.5;
    }
    return x + (k as f64) * 0.5;
}

fn main() -> i32 {
    let i: i32 = 0;
    while i < 16 {
        step(2.0, i);
        i = i + 1;
    }
    let last: f64 = step(2.0, 4);
    let neg: f64 = step(3.5, -1);
    print(last, neg);
    return 0;
}
"#,
        b"4 -3.5\n",
        0,
    );
}

#[test]
fn x87_unsigned_conversions_float_bool_and_signed_narrow_casts_execute() {
    inline_case(
        "x87_conv_bool_cast",
        r#"
fn ret_int_as_f64(n: i32) -> f64 {
    if n < 0 {
        return 0.0;
    }
    return n as f64;
}

fn main() -> i32 {
    let big_u: u32 = 3000000000;
    let big_f: f64 = big_u as f64;
    let back_u: u32 = big_f as u32;
    let u8v: u8 = 250;
    let i8v: i8 = -5 as i8;
    let u16v: u16 = 60000;
    let i16v: i16 = -300 as i16;
    print(big_f, back_u);
    print(u8v as f64, i8v as f64, u16v as f64, i16v as f64);
    print(ret_int_as_f64(42), ret_int_as_f64(-1));

    let z: f64 = 0.0;
    let nz: f64 = -0.0;
    let nan: f64 = z / z;
    let inf: f64 = 1.0 / z;
    let fin: f64 = 1.5;
    print(z as bool, nz as bool, nan as bool, inf as bool, fin as bool);

    let wrap_i8: i8 = 250 as i8;
    let wrap_i16: i16 = 65500 as i16;
    print(wrap_i8, wrap_i16);
    return 0;
}
"#,
        b"3000000000 3000000000\n250 -5 60000 -300\n42 0\nfalse false false true true\n-6 -36\n",
        0,
    );
}

#[test]
fn x87_edge_cases_signed_zero_subnormals_inf_nan_and_comparisons() {
    inline_case(
        "x87_edges",
        r#"
fn main() -> i32 {
    let pz: f64 = 0.0;
    let nz: f64 = -0.0;
    let inf: f64 = 1.0 / pz;
    let ninf: f64 = -1.0 / pz;
    let nan: f64 = pz / pz;
    print(pz, nz, pz == nz, pz != nz);
    print(inf > 1.0, ninf < -1.0, inf + ninf);
    print(nan == nan, nan != nan, nan < 0.0, nan <= 0.0, nan > 0.0, nan >= 0.0);

    // Build a subnormal (~5e-324): divide 1e-308 by 1e16 -> 1e-324 -> round to subnormal.
    let small: f64 = 1.0 / 1000000000.0;
    let tiny: f64 = small * small * small * small; // 1e-36
    let tinier: f64 = tiny * tiny * tiny * tiny * tiny * tiny * tiny * tiny; // 1e-288
    let sub: f64 = tinier / 10000000000000000000000000000000.0; // ~1e-319 (subnormal)
    print(sub > 0.0, sub < 1.0);
    let large: f64 = 100000000000000000000.0 * 100000000000000000000.0;
    print(large > 1.0, -large < -1.0);
    return 0;
}
"#,
        b"0 -0 true false\ntrue true NaN\nfalse true false false false false\ntrue true\ntrue true\n",
        0,
    );
}

#[test]
fn tls_zero_only_tbss_f64_tls_and_no_tls_startup_execute() {
    // Pure .tbss (zero-initialized TLS only) + .bss + strings.
    inline_case(
        "tls_tbss_only",
        r#"
thread_local let t_zero_a: i32 = 0;
thread_local let t_zero_b: f64 = 0.0;
let g_bss: i32 = 0;

fn bump() -> i32 {
    t_zero_a = t_zero_a + 11;
    t_zero_b = t_zero_b + 2.5;
    g_bss = g_bss + 3;
    return t_zero_a + g_bss;
}

fn main() -> i32 {
    let r: i32 = bump();
    print("tbss", t_zero_a, t_zero_b, g_bss, r);
    return r;
}
"#,
        b"tbss 11 2.5 3 14\n",
        14,
    );

    // Mixed initialized .tdata (including f64 + u8) + .tbss + .data + .bss + .rodata + non-TLS global f64.
    inline_case(
        "tls_mixed_tdata_tbss_f64",
        r#"
thread_local let tl_f: f64 = 3.25;
thread_local let tl_tag: u8 = 9;
thread_local let tl_tbss: i32 = 0;
let g_init: i32 = 50;
let g_zero: i32 = 0;
let g_f: f64 = 1.75;

fn touch() -> f64 {
    tl_f = tl_f * 2.0;
    tl_tag = (tl_tag + 1) as u8;
    tl_tbss = tl_tbss + 7;
    g_init = g_init + 5;
    g_zero = g_zero + 2;
    g_f = g_f + 0.5;
    return tl_f;
}

fn main() -> i32 {
    let v: f64 = touch();
    print("mix", v, tl_tag, tl_tbss, g_init, g_zero, g_f);
    return 0;
}
"#,
        b"mix 6.5 10 7 55 2 2.25\n",
        0,
    );
}

#[test]
fn helper_marshalling_narrow_signed_unsigned_and_mixed_widths() {
    inline_case(
        "helper_narrow_mixed",
        r#"
fn main() -> i32 {
    let u8_max: u8 = 255;
    let i8_min: i8 = -128 as i8;
    let u16_max: u16 = 65535;
    let i16_min: i16 = -32768 as i16;
    let u32_hi: u32 = 4000000000;
    let i32_neg: i32 = -2147483647 - 1;
    print("m", u8_max, i8_min, u16_max, i16_min, u32_hi, i32_neg, true, false, -1.25);
    return 0;
}
"#,
        b"m 255 -128 65535 -32768 4000000000 -2147483648 true false -1.25\n",
        0,
    );
}

#[test]
fn unsupported_i686_features_fail_loudly_with_diagnostics() {
    let diag_target = "i686-linux-gnu";
    expect_build_failure(
        "f32_param",
        "fn f(x: f32) -> i32 { return 0; }\nfn main() -> i32 { return 0; }\n",
        &[],
        &["f32", diag_target],
    );
    expect_build_failure(
        "f32_cast",
        "fn main() -> i32 { let x = 1.5 as f32; return 0; }\n",
        &[],
        &["f32", diag_target],
    );
    expect_build_failure(
        "f32_tls",
        "thread_local let x: f32 = 1.0;\nfn main() -> i32 { return 0; }\n",
        &[],
        &["f32", diag_target],
    );
    expect_build_failure(
        "i64_cast",
        "fn main() -> i32 { let x = 5 as i64; return 0; }\n",
        &[],
        &["64-bit integer", diag_target],
    );
    expect_build_failure(
        "u64_tls",
        "thread_local let x: u64 = 5;\nfn main() -> i32 { return 0; }\n",
        &[],
        &["64-bit integer", diag_target],
    );
    expect_build_failure(
        "throw_refused",
        "fn main() -> i32 { throw \"boom\"; return 0; }\n",
        &[],
        &[diag_target],
    );
    expect_build_failure(
        "clock_refused",
        "fn main() -> i32 { let t = clock(); return 0; }\n",
        &[],
        &[diag_target],
    );
    expect_build_failure(
        "shared_refused",
        "fn main() -> i32 { return 0; }\n",
        &["--shared"],
        &["shared"],
    );
}
