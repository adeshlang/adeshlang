//! Unified low-width native toolchain conformance suite (Milestone 6).
//!
//! 1. Architecture descriptors keep register width, pointer width, address width,
//!    bus width and memory model as separate facts for 8/16/24/32/64-bit targets.
//! 2. `adesh target` reports the verified profile, memory model and tier.
//! 3. Non-FullNative runtime profiles refuse unsupported language features at
//!    compile time with a named target diagnostic.
//! 4. Shared integer-legalization fixtures build for the low-width targets
//!    (object/image generation only). Emulator execution is covered by the
//!    per-target exec suites (avr_simavr_exec_test, i8086_dos_exec_test,
//!    native_phase22_i686_exec_test); DSP56000 has no emulator in this repo.

use adesh_object::TargetDescriptor;
use std::path::Path;
use std::process::{Command, Output};

fn run_adesh(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_adesh"))
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")))
        .args(args)
        .output()
        .expect("failed to execute adesh binary")
}

fn descriptor(triple: &str) -> adesh_object::ArchitectureDescriptor {
    TargetDescriptor::from_triple(triple)
        .unwrap_or_else(|e| panic!("{triple}: {e}"))
        .arch_descriptor()
        .clone()
}

#[test]
fn descriptors_separate_register_pointer_address_bus_and_memory_model() {
    let avr = descriptor("avr-unknown-unknown");
    assert_eq!(avr.register_width_bits, 8);
    assert_eq!(avr.pointer_width_bits, 16);
    assert_eq!(avr.data_layout.data_bus_width_bits, 8);
    assert_eq!(avr.memory_model.as_str(), "harvard-flash-sram");
    assert_eq!(
        avr.runtime_profile.as_str(),
        "Profile C: Bare-Metal Embedded Runtime"
    );

    let i8086 = descriptor("i8086-unknown-dos");
    assert_eq!(i8086.register_width_bits, 16);
    assert_eq!(i8086.address_width_bits, 20);
    assert_eq!(i8086.memory_model.as_str(), "segmented-real-mode");
    assert_eq!(
        i8086.runtime_profile.as_str(),
        "Profile B: Minimal Native Runtime (16-bit)"
    );

    let dsp = descriptor("dsp56000-unknown-unknown");
    assert_eq!(dsp.register_width_bits, 24);
    assert_eq!(dsp.memory_model.as_str(), "harvard-dual-xy-p");
    assert_eq!(
        dsp.runtime_profile.as_str(),
        "Profile D: Specialized DSP Runtime"
    );

    let i686 = descriptor("i686-unknown-linux-gnu");
    assert_eq!(i686.register_width_bits, 32);
    assert_eq!(i686.pointer_width_bits, 32);
    assert_eq!(i686.memory_model.as_str(), "flat");
    assert_eq!(
        i686.runtime_profile.as_str(),
        "Profile A: Full Native Runtime"
    );

    let x64 = descriptor("x86_64-unknown-linux-gnu");
    assert_eq!(x64.register_width_bits, 64);
    assert_eq!(x64.pointer_width_bits, 64);
    assert_eq!(
        x64.runtime_profile.as_str(),
        "Profile A: Full Native Runtime"
    );
}

#[test]
fn target_cli_reports_profile_memory_model_and_tier() {
    let list = run_adesh(&["target", "list"]);
    assert!(list.status.success(), "target list failed");
    let list_txt = String::from_utf8_lossy(&list.stdout);
    for triple in [
        "i686-unknown-linux-gnu",
        "i8086-unknown-dos",
        "avr-unknown-unknown",
        "dsp56000-unknown-unknown",
        "x86_64-unknown-linux-gnu",
    ] {
        assert!(
            list_txt.contains(triple),
            "missing {triple} in target list:\n{list_txt}"
        );
    }

    for (triple, profile, memory) in [
        (
            "avr-unknown-unknown",
            "Bare-Metal Embedded",
            "harvard-flash-sram",
        ),
        ("i8086-unknown-dos", "Minimal Native", "segmented-real-mode"),
        (
            "dsp56000-unknown-unknown",
            "Specialized DSP",
            "harvard-dual-xy-p",
        ),
        ("i686-unknown-linux-gnu", "Full Native", "flat"),
    ] {
        let info = run_adesh(&["target", "info", triple]);
        assert!(info.status.success(), "target info {triple} failed");
        let txt = String::from_utf8_lossy(&info.stdout);
        assert!(txt.contains(profile), "{triple} info:\n{txt}");
        assert!(txt.contains(memory), "{triple} info:\n{txt}");

        let caps = run_adesh(&["target", "capabilities", triple]);
        assert!(caps.status.success(), "target capabilities {triple} failed");
        let feats = run_adesh(&["target", "features", triple]);
        assert!(feats.status.success(), "target features {triple} failed");
    }
}

