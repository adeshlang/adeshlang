# AdeshLang Encoding Standard Library — Examples & Usage Guide

Welcome to the comprehensive examples suite for the **AdeshLang `Encoding` Standard Library**.

This directory contains executable AdeshLang scripts demonstrating production-grade text, Unicode, binary, Base64, Hex, percent encoding, integer/float serialization, endian conversion, VarInt/LEB128, Byte Order Mark (BOM) manipulation, and real-world protocol engineering.

---

## 1. Import Model

To use the `Encoding` library in your AdeshLang program:

```adesh
import Encoding;
```

or using namespaced imports:

```adesh
import { utf8Encode, utf8Decode, base64Encode, hexEncode } from Encoding;
```

---

## 2. Directory Map of Examples

| Category | File | Description |
| :--- | :--- | :--- |
| **Text Encodings** | [`basic_utf8.adesh`](./basic_utf8.adesh) | Primary UTF-8 encoding, decoding, length, and validation. |
| **Text Encodings** | [`utf8_unicode.adesh`](./utf8_unicode.adesh) | Multi-lingual scripts (Hindi, Sanskrit, CJK, Arabic, Emoji) & lossy decoding. |
| **Text Encodings** | [`utf8_validation.adesh`](./utf8_validation.adesh) | Strict validation & diagnostic error offset extraction. |
| **Text Encodings** | [`utf16_utf32.adesh`](./utf16_utf32.adesh) | UTF-16 LE/BE and UTF-32 LE/BE surrogate pair handling. |
| **Text Encodings** | [`ascii_strict.adesh`](./ascii_strict.adesh) | Strict ASCII validation & rejection of non-ASCII characters. |
| **Binary Encodings** | [`base64_hex.adesh`](./base64_hex.adesh) | RFC 4648 Base64 and lowercase Hexadecimal encodings. |
| **Binary Encodings** | [`base64_url_tokens.adesh`](./base64_url_tokens.adesh) | URL-safe Base64URL tokens (`+`/`/` → `-`/`_`, unpadded). |
| **Binary Encodings** | [`hex_identifiers.adesh`](./hex_identifiers.adesh) | Lowercase vs Uppercase hex for UUIDs & cryptographic digests. |
| **Web Encodings** | [`percent_form.adesh`](./percent_form.adesh) | RFC 3986 URI percent encoding & `application/x-www-form-urlencoded`. |
| **Binary Integers** | [`binary_integers.adesh`](./binary_integers.adesh) | Explicit Little-Endian vs Big-Endian integer conversions (`u32`, `u64`). |
| **VarInt & WASM** | [`varint_leb128.adesh`](./varint_leb128.adesh) | Base-128 VarInt, ZigZag signed integers, and WASM ULEB128/SLEB128. |
| **BOM Handling** | [`bom_handling.adesh`](./bom_handling.adesh) | Detecting, stripping, and prepending Byte Order Marks (`UTF-8`, `UTF-16LE/BE`). |
| **Real-World Use Case** | [`realworld_network_packet.adesh`](./realworld_network_packet.adesh) | Custom TCP binary protocol framing (Magic, Version, Cmd, Payload). |
| **Real-World Use Case** | [`realworld_wasm_header.adesh`](./realworld_wasm_header.adesh) | WebAssembly module header parser using ULEB128 section sizes. |
| **Real-World Use Case** | [`realworld_file_transcode.adesh`](./realworld_file_transcode.adesh) | End-to-end transcode & web transport pipeline (`UTF-8` → `UTF-16BE+BOM` → `Base64URL`). |

---

## 3. How to Run the Examples

Run any example script using the `adeshlang` CLI binary:

```bash
cargo run --bin adeshlang -- run examples/libraries/encoding/basic_utf8.adesh
cargo run --bin adeshlang -- run examples/libraries/encoding/realworld_network_packet.adesh
cargo run --bin adeshlang -- run examples/libraries/encoding/realworld_file_transcode.adesh
```

---

## 4. Deep-Dive Detailed Breakdown

### 4.1 UTF-8 Encodings & Unicode (`basic_utf8.adesh` & `utf8_unicode.adesh`)

UTF-8 is the primary text representation in AdeshLang.
- `Encoding.utf8Encode(text)`: Converts string to UTF-8 byte array (`[u8]`).
- `Encoding.utf8Decode(bytes)`: Decodes UTF-8 bytes to String strictly. Errors if invalid.
- `Encoding.utf8DecodeLossy(bytes)`: Decodes lossily, replacing malformed byte sequences with Unicode replacement character `U+FFFD` (``).

