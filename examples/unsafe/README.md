# Unsafe Examples

- `unsafe_alloc_free.adesh`
  - Manual allocation with `alloc`/`free` inside an explicit `unsafe` block.
  - Run: `adesh run examples/unsafe/unsafe_alloc_free.adesh`
  - Expected output:
    ```
    manual allocation demo
    first byte 0
    caller responsible for safety
    ```
  - Notes: borrow checking and ARC do not apply inside `unsafe`; caller must guarantee validity.
