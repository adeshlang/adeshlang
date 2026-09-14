# stdlib-guide.md

> Consolidated from 6 documentation files on 2026-08-29.

---


---

## Source: compression.md

# AdeshLang `Compression` Standard Library

## Overview

`Compression` is a production-grade, memory-safe, high-performance standard library for AdeshLang. Designed to meet and exceed the compression ecosystems of Rust, Go, C++, and Python, `Compression` provides lossless compression codecs, stateful streaming IO, trained dictionary compression, seekable indexed formats, ZIP and TAR archives, adaptive payload classification, and multi-threaded parallel compression while adhering to AdeshLang's ownership model, RAII, zero-GC architecture, and cross-platform runtime.

---

## 1. Quick Start & Import Model

Compression is an explicit standard library module. To use it in your AdeshLang program:

```adesh
import Compression;

// Simple general-purpose compression (defaults to Zstd)
let text = "Hello AdeshLang Compression Ecosystem!";
let compressed = Compression.compress(text);

// Decompress back to original
let original = Compression.decompress(compressed);
print("Match:", original == text);
```

You can also import specific functions:

```adesh
import { gzip, gunzip, zstd, unzstd, brotli, unbrotli } from Compression;

let gzData = gzip("Streaming payload", 6);
let decomp = gunzip(gzData);
```

---

## 2. Codec Selection Guide

| Codec | Speed (Encode / Decode) | Ratio | Key Strengths | Recommended Use Case |
| :--- | :--- | :--- | :--- | :--- |
| **Zstd** (Zstandard) | High / Extremely High | Very High | Modern default, dictionary support, seekable blocks | General purpose, log files, storage, RPC |
| **Gzip** | Medium / High | Moderate | Universal compatibility, legacy HTTP standard | Cross-system file exchange, legacy web |
| **Brotli** | Low (High levels) / High | Exceptional | Superior compression for text/JSON | Web static assets, HTTP response payloads |
| **LZ4** | Ultra High / Ultra High | Low to Mid | Near RAM bandwidth performance | High-throughput streaming, IPC, caching |
| **XZ / LZMA** | Low / Low | Maximum | Highest compression ratio for static binaries | Software distribution packages, firmware |
| **DEFLATE / ZLIB** | Medium / High | Moderate | Raw RFC 1951 / RFC 1950 implementation | PNG imaging, PDF streams, raw zip chunks |

---

## 3. Core Lossless Codec APIs

### General-Purpose API

```adesh
let compressed = Compression.compress(data, "zstd", 5);
let original = Compression.decompress(compressed, "zstd");
```

### Specific Codec Shortcuts

```adesh
// Zstandard (levels 1..22)
let zstdBytes = Compression.zstd(data, 5);
let rawData = Compression.unzstd(zstdBytes);

// GZIP (levels 1..9)
let gzBytes = Compression.gzip(data, 6);
let rawData = Compression.gunzip(gzBytes);

// Brotli (levels 1..11)
let brBytes = Compression.brotli(data, 6);
let rawData = Compression.unbrotli(brBytes);

// LZ4 (Ultra fast block compression)
let lz4Bytes = Compression.lz4(data);
let rawData = Compression.unlz4(lz4Bytes);

// XZ (Maximum ratio)
let xzBytes = Compression.xz(data);
let rawData = Compression.unxz(xzBytes);
```

---

## 4. Adaptive Compression & Classification

Adaptive mode analyzes payload entropy and structure (Text, JSON, Binary, Repetitive, Already Compressed) to automatically choose the best codec or skip compression if output would expand.

```adesh
let result = Compression.auto(payload);

if (result.wasCompressed) {
    print("Algorithm chosen:", result.algorithm);
    print("Original size:", result.originalSize);
    print("Compressed size:", result.compressedSize);
    print("Space saved %:", result.spaceSavedPercent);
} else {
    print("Compression skipped (data already compressed or high entropy).");
}
```

---

## 5. Seekable Random-Access Compressed Format

AdeshLang includes an indexed, chunked compressed format allowing O(1) random access to specific chunks without decompressing preceding blocks.

```adesh
// Create seekable compressed archive with 64KB chunks
let archive = Compression.createSeekable(datasetBytes, "zstd", 65536, 5);

// Directly read chunk #42 without full archive decompression
let chunk42 = Compression.readSeekableChunk(archive, 42);
```

---

## 6. ZIP & TAR Archives with Security Protection

`Compression` includes full ZIP and TAR archive generation and extraction with built-in defense against path traversal attacks (`..`, absolute paths, drive letters) and decompression bombs.

### ZIP Creation & Extraction

```adesh
let files = [
    { "name": "config.json", "content": "{\"port\": 8080}" },
    { "name": "docs/readme.txt", "content": "AdeshLang documentation" }
];

// Create ZIP archive
let zipBytes = Compression.createZip(files);

// Extract safely into destination folder
let extractedFileCount = Compression.extractZip(zipBytes, "output_dir");
```

### TAR Creation & Extraction

```adesh
let tarBytes = Compression.createTar(files);
let extractedCount = Compression.extractTar(tarBytes, "output_dir");
```

---

## 7. Multi-Threaded Parallel Compression

For large payloads, `parallelCompress` partitions input across available CPU cores and compresses chunks concurrently while preserving block ordering.

```adesh
let compressed = Compression.parallelCompress(largeBuffer, "zstd", 131072, 5, 4);
let decompressed = Compression.parallelDecompress(compressed, "zstd", null, 4);
```

---

## 8. Checksums & Verification

`Compression` exposes fast non-cryptographic checksum algorithms for payload validation:

```adesh
let c32  = Compression.crc32(data);    // IEEE 802.3 CRC32
let c32c = Compression.crc32c(data);   // Castagnoli CRC32C
let adl  = Compression.adler32(data);  // Adler32
let xx64 = Compression.xxhash64(data); // 64-bit XXHash
```

---

## 9. Security & Safeguards

1. **Decompression Bomb Protection**: All decoders enforce maximum output limits (`maxOutputSize`) and expansion ratio checks to prevent infinite memory expansion.
2. **Path Traversal Defense**: Archive extraction normalizes paths and forbids `..` or root-relative paths escaping the target folder.
3. **Integer Overflow Protection**: All offset and length calculations use checked arithmetic.
4. **No Dangling References**: Fully integrates with AdeshLang's RAII and borrowing safety rules.

---

## 10. Execution Backend Compatibility Matrix

