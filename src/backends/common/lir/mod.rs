//! LIR (Low-Level Intermediate Representation)
//!
//! LIR is an SSA-based, block-structured IR used for JIT and AOT compilation.
//! It operates on typed values and provides explicit control flow.

use crate::utils::collections::FastMap;
use num_bigint::BigInt;

/// Unique identifier for LIR values
pub type ValueId = u32;

/// Unique identifier for LIR blocks
pub type BlockId = u32;

/// LIR Type representation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LirType {
    I8,
    I16,
    I32,
    I64,
    I128,
    U8,
    U16,
    U32,
    U64,
    U128,
    F32,
    F64,
    Bool,
    Ptr,
    Void,
}

/// LIR Instruction
#[derive(Debug, Clone)]
pub enum LirInst {
    /// Load integer constant
    ConstI64(ValueId, i64),
    /// Load float constant
    ConstF64(ValueId, f64),
    /// Load boolean constant
    ConstBool(ValueId, bool),
    /// Load string constant
    ConstString(ValueId, String),
    /// Load BigInt constant (arbitrary precision)
    ConstBigInt(ValueId, BigInt),
    /// Load null (pointer)
    ConstNull(ValueId),
    // Fixed-width unsigned integer constants
    ConstU8(ValueId, u8),
    ConstU16(ValueId, u16),
    ConstU32(ValueId, u32),
    ConstU64(ValueId, u64),
    ConstU128(ValueId, u128),
    // Fixed-width signed integer constants
    ConstI8(ValueId, i8),
    ConstI16(ValueId, i16),
    ConstI32(ValueId, i32),
    // Note: ConstI64 already exists above
    ConstI128(ValueId, i128),
    // Fixed-width float constants
    ConstF32(ValueId, f32),
    // Note: ConstF64 already exists above
    /// Load function reference (closure/lambda) with captured variable names and async flag
    ConstFunc(ValueId, String, Vec<String>, bool), // last bool is is_async
    /// Load a variable from stack/memory
    LoadVar(ValueId, String),
    /// Store a value to a variable
    StoreVar(String, ValueId),
    /// Load module namespace - creates an object with module exports
    LoadModule(ValueId, String), // dst, module_alias
    /// Copy value
    Copy(ValueId, ValueId),
    /// Integer addition
    AddI64(ValueId, ValueId, ValueId),
    /// Float addition
    AddF64(ValueId, ValueId, ValueId),
    /// Integer subtraction
    SubI64(ValueId, ValueId, ValueId),
    /// Float subtraction
    SubF64(ValueId, ValueId, ValueId),
    /// Integer multiplication
    MulI64(ValueId, ValueId, ValueId),
    /// Float multiplication
    MulF64(ValueId, ValueId, ValueId),
    /// Integer division
    DivI64(ValueId, ValueId, ValueId),
    /// Float division
    DivF64(ValueId, ValueId, ValueId),
    /// Integer modulo
    ModI64(ValueId, ValueId, ValueId),
    /// Integer negation
    NegI64(ValueId, ValueId),
    /// Float negation
    NegF64(ValueId, ValueId),
    /// Boolean negation
    Not(ValueId, ValueId),
    /// Integer comparison: less than
    CmpLtI64(ValueId, ValueId, ValueId),
    /// Integer comparison: less than or equal
    CmpLeI64(ValueId, ValueId, ValueId),
    /// Integer comparison: greater than
    CmpGtI64(ValueId, ValueId, ValueId),
    /// Integer comparison: greater than or equal
    CmpGeI64(ValueId, ValueId, ValueId),
    /// Integer comparison: equal
    CmpEqI64(ValueId, ValueId, ValueId),
    /// Integer comparison: not equal
    CmpNeI64(ValueId, ValueId, ValueId),
    /// Float comparison: less than
    CmpLtF64(ValueId, ValueId, ValueId),
    /// Float comparison: less than or equal
    CmpLeF64(ValueId, ValueId, ValueId),
    /// Float comparison: greater than
    CmpGtF64(ValueId, ValueId, ValueId),
    /// Float comparison: greater than or equal
    CmpGeF64(ValueId, ValueId, ValueId),
    /// Float comparison: equal
    CmpEqF64(ValueId, ValueId, ValueId),
    /// Float comparison: not equal
    CmpNeF64(ValueId, ValueId, ValueId),
    /// Boolean AND
    And(ValueId, ValueId, ValueId),
    /// Boolean OR
    Or(ValueId, ValueId, ValueId),
    /// Bitwise AND
    BitAnd(ValueId, ValueId, ValueId),
    /// Bitwise OR
    BitOr(ValueId, ValueId, ValueId),
    /// Bitwise XOR
    BitXor(ValueId, ValueId, ValueId),
    /// Bitwise shift left
    Shl(ValueId, ValueId, ValueId),
    /// Bitwise shift right
    Shr(ValueId, ValueId, ValueId),
    /// Convert i64 to f64
    I64ToF64(ValueId, ValueId),
    /// Convert f64 to i64
    F64ToI64(ValueId, ValueId),
    /// Call a function
    Call(ValueId, String, Vec<ValueId>),
    /// Call a builtin function
    CallBuiltin(ValueId, String, Vec<ValueId>),
    /// Call a builtin function with generic type parameter (for input<T>(), etc.)
    CallBuiltinGeneric(ValueId, String, Vec<ValueId>, String), // dst, name, args, generic_type
    /// Tail call - a call in tail position that should use TCO (no return value needed, args are passed)
    TailCall(String, Vec<ValueId>),
    /// Unconditional jump
    Jump(BlockId),
    /// Conditional jump (if condition is true, jump to first block, else second)
    JumpIf(ValueId, BlockId, BlockId),
    /// Return from function
    Return(Option<ValueId>),
    /// Phi node for SSA
    Phi(ValueId, Vec<(BlockId, ValueId)>),

