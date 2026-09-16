//! Comprehensive Integration Test Suite for AdeshLang Input Subsystem
//!
//! Verifies:
//! 1. Basic input with mock values
//! 2. `input.confirm` with true/false mocking
//! 3. `input.password` masked input
//! 4. `input.checkbox` multi-select
//! 5. `input.radio` and `input.select` single select
//! 6. `input.fuzzy` search
//! 7. `input.slider` gauge input
//! 8. `input.datepicker` and `input.timepicker`
//! 9. `input.color` and `input.pin`
//! 10. `input.form` multi-field

use adeshlang::{Interpreter, ModuleLoader};
use std::path::Path;

fn run_code(src: &str) -> Result<(), String> {
    let src_str = src.to_string();
    let h = std::thread::Builder::new()
        .name("adesh_input_test".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            if let Some(m) = adeshlang::execution::runtime_core::INPUT_PLAYBACK.get() {
                if let Ok(mut q) = m.lock() {
                    q.clear();
                }
            }
            let mut loader = ModuleLoader::new(Path::new("."));
            let mut interp = Interpreter::new();
            interp.run_module(&src_str, &mut loader, None)
        })
        .unwrap();
    let res = h.join().unwrap().map_err(|e| e.to_string());
    if let Err(ref e) = res {
        eprintln!("Execution Error: {}", e);
    }
    res
}

#[test]
fn test_input_mock_and_basic_methods() {
    let code = r#"
    input.mock(["AdeshLangDev"]);
    let name = input("Enter name:");
    if (name != "AdeshLangDev") {
        throw "Basic input mock failed: " + name;
    }
    "#;
    assert!(run_code(code).is_ok());
}

#[test]
fn test_input_confirm() {
    let code = r#"
    input.mock(["true"]);
    let res = input.confirm("Proceed?", false);
    if (!res) {
        throw "Confirm mock true failed";
    }

    input.mock(["n"]);
    let res2 = input.confirm("Proceed?", true);
    if (res2) {
        throw "Confirm mock false failed";
    }
    "#;
    assert!(run_code(code).is_ok());
}

#[test]
fn test_input_password() {
    let code = r#"
    input.mock(["SecretKey123!"]);
    let pass = input.password("Enter password:");
    if (pass != "SecretKey123!") {
        throw "Password input mismatch: " + pass;
    }
    "#;
    assert!(run_code(code).is_ok());
}

#[test]
fn test_input_checkbox_and_radio() {
    let code = r#"
    input.mock(["[Rust, TypeScript]"]);
    let stack = input.checkbox("Pick tooling:", ["Rust", "TypeScript", "Python"]);
    if (stack.length != 2) {
        throw "Checkbox selection failed, length: " + string(stack.length);
    }

    input.mock(["Native JIT"]);
    let engine = input.radio("Pick engine:", ["Interpreter", "Native JIT", "AOT"]);
    if (engine != "Native JIT") {
        throw "Radio selection mismatch: " + engine;
    }
    "#;
    assert!(run_code(code).is_ok());
}

#[test]
fn test_input_fuzzy_and_slider() {
    let code = r#"
    input.mock(["AdeshLang"]);
    let picked = input.fuzzy("Search:", ["Rust", "AdeshLang", "Go"]);
    if (picked != "AdeshLang") {
        throw "Fuzzy pick mismatch: " + picked;
    }

    input.mock(["75.5"]);
    let slider_val = input.slider("Volume:", { min: 0, max: 100, step: 0.5, default: 50 });
    if (slider_val != 75.5) {
        throw "Slider value mismatch: " + string(slider_val);
    }
    "#;
    assert!(run_code(code).is_ok());
}

#[test]
fn test_input_datepicker_and_timepicker_and_pin() {
    let code = r#"
    input.mock(["2026-12-25"]);
    let date = input.datepicker("Select date:");
    if (date != "2026-12-25") {
        throw "Datepicker mismatch: " + date;
    }

    input.mock(["14:30:00"]);
    let time = input.timepicker("Select time:");
    if (time != "14:30:00") {
        throw "Timepicker mismatch: " + time;
    }

    input.mock(["4321"]);
    let pin = input.pin("Enter PIN:", 4);
    if (pin != "4321") {
        throw "PIN mismatch: " + pin;
    }
    "#;
    assert!(run_code(code).is_ok());
}

