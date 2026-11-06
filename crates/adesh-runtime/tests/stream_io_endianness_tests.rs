use adesh_runtime::stream_io::{
    extract_bytes_from_arg, io_pack_number, io_unpack_number, parse_endian_str,
};
use adesh_runtime::{RuntimeValue, aot_get_value, aot_store_value};
use std::ffi::CString;

#[test]
fn test_parse_endian_str() {
    assert_eq!(parse_endian_str("big"), Ok(true));
    assert_eq!(parse_endian_str("BE"), Ok(true));
    assert_eq!(parse_endian_str("  bIg  "), Ok(true));

    assert_eq!(parse_endian_str("little"), Ok(false));
    assert_eq!(parse_endian_str("LE"), Ok(false));
    assert_eq!(parse_endian_str("  Le  "), Ok(false));
    assert_eq!(parse_endian_str(""), Ok(false));

    assert!(parse_endian_str("random").is_err());
    assert!(parse_endian_str("middle").is_err());
    assert!(parse_endian_str("other").is_err());
}

#[test]
fn test_io_pack_unpack_integers_endianness() {
    // 1. u16
    let type_u16 = CString::new("u16").unwrap();
    let le = CString::new("LE").unwrap();
    let be = CString::new("BE").unwrap();

    let packed_le_handle =
        io_pack_number(
            type_u16.as_ptr() as u64,
            0x1234,
            le.as_ptr() as u64,
        );
    let bytes_le = extract_bytes_from_arg(packed_le_handle);
    assert_eq!(bytes_le, vec![0x34, 0x12]);

    let packed_be_handle =
        io_pack_number(
            type_u16.as_ptr() as u64,
            0x1234,
            be.as_ptr() as u64,
        );
    let bytes_be = extract_bytes_from_arg(packed_be_handle);
    assert_eq!(bytes_be, vec![0x12, 0x34]);

    // Unpack u16
    let unpacked_le =
        io_unpack_number(
            type_u16.as_ptr() as u64,
            packed_le_handle,
            le.as_ptr() as u64,
        );
    assert_eq!(unpacked_le, 0x1234);

    let unpacked_be =
        io_unpack_number(
            type_u16.as_ptr() as u64,
            packed_be_handle,
            be.as_ptr() as u64,
        );
    assert_eq!(unpacked_be, 0x1234);

    // 2. u32
    let type_u32 = CString::new("u32").unwrap();
    let val_u32 = 0x11223344u64;

    let handle_u32_le =
        io_pack_number(
            type_u32.as_ptr() as u64,
            val_u32,
            le.as_ptr() as u64,
        );
    assert_eq!(extract_bytes_from_arg(handle_u32_le), vec![0x44, 0x33, 0x22, 0x11]);

    let handle_u32_be =
        io_pack_number(
            type_u32.as_ptr() as u64,
            val_u32,
            be.as_ptr() as u64,
        );
    assert_eq!(extract_bytes_from_arg(handle_u32_be), vec![0x11, 0x22, 0x33, 0x44]);

    // 3. u64
    let type_u64 = CString::new("u64").unwrap();
    let val_u64 = 0x0102030405060708u64;

    let handle_u64_le =
        io_pack_number(
            type_u64.as_ptr() as u64,
            val_u64,
            le.as_ptr() as u64,
        );
    assert_eq!(
        extract_bytes_from_arg(handle_u64_le),
        vec![0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]
    );

    let handle_u64_be =
        io_pack_number(
            type_u64.as_ptr() as u64,
            val_u64,
            be.as_ptr() as u64,
        );
    assert_eq!(
        extract_bytes_from_arg(handle_u64_be),
        vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]
    );

    let unpacked_u64_le =
        io_unpack_number(
            type_u64.as_ptr() as u64,
            handle_u64_le,
            le.as_ptr() as u64,
        );
    let val_le = match aot_get_value(unpacked_u64_le) {
        Some(RuntimeValue::U64(u)) => u,
        _ => unpacked_u64_le,
    };
    assert_eq!(val_le, val_u64);

    let unpacked_u64_be =
        io_unpack_number(
            type_u64.as_ptr() as u64,
            handle_u64_be,
            be.as_ptr() as u64,
        );
    let val_be = match aot_get_value(unpacked_u64_be) {
        Some(RuntimeValue::U64(u)) => u,
        _ => unpacked_u64_be,
    };
    assert_eq!(val_be, val_u64);
}

