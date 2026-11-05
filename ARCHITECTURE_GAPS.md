# Adesh Native Production Toolchain — Architecture Gaps Analysis

**Generated:** 2026-10-05 (Phase 0 documentation truth-reset revision)
**Last updated:** 2026-11-05 (§3.5: the `x86_64-apple-darwin` link blocker is **closed**,
and it was never the writer's fault — the x86_64 backend materialized a symbol address as
`movabs` + an absolute relocation inside `.text`, which a PIE Mach-O image cannot carry
because dyld rebases only writable segments. The backend now selects the addressing form
by **image** format (`lea`+PC-relative on Mach-O, `movabs` on PE/ELF). Two gaps of the
documentation itself came out of that: a guard written against
`TargetDescriptor::object_format` cannot express "which image will this become", since
that field names the emitted container and is always `Adob`, so `image_format()` now
exists; and `LNK009` refused an address without naming what produced it, which is why a
compiler bug read as a linker bug for a day — the refusal now carries object, section,
offset and symbol. Earlier: 2026-11-04 (Phase 12 item 4: §3.5 now records that a cross
link is **tested** — `tests/native_phase12_cross_link_test.rs` links a source program for
four targets and asserts on the produced image bytes — and that doing so exposed two
architectural gaps the packaging work had been hiding: the AArch64 backend referenced
string-literal symbols it never defined (`LNK001`, fixed), and a real source link for
`x86_64-apple-darwin` failed `LNK009` and could not link at all (closed the next day).
Earlier that day, 2026-11-03 (Phase 12 items 1–3): §1.5 adds the
wrong-architecture runtime fallback the AOT driver used to make, now closed; §3.5
records that resolution is target-scoped, that `scripts/build-runtime-archives.sh` plus
packaging and the CI `runtime-archives` job are what produce a per-triple archive, and
that the job has never run so no non-host archive exists yet).
Earlier: 2026-11-02 (Phase 11: §1.5 recorded the silent-fallback gate
and the one fallback left in the linker; §2.1, §3.3, §3.4 and §3.5 were
re-checked against the code and the suites that now cover them)
**Canonical status source:** `CURRENT_STATE.md` (this file must agree with it)
**Scope:** Gaps between Current Baseline and Self-Contained Native Production Targets

---

## 1. Compiler Frontend & IR Lowering Gaps

### 1.1 Native IR Lowering Coverage (`HIR -> Machine IR`)
* **Current State:** `src/backends/native/lower.rs` lowers integers,
  arithmetic, comparisons, If/While/ForIn/Break/Continue, match (compare
  chains), float arithmetic via SSE2, strings/concat, ranges, membership,
  arrays/index mutation, methods, short-circuit values, try/catch, and
  defer. **Default executable builds use the native path**; Cranelift is
  opt-in (`--codegen=cranelift`). `--emit-adob` / `--emit-asm` emit
  intermediate artifacts only.
* **Architectural Gap — statement coverage (tracked in
  `IMPLEMENTATION_PLAN.md` Phase 1):**
  - Match/pattern lowering still emits per-arm compare chains; the
    `opt/switch_lowering.rs` (jump table) pass exists but is **not wired
    into the native path**.
  - Silent fallbacks must become structured compile errors. **Largely done since
    this gap was recorded** (see `CURRENT_STATE.md` §2): `src/backends/native/lower.rs`
    now calls `self.unsupported(...)` for unknown literals, unknown enum variant
    patterns, `instanceof`, `async fn`, `await`, and async lambdas, and the driver
    reports them together as "native backend cannot compile this program (N
    unsupported construct(s))" instead of emitting wrong code. The remaining
    unknowns are the ones that still need an audit: imports dropped mid-lowering and
    lambda capture environments (verify against `lower.rs` before claiming either).
  - Defer/panic lowering and runtime C ABI call coverage beyond the
    implemented `aot_*` set; complete composite-type support (today all
    composites are boxed runtime handles, not value types).

