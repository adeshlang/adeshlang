//! CPU feature detection for runtime SIMD dispatch

use crate::ir::simd::types::SimdIsa;

/// Detected CPU capabilities
#[derive(Debug, Clone)]
pub struct CpuFeatures {
    pub isa: SimdIsa,
    pub has_sse2: bool,
    pub has_sse42: bool,
    pub has_avx: bool,
    pub has_avx2: bool,
    pub has_avx512: bool,
    pub has_neon: bool,
    pub num_cores: usize,
}

impl CpuFeatures {
    pub fn detect() -> Self {
        #[allow(unused_mut)]
        let mut features = CpuFeatures {
            isa: SimdIsa::Scalar,
            has_sse2: false,
            has_sse42: false,
            has_avx: false,
            has_avx2: false,
            has_avx512: false,
            has_neon: false,
            num_cores: crate::runtime::thread::logical_cpu_count().max(1),
        };

        #[cfg(target_arch = "x86_64")]
        {
            features.has_sse2 = std::arch::is_x86_feature_detected!("sse2");
            features.has_sse42 = std::arch::is_x86_feature_detected!("sse4.2");
            features.has_avx = std::arch::is_x86_feature_detected!("avx");
            features.has_avx2 = std::arch::is_x86_feature_detected!("avx2");
            features.has_avx512 = std::arch::is_x86_feature_detected!("avx512f");
            features.isa = if features.has_avx512 {
                SimdIsa::Avx512
            } else if features.has_avx2 {
                SimdIsa::Avx2
            } else if features.has_avx {
                SimdIsa::Avx
            } else if features.has_sse42 {
                SimdIsa::Sse42
            } else if features.has_sse2 {
                SimdIsa::Sse2
            } else {
                SimdIsa::Scalar
            };
        }

        #[cfg(target_arch = "aarch64")]
        {
            features.has_neon = true;
            features.isa = SimdIsa::Neon;
        }

        features
    }

    pub fn simd_lanes_f32(&self) -> usize {
        self.isa.max_lanes(crate::ir::simd::types::SimdElement::F32) as usize
    }

    pub fn simd_lanes_f64(&self) -> usize {
        self.isa.max_lanes(crate::ir::simd::types::SimdElement::F64) as usize
    }
}

/// Global cached CPU features (lazy init, zero cost until first SIMD op)
use std::sync::OnceLock;
static CPU_FEATURES: OnceLock<CpuFeatures> = OnceLock::new();

pub fn cpu_features() -> &'static CpuFeatures {
    CPU_FEATURES.get_or_init(CpuFeatures::detect)
}