| Backend | Build | Unit Tests | Integration Tests | Security Safeguards | Status |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Interpreter** | Yes | Pass | Pass | Enforced | Verified |
| **Bytecode VM** | Yes | Pass | Pass | Enforced | Verified |
| **JIT** | Yes | Pass | Pass | Enforced | Verified |
| **Native JIT** | Yes | Pass | Pass | Enforced | Verified |
| **LLVM AOT** | Yes | Pass | Pass | Enforced | Verified |
| **WASM** | Yes | Pass | Pass | Enforced | Verified |


---

## Source: encoding.md

# AdeshLang Encoding Standard Library Documentation

The `Encoding` standard library provides text, Unicode, binary, Base64, Hex, percent, binary serialization, endian conversion, VarInt/LEB128, and BOM manipulation utilities for AdeshLang.

## Import Model

To use the library in AdeshLang:

```adesh
import Encoding;
```

---

## Technical Features & API Index

### 1. Text Encodings (UTF-8, UTF-16, UTF-32, ASCII)

- `Encoding.utf8Encode(text)`: Converts string to UTF-8 byte array.
- `Encoding.utf8Decode(bytes)`: Decodes UTF-8 byte array strictly to String.
- `Encoding.utf8DecodeLossy(bytes)`: Decodes UTF-8 byte array lossily (replaces invalid sequences with U+FFFD).
- `Encoding.isValidUtf8(bytes)`: Checks if bytes are valid UTF-8.
- `Encoding.validateUtf8(bytes)`: Detailed validation returning error offset if invalid.
- `Encoding.utf16Encode(text)` / `utf16Decode(bytes)`: UTF-16 conversions.
- `Encoding.utf16LEEncode` / `utf16LEDecode` / `utf16BEEncode` / `utf16BEDecode`
- `Encoding.utf32Encode` / `utf32Decode` / `utf32LEEncode` / `utf32LEDecode` / `utf32BEEncode` / `utf32BEDecode`
- `Encoding.asciiEncode(text)` / `asciiDecode(bytes)` / `isAscii(data)`: Strict ASCII conversion.

### 2. Binary Encodings (Base64, Base64URL, Hex)

- `Encoding.base64Encode(bytes)`: Standard Base64 encoding (RFC 4648).
- `Encoding.base64Decode(str)`: Standard Base64 decoding.
- `Encoding.base64UrlEncode(bytes)`: URL-safe Base64 encoding.
- `Encoding.base64UrlDecode(str)`: URL-safe Base64 decoding.
- `Encoding.hexEncode(bytes)`: Lowercase Hexadecimal encoding.
- `Encoding.hexEncodeUpper(bytes)`: Uppercase Hexadecimal encoding.
- `Encoding.hexDecode(str)`: Hexadecimal decoding.

### 3. Percent & Form URL Encodings

- `Encoding.percentEncode(text)`: Percent encodes reserved URI characters (RFC 3986).
- `Encoding.percentDecode(str)`: Percent decodes URI string.
- `Encoding.urlEncodeComponent(text)` / `urlDecodeComponent(str)`
- `Encoding.formEncode(text)` / `formDecode(str)`: `application/x-www-form-urlencoded` format (spaces ↔ `+`).

### 4. Endianness & Binary Integers / Floats

- `Encoding.u16ToBytesLE` / `u16ToBytesBE` / `bytesToU16LE` / `bytesToU16BE`
- `Encoding.u32ToBytesLE` / `u32ToBytesBE` / `bytesToU32LE` / `bytesToU32BE`
- `Encoding.u64ToBytesLE` / `u64ToBytesBE` / `bytesToU64LE` / `bytesToU64BE`
- `Encoding.i16ToBytesLE` / `i16ToBytesBE` / `bytesToI16LE` / `bytesToI16BE`
- `Encoding.i32ToBytesLE` / `i32ToBytesBE` / `bytesToI32LE` / `bytesToI32BE`
- `Encoding.i64ToBytesLE` / `i64ToBytesBE` / `bytesToI64LE` / `bytesToI64BE`
- `Encoding.f32ToBytesLE` / `f32ToBytesBE` / `bytesToF32LE` / `bytesToF32BE`
- `Encoding.f64ToBytesLE` / `f64ToBytesBE` / `bytesToF64LE` / `bytesToF64BE`
- `Encoding.readU16LE(buf, offset)` / `readU32LE` / `readU64LE` (with bounds safety)

### 5. VarInt, ZigZag & LEB128

- `Encoding.varIntEncode(num)` / `varIntDecode(bytes)`: Unsigned VarInt (Base-128).
- `Encoding.zigzagEncode(num)` / `zigzagDecode(num)`: ZigZag signed integer transformation.
- `Encoding.signedVarIntEncode(num)` / `signedVarIntDecode(bytes)`
- `Encoding.uleb128Encode(num)` / `uleb128Decode(bytes)`: WASM ULEB128 format.
- `Encoding.sleb128Encode(num)` / `sleb128Decode(bytes)`: WASM SLEB128 format.

### 6. Byte Order Mark (BOM)

- `Encoding.detectBom(bytes)`: Returns `"UTF-8"`, `"UTF-16LE"`, `"UTF-16BE"`, `"UTF-32LE"`, `"UTF-32BE"`, or `"NONE"`.
- `Encoding.removeBom(bytes)`: Strips BOM if present.
- `Encoding.addBom(bytes, encodingName)`: Prepends requested BOM header.


---

## Source: json.md

# AdeshLang Built-in JSON System Documentation

AdeshLang provides a **built-in, production-grade, language-level JSON system** that requires **no imports** (`no import JSON;`).

All JSON facilities are globally available under the `JSON` namespace as part of the language prelude.

---

## Quick Start

```adesh
// Parse JSON text into a value (No import needed!)
let data = JSON.parse("{\"name\": \"AdeshLang\", \"version\": 1}");

println(data.name); // Output: AdeshLang

// Compact serialization
let compact = JSON.stringify(data);

// Pretty printing
let pretty = JSON.stringifyPretty(data);
```

---

## API Cheat-Sheet

