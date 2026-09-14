# Adesh Testing Examples

This directory contains comprehensive examples demonstrating the Adesh testing framework.

## 📚 Examples

### 1. Basic Tests (`01_basic_tests.adesh`)
Introduction to the testing framework with simple assertions:
- Basic `assert()` usage
- Comparison assertions
- Arithmetic testing
- String equality testing
- Custom error messages

**Run:**
```bash
adesh run --test examples/testing/01_basic_tests.adesh
```

### 2. Function Tests (`02_function_tests.adesh`)
Testing custom functions and their behavior:
- Testing helper functions
- Testing with different inputs
- Edge case testing
- Recursive function testing

**Run:**
```bash
adesh run --test examples/testing/02_function_tests.adesh
```

### 3. Data Structure Tests (`03_data_structure_tests.adesh`)
Testing arrays, objects, and strings:
- Array creation and access
- Array operations (push, length)
- Object creation and modification
- String operations

**Run:**
```bash
adesh run --test examples/testing/03_data_structure_tests.adesh
```

### 4. Control Flow Tests (`04_control_flow_tests.adesh`)
Testing conditionals and loops:
- If/else logic
- Loop behavior
- Complex conditionals
- Iterative calculations

**Run:**
```bash
adesh run --test examples/testing/04_control_flow_tests.adesh
```

### 5. Single Test Selection (`05_single_test_selection.adesh`)
Run a single test from a file:
- Select by name
- Fast iteration while debugging

**Run all tests:**
```bash
adesh run --test examples/testing/05_single_test_selection.adesh
```

**Run a single test:**
```bash
adesh run --test examples/testing/05_single_test_selection.adesh test_add_zero
```

### 6. Test Blocks (`06_test_blocks.adesh`)
Group many tests under a named block:
- Grouping for related tests
- Run an entire group or a single test in the group

**Run all tests:**
```bash
adesh run --test examples/testing/06_test_blocks.adesh
```

**Run a group:**
```bash
adesh run --test examples/testing/06_test_blocks.adesh math
```

**Run a single test inside a group:**
```bash
adesh run --test examples/testing/06_test_blocks.adesh math::add
```

## 🎯 Testing with Different Backends

The test framework works with all Adesh backends:

### Interpreter (Default)
```bash
adesh run --test examples/testing/01_basic_tests.adesh
```

### JIT Backend
```bash
adesh run --test --jit examples/testing/01_basic_tests.adesh
```

### Native JIT Backend
```bash
adesh run --test --native-jit examples/testing/01_basic_tests.adesh
```

### Bytecode VM
```bash
adesh run --test --bytecode examples/testing/01_basic_tests.adesh
```

## 📋 Quick Start

1. **Run a single test file:**
   ```bash
   adesh run --test examples/testing/01_basic_tests.adesh
   ```

2. **Run a single test by name:**
    ```bash
    adesh run --test examples/testing/05_single_test_selection.adesh test_add_basic
    ```

3. **Run with verbose output:**
   ```bash
   adesh run --test --verbose examples/testing/01_basic_tests.adesh
   ```

4. **Run with profiling:**
   ```bash
   adesh run --test --profile examples/testing/01_basic_tests.adesh
   ```

## ✍️ Writing Your Own Tests

### Basic Structure

```adesh
// Helper functions (if needed)
fn my_function(x) {
    return x * 2;
}

// Test functions marked with 'test' keyword
test fn test_my_function() {
    assert_eq(my_function(5), 10);
    assert_eq(my_function(0), 0);
}

// Main function (optional, won't run in test mode)
fn main() {
    print("This is not executed with --test flag");
}
```

### Available Assertions

- **`assert(condition)`** - Verify a condition is true
- **`assert(condition, message)`** - Verify with custom error message
- **`assert_eq(a, b)`** - Verify two values are equal
- **`assert_ne(a, b)`** - Verify two values are not equal

### Best Practices

1. **Name tests descriptively:** Use `test_` prefix and describe what you're testing
2. **Test one thing:** Each test should verify a single behavior
3. **Use clear assertions:** Make it obvious what's being tested
4. **Add error messages:** Help debugging with custom messages
5. **Test edge cases:** Include boundary conditions and special values

## 📊 Expected Output

When you run tests, you'll see:

```
PASS test_true
PASS test_comparison
PASS test_arithmetic
PASS test_with_message
PASS test_strings
PASS test_booleans
PASS test_variables

Test Summary:
    Total: 7
    Passed: 7
    Failed: 0
    Panics: 0
    Ignored: 0
    Timeout: 0
    Duration: 1.20ms
All tests passed: 7 passed
```

## 🔧 Troubleshooting

### Tests not discovered?
Make sure your test functions:
- Start with `test` keyword
- Are named with `fn` keyword
- Take no parameters
- Are in the file you're running

### Assertion failures?
Check:
- Expected vs actual values
- Type compatibility
- Floating point precision (use ranges for floats)

### Backend compatibility?
Some features may not work on all backends. Test on your target backend early!

## 📖 More Information

See [TESTING_FRAMEWORK.md](../../TESTING_FRAMEWORK.md) for complete documentation.
