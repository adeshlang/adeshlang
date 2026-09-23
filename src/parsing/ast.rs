//! Abstract Syntax Tree (AST)
//!
//! Defines the surface-language AST nodes used by the interpreter, emitter,
//! and lowering passes. Supports rich expressions (functions, async/await,
//! ranges, optional chaining, updates), statements (classes, enums, imports),
//! and runtime `Value` variants for evaluation and builtins.
//!
//! The AST balances expressiveness with straightforward parsing and later
//! lowering into `HIR` and `LIR`.
use num_bigint::BigInt;
use num_traits::ToPrimitive;
use rustc_hash::FxHashMap as HashMap;
use std::any::Any;
use std::fmt;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, RwLock};

pub trait InterpreterEnv: Any {
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

pub enum NativeEffect {
    RegisterPromise(u64),
    AttachThen(u64, Option<Value>, Option<Value>, u64),
    ResolvePromise(u64, Value),
    RejectPromise(u64, Value),
    RegisterTimer(u64, Value, Arc<AtomicBool>, bool, u64),
    CancelTimer(u64),
    EnqueueMicrotask(Box<dyn FnOnce(&mut dyn InterpreterEnv) + 'static>),
}

pub trait BuiltinEnv {
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn native_side_effects(&self) -> Option<Arc<Mutex<Vec<NativeEffect>>>>;
    // Create a promise with the provided executor. Returns a Value::Promise(id)
    fn create_promise_executor(&mut self, executor: Value) -> Result<Value, String>;
    // schedule resolve/reject for a promise id from this env
    fn schedule_resolve(&mut self, id: u64, v: Value);
    fn schedule_reject(&mut self, id: u64, v: Value);
    // timers API (may be unsupported in some envs)
    fn alloc_timer_id(&mut self) -> Result<u64, String>;
    fn get_timer_sender(&self) -> Option<mpsc::Sender<u64>>;
    fn register_timer(
        &mut self,
        id: u64,
        callback: Value,
        cancel_flag: Arc<AtomicBool>,
        is_interval: bool,
        ms: u64,
    ) -> Result<(), String>;
    fn cancel_timer(&mut self, id: u64) -> Result<(), String>;
    // (event loop driver exposed as an inherent method on Interpreter)
}

pub struct NoopEnv;

impl BuiltinEnv for NoopEnv {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn native_side_effects(&self) -> Option<Arc<Mutex<Vec<NativeEffect>>>> {
        None
    }
    fn create_promise_executor(&mut self, _executor: Value) -> Result<Value, String> {
        Err("Promises unsupported in NoopEnv".into())
    }
    fn schedule_resolve(&mut self, _id: u64, _v: Value) {}
    fn schedule_reject(&mut self, _id: u64, _v: Value) {}
    fn alloc_timer_id(&mut self) -> Result<u64, String> {
        Err("Timers unsupported in NoopEnv".into())
    }
    fn get_timer_sender(&self) -> Option<mpsc::Sender<u64>> {
        None
    }
    fn register_timer(
        &mut self,
        _id: u64,
        _cb: Value,
        _cancel: Arc<AtomicBool>,
        _interval: bool,
        _ms: u64,
    ) -> Result<(), String> {
        Err("Timers unsupported in NoopEnv".into())
    }
    fn cancel_timer(&mut self, _id: u64) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub enum Pattern {
    Literal(Value),
    Variable(String),
    Wildcard,                          // _
    Or(Box<Pattern>, Box<Pattern>),    // pat | pat
    EnumVariant(String, Vec<Pattern>), // Enum variant pattern: Some(x)
}

/// Represents the element type stored in an array for type-aware metadata sizing
#[derive(Clone, Debug, PartialEq)]
pub enum ArrayElementType {
    /// Generic/dynamic - uses full 8 byte metadata fields
    Any,
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
}

impl ArrayElementType {
    /// Returns the byte size of each element for this type
    pub fn element_size(&self) -> usize {
        match self {
            ArrayElementType::Any => 8, // Dynamic, assume 8 bytes
            ArrayElementType::Byte => 1,
            ArrayElementType::Short => 2,
            ArrayElementType::Word => 4,
            ArrayElementType::Long => 8,
            ArrayElementType::Extended => 16,
        }
    }

    /// Parses a type annotation string to determine element type
    pub fn from_type_name(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "u8" | "i8" | "bool" | "byte" => ArrayElementType::Byte,
            "u16" | "i16" | "short" => ArrayElementType::Short,
            "u32" | "i32" | "f32" => ArrayElementType::Word,
            "u64" | "i64" | "f64" => ArrayElementType::Long,
            "u128" | "i128" => ArrayElementType::Extended,
            _ => ArrayElementType::Any,
        }
    }

    /// Returns the appropriate metadata size based on element type.
    /// Smaller elements use 4-byte length/capacity fields (limiting to ~4 billion elements).
    /// This is a space-time tradeoff: smaller arrays save memory, while Any/Long types
    /// use 8-byte fields for maximum capacity support.
    pub fn metadata_size(&self) -> usize {
        match self {
            ArrayElementType::Byte | ArrayElementType::Short => 4, // u32 for len/cap (max 4B elements)
            ArrayElementType::Word => 4,                           // u32 for len/cap
            ArrayElementType::Long | ArrayElementType::Extended | ArrayElementType::Any => 8, // usize (full capacity)
        }
    }
}

/// DynamicArray with efficient metadata tracking
#[derive(Clone, Debug)]
pub struct DynamicArray {
    /// The actual array data
    pub data: Vec<Value>,
    /// Element type for efficient storage
    pub element_type: ArrayElementType,
    /// The concrete type name (e.g., "u8", "i32", "f64")
    pub concrete_type: String,
    /// Current capacity (may differ from data.capacity() for tracking)
    pub tracked_capacity: usize,
}

impl DynamicArray {
    /// Create a new dynamic array from values with inferred element type
    pub fn new(values: Vec<Value>) -> Self {
        // Infer a concrete element type (u8/i8/u16/..., f32/f64) based on all
        // values in the array and convert stored values to that concrete type
        // so that sizeof/typeof reflect the chosen fixed-width type.
        let (concrete, element_type) = Self::infer_concrete_type_and_element(&values);
        let mut data_converted: Vec<Value> = Vec::with_capacity(values.len());
        for v in values.into_iter() {
            let converted = match concrete.as_str() {
                "u8" => match v {
                    Value::Number(n) => Value::U8(n as u8),
                    Value::BigInt(bi) => Value::U8(bi.to_u8().unwrap_or(0)),
                    Value::U8(x) => Value::U8(x),
                    Value::I8(x) => Value::U8(x as u8),
                    Value::U16(x) => Value::U8(x as u8),
                    Value::I16(x) => Value::U8(x as u8),
                    _ => v,
                },
                "i8" => match v {
                    Value::Number(n) => Value::I8(n as i8),
                    Value::BigInt(bi) => Value::I8(bi.to_i8().unwrap_or(0)),
                    Value::I8(x) => Value::I8(x),
                    Value::U8(x) => Value::I8(x as i8),
                    Value::U16(x) => Value::I8(x as i8),
                    _ => v,
                },
                "u16" => match v {
                    Value::Number(n) => Value::U16(n as u16),
                    Value::BigInt(bi) => Value::U16(bi.to_u16().unwrap_or(0)),
                    Value::U16(x) => Value::U16(x),
                    Value::I16(x) => Value::U16(x as u16),
                    _ => v,
                },
                "i16" => match v {
                    Value::Number(n) => Value::I16(n as i16),
                    Value::BigInt(bi) => Value::I16(bi.to_i16().unwrap_or(0)),
                    Value::I16(x) => Value::I16(x),
                    Value::U16(x) => Value::I16(x as i16),
                    _ => v,
                },
                "u32" => match v {
                    Value::Number(n) => Value::U32(n as u32),
                    Value::BigInt(bi) => Value::U32(bi.to_u32().unwrap_or(0)),
                    Value::U32(x) => Value::U32(x),
                    Value::I32(x) => Value::U32(x as u32),
                    _ => v,
                },
                "i32" => match v {
                    Value::Number(n) => Value::I32(n as i32),
                    Value::BigInt(bi) => Value::I32(bi.to_i32().unwrap_or(0)),
                    Value::I32(x) => Value::I32(x),
                    Value::U32(x) => Value::I32(x as i32),
                    _ => v,
                },
                "u64" => match v {
                    Value::Number(n) => Value::U64(n as u64),
                    Value::BigInt(bi) => Value::U64(bi.to_u64().unwrap_or(0)),
                    Value::U64(x) => Value::U64(x),
                    Value::I64(x) => Value::U64(x as u64),
                    _ => v,
                },
                "i64" => match v {
                    Value::Number(n) => Value::I64(n as i64),
                    Value::BigInt(bi) => Value::I64(bi.to_i64().unwrap_or(0)),
                    Value::I64(x) => Value::I64(x),
                    Value::U64(x) => Value::I64(x as i64),
                    _ => v,
                },
                "u128" => match v {
                    Value::Number(n) => Value::U128(n as u128),
                    Value::BigInt(bi) => Value::U128(bi.to_u128().unwrap_or(0)),
                    Value::U128(x) => Value::U128(x),
                    Value::I128(x) => Value::U128(x as u128),
                    _ => v,
                },
                "i128" => match v {
                    Value::Number(n) => Value::I128(n as i128),
                    Value::BigInt(bi) => Value::I128(bi.to_i128().unwrap_or(0)),
                    Value::I128(x) => Value::I128(x),
                    Value::U128(x) => Value::I128(x as i128),
                    _ => v,
                },
                "f32" => match v {
                    Value::Number(n) => Value::F32(n as f32),
                    Value::F64(n) => Value::F32(n as f32),
                    Value::F32(x) => Value::F32(x),
                    _ => v,
                },
                "f64" => match v {
                    Value::Number(n) => Value::F64(n),
                    Value::F32(n) => Value::F64(n as f64),
                    Value::F64(x) => Value::F64(x),
                    _ => v,
                },
                _ => v,
            };
            data_converted.push(converted);
        }
        DynamicArray {
            data: data_converted,
            element_type,
            concrete_type: concrete,
            tracked_capacity: 0,
        }
    }

