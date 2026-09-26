//! CLI Runtime Configuration
//!
//! Defines all configurable aspects of the AdeshLang runtime execution.

use std::time::Duration;

/// Execution backend selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum ExecutionBackend {
    /// Standard interpreter (default, most compatible)
    #[default]
    Interpreter,
    /// JIT compilation using LIR executor
    Jit,
    /// Native JIT compilation using Cranelift (true native code generation)
    NativeJit,
    /// Hybrid mode: interpret first, JIT hot functions
    Mixed,
    /// Safe mode: no JIT, full GC safety checks
    Safe,
    /// Bytecode VM execution
    Bytecode,
    /// Adaptive JIT with speculative optimization
    AdaptiveJit,
    /// Tiered JIT compilation
    TieredJit,
    /// Ahead-Of-Time native compilation via Cranelift
    Aot,
    /// WebAssembly backend
    Wasm,
    /// MLIR GPU backend (debug builds only)
    #[cfg(debug_assertions)]
    Gpu,
}

/// GPU backend target selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum GpuTarget {
    /// Automatically select a supported GPU target
    #[default]
    Auto,
    /// NVIDIA CUDA (NVVM/PTX)
    Cuda,
    /// AMD ROCm (ROCDL)
    Rocm,
    /// Vulkan (SPIR-V)
    Vulkan,
    /// Apple Metal
    Metal,
}

impl GpuTarget {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "cuda" | "nvidia" | "nvptx" => Some(Self::Cuda),
            "rocm" | "amd" | "rocdl" => Some(Self::Rocm),
            "vulkan" | "spirv" | "spir-v" => Some(Self::Vulkan),
            "metal" | "apple" => Some(Self::Metal),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Cuda => "cuda",
            Self::Rocm => "rocm",
            Self::Vulkan => "vulkan",
            Self::Metal => "metal",
        }
    }
}

impl ExecutionBackend {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "interpreter" | "interp" => Some(Self::Interpreter),
            "jit" => Some(Self::Jit),
            "native-jit" | "jit-native" | "njit" => Some(Self::NativeJit),
            "mixed" | "hybrid" => Some(Self::Mixed),
            "safe" => Some(Self::Safe),
            "bytecode" | "bc" | "vm" => Some(Self::Bytecode),
            "adaptive" | "adaptive-jit" | "ajit" => Some(Self::AdaptiveJit),
            "tiered" | "tiered-jit" | "tjit" => Some(Self::TieredJit),
            "aot" | "native" => Some(Self::Aot),
            "wasm" => Some(Self::Wasm),
            #[cfg(debug_assertions)]
            "gpu" | "mlir-gpu" => Some(Self::Gpu),
            _ => None,
        }
    }
}

/// Optimization level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OptLevel {
    /// No optimization (fastest compilation, slowest runtime)
    O0,
    /// Basic optimization
    #[default]
    O1,
    /// Standard optimization
    O2,
    /// Maximum optimization (slowest compilation, fastest runtime)
    O3,
}

impl OptLevel {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "O0" | "0" => Some(Self::O0),
            "O1" | "1" => Some(Self::O1),
            "O2" | "2" => Some(Self::O2),
            "O3" | "3" => Some(Self::O3),
            _ => None,
        }
    }
}

/// Recursion optimization mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecursionOptMode {
    /// Full optimization: TCO + memoization + trampolining + JIT inlining
    #[default]
    Full,
    /// Tail call optimization only
    Tco,
    /// Memoization only
    Memo,
    /// JIT inlining only
    JitInline,
    /// No recursion optimizations
    None,
}

impl RecursionOptMode {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "full" | "all" => Some(Self::Full),
            "tco" | "tailcall" => Some(Self::Tco),
            "memo" | "memoize" | "memoization" => Some(Self::Memo),
            "jit-inline" | "inline" => Some(Self::JitInline),
            "none" | "off" => Some(Self::None),
            _ => None,
        }
    }

    /// Convert to RecursionOptConfig for backend use
    #[cfg(not(target_arch = "wasm32"))]
    pub fn to_config(&self) -> crate::backends::recursion_opt::RecursionOptConfig {
        use crate::backends::recursion_opt::RecursionOptConfig;
        match self {
            Self::Full => RecursionOptConfig::default(), // All optimizations enabled
            Self::Tco => RecursionOptConfig {
                tco_enabled: true,
                memo_enabled: false,
                trampoline_enabled: false,
                jit_inline_enabled: false,
                max_recursion_depth: 1000,
                max_cache_size: 0,
            },
            Self::Memo => RecursionOptConfig {
                tco_enabled: false,
                memo_enabled: true,
                trampoline_enabled: false,
                jit_inline_enabled: false,
                max_recursion_depth: 1000,
                max_cache_size: 10000,
            },
            Self::JitInline => RecursionOptConfig {
                tco_enabled: false,
                memo_enabled: false,
                trampoline_enabled: false,
                jit_inline_enabled: true,
                max_recursion_depth: 1000,
                max_cache_size: 0,
            },
            Self::None => RecursionOptConfig::none(),
        }
    }
}

