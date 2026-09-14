# ALS for Sublime Text

AdeshLang Language Server support for Sublime Text using the LSP package.

## Features

- Syntax highlighting
- Auto-completion
- Hover information
- Go-to-definition
- Find references
- Document symbols
- Code formatting
- Diagnostics (errors, warnings)
- Semantic tokens (ownership/borrowing visualization)

## Prerequisites

1. **Sublime Text 4** (or Sublime Text 3 Build 3124+)
2. **Package Control** - Install from [packagecontrol.io](https://packagecontrol.io/installation)

## Installation

### 1. Install LSP Package

Open Command Palette (`Ctrl+Shift+P` or `Cmd+Shift+P`) and run:
```
Package Control: Install Package
```

Search for and install: **LSP**

### 2. Install ALS

Build the ALS language server:

```bash
cd als
cargo build --release
```

Note the absolute path to the `als` binary.

### 3. Configure LSP for AdeshLang

Open LSP settings via Command Palette:
```
Preferences: LSP Settings
```

Add the following to your LSP settings (available in this directory as `LSP.sublime-settings`).

## Usage

See full documentation in README.md

The ALS server now completes native annotation types including `set`, `tuple`,
`object`, `complex`, and fixed-width numeric types. Imaginary literals use the
`5j` syntax.

## License

MIT License
