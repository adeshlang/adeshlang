//! Milestone 4: 8-bit AVR (`avr-unknown-unknown`, ATmega328P `avr5`) firmware
//! generation (ELF32 `EM_AVR` & Intel HEX `.hex`), disassembly validation,
//! and execution under `simavr`.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

const TRIPLE: &str = "avr-unknown-unknown";

fn find_simavr() -> Option<PathBuf> {
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join("simavr");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    if std::env::var_os("ADESH_REQUIRE_FOREIGN_EXEC").is_some() {
        panic!("simavr is required when ADESH_REQUIRE_FOREIGN_EXEC=1 is set");
    }
    None
}

fn build_avr_binary(
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

/// Strip ANSI color sequences emitted by `simavr`'s UART stdio console and
/// normalize `simavr`'s control-char `.` marker before `\n` (when UART sends `\r\n` or `\n`).
fn parse_simavr_uart(stderr: &[u8]) -> Vec<String> {
    let raw = String::from_utf8_lossy(stderr);
    // Strip ANSI CSI sequences like \x1b[32m and \x1b[0m
    let mut clean = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            while let Some(&nc) = chars.peek() {
                chars.next();
                if nc.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            clean.push(c);
        }
    }
    clean
        .lines()
        .map(|line| {
            line.trim_end_matches('\r')
                .trim_end_matches('.')
                .to_string()
        })
        .filter(|line| !line.is_empty())
        .collect()
}

