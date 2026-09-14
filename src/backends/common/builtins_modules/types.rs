//! Type definitions for builtin runtime values and functions

use crate::utils::collections::FastMap;
use num_bigint::BigInt;

/// Unique promise identifier
pub type PromiseId = u64;

/// Function pointer type for runtime builtins
pub type BuiltinFn = fn(&[RuntimeValue]) -> RuntimeValue;

/// Callable function representation for Promise handlers
#[derive(Debug, Clone)]
pub struct CallableFunction {
    pub name: String,
    pub params: Vec<String>,
    /// For closures, captured variables
    pub captures: FastMap<String, RuntimeValue>,
    /// Whether this is an async function (returns Promise automatically)
    pub is_async: bool,
}

impl PartialEq for CallableFunction {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

/// Runtime value representation for builtins
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Char(char),
    String(String),
    Array(Vec<RuntimeValue>),
    Set(Vec<RuntimeValue>),
    Tuple(Vec<RuntimeValue>),
    Object(FastMap<String, RuntimeValue>),
    /// Promise with an ID referencing the global promise table
    Promise(PromiseId),
    /// Function reference for callbacks/handlers
    Function(CallableFunction),
    /// Arbitrary precision integer for large numbers
    BigInt(BigInt),
    // Fixed-width integer types (unsigned)
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    // Fixed-width integer types (signed)
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    // Fixed-width float types
    F32(f32),
    F64(f64),
    /// Raw array with element type and zero overhead
    RawArray(String, Vec<RuntimeValue>),
    /// Dynamic array with type metadata
    DynArray {
        data: Vec<RuntimeValue>,
        element_type: String,
        concrete_type: String,
        tracked_capacity: Option<usize>,
    },
    Null,
}

