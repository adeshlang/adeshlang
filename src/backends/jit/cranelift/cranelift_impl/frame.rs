//! JIT execution frame (activation record) implementation
//!
//! ## Performance Optimizations  
//! - Dual-slot values (f64 + RuntimeValue) for fast numeric operations
//! - Pre-allocated capacity to avoid resizing
//! - Unsafe unchecked access for hot paths

use crate::backends::jit::builtins::RuntimeValue;
use crate::backends::jit::lir::ValueId;
use crate::utils::collections::FastMap;

/// JIT execution frame (activation record)
/// Optimized to use Vec for values since ValueId is sequential
pub(in crate::backends::jit::cranelift) struct JitFrame {
    /// Fast numeric values indexed by ValueId (direct array access)
    values_f64: Vec<f64>,
    /// Full values indexed by ValueId (for complex types)
    values: Vec<RuntimeValue>,
    /// Variables by name (still need hashmap for string keys)
    vars: FastMap<String, RuntimeValue>,
}

impl JitFrame {
    pub(in crate::backends::jit::cranelift) fn new() -> Self {
        JitFrame {
            // Pre-allocate reasonable capacity to avoid resizing
            values_f64: Vec::with_capacity(64),
            values: Vec::with_capacity(64),
            vars: FastMap::default(),
        }
    }

    /// Get numeric value using fast f64 slot (bypasses RuntimeValue)
    #[inline(always)]
    pub(in crate::backends::jit::cranelift) fn get_value_f64(&self, id: ValueId) -> f64 {
        let idx = id as usize;
        if idx < self.values_f64.len() {
            // SAFETY: bounds check above
            unsafe { *self.values_f64.get_unchecked(idx) }
        } else {
            0.0
        }
    }

    /// Set numeric value to fast f64 slot
    #[inline(always)]
    pub(in crate::backends::jit::cranelift) fn set_value_f64(&mut self, id: ValueId, value: f64) {
        let idx = id as usize;
        // Ensure capacity
        if idx >= self.values_f64.len() {
            self.values_f64.resize(idx + 1, 0.0);
        }
        // SAFETY: ensured capacity above
        unsafe {
            *self.values_f64.get_unchecked_mut(idx) = value;
        }
    }

    #[inline(always)]
    pub(in crate::backends::jit::cranelift) fn get_value(&self, id: ValueId) -> RuntimeValue {
        let idx = id as usize;
        if idx < self.values.len() {
            self.values[idx].clone()
        } else {
            RuntimeValue::Null
        }
    }

    #[inline(always)]
    pub(in crate::backends::jit::cranelift) fn set_value(
        &mut self,
        id: ValueId,
        value: RuntimeValue,
    ) {
        let idx = id as usize;
        // Ensure capacity for both arrays
        if idx >= self.values.len() {
            self.values.resize(idx + 1, RuntimeValue::Null);
        }
        if idx >= self.values_f64.len() {
            self.values_f64.resize(idx + 1, 0.0);
        }

        // Also set f64 slot if numeric (for fast numeric operations)
        match &value {
            RuntimeValue::Float(f) => self.values_f64[idx] = *f,
            RuntimeValue::Int(i) => self.values_f64[idx] = *i as f64,
            RuntimeValue::F64(f) => self.values_f64[idx] = *f,
            RuntimeValue::F32(f) => self.values_f64[idx] = *f as f64,
            RuntimeValue::U8(n) => self.values_f64[idx] = *n as f64,
            RuntimeValue::U16(n) => self.values_f64[idx] = *n as f64,
            RuntimeValue::U32(n) => self.values_f64[idx] = *n as f64,
            RuntimeValue::U64(n) => self.values_f64[idx] = *n as f64,
            RuntimeValue::U128(n) => self.values_f64[idx] = *n as f64,
            RuntimeValue::I8(n) => self.values_f64[idx] = *n as f64,
            RuntimeValue::I16(n) => self.values_f64[idx] = *n as f64,
            RuntimeValue::I32(n) => self.values_f64[idx] = *n as f64,
            RuntimeValue::I64(n) => self.values_f64[idx] = *n as f64,
            RuntimeValue::I128(n) => self.values_f64[idx] = *n as f64,
            _ => {}
        }

        self.values[idx] = value;
    }

    #[inline(always)]
    pub(in crate::backends::jit::cranelift) fn get_var(&self, name: &str) -> Option<RuntimeValue> {
        self.vars.get(name).cloned()
    }

    #[inline(always)]
    pub(in crate::backends::jit::cranelift) fn set_var(
        &mut self,
        name: String,
        value: RuntimeValue,
    ) {
        self.vars.insert(name, value);
    }
}
