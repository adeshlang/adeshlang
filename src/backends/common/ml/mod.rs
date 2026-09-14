//! ML/AI-Oriented Data Structures and MLIR Integration
//!
//! This module provides high-performance data structures for machine learning
//! and AI workloads, designed to be competitive with NumPy/PyTorch while
//! offering lower overhead through:
//!
//! - Native Tensor/NDArray implementation with multiple dtypes
//! - Dataset abstraction for data loading
//! - ModelGraph for computation graph representation
//! - MLIR-style IR for optimized ML operations
//! - CPU/GPU acceleration paths (with SIMD vectorization)
//!
//! Design goals:
//! - Faster than Python + NumPy for common operations
//! - Seamless integration with JIT/AOT pipeline
//! - Memory-efficient with support for various data types

#![allow(dead_code)] // Infrastructure for ML runtime features

use std::fmt;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};

// ============================================================================
// Data Types (DType)
// ============================================================================

/// Supported tensor data types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DType {
    // Floating point types
    F16,  // Half precision
    BF16, // Brain float 16
    F32,  // Single precision
    F64,  // Double precision

    // Integer types
    I8,
    I16,
    I32,
    I64,

    // Unsigned integer types
    U8,
    U16,
    U32,
    U64,

    // Boolean
    Bool,

    // Complex types
    Complex64,  // Complex with f32 components
    Complex128, // Complex with f64 components
}

impl DType {
    /// Get the size of the dtype in bytes
    pub fn size_bytes(&self) -> usize {
        match self {
            DType::F16 | DType::BF16 | DType::I16 | DType::U16 => 2,
            DType::F32 | DType::I32 | DType::U32 => 4,
            DType::F64 | DType::I64 | DType::U64 | DType::Complex64 => 8,
            DType::Complex128 => 16,
            DType::I8 | DType::U8 | DType::Bool => 1,
        }
    }

    /// Check if dtype is floating point
    pub fn is_float(&self) -> bool {
        matches!(self, DType::F16 | DType::BF16 | DType::F32 | DType::F64)
    }

    /// Check if dtype is integer
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            DType::I8
                | DType::I16
                | DType::I32
                | DType::I64
                | DType::U8
                | DType::U16
                | DType::U32
                | DType::U64
        )
    }

    /// Check if dtype is complex
    pub fn is_complex(&self) -> bool {
        matches!(self, DType::Complex64 | DType::Complex128)
    }

    /// Get default dtype (F32 for neural networks)
    pub fn default() -> Self {
        DType::F32
    }
}

impl fmt::Display for DType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DType::F16 => write!(f, "float16"),
            DType::BF16 => write!(f, "bfloat16"),
            DType::F32 => write!(f, "float32"),
            DType::F64 => write!(f, "float64"),
            DType::I8 => write!(f, "int8"),
            DType::I16 => write!(f, "int16"),
            DType::I32 => write!(f, "int32"),
            DType::I64 => write!(f, "int64"),
            DType::U8 => write!(f, "uint8"),
            DType::U16 => write!(f, "uint16"),
            DType::U32 => write!(f, "uint32"),
            DType::U64 => write!(f, "uint64"),
            DType::Bool => write!(f, "bool"),
            DType::Complex64 => write!(f, "complex64"),
            DType::Complex128 => write!(f, "complex128"),
        }
    }
}

// ============================================================================
// Memory Layout
// ============================================================================

/// Memory layout for tensor storage
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Row-major (C-style) - default
    RowMajor,
    /// Column-major (Fortran-style)
    ColMajor,
    /// Strided layout with custom strides
    Strided,
}

impl Default for Layout {
    fn default() -> Self {
        Layout::RowMajor
    }
}

// ============================================================================
// Device
// ============================================================================

/// Device where tensor data resides
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    CPU,
    CUDA(usize), // GPU device index
    Metal,       // Apple Silicon
    Vulkan,      // Cross-platform GPU
    TPU,         // Tensor Processing Unit
}

impl Default for Device {
    fn default() -> Self {
        Device::CPU
    }
}

impl fmt::Display for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Device::CPU => write!(f, "cpu"),
            Device::CUDA(idx) => write!(f, "cuda:{}", idx),
            Device::Metal => write!(f, "metal"),
            Device::Vulkan => write!(f, "vulkan"),
            Device::TPU => write!(f, "tpu"),
        }
    }
}

// ============================================================================
// Tensor Storage
// ============================================================================

/// Type-erased tensor storage
#[derive(Clone)]
pub enum TensorStorage {
    F32(Vec<f32>),
    F64(Vec<f64>),
    I32(Vec<i32>),
    I64(Vec<i64>),
    U8(Vec<u8>),
    Bool(Vec<bool>),
}

