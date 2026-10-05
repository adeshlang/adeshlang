//! Phase 9 Compile-Time Evaluation (Const Eval) Subsystem.
//!
//! Provides a sandboxed, deterministic interpreter for compile-time constant expressions,
//! constant functions, and data construction with strict resource bounds.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Resource bounds for compile-time evaluation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ConstEvalLimits {
    pub max_steps: usize,
    pub max_memory_bytes: usize,
    pub max_recursion_depth: usize,
    pub max_evaluation_depth: usize,
}

impl Default for ConstEvalLimits {
    fn default() -> Self {
        Self {
            max_steps: 100_000,
            max_memory_bytes: 64 * 1024 * 1024, // 64 MB
            max_recursion_depth: 512,
            max_evaluation_depth: 256,
        }
    }
}

/// Constant evaluation error conditions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstEvalError {
    StepLimitExceeded(usize),
    MemoryLimitExceeded(usize),
    RecursionLimitExceeded(usize),
    EvaluationDepthExceeded(usize),
    DivisionByZero,
    TypeMismatch(String),
    UndefinedVariable(String),
    UndefinedFunction(String),
    AssertionFailed(String),
}

impl std::fmt::Display for ConstEvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConstEvalError::StepLimitExceeded(lim) => {
                write!(f, "Const eval exceeded maximum step limit ({})", lim)
            }
            ConstEvalError::MemoryLimitExceeded(lim) => {
                write!(
                    f,
                    "Const eval exceeded maximum memory limit ({} bytes)",
                    lim
                )
            }
            ConstEvalError::RecursionLimitExceeded(lim) => {
                write!(f, "Const eval exceeded maximum recursion depth ({})", lim)
            }
            ConstEvalError::EvaluationDepthExceeded(lim) => {
                write!(f, "Const eval exceeded maximum evaluation depth ({})", lim)
            }
            ConstEvalError::DivisionByZero => write!(f, "Division by zero in const expression"),
            ConstEvalError::TypeMismatch(msg) => write!(f, "Type mismatch in const eval: {}", msg),
            ConstEvalError::UndefinedVariable(v) => write!(f, "Undefined const variable '{}'", v),
            ConstEvalError::UndefinedFunction(func) => {
                write!(f, "Undefined const function '{}'", func)
            }
            ConstEvalError::AssertionFailed(msg) => write!(f, "Const assertion failed: {}", msg),
        }
    }
}

impl std::error::Error for ConstEvalError {}

/// Represents a compile-time value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConstValue {
    Integer(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Array(Vec<ConstValue>),
    Struct {
        name: String,
        fields: HashMap<String, ConstValue>,
    },
    Unit,
}

impl ConstValue {
    pub fn as_i64(&self) -> Result<i64, ConstEvalError> {
        match self {
            ConstValue::Integer(i) => Ok(*i),
            _ => Err(ConstEvalError::TypeMismatch("Expected integer".to_string())),
        }
    }

    pub fn as_bool(&self) -> Result<bool, ConstEvalError> {
        match self {
            ConstValue::Bool(b) => Ok(*b),
            _ => Err(ConstEvalError::TypeMismatch("Expected boolean".to_string())),
        }
    }

    pub fn approximate_size(&self) -> usize {
        match self {
            ConstValue::Integer(_)
            | ConstValue::Float(_)
            | ConstValue::Bool(_)
            | ConstValue::Unit => 8,
            ConstValue::String(s) => 24 + s.len(),
            ConstValue::Array(arr) => 24 + arr.iter().map(|v| v.approximate_size()).sum::<usize>(),
            ConstValue::Struct { fields, .. } => {
                48 + fields
                    .iter()
                    .map(|(k, v)| k.len() + v.approximate_size())
                    .sum::<usize>()
            }
        }
    }
}