**Example Snippet:**
```adesh
import Encoding;

let text = "AdeshLang 🚀 | Hindi: नमस्ते | Sanskrit: संस्कृतम्";
let bytes = Encoding.utf8Encode(text);
let decoded = Encoding.utf8Decode(bytes);
print("Decoded:", decoded);
```

### 4.2 UTF-8 Diagnostic Validation (`utf8_validation.adesh`)

`Encoding.validateUtf8(bytes)` verifies whether a byte array is valid UTF-8. If invalid, it returns a diagnostic message with the exact byte offset where corruption occurred.

```adesh
import Encoding;

let badBytes = [72, 101, 108, 128, 111]; // Bad byte at index 3
try {
    Encoding.validateUtf8(badBytes);
} catch (e) {
    print("Validation Diagnostic Error:", e); // "Invalid UTF-8 sequence at byte offset 3"
}
```

### 4.3 UTF-16 & UTF-32 (`utf16_utf32.adesh`)

Supports 2-byte (UTF-16) and 4-byte (UTF-32) encodings with Little-Endian (`LE`) and Big-Endian (`BE`) variants. Emojis and non-BMP characters are handled via surrogate pairs.

```adesh
import Encoding;

let utf16le = Encoding.utf16LEEncode("AdeshLang 🚀");
let restored = Encoding.utf16LEDecode(utf16le);

let utf32be = Encoding.utf32BEEncode("AdeshLang 🚀");
let restored32 = Encoding.utf32BEDecode(utf32be);
```

### 4.4 Base64 & Base64URL (`base64_hex.adesh` & `base64_url_tokens.adesh`)

- **Standard Base64 (RFC 4648)**: Uses `+` and `/` characters with `=` padding.
- **Base64URL (RFC 4648 §5)**: Replaces `+` with `-` and `/` with `_`, removing `=` padding for URL safety (ideal for OAuth & JWT tokens).

```adesh
import Encoding;

let rawBytes = [251, 255, 191, 186];
let stdB64 = Encoding.base64Encode(rawBytes);       // "+/+/..."
let urlToken = Encoding.base64UrlEncode(rawBytes);   // "-_-_..."
```

### 4.5 Hexadecimal Encodings (`hex_identifiers.adesh`)

- `Encoding.hexEncode(bytes)`: Encodes bytes to lowercase hex (`"deadbeef"`).
- `Encoding.hexEncodeUpper(bytes)`: Encodes bytes to uppercase hex (`"DEADBEEF"`).
- `Encoding.hexDecode(hexStr)`: Decodes hex string to byte array. Rejects odd lengths and invalid hex characters.

### 4.6 Percent & Form URL Encodings (`percent_form.adesh`)

- **Percent Encoding (RFC 3986)**: Encodes reserved characters as `%XX`.
- **Form Encoding (`application/x-www-form-urlencoded`)**: Encodes spaces as `+` and reserved characters as `%XX`.

```adesh
import Encoding;

let query = "name=Ajay Tainwala&city=New Delhi";
let formEnc = Encoding.formEncode(query); // "name%3DAjay+Tainwala%26city%3DNew+Delhi"
```

### 4.7 Endianness & Binary Integer Serialization (`binary_integers.adesh`)

- Converts integers (`u16`, `u32`, `u64`, `i16`, `i32`, `i64`) to/from explicit Little-Endian (`LE`) or Big-Endian (`BE`) byte arrays.
- `Encoding.readU32BE(buffer, offset)` reads an integer at a specific offset with bounds checking.

```adesh
import Encoding;

let val = 305419896; // 0x12345678
let leBytes = Encoding.u32ToBytesLE(val); // [0x78, 0x56, 0x34, 0x12]
let beBytes = Encoding.u32ToBytesBE(val); // [0x12, 0x34, 0x56, 0x78]
```

### 4.8 VarInt, ZigZag & WASM LEB128 (`varint_leb128.adesh`)

- **Unsigned VarInt**: Variable-length quantity (1-9 bytes for u64).
- **ZigZag**: Maps signed integers to unsigned space (0 → 0, -1 → 1, 1 → 2, -2 → 3).
- **WASM ULEB128 / SLEB128**: WASM specification compliant variable-length integer encoding.

```adesh
import Encoding;

let uleb = Encoding.uleb128Encode(624485); // [0xE5, 0x8E, 0x26]
let restored = Encoding.uleb128Decode(uleb); // 624485
```

