# Advanced Types Examples

This directory contains examples demonstrating AdeshLang's advanced type system features.

## Examples

### 01_option_type.adesh
Demonstrates null-safe programming with `Option<T>`:
- Basic Option usage (Some/None)
- Functions returning Option
- Default value handling
- Option with numeric literals (hex, binary)
- Configuration with optional values
- No null pointer exceptions!

**Run:**
```bash
adesh run examples/advanced_types/01_option_type.adesh
```

### 02_result_type.adesh
Demonstrates robust error handling with `Result<T, E>`:
- Basic Result usage (Ok/Err)
- Functions returning Result
- Input validation
- Chaining Result operations
- Result with numeric literals
- User validation example
- No exceptions - errors are values!

**Run:**
```bash
adesh run examples/advanced_types/02_result_type.adesh
```

## Key Concepts

### Option<T>
Use `Option<T>` when a value might not exist:
```adesh
fn find_user(id: i64): Option<User> {
    // Returns Some(user) if found, None otherwise
}

match find_user(42) {
    Some(user) => process(user),
    None => print("User not found")
}
```

### Result<T, E>
Use `Result<T, E>` when an operation can fail:
```adesh
fn parse_age(input: string): Result<i64, string> {
    // Returns Ok(age) if valid, Err(message) otherwise
}

match parse_age("25") {
    Ok(age) => print("Valid: " + string(age)),
    Err(error) => print("Invalid: " + error)
}
```

## Benefits

1. **Type Safety**: Compiler forces you to handle all cases
2. **No Null Pointers**: Option eliminates null pointer exceptions
3. **Explicit Errors**: Result makes error handling explicit
4. **Better Code**: Pattern matching makes intent clear
5. **Composable**: Easy to chain operations

## Comparison with Other Languages

### AdeshLang (Option)
```adesh
fn divide(a: f64, b: f64): Option<f64> {
    if b == 0.0 { return None; }
    return Some(a / b);
}
```

### Other Languages (Null)
```javascript
function divide(a, b) {
    if (b === 0) return null;  // Null pointer danger!
    return a / b;
}
```

### AdeshLang (Result)
```adesh
fn validate(age: i64): Result<i64, string> {
    if age < 0 { return Err("Negative age"); }
    return Ok(age);
}
```

### Other Languages (Exceptions)
```java
int validate(int age) throws InvalidAgeException {
    if (age < 0) throw new InvalidAgeException();  // Exception!
    return age;
}
```

## Related Documentation

- [TYPE_SYSTEM.md](../../docs/TYPE_SYSTEM.md) - Complete type system reference
- [semantics.md](../../docs/semantics.md) - Language semantics
- [functions.md](../../docs/functions.md) - Function reference

## Next Steps

1. Run the examples
2. Modify them to experiment
3. Create your own Option/Result functions
4. Explore pattern matching in detail

---

*These examples work identically across all AdeshLang backends: interpreter, JIT, Native JIT, AOT, and WASM.*