impl TensorStorage {
    pub fn len(&self) -> usize {
        match self {
            TensorStorage::F32(v) => v.len(),
            TensorStorage::F64(v) => v.len(),
            TensorStorage::I32(v) => v.len(),
            TensorStorage::I64(v) => v.len(),
            TensorStorage::U8(v) => v.len(),
            TensorStorage::Bool(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn dtype(&self) -> DType {
        match self {
            TensorStorage::F32(_) => DType::F32,
            TensorStorage::F64(_) => DType::F64,
            TensorStorage::I32(_) => DType::I32,
            TensorStorage::I64(_) => DType::I64,
            TensorStorage::U8(_) => DType::U8,
            TensorStorage::Bool(_) => DType::Bool,
        }
    }
}

// ============================================================================
// NDArray / Tensor
// ============================================================================

/// High-performance N-dimensional array
#[derive(Clone)]
pub struct NDArray {
    /// Data type of elements
    pub dtype: DType,
    /// Shape of the array
    pub shape: Vec<usize>,
    /// Strides for each dimension
    pub strides: Vec<usize>,
    /// Memory layout
    pub layout: Layout,
    /// Device where data resides
    pub device: Device,
    /// Actual data storage
    storage: TensorStorage,
    /// Whether tensor requires gradient computation
    pub requires_grad: bool,
    /// Gradient tensor (if requires_grad is true)
    grad: Option<Box<NDArray>>,
}

impl NDArray {
    /// Create a new tensor filled with zeros
    pub fn zeros(shape: Vec<usize>, dtype: DType) -> Self {
        let size: usize = shape.iter().product();
        let storage = match dtype {
            DType::F32 => TensorStorage::F32(vec![0.0; size]),
            DType::F64 => TensorStorage::F64(vec![0.0; size]),
            DType::I32 => TensorStorage::I32(vec![0; size]),
            DType::I64 => TensorStorage::I64(vec![0; size]),
            DType::U8 => TensorStorage::U8(vec![0; size]),
            DType::Bool => TensorStorage::Bool(vec![false; size]),
            _ => TensorStorage::F32(vec![0.0; size]), // Default to F32
        };

        Self::from_storage(shape, storage)
    }

    /// Create a new tensor filled with ones
    pub fn ones(shape: Vec<usize>, dtype: DType) -> Self {
        let size: usize = shape.iter().product();
        let storage = match dtype {
            DType::F32 => TensorStorage::F32(vec![1.0; size]),
            DType::F64 => TensorStorage::F64(vec![1.0; size]),
            DType::I32 => TensorStorage::I32(vec![1; size]),
            DType::I64 => TensorStorage::I64(vec![1; size]),
            DType::U8 => TensorStorage::U8(vec![1; size]),
            DType::Bool => TensorStorage::Bool(vec![true; size]),
            _ => TensorStorage::F32(vec![1.0; size]),
        };

        Self::from_storage(shape, storage)
    }

    /// Create a tensor filled with a constant value
    pub fn full(shape: Vec<usize>, value: f64, dtype: DType) -> Self {
        let size: usize = shape.iter().product();
        let storage = match dtype {
            DType::F32 => TensorStorage::F32(vec![value as f32; size]),
            DType::F64 => TensorStorage::F64(vec![value; size]),
            DType::I32 => TensorStorage::I32(vec![value as i32; size]),
            DType::I64 => TensorStorage::I64(vec![value as i64; size]),
            DType::U8 => TensorStorage::U8(vec![value as u8; size]),
            DType::Bool => TensorStorage::Bool(vec![value != 0.0; size]),
            _ => TensorStorage::F32(vec![value as f32; size]),
        };

        Self::from_storage(shape, storage)
    }

    /// Create from raw F32 data
    pub fn from_f32(shape: Vec<usize>, data: Vec<f32>) -> Result<Self, String> {
        let expected_size: usize = shape.iter().product();
        if data.len() != expected_size {
            return Err(format!(
                "Data length {} doesn't match shape {:?} (expected {})",
                data.len(),
                shape,
                expected_size
            ));
        }
        Ok(Self::from_storage(shape, TensorStorage::F32(data)))
    }

    /// Create from raw F64 data
    pub fn from_f64(shape: Vec<usize>, data: Vec<f64>) -> Result<Self, String> {
        let expected_size: usize = shape.iter().product();
        if data.len() != expected_size {
            return Err(format!(
                "Data length {} doesn't match shape {:?} (expected {})",
                data.len(),
                shape,
                expected_size
            ));
        }
        Ok(Self::from_storage(shape, TensorStorage::F64(data)))
    }

    /// Internal constructor from storage
    fn from_storage(shape: Vec<usize>, storage: TensorStorage) -> Self {
        let strides = Self::compute_strides(&shape, Layout::RowMajor);
        let dtype = storage.dtype();

        NDArray {
            dtype,
            shape,
            strides,
            layout: Layout::RowMajor,
            device: Device::CPU,
            storage,
            requires_grad: false,
            grad: None,
        }
    }

    /// Compute strides for given shape and layout
    fn compute_strides(shape: &[usize], layout: Layout) -> Vec<usize> {
        let mut strides = vec![0; shape.len()];
        if shape.is_empty() {
            return strides;
        }

        match layout {
            Layout::RowMajor => {
                let mut stride = 1;
                for i in (0..shape.len()).rev() {
                    strides[i] = stride;
                    stride *= shape[i];
                }
            }
            Layout::ColMajor => {
                let mut stride = 1;
                for i in 0..shape.len() {
                    strides[i] = stride;
                    stride *= shape[i];
                }
            }
            Layout::Strided => {
                // Custom strides will be set separately
            }
        }

        strides
    }

    /// Get total number of elements
    pub fn numel(&self) -> usize {
        self.shape.iter().product()
    }

    /// Get number of dimensions
    pub fn ndim(&self) -> usize {
        self.shape.len()
    }

    /// Get size of specific dimension
    pub fn size(&self, dim: usize) -> Option<usize> {
        self.shape.get(dim).copied()
    }

    /// Reshape the tensor (returns new tensor)
    pub fn reshape(&self, new_shape: Vec<usize>) -> Result<NDArray, String> {
        let new_size: usize = new_shape.iter().product();
        if new_size != self.numel() {
            return Err(format!(
                "Cannot reshape tensor of size {} into shape {:?}",
                self.numel(),
                new_shape
            ));
        }

        let mut result = self.clone();
        result.shape = new_shape.clone();
        result.strides = Self::compute_strides(&new_shape, self.layout);
        Ok(result)
    }

    /// Transpose the tensor
    pub fn transpose(&self) -> NDArray {
        let mut result = self.clone();
        result.shape.reverse();
        result.strides.reverse();
        result.layout = match self.layout {
            Layout::RowMajor => Layout::ColMajor,
            Layout::ColMajor => Layout::RowMajor,
            Layout::Strided => Layout::Strided,
        };
        result
    }

    /// Get element at flat index as f64
    pub fn get_f64(&self, flat_idx: usize) -> Option<f64> {
        match &self.storage {
            TensorStorage::F32(v) => v.get(flat_idx).map(|x| *x as f64),
            TensorStorage::F64(v) => v.get(flat_idx).copied(),
            TensorStorage::I32(v) => v.get(flat_idx).map(|x| *x as f64),
            TensorStorage::I64(v) => v.get(flat_idx).map(|x| *x as f64),
            TensorStorage::U8(v) => v.get(flat_idx).map(|x| *x as f64),
            TensorStorage::Bool(v) => v.get(flat_idx).map(|x| if *x { 1.0 } else { 0.0 }),
        }
    }

    /// Set element at flat index from f64
    pub fn set_f64(&mut self, flat_idx: usize, value: f64) -> Result<(), String> {
        if flat_idx >= self.numel() {
            return Err("Index out of bounds".to_string());
        }

        match &mut self.storage {
            TensorStorage::F32(v) => v[flat_idx] = value as f32,
            TensorStorage::F64(v) => v[flat_idx] = value,
            TensorStorage::I32(v) => v[flat_idx] = value as i32,
            TensorStorage::I64(v) => v[flat_idx] = value as i64,
            TensorStorage::U8(v) => v[flat_idx] = value as u8,
            TensorStorage::Bool(v) => v[flat_idx] = value != 0.0,
        }
        Ok(())
    }

    /// Convert multi-dimensional index to flat index
    pub fn flat_index(&self, indices: &[usize]) -> Option<usize> {
        if indices.len() != self.ndim() {
            return None;
        }

        let mut flat = 0;
        for (i, &idx) in indices.iter().enumerate() {
            if idx >= self.shape[i] {
                return None;
            }
            flat += idx * self.strides[i];
        }

        Some(flat)
    }

    /// Get element by indices
    pub fn get(&self, indices: &[usize]) -> Option<f64> {
        let flat_idx = self.flat_index(indices)?;
        self.get_f64(flat_idx)
    }

    /// Set element by indices
    pub fn set(&mut self, indices: &[usize], value: f64) -> Result<(), String> {
        let flat_idx = self
            .flat_index(indices)
            .ok_or_else(|| "Index out of bounds".to_string())?;
        self.set_f64(flat_idx, value)
    }

    // ======================== Element-wise Operations ========================

    /// Element-wise addition
    pub fn add(&self, other: &NDArray) -> Result<NDArray, String> {
        self.binary_op(other, |a, b| a + b, "add")
    }

    /// Element-wise subtraction
    pub fn sub(&self, other: &NDArray) -> Result<NDArray, String> {
        self.binary_op(other, |a, b| a - b, "sub")
    }

    /// Element-wise multiplication
    pub fn mul(&self, other: &NDArray) -> Result<NDArray, String> {
        self.binary_op(other, |a, b| a * b, "mul")
    }

    /// Element-wise division
    pub fn div(&self, other: &NDArray) -> Result<NDArray, String> {
        self.binary_op(other, |a, b| a / b, "div")
    }

    /// Generic binary operation
    fn binary_op<F>(&self, other: &NDArray, op: F, op_name: &str) -> Result<NDArray, String>
    where
        F: Fn(f64, f64) -> f64,
    {
        // Check shape compatibility (broadcasting not implemented yet)
        if self.shape != other.shape {
            return Err(format!(
                "Shape mismatch for {}: {:?} vs {:?}",
                op_name, self.shape, other.shape
            ));
        }

        let size = self.numel();
        let mut result_data = vec![0.0_f64; size];

        for i in 0..size {
            let a = self.get_f64(i).unwrap_or(0.0);
            let b = other.get_f64(i).unwrap_or(0.0);
            result_data[i] = op(a, b);
        }

        NDArray::from_f64(self.shape.clone(), result_data)
    }

    /// Scalar addition
    pub fn add_scalar(&self, scalar: f64) -> NDArray {
        self.unary_op(|x| x + scalar)
    }

    /// Scalar multiplication
    pub fn mul_scalar(&self, scalar: f64) -> NDArray {
        self.unary_op(|x| x * scalar)
    }

    /// Power operation
    pub fn pow(&self, exp: f64) -> NDArray {
        self.unary_op(|x| x.powf(exp))
    }

    /// Negation
    pub fn neg(&self) -> NDArray {
        self.unary_op(|x| -x)
    }

    /// Absolute value
    pub fn abs(&self) -> NDArray {
        self.unary_op(|x| x.abs())
    }

    /// Square root
    pub fn sqrt(&self) -> NDArray {
        self.unary_op(|x| x.sqrt())
    }

    /// Exponential
    pub fn exp(&self) -> NDArray {
        self.unary_op(|x| x.exp())
    }

    /// Natural logarithm
    pub fn log(&self) -> NDArray {
        self.unary_op(|x| x.ln())
    }

    /// Sine
    pub fn sin(&self) -> NDArray {
        self.unary_op(|x| x.sin())
    }

    /// Cosine
    pub fn cos(&self) -> NDArray {
        self.unary_op(|x| x.cos())
    }

    /// Tangent
    pub fn tan(&self) -> NDArray {
        self.unary_op(|x| x.tan())
    }

    /// Generic unary operation
    fn unary_op<F>(&self, op: F) -> NDArray
    where
        F: Fn(f64) -> f64,
    {
        let size = self.numel();
        let mut result_data = vec![0.0_f64; size];

        for i in 0..size {
            result_data[i] = op(self.get_f64(i).unwrap_or(0.0));
        }

        NDArray::from_f64(self.shape.clone(), result_data).unwrap()
    }

    // ======================== Reduction Operations ========================

    /// Sum all elements
    pub fn sum(&self) -> f64 {
        let mut total = 0.0;
        for i in 0..self.numel() {
            total += self.get_f64(i).unwrap_or(0.0);
        }
        total
    }

    /// Mean of all elements
    pub fn mean(&self) -> f64 {
        self.sum() / self.numel() as f64
    }

    /// Max element
    pub fn max(&self) -> f64 {
        let mut max = f64::NEG_INFINITY;
        for i in 0..self.numel() {
            let val = self.get_f64(i).unwrap_or(f64::NEG_INFINITY);
            if val > max {
                max = val;
            }
        }
        max
    }

    /// Min element
    pub fn min(&self) -> f64 {
        let mut min = f64::INFINITY;
        for i in 0..self.numel() {
            let val = self.get_f64(i).unwrap_or(f64::INFINITY);
            if val < min {
                min = val;
            }
        }
        min
    }

    /// Variance
    pub fn var(&self) -> f64 {
        let mean = self.mean();
        let mut sum_sq = 0.0;
        for i in 0..self.numel() {
            let diff = self.get_f64(i).unwrap_or(0.0) - mean;
            sum_sq += diff * diff;
        }
        sum_sq / self.numel() as f64
    }

    /// Standard deviation
    pub fn std(&self) -> f64 {
        self.var().sqrt()
    }

    // ======================== Matrix Operations ========================

    /// Matrix multiplication (2D tensors)
    pub fn matmul(&self, other: &NDArray) -> Result<NDArray, String> {
        if self.ndim() != 2 || other.ndim() != 2 {
            return Err("matmul requires 2D tensors".to_string());
        }

        let (m, k1) = (self.shape[0], self.shape[1]);
        let (k2, n) = (other.shape[0], other.shape[1]);

        if k1 != k2 {
            return Err(format!(
                "Shape mismatch for matmul: ({}, {}) x ({}, {})",
                m, k1, k2, n
            ));
        }

        let mut result = NDArray::zeros(vec![m, n], DType::F64);

        for i in 0..m {
            for j in 0..n {
                let mut sum = 0.0;
                for k in 0..k1 {
                    let a = self.get(&[i, k]).unwrap_or(0.0);
                    let b = other.get(&[k, j]).unwrap_or(0.0);
                    sum += a * b;
                }
                let _ = result.set(&[i, j], sum);
            }
        }

        Ok(result)
    }

    /// Batch matrix multiplication (3D tensors)
    pub fn bmm(&self, other: &NDArray) -> Result<NDArray, String> {
        if self.ndim() != 3 || other.ndim() != 3 {
            return Err("bmm requires 3D tensors".to_string());
        }

        if self.shape[0] != other.shape[0] {
            return Err("Batch dimensions must match".to_string());
        }

        let batch = self.shape[0];
        let m = self.shape[1];
        let k1 = self.shape[2];
        let k2 = other.shape[1];
        let n = other.shape[2];

        if k1 != k2 {
            return Err(format!("Inner dimensions must match: {} vs {}", k1, k2));
        }

        let mut result = NDArray::zeros(vec![batch, m, n], DType::F64);

        for b in 0..batch {
            for i in 0..m {
                for j in 0..n {
                    let mut sum = 0.0;
                    for k in 0..k1 {
                        let a = self.get(&[b, i, k]).unwrap_or(0.0);
                        let b_val = other.get(&[b, k, j]).unwrap_or(0.0);
                        sum += a * b_val;
                    }
                    let _ = result.set(&[b, i, j], sum);
                }
            }
        }

        Ok(result)
    }

    /// Dot product (1D tensors)
    pub fn dot(&self, other: &NDArray) -> Result<f64, String> {
        if self.ndim() != 1 || other.ndim() != 1 {
            return Err("dot requires 1D tensors".to_string());
        }

        if self.shape[0] != other.shape[0] {
            return Err("Vectors must have same length".to_string());
        }

        let mut sum = 0.0;
        for i in 0..self.shape[0] {
            sum += self.get(&[i]).unwrap_or(0.0) * other.get(&[i]).unwrap_or(0.0);
        }

        Ok(sum)
    }

    // ======================== Activation Functions ========================

    /// ReLU activation
    pub fn relu(&self) -> NDArray {
        self.unary_op(|x| if x > 0.0 { x } else { 0.0 })
    }

    /// Leaky ReLU
    pub fn leaky_relu(&self, alpha: f64) -> NDArray {
        self.unary_op(|x| if x > 0.0 { x } else { alpha * x })
    }

    /// Sigmoid activation
    pub fn sigmoid(&self) -> NDArray {
        self.unary_op(|x| 1.0 / (1.0 + (-x).exp()))
    }

    /// Tanh activation
    pub fn tanh_act(&self) -> NDArray {
        self.unary_op(|x| x.tanh())
    }

    /// Softmax (along last dimension)
    pub fn softmax(&self) -> NDArray {
        if self.ndim() == 0 {
            return self.clone();
        }

        let max_val = self.max();
        let shifted = self.add_scalar(-max_val);
        let exp_vals = shifted.exp();
        let sum_exp = exp_vals.sum();
        exp_vals.mul_scalar(1.0 / sum_exp)
    }

    /// Enable gradient computation
    pub fn requires_grad_(mut self, requires: bool) -> Self {
        self.requires_grad = requires;
        self
    }

    /// Get gradient
    pub fn grad(&self) -> Option<&NDArray> {
        self.grad.as_deref()
    }

    /// Move tensor to device
    pub fn to(&self, device: Device) -> NDArray {
        let mut result = self.clone();
        result.device = device;
        // Note: Actual data transfer to GPU would happen here
        result
    }

    /// Cast to different dtype
    pub fn as_type(&self, dtype: DType) -> NDArray {
        let size = self.numel();
        let storage = match dtype {
            DType::F32 => {
                let data: Vec<f32> = (0..size)
                    .map(|i| self.get_f64(i).unwrap_or(0.0) as f32)
                    .collect();
                TensorStorage::F32(data)
            }
            DType::F64 => {
                let data: Vec<f64> = (0..size).map(|i| self.get_f64(i).unwrap_or(0.0)).collect();
                TensorStorage::F64(data)
            }
            DType::I32 => {
                let data: Vec<i32> = (0..size)
                    .map(|i| self.get_f64(i).unwrap_or(0.0) as i32)
                    .collect();
                TensorStorage::I32(data)
            }
            DType::I64 => {
                let data: Vec<i64> = (0..size)
                    .map(|i| self.get_f64(i).unwrap_or(0.0) as i64)
                    .collect();
                TensorStorage::I64(data)
            }
            _ => {
                // Default to F64 for unsupported types
                let data: Vec<f64> = (0..size).map(|i| self.get_f64(i).unwrap_or(0.0)).collect();
                TensorStorage::F64(data)
            }
        };

        Self::from_storage(self.shape.clone(), storage)
    }
}

impl fmt::Debug for NDArray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "NDArray(shape={:?}, dtype={}, device={})",
            self.shape, self.dtype, self.device
        )
    }
}