### 1.2 Parallel-Move Resolution at Call Sites — RESOLVED
* **Status:** Resolved. `ParallelMoveResolver`
  (`crates/adesh-codegen/src/calling_convention/parallel_move.rs`) handles
  register chains, 2-cycles, N-cycles, stack↔register, stack↔stack,
  immediates, and mixed GPR/XMM moves with scratch registers; it is wired
  into native call lowering and exercised by
  `tests/native_parallel_move_e2e_test.rs`.
* **Remaining gap:** the FFI lowerer (`crates/adesh-codegen/src/ffi/mod.rs`)
  still emits sequential argument moves and bypasses the resolver; its
  `HiddenSret` return path silently emits no move. Both are tracked for
  Phase 1/2.

### 1.3 Float / SIMD Support in Native Codegen — SCALAR FP RESOLVED
* **Status:** Scalar floating point is implemented and execution-tested:
  SSE2 scalar encoders (addss/subss/mulss/divss and double forms,
  `ucomiss`/`ucomisd`, `cvt*` conversions), XMM0-13 allocation, Win64
  nonvolatile XMM6-13 handling, and scalar FP argument/return registers
  (XMM0-3 Win64 / XMM0-7 SysV). Packed SSE encodings exist in the encoder.
* **Architectural Gap:** vector/SIMD argument classification for the C ABI,
  `comiss`/`comisd` forms, wider atomics (only `lock xadd` /
  `lock cmpxchg` on 64-bit GPRs exist), and SIMD lowering from source
  (`opt/vectorization.rs` is not wired).

### 1.4 Heterogeneous IR (Device Kernels)
* **Current State:** GPU/NPU/TPU modules produce ADOB objects containing
  metadata and re-embedded source bytes. The container writers use custom
  formats (not NVIDIA fatbin, not AMDGPU MsgPack code objects); the one
  SPIR-V writer emits a fixed empty shader; the MLIR GPU path shells out to
  external `mlir-opt`/`llc`; there are zero CUDA/HIP/Vulkan driver calls.
* **Architectural Gap:** structured translation of device kernels into
  real PTX/AMDGCN/SPIR-V embedded in ADOB `.gpu_kernel` sections, plus a
  kernel-launch runtime. **Parked** until the core targets are solid
  (`IMPLEMENTATION_PLAN.md`).

### 1.5 Machine IR → Target Encoding: the Silent-Fallback Gate — CLOSED IN CODEGEN AND IN RUNTIME RESOLUTION, ONE LEFT IN THE LINKER
* **Status (2026-11-02):** No codegen backend emits a placeholder for something
  it cannot encode. AArch64 (2026-11-01) removed the `_ => { NOP }` final arm —
  it is now `unsupported =>`, so even a variant added later fails — and refuses
  29 unencodable `MachineInstruction` variants plus 14 operand forms; RISC-V
  (same day) refuses 21 variants and 43 operand forms, replacing every
  placeholder-register `unwrap_or`, the `sp`-based vector load/store, the
  `Parity`/`NotParity` collapse, the width defaults and the masked `shamt`.
  x86_64 routes every unencodable form through `unsupported()`, and the embedded
  and wasm backends each ship a loud-unsupported suite.
  The refusal now happens one level up as well: `create_backend` rejects
  `Architecture::RiscV32` before any body is encoded, and the linker refuses to
  hand an RV32 executable its RV64 `_start` stub. Pinned by
  `tests/native_phase11_codegen_loudness_test.rs`,
  `linker/tests/phase11_riscv32_entry_stub_test.rs`.
* **Same class, one level up (2026-11-03):** the AOT driver resolved a foreign
  target to the **host's** runtime archive. `get_static_runtime_lib` interleaved
  `target/debug/…` with `target/<triple>/debug/…` in one list and memoised the
  answer in a process-wide `OnceLock` with no triple in the key, so
  `--target aarch64-unknown-linux-gnu` could link x86-64 runtime objects — a
  silent wrong-architecture artifact rather than a wrong instruction. Resolution
  is now triple-scoped and the cache is keyed by triple; see §3.5 and
  `CURRENT_STATE.md` §0.8.
