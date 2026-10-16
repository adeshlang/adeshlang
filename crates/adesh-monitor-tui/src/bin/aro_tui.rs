fn main() {
    if let Err(e) = adesh_monitor_tui::run_tui() {
        eprintln!("TUI Error: {}", e);
        std::process::exit(1);
    }
}