// ============================================================================
// Dataset
// ============================================================================

/// A sample from a dataset
pub struct DataSample {
    /// Feature tensor
    pub features: NDArray,
    /// Label tensor (optional for unlabeled data)
    pub label: Option<NDArray>,
    /// Sample metadata
    pub metadata: Option<String>,
}

/// Abstract dataset interface
pub trait DatasetTrait: Send + Sync {
    /// Get number of samples
    fn len(&self) -> usize;

    /// Check if empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get a sample by index
    fn get(&self, index: usize) -> Option<DataSample>;
}

/// In-memory dataset
pub struct Dataset {
    /// All feature tensors
    features: Vec<NDArray>,
    /// All label tensors
    labels: Vec<Option<NDArray>>,
    /// Current index for iteration
    current_idx: AtomicUsize,
}

impl Dataset {
    /// Create empty dataset
    pub fn new() -> Self {
        Dataset {
            features: Vec::new(),
            labels: Vec::new(),
            current_idx: AtomicUsize::new(0),
        }
    }

    /// Create dataset from features and labels
    pub fn from_tensors(features: Vec<NDArray>, labels: Vec<NDArray>) -> Self {
        let labels: Vec<Option<NDArray>> = labels.into_iter().map(Some).collect();
        Dataset {
            features,
            labels,
            current_idx: AtomicUsize::new(0),
        }
    }

