# Adesh Editor — Usage Guide

A cross-platform TUI code editor for AdeshLang, built with Rust, Ratatui, and Crossterm.

---

## Table of Contents

1. [Getting Started](#getting-started)
2. [Modes](#modes)
3. [Menu Tabs](#menu-tabs)
4. [Keyboard Shortcuts](#keyboard-shortcuts)
5. [Command Line Mode](#command-line-mode)
6. [Integrated Terminal](#integrated-terminal)
7. [Running Code](#running-code)
8. [Search & Grep](#search--grep)
9. [Editing Features](#editing-features)
10. [Themes](#themes)
11. [Configuration](#configuration)
12. [Mouse Support](#mouse-support)
13. [Syntax Highlighting](#syntax-highlighting)

---

## Getting Started

### Launch the editor

```bash
# Open with no file (empty buffer)
adesh-editor

# Open a specific file
adesh-editor path/to/file.adesh

# Open a directory (sets workspace root)
adesh-editor path/to/project/
```

### Layout

```
┌─────────────────────────────────────────────────────┐
│ File  Edit  View  Run  Info     F1:Info  Alt:Menu    │ ← Menu bar
├─────────────────────────────────────────────────────┤
│ Adesh Editor │ [ file1.ad × ] [ file2.ad ● × ] ▶ Run │ ← Tab bar + Run button
├──────────┬──────────────────────────────────────────┤
│ PROJECT  │  1  fn main() {                          │ ← Explorer + Editor
│ 📁 src   │  2      print("Hello");                   │
│ 📄 main  │  3  }                                    │
│          │                                          │
├──────────┴──────────────────────────────────────────┤
│ Output Panel (Ctrl+` to toggle)                      │ ← Output panel
├─────────────────────────────────────────────────────┤
│ NORMAL │ Backend: Interpreter │ Ln 3, Col 1 │ ...   │ ← Status bar
└─────────────────────────────────────────────────────┘
```

---

## Modes

The editor uses modal editing (similar to Vim).

| Mode    | Description                          | How to enter          |
|---------|--------------------------------------|-----------------------|
| Normal  | Navigation and commands (default)    | Press `Esc` from any mode |
| Insert  | Type text into the buffer            | Press `i`, `a`, `o`, `O` in Normal |
| Visual  | Select text with cursor movement     | Press `v` in Normal   |
| Command | Enter `:` commands at the bottom bar | Press `:` in Normal   |

---

## Menu Tabs

Click a menu tab with the mouse or press `Alt+<key>` to open its dropdown.

### File (Alt+F)

| Item         | Action                              |
|--------------|-------------------------------------|
| New File     | Create a new file in the workspace   |
| Open File    | Open a file by path                  |
| Save         | Save the current buffer             |
| Save As     | Save to a new path                  |
| Save All     | Save all modified buffers            |
| Close Buffer | Close the current buffer             |
| Quit         | Exit the editor                      |

### Edit (Alt+E)

| Item             | Action                          |
|------------------|---------------------------------|
| Undo             | Undo last change                |
| Redo             | Redo last undone change         |
| Copy             | Copy selection (or yank line)   |
| Cut              | Cut selection (or cut line)     |
| Paste            | Paste from clipboard            |
| Duplicate Line   | Duplicate the current line      |
| Comment Toggle   | Toggle `//` line comment        |
| Format Document  | Auto-indent the document        |
| Find             | Open find modal                 |
| Replace          | Open find & replace modal       |

### View (Alt+V)

| Item                  | Action                          |
|-----------------------|---------------------------------|
| Toggle File Explorer  | Show/hide the file tree         |
| Toggle Output Panel   | Show/hide the output panel      |
| Toggle Line Numbers   | Cycle: absolute → relative → off |
| Toggle Word Wrap      | Enable/disable word wrapping    |
| Toggle Line Highlight | Show/hide current line highlight |
| Switch Theme          | Cycle through available themes  |
| Change Backend        | Select execution backend        |

### Run (Alt+R)

| Item                  | Action                          |
|-----------------------|---------------------------------|
| Run (F5)              | Run current file with active backend |
| Run with Backend      | Choose backend then run         |
| Stop (F6)             | Stop the running program        |

### Info (Alt+I)

| Item           | Action                          |
|----------------|---------------------------------|
| Show Info Page | Display the in-editor help page  |

---

## Keyboard Shortcuts

### Global Hotkeys (work in all modes)

| Key          | Action                          |
|--------------|---------------------------------|
| `Ctrl+Q`     | Quit editor                     |
| `Ctrl+S`     | Save current file               |
| `Ctrl+O`     | Open file                       |
| `Ctrl+P`     | Open command palette            |
| `Ctrl+F`     | Find in current file            |
| `Ctrl+Shift+F`| Grep across all open files     |
| `Ctrl+H`     | Find & Replace                  |
| `Ctrl+Z`     | Undo                            |
| `Ctrl+Y`     | Redo                            |
| `Ctrl+E`     | Toggle file explorer focus     |
| `Ctrl+`` ` `` | Toggle output panel            |
| `Ctrl+J`     | Toggle integrated terminal     |
| `Ctrl+G`     | Go to line                      |
| `Ctrl+D`     | Duplicate current line          |
| `Ctrl+/`     | Toggle line comment             |
| `Ctrl+A`     | Increment number at cursor (Normal) |
| `Ctrl+X`     | Decrement number at cursor (Normal) |
| `Alt+Up`     | Move current line up            |
| `Alt+Down`   | Move current line down          |
| `F1`         | Show info/help page             |
| `F2`         | Save current file               |
| `F3`         | Toggle file explorer            |
| `F4`         | Cycle line number mode          |
| `F5`         | Run program                     |
| `Ctrl+F5`    | Select backend & run            |
| `F6`         | Stop execution                  |

### Normal Mode — Navigation

| Key       | Action                    |
|-----------|---------------------------|
| `h` / `←` | Move left                 |
| `j` / `↓` | Move down                 |
| `k` / `↑` | Move up                   |
| `l` / `→` | Move right                |
| `w`       | Next word                 |
| `b`       | Previous word             |
| `0` / `Home` | Start of line          |
| `$` / `End`  | End of line            |
| `g`       | Top of file               |
| `G`       | Bottom of file            |
| `PageUp`  | Scroll up 15 lines        |
| `PageDn`  | Scroll down 15 lines      |
| `%`       | Jump to matching bracket  |
| `Tab`     | Next buffer               |
| `Shift+Tab` | Previous buffer         |

### Normal Mode — Editing

| Key     | Action                          |
|---------|---------------------------------|
| `i`     | Insert at cursor                |
| `I`     | Insert at start of line         |
| `a`     | Append after cursor             |
| `A`     | Append at end of line           |
| `o`     | Open new line below             |
| `O`     | Open new line above             |
| `v`     | Enter visual mode               |
| `u`     | Undo                            |
| `x`     | Delete character                |
| `dd`    | Delete (cut) line               |
| `dw`    | Delete word                     |
| `cw`    | Change word (delete + insert)   |
| `cc`    | Change line (clear + insert)   |
| `yy`    | Yank (copy) line                |
| `yw`    | Yank word                       |
| `p`     | Paste after cursor              |
| `P`     | Paste before cursor             |
| `J`     | Join lines                      |
| `r<c>`  | Replace character with `<c>`    |
| `~`     | Toggle case of character        |
| `zz`    | Scroll cursor to center         |
| `zt`    | Scroll cursor to top            |
| `zb`    | Scroll cursor to bottom         |
| `:`     | Enter command line mode         |

### Insert Mode

| Key        | Action                    |
|------------|---------------------------|
| `Esc`      | Back to Normal mode       |
| `Enter`    | New line (auto-indent)    |
| `Backspace`| Delete backward           |
| `Delete`   | Delete forward            |
| `Tab`      | Indent                    |
| `Shift+Tab`| Unindent                  |
| `Home`     | Start of line             |
| `End`      | End of line               |

Auto-close brackets is enabled by default. Typing `(` inserts `()` with the cursor between them.

### Visual Mode

| Key       | Action                    |
|-----------|---------------------------|
| `Esc`     | Exit visual mode          |
| `h/j/k/l` | Move selection            |
| `w/b`     | Move by word              |
| `0/$`     | Start/end of line         |
| `d` / `x` | Cut selection             |
| `y`       | Copy selection            |
| `c`       | Change selection (cut + insert) |

---

## Command Line Mode

Press `:` in Normal mode to open the command bar at the bottom of the screen. Type a command and press `Enter` to execute. Press `Esc` to cancel.

### Commands

| Command       | Action                          |
|---------------|---------------------------------|
| `:w`          | Save current file               |
| `:wa`         | Save all files                  |
| `:q`          | Quit (warns if unsaved)         |
| `:q!`         | Force quit (discard unsaved)   |
| `:wq` or `:x` | Save and quit                  |
| `:e <path>`   | Open file                       |
| `:new`        | New file prompt                 |
| `:N`          | Go to line N (e.g., `:42`)     |
| `:goto N`     | Go to line N                    |
| `:theme <name>` | Switch theme                 |
| `:theme`      | Open theme selector            |
| `:set <opt>=<val>` | Set an option             |
| `:backend <name>` | Change execution backend   |
| `:explorer`   | Toggle file explorer            |
| `:output`     | Toggle output panel             |
| `:run`        | Run current program             |
| `:stop`       | Stop running program            |
| `:format`     | Format document                 |
| `:help`       | Show info page                  |

### Set Options

Use `:set <option>=<value>` to change settings live.

| Option             | Values                          |
|--------------------|---------------------------------|
| `theme`            | `Adesh Dark`, `Adesh Light`, `Gruvbox`, `Monokai`, `Dracula`, `One Dark` |
| `tab_width`        | Number (default: 4)             |
| `line_numbers`     | `absolute`, `relative`, `none`  |
| `word_wrap`        | `true` / `false`                |
| `line_highlight`   | `true` / `false`                |
| `auto_close`       | `true` / `false` (auto-close brackets) |
| `trailing_ws`      | `true` / `false` (show trailing whitespace) |

---

## Integrated Terminal

Press `Ctrl+J` to open the integrated terminal panel at the bottom of the screen. Click the terminal panel to focus it, or press `Esc` to defocus.

### Terminal Keys

| Key       | Action                          |
|-----------|---------------------------------|
| `Ctrl+J`  | Open / close terminal           |
| `Esc`     | Defocus terminal (keeps it open) |
| `Enter`   | Execute the typed command       |
| `Up/Down` | Browse command history          |
| `Ctrl+C`  | Stop running command            |
| `Ctrl+L`  | Clear terminal                  |

### Built-in Commands

| Command | Action              |
|---------|---------------------|
| `clear` / `cls` | Clear terminal   |
| `help`  | Show built-in help  |
| `exit`  | Hint to use Ctrl+J  |

Any other command is executed via the system shell (`cmd /C` on Windows, `sh -c` on Unix).

---

## Running Code

### Run Button

A **▶ Run** button is displayed at the right end of the tab bar. Click it with the mouse to run the current file, or press `F5`.

The button turns yellow while a program is running.

### Run Options

| Key        | Action                          |
|------------|---------------------------------|
| `F5`       | Run with current backend        |
| `Ctrl+F5`  | Select backend, then run        |
| `F6`       | Stop the running program        |
| `Ctrl+`` ` `` | Toggle output panel          |

### Backends

The editor supports multiple AdeshLang execution backends:

| Backend      | Flag            |
|--------------|-----------------|
| Interpreter  | `--interpreter` |
| JIT          | `--jit`         |
| Native JIT   | `--native-jit`  |
| Bytecode VM  | `--bytecode`    |
| Adaptive JIT | `--adaptive`    |
| Tiered JIT   | `--tiered`      |
| Mixed        | `--mixed`       |
| Safe         | `--safe`        |
| AOT          | `--aot`         |
| WASM         | `--wasm`        |
| GPU          | `--gpu`         |

Switch backends via the View menu, `:backend <name>`, or `Ctrl+F5`.

### Output Panel

Program output (stdout and stderr) is displayed in the output panel. Toggle it with `Ctrl+`` ` `` or the View menu. The panel shows:
- Compilation/execution status
- Program output lines
- Error messages (prefixed with `[err]`)
- Exit code and execution time

---

## Search & Grep

### Find in Current File (Ctrl+F)

1. Press `Ctrl+F` to open the find modal
2. Type your search query
3. Press `Enter` to jump to the next match
4. Press `Esc` to cancel

Search matches are highlighted in the editor. The match count is shown in the modal.

### Find & Replace (Ctrl+H)

1. Press `Ctrl+H` to open the find & replace modal
2. Type the find query
3. Press `Tab` to switch to the replace field
4. Type the replacement text
5. Press `Enter` to replace the current match
6. Press `Esc` to cancel

### Grep Across All Open Files (Ctrl+Shift+F)

1. Press `Ctrl+Shift+F` to open the grep panel
2. Type your search query — results update live as you type
3. Use `Up`/`Down` to navigate results
4. Press `Enter` to jump to the result (switches buffer and goes to the line)
5. Press `Esc` to close

Results show `filename:line_number: content` for each match across all open buffers.

---

## Editing Features

### Copy / Cut / Paste

| Key     | Action                          |
|---------|---------------------------------|
| `yy`    | Copy (yank) current line        |
| `yw`    | Copy word at cursor             |
| `dd`    | Cut (delete) current line       |
| `dw`    | Delete word                     |
| `p`     | Paste after cursor              |
| `P`     | Paste before cursor             |
| `v` then `y` | Copy selection            |
| `v` then `d` | Cut selection             |
| `v` then `c` | Change selection          |

### Line Operations

| Key          | Action                    |
|--------------|---------------------------|
| `Ctrl+D`     | Duplicate current line    |
| `Alt+Up`     | Move line up              |
| `Alt+Down`   | Move line down            |
| `J`          | Join with next line        |

### Comment Toggle

Press `Ctrl+/` to toggle line comments. The comment style is auto-detected from the file extension:

| Language                    | Comment |
|-----------------------------|---------|
| Adesh (`.ad`, `.adl`)       | `//`    |
| Rust (`.rs`)                | `//`    |
| C/C++ (`.c`, `.cpp`, `.h`)  | `//`    |
| Python (`.py`)              | `#`     |
| YAML/TOML (`.yaml`, `.toml`)| `#`     |
| SQL (`.sql`)               | `--`    |

### Number Manipulation

| Key     | Action                        |
|---------|-------------------------------|
| `Ctrl+A`| Increment number at/near cursor |
| `Ctrl+X`| Decrement number at/near cursor |

### Character Operations

| Key     | Action                          |
|---------|---------------------------------|
| `r<c>`  | Replace character with `<c>`    |
| `~`     | Toggle case of character        |

### Bracket Matching

Press `%` in Normal mode to jump to the matching bracket. The cursor bracket and its match are highlighted.

### Auto-Close Brackets

When auto-close is enabled (default), typing any of `(`, `[`, `{`, `"`, `'`, `` ` `` inserts the closing pair automatically.

---

## Themes

Six built-in themes are available. Switch via:
- View menu → Switch Theme
- `:theme <name>` command
- `:theme` (opens theme selector)

| Theme       | Style  |
|-------------|--------|
| Adesh Dark  | Dark   |
| Adesh Light | Light  |
| Gruvbox     | Dark   |
| Monokai     | Dark   |
| Dracula     | Dark   |
| One Dark    | Dark   |

---

## Configuration

The editor reads configuration from `~/.config/adesh/editor.toml` (or `%APPDATA%\adesh\editor.toml` on Windows).

### Example Configuration

```toml
theme = "Gruvbox"
tab_width = 4
use_spaces = true
line_numbers = "relative"
default_backend = "Interpreter"
auto_save = false
word_wrap = false
show_line_highlight = true
auto_close_brackets = true
show_trailing_whitespace = true
```

### Options

| Option                  | Type    | Default     | Description                     |
|-------------------------|---------|-------------|---------------------------------|
| `theme`                 | string  | `Adesh Dark`| Theme name                     |
| `tab_width`             | number  | `4`         | Spaces per tab                  |
| `use_spaces`            | bool    | `true`      | Use spaces instead of tabs      |
| `line_numbers`          | string  | `absolute`  | `absolute`, `relative`, or `none` |
| `default_backend`       | string  | `Interpreter`| Default execution backend     |
| `auto_save`             | bool    | `false`     | Auto-save on changes            |
| `word_wrap`             | bool    | `false`     | Enable word wrapping            |
| `show_line_highlight`   | bool    | `true`      | Highlight current line          |
| `auto_close_brackets`   | bool    | `true`      | Auto-close bracket pairs        |
| `show_trailing_whitespace` | bool | `true`     | Show trailing whitespace        |

---

## Mouse Support

| Action              | Effect                          |
|---------------------|---------------------------------|
| Click menu tab      | Open/close dropdown menu        |
| Click dropdown item | Execute the menu item          |
| Click buffer tab    | Switch to that buffer          |
| Click `×` on a tab  | Close the file; modified files ask whether to save |
| Click ▶ Run button  | Run the current program        |
| Click explorer item | Select / open file            |
| Click editor area   | Place cursor at click position |
| Scroll up/down      | Scroll editor or explorer      |
| Click terminal      | Focus the terminal input       |

---

## File Explorer Operations

When the file explorer is focused (click it or press `Ctrl+E`), you can perform file operations:

| Key       | Action                          |
|-----------|---------------------------------|
| `Up` / `k`  | Move selection up            |
| `Down` / `j`| Move selection down          |
| `Enter`   | Open selected file             |
| `Ctrl+C`  | Copy selected file/folder      |
| `Ctrl+X`  | Cut (move) selected file/folder |
| `Ctrl+V`  | Paste copied/cut file into current directory |
| `Delete` / `d` | Delete selected file/folder |
| `Esc`     | Unfocus explorer               |

Copy supports recursive directory copy. Cut performs a move (rename).

Folders are collapsed by default so large workspaces open quickly. Use `Enter`, `Space`, or click a folder to expand it, and repeat the action to collapse it. A scrollbar appears on the right side of the explorer when the tree is taller than the panel.

The editor also shows a scrollbar on the right side of the code area when the file is taller than the visible editor. The thumb indicates the current scroll position.

---

## Cursor

The editor displays a **blinking bar** cursor (`|`) in all typing contexts:

- **Editor**: At the current cursor position (line and column)
- **Terminal**: At the end of the input line
- **Command line** (`:`): At the end of the command text
- **Prompt modals** (Open File, Save As, New File, Go to Line): At the end of the input text

The cursor position is calculated after each render frame, accounting for scroll offsets, line numbers, and explorer width.

---

## Syntax Highlighting

The editor provides comprehensive syntax highlighting for AdeshLang and other languages.

### Highlighted Elements

| Element        | Color (Adesh Dark)  | Examples                        |
|----------------|---------------------|---------------------------------|
| Keywords       | Purple (bold)       | `fn`, `let`, `if`, `struct`     |
| Constants      | Orange              | `true`, `false`, `null`        |
| Types          | Yellow              | `i32`, `String`, `Vec`         |
| Functions      | Blue                | `print`, `map`, `forEach`       |
| Strings        | Green               | `"hello"`, `'world'`          |
| Numbers        | Orange              | `42`, `0xFF`, `3.14`, `0b101`  |
| Comments       | Gray                | `// comment`, `/// doc`        |
| Block comments | Gray                | `/* block */`                  |
| Attributes     | Yellow (bold)       | `@decorator`                   |
| Operators      | Cyan                | `+`, `==`, `=>`, `..`         |
| Punctuation    | Gray                | `;`, `,`, `.`, `()[]{}`       |

### Supported Number Formats

- Decimal: `42`, `1_000`
- Hex: `0xFF`, `0x1A`
- Binary: `0b1010`
- Octal: `0o77`
- Float: `3.14`, `1.5e10`
- With suffix: `42f`, `3.14d`

### Multi-Character Operators

`==`, `!=`, `<=`, `>=`, `&&`, `||`, `->`, `=>`, `..`, `::`, `+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`, `^=`, `<<`, `>>`, `<<=`, `>>=`, `...`, `..=`

---

## Tips

- Press `F1` at any time to see the in-editor help page with all shortcuts.
- Use `Ctrl+P` to quickly access any command via the command palette.
- The status bar shows the current mode, backend, cursor position, word/char count, line number mode, and theme name.
- A blinking cursor is shown in the editor, terminal, command line, and prompt modals.
- The `●` indicator next to a buffer tab means the file has unsaved changes.
- Click the `×` on a tab to close it. For modified files, press `Y` to save and close, `N` to discard and close, or `Esc` to cancel. Untitled modified buffers open Save As before closing.
- Relative line numbers show the distance from the cursor line, which is useful for `j`/`k` navigation.
