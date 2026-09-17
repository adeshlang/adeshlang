//! `adesh toolchain install` — installs the version-pinned LLVM/MLIR
//! toolchain during (or after) installation, verifies it, and optionally
//! exposes it system-wide so every program can use clang/lld/llc/mlir tools.
//!
//! Channels, in priority order (per project decision):
//!   1. Pinned upstream binaries named in the toolchain manifest (LLVM's own
//!      official GitHub releases). Archives are extracted; upstream installer
//!      executables (Windows NSIS) are run silently.
//!   2. `--use-system-packages`: the system package manager (apt/dnf/pacman/
//!      brew/winget), used as a fallback when downloads are unavailable.
//! GPU support: when prebuilt MLIR tools lack the NVVM/ROCDL dialects,
//! `--build-mlir-source` builds them from the pinned upstream source.

use std::path::{Path, PathBuf};

use super::super::expose::{self, Scope};
use super::super::manifest::{self, ToolchainManifest};
use super::super::resolver::{bundled_root, installation_home, is_usable_existing, tool_version};
use super::super::source_build;

pub struct InstallOptions {
    pub components: Vec<String>,
    pub manifest: Option<String>,
    pub scope: Option<Scope>,
    pub force: bool,
    pub allow_unverified: bool,
    pub dry_run: bool,
    /// Build the MLIR GPU tools (mlir-opt/mlir-translate with NVVM/ROCDL)
    /// from the pinned upstream LLVM source when prebuilt binaries lack the
    /// GPU dialects. Adds 30–90 minutes.
    pub build_mlir_source: bool,
    /// Skip downloads entirely and install the toolchain through the
    /// system package manager (apt/dnf/pacman/brew/winget).
    pub use_system_packages: bool,
}

pub fn parse_install_options(args: &[String]) -> Result<InstallOptions, String> {
    let mut options = InstallOptions {
        components: Vec::new(),
        manifest: None,
        scope: None,
        force: false,
        allow_unverified: false,
        dry_run: false,
        build_mlir_source: false,
        use_system_packages: false,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--system" | "--machine" => {
                if options.scope.is_some() {
                    return Err("Choose either --system or --user, not both".to_string());
                }
                options.scope = Some(Scope::System);
            }
            "--user" => {
                if options.scope.is_some() {
                    return Err("Choose either --system or --user, not both".to_string());
                }
                options.scope = Some(Scope::User);
            }
            "--force" => options.force = true,
            "--allow-unverified" => options.allow_unverified = true,
            "--dry-run" => options.dry_run = true,
            "--build-mlir-source" => options.build_mlir_source = true,
            "--use-system-packages" => options.use_system_packages = true,
            "--manifest" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--manifest requires a path or URL".to_string())?;
                options.manifest = Some(value.clone());
            }
            flag if flag.starts_with("--manifest=") => {
                options.manifest = Some(flag.trim_start_matches("--manifest=").to_string());
            }
            flag if flag.starts_with("--components=") => {
                let list = flag.trim_start_matches("--components=");
                options.components = list
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect();
            }
            flag if flag.starts_with('-') => {
                return Err(format!(
                    "Unknown option `{flag}` for `adesh toolchain install`"
                ));
            }
            component => options.components.push(component.to_string()),
        }
    }
    Ok(options)
}

/// Load the manifest: explicit path/URL, env var, bundled copy, then remote.
fn load_manifest(explicit: Option<&str>, home: Option<&Path>) -> Result<ToolchainManifest, String> {
    if let Some(location) = explicit {
        if location.starts_with("https://") {
            let temp = std::env::temp_dir().join("adesh-toolchain-manifest.json");
            super::super::download::download_to(location, &temp)?;
            let text = std::fs::read_to_string(&temp)
                .map_err(|e| format!("Failed reading manifest: {e}"))?;
            let _ = std::fs::remove_file(&temp);
            return manifest::parse_manifest(&text);
        }
        return manifest::load_from_file(Path::new(location));
    }
    for candidate in manifest::local_manifest_candidates(home) {
        let as_text = candidate.display().to_string();
        // ADESH_TOOLCHAIN_MANIFEST may hold a URL instead of a path.
        if as_text.starts_with("https://") {
            return load_manifest(Some(&as_text), home);
        }
        if candidate.is_file() {
            return manifest::load_from_file(&candidate);
        }
    }
    let url = manifest::manifest_url();
    println!("  No local manifest found; fetching {url}");
    let temp = std::env::temp_dir().join("adesh-toolchain-manifest.json");
    super::super::download::download_to(&url, &temp)?;
    let text =
        std::fs::read_to_string(&temp).map_err(|e| format!("Failed reading manifest: {e}"))?;
    let _ = std::fs::remove_file(&temp);
    manifest::parse_manifest(&text)
}