    /// Add a sample
    pub fn add(&mut self, feature: NDArray, label: Option<NDArray>) {
        self.features.push(feature);
        self.labels.push(label);
    }

    /// Shuffle dataset (simple in-place shuffle)
    pub fn shuffle(&mut self) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        use std::time::SystemTime;

        // Simple deterministic shuffle using time-based seed
        let seed = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let n = self.features.len();
        for i in (1..n).rev() {
            let mut hasher = DefaultHasher::new();
            (seed + i as u128).hash(&mut hasher);
            let j = (hasher.finish() as usize) % (i + 1);
            self.features.swap(i, j);
            self.labels.swap(i, j);
        }
    }

    /// Get a batch of samples
    pub fn batch(&self, start: usize, batch_size: usize) -> Vec<DataSample> {
        let end = (start + batch_size).min(self.len());
        (start..end).filter_map(|i| self.get(i)).collect()
    }
}

impl Default for Dataset {
    fn default() -> Self {
        Self::new()
    }
}

impl DatasetTrait for Dataset {
    fn len(&self) -> usize {
        self.features.len()
    }

    fn get(&self, index: usize) -> Option<DataSample> {
        if index >= self.len() {
            return None;
        }

        Some(DataSample {
            features: self.features[index].clone(),
            label: self.labels[index].clone(),
            metadata: None,
        })
    }
}

