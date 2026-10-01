# Adesh Runtime ABI Specification (`ADESH_RUNTIME_ABI_V1`)

`ADESH_RUNTIME_ABI_V1` defines the unified, zero-copy, C-compatible binary interface across all Adesh execution backends: **Interpreter**, **Bytecode VM**, **Native JIT**, and **Native AOT / ADOB Linker**.

---

## 🏛️ In-Memory Data Structures (C-ABI Compatible)

### 1. `AdeshString` (32 Bytes on 64-bit Systems)
```rust
#[repr(C)]
pub struct AdeshString {
    pub ptr: *const u8,   // UTF-8 byte buffer (null-terminated for C-FFI)
    pub len: usize,       // Length in bytes (excluding null terminator)
    pub cap: usize,       // Total buffer capacity
    pub flags: u32,       // Bitflags: STATIC (0x1), MUTABLE (0x2), SSO (0x4)
    pub reserved: u32,    // Alignment padding (future expansion)
}
```

### 2. `AdeshSlice<T>` & `AdeshArray`
```rust
// Immutable View Slice (16 Bytes)
#[repr(C)]
pub struct AdeshSlice<T> {
    pub data: *const T,
    pub len: usize,
}

// Resizable Dynamic Array (32 Bytes)
#[repr(C)]
pub struct AdeshArray {
    pub data: *mut u8,
    pub len: usize,
    pub cap: usize,
    pub elem_size: usize,
}
```

### 3. `AdeshOption` & `AdeshResult` Tagged Unions
```rust
// Standardized Tagged Option
#[repr(C)]
pub struct AdeshOptionU64 {
    pub tag: u8,          // 0 = None, 1 = Some
    pub _pad: [u8; 7],    // 7-byte alignment padding
    pub value: u64,       // 64-bit payload
}

// Standardized Tagged Result
#[repr(C)]
pub struct AdeshResultU64 {
    pub tag: u8,          // 0 = Ok, 1 = Err
    pub _pad: [u8; 7],    // 7-byte alignment padding
    pub payload: u64,     // Value payload or error code
}
```

### 4. Closures & Fat Pointers
```rust
// Standard Function Closure
#[repr(C)]
pub struct AdeshClosure {
    pub fn_ptr: *const (),
    pub env_ptr: *mut (),
    pub drop_fn: Option<unsafe extern "C" fn(*mut ())>,
}

// Interface / Trait Object Fat Pointer
#[repr(C)]
pub struct AdeshFatPointer {
    pub data_ptr: *mut (),
    pub vtable_ptr: *const (),
}
```

---

## ⚡ Linkable C-ABI Export Symbols

The native runtime exports standard C-ABI functions linkable by both the internal `adeshlink` linker and external toolchains:

| Function | Signature | Description |
|---|---|---|
| `adesh_abi_version` | `() -> u32` | Returns active ABI version (e.g. `1`) |
| `adesh_str_new` | `(ptr: *const c_char, len: usize) -> AdeshString` | Allocates new managed string |
| `adesh_str_from_cstr` | `(c_str: *const c_char) -> AdeshString` | Converts null-terminated C string |
| `adesh_str_concat` | `(s1: AdeshString, s2: AdeshString) -> AdeshString` | High-speed string concatenation |
| `adesh_str_free` | `(s: AdeshString)` | Frees heap-allocated string |
| `adesh_arr_new` | `(elem_size: usize, cap: usize) -> AdeshArray` | Allocates resizable array |
| `adesh_arr_push` | `(arr: *mut AdeshArray, elem: *const u8) -> i32` | Appends element to array |
| `adesh_arr_get` | `(arr: *const AdeshArray, idx: usize) -> *mut u8` | Gets pointer to element |
| `adesh_arr_free` | `(arr: *mut AdeshArray)` | Deallocates array memory |
| `adesh_mem_alloc` | `(size: usize, align: usize) -> *mut u8` | Thread-safe memory allocation |
| `adesh_mem_free` | `(ptr: *mut u8, size: usize, align: usize)` | Thread-safe deallocation |
| `adesh_mem_realloc` | `(ptr: *mut u8, old: usize, new: usize, align: usize) -> *mut u8` | Reallocates memory |
| `adesh_panic_abort` | `(msg: *const c_char, len: usize) -> !` | Aborts execution with panic log |

---

## 📦 Precompiled Native Standard Library (`libadesh_std.adob`)

The standard library runtime is precompiled into a multi-target `AdobBundle` (`libadesh_std.adob`). When compiling user code with `adesh build`, `adeshlink` performs zero-copy symbol resolution directly from precompiled ADOB bundles, delivering sub-50ms native builds without recompiling the standard library.
