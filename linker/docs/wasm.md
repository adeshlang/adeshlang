# WebAssembly Backend (`wasm`)

**Status: multi-object linking works, including function pointers, and the
modules run on two engines.** `adeshlink` links relocatable WASM objects — the
ones clang/lld emit, with a `linking` custom section and `reloc.*` fixups — into
one module that V8 (Node) and Cranelift (wasmtime) both validate and execute in
the test suite. Host imports are routed, calls and data references that resolve
inside the link are rewritten to the definition, function/global/data indices
are renumbered, `call_indirect` is given a table the linker defines and fills
from the inputs' element segments, and export names from the object's export
section are preserved. All of these suites run in the ubuntu CI job (see §3).

Anything the linker cannot represent is still a loud error (`LNK001`/`LNK002`/
`LNK007`/`LNK008`/`LNK017`); nothing is silently dropped or emitted unrelocated.

```console
$ adeshlink --target wasm32-wasi hello.o -o hello.wasm
$ node -e 'const m=new WebAssembly.Module(require("fs").readFileSync("hello.wasm")); console.log(WebAssembly.Module.exports(m).map(e=>e.name))'
[ '_start', 'memory' ]
```

Two objects that only reference each other need no host at all, which is what
makes the output usable as a standalone module:

```console
$ adeshlink --target wasm32-wasi add.o twice.o -o add.wasm
$ node -e 'const m=new WebAssembly.Module(require("fs").readFileSync("add.wasm")); console.log(WebAssembly.Module.imports(m), new WebAssembly.Instance(m,{}).exports.add(20,22))'
[] 62
```

---

## 1. Reading objects

`linker/src/wasm/sections.rs` reads both plain modules and relocatable objects.
Modules and objects share version 1 (LLVM 19 writes `01 00 00 00` for both), so
an object is detected by its `linking` custom section rather than by the version
field; any other version is rejected loudly.

From an object the reader keeps:

- **Sections** — `CODE` becomes `.text` (read/exec/alloc), `DATA` becomes
  `.data` (read/write/alloc), `reloc.<section>` becomes `.reloc.<section>` of
  kind `Reloc`, and all metadata (`type`, `import`, `function`, `global`,
  `export`, `producers`, `linking`, …) becomes a non-allocatable `Custom`
  section.
- **Symbols** — the `linking` symbol table (subsection 8): defined
  function/global/table symbols, data symbols with their segment offset and
  size, section symbols, and undefined symbols, whose names are recovered from
  the import section (or from `WASM_SYM_EXPLICIT_NAME` when clang remaps an
  import). Function indices are translated out of the index space that imported
  functions prefix into code-payload offsets. `WASM_SYM_ABSOLUTE` and
  `WASM_SYM_TLS` symbols are rejected rather than guessed at.
- **Relocations** — `reloc.*` targets an indexed section, so entries land on
  the section they patch, with LLVM's `R_WASM_*` numbering mapped onto
  `RelocationKind::{WasmFunctionIndex, WasmMemoryOffset32/Sleb/I32,
  WasmTypeIndex, WasmTableIndexSleb/I32, WasmTableNumber, WasmGlobalIndex,
  WasmGlobalIndexI32}`. Unimplemented types (64-bit forms) are a hard error,
  never a dropped fixup.
- **Element segments** — the `element` section is parsed into
  `WasmElementSegment { table_offset, functions }`. Only the MVP encoding (flags
  `0`, one active segment for table 0) is accepted, which is what clang emits
  for address-taken functions; passive, declarative and typed segments are
  refused with `LNK017`. Each entry is the object's own function index —
  imported functions first — and a cross-object entry names an undefined
  function import, which the plan resolves by name.
- **Export section** — parsed into `WasmObjectExport` facts, because an
  `export_name("...")` lives only there: the symbol table keeps the C name while
  the export entry carries the host-visible one.
