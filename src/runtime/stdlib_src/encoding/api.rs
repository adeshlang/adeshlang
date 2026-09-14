//! Native Function Callbacks & Module Object Assembly for AdeshLang Encoding.

use crate::parsing::ast::{BuiltinEnv, NativeFn, Value};
use crate::runtime::stdlib_src::registry::BuiltinRegistry;
use crate::utils::collections::FastMap;
use std::sync::Arc;

use super::base64 as b64;
use super::binary;
use super::bom;
use super::hex;
use super::percent;
use super::utf;
use super::varint;

pub fn register_all(registry: &mut BuiltinRegistry) {
    // UTF
    registry.register(
        "Encoding.utf8Encode",
        "encoding",
        "Encode string to UTF-8 bytes",
        builtin_utf8_encode,
    );
    registry.register(
        "Encoding.utf8Decode",
        "encoding",
        "Decode UTF-8 bytes to string (strict)",
        builtin_utf8_decode,
    );
    registry.register(
        "Encoding.utf8DecodeLossy",
        "encoding",
        "Decode UTF-8 bytes to string (lossy)",
        builtin_utf8_decode_lossy,
    );
    registry.register(
        "Encoding.isValidUtf8",
        "encoding",
        "Check if byte slice is valid UTF-8",
        builtin_is_valid_utf8,
    );
    registry.register(
        "Encoding.validateUtf8",
        "encoding",
        "Validate UTF-8 with detailed error info",
        builtin_validate_utf8,
    );

    registry.register(
        "Encoding.utf16Encode",
        "encoding",
        "Encode string to UTF-16 bytes (LE)",
        builtin_utf16_encode,
    );
    registry.register(
        "Encoding.utf16Decode",
        "encoding",
        "Decode UTF-16 bytes to string",
        builtin_utf16_decode,
    );
    registry.register(
        "Encoding.utf16LEEncode",
        "encoding",
        "Encode string to UTF-16 LE bytes",
        builtin_utf16_le_encode,
    );
    registry.register(
        "Encoding.utf16LEDecode",
        "encoding",
        "Decode UTF-16 LE bytes to string",
        builtin_utf16_le_decode,
    );
    registry.register(
        "Encoding.utf16BEEncode",
        "encoding",
        "Encode string to UTF-16 BE bytes",
        builtin_utf16_be_encode,
    );
    registry.register(
        "Encoding.utf16BEDecode",
        "encoding",
        "Decode UTF-16 BE bytes to string",
        builtin_utf16_be_decode,
    );

    registry.register(
        "Encoding.utf32Encode",
        "encoding",
        "Encode string to UTF-32 bytes (LE)",
        builtin_utf32_encode,
    );
    registry.register(
        "Encoding.utf32Decode",
        "encoding",
        "Decode UTF-32 bytes to string",
        builtin_utf32_decode,
    );
    registry.register(
        "Encoding.utf32LEEncode",
        "encoding",
        "Encode string to UTF-32 LE bytes",
        builtin_utf32_le_encode,
    );
    registry.register(
        "Encoding.utf32LEDecode",
        "encoding",
        "Decode UTF-32 LE bytes to string",
        builtin_utf32_le_decode,
    );
    registry.register(
        "Encoding.utf32BEEncode",
        "encoding",
        "Encode string to UTF-32 BE bytes",
        builtin_utf32_be_encode,
    );
    registry.register(
        "Encoding.utf32BEDecode",
        "encoding",
        "Decode UTF-32 BE bytes to string",
        builtin_utf32_be_decode,
    );

    registry.register(
        "Encoding.asciiEncode",
        "encoding",
        "Encode string to ASCII bytes (strict)",
        builtin_ascii_encode,
    );
    registry.register(
        "Encoding.asciiDecode",
        "encoding",
        "Decode ASCII bytes to string (strict)",
        builtin_ascii_decode,
    );
    registry.register(
        "Encoding.isAscii",
        "encoding",
        "Check if string or bytes are ASCII",
        builtin_is_ascii,
    );

    // Base64
    registry.register(
        "Encoding.base64Encode",
        "encoding",
        "Encode bytes to Base64 string",
        builtin_base64_encode,
    );
    registry.register(
        "Encoding.base64Decode",
        "encoding",
        "Decode Base64 string to bytes",
        builtin_base64_decode,
    );
    registry.register(
        "Encoding.base64UrlEncode",
        "encoding",
        "Encode bytes to Base64URL string",
        builtin_base64_url_encode,
    );
    registry.register(
        "Encoding.base64UrlDecode",
        "encoding",
        "Decode Base64URL string to bytes",
        builtin_base64_url_decode,
    );

    // Hex
    registry.register(
        "Encoding.hexEncode",
        "encoding",
        "Encode bytes to lowercase hex string",
        builtin_hex_encode,
    );
    registry.register(
        "Encoding.hexEncodeUpper",
        "encoding",
        "Encode bytes to uppercase hex string",
        builtin_hex_encode_upper,
    );
    registry.register(
        "Encoding.hexDecode",
        "encoding",
        "Decode hex string to bytes",
        builtin_hex_decode,
    );

    // Percent & Form
    registry.register(
        "Encoding.percentEncode",
        "encoding",
        "Percent encode string (RFC 3986)",
        builtin_percent_encode,
    );
    registry.register(
        "Encoding.percentDecode",
        "encoding",
        "Percent decode string",
        builtin_percent_decode,
    );
    registry.register(
        "Encoding.urlEncodeComponent",
        "encoding",
        "URL encode component",
        builtin_url_encode_component,
    );
    registry.register(
        "Encoding.urlDecodeComponent",
        "encoding",
        "URL decode component",
        builtin_url_decode_component,
    );
    registry.register(
        "Encoding.formEncode",
        "encoding",
        "Form URL encode string",
        builtin_form_encode,
    );
    registry.register(
        "Encoding.formDecode",
        "encoding",
        "Form URL decode string",
        builtin_form_decode,
    );

    // Integers & Endianness
    registry.register(
        "Encoding.u16ToBytesLE",
        "encoding",
        "u16 to LE bytes",
        builtin_u16_to_bytes_le,
    );
    registry.register(
        "Encoding.u16ToBytesBE",
        "encoding",
        "u16 to BE bytes",
        builtin_u16_to_bytes_be,
    );
    registry.register(
        "Encoding.bytesToU16LE",
        "encoding",
        "LE bytes to u16",
        builtin_bytes_to_u16_le,
    );
    registry.register(
        "Encoding.bytesToU16BE",
        "encoding",
        "BE bytes to u16",
        builtin_bytes_to_u16_be,
    );

    registry.register(
        "Encoding.u32ToBytesLE",
        "encoding",
        "u32 to LE bytes",
        builtin_u32_to_bytes_le,
    );
    registry.register(
        "Encoding.u32ToBytesBE",
        "encoding",
        "u32 to BE bytes",
        builtin_u32_to_bytes_be,
    );
    registry.register(
        "Encoding.bytesToU32LE",
        "encoding",
        "LE bytes to u32",
        builtin_bytes_to_u32_le,
    );
    registry.register(
        "Encoding.bytesToU32BE",
        "encoding",
        "BE bytes to u32",
        builtin_bytes_to_u32_be,
    );

    registry.register(
        "Encoding.u64ToBytesLE",
        "encoding",
        "u64 to LE bytes",
        builtin_u64_to_bytes_le,
    );
    registry.register(
        "Encoding.u64ToBytesBE",
        "encoding",
        "u64 to BE bytes",
        builtin_u64_to_bytes_be,
    );
    registry.register(
        "Encoding.bytesToU64LE",
        "encoding",
        "LE bytes to u64",
        builtin_bytes_to_u64_le,
    );
    registry.register(
        "Encoding.bytesToU64BE",
        "encoding",
        "BE bytes to u64",
        builtin_bytes_to_u64_be,
    );

    registry.register(
        "Encoding.i16ToBytesLE",
        "encoding",
        "i16 to LE bytes",
        builtin_i16_to_bytes_le,
    );
    registry.register(
        "Encoding.i16ToBytesBE",
        "encoding",
        "i16 to BE bytes",
        builtin_i16_to_bytes_be,
    );
    registry.register(
        "Encoding.bytesToI16LE",
        "encoding",
        "LE bytes to i16",
        builtin_bytes_to_i16_le,
    );
    registry.register(
        "Encoding.bytesToI16BE",
        "encoding",
        "BE bytes to i16",
        builtin_bytes_to_i16_be,
    );

    registry.register(
        "Encoding.i32ToBytesLE",
        "encoding",
        "i32 to LE bytes",
        builtin_i32_to_bytes_le,
    );
    registry.register(
        "Encoding.i32ToBytesBE",
        "encoding",
        "i32 to BE bytes",
        builtin_i32_to_bytes_be,
    );
    registry.register(
        "Encoding.bytesToI32LE",
        "encoding",
        "LE bytes to i32",
        builtin_bytes_to_i32_le,
    );
    registry.register(
        "Encoding.bytesToI32BE",
        "encoding",
        "BE bytes to i32",
        builtin_bytes_to_i32_be,
    );

    registry.register(
        "Encoding.i64ToBytesLE",
        "encoding",
        "i64 to LE bytes",
        builtin_i64_to_bytes_le,
    );
    registry.register(
        "Encoding.i64ToBytesBE",
        "encoding",
        "i64 to BE bytes",
        builtin_i64_to_bytes_be,
    );
    registry.register(
        "Encoding.bytesToI64LE",
        "encoding",
        "LE bytes to i64",
        builtin_bytes_to_i64_le,
    );
    registry.register(
        "Encoding.bytesToI64BE",
        "encoding",
        "BE bytes to i64",
        builtin_bytes_to_i64_be,
    );

    // Floats
    registry.register(
        "Encoding.f32ToBytesLE",
        "encoding",
        "f32 to LE bytes",
        builtin_f32_to_bytes_le,
    );
    registry.register(
        "Encoding.f32ToBytesBE",
        "encoding",
        "f32 to BE bytes",
        builtin_f32_to_bytes_be,
    );
    registry.register(
        "Encoding.bytesToF32LE",
        "encoding",
        "LE bytes to f32",
        builtin_bytes_to_f32_le,
    );
    registry.register(
        "Encoding.bytesToF32BE",
        "encoding",
        "BE bytes to f32",
        builtin_bytes_to_f32_be,
    );

    registry.register(
        "Encoding.f64ToBytesLE",
        "encoding",
        "f64 to LE bytes",
        builtin_f64_to_bytes_le,
    );
    registry.register(
        "Encoding.f64ToBytesBE",
        "encoding",
        "f64 to BE bytes",
        builtin_f64_to_bytes_be,
    );
    registry.register(
        "Encoding.bytesToF64LE",
        "encoding",
        "LE bytes to f64",
        builtin_bytes_to_f64_le,
    );
    registry.register(
        "Encoding.bytesToF64BE",
        "encoding",
        "BE bytes to f64",
        builtin_bytes_to_f64_be,
    );

    // Slice read/write
    registry.register(
        "Encoding.readU16LE",
        "encoding",
        "Read u16 LE from buffer at offset",
        builtin_read_u16_le,
    );
    registry.register(
        "Encoding.readU16BE",
        "encoding",
        "Read u16 BE from buffer at offset",
        builtin_read_u16_be,
    );
    registry.register(
        "Encoding.readU32LE",
        "encoding",
        "Read u32 LE from buffer at offset",
        builtin_read_u32_le,
    );
    registry.register(
        "Encoding.readU32BE",
        "encoding",
        "Read u32 BE from buffer at offset",
        builtin_read_u32_be,
    );
    registry.register(
        "Encoding.readU64LE",
        "encoding",
        "Read u64 LE from buffer at offset",
        builtin_read_u64_le,
    );
    registry.register(
        "Encoding.readU64BE",
        "encoding",
        "Read u64 BE from buffer at offset",
        builtin_read_u64_be,
    );

    // VarInt & LEB128
    registry.register(
        "Encoding.varIntEncode",
        "encoding",
        "Encode unsigned u64 to VarInt bytes",
        builtin_varint_encode,
    );
    registry.register(
        "Encoding.varIntDecode",
        "encoding",
        "Decode unsigned VarInt bytes to u64",
        builtin_varint_decode,
    );
    registry.register(
        "Encoding.zigzagEncode",
        "encoding",
        "ZigZag encode i64 to u64",
        builtin_zigzag_encode,
    );
    registry.register(
        "Encoding.zigzagDecode",
        "encoding",
        "ZigZag decode u64 to i64",
        builtin_zigzag_decode,
    );
    registry.register(
        "Encoding.signedVarIntEncode",
        "encoding",
        "Signed VarInt encode",
        builtin_signed_varint_encode,
    );
    registry.register(
        "Encoding.signedVarIntDecode",
        "encoding",
        "Signed VarInt decode",
        builtin_signed_varint_decode,
    );

    registry.register(
        "Encoding.uleb128Encode",
        "encoding",
        "Encode ULEB128",
        builtin_uleb128_encode,
    );
    registry.register(
        "Encoding.uleb128Decode",
        "encoding",
        "Decode ULEB128",
        builtin_uleb128_decode,
    );
    registry.register(
        "Encoding.sleb128Encode",
        "encoding",
        "Encode SLEB128",
        builtin_sleb128_encode,
    );
    registry.register(
        "Encoding.sleb128Decode",
        "encoding",
        "Decode SLEB128",
        builtin_sleb128_decode,
    );

    // BOM
    registry.register(
        "Encoding.detectBom",
        "encoding",
        "Detect Byte Order Mark in byte slice",
        builtin_detect_bom,
    );
    registry.register(
        "Encoding.removeBom",
        "encoding",
        "Strip Byte Order Mark from byte slice",
        builtin_remove_bom,
    );
    registry.register(
        "Encoding.addBom",
        "encoding",
        "Prepend Byte Order Mark to byte slice",
        builtin_add_bom,
    );

    // Helper constructor wrappers
    registry.register(
        "Encoding.createBinaryReader",
        "encoding",
        "Create BinaryReader object",
        builtin_create_binary_reader,
    );
    registry.register(
        "Encoding.createBinaryWriter",
        "encoding",
        "Create BinaryWriter object",
        builtin_create_binary_writer,
    );
}

