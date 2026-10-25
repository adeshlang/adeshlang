//! Distributed CLI help system.
//!
//! `adesh --help` prints a compact command index, while `adesh <command> --help`
//! prints a detailed page for a single command. Unknown commands get
//! "did you mean" suggestions and related usage instead of the full help dump.

use std::fmt::Write as _;

/// Compiler/CLI version reported by help and `--version`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const BOLD: &str = "\x1b[1m";
const CYAN: &str = "\x1b[36m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const BLUE: &str = "\x1b[34m";
const DIM: &str = "\x1b[2m";
const RED_BOLD: &str = "\x1b[1;31m";
const RESET: &str = "\x1b[0m";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Group {
    BuildRun,
    Compile,
    Project,
    Analysis,
    Observe,
    Legacy,
    Reference,
}

impl Group {
    const ORDER: [Group; 7] = [
        Group::BuildRun,
        Group::Compile,
        Group::Project,
        Group::Analysis,
        Group::Observe,
        Group::Legacy,
        Group::Reference,
    ];

    fn title(self) -> &'static str {
        match self {
            Group::BuildRun => "BUILD & RUN",
            Group::Compile => "COMPILE & EMIT",
            Group::Project => "PROJECT & TOOLCHAIN",
            Group::Analysis => "ANALYSIS & INSPECTION",
            Group::Observe => "OBSERVABILITY & TOOLS",
            Group::Legacy => "LEGACY COMMANDS",
            Group::Reference => "REFERENCE",
        }
    }
}

/// A user-facing command and its one-line summary.
pub struct CommandSpec {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub summary: &'static str,
    pub group: Group,
}

/// Every command accepted by the `adesh` binary.
pub const COMMANDS: &[CommandSpec] = &[
    // Build & run
    CommandSpec {
        name: "run",
        aliases: &[],
        summary: "Run a program (default: interpreter)",
        group: Group::BuildRun,
    },
    CommandSpec {
        name: "test",
        aliases: &[],
        summary: "Run unit and integration tests",
        group: Group::BuildRun,
    },
    CommandSpec {
        name: "build",
        aliases: &[],
        summary: "Build a native executable or ADOB object",
        group: Group::BuildRun,
    },
    CommandSpec {
        name: "check",
        aliases: &[],
        summary: "Type-check and validate without executing",
        group: Group::BuildRun,
    },
    CommandSpec {
        name: "bench",
        aliases: &[],
        summary: "Benchmark program execution",
        group: Group::BuildRun,
    },
    CommandSpec {
        name: "lint",
        aliases: &[],
        summary: "Run semantic, type, and safety checks",
        group: Group::BuildRun,
    },
    CommandSpec {
        name: "format",
        aliases: &["fmt"],
        summary: "Format source code",
        group: Group::BuildRun,
    },
    CommandSpec {
        name: "repl",
        aliases: &[],
        summary: "Start the interactive REPL",
        group: Group::BuildRun,
    },
    // Compile & emit
    CommandSpec {
        name: "compile",
        aliases: &[],
        summary: "Compile to bytecode",
        group: Group::Compile,
    },
    CommandSpec {
        name: "compile-wasm",
        aliases: &[],
        summary: "Compile to WebAssembly (+ .js loader)",
        group: Group::Compile,
    },
    CommandSpec {
        name: "compile-wasm-js",
        aliases: &[],
        summary: "Compile WASM and JS into a directory",
        group: Group::Compile,
    },
    CommandSpec {
        name: "compile-native",
        aliases: &[],
        summary: "Compile to a native object or executable",
        group: Group::Compile,
    },
    CommandSpec {
        name: "compile-native-rust",
        aliases: &[],
        summary: "Emit native Rust source",
        group: Group::Compile,
    },
    CommandSpec {
        name: "compile-aot",
        aliases: &[],
        summary: "Compile a native executable (power-user AOT)",
        group: Group::Compile,
    },
    CommandSpec {
        name: "disassemble",
        aliases: &[],
        summary: "Disassemble a bytecode file",
        group: Group::Compile,
    },
    CommandSpec {
        name: "docs",
        aliases: &["doc"],
        summary: "Generate documentation",
        group: Group::Compile,
    },
    CommandSpec {
        name: "link",
        aliases: &[],
        summary: "Link objects with adeshlink",
        group: Group::Compile,
    },
    CommandSpec {
        name: "pgo",
        aliases: &[],
        summary: "Merge PGO profile data",
        group: Group::Compile,
    },
    // Project & toolchain
    CommandSpec {
        name: "init",
        aliases: &[],
        summary: "Initialize a new project",
        group: Group::Project,
    },
    CommandSpec {
        name: "new",
        aliases: &[],
        summary: "Create a new project directory",
        group: Group::Project,
    },
    CommandSpec {
        name: "clean",
        aliases: &[],
        summary: "Remove build artifacts",
        group: Group::Project,
    },
    CommandSpec {
        name: "target",
        aliases: &[],
        summary: "Show cross-compilation targets",
        group: Group::Project,
    },
    CommandSpec {
        name: "toolchain",
        aliases: &[],
        summary: "Manage native and external toolchains",
        group: Group::Project,
    },
    CommandSpec {
        name: "pkg",
        aliases: &[],
        summary: "Manage packages",
        group: Group::Project,
    },
    CommandSpec {
        name: "update",
        aliases: &[],
        summary: "Check for and install updates",
        group: Group::Project,
    },
    CommandSpec {
        name: "rollback",
        aliases: &[],
        summary: "Roll back to the previous version",
        group: Group::Project,
    },
    CommandSpec {
        name: "env",
        aliases: &[],
        summary: "Show environment paths (--json, --shell)",
        group: Group::Project,
    },
    CommandSpec {
        name: "repair",
        aliases: &[],
        summary: "Repair toolchain and environment settings",
        group: Group::Project,
    },
    CommandSpec {
        name: "doctor",
        aliases: &[],
        summary: "Inspect system and toolchain health",
        group: Group::Project,
    },
    CommandSpec {
        name: "gpu-check",
        aliases: &[],
        summary: "Probe GPU devices and the native toolchain",
        group: Group::Project,
    },
    // Analysis & inspection
    CommandSpec {
        name: "adob",
        aliases: &[],
        summary: "Inspect and validate ADOB object binaries",
        group: Group::Analysis,
    },
    CommandSpec {
        name: "inspect",
        aliases: &["objdump"],
        summary: "Objdump-style object inspection",
        group: Group::Analysis,
    },
    CommandSpec {
        name: "crypto",
        aliases: &[],
        summary: "Hashing, secure random bytes, certificate inspection",
        group: Group::Analysis,
    },
    CommandSpec {
        name: "size",
        aliases: &[],
        summary: "Binary size analysis (.adob/.obj)",
        group: Group::Analysis,
    },
    CommandSpec {
        name: "symbols",
        aliases: &[],
        summary: "List symbols in a binary",
        group: Group::Analysis,
    },
    CommandSpec {
        name: "sections",
        aliases: &[],
        summary: "List sections in a binary",
        group: Group::Analysis,
    },
    CommandSpec {
        name: "relocations",
        aliases: &[],
        summary: "List relocations in a binary",
        group: Group::Analysis,
    },
    CommandSpec {
        name: "map",
        aliases: &[],
        summary: "Show the link map of a binary",
        group: Group::Analysis,
    },
    CommandSpec {
        name: "self-test",
        aliases: &[],
        summary: "Run the compiler ecosystem self-test",
        group: Group::Analysis,
    },
    CommandSpec {
        name: "bugreport",
        aliases: &[],
        summary: "Write an adesh_bugreport.json bundle",
        group: Group::Analysis,
    },
    CommandSpec {
        name: "server",
        aliases: &[],
        summary: "Start the persistent compiler daemon",
        group: Group::Analysis,
    },
    // Observability & tools
    CommandSpec {
        name: "monitor",
        aliases: &["aro", "observatory"],
        summary: "Runtime Observatory TUI and telemetry CLI",
        group: Group::Observe,
    },
    CommandSpec {
        name: "editor",
        aliases: &[],
        summary: "Launch the TUI code editor",
        group: Group::Observe,
    },
    CommandSpec {
        name: "ai",
        aliases: &[],
        summary: "AI assistant (explain, fix, generate, chat)",
        group: Group::Observe,
    },
    // Legacy
    CommandSpec {
        name: "run-interpret",
        aliases: &[],
        summary: "Always run with the interpreter",
        group: Group::Legacy,
    },
    CommandSpec {
        name: "run-jit",
        aliases: &[],
        summary: "Always run with the JIT",
        group: Group::Legacy,
    },
    CommandSpec {
        name: "run-bc",
        aliases: &[],
        summary: "Run a compiled bytecode file",
        group: Group::Legacy,
    },
    // Reference
    CommandSpec {
        name: "language",
        aliases: &[],
        summary: "Language, type system, memory model, backends",
        group: Group::Reference,
    },
    CommandSpec {
        name: "help",
        aliases: &[],
        summary: "Show help for a command or topic",
        group: Group::Reference,
    },
    CommandSpec {
        name: "version",
        aliases: &[],
        summary: "Show version information",
        group: Group::Reference,
    },
];

