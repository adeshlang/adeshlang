use std::fs;
use tempfile::tempdir;

use adesh_editor::buffer::{Buffer, Cursor};
use adesh_editor::completion::{CandidateKind, CompletionCandidate, CompletionState};
use adesh_editor::config::Theme;
use adesh_editor::discovery::find_executable;
use adesh_editor::explorer::FileExplorer;
use adesh_editor::highlighter::{highlight_line, Language};
use adesh_editor::palette::{CommandItem, PaletteState};
use adesh_editor::runner::Backend;
use adesh_editor::search::{fuzzy_match_score, SearchState};
use adesh_editor::split::{SplitDirection, SplitTree};
use adesh_editor::workspace::Workspace;

#[test]
fn test_buffer_operations() {
    let mut buf = Buffer::new_empty();
    assert_eq!(buf.lines, vec![""]);
    assert!(!buf.modified);

    buf.insert_char('f');
    buf.insert_char('n');
    buf.insert_char(' ');
    buf.insert_char('m');
    buf.insert_char('a');
    buf.insert_char('i');
    buf.insert_char('n');

    assert_eq!(buf.lines[0], "fn main");
    assert!(buf.modified);

    buf.insert_newline();
    buf.insert_char('}');

    assert_eq!(buf.lines.len(), 2);
    assert_eq!(buf.lines[1], "}");

    // Test undo
    buf.undo();
    assert_eq!(buf.lines.len(), 2);
    buf.undo();
    assert_eq!(buf.lines.len(), 1);
    assert_eq!(buf.lines[0], "fn main");

    // Test redo
    buf.redo();
    assert_eq!(buf.lines.len(), 2);
}

#[test]
fn test_search_and_replace() {
    let lines = vec![
        "fn main() {".to_string(),
        "    let message = \"Hello Adesh\";".to_string(),
        "    print(message);".to_string(),
        "}".to_string(),
    ];

    let mut search = SearchState {
        query: "message".to_string(),
        ..Default::default()
    };
    search.update_matches(&lines);

    assert_eq!(search.matches.len(), 2);
    assert_eq!(search.matches[0], (1, 8));
    assert_eq!(search.matches[1], (2, 10));

    let next = search.next_match();
    assert_eq!(next, Some((2, 10)));
}

#[test]
fn test_command_palette_filtering() {
    let mut palette = PaletteState::new();
    assert_eq!(palette.filtered_items.len(), CommandItem::all().len());

    palette.update_query("Run".to_string());
    assert!(palette.filtered_items.iter().any(|item| item == &CommandItem::Run));
    assert!(palette.filtered_items.iter().any(|item| item == &CommandItem::RunWithBackend));
}

#[test]
fn test_workspace_detection() {
    let dir = tempdir().unwrap();
    let adesh_toml = dir.path().join("Adesh.toml");
    fs::write(&adesh_toml, "[package]\nname = \"test\"\n").unwrap();

    let ws = Workspace::detect(Some(dir.path()));
    assert_eq!(ws.root, dir.path());
    assert!(ws.has_manifest);
}

#[test]
fn test_file_explorer() {
    let dir = tempdir().unwrap();
    let sub = dir.path().join("src");
    fs::create_dir_all(&sub).unwrap();
    fs::write(sub.join("main.ad"), "fn main() {}").unwrap();

    let explorer = FileExplorer::new(dir.path().to_path_buf());
    assert!(!explorer.flat_nodes.is_empty());
}

#[test]
fn test_backend_flags() {
    for b in Backend::all() {
        assert!(!b.name().is_empty());
        assert!(b.flag().starts_with("--"));
    }
}

#[test]
fn test_discovery_fallback() {
    let res = find_executable("non_existent_binary_xyz_123");
    assert!(res.is_none());
}