// ----------------------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------------------

fn extract_byte(elem: &Value) -> Option<u8> {
    match elem {
        Value::Number(f) => Some(*f as u8),
        Value::U8(n) => Some(*n),
        Value::U16(n) => Some(*n as u8),
        Value::U32(n) => Some(*n as u8),
        Value::U64(n) => Some(*n as u8),
        Value::U128(n) => Some(*n as u8),
        Value::I8(n) => Some(*n as u8),
        Value::I16(n) => Some(*n as u8),
        Value::I32(n) => Some(*n as u8),
        Value::I64(n) => Some(*n as u8),
        Value::I128(n) => Some(*n as u8),
        Value::F32(n) => Some(*n as u8),
        Value::F64(n) => Some(*n as u8),
        _ => None,
    }
}

fn value_to_bytes(val: &Value) -> Result<Vec<u8>, String> {
    match val {
        Value::Str(s) => Ok(s.as_bytes().to_vec()),
        Value::Array(arr) => {
            let mut bytes = Vec::with_capacity(arr.len());
            for elem in arr.iter() {
                if let Some(b) = extract_byte(elem) {
                    bytes.push(b);
                } else {
                    return Err("Expected array of byte numbers".to_string());
                }
            }
            Ok(bytes)
        }
        Value::RawArray(_, arr) => {
            let mut bytes = Vec::with_capacity(arr.len());
            for elem in arr.iter() {
                if let Some(b) = extract_byte(elem) {
                    bytes.push(b);
                } else {
                    return Err("Expected raw array of byte numbers".to_string());
                }
            }
            Ok(bytes)
        }
        Value::DynArray(da) => {
            let mut bytes = Vec::with_capacity(da.data.len());
            for elem in da.data.iter() {
                if let Some(b) = extract_byte(elem) {
                    bytes.push(b);
                } else {
                    return Err("Expected dynamic array of byte numbers".to_string());
                }
            }
            Ok(bytes)
        }
        Value::Tuple(tup) => {
            let mut bytes = Vec::with_capacity(tup.len());
            for elem in tup.iter() {
                if let Some(b) = extract_byte(elem) {
                    bytes.push(b);
                } else {
                    return Err("Expected tuple of byte numbers".to_string());
                }
            }
            Ok(bytes)
        }
        _ => Err(format!("Expected String or Byte Array, got {:?}", val)),
    }
}

