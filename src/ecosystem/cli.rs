use crate::ecosystem::project::{ProjectLayout, ProjectTemplate};
use crate::ecosystem::resolver::DependencyResolver;
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdlCommand {
    New,
    Init,
    Build,
    Run,
    Test,
    Ai,
    Fmt,
    Lint,
    Check,
    Doc,
    Clean,
    Add,
    Remove,
    Update,
    Upgrade,
    Install,
    Restore,
    Resolve,
    Search,
    Info,
    List,
    Login,
    Logout,
    Publish,
    Unpublish,
    Cache,
    Doctor,
    Repair,
    Workspace,
    Graph,
    Tree,
    Why,
    Vendor,
    Benchmark,
    Profile,
    Version,
    Config,
    Env,
    Target,
    Backend,
    Script,
    Generate,
    Package,
    Verify,
    LocateBin,
    Help,
}

#[derive(Debug, Clone)]
pub struct CliOptions {
    pub command: AdlCommand,
    pub args: Vec<String>,
    pub project_dir: PathBuf,
    pub verbose: bool,
    pub release: bool,
    pub debug: bool,
    pub offline: bool,
    pub force: bool,
    pub dry_run: bool,
    pub help: bool,
}

pub fn run_from_env() -> Result<(), String> {
    let options = parse_args(env::args().collect::<Vec<_>>())?;
    dispatch(options)
}

pub fn parse_args(args: Vec<String>) -> Result<CliOptions, String> {
    let mut command = AdlCommand::Help;
    let mut command_set = false;
    let mut remaining = Vec::new();
    let mut verbose = false;
    let mut release = false;
    let mut debug = false;
    let mut offline = false;
    let mut force = false;
    let mut dry_run = false;
    let mut help = false;

    for arg in args.iter().skip(1) {
        match arg.as_str() {
            "--help" | "-h" => help = true,
            "--verbose" | "-v" => verbose = true,
            "--release" => release = true,
            "--debug" => debug = true,
            "--offline" => offline = true,
            "--force" => force = true,
            "--dry-run" => dry_run = true,
            "--locate-bin" | "--locate-bins" | "--locate" => {
                command = AdlCommand::LocateBin;
                command_set = true;
            }
            value if value.starts_with('-') => remaining.push(value.to_string()),
            value if !command_set => {
                command = parse_command(value)?;
                command_set = true;
            }
            value => remaining.push(value.to_string()),
        }
    }

    Ok(CliOptions {
        command,
        args: remaining,
        project_dir: env::current_dir().map_err(|error| error.to_string())?,
        verbose,
        release,
        debug,
        offline,
        force,
        dry_run,
        help,
    })
}