- **Link facts** (`linker/src/wasm/object_info.rs`) — function signatures, the
  type index of every defined body, the import descriptors, each body's
  byte range in `CODE`, each data segment's payload range, the element
  segments, and the object's exports. A module writer cannot rebuild these from
  merged sections, so they are captured at read time (`ObjectFile::wasm`).

`linker/tests/wasm_object_reader_test.rs` asserts all of this against a
hand-built object and against clang output embedded as a fixture, using
`llvm-readobj --symbols --relocations` as the oracle. It also pins that an
element segment keeps the object's own function indices (imports first) and
that a passive segment or a non-`i32.const` table offset is a loud
`LNK017`.

---

## 2. The link plan

`WasmLinkInfo` (`linker/src/wasm/link_info.rs`) is the single place where every
index is decided. It is built from the parsed objects, not from merged sections:
layout concatenates `CODE` payloads and folds the type/function tables into
`.rodata`, neither of which is a valid section to re-emit.

The plan produces:

| Field | Decision |
| --- | --- |
| `types` | signatures deduplicated by `(params, results)`, first-use order |
| `imports` | host **functions** deduplicated by `(module, name)`, in function-index order before every defined body |
| `functions` | each defined body with its interned type index and the object/body range to copy |
| `data_segments` | payload ranges placed sequentially from `DATA_BASE` (1024) on an 8-byte boundary |
| `exports` | functions flagged exported in the symbol table, named on the CLI, or all of them with `export_all`, **plus** every function the object's export section names — which also renames a same-index export (`export_name`) |
| entry | the requested entry is **exported**, never placed in a start section (see below) |
| `stack_top` | `align16(data_end) + 64 KiB` |
| `initial_pages` | enough pages for the stack, never fewer than an imported memory's minimum |
| `table_min` | at least the imported table minimum, and at least `table_entries.len() + 1` when the link fills a table |
| `table_entries` | every address-taken function, in element-segment order across all inputs, deduplicated into final function indices; slot 0 is left free so calling a null function pointer traps |
| `data_count_required` | set when any input had a data-count section (bulk-memory code) |

The entry point is exported rather than started, matching `wasm-ld`: a module
that both starts `_start` and exports it runs the program twice when the host
calls the export. The entry must have type `[] -> []`, since the host calls it
with no arguments and ignores any result.

The plan also collects **definitions**: every non-local function and data symbol
one of the inputs defines, with its signature. That table is the only place able
to tell the two kinds of import apart, because a relocatable object writes a
reference to a sibling's function exactly as it writes a host import — an `env`
entry plus a `R_WASM_FUNCTION_INDEX_LEB`. When a name is in the table, the plan
drops the import and the reference binds to the definition instead; when it is
not, the host has to supply it.

A table entry is resolved the same way: an object lists its element-segment
entries in its own numbering (imports first), so an entry that names an
undefined function import is looked up in the definitions by name and its body
becomes a slot. A host function can never take a slot — it has no body to place
— and that is a `LNK001`.

Anything the writer cannot represent is a **loud error** instead of a dropped
section:

| Input | Error |
| --- | --- |
| two inputs define one strong name (a call binds by name, so the link cannot say which body runs) | `LNK002` |
| a call whose signature disagrees with the definition it binds to | `LNK007` |
| the address of data no input defines and no host supplies | `LNK001` |
| a table entry naming a function no input defines (a host import has no body) | `LNK001` |
| a non-`__stack_pointer` global import, or any tag import | `LNK017` |
| defined `global`/`table`/`start` sections, or a defined memory | `LNK017` |
| an element segment that is passive, declarative, typed or for another table | `LNK017` |
| a memory exported under a name other than `memory`, or a table/global/tag export | `LNK017` |
| an input without link facts (e.g. an ADOB `.o`) | `LNK017` |
| a start function that is not `[] -> []` | `LNK006` |

A weak definition loses to a strong one whichever order the inputs arrive in;
two weak definitions are resolved by first input.

`linker/tests/wasm_link_info_test.rs` pins every row of both tables.

---

