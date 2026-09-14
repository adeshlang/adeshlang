//! Control flow and execution frame management
//!
//! This module defines the control flow results and execution frame used
//! during JIT execution.

use crate::backends::jit::builtins::RuntimeValue;
use crate::backends::jit::lir::{BlockId, ValueId};
use crate::utils::collections::FastMap;

/// Control flow result
pub enum ControlFlow {
    Next,
    Jump(BlockId),
    Return(RuntimeValue),
    TailCall(String, Vec<RuntimeValue>),
}

/// JIT execution frame
pub struct JitFrame {
    pub values: Vec<RuntimeValue>,
    pub vars: FastMap<String, RuntimeValue>,
}

impl JitFrame {
    pub fn new() -> Self {
        JitFrame {
            values: Vec::with_capacity(64),
            vars: FastMap::default(),
        }
    }

    #[inline(always)]
    pub fn get_value(&self, id: ValueId) -> RuntimeValue {
        let idx = id as usize;
        if idx < self.values.len() {
            self.values[idx].clone()
        } else {
            RuntimeValue::Null
        }
    }

    #[inline(always)]
    pub fn set_value(&mut self, id: ValueId, value: RuntimeValue) {
        let idx = id as usize;
        if idx >= self.values.len() {
            self.values.resize(idx + 1, RuntimeValue::Null);
        }
        self.values[idx] = value;
    }

    #[inline(always)]
    pub fn get_var(&self, name: &str) -> Option<RuntimeValue> {
        self.vars.get(name).cloned()
    }

    #[inline(always)]
    pub fn set_var(&mut self, name: String, value: RuntimeValue) {
        self.vars.insert(name, value);
    }
}
