# Formatting Implementation Plan & Architecture

## Overview

This document specifies the unified formatting engine and implementation architecture for the Adesh programming language. The formatting subsystem provides standardized string interpolation and format specifiers (`${expr:spec}`) across all execution backends (Interpreter, Bytecode VM v1 & v2, Native Cranelift JIT, WASM MVP, and AOT compiler).

---

## 1. Portable Crate: `crates/adesh-format`

The formatting engine is encapsulated in a standalone, portable Rust crate (`crates/adesh-format`) with zero external execution dependencies.

### Core Types & Data Structures

- **`FormatValue`**: Uniform representation of printable runtime values:
  - `Int(i128)`
  - `Uint(u128)`
  - `Float(f64)`
  - `Bool(bool)`
  - `Str(String)`
  - `Char(char)`
  - `Null`
  - `Array(Vec<FormatValue>)`
  - `Object(Vec<(String, FormatValue)>)`

- **`FormatSpec`**: Parsed format specifier options:
  - `fill`: Optional padding character (default: `' '`).
  - `align`: Alignment (`'<'` left, `'>'` right, `'^'` center).
  - `sign`: Sign handling (`'+'` force sign, `'-'` sign for negative only, `' '` space for positive).
  - `alt_form`: Alternate display form flag (`#` prefix for radixes).
  - `zero_pad`: Zero-padding flag (`0`).
  - `width`: Minimum field width.
  - `precision`: Number of decimal places or string truncation length.
  - `spec_type`: Formatting type code (`'d'`, `'f'`, `'x'`, `'X'`, `'b'`, `'B'`, `'o'`, `'e'`, `'E'`, `'s'`).
  - `currency`: ISO currency code (`"USD"`, `"EUR"`, `"GBP"`, `"JPY"`, `"INR"`, `"CAD"`, `"AUD"`, `"CHF"`, `"CNY"`).
  - `case`: Text casing transformation (`"upper"`, `"lower"`).
  - `grouping`: Digit separator option (`','` thousands comma grouping).

- **`parse(spec_str: &str) -> Result<FormatSpec, String>`**:
  Parses format strings such as `>12.2f`, `05d`, `currency(USD)`, `#x`, `,` or `upper`.

- **`render(val: &FormatValue, spec: &FormatSpec) -> String`**:
  Applies alignment, width, padding, precision, radix conversions, currency symbols, and grouping to produce the final formatted string.

---

## 2. Workspace Integration & Runtime Adapters

`adesh-format` is integrated as a core workspace dependency in `Cargo.toml` and re-exported through runtime adapter layers:

1. **`crates/adesh-runtime/src/native_abi.rs`**:
   - Implements C ABI runtime bridge function `jit_format_value_typed(value: u64, val_type: i64, spec_ptr: u64) -> u64`.
   - Maps tag types (0=Int, 1=Float, 2=String, 3=Bool, 4=Handle) to `FormatValue` and executes formatting.

2. **`src/execution/runtime_core/format.rs`**:
   - Adapts dynamic `Value` / `RuntimeValue` types to `FormatValue` and delegates formatting to `adesh_format::FormatValue::render`.

3. **`src/backends/common/builtins_modules/io.rs`**:
   - Uses `adesh_format` for `std::io` string formatting routines.

---

## 3. Bytecode VM OpCode Integration (`OpCode::Format = 26`)

- Added `OpCode::Format = 26` to bytecode specification (`bytecode_old.rs`):
  - Emitted when lowering `ExprKind::Format(inner, spec)`.
  - Pops the value and format specifier string from the evaluation stack.
  - Executes formatting in VM v1 (`v1_stack.rs`) and VM v2.
  - Fully disassembles as `FORMAT` in bytecode disassemblers.

---

## 4. Parser Format-Spec Validation (`literals.rs`)

- The parser in `src/parsing/parser/literals.rs` validates template literal expressions `${expr:spec}`:
  - Calls `adesh_format::FormatSpec::parse` during AST parsing.
  - Ensures no trailing unparsed tokens remain after the specifier string (`!p.is_end()`).
  - Rejects malformed specifiers at compile time with descriptive syntax errors.

---

## 5. Backend Native JIT & WASM Lowering

1. **Native JIT (Cranelift)**:
   - Fixed float argument lowering for `jit_format_value_typed`: when type tag is `1` (Float), `f64` SSA values are bitcast to `types::I64` before being passed into the 64-bit integer FFI argument slot (`builder.ins().bitcast(types::I64, MemFlags::new(), f64_val)`).

2. **WASM Backend**:
   - Added `wasm_mutable_names` analysis pass in `src/backends/wasm/compiler.rs`.
   - Scans AST statement and function trees for variable assignments (`Assign`, `AssignOp`, `AssignTuple`, `AssignObject`).
   - Prevents stale compile-time constant folding of mutated variables in `var_values` so WASM generates proper runtime variable lookups.

---

## 6. Examples & Conformance Verification

### Comprehensive Examples (`examples/formatting/`)
- `basic.adesh`: Basic currency formatting.
- `numbers.adesh`: Precision, zero-padding, signs, and grouping.
- `radix_bits.adesh`: Hexadecimal (`x`/`X`), binary (`b`), octal (`o`), and prefix (`#`) formatting.
- `currency_all.adesh`: Full ISO currency support (USD, EUR, GBP, JPY, INR, CAD, AUD, CHF, CNY).
- `text_unicode.adesh`: Text alignment (`<`, `>`, `^`), custom fill padding, uppercase/lowercase.
- `report_table.adesh`: Tabular report layout formatting.
- `runtime_values.adesh`: Primitive and complex runtime value formatting.

### Automated Conformance Suite (`tests/formatting_conformance.rs`)
- Exact-output regression tests verifying formatting equivalence across:
  - `adesh-format` engine direct API.
  - Interpreter.
  - VM v1 & VM v2 bytecode execution.
  - Cranelift Native JIT execution.
  - WASM module compilation.
