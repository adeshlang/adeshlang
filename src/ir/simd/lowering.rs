//! SIMD lowering — transforms SIMD IR to backend-specific vector code

use super::instructions::SimdInst;
use super::types::{SimdIsa, SimdType};

/// Target for SIMD lowering
#[derive(Debug, Clone)]
pub struct SimdLoweringTarget {
    pub isa: SimdIsa,
    pub enable_fma: bool,
    pub enable_masked_ops: bool,
}

impl Default for SimdLoweringTarget {
    fn default() -> Self {
        SimdLoweringTarget {
            isa: detect_host_isa(),
            enable_fma: true,
            enable_masked_ops: true,
        }
    }
}

/// Detect best available SIMD ISA on the host
pub fn detect_host_isa() -> SimdIsa {
    #[cfg(target_arch = "x86_64")]
    {
        if std::arch::is_x86_feature_detected!("avx512f") {
            return SimdIsa::Avx512;
        }
        if std::arch::is_x86_feature_detected!("avx2") {
            return SimdIsa::Avx2;
        }
        if std::arch::is_x86_feature_detected!("avx") {
            return SimdIsa::Avx;
        }
        if std::arch::is_x86_feature_detected!("sse4.2") {
            return SimdIsa::Sse42;
        }
        if std::arch::is_x86_feature_detected!("sse2") {
            return SimdIsa::Sse2;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        return SimdIsa::Neon;
    }
    #[allow(unreachable_code)]
    SimdIsa::Scalar
}

/// Lower a single SIMD instruction to backend-specific representation
pub struct SimdLowerer {
    pub target: SimdLoweringTarget,
}

impl SimdLowerer {
    pub fn new(target: SimdLoweringTarget) -> Self {
        SimdLowerer { target }
    }

    /// Validate that the ISA supports the requested vector width
    pub fn validate_width(&self, simd_type: &SimdType) -> bool {
        let max = self.target.isa.max_lanes(simd_type.element);
        simd_type.lanes <= max
    }

    /// Lower instructions — returns count of successfully lowered ops
    pub fn lower_block(&self, instructions: &[SimdInst]) -> usize {
        instructions
            .iter()
            .filter(|inst| self.lower_one(inst))
            .count()
    }

    fn lower_one(&self, inst: &SimdInst) -> bool {
        match inst {
            SimdInst::Add(_, _, _)
            | SimdInst::Sub(_, _, _)
            | SimdInst::Mul(_, _, _)
            | SimdInst::Load(_, _, _)
            | SimdInst::Store(_, _)
            | SimdInst::Splat(_, _, _)
            | SimdInst::ReduceAdd(_, _) => self.target.isa != SimdIsa::Scalar,
            _ => self.target.isa != SimdIsa::Scalar,
        }
    }
}
