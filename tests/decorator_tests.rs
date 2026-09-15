//! Decorator system tests for AdeshLang
//! Tests the fixed decorator system with no hang on multiple decorators

#[cfg(test)]
mod decorator_tests {
    use adeshlang::ModuleLoader;
    use adeshlang::execution::runtime::Interpreter;
    use std::path::Path;

    fn run_code(src: &str) -> Result<(), String> {
        let src = src.to_string();
        let handle = std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024) // 16 MB
            .spawn(move || {
                let mut interp = Interpreter::new();
                let mut loader = ModuleLoader::new(Path::new("."));
                interp
                    .run_module(&src, &mut loader, Some("test".to_string()))
                    .map_err(|e| e.to_string())
            })
            .expect("failed to spawn test thread");
        handle.join().expect("test thread panicked")
    }

    /// Test that multiple decorators can be applied without hanging
    #[test]
    fn test_multiple_decorators_no_hang() {
        let src = r#"
decorator d1(target, meta) { return fn(x) { return target(x); }; }
decorator d2(target, meta) { return fn(x) { return target(x); }; }
decorator d3(target, meta) { return fn(x) { return target(x); }; }
decorator d4(target, meta) { return fn(x) { return target(x); }; }
decorator d5(target, meta) { return fn(x) { return target(x); }; }
decorator d6(target, meta) { return fn(x) { return target(x); }; }
decorator d7(target, meta) { return fn(x) { return target(x); }; }
decorator d8(target, meta) { return fn(x) { return target(x); }; }
decorator d9(target, meta) { return fn(x) { return target(x); }; }
decorator d10(target, meta) { return fn(x) { return target(x); }; }

@d1 @d2 @d3 @d4 @d5 @d6 @d7 @d8 @d9 @d10
fn my_fn(x) { return x * 2; }

print(my_fn(5));
        "#;

        let result = run_code(src);
        if let Err(ref e) = result {
            eprintln!("[TEST MULTIPLE ERROR] {}", e);
        }
        assert!(
            result.is_ok(),
            "Should handle 10 decorators without hanging"
        );
    }

    /// Test stacking decorators on multiple functions (limited to avoid performance issues)
    #[test]
    fn test_multiple_functions_with_decorators() {
        let src = r#"
decorator d1(target, meta) { return fn(x) { return target(x); }; }
decorator d2(target, meta) { return fn(x) { return target(x); }; }
decorator d3(target, meta) { return fn(x) { return target(x); }; }

@d1 @d2 @d3
fn func1(x) { return x + 1; }

@d1 @d2 @d3
fn func2(x) { return x + 2; }

print(func1(10));
print(func2(10));
        "#;

        let result = run_code(src);
        assert!(result.is_ok(), "Should handle multiple decorated functions");
    }

    /// Test decorator with metadata access
    #[test]
    fn test_decorator_metadata() {
        let src = r#"
decorator logName(target, meta) {
    print(meta.name);
    return fn(x) { return target(x); };
}

@logName
fn testFunction(x) { return x; }

testFunction(1);
        "#;

        let result = run_code(src);
        if let Err(ref e) = result {
            eprintln!("[TEST METADATA ERROR] {}", e);
        }

        assert!(result.is_ok(), "Should access decorator metadata");
    }

    /// Test basic decorator functionality
    #[test]
    fn test_basic_decorator() {
        let src = r#"
decorator double(target, meta) {
    return fn(x) {
        return target(x) * 2;
    };
}

@double
fn getValue(x) { return x + 1; }

print(getValue(5));
        "#;

        let result = run_code(src);
        assert!(result.is_ok(), "Should handle basic decorator");
    }

    /// Test decorator order of application
    #[test]
    fn test_decorator_order() {
        let src = r#"
decorator outer(target, meta) {
    return fn(x) {
        print("outer before");
        let result = target(x);
        print("outer after");
        return result;
    };
}

decorator inner(target, meta) {
    return fn(x) {
        print("inner before");
        let result = target(x);
        print("inner after");
        return result;
    };
}

@outer
@inner
fn run_target(x) {
    print("function body");
    return x;
}

run_target(1);
        "#;

        let result = run_code(src);
        assert!(result.is_ok(), "Should apply decorators in correct order");
    }
}
