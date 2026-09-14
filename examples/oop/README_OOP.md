# AdeshLang OOP Examples

This directory contains comprehensive examples demonstrating AdeshLang's Object-Oriented Programming features.

## Current Features

### ✅ Working Features

1. **Classes** - Reference types with methods and inheritance
2. **Abstract Classes** - Non-instantiable with abstract method enforcement
3. **Interfaces** - Contract checking (static validation)
4. **Inheritance** - Single inheritance with method overriding
5. **Method Overloading** - Multiple methods with same name, different signatures
6. **Static Methods** - Class-level methods
7. **Constructors** - `init()` and `constructor()` methods
8. **Method Extension** - `extend on Type` for adding methods (AdeshLang unique)

### ⚠️ Planned Features

1. **Interface Dynamic Dispatch** - Runtime polymorphism through interfaces
2. **Struct Optimization** - Contiguous memory layout for value types
3. **Escape Analysis** - Stack allocation for non-escaping objects
4. **Vtable Optimization** - Compact vtables with devirtualization

## Examples

### Basic Class
```adesh
class Point {
    fn init(x, y) {
        this.x = x;
        this.y = y;
    }
    
    fn distance(other) {
        let dx = this.x - other.x;
        let dy = this.y - other.y;
        return sqrt(dx*dx + dy*dy);
    }
}

let p1 = new Point(0, 0);
let p2 = new Point(3, 4);
print("Distance:", p1.distance(p2));  // 5.0
```

### Abstract Classes
```adesh
abstract class Shape {
    abstract fn area();
    
    fn describe() {
        print("Area:", this.area());
    }
}

class Circle extends Shape {
    fn init(radius) {
        this.radius = radius;
    }
    
    fn area() {
        return 3.14159 * this.radius * this.radius;
    }
}

let c = new Circle(5);
c.describe();  // Area: 78.53975
```

### Method Extension (AdeshLang Unique)
```adesh
struct Vec2 {
    x: f32
    y: f32
}

extend on Vec2 {
    fn length() {
        return sqrt(this.x * this.x + this.y * this.y);
    }
}
```

### Interface Contracts
```adesh
interface Drawable {
    fn draw();
}

class Button implements Drawable {
    fn init(label) {
        this.label = label;
    }
    
    fn draw() {
        print("Button:", this.label);
    }
}
```

## Running Examples

```bash
# Run basic OOP test
adesh run examples/oop/oop_test_suite.adesh

# Run abstract class test
adesh run testing/06_oop/01_classes.adesh

# Run interface test
adesh run examples/oop/current_oop_test.adesh
```

## Architecture Notes

- **Classes**: Heap-allocated by default, use reference semantics
- **Structs**: Currently use HashMap (optimization planned)
- **Methods**: Stored per-class, shared across instances
- **Inheritance**: Single inheritance with method override support
- **Abstract Methods**: Enforced at class declaration time
- **Interfaces**: Validated at compile time (dynamic dispatch planned)

See `/docs/OOP_UNIFIED_SPECIFICATION.md` for complete specification.