fn exe_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// True when the requested component is already installed with a compatible
/// version. Only LLVM (the required component) is version-gated; MLIR is
/// validated by the presence of its tools.
fn already_installed(home: &Path, component: &str) -> bool {
    let toolchain = expose::active_toolchain_root(home);
    let bin = toolchain.join("bin");
    match component {
        "llvm" => {
            let clang = bin.join(exe_name("clang"));
            // Any existing LLVM on the machine is reused regardless of version
            // (the pinned version only governs fresh downloads).
            clang.is_file() && is_usable_existing(tool_version(&clang).as_deref())
        }
        "mlir" => {
            let mlir_opt = bin.join(exe_name("mlir-opt"));
            let mlir_translate = bin.join(exe_name("mlir-translate"));
            mlir_opt.is_file() && mlir_translate.is_file()
        }
        _ => false,
    }
}

pub fn execute_install_command(args: &[String]) {
    let options = match parse_install_options(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("adesh toolchain install: {e}");
            eprintln!(
                "Usage: adesh toolchain install [components] [--system|--user] \
                 [--manifest <path|url>] [--build-mlir-source] [--use-system-packages] [--force]"
            );
            std::process::exit(2);
        }
    };
    let home = installation_home().unwrap_or_else(|| PathBuf::from("."));
    let platform = manifest::platform_key();

    // Alternative channel: the system package manager.
    if options.use_system_packages {
        println!("AdeshLang Toolchain Installer (system packages)");
        println!("────────────────────────────────────────────");
        println!("  Platform   : {platform}");
        if let Err(e) = install_via_system_packages() {
            eprintln!("  ✗ {e}");
            std::process::exit(1);
        }
        run_mlir_source_build_if_requested(&home, &options);
        finish_with_exposure(&home, options.scope);
        return;
    }

    let toolchain_manifest = match load_manifest(options.manifest.as_deref(), Some(&home)) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("  ✗ Failed to load toolchain manifest: {e}");
            eprintln!("    Install offline later with: adesh toolchain install --manifest <path>");
            eprintln!(
                "    Or use the system package manager: adesh toolchain install --use-system-packages"
            );
            std::process::exit(1);
        }
    };
    let components = match toolchain_manifest.resolve_components(&options.components) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("  ✗ {e}");
            std::process::exit(2);
        }
    };
    println!("AdeshLang Toolchain Installer");
    println!("────────────────────────────────────────────");
    println!("  Platform   : {platform}");
    println!("  Components : {}", components.join(", "));
    println!(
        "  Target     : {}",
        home.join("toolchain").join("llvm").display()
    );

    // Plan and validate downloads up-front (dry-run friendly).
    let mut plan: Vec<(String, manifest::ComponentDownload)> = Vec::new();
    for component in &components {
        if !options.force && already_installed(&home, component) {
            println!(
                "  ✓ {component}: already present on this machine — reusing it, nothing downloaded (use --force to reinstall)"
            );
            continue;
        }
        let spec = match toolchain_manifest.component(&platform, component) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("  ✗ {e}");
                std::process::exit(1);
            }
        };
        if spec.url.is_empty() {
            let required = toolchain_manifest
                .components
                .get(component)
                .map(|c| c.required)
                .unwrap_or(true);
            if required {
                eprintln!("  ✗ Component `{component}` has no download URL for {platform} yet");
                eprintln!(
                    "    Try the system package manager: adesh toolchain install --use-system-packages"
                );
                std::process::exit(1);
            }
            println!(
                "  ! Component `{component}` ships no prebuilt binary for {platform}; \
                 build it from source with --build-mlir-source"
            );
            continue;
        }
        if spec.sha256.is_empty() && !options.allow_unverified {
            eprintln!(
                "  ✗ Component `{component}` has no SHA-256 checksum in the manifest; refusing to install \
                 (development builds may pass --allow-unverified)"
            );
            std::process::exit(1);
        }
        plan.push((component.clone(), spec.clone()));
    }
    if plan.is_empty() {
        println!("\nNothing to download. The toolchain is already in place.");
        run_mlir_source_build_if_requested(&home, &options);
        finish_with_exposure(&home, options.scope);
        return;
    }
    if options.dry_run {
        println!("\nDry run — would fetch:");
        for (component, spec) in &plan {
            println!(
                "  {component}: {} ({}, {})",
                spec.url,
                if spec.is_installer() {
                    "installer"
                } else {
                    "archive"
                },
                describe_size(spec.size)
            );
        }
        return;
    }

    let download_dir = std::env::temp_dir();
    let llvm_root = home.join("toolchain").join("llvm");

    for (component, spec) in &plan {
        println!(
            "\n▶ Installing {component} v{}",
            toolchain_manifest
                .components
                .get(component)
                .map(|c| c.version.as_str())
                .unwrap_or("unknown")
        );
        let artifact_name = spec
            .url
            .rsplit('/')
            .next()
            .unwrap_or("artifact")
            .split('?')
            .next()
            .unwrap_or("artifact")
            .to_string();
        let artifact_path = download_dir.join(format!("adesh-{component}-{artifact_name}"));

        let result = match super::super::download::download_to(&spec.url, &artifact_path) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("  ✗ Download failed: {e}");
                eprintln!(
                    "    Fallback: adesh toolchain install --use-system-packages --{}",
                    if options.scope == Some(Scope::System) {
                        "system"
                    } else {
                        "user"
                    }
                );
                let _ = std::fs::remove_file(&artifact_path);
                std::process::exit(1);
            }
        };

        if !spec.sha256.is_empty() {
            if !super::super::archive::sha256_matches(&result.sha256, &spec.sha256) {
                eprintln!("  ✗ SHA-256 mismatch for {component}");
                eprintln!("    expected {} — got {}", spec.sha256, result.sha256);
                let _ = std::fs::remove_file(&artifact_path);
                std::process::exit(1);
            }
            println!("  ✓ SHA-256 verified: {}", result.sha256);
        } else {
            println!("  ⚠ Installed WITHOUT checksum verification (--allow-unverified)");
        }

        if spec.is_installer() {
            if let Err(e) = run_upstream_installer(&artifact_path, &llvm_root) {
                eprintln!("  ✗ {e}");
                let _ = std::fs::remove_file(&artifact_path);
                std::process::exit(1);
            }
            println!(
                "  ✓ {component} installed via the upstream installer into {}",
                llvm_root.display()
            );
        } else {
            let format = if spec.format.is_empty() {
                super::super::archive::format_from_name(&artifact_name)
                    .or_else(|| super::super::archive::format_from_name(&spec.url))
                    .unwrap_or("zip")
                    .to_string()
            } else {
                spec.format.clone()
            };

            // Stage into a fresh directory, then merge into the toolchain root.
            let staging = home.join("toolchain").join(format!(".stage-{component}"));
            let _ = std::fs::remove_dir_all(&staging);
            match super::super::archive::extract(&artifact_path, &format, &staging) {
                Ok(count) => println!("  ✓ Extracted {count} entries"),
                Err(e) => {
                    eprintln!("  ✗ Extraction failed: {e}");
                    let _ = std::fs::remove_file(&artifact_path);
                    let _ = std::fs::remove_dir_all(&staging);
                    std::process::exit(1);
                }
            }

            // Archives either contain the toolchain files at the root, or a
            // single top-level directory (e.g. `clang+llvm-18.1.8-.../bin/clang`).
            let source = single_root(&staging).unwrap_or_else(|| staging.clone());
            merge_into(&source, &llvm_root);
            let _ = std::fs::remove_dir_all(&staging);
            println!("  ✓ {component} installed to {}", llvm_root.display());
        }
        let _ = std::fs::remove_file(&artifact_path);
    }

    // Post-install sanity: report which clang the toolchain resolved to.
    // Any version is accepted (an existing toolchain is never version-gated).
    let toolchain = expose::active_toolchain_root(&home);
    let clang = toolchain.join("bin").join(exe_name("clang"));
    if clang.is_file() {
        match tool_version(&clang) {
            Some(version) => println!("\n  ✓ clang {version} ready at {}", toolchain.display()),
            None => println!("\n  ⚠ Could not determine the resolved clang version"),
        }
    }

    run_mlir_source_build_if_requested(&home, &options);
    finish_with_exposure(&home, options.scope);
}

