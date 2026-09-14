//! MIR (Memory Intermediate Representation)
//!
//! MIR is responsible for enforcing all memory safety guarantees at compile-time:
//! - Ownership tracking and validation
//! - Borrow checking (mutable and immutable)
//! - Lifetime inference (implicit, no explicit syntax)
//! - Move semantics enforcement
//! - Drop insertion and ordering
//! - ARC semantics insertion where needed
//!
//! After MIR passes, all memory safety is guaranteed.
//! Backends never perform memory safety checks.

pub mod arc_insertion;
pub mod borrow_analysis;
pub mod drop_insertion;
pub mod lifetime_inference;
pub mod lower;
pub mod ownership_graph;
pub mod types;
pub mod validate;

// Re-export for use by lower.rs and other modules
pub use arc_insertion::ArcInsertion;
pub use borrow_analysis::BorrowAnalysis;
pub use drop_insertion::DropInsertion;
pub use lifetime_inference::LifetimeInference;
pub use ownership_graph::OwnershipGraph;
pub use types::*;

/// MIR Module - the result of lowering HIR with full memory safety analysis
#[derive(Debug, Clone)]
pub struct MirModule {
    /// Module name
    pub name: String,

    /// Functions in this module
    pub functions: Vec<MirFunction>,

    /// Global values/constants
    pub globals: Vec<MirGlobal>,

    /// Type definitions
    pub types: Vec<MirTypeDef>,

    /// Ownership graph for the entire module
    pub ownership: OwnershipGraph,
}

/// MIR Function with memory safety annotations
#[derive(Debug, Clone)]
pub struct MirFunction {
    /// Function name
    pub name: String,

    /// Parameters with ownership annotations
    pub params: Vec<MirParam>,

    /// Return type with ownership
    pub return_type: MirType,

    /// Function body (control flow graph)
    pub body: Vec<MirBlock>,

    /// Local variables with ownership info
    pub locals: Vec<MirLocal>,

    /// Captured variables (for closures)
    pub captures: Vec<MirCapture>,

    /// Is this function async?
    pub is_async: bool,

    /// Function-level ownership graph
    pub ownership: OwnershipGraph,
}

/// MIR Basic Block
#[derive(Debug, Clone)]
pub struct MirBlock {
    /// Block identifier
    pub id: BlockId,

    /// Statements in this block
    pub statements: Vec<MirStatement>,

    /// Block terminator (branch, return, etc.)
    pub terminator: MirTerminator,
}

pub type BlockId = u32;
pub type LocalId = u32;
pub type ValueId = u32;

/// MIR Statement - represents operations within a block
#[derive(Debug, Clone)]
pub enum MirStatement {
    /// Assign a value to a local
    Assign(LocalId, MirRvalue),

    /// Explicit drop call (inserted by drop analysis)
    Drop(LocalId),

    /// ARC clone operation (inserted by ARC analysis)
    ArcClone(LocalId, LocalId),

    /// ARC drop operation (inserted by ARC analysis)
    ArcDrop(LocalId),

    /// Function call
    Call {
        dest: LocalId,
        func: MirOperand,
        args: Vec<MirOperand>,
    },

    /// Storage live annotation (lifetime start)
    StorageLive(LocalId),

    /// Storage dead annotation (lifetime end)
    StorageDead(LocalId),

    /// No-op (for alignment, debugging)
    Nop,
}

/// MIR Right-value (computed value)
#[derive(Debug, Clone)]
pub enum MirRvalue {
    /// Use a local or constant
    Use(MirOperand),

    /// Take reference (borrow)
    Ref(BorrowKind, MirPlace),

    /// Dereference
    Deref(MirPlace),

    /// Binary operation
    BinaryOp(MirBinOp, MirOperand, MirOperand),

    /// Unary operation
    UnaryOp(MirUnOp, MirOperand),

    /// Cast between types
    Cast(MirOperand, MirType),

    /// Aggregate (struct, tuple, array)
    Aggregate(AggregateKind, Vec<MirOperand>),
}

/// MIR Operand (value that can be used)
#[derive(Debug, Clone)]
pub enum MirOperand {
    /// Move value (transfer ownership)
    Move(MirPlace),

    /// Copy value (for Copy types)
    Copy(MirPlace),

    /// Constant value
    Constant(MirConstant),
}

