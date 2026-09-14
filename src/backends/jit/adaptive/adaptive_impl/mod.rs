//! Adaptive JIT implementation modules

pub mod frame;
pub mod hidden_class;
pub mod inline_cache;
pub mod profiling;
pub mod speculation;
pub mod tiers;

// Re-export commonly used types
pub use frame::{ControlFlow, JitFrame};
pub use hidden_class::HiddenClassSystem;
pub use inline_cache::InlineCacheSystem;
pub use profiling::FunctionProfile;
pub use speculation::{DeoptPoint, SpeculativeOptimizer};
pub use tiers::{AdaptiveThresholds, Tier};

// Additional exports for tests
#[cfg(test)]
pub use profiling::ObservedType;
#[cfg(test)]
pub use speculation::AssumptionKind;