    /// Create a new dynamic array with explicit element type
    pub fn with_type(values: Vec<Value>, type_name: &str) -> Self {
        let element_type = ArrayElementType::from_type_name(type_name);
        DynamicArray {
            data: values,
            element_type,
            concrete_type: type_name.to_string(),
            tracked_capacity: 0,
        }
    }

    /// Create a new dynamic array with explicit element type and declared capacity
    pub fn with_type_and_capacity(
        values: Vec<Value>,
        type_name: &str,
        declared_capacity: usize,
    ) -> Self {
        let element_type = ArrayElementType::from_type_name(type_name);
        let capacity = values.capacity();
        DynamicArray {
            data: values,
            element_type,
            concrete_type: type_name.to_string(),
            tracked_capacity: declared_capacity.max(capacity),
        }
    }

    /// Inspect all values and pick a concrete minimal-width type (u8/i8/.../f32/f64)
    /// Returns (type_name, ArrayElementType)
    fn infer_concrete_type_and_element(values: &[Value]) -> (String, ArrayElementType) {
        if values.is_empty() {
            return ("number".to_string(), ArrayElementType::Any);
        }

        let mut any_float = false;
        let mut any_bigint = false;
        let mut min_signed: Option<i128> = None;
        let mut max_signed: Option<i128> = None;
        let mut max_unsigned: Option<u128> = None;

        for v in values.iter() {
            match v {
                &Value::F32(n) => {
                    any_float = true;
                    let n64 = n as f64;
                    if n64.fract() != 0.0 {
                        any_float = true;
                    }
                }
                &Value::F64(n) => {
                    any_float = true;
                    if n.fract() != 0.0 {
                        any_float = true;
                    }
                }
                &Value::Number(n) => {
                    if n.fract() != 0.0 {
                        any_float = true;
                    } else {
                        // integer-valued f64
                        if n < 0.0 {
                            let i = n as i128;
                            min_signed = Some(min_signed.map_or(i, |m| m.min(i)));
                            max_signed = Some(max_signed.map_or(i, |m| m.max(i)));
                        } else {
                            let u = n as u128;
                            max_unsigned = Some(max_unsigned.map_or(u, |m| m.max(u)));
                        }
                    }
                }
                Value::BigInt(_bi) => {
                    any_bigint = true;
                }
                &Value::U8(x) => {
                    let u = x as u128;
                    max_unsigned = Some(max_unsigned.map_or(u, |m| m.max(u)));
                }
                &Value::I8(x) => {
                    let i = x as i128;
                    min_signed = Some(min_signed.map_or(i, |m| m.min(i)));
                    max_signed = Some(max_signed.map_or(i, |m| m.max(i)));
                }
                &Value::U16(x) => {
                    let u = x as u128;
                    max_unsigned = Some(max_unsigned.map_or(u, |m| m.max(u)));
                }
                &Value::I16(x) => {
                    let i = x as i128;
                    min_signed = Some(min_signed.map_or(i, |m| m.min(i)));
                    max_signed = Some(max_signed.map_or(i, |m| m.max(i)));
                }
                &Value::U32(x) => {
                    let u = x as u128;
                    max_unsigned = Some(max_unsigned.map_or(u, |m| m.max(u)));
                }
                &Value::I32(x) => {
                    let i = x as i128;
                    min_signed = Some(min_signed.map_or(i, |m| m.min(i)));
                    max_signed = Some(max_signed.map_or(i, |m| m.max(i)));
                }
                &Value::U64(x) => {
                    let u = x as u128;
                    max_unsigned = Some(max_unsigned.map_or(u, |m| m.max(u)));
                }
                &Value::I64(x) => {
                    let i = x as i128;
                    min_signed = Some(min_signed.map_or(i, |m| m.min(i)));
                    max_signed = Some(max_signed.map_or(i, |m| m.max(i)));
                }
                &Value::U128(x) => {
                    let u = x as u128;
                    max_unsigned = Some(max_unsigned.map_or(u, |m| m.max(u)));
                }
                &Value::I128(x) => {
                    let i = x as i128;
                    min_signed = Some(min_signed.map_or(i, |m| m.min(i)));
                    max_signed = Some(max_signed.map_or(i, |m| m.max(i)));
                }
                _ => {
                    // Non-numeric -> fallback to Any
                    return ("number".to_string(), ArrayElementType::Any);
                }
            }
        }

        if any_bigint {
            return ("number".to_string(), ArrayElementType::Any);
        }

        // If any non-integer float present, choose float type
        if any_float {
            // Check if all floats fit in f32 without precision loss
            let mut all_f32 = true;
            for v in values.iter() {
                match v {
                    &Value::Number(n) => {
                        let as_f32 = n as f32 as f64;
                        if (as_f32 - n).abs() > 1e-6 {
                            all_f32 = false;
                            break;
                        }
                    }
                    Value::F32(_) => {}
                    &Value::F64(n) => {
                        let as_f32 = n as f32 as f64;
                        if (as_f32 - n).abs() > 1e-6 {
                            all_f32 = false;
                            break;
                        }
                    }
                    _ => {
                        all_f32 = false;
                        break;
                    }
                }
            }
            if all_f32 {
                return ("f32".to_string(), ArrayElementType::Word);
            } else {
                return ("f64".to_string(), ArrayElementType::Long);
            }
        }

        // Integer-only values: decide signed vs unsigned
        let signed_needed = min_signed.is_some();
        if signed_needed {
            let min_v = min_signed.unwrap_or(0);
            let max_v = max_signed.unwrap_or(min_v);
            if min_v >= i8::MIN as i128 && max_v <= i8::MAX as i128 {
                return ("i8".to_string(), ArrayElementType::Byte);
            } else if min_v >= i16::MIN as i128 && max_v <= i16::MAX as i128 {
                return ("i16".to_string(), ArrayElementType::Short);
            } else if min_v >= i32::MIN as i128 && max_v <= i32::MAX as i128 {
                return ("i32".to_string(), ArrayElementType::Word);
            } else if min_v >= i64::MIN as i128 && max_v <= i64::MAX as i128 {
                return ("i64".to_string(), ArrayElementType::Long);
            } else {
                return ("i128".to_string(), ArrayElementType::Extended);
            }
        } else {
            let max_u = max_unsigned.unwrap_or(0);
            if max_u <= u8::MAX as u128 {
                return ("u8".to_string(), ArrayElementType::Byte);
            } else if max_u <= u16::MAX as u128 {
                return ("u16".to_string(), ArrayElementType::Short);
            } else if max_u <= u32::MAX as u128 {
                return ("u32".to_string(), ArrayElementType::Word);
            } else if max_u <= u64::MAX as u128 {
                return ("u64".to_string(), ArrayElementType::Long);
            } else {
                return ("u128".to_string(), ArrayElementType::Extended);
            }
        }
    }

    /// Get the length of the array
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if the array is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Get the current capacity.
    /// Uses max of tracked and actual capacity to handle cases where Vec has grown
    /// beyond what we've explicitly tracked (e.g., from automatic doubling).
    pub fn capacity(&self) -> usize {
        self.tracked_capacity.max(self.data.capacity())
    }

    /// Reserve additional capacity
    pub fn reserve(&mut self, additional: usize) {
        self.data.reserve(additional);
        self.tracked_capacity = self.data.capacity();
    }

