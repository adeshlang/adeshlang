//! End-to-end: `adeshlink` produces a WASM module that needs nothing from the
//! host.
//!
//! The two fixture objects call each other across the object boundary and import
//! nothing, so a module that still lists an import here was linked wrongly: the
//! `env` entries a relocatable object writes for its undefined symbols are the
//! link's own business, not the host's. Instantiating with `{}` is the point —
//! it proves the output is self-contained and runs in a bare engine.

use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

#[path = "common/wasm_fixtures.rs"]
mod fixtures;

use fixtures::{PURE_CALLEE_OBJECT_HEX, PURE_CALLER_OBJECT_HEX, link_objects};

fn run_pure(module: &Path) -> String {
    if Command::new("node").arg("--version").output().is_err() {
        eprintln!("node is not on PATH; skipping execution");
        return String::new();
    }

    let script = r#"
const fs = require('fs');
const path = process.argv[process.argv.length - 1];
const mod = new WebAssembly.Module(fs.readFileSync(path));
const imports = WebAssembly.Module.imports(mod);
if (imports.length !== 0) {
  console.error('module is not self-contained, it imports ' +
    imports.map((i) => i.module + '.' + i.name).join(', '));
  process.exit(2);
}
const exports = WebAssembly.Module.exports(mod).map((e) => e.name).sort().join(',');
if (exports !== 'add,memory') {
  console.error('unexpected exports: ' + exports);
  process.exit(3);
}
const instance = new WebAssembly.Instance(mod, {});
const { add } = instance.exports;
const observed = [add(20, 22), add(0, 7), add(-3, 1), add(1000, 1)].join(',');
if (observed !== '62,7,-5,2001') {
  console.error('add produced ' + observed);
  process.exit(4);
}
process.stdout.write('ok\n');
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
fn a_two_object_link_is_a_self_contained_module() {
    let dir = tempdir().expect("temp dir");
    let module = link_objects(
        dir.path(),
        "pure",
        &[
            ("add", PURE_CALLER_OBJECT_HEX),
            ("twice", PURE_CALLEE_OBJECT_HEX),
        ],
    );
    assert_eq!(run_pure(&module), "ok\n");
}

/// Resolution follows the definition, not the order the objects were given in.
#[test]
fn a_self_contained_module_does_not_depend_on_input_order() {
    let dir = tempdir().expect("temp dir");
    let module = link_objects(
        dir.path(),
        "pure_reversed",
        &[
            ("twice", PURE_CALLEE_OBJECT_HEX),
            ("add", PURE_CALLER_OBJECT_HEX),
        ],
    );
    assert_eq!(run_pure(&module), "ok\n");
}
