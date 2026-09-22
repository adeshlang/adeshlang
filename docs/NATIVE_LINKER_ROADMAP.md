# Native Linker Roadmap — Self-Contained AOT Builds

> **Status:** Draft v1 (2026-09-22) — design approved for sequencing; no code yet.
> **Scope:** Plan for removing the external LLVM/Clang/MSVC linker dependency from
> default `adesh build` (AOT) by shipping a first-party native linker for ELF and PE.
> **Companion doc:** [AOT_COMPILATION.md](AOT_COMPILATION.md) describes how AOT works **today**.

---

## 1. Why

`adesh build` today produces native executables via a fully in-process pipeline
(Cranelift codegen, `cranelift-object` for object emission) — **except the final
link step**, which shells out to external tools:

| Step | Mechanism | External tools |
|------|-----------|----------------|
| Parse → typecheck → lower → VIR → Cranelift → `.o`/`.obj` | In-process | none |
| Link object files → executable | `src/backends/aot/cranelift_impl/linking.rs` | Windows: `lld-link` + Windows SDK / MSVC import libs; Unix: `clang`/`gcc` + `lld`/`ld` |
| Static/shared library assembly | same file | `llvm-ar` (falls back to `ar`, which does not exist on a clean Windows box) |
| GPU backend | `src/backends/mlir/pipeline.rs` | `mlir-opt`, `mlir-translate`, `llc`, `clang` (whole path is `#[cfg(debug_assertions)]`, so release builds never invoke it — see `src/cli/backends.rs`) |

Cost of that one subprocess step, measured on the current dist tree:

- `adesh.exe`: **28.5 MB**, fully self-contained (interpreter, JIT, bytecode VM,
  Cranelift AOT object generation, in-process WASM encoder, embedded wasmtime).
- Bundled toolchain: **168 MB** (`clang.exe` 100 MB + `lld-link.exe` 68 MB).
- Fresh `adesh toolchain install` downloads full upstream LLVM: **374 MB**
  (Windows installer) / **~1 GB** (Linux, macOS tarballs) — see
  `installer/manifests/toolchain-manifest.json`.
- Windows users additionally need **VS Build Tools + Windows SDK** for import
  libraries (`kernel32.lib`, `msvcrt.lib`, …), discovered via `vswhere` /
  drive scanning in `linking.rs` (`find_windows_sdk_paths`, `find_msvc_lib_paths`).

So ~85% of the installed footprint exists to serve a single process spawn, and
the "install AdeshLang, build a native binary" story has a hard prerequisite
(Windows SDK) that no other backend has. Removing it is the difference between
a language that *uses* LLVM tooling and a language that *ships like Go*.

**Target experience:**

```
adesh build hello.adesh      # no GCC, Clang, LLVM, LLD, or MSVC installed
./hello                     # genuine native executable
```

---

## 2. Goal and non-goals

**Goal.** Default `adesh build` on Linux (x86-64, aarch64) and Windows (x86-64)
produces a correct, self-contained native executable with **zero external
toolchain**, backed by a differential-tested first-party linker. The existing
LLVM path remains available as an explicit fallback (`--linker=lld`) for
compatibility, cross-compilation, and libc-linked builds.

**Non-goals (explicitly deferred):**

- lld parity: shared libraries / dynamic linking (`DT_NEEDED`, PLT/GOT, copy
  relocations), LTO, DWARF/PDB emission, TLS, `.eh_frame`/unwind tables.
- A native Mach-O linker (see D2).
- 32-bit targets, RISC-V, and other architectures before x86-64 + aarch64 are
  solid.
- Removing lld from the codebase. It stays as the fallback and compat mode.

Each non-goal is a feature knob we can turn on later; none block the milestone
of toolchain-free default builds.

---

## 3. Key decisions

### D1 — The default runtime targets raw syscalls, not libc

This is the highest-leverage decision in the document. A freestanding
`exit(0)` executable needs no linker sophistication at all. The trouble
starts with `println`: today it resolves through
`src/runtime/c_runtime/adesh_runtime.c`, which is C against libc
(`printf`, `malloc`, ...). The moment the runtime needs libc, the linker needs
either static libc objects (musl) or full dynamic linking — the single item
that moves the timeline by months.