    /// Push a value to the array
    pub fn push(&mut self, value: Value) {
        self.data.push(value);
        self.tracked_capacity = self.data.capacity();
    }

    /// Pop a value from the array
    pub fn pop(&mut self) -> Option<Value> {
        self.data.pop()
    }

    /// Get the total metadata size in bytes
    pub fn metadata_bytes(&self) -> usize {
        // pointer (8) + length field + capacity field
        8 + self.element_type.metadata_size() * 2
    }

    /// Get the total data size in bytes (excluding metadata)
    pub fn data_bytes(&self) -> usize {
        self.data.len() * self.element_type.element_size()
    }

    /// Get the total memory footprint (metadata + data)
    pub fn total_bytes(&self) -> usize {
        self.metadata_bytes() + self.data_bytes()
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Literal(Value),
    Variable(String),
    Assign(String, Box<Expr>),
    AssignOp(Box<Expr>, TokenKind, Box<Expr>),
    /// Tuple/array destructuring assignment: [a, b, c] = expr or (a, b) = expr
    AssignTuple(Vec<String>, Box<Expr>),
    /// Object destructuring assignment: {x, y: newY} = expr
    AssignObject(Vec<(String, Option<String>)>, Box<Expr>),
    Unary(TokenKind, Box<Expr>),
    Binary(Box<Expr>, TokenKind, Box<Expr>),
    Logical(Box<Expr>, TokenKind, Box<Expr>),
    Grouping(Box<Expr>),
    Call(Box<Expr>, Vec<Expr>, Vec<String>),
    Array(Vec<Expr>),
    Tuple(Vec<Expr>),
    Object(Vec<(String, Expr)>),
    StructLiteral(String, Vec<(String, Expr)>),
    SetLiteral(Vec<Expr>),
    Get(Box<Expr>, String),
    Set(Box<Expr>, String, Box<Expr>),
    // function literal: params, body, is_async
    // Optimized: Use Arc to avoid cloning function bodies in anonymous functions
    Fn(
        Vec<(String, Option<Expr>, Option<String>)>,
        std::sync::Arc<Vec<Stmt>>,
        bool,
    ),
    New(Box<Expr>, Vec<Expr>),
    Await(Box<Expr>),
    Spawn(Box<Expr>),
    Throw(Box<Expr>),
    // Spread operator: ...expr
    Spread(Box<Expr>),
    // Range operator: start..end or start..=end (inclusive)
    Range(Box<Expr>, Box<Expr>, bool), // bool = inclusive (..=) vs exclusive (..)
    // Dynamic index expression: target[index]
    Index(Box<Expr>, Box<Expr>),
    NonNull(Box<Expr>),
    OptGet(Box<Expr>, String),
    OptCall(Box<Expr>, Vec<Expr>, Vec<String>),
    Try(Box<Expr>),
    Conditional(Box<Expr>, Box<Expr>, Box<Expr>),
    Update(bool, bool, Box<Expr>),
    Match(Box<Expr>, Vec<(Pattern, Expr)>),
    /// Format expression: expr with format specifier string (e.g., `${x:>10}`, `${n:hex}`)
    Format(Box<Expr>, String),
    /// Cast expression: expr as type
    Cast(Box<Expr>, String),
}

#[derive(Clone, Debug, Default)]
pub struct Span {
    pub line: usize,
    pub col: usize,
    pub line_text: String,
}

#[derive(Clone, Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    ShareDeclaration(ShareDecl, bool /*export*/),
    StrongDeclaration(StrongDecl, bool /*export*/),
    WeakDeclaration(WeakDecl, bool /*export*/),
    TypeAlias(TypeAliasDecl, bool /*export*/),
    Let(
        String,
        Option<Expr>,
        Option<String>, /*type annotation*/
        bool,           /*export*/
        bool,           /*const*/
        bool,           /*readonly*/
    ),
    /// Tuple destructuring: let (a, b, c): (T1, T2, T3) = expr;
    /// The Option<Vec<String>> contains per-variable type annotations if provided
    LetTuple(
        Vec<String>,
        Option<Vec<String>>, /*type annotations*/
        Option<Expr>,
        bool, /*export*/
        bool, /*const*/
        bool, /*readonly*/
    ),
    /// Object destructuring: let { x, y, z } = obj;
    /// Vec<(String, Option<String>)> contains (key, alias) pairs
    /// alias is Some(name) for `{ x: newName }` syntax, None for `{ x }` shorthand
    LetObject(
        Vec<(String, Option<String>)>, /*key-alias pairs*/
        Option<Expr>,                  /*initializer*/
        bool,                          /*export*/
        bool,                          /*const*/
        bool,                          /*readonly*/
    ),
    ExprStmt(Expr),
    Block(Vec<Stmt>),
    If {
        cond: Expr,
        then_branch: Box<Stmt>,
        else_branch: Option<Box<Stmt>>,
    },
    While {
        cond: Expr,
        body: Box<Stmt>,
    },
    ForIn {
        name: String,
        iter: Expr,
        body: Box<Stmt>,
    },
    Break,
    Continue,
    Jump(Expr),
    /// extend [name]? on Type { fn ... } -- add methods to an existing class at runtime
    Extend(Option<String>, String, Vec<Function>, bool /*export*/),
    Struct(StructDecl, bool /*export*/),
    Enum(EnumDecl, bool /*export*/),
    #[allow(dead_code)]
    Interface(InterfaceDecl, bool /*export*/),
    Function(Function, bool /*export*/),
    Class(ClassDecl, bool /*export*/),
    ExportDefaultFunction(Function),
    ExportDefaultClass(ClassDecl),
    ExportDefault(String),
    Return(Option<Expr>),
    Import {
        path: String,
        alias: String,
    },
    ImportDefault {
        path: String,
        alias: String,
    },
    ImportNames {
        path: String,
        names: Vec<String>,
    },
    /// FFI: C header import @cImport("header.h")
    HeaderImport {
        path: String,
    },
    /// FFI: extern "ABI" fn name(params) -> ret_type
    ExternFunction(ExternFunctionDecl),
    /// FFI: extern "ABI" { fn ..., fn ... }
    ExternBlock {
        abi: String,
        functions: Vec<ExternFunctionDecl>,
    },
    TryCatch {
        try_block: Box<Stmt>,
        err_name: String,
        catch_block: Box<Stmt>,
    },
    /// Region block for arena-based memory management: region Name { statements }
    Region {
        name: Option<String>,
        body: Box<Stmt>,
    },
    /// Unsafe block: unsafe { statements }
    UnsafeBlock(Box<Stmt>),
    /// Defer block: defer { statements } - executes at scope exit in LIFO order
    Defer(Box<Stmt>),
    /// Decorator declaration with phases: decorator name(params) { compile { ... } runtime { ... } }
    Decorator(DecoratorDef, bool /*export*/),
}

#[derive(Clone, Debug)]
pub enum Visibility {
    Pub,
    Priv,
    Protected,
}

/// Decorator phase types - each decorator can implement one or more phases
#[derive(Clone, Debug)]
pub enum DecoratorPhase {
    /// Compile-time AST transformation and metadata injection
    Compile(std::sync::Arc<Vec<Stmt>>),
    /// Runtime wrapper/interception
    Runtime(std::sync::Arc<Vec<Stmt>>),
    /// Compile-time type constraints
    Typecheck(std::sync::Arc<Vec<Stmt>>),
    /// Backend-specific IR mutation hooks
    Emit(std::sync::Arc<Vec<Stmt>>),
}

/// Decorator definition with phases
#[derive(Clone, Debug)]
pub struct DecoratorDef {
    pub name: String,
    pub params: Vec<(String, Option<Expr>, Option<String>)>,
    pub phases: Vec<DecoratorPhase>,
    pub requires_unsafe: bool,
    pub is_new_style: bool,
}

/// A single stage in the decorator pipeline
#[derive(Clone, Debug)]
pub struct DecoratorPipelineStage {
    pub decorator_name: String,
    pub phase: DecoratorPhase,
    /// Arguments if decorator was called with parameters (e.g., @retry(3, 100))
    pub args: Vec<Expr>,
}

/// Compiled decorator pipeline for a function
#[derive(Clone, Debug)]
pub struct DecoratorPipeline {
    pub fn_id: String,
    pub stages: Vec<DecoratorPipelineStage>,
    /// Hash for caching compiled versions
    pub pipeline_hash: u64,
}

#[derive(Clone, Debug)]
pub struct Function {
    pub name: String,
    pub type_params: Vec<String>,
    pub params: Vec<(String, Option<Expr>, Option<String>)>,
    pub body: std::sync::Arc<Vec<Stmt>>,
    pub visibility: Option<Visibility>,
    pub ret_type: Option<String>,
    pub is_async: bool,
    pub is_static: bool,
    pub is_abstract: bool,
    pub is_constructor: bool,
    pub is_getter: bool,
    pub is_setter: bool,
    pub is_operator: bool,
    pub operator_symbol: Option<String>,
    pub decorators: Vec<Expr>,
    // Test metadata
    pub is_test: bool,
    pub test_ignore: bool,
    pub test_expect_fail: bool,
    pub test_timeout: Option<u64>,
    pub is_unsafe: bool,
}
impl Function {
    #[allow(dead_code)]
    pub fn new(
        name: String,
        type_params: Vec<String>,
        params: Vec<(String, Option<Expr>, Option<String>)>,
        body: std::sync::Arc<Vec<Stmt>>,
        visibility: Option<Visibility>,
        ret_type: Option<String>,
        is_async: bool,
    ) -> Self {
        Function {
            name,
            type_params,
            params,
            body,
            visibility,
            ret_type,
            is_async,
            is_static: false,
            is_abstract: false,
            is_constructor: false,
            is_getter: false,
            is_setter: false,
            is_operator: false,
            operator_symbol: None,
            decorators: Vec::new(),
            is_test: false,
            test_ignore: false,
            test_expect_fail: false,
            test_timeout: None,
            is_unsafe: false,
        }
    }

