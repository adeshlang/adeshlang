# Adesh Language Server (ALS)

A high-performance Language Server Protocol (LSP) implementation for the Adesh programming language.

## Features

### Core LSP Features

- **Syntax Highlighting**: Full syntax highlighting support via TextMate grammar (includes ARC, ownership, memory keywords)
- **IntelliSense & Auto-Completion**: Smart completions for keywords (ARC, ownership, memory), functions, classes, and user-defined symbols
- **Hover Information**: Type information, documentation, and signatures on hover (includes ARC keyword docs)
- **Real-time Diagnostics**: Syntax errors, semantic errors, and type checking
- **Go-to-Definition**: Jump to symbol definitions (functions, classes, variables)
- **Find References**: Find all usages of a symbol across the document
- **Document Symbols**: Outline view with classes, functions, and variables
- **Rename Symbol**: Refactor symbols across the document
- **Code Formatting**: Automatic code formatting using Adesh's formatter
- **Signature Help**: Parameter hints when calling functions
- **Code Actions**: Quick fixes and refactoring suggestions

### Supported Completions

- Keywords: `let`, `const`, `fn`, `class`, `if`, `else`, `while`, `for`, etc.
- ARC keywords: `share`, `strong`, `weak` with documentation
- Ownership keywords: `owned`, `borrowed`, `shared`, `unique`, `noalias`, `move`, `copy`
- Memory keywords: `alloc`, `free`, `drop`, `heap`, `stack`, `region`, `arena`
- Built-in functions: `print`, `input`, `len`, `type`, `map`, `filter`, `reduce`, etc.
- Math namespace: `Math.PI`, `Math.sin`, `Math.random`, etc.
- Time namespace: `time.now`, `time.epoch`, etc.
- User-defined symbols from the current document

## Installation

### Building from Source

```bash
# From the repository root
cargo build -p als --release

# The binary will be at target/release/als
```

### Adding to PATH

**Linux/macOS:**
```bash
# Option 1: Add to PATH
export PATH="$PATH:/path/to/mylang/target/release"

# Option 2: Create symlink (recommended)
sudo ln -s /path/to/mylang/target/release/als /usr/local/bin/als
```

**Windows (PowerShell):**
```powershell
# Add to PATH
$env:Path += ";C:\path\to\mylang\target\release"

# Or set permanently via System Properties > Environment Variables
```

### Verify Installation

ALS communicates over stdio using LSP `Content-Length` framing. Start it
through an editor. It does not implement `--version` or a newline-delimited
JSON command interface.

## Editor Integration

### VS Code

1. **Install the extension:**
   ```bash
   cd als-vscode
   npm install
   npm run compile
   npx vsce package
   ```

2. Install the generated `.vsix` file in VS Code:
   - Press `Ctrl+Shift+P` (or `Cmd+Shift+P` on macOS)
   - Type "Install from VSIX"
   - Select the `.vsix` file

3. **Configure server path** (if not in PATH):
   Open VS Code settings and set:
   ```json
   {
     "adesh.serverPath": "/path/to/AdeshLang/target/release/als"
   }
   ```

### Neovim

See [als-neovim/README.md](../als-neovim/README.md) for detailed setup instructions.

**Quick setup with nvim-lspconfig:**
```lua
-- Add to your init.lua
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

-- Register ALS
if not configs.als then
  configs.als = {
    default_config = {
      cmd = { 'als' },
      filetypes = { 'adesh' },
      root_dir = lspconfig.util.root_pattern('.git', '*.adesh'),
      settings = {},
    },
  }
end

-- Setup ALS
lspconfig.als.setup {}

-- Or use the provided Lua file
require('als').setup()
```

### Helix

