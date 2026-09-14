# ALS - Adesh Language Server for Neovim

Neovim integration for the Adesh Language Server (ALS), providing full IDE support for AdeshLang with CFG v2.1/v2.2 ownership and borrowing features.

## Features

### Language Features
- **Syntax highlighting** with ownership-aware coloring
- **Code completion** with ownership/borrowing keyword support
- **Hover documentation** showing ownership state and borrow information
- **Diagnostics** with CFG-based borrow checker errors
- **Go-to-definition** and **find references**
- **Signature help** for functions with ownership annotations
- **Formatting** support

### CFG v2.2 Specific Features
- **Ownership keywords**: `owned`, `borrowed`, `shared`, `unique`, `noalias`, `move`, `copy`
- **Memory management**: `alloc`, `free`, `drop`, `heap`, `stack`, `region`, `arena`
- **Unsafe blocks**: Visual highlighting for unsafe code regions
- **Borrow operators**: `&` (shared) and `&mut` (exclusive) highlighting

### Error Codes
ALS provides detailed error messages for borrow checking violations:

| Code | Description |
|------|-------------|
| E0501 | Conflicting borrow states across branches |
| E0502 | Cannot borrow as mutable (already borrowed) |
| E0503 | Use of moved value |
| E0504 | Cannot move out of borrowed content |
| E0505 | Value may have been moved in branch |
| E0506 | Cannot free while borrowed |
| E0507 | Use after free |
| E0508 | Double free |
| E0509 | Drop while borrowed |
| E0510 | Different exclusive borrows across branches |
| E0511 | Value freed in some branches |
| E0512 | Value moved inside loop |
| E0513 | Cannot mutate collection while iterating |
| E0514 | Borrow escapes loop scope |

## Installation

### Prerequisites
1. Neovim 0.8+ (0.10+ recommended for inlay hints)
2. [nvim-lspconfig](https://github.com/neovim/nvim-lspconfig)
3. ALS binary in your PATH (build from `als/` directory)

### Building ALS
```bash
cd path/to/AdeshLang/als
cargo build --release
# Add target/release to your PATH, or copy als binary to a bin directory
```

### Setup with nvim-lspconfig

Copy `als.lua` to your Neovim config or require it directly:

```lua
-- Option 1: Add to your init.lua
local als = require('als')  -- Assuming als.lua is in your lua path
als.setup()

-- Option 2: Include directly in your LSP config
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

if not configs.als then
  configs.als = {
    default_config = {
      cmd = { 'als' },
      filetypes = { 'adesh' },
      root_dir = function(fname)
        local util = require('lspconfig.util')
        return util.find_git_ancestor(fname) or vim.fn.getcwd()
      end,
    },
  }
end

lspconfig.als.setup({
  on_attach = your_on_attach,
  capabilities = your_capabilities,
})
```

### Filetype Detection
ALS automatically registers `.adesh` and `.vy` extensions. If not working, add:

```lua
vim.filetype.add({
  extension = {
    adesh = 'adesh',
    vy = 'adesh',
  },
})
```

The bundled syntax additions recognize native annotation types (`set`,
`tuple`, `object`, `complex`, fixed-width integers/floats) and imaginary
literals such as `5j`.

## Key Bindings

Default key bindings when ALS is attached:

| Key | Action |
|-----|--------|
| `gd` | Go to definition |
| `gD` | Go to declaration |
| `K` | Show hover (includes ownership info) |
| `gi` | Go to implementation |
| `gr` | Find references |
| `<C-k>` | Signature help |
| `<space>rn` | Rename symbol |
| `<space>ca` | Code actions |
| `<space>f` | Format document |

### AdeshLang-specific bindings
| Key | Action |
|-----|--------|
| `<space>vb` | Show borrow information |
| `<space>vo` | Show ownership information |
| `<space>vu` | Toggle unsafe block highlighting |

## Configuration

Configure ALS settings in your setup:

```lua
require('als').setup({
  settings = {
    adesh = {
      format = {
        indentSize = 4,
        useTabs = false,
      },
      borrowChecker = {
        enabled = true,
        showOwnershipHints = true,
        highlightBorrows = true,
      },
      unsafe = {
        warnOnUsage = true,
      },
      inlayHints = {
        enabled = true,
        showOwnership = true,
        showBorrowState = false,
      },
    },
  },
})
```

## Highlight Groups

ALS defines custom highlight groups for ownership visualization:

| Group | Description | Default Style |
|-------|-------------|---------------|
| `AdeshOwned` | Owned variables | Green, bold |
| `AdeshBorrowed` | Borrowed references | Blue, italic |
| `AdeshMoved` | Moved (consumed) values | Gray, strikethrough |
| `AdeshDropped` | Dropped values | Dark gray, strikethrough |
| `AdeshUnsafe` | Unsafe block background | Subtle red tint |
| `AdeshUnsafeKeyword` | `unsafe` keyword | Red, bold |
| `AdeshRegion` | Region names | Purple, italic |
| `AdeshBorrowOp` | `&` operator | Cyan, bold |
| `AdeshBorrowMutOp` | `&mut` operator | Orange, bold |

Customize in your config:
```lua
vim.api.nvim_set_hl(0, 'AdeshBorrowed', { fg = '#61AFEF', italic = true })
```

## Treesitter Integration

If you're using [nvim-treesitter](https://github.com/nvim-treesitter/nvim-treesitter) and there's a Adesh parser available, ALS provides highlight queries in `M.treesitter_queries`.

## Troubleshooting

### ALS not starting
1. Verify ALS is in your PATH: `which als`
2. Check `:LspInfo` in Neovim
3. View logs: `:LspLog`

### No completions
1. Ensure `omnifunc` is set: `:set omnifunc?`
2. Check if nvim-cmp is configured for LSP

### No diagnostics
1. Check `:LspInfo` for active clients
2. Verify file is recognized as `adesh`: `:set ft?`

## Integration with Other Plugins

### nvim-cmp
```lua
require('cmp').setup({
  sources = {
    { name = 'nvim_lsp' },
  },
})
```

### trouble.nvim
Works out of the box for diagnostic display.

### lspsaga.nvim
Compatible with ALS for enhanced UI.

## Contributing

ALS is part of the AdeshLang project. Contributions welcome at:
https://github.com/ajaytainwala-dev/mylang

## License

MIT License - See LICENSE in the repository root.
