use std::env;
use std::io;
use std::panic;
use std::path::PathBuf;

use crossterm::cursor::SetCursorStyle;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::{
    event, execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use adesh_editor::app::App;
use adesh_editor::input::handle_event;
use adesh_editor::ui::render_ui;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        SetCursorStyle::BlinkingBar
    )?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Setup panic hook to restore terminal state if panic occurs
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            SetCursorStyle::DefaultUserShape
        );
        original_hook(panic_info);
    }));

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let initial_path = if args.len() > 1 {
        Some(PathBuf::from(&args[1]))
    } else {
        None
    };

    let mut app = App::new(initial_path);

    // Event loop
    while app.running {
        app.check_auto_save();
        app.cleanup_toasts();
        terminal.draw(|f| render_ui(f, &mut app))?;

        if event::poll(std::time::Duration::from_millis(50))? {
            let evt = event::read()?;
            handle_event(&mut app, evt).await;
        }
    }

    // Terminal teardown
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        SetCursorStyle::DefaultUserShape
    )?;
    terminal.show_cursor()?;

    Ok(())
}
