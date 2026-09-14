# Contributing to AdeshLang

Thank you for your interest in contributing to AdeshLang! This guide covers everything you
need to know to build, test, and submit changes to the project. Please read it before
opening a pull request.

## Table of Contents

1. [Code of Conduct](#code-of-conduct)
2. [Project Overview](#project-overview)
3. [Getting Started](#getting-started)
4. [Building from Source](#building-from-source)
5. [Running Tests](#running-tests)
6. [Code Style & Linting](#code-style--linting)
7. [Making Changes](#making-changes)
8. [Submitting a Pull Request](#submitting-a-pull-request)
9. [Documentation](#documentation)
10. [Reporting Issues](#reporting-issues)
11. [Reporting Security Issues](#reporting-security-issues)

---

## Code of Conduct

Be respectful, constructive, and inclusive. AdeshLang welcomes contributors of all
experience levels. Derogatory comments, personal attacks, and harassment are not tolerated.

## Project Overview

AdeshLang is a Rust-Inspired, Multi-Backend, High-Performance Systems & Application
Programming Language. The project is organized as a Cargo workspace:

| Path | Purpose |
|------|---------|
| [`src/`](src/) | Language core: lexer, parser, type checker, borrow checker, execution backends, standard library |
| [`crates/`](crates/) | Auxiliary crates (e.g. WASM support) |
| [`als/`](als/) | Adesh Language Server (LSP) implementation |
| [`editor/`](editor/) | Cross-platform TUI editor (`adesh editor`) |
| [`ai/`](ai/) | Specialized AdeshLang AI/LLM model and its pipeline |
| [`installer/`](installer/) | Installer sources (Windows, Linux, macOS) |
| [`docs/`](docs/) | All user and developer documentation (start at [`docs/README.md`](docs/README.md)) |
| [`examples/`](examples/) | Runnable `.adesh` examples organized by topic |
| [`tests/`](tests/) | Integration test suites organized by feature area |
| [`benches/`](benches/) | Criterion benchmarks |

## Getting Started

1. **Fork the repository** on GitHub.
2. **Clone your fork**:
   ```bash
   git clone https://github.com/<your-username>/adeshlang.git
   cd adeshlang
   ```
3. **Add the upstream remote**:
   ```bash
   git remote add upstream https://github.com/adeshlang/adeshlang.git
   ```
4. **Create a branch** for your work:
   ```bash
   git checkout -b my-feature
   ```

## Building from Source

### Prerequisites

| Tool | Minimum Version | Purpose |
|------|-----------------|---------|
| Rust | 1.70+ (Edition 2024) | Compiler & runtime build system |
| Cargo | (included) | Package and build manager |
| C Compiler / Linker | Any | Native code linking for AOT / FFI (GCC / Clang / MSVC / MinGW) |

### Build

```bash
# Debug build
cargo build

# Release build (optimized, LTO)
cargo build --release
```

### Native Toolchain

The AOT and GPU backends require the pinned native toolchain
(LLVM/Clang/LLD 18.1.8 + MLIR). It is normally installed by the installer, but can be
fetched any time with:

```bash
# Assuming the compiler is built; otherwise just install the toolchain:
./target/debug/adesh toolchain install --system
```

> **Note:** `adesh toolchain install --system --build-mlir-source` also builds the MLIR
> GPU tools and takes 30–90 minutes. Most development does not need it.

## Running Tests

Run the full test suite before submitting changes:

```bash
# Library unit tests
cargo test --lib

# Integration test suites
cargo test --tests

# Everything
cargo test --workspace
```

The CI pipeline (see [`.github/workflows/ci.yml`](.github/workflows/ci.yml)) runs on both
Ubuntu and Windows and must pass before a pull request can be merged:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --lib`
- `cargo test --tests` (Ubuntu only)
- `cargo check --bin adesh`

Run all of these locally before pushing to keep CI green.

### Adding Tests

- **Language feature changes** require integration tests in [`tests/`](tests/) (e.g.
  `tests/types/`, `tests/memory/`, `tests/regressions/`).
- **Compiler unit tests** go next to the code in `src/` as `#[cfg(test)]` modules.
- **Examples** in [`examples/`](examples/) are also exercised by
  `tests/examples_integration.rs`, so keep them runnable.
- **Benchmarks** for performance-sensitive changes belong in [`benches/`](benches/).

## Code Style & Linting

- **Formatting:** All Rust code must be formatted with `rustfmt`:
  ```bash
  cargo fmt --all
  ```
- **Linting:** Clippy must pass with warnings denied:
  ```bash
  cargo clippy --workspace --all-targets --all-features -- -D warnings
  ```
- **Language sources:** `.adesh` examples should follow the conventions in
  [`docs/`](docs/) and the existing [`examples/`](examples/) code.
- Keep changes focused; avoid unrelated formatting churn in the same PR.

## Making Changes

1. **Check for open work:** Review the open issues and the roadmap in
   [`docs/status-and-roadmap.md`](docs/status-and-roadmap.md). For large features, open an
   issue first to discuss the design.
2. **Keep diffs reviewable:** Prefer small, focused pull requests over one giant change.
3. **Update version-sensitive files when necessary:** If you change behavior that is
   documented (docs, README badges), update the affected docs in the same PR.
4. **Commit messages:** Write clear, imperative commit messages, e.g.
   `Fix borrow checker edge case in closure captures`.

## Submitting a Pull Request

1. Push your branch to your fork:
   ```bash
   git push origin my-feature
   ```
2. Open a pull request against the **`main`** branch of `adeshlang/adeshlang`.
3. Fill out the PR description: what changed, why, and how it was tested.
4. Make sure the CI checks are green.

### Pull Request Checklist

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes
- [ ] `cargo test --lib` and `cargo test --tests` pass
- [ ] New/changed behavior is covered by tests
- [ ] Documentation in `docs/` updated where relevant

## Documentation

The documentation hub is [`docs/README.md`](docs/README.md). When you change behavior:

- Update the relevant guide (e.g. [`docs/language-reference.md`](docs/language-reference.md),
  [`docs/type-system-guide.md`](docs/type-system-guide.md),
  [`docs/memory-safety-guide.md`](docs/memory-safety-guide.md)).
- Add or update a runnable example in [`examples/`](examples/) if your change introduces a
  user-facing capability.

## Reporting Issues

Open a [GitHub issue](https://github.com/adeshlang/adeshlang/issues) with:

- **Title:** concise summary of the problem.
- **Environment:** OS, Rust version, AdeshLang version (`adesh --version`), backend used.
- **Repro:** minimal `.adesh` snippet plus the exact command and expected vs. actual output.
- **Impact:** does it block development, crash, or cause wrong behavior?

## Reporting Security Issues

Do **not** report security vulnerabilities in public issues. Follow the process in
[`SECURITY.md`](SECURITY.md) — report via **GitHub Security Advisories (private)** so the
issue can be fixed and disclosed responsibly.

---

Thank you for contributing to AdeshLang!