impl RuntimeValue {
    pub fn as_int(&self) -> Option<i64> {
        match self {
            RuntimeValue::Int(n) => Some(*n),
            RuntimeValue::Float(n) => Some(*n as i64),
            RuntimeValue::Char(c) => Some(*c as i64),
            RuntimeValue::BigInt(bi) => bi.try_into().ok(),
            // Fixed-width types
            RuntimeValue::U8(n) => Some(*n as i64),
            RuntimeValue::U16(n) => Some(*n as i64),
            RuntimeValue::U32(n) => Some(*n as i64),
            RuntimeValue::U64(n) => Some(*n as i64),
            RuntimeValue::U128(n) => (*n).try_into().ok(),
            RuntimeValue::I8(n) => Some(*n as i64),
            RuntimeValue::I16(n) => Some(*n as i64),
            RuntimeValue::I32(n) => Some(*n as i64),
            RuntimeValue::I64(n) => Some(*n),
            RuntimeValue::I128(n) => (*n).try_into().ok(),
            RuntimeValue::F32(n) => Some(*n as i64),
            RuntimeValue::F64(n) => Some(*n as i64),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            RuntimeValue::Float(n) => Some(*n),
            RuntimeValue::Int(n) => Some(*n as f64),
            RuntimeValue::Char(c) => Some(*c as u32 as f64),
            RuntimeValue::BigInt(bi) => {
                // Convert BigInt to f64 (may lose precision for very large numbers)
                use num_traits::ToPrimitive;
                bi.to_f64()
            }
            // Fixed-width types
            RuntimeValue::U8(n) => Some(*n as f64),
            RuntimeValue::U16(n) => Some(*n as f64),
            RuntimeValue::U32(n) => Some(*n as f64),
            RuntimeValue::U64(n) => Some(*n as f64),
            RuntimeValue::U128(n) => Some(*n as f64),
            RuntimeValue::I8(n) => Some(*n as f64),
            RuntimeValue::I16(n) => Some(*n as f64),
            RuntimeValue::I32(n) => Some(*n as f64),
            RuntimeValue::I64(n) => Some(*n as f64),
            RuntimeValue::I128(n) => Some(*n as f64),
            RuntimeValue::F32(n) => Some(*n as f64),
            RuntimeValue::F64(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            RuntimeValue::Bool(b) => Some(*b),
            RuntimeValue::Int(n) => Some(*n != 0),
            RuntimeValue::Float(n) => Some(*n != 0.0),
            RuntimeValue::Char(c) => Some(*c != '\0'),
            RuntimeValue::BigInt(bi) => Some(*bi != BigInt::from(0)),
            // Fixed-width types
            RuntimeValue::U8(n) => Some(*n != 0),
            RuntimeValue::U16(n) => Some(*n != 0),
            RuntimeValue::U32(n) => Some(*n != 0),
            RuntimeValue::U64(n) => Some(*n != 0),
            RuntimeValue::U128(n) => Some(*n != 0),
            RuntimeValue::I8(n) => Some(*n != 0),
            RuntimeValue::I16(n) => Some(*n != 0),
            RuntimeValue::I32(n) => Some(*n != 0),
            RuntimeValue::I64(n) => Some(*n != 0),
            RuntimeValue::I128(n) => Some(*n != 0),
            RuntimeValue::F32(n) => Some(*n != 0.0),
            RuntimeValue::F64(n) => Some(*n != 0.0),
            _ => None,
        }
    }

    pub fn as_promise(&self) -> Option<PromiseId> {
        match self {
            RuntimeValue::Promise(id) => Some(*id),
            _ => None,
        }
    }

    pub fn as_function(&self) -> Option<&CallableFunction> {
        match self {
            RuntimeValue::Function(f) => Some(f),
            _ => None,
        }
    }

    /// Get value as BigInt, converting if necessary
    pub fn as_bigint(&self) -> Option<BigInt> {
        match self {
            RuntimeValue::BigInt(bi) => Some(bi.clone()),
            RuntimeValue::Int(n) => Some(BigInt::from(*n)),
            RuntimeValue::Char(c) => Some(BigInt::from(*c as u32)),
            RuntimeValue::Float(n) if n.fract() == 0.0 => Some(BigInt::from(*n as i64)),
            // Handle fixed-width unsigned integers
            RuntimeValue::U8(n) => Some(BigInt::from(*n)),
            RuntimeValue::U16(n) => Some(BigInt::from(*n)),
            RuntimeValue::U32(n) => Some(BigInt::from(*n)),
            RuntimeValue::U64(n) => Some(BigInt::from(*n)),
            RuntimeValue::U128(n) => Some(BigInt::from(*n)),
            // Handle fixed-width signed integers
            RuntimeValue::I8(n) => Some(BigInt::from(*n)),
            RuntimeValue::I16(n) => Some(BigInt::from(*n)),
            RuntimeValue::I32(n) => Some(BigInt::from(*n)),
            RuntimeValue::I64(n) => Some(BigInt::from(*n)),
            RuntimeValue::I128(n) => Some(BigInt::from(*n)),
            // Handle fixed-width floats
            RuntimeValue::F32(n) if n.fract() == 0.0 => Some(BigInt::from(*n as i64)),
            RuntimeValue::F64(n) if n.fract() == 0.0 => Some(BigInt::from(*n as i64)),
            _ => None,
        }
    }

    /// Check if this value is a BigInt
    pub fn is_bigint(&self) -> bool {
        matches!(self, RuntimeValue::BigInt(_))
    }

    /// Check if this value is any numeric type (Int, Float, BigInt, or fixed-width)
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            RuntimeValue::Int(_)
                | RuntimeValue::Float(_)
                | RuntimeValue::Char(_)
                | RuntimeValue::BigInt(_)
                | RuntimeValue::U8(_)
                | RuntimeValue::U16(_)
                | RuntimeValue::U32(_)
                | RuntimeValue::U64(_)
                | RuntimeValue::U128(_)
                | RuntimeValue::I8(_)
                | RuntimeValue::I16(_)
                | RuntimeValue::I32(_)
                | RuntimeValue::I64(_)
                | RuntimeValue::I128(_)
                | RuntimeValue::F32(_)
                | RuntimeValue::F64(_)
        )
    }

    pub fn as_string(&self) -> String {
        match self {
            RuntimeValue::Int(n) => {
                // Fast path for integers using itoa
                itoa::Buffer::new().format(*n).to_string()
            }
            RuntimeValue::Float(n) => {
                // Fast path for floats using ryu
                ryu::Buffer::new().format(*n).to_string()
            }
            RuntimeValue::Bool(b) => {
                // Static strings - no allocation
                if *b {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            }
            RuntimeValue::Char(c) => c.to_string(),
            RuntimeValue::String(s) => s.clone(),
            RuntimeValue::BigInt(bi) => bi.to_string(),
            // Fixed-width integer types (unsigned) - use itoa
            RuntimeValue::U8(n) => itoa::Buffer::new().format(*n).to_string(),
            RuntimeValue::U16(n) => itoa::Buffer::new().format(*n).to_string(),
            RuntimeValue::U32(n) => itoa::Buffer::new().format(*n).to_string(),
            RuntimeValue::U64(n) => itoa::Buffer::new().format(*n).to_string(),
            RuntimeValue::U128(n) => itoa::Buffer::new().format(*n).to_string(),
            // Fixed-width integer types (signed) - use itoa
            RuntimeValue::I8(n) => itoa::Buffer::new().format(*n).to_string(),
            RuntimeValue::I16(n) => itoa::Buffer::new().format(*n).to_string(),
            RuntimeValue::I32(n) => itoa::Buffer::new().format(*n).to_string(),
            RuntimeValue::I64(n) => itoa::Buffer::new().format(*n).to_string(),
            RuntimeValue::I128(n) => itoa::Buffer::new().format(*n).to_string(),
            // Fixed-width float types - use ryu
            RuntimeValue::F32(n) => ryu::Buffer::new().format(*n).to_string(),
            RuntimeValue::F64(n) => ryu::Buffer::new().format(*n).to_string(),
            RuntimeValue::Array(arr) => {
                let items: Vec<String> = arr.iter().map(|v| v.as_string()).collect();
                format!("[{}]", items.join(", "))
            }
            RuntimeValue::Set(set_vals) => {
                let items: Vec<String> = set_vals.iter().map(|v| v.as_string()).collect();
                format!("{{{}}}", items.join(", "))
            }
            RuntimeValue::Tuple(tup) => {
                if tup.is_empty() {
                    "()".to_string()
                } else if tup.len() == 1 {
                    format!("({},)", tup[0].as_string())
                } else {
                    let items: Vec<String> = tup.iter().map(|v| v.as_string()).collect();
                    format!("({})", items.join(", "))
                }
            }
            RuntimeValue::Object(obj) => {
                let mut entries: Vec<String> = Vec::new();
                for (k, v) in obj.iter() {
                    // For object formatting we intentionally don't quote strings: `b: x`
                    let value_str = v.as_string();
                    entries.push(format!("{}: {}", k, value_str));
                }
                format!("{{{}}}", entries.join(", "))
            }
            RuntimeValue::Promise(id) => format!("Promise({})", id),
            RuntimeValue::Function(f) => format!("Function({})", f.name),
            RuntimeValue::RawArray(_elem_type, values) => {
                let items: Vec<String> = values.iter().map(|v| v.as_string()).collect();
                format!("[{}]", items.join(", "))
            }
            RuntimeValue::DynArray { data, .. } => {
                let items: Vec<String> = data.iter().map(|v| v.as_string()).collect();
                format!("[{}]", items.join(", "))
            }
            RuntimeValue::Null => "null".to_string(),
        }
    }
}