/// IR dump options
#[derive(Debug, Clone, Default)]
pub struct DumpOptions {
    /// Dump AST
    pub dump_ast: bool,
    /// Dump HIR
    pub dump_hir: bool,
    /// Dump MIR (Memory Intermediate Representation — ownership/borrow annotated)
    pub dump_mir: bool,
    /// Dump LIR
    pub dump_lir: bool,
    /// Dump VIR (Value Intermediate Representation)
    pub dump_vir: bool,
    /// Dump MLIR (Multi-Level IR)
    pub dump_mlir: bool,
    /// Dump bytecode
    pub dump_bytecode: bool,
    /// Dump all IRs
    pub dump_all: bool,
    /// Write AST dump to file (auto-generate name if true with None)
    pub write_ast_file: Option<String>,
    /// Write HIR dump to file (auto-generate name if true with None)
    pub write_hir_file: Option<String>,
    /// Write LIR dump to file (auto-generate name if true with None)
    pub write_lir_file: Option<String>,
    /// Write VIR dump to file (auto-generate name if true with None)
    pub write_vir_file: Option<String>,
    /// Write MLIR dump to file (auto-generate name if true with None)
    pub write_mlir_file: Option<String>,
    /// Emit GPU kernels in MLIR dump output
    pub mlir_enable_gpu: bool,
    /// Write IR dump to file (auto-generate name if true with None)
    pub write_ir_file: Option<String>,
    /// Dump CFG
    pub dump_cfg: bool,
    /// Write CFG dump to file (auto-generate name if true with None)
    pub write_cfg_file: Option<String>,
}

impl DumpOptions {
    pub fn should_dump_ast(&self) -> bool {
        self.dump_ast || self.dump_all || self.write_ast_file.is_some()
    }

    pub fn should_dump_hir(&self) -> bool {
        self.dump_hir || self.dump_all || self.write_hir_file.is_some()
    }

    pub fn should_dump_mir(&self) -> bool {
        self.dump_mir || self.dump_all
    }

    pub fn should_dump_lir(&self) -> bool {
        self.dump_lir || self.dump_all || self.write_lir_file.is_some()
    }

    pub fn should_dump_vir(&self) -> bool {
        self.dump_vir || self.dump_all || self.write_vir_file.is_some()
    }

    pub fn should_dump_mlir(&self) -> bool {
        self.dump_mlir || self.dump_all || self.write_mlir_file.is_some()
    }

    pub fn should_dump_bytecode(&self) -> bool {
        self.dump_bytecode || self.dump_all
    }

    pub fn should_dump_cfg(&self) -> bool {
        self.dump_cfg || self.dump_all || self.write_cfg_file.is_some()
    }
}

/// IO configuration
#[derive(Debug, Clone, Default)]
pub struct IoConfig {
    /// Use buffered stdout for performance
    pub buffered_stdout: bool,
    /// Enable interactive input mode
    pub interactive_input: bool,
    /// Disable ANSI color codes
    pub no_color: bool,
}

