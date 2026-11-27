//! Milestone 3: 16-bit Intel 8086 real-mode (`i8086-unknown-dos`) executable
//! generation (`.EXE` MZ Small Model and `.COM`), machine-code verification, and
//! execution under `dosbox-x`.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

const TRIPLE: &str = "i8086-unknown-dos";

fn find_dosbox_x() -> Option<PathBuf> {
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join("dosbox-x");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    if std::env::var_os("ADESH_REQUIRE_FOREIGN_EXEC").is_some() {
        panic!("dosbox-x is required when ADESH_REQUIRE_FOREIGN_EXEC=1 is set");
    }
    None
}

fn build_dos_binary(
    repo: &Path,
    src_path: &Path,
    out_path: &Path,
    fast: bool,
) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_adesh"));
    cmd.current_dir(repo)
        .arg("build")
        .arg(src_path)
        .args(["--target", TRIPLE, "-o"])
        .arg(out_path);
    if fast {
        cmd.arg("--fast");
    }
    cmd.output().expect("failed to invoke adesh binary")
}

fn run_in_dosbox_x(dosbox: &Path, work_dir: &Path, dos_cmd: &str) {
    let out = Command::new(dosbox)
        .env("SDL_VIDEODRIVER", "dummy")
        .env("SDL_AUDIODRIVER", "dummy")
        .args([
            "-nogui",
            "-silent",
            "-c",
            &format!("mount c {}", work_dir.display()),
            "-c",
            "c:",
            "-c",
            dos_cmd,
            "-c",
            "exit",
        ])
        .output()
        .expect("failed to run dosbox-x");
    assert!(
        out.status.success(),
        "dosbox-x exited with {:?}\nstdout:\n{}\nstderr:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn normalize_crlf(s: &str) -> String {
    s.replace("\r\n", "\n").trim_end().to_string()
}

#[test]
fn i8086_mz_exe_and_com_build_validate_and_execute_in_dosbox_x() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = tempfile::Builder::new()
        .prefix("adesh_i8086_exec_")
        .tempdir()
        .unwrap();

    let src_path = dir.path().join("prog.adesh");
    let src = r#"
let g_count: i16 = 15;

fn add3(a: i16, b: i16, c: i16) -> i16 {
    return a + b + c;
}

fn fib(n: i16) -> i16 {
    if n <= 1 {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

fn pressure6(a: i16, b: i16, c: i16, d: i16, e: i16, f: i16) -> i16 {
    let t1: i16 = a * 2 + b;
    let t2: i16 = c * 3 - d;
    let t3: i16 = e + f * 4;
    let t4: i16 = t1 + t2 + t3;
    return t4 + a + f;
}

fn bump_global(delta: i16) -> i16 {
    g_count = g_count + delta;
    return g_count;
}

fn loop_sum(limit: i16) -> i16 {
    let i: i16 = 0;
    let acc: i16 = 0;
    while i < limit {
        i = i + 1;
        if i == 3 {
            continue;
        }
        acc = acc + i;
    }
    return acc;
}

fn main() -> i16 {
    print(add3(10, 20, 30));
    print(fib(10));
    print(pressure6(1, 2, 3, 4, 5, 6));
    print(-42);
    print((100 - 25) / 5);
    print(29 % 7);
    print((0x55 & 0x0F) | 0x40 ^ 0x02);
    print((3 << 4) >> 2);
    print(loop_sum(6));
    print(g_count);
    print(bump_global(7));
    print(true);
    print(false);

    let target: i16 = 77;
    let ptr = &target;
    *ptr = *ptr + 23;
    print(target);

    let gp = &g_count;
    *gp = *gp + 8;
    print(g_count);

    return 0;
}
"#;
    std::fs::write(&src_path, src).unwrap();

    let exe_path = dir.path().join("PROG.EXE");
    let com_path = dir.path().join("PROG.COM");
    let fast_exe_path = dir.path().join("FAST.EXE");

    for (out_path, fast) in [
        (&exe_path, false),
        (&com_path, false),
        (&fast_exe_path, true),
    ] {
        let out = build_dos_binary(repo, &src_path, out_path, fast);
        assert!(
            out.status.success(),
            "adesh build failed for {} (fast={fast}):\n{}\n{}",
            out_path.display(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // Validate MZ header structure on PROG.EXE
    let exe_bytes = std::fs::read(&exe_path).unwrap();
    assert!(exe_bytes.len() >= 32, "MZ EXE too short");
    assert_eq!(&exe_bytes[0..2], b"MZ", "missing MZ magic header");
    let cparhdr = u16::from_le_bytes([exe_bytes[8], exe_bytes[9]]);
    let minalloc = u16::from_le_bytes([exe_bytes[10], exe_bytes[11]]);
    let e_ss = u16::from_le_bytes([exe_bytes[14], exe_bytes[15]]);
    let e_sp = u16::from_le_bytes([exe_bytes[16], exe_bytes[17]]);
    assert!(cparhdr >= 2, "expected valid MZ header paragraph count");
    assert!(
        minalloc > 0,
        "expected non-zero MZ e_minalloc for DGROUP/stack"
    );
    assert!(e_ss > 0, "expected Small Model separate DGROUP SS segment");
    assert_eq!(e_sp & 1, 0, "expected even initial SP");

    // Validate .COM non-empty and fits inside 64 KiB real-mode segment
    let com_bytes = std::fs::read(&com_path).unwrap();
    assert!(!com_bytes.is_empty() && com_bytes.len() <= 0xFF00);

    // Optional objdump disassembly verification (8086 mode)
    if let Ok(objdump_out) = Command::new("objdump")
        .args(["-D", "-b", "binary", "-m", "i8086", "--adjust-vma=0x100"])
        .arg(&com_path)
        .output()
    {
        if objdump_out.status.success() {
            let dis = String::from_utf8_lossy(&objdump_out.stdout);
            assert!(
                !dis.contains("(bad)"),
                "objdump -m i8086 found invalid 8086 instruction in .COM:\n{dis}"
            );
        }
    }

    let Some(dosbox) = find_dosbox_x() else {
        return;
    };

    run_in_dosbox_x(&dosbox, dir.path(), "PROG.EXE > OUT_EXE.TXT");
    run_in_dosbox_x(&dosbox, dir.path(), "PROG.COM > OUT_COM.TXT");
    run_in_dosbox_x(&dosbox, dir.path(), "FAST.EXE > OUT_FAST.TXT");

    let expected = [
        "60", "55", "45", "-42", "15", "1", "71", "12", "18", "15", "22", "true", "false", "100",
        "30",
    ]
    .join("\n");

    for fname in ["OUT_EXE.TXT", "OUT_COM.TXT", "OUT_FAST.TXT"] {
        let raw = std::fs::read_to_string(dir.path().join(fname))
            .unwrap_or_else(|e| panic!("missing {fname}: {e}"));
        let actual = normalize_crlf(&raw);
        assert_eq!(actual, expected, "output mismatch in {fname}");
    }
}

#[test]
fn i8086_refuses_hardware_float_and_overwide_types_cleanly() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = tempfile::Builder::new()
        .prefix("adesh_i8086_refuse_")
        .tempdir()
        .unwrap();

    let float_src = dir.path().join("float.adesh");
    std::fs::write(
        &float_src,
        "fn main() -> i16 { let x: f64 = 1.5; print(x); return 0; }\n",
    )
    .unwrap();
    let float_out = build_dos_binary(repo, &float_src, &dir.path().join("FLOAT.COM"), false);
    assert!(
        !float_out.status.success(),
        "i8086 must refuse f64 without hardware FPU"
    );
    let float_err = format!(
        "{}\n{}",
        String::from_utf8_lossy(&float_out.stdout),
        String::from_utf8_lossy(&float_out.stderr)
    );
    assert!(
        float_err.contains("i8086") || float_err.contains("float"),
        "expected descriptive i8086 float refusal, got:\n{float_err}"
    );

    let i64_src = dir.path().join("i64.adesh");
    std::fs::write(
        &i64_src,
        "fn main() -> i16 { let x: i64 = 1234567890123; print(x); return 0; }\n",
    )
    .unwrap();
    let i64_out = build_dos_binary(repo, &i64_src, &dir.path().join("I64.COM"), false);
    assert!(!i64_out.status.success(), "i8086 must refuse i64");
}

#[test]
fn i8086_32bit_legalized_exe_com_and_fast_build_and_execute() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = tempfile::Builder::new()
        .prefix("adesh_i8086_32bit_")
        .tempdir()
        .unwrap();

    let src_path = dir.path().join("wide32.adesh");
    let src = r#"
let g_wide: i32 = 100000;

fn add32(a: i32, b: i32) -> i32 {
    return a + b;
}

fn sub32(a: i32, b: i32) -> i32 {
    return a - b;
}

fn mul32(a: i32, b: i32) -> i32 {
    return a * b;
}

fn divmod32(a: i32, b: i32) -> i32 {
    return (a / b) * 10 + (a % b);
}

fn cmp32(a: i32, b: i32) -> i16 {
    if a == b {
        return 1;
    }
    if a < b {
        return 2;
    }
    return 3;
}

fn main() -> i16 {
    print(add32(70000, 35000));
    print(sub32(10000, 50000));
    print(mul32(1234, 567));
    print(divmod32(100000, 97));
    print(g_wide + 23456);
    print(cmp32(0x00020000, 0x00010001));
    print(cmp32(0x00010005, 0x00010005));
    print(cmp32(-50000, 50000));
    return 0;
}
"#;
    std::fs::write(&src_path, src).unwrap();

    let exe_path = dir.path().join("WIDE.EXE");
    let com_path = dir.path().join("WIDE.COM");
    let fast_exe_path = dir.path().join("WFAST.EXE");

    for (out_path, fast) in [(&exe_path, false), (&com_path, false), (&fast_exe_path, true)] {
        let out = build_dos_binary(repo, &src_path, out_path, fast);
        assert!(
            out.status.success(),
            "32-bit i8086 build failed for {} (fast={fast}):\n{}\n{}",
            out_path.display(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let exe_bytes = std::fs::read(&exe_path).unwrap();
    assert_eq!(&exe_bytes[0..2], b"MZ");
    let com_bytes = std::fs::read(&com_path).unwrap();
    assert!(!com_bytes.is_empty() && com_bytes.len() < 65000);

    let Some(dosbox) = find_dosbox_x() else {
        return;
    };
    run_in_dosbox_x(&dosbox, dir.path(), "WIDE.EXE > W_EXE.TXT");
    run_in_dosbox_x(&dosbox, dir.path(), "WIDE.COM > W_COM.TXT");
    run_in_dosbox_x(&dosbox, dir.path(), "WFAST.EXE > W_FAST.TXT");

    let expected = ["105000", "-40000", "699678", "10309", "123456", "3", "1", "2"].join("\n");
    for fname in ["W_EXE.TXT", "W_COM.TXT", "W_FAST.TXT"] {
        let raw = std::fs::read_to_string(dir.path().join(fname)).unwrap();
        assert_eq!(normalize_crlf(&raw), expected, "mismatch in {fname}");
    }
}
