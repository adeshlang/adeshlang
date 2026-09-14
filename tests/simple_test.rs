use adeshlang::{Interpreter, ModuleLoader};

fn run_test_module(src: &str) -> Result<(), String> {
    let src_owned = src.to_string();
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let mut loader = ModuleLoader::new(std::path::Path::new("."));
            let mut interp = Interpreter::new();
            interp.run_module(&src_owned, &mut loader, None).map_err(|e| e.to_string())
        })
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn simple_arithmetic() {
    let src = "print(2 + 3);";
    assert!(run_test_module(src).is_ok());
}

#[test]
fn simple_function() {
    let src = r#"
fn add(a, b) { return a + b; }
print(add(4, 5));
"#;
    assert!(run_test_module(src).is_ok());
}
