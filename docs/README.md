[← Back to repository README](../README.md)

<div align="center">

# AdeshLang v0.3.0

**A Rust-Inspired, Multi-Backend, High-Performance Systems & Application Programming Language**

[![Rust](https://img.shields.io/badge/Rust-2024%20Edition%20%7C%201.70%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-AdeshLang%20v2.0-blue.svg)](../LICENSE)
[![Memory Safety](https://img.shields.io/badge/Memory-Zero%20GC%20%2B%20ARC-red.svg)](MEMORY_SAFETY.md)
[![Status](https://img.shields.io/badge/Native%20Toolchain-Experimental%20%2B%20Execution--tested%20on%20Windows%20x64-yellow.svg)](../CURRENT_STATE.md)

*Compile-Time Memory-Safety Architecture • Zero Garbage Collector • 11 Execution Backends • Native JIT & AOT • Self-Contained Native Toolchain (ADOB + adeshlink) • First-Class ARC • Cross-Platform TUI Editor • SIMD & Parallel Execution • Modern Standard Library*

[Quick Start](#quick-start) • [Adesh Editor](#adesh-editor) • [Developer Tools & IDE Support (ALS)](#developer-tools--ide-support-als) • [Documentation](#documentation) • [License](#license)

</div>

---

## Overview

**AdeshLang** is a modern, statically-typed, multi-backend programming language designed for both high-level developer ergonomics and low-level bare-metal performance. It has a zero-GC compile-time memory-safety architecture (ownership, borrowing, and lifetime checking, plus first-class ARC), paired with versatile execution models ranging from instant interpretation to Native JIT compilation, standalone native binaries (AOT), WebAssembly, and an experimental external-MLIR GPU path.

### Key Pillars

- 🔒 **Deterministic Memory Safety (Zero-GC)**: Enforced compile-time ownership, borrowing, and lifetime checking, with no garbage collection pauses. (Note: this is the language *architecture*; backend coverage and the native path are still maturing — see [MEMORY_SAFETY.md](MEMORY_SAFETY.md) scope notes and [../CURRENT_STATE.md](../CURRENT_STATE.md).)
- ⚡ **First-Class Automatic Reference Counting (ARC)**: Built-in `share`, `strong`, and `weak` keywords with `.strong_count()`, `.weak_count()`, `.is_alive()`, and safe `.upgrade()` mechanics to break reference cycles effortlessly.
- 🚀 **Multi-Backend Architecture**: Write once and target 11 execution backends — Interpreter, Bytecode VM, Mixed (hybrid), JIT, Native JIT (100–232x faster on compute benchmarks), Adaptive JIT, Tiered JIT, Safe mode, AOT Compiler (standalone binaries), WebAssembly, and GPU/MLIR (experimental, external tools). Feature coverage varies by backend; semantic parity is a goal, not a guarantee (see [backends-guide.md](backends-guide.md)).
- 🎨 **Cross-Platform TUI Editor (`adesh editor`)**: Keyboard-driven terminal IDE inspired by Neovim, featuring syntax highlighting, multi-buffer tabs, integrated output panel, file explorer, command palette, and backend runner.
- 🤖 **Specialized AdeshLang AI/LLM (`adesh ai`)**: Lightweight, compiler-verified local language model specialized in understanding, generating, explaining, debugging, and completing AdeshLang code (`ai/`).
- 🧠 **Expressive Modern Type System**: Flow-sensitive type inference, generics, sum types (`Option<T>`, `Result<T, E>`), structural tuples, type aliases, union types, and numeric types (`i8`–`i64`, `u8`–`u64`, `f32`, `f64`).
- 🏛️ **Modern Object-Oriented Programming**: Full class hierarchy, granular access modifiers (`private`, `protected`, `public`), first-class properties with `get`/`set` accessors, abstract classes, interfaces, and `@sealed` inheritance controls.
- 🌐 **Rich Standard Library & Networking**: Production-grade built-ins including TLS 1.3/1.2 (mTLS & ALPN), HTTP/1.1 & HTTP/2, WebSockets, TCP/UDP networking, DNS resolution, comprehensive Cryptography (SHA-1/2/3, BLAKE2/3, Argon2, AES-GCM, ChaCha20-Poly1305, Ed25519, RSA, ECDSA, JWT, UUID), Compression (Gzip, Brotli, Zstandard, LZ4, Zip, Tar), Random generation, URL parsing, Math library, and Async IO.
- ⚡ **SIMD & Parallel Execution**: Built-in SIMD vector operations, parallel for-loops, parallel reduce, and zero-cost inline assembly for hardware-level control.
- 📦 **ADL Package Management Ecosystem**: Complete dependency resolution, manifest configuration (`adesh.adl`), version locking (`adesh.lock`) with SHA-256 integrity checks, and local `adl_modules/` loading.
- 🖥️ **Embedded Mode & Cross-Compilation**: Minimal runtime mode for resource-constrained targets, cross-compilation target listing and inspection, and standalone binary generation for any platform.
- 🧪 **Cargo-Grade Testing Framework (`adesh test`)**: Comprehensive unit and integration test runner with multi-module test discovery, substring/`--exact`/`--skip` filtering, multi-threaded Rayon parallelism (`--test-threads <N>`), sequential execution (`--serial`), and simultaneous multi-backend verification (`--runtimes=interp,jit,njit` / `--backend-check`).
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
- Non-blocking runner for all 11 execution backends
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
