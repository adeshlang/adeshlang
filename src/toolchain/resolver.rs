//! Deterministic discovery of the compiler toolchain used by installed AdeshLang.

use std::env;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const SUPPORTED_LLVM_MAJOR: u32 = 18;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolchainPreference {
    Bundled,
    System,
}

impl ToolchainPreference {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bundled => "bundled",
            Self::System => "system",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolchainPaths {
    pub root: PathBuf,
    pub clang: PathBuf,
    pub clangxx: PathBuf,
    pub lld: PathBuf,
    pub llvm_config: PathBuf,
    pub bundled: bool,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolchainError(pub String);

impl fmt::Display for ToolchainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ToolchainError {}

fn executable_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

pub fn installation_home() -> Option<PathBuf> {
    env::var_os("ADESH_HOME")
        .or_else(|| env::var_os("ADESHLANG_HOME"))
        .map(PathBuf::from)
        .or_else(|| {
            let exe = env::current_exe().ok()?;
            let bin = exe.parent()?;
            let home = if bin.file_name().and_then(|v| v.to_str()) == Some("bin") {
                bin.parent()?
            } else {
                bin
            };
            Some(home.to_path_buf())
        })
}

pub fn bundled_root() -> Option<PathBuf> {
    let home = installation_home()?;
    let root = home.join("toolchain").join("llvm");
    root.is_dir().then_some(root)
}

pub fn bin_directory() -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    Some(exe.parent()?.to_path_buf())
}

fn from_root(root: PathBuf, bundled: bool) -> Result<ToolchainPaths, ToolchainError> {
    let bin = root.join("bin");
    let clang = bin.join(executable_name("clang"));
    let clangxx = bin.join(executable_name("clang++"));
    let lld = if cfg!(windows) {
        let lld_link = bin.join("lld-link.exe");
        if lld_link.exists() {
            lld_link
        } else {
            bin.join("lld.exe")
        }
    } else {
        bin.join("lld")
    };
    let llvm_config = bin.join(executable_name("llvm-config"));
    if !clang.is_file() {
        return Err(ToolchainError(format!(
            "Clang was not found at {}",
            clang.display()
        )));
    }
    if !lld.is_file() {
        return Err(ToolchainError(format!(
            "LLD was not found at {}",
            lld.display()
        )));
    }
    let version = tool_version(&clang);
    Ok(ToolchainPaths {
        root,
        clang,
        clangxx,
        lld,
        llvm_config,
        bundled,
        version,
    })
}

pub fn tool_version(tool: &Path) -> Option<String> {
    let output = Command::new(tool).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.split_whitespace().find_map(|part| {
        let value = part.trim_start_matches("version");
        let major = value.split('.').next()?.parse::<u32>().ok()?;
        (major > 0).then(|| value.trim_end_matches(',').to_string())
    })
}

pub fn is_compatible(version: Option<&str>) -> bool {
    version.and_then(|v| v.split('.').next()?.parse::<u32>().ok()) == Some(SUPPORTED_LLVM_MAJOR)
}

/// A pre-existing toolchain is accepted regardless of version: if any LLVM
/// is already on the machine it is reused as-is and nothing is downloaded.
/// The manifest's pinned version only governs what a FRESH install downloads.
pub fn is_usable_existing(version: Option<&str>) -> bool {
    version
        .and_then(|v| v.split('.').next()?.parse::<u32>().ok())
        .is_some_and(|major| major > 0)
}

fn path_tool(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for entry in env::split_paths(&path) {
        let candidate = entry.join(executable_name(name));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn resolve(preference: Option<ToolchainPreference>) -> Result<ToolchainPaths, ToolchainError> {
    let explicit = env::var_os("ADESH_TOOLCHAIN")
        .or_else(|| env::var_os("ADESHLANG_TOOLCHAIN"))
        .map(PathBuf::from);
    let explicit = explicit.or_else(|| {
        let clang = env::var_os("ADESH_LLVM").map(PathBuf::from)?;
        Some(clang.parent()?.parent()?.to_path_buf())
    });
    if let Some(root) = explicit {
        let root = if root.file_name().and_then(|v| v.to_str()) == Some("bin") {
            root.parent().unwrap_or(&root).to_path_buf()
        } else {
            root
        };
        return from_root(root, false);
    }
    let preference = preference.unwrap_or(ToolchainPreference::Bundled);
    if preference == ToolchainPreference::Bundled {
        if let Some(root) = bundled_root() {
            if let Ok(toolchain) = from_root(root, true) {
                return Ok(toolchain);
            }
        }
    }
    if preference == ToolchainPreference::System {
        if let Some(clang) = path_tool("clang") {
            if let Some(bin) = clang.parent() {
                let root = bin.parent().unwrap_or(bin).to_path_buf();
                // Any discoverable LLVM is accepted, regardless of version:
                // an existing toolchain is always preferred over a download.
                return from_root(root, false);
            }
        }
    }
    Err(ToolchainError(
        "No AdeshLang toolchain found. Install it with `adesh toolchain install`, or set ADESH_TOOLCHAIN to an existing LLVM tree.".to_string(),
    ))
}

pub fn detect_system_toolchain() -> Option<(PathBuf, Option<String>)> {
    let clang = path_tool("clang")?;
    Some((clang.clone(), tool_version(&clang)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_checks_major_version() {
        assert!(is_compatible(Some("18.1.8")));
        assert!(!is_compatible(Some("17.0.6")));
        assert!(!is_compatible(None));
        // Any existing toolchain is reused regardless of version.
        assert!(is_usable_existing(Some("18.1.8")));
        assert!(is_usable_existing(Some("22.1.0")));
        assert!(is_usable_existing(Some("17.0.6")));
        assert!(!is_usable_existing(None));
    }

    #[test]
    fn executable_name_is_platform_aware() {
        assert!(executable_name("clang").starts_with("clang"));
    }
}
