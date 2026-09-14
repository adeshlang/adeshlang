use std::ffi::{CStr, CString};
use adesh_mobile_bridge::*;

#[test]
fn test_session_creation_and_destruction() {
    let session = AdeshSession::new();
    assert_eq!(session.get_backend_name(), "Interpreter");
}

#[test]
fn test_regression_interpreter_backend_hardcoded() {
    let session = AdeshSession::new();
    assert_eq!(
        session.get_backend_name(),
        "Interpreter",
        "Mobile bridge MUST use ExecutionBackend::Interpreter"
    );

    let res = session.run("let x = 42; print(x);", Some("test.adesh"));
    assert_eq!(res.backend_used, "Interpreter");
}

#[test]
fn test_hello_world_execution() {
    let session = AdeshSession::new();
    let code = r#"print("Hello, Mobile World!");"#;
    let res = session.run(code, Some("hello.adesh"));

    assert_eq!(res.status, ExecutionStatus::Completed);
    assert!(res.stdout.contains("Hello, Mobile World!"));
    assert!(res.diagnostics.is_empty());
}

#[test]
fn test_arithmetic_and_variables() {
    let session = AdeshSession::new();
    let code = r#"
        let a = 10;
        let b = 20;
        let sum = a + b;
        print("Sum:", sum);
    "#;
    let res = session.run(code, Some("arith.adesh"));

    assert_eq!(res.status, ExecutionStatus::Completed);
    assert!(res.stdout.contains("Sum: 30"));
}

#[test]
fn test_functions() {
    let session = AdeshSession::new();
    let code = r#"
        fn add(x, y) {
            return x + y;
        }
        print("Add result:", add(15, 25));
    "#;
    let res = session.run(code, Some("func.adesh"));

    assert_eq!(res.status, ExecutionStatus::Completed);
    assert!(res.stdout.contains("Add result: 40"));
}

#[test]
fn test_conditions_and_loops() {
    let session = AdeshSession::new();
    let code = r#"
        let count = 0;
        let i = 1;
        while (i <= 5) {
            count = count + i;
            i = i + 1;
        }
        if (count == 15) {
            print("Loop Success:", count);
        } else {
            print("Loop Failed");
        }
    "#;
    let res = session.run(code, Some("loop.adesh"));

    assert_eq!(res.status, ExecutionStatus::Completed);
    assert!(res.stdout.contains("Loop Success: 15"));
}

#[test]
fn test_collections_and_strings() {
    let session = AdeshSession::new();
    let code = r#"
        let arr = [10, 20, 30];
        print("First:", arr[0]);
        let msg = "AdeshLang";
        print("Msg:", msg);
    "#;
    let res = session.run(code, Some("coll.adesh"));

    assert_eq!(res.status, ExecutionStatus::Completed);
    assert!(res.stdout.contains("First: 10"));
    assert!(res.stdout.contains("Msg: AdeshLang"));
}

#[test]
fn test_syntax_error_diagnostics() {
    let session = AdeshSession::new();
    let code = "fn main() { let x = ; }";
    let diags = session.check(code, Some("bad.adesh"));

    assert!(!diags.is_empty());
    assert_eq!(diags[0].severity, DiagnosticSeverity::Error);
}

#[test]
fn test_code_formatting() {
    let session = AdeshSession::new();
    let code = "fn   foo( a , b ) { return a+b ; }";
    let formatted = session.format(code).unwrap();

    assert!(formatted.contains("fn foo(a, b)"));
}

#[test]
fn test_c_ffi_api() {
    unsafe {
        let session_ptr = adesh_session_create();
        assert!(!session_ptr.is_null());

        let version_ptr = adesh_get_version();
        let version = CStr::from_ptr(version_ptr).to_str().unwrap();
        assert!(version.contains("Mobile Bridge"));
        adesh_free_string(version_ptr);

        let code_c = CString::new("print(\"FFI Test\");").unwrap();
        let fn_c = CString::new("ffi.adesh").unwrap();

        let run_res_ptr = adesh_run(session_ptr, code_c.as_ptr(), fn_c.as_ptr());
        let run_res_str = CStr::from_ptr(run_res_ptr).to_str().unwrap();
        assert!(run_res_str.contains("FFI Test"));
        assert!(run_res_str.contains("Interpreter"));
        adesh_free_string(run_res_ptr);

        let fmt_c = CString::new("fn bar(x){return x;}").unwrap();
        let fmt_res_ptr = adesh_format(fmt_c.as_ptr());
        let fmt_res_str = CStr::from_ptr(fmt_res_ptr).to_str().unwrap();
        assert!(fmt_res_str.contains("fn bar(x)"));
        adesh_free_string(fmt_res_ptr);

        adesh_session_destroy(session_ptr);
    }
}

#[test]
fn test_repeated_executions() {
    let session = AdeshSession::new();
    for i in 1..=5 {
        let code = format!("print(\"Iteration {}\", {});", i, i * 10);
        let res = session.run(&code, Some("repeat.adesh"));
        assert_eq!(res.status, ExecutionStatus::Completed);
        assert!(res.stdout.contains(&format!("Iteration {}", i)));
    }
}