pub fn dispatch(options: CliOptions) -> Result<(), String> {
    if options.help || matches!(options.command, AdlCommand::Help) {
        print_help();
        return Ok(());
    }

    let layout = ProjectLayout::discover(&options.project_dir)
        .unwrap_or_else(|| ProjectLayout::new(&options.project_dir));

    match options.command {
        AdlCommand::Ai => {
            crate::cli::commands::execute_ai_command(&options.args);
            return Ok(());
        }
        AdlCommand::Repair => {
            crate::cli::repair::execute_repair_command();
            return Ok(());
        }
        AdlCommand::Doctor => {
            if layout.manifest_path().exists() {
                let _ = handle_metadata_command(
                    options.command,
                    &layout,
                    &options.args,
                    options.dry_run,
                );
            }
            crate::cli::doctor::execute_doctor_command();
            return Ok(());
        }
        AdlCommand::New | AdlCommand::Init => {
            let (template, name, target_dir) = parse_scaffold_args(&options.args, options.command)?;
            let layout = ProjectLayout::new(&target_dir);
            if options.dry_run {
                println!(
                    "Would create {:?} project '{}' at {}",
                    template,
                    name,
                    layout.root.display()
                );
                return Ok(());
            }
            layout.create_template(template, &name)?;
            let is_new = matches!(options.command, AdlCommand::New);
            if is_new {
                println!(
                    "✨ Created new AdeshLang project '{}' in {}",
                    name,
                    layout.root.display()
                );
                println!("\nNext steps:");
                println!("  cd {}", name);
                println!("  adl run     # or: adesh run src/main.adesh");
                println!("  adl build   # or: adesh build src/main.adesh");
            } else {
                println!(
                    "✨ Initialized AdeshLang project in {}",
                    layout.root.display()
                );
                println!("\nNext steps:");
                println!("  adl run     # or: adesh run src/main.adesh");
                println!("  adl build   # or: adesh build src/main.adesh");
            }
        }
        AdlCommand::Add => {
            let (name, requirement) = parse_dependency_args(&options.args)?;
            if options.dry_run {
                println!("Would add dependency {} {}", name, requirement);
                return Ok(());
            }
            layout.add_dependency_stub(&name, &requirement)?;
            println!("Added dependency {} {}", name, requirement);
        }
        AdlCommand::Remove => {
            let name = options
                .args
                .first()
                .cloned()
                .ok_or_else(|| "remove requires a dependency name".to_string())?;
            if options.dry_run {
                println!("Would remove dependency {}", name);
                return Ok(());
            }
            layout.remove_dependency_stub(&name)?;
            println!("Removed dependency {}", name);
        }
        AdlCommand::Install
        | AdlCommand::Resolve
        | AdlCommand::Restore
        | AdlCommand::Update
        | AdlCommand::Upgrade => {
            let manifest = layout.load_manifest()?;
            manifest.validate()?;
            layout.ensure_layout()?;
            let (sanitized_manifest, confidential) = manifest.sanitize_confidential();
            let resolver = DependencyResolver {
                offline: options.offline,
                ..DependencyResolver::new()
            };
            let resolved = resolver.resolve_manifest(&sanitized_manifest, &layout.root)?;
            if options.dry_run {
                println!(
                    "Would write lockfile with {} dependencies and populate adl_modules",
                    resolved.lockfile.dependencies.len()
                );
            } else {
                let mut lockfile = resolved.lockfile;
                layout.persist_sanitized_state(&sanitized_manifest, &mut lockfile, confidential)?;
                layout.populate_adl_modules(&lockfile)?;
                println!(
                    "Generated {} and populated adl_modules",
                    layout.lockfile_path().display()
                );
            }
        }
        AdlCommand::Build | AdlCommand::Run | AdlCommand::Test | AdlCommand::Check => {
            let manifest = layout.load_manifest()?;
            manifest.validate()?;
            let (sanitized_manifest, confidential) = manifest.sanitize_confidential();
            let resolver = DependencyResolver {
                offline: options.offline,
                ..DependencyResolver::new()
            };
            let mut resolved = resolver.resolve_manifest(&sanitized_manifest, &layout.root)?;
            resolved.lockfile.set_confidential(confidential.clone());
            layout.write_manifest(&sanitized_manifest)?;
            layout.write_lockfile(&resolved.lockfile)?;
            let plan = build_plan(&layout, &sanitized_manifest, &resolved.graph, &options)?;
            println!("{}", plan);
        }
        AdlCommand::Login => {
            let token = options
                .args
                .first()
                .cloned()
                .ok_or_else(|| "login requires a token".to_string())?;
            layout.ensure_layout()?;
            fs::write(
                layout.adl_dir().join("credentials"),
                format!("token = \"{}\"", token),
            )
            .map_err(|e| e.to_string())?;
            println!("Logged in successfully.");
        }
        AdlCommand::Logout => {
            let creds = layout.adl_dir().join("credentials");
            if creds.exists() {
                fs::remove_file(creds).map_err(|e| e.to_string())?;
            }
            println!("Logged out successfully.");
        }
        AdlCommand::Publish => {
            let manifest = layout.load_manifest()?;
            manifest.validate()?;
            let creds = layout.adl_dir().join("credentials");
            if !creds.exists() {
                return Err("Not logged in. Run 'adl login <token>' first.".to_string());
            }
            let name = manifest.project_name().unwrap();
            let version = manifest
                .project_version()
                .ok_or("project has no version")?
                .to_string();
            let target_registry_dir = layout
                .root
                .join(".adl")
                .join("registry")
                .join("official")
                .join(&name)
                .join(&version);
            fs::create_dir_all(&target_registry_dir).map_err(|e| e.to_string())?;
            fs::write(
                target_registry_dir.join("package.tar.gz"),
                "ADL_PACKAGE_DATA",
            )
            .map_err(|e| e.to_string())?;
            fs::copy(
                layout.manifest_path(),
                target_registry_dir.join("adesh.adl"),
            )
            .map_err(|e| e.to_string())?;
            println!("Published {}@{} to registry.", name, version);
        }
        AdlCommand::Unpublish => {
            let manifest = layout.load_manifest()?;
            let name = manifest.project_name().unwrap();
            let version = options
                .args
                .first()
                .cloned()
                .ok_or_else(|| "unpublish requires a version".to_string())?;
            let target_registry_dir = layout
                .root
                .join(".adl")
                .join("registry")
                .join("official")
                .join(&name)
                .join(&version);
            if target_registry_dir.exists() {
                fs::remove_dir_all(target_registry_dir).map_err(|e| e.to_string())?;
                println!("Unpublished {}@{} from registry.", name, version);
            } else {
                println!("Version {} of {} not found in registry.", version, name);
            }
        }
        AdlCommand::Fmt
        | AdlCommand::Lint
        | AdlCommand::Doc
        | AdlCommand::Clean
        | AdlCommand::Cache
        | AdlCommand::Graph
        | AdlCommand::Tree
        | AdlCommand::Why
        | AdlCommand::Info
        | AdlCommand::List
        | AdlCommand::Vendor
        | AdlCommand::Benchmark
        | AdlCommand::Profile
        | AdlCommand::Version
        | AdlCommand::Config
        | AdlCommand::Env
        | AdlCommand::Target
        | AdlCommand::Backend
        | AdlCommand::Script
        | AdlCommand::Generate
        | AdlCommand::Package
        | AdlCommand::Verify
        | AdlCommand::Search
        | AdlCommand::Workspace
        | AdlCommand::LocateBin => {
            handle_metadata_command(options.command, &layout, &options.args, options.dry_run)?;
        }
        AdlCommand::Help => unreachable!(),
    }
    Ok(())
}

