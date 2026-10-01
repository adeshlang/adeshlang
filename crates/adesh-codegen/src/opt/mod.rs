//! Binary & Machine IR Optimization Pipeline for Adesh.

pub mod array_opt;
pub mod dce;
pub mod jump_table;
pub mod lto;
pub mod peephole;

pub use array_opt::ArrayOptimizer;
pub use dce::DeadCodeElimination;
pub use jump_table::{SwitchCase, SwitchLowering, SwitchStrategy};
pub use lto::{LtoConfig, LtoEngine, LtoMode};
pub use peephole::PeepholeOptimizer;

use crate::machine_ir::{MachineFunction, NativeModule};

/// Complete Optimization Pipeline manager.
pub struct OptimizationPipeline {
    pub opt_level: u8,
    pub peephole: PeepholeOptimizer,
    pub dce: DeadCodeElimination,
    pub array_opt: ArrayOptimizer,
}

impl OptimizationPipeline {
    pub fn new(opt_level: u8) -> Self {
        Self {
            opt_level,
            peephole: PeepholeOptimizer::new(opt_level),
            dce: DeadCodeElimination::new(),
            array_opt: ArrayOptimizer::new(),
        }
    }

    /// Run full optimization pipeline on a native module.
    pub fn optimize_module(&self, module: &mut NativeModule) -> usize {
        if self.opt_level == 0 {
            return 0;
        }

        let mut total_improvements = 0;
        for func in &mut module.functions {
            total_improvements += self.optimize_function(func);
        }
        total_improvements
    }

    /// Run multiple iterative optimization passes on a function until fixpoint or max iterations.
    pub fn optimize_function(&self, func: &mut MachineFunction) -> usize {
        if self.opt_level == 0 {
            return 0;
        }

        let mut total = 0;
        let max_passes = match self.opt_level {
            1 => 2,
            2 => 4,
            _ => 8,
        };

        for _ in 0..max_passes {
            let p_changes = self.peephole.optimize_function(func);
            let d_changes = self.dce.run(func);
            let a_changes = self.array_opt.optimize_function(func);

            let pass_total = p_changes + d_changes + a_changes;
            total += pass_total;

            if pass_total == 0 {
                break; // Converged
            }
        }

        total
    }
}
