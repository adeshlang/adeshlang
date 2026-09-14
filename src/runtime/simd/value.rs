//! SIMD value representation at runtime

use crate::ir::simd::types::{SimdElement, SimdType};

/// Runtime SIMD vector value — lives on stack, no heap allocation
#[derive(Debug, Clone, PartialEq)]
pub struct SimdValue {
    pub simd_type: SimdType,
  pub lanes: Vec<f64>, // unified f64 storage; typed ops cast as needed
}

impl SimdValue {
    pub fn new(simd_type: SimdType) -> Self {
        let lanes = vec![0.0; simd_type.lanes as usize];
        SimdValue { simd_type, lanes }
    }

    pub fn from_lanes(simd_type: SimdType, lanes: Vec<f64>) -> Self {
        debug_assert_eq!(lanes.len(), simd_type.lanes as usize);
        SimdValue { simd_type, lanes }
    }

    pub fn splat(simd_type: SimdType, value: f64) -> Self {
        SimdValue {
            simd_type,
            lanes: vec![value; simd_type.lanes as usize],
        }
    }

    pub fn lane_count(&self) -> usize {
        self.lanes.len()
    }

    // --- Arithmetic ---
    pub fn add(&self, other: &SimdValue) -> Result<SimdValue, String> {
        self.binary_op(other, |a, b| a + b)
    }

    pub fn sub(&self, other: &SimdValue) -> Result<SimdValue, String> {
        self.binary_op(other, |a, b| a - b)
    }

    pub fn mul(&self, other: &SimdValue) -> Result<SimdValue, String> {
        self.binary_op(other, |a, b| a * b)
    }

    pub fn div(&self, other: &SimdValue) -> Result<SimdValue, String> {
        self.binary_op(other, |a, b| {
            if b == 0.0 {
                f64::NAN
            } else {
                a / b
            }
        })
    }

    pub fn abs(&self) -> SimdValue {
        SimdValue {
            simd_type: self.simd_type,
            lanes: self.lanes.iter().map(|x| x.abs()).collect(),
        }
    }

    pub fn sqrt(&self) -> SimdValue {
        SimdValue {
            simd_type: self.simd_type,
            lanes: self.lanes.iter().map(|x| x.sqrt()).collect(),
        }
    }

    pub fn min(&self, other: &SimdValue) -> Result<SimdValue, String> {
        self.binary_op(other, |a, b| a.min(b))
    }

    pub fn max(&self, other: &SimdValue) -> Result<SimdValue, String> {
        self.binary_op(other, |a, b| a.max(b))
    }

    pub fn fma(&self, b: &SimdValue, c: &SimdValue) -> Result<SimdValue, String> {
        if self.lane_count() != b.lane_count() || self.lane_count() != c.lane_count() {
            return Err("SIMD FMA: lane count mismatch".to_string());
        }
        let lanes: Vec<f64> = self
            .lanes
            .iter()
            .zip(b.lanes.iter())
            .zip(c.lanes.iter())
            .map(|((a, b), c)| a * b + c)
            .collect();
        Ok(SimdValue {
            simd_type: self.simd_type,
            lanes,
        })
    }

    pub fn reduce_add(&self) -> f64 {
        self.lanes.iter().sum()
    }

    pub fn reduce_mul(&self) -> f64 {
        self.lanes.iter().product()
    }

    pub fn reduce_min(&self) -> f64 {
        self.lanes.iter().cloned().fold(f64::INFINITY, f64::min)
    }

    pub fn reduce_max(&self) -> f64 {
        self.lanes.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
    }

    fn binary_op<F>(&self, other: &SimdValue, op: F) -> Result<SimdValue, String>
    where
        F: Fn(f64, f64) -> f64,
    {
        if self.lane_count() != other.lane_count() {
            return Err("SIMD lane count mismatch".to_string());
        }
        let lanes: Vec<f64> = self
            .lanes
            .iter()
            .zip(other.lanes.iter())
            .map(|(a, b)| op(*a, *b))
            .collect();
        Ok(SimdValue {
            simd_type: self.simd_type,
            lanes,
        })
    }
}

/// Parse Simd type from AdeshLang annotation
pub fn parse_simd_type(annotation: &str) -> Option<SimdType> {
    SimdType::from_annotation(annotation)
}

/// Create SimdValue from element type name and lane data
pub fn make_simd(elem_type: &str, lanes: u32, data: Vec<f64>) -> Result<SimdValue, String> {
    let elem = SimdElement::from_type_name(elem_type)
        .ok_or_else(|| format!("unknown SIMD element type: {}", elem_type))?;
    let simd_type = SimdType::new(elem, lanes);
    if data.len() != lanes as usize {
        return Err(format!(
            "expected {} lanes, got {}",
            lanes,
            data.len()
        ));
    }
    Ok(SimdValue::from_lanes(simd_type, data))
}
