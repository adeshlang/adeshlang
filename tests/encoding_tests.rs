//! Unit and Integration Tests for AdeshLang Encoding Standard Library.

use adeshlang::runtime::stdlib_src::encoding::base64::*;
use adeshlang::runtime::stdlib_src::encoding::binary::*;
use adeshlang::runtime::stdlib_src::encoding::bom::*;
use adeshlang::runtime::stdlib_src::encoding::hex::*;
use adeshlang::runtime::stdlib_src::encoding::percent::*;
use adeshlang::runtime::stdlib_src::encoding::utf::*;
use adeshlang::runtime::stdlib_src::encoding::varint::*;

#[test]
fn test_utf8_vectors() {
    let text = "Hello 🚀 नमस्ते संस्कृत 你好 こんにちは مرحبا";
    let bytes = utf8_encode(text);
    let decoded = utf8_decode(&bytes).unwrap();
    assert_eq!(decoded, text);
    assert!(is_valid_utf8(&bytes));
    assert!(validate_utf8(&bytes).is_ok());

    // Lossy decoding
    let lossy = utf8_decode_lossy(&bytes);
    assert_eq!(lossy, text);
}

#[test]
fn test_utf8_invalid_sequences() {
    let invalid = vec![0xFF, 0xFE, 0xFD];
    assert!(!is_valid_utf8(&invalid));
    assert!(utf8_decode(&invalid).is_err());

    let err = validate_utf8(&invalid).unwrap_err();
    assert_eq!(err.offset, 0);

    let lossy = utf8_decode_lossy(&invalid);
    assert!(lossy.contains('\u{FFFD}'));
}

#[test]
fn test_utf16_roundtrips() {
    let text = "AdeshLang 🚀 Unicode Test";

    let le = utf16_le_encode(text);
    assert_eq!(utf16_le_decode(&le).unwrap(), text);

    let be = utf16_be_encode(text);
    assert_eq!(utf16_be_decode(&be).unwrap(), text);

    assert_eq!(utf16_decode(&le).unwrap(), text);
}

#[test]
fn test_utf32_roundtrips() {
    let text = "AdeshLang 🚀 UTF-32 Test";

    let le = utf32_le_encode(text);
    assert_eq!(utf32_le_decode(&le).unwrap(), text);

    let be = utf32_be_encode(text);
    assert_eq!(utf32_be_decode(&be).unwrap(), text);

    assert_eq!(utf32_decode(&le).unwrap(), text);
}

#[test]
fn test_ascii_strict() {
    let ascii_text = "Hello World 123!";
    let bytes = ascii_encode(ascii_text).unwrap();
    assert_eq!(ascii_decode(&bytes).unwrap(), ascii_text);
    assert!(is_ascii(&bytes));

    let non_ascii = "Hello 🚀";
    assert!(ascii_encode(non_ascii).is_err());

    let non_ascii_bytes = vec![0x48, 0x65, 0xFF];
    assert!(ascii_decode(&non_ascii_bytes).is_err());
    assert!(!is_ascii(&non_ascii_bytes));
}

#[test]
fn test_base64_rfc4648_vectors() {
    let vectors: &[(&[u8], &str)] = &[
        (b"", ""),
        (b"f", "Zg=="),
        (b"fo", "Zm8="),
        (b"foo", "Zm9v"),
        (b"foob", "Zm9vYg=="),
        (b"fooba", "Zm9vYmE="),
        (b"foobar", "Zm9vYmFy"),
    ];

    for &(raw, expected) in vectors {
        let enc = base64_encode(raw);
        assert_eq!(enc, expected);
        let dec = base64_decode(expected).unwrap();
        assert_eq!(dec, raw);
    }
}

#[test]
fn test_base64_url_safe() {
    let data = vec![0xFB, 0xFF, 0xBF];
    let url_enc = base64_url_encode(&data);
    assert!(!url_enc.contains('+'));
    assert!(!url_enc.contains('/'));
    assert!(!url_enc.contains('='));

    let dec = base64_url_decode(&url_enc).unwrap();
    assert_eq!(dec, data);
}

#[test]
fn test_base64_streaming() {
    let mut encoder = Base64Encoder::new(false);
    encoder.write_chunk(b"foo");
    encoder.write_chunk(b"bar");
    let result = encoder.finish();
    assert_eq!(result, "Zm9vYmFy");

    let mut decoder = Base64Decoder::new(false);
    decoder.write_chunk("Zm9v");
    decoder.write_chunk("YmFy");
    let decoded = decoder.finish().unwrap();
    assert_eq!(decoded, b"foobar");
}

