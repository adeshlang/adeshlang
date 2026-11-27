# Adesh ABI & Calling Convention Specification Matrix

**Generated:** 2026-10-01  
**Scope:** Target ABI Rules, Register Usage, and Stack Alignment

**Low-width note (2026-11-27):** On i686 (`System V i386 cdecl`), integer
scalars and pointers occupy 4-byte stack slots, while `f64` arguments occupy
8-byte stack slots and `f64` values return in x87 `ST(0)` (`f32` is refused).
Runtime helpers whose C prototypes take 64-bit parameters (`aot_print_cstr`,
`aot_print_i64`, `aot_print_u64`, `aot_print_bool`, `adesh_native_print_cstr`)
have their arguments explicitly marshalled by `wide_helper_params` /
`marshal_wide_helper_args` in `src/backends/native/lower.rs` into two cdecl
32-bit words (low word first, sign/zero-extended high word second). Signed
narrow/32-bit values print via `aot_print_i32_narrow` and `f64` prints via
`aot_print_f64_narrow`. Genuine language-level `i64`/`u64` operations and
user function signatures remain explicitly refused on i686. Static single-thread
local-exec TLS uses `%gs:[0]` + Variant II negative offsets (`R_386_TLS_LE`).

---

## 1. Primary Calling Conventions

| ABI Name | Target Architecture | Integer Arg Registers | FP / Vector Arg Registers | Return Registers | Callee-Saved Registers | Shadow Space | Stack Alignment |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **System V AMD64** | x86_64 (Linux, macOS, BSD) | `RDI, RSI, RDX, RCX, R8, R9` | `XMM0 - XMM7` | `RAX, RDX` / `XMM0, XMM1` | `RBX, RSP, RBP, R12, R13, R14, R15` | None (128-byte red zone) | 16 Bytes |
| **System V i386 cdecl (verified scalar + x87 `f64` subset)** | i686 Linux | Stack arguments (4-byte scalar/pointer slots; 8-byte `f64` slots; 2-slot marshalling for listed 64-bit runtime helpers) | None (stack-passed 8-byte `f64`; `f32` refused) | `EAX` (int/ptr) / `ST(0)` (`f64`) | `EBX, EBP, ESI, EDI` | None | 4-byte scalar / 8-byte `f64` stack slots; genuine `i64`/`u64` and aggregates refused |
| **Microsoft x64** | x86_64 (Windows) | `RCX, RDX, R8, R9` | `XMM0 - XMM3` | `RAX` / `XMM0` | `RBX, RBP, RDI, RSI, RSP, R12 - R15, XMM6 - XMM15` | 32 Bytes (Mandatory) | 16 Bytes |
| **AAPCS64** | AArch64 (Linux, macOS, Windows) | `X0 - X7` | `V0 - V7` | `X0, X1` / `V0, V1` | `X19 - X28, X29 (FP), X30 (LR), D8 - D15` | None | 16 Bytes |
| **Standard RISC-V**| RISC-V (RV64GC. RV32 is refused before it can be mis-encoded: `create_backend` rejects the architecture, and the linker refuses to synthesize its RV64 `_start` stub for an RV32 executable) | `a0 - a7` (`x10 - x17`) | `fa0 - fa7` (`f10 - f17`) | `a0, a1` / `fa0, fa1` | `s0 - s11` (`x8, x9, x18 - x27`), `fs0 - fs11` | None | 16 Bytes |
| **AAPCS32 (Thumb)**| ARM Cortex-M / ARM32 | `r0 - r3` | `s0 - s15` / `d0 - d7` | `r0, r1` / `s0, s1` | `r4 - r11, r13 (SP), r14 (LR)` | None | 8 Bytes |
| **WASM ABI** | WASM32 / WASM64 | Stack / WASM Params | WASM Params | Stack / Multi-Value | Controlled by WebAssembly VM | None | Native |

---

## 2. Foreign Function Interface (FFI) ABI Mapping

`extern` declarations are parsed (`src/parsing/parser/extern_ffi.rs`, `extern` as a
`TokenKind` in `src/parsing/lexer.rs`) and the declared ABI string is resolved by
`AbiType::from_str` in `src/backends/common/ffi/import.rs`, which accepts exactly
these seven (anything else is the loud error `Unknown ABI '<x>'`, and
`is_valid_for_platform()` additionally rejects an ABI that does not apply to the host):

* `"c"` — the platform's standard C ABI (System V on Linux/macOS, Microsoft x64 on Windows).
* `"sysv"` — System V AMD64 ABI (Linux/BSD/macOS x86-64).
* `"windows"` — the Windows x64 calling convention.
* `"rust"` — Rust FFI (same machine calling convention as `c`).
* `"go"` — Go via cgo.
* `"wasm"` — a WebAssembly import, limited to `i32`, `i64`, `f32`, `f64` parameters.
* `"adesh"` — the Adesh internal calling convention. **Not implemented**: the variant is
  labelled "future extension" in `import.rs`, so there is no multi-register-return or
  fat-pointer convention to select yet.

There is **no** `"system"` and **no** `"native"` ABI string — declaring either fails at
registration with `Unknown ABI`.

**Reachability (Phase 18):** `extern "C"` / `"SysV"` declarations are retained in
HIR (`HirModule.externs`) and native-lowered to `NativeModule.imports` with C-ABI
marshalling (cstr / scalars / null pointers). Language-reachable and
execution-verified on **Linux x86-64** (`puts`/`fflush`), **Linux AArch64**
(qemu-user `puts`), and **macOS** both Apple arches (`macho-dyld-gate`). C-style
variadic (`...`) is language-reachable via `printf` on Linux x86-64 (SysV `%al`
set at the call site). Other ABI strings are refused loudly on the native path.
The interpreter still registers externs through libffi. SysV POD aggregate
*returns* (≤8 / ≤16 INTEGER, and sret for >16-byte) plus ≤8-byte *arguments*
are language-reachable on Linux x86-64 (`div`/`lldiv`/`make_big`/`pair_sum`).
`thread_local let` is language-reachable on **Linux x86-64** (ELF IE),
**Linux AArch64** (local-exec under qemu), **Windows x86-64** (TEB /
`_tls_index`, host-gated), and **macOS** both Apple arches (Mach-O TLV
`__thread_vars` / `__tlv_bootstrap`, host-gated via `macho-dyld-gate`).
`atomic let` + `atomic_fetch_add` are language-reachable on **Linux x86-64**
and **Linux AArch64** (qemu). Windows PE source `puts` / TLS are covered by
the PR `windows-pe-gate` (msvcrt + TEB). **Phase 18 exit criteria met** on
tip `da2a8f9` (2026-11-22): Light + Workspace + `macho-dyld-gate` +
`windows-pe-gate` + cargo audit green.

---

## 3. Aggregate & Struct Passing Rules

1. **Primitives ($\le 8$ bytes)**: Passed directly in a single integer or floating-point register.
2. **Small Structs ($\le 16$ bytes)**:
   - System V AMD64: Split into eightbytes; classified as INTEGER / SSE and passed in up to 2 registers.
   - Microsoft x64: Passed by value if 1, 2, 4, or 8 bytes; otherwise passed by reference via an implicit pointer.
   - AAPCS64: Up to 16 bytes passed in consecutive `Xn` registers or Homogeneous Floating-point/Vector Aggregates (HFA/HVA) in `Vn`.
3. **Large Structs ($> 16$ bytes)**: Passed via hidden first pointer argument (`sret` / return slot) allocated on the caller stack.