**Decision:** the freestanding runtime tier is written against raw syscalls:

- Linux x86-64: `write(2)`, `exit_group(2)`, `mmap(2)` for the allocator.
- Linux aarch64: same set via its syscall table.
- Windows: imports from `kernel32.dll` / `ntdll.dll` (`WriteFile`,
  `ExitProcess`/`TerminateProcess`, `GetStdHandle`, `GetProcessHeap`/
  `HeapAlloc`).

This is the Go model: Go's runtime is syscall-first, and that is the only
reason `cmd/link` can stay self-contained. AdeshLang's runtime keeps the
existing C/libc runtime as a separate tier ("compat mode") that continues to
link via lld/clang and the C runtime object. Both tiers share one ABI at the
`println`/allocation level; only the bottom layer differs.

**Consequence:** linker v1 scope is *static executables + fixed import set*.
No PLT, no GOT, no copy relocations.

### D2 — Own ELF and PE; Mach-O stays on the system linker

- **ELF (Linux/BSD):** first-party writer + linker. The formats and
  relocation sets needed for static executables are small and well specified.
- **PE/COFF (Windows):** first-party writer + import tables (D1). PE has no
  stable raw-syscall shortcut, so import tables are day-one scope, not an
  add-on. The structures (DOS/PE headers, sections, `.idata`/IAT, hint/name
  table) are simple and RVA-based.
- **Mach-O (macOS):** **not** first-party. arm64 macOS requires a valid ad-hoc
  code signature (page hashes in `LC_CODE_SIGNATURE`) and uses chained
  fixups, and every Mac already has Apple's linker via Command Line Tools.
  Even Zig — whose identity is self-contained cross-compilation — ships lld
  for Mach-O. macOS builds continue through the existing clang/lld path.

### D3 — Evolve `src/backends/aot/`, do not create a parallel tree

A linker skeleton already exists:

- `src/backends/aot/static_linker.rs` — `Linker`, `ObjectFile`,
  `Relocation { Absolute64, PCRel32, GOTRel }`, symbol resolution.
- `src/backends/aot/object_gen.rs` — `ObjectFormat::{ELF, MachO, COFF}`,
  `ObjectGenerator`, `DebugInfo` (line table, function ranges).
- `src/backends/aot/symbols.rs` — `Symbol`, `SymbolTable`.
- `src/backends/aot/abi.rs` — `Abi`, `CallingConvention`, `DataLayout`.

These are stubs (the ELF "generator" writes a magic number plus raw code;
relocations patch against a flat concatenation with no VMA layout), but the
module boundaries are right. The work is to make them real, in place. The
relocation enum becomes per-target (`R_X86_64_*`, `IMAGE_REL_AMD64_*`,
`R_AARCH64_*`), and a real layout step (VMA assignment, section alignment,
`PT_LOAD` mapping) replaces the offset concatenation. The `GOTRel` variant as
currently computed is wrong for real binaries and is removed with D1.

### D4 — lld remains the fallback, forever

`--linker=lld` keeps working for: libc/compat builds, cross-compilation to
targets the native linker does not cover, exotic linker flags, and as the
oracle for differential testing (§7). The native linker becomes the *default*
per target only after its differential suite is green in CI.

### D5 — Use the `object` crate for format mechanics; own the policy

The `object` crate is already in the dependency tree (via `cranelift-object`)
and parses real ELF/COFF/Mach-O relocatable files. We use it to **consume**
the genuine objects that `cranelift-object` emits — no hand-rolled parsers.
What we own is linking policy: symbol resolution, section layout,
relocation application, entry point, container emission. Writer-side,
executable images are small enough (ELF header + program headers + segments;
PE headers + sections + `.idata`) that hand-writing them is clearer than
fighting the `object` crate's relocatable-object writer.

### D6 — Two tiers of native emission

