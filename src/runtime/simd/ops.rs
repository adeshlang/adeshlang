//! SIMD-optimized array operations
//!
//! Fast paths for element-wise numeric array operations.
//! Uses native SIMD when profitable, scalar fallback otherwise.

use crate::parsing::ast::Value;
use crate::runtime::simd::broadcast::{broadcast_scalar_left, broadcast_scalar_right, BroadcastMode, detect_broadcast};
use crate::runtime::simd::cpu_features::cpu_features;
use crate::runtime::simd::fusion::{fused_mul_add, fused_sqrt_mul_add};

/// Extract f64 slice from numeric array for SIMD processing
fn extract_f64_slice(arr: &[Value]) -> Option<Vec<f64>> {
  let mut data = Vec::with_capacity(arr.len());
  for v in arr {
    match v {
      Value::Number(n) => data.push(*n),
      Value::F64(n) => data.push(*n),
      Value::F32(n) => data.push(*n as f64),
      Value::I64(n) => data.push(*n as f64),
      Value::I32(n) => data.push(*n as f64),
      Value::U64(n) => data.push(*n as f64),
      Value::U32(n) => data.push(*n as f64),
      Value::I8(n) => data.push(*n as f64),
      Value::U8(n) => data.push(*n as f64),
      Value::I16(n) => data.push(*n as f64),
      Value::U16(n) => data.push(*n as f64),
      _ => return None,
    }
  }
  Some(data)
}

fn f64_to_values(data: Vec<f64>) -> Vec<Value> {
  data.into_iter().map(Value::Number).collect()
}

/// SIMD-vectorized add for f64 arrays (4-wide unrolled)
fn simd_add_f64(a: &[f64], b: &[f64]) -> Vec<f64> {
  let lanes = cpu_features().simd_lanes_f64().max(1);
  let mut result = vec![0.0; a.len()];
  let simd_end = a.len() - (a.len() % lanes);

  // SIMD-width chunks
  for i in (0..simd_end).step_by(lanes) {
    for j in 0..lanes {
      result[i + j] = a[i + j] + b[i + j];
    }
  }
  // Scalar remainder
  for i in simd_end..a.len() {
    result[i] = a[i] + b[i];
  }
  result
}

fn simd_mul_f64(a: &[f64], b: &[f64]) -> Vec<f64> {
  let lanes = cpu_features().simd_lanes_f64().max(1);
  let mut result = vec![0.0; a.len()];
  let simd_end = a.len() - (a.len() % lanes);
  for i in (0..simd_end).step_by(lanes) {
    for j in 0..lanes {
      result[i + j] = a[i + j] * b[i + j];
    }
  }
  for i in simd_end..a.len() {
    result[i] = a[i] * b[i];
  }
  result
}

fn simd_sub_f64(a: &[f64], b: &[f64]) -> Vec<f64> {
  a.iter().zip(b.iter()).map(|(x, y)| x - y).collect()
}

fn simd_div_f64(a: &[f64], b: &[f64]) -> Vec<f64> {
  a.iter()
    .zip(b.iter())
    .map(|(x, y)| if *y == 0.0 { f64::NAN } else { x / y })
    .collect()
}

/// Element-wise array addition (NumPy-style, not concatenation)
pub fn array_add(left: &Value, right: &Value) -> Result<Value, String> {
  match detect_broadcast(left, right) {
    BroadcastMode::ElementWise => {
      let (la, ra) = extract_arrays(left, right)?;
      if la.len() != ra.len() {
        return Err(format!(
          "array length mismatch: {} vs {}",
          la.len(),
          ra.len()
        ));
      }
      if let (Some(a), Some(b)) = (extract_f64_slice(&la), extract_f64_slice(&ra)) {
        return Ok(Value::Array(f64_to_values(simd_add_f64(&a, &b))));
      }
      element_wise(&la, &ra, |a, b| numeric_add(a, b))
    }
    BroadcastMode::ScalarRight => {
      let arr = extract_single_array(left)?;
      let scalar = extract_scalar(right)?;
      if let Some(data) = extract_f64_slice(&arr) {
        let result: Vec<f64> = data.iter().map(|x| x + scalar).collect();
        return Ok(Value::Array(f64_to_values(result)));
      }
      Ok(Value::Array(broadcast_scalar_right(&arr, scalar, |a, b| a + b)))
    }
    BroadcastMode::ScalarLeft => {
      let arr = extract_single_array(right)?;
      let scalar = extract_scalar(left)?;
      if let Some(data) = extract_f64_slice(&arr) {
        let result: Vec<f64> = data.iter().map(|x| scalar + x).collect();
        return Ok(Value::Array(f64_to_values(result)));
      }
      Ok(Value::Array(broadcast_scalar_left(scalar, &arr, |a, b| a + b)))
    }
    BroadcastMode::Incompatible => Err("incompatible types for array addition".to_string()),
  }
}

