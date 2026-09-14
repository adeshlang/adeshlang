# AdeshLang Input Function Documentation

The `input` function is AdeshLang's comprehensive user input system, providing text input, type conversion, interactive forms, selection menus, and testing capabilities with full Unicode support.

## Basic Syntax

```python
input(prompt?, options?)
input<T>(prompt?, options?)  // Generic type version
```

## Simple Usage

```python
// Basic text input
let name = input("Enter your name: ");
print("Hello,", name);

// With default value
let city = input("City: ", {def: "New York"});

// Numeric input with automatic conversion
let age = input("Age: ", {type: "int"});
```

## Generic Type System

AdeshLang supports generic type parameters for automatic type conversion and validation:

```python
// Type-safe input with automatic conversion
let age: u8 = input<u8>("Enter age (0-255): ");
let pi: f64 = input<f64>("Enter pi: ");
let confirmed: bool = input<bool>("Confirm (yes/no): ");
let name: string = input<string>("Enter name: ");
```

### Supported Generic Types

#### Unsigned Integers
- `u8` (0-255)
- `u16` (0-65,535)
- `u32` (0-4,294,967,295)
- `u64` (0-18,446,744,073,709,551,615)
- `u128` (very large unsigned)

#### Signed Integers
- `i8` (-128 to 127)
- `i16` (-32,768 to 32,767)
- `i32` (-2,147,483,648 to 2,147,483,647)
- `i64` (large signed range)
- `i128` (very large signed)

#### Floating Point
- `f32` (32-bit float)
- `f64` (64-bit double precision)

#### Other Types
- `bool` / `boolean` (true/false, yes/no, y/n, 1/0)
- `string` / `str` (text)
- `char` (single character)
- `int` (generic integer)
- `float` / `number` (floating point)

## Parameters

### Prompt (Optional)
- **Type**: string
- **Description**: Text displayed to the user before input
- **Default**: Empty (no prompt)

### Options Object (Optional)
The second parameter can be an options object with the following properties:

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `def` | any | - | Default value if input is empty |
| `type` | string | `"string"` | Expected data type for conversion |
| `min` | number | - | Minimum value (for numbers) |
| `max` | number | - | Maximum value (for numbers) |
| `required` | boolean | `false` | Whether input is required |
| `regex` | string | - | Regular expression pattern for validation |
| `file` | string | - | Read input from file instead of stdin |

## Basic Examples

### Simple Text Input

```python
// Basic prompt
let name = input("What's your name? ");
print("Hello,", name);

// With default value
let language = input("Programming language: ", {def: "AdeshLang"});
print("You chose:", language);

// Required input (keeps asking until provided)
let email = input("Email (required): ", {required: true});
```

### Type Conversion

```python
// Using options object
let age = input("Age: ", {type: "int"});
let height = input("Height: ", {type: "float"});
let married = input("Married (true/false): ", {type: "bool"});

// Using generic types (preferred)
let age: u8 = input<u8>("Age: ");
let height: f32 = input<f32>("Height: ");
let married: bool = input<bool>("Married: ");
```

### Validation

```python
// Range validation
let score = input("Score (0-100): ", {
    type: "int",
    min: 0,
    max: 100
});

// Pattern validation
let phone = input("Phone: ", {
    regex: "\\d{3}-\\d{3}-\\d{4}"
});

// Email validation
let email = input("Email: ", {
    regex: ".+@.+\\..+"
});
```

## Advanced Features

### Unicode Support

```python
// Full Unicode support including emojis
let message = input("Message (Unicode OK): ");
print("You said:", message);

// Examples of valid input:
// "Hello, 世界 🌍"
// "नमस्ते भारत"
// "مرحبا بالعالم"
// "こんにちは世界"
```

### File Input

```python
// Read input from file instead of stdin
let config = input("", {file: "config.txt"});
print("Config:", config);
```

### Error Handling with Generic Types

When using generic types, AdeshLang provides detailed error messages:

```python
// This will show a helpful error if user enters invalid data
let age: u8 = input<u8>("Enter age (0-255): ");
// If user enters "300":
// ❌ input<u8> Type Conversion Error
//   Input value: 300
//   Reason: Expected integer in range 0-255
//   Type: u8 (generic parameter)
```

## Interactive Selection

### input.select()

Create interactive selection menus with keyboard navigation:

```python
// Basic selection
let color = input.select("Choose color:", ["red", "green", "blue"]);
print("You chose:", color);

// With objects
let options = [
    {name: "Option A", value: "a"},
    {name: "Option B", value: "b"},
    {name: "Option C", value: "c"}
];
let choice = input.select("Select option:", options);
```

**Navigation**:
- Arrow keys or WASD to navigate
- Numbers (1-9) for direct selection
- Enter to confirm selection
- Works in both terminal and non-terminal environments

### input.selectEnum()

Select from enumerated values:

