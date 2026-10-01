//! Metadata specifications for ADOB (Security, Debug, Unwind, Accelerators, Device Memory, Build).

/// Security metadata and hardening flags.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SecurityMetadata {
    pub stack_protection: bool,
    pub cfi: bool,
    pub shadow_stack: bool,
    pub pac: bool,
    pub bti: bool,
    pub dep_nx: bool,
    pub aslr: bool,
    pub relro: bool,
    pub signed_code: bool,
    pub control_flow_guard: bool,
}

/// Source file entry in debug info.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugSourceFile {
    pub file_id: u32,
    pub path: String,
    pub directory: String,
    pub checksum: Option<[u8; 16]>,
}

/// Source line table record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugLineRecord {
    pub code_offset: u64,
    pub file_id: u32,
    pub line: u32,
    pub column: u32,
    pub is_stmt: bool,
    pub is_prologue_end: bool,
    pub is_epilogue_begin: bool,
}

/// Variable location for debugging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugVariable {
    pub name: String,
    pub type_name: String,
    pub scope_start: u64,
    pub scope_end: u64,
    pub stack_offset: Option<i64>,
    pub register_id: Option<u32>,
}

/// High-level debug metadata.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DebugInfo {
    pub source_files: Vec<DebugSourceFile>,
    pub line_tables: Vec<DebugLineRecord>,
    pub variables: Vec<DebugVariable>,
    pub producer: String,
    pub language: String,
}

/// Exception and Unwind format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum UnwindFormat {
    #[default]
    None,
    WindowsSeh,
    DwarfCfi,
    ArmEhabi,
    ItaniumAbi,
    WasmExceptions,
}

/// Exception & Unwind metadata.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UnwindMetadata {
    pub format: UnwindFormat,
    pub raw_unwind_info: Vec<u8>,
    pub personality_symbol: Option<String>,
    pub lsda_symbol: Option<String>,
}

/// Accelerator artifact type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum AcceleratorArtifact {
    #[default]
    MachineCode,
    KernelBinary,
    DeviceIR,
    TensorProgram,
    Firmware,
    CommandGraph,
    CustomBinary,
}

/// Device Memory Space classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DeviceMemoryModel {
    HostMemory,
    #[default]
    DeviceMemory,
    SharedMemory,
    ConstantMemory,
    LocalMemory,
    ScratchMemory,
    DmaMemory,
    PersistentMemory,
}

/// GPU Kernel metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuKernelMetadata {
    pub kernel_name: String,
    pub grid_dimensions: (u32, u32, u32),
    pub block_dimensions: (u32, u32, u32),
    pub shared_memory_bytes: u32,
    pub constant_memory_bytes: u32,
    pub register_count: u32,
    pub architecture: String,
    pub kernel_abi: String,
    pub device_symbols: Vec<String>,
    pub host_dependencies: Vec<String>,
}

impl GpuKernelMetadata {
    pub fn new(kernel_name: impl Into<String>) -> Self {
        Self {
            kernel_name: kernel_name.into(),
            grid_dimensions: (1, 1, 1),
            block_dimensions: (1, 1, 1),
            shared_memory_bytes: 0,
            constant_memory_bytes: 0,
            register_count: 0,
            architecture: "generic".to_string(),
            kernel_abi: "default".to_string(),
            device_symbols: Vec::new(),
            host_dependencies: Vec::new(),
        }
    }
}

/// Tensor element types for NPU/TPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TensorElementType {
    F16,
    BF16,
    F32,
    F64,
    I8,
    U8,
    I16,
    I32,
    FP8,
    Custom(u32),
}

/// NPU / TPU Tensor graph metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TensorMetadata {
    pub name: String,
    pub shapes: Vec<Vec<usize>>,
    pub element_types: Vec<TensorElementType>,
    pub memory_spaces: Vec<DeviceMemoryModel>,
    pub dma_requirements: Vec<String>,
    pub tiling_factors: Vec<usize>,
    pub quantization_params: Option<String>,
    pub vector_width: u32,
    pub matrix_dimensions: (u32, u32),
}

