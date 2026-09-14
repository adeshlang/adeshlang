// Tests for type layout engine

#[cfg(test)]
mod type_layout_tests {
    use adeshlang::types::type_layout::{compute_layout, size_of, align_of};
    use adeshlang::types::typechecker::Ty;

    #[test]
    fn test_primitive_sizes() {
        assert_eq!(size_of(&Ty::U8).unwrap(), 1);
        assert_eq!(size_of(&Ty::U16).unwrap(), 2);
        assert_eq!(size_of(&Ty::U32).unwrap(), 4);
        assert_eq!(size_of(&Ty::U64).unwrap(), 8);
        assert_eq!(size_of(&Ty::U128).unwrap(), 16);

        assert_eq!(size_of(&Ty::I8).unwrap(), 1);
        assert_eq!(size_of(&Ty::I16).unwrap(), 2);
        assert_eq!(size_of(&Ty::I32).unwrap(), 4);
        assert_eq!(size_of(&Ty::I64).unwrap(), 8);
        assert_eq!(size_of(&Ty::I128).unwrap(), 16);

        assert_eq!(size_of(&Ty::F32).unwrap(), 4);
        assert_eq!(size_of(&Ty::F64Ty).unwrap(), 8);
    }

    #[test]
    fn test_bool_char_sizes() {
        assert_eq!(size_of(&Ty::Bool).unwrap(), 1);
        assert_eq!(size_of(&Ty::Char).unwrap(), 4); // UTF-32
    }

    #[test]
    fn test_pointer_size() {
        assert_eq!(size_of(&Ty::Ptr(Box::new(Ty::U32))).unwrap(), 8);
        assert_eq!(size_of(&Ty::PtrOwning(Box::new(Ty::I64))).unwrap(), 8);
        assert_eq!(size_of(&Ty::PtrShared(Box::new(Ty::U8))).unwrap(), 8);
        assert_eq!(size_of(&Ty::PtrMut(Box::new(Ty::F64Ty))).unwrap(), 8);
    }

    #[test]
    fn test_pointer_align() {
        assert_eq!(align_of(&Ty::Ptr(Box::new(Ty::U32))).unwrap(), 8);
        assert_eq!(align_of(&Ty::PtrOwning(Box::new(Ty::I64))).unwrap(), 8);
        assert_eq!(align_of(&Ty::PtrShared(Box::new(Ty::U8))).unwrap(), 8);
        assert_eq!(align_of(&Ty::PtrMut(Box::new(Ty::F64Ty))).unwrap(), 8);
    }

    #[test]
    fn test_empty_tuple() {
        assert_eq!(size_of(&Ty::Tuple(vec![])).unwrap(), 0);
    }

    #[test]
    fn test_simple_tuple() {
        let ty = Ty::Tuple(vec![Ty::U8, Ty::U32]);
        let layout = compute_layout(&ty).unwrap();
        // u8 (1 byte) + padding (3 bytes) + u32 (4 bytes) = 8 bytes
        assert_eq!(layout.size, 8);
        assert_eq!(layout.align, 4);
    }

    #[test]
    fn test_tuple_with_large_element() {
        let ty = Ty::Tuple(vec![Ty::U64, Ty::U8]);
        let layout = compute_layout(&ty).unwrap();
        // u64 (8 bytes) + u8 (1 byte) + padding (7 bytes) = 16 bytes
        assert_eq!(layout.size, 16);
        assert_eq!(layout.align, 8);
    }

    #[test]
    fn test_empty_record() {
        let ty = Ty::Record {
            required: vec![],
            optional: vec![],
        };
        assert_eq!(size_of(&ty).unwrap(), 0);
    }

    #[test]
    fn test_simple_record() {
        let ty = Ty::Record {
            required: vec![
                ("a".to_string(), Ty::U8),
                ("b".to_string(), Ty::U32),
            ],
            optional: vec![],
        };
        let layout = compute_layout(&ty).unwrap();
        // u8 (1) + padding (3) + u32 (4) = 8
        assert_eq!(layout.size, 8);
        assert_eq!(layout.align, 4);
    }

    #[test]
    fn test_record_with_optional_fields() {
        let ty = Ty::Record {
            required: vec![("a".to_string(), Ty::U32)],
            optional: vec![("b".to_string(), Ty::U8)],
        };
        let layout = compute_layout(&ty).unwrap();
        // u32 (4) + padding (0) + u8 (1) + padding (3) = 8
        assert_eq!(layout.size, 8);
        assert_eq!(layout.align, 4);
    }

    #[test]
    fn test_unknown_type_error() {
        assert!(size_of(&Ty::Any).is_err());
        assert!(size_of(&Ty::Unknown).is_err());
        assert!(align_of(&Ty::Any).is_err());
        assert!(align_of(&Ty::Unknown).is_err());
    }

    #[test]
    fn test_void_zero_size() {
        assert_eq!(size_of(&Ty::Void).unwrap(), 0);
        assert_eq!(size_of(&Ty::Never).unwrap(), 0);
    }

    #[test]
    fn test_nullable_adds_tag() {
        let ty = Ty::Nullable(Box::new(Ty::U32));
        let layout = compute_layout(&ty).unwrap();
        // u32 (4) + tag (1) + padding (3) = 8
        assert_eq!(layout.size, 8);
        assert_eq!(layout.align, 4);
    }

    #[test]
    fn test_union_max_size() {
        let ty = Ty::Union(vec![Ty::U32, Ty::U64, Ty::U8]);
        let layout = compute_layout(&ty).unwrap();
        // Size is max: u64 = 8
        assert_eq!(layout.size, 8);
        assert_eq!(layout.align, 8);
    }

    #[test]
    fn test_function_type_size() {
        let ty = Ty::Func {
            params: vec![Ty::U32, Ty::U64],
            ret: Box::new(Ty::U8),
        };
        assert_eq!(size_of(&ty).unwrap(), 8);
        assert_eq!(align_of(&ty).unwrap(), 8);
    }

    #[test]
    fn test_string_type_layout() {
        let layout = compute_layout(&Ty::Str).unwrap();
        assert_eq!(layout.size, 24);
        assert_eq!(layout.align, 8);
    }

    #[test]
    fn test_map_type_layout() {
        let ty = Ty::Map(Box::new(Ty::Str), Box::new(Ty::U32));
        let layout = compute_layout(&ty).unwrap();
        assert_eq!(layout.size, 24); // Like string: ptr + len + cap
        assert_eq!(layout.align, 8);
    }

    #[test]
    fn test_generic_param_error() {
        let ty = Ty::GenericParam("T".to_string());
        assert!(size_of(&ty).is_err());
        assert!(align_of(&ty).is_err());
    }

    #[test]
    fn test_logical_types() {
        // Logical types have default sizes
        assert_eq!(size_of(&Ty::Int).unwrap(), 8);  // i64 equivalent
        assert_eq!(size_of(&Ty::Float).unwrap(), 8); // f64 equivalent
    }

    #[test]
    fn test_layout_display() {
        let ty = Ty::U32;
        let layout = compute_layout(&ty).unwrap();
        let display = format!("{}", layout);
        assert!(display.contains("4B"));
        assert!(display.contains("align 4B"));
    }
}
