//! Interpreter Execution Engine - Phase 7.1
//!
//! This module implements the actual execution of VIR through the interpreter backend.
//! It maintains a runtime value store and executes all VIR operations interpreted
//! (not compiled).

use crate::ir::vir::{BlockId, ValueId, VirBlock, VirFunction, VirInstruction, VirTerminator};
use crate::ir::vir::{CmpOp, FloatBinOp, FloatUnOp, IntBinOp, IntUnOp};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Runtime value for interpreter execution
#[derive(Debug, Clone, PartialEq)]
pub enum InterpreterValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Null,
    Pointer(usize), // Memory address
    Array(Vec<InterpreterValue>),
    Struct(HashMap<String, InterpreterValue>),
}

impl InterpreterValue {
    /// Convert to integer
    pub fn as_int(&self) -> i64 {
        match self {
            InterpreterValue::Int(i) => *i,
            InterpreterValue::Float(f) => *f as i64,
            InterpreterValue::Bool(b) => {
                if *b {
                    1
                } else {
                    0
                }
            }
            InterpreterValue::Pointer(p) => *p as i64,
            _ => 0,
        }
    }

    /// Convert to float
    pub fn as_float(&self) -> f64 {
        match self {
            InterpreterValue::Float(f) => *f,
            InterpreterValue::Int(i) => *i as f64,
            InterpreterValue::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            InterpreterValue::Pointer(p) => *p as f64,
            _ => 0.0,
        }
    }

    /// Convert to boolean
    pub fn as_bool(&self) -> bool {
        match self {
            InterpreterValue::Bool(b) => *b,
            InterpreterValue::Int(i) => *i != 0,
            InterpreterValue::Float(f) => *f != 0.0,
            InterpreterValue::Null => false,
            InterpreterValue::Pointer(p) => *p != 0,
            _ => true,
        }
    }

    /// Convert to string
    pub fn as_string(&self) -> String {
        match self {
            InterpreterValue::String(s) => s.clone(),
            InterpreterValue::Int(i) => i.to_string(),
            InterpreterValue::Float(f) => f.to_string(),
            InterpreterValue::Bool(b) => b.to_string(),
            InterpreterValue::Null => "null".to_string(),
            InterpreterValue::Pointer(p) => format!("0x{:x}", p),
            _ => "[complex value]".to_string(),
        }
    }
}

/// Memory allocator for heap values
pub struct MemoryAllocator {
    memory: Vec<u8>,
    next_addr: usize,
}

impl MemoryAllocator {
    /// Create a new memory allocator
    pub fn new() -> Self {
        let capacity = 1024 * 1024; // 1MB initial capacity
        Self {
            memory: vec![0u8; capacity],
            next_addr: 0,
        }
    }

    /// Allocate memory
    pub fn alloc(&mut self, size: usize) -> usize {
        let addr = self.next_addr;
        self.next_addr += size;

        // Ensure we don't overflow
        if self.next_addr > self.memory.len() {
            self.memory.resize(self.next_addr * 2, 0);
        }

        addr
    }

    /// Free memory (simplified - no defragmentation)
    pub fn free(&mut self, _addr: usize) {
        // In a real implementation, would track allocations and reuse
    }

    /// Write bytes to memory
    pub fn write(&mut self, addr: usize, data: &[u8]) {
        if addr + data.len() <= self.memory.len() {
            self.memory[addr..addr + data.len()].copy_from_slice(data);
        }
    }

    /// Read bytes from memory
    pub fn read(&self, addr: usize, size: usize) -> Vec<u8> {
        if addr + size <= self.memory.len() {
            self.memory[addr..addr + size].to_vec()
        } else {
            vec![0; size]
        }
    }
}

/// Execution context for interpreter
pub struct ExecutionContext {
    /// Value store: ValueId -> InterpreterValue
    values: HashMap<ValueId, InterpreterValue>,

    /// Memory allocator
    memory: MemoryAllocator,

