//! Bytecode VM Executor - Phase 7.2
//!
//! This module implements a stack-based virtual machine for executing bytecode
//! generated in Phase 2.4.3. It uses a stack-oriented instruction architecture.

use super::interpreter_executor::{InterpreterValue, MemoryAllocator};
use std::collections::HashMap;

/// Bytecode instruction set
#[derive(Debug, Clone, PartialEq)]
pub enum BytecodeInstr {
    // Constants
    PushInt(i64),
    PushFloat(f64),
    PushBool(bool),
    PushNull,
    PushString(String),

    // Arithmetic
    AddInt,
    SubInt,
    MulInt,
    DivInt,
    RemInt,

    AddFloat,
    SubFloat,
    MulFloat,
    DivFloat,

    // Bitwise
    And,
    Or,
    Xor,
    Not,
    Shl,
    Shr,

    // Comparison
    EqInt,
    NeInt,
    LtInt,
    LeInt,
    GtInt,
    GeInt,

    EqFloat,
    NeFloat,
    LtFloat,
    LeFloat,
    GtFloat,
    GeFloat,

    // Stack operations
    Dup,
    Pop,
    Swap,
    Over,

    // Type casting
    CastToInt,
    CastToFloat,
    CastToBool,
    CastToString,

    // Variable operations
    SetLocal(usize),
    GetLocal(usize),
    SetGlobal(String),
    GetGlobal(String),

    // Array operations
    ArrayNew,
    ArrayLen,
    ArrayGet,
    ArraySet,

    // Struct operations
    StructNew(String),
    StructGet(String),
    StructSet(String),

    // Control flow
    Jump(usize),
    JumpIfTrue(usize),
    JumpIfFalse(usize),
    Call(String, usize), // function_name, num_args
    Return,

    // Memory
    Alloc(usize),
    Free,

    // I/O
    Print,
    PrintLn,

    // Exception handling
    Try,
    Catch(usize), // jump to catch block
    Throw(String),

    // Halt
    Halt,
}

/// Exception frame for exception handling
#[derive(Debug, Clone)]
struct ExceptionFrame {
    catch_addr: usize,
    stack_depth: usize,
}

/// Bytecode virtual machine
pub struct BytecodeVM {
    /// Instructions
    instructions: Vec<BytecodeInstr>,

    /// Instruction pointer
    ip: usize,

    /// Value stack
    stack: Vec<InterpreterValue>,

    /// Local variables (per frame)
    locals: Vec<HashMap<usize, InterpreterValue>>,

    /// Global variables
    globals: HashMap<String, InterpreterValue>,

    /// Memory allocator
    memory: MemoryAllocator,

    /// Exception handlers
    exception_stack: Vec<ExceptionFrame>,

    /// Current exception
    exception: Option<String>,

    /// Halt flag
    halted: bool,
}

impl BytecodeVM {
    /// Create a new bytecode VM
    pub fn new(instructions: Vec<BytecodeInstr>) -> Self {
        Self {
            instructions,
            ip: 0,
            stack: Vec::new(),
            locals: vec![HashMap::new()],
            globals: HashMap::new(),
            memory: MemoryAllocator::new(),
            exception_stack: Vec::new(),
            exception: None,
            halted: false,
        }
    }

    /// Push value onto stack
    fn push(&mut self, value: InterpreterValue) {
        self.stack.push(value);
    }

    /// Pop value from stack
    fn pop(&mut self) -> Option<InterpreterValue> {
        self.stack.pop()
    }

    /// Peek at top of stack
    fn peek(&self) -> Option<&InterpreterValue> {
        self.stack.last()
    }

    /// Get current stack size
    fn stack_size(&self) -> usize {
        self.stack.len()
    }

