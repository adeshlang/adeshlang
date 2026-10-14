# ARO (Adesh Runtime Observatory) — Architecture Specification

## 1. Executive Summary

**Adesh Runtime Observatory (ARO)** is the unified, production-grade observability platform for the AdeshLang ecosystem. It instruments and tracks the compiler toolchain (`adesh`, `adl`), the native linker (`adeshlink`), runtime execution (interpreter, bytecode VM, JIT, native binary), memory allocations, and hardware resources.

ARO features three client interfaces sharing a single Rust systems core:
1. **ARO Desktop Application**: Tauri 2 + React + TypeScript + Tailwind CSS.
2. **ARO Interactive TUI**: Terminal UI built in Rust using `ratatui` and `crossterm`.
3. **ARO CLI**: Fast scriptable CLI supporting stdout tables, `--json`, and `--csv` exports.

---

## 2. Workspace Crate Topology

```
AdeshLang/
├── ARO/                                    # ARO Root & Metadata Directory
│   ├── metadata.json                       # Component metadata & schema registry
│   └── README.md
├── crates/
│   ├── adesh-monitor-protocol/             # Strongly-typed Serde event schema & versions
│   ├── adesh-monitor-platform/             # Windows / Linux / macOS OS resource metrics
│   ├── adesh-monitor-core/                 # Ingest engine, SQLite session storage, IPC server
│   ├── adesh-monitor-cli/                  # Scriptable CLI interface (also integrated in `adesh monitor`)
│   ├── adesh-monitor-tui/                  # Ratatui + Crossterm terminal UI
│   └── adesh-monitor-desktop/              # Tauri 2 Desktop App + React/TS dashboard
├── src/                                    # Main compiler & CLI (adesh)
└── linker/                                 # adeshlink native linker
```

---

## 3. Communication & IPC Architecture

- **Transport:**
  - Windows: Named Pipe (`\\.\pipe\adesh_observatory_ipc`)
  - Unix (Linux/macOS): Unix Domain Socket (`/tmp/adesh_observatory.sock`)
- **Protocol:**
  - Newline-delimited JSON (NDJSON) or framed binary header + JSON payload.
  - Non-blocking send from toolchain processes to prevent compilation latency or stalls if the collector is stopped or busy.
- **Clock Discipline:**
  - Durations computed exclusively using monotonic clocks (`Instant`).
  - UTC wall-clock (`chrono::Utc`) used strictly for wall timestamps and chronological indexing.

---

## 4. Instrumentation Points

1. **Compiler Stages (`src/cli/parsing.rs`, `src/cli/commands.rs`):**
   - Lexing, Parsing, Compile-time Memory Safety Pass, Decorator Compile Passes, RAII transformations, AST to HIR, Optimization Passes, Backend Lowering.
2. **Linker Pipeline (`linker/src/linker.rs`, `linker/src/main.rs`):**
   - Object reading, Symbol resolution, Archive scanning, Section GC, ICF folding, Relocation resolution, Target emission (PE, ELF, Mach-O, WASM).
3. **Execution Runtimes:**
   - Interpreter / VM execution, JIT compilation & tier transitions, Native binary execution.
