// Bytecode-based interpreter optimization layer
// Compiles AST to bytecode once, then interprets bytecode (faster than AST walking)
// Uses unified runtime ABI for arithmetic operations

use crate::parsing::ast::Value;
// Use unified runtime ABI for arithmetic operations
use crate::runtime::abi::abi_add;
use std::collections::HashMap;

/// Lightweight bytecode instruction set
#[derive(Clone, Debug)]
pub enum BytecodeOp {
    // Constants and variables
    Const(Value),     // Push constant onto stack
    LoadVar(String),  // Load variable
    StoreVar(String), // Store to variable

    // Binary operations
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Lt,
    Lte,
    Gt,
    Gte,
    Eq,
    Neq,
    And,
    Or,

    // Unary operations
    Neg,
    Not,

    // Control flow
    JumpIfFalse(usize), // Jump if top of stack is falsy
    Jump(usize),        // Unconditional jump
    Pop,                // Discard top of stack

    // Function calls
    Call(usize),     // Call function with N args
    TailCall(usize), // Tail call: restart current function with N args from stack

    // Array/Object operations
    BuildArray(usize), // Create array from N stack items
    Index,             // Index operation

    // Special
    Return,       // Return from function
    Print(usize), // Print N values
}

/// Compiled bytecode function
#[derive(Clone)]
pub struct BytecodeFunc {
    pub ops: Vec<BytecodeOp>,
    pub constants: Vec<Value>,
    pub var_map: HashMap<String, usize>,
}

/// Bytecode interpreter state
pub struct BytecodeInterpreter {
    stack: Vec<Value>,
    frames: Vec<Frame>,
}

pub struct Frame {
    func: BytecodeFunc,
    pc: usize, // Program counter
    locals: Vec<Value>,
}

impl BytecodeInterpreter {
    pub fn new() -> Self {
        BytecodeInterpreter {
            stack: Vec::with_capacity(256),
            frames: Vec::new(),
        }
    }

    pub fn execute(&mut self, func: BytecodeFunc) -> Result<Value, String> {
        self.frames.push(Frame {
            func,
            pc: 0,
            locals: Vec::new(),
        });

        loop {
            let frame = self.frames.last_mut().ok_or("No active frame")?;

            if frame.pc >= frame.func.ops.len() {
                // Function end
                let _frame = self.frames.pop().ok_or("Frame stack empty")?;
                if self.frames.is_empty() {
                    return Ok(self.stack.last().cloned().unwrap_or(Value::Null));
                }
                continue;
            }

            let op = frame.func.ops[frame.pc].clone();
            frame.pc += 1;

            match op {
                BytecodeOp::Const(v) => {
                    self.stack.push(v);
                }
                BytecodeOp::LoadVar(_name) => {
                    let val = frame.locals.last().cloned().unwrap_or(Value::Null);
                    self.stack.push(val);
                }
                BytecodeOp::StoreVar(_name) => {
                    if let Some(v) = self.stack.pop() {
                        frame.locals.push(v);
                    }
                }
                BytecodeOp::Add => {
                    let b = self.stack.pop().ok_or("Stack underflow")?;
                    let a = self.stack.pop().ok_or("Stack underflow")?;
                    // Use unified runtime ABI for semantic consistency
                    let result = abi_add(&a, &b).map_err(|e| e.message)?;
                    self.stack.push(result);
                }
                BytecodeOp::Return => {
                    let val = self.stack.last().cloned().unwrap_or(Value::Null);
                    let _ = self.frames.pop();
                    if self.frames.is_empty() {
                        return Ok(val);
                    }
                    self.stack.push(val);
                }
                BytecodeOp::TailCall(nargs) => {
                    // TCO: pop nargs from the stack, reset PC to 0,
                    // rebind locals, and restart the current frame.
                    let mut args = Vec::with_capacity(nargs);
                    for _ in 0..nargs {
                        args.push(self.stack.pop().unwrap_or(Value::Null));
                    }
                    args.reverse();
                    let frame = self.frames.last_mut().ok_or("No active frame")?;
                    frame.pc = 0;
                    frame.locals = args;
                    // Continue the loop — will restart the function body
                }
                _ => {}
            }
        }
    }
}

// Note: add_values function removed - now using unified runtime ABI (abi_add)