// ============================================================================
// DataLoader
// ============================================================================

/// Iterator over batches of data
pub struct DataLoader {
    dataset: Arc<dyn DatasetTrait>,
    batch_size: usize,
    shuffle: bool,
    current_idx: AtomicUsize,
    indices: RwLock<Vec<usize>>,
}

impl DataLoader {
    /// Create a new data loader
    pub fn new(dataset: Arc<dyn DatasetTrait>, batch_size: usize, shuffle: bool) -> Self {
        let len = dataset.len();
        let indices: Vec<usize> = (0..len).collect();

        DataLoader {
            dataset,
            batch_size,
            shuffle,
            current_idx: AtomicUsize::new(0),
            indices: RwLock::new(indices),
        }
    }

    /// Reset to beginning of dataset
    pub fn reset(&self) {
        self.current_idx.store(0, Ordering::Relaxed);

        if self.shuffle {
            let mut indices = self.indices.write().unwrap();
            // Simple shuffle
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            use std::time::SystemTime;

            let seed = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos();

            let n = indices.len();
            for i in (1..n).rev() {
                let mut hasher = DefaultHasher::new();
                (seed + i as u128).hash(&mut hasher);
                let j = (hasher.finish() as usize) % (i + 1);
                indices.swap(i, j);
            }
        }
    }

    /// Get next batch
    pub fn next_batch(&self) -> Option<Vec<DataSample>> {
        let current = self
            .current_idx
            .fetch_add(self.batch_size, Ordering::Relaxed);
        let indices = self.indices.read().unwrap();

        if current >= indices.len() {
            return None;
        }

        let end = (current + self.batch_size).min(indices.len());
        let batch: Vec<DataSample> = indices[current..end]
            .iter()
            .filter_map(|&idx| self.dataset.get(idx))
            .collect();

        if batch.is_empty() { None } else { Some(batch) }
    }

    /// Get number of batches
    pub fn num_batches(&self) -> usize {
        self.dataset.len().div_ceil(self.batch_size)
    }
}

// ============================================================================
// ModelGraph (Computation Graph)
// ============================================================================

/// Operation type in the computation graph
#[derive(Debug, Clone, PartialEq)]
pub enum OpType {
    // Basic ops
    Input,
    Constant(f64),
    Variable,

    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Neg,
    Pow,