/// Resolve a command or alias to its canonical name.
pub fn canonical_command(name: &str) -> Option<&'static str> {
    COMMANDS.iter().find_map(|cmd| {
        if cmd.name == name || cmd.aliases.contains(&name) {
            Some(cmd.name)
        } else {
            None
        }
    })
}

/// True when `name` is a command or alias accepted by the CLI.
pub fn is_known_command(name: &str) -> bool {
    canonical_command(name).is_some()
}

/// Levenshtein edit distance, used for "did you mean" suggestions.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];

    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

fn max_distance(input: &str) -> usize {
    match input.chars().count() {
        0..=3 => 1,
        4..=6 => 2,
        _ => 3,
    }
}

/// Canonical command names closest to an unknown input.
pub fn suggest_commands(input: &str) -> Vec<&'static str> {
    let needle = input.to_ascii_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }

    let limit = max_distance(&needle);
    let mut scored: Vec<(&'static str, usize)> = Vec::new();
    for cmd in COMMANDS {
        for name in std::iter::once(cmd.name).chain(cmd.aliases.iter().copied()) {
            let distance = if name.starts_with(&needle) || needle.starts_with(name) {
                0
            } else {
                levenshtein(&needle, name)
            };
            if distance <= limit {
                scored.push((cmd.name, distance));
            }
        }
    }

    scored.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(b.0)));
    scored.dedup_by(|a, b| a.0 == b.0);
    scored.into_iter().take(3).map(|(name, _)| name).collect()
}

fn related_commands(input: &str) -> Vec<&'static str> {
    let needle = input.to_ascii_lowercase();
    if needle.chars().count() < 2 {
        return Vec::new();
    }
    COMMANDS
        .iter()
        .filter(|cmd| {
            cmd.name.contains(&needle) || cmd.aliases.iter().any(|alias| alias.contains(&needle))
        })
        .map(|cmd| cmd.name)
        .take(6)
        .collect()
}

/// Error text for an unknown command or option, including related usage hints.
pub fn unknown_command_message(input: &str) -> String {
    let mut out = String::new();
    let is_option = input.starts_with('-');

    if is_option {
        let _ = writeln!(out, "{RED_BOLD}error{RESET}: unknown option `{input}`");
    } else {
        let _ = writeln!(out, "{RED_BOLD}error{RESET}: unknown command `{input}`");
    }
    out.push('\n');

    let mut sections: Vec<String> = Vec::new();

    if !is_option
        && (input.ends_with(".adesh")
            || input.ends_with(".adl")
            || std::path::Path::new(input).is_file())
    {
        sections.push(format!(
            "  {YELLOW}tip{RESET}: to execute a source file, run `adesh run {input}`"
        ));
    }

    let suggestions = suggest_commands(input);
    if !suggestions.is_empty() {
        let mut section = format!("  {YELLOW}tip{RESET}: did you mean one of these?");
        for suggestion in &suggestions {
            let _ = write!(section, "\n      adesh {suggestion}");
        }
        sections.push(section);
    }

    let related: Vec<_> = related_commands(input)
        .into_iter()
        .filter(|name| !suggestions.contains(name))
        .collect();
    if !related.is_empty() {
        let mut section = "  related commands:".to_string();
        for name in related {
            let _ = write!(section, "\n      adesh {name}");
        }
        sections.push(section);
    }

    if is_option {
        sections.push(format!(
            "  {YELLOW}tip{RESET}: options are command-specific; try `adesh run --help`, `adesh build --help`, ..."
        ));
    } else {
        sections.push(format!(
            "  {YELLOW}tip{RESET}: for the full command list, run `adesh --help`"
        ));
    }
    sections.push(format!(
        "  {YELLOW}tip{RESET}: detailed help: `adesh <command> --help` or `adesh help <command>`"
    ));

    out.push_str(&sections.join("\n\n"));
    let _ = writeln!(
        out,
        "\n\n{BOLD}USAGE{RESET}:\n    adesh <COMMAND> [OPTIONS] <FILE> [-- <ARGS>...]"
    );
    out
}

/// Compact top-level help printed by `adesh`, `adesh --help`, and `adesh help`.
pub fn help_message() -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{BOLD}{CYAN}Adesh{RESET} {DIM}v{VERSION}{RESET}\n\
         {DIM}A statically-typed, multi-backend programming language for modern systems software{RESET}\n"
    );
    out.push_str(
        "Adesh combines strong type inference, compile-time memory safety,\n\
         deterministic ownership (no garbage collector), and zero-cost abstractions\n\
         with native AOT and JIT compilation. The same source program runs on the\n\
         interpreter, JIT, bytecode, and WebAssembly backends, with C/C++ FFI,\n\
         cross-compilation, and an integrated build, test, and profiling toolchain.\n",
    );

    out.push_str(&format!(
        "\n{BOLD}USAGE{RESET}\n\
         \x20   adesh <COMMAND> [OPTIONS] <FILE> [-- <ARGS>...]\n\
         \x20   adesh run -e \"<code>\"\n\
         \x20   adesh <COMMAND> --help        Detailed help for one command\n\
         \x20   adesh help <TOPIC>            Browse a help topic (e.g. language)\n"
    ));

    out.push_str(&format!("\n{BOLD}COMMANDS{RESET}\n"));
    for group in Group::ORDER {
        let _ = writeln!(out, "\n  {BOLD}{}{RESET}", group.title());
        for cmd in COMMANDS.iter().filter(|cmd| cmd.group == group) {
            let name = if cmd.aliases.is_empty() {
                cmd.name.to_string()
            } else {
                format!("{} ({})", cmd.name, cmd.aliases.join(", "))
            };
            let _ = writeln!(out, "    {name:<30}{}", cmd.summary);
        }
    }

    out.push_str(&format!(
        "\n{BOLD}GLOBAL OPTIONS{RESET}\n\
         \x20   -h, --help                     Show this help (or command help)\n\
         \x20   -v, --version                  Show version information\n\
         \x20   -q, --quiet                    Suppress progress and diagnostics\n\
         \x20   --verbose                      Enable verbose output\n\
         \x20   --no-color                     Disable ANSI colors\n\
         \x20   --json                         Machine-readable output (where supported)\n\
         \x20   --explain [CODE]               Explain a diagnostic code, or list all\n\
         \x20   --debug-backtrace              Show a backtrace on internal errors\n\
         \x20   --toolchain <bundled|system>  Select the native toolchain\n\
         \x20   --system-toolchain             Shortcut for --toolchain system\n\
         \x20   --bundled-toolchain            Shortcut for --toolchain bundled\n"
    ));

    out.push_str(&format!(
        "\n{BOLD}EXAMPLES{RESET}\n\
         \x20   {GREEN}adesh run{RESET} program.adesh\n\
         \x20   {GREEN}adesh run -e{RESET} \"print(42)\"\n\
         \x20   {GREEN}adesh test{RESET} tests/main.adesh\n\
         \x20   {GREEN}adesh build --fast{RESET} program.adesh -o app\n\
         \x20   {GREEN}adesh monitor list{RESET} --group\n\
         \x20   {GREEN}adesh help test{RESET}\n"
    ));

    out.push_str(&format!(
        "\n{BOLD}LEARN MORE{RESET}\n\
         \x20   adesh <COMMAND> --help    Options and examples for one command\n\
         \x20   adesh help language       Type system, memory model, backends, FFI\n\
         \x20   adesh --explain list      List every diagnostic code\n\
         \x20   {DIM}Docs: https://adeshlang.org/docs{RESET}\n\
         \x20   {DIM}https://github.com/adeshlang/adeshlang{RESET}\n"
    ));

    out
}

