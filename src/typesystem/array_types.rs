//! Comprehensive Multi-Tier Array System
//!
//! This module implements a sophisticated array representation strategy optimized
//! for performance across interpreter, baseline JIT, optimizing JIT, and AOT tiers.
//!
//! ## Array Kinds:
//! - **RawArray**: Zero-overhead fixed-size arrays (C-like)
//! - **SSOArray**: Small-size optimized with inline storage (≤24 bytes)
//! - **CompactArray**: u16 len/cap for arrays ≤65535 elements
//! - **DynArray**: Full-featured dynamic arrays with metadata
//! - **ArenaArray**: Index-based references for arena allocation
//!
//! ## Memory Layout Philosophy:
//! - Metadata-before-data for cache efficiency
//! - Tagged pointers for discriminant-free representation
//! - SSO for arrays fitting in 3 machine words
//! - Type specialization by element size

use crate::parsing::ast::Value;

/// Array element type classification for specialization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayElementType {
    /// 1-byte elements (u8, i8, bool)
    Byte,
    /// 2-byte elements (u16, i16)
    Short,
    /// 4-byte elements (u32, i32, f32)
    Word,
    /// 8-byte elements (u64, i64, f64)
    Long,
    /// 16-byte elements (u128, i128)
    Extended,
    /// Variable-size or mixed-type elements
    Any,
}

impl ArrayElementType {
    /// Size of a single element in bytes
    pub fn element_size(&self) -> usize {
        match self {
            ArrayElementType::Byte => 1,
            ArrayElementType::Short => 2,
            ArrayElementType::Word => 4,
            ArrayElementType::Long => 8,
            ArrayElementType::Extended => 16,
            ArrayElementType::Any => std::mem::size_of::<Value>(),
        }
    }

    /// Metadata size for length/capacity tracking
    /// Byte/Short/Word use u32 (4 bytes), Long/Extended/Any use usize (8 bytes on 64-bit)
    pub fn metadata_size(&self) -> usize {
        match self {
            ArrayElementType::Byte | ArrayElementType::Short | ArrayElementType::Word => 4,
            ArrayElementType::Long | ArrayElementType::Extended | ArrayElementType::Any => 8,
        }
    }

    /// Total metadata overhead (ptr + len + cap)
    pub fn total_metadata_bytes(&self) -> usize {
        8 + self.metadata_size() * 2
    }
}

/// Raw fixed-size array with zero runtime metadata overhead
///
/// Memory layout (data only, no header):
/// ```text
/// [elem₀][elem₁][elem₂]...[elemₙ]
/// ```
///
/// Benefits:
/// - Zero metadata overhead
/// - Direct memory access
/// - Cache-friendly sequential layout
/// - Compatible with C FFI
///
/// Use cases:
/// - Fixed-size buffers
/// - Mathematical vectors/matrices
/// - Low-level data structures
#[derive(Debug, Clone)]
pub struct RawArray {
    /// Element type for specialization
    pub element_type: String,
    /// Actual elements (stored as Value for compatibility)
    pub data: Vec<Value>,
    /// Compile-time known size (not stored at runtime in optimized tiers)
    pub size: usize,
}