| Function | Signature | Description |
| :--- | :--- | :--- |
| `JSON.parse(text, maxDepth?)` | `(str, [num]) -> Value` | Parse JSON text into an owned Value. |
| `JSON.parseBytes(bytes)` | `(array) -> Value` | Parse JSON from UTF-8 byte array or string. |
| `JSON.parseFile(path)` | `(str) -> Value` | Read file at `path` and parse content as JSON. |
| `JSON.stringify(value)` | `(Value) -> str` | Serialize value into compact JSON text. |
| `JSON.stringifyPretty(value, indent?)` | `(Value, [num]) -> str` | Serialize value into formatted, pretty-printed JSON. |
| `JSON.stringifyCompact(value)` | `(Value) -> str` | Alias for compact serialization. |
| `JSON.stringifyFile(path, value, pretty?)`| `(str, Value, [bool]) -> bool` | Serialize value to JSON and write directly to file. |
| `JSON.stringifyBytes(value)` | `(Value) -> array` | Serialize value to JSON UTF-8 byte array. |
| `JSON.isValid(text)` | `(str) -> bool` | Check if string is valid JSON without throwing. |
| `JSON.tryParse(text)` | `(str) -> Value` | Parse JSON safely; returns `null` on syntax error. |
| `JSON.minify(text)` | `(str) -> str` | Minify JSON string by removing whitespace. |
| `JSON.pretty(text)` | `(str) -> str` | Format unformatted JSON string into pretty JSON. |
| `JSON.encode(value)` | `(Value) -> str` | Encode value to JSON string. |
| `JSON.decode(text)` | `(str) -> Value` | Decode JSON string to Value. |
| `JSON.object()` | `() -> object` | Construct a new empty JSON object. |
| `JSON.array()` | `() -> array` | Construct a new empty JSON array. |
| `JSON.from(collection)` | `(col) -> Value` | Convert collection/map/struct to JSON value. |

---

## No-Import Requirement

Unlike conventional standard libraries that require module importing, AdeshLang embeds `JSON` directly into the language environment.

```adesh
// DO NOT write: import JSON;

// Write directly:
let config = JSON.parseFile("config.json");
```

---

## Error Handling & Security

1. **Parse Diagnostics**: Invalid JSON inputs raise detailed errors including line, column, and error description.
2. **Recursion Depth Limits**: `JSON.parse` enforces a default maximum nesting depth (128) to prevent stack overflow attacks on malicious nested inputs.

---

## Cross-Backend Support

The JSON system is fully integrated across all AdeshLang execution backends:
- Interpreter
- Bytecode VM
- JIT Compiler
- Native JIT
- LLVM AOT
- WebAssembly (WASM)


---

## Source: time.md

# AdeshLang Time Standard Library Documentation

## 1. Overview
The `Time` standard library for AdeshLang provides comprehensive, production-grade tools for date and time representation, calendar arithmetic, clock measurement, formatting, parsing, and timezones. 

It is designed with a strict type-safety philosophy that separates monotonic time (used for benchmarking and deadlines) from wall-clock/calendar time (used for human-readable dates and scheduling).

## 2. Importing Time
To use the time library in an AdeshLang program, import the `Time` module:
```adesh
import Time;
```

---

## 3. Duration
A `Duration` represents a span of elapsed time with nanosecond precision. It is signed, allowing representing differences backwards in time.

### Constructors
- `Time.durationNanoseconds(value: bigint): Duration`
- `Time.durationMicroseconds(value: bigint): Duration`
- `Time.durationMilliseconds(value: bigint): Duration`
- `Time.durationSeconds(value: bigint): Duration`
- `Time.durationMinutes(value: bigint): Duration`
- `Time.durationHours(value: bigint): Duration`
- `Time.durationDays(value: bigint): Duration`
- `Time.durationWeeks(value: bigint): Duration`
- `Time.durationSecondsF64(value: f64): Duration`
- `Time.durationMillisecondsF64(value: f64): Duration`

### Accessors
- `duration.wholeNanoseconds(): bigint`
- `duration.wholeMicroseconds(): bigint`
- `duration.wholeMilliseconds(): bigint`
- `duration.wholeSeconds(): bigint`
- `duration.wholeMinutes(): bigint`
- `duration.wholeHours(): bigint`
- `duration.wholeDays(): bigint`

### Arithmetic & Validation
- `duration.checkedAdd(other: Duration): Duration`
- `duration.checkedSub(other: Duration): Duration`
- `duration.checkedMul(factor: i32): Duration`
- `duration.checkedDiv(divisor: i32): Duration`
- `duration.isZero(): boolean`
- `duration.isPositive(): boolean`
- `duration.isNegative(): boolean`
- `duration.abs(): Duration`
- `duration.toString(): string` (outputs in compact format, e.g., `"5m 30s"`)

---

## 4. Instant
An `Instant` represents a monotonic point in time. It is retrieved using a monotonic hardware clock and cannot go backward even if the system wall clock is adjusted. It should NOT be converted into calendar dates or Unix timestamps.

### API
- `Time.instantNow(): Instant`
- `instant.elapsed(): Duration`
- `instant.durationSince(other: Instant): Duration`
- `instant.checkedAdd(duration: Duration): Instant`
- `instant.checkedSub(duration: Duration): Instant`
- `instant.lt(other: Instant): boolean`
- `instant.gt(other: Instant): boolean`
- `instant.eq(other: Instant): boolean`

---

## 5. Measuring Elapsed Time
Use `Instant` to benchmark work:
```adesh
let start = Time.instantNow();
doHeavyWork();
let elapsed = start.elapsed();
print("Completed in: " + elapsed.toString());
```

---

## 6. SystemTime
A `SystemTime` represents a wall-clock timestamp tied to the operating system's system clock. Unlike `Instant`, system time can jump forwards or backwards.

### API
- `Time.systemTimeNow(): SystemTime`
- `SystemTime.unixEpoch(): SystemTime`
- `systemTime.durationSince(other: SystemTime): Duration`
- `systemTime.durationSinceEpoch(): Duration`
- `systemTime.toUnixSeconds(): bigint`
- `systemTime.toUnixMilliseconds(): bigint`
- `systemTime.toUnixMicroseconds(): bigint`
- `systemTime.toUnixNanoseconds(): bigint`

---

## 7. Unix Timestamps
Timestamps can be created from Unix seconds/milliseconds/microseconds/nanoseconds:
- `Time.systemTimeFromUnixSeconds(secs: bigint): SystemTime`
- `Time.systemTimeFromUnixMilliseconds(ms: bigint): SystemTime`
- `Time.systemTimeFromUnixMicroseconds(us: bigint): SystemTime`
- `Time.systemTimeFromUnixNanoseconds(ns: bigint): SystemTime`

---

## 8. Date
A `Date` represents a timezone-naive calendar date (year, month, day) under the Gregorian calendar.

### API
- `Time.dateYmd(year: i32, month: i32, day: i32): Date`
- `Time.dateToday(): Date`
- `Time.dateTodayUtc(): Date`
- `date.year(): i32`
- `date.month(): i32`
- `date.day(): i32`
- `date.weekday(): Weekday`
- `date.dayOfYear(): i32`
- `date.isLeapYear(): boolean`
- `date.daysInMonth(): i32`
- `date.daysInYear(): i32`

### Date Arithmetic
- `date.addDays(days: i32): Date`
- `date.subDays(days: i32): Date`
- `date.addMonths(months: i32): Date`
- `date.addYears(years: i32): Date`
- `date.daysSince(other: Date): i32`

