//! SIMD / Vector Type Representation, Cost Model, and CPU Feature Support.
//!
//! Prepares target-independent vector structures, loop independence checking,
//! and feature detection flags (SSE2, SSE4.2, AVX, AVX2, AVX-512).

use serde::{Deserialize, Serialize};

/// Vector Element Type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VectorElementType {
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
}

impl VectorElementType {
    pub const fn is_floating_point(&self) -> bool {
        matches!(self, VectorElementType::F32 | VectorElementType::F64)
    }

    pub const fn is_integer(&self) -> bool {
        !self.is_floating_point()
    }
}

/// Target-Independent Vector Type (e.g. `<4 x f32>`, `<8 x i32>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VectorType {
    pub element_type: VectorElementType,
    pub lanes: u16,
}

impl VectorType {
    pub const fn v16i8() -> Self {
        Self {
            element_type: VectorElementType::I8,
            lanes: 16,
        }
    }

    pub const fn v8i16() -> Self {
        Self {
            element_type: VectorElementType::I16,
            lanes: 8,
        }
    }

    pub const fn v4i32() -> Self {
        Self {
            element_type: VectorElementType::I32,
            lanes: 4,
        }
    }

    pub const fn v8i32() -> Self {
        Self {
            element_type: VectorElementType::I32,
            lanes: 8,
        }
    }

    pub const fn v2i64() -> Self {
        Self {
            element_type: VectorElementType::I64,
            lanes: 2,
        }
    }

    pub const fn v4f32() -> Self {
        Self {
            element_type: VectorElementType::F32,
            lanes: 4,
        }
    }

    pub const fn v8f32() -> Self {
        Self {
            element_type: VectorElementType::F32,
            lanes: 8,
        }
    }

    pub const fn v2f64() -> Self {
        Self {
            element_type: VectorElementType::F64,
            lanes: 2,
        }
    }

    pub const fn v4f64() -> Self {
        Self {
            element_type: VectorElementType::F64,
            lanes: 4,
        }
    }

    pub fn total_bits(&self) -> usize {
        let elem_bits = match self.element_type {
            VectorElementType::I8 => 8,
            VectorElementType::I16 => 16,
            VectorElementType::I32 | VectorElementType::F32 => 32,
            VectorElementType::I64 | VectorElementType::F64 => 64,
        };
        elem_bits * self.lanes as usize
    }

    pub fn total_bytes(&self) -> usize {
        self.total_bits().div_ceil(8)
    }

    pub fn alignment(&self) -> usize {
        match self.total_bits() {
            64 => 8,
            128 => 16,
            256 => 32,
            512 => 64,
            _ => 16,
        }
    }

    pub fn is_floating_point(&self) -> bool {
        matches!(
            self.element_type,
            VectorElementType::F32 | VectorElementType::F64
        )
    }

    pub fn is_integer(&self) -> bool {
        !self.is_floating_point()
    }
}

/// CPU Feature Flags for Vector Operations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CpuFeatures {
    pub has_sse2: bool,
    pub has_sse42: bool,
    pub has_avx: bool,
    pub has_avx2: bool,
    pub has_avx512: bool,
    pub has_neon: bool,
}

impl CpuFeatures {
    pub fn x86_64_baseline() -> Self {
        Self {
            has_sse2: true,
            has_sse42: false,
            has_avx: false,
            has_avx2: false,
            has_avx512: false,
            has_neon: false,
        }
    }

    pub fn x86_64_modern() -> Self {
        Self {
            has_sse2: true,
            has_sse42: true,
            has_avx: true,
            has_avx2: true,
            has_avx512: false,
            has_neon: false,
        }
    }
}

/// Vector Cost Model for Auto-Vectorization Profitability Analysis.
#[derive(Debug, Clone)]
pub struct VectorCostModel {
    pub features: CpuFeatures,
}

impl VectorCostModel {
    pub fn new(features: CpuFeatures) -> Self {
        Self { features }
    }

    /// Estimate cost reduction ratio for vectorizing a loop with the given vector type.
    pub fn estimated_speedup(&self, vec_ty: VectorType) -> f64 {
        if vec_ty.total_bits() == 128 && self.features.has_sse2 {
            (vec_ty.lanes as f64) * 0.75
        } else if vec_ty.total_bits() == 256 && self.features.has_avx2 {
            (vec_ty.lanes as f64) * 0.85
        } else {
            1.0 // No vectorization advantage without hardware support
        }
    }
}
