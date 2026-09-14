//! Unit and Integration Tests for Windows Distribution & Health Inspector Tools

#[test]
fn test_doctor_command_execution() {
    // Verify adl doctor executes without panicking
    crate::toolchain::cli::doctor::execute_doctor_command();
}

#[test]
fn test_env_command_execution() {
    // Verify adl env executes without panicking
    crate::toolchain::cli::env::execute_env_command();
}

#[test]
fn test_repair_command_execution() {
    // Verify adl repair executes without panicking
    crate::toolchain::cli::repair::execute_repair_command();
}

#[test]
fn test_pkg_command_execution() {
    // Verify adl pkg list executes without panicking
    crate::toolchain::cli::pkg::execute_pkg_command(&["list".to_string()]);
}
