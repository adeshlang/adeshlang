# Adesh Formatting: Semantics and Implementation Plan

Adesh template strings use backticks and `${expression}` interpolation:

```adesh
let name = "Ada";
print(`Hello, ${name}!`);
```

An interpolation may add a format specification after a top-level colon:
`${expression:spec}`. Colons inside strings, parentheses, object literals, and
ternary expressions remain part of the expression.

## Contract

Formatting is deterministic and backend-independent. Every execution target
must evaluate the expression once, convert the resulting Adesh value, apply
the same specification, and return a string. Unknown or inapplicable type
conversions preserve the normal value representation rather than changing the
value or crashing the process.

The current specification is:

| Form | Meaning |
| --- | --- |
| `${x}` | Default value conversion |
| `${x:<12}`, `${x:^12}`, `${x:>12}` | Alignment and minimum display width |
| `${x:0>8}`, `${x:08}` | Fill/alignment or zero padding |
| `${x:+}` | Explicit plus sign for non-negative numeric values |
| `${x:int}`, `${x:d}` | Integer conversion |
| `${x:bin}`, `${x:hex}`, `${x:HEX}`, `${x:oct}` | Binary, hexadecimal, or octal |
| `${x:float(2)}` | Fixed decimal precision |
| `${x:e}`, `${x:E}` | Scientific notation |
| `${x:%}` | Percentage (`1.0` becomes `100.0%`) |
| `${x:currency(INR)}` | Currency symbol, grouping, and currency precision |
| `${x:upper}`, `${x:lower}` | Case conversion for text |
| `${x:quoted}` | Quoted/debug-style text |

Specifications may combine conversion, sign, and width, for example
`${amount:>14+currency(USD)}`. Width is measured using Unicode display width,
not UTF-8 byte length, so alignment is stable for non-ASCII text.

## Use-case catalog

* **User-facing messages:** names, status values, and localized text.
* **Tables and reports:** left/right/center alignment and fixed columns.
* **Identifiers and protocol fields:** zero-padded sequence numbers.
* **Diagnostics:** hexadecimal addresses, binary flags, quoted strings, and
  scientific measurements.
* **Finance:** grouped currency output with explicit currency codes.
* **Metrics:** percentages, decimal precision, and scientific notation.
* **Serialization helpers:** composing URLs, paths, log lines, and command
  previews without manual concatenation.
* **Unicode interfaces:** aligned labels containing symbols or international
  text.

## Runtime implementation plan

1. **Frontend:** keep the existing template lexer and top-level-colon parser;
   add diagnostics for malformed interpolation boundaries and eventually make
   invalid specifications a typed formatting error.
2. **Semantic authority:** keep conversion and padding rules in
   `src/execution/runtime_core/format.rs`, with unit tests for every
   conversion family, signs, combinations, Unicode width, and fixed-width
   numeric values.
3. **Interpreter and VM:** evaluate `Format` nodes through the semantic
   authority. VM v1 and v2 must share the same operation and golden outputs.
4. **JIT/native JIT:** pass typed values through the existing bridge and call
   the semantic authority; never duplicate parsing in code generation.
5. **AOT/native:** use the AOT bridge for boxed values and keep the standalone
   `adesh-runtime` ABI behaviorally equivalent, including currency and
   precision.
6. **WASM:** constant-fold formats through the same authority and use a single
   runtime helper for dynamic templates when dynamic string support is enabled.
7. **Conformance:** maintain one source corpus and run it through interpreter,
   VM v1/v2, JIT, native JIT, AOT/native, and WASM compile/run paths. Compare
   exact UTF-8 output and error behavior.
8. **Performance:** cache parsed immutable specifications in JIT/AOT execution
   where profiling demonstrates a benefit; preserve the same observable
   semantics.

## Recommended future extensions

These are deliberately staged after parity: locale-aware grouping and decimal
symbols, named/custom format providers, date/time formats, collection
pretty-print limits, and user-defined `Display`/`Debug` implementations. Each
extension must specify locale, rounding, sign, width, and error behavior before
being added to a backend.
