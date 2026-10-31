![Version](https://img.shields.io/badge/Version-v0.3.0-orange.svg)

<div align="center">

# AdeshLang v0.3.0

**A Rust-Inspired, Multi-Backend Systems & Application Programming Language**

[![Rust](https://img.shields.io/badge/Rust-2024%20Edition%20%7C%201.70%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-AdeshLang%20v0.3.0-blue.svg)](LICENSE)
[![Memory Safety](https://img.shields.io/badge/Memory-Zero%20GC%20%2B%20ARC-red.svg)](docs/MEMORY_SAFETY.md)
[![Status](https://img.shields.io/badge/Native%20Toolchain-Experimental%20%2B%20Execution--tested-yellow.svg)](CURRENT_STATE.md)

*Compile-Time Memory-Safety Architecture • Zero Garbage Collector • Four Execution Runtimes • Self-Contained Native Toolchain (ADOB + `adeshlink`) • First-Class ARC • Cross-Platform TUI Editor • Parallel Execution • Modern Standard Library*

[Documentation](docs/README.md) • [Runtimes](#the-four-execution-runtimes) • [Quick Start](#installation) • [Toolchain Status](CURRENT_STATE.md) • [Contributing](CONTRIBUTING.md) • [Security](SECURITY.md) • [License](#license)

</div>

---

## Overview

AdeshLang is a modern, statically-typed programming language with a zero-GC
memory-safety architecture — compile-time ownership, borrowing and lifetime
checking, plus first-class ARC — and **four execution runtimes** that share one
front end: Interpreter, JIT, AOT and WASM.

It ships with a **self-contained native toolchain** for the AOT runtime: its own
Machine IR code generator, ADOB object format, `adeshlink` linker and native
runtime. No LLVM, Clang, GCC, MSVC or GNU Binutils is required to produce a
native executable.

### The four execution runtimes

Every runtime consumes the same AST → HIR pipeline, so a program that type-checks
has one meaning regardless of how it is run. Select a runtime with
`--backend=<name>` (or `--backend <name>`); the flag rejects any other value with
`Invalid backend`.

| Runtime | Flag | What it does | Executed in CI |
|---|---|---|---|
| **Interpreter** | `--backend=interpreter` (default) | Tree-walking evaluation of HIR. Broadest language-feature coverage; also the only runtime that resolves `extern`/`$cImport` declarations through libffi. | Yes — all three OS jobs |
| **JIT** | `--backend=njit` (aliases `native-jit`, `jit-native`) | Generates native code at run time with Cranelift. The fastest runtime path on compute benchmarks. | Yes — x86-64 (Cranelift PLT support is x86-64-only, so this runtime compiles out on Apple Silicon) |
| **AOT** | `--backend=aot` / `--backend=native`, `adesh build` | Ahead-of-time compilation through the **bundled native codegen** (Machine IR → x86-64) and the **`adeshlink` linker** → ADOB → executable. No external toolchain involved. | Windows PE and Linux ELF: yes. Mach-O: emitted, **not** executed. |
| **WASM** | `--backend=wasm`, `--target wasm32-unknown-wasi` | Emits WebAssembly with a real **WASI Preview 1** compiler ABI; `adeshlink` links it — including multi-object links and indirect calls through a linker-defined function table. | Yes — on two engines: V8 (Node) and Cranelift (`wasmtime`) |

Feature coverage is not identical across the four; the per-feature matrix lives in
[`docs/backends-guide.md`](docs/backends-guide.md) and the per-target matrix in
[`RUNTIME_MATRIX.md`](RUNTIME_MATRIX.md).

**The honest toolchain status:** the native AOT pipeline (source → HIR → Machine
IR → x86-64 → ADOB → `adeshlink` → executable) is real and execution-verified on
Windows x86-64 and Linux ELF, and the WASM runtime's produced modules are executed
on two engines in CI. Mach-O artifacts are emitted with a full dyld
bind/rebase structure but have **no execution verification yet**. AArch64 and
RISC-V are emission-tested only — no CI runner executes their binaries. The
GPU/NPU/TPU and quantum paths are container packaging: they wrap metadata or
caller-supplied payloads, generate no device machine code from Adesh source, and
call no device driver. See [`CURRENT_STATE.md`](CURRENT_STATE.md) — the canonical
status document — and [`TARGET_MATRIX.md`](TARGET_MATRIX.md).

## Features

### Language
- **Static typing with inference**, plus a growing type-system layer: type aliases
  including **scalar aliases and member types** (the `type` keyword), now working
  across all four runtimes; structs, enums, classes and interfaces.
- **Generics** over type parameters. Bounded parameters (`<T: Ord>`) are **not**
  accepted by the parser — parameter names must be bare identifiers.
- **Zero-GC memory safety**: ownership, borrowing and lifetime checks at compile
  time, with **first-class ARC** for shared ownership. No collector, no pause.
- **`Result` / `Option`**, structured error handling, `try`/`catch`, and a
  diagnostics-code system explainer via `adesh --explain`.
- **First-class functions and closures**, `defer`, pattern matching, and
  expression-oriented control flow.
- **Async and concurrency**: `async`/`await`, tasks, threads and channels, plus
  `parallel_for` / `parallel_reduce` backed by a work-stealing scheduler
  ([`src/runtime/scheduler/`](src/runtime/scheduler/)).
- **Modules** and a package ecosystem.

### Toolchain
- **Self-contained native toolchain**: Machine IR codegen, the **ADOB** object
  format, the **`adeshlink`** linker and a native runtime — producing executables,
  static archives and shared objects with zero external dependencies.
- **Whole-program optimisation**: `--lto` runs the IR-level `LtoEngine` before the
  linker's section pass; `--pgo` plus `adesh pgo merge` drive profile-guided
  optimisation (covered by the `native_phase*_pgo_e2e_test` suites).
- **Native inspection tools**: `adesh link`, `inspect`, `objdump`, `size`,
  `symbols`, `relocations`, `sections`, `map` over ADOB/object inputs.
- **Cross-target emission** for AArch64 and RISC-V (RV64GC), and container
  packaging for accelerator/quantum payloads.

### Standard library
Fourteen modules in [`src/stdlib/`](src/stdlib/): **HTTP**, **TLS**, **WebSocket**,
**Crypto**, **Compression**, **Encoding**, **URL**, **Path**, **Process**, **IO**,
**Env**, **Random**, **System**, **Time**. HTTP is a synchronous and async
request/response client over an injected transport (`get`/`post`/`put`/`patch`/
`head`/`options` + `*Async` variants); TLS provides client `connect`/`wrap` and
server `bindServer`/`Server` with certificate and policy handling. The
`HttpVersion` class declares `HTTP/2.0` and `HTTP/3.0` constants but implements no
frame layer — there is no HPACK and no QUIC, so **HTTP/1.1 is the only protocol
actually negotiated today**.

### Ecosystem & tooling
- **`adl`** — the ADL package manager (`adl add`, `adl run`, `adl build`, `adl test`).
- **`adesh editor`** — cross-platform TUI editor with syntax highlighting, fuzzy
  command palette, file explorer, split buffers and LSP-backed completion.
- **`als`** — language server with IDE integrations for VS Code, Neovim, Helix,
  Emacs and Sublime ([`als-vscode/`](als-vscode/), [`als-neovim/`](als-neovim/),
  [`als-helix/`](als-helix/), [`als-emacs/`](als-emacs/),
  [`als-sublime/`](als-sublime/)).
- **Built-in AI** — `adesh ai`, an offline bundled model for code assistance.
- **Web editor** — the engine compiles to `wasm32-unknown-unknown`
  (`-p adesh-wasm`) and is deployed by the `Web Editor WASM Build` workflow to the
  standalone [`adeshlang/web-editor`](https://github.com/adeshlang/web-editor) repo.
- **Docker** images and Linux/Windows release packaging.

## Documentation

The complete language documentation lives in **[`docs/`](docs/README.md)** — the
documentation hub of this repository:

- 📚 [**Language Reference**](docs/language-reference.md)
- 🧠 [**Type System Guide**](docs/type-system-guide.md)
- 🔒 [**Memory Safety Guide**](docs/MEMORY_SAFETY.md)
- ⚡ [**Execution Runtimes**](docs/backends-guide.md)
- 🧪 [**Test Framework Guide**](docs/testing-guide.md)
- 🛠️ [**CLI Reference**](docs/cli-guide.md)
- 🔌 [**FFI Guide**](docs/ffi-guide.md)
- 📦 [**ADL Package Manager & Ecosystem**](docs/ECOSYSTEM_AND_ADL.md)
- 🎨 [**Adesh Editor**](docs/EDITOR.md)
- 🤖 [**AdeshLang AI Model**](docs/AI_MODEL_GUIDE.md)
- 🐳 [**Docker & Containerization Guide**](docs/DOCKER_GUIDE.md)
- 🛠️ [**Native Toolchain Status**](CURRENT_STATE.md) — canonical, honest status
- 🎯 [**Target Matrix**](TARGET_MATRIX.md) — per-target verification level

## Installation

The installer bundles the `adesh` compiler CLI, the `adl` package manager, the `als`
language server, the TUI editor, the standard library, and the native linker/runtime —
no Rust, LLVM, or MSVC toolchain required.
Installers are currently produced for **Windows** (setup EXE + portable ZIP) and
**Linux** (tarball), plus **Docker** containers. (macOS installers, `.deb`/`.rpm`,
`.pkg`/`.dmg`, and MSI packaging are not currently produced.)

```text
adesh doctor                                  # verify the installation
adesh gpu-check                               # native toolchain report (zero external deps)
adesh toolchain --external install llvm       # OPTIONAL: external LLVM for --external-linker
adesh ai status                               # bundled offline AI model
```

Select a runtime explicitly with `--backend`:

```bash
adesh run app.adesh                            # Interpreter (default)
adesh run --backend=njit app.adesh             # JIT (Cranelift, native code at runtime)
adesh build app.adesh -o app                   # AOT (bundled codegen + adeshlink)
adesh build --target=wasm32-unknown-wasi app.adesh   # WASM (WASI Preview 1)
```

### Docker Quickstart

```bash
docker build -t adeshlang:latest .
docker run --rm -it adeshlang:latest          # launch interactive REPL
docker run --rm -v $(pwd):/workspace adeshlang:latest myscript.adesh
```

Building from source:

```bash
git clone https://github.com/adeshlang/adeshlang.git
cd adeshlang
cargo build --release
```

On Windows, use `cargo build --release -j 2` — the link step can exceed the
default parallelism on this host.

## Contributing

We welcome contributions of all kinds — bug reports, features, documentation, and examples.
Please read the **[Contributing Guide](CONTRIBUTING.md)** before submitting a pull request,
and report security issues privately via **[SECURITY.md](SECURITY.md)**.

> **Documentation policy:** every capability claim must reference an execution
> test. A capability is "implemented" only when a produced artifact is
> executed and asserted in CI; emitting bytes is "emission", not "support".
> Status documents must stay consistent with `CURRENT_STATE.md`.

## License

AdeshLang is released under the **AdeshLang License v2.0** (MIT-compatible).
See [LICENSE](LICENSE) for full terms and [THIRD_PARTY_LICENSES](THIRD_PARTY_LICENSES) for
third-party notices.
