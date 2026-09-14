//! System-wide exposure of the toolchain.
//!
//! `adesh toolchain install --system` places the installed toolchain on the
//! system PATH and exports the AdeshLang environment variables so that the
//! clang/lld/llc/mlir executables are usable by every user and every program,
//! not only by `adesh` itself.
//!
//! - Windows: writes the machine (or user) environment via the registry
//!   (`HKLM\...\Session Manager\Environment` / `HKCU\Environment`), preserving
//!   `REG_EXPAND_SZ`, then broadcasts `WM_SETTINGCHANGE`.
//! - Unix: symlinks toolchain executables into `/usr/local/bin` (user scope:
//!   `~/.local/bin`) and writes `/etc/profile.d/adeshlang.sh` (user scope:
//!   the shell profile file).

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// All users (requires elevation on Windows, root on Unix).
    System,
    /// Current user only.
    User,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::System => "system",
            Scope::User => "user",
        }
    }
}

/// Tool executable suffix helper.
fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Locate the active LLVM toolchain root: the AdeshLang-bundled
/// `<home>/toolchain/llvm` when present, otherwise the standard location
/// used by the upstream LLVM installer (Windows).
pub fn active_toolchain_root(home: &Path) -> std::path::PathBuf {
    let bundled = home.join("toolchain").join("llvm");
    let bundled_clang = bundled.join("bin").join(exe("clang"));
    if bundled_clang.is_file() {
        return bundled;
    }
    if cfg!(windows) {
        for candidate in ["C:\\Program Files\\LLVM", "C:\\Program Files (x86)\\LLVM"] {
            let root = Path::new(candidate);
            if root.join("bin").join(exe("clang")).is_file() {
                return root.to_path_buf();
            }
        }
    }
    bundled
}

/// AdeshLang environment variables derived from the installation home.
pub fn env_pairs(home: &Path) -> Vec<(String, String)> {
    let toolchain = active_toolchain_root(home);
    let bin = toolchain.join("bin");
    let mut pairs = vec![
        ("ADESH_HOME".to_string(), home.display().to_string()),
        (
            "ADESH_TOOLCHAIN".to_string(),
            toolchain.display().to_string(),
        ),
    ];
    let conditional = [
        ("ADESH_CLANG", "clang"),
        ("ADESH_LLC", "llc"),
        ("ADESH_MLIR_OPT", "mlir-opt"),
        ("ADESH_MLIR_TRANSLATE", "mlir-translate"),
    ];
    for (var, tool) in conditional {
        let candidate = bin.join(exe(tool));
        if candidate.is_file() {
            pairs.push((var.to_string(), candidate.display().to_string()));
        }
    }
    pairs
}

// ── Unix ─────────────────────────────────────────────────────────────────────

#[cfg(unix)]
pub fn expose(home: &Path, scope: Scope) -> Result<(), String> {
    match scope {
        Scope::System => expose_unix_system(home),
        Scope::User => expose_unix_user(home),
    }
}

#[cfg(unix)]
fn is_root() -> bool {
    // SAFETY: geteuid is a simple libc call with no preconditions.
    unsafe { libc::geteuid() == 0 }
}

#[cfg(unix)]
fn symlink_bin(home: &Path, link_dir: &Path) -> Result<usize, String> {
    let bin = home.join("toolchain").join("llvm").join("bin");
    if !bin.is_dir() {
        return Err(format!(
            "Toolchain bin directory not found at {} — run `adesh toolchain install` first",
            bin.display()
        ));
    }
    std::fs::create_dir_all(link_dir)
        .map_err(|e| format!("Failed to create {}: {e}", link_dir.display()))?;
    let mut linked = 0usize;
    for entry in std::fs::read_dir(&bin)
        .map_err(|e| format!("Failed to list {}: {e}", bin.display()))?
        .flatten()
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name() else {
            continue;
        };
        let link = link_dir.join(name);
        let _ = std::fs::remove_file(&link); // refresh existing links
        std::os::unix::fs::symlink(&path, &link).map_err(|e| {
            format!(
                "Failed to symlink {} → {}: {e}",
                link.display(),
                path.display()
            )
        })?;
        linked += 1;
    }
    Ok(linked)
}

