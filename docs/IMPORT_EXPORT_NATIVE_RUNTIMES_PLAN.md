# Import/Export — Native Support in njit, vm, aot, wasm Runtimes

## Goal
Give the four non-interpreter runtimes (njit/native-JIT, vm/bytecode, aot, wasm)
native import/export support — i.e. each runtime resolves `import` statements,
loads the imported modules to collect their exports, and runs `main` with those
exports bound natively — instead of delegating the whole program to the
interpreter.

## Background / research findings
- The **interpreter** (`src/execution/runtime_core/`) already has full
  import/export: `ModuleLoader` cache, `Interpreter::run_module`, and
  `exec_stmt` handling of `StmtKind::Import` / `ImportDefault` / `ImportNames`
  and the `export` flag on declarations (file paths + stdlib modules).
- `njit` (`run_with_native_jit`) compiles one module to native machine code and
  runs `main`; it had **no** cross-module import linking (calls into imported
  modules fail loudly).
- `vm` (`run_with_bytecode`) compiles to `.adeshbc` bytecode and runs the VM;
  the bytecode compiler has **no** import handling.
- `aot` (`run_with_aot`) compiles to a standalone native binary; no language-level
  import linking.
- `wasm` (`run_with_wasm`) compiles to a `.wasm` module that only exports
  `main`; no cross-module import/export linking.

Only the interpreter owns the module loader. The four runtimes need their own
native resolution + binding so `main` runs in that runtime with imports available.

## Changes made (build verified: `cargo check` passes)

### 1. `src/cli/backends.rs`
Added a shared native module loader:
- `load_import_exports(src, root_dir) -> Result<std::collections::HashMap<String, Value>, String>`
  - Parses the source for `import "path" as alias`, `import default "path" as alias`,
    and `import {a,b} from "path"` statements.
  - Resolves each path via `crate::execution::runtime_core::interpreter_impl::utilities::resolve_path`
    (relative to the importing file's directory, `.adesh`/`.adl`/package resolution).
  - Reads the module source, executes it through `Interpreter::run_module` to
    collect its `exports` object (cached in `ModuleLoader`), and merges the
    exports keyed by alias (and the alias as a namespace object).
  - This is the **resolution + export-collection** part, done natively per
    runtime (not whole-program interpreter delegation).
- Added imports: `resolve_path`, `StmtKind`, `Value`, `Lexer`, `Parser`,
  `HashMap`, `Path`, `PathBuf`.
- Wired `run_with_native_jit` to call `load_import_exports` and pass the
  prebound globals into the native JIT compiler.

### 2. `src/execution/runtime_core/interpreter/module_loader.rs`
Added a public accessor so other modules can read the cached exports:
- `pub fn get_cached(&self, path: &str) -> Option<&Value>`

### 3. `src/backends/jit/native/compiler.rs`
Added pre-bound global support to `NativeJitCompiler`:
- Fields `prebound_globals: HashMap<String, crate::parsing::ast::Value>` and
  `prebound_globals_ptrs: Vec<*mut crate::parsing::ast::Value>`.
- `pub fn bind_prebound_global(&mut self, name, value)` registers an imported
  export as a global the compiled `main` can reference.
- `collect_globals` declares/initializes each prebound global as JIT data
  (leaked `Value` pointer) so compiled code can address it.

### 4. `src/backends/jit/native/mod.rs`
- `native_jit_run_with_stats(source, prebound_globals)` now accepts the prebound
  globals map and binds each into the compiler before `compile_module`.
- `native_jit_run` passes `Default::default()`.

### 5. `src/testing/executor.rs`
- `run_test_native_jit` now passes `Default::default()` for prebound globals.

### 6. `linker/src/relocation.rs`
- Temporarily added a wildcard match arm for
  `AArch64TlvDescriptorLo12 | X86_64TlvDescriptorRel32` to unblock the build
  (pre-existing non-exhaustive match on `RelocationKind`, unrelated to this task).

## Remaining work (not yet done)
- Wire `load_import_exports` into **vm** (`run_with_bytecode`) so the bytecode
  VM resolves/loads imports and binds them before running `main`.
- Wire `load_import_exports` into **aot** (`run_with_aot`) so the AOT binary
  resolves/loads imports and binds them before running `main`.
- Wire `load_import_exports` into **wasm** (`run_with_wasm`) so the wasm module
  resolves/loads imports and binds them before running `main`.
- For each runtime, ensure compiled `main` can actually **read imported
  constants** and **call imported functions** (the current njit binding stores a
  `Value` pointer in a global; making that usable for calls/reads requires
  cross-module function linking in each backend — a larger compiler change).
- Add end-to-end tests for import/export through each of the four backends.
- Revisit/decide whether to keep the temporary `linker/src/relocation.rs` change.

## Test files created (untracked, for manual verification)
- `import_test/math.adesh` — exports `add`, `mul`, `PI`.
- `import_test/main.adesh` — `import "./math.adesh" as math;` and uses them.
- Verified working through the interpreter (`--interpreter run`).

## Notes
- `cargo check` passes with the changes above.
- The deep native binding (making compiled `main` call imported functions) is
  the hard part and remains for the vm/aot/wasm wiring plus the njit call/read
  support.
- Do not delete `import_test/` without checking; it's manual verification scaffolding.