## 3. Relocation application

`linker/src/wasm/relocations.rs` (`apply`) walks every object's `.text`/`.data`
relocations and patches the padded placeholder in place; the writer then copies
the patched payloads verbatim. Each entry is resolved against the plan:

| Relocation | Becomes |
| --- | --- |
| `WASM_FUNCTION_INDEX_LEB` on an import the host must supply | the plan index of that `(module, name)` import |
| `WASM_FUNCTION_INDEX_LEB` on a name another input defines | `imports.len() + body ordinal` of the definition, in the same link |
| `WASM_FUNCTION_INDEX_LEB` on a defined function | `imports.len() + body ordinal` |
| `WASM_GLOBAL_INDEX_LEB` / `_I32` on `__stack_pointer` | global 0 (the only global the linker emits) |
| `WASM_MEMORY_ADDR_LEB` / `_SLEB` / `_I32` on a defined symbol | `DATA_BASE`-relative address of that object's data + addend |
| `WASM_MEMORY_ADDR_LEB` / `_SLEB` / `_I32` on a name another input defines | address of the definition's placed segment + addend |
| `WASM_TABLE_INDEX_SLEB` / `_I32` on a defined function | the function's slot in the plan's `table_entries` (1-based; slot 0 stays null) |
| `WASM_TABLE_INDEX_SLEB` / `_I32` on a name another input defines | the definition's slot, found through the table |
| `WASM_TABLE_NUMBER_LEB` on `__indirect_function_table` | `0` — the linker emits one table, whatever the objects numbered it |
| `WASM_TYPE_INDEX_LEB` on a callee | the plan's interned type index for that function's signature |

Encodings follow what the object reserved: LEB/SLEB keep the on-disk placeholder
width (LLVM pads to five bytes) and fail loudly when a value does not fit; `I32`
kinds patch a fixed four-byte little-endian word. A relocation against a symbol
missing from the plan, an address that lands outside every placed segment, or a
32-bit overflow is a hard error.

`linker/tests/wasm_relocations_test.rs` asserts each row at the byte level, and
five suites link real clang objects and execute the results:

