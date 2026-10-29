//! End-to-end: a real clang-emitted relocatable object links and runs.
//!
//! The fixtures are `clang --target=wasm32 -c` output for a C hello world that
//! calls WASI `fd_write`; they are embedded in `common/wasm_fixtures.rs` so the
//! test is hermetic. The linked module is instantiated by Node with a
//! `fd_write` stub that reads the real iovec out of the module's linear memory,
//! so a wrong data address or a mis-numbered call fails here instead of
//! printing garbage in a browser.

use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

#[path = "common/wasm_fixtures.rs"]
mod fixtures;

use fixtures::{EXPORTED_OBJECT_HEX, PLAIN_OBJECT_HEX, link_object};

/// Instantiate `module` in Node, call `_start` once, and return what the
/// `fd_write` stub read out of linear memory.
fn run_and_capture_stdout(module: &Path) -> String {
    if Command::new("node").arg("--version").output().is_err() {
        eprintln!("node is not on PATH; skipping execution");
        return String::new();
    }

    let script = r#"
const fs = require('fs');
const path = process.argv[process.argv.length - 1];
const mod = new WebAssembly.Module(fs.readFileSync(path));
const imports = WebAssembly.Module.imports(mod).map((i) => i.module + '.' + i.name);
if (imports.join(',') !== 'wasi_snapshot_preview1.fd_write') {
  console.error('unexpected imports: ' + imports.join(','));
  process.exit(2);
}
const exports = WebAssembly.Module.exports(mod).map((e) => e.name).sort().join(',');
if (exports !== '_start,memory') {
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
"#;

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

#[test]
fn a_plain_clang_hello_world_links_and_runs() {
    let dir = tempdir().expect("temp dir");
    let module = link_object(dir.path(), "hello_plain", PLAIN_OBJECT_HEX);
    assert_eq!(run_and_capture_stdout(&module), "hello from adeshlink\n");
}

#[test]
fn an_export_name_hello_world_links_and_runs() {
    let dir = tempdir().expect("temp dir");
    let module = link_object(dir.path(), "hello", EXPORTED_OBJECT_HEX);
    assert_eq!(run_and_capture_stdout(&module), "hello from adeshlink\n");
}