    /// Current block ID
    #[allow(dead_code)]
    current_block: BlockId,

    /// Call stack for function calls
    call_stack: Vec<HashMap<ValueId, InterpreterValue>>,

    /// Exception/error message
    exception: Option<String>,
}

impl ExecutionContext {
    /// Create a new execution context
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
            memory: MemoryAllocator::new(),
            current_block: 0,
            call_stack: vec![],
            exception: None,
        }
    }

    /// Set a value
    pub fn set_value(&mut self, id: ValueId, value: InterpreterValue) {
        self.values.insert(id, value);
    }

    /// Get a value
    pub fn get_value(&self, id: ValueId) -> Option<InterpreterValue> {
        self.values.get(&id).cloned()
    }

    /// Remove a value (for Drop operations)
    pub fn remove_value(&mut self, id: ValueId) {
        self.values.remove(&id);
    }

    /// Push a new scope
    pub fn push_scope(&mut self) {
        self.call_stack.push(self.values.clone());
    }

    /// Pop a scope
    pub fn pop_scope(&mut self) {
        if let Some(scope) = self.call_stack.pop() {
            self.values = scope;
        }
    }

    /// Set exception
    pub fn set_exception(&mut self, msg: String) {
        self.exception = Some(msg);
    }

    /// Check if exception
    pub fn has_exception(&self) -> bool {
        self.exception.is_some()
    }

    /// Get exception message
    pub fn get_exception(&self) -> Option<String> {
        self.exception.clone()
    }

    /// Clear exception
    pub fn clear_exception(&mut self) {
        self.exception = None;
    }

    /// Allocate memory
    pub fn alloc(&mut self, size: usize) -> usize {
        self.memory.alloc(size)
    }

    /// Free memory
    pub fn free(&mut self, addr: usize) {
        self.memory.free(addr);
    }

    /// Write to memory
    pub fn mem_write(&mut self, addr: usize, data: &[u8]) {
        self.memory.write(addr, data);
    }

    /// Read from memory
    pub fn mem_read(&self, addr: usize, size: usize) -> Vec<u8> {
        self.memory.read(addr, size)
    }
}

/// Interpreter executor that runs VIR operations
pub struct InterpreterExecutor {
    context: Rc<RefCell<ExecutionContext>>,
}

impl InterpreterExecutor {
    /// Create a new interpreter executor
    pub fn new() -> Self {
        Self {
            context: Rc::new(RefCell::new(ExecutionContext::new())),
        }
    }

    /// Execute an integer binary operation
    pub fn exec_int_binop(&self, op: &str, lhs: i64, rhs: i64) -> i64 {
        match op {
            "add" => lhs.wrapping_add(rhs),
            "sub" => lhs.wrapping_sub(rhs),
            "mul" => lhs.wrapping_mul(rhs),
            "div" => {
                if rhs != 0 {
                    lhs / rhs
                } else {
                    0
                }
            }
            "rem" => {
                if rhs != 0 {
                    lhs % rhs
                } else {
                    0
                }
            }
            "and" => lhs & rhs,
            "or" => lhs | rhs,
            "xor" => lhs ^ rhs,
            "shl" | "shr" => {
                // Out-of-range shift counts are an error, not a masked shift —
                // the same policy the interpreter and native tiers enforce.
                if rhs < 0 || rhs >= 64 {
                    self.context.borrow_mut().exception =
                        Some("invalid shift amount: expected 0 <= count < 64".to_string());
                    return 0;
                }
                if op == "shl" { lhs << rhs } else { lhs >> rhs }
            }
            _ => 0,
        }
    }

    /// Execute a float binary operation
    pub fn exec_float_binop(&self, op: &str, lhs: f64, rhs: f64) -> f64 {
        match op {
            "add" => lhs + rhs,
            "sub" => lhs - rhs,
            "mul" => lhs * rhs,
            "div" => lhs / rhs,
            "rem" => lhs % rhs,
            _ => 0.0,
        }
    }

