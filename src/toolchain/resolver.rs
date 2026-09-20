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
    // 1. Check if the currently running executable is inside a valid installation directory
    if let Ok(exe) = env::current_exe() {
        if let Some(bin) = exe.parent() {
            let candidate_root = if bin.file_name().and_then(|v| v.to_str()) == Some("bin") {
                bin.parent().unwrap_or(bin)
            } else {
                bin
            };
            // If the candidate root exists and contains a standard component (std, lib, bin, or toolchain)
            if candidate_root.join("std").exists()
                || candidate_root.join("lib").exists()
                || candidate_root.join("bin").exists()
                || candidate_root.join("toolchain").exists()
            {
                return Some(candidate_root.to_path_buf());
            }
        }
    }

    // 2. Check explicit ADESH_HOME / ADESHLANG_HOME environment variable (if valid on disk)
    if let Some(env_path) = env::var_os("ADESH_HOME")
        .or_else(|| env::var_os("ADESHLANG_HOME"))
        .map(PathBuf::from)
    {
        if env_path.exists() {
            return Some(env_path);
        }
    }

    // 3. Fallback to executable parent directory
    if let Ok(exe) = env::current_exe() {
        if let Some(bin) = exe.parent() {
            let home = if bin.file_name().and_then(|v| v.to_str()) == Some("bin") {
                bin.parent().unwrap_or(bin)
            } else {
                bin
            };
            return Some(home.to_path_buf());
        }
    }

    // 4. Return environment variable even if missing (for diagnostic reporting)
    env::var_os("ADESH_HOME")
        .or_else(|| env::var_os("ADESHLANG_HOME"))
        .map(PathBuf::from)
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
    version
        .and_then(|v| v.split('.').next()?.parse::<u32>().ok())
        .is_some_and(|major| major >= 18)
}

/// A pre-existing toolchain is accepted regardless of version: if any LLVM
/// is already on the machine it is reused as-is and nothing is downloaded.
/// The manifest's pinned version only governs what a FRESH install downloads.
pub fn is_usable_existing(version: Option<&str>) -> bool {
    version
        .and_then(|v| v.split('.').next()?.parse::<u32>().ok())
        .is_some_and(|major| major > 0)
}