#[test]
fn test_io_pack_unpack_u128_i128() {
    let type_u128 = CString::new("u128").unwrap();
    let type_i128 = CString::new("i128").unwrap();
    let le = CString::new("LE").unwrap();
    let be = CString::new("BE").unwrap();

    let u128_val: u128 = 0x0102030405060708090A0B0C0D0E0F10;
    let u128_handle = aot_store_value(RuntimeValue::U128(u128_val));

    let packed_u128_le =
        io_pack_number(
            type_u128.as_ptr() as u64,
            u128_handle,
            le.as_ptr() as u64,
        );
    assert_eq!(
        extract_bytes_from_arg(packed_u128_le),
        u128_val.to_le_bytes().to_vec()
    );

    let packed_u128_be =
        io_pack_number(
            type_u128.as_ptr() as u64,
            u128_handle,
            be.as_ptr() as u64,
        );
    assert_eq!(
        extract_bytes_from_arg(packed_u128_be),
        u128_val.to_be_bytes().to_vec()
    );

    // Negative i128
    let i128_val: i128 = -123456789012345678901234567890i128;
    let i128_handle = aot_store_value(RuntimeValue::I128(i128_val));

    let packed_i128_le =
        io_pack_number(
            type_i128.as_ptr() as u64,
            i128_handle,
            le.as_ptr() as u64,
        );
    assert_eq!(
        extract_bytes_from_arg(packed_i128_le),
        i128_val.to_le_bytes().to_vec()
    );

    let packed_i128_be =
        io_pack_number(
            type_i128.as_ptr() as u64,
            i128_handle,
            be.as_ptr() as u64,
        );
    assert_eq!(
        extract_bytes_from_arg(packed_i128_be),
        i128_val.to_be_bytes().to_vec()
    );
}

#[test]
fn test_io_pack_unpack_floats() {
    let type_f64 = CString::new("f64").unwrap();
    let le = CString::new("LE").unwrap();
    let be = CString::new("BE").unwrap();

    let pi = 3.141592653589793f64;
    let pi_handle = aot_store_value(RuntimeValue::F64(pi));

    let packed_le =
        io_pack_number(type_f64.as_ptr() as u64, pi_handle, le.as_ptr() as u64);
    assert_eq!(
        extract_bytes_from_arg(packed_le),
        pi.to_le_bytes().to_vec()
    );

    let packed_be =
        io_pack_number(type_f64.as_ptr() as u64, pi_handle, be.as_ptr() as u64);
    assert_eq!(
        extract_bytes_from_arg(packed_be),
        pi.to_be_bytes().to_vec()
    );

    // Unpack f64
    let unpacked_le_handle =
        io_unpack_number(type_f64.as_ptr() as u64, packed_le, le.as_ptr() as u64);
    let val_le = match aot_get_value(unpacked_le_handle) {
        Some(RuntimeValue::F64(f)) | Some(RuntimeValue::Float(f)) => f,
        other => panic!("expected Float/F64, got {:?}", other),
    };
    assert_eq!(val_le, pi);

    let unpacked_be_handle =
        io_unpack_number(type_f64.as_ptr() as u64, packed_be, be.as_ptr() as u64);
    let val_be = match aot_get_value(unpacked_be_handle) {
        Some(RuntimeValue::F64(f)) | Some(RuntimeValue::Float(f)) => f,
        other => panic!("expected Float/F64, got {:?}", other),
    };
    assert_eq!(val_be, pi);

    // Test Inf and NaN
    let inf_handle = aot_store_value(RuntimeValue::F64(f64::INFINITY));
    let packed_inf_be =
        io_pack_number(type_f64.as_ptr() as u64, inf_handle, be.as_ptr() as u64);
    assert_eq!(
        extract_bytes_from_arg(packed_inf_be),
        f64::INFINITY.to_be_bytes().to_vec()
    );

    let nan_handle = aot_store_value(RuntimeValue::F64(f64::NAN));
    let packed_nan_be =
        io_pack_number(type_f64.as_ptr() as u64, nan_handle, be.as_ptr() as u64);
    assert_eq!(
        extract_bytes_from_arg(packed_nan_be),
        f64::NAN.to_be_bytes().to_vec()
    );
}

#[test]
fn test_io_pack_invalid_endian_rejected() {
    let type_u32 = CString::new("u32").unwrap();
    let bad_endian = CString::new("middle_endian").unwrap();

    let handle =
        io_pack_number(
            type_u32.as_ptr() as u64,
            42,
            bad_endian.as_ptr() as u64,
        );
    // Must return empty byte array on invalid endianness
    assert!(extract_bytes_from_arg(handle).is_empty());
}
