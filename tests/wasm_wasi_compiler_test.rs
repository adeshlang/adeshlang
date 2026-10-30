//! `adesh build --target wasm32-*-wasi` must produce a module a real WASI
//! Preview 1 host can execute.
//!
//! The default compiler ABI imports `env.*` helpers and needs the generated
//! Node loader. Under a WASI target the module must instead import only
//! `wasi_snapshot_preview1.fd_write` and carry its own write/alloc/formatting
//! adapters. This suite builds with the CLI, checks the import list, then runs
//! the bytes under the workspace's wasmtime dependency with an in-test
//! `fd_write` host (wasmtime 33+ ships no preview1 runner) and asserts stdout.

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::tempdir;
use wasmtime::{Caller, Engine, Extern, Linker, Memory, Module, Store};

/// Linear-memory file descriptor stdout is written to.
const STDOUT: i32 = 1;
/// WASI `EBADF`, the only error this host can produce.
const EBADF: i32 = 8;

#[derive(Default)]
struct WasiCapture {
    stdout: Vec<u8>,
}

fn read_u32(memory: &Memory, caller: &Caller<'_, WasiCapture>, address: usize) -> Option<u32> {
    let mut word = [0u8; 4];
    memory.read(caller, address, &mut word).ok()?;
    Some(u32::from_le_bytes(word))
}

fn fd_write(
    mut caller: Caller<'_, WasiCapture>,
    fd: i32,
    iovs: i32,
    iovs_len: i32,
    nwritten: i32,
) -> i32 {
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return EBADF;
    };

    let mut payload = Vec::new();
    for index in 0..iovs_len.max(0) as usize {
        let entry = iovs.max(0) as usize + index * 8;
        let (Some(base), Some(len)) = (
            read_u32(&memory, &caller, entry),
            read_u32(&memory, &caller, entry + 4),
        ) else {
            return EBADF;
        };
        let mut buffer = vec![0u8; len as usize];
        if memory.read(&caller, base as usize, &mut buffer).is_err() {
            return EBADF;
        }
        payload.extend_from_slice(&buffer);
    }

    let written = payload.len() as u32;
    if memory
        .write(
            &mut caller,
            nwritten.max(0) as usize,
            &written.to_le_bytes(),
        )
        .is_err()
    {
        return EBADF;
    }
    if fd == STDOUT {
        caller.data_mut().stdout.extend_from_slice(&payload);
    }
    0
}

/// Build `source` for `wasm32-unknown-wasi` through the real CLI.
fn build(dir: &Path, source: &str) -> PathBuf {
    let src = dir.join("main.adesh");
    std::fs::write(&src, source).expect("write source");
    let out = dir.join("main.wasm");
    let output = Command::new(env!("CARGO_BIN_EXE_adesh"))
        .arg("build")
        .arg(&src)
        .arg("--target=wasm32-unknown-wasi")
        .arg("-o")
        .arg(&out)
        .arg("--quiet")
        .output()
        .expect("run adesh");
    assert!(
        output.status.success(),
        "build failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    out
}

/// Instantiate `module`, assert its import surface, call `main`, return stdout.
fn run(module: &Path) -> Vec<u8> {
    let engine = Engine::default();
    let compiled = Module::from_file(&engine, module).expect("wasmtime compiles the module");

    let imports: Vec<String> = compiled
        .imports()
        .map(|import| format!("{}::{}", import.module(), import.name()))
        .collect();
    assert_eq!(
        imports,
        ["wasi_snapshot_preview1::fd_write"],
        "a WASI build must import exactly `fd_write`"
    );

    let mut linker = Linker::new(&engine);
    linker
        .func_wrap("wasi_snapshot_preview1", "fd_write", fd_write)
        .expect("the host `fd_write` matches the module's import");

    let mut store = Store::new(&engine, WasiCapture::default());
    let instance = linker
        .instantiate(&mut store, &compiled)
        .expect("the module instantiates with the WASI host");
    let main = instance
        .get_typed_func::<(), ()>(&mut store, "main")
        .expect("the compiler exports `main`");
    main.call(&mut store, ()).expect("`main` runs");
    store.data().stdout.clone()
}

const PROGRAM: &str = r#"
struct Pair { left: number; right: number };
fn sum(a: number, b: number): number { return a + b; }
fn negate(x: number): number { return 0 - x; }
fn pick(p: Pair): number { return p.left; }
print("sum:");
print(sum(2, 3));
print(sum(1.5, 1.25));
print(negate(7));
print(sum(0.25, 0.25));
print(sum(0.1, 0.2));
let pair = Pair { left: 6.5, right: 9 };
print(pick(pair));
"#;

#[test]
fn a_wasi_build_imports_fd_write_and_prints_through_it() {
    let dir = tempdir().expect("temp dir");
    let module = build(dir.path(), PROGRAM);
    assert_eq!(
        run(&module),
        b"sum:\n5\n2.75\n-7\n0.5\n0.3\n6.5\n",
        "WASI mode prints strings, integers and six-digit-rounded fractions"
    );
}

#[test]
fn a_wasi_build_reports_file_writes_as_unavailable() {
    let dir = tempdir().expect("temp dir");
    let src = dir.path().join("main.adesh");
    std::fs::write(&src, r#"print("hello", { file: "out.txt" });"#).expect("write source");
    let out = dir.path().join("main.wasm");
    let output = Command::new(env!("CARGO_BIN_EXE_adesh"))
        .arg("build")
        .arg(&src)
        .arg("--target=wasm32-unknown-wasi")
        .arg("-o")
        .arg(&out)
        .arg("--quiet")
        .output()
        .expect("run adesh");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "build unexpectedly succeeded");
    assert!(
        stderr.contains("WASI Preview 1"),
        "expected a loud WASI file-write diagnostic, got: {stderr}"
    );
}

#[test]
fn a_non_wasi_wasm_build_keeps_the_env_abi() {
    let dir = tempdir().expect("temp dir");
    let src = dir.path().join("main.adesh");
    std::fs::write(&src, "print(\"hi\");").expect("write source");
    let out = dir.path().join("main.wasm");
    let output = Command::new(env!("CARGO_BIN_EXE_adesh"))
        .arg("build")
        .arg(&src)
        .arg("--target=wasm32-unknown-unknown")
        .arg("-o")
        .arg(&out)
        .arg("--quiet")
        .output()
        .expect("run adesh");
    assert!(
        output.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let engine = Engine::default();
    let compiled = Module::from_file(&engine, &out).expect("wasmtime compiles the module");
    let modules: Vec<&str> = compiled.imports().map(|import| import.module()).collect();
    assert!(
        !modules.is_empty() && modules.iter().all(|m| *m == "env"),
        "a non-WASI wasm target keeps the `env.*` ABI, got: {modules:?}"
    );
}
