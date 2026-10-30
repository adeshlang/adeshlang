// 3.14 is test data for float handling, not a mathematical constant.
#![allow(clippy::approx_constant)]

use adeshlang::cli::{parsing::check_ownership_and_parse, static_imports::link_ast};
use adeshlang::parsing::ast::StmtKind;
use std::path::Path;
use std::process::Command;

fn linked(source: &str, base: &Path) -> Result<Vec<adeshlang::parsing::ast::Stmt>, String> {
    let (ast, _) = check_ownership_and_parse(source, &Default::default())?;
    link_ast(ast, base)
}

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("math.adesh"),
        "fn helper(a, b) { return a + b; }
         export fn add(a, b) { return helper(a, b); }
         export fn unused(a) { return a * 99; }
         export const PI = 3.14;",
    )
    .unwrap();
    dir
}

#[test]
fn links_private_dependencies_but_not_unused_exports() {
    let dir = fixture();
    let ast = linked(
        "import \"./math.adesh\" as m; print(m.add(2, 3));",
        dir.path(),
    )
    .unwrap();
    let names: Vec<_> = ast
        .iter()
        .filter_map(|s| match &s.kind {
            StmtKind::Function(f, _) => Some(f.name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(names, ["__module_1_helper", "__module_1_add"]);
    assert!(
        !ast.iter()
            .any(|s| matches!(s.kind, StmtKind::Import { .. }))
    );
}

#[test]
fn rejects_access_to_private_symbols() {
    let dir = fixture();
    let error = linked(
        "import \"./math.adesh\" as m; print(m.helper(2, 3));",
        dir.path(),
    )
    .unwrap_err();
    assert!(error.contains("does not export 'helper'"), "{error}");
}

#[test]
fn handles_duplicate_aliases_default_exports_and_colliding_module_names() {
    let dir = fixture();
    std::fs::write(
        dir.path().join("other.adesh"),
        "export default fn add(a, b) { return a * b; }",
    )
    .unwrap();
    let ast = linked(
        "import \"./math.adesh\" as a; import \"./math.adesh\" as b;
         import \"./other.adesh\" as c; print(a.add(2, 3)); print(b.PI); print(c(4, 5));",
        dir.path(),
    )
    .unwrap();
    let names: Vec<_> = ast
        .iter()
        .filter_map(|s| match &s.kind {
            StmtKind::Function(f, _) => Some(f.name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        names,
        ["__module_1_helper", "__module_1_add", "__module_2_add"]
    );
}

#[test]
fn local_parameters_can_shadow_import_aliases() {
    let dir = fixture();
    let ast = linked(
        "import \"./math.adesh\" as m; fn f(m) { return m.PI; }",
        dir.path(),
    )
    .unwrap();
    assert_eq!(
        ast.len(),
        1,
        "shadowed namespace must not pull in declarations"
    );
    let StmtKind::Function(f, _) = &ast[0].kind else {
        panic!("missing f")
    };
    assert!(format!("{:?}", f.body).contains("Get("));
}

#[test]
fn missing_relative_import_is_not_replaced_by_a_similarly_named_file() {
    let dir = fixture();
    std::fs::write(dir.path().join("not_missing.adesh"), "export const x = 9;").unwrap();
    let err = linked("import \"./missing.adesh\" as m; print(m.x);", dir.path()).unwrap_err();
    assert!(err.contains("cannot resolve import"), "{err}");
}

#[test]
fn cyclic_module_graph_retains_function_dependencies_once() {
    let dir = fixture();
    std::fs::write(
        dir.path().join("a.adesh"),
        "import \"./b.adesh\" as b; export fn f(x) { return b.g(x); }",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("b.adesh"),
        "import \"./a.adesh\" as a; export fn g(x) { return a.f(x); }",
    )
    .unwrap();
    let ast = linked("import \"./a.adesh\" as a; print(a.f(1));", dir.path()).unwrap();
    assert_eq!(
        ast.iter()
            .filter(|s| matches!(s.kind, StmtKind::Function(..)))
            .count(),
        2
    );
}

#[test]
fn rejects_mutable_module_variables_instead_of_copying_them() {
    let dir = fixture();
    std::fs::write(dir.path().join("state.adesh"), "export let counter = 0;").unwrap();
    let err = linked(
        "import \"./state.adesh\" as s; print(s.counter);",
        dir.path(),
    )
    .unwrap_err();
    assert!(err.contains("mutable module variable"), "{err}");
}

fn execute_source(backend: &str, dir: &Path, source: &str, expected: &[f64]) {
    let main = dir.join("main.adesh");
    std::fs::write(&main, source).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_adesh"))
        .arg("run")
        .arg(&main)
        .arg(backend)
        .arg("--quiet")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "{backend}: {}\n{stdout}\n{stderr}",
        out.status
    );
    assert!(!stderr.contains("fallback"), "{stderr}");
    let values: Vec<f64> = stdout
        .split(['\r', '\n'])
        .filter_map(|line| line.parse::<f64>().ok())
        .collect();
    assert_eq!(
        values.len(),
        expected.len(),
        "{backend} did not execute imported code:\n{stdout}\n{stderr}"
    );
    for (actual, expected) in values.iter().zip(expected) {
        assert!(
            (actual - expected).abs() < 1e-10,
            "{backend} produced wrong values: {values:?}"
        );
    }
}

fn execute(backend: &str) {
    let dir = fixture();
    execute_source(
        backend,
        dir.path(),
        "import \"./math.adesh\" as m; fn main() { print(m.add(2, 3)); print(m.PI); }",
        &[5.0, 3.14],
    );
}

#[test]
fn njit_executes_imported_functions_and_constants() {
    execute("--njit");
}

#[test]
fn adesh_codegen_and_linker_execute_imported_functions_and_constants() {
    execute("--aot");
}

#[test]
fn wasm_executes_imported_functions_and_constants() {
    execute("--wasm");
}

#[test]
fn all_backends_execute_named_default_transitive_and_duplicate_imports() {
    for backend in ["--njit", "--aot", "--wasm"] {
        let dir = fixture();
        std::fs::create_dir(dir.path().join("nested")).unwrap();
        std::fs::write(
            dir.path().join("nested/wrapper.adesh"),
            "import \"../math.adesh\" as m; export fn add(a, b) { return m.add(a, b) + 1; }",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("other.adesh"),
            "export default fn add(a, b) { return a * b; }",
        )
        .unwrap();
        execute_source(
            backend,
            dir.path(),
            "import { add, PI } from \"./math.adesh\";
             import \"./math.adesh\" as again;
             import \"./other.adesh\";
             import \"./nested/wrapper.adesh\" as wrapped;
             fn main() {
                 print(add(2, 3)); print(again.add(3, 4)); print(PI);
                 print(other(4, 5)); print(wrapped.add(2, 3));
             }",
            &[5.0, 7.0, 3.14, 20.0, 6.0],
        );
    }
}

#[test]
fn all_backends_execute_original_asserting_fixture() {
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/import_export_fixtures/main.adesh"),
    )
    .unwrap();
    for backend in ["--njit", "--aot", "--wasm"] {
        let dir = fixture();
        std::fs::write(
            dir.path().join("math.adesh"),
            "export fn add(a, b) { return a + b; }
             export fn mul(a, b) { return a * b; }
             export const PI = 3.14;",
        )
        .unwrap();
        execute_source(backend, dir.path(), &source, &[5.0, 20.0, 3.14]);
    }
}

#[test]
fn all_backends_reject_invalid_imports_without_fallback() {
    for backend in ["--njit", "--aot", "--wasm"] {
        for (source, diagnostic) in [
            (
                "import \"./missing.adesh\" as m; fn main() { print(m.add(1, 2)); }",
                "cannot resolve import",
            ),
            (
                "import \"./math.adesh\" as m; fn main() { print(m.helper(1, 2)); }",
                "does not export 'helper'",
            ),
            (
                "import \"./math.adesh\" as m; fn main() { print(m); }",
                "must be accessed through an exported member",
            ),
        ] {
            let dir = fixture();
            let main = dir.path().join("main.adesh");
            std::fs::write(&main, source).unwrap();
            let out = Command::new(env!("CARGO_BIN_EXE_adesh"))
                .arg("run")
                .arg(&main)
                .arg(backend)
                .arg("--quiet")
                .output()
                .unwrap();
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(
                !out.status.success(),
                "{backend} accepted invalid imports: {stderr}"
            );
            assert!(stderr.contains(diagnostic), "{backend}: {stderr}");
            assert!(!stderr.contains("fallback"), "{backend}: {stderr}");
        }
    }
}

#[test]
fn wasm_compile_and_build_resolve_imports_from_source_not_output_directory() {
    let dir = fixture();
    let output_dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("main.adesh");
    std::fs::write(
        &main,
        "import \"./math.adesh\" as m; fn main() { print(m.add(2, 3)); print(m.PI); }",
    )
    .unwrap();
    for command in ["compile-wasm", "build"] {
        let wasm = output_dir.path().join(format!("{command}.wasm"));
        let mut cli = Command::new(env!("CARGO_BIN_EXE_adesh"));
        cli.current_dir(output_dir.path()).arg(command).arg(&main);
        if command == "build" {
            cli.args(["--emit=wasm", "-o"]).arg(&wasm).arg("--quiet");
        } else {
            cli.arg(&wasm);
        }
        let result = cli.output().unwrap();
        assert!(
            result.status.success(),
            "{command}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let loader = wasm.with_extension("cjs");
        adeshlang::backends::wasm::write_js_loader(&wasm, &loader).unwrap();
        let result = Command::new("node")
            .arg(&loader)
            .output()
            .expect("Node.js is required");
        let stdout = String::from_utf8_lossy(&result.stdout);
        assert!(
            result.status.success(),
            "{command}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let values: Vec<f64> = stdout.lines().filter_map(|s| s.parse().ok()).collect();
        assert_eq!(values, [5.0, 3.14], "{command}: {stdout}");
    }
}

#[test]
fn rejects_cyclic_constants_and_duplicate_import_bindings() {
    let dir = fixture();
    std::fs::write(
        dir.path().join("cycle.adesh"),
        "export const A = B; const B = A;",
    )
    .unwrap();
    let error = linked("import \"./cycle.adesh\" as c; print(c.A);", dir.path()).unwrap_err();
    assert!(error.contains("cyclic module constant"), "{error}");
    let error = linked(
        "import \"./math.adesh\" as m; import \"./math.adesh\" as m;",
        dir.path(),
    )
    .unwrap_err();
    assert!(error.contains("duplicate import binding"), "{error}");
}

#[test]
fn wasm_test_runner_executes_imports_and_fails_when_node_is_missing() {
    let dir = fixture();
    let main = dir.path().join("tests.adesh");
    std::fs::write(
        &main,
        "import \"./math.adesh\" as m; test fn imported() { print(m.add(2, 3)); }",
    )
    .unwrap();
    let run = |without_node: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_adesh"));
        command
            .arg("test")
            .arg(&main)
            .args(["--runtimes=wasm", "--nocapture"]);
        if without_node {
            command.env("PATH", "");
        }
        command.output().unwrap()
    };
    let out = run(false);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.split(['\r', '\n']).any(|line| line.trim() == "5"),
        "wasm test runner did not execute imported code: {stdout}"
    );
    let out = run(true);
    let diagnostic = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !out.status.success(),
        "missing Node.js must not pass: {diagnostic}"
    );
    assert!(diagnostic.contains("requires Node.js"), "{diagnostic}");
}

#[test]
fn njit_test_runner_resolves_imports_relative_to_test_source() {
    let dir = fixture();
    let main = dir.path().join("tests.adesh");
    std::fs::write(
        &main,
        "import \"./math.adesh\" as m; test fn imported() { print(m.add(2, 3)); }",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_adesh"))
        .arg("test")
        .arg(&main)
        .args(["--runtimes=njit", "--nocapture"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.lines().any(|line| line.trim() == "5"), "{stdout}");
}
