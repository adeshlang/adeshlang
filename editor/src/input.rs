use std::path::PathBuf;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind, MouseButton};


use crate::app::{App, MenuTab, Modal};
use crate::completion::{CandidateKind, CompletionCandidate};
use crate::config::LineNumberMode;
use crate::mode::Mode;
use crate::palette::CommandItem;
use crate::runner::Backend;
use crate::split::SplitDirection;

pub async fn handle_event(app: &mut App, event: Event) {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            handle_key_event(app, key).await;
        }
        Event::Mouse(mouse) => {
            handle_mouse_event(app, mouse).await;
        }
        _ => {}
    }
}

pub fn is_ctrl_backspace(key: &KeyEvent) -> bool {
    (key.code == KeyCode::Backspace && key.modifiers.contains(KeyModifiers::CONTROL))
        || key.code == KeyCode::Char('\u{7f}')
        || key.code == KeyCode::Char('\u{8}')
        || (key.modifiers.contains(KeyModifiers::CONTROL) && (key.code == KeyCode::Char('w') || key.code == KeyCode::Char('W')))
}

pub fn pop_word_from_string(s: &mut String) {
    while s.ends_with(|c: char| c.is_whitespace()) {
        s.pop();
    }
    if let Some(last) = s.chars().last() {
        let is_w = last.is_alphanumeric() || last == '_';
        while let Some(c) = s.chars().last() {
            if (c.is_alphanumeric() || c == '_') == is_w && !c.is_whitespace() {
                s.pop();
            } else {
                break;
            }
        }
    }
}

pub async fn handle_key_event(app: &mut App, key: KeyEvent) {
    // 0. Global Panel Toggle Keys (Ctrl+J / Ctrl+T / Ctrl+` / F6)
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Char('J') | KeyCode::Char('\n') | KeyCode::Char('t') | KeyCode::Char('T') => {
                app.show_terminal = !app.show_terminal;
                app.terminal_focused = app.show_terminal;
                if app.show_terminal {
                    app.show_output = false;
                }
                return;
            }
            KeyCode::Char('`') | KeyCode::Char('~') | KeyCode::Char('@') => {
                app.show_output = !app.show_output;
                if app.show_output {
                    app.show_terminal = false;
                    app.terminal_focused = false;
                }
                return;
            }
            _ => {}
        }
    }

    // 0. Context Menu for File Explorer
    if let Some(mut ctx) = app.explorer_context_menu.clone() {
        let item_count = 8;
        match key.code {
            KeyCode::Esc => {
                app.explorer_context_menu = None;
                return;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                ctx.selected_index = (ctx.selected_index + 1) % item_count;
                app.explorer_context_menu = Some(ctx);
                return;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if ctx.selected_index == 0 {
                    ctx.selected_index = item_count - 1;
                } else {
                    ctx.selected_index -= 1;
                }
                app.explorer_context_menu = Some(ctx);
                return;
            }
            KeyCode::Enter => {
                let sel = ctx.selected_index;
                app.execute_context_menu_action(sel);
                return;
            }
            _ => {}
        }
    }

    // 1. Command-line mode (:...)
    if app.show_command_line {
        handle_command_line_input(app, key).await;
        return;
    }

    // 2. Terminal mode
    if app.terminal_focused && app.modal == Modal::None && app.active_menu == MenuTab::None {
        handle_terminal_input(app, key);
        return;
    }

    // 3. Floating Completion Popup Active
    if app.completion.is_visible && app.modal == Modal::None {
        match key.code {
            KeyCode::Esc => {
                app.completion.dismiss();
                return;
            }
            KeyCode::Down | KeyCode::Tab => {
                app.completion.select_next();
                return;
            }
            KeyCode::Up | KeyCode::BackTab => {
                app.completion.select_prev();
                return;
            }
            KeyCode::Enter => {
                if let Some(cand) = app.completion.selected_candidate().cloned() {
                    let insert = cand.insert_text.unwrap_or(cand.label);
                    let prefix_len = app.completion.prefix.chars().count();
                    for _ in 0..prefix_len {
                        app.current_buffer_mut().backspace();
                    }
                    for ch in insert.chars() {
                        app.current_buffer_mut().insert_char(ch);
                    }
                    app.completion.dismiss();
                    return;
                }
            }
            _ => {}
        }
    }

    // 4. Modal Dialogs Handling
    if app.modal != Modal::None {
        handle_modal_input(app, key).await;
        return;
    }

    // 5. Menu Dropdowns
    if app.active_menu != MenuTab::None {
        handle_menu_key(app, key).await;
        return;
    }

    // 6. Alt+Letter Menu Activation
    if key.modifiers.contains(KeyModifiers::ALT) && !key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('f') | KeyCode::Char('F') => {
                app.active_menu = MenuTab::File;
                app.menu_selected = 0;
                return;
            }
            KeyCode::Char('e') | KeyCode::Char('E') => {
                app.active_menu = MenuTab::Edit;
                app.menu_selected = 0;
                return;
            }
            KeyCode::Char('v') | KeyCode::Char('V') => {
                app.active_menu = MenuTab::View;
                app.menu_selected = 0;
                return;
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                app.active_menu = MenuTab::Run;
                app.menu_selected = 0;
                return;
            }
            KeyCode::Char('g') | KeyCode::Char('G') => {
                app.active_menu = MenuTab::Git;
                app.menu_selected = 0;
                return;
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                app.active_menu = MenuTab::Adesh;
                app.menu_selected = 0;
                return;
            }
            KeyCode::Char('i') | KeyCode::Char('I') => {
                app.active_menu = MenuTab::Info;
                app.menu_selected = 0;
                return;
            }
            _ => {}
        }
    }

    // 7. Global Shortcuts (Ctrl+...)
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        if key.modifiers.contains(KeyModifiers::SHIFT) {
            match key.code {
                KeyCode::Char('f') | KeyCode::Char('F') => {
                    app.grep_query.clear();
                    app.grep_results.clear();
                    app.modal = Modal::WorkspaceGrep;
                    return;
                }
                _ => {}
            }
        }

        if key.modifiers.contains(KeyModifiers::ALT) {
            match key.code {
                KeyCode::Up => {
                    app.current_buffer_mut().add_cursor_above();
                    return;
                }
                KeyCode::Down => {
                    app.current_buffer_mut().add_cursor_below();
                    return;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                app.running = false;
                return;
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                app.save_current_buffer();
                return;
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                app.prompt_input.clear();
                app.populate_fuzzy_files();
                app.modal = Modal::FuzzyFileFinder;
                return;
            }
            KeyCode::Char('g') | KeyCode::Char('G') => {
                let root = app.workspace.root.clone();
                app.git.refresh(&root);
                app.git_selected = 0;
                app.modal = Modal::GitManager;
                return;
            }
            KeyCode::Char('f') | KeyCode::Char('F') => {
                app.modal = Modal::Search;
                let lines = app.current_buffer().lines.clone();
                app.search.update_matches(&lines);
                return;
            }
            KeyCode::Char('h') | KeyCode::Char('H') => {
                app.modal = Modal::Replace;
                let lines = app.current_buffer().lines.clone();
                app.search.update_matches(&lines);
                return;
            }
            KeyCode::Char('z') | KeyCode::Char('Z') => {
                app.current_buffer_mut().undo();
                return;
            }
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                app.current_buffer_mut().redo();
                return;
            }
            KeyCode::Char('o') | KeyCode::Char('O') => {
                app.modal = Modal::DocumentSymbols;
                return;
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                app.prompt_input.clear();
                app.modal = Modal::NewFilePrompt;
                return;
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                if let Some(text) = app.current_buffer().get_selected_text() {
                    app.clipboard = text;
                    app.toast("Copied selection", "INFO");
                } else {
                    app.clipboard = app.current_buffer().yank_line();
                    app.toast("Yanked line", "INFO");
                }
                return;
            }
            KeyCode::Char('x') | KeyCode::Char('X') => {
                if let Some(text) = app.current_buffer_mut().cut_selection() {
                    app.clipboard = text;
                    app.toast("Cut selection", "INFO");
                } else {
                    app.clipboard = app.current_buffer_mut().cut_line();
                    app.toast("Cut line", "INFO");
                }
                return;
            }
            KeyCode::Char('v') | KeyCode::Char('V') => {
                if !app.clipboard.is_empty() {
                    let clip = app.clipboard.clone();
                    app.current_buffer_mut().paste(&clip);
                    app.toast("Pasted", "INFO");
                }
                return;
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                let last_line = app.current_buffer().lines.len().saturating_sub(1);
                let last_col = app.current_buffer().lines[last_line].chars().count();
                app.current_buffer_mut().selection = Some(crate::buffer::Selection {
                    start_line: 0,
                    start_col: 0,
                    end_line: last_line,
                    end_col: last_col,
                });
                return;
            }
            KeyCode::Char('`') => {
                app.show_output = !app.show_output;
                return;
            }
            KeyCode::Char('j') | KeyCode::Char('J') => {
                app.show_terminal = !app.show_terminal;
                app.terminal_focused = app.show_terminal;
                return;
            }
            KeyCode::Char('e') | KeyCode::Char('E') => {
                app.show_explorer = true;
                app.explorer_focused = !app.explorer_focused;
                return;
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                app.current_buffer_mut().duplicate_line();
                return;
            }
            KeyCode::Char('/') => {
                let comment = app.current_buffer().comment_string();
                app.current_buffer_mut().comment_toggle(comment);
                return;
            }
            KeyCode::Char(' ') => {
                let word = app.current_buffer().get_word_under_cursor().unwrap_or_default();
                let candidates = vec![
                    CompletionCandidate { label: "fn".to_string(), insert_text: Some("fn name() {\n    \n}".to_string()), kind: CandidateKind::Snippet, detail: Some("Function declaration".to_string()), documentation: None },
                    CompletionCandidate { label: "let".to_string(), insert_text: Some("let ".to_string()), kind: CandidateKind::Keyword, detail: None, documentation: None },
                    CompletionCandidate { label: "mut".to_string(), insert_text: Some("mut ".to_string()), kind: CandidateKind::Keyword, detail: None, documentation: None },
                    CompletionCandidate { label: "struct".to_string(), insert_text: Some("struct Name {\n    \n}".to_string()), kind: CandidateKind::Snippet, detail: None, documentation: None },
                    CompletionCandidate { label: "enum".to_string(), insert_text: Some("enum Name {\n    Variant,\n}".to_string()), kind: CandidateKind::Snippet, detail: None, documentation: None },
                    CompletionCandidate { label: "trait".to_string(), insert_text: Some("trait Name {\n    fn method();\n}".to_string()), kind: CandidateKind::Snippet, detail: None, documentation: None },
                    CompletionCandidate { label: "impl".to_string(), insert_text: Some("impl Trait for Struct {\n    \n}".to_string()), kind: CandidateKind::Snippet, detail: None, documentation: None },
                    CompletionCandidate { label: "println".to_string(), insert_text: Some("println(\"\")".to_string()), kind: CandidateKind::Function, detail: Some("Print line to stdout".to_string()), documentation: None },
                    CompletionCandidate { label: "print".to_string(), insert_text: Some("print(\"\")".to_string()), kind: CandidateKind::Function, detail: None, documentation: None },
                ];
                let col = app.current_buffer().cursor.col;
                app.completion.show(col, &word, candidates);
                return;
            }
            KeyCode::Char('w') | KeyCode::Char('W') => {
                app.pending_operator = Some('w');
                return;
            }
            _ => {}
        }
    }

    if app.pending_operator == Some('w') {
        app.pending_operator = None;
        match key.code {
            KeyCode::Char('v') | KeyCode::Char('V') => {
                app.split_tree.split_active(SplitDirection::Vertical, None);
                app.toast("Split vertical", "INFO");
                return;
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                app.split_tree.split_active(SplitDirection::Horizontal, None);
                app.toast("Split horizontal", "INFO");
                return;
            }
            KeyCode::Char('c') | KeyCode::Char('C') | KeyCode::Char('q') => {
                app.split_tree.close_active_pane();
                if let Some(pane) = app.split_tree.active_pane() {
                    app.active_buffer = pane.buffer_index;
                }
                return;
            }
            KeyCode::Char('o') | KeyCode::Char('O') => {
                app.split_tree.only_active_pane();
                app.toast("Zoomed active pane", "INFO");
                return;
            }
            KeyCode::Char('w') | KeyCode::Char('W') | KeyCode::Right | KeyCode::Char('l') => {
                app.split_tree.cycle_pane(true);
                if let Some(pane) = app.split_tree.active_pane() {
                    app.active_buffer = pane.buffer_index;
                }
                return;
            }
            KeyCode::Left | KeyCode::Char('h') => {
                app.split_tree.cycle_pane(false);
                if let Some(pane) = app.split_tree.active_pane() {
                    app.active_buffer = pane.buffer_index;
                }
                return;
            }
            _ => {}
        }
    }

    // Function keys
    match key.code {
        KeyCode::F(1) => {
            app.modal = Modal::KeybindingsHelp;
            return;
        }
        KeyCode::F(2) => {
            app.save_current_buffer();
            return;
        }
        KeyCode::F(3) => {
            app.show_explorer = !app.show_explorer;
            return;
        }
        KeyCode::F(4) => {
            app.toggle_line_numbers();
            return;
        }
        KeyCode::F(5) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                app.modal = Modal::SelectBackend;
            } else {
                app.execute_palette_command(CommandItem::Run);
            }
            return;
        }
        KeyCode::F(6) => {
            app.runner.stop();
            app.toast("Stopped execution", "WARN");
            return;
        }
        _ => {}
    }

    // 8. File Explorer Focused
    if app.explorer_focused && app.show_explorer {
        handle_explorer_input(app, key).await;
        return;
    }

    // 9. Editor Buffer Navigation / Editing
    match app.mode {
        Mode::Normal => handle_normal_mode(app, key).await,
        Mode::Insert => handle_insert_mode(app, key).await,
        Mode::Visual => handle_visual_mode(app, key).await,
        Mode::Command => {
            app.show_command_line = true;
            app.command_line.clear();
        }
    }
}

