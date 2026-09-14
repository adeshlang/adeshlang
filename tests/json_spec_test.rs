//! Comprehensive integration tests for AdeshLang language-level JSON System.
//!
//! The AdeshLang interpreter is deeply recursive; tests are run on threads
//! with an enlarged stack (64 MiB) to avoid stack-overflow in debug builds.

use adeshlang::{Interpreter, ModuleLoader};
use std::path::Path;

/// Run AdeshLang source on a 64 MiB stack thread to avoid debug-build stack overflows.
fn run_code(src: &str) -> Result<(), String> {
    let src = src.to_owned();
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let mut loader = ModuleLoader::new(Path::new("."));
            let mut interp = Interpreter::new();
            interp
                .run_module(&src, &mut loader, None)
                .map_err(|e| e.to_string())
        })
        .expect("thread spawn failed")
        .join()
        .expect("thread panicked")
}

// ─── Basic parse / stringify ───────────────────────────────────────────────

#[test]
fn test_json_no_import_parse_and_stringify() {
    let code = r#"
        fn main() {
            let text = "{\"name\":\"AdeshLang\",\"version\":1}";
            let data = JSON.parse(text);
            let out  = JSON.stringify(data);
            if out.contains("AdeshLang") == false {
                throw "Expected AdeshLang in output";
            }
        }
    "#;
    let res = run_code(code);
    assert!(
        res.is_ok(),
        "JSON.parse and JSON.stringify failed: {:?}",
        res.err()
    );
}

// ─── Pretty-printing / compact ─────────────────────────────────────────────

#[test]
fn test_json_pretty_and_compact() {
    let code = r#"
        fn main() {
            let val    = JSON.parse("{\"a\":1,\"b\":[2,3]}");
            let pretty = JSON.stringifyPretty(val);
            if pretty.contains("\n") == false {
                throw "Pretty output should contain newlines";
            }
        }
    "#;
    let res = run_code(code);
    assert!(res.is_ok(), "JSON formatting failed: {:?}", res.err());
}

// ─── isValid / tryParse ────────────────────────────────────────────────────

#[test]
fn test_json_is_valid_and_try_parse() {
    let code = r#"
        fn main() {
            let v1 = JSON.isValid("{\"ok\": true}");
            let v2 = JSON.isValid("{invalid: true}");
            if v1 != true || v2 != false {
                throw "isValid failed";
            }
            let p1 = JSON.tryParse("{\"val\": 42}");
            let p2 = JSON.tryParse("bad json");
            if p1.val != 42 {
                throw "tryParse: expected p1.val == 42";
            }
            if p2 != null {
                throw "tryParse: expected null for bad input";
            }
        }
    "#;
    let res = run_code(code);
    assert!(res.is_ok(), "JSON validation failed: {:?}", res.err());
}

// ─── minify / pretty ───────────────────────────────────────────────────────

#[test]
fn test_json_minify_and_pretty() {
    let code = r#"
        fn main() {
            let compact = JSON.minify("{\n  \"x\": 10\n}");
            if compact != "{\"x\":10}" {
                throw "Minify failed: " + compact;
            }
            let expanded = JSON.pretty(compact);
            if expanded.contains("\n") == false {
                throw "Pretty failed";
            }
        }
    "#;
    let res = run_code(code);
    assert!(res.is_ok(), "Minify/pretty failed: {:?}", res.err());
}

// ─── Parse error diagnostics ───────────────────────────────────────────────

#[test]
fn test_json_parse_error_diagnostics() {
    let code = r#"
        fn main() {
            JSON.parse("{\"unclosed\": ");
        }
    "#;
    let err = run_code(code).unwrap_err();
    assert!(
        err.contains("JSON parse error"),
        "Expected 'JSON parse error' in: {err}"
    );
    assert!(err.contains("line"), "Expected 'line' in error: {err}");
}

// ─── Depth limit ───────────────────────────────────────────────────────────

#[test]
fn test_json_depth_limit() {
    let brackets: String = "[".repeat(150) + &"]".repeat(150);
    // embed literal brackets: no escaping needed because they aren't special in AdeshLang strings
    let code = format!("fn main() {{ JSON.parse(\"{brackets}\"); }}");
    let err = run_code(&code).unwrap_err();
    // Either our guard fires ("Exceeded maximum nesting depth") or serde_json's
    // own recursion limiter fires ("recursion limit exceeded") — both are correct.
    assert!(
        err.contains("Exceeded maximum nesting depth") || err.contains("recursion limit"),
        "Expected a nesting-depth error, got: {err}"
    );
}

// ─── File I/O ──────────────────────────────────────────────────────────────

#[test]
fn test_json_file_io() {
    let code = r#"
        fn main() {
            let path = "tmp_json_test_io.json";
            let obj  = JSON.object();
            obj.title = "AdeshLang";
            JSON.stringifyFile(path, obj, true);
            let back = JSON.parseFile(path);
            if back.title != "AdeshLang" {
                throw "File round-trip title mismatch";
            }
        }
    "#;
    let res = run_code(code);
    let _ = std::fs::remove_file("tmp_json_test_io.json");
    assert!(res.is_ok(), "JSON file I/O failed: {:?}", res.err());
}

// ─── Byte serialization ────────────────────────────────────────────────────

#[test]
fn test_json_bytes_parse_and_stringify() {
    let code = r#"
        fn main() {
            // Use JSON.stringify->bytes->parseBytes round-trip on a fully parsed object
            let src      = "{\"code\": 100, \"label\": \"ok\"}";
            let val      = JSON.parse(src);
            let bytes    = JSON.stringifyBytes(val);
            let restored = JSON.parseBytes(bytes);
            if restored.label != "ok" {
                throw "Byte round-trip label mismatch: " + str(restored.label);
            }
        }
    "#;
    let res = run_code(code);
    assert!(res.is_ok(), "JSON byte ops failed: {:?}", res.err());
}
