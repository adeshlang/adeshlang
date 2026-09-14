//! VIR (Value Intermediate Representation)
//!
//! VIR is a backend-neutral SSA-based IR consumed by all execution backends.
//!
//! Key properties:
//! - SSA form (Static Single Assignment)
//! - Backend-neutral (works for JIT, AOT, Bytecode, Interpreter, MLIR)
//! - Explicit memory operations (alloc, free, load, store)
//! - Explicit ARC operations (clone, drop, increment, decrement)
//! - Explicit drop calls
//! - No borrow checking (already done in MIR)
//! - No high-level constructs
//! - Supports structs, enums, monomorphized generics
//!
//! VIR is the output of MIR lowering and the input to all backends.

pub mod instructions;
pub mod lower;
pub mod pretty_print;
pub mod types;
pub mod validate;

use std::collections::HashMap;

pub use types::*;

/// VIR Module - a complete compilation unit
#[derive(Debug, Clone)]
pub struct VirModule {
    /// Module name
    pub name: String,

    /// Functions
    pub functions: Vec<VirFunction>,

    /// Global constants
    pub globals: Vec<VirGlobal>,

    /// Type definitions
    pub types: Vec<VirTypeDef>,

    /// String constants pool
    pub strings: HashMap<u32, String>,
}

/// VIR Function in SSA form
#[derive(Debug, Clone)]
pub struct VirFunction {
    /// Function name (mangled for generics)
    pub name: String,

    /// Parameters
    pub params: Vec<VirParam>,

    /// Return type
    pub return_type: VirType,

    /// Basic blocks (SSA form)
    pub blocks: Vec<VirBlock>,

    /// Local variables/values
    pub locals: Vec<VirLocal>,

    /// Is async?
    pub is_async: bool,
}

/// VIR Basic Block (SSA)
#[derive(Debug, Clone)]
pub struct VirBlock {
    /// Block ID
    pub id: BlockId,

    /// Block label (for debugging)
    pub label: Option<String>,

    /// Phi nodes (SSA)
    pub phis: Vec<VirPhi>,

    /// Instructions
    pub instructions: Vec<VirInstruction>,

    /// Terminator
    pub terminator: VirTerminator,
}

pub type BlockId = u32;
pub type ValueId = u32;
pub type LocalId = u32;
pub type StringId = u32;

/// SSA Phi node
#[derive(Debug, Clone)]
pub struct VirPhi {
    /// Destination value
    pub dest: ValueId,

    /// Type
    pub ty: VirType,

    /// Incoming values from predecessors
    pub incoming: Vec<(BlockId, ValueId)>,
}

/// VIR Instruction (SSA-based, explicit operations)
#[derive(Debug, Clone)]
pub enum VirInstruction {
    // === Constants ===
    /// Load integer constant
    ConstInt {
        dest: ValueId,
        value: i64,
        ty: VirType,
    },

    /// Load float constant
    ConstFloat {
        dest: ValueId,
        value: f64,
        ty: VirType,
    },

    /// Load boolean constant
    ConstBool { dest: ValueId, value: bool },

    /// Load string constant
    ConstString { dest: ValueId, string_id: StringId },

    /// Load null pointer
    ConstNull { dest: ValueId },

    // === Memory Operations (Explicit) ===
    /// Allocate memory on heap
    Alloc {
        dest: ValueId,
        ty: VirType,
        size: ValueId,
    },

    /// Free memory
    Free { ptr: ValueId },

    /// Load from memory
    Load {
        dest: ValueId,
        ptr: ValueId,
        ty: VirType,
    },

    /// Store to memory
    Store { ptr: ValueId, value: ValueId },

    /// Load from stack local
    LoadLocal { dest: ValueId, local: LocalId },

    /// Store to stack local
    StoreLocal { local: LocalId, value: ValueId },

    // === ARC Operations (Explicit) ===
    /// ARC: Increment reference count
    ArcIncrement { ptr: ValueId },

    /// ARC: Decrement reference count
    ArcDecrement { ptr: ValueId },

    /// ARC: Clone (increment + return)
    ArcClone { dest: ValueId, src: ValueId },

    /// ARC: Drop (decrement + conditional free)
    ArcDrop { ptr: ValueId },

    // === Drop Operations (Explicit) ===
    /// Call drop/destructor
    Drop { value: ValueId },

    // === Arithmetic ===
    /// Integer binary operation
    IntBinOp {
        dest: ValueId,
        op: IntBinOp,
        lhs: ValueId,
        rhs: ValueId,
        ty: VirType,
    },

    /// Float binary operation
    FloatBinOp {
        dest: ValueId,
        op: FloatBinOp,
        lhs: ValueId,
        rhs: ValueId,
        ty: VirType,
    },

    /// Integer unary operation
    IntUnOp {
        dest: ValueId,
        op: IntUnOp,
        operand: ValueId,
        ty: VirType,
    },

    /// Float unary operation
    FloatUnOp {
        dest: ValueId,
        op: FloatUnOp,
        operand: ValueId,
        ty: VirType,
    },

    // === Comparisons ===
    /// Integer comparison
    IntCmp {
        dest: ValueId,
        op: CmpOp,
        lhs: ValueId,
        rhs: ValueId,
    },

    /// Float comparison
    FloatCmp {
        dest: ValueId,
        op: CmpOp,
        lhs: ValueId,
        rhs: ValueId,
    },

    // === Type Operations ===
    /// Cast between types
    Cast {
        dest: ValueId,
        value: ValueId,
        from_ty: VirType,
        to_ty: VirType,
    },