async fn handle_normal_mode(app: &mut App, key: KeyEvent) {
    if is_ctrl_backspace(&key) {
        app.current_buffer_mut().backspace_word();
        return;
    }

    if app.current_buffer().selection.is_some() {
        match key.code {
            KeyCode::Char('y') => {
                if let Some(text) = app.current_buffer().get_selected_text() {
                    app.clipboard = text;
                    app.toast("Yanked selection", "INFO");
                }
                app.current_buffer_mut().selection = None;
                return;
            }
            KeyCode::Char('d') | KeyCode::Char('x') | KeyCode::Delete | KeyCode::Backspace => {
                if let Some(text) = app.current_buffer_mut().cut_selection() {
                    app.clipboard = text;
                    app.toast("Cut selection", "INFO");
                }
                return;
            }
            KeyCode::Char('c') => {
                app.current_buffer_mut().delete_selection_if_any();
                app.mode = Mode::Insert;
                return;
            }
            KeyCode::Esc => {
                app.current_buffer_mut().selection = None;
                return;
            }
            _ => {}
        }
    }

    if let Some(op) = app.pending_operator {
        app.pending_operator = None;
        let count = app.count_prefix.take().unwrap_or(1);
        match op {
            'r' => {
                if let KeyCode::Char(c) = key.code {
                    for _ in 0..count {
                        app.current_buffer_mut().replace_char(c);
                    }
                }
                return;
            }
            'f' | 'F' | 't' | 'T' => {
                if let KeyCode::Char(c) = key.code {
                    let forward = op == 'f' || op == 't';
                    let till = op == 't' || op == 'T';
                    for _ in 0..count {
                        app.current_buffer_mut().find_char_in_line(c, forward, till);
                    }
                }
                return;
            }
            'd' => {
                if key.code == KeyCode::Char('d') {
                    for _ in 0..count {
                        app.clipboard = app.current_buffer_mut().cut_line();
                    }
                    app.toast("Cut line", "INFO");
                } else if key.code == KeyCode::Char('w') {
                    for _ in 0..count {
                        app.current_buffer_mut().delete_word();
                    }
                }
                return;
            }
            'c' => {
                if key.code == KeyCode::Char('w') {
                    for _ in 0..count {
                        app.current_buffer_mut().change_word();
                    }
                    app.mode = Mode::Insert;
                } else if key.code == KeyCode::Char('c') {
                    for _ in 0..count {
                        app.clipboard = app.current_buffer_mut().cut_line();
                    }
                    app.mode = Mode::Insert;
                }
                return;
            }
            'y' => {
                if key.code == KeyCode::Char('y') {
                    for _ in 0..count {
                        app.clipboard = app.current_buffer().yank_line();
                    }
                    app.toast("Yanked line", "INFO");
                }
                return;
            }
            _ => {}
        }
    }

    // Accumulate count prefix for numbers in Normal mode (e.g., 5j, 10w, 3dd)
    if let KeyCode::Char(c) = key.code {
        if app.count_prefix.is_none() && c.is_ascii_digit() && c != '0' {
            app.count_prefix = Some((c as usize) - ('0' as usize));
            return;
        } else if let Some(p) = app.count_prefix {
            if c.is_ascii_digit() {
                app.count_prefix = Some(p.saturating_mul(10).saturating_add((c as usize) - ('0' as usize)));
                return;
            }
        }
    }

    let is_operator_init = matches!(key.code, KeyCode::Char('d' | 'c' | 'y' | 'r' | 'f' | 'F' | 't' | 'T'));
    let count = if is_operator_init {
        app.count_prefix.unwrap_or(1)
    } else {
        app.count_prefix.take().unwrap_or(1)
    };

    match key.code {
        KeyCode::Char(':') => {
            app.show_command_line = true;
            app.command_line.clear();
        }
        KeyCode::Char('i') => {
            app.mode = Mode::Insert;
            app.current_buffer_mut().clear_secondary_cursors();
        }
        KeyCode::Char('a') => {
            app.current_buffer_mut().move_right();
            app.mode = Mode::Insert;
        }
        KeyCode::Char('A') => {
            app.current_buffer_mut().move_end_of_line();
            app.mode = Mode::Insert;
        }
        KeyCode::Char('I') => {
            app.current_buffer_mut().move_first_non_blank();
            app.mode = Mode::Insert;
        }
        KeyCode::Char('o') => {
            let tab_w = app.config.tab_width;
            app.current_buffer_mut().move_end_of_line();
            app.current_buffer_mut().insert_newline_with_tab_width(tab_w);
            app.mode = Mode::Insert;
        }
        KeyCode::Char('O') => {
            app.current_buffer_mut().move_start_of_line();
            app.current_buffer_mut().record_snapshot();
            let indent: String = app.current_buffer().lines[app.current_buffer().cursor.line]
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect();
            let cur_line = app.current_buffer().cursor.line;
            app.current_buffer_mut().lines.insert(cur_line, indent.clone());
            app.current_buffer_mut().cursor.col = indent.chars().count();
            app.mode = Mode::Insert;
        }
        KeyCode::Char('v') => {
            let cursor = app.current_buffer().cursor;
            app.current_buffer_mut().selection = Some(crate::buffer::Selection {
                start_line: cursor.line,
                start_col: cursor.col,
                end_line: cursor.line,
                end_col: cursor.col,
            });
            app.mode = Mode::Visual;
        }
        KeyCode::Char('h') | KeyCode::Left => {
            for _ in 0..count {
                app.current_buffer_mut().move_left();
            }
        }
        KeyCode::Char('l') | KeyCode::Right => {
            for _ in 0..count {
                app.current_buffer_mut().move_right();
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            for _ in 0..count {
                app.current_buffer_mut().move_up();
            }
        }
        KeyCode::Char('j') | KeyCode::Down => {
            for _ in 0..count {
                app.current_buffer_mut().move_down();
            }
        }
        KeyCode::Char('w') => {
            for _ in 0..count {
                app.current_buffer_mut().move_word_next();
            }
        }
        KeyCode::Char('b') => {
            for _ in 0..count {
                app.current_buffer_mut().move_word_prev();
            }
        }
        KeyCode::Char('e') => {
            for _ in 0..count {
                app.current_buffer_mut().move_word_end();
            }
        }
        KeyCode::Home | KeyCode::Char('0') => app.current_buffer_mut().move_start_of_line(),
        KeyCode::Char('^') => app.current_buffer_mut().move_first_non_blank(),
        KeyCode::End | KeyCode::Char('$') => app.current_buffer_mut().move_end_of_line(),
        KeyCode::PageUp => app.current_buffer_mut().page_up(15 * count),
        KeyCode::PageDown => app.current_buffer_mut().page_down(15 * count),
        KeyCode::Char('G') => {
            if count > 1 {
                app.current_buffer_mut().goto_line(count.saturating_sub(1));
            } else {
                app.current_buffer_mut().move_bottom_of_file();
            }
        }
        KeyCode::Char('g') => {
            if app.pending_operator == Some('g') {
                if count > 1 {
                    app.current_buffer_mut().goto_line(count.saturating_sub(1));
                } else {
                    app.current_buffer_mut().move_top_of_file();
                }
                app.pending_operator = None;
            } else {
                app.pending_operator = Some('g');
            }
        }
        KeyCode::Char('u') => {
            for _ in 0..count {
                app.current_buffer_mut().undo();
            }
        }
        KeyCode::Char('x') => {
            for _ in 0..count {
                app.current_buffer_mut().delete_char();
            }
        }
        KeyCode::Char('p') => {
            if !app.clipboard.is_empty() {
                let clip = app.clipboard.clone();
                for _ in 0..count {
                    app.current_buffer_mut().paste(&clip);
                }
            }
        }
        KeyCode::Char('P') => {
            if !app.clipboard.is_empty() {
                let clip = app.clipboard.clone();
                for _ in 0..count {
                    app.current_buffer_mut().paste_before(&clip);
                }
            }
        }
        KeyCode::Char('J') => app.current_buffer_mut().join_lines(),
        KeyCode::Char('~') => app.current_buffer_mut().toggle_case(),
        KeyCode::Char('r') => app.pending_operator = Some('r'),
        KeyCode::Char('d') => app.pending_operator = Some('d'),
        KeyCode::Char('c') => app.pending_operator = Some('c'),
        KeyCode::Char('y') => app.pending_operator = Some('y'),
        KeyCode::Char('f') => app.pending_operator = Some('f'),
        KeyCode::Char('F') => app.pending_operator = Some('F'),
        KeyCode::Char('t') => app.pending_operator = Some('t'),
        KeyCode::Char('T') => app.pending_operator = Some('T'),
        KeyCode::Char('%') => {
            if let Some((l, c)) = app.current_buffer().matching_bracket_pos() {
                app.current_buffer_mut().cursor.line = l;
                app.current_buffer_mut().cursor.col = c;
                app.current_buffer_mut().cursor.desired_col = c;
            }
        }
        KeyCode::Char('K') => {
            if let Some(word) = app.current_buffer().get_word_under_cursor() {
                app.hover_content = Some(format!("Documentation for `{}`\nAdesh standard library symbol.", word));
                app.modal = Modal::HoverTooltip;
            }
        }
        KeyCode::Esc => {
            app.current_buffer_mut().clear_secondary_cursors();
            app.current_buffer_mut().selection = None;
            app.count_prefix = None;
            app.pending_operator = None;
        }
        _ => {}
    }
}

async fn handle_insert_mode(app: &mut App, key: KeyEvent) {
    let tab_width = app.config.tab_width;
    let auto_close = app.auto_close_brackets;

    if is_ctrl_backspace(&key) {
        app.current_buffer_mut().backspace_word();
        return;
    }

    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            app.completion.dismiss();
            app.current_buffer_mut().move_left();
        }
        KeyCode::Char(c) => {
            if auto_close {
                app.current_buffer_mut().insert_char_auto_close(c);
            } else {
                app.current_buffer_mut().insert_char(c);
            }
        }
        KeyCode::Enter => {
            app.current_buffer_mut().insert_newline_with_tab_width(tab_width);
        }
        KeyCode::Backspace => app.current_buffer_mut().backspace(),
        KeyCode::Delete => app.current_buffer_mut().delete_char(),
        KeyCode::Tab => app.current_buffer_mut().indent(tab_width),
        KeyCode::BackTab => app.current_buffer_mut().unindent(tab_width),
        KeyCode::Left => app.current_buffer_mut().move_left(),
        KeyCode::Right => app.current_buffer_mut().move_right(),
        KeyCode::Up => app.current_buffer_mut().move_up(),
        KeyCode::Down => app.current_buffer_mut().move_down(),
        KeyCode::Home => app.current_buffer_mut().move_start_of_line(),
        KeyCode::End => app.current_buffer_mut().move_end_of_line(),
        KeyCode::PageUp => app.current_buffer_mut().page_up(15),
        KeyCode::PageDown => app.current_buffer_mut().page_down(15),
        _ => {}
    }
}

