//! MIR (Mid-level Intermediate Representation) - SSA Form
//!
//! SSA-based IR for precise ownership and borrow analysis.
//! Every value is assigned exactly once, enabling exact dataflow.

use std::fmt;

/// Unique identifier for SSA values (assigned exactly once)
#[derive(Copy, Clone, Eq, PartialEq, Hash, Default)]
pub struct SsaVar(pub u32);

impl fmt::Debug for SsaVar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}", self.0)
    }
}

impl fmt::Display for SsaVar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}", self.0)
    }
}

/// Place identifier - unique memory location
#[derive(Copy, Clone, Eq, PartialEq, Hash, Default)]
pub struct PlaceId(pub u32);

impl fmt::Debug for PlaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "p{}", self.0)
    }
}

/// Block identifier
#[derive(Copy, Clone, Eq, PartialEq, Hash, Default, PartialOrd, Ord)]
pub struct BlockId(pub usize);

impl fmt::Debug for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "B{}", self.0)
    }
}

/// Type identifier (index into type table)
#[derive(Copy, Clone, Eq, PartialEq, Hash, Default, Debug)]
pub struct TypeId(pub u32);

/// Symbol for static/global names
#[derive(Clone, Eq, PartialEq, Hash)]
pub struct Symbol(pub String);

impl fmt::Debug for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Field index
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct FieldIdx(pub u32);

/// Borrow identifier for tracking borrow instances
#[derive(Copy, Clone, Eq, PartialEq, Hash, Default, Debug)]
pub struct BorrowId(pub u32);

/// Region identifier for region-based allocation
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct RegionId(pub u32);

/// Instruction identifier within a block
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct InstrId {
    pub block: BlockId,
    pub index: u32,
}

/// A place is a path to memory: base + projections
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub id: PlaceId,
    pub base: PlaceBase,
    pub projections: Vec<Projection>,
}

impl Place {
    pub fn local(var: SsaVar, id: PlaceId) -> Self {
        Self {
            id,
            base: PlaceBase::Local(var),
            projections: Vec::new(),
        }
    }

    pub fn with_field(mut self, field: FieldIdx) -> Self {
        self.projections.push(Projection::Field(field));
        self
    }

    pub fn with_deref(mut self) -> Self {
        self.projections.push(Projection::Deref);
        self
    }

    pub fn with_index(mut self, idx: SsaVar) -> Self {
        self.projections.push(Projection::Index(idx));
        self
    }
}

/// Base of a place expression
#[derive(Debug, Clone, PartialEq)]
pub enum PlaceBase {
    /// Stack local variable
    Local(SsaVar),
    /// Global/static variable
    Static(Symbol),
    /// Dereferencing a pointer
    Deref(SsaVar),
}

/// Projection from a place to sub-place
#[derive(Debug, Clone, PartialEq)]
pub enum Projection {
    /// Field access: .field
    Field(FieldIdx),
    /// Array/slice indexing: [idx]
    Index(SsaVar),
    /// Pointer dereference: *
    Deref,
}

/// Kind of borrow
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BorrowKind {
    /// Shared/immutable borrow (&T)
    Shared,
    /// Exclusive/mutable borrow (&mut T)
    Exclusive,
}

/// Allocation kind
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AllocKind {
    /// Stack allocation (automatic cleanup)
    Stack,
    /// Heap allocation (manual or GC cleanup)
    Heap,
    /// Region-based allocation (bulk cleanup)
    Region(RegionId),
}

/// Ownership kind for a place
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OwnershipKind {
    /// Fully owned value
    Owned,
    /// Shared borrow from another place
    SharedBorrow { from: PlaceId },
    /// Exclusive borrow from another place
    ExclusiveBorrow { from: PlaceId },
    /// Value has been moved away
    Moved,
}

/// Phi node: merges values from multiple predecessors at join points
#[derive(Debug, Clone)]
pub struct PhiNode {
    /// Destination SSA variable
    pub dest: SsaVar,
    /// Type of the value
    pub ty: TypeId,
    /// (predecessor block, value from that block) pairs
    pub sources: Vec<(BlockId, SsaVar)>,
}

impl PhiNode {
    pub fn new(dest: SsaVar, ty: TypeId) -> Self {
        Self {
            dest,
            ty,
            sources: Vec::new(),
        }
    }

    pub fn add_source(&mut self, block: BlockId, var: SsaVar) {
        self.sources.push((block, var));
    }
}

/// MIR instruction
#[derive(Debug, Clone)]
pub enum MirInstr {
    /// Assignment: dest = value
    Assign { dest: Place, value: RValue },

