# Object Key Order: Architecture Report and Migration Plan

Status: proposal for review. No refactoring has been done yet.

Goal: make insertion order the single object key-order contract in the
interpreter, native JIT (NJIT), AOT and WASM.

## 1. Current architecture

Each execution path stores objects in a different map type, so each one prints
keys in a different order. For `{ ...{a: 1, b: 2}, ...{c: 3, d: 4}, e: 5 }`:

| Backend | Object storage | Iteration order | Printed |
|---|---|---|---|
| Interpreter | `Value::Object(Arc<FxHashMap<String, Value>>)` (`src/parsing/ast.rs:12,1002`) | hash order | `{e: 5, d: 4, c: 3, b: 2, a: 1}` |
| WASM (static evaluation) | builds interpreter `Value` from `FastMap` (`src/backends/wasm/compiler.rs:2880-2900`) | hash order (same as interpreter) | same as interpreter |
| NJIT | common backend `RuntimeValue::Object(FastMap<String, RuntimeValue>)` (`src/backends/common/builtins/mod.rs:191`) | hash order, but a different insertion history (`src/backends/jit/native/runtime_bridge.rs:541-558`) | `{a: 1, d: 4, c: 3, b: 2, e: 5}` |
| AOT (linked runtime) | `adesh_runtime::RuntimeValue::Object(BTreeMap<String, RuntimeValue>)` (`crates/adesh-runtime/src/lib.rs:161`) | sorted | `{a: 1, b: 2, c: 3, d: 4, e: 5}` |

`FastMap` and `StringMap` are both `rustc_hash::FxHashMap` (`src/utils/collections.rs:8-11`).

Other object-like stores that cross these paths:

- `Value::Instance` fields and the property cache: `Arc<RwLock<FxHashMap<String, Value>>>` (`src/parsing/ast.rs:1670-1674`).
- Struct literals become `Value::Object` with a hidden `__struct` key (`src/execution/runtime_core/interpreter_core.rs:15649-15686`).
- Secondary representations: `typesystem/value_optimized.rs:63,219-223` (`StringMap`), `execution/vm/values.rs:23` (`std::HashMap`), `runtime/nanvalue.rs:431` (`std::HashMap`), and `backends/common/builtins_modules/types.rs:40` (`FastMap`).

### How values cross boundaries

- Compiled code refers to AOT objects only through opaque `u64` handles, plus
  key/value word arrays for `aot_make_object` (`crates/adesh-runtime/src/lib.rs:1471-1499`).
  No Rust map layout crosses the FFI, so changing the runtime map type does not change any ABI.
- Interpreter `Value` and the common `RuntimeValue` convert entry by entry in
  `src/backends/common/builtins/stdlib_bridge.rs:38-85`,
  `src/backends/jit/native/runtime_bridge.rs:1232-1337,1971-2006,2935-2991`
  and `src/backends/aot/runtime_bridge.rs:47-68,188-290`.
  Every one of these rebuilds a map. If either side is unordered, the order is lost.
- There is no direct conversion between interpreter `Value` and the linked
  `adesh_runtime::RuntimeValue`.

### Where order is observable

| Operation | Interpreter | AOT runtime | NJIT |
|---|---|---|---|
| Print / `toString` | `ast.rs:1144-1162`, `format.rs:69-80` | `lib.rs:499-501` | via `fmt` (`runtime_bridge.rs:1261-1315`), `format_object` `:1138-1171` |
| Pretty print | **sorts** keys: `pretty_print.rs:734-749` | **sorts** keys: `lib.rs:1397-1412` | uses the interpreter pretty printer (sorts) |
| Format specifiers | `format.rs:329-332` → `FormatValue::Object(Vec<..>)` (keeps incoming order) | `native_abi.rs:205-208` | via interpreter |
| JSON | `format.rs:195-211` → `serde_json::Map` (**sorted**, because `preserve_order` is off) | none | `backends/common/builtins/json.rs:10-57,113-145` (sorted) |
| keys/values/entries | `interpreter_impl/builtins/associated.rs:192-208` | `lib.rs:4184-4227` | common builtins |
| `for k in obj` | `interpreter_core.rs:7400-7402,20214`, `exec/stmt.rs:1069-1070` | lowering + `keys` | runtime bridge |
| Spread | `interpreter_core.rs:15623-15647` | `aot_object_spread` `lib.rs:1782-1794` | `runtime_bridge.rs:541-558` |
| Equality | order-independent (`ops.rs:373-378`) | `BTreeMap ==` (order-independent) | order-independent |
| Destructuring | keyed lookup, order-independent | — | — |

## 2. Affected files and usages

- `Value::Object(` appears 817 times in 103 files. Of these, 594 are the
  interpreter variant itself, in 79 files. The rest are `RuntimeValue::Object`,
  `VMValue::Object` and similar.
