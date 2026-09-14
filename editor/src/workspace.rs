use std::env;
use std::path::{Path, PathBuf};

pub struct Workspace {
    pub root: PathBuf,
    pub has_manifest: bool,
}

impl Workspace {
    pub fn detect(start_dir: Option<&Path>) -> Self {
        let start = match start_dir {
            Some(d) => d.to_path_buf(),
            None => env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        };

        let mut curr = if start.is_file() {
            start.parent().unwrap_or(&start).to_path_buf()
        } else {
            start
        };

        loop {
            // Look for project markers
            let adesh_toml = curr.join("Adesh.toml");
            let adl_manifest = curr.join("Adesh.adl");
            let cargo_toml = curr.join("Cargo.toml");
            let git_dir = curr.join(".git");

            if adesh_toml.is_file() || adl_manifest.is_file() {
                return Workspace {
                    root: curr,
                    has_manifest: true,
                };
            }

            if cargo_toml.is_file() || git_dir.is_dir() {
                return Workspace {
                    root: curr,
                    has_manifest: false,
                };
            }

            if let Some(parent) = curr.parent() {
                curr = parent.to_path_buf();
            } else {
                break;
            }
        }

        let fallback = match start_dir {
            Some(d) if d.is_dir() => d.to_path_buf(),
            Some(d) => d.parent().unwrap_or(d).to_path_buf(),
            None => env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        };

        Workspace {
            root: fallback,
            has_manifest: false,
        }
    }
}
