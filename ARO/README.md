# ARO — Adesh Runtime Observatory

**Adesh Runtime Observatory (ARO)** is the official production-grade observability, telemetry, and runtime monitoring platform for the AdeshLang ecosystem.

## Metadata
- **Name:** ARO
- **Title:** Adesh Runtime Observatory
- **Version:** 0.1.0
- **Ecosystem:** AdeshLang

## Architecture Overview
ARO provides 3 unified interfaces powered by a shared Rust systems core:
1. **ARO Desktop Application**: Built with Tauri 2, React, TypeScript, Vite, Tailwind CSS.
2. **ARO Interactive TUI**: Terminal UI built in Rust using `ratatui` and `crossterm`.
3. **ARO CLI**: Scriptable command-line interface (`adesh monitor` or `aro`).

## Architecture Crates
- `crates/adesh-monitor-protocol`: Versioned, strongly-typed JSON/IPC event schemas.
- `crates/adesh-monitor-platform`: OS-level telemetry provider (Windows, Linux, macOS) for CPU, memory, thread, disk, and hardware accelerator discovery.
- `crates/adesh-monitor-core`: Ingest engine, bounded queue backpressure, session tracking, and SQLite durable history store.
- `crates/adesh-monitor-cli`: Automation-ready CLI with JSON and CSV exports.
- `crates/adesh-monitor-tui`: Interactive keyboard-driven terminal dashboard.
- `crates/adesh-monitor-desktop`: Tauri 2 desktop dashboard.
