//! Phase 19.7 — Functional Native Standard Library (`libadesh_std.adob` / `AdobBundle`).
//!
//! Verifies:
//! 1. Real `.adesh` program importing `std:io`, `std:string`, `std:collections`, `std:math`,
//!    `std:time`, `std:fs`, and `std:path` builds via `adesh build` (default stdlib linking),
//!    executes natively on Linux x86-64, and produces exact output and filesystem effects.
//! 2. Linked binary contains no interpreter-only symbols.
//! 3. `--no-std` opts out of linking `libadesh_std.adob` (fails when `std:*` is used; succeeds for
//!    freestanding programs).
//! 4. Explicit `--stdlib <path>` works with both single-target `.adob` and multi-target `.adbb`
//!    (`AdobBundle`) archives.
//! 5. Corrupt archive/checksum, missing `--stdlib`, duplicate stdlib symbol definitions,
//!    cross-target mismatch (Windows `.adob` into Linux link), and unsupported `std:*` modules
//!    all fail loudly.

use adesh_codegen::stdlib_builder::StdlibAdobBuilder;
use adesh_linker::{LinkConfig, Linker, Target};
use adesh_object::bundle::AdobBundle;
use adesh_object::writer::AdobWriter;
use adesh_object::{AdobValidator, TargetDescriptor};
use std::path::Path;
use std::process::Command;