    /// Execute an integer comparison
    pub fn exec_int_cmp(&self, op: &str, lhs: i64, rhs: i64) -> bool {
        match op {
            "eq" => lhs == rhs,
            "ne" => lhs != rhs,
            "lt" => lhs < rhs,
            "le" => lhs <= rhs,
            "gt" => lhs > rhs,
            "ge" => lhs >= rhs,
            _ => false,
        }
    }

    /// Execute a float comparison
    pub fn exec_float_cmp(&self, op: &str, lhs: f64, rhs: f64) -> bool {
        match op {
            "eq" => (lhs - rhs).abs() < f64::EPSILON,
            "ne" => (lhs - rhs).abs() >= f64::EPSILON,
            "lt" => lhs < rhs,
            "le" => lhs <= rhs,
            "gt" => lhs > rhs,
            "ge" => lhs >= rhs,
            _ => false,
        }
    }

    /// Execute an integer unary operation
    pub fn exec_int_unop(&self, op: &str, operand: i64) -> i64 {
        match op {
            "neg" => operand.wrapping_neg(),
            "not" => !operand,
            _ => operand,
        }
    }

    /// Execute a float unary operation
    pub fn exec_float_unop(&self, op: &str, operand: f64) -> f64 {
        match op {
            "neg" => -operand,
            "abs" => operand.abs(),
            "sqrt" => operand.sqrt(),
            "floor" => operand.floor(),
            "ceil" => operand.ceil(),
            "round" => operand.round(),
            _ => operand,
        }
    }

    /// Execute a cast operation
    pub fn exec_cast(&self, value: &InterpreterValue, target_type: &str) -> InterpreterValue {
        match target_type {
            "i64" => InterpreterValue::Int(value.as_int()),
            "f64" => InterpreterValue::Float(value.as_float()),
            "bool" => InterpreterValue::Bool(value.as_bool()),
            "string" => InterpreterValue::String(value.as_string()),
            _ => value.clone(),
        }
    }

