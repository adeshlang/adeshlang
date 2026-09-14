use std::env;
use std::path::PathBuf;

/// Finds an executable by name in the following locations:
/// 1. Explicit environment variable check (e.g. ADESH_BINARY, ALS_BINARY, ADL_BINARY, ADESH_EDITOR_BINARY)
/// 2. Current executable's directory
/// 3. Sibling build directories (e.g., target/release, target/debug)
/// 4. Adesh installation directory (~/.adesh/bin or platform equivalent)
/// 5. System PATH
pub fn find_executable(binary_basename: &str) -> Option<PathBuf> {
    let exe_name = if cfg!(windows) && !binary_basename.to_lowercase().ends_with(".exe") {
        format!("{}.exe", binary_basename)
    } else {
        binary_basename.to_string()
    };

    // 1. Explicit env var override
    let env_var_name = match binary_basename {
        "adesh" => "ADESH_BINARY",
        "adl" => "ADL_BINARY",
        "als" | "adesh-language-server" => "ALS_BINARY",
        "adesh-editor" => "ADESH_EDITOR_BINARY",
        _ => "",
    };

    if !env_var_name.is_empty() {
        if let Ok(env_path) = env::var(env_var_name) {
            let p = PathBuf::from(&env_path);
            if p.is_file() {
                return Some(p);
            }
        }
    }

    // 2. Directory of current executable
    if let Ok(current_exe) = env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            let candidate = exe_dir.join(&exe_name);
            if candidate.is_file() {
                return Some(candidate);
            }

            // Sibling target/release or target/debug
            if let Some(parent) = exe_dir.parent() {
                let debug_cand = parent.join("debug").join(&exe_name);
                if debug_cand.is_file() {
                    return Some(debug_cand);
                }
                let release_cand = parent.join("release").join(&exe_name);
                if release_cand.is_file() {
                    return Some(release_cand);
                }
            }
        }
    }

    // 3. Adesh home / install dir (~/.adesh/bin)
    if let Some(home) = dirs::home_dir() {
        let install_cand = home.join(".adesh").join("bin").join(&exe_name);
        if install_cand.is_file() {
            return Some(install_cand);
        }
    }

    // 4. PATH search
    if let Ok(path_var) = env::var("PATH") {
        for path_entry in env::split_paths(&path_var) {
            let candidate = path_entry.join(&exe_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exe_name_formatting() {
        let found = find_executable("non_existent_binary_12345");
        assert!(found.is_none());
    }
}
