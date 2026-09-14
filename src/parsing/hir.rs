//! HIR (High-Level Intermediate Representation)
//!
//! HIR is a typed, structured representation generated from the AST.
//! It maintains language semantics while being easier to analyze and lower to LIR.

use num_bigint::BigInt;
use std::sync::Arc;

/// Unique identifier for HIR nodes
pub type HirId = u64;

/// HIR Binary Operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    IntDiv,
    Mod,
    Pow,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    StrictEq,
    StrictNe,
    And,
    Or,
    NullCoalesce,
    In,
    Instanceof,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
}

/// HIR Unary Operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
    BitNot,
    Typeof,
}

/// Array kind for type annotations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayKind {
    Dynamic,
    Fixed(usize),
    Raw,
    FixedRaw(usize),
}

/// HIR Type representation
#[derive(Debug, Clone, PartialEq)]
pub enum HirType {
    Int,
    Float,
    Bool,
    String,
    Char,
    Null,
    Array(Box<HirType>, ArrayKind),
    Dict(Box<HirType>, Box<HirType>),
    Set(Box<HirType>),
    Tuple(Vec<HirType>),
    Object,
    Class(String),
    Instance(String),
    Promise(Box<HirType>),
    Function(Vec<HirType>, Box<HirType>),
    Any,
    Unknown,
    // Fixed-width integer types
    U8,
    U16,
    U32,
    U64,
    U128,
    I8,
    I16,
    I32,
    I64,
    I128,
    // Fixed-width float types
    F32,
    F64,
    // Memory model types
    /// Borrow reference (&T) - access mode is auto-inferred
    /// The bool flag indicates if exclusive access is needed
    Borrow(Box<HirType>, bool),
    /// Legacy: Immutable borrow reference (&T) - deprecated, use Borrow
    BorrowImmut(Box<HirType>),
    /// Legacy: Mutable borrow reference (&mut T) - deprecated, use Borrow  
    BorrowMut(Box<HirType>),
    /// Shared reference via ARC (Shared<T>)
    Shared(Box<HirType>),
    /// Weak reference to shared value (Weak<T>)
    Weak(Box<HirType>),
    /// SIMD vector type: Simd<T, LANES> or vecN<T>
    Simd(Box<HirType>, u32),
}

/// HIR Literal values
#[derive(Debug, Clone, PartialEq)]
pub enum HirLiteral {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Char(char),
    BigInt(BigInt),
    Null,
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
}

/// HIR Pattern for match expressions
#[derive(Debug, Clone)]
pub enum HirPattern {
    /// Match a specific literal value
    Literal(HirLiteral),
    /// Bind matched value to a variable
    Variable(String),
    /// Match any value (wildcard _)
    Wildcard,
    /// Match either of two patterns
    Or(Box<HirPattern>, Box<HirPattern>),
    /// Match enum variant (Some(x))
    EnumVariant(String, Vec<HirPattern>),
}