fn build_plan(
    layout: &ProjectLayout,
    manifest: &crate::ecosystem::manifest::Manifest,
    graph: &crate::ecosystem::resolver::DependencyGraph,
    options: &CliOptions,
) -> Result<String, String> {
    let name = manifest
        .project_name()
        .unwrap_or_else(|| "unknown".to_string());
    let version = manifest
        .project_version()
        .map(|v| v.to_string())
        .unwrap_or_else(|| "0.1.0".to_string());
    let mut lines = Vec::new();
    lines.push(format!("Adl build plan for {} {}", name, version));
    lines.push(format!("Project root: {}", layout.root.display()));

    // Read compiler configuration from manifest
    let mut backend = "interpreter".to_string();
    let mut opt_level = "debug".to_string();
    if let Some(compiler_sec) = manifest.section("compiler") {
        if let Some(
            crate::ecosystem::manifest::ManifestValue::Identifier(val)
            | crate::ecosystem::manifest::ManifestValue::String(val),
        ) = compiler_sec.field("backend")
        {
            backend = val.clone();
        }
        if let Some(
            crate::ecosystem::manifest::ManifestValue::Identifier(val)
            | crate::ecosystem::manifest::ManifestValue::String(val),
        ) = compiler_sec.field("opt-level")
        {
            opt_level = val.clone();
        }
    }

    lines.push(format!(
        "Mode: {}",
        if options.release {
            "release"
        } else if options.debug {
            "debug"
        } else {
            "default"
        }
    ));
    lines.push(format!("Compiler Backend: {}", backend));
    lines.push(format!("Opt Level: {}", opt_level));

    if let Some(dependencies) = manifest.dependencies() {
        lines.push(format!("Dependencies: {}", dependencies.items.len()));
    }
    lines.push(format!("Lockfile: {}", layout.lockfile_path().display()));
    lines.push(format!("Bin directory: {}", layout.bin_dir().display()));
    lines.push(format!(
        "Target directory: {}",
        layout.target_dir().display()
    ));
    lines.push(format!(
        "Package cache: {}",
        layout.packages_dir().display()
    ));
    lines.push(format!("Graph nodes: {}", graph.nodes.len()));
    Ok(lines.join("\n"))
}