    /// Create a borrow: dest = &place or dest = &mut place
    Borrow {
        dest: SsaVar,
        place: PlaceId,
        kind: BorrowKind,
        borrow_id: BorrowId,
    },

    /// End a borrow (explicit lifetime end)
    EndBorrow { borrow_id: BorrowId },

    /// Move value: dest = move src
    Move { dest: Place, src: Place },

    /// Copy value: dest = copy src (for Copy types)
    Copy { dest: Place, src: Place },

    /// Drop a place (call destructor, release memory)
    Drop { place: PlaceId },

    /// Function call: dest = func(args)
    Call {
        dest: Option<Place>,
        func: Operand,
        args: Vec<Operand>,
    },

    /// Allocate memory
    Alloc {
        dest: SsaVar,
        ty: TypeId,
        kind: AllocKind,
    },

    /// Free memory (unsafe)
    Free { place: PlaceId },

    /// No-op (placeholder)
    Nop,
}

/// Right-hand side value in assignment
#[derive(Debug, Clone)]
pub enum RValue {
    /// Use an operand directly
    Use(Operand),

    /// Binary operation
    BinaryOp(BinOp, Operand, Operand),

    /// Unary operation
    UnaryOp(UnaryOp, Operand),

    /// Create aggregate (struct, tuple, array)
    Aggregate(AggregateKind, Vec<Operand>),

    /// Read from a place
    Read(Place),

    /// Take address of place
    Ref(PlaceId, BorrowKind),

    /// Create a constant
    Const(Constant),
}

/// Operand in an instruction
#[derive(Debug, Clone)]
pub enum Operand {
    /// Copy from place (value must be Copy)
    Copy(Place),
    /// Move from place
    Move(Place),
    /// Constant value
    Const(Constant),
}

/// Constant value
#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Unit,
    Null,
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
    BitNot,
}

/// Aggregate construction kind
#[derive(Debug, Clone)]
pub enum AggregateKind {
    Tuple,
    Array,
    Struct(Symbol),
}

/// Block terminator - how control leaves the block
#[derive(Debug, Clone)]
pub enum Terminator {
    /// Unconditional jump
    Goto { target: BlockId },

    /// Conditional branch
    Branch {
        cond: Operand,
        true_target: BlockId,
        false_target: BlockId,
    },

    /// Multi-way switch
    Switch {
        value: Operand,
        targets: Vec<(Constant, BlockId)>,
        default: BlockId,
    },

    /// Return from function
    Return { value: Option<Operand> },

    /// Panic/abort
    Panic { message: Option<Operand> },

    /// Unreachable (after diverging call)
    Unreachable,
}

impl Terminator {
    /// Get all successor block IDs
    pub fn successors(&self) -> Vec<BlockId> {
        match self {
            Terminator::Goto { target } => vec![*target],
            Terminator::Branch {
                true_target,
                false_target,
                ..
            } => {
                vec![*true_target, *false_target]
            }
            Terminator::Switch {
                targets, default, ..
            } => {
                let mut succs: Vec<_> = targets.iter().map(|(_, b)| *b).collect();
                succs.push(*default);
                succs
            }
            Terminator::Return { .. } | Terminator::Panic { .. } | Terminator::Unreachable => {
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssa_var_display() {
        let v = SsaVar(42);
        assert_eq!(format!("{}", v), "v42");
        assert_eq!(format!("{:?}", v), "v42");
    }

    #[test]
    fn test_place_construction() {
        let place = Place::local(SsaVar(0), PlaceId(0))
            .with_field(FieldIdx(1))
            .with_deref();

        assert_eq!(place.projections.len(), 2);
        assert!(matches!(
            place.projections[0],
            Projection::Field(FieldIdx(1))
        ));
        assert!(matches!(place.projections[1], Projection::Deref));
    }

    #[test]
    fn test_phi_node() {
        let mut phi = PhiNode::new(SsaVar(5), TypeId(0));
        phi.add_source(BlockId(1), SsaVar(2));
        phi.add_source(BlockId(2), SsaVar(3));

        assert_eq!(phi.dest, SsaVar(5));
        assert_eq!(phi.sources.len(), 2);
    }

    #[test]
    fn test_terminator_successors() {
        let goto = Terminator::Goto { target: BlockId(1) };
        assert_eq!(goto.successors(), vec![BlockId(1)]);

        let branch = Terminator::Branch {
            cond: Operand::Const(Constant::Bool(true)),
            true_target: BlockId(2),
            false_target: BlockId(3),
        };
        assert_eq!(branch.successors(), vec![BlockId(2), BlockId(3)]);

        let ret = Terminator::Return { value: None };
        assert!(ret.successors().is_empty());
    }
}