* **Same class, found while re-measuring it (2026-11-03):** the archive name
  list still accepted `adeshlang.*`. The root `adeshlang` crate is
  `crate-type = ["rlib", "cdylib"]`, so no staticlib of that name exists, and the
  hashed-`deps` search matched the cdylib's COFF **import library** instead —
  a valid archive that defines no `aot_*` symbol. Resolution reported success and
  the link died on `undefined reference to aot_print_cstr`, which sends the reader
  to the wrong file. `runtime_archive_names` now returns exactly the
  `adesh-runtime` staticlib name, and `adl repair` / `adl doctor` name the same
  file (they both pointed at `adeshlang.lib` before).
* **Remaining gap:** `linker/src/arch/mod.rs:44` maps `Arch::X86` to
  `X86_64Arch`, so an i686 link silently runs the 64-bit relocation handler.
  It is Tier 3 and un-advertised, but it is the same defect class the gate
  closed elsewhere: apply the rule (refuse `Arch::X86` with a named error) or
  implement an IA-32 handler.
* **Consequence of the gate, still open as capability work:** what used to be
  silently wrong is now simply unsupported — the first symbol materialization
  blocks RISC-V source builds (a `Move` from a `Symbol`, measured 2026-11-03)
  and `ParallelMove` has no encoding either, spilled FP/vector stack slots are
  refused on AArch64 (needs `LDR`/`STR Dd, [SP]`), spilled atomic operands and
  indexed `Load`/`Store` are refused on both. That is Phases 12–15, not this gate.

## 2. ABI & Calling Convention Gaps

### 2.1 Argument Classification and Variadic
* **Current State:** calling conventions provide argument/return register
  lists, stack-passed arguments, the Win64 32-byte shadow space (allocated
  in the caller), and scalar FP argument/return registers (XMM0-3 Win64 /
  XMM0-7 SysV). AAPCS64's callee-saved list is **correct** (X19–X28 and D8–D15
  are declared in `crates/adesh-codegen/src/abi/mod.rs` and preserved by the
  AArch64 prologue/epilogue, pinned by the STP/LDP assertions in
  `crates/adesh-codegen/src/targets/aarch64/mod.rs`) — the empty-list caveat
  that used to sit here predates the backend and no longer applies.
* **Architectural Gap:**
  - Struct-by-value classification (SysV register classes, memory
    fallback) in the *executable* path (the descriptive classifier in
    `crates/adesh-codegen/src/abi/mod.rs` is consumed by FFI lowering only).
  - Hidden return-slot pointers (sret) for large aggregates (the
    `ReturnLocation::HiddenSret` arm is currently a silent no-op in FFI
    lowering).
  - Variadic arguments: register save area, `va_list`, AL for SysV
    varargs; Win64 register spill to shadow space.
  - Vector/SIMD argument assignment beyond scalar FP.

### 2.2 Thread-Local Storage (TLS)
* **Current State:** PE TLS directory synthesis (`.tls` merge, `_tls_index`,
  `IMAGE_TLS_DIRECTORY64`) works. `TlsModel` is an enum serialized in ADOB
  metadata only; the TLS relocation kinds are inert (resolved as plain
  absolutes); ELF/Mach-O TLS input sections are a hard error; codegen emits
  no thread-pointer access sequences (`fs:`, `tpidr_el0`).
* **Architectural Gap:**
  - Codegen emission of TLS access sequences per model.
  - ELF: `PT_TLS` program header (constant defined but unused) plus
    DTPMOD/TPOFF dynamic relocation handling.
  - Mach-O: TLS section and `__DATA,__thread_vars` support.

## 3. Native Linker Gaps

### 3.1 Link-Time Optimization (LTO)
* **Status:** Section-level only. `--lto` / `--enable-lto` enable aggressive
  Identical Code Folding (`IcfMode::All`), recursive section GC
  (`gc_sections`), symbol stripping, and multi-pass dead data pruning. The
  CLI help text now describes this accurately (2026-10-05).
* **Architectural Gap:** true cross-module IR optimization. The
  machine-IR LTO engine (`crates/adesh-codegen/src/opt/lto.rs`: global dead
  function elimination + cross-module inlining) **is** instantiated — `--lto`
  routes native emission through `CompilerDriver` (`src/cli/build.rs:1148`),
  and `compile_and_emit_adob` runs the `LtoEngine` before lowering — but the
  CLI passes it a single module, so cross-module inlining has nothing to act
  on, and there is no `.adesh_ir` section carrying IR between compilation
  units (only `.adesh.meta`, `.adesh.drop_table`, `.adesh.unwind_map` exist).