async fn handle_visual_mode(app: &mut App, key: KeyEvent) {
    let tab_width = app.config.tab_width;

    if is_ctrl_backspace(&key) {
        app.current_buffer_mut().delete_selection_if_any();
        app.mode = Mode::Normal;
        return;
    }

    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            app.current_buffer_mut().selection = None;
        }
        KeyCode::Char('y') => {
            if let Some(text) = app.current_buffer().get_selected_text() {
                app.clipboard = text;
                app.toast("Yanked selection", "INFO");
            }
            app.mode = Mode::Normal;
            app.current_buffer_mut().selection = None;
        }
        KeyCode::Char('d') | KeyCode::Char('x') => {
            if let Some(text) = app.current_buffer_mut().cut_selection() {
                app.clipboard = text;
                app.toast("Cut selection", "INFO");
            }
            app.mode = Mode::Normal;
        }
        KeyCode::Char('c') => {
            app.current_buffer_mut().delete_selection_if_any();
            app.mode = Mode::Insert;
        }
        KeyCode::Char('>') => {
            app.current_buffer_mut().indent(tab_width);
            app.mode = Mode::Normal;
        }
        KeyCode::Char('<') => {
            app.current_buffer_mut().unindent(tab_width);
            app.mode = Mode::Normal;
        }
        KeyCode::Char('U') => {
            app.current_buffer_mut().transform_selection_case(true);
            app.mode = Mode::Normal;
        }
        KeyCode::Char('u') => {
            app.current_buffer_mut().transform_selection_case(false);
            app.mode = Mode::Normal;
        }
        KeyCode::Char('(') | KeyCode::Char('[') | KeyCode::Char('{') | KeyCode::Char('"') | KeyCode::Char('\'') | KeyCode::Char('`') => {
            if let KeyCode::Char(c) = key.code {
                app.current_buffer_mut().insert_char_auto_close(c);
                app.mode = Mode::Normal;
            }
        }
        KeyCode::Char('h') | KeyCode::Left => {
            app.current_buffer_mut().move_left();
            update_visual_selection(app);
        }
        KeyCode::Char('l') | KeyCode::Right => {
            app.current_buffer_mut().move_right();
            update_visual_selection(app);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.current_buffer_mut().move_up();
            update_visual_selection(app);
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.current_buffer_mut().move_down();
            update_visual_selection(app);
        }
        KeyCode::Char('w') => {
            app.current_buffer_mut().move_word_next();
            update_visual_selection(app);
        }
        KeyCode::Char('b') => {
            app.current_buffer_mut().move_word_prev();
            update_visual_selection(app);
        }
        KeyCode::Home | KeyCode::Char('0') => {
            app.current_buffer_mut().move_start_of_line();
            update_visual_selection(app);
        }
        KeyCode::Char('^') => {
            app.current_buffer_mut().move_first_non_blank();
            update_visual_selection(app);
        }
        KeyCode::End | KeyCode::Char('$') => {
            app.current_buffer_mut().move_end_of_line();
            update_visual_selection(app);
        }
        KeyCode::PageUp => {
            app.current_buffer_mut().page_up(15);
            update_visual_selection(app);
        }
        KeyCode::PageDown => {
            app.current_buffer_mut().page_down(15);
            update_visual_selection(app);
        }
        _ => {}
    }
}

