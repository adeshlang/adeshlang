//! Tier-specific execution support for the tiered JIT.
//!
//! Provides the core data structures used across all execution tiers:
//! - JitFrame: Execution frame with tier-specific optimizations
//! - ControlFlow: Result type for instruction execution

use crate::backends::jit::builtins::RuntimeValue;
use crate::backends::jit::lir::BlockId;
use crate::utils::collections::FastMap;

/// Control flow result from instruction execution.
#[derive(Debug)]
pub enum ControlFlow {
    /// Continue to next instruction
    Next,
    /// Jump to target block
    Jump(BlockId),
    /// Return from function with value
    Return(RuntimeValue),
    /// Tail call optimization
    TailCall(String, Vec<RuntimeValue>),
}

/// JIT execution frame with tier-specific optimizations.
///
/// The frame size varies by tier to optimize memory usage vs performance.
pub struct JitFrame {
    pub(super) values: Vec<RuntimeValue>,
    pub(super) vars: FastMap<String, RuntimeValue>,
    /// Pre-allocated capacity for optimizing tier
    capacity: usize,
}

impl JitFrame {
    /// Create a new frame for interpreter tier (minimal allocation).
    pub fn new() -> Self {
        JitFrame {
            values: Vec::with_capacity(32),
            vars: FastMap::default(),
            capacity: 32,
        }
    }

    /// Create a new frame for baseline tier (moderate allocation).
    pub fn new_baseline() -> Self {
        JitFrame {
            values: Vec::with_capacity(64),
            vars: FastMap::default(),
            capacity: 64,
        }
    }

    /// Create a new frame for optimizing tier (aggressive pre-allocation).
    pub fn new_optimizing() -> Self {
        JitFrame {
            values: Vec::with_capacity(128),
            vars: FastMap::default(),
            capacity: 128,
        }
    }

    /// Get value by ID (fast path with bounds checking).
    #[inline(always)]
    pub fn get_value(&self, id: crate::backends::jit::lir::ValueId) -> RuntimeValue {
        let idx = id as usize;
        if idx < self.values.len() {
            self.values[idx].clone()
        } else {
            RuntimeValue::Null
        }
    }

    /// Set value by ID (grows vector if needed).
    #[inline(always)]
    pub fn set_value(&mut self, id: crate::backends::jit::lir::ValueId, value: RuntimeValue) {
        let idx = id as usize;
        if idx >= self.values.len() {
            // Check if we need to grow beyond initial capacity
            if idx >= self.capacity {
                self.capacity = (idx + 1).next_power_of_two();
                self.values.reserve(self.capacity - self.values.len());
            }
            self.values.resize(idx + 1, RuntimeValue::Null);
        }
        self.values[idx] = value;
    }

    /// Get variable by name.
    #[inline(always)]
    pub fn get_var(&self, name: &str) -> Option<RuntimeValue> {
        self.vars.get(name).cloned()
    }

    /// Set variable by name.
    #[inline(always)]
    pub fn set_var(&mut self, name: String, value: RuntimeValue) {
        self.vars.insert(name, value);
    }
}
