-- Run from the repository root:
-- nvim --headless -u NONE -l als/tests/nvim_smoke.lua
local root = vim.fn.getcwd()
local module = dofile(root .. '/als-neovim/als.lua')
module.setup_highlights()
module.setup_syntax()
vim.bo.filetype = 'adl'
assert(vim.v.errmsg == '', vim.v.errmsg)
assert(module.als_config.settings.adesh.completion.autoImport)
assert(module.als_config.settings.adesh.inlayHints.showParameterNames)
local filename = vim.fn.tempname() .. '.adesh'
vim.fn.writefile({ 'class User {', '    name: string;', '}', 'let u: User = User();', 'u.na' }, filename)
vim.cmd.edit(vim.fn.fnameescape(filename))
vim.bo.filetype = 'adesh'
assert(vim.v.errmsg == '', vim.v.errmsg)
local bufnr = vim.api.nvim_get_current_buf()
local binary = root .. '/target/debug/als' .. (vim.fn.has('win32') == 1 and '.exe' or '')
local id = vim.lsp.start({
  name = 'als-smoke',
  cmd = { binary },
  root_dir = vim.fn.fnamemodify(filename, ':h'),
  init_options = { diagnostics = { enabled = false }, codeLens = { enabled = false } },
})
local function run()
  assert(id, 'LSP client started')
  local client = vim.lsp.get_client_by_id(id)
  assert(vim.wait(15000, function() return client.initialized end, 20), 'initialize completed')
  assert(client.server_capabilities.typeDefinitionProvider)
  local function request(method, params)
    local result, err = client:request_sync(method, params, 10000, bufnr)
    assert(result and not result.err, vim.inspect(err or result))
    return result.result
  end
  local uri = vim.uri_from_bufnr(bufnr)
  local items = request('textDocument/completion', {
    textDocument = { uri = uri }, position = { line = 4, character = 4 },
  })
  assert(vim.iter(items):any(function(item) return item.label == 'name' end), vim.inspect(items))
  local hover = request('textDocument/hover', {
    textDocument = { uri = uri }, position = { line = 3, character = 4 },
  })
  assert(hover.contents.value:find('User', 1, true), vim.inspect(hover))
  local ty = request('textDocument/typeDefinition', {
    textDocument = { uri = uri }, position = { line = 3, character = 4 },
  })
  assert(ty.uri == uri and ty.range.start.line == 0, vim.inspect(ty))
  print('PASS: Neovim initialize, completion, hover, type definition')
end
local ok, err = pcall(run)
if id then vim.lsp.stop_client(id, true) end
vim.fn.delete(filename)
if not ok then error(err) end
vim.cmd.quitall({ bang = true })
