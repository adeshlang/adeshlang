//! VIR Backend Adapter
//!
//! Common infrastructure for backends to consume VIR (Value Intermediate Representation).
//! This eliminates duplication by providing a single interface that all backends implement.

use crate::ir::vir::{
    ValueId, VirBlock, VirFunction, VirInstruction, VirModule, VirTerminator, VirType,
};
use std::collections::HashMap;

/// Result type for backend operations
pub type BackendResult<T> = Result<T, BackendError>;

/// Errors that can occur during backend execution
#[derive(Debug, Clone)]
pub enum BackendError {
    /// Unsupported VIR instruction
    UnsupportedInstruction(String),
    /// Type mismatch
    TypeMismatch { expected: String, found: String },
    /// Invalid register reference
    InvalidRegister(String),
    /// Invalid basic block reference
    InvalidBlock(String),
    /// Runtime error during execution
    RuntimeError(String),
    /// Memory allocation failure
    AllocationFailed(String),
    /// Function not found
    FunctionNotFound(String),
    /// Intrinsic not supported
    IntrinsicNotSupported(String),
    /// Feature not yet implemented
    NotImplemented(String),
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            BackendError::UnsupportedInstruction(inst) => {
                write!(f, "Unsupported VIR instruction: {}", inst)
            }
            BackendError::TypeMismatch { expected, found } => {
                write!(f, "Type mismatch: expected {}, found {}", expected, found)
            }
            BackendError::InvalidRegister(reg) => write!(f, "Invalid register: {}", reg),
            BackendError::InvalidBlock(block) => write!(f, "Invalid basic block: {}", block),
            BackendError::RuntimeError(msg) => write!(f, "Runtime error: {}", msg),
            BackendError::AllocationFailed(msg) => write!(f, "Allocation failed: {}", msg),
            BackendError::FunctionNotFound(name) => write!(f, "Function not found: {}", name),
            BackendError::IntrinsicNotSupported(name) => {
                write!(f, "Intrinsic not supported: {}", name)
            }
            BackendError::NotImplemented(msg) => write!(f, "Not implemented: {}", msg),
        }
    }
}

impl std::error::Error for BackendError {}

/// Trait that all VIR-consuming backends must implement
pub trait VirBackend {
    /// The backend's representation of a compiled module
    type CompiledModule;

    /// The backend's representation of a callable function
    type CompiledFunction;

    /// The backend's representation of a runtime value
    type RuntimeValue;

    /// Compile a VIR module into the backend's representation
    fn compile_module(&mut self, module: &VirModule) -> BackendResult<Self::CompiledModule>;

    /// Compile a single VIR function
    fn compile_function(&mut self, function: &VirFunction)
    -> BackendResult<Self::CompiledFunction>;

    /// Execute a compiled function with the given arguments
    fn execute_function(
        &mut self,
        function: &Self::CompiledFunction,
        args: &[Self::RuntimeValue],
    ) -> BackendResult<Self::RuntimeValue>;

    /// Get the name of this backend (for debugging/logging)
    fn name(&self) -> &str;

    /// Check if this backend supports a specific feature
    fn supports_feature(&self, _feature: &str) -> bool {
        // Default implementation: no special features
        false
    }
}

/// Helper for translating VIR types to backend-specific representations
pub struct TypeTranslator {
    #[allow(dead_code)]
    #[allow(dead_code)]
    type_cache: HashMap<String, usize>,
}

impl TypeTranslator {
    pub fn new() -> Self {
        Self {
            type_cache: HashMap::new(),
        }
    }

    /// Get the size of a VIR type in bytes
    pub fn size_of(&self, ty: &VirType) -> usize {
        ty.size_bytes().unwrap_or(8) // Default to pointer size if unknown
    }

    /// Get the alignment requirement for a VIR type
    pub fn align_of(&self, ty: &VirType) -> usize {
        ty.align_bytes().unwrap_or(8) // Default to pointer alignment if unknown
    }
}

impl Default for TypeTranslator {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper for managing intrinsic function lowering
pub struct IntrinsicLowering {
    supported_intrinsics: HashMap<String, IntrinsicHandler>,
}

/// Handler function for an intrinsic
pub type IntrinsicHandler = fn(&str, &[ValueId]) -> BackendResult<ValueId>;

impl IntrinsicLowering {
    pub fn new() -> Self {
        let mut lowering = Self {
            supported_intrinsics: HashMap::new(),
        };

        // Register common intrinsics
        lowering.register_intrinsic("size_of", Self::handle_size_of);
        lowering.register_intrinsic("align_of", Self::handle_align_of);
        lowering.register_intrinsic("offset_of", Self::handle_offset_of);

        lowering
    }

