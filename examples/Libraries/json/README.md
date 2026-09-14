# AdeshLang JSON Library Examples

AdeshLang provides a **built-in, language-level JSON system** — no `import JSON;` required anywhere.

## Files in this directory

| File | What it demonstrates |
| :--- | :--- |
| `basics.adesh` | `JSON.parse`, `JSON.stringify`, `JSON.stringifyPretty`, nested access |
| `validation.adesh` | `JSON.isValid`, `JSON.tryParse`, guarded parsing pattern |
| `formatting.adesh` | `JSON.minify`, `JSON.pretty`, `JSON.stringifyCompact` |
| `builders.adesh` | `JSON.object()`, `JSON.array()`, constructing values programmatically |
| `file_io.adesh` | `JSON.parseFile`, `JSON.stringifyFile`, disk round-trip |
| `bytes.adesh` | `JSON.stringifyBytes`, `JSON.parseBytes`, byte-array round-trip |
| `error_handling.adesh` | Parse-error diagnostics, depth-limit guard, safe fallback pattern |
| `all.adesh` | Full kitchen-sink demo combining every API |

## Quick Reference

```adesh
// Parse — no import needed
let obj = JSON.parse("{\"key\": \"value\"}");

// Stringify
let compact = JSON.stringify(obj);
let pretty  = JSON.stringifyPretty(obj);

// Validation / safe parse
if JSON.isValid(text) { ... }
let safe = JSON.tryParse(text);   // returns null on error

// Minify / pretty-format an unformatted string
let min = JSON.minify(json_str);
let fmt = JSON.pretty(min);

// Build objects / arrays in code
let o = JSON.object();
o.name = "hello";
let a = JSON.array();
push(a, 1); push(a, 2);

// File I/O
JSON.stringifyFile("out.json", obj, true);  // true = pretty
let back = JSON.parseFile("out.json");

// Byte round-trip
let bytes    = JSON.stringifyBytes(obj);
let restored = JSON.parseBytes(bytes);
```

## Running Examples

```sh
cargo run --bin adeshlang -- examples/Libraries/json/basics.adesh
cargo run --bin adeshlang -- examples/Libraries/json/all.adesh
```
