//! Broadcasting rules for array operations

use crate::parsing::ast::Value;

/// Broadcast mode for array operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BroadcastMode {
    /// Same-length arrays
    ElementWise,
    /// Array op scalar (broadcast scalar to all elements)
    ScalarRight,
    /// Scalar op array
    ScalarLeft,
    /// Incompatible shapes
    Incompatible,
}

fn is_array(v: &Value) -> bool {
    matches!(v, Value::Array(_) | Value::DynArray(_) | Value::RawArray(_, _))
}

/// Determine broadcast mode for a binary operation
pub fn detect_broadcast(left: &Value, right: &Value) -> BroadcastMode {
    match (left, right) {
        (l, r) if is_array(l) && is_array(r) => BroadcastMode::ElementWise,
        (l, r) if is_array(l) && is_numeric(r) => BroadcastMode::ScalarRight,
        (l, r) if is_numeric(l) && is_array(r) => BroadcastMode::ScalarLeft,
        _ => BroadcastMode::Incompatible,
    }
}

fn is_numeric(v: &Value) -> bool {
    matches!(
        v,
        Value::Number(_)
            | Value::I8(_)
            | Value::I16(_)
            | Value::I32(_)
            | Value::I64(_)
            | Value::I128(_)
            | Value::U8(_)
            | Value::U16(_)
            | Value::U32(_)
            | Value::U64(_)
            | Value::U128(_)
            | Value::F32(_)
            | Value::F64(_)
    )
}

/// Apply scalar broadcast: array op scalar
pub fn broadcast_scalar_right<F>(arr: &[Value], scalar: f64, op: F) -> Vec<Value>
where
    F: Fn(f64, f64) -> f64,
{
    arr.iter()
        .map(|v| {
            let a = value_to_f64(v);
            Value::Number(op(a, scalar))
        })
        .collect()
}

/// Apply scalar broadcast: scalar op array
pub fn broadcast_scalar_left<F>(scalar: f64, arr: &[Value], op: F) -> Vec<Value>
where
    F: Fn(f64, f64) -> f64,
{
    arr.iter()
        .map(|v| {
            let b = value_to_f64(v);
            Value::Number(op(scalar, b))
        })
        .collect()
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