/// Element-wise array subtraction
pub fn array_sub(left: &Value, right: &Value) -> Result<Value, String> {
  match detect_broadcast(left, right) {
    BroadcastMode::ElementWise => {
      let (la, ra) = extract_arrays(left, right)?;
      if la.len() != ra.len() {
        return Err("array length mismatch".to_string());
      }
      if let (Some(a), Some(b)) = (extract_f64_slice(&la), extract_f64_slice(&ra)) {
        return Ok(Value::Array(f64_to_values(simd_sub_f64(&a, &b))));
      }
      element_wise(&la, &ra, |a, b| numeric_sub(a, b))
    }
    BroadcastMode::ScalarRight => {
      let arr = extract_single_array(left)?;
      let scalar = extract_scalar(right)?;
      Ok(Value::Array(broadcast_scalar_right(&arr, scalar, |a, b| a - b)))
    }
    BroadcastMode::ScalarLeft => {
      let arr = extract_single_array(right)?;
      let scalar = extract_scalar(left)?;
      Ok(Value::Array(broadcast_scalar_left(scalar, &arr, |a, b| a - b)))
    }
    BroadcastMode::Incompatible => Err("incompatible types for array subtraction".to_string()),
  }
}

/// Element-wise array multiplication
pub fn array_mul(left: &Value, right: &Value) -> Result<Value, String> {
  match detect_broadcast(left, right) {
    BroadcastMode::ElementWise => {
      let (la, ra) = extract_arrays(left, right)?;
      if la.len() != ra.len() {
        return Err("array length mismatch".to_string());
      }
      if let (Some(a), Some(b)) = (extract_f64_slice(&la), extract_f64_slice(&ra)) {
        return Ok(Value::Array(f64_to_values(simd_mul_f64(&a, &b))));
      }
      element_wise(&la, &ra, |a, b| numeric_mul(a, b))
    }
    BroadcastMode::ScalarRight => {
      let arr = extract_single_array(left)?;
      let scalar = extract_scalar(right)?;
      if let Some(data) = extract_f64_slice(&arr) {
        let result: Vec<f64> = data.iter().map(|x| x * scalar).collect();
        return Ok(Value::Array(f64_to_values(result)));
      }
      Ok(Value::Array(broadcast_scalar_right(&arr, scalar, |a, b| a * b)))
    }
    BroadcastMode::ScalarLeft => {
      let arr = extract_single_array(right)?;
      let scalar = extract_scalar(left)?;
      Ok(Value::Array(broadcast_scalar_left(scalar, &arr, |a, b| a * b)))
    }
    BroadcastMode::Incompatible => Err("incompatible types for array multiplication".to_string()),
  }
}

/// Element-wise array division
pub fn array_div(left: &Value, right: &Value) -> Result<Value, String> {
  match detect_broadcast(left, right) {
    BroadcastMode::ElementWise => {
      let (la, ra) = extract_arrays(left, right)?;
      if la.len() != ra.len() {
        return Err("array length mismatch".to_string());
      }
      if let (Some(a), Some(b)) = (extract_f64_slice(&la), extract_f64_slice(&ra)) {
        return Ok(Value::Array(f64_to_values(simd_div_f64(&a, &b))));
      }
      element_wise(&la, &ra, |a, b| numeric_div(a, b))
    }
    BroadcastMode::ScalarRight => {
      let arr = extract_single_array(left)?;
      let scalar = extract_scalar(right)?;
      Ok(Value::Array(broadcast_scalar_right(
        &arr,
        scalar,
        |a, b| if b == 0.0 { f64::NAN } else { a / b },
      )))
    }
    BroadcastMode::ScalarLeft => {
      let arr = extract_single_array(right)?;
      let scalar = extract_scalar(left)?;
      Ok(Value::Array(broadcast_scalar_left(
        scalar,
        &arr,
        |a, b| if b == 0.0 { f64::NAN } else { a / b },
      )))
    }
    BroadcastMode::Incompatible => Err("incompatible types for array division".to_string()),
  }
}

/// Array sum reduction
pub fn array_sum(arr: &Value) -> Result<Value, String> {
  let data = extract_single_array(arr)?;
  if let Some(f64_data) = extract_f64_slice(&data) {
    let lanes = cpu_features().simd_lanes_f64().max(1);
    let mut acc = 0.0f64;
    let simd_end = f64_data.len() - (f64_data.len() % lanes);
    for i in (0..simd_end).step_by(lanes) {
      for j in 0..lanes {
        acc += f64_data[i + j];
      }
    }
    for i in simd_end..f64_data.len() {
      acc += f64_data[i];
    }
    return Ok(Value::Number(acc));
  }
  let mut acc = 0.0f64;
  for v in &data {
    acc += value_to_f64(v);
  }
  Ok(Value::Number(acc))
}

/// Array mean
pub fn array_mean(arr: &Value) -> Result<Value, String> {
  let data = extract_single_array(arr)?;
  if data.is_empty() {
    return Ok(Value::Number(f64::NAN));
  }
  let sum = array_sum(arr)?;
  if let Value::Number(s) = sum {
    Ok(Value::Number(s / data.len() as f64))
  } else {
    Err("mean: unexpected sum type".to_string())
  }
}