### 3.2 DWARF / CodeView Debug Information — UNWIND WIRED, DEBUG SECTIONS STILL STRIP-ONLY
* **Current State:** the link path only strips debug sections.
  `Dwarf5Generator` (`.debug_info`/`.debug_abbrev`/`.debug_str` plus a real
  `.debug_line` state-machine program in `linker/src/debug.rs`) and
  `CodeViewGenerator` are exercised by their own unit tests and **are not
  invoked by any link path**. The unwind half of this section has moved on:
  `EhFrameGenerator::assemble_eh_frame` writes an ELF `.eh_frame` (one CIE
  matching the real emitted prologue, one FDE per sized function) plus
  `.eh_frame_hdr` from `linker/src/layout.rs:1006`, and
  `WindowsPdataGenerator` synthesizes PE `.pdata`/`.xdata` during layout
  (`linker/src/layout.rs:1040-1102`) — the earlier "no CIE/FDE writer, the
  pdata generator is never called" note was wrong on both counts.
* **What that verification does not cover:** nothing executes either table. The
  FDE ranges are checked by an independent byte-level decoder, and PE
  exception-directory bytes are parsed back, but no test raises a signal and
  unwinds through them. Debug-info emission and unwind **behavior** are
  separate gaps.
* **Architectural Gap:** invoking the DWARF 5 / CodeView generators from the
  real link path, CodeView PDB streams for Windows, and a runtime-unwinder test
  (`IMPLEMENTATION_PLAN.md` Phase 5; `NEXT_IMPLEMENTATION_PLAN.md` Phase 16).

### 3.3 Dynamic Shared Library (`.so`, `.dll`, `.dylib`) Synthesis — ELF EXECUTION-VERIFIED, PE/Mach-O EMISSION-TESTED
* **Status:** Artifact emission is implemented for all three formats:
  - ELF: `.dynamic`, `DT_SONAME`, `DT_NEEDED`, `.dynsym`, `.dynstr`,
    `.hash`, `PT_DYNAMIC`, `PT_GNU_RELRO` (covers `.dynamic` bytes only),
    `PT_INTERP`, and `ET_DYN`.
  - Mach-O: `MH_DYLIB`, `LC_ID_DYLIB`, `LC_LOAD_DYLIB`
    (`/usr/lib/libSystem.B.dylib`), `LC_BUILD_VERSION`, partitioned
    `LC_DYSYMTAB`, and `LC_DYLD_INFO_ONLY` bind/rebase plus an exports trie.
  - PE: DLL export tables and COFF import libraries.
  - OS Router: routes C library imports to `libc.so.6` on ELF and
    `libSystem.B.dylib` on macOS Mach-O.
* **Verified how:** ELF is **loaded and called** — `tests/native_phase8_dynamic_elf_test.rs`
  builds `libadd42.so` and drives it from a PIE caller through
  `dlopen`/`dlsym` with an asserted result, in CI on the Linux runners.
* **Architectural Gap:** PE **DLL** load + export call and Mach-O **dylib** load
  are still emission-tested only; nobody has executed either. ELF imports are
  resolved through the synthesized GOT/PLT for the covered relocation kinds, and
  the Mach-O dyld chain is decoded back but never honored by real dyld.

### 3.4 Mach-O Executability
* **Status (re-checked 2026-11-02):** Emission is further along than this
  section claimed when it was written. The writer synthesizes 64-bit Mach-O
  layouts (`MH_EXECUTE`/`MH_DYLIB`, 4GB `__PAGEZERO`, `LC_BUILD_VERSION`,
  `LC_LOAD_DYLIB`, `LC_MAIN`, `LC_SYMTAB`, `LC_DYSYMTAB`) **plus**
  `LC_DYLD_INFO_ONLY` with rebase and bind opcode streams, an exports trie, an
  indirect symbol table, and `__stubs`/`__got`/lazy-pointer sections that form a
  call → stub → lazy-pointer → binder chain. TLS emits `__thread_data`/
  `__thread_bss` and a `__thread_vars` descriptor table. Every one of those
  structures is verified by an independent parse-back decoder
  (`linker/tests/macho_dyld_binding_test.rs`), which is exactly the limit: **no
  produced Mach-O image has ever been run**, so dyld's own acceptance of the
  bind/rebase chain is unverified and the target is Tier 2, not Tier 1.
  Chained fixups (`LC_DYLD_CHAINED_FIXUPS`) are still not emitted.