fn bytes_to_value(bytes: &[u8]) -> Value {
    Value::Array(bytes.iter().map(|&b| Value::Number(b as f64)).collect())
}

fn get_str(val: &Value) -> Option<&str> {
    match val {
        Value::Str(s) => Some(s.as_str()),
        _ => None,
    }
}

fn get_num(val: &Value) -> Option<f64> {
    match val {
        Value::Number(f) => Some(*f),
        Value::U8(n) => Some(*n as f64),
        Value::U16(n) => Some(*n as f64),
        Value::U32(n) => Some(*n as f64),
        Value::U64(n) => Some(*n as f64),
        Value::I8(n) => Some(*n as f64),
        Value::I16(n) => Some(*n as f64),
        Value::I32(n) => Some(*n as f64),
        Value::I64(n) => Some(*n as f64),
        Value::F32(n) => Some(*n as f64),
        Value::F64(n) => Some(*n),
        _ => None,
    }
}

// ----------------------------------------------------------------------------
// Builtin Callbacks
// ----------------------------------------------------------------------------

fn builtin_utf8_encode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let s = args
        .get(0)
        .and_then(get_str)
        .ok_or("utf8Encode requires (string)")?;
    Ok(bytes_to_value(&utf::utf8_encode(s)))
}

fn builtin_utf8_decode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("utf8Decode requires (bytes)")?)?;
    let s = utf::utf8_decode(&bytes)?;
    Ok(Value::Str(s))
}

