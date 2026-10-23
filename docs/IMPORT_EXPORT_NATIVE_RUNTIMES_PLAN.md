# Native import/export: implementation and verification

## Scope

Native JIT (`--njit`), Adesh native AOT (`--aot`, Adesh codegen plus the
built-in linker), and WebAssembly now use static module linking. They do not
execute imported modules in the interpreter or switch backends on failure.
VM import/export remains deferred. This is a verified subset, not full parity
with interpreter module semantics.

## Shared static linker

`src/cli/static_imports.rs::link_ast` resolves the module graph before backend
lowering. It:

- Resolves paths relative to the importing source, canonicalizes module cache
  keys, and probes `.adesh`, `.adl`, and `.ind` for extensionless paths.
  Missing paths fail rather than searching unrelated directory trees.
- Type-checks imported source and applies the existing parse, decorator,
  ownership, and RAII gates before using its declarations.
- Rewrites namespace members and named/default bindings to module-unique
  symbols. Functions retain their private helper dependencies; unused exported
  functions are not emitted.
- Substitutes scalar constant expressions structurally, without evaluating
  language code.
- Tracks local shadowing in functions, blocks, lambdas, loops, catches, and
  match expressions.
- Caches duplicate imports and supports cyclic function dependency graphs.
  Cyclic constants, duplicate bindings, missing exports, and private member
  access produce errors.
- Queues top-level executable statements once per imported module and rewrites
  them after the module graph has loaded.

Supported syntax includes:

```adesh
import "./math.adesh" as math;
import { add, PI } from "./math.adesh";
import "./multiply.adesh"; // default binding is named multiply
```

A namespace alias can also be called directly when its module exports a default
function. JavaScript-style `import multiply from "..."` is not Adesh syntax.

The older `src/cli/import_linking.rs` remains for legacy paths, including VM.
Its resolution tests are not evidence of native execution.

## Backend integration

| Backend/entry point | Implementation | Execution evidence |
| --- | --- | --- |
| `run --njit` | Linked AST to HIR/LIR and Cranelift machine code | Numeric outputs checked |
| `run --aot` | Linked AST to Adesh native codegen and built-in linker, including the runtime archive | Native executable outputs checked |
| `run --wasm` | Linked AST to wasm, executed through Node.js with generated host imports | Actual wasm outputs checked |
| `compile-wasm` | Source-relative linking before AST wasm codegen | Output executed from a separate directory |
| `build --emit=wasm` | Same linked AST wasm compiler | Output executed from a separate directory |

Native AOT failures are returned without Cranelift or interpreter fallback.
Wasm modules import `env` functions rather than WASI; Node.js supplies those
functions through the generated loader. Missing Node.js and execution failures
are errors. The wasm test executor now uses this same execution path instead of
reporting success when Wasmtime is absent.

Additional fixes include correct native-JIT global handle serialization,
predeclared wasm function signatures for forward calls, inferred numeric wasm
return signatures, and correct wasm parameter/local index assignment.

## Regression evidence

`tests/static_imports_test.rs` checks:

- Private helper dependency retention and unused export elimination.
- Private member rejection, missing-file errors, duplicate bindings, cyclic
  constants, function dependency cycles, and parameter shadowing.
- Named, default, transitive, duplicate, and colliding module imports executing
  in njit, Adesh AOT, and wasm, with numeric stdout comparisons.
- The original `add`/`mul`/`PI` fixture producing `5`, `20`, and `3.14` in all
  three backends.
- Invalid imports failing in all three backends without fallback.
- Both wasm compilation commands resolving imports from the source directory,
  independently of the working/output directory, and producing executable wasm.
- Source-relative imports through the njit and wasm test runners, with a missing
  Node.js installation correctly failing wasm tests.

These execution checks require Node.js and the host native runtime/toolchain.
Evidence gathered on Windows does not establish Linux/macOS linker correctness.
Checking artifact creation or a zero exit code alone is insufficient.

The type checker no longer assumes a void collection return for an unresolved
receiver's `add`, `insert`, or `set` call. This avoids rejecting arithmetic on
an imported `add` result before linking.

Exported declarations are exempt from declaration-level unused warnings.
Their bodies still receive unused-local and unused-parameter checks. The
obsolete native-JIT unsupported-import warning has been removed.

## Limitations and remaining work

- Mutable module variables, managed module-level bindings, and runtime-initialized
  module bindings are not implemented. Referenced mutable bindings and unsupported
  initializers fail explicitly rather than being copied as independent values.
- Namespaces are not first-class runtime values.
- Package/standard-library namespace resolution is not implemented by this
  linker; current coverage is filesystem imports.
- Imported classes, structs, enums, complex type annotations, re-exports, and
  full cross-module type inference are not established by these regressions.
- Initializer ordering in complex cyclic graphs is not equivalent to a complete
  runtime module lifecycle and needs further coverage.
- The existing AST wasm backend still has general language-semantic gaps,
  including assertion stubs and ignored unsupported statements. Numeric output
  comparisons, not the fixture's assertions alone, establish the tested import
  behavior. This change does not certify general wasm language compatibility.
- VM integration and non-Windows execution testing remain deferred.
- The legacy AOT test executor still selects Cranelift. Native Adesh AOT
  execution evidence above comes from `run --aot`, not that executor.
