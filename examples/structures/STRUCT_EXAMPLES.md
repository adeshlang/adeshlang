# AdeshLang Structure Examples

Comprehensive examples demonstrating struct definition, instantiation, and usage in AdeshLang.

## Overview

Structures (structs) in AdeshLang allow you to group related data with different types into a single composite type. This directory contains examples showing all aspects of working with structures.

## File Descriptions

### 1. basic_struct.adesh
**Level**: Beginner
**Topics**: Basic struct definition, instantiation, field access

Demonstrates:
- Defining a struct with multiple fields
- Creating instances of structs
- Accessing fields from struct instances
- Creating multiple instances of the same struct

**Example**:
```adesh
struct Person {
    name: String;
    age: Int;
    isStudent: Bool;
};

let person1 = Person {
    name: "Alice",
    age: 20,
    isStudent: true,
};

print(person1.name);  // Alice
print(person1.age);   // 20
```

### 2. struct_fields.adesh
**Level**: Beginner
**Topics**: Field access, field types, field operations

Demonstrates:
- Accessing individual fields from structs
- Working with different field types (String, Int, Float, Bool)
- Using struct fields in operations
- Field type operations

**Covered Field Types**:
- String fields
- Integer fields
- Float fields
- Boolean fields

### 3. nested_struct.adesh
**Level**: Intermediate
**Topics**: Nested structures, composition, accessing nested fields

Demonstrates:
- Defining structs that contain other structs
- Creating instances with nested data
- Accessing nested fields (dot notation chaining)
- Practical usage with Address and Employee example

**Example**:
```adesh
struct Address {
    street: String;
    city: String;
};

struct Employee {
    name: String;
    address: Address;
};

let emp = Employee {
    name: "John",
    address: Address { street: "123 Main", city: "NYC" }
};

print(emp.address.city);  // NYC
```

### 4. mixed_types.adesh
**Level**: Intermediate
**Topics**: Mixed field types, type combinations, practical examples

Demonstrates:
- Creating structs with various field type combinations
- Working with structs containing String, Int, Float, and Bool
- Practical examples: measurements, features, mixed data

**Use Cases**:
- Configuration structures
- Data records
- Feature flags
- Measurement data

### 5. struct_functions.adesh
**Level**: Intermediate
**Topics**: Passing structs to functions, returning structs, struct as parameters

Demonstrates:
- Functions that accept struct parameters
- Functions that return struct instances
- Creating structs inside functions
- Multiple struct parameters in functions

**Example**:
```adesh
fn printUser(u: User) {
    print(u.username);
}

fn createUser(name, email, age) {
    return User {
        username: name,
        email: email,
        age: age,
    };
}
```

### 6. struct_collections.adesh
**Level**: Intermediate
**Topics**: Working with multiple struct instances, managing collections

Demonstrates:
- Creating multiple instances of the same struct
- Processing collections of structs
- Organizing struct data
- Practical applications with inventory, scores, contacts

## Struct Syntax Reference

### Basic Definition
```adesh
struct StructName {
    fieldName1: Type1;
    fieldName2: Type2;
    fieldName3: Type3;
};
```

### Creating Instances
```adesh
let instance = StructName {
    fieldName1: value1,
    fieldName2: value2,
    fieldName3: value3,
};
```

### Accessing Fields
```adesh
instance.fieldName1
instance.fieldName2.nestedField
```

## Field Types

AdeshLang structs support the following field types:

- `String` - Text data
- `Int` - Integer values
- `Float` - Decimal numbers
- `Bool` - Boolean (true/false)
- Other structs (for nesting)

## Common Patterns

### Data Container
```adesh
struct User {
    username: String;
    email: String;
    age: Int;
};
```

### Nested Structure
```adesh
struct Address { /* ... */ };
struct Person {
    name: String;
    address: Address;
};
```

### Configuration
```adesh
struct Config {
    enabled: Bool;
    maxRetries: Int;
    timeout: Float;
};
```

### Record
```adesh
struct Score {
    playerName: String;
    points: Int;
};
```

## Testing

All examples have been tested and verified to work correctly:

```bash
# Run individual examples
cargo run --bin adeshlang -- run examples/structures/basic_struct.adesh
cargo run --bin adeshlang -- run examples/structures/struct_fields.adesh
cargo run --bin adeshlang -- run examples/structures/nested_struct.adesh
cargo run --bin adeshlang -- run examples/structures/mixed_types.adesh
cargo run --bin adeshlang -- run examples/structures/struct_functions.adesh
cargo run --bin adeshlang -- run examples/structures/struct_collections.adesh
```

## Learning Path

1. **Start with**: `basic_struct.adesh` - Learn struct definition and instantiation
2. **Then explore**: `struct_fields.adesh` - Master field access
3. **Progress to**: `nested_struct.adesh` - Understand composition
4. **Deep dive**: `mixed_types.adesh` - Work with various types
5. **Advanced**: `struct_functions.adesh` - Use structs with functions
6. **Practice**: `struct_collections.adesh` - Work with multiple instances

## Key Concepts

### Encapsulation
Group related data together in a single type for better organization.

### Type Safety
Struct fields have defined types, preventing type mismatch errors.

### Composition
Build complex types by nesting structs within other structs.

### Reusability
Define once, create many instances with the same structure.

### Function Integration
Pass structs to functions and return struct instances for flexible design.

## Best Practices

1. **Clear Naming**: Use descriptive struct and field names
2. **Single Responsibility**: Keep structs focused on related data
3. **Type Correctness**: Choose appropriate field types
4. **Documentation**: Comment complex structs
5. **Composition**: Use nesting for complex hierarchies

---

**Created**: December 2024
**Target**: AdeshLang v0.2+
**Status**: All examples tested and working