- Only **about 311 sites construct** an interpreter object (`Arc::new(map)` 280 times, `.into()` 31 times).
  The rest pattern-match or clone an existing `Arc`, and those keep compiling
  when the map type changes, because the operations they use (`get`, `insert`,
  `iter`, `keys`, `values`, `len`, `contains_key`, `extend`, `Arc::make_mut`)
  have the same names on the replacement type.
- The construction sites build their maps with `FastMap::default()` (163),
  `HashMap::default()` through local Fx aliases (108), `FxHashMap::default()`
  (5), `std::collections::HashMap::default()` (7),
  `HashMap::with_capacity_and_hasher` (2), and typed parameters or clones (26).
  Changing only the alias in `ast.rs` would therefore not compile. Each builder
  has to name the new type.
- Files with the most construction sites: `runtime/stdlib_src/http/api.rs` (48),
  `execution/runtime_core/interpreter_core.rs` (48), `runtime/stdlib_src/atp/api.rs` (18),
  `runtime/stdlib_src/http/websocket/api.rs` (16), `runtime/stdlib_src/dns/api.rs` (15),
  `runtime/stdlib_src/system/args.rs` (12), `runtime/stdlib_src/url/api.rs` (10).
- No code removes keys from the `Value::Object` payload. Two removals operate on
  instance fields: `execution/runtime_core/builtins.rs:319` and
  `interpreter_core.rs:4937`. If instance fields migrate too, these must become
  `shift_remove` (`IndexMap::remove` is deprecated and would add warnings).
- AOT runtime: object code exists only in `crates/adesh-runtime/src/lib.rs` and `native_abi.rs`
  (about 20 functions, listed in section 5.3).

## 3. Proposed representation

One ordered map type for every object store, defined once per crate:

```rust
// src/utils/collections.rs (interpreter, NJIT, WASM, common RuntimeValue)
pub type ObjectMap<V> = indexmap::IndexMap<String, V, FxBuildHasher>;
```

```rust
// crates/adesh-runtime (linked AOT runtime)
type ObjectMap = indexmap::IndexMap<String, RuntimeValue, BuildHasherDefault<FxHasher>>;
```

Why `IndexMap` with the existing Fx hasher:

- It keeps insertion order. Updating an existing key keeps its slot.
  `shift_remove` removes a key and closes the gap, and re-inserting that key
  appends it at the end. These are exactly the semantic-contract rules.
- Lookups are still hash lookups with the same hasher, so property access does
  not slow down (see section 7).
- `IndexMap`'s `PartialEq` ignores order, so equality stays order-independent
  without extra code.
- Clone-on-write through `Arc::make_mut` works unchanged.
- `indexmap 2.13` is already in `Cargo.lock` through transitive dependencies,
  so this adds a direct dependency but no new crate to the build.
- For the linked runtime, `indexmap` and `rustc-hash` are pure Rust and make no
  OS calls. That matters because the runtime is linked by adeshlink without
  libstd's usual support. Do **not** use `RandomState`: it needs the OS RNG.
  This still has to be verified by an actual custom link (section 6).

Rejected alternatives:

- Copying the interpreter's hash order into AOT/NJIT. The order would be
  arbitrary, and it would break whenever hashbrown changes or a map's insertion
  history differs.
- Changing `FastMap` globally. `FastMap` is also used for environments, caches
  and symbol tables, which need no order and should keep the cheaper type.
- Making `serde_json` `preserve_order` the whole fix. It covers JSON only, and it
  switches `serde_json::Map` for every crate in the build (Cargo feature
  unification), which can change unrelated output such as lockfiles and snapshots.
- A custom `Vec<(String, V)>` with linear search. It is faster for 1–4 keys and
  much slower for large objects, and it would be one more data structure to maintain.

## 4. Semantic contract (to document in `docs/language-reference.md`)

1. A new property is appended at the end.
2. Updating an existing property keeps its position.
3. Deleting a property removes its position (`shift_remove`).
4. Re-inserting a deleted property appends it at the end.
5. Iteration (`for in`, `keys`, `values`, `entries`) follows insertion order.
6. Printing, both plain and pretty, follows insertion order. Pretty printers stop sorting keys.
7. `JSON.stringify` follows insertion order. `JSON.parse` keeps source order.
8. Object equality stays order-independent.
9. Spread follows rule 1/2 per key: `{ ...a, k: v }` keeps `a`'s order, and `k`
   either keeps its slot (if `a` had it) or is appended.
10. Hidden keys (`__class__`, `__struct`) are filtered by name, as today. They
    take a slot but are never printed or enumerated.