    pub fn references_this(&self) -> bool {
        fn check_expr(expr: &Expr) -> bool {
            match &expr.kind {
                ExprKind::Variable(name) => name == "this" || name == "self",
                ExprKind::Assign(name, body) => {
                    name == "this" || name == "self" || check_expr(body)
                }
                ExprKind::AssignOp(left, _, right) => check_expr(left) || check_expr(right),
                ExprKind::Unary(_, body) => check_expr(body),
                ExprKind::Binary(left, _, right) => check_expr(left) || check_expr(right),
                ExprKind::Logical(left, _, right) => check_expr(left) || check_expr(right),
                ExprKind::Grouping(inner) => check_expr(inner),
                ExprKind::Call(callee, args, _) => {
                    check_expr(callee) || args.iter().any(check_expr)
                }
                ExprKind::Array(elements) => elements.iter().any(check_expr),
                ExprKind::Tuple(elements) => elements.iter().any(check_expr),
                ExprKind::Object(props) => props.iter().any(|(_, v)| check_expr(v)),
                ExprKind::StructLiteral(_, fields) => fields.iter().any(|(_, v)| check_expr(v)),
                ExprKind::SetLiteral(elements) => elements.iter().any(check_expr),
                ExprKind::Get(obj, _) => check_expr(obj),
                ExprKind::Set(obj, _, val) => check_expr(obj) || check_expr(val),
                ExprKind::New(callee, args) => check_expr(callee) || args.iter().any(check_expr),
                ExprKind::Await(body) => check_expr(body),
                ExprKind::Spawn(body) => check_expr(body),
                ExprKind::Throw(body) => check_expr(body),
                ExprKind::Spread(body) => check_expr(body),
                ExprKind::Range(start, end, _) => check_expr(start) || check_expr(end),
                ExprKind::Index(obj, idx) => check_expr(obj) || check_expr(idx),
                ExprKind::NonNull(body) => check_expr(body),
                ExprKind::OptGet(obj, _) => check_expr(obj),
                ExprKind::OptCall(callee, args, _) => {
                    check_expr(callee) || args.iter().any(check_expr)
                }
                ExprKind::Try(body) => check_expr(body),
                ExprKind::Conditional(cond, then, els) => {
                    check_expr(cond) || check_expr(then) || check_expr(els)
                }
                ExprKind::Update(_, _, body) => check_expr(body),
                ExprKind::Match(val, arms) => {
                    check_expr(val) || arms.iter().any(|(_, body)| check_expr(body))
                }
                ExprKind::Format(body, _) => check_expr(body),
                ExprKind::Cast(body, _) => check_expr(body),
                _ => false,
            }
        }

        fn check_stmt(stmt: &Stmt) -> bool {
            match &stmt.kind {
                StmtKind::Let(_, init, _, _, _, _) => init.as_ref().map_or(false, check_expr),
                StmtKind::LetTuple(_, _, init, _, _, _) => init.as_ref().map_or(false, check_expr),
                StmtKind::LetObject(_, init, _, _, _) => init.as_ref().map_or(false, check_expr),
                StmtKind::ExprStmt(expr) => check_expr(expr),
                StmtKind::Return(expr) => expr.as_ref().map_or(false, check_expr),
                StmtKind::If {
                    cond,
                    then_branch,
                    else_branch,
                } => {
                    check_expr(cond)
                        || check_stmt(then_branch)
                        || else_branch.as_ref().map_or(false, |b| check_stmt(b))
                }
                StmtKind::While { cond, body } => check_expr(cond) || check_stmt(body),
                StmtKind::ForIn { iter, body, .. } => check_expr(iter) || check_stmt(body),
                StmtKind::Block(stmts) => stmts.iter().any(check_stmt),
                StmtKind::TryCatch {
                    try_block,
                    catch_block,
                    ..
                } => check_stmt(try_block) || check_stmt(catch_block),
                StmtKind::Region { body, .. } => check_stmt(body),
                StmtKind::Defer(body) => check_stmt(body),
                StmtKind::UnsafeBlock(body) => check_stmt(body),
                _ => false,
            }
        }

        self.body.iter().any(check_stmt)
    }
}

#[derive(Clone, Debug)]
pub struct ClassDecl {
    pub name: String,
    pub type_params: Vec<String>,
    pub extends: Option<String>,
    pub implements: Vec<String>,
    pub methods: Vec<Function>,
    pub static_methods: Vec<Function>,
    pub static_properties: Vec<(String, Expr, Vec<Expr>)>,
    pub fields: Vec<(String, String, Visibility, Vec<Expr>, Option<Expr>)>,
    pub is_abstract: bool,
    pub is_sealed: bool,
    pub decorators: Vec<Expr>,
}

/// FFI: extern function declaration
#[derive(Clone, Debug)]
pub struct ExternFunctionDecl {
    /// Function name (symbol to resolve)
    pub name: String,
    /// Parameter names and types
    pub params: Vec<(String, String)>,
    /// Return type
    pub ret_type: String,
    /// ABI type (C, Rust, Go, SysV, Windows, Wasm, Adesh)
    pub abi: String,
}

#[derive(Clone, Debug)]
pub struct InterfaceDecl {
    pub name: String,
    pub type_params: Vec<String>,
    pub super_interfaces: Vec<String>,
    pub methods: Vec<Function>,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct StructDecl {
    pub name: String,
    pub type_params: Vec<String>,
    pub fields: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
pub struct TypeAliasDecl {
    pub name: String,
    pub type_params: Vec<String>,
    pub fields: Vec<(String, bool, String)>,
    pub defaults: Vec<(String, Expr)>,
}

#[derive(Clone, Debug)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<(String, Option<String>)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OwnershipKind {
    Unique,
    Shared,
    Strong,
    Weak,
}

#[derive(Clone, Debug)]
pub struct ShareDecl {
    pub name: String,
    pub expr: Expr,
    pub type_ann: Option<String>,
}

#[derive(Clone, Debug)]
pub struct StrongDecl {
    pub name: String,
    pub expr: Expr,
    pub type_ann: Option<String>,
}

#[derive(Clone, Debug)]
pub struct WeakDecl {
    pub name: String,
    pub expr: Expr,
    pub type_ann: Option<String>,
}

#[derive(Clone)]
pub enum Value {
    Number(f64),       // f64 is used for all numeric types
    BigInt(BigInt),    // BigInt is used for large integers
    Bool(bool),        // bool is used for boolean values
    Char(char),        // char is used for single characters
    Str(String),       // String is used for strings
    Null,              // Null is used for null values
    Array(Vec<Value>), // Array is used for arrays
    /// Raw array without metadata overhead (like C arrays) - stores element type name and elements
    /// The String is the element type (e.g., "u8", "i32", etc.)
    RawArray(String, Vec<Value>), // RawArray is used for raw arrays
    /// Dynamic array with metadata tracking (length, capacity, element type)
    DynArray(Box<DynamicArray>), // DynArray is used for dynamic arrays
    Tuple(Vec<Value>), // Tuple is used for tuples
    Object(std::sync::Arc<HashMap<String, Value>>), // Object is used for objects
    Set(Vec<Value>),   // Set is used for sets
    Complex(f64, f64), // Complex is used for complex numbers
    // Fixed-width integer types (unsigned)
    U8(u8),     // U8 is used for unsigned 8-bit integers
    U16(u16),   // U16 is used for unsigned 16-bit integers
    U32(u32),   // U32 is used for unsigned 32-bit integers
    U64(u64),   // U64 is used for unsigned 64-bit integers
    U128(u128), // U128 is used for unsigned 128-bit integers
    // Fixed-width integer types (signed)
    I8(i8),     // I8 is used for signed 8-bit integers
    I16(i16),   // I16 is used for signed 16-bit integers
    I32(i32),   // I32 is used for signed 32-bit integers
    I64(i64),   // I64 is used for signed 64-bit integers
    I128(i128), // I128 is used for signed 128-bit integers
    // Fixed-width float types
    F32(f32),                 // F32 is used for single-precision floating-point numbers
    F64(f64),                 // F64 is used for double-precision floating-point numbers
    Function(NativeFn),       // Function is used for native functions
    UserFunction(UserFn),     // UserFunction is used for user functions
    Struct(UserStruct),       // Struct is used for user structs
    Enum(UserEnum),           // Enum is used for user enums
    Interface(UserInterface), // Interface is used for user interfaces
    EnumCtor(Box<UserEnum>, String), // EnumCtor is used for enum constructors
    /// Promise represented by an internal id handled by the Interpreter
    Promise(u64),
    /// A native bound method on a value, e.g. array.append captured as a method
    BoundNative(String, Box<Value>),
    Class(UserClass),
    /// A 'super' helper carrying the parent class and the instance to bind against
    Super(Box<UserClass>, Box<UserInstance>),
    Instance(UserInstance),
    BoundMethod(UserFn, Box<UserInstance>),
    /// Structured runtime error value
    Error(crate::parsing::error::LangError),
    /// Shared borrow handle (runtime-only, not produced by parser literals)
    Ref(Box<Value>, crate::types::value_optimized::BorrowHandle),
    /// Strong reference counting pointer
    Share(StrongRef),
    /// Weak reference counting pointer
    Weak(WeakRef),
    /// Lazy range iterator (start, end, step) — avoids allocating a full Vec
    LazyRange(f64, f64, f64),
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Use depth-limited formatting to prevent stack overflow on deeply nested structures
        // Default max depth: 64 levels should be safe and sufficient for most use cases
        self.fmt_with_depth(f, 64)
    }
}

impl Value {
    /// Format with a maximum nesting depth to prevent stack overflow
    pub fn fmt_with_depth(&self, f: &mut fmt::Formatter<'_>, max_depth: usize) -> fmt::Result {
        self.fmt_depth_impl(f, max_depth, 0)
    }

