//! Tiered execution implementations.
//!
//! Contains the execution logic for each tier (Interpreter, Baseline JIT, Optimizing JIT).

use super::execution_tiers::{ControlFlow, JitFrame};
use crate::backends::builtins::RuntimeValue;
use crate::backends::jit::lir::LirFunction;
use crate::backends::jit::tiered::TieredJitContext;
use crate::utils::collections::FastMap;

impl TieredJitContext {
    /// Internal function execution with optional captures
    pub(in crate::backends::jit::tiered) fn execute_function_internal(
        &mut self,
        func: &LirFunction,
        mut args: Vec<RuntimeValue>,
        captures: Option<&FastMap<String, RuntimeValue>>,
    ) -> Result<RuntimeValue, String> {
        self.stats.total_calls += 1;
        let mut frame = JitFrame::new();
        let mut current_func = func.clone();

        // TCO trampoline loop
        'tco_loop: loop {
            // Bind captured variables first
            if let Some(caps) = captures {
                for (name, value) in caps {
                    frame.set_var(name.clone(), value.clone());
                }
            }

            // Bind arguments to parameters
            for (i, (name, _ty)) in current_func.params.iter().enumerate() {
                let value = args.get(i).cloned().unwrap_or(RuntimeValue::Null);
                frame.set_var(name.clone(), value);
            }

            // Execute blocks
            let mut current_block = current_func.entry_block;

            'block_loop: loop {
                let block = current_func
                    .get_block(current_block)
                    .ok_or_else(|| format!("Block {} not found", current_block))?;

                for inst in &block.instructions {
                    match self.execute_instruction_common(&mut frame, inst, &current_func)? {
                        ControlFlow::Next => {}
                        ControlFlow::Jump(target) => {
                            current_block = target;
                            continue 'block_loop;
                        }
                        ControlFlow::Return(value) => {
                            return Ok(value);
                        }
                        ControlFlow::TailCall(name, new_args) => {
                            // Look up the target function
                            if let Some(target_func) = self.functions.get(&name).cloned() {
                                // Update arguments and restart
                                args = new_args;
                                current_func = target_func;
                                frame = JitFrame::new();
                                continue 'tco_loop;
                            } else {
                                return Err(format!(
                                    "Tail call target function '{}' not found",
                                    name
                                ));
                            }
                        }
                    }
                }
                // If no control flow instruction caused a break, assume end of function
                return Ok(RuntimeValue::Null);
            }
        }
    }

    /// Tier 0: Interpreter (cold code)
    pub(in crate::backends::jit::tiered) fn execute_interpreter(
        &mut self,
        func: &LirFunction,
        mut args: Vec<RuntimeValue>,
    ) -> Result<RuntimeValue, String> {
        self.stats.interpreter_calls += 1;
        let mut frame = JitFrame::new();
        let mut current_func = func.clone();

        // TCO trampoline loop
        'tco_loop: loop {
            // Bind arguments to parameters
            for (i, (name, _ty)) in current_func.params.iter().enumerate() {
                let value = args.get(i).cloned().unwrap_or(RuntimeValue::Null);
                frame.set_var(name.clone(), value);
            }

            // Execute blocks
            let mut current_block = current_func.entry_block;

            'block_loop: loop {
                let block = current_func
                    .get_block(current_block)
                    .ok_or_else(|| format!("Block {} not found", current_block))?;

                for inst in &block.instructions {
                    match self.execute_instruction_common(&mut frame, inst, &current_func)? {
                        ControlFlow::Next => {}
                        ControlFlow::Jump(target) => {
                            current_block = target;
                            continue 'block_loop;
                        }
                        ControlFlow::Return(value) => {
                            return Ok(value);
                        }
                        ControlFlow::TailCall(name, new_args) => {
                            if let Some(target_func) = self.functions.get(&name).cloned() {
                                args = new_args;
                                current_func = target_func;
                                frame = JitFrame::new();
                                continue 'tco_loop;
                            } else {
                                return Err(format!(
                                    "Tail call target function '{}' not found",
                                    name
                                ));
                            }
                        }
                    }
                }
                // If no control flow instruction caused a break, assume end of function
                return Ok(RuntimeValue::Null);
            }
        }
    }

    /// Tier 1: Baseline JIT (fast compile, basic optimizations)
    pub(in crate::backends::jit::tiered) fn execute_baseline(
        &mut self,
        func: &LirFunction,
        mut args: Vec<RuntimeValue>,
    ) -> Result<RuntimeValue, String> {
        self.stats.baseline_calls += 1;
        let mut frame = JitFrame::new_baseline();
        let mut current_func = func.clone();

        // TCO trampoline loop
        'tco_loop: loop {
            // Bind arguments
            for (i, (name, _ty)) in current_func.params.iter().enumerate() {
                let value = args.get(i).cloned().unwrap_or(RuntimeValue::Null);
                frame.set_var(name.clone(), value);
            }

            // Execute with baseline optimizations
            let mut current_block = current_func.entry_block;

            'block_loop: loop {
                let block = current_func
                    .get_block(current_block)
                    .ok_or_else(|| format!("Block {} not found", current_block))?;

                for inst in &block.instructions {
                    match self.execute_instruction_common(&mut frame, inst, &current_func)? {
                        ControlFlow::Next => {}
                        ControlFlow::Jump(target) => {
                            current_block = target;
                            continue 'block_loop;
                        }
                        ControlFlow::Return(value) => {
                            return Ok(value);
                        }
                        ControlFlow::TailCall(name, new_args) => {
                            if let Some(target_func) = self.functions.get(&name).cloned() {
                                args = new_args;
                                current_func = target_func;
                                frame = JitFrame::new_baseline();
                                continue 'tco_loop;
                            } else {
                                return Err(format!(
                                    "Tail call target function '{}' not found",
                                    name
                                ));
                            }
                        }
                    }
                }
                // If no control flow instruction caused a break, assume end of function
                return Ok(RuntimeValue::Null);
            }
        }
    }

    /// Tier 2: Optimizing JIT (aggressive optimization)
    pub(in crate::backends::jit::tiered) fn execute_optimizing(
        &mut self,
        func: &LirFunction,
        mut args: Vec<RuntimeValue>,
    ) -> Result<RuntimeValue, String> {
        self.stats.optimizing_calls += 1;
        let mut frame = JitFrame::new_optimizing();
        let mut current_func = func.clone();

        // TCO trampoline loop
        'tco_loop: loop {
            // Bind arguments
            for (i, (name, _ty)) in current_func.params.iter().enumerate() {
                let value = args.get(i).cloned().unwrap_or(RuntimeValue::Null);
                frame.set_var(name.clone(), value);
            }

            // Execute with aggressive optimizations
            let mut current_block = current_func.entry_block;

            'block_loop: loop {
                let block = current_func
                    .get_block(current_block)
                    .ok_or_else(|| format!("Block {} not found", current_block))?;

                for inst in &block.instructions {
                    match self.execute_instruction_common(&mut frame, inst, &current_func)? {
                        ControlFlow::Next => {}
                        ControlFlow::Jump(target) => {
                            current_block = target;
                            continue 'block_loop;
                        }
                        ControlFlow::Return(value) => {
                            return Ok(value);
                        }
                        ControlFlow::TailCall(name, new_args) => {
                            if let Some(target_func) = self.functions.get(&name).cloned() {
                                args = new_args;
                                current_func = target_func;
                                frame = JitFrame::new_optimizing();
                                continue 'tco_loop;
                            } else {
                                return Err(format!(
                                    "Tail call target function '{}' not found",
                                    name
                                ));
                            }
                        }
                    }
                }
                // If no control flow instruction caused a break, assume end of function
                return Ok(RuntimeValue::Null);
            }
        }
    }

    /// Execute a function with 'this' bound
    pub(in crate::backends::jit::tiered) fn execute_function_with_this(
        &mut self,
        func: &LirFunction,
        mut args: Vec<RuntimeValue>,
        this_val: RuntimeValue,
    ) -> Result<RuntimeValue, String> {
        let mut frame = JitFrame::new();
        let mut current_func = func.clone();

        // TCO trampoline loop
        'tco_loop: loop {
            // Bind 'this'
            frame.set_var("this".to_string(), this_val.clone());

            // Bind arguments to parameters
            for (i, (name, _ty)) in current_func.params.iter().enumerate() {
                let value = args.get(i).cloned().unwrap_or(RuntimeValue::Null);
                frame.set_var(name.clone(), value);
            }

            // Execute blocks
            let mut current_block = current_func.entry_block;

            'block_loop: loop {
                let block = current_func
                    .get_block(current_block)
                    .ok_or_else(|| format!("Block {} not found", current_block))?;

                for inst in &block.instructions {
                    match self.execute_instruction_common(&mut frame, inst, &current_func)? {
                        ControlFlow::Next => {}
                        ControlFlow::Jump(target) => {
                            current_block = target;
                            continue 'block_loop;
                        }
                        ControlFlow::Return(value) => {
                            return Ok(value);
                        }
                        ControlFlow::TailCall(name, new_args) => {
                            if let Some(target_func) = self.functions.get(&name).cloned() {
                                args = new_args;
                                current_func = target_func;
                                frame = JitFrame::new();
                                continue 'tco_loop;
                            } else {
                                return Err(format!(
                                    "Tail call target function '{}' not found",
                                    name
                                ));
                            }
                        }
                    }
                }
                // If no control flow instruction caused a break, assume end of function
                return Ok(RuntimeValue::Null);
            }
        }
    }
}

/// Helper to execute function with captured variables (for closures)
pub(in crate::backends::jit::tiered) fn execute_function_with_captures(
    ctx: &mut TieredJitContext,
    func: &LirFunction,
    args: Vec<RuntimeValue>,
    captures: &FastMap<String, RuntimeValue>,
) -> Result<RuntimeValue, String> {
    ctx.execute_function_internal(func, args, Some(captures))
}