---

## 9. TimeOfDay
A `TimeOfDay` represents a timezone-naive clock time (hour, minute, second, nanosecond).

### API
- `Time.timeOfDay(hour: i32, minute: i32, second: i32, nanosecond: i32): TimeOfDay`
- `time.hour(): i32`
- `time.minute(): i32`
- `time.second(): i32`
- `time.nanosecond(): i32`
- `time.millisecond(): i32`
- `time.microsecond(): i32`
- `time.secondsSinceMidnight(): i32`

---

## 10. DateTime
A `DateTime` represents a timezone-naive local date and time. It is a combination of a `Date` and a `TimeOfDay`.

### API
- `Time.dateTime(date: Date, time: TimeOfDay): DateTime`
- `Time.dateTimeNow(): DateTime`
- `dt.date(): Date`
- `dt.time(): TimeOfDay`
- `dt.year(): i32`
- `dt.month(): i32`
- `dt.day(): i32`
- `dt.hour(): i32`
- `dt.minute(): i32`
- `dt.second(): i32`
- `dt.nanosecond(): i32`
- `dt.addDays(days: i32): DateTime`
- `dt.subHours(hours: i32): DateTime`

---

## 11. UTC
`UtcDateTime` represents an absolute instant in UTC.
- `Time.utcDateTimeNow(): UtcDateTime`
- `utc.unixSeconds(): bigint`
- `utc.unixMilliseconds(): bigint`
- `utc.unixNanoseconds(): bigint`
- `utc.toIsoString(): string`
- `utc.toRfc3339(): string`

---

## 12. UTC Offsets
`UtcOffset` represents a fixed timezone offset from UTC.
- `Time.utcOffset(seconds: i32): UtcOffset`
- `Time.utcOffsetHoursMinutes(hours: i32, minutes: i32): UtcOffset`
- `Time.utcOffsetUTC(): UtcOffset`
- `offset.totalSeconds(): i32`
- `offset.isUtc(): boolean`
- `offset.isPositive(): boolean`
- `offset.isNegative(): boolean`

---

## 13. Named Timezones
`TimeZone` represents a set of rules (e.g., transitions for daylight-saving time) for a specific IANA location.
- `Time.timeZoneUtc(): TimeZone`
- `Time.timeZoneLocal(): TimeZone`
- `Time.timeZoneOf(name: string): TimeZone`
- `tz.name(): string`
- `tz.offsetAt(utc: UtcDateTime): UtcOffset`

---

## 14. ZonedDateTime
A `ZonedDateTime` represents an absolute instant associated with a named `TimeZone`.
- `Time.zonedDateTimeNow(tz: TimeZone): ZonedDateTime`
- `zoned.zone(): TimeZone`
- `zoned.offset(): UtcOffset`
- `zoned.toUtc(): UtcDateTime`
- `zoned.toZone(otherZone: TimeZone): ZonedDateTime`

---

## 15. DST
Daylight Saving Time transitions are resolved automatically based on the named IANA timezone when querying offset at a specific absolute instant.

---

## 16. Formatting
Format instances of `Date`, `TimeOfDay`, `DateTime`, `UtcDateTime`, `OffsetDateTime`, or `ZonedDateTime` using pattern strings:
- `dt.format("yyyy-MM-dd HH:mm:ss")`
- `dt.toIsoString()`

---

## 17. Parsing
Parsing is strict by default to prevent unpredictable heuristics:
- `Time.dateParse(s: string, pattern: string): Date`
- `Time.timeOfDayParse(s: string, pattern: string): TimeOfDay`
- `Time.dateTimeParse(s: string, pattern: string): DateTime`

---

## 18. ISO 8601
Standard ISO-8601 formats are supported via `toIsoString()` and `parseIso()`.
- Example format: `2026-07-14T23:30:00Z` or `2026-07-14T23:30:00.123456789+05:30`.

---

## 19. RFC 3339
RFC 3339 compliant formatting and parsing is supported natively.
- `utc.toRfc3339()`
- `Time.offsetDateTimeParseRfc3339(s: string): OffsetDateTime`

---

## 20. Sleeping
- Blocking sleep: `Time.sleep(duration: Duration)`
- Example: `Time.sleep(Time.durationSeconds(2n))`

---

## 21. Timers
Timers allow waiting for a duration:
- `let timer = Time.timerAfter(duration: Duration)`
- `timer.wait()`
- `timer.cancel()`

---

## 22. Stopwatch
A high-resolution monotonic timer:
- `let sw = Time.stopwatchStartNew()`
- `sw.stop()`
- `sw.elapsed(): Duration`
- `sw.reset()`
- `sw.restart()`

---

## 23. Async Sleep
*Note: Currently blocked on the runtime thread executor model. Present standard blocking fallback `Time.sleep(duration)` is exposed as default.*

---

## 24. Ownership Behavior
- Small calendar types (`Date`, `TimeOfDay`, `UtcOffset`, `Weekday`, `Month`) represent small immutable values and copy/move safely.
- Duration is an immutable integer-like record.
- Monotonic instances (`Instant`) represent system-local references.

---

## 25. Thread Safety
All core types (`Duration`, `Date`, `TimeOfDay`, `UtcDateTime`, `UtcOffset`) are immutable and safe to cross thread boundaries.

---

## 26. Platform Behavior
Platform-dependent timestamps (like clock resolution) fallback safely to the host environment resolution.

---

## 27. Precision
Nanosecond storage is fully supported, though real system resolution will vary based on hardware platform capability (typically microsecond-level on Windows, nanosecond-level on modern Linux).

---

## 28. Leap Seconds
Leap seconds are handled by normalization (e.g. `23:59:60` is treated as a normal tick transition corresponding to the platform standard).

---

## 29. Common Mistakes
- **Do not use `DateTime.now()` for benchmarking**. Always use `Instant.now()` or `Stopwatch` because system clocks can jump.
- **Do not mix timezone-naive and timezone-aware types**. Compare `UtcDateTime` only with `UtcDateTime`, and naive `DateTime` only with `DateTime`.

---

## 30. Migration Examples

### JavaScript
```javascript
// JS
const now = new Date();
// AdeshLang
let now = Time.dateTimeNow();
```

### Python
```python
# Python
import datetime
now = datetime.datetime.now(datetime.timezone.utc)
# AdeshLang
let now = Time.utcDateTimeNow();
```

### Rust
```rust
// Rust
let start = std::time::Instant::now();
// AdeshLang
let start = Time.instantNow();
```

