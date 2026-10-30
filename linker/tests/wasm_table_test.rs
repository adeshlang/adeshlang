//! End-to-end: function tables and `call_indirect`, from real clang objects.
//!
//! A clang object that takes a function's address imports
//! `env.__indirect_function_table`, carries an active element segment, and
//! writes table slots as `R_WASM_TABLE_INDEX_*` fixups. The linker defines the
//! table itself and re-bases every object's segment into one numbering, so the
//! module must import nothing and still dispatch to the right body.

use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

#[path = "common/wasm_fixtures.rs"]
mod fixtures;

use fixtures::{
    INDIRECT_CALL_OBJECT_HEX, TABLE_CALLEE_OBJECT_HEX, TABLE_CALLER_OBJECT_HEX,
    TABLE_DATA_OBJECT_HEX, link_object, link_objects,
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

/// One object that takes two of its own functions' addresses and calls through
/// the pointer: `apply(0, 7)` is `sq(7)`, `apply(1, 3)` is `cu(3)`.
#[test]
fn an_indirect_call_inside_one_object_runs() {
    let dir = tempdir().expect("temp dir");
    let module = link_object(dir.path(), "indirect", INDIRECT_CALL_OBJECT_HEX);

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
if (exports !== 'apply,memory') {
  console.error('unexpected exports: ' + exports);
  process.exit(3);
}
const { apply } = new WebAssembly.Instance(mod, {}).exports;
const observed = [apply(0, 7), apply(1, 3), apply(0, -4), apply(1, 10)].join(',');
if (observed !== '49,27,16,1000') {
  console.error('indirect calls produced ' + observed);
  process.exit(4);
}
process.stdout.write('ok\n');
"#,
    );
    assert_eq!(report, "ok\n");
}

/// A static array of function pointers: the DATA word the program loads must
/// hold the same table slot the CODE relocation computed.
#[test]
fn a_static_table_of_function_addresses_runs() {
    let dir = tempdir().expect("temp dir");
    let module = link_object(dir.path(), "vtable", TABLE_DATA_OBJECT_HEX);

    let report = run_node(
        &module,
        r#"
const fs = require('fs');
const path = process.argv[process.argv.length - 1];
const mod = new WebAssembly.Module(fs.readFileSync(path));
if (WebAssembly.Module.imports(mod).length !== 0) {
  console.error('a self-contained module must import nothing');
  process.exit(2);
}
const exports = WebAssembly.Module.exports(mod).map((e) => e.name).sort().join(',');
if (exports !== 'call,memory') {
  console.error('unexpected exports: ' + exports);
  process.exit(3);
}
const { call } = new WebAssembly.Instance(mod, {}).exports;
const observed = [call(0, 6), call(1, 4), call(0, -5), call(1, 2)].join(',');
if (observed !== '36,64,25,8') {
  console.error('table dispatch produced ' + observed);
  process.exit(4);
}
process.stdout.write('ok\n');
"#,
    );
    assert_eq!(report, "ok\n");
}

/// The caller's element segment names functions another object defines, so the
/// plan has to translate import indices into the defining bodies' indices.
#[test]
fn an_indirect_call_across_objects_runs() {
    let dir = tempdir().expect("temp dir");
    let module = link_objects(
        dir.path(),
        "table",
        &[
            ("caller", TABLE_CALLER_OBJECT_HEX),
            ("fns", TABLE_CALLEE_OBJECT_HEX),
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
if (exports !== 'apply,memory') {
  console.error('unexpected exports: ' + exports);
  process.exit(3);
}
const { apply } = new WebAssembly.Instance(mod, {}).exports;
const observed = [apply(0, 9), apply(1, 2), apply(1, 5)].join(',');
if (observed !== '81,8,125') {
  console.error('cross-object table dispatch produced ' + observed);
  process.exit(4);
}
process.stdout.write('ok\n');
"#,
    );
    assert_eq!(report, "ok\n");
}