| Suite | Objects | Engine | WASI host |
| --- | --- | --- | --- |
| `linker/tests/wasm_hello_world_test.rs` | one (plain, and `export_name("_start")`) | V8, through `node` | a `fd_write` stub in the test's Node script |
| `linker/tests/wasm_cross_object_test.rs` | two (`_start`→`greet`, and a user of another object's data) | V8, through `node` | the same `fd_write` stub; none for the data pair |
| `linker/tests/wasm_pure_module_test.rs` | two (`add`→`twice`) | V8, through `node` | none — instantiated with `{}` |
| `linker/tests/wasm_table_test.rs` | one (`call_indirect` inside one object, and function pointers stored in data) and two (caller→callee through the table) | V8, through `node` | none — instantiated with `{}` |
| `tests/wasm_wasmtime_execution_test.rs` (root crate) | all of the above | Cranelift, through the workspace's `wasmtime` 36 dependency | a `wasi_snapshot_preview1.fd_write` implemented with `Linker::func_wrap`; an empty `Linker` for everything hostless |

Every hello-world case asserts that exactly `hello from adeshlink\n` reaches
stdout once; the cross-object call case also proves the callee's own host import
survives the link, and the data pair asserts that a read (`tag[0]` is `A`) and a
write (`counter++` twice from 7) land in the defining object's storage. The pure
pair asserts the module imports nothing at all — in Rust, through
`Module::imports`, and by instantiating it with no host.

The table cases are the same contract for function pointers: the module imports
nothing, exports only its function and `memory`, and dispatch through the
table returns `apply(0,7) = 49`, `apply(1,3) = 27` for a `pick` that chooses
between `sq` and `cu`, plus the data pair of table addresses (`call(0,6) = 36`,
`call(1,4) = 64`) and the cross-object caller whose element entries name
another object's function.

All use the clang fixtures in `linker/tests/common/wasm_fixtures.rs`, which
record each object's C source and exact command line, so the byte stream under
test is identical across engines. The ubuntu CI job runs every WASM suite by
name (the linker suites are not covered by the root-level `cargo test --tests`).

The wasmtime suite is a library test rather than a `wasmtime run` of the CLI:
wasmtime 33 dropped WASI preview1, so no runner-installed CLI could satisfy a
`wasi_snapshot_preview1` import — the host has to be part of the test.

---

## 4. The module writer

`WasmWriter::encode_binary` emits the sections the plan describes, in spec
order, and copies bodies and data payloads byte-for-byte out of their input
objects:

1. **Type (1)** — `0x60`, param vector, result vector.
2. **Import (2)** — `module`, `name`, kind `0x00`, type index.
3. **Function (3)** — type index per defined body.
4. **Table (4)** — only when `table_min > 0`; one `funcref` table, minimum only.
5. **Memory (5)** — always emitted, minimum only (the heap grows with
   `memory.grow`).
6. **Global (6)** — one mutable `i32` `__stack_pointer` initialised to
   `i32.const stack_top`.
7. **Export (7)** — every planned function plus `memory`.
8. **Element (9)** — one active segment at slot 1 of table 0, holding
   `table_entries`; only emitted when the link fills a table.
9. **Data count (12)** — before `CODE`, only for bulk-memory inputs.
10. **Code (10)** — each body re-framed with its own size LEB.
11. **Data (11)** — active segment 0 at the planned offset; segment order is
    preserved because code can name a segment by index.

No start section is emitted: the entry is an export (see §2).

The table the planner defines is what lets a `call_indirect` work with no host
at all: an object's `env.__indirect_function_table` import is dropped, every
`R_WASM_TABLE_NUMBER_LEB` becomes table 0, the object's element entries are
re-based into the output table, and the `R_WASM_TABLE_INDEX_*` fixups point at
those slots. For the fixture that stores function pointers in data, `wasm-ld
--no-entry vtab.o` is the oracle — table minimum 3, one element segment at
offset 1 with entries `[1, 2]`, data words `1` and `2` — and
`wasm_table_test.rs` executes the equivalent dispatch (`call(0,6) = 36`,
`call(1,4) = 64`).

`linker/tests/wasm_writer_test.rs` links a hand-built object end to end and
checks each section byte-for-byte, and then hands the output to a real engine
(`node -e 'new WebAssembly.Module(...)'`) and asserts the reported exports.

---

## Still missing, in plan order

- **WASI Preview 1 runtime in the linker.** `fd_write`, `args_get`,
  `proc_exit`, … are not provided by the linker; it only routes imports. (The
  compiler backend's `-wasi` path is separate and does emit a real
  `wasi_snapshot_preview1.fd_write` — see `tests/wasm_wasi_compiler_test.rs` at
  the workspace root — but this linker does not build on it.) A `wasm32-wasi`
  artifact linked here therefore expects a WASI host — both execution tests
  supply their own — not a bare `wasm32` engine. See `TARGET_MATRIX.md`. A
  program that calls nothing outside itself already needs no host at all.
- **One table, MVP element segments.** The link defines a single `funcref`
  table, so a table other than `__indirect_function_table` is `LNK008`.
  Passive, declarative and typed element segments are `LNK017`, and the
  `R_WASM_TABLE_INDEX_REL_SLEB` and the 64-bit relocation forms are unimplemented —
  clang's ordinary `call_indirect` output uses none of them, but hand-written
  LLVM IR and wasm64 objects would.

Those two gaps are why `wasm32-wasi` is `Tier2Experimental` rather than
`Tier1Supported`: single- and multi-object emission, relocation, cross-object
calls and indirect calls through a linker-defined table are real and executed
on two engines, but a program that writes to stdout still needs a WASI host
supplied to it (no shipped runtime provides preview1 any more). The compiler's
own WASI path is pinned separately by its execution test.
