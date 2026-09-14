//! Type System Tests
//!
//! Tests for union types, nullable types, visibility, and type narrowing.

#[cfg(test)]
mod tests {
    use adeshlang::parsing::ast::{TokenKind, Value};
    use adeshlang::parsing::lexer::Lexer;
    use adeshlang::{Interpreter, ModuleLoader};
    use std::path::Path;

    fn run_code(src: &str) -> Result<(), String> {
        let src_str = src.to_string();
        let h = std::thread::Builder::new()
            .name("adesh_test_thread".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || {
                let mut loader = ModuleLoader::new(Path::new("."));
                let mut interp = Interpreter::new();
                interp.run_module(&src_str, &mut loader, None)
            })
            .unwrap();
        h.join().unwrap().map_err(|e| e.to_string())
    }

    /// Test union type handling placeholder
    #[test]
    fn test_union_types() {
        // Union types are used internally (e.g. nullable is Union of T | Null)
        let src = r#"
        let x: int? = 10;
        "#;
        let res = run_code(src);
        assert!(res.is_ok());
    }

    /// Test nullable type checking
    #[test]
    fn test_nullable_types() {
        let src = r#"
        let x: int? = null;
        let y: int? = 10;
        "#;
        let res = run_code(src);
        assert!(
            res.is_ok(),
            "Expected nullable type code to run ok, got error: {:?}",
            res.err()
        );

        let bad_src = r#"
        let x: int = null;
        "#;
        let res2 = run_code(bad_src);
        assert!(
            res2.is_err(),
            "Expected type mismatch for null in non-nullable int"
        );
    }

    /// Test type narrowing in conditionals
    #[test]
    fn test_type_narrowing() {
        let src = r#"
        fn process(x: int?) {
            if (x != null) {
                let val: int = x; // should be narrowed to int
            }
        }
        "#;
        let res = run_code(src);
        assert!(
            res.is_ok(),
            "Expected narrowing code to run ok, got error: {:?}",
            res.err()
        );
    }

    /// Test visibility modifier enforcement
    #[test]
    fn test_visibility_enforcement() {
        let src = r#"
        class Parent {
            private fn secret() { return "private"; }
            protected fn family() { return "protected"; }
        }
        class Child extends Parent {
            fn run_test() {
                return this.family(); // protected is ok
            }
        }
        let c = new Child();
        print(c.run_test());
        "#;
        let res = run_code(src);
        assert!(
            res.is_ok(),
            "Expected visibility code to run ok, got error: {:?}",
            res.err()
        );

        // Calling protected from outside should fail
        let bad_src = r#"
        class Parent {
            protected fn family() {}
        }
        class Child extends Parent {}
        let c = new Child();
        c.family();
        "#;
        assert!(
            run_code(bad_src).is_err(),
            "Expected external access to protected method to fail"
        );
    }

    // Additional type tests would go here:
    // - union_narrowing
    // - nullable_propagation
    // - visibility_errors
    // - protected_inheritance

    /// Test lexing of typed numeric literal suffixes
    #[test]
    fn test_typed_numeric_literal_lexing() {
        // Test unsigned integer suffixes
        let src = "10u8 255u16 1024u32";
        let mut lx = Lexer::new(src);
        let toks = lx.tokenize().expect("tokenize failed");

        assert!(
            toks.iter()
                .any(|t| matches!(t.kind, TokenKind::U8Lit) && t.lexeme == "10")
        );
        assert!(
            toks.iter()
                .any(|t| matches!(t.kind, TokenKind::U16Lit) && t.lexeme == "255")
        );
        assert!(
            toks.iter()
                .any(|t| matches!(t.kind, TokenKind::U32Lit) && t.lexeme == "1024")
        );
    }

    /// Test lexing of signed integer literal suffixes
    #[test]
    fn test_signed_integer_literal_lexing() {
        let src = "10i8 255i16 1024i32 65536i64";
        let mut lx = Lexer::new(src);
        let toks = lx.tokenize().expect("tokenize failed");

        assert!(
            toks.iter()
                .any(|t| matches!(t.kind, TokenKind::I8Lit) && t.lexeme == "10")
        );
        assert!(
            toks.iter()
                .any(|t| matches!(t.kind, TokenKind::I16Lit) && t.lexeme == "255")
        );
        assert!(
            toks.iter()
                .any(|t| matches!(t.kind, TokenKind::I32Lit) && t.lexeme == "1024")
        );
        assert!(
            toks.iter()
                .any(|t| matches!(t.kind, TokenKind::I64Lit) && t.lexeme == "65536")
        );
    }

    /// Test lexing of float literal suffixes
    #[test]
    fn test_float_literal_lexing() {
        let src = "3.14f32 2.718f64";
        let mut lx = Lexer::new(src);
        let toks = lx.tokenize().expect("tokenize failed");

        assert!(
            toks.iter()
                .any(|t| matches!(t.kind, TokenKind::F32Lit) && t.lexeme == "3.14")
        );
        assert!(
            toks.iter()
                .any(|t| matches!(t.kind, TokenKind::F64Lit) && t.lexeme == "2.718")
        );
    }

    /// Test Value::truthy for fixed-width types
    #[test]
    fn test_fixed_width_truthy() {
        // Test unsigned types
        assert!(Value::U8(1).truthy());
        assert!(!Value::U8(0).truthy());
        assert!(Value::U16(1).truthy());
        assert!(!Value::U16(0).truthy());
        assert!(Value::U32(1).truthy());
        assert!(!Value::U32(0).truthy());
        assert!(Value::U64(1).truthy());
        assert!(!Value::U64(0).truthy());
        assert!(Value::U128(1).truthy());
        assert!(!Value::U128(0).truthy());

        // Test signed types
        assert!(Value::I8(1).truthy());
        assert!(Value::I8(-1).truthy());
        assert!(!Value::I8(0).truthy());
        assert!(Value::I16(1).truthy());
        assert!(!Value::I16(0).truthy());
        assert!(Value::I32(1).truthy());
        assert!(!Value::I32(0).truthy());
        assert!(Value::I64(1).truthy());
        assert!(!Value::I64(0).truthy());
        assert!(Value::I128(1).truthy());
        assert!(!Value::I128(0).truthy());

        // Test float types
        assert!(Value::F32(1.0).truthy());
        assert!(!Value::F32(0.0).truthy());
        assert!(Value::F64(1.0).truthy());
        assert!(!Value::F64(0.0).truthy());
    }

    /// Test Value Debug formatting for fixed-width types
    #[test]
    fn test_fixed_width_debug() {
        // Note: Debug output for fixed-width types does NOT include type suffix
        assert_eq!(format!("{:?}", Value::U8(255)), "255");
        assert_eq!(format!("{:?}", Value::U16(65535)), "65535");
        assert_eq!(format!("{:?}", Value::I32(-100)), "-100");
        assert_eq!(format!("{:?}", Value::F32(3.14)), "3.14");
        assert_eq!(format!("{:?}", Value::F64(2.718)), "2.718");
    }
}
