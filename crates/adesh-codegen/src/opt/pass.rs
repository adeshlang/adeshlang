//! Machine Optimization Pass Abstractions and Pass Manager.

use crate::error::CodegenError;
use crate::machine_ir::{MachineFunction, NativeModule};
use std::collections::HashMap;

/// Result of running an optimization pass.
#[derive(Debug, Clone, Default)]
pub struct PassStats {
    pub pass_name: String,
    pub instructions_before: usize,
    pub instructions_after: usize,
    pub changes_made: usize,
}

/// Abstract machine-level optimization pass operating on a MachineFunction.
pub trait MachinePass: Send + Sync {
    /// Name of the optimization pass for diagnostics and logging.
    fn name(&self) -> &'static str;

    /// Run the optimization on a single MachineFunction.
    /// Returns Ok(true) if changes were made, Ok(false) if no changes, or Err on invalid state.
    fn run_on_function(&mut self, func: &mut MachineFunction) -> Result<bool, CodegenError>;
}

/// Abstract module-level optimization pass operating on a NativeModule.
pub trait ModulePass: Send + Sync {
    /// Name of the optimization pass.
    fn name(&self) -> &'static str;

    /// Run the optimization on the entire NativeModule.
    fn run_on_module(&mut self, module: &mut NativeModule) -> Result<bool, CodegenError>;
}

/// Comprehensive Optimization Statistics Report.
#[derive(Debug, Clone, Default)]
pub struct OptimizationReport {
    pub function_reports: HashMap<String, Vec<PassStats>>,
    pub total_instructions_before: usize,
    pub total_instructions_after: usize,
    pub total_moves_eliminated: usize,
    pub total_spills_reduced: usize,
}

impl OptimizationReport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_pass(&mut self, func_name: &str, stats: PassStats) {
        self.function_reports
            .entry(func_name.to_string())
            .or_default()
            .push(stats);
    }
}
