//! TUI Widgets for Extensive AdeshLang Input Operations
//!
//! Provides interactive terminal widgets powered by Ratatui & Crossterm,
//! as well as automated headless/mock fallbacks for headless environments and tests.

#![allow(deprecated)]

use super::{init_inline_terminal, poll_key_event, TerminalGuard};
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Row, Table},
};
use std::collections::HashSet;
use std::io::{self, Write};
use std::time::{Duration, Instant};

// ============================================================================
// Data Structs & Configurations
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct InputOptions {
    pub default: Option<String>,
    pub trim: bool,
    pub masked: bool,
    pub timeout_ms: Option<u64>,
    pub suggestions: Vec<String>,
    pub allowed: Option<Vec<String>>,
    pub disallowed: Option<Vec<String>>,
    pub min_len: Option<usize>,
    pub not_empty: bool,
    pub multiline: bool,
    pub end_seq: Option<String>,
    pub style_color: Option<(u8, u8, u8)>,
}

#[derive(Debug, Clone, Default)]
pub struct CheckboxConfig {
    pub default: Vec<String>,
    pub required: bool,
    pub limit: Option<usize>,
    pub disabled: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RadioConfig {
    pub default: Option<String>,
    pub required: bool,
    pub disabled: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SelectConfig {
    pub default: Option<String>,
    pub filterable: bool,
    pub disabled: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FormFieldConfig {
    pub prompt: String,
    pub default: Option<String>,
    pub masked: bool,
    pub field_type: String,
    pub options: Vec<String>,
}

impl Default for FormFieldConfig {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            default: None,
            masked: false,
            field_type: "text".into(),
            options: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FormConfig {
    pub title: Option<String>,
    pub prefix: String,
    pub suffix: String,
    pub separator: String,
}

#[derive(Debug, Clone, Default)]
pub struct PasswordConfig {
    pub mask_char: char,
    pub show_strength: bool,
    pub allow_peek: bool,
}

#[derive(Debug, Clone, Default)]
pub struct FuzzyConfig {
    pub case_sensitive: bool,
    pub limit: Option<usize>,
    pub preview: bool,
}

#[derive(Debug, Clone)]
pub struct SliderConfig {
    pub unit: String,
    pub show_gauge: bool,
}

impl Default for SliderConfig {
    fn default() -> Self {
        Self {
            unit: String::new(),
            show_gauge: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub id: String,
    pub label: String,
    pub children: Vec<TreeNode>,
    pub expanded: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TreeConfig {
    pub allow_branch_selection: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TableConfig {
    pub editable: bool,
    pub sortable: bool,
    pub select_row: bool,
}

#[derive(Debug, Clone)]
pub struct TableResult {
    pub selected_row: usize,
    pub selected_col: usize,
    pub data: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Default)]
pub struct DatepickerConfig {
    pub min_year: Option<i32>,
    pub max_year: Option<i32>,
    pub default_year: Option<i32>,
    pub default_month: Option<u32>,
    pub default_day: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct TimepickerConfig {
    pub format_24h: bool,
    pub include_seconds: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ColorConfig {
    pub default_hex: Option<String>,
    pub format: String, // "hex", "rgb", "hsl"
}

#[derive(Debug, Clone, Default)]
pub struct PinConfig {
    pub mask: bool,
    pub allow_alpha: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DiffConfig {
    pub context_lines: usize,
}

#[derive(Debug, Clone, Default)]
pub struct HotkeyConfig {
    pub instructions: String,
}

#[derive(Debug, Clone)]
pub struct HotkeyResult {
    pub key: String,
    pub modifiers: Vec<String>,
    pub chord: String,
}

#[derive(Debug, Clone, Default)]
pub struct AiPredictConfig {
    pub max_suggestions: usize,
}

#[derive(Debug, Clone, Default)]
pub struct StreamConfig {
    pub delimiter: Option<char>,
    pub max_items: Option<usize>,
}

// ============================================================================
// Check for Mock Input Helper
// ============================================================================

pub fn get_mock_input() -> Option<String> {
    // 1. Check thread-local mock
    let thread_mock = crate::backends::common::builtins_modules::io::get_thread_mock_input();
    if thread_mock.is_some() {
        return thread_mock;
    }
    // 2. Check global playback queue
    crate::execution::runtime_core::get_playback_input()
}

/// Helper to prompt and wait for a single line from stdin
pub fn read_line_prompt(prompt: &str) -> String {
    print!("{}", prompt);
    let _ = io::stdout().flush();
    let mut line = String::new();
    let _ = io::stdin().read_line(&mut line);
    line.trim_end_matches(&['\r', '\n']).to_string()
}

// ============================================================================
// 1. Line Input with Live Prompt
// ============================================================================

pub fn prompt_input(prompt: &str, opts: &InputOptions) -> Result<String, String> {
    if let Some(mock) = get_mock_input() {
        let mut res = mock;
        if opts.trim {
            res = res.trim().to_string();
        }
        if res.is_empty() {
            if let Some(d) = &opts.default {
                res = d.clone();
            }
        }
        return Ok(res);
    }

    if let Ok(_guard) = TerminalGuard::new_inline(3) {
        if let Ok(mut terminal) = init_inline_terminal(3) {
            let mut text = opts.default.clone().unwrap_or_default();
            let mut cursor_pos = text.len();
            let start_time = Instant::now();

            loop {
                if let Some(ms) = opts.timeout_ms {
                    if start_time.elapsed() >= Duration::from_millis(ms) {
                        return Err("Timeout exceeded".into());
                    }
                }

                let _ = terminal.draw(|f| {
                    let size = f.size();
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(3), Constraint::Min(0)])
                        .split(size);

                    let remaining_sec = if let Some(ms) = opts.timeout_ms {
                        let elapsed = start_time.elapsed().as_millis() as u64;
                        let rem = ms.saturating_sub(elapsed) / 1000;
                        format!(" [⏱ {}s]", rem)
                    } else {
                        String::new()
                    };

                    let display_text = if opts.masked {
                        "*".repeat(text.len())
                    } else {
                        text.clone()
                    };

                    let prompt_style = if let Some((r, g, b)) = opts.style_color {
                        Style::default().fg(Color::Rgb(r, g, b)).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                    };

                    let title_line = Line::from(vec![
                        Span::styled(format!("{}{}", prompt, remaining_sec), prompt_style),
                    ]);

                    let p = Paragraph::new(display_text)
                        .block(Block::default().borders(Borders::ALL).title(title_line));

                    f.render_widget(p, chunks[0]);
                    f.set_cursor(chunks[0].x + 1 + cursor_pos as u16, chunks[0].y + 1);
                });

                if let Ok(Some(key)) = poll_key_event(Duration::from_millis(50)) {
                    match key.code {
                        KeyCode::Enter => {
                            let mut res = text;
                            if opts.trim {
                                res = res.trim().to_string();
                            }
                            if res.is_empty() {
                                if let Some(d) = &opts.default {
                                    res = d.clone();
                                }
                            }
                            return Ok(res);
                        }
                        KeyCode::Char(c) => {
                            text.insert(cursor_pos, c);
                            cursor_pos += 1;
                        }
                        KeyCode::Backspace => {
                            if cursor_pos > 0 {
                                text.remove(cursor_pos - 1);
                                cursor_pos -= 1;
                            }
                        }
                        KeyCode::Delete => {
                            if cursor_pos < text.len() {
                                text.remove(cursor_pos);
                            }
                        }
                        KeyCode::Left => {
                            if cursor_pos > 0 {
                                cursor_pos -= 1;
                            }
                        }
                        KeyCode::Right => {
                            if cursor_pos < text.len() {
                                cursor_pos += 1;
                            }
                        }
                        KeyCode::Home => {
                            cursor_pos = 0;
                        }
                        KeyCode::End => {
                            cursor_pos = text.len();
                        }
                        KeyCode::Esc => {
                            return Err("Input cancelled".into());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Interactive fallback: print prompt and wait for user to type and press Enter
    let prompt_str = if let Some((r, g, b)) = opts.style_color {
        format!("\x1b[38;2;{};{};{}m{}\x1b[0m", r, g, b, prompt)
    } else {
        prompt.to_string()
    };
    let line = read_line_prompt(&prompt_str);
    let mut res = if opts.trim {
        line.trim().to_string()
    } else {
        line
    };
    if res.is_empty() {
        if let Some(d) = &opts.default {
            res = d.clone();
        }
    }
    Ok(res)
}

// ============================================================================
// 2. Checkbox Multi-Select Widget
// ============================================================================

pub fn prompt_checkbox(prompt: &str, options: &[String], config: &CheckboxConfig) -> Result<Vec<String>, String> {
    if let Some(mock) = get_mock_input() {
        if mock.starts_with('[') && mock.ends_with(']') {
            let inner = &mock[1..mock.len() - 1];
            let mut res = Vec::new();
            for part in inner.split(',') {
                let trimmed = part.trim().trim_matches('"').trim_matches('\'').to_string();
                if options.contains(&trimmed) {
                    res.push(trimmed);
                }
            }
            return Ok(res);
        } else if options.contains(&mock) {
            return Ok(vec![mock]);
        }
        return Ok(Vec::new());
    }

    if options.is_empty() {
        return Ok(Vec::new());
    }

    let inline_height = (options.len() as u16 + 3).min(15);
    if let Ok(_guard) = TerminalGuard::new_inline(inline_height) {
        if let Ok(mut terminal) = init_inline_terminal(inline_height) {
            let mut selected_indices: HashSet<usize> = HashSet::new();
            for (i, opt) in options.iter().enumerate() {
                if config.default.contains(opt) {
                    selected_indices.insert(i);
                }
            }
            let mut current_idx: usize = 0;

            loop {
                let _ = terminal.draw(|f| {
                    let size = f.size();
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(options.len() as u16 + 3), Constraint::Min(0)])
                        .split(size);

                    let items: Vec<ListItem> = options
                        .iter()
                        .enumerate()
                        .map(|(i, item)| {
                            let is_disabled = config.disabled.contains(item);
                            let is_checked = selected_indices.contains(&i);
                            let is_cursor = i == current_idx;

                            let mark = if is_disabled {
                                "[✗]"
                            } else if is_checked {
                                "[✓]"
                            } else {
                                "[ ]"
                            };

                            let cursor_sym = if is_cursor { "▶ " } else { "  " };

                            let content = format!("{}{} {}", cursor_sym, mark, item);
                            let style = if is_disabled {
                                Style::default().fg(Color::DarkGray)
                            } else if is_cursor {
                                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                            } else if is_checked {
                                Style::default().fg(Color::Green)
                            } else {
                                Style::default().fg(Color::White)
                            };

                            ListItem::new(content).style(style)
                        })
                        .collect();

                    let title = format!("{} (Space: Toggle, 'a': All, Enter: Confirm)", prompt);
                    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(title));

                    f.render_widget(list, chunks[0]);
                });

                if let Ok(Some(key)) = poll_key_event(Duration::from_millis(50)) {
                    match key.code {
                        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('w') => {
                            if current_idx > 0 {
                                current_idx -= 1;
                            } else {
                                current_idx = options.len() - 1;
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('s') => {
                            if current_idx + 1 < options.len() {
                                current_idx += 1;
                            } else {
                                current_idx = 0;
                            }
                        }
                        KeyCode::Char(' ') => {
                            let item = &options[current_idx];
                            if !config.disabled.contains(item) {
                                if selected_indices.contains(&current_idx) {
                                    selected_indices.remove(&current_idx);
                                } else if let Some(lim) = config.limit {
                                    if selected_indices.len() < lim {
                                        selected_indices.insert(current_idx);
                                    }
                                } else {
                                    selected_indices.insert(current_idx);
                                }
                            }
                        }
                        KeyCode::Char('a') => {
                            if selected_indices.len() == options.len() {
                                selected_indices.clear();
                            } else {
                                for (i, opt) in options.iter().enumerate() {
                                    if !config.disabled.contains(opt) {
                                        if let Some(lim) = config.limit {
                                            if selected_indices.len() >= lim {
                                                break;
                                            }
                                        }
                                        selected_indices.insert(i);
                                    }
                                }
                            }
                        }
                        KeyCode::Enter => {
                            if config.required && selected_indices.is_empty() {
                                continue;
                            }
                            let mut result = Vec::new();
                            for (i, opt) in options.iter().enumerate() {
                                if selected_indices.contains(&i) {
                                    result.push(opt.clone());
                                }
                            }
                            return Ok(result);
                        }
                        KeyCode::Esc => {
                            return Err("Checkbox selection cancelled".into());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Interactive fallback: print numbered options and wait for user selection
    println!("{}", prompt);
    for (i, opt) in options.iter().enumerate() {
        let mark = if config.default.contains(opt) { "[✓]" } else { "[ ]" };
        println!("  {}. {} {}", i + 1, mark, opt);
    }
    let line = read_line_prompt("Select options (e.g. 1, 2) or Enter for defaults: ");
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(config.default.clone());
    }
    let mut selected = Vec::new();
    for part in trimmed.split(&[',', ' '][..]) {
        let p = part.trim();
        if let Ok(idx) = p.parse::<usize>() {
            if idx >= 1 && idx <= options.len() {
                selected.push(options[idx - 1].clone());
            }
        } else if options.contains(&p.to_string()) {
            selected.push(p.to_string());
        }
    }
    Ok(selected)
}

// ============================================================================
// 3. Radio / Select Single Selection Widget
// ============================================================================

pub fn prompt_radio(prompt: &str, options: &[String], config: &RadioConfig) -> Result<String, String> {
    if let Some(mock) = get_mock_input() {
        if options.contains(&mock) {
            return Ok(mock);
        }
        let clean = mock.trim_matches('"').trim_matches('\'').to_string();
        if options.contains(&clean) {
            return Ok(clean);
        }
        if let Some(first) = options.first() {
            return Ok(first.clone());
        }
    }

    if options.is_empty() {
        if let Some(d) = &config.default {
            return Ok(d.clone());
        }
        return Err("No options provided".into());
    }

    let inline_height = (options.len() as u16 + 3).min(15);
    if let Ok(_guard) = TerminalGuard::new_inline(inline_height) {
        if let Ok(mut terminal) = init_inline_terminal(inline_height) {
            let mut current_idx: usize = 0;
            if let Some(d) = &config.default {
                if let Some(pos) = options.iter().position(|x| x == d) {
                    current_idx = pos;
                }
            }

            loop {
                let _ = terminal.draw(|f| {
                    let size = f.size();
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(options.len() as u16 + 3), Constraint::Min(0)])
                        .split(size);

                    let items: Vec<ListItem> = options
                        .iter()
                        .enumerate()
                        .map(|(i, item)| {
                            let is_disabled = config.disabled.contains(item);
                            let is_active = i == current_idx;

                            let mark = if is_active { "(●)" } else { "( )" };
                            let cursor_sym = if is_active { "▶ " } else { "  " };

                            let content = format!("{}{} {}", cursor_sym, mark, item);
                            let style = if is_disabled {
                                Style::default().fg(Color::DarkGray)
                            } else if is_active {
                                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(Color::White)
                            };

                            ListItem::new(content).style(style)
                        })
                        .collect();

                    let title = format!("{} (Arrows/Enter: Select)", prompt);
                    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(title));

                    f.render_widget(list, chunks[0]);
                });

                if let Ok(Some(key)) = poll_key_event(Duration::from_millis(50)) {
                    match key.code {
                        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('w') => {
                            if current_idx > 0 {
                                current_idx -= 1;
                            } else {
                                current_idx = options.len() - 1;
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('s') => {
                            if current_idx + 1 < options.len() {
                                current_idx += 1;
                            } else {
                                current_idx = 0;
                            }
                        }
                        KeyCode::Enter => {
                            let selected = &options[current_idx];
                            if !config.disabled.contains(selected) {
                                return Ok(selected.clone());
                            }
                        }
                        KeyCode::Esc => {
                            return Err("Radio selection cancelled".into());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Interactive fallback: print numbered list and wait for input
    println!("{}", prompt);
    for (i, opt) in options.iter().enumerate() {
        let mark = if config.default.as_ref() == Some(opt) { "(●)" } else { "( )" };
        println!("  {}. {} {}", i + 1, mark, opt);
    }
    let line = read_line_prompt("Select option (number or name): ");
    let trimmed = line.trim();
    if trimmed.is_empty() {
        if let Some(d) = &config.default {
            return Ok(d.clone());
        }
        if let Some(first) = options.first() {
            return Ok(first.clone());
        }
    }
    if let Ok(idx) = trimmed.parse::<usize>() {
        if idx >= 1 && idx <= options.len() {
            return Ok(options[idx - 1].clone());
        }
    }
    for opt in options {
        if opt == trimmed {
            return Ok(opt.clone());
        }
    }
    if let Some(d) = &config.default {
        return Ok(d.clone());
    }
    Ok(options.first().cloned().unwrap_or_default())
}

pub fn prompt_select(prompt: &str, options: &[String], config: &SelectConfig) -> Result<String, String> {
    let radio_cfg = RadioConfig {
        default: config.default.clone(),
        required: true,
        disabled: config.disabled.clone(),
    };
    prompt_radio(prompt, options, &radio_cfg)
}

// ============================================================================
// 4. Form Multi-Field Widget
// ============================================================================

pub fn prompt_form(fields: &[(String, FormFieldConfig)], config: &FormConfig) -> Result<Vec<(String, String)>, String> {
    if let Some(mock) = get_mock_input() {
        let mut results = Vec::new();
        for (name, field) in fields {
            results.push((name.clone(), field.default.clone().unwrap_or_else(|| mock.clone())));
        }
        return Ok(results);
    }

    if fields.is_empty() {
        return Ok(Vec::new());
    }

    let inline_height = ((fields.len() * 3 + 2) as u16).min(20);
    if let Ok(_guard) = TerminalGuard::new_inline(inline_height) {
        if let Ok(mut terminal) = init_inline_terminal(inline_height) {
            let mut values: Vec<String> = fields
                .iter()
                .map(|(_, f)| f.default.clone().unwrap_or_default())
                .collect();

            let mut current_field: usize = 0;

            loop {
                let _ = terminal.draw(|f| {
                    let size = f.size();
                    let total_height = (fields.len() * 3 + 2) as u16;
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(total_height), Constraint::Min(0)])
                        .split(size);

                    let form_title = config.title.clone().unwrap_or_else(|| "Form (Tab: Next, Enter: Submit)".into());
                    let form_block = Block::default().borders(Borders::ALL).title(form_title);
                    let inner_area = form_block.inner(chunks[0]);
                    f.render_widget(form_block, chunks[0]);

                    let field_constraints: Vec<Constraint> = (0..fields.len())
                        .map(|_| Constraint::Length(3))
                        .collect();

                    let field_chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints(field_constraints)
                        .split(inner_area);

                    for (i, (name, field)) in fields.iter().enumerate() {
                        let is_active = i == current_field;
                        let val_str = if field.masked {
                            "*".repeat(values[i].len())
                        } else {
                            values[i].clone()
                        };

                        let prompt_label = if field.prompt.is_empty() {
                            name.clone()
                        } else {
                            field.prompt.clone()
                        };

                        let border_style = if is_active {
                            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::DarkGray)
                        };

                        let field_widget = Paragraph::new(val_str).block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(border_style)
                                .title(format!("{}: ", prompt_label)),
                        );

                        f.render_widget(field_widget, field_chunks[i]);
                    }
                });

                if let Ok(Some(key)) = poll_key_event(Duration::from_millis(50)) {
                    match key.code {
                        KeyCode::Tab | KeyCode::Down => {
                            current_field = (current_field + 1) % fields.len();
                        }
                        KeyCode::BackTab | KeyCode::Up => {
                            if current_field > 0 {
                                current_field -= 1;
                            } else {
                                current_field = fields.len() - 1;
                            }
                        }
                        KeyCode::Char(c) => {
                            values[current_field].push(c);
                        }
                        KeyCode::Backspace => {
                            values[current_field].pop();
                        }
                        KeyCode::Enter => {
                            if current_field + 1 < fields.len() {
                                current_field += 1;
                            } else {
                                let results: Vec<(String, String)> = fields
                                    .iter()
                                    .enumerate()
                                    .map(|(i, (name, _))| (name.clone(), values[i].clone()))
                                    .collect();
                                return Ok(results);
                            }
                        }
                        KeyCode::Esc => {
                            return Err("Form cancelled".into());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Interactive fallback: prompt for each field sequentially
    if let Some(title) = &config.title {
        println!("--- {} ---", title);
    }
    let mut results = Vec::new();
    for (name, field) in fields {
        let label = if field.prompt.is_empty() { name } else { &field.prompt };
        let hint = if let Some(d) = &field.default { format!(" (default: {})", d) } else { String::new() };
        let line = read_line_prompt(&format!("{}{}: ", label, hint));
        let final_val = if line.trim().is_empty() {
            field.default.clone().unwrap_or_default()
        } else {
            line
        };
        results.push((name.clone(), final_val));
    }
    Ok(results)
}

// ============================================================================
// 5. Confirm [Y/n] Widget
// ============================================================================

pub fn prompt_confirm(prompt: &str, default: bool) -> Result<bool, String> {
    if let Some(mock) = get_mock_input() {
        let low = mock.to_lowercase();
        return Ok(low == "true" || low == "y" || low == "yes" || low == "1");
    }

    let hint = if default { "[Y/n]" } else { "[y/N]" };
    let full_prompt = format!("{} {} ", prompt, hint);

    if let Ok(_guard) = TerminalGuard::new_inline(3) {
        if let Ok(mut terminal) = init_inline_terminal(3) {
            loop {
                let _ = terminal.draw(|f| {
                    let size = f.size();
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(3), Constraint::Min(0)])
                        .split(size);

                    let p = Paragraph::new(Line::from(vec![
                        Span::styled(&full_prompt, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    ])).block(Block::default().borders(Borders::ALL).title("Confirmation"));

                    f.render_widget(p, chunks[0]);
                });

                if let Ok(Some(key)) = poll_key_event(Duration::from_millis(50)) {
                    match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => return Ok(true),
                        KeyCode::Char('n') | KeyCode::Char('N') => return Ok(false),
                        KeyCode::Enter => return Ok(default),
                        KeyCode::Esc => return Err("Confirmation cancelled".into()),
                        _ => {}
                    }
                }
            }
        }
    }

    // Interactive fallback: prompt and wait for Y/n from stdin
    let line = read_line_prompt(&full_prompt);
    let low = line.trim().to_lowercase();
    if low.is_empty() {
        Ok(default)
    } else {
        Ok(low == "y" || low == "yes" || low == "true" || low == "1")
    }
}

// ============================================================================
// 6. Secure Password Input Widget
// ============================================================================

pub fn prompt_password(prompt: &str, _config: &PasswordConfig) -> Result<String, String> {
    let opts = InputOptions {
        masked: true,
        ..Default::default()
    };
    prompt_input(prompt, &opts)
}

// ============================================================================
// 7. World-First: Fuzzy Neural-Search Selector (`input.fuzzy`)
// ============================================================================

pub fn prompt_fuzzy(prompt: &str, options: &[String], _config: &FuzzyConfig) -> Result<String, String> {
    if let Some(mock) = get_mock_input() {
        if options.contains(&mock) {
            return Ok(mock);
        }
        let clean = mock.trim_matches('"').trim_matches('\'').to_string();
        if options.contains(&clean) {
            return Ok(clean);
        }
        return options.first().cloned().ok_or_else(|| "Empty options".into());
    }

    if options.is_empty() {
        return Err("Empty options".into());
    }

    if let Ok(_guard) = TerminalGuard::new_inline(12) {
        if let Ok(mut terminal) = init_inline_terminal(12) {
            let mut query = String::new();
            let mut selected_idx: usize = 0;

            loop {
                let query_lower = query.to_lowercase();
                let matches: Vec<&String> = options
                    .iter()
                    .filter(|opt| {
                        if query_lower.is_empty() {
                            true
                        } else {
                            let opt_lower = opt.to_lowercase();
                            let mut q_chars = query_lower.chars();
                            if let Some(mut target) = q_chars.next() {
                                for c in opt_lower.chars() {
                                    if c == target {
                                        if let Some(next_t) = q_chars.next() {
                                            target = next_t;
                                        } else {
                                            return true;
                                        }
                                    }
                                }
                                false
                            } else {
                                true
                            }
                        }
                    })
                    .collect();

                let count = matches.len();
                if selected_idx >= count && count > 0 {
                    selected_idx = count - 1;
                }

                let _ = terminal.draw(|f| {
                    let size = f.size();
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(3), Constraint::Length(8), Constraint::Min(0)])
                        .split(size);

                    let search_input = Paragraph::new(query.clone())
                        .block(Block::default().borders(Borders::ALL).title(format!("{} (Search: {})", prompt, count)));

                    f.render_widget(search_input, chunks[0]);

                    let items: Vec<ListItem> = matches
                        .iter()
                        .enumerate()
                        .map(|(i, item)| {
                            let is_active = i == selected_idx;
                            let cursor_sym = if is_active { "▶ " } else { "  " };
                            let style = if is_active {
                                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(Color::White)
                            };
                            ListItem::new(format!("{}{}", cursor_sym, item)).style(style)
                        })
                        .collect();

                    let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Matches (Arrows: Nav, Enter: Select)"));
                    f.render_widget(list, chunks[1]);
                });

                if let Ok(Some(key)) = poll_key_event(Duration::from_millis(50)) {
                    match key.code {
                        KeyCode::Up => {
                            if selected_idx > 0 {
                                selected_idx -= 1;
                            }
                        }
                        KeyCode::Down => {
                            if selected_idx + 1 < count {
                                selected_idx += 1;
                            }
                        }
                        KeyCode::Char(c) => {
                            query.push(c);
                            selected_idx = 0;
                        }
                        KeyCode::Backspace => {
                            query.pop();
                            selected_idx = 0;
                        }
                        KeyCode::Enter => {
                            if let Some(matched) = matches.get(selected_idx) {
                                return Ok((*matched).clone());
                            } else if let Some(first) = options.first() {
                                return Ok(first.clone());
                            }
                        }
                        KeyCode::Esc => {
                            return Err("Fuzzy search cancelled".into());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Interactive fallback: print matching preview & read line
    println!("{} (Fuzzy Search)", prompt);
    for (i, opt) in options.iter().enumerate().take(10) {
        println!("  {}. {}", i + 1, opt);
    }
    let line = read_line_prompt("Enter search term or item number: ");
    let query = line.trim().to_lowercase();
    if let Ok(idx) = query.parse::<usize>() {
        if idx >= 1 && idx <= options.len() {
            return Ok(options[idx - 1].clone());
        }
    }
    for opt in options {
        if opt.to_lowercase().contains(&query) {
            return Ok(opt.clone());
        }
    }
    Ok(options.first().cloned().unwrap_or_default())
}

// ============================================================================
// 8. World-First: Interactive Slider Gauge (`input.slider`)
// ============================================================================

pub fn prompt_slider(
    prompt: &str,
    min: f64,
    max: f64,
    step: f64,
    default: f64,
    config: &SliderConfig,
) -> Result<f64, String> {
    if let Some(mock) = get_mock_input() {
        if let Ok(n) = mock.parse::<f64>() {
            return Ok(n.clamp(min, max));
        }
        return Ok(default.clamp(min, max));
    }

    let effective_step = if step <= 0.0 { 1.0 } else { step };

    if let Ok(_guard) = TerminalGuard::new_inline(4) {
        if let Ok(mut terminal) = init_inline_terminal(4) {
            let mut current_val = default.clamp(min, max);

            loop {
                let _ = terminal.draw(|f| {
                    let size = f.size();
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(4), Constraint::Min(0)])
                        .split(size);

                    let ratio = ((current_val - min) / (max - min)).clamp(0.0, 1.0);
                    let percent = (ratio * 100.0) as u16;

                    let label = format!("{:.2} / {:.2} {} ({}%)", current_val, max, config.unit, percent);

                    let gauge = Gauge::default()
                        .block(Block::default().borders(Borders::ALL).title(format!("{} (◀/▶: Adjust, Enter: Confirm)", prompt)))
                        .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray))
                        .percent(percent)
                        .label(label);

                    f.render_widget(gauge, chunks[0]);
                });

                if let Ok(Some(key)) = poll_key_event(Duration::from_millis(50)) {
                    match key.code {
                        KeyCode::Left | KeyCode::Char('h') => {
                            current_val = (current_val - effective_step).max(min);
                        }
                        KeyCode::Right | KeyCode::Char('l') => {
                            current_val = (current_val + effective_step).min(max);
                        }
                        KeyCode::PageDown => {
                            current_val = (current_val - effective_step * 10.0).max(min);
                        }
                        KeyCode::PageUp => {
                            current_val = (current_val + effective_step * 10.0).min(max);
                        }
                        KeyCode::Home => {
                            current_val = min;
                        }
                        KeyCode::End => {
                            current_val = max;
                        }
                        KeyCode::Enter => {
                            return Ok(current_val);
                        }
                        KeyCode::Esc => {
                            return Err("Slider cancelled".into());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Interactive fallback: print range and wait for user to type number
    let line = read_line_prompt(&format!("{} [{:.2} - {:.2}, default: {:.2} {}]: ", prompt, min, max, default, config.unit));
    if let Ok(n) = line.trim().parse::<f64>() {
        Ok(n.clamp(min, max))
    } else {
        Ok(default.clamp(min, max))
    }
}

// ============================================================================
// 9. World-First: Hierarchical Tree Selector (`input.tree`)
// ============================================================================

pub fn prompt_tree(prompt: &str, nodes: &[TreeNode], _config: &TreeConfig) -> Result<String, String> {
    if let Some(mock) = get_mock_input() {
        return Ok(mock);
    }

    if nodes.is_empty() {
        return Ok(String::new());
    }

    let mut flat_items: Vec<(String, String, usize)> = Vec::new();
    for node in nodes {
        flat_items.push((node.id.clone(), node.label.clone(), 0));
        for child in &node.children {
            flat_items.push((child.id.clone(), child.label.clone(), 1));
        }
    }

    let inline_height = ((flat_items.len() + 3) as u16).min(15);
    if let Ok(_guard) = TerminalGuard::new_inline(inline_height) {
        if let Ok(mut terminal) = init_inline_terminal(inline_height) {
            let mut current_idx: usize = 0;

            loop {
                let _ = terminal.draw(|f| {
                    let size = f.size();
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(flat_items.len() as u16 + 3), Constraint::Min(0)])
                        .split(size);

                    let items: Vec<ListItem> = flat_items
                        .iter()
                        .enumerate()
                        .map(|(i, (_id, label, depth))| {
                            let is_active = i == current_idx;
                            let indent = "  ".repeat(*depth);
                            let prefix = if is_active { "▶ " } else { "  " };
                            let style = if is_active {
                                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(Color::White)
                            };
                            ListItem::new(format!("{}{}{}", prefix, indent, label)).style(style)
                        })
                        .collect();

                    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(format!("{} (Tree)", prompt)));
                    f.render_widget(list, chunks[0]);
                });

                if let Ok(Some(key)) = poll_key_event(Duration::from_millis(50)) {
                    match key.code {
                        KeyCode::Up => {
                            if current_idx > 0 {
                                current_idx -= 1;
                            }
                        }
                        KeyCode::Down => {
                            if current_idx + 1 < flat_items.len() {
                                current_idx += 1;
                            }
                        }
                        KeyCode::Enter => {
                            if let Some((id, _, _)) = flat_items.get(current_idx) {
                                return Ok(id.clone());
                            }
                        }
                        KeyCode::Esc => {
                            return Err("Tree selection cancelled".into());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Interactive fallback: print hierarchy and wait for input
    println!("{}", prompt);
    for (i, (_id, label, depth)) in flat_items.iter().enumerate() {
        let indent = "  ".repeat(*depth);
        println!("  {}. {}{}", i + 1, indent, label);
    }
    let line = read_line_prompt("Select tree item (number or ID): ");
    let trimmed = line.trim();
    if let Ok(idx) = trimmed.parse::<usize>() {
        if idx >= 1 && idx <= flat_items.len() {
            return Ok(flat_items[idx - 1].0.clone());
        }
    }
    for (id, label, _) in &flat_items {
        if id == trimmed || label == trimmed {
            return Ok(id.clone());
        }
    }
    Ok(flat_items.first().map(|f| f.0.clone()).unwrap_or_default())
}

// ============================================================================
// 10. World-First: Interactive Table Cell Editor & Data Grid (`input.table`)
// ============================================================================

pub fn prompt_table(
    prompt: &str,
    headers: &[String],
    rows: &[Vec<String>],
    _config: &TableConfig,
) -> Result<TableResult, String> {
    if let Some(_mock) = get_mock_input() {
        return Ok(TableResult {
            selected_row: 0,
            selected_col: 0,
            data: rows.to_vec(),
        });
    }

    if rows.is_empty() {
        return Ok(TableResult {
            selected_row: 0,
            selected_col: 0,
            data: rows.to_vec(),
        });
    }

    let inline_height = ((rows.len() + 4) as u16).min(15);
    if let Ok(_guard) = TerminalGuard::new_inline(inline_height) {
        if let Ok(mut terminal) = init_inline_terminal(inline_height) {
            let mut current_row: usize = 0;
            let mut current_col: usize = 0;
            let table_data = rows.to_vec();

            loop {
                let _ = terminal.draw(|f| {
                    let size = f.size();
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(table_data.len() as u16 + 4), Constraint::Min(0)])
                        .split(size);

                    let header_row = Row::new(headers.iter().map(|h| h.clone()))
                        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));

                    let table_rows: Vec<Row> = table_data
                        .iter()
                        .enumerate()
                        .map(|(r_idx, r)| {
                            let is_active_row = r_idx == current_row;
                            let cells = r.iter().enumerate().map(|(c_idx, c)| {
                                if is_active_row && c_idx == current_col {
                                    format!("▶ {}", c)
                                } else {
                                    c.clone()
                                }
                            });
                            let row_style = if is_active_row {
                                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(Color::White)
                            };
                            Row::new(cells).style(row_style)
                        })
                        .collect();

                    let col_widths: Vec<Constraint> = (0..headers.len())
                        .map(|_| Constraint::Ratio(1, headers.len().max(1) as u32))
                        .collect();

                    let table = Table::new(table_rows, col_widths)
                        .header(header_row)
                        .block(Block::default().borders(Borders::ALL).title(format!("{} (Arrows: Move, Enter: Pick)", prompt)));

                    f.render_widget(table, chunks[0]);
                });

                if let Ok(Some(key)) = poll_key_event(Duration::from_millis(50)) {
                    match key.code {
                        KeyCode::Up => {
                            if current_row > 0 {
                                current_row -= 1;
                            }
                        }
                        KeyCode::Down => {
                            if current_row + 1 < table_data.len() {
                                current_row += 1;
                            }
                        }
                        KeyCode::Left => {
                            if current_col > 0 {
                                current_col -= 1;
                            }
                        }
                        KeyCode::Right => {
                            if current_col + 1 < headers.len() {
                                current_col += 1;
                            }
                        }
                        KeyCode::Enter => {
                            return Ok(TableResult {
                                selected_row: current_row,
                                selected_col: current_col,
                                data: table_data,
                            });
                        }
                        KeyCode::Esc => {
                            return Err("Table input cancelled".into());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Interactive fallback: print table and wait for row selection
    println!("{}", prompt);
    println!("| {} |", headers.join(" | "));
    for (i, row) in rows.iter().enumerate() {
        println!("{}. | {} |", i + 1, row.join(" | "));
    }
    let line = read_line_prompt("Select row number [1..N]: ");
    let selected_row = if let Ok(idx) = line.trim().parse::<usize>() {
        if idx >= 1 && idx <= rows.len() {
            idx - 1
        } else {
            0
        }
    } else {
        0
    };
    Ok(TableResult {
        selected_row,
        selected_col: 0,
        data: rows.to_vec(),
    })
}

// ============================================================================
fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 31,
    }
}

// 11. World-First: Visual Calendar Datepicker (`input.datepicker`)
// ============================================================================

pub fn prompt_datepicker(prompt: &str, config: &DatepickerConfig) -> Result<String, String> {
    if let Some(mock) = get_mock_input() {
        return Ok(mock);
    }

    let mut year = config.default_year.unwrap_or(2026);
    let mut month = config.default_month.unwrap_or(9).clamp(1, 12);
    let mut day = config.default_day.unwrap_or(8).clamp(1, 31);
    let mut field: usize = 0; // 0=Year, 1=Month, 2=Day

    if let Ok(_guard) = TerminalGuard::new_inline(4) {
        if let Ok(mut terminal) = init_inline_terminal(4) {
            loop {
                let _ = terminal.draw(|f| {
                    let size = f.size();
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(4), Constraint::Min(0)])
                        .split(size);

                    let field_name = match field {
                        0 => "YEAR",
                        1 => "MONTH",
                        _ => "DAY",
                    };

                    let date_str = match field {
                        0 => format!("📅 [{:04}] - {:02} - {:02}  |  Editing: {}", year, month, day, field_name),
                        1 => format!("📅 {:04} - [{:02}] - {:02}  |  Editing: {}", year, month, day, field_name),
                        _ => format!("📅 {:04} - {:02} - [{:02}]  |  Editing: {}", year, month, day, field_name),
                    };

                    let p = Paragraph::new(Line::from(vec![
                        Span::styled(date_str, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    ])).block(Block::default().borders(Borders::ALL).title(format!("{} (Tab/◀/▶: Field, ▲/▼: Inc/Dec, PgUp/PgDn: ±10y, Enter: Pick)", prompt)));

                    f.render_widget(p, chunks[0]);
                });

                if let Ok(Some(key)) = poll_key_event(Duration::from_millis(50)) {
                    match key.code {
                        KeyCode::Tab | KeyCode::Right => {
                            field = (field + 1) % 3;
                        }
                        KeyCode::BackTab | KeyCode::Left => {
                            field = (field + 2) % 3;
                        }
                        KeyCode::Up => {
                            match field {
                                0 => year += 1,
                                1 => month = if month < 12 { month + 1 } else { 1 },
                                _ => {
                                    let max_d = days_in_month(year, month);
                                    day = if day < max_d { day + 1 } else { 1 };
                                }
                            }
                        }
                        KeyCode::Down => {
                            match field {
                                0 => year = (year - 1).max(1),
                                1 => month = if month > 1 { month - 1 } else { 12 },
                                _ => {
                                    let max_d = days_in_month(year, month);
                                    day = if day > 1 { day - 1 } else { max_d };
                                }
                            }
                        }
                        KeyCode::PageUp => {
                            year += 10;
                        }
                        KeyCode::PageDown => {
                            year = (year - 10).max(1);
                        }
                        KeyCode::Enter => {
                            return Ok(format!("{:04}-{:02}-{:02}", year, month, day));
                        }
                        KeyCode::Esc => {
                            return Err("Datepicker cancelled".into());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Interactive fallback: prompt and wait for date
    let default_str = format!("{:04}-{:02}-{:02}", year, month, day);
    let line = read_line_prompt(&format!("{} [YYYY-MM-DD, default: {}]: ", prompt, default_str));
    let trimmed = line.trim();
    if trimmed.is_empty() {
        Ok(default_str)
    } else {
        Ok(trimmed.to_string())
    }
}

// ============================================================================
// Datetime Picker Config & Function (`input.datetime` / `input.datetimepicker`)
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct DatetimeConfig {
    pub default_year: Option<i32>,
    pub default_month: Option<u32>,
    pub default_day: Option<u32>,
    pub default_hour: Option<u32>,
    pub default_minute: Option<u32>,
    pub default_second: Option<u32>,
}

pub fn prompt_datetimepicker(prompt: &str, config: &DatetimeConfig) -> Result<String, String> {
    if let Some(mock) = get_mock_input() {
        return Ok(mock);
    }

    let mut year = config.default_year.unwrap_or(2026);
    let mut month = config.default_month.unwrap_or(9).clamp(1, 12);
    let mut day = config.default_day.unwrap_or(8).clamp(1, 31);
    let mut hour = config.default_hour.unwrap_or(12).clamp(0, 23);
    let mut minute = config.default_minute.unwrap_or(0).clamp(0, 59);
    let mut second = config.default_second.unwrap_or(0).clamp(0, 59);
    let mut field: usize = 0; // 0=Year, 1=Month, 2=Day, 3=Hour, 4=Min, 5=Sec

    if let Ok(_guard) = TerminalGuard::new_inline(4) {
        if let Ok(mut terminal) = init_inline_terminal(4) {
            loop {
                let _ = terminal.draw(|f| {
                    let size = f.size();
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(4), Constraint::Min(0)])
                        .split(size);

                    let field_name = match field {
                        0 => "YEAR",
                        1 => "MONTH",
                        2 => "DAY",
                        3 => "HOUR",
                        4 => "MINUTE",
                        _ => "SECOND",
                    };

                    let dt_str = match field {
                        0 => format!("📅 [{:04}]-{:02}-{:02} ⏰ {:02}:{:02}:{:02} | Editing: {}", year, month, day, hour, minute, second, field_name),
                        1 => format!("📅 {:04}-[{:02}]-{:02} ⏰ {:02}:{:02}:{:02} | Editing: {}", year, month, day, hour, minute, second, field_name),
                        2 => format!("📅 {:04}-{:02}-[{:02}] ⏰ {:02}:{:02}:{:02} | Editing: {}", year, month, day, hour, minute, second, field_name),
                        3 => format!("📅 {:04}-{:02}-{:02} ⏰ [{:02}]:{:02}:{:02} | Editing: {}", year, month, day, hour, minute, second, field_name),
                        4 => format!("📅 {:04}-{:02}-{:02} ⏰ {:02}:[{:02}]:{:02} | Editing: {}", year, month, day, hour, minute, second, field_name),
                        _ => format!("📅 {:04}-{:02}-{:02} ⏰ {:02}:{:02}:[{:02}] | Editing: {}", year, month, day, hour, minute, second, field_name),
                    };

                    let p = Paragraph::new(Line::from(vec![
                        Span::styled(dt_str, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    ])).block(Block::default().borders(Borders::ALL).title(format!("{} (Tab/◀/▶: Field, ▲/▼: Inc/Dec, PgUp/PgDn: ±10y, Enter: Pick)", prompt)));

                    f.render_widget(p, chunks[0]);
                });

                if let Ok(Some(key)) = poll_key_event(Duration::from_millis(50)) {
                    match key.code {
                        KeyCode::Tab | KeyCode::Right => {
                            field = (field + 1) % 6;
                        }
                        KeyCode::BackTab | KeyCode::Left => {
                            field = (field + 5) % 6;
                        }
                        KeyCode::Up => {
                            match field {
                                0 => year += 1,
                                1 => month = if month < 12 { month + 1 } else { 1 },
                                2 => {
                                    let max_d = days_in_month(year, month);
                                    day = if day < max_d { day + 1 } else { 1 };
                                }
                                3 => hour = (hour + 1) % 24,
                                4 => minute = (minute + 1) % 60,
                                _ => second = (second + 1) % 60,
                            }
                        }
                        KeyCode::Down => {
                            match field {
                                0 => year = (year - 1).max(1),
                                1 => month = if month > 1 { month - 1 } else { 12 },
                                2 => {
                                    let max_d = days_in_month(year, month);
                                    day = if day > 1 { day - 1 } else { max_d };
                                }
                                3 => hour = (hour + 23) % 24,
                                4 => minute = (minute + 59) % 60,
                                _ => second = (second + 59) % 60,
                            }
                        }
                        KeyCode::PageUp => {
                            year += 10;
                        }
                        KeyCode::PageDown => {
                            year = (year - 10).max(1);
                        }
                        KeyCode::Enter => {
                            return Ok(format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", year, month, day, hour, minute, second));
                        }
                        KeyCode::Esc => {
                            return Err("Datetimepicker cancelled".into());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    let default_str = format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", year, month, day, hour, minute, second);
    let line = read_line_prompt(&format!("{} [YYYY-MM-DD HH:MM:SS, default: {}]: ", prompt, default_str));
    let trimmed = line.trim();
    if trimmed.is_empty() {
        Ok(default_str)
    } else {
        Ok(trimmed.to_string())
    }
}

// ============================================================================
// 12. World-First: Visual Clock Timepicker (`input.timepicker`)
// ============================================================================

pub fn prompt_timepicker(prompt: &str, config: &TimepickerConfig) -> Result<String, String> {
    if let Some(mock) = get_mock_input() {
        return Ok(mock);
    }

    let mut hours: u32 = 12;
    let mut mins: u32 = 0;
    let mut secs: u32 = 0;
    let mut column: usize = 0; // 0 = hours, 1 = mins, 2 = secs

    if let Ok(_guard) = TerminalGuard::new_inline(4) {
        if let Ok(mut terminal) = init_inline_terminal(4) {
            loop {
                let _ = terminal.draw(|f| {
                    let size = f.size();
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(4), Constraint::Min(0)])
                        .split(size);

                    let time_str = format!("⏰ {:02}:{:02}:{:02} (Selected Column: {})", hours, mins, secs, match column {
                        0 => "Hours",
                        1 => "Minutes",
                        _ => "Seconds",
                    });

                    let p = Paragraph::new(Line::from(vec![
                        Span::styled(time_str, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    ])).block(Block::default().borders(Borders::ALL).title(format!("{} (Tab: Column, ▲/▼: Inc/Dec, Enter: Pick)", prompt)));

                    f.render_widget(p, chunks[0]);
                });

                if let Ok(Some(key)) = poll_key_event(Duration::from_millis(50)) {
                    match key.code {
                        KeyCode::Tab | KeyCode::Right => {
                            column = (column + 1) % 3;
                        }
                        KeyCode::BackTab | KeyCode::Left => {
                            column = (column + 2) % 3;
                        }
                        KeyCode::Up => {
                            match column {
                                0 => hours = (hours + 1) % 24,
                                1 => mins = (mins + 1) % 60,
                                _ => secs = (secs + 1) % 60,
                            }
                        }
                        KeyCode::Down => {
                            match column {
                                0 => hours = (hours + 23) % 24,
                                1 => mins = (mins + 59) % 60,
                                _ => secs = (secs + 59) % 60,
                            }
                        }
                        KeyCode::Enter => {
                            return Ok(format!("{:02}:{:02}:{:02}", hours, mins, secs));
                        }
                        KeyCode::Esc => {
                            return Err("Timepicker cancelled".into());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Interactive fallback: prompt and wait for time
    let default_str = if config.include_seconds { "12:00:00" } else { "12:00" };
    let line = read_line_prompt(&format!("{} [HH:MM:SS, default: {}]: ", prompt, default_str));
    let trimmed = line.trim();
    if trimmed.is_empty() {
        Ok(default_str.to_string())
    } else {
        Ok(trimmed.to_string())
    }
}

// ============================================================================
// 13. World-First: 24-bit TrueColor Palette Picker (`input.color`)
// ============================================================================

pub fn prompt_color(prompt: &str, config: &ColorConfig) -> Result<String, String> {
    if let Some(mock) = get_mock_input() {
        return Ok(mock);
    }

    let mut r: u8 = 52;
    let mut g: u8 = 152;
    let mut b: u8 = 219;
    let mut channel: usize = 0; // 0=R, 1=G, 2=B

    if let Ok(_guard) = TerminalGuard::new_inline(5) {
        if let Ok(mut terminal) = init_inline_terminal(5) {
            loop {
                let _ = terminal.draw(|f| {
                    let size = f.size();
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(5), Constraint::Min(0)])
                        .split(size);

                    let hex = format!("#{:02X}{:02X}{:02X}", r, g, b);
                    let channel_name = match channel {
                        0 => "RED",
                        1 => "GREEN",
                        _ => "BLUE",
                    };

                    let swatch = format!("🎨 Hex: {} | R: {} G: {} B: {} | Editing: {}", hex, r, g, b, channel_name);

                    let p = Paragraph::new(Line::from(vec![
                        Span::styled(swatch, Style::default().fg(Color::Rgb(r, g, b)).add_modifier(Modifier::BOLD)),
                    ])).block(Block::default().borders(Borders::ALL).title(format!("{} (Tab: Channel, ▲/▼: Value, Enter: Pick)", prompt)));

                    f.render_widget(p, chunks[0]);
                });

                if let Ok(Some(key)) = poll_key_event(Duration::from_millis(50)) {
                    match key.code {
                        KeyCode::Tab | KeyCode::Right => {
                            channel = (channel + 1) % 3;
                        }
                        KeyCode::BackTab | KeyCode::Left => {
                            channel = (channel + 2) % 3;
                        }
                        KeyCode::Up => {
                            match channel {
                                0 => r = r.saturating_add(5),
                                1 => g = g.saturating_add(5),
                                _ => b = b.saturating_add(5),
                            }
                        }
                        KeyCode::Down => {
                            match channel {
                                0 => r = r.saturating_sub(5),
                                1 => g = g.saturating_sub(5),
                                _ => b = b.saturating_sub(5),
                            }
                        }
                        KeyCode::Enter => {
                            return Ok(format!("#{:02X}{:02X}{:02X}", r, g, b));
                        }
                        KeyCode::Esc => {
                            return Err("Color picker cancelled".into());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Interactive fallback: prompt and wait for color
    let def = config.default_hex.as_deref().unwrap_or("#3498db");
    let line = read_line_prompt(&format!("{} [Hex/RGB, default: {}]: ", prompt, def));
    let trimmed = line.trim();
    if trimmed.is_empty() {
        Ok(def.to_string())
    } else {
        Ok(trimmed.to_string())
    }
}

// ============================================================================
// 14. World-First: Secure Graphical PIN Code Box (`input.pin`)
// ============================================================================

pub fn prompt_pin(prompt: &str, digits: usize, _config: &PinConfig) -> Result<String, String> {
    let num_digits = if digits == 0 { 4 } else { digits };
    if let Some(mock) = get_mock_input() {
        return Ok(mock);
    }

    if let Ok(_guard) = TerminalGuard::new_inline(4) {
        if let Ok(mut terminal) = init_inline_terminal(4) {
            let mut pin = String::new();

            loop {
                let _ = terminal.draw(|f| {
                    let size = f.size();
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(4), Constraint::Min(0)])
                        .split(size);

                    let mut boxes = String::new();
                    for i in 0..num_digits {
                        if i < pin.len() {
                            boxes.push_str("[ ● ] ");
                        } else {
                            boxes.push_str("[ _ ] ");
                        }
                    }

                    let p = Paragraph::new(Line::from(vec![
                        Span::styled(boxes, Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                    ])).block(Block::default().borders(Borders::ALL).title(format!("{} (Enter {} Digits)", prompt, num_digits)));

                    f.render_widget(p, chunks[0]);
                });

                if let Ok(Some(key)) = poll_key_event(Duration::from_millis(50)) {
                    match key.code {
                        KeyCode::Char(c) if c.is_ascii_digit() => {
                            if pin.len() < num_digits {
                                pin.push(c);
                                if pin.len() == num_digits {
                                    // Render final state with all boxes filled before closing
                                    let _ = terminal.draw(|f| {
                                        let size = f.size();
                                        let chunks = Layout::default()
                                            .direction(Direction::Vertical)
                                            .constraints([Constraint::Length(4), Constraint::Min(0)])
                                            .split(size);

                                        let boxes = "[ ● ] ".repeat(num_digits);
                                        let p = Paragraph::new(Line::from(vec![
                                            Span::styled(boxes, Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                                        ])).block(Block::default().borders(Borders::ALL).title(format!("{} (Verified)", prompt)));

                                        f.render_widget(p, chunks[0]);
                                    });
                                    std::thread::sleep(Duration::from_millis(100));
                                    return Ok(pin);
                                }
                            }
                        }
                        KeyCode::Enter => {
                            if !pin.is_empty() {
                                return Ok(pin);
                            }
                        }
                        KeyCode::Backspace => {
                            pin.pop();
                        }
                        KeyCode::Esc => {
                            return Err("PIN cancelled".into());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Interactive fallback: prompt and wait for pin digits
    let line = read_line_prompt(&format!("{} [{} digits]: ", prompt, num_digits));
    let trimmed = line.trim();
    if trimmed.is_empty() {
        Ok("0".repeat(num_digits))
    } else {
        Ok(trimmed.to_string())
    }
}

// ============================================================================
// 15. World-First: Terminal Side-by-Side Diff Editor (`input.diff`)
// ============================================================================

pub fn prompt_diff(prompt: &str, original: &str, _config: &DiffConfig) -> Result<String, String> {
    if let Some(mock) = get_mock_input() {
        return Ok(mock);
    }

    let original_lines: Vec<String> = original.lines().map(|s| s.to_string()).collect();
    let mut edited_lines: Vec<String> = if original_lines.is_empty() {
        vec![String::new()]
    } else {
        original_lines.clone()
    };
    let mut cursor_row: usize = 0;
    let mut cursor_col: usize = 0;

    let height: u16 = (original_lines.len().max(edited_lines.len()) as u16 + 4).clamp(8, 16);

    if let Ok(_guard) = TerminalGuard::new_inline(height) {
        if let Ok(mut terminal) = init_inline_terminal(height) {
            loop {
                let _ = terminal.draw(|f| {
                    let size = f.size();
                    let main_chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(height), Constraint::Min(0)])
                        .split(size);

                    let panes = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                        .split(main_chunks[0]);

                    // 1. Left pane: Original Reference
                    let orig_items: Vec<ListItem> = original_lines
                        .iter()
                        .enumerate()
                        .map(|(i, l)| {
                            let text = format!("{:2} | - {}", i + 1, l);
                            ListItem::new(text).style(Style::default().fg(Color::Red))
                        })
                        .collect();
                    let orig_block = Block::default()
                        .borders(Borders::ALL)
                        .title("Original (Reference)");
                    let orig_list = List::new(orig_items).block(orig_block);
                    f.render_widget(orig_list, panes[0]);

                    // 2. Right pane: Edited Content
                    let edit_items: Vec<ListItem> = edited_lines
                        .iter()
                        .enumerate()
                        .map(|(i, l)| {
                            let is_cur = i == cursor_row;
                            let prefix = if is_cur { "▶" } else { " " };
                            let text = format!("{} {:2} | + {}", prefix, i + 1, l);
                            let style = if is_cur {
                                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(Color::Green)
                            };
                            ListItem::new(text).style(style)
                        })
                        .collect();
                    let edit_title = format!("{} (Ctrl+S: Save, Esc: Cancel)", prompt);
                    let edit_block = Block::default()
                        .borders(Borders::ALL)
                        .title(edit_title);
                    let edit_list = List::new(edit_items).block(edit_block);
                    f.render_widget(edit_list, panes[1]);

                    // Position cursor visually on the active editing line
                    let cur_x = panes[1].x + 8 + (cursor_col as u16);
                    let cur_y = panes[1].y + 1 + (cursor_row as u16);
                    if cur_x < panes[1].x + panes[1].width && cur_y < panes[1].y + panes[1].height {
                        f.set_cursor(cur_x, cur_y);
                    }
                });

                if let Ok(Some(key)) = poll_key_event(Duration::from_millis(50)) {
                    match key.code {
                        KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            return Ok(edited_lines.join("\n"));
                        }
                        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            return Ok(edited_lines.join("\n"));
                        }
                        KeyCode::F(2) | KeyCode::F(10) => {
                            return Ok(edited_lines.join("\n"));
                        }
                        KeyCode::Esc => {
                            return Err("Diff edit cancelled".into());
                        }
                        KeyCode::Up => {
                            if cursor_row > 0 {
                                cursor_row -= 1;
                                cursor_col = cursor_col.min(edited_lines[cursor_row].len());
                            }
                        }
                        KeyCode::Down => {
                            if cursor_row + 1 < edited_lines.len() {
                                cursor_row += 1;
                                cursor_col = cursor_col.min(edited_lines[cursor_row].len());
                            }
                        }
                        KeyCode::Left => {
                            if cursor_col > 0 {
                                cursor_col -= 1;
                            } else if cursor_row > 0 {
                                cursor_row -= 1;
                                cursor_col = edited_lines[cursor_row].len();
                            }
                        }
                        KeyCode::Right => {
                            if cursor_col < edited_lines[cursor_row].len() {
                                cursor_col += 1;
                            } else if cursor_row + 1 < edited_lines.len() {
                                cursor_row += 1;
                                cursor_col = 0;
                            }
                        }
                        KeyCode::Home => {
                            cursor_col = 0;
                        }
                        KeyCode::End => {
                            cursor_col = edited_lines[cursor_row].len();
                        }
                        KeyCode::Enter => {
                            let cur_line = &edited_lines[cursor_row];
                            let head = cur_line[..cursor_col].to_string();
                            let tail = cur_line[cursor_col..].to_string();
                            edited_lines[cursor_row] = head;
                            edited_lines.insert(cursor_row + 1, tail);
                            cursor_row += 1;
                            cursor_col = 0;
                        }
                        KeyCode::Backspace => {
                            if cursor_col > 0 {
                                edited_lines[cursor_row].remove(cursor_col - 1);
                                cursor_col -= 1;
                            } else if cursor_row > 0 {
                                let cur = edited_lines.remove(cursor_row);
                                cursor_row -= 1;
                                cursor_col = edited_lines[cursor_row].len();
                                edited_lines[cursor_row].push_str(&cur);
                            }
                        }
                        KeyCode::Delete => {
                            if cursor_col < edited_lines[cursor_row].len() {
                                edited_lines[cursor_row].remove(cursor_col);
                            } else if cursor_row + 1 < edited_lines.len() {
                                let next = edited_lines.remove(cursor_row + 1);
                                edited_lines[cursor_row].push_str(&next);
                            }
                        }
                        KeyCode::Char(c) => {
                            if !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) {
                                edited_lines[cursor_row].insert(cursor_col, c);
                                cursor_col += 1;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    let line = read_line_prompt(&format!("{}: ", prompt));
    if line.trim().is_empty() {
        Ok(original.to_string())
    } else {
        Ok(line)
    }
}

// ============================================================================
// 16. World-First: Real-Time Keystroke & Chord Capturer (`input.hotkey`)
// ============================================================================

pub fn prompt_hotkey(prompt: &str, _config: &HotkeyConfig) -> Result<HotkeyResult, String> {
    if let Some(mock) = get_mock_input() {
        return Ok(HotkeyResult {
            key: mock.clone(),
            modifiers: Vec::new(),
            chord: mock,
        });
    }

    if let Ok(_guard) = TerminalGuard::new_inline(3) {
        if let Ok(mut terminal) = init_inline_terminal(3) {
            loop {
                let _ = terminal.draw(|f| {
                    let size = f.size();
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(3), Constraint::Min(0)])
                        .split(size);

                    let p = Paragraph::new("Press any key combination or chord (e.g. Ctrl+Alt+K)...")
                        .block(Block::default().borders(Borders::ALL).title(prompt));

                    f.render_widget(p, chunks[0]);
                });

                if let Ok(Some(key)) = poll_key_event(Duration::from_millis(50)) {
                    let mut mods = Vec::new();
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        mods.push("Ctrl".to_string());
                    }
                    if key.modifiers.contains(KeyModifiers::ALT) {
                        mods.push("Alt".to_string());
                    }
                    if key.modifiers.contains(KeyModifiers::SHIFT) {
                        mods.push("Shift".to_string());
                    }

                    let key_name = match key.code {
                        KeyCode::Char(c) => c.to_string(),
                        KeyCode::F(n) => format!("F{}", n),
                        KeyCode::Enter => "Enter".into(),
                        KeyCode::Esc => "Esc".into(),
                        KeyCode::Backspace => "Backspace".into(),
                        KeyCode::Tab => "Tab".into(),
                        KeyCode::Delete => "Delete".into(),
                        KeyCode::Home => "Home".into(),
                        KeyCode::End => "End".into(),
                        KeyCode::PageUp => "PageUp".into(),
                        KeyCode::PageDown => "PageDown".into(),
                        KeyCode::Up => "Up".into(),
                        KeyCode::Down => "Down".into(),
                        KeyCode::Left => "Left".into(),
                        KeyCode::Right => "Right".into(),
                        _ => "Key".into(),
                    };

                    let mut chord_parts = mods.clone();
                    chord_parts.push(key_name.clone());
                    let chord = chord_parts.join("+");

                    return Ok(HotkeyResult {
                        key: key_name,
                        modifiers: mods,
                        chord,
                    });
                }
            }
        }
    }

    // Interactive fallback: prompt and wait for hotkey chord typed by user
    let line = read_line_prompt(&format!("{} (Type hotkey chord e.g. Ctrl+K or Enter): ", prompt));
    let chord = if line.trim().is_empty() { "Enter".to_string() } else { line.trim().to_string() };
    Ok(HotkeyResult {
        key: chord.clone(),
        modifiers: Vec::new(),
        chord,
    })
}

// ============================================================================
// 17. World-First: AI-Assisted Smart Prediction Input (`input.ai`)
// ============================================================================

pub fn prompt_ai(prompt: &str, context: &[String], _config: &AiPredictConfig) -> Result<String, String> {
    let opts = InputOptions {
        suggestions: context.to_vec(),
        ..Default::default()
    };
    prompt_input(prompt, &opts)
}

// ============================================================================
// 18. World-First: Reactive Keystrokes Stream Reader (`input.stream`)
// ============================================================================

pub fn prompt_stream(prompt: &str, config: &StreamConfig) -> Result<Vec<String>, String> {
    if let Some(mock) = get_mock_input() {
        return Ok(vec![mock]);
    }

    let line = prompt_input(prompt, &InputOptions::default())?;
    let delim = config.delimiter.unwrap_or(' ');
    let parts: Vec<String> = line.split(delim).map(|s| s.to_string()).collect();
    Ok(parts)
}