/// Complete runtime configuration
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Execution backend
    pub backend: ExecutionBackend,
    /// Optimization level
    pub opt_level: OptLevel,
    /// Enable ownership checking (default: true)
    pub check_ownership: bool,
    /// Enable move semantics checking (default: true)
    pub check_moves: bool,
    /// Recursion optimization mode
    pub recursion_opt: RecursionOptMode,
    /// Dump options
    pub dump: DumpOptions,
    /// IO configuration
    pub io: IoConfig,
    /// Enable profiling/timing output
    pub profile: bool,
    /// Enable verbose output
    pub verbose: bool,
    /// Enable quiet mode (suppress progress output)
    pub quiet: bool,
    /// Disable all compiler and runtime warnings
    pub disable_warnings: bool,
    /// Enable debug output
    pub debug: bool,
    /// Show memory usage statistics after execution
    pub show_memory: bool,
    /// Enable verbose FFI call logging and resolution tracing
    pub ffi_debug: bool,
    /// Additional header files to import for FFI (@cImport equivalent via CLI)
    pub ffi_imports: Vec<String>,
    /// Additional WASM modules to load and expose as imports
    pub wasm_modules: Vec<String>,
    /// Library search paths for FFI resolution (-L)
    pub lib_paths: Vec<String>,
    /// Libraries to link/load for FFI (-l)
    pub link_libs: Vec<String>,
    /// Stack size in bytes (default: 32MB, auto-scales adaptively to 1GB)
    pub stack_size: usize,
    /// Heap size limit in bytes (0 = unlimited)
    pub heap_size: usize,
    /// Enable adaptive memory management
    pub adaptive_memory: bool,
    /// Maximum stack size for adaptive growth
    pub max_stack_size: usize,
    /// Enable embedded mode (stack + arena only, heap + ARC disabled)
    pub embedded: bool,
    /// Run tests instead of executing main
    pub run_tests: bool,
    /// Enable cross-backend conformance checks while testing
    pub backend_check: bool,
    /// Explicit list of backends to run tests against (empty = use `backend` field only).
    /// Populated by `--runtimes=jit,njit,vm,...`
    pub test_backends: Vec<ExecutionBackend>,
    /// Stop test run on first FAIL/PANIC/TIMEOUT
    pub fail_fast: bool,
    /// Include only tests with one of these tags
    pub include_tags: Vec<String>,
    /// Run only a specific test (or test group prefix)
    pub test_name: Option<String>,
    /// Test report output format
    pub test_output_format: TestOutputFormat,
    /// Show captured output for passing tests (Rust-style --nocapture)
    pub test_nocapture: bool,
    /// Exactly match test name rather than substring match (cargo test --exact)
    pub test_exact: bool,
    /// Skip tests matching this pattern (cargo test --skip)
    pub test_skip: Option<String>,
    /// List all matching tests without running them (cargo test --list)
    pub test_list: bool,
    /// Run only ignored tests (cargo test --ignored)
    pub test_ignored: bool,
    /// Run both ignored and unignored tests (cargo test --include-ignored)
    pub test_include_ignored: bool,
    /// Number of worker threads for parallel test execution (--test-threads)
    pub test_threads: Option<usize>,
    /// Execute tests sequentially one-by-one (--serial / --sequential)
    pub test_serial: bool,
    /// Use LIR backend path instead of VIR (default: false, use VIR)
    pub use_lir: bool,
    /// GPU target for the MLIR GPU backend
    pub gpu_target: GpuTarget,
    /// GPU grid dimensions (blocks)
    pub gpu_grid: (u32, u32, u32),
    /// GPU block dimensions (threads)
    pub gpu_block: (u32, u32, u32),
    /// GPU shared memory size in bytes
    pub gpu_shared_mem: usize,
    /// Kill the process if a `run` still has not finished (CLI `--timeout` / `ADESH_RUN_TIMEOUT_MS`).
    pub run_timeout: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestOutputFormat {
    Text,
    Json,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            backend: ExecutionBackend::Interpreter,
            opt_level: OptLevel::O1,
            recursion_opt: RecursionOptMode::Full,
            dump: DumpOptions::default(),
            io: IoConfig::default(),
            profile: false,
            verbose: false,
            quiet: false,
            disable_warnings: std::env::var("ADESHLANG_DISABLE_WARNINGS").is_ok()
                || std::env::var("ADESHLANG_NO_WARNINGS").is_ok(),
            debug: false,
            show_memory: false,
            ffi_debug: false,
            ffi_imports: Vec::new(),
            wasm_modules: Vec::new(),
            lib_paths: Vec::new(),
            link_libs: Vec::new(),
            stack_size: 128 * 1024 * 1024, // 128MB initial (increased to prevent stack overflow in debug mode)
            heap_size: 0,                  // Unlimited
            adaptive_memory: true,         // Enable adaptive growth by default
            max_stack_size: 1024 * 1024 * 1024, // 1GB max
            embedded: false,               // Not embedded by default
            check_ownership: true,         // Enable ownership checking by default
            check_moves: true,             // Enable move checking by default
            run_tests: false,              // Don't run tests by default
            backend_check: false,
            test_backends: Vec::new(),
            fail_fast: false,
            include_tags: Vec::new(),
            test_name: None,
            test_output_format: TestOutputFormat::Text,
            test_nocapture: false,
            test_exact: false,
            test_skip: None,
            test_list: false,
            test_ignored: false,
            test_include_ignored: false,
            test_threads: None,
            test_serial: false,
            use_lir: false,
            gpu_target: GpuTarget::Auto,
            gpu_grid: (1, 1, 1),
            gpu_block: (256, 1, 1),
            gpu_shared_mem: 0,
            run_timeout: None,
        }
    }
}