/// Run an upstream installer executable silently (NSIS-style `/S`), redirecting
/// it into the AdeshLang-managed toolchain directory (`<home>\toolchain\llvm`)
/// instead of the machine-wide `C:\Program Files\LLVM`. That keeps the whole
/// toolchain inside the AdeshLang installation, removable with a single
/// uninstall.
fn run_upstream_installer(installer: &Path, target_dir: &Path) -> Result<(), String> {
    if !cfg!(windows) {
        return Err("Installer-executable components are only supported on Windows".to_string());
    }
    println!(
        "  ▶ Running the upstream installer silently into {} (this can take a few minutes)",
        target_dir.display()
    );
    let _ = std::fs::create_dir_all(target_dir);
    let mut command = std::process::Command::new(installer);
    command.arg("/S");
    // NSIS quirk: /D must be the LAST argument and must not be quoted, even
    // when the path contains spaces. raw_arg bypasses Rust's automatic
    // Windows argument quoting so the destination survives intact.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.raw_arg(format!("/D={}", target_dir.display()));
    }
    let status = command
        .status()
        .map_err(|e| format!("Failed to launch installer: {e}"))?;
    if !status.success() {
        return Err(format!("Installer exited with {status}"));
    }
    // NSIS /S returns while a child process finishes; wait for the toolchain
    // to appear at the requested destination.
    let clang = target_dir.join("bin").join(exe_name("clang"));
    let mut waited = 0u64;
    while waited < 600 {
        if clang.is_file() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(5));
        waited += 5;
    }
    Err("The installer did not produce a usable LLVM within 10 minutes".to_string())
}

