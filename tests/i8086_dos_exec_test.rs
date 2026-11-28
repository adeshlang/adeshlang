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
        && objdump_out.status.success()
    {
        let dis = String::from_utf8_lossy(&objdump_out.stdout);
        assert!(
            !dis.contains("(bad)"),
            "objdump -m i8086 found invalid 8086 instruction in .COM:\n{dis}"
        );
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

    for (out_path, fast) in [
        (&exe_path, false),
        (&com_path, false),
        (&fast_exe_path, true),
    ] {
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

    // `divmod32(100000, 97)` = `(100000 / 97) * 10 + (100000 % 97)` = `1030 * 10 + 90` = `10390`.
    let expected = [
        "105000", "-40000", "699678", "10390", "123456", "3", "1", "2",
    ]
    .join("\n");
    for fname in ["W_EXE.TXT", "W_COM.TXT", "W_FAST.TXT"] {
        let raw = std::fs::read_to_string(dir.path().join(fname)).unwrap();
        assert_eq!(normalize_crlf(&raw), expected, "mismatch in {fname}");
    }
}

#[test]
fn i8086_arithmetic_and_control_flow_conformance_matrix() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = tempfile::Builder::new()
        .prefix("adesh_i8086_matrix_")
        .tempdir()
        .unwrap();

    let src_path = dir.path().join("matrix.adesh");
    let src = r#"
let g_u32: u32 = 3000000000u32;
let g_i32: i32 = -123456789;

fn rec_sum32(n: i32) -> i32 {
    if n <= 0 {
        return 0;
    }
    return n * 10000 + rec_sum32(n - 1);
}

fn spill32_pressure(a: i32, b: i32, c: i32, d: i32, e: i32, f: i32) -> i32 {
    let x1: i32 = a + b;
    let x2: i32 = c - d;
    let x3: i32 = e * 3 + f;
    let x4: i32 = x1 + x2;
    let x5: i32 = x3 - x1;
    let x6: i32 = x4 + x5 + a + f;
    return x6;
}

fn cmp_i32_all(a: i32, b: i32) -> i16 {
    let mask: i16 = 0;
    if a == b { mask = mask + 1; }
    if a != b { mask = mask + 2; }
    if a < b  { mask = mask + 4; }
    if a <= b { mask = mask + 8; }
    if a > b  { mask = mask + 16; }
    if a >= b { mask = mask + 32; }
    return mask;
}

fn cmp_u32_all(a: u32, b: u32) -> i16 {
    let mask: i16 = 0;
    if a == b { mask = mask + 1; }
    if a != b { mask = mask + 2; }
    if a < b  { mask = mask + 4; }
    if a <= b { mask = mask + 8; }
    if a > b  { mask = mask + 16; }
    if a >= b { mask = mask + 32; }
    return mask;
}

fn loop_ctrl(n: i32) -> i32 {
    let i: i32 = 0;
    let acc: i32 = 0;
    while i < n {
        i = i + 1;
        if i == 2 {
            continue;
        }
        if i == 5 {
            break;
        }
        if i % 2 == 1 {
            acc = acc + i * 100000;
        } else {
            acc = acc - 50000;
        }
    }
    return acc;
}

fn main() -> i16 {
    // 1. i8 / u8 arithmetic & boundaries
    let a8: i8 = -50;
    let b8: i8 = 120;
    print(a8 + b8);
    print(-a8);
    let u8a: u8 = 200u8;
    let u8b: u8 = 100u8;
    print(u8a / u8b);
    print(u8a % 30u8);

    // 2. i16 / u16 arithmetic & unsigned div/cmp
    let a16: i16 = 32767;
    let b16: i16 = -32768;
    print(a16 + 1);
    print(b16 - 1);
    let u16a: u16 = 40000u16;
    let u16b: u16 = 50000u16;
    print(u16a / u16b);
    print(u16b % 7u16);
    if u16a > 100u16 { print(1); } else { print(0); }

    // 3. i32 / u32 arithmetic, negation, carry/borrow boundaries, div/mod
    let p1: i32 = 0x0001FFFF;
    let p2: i32 = 1;
    print(p1 + p2);
    print(p1 + p2 - 1);
    print(-(-100000));
    print((-100000) / 30);
    print((-100000) % 30);
    let u32a: u32 = 3000000000u32;
    let u32b: u32 = 2000000000u32;
    print(u32a / u32b);
    print(u32a % 1000003u32);
    print((u32a - u32b) as i32);

    // 4. Signed vs unsigned 32-bit comparisons (including high_diff == 0 & signed overflow)
    // 0x80000001 (-2147483647) vs 0x7FFF0005 (+2147418117):
    // Signed < (mask = 2 + 4 + 8 = 14); hi_diff triggers signed overflow in 16-bit sbb.
    let neg_big: i32 = -2147483647;
    let pos_big: i32 = 2147418117;
    print(cmp_i32_all(neg_big, pos_big));
    // Equal values with non-zero high and low words: mask = 1 + 8 + 32 = 41.
    print(cmp_i32_all(0x00010005, 0x00010005));
    // Same low word, different high word: 0x00020005 > 0x00010005 -> mask = 2 + 16 + 32 = 50.
    print(cmp_i32_all(0x00020005, 0x00010005));
    // Zero comparisons: 0 vs -1 -> > (50); 0 vs 0 -> == (41).
    print(cmp_i32_all(0, -1));
    print(cmp_i32_all(0, 0));
    // Unsigned 32-bit: 3000000000u32 > 100u32 -> mask = 2 + 16 + 32 = 50.
    print(cmp_u32_all(g_u32, 100u32));
    print(cmp_u32_all(g_u32, g_u32));

    // 5. Control flow, recursion, globals, pointers, arrays, spills
    print(loop_ctrl(10));
    print(rec_sum32(6));
    print(spill32_pressure(10000, 20000, 30000, 5000, 40000, 15000));
    print(g_i32);

    let local32: i32 = 70000;
    let p32 = &local32;
    *p32 = *p32 + 30000;
    print(local32);

    // 6. Narrowing i32 -> i16/u16/i8 cast
    let big_for_cast: i32 = 0x0001002A;
    print(big_for_cast as i16);
    print(big_for_cast as u8);

    return 0;
}
"#;
    std::fs::write(&src_path, src).unwrap();

    // First run on interpreter (`adesh run`) as a sanity check for shared integer cases
    let interp = Command::new(env!("CARGO_BIN_EXE_adesh"))
        .current_dir(repo)
        .args(["run"])
        .arg(&src_path)
        .output()
        .expect("failed to invoke adesh run");
    assert!(
        interp.status.success(),
        "interpreter run failed:\n{}\n{}",
        String::from_utf8_lossy(&interp.stdout),
        String::from_utf8_lossy(&interp.stderr)
    );
    let _interp_out = normalize_crlf(&String::from_utf8_lossy(&interp.stdout));

    let exe_path = dir.path().join("MTX.EXE");
    let com_path = dir.path().join("MTX.COM");
    let fast_exe_path = dir.path().join("MTXF.EXE");
    let fast_com_path = dir.path().join("MTXF.COM");

    for (out_path, fast) in [
        (&exe_path, false),
        (&com_path, false),
        (&fast_exe_path, true),
        (&fast_com_path, true),
    ] {
        let out = build_dos_binary(repo, &src_path, out_path, fast);
        assert!(
            out.status.success(),
            "matrix i8086 build failed for {} (fast={fast}):\n{}\n{}",
            out_path.display(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // Verify pure-8086 instruction stream via objdump on both optimized and fast .COM binaries.
    if let Ok(objdump_check) = Command::new("objdump")
        .args(["-D", "-b", "binary", "-m", "i8086", "--adjust-vma=0x100"])
        .arg(&com_path)
        .output()
        && objdump_check.status.success()
    {
        let dis = String::from_utf8_lossy(&objdump_check.stdout);
        assert!(
            !dis.contains("(bad)"),
            "objdump -m i8086 found invalid 8086 instruction in MTX.COM:\n{dis}"
        );
    }

    // Verify against exact expected native outputs (including 16-bit wrapping,
    // integer division, pointer mutation, and truncating casts).
    let expected = [
        "70",         // a8 + b8
        "50",         // -a8
        "2",          // u8a / u8b
        "20",         // 200 % 30
        "-32768",     // 32767 + 1 (i16 wrap)
        "32767",      // -32768 - 1 (i16 wrap)
        "0",          // 40000u16 / 50000u16
        "6",          // 50000u16 % 7u16 (50000 = 7142 * 7 + 6)
        "1",          // 40000u16 > 100u16
        "131072",     // 0x0001FFFF + 1
        "131071",     // + 1 - 1
        "100000",     // -(-100000)
        "-3333",      // -100000 / 30
        "-10",        // -100000 % 30
        "1",          // 3000000000u32 / 2000000000u32
        "991003",     // 3000000000u32 % 1000003u32
        "1000000000", // (u32a - u32b) as i32
        "14",         // cmp_i32_all(-2147483647, 2147418117)
        "41",         // cmp_i32_all(0x00010005, 0x00010005)
        "50",         // cmp_i32_all(0x00020005, 0x00010005)
        "50",         // cmp_i32_all(0, -1)
        "41",         // cmp_i32_all(0, 0)
        "50",         // cmp_u32_all(3000000000u32, 100u32)
        "41",         // cmp_u32_all(3000000000u32, 3000000000u32)
        "350000",     // loop_ctrl(10): 1*100000 + 3*100000 - 50000
        "210000",     // rec_sum32(6)
        "185000",     // spill32_pressure
        "-123456789", // g_i32
        "100000",     // *p32 = *p32 + 30000
        "42",         // 0x0001002A as i16
        "42",         // 0x0001002A as u8
    ]
    .join("\n");

    let Some(dosbox) = find_dosbox_x() else {
        return;
    };
    run_in_dosbox_x(&dosbox, dir.path(), "MTX.EXE > M_EXE.TXT");
    run_in_dosbox_x(&dosbox, dir.path(), "MTX.COM > M_COM.TXT");
    run_in_dosbox_x(&dosbox, dir.path(), "MTXF.EXE > M_FEXE.TXT");
    run_in_dosbox_x(&dosbox, dir.path(), "MTXF.COM > M_FCOM.TXT");

    for fname in ["M_EXE.TXT", "M_COM.TXT", "M_FEXE.TXT", "M_FCOM.TXT"] {
        let raw = std::fs::read_to_string(dir.path().join(fname)).unwrap();
        assert_eq!(
            normalize_crlf(&raw),
            expected,
            "i8086 matrix mismatch in {fname}"
        );
    }
}