1. **Direct tier (fast path):** LIR → Cranelift (in-memory, the same
   mechanism `cranelift-jit` uses) → machine code bytes → wrap in a minimal
   container. No object-file round trip, no relocations when everything is
   one compilation unit with internal calls resolved at codegen time. This is
   days of work and unblocks the end-to-end experience early.
2. **Object tier:** parse real `.o`/`.obj` (via `object`), resolve symbols,
   apply relocations, lay out sections, emit. This is the actual linker, and
   it is what the multi-module runtime objects flow through.

### D7 — The test harness is milestone zero

An AI-written linker produces plausible code quickly; only differential
testing makes it trustworthy. The harness (§7) is built **before** the ELF
writer. No milestone is "done" until its corpus passes in CI.

### D8 — Implementation language: Rust, as an in-process library

The native linker is written in Rust inside the compiler crate
(`src/backends/aot/`), not as a standalone tool.

- **The in-process architecture settles it** (D6, §5): the linker consumes
  Cranelift's in-memory code buffers directly. A C/C++ component would need
  FFI boundaries and a C/C++ toolchain in the compiler's own build —
  reintroducing the dependency this roadmap exists to remove.
- **Safety:** a linker is offset arithmetic and byte patching into our own
  buffers. Rust makes that bounds-checked and endianness-explicit; C/C++
  linkers carry a known silent-corruption bug class. In Rust, layout bugs
  panic loudly in the M0 harness instead of emitting corrupt binaries.
- **Ecosystem, already in the dependency tree:** `object` (parsing),
  `cranelift-object` (object emission), `target-lexicon` (triples),
  `zerocopy`/`byteorder` (endianness-safe writes), `rayon` (parallel layout,
  later). Performance is a non-issue at Adesh's link scale — mold's C++
  exists for Chromium-scale links; ours are milliseconds either way.
- **Rejected:** C (no ecosystem advantage, unsafe for zero gain); C++ (only
  wins when embedding LLVM/lld — that is the keep-lld track, not this one);
  Zig (good fit for the domain, but a second build toolchain in a project
  about removing toolchains); **Adesh itself — circular bootstrap**: the
  linker is the component that makes Adesh AOT possible. Go followed the
  correct order here: Go 1.0's linker was written in C and only rewritten in
  Go for Go 1.5, after Go self-hosted. The equivalent path for AdeshLang:
  Rust linker (M0–M5) → Adesh AOT self-hosts → optional post-M6 "linker in
  Adesh" rewrite as a self-hosting showcase (brand value, not engineering
  value).

---

## 4. Milestones

Estimates assume focused solo development with heavy AI assistance and the
review/test loop described in §7. Serial dependency: M0 → M1 → (M2, M4 in
parallel) → M3 → M5.

| ID | Milestone | Est. | Done when |
|----|-----------|------|-----------|
| M0 | Differential test harness + corpus | 3–5 days | Same programs link with native path *and* lld, run, and compare exit codes + stdout byte-for-byte in CI (linux-x86_64, windows-x86_64) |
| M1 | Freestanding ELF writer (direct tier, no relocs) | 2–4 days | `adesh build --experimental-native` on a `fn main() { exit(0) }`-class program emits an ELF that Linux actually executes; ELF parsed by `object` crate in CI; `readelf` structure smoke checks |
| M2 | Static ELF linker, x86-64 (object tier) | 2–4 weeks | Multi-module programs with `.text`/`.rodata`/`.data`/`.bss`, symbol resolution, `R_X86_64_{64,PC32,PLT32,32S}`; syscall runtime `println` works; M0 corpus green |
| M3 | aarch64 ELF | 2–3 weeks | `R_AARCH64_{ABS64,ADR_PREL_PG_HI21,ADD_ABS_LO12_NC}` handling incl. ADRP+ADD pairs; corpus green on linux-aarch64 runner |
| M4 | PE writer + import tables (object tier) | 2–4 weeks | Windows console exe imports ~8 functions from `kernel32.dll`/`ntdll.dll`; runs on a clean Windows VM with no SDK/VS installed; corpus green |
| M5 | Runtime tier + default switch | 2–4 weeks | Freestanding runtime (println, allocator via mmap/HeapAlloc, program args) stable; native linker is the **default** on linux x64/aarch64 + windows x64; `--linker=lld` documented as fallback |
| M6+ | Deferred features | see §2 | shared libs, DWARF, TLS, unwind, Mach-O — re-scoped only after M5 has soaked in real usage |

