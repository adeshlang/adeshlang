use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph},
};
use std::path::Path;

use crate::app::{App, MenuTab, Modal};
use crate::config::LineNumberMode;
use crate::git::{FileGitStatus, GutterDiffKind};
use crate::highlighter::{Language, highlight_line, parse_ansi_to_line};
use crate::mode::Mode;
use crate::runner::Backend;

pub fn render_ui(f: &mut Frame, app: &mut App) {
    let area = f.area();
    f.render_widget(Clear, area);

    let bottom_height = if app.show_terminal || app.show_output {
        app.bottom_panel_height
            .clamp(3, area.height.saturating_sub(8))
    } else {
        0
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Menu bar
            Constraint::Length(1), // Tab bar
            Constraint::Min(5),    // Main editor/split area
            Constraint::Length(if app.show_output && !app.show_terminal {
                bottom_height
            } else {
                0
            }),
            Constraint::Length(if app.show_terminal { bottom_height } else { 0 }),
            Constraint::Length(if app.show_command_line { 1 } else { 0 }),
            Constraint::Length(1), // Powerline status bar
        ])
        .split(area);

    if app.show_terminal {
        app.bottom_panel_drag_y = chunks[4].y;
    } else if app.show_output {
        app.bottom_panel_drag_y = chunks[3].y;
    } else {
        app.bottom_panel_drag_y = 0;
    }

    render_menu_bar(f, app, chunks[0]);
    render_tab_bar(f, app, chunks[1]);
    render_workspace(f, app, chunks[2]);

    if app.show_output && !app.show_terminal {
        render_output_panel(f, app, chunks[3]);
    }
    if app.show_terminal {
        render_terminal_panel(f, app, chunks[4]);
    }
    if app.show_command_line {
        render_command_line_bar(f, app, chunks[5]);
    }
    render_powerline_status_bar(f, app, chunks[6]);

    // Menu dropdown
    if app.active_menu != MenuTab::None && app.modal == Modal::None {
        render_menu_dropdown(f, app, chunks[0]);
    } else {
        app.menu_dropdown_rect = None;
    }

    // Floating auto-completion popup
    if app.completion.is_visible && app.modal == Modal::None {
        render_completion_popup(f, app, chunks[2]);
    }

    // Modal dialogs
    match app.modal {
        Modal::CommandPalette => render_command_palette(f, app, area),
        Modal::FuzzyFileFinder => render_fuzzy_file_finder(f, app, area),
        Modal::WorkspaceGrep => render_workspace_grep(f, app, area),
        Modal::DocumentSymbols => render_document_symbols(f, app, area),
        Modal::DiagnosticsList => render_diagnostics_list(f, app, area),
        Modal::HoverTooltip => render_hover_tooltip(f, app, area),
        Modal::KeybindingsHelp => render_keybindings_help(f, app, area),
        Modal::AstViewer => {
            render_text_viewer(f, app, area, "AST Inspector (:ast)", &app.ast_view_content)
        }
        Modal::BytecodeViewer => render_text_viewer(
            f,
            app,
            area,
            "Bytecode Disassembler (:bytecode)",
            &app.bytecode_view_content,
        ),
        Modal::HirViewer => render_text_viewer(
            f,
            app,
            area,
            "HIR — High-Level IR (:hir)",
            &app.hir_view_content,
        ),
        Modal::IrViewer => render_text_viewer(
            f,
            app,
            area,
            "IR — Intermediate Representation (:ir)",
            &app.ir_view_content,
        ),
        Modal::LirViewer => render_text_viewer(
            f,
            app,
            area,
            "LIR — Low-Level SSA IR (:lir)",
            &app.lir_view_content,
        ),
        Modal::MlirViewer => render_text_viewer(
            f,
            app,
            area,
            "MLIR — Multi-Level Intermediate Representation (:mlir)",
            &app.mlir_view_content,
        ),
        Modal::TokensViewer => render_text_viewer(
            f,
            app,
            area,
            "Lexer Token Stream (:tokens)",
            &app.tokens_view_content,
        ),
        Modal::AdeshDocViewer => render_text_viewer(
            f,
            app,
            area,
            "Adesh Standard Library Documentation (:doc)",
            &app.doc_view_content,
        ),
        Modal::AdeshCheckViewer => render_text_viewer(
            f,
            app,
            area,
            "Adesh Syntax & Type Diagnostics (:check)",
            &app.check_view_content,
        ),
        Modal::GitManager => render_git_manager_modal(f, app, area),
        Modal::GitDiff => render_git_diff_modal(f, app, area),
        Modal::GitLog => render_git_log_modal(f, app, area),
        Modal::GitCommitPrompt => render_prompt_modal(
            f,
            app,
            area,
            "Git Commit Message (Enter to commit, Esc to cancel)",
        ),
        Modal::GitBranchSelector => render_git_branch_selector(f, app, area),
        Modal::Search | Modal::Replace => render_search_modal(f, app, area),
        Modal::SelectBackend => render_backend_selector(f, app, area),
        Modal::OpenFilePrompt => render_prompt_modal(f, app, area, "Open File — Enter Path"),
        Modal::SaveAsPrompt => render_prompt_modal(f, app, area, "Save As — Enter Path"),
        Modal::NewFilePrompt => render_prompt_modal(f, app, area, "New File — Enter Name"),
        Modal::NewDirPrompt => render_prompt_modal(f, app, area, "New Directory — Enter Name"),
        Modal::RenameFilePrompt => {
            render_prompt_modal(f, app, area, "Rename File — Enter New Name")
        }
        Modal::GotoLinePrompt => render_prompt_modal(f, app, area, "Go to Line Number"),
        Modal::ThemeSelector => render_theme_selector(f, app, area),
        Modal::CloseUnsavedConfirm(idx) => render_close_unsaved_modal(f, app, area, idx),
        Modal::InfoPage => render_info_page(f, app, area),
        _ => {}
    }

    // File Explorer Right-Click Context Menu
    if app.explorer_context_menu.is_some() {
        render_explorer_context_menu(f, app, area);
    }

    // Toast notifications
    render_toasts(f, app, area);
}

fn render_menu_bar(f: &mut Frame, app: &mut App, area: Rect) {
    let mut spans = Vec::new();

    for (_idx, (label, tab)) in MenuTab::all().iter().enumerate() {
        let is_active = app.active_menu == *tab;
        let style = if is_active {
            Style::default()
                .fg(Color::Black)
                .bg(app.theme.line_number_curr)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.fg).bg(app.theme.status_bg)
        };
        spans.push(Span::styled(format!(" {} ", label), style));
        spans.push(Span::styled(" ", Style::default().bg(app.theme.status_bg)));
    }

    let hint = " F1:Help  F2:Save  F3:Tree  F4:Nums  Ctrl+P:Files  Ctrl+G:Git  ::Cmd ";
    spans.push(Span::styled(
        hint,
        Style::default()
            .fg(app.theme.comment)
            .bg(app.theme.status_bg),
    ));

    let run_btn_text = " [▶ Run (F5)] ";
    let run_btn_len = run_btn_text.len() as u16;
    let run_btn_x = area.width.saturating_sub(run_btn_len + 1);

    app.run_button_x = run_btn_x;
    app.run_button_w = run_btn_len;

    let line = Line::from(spans);
    let p = Paragraph::new(line).style(Style::default().bg(app.theme.status_bg));
    f.render_widget(p, area);

    // Render prominent Run button on top right of menu bar
    let run_btn_rect = Rect::new(area.x + run_btn_x, area.y, run_btn_len, 1);
    let run_p = Paragraph::new(Span::styled(
        run_btn_text,
        Style::default()
            .fg(Color::Black)
            .bg(app.theme.string)
            .add_modifier(Modifier::BOLD),
    ));
    f.render_widget(run_p, run_btn_rect);
}

