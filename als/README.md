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
# Navigate to ALS directory
cd als

# Build in release mode
cargo build --release

# The binary will be at target/release/als
```

### Adding to PATH

**Linux/macOS:**
```bash
# Option 1: Add to PATH
export PATH="$PATH:/path/to/mylang/als/target/release"

# Option 2: Create symlink (recommended)
sudo ln -s /path/to/mylang/als/target/release/als /usr/local/bin/als
```

**Windows (PowerShell):**
```powershell
# Add to PATH
$env:Path += ";C:\path\to\mylang\als\target\release"

# Or set permanently via System Properties > Environment Variables
```

### Verify Installation

```bash
# Check ALS is accessible
als --version

# Or run directly (ALS expects LSP messages on stdin/stdout)
echo '{"jsonrpc":"2.0","id":1,"method":"initialize"}' | als
```

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
     "adesh.serverPath": "/path/to/als/target/release/als"
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
language-server = { command = "als" }

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
   als --help
   ```

3. **Verify VS Code extension loaded:**
   - Open Output panel (`Ctrl+Shift+U`)
   - Select "Adesh Language Server" channel
   - Check for error messages

### No Completions/Hover

1. **Ensure document is saved** (ALS analyzes file content)
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

## Known Limitations

- **Cross-file navigation**: Go-to-definition only works within the current file
- **Type inference**: Limited type information for complex expressions
- **Refactoring**: Rename only affects current document
- **Workspace symbols**: Search limited to open documents

## License

MIT License - see the main repository LICENSE file.
