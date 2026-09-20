use adeshlang::parsing::ast::Value;
use adeshlang::runtime::abi::bitwise::{self, BitIntrinsic as Intrinsic, BitOp};

fn same(actual: Value, expected: Value) {
    assert_eq!(
        std::mem::discriminant(&actual),
        std::mem::discriminant(&expected)
    );
    assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
}

#[test]
fn all_integer_widths_and_randomized_identities() {
    macro_rules! check {
        ($variant:ident, $ty:ty) => {{
            let mut state = 0x9e3779b97f4a7c15_u128;
            for _ in 0..2048 {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                let x = state as $ty;
                let v = Value::$variant(x);
                let zero = Value::$variant(0);
                let ones = Value::$variant(!0);
                same(bitwise::binary(BitOp::Xor, &v, &zero).unwrap(), v.clone());
                same(bitwise::binary(BitOp::Or, &v, &zero).unwrap(), v.clone());
                same(bitwise::binary(BitOp::And, &v, &ones).unwrap(), v.clone());
                same(bitwise::binary(BitOp::Xor, &v, &v).unwrap(), zero);
                same(
                    bitwise::complement(&bitwise::complement(&v).unwrap()).unwrap(),
                    v.clone(),
                );
                for shift in [0, 1, <$ty>::BITS - 1] {
                    let count = Value::U32(shift);
                    same(
                        bitwise::binary(BitOp::Shl, &v, &count).unwrap(),
                        Value::$variant(x.wrapping_shl(shift)),
                    );
                    same(
                        bitwise::binary(BitOp::Shr, &v, &count).unwrap(),
                        Value::$variant(x >> shift),
                    );
                    same(
                        Intrinsic::RotateLeft
                            .eval(&[v.clone(), count.clone()])
                            .unwrap(),
                        Value::$variant(x.rotate_left(shift)),
                    );
                    same(
                        Intrinsic::RotateRight.eval(&[v.clone(), count]).unwrap(),
                        Value::$variant(x.rotate_right(shift)),
                    );
                }
                same(
                    Intrinsic::Count.eval(&[v.clone()]).unwrap(),
                    Value::U32(x.count_ones()),
                );
                same(
                    Intrinsic::LeadingZeros.eval(&[v.clone()]).unwrap(),
                    Value::U32(x.leading_zeros()),
                );
                same(
                    Intrinsic::TrailingZeros.eval(&[v.clone()]).unwrap(),
                    Value::U32(x.trailing_zeros()),
                );
                same(
                    Intrinsic::LeadingOnes.eval(&[v.clone()]).unwrap(),
                    Value::U32(x.leading_ones()),
                );
                same(
                    Intrinsic::TrailingOnes.eval(&[v.clone()]).unwrap(),
                    Value::U32(x.trailing_ones()),
                );
                same(
                    Intrinsic::Reverse.eval(&[v.clone()]).unwrap(),
                    Value::$variant(x.reverse_bits()),
                );
                same(
                    Intrinsic::SwapBytes.eval(&[v.clone()]).unwrap(),
                    Value::$variant(x.swap_bytes()),
                );
                for invalid in [
                    Value::I32(-1),
                    Value::U32(<$ty>::BITS),
                    Value::U128(u128::MAX),
                ] {
                    assert!(bitwise::binary(BitOp::Shl, &v, &invalid).is_err());
                    assert!(bitwise::binary(BitOp::Shr, &v, &invalid).is_err());
                }
            }
        }};
    }
    check!(U8, u8);
    check!(U16, u16);
    check!(U32, u32);
    check!(U64, u64);
    check!(U128, u128);
    check!(I8, i8);
    check!(I16, i16);
    check!(I32, i32);
    check!(I64, i64);
    check!(I128, i128);
}

#[test]
fn fields_masks_and_invalid_types() {
    same(
        Intrinsic::Mask.eval(&[Value::U8(64)]).unwrap(),
        Value::U64(u64::MAX),
    );
    same(
        Intrinsic::MaskAt
            .eval(&[Value::U8(64), Value::U8(0)])
            .unwrap(),
        Value::U64(0),
    );
    assert!(
        Intrinsic::MaskAt
            .eval(&[Value::U128(u128::MAX), Value::U8(1)])
            .is_err()
    );
    for (offset, width) in [(0, 128), (127, 1), (64, 64), (128, 0), (4, 8)] {
        let args = [
            Value::U128(0),
            Value::U32(offset),
            Value::U32(width),
            Value::U128(u128::MAX),
        ];
        let packed = Intrinsic::Insert.eval(&args).unwrap();
        same(
            Intrinsic::Extract
                .eval(&[packed, args[1].clone(), args[2].clone()])
                .unwrap(),
            Value::U128(bitwise::mask(width).unwrap()),
        );
    }
    same(
        Intrinsic::Test
            .eval(&[Value::U128(1 << 127), Value::U8(127)])
            .unwrap(),
        Value::Bool(true),
    );
    assert!(Intrinsic::Set.eval(&[Value::U8(0), Value::U8(8)]).is_err());
    for bad in [
        Value::F32(1.0),
        Value::F64(2.0),
        Value::Number(1.5),
        Value::Number(f64::NAN),
        Value::Bool(true),
    ] {
        assert!(bitwise::binary(BitOp::And, &bad, &bad).is_err());
        assert!(bitwise::complement(&bad).is_err());
        assert!(bitwise::binary(BitOp::Shl, &Value::U8(1), &bad).is_err());
    }
    assert!(bitwise::binary(BitOp::And, &Value::U32(1), &Value::U64(1)).is_err());
    same(
        bitwise::binary(BitOp::And, &Value::U8(3), &Value::Number(1.0)).unwrap(),
        Value::U8(1),
    );
    same(
        bitwise::binary(BitOp::And, &Value::Number(1.0), &Value::U8(3)).unwrap(),
        Value::U8(1),
    );
}

