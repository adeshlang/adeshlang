//! VIR Optimization Framework
//!
//! Centralized optimization pipeline that operates on VIR before backend lowering.
//! This eliminates duplication of optimization logic across backends.

pub mod constant_folding;
pub mod constant_propagation;
pub mod cse;
pub mod dead_code;
pub mod inlining;
pub mod loop_opt;

use crate::ir::vir::VirModule;

/// Optimization level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OptLevel {
    /// O0: No optimizations (Debug/Interpreter)
    None,

    /// O1: Basic optimizations (Bytecode VM)
    Basic,

    /// O2: Aggressive optimizations (Native JIT)
    Aggressive,

    /// O3: Maximum optimizations (AOT)
    Maximum,
}

/// Result type for optimizations
pub type OptResult<T> = Result<T, OptError>;

/// Optimization errors
#[derive(Debug, Clone)]
pub enum OptError {
    /// Optimization would break correctness
    InvalidTransform(String),

    /// Internal error
    Internal(String),
}

impl std::fmt::Display for OptError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            OptError::InvalidTransform(msg) => write!(f, "Invalid transformation: {}", msg),
            OptError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for OptError {}

/// Trait for VIR optimizations
pub trait VirOptimization {
    /// Get the name of this optimization
    fn name(&self) -> &str;

    /// Apply this optimization to a module
    fn apply(&self, module: &mut VirModule) -> OptResult<bool>;

    /// Check if this optimization should run at the given level
    fn enabled_at(&self, level: OptLevel) -> bool;
}

/// Optimization pipeline
pub struct OptimizationPipeline {
    passes: Vec<Box<dyn VirOptimization>>,
    level: OptLevel,
}

impl OptimizationPipeline {
    /// Create a new optimization pipeline
    pub fn new(level: OptLevel) -> Self {
        let mut pipeline = Self {
            passes: Vec::new(),
            level,
        };

        // Register passes in order
        pipeline.add_pass(Box::new(constant_folding::ConstantFolding::new()));
        pipeline.add_pass(Box::new(constant_propagation::ConstantPropagation::new()));
        pipeline.add_pass(Box::new(loop_opt::LoopOptimization::new()));
        pipeline.add_pass(Box::new(cse::CommonSubexpressionElimination::new()));
        pipeline.add_pass(Box::new(dead_code::DeadCodeElimination::new()));

        if level >= OptLevel::Aggressive {
            pipeline.add_pass(Box::new(inlining::Inlining::new()));
        }

        pipeline
    }

    /// Add an optimization pass
    pub fn add_pass(&mut self, pass: Box<dyn VirOptimization>) {
        self.passes.push(pass);
    }

    /// Run the optimization pipeline on a module
    pub fn optimize(&self, module: &mut VirModule) -> OptResult<()> {
        let mut changed = true;
        let mut iterations = 0;
        const MAX_ITERATIONS: usize = 10;

        // Iterate until fixpoint or max iterations
        while changed && iterations < MAX_ITERATIONS {
            changed = false;

            for pass in &self.passes {
                if pass.enabled_at(self.level) {
                    let pass_changed = pass.apply(module)?;
                    changed |= pass_changed;
                }
            }

            iterations += 1;
        }

        Ok(())
    }

    /// Get the optimization level
    pub fn level(&self) -> OptLevel {
        self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opt_level_ordering() {
        assert!(OptLevel::None < OptLevel::Basic);
        assert!(OptLevel::Basic < OptLevel::Aggressive);
        assert!(OptLevel::Aggressive < OptLevel::Maximum);
    }

    #[test]
    fn test_pipeline_creation() {
        let pipeline = OptimizationPipeline::new(OptLevel::None);
        assert_eq!(pipeline.level(), OptLevel::None);

        let pipeline = OptimizationPipeline::new(OptLevel::Aggressive);
        assert_eq!(pipeline.level(), OptLevel::Aggressive);
    }
}
