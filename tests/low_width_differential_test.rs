//! Milestone 6 differential and property tests for the low-width targets.
//!
//! Seeded, reproducible `i16` programs are generated together with their
//! expected output from a Rust reference evaluator. Two properties are checked:
//!
//! 1. Every generated program builds for every low-width target
//!    (`i686`, `i8086` DOS, `avr`, `dsp56000`).
//! 2. Every target that has an emulator on this machine prints exactly the
//!    reference values (`i686` under `qemu-i386`, `i8086` under `dosbox-x`,
//!    `avr` under `simavr`). DSP56000 has no emulator in this repository, so it
//!    is covered by the build property only.
//!
//! Programs only use values that stay inside the `i16` range at every step, so
//! the reference semantics are unambiguous on every target. Missing emulators
//! panic when `ADESH_REQUIRE_FOREIGN_EXEC=1` is set and otherwise skip loudly.

#[path = "../linker/tests/support/foreign_exec.rs"]
mod foreign_exec;

use foreign_exec::LinuxTools;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const I686: &str = "i686-unknown-linux-gnu";
const I8086: &str = "i8086-unknown-dos";
const AVR: &str = "avr-unknown-unknown";
const DSP56000: &str = "dsp56000-unknown-unknown";

const I16_MIN: i64 = i16::MIN as i64;
const I16_MAX: i64 = i16::MAX as i64;
const PROGRAMS: u64 = 6;
const STATEMENTS: usize = 4;

const MIX_FN: &str = "fn mix(x: i16, y: i16, z: i16) -> i16 {\n    return x - y + z;\n}\n\n";

struct Lcg(u64);

impl Lcg {
    fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }

    fn between(&mut self, lo: i64, hi: i64) -> i64 {
        lo + self.below((hi - lo + 1) as u64) as i64
    }
}

#[derive(Clone, Copy)]
enum Op {
    Add,
    Sub,
    Mul,
}

#[derive(Clone)]
enum Expr {
    Lit(i64),
    Var(usize),
    Bin(Op, Box<Expr>, Box<Expr>),
    Mix(Box<Expr>, Box<Expr>, Box<Expr>),
}

fn in_range(v: i64) -> Option<i64> {
    (I16_MIN..=I16_MAX).contains(&v).then_some(v)
}

impl Expr {
    fn render(&self, names: &[String]) -> String {
        match self {
            Expr::Lit(v) if *v < 0 => format!("(0 - {})", -v),
            Expr::Lit(v) => v.to_string(),
            Expr::Var(i) => names[*i].clone(),
            Expr::Bin(op, l, r) => {
                let sym = match op {
                    Op::Add => "+",
                    Op::Sub => "-",
                    Op::Mul => "*",
                };
                format!("({} {sym} {})", l.render(names), r.render(names))
            }
            Expr::Mix(x, y, z) => format!(
                "mix({}, {}, {})",
                x.render(names),
                y.render(names),
                z.render(names)
            ),
        }
    }

    /// `None` when any intermediate value leaves the `i16` range.
    fn eval(&self, env: &[i64]) -> Option<i64> {
        match self {
            Expr::Lit(v) => in_range(*v),
            Expr::Var(i) => in_range(env[*i]),
            Expr::Bin(op, l, r) => {
                let (a, b) = (l.eval(env)?, r.eval(env)?);
                in_range(match op {
                    Op::Add => a + b,
                    Op::Sub => a - b,
                    Op::Mul => a * b,
                })
            }
            Expr::Mix(x, y, z) => {
                let (x, y, z) = (x.eval(env)?, y.eval(env)?, z.eval(env)?);
                in_range(x - y + z)
            }
        }
    }
}

fn gen_expr(rng: &mut Lcg, depth: u32, nvars: usize) -> Expr {
    if depth == 0 || rng.below(3) == 0 {
        return if rng.below(2) == 0 {
            Expr::Lit(rng.between(-20, 20))
        } else {
            Expr::Var(rng.below(nvars as u64) as usize)
        };
    }
    let sub = |rng: &mut Lcg| Box::new(gen_expr(rng, depth - 1, nvars));
    match rng.below(4) {
        0 => Expr::Bin(Op::Add, sub(rng), sub(rng)),
        1 => Expr::Bin(Op::Sub, sub(rng), sub(rng)),
        2 => Expr::Bin(Op::Mul, sub(rng), sub(rng)),
        _ => Expr::Mix(sub(rng), sub(rng), sub(rng)),
    }
}

struct Program {
    source: String,
    expected: Vec<i64>,
}