/// Abstract Syntax Representation of a Const Expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConstExpr {
    Literal(ConstValue),
    Variable(String),
    Add(Box<ConstExpr>, Box<ConstExpr>),
    Sub(Box<ConstExpr>, Box<ConstExpr>),
    Mul(Box<ConstExpr>, Box<ConstExpr>),
    Div(Box<ConstExpr>, Box<ConstExpr>),
    Mod(Box<ConstExpr>, Box<ConstExpr>),
    BitAnd(Box<ConstExpr>, Box<ConstExpr>),
    BitOr(Box<ConstExpr>, Box<ConstExpr>),
    BitXor(Box<ConstExpr>, Box<ConstExpr>),
    Shl(Box<ConstExpr>, Box<ConstExpr>),
    Shr(Box<ConstExpr>, Box<ConstExpr>),
    Eq(Box<ConstExpr>, Box<ConstExpr>),
    Lt(Box<ConstExpr>, Box<ConstExpr>),
    Gt(Box<ConstExpr>, Box<ConstExpr>),
    Not(Box<ConstExpr>),
    IfThenElse {
        cond: Box<ConstExpr>,
        then_branch: Box<ConstExpr>,
        else_branch: Box<ConstExpr>,
    },
    Call {
        func: String,
        args: Vec<ConstExpr>,
    },
    Array(Vec<ConstExpr>),
    ArrayIndex {
        array: Box<ConstExpr>,
        index: Box<ConstExpr>,
    },
}

/// Constant function definition.
#[derive(Debug, Clone)]
pub struct ConstFunction {
    pub name: String,
    pub params: Vec<String>,
    pub body: ConstExpr,
}

/// Const Evaluation Engine.
pub struct ConstEvaluator {
    limits: ConstEvalLimits,
    step_count: usize,
    allocated_memory: usize,
    call_depth: usize,
    functions: HashMap<String, ConstFunction>,
}

impl ConstEvaluator {
    pub fn new(limits: ConstEvalLimits) -> Self {
        Self {
            limits,
            step_count: 0,
            allocated_memory: 0,
            call_depth: 0,
            functions: HashMap::new(),
        }
    }

    pub fn register_function(&mut self, func: ConstFunction) {
        self.functions.insert(func.name.clone(), func);
    }

    fn tick_step(&mut self) -> Result<(), ConstEvalError> {
        self.step_count += 1;
        if self.step_count > self.limits.max_steps {
            Err(ConstEvalError::StepLimitExceeded(self.limits.max_steps))
        } else {
            Ok(())
        }
    }

    pub fn eval(
        &mut self,
        expr: &ConstExpr,
        env: &HashMap<String, ConstValue>,
    ) -> Result<ConstValue, ConstEvalError> {
        self.eval_with_depth(expr, env, 0)
    }

