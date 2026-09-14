-- ALS (Adesh Language Server) NeoVim Configuration
-- Production-grade LSP configuration for AdeshLang with full IDE features:
-- - Type-aware auto-completion (IntelliSense)
-- - Inline diagnostics (errors, warnings, hints)
-- - Hover information, go-to-definition, find references
-- - Semantic syntax highlighting
-- - Inlay hints for types and ownership
-- - Signature help
-- - Code formatting
--
-- Installation:
-- 1. Build ALS: `cargo build --release` in the als/ directory
-- 2. Copy this file to your NeoVim config or require it from init.lua
-- 3. Ensure `als` binary is in PATH or set `cmd` below to the absolute path

local M = {}

-- ALS configuration
M.als_config = {
  cmd = { 'als' },
  filetypes = { 'adesh', 'adl' },
  root_dir = function(fname)
    local util = require('lspconfig.util')
    return util.find_git_ancestor(fname)
      or util.root_pattern('Cargo.toml', 'package.json', '.adesh')(fname)
      or vim.fn.getcwd()
  end,
  single_file_support = true,
  settings = {
    adesh = {
      format = {
        indentSize = 4,
        useTabs = false,
        enable = true,
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
      completion = {
        enabled = true,
        includeKeywords = true,
        includeBuiltins = true,
      },
      diagnostics = {
        enabled = true,
        refreshOnSave = true,
      },
      typeAssistance = {
        enabled = true,
        showInferredTypes = true,
        strictMode = false,
      },
    },
  },
  capabilities = {
    textDocument = {
      completion = {
        completionItem = {
          snippetSupport = true,
          commitCharactersSupport = true,
          documentationFormat = { 'markdown', 'plaintext' },
          deprecatedSupport = true,
          preselectSupport = true,
          tagSupport = { valueSet = { 1 } },
          insertReplaceSupport = true,
          resolveSupport = { properties = { 'documentation', 'detail', 'additionalTextEdits' } },
        },
        contextSupport = true,
      },
      hover = {
        contentFormat = { 'markdown', 'plaintext' },
        dynamicRegistration = false,
      },
      signatureHelp = {
        signatureInformation = {
          documentationFormat = { 'markdown', 'plaintext' },
          parameterInformation = { labelOffsetSupport = true },
          activeParameterSupport = true,
        },
      },
      semanticTokens = {
        dynamicRegistration = false,
        tokenTypes = {
          'borrowedVariable', 'ownedVariable', 'movedVariable', 'droppedVariable',
          'unsafeFunction', 'region', 'sharedVariable', 'strongReference', 'weakReference',
        },
        tokenModifiers = {
          'borrowed', 'owned', 'moved', 'mutable', 'unsafe', 'noalias', 'shared', 'weak',
        },
        formats = { 'relative' },
        requests = { range = true, full = true },
        overlappingTokenSupport = true,
        multilineTokenSupport = true,
      },
      inlayHint = {
        dynamicRegistration = false,
        resolveSupport = { properties = { 'tooltip', 'label' } },
      },
    },
  },
}

-- Ownership/borrowing highlight groups
M.setup_highlights = function()
  -- Variable state highlights
  vim.api.nvim_set_hl(0, 'AdeshOwned', { fg = '#4EC9B0', bold = true })
  vim.api.nvim_set_hl(0, 'AdeshBorrowed', { fg = '#569CD6', italic = true })
  vim.api.nvim_set_hl(0, 'AdeshMoved', { fg = '#808080', strikethrough = true })
  vim.api.nvim_set_hl(0, 'AdeshDropped', { fg = '#6A6A6A', strikethrough = true })

  -- Unsafe block highlight
  vim.api.nvim_set_hl(0, 'AdeshUnsafe', { bg = '#2D1515' })
  vim.api.nvim_set_hl(0, 'AdeshUnsafeKeyword', { fg = '#FF6B6B', bold = true })

  -- Region highlight
  vim.api.nvim_set_hl(0, 'AdeshRegion', { fg = '#C586C0', italic = true })

  -- Borrow operators
  vim.api.nvim_set_hl(0, 'AdeshBorrowOp', { fg = '#4FC1FF', bold = true })
  vim.api.nvim_set_hl(0, 'AdeshBorrowMutOp', { fg = '#FF8C00', bold = true })

  -- Diagnostic highlights
  vim.api.nvim_set_hl(0, 'AdeshError', { fg = '#F44747', bold = true })
  vim.api.nvim_set_hl(0, 'AdeshWarning', { fg = '#FF8C00' })
  vim.api.nvim_set_hl(0, 'AdeshHint', { fg = '#4EC9B0' })
  vim.api.nvim_set_hl(0, 'AdeshInfo', { fg = '#569CD6' })
end

-- Keywords for ownership and memory
M.ownership_keywords = {
  'owned', 'borrowed', 'shared', 'unique', 'noalias', 'move', 'copy',
}

M.arc_keywords = {
  'share', 'strong', 'weak',
}

M.memory_keywords = {
  'alloc', 'free', 'drop', 'heap', 'stack', 'region', 'arena',
  'sizeof', 'alignof', 'offsetof',
}

M.control_keywords = {
  'unsafe', 'panic', 'unreachable', 'defer',
}

-- Setup function
function M.setup(opts)
  opts = opts or {}

  local lspconfig = require('lspconfig')
  local configs = require('lspconfig.configs')

  -- Register ALS if not already registered
  if not configs.als then
    configs.als = {
      default_config = M.als_config,
    }
  end

  -- Setup highlights
  M.setup_highlights()

  -- Default on attach function
  local on_attach = opts.on_attach or function(client, bufnr)
    -- Enable completion triggered by <c-x><c-o>
    vim.api.nvim_buf_set_option(bufnr, 'omnifunc', 'v:lua.vim.lsp.omnifunc')

    -- Key mappings
    local bufopts = { noremap = true, silent = true, buffer = bufnr }

    -- Standard LSP mappings
    vim.keymap.set('n', 'gD', vim.lsp.buf.declaration, bufopts)
    vim.keymap.set('n', 'gd', vim.lsp.buf.definition, bufopts)
    vim.keymap.set('n', 'gy', vim.lsp.buf.type_definition, bufopts)
    vim.keymap.set('n', 'gi', vim.lsp.buf.implementation, bufopts)
    vim.keymap.set('n', 'K', vim.lsp.buf.hover, bufopts)
    vim.keymap.set('n', '<C-k>', vim.lsp.buf.signature_help, bufopts)
    vim.keymap.set('n', '<space>wa', vim.lsp.buf.add_workspace_folder, bufopts)
    vim.keymap.set('n', '<space>wr', vim.lsp.buf.remove_workspace_folder, bufopts)
    vim.keymap.set('n', '<space>wl', function()
      print(vim.inspect(vim.lsp.buf.list_workspace_folders()))
    end, bufopts)
    vim.keymap.set('n', '<space>D', vim.lsp.buf.type_definition, bufopts)
    vim.keymap.set('n', '<space>rn', vim.lsp.buf.rename, bufopts)
    vim.keymap.set('n', '<space>ca', vim.lsp.buf.code_action, bufopts)
    vim.keymap.set('n', 'gr', vim.lsp.buf.references, bufopts)
    vim.keymap.set('n', '<space>f', function()
      vim.lsp.buf.format({ async = true })
    end, bufopts)

    -- AdeshLang-specific mappings
    vim.keymap.set('n', '<space>vb', M.show_borrow_info, bufopts)
    vim.keymap.set('n', '<space>vo', M.show_ownership_info, bufopts)
    vim.keymap.set('n', '<space>vu', M.toggle_unsafe_highlight, bufopts)

    -- Show diagnostics on hover
    vim.api.nvim_create_autocmd('CursorHold', {
      buffer = bufnr,
      callback = function()
        local diagnostics_opts = {
          focusable = false,
          close_events = { 'BufLeave', 'CursorMoved', 'InsertEnter', 'FocusLost' },
          border = 'rounded',
          source = 'always',
          prefix = ' ',
          scope = 'cursor',
        }
        vim.diagnostic.open_float(nil, diagnostics_opts)
      end,
    })

    -- Setup inlay hints if available (Neovim 0.10+)
    if client.server_capabilities.inlayHintProvider then
      pcall(function()
        vim.lsp.inlay_hint.enable(bufnr, true)
      end)
    end

    -- Enable document highlighting if supported
    if client.server_capabilities.documentHighlightProvider then
      vim.api.nvim_create_autocmd('CursorHold', {
        buffer = bufnr,
        callback = function()
          vim.lsp.buf.document_highlight()
        end,
      })
      vim.api.nvim_create_autocmd('CursorMoved', {
        buffer = bufnr,
        callback = function()
          vim.lsp.buf.clear_references()
        end,
      })
    end
  end

  -- Get capabilities (with nvim-cmp if available)
  local capabilities = vim.lsp.protocol.make_client_capabilities()
  local has_cmp, cmp_nvim_lsp = pcall(require, 'cmp_nvim_lsp')
  if has_cmp then
    capabilities = cmp_nvim_lsp.default_capabilities(capabilities)
  end

  -- Merge with our capabilities
  capabilities = vim.tbl_deep_extend('force', capabilities, M.als_config.capabilities or {})

  -- Setup ALS
  lspconfig.als.setup({
    cmd = opts.cmd or M.als_config.cmd,
    filetypes = M.als_config.filetypes,
    root_dir = M.als_config.root_dir,
    single_file_support = M.als_config.single_file_support,
    on_attach = on_attach,
    capabilities = capabilities,
    settings = opts.settings or M.als_config.settings,
  })

  -- Setup filetype detection
  vim.filetype.add({
    extension = {
      adesh = 'adesh',
      vy = 'adesh',
      adl = 'adl',
    },
    filename = {
      ['adesh.adl'] = 'adl',
      ['adesh.lock.adl'] = 'adl',
    },
  })

  -- Setup diagnostic configuration
  vim.diagnostic.config({
    virtual_text = {
      prefix = '●',
      spacing = 4,
    },
    signs = true,
    underline = true,
    update_in_insert = true,
    severity_sort = true,
    float = {
      border = 'rounded',
      source = 'always',
      header = '',
      prefix = '',
    },
  })

  -- Setup syntax highlighting additions
  M.setup_syntax()
end

-- Show borrow information for current symbol
function M.show_borrow_info()
  local params = vim.lsp.util.make_position_params()
  vim.lsp.buf_request(0, 'textDocument/hover', params, function(err, result, ctx, config)
    if result and result.contents then
      local content = result.contents
      if type(content) == 'table' then
        content = content.value or vim.inspect(content)
      end
      vim.notify('Borrow Info:\n' .. content, vim.log.levels.INFO)
    else
      vim.notify('No borrow information available', vim.log.levels.WARN)
    end
  end)
end

-- Show ownership information
function M.show_ownership_info()
  local word = vim.fn.expand('<cword>')
  vim.notify('Ownership info for: ' .. word .. '\n(Feature requires ALS with ownership tracking)', vim.log.levels.INFO)
end

-- Toggle unsafe block highlighting
M._unsafe_highlight_enabled = true
function M.toggle_unsafe_highlight()
  M._unsafe_highlight_enabled = not M._unsafe_highlight_enabled
  if M._unsafe_highlight_enabled then
    M.setup_highlights()
    vim.notify('Unsafe highlighting enabled', vim.log.levels.INFO)
  else
    vim.api.nvim_set_hl(0, 'AdeshUnsafe', {})
    vim.notify('Unsafe highlighting disabled', vim.log.levels.INFO)
  end
end

-- Setup additional syntax patterns
function M.setup_syntax()
  vim.api.nvim_create_autocmd('FileType', {
    pattern = 'adesh',
    callback = function()
      -- Ownership keywords
      -- Ownership keywords
      vim.cmd([[syn keyword adeshOwnership owned borrowed shared unique noalias move copy]])
      vim.cmd([[hi def link adeshOwnership AdeshOwned]])

      -- ARC keywords
      vim.cmd([[syn keyword adeshArc share strong weak]])
      vim.cmd([[hi def link adeshArc Keyword]])

      -- Memory keywords
      vim.cmd([[syn keyword adeshMemory alloc free drop heap stack region arena]])
      vim.cmd([[hi def link adeshMemory Function]])

      -- Unsafe keyword
      vim.cmd([[syn keyword adeshUnsafeKeyword unsafe]])
      vim.cmd([[hi def link adeshUnsafeKeyword AdeshUnsafeKeyword]])

      -- Borrow operators
      vim.cmd([[syn match adeshBorrowOp /&\ze[^&=]/]])
      vim.cmd([[syn match adeshBorrowMutOp /&mut\>/]])
      vim.cmd([[hi def link adeshBorrowOp AdeshBorrowOp]])
      vim.cmd([[hi def link adeshBorrowMutOp AdeshBorrowMutOp]])

      -- Pointer types
      vim.cmd([[syn match adeshPointerType /\*[A-Za-z_][A-Za-z0-9_]*/]])
      vim.cmd([[hi def link adeshPointerType Type]])

      -- Control flow
      vim.cmd([[syn keyword adeshControl panic unreachable defer do]])
      vim.cmd([[hi def link adeshControl Keyword]])
      vim.cmd([[syn keyword adeshKeyword get set on extend sealed]])
      vim.cmd([[hi def link adeshKeyword Keyword]])

      -- Native datatype annotations and imaginary literals
      vim.cmd([[syn keyword adeshType int i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 bool string char tuple set object map complex]])
      vim.cmd([[hi def link adeshType Type]])
      vim.cmd([[syn match adeshComplexLiteral /\<[0-9][0-9_]*\%(\.[0-9][0-9_]*\)\?j\>/]])
      vim.cmd([[hi def link adeshComplexLiteral Number]])
      vim.cmd([[syn match adeshTypedLiteral /\<[0-9][0-9_]*\%(u8\|u16\|u32\|u64\|u128\|i8\|i16\|i32\|i64\|i128\|f32\|f64\)\>/]])
      vim.cmd([[hi def link adeshTypedLiteral Number]])
    end,
  })

  -- ADL (Adesh Package Manifest) syntax highlighting
  vim.api.nvim_create_autocmd('FileType', {
    pattern = 'adl',
    callback = function()
      -- Comments
      vim.cmd([[syn match adlCommentLine /\/\/.*$/]])
      vim.cmd([[syn match adlCommentHash /#.*$/]])
      vim.cmd([[syn region adlCommentBlock start=/\/\*/ end=\*\//])
      vim.cmd([[hi def link adlCommentLine Comment]])
      vim.cmd([[hi def link adlCommentHash Comment]])
      vim.cmd([[hi def link adlCommentBlock Comment]])

      -- Section headers [section]
      vim.cmd([[syn match adlSection /^\s*\[\h\w*\(-\w*\)*\]/]])
      vim.cmd([[hi def link adlSection PreProc]])

      -- Lock block keyword
      vim.cmd([[syn keyword adlLock lock]])
      vim.cmd([[hi def link adlLock Keyword]])

      -- Confidential block keyword
      vim.cmd([[syn keyword adlConfidential confidential]])
      vim.cmd([[hi def link adlConfidential Keyword]])

      -- Import / let / const / if / else keywords
      vim.cmd([[syn keyword adlImport import]])
      vim.cmd([[syn keyword adlDecl let const]])
      vim.cmd([[syn keyword adlCond if else]])
      vim.cmd([[hi def link adlImport Include]])
      vim.cmd([[hi def link adlDecl Statement]])
      vim.cmd([[hi def link adlCond Conditional]])

      -- Typed value prefixes
      vim.cmd([[syn keyword adlTypedVal path url duration version]])
      vim.cmd([[hi def link adlTypedVal Keyword]])

      -- Booleans and null
      vim.cmd([[syn keyword adlBool true false]])
      vim.cmd([[syn keyword adlNull null]])
      vim.cmd([[hi def link adlBool Boolean]])
      vim.cmd([[hi def link adlNull Constant]])

      -- Version requirements (^1.0.0, ~1.0.0, etc.)
      vim.cmd([[syn match adlVersion /\(\^\|~\|>=\|<=\|>\|<\|=\)\?\s*\d\+\.\d\+\.\d\+\([-.+][a-zA-Z0-9.+-]\+\)\?/]])
      vim.cmd([[hi def link adlVersion Special]])

      -- Strings
      vim.cmd([[syn region adlString start=/"/ skip=/\\"/ end=/"/]])
      vim.cmd([[hi def link adlString String]])

      -- Numbers
      vim.cmd([[syn match adlNumber /\<-\?\d\+\(\.\d\+\)\?\([eE][+-]\?\d\+\)\?\>/]])
      vim.cmd([[hi def link adlNumber Number]])

      -- Property keys (key =)
      vim.cmd([[syn match adlKey /^\s*\h\w*\(-\w*\)*\ze\s*=/]])
      vim.cmd([[hi def link adlKey Identifier]])

      -- Known section names as keywords
      vim.cmd([[syn keyword adlSectionName project compiler dependencies scripts workspace contained]])
      vim.cmd([[hi def link adlSectionName Type]])

      -- Punctuation
      vim.cmd([[syn match adlOperator /=/]])
      vim.cmd([[syn match adlPunct /[,{}\[\]]/]])
      vim.cmd([[hi def link adlOperator Operator]])
      vim.cmd([[hi def link adlPunct Delimiter]])
    end,
  })
end

-- CFG Error codes for diagnostics
M.borrow_errors = {
  E0501 = 'Conflicting borrow states across branches',
  E0502 = 'Cannot borrow as mutable (already borrowed)',
  E0503 = 'Use of moved value',
  E0504 = 'Cannot move out of borrowed content',
  E0505 = 'Value may have been moved in branch',
  E0506 = 'Cannot free while borrowed',
  E0507 = 'Use after free',
  E0508 = 'Double free',
  E0509 = 'Drop while borrowed',
  E0510 = 'Different exclusive borrows across branches',
  E0511 = 'Value freed in some branches',
  E0512 = 'Value moved inside loop',
  E0513 = 'Cannot mutate collection while iterating',
  E0514 = 'Borrow escapes loop scope',
}

-- Custom diagnostic signs for borrow errors
function M.setup_diagnostic_signs()
  local signs = {
    { name = 'DiagnosticSignError', text = '✗' },
    { name = 'DiagnosticSignWarn', text = '⚠' },
    { name = 'DiagnosticSignHint', text = '➤' },
    { name = 'DiagnosticSignInfo', text = 'ℹ' },
  }

  for _, sign in ipairs(signs) do
    vim.fn.sign_define(sign.name, { text = sign.text, texthl = sign.name })
  end
end

-- Treesitter queries for Adesh (if using treesitter)
M.treesitter_queries = {
  highlights = [[
    ; Ownership keywords
    ["owned" "borrowed" "shared" "unique" "noalias" "move" "copy"] @keyword.ownership

    ; ARC keywords
    ["share" "strong" "weak"] @keyword.arc

    ; Memory keywords
    ["alloc" "free" "drop" "heap" "stack" "region" "arena"] @function.builtin

    ; Unsafe
    "unsafe" @keyword.unsafe

    ; Borrow operators
    "&" @operator.borrow
    "&mut" @operator.borrow.mut

    ; Control flow
    ["panic" "unreachable" "defer"] @keyword.control
  ]],
}

return M