fn render_menu_dropdown(f: &mut Frame, app: &mut App, menu_bar_area: Rect) {
    let tabs = MenuTab::all();
    let idx = tabs
        .iter()
        .position(|(_, t)| *t == app.active_menu)
        .unwrap_or(0);
    let (tab_start, _) = MenuTab::tab_rect(idx);

    let items = app.active_menu.items();
    let dropdown_height = (items.len() as u16 + 2).min(18);
    let dropdown_width = items.iter().map(|s| s.len()).max().unwrap_or(12).max(12) as u16 + 4;

    let dropdown_area = Rect::new(
        menu_bar_area.x + tab_start,
        menu_bar_area.y + 1,
        dropdown_width,
        dropdown_height,
    );

    app.menu_dropdown_rect = Some((
        dropdown_area.x,
        dropdown_area.y,
        dropdown_area.width,
        dropdown_area.height,
    ));

    f.render_widget(Clear, dropdown_area);

    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let is_sel = i == app.menu_selected;
            let style = if is_sel {
                Style::default()
                    .fg(Color::Black)
                    .bg(app.theme.line_number_curr)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.theme.fg)
            };
            ListItem::new(format!(" {} ", label)).style(style)
        })
        .collect();

    let title = format!(" {} ", tabs[idx].0);
    let list = List::new(list_items).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .style(
                Style::default()
                    .bg(app.theme.popup_bg)
                    .fg(app.theme.popup_border),
            ),
    );
    f.render_widget(list, dropdown_area);
}

fn render_tab_bar(f: &mut Frame, app: &mut App, area: Rect) {
    app.tab_close_positions.clear();
    let mut spans = Vec::new();
    let mut current_x = area.x;

    for (idx, buf) in app.buffers.iter().enumerate() {
        let is_active = idx == app.active_buffer;
        let mut name = buf.file_name();
        if buf.modified {
            name.push_str(" ●");
        }

        let tab_name_text = format!(" {} ", name);
        let tab_name_len = tab_name_text.chars().count() as u16;
        let close_btn_text = "✕ ";
        let close_btn_len = close_btn_text.chars().count() as u16;

        let tab_start_x = current_x;
        let close_btn_start_x = current_x + tab_name_len;
        let close_btn_end_x = close_btn_start_x + close_btn_len;

        let name_style = if is_active {
            Style::default()
                .fg(app.theme.line_number_curr)
                .bg(app.theme.current_line_bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(app.theme.comment)
                .bg(app.theme.status_bg)
        };

        let close_style = if is_active {
            Style::default()
                .fg(app.theme.error)
                .bg(app.theme.current_line_bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(app.theme.comment)
                .bg(app.theme.status_bg)
        };

        spans.push(Span::styled(tab_name_text, name_style));
        spans.push(Span::styled(close_btn_text, close_style));
        app.tab_close_positions
            .push((tab_start_x, close_btn_start_x, close_btn_end_x, idx));
        current_x = close_btn_end_x;

        spans.push(Span::styled(
            "│",
            Style::default()
                .fg(app.theme.comment)
                .bg(app.theme.status_bg),
        ));
        current_x += 1;
    }

    let line = Line::from(spans);
    let p = Paragraph::new(line).style(Style::default().bg(app.theme.status_bg));
    f.render_widget(p, area);
}

fn render_workspace(f: &mut Frame, app: &mut App, area: Rect) {
    if app.show_explorer {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(28), Constraint::Min(10)])
            .split(area);

        render_explorer(f, app, chunks[0]);
        render_editor_splits(f, app, chunks[1]);
    } else {
        render_editor_splits(f, app, area);
    }
}

fn render_explorer(f: &mut Frame, app: &App, area: Rect) {
    let root_name = app
        .workspace
        .root
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let branch_info = if app.git.is_repo {
        format!(" [{}]", app.git.branch)
    } else {
        String::new()
    };
    let title = format!(" 📁 EXPLORER: {}{} ", root_name, branch_info);
    let border_style = if app.explorer_focused {
        Style::default()
            .fg(app.theme.line_number_curr)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(app.theme.comment)
    };

    let view_height = area.height.saturating_sub(2) as usize;
    let items: Vec<ListItem> = app
        .explorer
        .flat_nodes
        .iter()
        .enumerate()
        .skip(app.explorer.scroll_offset)
        .take(view_height)
        .map(|(idx, (path, depth))| {
            let is_sel = idx == app.explorer.selected_index && app.explorer_focused;
            let node = app.explorer.root_node.find(path);
            let icon = node.map(|n| n.icon()).unwrap_or("📄");
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let indent = "  ".repeat(*depth);

            // Git status indicator & styling
            let (git_badge, git_color) = if let Some(status) = app.git.file_statuses.get(path) {
                match status {
                    FileGitStatus::Modified => (" [M]", app.theme.line_number_curr),
                    FileGitStatus::Untracked => (" [U]", app.theme.string),
                    FileGitStatus::Added => (" [A]", app.theme.string),
                    FileGitStatus::Deleted => (" [D]", app.theme.error),
                    FileGitStatus::Renamed => (" [R]", app.theme.function),
                    _ => ("", app.theme.fg),
                }
            } else {
                ("", app.theme.fg)
            };

            let style = if is_sel {
                Style::default()
                    .fg(Color::Black)
                    .bg(app.theme.line_number_curr)
                    .add_modifier(Modifier::BOLD)
            } else if path.is_dir() {
                Style::default()
                    .fg(app.theme.function)
                    .add_modifier(Modifier::BOLD)
            } else if !git_badge.is_empty() {
                Style::default().fg(git_color)
            } else {
                Style::default().fg(app.theme.fg)
            };

            ListItem::new(format!(
                "{}{}{} {}{}",
                indent,
                icon,
                if path.is_dir() { "" } else { "" },
                name,
                git_badge
            ))
            .style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style)
            .style(Style::default().bg(app.theme.bg)),
    );

    f.render_widget(list, area);
}

fn render_editor_splits(f: &mut Frame, app: &mut App, area: Rect) {
    app.pane_positions.clear();
    let pane_layouts = app.split_tree.compute_layout(area);

    for (pane_id, pane_rect) in pane_layouts {
        let is_active = pane_id == app.split_tree.active_pane_id;
        let buf_idx = app
            .split_tree
            .find_pane_by_id(pane_id)
            .map(|p| p.buffer_index)
            .unwrap_or(0);
        let buf_idx = buf_idx.min(app.buffers.len().saturating_sub(1));

        app.pane_positions.push((pane_id, buf_idx, pane_rect));
        render_single_editor_pane(f, app, pane_rect, pane_id, buf_idx, is_active);
    }
}