/// Detailed help page for a command or alias.
///
/// `subtopic` is accepted for commands with subcommands (for example
/// `adesh toolchain install --help`); it currently refines nothing because each
/// command page documents all of its subcommands.
pub fn help_for_command(command: &str, _subtopic: Option<&str>) -> Option<String> {
    let canonical = canonical_command(command)?;
    let text = match canonical {
        "run" | "run-interpret" | "run-jit" | "run-bc" => run_help(),
        "test" => test_help(),
        "build" => build_help(),
        "check" => check_help(),
        "bench" => bench_help(),
        "lint" => lint_help(),
        "format" => format_help(),
        "repl" => repl_help(),
        "compile" => compile_help(),
        "compile-wasm" => compile_wasm_help(),
        "compile-wasm-js" => compile_wasm_js_help(),
        "compile-native" => compile_native_help(),
        "compile-native-rust" => compile_native_rust_help(),
        "compile-aot" => compile_aot_help(),
        "disassemble" => disassemble_help(),
        "docs" => docs_help(),
        "link" => link_help(),
        "pgo" => pgo_help(),
        "init" | "new" => init_help(),
        "clean" => clean_help(),
        "target" => target_help(),
        "toolchain" => toolchain_help(),
        "pkg" => pkg_help(),
        "update" => update_help(),
        "rollback" => rollback_help(),
        "env" => env_help(),
        "repair" => repair_help(),
        "doctor" => doctor_help(),
        "gpu-check" => gpu_check_help(),
        "adob" => adob_help(),
        "inspect" => inspect_help(),
        "crypto" => crypto_help(),
        "size" | "symbols" | "sections" | "relocations" | "map" => binary_tool_help(canonical),
        "self-test" => self_test_help(),
        "bugreport" => bugreport_help(),
        "server" => server_help(),
        "monitor" => monitor_help(),
        "editor" => editor_help(),
        "ai" => ai_help(),
        "language" => language_help(),
        "help" => help_help(),
        "version" => version_message(),
        _ => return None,
    };
    Some(text)
}

/// Generate version message
pub fn version_message() -> String {
    format!(
        "Adesh v{VERSION} - a statically-typed, multi-backend programming language\n\
         Strong type inference, compile-time memory safety, deterministic ownership,\n\
         zero-cost abstractions, and native AOT/JIT compilation."
    )
}

fn page(name: &str, tagline: &str, body: &str) -> String {
    format!(
        "{BOLD}{CYAN}Adesh {name}{RESET} {DIM}— {tagline}{RESET}\n\n{body}\n\
         {DIM}More: `adesh --help` · `adesh <command> --help` · `adesh help language`{RESET}\n\
         {DIM}Docs: https://adeshlang.org/docs{RESET}\n"
    )
}

fn run_help() -> String {
    let body = r#"USAGE:
    adesh run [OPTIONS] <FILE> [-- <ARGS>...]
    adesh run -e "<code>"                   Evaluate and run inline code
    adesh run --test <FILE> [FILTER]        (equivalent to `adesh test`)

ARGS:
    <FILE>          Program source (.adesh) or project directory
    <ARGS>...       Arguments forwarded to the program (after --)

EXECUTION BACKENDS:
    --interpreter, --interp      Standard interpreter (default, most compatible)
    --jit                        JIT compilation (LIR interpreter)
    --jit-native                 Native JIT: machine code at runtime (10-232x faster)
    --native-jit, --njit         Aliases for --jit-native
    --mixed, --hybrid            Interpret first, JIT hot functions
    --safe                       Safe mode: no JIT, full GC safety
    --bytecode, --bc, --vm       Bytecode VM (deprecated)
    --experimental-vm            Bytecode VM without the deprecation warning
    --adaptive, --adaptive-jit, --ajit   Adaptive JIT with speculative optimization
    --tiered, --tiered-jit, --tjit       Tiered JIT (T0 -> T1 -> T2)
    --aot, --aot-native, --compile       Ahead-of-time native backend
    --wasm                       WebAssembly backend (Wasmtime)
    --backend=<name>             Select a backend by name
    --gpu                        MLIR GPU backend (debug builds, compile-only)

PERFORMANCE (compute-heavy workloads):
    Interpreter:   1.0x (baseline)
    JIT (--jit):   0.8x (interpretation overhead)
    Native JIT:    10-232x faster (true native code execution)
    AOT compile:   similar to Native JIT (ahead-of-time compilation)

NATIVE JIT FEATURES:
    - Compiles LIR to native machine code at runtime using Cranelift
    - Executes via function pointers (zero interpretation overhead)
    - 71% instruction coverage (46/65 LIR instructions)
    - Full loop support (for, while, do-while)
    - Full string support (print with strings works)
    - Full memory management (malloc, free, pointers)
    - All arithmetic, bitwise, and comparison operations
    - Ownership and borrowing support (Arc operations)
    - Production-ready for most workloads

    Best for: loop-heavy algorithms, array processing, math-intensive code,
    string operations, memory-intensive workloads, performance-critical paths,
    and real-time systems (deterministic execution).

OPTIMIZATION:
    --opt O0..O3            Optimization level (default: O1)
    -O0, -O1, -O2, -O3      Shorthand optimization levels
    --opt-level <n>         Explicit optimization level
    --fast-recursion        Full recursion optimizations (TCO + memoization)
    --tco, --tail-call      Enable tail-call optimization
    --memo, --memoize       Enable memoization for pure functions
    --no-recursion-opt      Disable all recursion optimizations
    --recursion-opt=<mode>  Select recursion optimization mode

MEMORY (OWNERSHIP-BASED, NO GC):
    --embedded              Embedded mode: heap + ARC disabled; stack + arena only
    --stack-size <size>     Initial stack size ("64M", "128MB"; default 128MB,
                            auto-scales up to 1GB)
    --heap-size <size>      Heap size limit ("1G", "512MB"; default unlimited)
    --check-ownership       Enable compile-time ownership/borrow checking (default: on)
    --check-moves           Enable move semantics validation (default: on)
    --no-check-ownership    Disable the legacy ownership pass
    --no-ownership          Alias for --no-check-ownership
    --no-check-moves        Disable legacy move checking
    --no-moves              Alias for --no-check-moves

    Adesh uses deterministic ownership; there is no garbage
    collector. Memory is managed by ownership/move semantics, borrowing
    (&T, &mut T), explicit ARC via `share`, and region/arena allocation.

TYPE CHECKING & ANALYSIS:
    --type-check            Perform strict type checking (always on)
    --strict-types          Disallow implicit type conversions
    --show-inferred         Display inferred types during compilation

    Type and memory-safety checking always runs before execution, on every backend.

IR DEBUGGING:
    --dump-ast              Dump AST
    --dump-hir, --dump-ir   Dump HIR (high-level IR)
    --dump-mir              Dump MIR (ownership/borrow/lifetime annotations)
    --dump-vir              Dump VIR (SSA value IR)
    --dump-mlir             Dump MLIR (multi-level IR)
    --mlir-gpu              Emit GPU kernel stubs in MLIR output
    --dump-lir              Dump LIR (low-level SSA IR)
    --dump-bytecode         Dump bytecode instructions
    --dump-all              Dump all intermediate representations
    --dump-ast-write [file]     Write AST dump to file (auto-name if omitted)
    --dump-hir-write [file]     Write HIR dump to file
    --dump-ir-write [file]      Alias for --dump-hir-write
    --dump-vir-write [file]     Write VIR dump to file
    --dump-mlir-write [file]    Write MLIR dump to file
    --dump-lir-write [file]     Write LIR dump to file
    --dump-cfg              Dump the control-flow graph
    --dump-cfg-write [file]     Write the CFG to a file

    VIR: SSA-based, backend-neutral IR with basic blocks/phi nodes, explicit
    alloc/load/store, and explicit ARC clone/drop operations. It is consumed by
    the JIT, AOT, bytecode, and interpreter backends.
    MLIR: LLVM-dialect operations, MLIR function syntax, i64/f64 types, and
    compatibility with the LLVM optimization pipeline.

IO & OUTPUT:
    --buffered-stdout       Use buffered stdout for performance
    --interactive-input     Enable interactive input mode
    --no-color              Disable ANSI color codes
    --quiet, -q             Suppress progress and diagnostic output
    --verbose               Enable verbose output
    --debug                 Enable debug output
    --profile, --time       Print profiling/timing output
    --memory, --mem         Print memory usage statistics after execution
    --timeout <dur>         Kill the process if it has not finished (15s, 15000ms)
    --no-warnings, --disable-warnings, --allow-warnings
                            Disable compiler/runtime warnings
    --use-lir               Use the LIR path instead of VIR
    --import <header>       Import a C header for FFI
    --wasm-module <file>    Load a WASM module and expose its imports
    -L <dir>                Add a library search directory
    -l <lib>                Link/load a library

DIAGNOSTICS:
    --explain [CODE]        Explain a diagnostic code, or list all codes
    --debug-backtrace       Show a backtrace on internal errors

EXAMPLES:
    adesh run program.adesh
    adesh run --jit program.adesh
    adesh run --jit-native program.adesh
    adesh run --jit --opt O3 program.adesh
    adesh run --safe program.adesh
    adesh run --embedded program.adesh
    adesh run -e "print(42)"
    adesh run program.adesh --profile
    adesh run program.adesh --timeout 20s
    adesh run --jit --memory program.adesh
    adesh run program.adesh -- arg1 arg2
    adesh run --type-check program.adesh
    adesh run --show-inferred program.adesh
    adesh run --dump-hir program.adesh
    adesh run --dump-vir program.adesh
    adesh run --dump-mlir program.adesh
    adesh run --dump-hir-write ir.hir program.adesh
    adesh run --dump-vir-write ir.vir program.adesh
    adesh run --dump-mlir-write program.adesh
    adesh run --dump-lir-write program.adesh
    adesh run --dump-ast-write program.adesh
    adesh run --dump-cfg-write program.adesh
    adesh run --tco program.adesh
    adesh run --memo program.adesh
    adesh run --fast-recursion program.adesh

ENVIRONMENT:
    ADESH_RUN_TIMEOUT_MS=15000   Watchdog timeout for `run`
    ADESH_VERBOSE=1              Verbose compilation details
    ADESH_FAST_COMPILE=1         Default to fast compilation
    ADESH_BACKTRACE=1            Show backtraces on internal errors

LEGACY COMMANDS:
    adesh run-interpret <file>   Always use the interpreter
    adesh run-jit <file>         Always use the JIT
    adesh run-bc <file>          Run a compiled bytecode file"#;
    page("run", "execute a program", body)
}