impl RawArray {
    pub fn new(element_type: String, data: Vec<Value>) -> Self {
        let size = data.len();
        RawArray {
            element_type,
            data,
            size,
        }
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Memory overhead: 0 bytes in optimized tiers (size known at compile time)
    pub fn metadata_bytes(&self) -> usize {
        0
    }
}

/// Small-Size Optimized (SSO) array with inline storage
///
/// Memory layout for small arrays (≤24 bytes):
/// ```text
/// [discriminant:1][len:1][inline_data:22]
/// Total: 24 bytes (3 machine words on 64-bit)
/// ```
///
/// Memory layout for large arrays (>24 bytes):
/// ```text
/// [discriminant:1][padding:7][ptr:8][len:4][cap:4]
/// Total: 24 bytes
/// ```
///
/// Benefits:
/// - No heap allocation for small arrays
/// - Single cache line access
/// - Reduced allocator pressure
/// - Faster creation/destruction
///
/// Use cases:
/// - Small arrays (≤5 f32s, ≤11 i16s, ≤22 u8s)
/// - Function arguments
/// - Temporary arrays
#[derive(Debug, Clone)]
pub enum SSOArray {
    /// Inline storage for arrays ≤22 bytes
    Inline {
        /// Number of elements (not bytes)
        len: u8,
        /// Element type for interpretation
        element_type: ArrayElementType,
        /// Inline data buffer (up to 22 bytes)
        data: [u8; 22],
    },
    /// Heap allocation for larger arrays
    Heap {
        /// Pointer to heap-allocated data
        ptr: *mut u8,
        /// Number of elements
        len: u32,
        /// Allocated capacity
        cap: u32,
        /// Element type
        element_type: ArrayElementType,
    },
}

impl SSOArray {
    /// Create a new SSO array, choosing inline or heap representation
    pub fn new(element_type: ArrayElementType, data: Vec<Value>) -> Self {
        let len = data.len();
        let elem_size = element_type.element_size();
        let total_bytes = len * elem_size;

        if total_bytes <= 22 {
            // Use inline storage
            let mut inline_data = [0u8; 22];

            // Convert Values to bytes based on element type
            for (i, val) in data.iter().enumerate() {
                let offset = i * elem_size;
                Self::write_value_to_bytes(&mut inline_data[offset..], val, &element_type);
            }

            SSOArray::Inline {
                len: len as u8,
                element_type,
                data: inline_data,
            }
        } else {
            // Use heap storage - allocate properly
            let layout = std::alloc::Layout::from_size_align(total_bytes, elem_size)
                .unwrap_or_else(|_| std::alloc::Layout::from_size_align(total_bytes, 1).unwrap());

            let ptr = unsafe { std::alloc::alloc(layout) };

            if !ptr.is_null() {
                // Write values to heap memory
                for (i, val) in data.iter().enumerate() {
                    let offset = i * elem_size;
                    unsafe {
                        Self::write_value_to_bytes(
                            std::slice::from_raw_parts_mut(ptr.add(offset), elem_size),
                            val,
                            &element_type,
                        );
                    }
                }
            }

            SSOArray::Heap {
                ptr,
                len: len as u32,
                cap: len as u32,
                element_type,
            }
        }
    }

    /// Write a value to a byte slice based on element type
    fn write_value_to_bytes(buf: &mut [u8], val: &Value, elem_type: &ArrayElementType) {
        match (elem_type, val) {
            // Byte types (1 byte)
            (ArrayElementType::Byte, Value::U8(v)) => {
                if !buf.is_empty() {
                    buf[0] = *v;
                }
            }
            (ArrayElementType::Byte, Value::I8(v)) => {
                if !buf.is_empty() {
                    buf[0] = *v as u8;
                }
            }
            (ArrayElementType::Byte, Value::Bool(v)) => {
                if !buf.is_empty() {
                    buf[0] = if *v { 1 } else { 0 };
                }
            }

            // Short types (2 bytes)
            (ArrayElementType::Short, Value::U16(v)) => {
                if buf.len() >= 2 {
                    buf[..2].copy_from_slice(&v.to_le_bytes());
                }
            }
            (ArrayElementType::Short, Value::I16(v)) => {
                if buf.len() >= 2 {
                    buf[..2].copy_from_slice(&v.to_le_bytes());
                }
            }

            // Word types (4 bytes)
            (ArrayElementType::Word, Value::U32(v)) => {
                if buf.len() >= 4 {
                    buf[..4].copy_from_slice(&v.to_le_bytes());
                }
            }
            (ArrayElementType::Word, Value::I32(v)) => {
                if buf.len() >= 4 {
                    buf[..4].copy_from_slice(&v.to_le_bytes());
                }
            }
            (ArrayElementType::Word, Value::F32(v)) => {
                if buf.len() >= 4 {
                    buf[..4].copy_from_slice(&v.to_le_bytes());
                }
            }

            // Long types (8 bytes)
            (ArrayElementType::Long, Value::U64(v)) => {
                if buf.len() >= 8 {
                    buf[..8].copy_from_slice(&v.to_le_bytes());
                }
            }
            (ArrayElementType::Long, Value::I64(v)) => {
                if buf.len() >= 8 {
                    buf[..8].copy_from_slice(&v.to_le_bytes());
                }
            }
            (ArrayElementType::Long, Value::F64(v)) => {
                if buf.len() >= 8 {
                    buf[..8].copy_from_slice(&v.to_le_bytes());
                }
            }
            (ArrayElementType::Long, Value::Number(v)) => {
                if buf.len() >= 8 {
                    buf[..8].copy_from_slice(&v.to_le_bytes());
                }
            }

            // Extended types (16 bytes)
            (ArrayElementType::Extended, Value::U128(v)) => {
                if buf.len() >= 16 {
                    buf[..16].copy_from_slice(&v.to_le_bytes());
                }
            }
            (ArrayElementType::Extended, Value::I128(v)) => {
                if buf.len() >= 16 {
                    buf[..16].copy_from_slice(&v.to_le_bytes());
                }
            }

            // Any type - store discriminant + ptr (simplified)
            (ArrayElementType::Any, _) => {
                // For Any type, we'd typically store a tagged pointer
                // For now, zero-fill as placeholder
            }

            // Fallback for mismatched types
            _ => {}
        }
    }

