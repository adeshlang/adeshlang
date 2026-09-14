# testing-guide.md

> Consolidated from 1 documentation files on 2026-08-29.

---


---

## Source: TEST_FRAMEWORK_SPEC.md

# Adesh Test Framework Spec (Rust-Grade)

## 1) Assertion Lowering Logic

All assertion intrinsics lower to a single failure intrinsic:

```text
intrinsic __adesh_test_fail(file: string, line: int, message: string) -> never
```

### IR lowering example: `assert_eq(a, b)`

```text
v0 = eval a
v1 = eval b
v2 = eq v0, v1
br_if v2, bb_pass, bb_fail

bb_fail:
  v3 = format("assert_eq failed: left={:?}, right={:?}", v0, v1)
  call __adesh_test_fail(file="tests/math.adesh", line=42, message=v3)
  test_abort

bb_pass:
  continue
```

### Unwind behavior

`__adesh_test_fail` records failure metadata and emits an immediate `test_abort` edge that exits the current test frame. No further assertions in that test execute.

Pass criteria:
- PASS only if the test reaches normal end without `__adesh_test_fail` and without panic/trap.

## 2) Test Runner Architecture

```text
Test Discovery -> Parse once -> Build IR once
     -> Select tests by tags/filters
     -> Per-test isolated execution sandbox
     -> Backend execution (standard backend contract)
     -> Normalize status (PASS/FAIL/PANIC/TIMEOUT/IGNORED)
     -> Aggregate summary + backend matrix
     -> Text/JSON report
```

Isolation contract:
- fresh runtime instance per test
- reset heap/global state
- deterministic seed + frozen clock per test

## 3) Backend Matrix Comparison Algorithm

For each discovered test `t`:
1. Execute test across active backends with same IR.
2. Normalize native backend outcome to canonical statuses.
3. Build row: `t -> {backend -> status}`.
4. If row has >1 unique status, mark as inconsistency.

Pseudo-code:

```text
for test in tests:
  row = {}
  for backend in backends:
    row[backend] = normalize(run(test_ir, backend))
  matrix[test] = row
  if unique(row.values).len > 1:
    inconsistencies.push(test, row)
```

## 4) Example Terminal Output (text)

### Colored FAIL with stack trace

```text
FAIL math::adds_two - assert_eq failed: left=3 right=4
  at tests/math.adesh:42
  stack:
    0: math::adds_two
    1: __adesh_test_entry
```

### Backend Result Matrix with mismatch

```text
Backend Result Matrix
+------------------------------+-----------+-----------+-----------+
| Test                         | Interpreter | Jit      | Bytecode  |
+------------------------------+-----------+-----------+-----------+
| math::adds_two               | PASS      | FAIL      | PASS      |
| array::bounds                | PANIC     | PANIC     | PANIC     |
+------------------------------+-----------+-----------+-----------+
Backend Inconsistencies:
  - math::adds_two => Interpreter:PASS, Jit:FAIL, Bytecode:PASS
```

## 5) JSON Report Schema Example

```json
{
  "summary": {
    "total": 12,
    "passed": 10,
    "failed": 1,
    "panics": 0,
    "ignored": 1,
    "timeout": 0,
    "duration_ms": 483
  },
  "tests": [
    {
      "name": "math::adds_two",
      "status": "FAIL",
      "expect_fail_applied": false,
      "backends": {
        "Interpreter": {
          "status": "PASS",
          "duration_ms": 2,
          "message": null,
          "stack_trace": null
        },
        "Jit": {
          "status": "FAIL",
          "duration_ms": 1,
          "message": "assert_eq failed: left=3 right=4",
          "stack_trace": ["math::adds_two", "__adesh_test_entry"]
        }
      }
    }
  ],
  "matrix": {
    "backends": ["Interpreter", "Jit", "Bytecode"],
    "rows": {
      "math::adds_two": {
        "Interpreter": "PASS",
        "Jit": "FAIL",
        "Bytecode": "PASS"
      }
    },
    "inconsistencies": [
      "math::adds_two => Interpreter:PASS, Jit:FAIL, Bytecode:PASS"
    ]
  }
}
```