fn test_help() -> String {
    let body = r#"USAGE:
    adesh test [FILE] [FILTER] [OPTIONS]
    adesh run --test <FILE> [FILTER] [OPTIONS]
    adesh test --runtimes=<list> [FILE]
    adesh test --backend-check [FILE]

DISCOVERY:
    With no FILE, `adesh test` auto-discovers tests/main.adesh, tests.adesh,
    main.test.adesh, or the first .adesh file under tests/. A directory may be
    passed as FILE.

TEST FLAGS:
    --test, --tests             Run every `test fn` in the file/module
    --test-name <name>          Filter tests by substring or prefix
    --exact                     Exact match instead of substring
    --skip <pattern>            Skip tests matching the pattern
    --skip=<pattern>            Same, inline form
    --list                      List discovered tests without running them
    --ignored                   Run only ignored tests
    --include-ignored           Run ignored and non-ignored tests
    --test-threads <N>, -j <N>  Number of concurrent test threads
    --serial, --sequential, --one-by-one   Run tests sequentially
    --parallel                  Full multi-threaded parallelism
    --runtimes=<list>           Comma-separated backend list
    --backend-check             Test on ALL known backends (full matrix)
    --fail-fast                 Stop at the first FAIL/PANIC/TIMEOUT
    --quiet, -q                 Suppress passing-test output
    --nocapture                 Print stdout from every test
    --tags <tag>                Filter tests by tag (@tag("fast"), ...)
    --include-tags=<tag,...>    Include only tests with any of these tags
    --format json               Machine-readable JSON report (CI)
    --format text               Human-readable output (default)
    --                          Pass remaining arguments to the test harness

BACKEND NAMES FOR --runtimes:
    interp      Standard interpreter (default, always available)
    jit         JIT (LIR compiler, fast startup)
    njit        Native JIT (Cranelift machine code, 10-232x faster)
    vm          Bytecode VM (--vm / --bytecode)
    mixed       Hybrid interpreter + JIT for hot functions
    adaptive    Adaptive JIT with speculative optimization
    tiered      Tiered JIT (T0 -> T1 -> T2)
    aot         Ahead-of-time native compilation (subprocess)
    wasm        WebAssembly backend via Wasmtime (subprocess)
    gpu         MLIR GPU backend (debug builds; ADESH_TEST_GPU=1)

MULTI-BACKEND REPORT:
    When more than one backend is selected, output becomes a grid:

      Test                      interp    jit      njit     vm
      -----------------------   --------  ------   ------   ------
      math::add                   PASS     PASS     PASS     PASS     0.3ms
      math::div_by_zero           PASS     FAIL     FAIL     PASS     1.1ms  <- inconsistency

    A per-backend summary and an inconsistency report follow the grid, ending
    with an overall verdict:

      Per-Backend Summary
        interp:  3/3 PASS   jit: 2/3 FAIL   njit: 2/3 FAIL   vm: 3/3 PASS
      Inconsistency report   <- tests that differ across backends are highlighted
      Overall verdict:  all consistent OR which backends failed

EXAMPLES:
    adesh test
    adesh test tests/main.adesh
    adesh test tests/main.adesh opt_tests
    adesh test tests/main.adesh --exact opt_tests::test_origin_point
    adesh test tests/main.adesh --skip result_tests
    adesh test tests/main.adesh --list
    adesh test --fail-fast --nocapture tests.adesh
    adesh test --quiet tests.adesh
    adesh test --runtimes=interp,jit,njit tests.adesh
    adesh test --backend-check tests.adesh
    adesh test --format json tests.adesh > out.json
    adesh run --test tests/math_test.adesh
    adesh run --test tests/math_test.adesh test_add
    adesh run --test --test-name test_add tests/math_test.adesh
    adesh run --test --fail-fast all_tests.adesh
    adesh run --test --quiet all_tests.adesh
    adesh run --test --runtimes=jit,njit,vm,aot tests/all.adesh
    adesh run --test --backend-check integration_tests.adesh
    adesh run --test --tags fast tests/math_test.adesh
    adesh run --test --include-tags=fast,unit tests/all.adesh

INLINE TEST BLOCK:
    test math {
        test fn add() {}
        test fn sub() {}
    }"#;
    page("test", "run unit and integration tests", body)
}