impl RuntimeConfig {
    /// Create a new default configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a configuration optimized for development/debugging
    pub fn development() -> Self {
        Self {
            backend: ExecutionBackend::Interpreter,
            opt_level: OptLevel::O0,
            recursion_opt: RecursionOptMode::None,
            dump: DumpOptions::default(),
            io: IoConfig::default(),
            profile: false,
            verbose: true,
            quiet: false,
            disable_warnings: false,
            debug: true,
            show_memory: false,
            check_ownership: true, // Extra checks in dev mode
            check_moves: true,
            stack_size: 128 * 1024 * 1024, // 128MB initial (increased to prevent stack overflow in debug mode)
            heap_size: 0,                  // Unlimited
            adaptive_memory: true,         // Enable adaptive growth
            max_stack_size: 1024 * 1024 * 1024, // 1GB max (increased from 256MB)
            ffi_debug: true,
            ffi_imports: Vec::new(),
            wasm_modules: Vec::new(),
            lib_paths: Vec::new(),
            link_libs: Vec::new(),
            embedded: false,
            run_tests: false,
            backend_check: false,
            test_backends: Vec::new(),
            fail_fast: false,
            include_tags: Vec::new(),
            test_name: None,
            test_output_format: TestOutputFormat::Text,
            test_nocapture: false,
            test_exact: false,
            test_skip: None,
            test_list: false,
            test_ignored: false,
            test_include_ignored: false,
            test_threads: None,
            test_serial: false,
            use_lir: false,
            gpu_target: GpuTarget::Auto,
            gpu_grid: (1, 1, 1),
            gpu_block: (256, 1, 1),
            gpu_shared_mem: 0,
            run_timeout: None,
        }
    }

    /// Create a configuration optimized for production performance
    pub fn production() -> Self {
        Self {
            backend: ExecutionBackend::Jit,
            opt_level: OptLevel::O3,
            recursion_opt: RecursionOptMode::Full,
            dump: DumpOptions::default(),
            io: IoConfig {
                buffered_stdout: true,
                interactive_input: false,
                no_color: false,
            },
            profile: false,
            verbose: false,
            quiet: false,
            disable_warnings: false,
            debug: false,
            show_memory: false,
            stack_size: 128 * 1024 * 1024, // 128MB for production (increased to prevent stack overflow in debug mode)
            heap_size: 0,                  // Unlimited
            adaptive_memory: true,         // Enable adaptive growth
            max_stack_size: 2048 * 1024 * 1024, // 2GB max for production (increased from 512MB)
            ffi_debug: false,
            ffi_imports: Vec::new(),
            wasm_modules: Vec::new(),
            lib_paths: Vec::new(),
            link_libs: Vec::new(),
            embedded: false,
            check_ownership: true, // Keep ownership checks in production
            check_moves: true,
            run_tests: false,
            backend_check: false,
            test_backends: Vec::new(),
            fail_fast: false,
            include_tags: Vec::new(),
            test_name: None,
            test_output_format: TestOutputFormat::Text,
            test_nocapture: false,
            test_exact: false,
            test_skip: None,
            test_list: false,
            test_ignored: false,
            test_include_ignored: false,
            test_threads: None,
            test_serial: false,
            use_lir: false,
            gpu_target: GpuTarget::Auto,
            gpu_grid: (1, 1, 1),
            gpu_block: (256, 1, 1),
            gpu_shared_mem: 0,
            run_timeout: None,
        }
    }

    /// Create a safe mode configuration (maximum compatibility and safety)
    pub fn safe() -> Self {
        Self {
            backend: ExecutionBackend::Safe,
            opt_level: OptLevel::O1,
            recursion_opt: RecursionOptMode::None,
            dump: DumpOptions::default(),
            io: IoConfig::default(),
            profile: false,
            verbose: false,
            quiet: false,
            disable_warnings: false,
            debug: false,
            show_memory: false,
            stack_size: 16 * 1024 * 1024, // 16MB default
            heap_size: 0,                 // Unlimited
            adaptive_memory: true,        // Enable adaptive growth
            ffi_debug: false,
            ffi_imports: Vec::new(),
            wasm_modules: Vec::new(),
            lib_paths: Vec::new(),
            link_libs: Vec::new(),
            max_stack_size: 128 * 1024 * 1024, // 128MB max for safe mode
            embedded: false,
            check_ownership: true, // Maximum safety checks
            check_moves: true,
            run_tests: false,
            backend_check: false,
            test_backends: Vec::new(),
            fail_fast: false,
            include_tags: Vec::new(),
            test_name: None,
            test_output_format: TestOutputFormat::Text,
            test_nocapture: false,
            test_exact: false,
            test_skip: None,
            test_list: false,
            test_ignored: false,
            test_include_ignored: false,
            test_threads: None,
            test_serial: false,
            use_lir: false,
            gpu_target: GpuTarget::Auto,
            gpu_grid: (1, 1, 1),
            gpu_block: (256, 1, 1),
            gpu_shared_mem: 0,
            run_timeout: None,
        }
    }