### 4.9 Byte Order Mark (BOM) (`bom_handling.adesh`)

- `Encoding.detectBom(bytes)`: Returns `"UTF-8"`, `"UTF-16LE"`, `"UTF-16BE"`, `"UTF-32LE"`, `"UTF-32BE"`, or `"NONE"`.
- `Encoding.removeBom(bytes)`: Strips BOM if present and returns clean payload.
- `Encoding.addBom(bytes, format)`: Prepends requested BOM header.

---

## 5. Real-World Architecture Protocols

### Protocol 1: TCP Binary Framing (`realworld_network_packet.adesh`)
Demonstrates how to construct and parse binary packets for network servers/clients:
```
+----------------+----------------+----------------+-------------------+--------------------+
| Magic ("AD")   | Version (u16)  | Cmd Type (u8)  | Payload Len (u32) | Payload (UTF-8)    |
| 2 Bytes        | 2 Bytes (BE)   | 1 Byte         | 4 Bytes (BE)      | Variable           |
+----------------+----------------+----------------+-------------------+--------------------+
```

### Protocol 2: WebAssembly Module Inspector (`realworld_wasm_header.adesh`)
Parses WASM binary header bytes using `readU32LE` for versioning and `uleb128Decode` for variable-length section headers.

### Protocol 3: Transcode & Web Transport (`realworld_file_transcode.adesh`)
Shows end-to-end data transformation across different encoding layers:
`UTF-8 String` → `UTF-16BE Transcode + BOM` → `Base64URL Transfer Token` → `Decode & Strip BOM` → `Reconstruct UTF-8`.

---

## 6. Complete API Reference Quick Table

| API | Signature | Description |
| :--- | :--- | :--- |
| `Encoding.utf8Encode` | `(text: string) -> [u8]` | UTF-8 String to bytes |
| `Encoding.utf8Decode` | `(bytes: [u8]) -> string` | UTF-8 bytes to String (Strict) |
| `Encoding.utf8DecodeLossy` | `(bytes: [u8]) -> string` | UTF-8 bytes to String (Lossy) |
| `Encoding.isValidUtf8` | `(bytes: [u8]) -> bool` | UTF-8 boolean check |
| `Encoding.validateUtf8` | `(bytes: [u8]) -> null` | Detailed UTF-8 validation (throws on error) |
| `Encoding.utf16LEEncode` | `(text: string) -> [u8]` | UTF-16 Little-Endian encode |
| `Encoding.utf16BEDecode` | `(bytes: [u8]) -> string` | UTF-16 Big-Endian decode |
| `Encoding.utf32LEEncode` | `(text: string) -> [u8]` | UTF-32 Little-Endian encode |
| `Encoding.asciiEncode` | `(text: string) -> [u8]` | Strict ASCII encode |
| `Encoding.base64Encode` | `(bytes: [u8]) -> string` | Standard RFC 4648 Base64 |
| `Encoding.base64UrlEncode` | `(bytes: [u8]) -> string` | URL-safe Base64URL |
| `Encoding.hexEncode` | `(bytes: [u8]) -> string` | Lowercase Hexadecimal encode |
| `Encoding.hexEncodeUpper` | `(bytes: [u8]) -> string` | Uppercase Hexadecimal encode |
| `Encoding.hexDecode` | `(hexStr: string) -> [u8]` | Hexadecimal decode |
| `Encoding.percentEncode` | `(text: string) -> string` | RFC 3986 Percent encode |
| `Encoding.formEncode` | `(text: string) -> string` | Form URL encode (`+` for space) |
| `Encoding.u32ToBytesLE` | `(val: number) -> [u8]` | u32 Little-Endian bytes |
| `Encoding.u32ToBytesBE` | `(val: number) -> [u8]` | u32 Big-Endian bytes |
| `Encoding.readU32BE` | `(buffer: [u8], offset: number) -> number` | Read u32 BE at offset |
| `Encoding.uleb128Encode` | `(val: number) -> [u8]` | WASM ULEB128 encode |
| `Encoding.sleb128Encode` | `(val: number) -> [u8]` | WASM SLEB128 encode |
| `Encoding.detectBom` | `(bytes: [u8]) -> string` | Detect BOM format |
| `Encoding.removeBom` | `(bytes: [u8]) -> Object` | Strip BOM header |

---

Happy Coding with **AdeshLang Encoding Library**!