fn unique_temp_dir(name: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(&format!("adesh_p19_7_{name}_"))
        .tempdir()
        .expect("create tempdir")
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn phase19_7_real_stdlib_program_executes_end_to_end() {
    let dir = unique_temp_dir("e2e");
    let src_path = dir.path().join("stdlib_app.adesh");
    let out_bin = dir.path().join("stdlib_app");
    let work_dir = dir.path().join("sandbox");
    let sub_dir = work_dir.join("nested");
    let data_file = sub_dir.join("note.txt");

    let work_dir_str = work_dir.to_string_lossy().replace('\\', "/");
    let src = format!(
        r#"import "std:io" as io;
import "std:string" as s;
import "std:collections" as col;
import "std:math" as m;
import "std:time" as t;
import "std:fs" as fs;
import "std:path" as p;

fn main() {{
    // std:io
    io.println("io:ok");
    io.eprintln("err:ok");
    io.flush();

    // std:string (including UTF-8 char boundaries)
    let greeting = s.concat("héllo ", "world");
    io.println(s.len(greeting));
    io.println(s.slice(greeting, 0, 5));
    io.println(s.char_at(greeting, 1));
    io.println(s.index_of(greeting, "world"));
    io.println(s.contains(greeting, "world"));
    io.println(s.compare("abc", "abd"));

    // std:collections
    let v = col.vec_new();
    col.vec_push(v, "alpha");
    col.vec_push(v, "beta");
    col.vec_set(v, 1, "BETA");
    io.println(col.len(v));
    io.println(col.vec_get(v, 0));
    io.println(col.vec_pop(v));
    io.println(col.len(v));

    let mp = col.map_new();
    col.map_set(mp, "k1", "v1");
    col.map_set(mp, "k2", "v2");
    io.println(col.map_get(mp, "k1"));
    io.println(col.map_remove(mp, "k2"));
    io.println(col.contains(mp, "k2"));

    let st = col.set_new();
    io.println(col.set_add(st, "tag1"));
    io.println(col.set_add(st, "tag1"));
    io.println(col.contains(st, "tag1"));
    io.println(col.set_remove(st, "tag1"));

    // std:math (both integer and floating-point ABI paths)
    io.println(m.abs_i64(-42));
    io.println(m.min_i64(9, 4));
    io.println(m.max_i64(9, 4));
    io.println(m.clamp_i64(15, 0, 10));
    io.println(m.sqrt(81.0));
    io.println(m.pow(2.0, 5.0));
    io.println(m.floor(3.9));
    io.println(m.ceil(3.1));

    // std:time
    let ms = t.now_ms();
    let clk = t.clock();
    io.println(ms > 0);
    io.println(clk >= 0.0);

    // std:path + std:fs
    let base = "{work_dir_str}";
    let nested = p.join(base, "nested");
    let raw_file = p.join(nested, "./sub/../note.txt");
    let norm_file = p.normalize(raw_file);
    io.println(p.basename(norm_file));
    io.println(p.extname(norm_file));

    io.println(fs.mkdir(nested));
    io.println(fs.is_dir(nested));
    io.println(fs.write(norm_file, "phase-19.7-payload"));
    io.println(fs.exists(norm_file));
    io.println(fs.is_file(norm_file));
    io.println(fs.file_size(norm_file));
    io.println(fs.read(norm_file));
    let entries = fs.readdir(nested);
    io.println(col.len(entries));
    io.println(fs.delete(norm_file));
    io.println(fs.exists(norm_file));
}}
"#
    );
    std::fs::write(&src_path, src).expect("write stdlib_app.adesh");

    let build_out = Command::new(env!("CARGO_BIN_EXE_adesh"))
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(&out_bin)
        .arg("--quiet")
        .output()
        .expect("invoke adesh build");
    assert!(
        build_out.status.success(),
        "adesh build failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&build_out.stdout),
        String::from_utf8_lossy(&build_out.stderr)
    );
    assert!(out_bin.exists(), "expected compiled binary at {out_bin:?}");
    assert!(
        !data_file.exists(),
        "file should be created at runtime, not compile time"
    );

    let run_out = Command::new(&out_bin)
        .output()
        .expect("execute compiled binary");
    assert!(
        run_out.status.success(),
        "compiled binary exited with {:?}\nstdout: {}\nstderr: {}",
        run_out.status,
        String::from_utf8_lossy(&run_out.stdout),
        String::from_utf8_lossy(&run_out.stderr)
    );
    let stdout = String::from_utf8_lossy(&run_out.stdout);
    let stderr = String::from_utf8_lossy(&run_out.stderr);
    assert_eq!(stderr.trim(), "err:ok");

    let lines: Vec<&str> = stdout.lines().collect();
    let expected = [
        "io:ok",
        "12",                 // "héllo world" UTF-8 byte length == 12
        "héll",               // UTF-8 byte slice(0, 5): h(1)+é(2)+l(1)+l(1)
        "é",                  // char_at(1)
        "7",                  // UTF-8 byte index_of("world")
        "true",               // contains("world")
        "-1",                 // compare("abc", "abd")
        "2",                  // vec len
        "alpha",              // vec_get(0)
        "BETA",               // vec_pop()
        "1",                  // vec len after pop
        "v1",                 // map_get("k1")
        "true",               // map_remove("k2")
        "false",              // contains("k2")
        "true",               // set_add("tag1") first time
        "false",              // set_add("tag1") duplicate
        "true",               // contains("tag1")
        "true",               // set_remove("tag1")
        "42",                 // abs_i64(-42)
        "4",                  // min_i64(9, 4)
        "9",                  // max_i64(9, 4)
        "10",                 // clamp_i64(15, 0, 10)
        "9.0",                // sqrt(81.0)
        "32.0",               // pow(2.0, 5.0)
        "3.0",                // floor(3.9)
        "4.0",                // ceil(3.1)
        "true",               // now_ms() > 0
        "true",               // clock() >= 0.0
        "note.txt",           // basename
        ".txt",               // extname
        "true",               // mkdir
        "true",               // is_dir
        "true",               // write
        "true",               // exists
        "true",               // is_file
        "18",                 // file_size("phase-19.7-payload")
        "phase-19.7-payload", // read
        "1",                  // readdir len
        "true",               // delete
        "false",              // exists after delete
    ];
    assert_eq!(
        lines, expected,
        "unexpected stdout lines:\nactual:\n{stdout}"
    );

    // Verify no interpreter-only symbols were linked into the native binary.
    let bin_bytes = std::fs::read(&out_bin).expect("read binary");
    let bin_lossy = String::from_utf8_lossy(&bin_bytes);
    for forbidden in [
        "Interpreter::",
        "execute_statement",
        "adeshlang::execution::",
    ] {
        assert!(
            !bin_lossy.contains(forbidden),
            "native binary unexpectedly contains interpreter symbol/marker `{forbidden}`"
        );
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn phase19_7_no_std_opt_out_and_freestanding() {
    let dir = unique_temp_dir("nostd");

    // 1. Program using std:io MUST fail to link when --no-std is passed.
    let std_src = dir.path().join("uses_std.adesh");
    let std_bin = dir.path().join("uses_std");
    std::fs::write(
        &std_src,
        "import \"std:io\" as io;\nfn main() { io.println(\"nope\"); }\n",
    )
    .unwrap();
    let fail_out = Command::new(env!("CARGO_BIN_EXE_adesh"))
        .args(["build"])
        .arg(&std_src)
        .arg("-o")
        .arg(&std_bin)
        .args(["--no-std", "--quiet"])
        .output()
        .unwrap();
    assert!(
        !fail_out.status.success(),
        "--no-std should fail linking when std:* symbols are referenced"
    );
    let fail_err = String::from_utf8_lossy(&fail_out.stderr);
    assert!(
        fail_err.contains("adesh_std_io_println") || fail_err.contains("Undefined"),
        "expected unresolved stdlib symbol error, got: {fail_err}"
    );

    // 2. Freestanding program with no std:* imports succeeds under --no-std.
    let free_src = dir.path().join("freestanding.adesh");
    let free_bin = dir.path().join("freestanding");
    std::fs::write(
        &free_src,
        "fn add(a: int, b: int) -> int { return a + b; }\nfn main() -> int { return add(20, 22); }\n",
    )
    .unwrap();
    let ok_out = Command::new(env!("CARGO_BIN_EXE_adesh"))
        .args(["build"])
        .arg(&free_src)
        .arg("-o")
        .arg(&free_bin)
        .args(["--no-std", "--quiet"])
        .output()
        .unwrap();
    assert!(
        ok_out.status.success(),
        "freestanding --no-std build failed: {}",
        String::from_utf8_lossy(&ok_out.stderr)
    );
    let exec_out = Command::new(&free_bin).output().unwrap();
    assert_eq!(exec_out.status.code(), Some(42));
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn phase19_7_explicit_stdlib_adob_and_bundle_roundtrip() {
    let dir = unique_temp_dir("explicit_stdlib");
    let src_path = dir.path().join("app.adesh");
    std::fs::write(
        &src_path,
        r#"import "std:io" as io;
import "std:math" as m;
fn main() {
    io.println(m.max_i64(7, 19));
}
"#,
    )
    .unwrap();

    let linux_target = TargetDescriptor::from_triple("x86_64-unknown-linux-gnu").unwrap();
    let win_target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").unwrap();

    // 1. Single-target libadesh_std.adob
    let linux_obj = StdlibAdobBuilder::new(linux_target.clone())
        .emit_adob()
        .expect("emit linux stdlib adob");
    let adob_path = dir.path().join("libadesh_std.adob");
    std::fs::write(&adob_path, AdobWriter::write(&linux_obj).unwrap()).unwrap();

    let out_bin_adob = dir.path().join("app_adob");
    let res1 = Command::new(env!("CARGO_BIN_EXE_adesh"))
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(&out_bin_adob)
        .arg("--stdlib")
        .arg(&adob_path)
        .arg("--quiet")
        .output()
        .unwrap();
    assert!(
        res1.status.success(),
        "explicit --stdlib .adob build failed: {}",
        String::from_utf8_lossy(&res1.stderr)
    );
    let run1 = Command::new(&out_bin_adob).output().unwrap();
    assert!(run1.status.success());
    assert_eq!(String::from_utf8_lossy(&run1.stdout).trim(), "19");

    // 2. Multi-target AdobBundle (.adbb) containing Linux + Windows stdlib entries
    let win_obj = StdlibAdobBuilder::new(win_target)
        .emit_adob()
        .expect("emit windows stdlib adob");
    let mut bundle = AdobBundle::new();
    bundle.add_object(linux_obj);
    bundle.add_object(win_obj);
    let bundle_bytes = bundle.write().expect("encode AdobBundle");
    let bundle_path = dir.path().join("libadesh_std.adbb");
    std::fs::write(&bundle_path, &bundle_bytes).unwrap();

    let out_bin_bundle = dir.path().join("app_bundle");
    let res2 = Command::new(env!("CARGO_BIN_EXE_adesh"))
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(&out_bin_bundle)
        .arg("--stdlib")
        .arg(&bundle_path)
        .arg("--quiet")
        .output()
        .unwrap();
    assert!(
        res2.status.success(),
        "explicit --stdlib .adbb bundle build failed: {}",
        String::from_utf8_lossy(&res2.stderr)
    );
    let run2 = Command::new(&out_bin_bundle).output().unwrap();
    assert!(run2.status.success());
    assert_eq!(String::from_utf8_lossy(&run2.stdout).trim(), "19");
}

#[test]
fn phase19_7_rejects_missing_corrupt_duplicate_and_cross_target_stdlib() {
    let dir = unique_temp_dir("errors");
    let src_path = dir.path().join("app.adesh");
    std::fs::write(
        &src_path,
        "import \"std:io\" as io;\nfn main() { io.println(\"hi\"); }\n",
    )
    .unwrap();

    // 1. Missing --stdlib path
    let missing = dir.path().join("does_not_exist.adob");
    let missing_res = Command::new(env!("CARGO_BIN_EXE_adesh"))
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(dir.path().join("out_missing"))
        .arg("--stdlib")
        .arg(&missing)
        .arg("--quiet")
        .output()
        .unwrap();
    assert!(!missing_res.status.success());
    let missing_err = String::from_utf8_lossy(&missing_res.stderr);
    assert!(
        missing_err.contains("cannot read stdlib archive"),
        "unexpected error for missing --stdlib: {missing_err}"
    );

    // 2. Corrupt stdlib ADOB (checksum / integrity mismatch)
    let linux_target = TargetDescriptor::from_triple("x86_64-unknown-linux-gnu").unwrap();
    let linux_obj = StdlibAdobBuilder::new(linux_target.clone())
        .emit_adob()
        .unwrap();
    let mut corrupt_adob = linux_obj.clone();
    if let Some(text) = corrupt_adob.sections.iter_mut().find(|s| s.name == ".text")
        && let Some(first) = text.data.first_mut()
    {
        *first ^= 0xFF;
    }
    let val_err = StdlibAdobBuilder::validate_stdlib_adob(&corrupt_adob, &linux_target)
        .expect_err("corrupt stdlib ADOB must fail integrity validation");
    assert!(
        val_err.to_string().contains("digest mismatch") || val_err.to_string().contains("Invalid"),
        "unexpected corrupt ADOB error: {val_err}"
    );

    let corrupt_path = dir.path().join("corrupt.adob");
    std::fs::write(&corrupt_path, AdobWriter::write(&corrupt_adob).unwrap()).unwrap();
    let corrupt_cli = Command::new(env!("CARGO_BIN_EXE_adesh"))
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(dir.path().join("out_corrupt"))
        .arg("--stdlib")
        .arg(&corrupt_path)
        .arg("--quiet")
        .output()
        .unwrap();
    assert!(!corrupt_cli.status.success());
    let corrupt_err = String::from_utf8_lossy(&corrupt_cli.stderr);
    assert!(
        corrupt_err.contains("digest mismatch") || corrupt_err.contains("invalid stdlib"),
        "unexpected CLI error for corrupt stdlib: {corrupt_err}"
    );

    // 3. Corrupt AdobBundle
    let win_target = TargetDescriptor::from_triple("x86_64-pc-windows-msvc").unwrap();
    let win_obj = StdlibAdobBuilder::new(win_target.clone())
        .emit_adob()
        .unwrap();
    let mut bundle = AdobBundle::new();
    bundle.add_object(linux_obj.clone());
    bundle.add_object(win_obj.clone());
    let mut bundle_bytes = bundle.write().unwrap();
    let mid = bundle_bytes.len() / 2;
    bundle_bytes[mid] ^= 0x5A;
    assert!(
        AdobBundle::read(&bundle_bytes).is_err(),
        "corrupt AdobBundle must fail checksum/decode"
    );

    // 4. Cross-target mismatch: Windows stdlib ADOB passed to Linux link
    let win_path = dir.path().join("win_std.adob");
    std::fs::write(&win_path, AdobWriter::write(&win_obj).unwrap()).unwrap();
    let xtarget_cli = Command::new(env!("CARGO_BIN_EXE_adesh"))
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(dir.path().join("out_xtarget"))
        .arg("--stdlib")
        .arg(&win_path)
        .arg("--quiet")
        .output()
        .unwrap();
    assert!(!xtarget_cli.status.success());
    let xtarget_err = String::from_utf8_lossy(&xtarget_cli.stderr);
    assert!(
        xtarget_err.contains("target mismatch") || xtarget_err.contains("Target"),
        "expected cross-target mismatch error, got: {xtarget_err}"
    );

    // Also test direct Linker::link cross-target rejection
    let linux_path = dir.path().join("linux_std.adob");
    std::fs::write(&linux_path, AdobWriter::write(&linux_obj).unwrap()).unwrap();
    let link_target = Target::from_triple("x86_64-unknown-linux-gnu").unwrap();
    let link_cfg = LinkConfig::new(dir.path().join("direct_out"), link_target.clone());
    let direct_xtarget = Linker::link(&[win_path], link_cfg)
        .expect_err("linker must reject Windows ADOB when targeting Linux");
    assert!(
        format!("{direct_xtarget}").contains("target")
            || format!("{direct_xtarget}").contains("Windows")
            || format!("{direct_xtarget}").contains("mismatch"),
        "unexpected direct linker cross-target error: {direct_xtarget}"
    );

    // 5. Duplicate stdlib symbol definitions rejected by Linker::link
    let linux_dup_path = dir.path().join("linux_std_dup.adob");
    std::fs::write(&linux_dup_path, AdobWriter::write(&linux_obj).unwrap()).unwrap();
    let dup_cfg = LinkConfig::new(dir.path().join("direct_dup_out"), link_target);
    let dup_err = Linker::link(&[linux_path.clone(), linux_dup_path], dup_cfg)
        .expect_err("linker must reject duplicate stdlib symbols");
    assert!(
        format!("{dup_err}").contains("multiple definitions")
            || format!("{dup_err}").contains("LNK002"),
        "expected duplicate symbol error, got: {dup_err}"
    );

    // 6. Conflicting --no-std and --stdlib
    let conflict = Command::new(env!("CARGO_BIN_EXE_adesh"))
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(dir.path().join("out_conflict"))
        .arg("--no-std")
        .arg("--stdlib")
        .arg(&linux_path)
        .arg("--quiet")
        .output()
        .unwrap();
    assert!(!conflict.status.success());

    // Ensure AdobValidator succeeds on valid object
    AdobValidator::validate(&linux_obj).unwrap();
    let _ = Path::new(".");
}

#[test]
fn phase19_7_unsupported_std_module_fails_loudly() {
    let dir = unique_temp_dir("unsupported_mod");
    for (mod_name, call_src) in [
        (
            "http",
            "import \"std:http\" as http;\nfn main() { http.get(\"https://example.com\"); }\n",
        ),
        (
            "json",
            "import \"std:json\" as json;\nfn main() { json.parse(\"{}\"); }\n",
        ),
    ] {
        let src_path = dir.path().join(format!("{mod_name}_app.adesh"));
        std::fs::write(&src_path, call_src).unwrap();
        let res = Command::new(env!("CARGO_BIN_EXE_adesh"))
            .arg("build")
            .arg(&src_path)
            .arg("-o")
            .arg(dir.path().join(format!("{mod_name}_app")))
            .arg("--quiet")
            .output()
            .unwrap();
        assert!(
            !res.status.success(),
            "unsupported std:{mod_name} must fail loudly instead of silently stubbing"
        );
        let err = String::from_utf8_lossy(&res.stderr);
        assert!(
            err.contains("not supported") || err.contains("unsupported"),
            "expected explicit unsupported diagnostic for std:{mod_name}, got: {err}"
        );
    }
}