/// Trigger the MLIR-from-source build when requested, reporting the GPU
/// dialect probe result first.
fn run_mlir_source_build_if_requested(home: &Path, options: &InstallOptions) {
    if !options.build_mlir_source {
        return;
    }
    let toolchain = expose::active_toolchain_root(home);
    let bin = toolchain.join("bin");
    let mlir_opt = bin.join(exe_name("mlir-opt"));
    if !mlir_opt.is_file() {
        println!("\n▶ mlir-opt not present; building MLIR from upstream source");
    } else if source_build::mlir_gpu_dialects_available(&bin) {
        println!(
            "\n  ✓ mlir-opt/mlir-translate already support GPU dialects — no source build needed"
        );
        return;
    } else {
        println!(
            "\n▶ Installed MLIR tools lack NVVM/ROCDL GPU dialects; building from upstream source"
        );
    }
    let llvm_root = home.join("toolchain").join("llvm");
    match source_build::build_mlir_from_source(&source_build::source_url(), &llvm_root) {
        Ok(_) => {
            if source_build::mlir_gpu_dialects_available(&llvm_root.join("bin")) {
                println!("  ✓ GPU dialect support verified (NVVM/ROCDL)");
            } else {
                println!(
                    "  ⚠ Built MLIR still lacks GPU dialects — GPU runs will use the interpreter fallback"
                );
            }
        }
        Err(e) => {
            eprintln!("  ✗ MLIR source build failed: {e}");
            eprintln!("    GPU programs will fall back to the interpreter until this succeeds.");
        }
    }
}

fn finish_with_exposure(home: &Path, scope: Option<Scope>) {
    if let Some(scope) = scope {
        println!();
        match expose::expose(home, scope) {
            Ok(()) => {}
            Err(e) => {
                eprintln!("  ✗ Toolchain exposure failed: {e}");
                std::process::exit(1);
            }
        }
    }
    println!("\nNext step: run `adesh doctor` to verify the installation.");
}

fn describe_size(size: u64) -> String {
    if size == 0 {
        "size unknown".to_string()
    } else if size >= 1024 * 1024 {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    } else if size >= 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else {
        format!("{size} bytes")
    }
}