#[test]
fn test_hex_vectors() {
    let data = b"Hello";
    let hex_str = hex_encode(data);
    assert_eq!(hex_str, "48656c6c6f");
    assert_eq!(hex_encode_upper(data), "48656C6C6F");

    let decoded = hex_decode("48656c6c6f").unwrap();
    assert_eq!(decoded, data);

    assert!(hex_decode("48656c6c6").is_err()); // Odd length
    assert!(hex_decode("48656c6cGG").is_err()); // Invalid character
}

#[test]
fn test_hex_streaming() {
    let encoder = HexEncoder::new(false);
    assert_eq!(encoder.encode_chunk(b"Hello"), "48656c6c6f");

    let mut decoder = HexDecoder::new();
    decoder.write_chunk("48656c");
    decoder.write_chunk("6c6f");
    assert_eq!(decoder.finish().unwrap(), b"Hello");
}

#[test]
fn test_percent_and_form_encoding() {
    let original = "hello world & foo=bar?";
    let enc = percent_encode(original);
    assert_eq!(enc, "hello%20world%20%26%20foo%3Dbar%3F");
    assert_eq!(percent_decode(&enc).unwrap(), original);

    let form_enc = form_encode(original);
    assert_eq!(form_enc, "hello+world+%26+foo%3Dbar%3F");
    assert_eq!(form_decode(&form_enc).unwrap(), original);
}

#[test]
fn test_binary_integers_endianness() {
    let val: u32 = 0x12345678;
    let le = u32_to_bytes_le(val);
    assert_eq!(le, vec![0x78, 0x56, 0x34, 0x12]);
    assert_eq!(bytes_to_u32_le(&le).unwrap(), val);

    let be = u32_to_bytes_be(val);
    assert_eq!(be, vec![0x12, 0x34, 0x56, 0x78]);
    assert_eq!(bytes_to_u32_be(&be).unwrap(), val);
}

#[test]
fn test_binary_floats() {
    let val: f64 = std::f64::consts::PI;
    let le = f64_to_bytes_le(val);
    assert_eq!(bytes_to_f64_le(&le).unwrap(), val);

    let be = f64_to_bytes_be(val);
    assert_eq!(bytes_to_f64_be(&be).unwrap(), val);
}

#[test]
fn test_slice_bounds_checking() {
    let buf = vec![0x00, 0x01, 0x02, 0x03, 0x04];
    assert!(read_u32_le(&buf, 0).is_ok());
    assert!(read_u32_le(&buf, 2).is_err()); // Out of bounds
}

#[test]
fn test_binary_reader_writer() {
    let mut writer = BinaryWriter::new();
    writer.write_u16_be(0x1234);
    writer.write_u32_le(0x87654321);
    writer.write_f32_le(1.5f32);
    let bytes = writer.finish();

    let mut reader = BinaryReader::new(bytes);
    assert_eq!(reader.read_u16_be().unwrap(), 0x1234);
    assert_eq!(reader.read_u32_le().unwrap(), 0x87654321);
    assert_eq!(reader.read_f32_le().unwrap(), 1.5f32);
    assert_eq!(reader.remaining(), 0);
}

#[test]
fn test_varint_and_zigzag() {
    let val: u64 = 300;
    let enc = varint_encode(val);
    let (dec, read) = varint_decode(&enc).unwrap();
    assert_eq!(dec, val);
    assert_eq!(read, enc.len());

    let signed_val: i64 = -150;
    let signed_enc = signed_varint_encode(signed_val);
    let (signed_dec, _) = signed_varint_decode(&signed_enc).unwrap();
    assert_eq!(signed_dec, signed_val);
}

#[test]
fn test_leb128_wasm_vectors() {
    let u_val: u64 = 624485;
    let u_enc = uleb128_encode(u_val);
    assert_eq!(u_enc, vec![0xE5, 0x8E, 0x26]);
    let (u_dec, _) = uleb128_decode(&u_enc).unwrap();
    assert_eq!(u_dec, u_val);

    let s_val: i64 = -123456;
    let s_enc = sleb128_encode(s_val);
    let (s_dec, _) = sleb128_decode(&s_enc).unwrap();
    assert_eq!(s_dec, s_val);
}

#[test]
fn test_bom_detection_and_strip() {
    let utf8_bom = vec![0xEF, 0xBB, 0xBF, 0x48, 0x65, 0x6C, 0x6C, 0x6F];
    assert_eq!(detect_bom(&utf8_bom), BomKind::Utf8);

    let (bom_kind, clean) = remove_bom(&utf8_bom);
    assert_eq!(bom_kind, BomKind::Utf8);
    assert_eq!(clean, b"Hello");

    let re_added = add_bom(b"Hello", "UTF-8");
    assert_eq!(re_added, utf8_bom);
}