    fn fmt_depth_impl(
        &self,
        f: &mut fmt::Formatter<'_>,
        max_depth: usize,
        current_depth: usize,
    ) -> fmt::Result {
        use Value::*;

        // If we've reached max depth, abbreviate
        if current_depth >= max_depth {
            return write!(f, "<max_depth>");
        }

        match self {
            Number(n) => write!(f, "{}", n),
            BigInt(bi) => write!(f, "{}", bi),
            Bool(b) => write!(f, "{}", b),
            Char(c) => write!(f, "'{}'", c),
            Str(s) => write!(f, "\"{}\"", s),
            Null => write!(f, "null"),
            // Fixed-width integer types (unsigned) - no suffix when printing
            U8(n) => write!(f, "{}", n),
            U16(n) => write!(f, "{}", n),
            U32(n) => write!(f, "{}", n),
            U64(n) => write!(f, "{}", n),
            U128(n) => write!(f, "{}", n),
            // Fixed-width integer types (signed) - no suffix when printing
            I8(n) => write!(f, "{}", n),
            I16(n) => write!(f, "{}", n),
            I32(n) => write!(f, "{}", n),
            I64(n) => write!(f, "{}", n),
            I128(n) => write!(f, "{}", n),
            // Fixed-width float types - no suffix when printing
            F32(n) => write!(f, "{}", n),
            F64(n) => write!(f, "{}", n),
            Array(a) => {
                write!(f, "[")?;
                for (i, v) in a.iter().enumerate() {
                    if i != 0 {
                        write!(f, ", ")?;
                    }
                    v.fmt_depth_impl(f, max_depth, current_depth + 1)?;
                }
                write!(f, "]")
            }
            RawArray(_elem_type, a) => {
                write!(f, "[")?;
                for (i, v) in a.iter().enumerate() {
                    if i != 0 {
                        write!(f, ", ")?;
                    }
                    v.fmt_depth_impl(f, max_depth, current_depth + 1)?;
                }
                write!(f, "]")
            }
            DynArray(da) => {
                write!(f, "[")?;
                for (i, v) in da.data.iter().enumerate() {
                    if i != 0 {
                        write!(f, ", ")?;
                    }
                    v.fmt_depth_impl(f, max_depth, current_depth + 1)?;
                }
                write!(f, "]")
            }
            Tuple(t) => {
                if t.is_empty() {
                    write!(f, "()")
                } else if t.len() == 1 {
                    write!(f, "(")?;
                    t[0].fmt_depth_impl(f, max_depth, current_depth + 1)?;
                    write!(f, ",)")
                } else {
                    write!(f, "(")?;
                    for (i, v) in t.iter().enumerate() {
                        if i != 0 {
                            write!(f, ", ")?;
                        }
                        v.fmt_depth_impl(f, max_depth, current_depth + 1)?;
                    }
                    write!(f, ")")
                }
            }
            Object(map) => {
                let is_method_instance = !map.is_empty()
                    && map
                        .values()
                        .all(|v| matches!(v, Function(_) | UserFunction(_) | BoundNative(_, _)));
                if is_method_instance {
                    write!(f, "<Collection Instance>")
                } else {
                    write!(f, "{{")?;
                    let mut first = true;
                    for (k, v) in map.iter() {
                        if !first {
                            write!(f, ", ")?;
                        }
                        first = false;
                        write!(f, "{}: ", k)?;
                        v.fmt_depth_impl(f, max_depth, current_depth + 1)?;
                    }
                    write!(f, "}}")
                }
            }
            Set(s) => {
                write!(f, "{{")?;
                for (i, v) in s.iter().enumerate() {
                    if i != 0 {
                        write!(f, ", ")?;
                    }
                    v.fmt_depth_impl(f, max_depth, current_depth + 1)?;
                }
                write!(f, "}}")
            }
            Complex(r, i) => write!(f, "{}+{}j", r, i),
            Function(_) => write!(f, "<native fn>"),
            BoundNative(name, _v) => write!(f, "<bound native {}>", name),
            UserFunction(_) => write!(f, "<fn>"),
            Class(c) => write!(f, "<class {}>", c.name),
            Instance(i) => write!(f, "<{} instance>", i.class_name),
            BoundMethod(_, i) => write!(f, "<bound method on {}>", i.class_name),
            Super(_, _) => write!(f, "<super>"),
            Struct(s) => write!(f, "<struct {}>", s.name),
            Enum(e) => write!(f, "<enum {}>", e.name),
            Interface(i) => write!(f, "<interface {}>", i.name),
            Ref(v, _) => v.fmt_depth_impl(f, max_depth, current_depth),
            EnumCtor(e, v) => write!(f, "<enum ctor {}::{}>", e.name, v),
            Error(le) => write!(f, "<error {}>", le.message),
            Promise(id) => write!(f, "<promise {}>", id),
            Share(sr) => {
                let strong = sr.strong_count();
                let weak = sr.weak_count();
                let val = unsafe { &(*sr.ptr).value };
                write!(
                    f,
                    "<strong ref count={}, weak_count={}, value={:?}>",
                    strong, weak, val
                )
            }
            Weak(wr) => {
                let strong = wr.strong_count();
                let weak = wr.weak_count();
                write!(f, "<weak ref count={}, weak_count={}>", strong, weak)
            }
            LazyRange(s, e, step) => write!(f, "range({}, {}, {})", s, e, step),
        }
    }
}

impl Value {
    pub fn truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Null => false,
            Value::Number(n) => *n != 0.0,
            Value::BigInt(bi) => *bi != BigInt::from(0),
            Value::Char(_c) => true,
            Value::Str(s) => !s.is_empty(),
            Value::Array(a) => !a.is_empty(),
            Value::RawArray(_, a) => !a.is_empty(),
            Value::DynArray(da) => !da.is_empty(),
            Value::Tuple(t) => !t.is_empty(),
            Value::Set(s) => !s.is_empty(),
            // Fixed-width integer types
            Value::U8(n) => *n != 0,
            Value::U16(n) => *n != 0,
            Value::U32(n) => *n != 0,
            Value::U64(n) => *n != 0,
            Value::U128(n) => *n != 0,
            Value::I8(n) => *n != 0,
            Value::I16(n) => *n != 0,
            Value::I32(n) => *n != 0,
            Value::I64(n) => *n != 0,
            Value::I128(n) => *n != 0,
            // Fixed-width float types
            Value::F32(n) => *n != 0.0,
            Value::F64(n) => *n != 0.0,
            _ => true,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            Value::F32(n) => Some(*n as f64),
            Value::F64(n) => Some(*n),
            Value::U8(n) => Some(*n as f64),
            Value::U16(n) => Some(*n as f64),
            Value::U32(n) => Some(*n as f64),
            Value::U64(n) => Some(*n as f64),
            Value::U128(n) => Some(*n as f64),
            Value::I8(n) => Some(*n as f64),
            Value::I16(n) => Some(*n as f64),
            Value::I32(n) => Some(*n as f64),
            Value::I64(n) => Some(*n as f64),
            Value::I128(n) => Some(*n as f64),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct NativeFn(
    pub std::sync::Arc<dyn Fn(&mut dyn BuiltinEnv, Vec<Value>) -> Result<Value, String> + 'static>,
);

thread_local! {
    static CLONE_DEPTH: std::cell::Cell<usize> = std::cell::Cell::new(0);
}

#[allow(dead_code)]
pub struct UserFn {
    pub name: String,
    pub type_params: Vec<String>,
    pub params: Vec<(String, Option<Expr>, Option<String>)>,
    pub body: std::sync::Arc<Vec<Stmt>>,
    #[allow(dead_code)]
    pub closure: usize, // env id captured at definition time
    pub captured: Option<HashMap<String, Value>>, // captured env values for closures
    #[allow(dead_code)]
    pub visibility: Option<Visibility>,
    #[allow(dead_code)]
    pub ret_type: Option<String>,
    #[allow(dead_code)]
    pub is_async: bool,
    pub is_static: bool,
    pub is_abstract: bool,
    pub is_constructor: bool,
    pub is_getter: bool,
    pub is_setter: bool,
    pub is_operator: bool,
    pub operator_symbol: Option<String>,
    pub defining_class: Option<String>,
    pub is_unsafe: bool,
}

impl Clone for UserFn {
    fn clone(&self) -> Self {
        let depth = CLONE_DEPTH.with(|d| {
            let val = d.get() + 1;
            d.set(val);
            val
        });
        if depth > 100 {
            panic!(
                "UserFn clone depth exceeded 100! Potential cycle detected in function '{}'",
                self.name
            );
        }
        let res = UserFn {
            name: self.name.clone(),
            type_params: self.type_params.clone(),
            params: self.params.clone(),
            body: self.body.clone(),
            closure: self.closure,
            captured: self.captured.clone(),
            visibility: self.visibility.clone(),
            ret_type: self.ret_type.clone(),
            is_async: self.is_async,
            is_static: self.is_static,
            is_abstract: self.is_abstract,
            is_constructor: self.is_constructor,
            is_getter: self.is_getter,
            is_setter: self.is_setter,
            is_operator: self.is_operator,
            operator_symbol: self.operator_symbol.clone(),
            defining_class: self.defining_class.clone(),
            is_unsafe: self.is_unsafe,
        };
        CLONE_DEPTH.with(|d| d.set(d.get() - 1));
        res
    }
}

#[allow(dead_code)]
impl UserFn {
    pub fn with_visibility(mut self, v: Option<Visibility>) -> Self {
        self.visibility = v;
        self
    }