fn update_visual_selection(app: &mut App) {
    let cursor = app.current_buffer().cursor;
    if let Some(ref mut sel) = app.current_buffer_mut().selection {
        sel.end_line = cursor.line;
        sel.end_col = cursor.col;
    }
}

async fn handle_command_line_input(app: &mut App, key: KeyEvent) {
    if is_ctrl_backspace(&key) {
        pop_word_from_string(&mut app.command_line);
        if app.command_line.is_empty() {
            app.show_command_line = false;
        }
        return;
    }

    match key.code {
        KeyCode::Esc => {
            app.show_command_line = false;
            app.command_line.clear();
        }
        KeyCode::Enter => app.execute_command_line(),
        KeyCode::Backspace => {
            app.command_line.pop();
            if app.command_line.is_empty() {
                app.show_command_line = false;
            }
        }
        KeyCode::Char(c) => app.command_line.push(c),
        _ => {}
    }
}

async fn handle_modal_input(app: &mut App, key: KeyEvent) {
    match app.modal {
        Modal::GitManager => match key.code {
            KeyCode::Esc => app.modal = Modal::None,
            KeyCode::Down | KeyCode::Char('j') => {
                let total = app.git.staged_files.len() + app.git.unstaged_files.len() + app.git.untracked_files.len();
                if total > 0 {
                    app.git_selected = (app.git_selected + 1) % total;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let total = app.git.staged_files.len() + app.git.unstaged_files.len() + app.git.untracked_files.len();
                if total > 0 {
                    if app.git_selected == 0 {
                        app.git_selected = total - 1;
                    } else {
                        app.git_selected -= 1;
                    }
                }
            }
            KeyCode::Char('s') => {
                // Stage selected
                if let Some(path) = get_git_selected_path(app) {
                    let _ = app.git.stage_file(&path);
                    app.toast(&format!("Staged {}", path.display()), "SUCCESS");
                }
            }
            KeyCode::Char('u') => {
                // Unstage selected
                if let Some(path) = get_git_selected_path(app) {
                    let _ = app.git.unstage_file(&path);
                    app.toast(&format!("Unstaged {}", path.display()), "INFO");
                }
            }
            KeyCode::Char('a') => {
                match app.git.stage_all() {
                    Ok(()) => app.toast("Staged all files", "SUCCESS"),
                    Err(e) => app.toast(&e, "ERROR"),
                }
            }
            KeyCode::Char('c') => {
                app.prompt_input.clear();
                app.modal = Modal::GitCommitPrompt;
            }
            KeyCode::Char('P') => {
                match app.git.push() {
                    Ok(m) => app.toast(&m, "SUCCESS"),
                    Err(e) => app.toast(&e, "ERROR"),
                }
            }
            KeyCode::Char('p') => {
                match app.git.pull() {
                    Ok(m) => app.toast(&m, "SUCCESS"),
                    Err(e) => app.toast(&e, "ERROR"),
                }
            }
            KeyCode::Char('d') => {
                if let Some(path) = get_git_selected_path(app) {
                    app.git.load_diff(Some(&path));
                    app.modal = Modal::GitDiff;
                } else {
                    app.git.load_diff(None);
                    app.modal = Modal::GitDiff;
                }
            }
            KeyCode::Char('l') => {
                app.git.load_commit_history(30);
                app.git_log_selected = 0;
                app.modal = Modal::GitLog;
            }
            KeyCode::Char('b') => {
                app.modal = Modal::GitBranchSelector;
            }
            _ => {}
        },
        Modal::GitCommitPrompt => {
            if is_ctrl_backspace(&key) {
                pop_word_from_string(&mut app.prompt_input);
                return;
            }
            match key.code {
                KeyCode::Esc => app.modal = Modal::GitManager,
                KeyCode::Enter => {
                    let msg = app.prompt_input.trim().to_string();
                    if !msg.is_empty() {
                        match app.git.commit(&msg) {
                            Ok(_res) => {
                                app.toast("Committed successfully", "SUCCESS");
                                app.modal = Modal::GitManager;
                            }
                            Err(e) => app.toast(&e, "ERROR"),
                        }
                    }
                }
                KeyCode::Backspace => {
                    app.prompt_input.pop();
                }
                KeyCode::Char(c) => app.prompt_input.push(c),
                _ => {}
            }
        }
        Modal::GitLog => match key.code {
            KeyCode::Esc => app.modal = Modal::None,
            KeyCode::Down | KeyCode::Char('j') => {
                if !app.git.commit_history.is_empty() {
                    app.git_log_selected = (app.git_log_selected + 1) % app.git.commit_history.len();
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if !app.git.commit_history.is_empty() {
                    if app.git_log_selected == 0 {
                        app.git_log_selected = app.git.commit_history.len() - 1;
                    } else {
                        app.git_log_selected -= 1;
                    }
                }
            }
            _ => {}
        },
        Modal::FuzzyFileFinder => {
            if is_ctrl_backspace(&key) {
                pop_word_from_string(&mut app.prompt_input);
                app.populate_fuzzy_files();
                return;
            }
            match key.code {
                KeyCode::Esc => app.modal = Modal::None,
                KeyCode::Down => {
                    if !app.fuzzy_files.is_empty() {
                        app.fuzzy_selected = (app.fuzzy_selected + 1) % app.fuzzy_files.len();
                    }
                }
                KeyCode::Up => {
                    if !app.fuzzy_files.is_empty() {
                        if app.fuzzy_selected == 0 {
                            app.fuzzy_selected = app.fuzzy_files.len() - 1;
                        } else {
                            app.fuzzy_selected -= 1;
                        }
                    }
                }
                KeyCode::Enter => {
                    if let Some((_, path, _)) = app.fuzzy_files.get(app.fuzzy_selected).cloned() {
                        app.modal = Modal::None;
                        let _ = app.open_file(&path);
                    }
                }
                KeyCode::Backspace => {
                    app.prompt_input.pop();
                    app.populate_fuzzy_files();
                }
                KeyCode::Char(c) => {
                    app.prompt_input.push(c);
                    app.populate_fuzzy_files();
                }
                _ => {}
            }
        }
        Modal::WorkspaceGrep => {
            if is_ctrl_backspace(&key) {
                pop_word_from_string(&mut app.grep_query);
                app.run_workspace_grep();
                return;
            }
            match key.code {
                KeyCode::Esc => app.modal = Modal::None,
                KeyCode::Down => {
                    if !app.grep_results.is_empty() {
                        app.grep_selected = (app.grep_selected + 1) % app.grep_results.len();
                    }
                }
                KeyCode::Up => {
                    if !app.grep_results.is_empty() {
                        if app.grep_selected == 0 {
                            app.grep_selected = app.grep_results.len() - 1;
                        } else {
                            app.grep_selected -= 1;
                        }
                    }
                }
                KeyCode::Enter => {
                    if let Some((path, line_num, _)) = app.grep_results.get(app.grep_selected).cloned() {
                        app.modal = Modal::None;
                        if app.open_file(&path).is_ok() {
                            app.current_buffer_mut().goto_line(line_num);
                        }
                    }
                }
                KeyCode::Backspace => {
                    app.grep_query.pop();
                    app.run_workspace_grep();
                }
                KeyCode::Char(c) => {
                    app.grep_query.push(c);
                    app.run_workspace_grep();
                }
                _ => {}
            }
        }
        Modal::CommandPalette => {
            if is_ctrl_backspace(&key) {
                let mut q = app.palette.query.clone();
                pop_word_from_string(&mut q);
                app.palette.update_query(q);
                return;
            }
            match key.code {
                KeyCode::Esc => app.modal = Modal::None,
                KeyCode::Down => app.palette.select_next(),
                KeyCode::Up => app.palette.select_prev(),
                KeyCode::Enter => {
                    if let Some(cmd) = app.palette.selected_item() {
                        app.execute_palette_command(cmd);
                    }
                }
                KeyCode::Backspace => {
                    let mut q = app.palette.query.clone();
                    q.pop();
                    app.palette.update_query(q);
                }
                KeyCode::Char(c) => {
                    let mut q = app.palette.query.clone();
                    q.push(c);
                    app.palette.update_query(q);
                }
                _ => {}
            }
        }
        Modal::Search | Modal::Replace => {
            if is_ctrl_backspace(&key) {
                pop_word_from_string(&mut app.search.query);
                let lines = app.current_buffer().lines.clone();
                app.search.update_matches(&lines);
                return;
            }
            match key.code {
                KeyCode::Esc => app.modal = Modal::None,
                KeyCode::Enter => {
                    if let Some((l, c)) = app.search.next_match() {
                        app.current_buffer_mut().cursor.line = l;
                        app.current_buffer_mut().cursor.col = c;
                        app.current_buffer_mut().cursor.desired_col = c;
                    }
                }
                KeyCode::Backspace => {
                    app.search.query.pop();
                    let lines = app.current_buffer().lines.clone();
                    app.search.update_matches(&lines);
                }
                KeyCode::Char(c) => {
                    app.search.query.push(c);
                    let lines = app.current_buffer().lines.clone();
                    app.search.update_matches(&lines);
                }
                _ => {}
            }
        }
        Modal::SelectBackend => match key.code {
            KeyCode::Esc => app.modal = Modal::None,
            KeyCode::Down => {
                let all = Backend::all();
                let idx = all.iter().position(|b| b == &app.active_backend).unwrap_or(0);
                app.active_backend = all[(idx + 1) % all.len()].clone();
            }
            KeyCode::Up => {
                let all = Backend::all();
                let idx = all.iter().position(|b| b == &app.active_backend).unwrap_or(0);
                let next = if idx == 0 { all.len() - 1 } else { idx - 1 };
                app.active_backend = all[next].clone();
            }
            KeyCode::Enter => {
                let name = app.active_backend.name().to_string();
                app.modal = Modal::None;
                app.toast(&format!("Backend selected: {}", name), "SUCCESS");
            }
            _ => {}
        },
        Modal::ThemeSelector => match key.code {
            KeyCode::Esc => app.modal = Modal::None,
            KeyCode::Down => app.cycle_theme(),
            KeyCode::Enter => app.modal = Modal::None,
            _ => {}
        },
        Modal::NewFilePrompt => {
            if is_ctrl_backspace(&key) {
                pop_word_from_string(&mut app.prompt_input);
                return;
            }
            match key.code {
                KeyCode::Esc => app.modal = Modal::None,
                KeyCode::Enter => {
                    let name = app.prompt_input.trim().to_string();
                    app.modal = Modal::None;
                    if !name.is_empty() {
                        if let Ok(p) = app.explorer.create_file(&name) {
                            let _ = app.open_file(&p);
                        }
                    }
                }
                KeyCode::Backspace => {
                    app.prompt_input.pop();
                }
                KeyCode::Char(c) => app.prompt_input.push(c),
                _ => {}
            }
        }
        Modal::NewDirPrompt => {
            if is_ctrl_backspace(&key) {
                pop_word_from_string(&mut app.prompt_input);
                return;
            }
            match key.code {
                KeyCode::Esc => app.modal = Modal::None,
                KeyCode::Enter => {
                    let name = app.prompt_input.trim().to_string();
                    app.modal = Modal::None;
                    if !name.is_empty() {
                        if let Ok(_p) = app.explorer.create_dir(&name) {
                            app.toast(&format!("Created directory: {}", name), "SUCCESS");
                        }
                    }
                }
                KeyCode::Backspace => {
                    app.prompt_input.pop();
                }
                KeyCode::Char(c) => app.prompt_input.push(c),
                _ => {}
            }
        }
        Modal::RenameFilePrompt => {
            if is_ctrl_backspace(&key) {
                pop_word_from_string(&mut app.prompt_input);
                return;
            }
            match key.code {
                KeyCode::Esc => app.modal = Modal::None,
                KeyCode::Enter => {
                    let name = app.prompt_input.trim().to_string();
                    app.modal = Modal::None;
                    if !name.is_empty() {
                        if let Ok(new_path) = app.explorer.rename_file(&name) {
                            app.toast(&format!("Renamed to: {}", new_path.file_name().unwrap_or_default().to_string_lossy()), "SUCCESS");
                        }
                    }
                }
                KeyCode::Backspace => {
                    app.prompt_input.pop();
                }
                KeyCode::Char(c) => app.prompt_input.push(c),
                _ => {}
            }
        }
        Modal::OpenFilePrompt => {
            if is_ctrl_backspace(&key) {
                pop_word_from_string(&mut app.prompt_input);
                return;
            }
            match key.code {
                KeyCode::Esc => app.modal = Modal::None,
                KeyCode::Enter => {
                    let name = app.prompt_input.trim().to_string();
                    app.modal = Modal::None;
                    if !name.is_empty() {
                        let _ = app.open_file(name);
                    }
                }
                KeyCode::Backspace => {
                    app.prompt_input.pop();
                }
                KeyCode::Char(c) => app.prompt_input.push(c),
                _ => {}
            }
        }
        Modal::GotoLinePrompt => {
            if is_ctrl_backspace(&key) {
                pop_word_from_string(&mut app.prompt_input);
                return;
            }
            match key.code {
                KeyCode::Esc => app.modal = Modal::None,
                KeyCode::Enter => {
                    let line_str = app.prompt_input.trim().to_string();
                    app.modal = Modal::None;
                    if let Ok(n) = line_str.parse::<usize>() {
                        app.current_buffer_mut().goto_line(n);
                    }
                }
                KeyCode::Backspace => {
                    app.prompt_input.pop();
                }
                KeyCode::Char(c) => app.prompt_input.push(c),
                _ => {}
            }
        }
        Modal::SaveAsPrompt => {
            if is_ctrl_backspace(&key) {
                pop_word_from_string(&mut app.prompt_input);
                return;
            }
            match key.code {
                KeyCode::Esc => app.modal = Modal::None,
                KeyCode::Enter => {
                    let path = app.prompt_input.trim().to_string();
                    app.modal = Modal::None;
                    if !path.is_empty() {
                        app.save_as_current_buffer(&path);
                    }
                }
                KeyCode::Backspace => {
                    app.prompt_input.pop();
                }
                KeyCode::Char(c) => app.prompt_input.push(c),
                _ => {}
            }
        }
        Modal::CloseUnsavedConfirm(idx) => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                app.modal = Modal::None;
                app.save_current_buffer();
                app.force_close_buffer(idx);
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                app.modal = Modal::None;
                app.force_close_buffer(idx);
            }
            KeyCode::Esc => app.modal = Modal::None,
            _ => {}
        },
        Modal::AstViewer
        | Modal::BytecodeViewer
        | Modal::HirViewer
        | Modal::IrViewer
        | Modal::LirViewer
        | Modal::MlirViewer
        | Modal::TokensViewer
        | Modal::AdeshDocViewer
        | Modal::AdeshCheckViewer
        | Modal::GitDiff
        | Modal::KeybindingsHelp
        | Modal::InfoPage => match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {
                app.modal = Modal::None;
                app.text_viewer_scroll = 0;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.text_viewer_scroll = app.text_viewer_scroll.saturating_add(1);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.text_viewer_scroll = app.text_viewer_scroll.saturating_sub(1);
            }
            KeyCode::PageDown | KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) || key.code == KeyCode::PageDown => {
                app.text_viewer_scroll = app.text_viewer_scroll.saturating_add(15);
            }
            KeyCode::PageUp | KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) || key.code == KeyCode::PageUp => {
                app.text_viewer_scroll = app.text_viewer_scroll.saturating_sub(15);
            }
            KeyCode::Home => {
                app.text_viewer_scroll = 0;
            }
            KeyCode::End => {
                app.text_viewer_scroll = 99999;
            }
            _ => {}
        },
        _ => {
            if key.code == KeyCode::Esc || key.code == KeyCode::Enter {
                app.modal = Modal::None;
            }
        }
    }
}