fn render_single_editor_pane(
    f: &mut Frame,
    app: &mut App,
    area: Rect,
    _pane_id: usize,
    buf_idx: usize,
    is_active: bool,
) {
    if buf_idx >= app.buffers.len() {
        return;
    }

    let (
        buf_path,
        buf_lines,
        cursor_line,
        cursor_col,
        matching_pos,
        scroll_top,
        scroll_left,
        modified,
        file_name,
    ) = {
        let buf = &mut app.buffers[buf_idx];
        let view_height = area.height.saturating_sub(2) as usize;
        let view_width = area.width.saturating_sub(10) as usize;
        buf.adjust_scroll(view_height, view_width);
        (
            buf.path.clone(),
            buf.lines.clone(),
            buf.cursor.line,
            buf.cursor.col,
            buf.matching_bracket_pos(),
            buf.scroll_top,
            buf.scroll_left,
            buf.modified,
            buf.file_name(),
        )
    };

    let lang = Language::from_path(buf_path.as_deref());
    let border_style = if is_active {
        Style::default().fg(app.theme.line_number_curr)
    } else {
        Style::default().fg(app.theme.comment)
    };

    // Calculate line-by-line gutter diff
    let gutter_diff = app.git.compute_gutter_diff(buf_path.as_deref(), &buf_lines);

    let view_height = area.height.saturating_sub(2) as usize;
    let line_count = buf_lines.len();
    let digits = line_count.to_string().len().max(2);

    let mut rendered_lines = Vec::new();

    for row in 0..view_height {
        let line_idx = scroll_top + row;
        if line_idx >= line_count {
            rendered_lines.push(Line::from(vec![Span::styled(
                "~",
                Style::default().fg(app.theme.comment),
            )]));
            continue;
        }

        let is_curr = line_idx == cursor_line;
        let line_str = &buf_lines[line_idx];

        // Git gutter diff marker
        let diff_span = match gutter_diff.get(&line_idx) {
            Some(GutterDiffKind::Added) => Span::styled("▎", Style::default().fg(app.theme.string)),
            Some(GutterDiffKind::Modified) => {
                Span::styled("▎", Style::default().fg(app.theme.line_number_curr))
            }
            Some(GutterDiffKind::Deleted) => {
                Span::styled("▶", Style::default().fg(app.theme.error))
            }
            None => Span::styled(" ", Style::default().fg(app.theme.comment)),
        };

        // Line number formatting
        let num_str = match app.line_number_mode {
            LineNumberMode::Absolute => format!("{:>width$} ", line_idx + 1, width = digits),
            LineNumberMode::Relative => {
                if is_curr {
                    format!("{:>width$} ", line_idx + 1, width = digits)
                } else {
                    let diff = (line_idx as isize - cursor_line as isize).abs();
                    format!("{:>width$} ", diff, width = digits)
                }
            }
            LineNumberMode::None => String::new(),
        };

        let num_style = if is_curr {
            Style::default()
                .fg(app.theme.line_number_curr)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.line_number)
        };

        let mut spans = vec![diff_span, Span::styled(num_str, num_style)];

        let match_col = if let Some((ml, mc)) = matching_pos {
            if ml == line_idx { Some(mc) } else { None }
        } else {
            None
        };

        let hl_line = highlight_line(line_str, &app.theme, lang, app.rainbow_brackets, match_col);
        spans.extend(hl_line.spans);

        let row_style = if is_curr && app.show_line_highlight {
            Style::default().bg(app.theme.current_line_bg)
        } else {
            Style::default().bg(app.theme.bg)
        };

        rendered_lines.push(Line::from(spans).style(row_style));
    }

    // Breadcrumb title: 🏠 workspace ❯ folder ❯ filename
    let breadcrumb = format_breadcrumb(
        &app.workspace.root,
        buf_path.as_deref(),
        &file_name,
        modified,
        lang,
    );

    let paragraph = Paragraph::new(rendered_lines).block(
        Block::default()
            .title(breadcrumb)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style)
            .style(Style::default().bg(app.theme.bg)),
    );

    f.render_widget(paragraph, area);

    // Set hardware blinking cursor if active and not in overlay modal or command line
    if is_active
        && app.modal == Modal::None
        && !app.show_command_line
        && !app.terminal_focused
        && !app.explorer_focused
    {
        if cursor_line >= scroll_top && cursor_line < scroll_top + view_height {
            let num_width = match app.line_number_mode {
                LineNumberMode::None => 0,
                _ => digits + 1,
            };
            let gutter_width = 1 + num_width; // 1 for diff marker
            let cursor_screen_row = (cursor_line - scroll_top) as u16;
            let cursor_screen_col = cursor_col.saturating_sub(scroll_left) as u16;
            let cursor_x = area.x + 1 + gutter_width as u16 + cursor_screen_col;
            let cursor_y = area.y + 1 + cursor_screen_row;

            if cursor_x < area.x + area.width.saturating_sub(1)
                && cursor_y < area.y + area.height.saturating_sub(1)
            {
                f.set_cursor_position((cursor_x, cursor_y));
            }
        }
    }
}

fn format_breadcrumb(
    root: &Path,
    path: Option<&Path>,
    file_name: &str,
    modified: bool,
    lang: Language,
) -> String {
    let mod_dot = if modified { " ●" } else { "" };
    if let Some(p) = path {
        if let Ok(rel) = p.strip_prefix(root) {
            let mut parts = Vec::new();
            if let Some(parent) = rel.parent() {
                for c in parent.components() {
                    parts.push(c.as_os_str().to_string_lossy().to_string());
                }
            }
            if parts.is_empty() {
                format!(" ❯ {} ({}){} ", file_name, lang.display_name(), mod_dot)
            } else {
                format!(
                    " ❯ {} ❯ {} ({}){} ",
                    parts.join(" ❯ "),
                    file_name,
                    lang.display_name(),
                    mod_dot
                )
            }
        } else {
            format!(" ❯ {} ({}){} ", file_name, lang.display_name(), mod_dot)
        }
    } else {
        format!(" ❯ {} ({}){} ", file_name, lang.display_name(), mod_dot)
    }
}

fn render_output_panel(f: &mut Frame, app: &mut App, area: Rect) {
    app.bottom_panel_rect = Some(area);
    f.render_widget(Clear, area);
    let out = app.runner.output.lock().unwrap();
    let total_lines = out.lines.len();
    let view_height = area.height.saturating_sub(2) as usize;
    let max_scroll = total_lines.saturating_sub(view_height);
    let scroll = app.output_scroll.min(max_scroll);

    let title = if scroll > 0 {
        format!(
            " ⚙ OUTPUT: {} [{}] (Line {}/{} — Scroll / PgUp / PgDn) ",
            app.active_backend.name(),
            if out.is_running {
                "RUNNING..."
            } else {
                "FINISHED"
            },
            total_lines.saturating_sub(scroll),
            total_lines
        )
    } else {
        format!(
            " ⚙ OUTPUT: {} [{}] (Ctrl+` to toggle, drag border to resize, mouse wheel to scroll) ",
            app.active_backend.name(),
            if out.is_running {
                "RUNNING..."
            } else {
                "FINISHED"
            }
        )
    };

    let start_idx = (total_lines.saturating_sub(view_height)).saturating_sub(scroll);
    let end_idx = (start_idx + view_height).min(total_lines);
    let visible_lines = if start_idx < total_lines {
        &out.lines[start_idx..end_idx]
    } else {
        &[]
    };

    let items: Vec<ListItem> = visible_lines
        .iter()
        .map(|line| {
            if line.contains('\x1b') {
                ListItem::new(parse_ansi_to_line(line))
            } else if line.starts_with("[err]") {
                ListItem::new(line.clone()).style(Style::default().fg(app.theme.error))
            } else if line.starts_with("✓") || line.starts_with("[ok]") {
                ListItem::new(line.clone()).style(Style::default().fg(app.theme.string))
            } else {
                ListItem::new(line.clone()).style(Style::default().fg(app.theme.fg))
            }
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.function))
            .style(Style::default().bg(app.theme.status_bg)),
    );

    f.render_widget(list, area);
}

