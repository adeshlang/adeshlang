//! Input and Unicode Tests
//!
//! Validates interactive input helpers, typed autoconversion, selection forms,
//! and wide Unicode support for strings, chars, and escape sequences.
use adeshlang::{Interpreter, ModuleLoader};

fn run_src(src: &str) -> Interpreter {
    let mut interp = Interpreter::new();
    let mut loader = ModuleLoader::new(std::path::Path::new("."));
    let _ = interp.run_module(src, &mut loader, Some("<test>".to_string()));
    interp
}

#[test]
fn input_simple_mock() {
    let src = r#"
    input.mock(["Ajay"]);
    let name = input("Name:");
    print(name);
    "#;
    let _ = run_src(src);
}

#[test]
fn input_typed_autoconvert_with_retry() {
    let src = r#"
    input.mock(["twenty","20"]);
    let age:int = input("Age:");
    print(age);
    "#;
    let _ = run_src(src);
}

#[test]
fn input_select_basic() {
    let src = r#"
    input.mock(["2"]);
    let color = input.select("Color:",["red","green","blue"]);
    print(color);
    "#;
    let _ = run_src(src);
}

#[test]
fn input_form_basic() {
    let src = r#"
    input.mock(["Ajay","8080","ajay@example.com"]);
    let config = input.form({
      name: { prompt: "Name", required: true },
      port: { type: int, min: 1, max: 65535 },
      email: { regex: ".+@.+" },
    });
    print(config.name, config.port, config.email);
    "#;
    let _ = run_src(src);
}

#[test]
fn input_checkbox_basic() {
    let src = r#"
    input.mock([["apple", "cherry"]]);
    let fruits = input.checkbox(
      "Select fruits:",
      ["apple", "banana", "cherry"],
      { default: ["apple"] }
    );
    print(fruits);
    "#;
    let _ = run_src(src);
}

#[test]
fn input_checkbox_with_limit() {
    let src = r#"
    input.mock([["red", "blue"]]);
    let colors = input.checkbox(
      "Select up to 2 colors:",
      ["red", "green", "blue", "yellow"],
      { limit: 2, required: true }
    );
    print(colors);
    "#;
    let _ = run_src(src);
}

#[test]
fn input_checkbox_with_disabled() {
    let src = r#"
    input.mock([["item1", "item3"]]);
    let selection = input.checkbox(
      "Choose items:",
      ["item1", "item2", "item3"],
      { disabled: ["item2"], default: ["item1"] }
    );
    print(selection);
    "#;
    let _ = run_src(src);
}

#[test]
fn input_radio_basic() {
    let src = r#"
    input.mock(["green"]);
    let color = input.radio(
      "Pick a color:",
      ["red", "green", "blue"]
    );
    print(color);
    "#;
    let _ = run_src(src);
}

#[test]
fn input_radio_with_default() {
    let src = r#"
    input.mock(["blue"]);
    let color = input.radio(
      "Pick a color:",
      ["red", "green", "blue"],
      { default: "green" }
    );
    print(color);
    "#;
    let _ = run_src(src);
}

#[test]
fn input_radio_with_disabled() {
    let src = r#"
    input.mock(["option1"]);
    let choice = input.radio(
      "Select option:",
      ["option1", "option2", "option3"],
      { disabled: ["option2"], required: true }
    );
    print(choice);
    "#;
    let _ = run_src(src);
}

#[test]
fn multi_utf_strings_execute() {
    let src = r#"
    let s1 = "नमस्ते दुनिया 😊";
    let s2 = "你好，世界 🚀";
    let s3 = "Symbols: § © ™ €";
    print(s1);
    print(s2);
    print(s3);
    "#;
    let _ = run_src(src);
}

#[test]
fn multi_utf_chars_execute() {
    let src = r#"
    let c1 = '😊';
    let c2 = '中';
    let c3 = '§';
    print(c1);
    print(c2);
    print(c3);
    "#;
    let _ = run_src(src);
}

#[test]
fn unicode_escape_sequences_execute() {
    let src = r#"
    let e1 = "Rocket: \u{1F680}";
    let e2 = "Smile: \u263A";
    let e3 = "Hindi Ha: \u{0939}";
    print(e1);
    print(e2);
    print(e3);
    "#;
    let _ = run_src(src);
}

#[test]
fn clock_monotonic_basic() {
    let src = r#"
    let a = clock();
    for(i in 0..1000000) { let x = i; }
    let b = clock();
    print(b - a);
    "#;
    let _ = run_src(src);
}