/// HIR Expression
#[derive(Debug, Clone)]
pub enum HirExpr {
    /// Literal value
    Literal(HirLiteral),
    /// Variable reference
    LoadVar(String),
    /// Binary operation
    BinaryOp(Box<HirExpr>, BinOp, Box<HirExpr>),
    /// Unary operation
    UnaryOp(UnaryOp, Box<HirExpr>),
    /// Function call with optional type arguments
    Call(Box<HirExpr>, Vec<HirExpr>, Vec<String>),
    /// Method call (object.method(args))
    MethodCall(Box<HirExpr>, String, Vec<HirExpr>),
    /// Array literal
    ArrayLiteral(Vec<HirExpr>),
    /// Dict/Map literal
    DictLiteral(Vec<(HirExpr, HirExpr)>),
    /// Set literal
    SetLiteral(Vec<HirExpr>),
    /// Tuple literal
    TupleLiteral(Vec<HirExpr>),
    /// Object literal
    ObjectLiteral(Vec<(String, HirExpr)>),
    /// Destructuring assignment to tuple/array
    AssignTuple(Vec<String>, Box<HirExpr>),
    /// Destructuring assignment to object
    AssignObject(Vec<(String, Option<String>)>, Box<HirExpr>),
    /// Struct literal
    StructLiteral(String, Vec<(String, HirExpr)>),
    /// Array/Object index access
    Index(Box<HirExpr>, Box<HirExpr>),
    /// Member access (obj.field)
    MemberAccess(Box<HirExpr>, String),
    /// Set member (obj.field = value)
    SetMember(Box<HirExpr>, String, Box<HirExpr>),
    /// Conditional expression (ternary)
    Conditional(Box<HirExpr>, Box<HirExpr>, Box<HirExpr>),
    /// Variable assignment (returns the assigned value)
    StoreVar(String, Box<HirExpr>),
    /// Anonymous function
    Lambda(Vec<(String, Option<HirType>)>, Arc<Vec<HirStmt>>, bool), // last is is_async
    /// Create new instance (new ClassName(args))
    NewInstance(String, Vec<HirExpr>),
    /// Reference to 'this' in class methods
    This,
    /// Reference to 'super' in class methods
    Super,
    /// Await an async expression
    Await(Box<HirExpr>),
    /// Spawn an async task (concurrent execution)
    Spawn(Box<HirExpr>),
    /// Range expression (start..end or start...end)
    Range(Box<HirExpr>, Box<HirExpr>, bool), // bool: inclusive
    /// Spread expression (...expr)
    Spread(Box<HirExpr>),
    /// Optional chaining (expr?.field)
    OptionalGet(Box<HirExpr>, String),
    /// Non-null assertion (expr!)
    NonNull(Box<HirExpr>),
    /// Update expression (++x, x++, --x, x--)
    Update(Box<HirExpr>, bool, bool), // is_increment, is_prefix
    /// Match expression (match expr { pattern => expr, ... })
    Match(Box<HirExpr>, Vec<(HirPattern, HirExpr)>),
    /// Format expression with format specifier (${expr:format_spec})
    Format(Box<HirExpr>, String),
    /// Create borrow reference (&expr)
    /// The bool flag indicates if exclusive access is needed (auto-inferred from usage)
    /// - false = shared borrow (read-only access detected)
    /// - true = exclusive borrow (mutation detected)
    Borrow(Box<HirExpr>, bool),
    /// Legacy: Create immutable borrow reference (&expr) - deprecated, use Borrow
    BorrowImmut(Box<HirExpr>),
    /// Legacy: Create mutable borrow reference (&mut expr) - deprecated, use Borrow
    BorrowMut(Box<HirExpr>),
    /// Dereference a borrow (*expr)
    Deref(Box<HirExpr>),
    /// Create shared (ARC) reference (share expr)
    Share(Box<HirExpr>),
    /// Downgrade to weak reference (expr.downgrade() or weak expr)
    Downgrade(Box<HirExpr>),
    /// Allocate memory in unsafe block (alloc<T>(size))
    Alloc(Box<HirType>, Box<HirExpr>),
    /// Free memory in unsafe block (free(ptr))
    Free(Box<HirExpr>),
    /// Move expression - explicitly moves ownership (move expr)
    Move(Box<HirExpr>),
    /// Cast expression - cast value to target type (expr as type)
    Cast(Box<HirExpr>, HirType),
}

/// HIR Method definition
#[derive(Debug, Clone)]
pub struct HirMethod {
    pub name: String,
    pub params: Vec<(String, Option<HirType>)>,
    pub body: Arc<Vec<HirStmt>>,
    pub ret_type: Option<HirType>,
    pub is_static: bool,
    pub is_async: bool,
    pub is_unsafe: bool,
}

/// HIR Class definition
#[derive(Debug, Clone)]
pub struct HirClass {
    pub name: String,
    pub extends: Option<String>,
    pub methods: Vec<HirMethod>,
    pub static_methods: Vec<HirMethod>,
    pub decorators: Vec<HirExpr>,
}