```python
let priority = input.selectEnum("Priority:", ["low", "medium", "high"]);
print("Priority set to:", priority);
```

## Form Input

### input.form()

Create complex forms with multiple fields and validation:

```python
// Basic form
let user = input.form({
    name: {prompt: "Full Name", required: true},
    age: {type: "int", min: 18, max: 100},
    email: {regex: ".+@.+", required: true}
});

print("User:", user.name, "Age:", user.age, "Email:", user.email);
```

### Advanced Form Configuration

```python
// Form with global options
let config = input.form({
    // Field definitions
    username: {
        prompt: "Username",
        required: true,
        validation: {
            min_length: 3,
            max_length: 20
        }
    },
    password: {
        prompt: "Password",
        type: "password",  // Hidden input
        required: true
    },
    port: {
        prompt: "Port",
        type: "int",
        min: 1024,
        max: 65535,
        def: 8080
    }
}, {
    // Global form options
    title: "Server Configuration",
    prefix: ">> ",
    separator: ": ",
    style: {
        color: "#00FF00",
        bold: true
    }
});
```

### Form Field Options

| Option | Type | Description |
|--------|------|-------------|
| `prompt` | string | Field label/prompt |
| `type` | string | Data type for conversion |
| `required` | boolean | Whether field is required |
| `min` | number | Minimum value (numbers) |
| `max` | number | Maximum value (numbers) |
| `def` | any | Default value |
| `regex` | string | Validation pattern |
| `options` | array | List of valid options |
| `validation` | object | Complex validation rules |

### Form Global Options

| Option | Type | Description |
|--------|------|-------------|
| `title` | string | Form title (displayed first) |
| `prefix` | string | Text before each prompt |
| `suffix` | string | Text after each prompt |
| `separator` | string | Between prompt and input (default: ": ") |
| `style` | object | Global styling (color, bold, etc.) |
| `order` | array | Field display order |
| `render` | function | Custom field renderer |

## Testing with input.mock()

For automated testing, AdeshLang provides a powerful mocking system:

### Basic Mocking

```python
// Mock single value
input.mock("Alice");
let name = input("Name: ");  // Returns "Alice"

// Mock multiple values
input.mock(["Alice", "25", "alice@example.com"]);
let name = input("Name: ");     // Returns "Alice"
let age = input("Age: ");       // Returns "25"
let email = input("Email: ");   // Returns "alice@example.com"
```

### Type-Safe Mocking

```python
// Mock with automatic type conversion
input.mock(42);
let age: u8 = input<u8>("Age: ");  // Returns 42 as u8

input.mock("true");
let confirmed: bool = input<bool>("Confirm: ");  // Returns true

// Mock array for sequential inputs
input.mock([100, 200, 300]);
let a: u16 = input<u16>("First: ");   // Returns 100 as u16
let b: u16 = input<u16>("Second: ");  // Returns 200 as u16
let c: u16 = input<u16>("Third: ");   // Returns 300 as u16
```

### Form Mocking

```python
// Mock form inputs in order
input.mock(["John Doe", "30", "john@example.com"]);

let user = input.form({
    name: {prompt: "Name", required: true},
    age: {type: "int"},
    email: {regex: ".+@.+"}
});
// Returns: {name: "John Doe", age: 30, email: "john@example.com"}
```

### Selection Mocking

```python
// Mock selection by index (1-based)
input.mock("2");
let color = input.select("Color:", ["red", "green", "blue"]);
// Returns: "green"

// Mock by exact value
input.mock("blue");
let color = input.select("Color:", ["red", "green", "blue"]);
// Returns: "blue"
```

## Error Handling

### Graceful Degradation
- Invalid input returns to prompt (with helpful error message)
- File read errors fall back to default value if provided
- Type conversion errors show detailed explanations
- UTF-8 BOM is automatically stripped from input

### Type Validation Errors

```python
// Detailed error messages for out-of-range values
let small: u8 = input<u8>("Enter 0-255: ");
// User enters "300"
// Error: 300 out of range for u8 (0-255)

// Boolean conversion examples
let flag: bool = input<bool>("Yes or no: ");
// Accepts: "true", "false", "yes", "no", "y", "n", "1", "0"
// Case insensitive
```

## Performance Characteristics

### Input Processing
- **Buffered I/O**: Efficient reading with automatic UTF-8 handling
- **Zero-copy**: Direct string processing when possible
- **Thread-safe**: Safe for concurrent use
- **Memory efficient**: Minimal allocations for simple inputs

### Type Conversion Performance
- **Fast parsing**: Optimized numeric conversion
- **Validation caching**: Type constraints checked once
- **Error short-circuiting**: Fast failure on invalid input

## Best Practices

### 1. Use Generic Types for Type Safety

```python
// Good: Type-safe with clear constraints
let port: u16 = input<u16>("Port (1-65535): ");

// Avoid: Manual conversion with potential errors
let port_str = input("Port: ");
let port = int(port_str);  // Could fail
```