fn handle_metadata_command(
    command: AdlCommand,
    layout: &ProjectLayout,
    args: &[String],
    dry_run: bool,
) -> Result<(), String> {
    match command {
        AdlCommand::Clean => {
            if dry_run {
                println!("Would clean {}", layout.artifacts_dir().display());
                println!("Would clean {}", layout.bin_dir().display());
                println!("Would clean {}", layout.target_dir().display());
                return Ok(());
            }
            if layout.artifacts_dir().exists() {
                let _ = fs::remove_dir_all(layout.artifacts_dir());
            }
            if layout.bin_dir().exists() {
                let _ = fs::remove_dir_all(layout.bin_dir());
            }
            if layout.target_dir().exists() {
                let _ = fs::remove_dir_all(layout.target_dir());
            }
            println!("Cleaned target artifacts and binary directories");
        }
        AdlCommand::Cache => {
            println!("Cache: {}", layout.packages_dir().display());
            println!("Cache index: {}", layout.cache_dir().display());
        }
        AdlCommand::Doctor => {
            println!("Project: {}", layout.root.display());
            println!("Manifest: {}", layout.manifest_path().display());
            println!("Lockfile: {}", layout.lockfile_path().display());
            println!("Adl directory: {}", layout.adl_dir().display());
            println!("Bin directory: {}", layout.bin_dir().display());
            println!("Target directory: {}", layout.target_dir().display());
            let builds = layout.locate_binary_builds(None);
            println!("Located binaries: {} build(s)", builds.len());
        }
        AdlCommand::LocateBin => {
            let filter = args.first().cloned();
            let name_filter = filter.as_deref();
            let builds = layout.locate_binary_builds(name_filter);
            let sources = layout.list_binary_sources();

            if let Some(target_name) = name_filter {
                if let Some(exact_path) = layout.locate_binary(target_name) {
                    println!("{}", exact_path.display());
                } else if !builds.is_empty() {
                    println!(
                        "Found {} binary build(s) matching '{}':",
                        builds.len(),
                        target_name
                    );
                    for build in &builds {
                        println!(
                            "  {} ({}, {} bytes) -> {}",
                            build.name,
                            build.kind.description(),
                            build.size_bytes,
                            build.path.display()
                        );
                    }
                } else {
                    println!("Binary '{}' not found in build directories.", target_name);
                    println!("Searched directories:");
                    for dir in layout.binary_search_dirs() {
                        println!("  {}", dir.display());
                    }
                    if sources.iter().any(|s| s == target_name) {
                        println!(
                            "\nHint: Source entry point for '{}' exists. Build it with:",
                            target_name
                        );
                        println!("  adl build --bin {}", target_name);
                    }
                }
            } else {
                println!("ADL Binary Builds & Targets:");
                println!("=============================");
                println!("Project Root:     {}", layout.root.display());
                println!("ADL Bin Dir:      {}", layout.bin_dir().display());
                println!("Target Bin Dir:   {}", layout.target_bin_dir().display());
                println!();

                if builds.is_empty() {
                    println!("No compiled binary builds found yet.");
                } else {
                    println!("Located Binary Builds ({}):", builds.len());
                    for build in &builds {
                        let exec_tag = if build.is_executable {
                            " [executable]"
                        } else {
                            ""
                        };
                        println!(
                            "  • {:<16} {:<18} {:>8} bytes{} -> {}",
                            build.name,
                            format!("[{}]", build.kind.description()),
                            build.size_bytes,
                            exec_tag,
                            build.path.display()
                        );
                    }
                }

                if !sources.is_empty() {
                    println!("\nConfigured Source Binary Targets ({}):", sources.len());
                    for src in &sources {
                        let built_status = if layout.locate_binary(src).is_some() {
                            "(built)"
                        } else {
                            "(not built)"
                        };
                        println!("  • {:<16} {}", src, built_status);
                    }
                    println!("\nBuild binaries using: adl build [--bin <name>]");
                }
            }
        }
        AdlCommand::Workspace => {
            println!("Workspace discovery: {}", layout.root.display());
        }
        AdlCommand::Graph
        | AdlCommand::Tree
        | AdlCommand::Why
        | AdlCommand::List
        | AdlCommand::Info => {
            let manifest = layout.load_manifest()?;
            let resolver = DependencyResolver::new();
            let resolved = resolver.resolve_manifest(&manifest, &layout.root)?;
            match command {
                AdlCommand::Graph => {
                    println!("Dependency Graph:");
                    println!("==================");
                    for line in resolved.graph.to_graph_lines() {
                        println!("{}", line);
                    }
                }
                AdlCommand::Tree => {
                    println!("Dependency Tree:");
                    println!("================");
                    for line in resolved.graph.tree_lines() {
                        println!("{}", line);
                    }
                }
                AdlCommand::Why => {
                    let query = args.first().cloned().unwrap_or_else(|| {
                        manifest
                            .project_name()
                            .unwrap_or_else(|| "unknown".to_string())
                    });
                    let matches = resolved.graph.find_by_name(&query);
                    if matches.is_empty() {
                        println!(
                            "No dependency named '{}' is present in the current graph.",
                            query
                        );
                    } else {
                        println!("Why is '{}' present?", query);
                        println!("========================");
                        for node in matches {
                            println!("\nPackage: {} {}", node.name, node.version);
                            println!("Source:  {}", node.source.kind);
                            println!("Location: {}", node.source.location);
                            println!("Checksum: {}", node.checksum);
                        }
                    }
                }
                AdlCommand::List => {
                    let project = manifest
                        .project_name()
                        .unwrap_or_else(|| "unknown".to_string());
                    println!("Packages in {}:", project);
                    println!("================");
                    println!("(root) {}", project);
                    let mut nodes: Vec<_> = resolved.graph.nodes.values().collect();
                    nodes.sort_by(|a, b| a.name.cmp(&b.name));
                    for node in nodes {
                        println!("{} {}", node.name, node.version);
                    }
                }
                AdlCommand::Info => {
                    println!("Project Information:");
                    println!("====================");
                    println!(
                        "Name:         {}",
                        manifest
                            .project_name()
                            .unwrap_or_else(|| "unknown".to_string())
                    );
                    println!(
                        "Version:      {}",
                        manifest
                            .project_version()
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "unknown".to_string())
                    );
                    println!("Dependencies: {}", resolved.graph.nodes.len());
                    println!("Root path:    {}", layout.root.display());
                    println!("Manifest:     {}", layout.manifest_path().display());
                    println!("Lockfile:     {}", layout.lockfile_path().display());
                    println!("Bin dir:      {}", layout.bin_dir().display());
                    let builds = layout.locate_binary_builds(None);
                    println!("Builds:       {} binary file(s)", builds.len());
                }
                _ => {}
            }
        }
        AdlCommand::Env => {
            println!("ADL_ROOT={}", layout.root.display());
            println!("ADL_DIR={}", layout.adl_dir().display());
            println!("ADL_BIN_DIR={}", layout.bin_dir().display());
            println!("ADL_TARGET_DIR={}", layout.target_dir().display());
            println!("ADL_TARGET_BIN_DIR={}", layout.target_bin_dir().display());
            println!("ADL_PACKAGES_DIR={}", layout.packages_dir().display());
            println!("ADL_CACHE_DIR={}", layout.cache_dir().display());
            println!("ADL_ARTIFACTS_DIR={}", layout.artifacts_dir().display());
        }
        AdlCommand::Script => {
            let script_name = args
                .first()
                .ok_or_else(|| "script requires a script name".to_string())?;
            run_script(layout, script_name)?;
        }
        AdlCommand::Version => {
            println!("adl {}", env!("CARGO_PKG_VERSION"));
        }
        AdlCommand::Config
        | AdlCommand::Target
        | AdlCommand::Backend
        | AdlCommand::Generate
        | AdlCommand::Package
        | AdlCommand::Verify
        | AdlCommand::Search
        | AdlCommand::Fmt
        | AdlCommand::Lint
        | AdlCommand::Doc
        | AdlCommand::Benchmark
        | AdlCommand::Profile
        | AdlCommand::Vendor => {
            println!("{}: {}", command_name(command), args.join(" "));
        }
        _ => {}
    }
    Ok(())
}

