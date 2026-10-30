//! Execute a linker-produced module with the workspace's own wasmtime engine.
//!
//! The Node suites (`linker/tests/wasm_hello_world_test.rs` and friends) run the
//! same bytes in V8. This runs them through a second, independent engine —
//! wasmtime compiles with Cranelift — so an encoding that only V8 tolerates
//! cannot pass as linked correctly. The `wasi_snapshot_preview1.fd_write` host
//! is here rather than in the runtime because wasmtime 33+ dropped WASI
//! preview1, so no shipped wasmtime provides it; the module under test only
//! needs it, and the handler reads the real iovec out of the module's exported
//! memory. The hostless tests need nothing at all, which is what a two-object
//! link is expected to produce — including the ones that call through the
//! function table, since the linker defines the table and its element segment
//! rather than importing them.

use std::path::Path;

use tempfile::tempdir;
use wasmtime::{Caller, Engine, Extern, Instance, Linker, Memory, Module, Store};

#[path = "../linker/tests/common/wasm_fixtures.rs"]
mod fixtures;

use fixtures::{
    DATA_OWNER_OBJECT_HEX, DATA_USER_OBJECT_HEX, EXPORTED_OBJECT_HEX, INDIRECT_CALL_OBJECT_HEX,
    PLAIN_OBJECT_HEX, PURE_CALLEE_OBJECT_HEX, PURE_CALLER_OBJECT_HEX, TABLE_CALLEE_OBJECT_HEX,
    TABLE_CALLER_OBJECT_HEX, WASI_ENTRY_OBJECT_HEX, WASI_GREET_OBJECT_HEX, link_object,
    link_objects,
};

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

/// Compile, instantiate and call `_start` once, returning captured stdout.
fn run(module: &Path) -> Vec<u8> {
    let engine = Engine::default();
    let compiled =
        Module::from_file(&engine, module).expect("wasmtime compiles the linker's module");

    let mut linker = Linker::new(&engine);
    linker
        .func_wrap("wasi_snapshot_preview1", "fd_write", fd_write)
        .expect("the host `fd_write` matches the module's import");

    let mut store = Store::new(&engine, WasiCapture::default());
    let instance = linker
        .instantiate(&mut store, &compiled)
        .expect("the module instantiates with the WASI host");
    let start = instance
        .get_typed_func::<(), ()>(&mut store, "_start")
        .expect("the linker exported the entry point as `_start`");
    start
        .call(&mut store, ())
        .expect("`_start` runs to completion");

    store.data().stdout.clone()
}

#[test]
fn a_linked_hello_world_runs_under_wasmtime() {
    let dir = tempdir().expect("temp dir");
    let module = link_object(dir.path(), "hello_plain", PLAIN_OBJECT_HEX);
    assert_eq!(run(&module), b"hello from adeshlink\n");
}

#[test]
fn an_export_named_hello_world_runs_under_wasmtime() {
    let dir = tempdir().expect("temp dir");
    let module = link_object(dir.path(), "hello", EXPORTED_OBJECT_HEX);
    assert_eq!(run(&module), b"hello from adeshlink\n");
}

#[test]
fn a_cross_object_call_runs_under_wasmtime() {
    let dir = tempdir().expect("temp dir");
    let module = link_objects(
        dir.path(),
        "greet",
        &[
            ("start", WASI_ENTRY_OBJECT_HEX),
            ("greet", WASI_GREET_OBJECT_HEX),
        ],
    );
    assert_eq!(run(&module), b"hello from adeshlink\n");
}

/// Compile and instantiate a module that must need no host at all, then run
/// `body` against its exports.
///
/// The empty-import assertion is the linker's own contract; instantiating it
/// through a `Linker` with nothing defined is Cranelift agreeing.
fn with_hostless_module<T>(module: &Path, body: impl FnOnce(&mut Store<()>, &Instance) -> T) -> T {
    let engine = Engine::default();
    let compiled =
        Module::from_file(&engine, module).expect("wasmtime compiles the linker's module");
    assert_eq!(
        compiled.imports().count(),
        0,
        "the linked module must import nothing, but asks for {:?}",
        compiled.imports().collect::<Vec<_>>()
    );

    let mut store = Store::new(&engine, ());
    let instance = Linker::new(&engine)
        .instantiate(&mut store, &compiled)
        .expect("a module with no imports instantiates without a host");
    body(&mut store, &instance)
}

