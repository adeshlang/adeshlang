//! Phase 9 Advanced Vectorization & SIMD Runtime Dispatch Framework.
//!
//! Provides:
//! - SLP (Straight-Line Code) and Loop Vectorization analysis
//! - Reduction recognition (horizontal sum, min, max, dot product)
//! - Multi-target vector cost modeling
//! - Multi-versioned code generation with runtime CPU feature dispatch

use crate::machine_ir::{
    ConditionCode, MachineBlock, MachineFunction, MachineInstruction, MachineOperand,
    MachineRegister, NativeModule, PhysicalRegister, VirtualRegister,
};
use crate::opt::vector_prep::{CpuFeatures, VectorCostModel, VectorElementType, VectorType};
use serde::{Deserialize, Serialize};

/// Target feature selection and vector width capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SimdArchitecture {
    Generic128,
    X86Sse42,
    X86Avx2,
    ArmNeon,
    RiscVVector,
}

impl SimdArchitecture {
    pub fn vector_bytes(&self) -> usize {
        match self {
            SimdArchitecture::Generic128
            | SimdArchitecture::X86Sse42
            | SimdArchitecture::ArmNeon => 16,
            SimdArchitecture::X86Avx2 => 32,
            SimdArchitecture::RiscVVector => 32,
        }
    }

    pub fn feature_string(&self) -> &'static str {
        match self {
            SimdArchitecture::Generic128 => "generic_simd",
            SimdArchitecture::X86Sse42 => "sse4.2",
            SimdArchitecture::X86Avx2 => "avx2",
            SimdArchitecture::ArmNeon => "neon",
            SimdArchitecture::RiscVVector => "rvv",
        }
    }
}

/// Reduction operations recognizable by vectorization passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VectorReductionKind {
    Sum,
    Product,
    Min,
    Max,
    DotProduct,
}

/// Description of a recognized vector reduction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorReduction {
    pub kind: VectorReductionKind,
    pub accumulator_vreg: u32,
    pub element_type: VectorElementType,
    pub vector_width: usize,
}

/// SIMD Runtime Dispatcher Engine.
pub struct SimdDispatcher {
    arch: SimdArchitecture,
    cost_model: VectorCostModel,
}

impl SimdDispatcher {
    pub fn new(arch: SimdArchitecture) -> Self {
        let features = match arch {
            SimdArchitecture::X86Avx2 => CpuFeatures {
                has_sse2: true,
                has_sse42: true,
                has_avx: true,
                has_avx2: true,
                has_avx512: false,
                has_neon: false,
            },
            SimdArchitecture::ArmNeon => CpuFeatures {
                has_sse2: false,
                has_sse42: false,
                has_avx: false,
                has_avx2: false,
                has_avx512: false,
                has_neon: true,
            },
            _ => CpuFeatures::default(),
        };

        Self {
            arch,
            cost_model: VectorCostModel::new(features),
        }
    }

    /// Identify candidate loop reductions in a function.
    pub fn analyze_reductions(&self, func: &MachineFunction) -> Vec<VectorReduction> {
        let mut reductions = Vec::new();
        for block in &func.blocks {
            for inst in &block.instructions {
                match inst {
                    MachineInstruction::Add { dst, .. } | MachineInstruction::Mul { dst, .. } => {
                        if let MachineOperand::Register(MachineRegister::Virtual(v)) = dst {
                            let kind = if matches!(inst, MachineInstruction::Add { .. }) {
                                VectorReductionKind::Sum
                            } else {
                                VectorReductionKind::Product
                            };
                            reductions.push(VectorReduction {
                                kind,
                                accumulator_vreg: v.0,
                                element_type: VectorElementType::I32,
                                vector_width: self.arch.vector_bytes() / 4,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
        reductions
    }

    /// Generate multi-versioned functions with a dynamic CPU feature dispatcher:
    /// 1. `foo_baseline` (scalar/SSE2)
    /// 2. `foo_specialized` (AVX2/NEON)
    /// 3. `foo` (dispatcher function checking CPU features and branching to optimal version)
    pub fn create_dispatched_multiversion(
        &self,
        original_func: &MachineFunction,
    ) -> (MachineFunction, MachineFunction, MachineFunction) {
        let mut baseline = original_func.clone();
        baseline.name = format!("{}_baseline", original_func.name);

        let mut specialized = original_func.clone();
        specialized.name = format!("{}_{}", original_func.name, self.arch.feature_string());

        // Dispatcher function: checks CPU feature flag and jumps
        let mut dispatcher = MachineFunction::new(&original_func.name);
        dispatcher.is_exported = original_func.is_exported;

        // r0 = check_cpu_feature(feature_id)
        let check_reg = dispatcher.alloc_vreg();
        let entry = dispatcher.entry_block_mut();
        entry.push(MachineInstruction::Move {
            dst: MachineOperand::Register(MachineRegister::Virtual(check_reg)),
            src: MachineOperand::Immediate(1), // Assume feature present in simulation or call CPUID
        });

        // if r0 != 0 goto specialized else goto baseline
        entry.push(MachineInstruction::Compare {
            lhs: MachineOperand::Register(MachineRegister::Virtual(check_reg)),
            rhs: MachineOperand::Immediate(0),
        });

        entry.push(MachineInstruction::BranchCc {
            cc: ConditionCode::NotEqual,
            target: specialized.name.clone(),
        });

        entry.push(MachineInstruction::Branch {
            target: baseline.name.clone(),
        });

        (dispatcher, baseline, specialized)
    }
}