### Java
```java
// Java
LocalDate today = LocalDate.now();
// AdeshLang
let today = Time.dateToday();
```

### Go
```go
// Go
elapsed := time.Since(start)
// AdeshLang
let elapsed = start.elapsed();
```


---

## Source: MATH_LIBRARY.md

# AdeshLang Mathematical Libraries

AdeshLang provides comprehensive support for mathematical operations through the built-in `Math` module for real-number and `cmath` module for complex-number arithmetic, as well as native support for scientific mantissa literals.

---

## Scientific Mantissa Float Literals

AdeshLang supports native parsing of floating-point literals in scientific/mantissa format using `e` or `E` with positive or negative exponents.

```adesh
let light_speed = 2.99792458e8;
let plank = 6.62607015E-34;
```

---

## Math Module

The `Math` library provides constants and utility functions for real-number mathematical operations.

```adesh
import Math;
```

### Constants

| Constant | Value | Description |
|---|---|---|
| `Math.PI` / `Math.pi` | `3.141592653589793` | Ratio of a circle's circumference to its diameter |
| `Math.E` / `Math.e` | `2.718281828459045` | Euler's number, base of natural logarithms |
| `Math.TAU` / `Math.tau` | `6.283185307179586` | The circle constant, equal to 2π |
| `Math.SQRT2` | `1.4142135623730951` | Square root of 2 |
| `Math.SQRT1_2` | `0.7071067811865476` | Square root of 1/2 |
| `Math.LN2` | `0.6931471805599453` | Natural logarithm of 2 |
| `Math.LN10` | `2.302585092994046` | Natural logarithm of 10 |
| `Math.inf` | `inf` | Positive infinity |
| `Math.nan` | `NaN` | Not-a-Number |

---

### Random Number Generation

**Math.random()**
Return a random float in the range [0.0, 1.0).

```adesh
let r = Math.random();
```

**Math.seed(seed)**
Seed the random number generator for deterministic sequences.

```adesh
Math.seed(42);
```

**Math.randomInt(min, max)**
Return a random integer in the range [min, max] (inclusive).

```adesh
let dice = Math.randomInt(1, 6);
```

**Math.randomRange(min, max)**
Return a random float in the range [min, max).

```adesh
let temp = Math.randomRange(20.0, 30.0);
```

---

### Number Theoretic and Representation Functions

**Math.gcd(*integers)**
Return the greatest common divisor of the specified integer arguments.

```adesh
Math.gcd(24, 36)      // -> 12
Math.gcd(12, 18, 30)  // -> 6
```

**Math.lcm(*integers)**
Return the least common multiple of the specified integer arguments.

```adesh
Math.lcm(24, 36)      // -> 72
```

**Math.factorial(n)**
Return n! (factorial) as a float for non-negative integer n.

```adesh
Math.factorial(5)  // -> 120.0
```

**Math.comb(n, k)**
Return the number of ways to choose k items from n items without order (combinations).

```adesh
Math.comb(5, 2)  // -> 10.0
```

**Math.perm(n, k)**
Return the number of ways to choose k items from n items with order (permutations).

```adesh
Math.perm(5, 2)  // -> 20.0
```

**Math.isqrt(n)**
Return the integer square root of the non-negative integer n.

```adesh
Math.isqrt(25)  // -> 5.0
Math.isqrt(26)  // -> 5.0
```

---

### Power and Logarithmic Functions

**Math.pow(x, y)**
Return x raised to the power y.

```adesh
Math.pow(2, 10)    // -> 1024.0
Math.pow(9, 0.5)   // -> 3.0
```

**Math.sqrt(x)**
Return the square root of x.

```adesh
Math.sqrt(16)  // -> 4.0
```

**Math.cbrt(x)**
Return the cube root of x.

```adesh
Math.cbrt(27)  // -> 3.0
```

**Math.exp(x)**
Return e raised to the power x.

```adesh
Math.exp(1)  // -> 2.718281828459045
```

**Math.exp2(x)**
Return 2 raised to the power x.

```adesh
Math.exp2(10)  // -> 1024.0
```

**Math.expm1(x)**
Return e raised to the power x, minus 1. Accurate for small x.

```adesh
Math.expm1(1e-10)  // -> 1.00000000005e-10
```

**Math.log(x, [base])**
Return the logarithm of x to the given base. If base is omitted, return the natural logarithm (base e).

```adesh
Math.log(10)         // -> 2.302585092994046
Math.log(100, 10)    // -> 2.0
```

**Math.log10(x)**
Return the base-10 logarithm of x.

```adesh
Math.log10(100)  // -> 2.0
```

**Math.log2(x)**
Return the base-2 logarithm of x.

```adesh
Math.log2(8)  // -> 3.0
```

**Math.log1p(x)**
Return the natural logarithm of 1 + x. Accurate for small x.

```adesh
Math.log1p(1e-10)  // -> 1e-10
```

---

### Trigonometric Functions

**Math.sin(x)**
Return the sine of x radians.

```adesh
Math.sin(Math.PI / 2)  // -> 1.0
```

**Math.cos(x)**
Return the cosine of x radians.

```adesh
Math.cos(Math.PI)  // -> -1.0
```

**Math.tan(x)**
Return the tangent of x radians.

```adesh
Math.tan(Math.PI / 4)  // -> 1.0
```

**Math.asin(x)**
Return the arcsine of x, in radians.

```adesh
Math.asin(1.0)  // -> 1.5707963267948966
```

**Math.acos(x)**
Return the arccosine of x, in radians.

```adesh
Math.acos(0.0)  // -> 1.5707963267948966
```

**Math.atan(x)**
Return the arctangent of x, in radians.

```adesh
Math.atan(1.0)  // -> 0.7853981633974483
```

**Math.atan2(y, x)**
Return atan(y / x), in radians. The result is the angle from the X axis to point (x, y).

```adesh
Math.atan2(1, 1)  // -> 0.7853981633974483
```

---

### Hyperbolic Functions

**Math.sinh(x)**
Return the hyperbolic sine of x.

```adesh
Math.sinh(1.0)  // -> 1.1752011936438014
```

**Math.cosh(x)**
Return the hyperbolic cosine of x.

```adesh
Math.cosh(1.0)  // -> 1.5430806348152437
```

**Math.tanh(x)**
Return the hyperbolic tangent of x.

```adesh
Math.tanh(1.0)  // -> 0.7615941559557649
```

**Math.asinh(x)**
Return the inverse hyperbolic sine of x.

```adesh
Math.asinh(1.0)  // -> 0.881373587019543
```

**Math.acosh(x)**
Return the inverse hyperbolic cosine of x.

```adesh
Math.acosh(2.0)  // -> 1.3169578969248166
```

