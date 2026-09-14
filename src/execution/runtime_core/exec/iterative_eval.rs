//! Iterative expression evaluator
//!
//! This module provides an iterative, stack-based implementation of expression evaluation
//! to avoid stack overflow issues with deeply nested expressions.
//!
//! The iterative evaluator uses an explicit work stack instead of relying on the call stack,
//! allowing for much deeper expression nesting without hitting stack limits.
//!
//! This is a reference implementation showing how to eliminate recursion. Currently,
//! the system uses the recursive evaluator with depth protection, but this approach
//! can be enabled for testing or when dealing with extremely deep expressions.

use super::core::Exec;
use crate::parsing::ast::{Expr, ExprKind, Value};

/// A work item on the evaluation stack
#[derive(Debug)]
enum EvalTask {
    /// Evaluate an expression and push result
    Eval(Expr),
    /// Apply binary operation: left and right values are on value stack
    ApplyBinary { op: crate::parsing::ast::TokenKind },
    /// Apply unary operation: value is on value stack
    ApplyUnary(crate::parsing::ast::TokenKind),
    /// Build array from N values on stack
    BuildArray(usize),
    /// Apply function call: callee and args are on value stack
    ApplyCall { arg_count: usize },
    /// Apply index access: target and index are on value stack
    ApplyIndex,
}