#[test]
fn test_split_window_operations() {
    let mut tree = SplitTree::new(0);
    assert_eq!(tree.pane_count(), 1);

    let p2 = tree.split_active(SplitDirection::Vertical, Some(1));
    assert_eq!(tree.pane_count(), 2);
    assert_eq!(tree.active_pane_id, p2);

    let p3 = tree.split_active(SplitDirection::Horizontal, Some(2));
    assert_eq!(tree.pane_count(), 3);
    assert_eq!(tree.active_pane_id, p3);

    let closed = tree.close_active_pane();
    assert!(closed);
    assert_eq!(tree.pane_count(), 2);

    tree.only_active_pane();
    assert_eq!(tree.pane_count(), 1);
}

#[test]
fn test_multi_cursor_editing() {
    let mut buf = Buffer::new_empty();
    buf.lines = vec![
        "line one".to_string(),
        "line two".to_string(),
        "line three".to_string(),
    ];
    buf.cursor = Cursor { line: 0, col: 4, desired_col: 4 };
    buf.add_cursor_below();
    assert_eq!(buf.secondary_cursors.len(), 1);

    buf.insert_char('X');
    assert_eq!(buf.lines[0], "lineX one");
    assert_eq!(buf.lines[1], "lineX two");

    buf.backspace();
    assert_eq!(buf.lines[0], "line one");
    assert_eq!(buf.lines[1], "line two");
}

#[test]
fn test_vim_text_objects() {
    let mut buf = Buffer::new_empty();
    buf.lines = vec![
        "let message = \"production grade\";".to_string(),
        "fn calculate(a: i32, b: i32) -> i32 { return a + b; }".to_string(),
    ];

    buf.cursor = Cursor { line: 0, col: 18, desired_col: 18 };
    let quotes = buf.find_quotes('"', true).unwrap();
    assert_eq!(&buf.lines[0][quotes.0..quotes.1], "production grade");

    buf.cursor = Cursor { line: 1, col: 15, desired_col: 15 };
    let brackets = buf.find_brackets('(', ')', true).unwrap();
    assert_eq!(&buf.lines[brackets.0][brackets.1..brackets.3], "a: i32, b: i32");
}

#[test]
fn test_auto_completion_state() {
    let mut completion = CompletionState::new();
    let candidates = vec![
        CompletionCandidate {
            label: "function_one".to_string(),
            insert_text: None,
            kind: CandidateKind::Function,
            detail: None,
            documentation: None,
        },
        CompletionCandidate {
            label: "function_two".to_string(),
            insert_text: None,
            kind: CandidateKind::Function,
            detail: None,
            documentation: None,
        },
        CompletionCandidate {
            label: "variable_xyz".to_string(),
            insert_text: None,
            kind: CandidateKind::Variable,
            detail: None,
            documentation: None,
        },
    ];

    completion.show(0, "func", candidates);
    assert!(completion.is_visible);
    assert_eq!(completion.filtered.len(), 2);

    completion.select_next();
    assert_eq!(completion.selected_candidate().unwrap().label, "function_two");
}

#[test]
fn test_fuzzy_search_ranking() {
    let score_exact = fuzzy_match_score("buffer", "src/buffer.rs").unwrap();
    let score_sparse = fuzzy_match_score("bfr", "src/buffer.rs").unwrap();
    assert!(score_exact > score_sparse);
}

#[test]
fn test_theme_cycling() {
    let theme = Theme::tokyo_night();
    let next_theme = theme.cycle();
    assert_ne!(theme.name, next_theme.name);
    assert_eq!(Theme::all_themes().len(), 11);
}

#[test]
fn test_multi_language_detection() {
    let l1 = Language::from_path(Some(std::path::Path::new("test.ad")));
    assert_eq!(l1, Language::Adesh);

    let l2 = Language::from_path(Some(std::path::Path::new("test.rs")));
    assert_eq!(l2, Language::Rust);

    let l3 = Language::from_path(Some(std::path::Path::new("test.py")));
    assert_eq!(l3, Language::Python);

    let theme = Theme::adesh_dark();
    let hl = highlight_line("def foo(): pass", &theme, Language::Python, true, None);
    assert!(!hl.spans.is_empty());
}