fn parse_scaffold_args(
    args: &[String],
    command: AdlCommand,
) -> Result<(ProjectTemplate, String, PathBuf), String> {
    let mut template = ProjectTemplate::App;
    let mut name_opt: Option<String> = None;
    let mut dir_opt: Option<PathBuf> = None;

    for value in args {
        match value.as_str() {
            "--app" | "app" => template = ProjectTemplate::App,
            "--lib" | "lib" | "library" => template = ProjectTemplate::Library,
            "--workspace" | "workspace" => template = ProjectTemplate::Workspace,
            "--plugin" | "plugin" => template = ProjectTemplate::Plugin,
            "--package" | "package" => template = ProjectTemplate::Package,
            other if name_opt.is_none() && !other.starts_with('-') => {
                name_opt = Some(other.to_string())
            }
            other if !other.starts_with('-') => dir_opt = Some(PathBuf::from(other)),
            _ => {}
        }
    }

    let cwd = env::current_dir().map_err(|error| error.to_string())?;

    match command {
        AdlCommand::New => {
            let name = name_opt.ok_or_else(|| {
                "adl new requires a project name.\n\nUsage: adl new <project_name> [--lib]"
                    .to_string()
            })?;
            let target_dir = dir_opt.unwrap_or_else(|| cwd.join(&name));
            Ok((template, name, target_dir))
        }
        AdlCommand::Init => {
            let target_dir = dir_opt
                .or_else(|| name_opt.as_ref().map(PathBuf::from))
                .unwrap_or_else(|| cwd.clone());
            let name = target_dir
                .file_name()
                .and_then(|n| n.to_str())
                .filter(|s| !s.is_empty() && *s != ".")
                .map(String::from)
                .or(name_opt)
                .unwrap_or_else(|| "my_app".to_string());
            Ok((template, name, target_dir))
        }
        _ => {
            let name = name_opt.unwrap_or_else(|| "my_app".to_string());
            let target_dir = dir_opt.unwrap_or(cwd);
            Ok((template, name, target_dir))
        }
    }
}

fn parse_dependency_args(args: &[String]) -> Result<(String, String), String> {
    let name = args
        .first()
        .cloned()
        .ok_or_else(|| "add requires a dependency name".to_string())?;
    let requirement = args.get(1).cloned().unwrap_or_else(|| "*".to_string());
    Ok((name, requirement))
}