impl Exec {
    /// Iterative expression evaluator (experimental)
    ///
    /// This function evaluates expressions using an explicit work stack instead of
    /// recursion, preventing stack overflow on deeply nested expressions.
    ///
    /// NOTE: This is currently a proof-of-concept. For most expressions, we fall back
    /// to the recursive evaluator to maintain compatibility.
    #[allow(dead_code)]
    pub(in crate::execution::runtime_core) fn eval_expr_iterative(
        &mut self,
        initial_expr: &Expr,
    ) -> Result<Value, String> {
        let mut work_stack: Vec<EvalTask> = vec![EvalTask::Eval(initial_expr.clone())];
        let mut value_stack: Vec<Value> = Vec::new();

        while let Some(task) = work_stack.pop() {
            match task {
                EvalTask::Eval(expr) => {
                    match &expr.kind {
                        // Simple cases that don't need decomposition
                        ExprKind::Literal(v) => {
                            value_stack.push(v.clone());
                        }
                        ExprKind::Grouping(inner) => {
                            // Just unwrap the grouping
                            work_stack.push(EvalTask::Eval((**inner).clone()));
                        }

                        // Complex cases that need decomposition
                        ExprKind::Binary(left, op, right) => {
                            // Push apply task first (will execute after operands)
                            work_stack.push(EvalTask::ApplyBinary { op: op.clone() });
                            // Push right operand evaluation
                            work_stack.push(EvalTask::Eval((**right).clone()));
                            // Push left operand evaluation
                            work_stack.push(EvalTask::Eval((**left).clone()));
                        }

                        ExprKind::Unary(op, operand) => {
                            work_stack.push(EvalTask::ApplyUnary(op.clone()));
                            work_stack.push(EvalTask::Eval((**operand).clone()));
                        }

                        ExprKind::Array(elements) => {
                            let count = elements.len();
                            work_stack.push(EvalTask::BuildArray(count));
                            // Push elements in reverse order so they evaluate left-to-right
                            for elem in elements.iter().rev() {
                                work_stack.push(EvalTask::Eval(elem.clone()));
                            }
                        }

                        ExprKind::Call(callee, args, _type_args) => {
                            let arg_count = args.len();
                            work_stack.push(EvalTask::ApplyCall { arg_count });
                            // Push args in reverse
                            for arg in args.iter().rev() {
                                work_stack.push(EvalTask::Eval(arg.clone()));
                            }
                            // Push callee
                            work_stack.push(EvalTask::Eval((**callee).clone()));
                        }

                        ExprKind::Index(target, index) => {
                            work_stack.push(EvalTask::ApplyIndex);
                            work_stack.push(EvalTask::Eval((**index).clone()));
                            work_stack.push(EvalTask::Eval((**target).clone()));
                        }

                        // For other complex expressions, fall back to recursive eval
                        // This is a gradual migration strategy
                        _ => {
                            value_stack.push(self.eval_expr(&expr)?);
                        }
                    }
                }

                EvalTask::ApplyBinary { op } => {
                    // Pop right then left (they were pushed in that order)
                    let right = value_stack.pop().ok_or_else(|| {
                        format!("Stack underflow in binary operation {:?}: missing right operand (stack size: {})", op, value_stack.len())
                    })?;
                    let left = value_stack.pop().ok_or_else(|| {
                        format!("Stack underflow in binary operation {:?}: missing left operand (stack size: {})", op, value_stack.len())
                    })?;

                    // Use unified runtime ABI for consistent semantics
                    use crate::parsing::ast::TokenKind;
                    use crate::runtime::abi::{abi_add, abi_div, abi_mod, abi_mul, abi_sub};
                    use crate::runtime::abi::{
                        abi_cmp_eq, abi_cmp_ge, abi_cmp_gt, abi_cmp_le, abi_cmp_lt, abi_cmp_ne,
                    };

                    let result = match op {
                        TokenKind::Plus => abi_add(&left, &right).map_err(|e| e.message)?,
                        TokenKind::Minus => abi_sub(&left, &right).map_err(|e| e.message)?,
                        TokenKind::Star => abi_mul(&left, &right).map_err(|e| e.message)?,
                        TokenKind::Slash => abi_div(&left, &right).map_err(|e| e.message)?,
                        TokenKind::Percent => abi_mod(&left, &right).map_err(|e| e.message)?,
                        TokenKind::Less => abi_cmp_lt(&left, &right).map_err(|e| e.message)?,
                        TokenKind::LessEqual => abi_cmp_le(&left, &right).map_err(|e| e.message)?,
                        TokenKind::Greater => abi_cmp_gt(&left, &right).map_err(|e| e.message)?,
                        TokenKind::GreaterEqual => {
                            abi_cmp_ge(&left, &right).map_err(|e| e.message)?
                        }
                        TokenKind::EqualEqual => {
                            abi_cmp_eq(&left, &right).map_err(|e| e.message)?
                        }
                        TokenKind::BangEqual => abi_cmp_ne(&left, &right).map_err(|e| e.message)?,
                        _ => {
                            return Err(format!(
                                "Unsupported binary operator in iterative eval: {:?}",
                                op
                            ));
                        }
                    };
                    value_stack.push(result);
                }

                EvalTask::ApplyUnary(op) => {
                    let operand = value_stack.pop().ok_or_else(|| {
                        format!("Stack underflow in unary operation {:?}: missing operand (stack size: {})", op, value_stack.len())
                    })?;
                    use crate::parsing::ast::TokenKind;
                    use crate::runtime::abi::{abi_negate, abi_not};

                    let result = match op {
                        TokenKind::Minus => abi_negate(&operand).map_err(|e| e.message)?,
                        TokenKind::Bang => abi_not(&operand).map_err(|e| e.message)?,
                        _ => {
                            return Err(format!(
                                "Unsupported unary operator in iterative eval: {:?}",
                                op
                            ));
                        }
                    };
                    value_stack.push(result);
                }

                EvalTask::BuildArray(count) => {
                    if value_stack.len() < count {
                        return Err(format!(
                            "Stack underflow in array construction: need {} elements but only {} available",
                            count,
                            value_stack.len()
                        ));
                    }
                    let mut elements = Vec::with_capacity(count);
                    for _ in 0..count {
                        elements.push(value_stack.pop().unwrap());
                    }
                    elements.reverse(); // Restore original order
                    value_stack.push(Value::Array(elements));
                }

                EvalTask::ApplyCall { arg_count } => {
                    // TODO: Implement iterative call evaluation
                    // For now, fall back to recursive eval for calls
                    // This would need significant refactoring to make fully iterative
                    return Err(format!(
                        "Call evaluation with {} args not yet supported in iterative mode - use recursive evaluator",
                        arg_count
                    ));
                }

                EvalTask::ApplyIndex => {
                    let index = value_stack.pop().ok_or_else(|| {
                        format!(
                            "Stack underflow in index operation: missing index (stack size: {})",
                            value_stack.len()
                        )
                    })?;
                    let target = value_stack.pop().ok_or_else(|| {
                        format!(
                            "Stack underflow in index operation: missing target (stack size: {})",
                            value_stack.len()
                        )
                    })?;

                    let result = match (target, index) {
                        (Value::Array(arr), Value::Number(idx)) => {
                            let i = idx as usize;
                            arr.get(i)
                                .cloned()
                                .ok_or_else(|| format!("Index {} out of bounds", i))?
                        }
                        (Value::Str(s), Value::Number(idx)) => {
                            let i = idx as usize;
                            let c = s
                                .chars()
                                .nth(i)
                                .ok_or_else(|| format!("Index {} out of bounds", i))?;
                            Value::Str(c.to_string())
                        }
                        _ => return Err("Invalid index operation".to_string()),
                    };
                    value_stack.push(result);
                }
            }
        }

        // Final result should be the only value on the stack
        value_stack
            .pop()
            .ok_or_else(|| "Evaluation completed but no result on stack".to_string())
    }
}