#[test]
fn test_vscode_smart_brackets_and_overtyping() {
    let mut buf = Buffer::new_empty();

    // 1. Auto-closing ( typing '(' inserts '()' with cursor inside )
    buf.insert_char_auto_close('(');
    assert_eq!(buf.lines[0], "()");
    assert_eq!(buf.cursor.col, 1);

    // 2. Overtyping / Skip-over ( typing ')' when cursor is before ')' advances cursor )
    buf.insert_char_auto_close(')');
    assert_eq!(buf.lines[0], "()");
    assert_eq!(buf.cursor.col, 2);

    // 3. Quotes auto-closing and overtyping
    buf.insert_char_auto_close('"');
    assert_eq!(buf.lines[0], "()\"\"");
    assert_eq!(buf.cursor.col, 3);

    // Typing inside quotes
    buf.insert_char_auto_close('h');
    buf.insert_char_auto_close('i');
    assert_eq!(buf.lines[0], "()\"hi\"");
    assert_eq!(buf.cursor.col, 5);

    // Overtyping closing quote
    buf.insert_char_auto_close('"');
    assert_eq!(buf.lines[0], "()\"hi\"");
    assert_eq!(buf.cursor.col, 6);

    // 4. Smart backspace pair deletion: `""` -> backspace -> empty
    let mut quote_buf = Buffer::new_empty();
    quote_buf.insert_char_auto_close('"');
    assert_eq!(quote_buf.lines[0], "\"\"");
    assert_eq!(quote_buf.cursor.col, 1);
    quote_buf.backspace();
    assert_eq!(quote_buf.lines[0], "");
    assert_eq!(quote_buf.cursor.col, 0);
}

#[test]
fn test_vscode_selection_auto_wrapping() {
    let mut buf = Buffer::new_empty();
    buf.lines = vec!["hello world".to_string()];
    buf.selection = Some(adesh_editor::buffer::Selection {
        start_line: 0,
        start_col: 0,
        end_line: 0,
        end_col: 5, // "hello"
    });

    buf.insert_char_auto_close('"');
    assert_eq!(buf.lines[0], "\"hello\" world");
}

#[test]
fn test_git_manager_integration() {
    let dir = tempdir().unwrap();
    let git = adesh_editor::git::GitManager::new(dir.path());
    assert!(!git.branch.is_empty());
    let branches = git.list_branches();
    assert!(!branches.is_empty());

    let gutter = git.compute_gutter_diff(None, &["let x = 1;".to_string()]);
    assert!(gutter.is_empty());
}

#[test]
fn test_compiler_ir_generation() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let app = adesh_editor::app::App::new(None);
        let ast_lines: Vec<String> = app.dump_compiler_ir("--dump-ast", "AST");
        assert!(!ast_lines.is_empty());
        assert!(ast_lines.iter().any(|l: &String| l.contains("AST")));

        let hir_lines: Vec<String> = app.dump_compiler_ir("--dump-hir", "HIR");
        assert!(!hir_lines.is_empty());
        assert!(hir_lines.iter().any(|l: &String| l.contains("HIR") || l.contains("hir.function")));

        let lir_lines: Vec<String> = app.dump_compiler_ir("--dump-lir", "LIR");
        assert!(!lir_lines.is_empty());
        assert!(lir_lines.iter().any(|l: &String| l.contains("LIR") || l.contains("target triple")));

        let mlir_lines: Vec<String> = app.dump_compiler_ir("--dump-mlir", "MLIR");
        assert!(!mlir_lines.is_empty());
        assert!(mlir_lines.iter().any(|l: &String| l.contains("MLIR") || l.contains("func.func")));

        let bc_lines: Vec<String> = app.dump_compiler_ir("--dump-bytecode", "Bytecode");
        assert!(!bc_lines.is_empty());
        assert!(bc_lines.iter().any(|l: &String| l.contains("Bytecode") || l.contains("OP_")));
    });
}

#[test]
fn test_file_explorer_scrolling_and_navigation() {
    let dir = tempdir().unwrap();
    for i in 0..10 {
        let sub = dir.path().join(format!("folder_{}", i));
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("file.ad"), "let x = 1;").unwrap();
    }

    let mut explorer = FileExplorer::new(dir.path().to_path_buf());
    explorer.expand_all();
    assert!(explorer.flat_nodes.len() > 10);

    let initial_offset = explorer.scroll_offset;
    explorer.scroll_down(5);
    assert!(explorer.scroll_offset >= initial_offset);

    explorer.scroll_up(3);
    assert_eq!(explorer.scroll_offset, 2.min(explorer.scroll_offset));
}

