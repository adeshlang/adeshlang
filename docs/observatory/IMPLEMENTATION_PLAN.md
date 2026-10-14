# ARO (Adesh Runtime Observatory) — Implementation Plan

## Phase 0: Baseline & Reconnaissance (COMPLETED)
- [x] Workspace review (`Cargo.toml`, crates structure).
- [x] Test baseline verified (`cargo test --lib`: 895 passed; `cargo test -p adesh-linker`: 37 passed).
- [x] Created `ARO` folder with `metadata.json` and `README.md`.
- [x] Created `docs/observatory/ARCHITECTURE.md` and `docs/observatory/IMPLEMENTATION_PLAN.md`.

## Phase 1: Shared Protocol & Platform Crates (`adesh-monitor-protocol`, `adesh-monitor-platform`) (COMPLETED)
- [x] Implemented `crates/adesh-monitor-protocol`:
  - Versioned events (`SessionStart`, `SessionEnd`, `ProcessMetrics`, `CompilerStage`, `LinkerStage`, `RuntimeEvent`).
  - Monotonic elapsed duration tracking (`Instant`) and UTC ISO 8601 timestamps.
  - TelemetryEnvelope with UUID correlation, schema validation, and message size limits.
  - Serialization / deserialization tests passing.
- [x] Implemented `crates/adesh-monitor-platform`:
  - Cross-platform trait `MetricsProvider` for process metrics, CPU, memory (RSS/Working set), thread counts.
  - Native Windows implementation (`windows-sys` OpenProcess, GetProcessMemoryInfo, GetProcessTimes).
  - Native Linux implementation (`/proc` parser for stat, ticks, page sizes, threads, RSS).
  - Native macOS/Unix fallback provider.
  - Platform unit tests passing.

## Phase 2: Monitoring Core & Storage (`adesh-monitor-core`) (COMPLETED)
- [x] SQLite WAL schema with indexing for sessions, events, stage traces, and resource metrics.
- [x] Bounded queue ingestion engine with backpressure and worker thread.
- [x] IPC client (`TelemetryClient`) with non-blocking best-effort dispatch.
- [x] IPC RPC server handlers (`CollectorRequest` / `CollectorResponse`) for listing sessions, getting session details, and telemetry queries.
- [x] Core unit and integration tests passing.

## Phase 3: CLI & TUI Implementations (`adesh-monitor-cli`, `adesh-monitor-tui`) (COMPLETED)
- [x] `crates/adesh-monitor-cli` (`aro`):
  - Commands: `list`, `show`, `trace`, `stats`, `export` (`--format json|csv`), `doctor`, `purge`, `version`.
  - Machine-readable JSON and CSV streaming output.
  - Native integration with `HistoryStore`.
- [x] `crates/adesh-monitor-tui` (`aro-tui`):
  - Full-featured Ratatui + Crossterm interactive terminal UI.
  - Live session list, status color coding, elapsed duration display, dynamic refresh, keyboard navigation (q/Esc/r).
- [x] Integrated `adesh monitor` subcommand forwarding in `src/main.rs` and `src/cli/observatory.rs`.

## Phase 4: Compiler and Linker Instrumentation (COMPLETED)
- [x] Scoped telemetry spans in compiler pipeline (`src/cli/parsing.rs`):
  - Stages: `parse`, `decorator_phases`, `raii_transformation`, `hir_lower`, `compile_time_memory_safety`.
  - Non-blocking emission via `TelemetryClient`.
- [x] Telemetry in `adeshlink` (`linker/src/main.rs`):
  - Emits `LinkerStageEvent` on link start, completion, and error with input/output size tracking.
- [x] Compiler and linker execute without error or performance degradation when collector is offline.

## Phase 5: Desktop Application (`adesh-monitor-desktop` & `ARO/desktop`) (COMPLETED)
- [x] Tauri 2 desktop backend bridge in `crates/adesh-monitor-desktop`.
  - Tauri commands: `desktop_list_sessions`, `desktop_get_session_detail`, `desktop_get_collector_stats`.
- [x] Modern React + TypeScript + Tailwind CSS UI in `ARO/desktop`:
  - Full state management with `useReducer` and React Context (`ObservatoryContext.tsx`).
  - TopBar with omni-search, status indicator, filter mode dropdown, manual and interval-based polling.
  - ModernSidebar with active session indicators and badge counters.
  - CockpitOverview: KPI cards (Active Sessions, Total Traced, Events Ingested, Platform Status), Recharts CPU/Memory time-series chart, and interactive sessions data table.
  - StagesTimeline: Microsecond drill-down of compiler phases (`parse`, `hir_lower`, etc.) and linker stages with progress bars and status badges.
  - Hardware & Platform Discovery: Native OS capabilities inspector.
  - Linker Diagnostics: `adeshlink` PE/COFF, ELF, Mach-O metrics.
  - Desktop package configured with Vite and Tailwind.