fn render_literal(v: i64) -> String {
    if v < 0 {
        format!("(0 - {})", -v)
    } else {
        v.to_string()
    }
}

fn generate_program(seed: u64) -> Program {
    let mut rng = Lcg(seed);
    let mut names: Vec<String> = Vec::new();
    let mut env: Vec<i64> = Vec::new();
    let mut body = String::new();
    body.push_str("fn main() -> i16 {\n");

    for name in ["a", "b", "c"] {
        let v = rng.between(-100, 100);
        body.push_str(&format!("    let {name}: i16 = {};\n", render_literal(v)));
        names.push(name.to_string());
        env.push(v);
    }

    let mut expected = Vec::new();
    for i in 0..STATEMENTS {
        let (expr, value) = (0..64)
            .find_map(|_| {
                let e = gen_expr(&mut rng, 3, names.len());
                e.eval(&env).map(|v| (e, v))
            })
            .unwrap_or((Expr::Lit(7), 7));
        let tname = format!("t{i}");
        body.push_str(&format!(
            "    let {tname}: i16 = {};\n    print({tname});\n",
            expr.render(&names)
        ));
        names.push(tname);
        env.push(value);
        expected.push(value);
    }
    body.push_str("    return 0;\n}\n");

    Program {
        source: format!("{MIX_FN}{body}"),
        expected,
    }
}

fn seeds() -> impl Iterator<Item = u64> {
    (1..=PROGRAMS).map(|i| 0x5EED_0000 + i)
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|pv| {
        std::env::split_paths(&pv)
            .map(|d| d.join(name))
            .find(|p| p.is_file())
    })
}

fn require_tool(name: &str) -> Option<PathBuf> {
    if let Some(p) = find_on_path(name) {
        return Some(p);
    }
    if std::env::var_os("ADESH_REQUIRE_FOREIGN_EXEC").is_some() {
        panic!("{name} is required when ADESH_REQUIRE_FOREIGN_EXEC=1 is set");
    }
    eprintln!("SKIP: {name} is not on PATH; differential execution not run for it");
    None
}

fn build(dir: &Path, source: &str, triple: &str, out_name: &str) -> Output {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src_path = dir.join("prog.adesh");
    std::fs::write(&src_path, source).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_adesh"));
    cmd.current_dir(repo)
        .arg("build")
        .arg(&src_path)
        .args(["--target", triple]);
    if triple == I686 {
        cmd.arg("--emit=exe");
    }
    cmd.arg("-o").arg(dir.join(out_name));
    cmd.output().expect("failed to invoke adesh binary")
}