/// If the extracted tree contains exactly one top-level directory, return it.
fn single_root(dir: &Path) -> Option<PathBuf> {
    let mut entries = std::fs::read_dir(dir).ok()?.flatten();
    let first = entries.next()?;
    if entries.next().is_some() {
        return None;
    }
    let path = first.path();
    path.is_dir().then_some(path)
}

/// Copy `source` into `dest`, overwriting files but never deleting extras.
fn merge_into(source: &Path, dest: &Path) {
    for entry in walk(source) {
        let relative = entry.strip_prefix(source).unwrap_or(&entry);
        let target = dest.join(relative);
        if entry.is_dir() {
            let _ = std::fs::create_dir_all(&target);
        } else {
            if let Some(parent) = target.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::copy(&entry, &target);
        }
    }
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![dir.to_path_buf()];
    let mut queue: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(current) = queue.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.push(path.clone());
                queue.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out
}

// ── System package manager channel ───────────────────────────────────────────

/// Install the pinned toolchain through the system package manager and make
/// the unsuffixed tool names resolvable (clang-18 → clang, mlir-opt-18 → …).
fn install_via_system_packages() -> Result<(), String> {
    #[cfg(windows)]
    {
        let winget = which("winget").ok_or("winget is not available on this system")?;
        println!("  ▶ Installing LLVM via winget (may prompt for elevation)");
        let status = std::process::Command::new(&winget)
            .args(["install", "--id", "LLVM.LLVM", "-e"])
            .status()
            .map_err(|e| format!("Failed to run winget: {e}"))?;
        if !status.success() {
            return Err(format!("winget exited with {status}"));
        }
        let home = installation_home().unwrap_or_else(|| PathBuf::from("."));
        let toolchain = expose::active_toolchain_root(&home);
        println!("  ✓ LLVM installed to {}", toolchain.display());
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        // macOS: Homebrew llvm@18 (keg-only; linked below).
        if cfg!(target_os = "macos") {
            let brew = which("brew").ok_or("Homebrew is not available on this system")?;
            println!("  ▶ Installing llvm@18 via Homebrew");
            let status = std::process::Command::new(&brew)
                .args(["install", "llvm@18"])
                .status()
                .map_err(|e| format!("Failed to run brew: {e}"))?;
            if !status.success() {
                return Err(format!("brew exited with {status}"));
            }
            return Ok(());
        }

        // Linux: versioned distro packages for LLVM 18.
        let (manager, packages) = if which("apt-get").is_some() || which("apt").is_some() {
            (
                which("apt-get").or_else(|| which("apt")).unwrap(),
                vec!["clang-18", "lld-18", "llvm-18", "mlir-18-tools"],
            )
        } else if which("dnf").is_some() {
            (
                which("dnf").unwrap(),
                vec!["clang-tools-extra-18", "lld-18", "llvm-18", "mlir-18-tools"],
            )
        } else if which("pacman").is_some() {
            (
                which("pacman").unwrap(),
                vec!["clang", "lld", "llvm", "mlir"],
            )
        } else if which("zypper").is_some() {
            (
                which("zypper").unwrap(),
                vec!["clang18", "lld18", "llvm18", "mlir18"],
            )
        } else {
            return Err(
                "No supported package manager found (apt, dnf, pacman, or zypper). \
                 Install LLVM 18 manually or use the manifest download channel."
                    .to_string(),
            );
        };

        println!(
            "  ▶ Installing {:?} via the system package manager",
            packages
        );
        let manager_str = manager.to_string_lossy();
        if manager_str.contains("apt") {
            command.args(["install", "-y"]);
        } else if manager_str.contains("dnf") {
            command.args(["install", "-y"]);
        } else if manager_str.contains("zypper") {
            command.arg("--non-interactive").arg("install");
        } else {
            command.args(["-S", "--noconfirm"]);
        }
        for package in &packages {
            command.arg(package);
        }
        let status = command
            .status()
            .map_err(|e| format!("Failed to run package manager: {e}"))?;
        if !status.success() {
            return Err(format!("package manager exited with {status}"));
        }

        // Distro packages install versioned names (clang-18, mlir-opt-18).
        // Link the canonical names into /usr/local/bin for all users.
        if which("sudo").is_none()
            || std::env::var_os("USER").as_deref() == Some(std::ffi::OsStr::new("root"))
        {
            link_versioned_tools()?;
        } else {
            println!("  ! Skipping /usr/local/bin links (not root); re-run with sudo or call:");
            println!("    sudo adesh toolchain install --use-system-packages");
        }
        Ok(())
    }
}