/// Array min
pub fn array_min(arr: &Value) -> Result<Value, String> {
  let data = extract_single_array(arr)?;
  if data.is_empty() {
    return Err("min of empty array".to_string());
  }
  let mut min = f64::INFINITY;
  for v in &data {
    min = min.min(value_to_f64(v));
  }
  Ok(Value::Number(min))
}

/// Array max
pub fn array_max(arr: &Value) -> Result<Value, String> {
  let data = extract_single_array(arr)?;
  if data.is_empty() {
    return Err("max of empty array".to_string());
  }
  let mut max = f64::NEG_INFINITY;
  for v in &data {
    max = max.max(value_to_f64(v));
  }
  Ok(Value::Number(max))
}

/// Element-wise abs
pub fn array_abs(arr: &Value) -> Result<Value, String> {
  let data = extract_single_array(arr)?;
  Ok(Value::Array(
    data.iter()
      .map(|v| Value::Number(value_to_f64(v).abs()))
      .collect(),
  ))
}

/// Element-wise sqrt
pub fn array_sqrt(arr: &Value) -> Result<Value, String> {
  let data = extract_single_array(arr)?;
  Ok(Value::Array(
    data.iter()
      .map(|v| Value::Number(value_to_f64(v).sqrt()))
      .collect(),
  ))
}

/// Dot product of two arrays
pub fn array_dot(a: &Value, b: &Value) -> Result<Value, String> {
  let (la, ra) = extract_arrays(a, b)?;
  if la.len() != ra.len() {
    return Err("dot product: length mismatch".to_string());
  }
  if let (Some(a_data), Some(b_data)) = (extract_f64_slice(&la), extract_f64_slice(&ra)) {
    let lanes = cpu_features().simd_lanes_f64().max(1);
    let mut acc = 0.0f64;
    let simd_end = a_data.len() - (a_data.len() % lanes);
    for i in (0..simd_end).step_by(lanes) {
      for j in 0..lanes {
        acc += a_data[i + j] * b_data[i + j];
      }
    }
    for i in simd_end..a_data.len() {
      acc += a_data[i] * b_data[i];
    }
    return Ok(Value::Number(acc));
  }
  let mut acc = 0.0;
  for (x, y) in la.iter().zip(ra.iter()) {
    acc += value_to_f64(x) * value_to_f64(y);
  }
  Ok(Value::Number(acc))
}

/// Fused: c = a * b + d
pub fn array_fused_mul_add(a: &Value, b: &Value, d: &Value) -> Result<Value, String> {
  let la = extract_single_array(a)?;
  let lb = extract_single_array(b)?;
  let ld = extract_single_array(d)?;
  Ok(Value::Array(fused_mul_add(&la, &lb, &ld)?))
}

/// Fused: result = sqrt(a * b + c)
pub fn array_fused_sqrt_mul_add(a: &Value, b: &Value, c: &Value) -> Result<Value, String> {
  let la = extract_single_array(a)?;
  let lb = extract_single_array(b)?;
  let lc = extract_single_array(c)?;
  Ok(Value::Array(fused_sqrt_mul_add(&la, &lb, &lc)?))
}

// --- Helpers ---

fn extract_arrays<'a>(left: &'a Value, right: &'a Value) -> Result<(Vec<Value>, Vec<Value>), String> {
  let la = extract_single_array(left)?;
  let ra = extract_single_array(right)?;
  Ok((la, ra))
}

fn extract_single_array(v: &Value) -> Result<Vec<Value>, String> {
  match v {
    Value::Array(a) => Ok(a.clone()),
    Value::DynArray(d) => Ok(d.data.clone()),
    Value::RawArray(_, a) => Ok(a.clone()),
    _ => Err("expected array".to_string()),
  }
}

fn extract_scalar(v: &Value) -> Result<f64, String> {
  Ok(value_to_f64(v))
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

fn element_wise<F>(a: &[Value], b: &[Value], op: F) -> Result<Value, String>
where
  F: Fn(&Value, &Value) -> Result<Value, String>,
{
  let mut result = Vec::with_capacity(a.len());
  for (x, y) in a.iter().zip(b.iter()) {
    result.push(op(x, y)?);
  }
  Ok(Value::Array(result))
}

fn numeric_add(a: &Value, b: &Value) -> Result<Value, String> {
  Ok(Value::Number(value_to_f64(a) + value_to_f64(b)))
}

fn numeric_sub(a: &Value, b: &Value) -> Result<Value, String> {
  Ok(Value::Number(value_to_f64(a) - value_to_f64(b)))
}

fn numeric_mul(a: &Value, b: &Value) -> Result<Value, String> {
  Ok(Value::Number(value_to_f64(a) * value_to_f64(b)))
}

fn numeric_div(a: &Value, b: &Value) -> Result<Value, String> {
  let b = value_to_f64(b);
  Ok(Value::Number(if b == 0.0 {
    f64::NAN
  } else {
    value_to_f64(a) / b
  }))
}