    /// Read a value from a byte slice based on element type
    pub fn read_value_from_bytes(buf: &[u8], elem_type: &ArrayElementType) -> Option<Value> {
        match elem_type {
            ArrayElementType::Byte => {
                if !buf.is_empty() {
                    Some(Value::U8(buf[0]))
                } else {
                    None
                }
            }
            ArrayElementType::Short => {
                if buf.len() >= 2 {
                    let arr: [u8; 2] = buf[..2].try_into().ok()?;
                    Some(Value::U16(u16::from_le_bytes(arr)))
                } else {
                    None
                }
            }
            ArrayElementType::Word => {
                if buf.len() >= 4 {
                    let arr: [u8; 4] = buf[..4].try_into().ok()?;
                    Some(Value::U32(u32::from_le_bytes(arr)))
                } else {
                    None
                }
            }
            ArrayElementType::Long => {
                if buf.len() >= 8 {
                    let arr: [u8; 8] = buf[..8].try_into().ok()?;
                    Some(Value::Number(f64::from_le_bytes(arr)))
                } else {
                    None
                }
            }
            ArrayElementType::Extended => {
                if buf.len() >= 16 {
                    let arr: [u8; 16] = buf[..16].try_into().ok()?;
                    Some(Value::U128(u128::from_le_bytes(arr)))
                } else {
                    None
                }
            }
            ArrayElementType::Any => None, // Can't read back from Any without type info
        }
    }

    /// Get element at index (for inline arrays)
    pub fn get(&self, index: usize) -> Option<Value> {
        match self {
            SSOArray::Inline {
                len,
                element_type,
                data,
            } => {
                if index >= *len as usize {
                    return None;
                }
                let elem_size = element_type.element_size();
                let offset = index * elem_size;
                Self::read_value_from_bytes(&data[offset..], element_type)
            }
            SSOArray::Heap {
                ptr,
                len,
                element_type,
                ..
            } => {
                if index >= *len as usize || ptr.is_null() {
                    return None;
                }
                let elem_size = element_type.element_size();
                let offset = index * elem_size;
                unsafe {
                    let slice = std::slice::from_raw_parts(ptr.add(offset), elem_size);
                    Self::read_value_from_bytes(slice, element_type)
                }
            }
        }
    }

    /// Set element at index (for inline and heap SSO arrays)
    pub fn set(&mut self, index: usize, val: Value) -> Result<(), String> {
        match self {
            SSOArray::Inline {
                len,
                element_type,
                data,
            } => {
                if index >= *len as usize {
                    return Err(format!(
                        "Index {} out of bounds for SSO array of length {}",
                        index, len
                    ));
                }
                let elem_size = element_type.element_size();
                let offset = index * elem_size;
                Self::write_value_to_bytes(&mut data[offset..], &val, element_type);
                Ok(())
            }
            SSOArray::Heap {
                ptr,
                len,
                element_type,
                ..
            } => {
                if index >= *len as usize || ptr.is_null() {
                    return Err(format!(
                        "Index {} out of bounds for SSO array of length {}",
                        index, len
                    ));
                }
                let elem_size = element_type.element_size();
                let offset = index * elem_size;
                unsafe {
                    Self::write_value_to_bytes(
                        std::slice::from_raw_parts_mut(ptr.add(offset), elem_size),
                        &val,
                        element_type,
                    );
                }
                Ok(())
            }
        }
    }

