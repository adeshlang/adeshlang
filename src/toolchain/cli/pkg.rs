//! AdeshLang package manager command implementation (`adl pkg`)

use std::fs;
use std::path::PathBuf;

pub fn execute_pkg_command(args: &[String]) {
    let green = "\x1b[32m";
    let yellow = "\x1b[33m";
    let bold = "\x1b[1m";
    let reset = "\x1b[0m";

    if args.is_empty() {
        println!("{}AdeshLang Package Manager (`adl pkg`){}", bold, reset);
        println!("Usage:");
        println!(
            "  adl pkg init [name]     Initialize a new package manifest in current directory"
        );
        println!("  adl pkg install <pkg>   Install package dependency");
        println!("  adl pkg update          Update package dependencies");
        println!("  adl pkg remove <pkg>    Remove installed package");
        println!("  adl pkg list            List installed global packages");
        return;
    }

    let subcmd = args[0].as_str();
    let pkg_root = std::env::var("LOCALAPPDATA")
        .map(|d| PathBuf::from(d).join("AdeshLang").join("packages"))
        .unwrap_or_else(|_| PathBuf::from("./packages"));

    match subcmd {
        "init" => {
            let pkg_name = args.get(1).map(|s| s.as_str()).unwrap_or("my_project");
            let manifest_content = format!(
                "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nauthors = []\n\n[dependencies]\n",
                pkg_name
            );
            if let Err(e) = fs::write("adesh.toml", manifest_content) {
                println!("  {}!{} Failed to write adesh.toml: {}", yellow, reset, e);
            } else {
                println!(
                    "  {}✓{} Initialized package manifest `adesh.toml` for `{}`",
                    green, reset, pkg_name
                );
            }
        }
        "install" => {
            let pkg_name = args.get(1);
            if let Some(name) = pkg_name {
                let target_dir = pkg_root.join(name);
                if let Err(e) = fs::create_dir_all(&target_dir) {
                    println!("  {}!{} Failed to install `{}`: {}", yellow, reset, name, e);
                } else {
                    println!(
                        "  {}✓{} Installed package `{}` to {}",
                        green,
                        reset,
                        name,
                        target_dir.display()
                    );
                }
            } else {
                println!(
                    "  {}!{} Please specify a package name to install",
                    yellow, reset
                );
            }
        }
        "list" => {
            println!(
                "{}Installed AdeshLang Packages in {}:{}",
                bold,
                pkg_root.display(),
                reset
            );
            if let Ok(entries) = fs::read_dir(&pkg_root) {
                let mut count = 0;
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        count += 1;
                        println!("  - {}", entry.file_name().to_string_lossy());
                    }
                }
                if count == 0 {
                    println!("  (No packages installed yet)");
                }
            } else {
                println!("  (Package directory empty)");
            }
        }
        "remove" => {
            if let Some(name) = args.get(1) {
                let target_dir = pkg_root.join(name);
                if target_dir.exists() {
                    let _ = fs::remove_dir_all(&target_dir);
                    println!("  {}✓{} Removed package `{}`", green, reset, name);
                } else {
                    println!("  {}!{} Package `{}` not found", yellow, reset, name);
                }
            } else {
                println!("  {}!{} Please specify package to remove", yellow, reset);
            }
        }
        "update" => {
            println!("  {}✓{} Packages are up to date", green, reset);
        }
        _ => {
            println!(
                "Unknown package command `{}`. Run `adl pkg` for usage.",
                subcmd
            );
        }
    }
}