Note: in the current language there is no `delete` operation on plain objects
in the interpreter or AOT. Rules 3 and 4 govern the existing instance-field
removals and any future `delete`.

## 5. Migration plan

Each step builds, passes tests and lands on its own.

### 5.1 Foundation (small)
- Add `indexmap = "2"` to the root `Cargo.toml`. Add `ObjectMap`, plus an
  `object_map_with_capacity(n)` helper, to `src/utils/collections.rs`.
- Add `tests/object_key_order.rs` and cross-backend `.adesh` fixtures (section 6).
  At first, run only the interpreter, and mark the expectations that are not
  yet met with `#[ignore]` plus a comment pointing at this plan.

### 5.2 Interpreter and WASM (the largest step, mostly mechanical)
- Change `Value::Object` to `Arc<ObjectMap<Value>>`. Fix the roughly 311
  construction sites, guided by compiler errors: replace `FastMap::default()` /
  `HashMap::default()` with `ObjectMap::default()`, and update the 2
  typed-parameter functions (`core/regex.rs:61-65`, `concurrency/helpers.rs:213-215`).
  Pattern-matching sites need no change.
- Object spread (`interpreter_core.rs:15623-15647`) already inserts in order, so
  it becomes correct automatically. `Exec::eval_object`
  (`exec/expression_eval/literals.rs:76-81`) does not handle `"..."` keys at
  all. Fix that in the same step.
- Stop sorting in `execution/runtime_core/pretty_print.rs:734-749`.
- WASM static evaluation already produces interpreter `Value`. Update its map
  builders (`wasm/compiler.rs:2880-2900`, `wasm/static_calls.rs`) to `ObjectMap`.
- Keep `value_optimized.rs`, `vm/values.rs` and `nanvalue.rs` unchanged in this
  step, and convert at their boundaries. They are migrated in 5.5.

### 5.3 Common backend `RuntimeValue` (NJIT, in-process AOT bridge)
- Change `Object(FastMap<..>)` to `Object(ObjectMap<..>)` in
  `backends/common/builtins/mod.rs:191` and `builtins_modules/types.rs:40`.
- Update the conversion functions in `stdlib_bridge.rs`, `jit/native/runtime_bridge.rs`
  and `aot/runtime_bridge.rs` so they build `ObjectMap` and copy entries in order.
- Fix NJIT spread (`runtime_bridge.rs:541-558`) so it follows contract rule 9.

### 5.4 Linked AOT runtime (`crates/adesh-runtime`)
- Add `indexmap` (with `default-features = false`) and `rustc-hash` to `crates/adesh-runtime/Cargo.toml`.
- Change `RuntimeValue::Object(BTreeMap<..>)` to `Object(ObjectMap)`. Keep the
  separate `RuntimeValue::BTreeMap` variant, the handle table (`aot_free_range`
  needs ordered range lookup) and the `CLASS_PARENTS` table as `BTreeMap`.
- Functions to update: `aot_make_object`, `aot_object_spread`, `aot_set_field(_str)`,
  `aot_get_field(_str)`, `aot_get_index(_handle)`, `aot_set_index_impl`,
  `aot_collections_new("HashMap")`, the `keys`/`values`/`entries`/`get`/`set`
  branches of `aot_call_method_impl` (`lib.rs:4184-4227`), `aot_make_dict`,
  `aot_make_error`, `aot_set_class_tag`, and the `PartialEq` impl (`lib.rs:262-274`).
- Remove the key sort in the pretty printer (`lib.rs:1397-1412`).
- Filter `CLASS_TAG_KEY` in `native_abi.rs:205-208`. Today it is not filtered
  there, so a format specifier could print the hidden key.
- Rebuild with `cargo build -p adesh-runtime --target x86_64-pc-windows-msvc`.
  Run the AOT end-to-end tests and check that adeshlink reports no new
  unresolved symbols.

### 5.5 JSON and remaining stores
- JSON: write `value_to_json` / `runtime_to_serde_json` output directly from
  `ObjectMap` with a small streaming serializer, or enable
  `serde_json/preserve_order` after confirming that no other user depends on
  sorted JSON. The serializer is the lower-risk option. Do the same for parsing.
- Instance fields: move `UserInstance.fields` to `ObjectMap`, so printing an
  instance shows fields in declaration order. Change the two `remove` calls to
  `shift_remove`. `prop_cache` stays `FastMap`.
- Migrate `value_optimized.rs`, `vm/values.rs` and `nanvalue.rs`, or delete them
  if they are unused. This needs a separate check.

## 6. Regression test plan

Add one set of `.adesh` programs under `tests/fixtures/object_order/`. Run each
program on the interpreter, `--jit-native`, `--aot` and `--wasm`, and require
byte-identical stdout against a checked-in expected file. Each program targets
one area:

