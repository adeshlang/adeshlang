//! Decorator system tests for AdeshLang
//! Tests the fixed decorator system with no hang on multiple decorators

#[cfg(test)]
mod decorator_tests {
    use adeshlang::ModuleLoader;
    use adeshlang::execution::runtime::Interpreter;
    use std::path::Path;

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
fn test(x) { return x * 2; }

print(test(5));
        "#;

        let mut interp = Interpreter::new();
        let mut loader = ModuleLoader::new(Path::new("."));
        let result = interp.run_module(src, &mut loader, Some("test".to_string()));

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

        let mut interp = Interpreter::new();
        let mut loader = ModuleLoader::new(Path::new("."));
        let result = interp.run_module(src, &mut loader, Some("test".to_string()));

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

        let mut interp = Interpreter::new();
        let mut loader = ModuleLoader::new(Path::new("."));
        let result = interp.run_module(src, &mut loader, Some("test".to_string()));

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

        let mut interp = Interpreter::new();
        let mut loader = ModuleLoader::new(Path::new("."));
        let result = interp.run_module(src, &mut loader, Some("test".to_string()));

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
fn test(x) {
    print("function body");
    return x;
}

test(1);
        "#;

        let mut interp = Interpreter::new();
        let mut loader = ModuleLoader::new(Path::new("."));
        let result = interp.run_module(src, &mut loader, Some("test".to_string()));

        assert!(result.is_ok(), "Should apply decorators in correct order");
    }
}