* **Architectural Gap:** a real macOS execution test — link and run an artifact
  on the macOS runner with an asserted import and a TLV access (`NEXT_IMPLEMENTATION_PLAN.md`
  Phase 13/14). Until then no claim about Mach-O executability is defensible.

### 3.5 ELF Executables & Dynamic Linking — EXECUTION-VERIFIED FOR x86-64
* **Status (re-checked 2026-11-02):** static standalone executables are emitted
  (`ET_EXEC` with `_start` synthesis — argc/argv/envp setup and `exit`, gated on
  `entry_va == 0`) and **executed in CI with asserted exit codes**
  (`tests/native_phase3_linux_e2e_test.rs`). Dynamic objects carry a real
  GOT/PLT (`JUMP_SLOT`, `GOTPCREL`, `COPY`, PIE `RELATIVE`), `PT_TLS` with the
  `.tdata`/`.tbss` split, and `.eh_frame`/`.eh_frame_hdr`;
  `tests/native_phase8_dynamic_elf_test.rs` loads a produced `.so` through
  `dlopen`/`dlsym` and calls into it, and
  `tests/native_phase8_tls_ehframe_test.rs` executes local-exec and
  initial-exec TLS against a real loader.
* **Remaining Architectural Gap:** `.gnu.hash`, full RELRO, a runtime-unwinder
  test (`.eh_frame` is decoder-verified, not unwind-verified), and **no archive
  has been produced by CI.** Both halves moved on 2026-11-03. Resolution is
  scoped by triple: an `aarch64-unknown-linux-gnu` link stops with "No runtime
  archive for target …", every path it searched, and the command that would have
  produced one, and it is never offered the host archive (Phase 12 item 1,
  `linking.rs::runtime_archive_tests`). The producing side now exists too —
  `scripts/build-runtime-archives.sh` stages
  `target/<triple>/<profile>/libadesh_runtime.a`, `Dockerfile`,
  `scripts/release/package_unix.sh` and `build_windows_dist.ps1` ship
  `lib/<triple>/`, and the CI `runtime-archives` job runs the script for three
  Linux and two Apple triples (Phase 12 items 2 and 3). The job is dispatch-only
  and has never run, so the archives are still only on the machine that built
  them.
  The link question closed on 2026-11-04 (Phase 12 item 4,
  `tests/native_phase12_cross_link_test.rs`): with a hand-staged archive,
  `x86_64-unknown-linux-gnu`, `x86_64-unknown-linux-musl` and
  `aarch64-unknown-linux-gnu` each link a source program to an ELF whose
  `e_machine` matches the triple with the `aot_*` definitions present, and the
  assertions are on the image bytes rather than the exit code — which is what
  caught the two gaps this section is now about. (1) The AArch64 backend
  referenced `__str_<n>` without defining the string pool, so its first real link
  failed `LNK001`; `emit_object` emits `.rodata`/`.tls`/`.data` now. (2)
  `x86_64-apple-darwin` failed `LNK009` *after* resolving its archive, and this
  section first blamed the wrong component: it is not the Mach-O writer placing a
  fixup in `__TEXT` but the x86_64 **backend** baking an absolute symbol address
  into `.text` as `movabs` + `Absolute64`, which no PIE image can carry since dyld
  rebases only writable segments. **Closed 2026-11-05**: the backend picks the
  addressing form by image format (`lea`+`PcRelative32` on Mach-O, `movabs` on PE
  and ELF), and the triple links — 17,178-byte `MH_EXECUTE`+`MH_PIE` image whose
  `lea` was decoded against the section table and verified to land on the program's
  own string. Two structural lessons from fixing it, both still live risks. The
  guard is about the *image*, and the descriptor had no honest way to say that:
  `TargetDescriptor::object_format` names the emitted container (always `Adob`), so
  the first version of this fix was a branch that could never fire and failed
  silently instead of loudly. `TargetDescriptor::image_format()` now derives it from
  the operating system, and `image_format_comes_from_the_operating_system_not_the_emitted_container`
  pins the distinction. And the refusal itself was unactionable — it named an address
  and a rule but not the object, section or symbol behind the address, which is how a
  compiler bug spent a day documented as a linker bug. Mach-O rebase records now carry
  that provenance into the message. `aarch64-apple-darwin` cannot meet the same defect:
  the AArch64 backend emits no `Absolute64` relocation at all, so its blocker is only
  the archive no job has built. A `cross-link` CI job now consumes the
  `runtime-archives` artifacts and runs the suite on all three OSes; it, too, has never
  been dispatched.

