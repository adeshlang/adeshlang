//! Expression fusion — combine chained array operations into single pass
//!
//! Example: sqrt((a * b) + c) → one fused loop instead of 3 temporary arrays

use crate::parsing::ast::Value;

/// Fused array operation chain
#[derive(Debug, Clone)]
pub enum FusedOp {
    LoadA,
    LoadB,
    LoadC,
    LoadD,
    Add,
    Sub,
    Mul,
    Div,
    Sqrt,
    Abs,
    Neg,
    Store,
}

/// A fused operation sequence
#[derive(Debug, Clone)]
pub struct FusedExpr {
    pub ops: Vec<FusedOp>,
    pub inputs: Vec<String>,
    pub output: String,
}

impl FusedExpr {
    /// sqrt((a * b) + c) fusion
    pub fn sqrt_mul_add(a: impl Into<String>, b: impl Into<String>, c: impl Into<String>, out: impl Into<String>) -> Self {
        FusedExpr {
            ops: vec![
                FusedOp::LoadA,
                FusedOp::LoadB,
                FusedOp::Mul,
                FusedOp::LoadC,
                FusedOp::Add,
                FusedOp::Sqrt,
                FusedOp::Store,
            ],
            inputs: vec![a.into(), b.into(), c.into()],
            output: out.into(),
        }
    }

    /// (a * b) + d fusion (SAXPY-like)
    pub fn mul_add(a: impl Into<String>, b: impl Into<String>, d: impl Into<String>, out: impl Into<String>) -> Self {
        FusedExpr {
            ops: vec![
                FusedOp::LoadA,
                FusedOp::LoadB,
                FusedOp::Mul,
                FusedOp::LoadD,
                FusedOp::Add,
                FusedOp::Store,
            ],
            inputs: vec![a.into(), b.into(), d.into()],
            output: out.into(),
        }
    }
}

/// Execute a fused expression over arrays — single pass, no temporaries
pub fn execute_fused(
    expr: &FusedExpr,
    arrays: &std::collections::HashMap<String, Vec<Value>>,
) -> Result<Vec<Value>, String> {
    let len = arrays
        .values()
        .next()
        .map(|a| a.len())
        .unwrap_or(0);

    let mut result = Vec::with_capacity(len);

    for i in 0..len {
        let mut stack: Vec<f64> = Vec::with_capacity(4);

        for op in &expr.ops {
            match op {
                FusedOp::LoadA => stack.push(value_to_f64(&arrays["a"][i])),
                FusedOp::LoadB => stack.push(value_to_f64(&arrays["b"][i])),
                FusedOp::LoadC => stack.push(value_to_f64(&arrays["c"][i])),
                FusedOp::LoadD => stack.push(value_to_f64(&arrays["d"][i])),
                FusedOp::Add => {
                    let b = stack.pop().unwrap_or(0.0);
                    let a = stack.pop().unwrap_or(0.0);
                    stack.push(a + b);
                }
                FusedOp::Sub => {
                    let b = stack.pop().unwrap_or(0.0);
                    let a = stack.pop().unwrap_or(0.0);
                    stack.push(a - b);
                }
                FusedOp::Mul => {
                    let b = stack.pop().unwrap_or(0.0);
                    let a = stack.pop().unwrap_or(0.0);
                    stack.push(a * b);
                }
                FusedOp::Div => {
                    let b = stack.pop().unwrap_or(0.0);
                    let a = stack.pop().unwrap_or(0.0);
                    stack.push(if b == 0.0 { f64::NAN } else { a / b });
                }
                FusedOp::Sqrt => {
                    let a = stack.pop().unwrap_or(0.0);
                    stack.push(a.sqrt());
                }
                FusedOp::Abs => {
                    let a = stack.pop().unwrap_or(0.0);
                    stack.push(a.abs());
                }
                FusedOp::Neg => {
                    let a = stack.pop().unwrap_or(0.0);
                    stack.push(-a);
                }
                FusedOp::Store => {}
            }
        }

        result.push(Value::Number(stack.pop().unwrap_or(0.0)));
    }

    Ok(result)
}

/// Fused element-wise: c[i] = a[i] * b[i] + d[i]
pub fn fused_mul_add(
    a: &[Value],
    b: &[Value],
    d: &[Value],
) -> Result<Vec<Value>, String> {
    if a.len() != b.len() || a.len() != d.len() {
        return Err(format!(
            "fused_mul_add: length mismatch ({} vs {} vs {})",
            a.len(),
            b.len(),
            d.len()
        ));
    }
    let mut result = Vec::with_capacity(a.len());
    for i in 0..a.len() {
        let av = value_to_f64(&a[i]);
        let bv = value_to_f64(&b[i]);
        let dv = value_to_f64(&d[i]);
        result.push(Value::Number(av * bv + dv));
    }
    Ok(result)
}

/// Fused: result[i] = sqrt(a[i] * b[i] + c[i])
pub fn fused_sqrt_mul_add(
    a: &[Value],
    b: &[Value],
    c: &[Value],
) -> Result<Vec<Value>, String> {
    if a.len() != b.len() || a.len() != c.len() {
        return Err("fused_sqrt_mul_add: length mismatch".to_string());
    }
    let mut result = Vec::with_capacity(a.len());
    for i in 0..a.len() {
        let v = value_to_f64(&a[i]) * value_to_f64(&b[i]) + value_to_f64(&c[i]);
        result.push(Value::Number(v.sqrt()));
    }
    Ok(result)
}

fn value_to_f64(v: &Value) -> f64 {
    match v {
        Value::Number(n) => *n,
        Value::F64(n) => *n,
        Value::F32(n) => *n as f64,
        Value::I64(n) => *n as f64,
        Value::I32(n) => *n as f64,
        Value::U64(n) => *n as f64,
        Value::U32(n) => *n as f64,
        Value::I8(n) => *n as f64,
        Value::U8(n) => *n as f64,
        _ => 0.0,
    }
}