    #[allow(dead_code)]
    pub fn param_count(&self) -> usize {
        self.params.len()
    }

    pub fn matches_signature(&self, arg_count: usize) -> bool {
        // Check if this function can handle the given number of arguments
        let required_params = self
            .params
            .iter()
            .filter(|(_, default, _)| default.is_none())
            .count();
        arg_count >= required_params && arg_count <= self.params.len()
    }

    /// Calculate type distance for overload resolution
    /// Returns None if types are incompatible, otherwise returns distance score (lower is better)
    pub fn type_distance(param_type: &Option<String>, arg_type: &str) -> Option<usize> {
        match param_type {
            None => Some(1000), // No type annotation = accepts any type (lowest priority)
            Some(ptype) => {
                if ptype == arg_type {
                    Some(0) // Exact match
                } else if Self::is_subtype(arg_type, ptype) {
                    // Subtype match (e.g., Child -> Parent)
                    Some(Self::inheritance_distance(arg_type, ptype))
                } else if Self::is_convertible(arg_type, ptype) {
                    Some(100) // Implicit conversion (e.g., int -> float)
                } else {
                    None // Incompatible types
                }
            }
        }
    }

    /// Check if arg_type is a subtype of param_type
    fn is_subtype(_arg_type: &str, _param_type: &str) -> bool {
        // TODO: Implement proper inheritance checking
        // For now, return false - will be enhanced when we have type hierarchy info
        false
    }

    /// Calculate inheritance distance between types
    fn inheritance_distance(_arg_type: &str, _param_type: &str) -> usize {
        // TODO: Calculate actual depth in inheritance hierarchy
        // For now, return 1 for any subtype relationship
        1
    }

    /// Check if types are convertible (implicit conversion)
    fn is_convertible(from: &str, to: &str) -> bool {
        if from == to {
            return true;
        }
        matches!(
            (from, to),
            ("i8", "i16")
                | ("i8", "i32")
                | ("i8", "i64")
                | ("i8", "i128")
                | ("i16", "i32")
                | ("i16", "i64")
                | ("i16", "i128")
                | ("i32", "i64")
                | ("i32", "i128")
                | ("i64", "i128")
                | ("u8", "u16")
                | ("u8", "u32")
                | ("u8", "u64")
                | ("u8", "u128")
                | ("u16", "u32")
                | ("u16", "u64")
                | ("u16", "u128")
                | ("u32", "u64")
                | ("u32", "u128")
                | ("u64", "u128")
                | ("f32", "f64")
                | ("i8", "f64")
                | ("i16", "f64")
                | ("i32", "f64")
                | ("i64", "f64")
                | ("u8", "f64")
                | ("u16", "f64")
                | ("u32", "f64")
                | ("u64", "f64")
                | ("f64", "number")
                | ("number", "f64")
                | ("string", "str")
                | ("str", "string")
        )
    }

    /// Match signature with type information
    /// Returns total type distance if match is possible, None if incompatible
    pub fn matches_signature_with_types(&self, args: &[Value]) -> Option<usize> {
        // First check arity
        if !self.matches_signature(args.len()) {
            return None;
        }

        let mut total_distance = 0;

        for (i, arg) in args.iter().enumerate() {
            if i >= self.params.len() {
                break; // Extra args beyond params
            }

            let (_param_name, _default, param_type) = &self.params[i];

            if let Some(ptype) = param_type {
                if let Value::Instance(inst) = arg {
                    match Self::instance_type_distance(ptype, &inst.class) {
                        Some(dist) => {
                            total_distance += dist;
                            continue;
                        }
                        None => return None,
                    }
                }

                let arg_type = Self::value_type_name(arg);
                if ptype == &arg_type {
                    // Exact match
                    total_distance += 0;
                } else if Self::is_convertible(&arg_type, ptype) {
                    total_distance += 100;
                } else {
                    return None; // Incompatible type
                }
            } else {
                // No type annotation = accepts any type (lowest priority)
                total_distance += 1000;
            }
        }

        Some(total_distance)
    }

    /// Calculate type distance for instance types with inheritance traversal
    fn instance_type_distance(param_type: &str, class: &UserClass) -> Option<usize> {
        if class.name == param_type {
            return Some(0); // Exact match
        }

        if class.implements.iter().any(|i| i == param_type) {
            return Some(1); // Implements interface directly
        }

        // Walk inheritance chain
        let mut curr = class;
        let mut depth = 0;
        while let Some(parent) = &curr.parent {
            depth += 1;
            if parent.name == param_type {
                return Some(depth);
            }
            if parent.implements.iter().any(|i| i == param_type) {
                return Some(depth + 1);
            }
            curr = parent;
        }

        None
    }