    pub fn len(&self) -> usize {
        match self {
            SSOArray::Inline { len, .. } => *len as usize,
            SSOArray::Heap { len, .. } => *len as usize,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Memory overhead: 2 bytes for inline (discriminant + len), 16 bytes for heap
    pub fn metadata_bytes(&self) -> usize {
        match self {
            SSOArray::Inline { .. } => 2,
            SSOArray::Heap { .. } => 16, // ptr(8) + len(4) + cap(4)
        }
    }

    pub fn is_inline(&self) -> bool {
        matches!(self, SSOArray::Inline { .. })
    }
}

/// Compact array with u16 length/capacity for small collections
///
/// Memory layout:
/// ```text
/// [ptr:8][len:2][cap:2][elem_type:1][padding:3]
/// Total: 16 bytes
/// ```
///
/// Benefits:
/// - 8 bytes saved vs DynArray (for small element types)
/// - Supports up to 65,535 elements
/// - Good for typical application arrays
///
/// Use cases:
/// - Arrays with known small-to-medium size
/// - Collections in data structures
/// - API response arrays
#[derive(Debug, Clone)]
pub struct CompactArray {
    /// Pointer to heap-allocated data
    pub ptr: *mut Value,
    /// Number of elements (max 65,535)
    pub len: u16,
    /// Allocated capacity (max 65,535)
    pub cap: u16,
    /// Element type classification
    pub element_type: ArrayElementType,
}

impl CompactArray {
    pub fn new(element_type: ArrayElementType, data: Vec<Value>) -> Option<Self> {
        if data.len() > u16::MAX as usize {
            return None; // Too large for compact representation
        }

        let len = data.len() as u16;
        let cap = len;

        if data.is_empty() {
            return Some(CompactArray {
                ptr: std::ptr::null_mut(),
                len,
                cap,
                element_type,
            });
        }

        // Allocate heap memory for Value array
        let layout = std::alloc::Layout::array::<Value>(data.len()).ok()?;
        let ptr = unsafe { std::alloc::alloc(layout) as *mut Value };

        if ptr.is_null() {
            return None; // Allocation failed
        }

        // Copy values to heap
        unsafe {
            for (i, val) in data.into_iter().enumerate() {
                std::ptr::write(ptr.add(i), val);
            }
        }

        Some(CompactArray {
            ptr,
            len,
            cap,
            element_type,
        })
    }

    /// Get element at index
    pub fn get(&self, index: usize) -> Option<&Value> {
        if index >= self.len as usize || self.ptr.is_null() {
            return None;
        }
        unsafe { Some(&*self.ptr.add(index)) }
    }

    /// Get mutable element at index
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Value> {
        if index >= self.len as usize || self.ptr.is_null() {
            return None;
        }
        unsafe { Some(&mut *self.ptr.add(index)) }
    }

    /// Set element at index
    pub fn set(&mut self, index: usize, value: Value) -> bool {
        if index >= self.len as usize || self.ptr.is_null() {
            return false;
        }
        unsafe {
            std::ptr::write(self.ptr.add(index), value);
        }
        true
    }

    /// Convert to Vec<Value>
    pub fn to_vec(&self) -> Vec<Value> {
        if self.ptr.is_null() || self.len == 0 {
            return Vec::new();
        }

        let mut result = Vec::with_capacity(self.len as usize);
        for i in 0..self.len as usize {
            if let Some(val) = self.get(i) {
                result.push(val.clone());
            }
        }
        result
    }

    pub fn len(&self) -> usize {
        self.len as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Memory overhead: 16 bytes (ptr + len + cap + type + padding)
    pub fn metadata_bytes(&self) -> usize {
        16
    }
}

/// Full-featured dynamic array (existing DynamicArray)
///
/// Memory layout:
/// ```text
/// [ptr:8][len:4|8][cap:4|8][elem_type:1][concrete_type:var]
/// Total: 16-24 bytes + string allocation
/// ```
///
/// Benefits:
/// - Unbounded size
/// - Rich metadata (concrete type string)
/// - Full dynamic semantics
///
/// Use cases:
/// - Large arrays (>65,535 elements)
/// - Arrays needing growth
/// - Dynamic typing scenarios
#[derive(Debug, Clone)]
pub struct DynamicArray {
    /// Backing storage
    pub data: Vec<Value>,
    /// Element type classification
    pub element_type: ArrayElementType,
    /// Concrete type name for display (e.g., "u8", "f64")
    pub concrete_type: String,
    /// Whether we're tracking capacity explicitly
    pub tracked_capacity: bool,
}

impl DynamicArray {
    /// Create a new dynamic array with automatic type inference
    pub fn new(data: Vec<Value>) -> Self {
        let element_type = Self::infer_element_type(&data);
        let concrete_type = Self::infer_concrete_type(&data);
        DynamicArray {
            data,
            element_type,
            concrete_type,
            tracked_capacity: true,
        }
    }

    /// Create with explicit element type
    pub fn with_type(data: Vec<Value>, elem_type: ArrayElementType) -> Self {
        let concrete_type = format!("{:?}", elem_type).to_lowercase();
        DynamicArray {
            data,
            element_type: elem_type,
            concrete_type,
            tracked_capacity: true,
        }
    }

    /// Infer element type from data
    fn infer_element_type(data: &[Value]) -> ArrayElementType {
        if data.is_empty() {
            return ArrayElementType::Any;
        }

        // Check if all elements are the same type
        let first = &data[0];
        let all_same = data
            .iter()
            .all(|v| std::mem::discriminant(v) == std::mem::discriminant(first));

        if !all_same {
            return ArrayElementType::Any;
        }

        match first {
            Value::U8(_) | Value::I8(_) | Value::Bool(_) => ArrayElementType::Byte,
            Value::U16(_) | Value::I16(_) => ArrayElementType::Short,
            Value::U32(_) | Value::I32(_) | Value::F32(_) => ArrayElementType::Word,
            Value::U64(_) | Value::I64(_) | Value::F64(_) => ArrayElementType::Long,
            Value::U128(_) | Value::I128(_) => ArrayElementType::Extended,
            _ => ArrayElementType::Any,
        }
    }

    /// Infer concrete type name
    fn infer_concrete_type(data: &[Value]) -> String {
        if data.is_empty() {
            return "any".to_string();
        }

        match &data[0] {
            Value::U8(_) => "u8".to_string(),
            Value::I8(_) => "i8".to_string(),
            Value::U16(_) => "u16".to_string(),
            Value::I16(_) => "i16".to_string(),
            Value::U32(_) => "u32".to_string(),
            Value::I32(_) => "i32".to_string(),
            Value::U64(_) => "u64".to_string(),
            Value::I64(_) => "i64".to_string(),
            Value::U128(_) => "u128".to_string(),
            Value::I128(_) => "i128".to_string(),
            Value::F32(_) => "f32".to_string(),
            Value::F64(_) => "f64".to_string(),
            Value::Bool(_) => "bool".to_string(),
            _ => "any".to_string(),
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn capacity(&self) -> usize {
        self.data.capacity()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Memory overhead calculation
    pub fn metadata_bytes(&self) -> usize {
        8 + self.element_type.metadata_size() * 2
    }
}

/// Arena-allocated array using indices instead of pointers
///
/// Memory layout:
/// ```text
/// [arena_id:4][offset:4][len:4][elem_type:1][padding:3]
/// Total: 16 bytes
/// ```
///
/// Benefits:
/// - Compact representation (no 8-byte pointers)
/// - Arena-friendly (no individual allocations)
/// - Good for bulk allocation patterns
/// - GC-friendly (relocatable)
///
/// Use cases:
/// - Batch processing
/// - Temporary computation arrays
/// - Graph/tree node arrays
#[derive(Debug, Clone, Copy)]
pub struct ArenaArray {
    /// Arena identifier
    pub arena_id: u32,
    /// Offset within arena
    pub offset: u32,
    /// Number of elements
    pub len: u32,
    /// Element type
    pub element_type: ArrayElementType,
}

impl ArenaArray {
    pub fn new(arena_id: u32, offset: u32, len: u32, element_type: ArrayElementType) -> Self {
        ArenaArray {
            arena_id,
            offset,
            len,
            element_type,
        }
    }

    pub fn len(&self) -> usize {
        self.len as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Memory overhead: 16 bytes (arena_id + offset + len + type + padding)
    pub fn metadata_bytes(&self) -> usize {
        16
    }
}

/// Unified array representation for runtime
///
/// Uses tagged pointer or enum discriminant to distinguish array kinds
#[derive(Debug, Clone)]
pub enum ArrayKind {
    Raw(RawArray),
    SSO(SSOArray),
    Compact(CompactArray),
    Dynamic(DynamicArray),
    Arena(ArenaArray),
}

impl ArrayKind {
    pub fn len(&self) -> usize {
        match self {
            ArrayKind::Raw(a) => a.len(),
            ArrayKind::SSO(a) => a.len(),
            ArrayKind::Compact(a) => a.len(),
            ArrayKind::Dynamic(a) => a.len(),
            ArrayKind::Arena(a) => a.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn metadata_bytes(&self) -> usize {
        match self {
            ArrayKind::Raw(a) => a.metadata_bytes(),
            ArrayKind::SSO(a) => a.metadata_bytes(),
            ArrayKind::Compact(a) => a.metadata_bytes(),
            ArrayKind::Dynamic(a) => a.metadata_bytes(),
            ArrayKind::Arena(a) => a.metadata_bytes(),
        }
    }

    /// Get element type classification
    pub fn element_type(&self) -> ArrayElementType {
        match self {
            ArrayKind::Raw(_) => ArrayElementType::Any, // Determined by string type
            ArrayKind::SSO(SSOArray::Inline { element_type, .. }) => *element_type,
            ArrayKind::SSO(SSOArray::Heap { element_type, .. }) => *element_type,
            ArrayKind::Compact(a) => a.element_type,
            ArrayKind::Dynamic(a) => a.element_type,
            ArrayKind::Arena(a) => a.element_type,
        }
    }
}

/// Array allocation strategy selector
///
/// Chooses optimal array representation based on:
/// - Element type and size
/// - Array length
/// - Growth requirements
/// - Lifetime and scope
pub struct ArrayAllocator;

impl ArrayAllocator {
    /// Select optimal array representation
    pub fn allocate(element_type: ArrayElementType, len: usize, is_fixed_size: bool) -> ArrayKind {
        let elem_size = element_type.element_size();
        let total_bytes = len * elem_size;

        // Strategy 1: Use RawArray for compile-time fixed size
        if is_fixed_size {
            // In optimized tiers, this becomes a stack allocation with no metadata
            // For now, we create the structure for interpreter
            return ArrayKind::Raw(RawArray::new(
                format!("{:?}", element_type).to_lowercase(),
                Vec::new(),
            ));
        }

        // Strategy 2: Use SSO for small arrays
        if total_bytes <= 22 {
            return ArrayKind::SSO(SSOArray::new(element_type, Vec::new()));
        }

        // Strategy 3: Use CompactArray for small-to-medium arrays
        if len <= u16::MAX as usize {
            if let Some(compact) = CompactArray::new(element_type, Vec::new()) {
                return ArrayKind::Compact(compact);
            }
        }

        // Strategy 4: Fall back to DynamicArray for large or unbounded arrays
        ArrayKind::Dynamic(DynamicArray::with_type(Vec::new(), element_type))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_element_type_sizes() {
        assert_eq!(ArrayElementType::Byte.element_size(), 1);
        assert_eq!(ArrayElementType::Short.element_size(), 2);
        assert_eq!(ArrayElementType::Word.element_size(), 4);
        assert_eq!(ArrayElementType::Long.element_size(), 8);
        assert_eq!(ArrayElementType::Extended.element_size(), 16);
    }

    #[test]
    fn test_metadata_sizes() {
        assert_eq!(ArrayElementType::Byte.total_metadata_bytes(), 16);
        assert_eq!(ArrayElementType::Word.total_metadata_bytes(), 16);
        assert_eq!(ArrayElementType::Long.total_metadata_bytes(), 24);
    }

    #[test]
    fn test_sso_threshold() {
        // 22 bytes / 4 bytes per element = 5 f32s fit inline
        let element_type = ArrayElementType::Word;
        let sso = SSOArray::new(element_type, vec![]);
        assert!(sso.is_inline());
    }

    #[test]
    fn test_allocator_strategy() {
        // Small array should use SSO
        let small = ArrayAllocator::allocate(ArrayElementType::Byte, 10, false);
        assert!(matches!(small, ArrayKind::SSO(_)));

        // Medium array should use Compact
        let medium = ArrayAllocator::allocate(ArrayElementType::Word, 100, false);
        assert!(matches!(medium, ArrayKind::Compact(_)));

        // Fixed-size array should use Raw
        let fixed = ArrayAllocator::allocate(ArrayElementType::Byte, 10, true);
        assert!(matches!(fixed, ArrayKind::Raw(_)));
    }
}
