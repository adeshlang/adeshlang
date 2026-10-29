//! End-to-end: two real clang objects linked into one module that runs.
//!
//! The fixtures in `common/wasm_fixtures.rs` are `clang -c` output for a caller
//! and a callee in separate translation units, and for a user of data defined in
//! another object. In the object file, a reference to a symbol the same link
//! satisfies is written exactly like a host import, so this suite is what tells
//! those two apart in the emitted module: the phantom `env` import must be gone
//! and the call must land on the definition's body.

use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

#[path = "common/wasm_fixtures.rs"]
mod fixtures;

use fixtures::{
    DATA_OWNER_OBJECT_HEX, DATA_USER_OBJECT_HEX, WASI_ENTRY_OBJECT_HEX, WASI_GREET_OBJECT_HEX,
    link_objects,
};

fn run_node(module: &Path, script: &str) -> String {
    if Command::new("node").arg("--version").output().is_err() {
        eprintln!("node is not on PATH; skipping execution");
        return String::new();
    }

    let output = Command::new("node")
        .arg("-e")
        .arg(script)
        .arg(module)
        .output()
        .expect("run node");
    assert!(
        output.status.success(),
        "node rejected the module: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf8 stdout")
}

/// A call across the object boundary into a function that itself imports a host
/// function: only `fd_write` may be imported, `env.greet` must be resolved.
#[test]
fn a_cross_object_call_to_a_host_calling_definition_runs() {
    let dir = tempdir().expect("temp dir");
    let module = link_objects(
        dir.path(),
        "greet",
        &[
            ("start", WASI_ENTRY_OBJECT_HEX),
            ("greet", WASI_GREET_OBJECT_HEX),
        ],
    );

    let stdout = run_node(
        &module,
        r#"
const fs = require('fs');
const path = process.argv[process.argv.length - 1];
const mod = new WebAssembly.Module(fs.readFileSync(path));
const imports = WebAssembly.Module.imports(mod).map((i) => i.module + '.' + i.name);
if (imports.join(',') !== 'wasi_snapshot_preview1.fd_write') {
  console.error('unexpected imports: ' + imports.join(','));
  process.exit(2);
}
const exports = WebAssembly.Module.exports(mod).map((e) => e.name).sort().join(',');
if (exports !== '_start,greet,memory') {
  console.error('unexpected exports: ' + exports);
  process.exit(3);
}
let instance;
let out = '';
const wasi = {
  fd_write(fd, iovs, iovsLen, written) {
    const memory = instance.exports.memory;
    const view = new DataView(memory.buffer);
    let total = 0;
    for (let i = 0; i < iovsLen; i++) {
      const base = view.getUint32(iovs + i * 8, true);
      const len = view.getUint32(iovs + i * 8 + 4, true);
      out += Buffer.from(new Uint8Array(memory.buffer, base, len)).toString('utf8');
      total += len;
    }
    view.setUint32(written, total, true);
    return 0;
  },
};
instance = new WebAssembly.Instance(mod, { wasi_snapshot_preview1: wasi });
instance.exports._start();
process.stdout.write(out);
"#,
    );
    assert_eq!(stdout, "hello from adeshlink\n");
}

/// Reads and writes variables that live in the other object's data segments.
///
/// The module needs no host at all, so this also pins that a data-only
/// cross-object link does not smuggle in an import.
#[test]
fn cross_object_data_reads_and_writes_run() {
    let dir = tempdir().expect("temp dir");
    let module = link_objects(
        dir.path(),
        "data",
        &[
            ("use", DATA_USER_OBJECT_HEX),
            ("data", DATA_OWNER_OBJECT_HEX),
        ],
    );

    let report = run_node(
        &module,
        r#"
const fs = require('fs');
const path = process.argv[process.argv.length - 1];
const mod = new WebAssembly.Module(fs.readFileSync(path));
const imports = WebAssembly.Module.imports(mod);
if (imports.length !== 0) {
  console.error('unexpected imports: ' + imports.map((i) => i.module + '.' + i.name).join(','));
  process.exit(2);
}
const exports = WebAssembly.Module.exports(mod).map((e) => e.name).sort().join(',');
if (exports !== 'bump,memory,peek') {
  console.error('unexpected exports: ' + exports);
  process.exit(3);
}
const instance = new WebAssembly.Instance(mod, {});
const { peek, bump } = instance.exports;
const observed = [peek(0), peek(1), peek(2), peek(3), bump(), bump()].join(',');
if (observed !== '65,66,67,68,8,9') {
  console.error('cross-object data produced ' + observed);
  process.exit(4);
}
process.stdout.write('ok\n');
"#,
    );
    assert_eq!(report, "ok\n");
}