fn render_terminal_panel(f: &mut Frame, app: &mut App, area: Rect) {
    app.bottom_panel_rect = Some(area);
    f.render_widget(Clear, area);
    let history = app.terminal.get_history();
    let total_history = history.len();
    let view_height = area.height.saturating_sub(2) as usize;
    let max_history_lines = view_height.saturating_sub(1);
    let max_scroll = total_history.saturating_sub(max_history_lines);
    let scroll = app.terminal_scroll.min(max_scroll);

    let title = if scroll > 0 {
        format!(
            " 💻 INTEGRATED TERMINAL (Scrolled {} lines up — Scroll down or type to return) ",
            scroll
        )
    } else {
        " 💻 INTEGRATED TERMINAL (Ctrl+J to toggle, drag border to resize, mouse wheel to scroll) "
            .to_string()
    };

    let start_idx = (total_history.saturating_sub(max_history_lines)).saturating_sub(scroll);
    let end_idx = (start_idx + max_history_lines).min(total_history);
    let visible_history = if start_idx < total_history {
        &history[start_idx..end_idx]
    } else {
        &[]
    };

    let mut lines = Vec::new();
    for line in visible_history {
        if line.starts_with("> ") || line.starts_with("$ ") {
            lines.push(Line::from(vec![
                Span::styled(
                    "❯ ",
                    Style::default()
                        .fg(app.theme.function)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(&line[2..], Style::default().fg(app.theme.line_number_curr)),
            ]));
        } else if line.contains('\x1b') {
            lines.push(parse_ansi_to_line(line));
        } else if line.starts_with("[err]") {
            lines.push(Line::from(vec![Span::styled(
                line.as_str(),
                Style::default().fg(app.theme.error),
            )]));
        } else if line.starts_with("[exit:") || line.starts_with("[terminated]") {
            lines.push(Line::from(vec![Span::styled(
                line.as_str(),
                Style::default().fg(app.theme.comment),
            )]));
        } else {
            lines.push(Line::from(vec![Span::styled(
                line.as_str(),
                Style::default().fg(app.theme.fg),
            )]));
        }
    }

    // Active input line (when not scrolled up into past history):
    if scroll == 0 {
        lines.push(Line::from(vec![
            Span::styled(
                "❯ ",
                Style::default()
                    .fg(app.theme.function)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(app.terminal.get_input(), Style::default().fg(app.theme.fg)),
        ]));
    }

    let p = Paragraph::new(lines).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(if app.terminal_focused {
                Style::default()
                    .fg(app.theme.function)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.theme.comment)
            })
            .style(Style::default().bg(app.theme.status_bg)),
    );

    f.render_widget(p, area);

    if app.terminal_focused && scroll == 0 {
        let input_line_y = area.y + 1 + (visible_history.len() as u16);
        let cur_x = (area.x + 3 + app.terminal.get_input().chars().count() as u16)
            .min(area.x + area.width.saturating_sub(2));
        f.set_cursor_position((
            cur_x,
            input_line_y.min(area.y + area.height.saturating_sub(2)),
        ));
    }
}

fn render_command_line_bar(f: &mut Frame, app: &App, area: Rect) {
    let line = Line::from(vec![
        Span::styled(
            ":",
            Style::default()
                .fg(app.theme.line_number_curr)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(&app.command_line, Style::default().fg(app.theme.fg)),
    ]);
    let p = Paragraph::new(line).style(Style::default().bg(app.theme.status_bg));
    f.render_widget(p, area);

    let cur_x = (area.x + 1 + app.command_line.chars().count() as u16)
        .min(area.x + area.width.saturating_sub(1));
    f.set_cursor_position((cur_x, area.y));
}

fn render_powerline_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let buf = app.current_buffer();
    let lang = app.current_language();

    let (mode_str, mode_bg) = match app.mode {
        Mode::Normal => (" NORMAL ", app.theme.function),
        Mode::Insert => (" INSERT ", app.theme.string),
        Mode::Visual => (" VISUAL ", app.theme.keyword),
        Mode::Command => (" COMMAND ", app.theme.number),
    };

    let mode_span = Span::styled(
        mode_str,
        Style::default()
            .fg(Color::Black)
            .bg(mode_bg)
            .add_modifier(Modifier::BOLD),
    );

    let file_info = format!(
        " 📄 {} {} ",
        buf.file_name(),
        if buf.modified { "●" } else { "" }
    );
    let file_span = Span::styled(
        file_info,
        Style::default().fg(app.theme.fg).bg(app.theme.status_bg),
    );

    let split_info = format!(
        " [Pane {}/{}] ",
        app.split_tree.active_pane_id + 1,
        app.split_tree.pane_count()
    );
    let split_span = Span::styled(
        split_info,
        Style::default()
            .fg(app.theme.comment)
            .bg(app.theme.status_bg),
    );

    let git_span = if app.git.is_repo {
        let ahead_behind = match (app.git.ahead, app.git.behind) {
            (0, 0) => String::new(),
            (a, 0) => format!(" ⇡{}", a),
            (0, b) => format!(" ⇣{}", b),
            (a, b) => format!(" ⇡{}⇣{}", a, b),
        };
        let status_count = app.git.staged_files.len()
            + app.git.unstaged_files.len()
            + app.git.untracked_files.len();
        let changes = if status_count > 0 {
            format!(" *{}", status_count)
        } else {
            String::new()
        };
        Span::styled(
            format!("  {}{}{} ", app.git.branch, ahead_behind, changes),
            Style::default()
                .fg(app.theme.line_number_curr)
                .bg(app.theme.status_bg),
        )
    } else {
        Span::styled(
            " [No Git] ",
            Style::default()
                .fg(app.theme.comment)
                .bg(app.theme.status_bg),
        )
    };

    let backend_info = format!(" [{}] ", app.active_backend.name());
    let backend_span = Span::styled(
        backend_info,
        Style::default()
            .fg(app.theme.type_color)
            .bg(app.theme.status_bg),
    );

    let pos_info = format!(" Ln {}, Col {} ", buf.cursor.line + 1, buf.cursor.col + 1);
    let pos_span = Span::styled(
        pos_info,
        Style::default()
            .fg(Color::Black)
            .bg(app.theme.line_number_curr)
            .add_modifier(Modifier::BOLD),
    );

    let lang_span = Span::styled(
        format!(" {} │ UTF-8 ", lang.display_name()),
        Style::default().fg(app.theme.fg).bg(app.theme.status_bg),
    );

    let spans = vec![
        mode_span,
        file_span,
        split_span,
        git_span,
        backend_span,
        Span::styled(
            format!("  {}  ", app.status_message),
            Style::default()
                .fg(app.theme.comment)
                .bg(app.theme.status_bg),
        ),
        lang_span,
        pos_span,
    ];

    let p = Paragraph::new(Line::from(spans)).style(Style::default().bg(app.theme.status_bg));
    f.render_widget(p, area);
}

