//! Source -> mandatory gates -> typed Machine IR -> object -> adeshlink -> engines.
use std::path::{Path, PathBuf};
use std::process::Command;
use wasmtime::{Engine, Instance, Module, Store};

const SOURCE: &str = r#"
fn twice(x: i64): i64 { return x + x; }
export fn calculate(n: i64): i64 {
    let total: i64 = 0;
    while (n > 0) {
        total = total + n;
        n = n - 1;
        if (n == 2) { continue; }
        if (n == 1) { break; }
    }
    return twice(total) + 1;
}
export fn factorial(n: i64): i64 {
    if (n <= 1) { return 1; }
    else { return n * factorial(n - 1); }
}
export fn snapshot(): i64 {
    let x: i64 = 3;
    return x + (x = 9);
}
export fn lazy(flag: bool): bool {
    let x: bool = false;
    let ignored: bool = flag && (x = true);
    return x;
}
export fn lazy_or(flag: bool): bool {
    let x: bool = false;
    let ignored: bool = flag || (x = true);
    return x;
}
export fn shadow(x: i64): i64 {
    { let x: i64 = 17; }
    return x;
}
export fn single(x: f32, y: f32): f32 { return -(x * y); }
export fn double(x: f64, y: f64): f64 { return x + y; }
export fn narrow(x: i32): i32 { return x + 1; }
"#;

fn build(dir: &Path, source: &str, extra: &[&str]) -> (std::process::Output, PathBuf) {
    let input = dir.join("main.adesh");
    let output = dir.join("main.wasm");
    std::fs::write(&input, source).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_adesh"))
        .arg("build")
        .arg(input)
        .args([
            "--codegen=wasm-standalone",
            "--target=wasm32-unknown-unknown",
            "--quiet",
            "-o",
        ])
        .arg(&output)
        .args(extra)
        .output()
        .unwrap();
    (result, output)
}

