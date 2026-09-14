# ALS for Helix Editor

AdeshLang Language Server support for [Helix](https://helix-editor.com/).

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
- Inlay hints (type and ownership information)

## Installation

### 1. Install ALS

First, build the ALS language server:

```bash
cd als
cargo build --release
```

Make sure `als` is in your PATH or note its absolute path.

### 2. Configure Helix

Add the following to your Helix configuration file (`~/.config/helix/languages.toml`):

```toml
# AdeshLang language definition
[[language]]
name = "adesh"
scope = "source.adesh"
injection-regex = "adesh"
file-types = ["adesh", "vy"]
comment-token = "//"
comment-tokens = ["//"]
block-comment-tokens = { start = "/*", end = "*/" }
indent = { tab-width = 4, unit = "    " }
language-servers = ["als"]
auto-format = false

# Language server configuration
[language-server.als]
command = "als"  # Or use absolute path: "/path/to/als/target/release/als"
```

### 3. Create TextMate Grammar (Optional)

For better syntax highlighting, create a tree-sitter grammar or use the TextMate grammar.

Create `~/.config/helix/runtime/grammars/sources/adesh/grammar.js`:

```javascript
// Tree-sitter grammar for Adesh
// This is a placeholder - full tree-sitter implementation would be here
module.exports = grammar({
  name: 'adesh',
  rules: {
    source_file: $ => repeat($._definition),
    // ... add full grammar rules
  }
});
```

## Usage

Once configured, Helix will automatically start ALS when you open `.adesh` or `.vy` files.

### Key Bindings

Default Helix key bindings work with ALS:

- **`gd`** - Go to definition
- **`gr`** - Find references
- **`Space + s`** - Document symbols
- **`Space + a`** - Code actions
- **`K`** - Hover information
- **`:format`** - Format document

### Custom Settings

You can customize ALS behavior in your `languages.toml`:

```toml
[language-server.als.config]
format = { indentSize = 4, useTabs = false }
borrowChecker = { enabled = true, showOwnershipHints = true }
unsafe = { warnOnUsage = true }
inlayHints = { enabled = true, showOwnership = true }
```

## Features in Detail

### Ownership and Borrowing

ALS provides special visualization for AdeshLang's ownership system:

- **Semantic tokens** - Different colors for owned, borrowed, moved variables
- **Inlay hints** - Show ownership state inline
- **Diagnostics** - Borrow checker errors with detailed explanations

### Unsafe Blocks

Unsafe blocks are highlighted with warnings when `unsafe.warnOnUsage` is enabled.

### Memory Management

Special highlighting for:
- `alloc`, `free`, `drop` - Memory operations
- `shared`, `unique`, `weak` - Smart pointer types
- `region` - Region-based allocation
- Pointer types (`*i32`, `*u8`, etc.)
- Native annotation types (`set`, `tuple`, `object`, `complex`, and fixed-width
  numeric types) and imaginary literals such as `5j` are provided by the
  configured Helix grammar when installed; ALS supplies completion and
  diagnostics for them.

## Troubleshooting

### ALS Not Starting

1. Check ALS is accessible:
   ```bash
   which als
   # or
   /path/to/als --version
   ```

2. Check Helix logs:
   ```bash
   tail -f ~/.cache/helix/helix.log
   ```

3. Verify configuration:
   ```bash
   hx --health adesh
   ```

### No Completions or Hover

1. Ensure file is saved (`.adesh` extension)
2. Wait a moment for analysis to complete
3. Check ALS is running in process list:
   ```bash
   ps aux | grep als
   ```

### Syntax Highlighting Issues

If syntax highlighting doesn't work:

1. Tree-sitter grammar may not be installed
2. Use TextMate grammar as fallback
3. File must have `.adesh` or `.vy` extension

## Development

To contribute to ALS Helix support:

1. Test changes with: `hx test.adesh`
2. Check logs: `tail -f ~/.cache/helix/helix.log`
3. Report issues on GitHub

## License

MIT License - see main repository LICENSE file.