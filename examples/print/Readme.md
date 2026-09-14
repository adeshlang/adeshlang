# AdeshLang Print Function Documentation

The `print` function is AdeshLang's primary output mechanism, providing high-performance text output with extensive formatting capabilities including colors, styling, and file output.

## Basic Syntax

```python
print(value1, value2, ..., {options})
```

## Simple Usage

```python
// Basic printing
print("Hello, World!");
print(42);
print(3.14159);
print(true);

// Multiple values
print("Name:", "Alice", "Age:", 25);
```

## Parameters

### Values
- **Any number of values** can be passed to print
- Values are automatically converted to strings
- Supported types: strings, numbers, booleans, arrays, objects, null

### Options Object (Optional)
The last parameter can be an options object with the following properties:

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `sep` | string | `" "` | Separator between values |
| `end` | string | `"\n"` | String appended at the end |
| `file` | string | - | File path to write to (instead of stdout) |
| `color` | string | - | Text color in hex format (#RRGGBB or #RGB) |
| `background` | string | - | Background color in hex format |
| `bold` | boolean | `false` | Bold text styling |
| `italic` | boolean | `false` | Italic text styling |
| `underline` | boolean | `false` | Underline text styling |
| `strikethrough` | boolean | `false` | Strikethrough text styling |
| ``pretty`` | boolean/string | ``false`` | Pretty print mode: ``true``, ``"full"``, ``"compact"``, ``"simple"`` |
| `flush` | boolean | `false` | Force flush output buffer |

## Formatting Examples

### Custom Separators and Endings

```python
// Custom separator
print("apple", "banana", "cherry", {sep: ", "});
// Output: apple, banana, cherry

// No newline at end
print("Loading", {end: "..."});
print("Done!");
// Output: Loading...Done!

// Custom ending
print("Item 1", {end: " | "});
print("Item 2", {end: " | "});
print("Item 3");
// Output: Item 1 | Item 2 | Item 3
```

### Colors and Styling

```python
// Text colors (hex format)
print("Error!", {color: "#FF0000"});        // Red text
print("Success", {color: "#00FF00"});       // Green text
print("Warning", {color: "#FFA500"});       // Orange text

// Short hex format
print("Info", {color: "#00F"});             // Blue text (#0000FF)

// Background colors
print("Highlighted", {background: "#FFFF00"}); // Yellow background

// Text styling
print("Bold text", {bold: true});
print("Italic text", {italic: true});
print("Underlined", {underline: true});
print("Strikethrough", {strikethrough: true});

// Combined styling
print("Important!", {
    color: "#FF0000",
    background: "#FFFF00", 
    bold: true,
    underline: true
});
```

### File Output

```python
// Write to file (creates if doesn't exist, appends if exists)
print("Log entry 1", {file: "output.log"});
print("Log entry 2", {file: "output.log"});

// Styled output to file (includes ANSI codes)
print("Error occurred", {
    file: "error.log",
    color: "#FF0000",
    bold: true
});
```

## Advanced Features

### Unicode Support

```python
// Full Unicode support including emojis
print("Hello, 世界 🌍");
print("Emoji: 😀 😎 🚀");
print("Hindi: नमस्ते भारत");
print("Arabic: مرحبا بالعالم");
print("Mixed: Mix 混合 🧪 δοκιμή тест");
```

### Complex Data Types

```python
// Arrays
print([1, 2, 3, 4, 5]);
// Output: [1, 2, 3, 4, 5]

// Objects
print({name: "Alice", age: 30, city: "New York"});
// Output: {name: Alice, age: 30, city: New York}

// Nested structures
print({
    users: ["Alice", "Bob"],
    settings: {theme: "dark", lang: "en"}
});
// Output: {users: [Alice, Bob], settings: {theme: dark, lang: en}}
```

### Performance Optimized Printing

```python
// High-frequency printing (optimized for performance)
for i in 0..10000 {
    print("Processing item", i);
}

// Batch output with custom formatting
for item in items {
    print("Item:", item.name, "Value:", item.value, {
        sep: " | ",
        color: "#00FF00"
    });
}
```

## Related Functions

### println()
Convenience function that adds a newline (equivalent to `print(..., {end: "\n"})`):

```python
println("This adds a newline");
// Same as: print("This adds a newline", {end: "\n"});
```

### eprint()
Print to stderr instead of stdout:

```python
eprint("Error message");  // Goes to stderr
```

## Performance Characteristics

AdeshLang's print function is highly optimized across all execution modes:

### Execution Mode Performance
- **AOT (Compiled)**: ~100-200 nanoseconds per print (uses native C printf)
- **JIT**: ~200-500 nanoseconds per print (optimized runtime)
- **Interpreter**: ~500-1000 nanoseconds per print (buffered I/O)

### Optimization Features
- **8KB buffered I/O** for high-speed output
- **Fast integer formatting** using `itoa` crate (~10x faster than standard formatting)
- **Fast float formatting** using `ryu` crate (~5x faster than standard formatting)
- **Zero-allocation** for primitive types (integers, booleans, null)
- **Fast path detection** - unstyled output bypasses all styling logic
- **Single ANSI code generation** for styled output

### Benchmark Results
```python
// 10,000 simple prints
for i in 0..10000 { print("test"); }
// AOT: ~2-3ms, JIT: ~5-8ms, Interpreter: ~15-20ms
```

## Error Handling

### Graceful Degradation
- Invalid hex colors are ignored (no styling applied)
- File I/O errors are silently handled (no exception thrown)
- Deeply nested structures are abbreviated with `<...>` to prevent stack overflow

### Depth Limiting
```python
// Prevents infinite recursion on circular references
let obj = {a: 1};
obj.self = obj;
print(obj);  // Output: {a: 1, self: <...>}
```

## Best Practices

### 1. Use Appropriate Styling
```python
// Good: Semantic color usage
print("✓ Success", {color: "#00FF00"});
print("✗ Error", {color: "#FF0000"});
print("⚠ Warning", {color: "#FFA500"});

// Avoid: Overuse of styling
print("Normal text", {bold: true, italic: true, underline: true}); // Too much
```

### 2. Efficient High-Frequency Printing
```python
// Good: Let the optimizer handle it
for i in 0..10000 {
    print("Item", i);  // Automatically optimized
}

// Avoid: Manual string building (slower)
for i in 0..10000 {
    let msg = "Item " + str(i);
    print(msg);
}
```

### 3. File Output for Logging
```python
// Good: Direct file output
print("Error occurred at", timestamp(), {file: "error.log"});

// Avoid: Manual file operations for simple logging
let file = open("error.log", "a");
file.write("Error occurred at " + timestamp() + "\n");
file.close();
```

### 4. Color Accessibility
```python
// Good: High contrast colors
print("Error", {color: "#FF0000", bold: true});    // Red + bold
print("Success", {color: "#00AA00", bold: true});  // Green + bold

// Consider: Color-blind friendly alternatives
print("Error", {color: "#D32F2F", bold: true});    // Darker red
print("Success", {color: "#388E3C", bold: true});  // Darker green
```

## Common Patterns

### Progress Indicators
```python
for i in 0..100 {
    print("Progress:", i + 1, "/", 100, {end: "\r"});
    // Simulate work
    sleep(0.1);
}
print("Complete!", {color: "#00FF00", bold: true});
```

### Structured Logging
```python
fn log(level, message) {
    let colors = {
        "INFO": "#00AA00",
        "WARN": "#FFA500", 
        "ERROR": "#FF0000"
    };
    
    print("[" + level + "]", message, {
        color: colors[level],
        bold: level == "ERROR",
        file: "app.log"
    });
}

log("INFO", "Application started");
log("WARN", "Low memory warning");
log("ERROR", "Database connection failed");
```

### Table Output
```python
// Header
print("Name", "Age", "City", {sep: " | ", bold: true});
print("-" * 30);

// Data rows
let users = [
    {name: "Alice", age: 30, city: "New York"},
    {name: "Bob", age: 25, city: "London"},
    {name: "Charlie", age: 35, city: "Tokyo"}
];

for user in users {
    print(user.name, user.age, user.city, {sep: " | "});
}
```

## Technical Implementation

### Backend Compatibility
- **Interpreter**: Full feature support with buffered I/O
- **JIT**: Full feature support with runtime optimizations  
- **AOT**: Full feature support using native C printf
- **WASM**: Basic support (styling depends on host environment)

### Memory Safety
- All print operations are memory-safe
- Automatic buffer management
- No risk of buffer overflows or memory leaks
- Depth limiting prevents stack overflow on recursive structures

### Thread Safety
- Print operations are thread-safe
- Stdout locking handled automatically
- No race conditions in multi-threaded environments

## Troubleshooting

### Colors Not Showing
- Ensure terminal supports ANSI colors
- Check if output is redirected (colors disabled in pipes/files by default)
- Verify hex color format (#RRGGBB or #RGB)

### Performance Issues
- Use unstyled output for high-frequency printing when possible
- Consider batch operations for large datasets
- File output is generally faster than terminal output for large volumes

### File Output Issues
- Check file permissions
- Ensure directory exists
- File operations fail silently - check file system space

This comprehensive documentation covers all aspects of AdeshLang's print function, from basic usage to advanced performance optimization techniques.

##  Pretty Print Feature

AdeshLang's print function now includes a powerful **pretty print** feature that provides beautiful, colored, and well-formatted output for complex data structures. This makes debugging, logging, and data visualization significantly easier.

### Quick Start

```python
let data = { name: "Alice", age: 30, items: ["a", "b", "c"] };
print(data, { pretty: true });
```

**Output:**
```
{
  name: "Alice" string,
  age: 30 number,
  items: [
    "a" string,
    "b" string,
    "c" string
  ] array[3]
} object
```

### Pretty Print Modes

| Mode | Description | Use Case |
|------|-------------|----------|
| 	rue or "full" | Full colors + type hints | Development, debugging |
| "compact" | No types, minimal formatting | Production logs |
| "simple" | Basic ANSI colors + types | Limited terminal support |
| alse | Disabled (regular print) | Plain text output |

### Pretty Print Features

####  Auto-Coloring
Beautiful syntax highlighting inspired by VS Code:
- **Keys**: Light blue (#9CDCFE)
- **Strings**: Peach/orange (#CE9178)  
- **Numbers**: Light green (#B5CEA8)
- **Booleans**: Blue (#569CD6)
- **Null**: Gray (#808080)
- **Type Hints**: Teal (#4EC9B0)
- **Brackets**: Light gray (#D4D4D4)

####  Auto-Indentation
- Hierarchical 2-space indentation
- Clean bracket placement
- Aligned object values
- Readable nested structures

####  Type Hints
Shows type information for every value:
```python
print(42, { pretty: true });
// Output: 42 number

print("hello", { pretty: true });
// Output: "hello" string

print([1, 2, 3], { pretty: true });
// Output:
// [
//   1 number,
//   2 number,
//   3 number
// ] array[3]
```

### Pretty Print Examples

#### Mode Comparison

```python
let user = {
    name: "Bob",
    age: 25,
    skills: ["Python", "Rust", "Go"]
};

// Full mode (default)
println("FULL MODE:");
print(user, { pretty: true });

// Compact mode
println("\nCOMPACT MODE:");
print(user, { pretty: "compact" });

// Simple mode
println("\nSIMPLE MODE:");
print(user, { pretty: "simple" });

// Regular print
println("\nREGULAR:");
print(user);
```

#### Complex Nested Structures

```python
let config = {
    server: {
        host: "localhost",
        port: 8080,
        ssl: {
            enabled: true,
            cert: "/path/to/cert.pem"
        }
    },
    database: {
        type: "postgresql",
        connections: {
            max: 20,
            min: 5
        }
    }
};

print(config, { pretty: true });
```

**Output:**
```
{
  database: {
    connections: {
      max: 20 number,
      min: 5 number
    } object,
    type: "postgresql" string
  } object,
  server: {
    host: "localhost" string,
    port: 8080 number,
    ssl: {
      cert: "/path/to/cert.pem" string,
      enabled: true bool
    } object
  } object
} object
```

#### Type Demonstration

```python
// Primitives
print(42, { pretty: true });              // 42 number
print("text", { pretty: true });          // "text" string
print(true, { pretty: true });            // true bool
print(null, { pretty: true });            // null

// Fixed-width integers
let num: i32 = 100;
print(num, { pretty: true });             // 100 i32

let big: u64 = 12345;
print(big, { pretty: true });             // 12345 u64

// Collections
print([1, 2, 3], { pretty: true });       // Pretty array
print((1, "two", true), { pretty: true }); // Pretty tuple
print({1, 2, 3}, { pretty: true });       // Pretty set
```

#### Combining with Other Options

```python
// Pretty print with custom separator
print(value1, value2, { 
    pretty: true, 
    sep: "\n---\n" 
});

// Pretty print without trailing newline
print(data, { pretty: true, end: "" });

// Pretty print multiple values
print("User:", user, "Score:", score, { 
    pretty: "compact",
    sep: " | " 
});
```

### Real-World Use Cases

#### API Response Debugging

```python
let response = {
    status: 200,
    data: {
        user: {
            id: "user_123",
            email: "user@example.com",
            preferences: {
                theme: "dark",
                notifications: true
            }
        }
    },
    meta: {
        requestId: "req_abc",
        timestamp: "2026-01-21T10:30:00Z"
    }
};

println("API Response:");
print(response, { pretty: true });
```

#### Configuration Validation

```python
let appConfig = {
    server: { host: "0.0.0.0", port: 8080 },
    database: { 
        type: "postgres", 
        maxConnections: 20 
    },
    cache: { enabled: true, ttl: 3600 }
};

println("Current Configuration:");
print(appConfig, { pretty: true });
```

#### Error Context Logging

```python
let errorContext = {
    error: "ValidationError",
    message: "Invalid email format",
    details: {
        field: "email",
        value: "invalid-email",
        expectedFormat: "user@domain.com"
    },
    stackTrace: [
        "at validateEmail (validator.adesh:45)",
        "at processForm (form.adesh:102)"
    ]
};

println("Error Report:");
print(errorContext, { pretty: true });
```

### Best Practices for Pretty Print

#### 1. Choose the Right Mode

```python
// Development: Use full mode
print(debugData, { pretty: true });

// Production logs: Use compact mode  
print(logData, { pretty: "compact", file: "app.log" });

// Limited terminals: Use simple mode
print(data, { pretty: "simple" });
```

#### 2. Combine with Existing Options

```python
// Pretty print with file output
print(data, { 
    pretty: true, 
    file: "debug.txt" 
});

// Pretty print with custom formatting
print(item1, item2, { 
    pretty: "compact",
    sep: " | ",
    end: "\n\n"
});
```

#### 3. Performance Considerations

```python
// For high-frequency logging, use compact mode
for i in 0..10000 {
    print(processItem(i), { pretty: "compact" });
}

// For debugging, full mode is fine
print(complexObject, { pretty: true });
```

### Supported Types

Pretty print works with all AdeshLang value types:

-  **Primitives**: number, string, boolean, null, char
-  **Fixed-width**: i8, i16, i32, i64, i128, u8, u16, u32, u64, u128, f32, f64
-  **Collections**: arrays, tuples, sets, objects
-  **Complex**: nested structures, mixed types
-  **Special**: functions, classes, instances, promises, references

### Cross-Mode Compatibility

Pretty print works in **all execution modes**:

-  **Interpreter**: Full support
-  **JIT**: Full support  
-  **AOT**: Full support
-  **VM**: Full support
-  **WASM**: Full support

### Color Scheme Reference

| Element | Default Color | Simple Color | Usage |
|---------|--------------|--------------|-------|
| Keys | #9CDCFE (Light Blue) | Cyan | Object/map keys |
| Strings | #CE9178 (Peach) | Yellow | String values |
| Numbers | #B5CEA8 (Light Green) | Green | Numeric values |
| Booleans | #569CD6 (Blue) | Magenta | true/false |
| Null | #808080 (Gray) | Bright Black | null values |
| Type Hints | #4EC9B0 (Teal) | Bright Cyan | type |
| Brackets | #D4D4D4 (Light Gray) | White | [], {}, () |
| Functions | #DCDCAA (Yellow) | Bright Yellow | Functions |
| Classes | #4EC9B0 (Teal) | Bright Cyan | Classes |

### Tips and Tricks

#### Quick Debug Comparison

```python
// Compare regular vs pretty quickly
println("Regular:"); print(data);
println("\nPretty:"); print(data, { pretty: true });
```

#### Conditional Pretty Print

```python
let DEBUG = true;
print(data, { pretty: DEBUG ? true : "compact" });
```

#### Pretty Print in Logs

```python
fn log(message, data) {
    println("[LOG]", message);
    print(data, { pretty: "compact", file: "app.log" });
}
```

### Advanced Configuration

While the current implementation uses sensible defaults, here are the internal options:

- **Indent**: 2 spaces (hierarchical)
- **Max Depth**: 10 levels (prevents infinite recursion)
- **Alignment**: Enabled for object values
- **Type Hints**: Enabled in full/simple modes
- **Colors**: RGB (default) or ANSI (simple)

### Examples Directory

See the examples/print/ directory for comprehensive examples:

1. **01_basic_pretty_print.adesh** - Basic usage
2. **02_pretty_modes.adesh** - Mode comparison
3. **03_complex_structures.adesh** - Nested data
4. **04_type_hints.adesh** - Type system demo
5. **05_combined_options.adesh** - Advanced usage
6. **06_real_world_cases.adesh** - Practical scenarios