impl TensorMetadata {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            shapes: Vec::new(),
            element_types: Vec::new(),
            memory_spaces: Vec::new(),
            dma_requirements: Vec::new(),
            tiling_factors: Vec::new(),
            quantization_params: None,
            vector_width: 1,
            matrix_dimensions: (1, 1),
        }
    }
}

/// Complete Accelerator Metadata.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AcceleratorMetadata {
    pub artifact: AcceleratorArtifact,
    pub gpu_kernels: Vec<GpuKernelMetadata>,
    pub tensors: Vec<TensorMetadata>,
    pub device_memory_model: Vec<DeviceMemoryModel>,
}

/// Build & Reproducibility metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildMetadata {
    pub compiler_version: String,
    pub adob_version: u16,
    pub target_triple: String,
    pub opt_level: u8,
    pub feature_set: Vec<String>,
    pub source_hash: Option<[u8; 32]>,
    pub timestamp_utc: Option<u64>,
    pub deterministic: bool,
}

impl Default for BuildMetadata {
    fn default() -> Self {
        Self {
            compiler_version: env!("CARGO_PKG_VERSION").to_string(),
            adob_version: 1,
            target_triple: "x86_64-pc-windows-msvc".to_string(),
            opt_level: 2,
            feature_set: Vec::new(),
            source_hash: None,
            timestamp_utc: None,
            deterministic: true,
        }
    }
}

/// Advanced Runtime & Memory Safety Metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafetyMetadata {
    pub bounds_checking: bool,
    pub overflow_checks: bool,
    pub null_pointer_checks: bool,
    pub memory_sanitizer: bool,
    pub address_sanitizer: bool,
    pub thread_sanitizer: bool,
    pub strict_provenance: bool,
    pub isolated_heap: bool,
    pub stack_canary_present: bool,
}

impl Default for SafetyMetadata {
    fn default() -> Self {
        Self {
            bounds_checking: true,
            overflow_checks: true,
            null_pointer_checks: true,
            memory_sanitizer: false,
            address_sanitizer: false,
            thread_sanitizer: false,
            strict_provenance: true,
            isolated_heap: false,
            stack_canary_present: true,
        }
    }
}

/// Memory Consistency & Ordering Model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MemoryOrderModel {
    Relaxed,
    Acquire,
    Release,
    AcqRel,
    #[default]
    SeqCst,
}

/// Thread Local Storage Access Model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TlsModel {
    GeneralDynamic,
    LocalDynamic,
    InitialExec,
    #[default]
    LocalExec,
}

/// Thread Safety and Concurrency Metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadSafetyMetadata {
    pub default_memory_order: MemoryOrderModel,
    pub tls_model: TlsModel,
    pub atomic_alignment: u32,
    pub lock_free_primitives: bool,
    pub data_race_detection: bool,
}

impl Default for ThreadSafetyMetadata {
    fn default() -> Self {
        Self {
            default_memory_order: MemoryOrderModel::SeqCst,
            tls_model: TlsModel::LocalExec,
            atomic_alignment: 8,
            lock_free_primitives: true,
            data_race_detection: false,
        }
    }
}

/// Binary-Level & Link-Time Optimization Metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptimizationMetadata {
    pub lto_eligible: bool,
    pub icf_eligible: bool,
    pub dead_code_eliminated: bool,
    pub peephole_optimized: bool,
    pub constant_propagation: bool,
    pub branch_folded: bool,
    pub strength_reduced: bool,
    pub vectorized: bool,
}

impl Default for OptimizationMetadata {
    fn default() -> Self {
        Self {
            lto_eligible: true,
            icf_eligible: true,
            dead_code_eliminated: true,
            peephole_optimized: true,
            constant_propagation: true,
            branch_folded: true,
            strength_reduced: true,
            vectorized: false,
        }
    }
}