### 2. Provide Helpful Prompts

```python
// Good: Clear expectations
let age: u8 = input<u8>("Age in years (0-255): ");
let email = input("Email address: ", {regex: ".+@.+"});

// Avoid: Vague prompts
let x = input("Enter something: ");
```

### 3. Use Forms for Complex Input

```python
// Good: Structured data collection
let user = input.form({
    name: {prompt: "Full Name", required: true},
    age: {type: "int", min: 0, max: 150},
    email: {regex: ".+@.+", required: true}
});

// Avoid: Multiple separate inputs without structure
let name = input("Name: ");
let age = input("Age: ");
let email = input("Email: ");
```

### 4. Mock for Testing

```python
// Good: Comprehensive test coverage
fn test_user_registration() {
    input.mock(["Alice Smith", "25", "alice@test.com"]);
    
    let user = register_user();
    assert(user.name == "Alice Smith");
    assert(user.age == 25);
    assert(user.email == "alice@test.com");
}

// Test edge cases
fn test_age_validation() {
    input.mock("300");  // Invalid for u8
    // Should handle error gracefully
}
```

## Common Patterns

### Configuration Input

```python
fn get_server_config() {
    return input.form({
        host: {
            prompt: "Server Host",
            def: "localhost"
        },
        port: {
            prompt: "Port",
            type: "int",
            min: 1024,
            max: 65535,
            def: 8080
        },
        ssl: {
            prompt: "Enable SSL",
            type: "bool",
            def: false
        }
    }, {
        title: "Server Configuration",
        style: {color: "#00AA00", bold: true}
    });
}
```

### User Registration

```python
fn register_user() {
    let user = input.form({
        username: {
            prompt: "Username",
            required: true,
            validation: {min_length: 3}
        },
        email: {
            prompt: "Email",
            regex: ".+@.+\\..+",
            required: true
        },
        age: {
            prompt: "Age",
            type: "int",
            min: 13,
            max: 120
        },
        terms: {
            prompt: "Accept Terms (yes/no)",
            type: "bool",
            required: true
        }
    });
    
    if !user.terms {
        print("Registration cancelled - terms not accepted");
        return null;
    }
    
    return user;
}
```

### Interactive Menu System

```python
fn main_menu() {
    while true {
        let choice = input.select("Main Menu:", [
            "1. View Profile",
            "2. Edit Settings", 
            "3. Generate Report",
            "4. Exit"
        ]);
        
        if choice.contains("1") {
            view_profile();
        } else if choice.contains("2") {
            edit_settings();
        } else if choice.contains("3") {
            generate_report();
        } else if choice.contains("4") {
            break;
        }
    }
}
```

### Data Import Wizard

```python
fn import_data() {
    let config = input.form({
        file_path: {
            prompt: "Data file path",
            required: true
        },
        format: {
            prompt: "File format",
            options: ["CSV", "JSON", "XML"]
        },
        delimiter: {
            prompt: "CSV delimiter",
            def: ","
        },
        skip_header: {
            prompt: "Skip header row",
            type: "bool",
            def: true
        },
        batch_size: {
            prompt: "Batch size",
            type: "int",
            min: 1,
            max: 10000,
            def: 1000
        }
    }, {
        title: "Data Import Configuration"
    });
    
    print("Importing from:", config.file_path);
    print("Format:", config.format);
    // ... import logic
}
```

## Technical Implementation

### Backend Compatibility
- **Interpreter**: Full feature support with string-based processing
- **JIT**: Full feature support with RuntimeValue processing
- **VM**: Full feature support with bytecode optimization
- **AOT**: Basic input support (platform-dependent)
- **WASM**: Host-dependent input capabilities

### Thread Safety
- Thread-local storage for mock values prevents cross-thread interference
- Input operations are atomic and thread-safe
- Form state is isolated per execution context

### Memory Management
- Automatic UTF-8 BOM removal
- Efficient string processing with minimal allocations
- Proper cleanup of input buffers and validation state

### Error Recovery
- Graceful handling of I/O errors
- Automatic retry on invalid input (with helpful messages)
- Fallback to default values when appropriate

## Troubleshooting

### Input Not Working
- Check if stdin is available (not redirected)
- Verify terminal supports interactive input
- Ensure proper encoding (UTF-8 recommended)

### Type Conversion Errors
- Check input format matches expected type
- Verify numeric ranges for integer types
- Use appropriate boolean representations (true/false, yes/no, 1/0)

### Form Issues
- Ensure all required fields have valid configurations
- Check regex patterns are properly escaped
- Verify field names don't conflict with reserved words

### Mock Testing Problems
- Clear mock state between tests: `input.mock([])`
- Ensure mock values match expected types
- Provide enough mock values for all expected inputs

This comprehensive documentation covers all aspects of AdeshLang's input system, from basic text input to advanced form handling and testing capabilities. The input function provides a complete solution for user interaction with strong type safety, validation, and testing support.