fn get_git_selected_path(app: &App) -> Option<PathBuf> {
    let mut idx = app.git_selected;
    if idx < app.git.staged_files.len() {
        return Some(app.git.staged_files[idx].0.clone());
    }
    idx -= app.git.staged_files.len();
    if idx < app.git.unstaged_files.len() {
        return Some(app.git.unstaged_files[idx].0.clone());
    }
    idx -= app.git.unstaged_files.len();
    if idx < app.git.untracked_files.len() {
        return Some(app.git.untracked_files[idx].clone());
    }
    None
}

async fn handle_menu_key(app: &mut App, key: KeyEvent) {
    let tabs = MenuTab::all();
    let cur_tab_idx = tabs.iter().position(|(_, t)| *t == app.active_menu).unwrap_or(0);

    match key.code {
        KeyCode::Esc => {
            app.active_menu = MenuTab::None;
            app.menu_selected = 0;
        }
        KeyCode::Left => {
            let next_tab_idx = if cur_tab_idx == 0 { tabs.len() - 1 } else { cur_tab_idx - 1 };
            app.active_menu = tabs[next_tab_idx].1;
            app.menu_selected = 0;
        }
        KeyCode::Right => {
            let next_tab_idx = (cur_tab_idx + 1) % tabs.len();
            app.active_menu = tabs[next_tab_idx].1;
            app.menu_selected = 0;
        }
        KeyCode::Down => {
            let items = app.active_menu.items();
            if !items.is_empty() {
                app.menu_selected = (app.menu_selected + 1) % items.len();
            }
        }
        KeyCode::Up => {
            let items = app.active_menu.items();
            if !items.is_empty() {
                if app.menu_selected == 0 {
                    app.menu_selected = items.len() - 1;
                } else {
                    app.menu_selected -= 1;
                }
            }
        }
        KeyCode::Enter => {
            let tab = app.active_menu;
            let sel = app.menu_selected;
            app.execute_menu_item(tab, sel);
        }
        _ => {}
    }
}

