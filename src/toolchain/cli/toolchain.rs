//! `adesh toolchain` diagnostics.

use super::super::resolver::{
    SUPPORTED_LLVM_MAJOR, ToolchainPreference, bundled_root, detect_system_toolchain, resolve,
};

pub fn execute_toolchain_command(args: &[String]) {
    execute_toolchain_command_with_preference(args, None);
}

pub fn execute_toolchain_command_with_preference(
    args: &[String],
    preference: Option<ToolchainPreference>,
) {
    let subcommand = args.first().map(String::as_str).unwrap_or("current");
    match subcommand {
        "list" => {
            println!(
                "AdeshLang Toolchains\n\nBundled:\n  LLVM {}.x",
                SUPPORTED_LLVM_MAJOR
            );
            if let Some((path, version)) = detect_system_toolchain() {
                println!(
                    "System:\n  LLVM {} ({})",
                    version.as_deref().unwrap_or("unknown"),
                    path.display()
                );
            } else {
                println!("System:\n  not detected");
            }
        }
        "path" => match bundled_root() {
            Some(path) => println!("{}", path.display()),
            None => eprintln!("Bundled toolchain is not installed"),
        },
        "check" => match resolve(preference) {
            Ok(tc) => println!(
                "OK: {} LLVM {}",
                if tc.bundled { "bundled" } else { "system" },
                tc.version.as_deref().unwrap_or("unknown")
            ),
            Err(error) => {
                eprintln!("Toolchain check failed: {error}");
                std::process::exit(1);
            }
        },
        "update" => {
            println!(
                "AdeshLang Update Guidance:\n\
                 • Incremental Update (binaries, lib, std): Run 'powershell -ExecutionPolicy Bypass -File .\\scripts\\update_installed.ps1'\n\
                 • In-Place Upgrade: Run the latest setup installer without uninstalling\n\
                 • Re-download curated LLVM toolchain: Run 'adesh toolchain install --force'"
            );
        }
        "install" => super::install::execute_install_command(&args[1..]),
        "expose" => super::install::execute_expose_command(&args[1..]),
        "current" | _ => match resolve(preference) {
            Ok(tc) => println!(
                "Active: {}\nLocation: {}\nClang: {}\nLLD: {}",
                if tc.bundled {
                    ToolchainPreference::Bundled.as_str()
                } else {
                    ToolchainPreference::System.as_str()
                },
                tc.root.display(),
                tc.clang.display(),
                tc.lld.display()
            ),
            Err(error) => eprintln!("{error}"),
        },
    }
}