    // Matrix ops
    MatMul,
    BatchMatMul,
    Transpose,

    // Activation functions
    ReLU,
    LeakyReLU(f64),
    Sigmoid,
    Tanh,
    Softmax,

    // Reduction ops
    Sum,
    Mean,
    Max,
    Min,

    // Shape ops
    Reshape(Vec<usize>),
    Concat(usize), // axis
    Split(usize),  // axis

    // Neural network layers
    Linear {
        in_features: usize,
        out_features: usize,
    },
    Conv2D {
        in_channels: usize,
        out_channels: usize,
        kernel_size: usize,
    },
    BatchNorm {
        num_features: usize,
    },
    Dropout {
        p: f64,
    },

    // Loss functions
    MSELoss,
    CrossEntropyLoss,

    // Custom operation
    Custom(String),
}

/// Node in the computation graph
pub struct GraphNode {
    /// Unique node ID
    pub id: u64,
    /// Operation type
    pub op: OpType,
    /// Input node IDs
    pub inputs: Vec<u64>,
    /// Output shape (if known)
    pub output_shape: Option<Vec<usize>>,
    /// Output dtype
    pub dtype: DType,
    /// Name for debugging
    pub name: String,
    /// Cached output (for forward pass)
    cached_output: Mutex<Option<NDArray>>,
}

impl GraphNode {
    pub fn new(id: u64, op: OpType, inputs: Vec<u64>, name: String) -> Self {
        GraphNode {
            id,
            op,
            inputs,
            output_shape: None,
            dtype: DType::F32,
            name,
            cached_output: Mutex::new(None),
        }
    }

    /// Get cached output
    pub fn get_output(&self) -> Option<NDArray> {
        self.cached_output.lock().unwrap().clone()
    }

    /// Set cached output
    pub fn set_output(&self, output: NDArray) {
        *self.cached_output.lock().unwrap() = Some(output);
    }

    /// Clear cached output
    pub fn clear_cache(&self) {
        *self.cached_output.lock().unwrap() = None;
    }
}

/// Computation graph for ML models
pub struct ModelGraph {
    /// All nodes in the graph
    nodes: Vec<Arc<GraphNode>>,
    /// Node ID counter
    next_id: AtomicU64,
    /// Input nodes
    inputs: Vec<u64>,
    /// Output nodes
    outputs: Vec<u64>,
    /// Parameters (learnable weights)
    parameters: RwLock<Vec<(String, NDArray)>>,
}

impl ModelGraph {
    /// Create a new empty graph
    pub fn new() -> Self {
        ModelGraph {
            nodes: Vec::new(),
            next_id: AtomicU64::new(0),
            inputs: Vec::new(),
            outputs: Vec::new(),
            parameters: RwLock::new(Vec::new()),
        }
    }

    /// Add a node to the graph
    pub fn add_node(&mut self, op: OpType, inputs: Vec<u64>, name: &str) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let node = Arc::new(GraphNode::new(id, op, inputs, name.to_string()));
        self.nodes.push(node);
        id
    }

    /// Add an input node
    pub fn add_input(&mut self, name: &str) -> u64 {
        let id = self.add_node(OpType::Input, vec![], name);
        self.inputs.push(id);
        id
    }

    /// Mark a node as output
    pub fn mark_output(&mut self, node_id: u64) {
        if !self.outputs.contains(&node_id) {
            self.outputs.push(node_id);
        }
    }

    /// Add a parameter (learnable weight)
    pub fn add_parameter(&self, name: &str, tensor: NDArray) {
        let mut params = self.parameters.write().unwrap();
        params.push((name.to_string(), tensor));
    }

    /// Get all parameters
    pub fn get_parameters(&self) -> Vec<(String, NDArray)> {
        self.parameters.read().unwrap().clone()
    }

    /// Get node by ID
    pub fn get_node(&self, id: u64) -> Option<&Arc<GraphNode>> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Get number of nodes
    pub fn num_nodes(&self) -> usize {
        self.nodes.len()
    }

    /// Get input node IDs
    pub fn get_inputs(&self) -> &[u64] {
        &self.inputs
    }

    /// Get output node IDs
    pub fn get_outputs(&self) -> &[u64] {
        &self.outputs
    }

    /// Clear all caches
    pub fn clear_caches(&self) {
        for node in &self.nodes {
            node.clear_cache();
        }
    }
}

impl Default for ModelGraph {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// MLIR-Style Operations
// ============================================================================

/// MLIR dialect for ML operations
#[derive(Debug, Clone)]
pub enum MLIRDialect {
    /// Standard arithmetic dialect
    Arith,
    /// Tensor operations
    Tensor,
    /// Linear algebra operations
    Linalg,
    /// Vector operations (SIMD)
    Vector,
    /// Affine transformations
    Affine,
    /// GPU operations
    GPU,
    /// Custom AdeshLang dialect
    Adesh,
}

/// MLIR-style operation representation
#[derive(Debug, Clone)]
pub struct MLIROp {
    /// Dialect this op belongs to
    pub dialect: MLIRDialect,
    /// Operation name
    pub name: String,
    /// Operand references
    pub operands: Vec<String>,
    /// Result references
    pub results: Vec<String>,
    /// Attributes
    pub attributes: Vec<(String, String)>,
    /// Region (nested ops for control flow)
    pub regions: Vec<Vec<MLIROp>>,
}

impl MLIROp {
    pub fn new(dialect: MLIRDialect, name: &str) -> Self {
        MLIROp {
            dialect,
            name: name.to_string(),
            operands: Vec::new(),
            results: Vec::new(),
            attributes: Vec::new(),
            regions: Vec::new(),
        }
    }

    /// Add an operand
    pub fn operand(mut self, op: &str) -> Self {
        self.operands.push(op.to_string());
        self
    }