    // Memory management operations
    /// Create ARC reference: ArcNew(dst, value_id) -> ArcId stored in dst
    ArcNew(ValueId, ValueId),
    /// Clone ARC reference: ArcClone(dst, arc_id)
    ArcClone(ValueId, ValueId),
    /// Drop ARC reference: ArcDrop(arc_id)
    ArcDrop(ValueId),
    /// Create weak reference: WeakNew(dst, arc_id)
    WeakNew(ValueId, ValueId),
    /// Drop weak reference: WeakDrop(arc_id)
    WeakDrop(ValueId),
    /// Get value from ARC: ArcGet(dst, arc_id)
    ArcGet(ValueId, ValueId),
    /// Set value in ARC: ArcSet(arc_id, value_id)
    ArcSet(ValueId, ValueId),
    /// Get strong reference count: ArcStrongCount(dst, arc_id)
    ArcStrongCount(ValueId, ValueId),
    /// Get weak reference count: ArcWeakCount(dst, arc_id)
    ArcWeakCount(ValueId, ValueId),

    /// Allocate untyped memory (elem_size defaults to 1 byte): Alloc(dst, size_value_id)
    Alloc(ValueId, ValueId),
    /// Allocate typed memory: AllocTyped(dst, size_value_id_bytes, elem_size_bytes)
    /// elem_size_bytes is the element width; size_value_id is total bytes requested.
    AllocTyped(ValueId, ValueId, i64),
    /// Free memory: Free(pointer_value_id)
    Free(ValueId),
    /// Load from pointer using element-based indexing: PtrLoad(dst, ptr_value_id, index_value_id)
    /// Index is a runtime value (integer) interpreted as element index; elem_size is taken from pointer state.
    PtrLoad(ValueId, ValueId, ValueId),
    /// Store to pointer using element-based indexing: PtrStore(ptr_value_id, value_value_id, index_value_id)
    /// Index is a runtime value (integer) interpreted as element index; elem_size is taken from pointer state.
    PtrStore(ValueId, ValueId, ValueId),
}

/// A basic block in LIR
#[derive(Debug, Clone)]
pub struct LirBlock {
    pub id: BlockId,
    pub label: String,
    pub instructions: Vec<LirInst>,
}

impl LirBlock {
    pub fn new(id: BlockId, label: String) -> Self {
        LirBlock {
            id,
            label,
            instructions: Vec::new(),
        }
    }

    pub fn push(&mut self, inst: LirInst) {
        self.instructions.push(inst);
    }
}

/// LIR Function
#[derive(Debug, Clone)]
pub struct LirFunction {
    pub name: String,
    pub params: Vec<(String, LirType)>,
    pub ret_type: LirType,
    pub blocks: Vec<LirBlock>,
    pub entry_block: BlockId,
    next_value_id: ValueId,
    next_block_id: BlockId,
    pub var_map: FastMap<String, ValueId>,
    pub is_async: bool,
    pub is_exported: bool,
}

impl LirFunction {
    pub fn new(name: String, params: Vec<(String, LirType)>, ret_type: LirType) -> Self {
        let mut func = LirFunction {
            name,
            params,
            ret_type,
            blocks: Vec::new(),
            entry_block: 0,
            next_value_id: 0,
            next_block_id: 0,
            var_map: FastMap::default(),
            is_async: false,
            is_exported: false,
        };
        // Create entry block
        func.entry_block = func.create_block("entry".to_string());
        func
    }

    /// Create a new async function
    pub fn new_async(name: String, params: Vec<(String, LirType)>, ret_type: LirType) -> Self {
        let mut func = Self::new(name, params, ret_type);
        func.is_async = true;
        func
    }

    /// Create a new exported function
    pub fn new_exported(name: String, params: Vec<(String, LirType)>, ret_type: LirType) -> Self {
        let mut func = Self::new(name, params, ret_type);
        func.is_exported = true;
        func
    }

