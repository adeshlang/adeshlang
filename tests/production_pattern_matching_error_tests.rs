//! Production Pattern Matching, Algebraic Enums, and Exception Handling Tests

use adeshlang::{Interpreter, ModuleLoader};
use std::path::Path;

fn run_test_code(src: &str) -> Result<(), String> {
    let src_str = src.to_string();
    let h = std::thread::Builder::new()
        .name("adesh_pattern_test".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let mut loader = ModuleLoader::new(Path::new("."));
            let mut interp = Interpreter::new();
            interp.run_module(&src_str, &mut loader, None)
        })
        .unwrap();
    h.join().unwrap().map_err(|e| e.to_string())
}

#[test]
fn test_enum_variant_matching() {
    let code = r#"
        enum WebEvent {
            PageLoad,
            PageUnload,
            KeyPress(i32),
            Paste(string),
        }

        fn describe_event(event: WebEvent) -> string {
            return match (event) {
                WebEvent.PageLoad => "LOADED",
                WebEvent.PageUnload => "UNLOADED",
                WebEvent.KeyPress(key) => "KEY:" + key,
                WebEvent.Paste(text) => "PASTE:" + text,
                _ => "UNKNOWN",
            };
        }

        let e1 = WebEvent.PageLoad;
        let e2 = WebEvent.KeyPress(13);
        let e3 = WebEvent.Paste("AdeshLang");

        assert_eq(describe_event(e1), "LOADED");
        assert_eq(describe_event(e2), "KEY:13");
        assert_eq(describe_event(e3), "PASTE:AdeshLang");
    "#;
    let res = run_test_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}

#[test]
fn test_try_catch_throw_error_propagation() {
    let code = r#"
        fn divide(a: i32, b: i32) -> i32 {
            if (b == 0) {
                throw "Division by zero";
            }
            return a ~/ b;
        }

        fn safe_calc(a: i32, b: i32) -> string {
            try {
                let res = divide(a, b);
                return "Result: " + res;
            } catch (e) {
                return "Caught: " + e;
            }
        }

        assert_eq(safe_calc(10, 2), "Result: 5");
        assert_eq(safe_calc(10, 0), "Caught: Division by zero");
    "#;
    let res = run_test_code(code);
    assert!(res.is_ok(), "Failed: {:?}", res.err());
}
