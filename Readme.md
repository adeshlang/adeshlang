![Version](https://img.shields.io/badge/Version-v0.3.0-orange.svg)

<div align="center">

# AdeshLang v0.3.0

**A Rust-Inspired, Multi-Backend, High-Performance Systems & Application Programming Language**

[![Rust](https://img.shields.io/badge/Rust-2024%20Edition%20%7C%201.70%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-AdeshLang%20v2.0-blue.svg)](LICENSE)
[![Build](https://img.shields.io/badge/Build-Passing-brightgreen.svg)](https://github.com/adeshlang/adeshlang)
[![Tests](https://img.shields.io/badge/Tests-Passing%20(100%25)-brightgreen.svg)]()
[![Code Quality](https://img.shields.io/badge/Code%20Quality-Zero%20Errors%20%26%20Warnings-brightgreen.svg)]()
[![GPU](https://img.shields.io/badge/GPU-MLIR%20Backend-purple.svg)](docs/gpu/gpu-guide.md)
[![Memory Safety](https://img.shields.io/badge/Memory-Zero%20GC%20%2B%20ARC-red.svg)](docs/MEMORY_SAFETY.md)

*Compile-Time Memory Safety • Zero Garbage Collector • 11 Execution Backends • Native JIT & AOT • GPU/MLIR Acceleration • First-Class ARC • Cross-Platform TUI Editor • SIMD & Parallel Execution • Modern Standard Library*

[Documentation](docs/README.md) • [Quick Start](#installation) • [Contributing](CONTRIBUTING.md) • [Security](SECURITY.md) • [License](#license)

</div>

---

## Overview

AdeshLang is a modern, statically-typed, multi-backend programming language that delivers
Rust-grade compile-time memory safety without a garbage collector. Write once and run on
11 execution backends — from a fast interpreter to Native JIT, AOT binaries, WebAssembly,
and MLIR-based GPU acceleration — with guaranteed semantic parity.

**Highlights:** Zero-GC memory safety with first-class ARC • 11 execution backends • Native JIT (100–232x faster) • GPU/MLIR acceleration • Cross-platform TUI editor • Built-in AI/LLM for code • Rich standard library (TLS, HTTP/2, WebSockets, crypto, compression, async IO) • ADL package manager • LSP-based IDE support (VS Code, Neovim, Helix, Emacs, Sublime).

## Documentation

The complete language documentation lives in **[`docs/`](docs/README.md)** — the
documentation hub of this repository:

- 📚 [**Language Reference**](docs/language-reference.md)
- 🧠 [**Type System Guide**](docs/type-system-guide.md)
- 🔒 [**Memory Safety Guide**](docs/memory-safety-guide.md)
- ⚡ [**Execution Backends**](docs/backends-guide.md)
- 🛠️ [**CLI Reference**](docs/cli-guide.md)
- 📦 [**ADL Package Manager & Ecosystem**](docs/ECOSYSTEM_AND_ADL.md)
- 🎨 [**Adesh Editor**](docs/EDITOR.md)
- 🤖 [**AdeshLang AI Model**](docs/AI_MODEL_GUIDE.md)
- 🐳 [**Docker & Containerization Guide**](docs/DOCKER_GUIDE.md)

## Installation

The installer bundles the `adesh` compiler CLI, the `adl` package manager, the `als`
language server, the TUI editor, and the standard library — no Rust toolchain required.
Installers are available for **Windows** (EXE/MSI/ZIP), **Linux** (tarball, .deb, .rpm),
**macOS** (.pkg/.dmg), and **Docker** containers.

```text
adesh doctor                                  # verify the installation
adesh toolchain install --system              # LLVM/Clang/LLD 18.1.8 + MLIR
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

## License

AdeshLang is released under the **AdeshLang License v2.0** (MIT-compatible).
See [LICENSE](LICENSE) for full terms and [THIRD_PARTY_LICENSES](THIRD_PARTY_LICENSES) for
third-party notices.
