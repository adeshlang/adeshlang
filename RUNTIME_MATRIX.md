# Adesh Standalone Runtime Matrix

**Scope:** Implemented behavior in `crates/adesh-runtime`; this is not a list
of planned platform APIs.

## 1. Build and Runtime Selection

The native linker adds the `adesh_runtime` archive by default and extracts
members needed to resolve program symbols. The CLI does not currently provide
the `--runtime=minimal|static|dynamic|none` profiles described in older
versions of this document. Native Windows PE execution is the platform
validated by the current end-to-end tests. ELF and Mach-O linker pipeline
tests are not OS runtime tests.

## 2. Implemented Runtime Features

| Feature | Current behavior | Limits |
| :--- | :--- | :--- |
| Runtime values and handles | `RuntimeValue` store with `aot_*` construction, access, and removal exports. | Handle lifetimes are caller-managed; there is no automatic collection policy. |
| Collections | Arrays, tuples, sets, objects, strings, ranges, and collection helpers. Array/object index and field updates preserve existing handle identity. | Native language coverage is partial. |
| Strings and output | String conversion/concatenation helpers, value printers, and raw native literal output (`aot_print_cstr`). | Only selected FFI exports catch Rust panics. |
| Allocation | `aot_alloc`/`aot_free` (untracked escape hatch), `adesh_mem_alloc`/`adesh_mem_free`/`adesh_mem_realloc`, plus tracked-allocation APIs. | User `alloc`/`free` from native HIR lower to `adesh_rt_alloc_tracked`/`adesh_rt_free_tracked`. Closures/enums still use untracked `aot_alloc` until ownership transfer is modeled. |
| Tracked scopes | `adesh_rt_scope_enter`, `adesh_rt_alloc_tracked`, `adesh_rt_free_tracked`, `adesh_rt_scope_exit`, and `adesh_rt_validate_ptr`; scope tokens distinguish nested scopes with reused IDs. | Native HIR lowering emits scope enter/exit for function/block frames and abandons nested scopes on `break`/`continue`/`throw` (Phase 19, 2026-11-21). |
| ARC and weak references | Strong/weak handles, dead-object detection, and safe weak upgrade behavior. | Native `share`/`weak` lower to `adesh_rt_arc_new`/`adesh_rt_weak_new` with matching drops on scope exit. |
| Threading | Mutex/condition-variable wrappers and a shared reusable worker pool; parallel iteration submits work to the pool and contains task panics. | Small ranges execute inline. This is not an async executor. |
| Filesystem and input | Selected `aot_fs_*` helpers and input exports. | Not a comprehensive cross-platform OS abstraction. |

## 3. ABI and Safety Notes

The runtime exposes both `aot_*` functions used by native lowering and
`adesh_*` functions from the versioned C ABI. Examples include
`adesh_abi_version`, `adesh_str_new`, `adesh_arr_new`, `adesh_mem_alloc`,
`aot_make_array`, `aot_make_string`, and `aot_set_index`.

Functions that accept raw pointers require the caller to provide valid memory
for the full length/count passed. A null check cannot make an arbitrary
non-null pointer safe. Panic containment is present on selected exports, not
all of them, so consumers must also obey each function's documented preconditions.

## 4. Platform Notes

The runtime is implemented in Rust and uses Rust `std` plus selected `libc`
calls; it does not provide the previously listed direct OS mappings for every
feature. For example, the native literal-output path uses Win32 calls on
Windows and POSIX `write` on non-Windows targets. Do not infer implemented
thread, timer, network, dynamic-library, or WASI support from this table.
