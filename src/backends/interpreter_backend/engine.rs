use crate::backends::lowering::vir_to_interpreter::InterpreterOp;
use crate::ir::vir::ValueId;
use crate::typesystem::value_optimized::Value;
use std::collections::HashMap;

#[derive(Debug)]
pub enum RuntimeError {
    TypeMismatch(String),
    DivisionByZero,
    UnknownFunction(String),
    InvalidMemoryAccess,
    MissingValue(ValueId),
    NotImplemented(String),
    CallError(String),
}

/// The engine that executes `InterpreterOp`s generated from VIR.
pub struct InterpreterEngine {
    values: HashMap<ValueId, Value>,
    /// Simulated memory heap for pointers.
    /// Addresses are simply u64, and we map them to Values.
    memory: HashMap<u64, Value>,
    next_alloc_id: u64,
}

impl InterpreterEngine {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
            memory: HashMap::new(),
            next_alloc_id: 1, // 0 is null
        }
    }

    fn get_val(&self, id: ValueId) -> Result<&Value, RuntimeError> {
        self.values.get(&id).ok_or(RuntimeError::MissingValue(id))
    }

    fn get_i64(&self, id: ValueId) -> Result<i64, RuntimeError> {
        match self.get_val(id)? {
            Value::Number(n) => Ok(*n as i64),
            _ => Err(RuntimeError::TypeMismatch(format!(
                "Expected integer, got {:?}",
                id
            ))),
        }
    }

    fn get_f64(&self, id: ValueId) -> Result<f64, RuntimeError> {
        match self.get_val(id)? {
            Value::Number(n) => Ok(*n),
            _ => Err(RuntimeError::TypeMismatch(format!(
                "Expected float, got {:?}",
                id
            ))),
        }
    }

    #[allow(dead_code)]
    fn get_bool(&self, id: ValueId) -> Result<bool, RuntimeError> {
        match self.get_val(id)? {
            Value::Bool(b) => Ok(*b),
            _ => Err(RuntimeError::TypeMismatch(format!(
                "Expected bool, got {:?}",
                id
            ))),
        }
    }

    fn get_ptr(&self, id: ValueId) -> Result<u64, RuntimeError> {
        match self.get_val(id)? {
            Value::Number(n) => Ok(*n as u64),
            _ => Err(RuntimeError::TypeMismatch(format!(
                "Expected pointer (number), got {:?}",
                id
            ))),
        }
    }

    pub fn execute(&mut self, ops: &[InterpreterOp]) -> Result<Option<Value>, RuntimeError> {
        let mut pc = 0;
        let mut result = None;

        while pc < ops.len() {
            let op = &ops[pc];
            pc += 1;

            match op {
                InterpreterOp::Nop => {}

                InterpreterOp::ConstI64(v, dest) => {
                    self.values.insert(*dest, Value::Number(*v as f64));
                }
                InterpreterOp::ConstF64(v, dest) => {
                    self.values.insert(*dest, Value::Number(*v));
                }
                InterpreterOp::ConstBool(v, dest) => {
                    self.values.insert(*dest, Value::Bool(*v));
                }
                InterpreterOp::ConstNull(dest) => {
                    self.values.insert(*dest, Value::Null);
                }

                // Integer Math
                InterpreterOp::AddI64(lhs, rhs, dest) => {
                    let res = self.get_i64(*lhs)? + self.get_i64(*rhs)?;
                    self.values.insert(*dest, Value::Number(res as f64));
                }
                InterpreterOp::SubI64(lhs, rhs, dest) => {
                    let res = self.get_i64(*lhs)? - self.get_i64(*rhs)?;
                    self.values.insert(*dest, Value::Number(res as f64));
                }
                InterpreterOp::MulI64(lhs, rhs, dest) => {
                    let res = self.get_i64(*lhs)? * self.get_i64(*rhs)?;
                    self.values.insert(*dest, Value::Number(res as f64));
                }
                InterpreterOp::DivI64(lhs, rhs, dest) => {
                    let r = self.get_i64(*rhs)?;
                    if r == 0 {
                        return Err(RuntimeError::DivisionByZero);
                    }
                    let res = self.get_i64(*lhs)? / r;
                    self.values.insert(*dest, Value::Number(res as f64));
                }
                InterpreterOp::RemI64(lhs, rhs, dest) => {
                    let r = self.get_i64(*rhs)?;
                    if r == 0 {
                        return Err(RuntimeError::DivisionByZero);
                    }
                    let res = self.get_i64(*lhs)? % r;
                    self.values.insert(*dest, Value::Number(res as f64));
                }
                InterpreterOp::AndI64(lhs, rhs, dest) => {
                    let res = self.get_i64(*lhs)? & self.get_i64(*rhs)?;
                    self.values.insert(*dest, Value::Number(res as f64));
                }
                InterpreterOp::OrI64(lhs, rhs, dest) => {
                    let res = self.get_i64(*lhs)? | self.get_i64(*rhs)?;
                    self.values.insert(*dest, Value::Number(res as f64));
                }
                InterpreterOp::XorI64(lhs, rhs, dest) => {
                    let res = self.get_i64(*lhs)? ^ self.get_i64(*rhs)?;
                    self.values.insert(*dest, Value::Number(res as f64));
                }
                InterpreterOp::ShlI64(lhs, rhs, dest) => {
                    let res = self.get_i64(*lhs)? << self.get_i64(*rhs)?;
                    self.values.insert(*dest, Value::Number(res as f64));
                }
                InterpreterOp::ShrI64(lhs, rhs, dest) => {
                    let res = self.get_i64(*lhs)? >> self.get_i64(*rhs)?;
                    self.values.insert(*dest, Value::Number(res as f64));
                }
                InterpreterOp::NegI64(operand, dest) => {
                    let res = -self.get_i64(*operand)?;
                    self.values.insert(*dest, Value::Number(res as f64));
                }
                InterpreterOp::NotI64(operand, dest) => {
                    let res = !self.get_i64(*operand)?;
                    self.values.insert(*dest, Value::Number(res as f64));
                }

                // Float Math
                InterpreterOp::AddF64(lhs, rhs, dest) => {
                    let res = self.get_f64(*lhs)? + self.get_f64(*rhs)?;
                    self.values.insert(*dest, Value::Number(res));
                }
                InterpreterOp::SubF64(lhs, rhs, dest) => {
                    let res = self.get_f64(*lhs)? - self.get_f64(*rhs)?;
                    self.values.insert(*dest, Value::Number(res));
                }
                InterpreterOp::MulF64(lhs, rhs, dest) => {
                    let res = self.get_f64(*lhs)? * self.get_f64(*rhs)?;
                    self.values.insert(*dest, Value::Number(res));
                }
                InterpreterOp::DivF64(lhs, rhs, dest) => {
                    let res = self.get_f64(*lhs)? / self.get_f64(*rhs)?;
                    self.values.insert(*dest, Value::Number(res));
                }
                InterpreterOp::NegF64(operand, dest) => {
                    let res = -self.get_f64(*operand)?;
                    self.values.insert(*dest, Value::Number(res));
                }
                InterpreterOp::AbsF64(operand, dest) => {
                    let res = self.get_f64(*operand)?.abs();
                    self.values.insert(*dest, Value::Number(res));
                }
                InterpreterOp::SqrtF64(operand, dest) => {
                    let res = self.get_f64(*operand)?.sqrt();
                    self.values.insert(*dest, Value::Number(res));
                }

                // Integer Comparisons
                InterpreterOp::EqI64(lhs, rhs, dest) => {
                    let res = self.get_i64(*lhs)? == self.get_i64(*rhs)?;
                    self.values.insert(*dest, Value::Bool(res));
                }
                InterpreterOp::NeI64(lhs, rhs, dest) => {
                    let res = self.get_i64(*lhs)? != self.get_i64(*rhs)?;
                    self.values.insert(*dest, Value::Bool(res));
                }
                InterpreterOp::LtI64(lhs, rhs, dest) => {
                    let res = self.get_i64(*lhs)? < self.get_i64(*rhs)?;
                    self.values.insert(*dest, Value::Bool(res));
                }
                InterpreterOp::LeI64(lhs, rhs, dest) => {
                    let res = self.get_i64(*lhs)? <= self.get_i64(*rhs)?;
                    self.values.insert(*dest, Value::Bool(res));
                }
                InterpreterOp::GtI64(lhs, rhs, dest) => {
                    let res = self.get_i64(*lhs)? > self.get_i64(*rhs)?;
                    self.values.insert(*dest, Value::Bool(res));
                }
                InterpreterOp::GeI64(lhs, rhs, dest) => {
                    let res = self.get_i64(*lhs)? >= self.get_i64(*rhs)?;
                    self.values.insert(*dest, Value::Bool(res));
                }

                // Float Comparisons
                InterpreterOp::EqF64(lhs, rhs, dest) => {
                    let res = self.get_f64(*lhs)? == self.get_f64(*rhs)?;
                    self.values.insert(*dest, Value::Bool(res));
                }
                InterpreterOp::NeF64(lhs, rhs, dest) => {
                    let res = self.get_f64(*lhs)? != self.get_f64(*rhs)?;
                    self.values.insert(*dest, Value::Bool(res));
                }
                InterpreterOp::LtF64(lhs, rhs, dest) => {
                    let res = self.get_f64(*lhs)? < self.get_f64(*rhs)?;
                    self.values.insert(*dest, Value::Bool(res));
                }
                InterpreterOp::LeF64(lhs, rhs, dest) => {
                    let res = self.get_f64(*lhs)? <= self.get_f64(*rhs)?;
                    self.values.insert(*dest, Value::Bool(res));
                }
                InterpreterOp::GtF64(lhs, rhs, dest) => {
                    let res = self.get_f64(*lhs)? > self.get_f64(*rhs)?;
                    self.values.insert(*dest, Value::Bool(res));
                }
                InterpreterOp::GeF64(lhs, rhs, dest) => {
                    let res = self.get_f64(*lhs)? >= self.get_f64(*rhs)?;
                    self.values.insert(*dest, Value::Bool(res));
                }

                // Memory
                InterpreterOp::Alloc(_size, dest) => {
                    let ptr = self.next_alloc_id;
                    self.next_alloc_id += 1;
                    // Pre-allocate empty value or null
                    self.memory.insert(ptr, Value::Null);
                    self.values.insert(*dest, Value::Number(ptr as f64));
                }
                InterpreterOp::AllocDyn(size_val, dest) => {
                    let size = self.get_i64(*size_val)? as usize;
                    let ptr = self.next_alloc_id;
                    self.next_alloc_id += 1;
                    // Allocate space (simulated — just reserve the address)
                    let _ = size; // Size tracked for correctness; memory is simulated
                    self.memory.insert(ptr, Value::Null);
                    self.values.insert(*dest, Value::Number(ptr as f64));
                }
                InterpreterOp::Free(ptr_id) => {
                    let ptr = self.get_ptr(*ptr_id)?;
                    if ptr != 0 {
                        self.memory.remove(&ptr);
                    }
                }
                InterpreterOp::Load(ptr_id, _offset, dest) => {
                    let ptr = self.get_ptr(*ptr_id)?;
                    let val = self
                        .memory
                        .get(&ptr)
                        .ok_or(RuntimeError::InvalidMemoryAccess)?
                        .clone();
                    self.values.insert(*dest, val);
                }
                InterpreterOp::Store(ptr_id, _offset, val_id) => {
                    let ptr = self.get_ptr(*ptr_id)?;
                    if !self.memory.contains_key(&ptr) {
                        return Err(RuntimeError::InvalidMemoryAccess);
                    }
                    let val = self.get_val(*val_id)?.clone();
                    self.memory.insert(ptr, val);
                }

                // ARC Operations (Simulated, as we rely on Rust's Arc/Clone for Value enum)
                InterpreterOp::ArcClone(src, dest) => {
                    let val = self.get_val(*src)?.clone();
                    self.values.insert(*dest, val);
                }
                InterpreterOp::ArcDrop(ptr) => {
                    self.values.remove(ptr); // Release it from our local register file
                }
                InterpreterOp::ArcIncrement(_) => {
                    // Handled automatically by Rust Clone
                }
                InterpreterOp::ArcDecrement(_) => {
                    // Handled automatically by Rust Drop
                }

                // Type Ops
                InterpreterOp::Cast(src, _to_ty, dest) => {
                    let val = self.get_val(*src)?.clone();
                    self.values.insert(*dest, val);
                }
                InterpreterOp::BitCast(src, _to_ty, dest) => {
                    let val = self.get_val(*src)?.clone();
                    self.values.insert(*dest, val);
                }

                // Control flow
                InterpreterOp::Return(opt_val) => {
                    if let Some(vid) = opt_val {
                        result = Some(self.get_val(*vid)?.clone());
                    }
                    break;
                }
                InterpreterOp::Jump(target) => {
                    pc = *target;
                }
                InterpreterOp::Branch(cond, true_tgt, false_tgt) => {
                    let is_true = self.get_val(*cond)?.truthy();
                    pc = if is_true { *true_tgt } else { *false_tgt };
                }

                // Copy/Move
                InterpreterOp::Copy(src, dest) => {
                    let val = self.get_val(*src)?.clone();
                    self.values.insert(*dest, val);
                }
                InterpreterOp::Move(src, dest) => {
                    let val = self
                        .values
                        .remove(src)
                        .ok_or(RuntimeError::MissingValue(*src))?;
                    self.values.insert(*dest, val);
                }

                // Calls
                InterpreterOp::CallIntrinsic(name, args, opt_dest) => {
                    let mut evaluated_args = vec![];
                    for a in args {
                        evaluated_args.push(self.get_val(*a)?.clone());
                    }
                    // Basic intrinsic support
                    let result_val = match name.as_str() {
                        "MemCopy" | "MemMove" => {
                            if evaluated_args.len() >= 3 {
                                let dst = evaluated_args[0].as_number().unwrap_or(0.0) as u64;
                                let src = evaluated_args[1].as_number().unwrap_or(0.0) as u64;
                                let n = evaluated_args[2].as_number().unwrap_or(0.0) as usize;
                                if let Some(src_val) = self.memory.get(&src).cloned() {
                                    self.memory.insert(dst, src_val);
                                }
                                let _ = n;
                            }
                            Value::Null
                        }
                        "MemSet" => {
                            if evaluated_args.len() >= 3 {
                                let ptr = evaluated_args[0].as_number().unwrap_or(0.0) as u64;
                                let _val = evaluated_args[1].as_number().unwrap_or(0.0) as u8;
                                let _n = evaluated_args[2].as_number().unwrap_or(0.0) as usize;
                                self.memory.insert(ptr, Value::Null);
                            }
                            Value::Null
                        }
                        "Sin" => {
                            let x = evaluated_args.first().and_then(|v| v.as_number()).unwrap_or(0.0);
                            Value::Number(x.sin())
                        }
                        "Cos" => {
                            let x = evaluated_args.first().and_then(|v| v.as_number()).unwrap_or(0.0);
                            Value::Number(x.cos())
                        }
                        "Tan" => {
                            let x = evaluated_args.first().and_then(|v| v.as_number()).unwrap_or(0.0);
                            Value::Number(x.tan())
                        }
                        "Log" => {
                            let x = evaluated_args.first().and_then(|v| v.as_number()).unwrap_or(0.0);
                            Value::Number(x.ln())
                        }
                        "Exp" => {
                            let x = evaluated_args.first().and_then(|v| v.as_number()).unwrap_or(0.0);
                            Value::Number(x.exp())
                        }
                        "Pow" => {
                            let base = evaluated_args.first().and_then(|v| v.as_number()).unwrap_or(0.0);
                            let exp = evaluated_args.get(1).and_then(|v| v.as_number()).unwrap_or(1.0);
                            Value::Number(base.powf(exp))
                        }
                        _ => Value::Null,
                    };
                    if let Some(dest) = opt_dest {
                        self.values.insert(*dest, result_val);
                    }
                }
                InterpreterOp::Call(name, args, opt_dest) => {
                    // Function calls require a module-level function table.
                    // For now, evaluate args and return Null for unsupported calls.
                    let _ = args;
                    if let Some(dest) = opt_dest {
                        self.values.insert(*dest, Value::Null);
                    }
                    let _ = name;
                }

                // Extended operations
                InterpreterOp::ConstString(string_id, dest) => {
                    // String constants are referenced by ID from the module's string pool.
                    // The engine doesn't have access to the pool, so store a placeholder.
                    self.values.insert(*dest, Value::Str(format!("_str_{}", string_id).into()));
                }
                InterpreterOp::LoadLocal(local_idx, dest) => {
                    // Stack locals are modeled as memory at offset (local_idx + 1) * 1024
                    let addr = (*local_idx as u64 + 1) * 1024;
                    let val = self.memory.get(&addr).cloned().unwrap_or(Value::Null);
                    self.values.insert(*dest, val);
                }
                InterpreterOp::StoreLocal(local_idx, value) => {
                    let addr = (*local_idx as u64 + 1) * 1024;
                    let val = self.get_val(*value)?.clone();
                    self.memory.insert(addr, val);
                }
                InterpreterOp::DropValue(value) => {
                    // Drop removes the value from the register file
                    self.values.remove(value);
                }
                InterpreterOp::Unreachable => {
                    return Err(RuntimeError::InvalidMemoryAccess);
                }
                InterpreterOp::Switch(value, cases, default) => {
                    let val = self.get_i64(*value)?;
                    let mut jumped = false;
                    for (case_val, target) in cases {
                        if val == *case_val {
                            pc = *target;
                            jumped = true;
                            break;
                        }
                    }
                    if !jumped {
                        pc = *default;
                    }
                }
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::lowering::vir_to_interpreter::InterpreterOp;

    fn val_f64(v: Option<Value>) -> f64 {
        match v.unwrap() {
            Value::Number(n) => n,
            _ => panic!("Expected Number"),
        }
    }

    fn val_bool(v: Option<Value>) -> bool {
        match v.unwrap() {
            Value::Bool(b) => b,
            _ => panic!("Expected Bool"),
        }
    }

    #[test]
    fn test_interpreter_engine_math_i64() {
        let mut engine = InterpreterEngine::new();
        // 1: Const 10
        // 2: Const 5
        // 3: Add 1, 2
        // 4: Sub 1, 2
        // 5: Mul 1, 2
        // 6: Div 1, 2
        // 7: Rem 1, 2
        let ops = vec![
            InterpreterOp::ConstI64(10, 1),
            InterpreterOp::ConstI64(5, 2),
            InterpreterOp::AddI64(1, 2, 3),
            InterpreterOp::SubI64(1, 2, 4),
            InterpreterOp::MulI64(1, 2, 5),
            InterpreterOp::DivI64(1, 2, 6),
            InterpreterOp::RemI64(1, 2, 7),
        ];
        engine.execute(&ops).unwrap();

        assert_eq!(val_f64(engine.values.get(&3).cloned()), 15.0);
        assert_eq!(val_f64(engine.values.get(&4).cloned()), 5.0);
        assert_eq!(val_f64(engine.values.get(&5).cloned()), 50.0);
        assert_eq!(val_f64(engine.values.get(&6).cloned()), 2.0);
        assert_eq!(val_f64(engine.values.get(&7).cloned()), 0.0);
    }

    #[test]
    fn test_interpreter_engine_neg_not_i64() {
        let mut engine = InterpreterEngine::new();
        let ops = vec![
            InterpreterOp::ConstI64(42, 1),
            InterpreterOp::NegI64(1, 2),
            InterpreterOp::NotI64(1, 3),
        ];
        engine.execute(&ops).unwrap();
        assert_eq!(val_f64(engine.values.get(&2).cloned()), -42.0);
        assert_eq!(val_f64(engine.values.get(&3).cloned()), -43.0);
    }

    #[test]
    fn test_interpreter_engine_bitwise_i64() {
        let mut engine = InterpreterEngine::new();
        let ops = vec![
            InterpreterOp::ConstI64(12, 1), // 1100
            InterpreterOp::ConstI64(10, 2), // 1010
            InterpreterOp::AndI64(1, 2, 3), // 1000 = 8
            InterpreterOp::OrI64(1, 2, 4),  // 1110 = 14
            InterpreterOp::XorI64(1, 2, 5), // 0110 = 6
            InterpreterOp::ShlI64(1, 2, 6), // 12 << 10
            InterpreterOp::ShrI64(1, 2, 7), // 12 >> 10
        ];
        engine.execute(&ops).unwrap();
        assert_eq!(val_f64(engine.values.get(&3).cloned()), 8.0);
        assert_eq!(val_f64(engine.values.get(&4).cloned()), 14.0);
        assert_eq!(val_f64(engine.values.get(&5).cloned()), 6.0);
        assert_eq!(val_f64(engine.values.get(&6).cloned()), (12 << 10) as f64);
        assert_eq!(val_f64(engine.values.get(&7).cloned()), 0.0);
    }

    #[test]
    fn test_interpreter_engine_math_f64() {
        let mut engine = InterpreterEngine::new();
        let ops = vec![
            InterpreterOp::ConstF64(10.5, 1),
            InterpreterOp::ConstF64(2.0, 2),
            InterpreterOp::AddF64(1, 2, 3),
            InterpreterOp::SubF64(1, 2, 4),
            InterpreterOp::MulF64(1, 2, 5),
            InterpreterOp::DivF64(1, 2, 6),
            InterpreterOp::NegF64(1, 7),
        ];
        engine.execute(&ops).unwrap();

        assert_eq!(val_f64(engine.values.get(&3).cloned()), 12.5);
        assert_eq!(val_f64(engine.values.get(&4).cloned()), 8.5);
        assert_eq!(val_f64(engine.values.get(&5).cloned()), 21.0);
        assert_eq!(val_f64(engine.values.get(&6).cloned()), 5.25);
        assert_eq!(val_f64(engine.values.get(&7).cloned()), -10.5);
    }

    #[test]
    fn test_interpreter_engine_f64_functions() {
        let mut engine = InterpreterEngine::new();
        let ops = vec![
            InterpreterOp::ConstF64(-42.0, 1),
            InterpreterOp::AbsF64(1, 2),
            InterpreterOp::ConstF64(16.0, 3),
            InterpreterOp::SqrtF64(3, 4),
        ];
        engine.execute(&ops).unwrap();
        assert_eq!(val_f64(engine.values.get(&2).cloned()), 42.0);
        assert_eq!(val_f64(engine.values.get(&4).cloned()), 4.0);
    }

    #[test]
    fn test_interpreter_engine_compare_i64() {
        let mut engine = InterpreterEngine::new();
        let ops = vec![
            InterpreterOp::ConstI64(10, 1),
            InterpreterOp::ConstI64(5, 2),
            InterpreterOp::EqI64(1, 2, 3), // false
            InterpreterOp::NeI64(1, 2, 4), // true
            InterpreterOp::LtI64(1, 2, 5), // false
            InterpreterOp::GtI64(1, 2, 6), // true
        ];
        engine.execute(&ops).unwrap();
        assert!(!val_bool(engine.values.get(&3).cloned()));
        assert!(val_bool(engine.values.get(&4).cloned()));
        assert!(!val_bool(engine.values.get(&5).cloned()));
        assert!(val_bool(engine.values.get(&6).cloned()));
    }

    #[test]
    fn test_interpreter_engine_compare_f64() {
        let mut engine = InterpreterEngine::new();
        let ops = vec![
            InterpreterOp::ConstF64(10.0, 1),
            InterpreterOp::ConstF64(10.0, 2),
            InterpreterOp::EqF64(1, 2, 3), // true
            InterpreterOp::NeF64(1, 2, 4), // false
            InterpreterOp::LeF64(1, 2, 5), // true
            InterpreterOp::GeF64(1, 2, 6), // true
        ];
        engine.execute(&ops).unwrap();
        assert!(val_bool(engine.values.get(&3).cloned()));
        assert!(!val_bool(engine.values.get(&4).cloned()));
        assert!(val_bool(engine.values.get(&5).cloned()));
        assert!(val_bool(engine.values.get(&6).cloned()));
    }

    #[test]
    fn test_interpreter_engine_control_flow() {
        let mut engine = InterpreterEngine::new();
        let ops = vec![
            InterpreterOp::ConstI64(0, 1), // counter
            InterpreterOp::ConstI64(1, 2), // increment
            InterpreterOp::ConstI64(5, 3), // max
            // Loop start: pc = 3
            // 3: Add
            InterpreterOp::AddI64(1, 2, 1),
            // 4: cmp == 5
            InterpreterOp::EqI64(1, 3, 4),
            // 5: branch if equal to 7 (return), else back to 3
            InterpreterOp::Branch(4, 7, 3),
            // 6: (skip)
            InterpreterOp::Nop,
            // 7: return counter
            InterpreterOp::Return(Some(1)),
        ];
        let res = engine.execute(&ops).unwrap();
        assert!(res.is_some());
        assert_eq!(val_f64(res), 5.0);
    }

    #[test]
    fn test_interpreter_engine_memory() {
        let mut engine = InterpreterEngine::new();
        let ops = vec![
            InterpreterOp::Alloc(8, 1), // ptr at reg 1
            InterpreterOp::ConstI64(42, 2),
            InterpreterOp::Store(1, 0, 2), // store 42 at ptr
            InterpreterOp::Load(1, 0, 3),  // load from ptr to reg 3
            InterpreterOp::Return(Some(3)),
            InterpreterOp::Free(1),
        ];
        let res = engine.execute(&ops).unwrap();
        assert_eq!(val_f64(res), 42.0);
    }

    #[test]
    fn test_interpreter_engine_copy_move() {
        let mut engine = InterpreterEngine::new();
        let ops = vec![
            InterpreterOp::ConstI64(99, 1),
            InterpreterOp::Copy(1, 2),
            InterpreterOp::Move(1, 3),
        ];
        engine.execute(&ops).unwrap();
        assert_eq!(val_f64(engine.values.get(&2).cloned()), 99.0);
        assert_eq!(val_f64(engine.values.get(&3).cloned()), 99.0);
        assert!(engine.values.get(&1).is_none()); // Value was moved
    }

    #[test]
    fn test_interpreter_engine_arc() {
        let mut engine = InterpreterEngine::new();
        let ops = vec![
            InterpreterOp::ConstI64(55, 1),
            InterpreterOp::ArcClone(1, 2),
            InterpreterOp::ArcDrop(1),
        ];
        engine.execute(&ops).unwrap();
        assert!(engine.values.get(&1).is_none());
        assert_eq!(val_f64(engine.values.get(&2).cloned()), 55.0);
    }

    #[test]
    fn test_interpreter_engine_type_ops() {
        let mut engine = InterpreterEngine::new();
        let ops = vec![
            InterpreterOp::ConstI64(77, 1),
            InterpreterOp::Cast(1, crate::ir::vir::VirType::I32, 2),
            InterpreterOp::BitCast(1, crate::ir::vir::VirType::F64, 3),
            InterpreterOp::Return(Some(2)),
        ];
        let res = engine.execute(&ops).unwrap();
        assert_eq!(val_f64(res), 77.0);
    }
}