#[test]
fn bigint_infinite_twos_complement() {
    let x = num_bigint::BigInt::from(1u8) << 200usize;
    same(
        bitwise::complement(&Value::BigInt(x.clone())).unwrap(),
        Value::BigInt(-&x - 1),
    );
    same(
        bitwise::binary(BitOp::Shr, &Value::BigInt(-&x), &Value::U8(200)).unwrap(),
        Value::BigInt((-1).into()),
    );
    same(
        bitwise::binary(
            BitOp::And,
            &Value::BigInt(x.clone()),
            &Value::BigInt((-1).into()),
        )
        .unwrap(),
        Value::BigInt(x),
    );
    assert!(bitwise::binary(BitOp::Shl, &Value::BigInt(1.into()), &Value::I8(-1)).is_err());
}

fn interpret(source: &str) -> Result<(), String> {
    let source = source.to_owned();
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let mut loader =
                adeshlang::ModuleLoader::new(std::path::Path::new(env!("CARGO_MANIFEST_DIR")));
            adeshlang::Interpreter::new()
                .run_module(&source, &mut loader, None)
                .map_err(|e| e.to_string())
        })
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn interpreter_literals_operators_and_intrinsics() {
    interpret(r#"
        let a = 0b1111_0000u8;
        let b = 0b1010_1010u8;
        if (a & b) != 160u8 { throw "and"; }
        if (a | b) != 250u8 { throw "or"; }
        if (a ^ b) != 90u8 { throw "xor"; }
        if ~a != 15u8 { throw "not"; }
        if (a >> 4) != 15u8 { throw "shift"; }
        if (a << 4) != 0u8 { throw "wrap"; }
        if std.bit_count(a) != 4 { throw "count"; }
        if std.rotate_left(1u128, 127) != 170141183460469231731687303715884105728u128 { throw "wide rotate"; }
        if std.bit_extract(255u32, 4, 4) != 15u32 { throw "extract"; }
    "#).unwrap();
}

#[test]
fn interpreter_compound_assignment_evaluates_index_once() {
    interpret(
        r#"
        class Counter {
            Counter() { this.n = 0; }
            fn index() { this.n = this.n + 1; return 0; }
        }
        let c = new Counter();
        let values = [255u8];
        values[c.index()] &= 15u8;
        if c.n != 1 { throw "index evaluated twice"; }
        if values[0] != 15u8 { throw "compound result"; }
    "#,
    )
    .unwrap();
}

#[test]
fn interpreter_rejects_invalid_shift() {
    assert!(
        interpret("let x = 1u8; let y = x << 8;")
            .unwrap_err()
            .contains("invalid shift")
    );
}

#[test]
fn checker_rejects_float_and_mixed_width_operands() {
    for source in [
        "let x = 1.5 & 2.0;",
        "let x = 1u32 & 2u64;",
        "let x = ~1.5;",
    ] {
        assert!(
            adeshlang::types::type_system::check_module_in(source, None).is_err(),
            "{source}"
        );
    }
}

#[test]
fn interpreter_permission_flags_pattern() {
    let src = r#"
        const READ: u8 = 1u8 << 0;
        const WRITE: u8 = 1u8 << 1;
        fn has_permission(flags: u8, permission: u8): bool {
            return (flags & permission) != 0;
        }
        let flags: u8 = READ | WRITE;
        flags |= 4u8;
        flags &= ~WRITE;
        if !has_permission(flags, READ) { throw "read lost"; }
        if has_permission(flags, WRITE) { throw "write not cleared"; }
        if !has_permission(flags, 4u8) { throw "execute lost"; }
        if std.bit_count(flags) != 2 { throw "count"; }
    "#;
    interpret(src).unwrap();
    assert!(
        adeshlang::types::type_system::check_module_in(src, None).is_ok(),
        "type checker rejected the flags pattern"
    );
}

#[test]
fn const_flag_pattern_with_unsuffixed_literals() {
    // `const FLAG: u32 = 1 << 0;` must evaluate at compile time and type-check.
    let src = "const FLAG_READY: u32 = 1 << 0;\nprint(FLAG_READY);";
    assert!(
        adeshlang::types::type_system::check_module_in(src, None).is_ok(),
        "type checker rejected unsuffixed const shift"
    );
    interpret(src).unwrap();
}
