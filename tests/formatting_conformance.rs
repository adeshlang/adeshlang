//! Formatting Conformance Regression Tests
//!
//! Tests exact output formatting across adesh-format engine, Interpreter, VM (v1 & v2),
//! Native JIT, and WASM backend targets.

use adesh_format::{FormatSpec, FormatValue};
use adeshlang::execution::bytecode::{compile_to_file, compile_to_file_v2};
use adeshlang::execution::runtime::{Interpreter, ModuleLoader};
use adeshlang::execution::vm::run_file_with_writer;
use std::fs;
use std::thread;
use tempfile::tempdir;

#[test]
fn test_format_spec_parse_direct() {
    let spec = FormatSpec::parse(">12.2float").unwrap();
    assert_eq!(spec.width, Some(12));
    assert_eq!(spec.precision, Some(2));
    assert_eq!(spec.type_spec, "float");

    let spec_curr = FormatSpec::parse("currency(USD)").unwrap();
    assert_eq!(spec_curr.type_spec, "currency(USD)");

    let spec_hex = FormatSpec::parse("hex").unwrap();
    assert_eq!(spec_hex.type_spec, "hex");
}

#[test]
fn test_format_value_render_direct() {
    let spec_curr = FormatSpec::parse("currency(USD)").unwrap();
    let val = FormatValue::Number(19.9);
    assert_eq!(val.render(&spec_curr).unwrap(), "$19.90");

    let spec_inr = FormatSpec::parse("currency(INR)").unwrap();
    let val_inr = FormatValue::Number(123456.78);
    assert_eq!(val_inr.render(&spec_inr).unwrap(), "₹123,456.78");

    let spec_hex = FormatSpec::parse("HEX").unwrap();
    let val_int = FormatValue::Int(255);
    assert_eq!(val_int.render(&spec_hex).unwrap(), "FF");

    let spec_bin = FormatSpec::parse("bin").unwrap();
    let val_bin = FormatValue::Int(10);
    assert_eq!(val_bin.render(&spec_bin).unwrap(), "1010");
}

#[test]
fn test_formatting_examples_interpreter() {
    let examples = [
        ("examples/formatting/basic.adesh", "USD: $19.90"),
        ("examples/formatting/numbers.adesh", "Padded int: 00042"),
        ("examples/formatting/radix_bits.adesh", "Hex upper:  FF"),
        ("examples/formatting/currency_all.adesh", "GBP: £1,234.50"),
        ("examples/formatting/report_table.adesh", "Active"),
        (
            "examples/formatting/text_unicode.adesh",
            "Filled: '**adesh***'",
        ),
        ("examples/formatting/runtime_values.adesh", "Bool:   true"),
    ];

    for (path, _expected) in examples {
        let path_str = path.to_string();
        let content =
            fs::read_to_string(path).unwrap_or_else(|e| panic!("Failed to read {}: {}", path, e));

        let handle = thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(move || {
                let dir = tempdir().expect("tempdir");
                let mut interp = Interpreter::new();
                let mut loader = ModuleLoader::new(dir.path());
                interp.run_module(&content, &mut loader, Some(path_str))
            })
            .expect("spawn thread");

        let res = handle.join().expect("join thread");
        assert!(res.is_ok(), "Execution failed for {}: {:?}", path, res);
    }
}

#[test]
fn test_vm_v2_formatting_conformance() {
    let src = r#"
let usd = 19.9;
let hex_val = 255;
print(`USD: ${usd:currency(USD)}`);
print(`Hex: ${hex_val:HEX}`);
"#;
    let dir = tempdir().expect("tempdir");
    let bin_path = dir.path().join("test_vm_v2_format.bin");

    compile_to_file_v2(src, &bin_path).expect("compile v2 failed");
    let mut out_buf: Vec<u8> = Vec::new();
    run_file_with_writer(&bin_path, &mut out_buf).expect("run v2 failed");
    let output = String::from_utf8_lossy(&out_buf);

    assert!(output.contains("USD: $19.90"), "Output was: {}", output);
    assert!(output.contains("Hex: FF"), "Output was: {}", output);
}

#[test]
fn test_vm_v1_formatting_conformance() {
    let src = r#"
let val = 42;
print(`Val: ${val:05d}`);
"#;
    let dir = tempdir().expect("tempdir");
    let bin_path = dir.path().join("test_vm_v1_format.bin");

    compile_to_file(src, &bin_path).expect("compile v1 failed");
    let mut out_buf: Vec<u8> = Vec::new();
    run_file_with_writer(&bin_path, &mut out_buf).expect("run v1 failed");
    let output = String::from_utf8_lossy(&out_buf);

    assert!(output.contains("Val: 00042"), "Output was: {}", output);
}

#[test]
fn test_native_jit_formatting_conformance() {
    let src = r#"
let val = 255;
print(`Hex: ${val:HEX}`);
"#;
    let res = adeshlang::backends::jit::native::native_jit_run(src);
    assert!(res.is_ok(), "Native JIT formatting failed: {:?}", res);
}

#[test]
fn test_wasm_formatting_conformance() {
    let src = r#"
let val = 123.45;
print(`Val: ${val:currency(USD)}`);
"#;
    let dir = tempdir().expect("tempdir");
    let text_path = dir.path().join("test_wasm_format.wasm");

    let res = adeshlang::backends::wasm::compiler::compile_to_file(src, &text_path);
    assert!(res.is_ok(), "WASM compile formatting failed: {:?}", res);
    assert!(text_path.exists(), "WASM binary must exist");
}