    /// Get type name for a Value (simple version for error messages)
    pub fn value_type_name(value: &Value) -> String {
        match value {
            Value::Number(_) => "f64".to_string(),
            Value::U8(_) => "u8".to_string(),
            Value::U16(_) => "u16".to_string(),
            Value::U32(_) => "u32".to_string(),
            Value::U64(_) => "u64".to_string(),
            Value::U128(_) => "u128".to_string(),
            Value::I8(_) => "i8".to_string(),
            Value::I16(_) => "i16".to_string(),
            Value::I32(_) => "i32".to_string(),
            Value::I64(_) => "i64".to_string(),
            Value::I128(_) => "i128".to_string(),
            Value::F32(_) => "f32".to_string(),
            Value::F64(_) => "f64".to_string(),
            Value::Str(_) => "string".to_string(),
            Value::Char(_) => "char".to_string(),
            Value::Bool(_) => "bool".to_string(),
            Value::BigInt(_) => "bigint".to_string(),
            Value::Array(_) => "[]".to_string(),
            Value::DynArray(da) => format!("[{}]", da.concrete_type),
            Value::Object(_) => "object".to_string(),
            Value::Struct(s) => s.name.clone(),
            Value::Enum(e) => e.name.clone(),
            Value::Interface(i) => i.name.clone(),
            Value::Instance(i) => i.class.name.clone(),
            Value::EnumCtor(ename, _) => ename.name.clone(),
            Value::Tuple(items) => {
                let elem_types: Vec<String> = items.iter().map(Self::value_type_name).collect();
                format!("({})", elem_types.join(", "))
            }
            Value::Share(share) => Self::value_type_name(unsafe { &(*share.ptr).value }),
            Value::Ref(inner, _) => Self::value_type_name(inner),
            Value::Null => "null".to_string(),
            _ => "unknown".to_string(),
        }
    }
}

pub struct UserClass {
    pub name: String,
    pub methods: HashMap<String, Vec<UserFn>>, // Support method overloading
    #[allow(dead_code)]
    pub static_methods: HashMap<String, Vec<UserFn>>,
    #[allow(dead_code)]
    pub static_properties: HashMap<String, Value>,
    #[allow(dead_code)]
    pub operators: HashMap<String, UserFn>, // Operator overloading
    #[allow(dead_code)]
    pub getters: HashMap<String, UserFn>,
    #[allow(dead_code)]
    pub setters: HashMap<String, UserFn>,
    pub parent: Option<Box<UserClass>>,
    #[allow(dead_code)]
    pub implements: Vec<String>,
    pub is_abstract: bool,
    pub is_sealed: bool,
    /// Field visibility metadata: field_name -> Visibility
    /// If not present, field is public (default)
    pub field_visibility: HashMap<String, Visibility>,
    /// Field owner metadata: field_name -> defining class name
    pub field_owner: HashMap<String, String>,
    /// Field type metadata: field_name -> Type Name
    pub field_types: HashMap<String, String>,
    /// Field initializer expressions: field_name -> Expr
    pub field_initializers: HashMap<String, Expr>,
}

impl Clone for UserClass {
    fn clone(&self) -> Self {
        let depth = CLONE_DEPTH.with(|d| {
            let val = d.get() + 1;
            d.set(val);
            val
        });
        if depth > 100 {
            panic!(
                "UserClass clone depth exceeded 100! Potential cycle detected in class '{}'",
                self.name
            );
        }
        let res = UserClass {
            name: self.name.clone(),
            methods: self.methods.clone(),
            static_methods: self.static_methods.clone(),
            static_properties: self.static_properties.clone(),
            operators: self.operators.clone(),
            getters: self.getters.clone(),
            setters: self.setters.clone(),
            parent: self.parent.clone(),
            implements: self.implements.clone(),
            is_abstract: self.is_abstract,
            is_sealed: self.is_sealed,
            field_visibility: self.field_visibility.clone(),
            field_owner: self.field_owner.clone(),
            field_types: self.field_types.clone(),
            field_initializers: self.field_initializers.clone(),
        };
        CLONE_DEPTH.with(|d| d.set(d.get() - 1));
        res
    }
}

#[derive(Clone)]
pub struct UserStruct {
    pub name: String,
    pub fields: Vec<(String, String)>,
    pub methods: HashMap<String, Vec<UserFn>>,
    pub fields_map: HashMap<String, String>,
}

#[derive(Clone)]
pub struct UserEnum {
    pub name: String,
    pub variants: Vec<(String, Option<String>)>,
    pub methods: HashMap<String, Vec<UserFn>>,
    pub variants_map: HashMap<String, Option<String>>,
}

#[derive(Clone)]
pub struct UserInterface {
    pub name: String,
    pub methods: Vec<Function>,
}

#[derive(Clone)]
pub struct UserInstance {
    pub class_name: String,
    pub fields: Arc<RwLock<HashMap<String, Value>>>,
    pub class: Arc<UserClass>,
    pub prop_cache: Arc<RwLock<HashMap<String, Value>>>,
    /// Optional optimized field storage using computed layout
    /// When present, field reads/writes go through packed storage first,
    /// falling back to `fields` for unsupported/dynamic entries.
    pub layout: Option<Arc<crate::types::field_layout::FieldLayout>>,
    pub raw: Option<Arc<RwLock<Vec<u8>>>>,
}

// ============================================
// ARC OWNERSHIP RUNTIME STRUCTURES
// ============================================
// Canonical implementation lives in `memory::arc::shared_object`.
pub use crate::memory::arc::shared_object::{
    SharedObject, StrongRef, WeakRef, allocate_share, create_weak_from_strong, invoke_arc_method,
};

impl UserInstance {
    /// Initialize optimized packed storage with a given layout.
    /// Allocates a zeroed byte buffer sized to the layout's final size.
    pub fn init_layout(&mut self, layout: Arc<crate::types::field_layout::FieldLayout>) {
        let size = layout.get_final_size();
        self.raw = Some(Arc::new(RwLock::new(vec![0u8; size])));
        self.layout = Some(layout);
    }

    /// Get field value by name using optimized storage when available.
    /// Falls back to `fields` HashMap if layout/raw are missing or type unsupported.
    pub fn get_field(&self, name: &str) -> Option<Value> {
        if let (Some(layout), Some(raw)) = (&self.layout, &self.raw) {
            if let Some(fi) = layout.find_field(name) {
                if let Ok(buf) = raw.read() {
                    return Self::read_packed(&buf, fi).or_else(|| {
                        // Unsupported packed type → fallback
                        self.fields.read().ok()?.get(name).cloned()
                    });
                }
            }
        }
        // Fallback: regular map
        self.fields.read().ok()?.get(name).cloned()
    }

    /// Set field value by name using optimized storage when available.
    /// Falls back to `fields` HashMap if layout/raw are missing or type unsupported.
    pub fn set_field(&self, name: &str, v: Value) -> Result<(), String> {
        if let (Some(layout), Some(raw)) = (&self.layout, &self.raw) {
            if let Some(fi) = layout.find_field(name) {
                if let Ok(mut buf) = raw.write() {
                    if Self::write_packed(&mut buf, fi, &v) {
                        return Ok(());
                    }
                }
            }
        }
        // Fallback path: regular map update
        if let Ok(mut g) = self.fields.write() {
            g.insert(name.to_string(), v);
            Ok(())
        } else {
            Err("instance fields lock poisoned".to_string())
        }
    }

    #[inline]
    fn read_packed(buf: &[u8], fi: &crate::types::type_info::FieldInfo) -> Option<Value> {
        let off = fi.offset;
        let ty = fi.type_name.as_str();
        // Safety: bounds checked by slicing
        match ty {
            // Unsigned ints
            "u8" => buf.get(off).map(|b| Value::U8(*b)),
            "u16" => buf
                .get(off..off + 2)
                .map(|s| Value::U16(u16::from_le_bytes([s[0], s[1]]))),
            "u32" => buf
                .get(off..off + 4)
                .map(|s| Value::U32(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))),
            "u64" => buf.get(off..off + 8).map(|s| {
                let mut a = [0u8; 8];
                a.copy_from_slice(s);
                Value::U64(u64::from_le_bytes(a))
            }),
            "u128" => buf.get(off..off + 16).map(|s| {
                let mut a = [0u8; 16];
                a.copy_from_slice(s);
                Value::U128(u128::from_le_bytes(a))
            }),
            // Signed ints
            "i8" => buf.get(off).map(|b| Value::I8(*b as i8)),
            "i16" => buf
                .get(off..off + 2)
                .map(|s| Value::I16(i16::from_le_bytes([s[0], s[1]]))),
            "i32" => buf
                .get(off..off + 4)
                .map(|s| Value::I32(i32::from_le_bytes([s[0], s[1], s[2], s[3]]))),
            "i64" => buf.get(off..off + 8).map(|s| {
                let mut a = [0u8; 8];
                a.copy_from_slice(s);
                Value::I64(i64::from_le_bytes(a))
            }),
            "i128" => buf.get(off..off + 16).map(|s| {
                let mut a = [0u8; 16];
                a.copy_from_slice(s);
                Value::I128(i128::from_le_bytes(a))
            }),
            // Floats & Number
            "f32" => buf
                .get(off..off + 4)
                .map(|s| Value::F32(f32::from_le_bytes([s[0], s[1], s[2], s[3]]))),
            "f64" => buf.get(off..off + 8).map(|s| {
                let mut a = [0u8; 8];
                a.copy_from_slice(s);
                Value::F64(f64::from_le_bytes(a))
            }),
            "number" => buf.get(off..off + 8).map(|s| {
                let mut a = [0u8; 8];
                a.copy_from_slice(s);
                Value::Number(f64::from_le_bytes(a))
            }),
            // Bool
            "bool" => buf.get(off).map(|b| Value::Bool(*b != 0)),
            // Char (stored as u32)
            "char" => buf.get(off..off + 4).and_then(|s| {
                let c = u32::from_le_bytes([s[0], s[1], s[2], s[3]]);
                char::from_u32(c).map(Value::Char)
            }),
            // Strings/objects not yet packed
            _ => None,
        }
    }

