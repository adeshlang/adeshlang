![Version](https://img.shields.io/badge/Version-v0.3.0-orange.svg)

<div align="center">

# AdeshLang v0.3.0

**A Rust-Inspired, Multi-Backend Systems & Application Programming Language**

[![Rust](https://img.shields.io/badge/Rust-2024%20Edition%20%7C%201.70%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-AdeshLang%20v0.3.0-blue.svg)](LICENSE)
[![Memory Safety](https://img.shields.io/badge/Memory-Zero%20GC%20%2B%20ARC-red.svg)](docs/MEMORY_SAFETY.md)
[![Status](https://img.shields.io/badge/Native%20Toolchain-Experimental%20%2B%20Execution--tested%20on%20Windows%20x64-yellow.svg)](CURRENT_STATE.md)

*Compile-Time Memory-Safety Architecture • Zero Garbage Collector • 11 Execution Backends • Native JIT & AOT • Self-Contained Native Toolchain (ADOB + adeshlink) • First-Class ARC • Cross-Platform TUI Editor • SIMD & Parallel Execution • Modern Standard Library*

[Documentation](docs/README.md) • [Quick Start](#installation) • [Toolchain Status](CURRENT_STATE.md) • [Contributing](CONTRIBUTING.md) • [Security](SECURITY.md) • [License](#license)

</div>

---

## Overview

AdeshLang is a modern, statically-typed, multi-backend programming language
with a zero-GC memory-safety architecture (compile-time ownership, borrowing,
and lifetime checking, plus first-class ARC). It ships with a **self-contained
native toolchain** — its own Machine IR code generator, ADOB object format,
`adeshlink` linker, and native runtime — alongside 11 execution backends
(interpreter, bytecode VM, JIT variants, AOT, WebAssembly, and an
experimental external-MLIR GPU path). Backend feature coverage varies; see
[`docs/backends-guide.md`](docs/backends-guide.md).

**The honest toolchain status:** the native pipeline (source → HIR → Machine
IR → x86-64 → ADOB → `adeshlink` → executable) is **real and
execution-verified on Windows x86-64**, which is currently the only target
whose produced binaries are executed by CI tests. ELF output is emitted but
not run-tested; Mach-O artifacts cannot yet bind system libraries. See
[`CURRENT_STATE.md`](CURRENT_STATE.md) — the canonical status document — and
[`TARGET_MATRIX.md`](TARGET_MATRIX.md) for details.

**Highlights:** Zero-GC memory-safety architecture with first-class ARC • 11 execution backends (coverage varies) • Self-contained native toolchain: ADOB object format + `adeshlink` linker + native runtime • Native JIT (100–232x faster than the interpreter on compute benchmarks) • Cross-platform TUI editor • Built-in AI/LLM for code • Rich standard library (TLS, HTTP/2, WebSockets, crypto, compression) • ADL package manager • LSP-based IDE support (VS Code, Neovim, Helix, Emacs, Sublime).

## Documentation

The complete language documentation lives in **[`docs/`](docs/README.md)** — the
documentation hub of this repository:

- 📚 [**Language Reference**](docs/language-reference.md)
- 🧠 [**Type System Guide**](docs/type-system-guide.md)
- 🔒 [**Memory Safety Guide**](docs/memory-safety.md)
- ⚡ [**Execution Backends**](docs/backends-guide.md)
- 🧪 [**Test Framework Guide**](docs/testing-guide.md)
- 🛠️ [**CLI Reference**](docs/cli-guide.md)
- 📦 [**ADL Package Manager & Ecosystem**](docs/ECOSYSTEM_AND_ADL.md)
- 🎨 [**Adesh Editor**](docs/EDITOR.md)
- 🤖 [**AdeshLang AI Model**](docs/AI_MODEL_GUIDE.md)
- 🐳 [**Docker & Containerization Guide**](docs/DOCKER_GUIDE.md)
- 🛠️ [**Native Toolchain Status**](../CURRENT_STATE.md) — canonical, honest status

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
