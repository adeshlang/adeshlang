//! SIMD Intermediate Representation
//!
//! Architecture-neutral SIMD IR layer for AdeshLang.
//! Lowered to native vector instructions via Cranelift/LLVM backends.
//!
//! ```text
//! AdeshLang SIMD
//!       │
//!       ├── generic SIMD IR (this module)
//!       │
//!       ├── x86 backend (SSE/AVX/AVX2/AVX-512)
//!       ├── ARM backend (NEON/SVE)
//!       └── future architectures
//! ```

pub mod analysis;
pub mod cost_model;
pub mod instructions;
pub mod lowering;
pub mod types;
pub mod vectorize;

pub use analysis::*;
pub use cost_model::*;
pub use instructions::*;
pub use lowering::*;
pub use types::*;
pub use vectorize::*;
