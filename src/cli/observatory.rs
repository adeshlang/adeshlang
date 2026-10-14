//! `adesh monitor` CLI integration module
//!
//! Provides the entry point for running the interactive TUI or scriptable CLI
//! through the main `adesh` binary.

pub fn execute_monitor_command(args: &[String]) {
    if args.is_empty() {
        // Default to launching interactive TUI if no subcommands provided
        if let Err(e) = adesh_monitor_tui::run_tui() {
            eprintln!("Failed to launch Observatory TUI: {}", e);
            std::process::exit(1);
        }
        return;
    }

    if args[0] == "tui" || args[0] == "interactive" {
        if let Err(e) = adesh_monitor_tui::run_tui() {
            eprintln!("Failed to launch Observatory TUI: {}", e);
            std::process::exit(1);
        }
        return;
    }

    if let Err(e) = adesh_monitor_cli::run_cli(args) {
        eprintln!("Observatory Error: {}", e);
        std::process::exit(1);
    }
}