    /// Allocate a new value ID
    pub fn alloc_value(&mut self) -> ValueId {
        let id = self.next_value_id;
        self.next_value_id += 1;
        id
    }

    /// Create a new basic block
    pub fn create_block(&mut self, label: String) -> BlockId {
        let id = self.next_block_id;
        self.next_block_id += 1;
        self.blocks.push(LirBlock::new(id, label));
        id
    }

    /// Get a mutable reference to a block
    pub fn get_block_mut(&mut self, id: BlockId) -> Option<&mut LirBlock> {
        self.blocks.iter_mut().find(|b| b.id == id)
    }

    /// Get a reference to a block
    pub fn get_block(&self, id: BlockId) -> Option<&LirBlock> {
        self.blocks.iter().find(|b| b.id == id)
    }

    /// Push an instruction to a block
    pub fn push_to_block(&mut self, block_id: BlockId, inst: LirInst) {
        if let Some(block) = self.get_block_mut(block_id) {
            block.push(inst);
        }
    }

    /// Map a variable name to a value ID
    pub fn set_var(&mut self, name: String, value: ValueId) {
        self.var_map.insert(name, value);
    }

    /// Get the value ID for a variable
    pub fn get_var(&self, name: &str) -> Option<ValueId> {
        self.var_map.get(name).copied()
    }
}

/// Module exports - what a module makes available to importers
#[derive(Debug, Clone)]
pub struct ModuleExports {
    /// Exported functions (name -> function index in module)
    pub functions: FastMap<String, usize>,
    /// Exported classes (name -> class info)
    pub classes: FastMap<String, String>,
    /// Default export if any
    pub default_export: Option<String>,
}

impl ModuleExports {
    pub fn new() -> Self {
        ModuleExports {
            functions: FastMap::default(),
            classes: FastMap::default(),
            default_export: None,
        }
    }
}

/// LIR Module - the top-level compilation unit
#[derive(Debug, Clone)]
pub struct LirModule {
    pub functions: Vec<LirFunction>,
    pub globals: FastMap<String, LirType>,
    pub string_pool: Vec<String>,
    /// Module exports for import resolution
    pub exports: ModuleExports,
    /// Imported modules (alias -> module path)
    pub imports: FastMap<String, String>,
}

impl LirModule {
    pub fn new() -> Self {
        LirModule {
            functions: Vec::new(),
            globals: FastMap::default(),
            string_pool: Vec::new(),
            exports: ModuleExports::new(),
            imports: FastMap::default(),
        }
    }

    /// Add a string to the string pool and return its index
    pub fn add_string(&mut self, s: String) -> usize {
        if let Some(idx) = self.string_pool.iter().position(|x| x == &s) {
            idx
        } else {
            let idx = self.string_pool.len();
            self.string_pool.push(s);
            idx
        }
    }

    /// Add a function to the module
    pub fn add_function(&mut self, func: LirFunction) {
        self.functions.push(func);
    }
}

impl Default for LirModule {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lir_module_creation() {
        let module = LirModule::new();
        assert!(module.functions.is_empty());
        assert!(module.globals.is_empty());
    }

    #[test]
    fn test_lir_function_creation() {
        let func = LirFunction::new(
            "add".to_string(),
            vec![
                ("a".to_string(), LirType::I64),
                ("b".to_string(), LirType::I64),
            ],
            LirType::I64,
        );
        assert_eq!(func.name, "add");
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.blocks.len(), 1); // Entry block
    }

    #[test]
    fn test_value_allocation() {
        let mut func = LirFunction::new("test".to_string(), vec![], LirType::Void);
        let v1 = func.alloc_value();
        let v2 = func.alloc_value();
        let v3 = func.alloc_value();
        assert_eq!(v1, 0);
        assert_eq!(v2, 1);
        assert_eq!(v3, 2);
    }

    #[test]
    fn test_block_creation() {
        let mut func = LirFunction::new("test".to_string(), vec![], LirType::Void);
        let entry = func.entry_block;
        let loop_block = func.create_block("loop".to_string());
        let exit_block = func.create_block("exit".to_string());

        assert_eq!(entry, 0);
        assert_eq!(loop_block, 1);
        assert_eq!(exit_block, 2);
        assert_eq!(func.blocks.len(), 3);
    }

    #[test]
    fn test_string_pool() {
        let mut module = LirModule::new();
        let idx1 = module.add_string("hello".to_string());
        let idx2 = module.add_string("world".to_string());
        let idx3 = module.add_string("hello".to_string()); // Duplicate

        assert_eq!(idx1, 0);
        assert_eq!(idx2, 1);
        assert_eq!(idx3, 0); // Should return existing index
    }
}

// Submodule for lowering HIR to LIR
pub mod lower;

pub use lower::*;