fn handle_terminal_input(app: &mut App, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Char('J') | KeyCode::Char('\n') | KeyCode::Char('t') | KeyCode::Char('T') => {
                app.show_terminal = false;
                app.terminal_focused = false;
                return;
            }
            KeyCode::Char('`') | KeyCode::Char('~') | KeyCode::Char('@') => {
                app.show_terminal = false;
                app.terminal_focused = false;
                app.show_output = !app.show_output;
                return;
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                if app.terminal.is_running {
                    app.terminal.stop();
                } else {
                    app.terminal.input.clear();
                }
                app.terminal_scroll = 0;
                return;
            }
            KeyCode::Char('l') | KeyCode::Char('L') => {
                if let Ok(mut o) = app.terminal.output.lock() {
                    o.clear();
                    o.push("Adesh Editor — Integrated Terminal".to_string());
                }
                app.terminal_scroll = 0;
                return;
            }
            KeyCode::Char('u') | KeyCode::Char('U') => {
                app.terminal_scroll = app.terminal_scroll.saturating_add(10);
                return;
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                app.terminal_scroll = app.terminal_scroll.saturating_sub(10);
                return;
            }
            _ => {}
        }
    }

    if is_ctrl_backspace(&key) {
        pop_word_from_string(&mut app.terminal.input);
        app.terminal_scroll = 0;
        return;
    }

    match key.code {
        KeyCode::Esc => {
            app.terminal_focused = false;
            app.show_terminal = false;
        }
        KeyCode::PageUp => {
            app.terminal_scroll = app.terminal_scroll.saturating_add(10);
        }
        KeyCode::PageDown => {
            app.terminal_scroll = app.terminal_scroll.saturating_sub(10);
        }
        KeyCode::Enter => {
            app.terminal_scroll = 0;
            app.terminal.execute_line();
        }
        KeyCode::Backspace => {
            app.terminal_scroll = 0;
            app.terminal.backspace();
        }
        KeyCode::Up => {
            app.terminal_scroll = 0;
            app.terminal.prev_history();
        }
        KeyCode::Down => {
            app.terminal_scroll = 0;
            app.terminal.next_history();
        }
        KeyCode::Char(c) => {
            app.terminal_scroll = 0;
            app.terminal.input_char(c);
        }
        _ => {}
    }
}

