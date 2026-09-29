# Adesh Runtime ABI Specification (`ADESH_RUNTIME_ABI_V1`)

`ADESH_RUNTIME_ABI_V1` defines the formal contract between compiler, object format, linker, and runtime.

---

## 📜 Core Runtime Symbols

- `__adesh_alloc(size: usize, align: usize) -> *mut u8`: Thread-safe memory allocation.
- `__adesh_free(ptr: *mut u8, size: usize, align: usize)`: Thread-safe deallocation.
- `__adesh_panic(msg: *const u8, len: usize)`: Runtime panic handler.
- `__adesh_thread_init()`: Thread-local state initializer.