fn render_completion_popup(f: &mut Frame, app: &App, workspace_area: Rect) {
    let buf = app.current_buffer();
    let col_offset = (buf.cursor.col.saturating_sub(buf.scroll_left) as u16 + 8)
        .min(workspace_area.width.saturating_sub(30));
    let row_offset = (buf.cursor.line.saturating_sub(buf.scroll_top) as u16 + 2)
        .min(workspace_area.height.saturating_sub(10));

    let popup_w = 34u16;
    let popup_h = (app.completion.filtered.len() as u16 + 2).min(10);

    let popup_area = Rect::new(
        workspace_area.x + col_offset,
        workspace_area.y + row_offset,
        popup_w,
        popup_h,
    );

    f.render_widget(Clear, popup_area);

    let items: Vec<ListItem> = app
        .completion
        .filtered
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let is_sel = idx == app.completion.selected_index;
            let style = if is_sel {
                Style::default()
                    .fg(Color::Black)
                    .bg(app.theme.line_number_curr)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.theme.fg)
            };
            ListItem::new(format!(" {} {}", item.kind.badge(), item.label)).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(" Complete (Tab/Enter) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.line_number_curr))
            .style(Style::default().bg(app.theme.popup_bg)),
    );

    f.render_widget(list, popup_area);
}

fn render_git_manager_modal(f: &mut Frame, app: &App, area: Rect) {
    let w = (area.width * 85 / 100).max(65).min(area.width);
    let h = (area.height * 85 / 100).max(20).min(area.height);
    let popup_area = Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    );
    f.render_widget(Clear, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header / Branch info
            Constraint::Min(8),    // File list
            Constraint::Length(3), // Footer / Shortcuts
        ])
        .split(popup_area);

    // Header
    let branch_text = format!(
        " Branch:  {} │ Ahead: {} │ Behind: {} │ Staged: {} │ Unstaged: {} │ Untracked: {} ",
        app.git.branch,
        app.git.ahead,
        app.git.behind,
        app.git.staged_files.len(),
        app.git.unstaged_files.len(),
        app.git.untracked_files.len()
    );
    let header = Paragraph::new(branch_text).block(
        Block::default()
            .title(" 🌿 Git Version Control (Ctrl+G) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.line_number_curr))
            .style(Style::default().bg(app.theme.popup_bg)),
    );
    f.render_widget(header, chunks[0]);

    // Build unified selectable list of git files
    let mut items = Vec::new();
    let mut current_item_idx = 0;

    // 1. Staged Files
    if !app.git.staged_files.is_empty() {
        items.push(ListItem::new(Span::styled(
            "── Staged Changes ────────────────────────────────────────",
            Style::default()
                .fg(app.theme.string)
                .add_modifier(Modifier::BOLD),
        )));
        for (path, status) in &app.git.staged_files {
            let is_sel = current_item_idx == app.git_selected;
            let file_name = path.file_name().unwrap_or_default().to_string_lossy();
            let style = if is_sel {
                Style::default()
                    .fg(Color::Black)
                    .bg(app.theme.line_number_curr)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.theme.string)
            };
            items.push(
                ListItem::new(format!("  [{}] {} (Staged)", status.badge(), file_name))
                    .style(style),
            );
            current_item_idx += 1;
        }
    }

    // 2. Unstaged Files
    if !app.git.unstaged_files.is_empty() {
        items.push(ListItem::new(Span::styled(
            "── Changes Not Staged ────────────────────────────────────",
            Style::default()
                .fg(app.theme.line_number_curr)
                .add_modifier(Modifier::BOLD),
        )));
        for (path, status) in &app.git.unstaged_files {
            let is_sel = current_item_idx == app.git_selected;
            let file_name = path.file_name().unwrap_or_default().to_string_lossy();
            let style = if is_sel {
                Style::default()
                    .fg(Color::Black)
                    .bg(app.theme.line_number_curr)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.theme.line_number_curr)
            };
            items.push(ListItem::new(format!("  [{}] {}", status.badge(), file_name)).style(style));
            current_item_idx += 1;
        }
    }

    // 3. Untracked Files
    if !app.git.untracked_files.is_empty() {
        items.push(ListItem::new(Span::styled(
            "── Untracked Files ───────────────────────────────────────",
            Style::default()
                .fg(app.theme.error)
                .add_modifier(Modifier::BOLD),
        )));
        for path in &app.git.untracked_files {
            let is_sel = current_item_idx == app.git_selected;
            let file_name = path.file_name().unwrap_or_default().to_string_lossy();
            let style = if is_sel {
                Style::default()
                    .fg(Color::Black)
                    .bg(app.theme.line_number_curr)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.theme.fg)
            };
            items.push(ListItem::new(format!("  [U] {}", file_name)).style(style));
            current_item_idx += 1;
        }
    }

    if items.is_empty() {
        items.push(
            ListItem::new("  ✔ Working tree clean, nothing to commit.")
                .style(Style::default().fg(app.theme.string)),
        );
    }

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.comment))
            .style(Style::default().bg(app.theme.popup_bg)),
    );
    f.render_widget(list, chunks[1]);

    // Action shortcuts bar
    let footer_text = " [s] Stage  [u] Unstage  [a] Stage All  [c] Commit  [P] Push  [p] Pull  [d] Diff  [l] Log  [b] Branch  [r] Refresh  [Esc] Close ";
    let footer = Paragraph::new(footer_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.function))
            .style(
                Style::default()
                    .bg(app.theme.popup_bg)
                    .fg(app.theme.comment),
            ),
    );
    f.render_widget(footer, chunks[2]);
}

fn render_git_diff_modal(f: &mut Frame, app: &App, area: Rect) {
    let w = (area.width * 9 / 10).max(60).min(area.width);
    let h = (area.height * 9 / 10).max(20).min(area.height);
    let popup_area = Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    );
    f.render_widget(Clear, popup_area);

    let mut diff_lines = Vec::new();
    if app.git.current_diff.is_empty() {
        diff_lines.push(Line::from(vec![Span::styled(
            "No unstaged or staged diffs found for current buffer.",
            Style::default().fg(app.theme.string),
        )]));
    } else {
        for line in app.git.current_diff.lines() {
            let style = if line.starts_with('+') && !line.starts_with("+++") {
                Style::default().fg(app.theme.string)
            } else if line.starts_with('-') && !line.starts_with("---") {
                Style::default().fg(app.theme.error)
            } else if line.starts_with('@') {
                Style::default()
                    .fg(app.theme.line_number_curr)
                    .add_modifier(Modifier::BOLD)
            } else if line.starts_with("diff ") || line.starts_with("index ") {
                Style::default()
                    .fg(app.theme.function)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.theme.fg)
            };
            diff_lines.push(Line::from(vec![Span::styled(line.to_string(), style)]));
        }
    }

    let view_height = h.saturating_sub(2) as usize;
    let total_lines = diff_lines.len();
    let max_scroll = total_lines.saturating_sub(view_height);
    let scroll = app.text_viewer_scroll.min(max_scroll);
    let visible_lines: Vec<Line> = diff_lines
        .into_iter()
        .skip(scroll)
        .take(view_height)
        .collect();

    let title_info = if total_lines > view_height {
        format!(
            " 📄 Git Diff Viewer (:diff — Line {}/{} — ↑/↓/Scroll, Esc to close) ",
            scroll + 1,
            total_lines
        )
    } else {
        " 📄 Git Diff Viewer (:diff — Esc to close) ".to_string()
    };

    let p = Paragraph::new(visible_lines).block(
        Block::default()
            .title(title_info)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.line_number_curr))
            .style(Style::default().bg(app.theme.popup_bg)),
    );
    f.render_widget(p, popup_area);
}