fn builtin_utf8_decode_lossy(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("utf8DecodeLossy requires (bytes)")?)?;
    Ok(Value::Str(utf::utf8_decode_lossy(&bytes)))
}

fn builtin_is_valid_utf8(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("isValidUtf8 requires (bytes)")?)?;
    Ok(Value::Bool(utf::is_valid_utf8(&bytes)))
}

fn builtin_validate_utf8(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("validateUtf8 requires (bytes)")?)?;
    match utf::validate_utf8(&bytes) {
        Ok(()) => Ok(Value::Null),
        Err(err) => Err(err.message),
    }
}

fn builtin_utf16_encode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let s = args
        .get(0)
        .and_then(get_str)
        .ok_or("utf16Encode requires (string)")?;
    Ok(bytes_to_value(&utf::utf16_le_encode(s)))
}

fn builtin_utf16_decode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("utf16Decode requires (bytes)")?)?;
    Ok(Value::Str(utf::utf16_decode(&bytes)?))
}

fn builtin_utf16_le_encode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let s = args
        .get(0)
        .and_then(get_str)
        .ok_or("utf16LEEncode requires (string)")?;
    Ok(bytes_to_value(&utf::utf16_le_encode(s)))
}

fn builtin_utf16_le_decode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("utf16LEDecode requires (bytes)")?)?;
    Ok(Value::Str(utf::utf16_le_decode(&bytes)?))
}

fn builtin_utf16_be_encode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let s = args
        .get(0)
        .and_then(get_str)
        .ok_or("utf16BEEncode requires (string)")?;
    Ok(bytes_to_value(&utf::utf16_be_encode(s)))
}

fn builtin_utf16_be_decode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("utf16BEDecode requires (bytes)")?)?;
    Ok(Value::Str(utf::utf16_be_decode(&bytes)?))
}

fn builtin_utf32_encode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let s = args
        .get(0)
        .and_then(get_str)
        .ok_or("utf32Encode requires (string)")?;
    Ok(bytes_to_value(&utf::utf32_le_encode(s)))
}

fn builtin_utf32_decode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("utf32Decode requires (bytes)")?)?;
    Ok(Value::Str(utf::utf32_decode(&bytes)?))
}

fn builtin_utf32_le_encode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let s = args
        .get(0)
        .and_then(get_str)
        .ok_or("utf32LEEncode requires (string)")?;
    Ok(bytes_to_value(&utf::utf32_le_encode(s)))
}

fn builtin_utf32_le_decode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("utf32LEDecode requires (bytes)")?)?;
    Ok(Value::Str(utf::utf32_le_decode(&bytes)?))
}

fn builtin_utf32_be_encode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let s = args
        .get(0)
        .and_then(get_str)
        .ok_or("utf32BEEncode requires (string)")?;
    Ok(bytes_to_value(&utf::utf32_be_encode(s)))
}

fn builtin_utf32_be_decode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("utf32BEDecode requires (bytes)")?)?;
    Ok(Value::Str(utf::utf32_be_decode(&bytes)?))
}

fn builtin_ascii_encode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let s = args
        .get(0)
        .and_then(get_str)
        .ok_or("asciiEncode requires (string)")?;
    let bytes = utf::ascii_encode(s)?;
    Ok(bytes_to_value(&bytes))
}

fn builtin_ascii_decode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("asciiDecode requires (bytes)")?)?;
    let s = utf::ascii_decode(&bytes)?;
    Ok(Value::Str(s))
}

fn builtin_is_ascii(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let val = args.get(0).ok_or("isAscii requires (data)")?;
    if let Some(s) = get_str(val) {
        Ok(Value::Bool(utf::is_ascii(s.as_bytes())))
    } else {
        let bytes = value_to_bytes(val)?;
        Ok(Value::Bool(utf::is_ascii(&bytes)))
    }
}

// Base64
fn builtin_base64_encode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("base64Encode requires (bytes)")?)?;
    Ok(Value::Str(b64::base64_encode(&bytes)))
}

fn builtin_base64_decode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let s = args
        .get(0)
        .and_then(get_str)
        .ok_or("base64Decode requires (b64String)")?;
    let bytes = b64::base64_decode(s)?;
    Ok(bytes_to_value(&bytes))
}

fn builtin_base64_url_encode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("base64UrlEncode requires (bytes)")?)?;
    Ok(Value::Str(b64::base64_url_encode(&bytes)))
}

