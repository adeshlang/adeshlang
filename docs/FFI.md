# Adesh Foreign Function Interface (FFI) & Cross-Language Interoperability

Adesh provides first-class support for C and Rust interoperability.

---

## 🔀 Foreign Function Interface Model (`adesh::ffi`)

### 1. Calling C Functions from Adesh
```adesh
extern "C" {
    fn strlen(ptr: *const u8) -> usize;
    fn malloc(size: usize) -> *mut u8;
    fn free(ptr: *mut u8);
}
```

### 2. Exporting Adesh Functions to C
```adesh
export fn adesh_add(a: i32, b: i32) -> i32 {
    return a + b;
}
```

### 3. FFI Panic Boundary Policy
- `panic = abort`: Immediately aborts process on panic at FFI boundary.
- `panic = catch`: Catches panics internally and returns default error values.
- `panic = translated_error`: Translates panics into C Result error struct wrappers.