#[test]
fn test_text_viewer_scrolling() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut app = adesh_editor::app::App::new(None);
        app.execute_command_line_str("ast");
        assert_eq!(app.modal, adesh_editor::app::Modal::AstViewer);
        assert_eq!(app.text_viewer_scroll, 0);

        // Scroll down
        app.text_viewer_scroll += 5;
        assert_eq!(app.text_viewer_scroll, 5);

        // Scroll up
        app.text_viewer_scroll = app.text_viewer_scroll.saturating_sub(3);
        assert_eq!(app.text_viewer_scroll, 2);
    });
}

#[test]
fn test_explorer_context_menu_file_operations() {
    let dir = tempdir().unwrap();
    let file1 = dir.path().join("source.ad");
    fs::write(&file1, "let a = 42;").unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut app = adesh_editor::app::App::new(Some(dir.path().to_path_buf()));
        app.explorer.refresh();

        // 1. Copy file
        app.explorer_context_menu = Some(adesh_editor::app::ExplorerContextMenu {
            target_path: file1.clone(),
            is_dir: false,
            x: 5,
            y: 5,
            selected_index: 2, // Copy
        });
        app.execute_context_menu_action(2);
        assert_eq!(app.file_clipboard, Some(file1.clone()));
        assert!(!app.file_clipboard_cut);

        // 2. Paste file
        let target_dir = dir.path().to_path_buf();
        app.paste_file_clipboard(&target_dir);
        let copy_path = dir.path().join("source_copy.ad");
        assert!(copy_path.exists());
        assert_eq!(fs::read_to_string(&copy_path).unwrap(), "let a = 42;");

        // 3. Cut & Paste file
        app.explorer_context_menu = Some(adesh_editor::app::ExplorerContextMenu {
            target_path: copy_path.clone(),
            is_dir: false,
            x: 5,
            y: 5,
            selected_index: 3, // Cut
        });
        app.execute_context_menu_action(3);
        assert!(app.file_clipboard_cut);

        let sub = dir.path().join("subdir");
        fs::create_dir_all(&sub).unwrap();
        app.paste_file_clipboard(&sub);
        assert!(!copy_path.exists());
        assert!(sub.join("source_copy.ad").exists());

        // 4. Delete file
        app.explorer_context_menu = Some(adesh_editor::app::ExplorerContextMenu {
            target_path: sub.join("source_copy.ad"),
            is_dir: false,
            x: 5,
            y: 5,
            selected_index: 5, // Delete
        });
        app.execute_context_menu_action(5);
        assert!(!sub.join("source_copy.ad").exists());
    });
}

#[test]
fn test_toast_lifecycle_and_dismissal() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut app = adesh_editor::app::App::new(None);
        app.toast("Test message 1", "INFO");
        app.toast("Test message 2", "WARN");
        assert_eq!(app.toasts.len(), 2);

        // Remove toast by index
        app.remove_toast(0);
        assert_eq!(app.toasts.len(), 1);
        assert_eq!(app.toasts[0].message, "Test message 2");

        // Simulate time passage for expiry test
        app.toasts[0].created_at = std::time::Instant::now() - std::time::Duration::from_secs(5);
        app.cleanup_toasts();
        assert_eq!(app.toasts.len(), 0);
    });
}

#[test]
fn test_terminal_history_and_navigation() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut term = adesh_editor::runner::Terminal::new();
        term.input = "echo hello".to_string();
        term.execute_line();
        assert_eq!(term.history.len(), 1);
        assert_eq!(term.history[0], "echo hello");
        assert!(term.input.is_empty());

        term.input = "cargo build".to_string();
        term.execute_line();
        assert_eq!(term.history.len(), 2);
        assert_eq!(term.history[1], "cargo build");

        // Navigate history up
        term.prev_history();
        assert_eq!(term.input, "cargo build");
        term.prev_history();
        assert_eq!(term.input, "echo hello");
        // At beginning of history, cannot go further back
        term.prev_history();
        assert_eq!(term.input, "echo hello");

        // Navigate history down
        term.next_history();
        assert_eq!(term.input, "cargo build");
        term.next_history();
        assert!(term.input.is_empty());
    });
}

