use adeshlang::cli::import_linking::load_imports;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/import_export_fixtures")
}

fn read_main() -> String {
    std::fs::read_to_string(fixtures_dir().join("main.adesh")).expect("main.adesh fixture missing")
}

/// Native import resolution: `load_imports` must resolve and gate the imported
/// module, merge its declarations, and produce the namespace bindings — without
/// delegating to the interpreter.
#[test]
fn import_resolution_loads_and_merges_module() {
    let src = read_main();
    let result = load_imports(&src, &fixtures_dir());
    assert!(
        result.is_ok(),
        "import resolution failed: {:?}",
        result.err()
    );

    let resolution = result.unwrap();
    // The `math` alias should be bound to the imported module path.
    assert!(
        resolution.bindings.iter().any(|b| b.alias == "math"),
        "expected a namespace binding for alias 'math', got {:?}",
        resolution.bindings
    );
    // The merged module should contain the imported `add` function.
    assert!(
        resolution.functions.iter().any(|f| f.name == "add"),
        "expected imported function 'add' to be merged into main HIR"
    );
    assert!(
        resolution.functions.iter().any(|f| f.name == "mul"),
        "expected imported function 'mul' to be merged into main HIR"
    );
}

/// Selective (tree-shaken) import: when `main` only references `math.add`, only
/// the `add` function should be merged — `mul` (and other unused symbols) must
/// not be imported into the main compilation unit.
#[test]
fn import_resolution_tree_shakes_unused_symbols() {
    let src = std::fs::read_to_string(fixtures_dir().join("partial_main.adesh"))
        .expect("partial_main.adesh fixture missing");
    let result = load_imports(&src, &fixtures_dir());
    assert!(
        result.is_ok(),
        "import resolution failed: {:?}",
        result.err()
    );

    let resolution = result.unwrap();
    // `add` is used, so it must be merged.
    assert!(
        resolution.functions.iter().any(|f| f.name == "add"),
        "expected used function 'add' to be merged into main HIR"
    );
    // `mul` is NOT used, so it must NOT be merged (tree-shaken away).
    assert!(
        !resolution.functions.iter().any(|f| f.name == "mul"),
        "expected unused function 'mul' to be tree-shaken and not merged"
    );
    // The usage record for `math` should reflect only the referenced members.
    let usage = resolution
        .usage
        .get("math")
        .expect("expected usage record for alias 'math'");
    assert!(usage.members.contains("add"));
    assert!(!usage.members.contains("mul"));
    assert!(!usage.members.contains("PI"));
}
/// from the imported module should be merged into the main module's HIR and the
/// default-import binding produced.
#[test]
fn import_resolution_loads_default_export() {
    let src = std::fs::read_to_string(fixtures_dir().join("default_main.adesh"))
        .expect("default_main.adesh fixture missing");
    let result = load_imports(&src, &fixtures_dir());
    assert!(
        result.is_ok(),
        "default export resolution failed: {:?}",
        result.err()
    );

    let resolution = result.unwrap();
    // The default-exported `add` function should be merged into main HIR.
    assert!(
        resolution.functions.iter().any(|f| f.name == "add"),
        "expected default-exported function 'add' to be merged into main HIR"
    );
}

/// The interpreter (reference backend) executes the import/export program correctly.
#[test]
fn interpreter_runs_import_export() {
    let src = read_main();
    run_via_interpreter(src).expect("interpreter should run import/export program");
}

fn run_via_interpreter(src: String) -> Result<(), String> {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            use adeshlang::{Interpreter, ModuleLoader};
            let mut loader = ModuleLoader::new(fixtures_dir().as_path());
            let mut interp = Interpreter::new();
            interp.run_module(&src, &mut loader, None)
        })
        .unwrap()
        .join()
        .unwrap()
        .map_err(|e| e.to_string())
}