#[cfg(unix)]
fn write_profile_file(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
    }
    std::fs::write(path, content).map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

#[cfg(unix)]
fn profile_script(home: &Path) -> String {
    let mut script = String::from(
        "# AdeshLang environment (managed by `adesh toolchain expose`; safe to remove)\n",
    );
    for (key, value) in env_pairs(home) {
        script.push_str(&format!("export {key}='{value}'\n"));
    }
    script
}

#[cfg(unix)]
fn expose_unix_system(home: &Path) -> Result<(), String> {
    if !is_root() {
        return Err(
            "System-wide exposure requires root. Re-run with sudo, or use `--user`.".to_string(),
        );
    }
    let linked = symlink_bin(home, Path::new("/usr/local/bin"))?;
    write_profile_file(
        Path::new("/etc/profile.d/adeshlang.sh"),
        &profile_script(home),
    )?;
    println!(
        "  ✓ Exposed {linked} toolchain executables via /usr/local/bin and wrote /etc/profile.d/adeshlang.sh"
    );
    Ok(())
}

#[cfg(unix)]
fn expose_unix_user(home: &Path) -> Result<(), String> {
    let home_dir = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .ok_or("HOME is not set; cannot determine user profile location")?;
    let local_bin = home_dir.join(".local").join("bin");
    let linked = symlink_bin(home, &local_bin)?;
    // macOS default shell is zsh (reads ~/.zprofile); Linux uses ~/.profile.
    let profile = if cfg!(target_os = "macos") {
        home_dir.join(".zprofile")
    } else {
        home_dir.join(".profile")
    };
    write_profile_file(&profile, &profile_script(home))?;
    println!(
        "  ✓ Exposed {linked} toolchain executables via {} and wrote {}",
        local_bin.display(),
        profile.display()
    );
    println!(
        "  ! Ensure {} is on your PATH (most Linux distros include ~/.local/bin by default)",
        local_bin.display()
    );
    Ok(())
}

// ── Windows ──────────────────────────────────────────────────────────────────