#[cfg(not(windows))]
fn link_versioned_tools() -> Result<(), String> {
    let search_dirs = [
        PathBuf::from("/usr/lib/llvm-18/bin"),
        PathBuf::from("/usr/bin"),
    ];
    let tools = [
        "clang",
        "clang++",
        "lld",
        "ld.lld",
        "llc",
        "llvm-ar",
        "mlir-opt",
        "mlir-translate",
    ];
    std::fs::create_dir_all("/usr/local/bin")
        .map_err(|e| format!("Failed to create /usr/local/bin: {e}"))?;
    let mut linked = 0;
    for tool in tools {
        let suffixed = format!("{tool}-18");
        let mut found: Option<PathBuf> = None;
        for dir in &search_dirs {
            let candidate = dir.join(&suffixed);
            if candidate.is_file() {
                found = Some(candidate);
                break;
            }
        }
        let Some(source) = found else { continue };
        let link = Path::new("/usr/local/bin").join(tool);
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&source, &link)
            .map_err(|e| format!("Failed to link {tool}: {e}"))?;
        linked += 1;
    }
    println!("  ✓ Linked {linked} versioned tools into /usr/local/bin");
    Ok(())
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(exe_name(name));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// `adesh toolchain expose` — re-run system exposure without downloading.
pub fn execute_expose_command(args: &[String]) {
    let mut scope = None;
    for arg in args {
        match arg.as_str() {
            "--system" | "--machine" => scope = Some(Scope::System),
            "--user" => scope = Some(Scope::User),
            other => {
                eprintln!("Unknown option `{other}` for `adesh toolchain expose`");
                eprintln!("Usage: adesh toolchain expose --system|--user");
                std::process::exit(2);
            }
        }
    }
    let Some(scope) = scope else {
        eprintln!("Choose a scope: adesh toolchain expose --system|--user");
        std::process::exit(2);
    };
    let home = installation_home().unwrap_or_else(|| PathBuf::from("."));
    if bundled_root().is_none()
        && expose::active_toolchain_root(&home)
            .join("bin")
            .join(exe_name("clang"))
            .is_file()
    {
        // Upstream-installer location still counts as a usable toolchain.
    } else if bundled_root().is_none() {
        eprintln!("Toolchain is not installed — run `adesh toolchain install` first");
        std::process::exit(1);
    }
    if let Err(e) = expose::expose(&home, scope) {
        eprintln!("  ✗ {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_component_selection() {
        let o = parse_install_options(&["llvm".to_string()]).unwrap();
        assert_eq!(o.components, vec!["llvm".to_string()]);
    }

    #[test]
    fn parses_flags() {
        let o = parse_install_options(&[
            "--system".to_string(),
            "--manifest=https://example.com/m.json".to_string(),
            "--build-mlir-source".to_string(),
            "--force".to_string(),
        ])
        .unwrap();
        assert_eq!(o.scope, Some(Scope::System));
        assert!(o.force);
        assert!(o.build_mlir_source);
        assert_eq!(o.manifest.as_deref(), Some("https://example.com/m.json"));
    }

    #[test]
    fn rejects_conflicting_scopes() {
        let o = parse_install_options(&["--system".to_string(), "--user".to_string()]);
        assert!(o.is_err());
    }

    #[test]
    fn rejects_unknown_flags() {
        assert!(parse_install_options(&["--nope".to_string()]).is_err());
    }

    #[test]
    fn manifest_flag_requires_value() {
        assert!(parse_install_options(&["--manifest".to_string()]).is_err());
    }

    #[test]
    fn describe_size_is_human_friendly() {
        assert_eq!(describe_size(0), "size unknown");
        assert_eq!(describe_size(512), "512 bytes");
        assert!(describe_size(5 * 1024 * 1024).contains("MB"));
    }
}