fn build_help() -> String {
    let extra = r#"INCREMENTAL BUILD & CACHE:
    --incremental          Enable incremental caching (default: on)
    --no-incremental       Disable incremental caching
    --no-cache             Alias for --no-incremental
    -f, --force            Force a full rebuild, bypassing the cache
    --clear-cache          Clear the compiler artifact cache first

LINK-TIME OPTIMIZATION & PROFILE GUIDED OPTIMIZATION:
    --enable-lto, --lto    Enable LTO for production builds (20-30% smaller)
    --disable-lto, --no-lto  Disable LTO
    --lto=<mode>           off/no/false disable LTO
    --enable-dce           Request dead-code elimination (adeshlink performs DCE)
    --pgo=generate         Generate PGO profile data
    --pgo=use=<path>       Use a PGO profile
    --deterministic        Bit-for-bit reproducible output
    --strip, -s            Strip symbols and debug information

PROJECT TARGETS:
    --bin <name>           Build a named binary target
    --bins                 Build all binary targets

FFI:
    --emit-header [FILE]   Generate a C header for exported functions
    -I<DIR>                Add an include directory
    -L<DIR>                Add a library search path
    -l<LIB>                Link with a library
    --linker-arg <arg>     Pass an argument to the linker
    --external-linker      Delegate linking to external LLVM (clang + lld);
                           requires --codegen=cranelift and an LLVM install
                           (verify: adesh gpu-check --external-linker)

ENVIRONMENT:
    ADESH_RUN_TIMEOUT_MS=15000   Watchdog timeout for `run`
    ADESH_FAST_COMPILE=1         Default to --fast mode
    ADESH_ENABLE_LTO=1           Enable LTO for all builds
    ADESH_ENABLE_DCE=1           Enable dead-code elimination
    ADESH_VERBOSE=1              Show compilation details and linker commands

SPEED GUIDE:
    # Instant development builds (cold ~0.3s, cached <5ms)
    adesh build --fast program.adesh -o app.exe
    adesh build run --fast program.adesh

    # Incremental cached development
    adesh build program.adesh                       # 0ms when unchanged
    adesh build --force program.adesh               # Force recompile
    adesh build --clear-cache program.adesh         # Clean cache, then rebuild

    # Production release
    adesh build program.adesh -O3 --enable-lto -o app.exe
    adesh build program.adesh -O2 --enable-dce -o app.exe

    # Libraries and objects
    adesh build mylib.adesh --lib -o libmylib.a
    adesh build mylib.adesh --library-mode -o libmylib.a
    adesh build mylib.adesh --shared -o mylib.dll
    adesh build program.adesh -c --emit-header program.h

    # Build and run, debug builds
    adesh build program.adesh -o app --run
    adesh build program.adesh --debug -o app.exe"#;
    format!("{}\n{}", crate::cli::build::build_help_message(), extra)
}

fn check_help() -> String {
    let body = r#"USAGE:
    adesh check <FILE>

Runs, in order:
    1. Lexing, parsing, and semantic analysis
    2. Static type checking
    3. Compile-time memory safety, ownership, borrowing, and lifetime checks

FILE may also be an Adesh project manifest:
    adesh check project.adl            Validate a manifest
    adesh check project.lock.adl       Validate a lockfile

No code is executed.

EXAMPLES:
    adesh check program.adesh
    adesh check src/main.adesh
    adesh check project.adl

RELATED:
    adesh lint <file>      Same pipeline, reported as lint
    adesh run --type-check <file>"#;
    page("check", "type-check and validate without executing", body)
}

fn bench_help() -> String {
    let body = r#"USAGE:
    adesh bench [FILE]

Benchmarks execution with the interpreter. With no FILE, `adesh bench`
auto-discovers benches/main.adesh, bench.adesh, tests/main.adesh, or
src/main.adesh.

EXAMPLES:
    adesh bench
    adesh bench benchmarks/sort.adesh
    adesh bench tests/main.adesh"#;
    page("bench", "benchmark program execution", body)
}

fn lint_help() -> String {
    let body = r#"USAGE:
    adesh lint [FILE]

Runs semantic indexing, static type checking, and compile-time memory-safety
checks, then reports lint results. With no FILE, auto-discovers src/main.adesh
or main.adesh.

EXAMPLES:
    adesh lint
    adesh lint src/main.adesh"#;
    page("lint", "run semantic, type, and safety checks", body)
}

fn format_help() -> String {
    let body = r#"USAGE:
    adesh format [OPTIONS] <FILE>
    adesh fmt [OPTIONS] <FILE>        (alias)

OPTIONS:
    --write, -w          Write formatted output back to the file
    --indent <n>         Indentation size (default: 4)
    --tabs               Use tabs instead of spaces
    --check              Check formatting; exit 1 if the file would change

Without --write, formatted source is printed to stdout.

EXAMPLES:
    adesh format program.adesh
    adesh fmt program.adesh
    adesh format --write program.adesh
    adesh format --check program.adesh
    adesh format --tabs --indent 2 --write program.adesh"#;
    page("format", "format source code", body)
}

fn repl_help() -> String {
    let body = r#"USAGE:
    adesh repl

Starts the interactive Read-Eval-Print Loop. Type Adesh expressions and
statements; use Ctrl+D / Ctrl+C to exit.

EXAMPLES:
    adesh repl"#;
    page("repl", "interactive REPL", body)
}

fn compile_help() -> String {
    let body = r#"USAGE:
    adesh compile <IN.adesh> <OUT.adeshbc>

Compiles source to Adesh bytecode (v2) after type and memory-safety checks.

EXAMPLES:
    adesh compile program.adesh program.adeshbc
    adesh run-bc program.adeshbc
    adesh disassemble program.adeshbc"#;
    page("compile", "compile to bytecode", body)
}

fn compile_wasm_help() -> String {
    let body = r#"USAGE:
    adesh compile-wasm <IN.adesh> <OUT.wasm>

Compiles source to WebAssembly and also writes a .js loader next to the output.

EXAMPLES:
    adesh compile-wasm program.adesh app.wasm
    adesh run --wasm program.adesh"#;
    page("compile-wasm", "compile to WebAssembly", body)
}

fn compile_wasm_js_help() -> String {
    let body = r#"USAGE:
    adesh compile-wasm-js <IN.adesh> <OUT_DIR>

Compiles source to <OUT_DIR>/<stem>.wasm and <OUT_DIR>/<stem>.js.

EXAMPLES:
    adesh compile-wasm-js program.adesh dist/wasm"#;
    page("compile-wasm-js", "compile WASM and JS into a directory", body)
}

fn compile_native_help() -> String {
    let body = r#"USAGE:
    adesh compile-native <IN.adesh> <OUT>

Compiles source to a native object or executable through the native pipeline.

EXAMPLES:
    adesh compile-native program.adesh app
    adesh compile-native program.adesh app.exe

For the full option set use `adesh build --help`."#;
    page("compile-native", "compile to a native object/executable", body)
}

fn compile_native_rust_help() -> String {
    let body = r#"USAGE:
    adesh compile-native-rust <IN.adesh> <OUT.rs>

Lowers Adesh source to native Rust source for inspection or external builds.

EXAMPLES:
    adesh compile-native-rust program.adesh program.rs"#;
    page("compile-native-rust", "emit native Rust source", body)
}

