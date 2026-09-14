//! MLIR Backend
//!
//! This backend lowers VIR (Value IR) to MLIR for GPU acceleration and LLVM code generation.
//!
//! Architecture:
//! ```
//! VIR → MLIR Lowering → MLIR Dialects → {LLVM, GPU} → Binary
//! ```
//!
//! MLIR Dialects used:
//! - arith: Arithmetic operations
//! - memref: Memory references
//! - scf: Structured control flow
//! - affine: Affine loops (optimization)
//! - vector: SIMD operations
//! - gpu: GPU kernels
//! - llvm: LLVM IR lowering

pub mod arc_runtime;
pub mod dialects;
pub mod gpu;
pub mod lowering;
pub mod pipeline;
pub mod types;

use crate::backends::common::{BackendError, BackendResult, VirBackend};
use crate::backends::mlir::gpu::GpuKernelConfig;
use crate::ir::vir::{VirFunction, VirModule};
use crate::toolchain::config::GpuTarget;

/// MLIR Backend Configuration
#[derive(Debug, Clone)]
pub struct MlirConfig {
    /// Target GPU if available
    pub enable_gpu: bool,
    /// GPU target selection
    pub gpu_target: GpuTarget,
    /// GPU kernel launch configuration
    pub gpu_kernel_config: GpuKernelConfig,
    /// Optimization level
    pub opt_level: u8,
    /// Enable SIMD vectorization
    pub enable_vector: bool,
    /// Target triple (e.g., "x86_64-unknown-linux-gnu")
    pub target_triple: Option<String>,
}

impl Default for MlirConfig {
    fn default() -> Self {
        Self {
            enable_gpu: false,
            gpu_target: GpuTarget::Auto,
            gpu_kernel_config: GpuKernelConfig::default(),
            opt_level: 2,
            enable_vector: true,
            target_triple: None,
        }
    }
}

/// MLIR Backend
///
/// Lowers VIR to MLIR and optionally compiles to GPU or LLVM targets.
pub struct MlirBackend {
    config: MlirConfig,
}

impl MlirBackend {
    /// Create new MLIR backend with default configuration
    pub fn new() -> Self {
        Self {
            config: MlirConfig::default(),
        }
    }

    /// Create new MLIR backend with custom configuration
    pub fn with_config(config: MlirConfig) -> Self {
        Self { config }
    }

    /// Check if GPU is available
    pub fn is_gpu_available(&self) -> bool {
        gpu::is_gpu_available()
    }

    /// Get optimization level
    pub fn opt_level(&self) -> u8 {
        self.config.opt_level
    }
}

impl Default for MlirBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl VirBackend for MlirBackend {
    type CompiledModule = MlirCompiledModule;
    type CompiledFunction = MlirCompiledFunction;
    type RuntimeValue = Vec<u8>; // MLIR uses byte arrays for runtime values

    fn compile_module(&mut self, module: &VirModule) -> BackendResult<Self::CompiledModule> {
        // Lower VIR to MLIR
        let mlir_module = lowering::lower_module(module, &self.config)?;

        Ok(MlirCompiledModule {
            mlir_module,
            config: self.config.clone(),
        })
    }

    fn compile_function(
        &mut self,
        function: &VirFunction,
    ) -> BackendResult<Self::CompiledFunction> {
        // Lower single function to MLIR
        let mlir_func = lowering::lower_function(function, &self.config)?;

        Ok(MlirCompiledFunction {
            mlir_func,
            config: self.config.clone(),
        })
    }

    fn execute_function(
        &mut self,
        _compiled: &Self::CompiledFunction,
        _args: &[Self::RuntimeValue],
    ) -> BackendResult<Self::RuntimeValue> {
        // MLIR execution requires the external MLIR toolchain
        // (mlir-opt → mlir-translate → llc → clang) via the pipeline module.
        // Direct VIR interpretation is not used because the VIR → Interpreter
        // lowering path has known gaps (aggregate ops, string constants, etc.).
        // Use the AOT pipeline or the interpreter/cranelift backend for execution.
        Err(BackendError::NotImplemented(
            "MLIR direct execution requires the external MLIR toolchain. \
             Use the AOT pipeline (pipeline::MlirPipeline) or another backend for execution."
                .to_string(),
        ))
    }

    fn name(&self) -> &str {
        "MLIR"
    }

    fn supports_feature(&self, feature: &str) -> bool {
        match feature {
            "gpu" => self.config.enable_gpu && self.is_gpu_available(),
            "vector" => self.config.enable_vector,
            "llvm" => true, // MLIR can lower to LLVM
            _ => false,
        }
    }
}

/// Compiled MLIR Module
pub struct MlirCompiledModule {
    mlir_module: String, // TODO: Replace with actual MLIR module type
    config: MlirConfig,
}

impl MlirCompiledModule {
    /// Get MLIR IR as string
    pub fn to_mlir_string(&self) -> &str {
        &self.mlir_module
    }

    /// Get configuration
    pub fn config(&self) -> &MlirConfig {
        &self.config
    }
}

/// Compiled MLIR Function
pub struct MlirCompiledFunction {
    mlir_func: String,
    config: MlirConfig,
}

impl MlirCompiledFunction {
    /// Get MLIR IR as string
    pub fn to_mlir_string(&self) -> &str {
        &self.mlir_func
    }

    /// Get configuration
    pub fn config(&self) -> &MlirConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mlir_backend_creation() {
        let backend = MlirBackend::new();
        assert_eq!(backend.opt_level(), 2);
    }

    #[test]
    fn test_mlir_config_default() {
        let config = MlirConfig::default();
        assert!(!config.enable_gpu);
        assert_eq!(config.opt_level, 2);
        assert!(config.enable_vector);
    }
}
