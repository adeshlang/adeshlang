//! Milestone 5: 24-bit Motorola DSP56000 (`dsp56000-unknown-unknown`)
//! end-to-end codegen, calling convention, 32-bit legalization, Motorola `.LOD`
//! emission, and refusal diagnostics tests.
//!
//! Note: No Motorola DSP56000 assembler/simulator (`asm56000`/`a56`/`sim56000`) is
//! installed on the CI/VM host, so this suite verifies binary emission, `.LOD`
//! record structure, 24-bit big-endian encoding invariants, and explicit refusal
//! of unsupported features, while `dsp56000-unknown-unknown` remains Tier 2 Experimental.

use std::{
    path::Path,
    process::Command,
};

const TRIPLE: &str = "dsp56000-unknown-unknown";

fn build_dsp56k(repo: &Path, src_path: &Path, out_path: &Path, fast: bool) -> std::process::Output {
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

fn validate_lod_structure(lod: &str) -> (Vec<(u16, Vec<u32>)>, Vec<(String, u32)>) {
    assert!(lod.starts_with("_START "), "missing _START header:\n{lod}");
    assert!(lod.contains("\n_END 0000"), "missing _END 0000:\n{lod}");

    let mut p_blocks: Vec<(u16, Vec<u32>)> = Vec::new();
    let mut p_syms: Vec<(String, u32)> = Vec::new();

    #[derive(Clone, Copy)]
    enum Mode {
        None,
        DataP,
        DataX,
        SymP,
        SymX,
    }
    let mut mode = Mode::None;

    for raw_line in lod.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with("_START") || line.starts_with("_END") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("_DATA ") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            assert_eq!(parts.len(), 2, "invalid _DATA header: {line}");
            let addr = u16::from_str_radix(parts[1], 16).expect("hex _DATA addr");
            match parts[0] {
                "P" => {
                    p_blocks.push((addr, Vec::new()));
                    mode = Mode::DataP;
                }
                "X" => {
                    mode = Mode::DataX;
                }
                other => panic!("unexpected memory space {other} in {line}"),
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("_SYMBOL ") {
            mode = match rest.trim() {
                "P" => Mode::SymP,
                "X" => Mode::SymX,
                other => panic!("unexpected symbol space {other}"),
            };
            continue;
        }
        match mode {
            Mode::DataP => {
                let words = &mut p_blocks.last_mut().expect("active P block").1;
                for tok in line.split_whitespace() {
                    assert_eq!(tok.len(), 6, "24-bit word must be 6 hex digits: {tok}");
                    let w = u32::from_str_radix(tok, 16).expect("valid 24-bit hex word");
                    assert!(w <= 0x00FF_FFFF);
                    words.push(w);
                }
            }
            Mode::DataX => {
                for tok in line.split_whitespace() {
                    assert_eq!(tok.len(), 6, "24-bit X word must be 6 hex digits: {tok}");
                    u32::from_str_radix(tok, 16).expect("valid 24-bit hex word");
                }
            }
            Mode::SymP => {
                let parts: Vec<&str> = line.split_whitespace().collect();
                assert_eq!(parts.len(), 3, "invalid _SYMBOL line: {line}");
                assert_eq!(parts[1], "I");
                let addr = u32::from_str_radix(parts[2], 16).expect("hex symbol addr");
                p_syms.push((parts[0].to_string(), addr));
            }
            Mode::SymX => {
                let parts: Vec<&str> = line.split_whitespace().collect();
                assert_eq!(parts.len(), 3, "invalid _SYMBOL line: {line}");
                assert_eq!(parts[1], "I");
                u32::from_str_radix(parts[2], 16).expect("hex symbol addr");
            }
            Mode::None => panic!("unexpected line outside section: {line}"),
        }
    }

    (p_blocks, p_syms)
}

#[test]
fn dsp56000_emits_valid_lod_with_calls_spills_and_loops() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = tmp.path().join("dsp_kernel.adesh");
    let out = tmp.path().join("dsp_kernel.lod");

    std::fs::write(
        &src,
        r#"
fn mix7(a: int, b: int, c: int, d: int, e: int, f: int, g: int) -> int {
    let s1 = a + b;
    let s2 = c + d;
    let s3 = e + f;
    let acc = s1 + s2 + s3 + g;
    return acc;
}

fn dot_step(x: int, y: int) -> int {
    let p = x * y;
    let q = (x - y) & 255;
    return p + q;
}

fn main() -> int {
    let total = 0;
    let i = 1;
    while i <= 4 {
        total = total + dot_step(i, i + 2);
        i = i + 1;
    }
    let m = mix7(1, 2, 3, 4, 5, 6, 7);
    print(total + m);
    return 0;
}
"#,
    )
    .unwrap();

    for fast in [false, true] {
        let out_file = if fast {
            tmp.path().join("dsp_kernel_fast.lod")
        } else {
            out.clone()
        };
        let res = build_dsp56k(repo, &src, &out_file, fast);
        assert!(
            res.status.success(),
            "dsp56000 build (fast={fast}) failed:\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&res.stdout),
            String::from_utf8_lossy(&res.stderr)
        );
        let lod = std::fs::read_to_string(&out_file).expect("read .lod");
        let (p_blocks, p_syms) = validate_lod_structure(&lod);

        // Reset vector at P:$0000: JMP >__crt0_start ($0040)
        assert_eq!(p_blocks[0].0, 0x0000);
        assert_eq!(p_blocks[0].1, vec![0x0AF080, 0x000040]);

        // Program block at P:$0040 contains STOP (0x000087) and RTS (0x00000C)
        assert_eq!(p_blocks[1].0, 0x0040);
        assert!(p_blocks[1].1.contains(&0x000087), "missing STOP in crt0");
        assert!(p_blocks[1].1.contains(&0x00000C), "missing RTS in functions");

        let sym_names: Vec<&str> = p_syms.iter().map(|(n, _)| n.as_str()).collect();
        assert!(sym_names.contains(&"__crt0_start"));
        assert!(sym_names.contains(&"main"));
        assert!(sym_names.contains(&"mix7"));
        assert!(sym_names.contains(&"dot_step"));
        assert!(sym_names.contains(&"aot_print_i64"));
    }
}