fn render_git_log_modal(f: &mut Frame, app: &App, area: Rect) {
    let w = (area.width * 85 / 100).max(60).min(area.width);
    let h = (area.height * 85 / 100).max(18).min(area.height);
    let popup_area = Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    );
    f.render_widget(Clear, popup_area);

    let items: Vec<ListItem> = if app.git.commit_history.is_empty() {
        vec![
            ListItem::new("No commit history found.").style(Style::default().fg(app.theme.comment)),
        ]
    } else {
        app.git
            .commit_history
            .iter()
            .enumerate()
            .map(|(idx, commit)| {
                let is_sel = idx == app.git_log_selected;
                let text = format!(
                    " ⬢ {} │ {} │ {} │ {}",
                    commit.hash, commit.author, commit.date, commit.message
                );
                let style = if is_sel {
                    Style::default()
                        .fg(Color::Black)
                        .bg(app.theme.line_number_curr)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(app.theme.fg)
                };
                ListItem::new(text).style(style)
            })
            .collect()
    };

    let list = List::new(items).block(
        Block::default()
            .title(" 📜 Git Commit History (:git log — Esc to close) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.function))
            .style(Style::default().bg(app.theme.popup_bg)),
    );
    f.render_widget(list, popup_area);
}

fn render_git_branch_selector(f: &mut Frame, app: &App, area: Rect) {
    let w = 45u16.min(area.width);
    let h = 12u16.min(area.height);
    let popup_area = Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    );
    f.render_widget(Clear, popup_area);

    let branches = app.git.list_branches();
    let items: Vec<ListItem> = branches
        .iter()
        .map(|b| {
            let is_current = b == &app.git.branch;
            let icon = if is_current { "* " } else { "  " };
            let style = if is_current {
                Style::default()
                    .fg(app.theme.string)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.theme.fg)
            };
            ListItem::new(format!("{} {}", icon, b)).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title("  Switch Git Branch ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.function))
            .style(Style::default().bg(app.theme.popup_bg)),
    );
    f.render_widget(list, popup_area);
}

fn render_fuzzy_file_finder(f: &mut Frame, app: &App, area: Rect) {
    let w = (area.width * 7 / 10).max(40).min(area.width);
    let h = (area.height * 7 / 10).max(15).min(area.height);
    let popup_area = Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    );

    f.render_widget(Clear, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5)])
        .split(popup_area);

    let input_p = Paragraph::new(format!("> {}", app.prompt_input)).block(
        Block::default()
            .title(" 🔍 Fuzzy File Finder (Ctrl+P) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.function))
            .style(Style::default().bg(app.theme.popup_bg)),
    );
    f.render_widget(input_p, chunks[0]);

    let items: Vec<ListItem> = app
        .fuzzy_files
        .iter()
        .enumerate()
        .map(|(idx, (name, _, _))| {
            let is_sel = idx == app.fuzzy_selected;
            let style = if is_sel {
                Style::default()
                    .fg(Color::Black)
                    .bg(app.theme.line_number_curr)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.theme.fg)
            };
            ListItem::new(format!("  {}", name)).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.comment))
            .style(Style::default().bg(app.theme.popup_bg)),
    );
    f.render_widget(list, chunks[1]);
}

fn render_workspace_grep(f: &mut Frame, app: &App, area: Rect) {
    let w = (area.width * 8 / 10).max(50).min(area.width);
    let h = (area.height * 8 / 10).max(18).min(area.height);
    let popup_area = Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    );

    f.render_widget(Clear, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5)])
        .split(popup_area);

    let input_p = Paragraph::new(format!("Search: {}", app.grep_query)).block(
        Block::default()
            .title(" 🔎 Find in Workspace (Ctrl+Shift+F) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.function))
            .style(Style::default().bg(app.theme.popup_bg)),
    );
    f.render_widget(input_p, chunks[0]);

    let items: Vec<ListItem> = app
        .grep_results
        .iter()
        .enumerate()
        .map(|(idx, (path, line_num, content))| {
            let is_sel = idx == app.grep_selected;
            let file_name = path.file_name().unwrap_or_default().to_string_lossy();
            let text = format!("{}:{}: {}", file_name, line_num, content.trim());
            let style = if is_sel {
                Style::default()
                    .fg(Color::Black)
                    .bg(app.theme.line_number_curr)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.theme.fg)
            };
            ListItem::new(text).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.comment))
            .style(Style::default().bg(app.theme.popup_bg)),
    );
    f.render_widget(list, chunks[1]);
}

fn render_document_symbols(f: &mut Frame, app: &App, area: Rect) {
    let w = 50u16.min(area.width);
    let h = 20u16.min(area.height);
    let popup_area = Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    );
    f.render_widget(Clear, popup_area);

    let items = vec![
        ListItem::new(" ✦ main (fn) — line 1").style(Style::default().fg(app.theme.function)),
        ListItem::new(" ✦ calculate (fn) — line 12").style(Style::default().fg(app.theme.function)),
        ListItem::new(" ✦ Config (struct) — line 45")
            .style(Style::default().fg(app.theme.type_color)),
    ];

    let list = List::new(items).block(
        Block::default()
            .title(" 📑 Document Symbols (Outline) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.function))
            .style(Style::default().bg(app.theme.popup_bg)),
    );
    f.render_widget(list, popup_area);
}

fn render_diagnostics_list(f: &mut Frame, app: &App, area: Rect) {
    let w = 60u16.min(area.width);
    let h = 16u16.min(area.height);
    let popup_area = Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    );
    f.render_widget(Clear, popup_area);

    let diags = app.lsp.diagnostics.lock().unwrap();
    let items: Vec<ListItem> = if diags.is_empty() {
        vec![
            ListItem::new(" ✔ No problems found in workspace.")
                .style(Style::default().fg(app.theme.string)),
        ]
    } else {
        diags
            .iter()
            .map(|d| {
                ListItem::new(format!(" ✘ Line {}: {}", d.line + 1, d.message))
                    .style(Style::default().fg(app.theme.error))
            })
            .collect()
    };

    let list = List::new(items).block(
        Block::default()
            .title(" ⚠ Diagnostics / Problems (:problems) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.warning))
            .style(Style::default().bg(app.theme.popup_bg)),
    );
    f.render_widget(list, popup_area);
}

fn render_hover_tooltip(f: &mut Frame, app: &App, area: Rect) {
    let w = 50u16.min(area.width);
    let h = 10u16.min(area.height);
    let popup_area = Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    );
    f.render_widget(Clear, popup_area);

    let content = app
        .hover_content
        .as_deref()
        .unwrap_or("No documentation available.");
    let p = Paragraph::new(content).block(
        Block::default()
            .title(" 💡 Hover Info (K) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.function))
            .style(Style::default().bg(app.theme.popup_bg)),
    );
    f.render_widget(p, popup_area);
}

