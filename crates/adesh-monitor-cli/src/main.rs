fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = adesh_monitor_cli::run_cli(&args) {
        eprintln!("Observatory Error: {}", e);
        std::process::exit(1);
    }
}
