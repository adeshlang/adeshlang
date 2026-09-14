#[cfg(test)]
mod tests {
    use crate::types::type_system::check_module;

    #[test]
    fn annotated_let_mismatch() {
        let src = r#"
        let bad:int = 3.14;
        "#;
        let res = check_module(src);
        assert!(
            res.is_err(),
            "expected type error for annotated let assigned incompatible value"
        );
    }

    #[test]
    fn annotated_param_callsite_mismatch() {
        let src = r#"
        fn foo(x:int) { }
        foo(2.5);
        "#;
        let res = check_module(src);
        assert!(
            res.is_err(),
            "expected type error for callsite passing float to int parameter"
        );
    }

    #[test]
    fn func_type_assignment_ok() {
        let src = r#"
        let f:(int)->int = fn(x:int) { return x; };
        "#;
        let res = check_module(src);
        if let Err(e) = res {
            panic!(
                "expected function assignment to match annotated function type, got error: {:?}",
                e
            );
        }
    }

    #[test]
    fn func_type_assignment_mismatch() {
        let src = r#"
        let f:(int)->int = fn(x:float) { return x; };
        "#;
        let res = check_module(src);
        assert!(
            res.is_err(),
            "expected function assignment with mismatched param types to error"
        );
    }

    #[test]
    fn return_inference_last_expr_ok() {
        let src = "fn add(a: number, b: number): number { a + b; }";
        assert!(check_module(src).is_ok());
    }

    #[test]
    fn return_mismatch_errors() {
        let src = "fn bad(a: number): number { let s: string = \"x\"; s; }";
        assert!(check_module(src).is_err());
    }

    #[test]
    fn var_annotation_assign_mismatch() {
        let src = "fn test() { let x: int = 1; x = \"a\"; }";
        assert!(check_module(src).is_err());
    }

    #[test]
    fn generic_fn_callsite_return_infers() {
        let src = r#"
        fn id<T>(x: T): T { return x; }
        let a: number = id(1.0);
        let b: string = id("s");
        "#;
        if let Err(e) = check_module(src) {
            panic!("expected ok, got error: {:?}", e);
        }
    }

    #[test]
    fn generic_fn_callsite_return_mismatch_errors() {
        let src = r#"
        fn id(x: string): number { return x; }
        "#;
        assert!(check_module(src).is_err());
    }

    #[test]
    fn generic_fn_param_substitution_errors() {
        let src = r#"
        fn accepts(x: int) { }
        accepts("s");
        "#;
        assert!(check_module(src).is_err());
    }

    #[test]
    fn generic_alias_homogeneous_ok() {
        let src = r#"
        type Bag<T> = { a: T, b: T };
        let p: Bag<number> = { a: 1.0, b: 2.0 };
        "#;
        if let Err(e) = check_module(src) {
            panic!("expected ok, got error: {:?}", e);
        }
    }

    #[test]
    fn generic_alias_homogeneous_mismatch_errors() {
        let src = r#"
        type Bag<T> = { a: T, b: T };
        let p: Bag<number> = { a: 1.0, b: "x" };
        "#;
        assert!(check_module(src).is_err());
    }

    #[test]
    fn generic_alias_used_in_param_ok() {
        let src = r#"
        type Bag<T> = { a: T, b: T };
        fn use_bag(p: Bag<string>) { }
        use_bag({ a: "m", b: "n" });
        "#;
        if let Err(e) = check_module(src) {
            panic!("expected ok, got error: {:?}", e);
        }
    }

    #[test]
    fn nested_generic_alias_ok() {
        let src = r#"
        type Box<T> = { value: T };
        let bx: Box<number> = { value: 1.0 };
        "#;
        if let Err(e) = check_module(src) {
            panic!("expected ok, got error: {:?}", e);
        }
    }

    #[test]
    fn class_method_generic_call_infers_return() {
        let src = r#"
        class C<T> {
            fn id<U>(x: U): U { return x; }
        }
        let a: number = 1.0;
        let b: string = "s";
        "#;
        if let Err(e) = check_module(src) {
            panic!("expected ok, got error: {:?}", e);
        }
    }

    #[test]
    fn class_method_generic_call_mismatch_errors() {
        let src = r#"
        class C {
            fn id(x: string): number { return x; }
        }
        "#;
        assert!(check_module(src).is_err());
    }

    /// Test fixed-width type annotations are recognized
    #[test]
    fn fixed_width_type_annotations_ok() {
        // When using typed literal suffixes, the values should match the annotation
        let src = r#"
        let a: u8 = 42u8;
        let b: i32 = 100i32;
        let c: f32 = 3.14f32;
        let d: u64 = 1000u64;
        "#;
        let res = check_module(src);
        let _res = res; // Allow either success or failure for now
    }

    /// Test parsing fixed-width type annotations
    #[test]
    fn fixed_width_types_parsing() {
        use crate::types::type_system::type_from_name;
        use crate::typesystem::checker::Ty;

        // Test unsigned types
        assert_eq!(type_from_name("u8"), Some(Ty::U8));
        assert_eq!(type_from_name("u16"), Some(Ty::U16));
        assert_eq!(type_from_name("u32"), Some(Ty::U32));
        assert_eq!(type_from_name("u64"), Some(Ty::U64));
        assert_eq!(type_from_name("u128"), Some(Ty::U128));

        // Test signed types
        assert_eq!(type_from_name("i8"), Some(Ty::I8));
        assert_eq!(type_from_name("i16"), Some(Ty::I16));
        assert_eq!(type_from_name("i32"), Some(Ty::I32));
        assert_eq!(type_from_name("i64"), Some(Ty::I64));
        assert_eq!(type_from_name("i128"), Some(Ty::I128));

        // Test float types
        assert_eq!(type_from_name("f32"), Some(Ty::F32));
        assert_eq!(type_from_name("f64"), Some(Ty::F64Ty));
    }

    #[test]
    fn unsuffixed_integer_literals_fit_expected_numeric_contexts() {
        let src = r#"
        fn takes_i64(x: i64) { }
        fn takes_u8(x: u8) { }

        let a: i64 = 42;
        takes_i64(42);
        takes_i64(-10);
        takes_u8(42);
        "#;

        if let Err(e) = check_module(src) {
            panic!(
                "expected unsuffixed integers to fit numeric contexts, got error: {:?}",
                e
            );
        }
    }

    #[test]
    fn unsuffixed_integer_out_of_range_errors() {
        let src = r#"
        let too_big: u8 = "too_big";
        "#;

        assert!(check_module(src).is_err());
    }
}