fn parse_command(command: &str) -> Result<AdlCommand, String> {
    Ok(match command {
        "new" => AdlCommand::New,
        "init" => AdlCommand::Init,
        "build" => AdlCommand::Build,
        "run" => AdlCommand::Run,
        "test" => AdlCommand::Test,
        "ai" => AdlCommand::Ai,
        "fmt" | "format" => AdlCommand::Fmt,
        "lint" => AdlCommand::Lint,
        "check" => AdlCommand::Check,
        "doc" | "docs" => AdlCommand::Doc,
        "clean" => AdlCommand::Clean,
        "add" => AdlCommand::Add,
        "remove" | "rm" => AdlCommand::Remove,
        "update" => AdlCommand::Update,
        "upgrade" => AdlCommand::Upgrade,
        "install" => AdlCommand::Install,
        "restore" => AdlCommand::Restore,
        "resolve" => AdlCommand::Resolve,
        "search" => AdlCommand::Search,
        "info" => AdlCommand::Info,
        "list" => AdlCommand::List,
        "login" => AdlCommand::Login,
        "logout" => AdlCommand::Logout,
        "publish" => AdlCommand::Publish,
        "unpublish" => AdlCommand::Unpublish,
        "cache" => AdlCommand::Cache,
        "doctor" => AdlCommand::Doctor,
        "repair" => AdlCommand::Repair,
        "workspace" => AdlCommand::Workspace,
        "graph" => AdlCommand::Graph,
        "tree" => AdlCommand::Tree,
        "why" => AdlCommand::Why,
        "vendor" => AdlCommand::Vendor,
        "benchmark" | "bench" => AdlCommand::Benchmark,
        "profile" => AdlCommand::Profile,
        "version" | "--version" | "-V" => AdlCommand::Version,
        "config" => AdlCommand::Config,
        "env" => AdlCommand::Env,
        "target" => AdlCommand::Target,
        "backend" => AdlCommand::Backend,
        "script" => AdlCommand::Script,
        "generate" => AdlCommand::Generate,
        "package" => AdlCommand::Package,
        "verify" => AdlCommand::Verify,
        "locate-bin" | "locate-bins" | "locate" | "bin" | "bins" | "which" | "which-bin" => {
            AdlCommand::LocateBin
        }
        "help" => AdlCommand::Help,
        other => return Err(format!("unknown adl command: {other}")),
    })
}

