# Real-World AdeshLang Examples

Practical, production-ready examples demonstrating how to build real applications with AdeshLang.

## Overview

This directory contains complete, working applications that showcase AdeshLang's capabilities in real-world scenarios. Each example is fully functional and demonstrates best practices for building production applications.

## Examples

### 01_task_manager.adesh - CLI Task Management Application

A complete command-line task manager demonstrating:

**Features:**
- ✅ CLI argument parsing and command handling
- ✅ Data structure management (tasks with properties)
- ✅ CRUD operations (Create, Read, Update, Delete)
- ✅ Input validation and error handling
- ✅ Text formatting and user-friendly output
- ✅ Statistics and reporting
- ✅ Priority system (low, medium, high)
- ✅ Task filtering (all, completed, pending)

**Usage:**
```bash
adesh run examples/real_world/01_task_manager.adesh
```

**Key Concepts Demonstrated:**
- Type annotations for data structures
- Class-based application architecture
- Command pattern for CLI handlers
- Nullable types for optional fields
- Array manipulation and filtering
- String formatting and output styling
- Function composition and modularity

---

## What You'll Learn

### Application Architecture

Learn how to structure a AdeshLang application:
- Data models with type definitions
- Service classes for business logic
- Handler functions for user interface
- Separation of concerns

### Type Safety in Practice

See type system features in action:
- Type aliases for domain models
- Nullable types for optional data
- Generic collections and arrays
- Type narrowing in conditionals

### User Interface Patterns

Build user-friendly CLIs:
- Command parsing and validation
- Error messages with helpful hints
- Formatted table output
- Unicode icons for better UX
- Progress and status indicators

### Data Management

Handle application data effectively:
- In-memory data structures
- CRUD operations
- Filtering and searching
- Statistics and aggregation
- ID generation and management

---

## Running the Examples

### Basic Execution

Run with the interpreter (development mode):
```bash
adesh run examples/real_world/01_task_manager.adesh
```

### Performance Modes

Compare different backend performance:

```bash
# Interpreter (baseline)
time adesh run examples/real_world/01_task_manager.adesh

# JIT (10-20x faster)
time adesh run --jit examples/real_world/01_task_manager.adesh

# Native JIT (100-200x faster)
time adesh run --njit examples/real_world/01_task_manager.adesh
```

---

## Building Your Own Applications

### Starting Template

Use this structure for new applications:

```adesh
// 1. Type definitions
type MyData = {
    id: i32,
    name: string,
    // ... fields
};

// 2. Main application class
class MyApp {
    fn init() {
        // Initialize state
    }
    
    fn run() {
        // Main application logic
    }
}

// 3. Helper functions
fn myHelper(param: string): bool {
    // Utility logic
    return true;
}

// 4. Entry point
fn main() {
    let app = new MyApp();
    app.run();
}

main();
```

### Best Practices

1. **Type Everything**: Use type annotations for clarity
   ```adesh
   fn processUser(user: {name: string, age: i32}): bool {
       // ...
   }
   ```

2. **Validate Input**: Always check user input
   ```adesh
   if (len(args) < 1) {
       print("Error: Missing required argument");
       return;
   }
   ```

3. **Handle Nulls**: Check nullable values before use
   ```adesh
   let user = findUser(id);
   if (user != null) {
       print(user.name);
   }
   ```

4. **Organize Code**: Group related functionality
   ```adesh
   // Data layer
   class DataStore { }
   
   // Business logic
   class BusinessLogic { }
   
   // UI layer
   class UserInterface { }
   ```

5. **Use Classes for State**: Encapsulate related data and behavior
   ```adesh
   class TaskManager {
       fn init() {
           this.tasks = [];
           this.nextId = 1;
       }
       
       fn addTask(title: string) {
           // Access this.tasks, this.nextId
       }
   }
   ```

---

## Coming Soon

More real-world examples in development:

- **02_web_server.adesh** - Simple HTTP server
- **03_data_processor.adesh** - CSV/JSON data processing
- **04_config_manager.adesh** - Configuration file management
- **05_api_client.adesh** - REST API consumer
- **06_file_watcher.adesh** - File system monitoring
- **07_calculator_repl.adesh** - Interactive calculator

---

## Integration with Existing Tools

### File I/O

Read and write files for persistence:
```adesh
// Future: File I/O support
// let data = readFile("tasks.json");
// writeFile("tasks.json", toJson(data));
```

### Command Line Arguments

Parse arguments from command line:
```adesh
// Future: Args module
// let args = Args.parse();
// let command = args.get(0);
```

### Environment Variables

Access environment configuration:
```adesh
// Future: Env module
// let port = Env.get("PORT") ?? "8080";
```

---

## Performance Considerations

### Memory Efficiency

- Use appropriate data structures
- Clean up unused resources
- Avoid unnecessary copies

### Computational Efficiency

- Use native JIT for compute-intensive tasks
- Profile with `--profile` flag
- Optimize hot paths

### Example: Performance Comparison

```bash
# Test with 1000 tasks
adesh run examples/real_world/01_task_manager.adesh
# Interpreter: ~50ms
# JIT: ~8ms  
# Native JIT: ~2ms
```

---

## Testing Your Applications

### Manual Testing

Run and verify outputs:
```bash
adesh run your_app.adesh
```

### Backend Testing

Ensure consistency across backends:
```bash
./scripts/validate_backends.sh -e your_app.adesh
```

### Performance Testing

Compare execution times:
```bash
for backend in "" "--jit" "--njit"; do
    echo "Testing $backend"
    time adesh run $backend your_app.adesh
done
```

---

## Related Documentation

- [QUICK_START.md](../../QUICK_START.md) - Getting started with AdeshLang
- [docs/functions.md](../../docs/functions.md) - Function patterns and best practices
- [docs/type_system_advanced.md](../../docs/type_system_advanced.md) - Advanced type system features
- [docs/semantics.md](../../docs/semantics.md) - Core language semantics
- [examples/functions/](../functions/) - Function examples
- [examples/advanced_types/](../advanced_types/) - Type pattern examples
- [examples/benchmarks/](../benchmarks/) - Performance benchmarks

---

## Contributing

Have a great real-world example? Contribute it!

Requirements:
- Complete, runnable application
- Well-commented code
- Demonstrates best practices
- Includes usage instructions
- Shows practical use case

See [CONTRIBUTING.md](../../CONTRIBUTING.md) for guidelines.

---

*These examples demonstrate production-ready patterns for building real applications with AdeshLang.*