fn builtin_base64_url_decode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let s = args
        .get(0)
        .and_then(get_str)
        .ok_or("base64UrlDecode requires (b64UrlString)")?;
    let bytes = b64::base64_url_decode(s)?;
    Ok(bytes_to_value(&bytes))
}

// Hex
fn builtin_hex_encode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("hexEncode requires (bytes)")?)?;
    Ok(Value::Str(hex::hex_encode(&bytes)))
}

fn builtin_hex_encode_upper(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("hexEncodeUpper requires (bytes)")?)?;
    Ok(Value::Str(hex::hex_encode_upper(&bytes)))
}

fn builtin_hex_decode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let s = args
        .get(0)
        .and_then(get_str)
        .ok_or("hexDecode requires (hexString)")?;
    let bytes = hex::hex_decode(s)?;
    Ok(bytes_to_value(&bytes))
}

// Percent
fn builtin_percent_encode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let s = args
        .get(0)
        .and_then(get_str)
        .ok_or("percentEncode requires (string)")?;
    Ok(Value::Str(percent::percent_encode(s)))
}

fn builtin_percent_decode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let s = args
        .get(0)
        .and_then(get_str)
        .ok_or("percentDecode requires (string)")?;
    Ok(Value::Str(percent::percent_decode(s)?))
}

fn builtin_url_encode_component(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let s = args
        .get(0)
        .and_then(get_str)
        .ok_or("urlEncodeComponent requires (string)")?;
    Ok(Value::Str(percent::url_encode_component(s)))
}

fn builtin_url_decode_component(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let s = args
        .get(0)
        .and_then(get_str)
        .ok_or("urlDecodeComponent requires (string)")?;
    Ok(Value::Str(percent::url_decode_component(s)?))
}

fn builtin_form_encode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let s = args
        .get(0)
        .and_then(get_str)
        .ok_or("formEncode requires (string)")?;
    Ok(Value::Str(percent::form_encode(s)))
}

fn builtin_form_decode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let s = args
        .get(0)
        .and_then(get_str)
        .ok_or("formDecode requires (string)")?;
    Ok(Value::Str(percent::form_decode(s)?))
}

// Integer Encodings
fn builtin_u16_to_bytes_le(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let num =
        get_num(args.get(0).ok_or("u16ToBytesLE requires (num)")?).ok_or("Expected number")? as u16;
    Ok(bytes_to_value(&binary::u16_to_bytes_le(num)))
}
fn builtin_u16_to_bytes_be(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let num =
        get_num(args.get(0).ok_or("u16ToBytesBE requires (num)")?).ok_or("Expected number")? as u16;
    Ok(bytes_to_value(&binary::u16_to_bytes_be(num)))
}
fn builtin_bytes_to_u16_le(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("bytesToU16LE requires (bytes)")?)?;
    Ok(Value::Number(binary::bytes_to_u16_le(&bytes)? as f64))
}
fn builtin_bytes_to_u16_be(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("bytesToU16BE requires (bytes)")?)?;
    Ok(Value::Number(binary::bytes_to_u16_be(&bytes)? as f64))
}

fn builtin_u32_to_bytes_le(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let num =
        get_num(args.get(0).ok_or("u32ToBytesLE requires (num)")?).ok_or("Expected number")? as u32;
    Ok(bytes_to_value(&binary::u32_to_bytes_le(num)))
}
fn builtin_u32_to_bytes_be(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let num =
        get_num(args.get(0).ok_or("u32ToBytesBE requires (num)")?).ok_or("Expected number")? as u32;
    Ok(bytes_to_value(&binary::u32_to_bytes_be(num)))
}
fn builtin_bytes_to_u32_le(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("bytesToU32LE requires (bytes)")?)?;
    Ok(Value::Number(binary::bytes_to_u32_le(&bytes)? as f64))
}
fn builtin_bytes_to_u32_be(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("bytesToU32BE requires (bytes)")?)?;
    Ok(Value::Number(binary::bytes_to_u32_be(&bytes)? as f64))
}

fn builtin_u64_to_bytes_le(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let num =
        get_num(args.get(0).ok_or("u64ToBytesLE requires (num)")?).ok_or("Expected number")? as u64;
    Ok(bytes_to_value(&binary::u64_to_bytes_le(num)))
}
fn builtin_u64_to_bytes_be(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let num =
        get_num(args.get(0).ok_or("u64ToBytesBE requires (num)")?).ok_or("Expected number")? as u64;
    Ok(bytes_to_value(&binary::u64_to_bytes_be(num)))
}
fn builtin_bytes_to_u64_le(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("bytesToU64LE requires (bytes)")?)?;
    Ok(Value::Number(binary::bytes_to_u64_le(&bytes)? as f64))
}
fn builtin_bytes_to_u64_be(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("bytesToU64BE requires (bytes)")?)?;
    Ok(Value::Number(binary::bytes_to_u64_be(&bytes)? as f64))
}

fn builtin_i16_to_bytes_le(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let num =
        get_num(args.get(0).ok_or("i16ToBytesLE requires (num)")?).ok_or("Expected number")? as i16;
    Ok(bytes_to_value(&binary::i16_to_bytes_le(num)))
}
fn builtin_i16_to_bytes_be(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let num =
        get_num(args.get(0).ok_or("i16ToBytesBE requires (num)")?).ok_or("Expected number")? as i16;
    Ok(bytes_to_value(&binary::i16_to_bytes_be(num)))
}
fn builtin_bytes_to_i16_le(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("bytesToI16LE requires (bytes)")?)?;
    Ok(Value::Number(binary::bytes_to_i16_le(&bytes)? as f64))
}
fn builtin_bytes_to_i16_be(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("bytesToI16BE requires (bytes)")?)?;
    Ok(Value::Number(binary::bytes_to_i16_be(&bytes)? as f64))
}