/// MIR Place (location in memory)
#[derive(Debug, Clone)]
pub struct MirPlace {
    /// Base local variable
    pub local: LocalId,

    /// Projections (field access, indexing, etc.)
    pub projection: Vec<MirProjection>,
}

/// MIR Projection (access into a place)
#[derive(Debug, Clone)]
pub enum MirProjection {
    /// Field access
    Field(usize),

    /// Array/slice index
    Index(LocalId),

    /// Dereference
    Deref,

    /// Downcast to variant
    Downcast(usize),
}

/// Block terminator
#[derive(Debug, Clone)]
pub enum MirTerminator {
    /// Return from function
    Return(Option<MirOperand>),

    /// Unconditional branch
    Goto(BlockId),

    /// Conditional branch
    SwitchInt {
        discriminant: MirOperand,
        targets: Vec<(i64, BlockId)>,
        otherwise: BlockId,
    },

    /// Function call with continuation
    Call {
        func: MirOperand,
        args: Vec<MirOperand>,
        destination: Option<(MirPlace, BlockId)>,
        cleanup: Option<BlockId>,
    },

    /// Drop value and continue
    Drop {
        place: MirPlace,
        target: BlockId,
        unwind: Option<BlockId>,
    },

    /// Unreachable code
    Unreachable,
}

/// Binary operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirBinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

/// Unary operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirUnOp {
    Not,
    Neg,
}

/// Aggregate kind
#[derive(Debug, Clone)]
pub enum AggregateKind {
    Tuple,
    Struct(String, Vec<String>),
    Array(MirType, usize),
    /// Object/Map with key-value pairs (keys are field names)
    Object(Vec<String>),
}

/// Borrow kind
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorrowKind {
    /// Shared borrow (&T)
    Shared,

    /// Mutable borrow (&mut T)
    Mut,

    /// Unique borrow (for internal use)
    Unique,
}

/// MIR Constant
#[derive(Debug, Clone)]
pub enum MirConstant {
    Int(i64),
    UInt(u64),
    Float(f64),
    Bool(bool),
    String(String),
    Null,
}

/// Function parameter with ownership
#[derive(Debug, Clone)]
pub struct MirParam {
    pub name: String,
    pub ty: MirType,
    pub ownership: OwnershipKind,
}

/// Local variable
#[derive(Debug, Clone)]
pub struct MirLocal {
    pub name: Option<String>,
    pub ty: MirType,
    pub ownership: OwnershipKind,
}

/// Captured variable (for closures)
#[derive(Debug, Clone)]
pub struct MirCapture {
    pub name: String,
    pub ty: MirType,
    pub capture_kind: CaptureKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureKind {
    ByValue,
    ByRef,
    ByMutRef,
}

/// Global value/constant
#[derive(Debug, Clone)]
pub struct MirGlobal {
    pub name: String,
    pub ty: MirType,
    pub value: Option<MirConstant>,
    pub is_const: bool,
}

/// Type definition
#[derive(Debug, Clone)]
pub struct MirTypeDef {
    pub name: String,
    pub kind: MirTypeDefKind,
}

#[derive(Debug, Clone)]
pub enum MirTypeDefKind {
    Struct {
        fields: Vec<(String, MirType)>,
    },
    Enum {
        variants: Vec<(String, Vec<MirType>)>,
    },
    Alias(MirType),
}

impl MirModule {
    /// Create a new empty MIR module
    pub fn new(name: String) -> Self {
        Self {
            name,
            functions: Vec::new(),
            globals: Vec::new(),
            types: Vec::new(),
            ownership: OwnershipGraph::new(),
        }
    }

    /// Lower HIR module to MIR with full memory safety analysis
    pub fn from_hir(hir: &crate::parsing::hir::HirModule) -> Result<Self, String> {
        lower::lower_hir_to_mir(hir)
    }

    /// Validate MIR module
    pub fn validate(&self) -> Result<(), String> {
        validate::validate_mir(self)
    }
}

impl MirFunction {
    pub fn new(name: String) -> Self {
        Self {
            name,
            params: Vec::new(),
            return_type: MirType::Unit,
            body: Vec::new(),
            locals: Vec::new(),
            captures: Vec::new(),
            is_async: false,
            ownership: OwnershipGraph::new(),
        }
    }
}