fn render_keybindings_help(f: &mut Frame, app: &App, area: Rect) {
    let w = (area.width * 85 / 100).max(65).min(area.width);
    let h = (area.height * 85 / 100).max(22).min(area.height);
    let popup_area = Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    );
    f.render_widget(Clear, popup_area);

    let help_text = vec![
        Line::from(vec![Span::styled(
            "--- Quick Navigation & Panels ---",
            Style::default()
                .fg(app.theme.line_number_curr)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("  Ctrl+J / Ctrl+T Toggle Terminal (PgUp/PgDn/Ctrl+U/Ctrl+D to scroll history)"),
        Line::from("  Ctrl+`          Toggle Output Panel (Mouse wheel to scroll)"),
        Line::from("  Ctrl+P          Fuzzy File Finder"),
        Line::from("  Ctrl+Shift+F    Find in Workspace (Ripgrep)"),
        Line::from("  Ctrl+G / :git   Git Status & Version Control Manager"),
        Line::from("  Ctrl+W v/s/c/o  Split Panes (Vertical, Horizontal, Close, Zoom)"),
        Line::from("  Home / End      Start of line (col 0) / End of line"),
        Line::from("  PageUp / PageDn Scroll view by 15 lines"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "--- Mouse & Drag Operations ---",
            Style::default()
                .fg(app.theme.line_number_curr)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("  Left Click      Click anywhere in code to position cursor directly"),
        Line::from("  Drag to Select  Click and drag across text to select range"),
        Line::from("  Cut/Copy/Del    Ctrl+X/x cut, Ctrl+C/y copy, Backspace/Delete/d delete"),
        Line::from("  Drag Border     Drag bottom panel top border to resize height (3-35)"),
        Line::from(
            "  Right Click     Open File Explorer context menu (New, Rename, Copy, Cut, Del)",
        ),
        Line::from(""),
        Line::from(vec![Span::styled(
            "--- Editing, Text Objects & Word Control ---",
            Style::default()
                .fg(app.theme.line_number_curr)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("  Ctrl+Backspace  Delete entire preceding word in a single keystroke"),
        Line::from("  Vim Counts      5j, 5k, 4w, 3dd (cut 3 lines), 4x (del 4 chars), 2yy, 10G"),
        Line::from("  Auto-Close      Auto-closes (), [], {}, \"\", '', `` with smart overtyping"),
        Line::from("  Auto-Wrap       Typing quote/bracket with active selection wraps text"),
        Line::from("  ciw / di\" / yiw Vim Text Objects (Inner Word, In Quotes, In Brackets)"),
        Line::from("  Ctrl+Alt+Up/Dn  Multi-Cursor Column Editing"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "--- Adesh Tooling & Compilers ---",
            Style::default()
                .fg(app.theme.line_number_curr)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("  F5 / Ctrl+F5    Run Program / Run with Backend Selection"),
        Line::from("  :ast / :hir     Abstract Syntax Tree / High-Level IR Inspector"),
        Line::from("  :ir / :lir      Intermediate Representation / Low-Level SSA IR"),
        Line::from("  :mlir/:bytecode Multi-Level IR / Bytecode Disassembly"),
        Line::from("  :tokens/:check  Lexer Token Stream / Syntax & Type Diagnostics"),
        Line::from("  :theme          Switch Color Theme (Tokyo Night, Catppuccin, Nord, etc.)"),
    ];

    let p = Paragraph::new(help_text).block(
        Block::default()
            .title(" 📖 Adesh Editor Keybindings Cheatsheet (Esc/Enter to close) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.function))
            .style(Style::default().bg(app.theme.popup_bg)),
    );
    f.render_widget(p, popup_area);
}

fn render_text_viewer(f: &mut Frame, app: &App, area: Rect, title: &str, content: &[String]) {
    let w = (area.width * 85 / 100).max(50).min(area.width);
    let h = (area.height * 85 / 100).max(18).min(area.height);
    let popup_area = Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    );
    f.render_widget(Clear, popup_area);

    let view_height = h.saturating_sub(2) as usize;
    let total_lines = content.len();
    let max_scroll = total_lines.saturating_sub(view_height);
    let scroll = app.text_viewer_scroll.min(max_scroll);

    let items: Vec<ListItem> = if content.is_empty() {
        vec![ListItem::new("  (Empty content)").style(Style::default().fg(app.theme.comment))]
    } else {
        content
            .iter()
            .skip(scroll)
            .take(view_height)
            .map(|l| ListItem::new(format!(" {}", l)).style(Style::default().fg(app.theme.fg)))
            .collect()
    };

    let title_info = if total_lines > view_height {
        format!(
            " ⚙ {} (Line {}/{} — ↑/↓/Scroll/PgDn, Esc to close) ",
            title,
            scroll + 1,
            total_lines
        )
    } else {
        format!(" ⚙ {} (Esc to close) ", title)
    };

    let list = List::new(items).block(
        Block::default()
            .title(title_info)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.function))
            .style(Style::default().bg(app.theme.popup_bg)),
    );
    f.render_widget(list, popup_area);
}

fn render_command_palette(f: &mut Frame, app: &App, area: Rect) {
    let w = 55u16.min(area.width);
    let h = 16u16.min(area.height);
    let popup_area = Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    );
    f.render_widget(Clear, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5)])
        .split(popup_area);

    let input_p = Paragraph::new(format!("> {}", app.palette.query)).block(
        Block::default()
            .title(" Command Palette (Ctrl+P) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.function))
            .style(Style::default().bg(app.theme.popup_bg)),
    );
    f.render_widget(input_p, chunks[0]);

    let cur_x = (chunks[0].x + 3 + app.palette.query.chars().count() as u16)
        .min(chunks[0].x + chunks[0].width.saturating_sub(2));
    f.set_cursor_position((cur_x, chunks[0].y + 1));

    let items: Vec<ListItem> = app
        .palette
        .filtered_items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let is_sel = idx == app.palette.selected_index;
            let style = if is_sel {
                Style::default()
                    .fg(Color::Black)
                    .bg(app.theme.line_number_curr)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.theme.fg)
            };
            ListItem::new(format!("  {} [{}]", item.name(), item.shortcut())).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.comment))
            .style(Style::default().bg(app.theme.popup_bg)),
    );
    f.render_widget(list, chunks[1]);
}

fn render_search_modal(f: &mut Frame, app: &App, area: Rect) {
    let w = 45u16.min(area.width);
    let h = 5u16.min(area.height);
    let popup_area = Rect::new(area.x + area.width.saturating_sub(w + 2), area.y + 2, w, h);
    f.render_widget(Clear, popup_area);

    let title = format!(
        " Find ({}/{}) ",
        app.search.matches.len(),
        app.search.matches.len()
    );
    let p = Paragraph::new(format!("Query: {}", app.search.query)).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.line_number_curr))
            .style(Style::default().bg(app.theme.popup_bg)),
    );
    f.render_widget(p, popup_area);

    let cur_x = (popup_area.x + 8 + app.search.query.chars().count() as u16)
        .min(popup_area.x + popup_area.width.saturating_sub(2));
    f.set_cursor_position((cur_x, popup_area.y + 1));
}