fn diagnostics(out: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn is_missing_i686_runtime(diag: &str) -> bool {
    diag.contains("No runtime archive for target")
}

fn parse_lines(text: &str) -> Vec<i64> {
    text.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .map(|l| {
            l.parse::<i64>()
                .unwrap_or_else(|_| panic!("non-integer output line {l:?}"))
        })
        .collect()
}

fn parse_simavr_uart(stderr: &[u8]) -> String {
    let raw = String::from_utf8_lossy(stderr);
    let mut clean = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for nc in chars.by_ref() {
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
        .map(|line| line.trim_end_matches('\r').trim_end_matches('.'))
        .collect::<Vec<_>>()
        .join("\n")
}

fn run_dosbox_x(dosbox: &Path, work_dir: &Path, dos_cmd: &str) {
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

fn assert_matches_reference(target: &str, seed: u64, got: &[i64], program: &Program) {
    assert_eq!(
        got, program.expected,
        "{target} output diverges from the reference for seed {seed:#x}\nsource:\n{}",
        program.source
    );
}

#[test]
fn reference_evaluator_rejects_out_of_range_intermediates() {
    let lit = |v| Box::new(Expr::Lit(v));
    let big_mul = Expr::Bin(Op::Mul, lit(300), lit(200));
    assert_eq!(big_mul.eval(&[]), None);
    let min_sub = Expr::Bin(Op::Sub, lit(I16_MIN), lit(1));
    assert_eq!(min_sub.eval(&[]), None);
    let ok = Expr::Mix(lit(5), lit(3), lit(-2));
    assert_eq!(ok.eval(&[]), Some(0));
}

#[test]
fn generated_programs_are_deterministic_and_well_formed() {
    for seed in seeds() {
        let a = generate_program(seed);
        let b = generate_program(seed);
        assert_eq!(a.source, b.source, "seed {seed:#x} is not reproducible");
        assert_eq!(a.expected.len(), STATEMENTS);
        assert!(a.expected.iter().all(|v| (I16_MIN..=I16_MAX).contains(v)));
    }
}

#[test]
fn generated_programs_build_for_every_low_width_target() {
    for seed in seeds() {
        let program = generate_program(seed);
        for (triple, out_name) in [
            (I686, "prog.elf"),
            (I8086, "PROG.EXE"),
            (AVR, "prog.elf"),
            (DSP56000, "prog.lod"),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let out = build(dir.path(), &program.source, triple, out_name);
            let diag = diagnostics(&out);
            if triple == I686 && is_missing_i686_runtime(&diag) {
                assert!(
                    std::env::var_os("ADESH_REQUIRE_FOREIGN_EXEC").is_none(),
                    "i686 runtime archive is required; run scripts/build-runtime-archives.sh --targets {I686}"
                );
                eprintln!("SKIP: i686 runtime archive missing; i686 build not checked");
                continue;
            }
            assert!(
                out.status.success(),
                "seed {seed:#x} failed to build for {triple}:\n{diag}\nsource:\n{}",
                program.source
            );
            assert!(
                dir.path().join(out_name).is_file(),
                "{triple} wrote no output"
            );
        }
    }
}

#[test]
fn differential_execution_matches_reference_on_emulated_targets() {
    let qemu = LinuxTools::discover("as", "qemu-i386");
    let simavr = require_tool("simavr");
    let dosbox = require_tool("dosbox-x");

    for seed in seeds() {
        let program = generate_program(seed);

        if let Some(tools) = &qemu {
            let dir = tempfile::tempdir().unwrap();
            let out = build(dir.path(), &program.source, I686, "prog.elf");
            let diag = diagnostics(&out);
            if is_missing_i686_runtime(&diag) {
                assert!(
                    std::env::var_os("ADESH_REQUIRE_FOREIGN_EXEC").is_none(),
                    "i686 runtime archive is required; run scripts/build-runtime-archives.sh --targets {I686}"
                );
                eprintln!("SKIP: i686 runtime archive missing; i686 execution not run");
            } else {
                assert!(out.status.success(), "i686 build failed:\n{diag}");
                let exe = dir.path().join("prog.elf");
                let run = tools.tool("qemu-i386", &[OsStr::new(&tools.path(&exe))]);
                assert!(
                    run.status.success(),
                    "qemu-i386 exited with {:?}",
                    run.status
                );
                let text = String::from_utf8_lossy(&run.stdout);
                assert_matches_reference(I686, seed, &parse_lines(&text), &program);
            }
        } else if std::env::var_os("ADESH_REQUIRE_FOREIGN_EXEC").is_some() {
            panic!(
                "qemu-i386 and an assembler are required when ADESH_REQUIRE_FOREIGN_EXEC=1 is set"
            );
        } else {
            eprintln!("SKIP: qemu-i386 unavailable; i686 execution not run");
        }

        if let Some(dosbox) = &dosbox {
            let dir = tempfile::tempdir().unwrap();
            let out = build(dir.path(), &program.source, I8086, "PROG.EXE");
            assert!(
                out.status.success(),
                "i8086 build failed:\n{}",
                diagnostics(&out)
            );
            run_dosbox_x(dosbox, dir.path(), "PROG.EXE > OUT.TXT");
            let raw = std::fs::read_to_string(dir.path().join("OUT.TXT"))
                .expect("dosbox-x wrote no output file");
            let text = raw.replace("\r\n", "\n");
            assert_matches_reference(I8086, seed, &parse_lines(&text), &program);
        }

        if let Some(simavr) = &simavr {
            let dir = tempfile::tempdir().unwrap();
            let out = build(dir.path(), &program.source, AVR, "prog.elf");
            assert!(
                out.status.success(),
                "avr build failed:\n{}",
                diagnostics(&out)
            );
            let run = Command::new(simavr)
                .args(["-m", "atmega328p", "-f", "16000000"])
                .arg(dir.path().join("prog.elf"))
                .output()
                .expect("failed to run simavr");
            assert!(
                run.status.success(),
                "simavr exited with {:?}\n{}",
                run.status,
                String::from_utf8_lossy(&run.stderr)
            );
            let text = parse_simavr_uart(&run.stderr);
            assert_matches_reference(AVR, seed, &parse_lines(&text), &program);
        }
    }
}

#[test]
fn f64_print_and_arithmetic_differential_fuzz_on_i686() {
    use adesh_linker::{Linker, config::LinkConfig, target::Target};

    let Some(tools) = LinuxTools::discover("i686-linux-gnu-gcc", "qemu-i386") else {
        return;
    };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let runtime_a = repo
        .join("target")
        .join(I686)
        .join(profile)
        .join("libadesh_runtime.a");
    if !runtime_a.is_file() {
        assert!(
            std::env::var_os("ADESH_REQUIRE_FOREIGN_EXEC").is_none(),
            "missing i686 runtime archive"
        );
        eprintln!("SKIP: missing i686 runtime archive");
        return;
    }

    let mut cases: Vec<u64> = vec![
        0.0f64.to_bits(),
        (-0.0f64).to_bits(),
        1.0f64.to_bits(),
        (-1.0f64).to_bits(),
        0.5f64.to_bits(),
        0.1f64.to_bits(),
        f64::from_bits(1).to_bits(), // min subnormal 5e-324
        0x000f_ffff_ffff_ffff,       // max subnormal
        f64::MIN_POSITIVE.to_bits(), // min normal
        f64::MAX.to_bits(),          // max finite
        (-f64::MAX).to_bits(),
        f64::INFINITY.to_bits(),
        f64::NEG_INFINITY.to_bits(),
        f64::NAN.to_bits(),
        0x7ff8_0000_0000_0042, // NaN with custom payload
        1e-10f64.to_bits(),
        1e10f64.to_bits(),
        999_999_999.999_999_9f64.to_bits(),
        1000.0f64.to_bits(),
        0.000_976_562_5f64.to_bits(), // 2^-10
    ];
    let mut rng = Lcg(0xF640_D1FF_5EED);
    for _ in 0..48 {
        let hi = (rng.next_u64() << 32) | (rng.next_u64() & 0xffff_ffff);
        cases.push(hi);
    }

    let dir = tempfile::Builder::new()
        .prefix("adesh_f64_diff_")
        .tempdir()
        .unwrap();

    // Build a C helper that exposes exact IEEE-754 bit-pattern construction
    // so AdeshLang's `print(f64)` and `f64` arithmetic/comparison ops can be
    // tested on arbitrary finite/subnormal/inf/NaN bit patterns.
    let mut c_table = String::from("#include <stdint.h>\nstatic const uint64_t CASES[] = {\n");
    for bits in &cases {
        c_table.push_str(&format!("    0x{bits:016x}ULL,\n"));
    }
    c_table.push_str(
        "};\ndouble case_val(int32_t idx) {\n    union { uint64_t u; double d; } x;\n    x.u = CASES[idx];\n    return x.d;\n}\n",
    );
    let c_path = dir.path().join("cases.c");
    std::fs::write(&c_path, c_table).unwrap();
    let c_obj = dir.path().join("cases.o");
    let gcc = Command::new("i686-linux-gnu-gcc")
        .args([
            "-O2",
            "-c",
            "-fno-stack-protector",
            "-ffreestanding",
            "-fno-pic",
            "-fno-pie",
        ])
        .arg(&c_path)
        .arg("-o")
        .arg(&c_obj)
        .output()
        .unwrap();
    assert!(gcc.status.success(), "{gcc:?}");

    let adesh_path = dir.path().join("f64_diff.adesh");
    std::fs::write(
        &adesh_path,
        format!(
            r#"
extern "C" fn case_val(idx: i32) -> f64;
fn main() -> i32 {{
    let i: i32 = 0;
    let n: i32 = {};
    while i < n {{
        let v: f64 = case_val(i);
        print(v);
        i = i + 1;
    }}
    // Also verify arithmetic + comparison differential on finite pairs.
    let a: f64 = 12.25;
    let b: f64 = -3.5;
    print(a + b, a - b, a * b, a / b, -b, a > b, a == b);
    return 0;
}}
"#,
            cases.len()
        ),
    )
    .unwrap();

    let adob_path = dir.path().join("f64_diff.adob");
    let compiled = Command::new(env!("CARGO_BIN_EXE_adesh"))
        .current_dir(repo)
        .arg("build")
        .arg(&adesh_path)
        .args(["--target", I686, "--emit=adob", "-o"])
        .arg(&adob_path)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );

    let exe_path = dir.path().join("f64_diff.elf");
    let mut cfg = LinkConfig::new(exe_path.clone(), Target::from_triple(I686).unwrap());
    cfg.dynamic = false;
    Linker::link(&[adob_path, c_obj, runtime_a], cfg).unwrap();

    let run_out = tools.tool("qemu-i386", &[OsStr::new(&tools.path(&exe_path))]);
    assert!(run_out.status.success(), "{run_out:?}");
    let stdout = String::from_utf8_lossy(&run_out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), cases.len() + 1, "stdout:\n{stdout}");

    for (idx, (bits, line)) in cases.iter().zip(lines.iter()).enumerate() {
        let expected = f64::from_bits(*bits);
        if expected.is_nan() {
            assert_eq!(*line, "NaN", "case {idx} (bits=0x{bits:016x})");
        } else if expected == 0.0 {
            let expected_str = if expected.is_sign_negative() {
                "-0"
            } else {
                "0"
            };
            assert_eq!(*line, expected_str, "case {idx} (bits=0x{bits:016x})");
        } else if expected.is_infinite() {
            let expected_str = if expected.is_sign_negative() {
                "-inf"
            } else {
                "inf"
            };
            assert_eq!(*line, expected_str, "case {idx} (bits=0x{bits:016x})");
        } else {
            let parsed: f64 = line.parse().unwrap_or_else(|e| {
                panic!("case {idx} failed to parse `{line}` for bits=0x{bits:016x}: {e}")
            });
            assert_eq!(
                parsed.to_bits(),
                *bits,
                "case {idx} roundtrip bit mismatch: printed `{line}`, got bits=0x{:016x}, expected bits=0x{bits:016x} ({expected:?})",
                parsed.to_bits()
            );
        }
    }

    let a = 12.25f64;
    let b = -3.5f64;
    let expected_arith = format!(
        "{} {} {} {} {} {} {}",
        a + b,
        a - b,
        a * b,
        a / b,
        -b,
        a > b,
        a == b
    );
    assert_eq!(lines[cases.len()], expected_arith);
}

#[test]
fn high_register_pressure_spill_and_call_crossing_differential() {
    let source = r#"
fn leaf(x: i16, y: i16) -> i16 {
    return (x * 3) - y + 5;
}

fn main() -> i16 {
    let v0: i16 = 11;
    let v1: i16 = 22;
    let v2: i16 = -7;
    let v3: i16 = 15;
    let v4: i16 = -9;
    let v5: i16 = 4;
    let v6: i16 = 13;
    let v7: i16 = -2;
    let c1: i16 = leaf(v0, v1);
    let c2: i16 = leaf(v2, v3);
    let acc: i16 = v0 + v1 + v2 + v3 + v4 + v5 + v6 + v7 + c1 + c2;
    let tail: i16 = (v0 - v4) + (v1 - v5) + (v2 - v6) + (v3 - v7);
    print(acc);
    print(tail);
    return 0;
}
"#;
    let (v0, v1, v2, v3, v4, v5, v6, v7): (i64, i64, i64, i64, i64, i64, i64, i64) =
        (11, 22, -7, 15, -9, 4, 13, -2);
    let leaf = |x: i64, y: i64| x * 3 - y + 5;
    let expected = vec![
        v0 + v1 + v2 + v3 + v4 + v5 + v6 + v7 + leaf(v0, v1) + leaf(v2, v3),
        (v0 - v4) + (v1 - v5) + (v2 - v6) + (v3 - v7),
    ];
    let qemu = LinuxTools::discover("as", "qemu-i386");
    let simavr = require_tool("simavr");
    let dosbox = require_tool("dosbox-x");

    if let Some(tools) = &qemu {
        let dir = tempfile::tempdir().unwrap();
        let out = build(dir.path(), source, I686, "spill.elf");
        if !is_missing_i686_runtime(&diagnostics(&out)) {
            assert!(out.status.success(), "{}", diagnostics(&out));
            let run = tools.tool(
                "qemu-i386",
                &[OsStr::new(&tools.path(&dir.path().join("spill.elf")))],
            );
            assert!(run.status.success());
            assert_eq!(parse_lines(&String::from_utf8_lossy(&run.stdout)), expected);
        }
    }
    if let Some(dosbox) = &dosbox {
        let dir = tempfile::tempdir().unwrap();
        let out = build(dir.path(), source, I8086, "SPILL.EXE");
        assert!(out.status.success(), "{}", diagnostics(&out));
        run_dosbox_x(dosbox, dir.path(), "SPILL.EXE > OUT.TXT");
        let text = std::fs::read_to_string(dir.path().join("OUT.TXT"))
            .unwrap()
            .replace("\r\n", "\n");
        assert_eq!(parse_lines(&text), expected);
    }
    if let Some(simavr) = &simavr {
        let dir = tempfile::tempdir().unwrap();
        let out = build(dir.path(), source, AVR, "spill.elf");
        assert!(out.status.success(), "{}", diagnostics(&out));
        let run = Command::new(simavr)
            .args(["-m", "atmega328p", "-f", "16000000"])
            .arg(dir.path().join("spill.elf"))
            .output()
            .unwrap();
        assert!(run.status.success());
        assert_eq!(parse_lines(&parse_simavr_uart(&run.stderr)), expected);
    }
}
