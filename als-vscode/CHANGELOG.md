# Changelog

All notable changes to the **Adesh Language Support** extension will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-09-14

### Added
- **Official Metadata & Deployment Preparation**:
  - Configured official website URL: [adeshlang.org](https://adeshlang.org)
  - Added author metadata (`Ajay Tainwala`) and publisher (`AdeshLang`)
  - Added marketplace categories, gallery banners, badges, and workspace trust capabilities
  - Added `.vscodeignore` optimization for lightweight marketplace distribution
  - Added full `LICENSE` (MIT) and documentation suite
- **ADL (Adesh Dependency Language) Tools**:
  - Interactive commands: `Adesh: Add Dependency`, `Adesh: Create Lockfile Template`, `Adesh: Validate ADL Manifest`
  - Manifest schema validation against `schemas/adl-schema.json`
  - Dedicated syntax highlighting and bracket matching for `.adl` files
- **CFG Borrow Checker Diagnostics**:
  - Semantic token classification for borrowed, owned, moved, and dropped variables
  - Inlay hints for borrow states (`Unborrowed`, `Shared`, `Exclusive`)
  - Automatic diagnostics refresh on save
- **IntelliSense Enhancements**:
  - Enhanced type inference inlay hints
  - Standard library namespace completions (`Crypto`, `TLS`, `HTTP`, `WebSocket`, `IO`, `Math`, `Random`, `Time`, `System`)
  - Toggle commands for completions and diagnostics

## [0.2.0] - 2026-06-15

### Added
- Language Server Protocol (LSP) client over stdio
- Automatic discovery of local and workspace `als` binaries
- Real-time diagnostics for syntax and type errors
- Hover tooltips and Go to Definition support
- Document formatting provider

## [0.1.0] - 2026-01-20

### Added
- Initial release of Adesh Language Support for VS Code
- TextMate grammar syntax highlighting for `.adesh` and `.vy` files
- Language configuration: comments, bracket pairs, and auto-closing quotes
- Basic code snippets for language constructs