pub fn path_tool(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for entry in env::split_paths(&path) {
        let candidate = entry.join(executable_name(name));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn from_root(root: PathBuf, bundled: bool) -> Result<ToolchainPaths, ToolchainError> {
    let bin = if root.join("bin").is_dir() {
        root.join("bin")
    } else {
        root.clone()
    };
    let exe_suffix = if cfg!(windows) { ".exe" } else { "" };

    // Clang compiler candidate resolution
    let clang_candidates = [
        bin.join(format!("clang{}", exe_suffix)),
        bin.join(format!("clang-18{}", exe_suffix)),
        bin.join(format!("clang-19{}", exe_suffix)),
        bin.join(format!("clang-20{}", exe_suffix)),
        bin.join(format!("clang-17{}", exe_suffix)),
        bin.join(format!("clang-16{}", exe_suffix)),
    ];
    let clang = clang_candidates
        .into_iter()
        .find(|p| p.is_file())
        .or_else(|| path_tool("clang"));
    let Some(clang) = clang else {
        return Err(ToolchainError(format!(
            "Clang was not found in {} or system PATH",
            bin.display()
        )));
    };

    // Clang++ compiler candidate resolution
    let clangxx_candidates = [
        bin.join(format!("clang++{}", exe_suffix)),
        bin.join(format!("clang++-18{}", exe_suffix)),
        bin.join(format!("clang++-19{}", exe_suffix)),
        bin.join(format!("clang++-20{}", exe_suffix)),
        bin.join(format!("clang++-17{}", exe_suffix)),
        bin.join(format!("clang++-16{}", exe_suffix)),
    ];
    let clangxx = clangxx_candidates
        .into_iter()
        .find(|p| p.is_file())
        .or_else(|| path_tool("clang++"))
        .unwrap_or_else(|| bin.join(format!("clang++{}", exe_suffix)));

    // Linker candidate resolution (LLD / ld.lld / system ld)
    let lld_candidates = if cfg!(windows) {
        vec![
            bin.join("lld-link.exe"),
            bin.join("lld.exe"),
            bin.join("ld.lld.exe"),
        ]
    } else {
        vec![
            bin.join("lld"),
            bin.join("ld.lld"),
            bin.join("lld-18"),
            bin.join("ld.lld-18"),
            bin.join("lld-19"),
            bin.join("ld.lld-19"),
            bin.join("ld64.lld"),
            bin.join("lld-link"),
        ]
    };
    let mut lld = lld_candidates.into_iter().find(|p| p.is_file());
    if lld.is_none() {
        for name in ["ld.lld", "lld", "ld64.lld", "lld-link", "ld"] {
            if let Some(p) = path_tool(name) {
                lld = Some(p);
                break;
            }
        }
    }
    let Some(lld) = lld else {
        return Err(ToolchainError(format!(
            "LLD was not found in {} or system PATH",
            bin.display()
        )));
    };

    let llvm_config_candidates = [
        bin.join(format!("llvm-config{}", exe_suffix)),
        bin.join(format!("llvm-config-18{}", exe_suffix)),
        bin.join(format!("llvm-config-19{}", exe_suffix)),
    ];
    let llvm_config = llvm_config_candidates
        .into_iter()
        .find(|p| p.is_file())
        .or_else(|| path_tool("llvm-config"))
        .unwrap_or_else(|| bin.join(executable_name("llvm-config")));

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

pub fn candidate_toolchain_roots() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    // 1. From PATH tools
    if let Some(clang) = path_tool("clang") {
        if let Some(bin) = clang.parent() {
            candidates.push(bin.parent().unwrap_or(bin).to_path_buf());
            candidates.push(bin.to_path_buf());
        }
    }
    // 2. Linux well-known LLVM roots
    candidates.push(PathBuf::from("/usr/lib/llvm-18"));
    candidates.push(PathBuf::from("/usr/lib/llvm-19"));
    candidates.push(PathBuf::from("/usr/lib/llvm-20"));
    candidates.push(PathBuf::from("/usr/lib/llvm-17"));
    candidates.push(PathBuf::from("/usr/lib/llvm-16"));
    candidates.push(PathBuf::from("/usr/local"));
    candidates.push(PathBuf::from("/usr"));
    // 3. macOS Homebrew & MacPorts well-known roots
    candidates.push(PathBuf::from("/opt/homebrew/opt/llvm@18"));
    candidates.push(PathBuf::from("/opt/homebrew/opt/llvm@19"));
    candidates.push(PathBuf::from("/opt/homebrew/opt/llvm"));
    candidates.push(PathBuf::from("/usr/local/opt/llvm@18"));
    candidates.push(PathBuf::from("/usr/local/opt/llvm@19"));
    candidates.push(PathBuf::from("/usr/local/opt/llvm"));
    candidates.push(PathBuf::from("/opt/local/libexec/llvm-18"));
    // 4. Windows well-known roots
    candidates.push(PathBuf::from(r"C:\Program Files\LLVM"));
    candidates.push(PathBuf::from(r"C:\LLVM"));
    candidates.push(PathBuf::from(r"C:\Program Files (x86)\LLVM"));
    candidates
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

    let pref = preference.unwrap_or(ToolchainPreference::Bundled);
    if pref == ToolchainPreference::Bundled {
        if let Some(root) = bundled_root() {
            if let Ok(toolchain) = from_root(root, true) {
                return Ok(toolchain);
            }
        }
    }

    // Check system toolchains and well-known installation paths
    for root in candidate_toolchain_roots() {
        if root.exists() {
            if let Ok(toolchain) = from_root(root, false) {
                return Ok(toolchain);
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
        if cfg!(windows) {
            assert_eq!(executable_name("clang"), "clang.exe");
        } else {
            assert_eq!(executable_name("clang"), "clang");
        }
    }

    #[test]
    fn candidate_roots_contain_standard_paths() {
        let roots = candidate_toolchain_roots();
        assert!(!roots.is_empty());
        let root_strings: Vec<String> = roots
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        assert!(
            root_strings
                .iter()
                .any(|s| s.contains("llvm") || s.contains("LLVM") || s.contains("usr"))
        );
    }

    #[test]
    fn preference_as_str_returns_expected_values() {
        assert_eq!(ToolchainPreference::Bundled.as_str(), "bundled");
        assert_eq!(ToolchainPreference::System.as_str(), "system");
    }
}