fn print_help() {
    let colors =
        std::env::var("NO_COLOR").is_err() && std::env::var("TERM").map_or(true, |t| t != "dumb");

    let c_reset = if colors { "\x1b[0m" } else { "" };
    let c_bold = if colors { "\x1b[1m" } else { "" };
    let c_cyan = if colors { "\x1b[1;36m" } else { "" };
    let c_green = if colors { "\x1b[1;32m" } else { "" };
    let c_yellow = if colors { "\x1b[33m" } else { "" };
    let c_magenta = if colors { "\x1b[1;35m" } else { "" };
    let c_gray = if colors { "\x1b[90m" } else { "" };
    let c_white = if colors { "\x1b[1;37m" } else { "" };

    println!("{c_bold}{c_cyan}📦 ADL — AdeshLang Package Manager & Toolchain (v0.3.0){c_reset}");
    println!(
        "{c_gray}Official package manager, dependency resolver, and build orchestrator for AdeshLang.{c_reset}\n"
    );

    println!("{c_bold}{c_cyan}USAGE:{c_reset}");
    println!(
        "  {c_green}adl{c_reset} {c_yellow}<COMMAND>{c_reset} [{c_yellow}FLAGS{c_reset}] [{c_yellow}OPTIONS{c_reset}]\n"
    );

    println!("{c_bold}{c_cyan}COMMANDS:{c_reset}");

    let categories: &[(&str, &[(&str, &str)])] = &[
        (
            "Project & Scaffold",
            &[
                (
                    "new <name>",
                    "Create a new AdeshLang project with boilerplate layout",
                ),
                (
                    "init [dir]",
                    "Initialize an AdeshLang project in existing directory",
                ),
            ],
        ),
        (
            "Build, Run & Testing",
            &[
                (
                    "build",
                    "Compile the project into target binaries or libraries",
                ),
                (
                    "run [-- <args>]",
                    "Build and execute the main project script",
                ),
                (
                    "locate-bin [name]",
                    "Locate binary builds (.exe, libs) or display bin directory",
                ),
                ("test", "Execute project test suite and embedded assertions"),
                (
                    "check",
                    "Run type checker and memory-safety analysis without building",
                ),
                ("clean", "Remove target build artifacts and cache directory"),
            ],
        ),
        (
            "Dependency Management",
            &[
                (
                    "add <pkg>",
                    "Add a new package dependency to adesh.adl manifest",
                ),
                (
                    "remove <pkg>",
                    "Remove a package dependency from project manifest",
                ),
                (
                    "install",
                    "Resolve and download dependencies into adl_modules/",
                ),
                ("restore", "Restore dependencies matching lockfile state"),
                (
                    "update",
                    "Update package dependencies to latest compatible versions",
                ),
                (
                    "upgrade",
                    "Upgrade project manifest dependencies to newest versions",
                ),
                ("resolve", "Solve dependency graph and write adesh.lock.adl"),
            ],
        ),
        (
            "Registry & Package Inspection",
            &[
                (
                    "search <query>",
                    "Search global registry for published packages",
                ),
                (
                    "info <pkg>",
                    "Display metadata, dependencies, and specs for a package",
                ),
                ("list", "List installed dependencies and resolved versions"),
                ("tree", "Display hierarchical dependency tree structure"),
                (
                    "graph",
                    "Generate module and dependency visualization graph",
                ),
                (
                    "why <pkg>",
                    "Explain why a dependency is present in project graph",
                ),
            ],
        ),
        (
            "Formatting, Linting & Docs",
            &[
                (
                    "fmt",
                    "Format source code according to official style guidelines",
                ),
                ("lint", "Run static analyzer to identify warnings and bugs"),
                ("doc", "Generate HTML documentation for project API"),
            ],
        ),
        (
            "Registry Publishing & Auth",
            &[
                ("login", "Authenticate with global package registry"),
                ("logout", "Clear stored registry authentication tokens"),
                ("publish", "Publish package version to registry"),
                (
                    "unpublish <pkg>",
                    "Remove a published package version from registry",
                ),
            ],
        ),
        (
            "Toolchain & Diagnostics",
            &[
                ("doctor", "Diagnose environment health, linkers, and paths"),
                (
                    "bins",
                    "List all compiled binary builds and available binary targets",
                ),
                (
                    "vendor",
                    "Vendor all external module dependencies into vendor/",
                ),
                ("benchmark", "Execute performance benchmark suite"),
                ("profile", "Profile runtime memory and CPU performance"),
                ("cache", "Inspect or purge global package cache"),
                ("workspace", "Manage multi-package workspace manifest"),
                (
                    "script <name>",
                    "Execute custom scripts defined in adesh.adl",
                ),
                ("version", "Display ADL package manager version info"),
                ("config", "Inspect or update user configuration settings"),
                (
                    "env",
                    "Display environment paths and active toolchain config",
                ),
                ("target", "Display or configure active compilation targets"),
                ("backend", "Select or configure execution backend"),
                ("generate", "Generate code templates or bindings"),
                ("package", "Package project into distribution archive"),
                ("verify", "Verify integrity of installed package modules"),
            ],
        ),
    ];

    for (cat_title, cmds) in categories {
        println!("  {c_magenta}{cat_title}{c_reset}");
        for (cmd, desc) in *cmds {
            println!(
                "    {c_green}{:<22}{c_reset} {c_gray}•{c_reset} {desc}",
                cmd
            );
        }
        println!();
    }

    println!("{c_cyan}GLOBAL FLAGS:{c_reset}");
    println!("  {c_yellow}-h, --help{c_reset}       Print help information and command specs");
    println!("  {c_yellow}-v, --verbose{c_reset}    Enable detailed verbose debug logging");
    println!("  {c_yellow}--release{c_reset}        Execute build/run in optimized release mode");
    println!(
        "  {c_yellow}--debug{c_reset}          Enable debug symbols and intermediate IR dumps"
    );
    println!(
        "  {c_yellow}--offline{c_reset}        Run operations without contacting remote network"
    );
    println!(
        "  {c_yellow}--force{c_reset}          Force operation bypassing non-critical warnings"
    );
    println!(
        "  {c_yellow}--dry-run{c_reset}        Preview operation without modifying disk or network"
    );
    println!(
        "  {c_yellow}--locate-bin{c_reset}     Locate built binaries in project bin/target dirs\n"
    );

    println!("{c_cyan}EXAMPLE USAGE:{c_reset}");
    println!("  {c_gray}# Create a new binary application project{c_reset}");
    println!("  {c_white}adl new my_app{c_reset}\n");
    println!("  {c_gray}# Locate built project binaries or bin directory{c_reset}");
    println!("  {c_white}adl locate-bin{c_reset}\n");
    println!("  {c_gray}# Locate specific executable build{c_reset}");
    println!("  {c_white}adl locate-bin my_app{c_reset}\n");
    println!("  {c_gray}# Add HTTP package dependency to manifest{c_reset}");
    println!("  {c_white}adl add HTTP 1.2.0{c_reset}\n");
    println!("  {c_gray}# Install all dependencies into adl_modules/{c_reset}");
    println!("  {c_white}adl install{c_reset}\n");
    println!("  {c_gray}# Build and run project passing arguments after '--'{c_reset}");
    println!("  {c_white}adl run -- --port=8080{c_reset}\n");
    println!("  {c_gray}# Check system environment and toolchain health{c_reset}");
    println!("  {c_white}adl doctor{c_reset}\n");

    println!("{c_gray}For more details, visit: https://adeshlang.org/tools/cli{c_reset}");
}