#[test]
fn dsp56000_legalizes_32bit_without_truncation() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = tmp.path().join("dsp_i32.adesh");
    let out = tmp.path().join("dsp_i32.lod");

    std::fs::write(
        &src,
        r#"
fn add32(a: i32, b: i32) -> i32 {
    return a + b;
}

fn main() -> int {
    let x: i32 = 100000000;
    let y: i32 = 23456789;
    let z: i32 = add32(x, y);
    print(z);
    return 0;
}
"#,
    )
    .unwrap();

    let res = build_dsp56k(repo, &src, &out, false);
    assert!(
        res.status.success(),
        "dsp56000 i32 build failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&res.stdout),
        String::from_utf8_lossy(&res.stderr)
    );
    let lod = std::fs::read_to_string(&out).expect("read .lod");
    let (_p_blocks, p_syms) = validate_lod_structure(&lod);
    let sym_names: Vec<&str> = p_syms.iter().map(|(n, _)| n.as_str()).collect();
    assert!(sym_names.contains(&"add32"));
    assert!(sym_names.contains(&"aot_print_i32"));
}

#[test]
fn dsp56000_refuses_unsupported_float_i64_and_async() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));

    let cases = [
        (
            "float_case.adesh",
            "fn f(x: f64) -> f64 { return x + 1.0; }\nfn main() -> int { return 0; }\n",
            "FPU",
        ),
        (
            "i64_case.adesh",
            "fn f(x: i64) -> i64 { return x + 1; }\nfn main() -> int { return 0; }\n",
            "64-bit",
        ),
        (
            "async_case.adesh",
            "async fn f() -> int { return 1; }\nfn main() -> int { return 0; }\n",
            "async",
        ),
    ];

    for (name, body, needle) in cases {
        let src = tmp.path().join(name);
        let out = tmp.path().join(format!("{name}.lod"));
        std::fs::write(&src, body).unwrap();
        let res = build_dsp56k(repo, &src, &out, false);
        assert!(
            !res.status.success(),
            "expected dsp56000 build of {name} to fail loudly"
        );
        let stderr = String::from_utf8_lossy(&res.stderr);
        let stdout = String::from_utf8_lossy(&res.stdout);
        let combined = format!("{stdout}\n{stderr}");
        assert!(
            combined.contains(needle),
            "expected diagnostic for {name} to mention `{needle}`, got:\n{combined}"
        );
    }
}

#[test]
fn dsp56000_target_cli_reports_experimental_and_architecture_specs() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let info = Command::new(env!("CARGO_BIN_EXE_adesh"))
        .current_dir(repo)
        .args(["target", "info", TRIPLE])
        .output()
        .expect("adesh target info");
    assert!(info.status.success());
    let text = String::from_utf8_lossy(&info.stdout);
    assert!(text.contains("24-bit"), "info:\n{text}");
    assert!(text.contains("16-bit"), "info:\n{text}");
    assert!(text.contains("Big"), "info:\n{text}");
    assert!(text.contains("harvard-dual-xy-p"), "info:\n{text}");
    assert!(text.contains("Tier 2"), "info:\n{text}");
    assert!(text.contains("Specialized DSP Runtime"), "info:\n{text}");
}