fn builtin_i32_to_bytes_le(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let num =
        get_num(args.get(0).ok_or("i32ToBytesLE requires (num)")?).ok_or("Expected number")? as i32;
    Ok(bytes_to_value(&binary::i32_to_bytes_le(num)))
}
fn builtin_i32_to_bytes_be(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let num =
        get_num(args.get(0).ok_or("i32ToBytesBE requires (num)")?).ok_or("Expected number")? as i32;
    Ok(bytes_to_value(&binary::i32_to_bytes_be(num)))
}
fn builtin_bytes_to_i32_le(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("bytesToI32LE requires (bytes)")?)?;
    Ok(Value::Number(binary::bytes_to_i32_le(&bytes)? as f64))
}
fn builtin_bytes_to_i32_be(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("bytesToI32BE requires (bytes)")?)?;
    Ok(Value::Number(binary::bytes_to_i32_be(&bytes)? as f64))
}

fn builtin_i64_to_bytes_le(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let num =
        get_num(args.get(0).ok_or("i64ToBytesLE requires (num)")?).ok_or("Expected number")? as i64;
    Ok(bytes_to_value(&binary::i64_to_bytes_le(num)))
}
fn builtin_i64_to_bytes_be(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let num =
        get_num(args.get(0).ok_or("i64ToBytesBE requires (num)")?).ok_or("Expected number")? as i64;
    Ok(bytes_to_value(&binary::i64_to_bytes_be(num)))
}
fn builtin_bytes_to_i64_le(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("bytesToI64LE requires (bytes)")?)?;
    Ok(Value::Number(binary::bytes_to_i64_le(&bytes)? as f64))
}
fn builtin_bytes_to_i64_be(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("bytesToI64BE requires (bytes)")?)?;
    Ok(Value::Number(binary::bytes_to_i64_be(&bytes)? as f64))
}

// Floats
fn builtin_f32_to_bytes_le(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let num =
        get_num(args.get(0).ok_or("f32ToBytesLE requires (num)")?).ok_or("Expected number")? as f32;
    Ok(bytes_to_value(&binary::f32_to_bytes_le(num)))
}
fn builtin_f32_to_bytes_be(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let num =
        get_num(args.get(0).ok_or("f32ToBytesBE requires (num)")?).ok_or("Expected number")? as f32;
    Ok(bytes_to_value(&binary::f32_to_bytes_be(num)))
}
fn builtin_bytes_to_f32_le(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("bytesToF32LE requires (bytes)")?)?;
    Ok(Value::Number(binary::bytes_to_f32_le(&bytes)? as f64))
}
fn builtin_bytes_to_f32_be(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("bytesToF32BE requires (bytes)")?)?;
    Ok(Value::Number(binary::bytes_to_f32_be(&bytes)? as f64))
}

fn builtin_f64_to_bytes_le(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let num =
        get_num(args.get(0).ok_or("f64ToBytesLE requires (num)")?).ok_or("Expected number")?;
    Ok(bytes_to_value(&binary::f64_to_bytes_le(num)))
}
fn builtin_f64_to_bytes_be(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let num =
        get_num(args.get(0).ok_or("f64ToBytesBE requires (num)")?).ok_or("Expected number")?;
    Ok(bytes_to_value(&binary::f64_to_bytes_be(num)))
}
fn builtin_bytes_to_f64_le(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("bytesToF64LE requires (bytes)")?)?;
    Ok(Value::Number(binary::bytes_to_f64_le(&bytes)?))
}
fn builtin_bytes_to_f64_be(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("bytesToF64BE requires (bytes)")?)?;
    Ok(Value::Number(binary::bytes_to_f64_be(&bytes)?))
}

// Slice Read/Write
fn builtin_read_u16_le(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("readU16LE requires (buffer, offset)")?)?;
    let offset = get_num(args.get(1).ok_or("readU16LE requires offset")?)
        .ok_or("Expected number offset")? as usize;
    Ok(Value::Number(binary::read_u16_le(&bytes, offset)? as f64))
}
fn builtin_read_u16_be(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("readU16BE requires (buffer, offset)")?)?;
    let offset = get_num(args.get(1).ok_or("readU16BE requires offset")?)
        .ok_or("Expected number offset")? as usize;
    Ok(Value::Number(binary::read_u16_be(&bytes, offset)? as f64))
}
fn builtin_read_u32_le(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("readU32LE requires (buffer, offset)")?)?;
    let offset = get_num(args.get(1).ok_or("readU32LE requires offset")?)
        .ok_or("Expected number offset")? as usize;
    Ok(Value::Number(binary::read_u32_le(&bytes, offset)? as f64))
}
fn builtin_read_u32_be(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("readU32BE requires (buffer, offset)")?)?;
    let offset = get_num(args.get(1).ok_or("readU32BE requires offset")?)
        .ok_or("Expected number offset")? as usize;
    Ok(Value::Number(binary::read_u32_be(&bytes, offset)? as f64))
}
fn builtin_read_u64_le(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("readU64LE requires (buffer, offset)")?)?;
    let offset = get_num(args.get(1).ok_or("readU64LE requires offset")?)
        .ok_or("Expected number offset")? as usize;
    Ok(Value::Number(binary::read_u64_le(&bytes, offset)? as f64))
}
fn builtin_read_u64_be(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("readU64BE requires (buffer, offset)")?)?;
    let offset = get_num(args.get(1).ok_or("readU64BE requires offset")?)
        .ok_or("Expected number offset")? as usize;
    Ok(Value::Number(binary::read_u64_be(&bytes, offset)? as f64))
}

