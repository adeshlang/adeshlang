use adeshlang::{Interpreter, ModuleLoader};

fn run_test_module(src: &str) -> Result<(), String> {
    let src_owned = src.to_string();
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let mut loader = ModuleLoader::new(std::path::Path::new("."));
            let mut interp = Interpreter::new();
            interp
                .run_module(&src_owned, &mut loader, None)
                .map_err(|e| e.to_string())
        })
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn simple_arithmetic() {
    let src = r#"
    let sum = 2 + 3;
    if (sum != 5) {
        throw "Arithmetic assertion failed: expected 5, got " + string(sum);
    }
    "#;
    assert!(run_test_module(src).is_ok());
}

#[test]
fn simple_function() {
    let src = r#"
    fn add(a, b) { return a + b; }
    let res = add(4, 5);
    if (res != 9) {
        throw "Function call assertion failed: expected 9, got " + string(res);
    }
    "#;
    assert!(run_test_module(src).is_ok());
}