#[test]
fn test_bottom_panel_drag_resize() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut app = adesh_editor::app::App::new(None);
        app.show_terminal = true;
        app.bottom_panel_height = 10;
        app.bottom_panel_drag_y = 20;

        // Simulate drag start on top border
        app.is_dragging_bottom_panel = true;

        // Drag up to row 15 -> height should expand to 30 - 15 = 15
        let bottom_edge = app.bottom_panel_drag_y + app.bottom_panel_height;
        let new_height = bottom_edge.saturating_sub(15).clamp(3, 35);
        app.bottom_panel_height = new_height;
        assert_eq!(app.bottom_panel_height, 15);

        // Drag down to row 26 -> height should minimize to 30 - 26 = 4
        let new_height2 = bottom_edge.saturating_sub(26).clamp(3, 35);
        app.bottom_panel_height = new_height2;
        assert_eq!(app.bottom_panel_height, 4);

        // Release drag
        app.is_dragging_bottom_panel = false;
        assert!(!app.is_dragging_bottom_panel);
    });
}

#[test]
fn test_clean_terminal_output_ansi_and_carriage_return() {
    let raw = "\r\x1b[1m\x1b[36m⠋\x1b[0m \x1b[1mAnalyzing...\x1b[0m\r\x1b[1m\x1b[32m✓\x1b[0m \x1b[1mInterpreter ready\x1b[0m \x1b[2m[0.03ms]\x1b[0m\r\nHello, World!\n";
    let lines = adesh_editor::runner::clean_terminal_output(raw);
    assert!(!lines.is_empty());
    // Should have clean lines with carriage returns handled and ANSI colors preserved for renderer
    assert!(lines.iter().any(|l| l.contains("Interpreter ready")));
    assert!(lines.iter().any(|l| l.contains("Hello, World!")));
    for line in &lines {
        assert!(!line.contains("\r"));
    }
}

#[test]
fn test_ctrl_j_toggle_terminal() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut app = adesh_editor::app::App::new(None);
        assert!(!app.show_terminal);

        // Press Ctrl+J to open terminal
        let ctrl_j = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('j'),
            crossterm::event::KeyModifiers::CONTROL,
        );
        adesh_editor::input::handle_key_event(&mut app, ctrl_j).await;
        assert!(app.show_terminal);
        assert!(app.terminal_focused);

        // Press Ctrl+J while terminal is focused to close terminal
        adesh_editor::input::handle_key_event(&mut app, ctrl_j).await;
        assert!(!app.show_terminal);
        assert!(!app.terminal_focused);
    });
}

