# india-lang-legacy.md

> Consolidated from 1 documentation files on 2026-08-29.

---


---

## Source: IndiaLang_Documentation.md

# AdeshLang Programming Language Documentation

## Table of Contents
1. [Introduction](#introduction)
2. [Basic Syntax](#basic-syntax)
3. [Data Types](#data-types)
4. [Variables and Constants](#variables-and-constants)
5. [Functions](#functions)
6. [Control Flow](#control-flow)
7. [Object-Oriented Programming](#object-oriented-programming)
8. [Collections](#collections)
9. [Asynchronous Programming](#asynchronous-programming)
10. [Error Handling](#error-handling)
11. [Modules and Imports](#modules-and-imports)
12. [Type System](#type-system)
13. [Built-in Functions](#built-in-functions)
14. [Advanced Features](#advanced-features)

## Introduction

AdeshLang is a modern programming language that combines familiar syntax with powerful features for both synchronous and asynchronous programming. It supports object-oriented programming, functional programming paradigms, and includes built-in support for promises, timers, and advanced data structures.

## Basic Syntax

### Comments
```ind
// Single-line comment

/*
 * Multi-line comment
 * Can span multiple lines
 */

/** 
 * Documentation block
 * @param {number} x description
 * @returns {number} description
 */
```

### Basic Operations
```ind
let a = 10;
let b = 5;
let c = a * 2 + b / 5 - 3;
print("a:", a, "b:", b, "c:", c);
```

## Data Types

### Primitive Types
```ind
// Numbers
let integer = 42;
let float = 3.14;
let bigint = 100n;  // BigInt literals with 'n' suffix

// Strings
let message = "Hello World";
let concatenated = "Hello" + " " + "AdeshLang!";

// Booleans
let flag = true;
let isValid = false;

// Null
let empty = null;
```

### Complex Numbers
```ind
let z = complex(3, 4);  // Built-in complex number support
print(z);
```

## Variables and Constants

### Variable Declaration
```ind
let x;           // Uninitialized variable (becomes null)
let y = 42;      // Initialized variable
export let msg = "Hello";  // Exported variable
```

### Type Annotations
```ind
let a: int = 5;
let myarr: array<number> = [5, 6];
let bad: int = "hello";  // Type error (but still parseable)
```

## Functions

### Function Declaration
```ind
fn add(x, y) {
    return x + y;
}

// Function with default parameters
fn greet(name = "Guest", title = null) {
    if (title == null) {
        print("Hello " + name);
    } else {
        print(title + " " + name);
    }
}
```

### Arrow Functions
```ind
// Single parameter
let inc = x => x + 1;

// Multiple parameters with defaults
let add = (a, b = 2) => a + b;

// Block body
let greet = (name) => { 
    print("Hi " + name); 
};
```

### Rest Parameters and Spread Operator
```ind
// Rest parameters
fn testSimple(...args) {
    print("Args:", args);
}

// Spread operator
let arr = [1, 2, 3];
testSimple(...arr);

let arr1 = [10, 20];
let arr2 = [...arr1, 30, 40];
```

### Async Functions
```ind
async fn delayedAdd(a, b) {
    return a + b;
}

async fn compute() {
    let a = Promise(fn(res,_){ res(2); });
    let x = await a;
    return x + 5;
}
```

## Control Flow

### Conditional Statements
```ind
let n = 12;

if (n % 2 == 0) {
    print(n, "is even");
} else {
    print(n, "is odd");
}
```

### Loops

#### While Loop
```ind
let i = 1;
while (i <= 3) {
    print("Loop iteration:", i);
    i = i + 1;
}
```

#### For-In Loop
```ind
let nums = [1, 2, 3, 4, 5];
for (n in nums) {
    print(n);
}

// Range iteration
for (i in 1..5) {      // Exclusive range
    print(i);
}

for (i in 1...5) {     // Inclusive range
    print(i);
}
```

#### Loop Control
```ind
for (i in [1,2,3,4]) {
    if (i == 2) {
        continue;  // Skip to next iteration
    }
    if (i == 3) {
        break;     // Exit loop
    }
    if (i == 4) {
        jump 4;    // Jump statement
    }
    print(i);
}
```

## Object-Oriented Programming

### Classes

#### Basic Class with Constructor
```ind
class Person {
    constructor(name, age) {
        self.name = name;
        self.age = age;
        self.id = Math.random();
    }

    greet() {
        print("Hi, I'm", self.name, "and I'm", self.age, "years old.");
    }
}

let p = new Person("Ajay", 25);
p.greet();
```

#### Method Overloading
```ind
class Calculator {
    // Multiple methods with same name but different parameters
    add(a, b) {
        return a + b;
    }
    
    add(a, b, c) {
        return a + b + c;
    }
    
    add(numbers) {  // Array parameter
        return numbers.reduce((sum, n) => sum + n, 0);
    }
}

let calc = new Calculator();
print(calc.add(2, 3));           // Uses first overload
print(calc.add(1, 2, 3));        // Uses second overload  
print(calc.add([1, 2, 3, 4]));   // Uses third overload
```

#### Operator Overloading
```ind
class Vector {
    constructor(x, y) {
        self.x = x;
        self.y = y;
    }
    
    // Arithmetic operators
    operator+(other) {
        return new Vector(self.x + other.x, self.y + other.y);
    }
    
    operator-(other) {
        return new Vector(self.x - other.x, self.y - other.y);
    }
    
    operator*(scalar) {
        return new Vector(self.x * scalar, self.y * scalar);
    }
    
    // Comparison operators
    operator==(other) {
        return self.x == other.x && self.y == other.y;
    }
    
    operator<(other) {
        return self.magnitude() < other.magnitude();
    }
    
    // Index operators
    operator[](index) {
        if (index == 0) return self.x;
        if (index == 1) return self.y;
        throw "Index out of bounds";
    }
    
    operator[]=(index, value) {
        if (index == 0) self.x = value;
        else if (index == 1) self.y = value;
        else throw "Index out of bounds";
    }
    
    // String conversion
    operator toString() {
        return "(" + self.x + ", " + self.y + ")";
    }
    
    magnitude() {
        return Math.sqrt(self.x * self.x + self.y * self.y);
    }
}

let v1 = new Vector(3, 4);
let v2 = new Vector(1, 2);

let v3 = v1 + v2;              // Uses operator+
print("Sum:", v3.toString());   // Uses operator toString

print("Equal?", v1 == v2);     // Uses operator==
print("V1[0]:", v1[0]);        // Uses operator[]
v1[1] = 10;                    // Uses operator[]=
```

### Inheritance and Method Overriding

#### Basic Inheritance
```ind
class Animal {
    constructor(name) { 
        self.name = name; 
    }
    
    speak() {
        print("Some generic animal sound");
    }
}

class Dog extends Animal {
    constructor(name, breed) { 
        super(name);  // Call parent constructor
        self.breed = breed;
    }
    
    // Method overriding
    speak() { 
        print("Woof! I'm " + self.name + ", a " + self.breed);
    }
    
    // Method overloading in subclass
    speak(loud) {
        if (loud) {
            print("WOOF WOOF! I'm " + self.name.toUpperCase());
        } else {
            self.speak();  // Call the no-parameter version
        }
    }
}
```

#### Abstract Classes and Methods
```ind
abstract class Shape {
    constructor(name) {
        self.name = name;
    }
    
    // Abstract methods - must be implemented by subclasses
    abstract calculateArea();
    abstract calculatePerimeter();
    
    // Concrete method
    describe() {
        print("This is a " + self.name + " with area " + self.calculateArea());
    }
}

class Circle extends Shape {
    constructor(radius) {
        super("Circle");
        self.radius = radius;
    }
    
    // Implementation of abstract methods
    calculateArea() {
        return 3.14159 * self.radius * self.radius;
    }
    
    calculatePerimeter() {
        return 2 * 3.14159 * self.radius;
    }
}

let circle = new Circle(5);
circle.describe();  // Uses inherited method with overridden abstract method
```

### Interfaces
```ind
interface Speakable {
   public fn speak();
}

class Dog extends Animal implements Speakable {
    fn speak() { 
        print("Woof " + this.name); 
    }
}
```

### Access Modifiers
```ind
class Secret {
    private fn secret() { 
        print("secret"); 
    }
    
    public fn reveal() { 
        this.secret(); 
    }
}
```

### Static Methods and Properties
```ind
class MathUtils {
    static PI = 3.14159;
    static E = 2.71828;
    
    static add(a, b) {
        return a + b;
    }
    
    static multiply(a, b) {
        return a * b;
    }
    
    // Static method overloading
    static max(a, b) {
        return a > b ? a : b;
    }
    
    static max(numbers) {  // Array version
        return numbers.reduce((max, n) => n > max ? n : max, numbers[0]);
    }
    
    // Static operator overloading
    static operator+(a, b) {
        return MathUtils.add(a, b);
    }
}

// Static usage
print("PI:", MathUtils.PI);
print("Add:", MathUtils.add(5, 3));
print("Max of two:", MathUtils.max(10, 7));
print("Max of array:", MathUtils.max([3, 7, 2, 9, 1]));
```

### Property Getters and Setters
```ind
class Rectangle {
    constructor(width, height) {
        self._width = width;
        self._height = height;
    }
    
    // Getter properties
    get area() {
        return self._width * self._height;
    }
    
    get perimeter() {
        return 2 * (self._width + self._height);
    }
    
    // Setter properties with validation
    set width(value) {
        if (value <= 0) {
            throw "Width must be positive";
        }
        self._width = value;
    }
    
    set height(value) {
        if (value <= 0) {
            throw "Height must be positive";
        }
        self._height = value;
    }
    
    // Getter for private properties
    get width() {
        return self._width;
    }
    
    get height() {
        return self._height;
    }
}

let rect = new Rectangle(5, 3);
print("Area:", rect.area);          // Uses getter
print("Perimeter:", rect.perimeter); // Uses getter

rect.width = 7;                     // Uses setter
print("New area:", rect.area);      // Recalculated via getter
```

### Extensions
```ind
// Anonymous extension
extend on Person {
    fn salute() { 
        print("Salute " + self.name); 
    }
}

// Named extension
extend HelloExt on Person {
    fn hello() { 
        print("Hello " + self.name); 
    }
}
```

## Collections

### Arrays
```ind
let arr = [1, 2, 3, 4];
let matrix = [
    [1, 2, 3],
    [4, 5, 6],
    [7, 8, 9]
];

// Array methods
arr.append(5);
arr.extend([6, 7]);
arr.insert(0, 0);
let last = arr.pop();
let first = arr.pop(0);
arr.sort();
arr.reverse();
arr.clear();

// Array operations
let count = arr.count(1);
let index = arr.index(3);
```

### Objects
```ind
let person = {
    name: "Ajay",
    age: 25,
    skills: ["Rust", "JS", "Next.js"]
};

print(person.name);
print(person["age"]);
```

### Tuples
```ind
let t = (1, 2, 3);
let single = (42,);
let empty_tuple = ();
let from_array = tuple([9, 8, 7]);
```

### Sets
```ind
let s = {1, 2, 2, 3};  // Duplicates removed
let from_arr = set([1, 2, 2, 4]);

// Set operations
let s1 = set([1,2,3]);
let s2 = set([3,4]);
let union = s1.union(s2);
let intersection = s1.intersection(set([2,3]));
let with_added = s1.add(5);

// Set queries
let contains = s1.contains(2);
```

## Asynchronous Programming

### Promises
```ind
let p = Promise(fn(res, rej) {
    res(5);  // Resolve with value
});

// Promise chaining
let p2 = p.then(fn(x) { return x * 2; });
let p3 = p2.then(fn(y) { return y + 1; });

// Error handling
let p_err = Promise(fn(_res, rej) { 
    rej("oops"); 
});
p_err.then(null, fn(e) { 
    print("caught:", e); 
});
```

### Async/Await
```ind
// Top-level await
let p2 = Promise(fn(res,_){ res(10); });
let v = await p2;
print("result:", v);

// Async function
async fn fetchData() {
    let data = await someAsyncOperation();
    return data;
}
```

### Timers
```ind
// setTimeout
setTimeout(fn() { 
    print("timeout fired"); 
}, 2000);

// setInterval
let iid = setInterval(fn() { 
    print("interval tick"); 
}, 1000);

// Clear interval
clearInterval(iid);
```

## Error Handling

### Try-Catch
```ind
try {
    throw "Something went wrong!";
} catch (err) {
    print("Caught error:", err);
}

// Custom error handling
fn divide(a, b) {
    if (b == 0) {
        throw "Division by zero!";
    }
    return a / b;
}

try {
    let result = divide(10, 0);
} catch (err) {
    print("Error:", err);
}
```

## Modules and Imports

### Exporting
```ind
// utils.ind
export class Person {
    fn init(name, age) {
        this.name = name;
        this.age = age;
    }
}

export fn factorial(n) {
    // implementation
}
```

### Importing
```ind
// main.ind
import utils as mod;

let p = new mod.Person("Ravi", 30);
p.greet();
```

## Type System

AdeshLang supports optional type annotations:

```ind
let a: int = 5;
let arr: array<number> = [1, 2, 3];
let name: string = "John";
```

## Built-in Functions

### Core Functions
```ind
// Length and size
len(arr)        // Get length of array/string/collection
len(s)          // Get size of set

// Time
clock()         // Get current timestamp

// Type checking
type(value)     // Get type of value

// Functional programming
map(array, fn)  // Map function over array
```

### Higher-Order Functions
```ind
let nums = [1, 2, 3];
let doubled = map(nums, fn(x) => x * 2);
let tripled = map(nums, (x) => x * 3);
```

## Enhanced Object-Oriented Programming Features

AdeshLang now supports advanced OOP features including:

### Constructor Support
- Use `constructor` keyword instead of `init`
- Support for `self` keyword (preferred over `this`)
- Automatic constructor chaining with `super()`

### Method Overloading
- Multiple methods with the same name but different parameter counts
- Automatic resolution based on argument count at runtime
- Works with both instance and static methods

### Operator Overloading
- Arithmetic operators: `+`, `-`, `*`, `/`, `%`
- Comparison operators: `==`, `!=`, `<`, `<=`, `>`, `>=`
- Index operators: `[]`, `[]=`
- String conversion: `toString`
- Custom operators for user-defined types

### Static Methods and Properties
- Class-level methods and properties using `static` keyword
- Accessed directly on the class without instantiation
- Support for static method overloading

### Property Getters and Setters
- `get` and `set` keywords for property accessors
- Automatic property validation and computed properties
- Encapsulation of internal state

### Enhanced Inheritance
- Method overriding with proper `super` support
- Abstract classes and methods with `abstract` keyword
- Interface implementation validation

### Backward Compatibility
- All existing code using `init` and `this` continues to work
- Gradual migration path to new features
- Mixed usage of old and new syntax in same codebase

## Advanced Features

### Structs
```ind
struct Point {
    x,
    y
}

let p = new Point(3, 4);
print(p.x, p.y);
```

### Enums
```ind
enum Result {
    Ok(value),
    Err(msg)
}

let r1 = new Result.Ok(200);
let r2 = new Result.Err("not found");

print(r1.tag);    // "Ok"
print(r1.value);  // 200
```

### Range Operators
```ind
1..5    // Exclusive range [1, 2, 3, 4]
1...5   // Inclusive range [1, 2, 3, 4, 5]
```

### Closures
```ind
let base = 100;
let addBase = (x) => x + base;
print(addBase(5));  // 105

base = 200;
print(addBase(5));  // 205 (captures updated value)
```

## Example Programs

### Factorial with BigInt
```ind
fn factorial(n) {
    let acc = 1n;
    let i = 2n;
    while (i <= n) {
        acc = acc * i;
        i = i + 1n;
    }
    return acc;
}

let result = factorial(100n);
print("Factorial:", result);
```

### Async Data Processing
```ind
async fn processData() {
    let data = await fetchFromAPI();
    let processed = map(data, (item) => item * 2);
    return processed;
}

processData().then(fn(result) {
    print("Processed:", result);
});
```

This documentation covers the core features of AdeshLang as demonstrated in the example files. The language provides a rich set of features for modern programming including async/await, promises, object-oriented programming, functional programming constructs, and advanced data structures.

