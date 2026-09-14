fn main() {
    if let Err(error) = adeshlang::ecosystem::cli::run_from_env() {
        eprintln!("Error: {}", error);
        std::process::exit(1);
    }
}