    #[inline]
    fn write_packed(buf: &mut [u8], fi: &crate::types::type_info::FieldInfo, v: &Value) -> bool {
        let off = fi.offset;
        match (fi.type_name.as_str(), v) {
            ("u8", Value::U8(x)) => {
                buf[off] = *x;
                true
            }
            ("u16", Value::U16(x)) => {
                buf[off..off + 2].copy_from_slice(&x.to_le_bytes());
                true
            }
            ("u32", Value::U32(x)) => {
                buf[off..off + 4].copy_from_slice(&x.to_le_bytes());
                true
            }
            ("u64", Value::U64(x)) => {
                buf[off..off + 8].copy_from_slice(&x.to_le_bytes());
                true
            }
            ("u128", Value::U128(x)) => {
                buf[off..off + 16].copy_from_slice(&x.to_le_bytes());
                true
            }
            ("i8", Value::I8(x)) => {
                buf[off] = *x as u8;
                true
            }
            ("i16", Value::I16(x)) => {
                buf[off..off + 2].copy_from_slice(&x.to_le_bytes());
                true
            }
            ("i32", Value::I32(x)) => {
                buf[off..off + 4].copy_from_slice(&x.to_le_bytes());
                true
            }
            ("i64", Value::I64(x)) => {
                buf[off..off + 8].copy_from_slice(&x.to_le_bytes());
                true
            }
            ("i128", Value::I128(x)) => {
                buf[off..off + 16].copy_from_slice(&x.to_le_bytes());
                true
            }
            ("f32", Value::F32(x)) => {
                buf[off..off + 4].copy_from_slice(&x.to_le_bytes());
                true
            }
            ("f64" | "number", Value::F64(x)) => {
                buf[off..off + 8].copy_from_slice(&x.to_le_bytes());
                true
            }
            // Support Value::Number as f64 (the default numeric type)
            ("f64" | "number", Value::Number(x)) => {
                buf[off..off + 8].copy_from_slice(&x.to_le_bytes());
                true
            }
            ("bool", Value::Bool(b)) => {
                buf[off] = if *b { 1 } else { 0 };
                true
            }
            ("char", Value::Char(c)) => {
                buf[off..off + 4].copy_from_slice(&(*c as u32).to_le_bytes());
                true
            }
            // Packed references: store as 8-byte tagged pointer/handle
            // Tag format (lowest 3 bits): 000=null, 001=direct_ptr, 010=handle, 011=weak_ref
            // Null reference for any reference type
            (_, Value::Null) => {
                if fi.size == 8 {
                    buf[off..off + 8].copy_from_slice(&0u64.to_le_bytes()); // Tag: null
                    true
                } else {
                    false
                }
            }
            // Non-primitive references are not packed directly in the byte buffer to avoid memory leaks/dangling references.
            // They will fall back to the HashMap fields storage.
            (_, Value::Str(_)) | (_, Value::Array(_)) | (_, Value::Object(_)) => false,
            // Fallback - type not supported for packing
            _ => false,
        }
    }

    /// Attempt to pack a single field directly into the packed buffer without touching the map.
    /// Returns true if the field was packed; false if layout/raw missing or type unsupported.
    pub fn try_pack_only(&self, name: &str, v: &Value) -> bool {
        if let (Some(layout), Some(raw)) = (&self.layout, &self.raw) {
            if let Some(fi) = layout.find_field(name) {
                if let Ok(mut buf) = raw.write() {
                    return Self::write_packed(&mut buf, fi, v);
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod instance_packing_tests {
    use super::*;
    use crate::types::field_layout::FieldLayout;
    use crate::types::visibility::Visibility;

    #[test]
    fn packed_set_get_primitives_roundtrip() {
        // Build a simple layout: a:i32 @0, b:f64 @8 (due to alignment)
        let mut fl = FieldLayout::new();
        fl.add_field("a".to_string(), 4, 4, Visibility::Public, "i32".to_string());
        fl.add_field("b".to_string(), 8, 8, Visibility::Public, "f64".to_string());
        fl.finalize();
        let fl = Arc::new(fl);

        let mut inst = UserInstance {
            class_name: "Test".into(),
            fields: Arc::new(RwLock::new(HashMap::default())),
            class: Arc::new(UserClass {
                name: "Test".into(),
                methods: HashMap::default(),
                parent: None,
                implements: vec![],
                is_abstract: false,
                field_visibility: HashMap::default(),
                field_owner: HashMap::default(),
                field_types: HashMap::default(),
                field_initializers: HashMap::default(),
                static_methods: HashMap::default(),
                static_properties: HashMap::default(),
                operators: HashMap::default(),
                getters: HashMap::default(),
                setters: HashMap::default(),
                is_sealed: false,
            }),
            prop_cache: Arc::new(RwLock::new(HashMap::default())),
            layout: None,
            raw: None,
        };

        inst.init_layout(fl.clone());
        inst.set_field("a", Value::I32(42)).unwrap();
        inst.set_field("b", Value::F64(1.5)).unwrap();

        assert!(matches!(inst.get_field("a"), Some(Value::I32(42))));
        match inst.get_field("b") {
            Some(Value::F64(x)) => assert!((x - 1.5).abs() < 1e-12),
            _ => panic!("bad b"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TokenKind {
    // Single char
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    At,
    Comma,
    Dot,
    Semicolon,
    Colon,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    PlusPlus,
    MinusMinus,
    PlusEqual,
    MinusEqual,
    StarEqual,
    SlashEqual,
    PercentEqual,
    StarStar,
    StarStarEqual,

    // two-char
    Bang,
    BangEqual,
    Equal,
    EqualEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    StrictEqual,
    StrictNotEqual,
    ShiftLeft,
    ShiftRight,
    ShiftLeftEqual,
    ShiftRightEqual,
    Ampersand,
    Pipe,
    Caret,
    Tilde,
    TildeSlash,
    AmpersandEqual,
    PipeEqual,
    CaretEqual,
    AndAnd,
    OrOr,

    // spread/rest operator
    DotDotDot, // ...
    DotDot,    // ..

    DocComment,

    Question, // ?
    QuestionDot,
    NullCoalesce,
    NullCoalesceEqual,

    // literals/ident
    Identifier,
    Underscore,
    String,
    Template,
    Number,
    BigIntLit,
    /// Imaginary numeric literal, e.g. `5j`
    ComplexLit,
    CharLit,
    // Typed numeric literals (fixed-width integers and floats)
    U8Lit,
    U16Lit,
    U32Lit,
    U64Lit,
    U128Lit,
    I8Lit,
    I16Lit,
    I32Lit,
    I64Lit,
    I128Lit,
    F32Lit,
    F64Lit,

    // keywords
    Let,
    Const,
    Fn,
    Return,
    If,
    Else,
    Elif,
    Do,
    While,
    For,
    In,
    Of,
    True,
    False,
    Null,
    And,
    Or,
    Instanceof,
    Typeof,
    Not,
    Class,
    New,
    This,
    SelfKeyword,
    Uint,
    Int,
    Extend,
    Extends,
    Extern,
    Private,
    Protected,
    Public,
    Super,
    Abstract,
    Sealed,
    Interface,
    Implements,
    Type,
    Decorator,
    On,
    Import,
    As,
    Export,
    From,
    Default,
    Try,
    Catch,
    Throw,
    Async,
    Await,
    Spawn,
    Static,
    Constructor,
    Get,
    Set,
    Operator,

    Eof,
    Break,
    Continue,
    Jump,
    Arrow,
    Struct,
    Enum,
    Match,
    CImport, // #cImport directive
    // New keywords for arrays, readonly, SIMD
    Readonly,
    Raw,
    VecType, // SIMD vector type keyword

    // Memory management keywords
    Region,
    Unsafe,
    Share,
    Strong,
    Weak,
    Alloc,
    Free,

    // Control flow keywords
    Defer,

    // Decorator phase keywords
    Compile,   // compile { } phase
    Runtime,   // runtime { } phase
    Typecheck, // typecheck { } phase
    Emit,      // emit { } phase
    Require,   // require statement for contracts
    Proceed,   // call.proceed() in runtime phase

    // Testing keywords
    Test,       // test keyword for test functions
    Ignore,     // ignore attribute for tests
    ExpectFail, // expect_fail attribute for tests
}