**Math.atanh(x)**
Return the inverse hyperbolic tangent of x.

```adesh
Math.atanh(0.5)  // -> 0.5493061443340549
```

---

### Angle Conversion

**Math.degToRad(deg)**, **Math.radians(deg)**
Convert degrees to radians.

```adesh
Math.degToRad(180.0)  // -> 3.141592653589793
Math.radians(90.0)    // -> 1.5707963267948966
```

**Math.radToDeg(rad)**, **Math.degrees(rad)**
Convert radians to degrees.

```adesh
Math.radToDeg(Math.PI)  // -> 180.0
Math.degrees(Math.PI/2) // -> 90.0
```

---

### Rounding and Numeric Utilities

**Math.floor(x)**
Return the floor of x, the largest integer less than or equal to x.

```adesh
Math.floor(3.7)   // -> 3.0
Math.floor(-3.7)  // -> -4.0
```

**Math.ceil(x)**
Return the ceiling of x, the smallest integer greater than or equal to x.

```adesh
Math.ceil(3.2)   // -> 4.0
Math.ceil(-3.2)  // -> -3.0
```

**Math.round(x)**
Return x rounded to the nearest integer.

```adesh
Math.round(3.5)   // -> 4.0
Math.round(3.4)   // -> 3.0
```

**Math.trunc(x)**
Return x truncated to its integral part (toward zero).

```adesh
Math.trunc(3.7)   // -> 3.0
Math.trunc(-3.7)  // -> -3.0
```

**Math.abs(x)**, **Math.fabs(x)**
Return the absolute value of x.

```adesh
Math.abs(-5.0)   // -> 5.0
Math.fabs(-3.14) // -> 3.14
```

**Math.sign(x)**
Return the sign of x: 1.0 if positive, -1.0 if negative, 0.0 if zero.

```adesh
Math.sign(5)   // -> 1.0
Math.sign(-5)  // -> -1.0
Math.sign(0)   // -> 0.0
```

**Math.min(a, b, ...)**
Return the smallest of the input values.

```adesh
Math.min(3, 7, 2, 9)  // -> 2.0
```

**Math.max(a, b, ...)**
Return the largest of the input values.

```adesh
Math.max(3, 7, 2, 9)  // -> 9.0
```

---

### Float Utilities

**Math.fma(x, y, z)**
Return x * y + z, computed as a fused multiply-add operation (single rounding).

```adesh
Math.fma(2.0, 3.0, 1.0)  // -> 7.0
```

**Math.fmod(x, y)**
Return the floating-point remainder of x / y.

```adesh
Math.fmod(5.5, 2.0)  // -> 1.5
```

**Math.modf(x)**
Return the fractional and integral parts of x as a tuple `(fract, integral)`. Both parts carry the sign of x.

```adesh
let (fract, integral) = Math.modf(3.7);
// fract -> 0.7, integral -> 3.0
```

**Math.remainder(x, y)**
Return the IEEE 754 remainder of x / y.

```adesh
Math.remainder(5.5, 2.0)  // -> -0.5
```

**Math.copysign(x, y)**
Return a float with the magnitude of x and the sign of y.

```adesh
Math.copysign(3.0, -2.0)  // -> -3.0
```

**Math.frexp(x)**
Return the mantissa and exponent of x as a tuple `(m, e)` such that x = m * 2^e.

```adesh
let (m, e) = Math.frexp(12.0);
// m -> 0.75, e -> 4
```

**Math.ldexp(x, i)**
Return x * 2^i (inverse of frexp).

```adesh
Math.ldexp(0.75, 4)  // -> 12.0
```

**Math.ulp(x)**
Return the value of the least significant bit (ULP) of x.

```adesh
Math.ulp(1.0)  // -> 2.220446049250313e-16
```

**Math.nextafter(x, y, [steps])**
Return the next representable float after x moving toward y. Steps defaults to 1.

```adesh
Math.nextafter(1.0, 2.0)           // -> 1.0000000000000002
Math.nextafter(1.0, 2.0, 3)        // -> 1.0000000000000007
```

---

### Comparison and Classification

**Math.isclose(a, b, [rel_tol], [abs_tol])**
Return true if a and b are close to each other. Tolerance defaults: rel_tol=1e-09, abs_tol=0.0.

```adesh
Math.isclose(0.1 + 0.2, 0.3)       // -> true
Math.isclose(1e10, 1e10 + 1)       // -> false
Math.isclose(1e10, 1e10 + 1, 1e-6) // -> true
```

**Math.isfinite(x)**
Return true if x is finite (not infinity and not NaN).

```adesh
Math.isfinite(42)        // -> true
Math.isfinite(1.0 / 0.0) // -> false
```

**Math.isinf(x)**
Return true if x is positive or negative infinity.

```adesh
Math.isinf(1.0 / 0.0)  // -> true
```

**Math.isnan(x)**
Return true if x is NaN (Not-a-Number).

```adesh
Math.isnan(0.0 / 0.0)  // -> true
```

---

### Vector and Array Math

**Math.clamp(val, min, max)**
Clamp val to the range [min, max].

```adesh
Math.clamp(45, 0, 10)   // -> 10.0
Math.clamp(-5, 0, 10)   // -> 0.0
Math.clamp(5, 0, 10)    // -> 5.0
```

**Math.lerp(a, b, t)**
Return the linear interpolation between a and b: a + (b - a) * t.

```adesh
Math.lerp(10, 20, 0.5)  // -> 15.0
```

**Math.hypot(*coords)**
Return the Euclidean norm (sqrt of sum of squares) of the arguments.

```adesh
Math.hypot(3, 4)       // -> 5.0
Math.hypot(1, 1, 1, 1) // -> 2.0
```

**Math.dist(p, q)**
Return the Euclidean distance between two coordinate arrays p and q.

```adesh
Math.dist([0, 0], [3, 4])  // -> 5.0
```

**Math.fsum(iterable)**
Return an accurate floating-point sum of the elements in an array (uses Kahan summation).

```adesh
Math.fsum([0.1, 0.2, 0.3])  // -> 0.6
```

**Math.prod(iterable, [start])**
Return the product of elements in the array, starting from start (default 1.0).

```adesh
Math.prod([2, 3, 4])      // -> 24.0
Math.prod([2, 3, 4], 0)   // -> 0.0
```

**Math.sumprod(p, q)**
Return the sum of products of corresponding elements from two arrays.

```adesh
Math.sumprod([1, 2, 3], [4, 5, 6])  // -> 32.0
```

---

### Special Functions

**Math.erf(x)**
Return the error function of x.

```adesh
Math.erf(0.0)  // -> 0.0
Math.erf(2.0)  // -> 0.9953222650189527
```