fn command_name(command: AdlCommand) -> &'static str {
    match command {
        AdlCommand::New => "new",
        AdlCommand::Init => "init",
        AdlCommand::Build => "build",
        AdlCommand::Run => "run",
        AdlCommand::Test => "test",
        AdlCommand::Ai => "ai",
        AdlCommand::Fmt => "fmt",
        AdlCommand::Lint => "lint",
        AdlCommand::Check => "check",
        AdlCommand::Doc => "doc",
        AdlCommand::Clean => "clean",
        AdlCommand::Add => "add",
        AdlCommand::Remove => "remove",
        AdlCommand::Update => "update",
        AdlCommand::Upgrade => "upgrade",
        AdlCommand::Install => "install",
        AdlCommand::Restore => "restore",
        AdlCommand::Resolve => "resolve",
        AdlCommand::Search => "search",
        AdlCommand::Info => "info",
        AdlCommand::List => "list",
        AdlCommand::Login => "login",
        AdlCommand::Logout => "logout",
        AdlCommand::Publish => "publish",
        AdlCommand::Unpublish => "unpublish",
        AdlCommand::Cache => "cache",
        AdlCommand::Doctor => "doctor",
        AdlCommand::Repair => "repair",
        AdlCommand::Workspace => "workspace",
        AdlCommand::Graph => "graph",
        AdlCommand::Tree => "tree",
        AdlCommand::Why => "why",
        AdlCommand::Vendor => "vendor",
        AdlCommand::Benchmark => "benchmark",
        AdlCommand::Profile => "profile",
        AdlCommand::Version => "version",
        AdlCommand::Config => "config",
        AdlCommand::Env => "env",
        AdlCommand::Target => "target",
        AdlCommand::Backend => "backend",
        AdlCommand::Script => "script",
        AdlCommand::Generate => "generate",
        AdlCommand::Package => "package",
        AdlCommand::Verify => "verify",
        AdlCommand::LocateBin => "locate-bin",
        AdlCommand::Help => "help",
    }
}

fn run_script(layout: &ProjectLayout, script_name: &str) -> Result<(), String> {
    let manifest = layout.load_manifest()?;
    let scripts_sec = manifest
        .scripts()
        .ok_or_else(|| "No 'scripts' section found in manifest".to_string())?;

    let cmd_value = scripts_sec
        .field(script_name)
        .ok_or_else(|| format!("Script '{}' not found in manifest", script_name))?;
    let cmd_str = match cmd_value {
        crate::ecosystem::manifest::ManifestValue::String(s)
        | crate::ecosystem::manifest::ManifestValue::Identifier(s) => s.clone(),
        _ => return Err(format!("Script '{}' must be a string", script_name)),
    };

    println!("Running script '{}': {}", script_name, cmd_str);

    let status = if cfg!(target_os = "windows") {
        std::process::Command::new("powershell")
            .arg("-Command")
            .arg(&cmd_str)
            .status()
            .map_err(|e| format!("Failed to execute script: {}", e))?
    } else {
        std::process::Command::new("sh")
            .arg("-c")
            .arg(&cmd_str)
            .status()
            .map_err(|e| format!("Failed to execute script: {}", e))?
    };

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "Script '{}' failed with exit status: {:?}",
            script_name, status
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_locate_bin_commands() {
        assert_eq!(parse_command("locate-bin").unwrap(), AdlCommand::LocateBin);
        assert_eq!(parse_command("locate-bins").unwrap(), AdlCommand::LocateBin);
        assert_eq!(parse_command("locate").unwrap(), AdlCommand::LocateBin);
        assert_eq!(parse_command("bin").unwrap(), AdlCommand::LocateBin);
        assert_eq!(parse_command("bins").unwrap(), AdlCommand::LocateBin);
        assert_eq!(parse_command("which").unwrap(), AdlCommand::LocateBin);
    }

    #[test]
    fn test_parse_locate_bin_flag() {
        let args = vec![
            "adl".to_string(),
            "--locate-bin".to_string(),
            "my_app".to_string(),
        ];
        let parsed = parse_args(args).unwrap();
        assert_eq!(parsed.command, AdlCommand::LocateBin);
        assert_eq!(parsed.args, vec!["my_app".to_string()]);
    }
}
