# Adesh Runtime Observatory (ARO) — User & Operator Guide

**Adesh Runtime Observatory (ARO)** is the production-grade, multi-interface monitoring and diagnostics platform for the AdeshLang compiler toolchain and execution runtime.

---

## 1. Architectural Overview & Component Layout

ARO provides three first-class monitoring interfaces powered by a unified native Rust core (`adesh-monitor-core`):

```
                                  +-----------------------------+
                                  |   Adesh Runtime Engine &    |
                                  |  Compiler (`adesh`, linker) |
                                  +--------------+--------------+
                                                 |
                                     (Non-blocking Telemetry)
                                                 |
                                  +--------------v--------------+
                                  |     `adesh-monitor-core`    |
                                  |  - SQLite WAL History Store |
                                  |  - Bounded Ingestion Queue  |
                                  |  - Native IPC Named Pipe/RPC|
                                  +--------------+--------------+
                                                 |
                   +-----------------------------+-----------------------------+
                   |                             |                             |
      +------------v------------+   +------------v------------+   +------------v------------+
      |  Tauri 2 Desktop App    |   |  Interactive TUI        |   |  Scriptable CLI         |
      |  (React 18, Tailwind,   |   |  (`aro-tui` / Ratatui)  |   |  (`aro` / `adesh monitor|
      |   Recharts, pnpm)       |   |                         |   |   JSON/CSV exports)     |
      +-------------------------+   +-------------------------+   +-------------------------+
```

---

## 2. Desktop Application (Tauri 2 + React + TypeScript + pnpm)

The Desktop App is a high-performance, GPU-accelerated cockpit featuring dark-mode glassmorphic aesthetics, Recharts time-series telemetry, microsecond-level compiler stage drilldowns, and hardware capability inspection.

### Prerequisites:
- **Node.js**: v18+ (verified with v24)
- **pnpm**: v9+ (verified with v11.25)
- **Rust**: 2021/2024 edition with MSVC tools on Windows (or Xcode/GCC on macOS/Linux)
- **WebView2**: Built into Windows 10/11

### Running the Desktop App:

#### A. Running the Pre-compiled Binary Directly (Immediate Launch)
The native Windows executable is located at:
```powershell
.\ARO\desktop\src-tauri\target\debug\adesh-observatory-desktop.exe
```
Or start via PowerShell:
```powershell
Start-Process ".\ARO\desktop\src-tauri\target\debug\adesh-observatory-desktop.exe"
```

#### B. Running via pnpm & Vite Development Server
Navigate to `ARO/desktop`:
```powershell
cd ARO\desktop
pnpm install
pnpm run build
```
To run in live development mode:
```powershell
pnpm run dev
```
Open [http://localhost:1420](http://localhost:1420) in your browser, or launch the Tauri window:
```powershell
pnpm tauri dev
```

---

## 3. Interactive Terminal UI (`aro-tui`)

For headless servers, SSH sessions, and CI environments, ARO includes a keyboard-driven terminal dashboard.

### Launching:
```powershell
# Directly via cargo / binary:
cargo run -p adesh-monitor-tui

# Or through the unified AdeshLang toolchain:
adesh monitor tui
```

### Keybindings:
- `r`: Force instant refresh of all sessions and metrics.
- `q` or `Esc`: Gracefully detach and exit the TUI.
- `↑` / `↓` / `j` / `k`: Scroll through monitored execution sessions.

---

## 4. Scriptable Command-Line Interface (`aro`)

ARO provides a CLI for automation, scripted queries, and machine-readable exports.

### Commands:

#### 1. System Health & Platform Capabilities
```bash
aro doctor
```
*Outputs native OS capabilities (CPU metrics, memory RSS/working set, logical cores, and IPC connectivity status).*

#### 2. Listing Invocations & Telemetry Sessions
```bash
# Pretty-printed table
aro list

# Limit output and format as JSON
aro list --limit 10 --json
```

#### 3. Deep Session Inspection
```bash
aro show <SESSION-UUID>
```

#### 4. Compiler & Linker Microsecond Trace Breakdown
```bash
aro trace <SESSION-UUID>
```
*Outputs elapsed times for compiler phases (`parse`, `decorator_phases`, `raii_transformation`, `hir_lower`, `compile_time_memory_safety`) and linker passes (`adeshlink`).*

#### 5. Data Export for CI & Performance Regression Pipelines
```bash
# Export as JSON
aro export <SESSION-UUID> --format json --out session_report.json

# Export as CSV
aro export <SESSION-UUID> --format csv --out session_report.csv
```

#### 6. Purging Stored History
```bash
aro purge
```

---

## 5. Integrating with Your Adesh Programs

To record detailed telemetry for any Adesh script, run your program with the session tracking flag:

```powershell
$env:ADESH_OBSERVATORY_SESSION = "my-test-session"
adesh run tests/main.adesh
```

The telemetry will instantly appear across the **Desktop Cockpit**, **TUI**, and **CLI**.