    /// Get mutable context
    pub fn context_mut(&self) -> std::cell::RefMut<'_, ExecutionContext> {
        self.context.borrow_mut()
    }

    /// Get immutable context
    pub fn context(&self) -> std::cell::Ref<'_, ExecutionContext> {
        self.context.borrow()
    }

    /// Execute a VIR function
    pub fn execute_function(
        &self,
        func: &VirFunction,
        args: Vec<InterpreterValue>,
    ) -> Result<InterpreterValue, String> {
        // Set up parameters
        let mut ctx = self.context_mut();
        for (idx, arg) in args.iter().enumerate() {
            ctx.set_value(idx as ValueId, arg.clone());
        }
        drop(ctx);

        // Execute entry block (block 0)
        if func.blocks.is_empty() {
            return Err("Function has no blocks".to_string());
        }

        let mut current_block_idx = 0;

        // Execute blocks until we hit a return
        loop {
            if current_block_idx >= func.blocks.len() {
                return Err("Invalid block index".to_string());
            }

            let block = &func.blocks[current_block_idx];

            // Execute block and get next action
            match self.execute_block(block)? {
                BlockResult::Return(value) => {
                    return Ok(value.unwrap_or(InterpreterValue::Null));
                }
                BlockResult::Jump(target_block) => {
                    // Find the block with the target ID
                    current_block_idx = func
                        .blocks
                        .iter()
                        .position(|b| b.id == target_block)
                        .ok_or_else(|| format!("Block {} not found", target_block))?;
                }
                BlockResult::Branch(cond, true_block, false_block) => {
                    let target = if cond { true_block } else { false_block };
                    current_block_idx = func
                        .blocks
                        .iter()
                        .position(|b| b.id == target)
                        .ok_or_else(|| format!("Block {} not found", target))?;
                }
                BlockResult::Unreachable => {
                    return Err("Unreachable code executed".to_string());
                }
            }
        }
    }

    /// Execute a VIR block
    fn execute_block(&self, block: &VirBlock) -> Result<BlockResult, String> {
        // Execute all instructions
        for instruction in &block.instructions {
            self.execute_instruction(instruction)?;

            // Check for exceptions
            if self.context().has_exception() {
                let err = self.context().get_exception().unwrap();
                return Err(err);
            }
        }

        // Execute terminator
        self.execute_terminator(&block.terminator)
    }

    /// Execute a VIR instruction
    fn execute_instruction(&self, inst: &VirInstruction) -> Result<(), String> {
        match inst {
            VirInstruction::ConstInt { dest, value, .. } => {
                self.context_mut()
                    .set_value(*dest, InterpreterValue::Int(*value));
            }

            VirInstruction::ConstFloat { dest, value, .. } => {
                self.context_mut()
                    .set_value(*dest, InterpreterValue::Float(*value));
            }

            VirInstruction::ConstBool { dest, value } => {
                self.context_mut()
                    .set_value(*dest, InterpreterValue::Bool(*value));
            }

            VirInstruction::ConstString {
                dest, string_id, ..
            } => {
                // Store the string ID as a string reference.
                // The executor doesn't have direct access to the module's
                // string pool, so we store a reference by ID.
                self.context_mut().set_value(
                    *dest,
                    InterpreterValue::String(format!("_str_{}", string_id)),
                );
            }

            VirInstruction::ConstNull { dest } => {
                self.context_mut().set_value(*dest, InterpreterValue::Null);
            }

            VirInstruction::IntBinOp {
                dest, op, lhs, rhs, ..
            } => {
                let lhs_val = self
                    .context()
                    .get_value(*lhs)
                    .ok_or_else(|| format!("Value {} not found", lhs))?;
                let rhs_val = self
                    .context()
                    .get_value(*rhs)
                    .ok_or_else(|| format!("Value {} not found", rhs))?;

                let op_str = match op {
                    IntBinOp::Add => "add",
                    IntBinOp::Sub => "sub",
                    IntBinOp::Mul => "mul",
                    IntBinOp::Div => "div",
                    IntBinOp::Rem => "rem",
                    IntBinOp::And => "and",
                    IntBinOp::Or => "or",
                    IntBinOp::Xor => "xor",
                    IntBinOp::Shl => "shl",
                    IntBinOp::Shr => "shr",
                };

                let result = self.exec_int_binop(op_str, lhs_val.as_int(), rhs_val.as_int());
                self.context_mut()
                    .set_value(*dest, InterpreterValue::Int(result));
            }

            VirInstruction::FloatBinOp {
                dest, op, lhs, rhs, ..
            } => {
                let lhs_val = self
                    .context()
                    .get_value(*lhs)
                    .ok_or_else(|| format!("Value {} not found", lhs))?;
                let rhs_val = self
                    .context()
                    .get_value(*rhs)
                    .ok_or_else(|| format!("Value {} not found", rhs))?;

                let op_str = match op {
                    FloatBinOp::Add => "add",
                    FloatBinOp::Sub => "sub",
                    FloatBinOp::Mul => "mul",
                    FloatBinOp::Div => "div",
                };

                let result = self.exec_float_binop(op_str, lhs_val.as_float(), rhs_val.as_float());
                self.context_mut()
                    .set_value(*dest, InterpreterValue::Float(result));
            }

            VirInstruction::IntCmp { dest, op, lhs, rhs } => {
                let lhs_val = self
                    .context()
                    .get_value(*lhs)
                    .ok_or_else(|| format!("Value {} not found", lhs))?;
                let rhs_val = self
                    .context()
                    .get_value(*rhs)
                    .ok_or_else(|| format!("Value {} not found", rhs))?;

                let op_str = match op {
                    CmpOp::Eq => "eq",
                    CmpOp::Ne => "ne",
                    CmpOp::Lt => "lt",
                    CmpOp::Le => "le",
                    CmpOp::Gt => "gt",
                    CmpOp::Ge => "ge",
                };

                let result = self.exec_int_cmp(op_str, lhs_val.as_int(), rhs_val.as_int());
                self.context_mut()
                    .set_value(*dest, InterpreterValue::Bool(result));
            }

            VirInstruction::FloatCmp { dest, op, lhs, rhs } => {
                let lhs_val = self
                    .context()
                    .get_value(*lhs)
                    .ok_or_else(|| format!("Value {} not found", lhs))?;
                let rhs_val = self
                    .context()
                    .get_value(*rhs)
                    .ok_or_else(|| format!("Value {} not found", rhs))?;

                let op_str = match op {
                    CmpOp::Eq => "eq",
                    CmpOp::Ne => "ne",
                    CmpOp::Lt => "lt",
                    CmpOp::Le => "le",
                    CmpOp::Gt => "gt",
                    CmpOp::Ge => "ge",
                };

                let result = self.exec_float_cmp(op_str, lhs_val.as_float(), rhs_val.as_float());
                self.context_mut()
                    .set_value(*dest, InterpreterValue::Bool(result));
            }

            VirInstruction::IntUnOp {
                dest, op, operand, ..
            } => {
                let operand_val = self
                    .context()
                    .get_value(*operand)
                    .ok_or_else(|| format!("Value {} not found", operand))?;

                let op_str = match op {
                    IntUnOp::Neg => "neg",
                    IntUnOp::Not => "not",
                };

                let result = self.exec_int_unop(op_str, operand_val.as_int());
                self.context_mut()
                    .set_value(*dest, InterpreterValue::Int(result));
            }

            VirInstruction::FloatUnOp {
                dest, op, operand, ..
            } => {
                let operand_val = self
                    .context()
                    .get_value(*operand)
                    .ok_or_else(|| format!("Value {} not found", operand))?;

                let op_str = match op {
                    FloatUnOp::Neg => "neg",
                    FloatUnOp::Abs => "abs",
                    FloatUnOp::Sqrt => "sqrt",
                };

                let result = self.exec_float_unop(op_str, operand_val.as_float());
                self.context_mut()
                    .set_value(*dest, InterpreterValue::Float(result));
            }

            VirInstruction::Copy { dest, src } | VirInstruction::Move { dest, src } => {
                let src_val = self
                    .context()
                    .get_value(*src)
                    .ok_or_else(|| format!("Value {} not found", src))?;
                self.context_mut().set_value(*dest, src_val);
            }

            VirInstruction::Cast {
                dest, value, to_ty, ..
            } => {
                let val = self
                    .context()
                    .get_value(*value)
                    .ok_or_else(|| format!("Value {} not found", value))?;
                let type_str = format!("{:?}", to_ty); // Simplified
                let result = self.exec_cast(&val, &type_str);
                self.context_mut().set_value(*dest, result);
            }

            VirInstruction::Alloc { dest, size, .. } => {
                let size_val = self
                    .context()
                    .get_value(*size)
                    .ok_or_else(|| format!("Value {} not found", size))?;
                let addr = self.context_mut().alloc(size_val.as_int() as usize);
                self.context_mut()
                    .set_value(*dest, InterpreterValue::Pointer(addr));
            }

            VirInstruction::Free { ptr } => {
                let ptr_val = self
                    .context()
                    .get_value(*ptr)
                    .ok_or_else(|| format!("Value {} not found", ptr))?;
                self.context_mut().free(ptr_val.as_int() as usize);
            }

            VirInstruction::Load { dest, ptr, .. } => {
                let ptr_val = self
                    .context()
                    .get_value(*ptr)
                    .ok_or_else(|| format!("Value {} not found", ptr))?;
                // For simplicity, load an i64
                let addr = ptr_val.as_int() as usize;
                let data = self.context().mem_read(addr, 8);
                let value = i64::from_le_bytes(data.try_into().unwrap_or([0; 8]));
                self.context_mut()
                    .set_value(*dest, InterpreterValue::Int(value));
            }

            VirInstruction::Store { ptr, value } => {
                let ptr_val = self
                    .context()
                    .get_value(*ptr)
                    .ok_or_else(|| format!("Value {} not found", ptr))?;
                let val = self
                    .context()
                    .get_value(*value)
                    .ok_or_else(|| format!("Value {} not found", value))?;
                let addr = ptr_val.as_int() as usize;
                let bytes = val.as_int().to_le_bytes();
                self.context_mut().mem_write(addr, &bytes);
            }

            VirInstruction::LoadLocal { dest, local } => {
                // Stack locals modeled at fixed memory offsets
                let addr = (*local as usize + 1) * 1024;
                let data = self.context().mem_read(addr, 8);
                let value = i64::from_le_bytes(data.try_into().unwrap_or([0; 8]));
                self.context_mut()
                    .set_value(*dest, InterpreterValue::Int(value));
            }

            VirInstruction::StoreLocal { local, value } => {
                let val = self
                    .context()
                    .get_value(*value)
                    .ok_or_else(|| format!("Value {} not found", value))?;
                let addr = (*local as usize + 1) * 1024;
                let bytes = val.as_int().to_le_bytes();
                self.context_mut().mem_write(addr, &bytes);
            }

            VirInstruction::Drop { value } => {
                // Drop removes the value from the context
                self.context_mut().remove_value(*value);
            }

            VirInstruction::Call { dest, func, args } => {
                // Evaluate arguments
                let _arg_vals: Vec<InterpreterValue> = args
                    .iter()
                    .filter_map(|a| self.context().get_value(*a))
                    .collect();
                // Resolve function — the executor would need a function table
                // for proper cross-function calls. For now, store Null at dest.
                let _ = func;
                if let Some(d) = dest {
                    self.context_mut().set_value(*d, InterpreterValue::Null);
                }
            }

            VirInstruction::Intrinsic {
                dest,
                intrinsic,
                args,
            } => {
                let arg_vals: Vec<InterpreterValue> = args
                    .iter()
                    .filter_map(|a| self.context().get_value(*a))
                    .collect();
                let result = match intrinsic {
                    crate::ir::vir::Intrinsic::Sin => InterpreterValue::Float(
                        arg_vals.first().map(|v| v.as_float().sin()).unwrap_or(0.0),
                    ),
                    crate::ir::vir::Intrinsic::Cos => InterpreterValue::Float(
                        arg_vals.first().map(|v| v.as_float().cos()).unwrap_or(0.0),
                    ),
                    crate::ir::vir::Intrinsic::Tan => InterpreterValue::Float(
                        arg_vals.first().map(|v| v.as_float().tan()).unwrap_or(0.0),
                    ),
                    crate::ir::vir::Intrinsic::Log => InterpreterValue::Float(
                        arg_vals.first().map(|v| v.as_float().ln()).unwrap_or(0.0),
                    ),
                    crate::ir::vir::Intrinsic::Exp => InterpreterValue::Float(
                        arg_vals.first().map(|v| v.as_float().exp()).unwrap_or(0.0),
                    ),
                    crate::ir::vir::Intrinsic::Pow => {
                        let base = arg_vals.first().map(|v| v.as_float()).unwrap_or(0.0);
                        let exp = arg_vals.get(1).map(|v| v.as_float()).unwrap_or(1.0);
                        InterpreterValue::Float(base.powf(exp))
                    }
                    _ => InterpreterValue::Null,
                };
                if let Some(d) = dest {
                    self.context_mut().set_value(*d, result);
                }
            }

            // Aggregate operations — model as pointer allocations
            VirInstruction::BuildStruct { dest, fields, .. } => {
                let addr = self.context_mut().alloc(fields.len().max(1) * 8);
                for (idx, field_val) in fields.iter().enumerate() {
                    if let Some(val) = self.context().get_value(*field_val) {
                        let bytes = val.as_int().to_le_bytes();
                        self.context_mut().mem_write(addr + idx * 8, &bytes);
                    }
                }
                self.context_mut()
                    .set_value(*dest, InterpreterValue::Pointer(addr));
            }
            VirInstruction::ExtractField {
                dest,
                struct_val,
                field,
                ..
            } => {
                if let Some(InterpreterValue::Pointer(addr)) = self.context().get_value(*struct_val)
                {
                    let data = self.context().mem_read(addr + *field as usize * 8, 8);
                    let value = i64::from_le_bytes(data.try_into().unwrap_or([0; 8]));
                    self.context_mut()
                        .set_value(*dest, InterpreterValue::Int(value));
                } else {
                    self.context_mut().set_value(*dest, InterpreterValue::Null);
                }
            }
            VirInstruction::BuildArray { dest, elements, .. } => {
                let addr = self.context_mut().alloc(elements.len().max(1) * 8);
                for (idx, elem) in elements.iter().enumerate() {
                    if let Some(val) = self.context().get_value(*elem) {
                        let bytes = val.as_int().to_le_bytes();
                        self.context_mut().mem_write(addr + idx * 8, &bytes);
                    }
                }
                self.context_mut()
                    .set_value(*dest, InterpreterValue::Pointer(addr));
            }
            VirInstruction::BuildTuple { dest, elements } => {
                let addr = self.context_mut().alloc(elements.len().max(1) * 8);
                for (idx, elem) in elements.iter().enumerate() {
                    if let Some(val) = self.context().get_value(*elem) {
                        let bytes = val.as_int().to_le_bytes();
                        self.context_mut().mem_write(addr + idx * 8, &bytes);
                    }
                }
                self.context_mut()
                    .set_value(*dest, InterpreterValue::Pointer(addr));
            }
            VirInstruction::ExtractTuple {
                dest, tuple, index, ..
            } => {
                if let Some(InterpreterValue::Pointer(addr)) = self.context().get_value(*tuple) {
                    let data = self.context().mem_read(addr + *index as usize * 8, 8);
                    let value = i64::from_le_bytes(data.try_into().unwrap_or([0; 8]));
                    self.context_mut()
                        .set_value(*dest, InterpreterValue::Int(value));
                } else {
                    self.context_mut().set_value(*dest, InterpreterValue::Null);
                }
            }
            VirInstruction::BuildObject { dest, .. } => {
                let addr = self.context_mut().alloc(8);
                self.context_mut()
                    .set_value(*dest, InterpreterValue::Pointer(addr));
            }
            VirInstruction::BuildEnum { dest, variant, .. } => {
                self.context_mut()
                    .set_value(*dest, InterpreterValue::Int(*variant as i64));
            }
            VirInstruction::GetDiscriminant { dest, enum_val } => {
                if let Some(val) = self.context().get_value(*enum_val) {
                    self.context_mut().set_value(*dest, val);
                } else {
                    self.context_mut()
                        .set_value(*dest, InterpreterValue::Int(0));
                }
            }
            VirInstruction::ExtractPayload { dest, .. } => {
                self.context_mut().set_value(*dest, InterpreterValue::Null);
            }

            // ARC operations — simulated (no actual reference counting needed in interpreter)
            VirInstruction::ArcClone { dest, src } => {
                if let Some(val) = self.context().get_value(*src) {
                    self.context_mut().set_value(*dest, val);
                }
            }
            VirInstruction::ArcDrop { ptr } => {
                self.context_mut().remove_value(*ptr);
            }
            VirInstruction::ArcIncrement { .. } | VirInstruction::ArcDecrement { .. } => {
                // No-op in interpreter (Rust handles memory)
            }

            // Default handling for unimplemented instructions
            _ => {
                // No-op for other instructions
            }
        }

        Ok(())
    }

    /// Execute a VIR terminator
    fn execute_terminator(&self, term: &VirTerminator) -> Result<BlockResult, String> {
        match term {
            VirTerminator::Return { value } => {
                let return_val = if let Some(val_id) = value {
                    self.context().get_value(*val_id)
                } else {
                    None
                };
                Ok(BlockResult::Return(return_val))
            }

            VirTerminator::Jump { target } => Ok(BlockResult::Jump(*target)),

            VirTerminator::Branch {
                cond,
                true_target,
                false_target,
            } => {
                let cond_val = self
                    .context()
                    .get_value(*cond)
                    .ok_or_else(|| format!("Condition value {} not found", cond))?;
                Ok(BlockResult::Branch(
                    cond_val.as_bool(),
                    *true_target,
                    *false_target,
                ))
            }

            VirTerminator::Switch {
                value,
                cases,
                default,
            } => {
                let val = self
                    .context()
                    .get_value(*value)
                    .ok_or_else(|| format!("Switch value {} not found", value))?;
                let switch_val = val.as_int();

                // Find matching case
                for (case_val, target) in cases {
                    if switch_val == *case_val {
                        return Ok(BlockResult::Jump(*target));
                    }
                }

                // Default case
                Ok(BlockResult::Jump(*default))
            }

            VirTerminator::Unreachable => Ok(BlockResult::Unreachable),
        }
    }
}