    /// Execute a single instruction
    fn execute_instr(&mut self, instr: &BytecodeInstr) -> Result<(), String> {
        match instr {
            // Constants
            BytecodeInstr::PushInt(v) => {
                self.push(InterpreterValue::Int(*v));
            }
            BytecodeInstr::PushFloat(v) => {
                self.push(InterpreterValue::Float(*v));
            }
            BytecodeInstr::PushBool(v) => {
                self.push(InterpreterValue::Bool(*v));
            }
            BytecodeInstr::PushNull => {
                self.push(InterpreterValue::Null);
            }
            BytecodeInstr::PushString(s) => {
                self.push(InterpreterValue::String(s.clone()));
            }

            // Arithmetic - Int
            BytecodeInstr::AddInt => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                let result = a.as_int().wrapping_add(b.as_int());
                self.push(InterpreterValue::Int(result));
            }
            BytecodeInstr::SubInt => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                let result = a.as_int().wrapping_sub(b.as_int());
                self.push(InterpreterValue::Int(result));
            }
            BytecodeInstr::MulInt => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                let result = a.as_int().wrapping_mul(b.as_int());
                self.push(InterpreterValue::Int(result));
            }
            BytecodeInstr::DivInt => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                let result = if b.as_int() != 0 {
                    a.as_int() / b.as_int()
                } else {
                    return Err("Division by zero".to_string());
                };
                self.push(InterpreterValue::Int(result));
            }
            BytecodeInstr::RemInt => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                let result = if b.as_int() != 0 {
                    a.as_int() % b.as_int()
                } else {
                    return Err("Remainder by zero".to_string());
                };
                self.push(InterpreterValue::Int(result));
            }

            // Arithmetic - Float
            BytecodeInstr::AddFloat => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                self.push(InterpreterValue::Float(a.as_float() + b.as_float()));
            }
            BytecodeInstr::SubFloat => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                self.push(InterpreterValue::Float(a.as_float() - b.as_float()));
            }
            BytecodeInstr::MulFloat => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                self.push(InterpreterValue::Float(a.as_float() * b.as_float()));
            }
            BytecodeInstr::DivFloat => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                self.push(InterpreterValue::Float(a.as_float() / b.as_float()));
            }

            // Bitwise
            BytecodeInstr::And => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                self.push(InterpreterValue::Int(a.as_int() & b.as_int()));
            }
            BytecodeInstr::Or => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                self.push(InterpreterValue::Int(a.as_int() | b.as_int()));
            }
            BytecodeInstr::Xor => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                self.push(InterpreterValue::Int(a.as_int() ^ b.as_int()));
            }
            BytecodeInstr::Not => {
                let a = self.pop().ok_or("Stack underflow")?;
                self.push(InterpreterValue::Int(!a.as_int()));
            }
            BytecodeInstr::Shl => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                let count = b.as_int();
                if count < 0 || count >= 64 {
                    return Err("invalid shift amount: expected 0 <= count < 64".to_string());
                }
                self.push(InterpreterValue::Int(a.as_int() << count));
            }
            BytecodeInstr::Shr => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                let count = b.as_int();
                if count < 0 || count >= 64 {
                    return Err("invalid shift amount: expected 0 <= count < 64".to_string());
                }
                self.push(InterpreterValue::Int(a.as_int() >> count));
            }

            // Comparison - Int
            BytecodeInstr::EqInt => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                self.push(InterpreterValue::Bool(a.as_int() == b.as_int()));
            }
            BytecodeInstr::NeInt => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                self.push(InterpreterValue::Bool(a.as_int() != b.as_int()));
            }
            BytecodeInstr::LtInt => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                self.push(InterpreterValue::Bool(a.as_int() < b.as_int()));
            }
            BytecodeInstr::LeInt => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                self.push(InterpreterValue::Bool(a.as_int() <= b.as_int()));
            }
            BytecodeInstr::GtInt => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                self.push(InterpreterValue::Bool(a.as_int() > b.as_int()));
            }
            BytecodeInstr::GeInt => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                self.push(InterpreterValue::Bool(a.as_int() >= b.as_int()));
            }

            // Comparison - Float
            BytecodeInstr::EqFloat => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                let epsilon = f64::EPSILON;
                self.push(InterpreterValue::Bool(
                    (a.as_float() - b.as_float()).abs() < epsilon,
                ));
            }
            BytecodeInstr::NeFloat => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                let epsilon = f64::EPSILON;
                self.push(InterpreterValue::Bool(
                    (a.as_float() - b.as_float()).abs() >= epsilon,
                ));
            }
            BytecodeInstr::LtFloat => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                self.push(InterpreterValue::Bool(a.as_float() < b.as_float()));
            }
            BytecodeInstr::LeFloat => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                self.push(InterpreterValue::Bool(a.as_float() <= b.as_float()));
            }
            BytecodeInstr::GtFloat => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                self.push(InterpreterValue::Bool(a.as_float() > b.as_float()));
            }
            BytecodeInstr::GeFloat => {
                let b = self.pop().ok_or("Stack underflow")?;
                let a = self.pop().ok_or("Stack underflow")?;
                self.push(InterpreterValue::Bool(a.as_float() >= b.as_float()));
            }

            // Stack operations
            BytecodeInstr::Dup => {
                let v = self.peek().ok_or("Stack underflow")?;
                self.push(v.clone());
            }
            BytecodeInstr::Pop => {
                self.pop().ok_or("Stack underflow")?;
            }
            BytecodeInstr::Swap => {
                let len = self.stack.len();
                if len < 2 {
                    return Err("Stack underflow for swap".to_string());
                }
                self.stack.swap(len - 1, len - 2);
            }
            BytecodeInstr::Over => {
                let len = self.stack.len();
                if len < 2 {
                    return Err("Stack underflow for over".to_string());
                }
                let v = self.stack[len - 2].clone();
                self.push(v);
            }

            // Type casting
            BytecodeInstr::CastToInt => {
                let v = self.pop().ok_or("Stack underflow")?;
                self.push(InterpreterValue::Int(v.as_int()));
            }
            BytecodeInstr::CastToFloat => {
                let v = self.pop().ok_or("Stack underflow")?;
                self.push(InterpreterValue::Float(v.as_float()));
            }
            BytecodeInstr::CastToBool => {
                let v = self.pop().ok_or("Stack underflow")?;
                self.push(InterpreterValue::Bool(v.as_bool()));
            }
            BytecodeInstr::CastToString => {
                let v = self.pop().ok_or("Stack underflow")?;
                self.push(InterpreterValue::String(v.as_string()));
            }

            // Variable operations
            BytecodeInstr::SetLocal(idx) => {
                let v = self.pop().ok_or("Stack underflow")?;
                if let Some(locals) = self.locals.last_mut() {
                    locals.insert(*idx, v);
                }
            }
            BytecodeInstr::GetLocal(idx) => {
                if let Some(locals) = self.locals.last() {
                    if let Some(v) = locals.get(idx) {
                        self.push(v.clone());
                    } else {
                        self.push(InterpreterValue::Null);
                    }
                }
            }
            BytecodeInstr::SetGlobal(name) => {
                let v = self.pop().ok_or("Stack underflow")?;
                self.globals.insert(name.clone(), v);
            }
            BytecodeInstr::GetGlobal(name) => {
                if let Some(v) = self.globals.get(name) {
                    self.push(v.clone());
                } else {
                    self.push(InterpreterValue::Null);
                }
            }

            // Control flow
            BytecodeInstr::Jump(addr) => {
                self.ip = *addr;
                return Ok(()); // Skip normal IP increment
            }
            BytecodeInstr::JumpIfTrue(addr) => {
                let v = self.pop().ok_or("Stack underflow")?;
                if v.as_bool() {
                    self.ip = *addr;
                    return Ok(());
                }
            }
            BytecodeInstr::JumpIfFalse(addr) => {
                let v = self.pop().ok_or("Stack underflow")?;
                if !v.as_bool() {
                    self.ip = *addr;
                    return Ok(());
                }
            }
            BytecodeInstr::Call(_, num_args) => {
                // Push new local scope
                self.locals.push(HashMap::new());
                // Arguments are already on stack
                let _args = (0..*num_args)
                    .filter_map(|_| self.pop())
                    .collect::<Vec<_>>();
            }
            BytecodeInstr::Return => {
                // Pop local scope
                if self.locals.len() > 1 {
                    self.locals.pop();
                }
            }

            // Memory
            BytecodeInstr::Alloc(size) => {
                let addr = self.memory.alloc(*size);
                self.push(InterpreterValue::Pointer(addr));
            }
            BytecodeInstr::Free => {
                if let Some(InterpreterValue::Pointer(addr)) = self.pop() {
                    self.memory.free(addr);
                }
            }

            // I/O
            BytecodeInstr::Print => {
                if let Some(v) = self.peek() {
                    print!("{}", v.as_string());
                }
            }
            BytecodeInstr::PrintLn => {
                if let Some(v) = self.pop() {
                    println!("{}", v.as_string());
                }
            }

            // Exception handling
            BytecodeInstr::Try => {
                self.exception_stack.push(ExceptionFrame {
                    catch_addr: 0, // Will be set by Catch
                    stack_depth: self.stack_size(),
                });
            }
            BytecodeInstr::Catch(addr) => {
                if !self.exception_stack.is_empty() {
                    self.exception_stack.last_mut().unwrap().catch_addr = *addr;
                }
                if self.exception.is_some() {
                    self.ip = *addr;
                    return Ok(());
                }
            }
            BytecodeInstr::Throw(msg) => {
                self.exception = Some(msg.clone());
                if let Some(frame) = self.exception_stack.pop() {
                    if frame.catch_addr != 0 {
                        self.ip = frame.catch_addr;
                        // Reset stack to frame state
                        while self.stack.len() > frame.stack_depth {
                            self.stack.pop();
                        }
                        return Ok(());
                    }
                }
                return Err(msg.clone());
            }

            // Halt
            BytecodeInstr::Halt => {
                self.halted = true;
            }

            // Array operations (stub)
            BytecodeInstr::ArrayNew => {
                let len = self.pop().ok_or("Stack underflow")?;
                let arr = vec![InterpreterValue::Null; len.as_int() as usize];
                self.push(InterpreterValue::Array(arr));
            }
            BytecodeInstr::ArrayLen => {
                if let Some(InterpreterValue::Array(arr)) = self.pop() {
                    self.push(InterpreterValue::Int(arr.len() as i64));
                }
            }
            BytecodeInstr::ArrayGet => {
                let idx = self.pop().ok_or("Stack underflow")?;
                if let Some(InterpreterValue::Array(arr)) = self.pop() {
                    let raw_idx = idx.as_int();
                    let resolved = if raw_idx < 0 {
                        arr.len() as i64 + raw_idx
                    } else {
                        raw_idx
                    };
                    if resolved >= 0 && (resolved as usize) < arr.len() {
                        self.push(arr[resolved as usize].clone());
                    } else {
                        self.push(InterpreterValue::Null);
                    }
                }
            }
            BytecodeInstr::ArraySet => {
                let val = self.pop().ok_or("Stack underflow")?;
                let idx = self.pop().ok_or("Stack underflow")?;
                if let Some(InterpreterValue::Array(mut arr)) = self.pop() {
                    let raw_idx = idx.as_int();
                    let resolved = if raw_idx < 0 {
                        arr.len() as i64 + raw_idx
                    } else {
                        raw_idx
                    };
                    if resolved >= 0 && (resolved as usize) < arr.len() {
                        arr[resolved as usize] = val;
                    }
                    self.push(InterpreterValue::Array(arr));
                }
            }

            // Struct operations (simplified)
            BytecodeInstr::StructNew(_name) => {
                self.push(InterpreterValue::Struct(HashMap::new()));
            }
            BytecodeInstr::StructGet(field) => {
                if let Some(InterpreterValue::Struct(s)) = self.pop() {
                    if let Some(v) = s.get(field) {
                        self.push(v.clone());
                    } else {
                        self.push(InterpreterValue::Null);
                    }
                }
            }
            BytecodeInstr::StructSet(field) => {
                let val = self.pop().ok_or("Stack underflow")?;
                if let Some(InterpreterValue::Struct(mut s)) = self.pop() {
                    s.insert(field.clone(), val);
                    self.push(InterpreterValue::Struct(s));
                }
            }
        }

        self.ip += 1;
        Ok(())
    }

    /// Run the bytecode program
    pub fn run(&mut self) -> Result<InterpreterValue, String> {
        while self.ip < self.instructions.len() && !self.halted {
            let instr = self.instructions[self.ip].clone();
            self.execute_instr(&instr)?;
        }

        if self.halted {
            Ok(self.stack.pop().unwrap_or(InterpreterValue::Null))
        } else {
            Err("Program did not halt".to_string())
        }
    }

    /// Run with step limit (for debugging)
    pub fn run_steps(&mut self, max_steps: usize) -> Result<InterpreterValue, String> {
        let mut steps = 0;
        while self.ip < self.instructions.len() && !self.halted && steps < max_steps {
            let instr = self.instructions[self.ip].clone();
            self.execute_instr(&instr)?;
            steps += 1;
        }

        if self.halted {
            Ok(self.stack.pop().unwrap_or(InterpreterValue::Null))
        } else if steps >= max_steps {
            Err(format!("Max steps ({}) exceeded", max_steps))
        } else {
            Err("Program did not halt".to_string())
        }
    }

    /// Get current stack
    pub fn stack(&self) -> &[InterpreterValue] {
        &self.stack
    }

    /// Get globals
    pub fn globals(&self) -> &HashMap<String, InterpreterValue> {
        &self.globals
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_arithmetic() {
        let instrs = vec![
            BytecodeInstr::PushInt(5),
            BytecodeInstr::PushInt(3),
            BytecodeInstr::AddInt,
            BytecodeInstr::Halt,
        ];
        let mut vm = BytecodeVM::new(instrs);
        let result = vm.run().unwrap();
        assert_eq!(result.as_int(), 8);
    }

    #[test]
    fn test_stack_operations() {
        let instrs = vec![
            BytecodeInstr::PushInt(5),
            BytecodeInstr::Dup,
            BytecodeInstr::AddInt,
            BytecodeInstr::Halt,
        ];
        let mut vm = BytecodeVM::new(instrs);
        let result = vm.run().unwrap();
        assert_eq!(result.as_int(), 10);
    }

    #[test]
    fn test_comparison() {
        let instrs = vec![
            BytecodeInstr::PushInt(5),
            BytecodeInstr::PushInt(3),
            BytecodeInstr::GtInt,
            BytecodeInstr::Halt,
        ];
        let mut vm = BytecodeVM::new(instrs);
        let result = vm.run().unwrap();
        assert!(result.as_bool());
    }

    #[test]
    fn test_conditional_jump() {
        let instrs = vec![
            BytecodeInstr::PushBool(true),
            BytecodeInstr::JumpIfTrue(3),
            BytecodeInstr::PushInt(1),
            BytecodeInstr::Halt,
        ];
        let mut vm = BytecodeVM::new(instrs);
        let result = vm.run().unwrap();
        assert_eq!(result.as_int(), 0); // Jumped over the push
    }

    #[test]
    fn test_variables() {
        let instrs = vec![
            BytecodeInstr::PushInt(42),
            BytecodeInstr::SetLocal(0),
            BytecodeInstr::GetLocal(0),
            BytecodeInstr::Halt,
        ];
        let mut vm = BytecodeVM::new(instrs);
        let result = vm.run().unwrap();
        assert_eq!(result.as_int(), 42);
    }

    #[test]
    fn test_globals() {
        let instrs = vec![
            BytecodeInstr::PushInt(99),
            BytecodeInstr::SetGlobal("x".to_string()),
            BytecodeInstr::GetGlobal("x".to_string()),
            BytecodeInstr::Halt,
        ];
        let mut vm = BytecodeVM::new(instrs);
        let result = vm.run().unwrap();
        assert_eq!(result.as_int(), 99);
    }

    #[test]
    fn test_arrays() {
        let instrs = vec![
            BytecodeInstr::PushInt(3),
            BytecodeInstr::ArrayNew,
            BytecodeInstr::Dup,
            BytecodeInstr::PushInt(0),
            BytecodeInstr::PushInt(42),
            BytecodeInstr::ArraySet,
            BytecodeInstr::PushInt(0),
            BytecodeInstr::ArrayGet,
            BytecodeInstr::Halt,
        ];
        let mut vm = BytecodeVM::new(instrs);
        let result = vm.run().unwrap();
        assert_eq!(result.as_int(), 42);
    }

    #[test]
    fn test_exception_handling() {
        let instrs = vec![
            BytecodeInstr::Try,
            BytecodeInstr::Throw("Test error".to_string()),
            BytecodeInstr::Halt,
        ];
        let mut vm = BytecodeVM::new(instrs);
        let result = vm.run();
        assert!(result.is_err());
    }
}
