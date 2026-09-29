//! MLIR Dialect definitions, tensor structures, and polyhedral schedule representations.

use std::collections::HashMap;

/// Standard supported MLIR dialects in the Adesh native toolchain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DialectKind {
    /// Affine polyhedral transformations & loop nests
    Affine,
    /// Linear algebra structured ops on tensors/buffers
    Linalg,
    /// Tensor algebra and shape transformations
    Tensor,
    /// Memory reference buffers (strided memrefs)
    MemRef,
    /// GPU host-side launch & device execution
    Gpu,
    /// NVIDIA NVVM IR dialect
    Nvvm,
    /// AMD ROCm ROCDL IR dialect
    Rocdl,
    /// Khronos SPIR-V dialect
    Spirv,
    /// Quantum circuit & gate operations dialect
    Quantum,
    /// Adesh high-level functional & ownership dialect
    Adesh,
}

impl DialectKind {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Affine => "affine",
            Self::Linalg => "linalg",
            Self::Tensor => "tensor",
            Self::MemRef => "memref",
            Self::Gpu => "gpu",
            Self::Nvvm => "nvvm",
            Self::Rocdl => "rocdl",
            Self::Spirv => "spirv",
            Self::Quantum => "quantum",
            Self::Adesh => "adesh",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "affine" => Some(Self::Affine),
            "linalg" => Some(Self::Linalg),
            "tensor" => Some(Self::Tensor),
            "memref" => Some(Self::MemRef),
            "gpu" => Some(Self::Gpu),
            "nvvm" => Some(Self::Nvvm),
            "rocdl" => Some(Self::Rocdl),
            "spirv" => Some(Self::Spirv),
            "quantum" => Some(Self::Quantum),
            "adesh" => Some(Self::Adesh),
            _ => None,
        }
    }
}

/// N-dimensional tensor shape specification with element data type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TensorShape {
    /// Dimension sizes (dynamic dimensions denoted by -1)
    pub dims: Vec<i64>,
    /// Element type (e.g., "f32", "f64", "f16", "bf16", "i32", "i8", "complex64")
    pub element_type: String,
}

impl TensorShape {
    pub fn new(dims: Vec<i64>, element_type: impl Into<String>) -> Self {
        Self {
            dims,
            element_type: element_type.into(),
        }
    }

    /// Total number of elements if statically shaped.
    pub fn static_element_count(&self) -> Option<usize> {
        let mut count = 1usize;
        for &d in &self.dims {
            if d < 0 {
                return None;
            }
            count = count.checked_mul(d as usize)?;
        }
        Some(count)
    }
}

/// Polyhedral loop iteration domain and affine schedule map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolyhedralSchedule {
    /// Outer and inner loop variable names
    pub loop_vars: Vec<String>,
    /// Lower and upper iteration bounds per loop nest
    pub bounds: Vec<(i64, i64)>,
    /// Loop tiling factors (e.g. [32, 32, 16] for matrix multiplication)
    pub tile_sizes: Vec<usize>,
    /// Parallelization flags per loop dimension
    pub parallel_dims: Vec<bool>,
}

impl PolyhedralSchedule {
    pub fn new(loop_vars: Vec<String>, bounds: Vec<(i64, i64)>) -> Self {
        let len = loop_vars.len();
        Self {
            loop_vars,
            bounds,
            tile_sizes: vec![1; len],
            parallel_dims: vec![false; len],
        }
    }
}

/// An MLIR operation instance with attributes and tensor operands.
#[derive(Debug, Clone)]
pub struct DialectOp {
    pub dialect: DialectKind,
    pub op_name: String,
    pub operands: Vec<String>,
    pub results: Vec<String>,
    pub attributes: HashMap<String, String>,
    pub tensor_shape: Option<TensorShape>,
}

impl DialectOp {
    pub fn new(dialect: DialectKind, op_name: impl Into<String>) -> Self {
        Self {
            dialect,
            op_name: op_name.into(),
            operands: Vec::new(),
            results: Vec::new(),
            attributes: HashMap::new(),
            tensor_shape: None,
        }
    }
}

/// Registry of dialect modules and link-time lowering rules.
#[derive(Debug, Default)]
pub struct DialectRegistry {
    pub operations: Vec<DialectOp>,
    pub schedules: HashMap<String, PolyhedralSchedule>,
}

impl DialectRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_op(&mut self, op: DialectOp) {
        self.operations.push(op);
    }

    pub fn register_schedule(&mut self, kernel_name: String, schedule: PolyhedralSchedule) {
        self.schedules.insert(kernel_name, schedule);
    }
}