    /// Builder pattern: set backend
    pub fn with_backend(mut self, backend: ExecutionBackend) -> Self {
        self.backend = backend;
        self
    }

    /// Builder pattern: set optimization level
    pub fn with_opt_level(mut self, level: OptLevel) -> Self {
        self.opt_level = level;
        self
    }

    /// Builder pattern: set recursion optimization mode
    pub fn with_recursion_opt(mut self, mode: RecursionOptMode) -> Self {
        self.recursion_opt = mode;
        self
    }

    /// Builder pattern: enable profiling
    pub fn with_profiling(mut self) -> Self {
        self.profile = true;
        self
    }

    /// Builder pattern: enable verbose mode
    pub fn with_verbose(mut self) -> Self {
        self.verbose = true;
        self
    }

    /// Builder pattern: force LIR backend path instead of VIR
    pub fn with_use_lir(mut self) -> Self {
        self.use_lir = true;
        self
    }
}

/// Configuration file format (TOML-like)
pub const DEFAULT_CONFIG: &str = r#"
# AdeshLang Runtime Configuration
# This file configures the default behavior of the AdeshLang runtime.

[execution]
# Execution backend: interpreter, jit, mixed, safe, bytecode
backend = "interpreter"

# Optimization level: O0, O1, O2, O3
opt_level = "O1"

[io]
# Use buffered stdout
buffered_stdout = false

# Enable interactive input
interactive_input = false

# Disable ANSI colors
no_color = false

[debug]
# Enable profiling output
profile = false

# Enable verbose output
verbose = false

# Enable debug output
debug = false

[dump]
# Dump AST
dump_ast = false

# Dump HIR
dump_hir = false

# Dump LIR
dump_lir = false

# Dump bytecode
dump_bytecode = false
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_from_str() {
        assert_eq!(
            ExecutionBackend::from_str("jit"),
            Some(ExecutionBackend::Jit)
        );
        assert_eq!(
            ExecutionBackend::from_str("interpreter"),
            Some(ExecutionBackend::Interpreter)
        );
        assert_eq!(
            ExecutionBackend::from_str("mixed"),
            Some(ExecutionBackend::Mixed)
        );
        assert_eq!(
            ExecutionBackend::from_str("safe"),
            Some(ExecutionBackend::Safe)
        );
        #[cfg(debug_assertions)]
        assert_eq!(
            ExecutionBackend::from_str("gpu"),
            Some(ExecutionBackend::Gpu)
        );
        assert_eq!(ExecutionBackend::from_str("invalid"), None);
    }

    #[test]
    fn test_opt_level_from_str() {
        assert_eq!(OptLevel::from_str("O0"), Some(OptLevel::O0));
        assert_eq!(OptLevel::from_str("O3"), Some(OptLevel::O3));
        assert_eq!(OptLevel::from_str("invalid"), None);
    }

    #[test]
    fn test_gpu_target_from_str() {
        assert_eq!(GpuTarget::from_str("auto"), Some(GpuTarget::Auto));
        assert_eq!(GpuTarget::from_str("cuda"), Some(GpuTarget::Cuda));
        assert_eq!(GpuTarget::from_str("rocm"), Some(GpuTarget::Rocm));
        assert_eq!(GpuTarget::from_str("vulkan"), Some(GpuTarget::Vulkan));
        assert_eq!(GpuTarget::from_str("metal"), Some(GpuTarget::Metal));
        assert_eq!(GpuTarget::from_str("invalid"), None);
    }

    #[test]
    fn test_config_builder() {
        let config = RuntimeConfig::new()
            .with_backend(ExecutionBackend::Jit)
            .with_opt_level(OptLevel::O3)
            .with_profiling();

        assert_eq!(config.backend, ExecutionBackend::Jit);
        assert_eq!(config.opt_level, OptLevel::O3);
        assert!(config.profile);
    }
}