    fn eval_with_depth(
        &mut self,
        expr: &ConstExpr,
        env: &HashMap<String, ConstValue>,
        depth: usize,
    ) -> Result<ConstValue, ConstEvalError> {
        self.tick_step()?;
        if depth > self.limits.max_evaluation_depth {
            return Err(ConstEvalError::EvaluationDepthExceeded(
                self.limits.max_evaluation_depth,
            ));
        }

        match expr {
            ConstExpr::Literal(val) => Ok(val.clone()),
            ConstExpr::Variable(var) => env
                .get(var)
                .cloned()
                .ok_or_else(|| ConstEvalError::UndefinedVariable(var.clone())),
            ConstExpr::Add(lhs, rhs) => {
                let l = self.eval_with_depth(lhs, env, depth + 1)?;
                let r = self.eval_with_depth(rhs, env, depth + 1)?;
                match (l, r) {
                    (ConstValue::Integer(a), ConstValue::Integer(b)) => {
                        Ok(ConstValue::Integer(a.wrapping_add(b)))
                    }
                    (ConstValue::Float(a), ConstValue::Float(b)) => Ok(ConstValue::Float(a + b)),
                    (ConstValue::String(a), ConstValue::String(b)) => {
                        let res = format!("{}{}", a, b);
                        Ok(ConstValue::String(res))
                    }
                    _ => Err(ConstEvalError::TypeMismatch(
                        "Unsupported operands for +".to_string(),
                    )),
                }
            }
            ConstExpr::Sub(lhs, rhs) => {
                let l = self.eval_with_depth(lhs, env, depth + 1)?;
                let r = self.eval_with_depth(rhs, env, depth + 1)?;
                match (l, r) {
                    (ConstValue::Integer(a), ConstValue::Integer(b)) => {
                        Ok(ConstValue::Integer(a.wrapping_sub(b)))
                    }
                    (ConstValue::Float(a), ConstValue::Float(b)) => Ok(ConstValue::Float(a - b)),
                    _ => Err(ConstEvalError::TypeMismatch(
                        "Unsupported operands for -".to_string(),
                    )),
                }
            }
            ConstExpr::Mul(lhs, rhs) => {
                let l = self.eval_with_depth(lhs, env, depth + 1)?;
                let r = self.eval_with_depth(rhs, env, depth + 1)?;
                match (l, r) {
                    (ConstValue::Integer(a), ConstValue::Integer(b)) => {
                        Ok(ConstValue::Integer(a.wrapping_mul(b)))
                    }
                    (ConstValue::Float(a), ConstValue::Float(b)) => Ok(ConstValue::Float(a * b)),
                    _ => Err(ConstEvalError::TypeMismatch(
                        "Unsupported operands for *".to_string(),
                    )),
                }
            }
            ConstExpr::Div(lhs, rhs) => {
                let l = self.eval_with_depth(lhs, env, depth + 1)?;
                let r = self.eval_with_depth(rhs, env, depth + 1)?;
                match (l, r) {
                    (ConstValue::Integer(a), ConstValue::Integer(b)) => {
                        if b == 0 {
                            Err(ConstEvalError::DivisionByZero)
                        } else {
                            Ok(ConstValue::Integer(a / b))
                        }
                    }
                    (ConstValue::Float(a), ConstValue::Float(b)) => {
                        if b == 0.0 {
                            Err(ConstEvalError::DivisionByZero)
                        } else {
                            Ok(ConstValue::Float(a / b))
                        }
                    }
                    _ => Err(ConstEvalError::TypeMismatch(
                        "Unsupported operands for /".to_string(),
                    )),
                }
            }
            ConstExpr::Mod(lhs, rhs) => {
                let l = self.eval_with_depth(lhs, env, depth + 1)?;
                let r = self.eval_with_depth(rhs, env, depth + 1)?;
                match (l, r) {
                    (ConstValue::Integer(a), ConstValue::Integer(b)) => {
                        if b == 0 {
                            Err(ConstEvalError::DivisionByZero)
                        } else {
                            Ok(ConstValue::Integer(a % b))
                        }
                    }
                    _ => Err(ConstEvalError::TypeMismatch(
                        "Unsupported operands for %".to_string(),
                    )),
                }
            }
            ConstExpr::BitAnd(lhs, rhs) => {
                let a = self.eval_with_depth(lhs, env, depth + 1)?.as_i64()?;
                let b = self.eval_with_depth(rhs, env, depth + 1)?.as_i64()?;
                Ok(ConstValue::Integer(a & b))
            }
            ConstExpr::BitOr(lhs, rhs) => {
                let a = self.eval_with_depth(lhs, env, depth + 1)?.as_i64()?;
                let b = self.eval_with_depth(rhs, env, depth + 1)?.as_i64()?;
                Ok(ConstValue::Integer(a | b))
            }
            ConstExpr::BitXor(lhs, rhs) => {
                let a = self.eval_with_depth(lhs, env, depth + 1)?.as_i64()?;
                let b = self.eval_with_depth(rhs, env, depth + 1)?.as_i64()?;
                Ok(ConstValue::Integer(a ^ b))
            }
            ConstExpr::Shl(lhs, rhs) => {
                let a = self.eval_with_depth(lhs, env, depth + 1)?.as_i64()?;
                let b = self.eval_with_depth(rhs, env, depth + 1)?.as_i64()?;
                Ok(ConstValue::Integer(a.wrapping_shl(b as u32)))
            }
            ConstExpr::Shr(lhs, rhs) => {
                let a = self.eval_with_depth(lhs, env, depth + 1)?.as_i64()?;
                let b = self.eval_with_depth(rhs, env, depth + 1)?.as_i64()?;
                Ok(ConstValue::Integer(a.wrapping_shr(b as u32)))
            }
            ConstExpr::Eq(lhs, rhs) => {
                let l = self.eval_with_depth(lhs, env, depth + 1)?;
                let r = self.eval_with_depth(rhs, env, depth + 1)?;
                Ok(ConstValue::Bool(l == r))
            }
            ConstExpr::Lt(lhs, rhs) => {
                let a = self.eval_with_depth(lhs, env, depth + 1)?.as_i64()?;
                let b = self.eval_with_depth(rhs, env, depth + 1)?.as_i64()?;
                Ok(ConstValue::Bool(a < b))
            }
            ConstExpr::Gt(lhs, rhs) => {
                let a = self.eval_with_depth(lhs, env, depth + 1)?.as_i64()?;
                let b = self.eval_with_depth(rhs, env, depth + 1)?.as_i64()?;
                Ok(ConstValue::Bool(a > b))
            }
            ConstExpr::Not(sub) => {
                let val = self.eval_with_depth(sub, env, depth + 1)?.as_bool()?;
                Ok(ConstValue::Bool(!val))
            }
            ConstExpr::IfThenElse {
                cond,
                then_branch,
                else_branch,
            } => {
                let condition = self.eval_with_depth(cond, env, depth + 1)?.as_bool()?;
                if condition {
                    self.eval_with_depth(then_branch, env, depth + 1)
                } else {
                    self.eval_with_depth(else_branch, env, depth + 1)
                }
            }
            ConstExpr::Call { func, args } => {
                if self.call_depth >= self.limits.max_recursion_depth {
                    return Err(ConstEvalError::RecursionLimitExceeded(
                        self.limits.max_recursion_depth,
                    ));
                }

                let function = self
                    .functions
                    .get(func)
                    .cloned()
                    .ok_or_else(|| ConstEvalError::UndefinedFunction(func.clone()))?;

                if function.params.len() != args.len() {
                    return Err(ConstEvalError::TypeMismatch(format!(
                        "Function '{}' expects {} arguments, got {}",
                        func,
                        function.params.len(),
                        args.len()
                    )));
                }

                let mut new_env = HashMap::new();
                for (param, arg_expr) in function.params.iter().zip(args.iter()) {
                    let arg_val = self.eval_with_depth(arg_expr, env, depth + 1)?;
                    new_env.insert(param.clone(), arg_val);
                }

                self.call_depth += 1;
                let result = self.eval_with_depth(&function.body, &new_env, depth + 1);
                self.call_depth -= 1;
                result
            }
            ConstExpr::Array(elements) => {
                let mut vals = Vec::new();
                for el in elements {
                    vals.push(self.eval_with_depth(el, env, depth + 1)?);
                }
                Ok(ConstValue::Array(vals))
            }
            ConstExpr::ArrayIndex { array, index } => {
                let arr_val = self.eval_with_depth(array, env, depth + 1)?;
                let idx_val = self.eval_with_depth(index, env, depth + 1)?.as_i64()?;
                match arr_val {
                    ConstValue::Array(arr) => {
                        if idx_val < 0 || (idx_val as usize) >= arr.len() {
                            Err(ConstEvalError::AssertionFailed(format!(
                                "Index out of bounds in const eval: idx={}, len={}",
                                idx_val,
                                arr.len()
                            )))
                        } else {
                            Ok(arr[idx_val as usize].clone())
                        }
                    }
                    _ => Err(ConstEvalError::TypeMismatch("Expected array".to_string())),
                }
            }
        }
    }
}