    pub fn register_intrinsic(&mut self, name: &str, handler: IntrinsicHandler) {
        self.supported_intrinsics.insert(name.to_string(), handler);
    }

    pub fn lower_intrinsic(&self, name: &str, args: &[ValueId]) -> BackendResult<ValueId> {
        if let Some(handler) = self.supported_intrinsics.get(name) {
            handler(name, args)
        } else {
            Err(BackendError::IntrinsicNotSupported(name.to_string()))
        }
    }

    fn handle_size_of(_name: &str, _args: &[ValueId]) -> BackendResult<ValueId> {
        // Implementation would depend on backend
        Ok(8) // Placeholder - return a value ID
    }

    fn handle_align_of(_name: &str, _args: &[ValueId]) -> BackendResult<ValueId> {
        // Implementation would depend on backend
        Ok(8) // Placeholder
    }

    fn handle_offset_of(_name: &str, _args: &[ValueId]) -> BackendResult<ValueId> {
        // Implementation would depend on backend
        Ok(0) // Placeholder
    }
}

impl Default for IntrinsicLowering {
    fn default() -> Self {
        Self::new()
    }
}

/// Utility functions for VIR backends
pub mod utils {
    use super::*;

    /// Check if an instruction produces a side effect
    pub fn has_side_effect(inst: &VirInstruction) -> bool {
        matches!(
            inst,
            VirInstruction::Store { .. }
                | VirInstruction::StoreLocal { .. }
                | VirInstruction::Free { .. }
                | VirInstruction::ArcIncrement { .. }
                | VirInstruction::ArcDecrement { .. }
                | VirInstruction::ArcDrop { .. }
                | VirInstruction::Drop { .. }
                | VirInstruction::Call { .. }
                | VirInstruction::Intrinsic { .. }
        )
    }

    /// Extract the target block(s) from a terminator
    pub fn get_terminator_targets(term: &VirTerminator) -> Vec<u32> {
        match term {
            VirTerminator::Jump { target } => vec![*target],
            VirTerminator::Branch {
                true_target,
                false_target,
                ..
            } => vec![*true_target, *false_target],
            VirTerminator::Switch { cases, default, .. } => {
                let mut targets: Vec<u32> = cases.iter().map(|(_, t)| *t).collect();
                targets.push(*default);
                targets
            }
            VirTerminator::Return { .. } | VirTerminator::Unreachable => vec![],
        }
    }

    /// Count the number of instructions in a basic block
    pub fn count_instructions(block: &VirBlock) -> usize {
        block.instructions.len()
    }

    /// Check if a value is used in an instruction
    pub fn instruction_uses_value(inst: &VirInstruction, value_id: &ValueId) -> bool {
        match inst {
            VirInstruction::Load { ptr, .. } => ptr == value_id,
            VirInstruction::Store { ptr, value, .. } => ptr == value_id || value == value_id,
            VirInstruction::IntBinOp { lhs, rhs, .. } => lhs == value_id || rhs == value_id,
            VirInstruction::FloatBinOp { lhs, rhs, .. } => lhs == value_id || rhs == value_id,
            VirInstruction::IntCmp { lhs, rhs, .. } => lhs == value_id || rhs == value_id,
            VirInstruction::FloatCmp { lhs, rhs, .. } => lhs == value_id || rhs == value_id,
            VirInstruction::Call { args, func, .. } => {
                func == value_id || args.iter().any(|arg| arg == value_id)
            }
            VirInstruction::Copy { src, .. } | VirInstruction::Move { src, .. } => src == value_id,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_sizes() {
        let translator = TypeTranslator::new();
        assert_eq!(translator.size_of(&VirType::Bool), 1);
        assert_eq!(translator.size_of(&VirType::I32), 4);
        assert_eq!(translator.size_of(&VirType::I64), 8);
        assert_eq!(translator.size_of(&VirType::F64), 8);
        assert_eq!(
            translator.size_of(&VirType::TypedPtr(Box::new(VirType::I32))),
            8
        );
    }

    #[test]
    fn test_type_alignment() {
        let translator = TypeTranslator::new();
        assert_eq!(translator.align_of(&VirType::Bool), 1);
        assert_eq!(translator.align_of(&VirType::I32), 4);
        assert_eq!(translator.align_of(&VirType::I64), 8);
        assert_eq!(translator.align_of(&VirType::F64), 8);
    }

    #[test]
    fn test_intrinsic_lowering() {
        let lowering = IntrinsicLowering::new();
        assert!(lowering.lower_intrinsic("size_of", &[]).is_ok());
        assert!(lowering.lower_intrinsic("align_of", &[]).is_ok());
        assert!(lowering.lower_intrinsic("unsupported", &[]).is_err());
    }
}
