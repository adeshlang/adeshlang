# AdeshLang TODO & Backend Roadmap

## WebAssembly (WASM) Backend Enhancements
- [ ] **WASM Dynamic Enum Representation**:
  - Extend WASM data section and object layout to unroll runtime enum instances (`tag`, `__enum`, `value`).
- [ ] **WASM Parameter String Formatting**:
  - Enhance `emit_wasm_print_expr_dynamic` string pointer resolution for function parameter default overrides.
- [ ] **WASM Dynamic Object & Type Keyword Matching**:
  - Parity verification for complex generic default fields and enum match arms in WASM MVP target.
