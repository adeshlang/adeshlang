# Opt-in standalone WASM pipeline

The existing WASM compiler remains the default and is not removed.

```powershell
adesh build math.adesh --target wasm32-unknown-unknown --codegen=wasm-standalone -o math.wasm
adesh build math.adesh --target wasm32-unknown-unknown --codegen=wasm-standalone --emit=obj -o math.o
```

Example `math.adesh`:

```adesh
export fn twice(x: i64): i64 {
    return x + x;
}
```

The module is a library: instantiate it with an empty import object and call
its exported functions. JavaScript passes `i64` arguments as `BigInt`.
There is no implicit entry point invocation and no generated JavaScript loader.

## Architecture

The build's existing type, ownership and static-import gates produce HIR.
`src/backends/wasm/standalone.rs` lowers supported HIR into explicitly typed
Machine IR. `crates/adesh-codegen/src/targets/wasm/standalone.rs` emits standard
relocatable WASM, including linking symbols and call/data relocations.
The normal new build path uses adeshlink's object reader, link planner,
relocation engine and binary writer entirely in memory. Object-only output
can instead be passed to the public `adeshlink` file-based path.

Neither new lowering nor object emission uses `unsafe`; both forbid it.
This does not prove the complete compiler, linker, host or runtime memory-safe.
WASM memory bounds isolate linear-memory accesses, not arbitrary logical
object boundaries within that memory.

## Current source subset

- Function-only modules with explicit parameter and return types.
- `bool`, signed `i32`/`i64`, `f32`/`f64` (`float` is `f64`).
- Scalar locals, assignment, lexical blocks, `if`/`else`, `while`,
  `break`/`continue`, direct calls and recursion.
- Fixed-width add/subtract/multiply, integer bitwise operations, floating
  division, comparisons, negation and short-circuit boolean operations.
- Integer operations wrap at their declared width.
- Explicit `null` return types represent void, with `return;` or fallthrough.
- Only source `export` functions are exported; there is no automatic `main`.
- `--check` validates the same lowering and object encoding.

Dynamic `int` is deliberately rejected rather than silently truncating it
to `i64`. Use an explicit fixed-width type.

Unsupported constructs produce errors without falling back to the old
compiler. These include top-level statements/globals, dynamic values, strings,
collections, heap allocation, closures/indirect calls, classes, exceptions,
async, default parameters, casts, integer division/modulo/shifts and WASI or
browser runtime adapters. Unreachable statements and missing value returns
are rejected. `--run`, external/native link options, LTO and PGO are refused.
Optimization flags currently do not enable a Machine IR optimization pipeline.

## Limits and trade-offs

Object encoding defaults to 64 MiB output, 65,536 functions/imports, 65,536
locals per function and 4,096 blocks per function. HIR lowering additionally
limits nesting to 256 and visited nodes to 100,000 per function. These are
compiler rejection limits, not a cap on all process memory or execution time.
Parsing and the mandatory compiler gates run before these lowering limits.
Recursive guest code is subject to the host engine's stack limits.

Virtual registers map to WASM locals. Arbitrary control flow uses a
`br_table` dispatcher, favoring correctness and bounded compilation work over
optimal structured output. Signature lookup uses ordered maps; builds are
deterministic. Object linking avoids temporary files and releases the raw
object buffer after parsing. No best-in-class speed or memory claim is made.

## Validation suites

- `cargo test -p adesh-codegen --test wasm_standalone_exec`
- `cargo test --test wasm_standalone_source_test`
- `cargo test --lib output_path_is_not_an_optimization_level`
- Existing `wasm_syntax_regressions`, `wasm_wasi_compiler_test` and
  `wasm_wasmtime_execution_test` protect the legacy compiler and linker.

The first suite needs Node. The source suite needs Node and uses the
workspace's wasmtime dependency as a second execution engine.

## Remaining replacement work

Keep the default compiler until parity is demonstrated. Add globals and
tables/indirect calls to the new emitter, then checked runtime allocation,
strings/collections/closures/exceptions and explicit browser/WASI adapters.
Validate semantics and measure fresh-process compile time, peak memory,
output size and execution time at representative and stress inputs before
considering a default switch. Existing linker support for clang tables does
not mean the new emitter or HIR lowering supports them.