#[test]
fn low_width_profiles_refuse_unsupported_runtime_features_at_compile_time() {
    let dir = tempfile::Builder::new()
        .prefix("adesh_rt_gate_")
        .tempdir()
        .unwrap();

    let cases: [(&str, &str); 4] = [
        (
            "exceptions",
            "fn main() -> int { throw \"boom\"; return 0; }\n",
        ),
        (
            "clock_time",
            "fn main() -> int { let t = clock(); return 0; }\n",
        ),
        (
            "async_fn",
            "async fn worker() -> int { return 1; }\nfn main() -> int { return 0; }\n",
        ),
        (
            "float_f64",
            "fn main() -> int { let x: f64 = 1.5; print(x); return 0; }\n",
        ),
    ];

    for triple in [
        "i8086-unknown-dos",
        "avr-unknown-unknown",
        "dsp56000-unknown-unknown",
    ] {
        for (name, src) in cases {
            let src_path = dir
                .path()
                .join(format!("{}_{name}.adesh", triple.replace('-', "_")));
            let out_path = dir
                .path()
                .join(format!("{}_{name}.out", triple.replace('-', "_")));
            std::fs::write(&src_path, src).unwrap();

            let out = run_adesh(&[
                "build",
                src_path.to_str().unwrap(),
                "--target",
                triple,
                "-o",
                out_path.to_str().unwrap(),
            ]);
            let combined = format!(
                "{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(
                !out.status.success(),
                "{triple} accepted `{name}` but must refuse it:\n{combined}"
            );
            let canonical = TargetDescriptor::from_triple(triple)
                .unwrap()
                .triple_string();
            assert_eq!(
                canonical, triple,
                "{triple} must be spelled canonically in diagnostics"
            );
            assert!(
                combined.contains(&canonical),
                "{triple} / {name} diagnostic must name the canonical target `{canonical}`:\n{combined}"
            );
        }
    }
}

const WIDE_FIXTURE: &str = r#"
fn test_32() -> i32 {
    let x: i32 = 100000;
    let y: i32 = 23456;
    let sum: i32 = x + y;
    let diff: i32 = sum - 3456;
    let prod: i32 = 300 * 401;
    let quot: i32 = prod / 100;
    let rem: i32 = prod % 97;
    let sh: i32 = (4660 << 4) | 15;
    let sr: i32 = sh >> 4;
    return diff + quot + rem + sr;
}

fn main() -> i16 {
    print(test_32());
    return 0;
}
"#;

const NARROW_FIXTURE: &str = r#"
fn add3(a: i16, b: i16, c: i16) -> i16 {
    return a + b + c;
}

fn main() -> i16 {
    let x: i16 = add3(1000, 234, 56);
    print(x * 3 - 100);
    return 0;
}
"#;

fn build_fixture(dir: &Path, name: &str, src: &str, triple: &str, out: &str) -> Output {
    let src_path = dir.join(name);
    std::fs::write(&src_path, src).unwrap();
    run_adesh(&[
        "build",
        src_path.to_str().unwrap(),
        "--target",
        triple,
        "-o",
        dir.join(out).to_str().unwrap(),
    ])
}

#[test]
fn wide_integer_legalization_builds_on_8_bit_and_24_bit_targets() {
    let dir = tempfile::Builder::new()
        .prefix("adesh_int_legal_wide_")
        .tempdir()
        .unwrap();

    let avr = build_fixture(
        dir.path(),
        "wide.adesh",
        WIDE_FIXTURE,
        "avr-unknown-unknown",
        "wide_avr.elf",
    );
    assert!(
        avr.status.success(),
        "avr 32-bit legalization build failed: {}",
        String::from_utf8_lossy(&avr.stderr)
    );

    let dsp = build_fixture(
        dir.path(),
        "wide.adesh",
        WIDE_FIXTURE,
        "dsp56000-unknown-unknown",
        "wide_dsp.lod",
    );
    assert!(
        dsp.status.success(),
        "dsp56000 32-bit legalization build failed: {}",
        String::from_utf8_lossy(&dsp.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&dsp.stderr).contains("Store size"),
        "dsp56000 spill slots must use the 24-bit word width, not 8-byte stores"
    );
    let lod = std::fs::read_to_string(dir.path().join("wide_dsp.lod")).unwrap();
    assert!(lod.starts_with("_START") && lod.contains("_DATA P") && lod.contains("_END 0000"));
}

#[test]
fn sixteen_bit_target_builds_narrow_fixture_and_refuses_wide_integers() {
    let dir = tempfile::Builder::new()
        .prefix("adesh_int_legal_16_")
        .tempdir()
        .unwrap();

    let narrow = build_fixture(
        dir.path(),
        "narrow.adesh",
        NARROW_FIXTURE,
        "i8086-unknown-dos",
        "NARROW.EXE",
    );
    assert!(
        narrow.status.success(),
        "i8086 16-bit build failed: {}",
        String::from_utf8_lossy(&narrow.stderr)
    );

    let wide = build_fixture(
        dir.path(),
        "wide.adesh",
        WIDE_FIXTURE,
        "i8086-unknown-dos",
        "WIDE.EXE",
    );
    assert!(
        wide.status.success(),
        "i8086 32-bit legalization build failed: {}\n{}",
        String::from_utf8_lossy(&wide.stdout),
        String::from_utf8_lossy(&wide.stderr)
    );
    let wide_com = build_fixture(
        dir.path(),
        "wide.adesh",
        WIDE_FIXTURE,
        "i8086-unknown-dos",
        "WIDE.COM",
    );
    assert!(
        wide_com.status.success(),
        "i8086 32-bit .COM legalization build failed: {}\n{}",
        String::from_utf8_lossy(&wide_com.stdout),
        String::from_utf8_lossy(&wide_com.stderr)
    );

    let wide64 = build_fixture(
        dir.path(),
        "wide64.adesh",
        "fn main() -> i16 { let x: i64 = 1234567890123; print(x); return 0; }\n",
        "i8086-unknown-dos",
        "WIDE64.EXE",
    );
    assert!(
        !wide64.status.success(),
        "i8086 must refuse 64-bit integers, not truncate them"
    );
    let diag = format!(
        "{}{}",
        String::from_utf8_lossy(&wide64.stdout),
        String::from_utf8_lossy(&wide64.stderr)
    );
    assert!(
        diag.contains("i8086-unknown-dos") && diag.contains("64-bit"),
        "i8086 wide-integer refusal must name the target and the construct:\n{diag}"
    );
}
