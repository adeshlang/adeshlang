# AdeshLang Struct Quick Reference

Fast lookup guide for struct syntax and common patterns in AdeshLang.

## Quick Start

### Define a Struct
```adesh
struct Person {
    name: String;
    age: Int;
    active: Bool;
};
```

### Create an Instance
```adesh
let person = Person {
    name: "John",
    age: 30,
    active: true,
};
```

### Access Fields
```adesh
print(person.name);    // John
print(person.age);     // 30
```

## Common Patterns

### Pattern 1: Simple Data Container
```adesh
struct Book {
    title: String;
    author: String;
    year: Int;
};

let book = Book {
    title: "1984",
    author: "George Orwell",
    year: 1949,
};
```

### Pattern 2: Nested Structures
```adesh
struct Address {
    street: String;
    city: String;
};

struct Person {
    name: String;
    address: Address;
};

let person = Person {
    name: "Alice",
    address: Address {
        street: "123 Main St",
        city: "NYC",
    },
};

print(person.address.city);  // NYC
```

### Pattern 3: Mixed Types
```adesh
struct Product {
    name: String;
    price: Float;
    quantity: Int;
    available: Bool;
};
```

### Pattern 4: Function Parameters
```adesh
fn displayProduct(p: Product) {
    print(p.name);
    print(p.price);
}

let product = Product { /* ... */ };
displayProduct(product);
```

### Pattern 5: Return from Function
```adesh
fn createUser(name, email, age) {
    return User {
        username: name,
        email: email,
        age: age,
    };
}

let user = createUser("bob", "bob@example.com", 25);
```

## Field Types

| Type | Description | Example |
|------|-------------|---------|
| String | Text | "Hello World" |
| Int | Integer | 42 |
| Float | Decimal | 3.14 |
| Bool | Boolean | true/false |
| Struct | Another struct | Address, Price |

## Common Mistakes to Avoid

❌ **Incorrect**: Missing semicolon after field
```adesh
struct Bad {
    name: String,   // Wrong - should be ;
};
```

✅ **Correct**: Use semicolon
```adesh
struct Good {
    name: String;
};
```

❌ **Incorrect**: Comma in initialization
```adesh
let person = Person {
    name: "John";
    age: 30;
};
```

✅ **Correct**: Use comma in initialization
```adesh
let person = Person {
    name: "John",
    age: 30,
};
```

## Examples by Category

### Beginner Level
- `basic_struct.adesh` - Definition and instantiation
- `struct_fields.adesh` - Field access

### Intermediate Level
- `nested_struct.adesh` - Nested structures
- `mixed_types.adesh` - Multiple field types
- `struct_functions.adesh` - Functions with structs

### Advanced Level
- `struct_collections.adesh` - Multiple instances
- `ecommerce_example.adesh` - Real-world application

## Struct Organization Tips

1. **Group Related Data**
   ```adesh
   struct Address {
       street: String;
       city: String;
       zipcode: String;
   };
   ```

2. **Use Descriptive Names**
   ```adesh
   struct CustomerProfile {
       displayName: String;
       emailAddress: String;
       registrationDate: Int;
   };
   ```

3. **Leverage Nesting**
   ```adesh
   struct Order {
       customer: Customer;
       items: [Product];
       status: String;
   };
   ```

4. **Combine Simple Types**
   ```adesh
   struct Price {
       amount: Float;
       currency: String;
   };
   
   struct Product {
       name: String;
       price: Price;
   };
   ```

## Testing Your Structs

Always test:
1. Struct definition compiles
2. Instance creation works
3. Field access returns correct values
4. Nested fields access properly
5. Functions accept/return structs correctly

## Performance Notes

- Structs are stack-allocated by default
- Fields are stored inline
- No garbage collection overhead
- Nested structs add nested field storage

## Limitations & Notes

- Struct mutation requires reassignment
- Fields must be explicitly named
- No implicit type conversion
- No automatic equality comparison

---

**Version**: AdeshLang v0.2+
**Last Updated**: December 2024