fn success(result: &std::process::Output) {
    assert!(
        result.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn source_pipeline_executes_in_wasmtime_and_v8() {
    let dir = tempfile::tempdir().unwrap();
    let (result, output) = build(dir.path(), SOURCE, &[]);
    success(&result);
    let bytes = std::fs::read(&output).unwrap();
    let engine = Engine::default();
    let module = Module::new(&engine, &bytes).unwrap();
    assert_eq!(module.imports().len(), 0);
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    let calculate = instance
        .get_typed_func::<i64, i64>(&mut store, "calculate")
        .unwrap();
    assert_eq!(calculate.call(&mut store, 5).unwrap(), 29);
    assert_eq!(calculate.call(&mut store, 0).unwrap(), 1);
    assert_eq!(calculate.call(&mut store, -3).unwrap(), 1);
    let factorial = instance
        .get_typed_func::<i64, i64>(&mut store, "factorial")
        .unwrap();
    assert_eq!(factorial.call(&mut store, 10).unwrap(), 3_628_800);
    assert_eq!(
        instance
            .get_typed_func::<(), i64>(&mut store, "snapshot")
            .unwrap()
            .call(&mut store, ())
            .unwrap(),
        12
    );
    for (name, expected) in [("lazy", [0, 1]), ("lazy_or", [1, 0])] {
        let f = instance
            .get_typed_func::<i32, i32>(&mut store, name)
            .unwrap();
        for (flag, expected) in expected.into_iter().enumerate() {
            assert_eq!(f.call(&mut store, flag as i32).unwrap(), expected);
        }
    }
    assert_eq!(
        instance
            .get_typed_func::<i64, i64>(&mut store, "shadow")
            .unwrap()
            .call(&mut store, 4)
            .unwrap(),
        4
    );
    assert_eq!(
        instance
            .get_typed_func::<(f32, f32), f32>(&mut store, "single")
            .unwrap()
            .call(&mut store, (1.25, 2.5))
            .unwrap(),
        -3.125
    );
    assert_eq!(
        instance
            .get_typed_func::<(f64, f64), f64>(&mut store, "double")
            .unwrap()
            .call(&mut store, (0.25, 0.5))
            .unwrap(),
        0.75
    );
    assert_eq!(
        instance
            .get_typed_func::<i32, i32>(&mut store, "narrow")
            .unwrap()
            .call(&mut store, i32::MAX)
            .unwrap(),
        i32::MIN
    );
    let result = Command::new("node")
        .args([
            "-e",
            r#"
        const assert=require('assert'), fs=require('fs');
        const m=new WebAssembly.Module(fs.readFileSync(process.argv[1]));
        const e=new WebAssembly.Instance(m,{}).exports;
        assert.equal(e.calculate(5n),29n); assert.equal(e.factorial(10n),3628800n);
        assert.equal(e.snapshot(),12n); assert.equal(e.lazy(0),0); assert.equal(e.lazy(1),1);
        assert.equal(e.lazy_or(0),1); assert.equal(e.lazy_or(1),0);
        assert.equal(e.shadow(4n),4n); assert.equal(e.single(1.25,2.5),-3.125);
        assert.equal(e.double(0.25,0.5),0.75); assert.equal(e.narrow(2147483647),-2147483648);
    "#,
        ])
        .arg(output)
        .output()
        .expect("Node is required");
    success(&result);
}

#[test]
fn object_output_links_through_public_adeshlink_entry_point() {
    let dir = tempfile::tempdir().unwrap();
    let (result, object) = build(
        dir.path(),
        "export fn add(x: i64): i64 { return x + 2; }",
        &["--emit=obj"],
    );
    success(&result);
    let output = dir.path().join("linked.wasm");
    adesh_linker::Linker::link(
        &[object],
        adesh_linker::LinkConfig::new(
            output.clone(),
            adesh_linker::Target::from_triple("wasm32-unknown-unknown").unwrap(),
        ),
    )
    .unwrap();
    let engine = Engine::default();
    let module = Module::from_file(&engine, output).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    assert_eq!(
        instance
            .get_typed_func::<i64, i64>(&mut store, "add")
            .unwrap()
            .call(&mut store, 40)
            .unwrap(),
        42
    );
}

#[test]
fn refuses_unsupported_features_without_emitting_or_overwriting_output() {
    for source in [
        "export fn bad(x: int): int { return x; }",
        "export fn bad(x: i64): i64 { return x / 2; }",
        "export fn bad(x: i64): i64 { if (x > 0) { return x; } }",
        "export fn bad(): string { return \"unsupported\"; }",
        "let global: i64 = 2; export fn bad(): i64 { return global; }",
        "export fn bad(x): i64 { return x; }",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("main.wasm");
        std::fs::write(&output, b"preserve").unwrap();
        let (result, _) = build(dir.path(), source, &[]);
        assert!(!result.status.success(), "{source}");
        assert_eq!(std::fs::read(&output).unwrap(), b"preserve", "{source}");
    }
    let dir = tempfile::tempdir().unwrap();
    let (result, output) = build(
        dir.path(),
        "export fn bad(x: int): int { return x; }",
        &["--check"],
    );
    assert!(!result.status.success());
    assert!(!output.exists());
    let (result, output) = build(
        dir.path(),
        "export fn ok(): i64 { return 2; }",
        &["--target=wasm32-unknown-wasi"],
    );
    assert!(!result.status.success());
    assert!(!output.exists());
}

#[test]
fn builds_are_deterministic_and_large_local_indices_execute() {
    let mut source = "export fn many(): i64 {".to_string();
    for i in 0..300 {
        source.push_str(&format!("let x{i}: i64 = {i};"));
    }
    source.push_str("return x299; }");
    let dir = tempfile::tempdir().unwrap();
    let (result, output) = build(dir.path(), &source, &[]);
    success(&result);
    let first = std::fs::read(&output).unwrap();
    let (result, _) = build(dir.path(), &source, &[]);
    success(&result);
    assert_eq!(first, std::fs::read(&output).unwrap());
    let engine = Engine::default();
    let module = Module::new(&engine, first).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    assert_eq!(
        instance
            .get_typed_func::<(), i64>(&mut store, "many")
            .unwrap()
            .call(&mut store, ())
            .unwrap(),
        299
    );
}