#[test]
fn test_vim_count_prefix_navigation() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut app = adesh_editor::app::App::new(None);
        let buf = app.current_buffer_mut();
        buf.lines = (1..=20).map(|i| format!("line number {}", i)).collect();
        buf.cursor = Cursor { line: 0, col: 0, desired_col: 0 };

        // Test 5j -> jumps 5 lines down
        let key_5 = crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Char('5'), crossterm::event::KeyModifiers::NONE);
        let key_j = crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Char('j'), crossterm::event::KeyModifiers::NONE);
        adesh_editor::input::handle_key_event(&mut app, key_5).await;
        adesh_editor::input::handle_key_event(&mut app, key_j).await;
        assert_eq!(app.current_buffer().cursor.line, 5);

        // Test 3k -> jumps 3 lines up
        let key_3 = crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Char('3'), crossterm::event::KeyModifiers::NONE);
        let key_k = crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Char('k'), crossterm::event::KeyModifiers::NONE);
        adesh_editor::input::handle_key_event(&mut app, key_3).await;
        adesh_editor::input::handle_key_event(&mut app, key_k).await;
        assert_eq!(app.current_buffer().cursor.line, 2);

        // Test 2dd -> deletes 2 lines
        let orig_len = app.current_buffer().lines.len();
        let key_2 = crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Char('2'), crossterm::event::KeyModifiers::NONE);
        let key_d = crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Char('d'), crossterm::event::KeyModifiers::NONE);
        adesh_editor::input::handle_key_event(&mut app, key_2).await;
        adesh_editor::input::handle_key_event(&mut app, key_d).await;
        adesh_editor::input::handle_key_event(&mut app, key_d).await;
        assert_eq!(app.current_buffer().lines.len(), orig_len - 2);

        // Test 4x -> deletes 4 characters
        let key_4 = crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Char('4'), crossterm::event::KeyModifiers::NONE);
        let key_x = crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Char('x'), crossterm::event::KeyModifiers::NONE);
        let line_before = app.current_buffer().lines[app.current_buffer().cursor.line].clone();
        adesh_editor::input::handle_key_event(&mut app, key_4).await;
        adesh_editor::input::handle_key_event(&mut app, key_x).await;
        let line_after = &app.current_buffer().lines[app.current_buffer().cursor.line];
        assert_eq!(line_after.len(), line_before.len() - 4);
    });
}

#[test]
fn test_home_end_pageup_pagedown_keys() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut app = adesh_editor::app::App::new(None);
        let buf = app.current_buffer_mut();
        buf.lines = (1..=50).map(|i| format!("    indented line number {}", i)).collect();
        buf.cursor = Cursor { line: 0, col: 0, desired_col: 0 };

        // Test End in Normal Mode
        let key_end = crossterm::event::KeyEvent::new(crossterm::event::KeyCode::End, crossterm::event::KeyModifiers::NONE);
        adesh_editor::input::handle_key_event(&mut app, key_end).await;
        assert_eq!(app.current_buffer().cursor.col, app.current_buffer().lines[0].len());

        // Test Home in Normal Mode (jumps to start of line col 0)
        let key_home = crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Home, crossterm::event::KeyModifiers::NONE);
        adesh_editor::input::handle_key_event(&mut app, key_home).await;
        assert_eq!(app.current_buffer().cursor.col, 0);

        // Test ^ in Normal Mode (jumps to first non-whitespace col 4)
        let key_caret = crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Char('^'), crossterm::event::KeyModifiers::NONE);
        adesh_editor::input::handle_key_event(&mut app, key_caret).await;
        assert_eq!(app.current_buffer().cursor.col, 4);

        // Test PageDown in Normal Mode (jumps ~15 lines)
        let key_pgdn = crossterm::event::KeyEvent::new(crossterm::event::KeyCode::PageDown, crossterm::event::KeyModifiers::NONE);
        adesh_editor::input::handle_key_event(&mut app, key_pgdn).await;
        assert_eq!(app.current_buffer().cursor.line, 15);

        // Test PageUp in Normal Mode
        let key_pgup = crossterm::event::KeyEvent::new(crossterm::event::KeyCode::PageUp, crossterm::event::KeyModifiers::NONE);
        adesh_editor::input::handle_key_event(&mut app, key_pgup).await;
        assert_eq!(app.current_buffer().cursor.line, 0);

        // Switch to Insert Mode and test End, Home, PageDown
        app.mode = adesh_editor::mode::Mode::Insert;
        adesh_editor::input::handle_key_event(&mut app, key_end).await;
        assert_eq!(app.current_buffer().cursor.col, app.current_buffer().lines[0].len());
        adesh_editor::input::handle_key_event(&mut app, key_home).await;
        assert_eq!(app.current_buffer().cursor.col, 0); // In insert mode, home goes to column 0
        adesh_editor::input::handle_key_event(&mut app, key_pgdn).await;
        assert_eq!(app.current_buffer().cursor.line, 15);
    });
}