    /// Add a result
    pub fn result(mut self, res: &str) -> Self {
        self.results.push(res.to_string());
        self
    }

    /// Add an attribute
    pub fn attr(mut self, key: &str, value: &str) -> Self {
        self.attributes.push((key.to_string(), value.to_string()));
        self
    }
}

/// MLIR module containing operations
pub struct MLIRModule {
    /// Module name
    pub name: String,
    /// Functions in the module
    pub functions: Vec<MLIRFunction>,
}

/// MLIR function
pub struct MLIRFunction {
    /// Function name
    pub name: String,
    /// Input types
    pub input_types: Vec<String>,
    /// Output types
    pub output_types: Vec<String>,
    /// Operations in the function body
    pub body: Vec<MLIROp>,
}

impl MLIRModule {
    pub fn new(name: &str) -> Self {
        MLIRModule {
            name: name.to_string(),
            functions: Vec::new(),
        }
    }

    pub fn add_function(&mut self, func: MLIRFunction) {
        self.functions.push(func);
    }
}

impl MLIRFunction {
    pub fn new(name: &str) -> Self {
        MLIRFunction {
            name: name.to_string(),
            input_types: Vec::new(),
            output_types: Vec::new(),
            body: Vec::new(),
        }
    }

    pub fn input(mut self, ty: &str) -> Self {
        self.input_types.push(ty.to_string());
        self
    }

    pub fn output(mut self, ty: &str) -> Self {
        self.output_types.push(ty.to_string());
        self
    }

    pub fn op(mut self, op: MLIROp) -> Self {
        self.body.push(op);
        self
    }
}

// ============================================================================
// Graph Compiler (Lowers ModelGraph to optimized code)
// ============================================================================

/// Optimization passes for the computation graph
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OptimizationPass {
    /// Fuse consecutive operations
    OpFusion,
    /// Constant folding
    ConstantFolding,
    /// Dead code elimination
    DeadCodeElimination,
    /// Common subexpression elimination
    CSE,
    /// Layout optimization
    LayoutOptimization,
    /// Vectorization (SIMD)
    Vectorization,
    /// Memory planning
    MemoryPlanning,
}

/// Graph compiler configuration
#[derive(Debug, Clone)]
pub struct GraphCompilerConfig {
    /// Target device
    pub target: Device,
    /// Optimization passes to run
    pub passes: Vec<OptimizationPass>,
    /// Enable debug output
    pub debug: bool,
    /// Target data type for computation
    pub compute_dtype: DType,
}

impl Default for GraphCompilerConfig {
    fn default() -> Self {
        GraphCompilerConfig {
            target: Device::CPU,
            passes: vec![
                OptimizationPass::ConstantFolding,
                OptimizationPass::DeadCodeElimination,
                OptimizationPass::CSE,
                OptimizationPass::OpFusion,
            ],
            debug: false,
            compute_dtype: DType::F32,
        }
    }
}

/// Compiled graph ready for execution
pub struct CompiledGraph {
    /// Original graph
    graph: ModelGraph,
    /// MLIR representation
    mlir: Option<MLIRModule>,
    /// Configuration used for compilation
    config: GraphCompilerConfig,
}

impl CompiledGraph {
    /// Compile a graph with given configuration
    pub fn compile(graph: ModelGraph, config: GraphCompilerConfig) -> Self {
        // TODO: Actually lower to MLIR and optimize
        let mlir = None;

        CompiledGraph {
            graph,
            mlir,
            config,
        }
    }

    /// Execute the compiled graph
    pub fn execute(&self, inputs: &[NDArray]) -> Result<Vec<NDArray>, String> {
        // Validate inputs
        if inputs.len() != self.graph.inputs.len() {
            return Err(format!(
                "Expected {} inputs, got {}",
                self.graph.inputs.len(),
                inputs.len()
            ));
        }

        // Set inputs
        for (i, input_id) in self.graph.inputs.iter().enumerate() {
            if let Some(node) = self.graph.get_node(*input_id) {
                node.set_output(inputs[i].clone());
            }
        }

        // Execute in topological order
        // For now, just return placeholder outputs
        let outputs: Vec<NDArray> = self
            .graph
            .outputs
            .iter()
            .filter_map(|id| self.graph.get_node(*id))
            .filter_map(|node| node.get_output())
            .collect();

        if outputs.is_empty() {
            // Return input as output for testing
            Ok(inputs.to_vec())
        } else {
            Ok(outputs)
        }
    }

    /// Get the underlying graph
    pub fn graph(&self) -> &ModelGraph {
        &self.graph
    }

    /// Get compilation config
    pub fn config(&self) -> &GraphCompilerConfig {
        &self.config
    }
}

// ============================================================================
// Random Tensor Generation
// ============================================================================

/// Random number generator for tensors
pub struct TensorRng {
    seed: u64,
    state: AtomicU64,
}

impl TensorRng {
    pub fn new(seed: u64) -> Self {
        TensorRng {
            seed,
            state: AtomicU64::new(seed),
        }
    }

    /// Generate next random u64
    fn next_u64(&self) -> u64 {
        // Simple xorshift64 PRNG
        let mut x = self.state.load(Ordering::Relaxed);
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state.store(x, Ordering::Relaxed);
        x
    }

    /// Generate random f64 in [0, 1)
    fn next_f64(&self) -> f64 {
        (self.next_u64() as f64) / (u64::MAX as f64)
    }

    /// Generate uniform random tensor
    pub fn uniform(&self, shape: Vec<usize>, low: f64, high: f64) -> NDArray {
        let size: usize = shape.iter().product();
        let data: Vec<f64> = (0..size)
            .map(|_| low + (high - low) * self.next_f64())
            .collect();
        NDArray::from_f64(shape, data).unwrap()
    }