fn render_backend_selector(f: &mut Frame, app: &App, area: Rect) {
    let w = 40u16.min(area.width);
    let h = 15u16.min(area.height);
    let popup_area = Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    );
    f.render_widget(Clear, popup_area);

    let items: Vec<ListItem> = Backend::all()
        .iter()
        .map(|b| {
            let is_sel = b == &app.active_backend;
            let style = if is_sel {
                Style::default()
                    .fg(Color::Black)
                    .bg(app.theme.line_number_curr)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.theme.fg)
            };
            ListItem::new(format!("  {} ({})", b.name(), b.flag())).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(" Select Adesh Backend ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.function))
            .style(Style::default().bg(app.theme.popup_bg)),
    );
    f.render_widget(list, popup_area);
}

fn render_theme_selector(f: &mut Frame, app: &App, area: Rect) {
    let w = 40u16.min(area.width);
    let h = 16u16.min(area.height);
    let popup_area = Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    );
    f.render_widget(Clear, popup_area);

    let items: Vec<ListItem> = crate::config::Theme::all_themes()
        .iter()
        .map(|t| {
            let is_sel = t.name == app.theme.name;
            let style = if is_sel {
                Style::default()
                    .fg(Color::Black)
                    .bg(app.theme.line_number_curr)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.theme.fg)
            };
            ListItem::new(format!("  {}", t.name)).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(" Choose Color Theme ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.function))
            .style(Style::default().bg(app.theme.popup_bg)),
    );
    f.render_widget(list, popup_area);
}

fn render_prompt_modal(f: &mut Frame, app: &App, area: Rect, title: &str) {
    let w = 55u16.min(area.width);
    let h = 5u16.min(area.height);
    let popup_area = Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    );
    f.render_widget(Clear, popup_area);

    let p = Paragraph::new(format!("> {}", app.prompt_input)).block(
        Block::default()
            .title(format!(" {} ", title))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.function))
            .style(Style::default().bg(app.theme.popup_bg)),
    );
    f.render_widget(p, popup_area);

    let cur_x = (popup_area.x + 3 + app.prompt_input.chars().count() as u16)
        .min(popup_area.x + popup_area.width.saturating_sub(2));
    f.set_cursor_position((cur_x, popup_area.y + 1));
}

fn render_close_unsaved_modal(f: &mut Frame, app: &App, area: Rect, _idx: usize) {
    let w = 45u16.min(area.width);
    let h = 6u16.min(area.height);
    let popup_area = Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    );
    f.render_widget(Clear, popup_area);

    let text = vec![
        Line::from("Buffer has unsaved changes!"),
        Line::from("Save before closing? (y/n/Esc)"),
    ];

    let p = Paragraph::new(text).block(
        Block::default()
            .title(" Unsaved Changes ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.warning))
            .style(Style::default().bg(app.theme.popup_bg)),
    );
    f.render_widget(p, popup_area);
}

fn render_info_page(f: &mut Frame, app: &App, area: Rect) {
    let w = 55u16.min(area.width);
    let h = 13u16.min(area.height);
    let popup_area = Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    );
    f.render_widget(Clear, popup_area);

    let text = vec![
        Line::from("✨ AdeshLang Modern Production TUI Editor"),
        Line::from("Version 0.3.0 | Cross-Platform"),
        Line::from("Features: Multi-Pane Splits, Git & GitHub Integration,"),
        Line::from("          VSCode Smart Brackets & Overtyping,"),
        Line::from("          Adesh Compiler Tools (AST, Bytecode, Check),"),
        Line::from("          11 Aesthetic Color Themes & Ripgrep Search."),
        Line::from(""),
        Line::from("Press Esc or Enter to close."),
    ];

    let p = Paragraph::new(text).block(
        Block::default()
            .title(" About Adesh Editor ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.function))
            .style(Style::default().bg(app.theme.popup_bg)),
    );
    f.render_widget(p, popup_area);
}

fn render_toasts(f: &mut Frame, app: &mut App, area: Rect) {
    app.toast_close_positions.clear();
    app.cleanup_toasts();
    if app.toasts.is_empty() {
        return;
    }

    let mut toast_y = area.height.saturating_sub(4);
    let total_toasts = app.toasts.len();
    let start_idx = total_toasts.saturating_sub(3);

    for (idx, toast) in app.toasts.iter().enumerate().skip(start_idx).rev() {
        let msg_len = toast.message.chars().count() as u16;
        let toast_len = (msg_len + 8).min(area.width.saturating_sub(4)).max(18);
        let toast_rect = Rect::new(
            area.width.saturating_sub(toast_len + 2),
            toast_y,
            toast_len,
            3,
        );

        let color = match toast.kind.as_str() {
            "ERROR" => app.theme.error,
            "WARN" => app.theme.warning,
            "SUCCESS" => app.theme.string,
            _ => app.theme.function,
        };

        let line = Line::from(vec![
            Span::styled(
                format!(" {}", toast.message),
                Style::default().fg(app.theme.fg),
            ),
            Span::styled(
                " ✕",
                Style::default()
                    .fg(app.theme.error)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);

        let p = Paragraph::new(line).block(
            Block::default()
                .title(format!(" {} ", toast.kind))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(color))
                .style(Style::default().bg(app.theme.popup_bg)),
        );
        f.render_widget(p, toast_rect);

        app.toast_close_positions.push((
            toast_rect.x,
            toast_rect.y,
            toast_rect.width,
            toast_rect.height,
            idx,
        ));

        if toast_y >= 3 {
            toast_y -= 3;
        } else {
            break;
        }
    }
}

fn render_explorer_context_menu(f: &mut Frame, app: &App, area: Rect) {
    let ctx = match &app.explorer_context_menu {
        Some(c) => c,
        None => return,
    };

    let items = if ctx.is_dir {
        vec![
            "📂 Toggle Expand",
            "➕ New File Here",
            "📁 New Folder Here",
            "✏️ Rename",
            "📋 Copy",
            "✂️ Cut",
            "📥 Paste",
            "🗑️ Delete",
        ]
    } else {
        vec![
            "📄 Open",
            "✏️ Rename",
            "📋 Copy",
            "✂️ Cut",
            "📥 Paste",
            "🗑️ Delete",
            "➕ New File",
            "📁 New Folder",
        ]
    };

    let w = 22u16.min(area.width);
    let h = (items.len() as u16 + 2).min(area.height);
    let menu_x = ctx.x.min(area.width.saturating_sub(w));
    let menu_y = ctx.y.min(area.height.saturating_sub(h));
    let menu_area = Rect::new(menu_x, menu_y, w, h);

    f.render_widget(Clear, menu_area);

    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let is_sel = i == ctx.selected_index;
            let style = if is_sel {
                Style::default()
                    .fg(Color::Black)
                    .bg(app.theme.line_number_curr)
                    .add_modifier(Modifier::BOLD)
            } else if label.starts_with("🗑️") {
                Style::default().fg(app.theme.error)
            } else {
                Style::default().fg(app.theme.fg)
            };
            ListItem::new(format!(" {} ", label)).style(style)
        })
        .collect();

    let title = if ctx.is_dir {
        " 📁 Folder Actions "
    } else {
        " 📄 File Actions "
    };
    let list = List::new(list_items).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.function))
            .style(Style::default().bg(app.theme.popup_bg)),
    );

    f.render_widget(list, menu_area);
}