// VarInt & LEB128
fn builtin_varint_encode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let num = get_num(args.get(0).ok_or("varIntEncode requires (number)")?)
        .ok_or("Expected number")? as u64;
    Ok(bytes_to_value(&varint::varint_encode(num)))
}
fn builtin_varint_decode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("varIntDecode requires (bytes)")?)?;
    let (val, _read) = varint::varint_decode(&bytes)?;
    Ok(Value::Number(val as f64))
}

fn builtin_zigzag_encode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let num = get_num(args.get(0).ok_or("zigzagEncode requires (number)")?)
        .ok_or("Expected number")? as i64;
    Ok(Value::Number(varint::zigzag_encode(num) as f64))
}
fn builtin_zigzag_decode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let num = get_num(args.get(0).ok_or("zigzagDecode requires (number)")?)
        .ok_or("Expected number")? as u64;
    Ok(Value::Number(varint::zigzag_decode(num) as f64))
}

fn builtin_signed_varint_encode(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let num = get_num(args.get(0).ok_or("signedVarIntEncode requires (number)")?)
        .ok_or("Expected number")? as i64;
    Ok(bytes_to_value(&varint::signed_varint_encode(num)))
}
fn builtin_signed_varint_decode(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("signedVarIntDecode requires (bytes)")?)?;
    let (val, _read) = varint::signed_varint_decode(&bytes)?;
    Ok(Value::Number(val as f64))
}

fn builtin_uleb128_encode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let num = get_num(args.get(0).ok_or("uleb128Encode requires (number)")?)
        .ok_or("Expected number")? as u64;
    Ok(bytes_to_value(&varint::uleb128_encode(num)))
}
fn builtin_uleb128_decode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("uleb128Decode requires (bytes)")?)?;
    let (val, _read) = varint::uleb128_decode(&bytes)?;
    Ok(Value::Number(val as f64))
}

fn builtin_sleb128_encode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let num = get_num(args.get(0).ok_or("sleb128Encode requires (number)")?)
        .ok_or("Expected number")? as i64;
    Ok(bytes_to_value(&varint::sleb128_encode(num)))
}
fn builtin_sleb128_decode(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("sleb128Decode requires (bytes)")?)?;
    let (val, _read) = varint::sleb128_decode(&bytes)?;
    Ok(Value::Number(val as f64))
}

// BOM
fn builtin_detect_bom(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("detectBom requires (bytes)")?)?;
    Ok(Value::Str(bom::detect_bom(&bytes).name().to_string()))
}

fn builtin_remove_bom(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("removeBom requires (bytes)")?)?;
    let (bom_kind, clean_bytes) = bom::remove_bom(&bytes);
    let mut map = FastMap::default();
    map.insert("bom".to_string(), Value::Str(bom_kind.name().to_string()));
    map.insert("bytes".to_string(), bytes_to_value(clean_bytes));
    Ok(Value::Object(Arc::new(map)))
}

fn builtin_add_bom(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("addBom requires (bytes, encodingName)")?)?;
    let enc = args.get(1).and_then(get_str).unwrap_or("UTF-8");
    Ok(bytes_to_value(&bom::add_bom(&bytes, enc)))
}

// Object Wrappers for Readers and Writers
fn builtin_create_binary_writer(
    _env: &mut dyn BuiltinEnv,
    _args: Vec<Value>,
) -> Result<Value, String> {
    let _writer = binary::BinaryWriter::new();
    let mut map = FastMap::default();

    // We store a stateful writer in native closures or return a simple builder map
    map.insert("writeU8".to_string(), Value::Str("writeU8".to_string()));
    Ok(Value::Object(Arc::new(map)))
}

fn builtin_create_binary_reader(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let bytes = value_to_bytes(args.get(0).ok_or("createBinaryReader requires (bytes)")?)?;
    let reader = binary::BinaryReader::new(bytes);
    let mut map = FastMap::default();
    map.insert(
        "remaining".to_string(),
        Value::Number(reader.remaining() as f64),
    );
    Ok(Value::Object(Arc::new(map)))
}

// ----------------------------------------------------------------------------
// Module Object Constructor for `import Encoding;`
// ----------------------------------------------------------------------------