**Math.erfc(x)**
Return the complementary error function of x (1 - erf(x)).

```adesh
Math.erfc(2.0)  // -> 0.0046777349810473
```

**Math.gamma(x)**
Return the Gamma function of x.

```adesh
Math.gamma(5.0)  // -> 24.0
Math.gamma(0.5)  // -> 1.772453850905516
```

**Math.lgamma(x)**
Return the natural logarithm of the absolute value of the Gamma function of x.

```adesh
Math.lgamma(5.0)  // -> 3.1780538303479458
```

---

### String Formatting

**Math.formatDecimal(n, [precision])**
Format a float n as a string with the specified decimal precision (default 6), avoiding scientific notation.

```adesh
Math.formatDecimal(Math.PI)      // -> "3.141593"
Math.formatDecimal(Math.PI, 2)   // -> "3.14"
Math.formatDecimal(1e-7, 10)     // -> "0.0000001000"
```

---

## CMath Module (Complex Numbers)

The `cmath` library provides mathematical operations for complex numbers (`Value::Complex(real, imag)` or the `+` operator). Complex numbers can be constructed using `cmath.rect(r, phi)` or directly with expressions like `3 + 4i`.

```adesh
import cmath;
```

### Constants

| Constant | Value | Description |
|---|---|---|
| `cmath.pi` | `3.141592653589793` | The mathematical constant π |
| `cmath.e` | `2.718281828459045` | Euler's number |
| `cmath.tau` | `6.283185307179586` | The circle constant (2π) |
| `cmath.inf` | `inf` | Positive float infinity |
| `cmath.nan` | `NaN` | Float Not-a-Number |
| `cmath.infj` | `0.0 + infj` | Complex infinity (infinity along the imaginary axis) |
| `cmath.nanj` | `0.0 + NaNj` | Complex NaN |

---

### Trigonometric Functions

**cmath.cos(z)**
Return the cosine of complex z.

- **Parameters:** `z` — A complex number.
- **Returns:** `Complex` — The cosine of z.

```adesh
let z = cmath.cos(0 + 0i);
// z -> 1 + 0i
```

**cmath.sin(z)**
Return the sine of complex z.

- **Parameters:** `z` — A complex number.
- **Returns:** `Complex` — The sine of z.

```adesh
let z = cmath.sin(cmath.rect(1, 0));
```

**cmath.tan(z)**
Return the tangent of complex z.

- **Parameters:** `z` — A complex number.
- **Returns:** `Complex` — The tangent of z.

```adesh
let z = cmath.tan(0 + 0i);
// z -> 0 + 0i
```

---

### Inverse Trigonometric Functions

**cmath.acos(z)**
Return the arc cosine of complex z.

- **Parameters:** `z` — A complex number.
- **Returns:** `Complex` — The arc cosine of z.

```adesh
let z = cmath.acos(cmath.rect(0.5, 0));
```

**cmath.asin(z)**
Return the arc sine of complex z.

- **Parameters:** `z` — A complex number.
- **Returns:** `Complex` — The arc sine of z.

**cmath.atan(z)**
Return the arc tangent of complex z.

- **Parameters:** `z` — A complex number.
- **Returns:** `Complex` — The arc tangent of z.

---

### Hyperbolic Functions

**cmath.cosh(z)**
Return the hyperbolic cosine of complex z.

- **Parameters:** `z` — A complex number.
- **Returns:** `Complex` — The hyperbolic cosine of z.

```adesh
let z = cmath.cosh(0 + 0i);
// z -> 1 + 0i
```

**cmath.sinh(z)**
Return the hyperbolic sine of complex z.

- **Parameters:** `z` — A complex number.
- **Returns:** `Complex` — The hyperbolic sine of z.

```adesh
let z = cmath.sinh(0 + 0i);
// z -> 0 + 0i
```

**cmath.tanh(z)**
Return the hyperbolic tangent of complex z.

- **Parameters:** `z` — A complex number.
- **Returns:** `Complex` — The hyperbolic tangent of z.

---

### Inverse Hyperbolic Functions

**cmath.acosh(z)**
Return the inverse hyperbolic cosine of complex z.

- **Parameters:** `z` — A complex number.
- **Returns:** `Complex` — The inverse hyperbolic cosine of z.

**cmath.asinh(z)**
Return the inverse hyperbolic sine of complex z.

- **Parameters:** `z` — A complex number.
- **Returns:** `Complex` — The inverse hyperbolic sine of z.

**cmath.atanh(z)**
Return the inverse hyperbolic tangent of complex z.

- **Parameters:** `z` — A complex number.
- **Returns:** `Complex` — The inverse hyperbolic tangent of z.

---

### Power and Logarithmic Functions

**cmath.exp(z)**
Return e raised to the complex power z.

- **Parameters:** `z` — A complex number.
- **Returns:** `Complex` — e^z.

```adesh
let z = cmath.exp(cmath.rect(0, Math.PI));
// z -> -1 + 0i (Euler's identity: e^(iπ) = -1)
```

**cmath.log(z, [base])**
Return the logarithm of complex z to the given base. If base is omitted, return the natural logarithm (base e).

- **Parameters:**
  - `z` — A complex number.
  - `base` — Optional. A real number specifying the logarithm base.
- **Returns:** `Complex` — The logarithm of z.

```adesh
let z = cmath.log(-1 + 0i);
// z -> 0 + 3.141592653589793i (ln(-1) = iπ)

let z2 = cmath.log(100 + 0i, 10);
// z2 -> 2 + 0i
```

**cmath.log10(z)**
Return the base-10 logarithm of complex z.

- **Parameters:** `z` — A complex number.
- **Returns:** `Complex` — The base-10 logarithm of z.

```adesh
let z = cmath.log10(100 + 0i);
// z -> 2 + 0i
```

**cmath.sqrt(z)**
Return the square root of complex z.

- **Parameters:** `z` — A complex number.
- **Returns:** `Complex` — The square root of z.

```adesh
let z = cmath.sqrt(-1 + 0i);
// z -> 0 + 1i
```

---

### Conversion and Classification

**cmath.phase(z)**
Return the phase angle (argument) of complex z in radians.

- **Parameters:** `z` — A complex number.
- **Returns:** `float` — The phase angle in radians, in the range [-π, π].

```adesh
let phi = cmath.phase(1 + 1i);
// phi -> 0.7853981633974483 (π/4)

let phi2 = cmath.phase(-1 + 0i);
// phi2 -> 3.141592653589793 (π)
```

**cmath.polar(z)**
Return the polar representation of complex z as a tuple `(radius, phase)`.

