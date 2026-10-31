[← Back to repository README](../Readme.md)

<div align="center">

# AdeshLang v0.3.0

**A Rust-Inspired, Multi-Runtime, High-Performance Systems & Application Programming Language**

[![Rust](https://img.shields.io/badge/Rust-2024%20Edition%20%7C%201.70%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-AdeshLang%20v2.0-blue.svg)](../LICENSE)
[![Memory Safety](https://img.shields.io/badge/Memory-Zero%20GC%20%2B%20ARC-red.svg)](MEMORY_SAFETY.md)
[![Status](https://img.shields.io/badge/Native%20Toolchain-Experimental%20%2B%20Execution--tested%20(Windows%20PE%20%2B%20Linux%20ELF%20%2B%20WASM)-yellow.svg)](../CURRENT_STATE.md)

*Compile-Time Memory-Safety Architecture • Zero Garbage Collector • Four Execution Runtimes • Self-Contained Native Toolchain (ADOB + adeshlink) • First-Class ARC • Cross-Platform TUI Editor • SIMD & Parallel Execution • Modern Standard Library*

[Quick Start](#quick-start) • [Adesh Editor](#adesh-editor) • [Developer Tools & IDE Support (ALS)](#developer-tools--ide-support-als) • [Documentation](#documentation) • [License](#license)

</div>

---

## Overview

**AdeshLang** is a modern, statically-typed programming language designed for both high-level developer ergonomics and low-level systems performance. It has a zero-GC compile-time memory-safety architecture (ownership, borrowing, and lifetime checking, plus first-class ARC), paired with **four execution runtimes** that share one front end: Interpreter, JIT (native code at run time), AOT (standalone native binaries from the bundled codegen + `adeshlink`), and WebAssembly.

### Key Pillars

- 🔒 **Deterministic Memory Safety (Zero-GC)**: Enforced compile-time ownership, borrowing, and lifetime checking, with no garbage collection pauses. (Note: this is the language *architecture*; backend coverage and the native path are still maturing — see [MEMORY_SAFETY.md](MEMORY_SAFETY.md) scope notes and [../CURRENT_STATE.md](../CURRENT_STATE.md).)
- ⚡ **First-Class Automatic Reference Counting (ARC)**: Built-in `share`, `strong`, and `weak` keywords (`src/parsing/lexer.rs:1076-1078`) with `.strong_count()`, `.weak_count()`, `.is_alive()`, and safe `.upgrade()` mechanics to break reference cycles. The four introspection methods are dispatched as interpreter built-in methods (`src/execution/runtime_core/exec/expression_eval/builtin_methods.rs:25`) — treat them as an Interpreter/JIT facility, not a guarantee under AOT or WASM.
- 🚀 **Four Execution Runtimes**: Write once and run through `--backend=…` — **Interpreter** (`interpreter`, the default; broadest feature coverage and the only runtime that resolves `extern`/`$cImport` through libffi), **JIT** (`njit`, also accepted as `native-jit` / `jit-native`; Cranelift emits native code at run time and is the fastest runtime path on compute benchmarks), **AOT** (`aot` / `native`; the bundled Machine-IR codegen and `adeshlink` linker produce standalone executables with no external toolchain), and **WASM** (`wasm`, with a real WASI Preview 1 ABI and linker-defined function table for `call_indirect`). Any other `--backend` value fails with `Invalid backend`. Feature coverage varies by runtime; semantic parity is a goal, not a guarantee (see [backends-guide.md](backends-guide.md)). Execution-verified today: Windows PE, Linux ELF and WASM on two engines; Mach-O, AArch64 and RISC-V are emission-tested only (see [../TARGET_MATRIX.md](../TARGET_MATRIX.md)).
- 🎨 **Cross-Platform TUI Editor (`adesh editor`)**: Keyboard-driven terminal IDE inspired by Neovim, featuring syntax highlighting, multi-buffer tabs, integrated output panel, file explorer, command palette, and backend runner.
- 🤖 **Specialized AdeshLang AI/LLM (`adesh ai`)**: Lightweight, compiler-verified local language model specialized in understanding, generating, explaining, debugging, and completing AdeshLang code (`ai/`).
- 🧠 **Expressive Modern Type System**: Flow-sensitive type inference, generics, sum types (`Option<T>`, `Result<T, E>`), structural tuples, type aliases — including **scalar aliases and member types** via the `type` keyword, now working across all four runtimes — union types, and numeric types (`i8`–`i64`, `u8`–`u64`, `f32`, `f64`). Generic parameters must be **bare identifiers**: the parser rejects bounds such as `<T: Ord>` (`src/parsing/parser/functions.rs:24-37`).
- 🏛️ **Modern Object-Oriented Programming**: Full class hierarchy, granular access modifiers (`private`, `protected`, `public`), first-class properties with `get`/`set` accessors, abstract classes, interfaces, and `@sealed` inheritance controls.
- 🌐 **Rich Standard Library & Networking**: Built-ins in [`src/stdlib/`](../src/stdlib/) covering TLS (client `connect`/`wrap`, server `bindServer`/`Server`, ALPN query), HTTP request/response over an injected transport — **HTTP/1.1 is the protocol actually implemented; the `HttpVersion` class names `HTTP/2.0`/`HTTP/3.0` but ships no frame layer, no HPACK and no QUIC** — WebSockets, Cryptography (SHA-1/2/3, BLAKE3, Argon2id, AES-GCM, ChaCha20-Poly1305, Ed25519, RSA, ECDSA, JWT, UUID), Compression (DEFLATE, Gzip, Zlib, Brotli, Zstandard, LZ4, XZ/LZMA — see [`src/runtime/stdlib_src/compression/codecs.rs`](../src/runtime/stdlib_src/compression/codecs.rs)), Random generation, URL parsing, Time, Path, Env, Process and System.
- ⚡ **SIMD & Parallel Execution**: An auto-vectorisation pass and SIMD dispatcher in the native codegen ([`crates/adesh-codegen/src/opt/vectorization.rs`](../crates/adesh-codegen/src/opt/vectorization.rs), `vector_dispatch`), covered by the `native_phase7_simd_e2e_test`, `native_phase8_simd_e2e_test` and `native_phase9_vectorization_e2e_test` suites, plus `parallel_for` / `parallel_reduce` on a work-stealing scheduler ([`src/runtime/scheduler/`](../src/runtime/scheduler/)). There is **no inline-assembly facility** — the lexer has no `asm` keyword.
- 📦 **ADL Package Management Ecosystem**: Complete dependency resolution, manifest configuration (`adesh.adl`), version locking (`adesh.lock.adl`) with SHA-256 integrity checks, and local `adl_modules/` loading.
- 🖥️ **Embedded Mode & Cross-Compilation**: Minimal runtime mode for resource-constrained targets and cross-compilation target listing/inspection (`adesh target list`, `adesh target info <triple>`). Standalone binaries are **execution-verified only for Windows x86-64 and Linux x86-64**; AArch64 and RISC-V (RV64GC) produce artifacts that CI validates structurally but never runs, and there is no "any platform" guarantee. See [../TARGET_MATRIX.md](../TARGET_MATRIX.md).
- 🧪 **Cargo-Grade Testing Framework (`adesh test`)**: Comprehensive unit and integration test runner with multi-module test discovery, substring/`--exact`/`--skip` filtering, multi-threaded Rayon parallelism (`--test-threads <N>` or `-j <N>`), sequential execution (`--serial`), and simultaneous multi-runtime verification (`--runtimes=interpreter,njit,aot,wasm`, accepted with or without `=`; unknown names warn and are ignored) or the full matrix via `--backend-check`, which covers the six backends returned by `all_execution_backends()`.
- 🛠️ **Full IDE & Tooling Support**: First-class Language Server Protocol (`als`) implementation with diagnostics, inlay hints, type narrowing, unused variable detection, hover, definition jump, code formatting (`adesh fmt`), and integrations for VS Code, Neovim, Helix, Emacs, and Sublime Text.

---

## Quick Start

### Prerequisites

The installer bundles everything the language needs to run: the `adesh` compiler CLI,
the `adl` package manager, the `als` language server, the `adesh-editor` TUI editor,
and the standard library. No Rust toolchain is required.

The **native toolchain** (native codegen, ADOB object format, `adeshlink`
linker, and the Adesh runtime) is **bundled inside every installer** — nothing
is downloaded during installation and no LLVM, Clang, GCC, or MSVC is
required to compile, link, and run Adesh programs. An external LLVM toolchain
is **optional**: register one with `adesh toolchain --external install llvm`
to enable the opt-in `--external-linker` mode (`adesh build --codegen=cranelift
--external-linker`) and MLIR GPU source builds; verify it with
`adesh gpu-check --external-linker`.

After installing, run:

```text
adesh doctor
```

To register the optional external LLVM bridge later:

```text
adesh toolchain --external install llvm          # optional external LLVM bridge
adesh gpu-check --external-linker                # verify the external LLVM
adesh toolchain install --system --build-mlir-source   # optional MLIR GPU tools (30–90 min)
```

Installers are provided for Windows (setup EXE + MSI + portable ZIP), Linux
(tarball + install script, .deb, .rpm), and macOS (.pkg + .dmg).

#### Bundled AdeshLang AI Model

Every installer and the portable archives also bundle the default AdeshLang
AI model (`adesh-coder-0.5b`, Q4_0 quantization, ~275 MB) under
`<ADESH_HOME>\ai\models\` next to the Ollama/OpenRouter deployment configs,
so `adesh ai status`, `adesh ai generate`, `adesh ai explain`, and
`adesh ai fix` work offline immediately after installation. Larger
quantizations (Q8_0 and F16) remain opt-in downloads via `adesh ai setup`,
pinned and SHA-256-verified by `ai/models/manifest.json`.

#### Building From Source

| Tool | Version | Purpose | Installation |
|------|---------|---------|--------------|
| **Rust** | 1.70+ (Edition 2024) | Compiler & runtime build system | [rustup.rs](https://rustup.rs/) |
| **Cargo** | (included) | Package and build manager | Included with Rust |
| **C Compiler / Linker** | Any | Native code linking for AOT / FFI | GCC / Clang / MSVC / MinGW |

```bash
cargo build --release
```

---

## Adesh Editor

Launch the cross-platform TUI code editor via:

```bash
adesh editor
adesh editor main.ad
adesh editor ./my_project
```

Features:
- Syntax highlighting for AdeshLang grammar
- Multi-buffer tabs & file explorer
- Non-blocking runner with its own backend selector. The editor's `Backend::all()` list ([`editor/src/runner.rs:12-24,59`](../editor/src/runner.rs)) is broader than the four documented runtimes — it also offers `--jit`, `--bytecode`, `--adaptive`, `--tiered`, `--mixed`, `--safe` and `--gpu`, each of which maps to a real CLI flag, but only Interpreter / JIT (`--native-jit`) / AOT / WASM are covered by this documentation and by CI.
- Command palette (`Ctrl+P`) & Search/Replace (`Ctrl+F`, `Ctrl+H`)
- LSP integration with `als`

See [EDITOR.md](EDITOR.md) for full editor documentation.

---

## Developer Tools & IDE Support (ALS)

AdeshLang includes the **Adesh Language Server (ALS)**, implementing the Language Server Protocol (LSP) for seamless editor integration:

### ALS Capabilities

- 🎯 **Real-Time Diagnostics**: Instant feedback on syntax errors, type mismatches, and borrow checker violations.
- 💡 **Inlay Hints**: Inline type annotations for inferred variables and function returns.
- 🔍 **Flow-Sensitive Type Narrowing**: Automatic type refinement across `if/else`, `instanceof`, and logical `&&`/`||` branches.
- ⚠️ **Liveness & Unused Variable Detection**: Real-time unused variable analysis with editor dimming.
- ⚡ **Auto-Completion & Signatures**: Contextual symbol, method, and standard library completions.
- 📖 **Hover Documentation & Jump to Definition**: Instant type resolution and documentation tooltips.

---

## Documentation

This folder is AdeshLang's documentation hub. Explore our comprehensive guides:

- **[Editor Guide](EDITOR.md)**: Full guide to launching, configuring, and using `adesh editor`
- **[Release Engineering Guide](RELEASE.md)**: Step-by-step release builds and deployment per OS, and launching GitHub releases
- **[Language Reference](language-reference.md)**: Full grammar, syntax, functions, error model, and examples
- **[Type System Guide](type-system-guide.md)**: Generics, type inference, sum types, numeric types, and type layout
- **[Memory Safety Guide](memory-safety-guide.md)**: Ownership, borrowing, lifetimes, ARC, and Zero-GC architecture
- **[Execution Backends](backends-guide.md)**: In-depth breakdown of Interpreter, VM, JIT, NJIT, AOT, WASM
- **[Native Linker Roadmap](NATIVE_LINKER_ROADMAP.md)**: Design and milestones for toolchain-free AOT builds (first-party ELF/PE linker, syscall runtime, differential testing)
- **[AdeshLang AI & Specialized LLM Guide](AI_MODEL_GUIDE.md)**: Architecture, dataset pipeline, compiler validation, training, and `adesh ai` CLI integration

---

## License

AdeshLang is released under the **AdeshLang License v2.0** (MIT-compatible). See [LICENSE](../LICENSE) for details.