/// HIR Statement
#[derive(Debug, Clone)]
pub enum HirStmt {
    /// Variable declaration with optional initializer
    Let {
        name: String,
        ty: Option<HirType>,
        init: Option<HirExpr>,
        is_const: bool,
        /// Ownership mode: None = owned, Some(true) = mutable borrow, Some(false) = immutable borrow
        is_borrowed: Option<bool>,
    },
    /// Tuple destructuring declaration
    LetTuple {
        names: Vec<String>,
        init: Option<HirExpr>,
        is_const: bool,
    },
    /// Variable assignment (moves value to target)
    Assign {
        target: HirExpr,
        value: HirExpr,
        /// Whether this is a move operation (true) or copy (false, if allowed)
        is_move: bool,
    },
    /// Expression statement
    Expr(HirExpr),
    /// Return statement
    Return(Option<HirExpr>),
    /// If statement
    If {
        cond: HirExpr,
        then_branch: Box<HirStmt>,
        else_branch: Option<Box<HirStmt>>,
    },
    /// While loop
    While { cond: HirExpr, body: Box<HirStmt> },
    /// For-in loop
    ForIn {
        var: String,
        iter: HirExpr,
        body: Box<HirStmt>,
    },
    /// Block of statements
    Block(Vec<HirStmt>),
    /// Function definition
    FunctionDef {
        name: String,
        params: Vec<(String, Option<HirType>, Option<HirExpr>)>, // name, type, default
        body: Arc<Vec<HirStmt>>,
        ret_type: Option<HirType>,
        is_async: bool,
        decorators: Vec<HirExpr>,
        /// Ownership constraints: which parameters transfer ownership
        move_params: Vec<usize>, // indices of parameters that are moved
        is_unsafe: bool,
    },
    /// Class definition
    ClassDef(HirClass),
    /// Break statement
    Break,
    /// Continue statement
    Continue,
    /// Try-catch statement
    TryCatch {
        try_block: Box<HirStmt>,
        error_name: String,
        catch_block: Box<HirStmt>,
    },
    /// Throw statement
    Throw(HirExpr),
    /// Extend statement - adds methods to an existing class
    Extend {
        target: String,
        methods: Vec<HirFunction>,
    },
    /// Import statement - import entire module as alias
    /// Example: import "path/to/module.adesh" as m;
    Import { path: String, alias: String },
    /// Import default export
    /// Example: import MyClass from "path/to/module.adesh";
    ImportDefault { path: String, alias: String },
    /// Import specific names from module
    /// Example: from "path/to/module.adesh" import foo, bar;
    ImportNames { path: String, names: Vec<String> },
    /// Region block (arena-based allocation with bulk free)
    /// Example: region MyRegion { let a = Obj(); }
    Region { name: String, body: Box<HirStmt> },
    /// Defer block (execute at scope exit in LIFO order)
    /// Example: defer { cleanup(); }
    Defer(Box<HirStmt>),
    /// Unsafe block (opt-out of borrow checking)
    /// Example: unsafe { let p = alloc<u8>(128); }
    Unsafe(Box<HirStmt>),
}

/// HIR Function
#[derive(Debug, Clone)]
pub struct HirFunction {
    pub name: String,
    pub params: Vec<(String, Option<HirType>, Option<HirExpr>)>, // name, type, default
    pub body: Arc<Vec<HirStmt>>,
    pub ret_type: Option<HirType>,
    pub is_async: bool,
    pub decorators: Vec<HirExpr>,
    pub is_exported: bool,
    /// Ownership constraints: which parameters transfer ownership
    pub move_params: Vec<usize>, // indices of parameters that are moved
    // Test metadata
    pub is_test: bool,
    pub test_ignore: bool,
    pub test_expect_fail: bool,
    pub test_timeout: Option<u64>,
    pub is_unsafe: bool,
}

/// HIR Module - the top-level compilation unit
#[derive(Debug, Clone)]
pub struct HirModule {
    pub functions: Vec<HirFunction>,
    pub classes: Vec<HirClass>,
    pub statements: Vec<HirStmt>,
}

impl HirModule {
    pub fn new() -> Self {
        HirModule {
            functions: Vec::new(),
            classes: Vec::new(),
            statements: Vec::new(),
        }
    }
}

impl Default for HirModule {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hir_module_creation() {
        let module = HirModule::new();
        assert!(module.functions.is_empty());
        assert!(module.statements.is_empty());
    }

    #[test]
    fn test_hir_literal_creation() {
        let int_lit = HirLiteral::Int(42);
        let float_lit = HirLiteral::Float(1.234567); // Random value to avoid clippy constant lint
        let bool_lit = HirLiteral::Bool(true);
        let str_lit = HirLiteral::String("hello".to_string());

        match int_lit {
            HirLiteral::Int(n) => assert_eq!(n, 42),
            _ => panic!("Expected Int literal"),
        }

        match float_lit {
            HirLiteral::Float(n) => assert!((n - 1.234567).abs() < 0.001),
            _ => panic!("Expected Float literal"),
        }

        match bool_lit {
            HirLiteral::Bool(b) => assert!(b),
            _ => panic!("Expected Bool literal"),
        }

        match str_lit {
            HirLiteral::String(s) => assert_eq!(s, "hello"),
            _ => panic!("Expected String literal"),
        }
    }
}