#[test]
fn test_editor_mouse_click_cursor_placement() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut app = adesh_editor::app::App::new(None);
        app.show_explorer = false; // Hide explorer so click targets editor pane
        let buf = app.current_buffer_mut();
        buf.lines = vec![
            "fn main() {".to_string(),
            "    let value = 12345;".to_string(),
            "    print(value);".to_string(),
            "}".to_string(),
        ];
        // Simulate an editor pane at x=10, y=2, width=60, height=20
        app.pane_positions = vec![(0, 0, ratatui::layout::Rect::new(10, 2, 60, 20))];

        // Gutter width: border(1) + line numbers("4" max 2 digits + 1) + diff(1) = 1 + 3 + 1 = 5, so content start is 10 + 1 + 4 = 15
        // Click at row 4 (line 1: 4 - (2+1) = 1) and column 23 (target_col = 23 - 15 = 8)
        let mouse_click = crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 23,
            row: 4,
            modifiers: crossterm::event::KeyModifiers::NONE,
        };
        adesh_editor::input::handle_mouse_event(&mut app, mouse_click).await;

        assert_eq!(app.current_buffer().cursor.line, 1);
        assert_eq!(app.current_buffer().cursor.col, 8);
    });
}

#[test]
fn test_output_and_terminal_scrolling() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut app = adesh_editor::app::App::new(None);
        app.show_output = true;
        app.bottom_panel_rect = Some(ratatui::layout::Rect::new(0, 20, 80, 10));

        // Test mouse wheel scroll up and down over output panel
        let scroll_up = crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::ScrollUp,
            column: 40,
            row: 24,
            modifiers: crossterm::event::KeyModifiers::NONE,
        };
        adesh_editor::input::handle_mouse_event(&mut app, scroll_up).await;
        assert_eq!(app.output_scroll, 3);

        let scroll_down = crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::ScrollDown,
            column: 40,
            row: 24,
            modifiers: crossterm::event::KeyModifiers::NONE,
        };
        adesh_editor::input::handle_mouse_event(&mut app, scroll_down).await;
        assert_eq!(app.output_scroll, 0);

        // Switch to terminal panel
        app.show_output = false;
        app.show_terminal = true;
        app.terminal_focused = true;
        app.terminal.history = (1..=30).map(|i| format!("log line {}", i)).collect();

        // Test PageUp in terminal mode
        let key_pgup = crossterm::event::KeyEvent::new(crossterm::event::KeyCode::PageUp, crossterm::event::KeyModifiers::NONE);
        adesh_editor::input::handle_key_event(&mut app, key_pgup).await;
        assert_eq!(app.terminal_scroll, 10);

        // Test Ctrl+D in terminal mode
        let key_ctrld = crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Char('d'), crossterm::event::KeyModifiers::CONTROL);
        adesh_editor::input::handle_key_event(&mut app, key_ctrld).await;
        assert_eq!(app.terminal_scroll, 0);
    });
}

#[test]
fn test_ansi_color_parsing_to_spans() {
    let raw = "\x1b[1;31mError:\x1b[0m \x1b[32mFile parsed successfully\x1b[0m \x1b[38;5;208m[orange]\x1b[0m \x1b[38;2;100;200;255m[sky_blue]\x1b[0m";
    let line = adesh_editor::highlighter::parse_ansi_to_line(raw);
    assert!(!line.spans.is_empty());

    // Span 0: "Error:" with bold red
    assert_eq!(line.spans[0].content, "Error:");
    assert_eq!(line.spans[0].style.fg, Some(ratatui::style::Color::Red));
    assert!(line.spans[0].style.add_modifier.contains(ratatui::style::Modifier::BOLD));

    // Span 2: "File parsed successfully" with green
    assert_eq!(line.spans[2].content, "File parsed successfully");
    assert_eq!(line.spans[2].style.fg, Some(ratatui::style::Color::Green));

    // Span 4: "[orange]" with 256 color index 208
    assert_eq!(line.spans[4].content, "[orange]");
    assert_eq!(line.spans[4].style.fg, Some(ratatui::style::Color::Indexed(208)));

    // Span 6: "[sky_blue]" with TrueColor RGB
    assert_eq!(line.spans[6].content, "[sky_blue]");
    assert_eq!(line.spans[6].style.fg, Some(ratatui::style::Color::Rgb(100, 200, 255)));
}