### 3.6 Archive Interoperability
* **Current State:** `adeshlink ar` reads GNU/BSD archives (with symbol
  index rebuild) but writes archives **without a symbol-index member**, and
  silently skips unparseable `.lib` COFF import-library members.
* **Architectural Gap:** emit the `/` symbol index on write; thin-archive
  support; import-library member parsing.

## 4. Runtime & OS Abstraction Gaps

### 4.1 Native Platform Event Loop & Async Engine
* **Current State:** no async machinery exists in the runtime crate. The
  reusable `AdeshThreadPool` is implemented (worker reuse, per-task panic
  containment) and backs native parallel iteration; the cooperative
  `AsyncExecutor` is skeletal (ready queue + busy-loop `block_on`); there
  is no epoll/IOCP/kqueue reactor anywhere. The HTTP standard library uses
  Tokio; the interpreter has JS-style Promise/`setTimeout` queues. Native
  lowering compiles `Await`/`Spawn` as plain sequential expressions
  (tracked for a loud error in Phase 1).
* **Architectural Gap:** a zero-dependency reactor (Linux `epoll`/`io_uring`,
  Windows IOCP, macOS/BSD `kqueue`), an executor with real wakers, and
  async-fn → state-machine lowering in native codegen. **Parked** until
  Phase 1 makes async constructs fail loudly.

### 4.2 Stack Unwinding & Personality Routine
* **Current State:** panics abort or log; unwind table generators are
  test-only with zero-code xdata stubs; there is no CIE/FDE emitter.
* **Architectural Gap:** `.eh_frame` FDE/CIE writer plus a native
  personality routine (`__adesh_personality_v0`) for panic unwinding and
  defer cleanup.

## 5. Quantum Toolchain Gaps

### 5.1 End-to-End Quantum Execution Pipeline
* **Current State:** circuit building, OpenQASM 3.0 export, basis gate
  decomposition, topological routing, and the state-vector simulator are
  implemented in `linker/src/quantum/` (measurement is a deterministic
  threshold mock; the router leaves a SWAP insertion stub; `hybrid.rs`
  defines runtime symbol names without implementations). The main compiler
  has no quantum language constructs at all.
* **Architectural Gap:** language syntax and semantic analysis for
  `let q = qubit[2]; H(q[0]); CX(q[0], q[1]); let r = measure(q);`, JIT
  execution through the simulator, AOT compilation to QIR/QASM bundles, and
  eventually real hardware drivers (none exist; no cloud QPU integration).
  **Parked** (`IMPLEMENTATION_PLAN.md`).

## 6. Accelerator Codegen Gaps

### 6.1 Device ISA Emission and Launch Runtime
* **Current State:** `compile_kernel` re-embeds the kernel source as bytes;
  container writers require externally produced payloads and use invented
  formats (not real fatbin/HSACO/metallib); the one SPIR-V writer emits a
  fixed empty shader; the MLIR GPU path requires external
  `mlir-opt`/`mlir-translate`/`llc`; there are zero CUDA/HIP/Vulkan driver
  calls (no kernel can be launched). No Apple ANE code exists.
* **Architectural Gap:** real PTX/AMDGCN/SPIR-V generation from Adesh kernel
  code and a driver-based launch path. **Parked** until the core targets are
  solid; documentation must describe this subsystem as "container packaging
  scaffolding" until then.
