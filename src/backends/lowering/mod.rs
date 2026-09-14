//! Direct VIR lowering to various backend targets.
//!
//! This module provides direct lowering from VIR (Value IR) to specific
//! backend representations, bypassing the intermediate LIR layer.
//!
//! # Phase 2.4.1: Infrastructure Implementation
//!
//! This is the foundational implementation for Phase 2.4. It provides:
//! - Trait definitions for lowering
//! - Error types
//! - Statistics tracking
//! - Basic lowering infrastructure for each backend
//!
//! Future phases (2.4.2-2.4.5) will complete the implementation and integrate
//! with existing backends.

pub mod vir_to_bytecode;
pub mod vir_to_cranelift;
pub mod vir_to_interpreter;

// Phase 7 executors
pub mod bytecode_executor;
pub mod interpreter_executor;

pub use bytecode_executor::{BytecodeInstr, BytecodeVM};
pub use interpreter_executor::{ExecutionContext, InterpreterExecutor, InterpreterValue};
pub use vir_to_bytecode::VirToBytecode;
pub use vir_to_cranelift::VirToCranelift;
pub use vir_to_interpreter::VirToInterpreter;

/// Common result type for lowering operations.
pub type LoweringResult<T> = Result<T, LoweringError>;

/// Errors that can occur during VIR lowering.
#[derive(Debug, Clone)]
pub enum LoweringError {
    /// Unsupported VIR instruction for this backend.
    UnsupportedInstruction(String),

    /// Invalid VIR structure (e.g., malformed CFG).
    InvalidStructure(String),

    /// Type conversion error.
    TypeConversion(String),

    /// Value not found in mapping.
    ValueNotFound(String),

    /// Block not found.
    BlockNotFound(String),

    /// Function not found.
    FunctionNotFound(String),

    /// Backend-specific error.
    BackendError(String),
}

impl std::fmt::Display for LoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedInstruction(msg) => write!(f, "Unsupported instruction: {}", msg),
            Self::InvalidStructure(msg) => write!(f, "Invalid structure: {}", msg),
            Self::TypeConversion(msg) => write!(f, "Type conversion error: {}", msg),
            Self::ValueNotFound(msg) => write!(f, "Value not found: {}", msg),
            Self::BlockNotFound(msg) => write!(f, "Block not found: {}", msg),
            Self::FunctionNotFound(msg) => write!(f, "Function not found: {}", msg),
            Self::BackendError(msg) => write!(f, "Backend error: {}", msg),
        }
    }
}

impl std::error::Error for LoweringError {}

/// Statistics about the lowering process.
#[derive(Debug, Clone, Default)]
pub struct LoweringStats {
    /// Number of functions lowered.
    pub functions_lowered: usize,

    /// Number of basic blocks lowered.
    pub blocks_lowered: usize,

    /// Number of instructions lowered.
    pub instructions_lowered: usize,

    /// Number of values created.
    pub values_created: usize,

    /// Time spent lowering (in milliseconds).
    pub time_ms: u64,
}

impl LoweringStats {
    /// Create new empty stats.
    pub fn new() -> Self {
        Self::default()
    }

    /// Merge with another stats object.
    pub fn merge(&mut self, other: &LoweringStats) {
        self.functions_lowered += other.functions_lowered;
        self.blocks_lowered += other.blocks_lowered;
        self.instructions_lowered += other.instructions_lowered;
        self.values_created += other.values_created;
        self.time_ms += other.time_ms;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lowering_error_display() {
        let err = LoweringError::UnsupportedInstruction("test".to_string());
        assert!(err.to_string().contains("Unsupported instruction"));
    }

    #[test]
    fn test_lowering_stats_merge() {
        let mut stats1 = LoweringStats {
            functions_lowered: 1,
            blocks_lowered: 5,
            instructions_lowered: 20,
            values_created: 30,
            time_ms: 100,
        };

        let stats2 = LoweringStats {
            functions_lowered: 2,
            blocks_lowered: 10,
            instructions_lowered: 40,
            values_created: 60,
            time_ms: 200,
        };

        stats1.merge(&stats2);

        assert_eq!(stats1.functions_lowered, 3);
        assert_eq!(stats1.blocks_lowered, 15);
        assert_eq!(stats1.instructions_lowered, 60);
        assert_eq!(stats1.values_created, 90);
        assert_eq!(stats1.time_ms, 300);
    }
}