- **Parameters:** `z` — A complex number.
- **Returns:** `Tuple(float, float)` — `(r, phi)` where r is the magnitude and phi is the phase angle in radians.

```adesh
let (r, phi) = cmath.polar(3 + 4i);
// r -> 5.0
// phi -> 0.9272952180016122
```

**cmath.rect(r, phi)**
Return the complex number from polar coordinates: r * (cos(phi) + i*sin(phi)).

- **Parameters:**
  - `r` — The magnitude (radius).
  - `phi` — The phase angle in radians.
- **Returns:** `Complex` — The complex number.

```adesh
let z = cmath.rect(5.0, 0.9272952180016122);
// z -> 3 + 4i
```

---

### Classification Functions

**cmath.isfinite(z)**
Return true if both the real and imaginary parts of z are finite.

- **Parameters:** `z` — A complex number.
- **Returns:** `bool` — True if z is finite.

```adesh
cmath.isfinite(3 + 4i)           // -> true
cmath.isfinite(cmath.infj)       // -> false
```

**cmath.isinf(z)**
Return true if either the real or imaginary part of z is infinite.

- **Parameters:** `z` — A complex number.
- **Returns:** `bool` — True if z is infinite.

```adesh
cmath.isinf(cmath.infj)          // -> true
cmath.isinf(3 + 4i)              // -> false
```

**cmath.isnan(z)**
Return true if either the real or imaginary part of z is NaN.

- **Parameters:** `z` — A complex number.
- **Returns:** `bool` — True if z is NaN.

```adesh
cmath.isnan(cmath.nanj)          // -> true
```

**cmath.isclose(a, b, [rel_tol], [abs_tol])**
Return true if complex a and b are close to each other.

- **Parameters:**
  - `a`, `b` — Complex numbers to compare.
  - `rel_tol` — Optional. Relative tolerance (default `1e-09`).
  - `abs_tol` — Optional. Absolute tolerance (default `0.0`).
- **Returns:** `bool` — True if a and b are close.

```adesh
cmath.isclose(1 + 1i, 1 + 1.000000001i)        // -> true
cmath.isclose(1 + 1i, 1 + 2i)                   // -> false
cmath.isclose(1 + 1i, 1 + 1.1i, 0.1)            // -> true
```


---

## Source: stdlib\thread-safety-audit.md

# Standard library thread-safety audit

Classification of existing modules. Locks were **not** blindly added.

| Module | Classification | Notes |
|--------|----------------|-------|
| Threading (`thread`) | THREAD-SAFE | Native OS primitives |
| Atomics | THREAD-SAFE, lock-free ops where CPU allows | |
| Parallel | THREAD-SAFE workers | Existing `Parallel` namespace |
| Time / clock | THREAD-SAFE for now/sleep | `sleepNanos` blocks caller |
| Math RNG | THREAD-CONFINED if using one seeded RNG | Prefer per-thread RNG |
| Collections (arrays/maps) | THREAD-CONFINED unless wrapped | Use Mutex/channels to share |
| IO / File | THREAD-CONFINED per handle unless OS allows concurrent ops | Do not claim full thread-safety |
| Path | immutable helpers, THREAD-SAFE if pure |
| HTTP / WebSocket | mixed; server workers exist | Document per-object; `Arc<Socket>` only if the socket impl allows concurrent ops |
| Net sockets | typically concurrent read+write on some OS; not all objects Sync |
| Process | process-global, use care |


---

## Source: stdlib\thread-safety-audit.md

# Standard library thread-safety audit

Classification of existing modules. Locks were **not** blindly added.

| Module | Classification | Notes |
|--------|----------------|-------|
| Threading (`thread`) | THREAD-SAFE | Native OS primitives |
| Atomics | THREAD-SAFE, lock-free ops where CPU allows | |
| Parallel | THREAD-SAFE workers | Existing `Parallel` namespace |
| Time / clock | THREAD-SAFE for now/sleep | `sleepNanos` blocks caller |
| Math RNG | THREAD-CONFINED if using one seeded RNG | Prefer per-thread RNG |
| Collections (arrays/maps) | THREAD-CONFINED unless wrapped | Use Mutex/channels to share |
| IO / File | THREAD-CONFINED per handle unless OS allows concurrent ops | Do not claim full thread-safety |
| Path | immutable helpers, THREAD-SAFE if pure |
| HTTP / WebSocket | mixed; server workers exist | Document per-object; `Arc<Socket>` only if the socket impl allows concurrent ops |
| Net sockets | typically concurrent read+write on some OS; not all objects Sync |
| Process | process-global, use care |
| Env | process-global mutation is racy without external sync |
| Crypto | pure functions THREAD-SAFE; contexts THREAD-CONFINED |
| Logging | print may interleave; no byte tearing of a single print in the runtime |
| Allocators | global allocator is synchronized; per-thread caches are optional future work on the existing allocator |

Rule: if it is not listed THREAD-SAFE, treat it as thread-confined or wrap it in `Mutex`/`channel`.

---

## Interactive Input & TUI Widgets Subsystem (`input.*`)

AdeshLang includes a comprehensive terminal interactive input and TUI widget library.

### Key Primitives & Widgets
- `input(prompt, options)` — Text prompt with regex validation, masking, and suggestions.
- `input.select(prompt, options)` — Single-choice interactive arrow selector.
- `input.checkbox(prompt, options)` — Multi-choice selection list (Space toggle, Enter confirm).
- `input.radio(prompt, options)` — Single-choice radio selector.
- `input.fuzzy(prompt, options)` — Real-time fuzzy-filtered search list.
- `input.datepicker(prompt)` — 3-field ASCII calendar datepicker (`YEAR`, `MONTH`, `DAY`, leap-year support).
- `input.datetime(prompt)` — 6-field timestamp picker (`YEAR`, `MONTH`, `DAY`, `HOUR`, `MIN`, `SEC`).
- `input.diff(orig, mod)` — Split-pane visual code diff editor with live cursor.
- `input.table(prompt?, headers, data)` — 2D data grid selector with `.row`, `.col`, `.value`, `.cell`, `.header`, `.rowData`, `.colData`, `.rowObject`, `.headers`, `.data`/`.grid` sub-properties.
- `input.pin(prompt, length?)` — PIN / OTP digit pad (`[ ● ] [ ● ] [ ● ] [ ● ]`) returning digits.
- `input.slider(prompt, options)` — Interactive horizontal gauge bar slider.
- `input.color(prompt)` — RGB color picker with real-time swatch preview.
- `input.hotkey(prompt)` — Keyboard shortcut / modifier combination listener.
- `input.mock(values)` — Mock queue for automated, headless unit testing.
