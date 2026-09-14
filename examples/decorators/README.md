# AdeshLang Decorators

Decorators in AdeshLang provide a powerful way to modify or enhance functions, methods, and class properties at definition time. They follow a similar pattern to TypeScript/JavaScript decorators but with AdeshLang's unique syntax.

## Table of Contents

1. [Basic Concepts](#basic-concepts)
2. [Function Decorators](#function-decorators)
3. [Method Decorators](#method-decorators)
4. [Property Decorators](#property-decorators)
5. [Class Decorators](#class-decorators)
6. [Decorator Factories](#decorator-factories)
7. [Stacking Decorators](#stacking-decorators)
8. [Best Practices](#best-practices)

## Basic Concepts

### What is a Decorator?

A decorator is a special function that:
- Takes a target (function, method, property, or class) and metadata as parameters
- Returns a modified version of the target
- Is applied using the `@` syntax at definition time

### Decorator Signature

```adeshlang
decorator decoratorName(target, meta) {
  // target: the original function/property/class being decorated
  // meta: object with metadata (name, type, params, etc.)
  
  // Return modified target or wrapper
  return modifiedTarget;
}
```

### Metadata Object

The `meta` parameter contains information about the decorated target:

```adeshlang
{
  name: "functionName",           // Name of the decorated item
  type: "function" | "property" | "class" | "method",
  params: [Type1, Type2, ...],   // For functions: parameter types
  returnType: "ReturnType",       // For functions: return type
  className: "ClassName",         // For methods/properties: containing class
  isStatic: true/false,          // For methods/properties: static flag
}
```

## Function Decorators

Function decorators wrap standalone functions to add behavior.

### Basic Logging Decorator

```adeshlang
// Simple logging decorator
decorator log(target, meta) {
  return fn(...args) {
    print("Calling:", meta.name, "with args:", args);
    let result = target(...args);
    print("Result:", result);
    return result;
  };
}

@log
fn add(a, b) {
  return a + b;
}

add(2, 3);
// Output:
// Calling: add with args: [2, 3]
// Result: 5
```

### Performance Timing Decorator

```adeshlang
// Measure execution time
decorator measureTime(target, meta) {
  return fn(...args) {
    let startTime = Date.now();
    let result = target(...args);
    let endTime = Date.now();
    print(meta.name, "executed in", endTime - startTime, "ms");
    return result;
  };
}

@measureTime
fn slowOperation() {
  let sum = 0;
  for (let i = 0; i < 1000000; i = i + 1) {
    sum = sum + i;
  }
  return sum;
}
```

### Memoization Decorator

```adeshlang
// Cache function results
decorator memoize(target, meta) {
  let cache = {};
  
  return fn(...args) {
    let key = JSON.stringify(args);
    
    if (key in cache) {
      print("Cache hit for", meta.name);
      return cache[key];
    }
    
    print("Cache miss for", meta.name);
    let result = target(...args);
    cache[key] = result;
    return result;
  };
}

@memoize
fn fibonacci(n) {
  if (n <= 1) return n;
  return fibonacci(n - 1) + fibonacci(n - 2);
}
```

## Method Decorators

Method decorators work similarly to function decorators but operate on class methods.

### Method Authorization Decorator

```adeshlang
decorator requireAuth(target, meta) {
  return fn(...args) {
    // Check if user is authenticated
    if (!this.isAuthenticated) {
      throw "Unauthorized: " + meta.name + " requires authentication";
    }
    return target(...args);
  };
}

class UserService {
  let isAuthenticated = false;
  
  fn login(username, password) {
    this.isAuthenticated = true;
  }
  
  @requireAuth
  fn deleteAccount() {
    print("Account deleted");
  }
}
```

### Method Validation Decorator

```adeshlang
decorator validateArgs(target, meta) {
  return fn(...args) {
    // Check if all arguments are non-null
    for (let i = 0; i < args.length; i = i + 1) {
      if (args[i] == null) {
        throw meta.name + ": argument " + i + " cannot be null";
      }
    }
    return target(...args);
  };
}

class Calculator {
  @validateArgs
  fn divide(a, b) {
    return a / b;
  }
}
```

## Property Decorators

Property decorators modify class properties (static and instance).

### Uppercase Property Decorator

```adeshlang
decorator uppercase(target, meta) {
  if (typeof target == "string") {
    return target.toUpperCase();
  }
  return target;
}

class Config {
  @uppercase
  static environment = "development";
  
  @uppercase
  static mode = "debug";
}

print(Config.environment); // "DEVELOPMENT"
print(Config.mode);        // "DEBUG"
```

### Readonly Property Decorator

```adeshlang
decorator readonly(target, meta) {
  // Wrap in object with getter only
  return {
    __readonly: true,
    __value: target,
    get: fn() { return this.__value; }
  };
}

class Constants {
  @readonly
  static PI = 3.14159;
  
  @readonly
  static MAX_SIZE = 1000;
}
```

## Class Decorators

Class decorators modify entire classes, allowing you to add methods, seal classes, or implement mixins.

### Sealed Class Decorator

```adeshlang
decorator sealed(target, meta) {
  // Mark class as non-extendable
  print("Sealing class:", meta.name);
  target.__sealed = true;
  return target;
}

@sealed
class FinalClass {
  fn method() {
    return "Cannot extend this class";
  }
}
```

### Singleton Decorator

```adeshlang
decorator singleton(target, meta) {
  let instance = null;
  
  return {
    getInstance: fn() {
      if (instance == null) {
        instance = new target();
      }
      return instance;
    }
  };
}

@singleton
class Database {
  fn connect() {
    print("Connected to database");
  }
}

let db1 = Database.getInstance();
let db2 = Database.getInstance();
// db1 and db2 reference the same instance
```

### Logging Class Decorator

```adeshlang
decorator logClass(target, meta) {
  // Wrap all methods with logging
  let originalMethods = { ...target.methods };
  
  for (let methodName in originalMethods) {
    let originalMethod = originalMethods[methodName];
    target.methods[methodName] = fn(...args) {
      print("Method called:", meta.name + "." + methodName);
      return originalMethod(...args);
    };
  }
  
  return target;
}
```

## Decorator Factories

Decorator factories are functions that return decorators, allowing parameterization.

### Configurable Retry Decorator

```adeshlang
decorator retry(maxAttempts) {
  return fn(target, meta) {
    return fn(...args) {
      let attempts = 0;
      let lastError = null;
      
      while (attempts < maxAttempts) {
        attempts = attempts + 1;
        try {
          return target(...args);
        } catch (e) {
          lastError = e;
          print("Attempt", attempts, "failed:", e);
        }
      }
      
      throw "Failed after " + maxAttempts + " attempts: " + lastError;
    };
  };
}

@retry(3)
fn unstableNetworkCall() {
  // Simulated network call that might fail
  if (Math.random() > 0.5) {
    throw "Network error";
  }
  return "Success";
}
```

### Throttle Decorator Factory

```adeshlang
decorator throttle(delayMs) {
  return fn(target, meta) {
    let lastCallTime = 0;
    
    return fn(...args) {
      let now = Date.now();
      
      if (now - lastCallTime < delayMs) {
        print(meta.name, "throttled");
        return null;
      }
      
      lastCallTime = now;
      return target(...args);
    };
  };
}

@throttle(1000)  // Only allow calls every 1 second
fn apiCall() {
  print("API call executed");
}
```

### Configurable Logging Level

```adeshlang
decorator logLevel(level) {
  return fn(target, meta) {
    return fn(...args) {
      if (level == "debug") {
        print("[DEBUG]", meta.name, "args:", args);
      } else if (level == "info") {
        print("[INFO]", meta.name, "called");
      }
      
      let result = target(...args);
      
      if (level == "debug") {
        print("[DEBUG]", meta.name, "result:", result);
      }
      
      return result;
    };
  };
}

@logLevel("debug")
fn calculateTotal(items) {
  return items.length * 10;
}

@logLevel("info")
fn processOrder(orderId) {
  return "Order " + orderId + " processed";
}
```

## Stacking Decorators

Multiple decorators can be applied to the same target. They are applied from bottom to top (nearest to the function first).

### Example: Logging + Timing + Caching

```adeshlang
decorator log(target, meta) {
  return fn(...args) {
    print("→ Calling:", meta.name);
    let result = target(...args);
    print("← Returned:", result);
    return result;
  };
}

decorator time(target, meta) {
  return fn(...args) {
    let start = Date.now();
    let result = target(...args);
    print("⏱ Time:", Date.now() - start, "ms");
    return result;
  };
}

decorator cache(target, meta) {
  let cacheMap = {};
  
  return fn(...args) {
    let key = JSON.stringify(args);
    if (key in cacheMap) {
      print("💾 Cache hit");
      return cacheMap[key];
    }
    print("💾 Cache miss");
    let result = target(...args);
    cacheMap[key] = result;
    return result;
  };
}

@log
@time
@cache
fn expensiveCalculation(n) {
  let sum = 0;
  for (let i = 0; i < n; i = i + 1) {
    sum = sum + i * i;
  }
  return sum;
}

// First call: cache miss, timed, logged
expensiveCalculation(1000);

// Second call: cache hit, timed, logged
expensiveCalculation(1000);
```

### Order of Execution

When stacking decorators:

```adeshlang
@decorator1
@decorator2
@decorator3
fn myFunction() { ... }
```

Execution order:
1. `decorator3` wraps the original function
2. `decorator2` wraps the result of decorator3
3. `decorator1` wraps the result of decorator2

## Best Practices

### 1. Keep Decorators Pure

Decorators should be pure functions that don't have side effects on the original target.

```adeshlang
// ✅ Good: Returns new function
decorator good(target, meta) {
  return fn(...args) {
    // wrapper logic
    return target(...args);
  };
}

// ❌ Bad: Modifies target directly
decorator bad(target, meta) {
  target.modified = true;  // Don't do this
  return target;
}
```

### 2. Preserve Function Signature

Use rest parameters (`...args`) to handle any number of arguments.

```adeshlang
decorator preserve(target, meta) {
  return fn(...args) {
    return target(...args);
  };
}
```

### 3. Document Decorator Behavior

Add clear comments explaining what the decorator does.

```adeshlang
/**
 * @decorator deprecate
 * Marks a function as deprecated and warns when called.
 * @param {string} message - Deprecation message
 * @returns {function} Wrapped function that logs warning
 */
decorator deprecate(message) {
  return fn(target, meta) {
    return fn(...args) {
      print("⚠️ WARNING:", meta.name, "is deprecated:", message);
      return target(...args);
    };
  };
}
```

### 4. Handle Errors Gracefully

```adeshlang
decorator safeCall(target, meta) {
  return fn(...args) {
    try {
      return target(...args);
    } catch (error) {
      print("Error in", meta.name, ":", error);
      return null;
    }
  };
}
```

### 5. Use Meaningful Names

Choose descriptive names that clearly indicate the decorator's purpose.

```adeshlang
// ✅ Good names
@authorize
@validate
@memoize
@deprecated

// ❌ Bad names
@dec
@wrapper
@func
```

## Common Use Cases

### 1. **Logging and Debugging**
- Track function calls
- Log arguments and return values
- Measure execution time

### 2. **Validation**
- Validate input arguments
- Check preconditions
- Enforce type constraints

### 3. **Authorization**
- Check user permissions
- Verify authentication
- Role-based access control

### 4. **Caching**
- Memoize expensive calculations
- Cache API responses
- Implement LRU cache

### 5. **Error Handling**
- Retry failed operations
- Provide fallback values
- Log errors consistently

### 6. **Performance**
- Throttle function calls
- Debounce rapid invocations
- Rate limiting

## ⚠️ FIXED: Decorator Hang Bug

**RESOLVED**: The hang with 6+ decorators has been fixed! The issue was decorators being applied twice (during pre-registration and during execution), causing exponential wrapper nesting.

### What Was Fixed
- Decorators are now applied exactly once per function definition
- No more hang with multiple decorated functions in the same file
- Significantly improved performance

### Test Results
```
10 decorators on single function: ✅ Works perfectly
15 decorators on single function: ✅ Works perfectly
Multiple functions with decorators: ✅ Works  perfectly
```

### Best Practices
- Use 1-10 decorators per function for optimal performance
- Complex decorator chains (15+) may have some performance overhead
- Split very complex decorator logic across multiple simpler decorators

## Working Examples

### File 1: Advanced Performance (`advanced_performance.adesh`) ✅
Tests decorator performance and scalability:
- 10 stacked decorators
- Multiple decorated functions
- Performance benchmarking

```bash
cargo run -- run examples/decorators/advanced_performance.adesh
```

### File 2: Advanced Patterns (`advanced_patterns.adesh`) ✅
Real-world decorator patterns:
- **Performance Monitoring** - Track slow functions
- **Smart Caching with TTL** - Time-based cache invalidation
- **Input Validation** - Type and value checking
- **Rate Limiting** - Prevent excessive calls
- **Auto Retry** - Automatic retry logic
- **Debug Tracing** - Detailed execution logging

```bash
cargo run -- run examples/decorators/advanced_patterns.adesh
```

## Working Examples (18 Patterns in 3 Files)

### File 1: Basic Decorators (`1_basic_decorators.adesh`) ✅
6 fundamental patterns:
- **Logging** - Trace function calls and returns
- **Caching** - Store and reuse computed values  
- **Validation** - Check input parameters
- **Retry** - Implement retry logic
- **Transformation** - Modify string outputs
- **Metadata** - Access function information

```bash
cargo run -- run examples/decorators/1_basic_decorators.adesh
```

### File 2: Advanced Decorators (`2_advanced_decorators.adesh`) ✅
6 composition patterns:
- **Stacking** - Multiple decorators on one function
- **Wrapping** - Add delimiters around results
- **Conditional** - Execute based on conditions
- **Type Checking** - Validate argument types
- **Transformation** - Modify return values
- **State Tracking** - Count function calls

```bash
cargo run -- run examples/decorators/2_advanced_decorators.adesh
```

### File 3: Practical Decorators (`3_practical_decorators.adesh`) ✅
6 real-world patterns:
- **Authorization** - Control access to functions
- **Formatting** - Convert data formats
- **Error Boundaries** - Prevent runtime errors
- **Memoization** - Cache with input keys
- **Deprecation** - Warn about old functions
- **Null Safety** - Handle null inputs

```bash
cargo run -- run examples/decorators/3_practical_decorators.adesh
```

## Other Known Limitations

1. **No spread operators** - Use fixed-arity wrappers: `fn(a, b)` not `fn(...args)`
2. **No while loops** - Causes environment corruption in decorated functions
3. **Method decorators lose `this`** - Use standalone functions with instance parameter
4. **Unique variable names** - Each decorator needs distinct variable names

See `docs/DECORATOR_LIMITATIONS.md` for complete technical analysis.

## Legacy Examples Index

| File | Description | Status |
|------|-------------|--------|
| `function_log.adesh` | Basic function logging decorator | ✅ |
| `stacking_log_cache.adesh` | Multiple decorators stacked together | ⚠️ Check count |
| `log_measure.adesh` | Logging and timing decorators | ✅ |
| `factory_retry.adesh` | Decorator factory with retry logic | ✅ |
| `property_prefix.adesh` | Static property decorator example | ✅ |
| `class_sealed.adesh` | Class decorator for sealing classes | ✅ |
| `validate_authorize.adesh` | Validation and authorization decorators | ✅ |
| `method_decorators.adesh` | Comprehensive method decorator examples | ⚠️ See limitations |
| `advanced_patterns.adesh` | Advanced decorator patterns | ⚠️ Check count |
| `practical_examples.adesh` | Real-world decorator use cases | ⚠️ See limitations |
| **NEW: `1_basic_decorators.adesh`** | **6 working basic patterns** | ✅ Tested |
| **NEW: `2_advanced_decorators.adesh`** | **6 working advanced patterns** | ✅ Tested |
| **NEW: `3_practical_decorators.adesh`** | **6 working practical patterns** | ✅ Tested |

## Conclusion

Decorators in AdeshLang provide a clean, declarative way to enhance your code. They promote:

- **Separation of concerns**: Keep business logic separate from cross-cutting concerns
- **Code reuse**: Write decorators once, use them everywhere
- **Maintainability**: Change behavior without modifying original functions
- **Readability**: Clear indication of enhanced behavior at definition site

Start with simple decorators and gradually build more complex patterns as needed!