**Total to the Go-like milestone (M0–M5): roughly 2–4 months of focused work.**
That is materially better than generic "6–12 months for a linker" estimates
because: the codegen already exists (Cranelift), the object emission already
exists (`cranelift-object`), the parsing comes from the `object` crate, the
runtime decision (D1) removes dynamic linking from critical scope, and Mach-O
is out of scope (D2).

---

## 5. Target architecture

```
                adesh build hello.adesh
                         │
        ┌────────────────┴────────────────┐
        │                                 │
   default target                  --linker=lld (fallback)
        │                                 │
  Adesh LIR → Cranelift          existing lld-link / clang+lld
        │                         path (unchanged, compat)
 ┌──────┴───────┐
 │ direct tier  │   single unit, in-memory codegen → minimal container
 └──────┬───────┘
        │ multi-module / runtime objects
 ┌──────┴───────┐
 │ object tier  │   .o/.obj (parsed via `object` crate)
 │  ├ layout    │   VMA assignment, section alignment, PT_LOAD/sections
 │  ├ resolve   │   symbols, relocations (per-target enums)
 │  └ emit      │   ELF writer | PE writer (import tables)
 └──────────────┘
```

Module evolution (all inside `src/backends/aot/`, per D3):

```
src/backends/aot/
├── static_linker.rs        → real: layout + resolution orchestration
├── object_gen.rs           → real: ELF / PE executable writers
├── symbols.rs              → as-is (symbol table)
├── abi.rs                  → per-target ABI/layout data
└── native/
    ├── elf/                → headers, program headers, segments
    ├── pe/                 → headers, sections, import tables
    └── relocs/             → x86_64.rs, aarch64.rs (apply logic)
```

Agent parallelization rule: agents implement **against the interfaces**
(`Relocation` enums per target, the layout step, the writer traits). Nobody
invents a format parser; nobody bypasses the layout step. All agents' work
lands behind the M0 harness.

---

## 6. Platform notes

### Linux (ELF)

Static executable = ELF header + one or two `PT_LOAD` program headers + bytes.
Minimum relocation set for Cranelift-emitted x86-64 objects:
`R_X86_64_64`, `R_X86_64_PC32`, `R_X86_64_PLT32` (direct calls in static
images), `R_X86_64_32S`/`32` (absolute compact forms — prefer rejecting these
and asking Cranelift for large-model-friendly encodings where possible).
aarch64 adds the ADRP+ADD pair handling; the pair must be kept in sync when
one of the two relocations moves across a 4 KB boundary during layout.

### Windows (PE/COFF)

- Import tables for a fixed, small API set (D1). Structures: import directory,
  IAT/thunks, hint/name table — all RVA arithmetic, no linker sophistication.
- Base relocations (`.reloc`): needed for ASLR-relocatable images. v1 emits a
  minimal `.reloc` block for the few absolute addresses in the image.
- Entry point is our own `_start` equivalent calling the Adesh runtime init,
  then `main`; exit via `ExitProcess`.
- Acceptance runs on a **clean Windows VM** with no Visual Studio, no SDK,
  no LLVM — the exact environment the current SDK-discovery code fails in.

### macOS (Mach-O)

Unchanged: clang/lld via Command Line Tools (D2). Revisit only if there is a
product reason to ship toolchain-free macOS builds; the cost is ad-hoc
signing + chained fixups, which is a project by itself.

---

## 7. Validation strategy (M0, built first)

1. **Corpus:** every `examples/` program that the freestanding runtime tier
   supports, plus dedicated linker fixtures (cross-module calls, relocations
   at section boundaries, bss-heavy programs, large rodata, weak/strong
   duplicates).
