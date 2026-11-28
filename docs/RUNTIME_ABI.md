# Adesh Runtime ABI Specification (`ADESH_RUNTIME_ABI_V1`)

`ADESH_RUNTIME_ABI_V1` defines the unified, zero-copy, C-compatible binary interface across all Adesh execution backends: **Interpreter**, **Native JIT**, and **Native AOT / ADOB Linker**.

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

## 📦 Precompiled Native Standard Library (`libadesh_std.adob`) & Phase 19 Runtime ABI

The standard library runtime is built as target-aware ADOB / `AdobBundle` (`.adbb`) archives by `StdlibAdobBuilder` (`crates/adesh-codegen/src/stdlib_builder.rs`) and linked by default (`--no-std` excludes it). Modules with native runtime backing (`std:io`, `std:string`, `std:collections`, `std:math`, `std:time`, `std:fs`, `std:path`, `std:json`, `std:env`) resolve to real `adesh_std_*` / `adesh_rt_*` C-ABI symbols without Tokio or interpreter dependencies. Unsupported `std:*` modules (for example async/network runtime APIs) fail loudly at compile/link time with explicit diagnostics.

### Phase 19 Boundary, Lifetime & Unwinding ABI

| Surface | Symbol / Mechanism | Guarantees & Status |
|---|---|---|
| Handle validation | `HANDLE_TAG_MASK` (`0xDEAD_0000_0000_0000`), `HANDLE_TAG_PAYLOAD_MASK` | **Verified** — non-canonical / forged handles are rejected before dereference (`crates/adesh-runtime/src/native_abi.rs`). |
| Pointer provenance | `is_readable_ptr_range`, `checked_alloc_bytes` | **Verified** — rejects null, unmapped/invalid, zero/overflowing allocation sizes before FFI read/write. |
| FFI safety | `ffi_guard`, poison-tolerant locks, `MAX_SINGLE_IO_READ`, stdio FD guard | **Verified** — catches Rust panics at C-ABI boundaries, recovers poisoned mutexes safely, caps bounded I/O allocations, blocks closing FD 0/1/2. |
| Tracked scopes & allocs | `adesh_rt_scope_enter`, `adesh_rt_scope_exit`, `adesh_rt_alloc_tracked`, `adesh_rt_free_tracked` | **Verified on Linux x86-64** — LIFO cleanup of closures, enums, aggregates, and `defer` blocks across normal return, `break`, `continue`, and unwind exits. |
| Enum & ARC checks | `adesh_rt_enum_check_tag`, `adesh_rt_arc_set`, `adesh_rt_arc_drop`, `adesh_rt_arc_clone` | **Verified** — bounds-checked enum tags and CAS drop-race guard in `adesh_rt_arc_set`. |
| ELF x86-64 unwind | `.eh_frame`, `.eh_frame_hdr`, ALSD v1, `adesh_eh_personality` | **Verified on target platform (Linux x86-64)** — 3+ frame two-phase unwinding, rethrow, and `defer`/ARC cleanup (`native_phase19_memory_lifetime_test`). |
| Windows x64 SEH | `.pdata`, `.xdata` `UNWIND_INFO`, `adesh_win64_seh_handler` | **Implemented & Structurally Verified** on Linux (`native_phase19_win64_seh_test`); **Target-platform execution pending** on Windows CI (`windows-pe-gate`). |
