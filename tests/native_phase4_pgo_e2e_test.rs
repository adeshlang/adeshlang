//! Phase 4 PGO and tail-recursion execution coverage.

#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
fn generate_run_use_preserves_result() {
    use adesh_codegen::opt::pgo::ProfileData;
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!("adesh_pgo_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("input.adesh");
    std::fs::write(
        &src,
        r#"
fn helper(n: int): int {
    if n > 10 { return 42; }
    return 9;
}
fn main(): int { return helper(20); }
"#,
    )
    .unwrap();

    let cli = env!("CARGO_BIN_EXE_adesh");
    for lto in [false, true] {
        let exe = dir.join(format!("generate_{lto}.exe"));
        let mut build = Command::new(cli);
        build
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .arg("build")
            .arg(&src)
            .arg("-O1")
            .arg("--pgo=generate")
            .arg("-o")
            .arg(&exe);
        if lto {
            build.arg("--lto");
        }
        let out = build.output().unwrap();
        assert!(
            out.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let run = Command::new(&exe).current_dir(&dir).output().unwrap();
        assert_eq!(run.status.code(), Some(42));

        let profile_path = dir.join("adesh_pgo_profile.json");
        let json = std::fs::read_to_string(&profile_path)
            .expect("instrumented executable must dump real counters");
        let profile = ProfileData::from_json(&json).unwrap();
        assert_eq!(profile.functions["main"].entry_count, 1);
        assert!(profile.functions.values().all(|f| f.entry_count > 0));

        let used = dir.join(format!("use_{lto}.exe"));
        let mut use_build = Command::new(cli);
        use_build
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .arg("build")
            .arg(&src)
            .arg("-O1")
            .arg(format!("--pgo=use={}", profile_path.display()))
            .arg("-o")
            .arg(&used);
        if lto {
            use_build.arg("--lto");
        }
        let out = use_build.output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            Command::new(&used).output().unwrap().status.code(),
            Some(42)
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
fn native_tail_recursion_handles_deep_input_with_constant_stack() {
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!("adesh_tailrec_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("tailrec.adesh");
    std::fs::write(
        &src,
        r#"
fn count_down(n: int, acc: int): int {
    if n <= 0 { return acc; }
    return count_down(n - 1, acc);
}
fn main(): int { return count_down(250000, 42); }
"#,
    )
    .unwrap();
    let exe = dir.join("tailrec.exe");
    let build = Command::new(env!("CARGO_BIN_EXE_adesh"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .arg("build")
        .arg(&src)
        .arg("-O3")
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "build failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(&exe).output().unwrap();
    assert_eq!(
        run.status.code(),
        Some(42),
        "deep tail recursion must preserve the result; stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
fn native_direct_tail_call_links_and_returns_correctly() {
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!("adesh_tailcall_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("tailcall.adesh");
    std::fs::write(
        &src,
        r#"
fn callee(n: int): int { return n + 1; }
fn wrapper(n: int): int { return callee(n); }
fn main(): int { return wrapper(41); }
"#,
    )
    .unwrap();
    let exe = dir.join("tailcall.exe");
    let build = Command::new(env!("CARGO_BIN_EXE_adesh"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .arg("build")
        .arg(&src)
        .arg("-O2")
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "build failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert_eq!(Command::new(&exe).output().unwrap().status.code(), Some(42));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn layout_preserves_fallthrough() {
    use adesh_codegen::machine_ir::{MachineFunction, MachineInstruction};
    use adesh_codegen::opt::pass::MachinePass;
    use adesh_codegen::opt::{BlockProfile, FunctionProfile, PgoOptimizationPass, ProfileData};

    let mut f = MachineFunction::new("fallthrough");
    f.create_block("cold");
    f.create_block("hot");
    f.blocks[1].push(MachineInstruction::Return);
    f.blocks[2].push(MachineInstruction::Return);
    let mut p = FunctionProfile::new("fallthrough");
    p.entry_count = 1;
    p.add_block_profile(2, BlockProfile { execution_count: 1 });
    let mut data = ProfileData::new();
    data.add_function_profile(p);
    PgoOptimizationPass::new(data)
        .run_on_function(&mut f)
        .unwrap();
    assert_eq!(f.blocks[1].label, "hot");
    assert!(matches!(
        f.blocks[0].instructions.last(),
        Some(MachineInstruction::Branch { target }) if target == "cold"
    ));
}