#[test]
fn a_self_contained_module_runs_under_wasmtime_with_no_host() {
    let dir = tempdir().expect("temp dir");
    let module = link_objects(
        dir.path(),
        "pure",
        &[
            ("add", PURE_CALLER_OBJECT_HEX),
            ("twice", PURE_CALLEE_OBJECT_HEX),
        ],
    );

    let observed = with_hostless_module(&module, |store, instance| {
        let add = instance
            .get_typed_func::<(i32, i32), i32>(&mut *store, "add")
            .expect("the linker exported `add`");
        let mut values = Vec::new();
        for (left, right) in [(20, 22), (0, 7), (-3, 1), (1000, 1)] {
            values.push(
                add.call(&mut *store, (left, right))
                    .expect("`add` runs to completion"),
            );
        }
        values
    });
    assert_eq!(observed, [62, 7, -5, 2001]);
}

/// An object that takes two of its own functions' addresses: the linker has to
/// define the table and element segment, so nothing is imported.
#[test]
fn an_indirect_call_runs_under_wasmtime() {
    let dir = tempdir().expect("temp dir");
    let module = link_object(dir.path(), "indirect", INDIRECT_CALL_OBJECT_HEX);

    let observed = with_hostless_module(&module, |store, instance| {
        let apply = instance
            .get_typed_func::<(i32, i32), i32>(&mut *store, "apply")
            .expect("the linker exported `apply`");
        [(0, 7), (1, 3), (0, -4), (1, 10)]
            .into_iter()
            .map(|(which, value)| {
                apply
                    .call(&mut *store, (which, value))
                    .expect("`apply` runs to completion")
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(observed, [49, 27, 16, 1000]);
}

/// The table entry names a function in the other object, so this pins the
/// name-based resolution through the element segment as well.
#[test]
fn a_cross_object_indirect_call_runs_under_wasmtime() {
    let dir = tempdir().expect("temp dir");
    let module = link_objects(
        dir.path(),
        "table",
        &[
            ("caller", TABLE_CALLER_OBJECT_HEX),
            ("fns", TABLE_CALLEE_OBJECT_HEX),
        ],
    );

    let observed = with_hostless_module(&module, |store, instance| {
        let apply = instance
            .get_typed_func::<(i32, i32), i32>(&mut *store, "apply")
            .expect("the linker exported `apply`");
        [(0, 9), (1, 2), (1, 5)]
            .into_iter()
            .map(|(which, value)| {
                apply
                    .call(&mut *store, (which, value))
                    .expect("`apply` runs to completion")
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(observed, [81, 8, 125]);
}

#[test]
fn cross_object_data_reads_and_writes_run_under_wasmtime() {
    let dir = tempdir().expect("temp dir");
    let module = link_objects(
        dir.path(),
        "data",
        &[
            ("use", DATA_USER_OBJECT_HEX),
            ("data", DATA_OWNER_OBJECT_HEX),
        ],
    );

    let observed = with_hostless_module(&module, |store, instance| {
        let peek = instance
            .get_typed_func::<i32, i32>(&mut *store, "peek")
            .expect("the linker exported `peek`");
        let mut values = Vec::new();
        for index in 0..4 {
            values.push(peek.call(&mut *store, index).expect("`peek` runs"));
        }
        let bump = instance
            .get_typed_func::<(), i32>(&mut *store, "bump")
            .expect("the linker exported `bump`");
        values.push(bump.call(&mut *store, ()).expect("`bump` runs"));
        values.push(bump.call(&mut *store, ()).expect("`bump` runs twice"));
        values
    });
    assert_eq!(observed, [65, 66, 67, 68, 8, 9]);
}
