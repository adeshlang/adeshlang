//! `adesh-monitor-tui`
//!
//! Interactive Terminal User Interface (TUI) for Adesh Observatory (ARO) using Ratatui & Crossterm.

use adesh_monitor_core::ipc::send_collector_rpc;
use adesh_monitor_core::storage::HistoryStore;
use adesh_monitor_protocol::*;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};
use std::io;
use std::time::{Duration, Instant};

pub fn run_tui() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_app(&mut terminal);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("TUI Error: {:?}", err);
    }

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>) -> Result<(), Box<dyn std::error::Error>>
where
    <B as Backend>::Error: std::error::Error + 'static,
{
    let mut last_fetch = Instant::now() - Duration::from_secs(10);
    let mut raw_sessions: Vec<SessionSummary> = Vec::new();
    let mut selected_idx = 0;
    let mut mode_filter: Option<ExecutionMode> = None;
    let mut notice_msg: Option<(String, Instant)> = None;

    let filter_modes = [
        None,
        Some(ExecutionMode::Interpreter),
        Some(ExecutionMode::Jit),
        Some(ExecutionMode::BytecodeVm),
        Some(ExecutionMode::Linker),
        Some(ExecutionMode::AotNative),
    ];
    let mut filter_mode_idx = 0;

    loop {
        if last_fetch.elapsed() > Duration::from_millis(500) || raw_sessions.is_empty() {
            if let Ok(CollectorResponse::SessionsList { sessions: list }) =
                send_collector_rpc(&CollectorRequest::ListSessions { limit: 100 })
            {
                raw_sessions = list;
            } else if let Ok(store) = HistoryStore::default_store() {
                if let Ok(list) = store.list_sessions(100) {
                    raw_sessions = list;
                } else {
                    notice_msg = Some(("Observatory SQLite query error".to_string(), Instant::now()));
                }
            } else {
                notice_msg = Some(("Observatory storage unreachable".to_string(), Instant::now()));
            }

            // Always sort with newest at top
            raw_sessions.sort_by(|a, b| b.start_time.cmp(&a.start_time));
            last_fetch = Instant::now();
        }

        let displayed_sessions: Vec<&SessionSummary> = raw_sessions
            .iter()
            .filter(|s| match mode_filter {
                Some(m) => s.execution_mode == m,
                None => true,
            })
            .collect();

        let current_filter_label = match mode_filter {
            Some(m) => format!("{:?}", m),
            None => "All Modes".to_string(),
        };

        let footer_text = if let Some((ref msg, time)) = notice_msg {
            if time.elapsed() < Duration::from_secs(3) {
                msg.clone()
            } else {
                format!(
                    "Live ARO | {}/{} sessions | Mode: [{}] ('g'/'m' cycle) | 'c' clear | 'r' refresh | 'q' quit",
                    displayed_sessions.len(),
                    raw_sessions.len(),
                    current_filter_label
                )
            }
        } else {
            format!(
                "Live ARO | {}/{} sessions | Mode: [{}] ('g'/'m' cycle) | 'c' clear | 'r' refresh | 'q' quit",
                displayed_sessions.len(),
                raw_sessions.len(),
                current_filter_label
            )
        };

        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Header
                    Constraint::Min(8),    // Table
                    Constraint::Length(3), // Footer
                ])
                .split(f.area());

            // Header block
            let title = Paragraph::new(Line::from(vec![
                Span::styled(
                    " ADESH RUNTIME OBSERVATORY (ARO) ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("— Real-Time Cross-Platform Toolchain Monitor"),
            ]))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Blue)),
            );
            f.render_widget(title, chunks[0]);

            // Sessions table
            let header_cells = [
                "Session ID",
                "Timestamp",
                "Tool",
                "PID",
                "Mode",
                "Status",
                "Duration",
                "Peak RSS",
            ]
            .iter()
            .map(|h| {
                Cell::from(*h).style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
            });
            let header = Row::new(header_cells).height(1).bottom_margin(1);

            let rows = displayed_sessions.iter().enumerate().map(|(idx, s)| {
                let is_selected = idx == selected_idx;
                let dur = s
                    .duration_ms
                    .map(|d| format!("{}ms", d))
                    .unwrap_or_else(|| "running".to_string());
                let mem = format!("{:.1} MB", s.peak_rss_bytes as f64 / (1024.0 * 1024.0));
                let time_str = s.start_time.format("%H:%M:%S").to_string();
                let status_style = match s.status {
                    SessionStatus::Running => Style::default().fg(Color::Green),
                    SessionStatus::Completed => Style::default().fg(Color::Cyan),
                    SessionStatus::Failed => Style::default().fg(Color::Red),
                    SessionStatus::Terminated => Style::default().fg(Color::DarkGray),
                };

                let cells = vec![
                    Cell::from(s.session_id.clone()),
                    Cell::from(time_str),
                    Cell::from(s.tool_name.clone()),
                    Cell::from(s.pid.to_string()),
                    Cell::from(format!("{:?}", s.execution_mode)),
                    Cell::from(format!("{:?}", s.status)).style(status_style),
                    Cell::from(dur),
                    Cell::from(mem),
                ];

                let row = Row::new(cells).height(1);
                if is_selected {
                    row.style(
                        Style::default()
                            .bg(Color::Rgb(30, 60, 90))
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    row
                }
            });

            let table = Table::new(
                rows,
                [
                    Constraint::Length(38),
                    Constraint::Length(12),
                    Constraint::Length(10),
                    Constraint::Length(8),
                    Constraint::Length(14),
                    Constraint::Length(12),
                    Constraint::Length(12),
                    Constraint::Length(12),
                ],
            )
            .header(header)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" Execution Sessions (Mode: {}) ", current_filter_label)),
            );
            f.render_widget(table, chunks[1]);

            // Footer block
            let footer = Paragraph::new(footer_text).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Gray)),
            );
            f.render_widget(footer, chunks[2]);
        })?;

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Up => {
                    selected_idx = selected_idx.saturating_sub(1);
                }
                KeyCode::Down => {
                    if !displayed_sessions.is_empty() && selected_idx < displayed_sessions.len() - 1 {
                        selected_idx += 1;
                    }
                }
                KeyCode::Char('r') => {
                    last_fetch = Instant::now() - Duration::from_secs(10);
                }
                KeyCode::Char('g') | KeyCode::Char('m') => {
                    filter_mode_idx = (filter_mode_idx + 1) % filter_modes.len();
                    mode_filter = filter_modes[filter_mode_idx];
                    selected_idx = 0;
                }
                KeyCode::Char('c') | KeyCode::Char('x') => {
                    let _ = send_collector_rpc(&CollectorRequest::PurgeHistory);
                    if let Ok(store) = HistoryStore::default_store() {
                        let _ = store.purge_history();
                    }
                    raw_sessions.clear();
                    selected_idx = 0;
                    notice_msg = Some((
                        "Dashboard history cleared successfully! | 'r' refresh | 'q' quit".to_string(),
                        Instant::now(),
                    ));
                    last_fetch = Instant::now();
                }
                _ => {}
            }
        }
    }
}