#[cfg(windows)]
pub fn expose(home: &Path, scope: Scope) -> Result<(), String> {
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE, REG_EXPAND_SZ, REG_SZ,
        REG_VALUE_TYPE, RegCloseKey, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    };

    fn to_wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn open_key(root: HKEY, scope: Scope) -> Result<HKEY, String> {
        let subkey = match scope {
            Scope::System => "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment",
            Scope::User => "Environment",
        };
        let subkey_w = to_wide(subkey);
        // SAFETY: `root` is a predefined-key constant; `key` is a valid,
        // initialized out-param (HKEY is an isize handle in windows-sys).
        let mut key: HKEY = 0;
        let status =
            unsafe { RegOpenKeyExW(root, subkey_w.as_ptr(), 0, KEY_READ | KEY_WRITE, &mut key) };
        if status != 0 {
            return Err(format!(
                "Failed to open registry key `{subkey}` (error {status}){}",
                if scope == Scope::System {
                    " — run from an elevated (Administrator) shell"
                } else {
                    ""
                }
            ));
        }
        Ok(key)
    }

    fn query_string(key: HKEY, name: &str) -> Result<Option<String>, String> {
        let name_w = to_wide(name);
        let mut dtype: REG_VALUE_TYPE = 0;
        let mut len = 0u32;
        // SAFETY: `key` is an open handle; both calls use valid out-params.
        let status = unsafe {
            RegQueryValueExW(
                key,
                name_w.as_ptr(),
                std::ptr::null(),
                &mut dtype,
                std::ptr::null_mut(),
                &mut len,
            )
        };
        if status == 2 {
            return Ok(None); // not present
        }
        if status != 0 {
            return Err(format!(
                "Failed to read registry value `{name}` (error {status})"
            ));
        }
        if len == 0 || len % 2 != 0 {
            return Ok(None);
        }
        let mut buf = vec![0u16; (len / 2) as usize];
        // SAFETY: `buf` has room for the reported byte length.
        let status = unsafe {
            RegQueryValueExW(
                key,
                name_w.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut len,
            )
        };
        if status != 0 {
            return Err(format!(
                "Failed to read registry value `{name}` (error {status})"
            ));
        }
        while buf.last() == Some(&0) {
            buf.pop();
        }
        Ok(Some(String::from_utf16_lossy(&buf)))
    }

    /// Write a string value, choosing REG_EXPAND_SZ for PATH (it commonly
    /// contains %SYSTEMROOT% style references that must stay expandable).
    fn write_string(key: HKEY, name: &str, value: &str, expand: bool) -> Result<(), String> {
        let name_w = to_wide(name);
        let value_w = to_wide(value);
        let dtype = if expand { REG_EXPAND_SZ } else { REG_SZ };
        // SAFETY: `key` is an open handle; `value_w` is a valid null-terminated
        // buffer and `cbData` counts its total byte length.
        let status = unsafe {
            RegSetValueExW(
                key,
                name_w.as_ptr(),
                0,
                dtype,
                value_w.as_ptr().cast(),
                (value_w.len() * 2) as u32,
            )
        };
        if status != 0 {
            return Err(format!(
                "Failed to write registry value `{name}` (error {status})"
            ));
        }
        Ok(())
    }

    fn broadcast_change() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
        };
        let environment = to_wide("Environment");
        // SAFETY: informational broadcast with a valid string LPARAM.
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                environment.as_ptr() as isize,
                SMTO_ABORTIFHUNG,
                5000,
                std::ptr::null_mut(),
            );
        }
    }

    let root: HKEY = match scope {
        Scope::System => HKEY_LOCAL_MACHINE,
        Scope::User => HKEY_CURRENT_USER,
    };
    let key = open_key(root, scope)?;

    // 1. AdeshLang variables (plain REG_SZ).
    for (name, value) in env_pairs(home) {
        write_string(key, &name, &value, false)?;
    }

    // 2. PATH: append <home>\bin and the active toolchain's bin when missing.
    let path_entries = [home.join("bin"), active_toolchain_root(home).join("bin")];
    let existing = query_string(key, "PATH")?;
    let mut parts: Vec<String> = existing
        .as_deref()
        .map(|p| p.split(';').map(str::to_string).collect())
        .unwrap_or_default();
    let mut changed = false;
    for entry in &path_entries {
        let entry_str = entry.display().to_string();
        let present = parts
            .iter()
            .any(|p| p.eq_ignore_ascii_case(&entry_str) && !p.is_empty());
        if !present {
            parts.push(entry_str);
            changed = true;
        }
    }
    if changed {
        let new_path = parts.join(";");
        write_string(key, "PATH", &new_path, true)?;
    }

    // SAFETY: closing an open handle.
    unsafe { RegCloseKey(key) };
    broadcast_change();

    println!(
        "  ✓ Registered AdeshLang environment variables and PATH entries in the {} environment",
        scope.as_str()
    );
    if changed {
        println!("  ! New PATH entries take effect in newly opened terminals");
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
pub fn expose(_home: &Path, _scope: Scope) -> Result<(), String> {
    Err("Toolchain exposure is not supported on this platform".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_pairs_always_include_home_and_toolchain() {
        let home = Path::new("/opt/adeshlang");
        let pairs = env_pairs(home);
        assert!(
            pairs
                .iter()
                .any(|(k, v)| k == "ADESH_HOME" && v == "/opt/adeshlang")
        );
        // ADESH_TOOLCHAIN always tracks the active (found) toolchain root.
        let expected_root = active_toolchain_root(home).display().to_string();
        assert!(
            pairs
                .iter()
                .any(|(k, v)| k == "ADESH_TOOLCHAIN" && *v == expected_root)
        );
    }

    #[test]
    fn scope_names_are_stable() {
        assert_eq!(Scope::System.as_str(), "system");
        assert_eq!(Scope::User.as_str(), "user");
    }
}