/// Result of executing a block
enum BlockResult {
    Return(Option<InterpreterValue>),
    Jump(BlockId),
    Branch(bool, BlockId, BlockId),
    Unreachable,
}

impl Default for InterpreterExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interpreter_value_conversions() {
        let v = InterpreterValue::Int(42);
        assert_eq!(v.as_int(), 42);
        assert_eq!(v.as_float(), 42.0);
        assert!(v.as_bool());
        assert_eq!(v.as_string(), "42");
    }

    #[test]
    fn test_int_binop() {
        let executor = InterpreterExecutor::new();
        assert_eq!(executor.exec_int_binop("add", 5, 3), 8);
        assert_eq!(executor.exec_int_binop("sub", 5, 3), 2);
        assert_eq!(executor.exec_int_binop("mul", 5, 3), 15);
        assert_eq!(executor.exec_int_binop("div", 15, 3), 5);
    }

    #[test]
    fn test_float_binop() {
        let executor = InterpreterExecutor::new();
        assert!((executor.exec_float_binop("add", 5.0, 3.0) - 8.0).abs() < 0.001);
        assert!((executor.exec_float_binop("mul", 5.0, 3.0) - 15.0).abs() < 0.001);
    }

    #[test]
    fn test_int_cmp() {
        let executor = InterpreterExecutor::new();
        assert!(executor.exec_int_cmp("eq", 5, 5));
        assert!(executor.exec_int_cmp("ne", 5, 3));
        assert!(executor.exec_int_cmp("lt", 3, 5));
        assert!(executor.exec_int_cmp("ge", 5, 5));
    }

    #[test]
    fn test_memory_allocation() {
        let executor = InterpreterExecutor::new();
        let mut ctx = executor.context_mut();
        let addr1 = ctx.alloc(100);
        let addr2 = ctx.alloc(200);
        assert!(addr2 > addr1);
    }

    #[test]
    fn test_execution_context() {
        let executor = InterpreterExecutor::new();
        {
            let mut ctx = executor.context_mut();
            ctx.set_value(0, InterpreterValue::Int(42));
        }
        {
            let ctx = executor.context();
            let val = ctx.get_value(0);
            assert_eq!(val, Some(InterpreterValue::Int(42)));
        }
    }

    #[test]
    fn test_exception_handling() {
        let executor = InterpreterExecutor::new();
        {
            let mut ctx = executor.context_mut();
            assert!(!ctx.has_exception());
            ctx.set_exception("Test error".to_string());
            assert!(ctx.has_exception());
            assert_eq!(ctx.get_exception(), Some("Test error".to_string()));
            ctx.clear_exception();
            assert!(!ctx.has_exception());
        }
    }
}