    /// Bitcast (reinterpret)
    Bitcast {
        dest: ValueId,
        value: ValueId,
        to_ty: VirType,
    },

    // === Aggregate Operations ===
    /// Build struct
    BuildStruct {
        dest: ValueId,
        ty: String,
        field_names: Vec<String>,
        fields: Vec<ValueId>,
    },

    /// Extract struct field
    ExtractField {
        dest: ValueId,
        struct_val: ValueId,
        field: u32,
    },

    /// Insert into struct field
    InsertField {
        dest: ValueId,
        struct_val: ValueId,
        field: u32,
        value: ValueId,
    },

    /// Build array
    BuildArray {
        dest: ValueId,
        elem_ty: VirType,
        elements: Vec<ValueId>,
    },

    /// Array index
    ArrayIndex {
        dest: ValueId,
        array: ValueId,
        index: ValueId,
    },

    /// Build tuple
    BuildTuple {
        dest: ValueId,
        elements: Vec<ValueId>,
    },

    /// Extract tuple element
    ExtractTuple {
        dest: ValueId,
        tuple: ValueId,
        index: u32,
    },

    /// Build object/map with string keys
    BuildObject {
        dest: ValueId,
        keys: Vec<String>,
        values: Vec<ValueId>,
    },

    // === Enum Operations ===
    /// Build enum variant
    BuildEnum {
        dest: ValueId,
        ty: String,
        variant: u32,
        payload: Vec<ValueId>,
    },

    /// Get enum discriminant
    GetDiscriminant { dest: ValueId, enum_val: ValueId },

    /// Extract enum payload
    ExtractPayload {
        dest: ValueId,
        enum_val: ValueId,
        variant: u32,
    },

    // === Function Calls ===
    /// Call function
    Call {
        dest: Option<ValueId>,
        func: ValueId,
        args: Vec<ValueId>,
    },

    /// Call intrinsic
    Intrinsic {
        dest: Option<ValueId>,
        intrinsic: Intrinsic,
        args: Vec<ValueId>,
    },

    // === Misc ===
    /// Copy value
    Copy { dest: ValueId, src: ValueId },

    /// Move value (SSA, so just alias)
    Move { dest: ValueId, src: ValueId },

    /// No-op
    Nop,
}

/// VIR Terminator (control flow)
#[derive(Debug, Clone)]
pub enum VirTerminator {
    /// Return from function
    Return { value: Option<ValueId> },

    /// Unconditional branch
    Jump { target: BlockId },

    /// Conditional branch
    Branch {
        cond: ValueId,
        true_target: BlockId,
        false_target: BlockId,
    },

    /// Switch on integer
    Switch {
        value: ValueId,
        cases: Vec<(i64, BlockId)>,
        default: BlockId,
    },

    /// Unreachable
    Unreachable,
}

/// Integer binary operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntBinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    And,
    Or,
    Xor,
    Shl,
    Shr,
}

/// Float binary operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatBinOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// Integer unary operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntUnOp {
    Neg,
    Not,
}

/// Float unary operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatUnOp {
    Neg,
    Abs,
    Sqrt,
}

/// Comparison operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// Intrinsic operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intrinsic {
    /// Memory operations
    MemCopy,
    MemMove,
    MemSet,

    /// Atomic operations
    AtomicLoad,
    AtomicStore,
    AtomicCAS,
    AtomicAdd,

    /// Math
    Sin,
    Cos,
    Tan,
    Log,
    Exp,
    Pow,

    /// Bit operations
    CountOnes,
    CountZeros,
    LeadingZeros,
    TrailingZeros,

    /// Size/alignment
    SizeOf,
    AlignOf,
}

/// Function parameter
#[derive(Debug, Clone)]
pub struct VirParam {
    pub name: String,
    pub ty: VirType,
}

/// Local variable/value
#[derive(Debug, Clone)]
pub struct VirLocal {
    pub name: Option<String>,
    pub ty: VirType,
}

/// Global value
#[derive(Debug, Clone)]
pub struct VirGlobal {
    pub name: String,
    pub ty: VirType,
    pub value: VirConstant,
    pub is_const: bool,
}

/// Constant value
#[derive(Debug, Clone)]
pub enum VirConstant {
    Int(i64),
    UInt(u64),
    Float(f64),
    Bool(bool),
    String(String),
    Null,
}

/// Type definition
#[derive(Debug, Clone)]
pub struct VirTypeDef {
    pub name: String,
    pub kind: VirTypeDefKind,
}

#[derive(Debug, Clone)]
pub enum VirTypeDefKind {
    Struct {
        fields: Vec<(String, VirType)>,
    },
    Enum {
        variants: Vec<(String, Vec<VirType>)>,
    },
}

impl VirModule {
    pub fn new(name: String) -> Self {
        Self {
            name,
            functions: Vec::new(),
            globals: Vec::new(),
            types: Vec::new(),
            strings: HashMap::new(),
        }
    }

    /// Lower from MIR
    pub fn from_mir(mir: &crate::ir::mir::MirModule) -> Result<Self, String> {
        lower::lower_mir_to_vir(mir)
    }

    /// Validate VIR
    pub fn validate(&self) -> Result<(), String> {
        validate::validate_vir(self)
    }

    /// Add string constant
    pub fn add_string(&mut self, s: String) -> StringId {
        let id = self.strings.len() as StringId;
        self.strings.insert(id, s);
        id
    }
}

impl VirFunction {
    pub fn new(name: String, return_type: VirType) -> Self {
        Self {
            name,
            params: Vec::new(),
            return_type,
            blocks: Vec::new(),
            locals: Vec::new(),
            is_async: false,
        }
    }
}