async fn handle_explorer_input(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.explorer_focused = false;
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.explorer.select_next();
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.explorer.select_prev();
        }
        KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('o') | KeyCode::Right | KeyCode::Char('l') => {
            if let Some(path) = app.explorer.selected_path().cloned() {
                if path.is_dir() {
                    app.explorer.toggle_expand();
                } else {
                    let _ = app.open_file(&path);
                    app.explorer_focused = false;
                }
            }
        }
        KeyCode::Left | KeyCode::Char('h') => {
            if let Some(path) = app.explorer.selected_path().cloned() {
                if path.is_dir() && app.explorer.root_node.find(&path).map(|n| n.expanded).unwrap_or(false) {
                    app.explorer.toggle_expand();
                } else if let Some(parent) = path.parent() {
                    if let Some(pos) = app.explorer.flat_nodes.iter().position(|(p, _)| p == parent) {
                        app.explorer.selected_index = pos;
                        app.explorer.adjust_scroll();
                    }
                }
            }
        }
        KeyCode::Char('r') => {
            app.explorer.refresh();
            app.toast("Refreshed file explorer", "INFO");
        }
        KeyCode::Char('a') => {
            app.prompt_input.clear();
            app.modal = Modal::NewFilePrompt;
        }
        KeyCode::Char('A') => {
            app.prompt_input.clear();
            app.modal = Modal::NewDirPrompt;
        }
        KeyCode::Char('d') => {
            if let Some(path) = app.explorer.selected_path().cloned() {
                if let Err(_e) = std::fs::remove_file(&path) {
                    let _ = std::fs::remove_dir_all(&path);
                    app.toast(&format!("Deleted {}", path.display()), "WARN");
                } else {
                    app.toast(&format!("Deleted {}", path.display()), "WARN");
                }
                app.explorer.refresh();
            }
        }
        KeyCode::Char('y') => {
            if let Some(path) = app.explorer.selected_path().cloned() {
                app.copy_file_to_clipboard(path);
            }
        }
        KeyCode::Char('x') => {
            if let Some(path) = app.explorer.selected_path().cloned() {
                app.cut_file_to_clipboard(path);
            }
        }
        KeyCode::Char('p') => {
            if let Some(path) = app.explorer.selected_path().cloned() {
                let target_dir = if path.is_dir() { path } else { path.parent().unwrap_or(std::path::Path::new(".")).to_path_buf() };
                app.paste_file_clipboard(&target_dir);
            }
        }
        _ => {}
    }
}