#[test]
fn test_input_table_and_diff() {
    let code = r#"
    input.mock(["0"]);
    let sel = input.table(
        "Select Target:",
        ["Backend", "Engine", "Status"],
        [
            ["Interpreter", "AST", "Ready"],
            ["JIT", "LIR", "Active"]
        ]
    );
    if (sel.row != 0 || sel.col != 0) {
        throw "Table index mismatch: " + string(sel.row);
    }
    if (sel.value != "Interpreter" || sel.header != "Backend") {
        throw "Table value/header mismatch: " + sel.value;
    }
    if (sel.rowData[0] != "Interpreter" || sel.rowData[2] != "Ready") {
        throw "Table rowData mismatch";
    }
    if (sel.rowObject.Status != "Ready") {
        throw "Table rowObject mismatch: " + string(sel.rowObject);
    }

    input.mock(["host = 0.0.0.0"]);
    let patched = input.diff("Edit Config:", "host = 127.0.0.1");
    if (patched != "host = 0.0.0.0") {
        throw "Diff patch mismatch: " + patched;
    }
    "#;
    assert!(run_code(code).is_ok());
}

#[test]
fn test_input_chained_mock_syntax() {
    let code = r#"
    // Chained .mock on input()
    let name = input("Enter your name:").mock(["Ajay"]);
    if (name != "Ajay") {
        throw "Chained input.mock failed: " + name;
    }

    // Chained .mock with single value
    let city = input("Enter city:").mock("Bangalore");
    if (city != "Bangalore") {
        throw "Chained single string mock failed: " + city;
    }

    // Chained .mock on input.select()
    let lang = input.select("Choose Language:", ["Rust", "Python", "AdeshLang"]).mock(["AdeshLang"]);
    if (lang != "AdeshLang") {
        throw "Chained select mock failed: " + lang;
    }

    // Chained .mock on input.pin()
    let pin = input.pin("Enter 4-digit PIN:", 4).mock(["9876"]);
    if (pin != "9876") {
        throw "Chained pin mock failed: " + pin;
    }

    // Chained .mock on input.confirm()
    let is_confirmed = input.confirm("Proceed to deployment?").mock([true]);
    if (!is_confirmed) {
        throw "Chained confirm mock failed";
    }

    // Chained .mock on input.table()
    let selected = input.table(
        "Select Microservice:",
        ["ID", "Service", "Port"],
        [
            ["1", "AuthService", "8081"],
            ["2", "BillingService", "8082"]
        ]
    ).mock(["0"]);
    if (selected.row != 0 || selected.value != "1" || selected.rowObject.Service != "AuthService") {
        throw "Chained table mock failed";
    }
    "#;
    assert!(run_code(code).is_ok());
}

#[test]
fn test_input_slider_parameters_and_defaults() {
    let code = r#"
    // Test positional: prompt, min, max, step, default
    input.mock(["25"]);
    let s1 = input.slider("Volume:", 0, 100, 5, 20);
    if (s1 != 25) {
        throw "Slider positional mock failed: " + string(s1);
    }

    // Test chained .mock on positional slider
    let s2 = input.slider("Brightness:", 10, 50, 2, 30).mock(["42"]);
    if (s2 != 42) {
        throw "Slider chained positional mock failed: " + string(s2);
    }

    // Test object configuration with step and default
    let s3 = input.slider("Speed:", { min: 1, max: 200, step: 0.5, default: 75.5 }).mock(["120.5"]);
    if (s3 != 120.5) {
        throw "Slider object config mock failed: " + string(s3);
    }

    // Test fallback to default when mock is clamped/empty
    input.mock(["999"]);
    let s4 = input.slider("Clamped:", 0, 50, 1, 25);
    if (s4 != 50) {
        throw "Slider clamping failed: " + string(s4);
    }
    "#;
    assert!(run_code(code).is_ok());
}

#[test]
fn test_jit_and_native_jit_input_parity() {
    use adeshlang::backends::jit::jit_run;
    use adeshlang::backends::jit::native::native_jit_run;

    // Test JIT backend with input.mock and input.slider
    let jit_code = r#"
    fn main() {
        input.mock(["60"]);
        let v = input.slider("Volume:", 0, 100, 5, 20);
        return v;
    }
    "#;
    let jit_res = jit_run(jit_code);
    assert!(
        jit_res.is_ok(),
        "JIT slider test failed: {:?}",
        jit_res.err()
    );

    // Test Native JIT backend
    let native_code = r#"
    fn main() {
        input.mock(["75"]);
        let v = input.slider("Level:", 0, 100, 1, 50);
        return v;
    }
    "#;
    let native_res = native_jit_run(native_code);
    assert!(
        native_res.is_ok(),
        "Native JIT slider test failed: {:?}",
        native_res.err()
    );
}
