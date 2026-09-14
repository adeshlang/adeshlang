# Pointer Examples

Examples demonstrating smart pointer usage in AdeshLang.

## Files

| File | Description |
|------|-------------|
| `tree_structure.adesh` | Building tree data structures with Shared<T> for children and Weak<T> for parent back-references. Demonstrates safe cycle-breaking patterns. |

## Key Concepts

### Shared<T>
Reference-counted ownership. Multiple owners can hold references to the same data.

```adesh
let data = Shared.new({ value: 42 });
let copy = data.clone();  // Same data, ref_count++
```

### Weak<T>
Non-owning reference that doesn't prevent deallocation. Used to break cycles.

```adesh
let shared = Shared.new({ value: 42 });
let weak = shared.downgrade();
let upgraded = weak.upgrade();  // Returns null if data was freed
```

### Unique<T>
Move-only exclusive ownership. Transfer invalidates the original.

```adesh
let data = Unique.new({ value: 42 });
let moved = data.move();
// data is now invalid
```

## Running Examples

```bash
adesh run examples/pointers/tree_structure.adesh
```

## Related Documentation

- [Memory Model](../../docs/memory_model.md)
- [Examples Index](../../docs/examples.md)
