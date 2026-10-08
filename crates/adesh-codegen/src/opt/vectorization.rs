//! Auto-vectorization pass shell.
//!
//! The current MachineIR does not yet represent packed scalar lanes or loop
//! remainder semantics. This pass deliberately makes no transformations
//! rather than relying on the old unsound scalar-to-vector substitution.

use crate::error::CodegenError;
use crate::machine_ir::MachineFunction;
use crate::opt::pass::MachinePass;
use crate::opt::vector_prep::{CpuFeatures, VectorCostModel};

/// Conservative Auto-Vectorization Pass.
pub struct AutoVectorizePass {
    pub cost_model: VectorCostModel,
}

impl AutoVectorizePass {
    pub fn new(features: CpuFeatures) -> Self {
        Self {
            cost_model: VectorCostModel::new(features),
        }
    }
}

impl MachinePass for AutoVectorizePass {
    fn name(&self) -> &'static str {
        "auto_vectorize"
    }

    fn run_on_function(&mut self, func: &mut MachineFunction) -> Result<bool, CodegenError> {
        // The previous prototype replaced scalar Add/FAdd instructions with
        // vector ops in-place. That does not create lane values, prove loop
        // independence, handle remainders, or preserve scalar register
        // classes, so it could silently miscompile. Keep the pass fail-closed
        // until vector values and a legality-checked loop transformation are
        // represented in MachineIR.
        let _ = (&self.cost_model, func);
        Ok(false)
    }
}