pub async fn handle_mouse_event(app: &mut App, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Right) => {
            // Right-click in File Explorer opens the context menu
            if app.show_explorer && mouse.column < 28 && mouse.row >= 3 {
                let click_row = (mouse.row.saturating_sub(3)) as usize;
                let target_idx = app.explorer.scroll_offset + click_row;
                if target_idx < app.explorer.flat_nodes.len() {
                    app.explorer.selected_index = target_idx;
                    let (path, _) = app.explorer.flat_nodes[target_idx].clone();
                    let is_dir = path.is_dir();
                    app.explorer_context_menu = Some(crate::app::ExplorerContextMenu {
                        target_path: path,
                        is_dir,
                        x: mouse.column.min(55),
                        y: mouse.row.min(22),
                        selected_index: 0,
                    });
                    return;
                }
            }
        }
        MouseEventKind::Down(MouseButton::Left) => {
            // 1. Check clicks on Explorer Context Menu
            if let Some(ctx) = app.explorer_context_menu.clone() {
                let menu_w = 22u16;
                let menu_h = 10u16;
                let menu_x = ctx.x;
                let menu_y = ctx.y;

                if mouse.column >= menu_x
                    && mouse.column < menu_x + menu_w
                    && mouse.row > menu_y
                    && mouse.row < menu_y + menu_h - 1
                {
                    let item_idx = (mouse.row - (menu_y + 1)) as usize;
                    app.execute_context_menu_action(item_idx);
                    return;
                } else {
                    app.explorer_context_menu = None;
                    return;
                }
            }

            // 2. Check clicks on toast notification close buttons / popups
            for &(tx, ty, tw, th, idx) in &app.toast_close_positions {
                if mouse.column >= tx && mouse.column < tx + tw && mouse.row >= ty && mouse.row < ty + th {
                    app.remove_toast(idx);
                    return;
                }
            }

            // 3. If a top dropdown menu is currently open:
            if app.active_menu != MenuTab::None {
                // Check if clicked inside dropdown box
                if let Some((dx, dy, dw, dh)) = app.menu_dropdown_rect {
                    if mouse.column >= dx && mouse.column < dx + dw && mouse.row > dy && mouse.row < dy + dh - 1 {
                        let item_idx = (mouse.row - (dy + 1)) as usize;
                        let items = app.active_menu.items();
                        if item_idx < items.len() {
                            let tab = app.active_menu;
                            app.active_menu = MenuTab::None;
                            app.menu_dropdown_rect = None;
                            app.execute_menu_item(tab, item_idx);
                            return;
                        }
                    }
                }

                // Check if clicked on top menu bar (Row 0)
                if mouse.row == 0 {
                    // Check Run button
                    if app.run_button_w > 0 && mouse.column >= app.run_button_x && mouse.column < app.run_button_x + app.run_button_w {
                        app.active_menu = MenuTab::None;
                        app.menu_dropdown_rect = None;
                        app.execute_run();
                        return;
                    }

                    // Check menu tabs
                    let tabs = MenuTab::all();
                    for (idx, (_, tab)) in tabs.iter().enumerate() {
                        let (x_start, width) = MenuTab::tab_rect(idx);
                        if mouse.column >= x_start && mouse.column < x_start + width {
                            if app.active_menu == *tab {
                                app.active_menu = MenuTab::None;
                                app.menu_dropdown_rect = None;
                            } else {
                                app.active_menu = *tab;
                                app.menu_selected = 0;
                            }
                            return;
                        }
                    }
                }

                // Clicked outside both dropdown and menu bar -> close menu!
                app.active_menu = MenuTab::None;
                app.menu_dropdown_rect = None;
                return;
            }

            // 4. Normal Row 0 clicks (when menu was not open)
            if mouse.row == 0 {
                // Check if Run Button was clicked
                if app.run_button_w > 0 && mouse.column >= app.run_button_x && mouse.column < app.run_button_x + app.run_button_w {
                    app.execute_run();
                    return;
                }

                // Check menu tabs
                let tabs = MenuTab::all();
                for (idx, (_, tab)) in tabs.iter().enumerate() {
                    let (x_start, width) = MenuTab::tab_rect(idx);
                    if mouse.column >= x_start && mouse.column < x_start + width {
                        app.active_menu = *tab;
                        app.menu_selected = 0;
                        return;
                    }
                }
            }

            // 5. Check clicks on bottom panel drag border / terminal focus
            if (app.show_terminal || app.show_output) && app.bottom_panel_drag_y > 0 {
                if mouse.row == app.bottom_panel_drag_y {
                    app.is_dragging_bottom_panel = true;
                    return;
                } else if app.show_terminal && mouse.row > app.bottom_panel_drag_y {
                    app.terminal_focused = true;
                    app.explorer_focused = false;
                    return;
                }
            }

            // 6. Row 1: Buffer tabs
            if mouse.row == 1 {
                for &(start_x, close_btn_start_x, close_btn_end_x, buf_idx) in &app.tab_close_positions {
                    if mouse.column >= start_x && mouse.column < close_btn_start_x {
                        // Clicked on tab body -> switch active buffer
                        app.active_buffer = buf_idx;
                        if let Some(pane) = app.split_tree.active_pane_mut() {
                            pane.buffer_index = buf_idx;
                        }
                        return;
                    } else if mouse.column >= close_btn_start_x && mouse.column <= close_btn_end_x {
                        // Clicked on close button [✕] -> close buffer (or prompt unsaved confirm)
                        app.close_buffer(buf_idx);
                        return;
                    }
                }
            }

            // 7. Row >= 2: Workspace clicks (Explorer or Editor Text)
            if mouse.row >= 2 {
                if app.show_explorer && mouse.column < 28 {
                    app.explorer_focused = true;
                    // Row 0: Menu, Row 1: Tabs, Row 2: Explorer top border ╭───...───╮
                    // Content items start at row 3
                    if mouse.row >= 3 {
                        let click_row = (mouse.row.saturating_sub(3)) as usize;
                        let target_idx = app.explorer.scroll_offset + click_row;
                        if target_idx < app.explorer.flat_nodes.len() {
                            app.explorer.selected_index = target_idx;
                            let (path, _) = app.explorer.flat_nodes[target_idx].clone();
                            if path.is_dir() {
                                app.explorer.toggle_expand();
                            } else {
                                let _ = app.open_file(&path);
                                app.explorer_focused = false;
                            }
                        }
                    }
                    return;
                } else {
                    app.explorer_focused = false;

                    // Position cursor in clicked editor pane
                    let panes = app.pane_positions.clone();
                    for (pane_id, buf_idx, pane_rect) in panes {
                        if mouse.column > pane_rect.x
                            && mouse.column < pane_rect.x + pane_rect.width.saturating_sub(1)
                            && mouse.row > pane_rect.y
                            && mouse.row < pane_rect.y + pane_rect.height.saturating_sub(1)
                        {
                            app.split_tree.active_pane_id = pane_id;
                            app.active_buffer = buf_idx;
                            app.explorer_focused = false;
                            app.terminal_focused = false;

                            if buf_idx < app.buffers.len() {
                                let line_mode = app.line_number_mode;
                                let buf = &mut app.buffers[buf_idx];
                                let row_offset = (mouse.row - (pane_rect.y + 1)) as usize;
                                let target_line = (buf.scroll_top + row_offset).min(buf.lines.len().saturating_sub(1));
                                buf.cursor.line = target_line;

                                let digits = buf.lines.len().to_string().len().max(2);
                                let num_width = match line_mode {
                                    LineNumberMode::None => 0,
                                    _ => digits + 1,
                                };
                                let gutter_width = 1 + num_width; // 1 for diff marker
                                let content_start_x = pane_rect.x + 1 + gutter_width as u16;
                                if mouse.column >= content_start_x {
                                    let col_offset = (mouse.column - content_start_x) as usize;
                                    let target_col = buf.scroll_left + col_offset;
                                    let line_len = buf.lines[target_line].chars().count();
                                    buf.cursor.col = target_col.min(line_len);
                                } else {
                                    buf.cursor.col = 0;
                                }
                                buf.cursor.desired_col = buf.cursor.col;
                                buf.clear_secondary_cursors();
                                buf.selection = None;
                                app.mouse_drag_start = Some((target_line, buf.cursor.col));
                            }
                            return;
                        }
                    }
                }
            }
        }
        MouseEventKind::Moved | MouseEventKind::Drag(MouseButton::Left) => {
            // Drag-to-resize bottom panel (Terminal or Output)
            if app.is_dragging_bottom_panel {
                let bottom_edge = app.bottom_panel_drag_y + app.bottom_panel_height;
                let new_height = bottom_edge.saturating_sub(mouse.row);
                app.bottom_panel_height = new_height.clamp(3, 35);
                return;
            }

            // Drag to select code range in editor pane
            if let Some((start_line, start_col)) = app.mouse_drag_start {
                let panes = app.pane_positions.clone();
                for (_pane_id, buf_idx, pane_rect) in panes {
                    if mouse.column > pane_rect.x
                        && mouse.column < pane_rect.x + pane_rect.width.saturating_sub(1)
                        && mouse.row > pane_rect.y
                        && mouse.row < pane_rect.y + pane_rect.height.saturating_sub(1)
                    {
                        if buf_idx < app.buffers.len() {
                            let line_mode = app.line_number_mode;
                            let buf = &mut app.buffers[buf_idx];
                            let row_offset = (mouse.row - (pane_rect.y + 1)) as usize;
                            let target_line = (buf.scroll_top + row_offset).min(buf.lines.len().saturating_sub(1));

                            let digits = buf.lines.len().to_string().len().max(2);
                            let num_width = match line_mode {
                                LineNumberMode::None => 0,
                                _ => digits + 1,
                            };
                            let gutter_width = 1 + num_width;
                            let content_start_x = pane_rect.x + 1 + gutter_width as u16;
                            let target_col = if mouse.column >= content_start_x {
                                let col_offset = (mouse.column - content_start_x) as usize;
                                let line_len = buf.lines[target_line].chars().count();
                                (buf.scroll_left + col_offset).min(line_len)
                            } else {
                                0
                            };

                            buf.cursor.line = target_line;
                            buf.cursor.col = target_col;
                            buf.cursor.desired_col = target_col;
                            buf.selection = Some(crate::buffer::Selection {
                                start_line,
                                start_col,
                                end_line: target_line,
                                end_col: target_col,
                            });
                        }
                        return;
                    }
                }
            }

            // Mouse traversal for top menu dropdown
            if app.active_menu != MenuTab::None {
                if mouse.row == 0 {
                    let tabs = MenuTab::all();
                    for (idx, (_, tab)) in tabs.iter().enumerate() {
                        let (x_start, width) = MenuTab::tab_rect(idx);
                        if mouse.column >= x_start && mouse.column < x_start + width {
                            if app.active_menu != *tab {
                                app.active_menu = *tab;
                                app.menu_selected = 0;
                            }
                            return;
                        }
                    }
                }

                if let Some((dx, dy, dw, dh)) = app.menu_dropdown_rect {
                    if mouse.column >= dx && mouse.column < dx + dw && mouse.row > dy && mouse.row < dy + dh - 1 {
                        let item_idx = (mouse.row - (dy + 1)) as usize;
                        let items = app.active_menu.items();
                        if item_idx < items.len() {
                            app.menu_selected = item_idx;
                        }
                    }
                }
            }

            // Mouse traversal for explorer context menu
            if let Some(mut ctx) = app.explorer_context_menu.clone() {
                let menu_w = 22u16;
                let menu_h = 10u16;
                if mouse.column >= ctx.x && mouse.column < ctx.x + menu_w && mouse.row > ctx.y && mouse.row < ctx.y + menu_h - 1 {
                    let item_idx = (mouse.row - (ctx.y + 1)) as usize;
                    if item_idx < 8 {
                        ctx.selected_index = item_idx;
                        app.explorer_context_menu = Some(ctx);
                    }
                }
            }
        }
        MouseEventKind::ScrollDown => {
            if app.modal != Modal::None {
                app.text_viewer_scroll = app.text_viewer_scroll.saturating_add(3);
                return;
            }
            if let Some(bottom_rect) = app.bottom_panel_rect {
                if mouse.row >= bottom_rect.y && mouse.row < bottom_rect.y + bottom_rect.height
                    && mouse.column >= bottom_rect.x && mouse.column < bottom_rect.x + bottom_rect.width
                {
                    if app.show_output {
                        app.output_scroll = app.output_scroll.saturating_sub(3);
                        return;
                    } else if app.show_terminal {
                        app.terminal_scroll = app.terminal_scroll.saturating_sub(3);
                        return;
                    }
                }
            }
            if app.show_explorer && mouse.column < 28 {
                app.explorer.scroll_down(3);
            } else {
                app.current_buffer_mut().scroll_top += 3;
            }
        }
        MouseEventKind::ScrollUp => {
            if app.modal != Modal::None {
                app.text_viewer_scroll = app.text_viewer_scroll.saturating_sub(3);
                return;
            }
            if let Some(bottom_rect) = app.bottom_panel_rect {
                if mouse.row >= bottom_rect.y && mouse.row < bottom_rect.y + bottom_rect.height
                    && mouse.column >= bottom_rect.x && mouse.column < bottom_rect.x + bottom_rect.width
                {
                    if app.show_output {
                        app.output_scroll = app.output_scroll.saturating_add(3);
                        return;
                    } else if app.show_terminal {
                        app.terminal_scroll = app.terminal_scroll.saturating_add(3);
                        return;
                    }
                }
            }
            if app.show_explorer && mouse.column < 28 {
                app.explorer.scroll_up(3);
            } else {
                app.current_buffer_mut().scroll_top = app.current_buffer().scroll_top.saturating_sub(3);
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            app.is_dragging_bottom_panel = false;
            app.mouse_drag_start = None;
            let is_visual = app.mode == Mode::Visual;
            let buf = app.current_buffer_mut();
            if let Some(sel) = buf.selection {
                if sel.start_line == sel.end_line && sel.start_col == sel.end_col && !is_visual {
                    buf.selection = None;
                }
            }
        }
        _ => {}
    }
}