fn run_in_simavr(simavr: &Path, elf_path: &Path) -> Vec<String> {
    let out = Command::new(simavr)
        .args(["-m", "atmega328p", "-f", "16000000"])
        .arg(elf_path)
        .output()
        .expect("failed to run simavr");
    assert!(
        out.status.success(),
        "simavr exited with {:?}\nstdout:\n{}\nstderr:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    parse_simavr_uart(&out.stderr)
}

#[test]
fn avr_atmega328p_elf_and_hex_build_validate_and_execute_in_simavr() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = tempfile::Builder::new()
        .prefix("adesh_avr_exec_")
        .tempdir()
        .unwrap();

    let src_path = dir.path().join("firmware.adesh");
    let src = r#"
let g_count: i16 = 15;
let g_wide: i32 = 100000;
let g_zero: i16 = 0;

fn add3(a: i16, b: i16, c: i16) -> i16 {
    return a + b + c;
}

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

fn fib(n: i16) -> i16 {
    if n <= 1 {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

fn pressure_spill(a: i16, b: i16, c: i16, d: i16, e: i16, f: i16, g: i16, h: i16, i: i16, j: i16) -> i16 {
    let t1: i16 = a + b * 2;
    let t2: i16 = c * 3 - d;
    let t3: i16 = e + f * 4;
    let t4: i16 = g * 2 + h;
    let t5: i16 = i + j * 3;
    return t1 + t2 + t3 + t4 + t5 + a + j;
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

fn unsigned_ops(x: u32, y: u32) -> u32 {
    let sum: u32 = x + y;
    let shifted: u32 = (sum >> 2) << 1;
    if x > y {
        return shifted + 1;
    }
    return shifted;
}

fn byte_ops(a: u8, b: i8) -> i16 {
    let ua: i16 = a as i16;
    let sb: i16 = b as i16;
    return ua + sb;
}

fn main() -> i16 {
    print("avr-boot");
    print(add3(10, 20, 30));
    print(fib(9));
    print(pressure_spill(1, 2, 3, 4, 5, 6, 7, 8, 9, 10));
    print(-42);
    print((100 - 25) / 5);
    print(29 % 7);
    print((0x55 & 0x0F) | 0x40 ^ 0x02);
    print((3 << 4) >> 2);
    print(loop_sum(6));
    print(g_count);
    print(bump_global(7));
    print(g_zero);

    print(add32(70000, 35000));
    print(sub32(50000, 90000));
    print(mul32(1234, 567));
    print(divmod32(100007, 100));
    print(g_wide + 23456);
    print(unsigned_ops(4000000000u32, 100000u32));
    print(byte_ops(250u8, -10i8));

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

    let elf_path = dir.path().join("firmware.elf");
    let hex_path = dir.path().join("firmware.hex");
    let fast_elf_path = dir.path().join("firmware_fast.elf");

    for (out_path, fast) in [
        (&elf_path, false),
        (&hex_path, false),
        (&fast_elf_path, true),
    ] {
        let out = build_avr_binary(repo, &src_path, out_path, fast);
        assert!(
            out.status.success(),
            "adesh build failed for {} (fast={fast}):\n{}\n{}",
            out_path.display(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // Validate ELF32 EM_AVR header
    let elf_bytes = std::fs::read(&elf_path).unwrap();
    assert!(elf_bytes.len() >= 52);
    assert_eq!(&elf_bytes[0..4], b"\x7fELF");
    assert_eq!(elf_bytes[4], 1, "expected ELF32 class");
    assert_eq!(elf_bytes[5], 1, "expected little-endian ELF");
    assert_eq!(
        u16::from_le_bytes([elf_bytes[16], elf_bytes[17]]),
        2,
        "expected ET_EXEC"
    );
    assert_eq!(
        u16::from_le_bytes([elf_bytes[18], elf_bytes[19]]),
        83,
        "expected EM_AVR (83)"
    );
    assert_eq!(
        u32::from_le_bytes([elf_bytes[36], elf_bytes[37], elf_bytes[38], elf_bytes[39]]),
        0x05,
        "expected avr5 e_flags"
    );

    // Validate Intel HEX format
    let hex_str = std::fs::read_to_string(&hex_path).unwrap();
    let hex_lines: Vec<&str> = hex_str.lines().collect();
    assert!(hex_lines.len() >= 2, "expected multi-line Intel HEX");
    assert_eq!(
        *hex_lines.last().unwrap(),
        ":00000001FF",
        "missing Intel HEX EOF record"
    );
    for line in &hex_lines {
        assert!(line.starts_with(':'), "invalid Intel HEX line: {line}");
    }

    // Optional avr-objdump disassembly sanity check (stopping at __rodata_start so
    // Flash string literals aren't decoded as opcodes).
    if let Ok(nm_out) = Command::new("avr-nm").arg(&elf_path).output() {
        let stop_arg = String::from_utf8_lossy(&nm_out.stdout)
            .lines()
            .find(|l| l.ends_with(" __rodata_start"))
            .and_then(|l| l.split_whitespace().next())
            .map(|addr| format!("--stop-address=0x{addr}"));
        let mut cmd = Command::new("avr-objdump");
        cmd.args(["-d", "-m", "avr:5"]);
        if let Some(ref sa) = stop_arg {
            cmd.arg(sa);
        }
        cmd.arg(&elf_path);
        if let Ok(objdump_out) = cmd.output() {
            if objdump_out.status.success() {
                let dis = String::from_utf8_lossy(&objdump_out.stdout);
                assert!(
                    !dis.contains(".word") && !dis.contains("unknown"),
                    "avr-objdump found undecodable opcode in ELF:\n{dis}"
                );
            }
        }
    }

    let Some(simavr) = find_simavr() else {
        return;
    };

    let expected: Vec<&str> = vec![
        "avr-boot",
        "60",
        "34",
        "111",
        "-42",
        "15",
        "1",
        "71",
        "12",
        "18",
        "15",
        "22",
        "0",
        "105000",
        "-40000",
        "699678",
        "10007",
        "123456",
        "2000050001",
        "240",
        "true",
        "false",
        "100",
        "30",
    ];

    for path in [&elf_path, &fast_elf_path] {
        let actual = run_in_simavr(&simavr, path);
        assert_eq!(
            actual,
            expected,
            "simavr UART output mismatch for {}",
            path.display()
        );
    }
}

#[test]
fn avr_refuses_64bit_integers_and_floating_point_cleanly() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = tempfile::Builder::new()
        .prefix("adesh_avr_refuse_")
        .tempdir()
        .unwrap();

    for (name, src, needle) in [
        (
            "i64.adesh",
            "fn main() -> i16 { let x: i64 = 1234567890123; print(x); return 0; }\n",
            "64-bit",
        ),
        (
            "f64.adesh",
            "fn main() -> i16 { let x: f64 = 1.5; print(x); return 0; }\n",
            "float",
        ),
    ] {
        let src_path = dir.path().join(name);
        std::fs::write(&src_path, src).unwrap();
        let out = build_avr_binary(repo, &src_path, &dir.path().join("out.elf"), false);
        assert!(!out.status.success(), "AVR must refuse {name}");
        let err = format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            err.to_lowercase().contains(needle) || err.to_lowercase().contains("avr"),
            "expected descriptive AVR refusal for {name} (containing '{needle}'), got:\n{err}"
        );
    }
}

#[test]
fn avr_atmega2560_builds_avr6_elf_and_hex_and_executes_in_simavr() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = tempfile::Builder::new()
        .prefix("adesh_avr_2560_")
        .tempdir()
        .unwrap();
    let src_path = dir.path().join("mega2560.adesh");
    let src = r#"
let g_val: i16 = 40;

fn sum10(a: i16, b: i16, c: i16, d: i16, e: i16, f: i16, g: i16, h: i16, i: i16, j: i16) -> i16 {
    return a + b + c + d + e + f + g + h + i + j;
}

fn main() -> i16 {
    print(sum10(1, 2, 3, 4, 5, 6, 7, 8, 9, 10));
    print(g_val + 2);
    return 0;
}
"#;
    std::fs::write(&src_path, src).unwrap();

    let elf_path = dir.path().join("mega2560.elf");
    let out = Command::new(env!("CARGO_BIN_EXE_adesh"))
        .current_dir(repo)
        .arg("build")
        .arg(&src_path)
        .args(["--target", "atmega2560-unknown-unknown", "-o"])
        .arg(&elf_path)
        .output()
        .expect("failed to invoke adesh binary");
    assert!(
        out.status.success(),
        "atmega2560 ELF build failed:\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let elf = std::fs::read(&elf_path).unwrap();
    assert_eq!(&elf[0..4], b"\x7fELF");
    assert_eq!(u16::from_le_bytes([elf[18], elf[19]]), 83); // EM_AVR
    assert_eq!(u32::from_le_bytes([elf[36], elf[37], elf[38], elf[39]]), 0x06); // EF_AVR_ARCH_AVR6

    let hex_path = dir.path().join("mega2560.hex");
    let hex_out = Command::new(env!("CARGO_BIN_EXE_adesh"))
        .current_dir(repo)
        .arg("build")
        .arg(&src_path)
        .args(["--target", "atmega2560-unknown-unknown", "-o"])
        .arg(&hex_path)
        .output()
        .expect("failed to invoke adesh binary");
    assert!(hex_out.status.success(), "atmega2560 HEX build failed");
    let hex = std::fs::read_to_string(&hex_path).unwrap();
    assert!(hex.ends_with(":00000001FF\n") || hex.ends_with(":00000001FF\r\n"));

    if let Some(simavr) = find_simavr() {
        let sim = Command::new(simavr)
            .args(["-m", "atmega2560", "-f", "16000000"])
            .arg(&elf_path)
            .output()
            .expect("failed to run simavr");
        assert!(
            sim.status.success(),
            "simavr atmega2560 failed:\n{}",
            String::from_utf8_lossy(&sim.stderr)
        );
        let lines = parse_simavr_uart(&sim.stderr);
        assert_eq!(lines, vec!["55", "42"]);
    }
}
