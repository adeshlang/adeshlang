//! LLVM Backend Module - Phase 7.3
//!
//! Direct VIR to LLVM IR lowering with JIT and AOT compilation support

pub mod lowering;

pub use lowering::{
    LLVMBasicBlock, LLVMFunctionDef, LLVMInstr, LLVMType, LLVMValue, VirToLLVMLowerer,
};

/// LLVM backend configuration
pub struct LLVMBackendConfig {
    /// Optimization level (0-3)
    pub opt_level: usize,

    /// Target triple (e.g., "x86_64-unknown-linux-gnu")
    pub target_triple: String,

    /// Enable LTO (Link Time Optimization)
    pub enable_lto: bool,

    /// Enable SIMD vectorization
    pub enable_simd: bool,
}

impl Default for LLVMBackendConfig {
    fn default() -> Self {
        Self {
            opt_level: 2,
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            enable_lto: false,
            enable_simd: true,
        }
    }
}

/// LLVM code generator
pub struct LLVMCodegen {
    config: LLVMBackendConfig,
    lowerer: VirToLLVMLowerer,
}

impl LLVMCodegen {
    /// Create a new code generator
    pub fn new(module_name: String, config: LLVMBackendConfig) -> Self {
        Self {
            config,
            lowerer: VirToLLVMLowerer::new(module_name),
        }
    }

    /// Get configuration
    pub fn config(&self) -> &LLVMBackendConfig {
        &self.config
    }

    /// Get lowerer
    pub fn lowerer(&self) -> &VirToLLVMLowerer {
        &self.lowerer
    }

    /// Get mutable lowerer
    pub fn lowerer_mut(&mut self) -> &mut VirToLLVMLowerer {
        &mut self.lowerer
    }

    /// Generate LLVM IR
    pub fn generate_ir(&self) -> String {
        self.lowerer.to_ir_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_config_default() {
        let config = LLVMBackendConfig::default();
        assert_eq!(config.opt_level, 2);
        assert_eq!(config.target_triple, "x86_64-unknown-linux-gnu");
        assert!(!config.enable_lto);
        assert!(config.enable_simd);
    }

    #[test]
    fn test_codegen_new() {
        let codegen = LLVMCodegen::new("test".to_string(), LLVMBackendConfig::default());
        assert!(!codegen.generate_ir().is_empty());
    }
}
