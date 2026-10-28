# WebAssembly Backend (`wasm`)

**Status: not implemented.** `WasmWriter::encode_binary` returns
`LNK017 UnsupportedFormat` rather than emitting a module. WebAssembly output
today comes from the compiler's WASM backend, not from this linker.

Linking a wasm target fails loudly:

```console
$ adeshlink --target wasm32-wasi main.o -o app.wasm
error[LNK017]: the WebAssembly linker writer is not implemented: ...
  help: WebAssembly modules must be produced by the WASM compiler backend, not
        by `adeshlink`; ...
```

---

## 1. What the reader does support

`\0asm` module parsing exists in `linker/src/wasm/sections.rs`, but it is
module-shaped only: it ignores the version field (so a *relocatable* wasm
object is mis-parsed as a module), collapses every section to `SectionKind::Rodata`,
and never reads `.relwasm`, so relocations cannot reach any writer.

---

## 2. What a real writer requires

The section model below describes the target design, not current behaviour. To
close the gap (Phase 17, option B) the writer must emit, in section order:

1. **Type Section (ID 1)**: function signatures (`valtype` vectors), deduplicated.
2. **Import Section (ID 2)**: host functions, memory, tables, and globals imported
   from the environment — today this section is never emitted at all.
3. **Function Section (ID 3)**: type indices for all module-defined functions.
4. **Table Section (ID 4)**: function pointers and indirect call dispatch tables.
5. **Memory Section (ID 5)**: linear memory limits (initial pages, maximum pages).
6. **Global Section (ID 6)**: Mutable and immutable global variables (e.g., stack pointer `__stack_pointer`).
7. **Export Section (ID 7)**: Public functions, memory (`memory`), and globals exported to host.
8. **Start Section (ID 8)**: Optional initialization entry function.
9. **Element Section (ID 9)**: Table segment initializers.
10. **Code Section (ID 10)**: Function bodies containing local declarations and WASM bytecodes.
11. **Data Section (ID 11)**: Static data segments placed at linear memory offsets.
12. **Custom Section (ID 0)**: Metadata sections including `name`, `producers`, and `.adesh.meta`.

WASI Preview 1 (`fd_write`, `args_get`, `proc_exit`, …) is **not** implemented.
The compiler backend emits an `env.*` import namespace instead, so a
`wasm32-wasi` artifact is not a WASI module; see `TARGET_MATRIX.md`.