1. Insertion sequences: `{b, a, c}` prints `b, a, c`, and objects built by
   repeated property assignment keep that order.
2. Updates: `o.a = 9` keeps `a`'s position. Compound assignment (`o.a += 1`) does too.
3. Delete and re-insert: instance-field removal, plus a future `delete`
   (`#[ignore]` until that exists).
4. Nested objects, and arrays of objects.
5. Spread and merge: every case in `examples/syntax/test_spread_comprehensive.adesh`,
   plus override inside a spread and a spread of an empty object.
6. Objects returned from functions and closures, and objects passed into a
   function and mutated there.
7. Boundary crossings: stdlib calls that go through `stdlib_bridge`, NJIT↔interpreter
   fallbacks, and AOT handles round-tripped through `aot_get_index_handle`.
8. `for k in obj`, `keys`, `values`, `entries`.
9. Printing: plain, pretty, format specifiers, string interpolation, `JSON.stringify`,
   and a `JSON.parse` → stringify round trip.
10. Equality: `{a: 1, b: 2} == {b: 2, a: 1}` is `true` on every backend.

Also add Rust unit tests for `ObjectMap` behavior (rules 1–4) and for each
conversion function (order preserved both ways). Then update
`examples/syntax/test_spread_comprehensive.adesh` comments, which already
describe insertion order.

## 7. Performance

Micro-benchmark: `.scratch/objbench` (release build, best of 5, ns per
operation; each "create" allocates the `String` keys). The table shows the range
over two runs. "Update all" clones the map and then sets every key; "delete one"
clones, removes one key and iterates.

| Keys | Operation | FxHashMap (now: interpreter/NJIT) | IndexMap&lt;Fx&gt; (proposed) | BTreeMap (now: AOT) |
|---:|---|---:|---:|---:|
| 4 | create | 333–335 | 485 | 351–382 |
| 4 | lookup all | 36–51 | 40–50 | 69–74 |
| 4 | update all | 453–532 | 576–593 | 484–500 |
| 4 | clone | 239–252 | 290–307 | 329–441 |
| 16 | create | 1725–1918 | 1961–2064 | 2405–2953 |
| 16 | lookup all | 162–168 | 154–231 | 560–692 |
| 16 | clone | 1113–1156 | 2058–2326 | 1670–1940 |
| 64 | create | 4727–5195 | 9300–10921 | 10596–14092 |
| 64 | lookup all | 720–1131 | 463–532 | 3064–4497 |
| 64 | update all | 10238–11692 | 7293–8819 | 9949–10133 |
| 1024 | create | 181k–207k | 100k–172k | 241k–301k |
| 1024 | lookup all | 14.5k–16.5k | 9.2k–12.4k | 104k–124k |
| 1024 | delete one | 79k–126k | 62k–76k (`shift_remove`) | 97k–133k |
| all | iterate | 3–1056 | 1–1217 | 4–1584 |

What this means:

- **Interpreter and NJIT:** lookups and iteration are the same or faster. The
  cost is in creating small objects (about +45% at 4 keys, up to about 2× at 64
  keys) and cloning 16-key objects (about 1.8×). Copy-on-write
  (`Arc::make_mut`) clones objects, so the plan includes a before/after run of
  `benches/` and the object-heavy examples. If small-object creation turns out
  to matter, a known follow-up is a capacity hint at literal sites
  (`object_map_with_capacity(entries.len())`).
- **AOT:** `IndexMap` is faster than the current `BTreeMap` in nearly every
  column (lookups 1.5–10×), so the AOT change should be a speed-up.
- These are micro-benchmarks. The decision should rest on the end-to-end
  numbers measured after step 5.2.

## 8. Risks and compatibility

- **Output changes.** Every program that prints an object with two or more keys
  will print in a different order. That is the point, but snapshot tests, golden
  files and docs that captured the current hash order or sorted order need
  updating. Pretty print and AOT currently sort, so code relying on sorted
  output will change.
- **JSON order.** Fixing storage alone does not change JSON output (section 5.5).
  Users may currently rely on sorted JSON.
- **The linked runtime.** New dependencies must link under adeshlink without
  libstd support. Verify with an actual AOT end-to-end link, not only
  `cargo check`.
- **Hidden keys** keep their slots. The filtering must stay by name; one
  formatter is missing it today (`native_abi.rs:205-208`).
- **Memory.** `IndexMap` stores entries plus an index table, about 8–16 more
  bytes per entry than `FxHashMap`.
- **Large mechanical diff in 5.2.** About 311 sites across about 80 files. Keep
  it a pure type change (no behavior change other than order), so review is
  "does it compile, and do the order tests pass".
- **Secondary value representations** (`value_optimized`, `vm`, `nanvalue`) will
  silently lose order at their boundaries until 5.5.