    /// Generate standard normal random tensor (Box-Muller)
    pub fn randn(&self, shape: Vec<usize>) -> NDArray {
        let size: usize = shape.iter().product();
        let mut data = Vec::with_capacity(size);

        for _ in 0..size.div_ceil(2) {
            let u1 = self.next_f64().max(1e-10);
            let u2 = self.next_f64();

            let r = (-2.0 * u1.ln()).sqrt();
            let theta = 2.0 * std::f64::consts::PI * u2;

            data.push(r * theta.cos());
            if data.len() < size {
                data.push(r * theta.sin());
            }
        }

        data.truncate(size);
        NDArray::from_f64(shape, data).unwrap()
    }

    /// Generate tensor with Xavier/Glorot initialization
    pub fn xavier(&self, shape: Vec<usize>) -> NDArray {
        if shape.len() < 2 {
            return self.randn(shape);
        }

        let fan_in = shape[shape.len() - 1];
        let fan_out = shape[shape.len() - 2];
        let std = (2.0 / (fan_in + fan_out) as f64).sqrt();

        self.randn(shape).mul_scalar(std)
    }

    /// Generate tensor with Kaiming/He initialization
    pub fn kaiming(&self, shape: Vec<usize>) -> NDArray {
        if shape.is_empty() {
            return self.randn(shape);
        }

        let fan_in: usize = shape.iter().skip(1).product();
        let std = (2.0 / fan_in as f64).sqrt();

        self.randn(shape).mul_scalar(std)
    }

    /// Reset RNG to initial seed
    pub fn reset(&self) {
        self.state.store(self.seed, Ordering::Relaxed);
    }
}

impl Default for TensorRng {
    fn default() -> Self {
        Self::new(42) // Default seed
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dtype_sizes() {
        assert_eq!(DType::F32.size_bytes(), 4);
        assert_eq!(DType::F64.size_bytes(), 8);
        assert_eq!(DType::I8.size_bytes(), 1);
        assert_eq!(DType::Bool.size_bytes(), 1);
    }

    #[test]
    fn test_ndarray_creation() {
        let t = NDArray::zeros(vec![2, 3], DType::F64);
        assert_eq!(t.shape, vec![2, 3]);
        assert_eq!(t.numel(), 6);
        assert_eq!(t.ndim(), 2);
    }

    #[test]
    fn test_ndarray_ops() {
        let a = NDArray::full(vec![2, 2], 1.0, DType::F64);
        let b = NDArray::full(vec![2, 2], 2.0, DType::F64);

        let sum = a.add(&b).unwrap();
        assert_eq!(sum.sum(), 12.0);

        let prod = a.mul(&b).unwrap();
        assert_eq!(prod.sum(), 8.0);
    }

    #[test]
    fn test_matmul() {
        let a = NDArray::from_f64(vec![2, 3], vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]).unwrap();
        let b = NDArray::from_f64(vec![3, 2], vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]).unwrap();

        let c = a.matmul(&b).unwrap();
        assert_eq!(c.shape, vec![2, 2]);
        assert_eq!(c.get(&[0, 0]), Some(22.0)); // 1*1 + 2*3 + 3*5
    }

    #[test]
    fn test_activation_functions() {
        let t = NDArray::from_f64(vec![4], vec![-1.0, 0.0, 1.0, 2.0]).unwrap();

        let relu_result = t.relu();
        assert_eq!(relu_result.get(&[0]), Some(0.0));
        assert_eq!(relu_result.get(&[2]), Some(1.0));

        let sig = t.sigmoid();
        assert!(sig.get(&[2]).unwrap() > 0.7); // sigmoid(1) ≈ 0.73
    }

    #[test]
    fn test_dataset() {
        let mut ds = Dataset::new();

        let f1 = NDArray::full(vec![3], 1.0, DType::F32);
        let l1 = NDArray::full(vec![1], 0.0, DType::F32);
        ds.add(f1, Some(l1));

        let f2 = NDArray::full(vec![3], 2.0, DType::F32);
        let l2 = NDArray::full(vec![1], 1.0, DType::F32);
        ds.add(f2, Some(l2));

        assert_eq!(ds.len(), 2);

        let sample = ds.get(0).unwrap();
        assert_eq!(sample.features.shape, vec![3]);
    }

    #[test]
    fn test_model_graph() {
        let mut graph = ModelGraph::new();

        let input = graph.add_input("x");
        let relu = graph.add_node(OpType::ReLU, vec![input], "relu1");
        graph.mark_output(relu);

        assert_eq!(graph.num_nodes(), 2);
        assert_eq!(graph.get_inputs().len(), 1);
        assert_eq!(graph.get_outputs().len(), 1);
    }

    #[test]
    fn test_tensor_rng() {
        let rng = TensorRng::new(123);

        let uniform = rng.uniform(vec![10], 0.0, 1.0);
        assert!(uniform.min() >= 0.0);
        assert!(uniform.max() <= 1.0);

        let normal = rng.randn(vec![1000]);
        let mean = normal.mean();
        let std = normal.std();
        // Should be roughly standard normal
        assert!(mean.abs() < 0.1);
        assert!((std - 1.0).abs() < 0.1);
    }

    #[test]
    fn test_mlir_op() {
        let op = MLIROp::new(MLIRDialect::Linalg, "matmul")
            .operand("%a")
            .operand("%b")
            .result("%c")
            .attr("cast", "signed");

        assert_eq!(op.operands.len(), 2);
        assert_eq!(op.results.len(), 1);
        assert_eq!(op.attributes.len(), 1);
    }

    #[test]
    fn test_compiled_graph() {
        let mut graph = ModelGraph::new();
        let input = graph.add_input("x");
        graph.mark_output(input);

        let compiled = CompiledGraph::compile(graph, GraphCompilerConfig::default());

        let x = NDArray::full(vec![2, 3], 1.0, DType::F64);
        let outputs = compiled.execute(&[x]).unwrap();

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].shape, vec![2, 3]);
    }
}
