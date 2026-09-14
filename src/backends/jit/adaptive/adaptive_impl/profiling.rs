//! Runtime profiling and type feedback collection
//!
//! This module provides infrastructure for collecting runtime information about
//! function execution, type observations, and call site behavior. This data drives
//! adaptive compilation decisions and speculative optimizations.

use super::tiers::{AdaptiveThresholds, Tier};
use crate::backends::jit::builtins::RuntimeValue;
use crate::backends::jit::lir::BlockId;
use crate::utils::collections::FastMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Type feedback for a single call site or operation
#[derive(Debug, Clone)]
pub enum TypeFeedback {
    /// No information yet
    Uninitialized,
    /// Single observed type (monomorphic)
    Monomorphic(ObservedType),
    /// Two observed types (bimorphic)
    Bimorphic(ObservedType, ObservedType),
    /// Many types observed (polymorphic)
    Polymorphic,
    /// Too many types - give up on speculation
    Megamorphic,
}

/// Observed type at runtime
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObservedType {
    Int,
    Float,
    Bool,
    String,
    Null,
    Array,
    Object(u64), // Hidden class ID
    Function,
    BigInt,
    Unknown,
}

impl ObservedType {
    pub fn from_runtime_value(val: &RuntimeValue) -> Self {
        match val {
            RuntimeValue::Int(_) => ObservedType::Int,
            RuntimeValue::Tuple(_) => ObservedType::Array, // Treat tuples as arrays for type profiling
            RuntimeValue::Float(_) => ObservedType::Float,
            RuntimeValue::Bool(_) => ObservedType::Bool,
            RuntimeValue::Char(_) => ObservedType::String,
            RuntimeValue::String(_) => ObservedType::String,
            RuntimeValue::Null => ObservedType::Null,
            RuntimeValue::Array(_) => ObservedType::Array,
            RuntimeValue::Set(_) => ObservedType::Array,
            RuntimeValue::BigInt(_) => ObservedType::BigInt,
            RuntimeValue::Object(obj) => {
                // Get hidden class ID from object
                if let Some(hc_id) = obj.get("__hidden_class_id__") {
                    if let RuntimeValue::Int(id) = hc_id {
                        return ObservedType::Object(*id as u64);
                    }
                }
                ObservedType::Object(0)
            }
            RuntimeValue::Promise(_) => ObservedType::Function, // Treat promises as function-like
            RuntimeValue::Function(_) => ObservedType::Function,
            // Fixed-width integer types (treat as Int for type profiling)
            RuntimeValue::U8(_)
            | RuntimeValue::U16(_)
            | RuntimeValue::U32(_)
            | RuntimeValue::U64(_)
            | RuntimeValue::U128(_)
            | RuntimeValue::I8(_)
            | RuntimeValue::I16(_)
            | RuntimeValue::I32(_)
            | RuntimeValue::I64(_)
            | RuntimeValue::I128(_) => ObservedType::Int,
            // Fixed-width float types (treat as Float for type profiling)
            RuntimeValue::F32(_) | RuntimeValue::F64(_) => ObservedType::Float,
            // Array types
            RuntimeValue::RawArray(_, _) | RuntimeValue::DynArray { .. } => ObservedType::Array,
        }
    }
}

/// Call site profile for inline caching
#[derive(Debug)]
pub struct CallSiteProfile {
    /// Type feedback for arguments
    pub arg_types: Vec<TypeFeedback>,
    /// Type feedback for return value
    pub return_type: TypeFeedback,
    /// Number of times this call site was executed
    pub call_count: AtomicU64,
    /// Whether this site has been inlined
    pub is_inlined: AtomicBool,
}

impl CallSiteProfile {
    pub fn new(arg_count: usize) -> Self {
        CallSiteProfile {
            arg_types: vec![TypeFeedback::Uninitialized; arg_count],
            return_type: TypeFeedback::Uninitialized,
            call_count: AtomicU64::new(0),
            is_inlined: AtomicBool::new(false),
        }
    }

    /// Record observed types for arguments
    pub fn record_call(&self, _args: &[RuntimeValue], _result: &RuntimeValue) {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        // Type feedback recording would update arg_types and return_type
        // This is simplified - full impl would use compare-and-swap
    }
}

/// Function profile with detailed execution information
#[derive(Debug)]
pub struct FunctionProfile {
    /// Number of times the function was called
    pub call_count: AtomicU64,
    /// Total execution time (nanoseconds)
    pub total_time_ns: AtomicU64,
    /// Current compilation tier
    pub current_tier: Tier,
    /// Whether the function has been optimized
    pub is_optimized: AtomicBool,
    /// Whether we've seen a deoptimization
    pub has_deoptimized: AtomicBool,
    /// Call site profiles indexed by instruction address
    pub call_sites: FastMap<u32, CallSiteProfile>,
    /// Branch taken counts for each conditional
    pub branch_counts: FastMap<BlockId, (u64, u64)>, // (taken, not_taken)
    /// Loop back-edge counts
    pub loop_counts: FastMap<BlockId, u64>,
}

impl FunctionProfile {
    pub fn new() -> Self {
        FunctionProfile {
            call_count: AtomicU64::new(0),
            total_time_ns: AtomicU64::new(0),
            current_tier: Tier::Interpreter,
            is_optimized: AtomicBool::new(false),
            has_deoptimized: AtomicBool::new(false),
            call_sites: FastMap::default(),
            branch_counts: FastMap::default(),
            loop_counts: FastMap::default(),
        }
    }

    /// Record a function call
    pub fn record_call(&self) {
        self.call_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record execution time
    pub fn record_time(&self, ns: u64) {
        self.total_time_ns.fetch_add(ns, Ordering::Relaxed);
    }

    /// Check if function should be promoted to next tier
    pub fn should_promote(&self, thresholds: &AdaptiveThresholds) -> Option<Tier> {
        let calls = self.call_count.load(Ordering::Relaxed);

        match self.current_tier {
            Tier::Interpreter if calls >= thresholds.baseline_threshold => Some(Tier::Baseline),
            Tier::Baseline if calls >= thresholds.optimizing_threshold => {
                if !self.has_deoptimized.load(Ordering::Relaxed) {
                    Some(Tier::Optimizing)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

impl Default for FunctionProfile {
    fn default() -> Self {
        Self::new()
    }
}