2. **Differential execution:** build each corpus program twice — native
   linker and lld — run both, compare **exit code and stdout byte-for-byte**.
   Any mismatch is a release blocker for default-switching.
3. **Structural checks:** both outputs parsed with the `object` crate;
   assert entry point, segment permissions, and section presence. Golden
   files for tiny binaries (hex-stable given fixed timestamps).
4. **Clean-room acceptance:** Windows corpus runs on a VM with zero
   toolchain; Linux corpus runs in the `Dockerfile.slim` image.
5. **Fuzzing (later):** the `object`-crate inputs are trusted (Cranelift
   output), so fuzzing priority is low; revisit when third-party `.o` input
   is accepted (FFI), not before.

The existing `#[cfg(debug_assertions)]`-gated MLIR GPU path is a reminder of
the failure mode to avoid: capabilities that exist in dev builds and rot. The
native linker ships in release builds, gated behind flags only until the
differential suite is green — then it becomes the default for its targets.

---

## 8. Complementary packaging track (independent, do not wait)

The native linker removes the linker dependency at the source; these changes
remove most of the **install-size cost** of the current dependency within
days, and none of them are wasted by the native linker later:

| Change | Effect |
|--------|--------|
| Drop `clang.exe` (100 MB) from the Windows bundle; keep `lld-link.exe` + add `llvm-ar.exe` | Windows MSVC path already drives `lld-link` directly; bundle halves |
| Generate Windows import libs once from `.def` files and ship the handful used (`kernel32`, `ws2_32`, `msvcrt`, `ucrt`, …) | Deletes the "install VS Build Tools / Windows SDK" failure mode and the `vswhere`/drive-scan code |
| Manifest `defaultComponents: []` + lazy fetch of a slim `link` component on first AOT build | Base installer becomes toolchain-free (~29 MB); full 374 MB–1 GB LLVM download becomes opt-in |
| Use embedded wasmtime library in `run_with_wasm` instead of shelling out to a `wasmtime` CLI | Removes a false external dependency (`src/cli/backends.rs`) |

Sequencing note: this track touches files under active development
(`linking.rs`, `installer.iss`, the manifest); the native-linker track is
almost entirely new files, so the two can proceed without collisions if the
packaging track goes second.

---

## 9. Risks

| Risk | Mitigation |
|------|------------|
| Hidden libc dependencies in the freestanding runtime (`printf` habits die hard) | D1 enforced by corpus: freestanding tier binaries must show zero dynamic dependencies (`ldd` empty) and import only the fixed Win32 set |
| aarch64 ADRP pairs broken by layout shifts | Dedicated fixture with functions straddling 4 KB boundaries; linux-aarch64 CI runner from M3 onward |
| PE edge cases (alignment of sections, `/reloc` correctness) | Clean-VM acceptance (§7.4) plus differential comparison against `lld-link` output |
| Scope creep into shared libraries / dynamic linking | Non-goals are contractual; requests route to `--linker=lld` until re-scoped |
| Native linker bit-rot while GPU/WASM work dominates | M0 harness lives in CI from day one; default switches only when green, and stays green as a merge gate |

---

## 10. References

- Current AOT pipeline and linker drivers: [AOT_COMPILATION.md](AOT_COMPILATION.md),
  `src/backends/aot/cranelift_impl/linking.rs`, `src/backends/aot/linker/`
- Existing skeleton to evolve: `src/backends/aot/static_linker.rs`,
  `src/backends/aot/object_gen.rs`, `src/backends/aot/symbols.rs`
- Toolchain manifest (what installs download today):
  `installer/manifests/toolchain-manifest.json`, `src/toolchain/manifest.rs`
- Precedents worth studying: Go `cmd/link` (own linker, syscall runtime,
  kept an external-linking mode anyway); Zig (ships lld for Mach-O/ELF
  cross-links rather than writing one); mold/wild (Rust-native ELF linkers —
  useful reading for layout and performance, not a dependency)
- ELF x86-64 psABI relocation reference; PE/COFF spec import-table chapter;
  `object` crate documentation (already in the dependency tree)
