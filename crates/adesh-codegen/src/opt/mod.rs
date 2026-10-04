//! Binary & Machine IR Optimization Pipeline for Adesh.

pub mod alias_analysis;
pub mod array_opt;
pub mod branch_opt;
pub mod cmov;
pub mod coalescing;
pub mod constant_folding;
pub mod copy_prop;
pub mod cse;
pub mod dce;
pub mod dominance;
pub mod frame_opt;
pub mod gvn;
pub mod induction;
pub mod licm;
pub mod loop_analysis;
pub mod lto;
pub mod memory_effects;
pub mod ipo;
pub mod loop_opt;
pub mod opt_level;
pub mod pass;
pub mod peephole;
pub mod pgo;
pub mod sccp;
pub mod scheduler;
pub mod scheduler_v2;
pub mod strength_reduction;
pub mod switch_lowering;
pub mod tail_call;
pub mod vector_dispatch;
pub mod vector_prep;
pub mod vectorization;
pub mod verifier;

pub use alias_analysis::{
    AliasAnalysis, AliasResult, DeadStoreEliminationPass, LoadStoreForwardingPass,
};
pub use array_opt::ArrayOptimizer;
pub use branch_opt::BranchOptimizationPass;
pub use cmov::ConditionalMovePass;
pub use coalescing::MoveCoalescingPass;
pub use constant_folding::ConstantFoldingPass;
pub use copy_prop::CopyPropagationPass;
pub use cse::LocalCSEPass;
pub use dce::DeadCodeElimination;
pub use dominance::DominatorTree;
pub use frame_opt::StackFrameOptimizationPass;
pub use gvn::GVNPass;
pub use induction::InductionVariablePass;
pub use ipo::{IpoConfig, IpoEngine, IpoReport};
pub use licm::LICMPass;
pub use loop_analysis::{LoopInfo, NaturalLoop};
pub use loop_opt::{LoopOptConfig, LoopOptReport, LoopOptimizer};
pub use lto::{FunctionSummary, LtoConfig, LtoEngine, LtoMode, LtoReport, ModuleSummary};
pub use memory_effects::MemoryEffect;
pub use opt_level::OptLevel;
pub use pass::{MachinePass, ModulePass, OptimizationReport, PassStats};
pub use peephole::PeepholeOptimizer;
pub use pgo::{
    BlockProfile, EdgeProfile, FunctionProfile, PgoInstrumentationPass, PgoOptimizationPass,
    ProfileData,
};
pub use sccp::SCCPPass;
pub use scheduler::BasicBlockScheduler;
pub use scheduler_v2::{MachineSchedulerV2, SchedulerV2Config};
pub use strength_reduction::StrengthReductionPass;
pub use switch_lowering::{SwitchCase, SwitchLowering, SwitchStrategy};
pub use tail_call::TailCallOptimizationPass;
pub use vector_dispatch::{SimdArchitecture, SimdDispatcher, VectorReduction, VectorReductionKind};
pub use vector_prep::{CpuFeatures, VectorCostModel, VectorElementType, VectorType};
pub use vectorization::AutoVectorizePass;
pub use verifier::MachineIRVerifier;

pub use switch_lowering as jump_table;

use crate::error::CodegenError;
use crate::machine_ir::{MachineFunction, NativeModule};

/// Complete Production Optimization Pipeline Manager.
pub struct OptimizationPipeline {
    pub opt_level: OptLevel,
    pub const_folding: ConstantFoldingPass,
    pub sccp: SCCPPass,
    pub copy_prop: CopyPropagationPass,
    pub cse: LocalCSEPass,
    pub gvn: GVNPass,
    pub dse: DeadStoreEliminationPass,
    pub load_forwarding: LoadStoreForwardingPass,
    pub strength_red: StrengthReductionPass,
    pub licm: LICMPass,
    pub induction: InductionVariablePass,
    pub branch_opt: BranchOptimizationPass,
    pub coalescing: MoveCoalescingPass,
    pub tail_call: TailCallOptimizationPass,
    pub cmov: ConditionalMovePass,
    pub frame_opt: StackFrameOptimizationPass,
    pub peephole: PeepholeOptimizer,
    pub dce: DeadCodeElimination,
    pub array_opt: ArrayOptimizer,
}