fn compile_aot_help() -> String {
    let body = r#"USAGE:
    adesh compile-aot <IN.adesh> <OUT> [OPTIONS]

Legacy power-user AOT command. For new projects use `adesh build`, which has
sensible defaults and automatic output inference.

OUTPUT OPTIONS:
    --shared, --dll, --so       Shared/dynamic library (.dll/.so/.dylib)
    --static, --lib, --a        Static library (.lib/.a)
    --object, --obj, --o        Object file only (.obj/.o)
    -c, --compile-only          Compile only; do not link
    -S, --emit-asm              Emit assembly/IR (not fully implemented)
    --emit-header [FILE]        Generate a C header for exported functions
    --ffi                       Enable FFI mode (C ABI compatibility)
    -I<DIR>                     Add an include directory
    -L<DIR>                     Add a library search directory
    -l<LIB>                     Link with a library

OPTIMIZATION & CACHING:
    --opt O0..O3                Optimization level (default: O2)
    --fast                      Lightning-fast compilation (skip optimizations)
    --incremental               Enable incremental caching
    --no-cache                  Disable the compilation cache
    -f, --force                 Force a rebuild, ignoring the cache
    --clear-cache               Clear the compiler artifact cache
    --debug                     Include debug information

CROSS-COMPILATION:
    --target=<triple>           Cross-compile for <arch>-<vendor>-<os>-<env>

    Common targets:
      x86_64-unknown-linux-gnu     Linux x86_64 (glibc)
      x86_64-unknown-linux-musl    Linux x86_64 (musl, static)
      aarch64-unknown-linux-gnu    Linux ARM64
      arm-unknown-linux-gnueabihf  Linux ARM32 (hard float)
      x86_64-pc-windows-gnu        Windows x86_64 (MinGW)
      x86_64-pc-windows-msvc       Windows x86_64 (MSVC)
      x86_64-apple-darwin          macOS x86_64
      aarch64-apple-darwin         macOS ARM64 (Apple Silicon)
      aarch64-linux-android        Android ARM64
      armv7-linux-androideabi      Android ARM32
      aarch64-apple-ios            iOS ARM64

    Cross-compilation requires the matching toolchain:
      Linux targets: cross-gcc (e.g. x86_64-linux-gnu-gcc)
      Windows from Linux: mingw-w64
      macOS: osxcross or a macOS host
      Android: Android NDK
      iOS: Xcode on a macOS host

FFI (C/C++ INTEROP):
    Export functions with C ABI and generate headers automatically:

        export fn function_name(param1: type1, param2: type2): return_type {
            return value;
        }

    Type mappings:
        Adesh    C type
        i8           int8_t
        i16          int16_t
        i32          int32_t
        i64          int64_t
        u8           uint8_t
        u16          uint16_t
        u32          uint32_t
        u64          uint64_t
        f32          float
        f64          double
        bool         bool
        ptr          void*

    Compile and link with C:
        adesh compile-aot library.adesh library.o -c --emit-header library.h
        gcc program.c library.o -o program.exe

EXAMPLES:
    adesh compile-aot program.adesh app.exe
    adesh compile-aot program.adesh app
    adesh compile-aot program.adesh lib.dll --shared
    adesh compile-aot program.adesh lib.so --so
    adesh compile-aot program.adesh lib.a --static
    adesh compile-aot program.adesh app.o --object
    adesh compile-aot program.adesh app.exe -c
    adesh compile-aot program.adesh app.exe -O3
    adesh compile-aot program.adesh app.exe --debug
    adesh compile-aot program.adesh lib.dll --shared --emit-header lib.h
    adesh compile-aot program.adesh lib.dll --shared -fPIC
    adesh compile-aot --target=x86_64-unknown-linux-gnu program.adesh app
    adesh compile-aot --target=aarch64-unknown-linux-gnu program.adesh app
    adesh compile-aot --target=x86_64-pc-windows-gnu program.adesh app.exe
    adesh compile-aot --target=aarch64-apple-darwin program.adesh app --object

    # Multi-library C interop
    adesh compile-aot math.adesh math.o -c --emit-header math.h
    adesh compile-aot string.adesh string.o -c --emit-header string.h
    gcc app.c math.o string.o -o app.exe"#;
    page("compile-aot", "compile a native executable (power-user AOT)", body)
}

fn disassemble_help() -> String {
    let body = r#"USAGE:
    adesh disassemble <FILE.adeshbc> [--write [OUTPUT]]

OPTIONS:
    --write, -w [file]   Write disassembly to a file. Without a path the
                         default name is <input>.bc.adesh.

Without --write, disassembly is printed to stdout.

EXAMPLES:
    adesh disassemble out.adeshbc
    adesh disassemble out.adeshbc --write
    adesh disassemble out.adeshbc --write disassembly.adesh"#;
    page("disassemble", "disassemble a bytecode file", body)
}

fn docs_help() -> String {
    let body = r#"USAGE:
    adesh docs <IN.adesh> <OUT_DIR>
    adesh doc <IN.adesh> <OUT_DIR>       (alias)

Defaults:
    input  - src/main.adesh or src/lib.adesh when omitted
    output - ./docs when omitted

EXAMPLES:
    adesh docs src/main.adesh docs
    adesh docs src/lib.adesh public/docs"#;
    page("docs", "generate documentation", body)
}

fn link_help() -> String {
    let body = r#"USAGE:
    adesh link <INPUT...> [-o OUTPUT]

Links one or more objects with adeshlink. The default output is a.out.
Inputs can be ADOB objects or other supported object formats.

EXAMPLES:
    adesh link main.adob -o app
    adesh link main.o math.o string.o -o app.exe"#;
    page("link", "link objects with adeshlink", body)
}

