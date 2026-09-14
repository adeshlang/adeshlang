<div align="center">

# Adesh Language Support for Visual Studio Code

**Official VS Code Extension for the [AdeshLang](https://adeshlang.org) Programming Language**

[![Version](https://img.shields.io/badge/Version-v0.3.0-blue.svg)](https://adeshlang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](https://github.com/ajaytainwala-dev/mylang/blob/main/LICENSE)
[![Website](https://img.shields.io/badge/Website-adeshlang.org-purple.svg)](https://adeshlang.org)
[![Author](https://img.shields.io/badge/Author-Ajay%20Tainwala-orange.svg)](https://adeshlang.org)
[![Repository](https://img.shields.io/badge/GitHub-ajaytainwala--dev%2Fmylang-lightgrey.svg)](https://github.com/ajaytainwala-dev/mylang)

*Compile-Time Memory Safety • Zero Garbage Collection pauses • ALS Language Server Protocol • CFG Borrow Checker • Inlay Hints • ADL Manifest Tooling*

</div>

---

## 🌟 Overview

The **Adesh Language Support** extension delivers a first-class developer experience for **[AdeshLang](https://adeshlang.org)** (`.adesh`, `.vy`) source code and **ADL** (`.adl`, `adesh.adl`, `adesh.lock.adl`) package manifests in Visual Studio Code.

Powered by the high-performance **Adesh Language Server (ALS)**, this extension brings TypeScript-grade IDE responsiveness, compile-time borrow checking, real-time diagnostics, rich inlay hints, type inference, semantic token highlighting, and package manager automation directly to your editor.

---

## ✨ Features

### 🧠 Language Server Protocol (ALS) Integration
- **Automatic Server Discovery**: Finds your `als` binary automatically via user configuration (`adesh.serverPath`), workspace release/debug builds, or system `PATH`.
- **Status Bar Integration**: Visual status indicators (`Starting`, `Running`, `Stopped`, `Error`) with one-click access to server output logs.
- **Server Management**: Quick commands to start, stop, restart, or rebuild the language server from source.

### 🔒 CFG Ownership & Borrow Checking
- **Inline Diagnostics**: Catch ownership moves, use-after-move, mutable alias conflicts, and lifetime errors as you type.
- **Borrow Information on Hover**: Inspect ownership states, borrow paths, and lifetime scopes directly in hover tooltips.
- **Ownership Graph & Diagnostics Refresh**: Trigger full CFG borrow analysis on demand with `Adesh: Run Borrow Checker`.

### 💡 Intelligent Code Completion (IntelliSense)
- **Type-Aware Suggestions**: Fast, context-sensitive recommendations for variables, functions, classes, interfaces, and methods.
- **Keywords & Built-in Modules**: Auto-complete standard library namespaces (`Crypto`, `TLS`, `HTTP`, `WebSocket`, `IO`, `Math`, `Random`, `Time`, `System`) and language keywords.
- **Toggle on the Fly**: Toggle completion on or off anytime via the command palette.

### 🔍 Type Inference & Inlay Hints
- **Flow-Sensitive Type Inlay Hints**: View inferred types inline for untyped bindings.
- **Ownership State Hints**: Visual hints showing variable ownership and borrow states (`Unborrowed`, `Shared`, `Exclusive`).
- **Signature Help**: Parameter information and documentation for function calls.

### 🎨 Semantic Syntax Highlighting & Grammars
- Comprehensive TextMate grammars for Adesh source files (`.adesh`, `.vy`) and ADL manifests (`.adl`).
- Semantic token colors distinguishing:
  - `borrowedVariable` & `ownedVariable`
  - `movedVariable` (consumed/moved out)
  - `droppedVariable`
  - `unsafeFunction` & unsafe blocks
  - ARC `sharedVariable`, `strongReference`, and `weakReference`
  - Memory `region` blocks

### 📦 ADL (Adesh Dependency Language) Manifest Support
- **Full Schema Validation**: Instant JSON/ADL schema validation for `adesh.adl` and `*.adl`.
- **Dependency Management**: Interactive `Adesh: Add Dependency` command to insert dependencies with version constraints.
- **Lockfile Generation**: Generate fresh `adesh.lock.adl` templates with `Adesh: Create Lockfile Template`.
- **Manifest Formatting**: Align key-value pairs and validate project sections.

### 📐 Formatting & Editing Ergonomics
- Integrated document formatting via ALS (`Adesh: Format Document`).
- Configurable indentation size and space/tab preferences.
- Bracket pair colorization and auto-closing pairs for brackets, strings, and doc comments.
- Code snippets for common Adesh constructs (functions, structs, classes, match expressions, error handling, ARC sharing).

---

## 🚀 Getting Started

### 1. Installation
Install the extension from the Visual Studio Code Marketplace or Open VSX Registry:
```text
ext install AdeshLang.adesh-vscode
```

### 2. Prerequisites
Ensure you have the Adesh toolchain and `als` (Adesh Language Server) installed:
- Visit **[adeshlang.org](https://adeshlang.org)** for pre-built binaries and installation instructions.
- Or build `als` directly from the repository:
  ```bash
  cd als
  cargo build --release
  ```

### 3. Verification
Open any `.adesh` or `.adl` file. The status bar at the bottom right will show `$(check) Adesh` once the language server is running.

---

## ⚙️ Configuration Settings

Customize extension behavior in your `settings.json`:

| Setting | Type | Default | Description |
|---|---|---|---|
| `adesh.serverPath` | `string` | `""` | Custom path to the `als` executable. If empty, searches workspace and `PATH`. |
| `adesh.server.enabled` | `boolean` | `true` | Enable or disable the Adesh Language Server. |
| `adesh.server.timeout` | `number` | `30000` | Timeout (ms) for language server startup. |
| `adesh.trace.server` | `string` | `"off"` | Trace LSP messages (`off`, `messages`, `verbose`). |
| `adesh.borrowChecker.enabled` | `boolean` | `true` | Enable CFG-based borrow checking diagnostics. |
| `adesh.borrowChecker.showOwnershipHints` | `boolean` | `true` | Show ownership state hints on hover. |
| `adesh.borrowChecker.highlightBorrows` | `boolean` | `true` | Highlight borrowed and moved variables with semantic tokens. |
| `adesh.inlayHints.enabled` | `boolean` | `true` | Enable inlay hints for types and ownership. |
| `adesh.inlayHints.showOwnership` | `boolean` | `true` | Show ownership status in inlay hints. |
| `adesh.inlayHints.showBorrowState` | `boolean` | `false` | Show borrow state (`Unborrowed`, `Shared`, `Exclusive`) in inlay hints. |
| `adesh.completion.enabled` | `boolean` | `true` | Enable type-aware auto-completion suggestions. |
| `adesh.completion.includeKeywords` | `boolean` | `true` | Include language keywords in completion list. |
| `adesh.completion.includeBuiltins` | `boolean` | `true` | Include standard built-in modules in completions. |
| `adesh.diagnostics.enabled` | `boolean` | `true` | Enable real-time compiler diagnostics in the editor. |
| `adesh.diagnostics.refreshOnSave` | `boolean` | `true` | Re-run full diagnostics when saving files. |
| `adesh.typeAssistance.enabled` | `boolean` | `true` | Enable type checking, inference, and type assistance. |
| `adesh.typeAssistance.showInferredTypes` | `boolean` | `true` | Show inferred types on variables without annotations. |
| `adesh.format.enable` | `boolean` | `true` | Enable document formatting for Adesh files. |
| `adesh.format.indentSize` | `number` | `4` | Indentation spaces per level. |
| `adesh.adl.validate.enabled` | `boolean` | `true` | Enable validation for ADL manifest files. |

---

## ⌨️ Commands

Access these commands from the Command Palette (`Ctrl+Shift+P` / `Cmd+Shift+P`):

| Command | Identifier | Description |
|---|---|---|
| **Adesh: Restart Language Server** | `adesh.restartServer` | Restarts the ALS language server instance. |
| **Adesh: Stop Language Server** | `adesh.stopServer` | Stops the running language server. |
| **Adesh: Format Document** | `adesh.formatDocument` | Formats the active Adesh or ADL file. |
| **Adesh: Show Borrow Information** | `adesh.showBorrowInfo` | Displays ownership and borrow details at cursor. |
| **Adesh: Run Borrow Checker** | `adesh.runBorrowCheck` | Forces a borrow checker diagnostic pass. |
| **Adesh: Show Server Output** | `adesh.showOutput` | Opens the Adesh Language Server output channel. |
| **Adesh: Toggle Auto-Completion** | `adesh.toggleCompletion` | Toggles IntelliSense auto-completion. |
| **Adesh: Toggle Diagnostics** | `adesh.toggleDiagnostics` | Toggles inline diagnostic reporting. |
| **Adesh: Validate ADL Manifest** | `adesh.validateAdl` | Validates active ADL manifest syntax and schema. |
| **Adesh: Create Lockfile Template** | `adesh.createLockfile` | Generates a new `adesh.lock.adl` template. |
| **Adesh: Add Dependency** | `adesh.addDependency` | Interactively prompts and adds an ADL dependency. |

---

## 🧩 Supported File Types

- `.adesh` — AdeshLang source files
- `.vy` — AdeshLang alternate source extension
- `.adl` — Adesh Dependency Language manifests
- `adesh.adl` — Project manifest configuration
- `adesh.lock.adl` — Dependency lockfile

---

## 🔗 Links & Resources

- 🌐 **Official Website**: [https://adeshlang.org](https://adeshlang.org)
- 📖 **Documentation**: [https://adeshlang.org/docs](https://adeshlang.org)
- 🐙 **Source Code & Issues**: [https://github.com/ajaytainwala-dev/mylang](https://github.com/ajaytainwala-dev/mylang)
- 💬 **Discussions**: [https://github.com/ajaytainwala-dev/mylang/discussions](https://github.com/ajaytainwala-dev/mylang/discussions)

---

## 👤 Author & Maintainer

- **Ajay Tainwala** — *Creator of AdeshLang*
- Organization: **AdeshLang** ([adeshlang.org](https://adeshlang.org))

---

## 📄 License

This extension is licensed under the [MIT License](LICENSE).
