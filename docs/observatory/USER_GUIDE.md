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

### Building & Packaging the Desktop App:

You can compile the production release desktop application using `pnpm` and `cargo`:

```powershell
cd ARO\desktop
pnpm install
pnpm run build
cargo build --release --manifest-path src-tauri/Cargo.toml
```

This runs TypeScript typechecking, compiles and minifies frontend assets with Vite, embeds the official Adesh logo, and builds the optimized Rust native application. The generated binary is located at:
- **Standalone Release Executable**:
  `ARO\desktop\src-tauri\target\release\adesh-observatory-desktop.exe`

### Desktop Placement & Installers:
When installing AdeshLang via the Windows Setup (Inno Setup / WiX), Linux installer (`install-linux.sh` / `.deb` / `.rpm`), or macOS installer (`install-macos.sh` / `.pkg` / `.dmg`), an **Adesh Observatory** desktop shortcut is automatically placed on your Desktop with the official high-resolution Adesh logo, enabling instant one-click launch.

### Responsive UI for All Screen Sizes:
The Desktop Cockpit adaptively adjusts to any display resolution:
- **Compact & Laptop Screens**: Sidebar transforms into a slim icon navigation rail, metrics arrange in 1-column or 2-column grids, and table views support smooth horizontal scrolling.
- **Large & Ultra-wide Displays**: Expands into a multi-column cockpit with full telemetry cards, real-time Recharts duration/RSS visualizer, and stage waterfall views.

### Running the Desktop App:

#### A. Running the Pre-compiled Release Binary
```powershell
.\ARO\desktop\src-tauri\target\release\adesh-observatory-desktop.exe
```
Or launch detached in background via PowerShell:
```powershell
Start-Process ".\ARO\desktop\src-tauri\target\release\adesh-observatory-desktop.exe"
```

#### B. Live Development Mode (Hot-Reloading)
Navigate to `ARO/desktop` and launch the Tauri development window:
```powershell
cd ARO\desktop
pnpm tauri dev
```
To run only the Vite web dev server:
```powershell
pnpm run dev
```
Then view at [http://localhost:1420](http://localhost:1420).

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
- `c` or `x`: Purge / Clear all recorded sessions and reset dashboard.
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

## 5. Automatic Telemetry & Zero Configuration

Every execution of the Adesh toolchain automatically instruments and records telemetry directly:

- Any `adesh run <file>`, `adesh compile <file>`, `adesh test`, or `adeshlink` execution immediately records compiler stages, execution mode, duration, exit code, and process resources.
- Telemetry writes to SQLite WAL (`~/.adesh/observatory.db`) with zero overhead (<0.1ms) when offline, or broadcasts over local IPC named pipes / unix sockets when the collector is active.
- To provide a custom human-readable session ID, optionally specify:
  ```powershell
  $env:ADESH_OBSERVATORY_SESSION = "my-custom-benchmark"
  adesh run tests/test_large_loop.adesh
  ```

---

## 6. Dashboard Controls (Desktop App)

- **Refresh Telemetry Button**: Top-right navigation bar button to immediately re-poll telemetry.
- **Clear Dashboard Button**: Top-right trash icon button (with confirmation modal) to purge all recorded sessions from disk and reset the cockpit.
- **Microsecond Trace Waterfall**: Click on any session card in Cockpit Overview to view the chronological breakdown of all compiler and linker stages.