pub fn build_encoding_module_object() -> Value {
    let mut map = FastMap::default();

    map.insert(
        "utf8Encode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_utf8_encode))),
    );
    map.insert(
        "utf8Decode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_utf8_decode))),
    );
    map.insert(
        "utf8DecodeLossy".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_utf8_decode_lossy))),
    );
    map.insert(
        "isValidUtf8".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_is_valid_utf8))),
    );
    map.insert(
        "validateUtf8".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_validate_utf8))),
    );

    map.insert(
        "utf16Encode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_utf16_encode))),
    );
    map.insert(
        "utf16Decode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_utf16_decode))),
    );
    map.insert(
        "utf16LEEncode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_utf16_le_encode))),
    );
    map.insert(
        "utf16LEDecode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_utf16_le_decode))),
    );
    map.insert(
        "utf16BEEncode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_utf16_be_encode))),
    );
    map.insert(
        "utf16BEDecode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_utf16_be_decode))),
    );

    map.insert(
        "utf32Encode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_utf32_encode))),
    );
    map.insert(
        "utf32Decode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_utf32_decode))),
    );
    map.insert(
        "utf32LEEncode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_utf32_le_encode))),
    );
    map.insert(
        "utf32LEDecode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_utf32_le_decode))),
    );
    map.insert(
        "utf32BEEncode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_utf32_be_encode))),
    );
    map.insert(
        "utf32BEDecode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_utf32_be_decode))),
    );

    map.insert(
        "asciiEncode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_ascii_encode))),
    );
    map.insert(
        "asciiDecode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_ascii_decode))),
    );
    map.insert(
        "isAscii".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_is_ascii))),
    );

    map.insert(
        "base64Encode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_base64_encode))),
    );
    map.insert(
        "base64Decode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_base64_decode))),
    );
    map.insert(
        "base64UrlEncode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_base64_url_encode))),
    );
    map.insert(
        "base64UrlDecode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_base64_url_decode))),
    );

    map.insert(
        "hexEncode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_hex_encode))),
    );
    map.insert(
        "hexEncodeUpper".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_hex_encode_upper))),
    );
    map.insert(
        "hexDecode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_hex_decode))),
    );

    map.insert(
        "percentEncode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_percent_encode))),
    );
    map.insert(
        "percentDecode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_percent_decode))),
    );
    map.insert(
        "urlEncodeComponent".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_encode_component))),
    );
    map.insert(
        "urlDecodeComponent".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_decode_component))),
    );
    map.insert(
        "formEncode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_form_encode))),
    );
    map.insert(
        "formDecode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_form_decode))),
    );

    map.insert(
        "u16ToBytesLE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_u16_to_bytes_le))),
    );
    map.insert(
        "u16ToBytesBE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_u16_to_bytes_be))),
    );
    map.insert(
        "bytesToU16LE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_bytes_to_u16_le))),
    );
    map.insert(
        "bytesToU16BE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_bytes_to_u16_be))),
    );

    map.insert(
        "u32ToBytesLE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_u32_to_bytes_le))),
    );
    map.insert(
        "u32ToBytesBE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_u32_to_bytes_be))),
    );
    map.insert(
        "bytesToU32LE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_bytes_to_u32_le))),
    );
    map.insert(
        "bytesToU32BE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_bytes_to_u32_be))),
    );

    map.insert(
        "u64ToBytesLE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_u64_to_bytes_le))),
    );
    map.insert(
        "u64ToBytesBE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_u64_to_bytes_be))),
    );
    map.insert(
        "bytesToU64LE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_bytes_to_u64_le))),
    );
    map.insert(
        "bytesToU64BE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_bytes_to_u64_be))),
    );

    map.insert(
        "i16ToBytesLE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_i16_to_bytes_le))),
    );
    map.insert(
        "i16ToBytesBE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_i16_to_bytes_be))),
    );
    map.insert(
        "bytesToI16LE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_bytes_to_i16_le))),
    );
    map.insert(
        "bytesToI16BE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_bytes_to_i16_be))),
    );

    map.insert(
        "i32ToBytesLE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_i32_to_bytes_le))),
    );
    map.insert(
        "i32ToBytesBE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_i32_to_bytes_be))),
    );
    map.insert(
        "bytesToI32LE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_bytes_to_i32_le))),
    );
    map.insert(
        "bytesToI32BE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_bytes_to_i32_be))),
    );

    map.insert(
        "i64ToBytesLE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_i64_to_bytes_le))),
    );
    map.insert(
        "i64ToBytesBE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_i64_to_bytes_be))),
    );
    map.insert(
        "bytesToI64LE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_bytes_to_i64_le))),
    );
    map.insert(
        "bytesToI64BE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_bytes_to_i64_be))),
    );

    map.insert(
        "f32ToBytesLE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_f32_to_bytes_le))),
    );
    map.insert(
        "f32ToBytesBE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_f32_to_bytes_be))),
    );
    map.insert(
        "bytesToF32LE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_bytes_to_f32_le))),
    );
    map.insert(
        "bytesToF32BE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_bytes_to_f32_be))),
    );

    map.insert(
        "f64ToBytesLE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_f64_to_bytes_le))),
    );
    map.insert(
        "f64ToBytesBE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_f64_to_bytes_be))),
    );
    map.insert(
        "bytesToF64LE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_bytes_to_f64_le))),
    );
    map.insert(
        "bytesToF64BE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_bytes_to_f64_be))),
    );

    map.insert(
        "readU16LE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_read_u16_le))),
    );
    map.insert(
        "readU16BE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_read_u16_be))),
    );
    map.insert(
        "readU32LE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_read_u32_le))),
    );
    map.insert(
        "readU32BE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_read_u32_be))),
    );
    map.insert(
        "readU64LE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_read_u64_le))),
    );
    map.insert(
        "readU64BE".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_read_u64_be))),
    );

    map.insert(
        "varIntEncode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_varint_encode))),
    );
    map.insert(
        "varIntDecode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_varint_decode))),
    );
    map.insert(
        "zigzagEncode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_zigzag_encode))),
    );
    map.insert(
        "zigzagDecode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_zigzag_decode))),
    );
    map.insert(
        "signedVarIntEncode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_signed_varint_encode))),
    );
    map.insert(
        "signedVarIntDecode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_signed_varint_decode))),
    );

    map.insert(
        "uleb128Encode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_uleb128_encode))),
    );
    map.insert(
        "uleb128Decode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_uleb128_decode))),
    );
    map.insert(
        "sleb128Encode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_sleb128_encode))),
    );
    map.insert(
        "sleb128Decode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_sleb128_decode))),
    );

    map.insert(
        "detectBom".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_detect_bom))),
    );
    map.insert(
        "removeBom".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_remove_bom))),
    );
    map.insert(
        "addBom".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_add_bom))),
    );

    map.insert(
        "createBinaryReader".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_create_binary_reader))),
    );
    map.insert(
        "createBinaryWriter".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_create_binary_writer))),
    );

    Value::Object(Arc::new(map))
}