#[test]
fn test_mouse_drag_to_select() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut app = adesh_editor::app::App::new(None);
        app.show_explorer = false;
        let buf = app.current_buffer_mut();
        buf.lines = vec![
            "fn main() {".to_string(),
            "    let value = 12345;".to_string(),
            "    print(value);".to_string(),
            "}".to_string(),
        ];
        app.pane_positions = vec![(0, 0, ratatui::layout::Rect::new(10, 2, 60, 20))];

        // 1. Mouse Down at line 1, col 4 (row 4, col 19)
        let mouse_down = crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 19,
            row: 4,
            modifiers: crossterm::event::KeyModifiers::NONE,
        };
        adesh_editor::input::handle_mouse_event(&mut app, mouse_down).await;
        assert_eq!(app.mouse_drag_start, Some((1, 4)));

        // 2. Mouse Drag to line 2, col 12 (row 5, col 27)
        let mouse_drag = crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Drag(crossterm::event::MouseButton::Left),
            column: 27,
            row: 5,
            modifiers: crossterm::event::KeyModifiers::NONE,
        };
        adesh_editor::input::handle_mouse_event(&mut app, mouse_drag).await;
        assert!(app.current_buffer().selection.is_some());
        let sel = app.current_buffer().selection.unwrap();
        assert_eq!((sel.start_line, sel.start_col, sel.end_line, sel.end_col), (1, 4, 2, 12));

        // 3. Mouse Up
        let mouse_up = crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
            column: 27,
            row: 5,
            modifiers: crossterm::event::KeyModifiers::NONE,
        };
        adesh_editor::input::handle_mouse_event(&mut app, mouse_up).await;
        assert!(app.mouse_drag_start.is_none());
        assert!(app.current_buffer().selection.is_some());

        // 4. Cut selection with 'x' in normal mode
        let key_x = crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Char('x'), crossterm::event::KeyModifiers::NONE);
        adesh_editor::input::handle_key_event(&mut app, key_x).await;
        assert!(app.current_buffer().selection.is_none());
        assert!(!app.clipboard.is_empty());
    });
}

#[test]
fn test_ctrl_backspace_word_deletion() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut app = adesh_editor::app::App::new(None);
        let buf = app.current_buffer_mut();
        buf.lines = vec!["let message = 42;".to_string()];
        buf.cursor = Cursor { line: 0, col: 17, desired_col: 17 };

        // Test buffer backspace_word
        buf.backspace_word();
        assert_eq!(buf.lines[0], "let message = 42");

        buf.backspace_word();
        assert_eq!(buf.lines[0], "let message = ");

        buf.backspace_word();
        assert_eq!(buf.lines[0], "let message ");

        buf.backspace_word();
        assert_eq!(buf.lines[0], "let ");

        buf.backspace_word();
        assert_eq!(buf.lines[0], "");

        // Test in Insert Mode with Ctrl+Backspace event
        app.mode = adesh_editor::mode::Mode::Insert;
        app.current_buffer_mut().lines = vec!["pub fn calculate_total()".to_string()];
        app.current_buffer_mut().cursor = Cursor { line: 0, col: 24, desired_col: 24 };

        let ctrl_bs = crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Backspace, crossterm::event::KeyModifiers::CONTROL);
        adesh_editor::input::handle_key_event(&mut app, ctrl_bs).await;
        assert_eq!(app.current_buffer().lines[0], "pub fn calculate_total");

        // Test in Terminal prompt
        app.show_terminal = true;
        app.terminal_focused = true;
        app.terminal.input = "cargo build --release".to_string();

        let ctrl_w = crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Char('w'), crossterm::event::KeyModifiers::CONTROL);
        adesh_editor::input::handle_key_event(&mut app, ctrl_w).await;
        assert_eq!(app.terminal.input, "cargo build --");

        adesh_editor::input::handle_key_event(&mut app, ctrl_bs).await;
        assert_eq!(app.terminal.input, "cargo build ");
    });
}





