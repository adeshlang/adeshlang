# testing-guide.md

# AdeshLang Test Framework Guide (Cargo-Grade & Production Ready)

AdeshLang includes a comprehensive, production-grade testing harness inspired by `cargo test` and systems testing standards.

---

## 1. Writing Tests

Tests in AdeshLang are declared using the `test fn` syntax or `@test` decorator.

```adesh
// Unit test function
test fn test_addition() {
    let result = 2 + 2;
    assert_eq(result, 4);
}

// Custom assertion message
test fn test_division() {
    let x = 10;
    let y = 2;
    assert(x / y == 5, "10 / 2 must equal 5");
}

// Negative assertions
test fn test_inequality() {
    assert_ne(1, 2);
}
```

### Built-in Assertion Intrinsics

| Assertion | Description |
|---|---|
| `assert(condition, [message])` | Assert that condition evaluates to `true` |
| `assert_eq(left, right, [message])` | Assert that `left == right` |
| `assert_ne(left, right, [message])` | Assert that `left != right` |

---

## 2. Test Discovery & Module Import

Tests can be organized across multiple files and imported seamlessly:

```adesh
// tests/01_option.test.adesh
export test fn test_some() {
    let opt = Some(42);
    assert(opt.is_some());
}

// tests/main.adesh
import "./01_option.test.adesh" as opt_tests;

test fn test_suite() {
    opt_tests.test_some();
}
```

When running `adesh test tests/main.adesh`, the test runner automatically:
1. Recursively traverses imported module files.
2. Discovers all `test fn` and `export test fn` declarations.
3. Namespaces discovered tests based on their import aliases (e.g. `opt_tests::test_some`).

---

## 3. Running Tests via CLI

### Basic Invocations
```bash
# Run tests in current project or specified file
adesh test tests/main.adesh
adesh run --test tests/main.adesh
```

### Filter & Exact Matching
```bash
# Substring filtering (matches any test containing "option")
adesh test tests/main.adesh option

# Exact matching (--exact)
adesh test tests/main.adesh test_origin_point --exact

# Skipping tests (--skip)
adesh test tests/main.adesh --skip struct_tests
```

### Discovery & Ignored Tests
```bash
# List all discovered tests without executing
adesh test tests/main.adesh --list

# Run only ignored tests
adesh test tests/main.adesh --ignored

# Run both ignored and regular tests
adesh test tests/main.adesh --include-ignored
```

---

## 4. Multi-Backend Testing & Conformance

AdeshLang allows verifying identical test behavior across all execution runtimes simultaneously:

```bash
# Run across Interpreter, JIT, and Native JIT
adesh test --runtimes=interp,jit,njit tests/main.adesh

# Test across ALL 11 backends (Full Conformance Matrix)
adesh test --backend-check tests/main.adesh
```

### Supported Backend Names for `--runtimes`:
- `interp` / `interpreter` — Standard Interpreter
- `jit` — LIR JIT Compiler
- `njit` / `native-jit` — Cranelift Native JIT Machine Code
- `vm` / `bytecode` — Bytecode Virtual Machine
- `mixed` — Hybrid Interpreter + JIT
- `adaptive` — Adaptive Speculative JIT
- `tiered` — Tiered JIT
- `aot` — Ahead-of-Time Native Cranelift Compiler
- `wasm` — WebAssembly (Wasmtime)
- `gpu` — MLIR GPU Backend (debug builds)

---

## 5. Parallel & Serial Execution

### High-Performance Parallel Execution (Default)
By default, tests and backends execute concurrently using a Rayon worker thread pool:
```bash
# Run tests in parallel (default auto-detects CPU cores)
adesh test --parallel tests/main.adesh

# Set explicit thread pool size
adesh test --test-threads=8 tests/main.adesh
adesh test -j 8 tests/main.adesh
```

### Sequential / Step-by-Step Execution
To execute tests sequentially one-by-one:
```bash
# Run tests sequentially one-by-one
adesh test --serial tests/main.adesh
adesh test --sequential tests/main.adesh
adesh test --one-by-one tests/main.adesh
adesh test --test-threads=1 tests/main.adesh
```

---

## 6. Output & CI Reporting

### Terminal Output
```text
Running tests across 3 backend(s): interp, jit, njit

╔══════════════════════════════════════════════════════════════╗
║          Multi-Backend Test Report                           ║
╚══════════════════════════════════════════════════════════════╝
Backends tested: interp, jit, njit

  Test                       interp      jit      njit  
  test_suite_option_types    PASS       PASS      PASS     0.1ms
  test_suite_result_types    PASS       PASS      PASS     0.2ms
  test_suite_struct_types    PASS       PASS      PASS     0.1ms
  test_suite_enum_types      PASS       PASS      PASS     0.1ms

Per-Backend Summary
  Backend        Total  Passed  Failed  Panics  Timeout  Ignored  Result
  ──────────     ─────  ──────  ──────  ──────  ───────  ───────  ────────
  interp             4       4       0       0        0        0  ✓ PASS
  jit                4       4       0       0        0        0  ✓ PASS
  njit               4       4       0       0        0        0  ✓ PASS

  ════════════════════════════════════════════════════════════
  ✓ All backends passed — output is consistent across runtimes
```

### Machine-Readable JSON (`--format json`)
```bash
adesh test --format json tests/main.adesh
```
Produces structured JSON with total, passed, failed counts, per-test timing, and per-backend matrix status suitable for automated CI/CD validation.
