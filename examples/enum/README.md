# AdeshLang Enum Examples & Documentation

Enums in AdeshLang act as tagged sum types (or unions) that allow you to represent algebraic data types.

## Defining Enums

Enums are defined using the `enum` keyword with braces enclosing the variants. Variants can optionally have a single named payload identifier wrapped in parentheses:

```adesh
enum Message {
    Quit,
    Move(coords),
    Write(text)
}
```

## Instantiating Enum Variants

Enum variants can be instantiated by referencing their constructor properties via dot notation:
- Instantiating without `new` or with `new` is fully supported and equivalent.
- Variants without payloads are represented as constructors that produce tag-only structures.
- Passing a single argument to a variant constructor binds that argument as the `.value` payload.
- Passing multiple arguments to a variant constructor binds them as a `Value::Array` payload.

```adesh
let msg1 = Message.Quit;
let msg2 = Message.Move(100, 200); // msg2.value is [100, 200]
let msg3 = new Message.Write("Hello");
```

## Tag and Payload Matching

Every instantiated enum variant acts as an object containing the following keys:
1. `__enum`: The name of the parent enum type (string).
2. `tag`: The name of the variant constructor (string).
3. `value`: The payload value (if any).

You can perform safe pattern matching and control flow by checking `.tag` and `.value` fields:

```adesh
if (msg.tag == "Move") {
    let coords = msg.value;
    print("Coordinates:", coords[0], coords[1]);
}
```

## Built-in Option Enum

AdeshLang comes with a pre-registered `Option` enum globally available. You can use its variants `Some(val)` and `None` directly for optional values:

```adesh
let name = Some("Ajay");
let guest = None;
```