fn pgo_help() -> String {
    let body = r#"USAGE:
    adesh pgo merge <PROFILE...> [-o <MERGED.json>]

Merges PGO profiles into one file (default: merged.pgo.json).

EXAMPLES:
    adesh pgo merge run1.json run2.json -o merged.json
    adesh pgo merge profiles/*.json -o all.pgo.json

Related:
    adesh build --pgo=generate program.adesh
    adesh build --pgo=use=merged.json program.adesh"#;
    page("pgo", "merge PGO profile data", body)
}

fn init_help() -> String {
    let body = r#"USAGE:
    adesh init [DIR]
    adesh new <DIR>

Creates an ADL ecosystem project layout with a manifest and starter sources.
`init` defaults to the current directory; `new` defaults to my_app and is
intended for creating a new named project.

Next steps after initialization:
    cd <DIR>
    adesh run src/main.adesh
    adesh build src/main.adesh
    adesh test

EXAMPLES:
    adesh init
    adesh init my_app
    adesh new my_app"#;
    page("init / new", "initialize a new project", body)
}

fn clean_help() -> String {
    let body = r#"USAGE:
    adesh clean [-v|--verbose]

Removes build artifacts (.o, .obj, .a, .lib, .so, .dll, .dylib) from the
current directory. Use --verbose to print every removed file.

EXAMPLES:
    adesh clean
    adesh clean --verbose"#;
    page("clean", "remove build artifacts", body)
}

fn target_help() -> String {
    let body = r#"USAGE:
    adesh target [list]
    adesh target info <TRIPLE>

TARGETS:
    Linux:
      x86_64-unknown-linux-gnu     Linux x86_64 (glibc)
      x86_64-unknown-linux-musl    Linux x86_64 (musl, static)
      aarch64-unknown-linux-gnu    Linux ARM64
      arm-unknown-linux-gnueabihf  Linux ARM32 (hard float)

    Windows:
      x86_64-pc-windows-gnu        Windows x86_64 (MinGW)
      x86_64-pc-windows-msvc       Windows x86_64 (MSVC)
      i686-pc-windows-gnu          Windows x86 (MinGW)

    macOS:
      x86_64-apple-darwin          macOS x86_64 (Intel)
      aarch64-apple-darwin         macOS ARM64 (Apple Silicon)

    Mobile:
      aarch64-linux-android        Android ARM64
      armv7-linux-androideabi      Android ARM32
      aarch64-apple-ios            iOS ARM64

    Embedded:
      thumbv7em-none-eabihf        ARM Cortex-M4/M7
      riscv64gc-unknown-none-elf   RISC-V 64-bit

EXAMPLES:
    adesh target
    adesh target list
    adesh target info aarch64-apple-darwin
    adesh build --target=aarch64-unknown-linux-gnu program.adesh"#;
    page("target", "cross-compilation targets", body)
}

fn gpu_check_help() -> String {
    let gpu_block = if cfg!(debug_assertions) {
        r#"GPU OPTIONS (DEBUG BUILDS ONLY):
    --gpu                  Enable the GPU backend (MLIR -> CUDA/ROCm/Vulkan/Metal)
    --gpu-target <auto|cuda|rocm|vulkan|metal>   Select GPU target (default: auto)
    --gpu-grid <x,y,z>     Grid dimensions — blocks  (default: 1,1,1)
    --gpu-block <x,y,z>    Block dimensions — threads (default: 256,1,1)
    --gpu-shared-mem <bytes>   Shared memory per block (default: 0)
    --dump-mir             Show MIR with ownership/borrow annotations

GPU PIPELINE (4 steps):
    [1] gpu-kernel-outlining + canonicalize   -> .lowered.mlir
    [2] convert-func-to-llvm + reconcile      -> .llvm_ready.mlir
    [3] mlir-translate --mlir-to-llvmir       -> .ll
    [4] llc + clang                           -> .exe / .out

    If steps 2-4 fail (missing dialect plugins), the interpreter fallback runs
    the program.

"#
    } else {
        ""
    };

    let body = format!(
        r#"USAGE:
    adesh gpu-check [-v|--verbose] [--json] [--external-linker]

OPTIONS:
    -v, --verbose          Include optional and missing checks
    --json                 Machine-readable JSON output for scripts/CI
    --external-linker      Also require an external LLVM toolchain
                           (clang + lld) for `build --external-linker`
    --external-toolchain   Alias for --external-linker
    --use-llvm             Alias for --external-linker

{gpu_block}NATIVE PIPELINE:
    adesh codegen -> ADOB -> adeshlink needs no external tools.

EXTERNAL TOOL OVERRIDES:
    ADESH_MLIR_OPT, ADESH_MLIR_TRANSLATE, ADESH_LLC, ADESH_CLANG, ADESH_LLD

EXAMPLES:
    adesh gpu-check
    adesh gpu-check -v
    adesh gpu-check --json
    adesh gpu-check --external-linker
"#
    );
    page("gpu-check", "probe GPU devices and the native toolchain", &body)
}

fn adob_help() -> String {
    let body = r#"USAGE:
    adesh adob <SUBCOMMAND> <FILE.adob>

SUBCOMMANDS:
    inspect <file.adob>            Full object overview
    dump-symbols <file.adob>       Symbol table
    dump-relocations <file.adob>   Relocations table
    dump-sections <file.adob>      Section headers and metadata
    validate <file.adob>           Strict binary validation

EXAMPLES:
    adesh adob inspect out.adob
    adesh adob dump-symbols out.adob
    adesh adob validate out.adob"#;
    page("adob", "inspect and validate ADOB objects", body)
}

fn inspect_help() -> String {
    let body = r#"USAGE:
    adesh inspect <FILE.adob|FILE.o|FILE.obj>
    adesh objdump <FILE>               (alias)

Prints an objdump-style report of an object file.

EXAMPLES:
    adesh inspect out.adob
    adesh objdump out.obj"#;
    page("inspect", "objdump-style object inspection", body)
}

fn crypto_help() -> String {
    let body = r#"USAGE:
    adesh crypto <SUBCOMMAND> [ARGS]

SUBCOMMANDS:
    sha256 <file>            Compute the SHA-256 hash of a file
    hash <file>              Alias for sha256
    random [num_bytes]       Generate secure random hex bytes (default: 32)
    inspect-cert <cert.pem>  Show X.509 subject, issuer, serial, validity,
                             SHA-256 fingerprint, and CA flag

EXAMPLES:
    adesh crypto sha256 archive.tar.gz
    adesh crypto random 64
    adesh crypto inspect-cert server.pem"#;
    page("crypto", "cryptography utilities", body)
}

fn binary_tool_help(command: &str) -> String {
    let body = format!(
        r#"USAGE:
    adesh {command} <FILE.adob|FILE.obj>

Reads an ADOB object and reports its {what}.

EXAMPLES:
    adesh {command} out.adob
    adesh {command} out.obj

RELATED:
    adesh inspect <file>   Full objdump-style inspection
    adesh adob <cmd>       ADOB-specific tools"#,
        what = match command {
            "size" => "text/data sizes",
            "symbols" => "symbol table",
            "sections" => "sections",
            "relocations" => "relocations",
            "map" => "link map",
            _ => "contents",
        }
    );
    page(command, "binary analysis", &body)
}

fn self_test_help() -> String {
    let body = r#"USAGE:
    adesh self-test

Runs the compiler ecosystem self-test:
    [1/6] TargetSpec host detection
    [2/6] QueryEngine caching and dependency tracking
    [3/6] ConstEvaluator deterministic execution
    [4/6] MonomorphizationEngine specialization and deduplication
    [5/6] Sanitizer runtime hooks
    [6/6] AsyncExecutor cooperative runtime

EXAMPLES:
    adesh self-test"#;
    page("self-test", "compiler ecosystem self-test", body)
}

fn bugreport_help() -> String {
    let body = r#"USAGE:
    adesh bugreport

Writes adesh_bugreport.json in the current directory with compiler and target
metadata suitable for attaching to a bug report.

EXAMPLES:
    adesh bugreport"#;
    page("bugreport", "write a bug report bundle", body)
}

fn server_help() -> String {
    let body = r#"USAGE:
    adesh server

Starts the persistent compiler daemon (server mode) and preloads the prelude
module into its cache.

EXAMPLES:
    adesh server"#;
    page("server", "persistent compiler daemon", body)
}

fn monitor_help() -> String {
    let body = r#"USAGE:
    adesh monitor [SUBCOMMAND] [OPTIONS]
    adesh aro [SUBCOMMAND] [OPTIONS]          (alias)
    adesh observatory [SUBCOMMAND] [OPTIONS]  (alias)

With no subcommand, launches the interactive Terminal UI dashboard (Ratatui).

SUBCOMMANDS:
    tui, interactive           Launch the interactive TUI dashboard
    list [options]             List active and recorded execution sessions
    show <id>                  Show complete session details and metrics
    trace <id>                 Inspect compiler and linker stage timings
    stats                      Collector throughput and process resource stats
    export <id>                Export a session report to JSON or CSV
    doctor                     Check OS metrics provider, IPC, and SQLite storage
    purge                      Purge local monitoring database history
    version                    Observatory protocol and schema version

OPTIONS FOR 'list':
    --limit, -l <N>            Maximum sessions to list (default: 50)
    --group, -g                Group sessions by execution mode
    --mode, -m <mode>          Filter by mode (interpreter, jit, nativecompiler, ...)
    --json                     Machine-readable JSON output

OPTIONS FOR 'export':
    --format <json|csv>        Report format (default: json)
    --out <file>               Output destination (default: stdout)

CAPTURE & COCKPIT:
    - Instrumented CLI and linker stages emit non-blocking telemetry via local IPC
    - Interactive GUI cockpit is available in the desktop application (ARO/desktop)
    - Telemetry capture respects ADESH_OBSERVATORY_DISABLED=1

EXAMPLES:
    adesh monitor
    adesh monitor list
    adesh monitor list --group --limit 20
    adesh monitor show sess-1234
    adesh monitor trace sess-1234
    adesh monitor stats
    adesh monitor doctor
    adesh monitor export sess-1234 --out report.json"#;
    page("monitor", "Adesh Runtime Observatory (ARO)", body)
}

fn editor_help() -> String {
    let body = r#"USAGE:
    adesh editor                 Launch with an empty workspace
    adesh editor <file>          Open a file directly
    adesh editor <dir>           Open a directory workspace

KEY FEATURES & WORKFLOWS:
    Modal editing      Normal (Esc), Insert (i, a, o, O), Visual (v), Command (:)
    Run code           F5 or the Run button; executes the active buffer
                       across all execution backends
    Backend switcher   Ctrl+F5 or :backend <name> (11 execution backends)
    Stop program       F6 terminates background execution
    File explorer      Ctrl+E / F3 sidebar tree (copy, cut, paste, delete, auto-scroll)
    Buffers            Tab / Shift+Tab cycle tabs; mouse-clickable tabs and close buttons
    Search & replace   Ctrl+F find, Ctrl+H replace, Ctrl+Shift+F multi-buffer grep
    Terminal           Ctrl+J embedded interactive shell
    Output panel       Ctrl+` toggles stdout/stderr, exit code, execution time
    Command palette    Ctrl+P search, Ctrl+G or :<N> go to line
    File operations    Ctrl+S save, Ctrl+O open, Ctrl+Q / :q quit
    Formatting         :format or the Edit menu
    Themes             Adesh Dark/Light, Gruvbox, Monokai, Dracula, One Dark
                       (:theme <name>)
    In-editor help     F1 or :help for the complete shortcut cheatsheet

EXAMPLES:
    adesh editor
    adesh editor main.adesh
    adesh editor ./my_project"#;
    page("editor", "TUI code editor", body)
}

fn ai_help() -> String {
    let body = r#"USAGE:
    adesh ai <SUBCOMMAND> [ARGS]

SUBCOMMANDS:
    status                Show the native AI engine status report
    setup, download       Install/download AI model files
    update                Update AI model files
    train                 Train a model (requires Python 3)
    evaluate              Evaluate a model (requires Python 3)
    generate <prompt>     Generate Adesh code from a prompt
    explain <code>        Explain code
    fix <code>            Suggest fixes
    chat                  Interactive assistant

Without a subcommand, the AI inference entry point runs.

PYTHON REQUIREMENTS:
    Training and evaluation require a functional Python 3 installation
    (ADESH_PYTHON or PYTHON, a local/ADESH_HOME .venv, the py launcher, or
    python on PATH). Generation falls back to the native in-process engine.

EXAMPLES:
    adesh ai status
    adesh ai generate "sort a list of integers"
    adesh ai explain program.adesh
    adesh ai setup"#;
    page("ai", "AI assistant", body)
}

fn toolchain_help() -> String {
    let body = r#"USAGE:
    adesh toolchain [COMMAND] [OPTIONS]

COMMANDS:
    status, current               Active native toolchain status (default)
    list                          List native and registered external toolchains
    check                         Verify native toolchain readiness and components
    info                          Architecture and ABI specifications
    path                          Toolchain installation directory
    install                       Explain native toolchain installation
    update                        Toolchain update guidance
    external install <name>       Register an external cross-compilation toolchain
    external list                 List registered external toolchains
    --external install <name>     Alias for external toolchain installation
    --external list               Alias for external list
    expose --user|--system        Expose tools on the system PATH

TOOLCHAIN SELECTION:
    --system-toolchain            Use the system toolchain
    --bundled-toolchain           Use the bundled toolchain
    --toolchain <bundled|system>  Explicit selection

NATIVE TOOLCHAIN:
    Codegen: adesh-codegen; objects: ADOB v1.0.0; linker: adeshlink (PE/COFF,
    ELF, Mach-O, WASM); ABI: ADESH_RUNTIME_ABI_V1; targets: x86_64, aarch64,
    riscv64, wasm32. No LLVM, Clang, GCC, or MSVC is required.

EXAMPLES:
    adesh toolchain
    adesh toolchain status
    adesh toolchain list
    adesh toolchain check
    adesh toolchain info
    adesh toolchain path
    adesh toolchain external install gcc-aarch64
    adesh toolchain expose --user"#;
    page("toolchain", "manage toolchains", body)
}

fn pkg_help() -> String {
    let body = r#"USAGE:
    adesh pkg <SUBCOMMAND> [ARGS]

SUBCOMMANDS:
    init [name]        Create adesh.toml in the current directory
    install <pkg>      Install a package dependency
    update             Update package dependencies
    remove <pkg>       Remove an installed package
    list               List installed global packages

Package root:
    %LOCALAPPDATA%\\AdeshLang\\packages on Windows, ./packages otherwise.

EXAMPLES:
    adesh pkg init my_app
    adesh pkg install http
    adesh pkg list
    adesh pkg remove http"#;
    page("pkg", "manage packages", body)
}

fn update_help() -> String {
    let body = r#"USAGE:
    adesh update [--check] [--channel <stable|beta|nightly>] [--url <URL>]

OPTIONS:
    --check, -c            Check for updates without installing
    --channel <name>       stable (default), beta, or nightly
    --url <URL>            Override the update manifest URL

Updates are incremental and SHA-256 verified. After a successful update the
installation health check (`adesh doctor`) runs automatically.

EXAMPLES:
    adesh update
    adesh update --check
    adesh update --channel beta
    adesh update --url https://example.com/adesh/manifest.json"#;
    page("update", "check for and install updates", body)
}

fn rollback_help() -> String {
    let body = r#"USAGE:
    adesh rollback

Restores the previous Adesh version from the versioned installation layout
and then runs `adesh doctor`.

EXAMPLES:
    adesh rollback"#;
    page("rollback", "roll back to the previous version", body)
}

fn env_help() -> String {
    let body = r#"USAGE:
    adesh env [--json | --shell]

OPTIONS:
    --json       Emit machine-readable JSON (version, OS, architecture, home,
                 toolchain, LLVM path, bin directory, active toolchain)
    --shell      Emit shell exports for ADESH_HOME, ADESH_TOOLCHAIN, and PATH
                 (bash/zsh/POSIX, PowerShell, or fish)

Without options, prints a human-readable environment report, including
ADESH_HOME, ADESH_TOOLCHAIN, LLVM, bin directory, and active toolchain.

EXAMPLES:
    adesh env
    adesh env --json
    adesh env --shell
    eval "$(adesh env --shell)""#;
    page("env", "environment paths and toolchain details", body)
}

fn repair_help() -> String {
    let body = r#"USAGE:
    adesh repair

Repairs Adesh toolchain and environment settings by re-resolving
installation paths and restoring expected configuration.

EXAMPLES:
    adesh repair
    adesh doctor"#;
    page("repair", "repair toolchain and environment settings", body)
}

fn doctor_help() -> String {
    let body = r#"USAGE:
    adesh doctor

Checks the compiler version, OS/architecture, binary path, installation home,
stale environment variables, bundled and external toolchains, and PATH setup.
Exits non-zero when issues are found.

EXAMPLES:
    adesh doctor
    adesh repair"#;
    page("doctor", "inspect system and toolchain health", body)
}

fn help_help() -> String {
    let body = r#"USAGE:
    adesh help [COMMAND|TOPIC]
    adesh <COMMAND> --help

EXAMPLES:
    adesh help                 Compact command index
    adesh help run             Detailed help for `run`
    adesh help test            Detailed help for `test`
    adesh help build           Detailed help for `build`
    adesh help language        Type system, memory model, backends, FFI
    adesh help monitor         Adesh Runtime Observatory details

Every command also accepts `-h` / `--help`:
    adesh test --help
    adesh toolchain --help"#;
    page("help", "help for commands and topics", body)
}

fn language_help() -> String {
    let body = r#"TYPE SYSTEM:
    Adesh has a strong, statically-typed type system:
    - Explicit annotations: let x: i32 = 42
    - Inference: let y = 42 (inferred as i32)
    - Generics: fn max<T: Ord>(a: T, b: T) -> T
    - Traits: trait Iterator { fn next(&mut self) -> Option<T> }
    - Pattern matching with exhaustiveness checking
    - Algebraic data types: enum Result<T, E> { Ok(T), Err(E) }
    - Lifetimes: &'a T
    - Fixed-width types: u8, u16, u32, u64, u128, i8-i128, f32, f64
    - Nullable types: Option<T> (no null pointer exceptions)
    - Error handling: Result<T, E> (no exceptions)
    - Ownership and borrowing

MEMORY MODEL (OWNERSHIP-BASED, NO GC):
    Adesh uses deterministic ownership. There is no garbage
    collector. Memory is managed through:
    - Ownership (move semantics, deterministic drop)
    - Borrowing (&T, &mut T)
    - Explicit ARC (via the `share` keyword)
    - Regions / arena allocation
    - Embedded mode: heap + ARC disabled (stack + arena only)

EXECUTION BACKENDS:
    --interpreter            Standard interpreter (default)
    --jit                    JIT compilation (LIR interpreter)
    --jit-native             Native JIT via Cranelift (10-232x faster)
    --mixed                  Hybrid: interpret, then JIT hot functions
    --safe                   Safe mode: no JIT, full GC safety
    --bytecode, --vm         Bytecode VM (deprecated)
    --adaptive               Adaptive JIT with speculative optimization
    --tiered                 Tiered JIT (T0 -> T1 -> T2)
    --aot                    Ahead-of-time native backend
    --wasm                   WebAssembly backend via Wasmtime
    --gpu                    MLIR GPU backend (debug builds, compile-only)

PERFORMANCE (compute-heavy workloads):
    Interpreter:   1.0x (baseline)
    JIT (--jit):   0.8x (interpretation overhead)
    Native JIT:    10-232x faster
    AOT compile:   similar to Native JIT

NATIVE JIT FEATURES:
    - LIR compiled to native machine code at runtime (Cranelift)
    - Function-pointer execution with zero interpretation overhead
    - 71% instruction coverage (46/65 LIR instructions)
    - Loops (for, while, do-while), strings, malloc/free and pointers
    - Arithmetic, bitwise, and comparison operations
    - Ownership/borrowing support (Arc operations)

WHEN TO USE NATIVE JIT:
    Loop-heavy algorithms, array processing, math-intensive code, string
    operations, memory-intensive workloads, performance-critical code paths,
    and real-time systems (deterministic execution).

FFI:
    See `adesh compile-aot --help` for C/C++ interop, type mappings, header
    generation, and linking examples."#;
    page("language", "type system, memory model, and backends", body)
}