Add to `~/.config/helix/languages.toml`:
```toml
[[language]]
name = "adesh"
scope = "source.adesh"
injection-regex = "adesh"
file-types = ["adesh", "vy"]
comment-token = "//"
indent = { tab-width = 4, unit = "    " }
language-servers = ["als"]

[language-server.als]
command = "als"

[[grammar]]
name = "adesh"
source = { path = "/path/to/mylang/als-helix/grammars" }
```

### Sublime Text

1. Install [LSP](https://packagecontrol.io/packages/LSP) package
2. Add to LSP settings:
   ```json
   {
     "clients": {
       "als": {
         "command": ["/path/to/als"],
         "selector": "source.adesh"
       }
     }
   }
   ```

### Emacs

**With lsp-mode:**
```elisp
(use-package lsp-mode
  :hook (adesh-mode . lsp)
  :config
  (lsp-register-client
    (make-lsp-client
      :new-connection (lsp-stdio-connection '("als"))
      :major-modes '(adesh-mode)
      :server-id 'als)))
```

**With eglot:**
```elisp
(add-to-list 'eglot-server-programs '(adesh-mode . ("als")))
```

## Configuration

### VS Code Settings

```json
{
  "adesh.serverPath": "",              // Path to ALS (empty = search PATH)
  "adesh.server.enabled": true,        // Enable/disable server
  "adesh.server.timeout": 30000,       // Server startup timeout (ms)
  "adesh.trace.server": "off",         // Trace level: off/messages/verbose
  "adesh.format.indentSize": 4,        // Indentation size
  "adesh.format.useTabs": false,       // Use tabs instead of spaces
  "adesh.format.enable": true          // Enable formatting
}
```

### Debugging the Server

**Enable verbose logging:**
```bash
RUST_LOG=debug als
```

**Trace communication (VS Code):**
Set `"adesh.trace.server": "verbose"` in settings.

## Architecture

```
als/
├── src/
│   ├── main.rs        # Entry point, LSP connection setup
│   ├── server.rs      # Main server logic, request handling
│   ├── document.rs    # Document management, position tracking
│   ├── analysis.rs    # Code analysis using Adesh parser
│   ├── completion.rs  # Auto-completion provider
│   ├── diagnostics.rs # Diagnostic generation (errors, warnings)
│   ├── hover.rs       # Hover information provider
│   ├── symbols.rs     # Symbol navigation (definition, references)
│   └── formatting.rs  # Code formatting integration
└── Cargo.toml
```

### How It Works

1. **Document Sync**: ALS receives document updates from the editor via LSP
2. **Parsing**: Uses Adesh's lexer and parser to tokenize and parse source code
3. **Analysis**: Extracts symbols, diagnostics, and semantic information
4. **Position Tracking**: Maps AST nodes to source positions using token data
5. **Responses**: Returns LSP-compatible responses (completions, hovers, etc.)

## Troubleshooting

### Server Not Starting

1. **Check ALS is in PATH:**
   ```bash
   which als  # Linux/macOS
   where als  # Windows
   ```

2. **Check ALS runs correctly:**
   ```bash
   # Start through your editor and inspect its LSP log.
   ```

3. **Verify VS Code extension loaded:**
   - Open Output panel (`Ctrl+Shift+U`)
   - Select "Adesh Language Server" channel
   - Check for error messages

### No Completions/Hover

1. **Ensure the document is open** (ALS analyzes the current editor buffer, including unsaved edits)
2. **Check file extension** is `.adesh` or `.vy`
3. **Restart the server:**
   - VS Code: `Ctrl+Shift+P` → "Adesh: Restart Language Server"

### Performance Issues

- Large files may take longer to analyze
- Consider disabling features: `"adesh.server.enabled": false`
- Check system resources (RAM, CPU)

## Development

### Running Tests

```bash
cd als
cargo test
```

### Building Debug Version

```bash
cargo build
# Binary at target/debug/als
```

### Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run tests: `cargo test`
5. Submit a pull request

## Navigation and editing

ALS advertises declaration, type definition, implementation, document highlights,
call hierarchy, type hierarchy, linked editing, code lenses, semantic tokens,
inlay hints, folding, selection ranges, and import links.

- Completion resolves chained receivers, offers namespace members and workspace
  auto-import edits, and includes `.if`, `.for`, `.let`, `.return`, `.match`,
  `.not`, and `.print` postfix templates.
- Signature help tracks nested calls and active arguments, including incomplete
  constructor calls during editing.
- Diagnostics include unused bindings/imports, unreachable code, known-call
  arity, unknown members with suggestions, missing interface methods, immutable
  reassignment, synchronous `await`, and optional explicit-annotation warnings.
- Code actions include type annotations, documentation, import organization,
  missing interface methods, typo fixes, and selection-based refactorings.
  Review refactor previews before applying them.

### Editor support

VS Code registers `adesh.runFile`, `adesh.runTest`, `adesh.showReferences`,
`adesh.showImplementations`, `adesh.organizeImports`, and
`adesh.toggleInlayHints`. Run/Test use a process task, not shell interpolation,
and require workspace trust. Set `adesh.executablePath` for the compiler.

The Neovim, Emacs, Helix, and Sublime configurations use native LSP navigation
and code actions. Their examples disable code lenses because they do not
register the VS Code-specific lens commands. Server support does not imply that
every editor exposes every LSP method. Neovim also maps incoming/outgoing calls
to `<space>ci` / `<space>co` and organize imports to `<space>oi`.

### Runtime settings

Send settings through `initializationOptions`, then
`workspace/didChangeConfiguration`. Both `{ "adesh": { ... } }` and the
unwrapped settings object are accepted. Partial updates retain omitted values.
The server republishes diagnostics for open documents when settings change.

```json
{
  "inlayHints": {
    "enabled": true,
    "showTypes": true,
    "showParameterNames": true,
    "showReturnTypes": true,
    "showOwnership": true,
    "maxLength": 32
  },
  "completion": { "enabled": true, "autoImport": true, "postfix": true },
  "diagnostics": {
    "enabled": true,
    "unusedWarnings": true,
    "unreachableWarnings": true,
    "arityChecks": true,
    "memberChecks": true
  },
  "codeLens": {
    "enabled": true,
    "references": true,
    "implementations": true,
    "runTests": true
  },
  "typeAssistance": { "strictMode": false }
}
```

Formatting requests honor LSP `tabSize` / `insertSpaces`, with
`format.indentSize` / `format.useTabs` as defaults. `format.enable` disables
formatting. Strict mode is an editor annotation policy, not a separate compiler
type system.

### Validation

From the repository root:

```sh
cargo test -p als
cd als-vscode
npm run compile
```

`als/tests/stdio.rs` launches the actual executable and exercises LSP framing,
initialization capabilities, edits, requests, and clean shutdown.
`als/tests/protocol.rs` covers provider routing, configuration, safe named
import removal, workspace lifecycle, and UTF-16 semantic token positions.

With Neovim 0.11+ installed, also run from the repository root:

```sh
nvim --headless -u NONE -l als/tests/nvim_smoke.lua
```
## Known Limitations

- Startup indexing is synchronous and capped at 500 files per scanned root; generated
  build directories and symlinks are skipped. This is not a large-workspace
  background indexing engine.
- Complex generic inference and some incomplete syntax remain limited by the
  compiler semantic index.
- Some workspace reference and hierarchy searches still match names. Ambiguous
  same-named symbols and nested shadowing need further resolution work before
  rename can be considered rust-analyzer-equivalent.
- Ordinary semantic token positions are UTF-16; not every legacy navigation or
  diagnostic path has been audited for non-ASCII columns.
- Advanced borrow-state/chaining hint settings are retained for compatibility;
  they do not yet provide dataflow-based ownership visualization.
- Emacs, Helix, and Sublime runtime behavior requires validation in those editors.

## License

MIT License - see the main repository LICENSE file.
