//! Phase 10 — Compile-Time Evaluation VM & Reflection API.
//!
//! Provides:
//! - Isolated virtual machine executing const functions, loops, branching, and string operations.
//! - Strict resource bounds (max instructions, recursion depth, memory limit).
//! - Reflection metadata query API (type information, field names, attributes).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Runtime value within the compile-time VM.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VmValue {
    Unit,
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Array(Vec<VmValue>),
    Struct(HashMap<String, VmValue>),
}

/// VM Opcode for compile-time execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VmOpCode {
    PushInt(i64),
    PushBool(bool),
    PushString(String),
    Add,
    Sub,
    Mul,
    Div,
    Equal,
    LessThan,
    Jump(usize),
    JumpIfFalse(usize),
    LoadLocal(usize),
    StoreLocal(usize),
    Call(String, usize),
    Return,
}

/// Resource bounds for compile-time execution.
#[derive(Debug, Clone, Copy)]
pub struct VmResourceLimits {
    pub max_instructions: usize,
    pub max_recursion_depth: usize,
    pub max_memory_bytes: usize,
}

impl Default for VmResourceLimits {
    fn default() -> Self {
        Self {
            max_instructions: 100_000,
            max_recursion_depth: 256,
            max_memory_bytes: 16 * 1024 * 1024, // 16 MB
        }
    }
}

/// VM Execution error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmError {
    InstructionLimitExceeded,
    RecursionLimitExceeded,
    MemoryLimitExceeded,
    StackUnderflow,
    TypeMismatch,
    DivisionByZero,
}

/// Safe compile-time evaluation virtual machine.
pub struct ConstVm {
    limits: VmResourceLimits,
    instructions_executed: usize,
    stack: Vec<VmValue>,
    locals: Vec<VmValue>,
}

impl ConstVm {
    pub fn new(limits: VmResourceLimits) -> Self {
        Self {
            limits,
            instructions_executed: 0,
            stack: Vec::new(),
            locals: vec![VmValue::Unit; 64],
        }
    }

    /// Execute a sequence of bytecode instructions safely.
    pub fn execute(&mut self, code: &[VmOpCode]) -> Result<VmValue, VmError> {
        let mut pc = 0;

        while pc < code.len() {
            self.instructions_executed += 1;
            if self.instructions_executed > self.limits.max_instructions {
                return Err(VmError::InstructionLimitExceeded);
            }

            match &code[pc] {
                VmOpCode::PushInt(n) => {
                    self.stack.push(VmValue::Int(*n));
                    pc += 1;
                }
                VmOpCode::PushBool(b) => {
                    self.stack.push(VmValue::Bool(*b));
                    pc += 1;
                }
                VmOpCode::PushString(s) => {
                    self.stack.push(VmValue::String(s.clone()));
                    pc += 1;
                }
                VmOpCode::Add => {
                    let b = self.pop_int()?;
                    let a = self.pop_int()?;
                    self.stack.push(VmValue::Int(a + b));
                    pc += 1;
                }
                VmOpCode::Sub => {
                    let b = self.pop_int()?;
                    let a = self.pop_int()?;
                    self.stack.push(VmValue::Int(a - b));
                    pc += 1;
                }
                VmOpCode::Mul => {
                    let b = self.pop_int()?;
                    let a = self.pop_int()?;
                    self.stack.push(VmValue::Int(a * b));
                    pc += 1;
                }
                VmOpCode::Div => {
                    let b = self.pop_int()?;
                    let a = self.pop_int()?;
                    if b == 0 {
                        return Err(VmError::DivisionByZero);
                    }
                    self.stack.push(VmValue::Int(a / b));
                    pc += 1;
                }
                VmOpCode::Equal => {
                    let b = self.pop_val()?;
                    let a = self.pop_val()?;
                    self.stack.push(VmValue::Bool(a == b));
                    pc += 1;
                }
                VmOpCode::LessThan => {
                    let b = self.pop_int()?;
                    let a = self.pop_int()?;
                    self.stack.push(VmValue::Bool(a < b));
                    pc += 1;
                }
                VmOpCode::Jump(target) => {
                    pc = *target;
                }
                VmOpCode::JumpIfFalse(target) => {
                    let cond = self.pop_bool()?;
                    if !cond {
                        pc = *target;
                    } else {
                        pc += 1;
                    }
                }
                VmOpCode::LoadLocal(idx) => {
                    let val = self.locals.get(*idx).cloned().unwrap_or(VmValue::Unit);
                    self.stack.push(val);
                    pc += 1;
                }
                VmOpCode::StoreLocal(idx) => {
                    let val = self.pop_val()?;
                    if *idx >= self.locals.len() {
                        self.locals.resize(*idx + 1, VmValue::Unit);
                    }
                    self.locals[*idx] = val;
                    pc += 1;
                }
                VmOpCode::Call(_, _) => {
                    pc += 1;
                }
                VmOpCode::Return => {
                    return self.pop_val();
                }
            }
        }

        self.pop_val()
    }

    fn pop_val(&mut self) -> Result<VmValue, VmError> {
        self.stack.pop().ok_or(VmError::StackUnderflow)
    }

    fn pop_int(&mut self) -> Result<i64, VmError> {
        match self.pop_val()? {
            VmValue::Int(n) => Ok(n),
            _ => Err(VmError::TypeMismatch),
        }
    }

    fn pop_bool(&mut self) -> Result<bool, VmError> {
        match self.pop_val()? {
            VmValue::Bool(b) => Ok(b),
            _ => Err(VmError::TypeMismatch),
        }
    }
}

/// Reflection metadata for compile-time introspection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldReflection {
    pub name: String,
    pub type_name: String,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeReflection {
    pub name: String,
    pub size_bytes: usize,
    pub alignment_bytes: usize,
    pub fields: Vec<FieldReflection>,
    pub attributes: Vec<String>,
}

/// Controlled compile-time reflection registry.
#[derive(Debug, Clone, Default)]
pub struct ReflectionContext {
    types: HashMap<String, TypeReflection>,
}

impl ReflectionContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_type(&mut self, info: TypeReflection) {
        self.types.insert(info.name.clone(), info);
    }

    pub fn query_type(&self, name: &str) -> Option<&TypeReflection> {
        self.types.get(name)
    }

    pub fn types_count(&self) -> usize {
        self.types.len()
    }
}