impl OptimizationPipeline {
    pub fn new(opt_level: OptLevel) -> Self {
        let level_u8 = match opt_level {
            OptLevel::O0 => 0,
            OptLevel::O1 => 1,
            _ => 2,
        };
        Self {
            opt_level,
            const_folding: ConstantFoldingPass::new(),
            sccp: SCCPPass::new(),
            copy_prop: CopyPropagationPass::new(),
            cse: LocalCSEPass::new(),
            gvn: GVNPass::new(),
            dse: DeadStoreEliminationPass::new(),
            load_forwarding: LoadStoreForwardingPass::new(),
            strength_red: StrengthReductionPass::new(),
            licm: LICMPass::new(),
            induction: InductionVariablePass::new(),
            branch_opt: BranchOptimizationPass::new(),
            coalescing: MoveCoalescingPass::new(),
            tail_call: TailCallOptimizationPass::new(),
            cmov: ConditionalMovePass::new(),
            frame_opt: StackFrameOptimizationPass::new(),
            peephole: PeepholeOptimizer::new(level_u8),
            dce: DeadCodeElimination::new(),
            array_opt: ArrayOptimizer::new(),
        }
    }

    /// Run full pre-allocation optimization pipeline on a NativeModule.
    pub fn optimize_module_pre_alloc(
        &mut self,
        module: &mut NativeModule,
    ) -> Result<usize, CodegenError> {
        if !self.opt_level.is_optimized() {
            return Ok(0);
        }

        let mut total_improvements = 0;
        for func in &mut module.functions {
            total_improvements += self.optimize_function_pre_alloc(func)?;
        }
        Ok(total_improvements)
    }

    /// Run pre-allocation optimization passes on a function.
    pub fn optimize_function_pre_alloc(
        &mut self,
        func: &mut MachineFunction,
    ) -> Result<usize, CodegenError> {
        if !self.opt_level.is_optimized() {
            return Ok(0);
        }

        let mut total = 0;
        let max_passes = self.opt_level.max_pass_iterations();

        for _ in 0..max_passes {
            let mut pass_total = 0;

            // 1. Constant folding & propagation
            if self.const_folding.run_on_function(func)? {
                pass_total += 1;
            }

            // 2. Sparse Conditional Constant Propagation (SCCP)
            if self.sccp.run_on_function(func)? {
                pass_total += 1;
            }

            // 3. Copy propagation
            if self.copy_prop.run_on_function(func)? {
                pass_total += 1;
            }

            // 4. Load-store forwarding
            if self.load_forwarding.run_on_function(func)? {
                pass_total += 1;
            }

            // 5. Dead-store elimination
            if self.dse.run_on_function(func)? {
                pass_total += 1;
            }

            // 6. Local CSE & Global Value Numbering (GVN)
            if self.cse.run_on_function(func)? {
                pass_total += 1;
            }
            if self.opt_level.is_aggressive() && self.gvn.run_on_function(func)? {
                pass_total += 1;
            }

            // 7. Strength reduction & algebraic identities
            if self.strength_red.run_on_function(func)? {
                pass_total += 1;
            }

            // 8. Loop Invariant Code Motion (LICM)
            if self.opt_level.is_aggressive() && self.licm.run_on_function(func)? {
                pass_total += 1;
            }

            // 9. Induction Variable Analysis & Loop Reduction
            if self.opt_level.is_aggressive() && self.induction.run_on_function(func)? {
                pass_total += 1;
            }

            // 10. Branch optimization & jump threading
            if self.branch_opt.run_on_function(func)? {
                pass_total += 1;
            }

            // 11. Conditional move optimization
            if self.cmov.run_on_function(func)? {
                pass_total += 1;
            }

            // 12. Tail call optimization
            if self.opt_level.is_aggressive() && self.tail_call.run_on_function(func)? {
                pass_total += 1;
            }

            // 13. Array optimizations & bounds check elimination
            pass_total += self.array_opt.optimize_function(func);

            // 14. Dead code elimination
            pass_total += self.dce.run(func);

            // 15. Move coalescing
            if self.opt_level.is_aggressive() && self.coalescing.run_on_function(func)? {
                pass_total += 1;
            }

            total += pass_total;
            if pass_total == 0 {
                break; // Converged
            }
        }

        MachineIRVerifier::verify(func)?;
        Ok(total)
    }

    /// Run post-allocation optimization passes on a function (peephole, frame minimization).
    pub fn optimize_function_post_alloc(
        &mut self,
        func: &mut MachineFunction,
    ) -> Result<usize, CodegenError> {
        if !self.opt_level.is_optimized() {
            return Ok(0);
        }

        let mut total = 0;
        total += self.peephole.optimize_function(func);
        if self.frame_opt.run_on_function(func)? {
            total += 1;
        }

        MachineIRVerifier::verify(func)?;
        Ok(total)
    }
}
